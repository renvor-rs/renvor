//! `--orm seaorm`: the tree it generates, what it must not contain, and what it must not change.
//!
//! # Two obligations pull in opposite directions here
//!
//! Phase 007 FR-038 adds a value to `--orm`; Phase 007 FR-040 and FR-043 require every Phase 006
//! command to keep producing the project it produced before. Most of this file is the second
//! obligation, because it is the one a new feature quietly breaks.

mod harness;

use harness::renvor;

/// The template version this file expects generation to record.
///
/// One constant rather than a literal per assertion: a bump has to be a deliberate edit in one
/// place, and two assertions that disagree about the version would let one of them rot silently.
const TEMPLATE_VERSION: &str = "7";

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
        assert!(recorded.contains(&format!("template_version = \"{TEMPLATE_VERSION}\"")));
    }
}

/// `renvor check` accepts a project this generator produced.
#[test]
fn check_accepts_a_generated_seaorm_project() {
    let generated = generate_ok(&["--orm", "seaorm", "--database", "postgres"]);
    let (code, stdout, stderr) = renvor(&["check"], &generated.root, &[]);
    assert_eq!(code, 0, "check refused a generated project: {stderr}");
    assert!(
        stdout.contains(TEMPLATE_VERSION),
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
/// declarations and all — apart from `renvor.toml`, which records the template version and, since
/// version 6, names its persistence file conditionally.
///
/// `README.md` is the one other file the version-6 split could have disturbed, and
/// `the_sqlx_readme_is_unchanged_by_the_seaorm_split` compares its **persistence section** byte for
/// byte. No test compares the remaining files byte for byte; this one asserts the file **set**.
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
        for unbound in ["execute_unprepared", "Statement::from_string"] {
            assert!(
                !code.contains(unbound),
                "the generated repository uses `{unbound}`, which carries SQL with no bound \
                 values: {line}"
            );
        }
        // Concatenation, not just `format!`. A template edit could build SQL with `push_str` or
        // `+` and pass a scan that only looked for one macro.
        if code.contains("push_str") || code.contains("\" + ") {
            let upper = code.to_ascii_uppercase();
            for verb in ["SELECT", "INSERT", "UPDATE", "DELETE", "WHERE", "ORDER BY"] {
                assert!(
                    !upper.contains(verb),
                    "the generated repository concatenates `{verb}` into SQL: {line}"
                );
            }
        }
        // ANY interpolation into SQL, not two verbs. The previous form scanned for
        // `format!("SELECT` and `format!("INSERT` only, so `format!("ORDER BY {sort}")` — the
        // exact shape FR-035 forbids, and the one an allowlist exists to prevent — passed it.
        if let Some(rest) = code.split_once("format!(\"").map(|(_, rest)| rest) {
            let fragment = rest.to_ascii_uppercase();
            for verb in [
                "SELECT", "INSERT", "UPDATE", "DELETE", "ORDER BY", "WHERE", "FROM", "LIMIT",
                "OFFSET", "GROUP BY", "HAVING", "JOIN",
            ] {
                assert!(
                    !fragment.contains(verb),
                    "the generated repository interpolates `{verb}` into SQL: {line}"
                );
            }
        }
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

/// **FR-048.** The example domain is generated alongside the SeaORM sources, and compiles with them.
///
/// A review found this combination untested: the file-set test omitted `--example-domain`, and the
/// only test that passed it did not pass `--orm seaorm`. `templates::select` chooses the two groups
/// independently, so nothing exercised them together.
#[test]
fn the_example_domain_is_generated_alongside_the_seaorm_sources() {
    let generated = generate_ok(&[
        "--orm",
        "seaorm",
        "--database",
        "postgres",
        "--example-domain",
        "--seed-data",
    ]);
    let files = generated.files();
    for expected in [
        "src/domain.rs",
        "src/seed.rs",
        "src/entity.rs",
        "src/repository.rs",
    ] {
        assert!(
            files.contains(&expected.to_owned()),
            "`{expected}` is missing from --orm seaorm --example-domain --seed-data: {files:?}"
        );
    }

    // **FR-052.** The domain layer names no persistence technology at all. This is the file the
    // requirement is about, and until this test it was not even generated on the SeaORM path.
    let domain = generated.read("src/domain.rs");
    for forbidden in [
        "sea_orm",
        "sea-orm",
        "sqlx",
        "axum",
        "renvor_http",
        "ConnectionTrait",
    ] {
        assert!(
            !domain.contains(forbidden),
            "the domain layer names `{forbidden}`, which the dependency rule forbids"
        );
    }
    // CONTROL: the repository — the layer that IS allowed to know — does name SeaORM. Without
    // this, the absences above would also pass against an empty file.
    assert!(
        generated.read("src/repository.rs").contains("sea_orm"),
        "the persistence adapter does not name SeaORM, so the domain absences prove nothing"
    );
}

/// **FR-045.** `renvor routes` and `renvor openapi` behave IDENTICALLY on both persistence models.
///
/// # Not "succeed" — identical
///
/// Both refuse, and refusing is correct: a generated project declares no Renvor dependency, so it
/// has no route registry to ask. Route inspection asks the application binary for its own registry
/// rather than re-deriving one, and a project without the framework has none.
///
/// My first version of this test asserted success and failed, which was the test being wrong rather
/// than the CLI. What FR-045 requires is that choosing SeaORM does not change these surfaces — so
/// the assertion is that the exit code and the machine-readable reason match the SQLx project's,
/// which is the property an operator actually depends on.
#[test]
fn routes_and_openapi_behave_identically_on_both_persistence_models() {
    let sqlx = generate_ok(&["--orm", "sqlx", "--database", "postgres"]);
    let seaorm = generate_ok(&["--orm", "seaorm", "--database", "postgres"]);

    for command in ["routes", "openapi"] {
        let (sqlx_code, _out, sqlx_err) = renvor(&[command], &sqlx.root, &[]);
        let (seaorm_code, _out, seaorm_err) = renvor(&[command], &seaorm.root, &[]);

        assert_eq!(
            sqlx_code, seaorm_code,
            "`renvor {command}` exits {sqlx_code} on the SQLx project and {seaorm_code} on the \
             SeaORM one, so the ORM choice changed a transport surface"
        );

        // The machine-readable reason, not the prose — the prose is allowed to differ, the
        // contract is not.
        let reason = |text: &str| {
            text.lines()
                .find(|line| line.trim_start().starts_with("reason"))
                .map(|line| line.trim().to_owned())
        };
        assert_eq!(
            reason(&sqlx_err),
            reason(&seaorm_err),
            "`renvor {command}` reports a different reason for each ORM"
        );

        // CONTROL: there IS a reason to compare. Two `None`s would compare equal and prove
        // nothing.
        assert!(
            reason(&sqlx_err).is_some(),
            "`renvor {command}` reported no machine-readable reason, so the equality above is \
             vacuous"
        );
    }
}

// ──────────────────────────────────── the generated documentation names the right files

/// Collects every `src/…` path a generated file mentions.
///
/// Deliberately crude — it scans text rather than parsing TOML or Markdown, because the claims
/// being checked live in **comments and prose**, which no parser reaches.
fn source_paths_named_in(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("src/") {
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || "_/.".contains(c)))
            .unwrap_or(tail.len());
        let path = tail[..end].trim_end_matches('.').to_owned();
        if path.ends_with(".rs") && !found.contains(&path) {
            found.push(path);
        }
        rest = &tail[end.max(1)..];
    }
    found
}

