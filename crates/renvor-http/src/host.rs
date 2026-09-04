//! Fail-closed `Host` validation.
//!
//! # Why this is Renvor's own
//!
//! The selected middleware collection ships **no** host validator. Absence upstream is not an
//! argument that the control is unnecessary — it is why this module exists. ADR-0012, Finding 5.
//!
//! # There is no "allow any host" default
//!
//! An application that has not said which hosts are its own has not been configured, and guessing
//! on its behalf is how a host-header attack succeeds: an attacker sends `Host: evil.example`, the
//! application builds a password-reset link from it, and the link points at the attacker.
//!
//! So [`HostPolicy::default`] allows **nothing**, and every request is refused until an author
//! says otherwise. A configuration mistake produces a refusal, which is loud; the alternative
//! produces a working application with a latent redirect, which is silent.

use core::fmt;
use std::collections::BTreeSet;

/// A host that could not be used in a policy.
///
/// # Why this is an error rather than a silent drop
///
/// `allow` used to ignore a host it could not normalise. The result was fail-closed and therefore
/// safe — but an operator who typed `exampłe.com` with a Cyrillic character got a policy that
/// refused **every** request, with nothing anywhere saying why. Safe and unexplained is still a
/// support incident, and constitution principle IV prohibits absorbing a failure silently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidHost {
    /// The value that could not be used.
    pub host: String,
}

impl fmt::Display for InvalidHost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "`{}` cannot be used as an allowed host: it is empty, contains a control character, \
             is not ASCII, or is an ambiguous address form",
            self.host
        )
    }
}

impl core::error::Error for InvalidHost {}

/// The set of hosts an application answers for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostPolicy {
    allowed: BTreeSet<String>,
}

impl HostPolicy {
    /// A policy that allows **no** host. Every request is refused.
    #[must_use]
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Allows one host.
    ///
    /// The value is normalised the same way an inbound header is, so `Example.COM` configured and
    /// `example.com` received are the same host. Normalising only one side is how a policy that
    /// looks correct rejects every real request.
    ///
    /// # Errors
    ///
    /// [`InvalidHost`] if the value cannot be normalised. It is **not** silently dropped: see that
    /// type's documentation for why a safe-but-silent failure is still a failure.
    pub fn allow(mut self, host: impl AsRef<str>) -> Result<Self, InvalidHost> {
        let host = host.as_ref();

        // A URL IS NOT A HOST, AND ACCEPTING ONE IS WORSE THAN REFUSING IT.
        //
        // `allow("https://example.com")` used to NORMALISE — the value is unbracketed and contains
        // exactly one colon, so the part before it was taken and the policy allowed the host
        // `https`, while refusing `example.com`. An attacker sending `Host: https` was served, and
        // the value handed to the application for absolute-URL construction was `https`.
        //
        // Pasting a URL into a host allow-list is the likeliest operator mistake here, and it
        // produced an accepted arbitrary host rather than an error. These three checks run in
        // `allow` rather than in `normalise` because they are about what an operator CONFIGURED,
        // not about what a caller sent — an inbound `Host` cannot contain them anyway.
        for offending in ["://", "@", "/"] {
            if host.contains(offending) {
                return Err(InvalidHost {
                    host: host.to_owned(),
                });
            }
        }

        // THE POLICY IS HOST-ONLY. A port written in a configured entry (`example.com:8080`) is
        // validated — a garbage one is an error, see `normalise` — and then dropped: which port a
        // request arrived on is a fact about the request, carried on its effective origin, and not
        // a thing the allow-list decides.
        let (normalised, _port) = normalise(host).ok_or_else(|| InvalidHost {
            host: host.to_owned(),
        })?;
        self.allowed.insert(normalised);
        Ok(self)
    }

    /// Whether any host is configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }

