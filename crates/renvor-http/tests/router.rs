//! Real-router tests.
//!
//! # There is no imitation of the routing stack anywhere in this file
//!
//! SC-001 requires routing behaviour to be asserted against a **real** router driven as a **real**
//! service. Every test below builds an `axum::Router` through `renvor_http::route::build::router`
//! — the same function a served application calls — and drives it with `tower::ServiceExt::oneshot`,
//! which is the router's own `Service` implementation.
//!
//! No mock service, no fake dispatcher, and no assertion against a description of what the router
//! would do. A test that asserted against a substitute would pass while the shipped router did
//! something else, which is the failure this rule exists to prevent.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request as HttpRequest, StatusCode, header};
use http_body_util::BodyExt;
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::{
    CorsPolicy, HostPolicy, Request, Response, RouteGroup, RouteRegistry, TrustedProxies,
};
use tower::ServiceExt;

const HOST: &str = "example.test";

fn config() -> RouterConfig {
    RouterConfig {
        hosts: HostPolicy::deny_all().allow(HOST).expect("a valid host"),
        trusted_proxies: TrustedProxies::none(),
        cors: CorsPolicy::deny_all(),
        limits: renvor_http::Limits::new(),
        run_id: RunIdentifier::generate(&OsEntropy).expect("entropy"),
        cancel: CancelScope::root(),
        gate: WorkGate::new(),
        state: std::sync::Arc::new(renvor_core::TypedStateMap::new()),
    }
}

async fn hello(_: Request) -> Response {
    Response::text("hello")
}

async fn created(_: Request) -> Response {
    Response::status(201).expect("a status HTTP defines")
}

/// Echoes what the security layers resolved, so a test can assert on it from the outside.
async fn whoami(request: Request) -> Response {
    Response::text(format!(
        "{}|{}|{}",
        request.context().client().address(),
        request.context().client().is_forwarded(),
        request.context().host()
    ))
}

async fn echo_body(request: Request) -> Response {
    Response::text(format!("{}", request.body().len()))
}

fn registry() -> RouteRegistry {
    let group = RouteGroup::new("api-v1", "/api/v1")
        .expect("prefix")
        .get("/health", hello)
        .expect("route")
        .post("/items", created)
        .expect("route");

    let mut registry = RouteRegistry::new();
    registry.group(group).expect("group");
    registry.get("/whoami", whoami).expect("route");
    registry.post("/echo", echo_body).expect("route");
    registry
}

/// Builds a request carrying connection information, as a served router receives.
fn request(method: &str, path: &str) -> axum::http::request::Builder {
    HttpRequest::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, HOST)
}

fn with_peer(builder: axum::http::request::Builder, peer: IpAddr) -> HttpRequest<Body> {
    let mut request = builder.body(Body::empty()).expect("a valid request");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(peer, 40000)));
    request
}

fn localhost() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

