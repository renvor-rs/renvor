//! The starter matrix: every covering row of the Phase 011 generator proof, driven end to end.
//!
//! # What one row proves
//!
//! `renvor new … --framework-path <this workspace>` generates a framework-backed starter, and
//! generation itself runs the staged project's `cargo fmt --check`, `clippy -D warnings`, `build`,
//! `test` (which compiles the generated `tests/starter.rs`) and the route-dump smoke run before
//! placing it. This suite then runs the placed project's **own** test against the real services:
//! migrate, seed, start, register, log in, read the current user, refuse another user's delete,
//! log out under CSRF, answer the mailed flows identically, read the verification mail back from
//! the sink and confirm it, round-trip the cache, store and read an object, enqueue and complete a
//! job, answer health and metrics, export to a loopback OTLP receiver, and stop cleanly on the
//! interrupt a terminal sends. The starter is the proof; this file only turns the key.
//!
//! # Why these rows and not the 2 × 32 × 4 product
//!
//! The four full-featured rows cover every capability beside authentication on every persistence
//! row, which is where the implementation varies (the auth repositories and the job store are
//! per-engine). The single-capability rows prove each capability boots and serves **alone**, with
//! nothing else published, and the no-database row proves a starter without persistence. The
//! refusals cover every cross-choice rule by name. See `plan.md` §4 of the phase specification.
//!
//! # Gating, the same way the four-row suites gate
//!
//! Generation needs no service and always runs. The live proof needs the row's database
//! (`RENVOR_TEST_POSTGRES_URL` / `RENVOR_TEST_MYSQL_URL`), and for the capability rows the cache
//! and mail sink (`RENVOR_TEST_VALKEY_PASSWORD`, `RENVOR_TEST_SMTP_PASSWORD`,
//! `RENVOR_TEST_SMTP_API_URL`). A missing variable skips the live proof and says so; with
//! `RENVOR_TEST_REQUIRE_DATABASE=1` or `RENVOR_TEST_REQUIRE_CAPABILITIES=1` set, as the gate sets
//! them, a skip is a failure.
//!
//! # Which rows run: `RENVOR_TEST_STARTER_ROWS`
//!
//! Every row generates and builds a project, so the gate runs them **once**: its general
//! workspace test run sets `RENVOR_TEST_STARTER_ROWS=none` and its census runs them all against a
//! persistent build directory, refusing the variable so a skipped row can never satisfy a census
//! row. The platform job, which has no services, sets `RENVOR_TEST_STARTER_ROWS=nodb` to prove one
//! starter on macOS and Windows. Unset means every row. A comma-separated list of row names
//! selects rows; `none` skips them all, saying so. The refusals always run — they build nothing.
//!
//! # Serial, and one build directory
//!
//! The rows share the services and the test database, so one lock serialises them. They share
//! one `CARGO_TARGET_DIR` (`RENVOR_TEST_TARGET_DIR`, else a temporary directory kept for the life
//! of this binary), so the first row pays the cold build of the framework and the rest do not.
//!
//! # The touch before the placed project's test
//!
//! Generation compiled the project's test binary inside the staging directory, and cargo's
//! fingerprint does not change with the package path — so with a shared build directory the
//! placed project's `cargo test` would reuse that binary, whose compile-time `CARGO_MANIFEST_DIR`
//! names a directory that no longer exists. Touching the sources forces the recompile. A user
//! never meets this: generation builds into a temporary directory unless `CARGO_TARGET_DIR` is
//! set, and their first `cargo test` compiles fresh.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher as _, Hasher as _};
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

/// A service a row's live proof needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Service {
    Postgres,
    Mysql,
    Valkey,
    Smtp,
}

impl Service {
    /// The environment variable that says the service is there.
    fn variable(self) -> &'static str {
        match self {
            Self::Postgres => "RENVOR_TEST_POSTGRES_URL",
            Self::Mysql => "RENVOR_TEST_MYSQL_URL",
            Self::Valkey => "RENVOR_TEST_VALKEY_PASSWORD",
            Self::Smtp => "RENVOR_TEST_SMTP_PASSWORD",
        }
    }

    /// The variable that turns a skip into a failure.
    fn requirement(self) -> &'static str {
        match self {
            Self::Postgres | Self::Mysql => "RENVOR_TEST_REQUIRE_DATABASE",
            Self::Valkey | Self::Smtp => "RENVOR_TEST_REQUIRE_CAPABILITIES",
        }
    }
}

/// One covering row.
struct Row {
    /// The project name; also its binary's.
    name: &'static str,
    /// `--database`, or a starter without persistence.
    database: Option<&'static str>,
    /// `--orm`; ignored without a database.
    orm: &'static str,
    /// Everything after the persistence flags.
    flags: &'static [&'static str],
    /// What the live proof needs.
    needs: &'static [Service],
}

/// Every capability, authentication, the container profile with its cache, the example domain
/// and its seeds: every interaction at once.
const FULL: &[&str] = &[
    "--auth",
    "session",
    "--capabilities",
    "cache,jobs,mail,storage,observability",
    "--container",
    "--container-cache",
    "valkey",
    "--example-domain",
    "--seed-data",
];

const ROWS: [Row; 10] = [
    Row {
        name: "pgsqlx",
        database: Some("postgres"),
        orm: "sqlx",
        flags: FULL,
        needs: &[Service::Postgres, Service::Valkey, Service::Smtp],
    },
    Row {
        name: "mysqlx",
        database: Some("mysql"),
        orm: "sqlx",
        flags: FULL,
        needs: &[Service::Mysql, Service::Valkey, Service::Smtp],
    },
    Row {
        name: "pgsea",
        database: Some("postgres"),
        orm: "seaorm",
        flags: FULL,
        needs: &[Service::Postgres, Service::Valkey, Service::Smtp],
    },
    Row {
        name: "mysea",
        database: Some("mysql"),
        orm: "seaorm",
        flags: FULL,
        needs: &[Service::Mysql, Service::Valkey, Service::Smtp],
    },
    // Authentication with nothing beside the mail it needs: the auth ↔ mail bridge alone.
    Row {
        name: "authonly",
        database: Some("postgres"),
        orm: "sqlx",
        flags: &[
            "--auth",
            "session",
            "--capabilities",
            "mail",
            "--example-domain",
        ],
        needs: &[Service::Postgres, Service::Smtp],
    },
    // The cache alone, wired into the example domain's reads; the container cache is real.
    Row {
        name: "cacheonly",
        database: Some("postgres"),
        orm: "sqlx",
        flags: &[
            "--capabilities",
            "cache",
            "--container",
            "--container-cache",
            "valkey",
            "--example-domain",
        ],
        needs: &[Service::Postgres, Service::Valkey],
    },
    Row {
        name: "storageonly",
        database: Some("postgres"),
        orm: "sqlx",
        flags: &["--capabilities", "storage"],
        needs: &[Service::Postgres],
    },
    Row {
        name: "mailonly",
        database: Some("postgres"),
        orm: "sqlx",
        flags: &["--capabilities", "mail"],
        needs: &[Service::Postgres, Service::Smtp],
    },
    Row {
        name: "observeonly",
        database: Some("postgres"),
        orm: "sqlx",
        flags: &["--capabilities", "observability"],
        needs: &[Service::Postgres],
    },
    // No persistence at all: the kernel, the server, and one capability.
    Row {
        name: "nodb",
        database: None,
        orm: "sqlx",
        flags: &["--capabilities", "observability"],
        needs: &[],
    },
];

