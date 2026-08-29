//! The abuse-control contract every Renvor database adapter must satisfy.
//!
//! # Why this is here and not in each adapter's tests
//!
//! The same argument [`crate::persistence`] and [`crate::refresh`] make: two suites asserting the
//! same things in two files are two suites, and they diverge the first time one is edited —
//! quietly, because a weakened assertion still passes. These functions are compiled **once** and
//! called from all four rows, so *"identical across both engines and both adapters"* is a fact
//! about the build rather than a claim in a document.
//!
//! # Why a real database is not optional here either
//!
//! [`renvor_auth::abuse::AttemptRepository::observe`] promises to increment **and** report the
//! resulting count as one atomic step. A fake whose `async fn`s contain no `.await` cannot fail
//! that promise: `tokio::join!` runs two calls one after the other, no interleaving is attempted,
//! and a broken read-then-write passes. Batch G2 shipped a HIGH race under exactly that mistake.
//!
//! So the atomicity assertions below race **real pooled connections** against a real server, and
//! there is no sleep anywhere in this module. Coordination is a `tokio::sync::Barrier` to release
//! contenders together, and after that the database's own row lock — which is the mechanism under
//! test.
//!
//! # The table being measured
//!
//! ```text
//! rv_auth_attempt   dimension, bucket, window_start, current_count, previous_count, expires_at
//!                   ^^^^^^^^^^^^^^^^^  PRIMARY KEY -- and the whole of the row bound
//! ```
//!
//! `max_rows = |AttemptDimension| × buckets`, and it holds **whether or not `prune` is ever
//! called**. Assertion 4 is that claim, executed.

use core::future::Future;

use chrono::{DateTime, Duration, Utc};
use renvor_auth::abuse::{
    AbuseContract, AbuseGuard, AttemptBucket, AttemptBuckets, AttemptDimension, AttemptFlow,
    AttemptKey, AttemptKeyring, AttemptLimit, AttemptObservation, AttemptOutcome,
    AttemptRepository, AttemptState, FlowKeys,
};
use renvor_auth::audit::{CorrelationId, RecordingAuditSink};
use renvor_core::identity::ClientIdentity;
use std::collections::BTreeSet;
use std::net::{IpAddr, Ipv6Addr};

/// How many assertions [`run_every_abuse_assertion`] runs.
///
/// A count rather than a comment. The runner tallies its calls and compares, so an assertion
/// deleted from the runner fails the suite instead of quietly reducing coverage — the census entry
/// is one line per row and cannot see inside the function.
pub const ABUSE_ASSERTIONS: usize = 12;

/// The bucket count the racing and bound assertions use.
///
/// `AttemptBuckets::MIN`, deliberately: the smallest legal space is the one where saturation is
/// reachable in a test-sized number of requests, and the bound is the same claim at every size.
const TEST_BUCKETS: u32 = 256;

/// One stored counter row, as the adapter reports it back.
///
/// **There is no field here an identifier could occupy**, which is the point: the table holds two
/// integers and three instants, and the thing that was counted is not recoverable from any of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct StoredAttempt {
    /// The window the counts belong to.
    pub window_start: DateTime<Utc>,
    /// Attempts in that window.
    pub current: u64,
    /// Attempts in the window before it.
    pub previous: u64,
    /// When the row stops being worth keeping.
    pub expires_at: DateTime<Utc>,
}

/// What one adapter must supply for the contract below to run against it.
pub trait AbuseFixture: Sync {
    /// The adapter's attempt repository.
    type Repository: AttemptRepository;

    /// The repository under test. Its calls take **their own pooled connections**, which is what
    /// lets the racing assertions be a real race rather than a simulated one.
    fn repository(&self) -> &Self::Repository;

    /// Empties `rv_auth_attempt`.
    fn reset(&self) -> impl Future<Output = ()> + Send;

    /// How many rows the table holds.
    fn row_count(&self) -> impl Future<Output = u64> + Send;

    /// One row, if it exists.
    fn row(
        &self,
        dimension: AttemptDimension,
        bucket: u32,
    ) -> impl Future<Output = Option<StoredAttempt>> + Send;

