//! Every request control, asserted on the path a request actually takes.
//!
//! # Why this file exists separately from `router.rs`
//!
//! `router.rs` asserts what a **matched** route does. That left the largest hole in the phase: the
//! 404 and 405 paths never reached Renvor's code at all, so every control was absent on exactly the
//! requests an attacker sends most — the ones for endpoints that do not exist.
//!
//! These tests assert the controls on **unmatched** requests too, which is the only way that hole
//! is visible.

use core::time::Duration;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request as HttpRequest, StatusCode, header};
use http_body_util::BodyExt;
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::{
    CorsPolicy, HostPolicy, Limits, Request, Response, RouteRegistry, TrustedProxies,
};
use tower::ServiceExt;

const HOST: &str = "example.test";
const REQUEST_ID: &str = "x-request-id";

fn config() -> RouterConfig {
    RouterConfig {
        hosts: HostPolicy::deny_all().allow(HOST).expect("a valid host"),
        trusted_proxies: TrustedProxies::none(),
        cors: CorsPolicy::deny_all(),
        limits: Limits::new(),
        run_id: RunIdentifier::generate(&OsEntropy).expect("entropy"),
        cancel: CancelScope::root(),
        gate: WorkGate::new(),
        state: Arc::new(TypedStateMap::new()),
    }
}

async fn ok(_: Request) -> Response {
    Response::text("ok")
}

async fn created(_: Request) -> Response {
    Response::status(201)
}

/// Reports the path parameter it was given, so a test can assert it from outside.
async fn echo_param(request: Request) -> Response {
    Response::text(request.path_param("id").unwrap_or("<absent>").to_owned())
}

fn registry() -> RouteRegistry {
    let mut registry = RouteRegistry::new();
    registry.get("/declared", ok).expect("route");
    registry.post("/declared", created).expect("route");
    registry.get("/items/{id}", echo_param).expect("route");
    registry
}

fn build(config: RouterConfig) -> axum::Router {
    router(&registry(), config).expect("the configuration is valid")
}

fn peer() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 40000))
}

fn request(method: &str, path: &str) -> axum::http::request::Builder {
    HttpRequest::builder().method(method).uri(path)
}

fn served(builder: axum::http::request::Builder) -> HttpRequest<Body> {
    let mut request = builder.body(Body::empty()).expect("a valid request");
    request.extensions_mut().insert(peer());
    request
}

async fn body_string(response: axum::response::Response) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("collects")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

fn header_of(response: &axum::response::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

// ---------------------------------------------------------------------------------------------
// L-07 — no response leaves without passing every control
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn an_unknown_path_with_a_disallowed_host_returns_the_host_rejection_not_404() {
    // THE DEFECT. `GET /missing` with `Host: evil.example` returned 404 with no identifier: the
    // router's own fallback answered before host validation ever ran.
    //
    // Host validation is layer 2 and routing is inside layer 8, so the host rejection is the only
    // answer consistent with the declared order.
    let app = build(config());

    let response = app
        .oneshot(served(
            request("GET", "/missing").header(header::HOST, "evil.example"),
        ))
        .await
        .expect("responds");

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "an unmatched path bypassed host validation"
    );
    assert!(
        header_of(&response, REQUEST_ID).is_some(),
        "the refusal carried no request identifier"
    );
}

