//! Durable jobs with the in-memory store and a worker.
//!
//! ```sh
//! cargo run -p renvor --example jobs --features capability-jobs
//! ```
//!
//! The database-backed stores are `renvor_sqlx::jobs` and `renvor_seaorm::jobs` behind their
//! `jobs` features; they implement the same `JobStore`, so only the store construction changes.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use renvor::jobs::{
    HandlerError, HandlerFuture, Job, JobBounds, JobKind, JobMetrics, JobPayload, JobState,
    JobsClient, QueueName,
};
use renvor::kernel::cancel::CancelScope;
use renvor::kernel::clock::{Clock, SystemClock};
use renvor::kernel::lifecycle::drain::WorkGate;
use renvor::kernel::observe::OsEntropy;
use renvor::kernel::observe::metrics::Registry;
use renvor::{JobHandler, JobStore as _, MemoryJobStore, NewJob, Worker, WorkerConfig};

/// Counts how many times it ran, and fails the first attempt to show a bounded retry.
struct Greet(AtomicU32);

impl JobHandler for Greet {
    fn handle(&self, _job: Job, _cancel: CancelScope) -> HandlerFuture<'_> {
        let attempt = self.0.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move {
            if attempt == 1 {
                Err(HandlerError::Retry)
            } else {
                Ok(())
            }
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let entropy: Arc<dyn renvor::kernel::observe::EntropySource> = Arc::new(OsEntropy::new());
    let bounds = JobBounds::new();
    let store = Arc::new(MemoryJobStore::new(bounds, Arc::clone(&entropy)));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::new());
    let registry = Registry::new();
    let queue = QueueName::new("greetings")?;
    let kind = JobKind::new("greet")?;

    let client = JobsClient::new(
        Arc::clone(&store),
        Arc::clone(&clock),
        JobMetrics::register(&registry)?,
    );
    let enqueued = client
        .enqueue(
            NewJob::new(
                queue.clone(),
                kind.clone(),
                JobPayload::within(b"ada".to_vec(), &bounds)?,
            )
            .with_max_attempts(3)?,
        )
        .await?;
    let id = enqueued.id();
    println!("enqueued: {enqueued:?}");

    let config = WorkerConfig::new(queue)
        .with_poll_interval(Duration::from_millis(10))?
        .with_retry(
            renvor::kernel::retry::RetryPolicy::new(
                3,
                Duration::from_millis(10),
                Duration::from_millis(50),
                Duration::from_secs(1),
            )?
            .with_jitter(renvor::kernel::retry::Jitter::None),
        );
    let greet = Arc::new(Greet(AtomicU32::new(0)));
    let worker = Arc::new(
        Worker::new(Arc::clone(&store), config, clock, entropy, &registry)?
            .register(kind, Arc::clone(&greet) as Arc<dyn JobHandler>),
    );
    let cancel = CancelScope::root();
    let run = tokio::spawn(Arc::clone(&worker).run(WorkGate::new(), cancel.clone()));

    // Wait for the job to settle: one failed attempt, one retry, done.
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        if store
            .read(&id)
            .await?
            .is_some_and(|job| job.state == JobState::Completed)
        {
            break;
        }
    }
    cancel.cancel();
    let report = run.await?;
    let job = store.read(&id).await?.expect("the job exists");
    println!(
        "state: {:?}, attempts: {}, handler calls: {}, report: {report:?}",
        job.state,
        job.attempts,
        greet.0.load(Ordering::SeqCst)
    );
    Ok(())
}
