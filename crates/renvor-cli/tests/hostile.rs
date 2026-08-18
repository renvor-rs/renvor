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
const RESERVED_DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
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
    assert_eq!(
        document["error"]["details"]["rule"], "not_a_symlink",
        "the symlink must be refused as a symlink: {document}"
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
        document["error"]["code"], "destination_not_empty",
        "the code must say WHY it was rejected, not merely that it was: {document}"
    );
    assert_eq!(
        document["error"]["details"]["rule"], "destination_absent_or_empty",
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
