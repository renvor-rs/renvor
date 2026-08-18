//! `renvor dev` — the local development loop.
//!
//! # What this does in Phase 003, stated rather than implied
//!
//! There is no transport until Phase 004, so there is no server to reload. `dev` therefore runs the
//! generated project's own build-and-test loop and reports it. That is genuinely useful and it is
//! **not** what `dev` will eventually mean.
//!
//! It is documented here rather than quietly under-delivered, because a command whose behaviour
//! changes shape between phases is a compatibility question, and pretending Phase 003 shipped a
//! watch loop would make Phase 004 look like a regression.

use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;

/// Runs the command in a project directory.
///
/// # Errors
///
/// [`Code::ToolMissing`] (exit `5`) if `cargo` cannot be run, or [`Code::ManifestInvalid`] if the
/// directory is not a renvor project.
pub fn run(reporter: &Reporter, path: &std::path::Path, dry_run: bool) -> Result<Exit, CliError> {
    if !path.join("renvor.toml").is_file() {
        return Err(CliError::new(
            Code::ManifestInvalid,
            format!(
                "`{}` is not a renvor project; there is no `renvor.toml` here",
                path.display()
            ),
        )
        .with("field", "renvor.toml")
        .with("constraint", "must exist in the project directory"));
    }

    // `--dry-run` is global and this command runs a build. See the note in `docker.rs`.
    if dry_run {
        return Ok(reporter.finish(
            "dev",
            "dry run: would run `cargo test` in the project",
            serde_json::json!({ "dryRun": true, "wouldRun": ["cargo", "test"] }),
        ));
    }

    reporter.note("running `cargo test` in the project (Phase 003 has no transport to reload)");

    let status = std::process::Command::new("cargo")
        .arg("test")
        .current_dir(path)
        .status()
        .map_err(|error| {
            CliError::new(
                Code::ToolMissing,
                format!("`cargo` could not be run: {error}"),
            )
            .with("tool", "cargo")
            .with("required", "true")
            .with("found", "false")
        })?;

    if !status.success() {
        // NOT an internal error and NOT a renvor failure: the operator's tests failed, which is
        // information rather than a defect. Exit 3 says "the thing you asked about is not ok".
        return Err(CliError::new(
            Code::ManifestInvalid,
            "the project's own tests failed; the output above is from `cargo test`",
        )
        .with("field", "project")
        .with("constraint", "`cargo test` must pass"));
    }

    Ok(reporter.finish(
        "dev",
        "ok  the project builds and its tests pass",
        serde_json::json!({ "ran": "cargo test", "passed": true }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Format;

    #[test]
    fn a_dry_run_builds_nothing() {
        // `renvor dev --dry-run` used to run the whole test suite.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("renvor.toml"), b"[renvor]\n").expect("write");
        run(&Reporter::new(Format::Human, true), dir.path(), true).expect("a dry run succeeds");
        assert!(
            !dir.path().join("target").exists(),
            "a dry run built the project"
        );
    }

    #[test]
    fn a_directory_that_is_not_a_renvor_project_is_refused_before_anything_runs() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = run(&Reporter::new(Format::Human, true), dir.path(), false).unwrap_err();
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(!dir.path().join("target").exists(), "cargo ran anyway");
    }
}
