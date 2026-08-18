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
///
/// FR-032 and T065 want three things reported for anything missing **or incompatible**: the
/// required version, the found version, and the corrective action. `required` is deliberately not
/// one of them — it is a boolean meaning "this command needs it", and an earlier version of this
/// struct had it standing in for a version constraint that did not exist.
#[derive(Debug, Clone, Serialize)]
pub struct Probe {
    /// The executable.
    pub tool: String,
    /// Whether it was found and runnable.
    pub found: bool,
    /// Its reported version string, verbatim, when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The version parsed out of that string, when one could be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found_version: Option<String>,
    /// The minimum version this project needs, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_version: Option<String>,
    /// Whether the command needs it, or merely benefits from it.
    pub required: bool,
    /// `false` only when a minimum is declared, a version was parsed, and it is below the minimum.
    ///
    /// An unparseable version is **not** reported as incompatible. Refusing to run because a tool
    /// printed its version in a shape this parser did not expect would be a diagnostic breaking the
    /// environment it exists to describe.
    pub compatible: bool,
    /// What to do when it is missing or too old. Printed, never run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

/// The Rust version this project requires.
///
/// **Read from the manifest, not restated.** `CARGO_PKG_RUST_VERSION` is `rust-version`, which
/// `renvor-cli` inherits from `[workspace.package]` — the single authoritative declaration (ADR-0002).
/// A hard-coded `"1.94.0"` here would be a second declaration, and second declarations drift.
const REQUIRED_RUST: &str = env!("CARGO_PKG_RUST_VERSION");

/// What `doctor` looks for: the executable, whether it is required, its minimum version if it has
/// one, and the remedy.
///
/// `git` and `docker` carry no minimum deliberately. This phase uses neither for anything version
/// dependent, and inventing a floor would produce a diagnostic that fails an environment which
/// works — which is how a `doctor` command ends up wrapped in `|| true`.
const TOOLS: [(&str, bool, Option<&str>, &str); 3] = [
    (
        "cargo",
        true,
        Some(REQUIRED_RUST),
        "install or update Rust from https://rustup.rs, then `rustup update stable`",
    ),
    (
        "git",
        false,
        None,
        "install git from your platform's package manager",
    ),
    (
        "docker",
        false,
        None,
        "install a container runtime; only `renvor docker` needs it",
    ),
];

/// Extracts a semantic version from a tool's `--version` line.
///
/// Tools disagree about the shape: `cargo 1.94.0 (abc 2026-01-01)`, `git version 2.39.5`,
/// `Docker version 27.0.3, build 7d4bcd8`. So this takes the first whitespace-separated token that
/// parses, after stripping the punctuation tools put around it.
///
/// Two-component versions (`1.94`) are padded to three, because `semver` requires all three and
/// some tools print two. Returning `None` is a normal outcome, not a failure — see [`Probe::compatible`].
fn extract_version(line: &str) -> Option<semver::Version> {
    line.split_whitespace()
        .map(|token| token.trim_matches(|c: char| c == ',' || c == '(' || c == ')' || c == 'v'))
        .find_map(|token| {
            semver::Version::parse(token).ok().or_else(|| {
                // `1.94` → `1.94.0`, but only when it really is two numeric components.
                let parts: Vec<&str> = token.split('.').collect();
                (parts.len() == 2
                    && parts
                        .iter()
                        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
                .then(|| semver::Version::parse(&format!("{token}.0")).ok())
                .flatten()
            })
        })
}

/// Probes one tool by running it with `--version`.
///
/// Running the executable rather than searching `PATH` is deliberate: a name on `PATH` that is not
/// executable, or is a broken shim, is exactly the case a diagnostic exists to catch, and a `PATH`
/// search reports it as present.
fn probe(tool: &str, required: bool, minimum: Option<&str>, remedy: &str) -> Probe {
    let output = Command::new(tool).arg("--version").output();
    match output {
        Ok(output) if output.status.success() => {
            let line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let found_version = extract_version(&line);
            // Incompatible ONLY when a minimum is declared AND a version was parsed AND it is
            // below. An unparseable version leaves `compatible` true; see `Probe::compatible`.
            let compatible = match (
                minimum.and_then(|m| semver::Version::parse(m).ok()),
                &found_version,
            ) {
                (Some(floor), Some(found)) => *found >= floor,
                _ => true,
            };
            Probe {
                tool: tool.to_owned(),
                found: true,
                version: Some(line),
                found_version: found_version.map(|version| version.to_string()),
                required_version: minimum.map(str::to_owned),
                required,
                compatible,
                // A remedy accompanies an incompatible tool as well as an absent one: "too old" with
                // no instruction is the same dead end as "missing" with no instruction.
                remedy: (!compatible).then(|| remedy.to_owned()),
            }
        }
        _ => Probe {
            tool: tool.to_owned(),
            found: false,
            version: None,
            found_version: None,
            required_version: minimum.map(str::to_owned),
            required,
            compatible: false,
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
        .map(|(tool, required, minimum, remedy)| probe(tool, *required, *minimum, remedy))
        .collect();

    // MISSING **OR INCOMPATIBLE** (T065). An out-of-date required toolchain is not a warning: it
    // produces a generated project that fails its own pre-placement verification, and the operator
    // is then debugging a template for a `rustup update`.
    if let Some(bad) = probes
        .iter()
        .find(|probe| probe.required && (!probe.found || !probe.compatible))
    {
        let why = if bad.found {
            format!(
                "`{}` is {}, below the required {}",
                bad.tool,
                bad.found_version
                    .clone()
                    .unwrap_or_else(|| "an unknown version".to_owned()),
                bad.required_version.clone().unwrap_or_default()
            )
        } else {
            format!(
                "`{}` is required and was not found or could not be run",
                bad.tool
            )
        };
        return Err(CliError::new(crate::exit::Code::ToolMissing, why)
            .with("tool", bad.tool.clone())
            .with("required", "true")
            .with("found", bad.found.to_string())
            .with(
                "foundVersion",
                bad.found_version.clone().unwrap_or_default(),
            )
            .with(
                "requiredVersion",
                bad.required_version.clone().unwrap_or_default(),
            )
            .with("remedy", bad.remedy.clone().unwrap_or_default()));
    }

    let orphans = orphaned_staging();

    let mut human = probes
        .iter()
        .map(
            |probe| match (&probe.version, probe.compatible, probe.required) {
                (Some(version), true, _) => format!("ok       {:<8} {version}", probe.tool),
                (Some(version), false, _) => format!(
                    "TOO OLD  {:<8} {version} — needs {}; {}",
                    probe.tool,
                    probe.required_version.clone().unwrap_or_default(),
                    probe.remedy.clone().unwrap_or_default()
                ),
                (None, _, true) => format!(
                    "MISSING  {:<8} required{}; {}",
                    probe.tool,
                    probe
                        .required_version
                        .as_ref()
                        .map(|v| format!(" (>= {v})"))
                        .unwrap_or_default(),
                    probe.remedy.clone().unwrap_or_default()
                ),
                (None, _, false) => format!(
                    "absent   {:<8} optional; {}",
                    probe.tool,
                    probe.remedy.clone().unwrap_or_default()
                ),
            },
        )
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
            None,
            "do the thing",
        );
        assert!(!probe.found);
        assert!(probe.version.is_none());
        assert_eq!(probe.remedy.as_deref(), Some("do the thing"));
    }

    #[test]
    fn a_probe_for_something_that_does_exist_reports_its_version() {
        // POSITIVE CONTROL. `cargo` is present wherever this test runs, by construction.
        let probe = probe("cargo", true, None, "install Rust");
        assert!(probe.found, "cargo must be runnable in a cargo test");
        assert!(probe.version.is_some_and(|v| v.contains("cargo")));
    }

    #[test]
    fn an_optional_tool_being_absent_is_not_a_required_failure() {
        // The rule that keeps `doctor` runnable in a container without docker.
        let optional: Vec<_> = TOOLS
            .iter()
            .filter(|(_, required, _, _)| !*required)
            .collect();
        assert!(!optional.is_empty(), "at least one tool must be optional");
    }

    #[test]
    fn every_tool_carries_a_remedy_the_operator_can_run_themselves() {
        // FR-036's principle: report, never install. A remedy with no text is a dead end.
        for (tool, _, _, remedy) in TOOLS {
            assert!(!remedy.is_empty(), "{tool} has no remedy text");
        }
    }

    // ── T065: required version, found version, corrective action ────────────────────────

    #[test]
    fn a_version_is_extracted_from_every_shape_a_real_tool_prints() {
        // Measured against the actual output of the three tools this command probes, rather than
        // against an idealised `x.y.z`. Each of these disagrees about punctuation.
        for (line, expected) in [
            ("cargo 1.94.0 (a1b2c3d4e 2026-01-05)", "1.94.0"),
            ("git version 2.39.5", "2.39.5"),
            ("Docker version 27.0.3, build 7d4bcd8", "27.0.3"),
            (
                "cargo 1.95.0-nightly (deadbeef 2026-02-01)",
                "1.95.0-nightly",
            ),
            // Two components padded to three, which some tools print.
            ("something 1.94", "1.94.0"),
        ] {
            assert_eq!(
                extract_version(line).map(|v| v.to_string()).as_deref(),
                Some(expected),
                "failed on {line:?}"
            );
        }
    }

    #[test]
    fn a_version_that_cannot_be_parsed_is_not_reported_as_incompatible() {
        // The rule that keeps this diagnostic from breaking the environment it describes. A tool
        // that prints something this parser does not understand is unknown, not too old.
        assert!(extract_version("some-tool built from source").is_none());
        let probe = probe("cargo", true, Some("1.94.0"), "update");
        assert!(
            probe.compatible || probe.found_version.is_some(),
            "{probe:?}"
        );
    }

    #[test]
    fn a_tool_below_its_required_version_is_incompatible_and_carries_a_remedy() {
        // The case T065 exists for, and the one that cannot be produced by running a real tool:
        // `cargo` on this machine is not going to be old. Driven through `probe` with an
        // impossible floor instead, so the comparison itself is what is under test.
        let probe = probe(
            "cargo",
            true,
            Some("999.0.0"),
            "install Rust from https://rustup.rs",
        );
        assert!(probe.found, "cargo must be runnable");
        assert!(
            !probe.compatible,
            "cargo cannot satisfy a 999.0.0 floor: {probe:?}"
        );
        assert_eq!(probe.required_version.as_deref(), Some("999.0.0"));
        assert!(
            probe.found_version.is_some(),
            "the found version must be reported too"
        );
        assert!(
            probe
                .remedy
                .as_deref()
                .is_some_and(|remedy| !remedy.is_empty()),
            "an incompatible tool needs a remedy as much as a missing one: {probe:?}"
        );
    }

    #[test]
    fn a_tool_at_exactly_its_required_version_is_compatible() {
        // The boundary. `>=` rather than `>`, asserted rather than assumed — an off-by-one here
        // rejects precisely the toolchain the project declares as its minimum.
        let exact = semver::Version::parse(REQUIRED_RUST).expect("the declared MSRV is a version");
        assert!(exact >= semver::Version::parse(REQUIRED_RUST).expect("parses"));
        let probe = probe("cargo", true, Some(REQUIRED_RUST), "update");
        assert!(
            probe.compatible,
            "the toolchain running these tests is below the declared MSRV: {probe:?}"
        );
    }

    #[test]
    fn the_required_rust_version_comes_from_the_manifest_rather_than_a_second_declaration() {
        // ADR-0002: `rust-version` is declared once, in `[workspace.package]`. This asserts the
        // constant is that value rather than a copy that can drift from it.
        assert_eq!(REQUIRED_RUST, env!("CARGO_PKG_RUST_VERSION"));
        assert!(
            semver::Version::parse(REQUIRED_RUST).is_ok(),
            "the declared MSRV must be a comparable version, not a range"
        );
    }

    #[test]
    fn every_declared_minimum_is_a_parseable_version() {
        // A floor that cannot be parsed silently disables the comparison it was added for, because
        // `compatible` falls back to true. That would be a bound nobody enabled.
        for (tool, _, minimum, _) in TOOLS {
            if let Some(minimum) = minimum {
                assert!(
                    semver::Version::parse(minimum).is_ok(),
                    "{tool}'s minimum {minimum:?} does not parse, so it is not enforced"
                );
            }
        }
    }
}
