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
