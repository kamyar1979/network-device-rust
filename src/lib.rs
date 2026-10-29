#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! A dependency-free, strongly typed eDSL and base contracts for configuring
//! network devices of any vendor, imperatively through drivers or
//! declaratively through infrastructure-as-code (IaC) backends.
//!
//! Operations state **what** must hold on a device; each driver decides
//! **how** its device realizes it. Mutating operations converge: running one
//! again when the device already matches succeeds without change, so callers
//! can retry them. Every driver error is classified by [`ErrorKind`].
//!
//! # Typed execution
//!
//! A driver implements [`Driver`] and [`Execute`] for each operation it
//! supports; callers compose operations with `then` and [`Run`] them.
//!
//! ```
//! use network_device::{
//!     AdministrativeState, Change, Driver, Execute, InterfaceName, Run,
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
//!     type Output = Change;
//!
//!     fn execute<'a>(
//!         &'a self,
//!         _operation: SetInterfaceState,
//!     ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
//!         async { Ok(Change::Changed { writes: 1 }) }
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
//! # Run-time dispatch
//!
//! A driver implementing the whole neutral operation set ([`NeutralDriver`])
//! is also a [`DynDriver`], so a caller can choose the driver at run time, for
//! example per device from inventory, and run a [`DeviceOperation`] list:
//!
//! ```
//! # use network_device::{DeviceOperation, DynDriver, ErrorKind, run};
//! # async fn example(driver: Box<dyn DynDriver>, operations: Vec<DeviceOperation>) {
//! match run(driver.as_ref(), operations).await {
//!     Ok(outputs) => println!("{} operations applied", outputs.len()),
//!     Err(error) if error.kind() == ErrorKind::Transient => println!("retry: {error}"),
//!     Err(error) => println!("failed: {error}"),
//! }
//! # }
//! ```
//!
//! # One program, two interpreters
//!
//! The mutating operations converge, so they also describe desired state. The
//! same program a vendor driver applies to a device can be recorded by
//! [`DesiredStateRecorder`] and rendered as a neutral IaC document:
//!
//! ```
//! use network_device::{
//!     AdministrativeState, DesiredStateRecorder, InterfaceName, Run, SetInterfaceState,
//!     operation,
//! };
//!
//! # async fn example() -> Result<(), network_device::NotDescribed> {
//! let port = InterfaceName::try_from("Ethernet1/1").expect("valid interface name");
//! let program = operation(SetInterfaceState::new(port, AdministrativeState::Disabled));
//!
//! let recorder = DesiredStateRecorder::new("leaf-1");
//! program.run(&recorder).await?;
//! let document = recorder.into_state().to_json();
//! assert!(document.contains("\"administrative_state\": \"disabled\""));
//! # Ok(())
//! # }
//! ```
//!
//! # IaC backend
//!
//! The declarative model describes desired resources for an [`IacBackend`],
//! which plans and applies them, for example by rendering IaC files. It is
//! intentionally separate from the imperative operations.
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
//! remains the responsibility of the integrating application, which resolves
//! credentials from its own store and passes them to
//! [`DriverConnector::connect`]. This crate has no dependency on a credential
//! store.

mod connector;
mod desired;
mod dynamic;
mod error;
mod iac;
mod interface;
mod operation;
mod qos;
mod routing;

