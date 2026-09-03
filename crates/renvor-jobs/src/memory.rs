//! The deterministic substitute: every store transition under one lock, with `now` supplied.
//!
//! # The same contract as the rows
//!
//! This store is exercised by the same shared suite (`renvor_testkit::jobs`) as the four database
//! rows, so it cannot drift from what they promise (FR-040). What it does not have is durability,
//! which is the difference an author chooses visibly by constructing it.
//!
//! # One lock, so a transition is atomic
//!
//! A `Mutex` around the whole table makes every transition atomic in the sense the rows are —
//! claim, complete, fail, release — and makes the barrier races in the shared suite meaningful
//! rather than trivially serialised: the racers still contend, and exactly one wins.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use renvor_core::observe::entropy::EntropySource;

use crate::job::{
    ClaimedJob, Completion, Enqueued, FailureKind, FailureOutcome, IdempotencyKey, Job, JobBounds,
    JobError, JobId, JobState, LeaseToken, NewJob, QueueName, RECLAIM_BATCH,
};
use crate::store::JobStore;

/// One stored job with its lease.
#[derive(Clone, Debug)]
struct Row {
    job: Job,
    lease: Option<LeaseToken>,
    lease_expires_at: Option<SystemTime>,
}

#[derive(Debug, Default)]
struct Table {
    rows: HashMap<JobId, Row>,
    /// `(queue, key)` → job, the unique constraint.
    keys: HashMap<(QueueName, IdempotencyKey), JobId>,
    /// Claim order: `(run_at, id)` → id, for ready jobs.
    ready: BTreeMap<(SystemTime, JobId), JobId>,
}

/// An in-memory job store.
#[derive(Debug)]
pub struct MemoryJobStore {
    bounds: JobBounds,
    entropy: Arc<dyn EntropySource>,
    table: Mutex<Table>,
}

impl MemoryJobStore {
    /// Creates an empty store drawing identifiers and lease tokens from `entropy`.
    #[must_use]
    pub fn new(bounds: JobBounds, entropy: Arc<dyn EntropySource>) -> Self {
        Self {
            bounds,
            entropy,
            table: Mutex::new(Table::default()),
        }
    }

    /// The bounds this store validates against.
    #[must_use]
    pub const fn bounds(&self) -> &JobBounds {
        &self.bounds
    }

    /// How many jobs are held in any state.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .rows
            .len()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes every job. For a fixture's reset between contract assertions.
    pub fn clear(&self) {
        *self.table.lock().unwrap_or_else(PoisonError::into_inner) = Table::default();
    }

    /// Returns leased jobs whose lease expired to the ready state, counting the lost attempt.
    fn reclaim_expired(table: &mut Table, queue: &QueueName, now: SystemTime) {
        let expired: Vec<JobId> = table
            .rows
            .values()
            .filter(|row| {
                row.job.queue == *queue
                    && row.job.state == JobState::Leased
                    && row.lease_expires_at.is_some_and(|at| at <= now)
            })
            .take(RECLAIM_BATCH as usize)
            .map(|row| row.job.id)
            .collect();
        for id in expired {
            if let Some(row) = table.rows.get_mut(&id) {
                row.job.state = JobState::Ready;
                row.job.last_failure = Some(FailureKind::LeaseExpired);
                row.job.updated_at = now;
                row.lease = None;
                row.lease_expires_at = None;
                table.ready.insert((row.job.run_at, id), id);
            }
        }
    }

    /// The row a live lease token names, if the token is still held.
    fn row_for_lease<'a>(table: &'a mut Table, lease: &LeaseToken) -> Option<&'a mut Row> {
        table
            .rows
            .values_mut()
            .find(|row| row.lease.as_ref() == Some(lease))
    }
}

