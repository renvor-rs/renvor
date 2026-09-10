//! The acceptance criterion that cannot be checked by inspection: **the generated skeleton
//! formats, compiles, tests, and starts.**
//!
//! # Why this shells out to cargo instead of asserting on strings
//!
//! A unit test can assert that `src/main.rs` contains the text somebody expected. It cannot tell
//! you that the project compiles, and the two failures this suite has already caught were both
//! invisible to string assertions: MiniJinja stripping every file's trailing newline, and a
//! conditional `mod` block leaving one blank line too many. Both produced *correct-looking* text
//! that `cargo fmt --check` rejected.
//!
//! # Isolation
//!
//! Each generated project builds into its own `CARGO_TARGET_DIR` inside the temporary directory, so
//! these runs never contend with the outer build's lock and leave nothing behind.

use std::path::Path;
use std::process::Command;

/// Every combination of the flags that change what is rendered.
///
/// Enumerated rather than sampled: the two defects found so far both appeared in **one** variant
/// and not the others, so a suite that checked only the fullest combination would have missed both.
const VARIANTS: [&[&str]; 5] = [
    &[],
    &["--example-domain"],
    &["--example-domain", "--seed-data"],
    &["--example-domain", "--seed-data", "--container"],
    &["--container"],
];

/// Runs a command in a directory with an isolated target directory, returning combined output.
///
/// # The exit status is part of the output, and that was learned the hard way
///
/// This used to return the two streams and drop the status. On 2026-08-21 a full verification run
/// failed here once, under heavy parallel load, with the message
/// `generation failed for ["--container"]:` and **nothing after the colon** — the child had
/// produced no output at all, which is the one case where the streams say nothing and the status
/// says everything. The failure did not reproduce in six subsequent runs of the same command.
///
/// A diagnostic that is empty on the one failure mode nobody can reproduce is worse than no
/// diagnostic, because it sends the next reader looking at the program instead of at the process.
/// The status is now reported: an exit code distinguishes a refusal from a crash, and a signal
/// distinguishes a crash from an out-of-memory kill.
fn run(program: &str, args: &[&str], directory: &Path, target: &Path) -> Run {
    let output = Command::new(program)
        .args(args)
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", target)
        .output()
        .unwrap_or_else(|error| panic!("`{program}` could not be run: {error}"));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    Run {
        succeeded: output.status.success(),
        status: describe(&output.status),
        output: format!("{stdout}{stderr}"),
        stdout,
        stderr,
    }
}

/// One command's result.
///
/// `status` is deliberately **not** part of `output`: two call sites parse `output` as a JSON
/// document, and prefixing it with a status line makes that fail — which is what happened on the
/// first attempt at this change, and is why the two are separate fields rather than one string.
struct Run {
    succeeded: bool,
    /// Never empty. See [`describe`].
    status: String,
    /// Both streams, for a failure message that should show everything the command said.
    output: String,
    /// `stdout` **alone**. C-1 reserves it for the result, so a test that parses one JSON
    /// document must read this and not the combination: a diagnostic on `stderr` is not a second
    /// document, and reading the two together turned the FR-012-8 resolution notice — which
    /// prints on every leg whose toolchain is not the pin — into "trailing characters". Found by
    /// `verify (stable)` and `platform (macos-latest, stable)` on 2026-09-08.
    stdout: String,
    /// `stderr` alone: every notice, warning, and progress line.
    stderr: String,
}

/// How a child process ended, in a form that is never empty.
fn describe(status: &std::process::ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit {code}");
    }
    // No code means a signal, which is the case an empty diagnostic hides completely: a process
    // killed for using too much memory looks exactly like a process that chose to fail.
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(signal) = status.signal() {
            return format!("killed by signal {signal}");
        }
    }
    format!("ended without an exit code: {status}")
}

/// Generates one variant and returns its directory.
fn generate(base: &Path, flags: &[&str]) -> std::path::PathBuf {
    let mut args = vec!["new", "demo", "--yes"];
    args.extend_from_slice(flags);
    let outcome = run(
        env!("CARGO_BIN_EXE_renvor"),
        &args,
        base,
        &base.join(".target"),
    );
    assert!(
        outcome.succeeded,
        "generation failed for {flags:?} [{}]:\n{}",
        outcome.status, outcome.output
    );
    base.join("demo")
}

