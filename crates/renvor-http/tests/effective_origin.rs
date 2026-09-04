//! The effective origin, resolved by the real router and compared in full (Phase 010 correction
//! round, finding 1; RFC 6454 §4–§5; FR-085).
//!
//! # What was wrong
//!
//! The same-origin carve-out compared an `Origin` against the validated **host** alone, by
//! design ("scheme and port are deliberately ignored"). `https://example.test` was therefore
//! same-origin for a request that reached an `http` listener, and `http://example.test:8443` was
//! same-origin for port 80. RFC 6454 §5 says two origins are the same only when scheme, host, and
//! port all match, and a page on the plaintext origin — or on another port — is a different
//! security principal.
//!
//! # What these tests observe
//!
//! Two routes: one that reports the origin the router resolved (`context().origin()`), so the
//! scheme and port resolution is asserted directly; and a `POST` that answers `201`, so the
//! carve-out's verdict is a status rather than an inference. Every resolution path — the
//! configured scheme, a trusted proxy's `proto` in both header forms, an untrusted peer's, the
//! `Host` port, an IPv6 literal — is driven through the real layers.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request as HttpRequest, StatusCode};
use http_body_util::BodyExt as _;
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::{
    CorsPolicy, HostPolicy, Limits, Request, Response, RouteRegistry, Scheme, TrustedProxies,
};
use tower::ServiceExt as _;

const HOST: &str = "example.test";
const IPV6_HOST: &str = "[::1]";

/// A peer in the trusted set.
fn proxy() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
}

/// A peer outside it.
fn stranger() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9))
}

fn config(scheme: Scheme, trusted: TrustedProxies, cors: CorsPolicy) -> RouterConfig {
    RouterConfig {
        hosts: HostPolicy::deny_all()
            .allow(HOST)
            .expect("a valid host")
            .allow(IPV6_HOST)
            .expect("a valid host"),
        trusted_proxies: trusted,
        cors,
        limits: Limits::new(),
        run_id: RunIdentifier::generate(&OsEntropy).expect("entropy"),
        cancel: CancelScope::root(),
        gate: WorkGate::new(),
        state: Arc::new(TypedStateMap::new()),
        public_scheme: scheme,
    }
}

fn http_direct() -> RouterConfig {
    config(Scheme::Http, TrustedProxies::none(), CorsPolicy::deny_all())
}

fn http_behind_proxy() -> RouterConfig {
    config(
        Scheme::Http,
        TrustedProxies::none().trust(proxy()),
        CorsPolicy::deny_all(),
    )
}

/// Reports the origin the router resolved, as `scheme://host:port`.
async fn echo_origin(request: Request) -> Response {
    let origin = request.context().origin();
    Response::text(format!(
        "{}://{}:{}",
        origin.scheme().as_str(),
        origin.host(),
        origin.port()
    ))
}

async fn created(_: Request) -> Response {
    Response::status(201).expect("a status HTTP defines")
}

fn build(config: RouterConfig) -> axum::Router {
    let mut registry = RouteRegistry::new();
    registry.get("/origin", echo_origin).expect("route");
    registry.post("/write", created).expect("route");
    router(&registry, config).expect("the configuration is valid")
}

fn served(method: &str, path: &str, peer: IpAddr, headers: &[(&str, &str)]) -> HttpRequest<Body> {
    let mut builder = HttpRequest::builder().method(method).uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut request = builder.body(Body::empty()).expect("a valid request");
    request
        .extensions_mut()
        .insert(ConnectInfo(SocketAddr::new(peer, 40000)));
    request
}

