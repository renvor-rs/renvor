//! Caller-controlled text does not reach a refusal's telemetry record, its body, or its rendering.
//!
//! # What this file exists to prevent, and what it cannot prevent on its own
//!
//! `HttpError::new` used to take `detail: impl Into<String>`, and `refuse` emitted that string to
//! `tracing` **by design** — `contracts/problem-details.md` said the operator-facing detail *"goes
//! to the telemetry record, which is a different consumer with different rights"*. This phase's
//! second correction round withdrew exactly that defence for the database adapters, on the finding
//! that `CONSTITUTION.md` principle VI names telemetry and exempts no consumer. It reported this
//! site as equivalent in shape and left it, because changing it was a contract change outside a
//! database-scoped authority.
//!
//! The channel is now closed at the type: [`HttpErrorDetail`] is fieldless, so a runtime string has
//! nowhere to enter. **That is the property, and a test cannot prove it** — a type-level guarantee
//! is proven by the compiler, and the control for it is the `compile_fail` pair on
//! `renvor_core::closed_named_enum`.
//!
//! What this file proves is the half a compiler cannot: that the values which flow through a real
//! refusal — a host, an origin, a path, a body, all chosen by the caller — reach **no** field of
//! **no** emitted event, and that the fields which are emitted are the reviewed ones. Without the
//! second half the first is satisfied by a router that emits nothing at all.
//!
//! # Why a hand-written subscriber
//!
//! `tracing-subscriber` would do this in a few lines, and it is **not** added as a dependency for
//! one test file. `crates/renvor-http/tests/telemetry.rs` set that precedent in this crate for the
//! same reason, and this follows it.

use std::sync::{Arc, Mutex, PoisonError};

