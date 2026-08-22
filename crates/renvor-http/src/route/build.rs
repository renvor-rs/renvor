//! Turning the registry into a real `axum::Router`.
//!
//! # This is the only place `axum` and Renvor meet
//!
//! Above this module, routes are Renvor values. Below it, they are an `axum::Router`. Nothing
//! Renvor hands to an application passes through here in the other direction.
//!
//! # There is no context fallback, deliberately
//!
//! A handler receives a [`RequestContext`] that the security layers resolved. If that context is
//! absent, this module answers **500** rather than building a context of its own.
//!
//! Building one here would be a silent fallback: the request would be served with an identity
//! nothing validated, and every test would still pass. Refusing means a mis-assembled router fails
//! immediately and visibly, which is the only failure mode worth having.

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{ConnectInfo, Request as AxumRequest};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response as AxumResponse;
use axum::routing::{MethodFilter, on};
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, WorkGate};

use super::{Method, Request, Response, RouteRegistry};
use crate::admission::Admission;
use crate::context::{RequestContext, RequestId};
use crate::cors::{CorsPolicy, CorsPolicyError};
use crate::error::{HttpError, HttpErrorKind};
use crate::host::{self, HostPolicy};
use crate::identity::{ForwardingHeaders, TrustedProxies, resolve};
use crate::limits::Limits;
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

/// Shared state every route handler closure captures.
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

    // The `Allow` header on a 405 is built by the router itself from the methods registered on
    // each path, so there is no second method list here to disagree with the registry. That the
    // header is actually emitted is asserted against the real router in `tests/router.rs` rather
    // than assumed from the library's documentation.
    let admission = Admission::new(config.gate.clone(), config.limits.max_concurrent_requests);
    let shared = Arc::new(Shared { config, admission });

    let mut router = Router::new();

    for (path, routes) in registry.by_path() {
        // Annotated because each endpoint closure has its own type; the accumulator needs the
        // concrete router type the `on` calls fold into.
        let mut method_router: Option<axum::routing::MethodRouter> = None;

        for route in routes {
            let handler = Arc::clone(&route.handler);
            let shared = Arc::clone(&shared);
            let path_owned = path.to_owned();

            let endpoint = move |request: AxumRequest| {
                let handler = Arc::clone(&handler);
                let shared = Arc::clone(&shared);
                let path_owned = path_owned.clone();
                async move { dispatch(handler, shared, path_owned, request).await }
            };

            let filter = method_filter(route.method);
            method_router = Some(match method_router {
                None => on(filter, endpoint),
                Some(existing) => existing.on(filter, endpoint),
            });
        }

        if let Some(method_router) = method_router {
            router = router.route(path, method_router);
        }
    }

    Ok(router)
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

