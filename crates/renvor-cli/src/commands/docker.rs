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

/// Why the container runtime could not be used.
///
/// A closed set rather than a free string, so `details.reason` is matchable by a consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unavailable {
    /// The executable is not on `PATH`, or is not runnable.
    NotInstalled,
    /// The executable runs and the daemon does not answer.
    NotRunning,
}

impl Unavailable {
    const fn reason(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::NotRunning => "not_running",
        }
    }

    const fn remedy(self) -> &'static str {
        match self {
            Self::NotInstalled => {
                "install a container runtime; `renvor docker` is the only \
                                   command that needs one"
            }
            Self::NotRunning => {
                "the runtime is installed but its daemon is not answering; start \
                                 it and try again"
            }
        }
    }
}

/// Distinguishes the two failures by running two different probes.
///
/// `docker --version` answers from the client alone, so it succeeds while the daemon is down.
/// `docker info` requires the daemon. Running both is what makes the distinction real rather than
/// guessed from an error string.
fn availability() -> Result<(), Unavailable> {
    let client = std::process::Command::new("docker")
        .arg("--version")
        .output();
    match client {
        Ok(output) if output.status.success() => {}
        _ => return Err(Unavailable::NotInstalled),
    }

    let daemon = std::process::Command::new("docker").arg("info").output();
    match daemon {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(Unavailable::NotRunning),
    }
}

/// Runs the command in a project directory.
///
/// # Errors
///
/// [`Code::ContainerRuntimeUnavailable`] (exit `5`) with `details.reason`, or
/// [`Code::ManifestInvalid`] when the project has no container controls to drive.
pub fn run(reporter: &Reporter, path: &std::path::Path, action: Action) -> Result<Exit, CliError> {
    if !path.join("compose.yaml").is_file() {
        return Err(CliError::new(
            Code::ManifestInvalid,
            format!(
                "`{}` has no `compose.yaml`; generate the project with `--container` to get \
                 container development controls",
                path.display()
            ),
        )
        .with("field", "compose.yaml")
        .with("constraint", "must exist; generate with `--container`"));
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
            .with("reason", Unavailable::NotInstalled.reason())
        })?;

    if !status.success() {
        return Err(CliError::new(
            Code::ContainerRuntimeUnavailable,
            "the container runtime reported a failure; its output is above",
        )
        .with("reason", "command_failed"));
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
        let error = run(&Reporter::new(Format::Human, true), dir.path(), Action::Up).unwrap_err();
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "field" && v == "compose.yaml")
        );
    }

    #[test]
    fn the_two_unavailability_reasons_are_distinct_and_matchable() {
        // C-2 requires `details.reason` to distinguish them, which only helps if the strings
        // differ and neither is prose.
        assert_ne!(
            Unavailable::NotInstalled.reason(),
            Unavailable::NotRunning.reason()
        );
        assert_eq!(Unavailable::NotInstalled.reason(), "not_installed");
        assert_eq!(Unavailable::NotRunning.reason(), "not_running");
        assert_ne!(
            Unavailable::NotInstalled.remedy(),
            Unavailable::NotRunning.remedy()
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
