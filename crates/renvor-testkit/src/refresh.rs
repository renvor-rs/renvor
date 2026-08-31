//! The refresh-rotation contract every Renvor database adapter must satisfy.
//!
//! # Why this is here and not in each adapter's tests
//!
//! The same argument [`crate::persistence`] makes: *"two suites that assert the same things in two
//! files are two suites, and they diverge the first time one is edited — quietly, because a
//! weakened assertion still passes."* The functions below are compiled once and called from all
//! four rows, so *"identical across both engines and both adapters"* is a fact about the build.
//!
//! # Why a real database is not optional here
//!
//! It is not that a fake would be less convenient. A fake **cannot** fail these assertions.
//!
//! The defect this suite was written for shipped with a green unit suite: the rotation service
//! called `consume`, then minted, then `issue`d, and a concurrent replay could revoke the family
//! in the gap — leaving the winner's successor live in a family that had just been killed. The
//! unit test for it joined two rotations against an in-memory store whose `async fn`s contain no
//! `.await`. `tokio::join!` therefore ran them one after the other, no interleaving was ever
//! attempted, and the test passed.
//!
//! So every assertion below runs against a real server, and the ones that race use **two pooled
//! connections**. There is no sleep anywhere in this module: the coordination is either the
//! database's own row lock — which is the mechanism under test — or program order.
//!
//! # The transition being measured
//!
//! ```text
//! rv_auth_refresh_family   id, user_id, scopes, created_at, expires_at, revoked_at
//!        ^ the immutable grant, and the monotonic tombstone
//!        |
//! rv_auth_refresh          id, family_id, token_hash, issued_at, expires_at,
//!                          consumed_at, replaced_by, revoked_at
//! ```
//!
//! [`renvor_auth::repository::RefreshTokenRepository::advance`] locks the family, then the token, revalidates both, and
//! either consumes and inserts in one transaction or refuses. Everything below is a consequence of
//! that being one transaction rather than three statements.

use core::future::Future;

use chrono::{DateTime, Duration, Utc};
use renvor_auth::opaque::SecretDigest;
use renvor_auth::refresh::{
    AdvanceRequest, FamilyId, NewRefreshFamily, RefreshToken, RefreshTokenId, RefreshTransition,
};
use renvor_auth::repository::RefreshTokenRepository;
use renvor_auth::subject::UserId;
use renvor_auth::token::{Scope, ScopeSet};
use renvor_core::observe::entropy::{EntropySource, OsEntropy};

/// How many assertions [`run_every_refresh_assertion`] runs.
///
/// A count rather than a comment. The runner tallies what it called and compares, so an assertion
/// deleted from the runner fails the suite instead of quietly reducing coverage — the census
/// entry is per-row and cannot see inside this function.
pub const REFRESH_ASSERTIONS: usize = 12;

/// One stored token row, as the adapter reports it back for assertions.
///
/// **Carries the digest, never the token.** There is no field here a raw refresh token could
/// occupy, which is the same argument `NewRefreshFamily` makes about a write.
#[derive(Clone, Debug)]
pub struct StoredRefreshToken {
    /// What the store holds in the token's place.
    pub digest: SecretDigest,
    /// When it stops being valid.
    pub expires_at: DateTime<Utc>,
    /// When it was spent, if it was.
    pub consumed_at: Option<DateTime<Utc>>,
    /// When it was revoked, if it was.
    pub revoked_at: Option<DateTime<Utc>>,
    /// The successor that replaced it, if one did.
    pub replaced_by: Option<RefreshTokenId>,
}

/// The few driver-specific operations a refresh-contract run needs.
///
/// Deliberately tiny, and deliberately read-only apart from [`Self::create_user`] and
/// [`Self::reset`]: anything expressible through [`RefreshTokenRepository`] is an assertion below
/// rather than a method here. A fixture that grew its own `advance` would be re-implementing the
/// contract instead of being measured against it.
pub trait RefreshFixture: Sync {
    /// The adapter's refresh repository.
    type Repository: RefreshTokenRepository;

