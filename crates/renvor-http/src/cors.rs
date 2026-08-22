//! CORS policy, validated **before** a layer is built.
//!
//! # Why Renvor validates this itself
//!
//! ADR-0012, Finding 2. The selected middleware collection detects a wildcard-plus-credentials
//! configuration with `assert!`, and calls that check from **two** places: when the layer is
//! applied, and from `Service::poll_ready` — which runs **while serving a request**.
//!
//! A configuration mistake therefore panics a running server. Renvor refuses the same configuration
//! with a typed error when the policy is built, so the process never reaches a state where that
//! assertion could fire.
//!
//! # The upstream default is kept, because it is already right
//!
//! `CorsLayer::new()` **is** deny-by-default — its origin list defaults to empty. Only the
//! validation is taken over; the protocol work stays upstream, where it belongs.

use std::collections::BTreeSet;

/// A CORS configuration that could not be honoured safely.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CorsPolicyError {
    /// A wildcard origin was combined with credentials.
    WildcardWithCredentials,
    /// An origin was not a usable origin value.
    InvalidOrigin {
        /// The offending value.
        origin: String,
        /// Why it was refused.
        reason: &'static str,
    },
}

impl core::fmt::Display for CorsPolicyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::WildcardWithCredentials => f.write_str(
                "a wildcard origin cannot be combined with credentials: it would let any site read \
                 authenticated responses. Name the exact origins instead",
            ),
            Self::InvalidOrigin { origin, reason } => {
                write!(f, "`{origin}` is not a usable origin: {reason}")
            }
        }
    }
}

impl core::error::Error for CorsPolicyError {}

/// Which origins may read responses.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CorsPolicy {
    origins: BTreeSet<String>,
    wildcard: bool,
    credentials: bool,
}

impl CorsPolicy {
    /// A policy allowing **no** origin. The default.
    #[must_use]
    pub fn deny_all() -> Self {
        Self::default()
    }

    /// Allows one exact origin.
    ///
    /// **Exact only.** There is no suffix match and no pattern: `https://evil-example.com` ends
    /// with `example.com`, and a suffix rule that looked reasonable would admit it.
    ///
    /// # Errors
    ///
    /// [`CorsPolicyError::InvalidOrigin`] if the value is empty, carries a control character, has
    /// no scheme, or carries a path.
    pub fn allow_origin(mut self, origin: impl Into<String>) -> Result<Self, CorsPolicyError> {
        let origin = origin.into();
        let refuse = |reason| {
            Err(CorsPolicyError::InvalidOrigin {
                origin: origin.clone(),
                reason,
            })
        };

        if origin.trim().is_empty() {
            return refuse("an origin may not be empty");
        }
        if origin.chars().any(char::is_control) {
            return refuse("an origin may not contain a control character");
        }
        // `null` is a real Origin value, sent by sandboxed iframes and by some redirects. Allowing
        // it is allowing an unattributable caller.
        if origin.eq_ignore_ascii_case("null") {
            return refuse("`null` is not an attributable origin");
        }
        let Some((scheme, rest)) = origin.split_once("://") else {
            return refuse("an origin must include a scheme, as in `https://example.com`");
        };
        if scheme.is_empty() || rest.is_empty() {
            return refuse("an origin must include a scheme and a host");
        }
        if rest.contains('/') {
            return refuse("an origin carries no path");
        }

        self.origins.insert(origin);
        Ok(self)
    }

    /// Allows **any** origin.
    ///
    /// Combining this with [`CorsPolicy::allow_credentials`] is refused by
    /// [`CorsPolicy::validate`], which every construction path runs.
    #[must_use]
    pub const fn allow_any_origin(mut self) -> Self {
        self.wildcard = true;
        self
    }

    /// Permits credentialed cross-origin requests.
    #[must_use]
    pub const fn allow_credentials(mut self) -> Self {
        self.credentials = true;
        self
    }

    /// Checks the policy is safe to honour.
    ///
    /// # Errors
    ///
    /// [`CorsPolicyError::WildcardWithCredentials`] when both are set.
    pub const fn validate(&self) -> Result<(), CorsPolicyError> {
        if self.wildcard && self.credentials {
            return Err(CorsPolicyError::WildcardWithCredentials);
        }
        Ok(())
    }

    /// Whether `origin` may read responses.
    ///
    /// Comparison is **exact and case-sensitive** on the host. An origin is a serialisation with
    /// one canonical form; case-folding it would make `https://Example.com` and
    /// `https://example.com` interchangeable, and only one of them is what the operator wrote.
    #[must_use]
    pub fn allows(&self, origin: &str) -> bool {
        if self.wildcard {
            return true;
        }
        self.origins.contains(origin)
    }

    /// Whether credentials are permitted.
    #[must_use]
    pub const fn credentials_allowed(&self) -> bool {
        self.credentials
    }

    /// Whether any origin is permitted.
    #[must_use]
    pub const fn is_wildcard(&self) -> bool {
        self.wildcard
    }

    /// The exact origins permitted.
    pub fn origins(&self) -> impl Iterator<Item = &str> {
        self.origins.iter().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::{CorsPolicy, CorsPolicyError};

    #[test]
    fn the_default_policy_allows_no_origin() {
        let policy = CorsPolicy::default();
        assert!(!policy.allows("https://example.com"));
        assert!(!policy.allows("null"));
        assert!(!policy.is_wildcard());
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn a_wildcard_with_credentials_is_refused_when_the_policy_is_built() {
        // FR-022. The upstream library detects this too — with an assertion reachable from
        // `poll_ready`, i.e. while serving. This returns a value instead, so the process never
        // starts in that state.
        let policy = CorsPolicy::deny_all()
            .allow_any_origin()
            .allow_credentials();

        assert_eq!(
            policy.validate(),
            Err(CorsPolicyError::WildcardWithCredentials)
        );

        // POSITIVE CONTROLS: each half alone is permitted, so the refusal is about the
        // COMBINATION rather than about either setting.
        assert!(CorsPolicy::deny_all().allow_any_origin().validate().is_ok());
        assert!(
            CorsPolicy::deny_all()
                .allow_credentials()
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn origin_matching_is_exact_and_has_no_suffix_rule() {
        let policy = CorsPolicy::deny_all()
            .allow_origin("https://example.com")
            .expect("a valid origin");

        assert!(policy.allows("https://example.com"));

        for near_miss in [
            "https://evil-example.com", // suffix match would admit this
            "https://sub.example.com",  // subdomain
            "http://example.com",       // scheme
            "https://example.com:8443", // port
            "https://Example.com",      // case
            "https://example.com/",     // trailing slash is not an origin
        ] {
            assert!(!policy.allows(near_miss), "`{near_miss}` was allowed");
        }
    }

    #[test]
    fn an_unusable_origin_is_refused_at_configuration_time() {
        for (bad, _why) in [
            ("", "empty"),
            ("   ", "whitespace"),
            ("example.com", "no scheme"),
            ("https://", "no host"),
            ("https://example.com/path", "carries a path"),
            ("null", "unattributable"),
            ("NULL", "unattributable, any case"),
            ("https://example.com\r\nX: y", "control character"),
        ] {
            assert!(
                matches!(
                    CorsPolicy::deny_all().allow_origin(bad),
                    Err(CorsPolicyError::InvalidOrigin { .. })
                ),
                "`{bad}` was accepted"
            );
        }

        // POSITIVE CONTROL.
        assert!(
            CorsPolicy::deny_all()
                .allow_origin("https://example.com")
                .is_ok()
        );
    }
}
