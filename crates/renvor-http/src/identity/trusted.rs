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
    ///
    /// The address is canonicalised, so an IPv4-mapped IPv6 form and its plain IPv4 form are the
    /// same peer. See [`TrustedProxies::is_trusted`] for why that matters.
    #[must_use]
    pub fn trust(mut self, peer: IpAddr) -> Self {
        self.peers.insert(peer.to_canonical());
        self
    }

    /// Whether `peer` is trusted.
    ///
    /// The trust decision is made about the **direct peer** — the socket Renvor is actually talking
    /// to — and never about an address named inside a header. An address that claims to be a proxy
    /// is claiming it in the very data whose trustworthiness is in question.
    /// # Why both sides are canonicalised
    ///
    /// A listener bound to `[::]` accepts IPv4 connections and reports their peers as
    /// **IPv4-mapped IPv6** — `::ffff:10.0.0.1`. A set built from `trust(10.0.0.1)` is a set of
    /// `IpAddr::V4`, and `BTreeSet::contains` is variant-sensitive, so the lookup returned `false`
    /// and **the trusted-proxy feature was silently inoperative in a standard dual-stack
    /// deployment** — with nothing anywhere saying why.
    ///
    /// It failed closed, which is the safe direction. But the natural operator response to "my
    /// proxy is not being trusted" is to paste in whatever form the logs show, and that is how a
    /// silent misconfiguration becomes a deliberate one.
    #[must_use]
    pub fn is_trusted(&self, peer: IpAddr) -> bool {
        self.peers.contains(&peer.to_canonical())
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
    fn an_ipv4_mapped_peer_matches_its_plain_ipv4_configuration() {
        // The defect this guards: on a `[::]` listener an IPv4 peer arrives as `::ffff:10.0.0.1`,
        // and a set built from `trust(10.0.0.1)` never matched it.
        let configured: IpAddr = "10.0.0.1".parse().expect("v4");
        let observed: IpAddr = "::ffff:10.0.0.1".parse().expect("mapped");
        assert_ne!(
            configured, observed,
            "the two forms are genuinely different values"
        );

        let trusted = TrustedProxies::none().trust(configured);
        assert!(
            trusted.is_trusted(observed),
            "an IPv4-mapped peer did not match its plain IPv4 configuration"
        );

        // And the reverse, so neither direction depends on which form was configured.
        let trusted = TrustedProxies::none().trust(observed);
        assert!(trusted.is_trusted(configured));

        // POSITIVE CONTROL: a DIFFERENT address still does not match in either form.
        assert!(!trusted.is_trusted("10.0.0.2".parse().expect("v4")));
        assert!(!trusted.is_trusted("::ffff:10.0.0.2".parse().expect("mapped")));
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
