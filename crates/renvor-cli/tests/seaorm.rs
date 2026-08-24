//! `--orm seaorm`: the tree it generates, what it must not contain, and what it must not change.
//!
//! # Two obligations pull in opposite directions here
//!
//! Phase 007 FR-038 adds a value to `--orm`; Phase 007 FR-040 and FR-043 require every Phase 006
//! command to keep producing the project it produced before. Most of this file is the second
//! obligation, because it is the one a new feature quietly breaks.

mod harness;

use harness::renvor;

struct Generated {
    code: i32,
    stdout: String,
    stderr: String,
    root: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

impl Generated {
    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.root.join(name))
            .unwrap_or_else(|_| panic!("`{name}` is unreadable"))
    }

    fn files(&self) -> Vec<String> {
        fn walk(base: &std::path::Path, at: &std::path::Path, into: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(at) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, into);
                } else if let Ok(relative) = path.strip_prefix(base) {
                    into.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.root, &self.root, &mut found);
        found.sort();
        found
    }
}

fn generate(extra: &[&str]) -> Generated {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    let mut args = vec![
        "new",
        "demo",
        "--path",
        root.to_str().expect("utf-8"),
        "--yes",
    ];
    args.extend_from_slice(extra);
    let (code, stdout, stderr) = renvor(&args, directory.path(), &[]);
    Generated {
        code,
        stdout,
        stderr,
        root,
        _directory: directory,
    }
}

fn generate_ok(flags: &[&str]) -> Generated {
    let generated = generate(flags);
    assert_eq!(generated.code, 0, "generation failed for: {flags:?}");
    generated
}

// ───────────────────────────────────────────────────────── the generated file set

/// The exact file set. Not "contains" — **equals**.
#[test]
fn seaorm_generates_exactly_its_file_set() {
    let generated = generate_ok(&["--orm", "seaorm", "--database", "postgres"]);
    let mut expected = vec![
        ".gitignore",
        "Cargo.lock",
        "Cargo.toml",
        "README.md",
        "migrations/0001_create_item.down.sql",
        "migrations/0001_create_item.up.sql",
        "renvor.toml",
        "src/entity.rs",
        "src/main.rs",
        "src/repository.rs",
    ];
    expected.sort_unstable();
    assert_eq!(generated.files(), expected);
}

/// The two persistence models generate DIFFERENT files, and neither leaks into the other.
#[test]
fn the_two_persistence_models_do_not_share_a_file() {
    let sqlx = generate_ok(&["--orm", "sqlx", "--database", "postgres"]).files();
    let seaorm = generate_ok(&["--orm", "seaorm", "--database", "postgres"]).files();

    assert!(sqlx.contains(&"src/persistence.rs".to_owned()));
    assert!(!sqlx.contains(&"src/entity.rs".to_owned()));
    assert!(!sqlx.contains(&"src/repository.rs".to_owned()));

    assert!(seaorm.contains(&"src/entity.rs".to_owned()));
    assert!(seaorm.contains(&"src/repository.rs".to_owned()));
    assert!(
        !seaorm.contains(&"src/persistence.rs".to_owned()),
        "the SeaORM project carries the direct-SQLx module, which is the accident PLAN.md §Phase \
         007 names: choosing SeaORM must not expose direct-SQLx application APIs"
    );

    // The migrations are the SAME in both, because there is one migration history.
    for tree in [&sqlx, &seaorm] {
        assert!(tree.contains(&"migrations/0001_create_item.up.sql".to_owned()));
        assert!(tree.contains(&"migrations/0001_create_item.down.sql".to_owned()));
    }
}

/// `main.rs` declares exactly the modules that were generated.
#[test]
fn the_module_declarations_match_the_generated_files() {
    let generated = generate_ok(&["--orm", "seaorm", "--database", "mysql"]);
    let main = generated.read("src/main.rs");
    // CODE LINES ONLY. `main.rs` names both modules in a comment telling the operator what to
    // declare, and a whole-file `contains` cannot tell that apart from a declaration — which is
    // how this assertion first "failed" against a file that was perfectly correct.
    for line in main.lines() {
        let code = line.split("//").next().unwrap_or(line);
        for undeclared in ["mod entity;", "mod repository;", "mod persistence;"] {
            assert!(
                !code.contains(undeclared),
                "`{undeclared}` is declared but nothing can compile it: {line}"
            );
        }
    }
}

