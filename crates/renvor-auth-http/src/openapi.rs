//! The security schemes the transport actually implements — and no others.
//!
//! # FR-082 is a claim about implementation, so the schemes are derived from it
//!
//! *"OpenAPI declares the security schemes the transport actually implements, and no others."*
//!
//! The failure this forbids is the ordinary one: a document that announces OAuth2 flows, OpenID
//! discovery, or mutual TLS because a generator offered them, and a client that builds against
//! them and finds nothing serving. It is not a documentation defect — a security scheme is a
//! promise about how to authenticate, and an unimplemented one is a false promise in a machine-
//! readable file.
//!
//! Two things follow, and both are structural rather than editorial:
//!
//! - [`renvor_openapi::SecurityScheme`] is a **closed two-variant enum**. `oauth2`,
//!   `openIdConnect` and `mutualTLS` are not expressible, so they cannot be declared by accident.
//! - The cookie scheme's `name` is [`SESSION_COOKIE_NAME`] itself, not a copy of it. A document
//!   naming a cookie the transport does not set would be the same false promise one level down.
//!
//! # The bearer scheme follows the feature, not a flag in this module
//!
//! [`security_schemes`] returns the bearer entry only under `tokens` — the same feature that
//! decides whether `renvor-auth` has an access-token issuer at all. A build without it declares
//! one scheme, because it implements one, and the test in this module asserts exactly that count
//! rather than trusting the `cfg`.

use renvor_auth::cookie::SESSION_COOKIE_NAME;
use renvor_openapi::{ApiKeyLocation, SecurityRequirement, SecurityScheme};
use std::collections::BTreeMap;

/// The document name of the session-cookie scheme.
pub const SESSION_SCHEME: &str = "sessionCookie";

/// The document name of the bearer-token scheme. Declared only under `tokens`.
#[cfg(feature = "tokens")]
pub const BEARER_SCHEME: &str = "bearerToken";

/// Every scheme this transport implements.
///
/// Sorted, because it is a [`BTreeMap`] — two runs emit the same document.
#[must_use]
pub fn security_schemes() -> BTreeMap<String, SecurityScheme> {
    let mut schemes = BTreeMap::new();
    schemes.insert(
        SESSION_SCHEME.to_owned(),
        SecurityScheme::ApiKey {
            // The constant, not a copy of its value. A document naming a cookie the transport does
            // not set is a false promise a client would build against.
            name: SESSION_COOKIE_NAME.to_owned(),
            location: ApiKeyLocation::Cookie,
            description: Some(
                "An opaque server-side session identifier. Set by the login response and cleared \
                 by logout; never readable by script, and never valid across hosts."
                    .to_owned(),
            ),
        },
    );

    #[cfg(feature = "tokens")]
    schemes.insert(
        BEARER_SCHEME.to_owned(),
        SecurityScheme::Http {
            scheme: "bearer".to_owned(),
            bearer_format: Some("JWT".to_owned()),
            description: Some(
                "A short-lived signed access token. The verifier chooses the algorithm and the \
                 key; the token does not."
                    .to_owned(),
            ),
        },
    );

    schemes
}

/// The requirement for an operation reachable with a session cookie.
#[must_use]
pub fn session_required() -> SecurityRequirement {
    // No scopes: a session is not scoped. Scopes belong to token mode, and inventing an empty
    // scope list that means something would be a third meaning for the same syntax.
    SecurityRequirement::new(SESSION_SCHEME, Vec::new())
}

/// The requirement for an operation reachable with a bearer token granting `scopes`.
#[cfg(feature = "tokens")]
#[must_use]
pub fn bearer_required(scopes: &[&str]) -> SecurityRequirement {
    SecurityRequirement::new(
        BEARER_SCHEME,
        scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    )
}

/// The requirement for an operation that is deliberately **unauthenticated**.
///
/// An empty `Vec<SecurityRequirement>` is serialised as absent, which in OpenAPI means *"inherit
/// the document's top-level requirement"* — not *"no security"*. Login, registration and
/// forgot-password are reachable without a credential by design, and they must say so explicitly
/// rather than by omission.
#[must_use]
pub fn no_credential_required() -> Vec<SecurityRequirement> {
    vec![SecurityRequirement::default()]
}

#[cfg(test)]
mod tests {
    use super::{SESSION_SCHEME, no_credential_required, security_schemes, session_required};
    use renvor_auth::cookie::SESSION_COOKIE_NAME;
    use renvor_openapi::{ApiKeyLocation, SecurityScheme};

    #[test]
    fn the_declared_schemes_are_exactly_the_ones_implemented() {
        let schemes = security_schemes();

        // The count follows the feature. Asserted rather than assumed, so a `cfg` that stopped
        // matching the build would fail here instead of shipping a document announcing an
        // authentication method nothing serves.
        #[cfg(feature = "tokens")]
        assert_eq!(schemes.len(), 2, "token mode implements two schemes");
        #[cfg(not(feature = "tokens"))]
        assert_eq!(
            schemes.len(),
            1,
            "without token mode the transport implements only the session cookie"
        );

        let session = schemes.get(SESSION_SCHEME).expect("the session scheme");
        let SecurityScheme::ApiKey { name, location, .. } = session else {
            panic!("the session credential is a cookie, not an Authorization scheme");
        };
        assert_eq!(name, SESSION_COOKIE_NAME);
        assert_eq!(*location, ApiKeyLocation::Cookie);

        // The `__Host-` prefix is the property batch F built the cookie boundary around, and a
        // document that named an unprefixed cookie would describe a weaker credential than the one
        // the transport sets.
        assert!(
            name.starts_with("__Host-"),
            "the declared name lost its prefix"
        );
    }

    #[test]
    fn the_bearer_scheme_is_absent_without_token_mode() {
        // The other half of FR-082: "and no others". A build that does not implement token mode
        // must not declare a bearer scheme, or a client will send one and be refused.
        let schemes = security_schemes();
        let has_bearer = schemes
            .values()
            .any(|scheme| matches!(scheme, SecurityScheme::Http { .. }));
        #[cfg(feature = "tokens")]
        assert!(has_bearer, "token mode declares no bearer scheme");
        #[cfg(not(feature = "tokens"))]
        assert!(
            !has_bearer,
            "a bearer scheme is declared but not implemented"
        );
    }

    #[test]
    fn an_unauthenticated_operation_says_so_rather_than_omitting_it() {
        // An empty Vec serialises as absent, which OpenAPI reads as "inherit the top-level
        // requirement" — the opposite of what login means.
        let explicit = no_credential_required();
        assert_eq!(explicit.len(), 1, "the requirement list must not be empty");
        assert!(
            explicit[0].0.is_empty(),
            "an unauthenticated operation names no scheme"
        );

        // POSITIVE CONTROL: a credentialled requirement DOES name one, so the emptiness above is a
        // property of this helper rather than of the type.
        assert_eq!(session_required().0.len(), 1);
        assert!(session_required().0.contains_key(SESSION_SCHEME));
    }
}