#[tokio::test]
async fn an_unknown_path_with_an_allowed_host_is_404_and_carries_the_request_identifier() {
    // POSITIVE CONTROL for the test above: with an ALLOWED host the same path IS a 404, so the
    // rejection above is about the host rather than about every unmatched path being refused.
    let app = build(config());

    let response = app
        .oneshot(served(
            request("GET", "/missing").header(header::HOST, HOST),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let id = header_of(&response, REQUEST_ID).expect("a 404 must be correlatable");
    assert_eq!(id.len(), 16, "the identifier is not the generated form: {id}");
}

#[tokio::test]
async fn a_405_carries_both_allow_and_the_request_identifier() {
    let app = build(config());

    let response = app
        .oneshot(served(
            request("DELETE", "/declared").header(header::HOST, HOST),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let allow = header_of(&response, header::ALLOW.as_str()).expect("405 must carry Allow");
    assert!(allow.contains("GET"), "{allow}");
    assert!(allow.contains("POST"), "{allow}");
    assert!(
        !allow.contains("DELETE"),
        "Allow named a method that does not answer: {allow}"
    );

    assert!(
        header_of(&response, REQUEST_ID).is_some(),
        "the 405 carried no request identifier"
    );
}

#[tokio::test]
async fn an_unmatched_path_is_refused_during_drain_like_any_other_request() {
    // FR-030 reaches the fallback too. A 404 served during drain is work admitted after the gate
    // closed, which the drain cannot see.
    let mut configured = config();
    let gate = configured.gate.clone();
    configured.gate = gate.clone();
    let app = build(configured);

    gate.close();

    let response = app
        .oneshot(served(
            request("GET", "/missing").header(header::HOST, HOST),
        ))
        .await
        .expect("responds");

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "an unmatched path was served during drain"
    );
}

// ---------------------------------------------------------------------------------------------
// L-08 — the timeout bounds the whole request, body included
// ---------------------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn a_body_that_never_arrives_times_out_rather_than_holding_admission_for_ever() {
    // THE DEFECT. The body was read BEFORE the timeout was applied, so a client that opened a
    // request and then stalled held a work permit and a concurrency slot with no bound at all.
    let mut configured = config();
    configured.limits.request_timeout = Duration::from_secs(5);
    let gate = configured.gate.clone();
    let app = build(configured);

    // A body that is announced and never delivered.
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<axum::body::Bytes, String>>(1);
    let stalled = Body::from_stream(tokio_stream_of(receiver));

    let mut stalled_request = request("POST", "/declared")
        .header(header::HOST, HOST)
        .body(stalled)
        .expect("valid");
    stalled_request.extensions_mut().insert(peer());

    let response = app.oneshot(stalled_request).await.expect("responds");

    assert_eq!(
        response.status(),
        StatusCode::REQUEST_TIMEOUT,
        "a stalled body was not bounded by the request timeout"
    );
    assert_eq!(
        gate.outstanding(),
        0,
        "the timed-out request did not release its work permit"
    );
    drop(sender);
}

#[tokio::test(start_paused = true)]
async fn a_body_that_arrives_inside_the_bound_still_succeeds() {
    // POSITIVE CONTROL: the timeout does not simply reject every request with a streamed body.
    let mut configured = config();
    configured.limits.request_timeout = Duration::from_secs(5);
    let app = build(configured);

    let mut delivered = request("POST", "/declared")
        .header(header::HOST, HOST)
        .body(Body::from("payload"))
        .expect("valid");
    delivered.extensions_mut().insert(peer());

    let response = app.oneshot(delivered).await.expect("responds");
    assert_eq!(response.status(), StatusCode::CREATED);
}

/// Adapts a channel receiver into the stream `Body::from_stream` wants.
fn tokio_stream_of(
    receiver: tokio::sync::mpsc::Receiver<Result<axum::body::Bytes, String>>,
) -> impl futures_util::Stream<Item = Result<axum::body::Bytes, String>> {
    futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
    })
}

// ---------------------------------------------------------------------------------------------
// L-06 — CORS actually speaks the protocol
// ---------------------------------------------------------------------------------------------

fn cors_config(policy: CorsPolicy) -> RouterConfig {
    let mut configured = config();
    configured.cors = policy;
    configured
}

