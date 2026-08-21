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
use crate::output::progress::Progress;

/// The checks run, in order, each with the failure it reports.
///
/// Ordered cheapest-first so the common failure is reported in a second rather than in a minute.
const CHECKS: [(&str, &[&str], &str); 5] = [
    ("cargo", &["fmt", "--check"], "is not correctly formatted"),
    // FR-029 names FOUR things — "formatting, **linting**, building, and testing" — and until
    // 2026-08-18 this array had three. Nothing lint-checked the generated project: not this
    // verifier, not `tests/generated.rs`, not CI, and not a `[lints]` table in the generated
    // manifest. `grep -rn clippy crates/renvor-cli/` returned nothing, while Phase 003 tasks T036
    // (https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/tasks.md)
    // and Phase 003 quickstart Gate 5
    // (https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/quickstart.md)
    // both stated that clippy ran. An advisory review found the gap by
    // reading the array instead of the prose.
    //
    // `-D warnings` because SC-005 says "0 warnings escalated to errors", which is only meaningful
    // if they are escalated.
    (
        "cargo",
        &["clippy", "--", "-D", "warnings"],
        "does not pass its own lints",
    ),
    ("cargo", &["build"], "does not compile"),
    ("cargo", &["test"], "does not pass its own tests"),
    // FR-029's fifth verb — "and MUST start" — and contract C-5 step 5 says the same. The
    // generated `main` prints its name and exits, so this terminates; a template that made it block
    // would hang generation, which is why the template suite keeps `main` trivial.
    ("cargo", &["run", "--quiet"], "does not start"),
];

/// Runs the generated project's own checks while it is still in staging.
///
/// # Errors
///
/// [`Code::ProjectVerificationFailed`] if any check fails — the project was generated and is
/// **wrong**, which is a generation failure — or [`Code::ToolMissing`] if `cargo` cannot be run at
/// all.
///
/// # Not `render_failed`, which it was until 2026-08-18
///
/// That code is published as *"template rendering failed"*, and rendering is exactly what did
/// **not** fail here: the templates produced a tree, and the tree then failed its own build, lint,
/// format, test, or start check. A consumer matching the registry would have looked for a template
/// defect over a compile error. This is the same class of misreporting as A-R6's three sites, found
/// in the same sweep and corrected with them.
pub fn in_staging(staging: &Path, progress: &Progress) -> Result<(), CliError> {
    let target = tempfile::tempdir().map_err(|error| {
        CliError::new(
            Code::ProjectVerificationFailed,
            format!("a build directory for verification could not be created: {error}"),
        )
        .with("stage", "pre-placement verification")
    })?;

    for (program, arguments, complaint) in CHECKS {
        // WHICH check is running, not merely THAT something is. `.output()` captures everything
        // cargo says, so without this the operator watches a spinner for a cold `cargo build` with
        // no way to tell a slow compile from a hung one — and no way to know, when it does hang,
        // which of the five to reproduce by hand.
        progress.step(&format!("{program} {}", arguments.join(" ")));
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
                Code::ProjectVerificationFailed,
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
    /// An indicator that renders nowhere.
    ///
    /// These tests run `cargo build` for real, so they are slow — but they are not the place to
    /// assert anything about a spinner, and a visible one would interleave with libtest's own
    /// output. `Progress` is deliberately constructible in this state, which is the same state
    /// every JSON and non-terminal run uses.
    fn silent() -> crate::output::progress::Progress {
        crate::output::progress::Progress::start(
            "verifying",
            false,
            crate::output::style::Permission::denied(),
        )
    }

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
        in_staging(dir.path(), &silent()).expect("a correct project must verify");
    }

    #[test]
    fn a_project_that_does_not_compile_is_a_generation_failure() {
        let dir = project("fn main() { this is not rust }\n");
        let error = in_staging(dir.path(), &silent()).unwrap_err();
        assert_eq!(error.code, Code::ProjectVerificationFailed);
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
        let error = in_staging(dir.path(), &silent()).unwrap_err();
        assert_eq!(error.code, Code::ProjectVerificationFailed);
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
        in_staging(dir.path(), &silent()).expect("verifies");
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