    /// Writes a row directly, so an assertion can start from a state that would otherwise take
    /// `i64::MAX` requests to reach.
    fn seed(
        &self,
        dimension: AttemptDimension,
        bucket: u32,
        row: StoredAttempt,
    ) -> impl Future<Output = ()> + Send;

    /// Every row rendered as text, **read back from the server**, for the canary sweep.
    ///
    /// Read back rather than remembered: the assertion that no identifier is stored has to look at
    /// what the server holds, not at what the adapter believes it sent.
    fn dump(&self) -> impl Future<Output = Vec<String>> + Send;
}

/// A FIXED key, so every bucket index below is a deterministic fact rather than a sample.
fn keyring() -> AttemptKeyring {
    AttemptKeyring::from_bytes(
        [0x5C; 32],
        AttemptBuckets::new(TEST_BUCKETS).expect("MIN is a legal bucket count"),
    )
}

fn at(minute: i64) -> DateTime<Utc> {
    use chrono::TimeZone as _;
    Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0)
        .single()
        .expect("a real instant")
        + Duration::seconds(minute * 60)
}

fn correlation() -> CorrelationId {
    CorrelationId::from_bytes([0xAB; 8])
}

fn network(index: u32) -> ClientIdentity {
    ClientIdentity::DirectPeer(IpAddr::V6(Ipv6Addr::new(
        0x2001,
        0xdb8,
        0,
        0,
        (index >> 16) as u16,
        (index & 0xffff) as u16,
        0,
        1,
    )))
}

/// Builds the observation the guard would build, so a direct-repository assertion measures the
/// same shape a real flow produces.
fn observation(
    dimension: AttemptDimension,
    bucket: AttemptBucket,
    limit: AttemptLimit,
    now: DateTime<Utc>,
) -> AttemptObservation {
    let window_start = limit.window_start(now);
    AttemptObservation {
        dimension,
        bucket,
        window_start,
        previous_window_start: window_start - limit.window(),
        expires_at: window_start + limit.window() + limit.window(),
    }
}

fn quarter_hour() -> AttemptLimit {
    AttemptLimit::new(10, Duration::seconds(900)).expect("a legal limit")
}

// ---- 1 --------------------------------------------------------------------------------------

/// A first attempt creates exactly one row, and that row reads one.
pub async fn a_first_attempt_creates_exactly_one_row_reading_one<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    let ring = keyring();
    let dimension = AttemptDimension::LogInNetwork;
    let bucket = ring
        .bucket(dimension, AttemptKey::Network(network(1)))
        .expect("the axes match");

    let outcome = fixture
        .repository()
        .observe(observation(dimension, bucket, quarter_hour(), at(0)))
        .await
        .expect("the store accepts");

    let AttemptOutcome::Counted(state) = outcome else {
        panic!("a first attempt was not counted: {outcome:?}");
    };
    assert_eq!(state.current, 1, "a first attempt did not read one");
    assert_eq!(state.previous, 0, "a first attempt invented a history");
    assert_eq!(fixture.row_count().await, 1, "one attempt, one row");

    // The row on the server agrees with what the call reported. An adapter that returned a count
    // it had not written would pass every in-memory assertion.
    let stored = fixture
        .row(dimension, bucket.get())
        .await
        .expect("the row exists");
    assert_eq!(stored.current, 1);
    assert_eq!(stored.previous, 0);
    assert_eq!(stored.window_start, quarter_hour().window_start(at(0)));
}

// ---- 2 --------------------------------------------------------------------------------------

