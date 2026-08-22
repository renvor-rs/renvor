//! FR-008 and contract C-9 — a group contributes a prefix **and** middleware, and groups nest.
//!
//! # What was missing
//!
//! `RouteGroup` stored a name, a prefix, and routes, and nothing else. C-9 promises that *"a
//! group's middleware applies to every route it contains, and to no route outside it"* and that
//! *"nested groups compose left to right"*. Neither existed, so the contract described a capability
//! the implementation did not have.
//!
//! # Ordering is asserted as an onion, not as a list
//!
//! A middleware chain's order is only meaningful if you can see *both* halves — the run inward and
//! the run back out. Every test here records entry and exit separately, so a chain that ran the
//! layers in the right order but unwound in the wrong one would fail.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request as HttpRequest, StatusCode, header};
use http_body_util::BodyExt;
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::route::{Next, Request, Response, RouteGroup, RouteRegistry};
use renvor_http::{CorsPolicy, HostPolicy, Limits, TrustedProxies};
use tower::ServiceExt;

const HOST: &str = "example.test";

type Journal = Arc<Mutex<Vec<String>>>;

fn journal() -> Journal {
    Arc::new(Mutex::new(Vec::new()))
}

fn note(journal: &Journal, event: &str) {
    journal
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(event.to_owned());
}

fn entries(journal: &Journal) -> Vec<String> {
    journal
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

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

fn served(path: &str) -> HttpRequest<Body> {
    let mut request = HttpRequest::builder()
        .method("GET")
        .uri(path)
        .header(header::HOST, HOST)
        .body(Body::empty())
        .expect("a valid request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        40000,
    )));
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

/// Builds a middleware that records its entry and its exit.
fn recorder(journal: &Journal, name: &'static str) -> impl renvor_http::route::Middleware {
    let journal = Arc::clone(journal);
    move |request: Request, next: Next| {
        let journal = Arc::clone(&journal);
        async move {
            note(&journal, &format!("{name}:in"));
            let response = next.run(request).await;
            note(&journal, &format!("{name}:out"));
            response
        }
    }
}

#[tokio::test]
async fn group_middleware_runs_inside_the_group_and_not_outside_it() {
    let log = journal();

    let group = RouteGroup::new("guarded", "/guarded")
        .expect("prefix")
        .layer(recorder(&log, "guard"))
        .get("/inside", |_: Request| async { Response::text("inside") })
        .expect("route");

    let mut registry = RouteRegistry::new();
    registry.group(group).expect("group registers");
    registry
        .get("/outside", |_: Request| async { Response::text("outside") })
        .expect("route");

    let app = router(&registry, config()).expect("valid");

    let inside = app
        .clone()
        .oneshot(served("/guarded/inside"))
        .await
        .expect("responds");
    assert_eq!(inside.status(), StatusCode::OK);
    assert_eq!(body_string(inside).await, "inside");
    assert_eq!(
        entries(&log),
        vec!["guard:in", "guard:out"],
        "the group's middleware did not run for a route inside it"
    );

    // POSITIVE CONTROL, and the half that matters most: a route OUTSIDE the group must not be
    // touched by it. A middleware applied to everything would pass the assertion above.
    let outside = app.oneshot(served("/outside")).await.expect("responds");
    assert_eq!(body_string(outside).await, "outside");
    assert_eq!(
        entries(&log),
        vec!["guard:in", "guard:out"],
        "the group's middleware ran for a route outside the group"
    );
}

#[tokio::test]
async fn nested_groups_apply_both_layers_with_the_outer_one_outermost() {
    let log = journal();

    let inner = RouteGroup::new("inner", "/v1")
        .expect("prefix")
        .layer(recorder(&log, "inner"))
        .get("/thing", |_: Request| async { Response::text("handled") })
        .expect("route");

    let outer = RouteGroup::new("outer", "/api")
        .expect("prefix")
        .layer(recorder(&log, "outer"))
        .group(inner)
        .expect("nested group composes");

    let mut registry = RouteRegistry::new();
    registry.group(outer).expect("registers");

    // The prefixes composed left to right.
    assert_eq!(
        registry.routes()[0].path(),
        "/api/v1/thing",
        "nested prefixes did not compose"
    );

    let app = router(&registry, config()).expect("valid");
    let response = app
        .oneshot(served("/api/v1/thing"))
        .await
        .expect("responds");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "handled");

    // An onion, not a list: the outer layer is entered FIRST and exited LAST.
    assert_eq!(
        entries(&log),
        vec!["outer:in", "inner:in", "inner:out", "outer:out"],
        "the nested middleware did not compose outermost-first"
    );
}

#[tokio::test]
async fn a_middleware_that_answers_short_circuits_before_the_handler() {
    let log = journal();
    let reached = Arc::new(Mutex::new(false));
    let handler_reached = Arc::clone(&reached);

    let refusing = {
        let log = Arc::clone(&log);
        move |_: Request, _: Next| {
            let log = Arc::clone(&log);
            async move {
                note(&log, "refused");
                Response::status(403).expect("a status HTTP defines")
            }
        }
    };

    let group = RouteGroup::new("closed", "/closed")
        .expect("prefix")
        .layer(refusing)
        .get("/thing", move |_: Request| {
            let handler_reached = Arc::clone(&handler_reached);
            async move {
                *handler_reached
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner) = true;
                Response::text("should not run")
            }
        })
        .expect("route");

    let mut registry = RouteRegistry::new();
    registry.group(group).expect("registers");
    let app = router(&registry, config()).expect("valid");

    let response = app
        .oneshot(served("/closed/thing"))
        .await
        .expect("responds");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(entries(&log), vec!["refused"]);
    assert!(
        !*reached.lock().unwrap_or_else(PoisonError::into_inner),
        "the handler ran despite the middleware answering instead"
    );
}

#[tokio::test]
async fn a_route_with_no_group_middleware_reaches_its_handler_unchanged() {
    // POSITIVE CONTROL for the whole file: the chain machinery does not alter a route that has no
    // middleware, so every assertion above is about the middleware rather than about the chain.
    let mut registry = RouteRegistry::new();
    registry
        .get("/plain", |_: Request| async { Response::text("plain") })
        .expect("route");

    let app = router(&registry, config()).expect("valid");
    let response = app.oneshot(served("/plain")).await.expect("responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "plain");
}

#[tokio::test]
async fn middleware_sees_the_resolved_context_and_no_transport_type() {
    // The middleware seam is transport-neutral: what it receives is the same `Request` a handler
    // receives, carrying the context the security layers resolved.
    let seen = Arc::new(Mutex::new(String::new()));
    let recorded = Arc::clone(&seen);

    let inspecting = move |request: Request, next: Next| {
        let recorded = Arc::clone(&recorded);
        async move {
            *recorded.lock().unwrap_or_else(PoisonError::into_inner) =
                request.context().host().to_owned();
            next.run(request).await
        }
    };

    let group = RouteGroup::new("seen", "/seen")
        .expect("prefix")
        .layer(inspecting)
        .get("/thing", |_: Request| async { Response::text("ok") })
        .expect("route");

    let mut registry = RouteRegistry::new();
    registry.group(group).expect("registers");
    let app = router(&registry, config()).expect("valid");

    app.oneshot(served("/seen/thing")).await.expect("responds");

    assert_eq!(
        *seen.lock().unwrap_or_else(PoisonError::into_inner),
        HOST,
        "the middleware did not receive the validated host"
    );
}
