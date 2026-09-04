//! OTLP/HTTP trace export behind the `otel` feature (ADR-0036 decision 6, FR-077, FR-078).
//!
//! # Bounded, non-blocking, on the runtime
//!
//! Ended spans go through Renvor's own [`BoundedProcessor`], not the SDK's batch processor: a
//! bounded channel (default 2048) fed by `try_send`, so a full queue **drops** the span, counts
//! it in `renvor_otel_spans_dropped_total`, and emits a closed-field event — it never blocks the
//! request that ended the span and never panics. A Tokio task drains the channel in bounded
//! batches under a bounded export timeout, and shutdown flushes what is queued within a bounded
//! grace. Because the drain runs on the Tokio runtime, the HTTP client needs no bridge to a
//! foreign thread — the SDK's own batch processor drives its exporter from a thread it owns,
//! which is the shape this module exists to avoid.
//!
//! # Shutdown is bounded, and a forced close is reported
//!
//! [`OtelHandle::shutdown`] closes the queue, exports what is queued, and **joins** the drain
//! task within the shutdown bound. When the bound passes first the drain is **aborted and
//! joined** — no detached task keeps an exporter alive after `shutdown` returns — the spans it
//! never exported are counted in `renvor_otel_spans_unexported_total`, one warn event carries
//! the count, and the call returns [`OtelShutdownError::FlushTimedOut`] with the same number.
//! The whole call takes at most the shutdown bound, plus the drain's next yield if it had to be
//! aborted (an export future yields on every I/O wait, so the join is prompt), plus the SDK
//! provider's own shutdown, whose only processor here answers immediately. FR-012 and C-L2
//! (`Drain`): the drain ends at its deadline and reports outstanding work; shutdown continues
//! rather than hanging; a forced close is reported, never swallowed. The trap avoided is the
//! one the first version of this module set: a timeout that only warned, left the task running,
//! counted nothing, and returned the provider's `Ok`.
//!
//! ## What `unexported` counts
//!
//! A span is **accepted** when `try_send` takes it into the queue, and its export **concludes**
//! when the export request carrying it returns — `ok`, `failed`, or `timed_out`, each counted
//! in `renvor_otel_exports_total`. The unexported number is the spans accepted whose export
//! had not concluded when the drain stopped: the spans still in the channel, the batch being
//! assembled, and the chunk whose export was cut off. A chunk whose export had already failed
//! or timed out is **not** in it — that loss was reported when it happened, and counting it
//! again at shutdown would report it twice. A span that ends after the drain has closed the
//! queue is refused at `try_send` and counted as a **drop**, so nothing is accepted into a
//! channel nobody will read again. The number is exact for every span that ended before
//! `shutdown` was called; a span ending in the same instant as the count is at most one
//! transient over-count, undone as soon as its refused send is unwound.
//!
//! Dropping an [`OtelHandle`] without calling `shutdown` is a forced close too: the drain is
//! aborted from `Drop` (`JoinHandle::abort` needs no runtime, so this cannot panic outside
//! one), its queued spans are counted the same way, and a warn event says the handle was
//! dropped. `Drop` cannot wait for the abort to land, so a chunk whose export completes in that
//! same instant may be counted although it was exported: the figure at a drop is an upper
//! bound, the figure at `shutdown` is exact. Prefer `shutdown`.
//!
//! # Redaction reaches the exported attributes
//!
//! [`RedactingExporter`] rewrites span and event attributes under the same [`Redaction`] rule as
//! the JSON logs before they reach the OTLP encoder, so both sinks redact (C-O6).
//!
//! # TLS by default
//!
//! The connector is `hyper-rustls` with the native root store and the `ring` provider — the one
//! provider this workspace installs (ADR-0033 decision 6). An `http://` endpoint is accepted only
//! for a loopback host; anything else is refused at Validate. Header values are `Secret`s exposed
//! once into the exporter and never rendered.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{WithExportConfig as _, WithHttpConfig as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter, SpanProcessor};
use renvor_config::Secret;
use renvor_core::observe::metrics::{Counter, MetricsError, Registry};
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::registry::LookupSpan;

use crate::redaction::{Redaction, bounded};

/// The tracing target every export event is emitted on.
pub const OTEL_EVENT_TARGET: &str = "renvor.otel";
/// The default queue: spans waiting for export.
pub const DEFAULT_QUEUE: usize = 2048;
/// The cap on the queue.
pub const MAX_QUEUE: usize = 65_536;
/// The default export batch.
pub const DEFAULT_BATCH: usize = 512;
/// The cap on the export batch.
pub const MAX_BATCH: usize = 4096;
/// The default bound on one export request.
pub const DEFAULT_EXPORT_TIMEOUT: Duration = Duration::from_secs(10);
/// The default pause between exports of a partial batch.
pub const DEFAULT_SCHEDULED_DELAY: Duration = Duration::from_secs(5);
/// The default bound on the shutdown flush.
pub const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
/// The cap on every duration bound here.
pub const MAX_DURATION: Duration = Duration::from_secs(60);
/// Drops are reported on the first and then every this many, so a flood is one line per batch.
pub const DROP_REPORT_INTERVAL: u64 = 1024;
/// The most bytes an endpoint URL may carry.
pub const MAX_ENDPOINT_BYTES: usize = 2048;

/// Why the exporter could not be configured or built. **Closed**; names keys, never values.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OtelError {
    /// The endpoint is not an `https://` URL, nor `http://` to a loopback host, or is malformed.
    #[error("the OTLP endpoint under `{key}` is refused: it must be https, or http to loopback")]
    EndpointRefused {
        /// The configuration key the endpoint came from.
        key: String,
    },
    /// A header name is not an HTTP token, or a header value holds a control character.
    #[error("the OTLP header under `{key}` is refused")]
    HeaderRefused {
        /// The configuration key the header came from.
        key: String,
    },
    /// A bound exceeded its cap or fell below its floor.
    #[error("an OTLP bound is out of range: {bound}")]
    BoundOutOfRange {
        /// Which bound.
        bound: &'static str,
    },
    /// The TLS connector could not be built from the native root store.
    #[error("the OTLP TLS connector could not be built from the native root store")]
    TlsUnavailable,
    /// The exporter could not be built.
    #[error("the OTLP exporter could not be built")]
    ExporterUnavailable,
    /// A metrics family of the same name is registered with another shape.
    #[error("the OTLP metrics could not be registered")]
    Metrics(#[from] MetricsError),
}