/// Concurrent attempts on one bucket are each counted **exactly once**, and each caller is told a
/// different number.
///
/// # This is the assertion a fake cannot fail
///
/// Eight callers released together by a barrier, each on its own pooled connection. If `observe`
/// were a read-then-write, several would read the same value and the final count would be below
/// eight — and, worse, two callers would be told the same number, so two of them would agree they
/// were the one that crossed a threshold.
pub async fn concurrent_attempts_are_each_counted_exactly_once<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    const CONTENDERS: usize = 8;
    let ring = keyring();
    let dimension = AttemptDimension::LogInNetwork;
    let bucket = ring
        .bucket(dimension, AttemptKey::Network(network(2)))
        .expect("the axes match");
    let request = observation(dimension, bucket, quarter_hour(), at(0));

    let gate = tokio::sync::Barrier::new(CONTENDERS);
    // NO SLEEP. The barrier releases all eight together; from there the row's own lock decides the
    // order, which is the mechanism under test.
    //
    // `tokio::join!` with eight explicit arms rather than `join_all` over a `Vec`: this crate has
    // no futures combinator dependency and hosting a shared suite is not a reason to add one to
    // every consumer's graph.
    let contend = || async {
        gate.wait().await;
        fixture.repository().observe(request).await
    };
    let joined = tokio::join!(
        contend(),
        contend(),
        contend(),
        contend(),
        contend(),
        contend(),
        contend(),
        contend()
    );
    let outcomes = [
        joined.0, joined.1, joined.2, joined.3, joined.4, joined.5, joined.6, joined.7,
    ];

    let mut reported = BTreeSet::new();
    for outcome in outcomes {
        let AttemptOutcome::Counted(state) = outcome.expect("the store accepts") else {
            panic!("a concurrent attempt was not counted");
        };
        assert!(
            reported.insert(state.current),
            "two callers were told the same count {} — the increment is not atomic",
            state.current
        );
    }

    // Exactly 1..=CONTENDERS, with nothing missing and nothing repeated.
    assert_eq!(
        reported,
        (1..=CONTENDERS as u64).collect::<BTreeSet<_>>(),
        "the counts reported were not a permutation of 1..={CONTENDERS}"
    );
    let stored = fixture
        .row(dimension, bucket.get())
        .await
        .expect("the row exists");
    assert_eq!(
        stored.current, CONTENDERS as u64,
        "attempts were lost between the callers and the row"
    );
    assert_eq!(
        fixture.row_count().await,
        1,
        "contenders created extra rows"
    );
}

// ---- 3 --------------------------------------------------------------------------------------

/// With the counter one below its limit, **exactly one** of two concurrent attempts is over.
///
/// The threshold decision is derived from the returned count, so this is the property that makes
/// "the limit is a limit" true under concurrency rather than only in program order.
pub async fn exactly_one_of_two_concurrent_attempts_crosses_the_threshold<F: AbuseFixture>(
    fixture: &F,
) {
    fixture.reset().await;
    let ring = keyring();
    let limit = AttemptLimit::new(3, Duration::seconds(900)).expect("a legal limit");
    let dimension = AttemptDimension::LogInNetwork;
    let bucket = ring
        .bucket(dimension, AttemptKey::Network(network(3)))
        .expect("the axes match");
    let now = at(0);
    let window_start = limit.window_start(now);

    // Three of three already spent, so the next attempt is the one that crosses.
    fixture
        .seed(
            dimension,
            bucket.get(),
            StoredAttempt {
                window_start,
                current: 3,
                previous: 0,
                expires_at: window_start + limit.window() + limit.window(),
            },
        )
        .await;

    let request = observation(dimension, bucket, limit, now);
    let gate = tokio::sync::Barrier::new(2);
    let (first, second) = tokio::join!(
        async {
            gate.wait().await;
            fixture.repository().observe(request).await
        },
        async {
            gate.wait().await;
            fixture.repository().observe(request).await
        }
    );

    let over = [first, second]
        .into_iter()
        .map(|outcome| match outcome.expect("the store accepts") {
            AttemptOutcome::Counted(state) => limit.exceeded_by(&state, now),
            AttemptOutcome::ClockRegressed => panic!("the clock did not regress"),
        })
        .filter(|over| *over)
        .count();
    assert_eq!(
        over, 2,
        "with three of three already spent, BOTH further attempts must be over the limit"
    );

    // POSITIVE CONTROL: from one below the limit, exactly one of two concurrent attempts is
    // admitted — which is the property that would break if the increment were not atomic.
    fixture.reset().await;
    fixture
        .seed(
            dimension,
            bucket.get(),
            StoredAttempt {
                window_start,
                current: 2,
                previous: 0,
                expires_at: window_start + limit.window() + limit.window(),
            },
        )
        .await;
    let gate = tokio::sync::Barrier::new(2);
    let (first, second) = tokio::join!(
        async {
            gate.wait().await;
            fixture.repository().observe(request).await
        },
        async {
            gate.wait().await;
            fixture.repository().observe(request).await
        }
    );
    let admitted = [first, second]
        .into_iter()
        .map(|outcome| match outcome.expect("the store accepts") {
            AttemptOutcome::Counted(state) => limit.exceeded_by(&state, now),
            AttemptOutcome::ClockRegressed => panic!("the clock did not regress"),
        })
        .filter(|over| !*over)
        .count();
    assert_eq!(
        admitted, 1,
        "exactly one of two concurrent attempts may be the third"
    );
}