#[tokio::test]
async fn an_allowed_origin_receives_the_allow_origin_header() {
    // THE DEFECT. An allowed origin got 200 and NO `access-control-allow-origin`, so a browser
    // blocked every response the policy permitted. The policy was decorative.
    let policy = CorsPolicy::deny_all()
        .allow_origin("https://app.example")
        .expect("valid origin");
    let app = build(cors_config(policy));

    let response = app
        .oneshot(served(
            request("GET", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, "https://app.example"),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_of(&response, "access-control-allow-origin").as_deref(),
        Some("https://app.example"),
        "the allowed origin received no allow-origin header"
    );
    assert!(
        header_of(&response, "vary")
            .is_some_and(|vary| vary.to_ascii_lowercase().contains("origin")),
        "a cacheable response did not vary on Origin"
    );
}

#[tokio::test]
async fn a_disallowed_origin_is_refused_and_carries_no_allow_origin_header() {
    // POSITIVE CONTROL's counterpart: deny-by-default still denies, and denying must never leak
    // an allow-origin header.
    let policy = CorsPolicy::deny_all()
        .allow_origin("https://app.example")
        .expect("valid origin");
    let app = build(cors_config(policy));

    let response = app
        .oneshot(served(
            request("GET", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, "https://evil.example"),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        header_of(&response, "access-control-allow-origin"),
        None,
        "a refused origin was told it was allowed"
    );
}

#[tokio::test]
async fn a_preflight_is_answered_rather_than_routed() {
    // C-11 layer 4: a preflight is answered BEFORE admission spends a permit on it. `OPTIONS` is
    // not registered on this path, so an unanswered preflight would 405.
    let policy = CorsPolicy::deny_all()
        .allow_origin("https://app.example")
        .expect("valid origin");
    let app = build(cors_config(policy));

    let response = app
        .oneshot(served(
            request("OPTIONS", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, "https://app.example")
                .header("access-control-request-method", "POST"),
        ))
        .await
        .expect("responds");

    // 200, not 204. The preflight response is the SELECTED LIBRARY'S, per ADR-0012: Renvor owns
    // the CORS *validation* and writes none of the protocol. The library answers with an empty
    // 200, and asserting Renvor's preference over what actually ships would be asserting a wish.
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_of(&response, "access-control-allow-origin").as_deref(),
        Some("https://app.example")
    );
    let methods =
        header_of(&response, "access-control-allow-methods").expect("preflight names the methods");
    assert!(methods.contains("POST"), "{methods}");
}

#[tokio::test]
async fn a_preflight_from_a_disallowed_origin_is_refused() {
    // POSITIVE CONTROL for the preflight test: preflight is not a way around the origin check.
    let policy = CorsPolicy::deny_all()
        .allow_origin("https://app.example")
        .expect("valid origin");
    let app = build(cors_config(policy));

    let response = app
        .oneshot(served(
            request("OPTIONS", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, "https://evil.example")
                .header("access-control-request-method", "POST"),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(header_of(&response, "access-control-allow-origin"), None);
}

#[tokio::test]
async fn credentials_are_advertised_only_when_the_policy_permits_them() {
    let with = CorsPolicy::deny_all()
        .allow_origin("https://app.example")
        .expect("valid")
        .allow_credentials();
    let app = build(cors_config(with));

    let response = app
        .oneshot(served(
            request("GET", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, "https://app.example"),
        ))
        .await
        .expect("responds");
    assert_eq!(
        header_of(&response, "access-control-allow-credentials").as_deref(),
        Some("true")
    );

    // POSITIVE CONTROL: without the setting the header is absent, so the value above comes from
    // the policy rather than from it always being emitted.
    let without = CorsPolicy::deny_all()
        .allow_origin("https://app.example")
        .expect("valid");
    let app = build(cors_config(without));
    let response = app
        .oneshot(served(
            request("GET", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, "https://app.example"),
        ))
        .await
        .expect("responds");
    assert_eq!(header_of(&response, "access-control-allow-credentials"), None);
}

#[tokio::test]
async fn a_wildcard_policy_never_reflects_credentials() {
    // The combination is refused at configuration time, so the only reachable wildcard policy is
    // one WITHOUT credentials. Asserted so that a future edit which "helpfully" reflected the
    // origin and set credentials together fails here.
    let app = build(cors_config(CorsPolicy::deny_all().allow_any_origin()));

    let response = app
        .oneshot(served(
            request("GET", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, "https://anything.example"),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_of(&response, "access-control-allow-origin").as_deref(),
        Some("*"),
        "a wildcard policy must answer with the wildcard, not with the caller's origin"
    );
    assert_eq!(header_of(&response, "access-control-allow-credentials"), None);
}

#[tokio::test]
async fn a_same_origin_write_succeeds_under_the_default_deny_all_policy() {
    // Browsers send `Origin` on same-origin POST. A deny-by-default policy that refused them would
    // break every application's own frontend on the day it was generated.
    let app = build(config());

    let response = app
        .oneshot(served(
            request("POST", "/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, format!("https://{HOST}")),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::CREATED);
}

// ---------------------------------------------------------------------------------------------
// L-09, L-10, L-11 — request-lifetime correctness
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_path_parameter_reaches_the_handler() {
    // `path_param` returned `None` for every parameter of every route: the router passed an empty
    // map unconditionally.
    let app = build(config());

    let response = app
        .oneshot(served(
            request("GET", "/items/42").header(header::HOST, HOST),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "42");
}

#[tokio::test]
async fn a_panicking_handler_is_contained_and_reported_without_its_payload() {
    // C-10: a handler panic is caught, contained, and reported as a failure. Its payload never
    // reaches a response.
    const SECRET: &str = "panic-payload-must-not-escape";

    let mut registry = RouteRegistry::new();
    registry
        .get("/panics", |_: Request| async move {
            panic!("{SECRET}");
        })
        .expect("route");

    let configured = config();
    let gate = configured.gate.clone();
    let app = router(&registry, configured).expect("valid");

    // The default hook prints the panic; silencing it keeps the suite's output about the tests.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let response = app
        .oneshot(served(
            request("GET", "/panics").header(header::HOST, HOST),
        ))
        .await
        .expect("the router responds rather than aborting");
    std::panic::set_hook(previous);

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        header_of(&response, REQUEST_ID).is_some(),
        "the contained panic was not correlatable"
    );

    let body = body_string(response).await;
    assert!(
        !body.contains(SECRET),
        "the panic payload reached the response body"
    );
    assert_eq!(
        gate.outstanding(),
        0,
        "a panicking handler leaked its work permit"
    );
}

#[tokio::test]
async fn a_handler_that_does_not_panic_is_unaffected() {
    // POSITIVE CONTROL for the containment test: the boundary does not turn every response into a
    // 500.
    let app = build(config());
    let response = app
        .oneshot(served(
            request("GET", "/declared").header(header::HOST, HOST),
        ))
        .await
        .expect("responds");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "ok");
}

#[tokio::test]
async fn dropping_a_request_in_flight_cancels_its_scope() {
    // C-10: client disconnect and request timeout both cancel the request scope. Only the timeout
    // did; a disconnected client left the handler running against an uncancelled scope.
    let observed = Arc::new(AtomicBool::new(false));
    let handler_flag = Arc::clone(&observed);

    let mut registry = RouteRegistry::new();
    registry
        .get("/watches", move |request: Request| {
            let handler_flag = Arc::clone(&handler_flag);
            async move {
                // An application service watching the KERNEL's scope, with no transport type in
                // sight — which is the property the cancellation has to preserve.
                let scope = request.context().cancel_scope().clone();
                tokio::spawn(async move {
                    scope.cancelled().await;
                    handler_flag.store(true, Ordering::SeqCst);
                });
                std::future::pending::<()>().await;
                Response::text("unreachable")
            }
        })
        .expect("route");

    let configured = config();
    let gate = configured.gate.clone();
    let app = router(&registry, configured).expect("valid");

    let mut dispatching = Box::pin(app.oneshot(served(
        request("GET", "/watches").header(header::HOST, HOST),
    )));

    // Poll it far enough to reach the handler, then drop it — a client disconnect.
    let polled = tokio::time::timeout(Duration::from_millis(200), &mut dispatching).await;
    assert!(polled.is_err(), "the pending handler returned");
    drop(dispatching);

    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(
        observed.load(Ordering::SeqCst),
        "the abandoned request's scope was never cancelled"
    );
    assert_eq!(
        gate.outstanding(),
        0,
        "the abandoned request did not release its work permit"
    );
}
