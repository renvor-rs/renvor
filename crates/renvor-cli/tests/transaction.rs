//! Contract C-5, the transaction: what survives a failure at each step.
//!
//! # The one promise this file exists to check
//!
//! > *A failure anywhere before placement leaves the destination exactly as it was and leaves no
//! > staging behind.*
//!
//! Every other property of `renvor new` is visible in its success path. This one is only visible
//! when something goes wrong, which is why the failures below are **injected deliberately** rather
//! than waited for. `RENVOR_FAIL_AT` names a step of C-5 and that step fails.
//!
//! The injector's body is `cfg(debug_assertions)`, so a release binary never reads the variable —
//! the machinery is **absent** rather than present and switched off. That is a structural claim
//! about the build, not something these tests measure: every binary they run is a debug build, so
//! none of them can observe a release one. It is stated as an unverified property, not asserted
//! as a tested one.
//!
//! # Ordering, stated rather than implied
//!
//! T015–T019 were specified as a **failing-first** harness — written against code that did not
//! exist, so that the transactional guarantee was not tested by somebody who already believed it
//! held. **That ordering did not happen.** This file was written after the implementation, and no
//! rerun can undo that. What it can do, and does, is carry a **negative control for every
//! positive claim** — the injector proves it can make each step fail, `an_uninjected_run_into_the
//! _same_fixtures_succeeds` proves the harness is not simply refusing everything, and the prompt
//! census proves a newly added prompt cannot slip past the cancellation coverage. See the
//! "Ordering requirements that were missed" section of [Phase 003 tasks](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/tasks.md).

mod harness;

use std::path::Path;
use std::process::{Command, Stdio};

use harness::{Terminal, renvor};

/// Every prompt the wizard asks, in order, as the operator sees it.
///
/// This list is **asserted against a real transcript** by `the_wizard_asks_exactly_these_prompts`,
/// so it cannot silently fall out of date — which is what makes the parameterised cancellation
/// coverage below actually cover every prompt rather than every prompt somebody remembered.
const PROMPTS: [&str; 8] = [
    "Project name",
    "Local development domain",
    "Generate the example domain module?",
    "Generate seed data for it?",
    "Generate database persistence?",
    "Generate container development controls?",
    "Record that local HTTPS is wanted? (nothing is issued and no trust store is touched)",
    "Create this project?",
];

/// Answers prompts `0..stop` with their defaults, then returns the live terminal at prompt `stop`.
fn drive_to(terminal: &mut Terminal, stop: usize) {
    for prompt in PROMPTS.iter().take(stop) {
        terminal.expect(prompt);
        terminal.enter();
    }
    terminal.expect(PROMPTS[stop]);
}