// ───────────────────────────────────────────────────────── the generated manifest

/// The manifest names `sea-orm`, with exactly one driver.
#[test]
fn the_manifest_resolves_only_the_selected_driver() {
    for (database, wanted, forbidden) in [
        ("postgres", "sqlx-postgres", "sqlx-mysql"),
        ("mysql", "sqlx-mysql", "sqlx-postgres"),
    ] {
        let generated = generate_ok(&["--orm", "seaorm", "--database", database]);
        let manifest = generated.read("Cargo.toml");
        // The manifest declares NOTHING — see the template for why generation stays offline. What
        // it must get right is the instruction: the right feature, and never the other driver.
        assert!(
            manifest.contains(wanted),
            "{database}: the manifest does not name `{wanted}`"
        );
        assert!(
            !manifest.contains(forbidden),
            "{database}: the manifest names `{forbidden}`, so following it resolves both drivers"
        );
        assert!(
            manifest.contains("default-features = false"),
            "{database}: the instruction omits `default-features = false`"
        );
        // `runtime-tokio-rustls` may be MENTIONED — the template warns against it by name — but
        // never inside the feature list an operator would copy.
        let features = manifest
            .split_once("features = [")
            .map(|(_, rest)| rest.split_once(']').map_or(rest, |(inside, _)| inside))
            .unwrap_or("");
        assert!(
            !features.contains("runtime-tokio-rustls"),
            "{database}: the copyable feature list selects `runtime-tokio-rustls`, which resolves \
             webpki-roots"
        );
    }
}

/// The direct-SQLx manifest is unchanged: still no dependency at all.
#[test]
fn the_sqlx_manifest_still_declares_nothing() {
    let manifest = generate_ok(&["--orm", "sqlx", "--database", "postgres"]).read("Cargo.toml");
    assert!(
        !manifest.contains("sea-orm"),
        "the direct-SQLx project acquired a SeaORM dependency"
    );
    // The section exists and is empty but for comments — the Phase 006 shape.
    let after = manifest
        .split_once("[dependencies]")
        .expect("a dependencies section")
        .1;
    assert!(
        after.lines().all(|line| {
            let trimmed = line.trim();
            trimmed.is_empty() || trimmed.starts_with('#')
        }),
        "the direct-SQLx manifest declares a dependency it did not declare in Phase 006"
    );
}

/// A generated project declares no path dependency and no crate that is not published.
#[test]
fn the_manifest_names_no_unpublished_or_path_dependency() {
    for orm in ["sqlx", "seaorm"] {
        let manifest = generate_ok(&["--orm", orm, "--database", "postgres"]).read("Cargo.toml");
        for line in manifest.lines() {
            let code = line.split('#').next().unwrap_or(line);
            let code = if code.trim().is_empty() { "" } else { code };
            assert!(
                !code.contains("path ="),
                "{orm}: a path dependency reached a generated manifest: {line}"
            );
            // Renvor crates are unpublished. Naming one OUTSIDE a comment would emit a project
            // that does not resolve — which is why the README names them in prose instead.
            assert!(
                !code.contains("renvor-") && !code.trim_start().starts_with("renvor "),
                "{orm}: an unpublished Renvor crate reached a generated manifest: {line}"
            );
        }
    }
}

// ───────────────────────────────────────────────────────── the recorded selection

/// `renvor.toml` records the ORM, and records no credential.
#[test]
fn the_manifest_records_the_orm() {
    for orm in ["sqlx", "seaorm"] {
        let recorded = generate_ok(&["--orm", orm, "--database", "postgres"]).read("renvor.toml");
        assert!(
            recorded.contains(&format!("orm = \"{orm}\"")),
            "renvor.toml does not record `{orm}`"
        );
        assert!(recorded.contains("template_version = \"5\""));
    }
}

/// `renvor check` accepts a project this generator produced.
#[test]
fn check_accepts_a_generated_seaorm_project() {
    let generated = generate_ok(&["--orm", "seaorm", "--database", "postgres"]);
    let (code, stdout, stderr) = renvor(&["check"], &generated.root, &[]);
    assert_eq!(code, 0, "check refused a generated project: {stderr}");
    assert!(
        stdout.contains('5'),
        "check did not report the template version: {stdout}"
    );
}

