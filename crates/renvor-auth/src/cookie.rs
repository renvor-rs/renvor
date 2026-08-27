//! The session-cookie boundary: what is emitted, and what is accepted back.
//!
//! # Two prefix rules, and they are not the same rule
//!
//! `draft-ietf-httpbis-rfc6265bis-22` states the `__Host-` rule **twice, differently**, and this
//! module implements both because they answer different questions.
//!
//! §4.1.3.2, addressed to **servers**:
//!
//! > If a cookie's name begins with a **case-sensitive match** for the string `__Host-`, then the
//! > cookie will have been set with a `Secure` attribute, a `Path` attribute with a value of `/`,
//! > and no `Domain` attribute.
//!
//! §5.4, addressed to **user agents**:
//!
//! > User agents' requirements for cookie name prefixes differ slightly from servers'
//! > (Section 4.1.3) in that UAs MUST match the prefix string **case-insensitively**. This is
//! > because some servers will process cookies case-insensitively, resulting in them
//! > unintentionally miscapitalizing and accepting miscapitalized prefixes.
//!
//! So the case-insensitive rule exists **because a case-insensitive server is the vulnerable
//! party**. Reading it as licence to accept `__host-rv_session` would build the exact defect it
//! was written to describe. This module therefore uses each comparison for one job only:
//!
//! | question | comparison | why |
//! |---|---|---|
//! | is this cookie *my session*? | **case-sensitive** equality | §4.1.3.2 — the server rule |
//! | is this cookie *impersonating* my session? | **case-insensitive** equality, to **reject** | §5.4 — the threat it names |
//!
//! Case-insensitive matching appears here **only on a path that returns an error**. There is no
//! branch on which a case-folded name yields a session, which is the property that matters.
//!
//! # Why rejecting the miscapitalised twin is not a denial of service
//!
//! The obvious objection: an attacker who can set `__host-rv_session` on the victim's origin could
//! log them out for good. They cannot. §5.7 step 21 has the **user agent** apply the `__Host-`
//! criteria case-insensitively, so a conforming browser refuses to store a `__host-`-prefixed
//! cookie that carries a `Domain` attribute — which is what a sibling subdomain would have to
//! send. The UA's case-insensitive rule is precisely what makes this server's case-insensitive
//! *rejection* safe.
//!
//! # These attributes are emitted, not defaulted
//!
//! bis §5.7 step 21.3 requires the `Path` attribute be **present in the attribute list**; a cookie
//! that merely defaults to `/` does not satisfy `__Host-`. Measured rather than assumed — see
//! `specs/009-…/probes/cookie-0.18.2-emission.md` for the emitted string.

use crate::opaque::{Opaque, OpaqueKind};

/// The session cookie's name, in the **one** spelling this crate ever emits or accepts.
pub const SESSION_COOKIE_NAME: &str = "__Host-rv_session";

/// What a redacted cookie renders as.
const REDACTED: &str = "[redacted]";

/// The cross-site policy for the session cookie.
///
/// # There is deliberately no `None` variant
///
/// `SameSite=None` is a legal cookie attribute and a catastrophic session-cookie setting: it opts
/// the identifier into every cross-site request. Leaving the variant out means a deployment
/// **cannot** be configured into it — the same structural argument [`crate::Subject`] makes by not
/// being an `Option`. Callers who need `None` are not configuring a session cookie.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[non_exhaustive]
pub enum SameSiteChoice {
    /// Sent on same-site requests and on top-level cross-site **navigations**.
    ///
    /// The default, and the choice FR-025 requires be justified rather than defaulted into: `Lax`
    /// withholds the cookie from exactly the cross-site methods FR-028 protects — `POST`, `PUT`,
    /// `PATCH`, `DELETE` — while still arriving when a user follows a link from their mail client,
    /// which is the flow a verification or reset link depends on.
    ///
    /// It is **defence in depth, not the defence**. The CSRF control in `crate::csrf` does not
    /// assume any `SameSite` value, because `SameSite` is a user-agent behaviour and a server that
    /// rests its CSRF story on one is trusting the client.
    #[default]
    Lax,
    /// Sent only on same-site requests, including top-level navigation.
    ///
    /// Correct for a deployment with no external entry points. It breaks emailed links — the user
    /// arrives logged out — which is why it is not the default.
    Strict,
}

