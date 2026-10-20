//! Declarative desired-state resources and IaC backend contracts.

use crate::{AdministrativeState, InterfaceName, StaticRoute};
use std::error::Error;
use std::future::Future;

/// A declarative desired configuration for an interface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceConfiguration {
    /// Interface to configure.
    pub name: InterfaceName,
    /// Desired administrative state, when specified.
    pub administrative_state: Option<AdministrativeState>,
    /// Desired description, when specified.
    pub description: Option<String>,
}

impl InterfaceConfiguration {
    /// Starts a desired interface configuration.
    pub fn new(name: InterfaceName) -> Self {
        Self {
            name,
            administrative_state: None,
            description: None,
        }
    }

    /// Sets the desired administrative state.
    pub fn administrative_state(mut self, state: AdministrativeState) -> Self {
        self.administrative_state = Some(state);
        self
    }

    /// Sets the desired description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A declarative desired configuration for a static route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StaticRouteConfiguration {
    /// Desired route.
    pub route: StaticRoute,
}

impl StaticRouteConfiguration {
    /// Creates the desired state for a static route.
    pub const fn new(route: StaticRoute) -> Self {
        Self { route }
    }
}

/// Plans and applies a resource's declarative desired state.
///
/// Each resource type has its own implementation, keeping this contract open
/// to resources added by downstream crates. It intentionally promises only
/// plan and apply; backend-specific concepts remain in backend crates.
pub trait IacBackend<Resource: Sync>: Send + Sync {
    /// Error type returned by this backend.
    type Error: Error + Send + Sync + 'static;
    /// Backend-specific plan representation.
    type Plan: Send;
    /// Backend-specific apply result.
    type ApplyResult: Send;

    /// Computes a plan for the desired resource.
    #[allow(clippy::manual_async_fn)]
    fn plan<'a>(
        &'a self,
        desired: &'a Resource,
    ) -> impl Future<Output = Result<Self::Plan, Self::Error>> + Send + 'a;

    /// Applies the desired resource.
    #[allow(clippy::manual_async_fn)]
    fn apply<'a>(
        &'a self,
        desired: &'a Resource,
    ) -> impl Future<Output = Result<Self::ApplyResult, Self::Error>> + Send + 'a;
}
