//! The version-2 provenance record, end to end through the shipped binary (Phase 012, L-2,
//! FR-012-5b … FR-012-5e).
//!
//! # A fabricated tree and a fabricated record
//!
//! `tests/fixtures/record_v2/tree` is a hand-made project that is never built: `renvor check`
//! builds nothing, and these tests are about what the record says and what the working tree's
//! contents say beside it. `tests/fixtures/record_v2/generated.toml` is a version-2 record in the
//! renderer's exact layout whose `tree_digest` was computed **outside this crate** (Python's
//! `hashlib`, 2026-09-07) over that tree under scope 1 — so the *current* verdict below is a
//! control of the whole digest rule against an independent implementation, not of the crate
//! against itself.
//!
//! # What "historical" means here
//!
//! FR-012-5d compares by contents. Every test that edits the tree leaves the record untouched, so
//! the verdict can only come from the recomputed digest — never from a counter, and never from
//! the record having been rewritten. An edit inside a managed block and an edit outside one are
//! both changes to compiled source (freshness is not ownership); `README.md` is outside the
//! scope and changes nothing.

mod harness;

use std::path::{Path, PathBuf};

use harness::renvor;

/// The fixture's root: the tree and the record beside it.
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("record_v2")
}

/// The workspace root, for the contracts.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root resolves")
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("the fixture tree is readable") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("typed").is_dir() {
            std::fs::create_dir_all(&target).expect("mkdir");
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// A fresh copy of the fixture tree with the fixture record at `.renvor/generated.toml`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_tree(&fixture().join("tree"), dir.path());
    std::fs::create_dir_all(dir.path().join(".renvor")).expect("mkdir");
    std::fs::copy(
        fixture().join("generated.toml"),
        dir.path().join(".renvor").join("generated.toml"),
    )
    .expect("the record is copied");
    dir
}

