//! Renvor verification runner.
//!
//! One command, one behaviour, locally and in automation:
//!
//! ```text
//! cargo xtask verify
//! ```
//!
//! CI invokes this same entry point. Duplicated shell steps in workflow files are how
//! local and automated verification silently diverge, and divergence is how a skipped
//! check gets reported as a pass.
//!
//! The step list and exit codes are fixed by
//! `contracts/verification-sequence.md`.

use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Every step ran and passed.
const EXIT_OK: i32 = 0;
/// A step ran and failed.
const EXIT_STEP_FAILED: i32 = 1;
/// A required toolchain or database prerequisite is missing; no steps ran.
const EXIT_TOOLING_MISSING: i32 = 2;
/// The working tree was dirty after an otherwise successful run (step 9).
const EXIT_DIRTY_TREE: i32 = 3;

/// Total number of steps in the sequence, used only for progress output.
const TOTAL_STEPS: usize = 9;

/// A tool the sequence needs, how to detect it, and how to install it.
struct Tool {
    /// Program to execute for the probe.
    program: &'static str,
    /// Arguments that make the program report its version and exit 0.
    probe: &'static [&'static str],
    /// Human-readable name used in the missing-tooling report.
    name: &'static str,
    /// Which step needs it, so the operator knows what is at stake.
    purpose: &'static str,
    /// The exact command that installs it.
    install: &'static str,
}

/// An environment prerequisite step 4's four-row census cannot run without.
///
/// Separate from [`Tool`] because the remedy is different in kind: a missing tool is installed
/// once, and a missing database has to be started and pointed at. Reported in the same step-1
/// block, with the same exit code, because from the operator's side both are the same sentence —
/// *verification cannot run yet, and here is exactly what is absent*.
struct Prerequisite {
    /// The environment variable that must be set and non-empty.
    variable: &'static str,
    /// Which step needs it, so the operator knows what is at stake.
    purpose: &'static str,
    /// What to do about it. Never a value, and never a DSN: an example connection string in this
    /// file would be an example credential in this file.
    setup: &'static str,
}

/// The database environment step 4's census requires.
///
/// # Why this is a prerequisite rather than a condition
///
/// `contracts/verification-sequence.md` states of this sequence: *"Executed in order. None is
/// conditional. None is skipped."* Until Phase 008's correction cycle the census contradicted it —
/// with these variables absent it printed `ok — NOT RUN` and returned success, so a full
/// `cargo xtask verify` could exit **0** having executed none of the row-suite pairs. That is the
/// precise shape of the failure this repository has already been bitten by three times: a check
/// that did not run, reported as a check that passed.
///
/// The four rows of `PLAN.md` §10.1 are not optional, so their evidence is not optional either.
/// A contributor without databases now gets exit **2** and instructions, which is a refusal to
/// verify rather than a verification that quietly covered less.
///
/// **Nothing here starts a service.** Exit 2 names what is missing; it does not go and fix it.
/// A gate that silently provisions its own dependencies is a gate whose passing says nothing
/// about the machine it ran on.
const DATABASE_REQUIRED: &[Prerequisite] = &[
    Prerequisite {
        variable: "RENVOR_TEST_POSTGRES_URL",
        purpose: "the four-row persistence census, step 4",
        setup: "start a PostgreSQL the suite may create and drop tables in, and set this to its \
                connection string — see CONTRIBUTING.md, `Databases you need`",
    },
    Prerequisite {
        variable: "RENVOR_TEST_MYSQL_URL",
        purpose: "the four-row persistence census, step 4",
        setup: "start a MySQL the suite may create and drop tables in, and set this to its \
                connection string — see CONTRIBUTING.md, `Databases you need`",
    },
    Prerequisite {
        variable: "RENVOR_TEST_REQUIRE_DATABASE",
        purpose: "turns a skipped real-database test into a failure, steps 4 and 7",
        setup: "set it to `1`. Without it the test harness SKIPS every real-database test and \
                still prints `ok`, which is the condition this variable exists to end",
    },
    // THE CAPABILITY ENDPOINTS (Phase 010, FR-104). The cache and mail adapters have real-server
    // suites that skip quietly without their URL — and print `ok`. The require flag ends that
    // the same way the database one does.
    //
    // A CREDENTIAL IS ITS OWN VARIABLE, never part of a URL: constitution VI says a secret enters
    // no URL, and the suites refuse a `…_URL` that carries one. The gate requires the credential
    // variables so that the authenticated path — the one production uses — is what it exercises.
    Prerequisite {
        variable: "RENVOR_TEST_VALKEY_URL",
        purpose: "the cache adapter's real-server suite, step 4",
        setup: "start a Valkey (or Redis) the suite may write to and set this to its \
                `redis://host:port/db` URL WITHOUT a credential — see CONTRIBUTING.md, \
                `Databases you need`",
    },
    Prerequisite {
        variable: "RENVOR_TEST_VALKEY_PASSWORD",
        purpose: "the cache adapter's real-server suite authenticates with it, step 4",
        setup: "set this to the Valkey password (`requirepass`); the URL must not carry it",
    },
    Prerequisite {
        variable: "RENVOR_TEST_SMTP_URL",
        purpose: "the mail adapter's real-sink suite, step 4",
        setup: "start a Mailpit and set this to its `smtp://127.0.0.1:port` URL WITHOUT a \
                credential — see CONTRIBUTING.md, `Databases you need`",
    },
    Prerequisite {
        variable: "RENVOR_TEST_SMTP_USERNAME",
        purpose: "the mail adapter's real-sink suite authenticates with it, step 4",
        setup: "set this to the Mailpit SMTP username (`MP_SMTP_AUTH`'s user half)",
    },
    Prerequisite {
        variable: "RENVOR_TEST_SMTP_PASSWORD",
        purpose: "the mail adapter's real-sink suite authenticates with it, step 4",
        setup: "set this to the Mailpit SMTP password; the URL must not carry it",
    },
    Prerequisite {
        variable: "RENVOR_TEST_SMTP_API_URL",
        purpose: "the mail adapter's real-sink suite reads delivered messages back, step 4",
        setup: "set this to the Mailpit HTTP API, `http://127.0.0.1:8025`",
    },
    Prerequisite {
        variable: "RENVOR_TEST_REQUIRE_CAPABILITIES",
        purpose: "turns a skipped real-server capability test into a failure, step 4",
        setup: "set it to `1`. Without it the cache and mail suites SKIP without a server and \
                still print `ok`",
    },
];

/// Everything step 1 probes for. Order matches the step that consumes each tool.
const REQUIRED: &[Tool] = &[
    Tool {
        program: "git",
        probe: &["--version"],
        name: "git",
        purpose: "secret scanning and working-tree cleanliness, steps 8 and 9",
        install: "https://git-scm.com/downloads",
    },
    Tool {
        program: "cargo",
        probe: &["fmt", "--version"],
        name: "rustfmt",
        purpose: "formatting, step 2",
        install: "rustup component add rustfmt",
    },
    Tool {
        program: "cargo",
        probe: &["clippy", "--version"],
        name: "clippy",
        purpose: "lint, step 3",
        install: "rustup component add clippy",
    },
    Tool {
        program: "cargo",
        probe: &["deny", "--version"],
        name: "cargo-deny",
        purpose: "dependency and licence policy, step 6",
        install: "cargo install cargo-deny --locked",
    },
    Tool {
        program: "gitleaks",
        probe: &["version"],
        name: "gitleaks",
        purpose: "secret scan, step 8",
        install: "brew install gitleaks   (or see github.com/gitleaks/gitleaks)",
    },
];

fn main() -> std::process::ExitCode {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("verify") => std::process::ExitCode::from(verify() as u8),
        Some(other) => {
            eprintln!("error: unknown task `{other}`");
            usage();
            std::process::ExitCode::from(EXIT_STEP_FAILED as u8)
        }
        None => {
            usage();
            std::process::ExitCode::from(EXIT_STEP_FAILED as u8)
        }
    }
}

fn usage() {
    eprintln!("usage: cargo xtask verify");
    eprintln!();
    eprintln!("Runs the full verification sequence. Exit codes:");
    eprintln!("  0  every step ran and passed");
    eprintln!("  1  a step ran and failed");
    eprintln!("  2  a required tool or the database environment is missing; no steps ran");
    eprintln!("  3  the working tree was dirty after a successful run");
}

/// Runs the sequence and returns the process exit code.
fn verify() -> i32 {
    let root = workspace_root();
    let environment = |name: &str| std::env::var_os(name);

    // ---- Step 1: prerequisite probe. Fail closed, before anything else runs. ----
    //
    // Tools AND the database environment. The second half was added in Phase 008's correction
    // cycle: step 4's census used to report `ok — NOT RUN` without it and let the whole sequence
    // exit 0, which is a conditional step in a sequence whose contract says it has none.
    let missing_tools = probe_tooling();
    let missing_databases = missing_database_prerequisites(&environment);
    if let Some(code) = prerequisites_gate(&missing_tools, &missing_databases) {
        report_missing(&missing_tools, &missing_databases);
        return code;
    }
    step_ok(
        1,
        "prerequisite probe",
        "all required tooling present, and the four-row database environment is set",
    );

    // ---- Steps 2-5: Rust ----
    if !run(
        2,
        "formatting",
        "cargo",
        &["fmt", "--all", "--check"],
        &root,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }
    if !run(
        3,
        "lint",
        "cargo",
        &[
            "clippy",
            "--all-targets",
            "--all-features",
            "--",
            "-D",
            "warnings",
        ],
        &root,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }
    if !run(
        4,
        "tests",
        "cargo",
        &["test", "--workspace", "--all-features"],
        &root,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }
    // STILL STEP 4. The end-to-end route-relay proof is `#[ignore]`d because it invokes a nested
    // `cargo run` to build and execute a second crate's example, which would contend with the
    // build lock held by the test process that spawned it. `--ignored` is therefore not a
    // convenience here; it is the only safe way to run it.
    //
    // It ran in NO automated gate until 2026-08-23. `cargo test` skipped it and CI skipped it, so
    // every relay assertion that actually executed fed the CLI a payload a human had written. A
    // change to `answer_dump_request`'s envelope would have broken the real protocol and left the
    // suite green. Named as its own line of output, counted inside step 4, so the sequence stays
    // at nine steps.
    if !the_end_to_end_relay_ran(&root) {
        return EXIT_STEP_FAILED;
    }
    // STILL STEP 4. Four rows are specified by `PLAN.md` §10.1; nothing until now asserted that
    // four of them ran. See `the_four_rows_all_ran`.
    if !the_four_rows_all_ran(&root, &environment) {
        return EXIT_STEP_FAILED;
    }
    // Warnings are denied via RUSTDOCFLAGS; a broken intra-doc link is a failure.
    if !run(
        5,
        "API documentation",
        "cargo",
        &["doc", "--workspace", "--all-features", "--no-deps"],
        &root,
        &[("RUSTDOCFLAGS", "-D warnings")],
    ) {
        return EXIT_STEP_FAILED;
    }

    // ---- Step 6: dependency and licence policy ----
    if !run(
        6,
        "dependency and licence policy",
        "cargo",
        &["deny", "check"],
        &root,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }

    // ---- Step 7: architecture invariants ----
    //
    // Five claims the project makes that are otherwise only assertions in prose. Each is checked
    // against the RESOLVED graph, a real compile, the actual manifests, or the actual document
    // text — and each carries a positive control: a query that must find what the first query must
    // not, so a check that silently stopped working is caught rather than reported as a pass.
    if !architecture_invariants(&root) {
        return EXIT_STEP_FAILED;
    }

    // ---- Step 8: secret scan ----
    // `gitleaks detect` was REMOVED in Gitleaks 8.x. The history scanner is now
    // `gitleaks git`, and the working-tree scanner is `gitleaks dir`. Both run:
    // the history scan cannot see uncommitted files, and the directory scan cannot
    // see deleted-but-committed ones. Neither substitutes for the other.
    if !run(
        8,
        "secret scan (history)",
        "gitleaks",
        &["git", ".", "--no-banner"],
        &root,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }
    if !run(
        8,
        "secret scan (working tree)",
        "gitleaks",
        &["dir", ".", "--no-banner"],
        &root,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }

    // ---- Step 9: working-tree cleanliness ----
    // This is what proves the ignore rules are correct rather than merely present.
    match dirty_entries(&root) {
        Err(message) => {
            step_fail(9, "working-tree cleanliness", &message);
            EXIT_STEP_FAILED
        }
        Ok(entries) if entries.is_empty() => {
            step_ok(
                9,
                "working-tree cleanliness",
                "no untracked or modified files",
            );
            println!();
            println!("verification passed: all {TOTAL_STEPS} steps ran and passed.");
            EXIT_OK
        }
        Ok(entries) => {
            step_fail(
                9,
                "working-tree cleanliness",
                "the working tree is not clean",
            );
            eprintln!();
            eprintln!(
                "Every check passed, but the run left or found {} entr{} that git",
                entries.len(),
                if entries.len() == 1 { "y" } else { "ies" }
            );
            eprintln!("reports as untracked or modified:");
            eprintln!();
            for e in entries.iter().take(40) {
                eprintln!("    {e}");
            }
            if entries.len() > 40 {
                eprintln!("    ... and {} more", entries.len() - 40);
            }
            eprintln!();
            eprintln!("Either the ignore rules are incomplete or a step writes into the tree.");
            EXIT_DIRTY_TREE
        }
    }
}

/// Returns the tools that are absent. Never short-circuits: the operator should see
/// everything missing in one pass, not discover them one reinstall at a time.
fn probe_tooling() -> Vec<&'static Tool> {
    REQUIRED.iter().filter(|t| !present(t)).collect()
}

/// Returns the database prerequisites that are absent or empty.
///
/// Takes the lookup rather than reading the process environment, so the decision this makes is
/// testable without a test mutating global state that its neighbours are reading concurrently.
fn missing_database_prerequisites(
    env: &dyn Fn(&str) -> Option<std::ffi::OsString>,
) -> Vec<&'static Prerequisite> {
    DATABASE_REQUIRED
        .iter()
        .filter(|p| env(p.variable).is_none_or(|value| value.is_empty()))
        .collect()
}

/// The step-1 verdict: `None` to proceed, or the exit code to return without running anything.
///
/// A named function rather than an inline `if`, so that *"a missing prerequisite is never exit 0"*
/// is a claim a test can hold rather than a shape a reader has to trust.
fn prerequisites_gate(tools: &[&Tool], databases: &[&Prerequisite]) -> Option<i32> {
    if tools.is_empty() && databases.is_empty() {
        None
    } else {
        Some(EXIT_TOOLING_MISSING)
    }
}

fn present(tool: &Tool) -> bool {
    Command::new(tool.program)
        .args(tool.probe)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The observable contract for exit code 2, fixed by the verification-sequence contract.
///
/// The final line is the point of this function. A partial run that reports success is
/// the failure mode the fail-closed rule exists to prevent.
fn report_missing(tools: &[&Tool], databases: &[&Prerequisite]) {
    eprintln!("error: verification cannot run — a required prerequisite is missing");
    eprintln!();
    for tool in tools {
        eprintln!("  missing tool: {} ({})", tool.name, tool.purpose);
        eprintln!("    install: {}", tool.install);
        eprintln!();
    }
    for prerequisite in databases {
        eprintln!(
            "  missing environment: {} ({})",
            prerequisite.variable, prerequisite.purpose
        );
        eprintln!("    setup: {}", prerequisite.setup);
        eprintln!();
    }
    if !databases.is_empty() {
        eprintln!("The four rows of PLAN.md §10.1 are not optional, so neither is their evidence.");
        eprintln!(
            "This is a refusal to verify, not a verification that covered less. Nothing here"
        );
        eprintln!("starts a database for you: what runs is what you provided.");
        eprintln!();
    }
    eprintln!("no checks were run. verification did not pass.");
}

/// Runs one step. Returns `true` on success, printing a labelled result either way.
fn run(
    number: usize,
    title: &str,
    program: &str,
    args: &[&str],
    dir: &Path,
    env_vars: &[(&str, &str)],
) -> bool {
    println!("[{number}/{TOTAL_STEPS}] {title} ...");

    let mut command = Command::new(program);
    command.args(args).current_dir(dir);
    for (key, value) in env_vars {
        command.env(OsStr::new(key), OsStr::new(value));
    }

    let started = Instant::now();
    let outcome = command.status();
    let elapsed = started.elapsed();

    match outcome {
        Ok(status) if status.success() => {
            step_ok(number, title, "passed");
            timing(&format!("step {number} {title}"), elapsed);
            true
        }
        Ok(status) => {
            let code = status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "terminated by signal".to_string());
            step_fail(number, title, &format!("`{program}` exited with {code}"));
            false
        }
        Err(error) => {
            // Reaching here means the probe passed but execution still failed —
            // a genuine anomaly worth reporting distinctly rather than as a lint failure.
            step_fail(
                number,
                title,
                &format!("could not execute `{program}`: {error}"),
            );
            false
        }
    }
}

/// Wall-clock, rendered the same way everywhere so two runs can be compared by eye.
///
/// `std::time::Instant` and nothing else. A verification sequence that needed a crate to time
/// itself would have to justify that crate in the dependency inventory it exists to police, so the
/// measurement is deliberately built from what the standard library already provides.
fn humanise(elapsed: Duration) -> String {
    let total = elapsed.as_secs();
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}h {minutes:02}m {seconds:02}s")
    } else {
        format!("{minutes}m {seconds:02}s")
    }
}

/// One measured span, printed on its OWN line.
///
/// The `[n/11] title: ok — detail` and `FAILED` lines are load-bearing: `contracts/`, CI log
/// scraping and every past evidence record quote them verbatim. Timing is therefore ADDITIVE —
/// a new line beside them — rather than a suffix that would change the shape of a line other
/// things parse. Nothing here can fail the sequence; it only reports.
fn timing(label: &str, elapsed: Duration) {
    println!("       timing  {label}: {}", humanise(elapsed));
}

fn step_ok(number: usize, title: &str, detail: &str) {
    println!("[{number}/{TOTAL_STEPS}] {title}: ok — {detail}");
}

fn step_fail(number: usize, title: &str, detail: &str) {
    eprintln!("[{number}/{TOTAL_STEPS}] {title}: FAILED — {detail}");
}

/// Entries `git status --porcelain` reports, one per line.
fn dirty_entries(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// The workspace root, derived from this crate's manifest directory.
///
/// `CARGO_MANIFEST_DIR` points at `xtask/`; the workspace root is its parent. Deriving
/// it this way means `cargo xtask verify` behaves identically from any subdirectory.
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask manifest directory always has a parent")
        .to_path_buf()
}

