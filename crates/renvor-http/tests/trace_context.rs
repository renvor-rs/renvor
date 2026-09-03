//! Inbound W3C trace context and fetch metadata through the real router (FR-074, FR-075,
//! FR-085).
//!
//! A recording subscriber sees the handler span the router creates: a valid `traceparent`
//! becomes three span fields; an invalid one becomes none, is counted in the application's
//! metrics registry, and the request identifier is what it always was. The handler sees the
//! fetch metadata as bounded values, never as raw headers.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request as HttpRequest;
use http_body_util::BodyExt as _;
use renvor_core::observe::metrics::{Registry, SeriesValue};
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::{
    CorsPolicy, HostPolicy, Request, Response, RouteGroup, RouteRegistry, SecFetchSite,
    TrustedProxies,
};
use tower::ServiceExt as _;

const HOST: &str = "example.test";

/// One recorded span: its name and every field recorded at creation or afterwards.
type Recorded = (String, Vec<(String, String)>);

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<Recorded>>>);

struct Collector(Vec<(String, String)>);

impl tracing::field::Visit for Collector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn core::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
}

impl tracing::Subscriber for Recorder {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, attributes: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        let mut collector = Collector(Vec::new());
        attributes.record(&mut collector);
        let mut spans = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        spans.push((attributes.metadata().name().to_owned(), collector.0));
        tracing::span::Id::from_u64(spans.len() as u64)
    }
    fn record(&self, id: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        let mut collector = Collector(Vec::new());
        values.record(&mut collector);
        let mut spans = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(span) = spans.get_mut(id.into_u64() as usize - 1) {
            span.1.extend(collector.0);
        }
    }
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, _: &tracing::Event<'_>) {}
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

fn config(state: Arc<TypedStateMap>) -> RouterConfig {
    RouterConfig {
        hosts: HostPolicy::deny_all().allow(HOST).expect("a valid host"),
        trusted_proxies: TrustedProxies::none(),
        cors: CorsPolicy::deny_all(),
        limits: renvor_http::Limits::new(),
        run_id: RunIdentifier::generate(&OsEntropy).expect("entropy"),
        cancel: CancelScope::root(),
        gate: WorkGate::new(),
        state,
    }
}

async fn echo_metadata(request: Request) -> Response {
    let metadata = request.fetch_metadata();
    Response::text(format!(
        "{}|{}|{:?}",
        metadata.origin().unwrap_or("-"),
        metadata.origin_host().unwrap_or("-"),
        metadata.sec_fetch_site()
    ))
}

fn registry_with_state() -> (RouteRegistry, Arc<TypedStateMap>, Registry) {
    let mut registry = RouteRegistry::new();
    registry
        .group(
            RouteGroup::new("t", "/t")
                .expect("a valid prefix")
                .get("/echo", echo_metadata)
                .expect("a valid route"),
        )
        .expect("registers");
    let metrics = Registry::new();
    let mut state = TypedStateMap::new();
    state.insert(metrics.clone()).expect("fresh state");
    (registry, Arc::new(state), metrics)
}

fn request(headers: &[(&str, &str)]) -> HttpRequest<Body> {
    let mut builder = HttpRequest::builder()
        .method("GET")
        .uri("/t/echo")
        .header("host", HOST);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut request = builder.body(Body::empty()).expect("a valid request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        40000,
    )));
    request
}

fn handler_span(recorder: &Recorder) -> Recorded {
    recorder
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|(name, _)| name == "renvor.http.handler")
        .cloned()
        .expect("the router created the handler span")
}

fn field(span: &Recorded, name: &str) -> Option<String> {
    span.1
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.clone())
}

fn invalid_count(metrics: &Registry) -> f64 {
    metrics
        .snapshot()
        .families
        .iter()
        .filter(|family| family.name == "renvor_trace_context_inbound_invalid_total")
        .flat_map(|family| family.series.iter())
        .map(|series| match series.value {
            SeriesValue::Scalar(value) => value,
            SeriesValue::Histogram { .. } => 0.0,
        })
        .sum()
}

#[tokio::test]
async fn a_valid_traceparent_becomes_span_fields_and_leaves_the_request_id_alone() {
    let (registry, state, metrics) = registry_with_state();
    let app = router(&registry, config(state)).expect("valid");
    let recorder = Recorder::default();
    let response = {
        let _guard = tracing::subscriber::set_default(recorder.clone());
        app.oneshot(request(&[
            (
                "traceparent",
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            ),
            ("tracestate", "vendor=abc"),
        ]))
        .await
        .expect("the router answers")
    };
    assert_eq!(response.status(), 200);
    let span = handler_span(&recorder);
    assert_eq!(
        field(&span, "trace_id").as_deref(),
        Some("\"4bf92f3577b34da6a3ce929d0e0e4736\"")
    );
    assert_eq!(
        field(&span, "parent_span_id").as_deref(),
        Some("\"00f067aa0ba902b7\"")
    );
    assert_eq!(field(&span, "trace_flags").as_deref(), Some("\"01\""));
    // FR-075: the request identifier is Renvor's own entropy, never the caller's parent id.
    let request_id = field(&span, "request_id").expect("the request id is a span field");
    assert_eq!(
        request_id.len(),
        16,
        "a request id is sixteen hex characters"
    );
    assert_ne!(request_id, "00f067aa0ba902b7");
    assert_eq!(invalid_count(&metrics), 0.0);
    // Nothing is echoed back.
    let headers = response.headers().clone();
    assert!(headers.get("traceparent").is_none());
    assert!(headers.get("tracestate").is_none());
}