/// Every file under `root`, relative path and bytes, sorted — the whole working tree.
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("readable") {
            let entry = entry.expect("an entry");
            let path = entry.path();
            if entry.file_type().expect("typed").is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("under the root")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((relative, std::fs::read(&path).expect("read")));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn rewrite_record(dir: &Path, from: &str, to: &str) {
    let path = dir.join(".renvor").join("generated.toml");
    let text = std::fs::read_to_string(&path).expect("read");
    let edited = text.replace(from, to);
    assert_ne!(edited, text, "the record fixture must change");
    std::fs::write(path, edited).expect("write");
}

/// `renvor check` in human mode: exit code and stdout.
fn check(dir: &Path) -> (i32, String) {
    let (code, stdout, stderr) = renvor(&["check", "."], dir, &[]);
    assert!(
        stderr.is_empty(),
        "check writes nothing to stderr on success"
    );
    (code, stdout)
}

/// `renvor check --output json`: the parsed document.
fn check_json(dir: &Path) -> (i32, serde_json::Value) {
    let (code, stdout, _) = renvor(&["check", ".", "--output", "json"], dir, &[]);
    let document = serde_json::from_str(stdout.trim()).expect("one JSON document");
    (code, document)
}

const HISTORICAL: &str = "verified_with: historical — the tree verified at 2026-09-07T00:00:00Z \
                          (new) is not the current tree; not proof of the current tree";

fn assert_historical(dir: &Path) {
    let (code, stdout) = check(dir);
    assert_eq!(code, 0, "historical is reported, never refused");
    assert!(
        stdout.contains(HISTORICAL),
        "the verdict names the verification and the operation"
    );
    assert!(!stdout.contains("verified_with: current"));
    let (_, document) = check_json(dir);
    assert_eq!(document["result"]["verified_with"]["historical"], true);
}

#[test]
fn check_reports_verified_with_as_current_on_an_untouched_tree() {
    let dir = project();
    let (code, stdout) = check(dir.path());
    assert_eq!(code, 0);
    for needle in [
        "[toolchain]",
        "pinned",
        "rust_version",
        "channel 1.94.0 equals the recorded pin",
        "[verified_with]",
        "operation",
        "verified_at",
        "2026-09-07T00:00:00Z",
        "launched",
        "rustc (observed)",
        "rustc (resolved before verification)",
        "toolchain_file",
        "driver 0.1.94 (4a4ef493e3)",
        "verified_with: current",
    ] {
        assert!(
            stdout.contains(needle),
            "the human report names each table's fields"
        );
    }
    assert!(
        !stdout.contains("historical"),
        "an untouched tree is current"
    );

    let (code, document) = check_json(dir.path());
    assert_eq!(code, 0);
    let result = &document["result"];
    assert_eq!(result["toolchain"]["pinned"], "1.94.0");
    assert_eq!(result["toolchain"]["rust_version"], "1.94.0");
    let verified = &result["verified_with"];
    assert_eq!(verified["historical"], false);
    assert_eq!(verified["operation"], "new");
    assert_eq!(verified["tree_scope"], 1);
    assert_eq!(verified["observation"], "launched");
    assert_eq!(verified["rustc_release"], "1.94.0");
    assert_eq!(verified["resolved_rustc_release"], "1.94.0");
    assert_eq!(
        verified["configured_rustc_release"],
        serde_json::Value::Null,
        "an absent optional field is null, never filled in"
    );
    assert_eq!(verified["selected_by"], "toolchain_file");
    assert_eq!(verified["checks"]["clippy"]["driver_release"], "0.1.94");
    assert_eq!(verified["checks"]["build"]["units_launched"], 1);
    assert_eq!(verified["checks"]["run"]["outcome"], "passed");
    // The manifest's own fields are still there: the tables are additive (C-2).
    assert_eq!(result["name"], "fixture");
    assert_eq!(result["templateVersion"], "8");
}

#[test]
fn a_manual_edit_inside_a_managed_block_with_an_unchanged_record_is_historical() {
    let dir = project();
    let main = dir.path().join("src").join("main.rs");
    let text = std::fs::read_to_string(&main).expect("read");
    let edited = text.replace(
        "    // renvor:resources:begin\n    // renvor:resources:end\n",
        "    // renvor:resources:begin\n    let _ = 1;\n    // renvor:resources:end\n",
    );
    assert_ne!(edited, text, "the edit lands between the markers");
    std::fs::write(&main, edited).expect("write");
    assert_historical(dir.path());
}

#[test]
fn a_manual_edit_outside_a_managed_block_with_an_unchanged_record_is_historical() {
    let dir = project();
    let main = dir.path().join("src").join("main.rs");
    let text = std::fs::read_to_string(&main).expect("read");
    let edited = text.replace("    let _ = \"fixture\";\n", "    let _ = \"edited\";\n");
    assert_ne!(edited, text, "the edit lands outside the markers");
    std::fs::write(&main, edited).expect("write");
    assert_historical(dir.path());
}

#[test]
fn a_file_added_under_src_is_historical() {
    let dir = project();
    std::fs::write(
        dir.path().join("src").join("extra.rs"),
        "pub fn extra() {}\n",
    )
    .expect("write");
    assert_historical(dir.path());
}

#[test]
fn a_file_deleted_from_tests_is_historical() {
    let dir = project();
    std::fs::remove_file(dir.path().join("tests").join("smoke.rs")).expect("remove");
    assert_historical(dir.path());
}

#[test]
fn a_readme_edit_stays_current() {
    let dir = project();
    std::fs::write(dir.path().join("README.md"), "# fixture, edited\n").expect("write");
    let (code, stdout) = check(dir.path());
    assert_eq!(code, 0);
    assert!(
        stdout.contains("verified_with: current"),
        "README.md is outside the scope"
    );
    let (_, document) = check_json(dir.path());
    assert_eq!(document["result"]["verified_with"]["historical"], false);
}

#[test]
fn an_edited_pin_file_is_reported_never_refused() {
    let dir = project();
    std::fs::write(
        dir.path().join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.97.1\"\n",
    )
    .expect("write");
    let (code, stdout) = check(dir.path());
    assert_eq!(code, 0, "an author's edit is reported, never refused");
    assert!(stdout.contains("the file's channel 1.97.1 differs from the recorded pin 1.94.0"));
    // The pin file is in scope, so the evidence is historical too — by contents.
    assert!(stdout.contains(HISTORICAL));
}

#[test]
fn an_unknown_tree_scope_is_record_unsupported() {
    let dir = project();
    rewrite_record(dir.path(), "tree_scope = 1\n", "tree_scope = 9\n");
    let (code, document) = check_json(dir.path());
    assert_eq!(
        code, 3,
        "an unsupported input record is a validation failure"
    );
    assert_eq!(document["status"], "failure");
    assert_eq!(document["error"]["code"], "record_unsupported");
    assert_eq!(document["error"]["details"]["tree_scope"], "9");
    assert_eq!(document["error"]["details"]["supported"], "1");
    let (code, stdout, stderr) = renvor(&["check", "."], dir.path(), &[]);
    assert_eq!(code, 3);
    assert!(
        stdout.is_empty(),
        "a human-mode failure leaves stdout empty"
    );
    assert!(stderr.contains("rebuild the generator, not the project"));
}

#[test]
fn a_newer_record_is_refused_by_name_before_any_plan() {
    let dir = project();
    rewrite_record(dir.path(), "record_version = 2\n", "record_version = 3\n");
    let before = snapshot(dir.path());

    // Through `check`.
    let (code, document) = check_json(dir.path());
    assert_eq!(code, 3, "exit 3 — U-1");
    assert_eq!(document["error"]["code"], "record_unsupported");
    assert_eq!(document["error"]["details"]["record_version"], "3");
    assert_eq!(document["error"]["details"]["supported"], "2");
    assert!(
        document["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("rebuild the generator, not the project")
    );

    // Through `generate`, dry run and real run alike: refused before anything is planned, and
    // the working tree — the record included — is byte-identical afterwards.
    for arguments in [
        vec![
            "generate",
            "migration",
            "add_thing",
            "--dry-run",
            "--output",
            "json",
        ],
        vec!["generate", "migration", "add_thing", "--output", "json"],
    ] {
        let (code, stdout, _) = renvor(&arguments, dir.path(), &[]);
        assert_eq!(code, 3, "generate refuses the newer record by name");
        let document: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("one JSON document");
        assert_eq!(document["error"]["code"], "record_unsupported");
        assert_eq!(document["error"]["details"]["record_version"], "3");
        assert_eq!(document["error"]["details"]["supported"], "2");
        assert_eq!(
            snapshot(dir.path()),
            before,
            "the working tree is byte-identical after the refusal"
        );
    }
    assert!(
        !dir.path()
            .join("migrations")
            .join("add_thing.up.sql")
            .exists()
            && std::fs::read_dir(dir.path().join("migrations"))
                .expect("readable")
                .count()
                == 2,
        "no migration was written"
    );
}

#[test]
fn record_unsupported_is_one_string_across_contract_registry_fixture_and_help() {
    // FR-012-5b: the reason string and the two `details` keys are the same text in C-1's table,
    // C-2's registry, the JSON fixture, and the help/README sentences that name them.
    let root = workspace();
    for contract in ["command-surface.md", "json-output.md"] {
        let text = std::fs::read_to_string(root.join("contracts").join(contract))
            .expect("the contract is readable");
        for needle in [
            "`record_unsupported`",
            "`details.record_version`",
            "`details.supported`",
            "rebuild the generator, not the project",
        ] {
            assert!(
                text.contains(needle),
                "a contract does not carry the shared text"
            );
        }
    }

    // The fixture: the shape C-2 promises, and the bytes the binary emits today.
    let expected = include_str!("json/record_unsupported.json");
    assert!(
        !expected.contains('\r'),
        "the fixture was checked out with CRLF"
    );
    let document: serde_json::Value = serde_json::from_str(expected).expect("the fixture parses");
    assert_eq!(document["error"]["code"], "record_unsupported");
    assert_eq!(document["error"]["details"]["record_version"], "3");
    assert_eq!(document["error"]["details"]["supported"], "2");
    let dir = project();
    rewrite_record(dir.path(), "record_version = 2\n", "record_version = 3\n");
    let (_, stdout, _) = renvor(&["--output", "json", "check", "."], dir.path(), &[]);
    assert_eq!(
        stdout, expected,
        "the JSON document changed; C-8 governs presentation and may not move a byte of this"
    );

    // The help and the README sentences (T-012-02, T-012-03 — owned by the templates and
    // command-surface batches): the same reason string, spelled once. The README templates
    // carry the remedy sentence; the help sentence is the command-surface batch's, and this
    // assertion is what says so if it has not landed.
    let mut help = String::new();
    for command in ["generate", "check"] {
        let (code, text, _) = renvor(&[command, "--help"], dir.path(), &[]);
        assert_eq!(code, 0, "help renders");
        help.push_str(&text);
    }
    assert!(
        help.contains("record_unsupported"),
        "neither the generate help nor the check help names the refusal by its reason string"
    );
    for template in ["README.md.j2", "starter/README.md.j2"] {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("templates")
                .join(template),
        )
        .expect("the README template is readable");
        assert!(
            text.contains("rebuild the generator, not the project"),
            "a README template does not carry the remedy sentence"
        );
    }
}
