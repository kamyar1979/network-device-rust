# network-device

`network-device` is a dependency-free Rust core for configuring network
devices of any vendor with one uniform eDSL. A program of vendor-neutral
operations can be:

- **applied to a device** by a vendor driver (for example `nxapi` for Cisco
  NX-API), or
- **recorded as desired state** by `DesiredStateRecorder` and rendered as a
  neutral infrastructure-as-code (IaC) document.

Operations state **what** must hold on a device; each driver decides **how**
its device realizes it. Mutating operations converge (running one again when
the device already matches changes nothing), which is what makes the same
program both executable and declarative. Reads return normalized values for
discovery and for comparing a device with its desired state.

The crate contains neutral domain values, the standard operations, and open
extension contracts. It contains no protocol clients, vendor payloads,
persistence, IaC tool dependencies, or async runtime.

## Neutral operations

| Operation | Effect | Output |
| --- | --- | --- |
| `SetInterfaceState`, `SetInterfaceDescription` | Interface administrative state, description | `Change` |
| `EnsureStaticRoute`, `RemoveStaticRoute` | The route exists / is absent | `Change` |
| `EnsureBandwidthPolicy`, `RemoveBandwidthPolicy` | The policy exists on the device with exactly these limits / is absent | `Change` |
| `AttachBandwidthPolicy`, `DetachBandwidthPolicy` | The predefined policy applies / no longer applies to an interface, scoped to all its traffic or to one host address | `Change` |
| `SetInterfaceBandwidthLimit`, `RemoveInterfaceBandwidthLimit` | The interface is rate-limited directly / no longer limited, for devices without policy support | `Change` |
| `GetInterfaceState` | Read | `AdministrativeState` |
| `GetBandwidthPolicy` | Read | `Option<BandwidthPolicy>` |
| `GetInterfacePolicies` | Read: attached policies with their scopes, and the direct rate limit | `InterfacePolicies` |

A `BandwidthPolicy` is a validated `PolicyName` with `BandwidthLimits`: an
ingress and/or egress `BandwidthLimit` (rate in bits per second, optional
burst in bytes). `Change` is `Unchanged` or `Changed { writes }`.

Policies are defined once on a device and then only attached: attaching to a
host never creates a policy of its own, so a driver adds at most what the scope
needs (for example a classifier entry). Which enforcement a device gets
(attaching a predefined policy, or a direct rate limit on a device without
policy support) and which scope is the caller's decision, made from what it
knows about the device; a driver implements the operations its devices support
and answers the others with `ErrorKind::Unsupported`.

Every driver error implements `DeviceError` and is classified by `ErrorKind`:
`Unsupported`, `Rejected`, `Transient` (safe to retry), `UnknownOutcome` (a
write may have been applied), or `Authentication`, together with the number of
device writes acknowledged before the failure. Callers decide retries and
outcomes from the kind, never from vendor error types.

## Run-time dispatch

A driver implementing `Execute` for the whole neutral set (`NeutralDriver`) is
automatically a `DynDriver`, an object-safe trait. A caller that chooses the
driver at run time, for example per device from inventory, holds a
`Box<dyn DynDriver>` and runs a list of `DeviceOperation`s:

```rust,ignore
match network_device::run(driver.as_ref(), operations).await {
    Ok(outputs) => { /* every operation applied */ }
    Err(error) => {
        // error.completed_operations, error.completed_writes, error.kind()
    }
}
```

`run` executes in order, stops at the first error, and reports how far the
program got. A driver lacking a capability still implements the operation and
fails it with `ErrorKind::Unsupported` (see `Unsupported`).

## Desired state and neutral IaC

`DesiredStateRecorder` is a driver for the whole neutral set that changes no
device: it records what each operation requires into a `DesiredState` for one
device. Converging writes become entries that are present or explicitly
absent, so the state says both what must exist and what must be removed.
Reads answer from the recorded state.

```rust,ignore
let recorder = DesiredStateRecorder::new("leaf-1");
network_device::run(&recorder, operations).await?;
std::fs::write("leaf-1.json", recorder.into_state().to_json())?;
```

`DesiredState::to_json` renders a neutral document with the schema
`network-device.desired-state/1`: pretty-printed JSON with fixed key order
and sorted entries, so the same state always renders the same bytes and
changes diff cleanly. Tool-specific IaC (Terraform, Ansible, vendor
templates) is generated from this document or from `DesiredState` by an
`IacBackend`; this crate depends on none of those tools.

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

### Connecting and authenticating a driver

Implement `DriverConnector` in the protocol/provider integration crate. The
connector defines its own target, authentication, driver, and connection error
types, and consumes the authentication value while constructing an
authenticated driver. It does not retrieve credentials.

An external integration can use its existing `CredentialProvider` (for
example, `oss-providers::CredentialProvider`) to resolve a
`CredentialMaterial`, convert that material to the selected connector's
`Authentication` type, and then call `connect`:

```rust,ignore
let credential = credential_provider.credential(query).await?;
let authentication = ProviderAuthentication::try_from(credential.material)?;
let driver = connector.connect(&target, authentication).await?;

operation(provider_operation).run(&driver).await?;
```

This keeps secret retrieval, leases, and renewal in the external credential
integration while keeping protocol-specific authentication in the provider.
`network-device` itself has no dependency on `oss-providers`, Vault, or any
credential backend. The associated types and returned driver use static
dispatch; the connector contract adds no `Box`, trait object, or shared-state
wrapper.

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

## IaC backends

`IacBackend<Resource>` plans and applies a desired resource with a
backend-specific plan and result, for example a `DesiredState` rendered into a
tool's files. The finer-grained resources `InterfaceConfiguration` and
`StaticRouteConfiguration` remain available.

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
- `PolicyName` accepts 1 to 63 ASCII letters, digits, hyphens or underscores;
  `BandwidthPolicy` limits at least one direction.
- Standard operations: see the table above.
- Standard desired resources: `DesiredState`, `InterfaceConfiguration`, and
  `StaticRouteConfiguration`.

These are shared semantic types, not a claim that every device protocol maps
them to identical native paths or payloads.