/// Step 7: the crate DAG, the facade's feature isolation, that the lean facade **compiles**, that
/// no publishable package carries an unresolvable dependency, and the SC-022 wording agreement.
///
/// Kept in one step because all five share a shape: **a claim, and a control that proves the check
/// can fail**. Splitting them would multiply the progress output without adding information.
fn architecture_invariants(root: &std::path::Path) -> bool {
    if !crate_dag_holds(root) {
        return false;
    }
    if !transport_isolation_holds(root) {
        return false;
    }
    if !facade_feature_isolation_holds(root) {
        return false;
    }
    if !persistence_isolation_holds(root) {
        return false;
    }
    if !adapters_compile_per_driver(root) {
        return false;
    }

    if !lean_facade_compiles(root) {
        return false;
    }
    if !rustls_has_one_crypto_provider(root) {
        return false;
    }
    if !publishable_dependencies_are_resolvable(root) {
        return false;
    }
    if !required_package_metadata_is_present(root) {
        return false;
    }
    if !instability_wording_agrees(root) {
        return false;
    }
    if !the_executable_is_named_renvor(root) {
        return false;
    }
    step_ok(
        7,
        "architecture invariants",
        "crate DAG, transport, persistence and capability isolation, per-driver adapter \
         compiles, facade isolation, lean compile, one rustls crypto provider, \
         publishable dependencies, required package metadata, instability wording, and the \
         executable name all hold, each with a control",
    );
    true
}

/// Phase 007 FR-002, FR-003, FR-032, FR-050, FR-056, FR-057: the persistence adapters are isolated.
///
/// # Why this lives here rather than in a script
///
/// It was a shell script. The script lived in a scratch directory, the FR conformance table cited
/// it as the evidence for six requirements, and **the scratch directory was deleted** — after which
/// six requirements were evidenced by a file that no longer existed. A review found it.
///
/// Evidence that is not run by a gate is not evidence. This runs inside `cargo xtask verify`.
///
/// Every claim carries a control, because a count of zero proves nothing without proof the walk can
/// see what IS there.
///
/// | Claim | Control |
/// |---|---|
/// | `db-postgres` alone resolves no MySQL or SQLite driver | it resolves `sqlx-postgres` |
/// | `db-mysql` alone resolves no PostgreSQL or SQLite driver | it resolves `sqlx-mysql` |
/// | neither adapter resolves the other | each resolves `renvor-database` |
/// | `renvor-database` resolves no driver and no ORM under **any** feature | it resolves itself |
/// | the banned crates are absent workspace-wide | the permitted near-namesakes are present |
/// | no crate resolves `jsonwebtoken` or `aws-lc-*` without `tokens` | the same crate WITH `tokens` resolves all three |
/// One isolation row: a label, the `cargo tree` arguments, what must be absent, what must be
/// present.
///
/// A named type because the tuple is four fields wide and clippy is right that the inline form is
/// unreadable — and because "forbidden" and "controls" are otherwise two anonymous `&[&str]`s a
/// reader has to count positions to tell apart.
type IsolationCheck<'a> = (&'a str, &'a [&'a str], &'a [&'a str], &'a [&'a str]);

fn persistence_isolation_holds(root: &std::path::Path) -> bool {
    /// Matches a crate at an entry boundary, so `sea-orm` never matches `renvor-seaorm` and `rsa`
    /// never matches `rsa-something`.
    fn resolves(tree: &str, name: &str) -> bool {
        tree.lines().any(|line| {
            line.split_once(' ')
                .is_some_and(|(package, rest)| package == name && rest.starts_with('v'))
        })
    }

    fn fail(detail: &str) -> bool {
        step_fail(7, "architecture invariants", detail);
        false
    }

    // Each row: the tree to build, then what must be absent, then the control that must be present.
    let checks: [IsolationCheck<'_>; 40] = [
        // ---- API TOKEN MODE (FR-035, SC-011) ----------------------------------------------
        //
        // Added in batch G2, and the reason is that this property was TRUE and UNASSERTED. Batch G
        // measured it by hand, recorded that it held, and left T039 at `todo` with the note that
        // "an unasserted property is not evidence". This is the assertion.
        //
        // It matters more than an ordinary feature boundary. `tokens` is the only thing in this
        // workspace that resolves `jsonwebtoken`, and behind it `aws-lc-sys` — which **compiles C
        // and assembly** and therefore imposes a toolchain on every consumer who resolves it. A
        // consumer who is not using API tokens must not pay that, and the only way to be sure is to
        // walk the graph.
        //
        // Every row has its mirror image: the same package WITH the feature, asserting the same
        // crates are present. Without that, a typo in a package name would make both the absence
        // and its control vacuous.
        (
            "renvor-auth (no tokens)",
            &["-p", "renvor-auth"],
            &["jsonwebtoken", "aws-lc-rs", "aws-lc-sys"],
            &["renvor-database", "subtle"],
        ),
        (
            "renvor-auth (--features tokens)",
            &["-p", "renvor-auth", "--features", "tokens"],
            &[],
            &["jsonwebtoken", "aws-lc-rs", "aws-lc-sys"],
        ),
        // BATCH I. `renvor-testkit` gained an `auth` feature so the abuse-control contract could be
        // hosted without the token half — and `renvor-sqlx` and `renvor-seaorm` now turn it on
        // UNCONDITIONALLY in their dev-dependencies. If `auth` reached `renvor-auth/tokens`, every
        // adapter's test build would resolve `aws-lc-sys` and compile C and assembly, whether or
        // not the consumer asked for API tokens. That would be the native-build cost `tokens`
        // exists to keep optional, reintroduced through the test side where nobody was looking.
        // BATCH J. `renvor-auth`'s manifest header has said since batch A that "there is no router
        // here, no status code, no `sqlx`, and no `sea-orm`" — and until now nothing checked the
        // first two. The claim is what forced `renvor-auth-http` to exist as a separate crate
        // rather than as an optional feature on either side, so it had better be true.
        //
        // `--all-features` deliberately: a feature that pulled a transport in would otherwise be
        // invisible here, which is precisely the evasion the placement decision refused.
        (
            "renvor-auth (--all-features) resolves no transport",
            &["-p", "renvor-auth", "--all-features"],
            &["axum", "hyper", "tower-http", "renvor-http"],
            &["renvor-database", "renvor-config"],
        ),
        (
            "renvor-auth-http resolves BOTH sides",
            &["-p", "renvor-auth-http"],
            &[],
            &[
                "renvor-auth",
                "renvor-http",
                "renvor-error",
                "renvor-openapi",
            ],
        ),
        (
            "renvor-testkit (--features auth)",
            &["-p", "renvor-testkit", "--features", "auth"],
            &["jsonwebtoken", "aws-lc-rs", "aws-lc-sys"],
            &["renvor-auth", "chrono"],
        ),
        (
            "renvor-testkit (--features tokens)",
            &["-p", "renvor-testkit", "--features", "tokens"],
            &[],
            &["jsonwebtoken", "aws-lc-rs", "aws-lc-sys", "renvor-auth"],
        ),
        (
            "renvor-sqlx + db-postgres (no tokens)",
            &[
                "-p",
                "renvor-sqlx",
                "--no-default-features",
                "--features",
                "db-postgres",
            ],
            &["jsonwebtoken", "aws-lc-rs", "aws-lc-sys"],
            &["sqlx", "renvor-auth"],
        ),
        (
            "renvor-sqlx + db-postgres,tokens",
            &[
                "-p",
                "renvor-sqlx",
                "--no-default-features",
                "--features",
                "db-postgres,tokens",
            ],
            &[],
            &["jsonwebtoken", "aws-lc-rs"],
        ),
        (
            "renvor-seaorm + db-postgres (no tokens)",
            &[
                "-p",
                "renvor-seaorm",
                "--no-default-features",
                "--features",
                "db-postgres",
            ],
            &["jsonwebtoken", "aws-lc-rs", "aws-lc-sys"],
            &["sea-orm", "renvor-auth"],
        ),
        (
            "renvor-seaorm + db-postgres,tokens",
            &[
                "-p",
                "renvor-seaorm",
                "--no-default-features",
                "--features",
                "db-postgres,tokens",
            ],
            &[],
            &["jsonwebtoken", "aws-lc-rs"],
        ),
        // The shared contract crate is a dev-dependency of both adapters, so its own `tokens`
        // feature is a third place the JWT backend could arrive from. `--edges normal` already
        // excludes dev-dependencies; this asserts the crate itself is clean rather than relying on
        // the walk's edge filter to hide it.
        (
            "renvor-testkit (no tokens)",
            &["-p", "renvor-testkit"],
            &["jsonwebtoken", "aws-lc-rs", "aws-lc-sys", "renvor-auth"],
            &["renvor-database", "renvor-core"],
        ),
        // ---- PERSISTENCE (Phase 007) -------------------------------------------------------
        (
            "renvor-seaorm + db-postgres",
            &[
                "-p",
                "renvor-seaorm",
                "--no-default-features",
                "--features",
                "db-postgres",
            ],
            &[
                "sqlx-mysql",
                "sqlx-sqlite",
                "webpki-roots",
                "rsa",
                "sea-schema",
                "sea-orm-cli",
                "sea-orm-migration",
                "renvor-sqlx",
            ],
            &[
                "sqlx-postgres",
                "sea-orm",
                "sea-query-sqlx",
                "renvor-database",
            ],
        ),
        (
            "renvor-seaorm + db-mysql",
            &[
                "-p",
                "renvor-seaorm",
                "--no-default-features",
                "--features",
                "db-mysql",
            ],
            &[
                "sqlx-postgres",
                "sqlx-sqlite",
                "webpki-roots",
                "rsa",
                "sea-schema",
                "renvor-sqlx",
            ],
            &["sqlx-mysql", "sea-orm", "sea-query-sqlx"],
        ),
        (
            "renvor-seaorm with no driver",
            &["-p", "renvor-seaorm", "--no-default-features"],
            &["sqlx-postgres", "sqlx-mysql", "sqlx-sqlite"],
            &["sea-orm", "sqlx"],
        ),
        // The direct-SQLx adapter had NO per-driver row until Phase 008. Its `--all-features`
        // row below forbids the other ORM but permits both drivers, so nothing asserted that a
        // project on direct SQLx and MySQL keeps PostgreSQL out of its graph — the very thing
        // "no optional capability silently adds a second database" is about. The SeaORM adapter
        // had these two rows from the start; this closes the asymmetry.
        (
            "renvor-sqlx + db-postgres",
            &[
                "-p",
                "renvor-sqlx",
                "--no-default-features",
                "--features",
                "db-postgres",
            ],
            &[
                "sqlx-mysql",
                "sqlx-sqlite",
                "sea-orm",
                "renvor-seaorm",
                "rsa",
                "webpki-roots",
            ],
            &["sqlx-postgres", "renvor-database"],
        ),
        (
            "renvor-sqlx + db-mysql",
            &[
                "-p",
                "renvor-sqlx",
                "--no-default-features",
                "--features",
                "db-mysql",
            ],
            &[
                "sqlx-postgres",
                "sqlx-sqlite",
                "sea-orm",
                "renvor-seaorm",
                "rsa",
                "webpki-roots",
            ],
            &["sqlx-mysql", "renvor-database"],
        ),
        (
            "renvor-sqlx (--all-features)",
            &["-p", "renvor-sqlx", "--all-features"],
            &["sea-orm", "renvor-seaorm"],
            &["sqlx", "renvor-database"],
        ),
        // `renvor-core` joined this row's forbidden list in Phase 008's correction cycle. The
        // ports crate depended on the kernel for exactly one thing — projecting
        // `DatabaseErrorKind` onto `ErrorCategory` — and that projection reported ordinary
        // database outcomes as `Internal`, which C-E1 reserves for a kernel defect. Removing it
        // left the dependency unused, and an unused edge between two crates is an edge somebody
        // eventually uses.
        //
        // The controls are `renvor-validation` and `renvor-error`, which this crate really does
        // resolve. The row previously controlled on `renvor-database` itself — the ROOT of its own
        // tree — which is present in every possible output and therefore proves nothing about
        // whether the walk can see a dependency.
        (
            "renvor-database (--all-features)",
            &["-p", "renvor-database", "--all-features"],
            &[
                "sea-orm",
                "sqlx",
                "renvor-seaorm",
                "renvor-sqlx",
                "renvor-core",
            ],
            &["renvor-validation", "renvor-error"],
        ),
        // ── THE PHASE 010 CAPABILITIES (FR-002, FR-006, FR-102) ──────────────────────────
        // Each port crate without its adapter feature resolves no adapter; each adapter feature
        // resolves its adapter and nothing banned; the kernel and the auth domain resolve no
        // adapter at all; a MySQL application choosing durable jobs acquires no PostgreSQL
        // driver; and the facade resolves exactly the capability it asked for. Every row carries
        // a control that must resolve, so an absence is a measured absence.
        (
            "renvor-cache (no valkey) resolves no client",
            &["-p", "renvor-cache"],
            &["redis", "renvor-http", "sqlx"],
            &["renvor-core", "tokio"],
        ),
        (
            "renvor-cache + valkey",
            &["-p", "renvor-cache", "--features", "valkey"],
            &["renvor-http", "webpki-roots", "rsa", "aws-lc-rs"],
            &["redis", "rustls", "ring"],
        ),
        (
            "renvor-mail (no smtp, no auth) resolves no transport and no auth",
            &["-p", "renvor-mail"],
            &["lettre", "renvor-auth", "renvor-http"],
            &["renvor-core", "renvor-config"],
        ),
        (
            "renvor-mail + smtp",
            &["-p", "renvor-mail", "--features", "smtp"],
            &[
                "renvor-auth",
                "webpki-roots",
                "rsa",
                "native-tls",
                "openssl",
                "boring",
            ],
            &["lettre", "rustls-native-certs", "ring"],
        ),
        (
            "renvor-mail + auth resolves the auth port and no transport",
            &["-p", "renvor-mail", "--features", "auth"],
            &["lettre", "renvor-http"],
            &["renvor-auth"],
        ),
        (
            "renvor-storage (no filesystem) resolves no capability crate",
            &["-p", "renvor-storage"],
            &["cap-std", "cap-tempfile", "object_store", "opendal"],
            &["renvor-core"],
        ),
        (
            "renvor-storage + filesystem",
            &["-p", "renvor-storage", "--features", "filesystem"],
            &[
                "object_store",
                "opendal",
                "aws-sdk-s3",
                "rust-s3",
                "reqwest",
            ],
            &["cap-std", "cap-tempfile"],
        ),
        (
            "renvor-jobs resolves no driver and no adapter",
            &["-p", "renvor-jobs", "--all-features"],
            &["sqlx", "sea-orm", "redis", "lettre", "renvor-http"],
            &["renvor-core", "tokio"],
        ),
        (
            "renvor-observability (no otel, no http)",
            &["-p", "renvor-observability"],
            &[
                "opentelemetry",
                "opentelemetry_sdk",
                "hyper",
                "renvor-http",
                "reqwest",
            ],
            &["tracing-subscriber", "tracing-serde", "serde_json"],
        ),
        (
            "renvor-observability + otel",
            &["-p", "renvor-observability", "--features", "otel"],
            &[
                "renvor-http",
                "webpki-roots",
                "reqwest",
                "tonic",
                "rustls-platform-verifier",
            ],
            &[
                "opentelemetry",
                "opentelemetry-otlp",
                "hyper-rustls",
                "ring",
            ],
        ),
        (
            "renvor-observability + http resolves the transport and no exporter",
            &["-p", "renvor-observability", "--features", "http"],
            &["opentelemetry", "hyper-rustls"],
            &["renvor-http"],
        ),
        (
            "renvor-sqlx + db-mysql,jobs acquires no PostgreSQL driver",
            &[
                "-p",
                "renvor-sqlx",
                "--no-default-features",
                "--features",
                "db-mysql,jobs",
            ],
            &["sqlx-postgres", "sea-orm", "renvor-seaorm"],
            &["sqlx-mysql", "renvor-jobs"],
        ),
        (
            "renvor-seaorm + db-mysql,jobs acquires no PostgreSQL driver",
            &[
                "-p",
                "renvor-seaorm",
                "--no-default-features",
                "--features",
                "db-mysql,jobs",
            ],
            &["sqlx-postgres", "renvor-sqlx"],
            &["sqlx-mysql", "renvor-jobs", "sea-orm"],
        ),
        (
            "renvor-core (--all-features) resolves no adapter",
            &["-p", "renvor-core", "--all-features"],
            &[
                "redis",
                "lettre",
                "cap-std",
                "opentelemetry",
                "sqlx",
                "tracing-subscriber",
                "renvor-cache",
                "renvor-jobs",
            ],
            &["tracing", "tokio"],
        ),
        (
            "renvor-auth (--all-features) resolves no capability adapter",
            &["-p", "renvor-auth", "--all-features"],
            &["lettre", "redis", "cap-std", "opentelemetry", "renvor-mail"],
            &["renvor-database", "tracing"],
        ),
        (
            "renvor lean resolves no capability crate",
            &["-p", "renvor", "--no-default-features"],
            &[
                "renvor-cache",
                "renvor-jobs",
                "renvor-mail",
                "renvor-storage",
                "renvor-observability",
            ],
            &["renvor-core"],
        ),
        (
            "renvor + capability-cache resolves that crate alone",
            &[
                "-p",
                "renvor",
                "--no-default-features",
                "--features",
                "capability-cache",
            ],
            &[
                "renvor-jobs",
                "renvor-mail",
                "renvor-storage",
                "renvor-observability",
                "redis",
            ],
            &["renvor-cache"],
        ),
        (
            "renvor + capability-jobs resolves that crate alone",
            &[
                "-p",
                "renvor",
                "--no-default-features",
                "--features",
                "capability-jobs",
            ],
            &[
                "renvor-cache",
                "renvor-mail",
                "renvor-storage",
                "renvor-observability",
                "sqlx",
            ],
            &["renvor-jobs"],
        ),
        (
            "renvor + capability-mail resolves that crate alone",
            &[
                "-p",
                "renvor",
                "--no-default-features",
                "--features",
                "capability-mail",
            ],
            &[
                "renvor-cache",
                "renvor-jobs",
                "renvor-storage",
                "renvor-observability",
                "lettre",
            ],
            &["renvor-mail"],
        ),
        (
            "renvor + capability-storage resolves that crate alone",
            &[
                "-p",
                "renvor",
                "--no-default-features",
                "--features",
                "capability-storage",
            ],
            &[
                "renvor-cache",
                "renvor-jobs",
                "renvor-mail",
                "renvor-observability",
                "cap-std",
            ],
            &["renvor-storage"],
        ),
        (
            "renvor + observability resolves that crate and no exporter",
            &[
                "-p",
                "renvor",
                "--no-default-features",
                "--features",
                "observability",
            ],
            &[
                "renvor-cache",
                "renvor-jobs",
                "renvor-mail",
                "renvor-storage",
                "opentelemetry",
            ],
            &["renvor-observability"],
        ),
        (
            "renvor + observability-otel resolves the exporter",
            &[
                "-p",
                "renvor",
                "--no-default-features",
                "--features",
                "observability-otel",
            ],
            &[
                "renvor-cache",
                "renvor-jobs",
                "renvor-mail",
                "renvor-storage",
                "reqwest",
            ],
            &["renvor-observability", "opentelemetry-otlp", "hyper-rustls"],
        ),
    ];

    for (label, args, forbidden, controls) in checks {
        let Some(tree) = normal_edges(root, args) else {
            return fail(&format!("the `{label}` tree query failed"));
        };
        for name in forbidden {
            if resolves(&tree, name) {
                return fail(&format!(
                    "{label} resolves `{name}`, which persistence feature isolation forbids"
                ));
            }
        }
        for name in controls {
            if !resolves(&tree, name) {
                return fail(&format!(
                    "{label} does not resolve `{name}`, so the absences above prove nothing — the \
                     walk cannot see what IS there"
                ));
            }
        }
    }

    // The banned crates, workspace-wide and under every feature. `webpki-roots` fails the licence
    // allow-list; `rsa` carries RUSTSEC-2023-0071 with no fixed version.
    let Some(everything) = normal_edges(root, &["--workspace", "--all-features"]) else {
        return fail("the workspace-wide tree query failed");
    };
    for name in ["webpki-roots", "rsa"] {
        if resolves(&everything, name) {
            return fail(&format!(
                "`{name}` resolves somewhere in the workspace under --all-features"
            ));
        }
    }
    // CONTROL. `rustls-webpki` is a DIFFERENT crate from `webpki-roots` — the X.509 verifier, not
    // the root store — and `ring` and `rustls` are present. If the walk cannot see these, its
    // failure to see the two above means nothing.
    for name in ["rustls-webpki", "rustls", "ring"] {
        if !resolves(&everything, name) {
            return fail(&format!(
                "the workspace walk cannot see `{name}`, so the two absences above prove nothing"
            ));
        }
    }

    true
}