impl SameSiteChoice {
    /// The `cookie` crate's equivalent.
    const fn as_cookie(self) -> cookie::SameSite {
        match self {
            Self::Lax => cookie::SameSite::Lax,
            Self::Strict => cookie::SameSite::Strict,
        }
    }
}

/// How the session cookie is written.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CookiePolicy {
    /// The cross-site policy.
    pub same_site: SameSiteChoice,
}

impl Default for CookiePolicy {
    fn default() -> Self {
        Self {
            same_site: SameSiteChoice::Lax,
        }
    }
}

/// A rendered `Set-Cookie` value, split so the attributes can be read without the secret.
///
/// # The value and the attributes are separate fields
///
/// An audit record wants to say *"the cookie went out `Secure; HttpOnly; SameSite=Lax; Path=/`"*.
/// If that string were a slice of the whole header, one slicing mistake would put a live session
/// identifier in a log. [`attributes`](Self::attributes) reads a **different field**, so there is
/// no arithmetic that could make it return the value.
#[derive(Clone)]
pub struct SetCookie {
    /// `name=value`.
    pair: String,
    /// Everything after it, without the leading separator.
    attributes: String,
}

impl SetCookie {
    /// The attribute list, carrying **no** secret. Safe to log.
    #[must_use]
    pub fn attributes(&self) -> &str {
        &self.attributes
    }

    /// The complete header value, handed over **once**.
    ///
    /// Named to be conspicuous, for the reason [`Opaque::expose`] gives: every disclosure is
    /// visible at its call site.
    #[must_use]
    pub fn expose_header_value(self) -> String {
        format!("{}; {}", self.pair, self.attributes)
    }
}

impl core::fmt::Debug for SetCookie {
    /// Renders the attributes and **never** the value.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "SetCookie({REDACTED}; {})", self.attributes)
    }
}

/// Why a `Cookie` header did not yield a session.
///
/// **Operator-facing.** The requester is told [`crate::AuthError::CredentialNoLongerValid`] for
/// every one of these, the same two-value split [`crate::service::DispatchOutcome`] uses: loud to
/// the caller, uniform to the requester.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum CookieRejection {
    /// The header carried no cookie by that name.
    Absent,
    /// The name was present and its value was not a session identifier.
    Malformed,
    /// The name appeared **more than once**. Choosing one would be choosing arbitrarily, and an
    /// injected duplicate is how cookie shadowing works.
    Duplicated,
    /// A cookie whose name matches the session cookie's **only when case is ignored** was present.
    /// See the module header.
    PrefixImpersonation,
}

/// Renders the attribute list for a session cookie.
///
/// # Built from a cookie whose value is empty, on purpose
///
/// The crate renders `name=value; attrs`. Rendering with an **empty value** means the returned
/// string is one that never contained a session identifier — which is what lets
/// [`SetCookie::attributes`] be documented as safe to log without that promise resting on
/// arithmetic. The slice below is `name=`'s length, a constant, not a search for a separator.
fn attributes_of(policy: CookiePolicy, max_age: cookie::time::Duration, expired: bool) -> String {
    let mut builder = cookie::Cookie::build((SESSION_COOKIE_NAME, ""))
        // Every one of these is EMITTED rather than defaulted. bis §5.7 step 21.3 requires `Path`
        // be present in the attribute list; a cookie that merely defaults to `/` is not `__Host-`.
        .secure(true)
        .http_only(true)
        .path("/")
        .same_site(policy.same_site.as_cookie())
        .max_age(max_age);
    if expired {
        // A fixed past instant rather than the crate's own removal cookie, which derives its
        // `Expires` from `OffsetDateTime::now_utc()` — a wall-clock read inside a crate whose
        // entire expiry story is an injected `Clock`. A constant is also unambiguously in the
        // past, which "now" is not.
        builder = builder.expires(cookie::time::OffsetDateTime::UNIX_EPOCH);
    }
    // `Domain` is never set. `__Host-` is defined by its ABSENCE, and the way to guarantee an
    // attribute is absent is to have no code that sets it.
    let rendered = builder.build().to_string();
    rendered[SESSION_COOKIE_NAME.len() + 1..]
        .trim_start_matches("; ")
        .to_owned()
}

