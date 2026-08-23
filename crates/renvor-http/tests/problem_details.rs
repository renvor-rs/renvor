//! RFC 9457 conformance and the redaction suite, through a **real router**.
//!
//! # Canaries, and why every negative search is paired
//!
//! A test asserting "the response does not contain X" passes trivially when the search is broken,
//! when the response is empty, or when the request never reached the code under test. Every
//! negative assertion below is therefore paired with a **positive control** proving the same probe
//! finds the canary when it *is* present.
//!
//! # Nothing here imitates the routing stack
//!
//! Every test drives `renvor_http::route::build::router` — the function a served application calls
//! — through the router's own `Service` implementation, exactly as `tests/router.rs` does.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request as HttpRequest, StatusCode, header};
use http_body_util::BodyExt;
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, WorkGate};
use renvor_error::{Location, PROBLEM_MEDIA_TYPE};
use renvor_http::route::OperationSpec;
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::{CorsPolicy, HostPolicy, Request, Response, RouteRegistry, TrustedProxies};
use renvor_validation::Declaration;
use serde_json::{Value, json};
use tower::ServiceExt;

const HOST: &str = "example.test";

/// A value that would be unmistakable if it ever appeared in a response.
const CANARY: &str = "CANARY-4d81f0ba97c3e526-REJECTED-VALUE";

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

async fn ok(_: Request) -> Response {
    Response::json("{}")
}

async fn panics(_: Request) -> Response {
    // The panic payload carries the canary. It must never reach a response.
    panic!("internal failure while processing {CANARY}");
}

fn registry() -> RouteRegistry {
    let mut registry = RouteRegistry::new();
    registry
        .post("/items", ok)
        .expect("a valid route")
        .describe(
            OperationSpec::new().id("createItem").body(
                true,
                Declaration::new(json!({
                    "type": "object",
                    "required": ["name"],
                    "additionalProperties": false,
                    "properties": {
                        "name": {"type": "string", "minLength": 1, "maxLength": 8},
                        "quantity": {"type": "integer", "minimum": 1, "maximum": 10}
                    }
                }))
                .expect("a valid declaration"),
            ),
        )
        .expect("describes");

    registry
        .get("/items", ok)
        .expect("a valid route")
        .describe(
            OperationSpec::new().id("listItems").parameter(
                Location::Query,
                "page_size",
                false,
                Declaration::new(json!({"type": "integer", "minimum": 1, "maximum": 100}))
                    .expect("a valid declaration"),
            ),
        )
        .expect("describes");

    registry.get("/boom", panics).expect("a valid route");
    registry
}

struct Answer {
    status: StatusCode,
    content_type: String,
    request_id: Option<String>,
    body: String,
}

impl Answer {
    fn problem(&self) -> Value {
        serde_json::from_str(&self.body)
            .unwrap_or_else(|error| panic!("the body is not JSON ({error}): {}", self.body))
    }
}

async fn send(method: &str, uri: &str, body: &str) -> Answer {
    let service = router(&registry(), config()).expect("the router builds");

    let request = HttpRequest::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, HOST)
        .header(header::CONTENT_TYPE, "application/json")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            4000,
        )))
        .body(Body::from(body.to_owned()))
        .expect("a well-formed request");

    let response = service.oneshot(request).await.expect("the router answers");
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let body = String::from_utf8_lossy(
        &response
            .into_body()
            .collect()
            .await
            .expect("the body collects")
            .to_bytes(),
    )
    .into_owned();

    Answer {
        status,
        content_type,
        request_id,
        body,
    }
}