/// FR-001 and ADR-0010: the installed executable is named exactly `renvor`.
///
/// # Why this is a build gate rather than a convention
///
/// The executable name is the string a user types and a shell resolves. ADR-0010 makes it a
/// **compatibility promise**, and the package is called `renvor-cli` — so the default Cargo
/// behaviour, which names the binary after the package, produces the wrong answer. One deleted
/// `[[bin]]` block breaks every installed user's muscle memory and every script, silently, with a
/// green build.
///
/// # This is the SECOND mechanism, and the first one fires before it
///
/// `crates/renvor-cli/tests/generated.rs` uses `env!("CARGO_BIN_EXE_renvor")`, which fails to
/// compile if the executable is not named `renvor`. Measured 2026-08-18: renaming the binary to
/// `renvorx` fails **step 4 (tests)**, so step 7 is never reached.
///
/// So this check has **not** been demonstrated firing in isolation, and that is recorded rather
/// than implied. It is kept because the two mechanisms fail for different reasons — one on a
/// compile-time macro, one on the manifest text — and a future change that drops the integration
/// test would otherwise remove the only guard without anything noticing.
/// `rustls` is compiled with the `ring` provider and NO second one (ADR-0033 decision 6).
///
/// The RESP client calls `ClientConfig::builder()`, which panics when two providers are compiled
/// in; the SMTP client and the OTLP connector are told to use `ring` explicitly. A feature edge
/// enabling `aws-lc-rs` or `fips` anywhere in the workspace under `--all-features` would turn a
/// working cache adapter into a boot-time panic, so the edge is asserted absent — and `ring`
/// asserted present, so the walk is known to see feature edges at all.
fn rustls_has_one_crypto_provider(root: &std::path::Path) -> bool {
    let output = std::process::Command::new("cargo")
        .args([
            "tree",
            "--edges",
            "features",
            "--workspace",
            "--all-features",
            "--prefix",
            "none",
        ])
        .current_dir(root)
        .output()
        .ok();
    let Some(output) = output.filter(|o| o.status.success()) else {
        step_fail(
            7,
            "architecture invariants",
            "the feature-edge tree query for `rustls` failed",
        );
        return false;
    };
    let tree = String::from_utf8_lossy(&output.stdout);
    let feature = |name: &str| {
        tree.lines()
            .any(|line| line.trim_end_matches(" (*)") == format!("rustls feature \"{name}\""))
    };
    for banned in ["aws-lc-rs", "aws_lc_rs", "fips"] {
        if feature(banned) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`rustls` is compiled with the `{banned}` feature somewhere under \
                     --all-features; with `ring` also on, `ClientConfig::builder()` panics"
                ),
            );
            return false;
        }
    }
    if !feature("ring") {
        step_fail(
            7,
            "architecture invariants",
            "the feature walk cannot see `rustls feature \"ring\"`, so the absences above prove \
             nothing",
        );
        return false;
    }
    true
}

fn the_executable_is_named_renvor(root: &std::path::Path) -> bool {
    let manifest = root.join("crates/renvor-cli/Cargo.toml");
    let Ok(text) = std::fs::read_to_string(&manifest) else {
        step_fail(
            7,
            "architecture invariants",
            &format!("{} could not be read", manifest.display()),
        );
        return false;
    };

    // THE CLAIM.
    if !text.contains("[[bin]]") || !text.contains("name = \"renvor\"") {
        step_fail(
            7,
            "architecture invariants",
            "crates/renvor-cli/Cargo.toml does not declare `[[bin]] name = \"renvor\"`. Without \
             it Cargo names the executable after the package, `renvor-cli`, which breaks the \
             compatibility promise in ADR-0010 and FR-001",
        );
        return false;
    }

    // THE CONTROL. The check above passes for the wrong reason if `name = "renvor"` matches the
    // `[package]` name instead of the `[[bin]]` one — so assert the two are DIFFERENT strings and
    // that the package is the one that is not `renvor`. A check that cannot fail is not a check.
    if !text.contains("name = \"renvor-cli\"") {
        step_fail(
            7,
            "architecture invariants",
            "crates/renvor-cli/Cargo.toml no longer declares package `renvor-cli`, so the \
             executable-name check may be matching the package name rather than the `[[bin]]` \
             name and is no longer proving anything",
        );
        return false;
    }

    true
}

/// Runs a cargo subcommand quietly and reports only whether it succeeded.
///
/// Separate from [`run`] because these are *probes*: one of them is expected to fail, and a probe
/// that printed a step banner for its own expected failure would read as a broken run.
fn cargo_succeeds(root: &std::path::Path, args: &[&str]) -> bool {
    let started = Instant::now();
    let succeeded = std::process::Command::new("cargo")
        .args(args)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    // Every step-7 probe, timed individually. Several of them are real compiles under feature sets
    // no other step uses, which is exactly the kind of cost that hides inside a single step banner.
    // Reported whether the probe passed or failed, because a probe EXPECTED to fail still costs.
    timing(
        &format!("step 7 probe `cargo {}`", args.join(" ")),
        started.elapsed(),
    );
    succeeded
}

/// T111: the facade **compiles** without default features, for every target.
///
/// [`facade_feature_isolation_holds`] asks `cargo tree` what the graph resolves to. That is not
/// the same question as whether the code builds, and the difference is not academic: the
/// `configuration` example used `renvor::config` with no `required-features` declaration, so
/// `--no-default-features --all-targets` failed to compile while every tree query stayed green.
/// Resolving a graph is not compiling against it.
/// Step 7: every adapter **compiles** with each driver selected on its own.
///
/// # Resolving is not compiling, and the difference hid a real defect
///
/// `persistence_isolation_holds` reads `cargo tree`, which answers *what would be downloaded*. It
/// cannot answer *does this build*, and the two came apart: `renvor-sqlx` resolved cleanly with
/// only `db-mysql` and **failed to compile**, because a test helper named `sqlx::Postgres` from
/// inside a module gated on `any(db-postgres, db-mysql)`. Every test in that module was already
/// `db-postgres`-only, so with MySQL alone the module held a helper referring to a type that was
/// not there.
///
/// A project generated for MySQL on direct SQLx would have hit that the first time it ran the
/// adapter's own suite. Nothing detected it, because nothing had ever compiled that combination.
///
/// `--all-targets` is load-bearing here exactly as it is for the facade: the library alone
/// compiled fine, and only the test target failed.
fn adapters_compile_per_driver(root: &std::path::Path) -> bool {
    const ROWS: [(&str, &str); 4] = [
        ("renvor-sqlx", "db-postgres"),
        ("renvor-sqlx", "db-mysql"),
        ("renvor-seaorm", "db-postgres"),
        ("renvor-seaorm", "db-mysql"),
    ];

    for (package, feature) in ROWS {
        // CONTROL. `--all-targets` over a crate with no test targets compiles the library and
        // reports success, which would make every row below pass for free. Deleting the suites is
        // the way this gate goes quiet, so the gate says so.
        let tests = root.join("crates").join(package).join("tests");
        let populated = std::fs::read_dir(&tests).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "rs")
            })
        });
        if !populated {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`crates/{package}/tests` holds no `.rs` target, so `--all-targets` below \
                     would compile only the library and this gate would pass having checked \
                     nothing"
                ),
            );
            return false;
        }

        if !cargo_succeeds(
            root,
            &[
                "check",
                "--locked",
                "-p",
                package,
                "--no-default-features",
                "--features",
                feature,
                "--all-targets",
            ],
        ) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`cargo check --locked -p {package} --no-default-features --features \
                     {feature} --all-targets` FAILED. The dependency graph for this row resolves \
                     — `cargo tree` is satisfied — but the code does not build, so a project that \
                     selected only `{feature}` could not compile this adapter's own targets"
                ),
            );
            return false;
        }
    }
    true
}

fn lean_facade_compiles(root: &std::path::Path) -> bool {
    // THE GATE. `--all-targets` is the load-bearing flag: without it, examples and tests are never
    // built and the whole failure mode is invisible.
    if !cargo_succeeds(
        root,
        &[
            "check",
            "--locked",
            "-p",
            "renvor",
            "--no-default-features",
            "--all-targets",
        ],
    ) {
        step_fail(
            7,
            "architecture invariants",
            "`cargo check --locked -p renvor --no-default-features --all-targets` FAILED — a target \
             outside the `config` feature depends on it, or an example is missing its \
             `required-features` declaration",
        );
        return false;
    }

    // POSITIVE CONTROL 1: with default features the whole target set, examples included, compiles.
    // Without this, deleting every example would satisfy the gate above perfectly.
    if !cargo_succeeds(
        root,
        &["check", "--locked", "-p", "renvor", "--all-targets"],
    ) {
        step_fail(
            7,
            "architecture invariants",
            "`cargo check --locked -p renvor --all-targets` failed, so the lean check above proves \
             nothing about a build that works",
        );
        return false;
    }

    // POSITIVE CONTROL 2: the `configuration` example is a real target that genuinely needs the
    // feature. This is the one that must FAIL. If the example ever stopped using `renvor::config`,
    // the gate above would still pass while having nothing left to guard, and only this notices.
    if cargo_succeeds(
        root,
        &[
            "check",
            "--locked",
            "-p",
            "renvor",
            "--no-default-features",
            "--example",
            "configuration",
        ],
    ) {
        step_fail(
            7,
            "architecture invariants",
            "the `configuration` example builds WITHOUT the `config` feature, so its \
             `required-features` declaration guards nothing and the lean-build gate is vacuous",
        );
        return false;
    }

    true
}

/// Resolves one crate's normal-edge dependency graph.
fn normal_edges(root: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("cargo")
        .args(["tree", "--edges", "normal", "--prefix", "none"])
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The HTTP stack reaches the transport crate and stops there — asserted in BOTH directions.
///
/// # Why an absence needs a positive control
///
/// "`cargo tree` did not print `axum`" is also what a broken query prints. Every forbidden-name
/// scan below is therefore paired with a query where the same names MUST appear; if the control
/// stops finding them, the check reports that it has stopped working rather than reporting a pass
/// it never earned.
///
/// Three claims, and each is worthless without its control:
///
/// | Claim | Control |
/// |---|---|
/// | `renvor-core` resolves no HTTP crate under **any** feature | `renvor-http` resolves them |
/// | `renvor` without `transport-rest` resolves none | `renvor` **with** it resolves them |
/// | `renvor-http` depends inward only | it does depend on `renvor-core` |
fn transport_isolation_holds(root: &std::path::Path) -> bool {
    const HTTP_CRATES: [&str; 4] = ["axum ", "tower ", "tower-http ", "hyper "];

    // CLAIM 1 — the kernel carries no transport, under ALL features. `--all-features` matters:
    // an isolation claim that holds only for the default feature set is not the claim FR-002 makes.
    let Some(core) = normal_edges(root, &["-p", "renvor-core", "--all-features"]) else {
        step_fail(
            7,
            "architecture invariants",
            "`cargo tree -p renvor-core --all-features` failed",
        );
        return false;
    };
    for forbidden in HTTP_CRATES {
        if core.contains(forbidden) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "renvor-core resolves `{}` — the kernel must carry no transport under any \
                     feature combination (FR-002)",
                    forbidden.trim()
                ),
            );
            return false;
        }
    }

    // CONTROL 1 — the same names, in the crate that is supposed to have them.
    let Some(http) = normal_edges(root, &["-p", "renvor-http"]) else {
        step_fail(
            7,
            "architecture invariants",
            "`cargo tree -p renvor-http` failed",
        );
        return false;
    };
    for expected in HTTP_CRATES {
        if !http.contains(expected) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "renvor-http does not resolve `{}`, so the kernel absence above proves nothing",
                    expected.trim()
                ),
            );
            return false;
        }
    }

    // CLAIM 2 — the facade without the transport feature resolves none of it.
    let Some(lean) = normal_edges(root, &["-p", "renvor", "--no-default-features"]) else {
        step_fail(
            7,
            "architecture invariants",
            "the lean facade tree query failed",
        );
        return false;
    };
    for forbidden in HTTP_CRATES {
        if lean.contains(forbidden) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`renvor` without `transport-rest` resolves `{}` (FR-003)",
                    forbidden.trim()
                ),
            );
            return false;
        }
    }

    // CLAIM 2b — and the DEFAULT feature set does not either. `transport-rest` is off by default,
    // and this is what stops it from being quietly added to `default` later.
    let Some(default_tree) = normal_edges(root, &["-p", "renvor"]) else {
        step_fail(
            7,
            "architecture invariants",
            "the default facade tree query failed",
        );
        return false;
    };
    for forbidden in HTTP_CRATES {
        if default_tree.contains(forbidden) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`renvor` with DEFAULT features resolves `{}`; `transport-rest` must stay \
                     off by default",
                    forbidden.trim()
                ),
            );
            return false;
        }
    }

    // CONTROL 2 — with the feature, the same query MUST find them.
    let Some(with_transport) =
        normal_edges(root, &["-p", "renvor", "--features", "transport-rest"])
    else {
        step_fail(
            7,
            "architecture invariants",
            "the transport-enabled facade tree query failed",
        );
        return false;
    };
    for expected in HTTP_CRATES {
        if !with_transport.contains(expected) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`renvor --features transport-rest` does not resolve `{}`, so the isolation \
                     check proves nothing",
                    expected.trim()
                ),
            );
            return false;
        }
    }

    // CLAIM 3 — the transport depends INWARD. It must not reach back up to the facade, the
    // configuration crate, or the CLI.
    for outward in ["renvor v", "renvor-config ", "renvor-cli "] {
        if http.contains(outward) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "renvor-http depends on `{}` — the transport depends inward only (FR-001)",
                    outward.trim()
                ),
            );
            return false;
        }
    }

    // CONTROL 3 — it DOES depend on the kernel, so the absences above describe a real graph.
    if !http.contains("renvor-core ") {
        step_fail(
            7,
            "architecture invariants",
            "renvor-http does not resolve renvor-core, so the inward-dependency check is not \
             reading the graph",
        );
        return false;
    }

    true
}

/// T101: the crate DAG has the direction the plan claims, with positive controls.
fn crate_dag_holds(root: &std::path::Path) -> bool {
    // `renvor-core` carries no parser, no derive macro, and no secret type. That absence is the
    // whole reason `renvor-config` exists as a separate crate.
    let Some(core) = normal_edges(root, &["-p", "renvor-core"]) else {
        step_fail(
            7,
            "architecture invariants",
            "`cargo tree -p renvor-core` failed",
        );
        return false;
    };
    for forbidden in ["serde ", "toml ", "secrecy "] {
        if core.contains(forbidden) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "renvor-core resolves `{}`, which the crate split exists to prevent",
                    forbidden.trim()
                ),
            );
            return false;
        }
    }

    // POSITIVE CONTROL 1: the query works and finds what IS there.
    if !core.contains("petgraph") || !core.contains("tokio") {
        step_fail(
            7,
            "architecture invariants",
            "the renvor-core tree query found neither petgraph nor tokio, so it is not reading the graph",
        );
        return false;
    }

    // Nothing depends on `renvor-testkit`, which is what keeps `test-util` out of every production
    // graph.
    for package in ["renvor", "renvor-core", "renvor-config"] {
        let Some(tree) = normal_edges(root, &["-p", package]) else {
            step_fail(
                7,
                "architecture invariants",
                &format!("`cargo tree -p {package}` failed"),
            );
            return false;
        };
        if tree.contains("renvor-testkit") {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "{package} depends on renvor-testkit, which would put test-util in a production graph"
                ),
            );
            return false;
        }
    }

    // POSITIVE CONTROL 2: the facade DOES depend on renvor-core, so the absences above are facts
    // about the graph rather than about an empty query.
    let Some(facade) = normal_edges(root, &["-p", "renvor"]) else {
        step_fail(
            7,
            "architecture invariants",
            "`cargo tree -p renvor` failed",
        );
        return false;
    };
    if !facade.contains("renvor-core") {
        step_fail(
            7,
            "architecture invariants",
            "the facade tree does not show renvor-core, so the DAG queries are not working",
        );
        return false;
    }
    true
}

