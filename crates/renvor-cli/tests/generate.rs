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
    renvor_with(args, directory, &[])
}

/// `renvor` with extra environment, for the failure injector.
fn renvor_with(
    args: &[&str],
    directory: &Path,
    envs: &[(&str, &str)],
) -> (bool, serde_json::Value, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(args)
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", directory.join(".target"))
        .envs(envs.iter().copied())
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

/// Every file under `root` with its bytes, sorted — the whole project, so "unchanged" means
/// unchanged and not "the files this test thought to look at".
fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("read_dir").flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".target") {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative")
                    .display()
                    .to_string();
                files.push((relative, std::fs::read(&path).expect("read")));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn a_failure_at_every_placement_boundary_leaves_the_project_byte_identical() {
    // STANDARDS AXIS (P2), and the rule the correction round made explicit: a generation into an
    // existing project either lands whole or leaves the tree exactly as it found it. The commit
    // stages every file as a temporary sibling, then renames each into place, then rewrites the
    // record; a failure is injected after EVERY one of those steps — after each staged file,
    // after each placed file, and before the record — and the whole project is compared byte for
    // byte each time, the record and the migration directory included. `RENVOR_FAIL_AT` is
    // honoured by debug builds only, which `cargo test` is.
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "mysql");
    for (label, args) in [
        (
            "a migration pair",
            vec!["generate", "migration", "add_index", "--output", "json"],
        ),
        (
            "the auth set",
            vec![
                "generate",
                "migration",
                "--import",
                "auth",
                "--output",
                "json",
            ],
        ),
    ] {
        let before = snapshot(&project);
        let mut dry = args.clone();
        dry.push("--dry-run");
        let (ok, plan, _) = renvor(&dry, &project);
        assert!(ok, "{plan}");
        let files = plan["result"]["files"].as_array().expect("files").len();
        assert!(files >= 2, "{label}: {plan}");
        let mut boundaries: Vec<String> = Vec::with_capacity(2 * files + 1);
        for index in 0..files {
            boundaries.push(format!("generate-stage-{index}"));
        }
        for index in 0..files {
            boundaries.push(format!("generate-place-{index}"));
        }
        boundaries.push("generate-record".to_owned());
        for step in &boundaries {
            let (ok, document, _) =
                renvor_with(&args, &project, &[("RENVOR_FAIL_AT", step.as_str())]);
            assert!(
                !ok,
                "{label}: the injected failure at `{step}` was not reported: {document}"
            );
            assert_eq!(document["status"], "failure", "{document}");
            assert_eq!(
                document["error"]["details"]["injected"],
                step.as_str(),
                "{document}"
            );
            assert_eq!(
                snapshot(&project),
                before,
                "{label}: a failure at `{step}` left the project changed"
            );
        }
        // POSITIVE CONTROL: the same command without the injector lands whole.
        let (ok, document, _) = renvor(&args, &project);
        assert!(ok, "{document}");
        assert_eq!(document["result"]["written"], files);
        assert_ne!(snapshot(&project), before);
    }
}

#[test]
fn an_import_refuses_a_version_another_migration_already_holds() {
    // FOUND BY THE CODEX REVIEW (P1). An imported set was checked against files of the same
    // name only; a user's migration holding one of the set's versions under another name made
    // two files share a version, which SQLx's ledger cannot represent.
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "mysql");
    let mine = "20260901000001_mine.up.sql";
    std::fs::write(
        project.join("migrations").join(mine),
        "-- the user's own migration, versioned by hand\n",
    )
    .expect("write");
    let before = snapshot(&project);
    let record_before = record(&project);
    let (ok, refused, _) = renvor(
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
    assert!(!ok, "the collision was not refused: {refused}");
    assert_eq!(refused["error"]["code"], "generation_conflict", "{refused}");
    assert_eq!(refused["error"]["details"]["reason"], "version_present");
    assert_eq!(refused["error"]["details"]["versions"], "20260901000001");
    assert_eq!(snapshot(&project), before, "a refusal wrote something");
    assert_eq!(
        record(&project),
        record_before,
        "a refusal must leave the provenance record as it was"
    );
}

