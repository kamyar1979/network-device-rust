#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! A dependency-free, strongly typed eDSL and base contracts for network
//! device operations and declarative infrastructure resources.
//!
//! The imperative API runs operations directly through [`Driver`] and
//! [`Execute`]. The declarative API describes desired resources for an
//! [`IacBackend`]. These models are intentionally separate.
//!
//! # Direct execution
//!
//! ```
//! use network_device::{
//!     AdministrativeState, Driver, Execute, InterfaceName, Run,
//!     SetInterfaceState, operation,
//! };
//! use std::convert::Infallible;
//! use std::future::Future;
//!
//! struct Device;
//!
//! impl Driver for Device {
//!     type Error = Infallible;
//! }
//!
//! impl Execute<SetInterfaceState> for Device {
//!     type Output = ();
//!
//!     fn execute<'a>(
//!         &'a self,
//!         _operation: SetInterfaceState,
//!     ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
//!         async { Ok(()) }
//!     }
//! }
//!
//! # async fn example() -> Result<(), Infallible> {
//! let name = InterfaceName::try_from("Ethernet1/1").expect("valid interface name");
//! operation(SetInterfaceState::new(name, AdministrativeState::Enabled))
//!     .run(&Device)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! # IaC backend
//!
//! ```
//! use network_device::{
//!     AdministrativeState, IacBackend, InterfaceConfiguration, InterfaceName,
//! };
//! use std::convert::Infallible;
//! use std::future::Future;
//!
//! struct LocalBackend;
//!
//! impl IacBackend<InterfaceConfiguration> for LocalBackend {
//!     type Error = Infallible;
//!     type Plan = usize;
//!     type ApplyResult = ();
//!
//!     fn plan<'a>(
//!         &'a self,
//!         desired: &'a InterfaceConfiguration,
//!     ) -> impl Future<Output = Result<Self::Plan, Self::Error>> + Send + 'a {
//!         async move { Ok(usize::from(desired.administrative_state.is_some())) }
//!     }
//!
//!     fn apply<'a>(
//!         &'a self,
//!         _desired: &'a InterfaceConfiguration,
//!     ) -> impl Future<Output = Result<Self::ApplyResult, Self::Error>> + Send + 'a {
//!         async { Ok(()) }
//!     }
//! }
//!
//! # async fn example() -> Result<(), Infallible> {
//! let desired = InterfaceConfiguration::new(
//!     InterfaceName::try_from("Ethernet1/1").expect("valid interface name"),
//! )
//! .administrative_state(AdministrativeState::Enabled);
//! let backend = LocalBackend;
//! let _plan = backend.plan(&desired).await?;
//! backend.apply(&desired).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Driver connection and credentials
//!
//! [`DriverConnector`] constructs and authenticates a concrete provider driver
//! from a provider-defined target and authentication value. Credential lookup
//! remains the responsibility of the integrating application. For example, an
//! integration can resolve an `oss_providers::CredentialMaterial` using its
//! `CredentialProvider`, convert that value into the selected connector's
//! `Authentication` type, and pass it to [`DriverConnector::connect`]. This
//! crate has no dependency on `oss-providers`, Vault, or a credential store.

mod connector;
mod iac;
mod interface;
mod operation;
mod routing;