#[test]
fn every_generated_variant_formats_compiles_and_tests() {
    for flags in VARIANTS {
        let base = tempfile::tempdir().expect("tempdir");
        let project = generate(base.path(), flags);
        let target = base.path().join(".build");

        let outcome = run("cargo", &["fmt", "--check"], &project, &target);
        assert!(
            outcome.succeeded,
            "`cargo fmt --check` failed for {flags:?} [{}]:\n{}",
            outcome.status, outcome.output
        );

        let outcome = run("cargo", &["test"], &project, &target);
        assert!(
            outcome.succeeded,
            "`cargo test` failed for {flags:?} [{}]:\n{}",
            outcome.status, outcome.output
        );
    }
}

#[test]
fn the_generated_binary_starts_and_names_itself() {
    // "starts" is part of the criterion and is the one thing compiling does not prove.
    let base = tempfile::tempdir().expect("tempdir");
    let project = generate(base.path(), &["--example-domain", "--seed-data"]);
    let target = base.path().join(".build");
    let outcome = run("cargo", &["run", "--quiet"], &project, &target);
    assert!(
        outcome.succeeded,
        "the generated binary did not run [{}]:\n{}",
        outcome.status, outcome.output
    );
    assert!(
        outcome.output.contains("demo is running"),
        "{}",
        outcome.output
    );
    assert!(
        outcome.output.contains("2 items"),
        "seed data did not reach the domain module:\n{}",
        outcome.output
    );
}

#[test]
fn generating_the_same_configuration_twice_produces_identical_trees() {
    // SC-016, end to end rather than at the manifest layer. Reproducibility that holds for the
    // manifest and not for the bytes on disk is not reproducibility.
    let base = tempfile::tempdir().expect("tempdir");
    let one = base.path().join("one");
    let two = base.path().join("two");
    std::fs::create_dir_all(&one).expect("mkdir");
    std::fs::create_dir_all(&two).expect("mkdir");
    let flags = ["--example-domain", "--seed-data", "--container"];
    let first = generate(&one, &flags);
    let second = generate(&two, &flags);

    // THE PROVENANCE RECORD IS COMPARED BY PATH, NOT BY BYTES (Phase 012, FR-012-4). Its
    // `[verified_with]` table is *measured*: the instant the checks passed, the digest of the
    // tree they passed on, and whether a compiler was launched or artifacts were reused. Two runs
    // a second apart legitimately differ there, and a record that did not differ would be one
    // filled in from something other than the run — which is the failure FR-012-4 exists to
    // forbid. `template-contract.md` §"Snapshot stability policy" states the same exclusion for
    // `tests/snapshots.rs`, and for the same reason.
    //
    // The path stays compared, so a run that stopped writing a record fails here; and every other
    // file in the tree is still compared byte for byte, which is what SC-016 is about.
    let read = |root: &Path| -> Vec<(String, Option<Vec<u8>>)> {
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
                    let measured = relative.replace('\\', "/") == ".renvor/generated.toml";
                    files.push((
                        relative,
                        (!measured).then(|| std::fs::read(&path).expect("read")),
                    ));
                }
            }
        }
        files.sort();
        files
    };

    let (first, second) = (read(&first), read(&second));
    assert!(
        first.iter().any(
            |(path, bytes)| path.replace('\\', "/") == ".renvor/generated.toml" && bytes.is_none()
        ),
        "the run wrote no provenance record, so its exclusion above is hiding a missing file"
    );
    assert_eq!(first, second, "two identical runs produced different trees");
}

