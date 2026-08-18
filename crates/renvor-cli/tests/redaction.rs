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

#[test]
fn a_successful_run_redacts_in_json_as_well_as_human() {
    // ── THIS TEST EXISTS BECAUSE THE JSON SUCCESS PATH LEAKED ──────────────────────────
    //
    // Every other test in this file plants its secret in an input that **fails**, which exercises
    // `Envelope::failure` — and that path always redacted. `Envelope::success` did not. So one
    // input produced two different answers, and the mode a machine reads was the leaking one:
    //
    //   human: created 6 files (879 bytes) in /tmp/token=[redacted]
    //   json : "destination": "/tmp/token=abc123secret/demo"
    //
    // FR-041 says "every output mode", and SC-008 names JSON explicitly. A secret in a JSON field
    // is worse than one in prose, because that is the output a tool writes to a log.
    //
    // The destination is the injection point because it is operator-supplied, it reaches the
    // success result verbatim, and a checkout under a token-bearing directory is an ordinary thing
    // for CI to produce.
    let base = tempfile::tempdir().expect("tempdir");
    let secret_parent = base.path().join(format!("token={PLANTED}"));
    std::fs::create_dir(&secret_parent).expect("the secret-bearing parent is created");

    for (mode, expect_success) in [("json", true), ("human", true)] {
        let destination = secret_parent.join(format!("proj-{mode}"));
        let (code, stdout, stderr) = renvor(
            &[
                "new",
                "demo",
                "--path",
                destination.to_str().expect("utf-8"),
                "--yes",
                "--output",
                mode,
            ],
            base.path(),
        );
        assert_eq!(
            code, 0,
            "the run must SUCCEED, or it exercises the failure path instead"
        );
        assert!(expect_success);

        // Fixed messages, never interpolating the rendering — see the module header.
        assert!(
            !stdout.contains(PLANTED),
            "the planted value reached stdout on a SUCCESSFUL run"
        );
        assert!(
            !stderr.contains(PLANTED),
            "the planted value reached stderr on a SUCCESSFUL run"
        );
        assert!(
            stdout.contains(crate_redaction_marker()) || stderr.contains(crate_redaction_marker()),
            "nothing was redacted, so this run did not exercise the path it was written for"
        );
    }
}

/// The marker the redactor substitutes. Kept as a function so the literal appears once.
fn crate_redaction_marker() -> &'static str {
    "[redacted]"
}

#[test]
#[cfg(unix)]
fn no_raw_control_byte_from_a_path_ever_reaches_a_human_stream() {
    // ── ADVISORY SECURITY FINDING S-1, 2026-08-18, REPRODUCED AS A TEST ────────────────
    //
    // `Destination::open` RULE 1b refuses a control character in the directory renvor is about to
    // **create**. It says nothing about the components *above* it — and a directory that already
    // exists two levels up is opened by RULE 3 and its name interpolated into the success message
    // and into two error messages.
    //
    // The reviewer measured it with `od -c` and found the raw `ESC` bytes going straight to the
    // terminal, which renders the destination in colour instead of showing it. That is the operator
    // being shown a path that is not the one on disk — the exact thing RULE 1b's own message says
    // it refuses control characters to prevent.
    //
    // Windows-excluded because the fixture needs a directory name containing `ESC`, which is not a
    // legal filename character there. The neutraliser itself is platform-independent and is unit
    // tested on every platform in `output::redact::tests`.
    let base = tempfile::tempdir().expect("a temporary directory");
    let hostile = base.path().join("\u{1b}[31mRED\u{1b}[0m");
    std::fs::create_dir(&hostile).expect("the hostile parent is created");

    // ── The success path ──────────────────────────────────────────────────────────────
    let (exit, stdout, stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            hostile.join("demo").to_str().expect("utf-8"),
            "--yes",
        ],
        base.path(),
    );
    // Fixed messages throughout this test: `renvor-core`'s `diagnostics` suite refuses an
    // interpolated rendering in an assertion message inside a credential-handling file, because on
    // a regression the thing being printed is the leak itself.
    assert_eq!(
        exit, 0,
        "the run into a hostile parent should still succeed"
    );
    assert!(
        !stdout.contains('\u{1b}') && !stderr.contains('\u{1b}'),
        "a raw ESC byte reached a human stream on the success path"
    );
    assert!(
        stdout.contains("\\u{1b}[31m") || stderr.contains("\\u{1b}[31m"),
        "the sequence must be SHOWN rather than stripped, or the operator still cannot tell the \
         path is not what it looks like"
    );

    // ── An error path, which builds its message differently ───────────────────────────
    let (_, _, stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            hostile.join("absent").join("demo").to_str().expect("utf-8"),
            "--yes",
        ],
        base.path(),
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "a raw ESC byte reached stderr on the error path"
    );

    // ── JSON IS NOT DOUBLE-ESCAPED ────────────────────────────────────────────────────
    //
    // `serde_json` already escapes these bytes correctly, so the neutraliser must NOT run there.
    // If it did, the document would carry a literal `\u{1b}` whose backslash serde then escapes
    // again — corrupting the contract to fix a problem this path does not have.
    let (_, stdout, _) = renvor(
        &[
            "new",
            "second",
            "--path",
            hostile.join("second").to_str().expect("utf-8"),
            "--yes",
            "--output",
            "json",
        ],
        base.path(),
    );
    let document: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("the JSON document still parses");
    let destination = document["result"]["destination"]
        .as_str()
        .expect("a destination string");
    assert!(
        destination.contains('\u{1b}'),
        "JSON must carry the real bytes, escaped by serde and decoded back by the consumer"
    );
    assert!(!destination.contains("\\u{1b}"), "JSON was double-escaped");
}