async fn body_string(response: axum::response::Response) -> String {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("the body collects")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

#[tokio::test]
async fn a_declared_route_is_dispatched_to_its_handler() {
    let app = router(&registry(), config()).expect("the configuration is valid");

    let response = app
        .oneshot(with_peer(request("GET", "/api/v1/health"), localhost()))
        .await
        .expect("the router responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "hello");
}

#[tokio::test]
async fn an_undeclared_path_is_404() {
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(with_peer(request("GET", "/absent"), localhost()))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_undeclared_method_on_a_declared_path_is_405_and_names_the_allowed_methods() {
    // FR-011 and SC-002. The `Allow` header is the whole point: a 405 without it tells a caller
    // its method was wrong but not which one would be right.
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(with_peer(request("DELETE", "/api/v1/health"), localhost()))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    let allow = response
        .headers()
        .get(header::ALLOW)
        .expect("a 405 must carry an Allow header")
        .to_str()
        .expect("ascii");

    assert!(allow.contains("GET"), "Allow did not name GET: {allow}");
    assert!(
        !allow.contains("DELETE"),
        "Allow named the method that was refused: {allow}"
    );
}

#[tokio::test]
async fn a_group_prefix_reaches_the_real_router() {
    let app = router(&registry(), config()).expect("valid");

    // The grouped path answers...
    let grouped = app
        .clone()
        .oneshot(with_peer(request("POST", "/api/v1/items"), localhost()))
        .await
        .expect("responds");
    assert_eq!(grouped.status(), StatusCode::CREATED);

    // ...and the unprefixed form does not, so the prefix is real rather than cosmetic.
    let unprefixed = app
        .oneshot(with_peer(request("POST", "/items"), localhost()))
        .await
        .expect("responds");
    assert_eq!(unprefixed.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn every_response_carries_a_generated_request_identifier() {
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(with_peer(request("GET", "/api/v1/health"), localhost()))
        .await
        .expect("responds");

    let id = response
        .headers()
        .get("x-request-id")
        .expect("a request identifier is published")
        .to_str()
        .expect("ascii");

    assert_eq!(id.len(), 16, "not the documented width: {id}");
    assert!(
        id.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "not lowercase hex: {id}"
    );
}

#[tokio::test]
async fn a_caller_supplied_request_identifier_is_never_adopted() {
    // SC-005 and FR-015. This is the property `tower-http`'s own layer does NOT have, which is why
    // Renvor supplies its own — ADR-0012 Finding 1.
    let app = router(&registry(), config()).expect("valid");

    let hostile = "attacker-chosen-identifier";
    let response = app
        .oneshot(with_peer(
            request("GET", "/api/v1/health").header("x-request-id", hostile),
            localhost(),
        ))
        .await
        .expect("responds");

    let published = response
        .headers()
        .get("x-request-id")
        .expect("an identifier is published")
        .to_str()
        .expect("ascii");

    assert_ne!(published, hostile, "the caller's identifier was adopted");
    assert_eq!(published.len(), 16);
}

#[tokio::test]
async fn hostile_forwarding_headers_cannot_forge_client_identity_by_default() {
    // SC-004. The headline security property of the phase, asserted through the real router rather
    // than against the resolver in isolation.
    let app = router(&registry(), config()).expect("valid");
    let peer = IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9));

    let response = app
        .oneshot(with_peer(
            request("GET", "/whoami")
                .header("x-forwarded-for", "1.2.3.4")
                .header("forwarded", "for=1.2.3.4;host=evil.example"),
            peer,
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::OK);
    let observed = body_string(response).await;

    assert!(
        observed.starts_with("203.0.113.9|false|"),
        "identity was forged: {observed}"
    );
    assert!(
        !observed.contains("1.2.3.4"),
        "identity was forged: {observed}"
    );
    assert!(
        !observed.contains("evil.example"),
        "the host was taken from a forwarding header: {observed}"
    );
}

#[tokio::test]
async fn a_trusted_proxys_forwarding_header_is_honoured_through_the_real_router() {
    // POSITIVE CONTROL for the test above. Without it, a router that ignored every forwarding
    // header unconditionally would pass, and the trusted path would be unproven.
    let proxy = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let mut config = config();
    config.trusted_proxies = TrustedProxies::none().trust(proxy);

    let app = router(&registry(), config).expect("valid");

    let response = app
        .oneshot(with_peer(
            request("GET", "/whoami").header("x-forwarded-for", "198.51.100.7"),
            proxy,
        ))
        .await
        .expect("responds");

    let observed = body_string(response).await;
    assert!(
        observed.starts_with("198.51.100.7|true|"),
        "the trusted proxy's header was ignored: {observed}"
    );
}

#[tokio::test]
async fn an_unconfigured_host_is_refused_and_the_reason_is_not_disclosed() {
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(with_peer(
            HttpRequest::builder()
                .method("GET")
                .uri("/api/v1/health")
                .header(header::HOST, "evil.example"),
            localhost(),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // FR-041: the refusal says what happened, not which hosts are configured.
    let body = body_string(response).await;
    assert!(!body.contains(HOST), "the configured host leaked: {body}");
    assert!(
        !body.contains("evil.example"),
        "input was reflected: {body}"
    );
}

#[tokio::test]
async fn a_missing_host_header_is_refused() {
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(with_peer(
            HttpRequest::builder().method("GET").uri("/api/v1/health"),
            localhost(),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_disallowed_origin_is_refused_and_an_allowed_one_is_not() {
    let mut permissive = config();
    permissive.cors = CorsPolicy::deny_all()
        .allow_origin("https://allowed.example")
        .expect("a valid origin");

    let app = router(&registry(), permissive).expect("valid");

    let refused = app
        .clone()
        .oneshot(with_peer(
            request("GET", "/api/v1/health").header(header::ORIGIN, "https://evil.example"),
            localhost(),
        ))
        .await
        .expect("responds");
    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);

    // POSITIVE CONTROL — AND A NAMED WEAKNESS.
    //
    // This asserts only that the configured origin is ADMITTED, which is also what you get with no
    // CORS implementation at all. It controls for over-refusal, not for the feature.
    //
    // The assertion that would control for the feature — `Access-Control-Allow-Origin` on the
    // response — cannot be written yet, because the CORS protocol is not implemented: the policy
    // denies, and never grants. Recorded here rather than left for a reader to assume otherwise.
    // See `governance/phase-004-evidence.md` §Findings against the implementation.
    let allowed = app
        .oneshot(with_peer(
            request("GET", "/api/v1/health").header(header::ORIGIN, "https://allowed.example"),
            localhost(),
        ))
        .await
        .expect("responds");
    assert_eq!(allowed.status(), StatusCode::OK);
}

#[tokio::test]
async fn cors_denies_by_default() {
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(with_peer(
            request("GET", "/api/v1/health").header(header::ORIGIN, "https://anything.example"),
            localhost(),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_body_at_the_limit_passes_and_one_byte_more_does_not() {
    // SC-008 at the exact boundary. The boundary is INCLUSIVE, verified against the underlying
    // implementation rather than assumed — see the plan's package research.
    let mut small = config();
    small.limits.max_body_bytes = 16;

    let app = router(&registry(), small).expect("valid");

    let at_limit = app
        .clone()
        .oneshot({
            let mut request = request("POST", "/echo")
                .body(Body::from(vec![b'x'; 16]))
                .expect("valid");
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::new(localhost(), 40000)));
            request
        })
        .await
        .expect("responds");

    assert_eq!(
        at_limit.status(),
        StatusCode::OK,
        "the boundary was refused"
    );
    assert_eq!(body_string(at_limit).await, "16");

    let over_limit = app
        .oneshot({
            let mut request = request("POST", "/echo")
                .body(Body::from(vec![b'x'; 17]))
                .expect("valid");
            request
                .extensions_mut()
                .insert(ConnectInfo(SocketAddr::new(localhost(), 40000)));
            request
        })
        .await
        .expect("responds");

    assert_eq!(over_limit.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn a_router_cannot_be_built_for_an_unsafe_cors_configuration() {
    // FR-022: refused at CONFIGURATION time. The underlying library detects the same condition
    // with an assertion reachable while serving; this never reaches that state.
    let mut unsafe_config = config();
    unsafe_config.cors = CorsPolicy::deny_all()
        .allow_any_origin()
        .allow_credentials();

    assert!(
        router(&registry(), unsafe_config).is_err(),
        "a wildcard-plus-credentials router was built"
    );

    // POSITIVE CONTROL: a safe configuration DOES build.
    assert!(router(&registry(), config()).is_ok());
}

#[tokio::test]
async fn a_request_without_connection_information_fails_closed() {
    // Without a peer there is no fact to resolve identity from. Inventing one would make every
    // downstream decision rest on a value nothing observed.
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(
            request("GET", "/whoami")
                .body(Body::empty())
                .expect("valid"),
        )
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn a_repeated_forwarding_header_with_one_unreadable_value_still_fails_closed() {
    // The defect this guards: an unreadable value used to be dropped, so a repeated header
    // collapsed to a single readable one and the repeated-header refusal never fired. The peer is
    // TRUSTED here, so without the fix the surviving value would have been attributed.
    let proxy = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let mut configuration = config();
    configuration.trusted_proxies = TrustedProxies::none().trust(proxy);

    let app = router(&registry(), configuration).expect("valid");

    let mut request = request("GET", "/whoami")
        .body(Body::empty())
        .expect("a valid request");
    request.headers_mut().append(
        "x-forwarded-for",
        axum::http::HeaderValue::from_bytes(&[0xff, 0xfe]).expect("bytes are a valid header value"),
    );
    request
        .headers_mut()
        .append("x-forwarded-for", "198.51.100.7".parse().expect("valid"));
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(proxy, 40000)));

    let observed = body_string(app.oneshot(request).await.expect("responds")).await;

    assert!(
        observed.starts_with("10.0.0.1|false|"),
        "a repeated header with one unreadable value was attributed: {observed}"
    );
}

#[tokio::test]
async fn a_same_origin_write_is_not_refused_by_the_default_cors_policy() {
    // The defect this guards: browsers send `Origin` on same-origin POST/PUT/PATCH/DELETE, so a
    // default-configured application answered 400 to a form post from its own page.
    let mut registry = registry();
    registry
        .route(renvor_http::Method::Post, "/submit", created)
        .expect("registers");

    let app = router(&registry, config()).expect("valid");

    let response = app
        .oneshot(with_peer(
            request("POST", "/submit").header(header::ORIGIN, format!("https://{HOST}")),
            localhost(),
        ))
        .await
        .expect("responds");

    assert_eq!(
        response.status(),
        StatusCode::CREATED,
        "a same-origin write was refused by the CORS policy"
    );
}

#[tokio::test]
async fn a_cross_origin_request_is_still_refused_by_the_default_policy() {
    // POSITIVE CONTROL for the test above. Without it, a carve-out that matched everything would
    // pass and CORS would be inert.
    let app = router(&registry(), config()).expect("valid");

    let response = app
        .oneshot(with_peer(
            request("GET", "/api/v1/health").header(header::ORIGIN, "https://evil.example"),
            localhost(),
        ))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_handler_cannot_add_a_second_request_identifier() {
    // `Builder::header` appends, so a handler setting this header used to produce TWO values with
    // its own first — while the contract says the GENERATED identifier is what appears.
    async fn forges(_: Request) -> Response {
        Response::text("ok")
            .with_header("x-request-id", "handler-chosen")
            .expect("a representable header")
    }

    let mut registry = RouteRegistry::new();
    registry.get("/forge", forges).expect("registers");

    let app = router(&registry, config()).expect("valid");
    let response = app
        .oneshot(with_peer(request("GET", "/forge"), localhost()))
        .await
        .expect("responds");

    let values: Vec<&str> = response
        .headers()
        .get_all("x-request-id")
        .iter()
        .map(|value| value.to_str().expect("ascii"))
        .collect();

    assert_eq!(values.len(), 1, "a second identifier survived: {values:?}");
    assert_ne!(values[0], "handler-chosen");
    assert_eq!(values[0].len(), 16);
}