/// The origin the router resolved for a `GET /origin` from `peer` carrying `headers`.
async fn origin_seen(app: axum::Router, peer: IpAddr, headers: &[(&str, &str)]) -> String {
    let response = app
        .oneshot(served("GET", "/origin", peer, headers))
        .await
        .expect("answers");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

/// The status of a `POST /write` from `peer` carrying `headers`: `201` if the carve-out (or the
/// CORS policy) admitted the `Origin`, `400` if the origin refusal fired.
async fn write_status(app: axum::Router, peer: IpAddr, headers: &[(&str, &str)]) -> StatusCode {
    app.oneshot(served("POST", "/write", peer, headers))
        .await
        .expect("answers")
        .status()
}

// ---------------------------------------------------------------------------------------------
// The request's own origin
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_direct_request_resolves_to_the_configured_scheme_the_validated_host_and_its_port() {
    let app = build(http_direct());

    assert_eq!(
        origin_seen(app.clone(), stranger(), &[("host", HOST)]).await,
        "http://example.test:80",
        "an absent Host port is not the scheme's default"
    );
    assert_eq!(
        origin_seen(app.clone(), stranger(), &[("host", "example.test:8080")]).await,
        "http://example.test:8080",
        "the Host port was not carried"
    );
    // The host is the VALIDATED one: case-folded, trailing dot removed.
    assert_eq!(
        origin_seen(app, stranger(), &[("host", "EXAMPLE.test.:8080")]).await,
        "http://example.test:8080"
    );

    // A server configured as reached over `https` resolves to `https` and port 443.
    let app = build(config(
        Scheme::Https,
        TrustedProxies::none(),
        CorsPolicy::deny_all(),
    ));
    assert_eq!(
        origin_seen(app, stranger(), &[("host", HOST)]).await,
        "https://example.test:443"
    );
}

#[tokio::test]
async fn an_ipv6_host_resolves_with_its_brackets_and_its_port() {
    let app = build(http_direct());
    assert_eq!(
        origin_seen(app.clone(), stranger(), &[("host", "[::1]:8080")]).await,
        "http://[::1]:8080"
    );
    assert_eq!(
        origin_seen(app, stranger(), &[("host", "[::1]")]).await,
        "http://[::1]:80"
    );
}

#[tokio::test]
async fn a_host_whose_port_is_not_a_port_is_refused() {
    // THE STRENGTHENING. `example.test:notaport` and `example.test:0` used to validate as
    // `example.test` with the junk thrown away. Now that the port is one third of the origin, a
    // port that cannot be one refuses the whole value.
    let app = build(http_direct());
    for junk in ["example.test:notaport", "example.test:0", "example.test:"] {
        let response = app
            .clone()
            .oneshot(served("GET", "/origin", stranger(), &[("host", junk)]))
            .await
            .expect("answers");
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "a Host with a port that is not a port was accepted"
        );
    }

    // POSITIVE CONTROL: a valid port is accepted, so the refusals are about the ports.
    assert_eq!(
        origin_seen(app, stranger(), &[("host", "example.test:8080")]).await,
        "http://example.test:8080"
    );
}

// ---------------------------------------------------------------------------------------------
// The scheme a trusted proxy reports — and an untrusted peer cannot
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_trusted_proxys_proto_sets_the_scheme() {
    let app = build(http_behind_proxy());

    // RFC 7239 `proto`, in the same `Forwarded` field that attributes the client.
    assert_eq!(
        origin_seen(
            app.clone(),
            proxy(),
            &[
                ("host", HOST),
                ("forwarded", "for=198.51.100.7;proto=https")
            ]
        )
        .await,
        "https://example.test:443",
        "a trusted proxy's proto=https was not honoured"
    );
    // Quoted, and in any case (RFC 7239 §4: parameter names are case-insensitive; §5.4 gives the
    // value as a URI scheme, which RFC 3986 §3.1 makes case-insensitive).
    assert_eq!(
        origin_seen(
            app.clone(),
            proxy(),
            &[
                ("host", HOST),
                ("forwarded", "for=198.51.100.7;Proto=\"HTTPS\"")
            ]
        )
        .await,
        "https://example.test:443"
    );
    // The last element's proto, matching the last element's `for` — the hop the trusted peer
    // vouched for.
    assert_eq!(
        origin_seen(
            app.clone(),
            proxy(),
            &[
                ("host", HOST),
                (
                    "forwarded",
                    "for=1.2.3.4;proto=https, for=198.51.100.7;proto=http"
                )
            ]
        )
        .await,
        "http://example.test:80"
    );
    // The legacy pair, when the standard header is absent.
    assert_eq!(
        origin_seen(
            app.clone(),
            proxy(),
            &[
                ("host", HOST),
                ("x-forwarded-for", "198.51.100.7"),
                ("x-forwarded-proto", "https"),
            ]
        )
        .await,
        "https://example.test:443",
        "a trusted proxy's X-Forwarded-Proto was not honoured"
    );
    // The Host port still wins over the scheme's default.
    assert_eq!(
        origin_seen(
            app.clone(),
            proxy(),
            &[
                ("host", "example.test:8443"),
                ("forwarded", "for=198.51.100.7;proto=https")
            ]
        )
        .await,
        "https://example.test:8443"
    );

    // And the carve-out follows: an `https` Origin is same-origin behind the TLS terminator.
    assert_eq!(
        write_status(
            app,
            proxy(),
            &[
                ("host", HOST),
                ("forwarded", "for=198.51.100.7;proto=https"),
                ("origin", "https://example.test"),
            ]
        )
        .await,
        StatusCode::CREATED,
        "an https Origin was refused behind a trusted TLS terminator"
    );
}