use axum::http::{StatusCode, header};
use renvor_core::{CancelScope, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::route::{Request, Response, RouteRegistry};
use renvor_http::{CorsPolicy, HostPolicy, HttpErrorDetail, Limits, TrustedProxies};
use tower::ServiceExt as _;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

const HOST: &str = "api.example";

/// The value planted in every caller-controlled position a detail might once have interpolated.
///
/// One token, so a single `contains` finds it wherever it surfaced. It is deliberately not a
/// realistic secret: a realistic one would be a real secret in a source file.
const CANARY: &str = "CANARYdb1c4a";

/// Terms that would indicate a leak even without the canary — the shapes a real detail carried.
const FORBIDDEN: [&str; 4] = [CANARY, "postgres://", "Bearer ", "/srv/app/secrets"];

// ---------------------------------------------------------------------------------------------
// The capture
// ---------------------------------------------------------------------------------------------

type CapturedEvent = Vec<(String, String)>;

#[derive(Clone, Default)]
struct Recorder {
    seen: Arc<Mutex<Vec<CapturedEvent>>>,
}

struct Fields(CapturedEvent);

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
        // Span fields are captured as events too: a span is a rendering path like any other, and a
        // leak that only ever appeared on a span would be invisible to an event-only capture.
        let mut fields = Fields(Vec::new());
        attributes.record(&mut fields);
        let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        seen.push(fields.0);
        Id::from_u64(seen.len() as u64)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields(Vec::new());
        event.record(&mut fields);
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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

/// Every refusal reachable through the real router, each carrying the canary where a caller can
/// put one.
///
/// Used for the **response-body** assertion, which applies to every refusal however it is answered.
fn refusals() -> Vec<(&'static str, axum::http::request::Builder)> {
    vec![
        (
            "unknown path",
            axum::http::Request::builder()
                .method("GET")
                .uri(format!("/no-such-route-{CANARY}"))
                .header(header::HOST, HOST),
        ),
        (
            "rejected host",
            axum::http::Request::builder()
                .method("GET")
                .uri("/declared")
                .header(header::HOST, format!("{CANARY}.example")),
        ),
        (
            "rejected origin",
            axum::http::Request::builder()
                .method("GET")
                .uri("/declared")
                .header(header::HOST, HOST)
                .header(header::ORIGIN, format!("https://{CANARY}.example")),
        ),
    ]
}

/// The subset of [`refusals`] that is answered through `refuse`, and therefore emits a record.
///
/// # The unknown-path case is excluded, and it is excluded on a measurement
///
/// A 404 is answered by `not_found`, which calls `problem::from_http_error` directly and **emits
/// no telemetry at all** — it is the one refusal in this crate that does not pass through
/// `refuse`. That is pre-existing behaviour, unchanged by this correction and outside its
/// authority; it is named here so the exclusion reads as a measured fact rather than as a case
/// somebody quietly dropped when it failed.
///
/// It remains in [`refusals`], because the response-body assertion applies to it either way.
fn logged_refusals() -> Vec<(&'static str, axum::http::request::Builder)> {
    refusals()
        .into_iter()
        .filter(|(case, _)| *case != "unknown path")
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The assertions
// ---------------------------------------------------------------------------------------------

/// No caller-controlled value reaches any field of any record a refusal emits.
#[tokio::test]
async fn a_refusal_records_nothing_the_caller_supplied() {
    for (case, builder) in logged_refusals() {
        let recorder = Recorder::default();
        let seen = Arc::clone(&recorder.seen);
        let guard = tracing::subscriber::set_default(recorder);
        let response = app(config())
            .oneshot(served(builder))
            .await
            .expect("responds");
        drop(guard);

        assert!(
            response.status().is_client_error(),
            "case `{case}` was not refused, so it measured nothing"
        );

        let records = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
        assert!(
            !records.is_empty(),
            "case `{case}` emitted no records at all, so its absence assertions are vacuous"
        );

        for (index, record) in records.iter().enumerate() {
            for (position, (name, value)) in record.iter().enumerate() {
                for leak in FORBIDDEN {
                    // Neither the value nor the captured record is interpolated into this message.
                    // A failure that printed the leak would put it in CI output, which is one more
                    // log than the one this test exists to keep clean.
                    assert!(
                        !value.contains(leak) && !name.contains(leak),
                        "case `{case}`: record {index} field {position} carried a planted value \
                         into telemetry"
                    );
                }
            }
        }
    }
}

/// POSITIVE CONTROL. The refusal record carries the reviewed fields, from the closed set.
///
/// Without this, [`a_refusal_records_nothing_the_caller_supplied`] passes against a router that
/// stopped recording refusals — and a deleted field is not a redacted one.
#[tokio::test]
async fn a_refusal_still_records_its_code_and_a_reviewed_detail() {
    let permitted = HttpErrorDetail::ALL
        .iter()
        .map(|d| d.as_str())
        .collect::<Vec<&str>>();

    for (case, builder) in logged_refusals() {
        let recorder = Recorder::default();
        let seen = Arc::clone(&recorder.seen);
        let guard = tracing::subscriber::set_default(recorder);
        let _ = app(config())
            .oneshot(served(builder))
            .await
            .expect("responds");
        drop(guard);

        let records = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let refusal = records
            .iter()
            .find(|record| record.iter().any(|(name, _)| name == "detail"))
            .unwrap_or_else(|| panic!("case `{case}` emitted no record carrying a `detail` field"));

        let detail = refusal
            .iter()
            .find(|(name, _)| name == "detail")
            .map(|(_, value)| value.clone())
            .expect("just located");
        assert!(
            permitted.contains(&detail.as_str()),
            "case `{case}` emitted a `detail` that is not one of `HttpErrorDetail`'s reviewed \
             literals, so something is reaching that field from outside the closed set"
        );
        assert!(
            refusal.iter().any(|(name, _)| name == "code"),
            "case `{case}` recorded a detail with no `code` beside it"
        );
        assert!(
            refusal.iter().any(|(name, _)| name == "run_id"),
            "case `{case}` recorded a refusal with no `run_id`, which C-O3 requires"
        );
    }
}

/// The response body carries no caller-supplied value either.
///
/// The body is built from `public_message`, which never receives the detail — but "never receives"
/// is a property of today's code, and this is the assertion that would notice it changing.
#[tokio::test]
async fn a_refusal_body_carries_nothing_the_caller_supplied() {
    for (case, builder) in refusals() {
        let response = app(config())
            .oneshot(served(builder))
            .await
            .expect("responds");
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .expect("a bounded body");
        let text = String::from_utf8_lossy(&body);

        assert_ne!(status, StatusCode::OK, "case `{case}` was not refused");
        for leak in FORBIDDEN {
            assert!(
                !text.contains(leak),
                "case `{case}`: the response body echoed a planted value back to the caller"
            );
        }
        assert!(
            !text.is_empty(),
            "case `{case}` returned an empty body, so the assertion above is vacuous"
        );
    }
}
