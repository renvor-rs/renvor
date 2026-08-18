//! Pre-placement verification (FR-030).
//!
//! > *"Generation MUST verify the project before reporting success, so that a project that does not
//! > build is a generation failure rather than a user's discovery."*
//!
//! # Where this runs, and why that is the whole point
//!
//! **In staging, before the rename.** A project that fails its own checks never reaches the
//! destination, so a failed generation leaves nothing to clean up and nothing to explain.
//! Verifying after placement would turn a generator bug into the operator's problem.
//!
//! # This step deliberately steps outside the capability boundary, and says so
//!
//! Every other filesystem operation in this crate goes through a [`cap_std::fs::Dir`] handle.
//! This one cannot: `std::process::Command` takes a **path** for its working directory, and there
//! is no capability-based process API on any supported platform.
//!
//! So the exception is bounded and stated rather than quietly taken:
//!
//! - the path is `<parent the operator typed>/<staging name this process generated>`, not anything
//!   derived from a template or from configuration;
//! - the programs run are fixed string literals, never interpolated;
//! - no argument comes from user input.
//!
//! # Build output does not become part of the project
//!
//! `CARGO_TARGET_DIR` is redirected to a temporary directory **outside** staging. Without that,
//! `target/` would be renamed into the destination along with the project and would appear in the
//! manifest — several hundred megabytes of build artifacts presented as generated source. The test
//! module asserts the manifest is byte-identical before and after verification.
//!
//! # Offline
//!
//! FR-043. The generated skeleton declares **no dependencies**, so `cargo build` resolves nothing
//! from the network. That is a property of the template rather than of a network stub, and the
//! template test suite is what keeps it true.

use std::path::Path;
use std::process::Command;

use crate::exit::{CliError, Code};

/// The checks run, in order, each with the failure it reports.
///
/// Ordered cheapest-first so the common failure is reported in a second rather than in a minute.
const CHECKS: [(&str, &[&str], &str); 3] = [
    ("cargo", &["fmt", "--check"], "is not correctly formatted"),
    ("cargo", &["build"], "does not compile"),
    ("cargo", &["test"], "does not pass its own tests"),
];

/// Runs the generated project's own checks while it is still in staging.
///
/// # Errors
///
/// [`Code::RenderFailed`] if any check fails — the project was generated and is **wrong**, which
/// is a generation failure — or [`Code::ToolMissing`] if `cargo` cannot be run at all.
pub fn in_staging(staging: &Path) -> Result<(), CliError> {
    let target = tempfile::tempdir().map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!("a build directory for verification could not be created: {error}"),
        )
    })?;

    for (program, arguments, complaint) in CHECKS {
        let output = Command::new(program)
            .args(arguments)
            .current_dir(staging)
            .env("CARGO_TARGET_DIR", target.path())
            .output()
            .map_err(|error| {
                CliError::new(
                    Code::ToolMissing,
                    format!(
                        "`{program}` could not be run to verify the generated project: {error}"
                    ),
                )
                .with("tool", program)
                .with("required", "true")
                .with("found", "false")
            })?;

        if !output.status.success() {
            // The generated project is the defect, not the operator's input. Reporting the tool's
            // own output is what makes that actionable — a bare "verification failed" would send
            // somebody to re-read their flags for a bug in a template.
            let detail = String::from_utf8_lossy(&output.stderr);
            return Err(CliError::new(
                Code::RenderFailed,
                format!(
                    "the generated project {complaint}; nothing was written to the destination. \
                     This is a defect in renvor's templates, not in your command.\n{}",
                    detail.trim()
                ),
            )
            .with("check", format!("{program} {}", arguments.join(" ")))
            .with("stage", "pre-placement verification"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(main: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            b"[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n",
        )
        .expect("write");
        std::fs::write(dir.path().join("src/main.rs"), main).expect("write");
        dir
    }

    #[test]
    fn a_project_that_builds_and_tests_passes() {
        // POSITIVE CONTROL. Without it, a verifier that rejected everything would satisfy every
        // failure test below and make `renvor new` impossible to use.
        let dir = project("fn main() {}\n");
        in_staging(dir.path()).expect("a correct project must verify");
    }

    #[test]
    fn a_project_that_does_not_compile_is_a_generation_failure() {
        let dir = project("fn main() { this is not rust }\n");
        let error = in_staging(dir.path()).unwrap_err();
        assert_eq!(error.code, Code::RenderFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "stage" && v == "pre-placement verification"),
            "the failure must say it happened before placement"
        );
    }

    #[test]
    fn a_project_that_is_not_formatted_is_a_generation_failure() {
        // The defect that motivated this module: MiniJinja stripping a trailing newline produced a
        // project that compiled and failed `cargo fmt --check`. Compilation alone would have
        // shipped it.
        let dir = project("fn main() {   }");
        let error = in_staging(dir.path()).unwrap_err();
        assert_eq!(error.code, Code::RenderFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "check" && v.contains("fmt")),
            "the failing check must be named"
        );
    }

    #[test]
    fn verification_leaves_no_build_output_in_the_project() {
        // The rename moves whatever is in staging. A `target/` left here would be renamed into the
        // destination and would appear in the manifest as generated source.
        let dir = project("fn main() {}\n");
        in_staging(dir.path()).expect("verifies");
        assert!(
            !dir.path().join("target").exists(),
            "verification left build output that placement would have moved into the project"
        );
        let entries: Vec<String> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let mut sorted = entries.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec![
                "Cargo.lock".to_owned(),
                "Cargo.toml".to_owned(),
                "src".to_owned()
            ],
            "verification added or removed a file"
        );
    }
}