/// Why [`OtelHandle::shutdown`] did not flush cleanly. **Closed**; carries counts, never text.
///
/// FR-012 and C-L2 (`Drain`): a forced close is reported, never swallowed. A flush that misses
/// its bound is not a warning line — it is this error, carrying the number of spans it lost.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum OtelShutdownError {
    /// The drain did not finish within the shutdown bound. It was aborted and joined, and
    /// `unexported` spans were lost — see [the module documentation](self#what-unexported-counts)
    /// for exactly which spans that number holds.
    #[error(
        "the OTLP shutdown flush did not finish within its bound; {unexported} spans were not exported"
    )]
    FlushTimedOut {
        /// Spans accepted for export whose export had not concluded when the drain was aborted.
        unexported: u64,
    },
    /// The drain task ended abnormally — a panic inside the exporter — before the bound, leaving
    /// `unexported` spans behind.
    #[error("the OTLP drain task failed; {unexported} spans were not exported")]
    DrainFailed {
        /// Spans accepted for export whose export had not concluded when the drain ended.
        unexported: u64,
    },
    /// The SDK provider did not shut down cleanly after a clean flush. The SDK's text stays in
    /// the source; this `Display` renders none of it.
    #[error("the OTLP provider did not shut down cleanly")]
    Provider(#[source] OTelSdkError),
}

/// True for `localhost`, `127.0.0.0/8`, and `::1`, with or without brackets.
fn is_loopback(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// The host of `scheme://host[:port]/…`, without the port.
fn host_of(rest: &str) -> Option<&str> {
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if let Some(bracketed) = authority.strip_prefix('[') {
        return bracketed.split(']').next();
    }
    Some(
        authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host),
    )
}

/// Validates an endpoint (FR-077): `https://` anywhere, `http://` only to loopback.
fn validate_endpoint(endpoint: &str) -> bool {
    if endpoint.is_empty()
        || endpoint.len() > MAX_ENDPOINT_BYTES
        || endpoint.bytes().any(|byte| byte <= 0x20 || byte == 0x7f)
    {
        return false;
    }
    if let Some(rest) = endpoint.strip_prefix("https://") {
        return host_of(rest).is_some_and(|host| !host.is_empty());
    }
    if let Some(rest) = endpoint.strip_prefix("http://") {
        return host_of(rest).is_some_and(|host| !host.is_empty() && is_loopback(host));
    }
    false
}

fn is_token(text: &str) -> bool {
    !text.is_empty()
        && text.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

/// What the exporter is built from.
pub struct OtlpSettings {
    endpoint: String,
    endpoint_key: String,
    headers: Vec<(String, Secret<String>)>,
    service_name: String,
    queue: usize,
    batch: usize,
    export_timeout: Duration,
    scheduled_delay: Duration,
    shutdown_timeout: Duration,
    redaction: Redaction,
}

impl core::fmt::Debug for OtlpSettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Neither the endpoint (which may carry a tenant path) nor a header reaches `Debug`.
        f.debug_struct("OtlpSettings")
            .field("headers", &self.headers.len())
            .field("service_name", &self.service_name)
            .field("queue", &self.queue)
            .field("batch", &self.batch)
            .field("export_timeout", &self.export_timeout)
            .finish_non_exhaustive()
    }
}

impl OtlpSettings {
    /// Settings for `endpoint` (the full `…/v1/traces` URL), attributed to `key` for refusals.
    ///
    /// # Errors
    ///
    /// [`OtelError::EndpointRefused`].
    pub fn new(endpoint: &str, key: &str, service_name: &str) -> Result<Self, OtelError> {
        if !validate_endpoint(endpoint) {
            return Err(OtelError::EndpointRefused {
                key: key.to_owned(),
            });
        }
        let service_name = if service_name.is_empty() || service_name.len() > 128 {
            return Err(OtelError::BoundOutOfRange {
                bound: "service_name",
            });
        } else {
            service_name.to_owned()
        };
        Ok(Self {
            endpoint: endpoint.to_owned(),
            endpoint_key: key.to_owned(),
            headers: Vec::new(),
            service_name,
            queue: DEFAULT_QUEUE,
            batch: DEFAULT_BATCH,
            export_timeout: DEFAULT_EXPORT_TIMEOUT,
            scheduled_delay: DEFAULT_SCHEDULED_DELAY,
            shutdown_timeout: DEFAULT_SHUTDOWN_TIMEOUT,
            redaction: Redaction::new(),
        })
    }

    /// Adds a request header whose value is a secret, attributed to `key` for refusals.
    ///
    /// # Errors
    ///
    /// [`OtelError::HeaderRefused`] when the name is not a token or the value holds a control
    /// character.
    pub fn with_header(
        mut self,
        name: &str,
        value: Secret<String>,
        key: &str,
    ) -> Result<Self, OtelError> {
        let valid = is_token(name)
            && name.len() <= 128
            && !value.expose().is_empty()
            && value.expose().len() <= 4096
            && !value
                .expose()
                .bytes()
                .any(|byte| byte < 0x20 || byte == 0x7f);
        if !valid {
            return Err(OtelError::HeaderRefused {
                key: key.to_owned(),
            });
        }
        self.headers.push((name.to_ascii_lowercase(), value));
        Ok(self)
    }

    /// Replaces the queue bound (1 – 65 536) and the batch bound (1 – 4096, at most the queue).
    ///
    /// # Errors
    ///
    /// [`OtelError::BoundOutOfRange`].
    pub fn with_queue(mut self, queue: usize, batch: usize) -> Result<Self, OtelError> {
        if queue == 0 || queue > MAX_QUEUE {
            return Err(OtelError::BoundOutOfRange { bound: "queue" });
        }
        if batch == 0 || batch > MAX_BATCH || batch > queue {
            return Err(OtelError::BoundOutOfRange { bound: "batch" });
        }
        self.queue = queue;
        self.batch = batch;
        Ok(self)
    }

