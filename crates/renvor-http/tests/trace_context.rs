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
use axum::http::{HeaderValue, Request as HttpRequest};
use http_body_util::BodyExt as _;
use renvor_core::observe::metrics::{Registry, SeriesValue};
use renvor_core::observe::{TraceContext, TraceState};
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::Scheme;
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
        public_scheme: Scheme::Http,
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
        metadata
            .effective_origin()
            .map_or("-".to_owned(), |origin| format!(
                "{}://{}:{}",
                origin.scheme().as_str(),
                origin.host(),
                origin.port()
            )),
        metadata.sec_fetch_site()
    ))
}

/// Reports the combined `tracestate` the router validated, or `-`.
///
/// Reads it through the ONLY door a handler has — `Request::trace_context` — which is the door an
/// outbound propagator or a job enqueue would use. A test that reached into the router instead
/// would prove nothing about what a handler can propagate.
async fn echo_tracestate(request: Request) -> Response {
    Response::text(
        request
            .trace_context()
            .and_then(TraceContext::state)
            .map_or("-", TraceState::as_str)
            .to_owned(),
    )
}

fn registry_with_state() -> (RouteRegistry, Arc<TypedStateMap>, Registry) {
    let mut registry = RouteRegistry::new();
    registry
        .group(
            RouteGroup::new("t", "/t")
                .expect("a valid prefix")
                .get("/echo", echo_metadata)
                .expect("a valid route")
                .get("/tracestate", echo_tracestate)
                .expect("a valid route"),
        )
        .expect("registers");
    let metrics = Registry::new();
    let mut state = TypedStateMap::new();
    state.insert(metrics.clone()).expect("fresh state");
    (registry, Arc::new(state), metrics)
}

fn request(headers: &[(&str, &str)]) -> HttpRequest<Body> {
    request_to(
        "/t/echo",
        headers
            .iter()
            .map(|(name, value)| (*name, HeaderValue::from_str(value).expect("a header value")))
            .collect(),
    )
}

/// A request to `uri` carrying `headers` as raw values, so a field that is not visible ASCII
/// (legal on the wire as obs-text, unreadable as text) can be sent.
fn request_to(uri: &str, headers: Vec<(&str, HeaderValue)>) -> HttpRequest<Body> {
    let mut builder = HttpRequest::builder()
        .method("GET")
        .uri(uri)
        .header("host", HOST);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    let mut request = builder.body(Body::empty()).expect("a valid request");
    request.extensions_mut().insert(ConnectInfo(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::LOCALHOST),
        40000,
    )));
    request
}

/// Sends a request to the tracestate echo and returns what the handler could propagate.
async fn tracestate_seen(app: axum::Router, headers: Vec<(&str, HeaderValue)>) -> String {
    let response = app
        .oneshot(request_to("/t/tracestate", headers))
        .await
        .expect("answers");
    assert_eq!(response.status(), 200);
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    String::from_utf8(bytes.to_vec()).expect("utf-8")
}

const TRACEPARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

