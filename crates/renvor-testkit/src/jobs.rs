//! The job-store contract every implementation must satisfy — the memory substitute and all
//! four persistence rows — compiled once and called from each.
//!
//! # Why one copy
//!
//! Phase 009 recorded what happens when the "same suite" is written twice: two files, the same
//! test names, two implementations that diverge the first time one is edited. And it recorded the
//! sharper lesson from the refresh contract: a race arranged against an in-memory fake whose
//! `async fn`s never suspend does not interleave, so it proves nothing. The races here spawn
//! **tasks** on a multi-threaded runtime over a shared `Arc`, with a barrier, so the memory store
//! contends on its lock and the database rows contend on their statements — the same assertion,
//! honestly reached both ways.
//!
//! # The fixture is tiny
//!
//! An implementation supplies the store, its bounds, and a reset. Everything else — every
//! assertion — is here, and the runner counts its own calls so a census row (one line per
//! persistence row) cannot hide a skipped assertion.

use core::future::Future;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use renvor_core::observe::TraceContext;
use renvor_jobs::{
    Completion, Enqueued, FailureKind, FailureOutcome, IdempotencyKey, JobBounds, JobError,
    JobKind, JobPayload, JobState, JobStore, LeaseToken, NewJob, QueueName,
};

/// How many concurrent callers race in the two identity race assertions.
pub const RACERS: usize = 4;

/// How many concurrent callers race against the depth bound, per round.
pub const DEPTH_RACERS: usize = 8;

/// How many rounds the depth race runs: each starts from an empty queue.
pub const DEPTH_ROUNDS: usize = 3;

/// How many assertions [`the_shared_jobs_contract_holds`] runs.
pub const ASSERTIONS: usize = 17;

/// What an implementation supplies.
pub trait JobsFixture: Send + Sync {
    /// The store under test.
    type Store: JobStore + 'static;

    /// The store, shared, so the races can spawn tasks over it.
    fn store(&self) -> Arc<Self::Store>;

    /// The bounds the store was built with. The depth assertion needs `max_queue_depth` to be
    /// **3**, so a fixture must build its store that way.
    fn bounds(&self) -> JobBounds;

    /// Empties every job table, so each assertion starts from nothing.
    fn reset(&self) -> impl Future<Output = ()> + Send;
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}

fn queue() -> QueueName {
    QueueName::new("contract").unwrap()
}

fn kind() -> JobKind {
    JobKind::new("work").unwrap()
}

fn job(bounds: &JobBounds, payload: &[u8]) -> NewJob {
    NewJob::new(
        queue(),
        kind(),
        JobPayload::within(payload.to_vec(), bounds).unwrap(),
    )
}

const LEASE: Duration = Duration::from_secs(60);

/// Enqueue, read back, and every field survives the round trip.
pub async fn enqueue_then_read_round_trips<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let trace = TraceContext::parse(
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        None,
    )
    .unwrap();
    let new = job(&fixture.bounds(), b"payload-bytes")
        .with_idempotency_key(IdempotencyKey::new("order-1").unwrap())
        .with_max_attempts(7)
        .unwrap()
        .scheduled_at(at(2_000))
        .with_trace(trace.clone());
    let Enqueued::Created(id) = store.enqueue(new, at(1_000)).await.unwrap() else {
        panic!("a fresh enqueue creates");
    };
    let read = store.read(&id).await.unwrap().expect("the job exists");
    assert_eq!(read.id, id);
    assert_eq!(read.queue, queue());
    assert_eq!(read.kind, kind());
    assert_eq!(read.payload.as_bytes(), b"payload-bytes");
    assert_eq!(read.state, JobState::Ready);
    assert_eq!(read.attempts, 0);
    assert_eq!(read.max_attempts, 7);
    assert_eq!(read.run_at, at(2_000));
    assert_eq!(
        read.idempotency_key.as_ref().map(IdempotencyKey::as_str),
        Some("order-1")
    );
    assert_eq!(
        read.trace.as_ref(),
        Some(&trace),
        "the trace context survives storage"
    );
    assert_eq!(read.created_at, at(1_000));
    assert!(read.last_failure.is_none());
    assert!(read.finished_at.is_none());
    // A job with no `run_at` runs at enqueue time.
    let Enqueued::Created(now_id) = store
        .enqueue(job(&fixture.bounds(), b""), at(1_500))
        .await
        .unwrap()
    else {
        panic!("creates");
    };
    assert_eq!(
        store.read(&now_id).await.unwrap().unwrap().run_at,
        at(1_500)
    );
    assert!(
        store
            .read(&renvor_jobs::JobId::from_bytes([0xff; 16]))
            .await
            .unwrap()
            .is_none()
    );
}

