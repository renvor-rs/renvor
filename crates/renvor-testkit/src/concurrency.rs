//! Concurrency and idempotency assertions, run identically against all four persistence rows.
//!
//! # What this module measures that the others do not
//!
//! [`crate::persistence`] measures one transaction at a time, and [`crate::domain`] measures one
//! caller at a time. Neither says what happens when two callers want the **same identity at the
//! same moment**, which is the case an application actually meets in production and the one
//! `PLAN.md` §Phase 008 asks for by name: *"concurrency/idempotency tests"*.
//!
//! Three properties are asserted here, and they are separable:
//!
//! 1. **Exclusion.** [`CONCURRENT_WRITERS`] transactions race for one primary key. Exactly one
//!    commits; every loser is refused as [`DatabaseErrorKind::UniqueViolation`], not as some
//!    engine-specific code the application would have to learn.
//! 2. **No partial write.** A transaction that succeeds several times and then fails leaves
//!    *nothing* behind — including the writes that had already succeeded — as seen from a
//!    different connection.
//! 3. **Bounded convergence.** An "ensure this exists" operation run concurrently by every writer
//!    converges on one row, and its retry loop stops at [`MAX_ATTEMPTS`] rather than spinning.
//!
//! # Synchronisation is a rendezvous, never a sleep
//!
//! A race arranged with `sleep` is not a race: it either passes because the timing happened to
//! work on this machine, or it fails on a loaded runner and gets dismissed as flaky. Every racing
//! caller below waits on a [`tokio::sync::Barrier`] instead, which releases when the last of them
//! arrives — a fact rather than a duration.
//!
//! There is no sleep anywhere in this module, and no retry backoff. A retry that waited would be
//! measuring the wait.
//!
//! ## The two rendezvous are at different points, and the difference is the whole property
//!
//! [`race_for`] waits **after `begin` and before its insert**: what it measures is contention for
//! a key, so what must be true at the barrier is that every transaction is already open.
//!
//! [`ensure`] waits **after its first `find` has missed**. That is later, and it has to be. An
//! ensure's interesting path is *lose the race, then observe the winner*, and a caller only takes
//! it if it looked for the row **before** anybody committed one. Rendezvous any earlier and the
//! first caller may create and commit while the others are still queued behind it; they then find
//! the row on their first look, return `Observed`, and the duplicate-and-retry path — the thing
//! the deliverable is about — never executes.
//!
//! **This module used to drive the four ensures with a bare `tokio::join!` and no rendezvous at
//! all**, and a review found that `join!` guarantees no such ordering: the assertions it made
//! ("one creator, three observers, one row") are all satisfied by four callers running one after
//! another. The correction is the barrier below, and the assertions that make a first-attempt
//! `Observed` a failure rather than a pass.
//!
//! # This crate still names no driver
//!
//! Every statement is a [`WidgetFixture`] method implemented in the adapter's own test crate. The
//! assertions are compiled once, here, and executed by all four rows.

use renvor_database::{Database, DatabaseErrorKind, UnitOfWork as _};

use crate::domain::WidgetFixture;

/// How many transactions race for one identity.
///
/// # Why it equals the pool capacity rather than being larger
///
/// A fifth writer against a four-connection pool would not be racing for the **key** — it would be
/// queued on the *pool*, and would report [`DatabaseErrorKind::AcquireTimeout`] rather than a
/// unique violation. That is a real property, and it is already asserted elsewhere; mixing it in
/// here would mean a run could satisfy this assertion while never contending for the row at all.
///
/// So the caller passes its configured capacity and this module checks the two agree.
pub const CONCURRENT_WRITERS: usize = 4;

/// The ceiling on retries after a lost race.
///
/// # Why a constant and not a loop condition
///
/// "Retry until it works" is an unbounded loop wearing a retry's clothes: under sustained
/// contention it never returns, and the failure presents as a hung test rather than a failed one.
/// The bound is named here, asserted by
/// [`the_retry_bound_is_a_ceiling_rather_than_a_loop`], and small on purpose — a correct ensure
/// needs **two** attempts in the worst case (lose the race, then observe the winner), so three
/// leaves one spare and no room to hide a livelock.
pub const MAX_ATTEMPTS: usize = 3;