#[test]
fn two_names_generated_within_a_second_get_distinct_versions() {
    // Not flaky in either direction: when the two commands straddle a second boundary the
    // versions differ anyway, and when they do not the allocator moves the second one forward.
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "postgres");
    let (ok, first, _) = renvor(
        &["generate", "migration", "add_a", "--output", "json"],
        &project,
    );
    assert!(ok, "{first}");
    let (ok, second, _) = renvor(
        &["generate", "migration", "add_b", "--output", "json"],
        &project,
    );
    assert!(ok, "{second}");
    let version = |document: &serde_json::Value| {
        document["result"]["files"][0]["path"]
            .as_str()
            .expect("path")["migrations/".len()..][..14]
            .to_owned()
    };
    assert_ne!(version(&first), version(&second));
    // Each pair's up and down carry the same version.
    for document in [&first, &second] {
        let paths: Vec<&str> = document["result"]["files"]
            .as_array()
            .expect("files")
            .iter()
            .map(|f| f["path"].as_str().expect("path"))
            .collect();
        assert_eq!(paths.len(), 2, "{document}");
        assert_eq!(
            &paths[0][..24],
            &paths[1][..24],
            "one version per pair: {paths:?}"
        );
        assert!(paths[0].ends_with(".up.sql") && paths[1].ends_with(".down.sql"));
    }
    // Every version in the directory is held by exactly one pair.
    let names = migration_files(&project);
    let mut versions: Vec<String> = names
        .iter()
        .filter(|name| name.ends_with(".up.sql"))
        .map(|name| name.split('_').next().expect("a version").to_owned())
        .collect();
    let count = versions.len();
    versions.dedup();
    assert_eq!(versions.len(), count, "a version is shared: {names:?}");
}

#[test]
fn a_name_beside_an_import_is_refused_rather_than_ignored() {
    // FOUND BY THE CODEX REVIEW (P2). `generate migration add_users --import auth` imported the
    // set and reported success, discarding the name — different work from what the positional
    // argument asked for. The two are declared as conflicting, so the parser refuses them
    // together before anything is planned.
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "postgres");
    let before = snapshot(&project);
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args([
            "generate",
            "migration",
            "add_users",
            "--import",
            "auth",
            "--output",
            "json",
        ])
        .current_dir(&project)
        .output()
        .expect("renvor runs");
    assert_eq!(
        output.status.code(),
        Some(2),
        "a usage error exits 2:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // In JSON mode the refusal is the envelope on stdout; either stream must name the flag.
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        said.contains("--import"),
        "the refusal names the conflicting flag:\n{said}"
    );
    assert_eq!(snapshot(&project), before, "a usage error wrote something");
}

#[test]
fn the_record_digests_are_the_placed_files_including_the_resolved_lockfile() {
    // FOUND BY THE CODEX REVIEW (P2). The record was written before verification, and
    // verification is what resolves `Cargo.lock`; the placed lockfile therefore never matched
    // the record — for a starter the seeded framework lock was pruned, for a skeleton the file
    // did not exist yet — so the first `renvor generate` after `renvor new` read the lockfile as
    // changed by the user. The record is written after verification and describes the tree
    // that is placed.
    use sha2::{Digest as _, Sha256};
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "postgres");
    let record: toml::Value = toml::from_str(&record(&project)).expect("the record parses");
    let files = record["file"].as_array().expect("[[file]]");
    let mut listed = Vec::new();
    for file in files {
        let path = file["path"].as_str().expect("path");
        let bytes = std::fs::read(project.join(path))
            .unwrap_or_else(|error| panic!("`{path}` is recorded but not placed: {error}"));
        let digest = Sha256::digest(&bytes)
            .iter()
            .fold(String::new(), |acc, b| format!("{acc}{b:02x}"));
        assert_eq!(
            file["sha256"].as_str(),
            Some(digest.as_str()),
            "`{path}`: the record's digest is not the placed file's"
        );
        listed.push(path.to_owned());
    }
    assert!(
        listed.iter().any(|path| path == "Cargo.lock"),
        "the resolved lockfile is generated and must be recorded: {listed:?}"
    );
}