    /// The repository under test. Its calls take **their own pooled connections**, which is what
    /// lets the racing assertions be a real race rather than a simulated one.
    fn repository(&self) -> &Self::Repository;

    /// Empties the refresh tables and the users they reference.
    fn reset(&self) -> impl Future<Output = ()> + Send;

    /// Creates a user row for a family to reference, and returns its identity.
    fn create_user(&self) -> impl Future<Output = UserId> + Send;

    /// Every `token_hash` value the table holds, as **raw bytes read back from the database**.
    ///
    /// Read back rather than remembered: the assertion that the secret is not stored has to look
    /// at what the server has, not at what the adapter believes it sent.
    fn stored_token_hashes(&self) -> impl Future<Output = Vec<Vec<u8>>> + Send;

    /// Every token row in `family`, oldest first.
    fn tokens_in(&self, family: FamilyId) -> impl Future<Output = Vec<StoredRefreshToken>> + Send;

    /// The family's tombstone, if it is set.
    fn family_revoked_at(
        &self,
        family: FamilyId,
    ) -> impl Future<Output = Option<DateTime<Utc>>> + Send;

    /// The family's stored scope claim, exactly as the column holds it.
    fn family_scope_claim(&self, family: FamilyId) -> impl Future<Output = Option<String>> + Send;

    /// The family's stored subject.
    fn family_user(&self, family: FamilyId) -> impl Future<Output = Option<UserId>> + Send;
}

/// A fixed instant every assertion measures from, so nothing depends on the wall clock.
fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000 + seconds, 0).expect("a representable instant")
}

/// The scope set every family below is granted.
fn granted_scopes() -> ScopeSet {
    ScopeSet::new([
        Scope::new("read").expect("a well-formed scope"),
        Scope::new("write").expect("a well-formed scope"),
    ])
    .expect("a bounded scope set")
}

/// A generated token and the identity of the row that will hold it.
struct Prepared {
    id: RefreshTokenId,
    token: RefreshToken,
}

fn prepare(entropy: &dyn EntropySource) -> Prepared {
    Prepared {
        id: RefreshTokenId::generate(entropy).expect("a generated row identity"),
        token: RefreshToken::generate(entropy).expect("a generated token"),
    }
}

/// Starts a family whose first token expires at `token_expires_at`.
async fn begin_family<F: RefreshFixture>(
    fixture: &F,
    entropy: &dyn EntropySource,
    token_expires_at: DateTime<Utc>,
    family_expires_at: DateTime<Utc>,
) -> (FamilyId, Prepared) {
    let user = fixture.create_user().await;
    let family = FamilyId::generate(entropy).expect("a generated family identity");
    let first = prepare(entropy);
    fixture
        .repository()
        .begin_family(NewRefreshFamily {
            family,
            user,
            scopes: granted_scopes(),
            first_token_id: first.id,
            first_token: first.token.digest(),
            created_at: at(0),
            family_expires_at,
            token_expires_at,
        })
        .await
        .expect("a family begins");
    (family, first)
}

/// A family whose first token is comfortably live.
async fn live_family<F: RefreshFixture>(
    fixture: &F,
    entropy: &dyn EntropySource,
) -> (FamilyId, Prepared) {
    begin_family(fixture, entropy, at(3600), at(86_400)).await
}

/// An `advance` request replacing `presented` with `successor` at `now`.
fn request<'a>(
    presented: &'a SecretDigest,
    successor: &'a SecretDigest,
    successor_id: RefreshTokenId,
    now: DateTime<Utc>,
) -> AdvanceRequest<'a> {
    AdvanceRequest {
        presented,
        successor_id,
        successor,
        successor_expires_at: now + Duration::hours(1),
        now,
    }
}

