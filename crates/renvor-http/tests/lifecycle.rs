//! Admission, cancellation, drain, and middleware order — asserted through the **real** router.
//!
//! # Order is proven by behaviour, never by reading a list of layers
//!
//! FR-028. Each ordering test sends a request that violates **two** rules at once and asserts which
//! refusal comes back. A textual list of layers describes what someone wrote; this describes what
//! runs.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request as HttpRequest, StatusCode, header};
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::{CorsPolicy, HostPolicy, Request, Response, RouteRegistry, TrustedProxies};
use tower::ServiceExt;

const HOST: &str = "example.test";

fn localhost() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn config(gate: WorkGate, cancel: CancelScope) -> RouterConfig {
    RouterConfig {
        hosts: HostPolicy::deny_all().allow(HOST),
        trusted_proxies: TrustedProxies::none(),
        cors: CorsPolicy::deny_all(),
        limits: renvor_http::Limits::new(),
        run_id: RunIdentifier::generate(&OsEntropy).expect("entropy"),
        cancel,
        gate,
    }
}

fn get(path: &str) -> HttpRequest<Body> {
    let mut request = HttpRequest::builder()
        .method("GET")
        .uri(path)
        .header(header::HOST, HOST)
        .body(Body::empty())
        .expect("valid");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(localhost(), 40000)));
    request
}

async fn quick(_: Request) -> Response {
    Response::text("ok")
}

// ── admission and drain ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn once_drain_begins_a_new_request_does_not_reach_a_handler() {
    // FR-030 and SC-010. The handler records whether it ran, so this asserts the request was
    // REFUSED rather than merely answered with a 503 after doing the work.
    let reached = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&reached);

    let mut registry = RouteRegistry::new();
    registry
        .get("/health", move |_: Request| {
            let flag = Arc::clone(&flag);
            async move {
                flag.store(true, Ordering::SeqCst);
                Response::text("ok")
            }
        })
        .expect("registers");

    let gate = WorkGate::new();
    let app = router(&registry, config(gate.clone(), CancelScope::root())).expect("valid");

    // Drain begins.
    assert!(gate.close(), "the first close closes the gate");

    let response = app.oneshot(get("/health")).await.expect("responds");

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        !reached.load(Ordering::SeqCst),
        "a request reached its handler after drain began"
    );
}

#[tokio::test]
async fn before_drain_the_same_request_is_served() {
    // POSITIVE CONTROL for the test above. Without it, a router that refused everything would
    // pass and the refusal would prove nothing about the gate.
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("registers");

    let app = router(&registry, config(WorkGate::new(), CancelScope::root())).expect("valid");

    let response = app.oneshot(get("/health")).await.expect("responds");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_served_request_holds_a_permit_and_releases_it() {
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("registers");

    let gate = WorkGate::new();
    let app = router(&registry, config(gate.clone(), CancelScope::root())).expect("valid");

    assert_eq!(gate.outstanding(), 0);
    let response = app.oneshot(get("/health")).await.expect("responds");
    assert_eq!(response.status(), StatusCode::OK);

    // Released on the way out, so a completed request is not counted against a later drain.
    assert_eq!(gate.outstanding(), 0, "the permit leaked");
}

#[tokio::test]
async fn a_refused_request_still_releases_everything_it_took() {
    // The leak this would otherwise hide: a request refused after taking a concurrency slot.
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("registers");

    let gate = WorkGate::new();
    let app = router(&registry, config(gate.clone(), CancelScope::root())).expect("valid");

    // Refused on the host rule, which is layer 2 — before admission is even reached.
    let mut hostile = get("/health");
    hostile
        .headers_mut()
        .insert(header::HOST, "evil.example".parse().expect("valid"));

    let response = app.oneshot(hostile).await.expect("responds");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(gate.outstanding(), 0);
}

// ── cancellation ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn application_shutdown_cancels_an_in_flight_request_without_any_transport_type() {
    // FR-031 and SC-009. The handler's only view of cancellation is the kernel's own scope; the
    // closure below names no `axum` type at all.
    let observed = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&observed);

    let application = CancelScope::root();
    let for_handler = application.clone();

    let mut registry = RouteRegistry::new();
    registry
        .get("/slow", move |request: Request| {
            let flag = Arc::clone(&flag);
            let application = for_handler.clone();
            async move {
                // Cancel the APPLICATION scope from outside the request; the request's own scope is
                // a child, so it must observe it.
                application.cancel();
                request.context().cancelled().await;
                flag.store(true, Ordering::SeqCst);
                Response::text("cancelled")
            }
        })
        .expect("registers");

    let app = router(&registry, config(WorkGate::new(), application)).expect("valid");
    let response = app.oneshot(get("/slow")).await.expect("responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        observed.load(Ordering::SeqCst),
        "the application service never observed cancellation"
    );
}

#[tokio::test]
async fn a_request_that_completes_normally_is_not_cancelled() {
    // POSITIVE CONTROL for the test above: without it, a context that reported "cancelled" always
    // would pass, and the cancellation signal would mean nothing.
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancelled);

    let mut registry = RouteRegistry::new();
    registry
        .get("/quick", move |request: Request| {
            let flag = Arc::clone(&flag);
            async move {
                flag.store(request.context().is_cancelled(), Ordering::SeqCst);
                Response::text("ok")
            }
        })
        .expect("registers");

    let app = router(&registry, config(WorkGate::new(), CancelScope::root())).expect("valid");

    let response = app.oneshot(get("/quick")).await.expect("responds");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !cancelled.load(Ordering::SeqCst),
        "a normally-completing request reported itself cancelled"
    );
}

