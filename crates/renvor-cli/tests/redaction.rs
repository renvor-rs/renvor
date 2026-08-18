//! Redaction, from outside the process (FR-041).
//!
//! # Why this is its own file
//!
//! `renvor-core`'s diagnostics gate flags any file that handles credential-shaped values and
//! interpolates a rendering into an assertion message — because on a redaction regression that
//! prints the secret into the test log, on exactly the run where it matters most.
//!
//! Keeping these tests here means **every** assertion in this file can use a fixed message without
//! making the rest of the acceptance suite give up its diagnostics. That is a real trade: the
//! failures here are harder to debug, deliberately.

use std::path::Path;
use std::process::Command;

/// Runs `renvor` and returns (exit code, stdout, stderr).
fn renvor(args: &[&str], directory: &Path) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(args)
        .current_dir(directory)
        .output()
        .expect("renvor runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The planted value. Never interpolated into an assertion message.
const PLANTED: &str = "hunter2";

#[test]
fn no_output_mode_leaks_a_credential_shaped_value() {
    // Planted where it will reach a diagnostic: as the destination, which the invalid-name and
    // rejected-destination messages echo back.
    let base = tempfile::tempdir().expect("tempdir");

    for mode in ["human", "json"] {
        let (code, stdout, stderr) = renvor(
            &[
                "new",
                "--path",
                "password=hunter2",
                "--yes",
                "--output",
                mode,
            ],
            base.path(),
        );
        assert_ne!(
            code, 0,
            "a credential-shaped destination should not succeed"
        );
        // FIXED MESSAGES. Interpolating stdout or stderr here would print the credential into the
        // log on precisely the run where redaction regressed.
        assert!(
            !stdout.contains(PLANTED),
            "stdout leaked a credential in one output mode"
        );
        assert!(
            !stderr.contains(PLANTED),
            "stderr leaked a credential in one output mode"
        );
    }
}

#[test]
fn the_redaction_marker_is_present_when_something_was_redacted() {
    // POSITIVE CONTROL. Without it, a program that emitted no output at all would satisfy the test
    // above — "no leak" and "nothing happened" would be indistinguishable.
    let base = tempfile::tempdir().expect("tempdir");
    let (_, _, stderr) = renvor(&["new", "--path", "token=abc123", "--yes"], base.path());
    assert!(
        stderr.contains("[redacted]"),
        "no redaction marker appeared, so the absence of a leak may mean there was no output"
    );
}

#[test]
fn ordinary_output_is_not_mangled_by_redaction() {
    // The other control. A redactor that damaged normal output would be switched off by the first
    // person it inconvenienced.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, stdout, stderr) =
        renvor(&["new", "clean", "--yes", "--output", "json"], base.path());
    assert_eq!(code, 0, "an ordinary run failed");
    assert!(
        !stdout.contains("[redacted]"),
        "ordinary stdout was redacted"
    );
    assert!(
        !stderr.contains("[redacted]"),
        "ordinary stderr was redacted"
    );
    let document: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("one JSON document");
    assert_eq!(document["status"], "success");
}
