//! `renvor dev` — the local development loop.
//!
//! # What this does today, stated rather than implied
//!
//! **Phase 004 shipped a transport**, so the reason previously given here — "there is no transport
//! until Phase 004" — is no longer the reason. The current one is narrower: a **generated project**
//! does not depend on the framework, because nothing is published, so it has no server for `dev` to
//! reload.
//!
//! `dev` therefore runs the generated project's own build-and-test loop and reports it. That is
//! genuinely useful and it is **not** what `dev` will eventually mean.
//!
//! It is documented here rather than quietly under-delivered, because a command whose behaviour
//! changes shape between phases is a compatibility question, and pretending Phase 003 shipped a
//! watch loop would make Phase 004 look like a regression.

use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;
use crate::output::layout::{Report, Status};

/// Runs the command in a project directory.
///
/// # Errors
///
/// [`Code::ToolMissing`] (exit `5`) if `cargo` cannot be run, [`Code::ManifestInvalid`] if the
/// directory is not a renvor project, or [`Code::ProjectVerificationFailed`] if the project's own
/// tests fail.
pub fn run(reporter: &Reporter, path: &std::path::Path, dry_run: bool) -> Result<Exit, CliError> {
    if !path.join("renvor.toml").is_file() {
        return Err(CliError::new(
            Code::ManifestInvalid,
            format!(
                "`{}` is not a renvor project; there is no `renvor.toml` here",
                crate::output::redact::path(path)
            ),
        )
        .with("field", "renvor.toml")
        .with("constraint", "must exist in the project directory"));
    }

    // `--dry-run` is global and this command runs a build. See the note in `docker.rs`.
    if dry_run {
        return Ok(reporter.finish(
            "dev",
            &Report::new().status(
                Status::Info,
                "Dry run: would run `cargo test` in the project",
            ),
            serde_json::json!({ "dryRun": true, "wouldRun": ["cargo", "test"] }),
        ));
    }

    reporter.note(
        "running `cargo test` in the project (a generated project declares no renvor dependency \
         yet, so there is no server to reload)",
    );

    let status = std::process::Command::new("cargo")
        .arg("test")
        .current_dir(path)
        // `cargo test` inherits this process's `stdout` unless told otherwise, which put libtest's
        // output ahead of the JSON envelope and made `--output json dev` unparseable on every run,
        // success included. See `Reporter::child_stdout`.
        .stdout(reporter.child_stdout()?)
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
        //
        // `project_verification_failed` since 2026-08-18. This used to report `manifest_invalid`
        // — published as *"`renvor.toml` failed validation"* — with `details.field = "project"`,
        // a field name that exists in no manifest. A consumer matching the registry would have
        // sent somebody to look at a TOML file over a failing unit test (A-R6).
        return Err(CliError::new(
            Code::ProjectVerificationFailed,
            "the project's own tests failed; the output above is from `cargo test`",
        )
        .with("check", "cargo test")
        .with("stage", "renvor dev"));
    }

    Ok(reporter.finish(
        "dev",
        &Report::new().status(Status::Done, "The project builds and its tests pass"),
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
