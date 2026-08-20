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
    if !facade_feature_isolation_holds(root) {
        return false;
    }
    if !lean_facade_compiles(root) {
        return false;
    }
    if !publishable_dependencies_are_resolvable(root) {
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
        "crate DAG, facade isolation, lean compile, publishable dependencies, instability \
         wording, and the executable name all hold, each with a control",
    );
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
        // `publish = false` is what exempts a package from this rule.
        if text.contains("publish = false") {
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
