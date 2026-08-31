//! Turning the registry into a real `axum::Router`.
//!
//! # This is the only place `axum` and Renvor meet
//!
//! Above this module, routes are Renvor values. Below it, they are an `axum::Router`. Nothing
//! Renvor hands to an application passes through here in the other direction.
//!
//! # The declared middleware order IS the nesting, not a sequence written out by hand
//!
//! Contract C-11 declares nine layers, outermost to innermost. An earlier revision implemented
//! layers 1–8 as statements inside one `dispatch` function attached to each **matched** endpoint.
//! That had a hole large enough to drive an attack through: the router's own 404 and 405 paths
//! never called `dispatch`, so a request for a path that does not exist was answered with **no**
//! host validation, **no** identity resolution, **no** request identifier, **no** CORS check, and
//! **no** admission control. Verified: `GET /missing` with `Host: evil.example` returned `404` with
//! no `x-request-id`.
//!
//! The controls are now **layers around the whole router**, so an unmatched path meets exactly the
//! same controls a matched one does:
//!
//! ```text
//!  1,2,3 context ......  request id · host validation · client identity · origin refusal
//!    │
//!    4 CORS ...........  the protocol, upstream's. A preflight is ANSWERED here
//!      │
//!      5,6,7 admission   concurrency ceiling · work gate · timeout
//!        │
//!        └── the router  matching · 404 fallback · 405 with `Allow`
//!              │
//!              8,9 ....  body limit · trace span · the handler, inside a panic boundary
//! ```
//!
//! Each layer wraps the one below it, so the order is a property of how the stack is assembled
//! rather than of the sequence someone typed. Reordering it means moving a `layer` call, which is
//! visible in review — and the adjacent-pair tests in `tests/lifecycle.rs` and `tests/controls.rs`
//! assert it by observable behaviour rather than by reading this comment.
//!
//! # There is no context fallback, deliberately
//!
//! A handler receives a [`RequestContext`] the outer layers resolved. If that context is absent,
//! this module answers **500** rather than building one of its own.
//!
//! Building one here would be a silent fallback: the request would be served with an identity
//! nothing validated, and every test would still pass. Refusing means a mis-assembled router fails
//! immediately and visibly, which is the only failure mode worth having.

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Request as AxumRequest, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
// Aliased: this crate declares its OWN `Next` for the transport-neutral group middleware
// seam, and the two are different things — one carries `axum` types, the other cannot.
use axum::middleware::{Next as AxumNext, from_fn_with_state};
use axum::response::Response as AxumResponse;
use axum::routing::{MethodFilter, on};
use futures_util::FutureExt;
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use tower_http::cors::{AllowHeaders, AllowMethods, AllowOrigin, CorsLayer};
use tracing::Instrument as _;

use super::{Method, Next, PresentedCredentials, Request, Response, RouteRegistry};
use crate::admission::Admission;
use crate::context::{RequestContext, RequestId};
use crate::cors::{CorsPolicy, CorsPolicyError};
use crate::error::{HttpError, HttpErrorDetail, HttpErrorKind};
use crate::host::{self, HostPolicy};
use crate::identity::{ForwardingHeaders, TrustedProxies, resolve};
use crate::limits::Limits;
use crate::problem;
use crate::request_id;

/// Everything the security layers need in order to resolve a request.
///
/// Assembled once, shared by every request. `CorsPolicy` is validated when this is built, so a
/// router cannot exist for a configuration that would be unsafe to serve.
#[derive(Clone)]
pub struct RouterConfig {
    /// The hosts this application answers for. Denies all by default.
    pub hosts: HostPolicy,
    /// The peers whose forwarding headers are honoured. Empty by default.
    pub trusted_proxies: TrustedProxies,
    /// Which origins may read responses. Denies all by default.
    pub cors: CorsPolicy,
    /// The documented resource bounds.
    pub limits: Limits,
    /// This application run's identifier.
    pub run_id: RunIdentifier,
    /// The application-wide cancellation scope. Every request scope is a child of it.
    pub cancel: CancelScope,
    /// The kernel work gate every request takes a permit from.
    pub gate: WorkGate,
    /// The application's typed state, reachable from a handler through [`super::Request::state`].
    ///
    /// Shared rather than owned: this is the **same** map the kernel's providers registered into,
    /// so a handler reads what a provider wrote rather than a copy that could diverge from it.
    pub state: Arc<TypedStateMap>,
}