/// Whether `RENVOR_TEST_STARTER_ROWS` selects this row; a skip says so on stdout.
fn row_selected(name: &str) -> bool {
    match std::env::var("RENVOR_TEST_STARTER_ROWS") {
        Err(_) => true,
        Ok(value) if value.trim() == "none" => {
            println!("SKIPPED row {name}: RENVOR_TEST_STARTER_ROWS=none");
            false
        }
        Ok(value) => {
            let selected = value.split(',').any(|row| row.trim() == name);
            if !selected {
                println!("SKIPPED row {name}: not in RENVOR_TEST_STARTER_ROWS={value}");
            }
            selected
        }
    }
}

/// The framework checkout every starter points at: this workspace.
fn framework() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root exists")
}

/// One shared build directory per test binary.
fn target_dir() -> PathBuf {
    static TARGET: OnceLock<PathBuf> = OnceLock::new();
    TARGET
        .get_or_init(|| {
            if let Some(configured) = std::env::var_os("RENVOR_TEST_TARGET_DIR") {
                let path = PathBuf::from(configured);
                assert!(
                    path.is_absolute(),
                    "RENVOR_TEST_TARGET_DIR must be absolute: {}",
                    path.display()
                );
                std::fs::create_dir_all(&path).expect("the build directory exists");
                return path;
            }
            static KEPT: OnceLock<tempfile::TempDir> = OnceLock::new();
            KEPT.get_or_init(|| tempfile::tempdir().expect("tempdir"))
                .path()
                .to_path_buf()
        })
        .clone()
}

/// The rows share the services and the database; this serialises them.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One command's result; the status is separate so `output` stays parseable.
struct Run {
    succeeded: bool,
    status: String,
    /// Both streams, for a failure message that should show everything the command said.
    output: String,
    /// `stdout` **alone**. C-1 reserves it for the result, so parsing one JSON envelope must read
    /// this and not the combination: a diagnostic on `stderr` is not a second document. The
    /// FR-012-8 resolution notice prints on every leg whose toolchain is not the generated pin —
    /// which is every `stable` leg — and reading the two together turned the envelope into
    /// "trailing characters". Found by `verify (stable)` on 2026-09-08.
    stdout: String,
}

fn run(program: &str, args: &[&str], directory: &Path, envs: &[(&str, String)]) -> Run {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", target_dir())
        // Ten projects' incremental caches were a fifth of a 31 GB build directory on the first
        // full run; nothing here is rebuilt often enough to earn them. Passed through the sealed
        // verification environment as well, since the seal admits this variable.
        .env("CARGO_INCREMENTAL", "0");
    for (name, value) in envs {
        command.env(name, value);
    }
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("`{program}` could not be run: {error}"));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    Run {
        succeeded: output.status.success(),
        status: format!("{}", output.status),
        output: format!("{stdout}{stderr}"),
        stdout,
    }
}

/// The `renvor new` arguments for a row, plus `extra`.
fn arguments(row: &Row, extra: &[&str]) -> Vec<String> {
    let mut args = vec!["new".to_owned(), row.name.to_owned()];
    if let Some(database) = row.database {
        args.extend(["--database", database, "--orm", row.orm].map(str::to_owned));
    }
    args.extend(row.flags.iter().map(|flag| (*flag).to_owned()));
    args.extend([
        "--framework-path".to_owned(),
        framework().display().to_string(),
    ]);
    args.extend(["--output", "json", "--yes"].map(str::to_owned));
    args.extend(extra.iter().map(|flag| (*flag).to_owned()));
    args
}

/// Runs `renvor new` for a row and returns the parsed envelope.
fn attempt(base: &Path, row: &Row, extra: &[&str]) -> (Run, serde_json::Value) {
    let args = arguments(row, extra);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let outcome = run(env!("CARGO_BIN_EXE_renvor"), &borrowed, base, &[]);
    let document: serde_json::Value = serde_json::from_str(&outcome.stdout).unwrap_or_else(|_| {
        panic!(
            "not a JSON envelope for {} [{}]:\n{}",
            row.name, outcome.status, outcome.output
        )
    });
    (outcome, document)
}

/// Generates a row and returns the placed project and its envelope.
fn generate(base: &Path, row: &Row) -> (PathBuf, serde_json::Value) {
    let (outcome, document) = attempt(base, row, &[]);
    assert!(
        outcome.succeeded && document["status"] == "success",
        "generation failed for {} [{}]:\n{}",
        row.name,
        outcome.status,
        outcome.output
    );
    let project = base.join(row.name);
    assert!(project.is_dir(), "the destination was not placed");
    (project, document)
}

/// The capabilities a row selected, from its own flags.
fn selected(row: &Row) -> Vec<&'static str> {
    row.flags
        .iter()
        .position(|flag| *flag == "--capabilities")
        .map(|at| row.flags[at + 1].split(',').collect())
        .unwrap_or_default()
}

/// The package names the application reaches in the project's `Cargo.lock`: the closure of
/// `[dependencies]` — what the binary links, dependencies of dependencies included — and not of
/// `[dev-dependencies]`, whose closure is the test's (the testkit and its own graph). The lock's
/// `dependencies` lists do not distinguish edge kinds, so the walk starts from the manifest's
/// runtime roots rather than from the package. A lock entry is `name`, `name version`, or
/// `name version (source)`; the name is the first word.
fn lock_closure(project: &Path) -> std::collections::BTreeSet<String> {
    let manifest = std::fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    let manifest: toml::Value = toml::from_str(&manifest).expect("a manifest");
    let roots: Vec<String> = manifest["dependencies"]
        .as_table()
        .expect("[dependencies]")
        .keys()
        .cloned()
        .collect();
    let lock = std::fs::read_to_string(project.join("Cargo.lock")).expect("Cargo.lock");
    let lock: toml::Value = toml::from_str(&lock).expect("a lockfile");
    let mut edges: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for package in lock["package"].as_array().expect("packages") {
        let name = package["name"].as_str().expect("a name").to_owned();
        let dependencies = package
            .get("dependencies")
            .and_then(toml::Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(toml::Value::as_str)
                    .filter_map(|entry| entry.split_whitespace().next())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        edges.entry(name).or_default().extend(dependencies);
    }
    for root in &roots {
        assert!(edges.contains_key(root), "the lock names {root}");
    }
    let mut reached = std::collections::BTreeSet::new();
    let mut pending = roots;
    while let Some(name) = pending.pop() {
        if reached.insert(name.clone()) {
            pending.extend(edges.get(&name).into_iter().flatten().cloned());
        }
    }
    reached
}

/// Every selected choice is recorded and wired; every unselected one appears nowhere.
fn assert_recorded(project: &Path, row: &Row) {
    let manifest = std::fs::read_to_string(project.join("renvor.toml")).expect("renvor.toml");
    let cargo = std::fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    let auth = row
        .flags
        .windows(2)
        .any(|pair| pair == ["--auth", "session"]);
    assert_eq!(
        manifest.contains("auth = \"session\""),
        auth,
        "renvor.toml must record the auth starter exactly as chosen:\n{manifest}"
    );
    assert!(manifest.contains("source = \"path\""), "{manifest}");
    // FR-024: `Cargo.toml` text and file presence are the declaration; the lock closure walked
    // from this project's package is what cargo actually resolved, and an unselected capability
    // must be absent from it, not merely undeclared.
    let closure = lock_closure(project);
    for capability in ["cache", "jobs", "mail", "storage", "observability"] {
        let chosen = selected(row).contains(&capability);
        assert_eq!(
            closure.contains(format!("renvor-{capability}").as_str()),
            chosen,
            "the lock closure must reach renvor-{capability} exactly when it is selected: {closure:?}"
        );
        assert!(
            manifest.contains(&format!("{capability} = {chosen}")),
            "renvor.toml must record `{capability} = {chosen}`:\n{manifest}"
        );
        assert_eq!(
            cargo.contains(&format!("renvor-{capability}")),
            chosen,
            "Cargo.toml must depend on renvor-{capability} exactly when it is selected"
        );
        assert_eq!(
            project
                .join("src/capabilities")
                .join(format!("{capability}.rs"))
                .is_file(),
            chosen,
            "src/capabilities/{capability}.rs must exist exactly when it is selected"
        );
        assert_eq!(
            project
                .join("config")
                .join(format!(
                    "{}.toml{}",
                    if capability == "observability" {
                        "otlp"
                    } else {
                        capability
                    },
                    if capability == "observability" {
                        ".example"
                    } else {
                        ""
                    }
                ))
                .is_file(),
            chosen,
            "the {capability} configuration file must exist exactly when it is selected"
        );
    }
    assert_eq!(project.join("src/auth.rs").is_file(), auth);
    assert_eq!(cargo.contains("renvor-auth"), auth);
    // `renvor-auth` rides with either persistence adapter (each implements its repositories),
    // so it is in every database-backed starter's graph whether or not the auth starter was
    // chosen; the crate that follows the choice is `renvor-auth-http`, the routes
    // (phase-011-limitations.md).
    assert_eq!(closure.contains("renvor-auth-http"), auth, "{closure:?}");
    assert_eq!(
        closure.contains("renvor-auth"),
        auth || row.database.is_some(),
        "{closure:?}"
    );
    assert_eq!(
        project.join("src/capabilities/mod.rs").is_file(),
        !selected(row).is_empty()
    );
    assert_eq!(
        project.join("migrations").is_dir(),
        row.database.is_some(),
        "a migrations directory exists exactly when there is a database"
    );
}

/// A 32-byte key as 64 hexadecimal characters, from the standard library's own entropy. Never a
/// fixture: a key in a test file would be a credential in the repository.
fn random_key() -> String {
    let mut key = String::with_capacity(64);
    for _ in 0..4 {
        let word = RandomState::new().build_hasher().finish();
        key.push_str(&format!("{word:016x}"));
    }
    key
}

/// The value of a service's variable, or `None` for a skip — unless the gate requires it.
fn service(needed: Service) -> Option<String> {
    match std::env::var(needed.variable()) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => {
            assert!(
                std::env::var(needed.requirement()).is_err(),
                "{} is set and {} is not",
                needed.requirement(),
                needed.variable()
            );
            println!("SKIPPED live proof: {} is not set", needed.variable());
            None
        }
    }
}

