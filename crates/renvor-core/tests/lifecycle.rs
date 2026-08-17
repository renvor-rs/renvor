//! T042 and T043 — the phase sequence (SC-001) and reverse **actual initialisation** order
//! (SC-002).
//!
//! # The negative control is the point of this file
//!
//! C-L3 says, in as many words, that a test asserting against declaration order *can pass while
//! the implementation is wrong*. So [`rollback_replays_actual_initialisation_order`] does not
//! merely assert the right sequence — it also asserts that the **registration** sequence is a
//! different answer on the same graph. Without that, a rollback that happened to replay
//! registration order would satisfy the first assertion and the test would be decorative.

mod support;

use renvor_core::{ErrorCategory, LifecyclePhase};
use support::{Behaviour, Journal, Scripted, builder};

/// Registration order `[http, cache, db]` where `http` depends on `db`.
///
/// Resolution must move `db` in front of `http`, so registration order and initialisation order
/// are **different sequences** — which is what makes the negative control meaningful.
fn reordering_application(journal: &Journal, last: Behaviour) -> renvor_core::ApplicationBuilder {
    builder()
        .with_provider(
            Scripted::new(journal, "http")
                .provides(&["http"])
                .needs(&["database"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(journal, "cache")
                .provides(&["cache"])
                .behaving(last)
                .boxed(),
        )
        .with_provider(Scripted::new(journal, "db").provides(&["database"]).boxed())
}

#[tokio::test]
async fn a_successful_run_observes_exactly_the_declared_phase_sequence() {
    // SC-001: 0 runs observe a different order. FR-002: observable without instrumenting
    // internals — the log handle below is public API, taken before the run starts.
    let journal = Journal::new();
    let builder = reordering_application(&journal, Behaviour::Succeed);
    let phases = builder.phase_log();

    let application = builder
        .build()
        .expect("a well-formed application assembles")
        .boot()
        .await
        .expect("every provider succeeds");

    assert_eq!(
        phases.entries(),
        vec![
            LifecyclePhase::Load,
            LifecyclePhase::Validate,
            LifecyclePhase::Register,
            LifecyclePhase::Boot,
            LifecyclePhase::Ready,
        ]
    );
    assert_eq!(application.phase(), LifecyclePhase::Ready);
}

#[tokio::test]
async fn the_phase_sequence_is_strictly_increasing() {
    // FR-001 from the other side: not just "these five phases", but that no observed entry is
    // earlier than the one before it. A log that happened to contain the right *set* out of order
    // would pass the test above and fail this one.
    let journal = Journal::new();
    let builder = reordering_application(&journal, Behaviour::Succeed);
    let phases = builder.phase_log();
    builder
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let observed = phases.entries();
    for pair in observed.windows(2) {
        assert!(
            pair[0].position() < pair[1].position(),
            "phase {} followed {} — a later phase ran before an earlier one",
            pair[1],
            pair[0]
        );
    }
}

#[tokio::test]
async fn resolution_reorders_providers_relative_to_registration() {
    // The premise of the negative control below. If this ever stopped holding, the SC-002 test
    // would still pass while proving nothing, so it is asserted rather than assumed.
    let journal = Journal::new();
    let application = reordering_application(&journal, Behaviour::Succeed)
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let registration: Vec<&str> = application
        .registry()
        .providers()
        .iter()
        .map(|provider| provider.id().as_str())
        .collect();
    assert_eq!(registration, vec!["http", "cache", "db"]);

    assert_eq!(journal.inits(), vec!["db", "http", "cache"]);
    assert_ne!(
        journal.inits(),
        registration,
        "the fixture must reorder, or the negative control proves nothing"
    );
}

#[tokio::test]
async fn rollback_replays_actual_initialisation_order() {
    // SC-002 / C-L3 / FR-004. `cache` initialises last and fails, so `db` and `http` must be
    // stopped — in the reverse of the order they actually initialised.
    let journal = Journal::new();
    let failure = reordering_application(&journal, Behaviour::FailInit)
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("the last provider fails");

    assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);
    assert!(
        failure.origin().to_string().contains("cache"),
        "the originating failure names the provider: {}",
        failure.origin()
    );

    let initialised = journal.inits();
    assert_eq!(initialised, vec!["db", "http"], "cache never initialised");

    let stopped = journal.stops();
    let mut reverse_initialisation = initialised.clone();
    reverse_initialisation.reverse();
    assert_eq!(
        stopped, reverse_initialisation,
        "rollback must replay the realised order backwards"
    );

    // NEGATIVE CONTROL — the assertion C-L3 warns about. Reverse *registration* order over the
    // same two providers is `[db, http]`; reverse *initialisation* order is `[http, db]`. A test
    // that asserted the former would pass on an implementation that replayed registration order,
    // which is exactly the wrong implementation.
    let reverse_registration = vec!["db".to_owned(), "http".to_owned()];
    assert_ne!(
        reverse_initialisation, reverse_registration,
        "the fixture must distinguish the two orders"
    );
    assert_ne!(
        stopped, reverse_registration,
        "rollback used registration order, which C-L3 forbids"
    );
}

#[tokio::test]
async fn a_failed_boot_never_reaches_ready() {
    // C-L2: `Ready` is not reached. Asserted on the phase log, which survives the failure.
    let journal = Journal::new();
    let builder = reordering_application(&journal, Behaviour::FailInit);
    let phases = builder.phase_log();

    let failure = builder
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("boot fails");

    assert_eq!(
        phases.entries(),
        vec![
            LifecyclePhase::Load,
            LifecyclePhase::Validate,
            LifecyclePhase::Register,
            LifecyclePhase::Boot,
        ]
    );
    assert!(!phases.entries().contains(&LifecyclePhase::Ready));
    assert_eq!(failure.phases(), phases.entries());
}

#[tokio::test]
async fn a_failure_during_rollback_does_not_abort_the_rest_of_it() {
    // C-L4 / FR-005. `first` fails while stopping; `second` must still be stopped, and both the
    // rollback failure and the original failure must be reported.
    let journal = Journal::new();
    let failure = builder()
        .with_provider(
            Scripted::new(&journal, "first")
                .provides(&["first"])
                .behaving(Behaviour::FailStop)
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "second")
                .provides(&["second"])
                .needs(&["first"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "third")
                .needs(&["second"])
                .behaving(Behaviour::FailInit)
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("the third provider fails");

    assert_eq!(journal.inits(), vec!["first", "second"]);
    assert_eq!(
        journal.stops(),
        vec!["second", "first"],
        "the failing stop must not strand the providers behind it"
    );

    // The original failure is still the original failure.
    assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);
    assert!(failure.origin().to_string().contains("third"));

    // And the rollback failure is reported alongside it, not instead of it.
    let rollback = failure.rollback();
    assert!(!rollback.is_clean());
    assert_eq!(rollback.failures().len(), 1);
    assert_eq!(
        rollback.failures()[0].category(),
        ErrorCategory::ProviderStop
    );
    assert!(rollback.failures()[0].to_string().contains("first"));
    assert_eq!(
        rollback.stopped().len(),
        2,
        "a provider whose stop failed was still stopped, and says so"
    );
}

#[tokio::test]
async fn a_clean_rollback_reports_no_failures() {
    // POSITIVE CONTROL for the test above: `is_clean` discriminates rather than always being
    // false whenever a rollback happened at all.
    let journal = Journal::new();
    let failure = reordering_application(&journal, Behaviour::FailInit)
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("boot fails");

    assert!(failure.rollback().is_clean());
    assert!(failure.rollback().failures().is_empty());
    assert_eq!(failure.rollback().stopped().len(), 2);
}
