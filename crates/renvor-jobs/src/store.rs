//! The job-store port: seven transitions, each atomic where it lives.
//!
//! # The store decides; the worker computes
//!
//! The store owns the invariants a database can enforce: one row per idempotency key, one claim
//! per ready job, one completion per lease, attempts against `max_attempts`. The worker owns the
//! policy: when to retry (through the kernel's `RetryPolicy`), how many jobs at once, how long a
//! handler may run. So `fail` takes the **next run time** the worker computed, and the store
//! answers with whether it rescheduled or dead-lettered — the arithmetic on `attempts` is the
//! store's, because only the store sees the row.
//!
//! # `now` is a parameter
//!
//! Every method takes the instant it should act at. The memory substitute and the four database
//! rows then agree on time by construction, and a test moves a `FixedClock` rather than waiting.
//! A database row **binds** `now` as a parameter and never reads its server clock (FR-036), so
//! two engines and a test double cannot disagree about whether `run_at` has arrived.
//!
//! # Leases, and why a completed job keeps its token
//!
//! `complete` and `fail` are addressed by the [`LeaseToken`] the claim returned. A completed job
//! **keeps** its token so that a second `complete` with the same token can answer
//! [`Completion::AlreadyCompleted`] (FR-039) instead of [`JobError::LeaseNotHeld`]; `release`
//! and reclaim clear it, so a token that was given back or timed out is refused. That is the
//! whole idempotency story for the transitions, and it costs one column.

use core::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::job::{
    ClaimedJob, Completion, Enqueued, FailureKind, FailureOutcome, Job, JobError, JobId,
    LeaseToken, NewJob, QueueName,
};

/// The job-store port.
///
/// Native `async fn`, so generic-only; the worker and the providers are generic over the store,
/// as the persistence ports are.
pub trait JobStore: Send + Sync {
    /// Stores `job`, or reports the existing job that holds its idempotency key.
    ///
    /// # Errors
    ///
    /// [`JobError::QueueFull`] when the queue is at its depth bound (counted in the same
    /// transaction as the insert; see the contract for the exact guarantee under concurrency),
    /// [`JobError::Unavailable`] or [`JobError::TimedOut`] when the store fails.
    fn enqueue(
        &self,
        job: NewJob,
        now: SystemTime,
    ) -> impl Future<Output = Result<Enqueued, JobError>> + Send;

    /// Claims the earliest ready job in `queue` whose `run_at` has arrived, leasing it for
    /// `lease`. Reclaims expired leases in `queue` first (bounded by [`crate::job::RECLAIM_BATCH`]),
    /// counting the lost attempt with [`FailureKind::LeaseExpired`].
    ///
    /// Atomic across concurrent claimants: one ready job is claimed by exactly one caller.
    fn claim(
        &self,
        queue: &QueueName,
        now: SystemTime,
        lease: Duration,
    ) -> impl Future<Output = Result<Option<ClaimedJob>, JobError>> + Send;

    /// Marks the job under `lease` completed. Idempotent for the same lease.
    ///
    /// # Errors
    ///
    /// [`JobError::LeaseNotHeld`] when the token was released, reclaimed, or never issued.
    fn complete(
        &self,
        lease: &LeaseToken,
        now: SystemTime,
    ) -> impl Future<Output = Result<Completion, JobError>> + Send;

    /// Records a failed attempt under `lease`: reschedules at `next_run_at` if attempts remain
    /// and the failure is not terminal, otherwise dead-letters.
    ///
    /// # Errors
    ///
    /// [`JobError::LeaseNotHeld`] as for [`Self::complete`].
    fn fail(
        &self,
        lease: &LeaseToken,
        failure: FailureKind,
        next_run_at: SystemTime,
        now: SystemTime,
    ) -> impl Future<Output = Result<FailureOutcome, JobError>> + Send;

    /// Returns the job under `lease` to the ready state without recording a failure — the
    /// worker is stopping and could not finish. The attempt already counted stays counted:
    /// the handler may have had side effects, and a released job that costs nothing is a job
    /// that can loop for ever through clean shutdowns.
    ///
    /// # Errors
    ///
    /// [`JobError::LeaseNotHeld`] as for [`Self::complete`].
    fn release(
        &self,
        lease: &LeaseToken,
        now: SystemTime,
    ) -> impl Future<Output = Result<(), JobError>> + Send;

    /// Puts a dead job back in the ready state with its attempts reset — the explicit,
    /// application-driven re-enqueue FR-029 requires. Returns `false` if the job is not dead.
    fn revive(
        &self,
        id: &JobId,
        now: SystemTime,
    ) -> impl Future<Output = Result<bool, JobError>> + Send;

    /// Reads one job.
    fn read(&self, id: &JobId) -> impl Future<Output = Result<Option<Job>, JobError>> + Send;

    /// How many jobs in `queue` are ready or leased — the quantity the depth bound counts.
    fn depth(&self, queue: &QueueName) -> impl Future<Output = Result<u64, JobError>> + Send;
}

/// A shared store is itself a store.
impl<T> JobStore for Arc<T>
where
    T: JobStore + ?Sized,
{
    fn enqueue(
        &self,
        job: NewJob,
        now: SystemTime,
    ) -> impl Future<Output = Result<Enqueued, JobError>> + Send {
        (**self).enqueue(job, now)
    }

    fn claim(
        &self,
        queue: &QueueName,
        now: SystemTime,
        lease: Duration,
    ) -> impl Future<Output = Result<Option<ClaimedJob>, JobError>> + Send {
        (**self).claim(queue, now, lease)
    }

    fn complete(
        &self,
        lease: &LeaseToken,
        now: SystemTime,
    ) -> impl Future<Output = Result<Completion, JobError>> + Send {
        (**self).complete(lease, now)
    }

    fn fail(
        &self,
        lease: &LeaseToken,
        failure: FailureKind,
        next_run_at: SystemTime,
        now: SystemTime,
    ) -> impl Future<Output = Result<FailureOutcome, JobError>> + Send {
        (**self).fail(lease, failure, next_run_at, now)
    }

    fn release(
        &self,
        lease: &LeaseToken,
        now: SystemTime,
    ) -> impl Future<Output = Result<(), JobError>> + Send {
        (**self).release(lease, now)
    }

    fn revive(
        &self,
        id: &JobId,
        now: SystemTime,
    ) -> impl Future<Output = Result<bool, JobError>> + Send {
        (**self).revive(id, now)
    }

    fn read(&self, id: &JobId) -> impl Future<Output = Result<Option<Job>, JobError>> + Send {
        (**self).read(id)
    }

    fn depth(&self, queue: &QueueName) -> impl Future<Output = Result<u64, JobError>> + Send {
        (**self).depth(queue)
    }
}