/// Forces the placed project's test to recompile; see the module documentation.
fn touch(project: &Path) {
    for relative in ["tests/starter.rs", "src/main.rs"] {
        let file = std::fs::File::options()
            .write(true)
            .open(project.join(relative))
            .expect("the generated source exists");
        file.set_modified(std::time::SystemTime::now())
            .expect("the modification time is writable");
    }
}

/// Whether a `cargo test` transcript ends in a pass with at least one test run and none failed
/// — the placed project's test binary carries the starter test and, since the correction round,
/// the support module's own negative control, so the count is not pinned to one.
fn ran_to_a_pass(output: &str) -> bool {
    output.lines().any(|line| {
        line.starts_with("test result: ok.")
            && line.contains("; 0 failed;")
            && !line.starts_with("test result: ok. 0 passed")
    })
}

/// Runs the placed project's own test against the real services. `false` means skipped.
fn live(row: &Row, project: &Path) -> bool {
    let mut envs: Vec<(&str, String)> = Vec::new();
    for needed in row.needs {
        let Some(value) = service(*needed) else {
            return false;
        };
        match needed {
            Service::Postgres | Service::Mysql => envs.push(("RENVOR_DATABASE_URL", value)),
            Service::Valkey => envs.push(("RENVOR_CACHE_PASSWORD", value)),
            Service::Smtp => {
                envs.push(("RENVOR_MAIL_PASSWORD", value));
                if let Ok(api) = std::env::var("RENVOR_TEST_SMTP_API_URL") {
                    envs.push(("RENVOR_TEST_SMTP_API_URL", api));
                }
            }
        }
    }
    envs.push(("RENVOR_AUTH_CSRF_KEY", random_key()));
    envs.push(("RENVOR_AUTH_ABUSE_KEY", random_key()));
    if row.database.is_some() {
        // The generated test skips without a database; the gate must never see a skip here.
        envs.push(("RENVOR_TEST_REQUIRE_DATABASE", "1".to_owned()));
    }
    if std::env::var("RENVOR_TEST_REQUIRE_CAPABILITIES").is_ok() {
        // Likewise the verification mail: with the gate's requirement forwarded, a missing sink
        // fails the generated test instead of printing SKIPPED under a green census.
        envs.push(("RENVOR_TEST_REQUIRE_CAPABILITIES", "1".to_owned()));
    }
    touch(project);
    let outcome = run(
        "cargo",
        &["test", "--test", "starter", "--", "--nocapture"],
        project,
        &envs,
    );
    assert!(
        outcome.succeeded,
        "the placed project's own test failed for {} [{}]:\n{}",
        row.name, outcome.status, outcome.output
    );
    assert!(
        ran_to_a_pass(&outcome.output),
        "the placed project's test did not run to a pass for {}:\n{}",
        row.name,
        outcome.output
    );
    true
}

/// Every file under `root`, sorted, with its bytes — except the provenance record's one measured
/// table.
///
/// Phase 012 (FR-012-4): `[verified_with]` records the instant one run's checks passed and what
/// that run observed Cargo launch. Two runs that produced byte-identical projects differ there,
/// and a record that did **not** differ would be one filled in from something other than the run
/// it describes. Only that table is stripped, by [`without_verified_with`]; `record_version`,
/// `[toolchain]`, and every `[[file]]` digest are compared like any other file's bytes.
fn tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("relative")
                    .display()
                    .to_string();
                let bytes = std::fs::read(&path).expect("read");
                let bytes = if relative.replace('\\', "/") == ".renvor/generated.toml" {
                    without_verified_with(&String::from_utf8_lossy(&bytes)).into_bytes()
                } else {
                    bytes
                };
                files.push((relative, bytes));
            }
        }
    }
    files.sort();
    files
}

/// The provenance record without its `[verified_with]` table and that table's sub-tables.
///
/// A top-level table ends the block; `[verified_with.checks.build]` and its siblings belong to it
/// and go with it.
fn without_verified_with(record: &str) -> String {
    let mut kept = String::with_capacity(record.len());
    let mut inside = false;
    for line in record.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            inside =
                trimmed.starts_with("[verified_with]") || trimmed.starts_with("[verified_with.");
        }
        if !inside {
            kept.push_str(line);
            kept.push('\n');
        }
    }
    kept
}

macro_rules! row {
    ($test:ident, $index:expr) => {
        #[test]
        fn $test() {
            let _serial = serial();
            let row = &ROWS[$index];
            if !row_selected(row.name) {
                return;
            }
            let base = tempfile::tempdir().expect("tempdir");
            let (project, document) = generate(base.path(), row);
            assert!(
                document["result"]["manifest"]
                    .as_array()
                    .is_some_and(|entries| !entries.is_empty()),
                "an empty manifest for {}",
                row.name
            );
            assert_recorded(&project, row);
            live(row, &project);
        }
    };
}

row!(sqlx_postgres_with_everything_generates_and_proves_itself, 0);
row!(sqlx_mysql_with_everything_generates_and_proves_itself, 1);
row!(
    seaorm_postgres_with_everything_generates_and_proves_itself,
    2
);
row!(seaorm_mysql_with_everything_generates_and_proves_itself, 3);
row!(
    authentication_with_only_its_mail_generates_and_proves_itself,
    4
);
row!(the_cache_alone_generates_and_proves_itself, 5);
row!(storage_alone_generates_and_proves_itself, 6);
row!(mail_alone_generates_and_proves_itself, 7);
row!(observability_alone_generates_and_proves_itself, 8);
row!(a_starter_without_a_database_generates_and_proves_itself, 9);