/// **1.** A family begins with exactly one usable refresh token.
pub async fn a_family_begins_with_one_usable_token<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;

    let rows = fixture.tokens_in(family).await;
    assert_eq!(rows.len(), 1, "a new family holds more than one token row");
    let row = &rows[0];
    assert!(row.digest.matches(&first.token.digest()));
    assert!(row.consumed_at.is_none(), "a new token is already spent");
    assert!(row.revoked_at.is_none(), "a new token is already revoked");
    assert!(
        row.replaced_by.is_none(),
        "a new token already has a successor"
    );
    assert!(
        fixture.family_revoked_at(family).await.is_none(),
        "a new family carries a tombstone"
    );
}

/// **2.** A valid rotation consumes the old token and creates exactly one successor.
pub async fn a_rotation_consumes_the_old_and_creates_one_successor<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;
    let second = prepare(&entropy);

    let transition = fixture
        .repository()
        .advance(request(
            &first.token.digest(),
            &second.token.digest(),
            second.id,
            at(1),
        ))
        .await
        .expect("the transition ran");
    assert_eq!(transition, RefreshTransition::Advanced);

    let rows = fixture.tokens_in(family).await;
    assert_eq!(rows.len(), 2, "the rotation did not leave exactly two rows");
    let old = rows
        .iter()
        .find(|row| row.digest.matches(&first.token.digest()))
        .expect("the predecessor is still there");
    let new = rows
        .iter()
        .find(|row| row.digest.matches(&second.token.digest()))
        .expect("the successor was written");
    assert_eq!(
        old.consumed_at,
        Some(at(1)),
        "the predecessor was not spent"
    );
    assert_eq!(
        old.replaced_by,
        Some(second.id),
        "the predecessor does not point at its successor"
    );
    assert!(new.consumed_at.is_none(), "the successor is already spent");
    assert!(new.revoked_at.is_none(), "the successor is already revoked");
    assert!(
        fixture.family_revoked_at(family).await.is_none(),
        "an ordinary rotation revoked the family"
    );
}

/// **3.** Two concurrent presentations of one token produce exactly one rotation decision.
///
/// # The coordination is the database's own lock
///
/// Both futures run on one task through `join!`, each taking its **own pooled connection**. The
/// barrier releases them together; from there the family row's `FOR UPDATE` decides the order.
/// There is no sleep and no retry: whichever interleaving occurs — including full serialisation —
/// exactly one caller may consume the row, so the assertion is a property rather than a timing.
pub async fn two_concurrent_presentations_produce_one_rotation<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;
    let left = prepare(&entropy);
    let right = prepare(&entropy);
    let presented = first.token.digest();
    let gate = tokio::sync::Barrier::new(2);

    let one = async {
        gate.wait().await;
        fixture
            .repository()
            .advance(request(&presented, &left.token.digest(), left.id, at(1)))
            .await
            .expect("the transition ran")
    };
    let two = async {
        gate.wait().await;
        fixture
            .repository()
            .advance(request(&presented, &right.token.digest(), right.id, at(1)))
            .await
            .expect("the transition ran")
    };
    let (first_outcome, second_outcome) = tokio::join!(one, two);

    let advanced = [first_outcome, second_outcome]
        .iter()
        .filter(|outcome| matches!(outcome, RefreshTransition::Advanced))
        .count();
    assert_eq!(
        advanced, 1,
        "two concurrent presentations of one token produced {advanced} rotations"
    );
    let replays = [first_outcome, second_outcome]
        .iter()
        .filter(|outcome| matches!(outcome, RefreshTransition::Replayed { .. }))
        .count();
    assert_eq!(
        replays, 1,
        "the losing presentation was not treated as a replay"
    );

    // ASVS V10.4.5: the loser's replay took the family down, so nothing survives.
    assert!(
        fixture.family_revoked_at(family).await.is_some(),
        "a detected replay left the family live"
    );
    let usable = fixture
        .tokens_in(family)
        .await
        .into_iter()
        .filter(|row| row.consumed_at.is_none() && row.revoked_at.is_none())
        .count();
    assert_eq!(usable, 0, "a usable token survived the replay response");
}