// ---- 4 --------------------------------------------------------------------------------------

/// **SQ-4, executed against a real table.** More distinct identifiers than there are buckets
/// cannot create more rows than there are buckets — and `prune` is never called.
pub async fn more_identifiers_than_buckets_cannot_create_more_rows_than_buckets<F: AbuseFixture>(
    fixture: &F,
) {
    fixture.reset().await;
    // 400 distinct IPv6 addresses over a 256-bucket space. The design this replaced would have
    // produced 400 rows; a routine /64 would have produced 2^64.
    const IDENTIFIERS: u32 = 400;
    let guard = AbuseGuard::new(
        fixture.repository(),
        keyring(),
        AbuseContract::default(),
        RecordingAuditSink::new(),
    );

    for index in 0..IDENTIFIERS {
        // The outcome is not the point — some of these are refused, which is correct. What is
        // measured is how many ROWS exist afterwards.
        let _ = guard
            .admit(
                AttemptFlow::ResetPassword,
                FlowKeys {
                    account: None,
                    client: None,
                    network: network(1000 + index),
                },
                correlation(),
                at(0),
            )
            .await;
    }

    let rows = fixture.row_count().await;
    assert!(
        rows <= u64::from(TEST_BUCKETS),
        "{IDENTIFIERS} identifiers created {rows} rows, above the {TEST_BUCKETS}-row bound"
    );

    // POSITIVE CONTROL: the mapping is spreading rather than collapsing everything into one row,
    // which would also satisfy the bound and would make the control useless.
    assert!(
        rows > u64::from(TEST_BUCKETS) / 4,
        "{IDENTIFIERS} identifiers reached only {rows} buckets — the mapping is not spreading"
    );

    // And the bound held with NO pruning. That is the property the replaced design could not have
    // at any bucket count, because pruning is a race against whoever chooses the insert rate.
}

// ---- 5 --------------------------------------------------------------------------------------

/// When the window rolls by one, the old count becomes the tail the weighted estimate charges.
pub async fn the_window_rolls_and_carries_the_previous_count<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    let ring = keyring();
    let limit = quarter_hour();
    let dimension = AttemptDimension::LogInNetwork;
    let bucket = ring
        .bucket(dimension, AttemptKey::Network(network(5)))
        .expect("the axes match");

    for _ in 0..4 {
        fixture
            .repository()
            .observe(observation(dimension, bucket, limit, at(0)))
            .await
            .expect("the store accepts");
    }

    // 15 minutes later: the next window.
    let outcome = fixture
        .repository()
        .observe(observation(dimension, bucket, limit, at(15)))
        .await
        .expect("the store accepts");
    let AttemptOutcome::Counted(state) = outcome else {
        panic!("the rolled attempt was not counted");
    };
    assert_eq!(state.current, 1, "the new window did not start at one");
    assert_eq!(
        state.previous, 4,
        "the previous window's count was discarded, so a boundary burst would be free"
    );
    assert_eq!(state.window_start, limit.window_start(at(15)));

    // Still ONE row. Rolling a window is an update, not an insert — which is the move that turns
    // 96 rows a day per key into one row forever.
    assert_eq!(fixture.row_count().await, 1, "rolling the window inserted");
}

// ---- 6 --------------------------------------------------------------------------------------