/// T102: `renvor` with `--no-default-features` resolves none of the configuration crates, and
/// **with** default features resolves them — both directions, because either alone is half a claim.
fn facade_feature_isolation_holds(root: &std::path::Path) -> bool {
    let lean = normal_edges(root, &["-p", "renvor", "--no-default-features"]);
    let Some(lean) = lean else {
        step_fail(
            7,
            "architecture invariants",
            "the core-only facade tree query failed",
        );
        return false;
    };
    for forbidden in ["serde ", "toml ", "secrecy ", "confique "] {
        if lean.contains(forbidden) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`renvor` without default features resolves `{}`",
                    forbidden.trim()
                ),
            );
            return false;
        }
    }

    // POSITIVE CONTROL: the same query WITH default features must resolve them. Without this, a
    // broken query returning nothing would read as perfect isolation — and the plan's stated
    // limit, that default-feature consumers still get the configuration dependencies, would go
    // unproven rather than proven true.
    let Some(full) = normal_edges(root, &["-p", "renvor"]) else {
        step_fail(
            7,
            "architecture invariants",
            "the default facade tree query failed",
        );
        return false;
    };
    for expected in ["serde ", "toml ", "secrecy "] {
        if !full.contains(expected) {
            step_fail(
                7,
                "architecture invariants",
                &format!(
                    "`renvor` WITH default features does not resolve `{}`, so the isolation check proves nothing",
                    expected.trim()
                ),
            );
            return false;
        }
    }
    true
}

/// T118: every dependency of a publishable package is resolvable from a registry.
///
/// FR-040 prohibits a **path-only** dependency in a publishable package, and prohibits git
/// dependencies outright. `{ path, version }` is permitted, and is how a multi-crate workspace
/// publishes at all: cargo rewrites it to the version requirement and drops the path.
///
/// Three documents stated this rule and **two stated it wrongly**, as "any path dependency" —
/// which, read literally, made this workspace unpublishable by rule while it was publishable in
/// fact. Nothing noticed, because nothing executed it. This does.
///
/// # Why this reads the manifests as text rather than asking `cargo metadata`
///
/// `cargo metadata` emits JSON, and parsing JSON needs a dependency. `xtask` is **deliberately
/// dependency-free**, and its manifest says why: the verification runner is the thing that checks
/// the dependency policy, so giving it dependencies would put the checker's own supply chain
/// outside the check it performs. That principle outranks the convenience of a parser here.
///
/// The scan is therefore line-oriented, and **fails closed** on any manifest shape it does not
/// recognise — a `[dependencies.name]` sub-table is refused rather than skipped, because skipping
/// what it cannot read is exactly how a text scan reports a clean result it never earned.
fn publishable_dependencies_are_resolvable(root: &std::path::Path) -> bool {
    let Some(manifests) = workspace_manifests(root) else {
        step_fail(
            7,
            "architecture invariants",
            "the workspace manifests could not be read",
        );
        return false;
    };

    match scan_manifests(&manifests) {
        Ok(()) => true,
        Err(reason) => {
            step_fail(7, "architecture invariants", &reason);
            false
        }
    }
}

/// The pure half of [`publishable_dependencies_are_resolvable`].
///
/// Separated so its refusals can be unit-tested against synthetic manifests. A check whose failure
/// path has never run is a check nobody has evidence about.
fn scan_manifests(manifests: &[(String, String)]) -> Result<(), String> {
    let mut publishable = 0_usize;
    let mut examined = 0_usize;
    let mut path_and_version = 0_usize;

    for (name, text) in manifests {
        // `publish = false` is what exempts a package from this rule — as a KEY, not as the words
        // appearing anywhere in the file. See `is_publishable`.
        if !is_publishable(text) {
            continue;
        }
        publishable += 1;

        let mut section = String::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('[') {
                section = trimmed.to_owned();
                // Fail closed on the sub-table form this scan cannot read line-by-line.
                if section.starts_with("[dependencies.")
                    || section.starts_with("[build-dependencies.")
                {
                    return Err(format!(
                        "`{name}` declares `{section}`, a dependency sub-table this check \
                             cannot read line-by-line. Rewrite it inline, or teach this check the \
                             shape — do not leave it unexamined"
                    ));
                }
                continue;
            }

            // Dev-dependencies are stripped from the published manifest, so a path-only one there
            // is harmless. Normal and build dependencies are not.
            let relevant = matches!(section.as_str(), "[dependencies]" | "[build-dependencies]")
                || (section.starts_with("[target.")
                    && (section.ends_with(".dependencies]")
                        || section.ends_with(".build-dependencies]")));
            if !relevant {
                continue;
            }
            examined += 1;

            if trimmed.contains("git = ") {
                return Err(format!(
                    "publishable package `{name}` has a git dependency: `{trimmed}`. crates.io \
                         rejects it, and nothing pins what was built (FR-040)"
                ));
            }

            if trimmed.contains("path = ") {
                if !trimmed.contains("version = ") {
                    return Err(format!(
                        "publishable package `{name}` has a PATH-ONLY dependency: `{trimmed}`. \
                             Add `version` so cargo can rewrite it at publish time (FR-040)"
                    ));
                }
                path_and_version += 1;
            }
        }
    }

    // POSITIVE CONTROL: the scan read real manifests, found real dependency lines, and found at
    // least one in the `{ path, version }` form the corrected rule exists to permit. Without this,
    // a scan that read nothing would report perfect compliance — and the corrected wording would
    // itself go untested.
    if publishable < 2 || examined == 0 || path_and_version == 0 {
        return Err(format!(
            "the manifest scan saw {publishable} publishable package(s), {examined} dependency \
                 line(s), and {path_and_version} in the `{{ path, version }}` form; it is not \
                 reading the workspace"
        ));
    }

    Ok(())
}

/// The exact test path of the end-to-end route-relay proof.
///
/// Named once, and asserted to exist by `the_relay_test_named_by_the_gate_exists`.
const RELAY_TEST: &str =
    "commands::routes::tests::the_relay_reads_what_the_real_library_actually_prints";

/// Step 4: the `#[ignore]`d end-to-end route-relay proof actually RAN and passed.
///
/// # Why this captures output instead of trusting the exit status
///
/// `cargo test -- --exact <name>` where `<name>` matches nothing prints `0 passed` and exits **0**.
/// A gate that only checked the exit status would therefore keep passing if the test were renamed
/// or deleted — running nothing, reporting success. That is the same class of failure as the
/// `#[ignore]` this command exists to defeat, one level up, so the count is read back rather than
/// assumed.
fn the_end_to_end_relay_ran(root: &Path) -> bool {
    println!("[4/{TOTAL_STEPS}] tests (end-to-end route relay) ...");

    let output = Command::new("cargo")
        .args([
            "test",
            "-p",
            "renvor-cli",
            "--bins",
            "--",
            "--ignored",
            "--exact",
            RELAY_TEST,
        ])
        .current_dir(root)
        .output();

    let Ok(output) = output else {
        step_fail(
            4,
            "tests (end-to-end route relay)",
            "the end-to-end relay test could not be executed",
        );
        return false;
    };

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    if !output.status.success() {
        step_fail(
            4,
            "tests (end-to-end route relay)",
            &format!("the end-to-end relay test failed:\n{combined}"),
        );
        return false;
    }

    if !combined.contains("1 passed") {
        step_fail(
            4,
            "tests (end-to-end route relay)",
            &format!(
                "the filter `{RELAY_TEST}` matched no test, so the gate ran nothing and would \
                 have reported success. The test was renamed, deleted, or is no longer `#[ignore]`d"
            ),
        );
        return false;
    }

    step_ok(4, "tests (end-to-end route relay)", "passed");
    true
}