    /// Validates a raw header value, returning the normalised host and the **explicit** port when
    /// the value is permitted.
    ///
    /// **Fails closed.** Returns `None` when the value is absent, empty, malformed, carries a
    /// control character, names a port that is not a valid non-zero `u16`, or is not configured.
    ///
    /// The port is `Some` only when the header wrote one. Applying the scheme's default is the
    /// router's job, because a host policy does not know which scheme the request arrived under —
    /// and the port is returned at all because it is one third of the request's origin
    /// (RFC 6454 §4), which the CORS carve-out and the CSRF gate compare in full.
    #[must_use]
    pub fn validate(&self, raw: Option<&str>) -> Option<(String, Option<u16>)> {
        let raw = raw?;
        let (normalised, port) = normalise(raw)?;
        self.allowed
            .contains(&normalised)
            .then_some((normalised, port))
    }
}

/// Normalises an authority (`host[:port]`) into the host and the explicit port, or refuses it.
///
/// Each rule below closes a specific bypass. They are listed rather than merged into one regular
/// expression so that a reader can see which attack each one answers.
///
/// `pub(crate)` because [`crate::origin::EffectiveOrigin::parse`] runs the **same** function over
/// an `Origin` header's authority. One normaliser on both sides of an origin comparison is what
/// makes case, a trailing dot, and an IPv6 literal's brackets agree by construction.
pub(crate) fn normalise(raw: &str) -> Option<(String, Option<u16>)> {
    let trimmed = raw.trim();

    // Empty and whitespace-only. An empty `Host` matched an empty configured entry once, in a
    // codebase that trimmed only one side.
    if trimmed.is_empty() {
        return None;
    }

    // Control characters, including CR and LF. A host carrying them is a request-splitting attempt,
    // not a hostname.
    if trimmed.chars().any(char::is_control) {
        return None;
    }

    // Anything non-ASCII. Renvor does not perform IDNA here, and comparing unicode host values
    // byte-wise invites homograph confusion. A punycode host (`xn--…`) is ASCII and passes; a raw
    // unicode host is refused rather than compared naively.
    if !trimmed.is_ascii() {
        return None;
    }

    // A bracketed IPv6 literal keeps its brackets; the port after it is returned. Splitting on the
    // first `:` without this would truncate every IPv6 host at its first group.
    let (without_port, port) = if let Some(end) = trimmed.strip_prefix('[') {
        let close = end.find(']')?;
        let (literal, rest) = end.split_at(close);
        // Only an empty remainder or a port may follow the closing bracket.
        let rest = &rest[1..];
        let port = match rest.strip_prefix(':') {
            None if rest.is_empty() => None,
            None => return None,
            // A PORT THAT IS NOT ONE REFUSES THE VALUE. It used to be dropped, so
            // `[2001:db8::1]:junk` validated as `[2001:db8::1]`.
            Some(port) => Some(parse_port(port)?),
        };
        (format!("[{literal}]"), port)
    } else {
        match trimmed.split_once(':') {
            None => (trimmed.to_owned(), None),
            // A second colon is an unbracketed IPv6 literal, which is ambiguous with host:port.
            // Refuse rather than guess.
            Some((_, port)) if port.contains(':') => return None,
            // A PORT THAT IS NOT ONE REFUSES THE VALUE. `example.com:notaport` and
            // `example.com:0` used to validate as `example.com` with the junk thrown away, so a
            // value that was not a valid authority was accepted as if it were one. The port is
            // one third of the request's origin now (RFC 6454 §4), and a value we cannot fully
            // parse is a value we do not understand.
            Some((host, port)) => (host.to_owned(), Some(parse_port(port)?)),
        }
    };

    // A host with a port and nothing before it.
    if without_port.is_empty() {
        return None;
    }

    // Case folding, and the trailing dot: `example.com.` is the same host as `example.com` in DNS,
    // so treating them as different lets one bypass a policy written for the other.
    let mut host = without_port.to_ascii_lowercase();
    while host.ends_with('.') {
        host.pop();
    }

    if host.is_empty() {
        None
    } else {
        Some((host, port))
    }
}

