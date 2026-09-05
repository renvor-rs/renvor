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
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock, PoisonError};

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
    output: String,
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
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Run {
        succeeded: output.status.success(),
        status: format!("{}", output.status),
        output: combined,
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
    let document: serde_json::Value = serde_json::from_str(&outcome.output).unwrap_or_else(|_| {
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
    for capability in ["cache", "jobs", "mail", "storage", "observability"] {
        let chosen = selected(row).contains(&capability);
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
        outcome.output.contains("test result: ok. 1 passed"),
        "the placed project's test did not run to a pass for {}:\n{}",
        row.name,
        outcome.output
    );
    true
}

/// Every file under `root`, sorted, with its bytes.
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
                files.push((relative, std::fs::read(&path).expect("read")));
            }
        }
    }
    files.sort();
    files
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
        let document: serde_json::Value = serde_json::from_str(&outcome.output)
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
    assert_eq!(
        dry_document["result"]["manifest"], real_document["result"]["manifest"],
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