    /// Replaces the export, scheduled-delay, and shutdown bounds (each 1 ms – 60 s).
    ///
    /// # Errors
    ///
    /// [`OtelError::BoundOutOfRange`].
    pub fn with_timeouts(
        mut self,
        export: Duration,
        scheduled_delay: Duration,
        shutdown: Duration,
    ) -> Result<Self, OtelError> {
        for (bound, value) in [
            ("export_timeout", export),
            ("scheduled_delay", scheduled_delay),
            ("shutdown_timeout", shutdown),
        ] {
            if value < Duration::from_millis(1) || value > MAX_DURATION {
                return Err(OtelError::BoundOutOfRange { bound });
            }
        }
        self.export_timeout = export;
        self.scheduled_delay = scheduled_delay;
        self.shutdown_timeout = shutdown;
        Ok(self)
    }

    /// Adds field names to the redaction applied to exported attributes.
    #[must_use]
    pub fn with_redaction(mut self, redaction: Redaction) -> Self {
        self.redaction = redaction;
        self
    }

    /// The configuration key the endpoint is attributed to.
    #[must_use]
    pub fn endpoint_key(&self) -> &str {
        &self.endpoint_key
    }
}

/// The export counters: spans dropped at a full queue, spans unexported at a forced close, and
/// exports by outcome.
#[derive(Clone, Debug)]
pub struct OtelMetrics {
    dropped: Counter,
    unexported: Counter,
    exports: Counter,
}

impl OtelMetrics {
    /// Registers the three families, or returns the existing ones.
    ///
    /// # Errors
    ///
    /// [`MetricsError`] when a family of the same name has another shape.
    pub fn register(registry: &Registry) -> Result<Self, MetricsError> {
        Ok(Self {
            dropped: registry.counter(
                "renvor_otel_spans_dropped_total",
                "Spans dropped because the export queue was full.",
                &[],
            )?,
            unexported: registry.counter(
                "renvor_otel_spans_unexported_total",
                "Spans accepted for export that had not been exported when the drain was stopped.",
                &[],
            )?,
            exports: registry.counter(
                "renvor_otel_exports_total",
                "Export requests by closed outcome.",
                &["outcome"],
            )?,
        })
    }
}

/// A span exporter that redacts attributes before delegating (C-O6).
#[derive(Debug)]
pub struct RedactingExporter<E> {
    inner: E,
    rule: Redaction,
}

impl<E: SpanExporter> RedactingExporter<E> {
    /// Wraps `inner` under `rule`.
    pub const fn new(inner: E, rule: Redaction) -> Self {
        Self { inner, rule }
    }

    fn redact_attributes(rule: &Redaction, attributes: &mut [KeyValue]) {
        for attribute in attributes.iter_mut() {
            let key = attribute.key.as_str();
            if rule.applies_to(key) {
                attribute.value = opentelemetry::Value::String(crate::redaction::REDACTED.into());
            } else if let opentelemetry::Value::String(text) = &attribute.value {
                let text = text.as_str();
                if text.len() > crate::redaction::MAX_VALUE_BYTES {
                    attribute.value = opentelemetry::Value::String(bounded(text).into());
                }
            }
        }
    }

    /// Applies the rule to one span's attributes and event attributes.
    pub fn redact(rule: &Redaction, span: &mut SpanData) {
        Self::redact_attributes(rule, &mut span.attributes);
        for event in &mut span.events.events {
            Self::redact_attributes(rule, &mut event.attributes);
        }
    }
}