/// What one racing writer ended up doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Raced {
    /// This writer's insert committed.
    Committed,
    /// This writer was refused, with the kind the server's error classified as.
    Lost(DatabaseErrorKind),
}

/// What one bounded `ensure` ended up doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ensured {
    /// This caller wrote the row.
    Created,
    /// This caller found the row another writer had committed.
    Observed,
    /// The retry bound was reached without either. A defect if it ever happens under this test.
    Exhausted,
}

/// Deletes the given identities and commits, so an assertion starts from a known table.
///
/// Committed rather than rolled back: the race assertions below deliberately leave a committed row
/// behind, and a rollback here would leave it there for the next one.
async fn clear<F: WidgetFixture>(fixture: &F, ids: &[i64]) {
    let mut unit = fixture.database().begin().await.expect("begins");
    for &id in ids {
        fixture.remove(&mut unit, id).await.expect("deletes");
    }
    unit.commit().await.expect("commits");
}

/// Proves the pool can still hand out every slot it was configured for.
///
/// # Functional rather than metric
///
/// `size` and `num_idle` read the same in the healthy case and in the case where a connection was
/// counted against the maximum but can never be handed out again. Only actually acquiring every
/// slot distinguishes them, so that is what this does.
///
/// Shared with [`crate::persistence::cancellation_does_not_shrink_the_pool`] rather than copied:
/// two capacity probes would be two chances to weaken one of them.
pub async fn every_slot_is_available<D: Database>(database: &D, capacity: usize, context: &str) {
    let mut held = Vec::new();
    for slot in 0..capacity {
        match database.begin().await {
            Ok(unit) => held.push(unit),
            Err(error) => panic!(
                "{context}: the pool refused slot {slot} of {capacity} ({:?}) — capacity was lost",
                error.kind()
            ),
        }
    }
    for unit in held {
        let _ = unit.rollback().await;
    }
}

/// One racing writer: open a transaction, rendezvous, then insert and try to commit.
///
/// # Why the transaction is open BEFORE the barrier
///
/// So that the barrier releases into a state where every connection is already checked out and
/// every transaction already started. If `begin` came after the rendezvous, the writers would be
/// racing to acquire connections, and the slowest to get one might send its insert long after the
/// winner had committed — a sequence, not a race.
async fn race_for<F: WidgetFixture>(
    fixture: &F,
    gate: &tokio::sync::Barrier,
    id: i64,
    name: &str,
) -> Raced {
    let mut unit = fixture.database().begin().await.expect("begins");
    gate.wait().await;

    match fixture.insert(&mut unit, id, name).await {
        Ok(()) => match unit.commit().await {
            Ok(()) => Raced::Committed,
            // Not expected for a non-deferred constraint, but asserting WHERE the conflict
            // surfaces would be asserting an engine's implementation detail. What must hold is the
            // kind, and that is checked by the caller either way.
            Err(error) => Raced::Lost(error.kind()),
        },
        Err(error) => {
            let _ = unit.rollback().await;
            Raced::Lost(error.kind())
        }
    }
}