/// The names in `directory` that look like renvor staging.
fn staging_residue(directory: &Path) -> Vec<String> {
    std::fs::read_dir(directory)
        .expect("the directory is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".renvor-staging-"))
        .collect()
}

// ── T015: cancelling at each prompt ─────────────────────────────────────────────────────────

#[test]
fn the_wizard_asks_exactly_these_prompts() {
    // The guard that makes `cancelling_at_each_prompt_…` honest. T015 requires that "adding a
    // prompt without covering it fails the suite" — this is that failure.
    //
    // ── ASSERTED BEHAVIOURALLY, NOT BY PARSING THE TRANSCRIPT ──────────────────────────
    //
    // An earlier version extracted the questions by parsing lines out of the transcript and
    // comparing the list. That worked on Unix and failed on Windows, because **ConPTY mirrors the
    // screen rather than the bytes written** and merges lines unpredictably — the review screen's
    // last file-list entry arrived joined to the line after it. Questions also contain `? `
    // internally ("Generate seed data for it? (y/N)"), so no delimiter reliably bounds them.
    //
    // The transcript's *layout* is the terminal emulator's business. What this test actually needs
    // is two properties, and both are behavioural:
    //
    //   1. **Order and presence** — each `expect` below blocks until its own question appears, in
    //      order, so a renamed, removed, or reordered prompt fails at the prompt that went missing
    //      and names it.
    //   2. **Nothing extra** — after answering exactly `PROMPTS.len()` questions the run must reach
    //      completion. A prompt added to `prompts::fill` and not to `PROMPTS` leaves the process
    //      waiting for an answer that never comes, so `expect("created")` fails with a diagnosis
    //      showing the child still running and the unanswered question on screen.
    let root = tempfile::tempdir().expect("a temporary directory");
    let destination = root.path().join("demo");
    let mut terminal = Terminal::spawn(
        &["new", "--path", destination.to_str().expect("utf-8")],
        root.path(),
        &[],
    );
    for prompt in PROMPTS {
        terminal.expect(prompt);
        terminal.enter();
    }

    // Property 2. Nothing else was asked.
    // `DONE  Created N files` since 2026-08-21. The label is a WORD rather than a colour,
    // which is what makes this assertion possible at all under `NO_COLOR` and `TERM=dumb`.
    terminal.expect("DONE");
    terminal.expect("Created ");
    let exit = terminal.wait();
    assert_eq!(exit, 0, "the census run succeeds\n{}", terminal.visible());
    assert!(
        destination.join("renvor.toml").is_file(),
        "answering every prompt produced a project"
    );
}

#[test]
fn cancelling_at_each_prompt_exits_four_and_creates_no_destination() {
    // T015 and FR-011. Parameterised over `PROMPTS`, so this grows with the wizard.
    //
    // The last prompt is the review screen, which is reached only **after** rendering and the full
    // pre-placement verification. Cancelling there is the expensive case and the important one:
    // the tree exists, in staging, and declining must still leave the destination absent.
    for (index, prompt) in PROMPTS.iter().enumerate() {
        let root = tempfile::tempdir().expect("a temporary directory");
        let destination = root.path().join("demo");
        let mut terminal = Terminal::spawn(
            &["new", "--path", destination.to_str().expect("utf-8")],
            root.path(),
            &[],
        );
        drive_to(&mut terminal, index);
        terminal.escape();
        let exit = terminal.wait();

        assert_eq!(
            exit,
            4,
            "cancelling at {prompt:?} must exit 4\n--- transcript ---\n{}",
            terminal.visible()
        );
        assert!(
            !destination.exists(),
            "cancelling at {prompt:?} left a destination behind"
        );
        assert!(
            staging_residue(root.path()).is_empty(),
            "cancelling at {prompt:?} left staging behind: {:?}",
            staging_residue(root.path())
        );
    }
}

#[test]
fn declining_the_review_screen_still_prints_the_equivalent_command() {
    // US1 acceptance scenario 3, and the reason the equivalent command is printed *before* the
    // question rather than on the success path: declining must not lose the answers.
    let root = tempfile::tempdir().expect("a temporary directory");
    let destination = root.path().join("demo");
    let mut terminal = Terminal::spawn(
        &["new", "--path", destination.to_str().expect("utf-8")],
        root.path(),
        &[],
    );
    drive_to(&mut terminal, PROMPTS.len() - 1);
    terminal.key("n");
    let exit = terminal.wait();

    assert_eq!(
        exit,
        4,
        "declining is a cancellation\n{}",
        terminal.visible()
    );
    assert!(!destination.exists(), "declining created a destination");
    let visible = terminal.visible();
    assert!(
        visible.contains("equivalent command: renvor new"),
        "the answers must survive the refusal\n{visible}"
    );
    assert!(
        visible.contains("--example-domain"),
        "the equivalent command must carry the answers actually given\n{visible}"
    );
}

// ── T016: failing at each mutating step of contract C-5 ─────────────────────────────────────

/// The mutating steps of contract C-5, in protocol order.
///
/// C-5 defines **seven** steps. `validate` and `report` are excluded deliberately, and the reason
/// is stated here rather than left to inference: `validate` writes nothing, so it has no
/// post-condition to violate, and `report` runs after placement has already succeeded — there is
/// no "leaves the destination as it was" for a step that runs once it is not as it was.
const MUTATING_STEPS: [&str; 5] = ["stage", "render", "verify", "manifest", "place"];

/// Runs a flag-driven generation into `base`, optionally injecting a failure.
fn generate_into(base: &Path, inject: Option<&str>) -> (i32, String, String) {
    let environment: Vec<(&str, &str)> = inject
        .map(|step| vec![("RENVOR_FAIL_AT", step)])
        .unwrap_or_default();
    generate_into_with(base, &environment)
}

/// The same run, with the environment given explicitly.
///
/// Needed because the staging double-failure injects **two** steps at once
/// (`staging-open,staging-cleanup`), which `generate_into`'s `Option<&str>` cannot express.
fn generate_into_with(base: &Path, environment: &[(&str, &str)]) -> (i32, String, String) {
    renvor(
        &[
            "new",
            "demo",
            "--yes",
            "--example-domain",
            "--output",
            "json",
        ],
        base,
        environment,
    )
}

#[test]
fn a_failure_at_any_mutating_step_leaves_an_absent_destination_absent() {
    // T016, first half. The destination does not exist, so the promise is that it still does not.
    for step in MUTATING_STEPS {
        let base = tempfile::tempdir().expect("a temporary directory");
        let destination = base.path().join("demo");
        let (exit, stdout, stderr) = generate_into(base.path(), Some(step));

        assert_ne!(exit, 0, "injecting at `{step}` must fail the run: {stderr}");
        let document: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
            panic!("failing at `{step}` wrote no JSON document: {error}\n{stdout}")
        });
        assert_eq!(document["status"], "failure", "at `{step}`");
        assert_eq!(
            document["error"]["details"]["injected"], step,
            "the failure that occurred must be the one that was injected, not an unrelated one"
        );

        assert!(
            !destination.exists(),
            "failing at `{step}` created the destination"
        );
        assert!(
            staging_residue(base.path()).is_empty(),
            "failing at `{step}` left staging behind: {:?}",
            staging_residue(base.path())
        );
    }
}

