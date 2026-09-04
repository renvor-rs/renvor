//! The worker as a kernel provider.
//!
//! # What Boot and Stop mean for a worker
//!
//! Boot first proves the store answers — one bounded `depth` probe under
//! [`STORE_PROBE_TIMEOUT`](crate::worker::STORE_PROBE_TIMEOUT) — and fails with
//! [`WorkerBootError::StoreNotAnswering`] if it does not: bad credentials, a missing schema, a
//! permission failure, an unreachable backend, or a hang all fail Boot with a closed category
//! (FR-012). Only then does it spawn the run loop with the application's own `WorkGate` and a
//! child of its cancellation scope, and report ready. A worker that booted Ready over a store it
//! could not use would warn "claim failed" every poll interval for ever while readiness said the
//! opposite; nothing is spawned and readiness is never Ready when the probe fails.
//!
//! Stop cancels the scope and waits for the loop to finish — which the worker bounds by its stop
//! grace, aborting what is left and releasing each lease under
//! [`RELEASE_TIMEOUT`](crate::worker::RELEASE_TIMEOUT) (FR-033) — so a provider deadline is
//! never the thing that ends it. A release the store refused or did not answer is **reported**:
//! Stop returns [`WorkerBootError::LeasesNotReleased`] with the counts, and the kernel records
//! an unclean stop naming this provider (C-L2: a forced close is reported, never swallowed). The
//! report is kept on the handle either way.
//!
//! # It depends on what the application says it depends on
//!
//! A worker over a database-backed store should start after the database provider and stop
//! before it. That ordering is declared, not inferred: [`JobsWorkerProvider::depends_on`] names
//! the capability, and the kernel's resolver orders Boot and reverses Stop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use renvor_core::cancel::CancelScope;
use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::ProviderId;
use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};

use crate::job::JobError;
use crate::store::JobStore;
use crate::worker::{Worker, WorkerReport};

/// The capability a running worker offers.
pub const JOBS_CAPABILITY: &str = "jobs";

/// The capability identifier as a value.
#[must_use]
pub fn jobs_capability() -> CapabilityId {
    CapabilityId::new(JOBS_CAPABILITY)
}

/// Why the worker provider could not boot or stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum WorkerBootError {
    /// The provider was initialised twice, which the kernel prevents.
    #[error("jobs worker boot failed: the provider was initialised twice")]
    AlreadyBooted,
    /// The run loop task could not be joined at Stop.
    #[error("jobs worker stop failed: the run loop did not finish")]
    RunLoopDidNotFinish,
    /// The job store did not answer the readiness probe at Boot: bad credentials, a missing
    /// schema, a permission failure, an unreachable backend, or a hang past
    /// [`STORE_PROBE_TIMEOUT`](crate::worker::STORE_PROBE_TIMEOUT), which is reported as
    /// [`JobError::TimedOut`]. Closed: the category names the fault, never the store's own
    /// message.
    #[error(
        "jobs worker boot failed: the job store did not answer the readiness probe ({})",
        .0.as_str()
    )]
    StoreNotAnswering(JobError),
    /// Stop aborted running jobs whose leases the store did not then confirm released. Reported
    /// rather than swallowed (FR-012, C-L2: a forced close is reported): the kernel records an
    /// unclean stop naming this provider, and the counts say what is left leased.
    #[error(
        "jobs worker stop failed: {failed} lease release(s) failed and {timed_out} timed out; \
         those jobs stay leased until the lease expires and is reclaimed"
    )]
    LeasesNotReleased {
        /// Releases the store refused.
        failed: u64,
        /// Releases that did not answer within
        /// [`RELEASE_TIMEOUT`](crate::worker::RELEASE_TIMEOUT).
        timed_out: u64,
    },
}

/// The worker as a provider.
pub struct JobsWorkerProvider<S> {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    dependencies: Vec<CapabilityId>,
    worker: Mutex<Option<Arc<Worker<S>>>>,
    running: Mutex<Option<Running>>,
    handle: WorkerHandle,
}

/// A shared view of the worker after the provider has been handed to the application: whether
/// it is running, and the report of its finished run once Stop has completed.
///
/// The application takes the provider by value, so this is the only way to read the outcome of
/// a run afterwards — the number of jobs claimed, the number aborted at the stop grace, and how
/// many of their leases were released, refused, or not answered. The report is stored before
/// Stop reports a failed release, so it is there whether or not the stop was clean.
#[derive(Clone, Debug, Default)]
pub struct WorkerHandle {
    ready: Arc<AtomicBool>,
    last_report: Arc<Mutex<Option<WorkerReport>>>,
}