/// `renvor` in human mode: the exit code and both streams joined.
fn renvor_human(args: &[&str], directory: &Path) -> (Option<i32>, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(args)
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", directory.join(".target"))
        .output()
        .expect("renvor runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.code(), said)
}

/// Makes `path` read as generated with `bytes`: writes them and records their digest — the state
/// a project is in when an earlier `renvor` wrote a different render of the same file and nobody
/// touched it since. The record is what says so; nothing else does.
fn mark_as_generated(project: &Path, path: &str, bytes: &[u8]) {
    use sha2::{Digest as _, Sha256};
    std::fs::write(project.join(path), bytes).expect("write");
    let digest = Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |acc, b| format!("{acc}{b:02x}"));
    let record_path = project.join(".renvor/generated.toml");
    let text = std::fs::read_to_string(&record_path).expect("the record");
    let needle = format!("path = {path:?}\nsha256 = \"");
    let at = text
        .find(&needle)
        .unwrap_or_else(|| panic!("`{path}` is not in the record:\n{text}"));
    let start = at + needle.len();
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..start]);
    out.push_str(&digest);
    out.push_str(&text[start + 64..]);
    std::fs::write(&record_path, out).expect("rewrite");
}

/// The paths whose bytes differ between two snapshots of the same file set.
fn differing(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) -> Vec<String> {
    assert_eq!(
        before.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        after.iter().map(|(p, _)| p).collect::<Vec<_>>(),
        "the file set changed"
    );
    before
        .iter()
        .zip(after)
        .filter(|(b, a)| b.1 != a.1)
        .map(|(b, _)| b.0.clone())
        .collect()
}