#[test]
fn a_pre_existing_empty_destination_is_refused_before_any_step_can_be_injected() {
    // T016, second half. **Its premise changed on 2026-08-18 and so did its claim.**
    //
    // It used to read: an existing EMPTY destination is a legal target, so a failure injected at
    // each mutating step must leave it present and empty. Under the maintainer's destination
    // ruling the fixture is refused at validation, so no injected step ever runs — and the old
    // assertions would all still have PASSED, silently, while testing nothing. A gate that keeps
    // passing after the thing it tests becomes unreachable is the exact defect this phase's
    // advisory reviews found six times, so it is rewritten rather than left green.
    //
    // What it asserts now is stronger and is actually reachable: **for every injection point**,
    // the refusal happens before that point, the operator's directory is untouched, and no staging
    // was created. `details.injected` being absent is the assertion that makes this true rather
    // than assumed — its presence would prove the run got as far as the injected step.
    for step in MUTATING_STEPS {
        let base = tempfile::tempdir().expect("a temporary directory");
        let destination = base.path().join("demo");
        std::fs::create_dir(&destination).expect("the empty destination is created");

        let (exit, stdout, stderr) = generate_into(base.path(), Some(step));
        assert_eq!(
            exit, 3,
            "an existing empty destination must be refused, whatever is injected: {stderr}"
        );
        let document: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|error| panic!("no JSON document at `{step}`: {error}\n{stdout}"));
        assert_eq!(
            document["error"]["code"], "destination_exists",
            "at `{step}`"
        );
        assert!(
            document["error"]["details"]["injected"].is_null(),
            "the run reached the injected step `{step}`, so the refusal is NOT happening before \
             it: {document}"
        );

        assert!(
            destination.is_dir(),
            "with `{step}` injected, a pre-existing empty destination was removed"
        );
        assert_eq!(
            std::fs::read_dir(&destination).expect("readable").count(),
            0,
            "with `{step}` injected, something was written into the destination"
        );
        assert!(
            staging_residue(base.path()).is_empty(),
            "with `{step}` injected, staging was created despite the refusal: {:?}",
            staging_residue(base.path())
        );
    }
}

// ── T017: the positive control ──────────────────────────────────────────────────────────────

