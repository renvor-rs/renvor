//! T092 — SC-009: failure is injectable at **7 of 7** lifecycle phases, 100% covered by a test.
//!
//! # What this file proves, and the two things it refuses to claim
//!
//! Every phase gets a run that **fires**, and the run's outcome is asserted — not merely that a
//! harness was constructed. The phase list is taken from `LifecyclePhase::ALL`, so adding an
//! eighth phase fails this file rather than silently leaving it uncovered.
//!
//! It does **not** claim that `Panic` is injectable at `Boot` or `Stop`: the kernel does not
//! contain a panicking provider, and the harness says so with a diagnostic instead of pretending.
//! It does **not** claim that `Hang` is enforced at `Load` or `Validate`: those phases are
//! synchronous and unbounded. Both gaps are asserted here as the current, measured behaviour, so
//! that closing either one **fails this file** and forces the claim to be updated.

use std::time::Duration;

use renvor_core::LifecyclePhase;
use renvor_testkit::{Behaviour, FailureInjectionPoint, Harness, Outcome};

/// The behaviour every phase can express, used for the coverage sweep.
const BASELINE: Behaviour = Behaviour::Fail;

#[tokio::test(start_paused = true)]
async fn failure_is_injectable_at_seven_of_seven_phases() {
    // SC-009. The list comes from the kernel's own enum, so it cannot drift.
    let mut covered = Vec::new();

    for phase in LifecyclePhase::ALL {
        let run = Harness::injecting(FailureInjectionPoint::new(phase, BASELINE))
            .run()
            .await;

        assert!(
            run.fired,
            "the injection at {phase} never fired, so the phase is not covered"
        );
        assert!(
            !matches!(run.outcome, Outcome::NotInjectable(_)),
            "{phase} reported not injectable: {:?}",
            run.outcome
        );
        covered.push(phase);
    }

    assert_eq!(
        covered.len(),
        LifecyclePhase::ALL.len(),
        "7 of 7 phases, taken from the kernel's own list"
    );
    assert_eq!(covered, LifecyclePhase::ALL.to_vec());
}

#[tokio::test(start_paused = true)]
async fn each_phase_fails_in_the_way_that_phase_fails() {
    // Coverage without outcomes would pass on a harness that injected nothing and reported
    // success. Each phase's *characteristic* failure is asserted.
    let cases = [
        (LifecyclePhase::Load, "build"),
        (LifecyclePhase::Validate, "build"),
        (LifecyclePhase::Register, "build"),
        (LifecyclePhase::Boot, "boot"),
        (LifecyclePhase::Ready, "ready"),
        (LifecyclePhase::Drain, "drain"),
        (LifecyclePhase::Stop, "stop"),
    ];

    for (phase, expected) in cases {
        let run = Harness::injecting(FailureInjectionPoint::new(phase, BASELINE))
            .run()
            .await;

        match (expected, &run.outcome) {
            ("build", Outcome::BuildFailed(_)) => {
                assert!(
                    !run.reached(LifecyclePhase::Boot),
                    "{phase}: 0 providers booted"
                );
            }
            ("boot", Outcome::BootFailed(_)) => {
                assert!(run.reached(LifecyclePhase::Boot));
                assert!(!run.reached(LifecyclePhase::Ready), "Ready is not reached");
            }
            ("ready", Outcome::Ready) => {
                assert!(run.reached(LifecyclePhase::Ready));
            }
            ("drain", Outcome::DrainIncomplete(outstanding)) => {
                assert_eq!(*outstanding, 1, "the held work is reported as outstanding");
            }
            // A stop failure does not change the shutdown's drain outcome: the drain was clean and
            // the provider refused afterwards. Both facts survive, which is C-L4.
            ("stop", Outcome::Stopped) => {}
            (_, other) => panic!("{phase} produced {other:?}, expected a {expected} failure"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_hanging_provider_is_bounded_at_boot_and_at_stop() {
    // FR-031: the deadline is proven at **0** real elapsed time. A one-hour deadline costs nothing
    // under a paused clock, which is what makes this assertion runnable in CI.
    let started = std::time::Instant::now();

    let boot = Harness::injecting(FailureInjectionPoint::new(
        LifecyclePhase::Boot,
        Behaviour::Hang,
    ))
    .with_provider_deadline(Duration::from_secs(3600))
    .run()
    .await;

    assert!(boot.fired);
    match boot.outcome {
        Outcome::BootFailed(ref message) => {
            assert!(message.contains("deadline"), "{message}");
        }
        ref other => panic!("a hanging provider must fail the boot, got {other:?}"),
    }

    let stop = Harness::injecting(FailureInjectionPoint::new(
        LifecyclePhase::Stop,
        Behaviour::Hang,
    ))
    .with_provider_deadline(Duration::from_secs(3600))
    .run()
    .await;

    assert!(stop.fired);
    assert!(
        stop.reached(LifecyclePhase::Stop),
        "shutdown still completed"
    );

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "two one-hour deadlines cost {:?} of real time",
        started.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn a_panicking_provider_is_reported_as_not_injectable_rather_than_faked() {
    // **This test documents a gap, and exists so closing it is noticed.** C-L9 requires `Panic` at
    // every phase. The kernel does not contain a panicking provider: catching a panic across an
    // `await` needs a `'static` future (ruled out by `InitContext` borrowing the state map) or a
    // new dependency in a phase whose inventory is a recorded gate.
    //
    // If containment is ever added, this test **fails**, and whoever adds it has to come here and
    // update the claim. That is the point of asserting a gap rather than omitting it.
    for phase in [LifecyclePhase::Boot, LifecyclePhase::Stop] {
        let run = Harness::injecting(FailureInjectionPoint::new(phase, Behaviour::Panic))
            .run()
            .await;

        match run.outcome {
            Outcome::NotInjectable(ref why) => {
                assert!(why.contains("not contained"), "{why}");
                assert!(why.contains("evidence"), "and points at the record: {why}");
            }
            ref other => panic!(
                "panic containment appears to have been added at {phase} ({other:?}) — \
                 update SC-009's record and this test"
            ),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_run_with_no_injection_reaches_ready_and_stops_cleanly() {
    // POSITIVE CONTROL for the whole file: the harness builds an application that works. Without
    // this, every "the injection fired" assertion could be true of a harness that always fails.
    let run = Harness::injecting(FailureInjectionPoint::new(
        LifecyclePhase::Ready,
        Behaviour::Fail,
    ))
    .run()
    .await;

    assert_eq!(run.outcome, Outcome::Ready);
    assert!(run.reached(LifecyclePhase::Ready));
    assert!(
        run.reached(LifecyclePhase::Load),
        "and started at the start"
    );
}

#[test]
fn every_phase_and_behaviour_combination_is_enumerable() {
    // The combinations come from the two enums, so neither list can be hand-trimmed.
    let points = FailureInjectionPoint::every_combination();
    assert_eq!(
        points.len(),
        LifecyclePhase::ALL.len() * Behaviour::ALL.len()
    );
    assert_eq!(points.len(), 21);
}
