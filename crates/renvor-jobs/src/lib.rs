//! Durable background jobs: the port, the value types, the substitute, and the worker.
//!
//! # Where the durable store is
//!
//! Not here. [`JobStore`] is a port; [`MemoryJobStore`] is the deterministic substitute; the
//! durable implementations live in `renvor-sqlx` and `renvor-seaorm` behind their `jobs`
//! features, on the connection the application already owns (ADR-0032). That is what lets a
//! MySQL application choose jobs without resolving a PostgreSQL crate, and this crate names no
//! driver — asserted by `xtask` step 7.
//!
//! # What is bounded
//!
//! Payloads, queue depth, attempts, leases, handler time, worker concurrency, poll interval,
//! stop grace, and the retry schedule — every one with a default and a hard cap in [`job`] and
//! [`worker`]. Retries are scheduled by the kernel's pure policy and every attempt is one
//! structured event and one counter increment.
//!
//! # Configuration
//!
//! [`config::JobsSection`] is the typed `[jobs]` section: decoded by `renvor-config`, defaulted,
//! capped, and refused at **Validate** — naming the key, the constraint, and the layer — before
//! any provider boots (FR-011). It yields a [`JobBounds`] and a [`WorkerConfig`]; the store and
//! the handlers are code.
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** See the `renvor` facade documentation.

/// The typed `[jobs]` configuration section (FR-011): defaults, hard caps, and a Validate-phase
/// refusal naming key, constraint, and layer; yields the store's bounds and the worker's
/// configuration.
pub mod config;
pub mod job;
pub mod memory;
pub mod provider;
pub mod store;
pub mod worker;

pub use config::{JobsSection, JobsSettings};
pub use job::{
    ClaimedJob, Completion, DEFAULT_HANDLER_TIMEOUT, DEFAULT_LEASE, DEFAULT_MAX_ATTEMPTS,
    DEFAULT_MAX_PAYLOAD_BYTES, DEFAULT_MAX_QUEUE_DEPTH, Enqueued, FailureKind, FailureOutcome,
    IdempotencyKey, Job, JobBounds, JobError, JobId, JobKind, JobPayload, JobRefusal, JobState,
    LeaseToken, MAX_ATTEMPTS_CAP, MAX_HANDLER_TIMEOUT_CAP, MAX_IDEMPOTENCY_KEY_BYTES,
    MAX_IDENTIFIER_BYTES, MAX_LEASE_CAP, MAX_PAYLOAD_BYTES_CAP, NewJob, QueueName, RECLAIM_BATCH,
};
pub use memory::MemoryJobStore;
pub use provider::{JOBS_CAPABILITY, JobsWorkerProvider, WorkerBootError, jobs_capability};
pub use store::JobStore;
pub use worker::{
    DEFAULT_CONCURRENCY, DEFAULT_POLL_INTERVAL, DEFAULT_STOP_GRACE, HandlerError, HandlerFuture,
    JOB_SPAN_NAME, JOBS_EVENT_TARGET, JobHandler, JobMetrics, JobsClient, MAX_CONCURRENCY,
    MAX_STOP_GRACE, POLL_INTERVAL_RANGE, RELEASE_TIMEOUT, STORE_PROBE_TIMEOUT, Worker,
    WorkerConfig, WorkerReport,
};
