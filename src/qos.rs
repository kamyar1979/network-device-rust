//! Vendor-neutral bandwidth-policy values.
//!
//! A [`BandwidthPolicy`] states the limits a device must enforce; how a driver
//! realizes it (policy maps, classifiers, per-direction objects, device names)
//! is the driver's concern.

use std::fmt;
use std::net::IpAddr;
use std::num::NonZeroU64;
use std::str::FromStr;

/// A validated, vendor-neutral policy name.
///
/// Names are 1 to 63 ASCII letters, digits, hyphens or underscores. A driver
/// whose device accepts shorter names derives a device name from this one or
/// rejects the operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PolicyName(String);

/// Longest accepted [`PolicyName`].
pub const MAX_POLICY_NAME_LENGTH: usize = 63;

impl PolicyName {
    /// Creates a policy name, rejecting empty, overlong, or non-ASCII-word values.
    pub fn new(value: impl Into<String>) -> Result<Self, PolicyNameError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_POLICY_NAME_LENGTH {
            return Err(PolicyNameError::Length);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(PolicyNameError::Character);
        }
        Ok(Self(value))
    }

    /// Returns the name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for PolicyName {
    type Err = PolicyNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<&str> for PolicyName {
    type Error = PolicyNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for PolicyName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Validation errors for [`PolicyName`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyNameError {
    /// The name is empty or longer than [`MAX_POLICY_NAME_LENGTH`].
    Length,
    /// The name contains a character other than ASCII letters, digits, `-` or `_`.
    Character,
}

impl fmt::Display for PolicyNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Length => "policy name must be 1 to 63 characters",
            Self::Character => {
                "policy name must contain only ASCII letters, digits, hyphens or underscores"
            }
        })
    }
}

impl std::error::Error for PolicyNameError {}

/// A positive rate in bits per second.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BitsPerSecond(NonZeroU64);

impl BitsPerSecond {
    /// Creates a rate.
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    /// Returns the rate in bits per second.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// A positive burst size in bytes.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BurstBytes(NonZeroU64);

impl BurstBytes {
    /// Creates a burst size.
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    /// Returns the burst size in bytes.
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// The limit enforced in one traffic direction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BandwidthLimit {
    /// Committed rate.
    pub rate: BitsPerSecond,
    /// Burst size, when the policy sets one; otherwise the driver's default.
    pub burst: Option<BurstBytes>,
}

impl BandwidthLimit {
    /// Creates a limit without an explicit burst.
    pub const fn new(rate: BitsPerSecond) -> Self {
        Self { rate, burst: None }
    }

    /// Sets the burst size.
    pub const fn burst(mut self, burst: BurstBytes) -> Self {
        self.burst = Some(burst);
        self
    }
}

/// A bandwidth policy: at least one direction is limited.
///
/// Ingress is traffic entering the device on the interface (a customer's
/// upload); egress is traffic leaving it (a customer's download).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BandwidthPolicy {
    name: PolicyName,
    ingress: Option<BandwidthLimit>,
    egress: Option<BandwidthLimit>,
}

impl BandwidthPolicy {
    /// Creates a policy, requiring at least one limited direction.
    pub fn new(
        name: PolicyName,
        ingress: Option<BandwidthLimit>,
        egress: Option<BandwidthLimit>,
    ) -> Result<Self, BandwidthPolicyError> {
        if ingress.is_none() && egress.is_none() {
            return Err(BandwidthPolicyError::Unlimited);
        }
        Ok(Self {
            name,
            ingress,
            egress,
        })
    }

    /// Returns the policy name.
    pub fn name(&self) -> &PolicyName {
        &self.name
    }

    /// Returns the ingress limit, if any.
    pub const fn ingress(&self) -> Option<BandwidthLimit> {
        self.ingress
    }

    /// Returns the egress limit, if any.
    pub const fn egress(&self) -> Option<BandwidthLimit> {
        self.egress
    }
}

/// Validation error for [`BandwidthPolicy`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BandwidthPolicyError {
    /// Neither direction is limited.
    Unlimited,
}

impl fmt::Display for BandwidthPolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("bandwidth policy must limit ingress, egress, or both")
    }
}

impl std::error::Error for BandwidthPolicyError {}

/// The bandwidth limit of one host address on an interface.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct HostBandwidthLimit {
    /// Host whose traffic is limited.
    pub host: IpAddr,
    /// Limits enforced for that host's traffic.
    pub policy: BandwidthPolicy,
}

/// Bandwidth policies observed on one interface.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InterfacePolicies {
    /// Policies applied to all traffic of the interface.
    pub attached: Vec<PolicyName>,
    /// Host limits on the interface, at most one per host address.
    pub host_limits: Vec<HostBandwidthLimit>,
}

/// What a converging operation did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Change {
    /// The device already matched; nothing was written.
    Unchanged,
    /// The device was changed with this many acknowledged writes.
    Changed {
        /// Acknowledged device writes.
        writes: usize,
    },
}

impl Change {
    /// Builds the change from a count of acknowledged writes.
    pub const fn from_writes(writes: usize) -> Self {
        if writes == 0 {
            Self::Unchanged
        } else {
            Self::Changed { writes }
        }
    }

    /// Returns the number of acknowledged writes.
    pub const fn writes(self) -> usize {
        match self {
            Self::Unchanged => 0,
            Self::Changed { writes } => writes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate(value: u64) -> BitsPerSecond {
        BitsPerSecond::new(NonZeroU64::new(value).unwrap())
    }

    #[test]
    fn policy_names_are_ascii_words_of_bounded_length() {
        assert!(PolicyName::new("quota-exceeded_bandwidth").is_ok());
        assert_eq!(PolicyName::new(""), Err(PolicyNameError::Length));
        assert_eq!(
            PolicyName::new("x".repeat(MAX_POLICY_NAME_LENGTH + 1)),
            Err(PolicyNameError::Length)
        );
        assert!(PolicyName::new("x".repeat(MAX_POLICY_NAME_LENGTH)).is_ok());
        assert_eq!(PolicyName::new("a b"), Err(PolicyNameError::Character));
        assert_eq!(PolicyName::new("pölicy"), Err(PolicyNameError::Character));
    }

    #[test]
    fn a_policy_limits_at_least_one_direction() {
        let name = PolicyName::new("p").unwrap();
        assert_eq!(
            BandwidthPolicy::new(name.clone(), None, None),
            Err(BandwidthPolicyError::Unlimited)
        );
        let policy =
            BandwidthPolicy::new(name, None, Some(BandwidthLimit::new(rate(20_000_000)))).unwrap();
        assert_eq!(policy.egress().unwrap().rate.get(), 20_000_000);
        assert!(policy.ingress().is_none());
    }

    #[test]
    fn change_counts_writes() {
        assert_eq!(Change::from_writes(0), Change::Unchanged);
        assert_eq!(Change::from_writes(3), Change::Changed { writes: 3 });
        assert_eq!(Change::Changed { writes: 3 }.writes(), 3);
        assert_eq!(Change::Unchanged.writes(), 0);
    }
}