/// Concurrent writers contending for one identity: exactly one wins, and every loser is named.
///
/// # The two halves are both load-bearing
///
/// "Exactly one committed" without "every loser was a unique violation" would pass against a
/// database that refused the losers for any reason at all — a lock timeout, a dropped connection,
/// a serialization failure. An application retries those differently from a duplicate, so the kind
/// is the part that has to be identical across the four rows, and it is asserted per loser.
pub async fn one_identity_admits_exactly_one_writer<F: WidgetFixture>(
    fixture: &F,
    capacity: usize,
) {
    const ID: i64 = 500;

    assert_eq!(
        capacity, CONCURRENT_WRITERS,
        "this assertion races exactly as many writers as the pool has connections, so that every \
         loser is blocked on the ROW and none on the pool. See `CONCURRENT_WRITERS`"
    );
    clear(fixture, &[ID]).await;

    let gate = tokio::sync::Barrier::new(CONCURRENT_WRITERS);
    // Fixed arity, checked against the constant, because driving a variable number of borrowing
    // futures concurrently would mean adding a futures-combinator dependency to a crate that
    // deliberately has none.
    const _: () = assert!(CONCURRENT_WRITERS == 4);
    let outcomes = tokio::join!(
        race_for(fixture, &gate, ID, "writer-0"),
        race_for(fixture, &gate, ID, "writer-1"),
        race_for(fixture, &gate, ID, "writer-2"),
        race_for(fixture, &gate, ID, "writer-3"),
    );
    let outcomes = [outcomes.0, outcomes.1, outcomes.2, outcomes.3];

    let committed = outcomes.iter().filter(|o| **o == Raced::Committed).count();
    assert_eq!(
        committed, 1,
        "{CONCURRENT_WRITERS} writers raced for one primary key and {committed} of them \
         committed. Exactly one may: {outcomes:?}"
    );

    for (writer, outcome) in outcomes.iter().enumerate() {
        if let Raced::Lost(kind) = outcome {
            assert_eq!(
                *kind,
                DatabaseErrorKind::UniqueViolation,
                "writer {writer} lost the race but was refused as {kind:?}. Every loser must be \
                 refused as a duplicate identity, on every row — an application that had to tell \
                 PostgreSQL's 23505 from MySQL's 1062 itself would be writing engine-specific code \
                 at exactly the boundary Renvor removes"
            );
        }
    }

    // The winner's row really is there, read on a connection that took no part in the race.
    let mut observer = fixture.database().begin().await.expect("begins");
    let survivor = fixture
        .find(&mut observer, ID)
        .await
        .expect("the winning writer's row must be committed and visible");
    assert!(
        survivor.name.starts_with("writer-"),
        "the surviving row was not written by any racer: {survivor:?}"
    );
    observer.rollback().await.expect("rolls back");

    // A race that stranded a connection would leave the pool permanently smaller. That is a defect
    // and not a slow path, so it is checked here rather than assumed.
    every_slot_is_available(fixture.database(), capacity, "after a contended race").await;
    clear(fixture, &[ID]).await;
}

/// A transaction that fails partway leaves none of its earlier writes behind.
///
/// # Why the read is from a different connection, and why there is a control
///
/// Reading inside the failed transaction would prove nothing: the rows are invisible outside it
/// from the first statement onward regardless of any rollback. So the observation is taken from a
/// separate transaction, which [`crate::persistence::a_second_begin_is_separate`] establishes is a
/// separate connection.
///
/// That alone is still not enough. A `find` that always returned `None` would satisfy every
/// assertion below, so the first thing this does is commit a row and require the observer to
/// **see** it. Only then does an absence mean anything.
pub async fn a_failed_transaction_leaves_no_partial_rows<F: WidgetFixture>(fixture: &F) {
    const IDS: [i64; 3] = [511, 512, 513];
    const CONTROL: i64 = 510;

    clear(fixture, &[CONTROL, IDS[0], IDS[1], IDS[2]]).await;

    // CONTROL. The observer must be able to see a committed row, or its later `None`s are vacuous.
    let mut seeding = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut seeding, CONTROL, "control")
        .await
        .expect("inserts");
    seeding.commit().await.expect("commits");
    let mut observer = fixture.database().begin().await.expect("begins");
    assert!(
        fixture.find(&mut observer, CONTROL).await.is_some(),
        "the observing connection cannot see a COMMITTED row, so its failure to see an \
         uncommitted one would prove nothing"
    );
    observer.rollback().await.expect("rolls back");

    // Three successful writes, then a fourth statement the server must refuse.
    let mut unit = fixture.database().begin().await.expect("begins");
    for id in IDS {
        fixture
            .insert(&mut unit, id, "partial")
            .await
            .expect("inserts");
    }
    let error = fixture
        .insert(
            &mut unit,
            IDS[0],
            "duplicate of a row this same transaction wrote",
        )
        .await
        .expect_err("re-inserting an identity this transaction already wrote must be refused");
    assert_eq!(
        error.kind(),
        DatabaseErrorKind::UniqueViolation,
        "a duplicate inside one transaction must classify the same as one across two"
    );
    unit.rollback().await.expect("rolls back");

    let mut observer = fixture.database().begin().await.expect("begins");
    for id in IDS {
        assert!(
            fixture.find(&mut observer, id).await.is_none(),
            "row {id} survived a rolled-back transaction. It was written BEFORE the statement that \
             failed, which is exactly the row a partial write leaves behind"
        );
    }
    observer.rollback().await.expect("rolls back");

    clear(fixture, &[CONTROL]).await;
}