/// A gap of more than one window leaves no tail to charge.
pub async fn a_gap_of_more_than_one_window_discards_the_tail<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    let ring = keyring();
    let limit = quarter_hour();
    let dimension = AttemptDimension::ForgotPasswordNetwork;
    let bucket = ring
        .bucket(dimension, AttemptKey::Network(network(6)))
        .expect("the axes match");

    for _ in 0..7 {
        fixture
            .repository()
            .observe(observation(dimension, bucket, limit, at(0)))
            .await
            .expect("the store accepts");
    }

    // Two hours later — many windows away.
    let outcome = fixture
        .repository()
        .observe(observation(dimension, bucket, limit, at(120)))
        .await
        .expect("the store accepts");
    let AttemptOutcome::Counted(state) = outcome else {
        panic!("the attempt was not counted");
    };
    assert_eq!(state.current, 1);
    assert_eq!(
        state.previous, 0,
        "a count from two hours ago was charged against the current window"
    );
    assert_eq!(fixture.row_count().await, 1);
}

// ---- 7 --------------------------------------------------------------------------------------

/// A saturated counter neither wraps nor makes the engine raise.
///
/// # Why this is a real risk and not a theoretical one
///
/// `SET current_count = current_count + 1` in SQL raises at the ceiling — PostgreSQL `22003`,
/// MySQL `1264` in strict mode. Turning a rate-limit check on an unauthenticated endpoint into a
/// server error is worse than the overflow it prevents. The arithmetic is therefore in Rust, with
/// `saturating_add` and a ceiling that fits `BIGINT`.
pub async fn a_saturated_counter_neither_wraps_nor_errors<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    let ring = keyring();
    let limit = quarter_hour();
    let dimension = AttemptDimension::LogInNetwork;
    let bucket = ring
        .bucket(dimension, AttemptKey::Network(network(7)))
        .expect("the axes match");
    let window_start = limit.window_start(at(0));

    fixture
        .seed(
            dimension,
            bucket.get(),
            StoredAttempt {
                window_start,
                current: AttemptState::CEILING,
                previous: 0,
                expires_at: window_start + limit.window() + limit.window(),
            },
        )
        .await;

    let outcome = fixture
        .repository()
        .observe(observation(dimension, bucket, limit, at(0)))
        .await
        .expect("the store did not raise at the ceiling");
    let AttemptOutcome::Counted(state) = outcome else {
        panic!("the saturated attempt was not counted");
    };
    assert_eq!(
        state.current,
        AttemptState::CEILING,
        "the counter moved past its ceiling"
    );
    assert!(
        limit.exceeded_by(&state, at(0)),
        "a saturated counter must stay refused"
    );

    // The stored value is still representable, so the next read cannot be a negative count.
    let stored = fixture
        .row(dimension, bucket.get())
        .await
        .expect("the row exists");
    assert_eq!(stored.current, AttemptState::CEILING);
    assert!(i64::try_from(stored.current).is_ok(), "the column wrapped");
}

// ---- 8 --------------------------------------------------------------------------------------

/// A stored window in the future is refused, and the row is left exactly as it was.
pub async fn a_backwards_clock_is_refused_and_writes_nothing<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    let ring = keyring();
    let limit = quarter_hour();
    let dimension = AttemptDimension::LogInNetwork;
    let bucket = ring
        .bucket(dimension, AttemptKey::Network(network(8)))
        .expect("the axes match");

    for _ in 0..3 {
        fixture
            .repository()
            .observe(observation(dimension, bucket, limit, at(60)))
            .await
            .expect("the store accepts");
    }
    let before = fixture
        .row(dimension, bucket.get())
        .await
        .expect("the row exists");

    // The clock moves an hour backwards.
    let outcome = fixture
        .repository()
        .observe(observation(dimension, bucket, limit, at(0)))
        .await
        .expect("the store accepts");
    assert_eq!(
        outcome,
        AttemptOutcome::ClockRegressed,
        "a backwards clock was counted"
    );

    let after = fixture
        .row(dimension, bucket.get())
        .await
        .expect("the row exists");
    assert_eq!(
        before, after,
        "a backwards clock rewrote the row, erasing evidence"
    );
}

// ---- 9 --------------------------------------------------------------------------------------

