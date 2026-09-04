//! The durable job store over SQLx, for PostgreSQL and MySQL (ADR-0032).
//!
//! # One macro body, two engines
//!
//! Every statement is written once with placeholders rendered by [`DatabaseKind::placeholder`],
//! and the two engine modules are the same body instantiated twice. Where the engines genuinely
//! differ — a bounded bulk `UPDATE` — the difference is a macro parameter carrying the whole
//! statement, so it is visible at the instantiation and nowhere else.
//!
//! # The claim is one transaction with a re-checked write
//!
//! `SELECT … FOR UPDATE SKIP LOCKED` picks the oldest runnable row a concurrent claimer has not
//! locked, and the `UPDATE` that leases it re-checks `state = 0`. If an engine's lock semantics
//! ever surprise, the transition still cannot double-claim: `rows_affected = 0` means the row was
//! taken, the transaction rolls back, and the caller sees "nothing claimable now" (FR-028).
//!
//! # The depth bound is serialised per queue
//!
//! Every enqueue takes a row lock on the queue's row in `rv_job_queue` (`SELECT … FOR UPDATE`,
//! the first statement of its transaction) before it counts, so two writers cannot both count
//! `bound − 1` and both insert: the second waits for the first to commit and then counts the
//! first's row. The queue row is upserted by its own autocommit statement before the
//! transaction, for the reason the reclaim below is — a range statement inside the transaction
//! gap-locks on InnoDB. The lock is the first statement on purpose: under REPEATABLE READ a
//! consistent read taken before the lock would pin a snapshot that does not include the row the
//! previous holder committed, and the count would be stale. The guarantee is `depth ≤ bound`,
//! proven by an eight-racer barrier race in the shared contract (FR-026).
//!
//! # `rows_affected` is safe here
//!
//! Every transition changes `state` or `attempts`, so MySQL's "changed rows, not matched rows"
//! count — the trap Phase 009 recorded — is unambiguous on both engines.
//!
//! # An expired lease at the last attempt dead-letters
//!
//! Reclaim returns an expired lease to `ready` **only while attempts remain**. A handler that hangs
//! past its lease every time would otherwise be claimed again without bound, which is the unbounded
//! retry FR-092 forbids. The same rule is in the memory substitute and asserted by the shared
//! contract, so all five stores agree.
//!
//! # What reaches the log
//!
//! Nothing from a row. A stored identifier of the wrong width is reported as a fixed message and
//! `JobError::Unavailable`; the driver's own error is classified by [`classify_error`] into a
//! closed kind and never rendered (FR-037, FR-041).

use std::sync::Arc;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use renvor_core::observe::entropy::EntropySource;
use renvor_core::observe::trace_context::TraceContext;
use renvor_database::{DatabaseError, DatabaseErrorKind, DatabaseKind};
use renvor_jobs::{
    ClaimedJob, Completion, Enqueued, FailureKind, FailureOutcome, IdempotencyKey, Job, JobBounds,
    JobError, JobId, JobKind, JobPayload, JobState, JobStore, LeaseToken, NewJob, QueueName,
    RECLAIM_BATCH,
};

use crate::error::classify_error;

/// The tracing target for store-side reports.
const TARGET: &str = "renvor.jobs";

/// Every column of `rv_job`, in the order [`Row`] decodes them.
const COLUMNS: &str = "id, queue, kind, payload, idempotency_key, state, attempts, max_attempts, run_at, lease_token, lease_expires_at, last_failure, trace_parent, created_at, updated_at, completed_at";

/// One row of `rv_job`, decoded positionally.
type Row = (
    Vec<u8>,
    String,
    String,
    Vec<u8>,
    Option<String>,
    i16,
    i32,
    i32,
    DateTime<Utc>,
    Option<Vec<u8>>,
    Option<DateTime<Utc>>,
    Option<i16>,
    Option<String>,
    DateTime<Utc>,
    DateTime<Utc>,
    Option<DateTime<Utc>>,
);

/// Maps a classified database failure onto the job port's closed error.
fn job_error(error: &DatabaseError) -> JobError {
    match error.kind() {
        DatabaseErrorKind::AcquireTimeout | DatabaseErrorKind::DeadlineExceeded => {
            JobError::TimedOut
        }
        _ => JobError::Unavailable,
    }
}

