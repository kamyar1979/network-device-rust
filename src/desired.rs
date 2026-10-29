//! Desired state of one device, recorded from the same operations a driver
//! executes, and rendered as a neutral IaC document.
//!
//! [`DesiredStateRecorder`] is a [`Driver`] for the whole neutral operation
//! set. Instead of changing a device it records what each operation requires:
//! converging writes become present or explicitly absent entries, so the
//! rendered document states both what must exist and what must be removed.
//! The same program can therefore be applied to a device by a vendor driver,
//! or turned into a file by this recorder.

use crate::{
    AdministrativeState, AttachBandwidthPolicy, BandwidthLimit, BandwidthLimits, BandwidthPolicy,
    Change, DetachBandwidthPolicy, DeviceError, Driver, EnsureBandwidthPolicy, EnsureStaticRoute,
    ErrorKind, Execute, GetBandwidthPolicy, GetInterfacePolicies, GetInterfaceState, InterfaceName,
    InterfacePolicies, PolicyAttachment, PolicyName, PolicyScope, RemoveBandwidthPolicy,
    RemoveInterfaceBandwidthLimit, RemoveStaticRoute, SetInterfaceBandwidthLimit,
    SetInterfaceDescription, SetInterfaceState, StaticRoute,
};
use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::future::Future;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Schema identifier written into every rendered document.
pub const DESIRED_STATE_SCHEMA: &str = "network-device.desired-state/1";

/// Whether an entry must exist or must be absent.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Presence {
    /// The entry must exist.
    Present,
    /// The entry must not exist.
    Absent,
}

/// A bandwidth policy that must exist with these limits, or must be absent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PolicyState {
    /// The policy must exist exactly like this.
    Present(BandwidthPolicy),
    /// The policy must be absent.
    Absent,
}

/// A direct interface rate limit that must exist with these limits, or must be absent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitState {
    /// The interface must be rate-limited like this.
    Present(BandwidthLimits),
    /// No direct rate limit may remain.
    Absent,
}

/// The desired state of one interface.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InterfaceState {
    /// Administrative state, when the program sets one.
    pub administrative_state: Option<AdministrativeState>,
    /// Description, when the program sets one.
    pub description: Option<String>,
    /// Predefined policies attached (or detached) per scope.
    pub attachments: BTreeMap<(PolicyName, PolicyScope), Presence>,
    /// Direct rate limit, for devices enforcing without policies.
    pub bandwidth_limit: Option<LimitState>,
}

/// The desired state of one device.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DesiredState {
    /// Identifier of the device the state is for.
    pub device: String,
    /// Bandwidth policies defined on (or removed from) the device.
    pub policies: BTreeMap<PolicyName, PolicyState>,
    /// Interfaces the program configures.
    pub interfaces: BTreeMap<InterfaceName, InterfaceState>,
    /// Static routes that must exist or be absent.
    pub static_routes: BTreeMap<StaticRoute, Presence>,
}

