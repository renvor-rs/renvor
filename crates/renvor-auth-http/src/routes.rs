//! The routes, for every flow in spec §3.1–3.5.
//!
//! # Every route does the same five things, in the same order
//!
//! 1. **Resolve the correlation identifier** from the request context. It is opaque, it is the only
//!    thing that reaches a problem document, and a caller cannot supply one.
//! 2. **Count the attempt.** [`AbuseGuard::admit`] returns an [`Admitted`](renvor_auth::abuse::Admitted), and the flow's method
//!    will not compile without one — so a route that forgot to bound itself does not exist.
//! 3. **Read the body**, refusing a malformed one before anything else looks at it.
//! 4. **Call the operation**, which is where the policy and scope checks live (FR-057).
//! 5. **Render**, through [`crate::problem`] on every failure path.
//!
//! # What a handler is not allowed to see
//!
//! The `ClientIdentity` an abuse control counts comes from [`RequestContext`](renvor_http::RequestContext), resolved by the
//! Phase 004 layer that knows the trust configuration. **No handler here parses a forwarding
//! header**, and none can: `Request` does not carry one.
//!
//! The two credential headers it *does* carry — `Cookie:` and `Authorization:` — reach
//! `renvor-auth` unparsed, because deciding what a well-formed session cookie looks like is the
//! cookie boundary's job and doing it twice is how the two answers start disagreeing.
//!
//! # The mailed flows return the same body whether or not the account exists
//!
//! [`AcknowledgedResponse`](crate::dto::AcknowledgedResponse) has one shape and one value. That is FR-052 at the wire, and it is not
//! a choice made per call site: there is no other value the type can hold.

use std::sync::Arc;

use renvor_auth::abuse::{AbuseGuard, AttemptFlow, AttemptRepository, FlowKeys};
use renvor_auth::audit::{AuditSink, CorrelationId};
use renvor_auth::mail::MailPort;
use renvor_auth::opaque::SecretDigest;
use renvor_auth::password::PasswordBlocklist;
use renvor_auth::repository::{
    CredentialRepository, Registration, SingleUseTokenRepository, UserRepository,
};
use renvor_auth::service::{AuthenticationService, ServiceError, TokenLifetime};
use renvor_auth::session::{SessionOutcome, SessionRepository, SessionService};
use renvor_auth::{AuthError, Clock, CsrfKey, csrf};
use renvor_http::route::{Request, Response, RouteError, RouteGroup};

use crate::dto;
use crate::problem;

/// Everything the routes call.
///
/// # Eight type parameters, and none of them is `dyn`
///
/// A boxed port for each would have been shorter to write and would have put a vtable in front of
/// every authentication operation. More importantly it would have hidden which ports a flow needs:
/// this signature is the dependency list, and a flow that grew a new one would change it visibly.
pub struct AuthEndpoints<U, C, B, S, T, M, R, A> {
    /// Registration, login, the mailed flows, and the current user.
    pub authentication: AuthenticationService<U, C, B>,
    /// Session lifecycle and the cookie boundary.
    pub sessions: SessionService<S>,
    /// The **verification** token store.
    ///
    /// # Two fields, because a `SingleUseTokenRepository` is bound to one table
    ///
    /// `SqlxSingleUseTokenRepository::new` takes a `TokenTable` and holds it, so one value
    /// addresses one table. A single field served both flows, and the consequence was not
    /// cosmetic: `issue_and_send` calls `invalidate_all_for` before issuing, so **a verification
    /// resend silently destroyed the user's outstanding password-reset token**, and every reset
    /// token was written to and read from the verification table.
    ///
    /// Found by requirements review. The `OpaqueKind` tag meant a verification token still could
    /// not reset a password, so this was a correctness defect rather than a privilege one.
    pub verifications: T,
    /// The **password-reset** token store. A different table; see [`Self::verifications`].
    pub resets: T,
    /// Where mail goes. Phase 010 owns the operational adapter (FR-075).
    pub mail: M,
    /// The abuse controls. **Every flow passes through this.**
    pub abuse: AbuseGuard<R, A>,
    /// How long a mailed token stays valid.
    pub token_lifetime: TokenLifetime,
    /// The CSRF signing key.
    ///
    /// FR-028: *"Every cookie-authenticated unsafe operation is CSRF-protected."* In this route
    /// table that is exactly one operation — `POST /auth/logout`. Every other unsafe route is
    /// unauthenticated or presents a bearer token, and `GET /auth/me` is safe.
    ///
    /// The module shipped in batch F and nothing wired it until a requirements review found that
    /// logout was reachable cross-site.
    pub csrf: CsrfKey,
    /// The entropy port, for minting CSRF tokens. The only source of randomness here.
    pub entropy: Arc<dyn renvor_core::observe::EntropySource + Send + Sync>,
    /// The injected clock. There is no wall-clock read anywhere in this module.
    pub clock: Arc<dyn Clock>,
}

