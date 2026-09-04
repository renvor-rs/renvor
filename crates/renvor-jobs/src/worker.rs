//! The worker: bounded concurrency, kernel work permits, per-job timeouts, panic containment,
//! and retries that are scheduled by the kernel's policy and visible in telemetry.
//!
//! # Every unit of work is the kernel's unit of work
//!
//! A running job holds a [`WorkPermit`] from the application's [`WorkGate`], taken from the
//! provider's initialisation context. That is what makes `Drain` wait for in-flight jobs within
//! its budget and report the ones still running at the deadline through the same path a timed-out
//! HTTP request uses (C-L5, FR-032). A worker with a private gate would drain something the kernel
//! could not see.
//!
//! # A handler runs in its own task, for one reason
//!
//! Panic containment. A task that panics ends with a `JoinError` whose `is_panic()` is true; the
//! worker records the attempt as [`FailureKind::Panicked`] and the payload never reaches a log or a
//! row (FR-034). `std::panic::catch_unwind` cannot contain a panic that happens across an `.await`;
//! a task boundary can. The same task is what the handler timeout aborts (FR-035), under the
//! job's own cancellation scope — a child of the application's — so shutdown reaches it too.
//!
//! # Retries are the policy's, not the worker's
//!
//! When an attempt fails and attempts remain, the next run time is `now + policy.delay(attempt)`
//! from the kernel's pure schedule (ADR-0037), and the store reschedules or dead-letters by its own
//! count. The worker never invents a delay and never retries an [`HandlerError::Abandon`], which is
//! the closed "terminal" class: a refusal retried is work for nothing (FR-092).
//!
//! # Stop gives back what it aborts, bounded, and says how that went
//!
//! A job still running at the stop grace is aborted and its lease released — but a release is a
//! store call, and the store is the one dependency most likely to be failing at the moment an
//! application is stopping. So every release runs under [`RELEASE_TIMEOUT`], the aborted jobs'
//! releases run concurrently so the phase costs one bound rather than their sum, and the
//! outcome of each is **counted, never discarded**: only a release the store confirmed moves the
//! `released` counter; a refusal or a silence is a store error under its closed category, one
//! `warn` event, and a count in [`WorkerReport`] that the provider turns into an unclean stop
//! (FR-025, FR-033, C-L2). `MAX_STOP_GRACE + RELEASE_TIMEOUT` fits under the kernel's provider
//! deadline, checked at compile time.
//!
//! # What is emitted
//!
//! One structured event per attempt on the `renvor.jobs` target — job id, queue, kind, attempt,
//! `max_attempts`, the closed outcome, and the next run time as a number — and one span per
//! execution carrying the job's trace context as fields (FR-031, FR-038). Counters and a duration
//! histogram through the kernel's bounded metrics port (FR-083).

use core::fmt;
use core::future::Future;
use core::pin::Pin;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use renvor_core::cancel::CancelScope;
use renvor_core::clock::Clock;
use renvor_core::lifecycle::DEFAULT_PROVIDER_DEADLINE;
use renvor_core::lifecycle::drain::{WorkGate, WorkPermit};
use renvor_core::observe::entropy::EntropySource;
use renvor_core::observe::metrics::{Counter, Histogram, MetricsError, Registry};
use renvor_core::retry::RetryPolicy;
use tokio::sync::Semaphore;
use tracing::Instrument as _;

use crate::job::{
    ClaimedJob, DEFAULT_HANDLER_TIMEOUT, DEFAULT_LEASE, FailureKind, FailureOutcome, Job, JobError,
    JobKind, JobRefusal, LeaseToken, MAX_ATTEMPTS_CAP, MAX_HANDLER_TIMEOUT_CAP, MAX_LEASE_CAP,
    NewJob, QueueName,
};
use crate::store::JobStore;

/// The tracing target every job event and span is emitted on.
pub const JOBS_EVENT_TARGET: &str = "renvor.jobs";
/// The name of the span one job execution runs under.
pub const JOB_SPAN_NAME: &str = "renvor.job";

/// The default number of jobs one worker runs at once.
pub const DEFAULT_CONCURRENCY: usize = 4;
/// The hard cap on concurrency.
pub const MAX_CONCURRENCY: usize = 1024;
/// The default pause between empty claim attempts.
pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// The floor and cap on the poll interval.
pub const POLL_INTERVAL_RANGE: (Duration, Duration) =
    (Duration::from_millis(10), Duration::from_secs(60));
/// How long `stop` waits for running jobs before aborting them and releasing their leases.
pub const DEFAULT_STOP_GRACE: Duration = Duration::from_secs(5);
/// The hard cap on the stop grace, chosen to fit under the kernel's default provider deadline.
pub const MAX_STOP_GRACE: Duration = Duration::from_secs(25);
/// How long one lease release may take at stop. The aborted jobs' leases are released
/// concurrently, so this also bounds the whole release phase.
///
/// Sized so that the stop grace cap plus this bound fits under the kernel's default provider
/// deadline ([`DEFAULT_PROVIDER_DEADLINE`], 30 s): 25 s + 2 s ≤ 30 s, pinned at compile time
/// below. A release the store does not answer inside this bound is counted as timed out and
/// reported — never awaited for ever, which is what an unbounded release would be when the store
/// is the thing that is failing (FR-025, C-L7, FR-033).
pub const RELEASE_TIMEOUT: Duration = Duration::from_secs(2);
/// How long Boot waits for the store to answer the readiness probe.
///
/// Under the kernel's default provider deadline ([`DEFAULT_PROVIDER_DEADLINE`], 30 s), so a
/// store that accepts a connection and never answers fails Boot as `timed_out` — a closed
/// category that names the dependency — rather than as a kernel deadline that can only name the
/// provider (FR-012, C-L7).
pub const STORE_PROBE_TIMEOUT: Duration = Duration::from_secs(10);

// The two bounds above are promises about the kernel's deadline, so they are checked against it
// where the compiler can see both: a future edit to either constant, or to the kernel's, fails
// here rather than at a provider deadline in production.
const _: () = assert!(
    MAX_STOP_GRACE.as_millis() + RELEASE_TIMEOUT.as_millis()
        <= DEFAULT_PROVIDER_DEADLINE.as_millis(),
    "the stop grace cap plus the release bound must fit under the kernel's provider deadline"
);
const _: () = assert!(
    STORE_PROBE_TIMEOUT.as_millis() < DEFAULT_PROVIDER_DEADLINE.as_millis(),
    "the store probe must be bounded under the kernel's provider deadline"
);

/// How a handler reports failure. **Closed and fieldless**: the handler's own diagnostics belong
/// in its own span, never in a value the worker would store or log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandlerError {
    /// Transient. Retry if attempts remain.
    Retry,
    /// Terminal. Dead-letter now, whatever the attempt count.
    Abandon,
}

/// The future a handler returns.
pub type HandlerFuture<'a> = Pin<Box<dyn Future<Output = Result<(), HandlerError>> + Send + 'a>>;

