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
fn no_raw_control_character_from_a_path_ever_reaches_a_human_stream() {
    // ── ADVISORY SECURITY FINDING G/S-1, 2026-08-18 ───────────────────────────────────
    //
    // `Destination::open` RULE 1b refuses a control character in the directory renvor is about to
    // **create**. It says nothing about the components *above* it — and a directory that already
    // exists two levels up is opened by RULE 3 and its name interpolated into the success message
    // and into the error messages. The reviewer measured raw `ESC` bytes reaching the terminal with
    // `od -c`.
    //
    // ── NEWLINE AND TAB ARE COVERED HERE, NOT JUST ESCAPE ─────────────────────────────
    //
    // The first fix exempted `\n` and `\t` so that cargo's multi-line diagnostics stayed readable,
    // and that left a real hole: a newline in an ancestor directory ends the line the operator is
    // reading and starts one an attacker wrote, which is as good a forgery as any escape sequence.
    // Path-derived values are now escaped **completely** at the point they are interpolated, and
    // the multi-line output that motivated the exemption is preserved because it is a different
    // value — the message's own text, not the path.
    //
    // Windows-excluded because the fixture needs directory names containing `ESC`, a newline, and a
    // tab, none of which is a legal filename character there. The escaper itself is
    // platform-independent and unit tested on every platform in `output::redact::tests`.
    const HOSTILE: [(&str, &str); 3] = [
        ("\u{1b}[31mRED\u{1b}[0m", "an ANSI escape sequence"),
        ("above\nFORGED-LINE", "a newline"),
        ("col\tumn", "a tab"),
    ];

    for (index, (component, _description)) in HOSTILE.into_iter().enumerate() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let ancestor = base.path().join(component);
        std::fs::create_dir(&ancestor).expect("the hostile ancestor is created");

        // ── SUCCESS PATH ──────────────────────────────────────────────────────────────
        let (exit, stdout, stderr) = renvor(
            &[
                "new",
                "demo",
                "--path",
                ancestor.join("demo").to_str().expect("utf-8"),
                "--yes",
            ],
            base.path(),
        );
        assert_eq!(
            exit, 0,
            "case {index}: the run into a hostile ancestor should still succeed"
        );
        assert!(
            !stdout.chars().any(is_forbidden) && !stderr.chars().any(is_forbidden),
            "case {index}: a raw control character from the path reached a human stream on the \
             success path"
        );

        // ── ERROR PATH: the destination now exists, so the run is refused ──────────────
        let (exit, stdout, stderr) = renvor(
            &[
                "new",
                "demo",
                "--path",
                ancestor.join("demo").to_str().expect("utf-8"),
                "--yes",
            ],
            base.path(),
        );
        assert_eq!(exit, 3, "case {index}: the second run must be refused");
        assert!(
            !stdout.chars().any(is_forbidden) && !stderr.chars().any(is_forbidden),
            "case {index}: a raw control character from the path reached a human stream on the \
             error path"
        );

        // ── ERROR PATH: a missing parent, which builds its message differently ─────────
        let (_, stdout, stderr) = renvor(
            &[
                "new",
                "demo",
                "--path",
                ancestor
                    .join("absent")
                    .join("demo")
                    .to_str()
                    .expect("utf-8"),
                "--yes",
            ],
            base.path(),
        );
        assert!(
            !stdout.chars().any(is_forbidden) && !stderr.chars().any(is_forbidden),
            "case {index}: a raw control character reached a human stream on the missing-parent \
             path"
        );
    }
}

/// A control character that must never reach a human stream from a path.
///
/// `\n` is excluded because renvor's **own** messages end lines with it — the dry run lists files,
/// and the success message has a blank line before `next:`. What this asserts is that no control
/// character arrives *from the path*, which the fixtures above make unambiguous: their newline is
/// inside a directory name, so if it survived it would appear as an extra line break in the middle
/// of a path. That is caught by the `\n`-escaped form being present, asserted separately below.
fn is_forbidden(character: char) -> bool {
    character.is_control() && character != '\n'
}