/// **4.** A replay revokes the successor and the whole family, in one transaction.
pub async fn a_replay_revokes_the_successor_and_the_family<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;
    let second = prepare(&entropy);
    let third = prepare(&entropy);

    assert_eq!(
        fixture
            .repository()
            .advance(request(
                &first.token.digest(),
                &second.token.digest(),
                second.id,
                at(1)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Advanced
    );

    // The SAME token again.
    let replay = fixture
        .repository()
        .advance(request(
            &first.token.digest(),
            &third.token.digest(),
            third.id,
            at(2),
        ))
        .await
        .expect("the transition ran");
    assert_eq!(
        replay,
        RefreshTransition::Replayed { revoked: 2 },
        "the replay response did not revoke both rows in the family"
    );

    assert_eq!(
        fixture.family_revoked_at(family).await,
        Some(at(2)),
        "the replay left no tombstone"
    );
    let rows = fixture.tokens_in(family).await;
    assert_eq!(rows.len(), 2, "the replay wrote a third token row");
    assert!(
        rows.iter().all(|row| row.revoked_at == Some(at(2))),
        "a row in the family survived the replay response"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.digest.matches(&third.token.digest())),
        "the replay inserted the successor it was refusing"
    );
}

/// **5.** The original dangerous ordering, forced.
///
/// # What this reproduces
///
/// The defect, exactly: a caller reads the grant and prepares a successor; a concurrent caller
/// detects a replay and revokes the family; the first caller then tries to write. Under the old
/// three-statement design the write landed and the revoked family had a live token again.
///
/// The pause is expressed as **program order** rather than as a barrier, which is stronger: there
/// is no interleaving to hope for, and the write is attempted strictly after the revocation has
/// committed. The successor's secret was generated before the revocation existed.
pub async fn a_successor_prepared_before_a_revocation_cannot_land_after_it<F: RefreshFixture>(
    fixture: &F,
) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;

    // The winner reads and prepares. Nothing is durable.
    let paused = fixture
        .repository()
        .grant_for(&first.token.digest())
        .await
        .expect("the grant reads")
        .expect("the presented token is known");
    assert_eq!(paused.family, family);
    let stranded = prepare(&entropy);

    // ---- the winner is paused here ----

    // The loser rotates, then replays, which revokes the family.
    let loser = prepare(&entropy);
    assert_eq!(
        fixture
            .repository()
            .advance(request(
                &first.token.digest(),
                &loser.token.digest(),
                loser.id,
                at(1)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Advanced
    );
    let replay = prepare(&entropy);
    assert!(matches!(
        fixture
            .repository()
            .advance(request(
                &first.token.digest(),
                &replay.token.digest(),
                replay.id,
                at(2)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Replayed { .. }
    ));

    // ---- the winner is released ----

    let late = fixture
        .repository()
        .advance(request(
            &first.token.digest(),
            &stranded.token.digest(),
            stranded.id,
            at(3),
        ))
        .await
        .expect("the transition ran");
    assert_eq!(
        late,
        RefreshTransition::FamilyRevoked,
        "a write prepared before the revocation was accepted after it"
    );

    let rows = fixture.tokens_in(family).await;
    assert!(
        !rows
            .iter()
            .any(|row| row.digest.matches(&stranded.token.digest())),
        "THE DEFECT: the stranded successor was inserted into a revoked family"
    );
    let usable = rows
        .iter()
        .filter(|row| row.consumed_at.is_none() && row.revoked_at.is_none())
        .count();
    assert_eq!(usable, 0, "a revoked family has a usable descendant");
}

/// **6.** A live token in a revoked family cannot produce a successor.
///
/// Distinct from assertion 5: there the predecessor had been spent, so a replay check could have
/// refused it for the wrong reason. Here the predecessor is **live and unconsumed**, and the only
/// thing standing between it and a new row is the family's tombstone.
pub async fn a_late_insert_into_a_revoked_family_is_refused<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;

    let revoked = fixture
        .repository()
        .revoke_family(family, at(1))
        .await
        .expect("the family is revoked");
    assert_eq!(revoked, 1, "the deliberate revocation affected no row");

    let successor = prepare(&entropy);
    let transition = fixture
        .repository()
        .advance(request(
            &first.token.digest(),
            &successor.token.digest(),
            successor.id,
            at(2),
        ))
        .await
        .expect("the transition ran");
    assert_eq!(
        transition,
        RefreshTransition::FamilyRevoked,
        "a live token in a revoked family was rotated"
    );
    assert_eq!(
        fixture.tokens_in(family).await.len(),
        1,
        "a successor was inserted into a revoked family"
    );
}

/// **7.** The tombstone is monotonic and cannot be undone.
pub async fn family_revocation_is_monotonic<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, _first) = live_family(fixture, &entropy).await;

    assert_eq!(
        fixture
            .repository()
            .revoke_family(family, at(10))
            .await
            .expect("the family is revoked"),
        1
    );
    assert_eq!(fixture.family_revoked_at(family).await, Some(at(10)));

    // Later, earlier, and again — none of them may move it.
    for instant in [at(20), at(5), at(10)] {
        assert_eq!(
            fixture
                .repository()
                .revoke_family(family, instant)
                .await
                .expect("a repeated revocation answers"),
            0,
            "a repeated revocation changed a row that was already revoked"
        );
        assert_eq!(
            fixture.family_revoked_at(family).await,
            Some(at(10)),
            "a repeated revocation moved the tombstone"
        );
    }
}

