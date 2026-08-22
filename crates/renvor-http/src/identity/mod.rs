//! Resolving who is asking.
//!
//! # The order of the two questions is the whole design
//!
//! 1. **Is the direct peer trusted?** — decided from the socket, which a caller cannot influence.
//! 2. Only then: **what does the forwarding header say?**
//!
//! Reversing them, or skipping the first, is what every "spoofed client IP" report reduces to. The
//! function below cannot skip it: the trusted check happens before the header is read at all, so
//! there is no path through this code that consults a header for an untrusted peer.

pub mod forwarded;
pub mod trusted;

use std::net::IpAddr;

pub use trusted::TrustedProxies;

use crate::context::ClientIdentity;

/// What a request carried that could bear on identity.
///
/// A slice per header rather than a single value, so a **repeated** header is visible here and can
/// be refused, rather than being collapsed before this code ever sees it.
#[derive(Clone, Copy, Debug, Default)]
pub struct ForwardingHeaders<'a> {
    /// Every `Forwarded` header value, in order.
    pub forwarded: &'a [&'a str],
    /// Every `X-Forwarded-For` header value, in order.
    pub x_forwarded_for: &'a [&'a str],
}

/// Resolves the client identity for one request.
///
/// `peer` is the address of the socket. It is always a fact, and it is the answer whenever
/// anything else is uncertain.
///
/// # Fail-closed
///
/// Returns [`ClientIdentity::DirectPeer`] when the peer is untrusted, when no forwarding header is
/// present, when a header is repeated, or when a header cannot be parsed unambiguously. The
/// supplied value is discarded, never attributed.
#[must_use]
pub fn resolve(
    peer: IpAddr,
    trusted: &TrustedProxies,
    headers: ForwardingHeaders<'_>,
) -> ClientIdentity {
    // QUESTION 1. Asked first, and asked about the socket. Everything below this line is
    // unreachable for an untrusted peer, which is the property that makes spoofing impossible by
    // default rather than merely difficult.
    if !trusted.is_trusted(peer) {
        return ClientIdentity::DirectPeer(peer);
    }

    // QUESTION 2. `Forwarded` is the standard header and wins where both are present; falling back
    // to the legacy one only when the standard one is absent avoids having to reconcile two
    // possibly-disagreeing sources.
    // PRESENCE decides which header is consulted; PARSING decides the answer.
    //
    // The earlier form was `from_forwarded(...).or_else(|| from_x_forwarded_for(...))`, and that
    // `or_else` fired on a parse **failure** as well as on absence — so a trusted proxy sending a
    // malformed `Forwarded` alongside a valid `X-Forwarded-For` had the legacy value accepted. The
    // contract requires any malformed forwarding value to resolve to the direct peer, and the
    // comment right here claimed the fallback was for absence only. Found by review; the two now
    // agree.
    let claimed = match forwarded::single_value(headers.forwarded) {
        // The standard header is PRESENT: it decides, and a value it cannot parse fails closed
        // rather than deferring to the legacy header.
        Some(value) => forwarded::from_forwarded(value),
        // Absent: only then does the legacy header get a say.
        None => forwarded::single_value(headers.x_forwarded_for)
            .and_then(forwarded::from_x_forwarded_for),
    };

    match claimed {
        Some(client) => ClientIdentity::ViaTrustedProxy {
            client,
            proxy: peer,
        },
        // A trusted proxy that sent nothing parseable still yields a truthful answer.
        None => ClientIdentity::DirectPeer(peer),
    }
}

