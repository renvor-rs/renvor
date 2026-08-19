//! Hostile destinations and hostile templates, refused before anything is written.
//!
//! # Refused, and refused for the RIGHT REASON
//!
//! Every case below asserts `details.rule` — the name of the rule that fired — and not merely that
//! the run failed. That is not decoration. The `..` traversal rule was once absent entirely and
//! every traversal test still passed, because a path *ending* in `..` is caught by a different
//! rule: the suite was green, the destination was reachable, and the tests were passing for the
//! wrong reason. A refusal without the reason is a refusal you cannot trust.
//!
//! # Asserted from outside the process
//!
//! `paths.rs` unit-tests each rule directly, which proves the rule exists. These prove the rule is
//! **reached** — that the wiring from argv through validation to the capability actually runs it.
//! `renvor new --path ../escape` was accepted with exit 0 while every unit test passed.
//!
//! # Ordering, stated rather than implied
//!
//! T020–T023 were specified as a **failing-first** corpus, written before the validator existed.
//! **That ordering did not happen**; this file was written afterwards and no rerun can undo that.
//! What it carries instead is a positive control (T021) and per-rule attribution, so a corpus that
//! refused everything — or refused everything for one lazy reason — cannot pass. See the
//! "Ordering requirements that were missed" section of `tasks.md`.
//!
//! # T023 is not in this file, and that is a stated deviation
//!
//! T023 asks for the no-archive-capability assertion here. It lives in `tests/capabilities.rs`
//! instead, beside the FR-043 no-network-client assertion it shares a lockfile-closure walk and a
//! negative control with. Splitting them would put one claim in two files or duplicate the walk.
//! Recorded as a deviation in `tasks.md` rather than resolved by copying code.

mod harness;

use std::path::Path;

use harness::renvor;

/// Every Windows reserved device name, which `paths.rs` refuses **on every platform**.
///
/// Enumerated rather than described (T051): a class description like "COM followed by a digit"
/// invites an off-by-one at `COM0` and cannot be diffed in review.
const RESERVED_DEVICE_NAMES: [&str; 28] = [
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    // Microsoft's "Naming Files, Paths, and Namespaces" lists the superscript spellings alongside
    // the ASCII ones: `COM¹`, `COM²`, `COM³`, `LPT¹`, `LPT²`, `LPT³` alias the same devices, so
    // `echo test > COM\u{B9}` fails on Windows exactly as `> COM1` does. Written as escapes rather
    // than as literal characters so a reviewer can see which code point each one is, and so they
    // survive a copy through a terminal or an editor that normalises them.
    "COM\u{B9}",
    "COM\u{B2}",
    "COM\u{B3}",
    "LPT\u{B9}",
    "LPT\u{B2}",
    "LPT\u{B3}",
];

/// Runs `renvor new` and returns the parsed failure document, insisting the run failed.
fn refused(arguments: &[&str], working_directory: &Path) -> serde_json::Value {
    let (exit, stdout, stderr) = renvor(arguments, working_directory, &[]);
    assert_ne!(
        exit, 0,
        "{arguments:?} was ACCEPTED; it must be refused\n{stderr}"
    );
    assert_eq!(
        exit, 3,
        "{arguments:?} must fail validation (exit 3), not {exit}\n{stderr}"
    );
    let document: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("{arguments:?} wrote no JSON document: {error}\n{stdout}"));
    assert_eq!(document["status"], "failure", "{arguments:?}");
    assert!(
        document["error"]["code"].is_string(),
        "{arguments:?} produced no stable error code"
    );
    document
}