/// **The defect this exists for.** `renvor.toml`'s `[persistence]` comment said "`src/persistence.rs`
/// and `migrations/` exist" on **both** ORM paths. On `--orm seaorm` that file does not exist: the
/// tree contains `src/entity.rs` and `src/repository.rs`.
///
/// The manifest's own opening rule is "A choice appears here only if a generated file reflects it".
/// A comment naming a file the generator did not write breaks that rule in the one file whose
/// entire purpose is to be believed about what was written.
#[test]
fn the_recorded_manifest_names_only_files_that_exist() {
    for orm in ["sqlx", "seaorm"] {
        let generated = generate_ok(&["--orm", orm, "--database", "postgres"]);
        let recorded = generated.read("renvor.toml");
        let named = source_paths_named_in(&recorded);

        // CONTROL: the manifest names at least one source file. Zero would make every assertion
        // below vacuously true, which is exactly how a scan stops scanning.
        assert!(
            !named.is_empty(),
            "{orm}: the manifest names no `src/…` file, so this test proves nothing:\n{recorded}"
        );

        let present = generated.files();
        for path in &named {
            assert!(
                present.contains(path),
                "{orm}: `renvor.toml` names `{path}`, which generation did not write. The tree \
                 is: {present:?}"
            );
        }
        // And `migrations/`, which both comments also claim.
        assert!(
            present.iter().any(|file| file.starts_with("migrations/")),
            "{orm}: the manifest claims `migrations/` exists and it does not"
        );
    }
}

