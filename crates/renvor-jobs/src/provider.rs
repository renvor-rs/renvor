//! The worker as a kernel provider.
//!
//! # What Boot and Stop mean for a worker
//!
//! Boot spawns the run loop with the application's own `WorkGate` and a child of its
//! cancellation scope, and reports ready once the loop is running. Stop cancels the scope and
//! waits for the loop to finish — which the worker bounds by its stop grace, aborting and
//! releasing what is left (FR-033) — so a provider deadline is never the thing that ends it.
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
/// a run afterwards — the number of jobs claimed and the number aborted at the stop grace.
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
            *self
                .handle
                .last_report
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(report);
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
    use crate::worker::{HandlerFuture, JobHandler, Worker, WorkerConfig};

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

    fn queue() -> QueueName {
        QueueName::new("boot").unwrap()
    }

    fn worker(store: Arc<MemoryJobStore>, done: Arc<Done>) -> Worker<MemoryJobStore> {
        let config = WorkerConfig::new(queue())
            .with_poll_interval(Duration::from_millis(10))
            .unwrap()
            .with_stop_grace(Duration::from_millis(200))
            .unwrap();
        Worker::new(
            store,
            config,
            Arc::new(SystemClock::new()),
            Arc::new(OsEntropy::new()),
            &Registry::new(),
        )
        .unwrap()
        .register(JobKind::new("done").unwrap(), done)
    }

    fn store() -> Arc<MemoryJobStore> {
        Arc::new(MemoryJobStore::new(
            JobBounds::new(),
            Arc::new(OsEntropy::new()),
        ))
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
}