#[test]
fn every_invalid_combination_is_refused_before_any_write() {
    let framework = framework().display().to_string();
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let not_a_workspace = elsewhere.path().display().to_string();
    // (flags, the code, the flag the refusal names)
    let refusals: [(&[&str], &str, &str); 10] = [
        (
            &["--capabilities", "jobs", "--framework-path", &framework],
            "unsupported_combination",
            "--capabilities",
        ),
        (
            &["--auth", "session", "--framework-path", &framework],
            "unsupported_combination",
            "--auth",
        ),
        (
            &[
                "--database",
                "postgres",
                "--auth",
                "session",
                "--framework-path",
                &framework,
            ],
            "unsupported_combination",
            "--auth",
        ),
        (
            &["--capabilities", "cache"],
            "unsupported_combination",
            "--framework-path",
        ),
        (
            &["--capabilities", "s3", "--framework-path", &framework],
            "unsupported_value",
            "--capabilities",
        ),
        (
            &[
                "--capabilities",
                "cache,cache",
                "--framework-path",
                &framework,
            ],
            "unsupported_value",
            "--capabilities",
        ),
        (
            &[
                "--capabilities",
                "none,cache",
                "--framework-path",
                &framework,
            ],
            "unsupported_combination",
            "--capabilities",
        ),
        (
            &[
                "--database",
                "postgres",
                "--auth",
                "api",
                "--capabilities",
                "mail",
                "--framework-path",
                &framework,
            ],
            "unsupported_value",
            "--auth",
        ),
        (
            &[
                "--database",
                "postgres",
                "--auth",
                "full",
                "--capabilities",
                "mail",
                "--framework-path",
                &framework,
            ],
            "unsupported_value",
            "--auth",
        ),
        (
            &[
                "--capabilities",
                "cache",
                "--framework-path",
                &not_a_workspace,
            ],
            "unsupported_value",
            "--framework-path",
        ),
    ];
    for (flags, code, named) in refusals {
        let base = tempfile::tempdir().expect("tempdir");
        let mut args = vec!["new", "refused"];
        args.extend_from_slice(flags);
        args.extend_from_slice(&["--output", "json", "--yes"]);
        let outcome = run(env!("CARGO_BIN_EXE_renvor"), &args, base.path(), &[]);
        assert!(
            !outcome.succeeded,
            "{flags:?} was accepted:\n{}",
            outcome.output
        );
        let document: serde_json::Value = serde_json::from_str(&outcome.stdout)
            .unwrap_or_else(|_| panic!("not JSON for {flags:?}:\n{}", outcome.output));
        assert_eq!(document["error"]["code"], code, "{flags:?}: {document}");
        let details = document["error"]["details"].to_string();
        assert!(
            details.contains(named),
            "{flags:?}: the refusal must name {named}: {document}"
        );
        assert!(
            !base.path().join("refused").exists(),
            "{flags:?}: a refusal wrote the destination"
        );
        assert!(
            std::fs::read_dir(base.path())
                .expect("read_dir")
                .next()
                .is_none(),
            "{flags:?}: a refusal left something behind"
        );
    }
}

#[test]
fn a_dry_run_of_a_starter_matches_the_real_run_and_writes_nothing() {
    let _serial = serial();
    let row = &ROWS[9];
    if !row_selected(row.name) {
        return;
    }
    let base = tempfile::tempdir().expect("tempdir");
    let (dry, dry_document) = attempt(base.path(), row, &["--dry-run"]);
    assert!(dry.succeeded, "the dry run failed:\n{}", dry.output);
    assert!(
        std::fs::read_dir(base.path())
            .expect("read_dir")
            .next()
            .is_none(),
        "a dry run wrote something"
    );
    let (_project, real_document) = generate(base.path(), row);
    // THE RECORD IS COMPARED BY PATH AND KIND, NOT BY DIGEST (Phase 012, FR-012-4), for the same
    // reason `tree` strips one of its tables: `[verified_with]` measures the run that wrote it,
    // so the two runs' records differ by the instant they happened and their digests differ with
    // them. Every other entry, digest included, is compared — which is what SC-006 asserts.
    let entries = |document: &serde_json::Value| -> Vec<serde_json::Value> {
        document["result"]["manifest"]
            .as_array()
            .expect("a manifest")
            .iter()
            .map(|entry| {
                if entry["path"] == ".renvor/generated.toml" {
                    serde_json::json!({ "path": entry["path"], "kind": entry["kind"] })
                } else {
                    entry.clone()
                }
            })
            .collect()
    };
    let (dry_entries, real_entries) = (entries(&dry_document), entries(&real_document));
    assert!(
        dry_entries
            .iter()
            .any(|entry| entry["path"] == ".renvor/generated.toml"),
        "neither run listed a provenance record, so its exclusion above is hiding a missing file"
    );
    assert_eq!(
        dry_entries, real_entries,
        "the dry run's manifest differs from the real run's"
    );
}

#[test]
fn a_starter_generated_twice_is_byte_identical_and_a_rerun_changes_nothing() {
    let _serial = serial();
    let row = &ROWS[9];
    if !row_selected(row.name) {
        return;
    }
    let base = tempfile::tempdir().expect("tempdir");
    let one = base.path().join("one");
    let two = base.path().join("two");
    std::fs::create_dir_all(&one).expect("mkdir");
    std::fs::create_dir_all(&two).expect("mkdir");
    let (first, _) = generate(&one, row);
    let (second, _) = generate(&two, row);
    assert_eq!(
        tree(&first),
        tree(&second),
        "two identical runs produced different trees"
    );

    // A rerun into the placed destination is refused by name, and the tree is untouched.
    let before = tree(&first);
    let (outcome, document) = attempt(&one, row, &[]);
    assert!(
        !outcome.succeeded,
        "a rerun overwrote the destination:\n{}",
        outcome.output
    );
    assert_eq!(
        document["error"]["code"], "destination_exists",
        "{document}"
    );
    assert_eq!(
        tree(&first),
        before,
        "a refused rerun changed the destination"
    );
}

#[test]
fn a_failure_after_verification_leaves_the_destination_absent() {
    // `RENVOR_FAIL_AT` is honoured in debug builds only, which is what a test binary is.
    let _serial = serial();
    let row = &ROWS[9];
    if !row_selected(row.name) {
        return;
    }
    let base = tempfile::tempdir().expect("tempdir");
    let args = arguments(row, &[]);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    let outcome = run(
        env!("CARGO_BIN_EXE_renvor"),
        &borrowed,
        base.path(),
        &[("RENVOR_FAIL_AT", "verify".to_owned())],
    );
    assert!(
        !outcome.succeeded,
        "the injected failure did not fail:\n{}",
        outcome.output
    );
    assert!(
        !base.path().join(row.name).exists(),
        "a failed generation left the destination"
    );
    assert!(
        std::fs::read_dir(base.path())
            .expect("read_dir")
            .next()
            .is_none(),
        "a failed generation left staging behind"
    );
}

/// Runs `renvor generate …` in a placed project and returns the envelope.
fn generate_into(project: &Path, args: &[&str]) -> (Run, serde_json::Value) {
    let mut full = vec!["generate"];
    full.extend_from_slice(args);
    full.extend_from_slice(&["--output", "json"]);
    let outcome = run(env!("CARGO_BIN_EXE_renvor"), &full, project, &[]);
    let document: serde_json::Value = serde_json::from_str(&outcome.stdout).unwrap_or_else(|_| {
        panic!(
            "not a JSON envelope for generate {args:?} [{}]:\n{}",
            outcome.status, outcome.output
        )
    });
    (outcome, document)
}