/// The SQLx README documents the direct-SQLx path, and does not hand the reader SeaORM steps.
#[test]
fn the_sqlx_readme_documents_the_sqlx_path_only() {
    let readme = generate_ok(&["--orm", "sqlx", "--database", "postgres"]).read("README.md");

    for named in ["src/persistence.rs", "renvor-sqlx"] {
        assert!(
            readme.contains(named),
            "the direct-SQLx README no longer names `{named}`"
        );
    }
    for foreign in [
        "src/entity.rs",
        "src/repository.rs",
        "sea-orm",
        "SeaORM",
        "mod entity;",
    ] {
        assert!(
            !readme.contains(foreign),
            "the direct-SQLx README names `{foreign}`, so it documents a file this project does \
             not contain"
        );
    }
}

/// **FR-043's other half.** The SeaORM split changed the SQLx persistence section not at all.
///
/// # What this holds, exactly
///
/// The **whole persistence section**, byte for byte — from its heading to the start of
/// `### Applying the migrations` — not a sentence from it. An earlier version of this test asserted
/// one paragraph was present, and a review pointed out that every other byte of the section could
/// then be rewritten while it still passed. It does **not** hold the rest of the file: the sections
/// above and below are shared by both ORMs and are not what a persistence split can break.
///
/// Splitting a shared template by branch is the exact edit that rewords the branch nobody was
/// looking at, which is why the comparison is literal rather than a `contains` of something
/// memorable.
#[test]
fn the_sqlx_readme_is_unchanged_by_the_seaorm_split() {
    /// The Phase 006 persistence section, verbatim. Every byte, including the blank lines and the
    /// line breaks — a reflow is a change and this is the test that says so.
    const PHASE_006_PERSISTENCE: &str = "\
## Persistence

This project records `database = \"postgres\"` and `orm = \"sqlx\"` in `renvor.toml`, and
generation acted on both: `src/persistence.rs` holds the statements and the sort allowlist, and
`migrations/` holds a reversible `0001_create_item` pair.

For the reason above, `Cargo.toml` declares no driver yet. When the crates are published, add:

```toml
[dependencies]
renvor-sqlx = { version = \"<the published version>\", features = [\"db-postgres\"] }
```

`db-postgres` resolves the postgres driver and **only** that driver — selecting one
database does not build the other.

### What is already correct in `src/persistence.rs`

- **Every value is bound, never interpolated.** The statements are constants; the data arrives
  beside them as parameters.
- **Every sortable column is on an allowlist.** A column name cannot be a bound parameter, so the
  only safe construction is one where every possible value is written in the source. An unknown
  sort field maps to no column and the sort is refused — not silently replaced with a default.
- **Ordering is total.** `id` is in the allowlist as a tiebreaker, because a cursor built on an
  order the database may vary between calls silently skips or repeats rows.

";

    let readme = generate_ok(&["--database", "postgres"]).read("README.md");

    let start = readme
        .find("## Persistence")
        .expect("the direct-SQLx README has a persistence section");
    let end = readme[start..]
        .find("### Applying the migrations")
        .map(|at| start + at)
        .expect("the persistence section is followed by the migrations subsection");
    let section = &readme[start..end];

    assert_eq!(
        section, PHASE_006_PERSISTENCE,
        "the direct-SQLx persistence section changed. FR-043 requires the Phase 006 tree back \
         byte for byte apart from `renvor.toml`."
    );
}