/// The same `(queue, key)` twice: one row, the second call reports the first's id.
pub async fn a_duplicate_idempotency_key_reports_the_existing_job<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let key = IdempotencyKey::new("order-9").unwrap();
    let first = store
        .enqueue(
            job(&fixture.bounds(), b"a").with_idempotency_key(key.clone()),
            at(1),
        )
        .await
        .unwrap();
    let second = store
        .enqueue(
            job(&fixture.bounds(), b"b").with_idempotency_key(key),
            at(2),
        )
        .await
        .unwrap();
    assert!(matches!(first, Enqueued::Created(_)));
    assert_eq!(second, Enqueued::Duplicate(first.id()));
    assert_eq!(store.depth(&queue()).await.unwrap(), 1, "one row, not two");
    // The stored payload is the first writer's.
    assert_eq!(
        store
            .read(&first.id())
            .await
            .unwrap()
            .unwrap()
            .payload
            .as_bytes(),
        b"a"
    );
}

/// FR-024, SC-008: four concurrent enqueues with one key produce one row.
pub async fn concurrent_enqueues_with_one_key_store_exactly_one_job<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let barrier = Arc::new(tokio::sync::Barrier::new(RACERS));
    let bounds = fixture.bounds();
    let mut handles = Vec::new();
    for _ in 0..RACERS {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            store
                .enqueue(
                    job(&bounds, b"race")
                        .with_idempotency_key(IdempotencyKey::new("one-key").unwrap()),
                    at(10),
                )
                .await
                .unwrap()
        }));
    }
    let mut created = 0;
    let mut ids = Vec::new();
    for handle in handles {
        match handle.await.unwrap() {
            Enqueued::Created(id) => {
                created += 1;
                ids.push(id);
            }
            Enqueued::Duplicate(id) => ids.push(id),
        }
    }
    assert_eq!(created, 1, "exactly one racer creates");
    assert!(
        ids.windows(2).all(|pair| pair[0] == pair[1]),
        "every racer sees the same id"
    );
    assert_eq!(store.depth(&queue()).await.unwrap(), 1);
}

/// A scheduled job is invisible to a claim before `run_at`.
pub async fn claim_respects_run_at<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    store
        .enqueue(
            job(&fixture.bounds(), b"later").scheduled_at(at(500)),
            at(100),
        )
        .await
        .unwrap();
    assert!(
        store
            .claim(&queue(), at(499), LEASE)
            .await
            .unwrap()
            .is_none()
    );
    let claimed = store
        .claim(&queue(), at(500), LEASE)
        .await
        .unwrap()
        .expect("due now");
    assert_eq!(claimed.job.attempts, 1);
    assert_eq!(claimed.job.state, JobState::Leased);
    assert_eq!(claimed.lease_expires_at, at(500) + LEASE);
}

/// Claims come out in `run_at` order, then identifier order.
pub async fn claims_are_ordered_by_run_at<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let late = store
        .enqueue(job(&fixture.bounds(), b"late").scheduled_at(at(300)), at(1))
        .await
        .unwrap()
        .id();
    let early = store
        .enqueue(
            job(&fixture.bounds(), b"early").scheduled_at(at(200)),
            at(2),
        )
        .await
        .unwrap()
        .id();
    let first = store
        .claim(&queue(), at(1_000), LEASE)
        .await
        .unwrap()
        .unwrap();
    let second = store
        .claim(&queue(), at(1_000), LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(first.job.id, early);
    assert_eq!(second.job.id, late);
    assert!(
        store
            .claim(&queue(), at(1_000), LEASE)
            .await
            .unwrap()
            .is_none()
    );
}

