//! The request context: what the security layers resolved, handed to a handler as facts.
//!
//! # Why a handler never sees a header
//!
//! Client identity, host, and request identity are **resolved once**, by layers that know the
//! trust configuration, and handed onward as settled values. A handler that could read
//! `X-Forwarded-For` itself would be one line away from re-deriving identity without the trust
//! check — and that line would look perfectly reasonable in review.
//!
//! Removing the header from the handler's reach removes the mistake from the codebase, rather than
//! from the coding standard.

use core::fmt;
use std::net::IpAddr;

use renvor_core::{CancelScope, RunIdentifier};

/// Who Renvor believes is asking.
///
/// The variants are deliberately distinguishable: an operator reading a log can tell whether an
/// address was observed directly or was accepted from a trusted proxy, which is exactly the
/// question an incident asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// An opaque per-request correlation identifier.
///
/// Nested beneath the run identifier (contract C-O3) and generated from the same entropy
/// discipline as it (C-O4): it encodes no hostname, timestamp, process identifier, counter, or
/// configuration value.
///
/// **A caller-supplied value never becomes one of these.** There is no constructor that takes an
/// inbound header, which is what makes that a property of the type rather than a rule someone must
/// remember.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RequestId {
    bytes: [u8; 8],
}

impl RequestId {
    /// Builds an identifier from supplied entropy bytes and **nothing else**.
    ///
    /// The single construction site, for the same reason `RunIdentifier` has one: reviewing
    /// opacity means reviewing one function.
    #[must_use]
    pub const fn from_entropy(bytes: [u8; 8]) -> Self {
        Self { bytes }
    }

    /// The encoded form: 16 lowercase hex characters.
    ///
    /// Lowercase hex has one canonical form — no padding, no alphabet variants, no case ambiguity
    /// — so the rendering is a property of the bytes rather than of an encoder setting.
    #[must_use]
    pub fn encode(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut out = String::with_capacity(16);
        for byte in self.bytes {
            out.push(DIGITS[usize::from(byte >> 4)] as char);
            out.push(DIGITS[usize::from(byte & 0x0f)] as char);
        }
        out
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl fmt::Debug for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RequestId({})", self.encode())
    }
}

/// Everything the security layers resolved about one request.
///
/// Carries **no** HTTP type. This is the value that crosses into application code, and it is why
/// cancellation can reach an application service without `axum` following it.
#[derive(Clone)]
pub struct RequestContext {
    run: RunIdentifier,
    request: RequestId,
    client: ClientIdentity,
    host: String,
    cancel: CancelScope,
}

impl RequestContext {
    /// Assembles a context. Built by the transport layers, and by tests.
    #[must_use]
    pub fn new(
        run: RunIdentifier,
        request: RequestId,
        client: ClientIdentity,
        host: impl Into<String>,
        cancel: CancelScope,
    ) -> Self {
        Self {
            run,
            request,
            client,
            host: host.into(),
            cancel,
        }
    }

    /// The identifier of the application run this request belongs to.
    #[must_use]
    pub const fn run_id(&self) -> RunIdentifier {
        self.run
    }

    /// This request's opaque identifier.
    #[must_use]
    pub const fn request_id(&self) -> RequestId {
        self.request
    }

    /// Who Renvor believes is asking.
    #[must_use]
    pub const fn client(&self) -> ClientIdentity {
        self.client
    }

    /// The validated host this request was addressed to.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// This request's cancellation scope.
    ///
    /// A **child** of the application scope: application shutdown cancels every in-flight request,
    /// and one request's cancellation cancels no other. Client disconnect and request timeout both
    /// cancel this scope, so an application service watches one thing rather than two.
    #[must_use]
    pub const fn cancel_scope(&self) -> &CancelScope {
        &self.cancel
    }

    /// Resolves when this request is cancelled.
    pub async fn cancelled(&self) {
        self.cancel.cancelled().await;
    }

    /// Whether this request has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestContext")
            .field("run", &self.run)
            .field("request", &self.request)
            .field("client", &self.client)
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{ClientIdentity, RequestContext, RequestId};
    use renvor_core::{CancelScope, OsEntropy, RunIdentifier};
    use std::net::{IpAddr, Ipv4Addr};

    fn context() -> RequestContext {
        RequestContext::new(
            RunIdentifier::generate(&OsEntropy).expect("entropy"),
            RequestId::from_entropy([1, 2, 3, 4, 5, 6, 7, 8]),
            ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            "example.test",
            CancelScope::root().child("request"),
        )
    }

    #[test]
    fn a_request_id_is_a_pure_function_of_its_bytes() {
        let a = RequestId::from_entropy([0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 1]);
        let b = RequestId::from_entropy([0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 1]);
        assert_eq!(a.encode(), b.encode());
        assert_eq!(a.encode(), "deadbeef00000001");
        assert_eq!(a.encode().len(), 16);

        // POSITIVE CONTROL: different bytes render differently, so the equality above is about the
        // inputs rather than about a constant string.
        let c = RequestId::from_entropy([0xde, 0xad, 0xbe, 0xef, 0, 0, 0, 2]);
        assert_ne!(a.encode(), c.encode());
    }

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

    #[tokio::test]
    async fn cancellation_reaches_the_context_and_is_scoped() {
        let context = context();
        assert!(!context.is_cancelled());

        context.cancel_scope().cancel();
        assert!(context.is_cancelled());
        // Already-cancelled scopes resolve immediately rather than waiting for an event that has
        // already happened.
        context.cancelled().await;
    }

    #[tokio::test]
    async fn one_requests_cancellation_does_not_cancel_another() {
        let application = CancelScope::root();
        let first = application.child("request:1");
        let second = application.child("request:2");

        first.cancel();
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled(), "a sibling was cancelled");

        // POSITIVE CONTROL: cancelling the PARENT does reach the sibling, so the isolation above
        // is asymmetry rather than a scope that propagates nothing.
        application.cancel();
        assert!(second.is_cancelled());
    }
}
