//! T055 — cancellation during Boot (FR-024, C-L8) and duplicate state registration (FR-011).
//!
//! # What "no provider half-initialised" actually has to mean
//!
//! FR-024 is easy to satisfy vacuously: if nothing was running, nothing was left half-running. So
//! these tests cancel **after** at least one provider has initialised, and assert that the
//! providers that got up are taken back down — reverse actual initialisation order, same as any
//! other Boot failure. Cancellation is not a special path with weaker guarantees; that is the
//! whole claim.

mod support;

use renvor_core::{ErrorCategory, LifecyclePhase};
use support::{Behaviour, Journal, Marker, Scripted, builder};

#[tokio::test]
async fn cancellation_before_boot_starts_no_provider_at_all() {
    let journal = Journal::new();
    let builder = builder()
        .with_provider(
            Scripted::new(&journal, "first")
                .provides(&["first"])
                .boxed(),
        )
        .with_provider(Scripted::new(&journal, "second").needs(&["first"]).boxed());
    let phases = builder.phase_log();
    let application = builder.build().expect("assembles");

    application.cancel().cancel();
    let failure = application
        .boot()
        .await
        .expect_err("a cancelled application must not boot");

    assert_eq!(failure.origin().category(), ErrorCategory::Cancelled);
    assert!(journal.inits().is_empty(), "0 providers initialised");
    assert!(journal.stops().is_empty(), "and so 0 needed stopping");
    assert!(!phases.entries().contains(&LifecyclePhase::Ready));
}

#[tokio::test]
async fn cancellation_partway_through_boot_rolls_back_what_started() {
    // FR-024's real case. `first` initialises; `waiter` blocks on its own scope; the cancellation
    // arrives while it is blocked. `first` must be stopped rather than left running against an
    // application that never reached Ready.
    let journal = Journal::new();
    let application = builder()
        .with_provider(
            Scripted::new(&journal, "first")
                .provides(&["first"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "waiter")
                .needs(&["first"])
                .behaving(Behaviour::AwaitCancellation)
                .boxed(),
        )
        .build()
        .expect("assembles");

    // Cancelling from another task, so the signal genuinely arrives mid-`initialise` rather than
    // being observed by the pre-flight check.
    let scope = application.cancel().clone();
    tokio::spawn(async move { scope.cancel() });

    let failure = application
        .boot()
        .await
        .expect_err("cancellation must end the boot");

    assert_eq!(journal.inits(), vec!["first"]);
    assert_eq!(
        journal.stops(),
        vec!["first"],
        "the provider that got up must be taken back down"
    );
    assert!(failure.rollback().is_clean());
}

#[tokio::test]
async fn an_abandoned_provider_scope_is_cancelled_rather_than_left_live() {
    // The drop-guard half of FR-024, observed from outside: after a failed boot, the scope the
    // failing provider's work would have run under is cancelled. Nothing it started can still
    // believe it has a live provider behind it.
    let journal = Journal::new();
    let application = builder()
        .with_provider(
            Scripted::new(&journal, "first")
                .provides(&["first"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "broken")
                .needs(&["first"])
                .behaving(Behaviour::FailInit)
                .boxed(),
        )
        .build()
        .expect("assembles");

    let root = application.cancel().clone();
    assert!(!root.is_cancelled(), "live before the boot");

    let failure = application.boot().await.expect_err("boot fails");
    assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);

    // The root itself is untouched — a failed boot is not a process-wide cancellation — while the
    // per-provider scopes are gone with the providers they belonged to.
    assert!(
        !root.is_cancelled(),
        "rolling back one application must not cancel the caller's root scope"
    );
    assert_eq!(journal.stops(), vec!["first"]);
}

#[tokio::test]
async fn a_second_registration_of_the_same_state_type_fails_the_boot() {
    // FR-011 / SC-004: a distinct, named error — not a panic, and not a silent overwrite that
    // would make the winner depend on an initialisation order nobody wrote down.
    let journal = Journal::new();
    let failure = builder()
        .with_provider(
            Scripted::new(&journal, "first")
                .provides(&["first"])
                .behaving(Behaviour::RegisterMarker)
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "second")
                .needs(&["first"])
                .behaving(Behaviour::RegisterMarker)
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("the second registration of Marker must fail");

    assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);
    assert!(failure.origin().to_string().contains("second"));

    // The cause survives as a cause rather than being flattened into the message (C-E2), and it
    // is specifically a duplicate-state error.
    let cause = std::error::Error::source(failure.origin()).expect("the cause is preserved");
    let kernel = cause
        .downcast_ref::<renvor_core::KernelError>()
        .expect("the cause is a KernelError");
    assert_eq!(kernel.category(), ErrorCategory::StateDuplicate);
    assert!(kernel.to_string().contains("Marker"), "{kernel}");

    assert_eq!(journal.stops(), vec!["first"], "the first is rolled back");
}

#[tokio::test]
async fn state_registered_by_a_dependency_is_visible_to_its_dependent() {
    // POSITIVE CONTROL for the test above, and the point of resolving the order at all: the
    // duplicate failed *because* the second provider could see the first's registration. If
    // dependents could not see their dependencies' state, that test would pass for the wrong
    // reason and the ordering guarantee would be unspent.
    let journal = Journal::new();
    let application = builder()
        .with_provider(
            Scripted::new(&journal, "producer")
                .provides(&["marker"])
                .behaving(Behaviour::RegisterMarker)
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "consumer")
                .needs(&["marker"])
                .behaving(Behaviour::DegradeWithoutMarker)
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("both providers succeed");

    assert_eq!(journal.inits(), vec!["producer", "consumer"]);
    assert!(
        journal.degraded().is_empty(),
        "the consumer found the marker, so it had no reason to degrade"
    );
    assert_eq!(
        application.state().get::<Marker>().expect("registered"),
        &Marker("registered")
    );
}

#[tokio::test]
async fn retrieving_unregistered_state_is_an_error_not_a_panic() {
    // SC-004 from the read side. Nothing registered a Marker, and asking for one produces a named
    // error rather than ending the process.
    let application = builder().build().expect("assembles");
    let error = application
        .state()
        .get::<Marker>()
        .expect_err("nothing registered a Marker");
    assert_eq!(error.category(), ErrorCategory::StateMissing);
    assert!(error.to_string().contains("Marker"), "{error}");
}