fn text(value: &'static str) -> HeaderValue {
    HeaderValue::from_static(value)
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
    // The Origin is the request's OWN origin — the router's same-origin carve-out compares the
    // full triple (RFC 6454 §5), so only a same-origin `Origin` reaches a handler under the
    // default deny-all CORS policy. The handler sees the raw value AND the resolved triple, with
    // the default port applied.
    let response = app
        .clone()
        .oneshot(request(&[
            ("origin", "http://example.test"),
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
        b"http://example.test|http://example.test:80|Some(CrossSite)"
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
    assert!(metadata.effective_origin().is_none());
    assert_eq!(metadata.sec_fetch_site(), Some(SecFetchSite::SameOrigin));
    let control = renvor_http::FetchMetadata::new(Some("https://example.test\u{7}"), None);
    assert!(control.origin().is_none());
    assert!(control.effective_origin().is_none());
    // A value that is a URL rather than a serialised origin (RFC 6454 §6.1) is kept raw and
    // resolves to NO origin: it is never trimmed down to the origin it contains.
    let url = renvor_http::FetchMetadata::new(Some("https://example.test:8443/"), None);
    assert_eq!(url.origin(), Some("https://example.test:8443/"));
    assert!(url.effective_origin().is_none());

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

#[tokio::test]
async fn a_repeated_traceparent_is_invalid_and_counted_once() {
    // W3C Trace Context §3.2 defines `traceparent` as ONE field. RFC 7230 §3.2.2 forbids a sender
    // repeating a field whose value is not a list and gives a recipient no rule for combining one.
    // Reading the first — which `HeaderMap::get` does — is choosing between two values, which is
    // exactly what this crate's repeated-`Host` and repeated-`Forwarded` rules refuse to do: the
    // hop in front of Renvor may choose the other one. Phase 010 correction round, finding 4.
    let (registry, state, metrics) = registry_with_state();
    let app = router(&registry, config(state)).expect("valid");
    let recorder = Recorder::default();
    let response = {
        let _guard = tracing::subscriber::set_default(recorder.clone());
        app.oneshot(request(&[
            ("traceparent", TRACEPARENT),
            (
                "traceparent",
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b8-01",
            ),
        ]))
        .await
        .expect("the router answers")
    };
    assert_eq!(
        response.status(),
        200,
        "a repeated traceparent refused the request"
    );
    let span = handler_span(&recorder);
    assert!(
        field(&span, "trace_id").is_none(),
        "one of two traceparent values was adopted"
    );
    assert!(field(&span, "parent_span_id").is_none());
    assert!(field(&span, "trace_flags").is_none());
    assert!(field(&span, "request_id").is_some());
    assert_eq!(
        invalid_count(&metrics),
        1.0,
        "a repeated traceparent was not counted as an invalid context"
    );
}

#[tokio::test]
async fn multiple_tracestate_fields_are_combined_in_arrival_order() {
    // W3C §3.3.1.1: "multiple tracestate header fields MUST be combined per RFC 7230 §3.2.2" —
    // joined with `,`, in the order received. Only the first field was read, so a vendor whose
    // entry arrived in a second field was silently dropped from what a handler could propagate.
    let (registry, state, metrics) = registry_with_state();
    let app = router(&registry, config(state)).expect("valid");

    assert_eq!(
        tracestate_seen(
            app.clone(),
            vec![
                ("traceparent", text(TRACEPARENT)),
                ("tracestate", text("a=1")),
                ("tracestate", text("b=2")),
            ],
        )
        .await,
        "a=1,b=2",
        "two tracestate fields were not combined in arrival order"
    );
    // REVERSED ARRIVAL, reversed result: the order is the wire's, not a sort.
    assert_eq!(
        tracestate_seen(
            app.clone(),
            vec![
                ("traceparent", text(TRACEPARENT)),
                ("tracestate", text("b=2")),
                ("tracestate", text("a=1")),
            ],
        )
        .await,
        "b=2,a=1"
    );
    // Three fields, one of them itself a list: still one combined value.
    assert_eq!(
        tracestate_seen(
            app.clone(),
            vec![
                ("traceparent", text(TRACEPARENT)),
                ("tracestate", text("a=1,b=2")),
                ("tracestate", text("c=3")),
                ("tracestate", text("d=4")),
            ],
        )
        .await,
        "a=1,b=2,c=3,d=4"
    );

    // POSITIVE CONTROLS: a single field and no field behave as before.
    assert_eq!(
        tracestate_seen(
            app.clone(),
            vec![
                ("traceparent", text(TRACEPARENT)),
                ("tracestate", text("a=1"))
            ],
        )
        .await,
        "a=1"
    );
    assert_eq!(
        tracestate_seen(app.clone(), vec![("traceparent", text(TRACEPARENT))]).await,
        "-"
    );
    assert_eq!(
        invalid_count(&metrics),
        0.0,
        "a valid traceparent was counted"
    );
}

#[tokio::test]
async fn a_tracestate_field_that_is_not_visible_ascii_drops_the_whole_tracestate_alone() {
    // One field the transport cannot read as text makes the COMBINED value unreadable, and the
    // whole tracestate is dropped — which §3.3 permits — while the traceparent is kept and nothing
    // is counted. Combining the readable fields and skipping the other would hand a handler a
    // tracestate the caller never sent.
    let (registry, state, metrics) = registry_with_state();
    let app = router(&registry, config(state)).expect("valid");
    let recorder = Recorder::default();
    let seen = {
        let _guard = tracing::subscriber::set_default(recorder.clone());
        tracestate_seen(
            app.clone(),
            vec![
                ("traceparent", text(TRACEPARENT)),
                ("tracestate", text("a=1")),
                (
                    "tracestate",
                    HeaderValue::from_bytes(b"b=\xff").expect("obs-text is a legal header byte"),
                ),
            ],
        )
        .await
    };
    assert_eq!(seen, "-", "an unreadable field did not drop the tracestate");
    let span = handler_span(&recorder);
    assert_eq!(
        field(&span, "trace_id").as_deref(),
        Some("\"4bf92f3577b34da6a3ce929d0e0e4736\""),
        "the traceparent was lost with the tracestate"
    );
    assert_eq!(invalid_count(&metrics), 0.0);

    // POSITIVE CONTROL: the same two fields, both readable, combine — so the drop above is about
    // the unreadable byte rather than about two fields never combining.
    assert_eq!(
        tracestate_seen(
            app,
            vec![
                ("traceparent", text(TRACEPARENT)),
                ("tracestate", text("a=1")),
                ("tracestate", text("b=2")),
            ],
        )
        .await,
        "a=1,b=2"
    );
}
