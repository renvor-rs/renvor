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
        let normalised = normalise(host).ok_or_else(|| InvalidHost {
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

    /// Validates a raw header value, returning the normalised host when it is permitted.
    ///
    /// **Fails closed.** Returns `None` when the value is absent, empty, malformed, carries a
    /// control character, or is not configured.
    #[must_use]
    pub fn validate(&self, raw: Option<&str>) -> Option<String> {
        let raw = raw?;
        let normalised = normalise(raw)?;
        if self.allowed.contains(&normalised) {
            Some(normalised)
        } else {
            None
        }
    }
}

/// Normalises a host value, or refuses it.
///
/// Each rule below closes a specific bypass. They are listed rather than merged into one regular
/// expression so that a reader can see which attack each one answers.
fn normalise(raw: &str) -> Option<String> {
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

    // A bracketed IPv6 literal keeps its brackets; the port after it is dropped. Splitting on the
    // first `:` without this would truncate every IPv6 host at its first group.
    let without_port = if let Some(end) = trimmed.strip_prefix('[') {
        let close = end.find(']')?;
        let (literal, rest) = end.split_at(close);
        // Only an empty remainder or a port may follow the closing bracket.
        let rest = &rest[1..];
        if !rest.is_empty() && !rest.starts_with(':') {
            return None;
        }
        format!("[{literal}]")
    } else {
        // More than one colon in a non-bracketed value is an unbracketed IPv6 literal, which is
        // ambiguous with host:port. Refuse rather than guess.
        match trimmed.split(':').count() {
            1 => trimmed.to_owned(),
            2 => trimmed
                .split(':')
                .next()
                .expect("split always yields one part")
                .to_owned(),
            _ => return None,
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

    if host.is_empty() { None } else { Some(host) }
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
            Some("example.com".to_owned())
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
        // The literal is not truncated at its first colon.
        assert_eq!(
            normalise("[2001:db8::1]:8443").as_deref(),
            Some("[2001:db8::1]")
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
