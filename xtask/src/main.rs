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
//! `specs/001-governance-foundation/contracts/verification-sequence.md`.

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
        purpose: "secret scan, step 7",
        install: "brew install gitleaks   (or see github.com/gitleaks/gitleaks)",
    },
    Tool {
        program: "node",
        probe: &["--version"],
        name: "node",
        purpose: "documentation site, step 8",
        install: "see .nvmrc for the required version",
    },
    Tool {
        program: "npm",
        probe: &["--version"],
        name: "npm",
        purpose: "documentation site, step 8",
        install: "ships with node — see .nvmrc",
    },
    Tool {
        program: "lychee",
        probe: &["--version"],
        name: "lychee",
        purpose: "link checking, step 9",
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
    // Three claims the plan makes that are otherwise only assertions in prose. Each is checked
    // against the RESOLVED graph or the actual document text, and each carries a positive control
    // — a query that must find what the first query must not, so a check that silently stopped
    // working is caught rather than reported as a pass.
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
        eprintln!("a check that cannot run is a failure (FR-023). Steps 1-7 above did run");
        eprintln!("and did pass; steps 8-10 did not run.");
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

/// Step 7: the crate DAG, the facade's feature isolation, and the SC-022 wording agreement.
///
/// Kept in one step because the three share a shape: **a claim, and a control that proves the
/// check can fail**. Splitting them would triple the progress output without adding information.
fn architecture_invariants(root: &std::path::Path) -> bool {
    if !crate_dag_holds(root) {
        return false;
    }
    if !facade_feature_isolation_holds(root) {
        return false;
    }
    if !instability_wording_agrees(root) {
        return false;
    }
    step_ok(
        7,
        "architecture invariants",
        "crate DAG, facade isolation, and instability wording all hold, each with a control",
    );
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

/// T104: the instability-closure sentence is byte-identical in all three normative locations, and
/// **0** phase numbers appear inside FR-036's normative closure clause.
fn instability_wording_agrees(root: &std::path::Path) -> bool {
    let spec = root.join("specs/002-core-kernel/spec.md");
    let Ok(text) = std::fs::read_to_string(&spec) else {
        step_fail(
            7,
            "architecture invariants",
            "specs/002-core-kernel/spec.md is unreadable",
        );
        return false;
    };

    // Located by its own text rather than by line number, so editing the document around it does
    // not silently disable this check.
    let occurrences = text.matches(SC022_SENTENCE).count();
    if occurrences < SC022_REQUIRED_OCCURRENCES {
        step_fail(
            7,
            "architecture invariants",
            &format!(
                "the instability-closure sentence appears {occurrences} time(s); SC-022 requires \
                 {SC022_REQUIRED_OCCURRENCES} byte-identical copies across the clarification \
                 record, FR-036, and the Dependencies section"
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

/// How many normative locations must carry it: the clarification record, FR-036, and Dependencies.
const SC022_REQUIRED_OCCURRENCES: usize = 3;