/// Writes the `Set-Cookie` value that starts a session.
#[must_use]
pub fn issue(token: &Opaque, policy: CookiePolicy, max_age: chrono::Duration) -> SetCookie {
    SetCookie {
        pair: format!("{SESSION_COOKIE_NAME}={}", token.expose()),
        attributes: attributes_of(
            policy,
            cookie::time::Duration::seconds(max_age.num_seconds()),
            false,
        ),
    }
}

/// Writes the `Set-Cookie` value that expires the session cookie in the client.
///
/// **This is not logout.** Expiring the client's copy proves nothing about the server's row; see
/// [`crate::session`], where revocation happens first and this is emitted only afterwards.
///
/// Carries `Max-Age=0` **and** a past `Expires`: `Max-Age` is relative, so client clock skew
/// cannot revive the cookie, and `Expires` covers a user agent that does not implement `Max-Age`.
/// It keeps `Secure`, `HttpOnly` and `SameSite` — the crate's own removal cookie drops the last
/// two, which would briefly describe the session cookie as weaker than it was.
#[must_use]
pub fn expire(policy: CookiePolicy) -> SetCookie {
    SetCookie {
        pair: format!("{SESSION_COOKIE_NAME}="),
        attributes: attributes_of(policy, cookie::time::Duration::ZERO, true),
    }
}

/// Reads the session identifier out of a `Cookie` request header.
///
/// # Errors
///
/// [`CookieRejection`], deterministically: impersonation outranks duplication, which outranks
/// absence, which outranks a malformed value — so the answer does not depend on the order the
/// client happened to send the pairs in.
pub fn read(header: &str) -> Result<Opaque, CookieRejection> {
    let mut value: Option<String> = None;
    let mut seen = 0_usize;
    let mut impersonated = false;

    for parsed in cookie::Cookie::split_parse(header) {
        // A pair that does not parse cannot be ours: this crate's name and value are both
        // well-formed by construction. Skipping it is not leniency about the session — closing on
        // every malformed pair would let any script on the origin end a session by writing one
        // junk cookie.
        let Ok(candidate) = parsed else { continue };
        let name = candidate.name();
        if name == SESSION_COOKIE_NAME {
            // CASE-SENSITIVE, and this is the ONLY comparison that can yield a session.
            seen += 1;
            value = Some(candidate.value().to_owned());
        } else if name.eq_ignore_ascii_case(SESSION_COOKIE_NAME) {
            // CASE-INSENSITIVE, and this branch can only ever produce an error. ASCII folding
            // rather than Unicode: RFC 6265's cookie-name grammar is a `token`, so a non-ASCII
            // name is not a spelling of this one.
            impersonated = true;
        }
    }

    if impersonated {
        return Err(CookieRejection::PrefixImpersonation);
    }
    if seen > 1 {
        return Err(CookieRejection::Duplicated);
    }
    let Some(value) = value else {
        return Err(CookieRejection::Absent);
    };
    // `from_wire` is the length and alphabet check: exactly 64 lowercase hex characters. An
    // upper-case spelling of the same bytes is a DIFFERENT string and is refused, because two
    // spellings of one identifier would defeat the UNIQUE index the stored digest relies on.
    Opaque::from_wire(OpaqueKind::Session, &value).ok_or(CookieRejection::Malformed)
}