#[tokio::test]
async fn an_untrusted_peers_proto_is_never_read() {
    // SC-004's rule, applied to one more header: the socket decides whether a header is read at
    // all, and this peer is not in the trusted set.
    let app = build(http_behind_proxy());

    for headers in [
        vec![
            ("host", HOST),
            ("forwarded", "for=198.51.100.7;proto=https"),
        ],
        vec![
            ("host", HOST),
            ("x-forwarded-for", "198.51.100.7"),
            ("x-forwarded-proto", "https"),
        ],
    ] {
        assert_eq!(
            origin_seen(app.clone(), stranger(), &headers).await,
            "http://example.test:80",
            "an untrusted peer flipped the scheme"
        );
    }

    assert_eq!(
        write_status(
            app,
            stranger(),
            &[
                ("host", HOST),
                ("x-forwarded-proto", "https"),
                ("origin", "https://example.test"),
            ]
        )
        .await,
        StatusCode::BAD_REQUEST,
        "an untrusted peer's proto made an https Origin same-origin"
    );
}

#[tokio::test]
async fn a_trusted_proxy_with_no_usable_proto_leaves_the_configured_scheme() {
    // The fail-closed direction: the listener's own truth, never a guess. An `https` Origin then
    // fails to match and is refused — loud — rather than admitted on a scheme nothing vouched for.
    let app = build(http_behind_proxy());

    for (label, headers) in [
        // No proto at all.
        (
            "absent",
            vec![("host", HOST), ("forwarded", "for=198.51.100.7")],
        ),
        // A scheme no browser sends this server a request under.
        (
            "unparseable",
            vec![("host", HOST), ("forwarded", "for=198.51.100.7;proto=ftp")],
        ),
        (
            "empty",
            vec![("host", HOST), ("forwarded", "for=198.51.100.7;proto=")],
        ),
        // The standard header is PRESENT and has no proto: the legacy header gets no say.
        (
            "forwarded-present",
            vec![
                ("host", HOST),
                ("forwarded", "for=198.51.100.7"),
                ("x-forwarded-proto", "https"),
            ],
        ),
        // A repeated legacy header is refused rather than chosen between.
        (
            "repeated",
            vec![
                ("host", HOST),
                ("x-forwarded-for", "198.51.100.7"),
                ("x-forwarded-proto", "https"),
                ("x-forwarded-proto", "https"),
            ],
        ),
        // The identity did not resolve through the proxy (its `for` is unparseable), so nothing
        // it says about the scheme is read either — the identity module's rule.
        (
            "identity-unresolved",
            vec![("host", HOST), ("forwarded", "for=unknown;proto=https")],
        ),
        // A list in the legacy header: the RIGHTMOST entry is the nearest hop's, exactly as for
        // `X-Forwarded-For`, and here it says `http`.
        (
            "list-rightmost-http",
            vec![
                ("host", HOST),
                ("x-forwarded-for", "198.51.100.7"),
                ("x-forwarded-proto", "https, http"),
            ],
        ),
    ] {
        assert_eq!(
            origin_seen(app.clone(), proxy(), &headers).await,
            "http://example.test:80",
            "case `{label}` did not fall back to the configured scheme"
        );
    }

    // POSITIVE CONTROL for the list case: the rightmost entry IS read when it says `https`.
    assert_eq!(
        origin_seen(
            app,
            proxy(),
            &[
                ("host", HOST),
                ("x-forwarded-for", "198.51.100.7"),
                ("x-forwarded-proto", "http, https"),
            ]
        )
        .await,
        "https://example.test:443"
    );
}

