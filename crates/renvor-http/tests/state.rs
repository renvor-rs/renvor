//! FR-012 and FR-013 — application state reaches a handler, and a missing entry is reported.
//!
//! # What these assert that a unit test could not
//!
//! The kernel's `TypedStateMap` already has its own tests. What was missing is the **bridge**: that
//! a value registered by an application is reachable from a handler *through a real router*, and
//! that a lookup which cannot be satisfied is an explicit reported failure rather than a panic or a
//! substituted default.
//!
//! Every test here drives `axum::Router` through `tower::ServiceExt::oneshot`, so what is asserted
//! is the served path rather than a description of it.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

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

/// What an application registers. A plain type — it names no transport type, which is the point.
#[derive(Debug, PartialEq)]
struct Database {
    dsn: &'static str,
}

/// Registered by no test, so every lookup of it must report missing.
#[derive(Debug)]
struct NeverRegistered;

fn config(state: Arc<TypedStateMap>) -> RouterConfig {
    RouterConfig {
        hosts: HostPolicy::deny_all().allow(HOST).expect("a valid host"),
        trusted_proxies: TrustedProxies::none(),
        cors: CorsPolicy::deny_all(),
        limits: Limits::new(),
        run_id: RunIdentifier::generate(&OsEntropy).expect("entropy"),
        cancel: CancelScope::root(),
        gate: WorkGate::new(),
        state,
    }
}

fn state_with_database() -> Arc<TypedStateMap> {
    let mut state = TypedStateMap::new();
    state
        .insert(Database {
            dsn: "postgres://example",
        })
        .expect("the first registration succeeds");
    Arc::new(state)
}

/// Reads registered state and reports what it found.
async fn reads_state(request: Request) -> Response {
    match request.state::<Database>() {
        Ok(database) => Response::text(format!("found:{}", database.dsn)),
        Err(error) => Response::status(500)
            .expect("a status HTTP defines")
            .with_body(format!("missing:{error}")),
    }
}

/// Reads a type nothing registered. FR-013's case.
async fn reads_absent_state(request: Request) -> Response {
    match request.state::<NeverRegistered>() {
        Ok(_) => Response::text("found"),
        Err(error) => Response::status(503)
            .expect("a status HTTP defines")
            .with_body(format!("reported:{error}")),
    }
}

fn registry() -> RouteRegistry {
    let mut registry = RouteRegistry::new();
    registry.get("/state", reads_state).expect("route");
    registry.get("/absent", reads_absent_state).expect("route");
    registry
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
        .expect("the body collects")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

#[tokio::test]
async fn registered_state_reaches_a_handler_through_a_real_router() {
    // FR-012. The bridge exists, or this cannot pass.
    let app = router(&registry(), config(state_with_database())).expect("valid");

    let response = app.oneshot(served("/state")).await.expect("responds");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_string(response).await, "found:postgres://example");
}

#[tokio::test]
async fn a_missing_state_entry_is_an_explicit_reported_failure() {
    // FR-013. Not a panic, and not a substituted default: the handler receives an error value and
    // is able to report it.
    let app = router(&registry(), config(state_with_database())).expect("valid");

    let response = app.oneshot(served("/absent")).await.expect("responds");

    // The handler ran and chose its own status, which is only possible if it received an `Err`
    // rather than a panic or a zero value.
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = body_string(response).await;
    assert!(body.starts_with("reported:"), "{body}");
    assert!(
        body.contains("NeverRegistered"),
        "the failure must name the type that was missing: {body}"
    );
}

#[tokio::test]
async fn a_handler_reading_state_from_an_empty_map_reports_rather_than_panics() {
    // POSITIVE CONTROL for the two above: with NOTHING registered, the SAME lookup that succeeded
    // above now reports. So the success above is a fact about the registered value rather than
    // about the lookup always succeeding, and the failure above is a fact about the absent type
    // rather than about the lookup always failing.
    let app = router(&registry(), config(Arc::new(TypedStateMap::new()))).expect("valid");

    let response = app.oneshot(served("/state")).await.expect("responds");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = body_string(response).await;
    assert!(body.starts_with("missing:"), "{body}");
    assert!(body.contains("Database"), "{body}");
}