/// The SeaORM README documents the SeaORM path, and never sends the reader to the SQLx one.
#[test]
fn the_seaorm_readme_documents_the_seaorm_path_only() {
    let readme = generate_ok(&["--orm", "seaorm", "--database", "postgres"]).read("README.md");

    for named in ["src/entity.rs", "src/repository.rs", "migrations/"] {
        assert!(
            readme.contains(named),
            "the SeaORM README does not name `{named}`"
        );
    }
    // THE DEFECT. The shared section told a SeaORM reader to add `renvor-sqlx` and described a
    // module their project does not have. Following it produced a project that does not resolve.
    for wrong in ["renvor-sqlx", "src/persistence.rs"] {
        assert!(
            !readme.contains(wrong),
            "the SeaORM README names `{wrong}`, which is the direct-SQLx path — following it is \
             what the split exists to prevent"
        );
    }
}

/// The SeaORM README states the compilation boundary rather than leaving it to be assumed.
///
/// # Why prose is asserted here
///
/// The claim being protected is not structural — it is that a reader is TOLD the two generated
/// files are not compiled by their project, and told what the SeaORM 2.0.2 compile actually was:
/// a manual verification result, not something the generator or CI does. A test that only checked
/// file names would pass against a README that quietly implied `cargo build` compiles them, which
/// is the belief this whole change exists to correct.
#[test]
fn the_seaorm_readme_states_the_compilation_boundary() {
    let readme = generate_ok(&["--orm", "seaorm", "--database", "postgres"]).read("README.md");

    for (claim, phrase) in [
        ("the modules are not declared", "does **not** declare"),
        ("nothing here compiles them", "builds neither file"),
        ("the manifest declares nothing", "declares no dependency"),
        ("offline generation is the reason", "offline"),
        ("the separate compile is named", "SeaORM 2.0.2"),
        ("and is attributed to a person", "added by hand"),
    ] {
        assert!(
            readme.contains(phrase),
            "the SeaORM README does not state that {claim} — `{phrase}` is absent:\n{readme}"
        );
    }
}

/// The SeaORM README must not claim the direct-SQLx module's sort allowlist.
///
/// # The defect this exists for
///
/// The version-6 split gave the SeaORM path its own README section, and its first draft carried
/// the SQLx bullets across unchanged: "every sortable column is on an allowlist", "an unknown sort
/// field is refused", "`id` is in the allowlist as a tiebreaker". `page_after` takes **no sort
/// field at all** — it always orders by `id` ascending — so all three described an API and a
/// failure path the reader's project does not have.
///
/// That is the same defect the split was written to remove, reintroduced one branch over. A review
/// caught it; this is what catches it next time.
#[test]
fn the_seaorm_readme_does_not_claim_a_sort_api_the_repository_lacks() {
    let generated = generate_ok(&["--orm", "seaorm", "--database", "postgres"]);
    let repository = generated.read("src/repository.rs");
    let readme = generated.read("README.md");

    // THE PREMISE, ASSERTED RATHER THAN ASSUMED. Everything below is only correct while the
    // repository really does take no sort field. If a later phase adds one, this fails first and
    // the README claims get rewritten — instead of the absences below quietly becoming true for a
    // reason nobody checked.
    assert!(
        !repository.to_ascii_lowercase().contains("sort"),
        "the generated SeaORM repository now has a sort surface, so the README's description of \
         sorting has to be rewritten rather than left absent"
    );

    for borrowed in [
        "sortable column is on an allowlist",
        "sort field maps to no column",
        "in the allowlist as a tiebreaker",
    ] {
        assert!(
            !readme.contains(borrowed),
            "the SeaORM README claims `{borrowed}`, which describes `src/persistence.rs` on the \
             direct-SQLx path. This project's `page_after` takes no sort field."
        );
    }

    // CONTROL: the SQLx README *does* carry those sentences, so their absence above is a property
    // of the SeaORM branch and not of the needles being unfindable.
    let sqlx = generate_ok(&["--orm", "sqlx", "--database", "postgres"]).read("README.md");
    assert!(
        sqlx.contains("sortable column is on an allowlist"),
        "the direct-SQLx README lost the allowlist description, so the SeaORM absences prove \
         nothing"
    );
}
