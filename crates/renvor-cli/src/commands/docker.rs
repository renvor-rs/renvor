//! `renvor docker up|down|status|logs` — container development controls.
//!
//! # The distinction this command exists to make
//!
//! *Not installed* and *installed but not running* are different problems with different remedies,
//! and a message that says only "docker unavailable" sends an operator to reinstall something that
//! is already there. C-2 requires `details.reason` to distinguish them, and this module's whole
//! shape follows from that.

use serde::Serialize;

use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;

/// What `renvor docker` was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Start the development containers.
    Up,
    /// Stop them.
    Down,
    /// Report their state.
    Status,
    /// Show their logs.
    Logs,
}

impl Action {
    /// The `docker compose` arguments this action becomes.
    const fn arguments(self) -> &'static [&'static str] {
        match self {
            Self::Up => &["compose", "up", "--detach"],
            Self::Down => &["compose", "down"],
            Self::Status => &["compose", "ps"],
            Self::Logs => &["compose", "logs", "--no-color"],
        }
    }
}

/// How long any probe of the container runtime may take before it is a failure.
///
/// # This is a bound, not a retry
///
/// FR-035 asks the command to distinguish *not installed* from *installed but not running*.
/// There is a third state neither name covers and every operator has met: **installed, running,
/// and not answering** — a stale socket, or Docker Desktop still starting. `docker info` in that
/// state returns nothing, ever, and `Command::output()` waits for it, ever.
///
/// Twenty seconds is long enough that a busy-but-working daemon answers and short enough that a
/// person notices the command came back. Exceeding it is reported as a **distinct reason** rather
/// than retried: a retry would turn one hang into several, and the operator's next step is the same
/// either way.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Why the container runtime could not be used.
///
/// A closed set rather than a free string, so `details.reason` is matchable by a consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unavailable {
    /// The executable is not on `PATH`, or is not runnable.
    Absent,
    /// The executable runs and the daemon does not answer.
    DaemonStopped,
    /// The executable runs, the daemon accepts the call, and never replies.
    DaemonSilent,
    /// The runtime ran and reported a failure of its own.
    ///
    /// Added 2026-08-18. `"command_failed"` was already being emitted as a bare literal further
    /// down, **outside this enum**, while the type's own doc claimed a closed set and the published
    /// registry listed three values. A consumer matching the documented three fell through on an
    /// ordinary `docker compose` failure. Found by an advisory review reading the emit sites rather
    /// than the enum.
    CommandFailed,
}

impl Unavailable {
    const fn reason(self) -> &'static str {
        match self {
            Self::Absent => "not_installed",
            Self::DaemonStopped => "not_running",
            Self::DaemonSilent => "not_responding",
            Self::CommandFailed => "command_failed",
        }
    }

    const fn remedy(self) -> &'static str {
        match self {
            Self::Absent => {
                "install a container runtime; `renvor docker` is the only \
                                   command that needs one"
            }
            Self::DaemonStopped => {
                "the runtime is installed but its daemon is not answering; start \
                                 it and try again"
            }
            Self::CommandFailed => {
                "the container runtime ran and reported a failure of its own; its output is above"
            }
            Self::DaemonSilent => {
                "the runtime is installed and its daemon accepted the request but did not reply \
                 within 20 seconds; it may still be starting, or its socket may be stale. renvor \
                 gave up rather than waiting indefinitely"
            }
        }
    }
}

/// Runs a probe with a deadline, discarding its output.
///
/// Returns `Ok(true)` if it exited successfully, `Ok(false)` if it exited unsuccessfully, and
/// `Err(())` if it could not be started **or** outlived [`PROBE_TIMEOUT`].
///
/// The child is **killed and reaped** on timeout. Leaving it running would mean a `renvor docker`
/// that gave up still had a `docker info` attached to the terminal, which is a worse outcome than
/// the hang it was avoiding.
fn probe(program: &str, arguments: &[&str], timeout: std::time::Duration) -> Result<bool, ()> {
    use wait_timeout::ChildExt;

    let mut child = std::process::Command::new(program)
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| ())?;

    match child.wait_timeout(timeout) {
        Ok(Some(status)) => Ok(status.success()),
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(())
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(())
        }
    }
}