/// Every (row, suite) pair the persistence evidence rests on.
///
/// `(crate, test binary, test path)`. Four rows of `PLAN.md` §10.1's backend matrix. Five suites
/// are compiled once in `renvor-testkit` and called by every row — the **ports** contract, the
/// **domain** example, **concurrency and idempotency**, **portability**, and the **upgrade
/// path**. Two more are per-adapter by necessity, because they drive each adapter's own code: the
/// **startup diagnostic**, which carries two required tests (a refused socket and a refused
/// credential reach different code), and **error classification**.
///
/// # The rows are not symmetric, and the asymmetry is real rather than an omission
///
/// Each direct-SQLx row carries **twelve** required tests and each SeaORM row **eleven**. The
/// difference is the transaction-conflict test: a deadlock is provoked by two sessions taking two
/// row locks in opposite orders, which `renvor-sqlx` does through `sqlx::Pool` directly.
/// `renvor-seaorm`'s suite classifies a `DbErr` through the idiomatic `Statement` path and has no
/// equivalent, so inventing two entries for tests that do not exist would fail the census on every
/// run. Its three tests are enumerated; the fourth is recorded here as absent rather than assumed.
///
/// # Why error classification is here at all
///
/// **It was not, until a review found the gap.** `PLAN.md` §819 makes *"database error
/// normalization"* a Phase 008 deliverable, and these suites are its only real-database coverage —
/// they are where not-null, check-violation and transaction-conflict classification are measured
/// against servers rather than against a fabricated driver error. With no entry here, deleting or
/// feature-gating one of them left `cargo test --workspace` reporting fewer tests and succeeding,
/// and the census green: exactly the disappearance the census exists to prevent, in the suites
/// that back the deliverable it was built for.
///
/// The four-per-adapter enumeration is deliberate rather than a sample. A census that named only
/// `each_constraint_violation_is_classified_as_itself` would leave its own control — the one
/// asserting the four kinds do not collapse onto one — deletable without complaint.
///
/// **This was twenty-four before Phase 008's correction cycle**, then twenty-eight when the
/// server-side refusal test was added on the finding that `ConnectFailed`'s advice covered only
/// one of the five causes it is returned for. The counts are stated here rather than left implicit
/// so that a reader comparing this against phase evidence written earlier sees the change rather
/// than a discrepancy.
///
/// **Forty-six before batch F**, and the prose above said "forty-two" while the array said
/// forty-six — a stale count left by an earlier batch. Corrected here rather than left to be
/// noticed again: a missing entry fails the gate whichever suite it belongs to.
///
/// # The refresh-rotation pair, added in batch G2
///
/// **Fifty-four became fifty-eight.** `renvor_testkit::refresh` is the shared contract behind the
/// refresh-token transition, and it is here for a reason narrower than symmetry: the defect it was
/// written for — a successor inserted into a family a concurrent replay had already revoked —
/// **cannot be reproduced without a real server**. The unit test that was supposed to catch it
/// raced two rotations against an in-memory store whose `async fn`s contain no `.await`, so
/// `tokio::join!` ran them one after the other and nothing interleaved.
///
/// One row per adapter-engine pair, matching every other shared suite here. The runner itself
/// asserts it called all twelve of its assertions, because a census entry is one line per row and
/// cannot see inside the function it names.
///
/// This pair is also why the census invocation carries `tokens`: see `the_four_rows_all_ran`.
/// The database-backed suites that must report in, with the features each needs.
///
/// # The feature string moved from the invocation to the row, and the reason is `renvor-auth-http`
///
/// Every entry used to be run with one hard-coded `db-postgres,db-mysql,tokens`, with a comment
/// saying the other suites were unaffected by it. That stopped being true the moment a required
/// suite lived in a crate with **no database features at all**: `renvor-auth-http` reaches
/// PostgreSQL through a dev-dependency, so passing it `--features db-postgres` is not a harmless
/// extra — it is an error, and the census would have failed to run rather than failed to find.
const ROW_EVIDENCE: [(&str, &str, &str, &str); 67] = [
    // THE TEST APPLICATION (FR-083). Not a four-row entry — it exercises the ROUTES, and the thing
    // that varies by engine is the adapter, which the four-row suites already measure. It reaches
    // PostgreSQL through a dev-dependency, so it takes NO database feature of its own; passing one
    // is what the per-row feature string above exists to avoid.
    (
        "renvor-auth-http",
        "test_application",
        "every_flow_answers_and_nothing_secret_comes_back",
        "tokens",
    ),
    (
        "renvor-sqlx",
        "abuse_controls",
        "postgres::the_shared_abuse_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "abuse_controls",
        "mysql::the_shared_abuse_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "abuse_controls",
        "postgres::the_shared_abuse_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "abuse_controls",
        "mysql::the_shared_abuse_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "refresh_rotation",
        "postgres::the_shared_refresh_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "refresh_rotation",
        "mysql::the_shared_refresh_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "refresh_rotation",
        "postgres::the_shared_refresh_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "refresh_rotation",
        "mysql::the_shared_refresh_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "shared_contract",
        "postgres::the_shared_persistence_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "shared_contract",
        "mysql::the_shared_persistence_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "contract",
        "postgres::the_shared_persistence_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "contract",
        "mysql::the_shared_persistence_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "domain",
        "postgres::the_shared_domain_example_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "domain",
        "mysql::the_shared_domain_example_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "domain",
        "postgres::the_shared_domain_example_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "domain",
        "mysql::the_shared_domain_example_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "domain",
        "postgres::the_shared_concurrency_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "domain",
        "mysql::the_shared_concurrency_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "domain",
        "postgres::the_shared_concurrency_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "domain",
        "mysql::the_shared_concurrency_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "portability",
        "postgres::the_portability_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "portability",
        "mysql::the_portability_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "portability",
        "postgres::the_portability_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "portability",
        "mysql::the_portability_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "domain",
        "postgres::the_upgrade_path_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "domain",
        "mysql::the_upgrade_path_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "domain",
        "postgres::the_upgrade_path_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "domain",
        "mysql::the_upgrade_path_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "startup_diagnostic",
        "postgres::a_failed_start_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "startup_diagnostic",
        "mysql::a_failed_start_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "startup_diagnostic",
        "postgres::a_failed_start_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "startup_diagnostic",
        "mysql::a_failed_start_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // The server-side refusal rows, added by Phase 008's correction cycle. A refused socket and a
    // refused credential reach different code — the I/O arm and `classify_connect_error` — so a
    // census that required only the first would let the second be deleted silently.
    (
        "renvor-sqlx",
        "startup_diagnostic",
        "postgres::a_server_side_refusal_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "startup_diagnostic",
        "mysql::a_server_side_refusal_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "startup_diagnostic",
        "postgres::a_server_side_refusal_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "startup_diagnostic",
        "mysql::a_server_side_refusal_names_the_provider_and_what_to_do",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // The error-classification rows, added by Phase 008's second correction cycle on the finding
    // that `PLAN.md` §819's "database error normalization" deliverable had its real-database
    // coverage outside the census entirely.
    (
        "renvor-sqlx",
        "error_classification",
        "postgres::each_constraint_violation_is_classified_as_itself",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "error_classification",
        "mysql::each_constraint_violation_is_classified_as_itself",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "error_classification",
        "postgres::each_constraint_violation_is_classified_as_itself",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "error_classification",
        "mysql::each_constraint_violation_is_classified_as_itself",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // The control for the four above. Without its own entry it could be deleted while the census
    // stayed green, and a mapping that collapsed the four kinds onto one would then be caught by
    // three assertions instead of by the property stated directly.
    (
        "renvor-sqlx",
        "error_classification",
        "postgres::the_four_violations_do_not_collapse_onto_one_kind",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "error_classification",
        "mysql::the_four_violations_do_not_collapse_onto_one_kind",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "error_classification",
        "postgres::the_four_violations_do_not_collapse_onto_one_kind",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "error_classification",
        "mysql::the_four_violations_do_not_collapse_onto_one_kind",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // Transaction conflict: DIRECT-SQLX ROWS ONLY, and that is measured rather than overlooked.
    // Provoking a deadlock takes two sessions holding two row locks in opposite orders, which the
    // SQLx suite arranges over `sqlx::Pool`; `renvor-seaorm`'s suite has no such test, so there
    // are two entries here and not four. Adding the missing pair means writing the tests first.
    (
        "renvor-sqlx",
        "error_classification",
        "postgres::a_lost_conflict_is_retryable_rather_than_a_rejection",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "error_classification",
        "mysql::a_lost_conflict_is_retryable_rather_than_a_rejection",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // The redaction rows. Differently named per adapter because they defend against different
    // text: the driver's message on one side, SeaORM's — which also carries the generated SQL —
    // on the other. Both are the executable half of CONSTITUTION.md:107 for this path.
    (
        "renvor-sqlx",
        "error_classification",
        "postgres::a_violation_never_carries_the_server_text",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "error_classification",
        "mysql::a_violation_never_carries_the_server_text",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "error_classification",
        "postgres::a_violation_never_carries_the_seaorm_text",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "error_classification",
        "mysql::a_violation_never_carries_the_seaorm_text",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // ---- PHASE 009: the authentication rows ----
    //
    // Four entries, one per row, all naming the SAME test — because the assertion is that the four
    // rows agree, and a census that named a different test per adapter could not tell agreement
    // from coincidence. `renvor-auth` itself resolves no driver, so these suites are the only place
    // its ports meet a real server.
    (
        "renvor-sqlx",
        "auth_repositories",
        "postgres::a_single_use_token_is_consumed_exactly_once_under_concurrency",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "auth_repositories",
        "mysql::a_single_use_token_is_consumed_exactly_once_under_concurrency",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "auth_repositories",
        "postgres::a_single_use_token_is_consumed_exactly_once_under_concurrency",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "auth_repositories",
        "mysql::a_single_use_token_is_consumed_exactly_once_under_concurrency",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // ---- PHASE 009 BATCH F: the session rows ----
    //
    // Two tests, four rows each. The first is the revocation race — exactly one of four concurrent
    // logouts may revoke a live session, and that is a property of the database rather than of the
    // code that calls it.
    //
    // The second exists because of a difference between the engines that no unit test can reach:
    // `touch` decides liveness from `rows_affected`, and MySQL reports rows *changed* unless the
    // client negotiated `CLIENT_FOUND_ROWS`. Two requests in one microsecond write `last_seen_at`
    // the value it already holds — zero changed, one matched — and the user is signed out. Reading
    // `sqlx-mysql`'s source says the flag is set; only this says it stayed set.
    (
        "renvor-sqlx",
        "auth_repositories",
        "postgres::two_concurrent_logouts_revoke_exactly_once",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "auth_repositories",
        "mysql::two_concurrent_logouts_revoke_exactly_once",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "auth_repositories",
        "postgres::two_concurrent_logouts_revoke_exactly_once",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "auth_repositories",
        "mysql::two_concurrent_logouts_revoke_exactly_once",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "auth_repositories",
        "postgres::touching_twice_at_one_instant_keeps_the_session_live",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "auth_repositories",
        "mysql::touching_twice_at_one_instant_keeps_the_session_live",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "auth_repositories",
        "postgres::touching_twice_at_one_instant_keeps_the_session_live",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "auth_repositories",
        "mysql::touching_twice_at_one_instant_keeps_the_session_live",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    // THE FOUR JOB-STORE ROWS (Phase 010, FR-040): one shared contract, four rows. `jobs` joins
    // the feature string of every row above rather than forming a second group, so the package
    // compiles once for the census and the auth rows lose nothing by carrying it.
    (
        "renvor-sqlx",
        "jobs",
        "postgres::the_shared_jobs_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-sqlx",
        "jobs",
        "mysql::the_shared_jobs_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "jobs",
        "postgres::the_shared_jobs_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
    (
        "renvor-seaorm",
        "jobs",
        "mysql::the_shared_jobs_contract_holds",
        "db-postgres,db-mysql,tokens,jobs",
    ),
];

/// STEP 4, still: every one of the four rows reported in.
///
/// # `cargo test --workspace` cannot notice a missing row
///
/// Step 4 runs the whole workspace, so all four rows execute today. What it does not do — what
/// nothing did before this — is state how many rows were *supposed* to run. Delete
/// `crates/renvor-seaorm/tests/contract.rs` and `cargo test --workspace` runs fewer tests and
/// reports success. A row can be removed, renamed, or feature-gated out of existence and the gate
/// stays green, because a smaller test count is not a failure.
///
/// This is the same shape as two failures this repository has already been bitten by and fixed:
/// the relay test above, which *"ran in NO automated gate until 2026-08-23"*, and the
/// real-database suites that *"reported `ok` in CI having connected to nothing"*
/// (`crates/renvor-sqlx/tests/support/mod.rs`). Both were closed by making the gate assert that
/// the thing ran. This closes the third.
///
/// # Why `ok` here means the row genuinely reached a database
///
/// A test that skipped would also print `ok`. It cannot skip under this gate: `support::url`
/// **panics** when `RENVOR_TEST_REQUIRE_DATABASE` is set and a URL is absent.
///
/// And that variable is no longer optional. Step 1 refuses the whole sequence with
/// [`EXIT_TOOLING_MISSING`] — exit **2**, before any step runs — unless all three of
/// `RENVOR_TEST_POSTGRES_URL`, `RENVOR_TEST_MYSQL_URL` and `RENVOR_TEST_REQUIRE_DATABASE` are set
/// and non-empty. So reaching this function at all means the census was mandatory.
///
/// **The paragraph that used to stand here said the opposite** — that the census *"is reported as
/// not-run when it is not ... so a contributor without local databases still gets a usable
/// `cargo xtask verify`"*. That was the behaviour this correction removed, and a review found the
/// comment still describing it. A doc comment that survives the behaviour it documents is how the
/// next reader concludes the skip is still available.
///
/// # It re-runs two test binaries
///
/// Deliberate, and it costs a couple of seconds. `run` streams its child's output to the operator
/// rather than capturing it, which is the right behaviour for a step whose output a human is
/// watching; capturing step 4 wholesale to satisfy this check would trade live test progress for a
/// census. Two small binaries run twice is the cheaper trade.
/// Every byte of `text` that is not part of an ANSI escape sequence.
///
/// Cargo colours its status lines when `CARGO_TERM_COLOR=always`, which GitHub Actions sets, and it
/// closes the colour BEFORE the path: the bytes are `…Running\x1b[0m tests/name.rs`. A literal
/// search for `Running tests/name.rs` therefore matches on a developer's machine, where output is
/// piped and cargo disables colour, and fails in CI on the same commit. The census sets
/// `CARGO_TERM_COLOR=never` so this should not arise, and strips escapes anyway rather than trust
/// one environment variable to hold across every runner this project is verified on.
fn without_ansi(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\u{1b}' {
            plain.push(character);
            continue;
        }
        // CSI sequences end at the first byte in `@`..=`~`; anything else is a short escape whose
        // next character is consumed with it.
        if chars.next() == Some('[') {
            for terminator in chars.by_ref() {
                if ('@'..='~').contains(&terminator) {
                    break;
                }
            }
        }
    }
    plain
}

/// Whether a grouped `cargo test` run actually executed a given test binary.
///
/// Cargo announces each binary with `Running tests/<name>.rs` on **stderr**, while the harness
/// writes `test <name> ... ok` to **stdout**. `Command::output` hands back two separate buffers, so
/// the two streams cannot be interleaved after the fact — an earlier revision of this census
/// concatenated them and tried to slice per-binary sections out of the result, which put every
/// header after every evidence line and made each "section" empty. The streams are therefore used
/// for what each one actually carries: this function reads stderr to establish that a binary RAN.
fn binary_ran(stderr: &str, binary: &str) -> bool {
    without_ansi(stderr).contains(&format!("Running tests/{binary}.rs"))
}

/// Rows in one census group whose test names are not unique.
///
/// **This is what keeps grouping honest.** Running several binaries in one invocation merges their
/// stdout, so a row matched against the merged text could be satisfied by an identically named test
/// in a sibling binary — quietly turning a per-binary requirement into a per-package one. Rather
/// than assume that never happens, the census refuses to run a group in which it COULD: if two rows
/// in the same group share a test name, attribution is ambiguous and this reports it instead of
/// guessing. Today every group is collision-free, and this is what keeps that true after a rename.
fn ambiguous_rows_in_group<'a>(package: &str, features: &str) -> Vec<&'a str> {
    let mut names: Vec<&str> = ROW_EVIDENCE
        .iter()
        .filter(|(p, _, _, f)| *p == package && *f == features)
        .map(|(_, _, test, _)| *test)
        .collect();
    names.sort_unstable();
    let mut ambiguous = Vec::new();
    for pair in names.windows(2) {
        if pair[0] == pair[1] && !ambiguous.contains(&pair[0]) {
            ambiguous.push(pair[0]);
        }
    }
    ambiguous
}

fn the_four_rows_all_ran(root: &Path, env: &dyn Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    const TITLE: &str = "tests (four-row persistence census)";

    // Defence in depth. Step 1 already refuses the run without these, so reaching here with one
    // absent means the environment changed mid-sequence. Either way the answer is the same: a
    // census that did not run is not a census that passed.
    let absent = missing_database_prerequisites(env);
    if !absent.is_empty() {
        step_fail(
            4,
            TITLE,
            &format!(
                "the database environment is incomplete — {} of {} variables are absent or \
                 empty, so none of the {} row-suite pairs could run. This is a FAILURE rather \
                 than a skip: `contracts/verification-sequence.md` states that no step in this \
                 sequence is conditional. Run `cargo xtask verify` again with the environment \
                 CONTRIBUTING.md describes",
                absent.len(),
                DATABASE_REQUIRED.len(),
                ROW_EVIDENCE.len()
            ),
        );
        return false;
    }

    // Group the rows by PACKAGE AND FEATURE SET, not by binary.
    //
    // Every binary in a group is handed to ONE `cargo test` invocation via repeated `--test`. The
    // rows, the required output lines, and the failure conditions are unchanged; what changes is
    // how many times Cargo is asked to resolve features and link a test harness for the same
    // package. Seventeen invocations became three, and because the feature set is what varies
    // between them, the seventeen were re-resolving and re-linking work the three now do once.
    //
    // Grouping is only safe because attribution survives it. Cargo prints a `Running tests/<name>.rs`
    // header before each binary's block, so the combined output is SPLIT on those headers and every
    // row is still checked against the output of its OWN binary. A row can therefore never be
    // satisfied by an identically named test in a sibling binary — which is the property the
    // per-binary invocation gave for free, and the one thing a naive merge would silently lose.
    let mut groups: Vec<(&str, &str)> = ROW_EVIDENCE
        .iter()
        .map(|(package, _, _, features)| (*package, *features))
        .collect();
    // Sorted before `dedup`, which only removes ADJACENT duplicates.
    groups.sort_unstable();
    groups.dedup();

    for (package, features) in groups {
        let mut binaries: Vec<&str> = ROW_EVIDENCE
            .iter()
            .filter(|(p, _, _, f)| *p == package && *f == features)
            .map(|(_, binary, _, _)| *binary)
            .collect();
        binaries.sort_unstable();
        binaries.dedup();

        let mut args: Vec<&str> = vec!["test", "-p", package, "--features", features];
        for binary in &binaries {
            // PER ROW, not one string for every binary. `tokens` is needed by the refresh-rotation
            // pair — a census row for a test compiled out is a row that can never report in — and
            // the test application takes no database feature at all, because it has none to take.
            args.push("--test");
            args.push(binary);
        }

        let started = Instant::now();
        let output = Command::new("cargo")
            .args(&args)
            // Deterministic output regardless of runner. See `without_ansi`.
            .env("CARGO_TERM_COLOR", "never")
            .current_dir(root)
            .output();
        let elapsed = started.elapsed();
        timing(
            &format!("step 4 census {package} ({} binaries)", binaries.len()),
            elapsed,
        );

        let Ok(output) = output else {
            step_fail(
                4,
                TITLE,
                &format!("`{package}` census group could not be executed"),
            );
            return false;
        };
        // Kept SEPARATE. stderr says which binaries ran; stdout carries the evidence lines.
        let stdout = without_ansi(&String::from_utf8_lossy(&output.stdout));
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            step_fail(
                4,
                TITLE,
                &format!("`{package}` census group failed:\n{stdout}{stderr}"),
            );
            return false;
        }

        // Refuse an ambiguous group rather than credit a row to the wrong binary.
        let ambiguous = ambiguous_rows_in_group(package, features);
        if !ambiguous.is_empty() {
            step_fail(
                4,
                TITLE,
                &format!(
                    "the `{package}` census group contains rows sharing a test name ({}), so a \
                     grouped run cannot attribute evidence to the binary that owns it. Give the \
                     tests distinct names, or split the group",
                    ambiguous.join(", ")
                ),
            );
            return false;
        }

        for (expected_package, expected_binary, test, _) in ROW_EVIDENCE {
            if expected_package != package {
                continue;
            }
            // The binary has to have RUN. A binary deleted, renamed, or filtered out prints no
            // `Running` line, and its rows must fail rather than be looked for elsewhere.
            if !binary_ran(&stderr, expected_binary) {
                step_fail(
                    4,
                    TITLE,
                    &format!(
                        "row `{package}::{test}` did not report in: the runner printed no \
                         `Running tests/{expected_binary}.rs` line, so the binary carrying this \
                         row never ran"
                    ),
                );
                return false;
            }
            // The runner's own line for a test that RAN and passed. A row that was deleted,
            // renamed, or compiled out produces no such line.
            let evidence = format!("test {test} ... ok");
            if !stdout.contains(&evidence) {
                step_fail(
                    4,
                    TITLE,
                    &format!(
                        "row `{package}::{test}` did not report in. Expected the line \
                         `{evidence}` from `{expected_binary}` in the runner's output. The row was \
                         removed, renamed, or feature-gated out — and without this census the \
                         workspace test run would simply have executed one row fewer and reported \
                         success"
                    ),
                );
                return false;
            }
        }
    }

    step_ok(
        4,
        TITLE,
        &format!(
            "all {} required suites reported in (12 tests on each direct-SQLx row, 11 on each \
             SeaORM row, the refresh-rotation, abuse-control, and job-store \
         contracts on every row, and the \
             end-to-end test application)",
            ROW_EVIDENCE.len()
        ),
    );
    true
}

/// Whether a manifest's package is intended for publication.
///
/// **Comment-aware, and that is the whole point.** Both this scan and the FR-040 dependency scan
/// previously tested `text.contains("publish = false")` against the raw file. `renvor-core`,
/// `renvor-http` and `renvor-testkit` each *discuss* `publish = false` in a leading comment
/// explaining why they are **not** marked that way — so all three were read as unpublishable and
/// silently skipped by both gates. Three of the five publishable packages were unexamined, and the
/// gates reported success. Discovered 2026-08-23 while adding the metadata check, because
/// `cargo metadata` says five and the scan said two.
fn is_publishable(manifest: &str) -> bool {
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        // Strip a trailing comment before looking, so `publish = false # ...` still counts.
        let code = trimmed.split('#').next().unwrap_or(trimmed).trim();
        if code.starts_with("publish") {
            let rest = code.trim_start_matches("publish").trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                let value = value.trim();
                if value == "false" || value.starts_with("[]") {
                    return false;
                }
            }
        }
    }
    true
}

/// The metadata every publishable package must declare, per `contracts/package-metadata.md`.
///
/// The contract said *"A missing field fails metadata validation (FR-040)"* from the day it was
/// written, and until 2026-08-23 **nothing validated them**. Cargo emits a warning and packages
/// happily: deleting `keywords`, `categories`, `documentation` and `homepage` from a crate and
/// running `cargo package --workspace` exited `0` with no error. This is the check that makes the
/// sentence true rather than the sentence that was corrected to match the gap.
const REQUIRED_PACKAGE_FIELDS: [&str; 11] = [
    "name",
    "version",
    "description",
    "license",
    "repository",
    "homepage",
    "documentation",
    "readme",
    "keywords",
    "categories",
    "rust-version",
];

/// How a field arrived: written here, or inherited from `[workspace.package]`.
#[derive(Debug, PartialEq, Eq)]
enum Declared {
    Directly,
    FromWorkspace,
}

/// The `[package]` table of a manifest, as text. Empty when there is no such table.
fn package_table(manifest: &str) -> String {
    let mut inside = false;
    let mut collected = String::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[package]";
            continue;
        }
        if inside {
            collected.push_str(line);
            collected.push('\n');
        }
    }
    collected
}

/// The `[workspace.package]` table of the workspace root, as text.
fn workspace_package_table(root_manifest: &str) -> String {
    let mut inside = false;
    let mut collected = String::new();
    for line in root_manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == "[workspace.package]";
            continue;
        }
        if inside {
            collected.push_str(line);
            collected.push('\n');
        }
    }
    collected
}

/// Whether `table` declares `field`, and how.
///
/// `field.workspace = true` counts as declared **here** and inherited **there** — the caller must
/// then confirm the workspace actually provides it, or an inherited field would satisfy this check
/// while resolving to nothing.
fn declaration_of(table: &str, field: &str) -> Option<Declared> {
    for line in table.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix(field) {
            let rest = rest.trim_start();
            if rest.starts_with('=') {
                return Some(Declared::Directly);
            }
            if rest.starts_with(".workspace") {
                return Some(Declared::FromWorkspace);
            }
        }
    }
    None
}

/// Every publishable package declares every required field, resolving workspace inheritance.
///
/// Returns the number of publishable packages examined, so a scan that silently examined none
/// cannot report success.
fn required_metadata_is_declared(
    manifests: &[(String, String)],
    root_manifest: &str,
) -> Result<usize, String> {
    let workspace = workspace_package_table(root_manifest);
    let mut publishable = 0_usize;

    for (name, text) in manifests {
        if !is_publishable(text) {
            continue;
        }
        publishable += 1;

        let table = package_table(text);
        if table.trim().is_empty() {
            return Err(format!(
                "publishable package `{name}` has no readable `[package]` table, so its metadata \
                 cannot be checked at all"
            ));
        }

        for field in REQUIRED_PACKAGE_FIELDS {
            match declaration_of(&table, field) {
                None => {
                    return Err(format!(
                        "publishable package `{name}` declares no `{field}`. \
                         `contracts/package-metadata.md` requires it of every package intended for \
                         publication (FR-040). Cargo only warns, which is why this check exists"
                    ));
                }
                Some(Declared::FromWorkspace) => {
                    if declaration_of(&workspace, field).is_none() {
                        return Err(format!(
                            "publishable package `{name}` inherits `{field}` from the workspace, \
                             but `[workspace.package]` does not declare it. The field resolves to \
                             nothing and the package would publish without it"
                        ));
                    }
                }
                Some(Declared::Directly) => {}
            }
        }

        // The shipped file set is stated, never inferred.
        let has_include = declaration_of(&table, "include").is_some();
        let has_exclude = declaration_of(&table, "exclude").is_some();
        if !has_include && !has_exclude {
            return Err(format!(
                "publishable package `{name}` declares neither `include` nor `exclude`. The \
                 contract requires the shipped file set to be explicit rather than inferred"
            ));
        }
    }

    if publishable == 0 {
        return Err(
            "the metadata scan found no publishable package. An empty scan proves nothing"
                .to_owned(),
        );
    }

    Ok(publishable)
}

/// Step 7: required package metadata is present on every publishable package.
fn required_package_metadata_is_present(root: &std::path::Path) -> bool {
    let Some(manifests) = workspace_manifests(root) else {
        step_fail(
            7,
            "architecture invariants",
            "the workspace manifests could not be read",
        );
        return false;
    };

    let Ok(root_manifest) = std::fs::read_to_string(root.join("Cargo.toml")) else {
        step_fail(
            7,
            "architecture invariants",
            "the workspace root manifest could not be read, so inherited metadata cannot be \
             resolved",
        );
        return false;
    };

    match required_metadata_is_declared(&manifests, &root_manifest) {
        Ok(_) => true,
        Err(reason) => {
            step_fail(7, "architecture invariants", &reason);
            false
        }
    }
}

/// Reads every workspace member manifest as `(package name, text)`.
///
/// Discovered from the directory layout rather than from a list, so a new crate is examined
/// without anybody remembering to add it here.
fn workspace_manifests(root: &std::path::Path) -> Option<Vec<(String, String)>> {
    let mut manifests = Vec::new();
    for directory in [root.join("crates"), root.to_path_buf()] {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let manifest = entry.path().join("Cargo.toml");
            if !manifest.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&manifest).ok()?;
            // The virtual workspace root declares no package and has nothing to check.
            if !text.contains("[package]") {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            manifests.push((name, text));
        }
    }
    manifests.sort();
    (!manifests.is_empty()).then_some(manifests)
}

/// T104: the instability-closure sentence is byte-identical in all three normative locations, and
/// **0** phase numbers appear inside FR-036's normative closure clause.
fn instability_wording_agrees(root: &std::path::Path) -> bool {
    let spec = root.join("contracts/api-stability.md");
    let Ok(text) = std::fs::read_to_string(&spec) else {
        step_fail(
            7,
            "architecture invariants",
            "contracts/api-stability.md is unreadable",
        );
        return false;
    };

    // Located by its own text rather than by line number, so editing the document around it does
    // not silently disable this check.
    let occurrences = text.matches(SC022_SENTENCE).count();
    if occurrences != SC022_REQUIRED_OCCURRENCES {
        step_fail(
            7,
            "architecture invariants",
            &format!(
                "the instability-closure sentence appears {occurrences} time(s) in \
                 contracts/api-stability.md; SC-022 requires exactly \
                 {SC022_REQUIRED_OCCURRENCES}"
            ),
        );
        return false;
    }

    // **0 phase numbers inside the normative clause.** The requirement's surrounding prose does
    // mention Phase 004 — deliberately, as roadmap rationale — so this checks the conditions
    // themselves, between where they are introduced and where the rationale begins. Checking the
    // whole requirement would fail on text that is explicitly not part of the condition.
    let Some(clause_start) = text.find("requires **both** of the following") else {
        step_fail(
            7,
            "architecture invariants",
            "FR-036's closure clause could not be located; the wording check is not reading it",
        );
        return false;
    };
    let after = &text[clause_start..];
    let clause_end = after
        .find("*(Roadmap rationale")
        .unwrap_or_else(|| after.len().min(2000));
    let clause = &after[..clause_end];

    if clause.contains("Phase ") || clause.contains("phase 0") {
        step_fail(
            7,
            "architecture invariants",
            "FR-036's normative closure clause names a phase number; the gate is event-named",
        );
        return false;
    }

    // POSITIVE CONTROL: the clause really was extracted and really does state both conditions, so
    // the absence above means "no phase number" rather than "nothing was scanned".
    if !clause.contains("supersedes ADR-0002") || !clause.contains("first real transport adapter") {
        step_fail(
            7,
            "architecture invariants",
            "the extracted closure clause is missing a condition, so the scan is looking at the wrong text",
        );
        return false;
    }
    true
}

