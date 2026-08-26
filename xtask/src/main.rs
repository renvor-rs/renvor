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

/// Every step ran and passed.
const EXIT_OK: i32 = 0;
/// A step ran and failed.
const EXIT_STEP_FAILED: i32 = 1;
/// A required toolchain is missing; no steps ran.
const EXIT_TOOLING_MISSING: i32 = 2;
/// The working tree was dirty after an otherwise successful run (step 11).
const EXIT_DIRTY_TREE: i32 = 3;

/// Total number of steps in the sequence, used only for progress output.
const TOTAL_STEPS: usize = 11;

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

/// Everything step 1 probes for. Order matches the step that consumes each tool.
const REQUIRED: &[Tool] = &[
    Tool {
        program: "git",
        probe: &["--version"],
        name: "git",
        purpose: "secret scanning and working-tree cleanliness, steps 8 and 11",
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
    Tool {
        program: "node",
        probe: &["--version"],
        name: "node",
        purpose: "documentation site, step 9",
        install: "see .nvmrc for the required version",
    },
    Tool {
        program: "npm",
        probe: &["--version"],
        name: "npm",
        purpose: "documentation site, step 9",
        install: "ships with node — see .nvmrc",
    },
    Tool {
        program: "lychee",
        probe: &["--version"],
        name: "lychee",
        purpose: "link checking, step 10",
        install: "cargo install lychee --locked",
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
    eprintln!("  2  required tooling is missing; no steps ran");
    eprintln!("  3  the working tree was dirty after a successful run");
}

/// Runs the sequence and returns the process exit code.
fn verify() -> i32 {
    let root = workspace_root();

    // ---- Step 1: toolchain probe. Fail closed, before anything else runs. ----
    let missing = probe_tooling();
    if !missing.is_empty() {
        report_missing(&missing);
        return EXIT_TOOLING_MISSING;
    }
    step_ok(1, "toolchain probe", "all required tooling present");

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
    // at eleven steps.
    if !the_end_to_end_relay_ran(&root) {
        return EXIT_STEP_FAILED;
    }
    // STILL STEP 4. Four rows are specified by `PLAN.md` §10.1; nothing until now asserted that
    // four of them ran. See `the_four_rows_all_ran`.
    if !the_four_rows_all_ran(&root) {
        return EXIT_STEP_FAILED;
    }
    // Warnings are denied via RUSTDOCFLAGS; a broken intra-doc link is a failure.
    if !run(
        5,
        "API documentation",
        "cargo",
        &["doc", "--workspace", "--no-deps"],
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

    // ---- Step 9: documentation site ----
    let docs = root.join("docs");
    if !docs.join("package.json").is_file() {
        step_fail(
            9,
            "documentation site",
            "docs/package.json not found — the documentation package is missing",
        );
        eprintln!();
        eprintln!("This is a FAILURE, not a skip. The sequence has no conditional steps:");
        eprintln!("a check that cannot run is a failure (FR-023). Steps 1-8 above did run");
        eprintln!("and did pass; steps 9-11 did not run.");
        return EXIT_STEP_FAILED;
    }
    if !run(
        9,
        "documentation site (install)",
        "npm",
        &["ci"],
        &docs,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }
    if !run(
        9,
        "documentation site (build)",
        "npm",
        &["run", "build"],
        &docs,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }

    // ---- Step 10: link check over the BUILT output ----
    //
    // `--root-dir` is required, not optional: the built site uses root-relative links
    // (`/docs/intro`). Without it lychee cannot resolve a single one against the local
    // filesystem and reports every internal link as broken — 142 false failures that
    // would train a reader to ignore this step.
    //
    // Exclusions live in `lychee.toml`, each individually justified with a removal
    // condition, rather than as flags buried here where nobody reviews them.
    let link_root = docs.join("build");
    let link_root = link_root.to_string_lossy().to_string();
    if !run(
        10,
        "link check",
        "lychee",
        &[
            "--no-progress",
            "--require-https",
            "--config",
            "lychee.toml",
            "--root-dir",
            &link_root,
            "docs/build",
        ],
        &root,
        &[],
    ) {
        return EXIT_STEP_FAILED;
    }

    // ---- Step 11: working-tree cleanliness ----
    // This is what proves the ignore rules are correct rather than merely present.
    match dirty_entries(&root) {
        Err(message) => {
            step_fail(11, "working-tree cleanliness", &message);
            EXIT_STEP_FAILED
        }
        Ok(entries) if entries.is_empty() => {
            step_ok(
                11,
                "working-tree cleanliness",
                "no untracked or modified files",
            );
            println!();
            println!("verification passed: all {TOTAL_STEPS} steps ran and passed.");
            EXIT_OK
        }
        Ok(entries) => {
            step_fail(
                11,
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
fn report_missing(missing: &[&Tool]) {
    eprintln!("error: verification cannot run — required tooling is missing");
    eprintln!();
    for tool in missing {
        eprintln!("  missing: {} ({})", tool.name, tool.purpose);
        eprintln!("    install: {}", tool.install);
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

    match command.status() {
        Ok(status) if status.success() => {
            step_ok(number, title, "passed");
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
    if !lean_facade_compiles(root) {
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
        "crate DAG, transport and persistence isolation, facade isolation, lean compile, \
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
    let checks: [IsolationCheck<'_>; 5] = [
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
        (
            "renvor-sqlx (--all-features)",
            &["-p", "renvor-sqlx", "--all-features"],
            &["sea-orm", "renvor-seaorm"],
            &["sqlx", "renvor-database"],
        ),
        (
            "renvor-database (--all-features)",
            &["-p", "renvor-database", "--all-features"],
            &["sea-orm", "sqlx", "renvor-seaorm", "renvor-sqlx"],
            &["renvor-database"],
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
    std::process::Command::new("cargo")
        .args(args)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// T111: the facade **compiles** without default features, for every target.
///
/// [`facade_feature_isolation_holds`] asks `cargo tree` what the graph resolves to. That is not
/// the same question as whether the code builds, and the difference is not academic: the
/// `configuration` example used `renvor::config` with no `required-features` declaration, so
/// `--no-default-features --all-targets` failed to compile while every tree query stayed green.
/// Resolving a graph is not compiling against it.
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
/// `(crate, test binary, test path)`. Four rows of `PLAN.md` §10.1's backend matrix, each measured
/// by two suites: the shared **ports** contract and the shared **domain** example. Eight entries,
/// and a missing one fails the gate whichever half it belongs to.
const ROW_EVIDENCE: [(&str, &str, &str); 8] = [
    (
        "renvor-sqlx",
        "shared_contract",
        "postgres::the_shared_persistence_contract_holds",
    ),
    (
        "renvor-sqlx",
        "shared_contract",
        "mysql::the_shared_persistence_contract_holds",
    ),
    (
        "renvor-seaorm",
        "contract",
        "postgres::the_shared_persistence_contract_holds",
    ),
    (
        "renvor-seaorm",
        "contract",
        "mysql::the_shared_persistence_contract_holds",
    ),
    (
        "renvor-sqlx",
        "domain",
        "postgres::the_shared_domain_example_holds",
    ),
    (
        "renvor-sqlx",
        "domain",
        "mysql::the_shared_domain_example_holds",
    ),
    (
        "renvor-seaorm",
        "domain",
        "postgres::the_shared_domain_example_holds",
    ),
    (
        "renvor-seaorm",
        "domain",
        "mysql::the_shared_domain_example_holds",
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
/// **panics** when `RENVOR_TEST_REQUIRE_DATABASE` is set and a URL is absent. So the census is
/// mandatory exactly when that variable is set, and is reported as not-run when it is not — the
/// same contract the test harness itself applies, so a contributor without local databases still
/// gets a usable `cargo xtask verify`.
///
/// # It re-runs two test binaries
///
/// Deliberate, and it costs a couple of seconds. `run` streams its child's output to the operator
/// rather than capturing it, which is the right behaviour for a step whose output a human is
/// watching; capturing step 4 wholesale to satisfy this check would trade live test progress for a
/// census. Two small binaries run twice is the cheaper trade.
fn the_four_rows_all_ran(root: &Path) -> bool {
    const TITLE: &str = "tests (four-row persistence census)";

    if std::env::var_os("RENVOR_TEST_REQUIRE_DATABASE").is_none() {
        step_ok(
            4,
            TITLE,
            "NOT RUN — set RENVOR_TEST_REQUIRE_DATABASE with both database URLs to require it",
        );
        return true;
    }

    // Group the rows by the binary that carries them, so each binary runs once.
    let mut binaries: Vec<(&str, &str)> = ROW_EVIDENCE
        .iter()
        .map(|(package, binary, _)| (*package, *binary))
        .collect();
    // Sorted before `dedup`, which only removes ADJACENT duplicates. Unsorted, a binary named
    // again later in the table would be run twice.
    binaries.sort_unstable();
    binaries.dedup();

    for (package, binary) in binaries {
        let output = Command::new("cargo")
            .args([
                "test",
                "-p",
                package,
                "--features",
                "db-postgres,db-mysql",
                "--test",
                binary,
            ])
            .current_dir(root)
            .output();

        let Ok(output) = output else {
            step_fail(
                4,
                TITLE,
                &format!("`{package}/{binary}` could not be executed"),
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
                TITLE,
                &format!("`{package}/{binary}` failed:\n{combined}"),
            );
            return false;
        }

        for (expected_package, expected_binary, test) in ROW_EVIDENCE {
            if expected_package != package || expected_binary != binary {
                continue;
            }
            // The runner's own line for a test that RAN and passed. A row that was deleted,
            // renamed, or compiled out produces no such line.
            let evidence = format!("test {test} ... ok");
            if !combined.contains(&evidence) {
                step_fail(
                    4,
                    TITLE,
                    &format!(
                        "row `{package}::{test}` did not report in. Expected the line \
                         `{evidence}` in the runner's output. The row was removed, renamed, or \
                         feature-gated out — and without this census the workspace test run would \
                         simply have executed one row fewer and reported success"
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
            "all {} row-suite pairs reported in (4 rows x 2 shared suites)",
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
    /// path resolves as ignored. The end-to-end proof is step 11 itself, which runs
    /// `git status --porcelain` on every invocation. This test guards the prose; step 11
    /// guards the behaviour.
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
                "the tracked ignore file has no `{rule}` rule. Step 11 asserts the working \
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

    /// The published documentation site lists the same number of steps.
    ///
    /// The site's page duplicates the step table, and it has been left behind **three times**:
    /// the constitution version, the amendment count, and the step list were each corrected in
    /// `.md` sources while `docs/docs/*.mdx` kept the old values, because the sweeps that found
    /// them globbed `*.md`. A reader following the published site was told the command runs ten
    /// steps while it ran eleven, because the site's table omitted the architecture-invariants
    /// step exactly as the contract's did.
    ///
    /// Checking it here is the only thing that has actually stopped that recurring.
    #[test]
    fn the_documentation_site_lists_the_same_step_count() {
        const PAGE: &str = include_str!("../../docs/docs/verification.mdx");

        let mut numbers: Vec<usize> = Vec::new();
        for line in PAGE.lines() {
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

        // POSITIVE CONTROL: a parse that matched nothing would pass every assertion below.
        assert!(
            numbers.len() >= super::TOTAL_STEPS,
            "the site parse found only {} numbered rows; it is not reading the step table",
            numbers.len()
        );

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
            "docs/docs/verification.mdx publishes {steps} steps but the command runs {}. The \
             site is what a contributor reads before running anything.",
            super::TOTAL_STEPS
        );
    }

    /// Every required tool names the step that actually consumes it.
    ///
    /// `report_missing` prints `Tool::purpose` verbatim, so these strings are **observable
    /// output**, and `contracts/verification-sequence.md` publishes an example of that output.
    /// They drifted: after the architecture-invariants step was restored to the published table,
    /// `gitleaks` still said step 7, `node` and `npm` said 8, and `lychee` said 9 — one behind
    /// the sequence the command actually runs.
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
                "secret scanning and working-tree cleanliness, steps 8 and 11",
            ),
            ("rustfmt", "formatting, step 2"),
            ("clippy", "lint, step 3"),
            ("cargo-deny", "dependency and licence policy, step 6"),
            ("gitleaks", "secret scan, step 8"),
            ("node", "documentation site, step 9"),
            ("npm", "documentation site, step 9"),
            ("lychee", "link checking, step 10"),
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

        // ELEVEN since Phase 007: `renvor-seaorm` joined the ten Phase 006 left. The number is
        // asserted rather than inferred so that adding a publishable package without adding it to
        // the release ordering fails here — which is exactly what happened when this count was
        // five and three packages had just been added, again at Phase 006, and again here: this
        // assertion is what first reported the new crate, before any manual list was touched.
        assert_eq!(
            examined, 11,
            "the workspace publishes eleven packages; the scan examined {examined}"
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
            "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
            "Eleven", "Twelve", "Thirteen", "Fourteen", "Fifteen",
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
    }

    // ── published documentation agrees with the contracts it republishes ────────────────────

    /// The error-code values of a registry table: rows shaped `| `code` | <exit> | ... |`.
    ///
    /// The exit-code column is what separates the registry from the several OTHER tables in these
    /// documents whose first column is also a backticked identifier — `command`, `status`,
    /// `result`, `error` are JSON FIELD names, not error codes, and an earlier version of this
    /// parser collected them and reported four phantom omissions.
    fn codes_in(markdown: &str) -> std::collections::BTreeSet<String> {
        markdown
            .lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("| `")?;
                let (code, tail) = rest.split_once('`')?;
                let exit = tail.trim_start().strip_prefix('|')?.trim();
                let (exit, _) = exit.split_once('|')?;
                exit.trim().parse::<u8>().ok()?;
                code.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_')
                    .then(|| code.to_owned())
            })
            .collect()
    }

    #[test]
    fn the_published_cli_page_lists_every_error_code_the_contract_publishes() {
        // FR-036 / C-2. `docs/docs/cli.mdx` REPUBLISHES the closed error-code registry. On
        // 2026-08-22 it carried 19 of the 20 codes — `transport_not_wired`, added by this phase,
        // was missing — while `exit::tests::the_registry_matches_the_published_contract_exactly`
        // checked only the contract file and passed.
        //
        // `xtask` already guards exactly this drift class for `docs/docs/verification.mdx`. Nothing
        // guarded `cli.mdx`, which is how a published page went one code short.
        let root = super::workspace_root();
        let contract = std::fs::read_to_string(root.join("contracts/json-output.md"))
            .expect("the JSON output contract is readable");
        let page = std::fs::read_to_string(root.join("docs/docs/cli.mdx"))
            .expect("the published CLI page is readable");

        let published = codes_in(&contract);
        assert!(
            published.len() >= 15,
            "only {} code(s) parsed from the contract, so the comparison would prove nothing: {:?}",
            published.len(),
            published
        );

        let on_the_page = codes_in(&page);
        let missing: Vec<&String> = published.difference(&on_the_page).collect();

        assert!(
            missing.is_empty(),
            "the published CLI page omits {} code(s) the contract publishes: {missing:?}. A \
             reader of the site would not learn they exist",
            missing.len()
        );
    }

    #[test]
    fn the_published_cli_page_does_not_call_a_shipped_command_absent() {
        // FR-040 read for INTENT rather than only literally. The requirement names public
        // statements that a capability "becomes available in Phase 004"; `cli.mdx` carried the
        // CONVERSE falsehood — `routes` listed among commands "that later phases will add —
        // absent, not stubbed", under a heading reading "Everything below is implemented and
        // tested". Phase 004 shipped `routes`.
        let root = super::workspace_root();
        let page = std::fs::read_to_string(root.join("docs/docs/cli.mdx"))
            .expect("the published CLI page is readable");
        let surface = std::fs::read_to_string(root.join("contracts/command-surface.md"))
            .expect("the command-surface contract is readable");

        let later_phases = page
            .lines()
            .find(|line| line.contains("later phases will add"))
            .expect("the page names the commands later phases will add");

        for command in ["new", "doctor", "check", "dev", "docker", "tls", "routes"] {
            let shipped = surface.contains(&format!("`renvor {command}"));
            if !shipped {
                continue;
            }
            assert!(
                !later_phases.contains(&format!("`{command}`")),
                "`renvor {command}` has shipped — the command-surface contract documents it — but \
                 the published page still lists it among the commands later phases will add"
            );
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