#[test]
fn an_uninjected_run_into_the_same_fixtures_succeeds() {
    // T017, and it is not a formality. Without it, a harness that refused **everything** — a typo
    // in the argument list, a broken fixture, a `renvor` that cannot start at all — satisfies both
    // T016 tests completely, because both assert only that the run fails and writes nothing.
    //
    // ── THE SECOND CASE MOVED OUT ON 2026-08-18 ───────────────────────────────────────
    //
    // This used to loop over `[false, true]`, the second arm creating an **empty** destination
    // first and expecting the run to succeed. The maintainer ruling made every existing
    // destination a refusal, so that arm is now a refusal test and lives in
    // `an_existing_empty_destination_is_refused_before_anything_is_staged` below. It was moved
    // rather than deleted: the case still matters, its expected answer changed.
    let base = tempfile::tempdir().expect("a temporary directory");
    let destination = base.path().join("demo");

    let (exit, stdout, stderr) = generate_into(base.path(), None);
    assert_eq!(
        exit, 0,
        "the same fixture must succeed when nothing is injected: {stderr}"
    );
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["status"], "success");
    assert!(
        destination.join("renvor.toml").is_file(),
        "a project was produced"
    );
    assert!(
        destination.join("src/domain.rs").is_file(),
        "the project is complete"
    );
    assert!(
        staging_residue(base.path()).is_empty(),
        "a successful run left staging behind"
    );
}

#[test]
fn an_existing_empty_destination_is_refused_before_anything_is_staged() {
    // THE END-TO-END TEST OF THE 2026-08-18 DESTINATION POLICY.
    //
    // Its predecessor asserted the opposite outcome for exactly this fixture — that an existing
    // empty destination was generated INTO — so this fails against the previous implementation on
    // its first assertion, and it fails for the right reason rather than by construction.
    //
    // Three separate things are checked, and the third is the one the ruling turns on:
    //
    //   1. the run is refused, with `destination_exists` and `details.rule = destination_absent`;
    //   2. the operator's directory is still there, still empty, and — on Unix — still the SAME
    //      inode with the same mode and ownership, which is what "not replaced" means;
    //   3. **no staging directory was ever created**. The refusal happens in `Destination::open`,
    //      before `Staging::create`, so a failed run does not even briefly put a `.renvor-staging-`
    //      directory in the operator's parent. Residue here would mean the check moved.
    let base = tempfile::tempdir().expect("a temporary directory");
    let destination = base.path().join("demo");
    std::fs::create_dir(&destination).expect("the empty destination is created");
    let before = std::fs::metadata(&destination).expect("metadata");

    let (exit, stdout, stderr) = generate_into(base.path(), None);
    assert_eq!(
        exit, 3,
        "an existing empty destination must be refused, not generated into: {stdout}{stderr}"
    );
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["status"], "failure");
    assert_eq!(
        document["error"]["code"], "destination_exists",
        "{document}"
    );
    assert_eq!(
        document["error"]["details"]["rule"], "destination_absent",
        "{document}"
    );
    assert_eq!(
        document["error"]["details"]["found"], "directory",
        "{document}"
    );

    let after = std::fs::metadata(&destination).expect("the operator's directory was removed");
    assert!(after.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        assert_eq!(
            (before.ino(), before.mode(), before.uid(), before.gid()),
            (after.ino(), after.mode(), after.uid(), after.gid()),
            "the operator's directory was deleted and recreated: a different inode, mode, or \
             owner is exactly the replacement finding A-R8 recorded"
        );
    }
    #[cfg(not(unix))]
    let _ = before;

    let contents: Vec<_> = std::fs::read_dir(&destination)
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(contents.is_empty(), "the refusal wrote {contents:?}");
    assert!(
        staging_residue(base.path()).is_empty(),
        "a destination refused before staging must not have created staging"
    );
}

#[test]
fn an_unrecognised_injection_point_is_not_a_failure() {
    // The control on the injector itself. `fail_at` compares the requested step to its own name;
    // an implementation that fired whenever the variable was *set* would make every test above
    // pass for the wrong reason, and would also make the mechanism unusable for finding which
    // step actually broke.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, _, stderr) = generate_into(base.path(), Some("not-a-step-of-contract-c-5"));
    assert_eq!(
        exit, 0,
        "an unknown injection point must be inert: {stderr}"
    );
    assert!(base.path().join("demo/renvor.toml").is_file());
}

// ── T018: concurrency ───────────────────────────────────────────────────────────────────────