impl<E: SpanExporter> SpanExporter for RedactingExporter<E> {
    fn export(&self, mut batch: Vec<SpanData>) -> impl Future<Output = OTelSdkResult> + Send {
        for span in &mut batch {
            Self::redact(&self.rule, span);
        }
        self.inner.export(batch)
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}

/// What the drain task shares with the processor.
struct Shared {
    dropped: AtomicU64,
    /// Spans accepted into the queue whose export has not concluded: the module documentation's
    /// "unexported" number, read at a forced close.
    queued: AtomicU64,
    metrics: OtelMetrics,
}

/// Renvor's bounded span processor: `try_send` into a bounded channel, a Tokio drain task, and
/// the count of accepted spans whose export has not concluded (the module's `unexported`).
pub struct BoundedProcessor {
    sender: tokio::sync::mpsc::Sender<SpanData>,
    shared: Arc<Shared>,
}

impl core::fmt::Debug for BoundedProcessor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("BoundedProcessor")
            .field("dropped", &self.shared.dropped.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

/// The drain task's handle, for a bounded shutdown flush; `None` once the task has been taken
/// for joining, so `Drop` finds nothing to abort after `shutdown`.
struct Drain {
    stop: Arc<tokio::sync::Notify>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl BoundedProcessor {
    /// Starts the drain task on the current runtime and returns the processor with its drain.
    ///
    /// Must be called inside a Tokio runtime.
    fn start<E>(
        exporter: E,
        queue: usize,
        batch: usize,
        export_timeout: Duration,
        scheduled_delay: Duration,
        metrics: OtelMetrics,
    ) -> (Self, Drain)
    where
        E: SpanExporter + Send + Sync + 'static,
    {
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<SpanData>(queue);
        let shared = Arc::new(Shared {
            dropped: AtomicU64::new(0),
            queued: AtomicU64::new(0),
            metrics,
        });
        let stop = Arc::new(tokio::sync::Notify::new());
        let stop_for_task = Arc::clone(&stop);
        let shared_for_task = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            let mut pending: Vec<SpanData> = Vec::with_capacity(batch);
            let mut stopping = false;
            loop {
                if !stopping {
                    // Fill the batch, or wait out the scheduled delay, or notice the stop.
                    let deadline = tokio::time::sleep(scheduled_delay);
                    tokio::pin!(deadline);
                    loop {
                        if pending.len() >= batch {
                            break;
                        }
                        tokio::select! {
                            received = receiver.recv() => match received {
                                Some(span) => pending.push(span),
                                None => { stopping = true; break; }
                            },
                            () = stop_for_task.notified() => { stopping = true; break; }
                            () = &mut deadline => break,
                        }
                    }
                }
                if stopping {
                    // Close the queue FIRST: a span ending from here on is refused at `try_send`
                    // and counted as a drop, instead of being accepted into a channel this task
                    // will never read again. Everything sent before the close is still received
                    // by the sweep, which is bounded by the channel's own capacity.
                    receiver.close();
                    while let Ok(span) = receiver.try_recv() {
                        pending.push(span);
                    }
                }
                if !pending.is_empty() {
                    for chunk in pending.chunks(batch) {
                        let outcome =
                            tokio::time::timeout(export_timeout, exporter.export(chunk.to_vec()))
                                .await;
                        // The export concluded, whatever its outcome: these spans leave the
                        // unexported count. A failure or a timeout is counted in
                        // `exports{outcome}` and reported below, once — not again at shutdown.
                        shared_for_task
                            .queued
                            .fetch_sub(chunk.len() as u64, Ordering::AcqRel);
                        let label = match &outcome {
                            Ok(Ok(())) => "ok",
                            Ok(Err(_)) => "failed",
                            Err(_) => "timed_out",
                        };
                        shared_for_task
                            .metrics
                            .exports
                            .increment(&[("outcome", label)], 1);
                        if label != "ok" {
                            tracing::warn!(
                                target: OTEL_EVENT_TARGET,
                                spans = chunk.len(),
                                outcome = label,
                                "an OTLP export did not succeed"
                            );
                        }
                    }
                    pending.clear();
                }
                if stopping {
                    break;
                }
            }
        });
        (
            Self { sender, shared },
            Drain {
                stop,
                task: Mutex::new(Some(task)),
            },
        )
    }

    /// Spans dropped at a full queue since start.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

impl SpanProcessor for BoundedProcessor {
    fn on_start(&self, _span: &mut opentelemetry_sdk::trace::Span, _cx: &opentelemetry::Context) {}

    fn on_end(&self, span: SpanData) {
        // Counted BEFORE it is offered, so the count never runs below the truth: an accepted
        // span is in the count before it is in the channel, and a refused one is taken back out.
        // Counting after `try_send` would let the drain conclude the span's export before its
        // increment landed, and a `u64` that dips below zero wraps.
        self.shared.queued.fetch_add(1, Ordering::AcqRel);
        if let Err(refused) = self.sender.try_send(span) {
            self.shared.queued.fetch_sub(1, Ordering::AcqRel);
            // `closed` is the queue after the drain stopped — or a drain that is gone.
            let reason = match refused {
                tokio::sync::mpsc::error::TrySendError::Full(_) => "full",
                tokio::sync::mpsc::error::TrySendError::Closed(_) => "closed",
            };
            let dropped = self.shared.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            self.shared.metrics.dropped.increment(&[], 1);
            if dropped == 1 || dropped.is_multiple_of(DROP_REPORT_INTERVAL) {
                tracing::warn!(
                    target: OTEL_EVENT_TARGET,
                    dropped_total = dropped,
                    reason,
                    "the OTLP export queue refused a span; spans are being dropped"
                );
            }
        }
    }

    fn force_flush(&self) -> OTelSdkResult {
        // The drain task exports on its own schedule; a synchronous flush from an arbitrary
        // thread cannot wait on it without blocking, so this reports success and the handle's
        // async `shutdown` is the flush that waits.
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        // Flushed by `OtelHandle::shutdown`, which runs before the provider is shut down.
        Ok(())
    }
}

/// How a drain was force-closed: the closed label on the one event, and the error returned.
#[derive(Clone, Copy, Debug)]
enum ForcedClose {
    /// The shutdown bound passed; the drain was aborted and joined.
    TimedOut,
    /// The drain task ended abnormally within the bound.
    DrainFailed,
}

impl ForcedClose {
    const fn label(self) -> &'static str {
        match self {
            Self::TimedOut => "timed_out",
            Self::DrainFailed => "drain_failed",
        }
    }

    const fn into_error(self, unexported: u64) -> OtelShutdownError {
        match self {
            Self::TimedOut => OtelShutdownError::FlushTimedOut { unexported },
            Self::DrainFailed => OtelShutdownError::DrainFailed { unexported },
        }
    }
}

/// The running exporter: shut it down to flush what is queued within a bound. Dropping it
/// without `shutdown` aborts the drain and counts what it strands (module documentation).
pub struct OtelHandle {
    provider: SdkTracerProvider,
    drain: Drain,
    shutdown_timeout: Duration,
    shared: Arc<Shared>,
}

impl core::fmt::Debug for OtelHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OtelHandle")
            .field("dropped", &self.dropped())
            .finish_non_exhaustive()
    }
}