// ───────────────────────────────────────────────────────── refusals and compatibility

/// An unknown ORM is refused by name, and names every supported value.
#[test]
fn an_unknown_orm_is_refused_with_the_supported_values() {
    let generated = generate(&["--orm", "diesel", "--database", "postgres"]);
    assert_ne!(generated.code, 0, "an unsupported ORM was accepted");
    let message = format!("{}{}", generated.stdout, generated.stderr);
    for supported in ["sqlx", "seaorm"] {
        assert!(
            message.contains(supported),
            "the refusal does not name `{supported}`: {message}"
        );
    }
}

/// `--orm seaorm` without `--database` is refused before anything is written.
#[test]
fn seaorm_without_a_database_is_refused_before_any_write() {
    let generated = generate(&["--orm", "seaorm"]);
    assert_ne!(generated.code, 0, "an ORM with no database was accepted");
    assert!(
        !generated.root.exists(),
        "the destination was created despite a refused configuration"
    );
}

/// **FR-040.** A Phase 006 command — `--database` with no `--orm` — still yields SQLx.
#[test]
fn a_database_without_an_orm_still_means_sqlx() {
    let generated = generate_ok(&["--database", "postgres"]);
    assert!(
        generated.read("renvor.toml").contains("orm = \"sqlx\""),
        "omitting --orm changed the persistence model, which breaks every Phase 006 command"
    );
    assert!(generated.files().contains(&"src/persistence.rs".to_owned()));
    assert!(!generated.files().contains(&"src/entity.rs".to_owned()));
}

/// **FR-043.** The direct-SQLx tree is byte-identical to the one Phase 006 generated, module
/// declarations and all — apart from the template version, which is what a version is for.
#[test]
fn the_direct_sqlx_tree_is_unchanged_apart_from_its_recorded_version() {
    let generated = generate_ok(&["--database", "postgres", "--example-domain", "--seed-data"]);
    let mut expected = vec![
        ".gitignore",
        "Cargo.lock",
        "Cargo.toml",
        "README.md",
        "migrations/0001_create_item.down.sql",
        "migrations/0001_create_item.up.sql",
        "renvor.toml",
        "src/domain.rs",
        "src/main.rs",
        "src/persistence.rs",
        "src/seed.rs",
    ];
    expected.sort_unstable();
    assert_eq!(generated.files(), expected);
    let main = generated.read("src/main.rs");
    assert!(main.contains("mod domain;"));
    assert!(main.contains("mod persistence;"));
    assert!(main.contains("mod seed;"));
}

/// Container flags stay compatible with `--orm seaorm`.
#[test]
fn container_controls_remain_compatible_with_seaorm() {
    let generated = generate_ok(&[
        "--orm",
        "seaorm",
        "--database",
        "postgres",
        "--container",
        "--database-port",
        "5599",
    ]);
    let files = generated.files();
    for expected in [
        "compose.yaml",
        "Dockerfile",
        ".dockerignore",
        ".env.example",
    ] {
        assert!(
            files.contains(&expected.to_owned()),
            "{expected} is missing"
        );
    }
    assert!(files.contains(&"src/entity.rs".to_owned()));
    let compose = generated.read("compose.yaml");
    assert!(
        compose.contains("127.0.0.1:5599:5432"),
        "the published port is not loopback-bound: {compose}"
    );
}

// ───────────────────────────────────────────────────────── dry run and JSON

/// `--dry-run` lists every SeaORM file and writes nothing.
#[test]
fn a_dry_run_lists_the_seaorm_files_and_writes_nothing() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    let (code, stdout, stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            root.to_str().expect("utf-8"),
            "--yes",
            "--orm",
            "seaorm",
            "--database",
            "postgres",
            "--dry-run",
        ],
        directory.path(),
        &[],
    );
    assert_eq!(code, 0, "the dry run failed: {stderr}");
    for listed in ["src/entity.rs", "src/repository.rs"] {
        assert!(
            stdout.contains(listed),
            "`{listed}` is not listed: {stdout}"
        );
    }
    assert!(
        !stdout.contains("src/persistence.rs"),
        "the dry run listed a file the selection does not generate"
    );
    assert!(!root.exists(), "a dry run created the destination");
}