/// The placed project's own checks — the ones generation runs before placing a starter — after a
/// generator wrote into it: it must still format, lint on every target, and pass its tests.
fn checks_after_generation(project: &Path, test: &str, needs: &[Service]) -> bool {
    for (args, what) in [
        (vec!["fmt", "--check"], "cargo fmt --check"),
        (
            vec!["clippy", "--all-targets", "--", "-D", "warnings"],
            "cargo clippy --all-targets -- -D warnings",
        ),
    ] {
        let outcome = run("cargo", &args, project, &[]);
        assert!(
            outcome.succeeded,
            "`{what}` failed after generation [{}]:\n{}",
            outcome.status, outcome.output
        );
    }
    let mut envs: Vec<(&str, String)> = Vec::new();
    for needed in needs {
        let Some(value) = service(*needed) else {
            return false;
        };
        match needed {
            Service::Postgres | Service::Mysql => envs.push(("RENVOR_DATABASE_URL", value)),
            Service::Valkey => envs.push(("RENVOR_CACHE_PASSWORD", value)),
            Service::Smtp => {
                envs.push(("RENVOR_MAIL_PASSWORD", value));
                if let Ok(api) = std::env::var("RENVOR_TEST_SMTP_API_URL") {
                    envs.push(("RENVOR_TEST_SMTP_API_URL", api));
                }
            }
        }
    }
    envs.push(("RENVOR_AUTH_CSRF_KEY", random_key()));
    envs.push(("RENVOR_AUTH_ABUSE_KEY", random_key()));
    envs.push(("RENVOR_TEST_REQUIRE_DATABASE", "1".to_owned()));
    touch(project);
    let outcome = run(
        "cargo",
        &["test", "--test", test, "--", "--nocapture"],
        project,
        &envs,
    );
    assert!(
        outcome.succeeded && ran_to_a_pass(&outcome.output),
        "the generated test `{test}` did not pass [{}]:\n{}",
        outcome.status,
        outcome.output
    );
    true
}

macro_rules! resource_row {
    ($test:ident, $row_name:literal, $database:literal, $orm:literal) => {
        #[test]
        fn $test() {
            // FR-045 and FR-048. A resource generated into a placed starter compiles, lints on
            // every target, and its own generated test drives it live; a rerun is a no-op; a
            // file the user changed is a conflict that writes nothing.
            let _serial = serial();
            if !row_selected($row_name) {
                return;
            }
            let row = Row {
                name: $row_name,
                database: Some($database),
                orm: $orm,
                flags: &[
                    "--auth",
                    "session",
                    "--capabilities",
                    "mail",
                    "--example-domain",
                ],
                needs: if $database == "postgres" {
                    &[Service::Postgres, Service::Smtp]
                } else {
                    &[Service::Mysql, Service::Smtp]
                },
            };
            let base = tempfile::tempdir().expect("tempdir");
            let (project, _) = generate(base.path(), &row);
            let (outcome, document) = generate_into(
                &project,
                &[
                    "resource",
                    "Post",
                    "title:string",
                    "body:text",
                    "published:boolean",
                ],
            );
            assert!(outcome.succeeded, "{document}\n{}", outcome.output);
            assert_eq!(document["result"]["written"], 6, "{document}");
            assert!(project.join("src/resources/post.rs").is_file());
            assert!(project.join("tests/post.rs").is_file());
            let routes = std::fs::read_to_string(project.join("src/routes.rs")).expect("routes");
            assert!(routes.contains("crate::resources::post::declare(&mut routes)?;"));
            let record =
                std::fs::read_to_string(project.join(".renvor/generated.toml")).expect("record");
            assert!(
                record.contains("[[resource]]") && record.contains("name = \"Post\""),
                "the record carries the resource's definition:\n{record}"
            );

            // A name that would be a bare SQL keyword is refused before anything is planned.
            let (outcome, refused) =
                generate_into(&project, &["resource", "Order", "title:string"]);
            assert!(
                !outcome.succeeded,
                "a reserved word was accepted: {refused}"
            );
            assert_eq!(refused["error"]["code"], "unsupported_value", "{refused}");
            assert_eq!(
                refused["error"]["details"]["reason"], "reserved_identifier",
                "{refused}"
            );
            assert!(!project.join("src/resources/order.rs").exists());

            // A rerun is a no-op, and says so.
            let (outcome, again) = generate_into(
                &project,
                &[
                    "resource",
                    "Post",
                    "title:string",
                    "body:text",
                    "published:boolean",
                ],
            );
            assert!(outcome.succeeded, "{again}");
            assert_eq!(again["result"]["written"], 0, "{again}");

            checks_after_generation(&project, "post", row.needs);

            // The user changes the module; a rerun with a different shape is a conflict.
            let module = project.join("src/resources/post.rs");
            let mut text = std::fs::read_to_string(&module).expect("module");
            text.push_str("\n// mine\n");
            std::fs::write(&module, &text).expect("write");
            let (outcome, refused) =
                generate_into(&project, &["resource", "Post", "title:string", "body:text"]);
            assert!(
                !outcome.succeeded,
                "a changed module was overwritten: {refused}"
            );
            assert_eq!(refused["error"]["code"], "generation_conflict", "{refused}");
            assert_eq!(
                std::fs::read_to_string(&module).expect("module"),
                text,
                "the user's module was touched"
            );
        }
    };
}

resource_row!(
    a_resource_generated_into_a_sqlx_starter_proves_itself,
    "ressqlx",
    "postgres",
    "sqlx"
);
resource_row!(
    a_resource_generated_into_a_seaorm_starter_proves_itself,
    "ressea",
    "mysql",
    "seaorm"
);

/// Whether `address` answers HTTP at all — the HTTP provider starts last, so any status line
/// means every provider booted.
fn answers_http(address: &str) -> bool {
    let Ok(mut stream) = TcpStream::connect(address) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let request = format!("GET / HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reply = String::new();
    let _ = stream.read_to_string(&mut reply);
    reply.starts_with("HTTP/1.1 ")
}

/// Starts the placed project's binary against the services **without resetting anything**,
/// waits until it answers over HTTP, and ends it. What it proves: every migration the ledger
/// already holds is accepted — its checksum unchanged — and every migration added since applies
/// forward. `false` means skipped for a missing service.
fn boots_against_the_existing_ledger(project: &Path, needs: &[Service]) -> bool {
    let mut envs: Vec<(&str, String)> = Vec::new();
    for needed in needs {
        let Some(value) = service(*needed) else {
            return false;
        };
        match needed {
            Service::Postgres | Service::Mysql => envs.push(("RENVOR_DATABASE_URL", value)),
            Service::Valkey => envs.push(("RENVOR_CACHE_PASSWORD", value)),
            Service::Smtp => envs.push(("RENVOR_MAIL_PASSWORD", value)),
        }
    }
    envs.push(("RENVOR_AUTH_CSRF_KEY", random_key()));
    envs.push(("RENVOR_AUTH_ABUSE_KEY", random_key()));
    envs.push(("RENVOR_HTTP_ADDRESS", "127.0.0.1:0".to_owned()));
    let mut command = Command::new("cargo");
    command
        .args(["run", "--quiet"])
        .current_dir(project)
        .env("CARGO_TARGET_DIR", target_dir())
        .env("CARGO_INCREMENTAL", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (name, value) in &envs {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("cargo run starts");
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (announce, announced) = std::sync::mpsc::channel::<Option<String>>();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = announce.send(None);
                    return;
                }
                Ok(_) => {
                    if let Some(address) = line.trim().split(" is listening at http://").nth(1) {
                        let _ = announce.send(Some(address.to_owned()));
                    }
                }
            }
        }
    });
    let diagnostics = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut text);
        text
    });
    // Generous: `cargo run` may compile first.
    let address = match announced.recv_timeout(Duration::from_secs(900)) {
        Ok(Some(address)) => address,
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            let stderr = diagnostics.join().unwrap_or_default();
            let tail: Vec<&str> = stderr.lines().rev().take(40).collect::<Vec<_>>();
            panic!(
                "the project exited before it announced its address: it must accept the ledger \
                 it already applied and add the new migrations forward (last lines):\n{}",
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            );
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the project did not announce its address within the bound");
        }
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if answers_http(&address) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the project announced {address} but never answered on it"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    true
}

