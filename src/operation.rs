//! Typed imperative operations and their embedded DSL composition.

use crate::{InterfaceName, StaticRoute};
use std::error::Error;
use std::future::Future;

/// A device driver capable of executing one or more typed operations.
pub trait Driver: Send + Sync {
    /// Error type returned by this driver's operations.
    type Error: Error + Send + Sync + 'static;
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
