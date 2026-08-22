//! The trusted-proxy set. **Empty by default.**
//!
//! # Why empty rather than "the usual private ranges"
//!
//! A private-range default is wrong precisely when it matters. A server reachable from the internet
//! behind no proxy would honour forwarding headers from anyone able to reach it from such a range —
//! and in a shared network, a container host, or a cloud VPC, that is a large and untrusted set.
//!
//! Empty means an operator who has not described their topology gets the only answer that is always
//! true: the address Renvor observed.

use std::collections::BTreeSet;
use std::net::IpAddr;

/// The peers whose forwarding headers Renvor will honour.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrustedProxies {
    peers: BTreeSet<IpAddr>,
}

impl TrustedProxies {
    /// Trusts nobody. The default.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Trusts one peer address.
    #[must_use]
    pub fn trust(mut self, peer: IpAddr) -> Self {
        self.peers.insert(peer);
        self
    }

    /// Whether `peer` is trusted.
    ///
    /// The trust decision is made about the **direct peer** — the socket Renvor is actually talking
    /// to — and never about an address named inside a header. An address that claims to be a proxy
    /// is claiming it in the very data whose trustworthiness is in question.
    #[must_use]
    pub fn is_trusted(&self, peer: IpAddr) -> bool {
        self.peers.contains(&peer)
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::TrustedProxies;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn the_default_trusts_nobody() {
        let trusted = TrustedProxies::default();
        assert!(trusted.is_empty());
        assert!(!trusted.is_trusted(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        // Not even a private range, which a "sensible default" would have included.
        assert!(!trusted.is_trusted(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(!trusted.is_trusted(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))));
    }

    #[test]
    fn an_explicitly_trusted_peer_is_trusted_and_others_are_not() {
        let proxy = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let trusted = TrustedProxies::none().trust(proxy);

        assert!(trusted.is_trusted(proxy));
        // POSITIVE CONTROL: a neighbouring address in the same private range is NOT trusted, so
        // trust is per-address rather than per-range-by-accident.
        assert!(!trusted.is_trusted(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2))));
    }
}