/// FR-047, with the proofs the correction round of Phase 011 added: a starter generated without
/// authentication gains the session starter later — **after** it has booted and migrated, after a
/// resource was generated into it, and beside a line the user wrote outside the markers of
/// `src/routes.rs`. The applied migrations stay byte-identical and the owner column arrives
/// forward; the resource is rendered again with its guards; the lockfile is resolved for the new
/// dependencies; the user's line is a conflict that writes nothing.
fn the_auth_starter_added_later(row_name: &'static str, database: &'static str, orm: &'static str) {
    let _serial = serial();
    if !row_selected(row_name) {
        return;
    }
    let needs: &[Service] = if database == "postgres" {
        &[Service::Postgres, Service::Smtp]
    } else {
        &[Service::Mysql, Service::Smtp]
    };
    let row = Row {
        name: row_name,
        database: Some(database),
        orm,
        flags: &["--capabilities", "mail", "--example-domain"],
        needs,
    };
    let base = tempfile::tempdir().expect("tempdir");
    let (project, _) = generate(base.path(), &row);
    assert!(!project.join("src/auth.rs").exists());

    // 1. A resource before auth: its writes are public, as the manifest says.
    let (outcome, document) = generate_into(&project, &["resource", "Post", "title:string"]);
    assert!(outcome.succeeded, "{document}\n{}", outcome.output);
    let module = project.join("src/resources/post.rs");
    let post_before = std::fs::read_to_string(&module).expect("module");
    assert!(!post_before.contains("require_session"), "{post_before}");

    // 2. The project boots and migrates: the ledger now holds the item and post migrations by
    //    their checksums. Skipped without the services, and everything below that needs the
    //    ledger is skipped with it.
    let migrated = checks_after_generation(&project, "post", row.needs);

    // 3. A line of the user's own outside the markers makes the auth generator refuse — nothing
    //    written — and is put back for the run that succeeds.
    let routes = project.join("src/routes.rs");
    let original = std::fs::read_to_string(&routes).expect("routes");
    std::fs::write(
        &routes,
        format!("{original}\n// the user's own registration lives here\n"),
    )
    .expect("write");
    let (outcome, refused) = generate_into(&project, &["auth"]);
    assert!(
        !outcome.succeeded,
        "the user's line was overwritten: {refused}"
    );
    assert_eq!(refused["error"]["code"], "generation_conflict", "{refused}");
    assert_eq!(
        refused["error"]["details"]["reason"], "changed_since_generation",
        "{refused}"
    );
    assert!(
        refused["error"]["details"]["changed"]
            .as_str()
            .is_some_and(|paths| paths.contains("src/routes.rs")),
        "{refused}"
    );
    assert!(
        refused["error"]["details"]["paths"]
            .as_str()
            .is_some_and(|paths| paths.contains("src/routes.rs")),
        "{refused}"
    );
    assert!(
        !project.join("src/auth.rs").exists(),
        "a conflict wrote the starter"
    );
    std::fs::write(&routes, &original).expect("restore");

    // 4. The auth starter renders every generator-owned file again, so each is regenerable, and
    //    FR-048 (as decided 2026-09-05) replaces one only under `--overwrite-unchanged`: without
    //    the flag the run is refused naming it, and nothing is written.
    let (outcome, refused) = generate_into(&project, &["auth"]);
    assert!(
        !outcome.succeeded,
        "regenerable files were replaced without the flag: {refused}"
    );
    assert_eq!(refused["error"]["code"], "generation_conflict", "{refused}");
    assert_eq!(
        refused["error"]["details"]["reason"], "overwrite_required",
        "{refused}"
    );
    assert_eq!(
        refused["error"]["details"]["flag"], "--overwrite-unchanged",
        "{refused}"
    );
    assert!(
        refused["error"]["details"]["regenerable"]
            .as_str()
            .is_some_and(|paths| paths.contains("src/main.rs")),
        "{refused}"
    );
    assert!(
        !project.join("src/auth.rs").exists(),
        "a refusal wrote the starter"
    );
    // With the flag, added.
    let item_up = project.join("migrations/0001_create_item.up.sql");
    let item_up_before = std::fs::read_to_string(&item_up).expect("the item migration");
    let (outcome, document) = generate_into(&project, &["auth", "--overwrite-unchanged"]);
    assert!(outcome.succeeded, "{document}\n{}", outcome.output);
    assert!(project.join("src/auth.rs").is_file());
    assert!(project.join("config/auth.toml").is_file());
    let manifest = std::fs::read_to_string(project.join("renvor.toml")).expect("renvor.toml");
    assert!(manifest.contains("auth = \"session\""), "{manifest}");
    let files = document["result"]["files"].as_array().expect("files");
    let action_of = |path: &str| {
        files
            .iter()
            .find(|f| f["path"] == path)
            .map(|f| f["action"].as_str().unwrap_or("").to_owned())
    };
    assert_eq!(
        action_of("src/main.rs").as_deref(),
        Some("regenerate"),
        "the untouched main.rs is regenerated: {document}"
    );
    assert_eq!(
        action_of("Cargo.lock").as_deref(),
        Some("edit"),
        "the lockfile is resolved for the auth dependencies: {document}"
    );
    assert!(
        files.iter().all(|f| !f["path"]
            .as_str()
            .unwrap_or("")
            .starts_with("migrations/0001_")),
        "an applied migration was planned again: {document}"
    );
    assert!(
        files.iter().any(|f| f["path"]
            .as_str()
            .unwrap_or("")
            .ends_with("_add_item_owner.up.sql")),
        "no forward migration adds the owner column: {document}"
    );
    assert_eq!(
        std::fs::read_to_string(&item_up).expect("still there"),
        item_up_before,
        "the applied item migration was rewritten"
    );
    assert_eq!(
        action_of("src/resources/post.rs").as_deref(),
        Some("regenerate"),
        "the resource is rendered again with its guards: {document}"
    );
    let post_after = std::fs::read_to_string(&module).expect("module");
    assert!(post_after.contains("require_session"), "{post_after}");

    let (outcome, again) = generate_into(&project, &["auth"]);
    assert!(outcome.succeeded, "{again}");
    assert_eq!(
        again["result"]["written"], 0,
        "a rerun wrote something: {again}"
    );

    // 5. The lockfile the command wrote is the one the build resolves.
    let locked = run("cargo", &["build", "--locked"], &project, &[]);
    assert!(
        locked.succeeded,
        "`cargo build --locked` failed after the auth starter was added [{}]:\n{}",
        locked.status, locked.output
    );

    // 6. Against the ledger the pre-auth run left: the item migration's checksum is unchanged,
    //    and the auth set and the owner column apply forward.
    if migrated {
        assert!(boots_against_the_existing_ledger(&project, row.needs));
    }

    // 7. The regenerated tests: the starter's, and the resource's, which now refuses a write
    //    without a session.
    checks_after_generation(&project, "starter", row.needs);
    checks_after_generation(&project, "post", row.needs);
}

