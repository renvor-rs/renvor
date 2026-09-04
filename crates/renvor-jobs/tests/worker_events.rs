//! FR-031 and FR-038: what the worker emits, asserted from a subscriber's point of view.
//!
//! Written with a hand-rolled recording subscriber rather than `tracing-subscriber`, for the
//! reason the kernel's span test gives: this crate carries no subscriber stack, and forty lines
//! is the price of asserting what a subscriber **actually receives**.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use renvor_core::cancel::CancelScope;
use renvor_core::clock::{Clock as _, FixedClock};
use renvor_core::lifecycle::drain::WorkGate;
use renvor_core::observe::metrics::Registry;
use renvor_core::observe::{EntropySource, FixedEntropy, TraceContext};
use renvor_core::retry::{Jitter, RetryPolicy};
use renvor_jobs::{
    HandlerError, HandlerFuture, JOB_SPAN_NAME, JOBS_EVENT_TARGET, Job, JobBounds, JobHandler,
    JobKind, JobPayload, JobState, JobStore, MemoryJobStore, NewJob, QueueName, Worker,
    WorkerConfig,
};

/// One record: its name or target, and the fields it carried, rendered.
type Record = (String, Vec<(String, String)>);

#[derive(Clone, Default)]
struct Recorder {
    spans: Arc<Mutex<Vec<Record>>>,
    events: Arc<Mutex<Vec<Record>>>,
}

#[derive(Default)]
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
        let mut collector = Collector::default();
        attributes.record(&mut collector);
        let mut spans = self.spans.lock().unwrap_or_else(PoisonError::into_inner);
        spans.push((attributes.metadata().name().to_owned(), collector.0));
        tracing::span::Id::from_u64(spans.len() as u64)
    }
    fn record(&self, id: &tracing::span::Id, values: &tracing::span::Record<'_>) {
        let mut collector = Collector::default();
        values.record(&mut collector);
        let mut spans = self.spans.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(span) = spans.get_mut(id.into_u64() as usize - 1) {
            span.1.extend(collector.0);
        }
    }
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut collector = Collector::default();
        event.record(&mut collector);
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((event.metadata().target().to_owned(), collector.0));
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

struct AlwaysFails(AtomicU32);

impl JobHandler for AlwaysFails {
    fn handle(&self, _: Job, _: CancelScope) -> HandlerFuture<'_> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(HandlerError::Retry) })
    }
}

fn field(record: &Record, name: &str) -> String {
    record
        .1
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| panic!("field {name} is missing from the record"))
}

fn number(text: &str) -> u64 {
    text.parse()
        .unwrap_or_else(|_| panic!("the next run field is not a bare number"))
}