impl WorkerHandle {
    /// Whether the run loop is running: true from Boot to Stop.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// The report of the finished run, if Stop has completed.
    #[must_use]
    pub fn report(&self) -> Option<WorkerReport> {
        self.last_report
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

struct Running {
    cancel: CancelScope,
    task: tokio::task::JoinHandle<WorkerReport>,
}

impl<S> core::fmt::Debug for JobsWorkerProvider<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("JobsWorkerProvider")
            .field("id", &self.id)
            .field("running", &self.handle.is_running())
            .finish_non_exhaustive()
    }
}

impl<S: JobStore + 'static> JobsWorkerProvider<S> {
    /// Declares a provider that runs `worker` from Boot to Stop.
    #[must_use]
    pub fn new(id: ProviderId, worker: Worker<S>) -> Self {
        Self {
            id,
            provides: vec![jobs_capability()],
            dependencies: Vec::new(),
            worker: Mutex::new(Some(Arc::new(worker))),
            running: Mutex::new(None),
            handle: WorkerHandle::default(),
        }
    }

    /// Orders this provider after the provider offering `capability`.
    #[must_use]
    pub fn depends_on(mut self, capability: CapabilityId) -> Self {
        self.dependencies.push(capability);
        self
    }

    /// A handle to keep after the provider is given to the application.
    #[must_use]
    pub fn handle(&self) -> WorkerHandle {
        self.handle.clone()
    }
}

impl<S: JobStore + 'static> Provider for JobsWorkerProvider<S> {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn dependencies(&self) -> &[CapabilityId] {
        &self.dependencies
    }

    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            let worker = self
                .worker
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .ok_or_else(|| Box::new(WorkerBootError::AlreadyBooted) as BoxedCause)?;
            // Prove the store answers BEFORE anything is spawned or registered (FR-012). A
            // failed probe returns here with nothing running and readiness never Ready.
            worker.probe_store().await.map_err(|error| {
                Box::new(WorkerBootError::StoreNotAnswering(error)) as BoxedCause
            })?;
            let gate = context.work().clone();
            let cancel = context.cancel().child("jobs-worker");
            let task = tokio::spawn(Arc::clone(&worker).run(gate, cancel.clone()));
            *self.running.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(Running { cancel, task });
            context.register_readiness(Arc::new(WorkerReadiness {
                name: self.id.as_str().to_owned(),
                ready: Arc::clone(&self.handle.ready),
            }));
            self.handle.ready.store(true, Ordering::Release);
            Ok(())
        })
    }

    fn stop(&self) -> ProviderFuture<'_> {
        Box::pin(async move {
            self.handle.ready.store(false, Ordering::Release);
            let running = self
                .running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            let Some(Running { cancel, task }) = running else {
                return Ok(());
            };
            cancel.cancel();
            let report = task
                .await
                .map_err(|_| Box::new(WorkerBootError::RunLoopDidNotFinish) as BoxedCause)?;
            let (failed, timed_out) = (report.release_failed, report.release_timed_out);
            *self
                .handle
                .last_report
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(report);
            // A forced close is reported, never swallowed (FR-012, C-L2): leases the store did
            // not confirm released become the kernel's unclean stop, naming this provider, with
            // the counts. The report is kept first so the handle tells the same story.
            if failed > 0 || timed_out > 0 {
                return Err(
                    Box::new(WorkerBootError::LeasesNotReleased { failed, timed_out })
                        as BoxedCause,
                );
            }
            Ok(())
        })
    }
}

/// Readiness of the worker: an atomic flipped by Boot and Stop.
#[derive(Debug)]
struct WorkerReadiness {
    name: String,
    ready: Arc<AtomicBool>,
}

impl ReadinessContributor for WorkerReadiness {
    fn name(&self) -> &str {
        &self.name
    }