impl RouterConfig {
    /// Assembles a configuration with the documented defaults, denying everything not named.
    ///
    /// # Errors
    ///
    /// Propagates entropy failure rather than falling back to a weaker source.
    pub fn new(
        cancel: CancelScope,
    ) -> Result<Self, renvor_core::observe::entropy::EntropyUnavailable> {
        Ok(Self {
            hosts: HostPolicy::deny_all(),
            trusted_proxies: TrustedProxies::none(),
            cors: CorsPolicy::deny_all(),
            limits: Limits::new(),
            run_id: RunIdentifier::generate(&OsEntropy)?,
            cancel,
            gate: WorkGate::new(),
            // Empty rather than absent. A lookup against it reports the type it could not find,
            // which is FR-013's requirement; an `Option` here would put a "was state configured
            // at all?" branch in front of every handler that reads it.
            state: Arc::new(TypedStateMap::new()),
        })
    }

    /// Checks the configuration is safe to serve.
    ///
    /// # Errors
    ///
    /// [`CorsPolicyError`] when the CORS policy could not be honoured safely. Checked **here**, so
    /// the failure happens while a router is being built rather than while a request is being
    /// served.
    pub const fn validate(&self) -> Result<(), CorsPolicyError> {
        self.cors.validate()
    }
}

/// Shared state every layer and every endpoint closure captures.
struct Shared {
    config: RouterConfig,
    admission: Admission,
}

/// Builds a real `axum::Router` from `registry`.
///
/// # Errors
///
/// [`CorsPolicyError`] if the configuration would be unsafe to serve. Refusing here is what makes
/// FR-022's "at configuration time" true.
pub fn router(registry: &RouteRegistry, config: RouterConfig) -> Result<Router, CorsPolicyError> {
    config.validate()?;

    let admission = Admission::new(config.gate.clone(), config.limits.max_concurrent_requests);
    let cors = cors_layer(&config.cors);
    let shared = Arc::new(Shared { config, admission });

    // The `Allow` header on a 405 is built by the router itself from the methods registered on
    // each path, so there is no second method list here to disagree with the registry. That the
    // header is actually emitted is asserted against the real router rather than assumed from the
    // library's documentation.
    let mut inner = Router::new();

    for (path, routes) in registry.by_path() {
        // Annotated because each endpoint closure has its own type; the accumulator needs the
        // concrete router type the `on` calls fold into.
        let mut method_router: Option<axum::routing::MethodRouter> = None;

        for route in routes {
            let handler = Arc::clone(&route.handler);
            // The group middleware this route captured at declaration. Cloning an `Arc<[_]>` is a
            // reference-count bump, so the chain is not copied per request.
            let middleware = Arc::clone(&route.middleware);
            // The route's OWN declaration, captured here so dispatch validates against the same
            // value `describe::document` publishes. Cloning an `Arc` is a reference-count bump; a
            // spec holds schemas and copying one per request would be a deep copy of each.
            let spec = route.spec.clone();
            let shared = Arc::clone(&shared);
            let path_owned = path.to_owned();

            let endpoint = move |request: AxumRequest| {
                let handler = Arc::clone(&handler);
                let middleware = Arc::clone(&middleware);
                let spec = spec.clone();
                let shared = Arc::clone(&shared);
                let path_owned = path_owned.clone();
                async move { dispatch(handler, middleware, spec, shared, path_owned, request).await }
            };

            let filter = method_filter(route.method);
            method_router = Some(match method_router {
                None => on(filter, endpoint),
                Some(existing) => existing.on(filter, endpoint),
            });
        }

        if let Some(method_router) = method_router {
            inner = inner.route(path, method_router);
        }
    }

    // The 404, expressed as a route rather than left to the library's default, so that it is a
    // response this crate produced and can be reasoned about. It carries no identifier of its own:
    // the outermost layer attaches that to EVERY response, which is what makes "every request is
    // correlatable" true rather than true-for-matched-routes.
    let inner = inner.fallback(not_found);

    // `.fallback_service(inner)` routes EVERY request into the inner router, so the layers below
    // wrap matching itself rather than wrapping the endpoints matching selected. That is the whole
    // difference between "the controls run for declared routes" and "the controls run".
    //
    // The FIRST `.layer` call is the innermost, so this reads bottom-up: admission, then CORS,
    // then context.
    Ok(Router::new()
        .fallback_service(inner)
        .layer(from_fn_with_state(Arc::clone(&shared), admit_and_bound))
        .layer(cors)
        .layer(from_fn_with_state(shared, establish_context)))
}

