//! T057 and T058 — the drain budget (SC-006, FR-007, FR-042) and shutdown semantics (FR-006,
//! FR-008, FR-009).
//!
//! Every test that involves elapsed time runs under `start_paused = true`, so the runtime's clock
//! advances only when nothing is runnable. A "thirty second" budget therefore costs **0** real
//! seconds, which is FR-031's requirement and also the difference between a suite that is run and
//! one that is skipped.

mod support;

use std::time::Duration;

use renvor_core::lifecycle::DEFAULT_DRAIN_BUDGET;
use renvor_core::{DrainOutcome, ErrorCategory, LifecyclePhase};
use support::{Journal, Scripted, builder};

#[tokio::test(start_paused = true)]
async fn a_drain_with_no_work_in_flight_is_clean() {
    let journal = Journal::new();
    let mut application = builder()
        .with_provider(
            Scripted::new(&journal, "db")
                .provides(&["database"])
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let report = application.shutdown().await;
    assert_eq!(report.drain(), DrainOutcome::Clean);
    assert!(report.is_clean());
    assert_eq!(journal.stops(), vec!["db"]);
}

#[tokio::test(start_paused = true)]
async fn an_over_budget_drain_is_reported_as_incomplete_and_never_as_clean() {
    // SC-006: 100% of over-budget drains report incomplete; **0** report clean.
    let journal = Journal::new();
    let mut application = builder()
        .with_drain_budget(Duration::from_secs(5))
        .with_provider(
            Scripted::new(&journal, "db")
                .provides(&["database"])
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    // Work that outlives the budget: the permit is held for the whole shutdown.
    let _permit = application
        .work()
        .begin("long request")
        .expect("the gate is open before shutdown");

    let report = application.shutdown().await;
    assert_eq!(report.drain(), DrainOutcome::Incomplete { outstanding: 1 });
    assert!(!report.drain().is_clean(), "0 clean reports");
    assert!(!report.is_clean());

    // Providers still stop. An incomplete drain is not an excuse to skip shutdown.
    assert_eq!(journal.stops(), vec!["db"]);
}

#[tokio::test(start_paused = true)]
async fn a_zero_budget_with_work_in_flight_reports_it_as_outstanding() {
    // FR-042. Choosing an immediate stop must never silently read as a clean one.
    let journal = Journal::new();
    let mut application = builder()
        .with_drain_budget(Duration::ZERO)
        .with_provider(
            Scripted::new(&journal, "db")
                .provides(&["database"])
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    assert_eq!(application.drain_budget(), Duration::ZERO, "zero is valid");

    let _first = application.work().begin("a").expect("open");
    let _second = application.work().begin("b").expect("open");

    let report = application.shutdown().await;
    assert_eq!(report.drain(), DrainOutcome::Incomplete { outstanding: 2 });
    assert_eq!(
        journal.stops(),
        vec!["db"],
        "and it still stops immediately"
    );
}

#[tokio::test(start_paused = true)]
async fn a_zero_budget_with_nothing_in_flight_is_clean() {
    // POSITIVE CONTROL for the test above: zero is a budget, not a verdict. Without this, an
    // implementation that reported `Incomplete` for every zero budget would pass.
    let mut application = builder()
        .with_drain_budget(Duration::ZERO)
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    assert_eq!(application.shutdown().await.drain(), DrainOutcome::Clean);
}

#[tokio::test(start_paused = true)]
async fn work_that_finishes_inside_the_budget_drains_cleanly() {
    // POSITIVE CONTROL for the over-budget test: the drain genuinely waits and genuinely notices
    // completion, rather than reporting the count it saw when it started.
    let mut application = builder()
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let permit = application.work().begin("short request").expect("open");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        drop(permit);
    });

    assert_eq!(application.drain_budget(), DEFAULT_DRAIN_BUDGET);
    assert_eq!(application.shutdown().await.drain(), DrainOutcome::Clean);
}

#[tokio::test(start_paused = true)]
async fn new_work_after_shutdown_begins_is_refused_with_an_error() {
    // FR-006: rejected **with an error stating the application is shutting down**. Neither
    // silently dropped nor silently accepted.
    let mut application = builder()
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    assert!(
        application.work().begin("before").is_ok(),
        "work is admitted while running"
    );

    application.shutdown().await;

    let error = application
        .work()
        .begin("after")
        .expect_err("work submitted after shutdown must be refused");
    assert_eq!(error.category(), ErrorCategory::ShuttingDown);
    assert!(error.to_string().contains("after"), "{error}");
}

#[tokio::test(start_paused = true)]
async fn a_second_shutdown_is_safe_and_stops_no_provider_twice() {
    // FR-008 / C-L6. `Stop` runs at most once per provider, and the second request observes the
    // first's outcome rather than producing a new one.
    let journal = Journal::new();
    let mut application = builder()
        .with_provider(
            Scripted::new(&journal, "db")
                .provides(&["database"])
                .boxed(),
        )
        .with_provider(Scripted::new(&journal, "http").needs(&["database"]).boxed())
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let first: Vec<String> = application
        .shutdown()
        .await
        .stop()
        .stopped()
        .iter()
        .map(|id| id.as_str().to_owned())
        .collect();
    assert_eq!(first, vec!["http", "db"], "reverse initialisation order");

    let second: Vec<String> = application
        .shutdown()
        .await
        .stop()
        .stopped()
        .iter()
        .map(|id| id.as_str().to_owned())
        .collect();

    assert_eq!(
        second, first,
        "the second request observes the first outcome"
    );
    assert_eq!(
        journal.stops(),
        vec!["http", "db"],
        "each provider was stopped exactly once across both requests"
    );
}

#[tokio::test(start_paused = true)]
async fn concurrent_shutdown_requests_are_safe() {
    // C-L6's "concurrent double shutdown". `shutdown` takes `&mut self`, so the compiler will not
    // let two run at once — the guarantee is structural. This exercises back-to-back requests from
    // separate awaits, which is as close as the borrow checker permits.
    let journal = Journal::new();
    let mut application = builder()
        .with_provider(
            Scripted::new(&journal, "db")
                .provides(&["database"])
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    for _ in 0..3 {
        let report = application.shutdown().await;
        assert!(report.is_clean());
    }
    assert_eq!(journal.stops(), vec!["db"], "one stop, three requests");
}

#[tokio::test(start_paused = true)]
async fn shutdown_before_ready_records_only_the_phases_that_ran() {
    // FR-009 / C-L6. An application that never booted still shuts down, and its phase record does
    // **not** claim it passed through Boot and Ready on the way.
    //
    // The other half of FR-009 — "rolls back whatever *was* initialised" — cannot arise for an
    // application value at all: `boot` consumes `self`, so a partially-initialised application is
    // never handed back to anyone. The interrupted-boot case is covered in `lifecycle_edges.rs`,
    // where cancellation mid-Boot rolls back in reverse order.
    let journal = Journal::new();
    let builder = builder().with_provider(
        Scripted::new(&journal, "db")
            .provides(&["database"])
            .boxed(),
    );
    let phases = builder.phase_log();
    let mut application = builder.build().expect("assembles");

    assert_eq!(application.phase(), LifecyclePhase::Register);
    let report = application.shutdown().await;
    assert!(report.is_clean());

    assert_eq!(
        phases.entries(),
        vec![
            LifecyclePhase::Load,
            LifecyclePhase::Validate,
            LifecyclePhase::Register,
            LifecyclePhase::Drain,
            LifecyclePhase::Stop,
        ],
        "Boot and Ready never ran, so they must not appear"
    );
    assert!(journal.inits().is_empty(), "0 providers ever initialised");
    assert!(journal.stops().is_empty(), "so 0 needed stopping");
}

#[tokio::test(start_paused = true)]
async fn a_full_run_records_all_seven_phases() {
    // POSITIVE CONTROL for the test above: the phases skipped there are genuinely recordable, so
    // their absence means "did not run" rather than "cannot be recorded".
    let mut application = builder()
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");
    let phases = application.phases();
    application.shutdown().await;

    assert_eq!(phases.len(), 5);
    assert_eq!(application.phases(), LifecyclePhase::ALL.to_vec());
    assert_eq!(application.phase(), LifecyclePhase::Stop);
}

#[tokio::test(start_paused = true)]
async fn every_stop_failure_is_reported_not_just_the_first() {
    // T063 / FR-005 / C-L4. Two providers fail to stop; both failures survive. A `?` on the first
    // would report one and strand the other, and the stranded one is the earlier-initialised
    // provider with the most to release.
    let journal = Journal::new();
    let mut application = builder()
        .with_provider(
            support::Scripted::new(&journal, "first")
                .provides(&["first"])
                .behaving(support::Behaviour::FailStop)
                .boxed(),
        )
        .with_provider(
            support::Scripted::new(&journal, "second")
                .provides(&["second"])
                .needs(&["first"])
                .behaving(support::Behaviour::FailStop)
                .boxed(),
        )
        .with_provider(
            support::Scripted::new(&journal, "third")
                .needs(&["second"])
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let report = application.shutdown().await;
    let failures = report.stop().failures();

    assert_eq!(failures.len(), 2, "both failures survived");
    for failure in failures {
        assert_eq!(failure.category(), ErrorCategory::ProviderStop);
    }
    assert_eq!(
        journal.stops(),
        vec!["third", "second", "first"],
        "a failing stop must not strand the providers behind it"
    );
    assert_eq!(
        report.stop().stopped().len(),
        3,
        "all three were stopped, including the two that refused"
    );
}
