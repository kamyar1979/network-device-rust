//! Static construction and authentication boundary for device drivers.

use crate::Driver;
use std::error::Error;
use std::future::Future;

/// Constructs and authenticates a provider-specific device driver.
///
/// A connector owns the protocol-specific connection process. The caller
/// supplies authentication material it has already resolved; the connector
/// never fetches credentials from a vault or another credential store.
///
/// Each provider chooses its own target, authentication, driver, and
/// connection error types. The returned driver remains statically typed.
///
/// # Example
///
/// ```
/// use network_device::{Driver, DriverConnector};
/// use std::convert::Infallible;
/// use std::future::Future;
///
/// struct Target { address: String }
/// struct Authentication { username: String, password: String }
/// struct DeviceDriver;
/// struct Connector;
///
/// impl Driver for DeviceDriver {
///     type Error = Infallible;
/// }
///
/// impl DriverConnector for Connector {
///     type Driver = DeviceDriver;
///     type Target = Target;
///     type Authentication = Authentication;
///     type Error = Infallible;
///
///     fn connect<'a>(
///         &'a self,
///         target: &'a Self::Target,
///         authentication: Self::Authentication,
///     ) -> impl Future<Output = Result<Self::Driver, Self::Error>> + Send + 'a {
///         async move {
///             let _endpoint = &target.address;
///             let _username = authentication.username;
///             let _password = authentication.password;
///             Ok(DeviceDriver)
///         }
///     }
/// }
/// ```
pub trait DriverConnector: Send + Sync {
    /// Concrete driver returned after a successful connection.
    type Driver: Driver;
    /// Provider-defined information needed to locate the device.
    type Target: Sync;
    /// Provider-defined, already-resolved authentication material.
    type Authentication: Send + 'static;
    /// Error returned while connecting or authenticating.
    type Error: Error + Send + Sync + 'static;

    /// Connects to the target and authenticates using the supplied material.
    ///
    /// Authentication is passed by value so implementations can consume
    /// secrets without requiring a clone or shared ownership wrapper.
    fn connect<'a>(
        &'a self,
        target: &'a Self::Target,
        authentication: Self::Authentication,
    ) -> impl Future<Output = Result<Self::Driver, Self::Error>> + Send + 'a;
}

#[cfg(test)]
mod tests {
    use super::DriverConnector;
    use crate::{
        AdministrativeState, Driver, Execute, InterfaceName, Run, SetInterfaceState, operation,
    };
    use std::convert::Infallible;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("test future unexpectedly yielded"),
        }
    }

    struct DeviceTarget(&'static str);

    struct DeviceAuthentication {
        username: String,
        password: String,
    }

    struct DeviceDriver {
        endpoint: String,
        authenticated_user: String,
    }

    impl Driver for DeviceDriver {
        type Error = Infallible;
    }

    struct TestConnector;

    impl DriverConnector for TestConnector {
        type Driver = DeviceDriver;
        type Target = DeviceTarget;
        type Authentication = DeviceAuthentication;
        type Error = Infallible;

        #[allow(clippy::manual_async_fn)]
        fn connect<'a>(
            &'a self,
            target: &'a Self::Target,
            authentication: Self::Authentication,
        ) -> impl Future<Output = Result<Self::Driver, Self::Error>> + Send + 'a {
            async move {
                let DeviceAuthentication { username, password } = authentication;
                assert_eq!(password, "consumed-secret");
                Ok(DeviceDriver {
                    endpoint: target.0.to_owned(),
                    authenticated_user: username,
                })
            }
        }
    }

    impl Execute<SetInterfaceState> for DeviceDriver {
        type Output = String;

        #[allow(clippy::manual_async_fn)]
        fn execute<'a>(
            &'a self,
            operation: SetInterfaceState,
        ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
            async move {
                let state = match operation.state {
                    AdministrativeState::Enabled => "enabled",
                    AdministrativeState::Disabled => "disabled",
                };
                Ok(format!(
                    "{}:{}:{}:{}",
                    self.endpoint, self.authenticated_user, operation.interface, state
                ))
            }
        }
    }

    #[test]
    fn connector_consumes_authentication_and_created_driver_runs_operation() {
        let connector = TestConnector;
        let target = DeviceTarget("device.example");
        let authentication = DeviceAuthentication {
            username: String::from("operator"),
            password: String::from("consumed-secret"),
        };
        let driver = block_on(connector.connect(&target, authentication)).unwrap();
        let interface = InterfaceName::try_from("Ethernet1/1").unwrap();

        let result = block_on(
            operation(SetInterfaceState::new(
                interface,
                AdministrativeState::Enabled,
            ))
            .run(&driver),
        )
        .unwrap();

        assert_eq!(result, "device.example:operator:Ethernet1/1:enabled");
    }
}