// A current-thread runtime, so the thread-local `set_default` subscriber sees what every spawned
// task emits without installing a global subscriber (C-O7 holds in tests as in the library).
#[tokio::test(flavor = "current_thread")]
async fn every_attempt_is_one_structured_event_and_the_span_carries_the_trace() {
    let recorder = Recorder::default();
    let _guard = tracing::subscriber::set_default(recorder.clone());

    let registry = Registry::new();
    let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
    let entropy: Arc<dyn EntropySource> = Arc::new(FixedEntropy::new([0x99; 16]));
    let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
    let queue = QueueName::new("events").unwrap();
    let kind = JobKind::new("fail").unwrap();
    let trace = TraceContext::parse(
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        None,
    )
    .unwrap();
    let id = store
        .enqueue(
            NewJob::new(
                queue.clone(),
                kind.clone(),
                JobPayload::within(b"hunter2CanaryDoNotLeak".to_vec(), &JobBounds::new()).unwrap(),
            )
            .with_max_attempts(3)
            .unwrap()
            .with_trace(trace),
            clock.now(),
        )
        .await
        .unwrap()
        .id();
    let config = WorkerConfig::new(queue.clone())
        .with_poll_interval(Duration::from_millis(10))
        .unwrap()
        .with_stop_grace(Duration::from_millis(100))
        .unwrap()
        .with_retry(
            RetryPolicy::new(
                100,
                Duration::from_secs(1),
                Duration::from_secs(8),
                Duration::from_secs(1),
            )
            .unwrap()
            .with_jitter(Jitter::None),
        );
    let worker = Arc::new(
        Worker::new(
            Arc::clone(&store),
            config,
            clock.clone(),
            Arc::clone(&entropy),
            &registry,
        )
        .unwrap()
        .register(kind, Arc::new(AlwaysFails(AtomicU32::new(0)))),
    );
    let cancel = CancelScope::root();
    let run = tokio::spawn(Arc::clone(&worker).run(WorkGate::new(), cancel.clone()));
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(20)).await;
        if store.read(&id).await.unwrap().unwrap().state == JobState::Dead {
            break;
        }
        // Each attempt reschedules a few seconds out; the injected clock jumps past it.
        clock.advance(Duration::from_secs(10));
    }
    cancel.cancel();
    run.await.unwrap();

    // Exactly one "job attempt finished" event per attempt, with the closed field set.
    let events = recorder
        .events
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let finished: Vec<&Record> = events
        .iter()
        .filter(|(target, fields)| {
            target == JOBS_EVENT_TARGET
                && fields
                    .iter()
                    .any(|(k, v)| k == "message" && v.contains("job attempt finished"))
        })
        .collect();
    assert_eq!(finished.len(), 3, "one event per attempt");
    for (index, record) in finished.iter().enumerate() {
        assert_eq!(field(record, "attempt"), (index + 1).to_string());
        assert_eq!(field(record, "max_attempts"), "3");
        assert_eq!(field(record, "queue"), "events");
        assert_eq!(field(record, "kind"), "fail");
        assert!(field(record, "job_id").contains(&id.encode()));
    }
    assert_eq!(field(finished[0], "outcome"), "\"retried\"");
    assert_eq!(field(finished[1], "outcome"), "\"retried\"");
    assert_eq!(field(finished[2], "outcome"), "\"dead_lettered\"");
    // A retried attempt carries a numeric next run time; the dead one carries no such field at
    // all (tracing records `Some(v)` as `v` and omits `None`). With jitter off the schedule is
    // exact: base 1 s × 2^(attempt−1), so the second delay is one second longer than the first,
    // offset by the 10 s the injected clock jumped between attempts.
    let first_next = number(&field(finished[0], "next_run_at_unix_ms"));
    let second_next = number(&field(finished[1], "next_run_at_unix_ms"));
    assert!(
        finished[2]
            .1
            .iter()
            .all(|(k, _)| k != "next_run_at_unix_ms"),
        "a dead-lettered attempt has no next run"
    );
    assert_eq!(field(finished[2], "failure"), "\"handler_failed\"");
    assert_eq!(first_next % 1_000, 0);
    assert_eq!(second_next - first_next, 10_000 + 1_000);

    // Nothing emitted carries the payload.
    let everything = format!("{events:?}");
    assert!(
        !everything.contains("hunter2"),
        "the payload reached an event"
    );

    // The execution span carries the trace context as fields (FR-038).
    let spans = recorder
        .spans
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let job_spans: Vec<&Record> = spans
        .iter()
        .filter(|(name, _)| name == JOB_SPAN_NAME)
        .collect();
    assert_eq!(job_spans.len(), 3, "one span per execution");
    for span in job_spans {
        assert_eq!(
            field(span, "trace_id"),
            "\"4bf92f3577b34da6a3ce929d0e0e4736\""
        );
        assert_eq!(field(span, "parent_span_id"), "\"00f067aa0ba902b7\"");
        assert_eq!(field(span, "trace_flags"), "\"01\"");
    }
    let everything = format!("{spans:?}");
    assert!(
        !everything.contains("hunter2"),
        "the payload reached a span"
    );
}