/// The instability-closure sentence SC-022 requires to be byte-identical everywhere.
const SC022_SENTENCE: &str =
    "the first real transport adapter has exercised the surface and its feedback has been applied";

/// How many copies of the sentence must exist. **Exactly** this many, not at least.
///
/// # Why this went from 3 to 1 on 2026-08-19, and why that is not a loosened gate
///
/// It was **3** because the statement lived in three places inside one phase specification — the
/// clarification record, FR-036, and the Dependencies section — and the failure this guarded
/// against was somebody editing one copy and not the others. Counting them was the only way to
/// detect that drift.
///
/// The statement now has **one** authoritative home, `contracts/api-stability.md`, and the phase
/// specification that held the three copies is no longer part of the public tree. Two copies
/// cannot disagree when there is one copy, so the drift class is eliminated rather than merely
/// unchecked. The substantive checks below — that the closure clause names **no phase number**,
/// and the two positive controls proving the clause was really extracted — are unchanged and still
/// run.
///
/// # Why the comparison is `!=` and not `<`
///
/// A lower bound would accept a second copy appearing later. That is exactly the drift the single
/// authoritative home was adopted to eliminate: with two copies, one can be edited and the other
/// left, and a `>=` check passes throughout. `contracts/api-stability.md` states it is the single
/// authoritative copy, so the check enforces precisely that rather than a weaker property the
/// contract does not claim.
const SC022_REQUIRED_OCCURRENCES: usize = 1;

#[cfg(test)]
mod tests {
    /// A missing database prerequisite can only produce the missing-prerequisite code.
    ///
    /// Stated as three separate claims rather than one, because the failure being guarded against
    /// is a gate returning **success**, and `assert_eq!(.., Some(2))` alone would still pass if
    /// someone later made `EXIT_TOOLING_MISSING` equal `EXIT_OK`.
    #[test]
    fn a_missing_database_prerequisite_can_never_yield_exit_zero() {
        let nothing = |_: &str| None;
        let databases = super::missing_database_prerequisites(&nothing);
        assert_eq!(
            databases.len(),
            super::DATABASE_REQUIRED.len(),
            "the probe did not see the absent variables"
        );

        let verdict = super::prerequisites_gate(&[], &databases);
        assert_eq!(verdict, Some(super::EXIT_TOOLING_MISSING));
        assert_ne!(verdict, Some(super::EXIT_OK));
        assert_ne!(
            verdict, None,
            "`None` means the sequence proceeds and can pass"
        );
    }

    /// CONTROL: with the environment complete, the gate lets the run proceed.
    ///
    /// Without this the test above would pass against a gate that refused everything
    /// unconditionally, which is a different defect wearing the same green tick.
    #[test]
    fn a_complete_environment_lets_verification_proceed() {
        let everything = |_: &str| Some(std::ffi::OsString::from("set"));
        let databases = super::missing_database_prerequisites(&everything);
        assert!(
            databases.is_empty(),
            "the probe reported a variable that was set"
        );
        assert_eq!(super::prerequisites_gate(&[], &databases), None);
    }

    /// An empty value is absence. CI writes these variables from shell expansion, and an
    /// expansion that produced nothing would otherwise satisfy a presence check while naming no
    /// database at all.
    #[test]
    fn an_empty_variable_counts_as_missing() {
        let empty = |_: &str| Some(std::ffi::OsString::new());
        assert_eq!(
            super::missing_database_prerequisites(&empty).len(),
            super::DATABASE_REQUIRED.len()
        );
    }

    /// The census must see a binary run whether or not cargo coloured the line.
    ///
    /// This is a REGRESSION CONTROL for a failure that passed locally and failed in CI on the same
    /// commit. Cargo closes the colour before the path — `Running\x1b[0m tests/x.rs` — so a literal
    /// search for `Running tests/x.rs` matches only when colour is off, which is what happens when a
    /// developer pipes the output and is NOT what happens under `CARGO_TERM_COLOR=always`, which
    /// GitHub Actions sets. Both spellings are asserted so neither environment can regress alone.
    #[test]
    fn a_coloured_running_line_is_still_recognised() {
        use crate::binary_ran;

        let plain = "     Running tests/test_application.rs (target/debug/deps/test_application-1)";
        let coloured = "\u{1b}[1m\u{1b}[92m     Running\u{1b}[0m tests/test_application.rs (target/debug/deps/x-1)";

        assert!(
            binary_ran(plain, "test_application"),
            "the uncoloured line must be recognised"
        );
        assert!(
            binary_ran(coloured, "test_application"),
            "the coloured line must be recognised — this is the exact shape CI emits"
        );
        assert!(
            !binary_ran(coloured, "some_other_binary"),
            "a binary that did not run must not be credited by another binary's line"
        );
    }

    /// RED (Correction E). `contracts/verification-sequence.md` says of this sequence: *"Executed
    /// in order. None is conditional. None is skipped."* Step 4's census is conditional on an
    /// environment variable, and when it is absent the census prints `ok — NOT RUN` and returns
    /// success — so a full `cargo xtask verify` exits 0 having executed none of the census pairs.
    #[test]
    fn a_run_without_the_database_environment_cannot_report_the_census_as_ok() {
        let nothing = |_: &str| None;
        assert!(
            !super::the_four_rows_all_ran(std::path::Path::new("."), &nothing),
            "the census reported success without running"
        );
    }

    /// Every test in the two error-classification binaries has a census entry, and vice versa.
    ///
    /// # Derived from the suites rather than counted by hand
    ///
    /// The gap a review found here was not an arithmetic error. It was that an entire
    /// deliverable's real-database coverage — `PLAN.md` §819's *"database error normalization"* —
    /// had **no** entry in `ROW_EVIDENCE` at all, so deleting or feature-gating one of those
    /// suites left `cargo test --workspace` reporting fewer tests and succeeding, and the census
    /// green beside it. Nothing compared the table against the suites.
    ///
    /// This reads the suites. It fails in both directions: a test added to either file without
    /// its pair of rows, and a row naming a test that is not there. Both are how the table drifts.
    ///
    /// The sources are `include_str!`d, so they resolve at COMPILE time — a moved or deleted
    /// suite is a build failure rather than a silently skipped test.
    #[test]
    fn every_error_classification_test_is_censused() {
        const SUITES: [(&str, &str); 2] = [
            (
                "renvor-sqlx",
                include_str!("../../crates/renvor-sqlx/tests/error_classification.rs"),
            ),
            (
                "renvor-seaorm",
                include_str!("../../crates/renvor-seaorm/tests/error_classification.rs"),
            ),
        ];
        /// Both suites are macro-generated, once per engine, from one `mod` body.
        const ENGINES: [&str; 2] = ["postgres", "mysql"];

        /// The name of every `#[tokio::test] async fn` in one suite.
        ///
        /// Keyed on the ATTRIBUTE rather than on `async fn` alone: each suite also declares
        /// `fixture` and `refused` helpers, and a parser that took every `async fn` would demand
        /// census rows for functions that are not tests.
        fn tests_in(source: &str) -> Vec<&str> {
            let mut names = Vec::new();
            let mut is_test = false;
            for line in source.lines() {
                let trimmed = line.trim();
                if trimmed == "#[tokio::test]" {
                    is_test = true;
                } else if is_test {
                    if let Some(rest) = trimmed.strip_prefix("async fn ") {
                        names.push(rest.split('(').next().unwrap_or(rest));
                    }
                    is_test = false;
                }
            }
            names
        }

        let censused: Vec<(&str, &str)> = super::ROW_EVIDENCE
            .iter()
            .filter(|(_, binary, _, _)| *binary == "error_classification")
            .map(|(package, _, test, _)| (*package, *test))
            .collect();

        let mut expected = 0_usize;
        for (package, source) in SUITES {
            let names = tests_in(source);
            // POSITIVE CONTROL. A parser that matched nothing would make every assertion below
            // vacuous, and one that matched too much would name the helpers.
            assert!(
                names.len() >= 3,
                "the parser found {} tests in {package}'s error-classification suite, which is \
                 fewer than the suite is known to carry. It is matching the wrong thing",
                names.len()
            );
            for helper in ["fixture", "refused"] {
                assert!(
                    !names.contains(&helper),
                    "the parser counted `{helper}` as a test in {package}; it is a helper, and a \
                     census row for it would never be satisfied"
                );
            }

            for name in names {
                for engine in ENGINES {
                    let path = format!("{engine}::{name}");
                    assert!(
                        censused.contains(&(package, path.as_str())),
                        "`{package}` carries the error-classification test `{path}`, and the \
                         census does not require it. Add it to `ROW_EVIDENCE` — without a row, \
                         deleting or feature-gating it leaves both the workspace test run and \
                         this gate green"
                    );
                    expected += 1;
                }
            }
        }

        // THE OTHER DIRECTION. A row naming a test that does not exist fails the census on every
        // run against a real database, which is a slow and confusing way to learn about a typo.
        assert_eq!(
            censused.len(),
            expected,
            "the census requires {} error-classification pairs and the suites carry {expected}. \
             A row names a test that is not there: {censused:?}",
            censused.len()
        );
    }

    /// The recovery guidance for a partial MySQL migration describes what the runner does.
    ///
    /// # A guard for prose that was wrong until a review caught it
    ///
    /// `contracts/database-portability.md` instructed operators that after a partial MySQL failure
    /// *"the recovery path is 'run the rest', not 'run it again from the start'"*, and the
    /// documentation page said the same in its own words. SQLx refuses every subsequent run with
    /// `MigrateError::Dirty` before sending a statement, so the instruction sent an operator round
    /// a loop with no exit.
    ///
    /// `a_partial_migration_is_refused_on_the_next_run_rather_than_resumed` proves the behaviour
    /// against both engines. This proves the three documents still **describe** it — a test and the
    /// prose it backs can drift apart without either one failing, and that is exactly what
    /// happened.
    ///
    /// **The third document is ADR-0023**, which the contract names as its source of authority and
    /// which carried the same false rule in its own decision text. It was found by a sweep rather
    /// than by the review, which cited the contract and the guide. A contract corrected against a
    /// record still stating the opposite is a contract with a citation that contradicts it.
    ///
    /// Whitespace is collapsed before searching, because both documents wrap: the phrase this
    /// guards against was split across two lines in the contract, which is why a plain `grep`
    /// for it found nothing.
    ///
    /// # The false phrases still appear, and the test is built around that
    ///
    /// This repository corrects rather than erases: the contract QUOTES the instruction it
    /// withdrew, so the reader can see what changed. A guard that simply forbade the words would
    /// therefore fail on the fix, and the obvious way to make it pass would be to delete the
    /// record of the mistake. So what is asserted is **order**: every occurrence of a resumption
    /// phrase must come after the sentence that withdraws it. A document that reinstates the
    /// instruction, or that carries it with no withdrawal at all, fails.
    #[test]
    fn the_partial_migration_guidance_matches_what_the_runner_does() {
        /// `(name, source, the sentence that withdraws the instruction)`.
        ///
        const DOCUMENTS: [(&str, &str, Option<&str>); 2] = [
            (
                "contracts/database-portability.md",
                include_str!("../../contracts/database-portability.md"),
                Some("used to stand here was false"),
            ),
            (
                "decisions/0023-database-portability-across-the-four-rows.md",
                include_str!("../../decisions/0023-database-portability-across-the-four-rows.md"),
                Some("Amended 2026-08-27"),
            ),
        ];

        for (name, markdown, withdrawal) in DOCUMENTS {
            let flat = markdown.split_whitespace().collect::<Vec<_>>().join(" ");

            // POSITIVE CONTROL, first: a document that stopped covering the subject would satisfy
            // every "does not say" assertion below by saying nothing at all.
            assert!(
                flat.contains("MigrationDirty") || flat.contains("MigrationDirty`"),
                "{name} no longer names the kind a run after a partial failure returns. The \
                 guidance was removed rather than corrected"
            );
            assert!(
                flat.contains("_sqlx_migrations"),
                "{name} no longer names the ledger an operator has to reconcile, so its recovery \
                 procedure cannot be followed"
            );

            let withdrawal = withdrawal.and_then(|marker| flat.find(marker));

            for resumption in [
                "the recovery path is",
                "run the rest",
                "safe to re-run after a partial failure",
                "leaves a database the next run can continue from",
            ] {
                let Some(at) = flat.find(resumption) else {
                    continue;
                };
                let Some(withdrawn_at) = withdrawal else {
                    panic!(
                        "{name} tells an operator a partial migration can be resumed \
                         (`{resumption}`) and never withdraws it. It cannot be resumed: every \
                         later run is refused before a statement is sent"
                    )
                };
                assert!(
                    at > withdrawn_at,
                    "{name} states `{resumption}` BEFORE the sentence that withdraws it, so it \
                     reads as live guidance rather than as the quoted mistake"
                );
            }
        }
    }

    use super::scan_manifests;

    /// `TOTAL_STEPS` equals the number of steps the published contract lists.
    ///
    /// The contract table drifted from the implementation once already: it listed ten steps while
    /// `xtask` ran eleven, having never mentioned the architecture-invariants step at all. Nothing
    /// detected that, because nothing compared the two. This does.
    ///
    /// The contract is read with `include_str!`, so it resolves at COMPILE time — a moved or
    /// deleted contract is a build failure rather than a silently skipped test.
    #[test]
    fn the_step_count_matches_the_published_contract() {
        const CONTRACT: &str = include_str!("../../contracts/verification-sequence.md");

        // Rows of the step table look like `| 7 | Architecture invariants | ... |`.
        let mut numbers: Vec<usize> = Vec::new();
        for line in CONTRACT.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix('|') else {
                continue;
            };
            let Some((first, _)) = rest.split_once('|') else {
                continue;
            };
            if let Ok(number) = first.trim().parse::<usize>() {
                numbers.push(number);
            }
        }

        // POSITIVE CONTROL FIRST. A parse that matched nothing would make every assertion below
        // vacuously true, which is the failure mode this whole file is written against.
        assert!(
            numbers.len() >= super::TOTAL_STEPS,
            "the contract parse found only {} numbered rows; it is not reading the step table",
            numbers.len()
        );

        // The step table is the leading run 1, 2, 3, ... — the exit-code table that follows also
        // has numbered rows, and it legitimately starts again at 0.
        let mut steps = 0usize;
        for (index, number) in numbers.iter().enumerate() {
            if *number == index + 1 {
                steps = *number;
            } else {
                break;
            }
        }