/// Translates a validated [`CorsPolicy`] into the upstream layer that speaks the protocol.
///
/// # Renvor validates; upstream implements
///
/// ADR-0012 Finding 2 records the split precisely: Renvor took over **validation** because the
/// upstream check is an `assert!` reachable from `poll_ready`, i.e. while serving. The protocol —
/// which headers to emit, how to answer a preflight, what to put in `Vary` — stays upstream, where
/// it belongs. Renvor writes none of it.
///
/// That split is only sound because [`RouterConfig::validate`] refuses every configuration the
/// upstream assertion would fire on. `mirror_request` is deliberately used for methods and headers
/// rather than `any()`: `any()` is the literal `*`, which upstream refuses to combine with
/// credentials, and mirroring is not a wildcard.
fn cors_layer(policy: &CorsPolicy) -> CorsLayer {
    let origins = if policy.is_wildcard() {
        AllowOrigin::any()
    } else {
        AllowOrigin::list(
            policy
                .origins()
                // An origin that cannot be a header value cannot have been accepted by
                // `allow_origin`, which refuses control characters — so this filter drops nothing
                // in practice and exists so that a future loosening there cannot panic here.
                .filter_map(|origin| HeaderValue::from_str(origin).ok()),
        )
    };

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(AllowMethods::mirror_request())
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(policy.credentials_allowed())
}

/// Whether the router will accept `path` as a route pattern.
///
/// # This asks the router rather than re-implementing its grammar
///
/// `validate_path` catches what can be stated in a sentence — empty, unrooted, control characters.
/// It cannot catch the rest, and the rest is not obvious. Measured against the shipped router:
///
/// ```text
///   /x{a}      accepted            /{a}x        REJECTED
///   /{{a}}     accepted            /{}          REJECTED
///   /{*rest}   accepted            /{*rest}/x   REJECTED  (a catch-all must be last)
///   /a*b       accepted            /*           REJECTED
///   /a:b       accepted            /:old        REJECTED  (the pre-0.8 spelling)
/// ```
///
/// A hand-written grammar matching that would be a second implementation of the router's matcher,
/// able to drift from it in both directions: accepting a pattern the router then panics on, or
/// refusing one the router would have taken. ADR-0012's whole argument is that Renvor writes a
/// primitive only where the maintained one is unfit — the matcher is fit, so the question is put
/// to it directly.
///
/// # The panic is contained here so it is not a panic later
///
/// The router signals a bad pattern by panicking during construction. Left alone, that turns an
/// author's typo into a process abort with a backtrace, at a point where the route already looked
/// registered. Catching it at **registration** converts it into the reported error contract C-9
/// requires.
///
/// The panic hook is deliberately **not** suppressed: swapping it is process-global and would race
/// with any other thread's genuine panic. The library's own message is printed alongside Renvor's
/// error, which is more information rather than less.
pub(crate) fn accepts_pattern(path: &str) -> bool {
    let path = path.to_owned();
    std::panic::catch_unwind(move || {
        let _: Router = Router::new().route(&path, axum::routing::get(|| async {}));
    })
    .is_ok()
}

/// Maps a Renvor method onto the router's filter. Total by construction.
const fn method_filter(method: Method) -> MethodFilter {
    match method {
        Method::Get => MethodFilter::GET,
        Method::Post => MethodFilter::POST,
        Method::Put => MethodFilter::PUT,
        Method::Patch => MethodFilter::PATCH,
        Method::Delete => MethodFilter::DELETE,
        Method::Head => MethodFilter::HEAD,
        Method::Options => MethodFilter::OPTIONS,
    }
}

/// The answer for a path no route declares.
///
/// # It reads the context rather than answering blind
///
/// Phase 004 answered this as plain text with no correlation, because a plain string needs
/// nothing. An RFC 9457 document carries a correlation identifier, so this reads the one the
/// outermost layer already resolved — the SAME `RequestId` that becomes the `x-request-id` header
/// and the telemetry field.
///
/// A context that is absent means a mis-assembled router, and the document is emitted with the
/// documented placeholder rather than a fabricated identifier: inventing one would let an operator
/// search for a request that never had that identity.
async fn not_found(request: AxumRequest) -> AxumResponse {
    let request_id = request
        .extensions()
        .get::<RequestContext>()
        .map(RequestContext::request_id);

    problem::from_http_error(
        &HttpError::new(HttpErrorKind::NotFound, HttpErrorDetail::NoRouteDeclared),
        request_id,
    )
}