/// Classifies a driver error and maps it, in one step.
fn failed(error: &sqlx::Error) -> JobError {
    job_error(&classify_error(error))
}

/// A stored value that cannot be what the store wrote: reported without the value.
fn corrupt(what: &'static str) -> JobError {
    tracing::error!(
        target: TARGET,
        column = what,
        "a stored job column does not decode; the row is unreadable"
    );
    JobError::Unavailable
}

fn sixteen(bytes: &[u8], what: &'static str) -> Result<[u8; 16], JobError> {
    bytes.try_into().map_err(|_| corrupt(what))
}

fn db_time(at: SystemTime) -> DateTime<Utc> {
    DateTime::<Utc>::from(at)
}

fn sys_time(at: DateTime<Utc>) -> SystemTime {
    at.into()
}

fn attempts_from(value: i32, what: &'static str) -> Result<u32, JobError> {
    u32::try_from(value).map_err(|_| corrupt(what))
}

/// Rebuilds a [`Job`] from a row, bounding the payload on read (FR-025).
fn job_from_row(row: Row, bounds: &JobBounds) -> Result<Job, JobError> {
    let (
        id,
        queue,
        kind,
        payload,
        idempotency_key,
        state,
        attempts,
        max_attempts,
        run_at,
        _lease_token,
        _lease_expires_at,
        last_failure,
        trace_parent,
        created_at,
        updated_at,
        completed_at,
    ) = row;
    let state = u8::try_from(state)
        .ok()
        .and_then(JobState::from_u8)
        .ok_or_else(|| corrupt("state"))?;
    let last_failure = match last_failure {
        None => None,
        Some(code) => Some(
            u8::try_from(code)
                .ok()
                .and_then(FailureKind::from_u8)
                .ok_or_else(|| corrupt("last_failure"))?,
        ),
    };
    Ok(Job {
        id: JobId::from_bytes(sixteen(&id, "id")?),
        queue: QueueName::new(&queue).map_err(|_| corrupt("queue"))?,
        kind: JobKind::new(&kind).map_err(|_| corrupt("kind"))?,
        payload: JobPayload::from_stored(payload, bounds)?,
        state,
        attempts: attempts_from(attempts, "attempts")?,
        max_attempts: attempts_from(max_attempts, "max_attempts")?,
        run_at: sys_time(run_at),
        idempotency_key: match idempotency_key {
            None => None,
            Some(key) => Some(IdempotencyKey::new(&key).map_err(|_| corrupt("idempotency_key"))?),
        },
        last_failure,
        // A stored `traceparent` that no longer parses is dropped, not propagated: the bound on
        // read is the parser's (FR-038), and a job without a trace is still a job.
        trace: trace_parent.and_then(|rendered| TraceContext::parse(&rendered, None).ok()),
        created_at: sys_time(created_at),
        updated_at: sys_time(updated_at),
        finished_at: completed_at.map(sys_time),
    })
}