/// `prune` removes expired rows **inside its bucket range** and nothing else.
pub async fn a_bounded_prune_removes_only_expired_rows_in_its_range<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    let limit = quarter_hour();
    let dimension = AttemptDimension::LogInNetwork;
    let stale = at(-600);
    let fresh = at(0);

    // Buckets 0..8 expired; buckets 8..16 live. Seeded directly, because reaching sixteen chosen
    // buckets through the keyed mapping would mean searching for pre-images.
    for bucket in 0..8_u32 {
        fixture
            .seed(
                dimension,
                bucket,
                StoredAttempt {
                    window_start: stale,
                    current: 3,
                    previous: 0,
                    expires_at: stale + limit.window() + limit.window(),
                },
            )
            .await;
    }
    for bucket in 8..16_u32 {
        fixture
            .seed(
                dimension,
                bucket,
                StoredAttempt {
                    window_start: fresh,
                    current: 3,
                    previous: 0,
                    expires_at: fresh + limit.window() + limit.window(),
                },
            )
            .await;
    }
    assert_eq!(fixture.row_count().await, 16);

    // A range covering only the first four expired rows: bounded work, chosen by the caller.
    let removed = fixture
        .repository()
        .prune(dimension, 0, 4, fresh)
        .await
        .expect("the store accepts");
    assert_eq!(removed, 4, "the range deleted the wrong number of rows");
    assert_eq!(fixture.row_count().await, 12);

    // Now the whole table, including the live rows' range.
    let removed = fixture
        .repository()
        .prune(dimension, 0, TEST_BUCKETS, fresh)
        .await
        .expect("the store accepts");
    assert_eq!(removed, 4, "an unexpired row was pruned");
    assert_eq!(
        fixture.row_count().await,
        8,
        "pruning removed a live counter, which would erase evidence"
    );

    // POSITIVE CONTROL: the live rows go once they DO expire, so the survivals above are about
    // expiry rather than about a delete that never matches.
    let removed = fixture
        .repository()
        .prune(dimension, 0, TEST_BUCKETS, at(600))
        .await
        .expect("the store accepts");
    assert_eq!(removed, 8);
    assert_eq!(fixture.row_count().await, 0);
}

// ---- 10 -------------------------------------------------------------------------------------

/// A successful admission never clears, decrements, or deletes a counter. FR-068.
pub async fn a_successful_admission_never_clears_a_counter<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    let ring = keyring();
    let guard = AbuseGuard::new(
        fixture.repository(),
        keyring(),
        AbuseContract::default(),
        RecordingAuditSink::new(),
    );
    let address = network(10);

    for _ in 0..5 {
        let _admission = guard
            .admit(
                AttemptFlow::ResetPassword,
                FlowKeys {
                    account: None,
                    client: None,
                    network: address,
                },
                correlation(),
                at(0),
            )
            .await
            .expect("well under the limit");
    }

    let bucket = ring
        .bucket(
            AttemptDimension::ResetPasswordNetwork,
            AttemptKey::Network(address),
        )
        .expect("the axes match");
    let stored = fixture
        .row(AttemptDimension::ResetPasswordNetwork, bucket.get())
        .await
        .expect("the row exists");
    assert_eq!(
        stored.current, 5,
        "five successful admissions did not leave five counted — success erased evidence"
    );
}

// ---- 11 -------------------------------------------------------------------------------------

/// A known and an unknown identifier are indistinguishable in what they store and in what they
/// are told.
///
/// Nothing in the control knows whether an account exists, so there is no difference to observe.
/// The assertion is that this remains true through a real store: the same number of rows, the same
/// counts, and the same sequence of admissions and refusals.
pub async fn a_known_and_an_unknown_identifier_are_indistinguishable_in_storage<F: AbuseFixture>(
    fixture: &F,
) {
    let contract = AbuseContract::default().with(
        AttemptDimension::ForgotPasswordAccount,
        AttemptLimit::new(2, Duration::seconds(3600)).expect("a legal limit"),
    );

    let mut transcripts = Vec::new();
    let mut shapes = Vec::new();
    for identifier in ["ada@example.test", "certainly-nobody@example.test"] {
        fixture.reset().await;
        let guard = AbuseGuard::new(
            fixture.repository(),
            keyring(),
            contract,
            RecordingAuditSink::new(),
        );
        let mut transcript = Vec::new();
        for index in 0..4 {
            let outcome = guard
                .admit(
                    AttemptFlow::ForgotPassword,
                    FlowKeys {
                        account: Some(identifier),
                        client: None,
                        // A distinct address per attempt, so the network axis cannot be what
                        // refuses and mask a difference on the account axis.
                        network: network(2000 + index),
                    },
                    correlation(),
                    at(0),
                )
                .await;
            transcript.push(outcome.is_ok());
        }
        transcripts.push(transcript);
        shapes.push(fixture.row_count().await);
    }

    assert_eq!(
        transcripts[0], transcripts[1],
        "a known and an unknown identifier were admitted differently"
    );
    assert_eq!(
        shapes[0], shapes[1],
        "a known and an unknown identifier left different amounts of state"
    );
    // And the shape is the one the contract promises.
    assert_eq!(transcripts[0], vec![true, true, false, false]);
}

