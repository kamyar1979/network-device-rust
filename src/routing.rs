//! Vendor-neutral static-route values.

use crate::IpPrefix;
use std::fmt;
use std::net::IpAddr;

/// A static route's destination and next-hop address.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StaticRoute {
    /// Destination network.
    destination: IpPrefix,
    /// IP address of the next hop.
    next_hop: IpAddr,
}

impl StaticRoute {
    /// Creates a route, requiring destination and next hop to use the same IP family.
    pub fn new(destination: IpPrefix, next_hop: IpAddr) -> Result<Self, StaticRouteError> {
        if destination.address().is_ipv4() != next_hop.is_ipv4() {
            return Err(StaticRouteError::AddressFamilyMismatch);
        }
        Ok(Self {
            destination,
            next_hop,
        })
    }

    /// Returns the route destination.
    pub const fn destination(self) -> IpPrefix {
        self.destination
    }

    /// Returns the route's next-hop address.
    pub const fn next_hop(self) -> IpAddr {
        self.next_hop
    }
}

/// Validation error for [`StaticRoute`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StaticRouteError {
    /// The destination and next hop use different IP address families.
    AddressFamilyMismatch,
}

impl fmt::Display for StaticRouteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .write_str("static-route destination and next hop must use the same address family")
    }
}

impl std::error::Error for StaticRouteError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn route_rejects_a_next_hop_from_another_address_family() {
        let destination = "192.0.2.0/24".parse::<IpPrefix>().unwrap();
        assert_eq!(
            StaticRoute::new(destination, IpAddr::V6(Ipv6Addr::LOCALHOST)),
            Err(StaticRouteError::AddressFamilyMismatch)
        );
        assert!(StaticRoute::new(destination, IpAddr::V4(Ipv4Addr::LOCALHOST)).is_ok());
    }
}