/// Something that runs one kind of job.
///
/// Boxed futures by hand, for the reason the kernel's `Provider` gives: the registry holds a
/// heterogeneous set behind `dyn`, and a trait with `async fn` is not `dyn`-compatible.
pub trait JobHandler: Send + Sync + 'static {
    /// Runs `job`. `cancel` fires when the application drains or the job times out; a handler that
    /// watches it stops early, one that does not is bounded by the timeout anyway.
    fn handle(&self, job: Job, cancel: CancelScope) -> HandlerFuture<'_>;
}

/// The worker's bounds and policy.
#[derive(Clone, Debug)]
pub struct WorkerConfig {
    queue: QueueName,
    concurrency: usize,
    poll_interval: Duration,
    lease: Duration,
    handler_timeout: Duration,
    stop_grace: Duration,
    retry: RetryPolicy,
}

impl WorkerConfig {
    /// A worker on `queue` with the documented defaults: 4 concurrent jobs, a 500 ms poll, a
    /// 60 s lease, a 5 min handler timeout, a 5 s stop grace, and a 1 s → 5 min jittered schedule.
    #[must_use]
    pub fn new(queue: QueueName) -> Self {
        Self {
            queue,
            concurrency: DEFAULT_CONCURRENCY,
            poll_interval: DEFAULT_POLL_INTERVAL,
            lease: DEFAULT_LEASE,
            handler_timeout: DEFAULT_HANDLER_TIMEOUT,
            stop_grace: DEFAULT_STOP_GRACE,
            retry: RetryPolicy::new(
                MAX_ATTEMPTS_CAP,
                Duration::from_secs(1),
                Duration::from_secs(5 * 60),
                DEFAULT_HANDLER_TIMEOUT,
            )
            .expect("the default retry policy is within every bound"),
        }
    }