/// **Layers 1, 2, 3 — and the origin refusal.** The outermost layer.
///
/// Everything below it, including the router's own 404 and 405, runs inside this — which is what
/// makes every response correlatable and every request host-validated.
///
/// # Why the identifier is attached on the way OUT
///
/// It is attached to whatever response came back, from wherever it came back: a handler's, a 404,
/// a 405, a CORS preflight, a 503 from admission, a 408 from the timeout. One insertion site means
/// there is no response shape that can be added later and quietly miss it.
async fn establish_context(
    State(shared): State<Arc<Shared>>,
    request: AxumRequest,
    next: AxumNext,
) -> AxumResponse {
    // 1. REQUEST ID — first, so every rejection below is correlatable.
    let Ok(request_id) = request_id::generate(&OsEntropy) else {
        // The one response that genuinely cannot carry an identifier: generating it is what
        // failed. Refused rather than served uncorrelated, per C-11's fail-closed table.
        return refuse(
            &HttpError::new(
                HttpErrorKind::HandlerFailed,
                HttpErrorDetail::EntropyUnavailable,
            ),
            None,
            shared.config.run_id,
        );
    };

    let mut response = resolve_and_run(&shared, request_id, request, next).await;

    // ONE INSERTION SITE, and it is on the way out of the OUTERMOST layer — so it covers every
    // response shape without exception: a handler's, a 404, a 405, a CORS preflight, a 503 from
    // admission, a 408 from the timeout, and every refusal below.
    //
    // An earlier revision attached it only on the `next.run` path, and every early refusal in this
    // very function therefore returned without one. That is the same class of defect as the
    // fallback bypass this layering exists to close, reintroduced one level in — which is why the
    // insertion is here rather than at each `return`.
    //
    // INSERT, NOT APPEND. A handler that set this header of its own would otherwise produce TWO
    // values with its own first, and the contract says the GENERATED identifier is what appears.
    if let Ok(value) = request_id.encode().parse() {
        response
            .headers_mut()
            .insert(request_id::REQUEST_ID_HEADER, value);
    }

    response
}