impl OtelHandle {
    /// Spans dropped at a full queue since start.
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.shared.dropped.load(Ordering::Relaxed)
    }

    /// Closes the queue, exports what is queued within the shutdown bound, joins the drain, and
    /// shuts the provider down.
    ///
    /// Bounded: at most the shutdown bound, plus the drain's next yield if it had to be aborted,
    /// plus the provider's own shutdown (its only processor answers immediately). When the bound
    /// passes first the drain is aborted **and joined**, the spans it never exported are counted
    /// in `renvor_otel_spans_unexported_total` and one warn event, and
    /// [`OtelShutdownError::FlushTimedOut`] carries the same count — FR-012 and C-L2: a forced
    /// close is reported, never swallowed. A clean flush returns `Ok` with nothing unexported.
    /// The drain task never outlives this call, on either path.
    ///
    /// # Errors
    ///
    /// [`OtelShutdownError`]: the flush missed its bound, the drain task failed, or the provider
    /// did not shut down cleanly after a clean flush.
    pub async fn shutdown(self) -> Result<(), OtelShutdownError> {
        self.drain.stop.notify_one();
        // Taken out of the handle, so the `Drop` that runs when `self` goes out of scope below
        // finds nothing to abort a second time.
        let task = self
            .drain
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let forced = match task {
            None => None,
            Some(mut task) => match tokio::time::timeout(self.shutdown_timeout, &mut task).await {
                Ok(Ok(())) => None,
                Ok(Err(_ended_abnormally)) => Some(ForcedClose::DrainFailed),
                Err(_elapsed) => {
                    // Abort, THEN join: the aborted task resolves at its next yield and nothing
                    // of it survives this call. The join's own result is the cancellation just
                    // requested — or, in the same instant, a finish or a panic — and changes
                    // nothing reported here: the bound passed, the task is finished, and the
                    // count is read after it.
                    task.abort();
                    let _cancelled = (&mut task).await;
                    Some(ForcedClose::TimedOut)
                }
            },
        };
        let unexported = self.shared.queued.load(Ordering::SeqCst);
        if let Some(forced) = forced {
            self.shared.metrics.unexported.increment(&[], unexported);
            tracing::warn!(
                target: OTEL_EVENT_TARGET,
                spans_unexported = unexported,
                bound_ms = self.shutdown_timeout.as_millis(),
                cause = forced.label(),
                "the OTLP drain was force-closed; spans accepted for export were not exported"
            );
        }
        // The processor's own shutdown is a no-op (flushed or aborted above); this releases the
        // provider. Bounded: nothing here waits on the drain.
        let provider = self.provider.shutdown();
        match (forced, provider) {
            (None, Ok(())) => Ok(()),
            (None, Err(error)) => Err(OtelShutdownError::Provider(error)),
            (Some(forced), provider) => {
                if provider.is_err() {
                    // Reported here because the forced close is the error returned; the SDK's
                    // text is not rendered (C-O16).
                    tracing::warn!(
                        target: OTEL_EVENT_TARGET,
                        "the OTLP provider did not shut down cleanly after a forced close"
                    );
                }
                Err(forced.into_error(unexported))
            }
        }
    }
}

impl Drop for OtelHandle {
    /// A dropped handle is a forced close: the drain is aborted so no detached task survives
    /// the handle, and the spans it strands are counted — an upper bound, because `Drop` cannot
    /// wait for the abort to land (module documentation). `JoinHandle::abort` needs no runtime,
    /// so this cannot panic outside one.
    fn drop(&mut self) {
        let task = self
            .drain
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(task) = task {
            task.abort();
            let unexported = self.shared.queued.load(Ordering::SeqCst);
            self.shared.metrics.unexported.increment(&[], unexported);
            tracing::warn!(
                target: OTEL_EVENT_TARGET,
                spans_unexported = unexported,
                cause = "dropped",
                "the OTLP handle was dropped without shutdown; the drain was aborted"
            );
        }
    }
}

/// Builds the TLS connector: native roots, the `ring` provider, HTTP/1.1.
fn connector() -> Result<
    hyper_rustls::HttpsConnector<hyper_util::client::legacy::connect::HttpConnector>,
    OtelError,
> {
    hyper_rustls::HttpsConnectorBuilder::new()
        .with_provider_and_native_roots(rustls::crypto::ring::default_provider())
        .map_err(|_| OtelError::TlsUnavailable)
        .map(|builder| builder.https_or_http().enable_http1().build())
}

/// Builds the tracing layer and the handle that shuts the export down.
///
/// Must be called inside a Tokio runtime: the drain task is spawned here. The layer is returned,
/// never installed (C-O7); the caller composes it into the subscriber it installs.
///
/// # Errors
///
/// [`OtelError`] for a refused setting, an unavailable root store, or an exporter that could
/// not be built.
pub fn layer<S>(
    settings: &OtlpSettings,
    registry: &Registry,
) -> Result<
    (
        OpenTelemetryLayer<S, opentelemetry_sdk::trace::Tracer>,
        OtelHandle,
    ),
    OtelError,