/// FR-027, SC-009: four concurrent claims on one ready job yield exactly one claim.
pub async fn concurrent_claims_admit_exactly_one_worker<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    store
        .enqueue(job(&fixture.bounds(), b"one"), at(1))
        .await
        .unwrap();
    let barrier = Arc::new(tokio::sync::Barrier::new(RACERS));
    let mut handles = Vec::new();
    for _ in 0..RACERS {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            store.claim(&queue(), at(10), LEASE).await.unwrap()
        }));
    }
    let mut claims = 0;
    for handle in handles {
        if handle.await.unwrap().is_some() {
            claims += 1;
        }
    }
    assert_eq!(claims, 1, "exactly one racer claims");
}

/// FR-039: completing twice is a no-op with a distinct outcome; a foreign lease is refused.
pub async fn complete_is_idempotent_and_a_foreign_lease_is_refused<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    store
        .enqueue(job(&fixture.bounds(), b"c"), at(1))
        .await
        .unwrap();
    let claimed = store.claim(&queue(), at(2), LEASE).await.unwrap().unwrap();
    assert_eq!(
        store.complete(&claimed.lease, at(3)).await.unwrap(),
        Completion::Completed
    );
    assert_eq!(
        store.complete(&claimed.lease, at(4)).await.unwrap(),
        Completion::AlreadyCompleted
    );
    let read = store.read(&claimed.job.id).await.unwrap().unwrap();
    assert_eq!(read.state, JobState::Completed);
    assert_eq!(
        read.finished_at,
        Some(at(3)),
        "the second complete changed nothing"
    );
    // A token nobody issued.
    let foreign = LeaseToken::from_bytes([0xee; 16]);
    assert_eq!(
        store.complete(&foreign, at(5)).await.unwrap_err(),
        JobError::LeaseNotHeld
    );
    assert_eq!(
        store
            .fail(&foreign, FailureKind::HandlerFailed, at(6), at(5))
            .await
            .unwrap_err(),
        JobError::LeaseNotHeld
    );
    assert_eq!(
        store.release(&foreign, at(5)).await.unwrap_err(),
        JobError::LeaseNotHeld
    );
    // Completed jobs do not count toward depth.
    assert_eq!(store.depth(&queue()).await.unwrap(), 0);
}

/// FR-029, FR-030: failures reschedule at the supplied time until attempts run out, then dead.
pub async fn failures_reschedule_then_dead_letter<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let id = store
        .enqueue(
            job(&fixture.bounds(), b"f").with_max_attempts(2).unwrap(),
            at(1),
        )
        .await
        .unwrap()
        .id();
    let first = store.claim(&queue(), at(2), LEASE).await.unwrap().unwrap();
    assert_eq!(
        store
            .fail(&first.lease, FailureKind::HandlerFailed, at(100), at(3))
            .await
            .unwrap(),
        FailureOutcome::Rescheduled {
            run_at: at(100),
            attempts: 1
        }
    );
    // Not claimable before the rescheduled time, and the old lease is gone.
    assert!(
        store
            .claim(&queue(), at(99), LEASE)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        store.complete(&first.lease, at(50)).await.unwrap_err(),
        JobError::LeaseNotHeld
    );
    let second = store
        .claim(&queue(), at(100), LEASE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.job.attempts, 2);
    assert_eq!(second.job.last_failure, Some(FailureKind::HandlerFailed));
    assert_eq!(
        store
            .fail(&second.lease, FailureKind::TimedOut, at(200), at(101))
            .await
            .unwrap(),
        FailureOutcome::DeadLettered { attempts: 2 }
    );
    let read = store.read(&id).await.unwrap().unwrap();
    assert_eq!(read.state, JobState::Dead);
    assert_eq!(read.last_failure, Some(FailureKind::TimedOut));
    assert_eq!(read.finished_at, Some(at(101)));
    assert!(
        store
            .claim(&queue(), at(1_000), LEASE)
            .await
            .unwrap()
            .is_none(),
        "dead jobs are never claimed"
    );
    assert_eq!(
        store.depth(&queue()).await.unwrap(),
        0,
        "dead jobs do not count toward depth"
    );
}

