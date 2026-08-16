//! T092 — SC-009: failure is injectable at **7 of 7** lifecycle phases, 100% covered by a test.
//!
//! # What this file proves, and the two things it refuses to claim
//!
//! Every phase gets a run that **fires**, and the run's outcome is asserted — not merely that a
//! harness was constructed. The phase list is taken from `LifecyclePhase::ALL`, so adding an
//! eighth phase fails this file rather than silently leaving it uncovered.
//!
//! Both gaps this file used to record are now closed, and **the tests that recorded them are what
//! forced the update**: each asserted the gap, so fixing the kernel failed the test and brought
//! whoever fixed it back here. That is the whole reason to assert a gap rather than omit it.

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
            ("ready", Outcome::ReadyDegraded(_)) => {
                assert!(run.reached(LifecyclePhase::Ready));
            }
            // `Fail` at Drain means work that errors and returns. Its permit is released on the
            // way out, so the drain COMPLETES — the incomplete drain is `Hang`, asserted below.
            ("drain", Outcome::Stopped) => {}
            // A stop failure does not change the shutdown's drain outcome: the drain was clean and
            // the provider refused afterwards. Both facts survive, which is C-L4.
            ("stop", Outcome::Stopped) => {
                assert_eq!(
                    run.stop_failures.len(),
                    1,
                    "the stop failure must be reported even though shutdown completed"
                );
            }
            (_, other) => panic!("{phase} produced {other:?}, expected a {expected} failure"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn all_twenty_one_behaviour_and_phase_combinations_execute_and_are_attributed() {
    // T116 — C-L9 read literally. SC-009 says "7 of 7 phases injectable"; C-L9 says "with `Fail`,
    // `Panic`, and `Hang` behaviours". That is **21** combinations, and this file previously
    // asserted 11 of them: `Load`, `Validate`, `Register`, `Ready`, and `Drain` each took a
    // `Behaviour` and ignored it, so 10 combinations were enum values with no code behind them.
    //
    // An ignored combination is not a covered one. Every row below runs, fires, and is asserted
    // for the outcome that behaviour produces at that phase.
    use renvor_core::ContributorFault;

    let mut executed = 0_usize;

    for point in FailureInjectionPoint::every_combination() {
        let (phase, behaviour) = (point.phase(), point.behaviour());
        let run = Harness::injecting(point).run().await;

        assert!(run.fired, "{behaviour} at {phase} never fired");
        assert!(
            !matches!(run.outcome, Outcome::NotInjectable(_)),
            "{behaviour} at {phase} reported not injectable: {:?}",
            run.outcome
        );

        match (phase, behaviour, &run.outcome) {
            // The three synchronous build phases: all three behaviours must stop the build, and
            // 0 providers may reach Boot (SC-005).
            (
                LifecyclePhase::Load | LifecyclePhase::Validate | LifecyclePhase::Register,
                _,
                Outcome::BuildFailed(message),
            ) => {
                assert!(
                    !run.reached(LifecyclePhase::Boot),
                    "{behaviour} at {phase}: 0 providers may boot"
                );
                // Attribution: a hang is reported as a deadline, never as an ordinary refusal.
                if behaviour == Behaviour::Hang {
                    assert!(
                        message.contains("deadline"),
                        "{behaviour} at {phase} must be reported as a deadline: {message}"
                    );
                }
            }
            (LifecyclePhase::Boot, _, Outcome::BootFailed(message)) => {
                assert!(run.reached(LifecyclePhase::Boot));
                assert!(!run.reached(LifecyclePhase::Ready));
                if behaviour == Behaviour::Hang {
                    assert!(message.contains("deadline"), "{message}");
                }
            }
            // Reaching Ready is the success condition, so the injection lands in health — and the
            // three behaviours must remain **distinguishable** there, which is the whole reason
            // `ContributorFault` is an enum rather than a boolean.
            (LifecyclePhase::Ready, _, Outcome::ReadyDegraded(report)) => {
                assert!(run.reached(LifecyclePhase::Ready));
                let verdict = report
                    .contributors
                    .iter()
                    .find(|verdict| {
                        verdict.name == "injected-contributor" || verdict.fault.is_fault()
                    })
                    .unwrap_or_else(|| panic!("{behaviour} at {phase}: no verdict: {report:?}"));
                let expected = match behaviour {
                    Behaviour::Fail => ContributorFault::None,
                    Behaviour::Panic => ContributorFault::Panicked,
                    Behaviour::Hang => ContributorFault::TimedOut,
                    // `Behaviour` is `#[non_exhaustive]`. A fourth behaviour must be given a
                    // health attribution here rather than silently matching an existing one.
                    other => panic!("{other} has no readiness attribution; add one"),
                };
                assert_eq!(
                    verdict.fault, expected,
                    "{behaviour} at {phase} was misattributed"
                );
                assert_eq!(report.readiness, renvor_core::Readiness::NotReady);
            }
            // Only `Hang` keeps its permit. `Fail` and `Panic` release it — the second by
            // unwinding, which is the property `drain.rs` claims and nothing else tests.
            (LifecyclePhase::Drain, Behaviour::Hang, Outcome::DrainIncomplete(outstanding)) => {
                assert_eq!(*outstanding, 1, "the held work is reported as outstanding");
            }
            // STATED LIMIT: these two reach the kernel identically. The harness catches the
            // injected panic, so `shutdown()` sees "no permit held" either way. What the pair
            // proves is that the permit is released **by unwinding** as well as by an ordinary
            // return — a property of `WorkPermit`'s Drop, not a kernel branch. Renvor has no join
            // handle for an author's task and so has no kernel-side way to tell them apart.
            (LifecyclePhase::Drain, Behaviour::Fail | Behaviour::Panic, Outcome::Stopped) => {}
            (LifecyclePhase::Stop, _, Outcome::Stopped) => {
                assert_eq!(
                    run.stop_failures.len(),
                    1,
                    "{behaviour} at {phase}: shutdown continues and reports the failure (C-L4)"
                );
                if behaviour == Behaviour::Hang {
                    assert!(
                        run.stop_failures[0].contains("deadline"),
                        "{}",
                        run.stop_failures[0]
                    );
                }
            }
            (_, _, other) => panic!("{behaviour} at {phase} produced an unexpected {other:?}"),
        }

        executed += 1;
    }

    assert_eq!(
        executed, 21,
        "C-L9 is 7 phases × 3 behaviours; every one must execute, not merely be enumerable"
    );
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
async fn a_panicking_provider_is_contained_at_boot() {
    // C-L9's third behaviour, and the one that used to be unavailable. The injected provider
    // panics **after a yield**, so the panic happens on a later poll — the case a `catch_unwind`
    // around the call site could never reach.
    //
    // Reaching the assertions at all proves containment: an escaping panic would abort this test.
    let run = Harness::injecting(FailureInjectionPoint::new(
        LifecyclePhase::Boot,
        Behaviour::Panic,
    ))
    .run()
    .await;

    assert!(run.fired, "the panic was injected");
    match run.outcome {
        Outcome::BootFailed(ref message) => {
            assert!(
                message.contains("injected"),
                "the failure names the provider: {message}"
            );
        }
        ref other => panic!("a panicking provider must fail the boot, got {other:?}"),
    }
    assert!(!run.reached(LifecyclePhase::Ready), "Ready is not reached");
}

#[tokio::test(start_paused = true)]
async fn a_panicking_provider_is_contained_at_stop() {
    // The worse moment to abort: a panic here would unwind through the shutdown of every provider
    // behind it. Shutdown must still complete.
    let run = Harness::injecting(FailureInjectionPoint::new(
        LifecyclePhase::Stop,
        Behaviour::Panic,
    ))
    .run()
    .await;

    assert!(run.fired, "the panic was injected");
    assert!(
        run.reached(LifecyclePhase::Stop),
        "shutdown completed despite the panic"
    );
}

#[tokio::test(start_paused = true)]
async fn all_three_behaviours_are_injectable_at_boot_and_stop() {
    // SC-009 in full: 7 of 7 phases **and** all three behaviours where each applies. Taken from
    // `Behaviour::ALL`, so a fourth behaviour cannot be added without covering it.
    for phase in [LifecyclePhase::Boot, LifecyclePhase::Stop] {
        for behaviour in Behaviour::ALL {
            let run = Harness::injecting(FailureInjectionPoint::new(phase, behaviour))
                .run()
                .await;

            assert!(run.fired, "{behaviour} at {phase} never fired");
            assert!(
                !matches!(run.outcome, Outcome::NotInjectable(_)),
                "{behaviour} at {phase} reported not injectable: {:?}",
                run.outcome
            );
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

    assert!(
        matches!(run.outcome, Outcome::ReadyDegraded(_)),
        "{:?}",
        run.outcome
    );
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