>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    let metrics = OtelMetrics::register(registry)?;
    let client =
        opentelemetry_http::hyper::HyperClient::new(connector()?, settings.export_timeout, None);
    let headers: HashMap<String, String> = settings
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), value.expose().clone()))
        .collect();
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_http_client(client)
        .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
        .with_endpoint(settings.endpoint.clone())
        .with_timeout(settings.export_timeout)
        .with_headers(headers)
        .build()
        .map_err(|_| OtelError::ExporterUnavailable)?;
    // The resource reaches the wire through the EXPORTER: the SDK hands it to each span
    // processor's `set_resource`, and Renvor's processor owns the exporter inside its drain task
    // by then. Set on the exporter here, before the task starts, and on the provider for the SDK.
    let resource = Resource::builder()
        .with_service_name(settings.service_name.clone())
        .build();
    let mut exporter = RedactingExporter::new(exporter, settings.redaction.clone());
    exporter.set_resource(&resource);
    let (processor, drain) = BoundedProcessor::start(
        exporter,
        settings.queue,
        settings.batch,
        settings.export_timeout,
        settings.scheduled_delay,
        metrics,
    );
    let shared = Arc::clone(&processor.shared);
    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_span_processor(processor)
        .build();
    let tracer = provider.tracer("renvor");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    Ok((
        layer,
        OtelHandle {
            provider,
            drain,
            shutdown_timeout: settings.shutdown_timeout,
            shared,
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::Duration;

    use opentelemetry::KeyValue;
    use opentelemetry::trace::{
        SpanContext, SpanId, SpanKind, Status, TraceFlags, TraceId, TraceState,
    };
    use opentelemetry_sdk::error::OTelSdkResult;
    use opentelemetry_sdk::trace::{
        SdkTracerProvider, SpanData, SpanEvents, SpanExporter, SpanLinks, SpanProcessor as _,
    };
    use renvor_config::Secret;
    use renvor_core::observe::metrics::{Registry, SeriesValue};

    use super::{
        BoundedProcessor, Drain, OtelError, OtelHandle, OtelMetrics, OtelShutdownError,
        OtlpSettings, RedactingExporter, validate_endpoint,
    };
    use crate::redaction::{MAX_VALUE_BYTES, REDACTED, Redaction};

    fn span(name: &'static str, attributes: Vec<KeyValue>) -> SpanData {
        SpanData {
            span_context: SpanContext::new(
                TraceId::from_bytes([1; 16]),
                SpanId::from_bytes([1; 8]),
                TraceFlags::SAMPLED,
                false,
                TraceState::default(),
            ),
            parent_span_id: SpanId::INVALID,
            parent_span_is_remote: false,
            span_kind: SpanKind::Internal,
            name: name.into(),
            start_time: std::time::SystemTime::UNIX_EPOCH,
            end_time: std::time::SystemTime::UNIX_EPOCH,
            attributes,
            dropped_attributes_count: 0,
            events: SpanEvents::default(),
            links: SpanLinks::default(),
            status: Status::Unset,
            instrumentation_scope: Default::default(),
        }
    }

    /// An exporter that records batches and can be told to take a while.
    #[derive(Debug, Clone, Default)]
    struct Recording {
        batches: Arc<Mutex<Vec<Vec<SpanData>>>>,
        delay: Option<Duration>,
    }

    impl Recording {
        fn spans(&self) -> Vec<SpanData> {
            self.batches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .flatten()
                .cloned()
                .collect()
        }
    }

    impl SpanExporter for Recording {
        async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            self.batches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(batch);
            Ok(())
        }
    }

    /// An exporter whose export never completes: the detached-drain scenario of finding 14.
    #[derive(Debug)]
    struct Pending;

    impl SpanExporter for Pending {
        async fn export(&self, _batch: Vec<SpanData>) -> OTelSdkResult {
            std::future::pending().await
        }
    }

    /// An exporter that panics, so the drain task ends abnormally.
    #[derive(Debug)]
    struct Panicking;

    impl SpanExporter for Panicking {
        async fn export(&self, _batch: Vec<SpanData>) -> OTelSdkResult {
            panic!("this exporter panics on purpose")
        }
    }

    /// An exporter that holds its export open until released, and says when it is inside one.
    #[derive(Debug, Clone, Default)]
    struct Gated {
        inside: Arc<tokio::sync::Notify>,
        release: Arc<tokio::sync::Notify>,
        batches: Arc<Mutex<Vec<Vec<SpanData>>>>,
    }

    impl SpanExporter for Gated {
        async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
            self.inside.notify_one();
            self.release.notified().await;
            self.batches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(batch);
            Ok(())
        }
    }

    /// The handle over a started processor, assembled as `layer` assembles it, with a probe on
    /// the drain task so a test can prove the task is finished rather than assume it.
    fn handle_over(
        processor: BoundedProcessor,
        drain: Drain,
        shutdown_timeout: Duration,
    ) -> (OtelHandle, tokio::task::AbortHandle) {
        let probe = drain
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .map(tokio::task::JoinHandle::abort_handle)
            .unwrap();
        let shared = Arc::clone(&processor.shared);
        let provider = SdkTracerProvider::builder()
            .with_span_processor(processor)
            .build();
        (
            OtelHandle {
                provider,
                drain,
                shutdown_timeout,
                shared,
            },
            probe,
        )
    }

    fn total(registry: &Registry, name: &str) -> f64 {
        registry
            .snapshot()
            .families
            .iter()
            .filter(|family| family.name == name)
            .flat_map(|family| family.series.iter())
            .map(|series| match series.value {
                SeriesValue::Scalar(value) => value,
                SeriesValue::Histogram { .. } => 0.0,
            })
            .sum()
    }

    #[test]
    fn endpoints_are_https_or_loopback_http() {
        for ok in [
            "https://otel.example.test/v1/traces",
            "https://otel.example.test:4318/v1/traces",
            "http://127.0.0.1:4318/v1/traces",
            "http://localhost:4318/v1/traces",
            "http://[::1]:4318/v1/traces",
        ] {
            assert!(validate_endpoint(ok), "an accepted endpoint was refused");
        }
        for (index, bad) in [
            "",
            "otel.example.test/v1/traces",
            "http://otel.example.test/v1/traces",
            "http://10.0.0.5:4318/v1/traces",
            "ftp://127.0.0.1/v1/traces",
            "https://",
            "https://otel.example.test/v1/traces\n",
            "http://127.0.0.1@otel.example.test/v1/traces",
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                !validate_endpoint(bad),
                "rejected endpoint case {index} was accepted"
            );
        }
        let refused =
            OtlpSettings::new("http://otel.example.test/v1/traces", "otel.endpoint", "svc")
                .unwrap_err();
        assert_eq!(
            refused,
            OtelError::EndpointRefused {
                key: "otel.endpoint".to_owned()
            }
        );
        assert!(!refused.to_string().contains("otel.example.test"));
    }

    #[test]
    fn headers_and_bounds_are_validated_and_nothing_secret_is_rendered() {
        let settings =
            OtlpSettings::new("https://otel.example.test/v1/traces", "k", "svc").unwrap();
        let settings = settings
            .with_header(
                "authorization",
                Secret::new("otel.header", "Bearer hunter2CanaryDoNotLeak".to_owned()),
                "otel.header",
            )
            .unwrap();
        assert!(!format!("{settings:?}").contains("hunter2"));
        assert!(!format!("{settings:?}").contains("otel.example.test"));
        let bad_name = settings
            .with_header("bad name", Secret::new("k", "v".to_owned()), "otel.header")
            .unwrap_err();
        assert_eq!(
            bad_name,
            OtelError::HeaderRefused {
                key: "otel.header".to_owned()
            }
        );
        let base = || OtlpSettings::new("https://otel.example.test/v1/traces", "k", "svc").unwrap();
        assert!(
            base()
                .with_header("x", Secret::new("k", "a\r\nb".to_owned()), "k")
                .is_err()
        );
        assert!(base().with_queue(0, 1).is_err());
        assert!(
            base().with_queue(10, 11).is_err(),
            "a batch larger than the queue"
        );
        assert!(
            base()
                .with_queue(super::MAX_QUEUE, super::MAX_BATCH)
                .is_ok()
        );
        assert!(
            base()
                .with_timeouts(
                    Duration::from_secs(61),
                    Duration::from_secs(1),
                    Duration::from_secs(1)
                )
                .is_err()
        );
        assert!(OtlpSettings::new("https://h/v1/traces", "k", "").is_err());
    }

    #[tokio::test]
    async fn the_redacting_exporter_rewrites_span_and_event_attributes() {
        let inner = Recording::default();
        let exporter = RedactingExporter::new(inner.clone(), Redaction::new().with_field("ssn"));
        let mut one = span(
            "s",
            vec![
                KeyValue::new("password", "hunter2CanaryDoNotLeak-attr"),
                KeyValue::new("http.route", "/x"),
                KeyValue::new("customer.ssn", "hunter2CanaryDoNotLeak-configured"),
                KeyValue::new("blob", "b".repeat(MAX_VALUE_BYTES + 10)),
            ],
        );
        one.events.events.push(opentelemetry::trace::Event::new(
            "e",
            std::time::SystemTime::UNIX_EPOCH,
            vec![KeyValue::new("token", "hunter2CanaryDoNotLeak-event")],
            0,
        ));
        exporter.export(vec![one]).await.unwrap();
        let exported = inner.spans();
        assert_eq!(exported.len(), 1);
        let rendered = format!("{exported:?}");
        assert!(
            !rendered.contains("hunter2"),
            "a canary reached the exporter"
        );
        let value = |key: &str| {
            exported[0]
                .attributes
                .iter()
                .find(|attribute| attribute.key.as_str() == key)
                .map(|attribute| attribute.value.to_string())
                .unwrap_or_default()
        };
        assert_eq!(value("password"), REDACTED);
        assert_eq!(value("customer.ssn"), REDACTED);
        assert_eq!(value("http.route"), "/x");
        assert!(value("blob").contains("…[truncated"));
        assert_eq!(
            exported[0].events.events[0].attributes[0].value.to_string(),
            REDACTED
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_full_queue_drops_and_counts_and_never_blocks() {
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let slow = Recording {
            batches: Arc::default(),
            delay: Some(Duration::from_millis(300)),
        };
        // Queue 4, batch 2, a slow exporter: the drain is busy, so the fifth and later ends drop.
        let (processor, drain) = BoundedProcessor::start(
            slow.clone(),
            4,
            2,
            Duration::from_secs(5),
            Duration::from_millis(50),
            metrics,
        );
        let started = std::time::Instant::now();
        for index in 0..64 {
            processor.on_end(span("s", vec![KeyValue::new("index", index)]));
        }
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "ending spans blocked on the exporter"
        );
        assert!(
            processor.dropped() > 0,
            "nothing was dropped at a full queue"
        );
        assert_eq!(
            total(&registry, "renvor_otel_spans_dropped_total"),
            processor.dropped() as f64
        );
        drain.stop.notify_one();
        let task = drain
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap();
        let exported = slow.spans().len() as u64;
        assert!(exported >= 1, "nothing was exported");
        assert_eq!(
            exported + processor.dropped(),
            64,
            "every span was exported or counted"
        );
        assert!(total(&registry, "renvor_otel_exports_total") >= 1.0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_flushes_what_is_queued_and_a_slow_export_times_out() {
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let fast = Recording::default();
        let (processor, drain) = BoundedProcessor::start(
            fast.clone(),
            64,
            8,
            Duration::from_secs(5),
            Duration::from_secs(30),
            metrics.clone(),
        );
        for index in 0..5 {
            processor.on_end(span("s", vec![KeyValue::new("index", index)]));
        }
        // Nothing exported yet: the batch is not full and the delay is long.
        assert!(fast.spans().is_empty());
        drain.stop.notify_one();
        let task = drain
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fast.spans().len(), 5, "the shutdown flush lost spans");
        assert_eq!(processor.dropped(), 0);

        // An exporter slower than the export bound is a counted timeout, not a hang.
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let stuck = Recording {
            batches: Arc::default(),
            delay: Some(Duration::from_secs(5)),
        };
        let (processor, drain) = BoundedProcessor::start(
            stuck,
            8,
            8,
            Duration::from_millis(100),
            Duration::from_millis(10),
            metrics,
        );
        processor.on_end(span("s", Vec::new()));
        tokio::time::sleep(Duration::from_millis(400)).await;
        drain.stop.notify_one();
        let task = drain
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        let snapshot = registry.snapshot();
        let timed_out = snapshot
            .families
            .iter()
            .find(|family| family.name == "renvor_otel_exports_total")
            .and_then(|family| {
                family
                    .series
                    .iter()
                    .find(|series| series.labels.contains(&("outcome", "timed_out".to_owned())))
            })
            .map(|series| match series.value {
                SeriesValue::Scalar(value) => value,
                SeriesValue::Histogram { .. } => 0.0,
            })
            .unwrap_or(0.0);
        assert!(
            timed_out >= 1.0,
            "the slow export was not counted as timed out"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_timed_out_flush_aborts_the_drain_and_reports_every_unexported_span() {
        // Finding 14. Eight spans, batch four, an export that never completes: four spans are in
        // the cut-off export and four are still in the channel. All eight are unexported, the
        // drain must be aborted AND joined, and the bound must hold.
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let (processor, drain) = BoundedProcessor::start(
            Pending,
            16,
            4,
            Duration::from_secs(60),
            Duration::from_secs(30),
            metrics,
        );
        for index in 0..8 {
            processor.on_end(span("s", vec![KeyValue::new("index", index)]));
        }
        let bound = Duration::from_millis(200);
        let (handle, probe) = handle_over(processor, drain, bound);
        let outcome = tokio::time::timeout(bound + Duration::from_millis(300), handle.shutdown())
            .await
            .expect("shutdown did not return within its bound plus slack");
        assert!(
            matches!(
                outcome,
                Err(OtelShutdownError::FlushTimedOut { unexported: 8 })
            ),
            "a flush past its bound must return FlushTimedOut carrying all eight spans"
        );
        assert!(probe.is_finished(), "the drain task outlived shutdown");
        assert_eq!(
            total(&registry, "renvor_otel_spans_unexported_total"),
            8.0,
            "the unexported counter must carry all eight spans"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_clean_flush_returns_ok_with_nothing_unexported() {
        // The positive control for finding 14: an exporter that answers, three queued spans,
        // shutdown flushes them, joins the drain, and the unexported counter stays at zero.
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let fast = Recording::default();
        let (processor, drain) = BoundedProcessor::start(
            fast.clone(),
            16,
            8,
            Duration::from_secs(5),
            Duration::from_secs(30),
            metrics,
        );
        for index in 0..3 {
            processor.on_end(span("s", vec![KeyValue::new("index", index)]));
        }
        let (handle, probe) = handle_over(processor, drain, Duration::from_secs(5));
        let outcome = tokio::time::timeout(Duration::from_secs(10), handle.shutdown())
            .await
            .expect("shutdown did not return within its bound plus slack");
        assert!(outcome.is_ok(), "a clean flush must return Ok");
        assert!(probe.is_finished(), "the drain task outlived shutdown");
        assert_eq!(fast.spans().len(), 3, "the clean flush lost spans");
        assert_eq!(
            total(&registry, "renvor_otel_spans_unexported_total"),
            0.0,
            "a clean flush must leave the unexported counter at zero"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_drain_that_panics_is_reported_with_every_span_it_left_behind() {
        // A drain that ends abnormally within the bound is not a clean flush either: the spans
        // it never exported are counted and the error names the cause.
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let (processor, drain) = BoundedProcessor::start(
            Panicking,
            16,
            8,
            Duration::from_secs(5),
            Duration::from_secs(30),
            metrics,
        );
        for index in 0..3 {
            processor.on_end(span("s", vec![KeyValue::new("index", index)]));
        }
        let (handle, probe) = handle_over(processor, drain, Duration::from_secs(5));
        let outcome = tokio::time::timeout(Duration::from_secs(10), handle.shutdown())
            .await
            .expect("shutdown did not return within its bound plus slack");
        assert!(
            matches!(
                outcome,
                Err(OtelShutdownError::DrainFailed { unexported: 3 })
            ),
            "a panicked drain must return DrainFailed carrying the three spans it left behind"
        );
        assert!(probe.is_finished(), "the drain task outlived shutdown");
        assert_eq!(
            total(&registry, "renvor_otel_spans_unexported_total"),
            3.0,
            "the unexported counter must carry the three spans"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_the_handle_without_shutdown_aborts_the_drain_and_counts_its_spans() {
        // A dropped handle is a forced close too: no detached task may survive it, and the spans
        // it strands are counted.
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let (processor, drain) = BoundedProcessor::start(
            Pending,
            16,
            4,
            Duration::from_secs(60),
            Duration::from_secs(30),
            metrics,
        );
        for index in 0..2 {
            processor.on_end(span("s", vec![KeyValue::new("index", index)]));
        }
        let (handle, probe) = handle_over(processor, drain, Duration::from_secs(5));
        drop(handle);
        // The abort takes effect at the task's next poll; wait for that, bounded.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !probe.is_finished() && std::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            probe.is_finished(),
            "the drain task survived a dropped handle"
        );
        assert_eq!(
            total(&registry, "renvor_otel_spans_unexported_total"),
            2.0,
            "the spans stranded by the drop were not counted"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_span_ending_after_the_stop_sweep_is_refused_and_counted_rather_than_lost() {
        // Between the drain's final sweep and its exit the final export is in flight. A span
        // ending then must be refused at a closed queue and counted as dropped — not accepted
        // into a channel nobody will read again and lost uncounted.
        let registry = Registry::new();
        let metrics = OtelMetrics::register(&registry).unwrap();
        let gated = Gated::default();
        let (processor, drain) = BoundedProcessor::start(
            gated.clone(),
            16,
            8,
            Duration::from_secs(5),
            Duration::from_secs(30),
            metrics,
        );
        processor.on_end(span("first", Vec::new()));
        drain.stop.notify_one();
        tokio::time::timeout(Duration::from_secs(5), gated.inside.notified())
            .await
            .expect("the final export did not start");
        processor.on_end(span("late", Vec::new()));
        assert_eq!(
            processor.dropped(),
            1,
            "a span ending after the stop sweep must be refused and counted as dropped"
        );
        gated.release.notify_one();
        let task = drain
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap();
        let exported: usize = gated
            .batches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(Vec::len)
            .sum();
        assert_eq!(exported, 1, "the swept span was not exported");
        assert_eq!(
            processor.shared.queued.load(Ordering::SeqCst),
            0,
            "a refused span must not linger in the unexported count"
        );
    }

    #[test]
    fn renvor_names_equal_the_semantic_conventions() {
        // FR-079, T038: the literals the kernel records are the published constants. The HTTP,
        // URL, and database names are stable in semconv 0.32.1 and compared to the crate; the
        // messaging names are still `semconv_experimental` there — not enabled, because an
        // experimental feature is not a convention — so they are pinned to the published spelling.
        use opentelemetry_semantic_conventions::attribute as sc;
        use renvor_core::observe::semconv as ours;
        assert_eq!(ours::HTTP_REQUEST_METHOD, sc::HTTP_REQUEST_METHOD);
        assert_eq!(ours::HTTP_ROUTE, sc::HTTP_ROUTE);
        assert_eq!(
            ours::HTTP_RESPONSE_STATUS_CODE,
            sc::HTTP_RESPONSE_STATUS_CODE
        );
        assert_eq!(ours::URL_PATH, sc::URL_PATH);
        assert_eq!(ours::DB_SYSTEM_NAME, sc::DB_SYSTEM_NAME);
        assert_eq!(ours::MESSAGING_SYSTEM, "messaging.system");
        assert_eq!(
            ours::MESSAGING_DESTINATION_NAME,
            "messaging.destination.name"
        );
        assert_eq!(ours::MESSAGING_OPERATION_TYPE, "messaging.operation.type");
    }
}