#[cfg(test)]
mod tests {
    use super::{
        CookiePolicy, CookieRejection, SESSION_COOKIE_NAME, SameSiteChoice, expire, issue, read,
    };
    use crate::opaque::{Opaque, OpaqueKind};

    /// A syntactically valid session identifier: 64 lowercase hex characters.
    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn token() -> Opaque {
        Opaque::from_wire(OpaqueKind::Session, TOKEN).expect("fixture is valid hex")
    }

    fn week() -> chrono::Duration {
        chrono::Duration::days(7)
    }

    // ---- what is emitted -------------------------------------------------------------------

    #[test]
    fn the_issued_cookie_emits_every_required_attribute() {
        let attributes = issue(&token(), CookiePolicy::default(), week())
            .attributes()
            .to_owned();
        for required in ["Secure", "HttpOnly", "Path=/", "SameSite=Lax"] {
            assert!(
                attributes.contains(required),
                "FR-024 requires {required} be emitted; got [{attributes}]"
            );
        }
    }

    #[test]
    fn the_issued_cookie_names_no_domain() {
        // The negative control for the attribute test above: `__Host-` is defined by the ABSENCE
        // of `Domain`, which no `contains` assertion can establish.
        let attributes = issue(&token(), CookiePolicy::default(), week())
            .attributes()
            .to_owned();
        assert!(
            !attributes.to_ascii_lowercase().contains("domain"),
            "`__Host-` requires no Domain attribute; got [{attributes}]"
        );
    }

    #[test]
    fn the_issued_cookie_carries_the_host_prefix() {
        let header = issue(&token(), CookiePolicy::default(), week()).expose_header_value();
        assert!(
            header.starts_with(&format!("{SESSION_COOKIE_NAME}=")),
            "got [{header}]"
        );
    }

    #[test]
    fn the_issued_cookie_bounds_its_own_lifetime() {
        let attributes = issue(
            &token(),
            CookiePolicy::default(),
            chrono::Duration::hours(2),
        )
        .attributes()
        .to_owned();
        assert!(
            attributes.contains("Max-Age=7200"),
            "a session cookie without a Max-Age outlives the server row; got [{attributes}]"
        );
    }

    #[test]
    fn strict_is_emitted_when_it_is_chosen() {
        let policy = CookiePolicy {
            same_site: SameSiteChoice::Strict,
        };
        let attributes = issue(&token(), policy, week()).attributes().to_owned();
        assert!(attributes.contains("SameSite=Strict"), "got [{attributes}]");
    }

    #[test]
    fn the_explicit_build_matches_what_the_crate_itself_would_conform_to() {
        // The attributes above are set by hand, because FR-024 asks for them to be EMITTED and a
        // reader should see that in the source. This keeps the crate as the authority anyway: it
        // renders the same cookie through `prefixed_mut(Host)`, whose `conform` is what actually
        // implements bis §4.1.3.2, and requires the two to agree byte for byte. If a future
        // `cookie` release changes what `__Host-` means, this fails rather than drifting.
        let mut jar = cookie::CookieJar::new();
        let mut theirs = cookie::Cookie::new("rv_session", TOKEN);
        theirs.set_http_only(true);
        theirs.set_same_site(cookie::SameSite::Lax);
        theirs.set_max_age(cookie::time::Duration::days(7));
        // Deliberately non-conformant, so `conform` has something to correct.
        theirs.set_domain("example.test");
        theirs.set_path("/wrong");
        jar.prefixed_mut(cookie::prefix::Host).add(theirs);
        let reference = jar
            .delta()
            .next()
            .expect("one cookie was added")
            .to_string();

        let ours = issue(&token(), CookiePolicy::default(), week()).expose_header_value();

        let sorted = |header: &str| {
            let mut parts: Vec<String> = header.split("; ").map(str::to_owned).collect();
            parts.sort();
            parts
        };
        assert_eq!(
            sorted(&ours),
            sorted(&reference),
            "\nours: {ours}\ntheirs: {reference}"
        );
    }