/// **8.** The subject and the canonical scope claim survive rotation exactly, and cannot widen.
pub async fn the_subject_and_scopes_survive_rotation_exactly<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;
    let before = fixture
        .repository()
        .grant_for(&first.token.digest())
        .await
        .expect("the grant reads")
        .expect("the presented token is known");
    assert_eq!(before.scopes, granted_scopes());
    // CANONICAL: sorted and space-delimited, exactly as `ScopeSet::to_claim` produces it.
    assert_eq!(
        fixture.family_scope_claim(family).await.as_deref(),
        Some("read write"),
        "the stored claim is not the canonical form"
    );

    let second = prepare(&entropy);
    assert_eq!(
        fixture
            .repository()
            .advance(request(
                &first.token.digest(),
                &second.token.digest(),
                second.id,
                at(1)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Advanced
    );

    let after = fixture
        .repository()
        .grant_for(&second.token.digest())
        .await
        .expect("the grant reads")
        .expect("the successor is known");
    assert_eq!(after.user, before.user, "the rotation changed the subject");
    assert_eq!(
        after.scopes, before.scopes,
        "the rotation changed the scopes"
    );
    assert_eq!(
        after.family, before.family,
        "the rotation changed the family"
    );
    assert_eq!(
        after.family_expires_at, before.family_expires_at,
        "the rotation extended the chain's absolute end"
    );
    assert_eq!(
        fixture.family_user(family).await,
        Some(before.user),
        "the stored subject is not the one the grant reports"
    );
    // The scope claim is on the FAMILY and the rotation never writes it, so there is no statement
    // a widened set could have come from.
    assert_eq!(
        fixture.family_scope_claim(family).await.as_deref(),
        Some("read write")
    );
}

/// **9.** An unknown or expired token changes nothing, in its own family or in anyone else's.
pub async fn an_unknown_or_expired_token_disturbs_no_family<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    // One family whose token has already expired, and one that is healthy.
    let (stale, expired) = begin_family(fixture, &entropy, at(10), at(86_400)).await;
    let (healthy, live) = live_family(fixture, &entropy).await;

    let successor = prepare(&entropy);
    assert_eq!(
        fixture
            .repository()
            .advance(request(
                &expired.token.digest(),
                &successor.token.digest(),
                successor.id,
                at(11)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Unusable,
        "an expired token was rotated"
    );
    // An expired token is NOT a replay. Nothing is revoked.
    assert!(
        fixture.family_revoked_at(stale).await.is_none(),
        "an expired token revoked its own family"
    );
    assert_eq!(fixture.tokens_in(stale).await.len(), 1);

    let stranger = prepare(&entropy);
    let ghost = prepare(&entropy);
    assert_eq!(
        fixture
            .repository()
            .advance(request(
                &stranger.token.digest(),
                &ghost.token.digest(),
                ghost.id,
                at(12)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Unusable,
        "an unknown digest was rotated"
    );
    assert!(
        fixture
            .repository()
            .grant_for(&stranger.token.digest())
            .await
            .expect("the grant reads")
            .is_none(),
        "an unknown digest reported a grant"
    );

    // The healthy family is untouched by either.
    assert!(fixture.family_revoked_at(healthy).await.is_none());
    let rows = fixture.tokens_in(healthy).await;
    assert_eq!(rows.len(), 1, "another family gained or lost a row");
    assert!(rows[0].digest.matches(&live.token.digest()));
    assert!(rows[0].consumed_at.is_none());
}

/// **10.** No raw refresh token reaches storage — checked against what the **server** holds.
pub async fn no_raw_refresh_token_reaches_storage<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;
    let second = prepare(&entropy);
    assert_eq!(
        fixture
            .repository()
            .advance(request(
                &first.token.digest(),
                &second.token.digest(),
                second.id,
                at(1)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Advanced
    );

    let wire = [first.token.expose(), second.token.expose()];
    let stored = fixture.stored_token_hashes().await;
    assert_eq!(stored.len(), 2, "the table does not hold the two rows");
    for held in &stored {
        for exposed in &wire {
            assert_ne!(
                held.as_slice(),
                exposed.as_bytes(),
                "the table holds a presented value rather than its digest"
            );
            // Not a prefix either: a truncated copy of a secret is still a copy of a secret.
            assert!(
                !exposed.as_bytes().starts_with(held.as_slice()),
                "the stored value is a prefix of the token"
            );
        }
        assert_eq!(held.len(), 32, "the stored value is not a SHA-256 digest");
    }

    // And nothing that renders these values renders the secret.
    for exposed in &wire {
        let rendered = format!(
            "{:?} {:?} {:?}",
            fixture.tokens_in(family).await,
            first.token,
            second.token
        );
        assert!(
            !rendered.contains(exposed.as_str()),
            "a diagnostic rendered a live refresh token"
        );
    }
}

/// **11.** A failing transition rolls back **both** halves.
///
/// # How the failure is forced
///
/// The successor's digest is set to one the table already holds, so the insert violates
/// `token_hash`'s unique constraint. That is a real server-side failure in the second half of the
/// transition, after the first half has already written — exactly the shape that leaves a consumed
/// predecessor with no successor if the two are not one transaction.
pub async fn a_failed_transition_rolls_back_both_halves<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;

    // A second family, so the colliding digest is not this family's own.
    let (_other, foreign) = live_family(fixture, &entropy).await;
    let clash = prepare(&entropy);

    let failure = fixture
        .repository()
        .advance(request(
            &first.token.digest(),
            &foreign.token.digest(),
            clash.id,
            at(1),
        ))
        .await;
    assert!(
        failure.is_err(),
        "a duplicate token digest was accepted as a successor"
    );

    let rows = fixture.tokens_in(family).await;
    assert_eq!(
        rows.len(),
        1,
        "the failed transition left a successor behind"
    );
    assert!(
        rows[0].consumed_at.is_none(),
        "the failed transition consumed the predecessor and rolled back nothing"
    );
    assert!(
        rows[0].replaced_by.is_none(),
        "the failed transition recorded a successor that does not exist"
    );

    // And the predecessor is still usable, which is the point of rolling back.
    let successor = prepare(&entropy);
    assert_eq!(
        fixture
            .repository()
            .advance(request(
                &first.token.digest(),
                &successor.token.digest(),
                successor.id,
                at(2)
            ))
            .await
            .expect("the transition ran"),
        RefreshTransition::Advanced,
        "a token left unconsumed by a rolled-back transition cannot be spent"
    );
}

/// **12.** A rotation and a deliberate revocation of the same family do not deadlock.
///
/// # This is the assertion that makes the lock ORDER load-bearing
///
/// Every other assertion passes just as well if both operations take the family lock and the token
/// lock in the *opposite* sequence, because a consistent order is a consistent order. What breaks
/// is the pair: `revoke_family` writes the family row and then sweeps the tokens — family, then
/// token — so an `advance` that locked the token first could hold T while waiting for F exactly as
/// the revocation holds F while waiting for T.
///
/// Both engines detect that and abort one side, so the mutation shows up as an **error** rather
/// than as a hang. Both calls returning `Ok` is therefore the whole assertion; the end state is
/// checked too, because either winner must leave the same one.
pub async fn a_rotation_and_a_revocation_do_not_deadlock<F: RefreshFixture>(fixture: &F) {
    fixture.reset().await;
    let entropy = OsEntropy;
    let (family, first) = live_family(fixture, &entropy).await;
    let successor = prepare(&entropy);
    let presented = first.token.digest();
    let gate = tokio::sync::Barrier::new(2);

    let rotating = async {
        gate.wait().await;
        fixture
            .repository()
            .advance(request(
                &presented,
                &successor.token.digest(),
                successor.id,
                at(1),
            ))
            .await
    };
    let revoking = async {
        gate.wait().await;
        fixture.repository().revoke_family(family, at(1)).await
    };
    let (rotated, revoked) = tokio::join!(rotating, revoking);

    let rotated = rotated.expect("the rotation deadlocked or was aborted");
    revoked.expect("the revocation deadlocked or was aborted");
    assert!(
        matches!(
            rotated,
            RefreshTransition::Advanced | RefreshTransition::FamilyRevoked
        ),
        "a rotation racing a revocation reported something neither side can produce"
    );

    // Whichever won, the end state is the same one.
    assert!(
        fixture.family_revoked_at(family).await.is_some(),
        "the revocation left no tombstone"
    );
    let usable = fixture
        .tokens_in(family)
        .await
        .into_iter()
        .filter(|row| row.consumed_at.is_none() && row.revoked_at.is_none())
        .count();
    assert_eq!(
        usable, 0,
        "a token survived in a family that was revoked in the same instant"
    );
}

/// Runs every assertion above, in order, and proves it ran all of them.
///
/// Calling this from all four rows is what makes *"the behaviour is identical across both
/// engines and both adapters"* a property of the build rather than of two files that agree by
/// inspection. There is no fifth thing to assert for it: the sameness *is* this function being the
/// only copy.
pub async fn run_every_refresh_assertion<F: RefreshFixture>(fixture: &F) {
    let mut ran = 0_usize;

    a_family_begins_with_one_usable_token(fixture).await;
    ran += 1;
    a_rotation_consumes_the_old_and_creates_one_successor(fixture).await;
    ran += 1;
    two_concurrent_presentations_produce_one_rotation(fixture).await;
    ran += 1;
    a_replay_revokes_the_successor_and_the_family(fixture).await;
    ran += 1;
    a_successor_prepared_before_a_revocation_cannot_land_after_it(fixture).await;
    ran += 1;
    a_late_insert_into_a_revoked_family_is_refused(fixture).await;
    ran += 1;
    family_revocation_is_monotonic(fixture).await;
    ran += 1;
    the_subject_and_scopes_survive_rotation_exactly(fixture).await;
    ran += 1;
    an_unknown_or_expired_token_disturbs_no_family(fixture).await;
    ran += 1;
    no_raw_refresh_token_reaches_storage(fixture).await;
    ran += 1;
    a_failed_transition_rolls_back_both_halves(fixture).await;
    ran += 1;
    a_rotation_and_a_revocation_do_not_deadlock(fixture).await;
    ran += 1;

    // THE POSITIVE CONTROL. The census entry for this suite is one line per row and cannot see
    // inside this function, so a deleted call would reduce coverage with every gate still green.
    assert_eq!(
        ran, REFRESH_ASSERTIONS,
        "the refresh contract declares {REFRESH_ASSERTIONS} assertions and ran {ran}"
    );
}