// ---------------------------------------------------------------------------------------------
// The same-origin carve-out compares all three fields
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_same_host_origin_on_another_scheme_is_refused_at_the_carve_out() {
    // THE DEFECT. The carve-out said "scheme and port are deliberately ignored", so this was 201.
    let app = build(http_direct());

    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[("host", HOST), ("origin", "https://example.test")]
        )
        .await,
        StatusCode::BAD_REQUEST,
        "an https Origin was same-origin for an http request"
    );

    // POSITIVE CONTROL: the same host on the SAME scheme is same-origin and admitted, so the
    // refusal above is about the scheme rather than about the carve-out being gone.
    assert_eq!(
        write_status(
            app,
            stranger(),
            &[("host", HOST), ("origin", "http://example.test")]
        )
        .await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn a_same_host_origin_on_another_port_is_refused_at_the_carve_out() {
    let app = build(http_direct());

    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[("host", HOST), ("origin", "http://example.test:8443")]
        )
        .await,
        StatusCode::BAD_REQUEST,
        "an Origin on another port was same-origin"
    );

    // RFC 6454 §4 step 5: the written default port and the absent one are the same origin, on
    // either side of the comparison.
    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[("host", HOST), ("origin", "http://example.test:80")]
        )
        .await,
        StatusCode::CREATED
    );
    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[
                ("host", "example.test:80"),
                ("origin", "http://example.test")
            ]
        )
        .await,
        StatusCode::CREATED
    );
    // And a non-default port written on both sides.
    assert_eq!(
        write_status(
            app,
            stranger(),
            &[
                ("host", "example.test:8443"),
                ("origin", "http://example.test:8443")
            ]
        )
        .await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn a_configured_https_scheme_makes_an_https_origin_same_origin() {
    let app = build(config(
        Scheme::Https,
        TrustedProxies::none(),
        CorsPolicy::deny_all(),
    ));

    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[("host", HOST), ("origin", "https://example.test")]
        )
        .await,
        StatusCode::CREATED
    );
    assert_eq!(
        write_status(
            app,
            stranger(),
            &[("host", HOST), ("origin", "http://example.test")]
        )
        .await,
        StatusCode::BAD_REQUEST,
        "an http Origin was same-origin for an https server"
    );
}

#[tokio::test]
async fn an_ipv6_origin_is_compared_with_its_brackets_and_its_port() {
    let app = build(http_direct());

    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[("host", "[::1]:8080"), ("origin", "http://[::1]:8080")]
        )
        .await,
        StatusCode::CREATED,
        "a same-origin IPv6 request was refused"
    );
    assert_eq!(
        write_status(
            app,
            stranger(),
            &[("host", "[::1]:8080"), ("origin", "http://[::1]")]
        )
        .await,
        StatusCode::BAD_REQUEST,
        "port 80 was same-origin with port 8080"
    );
}

#[tokio::test]
async fn an_origin_that_is_not_same_origin_still_reaches_the_cors_policy() {
    // The carve-out is a carve-out, not the whole check: an Origin the policy names is admitted
    // exactly as before, and one that does not parse as an origin falls through to the policy the
    // same way an unmatched one does — where a policy that does not name it refuses it.
    let policy = CorsPolicy::deny_all()
        .allow_origin("https://app.example")
        .expect("a valid origin");
    let app = build(config(Scheme::Http, TrustedProxies::none(), policy));

    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[("host", HOST), ("origin", "https://app.example")]
        )
        .await,
        StatusCode::CREATED,
        "an Origin the CORS policy allows was refused"
    );
    assert_eq!(
        write_status(
            app.clone(),
            stranger(),
            &[("host", HOST), ("origin", "https://app.example/")]
        )
        .await,
        StatusCode::BAD_REQUEST,
        "a value that is not a serialised origin matched a policy entry"
    );
    assert_eq!(
        write_status(app, stranger(), &[("host", HOST), ("origin", "null")]).await,
        StatusCode::BAD_REQUEST
    );
}
