//! Phase 003 projects still validate under the Phase 004 CLI.
//!
//! # The defect this exists for
//!
//! Phase 004 added `transport` to the generated manifest. The first attempt made the field
//! **required**, and `renvor check` then rejected **every project Phase 003 had generated**, with
//! `missing field \`transport\`` and no migration path. A framework that invalidates the projects it
//! generated one phase earlier has broken its own output.
//!
//! # Why a synthetic manifest was not enough
//!
//! That defect was fixed, and its first regression test used a manifest **written by hand to
//! resemble** Phase 003's. Such a test proves only that the code agrees with the test author's
//! recollection — if the recollection is wrong in the same direction as the code, both are wrong
//! together and the test passes.
//!
//! The fixture these tests read was produced by **running the Phase 003 generator**, built from
//! `10da854736598d99218d1627c3ad79866a2f7f89`, the live `main` this branch forked from. Its
//! provenance is recorded beside it. It is a real artifact, not a description of one.
//!
//! # One fixture here was made differently, and says so
//!
//! Three of the four fixtures — Phase 003, version 3, version 6 — were produced by building the
//! generator of their version and running it. The fourth, `template-7-project`, was **not**: its
//! version is `main` at `7281e4f`, whose generator is the one this branch is editing, and a
//! second full workspace build for that is not what the difference is worth. It was reconstructed
//! by applying the inverse of this branch's own template diff to the current binary's output, and
//! that recipe was first proved byte-for-byte against `snapshots__manifest-v7-bare.snap` — a
//! tracked, unmodified record of what the version-7 generator produced. Its `PROVENANCE.md` gives
//! the digests and the two places it departs from raw generator output.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The captured Phase 003 project.
fn phase_003_project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("phase-003-project")
}

fn check(directory: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_renvor"))
        .arg("check")
        // PATH is positional for `check` — `--path` is `new`'s spelling, and the two commands
        // deliberately differ. Getting this wrong produced a `usage` failure that looked, at a
        // glance, exactly like the manifest rejection this test exists to catch.
        .arg(directory)
        .arg("--output")
        .arg("json")
        .output()
        .expect("the CLI runs")
}

/// `renvor check` in its human form, for the lines the JSON reports as fields.
///
/// `verified_with: current` is a verdict the human report states in one line and the document
/// states as `historical: false`; the two are asserted together below, because a report that says
/// one thing on the terminal and another in the pipe is the defect.
fn check_human(directory: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_renvor"))
        .arg("check")
        .arg(directory)
        .output()
        .expect("the CLI runs")
}

#[test]
fn the_fixture_really_is_a_phase_003_manifest() {
    // Guards the two tests below. If someone regenerates this fixture with the Phase 004
    // generator, it grows a `transport` field and stops testing backward compatibility — while
    // still passing. This fails instead.
    let manifest = std::fs::read_to_string(phase_003_project().join("renvor.toml"))
        .expect("the fixture manifest is readable");

    assert!(
        manifest.contains("template_version = \"1\""),
        "the fixture is not template version 1, so it is not a Phase 003 artifact:\n{manifest}"
    );
    assert!(
        !manifest.contains("transport"),
        "the fixture records a transport, so it was not produced by the Phase 003 generator:\n\
         {manifest}"
    );
}

#[test]
fn a_real_phase_003_project_still_passes_check() {
    // THE REGRESSION. This is the exact input that used to fail with `missing field transport`.
    let output = check(&phase_003_project());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "a Phase 003 project was rejected by the Phase 004 CLI.\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("check emits one JSON document");
    assert_eq!(parsed["status"], "success", "{stdout}");
}