pub use connector::DriverConnector;
pub use iac::{IacBackend, InterfaceConfiguration, StaticRouteConfiguration};
pub use interface::{
    InterfaceName, InterfaceNameError, IpPrefix, IpPrefixError, IpPrefixParseError,
};
pub use operation::{
    AdministrativeState, Driver, EnsureStaticRoute, Execute, OperationStep, RemoveStaticRoute, Run,
    Sequence, SetInterfaceDescription, SetInterfaceState, operation,
};
pub use routing::{StaticRoute, StaticRouteError};

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::future::Future;
    use std::net::{IpAddr, Ipv4Addr};
    use std::pin::pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestError {
        Failed,
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("test operation failed")
        }
    }

    impl std::error::Error for TestError {}

    struct TestDriver {
        events: Sender<&'static str>,
        fail_first: AtomicBool,
    }

    impl Driver for TestDriver {
        type Error = TestError;
    }

    struct FirstOperation;
    struct SecondOperation;

    impl Execute<FirstOperation> for TestDriver {
        type Output = u8;

        #[allow(clippy::manual_async_fn)]
        fn execute<'a>(
            &'a self,
            _operation: FirstOperation,
        ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
            async move {
                self.events.send("first").expect("receiver is alive");
                if self.fail_first.load(Ordering::Relaxed) {
                    Err(TestError::Failed)
                } else {
                    Ok(7)
                }
            }
        }
    }

    impl Execute<SecondOperation> for TestDriver {
        type Output = String;

        #[allow(clippy::manual_async_fn)]
        fn execute<'a>(
            &'a self,
            _operation: SecondOperation,
        ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
            async move {
                self.events.send("second").expect("receiver is alive");
                Ok(String::from("completed"))
            }
        }
    }

    fn test_driver(fail_first: bool) -> (TestDriver, Receiver<&'static str>) {
        let (events, receiver) = mpsc::channel();
        (
            TestDriver {
                events,
                fail_first: AtomicBool::new(fail_first),
            },
            receiver,
        )
    }

    #[test]
    fn operation_sequence_runs_in_order_and_returns_second_output() {
        let (driver, events) = test_driver(false);
        let output = block_on(
            operation(FirstOperation)
                .then(operation(SecondOperation))
                .run(&driver),
        )
        .unwrap();

        assert_eq!(output, "completed");
        assert_eq!(events.try_iter().collect::<Vec<_>>(), ["first", "second"]);
    }

    #[test]
    fn operation_sequence_stops_after_first_error() {
        let (driver, events) = test_driver(true);
        let result = block_on(
            operation(FirstOperation)
                .then(operation(SecondOperation))
                .run(&driver),
        );

        assert_eq!(result, Err(TestError::Failed));
        assert_eq!(events.try_iter().collect::<Vec<_>>(), ["first"]);
    }

    struct CustomOperation(&'static str);

    impl Execute<CustomOperation> for TestDriver {
        type Output = usize;

        #[allow(clippy::manual_async_fn)]
        fn execute<'a>(
            &'a self,
            operation: CustomOperation,
        ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
            async move {
                self.events.send(operation.0).expect("receiver is alive");
                Ok(operation.0.len())
            }
        }
    }

    #[test]
    fn downstream_custom_operation_is_supported_without_core_changes() {
        let (driver, events) = test_driver(false);
        let result = block_on(operation(CustomOperation("extension")).run(&driver)).unwrap();

        assert_eq!(result, "extension".len());
        assert_eq!(events.try_iter().collect::<Vec<_>>(), ["extension"]);
    }

    struct TestIacBackend;

    impl IacBackend<StaticRouteConfiguration> for TestIacBackend {
        type Error = Infallible;
        type Plan = bool;
        type ApplyResult = usize;

        #[allow(clippy::manual_async_fn)]
        fn plan<'a>(
            &'a self,
            desired: &'a StaticRouteConfiguration,
        ) -> impl Future<Output = Result<Self::Plan, Self::Error>> + Send + 'a {
            async move { Ok(desired.route.destination().prefix_length() == 24) }
        }

        #[allow(clippy::manual_async_fn)]
        fn apply<'a>(
            &'a self,
            desired: &'a StaticRouteConfiguration,
        ) -> impl Future<Output = Result<Self::ApplyResult, Self::Error>> + Send + 'a {
            async move { Ok(usize::from(desired.route.destination().prefix_length())) }
        }
    }

    #[test]
    fn iac_backend_can_plan_and_apply_a_standard_resource() {
        let destination = "192.0.2.0/24".parse::<IpPrefix>().unwrap();
        let route = StaticRoute::new(destination, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))).unwrap();
        let desired = StaticRouteConfiguration::new(route);
        let backend = TestIacBackend;

        assert!(block_on(backend.plan(&desired)).unwrap());
        assert_eq!(block_on(backend.apply(&desired)).unwrap(), 24);
    }
}