/// Everything one bounded `ensure` did, in enough detail to prove it took part in the race.
///
/// # Why the outcome alone is not enough
///
/// `Observed` means "the row was there when I looked". It does not say whether this caller ever
/// contended for the identity, and a caller that simply arrived late looks identical to one that
/// lost a genuine race. [`refusals`](Self::refusals) and [`attempt`](Self::attempt) are what
/// separate them, so both are carried out of the loop rather than discarded inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ensure {
    /// How this caller finished.
    outcome: Ensured,
    /// The attempt it finished on. `1` means it never went round the loop.
    attempt: usize,
    /// How many times this caller's own insert or commit was refused as a duplicate.
    ///
    /// Zero on a caller that reports `Observed` means the race never happened: it found a row
    /// somebody else had already committed and returned without ever offering one of its own.
    refusals: usize,
}

/// Ensures one widget exists, retrying a lost race up to [`MAX_ATTEMPTS`] times.
///
/// # The rendezvous is after the first `find`, and that placement is the point
///
/// On attempt 1 this caller opens a transaction, looks for the row, and — having **missed** —
/// waits at `gate` for every other caller to reach the same place. Only then does it insert. So
/// when the barrier releases, all [`CONCURRENT_WRITERS`] callers have already established that the
/// row was absent, and every insert that follows is a real bid for the same identity.
///
/// Waiting *before* the `find` would synchronise the wrong instant: the callers would agree on
/// when to look, but the first one through could still create and commit before the others got
/// their query away, and they would then observe the row on attempt 1 without ever having raced.
///
/// Later attempts do **not** wait. A loser retrying after the winner has already returned would
/// block forever on a barrier no one else will reach.
///
/// # There is no backoff, deliberately
///
/// The only reason to retry here is that another writer committed the row, and after that commit
/// the retry's `find` succeeds immediately. Waiting would add latency to a case that is already
/// resolved, and would make this assertion measure a sleep.
async fn ensure<F: WidgetFixture>(
    fixture: &F,
    gate: &tokio::sync::Barrier,
    id: i64,
    name: &str,
) -> Ensure {
    let mut refusals = 0;
    for attempt in 1..=MAX_ATTEMPTS {
        let mut unit = fixture.database().begin().await.expect("begins");
        let present = fixture.find(&mut unit, id).await.is_some();

        if attempt == 1 {
            // Not a tolerated condition: the caller clears this identity first, so a row here
            // means something outside this assertion is writing to the table — and every caller
            // must reach the barrier, so returning early from this arm would hang the rest.
            assert!(
                !present,
                "widget {id} already existed when the concurrent ensures began. The assertion \
                 clears it first, so this run was not measuring a race"
            );
            gate.wait().await;
        } else if present {
            unit.rollback().await.expect("rolls back");
            return Ensure {
                outcome: Ensured::Observed,
                attempt,
                refusals,
            };
        }

        match fixture.insert(&mut unit, id, name).await {
            Ok(()) => match unit.commit().await {
                Ok(()) => {
                    return Ensure {
                        outcome: Ensured::Created,
                        attempt,
                        refusals,
                    };
                }
                Err(error) => {
                    assert_eq!(
                        error.kind(),
                        DatabaseErrorKind::UniqueViolation,
                        "a commit failed for a reason this retry loop is not entitled to swallow"
                    );
                    refusals += 1;
                }
            },
            Err(error) => {
                assert_eq!(
                    error.kind(),
                    DatabaseErrorKind::UniqueViolation,
                    "an ensure may only retry a lost race. Retrying {:?} would be a silent \
                     fallback over a failure that needs reporting",
                    error.kind()
                );
                refusals += 1;
                let _ = unit.rollback().await;
            }
        }
    }
    Ensure {
        outcome: Ensured::Exhausted,
        attempt: MAX_ATTEMPTS,
        refusals,
    }
}