/// Layers 2 and 3, the origin refusal, and the call inward.
///
/// Split from [`establish_context`] so that **every** exit — including each early refusal — passes
/// back through one place that attaches the request identifier.
async fn resolve_and_run(
    shared: &Arc<Shared>,
    request_id: RequestId,
    mut request: AxumRequest,
    next: AxumNext,
) -> AxumResponse {
    let run_id = shared.config.run_id;

    // 2. HOST — fail closed before any work.
    //
    // Read through `header_values`, which KEEPS an undecodable value as a placeholder no parser
    // accepts instead of dropping it. This path previously used `filter_map(|v| v.to_str().ok())`
    // and so counted what survived the filter rather than what arrived: two Host headers, exactly
    // one of them decodable, collapsed to one and the repeated-header refusal never fired.
    //
    // That is the same defect this crate already found and fixed for forwarding headers one
    // function away; the Host path did not get the fix. Renvor would have validated and authorised
    // on the decodable name while a fronting hop resolving the repeated header to the last value
    // used a different one — each hop picking a different value and both believing they validated,
    // which is precisely what the rule exists to prevent. Found by review.
    let host_values = header_values(request.headers(), "host");
    let host_values: Vec<&str> = host_values.iter().map(String::as_str).collect();

    let Some(single) = host::single_host(&host_values) else {
        return refuse(
            &HttpError::new(
                HttpErrorKind::HostRejected,
                HttpErrorDetail::HostHeaderAbsentOrRepeated,
            ),
            Some(request_id),
            run_id,
        );
    };
    let Some(validated_host) = shared.config.hosts.validate(Some(single)) else {
        return refuse(
            &HttpError::new(
                HttpErrorKind::HostRejected,
                HttpErrorDetail::HostNotConfigured,
            ),
            Some(request_id),
            run_id,
        );
    };

    // 3. CLIENT IDENTITY — the peer is the socket, never a header.
    let Some(peer) = peer_address(request.extensions()) else {
        // Fail closed: without a peer there is no fact to fall back to, and inventing one would
        // make every downstream decision rest on a value nothing observed.
        return refuse(
            &HttpError::new(
                HttpErrorKind::StateUnavailable,
                HttpErrorDetail::ConnectInfoMissing,
            ),
            Some(request_id),
            run_id,
        );
    };

    let forwarded = header_values(request.headers(), "forwarded");
    let xff = header_values(request.headers(), "x-forwarded-for");
    let forwarded_refs: Vec<&str> = forwarded.iter().map(String::as_str).collect();
    let xff_refs: Vec<&str> = xff.iter().map(String::as_str).collect();

    let client = resolve(
        peer,
        &shared.config.trusted_proxies,
        ForwardingHeaders {
            forwarded: &forwarded_refs,
            x_forwarded_for: &xff_refs,
        },
    );

    // ORIGIN REFUSAL — an Origin that is present, CROSS-origin, and not permitted is refused.
    //
    // THIS IS STRICTER THAN CORS, AND DELIBERATELY SO.
    //
    // CORS as specified is enforced by the browser: a server emits headers and the browser decides
    // what the page may read. Renvor additionally refuses the request outright, so a disallowed
    // origin gets no response body to leak even to a non-browser caller. That is the deny-first
    // posture constitution principle VI requires, and it is why this check sits here rather than
    // being left entirely to the layer below.
    //
    // THE SAME-ORIGIN CARVE-OUT IS NOT A LOOSENING.
    //
    // Browsers send `Origin` on same-origin `POST`, `PUT`, `PATCH`, and `DELETE` as well as on
    // cross-origin requests. Without this check a default-configured application answered **400**
    // to a form post from its own page — the CORS policy denies by default, and every same-origin
    // write carried an Origin it had not been told to allow.
    //
    // A request whose Origin matches the host it was addressed to is by definition not
    // cross-origin, and CORS governs cross-origin access only. Comparing against `validated_host`
    // — the value host validation already accepted — rather than against the raw header is what
    // keeps this from becoming a bypass: an attacker cannot satisfy it without also satisfying
    // host validation.
    // An `Origin` that is PRESENT but undecodable is refused rather than skipped. The earlier form
    // used `.and_then(|value| value.to_str().ok())`, so a value carrying non-ASCII bytes made the
    // binding fail and the whole check was passed over — a fail-OPEN, in a table (C-11) that says
    // this control fails closed. Found by review.
    let origin = match request.headers().get(header::ORIGIN) {
        None => None,
        Some(value) => match value.to_str() {
            Ok(origin) => Some(origin),
            Err(_) => {
                return refuse(
                    &HttpError::new(
                        HttpErrorKind::OriginRejected,
                        HttpErrorDetail::OriginUnreadable,
                    ),
                    Some(request_id),
                    run_id,
                );
            }
        },
    };

    if let Some(origin) = origin
        && !is_same_origin(origin, &validated_host)
        && !shared.config.cors.allows(origin)
    {
        return refuse(
            &HttpError::new(
                HttpErrorKind::OriginRejected,
                HttpErrorDetail::OriginNotConfigured,
            ),
            Some(request_id),
            run_id,
        );
    }

    let scope = shared.config.cancel.child(format!("request:{request_id}"));
    let context = RequestContext::new(run_id, request_id, client, validated_host, scope.clone());

    // CLIENT DISCONNECT CANCELS THE REQUEST SCOPE.
    //
    // A disconnected client is a dropped future, and a dropped future runs `Drop`. Arming a guard
    // here is what makes contract C-10's *"client disconnect and request timeout both cancel that
    // scope"* true: previously only the timeout branch cancelled, so a service holding a clone of
    // the scope never learned that the caller had gone.
    let mut cancel_on_drop = CancelOnDrop { scope, armed: true };

    request.extensions_mut().insert(context);
    let response = next.run(request).await;

    // The response was produced, so this request was not abandoned.
    cancel_on_drop.armed = false;
    response
}

/// **Layers 5, 6, 7.** Admission, then the bound on everything inside it.
///
/// # The timeout is outside the body read, and that is the fix
///
/// An earlier revision read the request body and *then* wrapped only the handler in a timeout. A
/// client that opened a request and delivered its body one byte at a time therefore held a work
/// permit and a concurrency slot with **no bound at all** — the documented 30-second timeout could
/// not fire, because it had not started.
///
/// C-11 places the timeout at layer 7 and the body limit at layer 8, i.e. the timeout **outside**
/// the body. Nesting it here makes that true: everything inner — routing, the body read, the
/// handler — is inside one deadline.
async fn admit_and_bound(
    State(shared): State<Arc<Shared>>,
    request: AxumRequest,
    next: AxumNext,
) -> AxumResponse {
    let Some(context) = request.extensions().get::<RequestContext>().cloned() else {
        return internal_error();
    };
    let request_id = context.request_id();

    // 5 and 6. CONCURRENCY CEILING, then WORK GATE ADMISSION.
    //
    // The guard is bound to a name that lives until the end of this function, so it is released on
    // EVERY exit below — including the timeout path and including a panic. Binding it to `_` would
    // drop it here and admit an unbounded number of requests while reporting that it did not.
    let _admitted = match shared.admission.admit() {
        Ok(guard) => guard,
        Err(error) => return refuse(&error, Some(request_id), shared.config.run_id),
    };

    // 7. TIMEOUT — inside admission, so a timed-out request still releases its permit, and the
    // request's own scope is CANCELLED rather than the future merely being dropped. An application
    // service watching for cancellation therefore observes a timeout through the same mechanism it
    // observes a client disconnect, rather than through two.
    match tokio::time::timeout(shared.config.limits.request_timeout, next.run(request)).await {
        Ok(response) => response,
        Err(_) => {
            context.cancel_scope().cancel();
            refuse(
                &HttpError::new(HttpErrorKind::TimedOut, HttpErrorDetail::TimeoutElapsed),
                Some(request_id),
                shared.config.run_id,
            )
        }
    }
}