#[test]
fn a_dry_run_writes_nothing_and_its_manifest_matches_the_real_run() {
    // SC-006, from outside the process, against the JSON contract rather than against internals.
    let base = tempfile::tempdir().expect("tempdir");
    let target = base.path().join(".target");

    let dry_run = run(
        env!("CARGO_BIN_EXE_renvor"),
        &[
            "new",
            "demo",
            "--yes",
            "--example-domain",
            "--dry-run",
            "--output",
            "json",
        ],
        base.path(),
        &target,
    );
    assert!(
        dry_run.succeeded,
        "the dry run failed [{}]:\n{}",
        dry_run.status, dry_run.output
    );
    assert!(
        !base.path().join("demo").exists(),
        "the dry run created the destination"
    );

    let dry: serde_json::Value =
        serde_json::from_str(dry_run.stdout.trim()).unwrap_or_else(|error| {
            panic!(
                "stdout was not one JSON document: {error}\n{}",
                dry_run.output
            )
        });
    assert_eq!(dry["status"], "success");
    assert_eq!(dry["result"]["dryRun"], true);

    let real_run = run(
        env!("CARGO_BIN_EXE_renvor"),
        &[
            "new",
            "demo",
            "--yes",
            "--example-domain",
            "--output",
            "json",
        ],
        base.path(),
        &target,
    );
    assert!(
        real_run.succeeded,
        "the real run failed [{}]:\n{}",
        real_run.status, real_run.output
    );
    let real: serde_json::Value =
        serde_json::from_str(real_run.stdout.trim()).expect("one JSON document");
    // C-1's stream discipline, now that a run has something to say: whatever the notices were,
    // they are on `stderr` and `stdout` carries the one document and nothing else.
    assert!(
        !real_run.stdout.trim_end().contains('\n') || real_run.stdout.trim_start().starts_with('{'),
        "stdout carries the JSON result and nothing else"
    );
    // Whichever notices this leg produced — the resolution notice prints only where the toolchain
    // is not the project's pin, so a leg running on the pin produces none — each is on `stderr`
    // and on no other stream.
    for notice in [
        "toolchain resolved before verification",
        "verification launched",
        "verification reused cached artifacts",
    ] {
        assert!(
            !real_run.stdout.contains(notice),
            "a diagnostic notice reached stdout, which C-1 reserves for the result"
        );
        assert_eq!(
            real_run.output.contains(notice),
            real_run.stderr.contains(notice),
            "a notice appeared somewhere other than stderr"
        );
    }

    // THE RECORD'S DIGEST AND SIZE ARE NOT COMPARED, for the reason
    // `generating_the_same_configuration_twice_produces_identical_trees` states above: since
    // Phase 012 the record carries the instant the checks passed and what they observed, so two
    // runs differ there by construction. Its PATH is still compared, and so is every other
    // entry's digest and size — which is what SC-006 asserts.
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
    let (dry_entries, real_entries) = (entries(&dry), entries(&real));
    assert!(
        dry_entries
            .iter()
            .any(|entry| entry["path"] == ".renvor/generated.toml"),
        "neither run listed a provenance record, so its exclusion above is hiding a missing file"
    );
    assert_eq!(
        dry_entries, real_entries,
        "the dry-run manifest does not match what the real run created"
    );
}

#[test]
fn a_reserved_flag_exits_three_with_a_parseable_error() {
    // C-1's reserved-flag rule and C-2's "exactly one document on failure too", together.
    let base = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        // `--frontend` rather than `--auth`: Phase 011 shipped the auth starter, so `--auth` is
        // honoured now — as `--database` has been since Phase 006 — and a test driving this
        // contract with either would be asserting the opposite of what the CLI does.
        .args(["new", "demo", "--frontend", "react", "--output", "json"])
        .current_dir(base.path())
        .output()
        .expect("runs");
    assert_eq!(output.status.code(), Some(3), "exit code");
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout carried exactly one JSON document");
    assert_eq!(document["status"], "failure");
    assert_eq!(document["error"]["code"], "reserved_for_later_phase");
    assert!(
        document["error"]["details"]["phase"].is_string(),
        "the error must name the phase that will support the flag"
    );
    assert!(
        !base.path().join("demo").exists(),
        "a refused flag still created something"
    );
}

#[test]
fn stdout_carries_only_the_result_so_a_pipeline_needs_no_filtering() {
    // C-1's stream discipline, asserted the way it is actually consumed. A single stray `println!`
    // anywhere in the success path breaks this and nothing else would notice.
    let base = tempfile::tempdir().expect("tempdir");
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(["new", "demo", "--yes", "--dry-run", "--output", "json"])
        .current_dir(base.path())
        .output()
        .expect("runs");
    assert!(output.status.success());
    serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout was not exactly one JSON document: {error}\n---\n{}\n---",
            String::from_utf8_lossy(&output.stdout)
        )
    });
}