fn action_of(document: &serde_json::Value, path: &str) -> Option<String> {
    document["result"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .find(|f| f["path"] == path)
        .map(|f| f["action"].as_str().unwrap_or("").to_owned())
}

#[test]
fn a_regenerable_file_is_refused_without_the_flag_and_replaced_only_with_it() {
    // FR-048 AS DECIDED (2026-09-05, the maintainer's option 3). A file that differs from the
    // render but is unchanged since generation — its digest is the recorded one — is
    // *regenerable*: reported, and replaced only under `--overwrite-unchanged`. Without the flag
    // the whole run is refused, names the flag, and writes nothing; with it that file is
    // replaced and nothing else moves. A dry run classifies exactly as the real run does.
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "postgres");
    let args = ["generate", "migration", "add_index", "--output", "json"];
    let (ok, first, _) = renvor(&args, &project);
    assert!(ok, "{first}");
    let up = first["result"]["files"][0]["path"]
        .as_str()
        .expect("path")
        .to_owned();
    let render = std::fs::read(project.join(&up)).expect("the render");
    // An earlier renvor wrote a different render of the same file, and recorded it.
    mark_as_generated(
        &project,
        &up,
        b"-- add_index: an older render, generator-owned and never touched\n",
    );
    let before = snapshot(&project);

    let (ok, refused, stderr) = renvor(&args, &project);
    assert!(
        !ok,
        "a regenerable file was replaced without the flag: {refused}"
    );
    assert_eq!(refused["error"]["code"], "generation_conflict", "{refused}");
    let details = &refused["error"]["details"];
    assert_eq!(details["reason"], "overwrite_required", "{refused}");
    assert_eq!(details["flag"], "--overwrite-unchanged", "{refused}");
    assert_eq!(details["regenerable"], up, "{refused}");
    assert_eq!(details["paths"], up, "{refused}");
    assert_eq!(details["count"], "1", "{refused}");
    assert!(
        details.get("changed").is_none(),
        "nothing was changed by the user: {refused}"
    );
    assert_eq!(snapshot(&project), before, "a refusal wrote something");
    // Paths, never contents: no line of either version of the file reaches either stream.
    let said = format!("{refused}{stderr}");
    assert!(!said.contains("older render"), "{said}");
    assert!(!said.contains("Applied on Boot"), "{said}");

    // Human output names the flag and the path the same way, and prints no contents either.
    let (code, human) = renvor_human(&["generate", "migration", "add_index"], &project);
    assert_eq!(code, Some(3), "{human}");
    assert!(human.contains("--overwrite-unchanged"), "{human}");
    assert!(human.contains(&up), "{human}");
    assert!(!human.contains("older render"), "{human}");
    assert!(!human.contains("Applied on Boot"), "{human}");
    assert_eq!(snapshot(&project), before);

    // A dry run is the same classification: the same refusal, the same details.
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
    assert!(!ok, "{dry}");
    assert_eq!(
        dry["error"], refused["error"],
        "a dry run classified differently from the real run"
    );
    assert_eq!(snapshot(&project), before);

    // With the flag: the dry run reports the plan, writes nothing; the real run applies it.
    let with = [
        "generate",
        "migration",
        "add_index",
        "--overwrite-unchanged",
        "--output",
        "json",
    ];
    let mut dry_with = with.to_vec();
    dry_with.push("--dry-run");
    let (ok, planned, _) = renvor(&dry_with, &project);
    assert!(ok, "{planned}");
    assert_eq!(planned["result"]["dryRun"], true);
    assert_eq!(planned["result"]["written"], 0);
    assert_eq!(snapshot(&project), before, "a dry run wrote");
    let (ok, done, _) = renvor(&with, &project);
    assert!(ok, "{done}");
    assert_eq!(done["result"]["written"], 1);
    assert_eq!(
        done["result"]["files"], planned["result"]["files"],
        "the dry run's classification is not the real plan's"
    );
    assert_eq!(action_of(&done, &up).as_deref(), Some("regenerate"));
    assert_eq!(
        std::fs::read(project.join(&up)).expect("read"),
        render,
        "the regenerable file is replaced by the render"
    );
    let after = snapshot(&project);
    assert_eq!(
        differing(&before, &after),
        [".renvor/generated.toml".to_owned(), up.clone()],
        "only the regenerated file and the record moved"
    );

    // Idempotent: again with the flag, and again without it, both write nothing.
    for again in [with.as_slice(), args.as_slice()] {
        let (ok, document, _) = renvor(again, &project);
        assert!(ok, "{document}");
        assert_eq!(document["result"]["written"], 0, "{document}");
        assert!(
            document["result"]["files"]
                .as_array()
                .expect("files")
                .iter()
                .all(|f| f["action"] == "unchanged"),
            "{document}"
        );
        assert_eq!(snapshot(&project), after);
    }
}

