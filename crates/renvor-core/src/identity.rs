//! Who the runtime believes is asking — the value, not the decision that produced it.
//!
//! # Why this type is in the kernel and its classification is not
//!
//! [`ClientIdentity`] began in `renvor-http`, next to the code that decides it. That is still where
//! the **decision** lives: `renvor_http::identity::resolve`, `TrustedProxies`, and the `Forwarded` /
//! `X-Forwarded-For` parsers have not moved and will not. They are transport facts and they belong
//! to the transport.
//!
//! The *value* is not a transport fact. It is an `IpAddr` and a note about how it was learned, and
//! by Phase 009 two crates need it: the transport that resolves it, and `renvor-auth`, whose abuse
//! controls count a network dimension.
//!
//! `renvor-auth` must not depend on `renvor-http` — the crate DAG gate enforces that, and it should:
//! an authentication rule that named a protocol would be a fact about HTTP. So the alternatives were
//! to duplicate the type in `renvor-auth`, or to move the value down to the crate both already
//! depend on. **Duplicating it would mean two types that must agree and no compiler check that they
//! do**, and the first thing that happens to such a pair is that one of them grows a constructor
//! from a header.
//!
//! What crossed the line is therefore exactly one enum over `IpAddr`. No parser, no trust
//! configuration, no header name, and nothing that could grow into one — `renvor-core` still
//! resolves no HTTP crate under any feature, which verification step 7 asserts with a positive
//! control.
//!
//! # This type is not a capability
//!
//! Both variants are public and constructible. Holding a `ClientIdentity` is **not** evidence that
//! anything was trusted, and code that treats it as evidence has misread it. The only thing that
//! establishes trust is having called `renvor_http::identity::resolve` with a real socket address
//! and a real [`TrustedProxies`](https://docs.rs/renvor-http) configuration.
//!
//! Making it a capability was considered and rejected: a private constructor here would mean the
//! kernel deciding who may name an address, and every test in every crate would need a factory to
//! get one. The honest position is that this is a *value*, the resolver is the *authority*, and the
//! two are not confused because the resolver is the only thing that reads a header.

use core::fmt;
use std::net::IpAddr;

/// Who Renvor believes is asking.
///
/// The variants are deliberately distinguishable: an operator reading a log can tell whether an
/// address was observed directly or was accepted from a trusted proxy, which is exactly the
/// question an incident asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClientIdentity {
    /// The address of the socket Renvor is talking to. Always a fact.
    DirectPeer(IpAddr),
    /// An address taken from a forwarding header, because the direct peer was explicitly trusted.
    ViaTrustedProxy {
        /// The address the trusted proxy reported.
        client: IpAddr,
        /// The trusted peer that reported it.
        proxy: IpAddr,
    },
}

impl ClientIdentity {
    /// The address to attribute the request to.
    #[must_use]
    pub const fn address(self) -> IpAddr {
        match self {
            Self::DirectPeer(address) => address,
            Self::ViaTrustedProxy { client, .. } => client,
        }
    }

    /// Whether this identity came from a forwarding header.
    #[must_use]
    pub const fn is_forwarded(self) -> bool {
        matches!(self, Self::ViaTrustedProxy { .. })
    }
}

impl fmt::Display for ClientIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DirectPeer(address) => write!(f, "{address}"),
            Self::ViaTrustedProxy { client, proxy } => write!(f, "{client} (via {proxy})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ClientIdentity;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn direct_peer_identity_is_not_marked_forwarded() {
        let direct = ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)));
        assert!(!direct.is_forwarded());
        assert_eq!(direct.address(), IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)));

        // POSITIVE CONTROL: the forwarded variant IS marked, and reports the client rather than
        // the proxy — reporting the proxy would attribute every request to the load balancer.
        let forwarded = ClientIdentity::ViaTrustedProxy {
            client: IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7)),
            proxy: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        };
        assert!(forwarded.is_forwarded());
        assert_eq!(
            forwarded.address(),
            IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7))
        );
    }

    #[test]
    fn the_proxy_is_named_in_the_rendering_and_the_direct_peer_is_not() {
        // An operator reading a log line needs to know an address was *claimed*, not observed.
        let forwarded = ClientIdentity::ViaTrustedProxy {
            client: IpAddr::V4(Ipv4Addr::new(198, 51, 100, 7)),
            proxy: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        };
        assert_eq!(forwarded.to_string(), "198.51.100.7 (via 10.0.0.1)");

        // POSITIVE CONTROL: the direct form carries no parenthetical, so the one above is about
        // the variant rather than about `Display` always appending something.
        let direct = ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1)));
        assert_eq!(direct.to_string(), "203.0.113.1");
    }
}