    /// Replaces the concurrency bound (1–1024).
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::BoundOutOfRange`].
    pub fn with_concurrency(mut self, concurrency: usize) -> Result<Self, JobError> {
        if concurrency == 0 || concurrency > MAX_CONCURRENCY {
            return Err(JobError::Refused(JobRefusal::BoundOutOfRange));
        }
        self.concurrency = concurrency;
        Ok(self)
    }

    /// Replaces the poll interval (10 ms – 60 s).
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::BoundOutOfRange`].
    pub fn with_poll_interval(mut self, interval: Duration) -> Result<Self, JobError> {
        if interval < POLL_INTERVAL_RANGE.0 || interval > POLL_INTERVAL_RANGE.1 {
            return Err(JobError::Refused(JobRefusal::BoundOutOfRange));
        }
        self.poll_interval = interval;
        Ok(self)
    }

    /// Replaces the lease (1 s – 1 h).
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::BoundOutOfRange`].
    pub fn with_lease(mut self, lease: Duration) -> Result<Self, JobError> {
        if lease < Duration::from_secs(1) || lease > MAX_LEASE_CAP {
            return Err(JobError::Refused(JobRefusal::BoundOutOfRange));
        }
        self.lease = lease;
        Ok(self)
    }

    /// Replaces the handler timeout (1 s – 24 h).
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::BoundOutOfRange`].
    pub fn with_handler_timeout(mut self, timeout: Duration) -> Result<Self, JobError> {
        if timeout < Duration::from_secs(1) || timeout > MAX_HANDLER_TIMEOUT_CAP {
            return Err(JobError::Refused(JobRefusal::BoundOutOfRange));
        }
        self.handler_timeout = timeout;
        Ok(self)
    }

    /// Replaces the stop grace (≤ 25 s).
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::BoundOutOfRange`].
    pub fn with_stop_grace(mut self, grace: Duration) -> Result<Self, JobError> {
        if grace > MAX_STOP_GRACE {
            return Err(JobError::Refused(JobRefusal::BoundOutOfRange));
        }
        self.stop_grace = grace;
        Ok(self)
    }

    /// Replaces the retry schedule. Only its delay fields are used; the job's own `max_attempts`
    /// bounds the count.
    #[must_use]
    pub const fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    /// The queue.
    #[must_use]
    pub const fn queue(&self) -> &QueueName {
        &self.queue
    }

    /// The concurrency bound.
    #[must_use]
    pub const fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// The lease a claim holds.
    #[must_use]
    pub const fn lease(&self) -> Duration {
        self.lease
    }

    /// The handler timeout.
    #[must_use]
    pub const fn handler_timeout(&self) -> Duration {
        self.handler_timeout
    }
}

/// The counters and histogram the jobs capability records (FR-083).
#[derive(Clone, Debug)]
pub struct JobMetrics {
    enqueued: Counter,
    claimed: Counter,
    attempts: Counter,
    released: Counter,
    store_errors: Counter,
    duration: Histogram,
}

impl JobMetrics {
    /// Registers the instruments in `registry`, or returns the ones already registered.
    ///
    /// # Errors
    ///
    /// [`MetricsError`] if a name is already registered with a different shape.
    pub fn register(registry: &Registry) -> Result<Self, MetricsError> {
        Ok(Self {
            enqueued: registry.counter(
                "renvor_jobs_enqueued_total",
                "Jobs accepted by enqueue, by outcome.",
                &["queue", "outcome"],
            )?,
            claimed: registry.counter(
                "renvor_jobs_claimed_total",
                "Jobs claimed by a worker.",
                &["queue"],
            )?,
            attempts: registry.counter(
                "renvor_jobs_attempts_total",
                "Job attempts finished, by outcome.",
                &["queue", "kind", "outcome"],
            )?,
            released: registry.counter(
                "renvor_jobs_released_total",
                "Leases released at shutdown.",
                &["queue"],
            )?,
            store_errors: registry.counter(
                "renvor_jobs_store_errors_total",
                "Store operations that failed, by category.",
                &["queue", "category"],
            )?,
            duration: registry.histogram(
                "renvor_jobs_duration_seconds",
                "Handler execution time.",
                &["queue", "kind"],
                &[0.01, 0.1, 1.0, 10.0, 60.0, 600.0],
            )?,
        })
    }

    /// The `enqueued` counter, for an application that enqueues without [`JobsClient`].
    #[must_use]
    pub const fn enqueued(&self) -> &Counter {
        &self.enqueued
    }
}

/// The application's way in: enqueue with the clock and the metrics applied.
#[derive(Clone, Debug)]
pub struct JobsClient<S> {
    store: Arc<S>,
    clock: Arc<dyn Clock>,
    metrics: JobMetrics,
}

impl<S: JobStore> JobsClient<S> {
    /// Builds a client over `store`.
    #[must_use]
    pub fn new(store: Arc<S>, clock: Arc<dyn Clock>, metrics: JobMetrics) -> Self {
        Self {
            store,
            clock,
            metrics,
        }
    }

    /// Enqueues `job` at the clock's `now`, counting the outcome.
    ///
    /// # Errors
    ///
    /// As [`JobStore::enqueue`].
    pub async fn enqueue(&self, job: NewJob) -> Result<crate::job::Enqueued, JobError> {
        let queue = job.queue().clone();
        let outcome = self.store.enqueue(job, self.clock.now()).await;
        let label = match &outcome {
            Ok(crate::job::Enqueued::Created(_)) => "created",
            Ok(crate::job::Enqueued::Duplicate(_)) => "duplicate",
            Err(JobError::QueueFull) => "queue_full",
            Err(_) => "error",
        };
        self.metrics
            .enqueued
            .increment(&[("queue", queue.as_str()), ("outcome", label)], 1);
        outcome
    }

    /// The store.
    #[must_use]
    pub fn store(&self) -> &Arc<S> {
        &self.store
    }
}

/// What one attempt ended as, for the event and the counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttemptOutcome {
    Completed,
    Retried,
    DeadLettered,
}

impl AttemptOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Retried => "retried",
            Self::DeadLettered => "dead_lettered",
        }
    }
}

/// Leases held by running tasks, so `stop` can release the ones it aborts.
type InFlight = Arc<Mutex<HashMap<tokio::task::Id, LeaseToken>>>;

/// How one lease release ended. Closed, so the report and the metric agree by construction on
/// what "released" means: **the store confirmed it**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReleaseOutcome {
    Released,
    Failed,
    TimedOut,
}

/// A worker over one queue.
pub struct Worker<S> {
    store: Arc<S>,
    config: WorkerConfig,
    handlers: HashMap<JobKind, Arc<dyn JobHandler>>,
    clock: Arc<dyn Clock>,
    entropy: Arc<dyn EntropySource>,
    metrics: JobMetrics,
}

impl<S> fmt::Debug for Worker<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Worker")
            .field("queue", &self.config.queue)
            .field("concurrency", &self.config.concurrency)
            .field("handlers", &self.handlers.len())
            .finish_non_exhaustive()
    }
}

/// What a finished run reports.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkerReport {
    /// Jobs claimed over the run.
    pub claimed: u64,
    /// Jobs still running when the stop grace elapsed, and aborted. A release was attempted for
    /// each; `released`, `release_failed`, and `release_timed_out` say how those ended.
    pub aborted: u64,
    /// Leases the store **confirmed** released — at the stop grace, or because shutdown began
    /// between a claim and its permit. Only a confirmed release counts.
    pub released: u64,
    /// Releases the store refused. Those jobs stay leased until the lease expires and is
    /// reclaimed, with the attempt counted (C-J4).
    pub release_failed: u64,
    /// Releases that did not answer within [`RELEASE_TIMEOUT`]; the same consequence as a
    /// refusal, counted apart because a silent store and a refusing store are different faults.
    pub release_timed_out: u64,
}

impl WorkerReport {
    fn record(&mut self, outcome: ReleaseOutcome) {
        match outcome {
            ReleaseOutcome::Released => self.released += 1,
            ReleaseOutcome::Failed => self.release_failed += 1,
            ReleaseOutcome::TimedOut => self.release_timed_out += 1,
        }
    }
}

impl<S: JobStore + 'static> Worker<S> {
    /// Builds a worker with no handlers. Register at least one before running.
    ///
    /// # Errors
    ///
    /// [`MetricsError`] if the instruments cannot be registered.
    pub fn new(
        store: Arc<S>,
        config: WorkerConfig,
        clock: Arc<dyn Clock>,
        entropy: Arc<dyn EntropySource>,
        registry: &Registry,
    ) -> Result<Self, MetricsError> {
        Ok(Self {
            store,
            config,
            handlers: HashMap::new(),
            clock,
            entropy,
            metrics: JobMetrics::register(registry)?,
        })
    }

    /// Registers the handler for `kind`, replacing any earlier one.
    #[must_use]
    pub fn register(mut self, kind: JobKind, handler: Arc<dyn JobHandler>) -> Self {
        self.handlers.insert(kind, handler);
        self
    }

    /// The configuration.
    #[must_use]
    pub const fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// The metrics this worker records to.
    #[must_use]
    pub const fn metrics(&self) -> &JobMetrics {
        &self.metrics
    }

    /// One bounded call against the store, for Boot: `depth` on the configured queue under
    /// [`STORE_PROBE_TIMEOUT`]. Proves the store answers — credentials, schema, permission,
    /// route — before the provider reports ready (FR-012), because a run loop over a store that
    /// cannot answer would warn "claim failed" every poll interval for ever while readiness said
    /// otherwise.
    ///
    /// `depth` rather than `claim`: it reads the table the worker will claim from and moves
    /// nothing, so a probe can never lease a job that no loop is yet running to finish.
    ///
    /// # Errors
    ///
    /// The store's own closed category, or [`JobError::TimedOut`] when the bound elapsed. On
    /// success one `info` event carries the queue and its depth — closed fields, no address.
    pub async fn probe_store(&self) -> Result<u64, JobError> {
        let depth =
            match tokio::time::timeout(STORE_PROBE_TIMEOUT, self.store.depth(&self.config.queue))
                .await
            {
                Ok(answer) => answer?,
                Err(_elapsed) => return Err(JobError::TimedOut),
            };
        tracing::info!(
            target: JOBS_EVENT_TARGET,
            queue = %self.config.queue,
            depth,
            "the job store answered the readiness probe"
        );
        Ok(depth)
    }

    /// Runs until `cancel` fires, then stops claiming, waits up to the stop grace for running
    /// jobs, aborts the rest, and releases their leases — each release bounded by
    /// [`RELEASE_TIMEOUT`], run concurrently, and counted in the report by how it ended.
    ///
    /// Every claimed job holds a permit from `gate` while it runs, so the kernel's drain sees it.
    /// A claim refused by the gate (shutdown began between the claim and the permit) is released
    /// at once, under the same bound and counted the same way, so a job is never left leased by
    /// a worker that will not run it without the report saying so.
    pub async fn run(self: Arc<Self>, gate: WorkGate, cancel: CancelScope) -> WorkerReport {
        let semaphore = Arc::new(Semaphore::new(self.config.concurrency));
        let in_flight: InFlight = Arc::new(Mutex::new(HashMap::new()));
        let mut tasks = tokio::task::JoinSet::new();
        let mut report = WorkerReport::default();

        'outer: loop {
            if cancel.is_cancelled() {
                break;
            }
            // Reap finished tasks so the join set does not grow without bound.
            while tasks.try_join_next().is_some() {}

            let mut claimed_any = false;
            loop {
                let Ok(slot) = Arc::clone(&semaphore).try_acquire_owned() else {
                    break;
                };
                let now = self.clock.now();
                let claimed = match self
                    .store
                    .claim(&self.config.queue, now, self.config.lease)
                    .await
                {
                    Ok(Some(claimed)) => claimed,
                    Ok(None) => break,
                    Err(error) => {
                        self.metrics.store_errors.increment(
                            &[
                                ("queue", self.config.queue.as_str()),
                                ("category", error.as_str()),
                            ],
                            1,
                        );
                        tracing::warn!(
                            target: JOBS_EVENT_TARGET,
                            queue = %self.config.queue,
                            category = error.as_str(),
                            "claim failed; the worker will poll again"
                        );
                        break;
                    }
                };
                claimed_any = true;
                report.claimed += 1;
                self.metrics
                    .claimed
                    .increment(&[("queue", self.config.queue.as_str())], 1);

                let permit = match gate.begin("job") {
                    Ok(permit) => permit,
                    Err(_shutting_down) => {
                        // Shutdown began between the claim and the permit: give the job back,
                        // bounded and counted exactly as a release at the stop grace is.
                        report.record(self.release_lease(&claimed.lease).await);
                        drop(slot);
                        break 'outer;
                    }
                };

                let worker = Arc::clone(&self);
                let scope = cancel.child(format!("job:{}", claimed.job.id));
                let in_flight_for_task = Arc::clone(&in_flight);
                let lease = claimed.lease;
                let handle = tasks.spawn(async move {
                    let _slot = slot;
                    let _permit: WorkPermit = permit;
                    worker.run_one(claimed, scope).await;
                    in_flight_for_task
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(&tokio::task::id());
                });
                in_flight
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(handle.id(), lease);
            }

            if !claimed_any {
                tokio::select! {
                    () = cancel.cancelled() => break,
                    () = tokio::time::sleep(self.config.poll_interval) => {}
                }
            }
        }

        // Stop: wait for running jobs up to the grace, then abort and release what is left.
        let graceful = tokio::time::timeout(self.config.stop_grace, async {
            while tasks.join_next().await.is_some() {}
        })
        .await;
        if graceful.is_err() {
            tasks.abort_all();
            while tasks.join_next().await.is_some() {}
            let leases: Vec<LeaseToken> = in_flight
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .drain()
                .map(|(_, lease)| lease)
                .collect();
            // The releases run concurrently, each under RELEASE_TIMEOUT, so the whole phase is
            // bounded by one RELEASE_TIMEOUT rather than by their sum: N leases against a store
            // that has stopped answering must not cost N × the bound, which would be an unbounded
            // wait in all but name (FR-025, C-L7) and would run the provider past its deadline.
            let mut releases = tokio::task::JoinSet::new();
            for lease in leases {
                report.aborted += 1;
                let worker = Arc::clone(&self);
                releases.spawn(async move { worker.release_lease(&lease).await });
            }
            while let Some(joined) = releases.join_next().await {
                let outcome = joined.unwrap_or_else(|_did_not_finish| {
                    // The store's `release` panicked (or the runtime is going down under it):
                    // contained by the task boundary and counted as the store not answering, so
                    // the report stays whole and the stop still says what is left leased.
                    self.release_failed(JobError::Unavailable);
                    ReleaseOutcome::Failed
                });
                report.record(outcome);
            }
        }
        tracing::info!(
            target: JOBS_EVENT_TARGET,
            queue = %self.config.queue,
            claimed = report.claimed,
            aborted = report.aborted,
            released = report.released,
            release_failed = report.release_failed,
            release_timed_out = report.release_timed_out,
            "worker stopped"
        );
        report
    }

    /// Gives one lease back under [`RELEASE_TIMEOUT`] and says how that ended.
    ///
    /// Only a release the store **confirmed** moves the `released` counter. A refusal or a
    /// silence is a store error under its closed category, one `warn` event carrying the queue
    /// and the category (never the token), and a count in the report. The outcome is reported,
    /// never discarded: a lease the store still holds is a job no worker will run until the
    /// lease expires and is reclaimed (C-J4), and an operator has to learn that from somewhere
    /// other than a queue that has gone quiet.
    async fn release_lease(&self, lease: &LeaseToken) -> ReleaseOutcome {
        match tokio::time::timeout(RELEASE_TIMEOUT, self.store.release(lease, self.clock.now()))
            .await
        {
            Ok(Ok(())) => {
                self.metrics
                    .released
                    .increment(&[("queue", self.config.queue.as_str())], 1);
                ReleaseOutcome::Released
            }
            Ok(Err(error)) => {
                self.release_failed(error);
                ReleaseOutcome::Failed
            }
            Err(_elapsed) => {
                self.release_failed(JobError::TimedOut);
                ReleaseOutcome::TimedOut
            }
        }
    }

    /// Counts and reports a release that did not succeed, by its closed category.
    fn release_failed(&self, error: JobError) {
        self.metrics.store_errors.increment(
            &[
                ("queue", self.config.queue.as_str()),
                ("category", error.as_str()),
            ],
            1,
        );
        tracing::warn!(
            target: JOBS_EVENT_TARGET,
            queue = %self.config.queue,
            category = error.as_str(),
            "a lease was not released at stop; the job stays leased until the lease expires"
        );
    }

    /// Runs one claimed job to a transition.
    async fn run_one(&self, claimed: ClaimedJob, scope: CancelScope) {
        let job = claimed.job;
        let lease = claimed.lease;
        let queue = job.queue.clone();
        let kind = job.kind.clone();
        let attempt = job.attempts;
        let max_attempts = job.max_attempts;
        let id = job.id;

        let span = tracing::info_span!(
            target: JOBS_EVENT_TARGET,
            JOB_SPAN_NAME,
            job_id = %id,
            queue = %queue,
            kind = %kind,
            attempt,
            trace_id = tracing::field::Empty,
            parent_span_id = tracing::field::Empty,
            trace_flags = tracing::field::Empty,
        );
        if let Some(trace) = job.trace.as_ref() {
            span.record("trace_id", trace.trace_id().encode());
            span.record("parent_span_id", trace.parent_id().encode());
            span.record("trace_flags", trace.flags().encode());
        }

        let started = tokio::time::Instant::now();
        let failure: Option<FailureKind> = match self.handlers.get(&kind) {
            None => {
                // No handler for this kind is a configuration defect, not a transient failure:
                // dead-letter it so it is visible rather than retried for ever.
                tracing::error!(
                    target: JOBS_EVENT_TARGET,
                    job_id = %id,
                    queue = %queue,
                    kind = %kind,
                    "no handler is registered for this kind; the job is dead-lettered"
                );
                Some(FailureKind::Abandoned)
            }
            Some(handler) => {
                let handler = Arc::clone(handler);
                let task_scope = scope.clone();
                let task = tokio::spawn(
                    async move { handler.handle(job, task_scope).await }.instrument(span.clone()),
                );
                let abort = task.abort_handle();
                match tokio::time::timeout(self.config.handler_timeout, task).await {
                    Ok(Ok(Ok(()))) => None,
                    Ok(Ok(Err(HandlerError::Retry))) => Some(FailureKind::HandlerFailed),
                    Ok(Ok(Err(HandlerError::Abandon))) => Some(FailureKind::Abandoned),
                    Ok(Err(join)) if join.is_panic() => Some(FailureKind::Panicked),
                    // Cancelled by shutdown before it finished: released by `run`'s abort path
                    // if it was aborted there, otherwise treated as a retryable failure.
                    Ok(Err(_cancelled)) => Some(FailureKind::HandlerFailed),
                    Err(_elapsed) => {
                        abort.abort();
                        scope.cancel();
                        Some(FailureKind::TimedOut)
                    }
                }
            }
        };
        let elapsed = started.elapsed();
        self.metrics.duration.observe(
            &[("queue", queue.as_str()), ("kind", kind.as_str())],
            elapsed.as_secs_f64(),
        );

        let now = self.clock.now();
        let (outcome, next_run) = match failure {
            None => match self.store.complete(&lease, now).await {
                Ok(_) => (AttemptOutcome::Completed, None),
                Err(error) => {
                    self.store_error(&queue, error);
                    return;
                }
            },
            Some(kind_of_failure) => {
                let delay = self
                    .config
                    .retry
                    .delay(attempt, &*self.entropy)
                    .unwrap_or(self.config.retry.max_delay());
                let next_run_at = now + delay;
                match self
                    .store
                    .fail(&lease, kind_of_failure, next_run_at, now)
                    .await
                {
                    Ok(FailureOutcome::Rescheduled { run_at, .. }) => {
                        (AttemptOutcome::Retried, Some(run_at))
                    }
                    Ok(FailureOutcome::DeadLettered { .. }) => (AttemptOutcome::DeadLettered, None),
                    Err(error) => {
                        self.store_error(&queue, error);
                        return;
                    }
                }
            }
        };

        self.metrics.attempts.increment(
            &[
                ("queue", queue.as_str()),
                ("kind", kind.as_str()),
                ("outcome", outcome.as_str()),
            ],
            1,
        );
        let _entered = span.enter();
        tracing::info!(
            target: JOBS_EVENT_TARGET,
            job_id = %id,
            queue = %queue,
            kind = %kind,
            attempt,
            max_attempts,
            outcome = outcome.as_str(),
            failure = failure.map(FailureKind::as_str),
            next_run_at_unix_ms = next_run.map(unix_ms),
            duration_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            "job attempt finished"
        );
    }

    fn store_error(&self, queue: &QueueName, error: JobError) {
        self.metrics.store_errors.increment(
            &[("queue", queue.as_str()), ("category", error.as_str())],
            1,
        );
        tracing::error!(
            target: JOBS_EVENT_TARGET,
            queue = %queue,
            category = error.as_str(),
            "a job transition could not be recorded"
        );
    }
}

/// Milliseconds since the Unix epoch, saturating.
fn unix_ms(at: SystemTime) -> u64 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map(|since| u64::try_from(since.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// A store double shared by this module's tests and the provider's: it delegates to
/// [`MemoryJobStore`](crate::memory::MemoryJobStore) except where one call is told to fail or
/// hang, and counts its claims — so a test can prove the run loop never started.
///
/// Kept here rather than in either test module because both need it, and a second copy of a
/// delegating `JobStore` is the kind of duplicate that drifts.
#[cfg(test)]
pub(crate) mod faulty {
    use core::future::Future;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, SystemTime};

    use crate::job::{
        ClaimedJob, Completion, Enqueued, FailureKind, FailureOutcome, Job, JobError, JobId,
        LeaseToken, NewJob, QueueName,
    };
    use crate::memory::MemoryJobStore;
    use crate::store::JobStore;

    /// How one store call misbehaves.
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum Fault {
        /// The call is delegated to the memory store.
        Answers,
        /// The call fails with this closed category — bad credentials, a missing schema, a
        /// refused statement.
        Fails(JobError),
        /// The call never returns — a lost connection or a locked table.
        Hangs,
    }

    /// A store that delegates to memory except where a fault is set.
    pub(crate) struct Faulty {
        inner: Arc<MemoryJobStore>,
        depth: Fault,
        release: Fault,
        claims: AtomicU32,
    }

    impl Faulty {
        pub(crate) fn new(inner: Arc<MemoryJobStore>) -> Self {
            Self {
                inner,
                depth: Fault::Answers,
                release: Fault::Answers,
                claims: AtomicU32::new(0),
            }
        }

        /// Makes `depth` — the Boot probe — misbehave.
        pub(crate) fn with_depth(mut self, fault: Fault) -> Self {
            self.depth = fault;
            self
        }

        /// Makes `release` — the stop path — misbehave.
        pub(crate) fn with_release(mut self, fault: Fault) -> Self {
            self.release = fault;
            self
        }

        /// How many times `claim` was called: zero proves the run loop never started.
        pub(crate) fn claims(&self) -> u32 {
            self.claims.load(Ordering::SeqCst)
        }
    }

    async fn apply<T>(
        fault: Fault,
        answer: impl Future<Output = Result<T, JobError>> + Send,
    ) -> Result<T, JobError> {
        match fault {
            Fault::Answers => answer.await,
            Fault::Fails(error) => Err(error),
            Fault::Hangs => core::future::pending().await,
        }
    }

    impl JobStore for Faulty {
        async fn enqueue(&self, job: NewJob, now: SystemTime) -> Result<Enqueued, JobError> {
            self.inner.enqueue(job, now).await
        }
        async fn claim(
            &self,
            queue: &QueueName,
            now: SystemTime,
            lease: Duration,
        ) -> Result<Option<ClaimedJob>, JobError> {
            self.claims.fetch_add(1, Ordering::SeqCst);
            self.inner.claim(queue, now, lease).await
        }
        async fn complete(
            &self,
            lease: &LeaseToken,
            now: SystemTime,
        ) -> Result<Completion, JobError> {
            self.inner.complete(lease, now).await
        }
        async fn fail(
            &self,
            lease: &LeaseToken,
            failure: FailureKind,
            next_run_at: SystemTime,
            now: SystemTime,
        ) -> Result<FailureOutcome, JobError> {
            self.inner.fail(lease, failure, next_run_at, now).await
        }
        async fn release(&self, lease: &LeaseToken, now: SystemTime) -> Result<(), JobError> {
            apply(self.release, self.inner.release(lease, now)).await
        }
        async fn revive(&self, id: &JobId, now: SystemTime) -> Result<bool, JobError> {
            self.inner.revive(id, now).await
        }
        async fn read(&self, id: &JobId) -> Result<Option<Job>, JobError> {
            self.inner.read(id).await
        }
        async fn depth(&self, queue: &QueueName) -> Result<u64, JobError> {
            apply(self.depth, self.inner.depth(queue)).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::faulty::{Fault, Faulty};
    use super::{
        AttemptOutcome, DEFAULT_CONCURRENCY, HandlerError, HandlerFuture, JobHandler, JobsClient,
        MAX_CONCURRENCY, MAX_STOP_GRACE, POLL_INTERVAL_RANGE, RELEASE_TIMEOUT, Worker,
        WorkerConfig, WorkerReport,
    };
    use crate::job::{
        ClaimedJob, Completion, Enqueued, FailureKind, FailureOutcome, Job, JobBounds, JobError,
        JobId, JobKind, JobPayload, JobState, LeaseToken, NewJob, QueueName,
    };
    use crate::memory::MemoryJobStore;
    use crate::store::JobStore;
    use renvor_core::cancel::CancelScope;
    use renvor_core::clock::{Clock as _, FixedClock};
    use renvor_core::lifecycle::drain::WorkGate;
    use renvor_core::observe::metrics::{Registry, SeriesValue};
    use renvor_core::observe::{EntropySource, FixedEntropy, TraceContext};
    use renvor_core::retry::{Jitter, RetryPolicy};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{Duration, SystemTime};

    /// The value of a counter family summed over its series.
    fn counter_total(registry: &Registry, name: &str) -> f64 {
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

    /// The value of the series of `name` carrying `label = value`, or zero if none does.
    fn counter_with(registry: &Registry, name: &str, label: &str, value: &str) -> f64 {
        registry
            .snapshot()
            .families
            .iter()
            .filter(|family| family.name == name)
            .flat_map(|family| family.series.iter())
            .filter(|series| series.labels.iter().any(|(k, v)| *k == label && v == value))
            .map(|series| match series.value {
                SeriesValue::Scalar(value) => value,
                SeriesValue::Histogram { .. } => 0.0,
            })
            .sum()
    }

    struct Counting {
        calls: AtomicU32,
        outcome: Result<(), HandlerError>,
    }

    impl JobHandler for Counting {
        fn handle(&self, _: Job, _: CancelScope) -> HandlerFuture<'_> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let outcome = self.outcome;
            Box::pin(async move { outcome })
        }
    }

    struct Panicking;

    impl JobHandler for Panicking {
        fn handle(&self, _: Job, _: CancelScope) -> HandlerFuture<'_> {
            Box::pin(async { panic!("hunter2CanaryDoNotLeak") })
        }
    }

    struct Hanging;

    impl JobHandler for Hanging {
        fn handle(&self, _: Job, _: CancelScope) -> HandlerFuture<'_> {
            Box::pin(std::future::pending())
        }
    }

    fn queue() -> QueueName {
        QueueName::new("q").unwrap()
    }

    fn kind() -> JobKind {
        JobKind::new("k").unwrap()
    }

    fn new_job(max_attempts: u32) -> NewJob {
        NewJob::new(
            queue(),
            kind(),
            JobPayload::within(b"hunter2CanaryDoNotLeak".to_vec(), &JobBounds::new()).unwrap(),
        )
        .with_max_attempts(max_attempts)
        .unwrap()
    }

    fn config() -> WorkerConfig {
        WorkerConfig::new(queue())
            .with_poll_interval(Duration::from_millis(10))
            .unwrap()
            .with_handler_timeout(Duration::from_secs(1))
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
            )
    }

    /// Runs a worker until the store is drained of ready work, then cancels it.
    async fn drive<S: JobStore + 'static>(
        worker: Arc<Worker<S>>,
        store: &Arc<S>,
        clock: &Arc<FixedClock>,
        until_idle: impl Fn(&Job) -> bool,
        id: crate::job::JobId,
    ) -> super::WorkerReport {
        let gate = WorkGate::new();
        let cancel = CancelScope::root();
        let run = tokio::spawn(Arc::clone(&worker).run(gate, cancel.clone()));
        // Advance the paused clock and the injected clock in step until the job settles.
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            let job = store.read(&id).await.unwrap().unwrap();
            if until_idle(&job) {
                break;
            }
            // Move wall time forward so rescheduled jobs become claimable.
            clock.advance(Duration::from_secs(10));
        }
        cancel.cancel();
        run.await.unwrap()
    }

    /// A store whose first successful claim closes the gate before returning, so the worker
    /// meets a closed gate with a lease in hand — the window between claim and permit.
    struct ClosingOnClaim {
        inner: Arc<MemoryJobStore>,
        gate: WorkGate,
        closed: AtomicBool,
    }

    impl JobStore for ClosingOnClaim {
        async fn enqueue(&self, job: NewJob, now: SystemTime) -> Result<Enqueued, JobError> {
            self.inner.enqueue(job, now).await
        }
        async fn claim(
            &self,
            queue: &QueueName,
            now: SystemTime,
            lease: Duration,
        ) -> Result<Option<ClaimedJob>, JobError> {
            let claimed = self.inner.claim(queue, now, lease).await?;
            if claimed.is_some() && !self.closed.swap(true, Ordering::SeqCst) {
                self.gate.close();
            }
            Ok(claimed)
        }
        async fn complete(
            &self,
            lease: &LeaseToken,
            now: SystemTime,
        ) -> Result<Completion, JobError> {
            self.inner.complete(lease, now).await
        }
        async fn fail(
            &self,
            lease: &LeaseToken,
            failure: FailureKind,
            next_run_at: SystemTime,
            now: SystemTime,
        ) -> Result<FailureOutcome, JobError> {
            self.inner.fail(lease, failure, next_run_at, now).await
        }
        async fn release(&self, lease: &LeaseToken, now: SystemTime) -> Result<(), JobError> {
            self.inner.release(lease, now).await
        }
        async fn revive(&self, id: &JobId, now: SystemTime) -> Result<bool, JobError> {
            self.inner.revive(id, now).await
        }
        async fn read(&self, id: &JobId) -> Result<Option<Job>, JobError> {
            self.inner.read(id).await
        }
        async fn depth(&self, queue: &QueueName) -> Result<u64, JobError> {
            self.inner.depth(queue).await
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_job_claimed_as_shutdown_begins_is_given_back_not_run() {
        // FR-032: a permit refused after a claim means the lease is released, the handler never
        // runs, and the loop ends — it does not spin re-claiming with a closed gate and burning
        // the job's attempts.
        let registry = Registry::new();
        let entropy: Arc<dyn EntropySource> = Arc::new(FixedEntropy::new([0x55; 16]));
        let inner = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let id = inner.enqueue(new_job(5), clock.now()).await.unwrap().id();
        let gate = WorkGate::new();
        let store = Arc::new(ClosingOnClaim {
            inner: Arc::clone(&inner),
            gate: gate.clone(),
            closed: AtomicBool::new(false),
        });
        let counting = Arc::new(Counting {
            calls: AtomicU32::new(0),
            outcome: Ok(()),
        });
        let worker = Arc::new(
            Worker::new(store, config(), clock.clone(), entropy, &registry)
                .unwrap()
                .register(kind(), counting.clone()),
        );
        let cancel = CancelScope::root();
        // The run ends on its own once the gate refuses: no cancel is needed.
        let report = tokio::time::timeout(
            Duration::from_secs(5),
            Arc::clone(&worker).run(gate.clone(), cancel),
        )
        .await
        .expect("the loop ends when the gate closes");
        assert_eq!(report.claimed, 1);
        assert_eq!(report.aborted, 0);
        assert_eq!(counting.calls.load(Ordering::SeqCst), 0, "the handler ran");
        let job = inner.read(&id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Ready, "the lease was not given back");
        assert_eq!(job.attempts, 1, "the claim counts, and nothing more");
        assert_eq!(counter_total(&registry, "renvor_jobs_released_total"), 1.0);
        assert_eq!(gate.outstanding(), 0);
    }

    #[test]
    fn config_bounds_are_capped() {
        let config = WorkerConfig::new(queue());
        assert_eq!(config.concurrency(), DEFAULT_CONCURRENCY);
        assert!(config.clone().with_concurrency(MAX_CONCURRENCY).is_ok());
        assert!(config.clone().with_concurrency(0).is_err());
        assert!(
            config
                .clone()
                .with_concurrency(MAX_CONCURRENCY + 1)
                .is_err()
        );
        assert!(
            config
                .clone()
                .with_poll_interval(POLL_INTERVAL_RANGE.0)
                .is_ok()
        );
        assert!(
            config
                .clone()
                .with_poll_interval(Duration::from_millis(1))
                .is_err()
        );
        assert!(
            config
                .clone()
                .with_lease(Duration::from_secs(2 * 60 * 60))
                .is_err()
        );
        assert!(
            config
                .clone()
                .with_handler_timeout(Duration::from_millis(1))
                .is_err()
        );
        assert!(config.clone().with_stop_grace(MAX_STOP_GRACE).is_ok());
        assert!(
            config
                .with_stop_grace(MAX_STOP_GRACE + Duration::from_secs(1))
                .is_err()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_failing_handler_is_retried_exactly_max_attempts_times_then_dead_lettered() {
        // SC-003: bounded and observable. Three attempts, three events, then dead.
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn renvor_core::observe::EntropySource> =
            Arc::new(FixedEntropy::new([0x11; 16]));
        let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let handler = Arc::new(Counting {
            calls: AtomicU32::new(0),
            outcome: Err(HandlerError::Retry),
        });
        let client = JobsClient::new(
            Arc::clone(&store),
            clock.clone(),
            super::JobMetrics::register(&registry).unwrap(),
        );
        let id = client.enqueue(new_job(3)).await.unwrap().id();
        let worker = Arc::new(
            Worker::new(
                Arc::clone(&store),
                config(),
                clock.clone(),
                entropy,
                &registry,
            )
            .unwrap()
            .register(kind(), handler.clone()),
        );
        let report = drive(
            worker,
            &store,
            &clock,
            |job| job.state == JobState::Dead,
            id,
        )
        .await;

        assert_eq!(
            handler.calls.load(Ordering::SeqCst),
            3,
            "exactly max_attempts calls"
        );
        let job = store.read(&id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Dead);
        assert_eq!(job.attempts, 3);
        assert_eq!(job.last_failure, Some(FailureKind::HandlerFailed));
        assert_eq!(report.claimed, 3);

        // The metric: two retried, one dead-lettered, for this queue and kind.
        let snapshot = registry.snapshot();
        let attempts = snapshot
            .families
            .iter()
            .find(|family| family.name == "renvor_jobs_attempts_total")
            .unwrap();
        let count = |outcome: &str| {
            attempts
                .series
                .iter()
                .find(|series| {
                    series
                        .labels
                        .iter()
                        .any(|(k, v)| *k == "outcome" && v == outcome)
                })
                .map(|series| match series.value {
                    SeriesValue::Scalar(value) => value,
                    _ => unreachable!(),
                })
                .unwrap_or(0.0)
        };
        assert_eq!(count(AttemptOutcome::Retried.as_str()), 2.0);
        assert_eq!(count(AttemptOutcome::DeadLettered.as_str()), 1.0);
        assert_eq!(count(AttemptOutcome::Completed.as_str()), 0.0);
        let enqueued = snapshot
            .families
            .iter()
            .find(|family| family.name == "renvor_jobs_enqueued_total")
            .unwrap();
        assert_eq!(enqueued.series[0].value, SeriesValue::Scalar(1.0));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn an_abandoning_handler_dead_letters_on_the_first_attempt() {
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn renvor_core::observe::EntropySource> =
            Arc::new(FixedEntropy::new([0x22; 16]));
        let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let handler = Arc::new(Counting {
            calls: AtomicU32::new(0),
            outcome: Err(HandlerError::Abandon),
        });
        let id = store.enqueue(new_job(5), clock.now()).await.unwrap().id();
        let worker = Arc::new(
            Worker::new(
                Arc::clone(&store),
                config(),
                clock.clone(),
                entropy,
                &registry,
            )
            .unwrap()
            .register(kind(), handler.clone()),
        );
        drive(
            worker,
            &store,
            &clock,
            |job| job.state == JobState::Dead,
            id,
        )
        .await;
        assert_eq!(handler.calls.load(Ordering::SeqCst), 1);
        let job = store.read(&id).await.unwrap().unwrap();
        assert_eq!(job.last_failure, Some(FailureKind::Abandoned));
        assert_eq!(job.attempts, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_panicking_handler_is_contained_and_its_payload_never_reaches_the_row() {
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn renvor_core::observe::EntropySource> =
            Arc::new(FixedEntropy::new([0x33; 16]));
        let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let id = store.enqueue(new_job(2), clock.now()).await.unwrap().id();
        let worker = Arc::new(
            Worker::new(
                Arc::clone(&store),
                config(),
                clock.clone(),
                entropy,
                &registry,
            )
            .unwrap()
            .register(kind(), Arc::new(Panicking)),
        );
        // The worker keeps running after the panic — it reaches the second attempt and the
        // dead-letter, which is the containment claim.
        drive(
            worker,
            &store,
            &clock,
            |job| job.state == JobState::Dead,
            id,
        )
        .await;
        let job = store.read(&id).await.unwrap().unwrap();
        assert_eq!(job.last_failure, Some(FailureKind::Panicked));
        assert_eq!(job.attempts, 2);
        // The panic payload is a canary; the row's Debug must not carry it.
        let rendered = format!("{job:?}");
        assert!(
            !rendered.contains("hunter2"),
            "the panic payload reached the row"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_hanging_handler_is_bounded_by_the_timeout() {
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn renvor_core::observe::EntropySource> =
            Arc::new(FixedEntropy::new([0x44; 16]));
        let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let id = store.enqueue(new_job(1), clock.now()).await.unwrap().id();
        // `drive` moves the injected clock 10 s per tick, so the default 60 s lease would expire
        // — and an expired lease at the last attempt dead-letters as `LeaseExpired`. This test is
        // about the HANDLER timeout, so the lease is longer than the clock can run here.
        let config = config().with_lease(Duration::from_secs(60 * 60)).unwrap();
        let worker = Arc::new(
            Worker::new(
                Arc::clone(&store),
                config,
                clock.clone(),
                entropy,
                &registry,
            )
            .unwrap()
            .register(kind(), Arc::new(Hanging)),
        );
        let started = std::time::Instant::now();
        drive(
            worker,
            &store,
            &clock,
            |job| job.state == JobState::Dead,
            id,
        )
        .await;
        let job = store.read(&id).await.unwrap().unwrap();
        assert_eq!(job.last_failure, Some(FailureKind::TimedOut));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the hang was not bounded"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_job_without_a_handler_is_dead_lettered_not_retried_for_ever() {
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn renvor_core::observe::EntropySource> =
            Arc::new(FixedEntropy::new([0x55; 16]));
        let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let id = store.enqueue(new_job(5), clock.now()).await.unwrap().id();
        let worker = Arc::new(
            Worker::new(
                Arc::clone(&store),
                config(),
                clock.clone(),
                entropy,
                &registry,
            )
            .unwrap(),
        );
        drive(
            worker,
            &store,
            &clock,
            |job| job.state == JobState::Dead,
            id,
        )
        .await;
        let job = store.read(&id).await.unwrap().unwrap();
        assert_eq!(job.attempts, 1);
        assert_eq!(job.last_failure, Some(FailureKind::Abandoned));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn shutdown_releases_the_lease_of_a_job_it_had_to_abort() {
        // FR-033: a clean stop leaves no job leased.
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn renvor_core::observe::EntropySource> =
            Arc::new(FixedEntropy::new([0x66; 16]));
        let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let id = store.enqueue(new_job(5), clock.now()).await.unwrap().id();
        let config = config()
            .with_handler_timeout(Duration::from_secs(60))
            .unwrap();
        let worker = Arc::new(
            Worker::new(
                Arc::clone(&store),
                config,
                clock.clone(),
                entropy,
                &registry,
            )
            .unwrap()
            .register(kind(), Arc::new(Hanging)),
        );
        let gate = WorkGate::new();
        let cancel = CancelScope::root();
        let run = tokio::spawn(Arc::clone(&worker).run(gate.clone(), cancel.clone()));
        // Wait until the job is leased and the permit is held.
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if store.read(&id).await.unwrap().unwrap().state == JobState::Leased {
                break;
            }
        }
        assert_eq!(
            gate.outstanding(),
            1,
            "the running job holds a kernel work permit"
        );
        cancel.cancel();
        let report = run.await.unwrap();
        assert_eq!(report.aborted, 1);
        assert_eq!(report.released, 1, "a confirmed release is counted");
        assert_eq!(report.release_failed, 0);
        assert_eq!(report.release_timed_out, 0);
        let job = store.read(&id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Ready, "the lease was released");
        assert_eq!(gate.outstanding(), 0, "the permit was returned");
    }

    /// Leases one hanging job under a worker whose store's `release` is `fault`, cancels the
    /// worker, and returns its report with the registry it recorded to and the row it left.
    ///
    /// Bounded: the run must end within the stop grace (100 ms from [`config`]) plus
    /// [`RELEASE_TIMEOUT`] plus 500 ms, or the test **fails** — a hung release must show as an
    /// assertion, never as a test that looks like a slow machine.
    async fn abort_one_with_release(
        fault: Fault,
    ) -> (WorkerReport, Registry, Arc<MemoryJobStore>, JobId) {
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn EntropySource> = Arc::new(FixedEntropy::new([0x88; 16]));
        let inner = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let id = inner.enqueue(new_job(5), clock.now()).await.unwrap().id();
        let store = Arc::new(Faulty::new(Arc::clone(&inner)).with_release(fault));
        let config = config()
            .with_handler_timeout(Duration::from_secs(60))
            .unwrap();
        let worker = Arc::new(
            Worker::new(store, config, clock.clone(), entropy, &registry)
                .unwrap()
                .register(kind(), Arc::new(Hanging)),
        );
        let cancel = CancelScope::root();
        let run = tokio::spawn(Arc::clone(&worker).run(WorkGate::new(), cancel.clone()));
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if inner.read(&id).await.unwrap().unwrap().state == JobState::Leased {
                break;
            }
        }
        cancel.cancel();
        let bound = Duration::from_millis(100) + RELEASE_TIMEOUT + Duration::from_millis(500);
        let report = tokio::time::timeout(bound, run)
            .await
            .expect("the stop did not end within the grace plus the release bound")
            .unwrap();
        (report, registry, inner, id)
    }

    // Paused time, so the release bound costs 0 real seconds and a release that is NOT bounded
    // fails at the outer timeout instead of hanging the suite.
    #[tokio::test(start_paused = true)]
    async fn a_release_that_hangs_at_stop_is_bounded_and_reported() {
        // FR-025, C-L7, FR-033: a store that never answers `release` at stop must not hold the
        // stop for ever, and a lease the store did not confirm released is never counted as
        // released — in the report or in the metric.
        let (report, registry, inner, id) = abort_one_with_release(Fault::Hangs).await;
        assert_eq!(report.aborted, 1);
        assert_eq!(
            report.release_timed_out, 1,
            "the timed-out release is not reported"
        );
        assert_eq!(
            report.released, 0,
            "an unconfirmed release was counted as released"
        );
        assert_eq!(report.release_failed, 0);
        assert_eq!(
            counter_total(&registry, "renvor_jobs_released_total"),
            0.0,
            "the released metric moved on a release the store never confirmed"
        );
        assert_eq!(
            counter_with(
                &registry,
                "renvor_jobs_store_errors_total",
                "category",
                JobError::TimedOut.as_str()
            ),
            1.0,
            "the timed-out release is not counted as a store error"
        );
        // The truth of the row: still leased, because the store never released it.
        let job = inner.read(&id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Leased);
    }

    #[tokio::test(start_paused = true)]
    async fn a_release_that_fails_at_stop_is_counted_and_never_marked_released() {
        // A refused release is a failure the report carries, not a discarded result.
        let (report, registry, inner, id) =
            abort_one_with_release(Fault::Fails(JobError::Unavailable)).await;
        assert_eq!(report.aborted, 1);
        assert_eq!(
            report.release_failed, 1,
            "the failed release is not reported"
        );
        assert_eq!(
            report.released, 0,
            "a refused release was counted as released"
        );
        assert_eq!(report.release_timed_out, 0);
        assert_eq!(
            counter_total(&registry, "renvor_jobs_released_total"),
            0.0,
            "the released metric moved on a release the store refused"
        );
        assert_eq!(
            counter_with(
                &registry,
                "renvor_jobs_store_errors_total",
                "category",
                JobError::Unavailable.as_str()
            ),
            1.0,
            "the refused release is not counted as a store error"
        );
        let job = inner.read(&id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Leased);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_completed_job_carries_its_trace_context_into_the_execution_span() {
        // FR-038, FR-076: the span exists with the trace fields. Asserted through a recording
        // subscriber in `tests/worker_events.rs`; here the store-side half: the trace round-trips.
        let registry = Registry::new();
        let clock = Arc::new(FixedClock::at_unix_seconds(1_000));
        let entropy: Arc<dyn renvor_core::observe::EntropySource> =
            Arc::new(FixedEntropy::new([0x77; 16]));
        let store = Arc::new(MemoryJobStore::new(JobBounds::new(), Arc::clone(&entropy)));
        let trace = TraceContext::parse(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            None,
        )
        .unwrap();
        let id = store
            .enqueue(new_job(1).with_trace(trace.clone()), clock.now())
            .await
            .unwrap()
            .id();
        let handler = Arc::new(Counting {
            calls: AtomicU32::new(0),
            outcome: Ok(()),
        });
        let worker = Arc::new(
            Worker::new(
                Arc::clone(&store),
                config(),
                clock.clone(),
                entropy,
                &registry,
            )
            .unwrap()
            .register(kind(), handler),
        );
        drive(
            worker,
            &store,
            &clock,
            |job| job.state == JobState::Completed,
            id,
        )
        .await;
        let job = store.read(&id).await.unwrap().unwrap();
        assert_eq!(job.state, JobState::Completed);
        assert_eq!(job.trace.as_ref(), Some(&trace));
    }
}