#[test]
fn the_auth_starter_added_to_a_starter_proves_itself() {
    the_auth_starter_added_later("authadded", "postgres", "sqlx");
}

#[test]
fn the_auth_starter_added_to_a_mysql_seaorm_starter_proves_itself() {
    // The forward owner migration is engine-specific SQL, so the upgrade is proven on the
    // other engine — with the other persistence model — as well.
    the_auth_starter_added_later("authaddedmysql", "mysql", "seaorm");
}

#[test]
fn the_auth_starter_is_refused_where_new_would_refuse_it() {
    // The same combination rules as `renvor new --auth session`: no `mail`, no starter.
    let _serial = serial();
    if !row_selected("authrefused") {
        return;
    }
    let row = Row {
        name: "authrefused",
        database: Some("postgres"),
        orm: "sqlx",
        flags: &["--capabilities", "storage"],
        needs: &[],
    };
    let base = tempfile::tempdir().expect("tempdir");
    let (project, _) = generate(base.path(), &row);
    let (outcome, refused) = generate_into(&project, &["auth"]);
    assert!(!outcome.succeeded, "{refused}");
    assert_eq!(
        refused["error"]["code"], "unsupported_combination",
        "{refused}"
    );
    assert!(
        !project.join("src/auth.rs").exists(),
        "a refusal wrote the starter"
    );
}

// ────────────────────────────────────────── the legacy-tree selection control

/// The second toolchain this control selects with, or `None` after saying why there is none.
///
/// The same gate `toolchain_selection.rs` applies, for the same reason: a selection rule needs two
/// toolchains to be about anything, `RENVOR_TEST_CONTROL_TOOLCHAIN` names the second, and
/// `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` — which the `verify` legs set at job level, and which
/// therefore reaches the census this file runs in — turns a skip into a failure. Installed-ness is
/// read from the filesystem: `rustup toolchain list` is what SR-012-4 forbids, and on a rustup
/// that auto-installs, asking is not a read-only question.
fn control_toolchain() -> Option<String> {
    let required = std::env::var("RENVOR_TEST_REQUIRE_TOOLCHAINS").is_ok_and(|v| v.trim() == "1");
    let named = match std::env::var("RENVOR_TEST_CONTROL_TOOLCHAIN") {
        Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
        _ => {
            assert!(
                !required,
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but RENVOR_TEST_CONTROL_TOOLCHAIN is not set"
            );
            println!("SKIPPED: RENVOR_TEST_CONTROL_TOOLCHAIN is not set");
            return None;
        }
    };
    let home = std::env::var_os("RUSTUP_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join(".rustup"))
        });
    let installed = home.is_some_and(|home| {
        std::fs::read_dir(home.join("toolchains")).is_ok_and(|entries| {
            let prefix = format!("{named}-");
            entries.filter_map(Result::ok).any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|found| found == named || found.starts_with(prefix.as_str()))
            })
        })
    });
    if installed {
        return Some(named);
    }
    assert!(
        !required,
        "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but RENVOR_TEST_CONTROL_TOOLCHAIN names a toolchain that is not installed"
    );
    println!("SKIPPED: RENVOR_TEST_CONTROL_TOOLCHAIN names a toolchain that is not installed");
    None
}

/// What `rustc +<toolchain> -vV` calls its release. `RUSTUP_AUTO_INSTALL=0`: a probe that installs
/// what it was looking for is not an observation (SR-012-1), and the name has already been read as
/// installed from the filesystem.
fn release_of(toolchain: &str) -> Option<String> {
    let output = Command::new("rustc")
        .arg(format!("+{toolchain}"))
        .arg("-vV")
        .env("RUSTUP_AUTO_INSTALL", "0")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("release:")
                .map(|value| value.trim().to_owned())
        })
}

/// A file's SHA-256 in the hex form the provenance record uses, from whichever hasher this
/// machine has. See `toolchain_selection.rs` for why this is a child process and not a crate.
fn sha256_of(path: &Path) -> Option<String> {
    for (program, arguments) in [("sha256sum", &[][..]), ("shasum", &["-a", "256"][..])] {
        let Ok(output) = Command::new(program).args(arguments).arg(path).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let hex = text.split_whitespace().next()?.to_owned();
        if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hex);
        }
    }
    None
}

/// Turns a freshly generated starter into a **pin-less legacy tree**, as a generator older than
/// record version 2 left one.
///
/// # Why a downgraded record and not the template-7 fixture
///
/// The generator's only signal for "legacy" is a provenance record with no `record_version`
/// (`commands::generate::run_generate`), so a current starter whose record has that line removed
/// is legacy by every predicate the code applies — and, unlike a Phase-011 fixture, it is a tree
/// this framework can still build, which is what `generate auth` has to do here. What is being
/// measured is the toolchain the tree selects and the files a legacy tree is given, neither of
/// which depends on how old its sources are. The template-7 fixture keeps its own job in
/// `legacy_compatibility.rs`, where nothing is built.
///
/// Three things change together, so the tree is consistent with what it claims: the record loses
/// its version marker and its two evidence tables, `rust-toolchain.toml` is removed from the tree
/// **and** from the record's file list, and `Cargo.toml` loses its `rust-version` line with its
/// digest recorded again — otherwise the manifest would read as changed since generation and the
/// conflict check would refuse before anything resolved.
fn downgrade_to_a_legacy_tree(project: &Path) -> bool {
    let pin = project.join("rust-toolchain.toml");
    assert!(pin.is_file(), "the starter did not pin a toolchain");
    std::fs::remove_file(&pin).expect("the pin is removed");

    let manifest_path = project.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).expect("Cargo.toml");
    let without: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with("rust-version"))
        .map(|line| format!("{line}\n"))
        .collect();
    assert_ne!(manifest, without, "the starter declared no `rust-version`");
    std::fs::write(&manifest_path, &without).expect("writable");
    let Some(digest) = sha256_of(&manifest_path) else {
        println!("SKIPPED: no sha256 hasher, so a legacy tree cannot be prepared");
        return false;
    };

    // The record, block by block. A blank line separates blocks, and a block is dropped whole
    // when its header is one of the two evidence tables — `[verified_with.checks.fmt]` and its
    // siblings are `[verified_with…]` too, and a filter that matched only the exact header left
    // them behind for a reader that then refused the file.
    let record_path = project.join(".renvor").join("generated.toml");
    let record = std::fs::read_to_string(&record_path).expect("the record");
    let mut blocks: Vec<Vec<String>> = vec![Vec::new()];
    for line in record.lines() {
        if line.is_empty() {
            blocks.push(Vec::new());
        } else if !line.starts_with("record_version = ") {
            blocks.last_mut().expect("a block").push(line.to_owned());
        }
    }
    let kept: Vec<String> = blocks
        .into_iter()
        .filter(|block| {
            let header = block.first().map(String::as_str).unwrap_or("");
            !(header.starts_with("[toolchain]")
                || header.starts_with("[verified_with")
                || block
                    .iter()
                    .any(|line| line == "path = \"rust-toolchain.toml\""))
        })
        .map(|block| block.join("\n"))
        .filter(|block| !block.is_empty())
        .collect();
    let rewritten = kept.join("\n\n").replace(
        &format!(
            "path = \"Cargo.toml\"\nsha256 = \"{}\"",
            record
                .lines()
                .skip_while(|line| *line != "path = \"Cargo.toml\"")
                .nth(1)
                .and_then(|line| line.strip_prefix("sha256 = \""))
                .and_then(|line| line.strip_suffix('"'))
                .expect("the record digests Cargo.toml")
        ),
        &format!("path = \"Cargo.toml\"\nsha256 = \"{digest}\""),
    );
    std::fs::write(&record_path, format!("{rewritten}\n")).expect("writable");
    assert!(
        !rewritten.contains("record_version")
            && !rewritten.contains("[toolchain]")
            && !rewritten.contains("rust-toolchain.toml"),
        "the record still describes a declared tree:\n{rewritten}"
    );
    true
}