#[test]
fn json_dev_puts_exactly_one_document_on_stdout_and_the_child_output_on_stderr() {
    // The sibling of `stdout_carries_only_the_result_so_a_pipeline_needs_no_filtering`, for the
    // case that test cannot reach. That one passes `--dry-run`, which returns before `cargo` is
    // ever spawned — so the whole child-process half of C-1's stream discipline was untested, and
    // `--output json dev` emitted libtest's output ahead of the envelope on every real run.
    //
    // `dev` runs the generated project's own `cargo test`, so this is slow by construction. It is
    // in `generated.rs` rather than `cli.rs` because everything here already pays that cost.
    let base = tempfile::tempdir().expect("tempdir");
    let project = generate(base.path(), &["--example-domain"]);

    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(["--output", "json", "dev"])
        .current_dir(&project)
        .env("CARGO_TARGET_DIR", base.path().join(".target"))
        .output()
        .expect("runs");

    assert!(
        output.status.success(),
        "dev failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "stdout was not exactly one JSON document: {error}\n---\n{}\n---",
                String::from_utf8_lossy(&output.stdout)
            )
        });
    assert_eq!(document["command"], "dev");
    assert_eq!(document["status"], "success");

    // The child's output must not be discarded — it is the useful half of `dev`. It moves to
    // stderr, which C-1 reserves for diagnostics, rather than being thrown away.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("test result: ok"),
        "`cargo test`'s output must be redirected to stderr, not discarded; stderr was:\n{stderr}"
    );
}

/// FR-012-7d (j): what the generator's own cache paths do against a **shared** build directory.
///
/// The maintainer's U-2 amendment carried this into implementation rather than another
/// feasibility round. A check whose units Cargo positively reports `Fresh` is a supported outcome
/// that the record represents explicitly; the question this answers is whether `renvor new`'s
/// staging ever *reaches* it, on this platform, when an absolute `CARGO_TARGET_DIR` already holds
/// the artifacts of an identical tree.
///
/// It asserts an invariant rather than an outcome — the outcome is the measurement, and asserting
/// one would be asserting what the platform happens to do. What must hold either way is that the
/// record and the observation agree: a launch observed means an identity was queried, and no
/// launch observed means the identity is absent rather than filled in from somewhere.
///
/// The observation is printed with a fixed prefix so that each platform leg's log carries the
/// answer for `governance/phase-012-evidence.md` §1.2.
#[test]
fn a_second_generation_against_a_shared_build_directory_records_its_observation() {
    let base = tempfile::tempdir().expect("tempdir");
    let shared = base.path().join(".shared-target");
    std::fs::create_dir_all(&shared).expect("mkdir");

    let mut observations = Vec::new();
    for pass in 0..2u8 {
        let workspace = base.path().join(format!("pass-{pass}"));
        std::fs::create_dir_all(&workspace).expect("mkdir");
        let outcome = run(
            env!("CARGO_BIN_EXE_renvor"),
            &["new", "demo", "--yes"],
            &workspace,
            &shared,
        );
        assert!(
            outcome.succeeded,
            "generation failed against a shared build directory [{}]:\n{}",
            outcome.status, outcome.output
        );
        let record = std::fs::read_to_string(workspace.join("demo/.renvor/generated.toml"))
            .expect("the record is readable");
        let value = |key: &str| {
            record
                .lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix(" = "))
                .map(|text| text.trim_matches('"').to_owned())
        };
        let observation = value("observation").expect("the record records an observation");
        // The invariant, whichever way the platform went: an observation and an identity agree.
        match observation.as_str() {
            "launched" | "mixed" => assert!(
                value("rustc_release").is_some(),
                "a launch was observed but no compiler identity was queried"
            ),
            "cached" => assert!(
                value("rustc_release").is_none(),
                "no launch was observed, yet an observed identity was recorded"
            ),
            _ => panic!("the record's observation is not one of the three named states"),
        }
        observations.push(observation);
    }
    // The measurement itself, for `phase-012-evidence.md` §1.2. `libtest` hides this for a passing
    // test unless the run captures stdout, which is why the prefix is greppable in a CI log.
    println!(
        "MEASUREMENT renvor-new-shared-target: first={} second={}",
        observations[0], observations[1]
    );
}