    // ---- what is never exposed -------------------------------------------------------------

    #[test]
    fn the_attribute_view_does_not_contain_the_token() {
        let cookie = issue(&token(), CookiePolicy::default(), week());
        assert!(
            !cookie.attributes().contains(TOKEN),
            "the audit-safe view leaked the session identifier"
        );
    }

    #[test]
    fn debug_does_not_render_the_token() {
        let rendered = format!("{:?}", issue(&token(), CookiePolicy::default(), week()));
        assert!(
            !rendered.contains(TOKEN),
            "Debug leaked the session identifier: {rendered}"
        );
        assert!(rendered.contains("[redacted]"), "got {rendered}");
    }

    #[test]
    fn the_expiry_cookie_carries_no_token() {
        let header = expire(CookiePolicy::default()).expose_header_value();
        assert!(
            !header.contains(TOKEN),
            "the expiry cookie must not restate the value it is expiring: {header}"
        );
    }

    #[test]
    fn the_expiry_cookie_expires_immediately_and_in_the_past() {
        let attributes = expire(CookiePolicy::default()).attributes().to_owned();
        assert!(attributes.contains("Max-Age=0"), "got [{attributes}]");
        assert!(
            attributes.contains("Expires=Thu, 01 Jan 1970 00:00:00 GMT"),
            "a UA without Max-Age support needs a date that is unambiguously past; got [{attributes}]"
        );
    }

    #[test]
    fn the_expiry_cookie_keeps_the_security_attributes() {
        // The `cookie` crate's own removal cookie DROPS HttpOnly and SameSite (probe, §2). A
        // `Set-Cookie` that clears those attributes on the way out is a weaker cookie, briefly.
        let attributes = expire(CookiePolicy::default()).attributes().to_owned();
        for required in ["Secure", "HttpOnly", "Path=/"] {
            assert!(attributes.contains(required), "got [{attributes}]");
        }
    }

    // ---- what is accepted ------------------------------------------------------------------

    #[test]
    fn a_correctly_spelled_cookie_is_read() {
        let header = format!("other=1; {SESSION_COOKIE_NAME}={TOKEN}; another=2");
        let found = read(&header).expect("the positive control must pass");
        assert_eq!(found.expose(), TOKEN);
        assert_eq!(found.kind(), OpaqueKind::Session);
    }

    #[test]
    fn a_round_trip_through_the_issued_header_reads_back() {
        // Proves `issue` and `read` agree on the name, rather than each being self-consistent.
        let issued = issue(&token(), CookiePolicy::default(), week()).expose_header_value();
        let pair = issued.split(';').next().expect("a header has a first pair");
        assert_eq!(read(pair).expect("round trip").expose(), TOKEN);
    }

    // ---- what is refused, and why ----------------------------------------------------------

