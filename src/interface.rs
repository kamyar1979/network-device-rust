//! Neutral interface and IP-prefix value types.

use std::fmt;
use std::net::IpAddr;
use std::str::FromStr;

/// A validated, vendor-neutral interface identifier.
///
/// The value is preserved as supplied because interface naming conventions
/// vary across vendors and platforms.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceName(String);

impl InterfaceName {
    /// Creates an interface name, rejecting empty and control-character values.
    pub fn new(value: impl Into<String>) -> Result<Self, InterfaceNameError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(InterfaceNameError::Empty);
        }
        if value.chars().any(char::is_control) {
            return Err(InterfaceNameError::ControlCharacter);
        }
        Ok(Self(value))
    }

    /// Returns the original interface identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for InterfaceName {
    type Err = InterfaceNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<&str> for InterfaceName {
    type Error = InterfaceNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for InterfaceName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Validation errors for [`InterfaceName`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterfaceNameError {
    /// The name is empty or contains only whitespace.
    Empty,
    /// The name contains a control character.
    ControlCharacter,
}

impl fmt::Display for InterfaceNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "interface name must not be empty or whitespace-only",
            Self::ControlCharacter => "interface name must not contain control characters",
        })
    }
}

impl std::error::Error for InterfaceNameError {}

/// A validated IP address and network prefix length.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct IpPrefix {
    /// Address portion of the prefix.
    address: IpAddr,
    /// Prefix length, from zero through 32 or 128 according to address family.
    prefix_length: u8,
}

impl IpPrefix {
    /// Creates a prefix after validating its length against the address family.
    pub fn new(address: IpAddr, prefix_length: u8) -> Result<Self, IpPrefixError> {
        let maximum = match address {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        if prefix_length > maximum {
            return Err(IpPrefixError {
                prefix_length,
                maximum,
            });
        }
        Ok(Self {
            address,
            prefix_length,
        })
    }

    /// Returns the address portion of this prefix.
    pub const fn address(self) -> IpAddr {
        self.address
    }

    /// Returns the prefix length.
    pub const fn prefix_length(self) -> u8 {
        self.prefix_length
    }
}

impl FromStr for IpPrefix {
    type Err = IpPrefixParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let (address, length) = value
            .rsplit_once('/')
            .ok_or(IpPrefixParseError::MissingPrefixLength)?;
        let address = address
            .parse::<IpAddr>()
            .map_err(|_| IpPrefixParseError::InvalidAddress)?;
        let length = length
            .parse::<u8>()
            .map_err(|_| IpPrefixParseError::InvalidPrefixLength)?;
        Self::new(address, length).map_err(IpPrefixParseError::OutOfRange)
    }
}

impl fmt::Display for IpPrefix {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.address, self.prefix_length)
    }
}

/// Error returned when an IP prefix length exceeds its address family's limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpPrefixError {
    /// Supplied prefix length.
    pub prefix_length: u8,
    /// Maximum permitted prefix length.
    pub maximum: u8,
}

impl fmt::Display for IpPrefixError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "prefix length {} exceeds maximum {}",
            self.prefix_length, self.maximum
        )
    }
}

impl std::error::Error for IpPrefixError {}

/// Parsing errors for [`IpPrefix`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpPrefixParseError {
    /// The input did not contain a slash and prefix length.
    MissingPrefixLength,
    /// The address portion is not a valid IPv4 or IPv6 address.
    InvalidAddress,
    /// The prefix portion is not an unsigned integer.
    InvalidPrefixLength,
    /// The parsed prefix length is invalid for the address family.
    OutOfRange(IpPrefixError),
}

impl fmt::Display for IpPrefixParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefixLength => formatter.write_str("IP prefix must include /length"),
            Self::InvalidAddress => formatter.write_str("invalid IP address in prefix"),
            Self::InvalidPrefixLength => formatter.write_str("invalid IP prefix length"),
            Self::OutOfRange(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for IpPrefixParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_names_reject_empty_and_control_characters() {
        assert_eq!(InterfaceName::new("  "), Err(InterfaceNameError::Empty));
        assert_eq!(
            InterfaceName::new("eth\n0"),
            Err(InterfaceNameError::ControlCharacter)
        );
        assert_eq!(
            InterfaceName::new("Ethernet 1/1").unwrap().as_str(),
            "Ethernet 1/1"
        );
    }

    #[test]
    fn prefixes_validate_both_address_families() {
        assert_eq!(
            "192.0.2.1/32".parse::<IpPrefix>().unwrap().prefix_length(),
            32
        );
        assert_eq!(
            "2001:db8::1/128"
                .parse::<IpPrefix>()
                .unwrap()
                .prefix_length(),
            128
        );
        assert_eq!(
            "192.0.2.1/33".parse::<IpPrefix>(),
            Err(IpPrefixParseError::OutOfRange(IpPrefixError {
                prefix_length: 33,
                maximum: 32,
            }))
        );
    }
}