    fn readiness(&self) -> Readiness {
        if self.ready.load(Ordering::Acquire) {
            Readiness::Ready
        } else {
            Readiness::NotReady
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;
    use std::time::Duration;

    use renvor_core::clock::SystemClock;
    use renvor_core::observe::OsEntropy;
    use renvor_core::observe::metrics::Registry;
    use renvor_core::{ApplicationBuilder, ErrorCategory, Readiness};

    use super::*;
    use crate::job::{Job, JobBounds, JobKind, JobPayload, JobState, NewJob, QueueName};
    use crate::memory::MemoryJobStore;
    use crate::worker::faulty::{Fault, Faulty};
    use crate::worker::{HandlerFuture, JobHandler, STORE_PROBE_TIMEOUT, Worker, WorkerConfig};

    /// A provider that needs `jobs` and offers nothing.
    struct Consumer {
        id: ProviderId,
        needs: Vec<CapabilityId>,
    }

    impl Provider for Consumer {
        fn id(&self) -> &ProviderId {
            &self.id
        }
        fn provides(&self) -> &[CapabilityId] {
            &[]
        }
        fn dependencies(&self) -> &[CapabilityId] {
            &self.needs
        }
        fn initialise<'a>(&'a self, _: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
            Box::pin(async { Ok(()) })
        }
        fn stop(&self) -> ProviderFuture<'_> {
            Box::pin(async { Ok(()) })
        }
    }

    struct Done(AtomicU32);

    impl JobHandler for Done {
        fn handle(&self, _: Job, _: CancelScope) -> HandlerFuture<'_> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }
    }

    /// Never finishes, so a stop has to abort it and release its lease.
    struct Hang;

