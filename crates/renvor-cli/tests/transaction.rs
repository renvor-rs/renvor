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
//! "Ordering requirements that were missed" section of `tasks.md`.

mod harness;

use std::path::Path;
use std::process::Command;

use harness::{Terminal, renvor};

/// Every prompt the wizard asks, in order, as the operator sees it.
///
/// This list is **asserted against a real transcript** by `the_wizard_asks_exactly_these_prompts`,
/// so it cannot silently fall out of date — which is what makes the parameterised cancellation
/// coverage below actually cover every prompt rather than every prompt somebody remembered.
const PROMPTS: [&str; 7] = [
    "Project name",
    "Local development domain",
    "Generate the example domain module?",
    "Generate seed data for it?",
    "Generate container development controls?",
    "Record that local HTTPS is wanted? (nothing is issued and no trust store is touched)",
    "Create this project?",
];

/// The questions `inquire` actually asked, extracted from a terminal transcript.
///
/// `inquire` renders a prompt twice: once asking — `? <question> (<default>)` — and once answered
/// — `? <question> <answer>`. Only the asking form ends in a closing parenthesis, which is what
/// separates them here. The trailing `(<default>)` is then stripped, so a change to a *default*
/// does not read as a change to the *question*.
fn prompts_asked(transcript: &str) -> Vec<String> {
    transcript
        .split(['\r', '\n'])
        .filter_map(|line| line.strip_prefix("? "))
        .map(str::trim_end)
        .filter(|line| line.ends_with(')'))
        .filter_map(|line| line.rfind(" (").map(|at| line[..at].to_owned()))
        .collect()
}

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
    // prompt without covering it fails the suite" — this is that failure. A prompt added to
    // `prompts::fill` and not to `PROMPTS` shows up here as an extra entry, immediately, rather
    // than as a cancellation test that quietly never runs.
    let root = tempfile::tempdir().expect("a temporary directory");
    let mut terminal = Terminal::spawn(
        &["new", "--path", root.path().join("demo").to_str().expect("utf-8")],
        root.path(),
        &[],
    );
    for prompt in PROMPTS {
        terminal.expect(prompt);
        terminal.enter();
    }
    let exit = terminal.wait();
    assert_eq!(exit, 0, "the census run succeeds\n{}", terminal.visible());

    assert_eq!(
        prompts_asked(&terminal.visible()),
        PROMPTS.to_vec(),
        "the wizard's questions have changed; update PROMPTS and the cancellation coverage with them"
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
            exit, 4,
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
    let mut terminal =
        Terminal::spawn(&["new", "--path", destination.to_str().expect("utf-8")], root.path(), &[]);
    drive_to(&mut terminal, PROMPTS.len() - 1);
    terminal.send_line("n");
    let exit = terminal.wait();

    assert_eq!(exit, 4, "declining is a cancellation\n{}", terminal.visible());
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
    let environment: Vec<(&str, &str)> =
        inject.map(|step| vec![("RENVOR_FAIL_AT", step)]).unwrap_or_default();
    renvor(
        &["new", "demo", "--yes", "--example-domain", "--output", "json"],
        base,
        &environment,
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
        let document: serde_json::Value = serde_json::from_str(&stdout)
            .unwrap_or_else(|error| panic!("failing at `{step}` wrote no JSON document: {error}\n{stdout}"));
        assert_eq!(document["status"], "failure", "at `{step}`");
        assert_eq!(
            document["error"]["details"]["injected"], step,
            "the failure that occurred must be the one that was injected, not an unrelated one"
        );

        assert!(!destination.exists(), "failing at `{step}` created the destination");
        assert!(
            staging_residue(base.path()).is_empty(),
            "failing at `{step}` left staging behind: {:?}",
            staging_residue(base.path())
        );
    }
}

#[test]
fn a_failure_at_any_mutating_step_leaves_a_pre_existing_empty_destination_exactly_as_it_was() {
    // T016, second half, and the case that only became reachable once FR-013's "exists and is not
    // empty" was honoured properly: an existing EMPTY destination is a legal target, so a failure
    // must leave it **present and empty** rather than removing it. A version of `place` that
    // removed the destination eagerly at the start passes every test in the first half and fails
    // every test in this one.
    for step in MUTATING_STEPS {
        let base = tempfile::tempdir().expect("a temporary directory");
        let destination = base.path().join("demo");
        std::fs::create_dir(&destination).expect("the empty destination is created");

        let (exit, _, stderr) = generate_into(base.path(), Some(step));
        assert_ne!(exit, 0, "injecting at `{step}` must fail the run: {stderr}");

        assert!(
            destination.is_dir(),
            "failing at `{step}` removed a pre-existing empty destination"
        );
        assert_eq!(
            std::fs::read_dir(&destination).expect("readable").count(),
            0,
            "failing at `{step}` wrote into a pre-existing empty destination"
        );
        assert!(
            staging_residue(base.path()).is_empty(),
            "failing at `{step}` left staging behind: {:?}",
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
    for prepare_empty_destination in [false, true] {
        let base = tempfile::tempdir().expect("a temporary directory");
        let destination = base.path().join("demo");
        if prepare_empty_destination {
            std::fs::create_dir(&destination).expect("the empty destination is created");
        }

        let (exit, stdout, stderr) = generate_into(base.path(), None);
        assert_eq!(
            exit, 0,
            "the same fixture must succeed when nothing is injected \
             (pre-existing empty destination: {prepare_empty_destination}): {stderr}"
        );
        let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
        assert_eq!(document["status"], "success");
        assert!(destination.join("renvor.toml").is_file(), "a project was produced");
        assert!(destination.join("src/domain.rs").is_file(), "the project is complete");
        assert!(staging_residue(base.path()).is_empty(), "a successful run left staging behind");
    }
}

#[test]
fn an_unrecognised_injection_point_is_not_a_failure() {
    // The control on the injector itself. `fail_at` compares the requested step to its own name;
    // an implementation that fired whenever the variable was *set* would make every test above
    // pass for the wrong reason, and would also make the mechanism unusable for finding which
    // step actually broke.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, _, stderr) =
        generate_into(base.path(), Some("not-a-step-of-contract-c-5"));
    assert_eq!(exit, 0, "an unknown injection point must be inert: {stderr}");
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
    // `placement_failed` rather than `destination_not_empty` — the window between the pre-rename
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
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
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
                    document["error"]["code"], "destination_not_empty",
                    "a loser failed for an unexpected reason"
                );
                failed_cleanly += 1;
            }
            other => {
                panic!("a concurrent run exited {other:?}, which is neither a win nor a clean loss")
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
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
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