impl DesiredState {
    /// Creates an empty desired state for one device.
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
            ..Self::default()
        }
    }

    /// Renders the state as a neutral IaC document: pretty-printed JSON with
    /// [`DESIRED_STATE_SCHEMA`], keys in a fixed order and entries sorted, so
    /// the same state always renders the same bytes.
    pub fn to_json(&self) -> String {
        let document = Json::Object(vec![
            ("schema", Json::from(DESIRED_STATE_SCHEMA)),
            ("device", Json::from(self.device.as_str())),
            (
                "policies",
                Json::Array(
                    self.policies
                        .iter()
                        .map(|(name, state)| match state {
                            PolicyState::Present(policy) => Json::Object(vec![
                                ("name", Json::from(name.as_str())),
                                ("ensure", Json::from("present")),
                                ("ingress", limit_json(policy.limits.ingress())),
                                ("egress", limit_json(policy.limits.egress())),
                            ]),
                            PolicyState::Absent => Json::Object(vec![
                                ("name", Json::from(name.as_str())),
                                ("ensure", Json::from("absent")),
                            ]),
                        })
                        .collect(),
                ),
            ),
            (
                "interfaces",
                Json::Array(
                    self.interfaces
                        .iter()
                        .map(|(name, interface)| interface_json(name, interface))
                        .collect(),
                ),
            ),
            (
                "static_routes",
                Json::Array(
                    self.static_routes
                        .iter()
                        .map(|(route, presence)| {
                            Json::Object(vec![
                                ("destination", Json::String(route.destination().to_string())),
                                ("next_hop", Json::String(route.next_hop().to_string())),
                                ("ensure", presence_json(*presence)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]);
        let mut output = String::new();
        document.write(&mut output, 0);
        output.push('\n');
        output
    }

    fn interface(&mut self, name: &InterfaceName) -> &mut InterfaceState {
        self.interfaces.entry(name.clone()).or_default()
    }
}

fn presence_json(presence: Presence) -> Json {
    Json::from(match presence {
        Presence::Present => "present",
        Presence::Absent => "absent",
    })
}

fn limit_json(limit: Option<BandwidthLimit>) -> Json {
    match limit {
        None => Json::Null,
        Some(limit) => Json::Object(vec![
            ("rate_bps", Json::Number(limit.rate.get())),
            (
                "burst_bytes",
                limit
                    .burst
                    .map_or(Json::Null, |burst| Json::Number(burst.get())),
            ),
        ]),
    }
}

fn interface_json(name: &InterfaceName, interface: &InterfaceState) -> Json {
    Json::Object(vec![
        ("name", Json::from(name.as_str())),
        (
            "administrative_state",
            interface.administrative_state.map_or(Json::Null, |state| {
                Json::from(match state {
                    AdministrativeState::Enabled => "enabled",
                    AdministrativeState::Disabled => "disabled",
                })
            }),
        ),
        (
            "description",
            interface
                .description
                .as_deref()
                .map_or(Json::Null, Json::from),
        ),
        (
            "attachments",
            Json::Array(
                interface
                    .attachments
                    .iter()
                    .map(|((name, scope), presence)| {
                        Json::Object(vec![
                            ("policy", Json::from(name.as_str())),
                            (
                                "host",
                                match scope {
                                    PolicyScope::Interface => Json::Null,
                                    PolicyScope::Host(host) => Json::String(host.to_string()),
                                },
                            ),
                            ("ensure", presence_json(*presence)),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "bandwidth_limit",
            match interface.bandwidth_limit {
                None => Json::Null,
                Some(LimitState::Present(limits)) => Json::Object(vec![
                    ("ensure", Json::from("present")),
                    ("ingress", limit_json(limits.ingress())),
                    ("egress", limit_json(limits.egress())),
                ]),
                Some(LimitState::Absent) => Json::Object(vec![("ensure", Json::from("absent"))]),
            },
        ),
    ])
}

/// Records a program into a [`DesiredState`] instead of changing a device.
///
/// Writes report one write when they changed the recorded state and
/// [`Change::Unchanged`] when it already matched. Reads answer from the
/// recorded state.
#[derive(Debug, Default)]
pub struct DesiredStateRecorder {
    state: Mutex<DesiredState>,
}

impl DesiredStateRecorder {
    /// Starts recording the desired state of one device.
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            state: Mutex::new(DesiredState::new(device)),
        }
    }

    /// Returns a copy of the state recorded so far.
    pub fn state(&self) -> DesiredState {
        self.lock().clone()
    }

    /// Finishes recording and returns the state.
    pub fn into_state(self) -> DesiredState {
        self.state
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lock(&self) -> MutexGuard<'_, DesiredState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn record(&self, update: impl FnOnce(&mut DesiredState) -> bool) -> Change {
        Change::from_writes(usize::from(update(&mut self.lock())))
    }
}

/// Replaces `slot` with `value`, reporting whether it changed.
fn set<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

/// Inserts `value` under `key`, reporting whether the map changed.
fn put<K: Ord, V: PartialEq>(map: &mut BTreeMap<K, V>, key: K, value: V) -> bool {
    if map.get(&key) == Some(&value) {
        false
    } else {
        map.insert(key, value);
        true
    }
}

/// A read the recorded program did not describe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotDescribed {
    what: String,
}

impl NotDescribed {
    /// Returns what the program did not describe.
    pub fn what(&self) -> &str {
        &self.what
    }
}

impl fmt::Display for NotDescribed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "the recorded program does not describe {}",
            self.what
        )
    }
}

impl std::error::Error for NotDescribed {}

impl DeviceError for NotDescribed {
    fn kind(&self) -> ErrorKind {
        ErrorKind::Rejected
    }
}

impl Driver for DesiredStateRecorder {
    type Error = NotDescribed;
}

macro_rules! recorded {
    ($operation:ty, |$state:ident, $op:ident| $update:expr) => {
        impl Execute<$operation> for DesiredStateRecorder {
            type Output = Change;

            #[allow(clippy::manual_async_fn)]
            fn execute<'a>(
                &'a self,
                $op: $operation,
            ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
                async move { Ok(self.record(|$state| $update)) }
            }
        }
    };
}

recorded!(SetInterfaceState, |state, op| set(
    &mut state.interface(&op.interface).administrative_state,
    Some(op.state)
));
recorded!(SetInterfaceDescription, |state, op| set(
    &mut state.interface(&op.interface).description,
    Some(op.description)
));
recorded!(EnsureStaticRoute, |state, op| put(
    &mut state.static_routes,
    op.route,
    Presence::Present
));
recorded!(RemoveStaticRoute, |state, op| put(
    &mut state.static_routes,
    op.route,
    Presence::Absent
));
recorded!(EnsureBandwidthPolicy, |state, op| put(
    &mut state.policies,
    op.policy.name.clone(),
    PolicyState::Present(op.policy)
));
recorded!(RemoveBandwidthPolicy, |state, op| put(
    &mut state.policies,
    op.name,
    PolicyState::Absent
));
recorded!(AttachBandwidthPolicy, |state, op| put(
    &mut state.interface(&op.interface).attachments,
    (op.name, op.scope),
    Presence::Present
));
recorded!(DetachBandwidthPolicy, |state, op| put(
    &mut state.interface(&op.interface).attachments,
    (op.name, op.scope),
    Presence::Absent
));
recorded!(SetInterfaceBandwidthLimit, |state, op| set(
    &mut state.interface(&op.interface).bandwidth_limit,
    Some(LimitState::Present(op.limits))
));
recorded!(RemoveInterfaceBandwidthLimit, |state, op| set(
    &mut state.interface(&op.interface).bandwidth_limit,
    Some(LimitState::Absent)
));

impl Execute<GetInterfaceState> for DesiredStateRecorder {
    type Output = AdministrativeState;

    #[allow(clippy::manual_async_fn)]
    fn execute<'a>(
        &'a self,
        operation: GetInterfaceState,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
        async move {
            self.lock()
                .interfaces
                .get(&operation.interface)
                .and_then(|interface| interface.administrative_state)
                .ok_or_else(|| NotDescribed {
                    what: format!("the administrative state of {}", operation.interface),
                })
        }
    }
}

impl Execute<GetBandwidthPolicy> for DesiredStateRecorder {
    type Output = Option<BandwidthPolicy>;

    #[allow(clippy::manual_async_fn)]
    fn execute<'a>(
        &'a self,
        operation: GetBandwidthPolicy,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
        async move {
            Ok(match self.lock().policies.get(&operation.name) {
                Some(PolicyState::Present(policy)) => Some(policy.clone()),
                _ => None,
            })
        }
    }
}

impl Execute<GetInterfacePolicies> for DesiredStateRecorder {
    type Output = InterfacePolicies;

    #[allow(clippy::manual_async_fn)]
    fn execute<'a>(
        &'a self,
        operation: GetInterfacePolicies,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send + 'a {
        async move {
            let state = self.lock();
            let Some(interface) = state.interfaces.get(&operation.interface) else {
                return Ok(InterfacePolicies::default());
            };
            Ok(InterfacePolicies {
                attachments: interface
                    .attachments
                    .iter()
                    .filter(|(_, presence)| **presence == Presence::Present)
                    .map(|((name, scope), _)| PolicyAttachment {
                        name: name.clone(),
                        scope: *scope,
                    })
                    .collect(),
                bandwidth_limit: match interface.bandwidth_limit {
                    Some(LimitState::Present(limits)) => Some(limits),
                    _ => None,
                },
            })
        }
    }
}

/// Minimal JSON value with deterministic, pretty-printed output.
enum Json {
    Null,
    Number(u64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(&'static str, Json)>),
}

impl From<&str> for Json {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl Json {
    fn write(&self, output: &mut String, indent: usize) {
        match self {
            Self::Null => output.push_str("null"),
            Self::Number(value) => {
                let _ = write!(output, "{value}");
            }
            Self::String(value) => write_string(output, value),
            Self::Array(items) if items.is_empty() => output.push_str("[]"),
            Self::Array(items) => {
                output.push('[');
                for (index, item) in items.iter().enumerate() {
                    output.push_str(if index == 0 { "\n" } else { ",\n" });
                    push_indent(output, indent + 1);
                    item.write(output, indent + 1);
                }
                output.push('\n');
                push_indent(output, indent);
                output.push(']');
            }
            Self::Object(fields) if fields.is_empty() => output.push_str("{}"),
            Self::Object(fields) => {
                output.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    output.push_str(if index == 0 { "\n" } else { ",\n" });
                    push_indent(output, indent + 1);
                    write_string(output, key);
                    output.push_str(": ");
                    value.write(output, indent + 1);
                }
                output.push('\n');
                push_indent(output, indent);
                output.push('}');
            }
        }
    }
}

fn push_indent(output: &mut String, indent: usize) {
    for _ in 0..indent {
        output.push_str("  ");
    }
}

fn write_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(output, "\\u{:04x}", u32::from(control));
            }
            other => output.push(other),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BitsPerSecond, BurstBytes, DeviceOperation, DeviceOutput, run};
    use std::net::{IpAddr, Ipv4Addr};
    use std::num::NonZeroU64;
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

    fn rate(value: u64) -> BitsPerSecond {
        BitsPerSecond::new(NonZeroU64::new(value).unwrap())
    }

    fn name(value: &str) -> PolicyName {
        PolicyName::new(value).unwrap()
    }

    fn port() -> InterfaceName {
        InterfaceName::new("Ethernet1/1").unwrap()
    }

    fn host() -> PolicyScope {
        PolicyScope::Host(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 20)))
    }

    fn limits() -> BandwidthLimits {
        BandwidthLimits::new(
            Some(BandwidthLimit::new(rate(10_000_000))),
            Some(
                BandwidthLimit::new(rate(20_000_000))
                    .burst(BurstBytes::new(NonZeroU64::new(1_500_000).unwrap())),
            ),
        )
        .unwrap()
    }

    fn throttle() -> BandwidthPolicy {
        BandwidthPolicy::new(name("throttle"), limits())
    }

    fn program() -> Vec<DeviceOperation> {
        vec![
            DeviceOperation::EnsureBandwidthPolicy(EnsureBandwidthPolicy::new(throttle())),
            DeviceOperation::AttachBandwidthPolicy(AttachBandwidthPolicy::new(
                name("throttle"),
                port(),
                PolicyScope::Interface,
            )),
            DeviceOperation::AttachBandwidthPolicy(AttachBandwidthPolicy::new(
                name("throttle"),
                port(),
                host(),
            )),
            DeviceOperation::RemoveBandwidthPolicy(RemoveBandwidthPolicy::new(name("old"))),
            DeviceOperation::SetInterfaceDescription(SetInterfaceDescription::new(
                port(),
                "customer \"A\"\tport",
            )),
            DeviceOperation::RemoveStaticRoute(RemoveStaticRoute::new(
                StaticRoute::new(
                    "198.51.100.0/24".parse().unwrap(),
                    IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
                )
                .unwrap(),
            )),
        ]
    }

    #[test]
    fn the_same_program_records_through_the_runtime_interface() {
        let recorder = DesiredStateRecorder::new("leaf-1");
        let outputs = block_on(run(&recorder, program())).unwrap();
        assert!(outputs.iter().all(|output| output.writes() == 1));

        let state = recorder.state();
        assert_eq!(
            state.policies[&name("throttle")],
            PolicyState::Present(throttle())
        );
        assert_eq!(state.policies[&name("old")], PolicyState::Absent);
        let interface = &state.interfaces[&port()];
        assert_eq!(
            interface.attachments[&(name("throttle"), PolicyScope::Interface)],
            Presence::Present
        );
        assert_eq!(
            interface.attachments[&(name("throttle"), host())],
            Presence::Present
        );
        assert_eq!(state.static_routes.values().next(), Some(&Presence::Absent));
    }

    #[test]
    fn recording_converges_and_later_operations_win() {
        let recorder = DesiredStateRecorder::new("leaf-1");
        block_on(run(&recorder, program())).unwrap();
        let again = block_on(run(&recorder, program())).unwrap();
        assert!(again.iter().all(|output| output.writes() == 0));

        block_on(run(
            &recorder,
            vec![
                DeviceOperation::DetachBandwidthPolicy(DetachBandwidthPolicy::new(
                    name("throttle"),
                    port(),
                    host(),
                )),
                DeviceOperation::SetInterfaceBandwidthLimit(SetInterfaceBandwidthLimit::new(
                    port(),
                    limits(),
                )),
                DeviceOperation::RemoveInterfaceBandwidthLimit(RemoveInterfaceBandwidthLimit::new(
                    port(),
                )),
            ],
        ))
        .unwrap();
        let interface = &recorder.state().interfaces[&port()];
        assert_eq!(
            interface.attachments[&(name("throttle"), host())],
            Presence::Absent
        );
        assert_eq!(
            interface.attachments[&(name("throttle"), PolicyScope::Interface)],
            Presence::Present
        );
        assert_eq!(interface.bandwidth_limit, Some(LimitState::Absent));
    }

    #[test]
    fn reads_answer_from_the_recorded_state() {
        let recorder = DesiredStateRecorder::new("leaf-1");
        block_on(run(&recorder, program())).unwrap();
        let outputs = block_on(run(
            &recorder,
            vec![
                DeviceOperation::GetBandwidthPolicy(GetBandwidthPolicy::new(name("throttle"))),
                DeviceOperation::GetBandwidthPolicy(GetBandwidthPolicy::new(name("old"))),
                DeviceOperation::GetInterfacePolicies(GetInterfacePolicies::new(port())),
            ],
        ))
        .unwrap();
        assert_eq!(outputs[0], DeviceOutput::BandwidthPolicy(Some(throttle())));
        assert_eq!(outputs[1], DeviceOutput::BandwidthPolicy(None));
        let DeviceOutput::InterfacePolicies(observed) = &outputs[2] else {
            panic!("expected interface policies");
        };
        assert_eq!(observed.attachments.len(), 2);
        assert!(observed.bandwidth_limit.is_none());

        let error = block_on(run(
            &recorder,
            vec![DeviceOperation::GetInterfaceState(GetInterfaceState::new(
                port(),
            ))],
        ))
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Rejected);
    }

    #[test]
    fn the_rendered_document_is_deterministic_neutral_json() {
        let recorder = DesiredStateRecorder::new("leaf-1");
        block_on(run(&recorder, program())).unwrap();
        block_on(run(
            &recorder,
            vec![DeviceOperation::SetInterfaceBandwidthLimit(
                SetInterfaceBandwidthLimit::new(
                    InterfaceName::new("Ethernet1/2").unwrap(),
                    limits(),
                ),
            )],
        ))
        .unwrap();
        let expected = r#"{
  "schema": "network-device.desired-state/1",
  "device": "leaf-1",
  "policies": [
    {
      "name": "old",
      "ensure": "absent"
    },
    {
      "name": "throttle",
      "ensure": "present",
      "ingress": {
        "rate_bps": 10000000,
        "burst_bytes": null
      },
      "egress": {
        "rate_bps": 20000000,
        "burst_bytes": 1500000
      }
    }
  ],
  "interfaces": [
    {
      "name": "Ethernet1/1",
      "administrative_state": null,
      "description": "customer \"A\"\tport",
      "attachments": [
        {
          "policy": "throttle",
          "host": null,
          "ensure": "present"
        },
        {
          "policy": "throttle",
          "host": "192.0.2.20",
          "ensure": "present"
        }
      ],
      "bandwidth_limit": null
    },
    {
      "name": "Ethernet1/2",
      "administrative_state": null,
      "description": null,
      "attachments": [],
      "bandwidth_limit": {
        "ensure": "present",
        "ingress": {
          "rate_bps": 10000000,
          "burst_bytes": null
        },
        "egress": {
          "rate_bps": 20000000,
          "burst_bytes": 1500000
        }
      }
    }
  ],
  "static_routes": [
    {
      "destination": "198.51.100.0/24",
      "next_hop": "192.0.2.1",
      "ensure": "absent"
    }
  ]
}
"#;
        assert_eq!(recorder.into_state().to_json(), expected);
    }
}