/// A terminal failure dead-letters at once, whatever attempts remain.
pub async fn an_abandoned_job_dead_letters_immediately<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let id = store
        .enqueue(
            job(&fixture.bounds(), b"a").with_max_attempts(9).unwrap(),
            at(1),
        )
        .await
        .unwrap()
        .id();
    let claimed = store.claim(&queue(), at(2), LEASE).await.unwrap().unwrap();
    assert_eq!(
        store
            .fail(&claimed.lease, FailureKind::Abandoned, at(100), at(3))
            .await
            .unwrap(),
        FailureOutcome::DeadLettered { attempts: 1 }
    );
    assert_eq!(
        store.read(&id).await.unwrap().unwrap().state,
        JobState::Dead
    );
}

/// FR-028: an expired lease is reclaimed on the next claim, with the attempt counted.
pub async fn an_expired_lease_is_reclaimed_with_the_attempt_counted<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let id = store
        .enqueue(job(&fixture.bounds(), b"l"), at(1))
        .await
        .unwrap()
        .id();
    let first = store.claim(&queue(), at(10), LEASE).await.unwrap().unwrap();
    // Still leased one second before expiry.
    assert!(
        store
            .claim(&queue(), at(10) + LEASE - Duration::from_secs(1), LEASE)
            .await
            .unwrap()
            .is_none()
    );
    // At expiry the job is reclaimed and re-claimed in one call.
    let second = store
        .claim(&queue(), at(10) + LEASE, LEASE)
        .await
        .unwrap()
        .expect("reclaimed");
    assert_eq!(second.job.id, id);
    assert_eq!(second.job.attempts, 2, "the lost attempt was counted");
    assert_eq!(second.job.last_failure, Some(FailureKind::LeaseExpired));
    assert_ne!(second.lease, first.lease, "a new lease was issued");
    // The old lease is dead.
    assert_eq!(
        store.complete(&first.lease, at(200)).await.unwrap_err(),
        JobError::LeaseNotHeld
    );
    assert_eq!(
        store.complete(&second.lease, at(200)).await.unwrap(),
        Completion::Completed
    );
}

/// FR-033: release returns the job to ready at once and invalidates the lease.
pub async fn release_returns_the_job_to_ready_and_clears_the_lease<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let id = store
        .enqueue(job(&fixture.bounds(), b"r"), at(1))
        .await
        .unwrap()
        .id();
    let claimed = store.claim(&queue(), at(2), LEASE).await.unwrap().unwrap();
    store.release(&claimed.lease, at(3)).await.unwrap();
    let read = store.read(&id).await.unwrap().unwrap();
    assert_eq!(read.state, JobState::Ready);
    assert_eq!(read.run_at, at(3), "released jobs are claimable at once");
    assert_eq!(read.attempts, 1, "the attempt stays counted");
    assert!(read.last_failure.is_none(), "a release is not a failure");
    assert_eq!(
        store.complete(&claimed.lease, at(4)).await.unwrap_err(),
        JobError::LeaseNotHeld
    );
    let again = store.claim(&queue(), at(4), LEASE).await.unwrap().unwrap();
    assert_eq!(again.job.attempts, 2);
}

/// FR-026: the depth bound refuses, and completed jobs free room.
pub async fn the_depth_bound_refuses_and_finished_jobs_free_room<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let bounds = fixture.bounds();
    assert_eq!(
        bounds.max_queue_depth(),
        3,
        "the fixture must build its store with depth 3"
    );
    for _ in 0..3 {
        store.enqueue(job(&bounds, b"d"), at(1)).await.unwrap();
    }
    assert_eq!(
        store.enqueue(job(&bounds, b"d"), at(2)).await.unwrap_err(),
        JobError::QueueFull
    );
    // Leased still counts; completed does not.
    let claimed = store.claim(&queue(), at(3), LEASE).await.unwrap().unwrap();
    assert_eq!(
        store.enqueue(job(&bounds, b"d"), at(4)).await.unwrap_err(),
        JobError::QueueFull
    );
    store.complete(&claimed.lease, at(5)).await.unwrap();
    assert!(store.enqueue(job(&bounds, b"d"), at(6)).await.is_ok());
    // Another queue is bounded separately.
    let other = NewJob::new(
        QueueName::new("other").unwrap(),
        kind(),
        JobPayload::within(Vec::new(), &bounds).unwrap(),
    );
    assert!(store.enqueue(other, at(7)).await.is_ok());
}

