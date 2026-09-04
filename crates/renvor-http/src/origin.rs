//! The effective origin: the scheme, host, and port a request was addressed to — and the same
//! triple parsed from an `Origin` header, so the two can be compared **field by field**.
//!
//! # A host comparison is not an origin comparison
//!
//! RFC 6454 §4 defines an origin as the triple *(scheme, host, port)*, and §5 says two origins are
//! the same only when **all three** match. Both places this crate decided "is this request
//! same-origin?" — the CORS carve-out in `route::build` and the CSRF gate in `renvor-auth-http` —
//! compared the **host alone**, with the scheme never read and the port stripped by host
//! validation. So `https://app.example` and `http://app.example` were the same origin, and so
//! were `app.example:8443` and `app.example:443`. A page served over plaintext on the same host,
//! or from another port of it, is a different security principal, and the comparison let it
//! through both gates. Found by review (Phase 010 correction round, finding 1).
//!
//! # Where each field of the request's own origin comes from
//!
//! | Field | Source | Why |
//! |---|---|---|
//! | scheme | the trusted proxy's `proto` when the identity is `ViaTrustedProxy`; otherwise the **configured** public scheme (`HttpServerConfig::public_scheme`, default `http`) | the listener speaks HTTP, so `http` is the listener's own truth; only a peer the operator trusts may say a TLS terminator sat in front of it |
//! | host | the validated `Host` | the value host validation already accepted, so an attacker cannot satisfy an origin check without first satisfying host validation |
//! | port | the validated `Host`'s port, else the scheme's default | RFC 6454 §4 step 5: an absent port is the scheme's default port |
//!
//! A missing, repeated, or unparseable `proto` from a trusted proxy falls back to the **configured**
//! scheme rather than guessing. That is the fail-closed direction: an `https://` `Origin` then
//! fails to match and the request is refused, rather than being admitted on a scheme nothing
//! vouched for. An untrusted peer's `proto` headers are never read at all — the identity module's
//! rule, applied to one more header.
//!
//! # `Origin` is parsed to the same shape, by the same rules
//!
//! RFC 6454 §6.1 serialises an origin as `scheme "://" host [ ":" port ]` and nothing else; §7
//! says the `Origin` header carries that or the literal `null`. [`EffectiveOrigin::parse`] accepts
//! exactly that grammar with `http` or `https` as the scheme, normalises the host by the **same**
//! function host validation uses (so the two sides cannot disagree about case, a trailing dot, or
//! an IPv6 literal), and applies the scheme's default port when none is given. Everything else —
//! `null`, a missing scheme, userinfo, a path, a query, a fragment, whitespace, a control
//! character, a garbage port, more than [`MAX_ORIGIN_BYTES`] — refuses the **whole** value rather
//! than trimming it down to something that might match.

use core::fmt;

use crate::host;
pub use crate::route::MAX_ORIGIN_BYTES;

/// The scheme a request was made under, as far as an origin is concerned.
///
/// A closed enum rather than a string: RFC 6454 §4 folds the scheme to lowercase before comparing,
/// and a string would let `"HTTPS"` and `"https"` be two origins. Only the two schemes a browser
/// can send this server a request under are representable, so an `Origin` naming any other scheme
/// (`ftp://`, `chrome-extension://`) fails to parse rather than being carried as text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scheme {
    /// `http`, default port 80.
    Http,
    /// `https`, default port 443.
    Https,
}

impl Scheme {
    /// Parses a scheme token case-insensitively; anything but `http` or `https` is `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("http") {
            Some(Self::Http)
        } else if value.eq_ignore_ascii_case("https") {
            Some(Self::Https)
        } else {
            None
        }
    }

    /// The port an origin under this scheme has when none is written (RFC 6454 §4 step 5).
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Self::Http => 80,
            Self::Https => 443,
        }
    }

    /// The canonical lowercase spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// An origin as RFC 6454 §4 defines it: scheme, host, and port, all resolved.
///
/// Two values are equal only when all three fields are equal, which is §5's rule. The host is
/// compared byte-for-byte because both constructors normalise it the same way — `parse` through
/// the host module, and the router from the value host validation returned — so case folding has
/// already happened by the time two of these meet.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EffectiveOrigin {
    scheme: Scheme,
    host: String,
    port: u16,
}

impl EffectiveOrigin {
    /// Assembles an origin from parts a caller already validated.
    ///
    /// The router is the one production caller, and it passes the host **host validation
    /// returned** and the port that header carried (or the scheme's default). No normalisation
    /// happens here: a host that has not been through `HostPolicy::validate` is not a value the
    /// router would ever build one of these from, and normalising twice would hide a caller that
    /// skipped validation.
    #[must_use]
    pub fn new(scheme: Scheme, host: impl Into<String>, port: u16) -> Self {
        Self {
            scheme,
            host: host.into(),
            port,
        }
    }