        assert_eq!(
            steps,
            super::TOTAL_STEPS,
            "the contract publishes {steps} steps but TOTAL_STEPS is {}. One of them was changed \
             without the other; they are the same promise stated twice.",
            super::TOTAL_STEPS
        );
    }

    /// The published exit-code tables describe what the command actually does with exit 2.
    ///
    /// # The drift this catches
    ///
    /// `contracts/verification-sequence.md` described exit **2** as *"a required toolchain is
    /// missing; no steps ran"*. Phase 008 widened the condition: step 1 now also refuses when the
    /// four-row database environment is absent, and returns the same code. So the contract named
    /// one of the two conditions that produce it, and a reader diagnosing a `2` had no reason to
    /// look at their environment variables.
    ///
    /// The same code is published in three places — this contract, `CONTRIBUTING.md`, and the
    /// doc comment on `EXIT_TOOLING_MISSING` — and nothing bound them together. Two of the three
    /// were updated. This is what notices next time.
    ///
    /// **What this does not do.** It asserts the two conditions are *named*, not that the wording
    /// is good. A table saying `2` means "tool or database" while the implementation returned it
    /// for something else entirely would still pass; the behavioural proof is
    /// `a_missing_database_prerequisite_can_never_yield_exit_zero`.
    #[test]
    fn the_published_exit_codes_describe_what_the_command_does() {
        const CONTRACT: &str = include_str!("../../contracts/verification-sequence.md");
        const CONTRIBUTING: &str = include_str!("../../CONTRIBUTING.md");

        /// The meaning column of the exit-code row for `code`.
        ///
        /// Keyed on the row having exactly TWO cells. Both documents carry other numbered tables —
        /// the contract's step table has four columns and legitimately has a row numbered 2 — and
        /// an earlier parser in this file was written after exactly that collision.
        fn meaning_of(markdown: &str, code: u8) -> Option<String> {
            markdown.lines().find_map(|line| {
                let trimmed = line.trim();
                if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
                    return None;
                }
                let cells: Vec<&str> = trimmed
                    .trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .collect();
                if cells.len() != 2 {
                    return None;
                }
                (cells[0].trim_matches('`').parse::<u8>().ok()? == code)
                    .then(|| cells[1].to_owned())
            })
        }

        // POSITIVE CONTROLS. A parser that matched nothing, or that matched the step table
        // instead, would make every assertion below vacuous. Row 1 must be found in both, and it
        // must be the exit-code row rather than the contract's step row named "Formatting".
        for (label, markdown) in [("contract", CONTRACT), ("guide", CONTRIBUTING)] {
            let one = meaning_of(markdown, 1)
                .unwrap_or_else(|| panic!("the {label} exit-code table has no row for 1"));
            assert!(
                one.contains("failed"),
                "the {label} row for 1 reads as the step table rather than the exit-code table"
            );
        }

        for (label, markdown) in [("contract", CONTRACT), ("guide", CONTRIBUTING)] {
            let two = meaning_of(markdown, 2)
                .unwrap_or_else(|| panic!("the {label} exit-code table has no row for 2"));
            let lowered = two.to_lowercase();
            assert!(
                lowered.contains("tool"),
                "the {label} row for 2 does not name the missing-tooling condition"
            );
            assert!(
                lowered.contains("database"),
                "the {label} row for 2 names only the tooling condition, but \
                 `missing_database_prerequisites` returns the same code for an absent four-row \
                 environment"
            );
        }

        // The third authority: the constant's own documentation, which an author reads before the
        // published tables.
        const SOURCE: &str = include_str!("main.rs");
        let declaration = SOURCE
            .lines()
            .zip(SOURCE.lines().skip(1))
            .find(|(_, next)| next.contains("const EXIT_TOOLING_MISSING"))
            .map(|(doc, _)| doc.to_lowercase())
            .expect("EXIT_TOOLING_MISSING is declared with a doc comment above it");
        assert!(
            declaration.contains("database"),
            "EXIT_TOOLING_MISSING's own documentation names only the tooling condition"
        );
    }

    /// The published contract does not claim that a step of the sequence currently fails.
    ///
    /// It did. Under "Working-tree cleanliness" the contract carried a **Known starting
    /// condition** asserting that `.DS_Store`, `.idea/`, and `.playwright-mcp/` were
    /// inadequately ignored and that *"step 11 fails until the ignore rules are corrected"*.
    /// The ignore rules were corrected; the sentence was not. So a normative document
    /// described its own subject as failing while `cargo xtask verify` exited 0 — and this
    /// test file runs inside step 4 of that very sequence, which means the contract was
    /// predicting a failure of the run that was executing it.
    ///
    /// Two independent bindings, neither of which is a prose search for a failure claim:
    ///
    /// 1. **Structural** — the `Known starting condition` label must not reappear. That
    ///    label is the exact vector: it is where a *current-state* assertion was parked
    ///    inside a document that otherwise states obligations.
    /// 2. **Factual** — the three artefact paths that label named must carry a rule in the
    ///    tracked ignore file.
    ///
    /// **What this does not do**, stated so the guard is not credited with more than it
    /// buys: binding 1 catches the label, not every conceivable false current-state claim
    /// phrased some other way; and binding 2 proves a *rule is present*, not that a given
    /// path resolves as ignored. The end-to-end proof is the working-tree step itself — now
    /// step 9 — which runs `git status --porcelain` on every invocation. This test guards the
    /// prose; the executable step guards the behaviour.
    #[test]
    fn the_contract_does_not_claim_the_sequence_currently_fails() {
        const CONTRACT: &str = include_str!("../../contracts/verification-sequence.md");
        const IGNORE_RULES: &str = include_str!("../../.gitignore");

        // POSITIVE CONTROL FIRST. If either file were empty or moved, every assertion below
        // would pass vacuously — which is the failure mode this whole module is written
        // against. `include_str!` already makes a moved file a build error; these prove the
        // *content* being searched is the content that matters.
        assert!(
            CONTRACT.contains("## Working-tree cleanliness"),
            "the verification contract has no working-tree cleanliness section; this test is \
             searching the wrong content"
        );
        assert!(
            IGNORE_RULES.lines().any(|line| line.trim() == "target/"),
            "the tracked ignore file does not contain the build-output rule; this test is \
             searching the wrong content"
        );

        // (1) The defect vector.
        assert!(
            !CONTRACT.contains("Known starting condition"),
            "contracts/verification-sequence.md carries a `Known starting condition` block \
             again. That block asserted a CURRENT failure of step 11 and stayed after the \
             cause was fixed. A contract states obligations; the current result of meeting \
             them belongs in a CI run, not in normative text."
        );

        // (2) The artefacts that block named are covered by the tracked ignore rules.
        for rule in [".idea/", ".DS_Store", ".playwright-mcp/"] {
            assert!(
                IGNORE_RULES.lines().any(|line| line.trim() == rule),
                "the tracked ignore file has no `{rule}` rule. Step 9 asserts the working \
                 tree is clean after a full run, so an uncovered editor or OS artefact fails \
                 the sequence — and the contract no longer warns anyone that it would."
            );
        }

        // NEGATIVE CONTROL. A rule-matcher that reported everything present would make the
        // loop above meaningless. A tracked file must NOT be found as an ignore rule.
        assert!(
            !IGNORE_RULES.lines().any(|line| line.trim() == "README.md"),
            "the rule matcher reports a tracked file as an ignore rule; it is not \
             discriminating and the assertions above prove nothing"
        );
    }

    /// Every required tool names the step that actually consumes it.
    ///
    /// `report_missing` prints `Tool::purpose` verbatim, so these strings are **observable
    /// output**, and `contracts/verification-sequence.md` publishes an example of that output.
    /// They drifted when the sequence changed, so the values remain bound to the executable
    /// sequence rather than being trusted as prose.
    ///
    /// The step-count tests did not catch it and could not: they compare a **count**, and the
    /// count was right the whole time. A defect in which value sits next to which name is only
    /// visible to a test that asserts the values. This one does, exactly, per tool.
    #[test]
    fn every_required_tool_names_the_step_that_consumes_it() {
        // The right-hand side is the executable sequence, read off the `run`/`step_ok` call
        // sites in `verify` — not copied from the table it is meant to check.
        const EXPECTED: &[(&str, &str)] = &[
            (
                "git",
                "secret scanning and working-tree cleanliness, steps 8 and 9",
            ),
            ("rustfmt", "formatting, step 2"),
            ("clippy", "lint, step 3"),
            ("cargo-deny", "dependency and licence policy, step 6"),
            ("gitleaks", "secret scan, step 8"),
        ];

        // POSITIVE CONTROL: the expectation table must cover the real table exactly. Without
        // this, adding a tool with a wrong purpose would pass by never being looked at, and
        // deleting a tool would pass by leaving an expectation nothing compares against.
        assert_eq!(
            EXPECTED.len(),
            super::REQUIRED.len(),
            "REQUIRED has {} tools but this test expects {}. A tool was added or removed \
             without updating the expected purpose beside it.",
            super::REQUIRED.len(),
            EXPECTED.len()
        );

        for (name, expected_purpose) in EXPECTED {
            let tool = super::REQUIRED
                .iter()
                .find(|t| t.name == *name)
                .unwrap_or_else(|| panic!("REQUIRED has no tool named `{name}`"));
            assert_eq!(
                tool.purpose, *expected_purpose,
                "`{name}` tells the user its purpose is {:?}, but it is consumed by {:?}. \
                 This string is printed verbatim by `report_missing`, so it is a published \
                 contract, not a comment.",
                tool.purpose, expected_purpose
            );
        }

        // No purpose may name a step the sequence does not have. This catches the direction the
        // table above cannot: an expectation and an implementation that are wrong together.
        for tool in super::REQUIRED {
            for token in tool.purpose.split(|c: char| !c.is_ascii_digit()) {
                if token.is_empty() {
                    continue;
                }
                let step: usize = token.parse().expect("digits parse");
                assert!(
                    (1..=super::TOTAL_STEPS).contains(&step),
                    "`{}` names step {step}, outside the 1..={} sequence",
                    tool.name,
                    super::TOTAL_STEPS
                );
            }
        }
    }

    /// A publishable manifest with the `{ path, version }` form the rule permits.
    fn compliant() -> (String, String) {
        (
            "renvor".to_owned(),
            "[package]\nname = \"renvor\"\n\n[dependencies]\n\
             renvor-core = { path = \"../renvor-core\", version = \"0.0.0\" }\n\
             tokio = { version = \"1.53.1\" }\n\
             \n[dev-dependencies]\nscratch = { path = \"../scratch\" }\n"
                .to_owned(),
        )
    }

    /// A second publishable manifest, so the `publishable < 2` control is satisfied.
    fn second() -> (String, String) {
        (
            "renvor-core".to_owned(),
            "[package]\nname = \"renvor-core\"\n\n[dependencies]\npetgraph = { version = \"0.8.3\" }\n"
                .to_owned(),
        )
    }

    #[test]
    fn the_permitted_path_and_version_form_passes() {
        // POSITIVE CONTROL for every refusal below: the scan accepts what the corrected rule
        // permits, so its refusals are about the offending shape rather than about paths on sight.
        // The dev-dependency here is path-ONLY and must be ignored: dev-dependencies are stripped
        // from the published manifest.
        scan_manifests(&[compliant(), second()]).expect("`{ path, version }` is permitted");
    }

    #[test]
    fn a_path_only_dependency_is_refused() {
        let broken = (
            "renvor".to_owned(),
            "[package]\nname = \"renvor\"\n\n[dependencies]\n\
             renvor-core = { path = \"../renvor-core\" }\n"
                .to_owned(),
        );
        let reason = scan_manifests(&[broken, second()]).expect_err("path-only is prohibited");
        assert!(reason.contains("PATH-ONLY"), "{reason}");
        assert!(reason.contains("renvor-core"), "names it: {reason}");
    }

    #[test]
    fn a_git_dependency_is_refused() {
        let broken = (
            "renvor".to_owned(),
            "[package]\nname = \"renvor\"\n\n[dependencies]\n\
             thing = { git = \"https://example.invalid/thing\" }\n"
                .to_owned(),
        );
        let reason = scan_manifests(&[broken, second()]).expect_err("git is prohibited");
        assert!(reason.contains("git dependency"), "{reason}");
    }

    #[test]
    fn a_dependency_sub_table_is_refused_rather_than_skipped() {
        // The failure mode a line-oriented scan is prone to: a shape it cannot read looks exactly
        // like a shape with nothing wrong. This must fail closed.
        let unreadable = (
            "renvor".to_owned(),
            "[package]\nname = \"renvor\"\n\n[dependencies.renvor-core]\npath = \"../renvor-core\"\n"
                .to_owned(),
        );
        let reason =
            scan_manifests(&[unreadable, second()]).expect_err("an unreadable shape fails");
        assert!(reason.contains("sub-table"), "{reason}");
    }

    #[test]
    fn a_non_publishable_package_is_exempt() {
        let internal = (
            "xtask".to_owned(),
            "[package]\nname = \"xtask\"\npublish = false\n\n[dependencies]\n\
             thing = { git = \"https://example.invalid/thing\" }\n"
                .to_owned(),
        );
        scan_manifests(&[compliant(), second(), internal])
            .expect("`publish = false` is what exempts a package from this rule");
    }

    #[test]
    fn the_real_workspace_satisfies_the_rule() {
        // The check run against the actual manifests, not only synthetic ones. `cargo xtask
        // verify` step 7 runs the same pair; this makes `cargo test` run it too, so a manifest
        // regression is caught by the fast suite rather than only by the full sequence.
        let root = super::workspace_root();
        let manifests =
            super::workspace_manifests(&root).expect("the workspace manifests are readable");
        assert!(
            manifests.len() >= 4,
            "discovery found only {} manifest(s): {:?}",
            manifests.len(),
            manifests.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );
        scan_manifests(&manifests).expect("the real workspace satisfies FR-040");
    }

    // ── required package metadata (FR-040, `contracts/package-metadata.md`) ──────────────────

    /// A workspace root that provides the fields the fixtures below inherit.
    fn workspace_root_manifest() -> String {
        "[workspace.package]\n\
         version = \"0.0.0\"\n\
         rust-version = \"1.94.0\"\n\
         license = \"MIT OR Apache-2.0\"\n\
         repository = \"https://example.invalid/repo\"\n\
         homepage = \"https://example.invalid\"\n"
            .to_owned()
    }

    /// A manifest declaring everything the contract requires.
    fn complete_manifest() -> String {
        "[package]\n\
         name = \"renvor-example\"\n\
         version.workspace = true\n\
         description = \"One sentence.\"\n\
         license.workspace = true\n\
         repository.workspace = true\n\
         homepage.workspace = true\n\
         documentation = \"https://docs.rs/renvor-example\"\n\
         readme = \"README.md\"\n\
         keywords = [\"renvor\"]\n\
         categories = [\"web-programming\"]\n\
         rust-version.workspace = true\n\
         include = [\"src/**\"]\n\
         \n\
         [dependencies]\n"
            .to_owned()
    }

    /// Removes one `[package]` field from a manifest, whichever form it takes.
    fn without_field(manifest: &str, field: &str) -> String {
        manifest
            .lines()
            .filter(|line| {
                let trimmed = line.trim();
                !(trimmed.starts_with(&format!("{field} ="))
                    || trimmed.starts_with(&format!("{field}.workspace")))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_complete_manifest_passes_the_metadata_check() {
        // POSITIVE CONTROL. Without it, a check that rejected everything would satisfy every
        // negative test below and the suite would prove nothing about acceptance.
        let examined = super::required_metadata_is_declared(
            &[("renvor-example".to_owned(), complete_manifest())],
            &workspace_root_manifest(),
        )
        .expect("a complete manifest is accepted");
        assert_eq!(
            examined, 1,
            "the check examined the wrong number of packages"
        );
    }

    #[test]
    fn removing_any_required_field_is_rejected() {
        // TABLE-DRIVEN. One case per field the contract names, each proving the check actually
        // depends on that field rather than on some other line happening to be present.
        for field in super::REQUIRED_PACKAGE_FIELDS {
            let broken = without_field(&complete_manifest(), field);
            assert_ne!(
                broken,
                complete_manifest(),
                "the fixture never contained `{field}`, so removing it tested nothing"
            );

            let reason = super::required_metadata_is_declared(
                &[("renvor-example".to_owned(), broken)],
                &workspace_root_manifest(),
            )
            .expect_err("a missing required field must be rejected");

            assert!(
                reason.contains(field),
                "`{field}` was removed but the failure named something else: {reason}"
            );
        }
    }

    #[test]
    fn a_manifest_declaring_neither_include_nor_exclude_is_rejected() {
        let broken = without_field(&complete_manifest(), "include");
        let reason = super::required_metadata_is_declared(
            &[("renvor-example".to_owned(), broken)],
            &workspace_root_manifest(),
        )
        .expect_err("an unstated file set is rejected");
        assert!(reason.contains("include"), "{reason}");
    }

    #[test]
    fn an_exclude_satisfies_the_file_set_rule_just_as_include_does() {
        // CONTROL for the test above: the rule is "state the set", not "use `include`".
        let with_exclude = without_field(&complete_manifest(), "include").replace(
            "[dependencies]",
            "exclude = [\"tests/**\"]\n\n[dependencies]",
        );
        super::required_metadata_is_declared(
            &[("renvor-example".to_owned(), with_exclude)],
            &workspace_root_manifest(),
        )
        .expect("`exclude` states the shipped set just as `include` does");
    }

    #[test]
    fn a_field_inherited_from_a_workspace_that_does_not_declare_it_is_rejected() {
        // The failure this catches is invisible to a presence check: `license.workspace = true`
        // LOOKS declared and resolves to nothing.
        let empty_workspace = "[workspace.package]\nversion = \"0.0.0\"\n";
        let reason = super::required_metadata_is_declared(
            &[("renvor-example".to_owned(), complete_manifest())],
            empty_workspace,
        )
        .expect_err("an unresolvable inherited field is rejected");
        assert!(
            reason.contains("inherits") && reason.contains("resolves to nothing"),
            "{reason}"
        );
    }

    #[test]
    fn a_package_marked_unpublishable_is_exempt() {
        // `xtask` declares `publish = false` and is not held to the publication contract.
        let internal = "[package]\nname = \"xtask\"\npublish = false\n".to_owned();
        let examined = super::required_metadata_is_declared(
            &[
                ("renvor-example".to_owned(), complete_manifest()),
                ("xtask".to_owned(), internal),
            ],
            &workspace_root_manifest(),
        )
        .expect("an unpublishable package is exempt");
        assert_eq!(examined, 1, "the exempt package was counted as publishable");
    }

    #[test]
    fn every_real_publishable_package_declares_its_required_metadata() {
        // The check run against the ACTUAL manifests. This is the assertion that would have failed
        // on 2026-08-22, when four required fields could be deleted and `cargo package` still
        // exited zero.
        let root = super::workspace_root();
        let manifests =
            super::workspace_manifests(&root).expect("the workspace manifests are readable");
        let root_manifest = std::fs::read_to_string(root.join("Cargo.toml"))
            .expect("the workspace root manifest is readable");

        let examined = super::required_metadata_is_declared(&manifests, &root_manifest)
            .expect("every publishable package satisfies the metadata contract");

        // THIRTEEN since Phase 009 batch J: `renvor-auth-http` joined the twelve that earlier
        // Phase 009 work left, which were themselves the eleven Phase 007 ended with plus
        // `renvor-auth`. The number is asserted rather than inferred so that adding a publishable
        // package without adding it to the release ordering fails here — which is exactly what
        // happened when this count was five and three packages had just been added, again at
        // Phase 006, again at Phase 007, again earlier in Phase 009 for `renvor-auth`, and AGAIN
        // at batch J for `renvor-auth-http`.
        //
        // That fifth occurrence is the instructive one. Batch J *did* add the crate to
        // `RELEASING.md`'s ordered table and to the release rehearsal's `CRATES` list — the two
        // lists a publication actually reads — and still left both spelled counts at twelve. The
        // structural lists agreed with the manifests while two sentences did not, which is the
        // failure mode a count derived from `cargo metadata` cannot have and a sentence always
        // can. Raising the pin is the correct response only once those lists are confirmed to
        // already carry the new package, which they were.
        //
        // It is the cheapest test in this file and the one that has caught the most.
        //
        // EIGHTEEN since Phase 010: the five capability crates — `renvor-cache`, `renvor-jobs`,
        // `renvor-mail`, `renvor-storage`, `renvor-observability` — joined the thirteen Phase 009
        // ended with. Their skeletons, this pin, `RELEASING.md`'s table and sentence, and the
        // rehearsal's `CRATES` list landed in ONE commit, so no state ever existed in which the
        // manifests and the two spelled counts disagreed.
        assert_eq!(
            examined, 18,
            "the workspace publishes eighteen packages; the scan examined {examined}"
        );
    }

    /// No allowlist in `.gitleaks.toml` excludes shipped source by **path**.
    ///
    /// # The fix this refuses
    ///
    /// Step 8 failed on 2026-08-30 with two `generic-api-key` matches on a synthetic
    /// opaque-token fixture in `renvor-auth-http`'s test application. The narrow fix is a
    /// content regex, which is what FP-003 uses.
    ///
    /// The TEMPTING fix is `paths = ['''^crates/renvor-auth-http/''']`, and it is the one this
    /// test exists to refuse. Gitleaks skips an allowlisted path **before reading it**, so that
    /// entry would stop the scanner opening the file for all time — and the file in question is a
    /// test application, which is precisely where a real credential gets pasted by accident.
    ///
    /// This is not a hypothetical. FP-001 records the same property proven by experiment: a
    /// `paths` form of that entry produced `scanned ~0 bytes` and let an injected canary through
    /// undetected. That finding lived only in a comment, where the next person to hit a step 8
    /// failure at 3am would not read it. This test is that comment made enforceable.
    #[test]
    fn no_gitleaks_allowlist_excludes_shipped_source_by_path() {
        let root = super::workspace_root();
        let config = std::fs::read_to_string(root.join(".gitleaks.toml"))
            .expect(".gitleaks.toml is readable");

        let mut path_entries = 0_usize;
        for (index, line) in config.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.starts_with("paths") {
                continue;
            }
            path_entries += 1;
            assert!(
                !trimmed.contains("crates"),
                ".gitleaks.toml line {} excludes shipped source by PATH: `{trimmed}`. A path \
                 allowlist stops gitleaks reading the file at all, so a real credential added to \
                 it later would never be reported. Suppress the specific match with `regexes`",
                index + 1
            );
        }

        // Positive control: a scan that matched nothing would pass the loop above without
        // examining anything. FP-002 declares `paths = ['''^target/''']`, so at least one entry
        // must be seen — otherwise this test is reporting success for having read nothing.
        assert!(
            path_entries >= 1,
            "no `paths` allowlist was found at all, so this check proved nothing — FP-002 \
             declares one and the parser must be able to see it"
        );
    }

    /// `RELEASING.md`'s publishable-package headline agrees with the manifests.
    ///
    /// # The third occurrence of one failure
    ///
    /// A count stated in prose above a table nobody re-derives. `governance/waivers.md` did it
    /// twice — six-versus-seven, then eleven-versus-thirteen — and after the second the count test
    /// was written. `RELEASING.md` then did it a third time, saying **Eight** while its own table
    /// listed eleven and `xtask` asserted eleven, because the test written after the second
    /// occurrence covered only the two files that had already failed.
    ///
    /// This covers the third. The number is read from `cargo metadata`, so it tracks the
    /// manifests rather than another sentence.
    #[test]
    fn publishable_package_count_is_stated_correctly() {
        let root = super::workspace_root();
        let manifests =
            super::workspace_manifests(&root).expect("the workspace manifests are readable");
        let root_manifest = std::fs::read_to_string(root.join("Cargo.toml"))
            .expect("the workspace root manifest is readable");
        let examined = super::required_metadata_is_declared(&manifests, &root_manifest)
            .expect("every publishable package satisfies the metadata contract");

        let words = [
            "Zero",
            "One",
            "Two",
            "Three",
            "Four",
            "Five",
            "Six",
            "Seven",
            "Eight",
            "Nine",
            "Ten",
            "Eleven",
            "Twelve",
            "Thirteen",
            "Fourteen",
            "Fifteen",
            "Sixteen",
            "Seventeen",
            "Eighteen",
            "Nineteen",
            "Twenty",
        ];
        let spelled = words
            .get(examined)
            .unwrap_or_else(|| panic!("no spelling for {examined}"));

        let releasing =
            std::fs::read_to_string(root.join("RELEASING.md")).expect("RELEASING.md is readable");
        assert!(
            releasing.contains(&format!("**{spelled} publishable packages.**")),
            "RELEASING.md does not say `**{spelled} publishable packages.**`, which is what the \
             manifests carry"
        );

        // The list the release rehearsal actually publishes must be the same size.
        let workflow = std::fs::read_to_string(root.join(".github/workflows/release-dry-run.yml"))
            .expect("the release dry-run workflow is readable");
        let listed = workflow
            .lines()
            .find_map(|line| line.trim().strip_prefix("CRATES:"))
            .expect("the workflow declares a CRATES list")
            .trim()
            .trim_matches('"')
            .split_whitespace()
            .count();
        assert_eq!(
            listed, examined,
            "the release rehearsal publishes {listed} crates but the workspace has {examined}"
        );
    }

    /// The publication order in `release-dry-run.yml` is topologically valid.
    ///
    /// # Why the workflow's own assertion cannot do this
    ///
    /// It `sort`s both sides before comparing, so it checks **membership** and says nothing about
    /// **order** — and `cargo publish --dry-run` computes its own order rather than following the
    /// list, so a wrong order dry-runs green and fails only on a real publish, with a
    /// missing-dependency registry error that reads like a network fault.
    ///
    /// Phase 007 broke it: making room for `renvor-testkit` moved `renvor-database` from position
    /// 6 to 4, past `renvor-validation`, which it depends on. A dependency review found it.
    ///
    /// # Read from `cargo tree`, not from a hand-rolled parse
    ///
    /// The first version of this test sliced `cargo metadata`'s JSON by hand — `xtask` carries no
    /// dependencies, so there is no JSON crate — and it reported `renvor-core` depending on
    /// `renvor-error`, which is false: the slice ran past the package's own block. `normal_edges`
    /// is the same helper step 7 uses, and it answers the question directly.
    #[test]
    fn publication_order_is_topological() {
        let root = super::workspace_root();
        let workflow = std::fs::read_to_string(root.join(".github/workflows/release-dry-run.yml"))
            .expect("the release dry-run workflow is readable");
        let line = workflow
            .lines()
            .find_map(|line| line.trim().strip_prefix("CRATES:"))
            .expect("the workflow declares a CRATES list");
        let order: Vec<String> = line
            .trim()
            .trim_matches('"')
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        assert!(
            order.len() > 1,
            "the CRATES list parsed to {order:?}, so this test is checking nothing"
        );

        for (index, package) in order.iter().enumerate() {
            let tree = super::normal_edges(&root, &["-p", package, "--all-features"])
                .unwrap_or_else(|| panic!("`cargo tree -p {package}` failed"));
            for (later, later_index) in order.iter().zip(0..).skip(index + 1) {
                let resolves = tree.lines().any(|line| {
                    line.split_once(' ')
                        .is_some_and(|(name, rest)| name == later && rest.starts_with('v'))
                });
                assert!(
                    !resolves,
                    "`{package}` publishes at position {index} but depends on `{later}` at \
                     position {later_index}. `cargo publish` would fail on `{package}` with a \
                     missing-`{later}` registry error"
                );
            }
        }

        // CONTROL. A walk that saw nothing would pass every assertion above. The facade is last
        // and must resolve the kernel, which is first.
        let facade = super::normal_edges(&root, &["-p", "renvor", "--all-features"])
            .expect("the facade tree query works");
        assert!(
            facade.lines().any(|line| line.starts_with("renvor-core v")),
            "the facade does not resolve renvor-core, so the ordering assertions above prove \
             nothing"
        );
    }

    /// Persistence isolation holds, and is reachable from `cargo test` as well as from `verify`.
    ///
    /// Running it here too is deliberate. Step 7 runs it inside the full sequence, which takes
    /// minutes; this gives the same answer in seconds, and it means the check is exercised by
    /// `cargo test --workspace` on every machine and in CI even if the sequence is not run.
    #[test]
    fn persistence_isolation_holds_with_its_controls() {
        assert!(
            super::persistence_isolation_holds(&super::workspace_root()),
            "persistence feature isolation failed — see the printed step-7 detail above"
        );
    }

    /// The waiver counts must agree with the waiver table, in every place either is stated.
    ///
    /// # This is the third occurrence of the same defect
    ///
    /// `governance/waivers.md`'s headline said "6 active waivers" while the table carried seven,
    /// and was corrected on 2026-08-21. It then said "11" while the table carried thirteen, from
    /// 2026-08-24 until Phase 007's preconditions audit found it — and that time `GOVERNANCE.md`
    /// was stale too, so the cross-check that caught the first occurrence did not catch the second.
    ///
    /// A count is a claim about a table sitting a few hundred lines below it. Nobody re-reads both.
    /// So the claim is asserted here instead, in every place it is made: the ledger headline, the
    /// ledger's level-and-phase summary table, and `GOVERNANCE.md`'s prose and table.
    #[test]
    fn the_active_waiver_counts_match_the_waiver_table() {
        let root = super::workspace_root();
        let ledger = std::fs::read_to_string(root.join("governance/waivers.md"))
            .expect("the waiver ledger is readable");
        let governance =
            std::fs::read_to_string(root.join("GOVERNANCE.md")).expect("GOVERNANCE.md is readable");

        /// Every distinct `W-###` that opens a table row in `text`.
        fn identifiers(text: &str) -> Vec<String> {
            let mut found: Vec<String> = text
                .lines()
                .filter_map(|line| {
                    let trimmed = line
                        .trim_start()
                        .strip_prefix('|')?
                        .trim()
                        .trim_matches('*');
                    trimmed
                        .starts_with("W-")
                        .then(|| trimmed.chars().take(5).collect::<String>())
                })
                .collect();
            found.sort();
            found.dedup();
            found
        }

        let granted = identifiers(&ledger);
        // A POSITIVE CONTROL. A parser that silently matched nothing would make every assertion
        // below compare zero against zero and pass.
        assert!(
            granted.len() >= 11,
            "the waiver-row parser found only {} rows, so it is not reading the table",
            granted.len()
        );

        assert!(
            ledger.contains(&format!("**{} active waivers**", granted.len())),
            "the ledger headline does not say `{} active waivers`, which is what its table carries",
            granted.len()
        );

        // The level-and-phase summary lists every waiver EXCEPT W-001, which is the approval gap
        // rather than a review-gap exception and is counted separately by the ledger's own rules.
        for id in &granted {
            if id == "W-001" {
                continue;
            }
            assert!(
                ledger.matches(id.as_str()).count() > 1,
                "{id} appears in the ledger's table but nowhere else in it, so at least one \
                 summary was left behind"
            );
            assert!(
                governance.contains(id.as_str()),
                "{id} is granted in the ledger but absent from GOVERNANCE.md's table"
            );
        }

        let words = [
            "Zero",
            "One",
            "Two",
            "Three",
            "Four",
            "Five",
            "Six",
            "Seven",
            "Eight",
            "Nine",
            "Ten",
            "Eleven",
            "Twelve",
            "Thirteen",
            "Fourteen",
            "Fifteen",
            "Sixteen",
            "Seventeen",
            "Eighteen",
            "Nineteen",
            "Twenty",
            "Twenty-one",
            "Twenty-two",
            "Twenty-three",
            "Twenty-four",
            "Twenty-five",
            "Twenty-six",
            "Twenty-seven",
            "Twenty-eight",
            "Twenty-nine",
            "Thirty",
        ];
        let spelled = words
            .get(granted.len())
            .unwrap_or_else(|| panic!("no spelling for {}", granted.len()));
        assert!(
            governance.contains(&format!("**{spelled}** waivers are currently active")),
            "GOVERNANCE.md does not say `**{spelled}** waivers are currently active`, which is \
             what the ledger's table carries"
        );
        assert_eq!(
            identifiers(&governance).len(),
            granted.len(),
            "GOVERNANCE.md's waiver table has a different number of rows from the ledger's"
        );

        // ── the prose counts, which the checks above do not reach ──────────────────────────
        //
        // The headline, the summary table and GOVERNANCE.md were all bound. The SENTENCE that
        // states why the exceptions exist was not, and it said "All fourteen" while the set it
        // names held fifteen. It had been wrong once before — it read "All eight" while listing
        // ten — and was corrected by hand both times, which is the shape this repository treats as
        // a defect rather than a typo.
        //
        // The exception set is every granted waiver except W-001, which is the approval gap and is
        // counted separately by this ledger's own rules — the same exclusion the loop above makes.
        //
        // Not every exception has that reason. On 2026-09-04 the ledger gained two waivers of a
        // rule that has nothing to do with who reviews — constitution principle VII's generator
        // obligation, left unmet by two library-only phases — and a sentence that said "All
        // twenty-two exist for the same underlying reason: the project has one person" would have
        // been false the moment it was written. So the sentence is bound to the set it may claim:
        // the waivers whose VIOLATED RULE is an independent-review rule, read from the table's own
        // rule cell rather than from a list this test would have to maintain by hand. The rest are
        // counted too, and the ledger must say they exist for a different reason.
        fn review_gap(text: &str) -> Vec<String> {
            let mut found: Vec<String> = text
                .lines()
                .filter_map(|line| {
                    let mut cells = line.trim_start().strip_prefix('|')?.split('|');
                    let identifier = cells.next()?.trim().trim_matches('*');
                    let rule = cells.next()?;
                    (identifier.starts_with("W-") && rule.contains("independent"))
                        .then(|| identifier.chars().take(5).collect::<String>())
                })
                .collect();
            found.sort();
            found.dedup();
            found
        }
        let exceptions: Vec<&String> = granted.iter().filter(|id| *id != "W-001").collect();
        let review_gap = review_gap(&ledger);
        // A POSITIVE CONTROL for the rule-cell parser, in the same shape as the one above: a parser
        // that read the wrong cell would find no rule mentioning independence and count zero.
        assert!(
            review_gap.len() >= 11 && review_gap.iter().all(|id| exceptions.contains(&id)),
            "the review-gap parser found {} rows, so it is not reading the rule cell",
            review_gap.len()
        );
        let spelled_review_gap = words
            .get(review_gap.len())
            .unwrap_or_else(|| panic!("no spelling for {}", review_gap.len()));
        assert!(
            ledger.contains(&format!(
                "**All {} exist for the same underlying reason",
                spelled_review_gap.to_lowercase()
            )),
            "the ledger does not say `All {} exist for the same underlying reason`, which is what \
             its own review-gap exception set holds",
            spelled_review_gap.to_lowercase()
        );
        let other = exceptions.len() - review_gap.len();
        if other > 0 {
            let spelled_other = words
                .get(other)
                .unwrap_or_else(|| panic!("no spelling for {other}"));
            assert!(
                ledger.contains(&format!("**The other {}, ", spelled_other.to_lowercase())),
                "the ledger holds {other} exception(s) whose violated rule is not an \
                 independent-review rule, and does not say `The other {} ...` exist for a different \
                 reason",
                spelled_other.to_lowercase()
            );
        }

        // ── nothing is described as granted before it is granted ───────────────────────────
        //
        // The W-017 scope section said Phase closure "is **W-018**, granted separately, on the
        // same terms" while no W-018 existed, and a blockquote saying "Both were granted" sat
        // directly under a section describing one. A ledger that names a waiver it has not issued
        // reads, to anyone auditing it, as one that has.
        //
        // W-007 is the single exemption, and an explicit one: both documents record at length that
        // it does not exist and that the identifier is permanently burned.
        for (label, text) in [("ledger", &ledger), ("GOVERNANCE.md", &governance)] {
            let mut rest = text.as_str();
            while let Some(at) = rest.find("W-0") {
                let identifier: String = rest[at..].chars().take(5).collect();
                rest = &rest[at + 3..];
                if identifier.len() != 5 || !identifier[2..].chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                if identifier == "W-007" {
                    continue;
                }
                assert!(
                    granted.contains(&identifier),
                    "{label} refers to {identifier}, which its own table does not grant"
                );
            }
        }
    }

    #[test]
    fn the_phase_evidence_distinguishes_workspace_from_generated_project_evidence() {
        // FR-039 and SC-020: development/workspace integration evidence must be distinguished from
        // evidence about what an externally generated project can build. Both were recorded as
        // satisfied with NO executable evidence — the distinction was a property of prose that
        // nothing read back.
        let root = super::workspace_root();
        let evidence = std::fs::read_to_string(root.join("governance/phase-004-evidence.md"))
            .expect("the phase evidence pack is readable");

        // The limitation that makes the distinction matter must be stated, not implied.
        assert!(
            evidence.contains("**No generated project depends on the framework**")
                && evidence.contains("`renvor routes` reaches no generated project"),
            "the evidence pack no longer states that no generated project depends on the \
             framework, which is the fact that keeps workspace evidence from reading as evidence \
             about generated projects"
        );

        // And it must not claim the converse anywhere.
        for forbidden in [
            "proves a generated project can build",
            "generated projects are proven to build",
        ] {
            assert!(
                !evidence.contains(forbidden),
                "the evidence pack claims workspace integration as evidence about externally \
                 generated projects: `{forbidden}`"
            );
        }
    }

    #[test]
    fn the_relay_test_named_by_the_gate_exists() {
        // The gate names one test by its exact path. If that path stops resolving, `--exact`
        // matches nothing, `cargo test` exits 0 with "0 passed", and the gate reports success
        // while running the end-to-end proof not at all. Measured, not assumed: a filter naming a
        // non-existent test was run by hand and exited 0.
        //
        // `the_end_to_end_relay_ran` defends against that at run time by reading the count back.
        // This defends against it in the fast suite, so a rename is caught by `cargo test` rather
        // than only by the full sequence.
        let root = super::workspace_root();
        let source = std::fs::read_to_string(root.join("crates/renvor-cli/src/commands/routes.rs"))
            .expect("the routes command source is readable");

        let function = super::RELAY_TEST
            .rsplit("::")
            .next()
            .expect("the test path names a function");

        assert!(
            source.contains(&format!("fn {function}(")),
            "the gate runs `--exact {}`, but no `fn {function}` exists in \
             `crates/renvor-cli/src/commands/routes.rs`. The filter would match nothing and the \
             gate would pass without running the end-to-end proof",
            super::RELAY_TEST
        );

        assert!(
            source.contains("#[ignore"),
            "the relay test is no longer `#[ignore]`d. If that is deliberate the gate must stop \
             passing `--ignored`, because `--ignored` runs ONLY ignored tests and would then match \
             nothing"
        );
    }

    #[test]
    fn a_comment_discussing_publish_false_does_not_exempt_a_package() {
        // REGRESSION, 2026-08-23. `renvor-core`, `renvor-http` and `renvor-testkit` each carry a
        // leading comment EXPLAINING why they are not `publish = false`. Both scans tested
        // `text.contains("publish = false")` against the raw file, so all three were read as
        // unpublishable and skipped. Three of five publishable packages went unexamined while both
        // gates reported success — a comment was switching off a gate.
        let discussed = "# A `publish = false` crate cannot be depended on, so this one is not.\n\
                         [package]\n\
                         name = \"renvor-example\"\n";
        assert!(
            super::is_publishable(discussed),
            "a package that merely MENTIONS `publish = false` in a comment was treated as exempt"
        );

        // CONTROL: the real key still exempts.
        let marked = "[package]\nname = \"xtask\"\npublish = false\n";
        assert!(
            !super::is_publishable(marked),
            "an actually-unpublishable package was treated as publishable"
        );

        // And a trailing comment on the real key does not hide it.
        let trailing = "[package]\nname = \"xtask\"\npublish = false # internal only\n";
        assert!(
            !super::is_publishable(trailing),
            "a trailing comment hid the key"
        );
    }

    #[test]
    fn a_metadata_scan_that_reads_nothing_is_a_failure_rather_than_a_pass() {
        super::required_metadata_is_declared(&[], &workspace_root_manifest())
            .expect_err("an empty scan proves nothing");
    }

    #[test]
    fn a_scan_that_reads_nothing_is_a_failure_rather_than_a_pass() {
        // The control that matters most: an empty or unreadable workspace must not report
        // compliance. Every other assertion in this module rests on the scan having read something.
        let reason = scan_manifests(&[]).expect_err("an empty scan proves nothing");
        assert!(reason.contains("not reading the workspace"), "{reason}");

        let no_dependencies = (
            "renvor".to_owned(),
            "[package]\nname = \"renvor\"\n".to_owned(),
        );
        scan_manifests(&[no_dependencies, second()])
            .expect_err("a workspace with no path dependency at all fails the control");
    }
}