#[test]
fn concurrent_runs_at_one_destination_produce_one_project_and_no_corruption() {
    // FR-015: "Concurrent runs targeting the same destination MUST NOT interleave into a corrupt
    // state." Asserted by actually racing them, because the property is about timing and no
    // amount of reading the code establishes it.
    //
    // The design's answer: each run stages under a name carrying its own process id, and
    // placement is a rename onto a path that must not exist. So the losers fail cleanly rather
    // than merging into the winner's tree.
    //
    // ── THIS TEST FOUND A REAL DEFECT THAT LOCAL RUNS COULD NOT ────────────────────────
    //
    // Five clean local runs, then **macOS and Windows CI both failed**: a loser reported
    // `placement_failed` rather than the destination-exists code — the window between the pre-rename
    // check and the rename, narrow enough that a fast idle machine never lands in it and wide
    // enough that a loaded runner does.
    //
    // ── WHY THIS RUNS THREE PROCESSES AND NOT TWELVE ───────────────────────────────────
    //
    // **Every one of these processes runs a full `cargo build`** for pre-placement verification.
    // Racing twelve of them stresses cargo far more than it stresses the rename — it is a heavy
    // test wearing a precise test's name, and heaviness is not rigour.
    //
    // The contention is carried by `generate::place::tests::racing_placements_…`, which races
    // `place` directly: sixteen threads, no subprocess, no build, milliseconds, and it hits the
    // window orders of magnitude more often. **That is the test that reproduces the race.**
    //
    // What this one uniquely proves is the *wiring*: that a real process which loses reports a
    // parseable JSON failure with a stable code, that the survivor is a whole project, and that no
    // staging is left in the operator's directory. Three processes establish all of that.
    //
    // Reduced from twelve on that reasoning, **not** because it was the failing test — it was not;
    // the macOS failure was the thread race, for an unrelated staging-name collision.
    let base = tempfile::tempdir().expect("tempdir");
    let destination = base.path().join("contended");

    let mut children = Vec::new();
    for _ in 0..3 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_renvor"))
                .args([
                    "new",
                    "contended",
                    "--yes",
                    "--example-domain",
                    "--output",
                    "json",
                ])
                .current_dir(base.path())
                // These are flag-path runs even when `cargo test` itself inherited a terminal.
                // `--yes` waives confirmation only; it deliberately does not suppress prompts.
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("spawns"),
        );
    }

    let mut succeeded = 0;
    let mut failed_cleanly = 0;
    for child in children {
        let output = child.wait_with_output().expect("waits");
        match output.status.code() {
            Some(0) => succeeded += 1,
            Some(3) => {
                // A loser must still honour C-2: exactly one parseable document naming a stable
                // code. "It failed somehow" is not an acceptable answer to a consumer.
                let document: serde_json::Value =
                    serde_json::from_slice(&output.stdout).expect("a loser wrote no document");
                assert_eq!(document["status"], "failure");
                assert_eq!(
                    document["error"]["code"], "destination_exists",
                    "a loser failed for an unexpected reason"
                );
                failed_cleanly += 1;
            }
            other => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                panic!(
                    "a concurrent run exited {other:?}, which is neither a win nor a clean loss\n\
                     --- stdout ---\n{}\n--- stderr ---\n{}",
                    stdout.trim(),
                    stderr.trim()
                )
            }
        }
    }

    assert_eq!(succeeded, 1, "exactly one run must win");
    assert_eq!(failed_cleanly, 2, "every other run must lose cleanly");

    // THE PROJECT MUST BE WHOLE, not a merge of six renders. Its own checks are the strongest
    // available statement of that, and they are what a corrupted tree would fail.
    let (code, _, stderr) = renvor(
        &["check", destination.to_str().expect("utf-8")],
        base.path(),
        &[],
    );
    assert_eq!(code, 0, "the surviving project is not valid:\n{stderr}");
    assert!(
        destination.join("src/domain.rs").is_file(),
        "the survivor is incomplete"
    );

    // AND NO RESIDUE. A staging directory left behind by a loser is not corruption, but it is
    // litter in the operator's working directory and the design promises it will not happen.
    let residue: Vec<String> = std::fs::read_dir(base.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".renvor-staging-"))
        .collect();
    assert!(
        residue.is_empty(),
        "concurrent losers left staging behind: {residue:?}"
    );
}
// ── T019: residue after a kill ──────────────────────────────────────────────────────────────