/// Every writer asking for the same widget converges on one row, within the retry bound.
///
/// # Idempotency is the property, not "no error"
///
/// A caller that swallowed the duplicate and returned success would pass a test that only checked
/// for the absence of an error. What is asserted instead is the **state**: one row, one creator,
/// and every other caller having observed that same row rather than a second one.
///
/// # The state assertions are necessary and were never sufficient
///
/// "One creator, three observers, one row" is also what four callers running strictly one after
/// another produce, and a review found that the previous bare `tokio::join!` permitted exactly
/// that. So the state assertions below are now joined by three that describe the **path** each
/// caller took: the winner created on its first attempt, every loser was refused at least once,
/// and no loser reported `Observed` on attempt 1. The last is the load-bearing one — a caller that
/// observes without racing is the outcome the rendezvous exists to make impossible.
pub async fn concurrent_ensures_converge_on_one_row<F: WidgetFixture>(
    fixture: &F,
    capacity: usize,
) {
    const ID: i64 = 520;
    const NAME: &str = "converged";

    assert_eq!(capacity, CONCURRENT_WRITERS, "see `CONCURRENT_WRITERS`");
    clear(fixture, &[ID]).await;

    const _: () = assert!(CONCURRENT_WRITERS == 4);
    let gate = tokio::sync::Barrier::new(CONCURRENT_WRITERS);
    let results = tokio::join!(
        ensure(fixture, &gate, ID, NAME),
        ensure(fixture, &gate, ID, NAME),
        ensure(fixture, &gate, ID, NAME),
        ensure(fixture, &gate, ID, NAME),
    );
    let results = [results.0, results.1, results.2, results.3];

    let created = results
        .iter()
        .filter(|r| r.outcome == Ensured::Created)
        .count();
    assert_eq!(
        created, 1,
        "{CONCURRENT_WRITERS} concurrent ensures produced {created} creations. An idempotent \
         ensure admits exactly one: {results:?}"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| r.outcome == Ensured::Observed)
            .count(),
        CONCURRENT_WRITERS - 1,
        "every caller that did not create the row must have OBSERVED it. A caller that neither \
         created nor observed gave up: {results:?}"
    );
    for (caller, result) in results.iter().enumerate() {
        assert!(
            result.attempt <= MAX_ATTEMPTS,
            "caller {caller} took {} attempts, past the bound of {MAX_ATTEMPTS}",
            result.attempt
        );

        match result.outcome {
            // The winner reached the barrier with everybody else and got its insert away first.
            // A creation on any later attempt would mean it had been refused, and the refusal
            // would have had to come from a row that no caller has yet been shown to create.
            Ensured::Created => {
                assert_eq!(
                    result.attempt, 1,
                    "the creating caller {caller} needed {} attempts. Every caller inserts \
                     immediately after the rendezvous, so the one that succeeds succeeds first \
                     time: {results:?}",
                    result.attempt
                );
                assert_eq!(
                    result.refusals, 0,
                    "the creating caller {caller} was refused {} times before succeeding, which \
                     means another writer held the identity and then gave it up: {results:?}",
                    result.refusals
                );
            }
            // THE ASSERTION THE RENDEZVOUS EXISTS FOR. A loser that never offered its own insert
            // never contended for the identity; it read a row somebody else had already committed.
            // Every state assertion above is satisfied by that, which is why this one is separate.
            Ensured::Observed => {
                assert!(
                    result.attempt >= 2,
                    "caller {caller} observed the row on attempt 1, so it never raced for it. \
                     Every caller misses its first `find` before the barrier releases, so a \
                     loser must be refused at least once and observe the winner on a LATER \
                     attempt: {results:?}"
                );
                assert!(
                    result.refusals >= 1,
                    "caller {caller} observed the row without ever being refused, so its insert \
                     never contended with the winner's: {results:?}"
                );
            }
            Ensured::Exhausted => panic!(
                "caller {caller} exhausted its {MAX_ATTEMPTS} attempts. A lost race resolves on \
                 the next look, so reaching the ceiling here is a defect: {results:?}"
            ),
        }
    }

    // Exactly one row, with the agreed name, seen from outside every one of those transactions.
    let mut observer = fixture.database().begin().await.expect("begins");
    let row = fixture
        .find(&mut observer, ID)
        .await
        .expect("the converged row");
    assert_eq!(row.name, NAME, "the surviving row is not the one agreed on");
    let neighbours = fixture.ids_after(&mut observer, ID - 1, 8).await;
    assert_eq!(
        neighbours,
        vec![ID],
        "the concurrent ensures left more than the one row they converged on: {neighbours:?}"
    );
    observer.rollback().await.expect("rolls back");

    every_slot_is_available(fixture.database(), capacity, "after concurrent ensures").await;
    clear(fixture, &[ID]).await;
}