    #[test]
    fn a_miscapitalised_prefix_is_rejected() {
        let header = format!("__host-rv_session={TOKEN}");
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::PrefixImpersonation
        );
    }

    #[test]
    fn an_upper_cased_name_is_rejected() {
        let header = format!("__HOST-RV_SESSION={TOKEN}");
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::PrefixImpersonation
        );
    }

    #[test]
    fn impersonation_outranks_a_correctly_spelled_cookie_beside_it() {
        // The conflicting case. A server that took the well-spelled one would be ignoring an
        // active attack signal on the same request.
        let header = format!("{SESSION_COOKIE_NAME}={TOKEN}; __host-rv_session={TOKEN}");
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::PrefixImpersonation
        );
    }

    #[test]
    fn the_answer_does_not_depend_on_the_order_of_the_pairs() {
        let forward = format!("{SESSION_COOKIE_NAME}={TOKEN}; __host-rv_session={TOKEN}");
        let backward = format!("__host-rv_session={TOKEN}; {SESSION_COOKIE_NAME}={TOKEN}");
        let forward = read(&forward).expect_err("refused");
        let backward = read(&backward).expect_err("refused");
        assert_eq!(forward, backward);
        assert_eq!(forward, CookieRejection::PrefixImpersonation);
    }

    #[test]
    fn a_duplicated_name_is_rejected() {
        let header = format!("{SESSION_COOKIE_NAME}={TOKEN}; {SESSION_COOKIE_NAME}={TOKEN}");
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::Duplicated
        );
    }

    #[test]
    fn a_duplicate_is_rejected_even_when_the_values_differ() {
        let other = "f".repeat(64);
        let header = format!("{SESSION_COOKIE_NAME}={TOKEN}; {SESSION_COOKIE_NAME}={other}");
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::Duplicated
        );
    }

    #[test]
    fn a_value_that_is_not_hex_is_rejected() {
        let header = format!("{SESSION_COOKIE_NAME}={}", "z".repeat(64));
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::Malformed
        );
    }

    #[test]
    fn a_value_of_the_wrong_length_is_rejected() {
        let header = format!("{SESSION_COOKIE_NAME}=abcdef");
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::Malformed
        );
    }

    #[test]
    fn an_upper_case_hex_value_is_rejected() {
        // `Opaque::from_wire` accepts lowercase only, so an upper-case spelling of the SAME bytes
        // is a different string and must not authenticate. Two spellings of one identifier would
        // defeat the UNIQUE index the digest relies on.
        let header = format!("{SESSION_COOKIE_NAME}={}", TOKEN.to_ascii_uppercase());
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::Malformed
        );
    }

    #[test]
    fn a_percent_escaped_value_is_rejected() {
        // `percent-encode` is off, so `%30` stays literal and fails the hex check rather than
        // decoding into a `0` that would pass it.
        let header = format!("{SESSION_COOKIE_NAME}=%30{}", &TOKEN[2..]);
        assert_eq!(
            read(&header).expect_err("this input is refused"),
            CookieRejection::Malformed
        );
    }

    #[test]
    fn an_empty_header_is_absent() {
        assert_eq!(read("").expect_err("empty"), CookieRejection::Absent);
    }

    #[test]
    fn a_header_without_our_cookie_is_absent() {
        assert_eq!(
            read("a=1; b=2").expect_err("no session"),
            CookieRejection::Absent
        );
    }

    // ---- what is deliberately NOT refused --------------------------------------------------

    #[test]
    fn an_unrelated_malformed_pair_does_not_lose_the_session() {
        // The negative control for fail-closed: closing on EVERYTHING would let any third-party
        // script on the origin log the user out by writing one junk cookie.
        let header = format!("junk; ={SESSION_COOKIE_NAME}; {SESSION_COOKIE_NAME}={TOKEN}");
        assert_eq!(read(&header).expect("still readable").expose(), TOKEN);
    }

    #[test]
    fn other_host_prefixed_cookies_are_untouched() {
        let header = format!("__Host-csrf=abc; __host-unrelated=z; {SESSION_COOKIE_NAME}={TOKEN}");
        assert_eq!(read(&header).expect("still readable").expose(), TOKEN);
    }

    #[test]
    fn the_rejection_reason_cannot_carry_the_rejected_value() {
        // `CookieRejection` is fieldless, so its rendering has nowhere to put the input. Asserted
        // rather than reviewed.
        let rejected = read(&format!("{SESSION_COOKIE_NAME}={}", "z".repeat(64)))
            .expect_err("this input is refused");
        assert!(!format!("{rejected:?}").contains('z'), "{rejected:?}");
    }
}