/// Resolves identity, builds the context, and calls the handler.
///
/// The order here **is** the documented middleware order for layers 1–4, and it is fixed in code
/// rather than assembled from a list, because these four are Renvor's own and there is no reason
/// for their relative order to be configurable.
async fn dispatch(
    handler: Arc<dyn super::Handler>,
    shared: Arc<Shared>,
    path: String,
    request: AxumRequest,
) -> AxumResponse {
    let (parts, body) = request.into_parts();

    // 1. REQUEST ID — first, so every rejection below is correlatable.
    let Ok(request_id) = request_id::generate(&OsEntropy) else {
        return refuse(
            &HttpError::new(
                HttpErrorKind::HandlerFailed,
                "entropy unavailable for request-identifier generation",
            ),
            None,
        );
    };

    // 2. HOST — fail closed before any work.
    let host_values: Vec<&str> = parts
        .headers
        .get_all(header::HOST)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .collect();

    let Some(single) = host::single_host(&host_values) else {
        return refuse(
            &HttpError::new(
                HttpErrorKind::HostRejected,
                "absent, or more than one Host header",
            ),
            Some(request_id),
        );
    };
    let Some(validated_host) = shared.config.hosts.validate(Some(single)) else {
        return refuse(
            &HttpError::new(
                HttpErrorKind::HostRejected,
                "host is not in the configured set",
            ),
            Some(request_id),
        );
    };

    // 3. CLIENT IDENTITY — the peer is the socket, never a header.
    let Some(peer) = peer_address(&parts.extensions) else {
        // Fail closed: without a peer there is no fact to fall back to, and inventing one would
        // make every downstream decision rest on a value nothing observed.
        return refuse(
            &HttpError::new(
                HttpErrorKind::StateUnavailable,
                "no connection information; the router was not served with connect info",
            ),
            Some(request_id),
        );
    };

    let forwarded = header_values(&parts.headers, "forwarded");
    let xff = header_values(&parts.headers, "x-forwarded-for");
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

    // 4. CORS — an Origin that is present and not permitted is refused.
    if let Some(origin) = parts
        .headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        && !shared.config.cors.allows(origin)
    {
        return refuse(
            &HttpError::new(
                HttpErrorKind::OriginRejected,
                "origin is not in the configured set",
            ),
            Some(request_id),
        );
    }

    // 5 and 6. CONCURRENCY CEILING, then WORK GATE ADMISSION.
    //
    // The guard is bound to a name that lives until the end of this function, so it is released on
    // EVERY exit below — including the timeout path and including a panic. Binding it to `_` would
    // drop it here and admit an unbounded number of requests while reporting that it did not.
    let _admitted = match shared.admission.admit() {
        Ok(guard) => guard,
        Err(error) => return refuse(&error, Some(request_id)),
    };

    let scope = shared.config.cancel.child(format!("request:{request_id}"));
    let context = RequestContext::new(
        shared.config.run_id,
        request_id,
        client,
        validated_host,
        scope.clone(),
    );

    // 8. BODY LIMIT — innermost bound, applied to the bytes a handler will read.
    let collected = match axum::body::to_bytes(body, shared.config.limits.max_body_bytes).await {
        Ok(bytes) => bytes.to_vec(),
        Err(_) => {
            return refuse(
                &HttpError::new(
                    HttpErrorKind::BodyTooLarge,
                    "request body exceeded the configured limit",
                ),
                Some(request_id),
            );
        }
    };

    let query = parts.uri.query().unwrap_or_default().to_owned();
    let renvor_request = Request::new(context, collected, query, BTreeMap::new());

    // The path is captured for telemetry; structured fields only, never an interpolated sentence.
    tracing::debug!(
        route = %path,
        request_id = %request_id,
        run_id = %shared.config.run_id,
        "dispatching"
    );

    // 7. TIMEOUT — inside admission, so a timed-out request still releases its permit, and the
    // request's own scope is CANCELLED rather than the future merely being dropped. An application
    // service watching for cancellation therefore observes a timeout through the same mechanism it
    // observes a client disconnect, rather than through two.
    let timeout = shared.config.limits.request_timeout;
    match tokio::time::timeout(timeout, handler.call(renvor_request)).await {
        Ok(response) => into_axum(response, request_id),
        Err(_) => {
            scope.cancel();
            refuse(
                &HttpError::new(
                    HttpErrorKind::TimedOut,
                    "request exceeded the configured timeout",
                ),
                Some(request_id),
            )
        }
    }
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

/// Renders a Renvor response, attaching the generated request identifier.
fn into_axum(response: Response, request_id: RequestId) -> AxumResponse {
    let mut builder = AxumResponse::builder().status(response.status_code());

    for (name, value) in response.headers() {
        builder = builder.header(name.as_str(), value.as_str());
    }

    // The GENERATED identifier is published. An inbound value never reaches here — there is no
    // path that carries one this far.
    builder = builder.header(request_id::REQUEST_ID_HEADER, request_id.encode());

    builder
        .body(Body::from(response.body().to_vec()))
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
/// # Why the identifier is attached here too
///
/// A rejection that cannot be correlated is an incident with no trail. The identifier layer is
/// outermost precisely so that every refusal below it can carry one, and a refusal path that
/// dropped it would make that ordering pointless for the responses that matter most.
///
/// `request_id` is `None` only when the failure was the identifier's own generation.
fn refuse(error: &HttpError, request_id: Option<RequestId>) -> AxumResponse {
    match request_id {
        Some(id) => tracing::warn!(
            code = error.kind().code(),
            detail = error.detail(),
            request_id = %id,
            "request refused"
        ),
        None => tracing::warn!(
            code = error.kind().code(),
            detail = error.detail(),
            "request refused before an identifier could be generated"
        ),
    }

    let mut builder = AxumResponse::builder()
        .status(error.status())
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8");

    if let Some(id) = request_id {
        builder = builder.header(request_id::REQUEST_ID_HEADER, id.encode());
    }

    builder
        .body(Body::from(error.public_message()))
        .unwrap_or_else(|_| {
            AxumResponse::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .expect("an empty 500 always builds")
        })
}