#[test]
fn a_phase_004_project_also_passes_check() {
    // POSITIVE CONTROL. Without it, a `check` that accepted everything would pass the test above
    // and the compatibility would be unproven.
    //
    // Generated here by the CURRENT binary rather than stored, so it is genuinely this version's
    // output rather than a second fixture that could go stale.
    let workspace = tempfile::tempdir().expect("tempdir");
    let project = workspace.path().join("current-api");

    let generated = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .arg("new")
        .arg("current-api")
        .arg("--path")
        .arg(&project)
        .arg("--target")
        .arg("api")
        .arg("--yes")
        .output()
        .expect("the CLI runs");
    assert!(
        generated.status.success(),
        "generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let manifest = std::fs::read_to_string(project.join("renvor.toml")).expect("readable");
    assert!(
        manifest.contains("transport"),
        "this version's generator did not record a transport, so the control proves nothing:\n\
         {manifest}"
    );

    let output = check(&project);
    assert!(
        output.status.success(),
        "this version's own output failed its own check: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

#[test]
fn a_manifest_naming_an_unsupported_transport_is_still_refused() {
    // The compatibility above must not have been bought by accepting anything. A transport this
    // version does not ship describes a project it could not have generated, and is refused —
    // which is a different fact from the field being absent.
    let workspace = tempfile::tempdir().expect("tempdir");
    let project = workspace.path().join("bogus");
    std::fs::create_dir_all(&project).expect("mkdir");

    let mut manifest =
        std::fs::read_to_string(phase_003_project().join("renvor.toml")).expect("readable");
    manifest.push_str("\ntransport = \"carrier-pigeon\"\n");
    std::fs::write(project.join("renvor.toml"), manifest).expect("write");

    let output = check(&project);
    assert!(
        !output.status.success(),
        "an unsupported transport was accepted"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("check emits one JSON document on failure too");
    assert_eq!(parsed["status"], "failure", "{stdout}");
    assert_eq!(parsed["error"]["code"], "manifest_invalid", "{stdout}");
}

// ─────────────────────────────────────────── template version 3 → 4 (Phase 006 containers)

/// The captured template-version-3 project.
///
/// Produced by running the version-3 generator from a detached worktree at `1a83149`, for the same
/// reason the Phase 003 fixture was: an imitation proves only that the code matches somebody's
/// recollection. See its `PROVENANCE.md`.
fn version_3_project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("phase-006-v3-project")
}

#[test]
fn the_fixture_really_is_a_version_3_manifest() {
    // The same guard the Phase 003 fixture has. Regenerating this with the current binary would
    // give it a `[container]` section and stop it testing anything — while still passing.
    let manifest =
        std::fs::read_to_string(version_3_project().join("renvor.toml")).expect("readable");
    assert!(
        manifest.contains("template_version = \"3\""),
        "the fixture is not template version 3:\n{manifest}"
    );
    assert!(
        !manifest.contains("[container]"),
        "the fixture already has a `[container]` section, so it is not a version-3 artifact:\n\
         {manifest}"
    );
    // It DOES record `container = true` in `[project]`, which is exactly the interesting case:
    // version 3 generated container controls and described them nowhere else.
    assert!(manifest.contains("container = true"));
}

#[test]
fn a_version_3_project_still_validates() {
    // THE DEFECT THIS EXISTS FOR, one version later. Phase 004 made `transport` required and
    // invalidated every Phase 003 project. Adding `[container]` in version 4 is the same shape of
    // change, and `#[serde(default)]` on the section is what stops it being the same mistake.
    let output = check(&version_3_project());
    assert!(
        output.status.success(),
        "a template-version-3 project was rejected by the current CLI\n--- stdout ---\n{}\n\
         --- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

#[test]
fn a_version_3_project_is_not_silently_upgraded() {
    // `renvor check` VALIDATES. It must not rewrite the manifest it was pointed at — a validator
    // that edits its input is a migration tool wearing a validator's name, and the operator who
    // ran it in CI did not ask for a schema change.
    let before =
        std::fs::read_to_string(version_3_project().join("renvor.toml")).expect("readable");
    let _ = check(&version_3_project());
    let after = std::fs::read_to_string(version_3_project().join("renvor.toml")).expect("readable");
    assert_eq!(before, after, "`renvor check` rewrote the manifest");
}

// ─────────────────────────────────────────── template version 6 → 7 (Phase 011 starters)

/// The captured template-version-6 project.
///
/// Produced by running the version-6 generator at `6b9b70a`, the last commit before any Phase 011
/// template changed, with a SeaORM database and a cache container — the configuration whose
/// manifest carries every key version 7 made conditional. See its `PROVENANCE.md`.
fn version_6_project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("phase-010-v6-project")
}

#[test]
fn the_fixture_really_is_a_version_6_manifest() {
    // The same guard the earlier fixtures have. Regenerating this with the current binary would
    // give it `auth`, `[capabilities]`, and possibly `[framework]`, and stop it testing anything.
    let manifest =
        std::fs::read_to_string(version_6_project().join("renvor.toml")).expect("readable");
    assert!(
        manifest.contains("template_version = \"6\""),
        "the fixture is not template version 6:\n{manifest}"
    );
    for absent in ["auth", "[framework]", "[capabilities]"] {
        assert!(
            !manifest.contains(absent),
            "the fixture already carries `{absent}`, so it is not a version-6 artifact:\n{manifest}"
        );
    }
    // And it records the sentence version 7 made conditional, as a constant.
    assert!(manifest.contains("cache_wired_into_application = false"));
}

#[test]
fn a_version_6_project_still_validates() {
    // Phase 011 added `project.auth`, `[framework]`, `[auth]`, and `[capabilities]`. Making any
    // of them required would be the Phase 004 defect a third time.
    let output = check(&version_6_project());
    assert!(
        output.status.success(),
        "a template-version-6 project was rejected by the current CLI\n--- stdout ---\n{}\n\
         --- stderr ---\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("check emits one JSON document");
    assert_eq!(parsed["status"], "success");
    // The absences are reported as absences, not invented.
    assert!(parsed["result"]["auth"].is_null());
    assert!(parsed["result"]["capabilities"].is_null());
    assert!(parsed["result"]["framework"].is_null());
}

#[test]
fn a_version_6_project_is_not_silently_upgraded() {
    let before =
        std::fs::read_to_string(version_6_project().join("renvor.toml")).expect("readable");
    let _ = check(&version_6_project());
    let after = std::fs::read_to_string(version_6_project().join("renvor.toml")).expect("readable");
    assert_eq!(before, after, "`renvor check` rewrote the manifest");
}

#[test]
fn a_current_skeleton_still_passes_check_and_records_its_choices() {
    // POSITIVE CONTROL for the version-6 tests, and the compatibility promise from the other
    // side: the skeleton the current generator produces records `auth = "none"` and a
    // `[capabilities]` table of five declines, and validates.
    //
    // It also carries what template version 8 added — the pin, the floor, the README that
    // explains them, and the evidence `renvor check` reads back. Those are asserted here rather
    // than in a file of their own because THIS is the tree the legacy tests below are a contrast
    // to: without them, "a legacy tree has no pin" would be true of every tree.
    let workspace = tempfile::tempdir().expect("tempdir");
    let project = workspace.path().join("current-skeleton");
    let generated = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .arg("new")
        .arg("current-skeleton")
        .arg("--path")
        .arg(&project)
        .arg("--yes")
        .output()
        .expect("the CLI runs");
    assert!(
        generated.status.success(),
        "generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );
    let manifest = std::fs::read_to_string(project.join("renvor.toml")).expect("readable");
    assert!(manifest.contains("template_version = \"8\""), "{manifest}");
    assert!(manifest.contains("auth = \"none\""), "{manifest}");
    assert!(manifest.contains("[capabilities]"), "{manifest}");
    assert!(
        !manifest.contains("[framework]") && !manifest.contains("[auth]"),
        "a skeleton must record no framework and no auth table:\n{manifest}"
    );

    // D-L2-4: a skeleton pins its own MSRV, so the two numbers are the same number — read from
    // the two files that carry them rather than from the record, which is what an operator
    // reads.
    let pin = std::fs::read_to_string(project.join("rust-toolchain.toml"))
        .expect("a generated project carries `rust-toolchain.toml`");
    let channel = pin
        .lines()
        .find_map(|line| line.strip_prefix("channel = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("`rust-toolchain.toml` names a channel");
    let cargo = std::fs::read_to_string(project.join("Cargo.toml")).expect("readable");
    let rust_version = cargo
        .lines()
        .find_map(|line| line.strip_prefix("rust-version = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("`Cargo.toml` declares a `rust-version`");
    assert_eq!(
        channel, rust_version,
        "a skeleton pins its own MSRV, so the pin and the floor are one release"
    );

    // The README is where the operator is sent for both numbers and for what honours them, so it
    // has to name all three — the two releases and the rustup the pin needs.
    let readme = std::fs::read_to_string(project.join("README.md")).expect("readable");
    assert!(readme.contains("## Toolchain"), "{readme}");
    assert!(
        readme.contains(channel) && readme.contains(rust_version),
        "the README names neither the pin nor the floor:\n{readme}"
    );
    assert!(
        readme.contains("1.28.1"),
        "the README does not name the rustup floor the pin needs:\n{readme}"
    );

    let output = check(&project);
    assert!(
        output.status.success(),
        "this version's own output failed its own check: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("check emits one JSON document");
    assert_eq!(parsed["result"]["toolchain"]["pinned"], channel);
    assert_eq!(parsed["result"]["verified_with"]["operation"], "new");
    assert_eq!(
        parsed["result"]["verified_with"]["historical"], false,
        "a tree that has not been touched since generation is described by its own evidence"
    );
    let human = check_human(&project);
    assert!(
        String::from_utf8_lossy(&human.stdout).contains("verified_with: current"),
        "the human report does not state the verdict the document reports as `historical: false`"
    );
}

// ─────────────────────────────────── template version 7 → 8 (Phase 012 toolchain declaration)

/// The captured template-version-7 starter.
///
/// Reconstructed from this branch's own template diff rather than by building the version-7
/// generator, and the recipe was proved byte-for-byte against `snapshots__manifest-v7-bare.snap`
/// before it was applied. `PROVENANCE.md` beside the tree states both, and states the two
/// departures from raw generator output (`Cargo.lock` excluded; the framework path replaced by
/// `/opt/renvor`).
fn template_7_project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("template-7-project")
}

/// The framework checkout the fixture's `[framework].path` stands in for.
fn workspace_root() -> PathBuf {
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

/// A writable copy of the template-7 fixture, with a framework path that exists on this machine.
///
/// The fixture records `/opt/renvor` so that no machine's own layout is committed; `renvor
/// generate` resolves `[framework].path` and refuses a directory that is not there, so the real
/// checkout is substituted here. The two files that carry the path are `renvor.toml` and
/// `Cargo.toml`, and their recorded digests are stale from this moment — which changes nothing
/// below: neither file is in a `generate resource` or `generate migration` write set, and every
/// assertion is about the record's head, not about those two entries.
fn legacy_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_tree(&template_7_project(), dir.path());
    let root = workspace_root();
    let root = root.to_str().expect("a utf-8 workspace path");
    // ESCAPED FOR A TOML BASIC STRING, because both files put the path inside one. On Windows the
    // workspace root is `C:\\Users\\…`, and a raw backslash there is an escape sequence: the
    // manifests became unparseable and every test in this file failed with a TOML error naming a
    // column, not a missing pin. Found by the Windows platform legs on 2026-09-08.
    let root = root.replace('\\', "\\\\").replace('"', "\\\"");
    let root = root.as_str();
    for name in ["renvor.toml", "Cargo.toml"] {
        let path = dir.path().join(name);
        let text = std::fs::read_to_string(&path).expect("readable");
        let substituted = text.replace("/opt/renvor", root);
        assert_ne!(
            substituted, text,
            "the fixture no longer carries the placeholder framework path"
        );
        std::fs::write(&path, substituted).expect("write");
    }
    dir
}

/// `renvor generate <arguments>` in `project`: exit code, stdout, stderr.
fn generate(project: &Path, arguments: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .arg("generate")
        .args(arguments)
        .arg("--path")
        .arg(project)
        .output()
        .expect("the CLI runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// The record's head: everything before the first `[[file]]`.
///
/// For a version-2 record this span holds `record_version`, `[toolchain]`, and the whole of
/// `[verified_with]`; for a legacy record it holds the header and the two versions and nothing
/// else. Comparing it before and after is how "leaves `[verified_with]` byte-identical" is
/// asserted without asserting on the digests, which a generation is supposed to change.
fn record_head(project: &Path) -> String {
    let text = std::fs::read_to_string(project.join(".renvor").join("generated.toml"))
        .expect("the provenance record is readable");
    let end = text.find("\n[[file]]\n").expect("the record lists files");
    text[..end].to_owned()
}

/// The one sentence FR-012-10a requires, exactly as `commands/generate.rs` spells it.
const LEGACY_SENTENCE: &str = "this project was generated before template version 8 and pins no \
                               toolchain; no renvor generate toolchain action exists — a new \
                               project's README shows the two files to add by hand";

/// The opening of the FR-012-8 (1) resolution notice, which a non-verifying operation never
/// prints.
const RESOLUTION_NOTICE: &str = "toolchain resolved before verification";

#[test]
fn the_fixture_really_is_a_template_7_tree() {
    // The same guard the three older fixtures have, for the three absences that make this tree a
    // legacy one. Regenerating it with the current binary would give it a pin, a `rust-version`,
    // and a version-2 record, and it would stop testing anything — while still passing.
    let manifest =
        std::fs::read_to_string(template_7_project().join("renvor.toml")).expect("readable");
    assert!(
        manifest.contains("template_version = \"7\""),
        "the fixture is not template version 7:\n{manifest}"
    );

    assert!(
        !template_7_project().join("rust-toolchain.toml").exists(),
        "the fixture carries a pin, so it is not a version-7 artifact"
    );

    let cargo = std::fs::read_to_string(template_7_project().join("Cargo.toml")).expect("readable");
    assert!(
        !cargo.contains("rust-version"),
        "the fixture's `Cargo.toml` declares a `rust-version`, which version 7 never wrote:\n\
         {cargo}"
    );

    let record =
        std::fs::read_to_string(template_7_project().join(".renvor").join("generated.toml"))
            .expect("readable");
    for absent in ["record_version", "[toolchain]", "[verified_with]"] {
        assert!(
            !record.contains(absent),
            "the record carries `{absent}`, so it is not the legacy record a version-7 generator \
             wrote:\n{record}"
        );
    }
    assert!(
        record.contains("[[file]]"),
        "the record lists no files, so it cannot classify anything:\n{record}"
    );

    // And it is a STARTER, which is what makes `generate resource` reach it at all: a skeleton
    // has nothing a resource could be wired into and is refused before any of this matters.
    assert!(manifest.contains("[framework]"), "{manifest}");
    assert!(manifest.contains("[persistence]"), "{manifest}");
}

#[test]
fn generate_resource_into_a_template_7_tree_runs_no_verification_prints_no_notice_and_leaves_verified_with_byte_identical()
 {
    // FR-012-10a and FR-012-10c. `generate resource` runs `rustfmt` and nothing else: it never
    // builds, it writes no evidence, and it says nothing about a toolchain it did not resolve.
    let project = legacy_copy();
    let before = record_head(project.path());

    let (exit, stdout, stderr) = generate(
        project.path(),
        &["resource", "Widget", "title:string", "--output", "json"],
    );
    assert_eq!(exit, 0, "a legacy tree was refused:\n{stdout}\n{stderr}");

    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["status"], "success", "{document}");
    assert_eq!(
        document["result"]["toolchain"],
        serde_json::Value::Null,
        "a legacy tree declares no pin, and nothing may invent one for it"
    );
    assert_eq!(
        document["result"]["verified_with"],
        serde_json::Value::Null,
        "an operation that verifies nothing reports no evidence"
    );

    let after = record_head(project.path());
    assert_eq!(
        before, after,
        "`generate resource` rewrote the record's head — the span that holds `record_version`, \
         `[toolchain]`, and `[verified_with]`"
    );
    for absent in ["record_version", "[toolchain]", "[verified_with]"] {
        assert!(
            !after.contains(absent),
            "the legacy record was upgraded in place by an operation that verified nothing: \
             `{absent}` appeared\n{after}"
        );
    }

    assert!(
        !stderr.contains(RESOLUTION_NOTICE),
        "a resolution notice was printed by an operation that resolved no toolchain:\n{stderr}"
    );
}

#[test]
fn generate_into_a_template_7_tree_inserts_no_pin_and_no_rust_version() {
    // FR-012-10b, the no-silent-insertion control. The toolchain declaration is a template group
    // that renders for `renvor new` or for a record that declares a pin — and a legacy record
    // declares none, so the two files stay absent and the plan names neither.
    let project = legacy_copy();
    let cargo_before =
        std::fs::read_to_string(project.path().join("Cargo.toml")).expect("readable");

    let (exit, stdout, stderr) = generate(
        project.path(),
        &["resource", "Widget", "title:string", "--output", "json"],
    );
    assert_eq!(exit, 0, "{stdout}\n{stderr}");

    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    let planned: Vec<&str> = document["result"]["files"]
        .as_array()
        .expect("a file list")
        .iter()
        .map(|entry| entry["path"].as_str().expect("a path"))
        .collect();
    assert!(
        !planned.contains(&"rust-toolchain.toml"),
        "the plan named a pin file for a tree that pins nothing: {planned:?}"
    );
    assert!(
        !planned.contains(&"Cargo.toml"),
        "the plan re-rendered `Cargo.toml` on a legacy tree: {planned:?}"
    );

    assert!(
        !project.path().join("rust-toolchain.toml").exists(),
        "a pin was inserted into a project that asked for none"
    );
    let cargo_after = std::fs::read_to_string(project.path().join("Cargo.toml")).expect("readable");
    assert!(
        !cargo_after.contains("rust-version"),
        "a `rust-version` line was inserted into a legacy manifest:\n{cargo_after}"
    );
    assert_eq!(
        cargo_before, cargo_after,
        "`Cargo.toml` was rewritten by an operation that does not own it"
    );
}

/// The state the **first** `generate auth` on a legacy tree leaves behind, and what the next
/// generation into that tree must still do.
///
/// `auth` is the one action that verifies, so it is the one that writes `[verified_with]` — and
/// with it a `[toolchain]` table saying `pinned = "none"`, `rust_version = "none"`: the honest
/// record that this tree declares nothing (FR-012-10a). The tree is not thereby a declared tree.
/// A predicate that reads "a `[toolchain]` table exists" instead of "this record names a pin"
/// flips at exactly that point, and the next generation renders the pin group into a project that
/// asked for none — the silent insertion FR-012-10b forbids, arriving one run late.
///
/// **What this file can and cannot show.** `generate resource` re-renders one starter file and
/// never plans the toolchain group, so it stays correct under either predicate: this is a
/// *control* over the state, not the reproduction. The reproduction is at the surface that does
/// plan that group — `a_second_auth_on_a_verified_legacy_tree_plans_no_pin_and_no_rust_version`
/// in `src/commands/generate.rs`, which fails under the wrong predicate — and the live pass, on a
/// real tree with a real build, is the starter matrix's
/// `c_sel_3_a_legacy_tree_resolves_its_ancestor_and_stays_pin_less_across_repeated_auth`. The record is written by hand here
/// because a real `auth` needs the framework checkout and a full starter build.
#[test]
fn a_legacy_tree_that_has_already_been_verified_still_has_no_pin_inserted() {
    let project = legacy_copy();
    let record_path = project.path().join(".renvor").join("generated.toml");
    let record = std::fs::read_to_string(&record_path).expect("readable");
    // A VERSION-2 record: what `apply::commit` rendered for a legacy tree verified by an earlier
    // generator — the version marker, then the table that says the tree declares nothing. It is
    // deliberately not re-labelled to the version this generator writes: a historical record must
    // keep reading, and an operation that verifies nothing must carry its version unchanged.
    let verified = record.replace(
        "generator_version = \"0.0.0\"",
        "record_version = 2\ngenerator_version = \"0.0.0\"",
    ) + "\n[toolchain]\npinned = \"none\"\nrust_version = \"none\"\n";
    assert_ne!(record, verified, "the record fixture was not rewritten");
    std::fs::write(&record_path, &verified).expect("writable");

    let cargo_before =
        std::fs::read_to_string(project.path().join("Cargo.toml")).expect("readable");
    let (exit, stdout, stderr) = generate(
        project.path(),
        &["resource", "Gadget", "title:string", "--output", "json"],
    );
    assert_eq!(exit, 0, "{stdout}\n{stderr}");

    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    let planned: Vec<&str> = document["result"]["files"]
        .as_array()
        .expect("a file list")
        .iter()
        .map(|entry| entry["path"].as_str().expect("a path"))
        .collect();
    assert!(
        !planned.contains(&"rust-toolchain.toml"),
        "a verified legacy tree was treated as declaring a pin: {planned:?}"
    );
    assert!(
        !project.path().join("rust-toolchain.toml").exists(),
        "a pin was inserted into a project that asked for none"
    );
    let cargo_after = std::fs::read_to_string(project.path().join("Cargo.toml")).expect("readable");
    assert!(
        !cargo_after.contains("rust-version"),
        "a `rust-version` line was inserted into a legacy manifest:\n{cargo_after}"
    );
    assert_eq!(
        cargo_before, cargo_after,
        "`Cargo.toml` was rewritten by an operation that does not own it"
    );
    // And the record still says what it said: `none` twice, never filled in.
    let after = std::fs::read_to_string(&record_path).expect("readable");
    assert!(
        after.contains("pinned = \"none\"") && after.contains("rust_version = \"none\""),
        "the record's honest `none` was overwritten:\n{after}"
    );
}

#[test]
fn every_generate_into_a_legacy_tree_states_once_that_no_toolchain_action_exists() {
    // FR-012-10a's last sentence. ONCE — not per file, not per template, and not zero times,
    // which is what would send an operator hunting for a `renvor generate toolchain` that does
    // not exist. Both actions a legacy tree accepts are run, each in its own copy, because
    // "every generate" is a claim about the run and not about one action.
    for arguments in [
        vec!["resource", "Widget", "title:string"],
        vec!["migration", "add_widget"],
    ] {
        let project = legacy_copy();
        let (exit, stdout, stderr) = generate(project.path(), &arguments);
        assert_eq!(exit, 0, "{arguments:?}:\n{stdout}\n{stderr}");
        assert_eq!(
            stderr.matches(LEGACY_SENTENCE).count(),
            1,
            "{arguments:?} did not state exactly once that no toolchain action exists:\n{stderr}"
        );
    }
}

#[test]
fn a_current_tree_is_not_treated_as_legacy() {
    // THE CONTROL. Without it, a sentence printed on every run — or a `record_version` that no
    // generation ever keeps — would pass every test above. The same action is run on both trees,
    // so the only difference is the tree.
    let workspace = tempfile::tempdir().expect("tempdir");
    let project = workspace.path().join("current-tree");
    let generated = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .arg("new")
        .arg("current-tree")
        .arg("--path")
        .arg(&project)
        // A database, because `generate migration` is the action both trees accept; a starter
        // would cost a framework build and prove the same thing about the record.
        .arg("--database")
        .arg("postgres")
        .arg("--example-domain")
        .arg("--yes")
        .output()
        .expect("the CLI runs");
    assert!(
        generated.status.success(),
        "generation failed: {}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let (exit, stdout, stderr) = generate(&project, &["migration", "add_widget"]);
    assert_eq!(exit, 0, "{stdout}\n{stderr}");
    assert!(
        !stderr.contains(LEGACY_SENTENCE),
        "a tree generated by this version was called legacy:\n{stderr}"
    );

    let record =
        std::fs::read_to_string(project.join(".renvor").join("generated.toml")).expect("readable");
    assert!(
        record.contains("record_version = 3"),
        "a generation dropped the record version it read:\n{record}"
    );

    // And the same action on the legacy tree DOES say it — the pair is the evidence, not either
    // half.
    let legacy = legacy_copy();
    let (exit, stdout, stderr) = generate(legacy.path(), &["migration", "add_widget"]);
    assert_eq!(exit, 0, "{stdout}\n{stderr}");
    assert!(
        stderr.contains(LEGACY_SENTENCE),
        "the legacy tree said nothing, so the control above proves nothing:\n{stderr}"
    );
}

// `generate auth` into a legacy tree — `generate_auth_into_a_template_7_tree_verifies_with_
// pinned_none_and_the_notice` in the task plan — is NOT here, and the omission is deliberate
// rather than an oversight. `auth` is the one generate action that verifies: it copies the merged
// tree to a scratch directory and runs the five checks over it, which needs the framework
// checkout and a full starter build. That is the starter matrix's leg, and running it from this
// file would put a multi-minute build behind a test whose subject is a stderr line. The behaviour
// it would assert — `[toolchain]` `pinned = "none"` and `rust_version = "none"`, `[verified_with]`
// with `operation = "auth"`, and the FR-012-8 (1) notice ending `the project pins nothing` — is
// covered as a unit by `commands::generate::toolchain_tests::a_legacy_record_holds_the_compiler_
// to_nothing` and by `toolchain::notice::tests::a_legacy_tree_pins_nothing`.
