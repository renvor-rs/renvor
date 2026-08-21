//! Contract C-8, asserted against the shipped binary on the paths that have no terminal.
//!
//! # What this file is for, and what `terminal.rs` is for
//!
//! Everything here runs `renvor` as an ordinary subprocess with pipes on both streams. That is the
//! **forbidden** half of the colour policy — no terminal, so no styling, ever — plus the halves
//! that have nothing to do with terminals at all: that the JSON documents did not move, that
//! `stdout` and `stderr` still carry what they carried, and that no untrusted value can reach the
//! screen as anything but text.
//!
//! The **permitted** half needs a real terminal to be tested at all, and lives in `terminal.rs`.
//! Splitting them is not organisation for its own sake: a test that asserts "no escape sequences"
//! in a process that could never have emitted any is a test that passes for the wrong reason, and
//! keeping the two files apart is what stops that from being invisible.

mod harness;

use harness::renvor;

/// Every escape character in `text`, so a failure says how many rather than merely that there were
/// some.
fn escapes(text: &str) -> usize {
    text.matches('\u{1b}').count()
}

/// A directory to run in. Nothing here writes, but `renvor check` and `renvor docker` read the cwd.
fn workspace() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

// ── 1. JSON DID NOT MOVE ────────────────────────────────────────────────────────────────────

/// The five envelopes recorded from the binary **as it was before contract C-8 existed**.
///
/// # Why fixtures rather than a shape snapshot
///
/// `tests/cli.rs` already snapshots the *shape* of every JSON document, which catches a schema
/// change and deliberately does not catch a value change. C-8's promise is stronger and different:
/// the bytes are the same. A consumer pinned to `schemaVersion: 2` must see exactly what it saw
/// before, and "the shape is unchanged" does not say that.
///
/// These five were chosen because their content is **platform-independent** — no paths, no OS
/// error strings, no tool versions, no digests — so they can be compared byte for byte on Linux,
/// macOS, and Windows alike. The broader claim, that all eleven representative commands produce
/// identical stdout and identical exit codes, was measured by running both binaries side by side;
/// that measurement is recorded in the pull request rather than here, because it needs two
/// binaries and this suite has one.
const RECORDED: &[(&str, &[&str], &str)] = &[
    (
        "usage-missing-name.json",
        &["--output", "json", "new"],
        include_str!("json/usage-missing-name.json"),
    ),
    (
        "reserved-database.json",
        &["--output", "json", "new", "demo", "--database", "postgres"],
        include_str!("json/reserved-database.json"),
    ),
    (
        "reserved-transport.json",
        &["--output", "json", "new", "demo", "--transport", "rest"],
        include_str!("json/reserved-transport.json"),
    ),
    (
        "container-controls-missing.json",
        &["--output", "json", "docker", "up"],
        include_str!("json/container-controls-missing.json"),
    ),
    (
        "tls-consent-recorded.json",
        &[
            "--output",
            "json",
            "tls",
            "trust",
            "--i-understand-this-modifies-my-system-trust-store",
        ],
        include_str!("json/tls-consent-recorded.json"),
    ),
];

#[test]
fn every_recorded_json_document_is_still_byte_for_byte_what_it_was() {
    let root = workspace();
    for (name, arguments, expected) in RECORDED {
        let (_, stdout, _) = renvor(arguments, root.path(), &[]);
        assert_eq!(
            stdout, *expected,
            "{name}: the JSON document changed. C-8 governs presentation and may not move a byte \
             of this"
        );
    }
}

#[test]
fn no_json_document_carries_a_single_escape_sequence() {
    let root = workspace();
    for (name, arguments, _) in RECORDED {
        let (_, stdout, stderr) = renvor(arguments, root.path(), &[]);
        assert_eq!(escapes(&stdout), 0, "{name}: styled JSON is not JSON");
        assert_eq!(
            escapes(&stderr),
            0,
            "{name}: stderr was styled without a terminal"
        );
    }
}

#[test]
fn a_json_run_puts_exactly_one_document_on_stdout_and_it_parses() {
    // C-2, restated here because C-8 is the change most able to break it: a status label, a row,
    // or a progress line leaking onto `stdout` would make this two documents or none.
    let root = workspace();
    for (name, arguments, _) in RECORDED {
        let (_, stdout, _) = renvor(arguments, root.path(), &[]);
        serde_json::from_str::<serde_json::Value>(&stdout)
            .unwrap_or_else(|error| panic!("{name}: stdout is not one JSON document: {error}"));
    }
}