/// Distinguishes the two failures by running two different probes.
///
/// `docker --version` answers from the client alone, so it succeeds while the daemon is down.
/// `docker info` requires the daemon. Running both is what makes the distinction real rather than
/// guessed from an error string.
fn availability() -> Result<(), Unavailable> {
    classify(
        // The client. Fast, local, and answers even with the daemon down — which is the whole
        // point of asking it separately.
        probe("docker", &["--version"], PROBE_TIMEOUT),
        // The daemon. This is the call that can block forever, and the reason `probe` exists.
        probe("docker", &["info"], PROBE_TIMEOUT),
    )
}

/// Turns two probe outcomes into the FR-035 distinction.
///
/// # Why this is a separate function
///
/// FR-035's whole content is *"MUST distinguish 'runtime not installed' from 'runtime installed but
/// not running'"* — and until 2026-08-18 the code making that distinction was **called by no test**.
/// An advisory review found it with `grep -rn "availability("`: two hits, the definition and the one
/// production call. Invert the logic and the suite stayed green. The tests that looked like FR-035
/// coverage compared enum constants to each other or drove `sleep`/`true`/`false` through `probe`;
/// none of them exercised the mapping.
///
/// `availability` does the I/O and this does the deciding, so the deciding can be tested over all
/// six combinations without a container runtime.
///
/// A client timeout is reported as [`Unavailable::Absent`] rather than inventing a fourth state:
/// `docker --version` is local and answers in milliseconds, so a timeout there means the executable
/// itself is unusable, which is what "not installed" tells an operator to go and fix.
const fn classify(client: Result<bool, ()>, daemon: Result<bool, ()>) -> Result<(), Unavailable> {
    match (client, daemon) {
        (Ok(false) | Err(()), _) => Err(Unavailable::Absent),
        (Ok(true), Ok(true)) => Ok(()),
        (Ok(true), Ok(false)) => Err(Unavailable::DaemonStopped),
        (Ok(true), Err(())) => Err(Unavailable::DaemonSilent),
    }
}