#[test]
fn a_killed_run_leaves_identifiable_residue_beside_the_destination_and_no_project() {
    // The residue promise, which until now rested entirely on a comment.
    //
    // `Staging`'s `Drop` cleans up on every ordinary failure path including a panic — but a
    // SIGKILL runs no destructor, and the design's answer to that is explicit: residue survives,
    // and it is made **identifiable** and placed **beside** the destination rather than inside it,
    // so it can never become part of a project.
    //
    // Killed during pre-placement verification, which takes seconds — a comfortable window,
    // and just as much "mid-transaction" as killing during the render itself.
    let base = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(["new", "doomed", "--yes"])
        .current_dir(base.path())
        // This is a flag-path run even when `cargo test` itself inherited a terminal. `--yes`
        // waives confirmation only; it deliberately does not suppress prompts.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawns");

    // Wait for staging to appear rather than sleeping a fixed time, so this is not a race against
    // a slow machine.
    let staging = loop {
        if let Some(name) = std::fs::read_dir(base.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| name.starts_with(".renvor-staging-"))
        {
            break name;
        }
        if child.try_wait().expect("try_wait").is_some() {
            panic!("the run finished before staging was observed; it is too fast to kill here");
        }
        std::thread::yield_now();
    };

    child.kill().expect("kill");
    child.wait().expect("wait");

    // THE THREE PROMISES.
    assert!(
        base.path().join(&staging).exists(),
        "the test observed staging and it vanished; nothing is being asserted"
    );
    assert!(
        !base.path().join("doomed").exists(),
        "a killed run left a destination behind — the transaction's central promise"
    );
    assert!(
        !base
            .path()
            .join(&staging)
            .starts_with(base.path().join("doomed")),
        "staging is inside the destination, so residue could become part of a project"
    );
    assert!(
        staging.starts_with(".renvor-staging-") && staging.contains(&child.id().to_string()),
        "residue must be identifiable as renvor's and name the process that left it: {staging}"
    );
}

// ── The staging double failure (advisory finding F/minor) ───────────────────────────────────

#[test]
fn a_staging_open_failure_that_cleans_up_says_nothing_was_written() {
    // The single failure: `open_dir` fails, the cleanup succeeds. The message may say nothing was
    // written, because nothing was — and the control for the test below, which asserts the
    // opposite message on the double failure. Without this pair, a version that always claimed
    // residue would pass the double-failure test and be wrong on the ordinary one.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, stdout, _) = generate_into_with(base.path(), &[("RENVOR_FAIL_AT", "staging-open")]);

    assert_eq!(
        exit, 3,
        "an unopenable staging directory is a clean refusal"
    );
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["error"]["code"], "staging_failed");
    assert!(
        document["error"]["details"]["residue"].is_null(),
        "no residue may be reported when the cleanup succeeded"
    );
    assert!(
        staging_residue(base.path()).is_empty(),
        "the cleanup did not actually run"
    );
}

