# network-device

`network-device` is a dependency-free Rust core for two distinct ways to
express network configuration:

- **Operational eDSL:** typed imperative operations executed immediately by
  a selected device driver.
- **Desired state:** declarative resource values planned and applied by an
  infrastructure-as-code (IaC) backend.

The crate contains neutral domain values, standard operation/resource types,
and open extension contracts. It contains no protocol clients, vendor payload
or path abstraction, persistence, Terraform/Pulumi dependencies, or runtime.

## Operational eDSL

Implement `Driver` and `Execute<Operation>` for each supported operation. The
operation types are owned by the caller and consumed exactly once. The eDSL
composes these operations with statically typed `then`; it executes in order,
stops at the first error, and returns the last operation's output.

```rust
use network_device::{
    AdministrativeState, Driver, Execute, InterfaceName, Run, SetInterfaceState,
    operation,
};
use std::convert::Infallible;
use std::future::Future;

struct Device;

impl Driver for Device {
    type Error = Infallible;
}

impl Execute<SetInterfaceState> for Device {
    type Output = ();

    fn execute<'a>(
        &'a self,
        operation: SetInterfaceState,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
        async move {
            let _interface = operation.interface;
            let _state = operation.state;
            Ok(())
        }
    }
}

async fn enable() -> Result<(), Infallible> {
    let interface = InterfaceName::try_from("Ethernet1/1").expect("valid interface name");
    operation(SetInterfaceState::new(interface, AdministrativeState::Enabled))
        .run(&Device)
        .await
}
```

Sequential composition is static and has no boxing or dynamic dispatch:

```rust,ignore
operation(SetInterfaceDescription::new(interface.clone(), "uplink"))
    .then(operation(SetInterfaceState::new(interface, AdministrativeState::Enabled)))
    .run(&driver)
    .await?;
```

Each downstream crate can define its own operation and implement
`Execute<CustomOperation>` for a driver it owns, or implement a standard
operation for its own driver. New operation kinds do not require changes to
this crate.

## Desired State and IaC

Desired-state resources are not imperative commands. For example,
`InterfaceConfiguration` describes a target configuration, while
`SetInterfaceState` asks a driver to perform an operation now. Both can use
the same neutral values such as `InterfaceName` and `AdministrativeState`.

`IacBackend<Resource>` is generic over the resource type, not a closed enum.
Each backend chooses its own plan and apply result types and implements only
the resource types it supports:

```rust
use network_device::{
    AdministrativeState, IacBackend, InterfaceConfiguration, InterfaceName,
};
use std::convert::Infallible;
use std::future::Future;

struct ExampleBackend;

impl IacBackend<InterfaceConfiguration> for ExampleBackend {
    type Error = Infallible;
    type Plan = String;
    type ApplyResult = ();

    fn plan<'a>(
        &'a self,
        desired: &'a InterfaceConfiguration,
    ) -> impl Future<Output = Result<Self::Plan, Self::Error>> + Send + 'a {
        async move { Ok(format!("configure {}", desired.name)) }
    }

    fn apply<'a>(
        &'a self,
        _desired: &'a InterfaceConfiguration,
    ) -> impl Future<Output = Result<Self::ApplyResult, Self::Error>> + Send + 'a {
        async { Ok(()) }
    }
}

async fn apply() -> Result<(), Infallible> {
    let desired = InterfaceConfiguration::new(
        InterfaceName::try_from("Ethernet1/1").expect("valid interface name"),
    )
    .administrative_state(AdministrativeState::Enabled)
    .description("uplink");
    let backend = ExampleBackend;
    let _plan = backend.plan(&desired).await?;
    backend.apply(&desired).await
}
```

Potential implementation crates include `network-device-nxapi`,
`network-device-netconf`, `network-device-terraform`,
`network-device-opentofu`, and `network-device-pulumi`. Those names describe
future integrations only; this crate implements none of them. Terraform and
Pulumi have different semantics, so the shared IaC contract deliberately
stops at generic `plan` and `apply` operations with backend-specific result
types.

## Base Types

- `InterfaceName` rejects empty names and control characters but preserves
  vendor-specific spelling.
- `IpPrefix` validates IPv4 and IPv6 prefix lengths.
- `StaticRoute` validates that its destination and next hop use the same IP
  family.
- Standard operations: `SetInterfaceState`, `SetInterfaceDescription`,
  `EnsureStaticRoute`, and `RemoveStaticRoute`.
- Standard desired resources: `InterfaceConfiguration` and
  `StaticRouteConfiguration`.

These are shared semantic types, not a claim that every device protocol maps
them to identical native paths or payloads.