/// Generates the store for one engine.
macro_rules! job_store {
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $reclaim:literal, $queue_upsert:literal, $engine_doc:literal) => {
        #[cfg(feature = $feature)]
        #[doc = $engine_doc]
        pub mod $module {
            use super::{
                Arc, COLUMNS, ClaimedJob, Completion, DatabaseErrorKind, DatabaseKind, Enqueued,
                EntropySource, FailureKind, FailureOutcome, Job, JobBounds, JobError, JobId,
                JobStore, LeaseToken, NewJob, QueueName, RECLAIM_BATCH, Row, SystemTime,
                classify_error, db_time, failed, job_error, job_from_row, sixteen,
            };
            use std::time::Duration;

            /// This engine's placeholder rule.
            const KIND: DatabaseKind = $kind;

            /// Reads and writes `rv_job` (ADR-0032).
            #[derive(Clone)]
            pub struct SqlxJobStore {
                pool: sqlx::Pool<$driver>,
                bounds: JobBounds,
                entropy: Arc<dyn EntropySource>,
            }

            impl core::fmt::Debug for SqlxJobStore {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    f.debug_struct("SqlxJobStore")
                        .field("bounds", &self.bounds)
                        .finish_non_exhaustive()
                }
            }

            impl SqlxJobStore {
                /// Wraps a pool. Identifiers and lease tokens come from `entropy` and nothing else.
                #[must_use]
                pub fn new(
                    pool: sqlx::Pool<$driver>,
                    bounds: JobBounds,
                    entropy: Arc<dyn EntropySource>,
                ) -> Self {
                    Self {
                        pool,
                        bounds,
                        entropy,
                    }
                }

                /// The bounds this store validates against.
                #[must_use]
                pub const fn bounds(&self) -> &JobBounds {
                    &self.bounds
                }

                /// The id already holding `(queue, key)`, if any.
                async fn existing_for_key(
                    &self,
                    queue: &QueueName,
                    key: &str,
                ) -> Result<Option<JobId>, JobError> {
                    let select = format!(
                        "SELECT id FROM rv_job WHERE queue = {} AND idempotency_key = {}",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    let id: Option<Vec<u8>> = sqlx::query_scalar(sqlx::AssertSqlSafe(select))
                        .bind(queue.as_str())
                        .bind(key)
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    match id {
                        Some(bytes) => Ok(Some(JobId::from_bytes(sixteen(&bytes, "id")?))),
                        None => Ok(None),
                    }
                }
            }

            impl JobStore for SqlxJobStore {
                async fn enqueue(&self, job: NewJob, now: SystemTime) -> Result<Enqueued, JobError> {
                    // 0. THE QUEUE ROW, as its own autocommit statement (see the module docs for
                    //    why not inside the transaction), so the lock below has a row to take.
                    let upsert = format!($queue_upsert, KIND.placeholder(1));
                    sqlx::query(sqlx::AssertSqlSafe(upsert))
                        .bind(job.queue().as_str())
                        .execute(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;

                    let mut transaction = self.pool.begin().await.map_err(|e| failed(&e))?;

                    // 1. THE LOCK, first: every enqueue on this queue serialises here, so the
                    //    count in step 3 is taken after every earlier writer committed.
                    let lock = format!(
                        "SELECT queue FROM rv_job_queue WHERE queue = {} FOR UPDATE",
                        KIND.placeholder(1),
                    );
                    let _held: String = sqlx::query_scalar(sqlx::AssertSqlSafe(lock))
                        .bind(job.queue().as_str())
                        .fetch_one(&mut *transaction)
                        .await
                        .map_err(|error| failed(&error))?;

                    // 2. THE KEY, if any. Answered from the row rather than from a failed insert
                    //    when it can be, so the common duplicate costs no constraint violation.
                    if let Some(key) = job.idempotency_key() {
                        let select = format!(
                            "SELECT id FROM rv_job WHERE queue = {} AND idempotency_key = {}",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        );
                        let existing: Option<Vec<u8>> =
                            sqlx::query_scalar(sqlx::AssertSqlSafe(select))
                                .bind(job.queue().as_str())
                                .bind(key.as_str())
                                .fetch_optional(&mut *transaction)
                                .await
                                .map_err(|error| failed(&error))?;
                        if let Some(bytes) = existing {
                            return Ok(Enqueued::Duplicate(JobId::from_bytes(sixteen(
                                &bytes, "id",
                            )?)));
                        }
                    }

                    // 3. THE DEPTH, counted under the queue lock (FR-026): `depth ≤ bound`.
                    let count = format!(
                        "SELECT COUNT(*) FROM rv_job WHERE queue = {} AND state IN (0, 1)",
                        KIND.placeholder(1),
                    );
                    let depth: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count))
                        .bind(job.queue().as_str())
                        .fetch_one(&mut *transaction)
                        .await
                        .map_err(|error| failed(&error))?;
                    if u64::try_from(depth).unwrap_or(u64::MAX) >= self.bounds.max_queue_depth() {
                        return Err(JobError::QueueFull);
                    }

                    // 4. THE ROW. The identifier is entropy, never a sequence (FR-042).
                    let id = JobId::generate(&*self.entropy)?;
                    let run_at = job.run_at().unwrap_or(now);
                    let insert = format!(
                        "INSERT INTO rv_job ({COLUMNS}) VALUES ({}, {}, {}, {}, {}, 0, 0, {}, {}, NULL, NULL, NULL, {}, {}, {}, NULL)",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        KIND.placeholder(5),
                        KIND.placeholder(6),
                        KIND.placeholder(7),
                        KIND.placeholder(8),
                        KIND.placeholder(9),
                        KIND.placeholder(10),
                    );
                    let max_attempts = i32::try_from(job.max_attempts())
                        .map_err(|_| JobError::Refused(renvor_jobs::JobRefusal::AttemptsOutOfRange))?;
                    let outcome = sqlx::query(sqlx::AssertSqlSafe(insert))
                        .bind(id.as_bytes().as_slice())
                        .bind(job.queue().as_str())
                        .bind(job.kind().as_str())
                        .bind(job.payload().as_bytes())
                        .bind(job.idempotency_key().map(|key| key.as_str()))
                        .bind(max_attempts)
                        .bind(db_time(run_at))
                        .bind(job.trace().map(|trace| trace.render_traceparent()))
                        .bind(db_time(now))
                        .bind(db_time(now))
                        .execute(&mut *transaction)
                        .await;
                    match outcome {
                        Ok(_) => {
                            transaction.commit().await.map_err(|e| failed(&e))?;
                            Ok(Enqueued::Created(id))
                        }
                        Err(error) => {
                            let classified = classify_error(&error);
                            // AN EXPECTED OUTCOME under the race, not a fault: the unique index
                            // is what makes four concurrent enqueues store one row (FR-024).
                            if classified.kind() == DatabaseErrorKind::UniqueViolation
                                && let Some(key) = job.idempotency_key()
                            {
                                drop(transaction);
                                return match self.existing_for_key(job.queue(), key.as_str()).await? {
                                    Some(existing) => Ok(Enqueued::Duplicate(existing)),
                                    None => Err(JobError::Unavailable),
                                };
                            }
                            Err(job_error(&classified))
                        }
                    }
                }

                async fn claim(
                    &self,
                    queue: &QueueName,
                    now: SystemTime,
                    lease: Duration,
                ) -> Result<Option<ClaimedJob>, JobError> {
                    // 1. RECLAIM expired leases, bounded (FR-028), as its OWN autocommit statement
                    //    and not inside the claim transaction. At the last attempt the job
                    //    dead-letters instead of returning to ready.
                    //
                    //    Outside the transaction on purpose. Under InnoDB's default isolation a
                    //    range `UPDATE` that matches nothing still gap-locks the index range it
                    //    scanned; held across the `SELECT … FOR UPDATE` below, two claimers each
                    //    hold a gap the other's lease write must insert into, and MySQL reports a
                    //    deadlock. Found by the shared contract's four-racer claim on MySQL — the
                    //    PostgreSQL rows never showed it.
                    let reclaim = format!(
                        $reclaim,
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                        RECLAIM_BATCH,
                    );
                    sqlx::query(sqlx::AssertSqlSafe(reclaim))
                        .bind(db_time(now))
                        .bind(db_time(now))
                        .bind(queue.as_str())
                        .bind(db_time(now))
                        .execute(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;

                    let mut transaction = self.pool.begin().await.map_err(|e| failed(&e))?;

                    // 2. THE OLDEST RUNNABLE ROW nobody else holds.
                    let select = format!(
                        "SELECT {COLUMNS} FROM rv_job WHERE queue = {} AND state = 0 AND run_at <= {} ORDER BY run_at, id LIMIT 1 FOR UPDATE SKIP LOCKED",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                    );
                    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(select))
                        .bind(queue.as_str())
                        .bind(db_time(now))
                        .fetch_optional(&mut *transaction)
                        .await
                        .map_err(|error| failed(&error))?;
                    let Some(row) = row else {
                        transaction.commit().await.map_err(|e| failed(&e))?;
                        return Ok(None);
                    };

                    // 3. THE LEASE, re-checking the state (FR-028, FR-039).
                    let token = LeaseToken::generate(&*self.entropy)?;
                    let expires_at = now + lease;
                    let update = format!(
                        "UPDATE rv_job SET state = 1, attempts = attempts + 1, lease_token = {}, lease_expires_at = {}, updated_at = {} WHERE id = {} AND state = 0",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                        KIND.placeholder(4),
                    );
                    let done = sqlx::query(sqlx::AssertSqlSafe(update))
                        .bind(token.as_bytes().as_slice())
                        .bind(db_time(expires_at))
                        .bind(db_time(now))
                        .bind(row.0.as_slice())
                        .execute(&mut *transaction)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() != 1 {
                        // Lost to a concurrent claimer despite the lock: nothing claimable now.
                        transaction.rollback().await.map_err(|e| failed(&e))?;
                        return Ok(None);
                    }
                    transaction.commit().await.map_err(|e| failed(&e))?;

                    let mut job = job_from_row(row, &self.bounds)?;
                    job.state = renvor_jobs::JobState::Leased;
                    job.attempts = job.attempts.saturating_add(1);
                    job.updated_at = now;
                    Ok(Some(ClaimedJob {
                        job,
                        lease: token,
                        lease_expires_at: expires_at,
                    }))
                }

                async fn complete(
                    &self,
                    lease: &LeaseToken,
                    now: SystemTime,
                ) -> Result<Completion, JobError> {
                    let update = format!(
                        "UPDATE rv_job SET state = 2, completed_at = {}, updated_at = {} WHERE lease_token = {} AND state = 1",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                    );
                    let done = sqlx::query(sqlx::AssertSqlSafe(update))
                        .bind(db_time(now))
                        .bind(db_time(now))
                        .bind(lease.as_bytes().as_slice())
                        .execute(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() == 1 {
                        return Ok(Completion::Completed);
                    }
                    // Idempotent: the token stays on a completed row, so a second call finds it.
                    let select = format!(
                        "SELECT state FROM rv_job WHERE lease_token = {}",
                        KIND.placeholder(1),
                    );
                    let state: Option<i16> = sqlx::query_scalar(sqlx::AssertSqlSafe(select))
                        .bind(lease.as_bytes().as_slice())
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    match state {
                        Some(2) => Ok(Completion::AlreadyCompleted),
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
                    let mut transaction = self.pool.begin().await.map_err(|e| failed(&e))?;
                    let lock = format!(
                        "SELECT id, attempts, max_attempts FROM rv_job WHERE lease_token = {} AND state = 1 FOR UPDATE",
                        KIND.placeholder(1),
                    );
                    let held: Option<(Vec<u8>, i32, i32)> =
                        sqlx::query_as(sqlx::AssertSqlSafe(lock))
                            .bind(lease.as_bytes().as_slice())
                            .fetch_optional(&mut *transaction)
                            .await
                            .map_err(|error| failed(&error))?;
                    let Some((id, attempts, max_attempts)) = held else {
                        return Err(JobError::LeaseNotHeld);
                    };
                    let attempts = u32::try_from(attempts).unwrap_or(u32::MAX);
                    let max_attempts = u32::try_from(max_attempts).unwrap_or(0);
                    let code = i16::from(failure.as_u8());

                    // THE DECISION, in Rust, as the memory substitute makes it.
                    let dead = failure.is_terminal() || attempts >= max_attempts;
                    let update = if dead {
                        format!(
                            "UPDATE rv_job SET state = 3, last_failure = {}, lease_token = NULL, lease_expires_at = NULL, completed_at = {}, updated_at = {} WHERE id = {} AND state = 1",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                        )
                    } else {
                        format!(
                            "UPDATE rv_job SET state = 0, last_failure = {}, lease_token = NULL, lease_expires_at = NULL, run_at = {}, updated_at = {} WHERE id = {} AND state = 1",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                        )
                    };
                    let second = if dead { db_time(now) } else { db_time(next_run_at) };
                    let done = sqlx::query(sqlx::AssertSqlSafe(update))
                        .bind(code)
                        .bind(second)
                        .bind(db_time(now))
                        .bind(id.as_slice())
                        .execute(&mut *transaction)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() != 1 {
                        return Err(JobError::LeaseNotHeld);
                    }
                    transaction.commit().await.map_err(|e| failed(&e))?;
                    Ok(if dead {
                        FailureOutcome::DeadLettered { attempts }
                    } else {
                        FailureOutcome::Rescheduled {
                            run_at: next_run_at,
                            attempts,
                        }
                    })
                }

                async fn release(&self, lease: &LeaseToken, now: SystemTime) -> Result<(), JobError> {
                    let update = format!(
                        "UPDATE rv_job SET state = 0, run_at = {}, lease_token = NULL, lease_expires_at = NULL, updated_at = {} WHERE lease_token = {} AND state = 1",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                    );
                    let done = sqlx::query(sqlx::AssertSqlSafe(update))
                        .bind(db_time(now))
                        .bind(db_time(now))
                        .bind(lease.as_bytes().as_slice())
                        .execute(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() == 1 {
                        Ok(())
                    } else {
                        Err(JobError::LeaseNotHeld)
                    }
                }

                async fn revive(&self, id: &JobId, now: SystemTime) -> Result<bool, JobError> {
                    let update = format!(
                        "UPDATE rv_job SET state = 0, attempts = 0, run_at = {}, updated_at = {}, completed_at = NULL, last_failure = NULL, lease_token = NULL, lease_expires_at = NULL WHERE id = {} AND state = 3",
                        KIND.placeholder(1),
                        KIND.placeholder(2),
                        KIND.placeholder(3),
                    );
                    let done = sqlx::query(sqlx::AssertSqlSafe(update))
                        .bind(db_time(now))
                        .bind(db_time(now))
                        .bind(id.as_bytes().as_slice())
                        .execute(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() == 1 {
                        return Ok(true);
                    }
                    let exists = format!(
                        "SELECT COUNT(*) FROM rv_job WHERE id = {}",
                        KIND.placeholder(1),
                    );
                    let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(exists))
                        .bind(id.as_bytes().as_slice())
                        .fetch_one(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    if count == 0 {
                        Err(JobError::NotFound)
                    } else {
                        Ok(false)
                    }
                }

                async fn read(&self, id: &JobId) -> Result<Option<Job>, JobError> {
                    let select = format!(
                        "SELECT {COLUMNS} FROM rv_job WHERE id = {}",
                        KIND.placeholder(1),
                    );
                    let row: Option<Row> = sqlx::query_as(sqlx::AssertSqlSafe(select))
                        .bind(id.as_bytes().as_slice())
                        .fetch_optional(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    match row {
                        Some(row) => job_from_row(row, &self.bounds).map(Some),
                        None => Ok(None),
                    }
                }

                async fn depth(&self, queue: &QueueName) -> Result<u64, JobError> {
                    let count = format!(
                        "SELECT COUNT(*) FROM rv_job WHERE queue = {} AND state IN (0, 1)",
                        KIND.placeholder(1),
                    );
                    let depth: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(count))
                        .bind(queue.as_str())
                        .fetch_one(&self.pool)
                        .await
                        .map_err(|error| failed(&error))?;
                    Ok(u64::try_from(depth).unwrap_or(0))
                }
            }
        }
    };
}

job_store!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    DatabaseKind::Postgres,
    // A bounded bulk UPDATE on PostgreSQL goes through a subselect: `UPDATE … LIMIT` is not SQL
    // the engine accepts. Placeholders: updated_at, completed_at, queue, the expiry cutoff; the
    // batch size is a literal.
    "UPDATE rv_job SET state = CASE WHEN attempts >= max_attempts THEN 3 ELSE 0 END, lease_token = NULL, lease_expires_at = NULL, last_failure = 4, updated_at = {}, completed_at = CASE WHEN attempts >= max_attempts THEN {} ELSE NULL END WHERE id IN (SELECT id FROM rv_job WHERE queue = {} AND state = 1 AND lease_expires_at <= {} LIMIT {})",
    // The queue row an enqueue locks: created once, then a no-op.
    "INSERT INTO rv_job_queue (queue) VALUES ({}) ON CONFLICT (queue) DO NOTHING",
    "The store on PostgreSQL."
);
job_store!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    DatabaseKind::MySql,
    // MySQL takes `LIMIT` on the UPDATE itself and refuses a subselect on the table being
    // updated. Same placeholders in the same order, so the macro body binds identically.
    "UPDATE rv_job SET state = CASE WHEN attempts >= max_attempts THEN 3 ELSE 0 END, lease_token = NULL, lease_expires_at = NULL, last_failure = 4, updated_at = {}, completed_at = CASE WHEN attempts >= max_attempts THEN {} ELSE NULL END WHERE queue = {} AND state = 1 AND lease_expires_at <= {} LIMIT {}",
    // MySQL has no `ON CONFLICT`; a duplicate-key update that sets the key to itself is the
    // narrowest no-op (`INSERT IGNORE` would also swallow unrelated errors).
    "INSERT INTO rv_job_queue (queue) VALUES ({}) ON DUPLICATE KEY UPDATE queue = queue",
    "The store on MySQL."
);
