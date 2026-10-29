//! Typed imperative operations and their embedded DSL composition.

use crate::{
    AccessList, AccessListName, BandwidthLimits, BandwidthPolicy, DeviceError, InterfaceName,
    PolicyName, PolicyScope, StaticRoute,
};
use std::future::Future;

/// A device driver capable of executing one or more typed operations.
pub trait Driver: Send + Sync {
    /// Error type returned by this driver's operations, classified for callers.
    type Error: DeviceError;
}

/// Executes a specific operation on a [`Driver`].
///
/// Implement this trait for each operation a driver supports. External crates
/// can introduce their own operation types and implementations.
pub trait Execute<Operation: Send>: Driver {
    /// Value produced by this operation.
    type Output: Send;

    /// Executes the owned operation.
    #[allow(clippy::manual_async_fn)]
    fn execute<'a>(
        &'a self,
        operation: Operation,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a;
}

/// A typed step in a direct-operation program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationStep<Operation> {
    operation: Operation,
}

/// Starts a typed operation step.
pub const fn operation<Operation>(operation: Operation) -> OperationStep<Operation> {
    OperationStep { operation }
}

impl<Operation> OperationStep<Operation> {
    /// Sequences this step before `next`; the sequence returns the next step's output.
    pub fn then<Next>(self, next: Next) -> Sequence<Self, Next> {
        Sequence {
            first: self,
            second: next,
        }
    }
}

/// Two operation steps executed in order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Sequence<First, Second> {
    first: First,
    second: Second,
}

impl<First, Second> Sequence<First, Second> {
    /// Appends another step to this sequence.
    pub fn then<Next>(self, next: Next) -> Sequence<Self, Next> {
        Sequence {
            first: self,
            second: next,
        }
    }
}

/// Runs a DSL value against a statically selected driver.
pub trait Run<DriverType: Driver>: Send {
    /// Output of the final operation in this runnable value.
    type Output: Send;

    /// Runs the value, returning the first error or the final operation's output.
    #[allow(clippy::manual_async_fn)]
    fn run<'a>(
        self,
        driver: &'a DriverType,
    ) -> impl Future<Output = Result<Self::Output, DriverType::Error>> + Send + 'a
    where
        Self: 'a,
        DriverType: 'a;
}

impl<DriverType, Operation> Run<DriverType> for OperationStep<Operation>
where
    DriverType: Execute<Operation>,
    Operation: Send,
{
    type Output = <DriverType as Execute<Operation>>::Output;

    #[allow(clippy::manual_async_fn)]
    fn run<'a>(
        self,
        driver: &'a DriverType,
    ) -> impl Future<Output = Result<Self::Output, DriverType::Error>> + Send + 'a
    where
        Self: 'a,
        DriverType: 'a,
    {
        async move { driver.execute(self.operation).await }
    }
}

impl<DriverType, First, Second> Run<DriverType> for Sequence<First, Second>
where
    DriverType: Driver,
    First: Run<DriverType>,
    Second: Run<DriverType>,
{
    type Output = Second::Output;

    #[allow(clippy::manual_async_fn)]
    fn run<'a>(
        self,
        driver: &'a DriverType,
    ) -> impl Future<Output = Result<Self::Output, DriverType::Error>> + Send + 'a
    where
        Self: 'a,
        DriverType: 'a,
    {
        async move {
            self.first.run(driver).await?;
            self.second.run(driver).await
        }
    }
}

/// Desired administrative state for an interface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AdministrativeState {
    /// The interface should be administratively enabled.
    Enabled,
    /// The interface should be administratively disabled.
    Disabled,
}

/// Imperative operation to set an interface's administrative state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetInterfaceState {
    /// Interface to modify.
    pub interface: InterfaceName,
    /// Requested state.
    pub state: AdministrativeState,
}

impl SetInterfaceState {
    /// Creates the operation for one interface and state.
    pub fn new(interface: InterfaceName, state: AdministrativeState) -> Self {
        Self { interface, state }
    }
}

/// Imperative operation to set an interface description.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetInterfaceDescription {
    /// Interface to modify.
    pub interface: InterfaceName,
    /// New description.
    pub description: String,
}

impl SetInterfaceDescription {
    /// Creates the operation for one interface and description.
    pub fn new(interface: InterfaceName, description: impl Into<String>) -> Self {
        Self {
            interface,
            description: description.into(),
        }
    }
}

/// Imperative operation to ensure a static route exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EnsureStaticRoute {
    /// Route to ensure.
    pub route: StaticRoute,
}

impl EnsureStaticRoute {
    /// Creates the operation for one static route.
    pub const fn new(route: StaticRoute) -> Self {
        Self { route }
    }
}

/// Imperative operation to remove a static route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoveStaticRoute {
    /// Route to remove.
    pub route: StaticRoute,
}

impl RemoveStaticRoute {
    /// Creates the operation for one static route.
    pub const fn new(route: StaticRoute) -> Self {
        Self { route }
    }
}