    impl JobHandler for Hang {
        fn handle(&self, _: Job, _: CancelScope) -> HandlerFuture<'_> {
            Box::pin(std::future::pending())
        }
    }

    fn queue() -> QueueName {
        QueueName::new("boot").unwrap()
    }

    fn config() -> WorkerConfig {
        WorkerConfig::new(queue())
            .with_poll_interval(Duration::from_millis(10))
            .unwrap()
            .with_stop_grace(Duration::from_millis(200))
            .unwrap()
    }

    fn worker<S: JobStore + 'static>(store: Arc<S>, done: Arc<Done>) -> Worker<S> {
        Worker::new(
            store,
            config(),
            Arc::new(SystemClock::new()),
            Arc::new(OsEntropy::new()),
            &Registry::new(),
        )
        .unwrap()
        .register(JobKind::new("done").unwrap(), done)
    }

    /// A worker whose only handler hangs, with a handler timeout longer than any test here, so
    /// the stop grace — not the timeout — is what ends the job.
    fn hanging_worker<S: JobStore + 'static>(store: Arc<S>) -> Worker<S> {
        Worker::new(
            store,
            config()
                .with_handler_timeout(Duration::from_secs(60))
                .unwrap(),
            Arc::new(SystemClock::new()),
            Arc::new(OsEntropy::new()),
            &Registry::new(),
        )
        .unwrap()
        .register(JobKind::new("hang").unwrap(), Arc::new(Hang))
    }

    fn store() -> Arc<MemoryJobStore> {
        Arc::new(MemoryJobStore::new(
            JobBounds::new(),
            Arc::new(OsEntropy::new()),
        ))
    }

    async fn enqueue(store: &MemoryJobStore, kind: &str) -> crate::job::JobId {
        store
            .enqueue(
                NewJob::new(
                    queue(),
                    JobKind::new(kind).unwrap(),
                    JobPayload::within(b"x".to_vec(), &JobBounds::new()).unwrap(),
                ),
                std::time::SystemTime::now(),
            )
            .await
            .unwrap()
            .id()
    }

    /// Waits until the job is leased — the handler is running and holds a permit.
    async fn until_leased(store: &MemoryJobStore, id: &crate::job::JobId) {
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if store.read(id).await.unwrap().unwrap().state == JobState::Leased {
                return;
            }
        }
        panic!("the job was never leased");
    }

    /// `error` and every source under it, rendered and joined: the kernel preserves a provider's
    /// cause under `ProviderInit`/`ProviderStop` (C-E2) rather than flattening it into the
    /// message, so an assertion about the cause has to walk the chain.
    fn chain(error: &dyn std::error::Error) -> String {
        let mut rendered = error.to_string();
        let mut next = error.source();
        while let Some(cause) = next {
            rendered.push_str(": ");
            rendered.push_str(&cause.to_string());
            next = cause.source();
        }
        rendered
    }

    #[test]
    fn a_consumer_with_no_jobs_provider_fails_at_register_naming_both_ends() {
        // SC-001: the missing capability is refused before anything boots.
        let error = ApplicationBuilder::new()
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-jobs"),
                needs: vec![jobs_capability()],
            }))
            .build()
            .expect_err("no provider offers `jobs`");
        let kernel = error.kernel().expect("a kernel error");
        assert_eq!(kernel.category(), ErrorCategory::DependencyMissing);
        let rendered = kernel.to_string();
        assert!(
            rendered.contains("needs-jobs"),
            "the dependent is not named"
        );
        assert!(
            rendered.contains(JOBS_CAPABILITY),
            "the capability is not named"
        );
    }

    #[test]
    fn depends_on_is_a_real_dependency_the_kernel_enforces() {
        // A worker over a database that is not there must not boot and claim rows from nothing.
        let provider = JobsWorkerProvider::new(
            ProviderId::new("jobs-worker"),
            worker(store(), Arc::new(Done(AtomicU32::new(0)))),
        )
        .depends_on(CapabilityId::new("database"));
        assert_eq!(provider.dependencies(), &[CapabilityId::new("database")]);
        let error = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .build()
            .expect_err("nothing offers `database`");
        let rendered = error.kernel().expect("a kernel error").to_string();
        assert!(rendered.contains("jobs-worker") && rendered.contains("database"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_worker_boots_ready_runs_a_job_and_stop_returns_its_report() {
        // POSITIVE CONTROL for the two refusals above, and the provider's own contract.
        let store = store();
        let done = Arc::new(Done(AtomicU32::new(0)));
        let id = store
            .enqueue(
                NewJob::new(
                    queue(),
                    JobKind::new("done").unwrap(),
                    JobPayload::within(b"x".to_vec(), &JobBounds::new()).unwrap(),
                ),
                std::time::SystemTime::now(),
            )
            .await
            .unwrap()
            .id();
        let provider = JobsWorkerProvider::new(
            ProviderId::new("jobs"),
            worker(Arc::clone(&store), Arc::clone(&done)),
        );
        let handle = provider.handle();
        assert!(!handle.is_running(), "not running before Boot");
        assert!(handle.report().is_none());

        let mut application = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-jobs"),
                needs: vec![jobs_capability()],
            }))
            .build()
            .expect("register succeeds")
            .boot()
            .await
            .expect("boot reaches Ready");
        assert!(handle.is_running());
        let readiness = application.health().readiness();
        let verdict = readiness
            .contributors
            .iter()
            .find(|verdict| verdict.name == "jobs")
            .expect("the worker registered a readiness contributor");
        assert_eq!(verdict.readiness, Readiness::Ready);

        // The job runs under the application's own gate.
        let mut settled = false;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if store.read(&id).await.unwrap().unwrap().state == JobState::Completed {
                settled = true;
                break;
            }
        }
        assert!(settled, "the job did not complete under the booted worker");
        assert_eq!(done.0.load(Ordering::SeqCst), 1);

        // Stop ends the loop, the provider reports it, and readiness goes negative.
        let report = application.shutdown().await;
        assert!(report.stop().is_clean(), "stop failed: {report:?}");
        assert!(!handle.is_running());
        let run = handle.report().expect("the finished run is reported");
        assert_eq!(run.claimed, 1);
        assert_eq!(run.aborted, 0);
        let verdict = application
            .health()
            .readiness()
            .contributors
            .iter()
            .find(|verdict| verdict.name == "jobs")
            .map(|verdict| verdict.readiness);
        assert_eq!(verdict, Some(Readiness::NotReady));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_store_that_does_not_answer_fails_boot_and_never_starts_the_loop() {
        // FR-012, SC-001: bad credentials, a missing schema, a permission failure, or an
        // unreachable backend must fail Boot with a closed category — not boot Ready and log
        // "claim failed; the worker will poll again" for ever.
        let inner = store();
        let faulty = Arc::new(
            Faulty::new(Arc::clone(&inner)).with_depth(Fault::Fails(JobError::Unavailable)),
        );
        let provider = JobsWorkerProvider::new(
            ProviderId::new("jobs-worker"),
            worker(Arc::clone(&faulty), Arc::new(Done(AtomicU32::new(0)))),
        );
        let handle = provider.handle();
        let failure = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .build()
            .expect("register succeeds")
            .boot()
            .await
            .expect_err("a store that does not answer must fail Boot");
        assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);
        let rendered = chain(failure.origin());
        assert!(
            rendered.contains("jobs-worker"),
            "the provider is not named"
        );
        assert!(
            rendered.contains("store did not answer"),
            "the probe is not named"
        );
        assert!(
            rendered.contains("unavailable"),
            "the category is not named"
        );
        assert_eq!(faulty.claims(), 0, "the run loop started");
        assert!(
            !handle.is_running(),
            "the worker reports running after a failed Boot"
        );
    }

    // Paused time: the probe bound costs 0 real seconds, and the elapsed check below is exact.
    #[tokio::test(start_paused = true)]
    async fn a_hanging_store_fails_boot_within_the_probe_bound() {
        // The probe is bounded on its own (FR-025, C-L7): a store that accepts the connection and
        // never answers fails Boot as `timed_out` within STORE_PROBE_TIMEOUT — under the kernel's
        // provider deadline, so the failure names the dependency, not only the provider.
        let inner = store();
        let faulty = Arc::new(Faulty::new(Arc::clone(&inner)).with_depth(Fault::Hangs));
        let provider = JobsWorkerProvider::new(
            ProviderId::new("jobs-worker"),
            worker(Arc::clone(&faulty), Arc::new(Done(AtomicU32::new(0)))),
        );
        let handle = provider.handle();
        let started = tokio::time::Instant::now();
        let failure = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .build()
            .expect("register succeeds")
            .boot()
            .await
            .expect_err("a hanging store must fail Boot");
        assert!(
            started.elapsed() <= STORE_PROBE_TIMEOUT + Duration::from_secs(1),
            "the probe was not bounded by its own timeout"
        );
        assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);
        let rendered = chain(failure.origin());
        assert!(
            rendered.contains(JobError::TimedOut.as_str()),
            "the timeout category is not named"
        );
        assert_eq!(faulty.claims(), 0, "the run loop started");
        assert!(!handle.is_running());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_provider_reports_leases_it_could_not_release() {
        // FR-012, C-L2: a forced close is reported, never swallowed. A release the store refused
        // at stop reaches the kernel's stop report as this provider's failure, with the counts.
        let inner = store();
        let id = enqueue(&inner, "hang").await;
        let faulty = Arc::new(
            Faulty::new(Arc::clone(&inner)).with_release(Fault::Fails(JobError::Unavailable)),
        );
        let provider =
            JobsWorkerProvider::new(ProviderId::new("jobs"), hanging_worker(Arc::clone(&faulty)));
        let handle = provider.handle();
        let mut application = ApplicationBuilder::new()
            // The hanging job holds a kernel permit, so the drain runs to its budget by design;
            // this test is about Stop, so the budget is short.
            .with_drain_budget(Duration::from_millis(200))
            .with_provider(Box::new(provider))
            .build()
            .expect("register succeeds")
            .boot()
            .await
            .expect("boot reaches Ready");
        until_leased(&inner, &id).await;

        let report = application.shutdown().await;
        assert!(
            !report.stop().is_clean(),
            "a failed release was swallowed by Stop"
        );
        let rendered = report
            .stop()
            .failures()
            .iter()
            .map(|failure| chain(failure))
            .collect::<Vec<_>>()
            .join("; ");
        assert!(rendered.contains("`jobs`"), "the provider is not named");
        assert!(
            rendered.contains("lease release"),
            "the stop error does not name the release"
        );
        assert!(
            rendered.contains("1 lease release(s) failed and 0 timed out"),
            "the counts are not reported"
        );
        let run = handle
            .report()
            .expect("the report is kept even when Stop fails");
        assert_eq!(run.aborted, 1);
        assert_eq!(run.release_failed, 1);
        assert_eq!(run.released, 0);
        assert_eq!(
            inner.read(&id).await.unwrap().unwrap().state,
            JobState::Leased,
            "the row disagrees with the report"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_clean_stop_reports_every_aborted_lease_released() {
        // POSITIVE CONTROL for the test above: when the store answers, a stop that had to abort a
        // job releases its lease, counts the confirmed release, and the kernel's stop is clean.
        let inner = store();
        let id = enqueue(&inner, "hang").await;
        let provider =
            JobsWorkerProvider::new(ProviderId::new("jobs"), hanging_worker(Arc::clone(&inner)));
        let handle = provider.handle();
        let mut application = ApplicationBuilder::new()
            .with_drain_budget(Duration::from_millis(200))
            .with_provider(Box::new(provider))
            .build()
            .expect("register succeeds")
            .boot()
            .await
            .expect("boot reaches Ready");
        until_leased(&inner, &id).await;

        let report = application.shutdown().await;
        assert!(
            report.stop().is_clean(),
            "a clean stop was reported unclean"
        );
        let run = handle.report().expect("the finished run is reported");
        assert_eq!(run.aborted, 1);
        assert_eq!(
            run.released, run.aborted,
            "a confirmed release is not counted"
        );
        assert_eq!(run.release_failed, 0);
        assert_eq!(run.release_timed_out, 0);
        assert_eq!(
            inner.read(&id).await.unwrap().unwrap().state,
            JobState::Ready,
            "the lease was not given back"
        );
    }
}
