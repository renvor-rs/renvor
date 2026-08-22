//! C-11 layer 9 and C-10 *Errors and telemetry*, asserted by observation.
//!
//! Both properties were implemented and neither was observable: the handler span existed in the
//! source and no test entered a subscriber to see it, and the run identifier was written into every
//! refusal record with nothing reading one back. A field that is only ever written is a field that
//! can be deleted without a single test noticing.
//!
//! # Why a hand-written subscriber
//!
//! `tracing-subscriber` would do this in a few lines, and it is **not** added as a dependency for
//! two test assertions. The `Subscriber` trait is part of `tracing`, which this crate already
//! depends on, so the capture below costs nothing beyond this file.

use std::sync::{Arc, Mutex, PoisonError};

use axum::http::{StatusCode, header};
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::route::{Request, Response, RouteRegistry};
use renvor_http::{CorsPolicy, HostPolicy, Limits, TrustedProxies};
use tower::ServiceExt as _;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

const HOST: &str = "api.example";

// ---------------------------------------------------------------------------------------------
// The capture
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Captured {
    span_names: Vec<String>,
    span_fields: Vec<Vec<(String, String)>>,
    event_fields: Vec<Vec<(String, String)>>,
}

#[derive(Clone, Default)]
struct Recorder {
    seen: Arc<Mutex<Captured>>,
}

/// Collects field name/value pairs, whichever way `tracing` chooses to hand them over.
struct Fields(Vec<(String, String)>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name().to_owned(), value.to_owned()));
    }
}

impl Subscriber for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, attributes: &Attributes<'_>) -> Id {
        let mut fields = Fields(Vec::new());
        attributes.record(&mut fields);

        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.span_names
            .push(attributes.metadata().name().to_owned());
        seen.span_fields.push(fields.0);

        // `Id` may not be zero, and one span has been pushed by the time this runs.
        Id::from_u64(seen.span_names.len() as u64)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields(Vec::new());
        event.record(&mut fields);
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .event_fields
            .push(fields.0);
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

// ---------------------------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------------------------

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

fn app(configured: RouterConfig) -> axum::Router {
    let mut registry = RouteRegistry::new();
    registry
        .get("/declared", |_: Request| async { Response::text("ok") })
        .expect("route");
    router(&registry, configured).expect("a valid router")
}

fn served(builder: axum::http::request::Builder) -> axum::http::Request<axum::body::Body> {
    let mut request = builder.body(axum::body::Body::empty()).expect("valid");
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "203.0.113.9:44321"
            .parse::<std::net::SocketAddr>()
            .expect("a valid peer"),
    ));
    request
}

// ---------------------------------------------------------------------------------------------
// C-11 layer 9 — the span covers handler execution
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_served_request_opens_the_handler_span_with_structured_fields_only() {
    let recorder = Recorder::default();
    let seen = Arc::clone(&recorder.seen);
    let guard = tracing::subscriber::set_default(recorder);

    let response = app(config())
        .oneshot(served(
            axum::http::Request::builder()
                .method("GET")
                .uri("/declared")
                .header(header::HOST, HOST),
        ))
        .await
        .expect("responds");
    assert_eq!(response.status(), StatusCode::OK);
    drop(guard);

    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    let index = seen
        .span_names
        .iter()
        .position(|name| name == "renvor.http.handler")
        .expect("C-11 layer 9 requires a span covering handler execution");

    let fields = &seen.span_fields[index];
    for required in ["route", "request_id", "run_id"] {
        assert!(
            fields.iter().any(|(name, _)| name == required),
            "the handler span is missing the `{required}` field: {fields:?}"
        );
    }

    // C-O2: structured fields only. A span whose message carried an interpolated sentence would
    // record it under the reserved `message` field.
    assert!(
        !fields.iter().any(|(name, _)| name == "message"),
        "the handler span carries an interpolated message: {fields:?}"
    );
}

#[tokio::test]
async fn the_capture_finds_no_handler_span_when_no_request_is_served() {
    // POSITIVE CONTROL for the test above. If the capture reported the span whether or not one was
    // opened, the assertion there would hold for a router that opened none.
    let recorder = Recorder::default();
    let seen = Arc::clone(&recorder.seen);
    let guard = tracing::subscriber::set_default(recorder);
    let _unused = app(config());
    drop(guard);

    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        !seen
            .span_names
            .iter()
            .any(|name| name == "renvor.http.handler"),
        "the capture reports a handler span for a request that was never made"
    );
}

// ---------------------------------------------------------------------------------------------
// C-10 — a refusal record carries the run identifier
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn a_refusal_record_carries_the_run_identifier() {
    // C-O3: emitted records carry the run identifier with the request identifier nested beneath it.
    // A refusal that is not correlatable is an incident with no trail — and refusals are the
    // records an operator reaches for first.
    let configured = config();
    let run_id = configured.run_id.to_string();

    let recorder = Recorder::default();
    let seen = Arc::clone(&recorder.seen);
    let guard = tracing::subscriber::set_default(recorder);

    let response = app(configured)
        .oneshot(served(
            axum::http::Request::builder()
                .method("GET")
                .uri("/declared")
                .header(header::HOST, "evil.example"),
        ))
        .await
        .expect("responds");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    drop(guard);

    let seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
    let refusal = seen
        .event_fields
        .iter()
        .find(|fields| fields.iter().any(|(name, _)| name == "code"))
        .expect("a refused request emitted no record naming its code");

    let recorded = refusal
        .iter()
        .find(|(name, _)| name == "run_id")
        .map(|(_, value)| value.clone())
        .expect("the refusal record carries no run identifier");

    assert_eq!(
        recorded, run_id,
        "the refusal recorded a run identifier that is not this run's"
    );
}