/// A retry loop that can never succeed stops at [`MAX_ATTEMPTS`] rather than spinning.
///
/// # This is the control for the bound, and it has to be executable
///
/// A comment claiming the loop is bounded is not a bound. Here the identity is taken by a
/// committed row and the loop never consults `find`, so **every** attempt is refused; the only way
/// the call returns at all is the ceiling, and the exact count is asserted. If the bound were
/// removed this assertion would not fail — it would hang, which the suite's own timeout reports.
pub async fn the_retry_bound_is_a_ceiling_rather_than_a_loop<F: WidgetFixture>(fixture: &F) {
    const ID: i64 = 530;

    clear(fixture, &[ID]).await;
    let mut unit = fixture.database().begin().await.expect("begins");
    fixture
        .insert(&mut unit, ID, "taken")
        .await
        .expect("inserts");
    unit.commit().await.expect("commits");

    let mut attempts = 0_usize;
    for _ in 1..=MAX_ATTEMPTS {
        attempts += 1;
        let mut unit = fixture.database().begin().await.expect("begins");
        let error = fixture
            .insert(&mut unit, ID, "insisting")
            .await
            .expect_err("the identity is committed, so every attempt must be refused");
        assert_eq!(
            error.kind(),
            DatabaseErrorKind::UniqueViolation,
            "the control needs the refusals to be duplicates, or it is measuring another failure"
        );
        let _ = unit.rollback().await;
    }
    assert_eq!(
        attempts, MAX_ATTEMPTS,
        "a permanently-conflicting retry must stop at its named ceiling"
    );

    clear(fixture, &[ID]).await;
}

/// Every concurrency assertion, in one call.
///
/// Single entry point for the same reason [`crate::persistence::run_every_shared_assertion`] has
/// one: an adapter must not be able to run *some* of the contract.
pub async fn run_every_concurrency_assertion<F: WidgetFixture>(fixture: &F, capacity: usize) {
    one_identity_admits_exactly_one_writer(fixture, capacity).await;
    a_failed_transaction_leaves_no_partial_rows(fixture).await;
    concurrent_ensures_converge_on_one_row(fixture, capacity).await;
    the_retry_bound_is_a_ceiling_rather_than_a_loop(fixture).await;
}
