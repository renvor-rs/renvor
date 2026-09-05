//! `renvor generate migration`, driven through the binary against a real generated project
//! (Phase 011, FR-046 and FR-048).
//!
//! # Why a skeleton
//!
//! A migration pair and an imported set are files under `migrations/`; they need a database in
//! `renvor.toml` and nothing from the framework. The dependency-free skeleton generates in
//! seconds, so every case here runs on every `cargo test`, while the starter-only commands are
//! proven under the starter matrix's own gate.

use std::path::{Path, PathBuf};
use std::process::Command;

fn renvor(args: &[&str], directory: &Path) -> (bool, serde_json::Value, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(args)
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", directory.join(".target"))
        .output()
        .expect("renvor runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let document = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("not a JSON envelope:\n{stdout}\n{stderr}"));
    (output.status.success(), document, stderr)
}

/// A skeleton with a database, so `migrations/` exists and `renvor.toml` records the engine.
fn project(base: &Path, database: &str) -> PathBuf {
    let (ok, document, stderr) = renvor(
        &[
            "new",
            "demo",
            "--database",
            database,
            "--example-domain",
            "--output",
            "json",
            "--yes",
        ],
        base,
    );
    assert!(ok, "generation failed: {document}\n{stderr}");
    base.join("demo")
}

fn migration_files(project: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(project.join("migrations"))
        .expect("migrations/")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn record(project: &Path) -> String {
    std::fs::read_to_string(project.join(".renvor/generated.toml")).expect("the record")
}

#[test]
fn a_migration_pair_is_written_once_and_a_rerun_is_a_no_op() {
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "postgres");
    let before = migration_files(&project);
    assert!(!record(&project).contains("add_index"));

    let (ok, document, stderr) = renvor(
        &["generate", "migration", "add_index", "--output", "json"],
        &project,
    );
    assert!(ok, "{document}\n{stderr}");
    assert_eq!(document["status"], "success");
    assert_eq!(document["result"]["written"], 2);
    let files = document["result"]["files"].as_array().expect("files");
    assert_eq!(files.len(), 2);
    for file in files {
        assert_eq!(file["action"], "write");
        let path = file["path"].as_str().expect("a path");
        assert!(
            path.starts_with("migrations/") && path.contains("_add_index."),
            "{path}"
        );
        let version = &path["migrations/".len().."migrations/".len() + 14];
        assert!(
            version.len() == 14 && version.bytes().all(|b| b.is_ascii_digit()),
            "the version is a 14-digit UTC instant: {path}"
        );
        assert!(project.join(path).is_file());
    }
    let after = migration_files(&project);
    assert_eq!(after.len(), before.len() + 2);
    assert!(
        record(&project).contains("_add_index.up.sql"),
        "the provenance record lists what was generated"
    );

    // The same command again finds the pair it wrote — no second pair, no change.
    let (ok, again, _) = renvor(
        &["generate", "migration", "add_index", "--output", "json"],
        &project,
    );
    assert!(ok, "{again}");
    assert_eq!(again["result"]["written"], 0);
    assert!(
        again["result"]["files"]
            .as_array()
            .expect("files")
            .iter()
            .all(|file| file["action"] == "unchanged")
    );
    assert_eq!(
        migration_files(&project),
        after,
        "a rerun stacked a second pair"
    );
}

#[test]
fn a_file_changed_since_generation_is_a_conflict_that_writes_nothing() {
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "postgres");
    let (ok, first, _) = renvor(
        &["generate", "migration", "widen_name", "--output", "json"],
        &project,
    );
    assert!(ok, "{first}");
    let up = first["result"]["files"][0]["path"]
        .as_str()
        .expect("path")
        .to_owned();
    let down = first["result"]["files"][1]["path"]
        .as_str()
        .expect("path")
        .to_owned();
    // The user writes their migration into the up file — which is the point of the file.
    std::fs::write(
        project.join(&up),
        "ALTER TABLE item ALTER COLUMN name TYPE VARCHAR(400);\n",
    )
    .expect("write");
    // Remove the down file, so a rerun WOULD have something to write — and must not.
    std::fs::remove_file(project.join(&down)).expect("remove");

    let (ok, document, _) = renvor(
        &["generate", "migration", "widen_name", "--output", "json"],
        &project,
    );
    assert!(!ok, "a changed file was overwritten: {document}");
    assert_eq!(
        document["error"]["code"], "generation_conflict",
        "{document}"
    );
    assert_eq!(document["error"]["details"]["paths"], up, "{document}");
    assert_eq!(document["error"]["details"]["count"], "1");
    assert!(
        !project.join(&down).exists(),
        "a conflict must write nothing, not even the file that could have been written"
    );
    assert_eq!(
        std::fs::read_to_string(project.join(&up)).expect("read"),
        "ALTER TABLE item ALTER COLUMN name TYPE VARCHAR(400);\n",
        "the user's file was touched"
    );
}