/// **Layers 8 and 9.** The body limit, the span, and the handler inside a panic boundary.
async fn dispatch(
    handler: Arc<dyn super::Handler>,
    middleware: Arc<[Arc<dyn super::Middleware>]>,
    spec: Option<Arc<super::OperationSpec>>,
    shared: Arc<Shared>,
    path: String,
    request: AxumRequest,
) -> AxumResponse {
    let (mut parts, body) = request.into_parts();

    let Some(context) = parts.extensions.get::<RequestContext>().cloned() else {
        // See the module documentation: a context this module did not receive is a mis-assembled
        // router, and inventing one would serve the request with an identity nothing validated.
        return internal_error();
    };
    let request_id = context.request_id();
    let run_id = context.run_id();

    // PATH PARAMETERS, from the router that matched them. Previously an empty map was passed
    // unconditionally, so `path_param` returned `None` for every parameter of every route.
    let path_params = captured_parameters(&mut parts).await;

    // 8. BODY LIMIT — innermost bound, applied to the bytes a handler will read.
    let collected = match axum::body::to_bytes(body, shared.config.limits.max_body_bytes).await {
        Ok(bytes) => bytes.to_vec(),
        Err(error) => {
            // Over-limit and "the connection failed while reading" are different events and are
            // reported differently. Reporting an IO failure as `413` told an operator their client
            // had sent too much when it had in fact gone away.
            let over_limit = error
                .to_string()
                .to_ascii_lowercase()
                .contains("length limit");
            let reported = if over_limit {
                HttpError::new(
                    HttpErrorKind::BodyTooLarge,
                    HttpErrorDetail::BodyLimitExceeded,
                )
            } else {
                HttpError::new(
                    HttpErrorKind::BodyUnreadable,
                    HttpErrorDetail::BodyReadInterrupted,
                )
            };
            return refuse(&reported, Some(request_id), run_id);
        }
    };

    let query = parts.uri.query().unwrap_or_default().to_owned();

    // THE TWO CREDENTIAL HEADERS, and no others.
    //
    // Read one at a time by name rather than from the full map the declared-constraint check
    // builds below — that map is scoped to the validation block, and more importantly, taking two
    // named values is a different act from handing a handler every header. `PresentedCredentials`
    // records why a credential the application must validate is not the identity this layer
    // resolved.
    //
    // A header whose bytes are not text becomes `""`, which presents nothing. That is the
    // fail-closed direction: an unreadable credential refuses rather than being lossily repaired
    // into one that might parse.
    let credentials = PresentedCredentials::new(
        parts
            .headers
            .get(header::COOKIE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default(),
        parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default(),
    );

    // VALIDATION, against the route's OWN declaration — the same value the description publishes.
    //
    // Before the handler, deliberately: an operation that declared its inputs should never receive
    // one it declared invalid, and a handler that had to re-check would be a second rule that can
    // disagree with the published one.
    //
    // A route with no declaration is not validated and not refused. It declares nothing, so there
    // is nothing to enforce — and inventing a rule for it would enforce something the description
    // does not publish.
    if let Some(declared) = spec.as_deref() {
        let headers: BTreeMap<String, String> = parts
            .headers
            .iter()
            .filter_map(|(name, value)| {
                // A header whose bytes are not text is skipped rather than lossily converted: a
                // lossy conversion invents a value, and a declared header that is absent from this
                // map is reported as missing, which is the truthful outcome.
                value
                    .to_str()
                    .ok()
                    .map(|text| (name.as_str().to_ascii_lowercase(), text.to_owned()))
            })
            .collect();

        if let Err(rejection) = declared.validate(&path_params, &query, &headers, &collected) {
            tracing::info!(
                route = %path,
                request_id = %request_id,
                run_id = %run_id,
                code = rejection.code().as_str(),
                // The ISSUE COUNT, never the issues themselves and never the rejected values. A
                // count is an operational fact; the values are caller data.
                issues = rejection_issue_count(&rejection),
                "request refused by its declared constraints"
            );
            return problem::from_rejection(&rejection, Some(request_id));
        }
    }

    let renvor_request = Request::new(context, collected, query, path_params)
        .with_state(Arc::clone(&shared.config.state))
        .with_credentials(credentials);

    // 9. TRACE — nearest the handler, so the span covers handler execution. Structured fields
    // only, never an interpolated sentence, and every field is one Renvor generated.
    let span = tracing::info_span!(
        "renvor.http.handler",
        route = %path,
        request_id = %request_id,
        run_id = %run_id,
    );

    // THE PANIC BOUNDARY. Contract C-10: *"a handler panic is caught, contained, and reported as a
    // failure. It is never a hang, and its payload never reaches a response."*
    //
    // `std::panic::catch_unwind` cannot do this — a future panics across a `poll`, not inside one
    // call — so the maintained future-aware version is used. `AssertUnwindSafe` is sound here
    // because nothing observable is shared across the boundary: the response is discarded on a
    // panic, and the admission guard releases through unwinding exactly as it does through a
    // return.
    // THE GROUP MIDDLEWARE CHAIN, inside the panic boundary and inside the span.
    //
    // Inside, because a middleware is author code exactly as a handler is: one that panics must be
    // contained on the same terms, and one that does work worth tracing must appear under the same
    // span. A chain built outside the boundary would leave a panicking middleware uncontained,
    // which is the failure the boundary exists to prevent.
    let chain = Next::new(middleware, handler);

    // `.instrument(span)` rather than `span.enter()`.
    //
    // `Span::enter` returns a thread-local RAII guard, and holding one across an `.await` is the
    // hazard `Instrument` exists to prevent: the future yields with the span still on that worker
    // thread's stack, the executor polls a DIFFERENT request on the same thread, and that
    // request's events are parented to this one — carrying the wrong `request_id`. When this
    // future resumes on another worker the guard exits a span that thread never entered.
    //
    // Contract C-11 says *"the generated identifier is what appears in the response and in
    // telemetry"*. Mis-parenting breaks that for everything the application handler logs, which is
    // the only reason this span exists. Found by review.
    match AssertUnwindSafe(chain.run(renvor_request))
        .catch_unwind()
        .instrument(span)
        .await
    {
        Ok(response) => into_axum(response, request_id, run_id),
        Err(_) => {
            // The payload is NOT logged and NOT rendered. It is author data of unknown
            // sensitivity — a panic message routinely contains the value that caused it.
            tracing::error!(
                route = %path,
                request_id = %request_id,
                run_id = %run_id,
                "a handler or group middleware panicked; the request was failed and the payload \
                 discarded"
            );
            refuse(
                &HttpError::new(
                    HttpErrorKind::HandlerFailed,
                    HttpErrorDetail::HandlerPanicked,
                ),
                Some(request_id),
                run_id,
            )
        }
    }
}

/// The parameters the router captured for this request.
///
/// Read through the library's own extractor rather than by re-parsing the path, so the values are
/// the ones the router matched rather than a second interpretation that could differ from it.
async fn captured_parameters(parts: &mut axum::http::request::Parts) -> BTreeMap<String, String> {
    use axum::extract::{FromRequestParts, RawPathParams};

    RawPathParams::from_request_parts(parts, &())
        .await
        .map(|params| {
            params
                .iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads every value of a header by name, **preserving unreadable ones as unusable**.
///
/// # Why a value that cannot be decoded is not simply skipped
///
/// The earlier form used `filter_map(|v| v.to_str().ok())`, which **silently dropped** a value with
/// non-UTF-8 bytes. Two headers where one was malformed therefore collapsed into a single readable
/// one — and the repeated-header refusal, which exists precisely so that two hops cannot resolve a
/// repeated header differently, never fired.
///
/// A value that cannot be decoded is now kept as a placeholder that no parser accepts, so the
/// **count** stays truthful and the repeated-header rule still applies. Found by review.
fn header_values(headers: &HeaderMap, name: &str) -> Vec<String> {
    headers
        .get_all(name)
        .iter()
        .map(|value| {
            value.to_str().map_or_else(
                // A NUL can never appear in a legitimate forwarding value, and every parser in
                // this crate refuses a control character — so this placeholder is unusable by
                // construction rather than by a rule someone has to remember.
                |_| "\0unreadable".to_owned(),
                str::to_owned,
            )
        })
        .collect()
}

/// The socket address, from connection information the server loop attached.
fn peer_address(extensions: &axum::http::Extensions) -> Option<IpAddr> {
    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip())
}

/// Cancels the request scope unless the request produced a response.
///
/// A guard rather than an explicit call, for the reason every guard in this crate exists: the path
/// that forgets is the one nobody wrote — here, the future being dropped because the client hung
/// up, which has no statement of its own to attach a call to.
struct CancelOnDrop {
    scope: CancelScope,
    armed: bool,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if self.armed {
            self.scope.cancel();
        }
    }
}

/// Renders a Renvor response.
fn into_axum(response: Response, request_id: RequestId, run_id: RunIdentifier) -> AxumResponse {
    let mut builder = AxumResponse::builder().status(response.status_code());

    for (name, value) in response.headers() {
        builder = builder.header(name.as_str(), value.as_str());
    }

    match builder.body(Body::from(response.body().to_vec())) {
        Ok(rendered) => rendered,
        Err(error) => {
            // Recorded rather than silent. This used to substitute a bare 500 with NO log line, so
            // a handler returning an unrepresentable header name lost its entire response — status,
            // body, and identifier — with nothing to diagnose it by.
            tracing::error!(
                request_id = %request_id,
                run_id = %run_id,
                reason = %error,
                "a handler response could not be rendered"
            );
            internal_error()
        }
    }
}

/// Whether `origin` addresses the same host the request was addressed to.
///
/// Compared against the **validated** host, so this cannot be satisfied without first satisfying
/// host validation. Scheme and port are part of an origin and are deliberately ignored here: a
/// request that reached this application on this host is same-origin for the purpose of deciding
/// whether CORS governs it at all.
fn is_same_origin(origin: &str, validated_host: &str) -> bool {
    origin
        .split_once("://")
        .map(|(_, authority)| authority)
        .and_then(|authority| {
            // Strip a port, leaving a bracketed IPv6 literal intact.
            authority.strip_prefix('[').map_or_else(
                || authority.split(':').next(),
                |rest| rest.find(']').map(|close| &authority[..=close + 1]),
            )
        })
        .is_some_and(|host| host.eq_ignore_ascii_case(validated_host))
}

/// A `500` with no body, for a failure that must not describe itself.
fn internal_error() -> AxumResponse {
    plain(StatusCode::INTERNAL_SERVER_ERROR, String::new())
}

/// Builds a plain-text response, or an empty `500` if even that could not be built.
fn plain(status: StatusCode, body: String) -> AxumResponse {
    AxumResponse::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| {
            AxumResponse::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("an empty 500 always builds")
        })
}

/// Renders a refusal, carrying only what a caller is allowed to know.
///
/// `error.detail()` is deliberately **not** put in the body. It exists for telemetry, and the body
/// is built from [`HttpError::public_message`], which never receives it.
///
/// # Why the run identifier is on every record
///
/// Contract C-O3 nests the request identifier beneath the run identifier. A refusal record that
/// carried only the request identifier could not be correlated to the run that produced it, which
/// is the first question asked of a refusal seen in aggregate.
///
/// `request_id` is `None` only when the failure was the identifier's own generation.
fn refuse(error: &HttpError, request_id: Option<RequestId>, run_id: RunIdentifier) -> AxumResponse {
    match request_id {
        Some(id) => tracing::warn!(
            code = error.kind().code(),
            detail = error.detail().as_str(),
            request_id = %id,
            run_id = %run_id,
            "request refused"
        ),
        None => tracing::warn!(
            code = error.kind().code(),
            detail = error.detail().as_str(),
            run_id = %run_id,
            "request refused before an identifier could be generated"
        ),
    }

    // RFC 9457, not plain text. Phase 004 answered a refusal with `error.public_message()`; a
    // machine-readable failure is what Phase 005 promises, and the media type is how a consumer
    // tells a problem document from a payload.
    //
    // `error.detail()` is deliberately NOT passed. It is the operator-facing string, it went to
    // the telemetry record above, and `problem::from_http_error` cannot read it — the document's
    // `detail` comes from the code, as a `&'static str`.
    //
    // The identifier is NOT attached as a header here. The outermost layer attaches it to every
    // response, including this one, so there is one insertion site rather than one per refusal
    // path.
    problem::from_http_error(error, request_id)
}

/// How many constraints a rejection reported.
///
/// A count, never the issues. `tracing` fields reach a log, and a log is a place a rejected value
/// must not be — contract C-E3's redaction rule does not stop at the response boundary.
fn rejection_issue_count(rejection: &super::RequestRejection) -> usize {
    match rejection {
        super::RequestRejection::Invalid(issues) => issues.len(),
        _ => 0,
    }
}