    /// Parses a serialised origin (RFC 6454 §6.1) into the resolved triple, or refuses it.
    ///
    /// Accepted: `scheme "://" host [ ":" port ]` with the scheme `http` or `https` in any case,
    /// the host normalised exactly as an inbound `Host` is (ASCII only, lowercase, trailing dots
    /// removed, a bracketed IPv6 literal kept with its brackets, an unbracketed one refused), and
    /// an explicit port that is a valid non-zero `u16`. An absent port is the scheme's default.
    ///
    /// **Fails closed** on everything else: `null`, a missing scheme, userinfo (`@`), a path,
    /// query, or fragment, a backslash (a path separator to every URL parser that treats `\` as
    /// `/`), whitespace, a control character, a non-ASCII byte, a port that is not a valid
    /// non-zero `u16`, an empty value, or a value over [`MAX_ORIGIN_BYTES`]. A garbage port
    /// refuses the whole value rather than being stripped — a value we cannot fully parse is a
    /// value we do not understand.
    #[must_use]
    pub fn parse(origin: &str) -> Option<Self> {
        // BOUNDED FIRST, before a single byte is inspected.
        if origin.is_empty() || origin.len() > MAX_ORIGIN_BYTES {
            return None;
        }

        // Whitespace, a control character, or a non-ASCII byte anywhere refuses the value. A
        // serialised origin contains none of them, and trimming would turn a value the sender did
        // not serialise into one that matches.
        if origin
            .bytes()
            .any(|byte| !byte.is_ascii() || byte.is_ascii_control() || byte == b' ')
        {
            return None;
        }

        // `null` has no `://`, and neither does a bare host. Both land here.
        let (scheme, authority) = origin.split_once("://")?;
        let scheme = Scheme::parse(scheme)?;

        // Userinfo, a path, a query, a fragment — and a backslash, which the WHATWG URL parser
        // treats as `/` for these schemes. None is part of an origin; their presence means the
        // value is a URL, and a URL is refused rather than reduced to the origin it contains.
        if authority.is_empty()
            || authority
                .bytes()
                .any(|byte| matches!(byte, b'@' | b'/' | b'\\' | b'?' | b'#'))
        {
            return None;
        }

        // THE SAME NORMALISER HOST VALIDATION USES. Case, the trailing dot, the bracketed IPv6
        // literal, the unbracketed one, and the port rules are decided in one place for both
        // sides of the comparison.
        let (host, port) = host::normalise(authority)?;

        Some(Self {
            scheme,
            host,
            port: port.unwrap_or_else(|| scheme.default_port()),
        })
    }

    /// The scheme.
    #[must_use]
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The normalised host, brackets included for an IPv6 literal.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The resolved port: the explicit one, or the scheme's default.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

/// Renders the triple, and nothing an author did not already admit through host validation for
/// the request's own origin. For a parsed `Origin` the host is the caller's — bounded, ASCII, and
/// control-free by construction, and already carried in full by `FetchMetadata`'s raw accessor.
impl fmt::Debug for EffectiveOrigin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EffectiveOrigin")
            .field("scheme", &self.scheme.as_str())
            .field("host", &self.host)
            .field("port", &self.port)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{EffectiveOrigin, MAX_ORIGIN_BYTES, Scheme};

    fn origin(scheme: Scheme, host: &str, port: u16) -> EffectiveOrigin {
        EffectiveOrigin::new(scheme, host, port)
    }

    #[test]
    fn the_scheme_parses_case_insensitively_and_nothing_else_parses() {
        assert_eq!(Scheme::parse("http"), Some(Scheme::Http));
        assert_eq!(Scheme::parse("HTTPS"), Some(Scheme::Https));
        assert_eq!(Scheme::parse("Http"), Some(Scheme::Http));
        for other in ["ftp", "ws", "wss", "", " http", "http ", "https:"] {
            assert_eq!(Scheme::parse(other), None, "`{other}` parsed as a scheme");
        }
        assert_eq!(Scheme::Http.default_port(), 80);
        assert_eq!(Scheme::Https.default_port(), 443);
        assert_eq!(Scheme::Http.as_str(), "http");
        assert_eq!(Scheme::Https.as_str(), "https");
    }

    #[test]
    fn an_absent_port_is_the_schemes_default() {
        // RFC 6454 §4 step 5. Without this, `https://a.example` and `https://a.example:443` — the
        // same origin to every browser — would be two origins here, and a same-origin request
        // whose `Host` carried an explicit `:443` would be refused.
        assert_eq!(
            EffectiveOrigin::parse("https://a.example"),
            Some(origin(Scheme::Https, "a.example", 443))
        );
        assert_eq!(
            EffectiveOrigin::parse("https://a.example"),
            EffectiveOrigin::parse("https://a.example:443")
        );
        assert_eq!(
            EffectiveOrigin::parse("http://a.example"),
            Some(origin(Scheme::Http, "a.example", 80))
        );
        assert_eq!(
            EffectiveOrigin::parse("http://a.example"),
            EffectiveOrigin::parse("http://a.example:80")
        );
    }

    #[test]
    fn an_explicit_port_is_kept() {
        assert_eq!(
            EffectiveOrigin::parse("https://a.example:8443"),
            Some(origin(Scheme::Https, "a.example", 8443))
        );
        assert_eq!(
            EffectiveOrigin::parse("http://127.0.0.1:8080"),
            Some(origin(Scheme::Http, "127.0.0.1", 8080))
        );
    }