// ---- 12 -------------------------------------------------------------------------------------

/// No identifier reaches storage in any form — not raw, not hashed, not encoded.
///
/// # The canary was never handed to the store, and that is the argument
///
/// The identifier is mapped to an integer before any statement is composed. The sweep below reads
/// every row back **from the server** and looks for it, which is what turns "the code does not send
/// it" into "the server does not have it".
pub async fn no_identifier_reaches_storage_in_any_form<F: AbuseFixture>(fixture: &F) {
    fixture.reset().await;
    const CANARY: &str = "canary-9f3a@example.test";
    let guard = AbuseGuard::new(
        fixture.repository(),
        keyring(),
        AbuseContract::default(),
        RecordingAuditSink::new(),
    );

    let _admission = guard
        .admit(
            AttemptFlow::ForgotPassword,
            FlowKeys {
                account: Some(CANARY),
                client: None,
                network: network(12),
            },
            correlation(),
            at(0),
        )
        .await
        .expect("the first attempt is admitted");

    let dumped = fixture.dump().await;
    assert!(!dumped.is_empty(), "the sweep read nothing back");
    let joined = dumped.join("\n");
    for fragment in [CANARY, "canary", "9f3a", "example.test", "2001:db8"] {
        assert!(
            !joined.contains(fragment),
            "the abuse table holds {fragment:?}: {joined}"
        );
    }

    // POSITIVE CONTROL: the sweep can find something that IS there, so the absences above are
    // facts about the table rather than about a search that never matches.
    assert!(
        joined.contains(&AttemptDimension::ForgotPasswordAccount.code().to_string()),
        "the sweep did not find the dimension code, so it is not reading the rows"
    );
}

/// Runs every assertion in this module against one row.
///
/// The four call sites — direct-SQLx × {PostgreSQL, MySQL} and SeaORM × {PostgreSQL, MySQL} — call
/// **this** function, so requirement 12 of the batch brief ("the behavior remains identical across
/// both engines and both adapters") is the single compiled copy rather than a thirteenth assertion.
pub async fn run_every_abuse_assertion<F: AbuseFixture>(fixture: &F) {
    let mut ran = 0_usize;

    a_first_attempt_creates_exactly_one_row_reading_one(fixture).await;
    ran += 1;
    concurrent_attempts_are_each_counted_exactly_once(fixture).await;
    ran += 1;
    exactly_one_of_two_concurrent_attempts_crosses_the_threshold(fixture).await;
    ran += 1;
    more_identifiers_than_buckets_cannot_create_more_rows_than_buckets(fixture).await;
    ran += 1;
    the_window_rolls_and_carries_the_previous_count(fixture).await;
    ran += 1;
    a_gap_of_more_than_one_window_discards_the_tail(fixture).await;
    ran += 1;
    a_saturated_counter_neither_wraps_nor_errors(fixture).await;
    ran += 1;
    a_backwards_clock_is_refused_and_writes_nothing(fixture).await;
    ran += 1;
    a_bounded_prune_removes_only_expired_rows_in_its_range(fixture).await;
    ran += 1;
    a_successful_admission_never_clears_a_counter(fixture).await;
    ran += 1;
    a_known_and_an_unknown_identifier_are_indistinguishable_in_storage(fixture).await;
    ran += 1;
    no_identifier_reaches_storage_in_any_form(fixture).await;
    ran += 1;

    // THE POSITIVE CONTROL. The census entry for this suite is one line per row and cannot see
    // inside this function, so a deleted call would reduce coverage with every gate still green.
    assert_eq!(
        ran, ABUSE_ASSERTIONS,
        "the abuse contract declares {ABUSE_ASSERTIONS} assertions and ran {ran}"
    );
}
