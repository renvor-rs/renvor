//! `renvor doctor` — environment readiness.
//!
//! # It reports; it never installs
//!
//! FR-036's principle generalises: a diagnostic that fixes things is a diagnostic nobody can run
//! safely. `doctor` reads versions and reports them. Every remedy is printed for the operator to
//! run, never executed.
//!
//! # No network
//!
//! FR-043. Every probe below runs a local executable with `--version`. Nothing resolves a name or
//! opens a socket, which is why the offline test needs no network stub.

use std::process::Command;

use serde::Serialize;

use crate::exit::{CliError, Exit};
use crate::output::Reporter;

/// One thing checked.
#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    /// The executable.
    pub tool: String,
    /// Whether it was found and runnable.
    pub found: bool,
    /// Its reported version, when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Whether the command needs it, or merely benefits from it.
    pub required: bool,
    /// What to do when it is missing. Printed, never run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

/// What `doctor` looks for, and whether each is required.
const TOOLS: [(&str, bool, &str); 3] = [
    ("cargo", true, "install Rust from https://rustup.rs"),
    (
        "git",
        false,
        "install git from your platform's package manager",
    ),
    (
        "docker",
        false,
        "install a container runtime; only `renvor docker` needs it",
    ),
];

/// Probes one tool by running it with `--version`.
///
/// Running the executable rather than searching `PATH` is deliberate: a name on `PATH` that is not
/// executable, or is a broken shim, is exactly the case a diagnostic exists to catch, and a `PATH`
/// search reports it as present.
fn probe(tool: &str, required: bool, remedy: &str) -> Probe {
    let output = Command::new(tool).arg("--version").output();
    match output {
        Ok(output) if output.status.success() => Probe {
            tool: tool.to_owned(),
            found: true,
            version: Some(String::from_utf8_lossy(&output.stdout).trim().to_owned()),
            required,
            remedy: None,
        },
        _ => Probe {
            tool: tool.to_owned(),
            found: false,
            version: None,
            required,
            remedy: Some(remedy.to_owned()),
        },
    }
}

/// Orphaned staging directories found in the current directory.
///
/// # Why `doctor` reports these and does not remove them
///
/// `Staging`'s `Drop` cleans up on every ordinary failure including a panic, but a `SIGKILL` runs
/// no destructor — so residue survives, by design, named `.renvor-staging-{pid}-…` and placed
/// **beside** the destination rather than inside it. `tests/acceptance.rs` proves that.
///
/// The half that was missing is that nothing helped an operator find it. This does.
///
/// **It does not delete them** (T066, contract C-5). A diagnostic that deletes is a diagnostic
/// nobody can run safely on a directory they care about — and renvor cannot tell an abandoned
/// staging directory from one belonging to a `renvor new` running in another terminal **right
/// now**. The remedy is printed for the operator to run, with the process id visible so they can
/// check before removing anything.
fn orphaned_staging() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(".") else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".renvor-staging-"))
        .collect();
    // Sorted so the report is stable between runs and diffable.
    found.sort();
    found
}

/// Runs the command.
///
/// # Errors
///
/// [`crate::exit::Code::ToolMissing`] (exit `5`) when a **required** tool is absent. An optional
/// tool being absent is reported and is not a failure — exiting non-zero for something the
/// operator does not need is how a diagnostic gets wrapped in `|| true`.
pub fn run(reporter: &Reporter) -> Result<Exit, CliError> {
    let probes: Vec<Probe> = TOOLS
        .iter()
        .map(|(tool, required, remedy)| probe(tool, *required, remedy))
        .collect();

    if let Some(missing) = probes.iter().find(|probe| probe.required && !probe.found) {
        return Err(CliError::new(
            crate::exit::Code::ToolMissing,
            format!(
                "`{}` is required and was not found or could not be run",
                missing.tool
            ),
        )
        .with("tool", missing.tool.clone())
        .with("required", "true")
        .with("found", "false")
        .with("remedy", missing.remedy.clone().unwrap_or_default()));
    }

    let orphans = orphaned_staging();

    let mut human = probes
        .iter()
        .map(|probe| match (&probe.version, probe.required) {
            (Some(version), _) => format!("ok       {:<8} {version}", probe.tool),
            (None, true) => format!("MISSING  {:<8} required", probe.tool),
            (None, false) => format!("absent   {:<8} optional", probe.tool),
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !orphans.is_empty() {
        human.push_str(&format!(
            "\n\n{} orphaned staging {} in this directory, left by a run that was killed:",
            orphans.len(),
            if orphans.len() == 1 {
                "directory"
            } else {
                "directories"
            }
        ));
        for orphan in &orphans {
            human.push_str(&format!("\n  {orphan}"));
        }
        human.push_str(
            "\nrenvor has NOT removed them: one may belong to a `renvor new` running right now. \
             Check the process id in the name, then remove them yourself.",
        );
    }

    Ok(reporter.finish(
        "doctor",
        &human,
        serde_json::json!({ "probes": probes, "orphanedStaging": orphans }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_for_something_that_does_not_exist_reports_absent_with_a_remedy() {
        let probe = probe(
            "renvor-definitely-not-a-real-executable",
            false,
            "do the thing",
        );
        assert!(!probe.found);
        assert!(probe.version.is_none());
        assert_eq!(probe.remedy.as_deref(), Some("do the thing"));
    }

    #[test]
    fn a_probe_for_something_that_does_exist_reports_its_version() {
        // POSITIVE CONTROL. `cargo` is present wherever this test runs, by construction.
        let probe = probe("cargo", true, "install Rust");
        assert!(probe.found, "cargo must be runnable in a cargo test");
        assert!(probe.version.is_some_and(|v| v.contains("cargo")));
    }

    #[test]
    fn an_optional_tool_being_absent_is_not_a_required_failure() {
        // The rule that keeps `doctor` runnable in a container without docker.
        let optional: Vec<_> = TOOLS.iter().filter(|(_, required, _)| !*required).collect();
        assert!(!optional.is_empty(), "at least one tool must be optional");
    }

    #[test]
    fn every_tool_carries_a_remedy_the_operator_can_run_themselves() {
        // FR-036's principle: report, never install. A remedy with no text is a dead end.
        for (tool, _, remedy) in TOOLS {
            assert!(!remedy.is_empty(), "{tool} has no remedy text");
        }
    }
}