/// Converging operation: the policy exists on the device with exactly these
/// limits, created or updated as needed; running it again changes nothing. Only devices that support
/// policies implement it; the caller decides from the device's capabilities.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnsureBandwidthPolicy {
    /// Policy to define.
    pub policy: BandwidthPolicy,
}

impl EnsureBandwidthPolicy {
    /// Creates the operation for one policy.
    pub const fn new(policy: BandwidthPolicy) -> Self {
        Self { policy }
    }
}

/// Converging operation: the policy is not defined on the device. Removing a
/// missing policy succeeds.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveBandwidthPolicy {
    /// Policy to remove.
    pub name: PolicyName,
}

impl RemoveBandwidthPolicy {
    /// Creates the operation for one policy.
    pub const fn new(name: PolicyName) -> Self {
        Self { name }
    }
}

/// Converging operation: the predefined policy applies to the interface's
/// traffic in the given scope. The policy, and for an access-list scope the
/// access list, must already exist; a driver adds only what the scope needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachBandwidthPolicy {
    /// Predefined policy to attach.
    pub name: PolicyName,
    /// Interface the policy applies to.
    pub interface: InterfaceName,
    /// Traffic it applies to.
    pub scope: PolicyScope,
}

impl AttachBandwidthPolicy {
    /// Creates the operation for one policy, interface and scope.
    pub const fn new(name: PolicyName, interface: InterfaceName, scope: PolicyScope) -> Self {
        Self {
            name,
            interface,
            scope,
        }
    }
}

/// Converging operation: the policy no longer applies to the interface's
/// traffic in the given scope. The policy itself stays defined.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetachBandwidthPolicy {
    /// Policy to detach.
    pub name: PolicyName,
    /// Interface to detach it from.
    pub interface: InterfaceName,
    /// Traffic it no longer applies to.
    pub scope: PolicyScope,
}

impl DetachBandwidthPolicy {
    /// Creates the operation for one policy, interface and scope.
    pub const fn new(name: PolicyName, interface: InterfaceName, scope: PolicyScope) -> Self {
        Self {
            name,
            interface,
            scope,
        }
    }
}

/// Converging operation: the interface's traffic is rate-limited directly with
/// these limits. This is the enforcement for devices without policy support.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetInterfaceBandwidthLimit {
    /// Interface to limit.
    pub interface: InterfaceName,
    /// Limits to enforce.
    pub limits: BandwidthLimits,
}

impl SetInterfaceBandwidthLimit {
    /// Creates the operation for one interface.
    pub const fn new(interface: InterfaceName, limits: BandwidthLimits) -> Self {
        Self { interface, limits }
    }
}

/// Converging operation: no direct rate limit remains on the interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveInterfaceBandwidthLimit {
    /// Interface to release.
    pub interface: InterfaceName,
}

impl RemoveInterfaceBandwidthLimit {
    /// Creates the operation for one interface.
    pub const fn new(interface: InterfaceName) -> Self {
        Self { interface }
    }
}

/// Converging operation: the access list exists with exactly these entries,
/// in this order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnsureAccessList {
    /// Desired list.
    pub list: AccessList,
}

impl EnsureAccessList {
    /// Creates the operation for one list.
    pub const fn new(list: AccessList) -> Self {
        Self { list }
    }
}

/// Converging operation: the access list is absent. Removing a missing list
/// succeeds; a driver refuses while a policy is attached to its traffic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoveAccessList {
    /// List to remove.
    pub name: AccessListName,
}

impl RemoveAccessList {
    /// Creates the operation for one list.
    pub const fn new(name: AccessListName) -> Self {
        Self { name }
    }
}

/// Read: an access list as the device holds it, or `None` when absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetAccessList {
    /// List to read.
    pub name: AccessListName,
}

impl GetAccessList {
    /// Creates the read for one list.
    pub const fn new(name: AccessListName) -> Self {
        Self { name }
    }
}

/// Read: the interface's administrative state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetInterfaceState {
    /// Interface to read.
    pub interface: InterfaceName,
}

impl GetInterfaceState {
    /// Creates the read for one interface.
    pub const fn new(interface: InterfaceName) -> Self {
        Self { interface }
    }
}

/// Read: a bandwidth policy as the device holds it, or `None` when absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetBandwidthPolicy {
    /// Policy to read.
    pub name: PolicyName,
}

impl GetBandwidthPolicy {
    /// Creates the read for one policy.
    pub const fn new(name: PolicyName) -> Self {
        Self { name }
    }
}

/// Read: the policies attached to an interface and its direct rate limit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GetInterfacePolicies {
    /// Interface to read.
    pub interface: InterfaceName,
}

impl GetInterfacePolicies {
    /// Creates the read for one interface.
    pub const fn new(interface: InterfaceName) -> Self {
        Self { interface }
    }
}