#[test]
#[cfg(unix)]
fn a_control_character_from_a_path_is_shown_escaped_rather_than_removed() {
    // The other half of the requirement, and the reason stripping was rejected: an operator who
    // cannot see the sequence cannot tell the path is not what it looks like. This also covers the
    // newline case that `is_forbidden` above deliberately cannot — a newline inside a directory
    // name must appear as the two characters `\n`, not as a line break.
    let base = tempfile::tempdir().expect("a temporary directory");
    let ancestor = base.path().join("above\nFORGED-LINE");
    std::fs::create_dir(&ancestor).expect("the hostile ancestor is created");

    let (exit, stdout, stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            ancestor.join("demo").to_str().expect("utf-8"),
            "--yes",
        ],
        base.path(),
    );
    assert_eq!(exit, 0);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("above\\nFORGED-LINE"),
        "the newline must be SHOWN as an escape, or the operator sees a forged line break"
    );
    assert!(
        !combined.contains(
            "above
FORGED-LINE"
        ),
        "a raw newline from the path survived into human output"
    );
}

#[test]
#[cfg(unix)]
fn json_carries_the_real_bytes_and_is_not_double_escaped() {
    // `serde_json` already escapes these bytes correctly, so the strict escaper must NOT run on the
    // machine-readable side. If it did, `details.destination` would carry the literal text
    // `\u{1b}` — six characters a consumer cannot turn back into a path — instead of the byte.
    let base = tempfile::tempdir().expect("a temporary directory");
    let ancestor = base.path().join("esc\u{1b}and\ttab");
    std::fs::create_dir(&ancestor).expect("the hostile ancestor is created");

    let (_, stdout, _) = renvor(
        &[
            "new",
            "demo",
            "--path",
            ancestor.join("demo").to_str().expect("utf-8"),
            "--yes",
            "--output",
            "json",
        ],
        base.path(),
    );
    let document: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("the JSON document parses");
    let destination = document["result"]["destination"]
        .as_str()
        .expect("a destination string");

    assert!(
        destination.contains('\u{1b}') && destination.contains('\t'),
        "JSON must carry the real bytes, escaped by serde and decoded back by the consumer"
    );
    assert!(
        !destination.contains("\\u{1b}"),
        "JSON was double-escaped: the consumer would receive literal text, not a usable path"
    );
    // POSITIVE CONTROL: the value must be a path that actually resolves, which is the whole point
    // of keeping it raw.
    assert!(
        std::path::Path::new(destination).is_dir(),
        "the JSON destination does not resolve to the directory that was created"
    );
}

#[test]
fn a_credential_in_a_rejected_argument_is_redacted_in_every_mode() {
    // `no_output_mode_leaks_a_credential_shaped_value` plants its secret in argv that clap
    // **accepts**, so it exercises the Reporter and passes. The path it cannot reach is argv clap
    // **rejects**: that returns from clap before any Reporter exists, and human mode printed the
    // credential verbatim while JSON mode redacted the identical string. FR-041 says every mode.
    let base = tempfile::tempdir().expect("tempdir");

    // `echoed` says whether clap puts the *value* into its message. For a `--flag=value` shape it
    // reports only the flag name, so there is nothing to redact and no marker to look for; for a
    // bare `key=value` it echoes the whole token. Asserting the marker unconditionally would make
    // the second case fail for the right reason and the first for the wrong one.
    // Indexed rather than named: `crates/renvor-core/tests/diagnostics.rs` forbids interpolating a
    // rendering into an assertion message in a credential-handling file, because on a redaction
    // regression that message is where the credential lands. `index` and `position` identify
    // which arm failed without printing anything derived from the output. Both names come from
    // that gate's own allowlist; widening the allowlist to fit a message would defeat it.
    for (index, (hostile, echoed)) in [("password=hunter2", true), ("--token=hunter2", false)]
        .into_iter()
        .enumerate()
    {
        for (position, mode) in ["human", "json"].into_iter().enumerate() {
            let (_code, stdout, stderr) = renvor(&[hostile, "--output", mode], base.path());
            let combined = format!("{stdout}{stderr}");
            assert!(
                !combined.contains(PLANTED),
                "the credential survived on the rejected-argument path (index {index}, position {position})"
            );
            if echoed {
                assert!(
                    combined.contains("[redacted]"),
                    "nothing was redacted (index {index}, position {position}), so the assertion above proved nothing"
                );
            } else {
                // POSITIVE CONTROL for the non-echoing shape: the run must still have produced a
                // diagnostic, or "the secret is absent" would be satisfied by empty output.
                assert!(
                    combined.contains("error:"),
                    "no diagnostic at all (index {index}, position {position}), so the assertion above proved nothing"
                );
            }
        }
    }
}