/// Parses the text after the colon as a port, or refuses it.
///
/// A port is one or more ASCII digits naming a non-zero `u16` (RFC 3986 §3.2.3's `*DIGIT`, with
/// the value bounded by what a socket can carry). Checked digit by digit **before** `u16::from_str`
/// runs, because that parser accepts a leading `+` that no authority grammar does.
fn parse_port(raw: &str) -> Option<u16> {
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let port: u16 = raw.parse().ok()?;
    (port != 0).then_some(port)
}

/// Selects the single host value from what a request carried, refusing ambiguity.
///
/// **More than one `Host` header is refused outright.** Choosing the first or the last is how a
/// request-smuggling difference between two hops becomes a policy bypass: each hop picks a
/// different one, and both believe they validated.
#[must_use]
pub fn single_host<'a>(values: &[&'a str]) -> Option<&'a str> {
    match values {
        [only] => Some(only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{HostPolicy, normalise, single_host};

    #[test]
    fn the_default_policy_allows_nothing() {
        let policy = HostPolicy::default();
        assert!(policy.is_empty());
        assert_eq!(policy.validate(Some("example.com")), None);
    }

    #[test]
    fn a_configured_host_is_accepted_and_others_are_not() {
        let policy = HostPolicy::deny_all()
            .allow("example.com")
            .expect("a valid host");
        assert_eq!(
            policy.validate(Some("example.com")),
            Some(("example.com".to_owned(), None))
        );
        assert_eq!(policy.validate(Some("evil.example")), None);
    }

    #[test]
    fn normalisation_is_applied_to_both_sides() {
        // Configured with mixed case and a port; received in another form. Both must land on the
        // same normalised value, or a policy that looks right rejects every real request.
        let policy = HostPolicy::deny_all()
            .allow("Example.COM")
            .expect("a valid host");
        assert!(policy.validate(Some("example.com")).is_some());
        assert!(policy.validate(Some("EXAMPLE.com:8080")).is_some());
        assert!(policy.validate(Some("example.com.")).is_some());
    }

    #[test]
    fn every_malformed_form_fails_closed() {
        let policy = HostPolicy::deny_all()
            .allow("example.com")
            .expect("a valid host");

        for hostile in [
            "",                    // empty
            "   ",                 // whitespace only
            "example.com\r\nX: y", // request splitting
            "exam\tple.com",       // control character
            "exampłe.com",         // non-ASCII homograph
            ":8080",               // port with no host
            "example.com:80:443",  // ambiguous
            "2001:db8::1",         // unbracketed IPv6, ambiguous with host:port
            "[2001:db8::1]junk",   // junk after the bracket
        ] {
            assert_eq!(
                policy.validate(Some(hostile)),
                None,
                "`{hostile}` was accepted"
            );
        }

        // POSITIVE CONTROL: the validator accepts a legitimate host, so the refusals above are
        // about the inputs rather than about a validator that refuses everything.
        assert!(policy.validate(Some("example.com")).is_some());
    }

    #[test]
    fn a_bracketed_ipv6_host_survives_normalisation() {
        let policy = HostPolicy::deny_all()
            .allow("[2001:db8::1]")
            .expect("a valid host");
        assert!(policy.validate(Some("[2001:db8::1]")).is_some());
        assert!(policy.validate(Some("[2001:db8::1]:8443")).is_some());
        // The literal is not truncated at its first colon, and the port after the bracket is the
        // port — not part of the literal.
        assert_eq!(
            normalise("[2001:db8::1]:8443"),
            Some(("[2001:db8::1]".to_owned(), Some(8443)))
        );
    }

    #[test]
    fn the_port_is_returned_and_a_port_that_is_not_one_refuses_the_whole_value() {
        // THE STRENGTHENING (Phase 010 correction round, finding 1). `example.com:notaport` and
        // `example.com:0` used to VALIDATE: the text after the colon was thrown away, so a value
        // that was not a valid authority was accepted as if it were one. Now that the port is one
        // third of the request's origin (RFC 6454 §4), a port that cannot be one refuses the value
        // — a value we cannot fully parse is a value we do not understand.
        let policy = HostPolicy::deny_all()
            .allow("example.com")
            .expect("a valid host");

        assert_eq!(
            policy.validate(Some("example.com")),
            Some(("example.com".to_owned(), None)),
            "an absent port must be reported as absent, not defaulted here"
        );
        assert_eq!(
            policy.validate(Some("example.com:8080")),
            Some(("example.com".to_owned(), Some(8080)))
        );
        assert_eq!(
            policy.validate(Some("EXAMPLE.com.:443")),
            Some(("example.com".to_owned(), Some(443)))
        );
        // RFC 3986 `port = *DIGIT`: leading zeros are digits, and browsers normalise them away.
        assert_eq!(
            policy.validate(Some("example.com:00080")),
            Some(("example.com".to_owned(), Some(80)))
        );

        for junk in [
            "example.com:notaport",
            "example.com:0",
            "example.com:",
            "example.com:65536",
            "example.com:+80",
            "example.com:-80",
            "example.com:80x",
            "example.com:8 0",
            "[2001:db8::1]:",
            "[2001:db8::1]:0",
            "[2001:db8::1]:junk",
        ] {
            assert_eq!(policy.validate(Some(junk)), None, "`{junk}` was accepted");
        }

        // And a configured entry carrying a garbage port is an error rather than a silent host.
        assert!(
            HostPolicy::deny_all()
                .allow("example.com:notaport")
                .is_err()
        );
        assert!(HostPolicy::deny_all().allow("example.com:0").is_err());
        // POSITIVE CONTROL: a configured entry with a VALID port is accepted (and the port dropped:
        // the policy is host-only), so the errors above are about the ports.
        assert!(
            HostPolicy::deny_all()
                .allow("example.com:8080")
                .expect("a valid host with a valid port")
                .validate(Some("example.com"))
                .is_some()
        );
    }

    #[test]
    fn an_unusable_configured_host_is_an_error_rather_than_a_silent_drop() {
        // The defect this replaced: a typo'd allow-list entry was ignored, the policy then refused
        // every request, and nothing said why.
        for bad in [
            "",
            "   ",
            "exampłe.com",
            "example.com:80:443",
            "2001:db8::1",
            "[abc",
        ] {
            assert!(
                HostPolicy::deny_all().allow(bad).is_err(),
                "`{bad}` was accepted into the policy"
            );
        }

        // POSITIVE CONTROL: a usable host is accepted, so the refusals are about the inputs.
        assert!(HostPolicy::deny_all().allow("example.com").is_ok());

        // AND A DELIBERATE NON-REFUSAL, recorded because the first draft of this test expected the
        // opposite and was wrong. `normalise` TRIMS before it checks for control characters, so a
        // configured value with trailing CRLF yields the clean host `example.com` rather than an
        // error — there is no control character left in what gets stored.
        //
        // That is safe on both sides: the same normalisation runs on the inbound header, so the
        // two cannot disagree. And a raw CR or LF cannot reach the inbound path at all — a header
        // value carrying one is refused by the HTTP layer before this code sees it.
        let trimmed = HostPolicy::deny_all()
            .allow("example.com\r\n")
            .expect("trailing whitespace is trimmed, not refused");
        assert!(trimmed.validate(Some("example.com")).is_some());
    }

    #[test]
    fn a_missing_host_fails_closed() {
        let policy = HostPolicy::deny_all()
            .allow("example.com")
            .expect("a valid host");
        assert_eq!(policy.validate(None), None);
    }

    #[test]
    fn more_than_one_host_header_is_refused_rather_than_chosen_between() {
        assert_eq!(single_host(&["example.com"]), Some("example.com"));
        assert_eq!(single_host(&[]), None);
        // Two hops picking differently is how a smuggled request passes a policy both believed
        // they had applied.
        assert_eq!(single_host(&["example.com", "evil.example"]), None);
    }
}