impl JobStore for MemoryJobStore {
    async fn enqueue(&self, job: NewJob, now: SystemTime) -> Result<Enqueued, JobError> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(key) = job.idempotency_key()
            && let Some(existing) = table.keys.get(&(job.queue().clone(), key.clone()))
        {
            return Ok(Enqueued::Duplicate(*existing));
        }
        let depth = table
            .rows
            .values()
            .filter(|row| {
                row.job.queue == *job.queue()
                    && matches!(row.job.state, JobState::Ready | JobState::Leased)
            })
            .count() as u64;
        if depth >= self.bounds.max_queue_depth() {
            return Err(JobError::QueueFull);
        }
        let id = JobId::generate(&*self.entropy)?;
        let run_at = job.run_at().unwrap_or(now);
        let stored = Job {
            id,
            queue: job.queue().clone(),
            kind: job.kind().clone(),
            payload: job.payload().clone(),
            state: JobState::Ready,
            attempts: 0,
            max_attempts: job.max_attempts(),
            run_at,
            idempotency_key: job.idempotency_key().cloned(),
            last_failure: None,
            trace: job.trace().cloned(),
            created_at: now,
            updated_at: now,
            finished_at: None,
        };
        if let Some(key) = job.idempotency_key() {
            table.keys.insert((job.queue().clone(), key.clone()), id);
        }
        table.ready.insert((run_at, id), id);
        table.rows.insert(
            id,
            Row {
                job: stored,
                lease: None,
                lease_expires_at: None,
            },
        );
        Ok(Enqueued::Created(id))
    }

    async fn claim(
        &self,
        queue: &QueueName,
        now: SystemTime,
        lease: Duration,
    ) -> Result<Option<ClaimedJob>, JobError> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        Self::reclaim_expired(&mut table, queue, now);
        let candidate = table
            .ready
            .iter()
            .find(|((run_at, _), id)| {
                *run_at <= now
                    && table.rows.get(id).is_some_and(|row| {
                        row.job.queue == *queue && row.job.state == JobState::Ready
                    })
            })
            .map(|(key, id)| (*key, *id));
        let Some((key, id)) = candidate else {
            return Ok(None);
        };
        let token = LeaseToken::generate(&*self.entropy)?;
        table.ready.remove(&key);
        let row = table.rows.get_mut(&id).ok_or(JobError::NotFound)?;
        row.job.state = JobState::Leased;
        row.job.attempts += 1;
        row.job.updated_at = now;
        row.lease = Some(token);
        row.lease_expires_at = Some(now + lease);
        Ok(Some(ClaimedJob {
            job: row.job.clone(),
            lease: token,
            lease_expires_at: now + lease,
        }))
    }

    async fn complete(&self, lease: &LeaseToken, now: SystemTime) -> Result<Completion, JobError> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let row = Self::row_for_lease(&mut table, lease).ok_or(JobError::LeaseNotHeld)?;
        match row.job.state {
            JobState::Leased => {
                row.job.state = JobState::Completed;
                row.job.updated_at = now;
                row.job.finished_at = Some(now);
                Ok(Completion::Completed)
            }
            JobState::Completed => Ok(Completion::AlreadyCompleted),
            _ => Err(JobError::LeaseNotHeld),
        }
    }

    async fn fail(
        &self,
        lease: &LeaseToken,
        failure: FailureKind,
        next_run_at: SystemTime,
        now: SystemTime,
    ) -> Result<FailureOutcome, JobError> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let row = Self::row_for_lease(&mut table, lease).ok_or(JobError::LeaseNotHeld)?;
        if row.job.state != JobState::Leased {
            return Err(JobError::LeaseNotHeld);
        }
        let id = row.job.id;
        row.job.last_failure = Some(failure);
        row.job.updated_at = now;
        if failure.is_terminal() || row.job.attempts >= row.job.max_attempts {
            row.job.state = JobState::Dead;
            row.job.finished_at = Some(now);
            row.lease = None;
            row.lease_expires_at = None;
            return Ok(FailureOutcome::DeadLettered {
                attempts: row.job.attempts,
            });
        }
        row.job.state = JobState::Ready;
        row.job.run_at = next_run_at;
        row.lease = None;
        row.lease_expires_at = None;
        let attempts = row.job.attempts;
        table.ready.insert((next_run_at, id), id);
        Ok(FailureOutcome::Rescheduled {
            run_at: next_run_at,
            attempts,
        })
    }

    async fn release(&self, lease: &LeaseToken, now: SystemTime) -> Result<(), JobError> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let row = Self::row_for_lease(&mut table, lease).ok_or(JobError::LeaseNotHeld)?;
        if row.job.state != JobState::Leased {
            return Err(JobError::LeaseNotHeld);
        }
        let id = row.job.id;
        row.job.state = JobState::Ready;
        row.job.run_at = now;
        row.job.updated_at = now;
        row.lease = None;
        row.lease_expires_at = None;
        table.ready.insert((now, id), id);
        Ok(())
    }

    async fn revive(&self, id: &JobId, now: SystemTime) -> Result<bool, JobError> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let row = table.rows.get_mut(id).ok_or(JobError::NotFound)?;
        if row.job.state != JobState::Dead {
            return Ok(false);
        }
        row.job.state = JobState::Ready;
        row.job.attempts = 0;
        row.job.run_at = now;
        row.job.updated_at = now;
        row.job.finished_at = None;
        row.job.last_failure = None;
        row.lease = None;
        row.lease_expires_at = None;
        table.ready.insert((now, *id), *id);
        Ok(true)
    }

    async fn read(&self, id: &JobId) -> Result<Option<Job>, JobError> {
        let table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(table.rows.get(id).map(|row| row.job.clone()))
    }

    async fn depth(&self, queue: &QueueName) -> Result<u64, JobError> {
        let table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(table
            .rows
            .values()
            .filter(|row| {
                row.job.queue == *queue
                    && matches!(row.job.state, JobState::Ready | JobState::Leased)
            })
            .count() as u64)
    }
}
