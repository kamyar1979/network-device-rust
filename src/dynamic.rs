//! Run-time dispatch over the neutral operation set.
//!
//! The typed eDSL fixes the driver at compile time. A caller that chooses the
//! driver at run time (for example per device, from inventory) holds a
//! `Box<dyn DynDriver>` instead and runs a [`DeviceOperation`] list with
//! [`run`]. Every driver that implements [`Execute`] for the whole neutral set
//! is a [`DynDriver`] through a blanket implementation.

use crate::{
    AdministrativeState, AttachBandwidthPolicy, BandwidthPolicy, Change, DetachBandwidthPolicy,
    DeviceError, Driver, EnsureBandwidthPolicy, EnsureStaticRoute, ErrorKind, Execute,
    GetBandwidthPolicy, GetInterfacePolicies, GetInterfaceState, InterfacePolicies,
    RemoveBandwidthPolicy, RemoveInterfaceBandwidthLimit, RemoveStaticRoute,
    SetInterfaceBandwidthLimit, SetInterfaceDescription, SetInterfaceState,
};
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// One operation of the neutral set, chosen at run time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceOperation {
    /// See [`SetInterfaceState`].
    SetInterfaceState(SetInterfaceState),
    /// See [`SetInterfaceDescription`].
    SetInterfaceDescription(SetInterfaceDescription),
    /// See [`EnsureStaticRoute`].
    EnsureStaticRoute(EnsureStaticRoute),
    /// See [`RemoveStaticRoute`].
    RemoveStaticRoute(RemoveStaticRoute),
    /// See [`EnsureBandwidthPolicy`].
    EnsureBandwidthPolicy(EnsureBandwidthPolicy),
    /// See [`RemoveBandwidthPolicy`].
    RemoveBandwidthPolicy(RemoveBandwidthPolicy),
    /// See [`AttachBandwidthPolicy`].
    AttachBandwidthPolicy(AttachBandwidthPolicy),
    /// See [`DetachBandwidthPolicy`].
    DetachBandwidthPolicy(DetachBandwidthPolicy),
    /// See [`SetInterfaceBandwidthLimit`].
    SetInterfaceBandwidthLimit(SetInterfaceBandwidthLimit),
    /// See [`RemoveInterfaceBandwidthLimit`].
    RemoveInterfaceBandwidthLimit(RemoveInterfaceBandwidthLimit),
    /// See [`GetInterfaceState`].
    GetInterfaceState(GetInterfaceState),
    /// See [`GetBandwidthPolicy`].
    GetBandwidthPolicy(GetBandwidthPolicy),
    /// See [`GetInterfacePolicies`].
    GetInterfacePolicies(GetInterfacePolicies),
}

impl DeviceOperation {
    /// Stable snake-case name of the operation, for diagnostics.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::SetInterfaceState(_) => "set_interface_state",
            Self::SetInterfaceDescription(_) => "set_interface_description",
            Self::EnsureStaticRoute(_) => "ensure_static_route",
            Self::RemoveStaticRoute(_) => "remove_static_route",
            Self::EnsureBandwidthPolicy(_) => "ensure_bandwidth_policy",
            Self::RemoveBandwidthPolicy(_) => "remove_bandwidth_policy",
            Self::AttachBandwidthPolicy(_) => "attach_bandwidth_policy",
            Self::DetachBandwidthPolicy(_) => "detach_bandwidth_policy",
            Self::SetInterfaceBandwidthLimit(_) => "set_interface_bandwidth_limit",
            Self::RemoveInterfaceBandwidthLimit(_) => "remove_interface_bandwidth_limit",
            Self::GetInterfaceState(_) => "get_interface_state",
            Self::GetBandwidthPolicy(_) => "get_bandwidth_policy",
            Self::GetInterfacePolicies(_) => "get_interface_policies",
        }
    }

    /// Whether the operation only reads device state.
    pub const fn is_read(&self) -> bool {
        matches!(
            self,
            Self::GetInterfaceState(_)
                | Self::GetBandwidthPolicy(_)
                | Self::GetInterfacePolicies(_)
        )
    }
}

/// The result of one [`DeviceOperation`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceOutput {
    /// Result of a converging write.
    Change(Change),
    /// Result of [`GetInterfaceState`].
    AdministrativeState(AdministrativeState),
    /// Result of [`GetBandwidthPolicy`]; `None` when the policy is absent.
    BandwidthPolicy(Option<BandwidthPolicy>),
    /// Result of [`GetInterfacePolicies`].
    InterfacePolicies(InterfacePolicies),
}

impl DeviceOutput {
    /// Acknowledged writes, zero for reads.
    pub const fn writes(&self) -> usize {
        match self {
            Self::Change(change) => change.writes(),
            _ => 0,
        }
    }
}

/// Future returned by [`DynDriver::execute`].
pub type DynFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DeviceOutput, Box<dyn DeviceError>>> + Send + 'a>>;

/// Object-safe execution of the neutral operation set.
pub trait DynDriver: Send + Sync {
    /// Executes one operation.
    fn execute(&self, operation: DeviceOperation) -> DynFuture<'_>;
}