/// FR-026 under concurrency: the depth bound is a bound, not an estimate. Eight racers enqueue
/// into an empty queue bounded at 3 after a barrier; the queue never holds more than 3, and
/// every racer beyond the bound is told `QueueFull`. A count taken under READ COMMITTED with no
/// serialisation lets two writers both see `bound − 1` and both insert, which the first version
/// of this contract stated as `depth ≤ bound + writers − 1` — a bound that grows with the load
/// it exists to bound.
pub async fn concurrent_enqueues_never_exceed_the_depth_bound<F: JobsFixture>(fixture: &F) {
    let bounds = fixture.bounds();
    let bound = bounds.max_queue_depth();
    assert_eq!(bound, 3, "the fixture must build its store with depth 3");
    for round in 0..DEPTH_ROUNDS {
        fixture.reset().await;
        let store = fixture.store();
        let barrier = Arc::new(tokio::sync::Barrier::new(DEPTH_RACERS));
        let mut handles = Vec::new();
        for racer in 0..DEPTH_RACERS {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            // Half the racers carry a distinct idempotency key, so the keyed path — which reads
            // the key's row before counting — races too. On InnoDB a consistent read taken
            // BEFORE the queue lock pins a snapshot that predates the previous holder's commit,
            // and a count from that snapshot admits one job too many; the keyed racers are the
            // ones that would show it.
            let mut new_job = job(&bounds, b"depth");
            if racer % 2 == 0 {
                new_job = new_job.with_idempotency_key(
                    IdempotencyKey::new(&format!("depth-{round}-{racer}")).unwrap(),
                );
            }
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                store.enqueue(new_job, at(10)).await
            }));
        }
        let (mut created, mut full) = (0_u64, 0_u64);
        for handle in handles {
            match handle.await.unwrap() {
                Ok(Enqueued::Created(_)) => created += 1,
                Ok(Enqueued::Duplicate(_)) => panic!("a distinct key was reported as a duplicate"),
                Err(JobError::QueueFull) => full += 1,
                Err(_) => panic!("an enqueue failed for a reason other than a full queue"),
            }
        }
        let depth = store.depth(&queue()).await.unwrap();
        assert!(
            depth <= bound,
            "round {round}: the queue holds more jobs than its bound"
        );
        assert_eq!(
            created, bound,
            "round {round}: the number of admitted enqueues is not the bound"
        );
        assert_eq!(
            full,
            DEPTH_RACERS as u64 - bound,
            "round {round}: the racers beyond the bound were not all told the queue is full"
        );
    }
}

/// FR-029: a dead job is revived explicitly, with its attempts reset; a live one is not.
pub async fn revive_puts_a_dead_job_back_with_attempts_reset<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let id = store
        .enqueue(
            job(&fixture.bounds(), b"v").with_max_attempts(1).unwrap(),
            at(1),
        )
        .await
        .unwrap()
        .id();
    assert!(
        !store.revive(&id, at(2)).await.unwrap(),
        "a ready job is not revived"
    );
    let claimed = store.claim(&queue(), at(2), LEASE).await.unwrap().unwrap();
    store
        .fail(&claimed.lease, FailureKind::HandlerFailed, at(9), at(3))
        .await
        .unwrap();
    assert_eq!(
        store.read(&id).await.unwrap().unwrap().state,
        JobState::Dead
    );
    assert!(store.revive(&id, at(10)).await.unwrap());
    let read = store.read(&id).await.unwrap().unwrap();
    assert_eq!(read.state, JobState::Ready);
    assert_eq!(read.attempts, 0);
    assert_eq!(read.run_at, at(10));
    assert!(read.last_failure.is_none());
    assert!(read.finished_at.is_none());
    let again = store.claim(&queue(), at(10), LEASE).await.unwrap().unwrap();
    assert_eq!(again.job.attempts, 1);
    assert_eq!(
        store
            .revive(&renvor_jobs::JobId::from_bytes([0xaa; 16]), at(11))
            .await
            .unwrap_err(),
        JobError::NotFound
    );
}

/// Different queues do not see each other's jobs.
pub async fn queues_are_isolated<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    store
        .enqueue(job(&fixture.bounds(), b"q"), at(1))
        .await
        .unwrap();
    let other = QueueName::new("other").unwrap();
    assert!(store.claim(&other, at(2), LEASE).await.unwrap().is_none());
    assert_eq!(store.depth(&other).await.unwrap(), 0);
    assert!(store.claim(&queue(), at(2), LEASE).await.unwrap().is_some());
}