/// The staging directories left in `directory`, which must always be none.
fn staging_residue(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("the directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".renvor-staging-"))
        .collect()
}

// ── T020: the destination corpus ────────────────────────────────────────────────────────────

#[test]
fn every_traversal_spelling_is_refused_by_the_traversal_rule() {
    // Five spellings, because the rule was once absent and the suite stayed green: the only
    // traversal case it had ENDED in `..`, which `destination_absent_or_empty` catches for an
    // unrelated reason. Asserting `rule == "no_traversal"` is what makes this test about the rule
    // rather than about the outcome.
    let base = tempfile::tempdir().expect("a temporary directory");
    let work = base.path().join("work");
    std::fs::create_dir_all(&work).expect("the working directory is created");

    for case in [
        "../escape",
        "a/../../b",
        "./../escape",
        "x/../../../y",
        "../",
    ] {
        let document = refused(
            &["new", "demo", "--path", case, "--yes", "--output", "json"],
            &work,
        );
        assert_eq!(
            document["error"]["details"]["rule"], "no_traversal",
            "`{case}` was refused by a different rule than the one meant to catch it: {document}"
        );
    }
    assert!(
        staging_residue(&work).is_empty(),
        "refused runs left staging behind"
    );
}

#[test]
fn an_absolute_path_in_the_name_position_is_refused() {
    // Absolute-path injection. The positional argument is the project NAME; a name is a single
    // path component that becomes a package name, so an absolute path there must be refused
    // rather than quietly treated as a destination.
    //
    // ── THIS TEST WAS PASSING FOR THE WRONG REASON UNTIL 2026-08-18 ────────────────────
    //
    // It asserted only that the run failed and that some code was present. It did — but by the
    // character-set rule, as though `/` were an unusual punctuation choice, and with **no
    // `details.rule` at all**, because `validate_project_name` emitted none for any of its five
    // refusals. Data-model §5 rule 2 therefore had no implementation of its own, and this test
    // was the thing standing in for one. Asserting the rule is what makes it a test of the rule.
    let base = tempfile::tempdir().expect("a temporary directory");
    for case in [
        "/etc/renvor-injected",
        "/tmp/renvor-injected",
        "//srv/renvor",
        // Windows drive-relative: neither absolute nor a plain name, and exactly the shape a
        // lexical check misses.
        "C:renvor-injected",
    ] {
        let document = refused(&["new", case, "--yes", "--output", "json"], base.path());
        assert_eq!(
            document["error"]["details"]["rule"], "single_path_component",
            "`{case}` must be refused as a path in the NAME position, not by some other rule: \
             {document}"
        );
        assert!(
            !Path::new(case).exists(),
            "`{case}` was created on the real filesystem by a run that must write nothing"
        );
    }
    assert!(staging_residue(base.path()).is_empty());
}

#[test]
fn every_project_name_refusal_names_a_distinct_rule() {
    // FR-014 wants a refusal to be a diagnosis. Five different problems sharing one code and no
    // `details.rule` is a verdict — the operator learns their name was rejected and not which of
    // five unrelated things is wrong with it.
    let base = tempfile::tempdir().expect("a temporary directory");
    let cases = [
        ("x".repeat(65), "name_length"),
        ("/absolute/path".to_owned(), "single_path_component"),
        ("has space".to_owned(), "name_character_set"),
        ("1leading-digit".to_owned(), "name_starts_with_letter"),
        ("CON".to_owned(), "reserved_device_name"),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (name, expected) in &cases {
        let document = refused(&["new", name, "--yes", "--output", "json"], base.path());
        assert_eq!(
            document["error"]["details"]["rule"], *expected,
            "`{name}` was refused by the wrong rule: {document}"
        );
        seen.insert(*expected);
    }
    assert_eq!(
        seen.len(),
        cases.len(),
        "two cases collapsed onto one rule: {seen:?}"
    );
}

#[test]
fn every_reserved_device_name_is_refused_on_every_platform() {
    // T020 and T051, end to end. `paths.rs` unit-tests the table; this proves the table is
    // REACHED from the command line — and it runs on Linux and macOS too, because a project
    // generated on one platform is opened on another.
    let base = tempfile::tempdir().expect("a temporary directory");
    for reserved in RESERVED_DEVICE_NAMES {
        for spelling in [
            reserved.to_owned(),
            reserved.to_lowercase(),
            format!("{reserved}.txt"),
        ] {
            let document = refused(
                &["new", &spelling, "--yes", "--output", "json"],
                base.path(),
            );
            assert_eq!(
                document["error"]["code"], "invalid_project_name",
                "`{spelling}` must be refused as a name: {document}"
            );
            assert_eq!(
                document["error"]["details"]["rule"], "reserved_device_name",
                "`{spelling}` was refused by the wrong rule: {document}"
            );
            assert!(
                !base.path().join(&spelling).exists(),
                "`{spelling}` was created despite being refused"
            );
        }
    }
    assert!(
        staging_residue(base.path()).is_empty(),
        "refused runs left staging behind"
    );
}

#[test]
#[cfg(unix)]
fn a_destination_that_is_a_symlink_to_another_directory_is_refused() {
    // The escape a purely lexical check cannot see, and the reason `path-clean` was rejected in
    // research D6: the path contains no `..` and is not absolute, and it still leaves the tree.
    let base = tempfile::tempdir().expect("a temporary directory");
    let outside = base.path().join("outside");
    std::fs::create_dir_all(&outside).expect("the outside directory is created");
    std::fs::write(outside.join("precious"), b"do not touch").expect("the witness is written");

    let work = base.path().join("work");
    std::fs::create_dir_all(&work).expect("the working directory is created");
    std::os::unix::fs::symlink(&outside, work.join("linked")).expect("the symlink is created");

    let document = refused(
        &[
            "new", "demo", "--path", "linked", "--yes", "--output", "json",
        ],
        &work,
    );
    // Since the 2026-08-18 destination policy this is refused by the same rule as every other
    // existing entry — `destination_absent` — and `details.found` is what says it was a symlink.
    // There is no longer a separate symlink rule, because there is no longer any existing entry
    // that is acceptable, so a special case for one of them would only be a special case.
    assert_eq!(
        document["error"]["code"], "destination_exists",
        "the symlink must be refused: {document}"
    );
    assert_eq!(
        document["error"]["details"]["rule"], "destination_absent",
        "{document}"
    );
    assert_eq!(
        document["error"]["details"]["found"], "symlink",
        "the refusal must still say it was a symlink, not merely that something was there: \
         {document}"
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("precious")).expect("the witness survives"),
        "do not touch",
        "a refused destination still reached outside the tree"
    );
}

#[test]
fn an_existing_non_empty_destination_is_refused_without_being_touched() {
    // FR-013. renvor does not merge into an existing project, and it must not damage one either.
    let base = tempfile::tempdir().expect("a temporary directory");
    let occupied = base.path().join("occupied");
    std::fs::create_dir_all(&occupied).expect("the occupied destination is created");
    std::fs::write(occupied.join("theirs"), b"theirs").expect("their file is written");

    let document = refused(
        &[
            "new", "demo", "--path", "occupied", "--yes", "--output", "json",
        ],
        base.path(),
    );
    assert_eq!(
        document["error"]["code"], "destination_exists",
        "the code must say WHY it was rejected, not merely that it was: {document}"
    );
    assert_eq!(
        document["error"]["details"]["rule"], "destination_absent",
        "{document}"
    );
    assert_eq!(
        std::fs::read_to_string(occupied.join("theirs")).expect("their file survives"),
        "theirs",
        "an occupied destination was written into"
    );
    assert_eq!(
        std::fs::read_dir(&occupied).expect("readable").count(),
        1,
        "an occupied destination gained entries"
    );
    assert!(
        staging_residue(base.path()).is_empty(),
        "a refused run left staging behind"
    );
}

// ── T021: the positive control ──────────────────────────────────────────────────────────────

#[test]
fn an_ordinary_legitimate_destination_still_generates() {
    // SC-009 requires this explicitly, and it is the only thing standing between the corpus above
    // and a generator that refuses everything — which would satisfy every assertion in this file.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, _, stderr) = renvor(&["new", "fine", "--yes"], base.path(), &[]);
    assert_eq!(exit, 0, "an ordinary destination was refused:\n{stderr}");
    assert!(base.path().join("fine/Cargo.toml").is_file());
    assert!(base.path().join("fine/renvor.toml").is_file());
    assert!(staging_residue(base.path()).is_empty());
}

// ── T022: hostile templates ─────────────────────────────────────────────────────────────────

#[test]
fn no_shipped_template_can_write_outside_the_destination() {
    // T022's observable half. `Renderer::new` refuses an entry whose path escapes the staging root
    // at LOAD time, so such an entry "cannot exist in a shipped binary"; `render.rs` unit-tests that
    // refusal directly, including for `../escape`, `/etc/passwd`, `a/../../b` and an empty path.
    //
    // ── THE FIRST VERSION OF THIS TEST COULD NOT FAIL, AND AN ADVISORY REVIEW FOUND IT ─
    //
    // It walked the DESTINATION looking for paths not starting with the destination — seeded with
    // `walk(&destination, &destination, …)`, recursing only into children of `destination`. Every
    // path it could ever examine satisfied the predicate, so its `outside` vector was provably
    // always empty. It asserted a tautology, in a file whose entire subject is tests that pass for
    // the wrong reason.
    //
    // What it does now is compare the DESTINATION'S PARENT before and after, and require that the
    // only thing that appeared is the destination itself. Anything a template wrote elsewhere in
    // the parent shows up as an unexpected entry, and anything it wrote further afield is caught by
    // the sentinel below.
    let base = tempfile::tempdir().expect("a temporary directory");
    let sentinel = base.path().join("sentinel");
    std::fs::create_dir_all(&sentinel).expect("the sentinel directory is created");
    std::fs::write(sentinel.join("witness"), b"untouched").expect("the witness is written");

    /// Every path under `root`, relative to it, sorted.
    fn snapshot(root: &Path) -> Vec<String> {
        let mut found = Vec::new();
        fn walk(directory: &Path, root: &Path, found: &mut Vec<String>) {
            for entry in std::fs::read_dir(directory)
                .expect("readable")
                .filter_map(Result::ok)
            {
                let path = entry.path();
                found.push(
                    path.strip_prefix(root)
                        .expect("under the root by construction")
                        .display()
                        .to_string(),
                );
                if entry.file_type().expect("a file type").is_dir() {
                    walk(&path, root, found);
                }
            }
        }
        walk(root, root, &mut found);
        found.sort();
        found
    }

    for variant in [
        vec!["new", "plain", "--yes"],
        vec!["new", "domain", "--yes", "--example-domain"],
        vec!["new", "seeded", "--yes", "--example-domain", "--seed-data"],
        vec!["new", "boxed", "--yes", "--container"],
        vec!["new", "secured", "--yes", "--local-https"],
    ] {
        let name = variant[1];
        let before = snapshot(base.path());
        let (exit, _, stderr) = renvor(&variant, base.path(), &[]);
        assert_eq!(exit, 0, "{variant:?} failed: {stderr}");
        let after = snapshot(base.path());

        // Everything that appeared must be the destination or live inside it.
        let appeared: Vec<&String> = after.iter().filter(|path| !before.contains(path)).collect();
        assert!(
            !appeared.is_empty(),
            "{variant:?} created nothing, so this comparison proves nothing"
        );
        let stray: Vec<&&String> = appeared
            .iter()
            .filter(|path| {
                let path = std::path::Path::new(path.as_str());
                path != std::path::Path::new(name) && !path.starts_with(name)
            })
            .collect();
        assert!(
            stray.is_empty(),
            "{variant:?} wrote outside its destination `{name}`: {stray:?}"
        );
    }

    assert_eq!(
        std::fs::read_to_string(sentinel.join("witness")).expect("the witness survives"),
        "untouched",
        "a template wrote outside every destination"
    );
    assert!(staging_residue(base.path()).is_empty());
}

#[test]
fn the_directory_name_taken_from_a_path_is_checked_too() {
    // ── FOUND BY AN ADVISORY SECURITY REVIEW, 2026-08-18 ──────────────────────────────
    //
    // `validate_project_name` guards the **NAME** argument. When the operator supplies a name
    // separately, the directory name comes from `--path`'s final component and reached no such
    // check — so `renvor new x --path $'…/inject\nLINE'` created a directory whose name contained
    // a literal newline, and `--path '…/trailing. '` created one Windows would silently rename.
    // The same strings in the NAME position were correctly refused, which is what made the gap
    // invisible: the rule existed and guarded one of the two ways in.
    let base = tempfile::tempdir().expect("a temporary directory");
    let parent = base.path().join("out");
    std::fs::create_dir(&parent).expect("the parent is created");

    for (case, expected_rule) in [
        (
            format!("{}/inject\nLINE", parent.display()),
            "name_control_character",
        ),
        (
            format!("{}/tab\there", parent.display()),
            "name_control_character",
        ),
        (
            format!("{}/trailing. ", parent.display()),
            "name_trailing_dot_or_space",
        ),
        (
            format!("{}/trailing.", parent.display()),
            "name_trailing_dot_or_space",
        ),
        (
            format!("{}/trailing ", parent.display()),
            "name_trailing_dot_or_space",
        ),
    ] {
        let document = refused(
            &["new", "x", "--path", &case, "--yes", "--output", "json"],
            base.path(),
        );
        assert_eq!(
            document["error"]["details"]["rule"], expected_rule,
            "the path-derived name was refused by the wrong rule: {document}"
        );
    }

    assert_eq!(
        std::fs::read_dir(&parent).expect("readable").count(),
        0,
        "a refused path-derived name still created a directory"
    );
}

#[test]
fn an_ordinary_punctuated_directory_name_is_still_accepted() {
    // THE CONTROL for the test above, and it is doing real work: the fix deliberately rejects only
    // control characters and a trailing dot or space, NOT the full ASCII-alphanumeric rule that
    // guards a package name. An operator may reasonably write `--path ./my.project` or a path with
    // a space in it, and refusing those would break ordinary use to prevent nothing.
    let base = tempfile::tempdir().expect("a temporary directory");
    for name in ["my.project", "my project", "weird!@#name", "v1.2.3"] {
        let destination = base.path().join(name);
        let (exit, _, stderr) = renvor(
            &[
                "new",
                "app",
                "--path",
                destination.to_str().expect("utf-8"),
                "--yes",
            ],
            base.path(),
            &[],
        );
        assert_eq!(
            exit, 0,
            "`{name}` is an ordinary directory name and was refused:\n{stderr}"
        );
        assert!(
            destination.join("renvor.toml").is_file(),
            "`{name}` produced no project"
        );
    }
}

/// The four classes of control character an attacker can put in a *malformed* argument, and the
/// bytes that prove each reached the terminal unfiltered.
///
/// Kept as bytes rather than as an escaped string so the assertion below is about what the
/// terminal receives, not about how Rust spells it.
const TERMINAL_CONTROL_BYTES: [(&str, u8); 5] = [
    ("ESC (CSI, colour and cursor control)", 0x1b),
    ("BEL (terminates an OSC payload)", 0x07),
    ("BS (rewrites the line already printed)", 0x08),
    ("CR (returns to column zero and overwrites)", 0x0d),
    ("C1 CSI (the 8-bit form of ESC-[)", 0x9b),
];

#[test]
fn no_raw_control_character_from_argv_ever_reaches_a_human_stream() {
    // The sibling of `redaction.rs::no_raw_control_character_from_a_path_ever_reaches_a_human_stream`
    // for the one path that test structurally cannot reach: a **malformed** invocation, which
    // returns from clap before any Reporter exists. Every hostile fixture in this suite and in
    // `redaction.rs` drives a *valid* invocation, so this class had no coverage at all.
    let base = tempfile::tempdir().expect("tempdir");

    // Four distinct shapes, so a fix that handles only the unknown-flag case is caught.
    let hostile: [(&str, String); 4] = [
        (
            "unknown flag carrying a colour sequence",
            format!("--{}[31mBOOM{}[0m", '\u{1b}', '\u{1b}'),
        ),
        (
            "unknown flag carrying OSC window-title and clipboard writes",
            format!(
                "--b{}]0;PWNED{}{}]52;c;cm93bmVk{}",
                '\u{1b}', '\u{7}', '\u{1b}', '\u{7}'
            ),
        ),
        (
            "unknown flag carrying an embedded newline that forges a line",
            "--evil\nerror: forged line written by the attacker\nreal".to_owned(),
        ),
        (
            "unknown flag carrying C1 controls and a bidi override",
            format!("--c{}31mC1{}D{}", '\u{9b}', '\u{d}', '\u{202e}'),
        ),
    ];

    for (description, argument) in &hostile {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_renvor"))
            .args([argument.as_str(), "new", "demo"])
            .current_dir(base.path())
            .output()
            .expect("renvor runs");

        for stream_name in ["stdout", "stderr"] {
            let raw = if stream_name == "stdout" {
                &output.stdout
            } else {
                &output.stderr
            };
            for (byte_name, byte) in TERMINAL_CONTROL_BYTES {
                assert!(
                    !raw.contains(&byte),
                    "{description}: a raw {byte_name} byte reached {stream_name}"
                );
            }
            // A newline is legitimate line structure, so it cannot be banned outright. What is
            // banned is a newline that came from the argument: the forged text that follows it.
            let text = String::from_utf8_lossy(raw);
            assert!(
                !text.contains("\nerror: forged line written by the attacker"),
                "{description}: the argument forged its own line on {stream_name}"
            );
        }

        // POSITIVE CONTROL. Every assertion above is an absence, and a binary that printed nothing
        // at all would satisfy all of them. The diagnostic must still be there and still be useful.
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("error:") && stderr.contains("Usage:"),
            "{description}: the usage diagnostic was lost, so the assertions above proved nothing"
        );
        assert_eq!(output.status.code(), Some(2), "{description}: exit code");
    }
}