    #[test]
    fn an_ipv6_literal_keeps_its_brackets_and_its_port() {
        assert_eq!(
            EffectiveOrigin::parse("http://[::1]:8080"),
            Some(origin(Scheme::Http, "[::1]", 8080))
        );
        assert_eq!(
            EffectiveOrigin::parse("http://[::1]"),
            Some(origin(Scheme::Http, "[::1]", 80))
        );
        assert_eq!(
            EffectiveOrigin::parse("https://[2001:DB8::1]:8443"),
            Some(origin(Scheme::Https, "[2001:db8::1]", 8443))
        );
    }

    #[test]
    fn scheme_and_host_are_normalised_the_way_host_validation_normalises_them() {
        // Case and the trailing dot: the same rules `HostPolicy::validate` applies, so the two
        // sides of an origin comparison cannot disagree about either.
        assert_eq!(
            EffectiveOrigin::parse("HTTPS://A.EXAMPLE."),
            EffectiveOrigin::parse("https://a.example")
        );
        assert_eq!(
            EffectiveOrigin::parse("HTTPS://A.EXAMPLE.")
                .as_ref()
                .map(EffectiveOrigin::host),
            Some("a.example")
        );
    }

    #[test]
    fn a_different_scheme_is_a_different_origin() {
        // THE DEFECT. `http://a.example` and `https://a.example` share a host, and the host is all
        // the old comparison read.
        assert_ne!(
            EffectiveOrigin::parse("http://a.example"),
            EffectiveOrigin::parse("https://a.example")
        );
        // Both parse — the inequality above is about the scheme, not about one side refusing.
        assert!(EffectiveOrigin::parse("http://a.example").is_some());
        assert!(EffectiveOrigin::parse("https://a.example").is_some());
    }

    #[test]
    fn the_same_host_on_a_different_port_is_a_different_origin() {
        // THE OTHER HALF OF THE DEFECT. The port was stripped before comparison.
        assert_ne!(
            EffectiveOrigin::parse("https://a.example:8443"),
            EffectiveOrigin::parse("https://a.example")
        );
        assert!(EffectiveOrigin::parse("https://a.example:8443").is_some());
    }

    #[test]
    fn every_non_origin_form_is_refused_whole() {
        for hostile in [
            "null",                     // RFC 6454 §7.1: the opaque origin, which matches nothing
            "a.example",                // no scheme
            "//a.example",              // scheme-relative
            "https://a.example/x",      // a path
            "https://a.example/",       // even an empty path is not part of a serialised origin
            "https://a.example?x=1",    // a query
            "https://a.example#x",      // a fragment
            "https://u@a.example",      // userinfo
            "https://u:p@a.example",    // userinfo with a password
            "https://a.example\\x",     // a backslash, a path separator to some parsers
            "https://a.example:0",      // port 0
            "https://a.example:x",      // a garbage port
            "https://a.example:",       // an empty port
            "https://a.example:65536",  // over u16
            "https://a.example:+80",    // a sign `u16::from_str` would otherwise accept
            "https://2001:db8::1",      // an unbracketed IPv6 literal
            "https://[2001:db8::1",     // an unclosed bracket
            "https://[2001:db8::1]x",   // junk after the bracket
            "https://",                 // no host
            "https:// a.example",       // whitespace
            "https://a.example ",       // trailing whitespace
            " https://a.example",       // leading whitespace
            "https://a.exam\u{7}ple",   // a control character
            "https://a.exampl\u{142}e", // a non-ASCII homograph
            "ftp://a.example",          // a scheme no browser sends this server a request under
            "",                         // empty
        ] {
            assert_eq!(
                EffectiveOrigin::parse(hostile),
                None,
                "`{hostile}` parsed as an origin"
            );
        }

        // POSITIVE CONTROL: a legitimate origin parses, so the refusals are about the inputs
        // rather than about a parser that refuses everything.
        assert!(EffectiveOrigin::parse("https://a.example").is_some());
    }

    #[test]
    fn a_value_over_the_bound_is_refused_rather_than_truncated() {
        let long = format!("https://{}.example", "a".repeat(MAX_ORIGIN_BYTES));
        assert!(long.len() > MAX_ORIGIN_BYTES);
        assert_eq!(EffectiveOrigin::parse(&long), None);

        // POSITIVE CONTROL: a value AT the bound is not refused for its length.
        let prefix = "https://";
        let suffix = ".example";
        let label = "a".repeat(MAX_ORIGIN_BYTES - prefix.len() - suffix.len());
        let at_bound = format!("{prefix}{label}{suffix}");
        assert_eq!(at_bound.len(), MAX_ORIGIN_BYTES);
        assert!(EffectiveOrigin::parse(&at_bound).is_some());
    }

    #[test]
    fn debug_renders_the_triple() {
        let rendered = format!("{:?}", origin(Scheme::Https, "a.example", 8443));
        assert!(rendered.contains("https"), "{rendered}");
        assert!(rendered.contains("a.example"), "{rendered}");
        assert!(rendered.contains("8443"), "{rendered}");
    }
}
