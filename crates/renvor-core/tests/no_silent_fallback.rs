//! T045 — FR-022 and C-E4: a required capability that is unavailable **fails the operation**.
//!
//! # Why this file needs a positive control more than most
//!
//! Every assertion here is a zero: 0 runs boot a degraded application, 0 substitute a default for
//! a missing provider, 0 downgrade a hard failure to a warning. A test suite full of zeros passes
//! trivially if the thing counting them cannot count — a journal that never records degradation
//! reports zero degradation on a kernel that degrades constantly.
//!
//! So [`the_degradation_detector_fires_when_a_provider_actually_degrades`] registers a provider
//! that **deliberately carries on without the capability it needs** and asserts the detector
//! catches it. Only then do the zeros elsewhere mean anything.
//!
//! The configuration-layer instance of FR-022 is proven separately, at the eight-obligation gate's
//! obligation 6 and at T069.

mod support;

use renvor_core::{ErrorCategory, LifecyclePhase};
use support::{Behaviour, Journal, Scripted, builder};

#[tokio::test]
async fn the_degradation_detector_fires_when_a_provider_actually_degrades() {
    // POSITIVE CONTROL, and the load-bearing test in this file. `DegradeWithoutMarker` is the
    // FR-022 anti-pattern written on purpose: it needs a Marker, does not find one, records that
    // it degraded, and reports success anyway.
    let journal = Journal::new();
    let application = builder()
        .with_provider(
            Scripted::new(&journal, "degrader")
                .behaving(Behaviour::DegradeWithoutMarker)
                .boxed(),
        )
        .build()
        .expect("assembles");

    let application = application.boot().await;
    assert!(
        application.is_ok(),
        "the degrading provider reports success"
    );

    assert_eq!(
        journal.degraded(),
        vec!["degrader"],
        "the detector must be able to catch a degrading provider"
    );
}

#[tokio::test]
async fn a_missing_required_capability_fails_the_operation_rather_than_degrading_it() {
    // FR-022 / C-E4. The kernel's own answer to an unavailable required capability: refuse.
    let journal = Journal::new();
    let application = builder().with_provider(
        Scripted::new(&journal, "api")
            .needs(&["database"])
            .behaving(Behaviour::DegradeWithoutMarker)
            .boxed(),
    );
    let phases = application.phase_log();

    let error = application
        .build()
        .expect_err("a required capability nobody provides must fail the build");

    assert_eq!(error.category(), Some(ErrorCategory::DependencyMissing));

    // 0 runs boot a degraded application: Boot is never entered, so the provider that *would*
    // have degraded never ran. The detector proven live above stays silent here.
    assert!(!phases.entries().contains(&LifecyclePhase::Boot));
    assert!(journal.degraded().is_empty(), "0 degraded runs");
    assert!(journal.inits().is_empty(), "0 providers initialised");
}

#[tokio::test]
async fn no_default_is_substituted_for_a_missing_required_provider() {
    // 0 substitutions. If the kernel quietly supplied a stand-in for `database`, the build would
    // succeed and the dependent would initialise — so the absence of an init is the assertion.
    let journal = Journal::new();
    let error = builder()
        .with_provider(
            Scripted::new(&journal, "cache")
                .provides(&["cache"])
                .boxed(),
        )
        .with_provider(Scripted::new(&journal, "api").needs(&["database"]).boxed())
        .build()
        .expect_err("no stand-in may be invented for `database`");

    assert_eq!(error.category(), Some(ErrorCategory::DependencyMissing));
    assert!(
        journal.inits().is_empty(),
        "not even the provider whose own dependencies were satisfiable may start"
    );

    // POSITIVE CONTROL: supplying the capability makes the very same graph boot, so the refusal
    // above is about the missing capability and not about the shape of the graph.
    let journal = Journal::new();
    let booted = builder()
        .with_provider(
            Scripted::new(&journal, "cache")
                .provides(&["cache"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "db")
                .provides(&["database"])
                .boxed(),
        )
        .with_provider(Scripted::new(&journal, "api").needs(&["database"]).boxed())
        .build()
        .expect("assembles")
        .boot()
        .await;
    assert!(booted.is_ok());
    assert_eq!(journal.inits().len(), 3);
}

#[tokio::test]
async fn a_hard_failure_is_not_downgraded_to_a_warning() {
    // 0 downgrades. A provider that fails to initialise ends the start; it does not produce a
    // running application carrying a note about it.
    let journal = Journal::new();
    let builder = builder()
        .with_provider(Scripted::new(&journal, "ok").provides(&["ok"]).boxed())
        .with_provider(
            Scripted::new(&journal, "broken")
                .needs(&["ok"])
                .behaving(Behaviour::FailInit)
                .boxed(),
        );
    let phases = builder.phase_log();

    let failure = builder
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("a failing provider must fail the boot");

    assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);
    assert!(
        !phases.entries().contains(&LifecyclePhase::Ready),
        "Ready must not be reached with a failed provider"
    );
    assert_eq!(
        journal.stops(),
        vec!["ok"],
        "the provider that did start is stopped, not left running"
    );
}

#[tokio::test]
async fn a_rollback_failure_is_not_swallowed_by_the_original_failure() {
    // The subtlest downgrade: reporting only the first problem. C-L4 requires both to survive.
    let journal = Journal::new();
    let failure = builder()
        .with_provider(
            Scripted::new(&journal, "leaky")
                .provides(&["leaky"])
                .behaving(Behaviour::FailStop)
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "broken")
                .needs(&["leaky"])
                .behaving(Behaviour::FailInit)
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("boot fails");

    assert_eq!(failure.origin().category(), ErrorCategory::ProviderInit);
    assert_eq!(failure.rollback().failures().len(), 1);
    assert!(
        failure.to_string().contains("failed to stop")
            || failure.to_string().contains("also failed to stop"),
        "the summary must mention that rollback was not clean: {failure}"
    );
}