#[test]
fn a_staging_cleanup_failure_reports_both_errors_and_the_exact_remaining_path() {
    // ── ADVISORY FINDING F/minor, 2026-08-18 ──────────────────────────────────────────
    //
    // The first fix for the orphaned-staging leak wrote `let _ = parent.remove_dir_all(&name)` and
    // kept the message "Nothing was written". The reviewer pointed out that if the condition which
    // broke `open_dir` also breaks the removal, the directory survives and that message is a false
    // statement about the filesystem — a narrower recurrence of the bug it was fixing.
    //
    // Reaching that state needs TWO failures at once, which no real filesystem will oblige with on
    // demand, so `inject::io_failure` takes a comma-separated list and this drives both. The
    // assertions are about what an operator can act on: the residue is really there, the error
    // NAMES it exactly, both underlying errors are reported, and the message does not claim
    // nothing was written.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, stdout, _) = generate_into_with(
        base.path(),
        &[("RENVOR_FAIL_AT", "staging-open,staging-cleanup")],
    );

    assert_eq!(exit, 3);
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    let error = &document["error"];
    assert_eq!(error["code"], "staging_failed");

    // Both underlying causes, not just the one that happened to be returned.
    assert!(
        error["details"]["openError"].is_string(),
        "the open failure must be reported: {error}"
    );
    assert!(
        error["details"]["cleanupError"].is_string(),
        "the cleanup failure must be reported too, or the operator cannot tell why residue \
         remains: {error}"
    );

    // The message must NOT claim nothing was written.
    let message = error["message"].as_str().expect("a message");
    assert!(
        !message.contains("Nothing was written;"),
        "the message claims nothing was written while a staging directory remains: {message}"
    );
    assert!(
        message.contains("still there"),
        "the message must say the directory remains: {message}"
    );

    // And the EXACT path, matched against what is actually on disk.
    let residue = staging_residue(base.path());
    assert_eq!(
        residue.len(),
        1,
        "the double failure must actually leave exactly one staging directory: {residue:?}"
    );
    let reported = error["details"]["residue"]
        .as_str()
        .expect("the residue path must be reported");
    assert!(
        reported.ends_with(&residue[0]),
        "the reported residue `{reported}` is not the directory actually left, `{}`",
        residue[0]
    );
    // Resolved against `base`, because the reported path is relative to the **operator's** working
    // directory — which is where they are standing when they read it, and therefore the form they
    // can paste. The test runs from somewhere else, so it has to rejoin.
    assert!(
        base.path().join(reported).is_dir(),
        "the reported residue path does not resolve to a directory that is really there: \
         {reported}"
    );
}

#[test]
fn a_failed_drop_cleanup_is_reported_and_never_claimed_as_a_removal() {
    // `Drop for Staging` discarded the removal `Result`, and every failure path that reaches it had
    // already built a message asserting *"the staged tree has been removed"* — a claim made before
    // the removal was attempted, and therefore false whenever it fails.
    //
    // On Windows this is the ordinary case, not the exotic one: an antivirus scanner, the search
    // indexer, or an open Explorer window holds a handle renvor does not own, and the removal fails
    // with `os error 32`. `dropping_without_placing_removes_the_staged_tree` covers only success.
    let base = tempfile::tempdir().expect("tempdir");
    let destination = base.path().join("demo");

    // A destination that appears mid-run is the failure this message belongs to.
    let (code, stdout, _stderr) = generate_into_with(
        base.path(),
        &[("RENVOR_FAIL_AT", "place,staging-drop-cleanup")],
    );

    let document: serde_json::Value = serde_json::from_slice(stdout.as_bytes())
        .unwrap_or_else(|error| panic!("stdout was not one JSON document: {error}\n{stdout}"));
    assert_eq!(code, 1, "an injected internal failure exits 1");

    let message = document["error"]["message"]
        .as_str()
        .expect("the failure carries a message");
    assert!(
        !message.contains("staged tree has been removed"),
        "the message claims a removal that failed:\n{message}"
    );

    let residue = document["error"]["details"]["residue"]
        .as_str()
        .unwrap_or_else(|| panic!("the surviving staged tree is not named:\n{document:#}"));
    // The child ran with `base` as its working directory, so the path it reports is relative to
    // that, not to the test process. Resolving it here is what makes the assertion about renvor's
    // report rather than about the harness.
    assert!(
        base.path().join(residue).exists(),
        "the named residue path does not exist, so the report is wrong in the other direction"
    );
    assert!(
        document["error"]["details"]["cleanupError"].is_string(),
        "the cleanup failure is not reported as structured detail:\n{document:#}"
    );

    // POSITIVE CONTROL: without the injection the same scenario must clean up silently and leave
    // nothing behind, or the assertions above would be satisfied by a permanently broken cleanup.
    let clean_base = tempfile::tempdir().expect("tempdir");
    let (clean_code, clean_stdout, _) =
        generate_into_with(clean_base.path(), &[("RENVOR_FAIL_AT", "place")]);
    assert_eq!(
        clean_code, 1,
        "the control must still fail for the same reason"
    );
    let clean: serde_json::Value = serde_json::from_slice(clean_stdout.as_bytes()).expect("json");
    assert!(
        clean["error"]["details"]["residue"].is_null(),
        "a successful cleanup must report no residue:\n{clean:#}"
    );
    let _ = destination;
}