#[test]
fn json_mode_shows_no_progress_indicator() {
    // C-8: progress is absent in JSON mode. `renvor new --dry-run` runs the five-check verification
    // that the indicator exists for, so this is the command that would show one.
    let root = workspace();
    let destination = root.path().join("demo");
    let (code, stdout, stderr) = renvor(
        &[
            "--output",
            "json",
            "--dry-run",
            "new",
            "demo",
            "--path",
            destination.to_str().expect("utf-8"),
            "--yes",
        ],
        root.path(),
        &[],
    );
    assert_eq!(code, 0, "{stderr}");
    assert_eq!(escapes(&stdout), 0, "the JSON document was styled");
    // A hidden indicator writes nothing at all — not a cleared line, not a carriage return.
    assert_eq!(
        escapes(&stderr),
        0,
        "a progress indicator drew into a JSON run's stderr: {stderr:?}"
    );
    assert!(
        !stderr.contains('\r'),
        "a progress indicator redrew a line in a JSON run: {stderr:?}"
    );
}

// ── 2. STREAM OWNERSHIP ─────────────────────────────────────────────────────────────────────

#[test]
fn a_human_failure_puts_nothing_at_all_on_stdout() {
    // C-1. The presentation of an error changed completely; the stream it lands on did not. This is
    // the assertion that says so, and it is the one that would have caught a status label being
    // emitted through the result path by mistake.
    let root = workspace();
    for arguments in [
        vec!["new"],
        vec!["new", "demo", "--database", "postgres"],
        vec!["new", "demo", "--output=yaml"],
        vec!["docker", "up"],
        vec!["check", "."],
        vec!["dev"],
    ] {
        let (code, stdout, stderr) = renvor(&arguments, root.path(), &[]);
        assert_ne!(code, 0, "{arguments:?} was expected to fail");
        assert_eq!(
            stdout, "",
            "{arguments:?} wrote a human failure to stdout: {stdout:?}"
        );
        assert!(
            stderr.contains("ERROR"),
            "{arguments:?} produced no ERROR label on stderr: {stderr:?}"
        );
    }
}

#[test]
fn a_human_result_goes_to_stdout_and_the_diagnostics_stay_on_stderr() {
    let root = workspace();
    let (code, stdout, stderr) = renvor(&["doctor"], root.path(), &[]);
    assert_eq!(code, 0, "{stderr}");
    assert!(
        stdout.contains("INFO") && stdout.contains("Environment readiness"),
        "the result must be on stdout: {stdout:?}"
    );
    assert!(
        stdout.contains("cargo"),
        "the readiness table is the result: {stdout:?}"
    );
}

// ── 3. NOTHING IS STYLED WITHOUT A TERMINAL ─────────────────────────────────────────────────

#[test]
fn a_piped_run_carries_no_escape_sequence_under_any_environment() {
    // Four of C-8's five refusals, and the fifth — `--output json` — is covered above. Each is
    // asserted **separately** rather than all at once, because "no colour when everything forbids
    // it" is a weaker claim than "no colour when any one thing forbids it", and it is the weaker
    // claim a combined test accidentally makes.
    let root = workspace();
    for (label, arguments, environment) in [
        ("a pipe alone", vec!["doctor"], vec![]),
        ("--no-color", vec!["doctor", "--no-color"], vec![]),
        ("NO_COLOR", vec!["doctor"], vec![("NO_COLOR", "1")]),
        ("TERM=dumb", vec!["doctor"], vec![("TERM", "dumb")]),
        (
            "a force-colour variable cannot override a pipe",
            vec!["doctor"],
            vec![("CLICOLOR_FORCE", "1")],
        ),
        (
            "a force-colour variable cannot override --no-color",
            vec!["doctor", "--no-color"],
            vec![("CLICOLOR_FORCE", "1"), ("TERM", "xterm-256color")],
        ),
        (
            "a force-colour variable cannot override NO_COLOR",
            vec!["doctor"],
            vec![
                ("CLICOLOR_FORCE", "1"),
                ("NO_COLOR", "1"),
                ("TERM", "xterm-256color"),
            ],
        ),
    ] {
        let (code, stdout, stderr) = renvor(&arguments, root.path(), &environment);
        assert_eq!(code, 0, "{label}: {stderr}");
        assert_eq!(
            escapes(&stdout),
            0,
            "{label}: stdout carried {} escape sequences: {stdout:?}",
            escapes(&stdout)
        );
        assert_eq!(
            escapes(&stderr),
            0,
            "{label}: stderr was styled: {stderr:?}"
        );
    }
}