#[cfg(test)]
mod tests {
    use super::{ForwardingHeaders, TrustedProxies, resolve};
    use crate::context::ClientIdentity;
    use std::net::{IpAddr, Ipv4Addr};

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, b, c, d))
    }

    #[test]
    fn under_the_default_configuration_hostile_headers_cannot_forge_identity() {
        // SC-004. This is the headline security property of the phase.
        let peer = v4(203, 0, 113, 9);
        let trusted = TrustedProxies::default();

        let forwarded = ["for=1.2.3.4;host=evil.example"];
        let xff = ["1.2.3.4"];

        let identity = resolve(
            peer,
            &trusted,
            ForwardingHeaders {
                forwarded: &forwarded,
                x_forwarded_for: &xff,
            },
        );

        assert_eq!(identity, ClientIdentity::DirectPeer(peer));
        assert_ne!(identity.address(), v4(1, 2, 3, 4));
        assert!(!identity.is_forwarded());
    }

    #[test]
    fn a_trusted_proxys_header_is_honoured() {
        // POSITIVE CONTROL for the test above: without this, an implementation that ignored every
        // header always would pass, and the trusted path would be unproven.
        let proxy = v4(10, 0, 0, 1);
        let trusted = TrustedProxies::none().trust(proxy);
        let xff = ["198.51.100.7"];

        let identity = resolve(
            proxy,
            &trusted,
            ForwardingHeaders {
                forwarded: &[],
                x_forwarded_for: &xff,
            },
        );

        assert_eq!(
            identity,
            ClientIdentity::ViaTrustedProxy {
                client: v4(198, 51, 100, 7),
                proxy,
            }
        );
    }

    #[test]
    fn a_peer_outside_the_trusted_set_is_ignored_even_when_the_set_is_non_empty() {
        let trusted = TrustedProxies::none().trust(v4(10, 0, 0, 1));
        let attacker = v4(203, 0, 113, 9);
        let xff = ["198.51.100.7"];

        let identity = resolve(
            attacker,
            &trusted,
            ForwardingHeaders {
                forwarded: &[],
                x_forwarded_for: &xff,
            },
        );

        assert_eq!(identity, ClientIdentity::DirectPeer(attacker));
    }

    #[test]
    fn a_trusted_proxy_sending_an_unparseable_value_falls_back_to_the_peer() {
        let proxy = v4(10, 0, 0, 1);
        let trusted = TrustedProxies::none().trust(proxy);

        for hostile in ["not-an-address", "", "for=unknown", "1.2.3.4\r\nX: y"] {
            let values = [hostile];
            let identity = resolve(
                proxy,
                &trusted,
                ForwardingHeaders {
                    forwarded: &values,
                    x_forwarded_for: &values,
                },
            );
            assert_eq!(
                identity,
                ClientIdentity::DirectPeer(proxy),
                "`{hostile}` was attributed"
            );
        }
    }

    #[test]
    fn a_repeated_forwarding_header_is_refused() {
        let proxy = v4(10, 0, 0, 1);
        let trusted = TrustedProxies::none().trust(proxy);
        let doubled = ["198.51.100.7", "1.2.3.4"];

        let identity = resolve(
            proxy,
            &trusted,
            ForwardingHeaders {
                forwarded: &[],
                x_forwarded_for: &doubled,
            },
        );

        // Neither value is chosen. Choosing either would make Renvor's answer depend on a
        // header-ordering rule that the hop in front of it may resolve differently.
        assert_eq!(identity, ClientIdentity::DirectPeer(proxy));
    }

    #[test]
    fn a_malformed_forwarded_does_not_fall_through_to_x_forwarded_for() {
        // The defect this replaced: `or_else` fired on a parse failure, so a trusted proxy sending
        // a broken standard header had its legacy header accepted instead of failing closed.
        let proxy = v4(10, 0, 0, 1);
        let trusted = TrustedProxies::none().trust(proxy);
        let malformed = ["for=not-an-address"];
        let valid_legacy = ["198.51.100.7"];

        let identity = resolve(
            proxy,
            &trusted,
            ForwardingHeaders {
                forwarded: &malformed,
                x_forwarded_for: &valid_legacy,
            },
        );

        assert_eq!(
            identity,
            ClientIdentity::DirectPeer(proxy),
            "a malformed `Forwarded` fell through to `X-Forwarded-For`"
        );

        // POSITIVE CONTROL: with the standard header ABSENT, the legacy one IS consulted — so the
        // refusal above is about the parse failure rather than about the legacy header being dead.
        let identity = resolve(
            proxy,
            &trusted,
            ForwardingHeaders {
                forwarded: &[],
                x_forwarded_for: &valid_legacy,
            },
        );
        assert_eq!(identity.address(), v4(198, 51, 100, 7));
    }

    #[test]
    fn forwarded_wins_over_x_forwarded_for_when_both_are_present() {
        let proxy = v4(10, 0, 0, 1);
        let trusted = TrustedProxies::none().trust(proxy);
        let forwarded = ["for=198.51.100.7"];
        let xff = ["1.2.3.4"];

        let identity = resolve(
            proxy,
            &trusted,
            ForwardingHeaders {
                forwarded: &forwarded,
                x_forwarded_for: &xff,
            },
        );

        assert_eq!(identity.address(), v4(198, 51, 100, 7));
    }
}
