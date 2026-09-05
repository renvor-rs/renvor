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
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    Run {
        succeeded: output.status.success(),
        status: describe(&output.status),
        output: combined,
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
    output: String,
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

    let read = |root: &Path| -> Vec<(String, Vec<u8>)> {
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
    };

    assert_eq!(
        read(&first),
        read(&second),
        "two identical runs produced different trees"
    );
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
        serde_json::from_str(dry_run.output.trim()).unwrap_or_else(|error| {
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
        serde_json::from_str(real_run.output.trim()).expect("one JSON document");

    assert_eq!(
        dry["result"]["manifest"], real["result"]["manifest"],
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