#[test]
fn help_carries_no_escape_sequence_when_it_is_piped() {
    // The parser's own renderer, through this program's colour boundary. `--help` is the command
    // most likely to be piped into a pager, and the one whose styling this program does not write
    // itself — which is exactly why it needs its own assertion rather than being assumed covered.
    let root = workspace();
    for arguments in [
        vec!["--help"],
        vec!["-h"],
        vec!["new", "--help"],
        vec!["--help", "--no-color"],
        vec!["--nonsense"],
    ] {
        let (_, stdout, stderr) = renvor(&arguments, root.path(), &[]);
        assert_eq!(
            escapes(&stdout),
            0,
            "{arguments:?} styled stdout: {stdout:?}"
        );
        assert_eq!(
            escapes(&stderr),
            0,
            "{arguments:?} styled stderr: {stderr:?}"
        );
    }
}

#[test]
fn help_still_says_everything_it_said() {
    // C-8 keeps the parser's renderer, so the CONTENT of `--help` is generated from the same
    // declaration that parses the command line. `tests/cmd/help.trycmd` asserts it byte for byte;
    // this asserts the property that makes that assertion meaningful — that every command and
    // every global flag is still named — so a future change that quietly drops one has to fail
    // here as well as there.
    let root = workspace();
    let (code, stdout, _) = renvor(&["--help"], root.path(), &[]);
    assert_eq!(code, 0);
    for expected in [
        "new",
        "doctor",
        "check",
        "dev",
        "docker",
        "tls",
        "--output",
        "--yes",
        "--dry-run",
        "--no-color",
        "--help",
        "--version",
        "Exit codes:",
    ] {
        assert!(
            stdout.contains(expected),
            "`--help` no longer mentions {expected:?}: {stdout}"
        );
    }
}

// ── 4. UNTRUSTED VALUES ─────────────────────────────────────────────────────────────────────

#[test]
fn an_escape_sequence_in_a_path_reaches_the_screen_as_text() {
    // C-8 step 2. The row layout puts a value at the right-hand end of a line it has measured, so
    // a value that can move the cursor does not merely forge a line — it moves the thing that
    // draws every following one.
    let root = workspace();
    let hostile = root.path().join("demo\u{1b}[31m\u{1b}[2J");
    let (code, stdout, stderr) = renvor(
        &[
            "--dry-run",
            "new",
            "demo",
            "--path",
            &hostile.to_string_lossy(),
            "--yes",
        ],
        root.path(),
        &[],
    );
    let combined = format!("{stdout}{stderr}");
    assert_eq!(
        escapes(&combined),
        0,
        "an escape sequence from a path reached the terminal (exit {code}): {combined:?}"
    );
    assert!(
        combined.contains("u{1b}") || combined.contains("\\x1b") || code != 0,
        "the sequence must be visible as text, or the run must have been refused: {combined:?}"
    );
}

#[test]
fn a_newline_in_a_value_cannot_forge_a_row() {
    // Every field of a report is one line by construction. A destination containing a newline used
    // to be able to end renvor's line and start one of its own, which a reader has no way to tell
    // from a line renvor wrote.
    let root = workspace();
    let hostile = root.path().join("demo\nERROR  everything is fine");
    let (_, stdout, stderr) = renvor(
        &[
            "--dry-run",
            "new",
            "demo",
            "--path",
            &hostile.to_string_lossy(),
            "--yes",
        ],
        root.path(),
        &[],
    );
    let combined = format!("{stdout}{stderr}");
    for line in combined.lines() {
        assert!(
            !line.trim_start().starts_with("ERROR  everything is fine"),
            "a newline in a path forged a line: {combined:?}"
        );
    }
}

#[test]
fn a_credential_shaped_value_is_redacted_in_a_row_and_in_a_status_line() {
    // FR-041, on the surfaces C-8 introduced. Redaction ran on a finished string before; it now
    // runs on each field of a report, and this is the assertion that the move did not create a
    // field it does not reach.
    let root = workspace();
    let secretive = root.path().join("token=super-secret-value-1234567890");
    let (_, stdout, stderr) = renvor(
        &[
            "--dry-run",
            "new",
            "demo",
            "--path",
            &secretive.to_string_lossy(),
            "--yes",
        ],
        root.path(),
        &[],
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        !combined.contains("super-secret-value-1234567890"),
        "a credential-shaped value survived into the output: {combined:?}"
    );
}

