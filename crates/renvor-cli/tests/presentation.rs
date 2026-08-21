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

/// Every command whose JSON failure envelope is produced by the **argument parser** rather than by
/// this crate.
///
/// They are listed separately from [`RECORDED`] because their bytes are platform-dependent — the
/// parser writes `renvor.exe` into its usage line on Windows — so they are asserted as a property
/// rather than as fixtures.
const PARSER_ERRORS: &[&[&str]] = &[
    &["--output", "json", "--nonsense"],
    &["--output", "json", "frobnicate"],
    &["--output", "json"],
    &["--output", "json", "new", "a", "b", "c"],
    &["--output", "json", "new", "demo", "--nonsense"],
];

/// Every string anywhere in a JSON document, **after decoding**.
fn decoded_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| decoded_strings(item, out)),
        serde_json::Value::Object(fields) => {
            for (key, field) in fields {
                out.push(key.clone());
                decoded_strings(field, out);
            }
        }
        _ => {}
    }
}

#[test]
fn no_json_string_carries_an_escape_sequence_once_it_is_decoded() {
    // THE ASSERTION THAT `escapes()` CANNOT MAKE, AND THE DEFECT IT DID NOT CATCH.
    //
    // `escapes()` counts literal `ESC` characters in the raw output. A JSON document never
    // contains one: `serde_json` writes `\u001b`, six ASCII characters, **before** anything sees
    // the bytes. So every "no escape sequences" assertion in this file returns 0 for a document
    // whose decoded content is full of them, and the strip at the writer has nothing to remove
    // either.
    //
    // That is exactly what happened. Styling `--help` meant the parser's usage errors carried its
    // colours, and in JSON mode that rendering becomes `error.message` — so
    // `renvor --output json --nonsense | jq -r .error.message` printed live escape sequences on a
    // pipe. An advisory review found it; both mechanisms this crate relies on were blind to it by
    // construction.
    //
    // This decodes the document and inspects the values, which is what a consumer does.
    let root = workspace();
    let mut inspected = 0_usize;
    for arguments in RECORDED
        .iter()
        .map(|(_, arguments, _)| *arguments)
        .chain(PARSER_ERRORS.iter().copied())
    {
        let (_, stdout, _) = renvor(arguments, root.path(), &[]);
        let document: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|error| panic!("{arguments:?}: not one JSON document: {error}"));
        let mut strings = Vec::new();
        decoded_strings(&document, &mut strings);
        for text in &strings {
            assert!(
                !text.contains('\u{1b}'),
                "{arguments:?}: a decoded JSON string carries an escape sequence: {text:?}"
            );
        }
        inspected += strings.len();
    }
    // POSITIVE CONTROL. A walk that found no strings would pass while inspecting nothing.
    assert!(
        inspected > 40,
        "only {inspected} strings were inspected; the walk is not reaching the documents"
    );
}

#[test]
fn a_parser_error_still_produces_a_usage_envelope_in_json_mode() {
    // The other half of the same fix: taking the UNSTYLED rendering must not have emptied the
    // message. C-2 requires a parseable answer precisely when the command line could not be
    // parsed, and the useful part of that answer is the parser's own text.
    let root = workspace();
    for arguments in PARSER_ERRORS {
        let (code, stdout, _) = renvor(arguments, root.path(), &[]);
        assert_eq!(code, 2, "{arguments:?} must be a usage error");
        let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
        assert_eq!(document["error"]["code"], "usage", "{arguments:?}");
        let message = document["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{arguments:?}: no message"));
        assert!(
            message.contains("Usage:") || message.contains("error:"),
            "{arguments:?}: the parser's text was lost: {message:?}"
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
    (
        "generate/place.rs",
        "one `writeln!` to stderr from a `Drop`, where no reporter is in scope and no error can \
         be returned. Found by widening this scan; checked rather than exempted on sight — the \
         value goes through `redact::path`, which escapes every control character, and stderr is \
         the diagnostic stream in both output modes, so it cannot disturb the JSON document. \
         Silence was the previous behaviour and is the one thing that must not happen: a staged \
         tree survives and nothing says so",
    ),
];

/// Every way a shipped file could reach a stream without the reporter.
///
/// # The macros are not the whole list, and the first version of this scan thought they were
///
/// `println!` is the obvious bypass. `writeln!(std::io::stdout(), …)` is the same bypass spelled
/// differently — and it is the spelling the panic hook legitimately uses, so it is the shape a
/// future one would most plausibly take. A scan that looked only for the macros reported "clean"
/// for a class of write it never searched for, which an advisory review pointed out.
const BYPASSES: &[&str] = &[
    // LONGEST FIRST. `eprintln!` contains `println!`, so a shortest-first list reports the wrong
    // needle — which the control below caught the moment it started running the real predicate.
    "eprintln!",
    "eprint!",
    "println!",
    "print!",
    // The same write, spelled with the writer named explicitly.
    "std::io::stdout()",
    "std::io::stderr()",
    // The prompt and progress libraries' own writers, which do not pass through redaction. The
    // adapter reaches them through wrappers that take `&'static str`; anything else naming them
    // is a bypass.
    "cliclack::log",
    "cliclack::note",
    "cliclack::outro_note",
];

/// Whether one line of source is a bypass, and which one.
///
/// Extracted so that the scan and **its own control** run the same code. The control used to
/// re-implement a two-line predicate over a synthetic string, which meant it would have passed
/// unchanged if the real needle list had been emptied — a control that does not exercise the
/// thing it controls.
fn bypass_in(line: &str) -> Option<&'static str> {
    let code = line.trim_start();
    // Comments and doc comments talk ABOUT these at length, which is the point of the surrounding
    // documentation. Only code counts.
    if code.starts_with("//") || code.starts_with('*') {
        return None;
    }
    BYPASSES
        .iter()
        .find(|forbidden| code.contains(*forbidden))
        .copied()
}

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
            if let Some(found) = bypass_in(line) {
                // COLLECTED, not asserted here. Asserting inside the loop reports the first
                // offence and hides every other one, which turns a single review pass into as
                // many passes as there are breaches.
                offences.push(format!("  {relative}:{}  {found}", number + 1));
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
fn the_scan_above_runs_over_a_planted_bypass_and_catches_it() {
    // THE CONTROL FOR THE CONTROL, and it calls the real predicate rather than a copy of it.
    //
    // Every needle is exercised, so emptying `BYPASSES` fails here — which the previous version
    // of this test would not have noticed, because it re-implemented the check inline.
    for needle in BYPASSES {
        let planted = format!("    {needle}(\"boom\");");
        assert_eq!(
            bypass_in(&planted),
            Some(*needle),
            "the scan does not catch {needle}"
        );
    }

    // And the comment exemption must not swallow a real breach.
    assert_eq!(
        bypass_in("    // println! is forbidden here"),
        None,
        "a comment about the rule is not a breach of it"
    );
    assert_eq!(
        bypass_in("    /// `eprintln!` is discussed in this doc comment"),
        None,
        "a doc comment about the rule is not a breach of it"
    );
    // Ordinary code is not a false positive.
    assert_eq!(bypass_in("    let total = a + b;"), None);
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
