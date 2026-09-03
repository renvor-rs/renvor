//! The durable job store over SeaORM, for PostgreSQL and MySQL (ADR-0032).
//!
//! # The mirror of the direct-SQLx store, statement for statement
//!
//! The same transaction shapes as `renvor_sqlx::jobs` — a claim that locks with `FOR UPDATE SKIP
//! LOCKED` and re-checks the state on write, a failure decided in Rust inside a locked read, a
//! reclaim that dead-letters at the last attempt — because FR-040 requires one shared contract to
//! hold for all four rows, and a different shape would make that a coincidence. The reasoning for
//! each statement is in the SQLx copy and is not repeated here.
//!
//! Statements go through [`sea_orm::Statement::from_sql_and_values`], which passes the SQL to the
//! driver unchanged and binds values by position; placeholders are rendered by
//! [`DatabaseKind::placeholder`].

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

use crate::SeaOrmDatabase;
use crate::error::classify_db_error;

/// The tracing target for store-side reports.
const TARGET: &str = "renvor.jobs";

/// Every column of `rv_job`, in the order [`job_from_row`] reads them.
const COLUMNS: &str = "id, queue, kind, payload, idempotency_key, state, attempts, max_attempts, run_at, lease_token, lease_expires_at, last_failure, trace_parent, created_at, updated_at, completed_at";

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
fn failed(error: &sea_orm::DbErr) -> JobError {
    job_error(&classify_db_error(error))
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

/// Reads one typed column, reporting the column name and never the value on failure.
fn column<T>(row: &sea_orm::QueryResult, name: &'static str) -> Result<T, JobError>
where
    T: sea_orm::TryGetable,
{
    row.try_get("", name).map_err(|_| corrupt(name))
}

/// Rebuilds a [`Job`] from a row, bounding the payload on read (FR-025).
fn job_from_row(row: &sea_orm::QueryResult, bounds: &JobBounds) -> Result<Job, JobError> {
    let id: Vec<u8> = column(row, "id")?;
    let queue: String = column(row, "queue")?;
    let kind: String = column(row, "kind")?;
    let payload: Vec<u8> = column(row, "payload")?;
    let idempotency_key: Option<String> = column(row, "idempotency_key")?;
    let state: i16 = column(row, "state")?;
    let attempts: i32 = column(row, "attempts")?;
    let max_attempts: i32 = column(row, "max_attempts")?;
    let run_at: DateTime<Utc> = column(row, "run_at")?;
    let last_failure: Option<i16> = column(row, "last_failure")?;
    let trace_parent: Option<String> = column(row, "trace_parent")?;
    let created_at: DateTime<Utc> = column(row, "created_at")?;
    let updated_at: DateTime<Utc> = column(row, "updated_at")?;
    let completed_at: Option<DateTime<Utc>> = column(row, "completed_at")?;

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
        attempts: u32::try_from(attempts).map_err(|_| corrupt("attempts"))?,
        max_attempts: u32::try_from(max_attempts).map_err(|_| corrupt("max_attempts"))?,
        run_at: sys_time(run_at),
        idempotency_key: match idempotency_key {
            None => None,
            Some(key) => Some(IdempotencyKey::new(&key).map_err(|_| corrupt("idempotency_key"))?),
        },
        last_failure,
        trace: trace_parent.and_then(|rendered| TraceContext::parse(&rendered, None).ok()),
        created_at: sys_time(created_at),
        updated_at: sys_time(updated_at),
        finished_at: completed_at.map(sys_time),
    })
}