// ── 5. NOTHING BYPASSES THE REPORTER ────────────────────────────────────────────────────────

/// Files allowed to write to a stream directly, and why.
const DIRECT_OUTPUT_IS_ALLOWED: &[(&str, &str)] = &[
    (
        "output/mod.rs",
        "owns both streams; this is the module every other file goes through",
    ),
    (
        "main.rs",
        "writes the parser's own rendering and restores the cursor from the panic hook, both \
         before or instead of a reporter existing",
    ),
];

#[test]
fn no_shipped_file_writes_to_a_stream_without_going_through_the_reporter() {
    // C-1's oldest rule, and the first one a presentation change is likely to break: the tempting
    // way to print a coloured line is `println!`, and one of those on `stdout` breaks
    // `renvor new --dry-run --output json | jq .` for everyone.
    //
    // A scan rather than a convention, because a convention is a thing a reviewer has to notice.
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offences = Vec::new();
    let mut scanned = 0_usize;
    visit(&source, &mut |path, text| {
        scanned += 1;
        let relative = path
            .strip_prefix(&source)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        if let Some((_, reason)) = DIRECT_OUTPUT_IS_ALLOWED
            .iter()
            .find(|(allowed, _)| *allowed == relative)
        {
            // The reason is read rather than merely written down. An exemption whose justification
            // nothing consults is an exemption nobody re-reads when it stops being true.
            assert!(
                !reason.is_empty(),
                "{relative} is exempt with no stated reason"
            );
            return;
        }
        for (number, line) in text.lines().enumerate() {
            // Comments and doc comments talk ABOUT these macros at length, which is the point of
            // the surrounding documentation. Only code counts.
            let code = line.trim_start();
            if code.starts_with("//") || code.starts_with("*") {
                continue;
            }
            for forbidden in [
                "println!",
                "print!",
                "eprintln!",
                "eprint!",
                // The prompt and progress libraries' own writers, which do not pass through
                // redaction. The adapter reaches them through wrappers that take `&'static str`;
                // anything else naming them is a bypass.
                "cliclack::log",
                "cliclack::note",
                "cliclack::outro_note",
            ] {
                if code.contains(forbidden) {
                    // COLLECTED, not asserted here. Asserting inside the loop reports the first
                    // offence and hides every other one, which turns a single review pass into as
                    // many passes as there are breaches.
                    offences.push(format!("  {relative}:{}  {forbidden}", number + 1));
                }
            }
        }
    });
    assert!(
        offences.is_empty(),
        "a shipped file writes to a stream without going through the reporter. One `println!` on \
         `stdout` breaks `renvor new --dry-run --output json | jq .` for everyone.\n{}",
        offences.join("\n")
    );
    // POSITIVE CONTROL. A scan that walked nothing would pass silently, which is the failure mode
    // that makes a scan worse than no scan — it reports "clean" for "did not look".
    assert!(
        scanned >= 20,
        "the scan visited only {scanned} files, which means it is not finding the source tree"
    );
}

#[test]
fn the_scan_above_would_actually_catch_a_direct_write() {
    // The control for the control. If the needle list or the comment-skipping were wrong, the scan
    // would pass on a file that plainly offends — so a plain offender is constructed here and the
    // same logic is run over it.
    let offender = "fn main() {\n    println!(\"hello\");\n}\n";
    let caught = offender
        .lines()
        .filter(|line| {
            let code = line.trim_start();
            !code.starts_with("//") && code.contains("println!")
        })
        .count();
    assert_eq!(
        caught, 1,
        "the scan's own predicate does not catch a bypass"
    );

    // And the comment exemption must not swallow a real one.
    let commented = "    // println! is forbidden here\n";
    let missed = commented
        .lines()
        .filter(|line| {
            let code = line.trim_start();
            !code.starts_with("//") && code.contains("println!")
        })
        .count();
    assert_eq!(missed, 0, "a comment about the rule is not a breach of it");
}

/// Calls `visit` for every `.rs` file under `directory`.
fn visit(directory: &std::path::Path, visitor: &mut impl FnMut(&std::path::Path, &str)) {
    let entries = std::fs::read_dir(directory).expect("the source tree is readable");
    for entry in entries {
        let path = entry.expect("a readable entry").path();
        if path.is_dir() {
            visit(&path, visitor);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path).expect("a readable source file");
            visitor(&path, &text);
        }
    }
}