// ---------------------------------------------------------------------------------------------
// RFC 9457 conformance
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_validation_failure_is_rfc_9457_with_the_right_media_type() {
    let answer = send("POST", "/items", r#"{"name":""}"#).await;

    assert_eq!(answer.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        answer.content_type, PROBLEM_MEDIA_TYPE,
        "a problem document was served as something other than application/problem+json"
    );

    let problem = answer.problem();
    for member in ["type", "title", "status", "detail", "instance"] {
        assert!(problem.get(member).is_some(), "`{member}` is missing");
    }
    assert!(problem["status"].is_number(), "`status` is not a number");
    assert_eq!(
        problem["status"].as_u64(),
        Some(u64::from(answer.status.as_u16())),
        "the document's status disagrees with the response's status"
    );
    assert_eq!(problem["code"], json!("validation_failed"));
    assert!(
        problem["type"]
            .as_str()
            .is_some_and(|uri| uri.starts_with("https://renvor.dev/problems/")),
        "`type` is not under the published namespace"
    );
}

#[tokio::test]
async fn invalid_parameters_identify_the_input_safely() {
    let answer = send("POST", "/items", r#"{"name":"","quantity":99}"#).await;
    let problem = answer.problem();

    let params = problem["invalidParams"]
        .as_array()
        .expect("invalidParams is present for a validation failure");
    assert_eq!(
        params.len(),
        2,
        "not every violation was reported: {params:?}"
    );

    for param in params {
        assert!(
            ["path", "query", "header", "body"]
                .contains(&param["location"].as_str().unwrap_or_default()),
            "an invalid parameter has no recognisable location: {param}"
        );
        assert!(param["pointer"].is_string(), "no pointer: {param}");
        assert!(param["reason"].is_string(), "no reason: {param}");
        // THE POINT: there is no member carrying what the caller sent.
        assert!(
            param.get("value").is_none() && param.get("rejected").is_none(),
            "an invalid parameter carries the rejected value: {param}"
        );
    }

    let pointers: Vec<&str> = params
        .iter()
        .filter_map(|param| param["pointer"].as_str())
        .collect();
    assert!(pointers.contains(&"/name"), "got {pointers:?}");
    assert!(pointers.contains(&"/quantity"), "got {pointers:?}");
}

#[tokio::test]
async fn the_correlation_identifier_equals_the_response_header() {
    // FR-015, SC-018. One `RequestId`, three renderings.
    let answer = send("POST", "/items", r#"{"name":""}"#).await;
    let problem = answer.problem();

    let header_value = answer
        .request_id
        .as_deref()
        .expect("every response carries x-request-id");

    assert_eq!(
        problem["correlationId"].as_str(),
        Some(header_value),
        "the document's correlationId and the x-request-id header disagree"
    );
    assert!(
        problem["instance"]
            .as_str()
            .is_some_and(|instance| instance.contains(header_value)),
        "the instance does not identify this occurrence"
    );
}

#[tokio::test]
async fn an_unreadable_body_is_reported_as_malformed_not_as_a_constraint_violation() {
    // FR-008. A document that never parsed has no fields to point at, and telling a caller to
    // correct a field in it would be nonsense.
    let answer = send("POST", "/items", "{not json at all").await;
    let problem = answer.problem();

    assert_eq!(problem["code"], json!("malformed_body"));
    assert!(
        problem.get("invalidParams").is_none(),
        "a body that never parsed reported field-level violations: {problem}"
    );
}

#[tokio::test]
async fn a_missing_required_body_is_its_own_code() {
    let answer = send("POST", "/items", "").await;
    assert_eq!(answer.problem()["code"], json!("missing_body"));
}

#[tokio::test]
async fn a_query_parameter_is_validated_and_reported_by_name() {
    let answer = send("GET", "/items?page_size=9999", "").await;
    let problem = answer.problem();

    assert_eq!(problem["code"], json!("validation_failed"));
    let param = &problem["invalidParams"][0];
    assert_eq!(param["location"], json!("query"));
    assert_eq!(param["pointer"], json!("page_size"));
    assert_eq!(param["reason"], json!("out_of_range"));
}

#[tokio::test]
async fn a_valid_request_reaches_the_handler() {
    // POSITIVE CONTROL for every refusal above. Without it, a validator that rejected everything
    // would pass all of them.
    let answer = send("POST", "/items", r#"{"name":"widget","quantity":3}"#).await;
    assert_eq!(answer.status, StatusCode::OK, "body was {}", answer.body);
    assert_eq!(answer.content_type, "application/json");
}

#[tokio::test]
async fn a_route_without_declarations_is_not_validated() {
    // `/boom` declares nothing, so it is not refused for its input — it reaches the handler, which
    // is what the panic test below depends on.
    let answer = send("GET", "/boom", "").await;
    assert_eq!(
        answer.status,
        StatusCode::INTERNAL_SERVER_ERROR,
        "the undeclared route was refused before the handler ran"
    );
}

// ---------------------------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_rejected_value_never_appears_in_the_response() {
    // POSITIVE CONTROL: prove the probe discriminates.
    assert!(
        format!(r#"{{"detail":"{CANARY}"}}"#).contains(CANARY),
        "the canary probe does not discriminate"
    );

    // The canary as a value, as an unknown member name, and inside a nested value.
    for body in [
        format!(r#"{{"name":"{CANARY}"}}"#),
        format!(r#"{{"name":"ok","{CANARY}":1}}"#),
        format!(r#"{{"name":"ok","quantity":"{CANARY}"}}"#),
    ] {
        let answer = send("POST", "/items", &body).await;
        assert!(
            !answer.body.contains(CANARY),
            "the rejected value reached the response for body `{body}`: {}",
            answer.body
        );
        // And the response is not empty, so the assertion above is not passing vacuously.
        assert!(
            answer.body.contains("\"code\""),
            "the response carried no problem document at all: {}",
            answer.body
        );
    }
}

#[tokio::test]
async fn a_panic_payload_never_reaches_the_response() {
    let answer = send("GET", "/boom", "").await;

    assert_eq!(answer.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(answer.content_type, PROBLEM_MEDIA_TYPE);

    let problem = answer.problem();
    assert_eq!(problem["code"], json!("internal_error"));
    assert!(
        !answer.body.contains(CANARY),
        "the panic payload reached the response: {}",
        answer.body
    );
    // The detail is the FIXED constant, carrying nothing about the cause.
    assert_eq!(
        problem["detail"],
        json!("The request could not be completed.")
    );
}

#[tokio::test]
async fn no_response_carries_an_internal_disclosure() {
    // SC-002. Everything that must never appear, over every failure this suite can produce.
    let forbidden = [
        "src/",
        ".rs:",
        "panic",
        "unwrap",
        "backtrace",
        "thread '",
        "SELECT ",
        "INSERT ",
        "/Users/",
        "C:\\\\",
        "Caused by",
    ];

    // POSITIVE CONTROL: the probe finds these when present.
    let sample = "thread 'main' panicked at src/lib.rs:12";
    assert!(
        forbidden.iter().any(|needle| sample.contains(needle)),
        "the disclosure probe does not discriminate"
    );

    for (method, uri, body) in [
        ("POST", "/items", r#"{"name":""}"#),
        ("POST", "/items", "{broken"),
        ("POST", "/items", ""),
        ("GET", "/items?page_size=0", ""),
        ("GET", "/boom", ""),
        ("GET", "/nowhere", ""),
    ] {
        let answer = send(method, uri, body).await;
        for needle in forbidden {
            assert!(
                !answer.body.contains(needle),
                "`{method} {uri}` disclosed `{needle}`: {}",
                answer.body
            );
        }
    }
}

#[tokio::test]
async fn an_unmatched_path_is_also_a_problem_document() {
    // Phase 004 answered a 404 as plain text. A machine-readable failure is what Phase 005
    // promises, and it must hold for the paths the router itself answers — not only for the ones
    // a handler reached.
    let answer = send("GET", "/nowhere", "").await;

    assert_eq!(answer.status, StatusCode::NOT_FOUND);
    assert_eq!(answer.content_type, PROBLEM_MEDIA_TYPE);
    assert_eq!(answer.problem()["code"], json!("not_found"));
    assert!(
        answer.request_id.is_some(),
        "an unmatched path lost its request identifier"
    );
}

#[tokio::test]
async fn a_rejected_host_is_a_problem_document_with_no_host_echoed() {
    let service = router(&registry(), config()).expect("the router builds");
    let request = HttpRequest::builder()
        .method("GET")
        .uri("/items")
        .header(header::HOST, "evil.example")
        .extension(ConnectInfo(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            4000,
        )))
        .body(Body::empty())
        .expect("a well-formed request");

    let response = service.oneshot(request).await.expect("the router answers");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = String::from_utf8_lossy(
        &response
            .into_body()
            .collect()
            .await
            .expect("collects")
            .to_bytes(),
    )
    .into_owned();

    let problem: Value = serde_json::from_str(&body).expect("a problem document");
    assert_eq!(problem["code"], json!("host_rejected"));
    assert!(
        !body.contains("evil.example"),
        "the rejected host was echoed back: {body}"
    );
}