/// FR-037: a stored job's `Debug` never shows its payload.
pub async fn a_read_job_never_debugs_its_payload<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let id = store
        .enqueue(job(&fixture.bounds(), b"hunter2CanaryDoNotLeak"), at(1))
        .await
        .unwrap()
        .id();
    let read = store.read(&id).await.unwrap().unwrap();
    let rendered = format!("{read:?}");
    assert!(!rendered.contains("hunter2"), "the payload reached Debug");
    // POSITIVE CONTROL: the length is shown.
    assert!(
        rendered.contains("payload_bytes: 22"),
        "Debug did not report the payload length"
    );
}

/// FR-092: an expired lease at the last attempt dead-letters instead of returning to ready, so
/// a handler that outlives its lease every time is bounded like any other failure. The control
/// is the same shape one attempt short, which returns to ready.
pub async fn an_expired_lease_at_the_last_attempt_dead_letters<F: JobsFixture>(fixture: &F) {
    fixture.reset().await;
    let store = fixture.store();
    let last = store
        .enqueue(
            job(&fixture.bounds(), b"last")
                .with_max_attempts(1)
                .unwrap(),
            at(1),
        )
        .await
        .unwrap()
        .id();
    let control = store
        .enqueue(
            job(&fixture.bounds(), b"control")
                .with_max_attempts(2)
                .unwrap(),
            at(2),
        )
        .await
        .unwrap()
        .id();
    // Both are claimed and both leases expire.
    assert!(
        store
            .claim(&queue(), at(10), LEASE)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .claim(&queue(), at(10), LEASE)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .claim(&queue(), at(10), LEASE)
            .await
            .unwrap()
            .is_none()
    );
    let expired = at(10) + LEASE;
    // The control is reclaimed and handed out again; the last-attempt job is not.
    let again = store
        .claim(&queue(), expired, LEASE)
        .await
        .unwrap()
        .expect("the control returns to ready and is claimable");
    assert_eq!(again.job.id, control);
    assert_eq!(again.job.attempts, 2);
    assert_eq!(again.job.last_failure, Some(FailureKind::LeaseExpired));
    assert!(
        store
            .claim(&queue(), expired, LEASE)
            .await
            .unwrap()
            .is_none()
    );
    let dead = store.read(&last).await.unwrap().unwrap();
    assert_eq!(dead.state, JobState::Dead);
    assert_eq!(dead.attempts, 1);
    assert_eq!(dead.last_failure, Some(FailureKind::LeaseExpired));
    assert_eq!(dead.finished_at, Some(expired));
    assert_eq!(
        store.depth(&queue()).await.unwrap(),
        1,
        "only the control is live"
    );
}

/// Runs every assertion above and proves it ran all [`ASSERTIONS`] of them.
pub async fn the_shared_jobs_contract_holds<F: JobsFixture>(fixture: &F) {
    let mut ran = 0_usize;
    macro_rules! run {
        ($assertion:ident) => {
            $assertion(fixture).await;
            ran += 1;
        };
    }
    run!(enqueue_then_read_round_trips);
    run!(a_duplicate_idempotency_key_reports_the_existing_job);
    run!(concurrent_enqueues_with_one_key_store_exactly_one_job);
    run!(claim_respects_run_at);
    run!(claims_are_ordered_by_run_at);
    run!(concurrent_claims_admit_exactly_one_worker);
    run!(complete_is_idempotent_and_a_foreign_lease_is_refused);
    run!(failures_reschedule_then_dead_letter);
    run!(an_abandoned_job_dead_letters_immediately);
    run!(an_expired_lease_is_reclaimed_with_the_attempt_counted);
    run!(an_expired_lease_at_the_last_attempt_dead_letters);
    run!(release_returns_the_job_to_ready_and_clears_the_lease);
    run!(the_depth_bound_refuses_and_finished_jobs_free_room);
    run!(concurrent_enqueues_never_exceed_the_depth_bound);
    run!(revive_puts_a_dead_job_back_with_attempts_reset);
    run!(queues_are_isolated);
    run!(a_read_job_never_debugs_its_payload);
    assert_eq!(
        ran, ASSERTIONS,
        "the runner did not call every assertion; the census counts rows, not assertions"
    );
}