/// The correlation identifier for one request.
///
/// `RequestId::encode` produces exactly sixteen lowercase hexadecimal characters, which is exactly
/// what [`CorrelationId::parse`] accepts. The `unwrap_or_else` is therefore unreachable — and it is
/// written rather than a `expect`, because a panic in the correlation path would turn a
/// transport-layer surprise into a 500 for a request that was otherwise fine.
fn correlation_of(request: &Request) -> CorrelationId {
    CorrelationId::parse(&request.context().request_id().encode())
        .unwrap_or_else(|| CorrelationId::from_bytes([0; 8]))
}

/// Reads a JSON body, or renders the refusal.
///
/// The parse error is **discarded**. `serde_json` reports the offending line, column, and often the
/// unexpected token — all of which are caller data, and a reset password is caller data.
fn read_body<T: serde::de::DeserializeOwned>(
    request: &Request,
    correlation: CorrelationId,
) -> Result<T, Response> {
    serde_json::from_slice(request.body()).map_err(|_| {
        // `malformed_body`, NOT `authentication_required`.
        //
        // The first version returned 401 for every unparseable body, so `POST /auth/register` with
        // a stray `{` answered "the request presented no usable credential" — for a route that
        // takes no credential. A wrong answer a client will branch on. Found by security review.
        problem::malformed_body(correlation)
    })
}

/// A JSON response with a status.
fn json(status: u16, body: &impl serde::Serialize) -> Response {
    match serde_json::to_string(body) {
        Ok(rendered) => Response::status(status)
            .unwrap_or_else(|_| Response::text(""))
            .with_header("content-type", "application/json")
            .unwrap_or_else(|_| Response::text(""))
            .with_body(rendered),
        // A serialisation failure here is a defect in this crate. A bare status is the honest
        // answer; a partial body would be parsed and acted on.
        Err(_) => Response::status(500).unwrap_or_else(|_| Response::text("")),
    }
}

/// The keys a flow counts, built from the request and nothing else.
fn keys_for<'a>(
    request: &Request,
    account: Option<&'a str>,
    client: Option<[u8; 16]>,
) -> FlowKeys<'a> {
    FlowKeys {
        account,
        client,
        // FROM THE RESOLVED CONTEXT, never from a header this crate read. Phase 004's layer
        // already decided whether the direct peer was trusted; repeating that decision here is
        // exactly what FR-065 forbids.
        network: request.context().client(),
    }
}

/// Attaches a `Set-Cookie` header, or reports the defect.
fn with_cookie(response: Response, cookie: renvor_auth::SetCookie) -> Response {
    response
        .with_header("set-cookie", cookie.expose_header_value())
        .unwrap_or_else(|_| Response::status(500).unwrap_or_else(|_| Response::text("")))
}