pub use connector::DriverConnector;
pub use desired::{
    DESIRED_STATE_SCHEMA, DesiredState, DesiredStateRecorder, InterfaceState, LimitState,
    NotDescribed, PolicyState, Presence,
};
pub use dynamic::{
    DeviceOperation, DeviceOutput, DynDriver, DynFuture, NeutralDriver, ProgramError, run,
};
pub use error::{DeviceError, ErrorKind, Unsupported};
pub use iac::{IacBackend, InterfaceConfiguration, StaticRouteConfiguration};
pub use interface::{
    InterfaceName, InterfaceNameError, IpPrefix, IpPrefixError, IpPrefixParseError,
};
pub use operation::{
    AdministrativeState, AttachBandwidthPolicy, DetachBandwidthPolicy, Driver,
    EnsureBandwidthPolicy, EnsureStaticRoute, Execute, GetBandwidthPolicy, GetInterfacePolicies,
    GetInterfaceState, OperationStep, RemoveBandwidthPolicy, RemoveInterfaceBandwidthLimit,
    RemoveStaticRoute, Run, Sequence, SetInterfaceBandwidthLimit, SetInterfaceDescription,
    SetInterfaceState, operation,
};
pub use qos::{
    BandwidthLimit, BandwidthLimits, BandwidthLimitsError, BandwidthPolicy, BitsPerSecond,
    BurstBytes, Change, InterfacePolicies, MAX_POLICY_NAME_LENGTH, PolicyAttachment, PolicyName,
    PolicyNameError, PolicyScope,
};
pub use routing::{StaticRoute, StaticRouteError};

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU64;
    use std::pin::pin;
    use std::sync::Mutex;
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
        Partial { kind: ErrorKind, writes: usize },
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("test operation failed")
        }
    }

    impl std::error::Error for TestError {}

    impl DeviceError for TestError {
        fn kind(&self) -> ErrorKind {
            match self {
                Self::Failed => ErrorKind::Rejected,
                Self::Partial { kind, .. } => *kind,
            }
        }

        fn completed_writes(&self) -> usize {
            match self {
                Self::Failed => 0,
                Self::Partial { writes, .. } => *writes,
            }
        }
    }

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

    /// Implements the neutral set: each write reports two device writes, reads
    /// return fixed values, and `fail_on` makes one operation fail.
    struct NeutralTestDriver {
        log: Mutex<Vec<&'static str>>,
        fail_on: Option<(&'static str, TestError)>,
    }

    impl NeutralTestDriver {
        fn new(fail_on: Option<(&'static str, TestError)>) -> Self {
            Self {
                log: Mutex::new(Vec::new()),
                fail_on,
            }
        }

        fn record(&self, name: &'static str) -> Result<(), TestError> {
            self.log.lock().unwrap().push(name);
            match self.fail_on {
                Some((failing, error)) if failing == name => Err(error),
                _ => Ok(()),
            }
        }
    }

    impl Driver for NeutralTestDriver {
        type Error = TestError;
    }

    macro_rules! neutral_write {
        ($operation:ty, $name:literal) => {
            impl Execute<$operation> for NeutralTestDriver {
                type Output = Change;

                #[allow(clippy::manual_async_fn)]
                fn execute<'a>(
                    &'a self,
                    _operation: $operation,
                ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
                    async move { self.record($name).map(|()| Change::Changed { writes: 2 }) }
                }
            }
        };
    }

    neutral_write!(SetInterfaceState, "set_interface_state");
    neutral_write!(SetInterfaceDescription, "set_interface_description");
    neutral_write!(EnsureStaticRoute, "ensure_static_route");
    neutral_write!(RemoveStaticRoute, "remove_static_route");
    neutral_write!(EnsureBandwidthPolicy, "ensure_bandwidth_policy");
    neutral_write!(RemoveBandwidthPolicy, "remove_bandwidth_policy");
    neutral_write!(AttachBandwidthPolicy, "attach_bandwidth_policy");
    neutral_write!(DetachBandwidthPolicy, "detach_bandwidth_policy");
    neutral_write!(SetInterfaceBandwidthLimit, "set_interface_bandwidth_limit");
    neutral_write!(
        RemoveInterfaceBandwidthLimit,
        "remove_interface_bandwidth_limit"
    );

    impl Execute<GetInterfaceState> for NeutralTestDriver {
        type Output = AdministrativeState;

        #[allow(clippy::manual_async_fn)]
        fn execute<'a>(
            &'a self,
            _operation: GetInterfaceState,
        ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
            async move {
                self.record("get_interface_state")
                    .map(|()| AdministrativeState::Disabled)
            }
        }
    }

    impl Execute<GetBandwidthPolicy> for NeutralTestDriver {
        type Output = Option<BandwidthPolicy>;

        #[allow(clippy::manual_async_fn)]
        fn execute<'a>(
            &'a self,
            operation: GetBandwidthPolicy,
        ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
            async move {
                self.record("get_bandwidth_policy")
                    .map(|()| Some(policy(operation.name.as_str())))
            }
        }
    }

    impl Execute<GetInterfacePolicies> for NeutralTestDriver {
        type Output = InterfacePolicies;

        #[allow(clippy::manual_async_fn)]
        fn execute<'a>(
            &'a self,
            _operation: GetInterfacePolicies,
        ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
            async move {
                self.record("get_interface_policies")
                    .map(|()| InterfacePolicies {
                        attachments: vec![PolicyAttachment {
                            name: PolicyName::new("throttle").unwrap(),
                            scope: PolicyScope::Interface,
                        }],
                        bandwidth_limit: None,
                    })
            }
        }
    }

    fn policy(name: &str) -> BandwidthPolicy {
        BandwidthPolicy::new(
            PolicyName::new(name).unwrap(),
            BandwidthLimits::new(
                Some(BandwidthLimit::new(BitsPerSecond::new(
                    NonZeroU64::new(10_000_000).unwrap(),
                ))),
                None,
            )
            .unwrap(),
        )
    }

    fn port() -> InterfaceName {
        InterfaceName::new("Ethernet1/1").unwrap()
    }

    fn program() -> Vec<DeviceOperation> {
        vec![
            DeviceOperation::EnsureBandwidthPolicy(EnsureBandwidthPolicy::new(policy("throttle"))),
            DeviceOperation::AttachBandwidthPolicy(AttachBandwidthPolicy::new(
                PolicyName::new("throttle").unwrap(),
                port(),
                PolicyScope::Interface,
            )),
            DeviceOperation::AttachBandwidthPolicy(AttachBandwidthPolicy::new(
                PolicyName::new("throttle").unwrap(),
                port(),
                PolicyScope::Host(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 20))),
            )),
        ]
    }

    #[test]
    fn a_neutral_driver_runs_a_runtime_program_through_a_trait_object() {
        let driver: Box<dyn DynDriver> = Box::new(NeutralTestDriver::new(None));
        let outputs = block_on(run(driver.as_ref(), program())).unwrap();

        assert_eq!(outputs.len(), 3);
        assert_eq!(outputs.iter().map(DeviceOutput::writes).sum::<usize>(), 6);
    }

    #[test]
    fn a_program_stops_at_the_first_error_and_reports_its_progress() {
        let partial = TestError::Partial {
            kind: ErrorKind::UnknownOutcome,
            writes: 1,
        };
        let driver = NeutralTestDriver::new(Some(("attach_bandwidth_policy", partial)));
        let error = block_on(run(&driver, program())).unwrap_err();

        assert_eq!(error.completed_operations, 1);
        assert_eq!(error.completed_writes, 3);
        assert_eq!(error.operation, "attach_bandwidth_policy");
        assert_eq!(error.kind(), ErrorKind::UnknownOutcome);
        assert!(std::error::Error::source(&error).is_some());
        assert_eq!(
            *driver.log.lock().unwrap(),
            ["ensure_bandwidth_policy", "attach_bandwidth_policy"]
        );
    }

    #[test]
    fn reads_return_normalized_values() {
        let driver = NeutralTestDriver::new(None);
        let outputs = block_on(run(
            &driver,
            vec![
                DeviceOperation::GetInterfaceState(GetInterfaceState::new(port())),
                DeviceOperation::GetBandwidthPolicy(GetBandwidthPolicy::new(
                    PolicyName::new("throttle").unwrap(),
                )),
                DeviceOperation::GetInterfacePolicies(GetInterfacePolicies::new(port())),
            ],
        ))
        .unwrap();

        assert_eq!(
            outputs[0],
            DeviceOutput::AdministrativeState(AdministrativeState::Disabled)
        );
        assert_eq!(
            outputs[1],
            DeviceOutput::BandwidthPolicy(Some(policy("throttle")))
        );
        let DeviceOutput::InterfacePolicies(observed) = &outputs[2] else {
            panic!("expected interface policies");
        };
        assert_eq!(observed.attachments[0].scope, PolicyScope::Interface);
        assert!(outputs.iter().all(|output| output.writes() == 0));
    }

    struct TestIacBackend;

    impl IacBackend<StaticRouteConfiguration> for TestIacBackend {
        type Error = std::convert::Infallible;
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

    #[test]
    fn operation_names_and_read_flags_are_stable() {
        let read = DeviceOperation::GetInterfaceState(GetInterfaceState::new(port()));
        assert_eq!(read.name(), "get_interface_state");
        assert!(read.is_read());
        assert!(!program()[0].is_read());
        assert_eq!(program()[2].name(), "attach_bandwidth_policy");
    }
}