/// A driver implementing the whole neutral operation set.
///
/// Implemented automatically; it names the bounds of the blanket
/// [`DynDriver`] implementation. A driver lacking a capability still
/// implements the operation and fails it with [`ErrorKind::Unsupported`].
pub trait NeutralDriver:
    Driver
    + Execute<SetInterfaceState, Output = Change>
    + Execute<SetInterfaceDescription, Output = Change>
    + Execute<EnsureStaticRoute, Output = Change>
    + Execute<RemoveStaticRoute, Output = Change>
    + Execute<EnsureBandwidthPolicy, Output = Change>
    + Execute<RemoveBandwidthPolicy, Output = Change>
    + Execute<AttachBandwidthPolicy, Output = Change>
    + Execute<DetachBandwidthPolicy, Output = Change>
    + Execute<SetInterfaceBandwidthLimit, Output = Change>
    + Execute<RemoveInterfaceBandwidthLimit, Output = Change>
    + Execute<GetInterfaceState, Output = AdministrativeState>
    + Execute<GetBandwidthPolicy, Output = Option<BandwidthPolicy>>
    + Execute<GetInterfacePolicies, Output = InterfacePolicies>
{
}

impl<T> NeutralDriver for T where
    T: Driver
        + Execute<SetInterfaceState, Output = Change>
        + Execute<SetInterfaceDescription, Output = Change>
        + Execute<EnsureStaticRoute, Output = Change>
        + Execute<RemoveStaticRoute, Output = Change>
        + Execute<EnsureBandwidthPolicy, Output = Change>
        + Execute<RemoveBandwidthPolicy, Output = Change>
        + Execute<AttachBandwidthPolicy, Output = Change>
        + Execute<DetachBandwidthPolicy, Output = Change>
        + Execute<SetInterfaceBandwidthLimit, Output = Change>
        + Execute<RemoveInterfaceBandwidthLimit, Output = Change>
        + Execute<GetInterfaceState, Output = AdministrativeState>
        + Execute<GetBandwidthPolicy, Output = Option<BandwidthPolicy>>
        + Execute<GetInterfacePolicies, Output = InterfacePolicies>
{
}

fn boxed<E: DeviceError>(error: E) -> Box<dyn DeviceError> {
    Box::new(error)
}

impl<T: NeutralDriver> DynDriver for T {
    fn execute(&self, operation: DeviceOperation) -> DynFuture<'_> {
        Box::pin(async move {
            match operation {
                DeviceOperation::SetInterfaceState(operation) => {
                    Execute::<SetInterfaceState>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::SetInterfaceDescription(operation) => {
                    Execute::<SetInterfaceDescription>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::EnsureStaticRoute(operation) => {
                    Execute::<EnsureStaticRoute>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::RemoveStaticRoute(operation) => {
                    Execute::<RemoveStaticRoute>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::EnsureBandwidthPolicy(operation) => {
                    Execute::<EnsureBandwidthPolicy>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::RemoveBandwidthPolicy(operation) => {
                    Execute::<RemoveBandwidthPolicy>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::AttachBandwidthPolicy(operation) => {
                    Execute::<AttachBandwidthPolicy>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::DetachBandwidthPolicy(operation) => {
                    Execute::<DetachBandwidthPolicy>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::SetInterfaceBandwidthLimit(operation) => {
                    Execute::<SetInterfaceBandwidthLimit>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::RemoveInterfaceBandwidthLimit(operation) => {
                    Execute::<RemoveInterfaceBandwidthLimit>::execute(self, operation)
                        .await
                        .map(DeviceOutput::Change)
                        .map_err(boxed)
                }
                DeviceOperation::GetInterfaceState(operation) => {
                    Execute::<GetInterfaceState>::execute(self, operation)
                        .await
                        .map(DeviceOutput::AdministrativeState)
                        .map_err(boxed)
                }
                DeviceOperation::GetBandwidthPolicy(operation) => {
                    Execute::<GetBandwidthPolicy>::execute(self, operation)
                        .await
                        .map(DeviceOutput::BandwidthPolicy)
                        .map_err(boxed)
                }
                DeviceOperation::GetInterfacePolicies(operation) => {
                    Execute::<GetInterfacePolicies>::execute(self, operation)
                        .await
                        .map(DeviceOutput::InterfacePolicies)
                        .map_err(boxed)
                }
            }
        })
    }
}

/// Runs operations in order, stopping at the first error.
///
/// Returns every output, or a [`ProgramError`] stating how far the program got
/// and how many device writes were acknowledged before the failure.
pub async fn run(
    driver: &dyn DynDriver,
    operations: Vec<DeviceOperation>,
) -> Result<Vec<DeviceOutput>, ProgramError> {
    let mut outputs = Vec::with_capacity(operations.len());
    let mut completed_writes = 0;
    for operation in operations {
        let name = operation.name();
        match driver.execute(operation).await {
            Ok(output) => {
                completed_writes += output.writes();
                outputs.push(output);
            }
            Err(source) => {
                return Err(ProgramError {
                    completed_operations: outputs.len(),
                    completed_writes: completed_writes + source.completed_writes(),
                    operation: name,
                    source,
                });
            }
        }
    }
    Ok(outputs)
}

/// The failure of a [`run`] program.
#[derive(Debug)]
pub struct ProgramError {
    /// Operations that completed before the failing one.
    pub completed_operations: usize,
    /// Device writes acknowledged before the failure, including those the
    /// failing operation reported.
    pub completed_writes: usize,
    /// Name of the failing operation, as [`DeviceOperation::name`].
    pub operation: &'static str,
    /// The driver's error.
    pub source: Box<dyn DeviceError>,
}

impl ProgramError {
    /// Classification of the driver's error.
    pub fn kind(&self) -> ErrorKind {
        self.source.kind()
    }
}

impl fmt::Display for ProgramError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "operation {} ('{}') failed ({}) after {} completed operations and {} writes",
            self.completed_operations + 1,
            self.operation,
            self.kind(),
            self.completed_operations,
            self.completed_writes
        )
    }
}

impl Error for ProgramError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&*self.source as &(dyn Error + 'static))
    }
}