#[test]
fn the_framework_sets_import_byte_for_byte_and_compose_in_one_directory() {
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "mysql");
    let (ok, auth, stderr) = renvor(
        &[
            "generate",
            "migration",
            "--import",
            "auth",
            "--output",
            "json",
        ],
        &project,
    );
    assert!(ok, "{auth}\n{stderr}");
    assert_eq!(
        auth["result"]["written"], 16,
        "MySQL's auth set is eight pairs"
    );
    let (ok, jobs, _) = renvor(
        &[
            "generate",
            "migration",
            "--import",
            "jobs",
            "--output",
            "json",
        ],
        &project,
    );
    assert!(ok, "{jobs}");
    assert_eq!(
        jobs["result"]["written"], 10,
        "the job store's set is five pairs"
    );
    let names = migration_files(&project);
    assert!(names.iter().any(|name| name.starts_with("20260901")));
    assert!(names.iter().any(|name| name.starts_with("20260904")));
    assert!(names.contains(&"0001_create_item.up.sql".to_owned()));
    // Byte for byte: the file the project holds is the file the crate embeds.
    let set = renvor_jobs::migrations::for_engine("mysql").expect("mysql");
    for file in set.files() {
        assert_eq!(
            std::fs::read_to_string(project.join("migrations").join(file.name())).expect("read"),
            file.contents()
        );
    }
    // Importing again is a no-op; a set is never duplicated.
    let (ok, again, _) = renvor(
        &[
            "generate",
            "migration",
            "--import",
            "jobs",
            "--output",
            "json",
        ],
        &project,
    );
    assert!(ok, "{again}");
    assert_eq!(again["result"]["written"], 0);
    assert_eq!(migration_files(&project), names);
}

#[test]
fn a_dry_run_writes_nothing_and_a_project_without_a_database_is_refused() {
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "postgres");
    let before = migration_files(&project);
    let record_before = record(&project);
    let (ok, dry, _) = renvor(
        &[
            "generate",
            "migration",
            "add_index",
            "--dry-run",
            "--output",
            "json",
        ],
        &project,
    );
    assert!(ok, "{dry}");
    assert_eq!(dry["result"]["dryRun"], true);
    assert_eq!(dry["result"]["written"], 0);
    assert_eq!(dry["result"]["files"].as_array().expect("files").len(), 2);
    assert_eq!(
        migration_files(&project),
        before,
        "a dry run wrote a migration"
    );
    assert_eq!(
        record(&project),
        record_before,
        "a dry run touched the record"
    );

    // No database: nothing to migrate, and the refusal says why.
    let (ok, document, stderr) =
        renvor(&["new", "plain", "--output", "json", "--yes"], base.path());
    assert!(ok, "{document}\n{stderr}");
    let (ok, refused, _) = renvor(
        &["generate", "migration", "add_index", "--output", "json"],
        &base.path().join("plain"),
    );
    assert!(!ok);
    assert_eq!(
        refused["error"]["code"], "unsupported_combination",
        "{refused}"
    );
    assert_eq!(refused["error"]["details"]["reason"], "no_database");

    // An unknown set, and a name outside the grammar, are refused by name.
    let (ok, unknown, _) = renvor(
        &[
            "generate",
            "migration",
            "--import",
            "s3",
            "--output",
            "json",
        ],
        &project,
    );
    assert!(!ok);
    assert_eq!(unknown["error"]["code"], "unsupported_value");
    assert_eq!(unknown["error"]["details"]["flag"], "--import");
    let (ok, bad, _) = renvor(
        &["generate", "migration", "Add-Index", "--output", "json"],
        &project,
    );
    assert!(!ok);
    assert_eq!(bad["error"]["code"], "unsupported_value");
}