#[tokio::test(start_paused = true)]
async fn a_timed_out_request_is_408_and_cancels_its_scope() {
    // The timeout and the cancellation are one mechanism from the application's point of view.
    let mut registry = RouteRegistry::new();
    registry
        .get("/hang", |_: Request| async move {
            // Longer than the configured timeout. Under a paused clock this costs no real time.
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Response::text("never")
        })
        .expect("registers");

    let mut configuration = config(WorkGate::new(), CancelScope::root());
    configuration.limits.request_timeout = Duration::from_secs(1);

    let gate = configuration.gate.clone();
    let app = router(&registry, configuration).expect("valid");

    let response = app.oneshot(get("/hang")).await.expect("responds");
    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);

    // The permit is released even on the timeout path, so a timed-out request cannot make a later
    // drain report work that is no longer running.
    assert_eq!(gate.outstanding(), 0, "the timeout path leaked a permit");
}

// ── middleware order, proven by behaviour ────────────────────────────────────────────────────

#[tokio::test]
async fn host_validation_runs_before_admission() {
    // Layer 2 outside layer 6. The gate is CLOSED and the host is wrong: if admission ran first the
    // answer would be 503, and if host validation ran first it is 400.
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("registers");

    let gate = WorkGate::new();
    gate.close();
    let app = router(&registry, config(gate, CancelScope::root())).expect("valid");

    let mut request = get("/health");
    request
        .headers_mut()
        .insert(header::HOST, "evil.example".parse().expect("valid"));

    let response = app.oneshot(request).await.expect("responds");
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "admission answered before host validation"
    );
}

#[tokio::test]
async fn host_validation_runs_before_the_body_limit() {
    // Layer 2 outside layer 8. The body is over the limit AND the host is wrong.
    let mut registry = RouteRegistry::new();
    registry
        .route(renvor_http::Method::Post, "/echo", quick)
        .expect("registers");

    let mut configuration = config(WorkGate::new(), CancelScope::root());
    configuration.limits.max_body_bytes = 4;
    let app = router(&registry, configuration).expect("valid");

    let mut request = HttpRequest::builder()
        .method("POST")
        .uri("/echo")
        .header(header::HOST, "evil.example")
        .body(Body::from(vec![b'x'; 4096]))
        .expect("valid");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(localhost(), 40000)));

    let response = app.oneshot(request).await.expect("responds");
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "the body limit answered before host validation"
    );
}

#[tokio::test]
async fn cors_runs_before_admission() {
    // Layer 4 outside layer 6. The gate is closed AND the origin is disallowed.
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("registers");

    let gate = WorkGate::new();
    gate.close();
    let app = router(&registry, config(gate, CancelScope::root())).expect("valid");

    let mut request = get("/health");
    request.headers_mut().insert(
        header::ORIGIN,
        "https://evil.example".parse().expect("valid"),
    );

    let response = app.oneshot(request).await.expect("responds");
    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "admission answered before the CORS check"
    );
}

#[tokio::test]
async fn admission_runs_before_the_body_limit() {
    // Layer 6 outside layer 8. The gate is closed AND the body is over the limit.
    let mut registry = RouteRegistry::new();
    registry
        .route(renvor_http::Method::Post, "/echo", quick)
        .expect("registers");

    let gate = WorkGate::new();
    gate.close();
    let mut configuration = config(gate, CancelScope::root());
    configuration.limits.max_body_bytes = 4;
    let app = router(&registry, configuration).expect("valid");

    let mut request = HttpRequest::builder()
        .method("POST")
        .uri("/echo")
        .header(header::HOST, HOST)
        .body(Body::from(vec![b'x'; 4096]))
        .expect("valid");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(localhost(), 40000)));

    let response = app.oneshot(request).await.expect("responds");
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the body limit answered before admission"
    );
}

#[tokio::test]
async fn every_refusal_carries_a_request_identifier_because_correlation_is_outermost() {
    // Layer 1 outside everything. A rejection that cannot be correlated is an incident with no
    // trail, which is why the identifier layer is first — and why every refusal below it must
    // carry the identifier, not just the successful path.
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("registers");

    let gate = WorkGate::new();
    gate.close();
    let app = router(&registry, config(gate, CancelScope::root())).expect("valid");

    // A host refusal (layer 2), a CORS refusal (layer 4), and an admission refusal (layer 6) must
    // each be correlatable.
    let mut wrong_host = get("/health");
    wrong_host
        .headers_mut()
        .insert(header::HOST, "evil.example".parse().expect("valid"));

    let mut wrong_origin = get("/health");
    wrong_origin.headers_mut().insert(
        header::ORIGIN,
        "https://evil.example".parse().expect("valid"),
    );

    for (label, request) in [
        ("host", wrong_host),
        ("cors", wrong_origin),
        ("admission", get("/health")),
    ] {
        let response = app.clone().oneshot(request).await.expect("responds");
        assert_ne!(response.status(), StatusCode::OK, "{label} was not refused");

        let id = response
            .headers()
            .get("x-request-id")
            .unwrap_or_else(|| panic!("the {label} refusal carried no request identifier"))
            .to_str()
            .expect("ascii");
        assert_eq!(id.len(), 16, "{label}: {id}");
    }
}

#[tokio::test]
async fn a_served_response_also_carries_the_identifier() {
    // POSITIVE CONTROL for the test above: the header is present on the success path too, so its
    // presence on refusals is not an artefact of the refusal renderer alone.
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("registers");

    let app = router(&registry, config(WorkGate::new(), CancelScope::root())).expect("valid");

    let response = app.oneshot(get("/health")).await.expect("responds");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-request-id").is_some());
}