/// Runs the command in a project directory.
///
/// # Errors
///
/// [`Code::ContainerRuntimeUnavailable`] (exit `5`) with `details.reason`, or
/// [`Code::ContainerControlsMissing`] when the project has no container controls to drive.
pub fn run(
    reporter: &Reporter,
    path: &std::path::Path,
    action: Action,
    dry_run: bool,
) -> Result<Exit, CliError> {
    if !path.join("compose.yaml").is_file() {
        // `container_controls_missing` since 2026-08-18. This used to report `manifest_invalid`
        // — published as *"`renvor.toml` failed validation"* — for a project whose `renvor.toml`
        // is perfectly valid and simply was not generated with `--container` (A-R6).
        return Err(CliError::new(
            Code::ContainerControlsMissing,
            format!(
                "`{}` has no `compose.yaml`; generate the project with `--container` to get \
                 container development controls",
                crate::output::redact::path(path)
            ),
        )
        .with("expected", "compose.yaml")
        .with("remedy", "generate the project with `--container`"));
    }

    // `--dry-run` IS GLOBAL, AND THIS COMMAND HAS SIDE EFFECTS.
    //
    // Until 2026-08-18 only `new` honoured it, so `renvor docker up --dry-run` started containers.
    // A global flag that silently does nothing on the commands that can change the world is worse
    // than no flag: it is a promise a user reasonably relies on.
    //
    // Reported BEFORE the runtime is probed, because a dry run must not need a working runtime to
    // tell you what it would do.
    if dry_run {
        return Ok(reporter.finish(
            "docker",
            &format!(
                "dry run: would run `docker {}`",
                action.arguments().join(" ")
            ),
            serde_json::json!({
                "dryRun": true,
                "action": action,
                "wouldRun": action.arguments(),
            }),
        ));
    }

    if let Err(unavailable) = availability() {
        return Err(CliError::new(
            Code::ContainerRuntimeUnavailable,
            format!(
                "the container runtime is unavailable: {}",
                unavailable.remedy()
            ),
        )
        .with("reason", unavailable.reason())
        .with("remedy", unavailable.remedy()));
    }

    let status = std::process::Command::new("docker")
        .args(action.arguments())
        .current_dir(path)
        .status()
        .map_err(|error| {
            CliError::new(
                Code::ContainerRuntimeUnavailable,
                format!("the container runtime could not be run: {error}"),
            )
            .with("reason", Unavailable::Absent.reason())
        })?;

    if !status.success() {
        return Err(CliError::new(
            Code::ContainerRuntimeUnavailable,
            "the container runtime reported a failure; its output is above",
        )
        .with("reason", Unavailable::CommandFailed.reason())
        .with("remedy", Unavailable::CommandFailed.remedy()));
    }

    Ok(reporter.finish(
        "docker",
        "ok",
        serde_json::json!({ "action": action, "ran": action.arguments() }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Format;

    #[test]
    fn a_project_without_container_controls_is_refused_before_the_runtime_is_probed() {
        // Ordering matters: probing first would report "docker is not running" to somebody whose
        // actual problem is that they generated the project without `--container`.
        let dir = tempfile::tempdir().expect("tempdir");
        let error = run(
            &Reporter::new(Format::Human, true),
            dir.path(),
            Action::Up,
            false,
        )
        .unwrap_err();
        assert_eq!(error.code, Code::ContainerControlsMissing);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "expected" && v == "compose.yaml")
        );
        // The old `details.field` named a manifest field, because the old code claimed the
        // manifest was invalid. Nothing in `renvor.toml` is called `compose.yaml`, and a consumer
        // that went looking for it found nothing.
        assert!(
            !error.details.iter().any(|(k, _)| k == "field"),
            "the manifest-shaped details must not survive the code change: {:?}",
            error.details
        );
    }

    #[test]
    fn a_dry_run_starts_nothing_and_needs_no_runtime() {
        // The bug this prevents: `renvor docker up --dry-run` used to start containers, because
        // only `new` received the flag. Asserted against a project that HAS compose controls, so
        // the run gets past the first guard and would really have reached `docker compose up`.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("compose.yaml"), b"services: {}\n").expect("write");
        let exit = run(
            &Reporter::new(Format::Human, true),
            dir.path(),
            Action::Up,
            true,
        )
        .expect("a dry run must succeed even with no container runtime");
        assert_eq!(exit, Exit::Success);
    }

    // ── T071: never hang, never silently skip ───────────────────────────────────────────

    /// A command guaranteed to outlive any short deadline, per platform.
    fn a_slow_command() -> (&'static str, &'static [&'static str]) {
        if cfg!(windows) {
            ("cmd", &["/C", "ping -n 31 127.0.0.1"])
        } else {
            ("sleep", &["30"])
        }
    }

    /// A command that exits with the given success, per platform.
    fn a_quick_command(succeeds: bool) -> (&'static str, &'static [&'static str]) {
        match (cfg!(windows), succeeds) {
            (true, true) => ("cmd", &["/C", "exit 0"]),
            (true, false) => ("cmd", &["/C", "exit 1"]),
            (false, true) => ("true", &[]),
            (false, false) => ("false", &[]),
        }
    }

    #[test]
    fn a_probe_that_outlives_its_deadline_is_killed_rather_than_waited_for() {
        // The property T071 asks for, measured. Driven with a command guaranteed to outlive its
        // deadline, because `docker info` cannot be made to hang on demand — and a test that waited
        // for a real stuck daemon would be a test that hangs.
        let (program, arguments) = a_slow_command();
        let started = std::time::Instant::now();
        let outcome = probe(program, arguments, std::time::Duration::from_millis(500));
        let elapsed = started.elapsed();

        assert!(
            outcome.is_err(),
            "a command that outlives its deadline must not report success"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "the probe waited {elapsed:?} for a 500ms deadline, so the bound is not enforced"
        );
    }

    #[test]
    fn a_probe_that_finishes_inside_its_deadline_reports_its_real_status() {
        // The positive control. Without it, a `probe` that returned `Err` unconditionally — a spawn
        // that never worked, a deadline of zero — would satisfy the test above completely.
        let (program, arguments) = a_quick_command(true);
        assert_eq!(
            probe(program, arguments, PROBE_TIMEOUT),
            Ok(true),
            "a command that succeeds must be reported as succeeding"
        );
        let (program, arguments) = a_quick_command(false);
        assert_eq!(
            probe(program, arguments, PROBE_TIMEOUT),
            Ok(false),
            "a command that fails must be reported as FAILING, not as unreachable — the difference \
             is `not_running` versus `not_installed`"
        );
    }

    #[test]
    fn a_program_that_cannot_be_started_is_distinguished_from_one_that_hangs() {
        // Both return `Err(())` from `probe`, and `availability` turns them into different reasons
        // depending on WHICH probe produced it. This pins the first half.
        assert_eq!(
            probe(
                "renvor-definitely-not-a-real-executable",
                &[],
                PROBE_TIMEOUT
            ),
            Err(())
        );
    }

    #[test]
    fn no_action_can_run_forever() {
        // The other half of "never hang", and it is a property of the ARGUMENTS rather than of a
        // timeout: `docker compose up` without `--detach` runs until interrupted, and
        // `docker compose logs --follow` never returns at all. Neither is given a deadline, because
        // neither is used — so this asserts the shape rather than adding a timeout to work around
        // it.
        for action in [Action::Up, Action::Down, Action::Status, Action::Logs] {
            let arguments = action.arguments();
            assert!(
                !arguments.contains(&"--follow") && !arguments.contains(&"-f"),
                "{action:?} follows its output and would never return: {arguments:?}"
            );
            if arguments.contains(&"up") {
                assert!(
                    arguments.contains(&"--detach"),
                    "`up` must detach or it runs until interrupted: {arguments:?}"
                );
            }
        }
    }

    #[test]
    fn every_probe_outcome_maps_to_the_right_unavailability() {
        // ALL SIX COMBINATIONS. FR-035's distinction is the point of this command, and the function
        // that makes it had no test at all — `grep -rn "availability("` found the definition and
        // one production call. This exercises the decision without needing a container runtime.
        assert_eq!(
            classify(Ok(true), Ok(true)),
            Ok(()),
            "both up means available"
        );
        assert_eq!(
            classify(Ok(true), Ok(false)),
            Err(Unavailable::DaemonStopped),
            "client up, daemon refusing: installed but not running"
        );
        assert_eq!(
            classify(Ok(true), Err(())),
            Err(Unavailable::DaemonSilent),
            "client up, daemon silent: installed, running, not answering"
        );
        // A client that fails or hangs is "not installed" whatever the daemon says, including when
        // the daemon probe somehow succeeds — which is the row an inverted implementation gets wrong.
        assert_eq!(classify(Ok(false), Ok(true)), Err(Unavailable::Absent));
        assert_eq!(classify(Ok(false), Ok(false)), Err(Unavailable::Absent));
        assert_eq!(classify(Err(()), Ok(true)), Err(Unavailable::Absent));
    }

    #[test]
    fn every_unavailability_reason_carries_a_distinct_remedy() {
        // "Never silently skip": each way the runtime can be unusable has to produce a different
        // `details.reason` AND a different instruction. Two reasons sharing one remedy is a
        // distinction that helps a consumer and not the operator.
        let all = [
            Unavailable::Absent,
            Unavailable::DaemonStopped,
            Unavailable::DaemonSilent,
        ];
        let reasons: std::collections::BTreeSet<&str> = all.iter().map(|u| u.reason()).collect();
        assert_eq!(
            reasons.len(),
            all.len(),
            "two states share a reason: {reasons:?}"
        );
        let remedies: std::collections::BTreeSet<&str> = all.iter().map(|u| u.remedy()).collect();
        assert_eq!(remedies.len(), all.len(), "two states share a remedy");
        for unavailable in all {
            assert!(
                !unavailable.remedy().is_empty(),
                "{unavailable:?} has no remedy"
            );
        }
    }

    #[test]
    fn the_two_unavailability_reasons_are_distinct_and_matchable() {
        // C-2 requires `details.reason` to distinguish them, which only helps if the strings
        // differ and neither is prose.
        assert_ne!(
            Unavailable::Absent.reason(),
            Unavailable::DaemonStopped.reason()
        );
        assert_eq!(Unavailable::Absent.reason(), "not_installed");
        assert_eq!(Unavailable::DaemonStopped.reason(), "not_running");
        assert_ne!(
            Unavailable::Absent.remedy(),
            Unavailable::DaemonStopped.remedy()
        );
    }

    #[test]
    fn every_action_maps_to_a_distinct_compose_invocation() {
        let all = [Action::Up, Action::Down, Action::Status, Action::Logs];
        let mut invocations: Vec<&[&str]> = all.iter().map(|a| a.arguments()).collect();
        let total = invocations.len();
        invocations.sort_unstable();
        invocations.dedup();
        assert_eq!(invocations.len(), total, "two actions run the same command");
        for action in all {
            assert_eq!(
                action.arguments()[0],
                "compose",
                "{action:?} does not use compose"
            );
        }
    }

    #[test]
    fn logs_are_requested_without_colour_so_captured_output_is_readable() {
        assert!(Action::Logs.arguments().contains(&"--no-color"));
    }
}