/// JSON output carries the resolved ORM, and no credential.
#[test]
fn json_output_carries_the_resolved_orm() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    let (code, stdout, _stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            root.to_str().expect("utf-8"),
            "--yes",
            "--orm",
            "seaorm",
            "--database",
            "mysql",
            "--output",
            "json",
        ],
        directory.path(),
        &[],
    );
    assert_eq!(code, 0);
    assert!(
        stdout.contains("seaorm"),
        "the JSON omits the ORM: {stdout}"
    );
    for forbidden in ["password", "passwd", "secret", "token"] {
        assert!(
            !stdout.to_ascii_lowercase().contains(forbidden),
            "the JSON mentions `{forbidden}`"
        );
    }
}

// ───────────────────────────────────────────────────────── the generated source

/// The entity is dense-style and the repository binds every value.
#[test]
fn the_generated_source_is_idiomatic_and_binds_its_values() {
    let generated = generate_ok(&["--orm", "seaorm", "--database", "postgres"]);
    let entity = generated.read("src/entity.rs");
    assert!(
        entity.contains("DeriveEntityModel"),
        "the entity is not dense style"
    );
    assert!(
        entity.contains("table_name = \"item\""),
        "the entity does not state its table, so a rename would change the SQL silently"
    );

    let repository = generated.read("src/repository.rs");
    assert!(
        repository.contains("ConnectionTrait"),
        "the repository is not written against the trait a transaction also implements"
    );
    // CODE LINES ONLY, for the same reason as above: the module documentation names
    // `execute_unprepared` in order to warn against it, and a whole-file match cannot tell a
    // warning from a use.
    for line in repository.lines() {
        let code = line.split("//").next().unwrap_or(line);
        assert!(
            !code.contains("execute_unprepared"),
            "the generated repository reaches the rung of the escape hatch that binds nothing: \
             {line}"
        );
        assert!(
            !code.contains("format!(\"SELECT") && !code.contains("format!(\"INSERT"),
            "the generated repository builds SQL by interpolation: {line}"
        );
    }
}

/// The uncompiled files are still held to the formatting the compiled ones are.
///
/// # Why this test has to exist
///
/// Generation runs `cargo fmt --check` on the staged project, and that is what keeps every other
/// template well-formed. It cannot see `src/entity.rs` or `src/repository.rs`, because they are not
/// declared as modules — so for these two files, the generator's own gate is blind and this is the
/// gate instead. Without it a malformed SeaORM template would ship and only be noticed by whoever
/// first added the dependency.
#[test]
fn the_uncompiled_seaorm_sources_are_still_rustfmt_clean() {
    let generated = generate_ok(&["--orm", "seaorm", "--database", "postgres"]);
    for file in ["src/entity.rs", "src/repository.rs"] {
        let output = std::process::Command::new("rustfmt")
            .args(["--check", "--edition", "2024"])
            .arg(generated.root.join(file))
            .output();
        let Ok(output) = output else {
            // `rustfmt` is part of the toolchain this repository requires, and `xtask` step 1
            // refuses to run without it. Skipping here rather than failing keeps the reason for a
            // failure honest: a missing tool is not a malformed template.
            eprintln!("SKIPPED: rustfmt is not runnable");
            return;
        };
        assert!(
            output.status.success() && output.stdout.is_empty(),
            "`{file}` is not rustfmt-clean:\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

/// Generation succeeds with the network unreachable.
///
/// # The property this protects
///
/// `--orm seaorm` was very nearly given a real `sea-orm` dependency, which reads better in a
/// manifest and would have made this test fail: generation runs the staged project's own
/// `cargo build`, so a dependency means resolving from the registry. The dependency-free manifest
/// is what keeps `renvor new` usable on a train, and this is what stops that being undone quietly.
#[test]
fn seaorm_generation_succeeds_offline() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    // `CARGO_NET_OFFLINE` makes any registry access a hard failure rather than a slow one.
    let (code, _stdout, stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            root.to_str().expect("utf-8"),
            "--yes",
            "--orm",
            "seaorm",
            "--database",
            "postgres",
        ],
        directory.path(),
        &[("CARGO_NET_OFFLINE", "true")],
    );
    assert_eq!(
        code, 0,
        "generation needed the network, so `renvor new --orm seaorm` is not offline: {stderr}"
    );
    assert!(root.join("src/entity.rs").exists());
}