#[test]
fn a_non_utf8_argument_is_a_classified_error_and_not_a_panic() {
    // `std::env::args()` panics on an argument that is not valid Unicode, and the `--output`
    // pre-scan called it. The result was exit 101 — a code contract C-1 does not define — with
    // zero bytes on stdout, so a `--output json` consumer got no document at all.
    let base = tempfile::tempdir().expect("tempdir");

    #[cfg(unix)]
    let bad: std::ffi::OsString = {
        use std::os::unix::ffi::OsStringExt as _;
        std::ffi::OsString::from_vec(b"proj-\xff".to_vec())
    };
    #[cfg(windows)]
    let bad: std::ffi::OsString = {
        use std::os::windows::ffi::OsStringExt as _;
        // An unpaired high surrogate: ill-formed UTF-16, the Windows equivalent of a stray 0xFF.
        std::ffi::OsString::from_wide(&[0x0070, 0x0072, 0x006f, 0x006a, 0xd800])
    };

    for extra in [vec![], vec!["--output", "json"]] {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_renvor"));
        command.arg("new").arg(&bad);
        command.args(&extra);
        let output = command
            .current_dir(base.path())
            .output()
            .expect("renvor runs");

        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, 101,
            "a non-UTF-8 argument still panics (extra: {extra:?})"
        );
        assert!(
            [1, 2, 3].contains(&code),
            "exit code {code} is not in the C-1 taxonomy (extra: {extra:?})"
        );

        if !extra.is_empty() {
            serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap_or_else(|error| {
                panic!(
                    "`--output json` produced no parseable document: {error}\n---\n{}\n---",
                    String::from_utf8_lossy(&output.stdout)
                )
            });
        }
    }
}