#[test]
fn a_changed_file_is_refused_with_the_flag_and_a_mixed_plan_writes_nothing() {
    // FR-048 AS DECIDED: `--overwrite-unchanged` replaces only what is unchanged since
    // generation. A file the user changed refuses the run with or without the flag, and a plan
    // that mixes a file to create, one to regenerate, and one that conflicts writes none of them
    // — not even the one that could have been created. No output carries the user's contents.
    let base = tempfile::tempdir().expect("tempdir");
    let project = project(base.path(), "mysql");
    let import = [
        "generate",
        "migration",
        "--import",
        "auth",
        "--output",
        "json",
    ];
    let (ok, first, _) = renvor(&import, &project);
    assert!(ok, "{first}");
    let paths: Vec<String> = first["result"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f["path"].as_str().expect("path").to_owned())
        .collect();
    assert_eq!(paths.len(), 16, "{first}");
    // A whole pair is removed (a lone half would trip the version check first); the next two
    // files, of other versions, are the regenerable and the changed one.
    let version = |path: &str| {
        path.strip_prefix("migrations/")
            .and_then(|name| name.split('_').next())
            .expect("a version")
            .to_owned()
    };
    let gone = version(&paths[0]);
    let absent: Vec<&String> = paths.iter().filter(|p| version(p) == gone).collect();
    assert_eq!(absent.len(), 2, "{paths:?}");
    let mut rest = paths.iter().filter(|p| version(p) != gone);
    let regenerable = rest.next().expect("a second version");
    let changed = rest
        .find(|p| version(p) != version(regenerable))
        .expect("a third version");
    let original = std::fs::read(project.join(changed)).expect("read");
    for path in &absent {
        std::fs::remove_file(project.join(path)).expect("remove");
    }
    mark_as_generated(
        &project,
        regenerable,
        b"-- an older render of this migration, never touched\n",
    );
    let canary = "-- CANARY-7f3a: the user's own statement, which must never be printed\n";
    std::fs::write(project.join(changed), canary).expect("write");
    let before = snapshot(&project);

    let mut with = import.to_vec();
    with.push("--overwrite-unchanged");
    let mut dry = with.clone();
    dry.push("--dry-run");
    // Without the flag both refuse — the regenerable one beside the changed one, the flag named;
    // with it only the changed one refuses, and the flag is not what is missing.
    for (label, args, flagged) in [
        ("without the flag", import.as_slice(), false),
        ("with the flag", with.as_slice(), true),
        ("with the flag, dry", dry.as_slice(), true),
    ] {
        let (ok, refused, stderr) = renvor(args, &project);
        assert!(
            !ok,
            "{label}: a changed file did not refuse the run: {refused}"
        );
        assert_eq!(
            refused["error"]["code"], "generation_conflict",
            "{label}: {refused}"
        );
        let details = &refused["error"]["details"];
        assert_eq!(
            details["reason"], "changed_since_generation",
            "{label}: {refused}"
        );
        assert_eq!(details["changed"], changed.as_str(), "{label}: {refused}");
        if flagged {
            assert!(details.get("regenerable").is_none(), "{label}: {refused}");
            assert!(details.get("flag").is_none(), "{label}: {refused}");
            assert_eq!(details["paths"], changed.as_str(), "{label}: {refused}");
            assert_eq!(details["count"], "1", "{label}: {refused}");
        } else {
            assert_eq!(
                details["regenerable"],
                regenerable.as_str(),
                "{label}: {refused}"
            );
            assert_eq!(
                details["flag"], "--overwrite-unchanged",
                "{label}: {refused}"
            );
            assert_eq!(
                details["paths"],
                format!("{regenerable}, {changed}"),
                "{label}: {refused}"
            );
            assert_eq!(details["count"], "2", "{label}: {refused}");
        }
        assert_eq!(
            snapshot(&project),
            before,
            "{label}: a refusal wrote something"
        );
        assert!(
            absent.iter().all(|path| !project.join(path).exists()),
            "{label}: a file that could have been created was created"
        );
        let said = format!("{refused}{stderr}");
        assert!(
            !said.contains("CANARY-7f3a"),
            "{label}: contents leaked:\n{said}"
        );
        assert!(
            !said.contains("older render"),
            "{label}: contents leaked:\n{said}"
        );
    }
    let (code, human) = renvor_human(
        &[
            "generate",
            "migration",
            "--import",
            "auth",
            "--overwrite-unchanged",
        ],
        &project,
    );
    assert_eq!(code, Some(3), "{human}");
    assert!(human.contains(changed.as_str()), "{human}");
    assert!(!human.contains("CANARY-7f3a"), "{human}");
    assert_eq!(snapshot(&project), before);

    // The flag never waived the changed file; with that file back as generated, the plan lands
    // whole: the absent one created, the regenerable one replaced, the restored one untouched.
    std::fs::write(project.join(changed), &original).expect("restore");
    let (ok, done, _) = renvor(&with, &project);
    assert!(ok, "{done}");
    assert_eq!(done["result"]["written"], 3, "{done}");
    for path in &absent {
        assert_eq!(action_of(&done, path).as_deref(), Some("write"), "{done}");
    }
    assert_eq!(action_of(&done, regenerable).as_deref(), Some("regenerate"));
    assert_eq!(action_of(&done, changed).as_deref(), Some("unchanged"));
    let (ok, again, _) = renvor(&import, &project);
    assert!(ok, "{again}");
    assert_eq!(again["result"]["written"], 0, "{again}");
}