/// `POST /auth/register`. **Bounded** — `AttemptFlow::Register`.
///
/// # It was not bounded until a security review said what that cost
///
/// FR-063 names six flows and registration is not among them, so the first version of this handler
/// left it unbounded and recorded the gap. The review priced it: an **unauthenticated,
/// unrate-limited Argon2id amplifier** at 64 MiB and ~72 ms per request, and an unbounded source of
/// rows in `rv_auth_user` — which the SQ-4 bound does not cover, because SQ-4 is about
/// `rv_auth_attempt`.
///
/// A seventh flow costs two dimensions, a changed row bound, a changed schema `CHECK`, and every
/// document that states the formula. That is what it cost.
async fn register<U, C, B, S, T, M, R, A>(
    endpoints: &AuthEndpoints<U, C, B, S, T, M, R, A>,
    request: Request,
) -> Response
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
    R: AttemptRepository,
    A: AuditSink,
{
    let correlation = correlation_of(&request);
    let body: dto::RegisterRequest = match read_body(&request, correlation) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let now = endpoints.clock.now();

    let admitted = match endpoints
        .abuse
        .admit(
            AttemptFlow::Register,
            keys_for(&request, Some(&body.email), None),
            correlation,
            now,
        )
        .await
    {
        Ok(admitted) => admitted,
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    match endpoints
        .authentication
        .register(admitted, &body.email, &body.password, now)
        .await
    {
        // A DUPLICATE IS NOT AN ERROR AND NOT A DIFFERENT ANSWER.
        //
        // Both arms produce the SAME value of the SAME type — `AcknowledgedResponse` has one shape
        // and one value, so there is nothing here to differ. The first draft of this function
        // returned the new account's identifier on one arm and an empty string on the other, which
        // is an enumeration oracle with extra steps: a caller reads the length and knows.
        //
        // The cost is that a client does not learn the identifier from registration. It learns it
        // from logging in, which it must do anyway, and which requires the password.
        Ok(Registration::Created(_) | Registration::AlreadyRegistered) => {
            json(202, &dto::AcknowledgedResponse::new())
        }
        Err(error) => problem::render_service_error(&error, correlation),
    }
}

/// `POST /auth/login`. **Bounded** — `AttemptFlow::LogIn`.
async fn log_in<U, C, B, S, T, M, R, A>(
    endpoints: &AuthEndpoints<U, C, B, S, T, M, R, A>,
    request: Request,
) -> Response
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
    S: SessionRepository,
    R: AttemptRepository,
    A: AuditSink,
{
    let correlation = correlation_of(&request);
    let body: dto::LogInRequest = match read_body(&request, correlation) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let now = endpoints.clock.now();

    // COUNTED FIRST, and counted whether or not the account exists. The admission is the only way
    // to reach `log_in`, so there is no path that skips this.
    let admitted = match endpoints
        .abuse
        .admit(
            AttemptFlow::LogIn,
            keys_for(&request, Some(&body.email), None),
            correlation,
            now,
        )
        .await
    {
        Ok(admitted) => admitted,
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    let authenticated = match endpoints
        .authentication
        .log_in(admitted, &body.email, &body.password)
        .await
    {
        Ok(authenticated) => authenticated,
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    // The session is established AFTER authentication, and it retires whatever the caller arrived
    // holding — which is the fixation defence, and why the presented cookie is passed in.
    match endpoints
        .sessions
        .begin(authenticated.subject, request.credentials().cookie(), now)
        .await
    {
        Ok((cookie, _established)) => {
            // THE CSRF TOKEN IS BOUND TO THE SESSION JUST ESTABLISHED, which is why it is minted
            // here and not earlier: `csrf::issue` signs over the session's digest, so a token from
            // before the rotation would not verify against the session after it. That is the
            // property `rotating_the_session_invalidates_every_token_bound_to_it` asserts.
            let Some(session) = cookie_digest(&cookie) else {
                return problem::render_service_error(
                    &ServiceError::Refused(AuthError::PolicyMisconfigured),
                    correlation,
                );
            };
            let Ok(csrf) = csrf::issue(&endpoints.csrf, &session, endpoints.entropy.as_ref())
            else {
                return problem::render_service_error(
                    &ServiceError::Refused(AuthError::EntropyUnavailable),
                    correlation,
                );
            };
            with_cookie(
                json(
                    200,
                    &dto::AuthenticatedResponse {
                        id: authenticated.subject.user_id().to_string(),
                        csrf_token: csrf.expose(),
                    },
                ),
                cookie,
            )
        }
        Err(error) => problem::render_service_error(&error, correlation),
    }
}

/// The session digest a `Set-Cookie` names, for binding a CSRF token to it.
///
/// Parses the header this crate just produced rather than reaching into the session service: the
/// digest is what `csrf::issue` signs over, and `cookie::read` is the one function that knows how
/// to recover an identifier from a cookie header. A second parser here would be a second answer.
fn cookie_digest(cookie: &renvor_auth::SetCookie) -> Option<SecretDigest> {
    // `expose_header_value` consumes, so this works on a clone — the value is going to the client
    // in the same response either way, and cloning it changes no disclosure.
    let rendered = cookie.clone().expose_header_value();
    let pair = rendered.split(';').next()?;
    renvor_auth::cookie::read(pair)
        .ok()
        .map(|secret| SecretDigest::of(&secret))
}

/// `POST /auth/logout`. **CSRF-protected** — the one cookie-authenticated unsafe operation here.
///
/// Not one of the abuse-bounded flows, and deliberately so: the only thing it can do is revoke, so
/// an attacker who floods it spends their own budget destroying their own sessions.
async fn log_out<U, C, B, S, T, M, R, A>(
    endpoints: &AuthEndpoints<U, C, B, S, T, M, R, A>,
    request: Request,
) -> Response
where
    S: SessionRepository,
{
    let correlation = correlation_of(&request);
    let now = endpoints.clock.now();

    // CSRF FIRST, before anything is revoked.
    //
    // `is_required` decides from the method and the credential kind — a cookie-authenticated unsafe
    // request needs a token; a bearer-authenticated one does not, because a browser does not attach
    // an `Authorization:` header cross-site on its own.
    let presented_cookie = request.credentials().cookie();
    let credential = if request.credentials().authorization().is_empty() {
        csrf::Credential::Cookie
    } else {
        csrf::Credential::BearerToken
    };
    if csrf::is_required("POST", credential) {
        // 009/L-4, CLOSED AT THE TRANSPORT (FR-085): a cookie-authenticated unsafe request that
        // the user agent itself marks `cross-site`, or whose `Origin` names another host than
        // the one validated for this request, is refused before the body is read. Absent
        // headers do not refuse — the session-bound CSRF token below stays the primary control.
        if cross_site_refused(&request) {
            return problem::render_service_error(
                &ServiceError::Refused(AuthError::NotPermitted),
                correlation,
            );
        }
        let body: dto::LogoutRequest = match read_body(&request, correlation) {
            Ok(body) => body,
            Err(response) => return response,
        };
        let Ok(secret) = renvor_auth::cookie::read(presented_cookie) else {
            // No readable session means nothing to protect and nothing to revoke. The generic
            // credential refusal, which is what an expired cookie gets too.
            return problem::render_service_error(
                &ServiceError::Refused(AuthError::CredentialNoLongerValid),
                correlation,
            );
        };
        if csrf::verify(
            &endpoints.csrf,
            &SecretDigest::of(&secret),
            &[body.csrf_token.as_str()],
        )
        .is_err()
        {
            // EVERY `CsrfRejection` becomes one refusal. Which of absent, duplicated, malformed,
            // unbound or forged it was is operator-facing; telling a requester would say which
            // half of the double submit they got wrong.
            return problem::render_service_error(
                &ServiceError::Refused(AuthError::NotPermitted),
                correlation,
            );
        }
    }

    // ONE ANSWER whether or not a live session was found. `LogoutOutcome` distinguishes them for
    // the caller; the requester is told the same thing either way, because the goal is that no
    // usable session remains and none does.
    match endpoints
        .sessions
        .log_out(request.credentials().cookie(), now)
        .await
    {
        Ok((cookie, _outcome)) => with_cookie(json(200, &dto::AcknowledgedResponse::new()), cookie),
        Err(error) => problem::render_service_error(&error, correlation),
    }
}

/// `GET /auth/me`.
/// True when the user agent said the request is cross-site, or its `Origin` names a host other
/// than the one this request was validated for (FR-085). Absent headers never refuse.
pub(crate) fn cross_site_refused(request: &Request) -> bool {
    let metadata = request.fetch_metadata();
    if metadata.sec_fetch_site() == Some(renvor_http::SecFetchSite::CrossSite) {
        return true;
    }
    match metadata.origin_host() {
        Some(host) => !host.eq_ignore_ascii_case(request.context().host()),
        // An `Origin` that is present but not a URL (`null`, or garbage) names no host this
        // request was validated for.
        None => metadata.origin().is_some(),
    }
}

async fn current_user<U, C, B, S, T, M, R, A>(
    endpoints: &AuthEndpoints<U, C, B, S, T, M, R, A>,
    request: Request,
) -> Response
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
    S: SessionRepository,
{
    let correlation = correlation_of(&request);
    let now = endpoints.clock.now();

    let subject = match endpoints
        .sessions
        .authenticate(request.credentials().cookie(), now)
        .await
    {
        Ok(SessionOutcome::Live(subject)) => subject,
        // EVERY rejection reason becomes one refusal. `SessionRejection` is operator-facing and
        // never rendered — a malformed cookie, an expired session and a revoked one are the same
        // answer to the holder of a dead cookie.
        Ok(SessionOutcome::Rejected(_)) => {
            return problem::render_service_error(
                &ServiceError::Refused(AuthError::CredentialNoLongerValid),
                correlation,
            );
        }
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    match endpoints.authentication.current_user(subject).await {
        Ok(user) => json(
            200,
            &dto::CurrentUserResponse {
                id: user.id.to_string(),
                email: user.email,
                email_verified: user.email_verified_at.is_some(),
            },
        ),
        Err(error) => problem::render_service_error(&error, correlation),
    }
}

/// The two mailed flows, which differ only in which token they issue and which template carries it.
///
/// One function, because their **being indistinguishable is the requirement**. Two functions would
/// be two places for the generic response to drift, and the first thing that drifts is the one
/// nobody is looking at.
async fn mailed_flow<U, C, B, S, T, M, R, A>(
    endpoints: &AuthEndpoints<U, C, B, S, T, M, R, A>,
    request: Request,
    flow: AttemptFlow,
) -> Response
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
    T: SingleUseTokenRepository,
    M: MailPort,
    R: AttemptRepository,
    A: AuditSink,
{
    let correlation = correlation_of(&request);
    let body: dto::AddressRequest = match read_body(&request, correlation) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let now = endpoints.clock.now();

    let admitted = match endpoints
        .abuse
        .admit(
            flow,
            keys_for(&request, Some(&body.email), None),
            correlation,
            now,
        )
        .await
    {
        Ok(admitted) => admitted,
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    // `DispatchOutcome` is DISCARDED here, and that is the whole design.
    //
    // It distinguishes "no account", "delivered" and "the transport refused it" — three facts an
    // operator needs and a requester must not have. `renvor-auth` returns them so the caller can
    // log them; this is the boundary at which they stop.
    let dispatched = match flow {
        AttemptFlow::ForgotPassword => {
            endpoints
                .authentication
                .forgot_password(
                    admitted,
                    &endpoints.resets,
                    &endpoints.mail,
                    &body.email,
                    endpoints.token_lifetime,
                    now,
                )
                .await
        }
        _ => {
            endpoints
                .authentication
                .send_verification(
                    admitted,
                    &endpoints.verifications,
                    &endpoints.mail,
                    &body.email,
                    endpoints.token_lifetime,
                    now,
                )
                .await
        }
    };

    match dispatched {
        Ok((_acknowledged, _outcome)) => json(202, &dto::AcknowledgedResponse::new()),
        Err(error) => problem::render_service_error(&error, correlation),
    }
}

/// `POST /auth/verification/confirm`. **Bounded** — `AttemptFlow::VerificationComplete`.
async fn confirm_verification<U, C, B, S, T, M, R, A>(
    endpoints: &AuthEndpoints<U, C, B, S, T, M, R, A>,
    request: Request,
) -> Response
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
    T: SingleUseTokenRepository,
    R: AttemptRepository,
    A: AuditSink,
{
    let correlation = correlation_of(&request);
    let body: dto::TokenRequest = match read_body(&request, correlation) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let now = endpoints.clock.now();

    // NETWORK AXIS ONLY. The request carries a token and no account identifier; resolving the
    // token to charge an account axis would leave an invalid token with no account to charge, and
    // that difference in stored state is observable. See the contract matrix.
    let admitted = match endpoints
        .abuse
        .admit(
            AttemptFlow::VerificationComplete,
            keys_for(&request, None, None),
            correlation,
            now,
        )
        .await
    {
        Ok(admitted) => admitted,
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    match endpoints
        .authentication
        .confirm_verification(admitted, &endpoints.verifications, &body.token, now)
        .await
    {
        Ok(_user) => json(200, &dto::AcknowledgedResponse::new()),
        Err(error) => problem::render_service_error(&error, correlation),
    }
}

/// `POST /auth/password/reset`. **Bounded** — `AttemptFlow::ResetPassword`.
async fn reset_password<U, C, B, S, T, M, R, A>(
    endpoints: &AuthEndpoints<U, C, B, S, T, M, R, A>,
    request: Request,
) -> Response
where
    U: UserRepository,
    C: CredentialRepository,
    B: PasswordBlocklist,
    T: SingleUseTokenRepository,
    R: AttemptRepository,
    A: AuditSink,
{
    let correlation = correlation_of(&request);
    let body: dto::ResetRequest = match read_body(&request, correlation) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let now = endpoints.clock.now();

    let admitted = match endpoints
        .abuse
        .admit(
            AttemptFlow::ResetPassword,
            keys_for(&request, None, None),
            correlation,
            now,
        )
        .await
    {
        Ok(admitted) => admitted,
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    match endpoints
        .authentication
        .reset_password(
            admitted,
            &endpoints.resets,
            &body.token,
            &body.password,
            now,
        )
        .await
    {
        Ok(_user) => json(200, &dto::AcknowledgedResponse::new()),
        Err(error) => problem::render_service_error(&error, correlation),
    }
}

/// The endpoints, shared across handlers.
///
/// A named alias rather than the bare `Arc<AuthEndpoints<..>>`: eight type parameters behind an
/// `Arc` is genuinely hard to read at a call site, and clippy's `type_complexity` lint said so.
/// The alias is the fix rather than an `#[allow]`, because the lint was right.
pub type SharedEndpoints<U, C, B, S, T, M, R, A> = Arc<AuthEndpoints<U, C, B, S, T, M, R, A>>;

/// Builds the authentication route group.
///
/// # The seven bounded flows, and the two that are not
///
/// | Route | Bounded by |
/// |---|---|
/// | `POST /auth/login` | `AttemptFlow::LogIn` |
/// | `POST /auth/verification/resend` | `AttemptFlow::VerificationResend` |
/// | `POST /auth/verification/confirm` | `AttemptFlow::VerificationComplete` |
/// | `POST /auth/password/forgot` | `AttemptFlow::ForgotPassword` |
/// | `POST /auth/password/reset` | `AttemptFlow::ResetPassword` |
/// | `POST /auth/token/refresh` | `AttemptFlow::TokenRefresh` — see `token_routes`, which exists only under the `tokens` feature |
/// | `POST /auth/register` | `AttemptFlow::Register` — a **seventh** flow, added after review |
/// | `POST /auth/logout` | nothing — it can only revoke |
/// | `GET /auth/me` | nothing — it reads the caller's own account |
///
/// This is not a convention. Each of those handlers calls `AbuseGuard::admit` because the operation
/// it calls **takes an `Admitted`** — a type with no public constructor, no `Copy`, and no `Clone`,
/// so one counted attempt admits exactly one call.
///
/// # Errors
///
/// [`RouteError`] if a path or group name is invalid — a defect in this function, not a caller's
/// doing, since every path here is a literal.
pub fn routes<U, C, B, S, T, M, R, A>(
    endpoints: SharedEndpoints<U, C, B, S, T, M, R, A>,
) -> Result<RouteGroup, RouteError>
where
    U: UserRepository + Send + Sync + 'static,
    C: CredentialRepository + Send + Sync + 'static,
    B: PasswordBlocklist + Send + Sync + 'static,
    S: SessionRepository + Send + Sync + 'static,
    T: SingleUseTokenRepository + Send + Sync + 'static,
    M: MailPort + Send + Sync + 'static,
    R: AttemptRepository + Send + Sync + 'static,
    A: AuditSink + Send + Sync + 'static,
{
    // A macro rather than eight near-identical closures. Each route needs the same three lines of
    // `Arc` bookkeeping, and writing them out eight times is eight chances to clone the wrong one.
    macro_rules! route {
        ($group:expr, $method:ident, $path:literal, $handler:expr) => {{
            let shared = Arc::clone(&endpoints);
            $group.$method($path, move |request: Request| {
                let shared = Arc::clone(&shared);
                async move { $handler(&shared, request).await }
            })?
        }};
    }

    let group = RouteGroup::new("auth", "/auth")?;
    let group = route!(group, post, "/register", register);
    let group = route!(group, post, "/login", log_in);
    let group = route!(group, post, "/logout", log_out);
    let group = route!(group, get, "/me", current_user);
    let group = route!(group, post, "/verification/confirm", confirm_verification);
    let group = route!(group, post, "/password/reset", reset_password);

    // The two mailed flows share a handler and differ only in the flow they name.
    let shared = Arc::clone(&endpoints);
    let group = group.post("/verification/resend", move |request: Request| {
        let shared = Arc::clone(&shared);
        async move { mailed_flow(&shared, request, AttemptFlow::VerificationResend).await }
    })?;
    let shared = Arc::clone(&endpoints);
    let group = group.post("/password/forgot", move |request: Request| {
        let shared = Arc::clone(&shared);
        async move { mailed_flow(&shared, request, AttemptFlow::ForgotPassword).await }
    })?;

    Ok(group)
}

/// The token-mode endpoints, kept separate so [`AuthEndpoints`] has the same shape under every
/// feature.
///
/// A `#[cfg]`'d field on the main struct would have meant a construction site that compiles under
/// one feature and not the other — which is a build-configuration difference an author discovers
/// at the wrong moment.
#[cfg(feature = "tokens")]
pub struct TokenEndpoints<F, R, A> {
    /// The atomic refresh transition.
    pub rotation: renvor_auth::refresh::RefreshRotation<F>,
    /// The access-token issuer.
    pub issuer: renvor_auth::token::AccessTokenIssuer,
    /// The abuse controls. The same guard the other flows use.
    pub abuse: AbuseGuard<R, A>,
    /// The injected clock.
    pub clock: Arc<dyn Clock>,
    /// The entropy port. There is no other source of randomness here.
    pub entropy: Arc<dyn renvor_core::observe::EntropySource + Send + Sync>,
}

/// `POST /auth/token/refresh`. **Bounded** — `AttemptFlow::TokenRefresh`.
#[cfg(feature = "tokens")]
async fn refresh_tokens<F, R, A>(endpoints: &TokenEndpoints<F, R, A>, request: Request) -> Response
where
    F: renvor_auth::repository::RefreshTokenRepository,
    R: AttemptRepository,
    A: AuditSink,
{
    use renvor_auth::refresh::{RefreshOutcome, RefreshToken};

    let correlation = correlation_of(&request);
    let body: dto::RefreshRequest = match read_body(&request, correlation) {
        Ok(body) => body,
        Err(response) => return response,
    };
    let now = endpoints.clock.now();

    // THE CLIENT KEY IS THE TOKEN'S DIGEST, computed without touching the store — see
    // `RefreshToken::client_key`. Counting happens before anything reads a row, so a caller sending
    // arbitrary bytes cannot force a database lookup.
    let admitted = match endpoints
        .abuse
        .admit(
            AttemptFlow::TokenRefresh,
            keys_for(
                &request,
                None,
                Some(RefreshToken::client_key(&body.refresh_token)),
            ),
            correlation,
            now,
        )
        .await
    {
        Ok(admitted) => admitted,
        Err(error) => return problem::render_service_error(&error, correlation),
    };

    match endpoints
        .rotation
        .rotate(
            admitted,
            &body.refresh_token,
            &endpoints.issuer,
            endpoints.clock.as_ref(),
            endpoints.entropy.as_ref(),
        )
        .await
    {
        Ok(RefreshOutcome::Rotated(tokens)) => json(
            200,
            &dto::RotatedResponse {
                // THE TWO SECRETS THAT LEAVE, at the one call site that hands them over. Both
                // `expose` methods are named to be conspicuous for exactly this reason.
                access_token: tokens.access.expose().to_owned(),
                refresh_token: tokens.refresh.expose(),
                token_type: "Bearer",
            },
        ),
        // EVERY rejection is the same answer. `RefreshRejection` distinguishes replay from expiry
        // from an unknown token, and `revoked` counts what a replay killed — all of it operator
        // facing, none of it the presenter's business. A replay that reported "your family was
        // revoked" would confirm to a thief that they had held a real token.
        // `RefreshOutcome` is `#[non_exhaustive]`, so an added variant lands here too — and lands
        // on the REFUSING arm rather than a permitting one, which is the fail-closed direction.
        Ok(_) => problem::render_service_error(
            &ServiceError::Refused(AuthError::CredentialNoLongerValid),
            correlation,
        ),
        Err(error) => problem::render_service_error(&ServiceError::Refused(error), correlation),
    }
}

/// Builds the token-mode route group: `POST /auth/token/refresh`.
///
/// # Errors
///
/// [`RouteError`] if the path or group name is invalid — a defect here, since both are literals.
#[cfg(feature = "tokens")]
pub fn token_routes<F, R, A>(
    endpoints: Arc<TokenEndpoints<F, R, A>>,
) -> Result<RouteGroup, RouteError>
where
    F: renvor_auth::repository::RefreshTokenRepository + Send + Sync + 'static,
    R: AttemptRepository + Send + Sync + 'static,
    A: AuditSink + Send + Sync + 'static,
{
    let group = RouteGroup::new("auth-token", "/auth/token")?;
    let shared = Arc::clone(&endpoints);
    let group = group.post("/refresh", move |request: Request| {
        let shared = Arc::clone(&shared);
        async move { refresh_tokens(&shared, request).await }
    })?;
    Ok(group)
}