#[test]
fn a_hostile_argv0_does_not_reach_the_terminal_raw() {
    // `argv[0]` is echoed into clap's `Usage:` line, and the binary's name is chosen by whoever
    // invokes it — a symlink, a copy, or a shell alias. The `--help` arm is the one with no prior
    // coverage at all: it writes to **stdout** and exits **0**, so nothing in the suite looked at
    // it. Both arms went through `error.print()`, which filters nothing.
    let base = tempfile::tempdir().expect("tempdir");
    let hostile_name = format!("ren{}[31mPWN{}[0mvor", '\u{1b}', '\u{1b}');
    let planted = base.path().join(&hostile_name);
    match std::fs::copy(env!("CARGO_BIN_EXE_renvor"), &planted) {
        Ok(_) => {}
        // Windows refuses to CREATE this name at all (`ERROR_INVALID_NAME`, os error 123): a
        // control character is not a legal filename character there. So the vector genuinely does
        // not exist on Windows, and this is asserted rather than skipped — the reason a test does
        // not apply is worth pinning, because "we could not set it up" and "it cannot happen" are
        // very different statements and only one of them stays true if the platform changes.
        // The `cfg!` is in the GUARD, not in an assertion: on any other platform this arm does not
        // match and the run falls through to the panic below, which is the assertion — a control
        // character refused in a filename off Windows is a real finding, not a skip.
        Err(error) if error.kind() == std::io::ErrorKind::InvalidFilename && cfg!(windows) => {
            return;
        }
        Err(error) => panic!("the binary could not be copied: {error:?}"),
    }

    for (arguments, stream_name, expected_code) in [
        (vec!["--bogus", "new", "demo"], "stderr", 2),
        (vec!["--help"], "stdout", 0),
    ] {
        let output = std::process::Command::new(&planted)
            .args(&arguments)
            .current_dir(base.path())
            .output()
            .expect("the planted binary runs");

        let raw = if stream_name == "stdout" {
            &output.stdout
        } else {
            &output.stderr
        };
        assert!(
            !raw.contains(&0x1b),
            "a raw ESC byte from argv[0] reached {stream_name} for {arguments:?}"
        );
        // POSITIVE CONTROL: the usage text must still be produced, or the absence above is
        // satisfied by a binary that printed nothing.
        assert!(
            String::from_utf8_lossy(raw).contains("Usage:"),
            "the usage text was lost for {arguments:?}, so the assertion above proved nothing"
        );
        assert_eq!(
            output.status.code(),
            Some(expected_code),
            "exit code for {arguments:?}"
        );
    }
}