#[tokio::test]
async fn an_invalid_context_is_ignored_and_counted_and_a_bad_tracestate_is_dropped_alone() {
    let (registry, state, metrics) = registry_with_state();
    let app = router(&registry, config(state)).expect("valid");
    let recorder = Recorder::default();
    let invalid: [Vec<(&str, &str)>; 3] = [
        // An all-zero trace id.
        vec![(
            "traceparent",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
        )],
        // Uppercase hex.
        vec![(
            "traceparent",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
        )],
        // Garbage.
        vec![("traceparent", "not-a-traceparent")],
    ];
    for (index, headers) in invalid.iter().enumerate() {
        let _guard = tracing::subscriber::set_default(recorder.clone());
        let response = app
            .clone()
            .oneshot(request(headers))
            .await
            .expect("the router answers");
        assert_eq!(response.status(), 200, "case {index} was refused");
        let span = recorder
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .filter(|(name, _)| name == "renvor.http.handler")
            .nth(index)
            .cloned()
            .expect("a handler span per request");
        assert!(
            field(&span, "trace_id").is_none(),
            "case {index} recorded a trace id"
        );
        assert!(field(&span, "request_id").is_some());
    }
    assert_eq!(
        invalid_count(&metrics),
        3.0,
        "each invalid context is counted once"
    );

    // A valid traceparent with an oversized tracestate: W3C §3.3 lets a vendor discard the
    // tracestate and keep the traceparent, which is what the kernel's parser does — so the
    // trace fields are recorded and nothing is counted.
    let oversized = format!("k={}", "v".repeat(600));
    let _guard = tracing::subscriber::set_default(recorder.clone());
    let _ = app
        .clone()
        .oneshot(request(&[
            (
                "traceparent",
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            ),
            ("tracestate", oversized.as_str()),
        ]))
        .await
        .expect("answers");
    let span = recorder
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .filter(|(name, _)| name == "renvor.http.handler")
        .nth(3)
        .cloned()
        .expect("a fourth handler span");
    assert_eq!(
        field(&span, "trace_id").as_deref(),
        Some("\"4bf92f3577b34da6a3ce929d0e0e4736\"")
    );
    assert_eq!(invalid_count(&metrics), 3.0);

    // No traceparent at all: neither a field nor a count.
    let _ = app.oneshot(request(&[])).await.expect("answers");
    assert_eq!(invalid_count(&metrics), 3.0);
}

#[tokio::test]
async fn fetch_metadata_reaches_the_handler_bounded_and_closed() {
    let (registry, state, _metrics) = registry_with_state();
    let app = router(&registry, config(state)).expect("valid");
    let response = app
        .clone()
        .oneshot(request(&[
            ("origin", "https://example.test:8443/"),
            ("sec-fetch-site", "Cross-Site"),
        ]))
        .await
        .expect("answers");
    assert_eq!(response.status(), 200);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    assert_eq!(
        body.as_ref(),
        b"https://example.test:8443/|example.test|Some(CrossSite)"
    );

    let response = app
        .clone()
        .oneshot(request(&[("sec-fetch-site", "something-new")]))
        .await
        .expect("answers");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    assert_eq!(body.as_ref(), b"-|-|Some(Unrecognised)");

    // An Origin over the bound, or holding a control character, is absent — never truncated
    // into something that could match a host. Asserted on the type: the router's own origin
    // policy compares the whole value against the host, so no over-bound origin reaches a
    // handler through it, and the bound is the type's promise either way.
    let long = format!("https://{}.example.test", "a".repeat(1100));
    let metadata = renvor_http::FetchMetadata::new(Some(&long), Some("same-origin"));
    assert!(metadata.origin().is_none());
    assert!(metadata.origin_host().is_none());
    assert_eq!(metadata.sec_fetch_site(), Some(SecFetchSite::SameOrigin));
    let control = renvor_http::FetchMetadata::new(Some("https://example.test\u{7}"), None);
    assert!(control.origin().is_none());

    let response = app.oneshot(request(&[])).await.expect("answers");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    assert_eq!(body.as_ref(), b"-|-|None");
    assert_eq!(
        SecFetchSite::parse(" same-origin "),
        SecFetchSite::SameOrigin
    );
}
