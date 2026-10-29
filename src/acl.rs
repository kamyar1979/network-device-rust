//! Vendor-neutral access lists: named sets of traffic matches.
//!
//! The caller names an access list and fills it; a policy can then be attached
//! to the traffic it matches. How a driver realizes a list (ACLs per address
//! family, classifiers) is the driver's concern.

use crate::IpPrefix;
use std::fmt;
use std::str::FromStr;

/// A validated, vendor-neutral access-list name.
///
/// Names are 1 to 63 ASCII letters, digits, hyphens or underscores. A driver
/// whose device accepts shorter names rejects the operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AccessListName(String);

/// Longest accepted [`AccessListName`].
pub const MAX_ACCESS_LIST_NAME_LENGTH: usize = 63;

impl AccessListName {
    /// Creates a name, rejecting empty, overlong, or non-ASCII-word values.
    pub fn new(value: impl Into<String>) -> Result<Self, AccessListNameError> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_ACCESS_LIST_NAME_LENGTH {
            return Err(AccessListNameError::Length);
        }
        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        {
            return Err(AccessListNameError::Character);
        }
        Ok(Self(value))
    }

    /// Returns the name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AccessListName {
    type Err = AccessListNameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<&str> for AccessListName {
    type Error = AccessListNameError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for AccessListName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Validation errors for [`AccessListName`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessListNameError {
    /// The name is empty or longer than [`MAX_ACCESS_LIST_NAME_LENGTH`].
    Length,
    /// The name contains a character other than ASCII letters, digits, `-` or `_`.
    Character,
}

impl fmt::Display for AccessListNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Length => "access-list name must be 1 to 63 characters",
            Self::Character => {
                "access-list name must contain only ASCII letters, digits, hyphens or underscores"
            }
        })
    }
}

impl std::error::Error for AccessListNameError {}

/// One permit entry: IP traffic from `source` to `destination`; a missing
/// side matches any address of the entry's family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AccessEntry {
    source: Option<IpPrefix>,
    destination: Option<IpPrefix>,
}

impl AccessEntry {
    /// Creates an entry, requiring at least one side and one address family.
    pub fn new(
        source: Option<IpPrefix>,
        destination: Option<IpPrefix>,
    ) -> Result<Self, AccessEntryError> {
        match (source, destination) {
            (None, None) => Err(AccessEntryError::Unbounded),
            (Some(source), Some(destination))
                if source.address().is_ipv4() != destination.address().is_ipv4() =>
            {
                Err(AccessEntryError::AddressFamilyMismatch)
            }
            _ => Ok(Self {
                source,
                destination,
            }),
        }
    }

    /// Matches traffic sent by `source`.
    pub fn from_source(source: IpPrefix) -> Self {
        Self {
            source: Some(source),
            destination: None,
        }
    }

    /// Matches traffic sent to `destination`.
    pub fn to_destination(destination: IpPrefix) -> Self {
        Self {
            source: None,
            destination: Some(destination),
        }
    }

    /// Returns the source prefix, if any.
    pub const fn source(self) -> Option<IpPrefix> {
        self.source
    }

    /// Returns the destination prefix, if any.
    pub const fn destination(self) -> Option<IpPrefix> {
        self.destination
    }

    /// Whether the entry matches IPv4 traffic.
    pub fn is_ipv4(self) -> bool {
        self.source
            .or(self.destination)
            .is_some_and(|prefix| prefix.address().is_ipv4())
    }
}

/// Validation errors for [`AccessEntry`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessEntryError {
    /// Neither a source nor a destination is given.
    Unbounded,
    /// The source and destination use different IP address families.
    AddressFamilyMismatch,
}

impl fmt::Display for AccessEntryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unbounded => "access entry needs a source, a destination, or both",
            Self::AddressFamilyMismatch => {
                "access entry source and destination must use the same address family"
            }
        })
    }
}

impl std::error::Error for AccessEntryError {}

/// A named access list with at least one entry; entries keep their order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AccessList {
    name: AccessListName,
    entries: Vec<AccessEntry>,
}

impl AccessList {
    /// Creates a list, requiring at least one entry.
    pub fn new(name: AccessListName, entries: Vec<AccessEntry>) -> Result<Self, AccessListError> {
        if entries.is_empty() {
            return Err(AccessListError::Empty);
        }
        Ok(Self { name, entries })
    }

    /// Returns the list name.
    pub fn name(&self) -> &AccessListName {
        &self.name
    }

    /// Returns the entries in order.
    pub fn entries(&self) -> &[AccessEntry] {
        &self.entries
    }
}

/// Validation error for [`AccessList`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessListError {
    /// The list has no entries.
    Empty,
}

impl fmt::Display for AccessListError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("access list must have at least one entry")
    }
}

impl std::error::Error for AccessListError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefix(value: &str) -> IpPrefix {
        value.parse().unwrap()
    }

    #[test]
    fn names_are_ascii_words_of_bounded_length() {
        assert!(AccessListName::new("edge-filter-1").is_ok());
        assert_eq!(AccessListName::new(""), Err(AccessListNameError::Length));
        assert_eq!(
            AccessListName::new("svc:1"),
            Err(AccessListNameError::Character)
        );
    }

    #[test]
    fn entries_need_one_side_and_one_family() {
        assert_eq!(
            AccessEntry::new(None, None),
            Err(AccessEntryError::Unbounded)
        );
        assert_eq!(
            AccessEntry::new(
                Some(prefix("192.0.2.20/32")),
                Some(prefix("2001:db8::1/128"))
            ),
            Err(AccessEntryError::AddressFamilyMismatch)
        );
        let entry = AccessEntry::from_source(prefix("192.0.2.20/32"));
        assert!(entry.is_ipv4());
        assert!(!AccessEntry::to_destination(prefix("2001:db8::1/128")).is_ipv4());
    }

    #[test]
    fn a_list_has_at_least_one_entry() {
        let name = AccessListName::new("svc").unwrap();
        assert_eq!(
            AccessList::new(name.clone(), vec![]),
            Err(AccessListError::Empty)
        );
        let list = AccessList::new(
            name,
            vec![
                AccessEntry::from_source(prefix("192.0.2.20/32")),
                AccessEntry::to_destination(prefix("192.0.2.20/32")),
            ],
        )
        .unwrap();
        assert_eq!(list.entries().len(), 2);
    }
}