#[test]
fn help_reports_a_failed_write_instead_of_exiting_zero() {
    // `--help` discarded the write `Result` and hardcoded exit 0, so on a full filesystem it wrote
    // nothing, said nothing, and reported success — which C-1 forbids.
    //
    // `/dev/full` accepts `open` and fails every `write` with ENOSPC, which is exactly the shape
    // being tested. It exists on Linux and not on macOS, so this asserts on the Linux CI job and
    // says so loudly elsewhere rather than passing silently.
    if !std::path::Path::new("/dev/full").exists() {
        eprintln!(
            "SKIPPED help_reports_a_failed_write_instead_of_exiting_zero: no /dev/full on this \
             platform. This test did NOT run and proves nothing here; the Linux CI job is where \
             it is required to execute."
        );
        return;
    }

    let full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .expect("/dev/full opens");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_renvor"))
        .arg("--help")
        .stdout(std::process::Stdio::from(full))
        .output()
        .expect("renvor runs");

    assert_ne!(
        output.status.code(),
        Some(0),
        "a help write that failed still reported success"
    );
    assert!(
        !output.stderr.is_empty(),
        "the write failure was not reported anywhere"
    );
}

#[test]
#[cfg(unix)]
fn an_external_tools_version_output_cannot_forge_a_doctor_row() {
    // `doctor` interpolates another program's raw `--version` bytes into its report. A newline in
    // that output ends the row renvor wrote and starts one the *tool* wrote, which renvor then
    // presents as its own finding — while exiting 0.
    //
    // The existing structural guard (`no_production_site_interpolates_a_raw_path_into_a_message`)
    // cannot catch this: `doctor.rs` is not in its source list and it only scans `.display()`
    // sites. This asserts the behaviour instead, by comparing against a benign shim so the
    // difference measured is exactly the injected newline.
    use std::os::unix::fs::PermissionsExt as _;

    fn report_lines(base: &std::path::Path, version_output: &str) -> usize {
        let bin = base.join("bin");
        std::fs::create_dir_all(&bin).expect("shim directory");
        let shim = bin.join("git");
        std::fs::write(
            &shim,
            format!("#!/bin/sh\nprintf '%s' '{version_output}'\n"),
        )
        .expect("write shim");
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755))
            .expect("make the shim executable");

        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_renvor"))
            .arg("doctor")
            .env("PATH", path)
            .current_dir(base)
            .output()
            .expect("renvor runs");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(
            text.contains("2.54.0"),
            "POSITIVE CONTROL: the shim's version never reached the report, so this test would \
             pass without exercising the interpolation at all:\n{text}"
        );
        text.lines().count()
    }

    let benign = tempfile::tempdir().expect("tempdir");
    let hostile = tempfile::tempdir().expect("tempdir");

    let expected = report_lines(benign.path(), "git version 2.54.0");
    let actual = report_lines(
        hostile.path(),
        "git version 2.54.0\nok       FORGED   9.9.9",
    );

    assert_eq!(
        actual,
        expected,
        "an external tool's output added {} line(s) to renvor's own report",
        actual.saturating_sub(expected)
    );
}