/// `renvor generate …` in `project`, with the **inherited toolchain selection removed**.
///
/// `cargo` is launched through a rustup proxy, and the proxy exports `RUSTUP_TOOLCHAIN` (and
/// `RUSTUP_TOOLCHAIN_SOURCE`) into this test binary — the highest-precedence selection rustup
/// knows, inherited by every child. A control about what a *file* selects has to remove it, or it
/// measures the environment and passes while proving nothing. `toolchain_selection.rs` says the
/// same thing at greater length; this is the one place in this file that needs it.
fn generate_into_with_the_tree_deciding(project: &Path, args: &[&str]) -> (Run, serde_json::Value) {
    let mut full = vec!["generate"];
    full.extend_from_slice(args);
    full.extend_from_slice(&["--output", "json"]);
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(&full)
        .current_dir(project)
        .env("CARGO_TARGET_DIR", target_dir())
        .env("CARGO_INCREMENTAL", "0")
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTUP_TOOLCHAIN_SOURCE")
        .output()
        .expect("the generator runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let outcome = Run {
        succeeded: output.status.success(),
        status: format!("{}", output.status),
        output: format!("{stdout}{stderr}"),
        stdout,
    };
    let document: serde_json::Value = serde_json::from_str(&outcome.stdout).unwrap_or_else(|_| {
        panic!(
            "not a JSON envelope for generate {args:?} [{}]:\n{}",
            outcome.status, outcome.output
        )
    });
    (outcome, document)
}

/// **C-sel-3.** A legacy, pin-less project under an ancestor that pins `Z`: `generate auth`
/// resolves `Z` in the project directory **and** in the sibling scratch copy, agrees, verifies,
/// and records what it observed — and inserts no pin on that run or on the next.
///
/// # The two halves, and why they are one test
///
/// *Selection.* FR-012-12 puts the scratch copy **beside** the project so that both share every
/// ancestor and the resolution measured in one is the resolution of the other. A copy under the
/// system temporary directory — the shape before that requirement — would have resolved whatever
/// the default is, and FR-012-13's divergence check would have failed the run. That the two agree
/// on `Z`, when `Z` is neither the default nor anything the project states, is the positive
/// measurement; C-sel-2 is its negative twin, where an override on one directory alone makes the
/// same check fire.
///
/// *No insertion, twice.* FR-012-10a/b. The run writes `[toolchain]` `none` twice — the honest
/// record of a tree that declares nothing — and the **next** `auth` reads that record. A predicate
/// reading "a `[toolchain]` table exists" as "declares a pin" flips exactly there, so the repeat
/// is where a pin would appear if it were going to, and the applied migrations are checked again
/// with it.
///
/// Gated on the control toolchain like every other selection control, and on the services the
/// verification needs. `Z` is [`control_toolchain`]'s release, which the `verify` legs provision
/// as the other leg's (U-10) — a release the ancestor names and nothing else does.
#[test]
fn c_sel_3_a_legacy_tree_resolves_its_ancestor_and_stays_pin_less_across_repeated_auth() {
    let _serial = serial();
    if !row_selected("legacyauth") {
        return;
    }
    let Some(ancestor_toolchain) = control_toolchain() else {
        return;
    };
    let row = Row {
        name: "legacyauth",
        database: Some("postgres"),
        orm: "sqlx",
        flags: &["--capabilities", "mail", "--example-domain"],
        needs: &[Service::Postgres, Service::Smtp],
    };
    let base = tempfile::tempdir().expect("tempdir");
    let (project, _) = generate(base.path(), &row);
    if !downgrade_to_a_legacy_tree(&project) {
        return;
    }

    // THE ANCESTOR'S PIN, written after the project is generated so that `renvor new` above ran
    // under the ordinary selection and only `generate auth` sees this one.
    std::fs::write(
        base.path().join("rust-toolchain.toml"),
        format!("[toolchain]\nchannel = \"{ancestor_toolchain}\"\n"),
    )
    .expect("the ancestor pin is written");
    let resolved = release_of(&ancestor_toolchain).expect("the ancestor's rustc answers -vV");

    let (outcome, document) =
        generate_into_with_the_tree_deciding(&project, &["auth", "--overwrite-unchanged"]);
    assert!(outcome.succeeded, "{document}\n{}", outcome.output);

    // THE SELECTION. Both resolutions agreed — the run would have refused otherwise — and what
    // they agreed on is the ancestor's release, attributed to the file that names it.
    let record = std::fs::read_to_string(project.join(".renvor").join("generated.toml"))
        .expect("the record");
    let field = |table: &str, key: &str| -> String {
        let parsed: toml::Value = toml::from_str(&record).expect("the record parses");
        parsed
            .get(table)
            .and_then(|table| table.get(key))
            .and_then(toml::Value::as_str)
            .unwrap_or_else(|| panic!("the record has no `{table}.{key}`:\n{record}"))
            .to_owned()
    };
    assert_eq!(
        field("verified_with", "resolved_rustc_release"),
        resolved,
        "the ancestor's toolchain file did not select the compiler"
    );
    assert_eq!(
        field("verified_with", "selected_by"),
        "toolchain_file",
        "the resolution was not attributed to a toolchain file"
    );
    assert_eq!(field("verified_with", "operation"), "auth");

    // NO INSERTION, and the record says so rather than saying nothing.
    assert!(
        !project.join("rust-toolchain.toml").exists(),
        "a pin was inserted into a project that declares none"
    );
    let cargo = std::fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    assert!(
        !cargo.contains("rust-version"),
        "a `rust-version` line was inserted into a legacy manifest:\n{cargo}"
    );
    assert_eq!(field("toolchain", "pinned"), "none");
    assert_eq!(field("toolchain", "rust_version"), "none");

    // THE REPEAT. The record now carries `[toolchain]`, and the tree is still undeclared.
    let item_up = project.join("migrations/0001_create_item.up.sql");
    let item_up_before = std::fs::read_to_string(&item_up).expect("the item migration");
    let (outcome, again) =
        generate_into_with_the_tree_deciding(&project, &["auth", "--overwrite-unchanged"]);
    assert!(outcome.succeeded, "{again}\n{}", outcome.output);
    let planned: Vec<&str> = again["result"]["files"]
        .as_array()
        .expect("files")
        .iter()
        .filter_map(|entry| entry["path"].as_str())
        .collect();
    assert!(
        !planned.contains(&"rust-toolchain.toml"),
        "the second auth planned a pin file for a tree that declares none: {planned:?}"
    );
    assert!(
        !project.join("rust-toolchain.toml").exists(),
        "the second auth inserted a pin"
    );
    let cargo = std::fs::read_to_string(project.join("Cargo.toml")).expect("Cargo.toml");
    assert!(
        !cargo.contains("rust-version"),
        "the second auth inserted a `rust-version` line:\n{cargo}"
    );
    assert!(
        planned
            .iter()
            .all(|path| !path.starts_with("migrations/0001_")),
        "an applied migration was planned again: {planned:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&item_up).expect("still there"),
        item_up_before,
        "the applied item migration was rewritten"
    );
    assert_eq!(again["result"]["toolchain"]["pinned"], "none", "{again}");
    assert_eq!(
        again["result"]["verified_with"]["operation"], "auth",
        "{again}"
    );

    // And the tree the two runs left still proves itself.
    checks_after_generation(&project, "starter", row.needs);
}