/// Generates the store for one engine.
macro_rules! job_store {
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $reclaim:literal, $engine_doc:literal) => {
        #[cfg(feature = $feature)]
        #[doc = $engine_doc]
        pub mod $module {
            use super::{
                Arc, COLUMNS, ClaimedJob, Completion, DatabaseErrorKind, DatabaseKind, Enqueued,
                EntropySource, FailureKind, FailureOutcome, Job, JobBounds, JobError, JobId,
                JobStore, LeaseToken, NewJob, QueueName, RECLAIM_BATCH, SeaOrmDatabase,
                SystemTime, classify_db_error, column, db_time, failed, job_error, job_from_row,
                sixteen,
            };
            use renvor_database::{Database as _, UnitOfWork as _};
            use sea_orm::ConnectionTrait as _;
            use std::time::Duration;

            /// This engine's placeholder rule.
            const KIND: DatabaseKind = $kind;

            fn statement(
                backend: sea_orm::DbBackend,
                sql: String,
                values: impl IntoIterator<Item = sea_orm::Value>,
            ) -> sea_orm::Statement {
                sea_orm::Statement::from_sql_and_values(backend, sql, values)
            }

            /// Reads and writes `rv_job` (ADR-0032).
            #[derive(Clone)]
            pub struct SeaOrmJobStore {
                database: Arc<SeaOrmDatabase<$driver>>,
                bounds: JobBounds,
                entropy: Arc<dyn EntropySource>,
            }

            impl core::fmt::Debug for SeaOrmJobStore {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    f.debug_struct("SeaOrmJobStore")
                        .field("bounds", &self.bounds)
                        .finish_non_exhaustive()
                }
            }

            impl SeaOrmJobStore {
                /// Wraps a database handle. Identifiers and lease tokens come from `entropy`.
                #[must_use]
                pub fn new(
                    database: Arc<SeaOrmDatabase<$driver>>,
                    bounds: JobBounds,
                    entropy: Arc<dyn EntropySource>,
                ) -> Self {
                    Self {
                        database,
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
                    let connection = self.database.acquire().await.map_err(|e| job_error(&e))?;
                    let backend = connection.get_database_backend();
                    let select = statement(
                        backend,
                        format!(
                            "SELECT id FROM rv_job WHERE queue = {} AND idempotency_key = {}",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        ),
                        [queue.as_str().into(), key.into()],
                    );
                    let row = connection
                        .query_one_raw(select)
                        .await
                        .map_err(|error| failed(&error))?;
                    match row {
                        Some(row) => {
                            let bytes: Vec<u8> = column(&row, "id")?;
                            Ok(Some(JobId::from_bytes(sixteen(&bytes, "id")?)))
                        }
                        None => Ok(None),
                    }
                }

                async fn count(
                    &self,
                    executor: &impl sea_orm::ConnectionTrait,
                    sql: String,
                    values: impl IntoIterator<Item = sea_orm::Value>,
                ) -> Result<i64, JobError> {
                    let backend = executor.get_database_backend();
                    let row = executor
                        .query_one_raw(statement(backend, sql, values))
                        .await
                        .map_err(|error| failed(&error))?
                        .ok_or(JobError::Unavailable)?;
                    row.try_get_by_index::<i64>(0).map_err(|_| JobError::Unavailable)
                }
            }

            impl JobStore for SeaOrmJobStore {
                async fn enqueue(&self, job: NewJob, now: SystemTime) -> Result<Enqueued, JobError> {
                    let unit = self.database.begin().await.map_err(|e| job_error(&e))?;
                    let backend = unit.get_database_backend();

                    // 1. THE KEY, if any.
                    if let Some(key) = job.idempotency_key() {
                        let select = statement(
                            backend,
                            format!(
                                "SELECT id FROM rv_job WHERE queue = {} AND idempotency_key = {}",
                                KIND.placeholder(1),
                                KIND.placeholder(2),
                            ),
                            [job.queue().as_str().into(), key.as_str().into()],
                        );
                        let existing = unit
                            .query_one_raw(select)
                            .await
                            .map_err(|error| failed(&error))?;
                        if let Some(row) = existing {
                            let bytes: Vec<u8> = column(&row, "id")?;
                            unit.rollback().await.map_err(|e| job_error(&e))?;
                            return Ok(Enqueued::Duplicate(JobId::from_bytes(sixteen(
                                &bytes, "id",
                            )?)));
                        }
                    }

                    // 2. THE DEPTH, in this transaction (FR-026).
                    let depth = self
                        .count(
                            &unit,
                            format!(
                                "SELECT COUNT(*) FROM rv_job WHERE queue = {} AND state IN (0, 1)",
                                KIND.placeholder(1),
                            ),
                            [job.queue().as_str().into()],
                        )
                        .await?;
                    if u64::try_from(depth).unwrap_or(u64::MAX) >= self.bounds.max_queue_depth() {
                        unit.rollback().await.map_err(|e| job_error(&e))?;
                        return Err(JobError::QueueFull);
                    }

                    // 3. THE ROW.
                    let id = JobId::generate(&*self.entropy)?;
                    let run_at = job.run_at().unwrap_or(now);
                    let max_attempts = i32::try_from(job.max_attempts())
                        .map_err(|_| JobError::Refused(renvor_jobs::JobRefusal::AttemptsOutOfRange))?;
                    let insert = statement(
                        backend,
                        format!(
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
                        ),
                        [
                            id.as_bytes().to_vec().into(),
                            job.queue().as_str().into(),
                            job.kind().as_str().into(),
                            job.payload().as_bytes().to_vec().into(),
                            job.idempotency_key().map(|key| key.as_str().to_owned()).into(),
                            max_attempts.into(),
                            db_time(run_at).into(),
                            job.trace().map(|trace| trace.render_traceparent()).into(),
                            db_time(now).into(),
                            db_time(now).into(),
                        ],
                    );
                    match unit.execute_raw(insert).await {
                        Ok(_) => {
                            unit.commit().await.map_err(|e| job_error(&e))?;
                            Ok(Enqueued::Created(id))
                        }
                        Err(error) => {
                            let classified = classify_db_error(&error);
                            unit.rollback().await.map_err(|e| job_error(&e))?;
                            if classified.kind() == DatabaseErrorKind::UniqueViolation
                                && let Some(key) = job.idempotency_key()
                            {
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
                    // 1. RECLAIM, bounded, dead-lettering at the last attempt — as its own
                    //    autocommit statement before the transaction, for the InnoDB gap-lock
                    //    reason `renvor_sqlx::jobs` records.
                    let connection = self.database.acquire().await.map_err(|e| job_error(&e))?;
                    let backend = connection.get_database_backend();
                    let reclaim = statement(
                        backend,
                        format!(
                            $reclaim,
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                            RECLAIM_BATCH,
                        ),
                        [
                            db_time(now).into(),
                            db_time(now).into(),
                            queue.as_str().into(),
                            db_time(now).into(),
                        ],
                    );
                    connection
                        .execute_raw(reclaim)
                        .await
                        .map_err(|error| failed(&error))?;
                    drop(connection);

                    let unit = self.database.begin().await.map_err(|e| job_error(&e))?;
                    let backend = unit.get_database_backend();

                    // 2. THE OLDEST RUNNABLE ROW nobody else holds.
                    let select = statement(
                        backend,
                        format!(
                            "SELECT {COLUMNS} FROM rv_job WHERE queue = {} AND state = 0 AND run_at <= {} ORDER BY run_at, id LIMIT 1 FOR UPDATE SKIP LOCKED",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                        ),
                        [queue.as_str().into(), db_time(now).into()],
                    );
                    let row = unit
                        .query_one_raw(select)
                        .await
                        .map_err(|error| failed(&error))?;
                    let Some(row) = row else {
                        unit.commit().await.map_err(|e| job_error(&e))?;
                        return Ok(None);
                    };
                    let id: Vec<u8> = column(&row, "id")?;

                    // 3. THE LEASE, re-checking the state.
                    let token = LeaseToken::generate(&*self.entropy)?;
                    let expires_at = now + lease;
                    let update = statement(
                        backend,
                        format!(
                            "UPDATE rv_job SET state = 1, attempts = attempts + 1, lease_token = {}, lease_expires_at = {}, updated_at = {} WHERE id = {} AND state = 0",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                            KIND.placeholder(4),
                        ),
                        [
                            token.as_bytes().to_vec().into(),
                            db_time(expires_at).into(),
                            db_time(now).into(),
                            id.into(),
                        ],
                    );
                    let done = unit
                        .execute_raw(update)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() != 1 {
                        unit.rollback().await.map_err(|e| job_error(&e))?;
                        return Ok(None);
                    }
                    unit.commit().await.map_err(|e| job_error(&e))?;

                    let mut job = job_from_row(&row, &self.bounds)?;
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
                    let connection = self.database.acquire().await.map_err(|e| job_error(&e))?;
                    let backend = connection.get_database_backend();
                    let update = statement(
                        backend,
                        format!(
                            "UPDATE rv_job SET state = 2, completed_at = {}, updated_at = {} WHERE lease_token = {} AND state = 1",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                        ),
                        [
                            db_time(now).into(),
                            db_time(now).into(),
                            lease.as_bytes().to_vec().into(),
                        ],
                    );
                    let done = connection
                        .execute_raw(update)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() == 1 {
                        return Ok(Completion::Completed);
                    }
                    let select = statement(
                        backend,
                        format!(
                            "SELECT state FROM rv_job WHERE lease_token = {}",
                            KIND.placeholder(1),
                        ),
                        [lease.as_bytes().to_vec().into()],
                    );
                    let row = connection
                        .query_one_raw(select)
                        .await
                        .map_err(|error| failed(&error))?;
                    match row {
                        Some(row) if column::<i16>(&row, "state")? == 2 => {
                            Ok(Completion::AlreadyCompleted)
                        }
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
                    let unit = self.database.begin().await.map_err(|e| job_error(&e))?;
                    let backend = unit.get_database_backend();
                    let lock = statement(
                        backend,
                        format!(
                            "SELECT id, attempts, max_attempts FROM rv_job WHERE lease_token = {} AND state = 1 FOR UPDATE",
                            KIND.placeholder(1),
                        ),
                        [lease.as_bytes().to_vec().into()],
                    );
                    let held = unit
                        .query_one_raw(lock)
                        .await
                        .map_err(|error| failed(&error))?;
                    let Some(held) = held else {
                        unit.rollback().await.map_err(|e| job_error(&e))?;
                        return Err(JobError::LeaseNotHeld);
                    };
                    let id: Vec<u8> = column(&held, "id")?;
                    let attempts = u32::try_from(column::<i32>(&held, "attempts")?).unwrap_or(u32::MAX);
                    let max_attempts =
                        u32::try_from(column::<i32>(&held, "max_attempts")?).unwrap_or(0);
                    let code = i16::from(failure.as_u8());

                    let dead = failure.is_terminal() || attempts >= max_attempts;
                    let sql = if dead {
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
                    let done = unit
                        .execute_raw(statement(
                            backend,
                            sql,
                            [code.into(), second.into(), db_time(now).into(), id.into()],
                        ))
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() != 1 {
                        unit.rollback().await.map_err(|e| job_error(&e))?;
                        return Err(JobError::LeaseNotHeld);
                    }
                    unit.commit().await.map_err(|e| job_error(&e))?;
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
                    let connection = self.database.acquire().await.map_err(|e| job_error(&e))?;
                    let backend = connection.get_database_backend();
                    let update = statement(
                        backend,
                        format!(
                            "UPDATE rv_job SET state = 0, run_at = {}, lease_token = NULL, lease_expires_at = NULL, updated_at = {} WHERE lease_token = {} AND state = 1",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                        ),
                        [
                            db_time(now).into(),
                            db_time(now).into(),
                            lease.as_bytes().to_vec().into(),
                        ],
                    );
                    let done = connection
                        .execute_raw(update)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() == 1 {
                        Ok(())
                    } else {
                        Err(JobError::LeaseNotHeld)
                    }
                }

                async fn revive(&self, id: &JobId, now: SystemTime) -> Result<bool, JobError> {
                    let connection = self.database.acquire().await.map_err(|e| job_error(&e))?;
                    let backend = connection.get_database_backend();
                    let update = statement(
                        backend,
                        format!(
                            "UPDATE rv_job SET state = 0, attempts = 0, run_at = {}, updated_at = {}, completed_at = NULL, last_failure = NULL, lease_token = NULL, lease_expires_at = NULL WHERE id = {} AND state = 3",
                            KIND.placeholder(1),
                            KIND.placeholder(2),
                            KIND.placeholder(3),
                        ),
                        [
                            db_time(now).into(),
                            db_time(now).into(),
                            id.as_bytes().to_vec().into(),
                        ],
                    );
                    let done = connection
                        .execute_raw(update)
                        .await
                        .map_err(|error| failed(&error))?;
                    if done.rows_affected() == 1 {
                        return Ok(true);
                    }
                    let count = self
                        .count(
                            &connection,
                            format!("SELECT COUNT(*) FROM rv_job WHERE id = {}", KIND.placeholder(1)),
                            [id.as_bytes().to_vec().into()],
                        )
                        .await?;
                    if count == 0 {
                        Err(JobError::NotFound)
                    } else {
                        Ok(false)
                    }
                }

                async fn read(&self, id: &JobId) -> Result<Option<Job>, JobError> {
                    let connection = self.database.acquire().await.map_err(|e| job_error(&e))?;
                    let backend = connection.get_database_backend();
                    let select = statement(
                        backend,
                        format!("SELECT {COLUMNS} FROM rv_job WHERE id = {}", KIND.placeholder(1)),
                        [id.as_bytes().to_vec().into()],
                    );
                    let row = connection
                        .query_one_raw(select)
                        .await
                        .map_err(|error| failed(&error))?;
                    match row {
                        Some(row) => job_from_row(&row, &self.bounds).map(Some),
                        None => Ok(None),
                    }
                }

                async fn depth(&self, queue: &QueueName) -> Result<u64, JobError> {
                    let connection = self.database.acquire().await.map_err(|e| job_error(&e))?;
                    let depth = self
                        .count(
                            &connection,
                            format!(
                                "SELECT COUNT(*) FROM rv_job WHERE queue = {} AND state IN (0, 1)",
                                KIND.placeholder(1),
                            ),
                            [queue.as_str().into()],
                        )
                        .await?;
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
    "UPDATE rv_job SET state = CASE WHEN attempts >= max_attempts THEN 3 ELSE 0 END, lease_token = NULL, lease_expires_at = NULL, last_failure = 4, updated_at = {}, completed_at = CASE WHEN attempts >= max_attempts THEN {} ELSE NULL END WHERE id IN (SELECT id FROM rv_job WHERE queue = {} AND state = 1 AND lease_expires_at <= {} LIMIT {})",
    "The store on PostgreSQL."
);
job_store!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    DatabaseKind::MySql,
    "UPDATE rv_job SET state = CASE WHEN attempts >= max_attempts THEN 3 ELSE 0 END, lease_token = NULL, lease_expires_at = NULL, last_failure = 4, updated_at = {}, completed_at = CASE WHEN attempts >= max_attempts THEN {} ELSE NULL END WHERE queue = {} AND state = 1 AND lease_expires_at <= {} LIMIT {}",
    "The store on MySQL."
);
