//! Contract C-8 on the paths that need a real terminal.
//!
//! # Everything here runs the shipped binary through a pseudo-terminal
//!
//! Styling, prompt behaviour, cursor restoration, and the width-dependent half of the layout are
//! all decisions the program makes *because* it is attached to a terminal. A test that pipes the
//! streams exercises none of them — it exercises the branch that refuses, which
//! `tests/presentation.rs` covers at length. Driving the real binary through a pty is the only way
//! the code under test is the code that ships.
//!
//! `portable-pty` wraps ConPTY on Windows and `openpty` elsewhere, so every assertion in this file
//! runs on every leg of the platform matrix rather than quietly vanishing on the one platform with
//! the most surprising terminal.
//!
//! # These tests set their own environment, and that is the point
//!
//! [`harness::Terminal::spawn`] pins `NO_COLOR=1` and `TERM=dumb` so that the *other* suites read
//! plain text. Contract C-8 says those two suppress colour, so a test that wants to see colour has
//! to undo them — and undoing them is itself an assertion that they were doing something.

mod harness;

use harness::Terminal;

/// A terminal that is allowed to be in colour.
///
/// `NO_COLOR=""` rather than removing the variable: the harness sets it and this must **override**
/// it, which is also a direct test of C-8's empty-string rule — an empty `NO_COLOR` is not a
/// request for no colour.
const IN_COLOUR: &[(&str, &str)] = &[("NO_COLOR", ""), ("TERM", "xterm-256color")];

/// The SGR sequences the palette actually emits, so a failure names which colour is missing.
const CYAN: &str = "\u{1b}[36m";
const GREEN: &str = "\u{1b}[32m";
const BLUE: &str = "\u{1b}[34m";
const SHOW_CURSOR: &str = "\u{1b}[?25h";

fn workspace() -> tempfile::TempDir {
    tempfile::tempdir().expect("a temporary directory")
}

/// Runs `renvor doctor` on a terminal of `columns` columns and returns the raw transcript.
fn doctor(columns: u16, environment: &[(&str, &str)]) -> String {
    let root = workspace();
    let mut terminal = Terminal::spawn_sized(&["doctor"], root.path(), environment, columns);
    assert_eq!(terminal.wait(), 0, "{}", terminal.visible());
    terminal.transcript.clone()
}

// ── 1. STYLING APPEARS ONLY WHERE IT IS PERMITTED ───────────────────────────────────────────

#[test]
fn a_capable_terminal_gets_the_semantic_palette() {
    // The positive control for every "no escape sequences" assertion in `presentation.rs`. Without
    // this, all of those would pass just as happily against a program that had lost the ability to
    // emit colour at all.
    let transcript = doctor(100, IN_COLOUR);
    assert!(
        transcript.contains(BLUE),
        "INFO must be the information role: {transcript:?}"
    );
    assert!(
        transcript.contains(GREEN),
        "OK must be the success role: {transcript:?}"
    );
    assert!(
        transcript.contains("INFO") && transcript.contains("OK"),
        "the words must survive alongside the colour: {transcript:?}"
    );
}

#[test]
fn each_refusal_suppresses_colour_on_a_terminal_that_could_have_had_it() {
    // Each clause on its own, on a real terminal — the case that a piped test cannot reach, because
    // a pipe already refuses for its own reason and would hide a broken clause behind a correct
    // one.
    for (label, environment, arguments) in [
        (
            "--no-color",
            IN_COLOUR.to_vec(),
            vec!["doctor", "--no-color"],
        ),
        (
            "NO_COLOR=1",
            vec![("NO_COLOR", "1"), ("TERM", "xterm-256color")],
            vec!["doctor"],
        ),
        (
            "TERM=dumb",
            vec![("NO_COLOR", ""), ("TERM", "dumb")],
            vec!["doctor"],
        ),
        (
            "--output json",
            IN_COLOUR.to_vec(),
            vec!["--output", "json", "doctor"],
        ),
        (
            "CLICOLOR_FORCE cannot override --no-color",
            vec![
                ("NO_COLOR", ""),
                ("TERM", "xterm-256color"),
                ("CLICOLOR_FORCE", "1"),
            ],
            vec!["doctor", "--no-color"],
        ),
        (
            "CLICOLOR_FORCE cannot override NO_COLOR",
            vec![
                ("NO_COLOR", "1"),
                ("TERM", "xterm-256color"),
                ("CLICOLOR_FORCE", "1"),
            ],
            vec!["doctor"],
        ),
    ] {
        let root = workspace();
        let mut terminal = Terminal::spawn_sized(&arguments, root.path(), &environment, 100);
        assert_eq!(terminal.wait(), 0, "{label}: {}", terminal.visible());
        // POSITIVE CONTROL. "No colour" is satisfied by no output at all, so the assertions below
        // would pass against a binary that printed nothing — which is the failure mode that makes
        // an absence assertion worthless.
        assert!(
            terminal.visible().contains("cargo") || terminal.visible().contains("schemaVersion"),
            "{label} produced no readiness output, so the assertions below prove nothing: {}",
            terminal.visible()
        );
        // GREEN AND BLUE ONLY, AND CYAN IS LEFT OUT ON PURPOSE.
        //
        // `doctor` has no prompt and no progress, so it never reaches `Role::Accent` — measured:
        // a colour pty gives SGR 32 x3, SGR 34 x1, SGR 90 x3, and SGR 36 **x0**. Asserting the
        // absence of cyan here would have passed with `--no-color` deleted, which is a test that
        // reports success for a broken flag. The positive control above asserts blue and green
        // for the same reason: those are the two this command actually emits.
        for (name, sequence) in [("green", GREEN), ("blue", BLUE)] {
            assert!(
                !terminal.transcript.contains(sequence),
                "{label} did not suppress {name}: {:?}",
                terminal.transcript
            );
        }
    }
}

#[test]
fn the_accent_role_is_used_where_it_is_defined_to_be_used_and_suppressed_on_request() {
    // CYAN'S OWN POSITIVE CONTROL, WHICH THE COLOUR TESTS ABOVE COULD NOT GIVE IT.
    //
    // `Role::Accent` is defined for "the active question, the selected choice, a command name" —
    // and `doctor`, which every other colour test drives, has none of those. So no test asserted
    // that the accent ever appears, while six asserted it was absent. An absence with no matching
    // presence is not evidence.
    //
    // The wizard is where the accent lives, so the wizard is where it is checked.
    let root = workspace();
    let destination = root.path().join("demo");

    let mut styled = Terminal::spawn(
        &["new", "--path", destination.to_str().expect("utf-8")],
        root.path(),
        IN_COLOUR,
    );
    styled.expect("Project name");
    styled.escape();
    assert_eq!(styled.wait(), 4);
    assert!(
        styled.transcript.contains(CYAN),
        "the live question must be the accent role: {:?}",
        styled.transcript
    );

    let mut plain = Terminal::spawn(
        &[
            "new",
            "--no-color",
            "--path",
            destination.to_str().expect("utf-8"),
        ],
        root.path(),
        IN_COLOUR,
    );
    plain.expect("Project name");
    plain.escape();
    assert_eq!(plain.wait(), 4);
    assert!(
        !plain.transcript.contains(CYAN),
        "`--no-color` must reach the prompt library too: {:?}",
        plain.transcript
    );
}

#[test]
fn help_is_styled_on_a_terminal_and_not_when_the_flag_forbids_it() {
    // C-8 keeps the parser's renderer and applies this program's policy to it. The parser
    // suppresses its own colour for a pipe, for `NO_COLOR`, and for `TERM=dumb` — but it has never
    // heard of `--no-color`, so that clause is enforced where the help text is written, and it is
    // the clause worth its own assertion.
    let root = workspace();

    let mut styled = Terminal::spawn_sized(&["--help"], root.path(), IN_COLOUR, 100);
    assert_eq!(styled.wait(), 0);
    assert!(
        styled.transcript.contains('\u{1b}'),
        "`--help` on a terminal must use the palette: {:?}",
        styled.transcript
    );

    let mut plain = Terminal::spawn_sized(&["--help", "--no-color"], root.path(), IN_COLOUR, 100);
    assert_eq!(plain.wait(), 0);
    assert!(
        !plain.transcript.contains('\u{1b}'),
        "`--no-color` must reach the parser's rendering too: {:?}",
        plain.transcript
    );
    // And the CONTENT is the same either way. A colour policy that changed what `--help` says
    // would be a different kind of defect entirely.
    assert_eq!(
        styled.visible().replace("\r\n", "\n"),
        plain.visible().replace("\r\n", "\n"),
        "styling changed the help text"
    );
}

// ── 2. THE LAYOUT RESPONDS TO THE WIDTH ─────────────────────────────────────────────────────

#[test]
fn rows_fit_the_terminal_at_forty_eighty_and_one_hundred_and_twenty_columns() {
    // Three reviewed widths. 40 is narrow enough that a long value has to stack; 80 is the width
    // most terminals open at; 120 is wide enough to prove the measure cap is doing something.
    for columns in [40_u16, 80, 120] {
        let transcript = doctor(columns, &[]);
        let visible = transcript.replace('\r', "");
        let rows: Vec<&str> = visible
            .lines()
            .filter(|line| line.contains(" ...") || line.contains("... "))
            .collect();
        assert!(
            !rows.is_empty(),
            "no leader row was produced at {columns} columns: {visible:?}"
        );
        for row in &rows {
            let width = row.chars().count();
            assert!(
                width <= usize::from(columns),
                "a row overflowed {columns} columns ({width}): {row:?}"
            );
        }
    }
}

#[test]
fn a_narrow_terminal_stacks_rather_than_truncating() {
    // The fallback branch, end to end. At twenty columns nothing can share a line with a path, and
    // the rule is that the value survives whole — a value shortened to fit is a value that looks
    // like a different one, and here it is a filename an operator would go looking for.
    //
    // `check` on a missing manifest is the cheapest command that puts a long value in a row.
    // `layout.rs` proves the never-truncate property exhaustively across every width from one to
    // a hundred and twenty; what this adds is that the real binary reaches that branch.
    let root = workspace();
    let long = "a-project-directory-with-a-deliberately-long-name";
    let mut terminal = Terminal::spawn_sized(&["check", long], root.path(), &[], 20);
    assert_ne!(terminal.wait(), 0, "a missing manifest is a failure");
    let visible = terminal.visible();
    // ConPTY mirrors the screen, so a value wider than the terminal arrives wrapped. Flattening
    // the transcript is what "nothing was truncated" means on a terminal narrower than the value.
    let flat = visible.replace(['\r', '\n'], "");
    assert!(
        flat.contains(long),
        "the value was truncated away at 20 columns: {visible}"
    );
    assert!(
        flat.contains("renvor.toml"),
        "the row's other value was truncated away too: {visible}"
    );
}

#[test]
fn a_very_wide_terminal_does_not_produce_a_very_wide_leader() {
    // The measure cap. Without it a 400-column terminal puts several hundred dots between a label
    // and its value, and the leader stops doing the job it exists for.
    let transcript = doctor(400, &[]);
    let visible = transcript.replace('\r', "");
    let rows: Vec<&str> = visible
        .lines()
        .filter(|line| line.contains("..."))
        .collect();
    // POSITIVE CONTROL. A `for` over an empty iterator passes without executing its body, so
    // without this the test would report success on a build that produced no leader rows at all.
    assert!(
        !rows.is_empty(),
        "no leader row was produced at 400 columns, so the assertion below proves nothing: \
         {visible:?}"
    );
    for line in rows {
        assert!(
            line.chars().count() <= 120,
            "a row was allowed to grow to {} columns on a 400-column terminal: {line:?}",
            line.chars().count()
        );
    }
}

// ── 3. PROMPTS ──────────────────────────────────────────────────────────────────────────────

/// Starts `renvor new` on a terminal and waits for the first question.
fn wizard(root: &std::path::Path, destination: &std::path::Path) -> Terminal {
    let mut terminal = Terminal::spawn(
        &["new", "--path", destination.to_str().expect("utf-8")],
        root,
        &[],
    );
    terminal.expect("Project name");
    terminal
}

#[test]
fn control_c_at_a_prompt_is_a_cancellation() {
    // C-8's cancellation table, row one. The previous prompt library reported this as a typed enum
    // variant; this one reports an `io::ErrorKind`, and the whole reason the swap was measured
    // before it was made is that this exit code had to stay `4`.
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = wizard(root.path(), &destination);
    terminal.key("\u{3}");
    assert_eq!(
        terminal.wait(),
        4,
        "Ctrl-C must exit 4\n{}",
        terminal.visible()
    );
    assert!(
        !destination.exists(),
        "cancelling must leave the destination untouched"
    );
}

#[test]
fn escape_at_a_prompt_is_a_cancellation() {
    // Row two, and the one worth proving rather than assuming: the prompt library's own event
    // handling contains an Escape/Cancel toggle, so "Escape cancels" is a claim about a specific
    // version rather than about the shape of the API.
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = wizard(root.path(), &destination);
    terminal.escape();
    assert_eq!(
        terminal.wait(),
        4,
        "Escape must exit 4\n{}",
        terminal.visible()
    );
    assert!(!destination.exists());
}

#[test]
fn a_confirmation_submits_on_the_keypress_and_n_declines() {
    // The recorded behavioural difference from the previous prompt library, asserted so that it is
    // a known property rather than a surprise. `n` answers no with no Enter after it.
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = wizard(root.path(), &destination);
    terminal.enter();
    terminal.expect("Local development domain");
    terminal.enter();
    terminal.expect("Generate the example domain module?");
    terminal.key("n");
    // If `n` had not submitted, this next question would never be asked and the expect would time
    // out — which is the assertion.
    terminal.expect("Generate container development controls?");
    terminal.key("n");
    terminal.expect("Record that local HTTPS is wanted?");
    terminal.key("n");
    terminal.expect("Create this project?");
    terminal.key("n");
    assert_eq!(
        terminal.wait(),
        4,
        "declining the review exits 4\n{}",
        terminal.visible()
    );
    assert!(!destination.exists());
}

#[test]
fn a_prompt_never_appears_when_there_is_no_terminal_to_draw_it_on() {
    // C-8, and C-1 before it. The command must not block waiting for an answer nobody can give,
    // and the message must name what to pass instead — a refusal that says only "no terminal"
    // leaves an automation author guessing.
    let root = workspace();
    let (code, stdout, stderr) = harness::renvor(&["new"], root.path(), &[]);
    assert_eq!(code, 2, "a missing terminal is a usage error");
    assert_eq!(stdout, "", "nothing goes to stdout");
    assert!(
        stderr.contains("--path") || stderr.contains("NAME"),
        "the refusal must name what to supply instead: {stderr:?}"
    );
}

// ── 4. THE TERMINAL IS PUT BACK ─────────────────────────────────────────────────────────────

/// Whether a transcript ends with the cursor visible.
///
/// Hiding the cursor and showing it again are both explicit sequences, so the question is whether
/// the **last** one is the one that shows it. A transcript that never hid it is also fine, and is
/// the case for a run with no prompt.
fn cursor_is_visible_at_the_end(transcript: &str) -> bool {
    let hidden = transcript.rfind("\u{1b}[?25l");
    let shown = transcript.rfind(SHOW_CURSOR);
    match (hidden, shown) {
        (None, _) => true,
        (Some(_), None) => false,
        (Some(hidden), Some(shown)) => shown > hidden,
    }
}

#[test]
fn the_cursor_is_restored_after_a_successful_run() {
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = wizard(root.path(), &destination);
    // SEVEN answers, not six: the six wizard questions and then the review screen. Answering six
    // leaves the review waiting for an answer that never comes, which is a hang rather than a
    // failure — and a hang is the one outcome a test suite reports as "still running".
    for prompt in [
        "Local development domain",
        "Generate the example domain module?",
        "Generate seed data for it?",
        "Generate container development controls?",
        "Record that local HTTPS is wanted?",
    ] {
        terminal.enter();
        terminal.expect(prompt);
    }
    terminal.enter();
    terminal.expect("Create this project?");
    terminal.enter();
    assert_eq!(terminal.wait(), 0, "{}", terminal.visible());
    assert!(
        cursor_is_visible_at_the_end(&terminal.transcript),
        "a successful run left the cursor hidden"
    );
}

#[test]
fn the_cursor_is_restored_after_a_cancellation() {
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = wizard(root.path(), &destination);
    terminal.escape();
    assert_eq!(terminal.wait(), 4);
    assert!(
        cursor_is_visible_at_the_end(&terminal.transcript),
        "cancelling left the cursor hidden — it outlives the process and follows the reader into \
         their shell"
    );
}

#[test]
fn the_cursor_is_restored_after_a_failure() {
    // A run that reaches the prompts and then fails on something else. The destination's parent is
    // removed underneath it, so placement fails after the wizard has hidden the cursor.
    let root = workspace();
    let parent = root.path().join("gone");
    std::fs::create_dir_all(&parent).expect("the parent is created");
    let destination = parent.join("demo");
    let mut terminal = wizard(root.path(), &destination);
    terminal.enter();
    terminal.expect("Local development domain");
    std::fs::remove_dir_all(&parent).expect("the parent is removed");
    for _ in 0..5 {
        terminal.enter();
    }
    let code = terminal.wait();
    assert_ne!(
        code,
        0,
        "the run was expected to fail\n{}",
        terminal.visible()
    );
    assert!(
        cursor_is_visible_at_the_end(&terminal.transcript),
        "a failing run left the cursor hidden\n{}",
        terminal.visible()
    );
}

#[test]
fn the_cursor_is_restored_after_a_panic() {
    // THE CASE A `Drop` IMPLEMENTATION CANNOT COVER.
    //
    // The panic hook calls `std::process::exit`, which does not unwind and therefore runs no
    // destructors — so the progress indicator's `Drop` and the prompt's own cleanup both never
    // run. Restoring the cursor is done in the hook for exactly that reason, and this is the test
    // that says the reason was real.
    //
    // `RENVOR_PANIC_FOR_TESTS` only exists in a debug build, so this cannot reach a shipped binary.
    let root = workspace();
    let mut terminal =
        Terminal::spawn(&["doctor"], root.path(), &[("RENVOR_PANIC_FOR_TESTS", "1")]);
    let code = terminal.wait();
    assert_eq!(code, 1, "a panic exits 1\n{}", terminal.visible());
    assert!(
        terminal.transcript.contains(SHOW_CURSOR),
        "the panic path must restore the cursor before exiting: {:?}",
        terminal.transcript
    );
}

// ── 5. PROGRESS ─────────────────────────────────────────────────────────────────────────────

#[test]
fn the_verification_step_names_each_check_as_it_runs() {
    // The operation the indicator exists for: five `cargo` invocations whose output is captured,
    // so without this the terminal shows nothing at all for as long as a cold compile takes.
    //
    // Asserted on the check NAMES rather than on the spinner frames. The frames are a rendering
    // detail; the names are the information, and they are what turns "something is happening" into
    // "this is what is happening, and this is what to reproduce by hand if it fails".
    //
    // **`TERM` is set to a capable value on purpose.** The harness pins `TERM=dumb`, and a
    // redrawing indicator needs the cursor movement a dumb terminal does not have — so it draws
    // nothing there. That is correct and is recorded in C-8; it also means this test would assert
    // nothing at all if it used the harness default.
    //
    // Every answer is supplied as a flag so the wizard has nothing to ask. `--yes` would not do
    // it: C-1 scopes `--yes` to the review screen and it never waives the questions.
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = Terminal::spawn(
        &[
            "--dry-run",
            "new",
            "demo",
            "--path",
            destination.to_str().expect("utf-8"),
            "--local-domain",
            "demo.test",
            "--example-domain",
            "--seed-data",
            "--container",
            "--local-https",
        ],
        root.path(),
        &[("TERM", "xterm-256color")],
    );
    assert_eq!(terminal.wait(), 0, "{}", terminal.visible());
    let visible = terminal.visible();
    assert!(
        visible.contains("verifying the generated project"),
        "the indicator must name the operation: {visible}"
    );
    for check in [
        "cargo fmt --check",
        "cargo clippy -- -D warnings",
        "cargo build",
        "cargo test",
        "cargo run --quiet",
    ] {
        assert!(
            visible.contains(check),
            "the indicator must name {check:?} while it runs: {visible}"
        );
    }
}

#[test]
fn a_terminal_that_cannot_be_redrawn_still_gets_told_the_work_is_happening() {
    // A REGRESSION THIS BRANCH INTRODUCED, FOUND BY AN ADVISORY REVIEW, AND MEASURED BOTH WAYS.
    //
    // The progress library refuses to draw where it cannot redraw: `TERM=dumb`, and **also `TERM`
    // unset entirely**, which is the state of cron, systemd units, and several embedded
    // terminals. The code this branch replaced printed one static line unconditionally, so on
    // those terminals the first version of this change turned a line into tens of seconds of
    // total silence through a cold five-check build.
    //
    // That is the exact "indistinguishable from a hang" failure the indicator exists to fix, and
    // it landed hardest on the readers the rest of this contract is most careful about. The test
    // that used to sit here asserted the SILENCE as correct behaviour, which is how a regression
    // gets a green tick.
    //
    // What has to hold is not "an indicator appears" — it is that the operator is told. Both
    // spellings are checked below, on the two terminals that resolve differently.
    for term in ["dumb", "xterm-256color"] {
        let root = workspace();
        let destination = root.path().join("demo");
        let mut terminal = Terminal::spawn(
            &[
                "--dry-run",
                "new",
                "demo",
                "--path",
                destination.to_str().expect("utf-8"),
                "--local-domain",
                "demo.test",
                "--example-domain",
                "--seed-data",
                "--container",
                "--local-https",
            ],
            root.path(),
            &[("TERM", term)],
        );
        assert_eq!(terminal.wait(), 0, "TERM={term}: {}", terminal.visible());
        let visible = terminal.visible();
        assert!(
            visible.contains("verifying the generated project"),
            "TERM={term} was told nothing while five cargo checks ran: {visible}"
        );
        // And the command still reports its result, which is the half that matters most.
        assert!(visible.contains("Dry run:"), "TERM={term}: {visible}");
    }
}

#[test]
fn only_a_redrawable_terminal_gets_the_live_indicator() {
    // The other half: the FALLBACK is a static line, not a spinner drawn into a terminal that
    // cannot erase it. A dumb terminal must see the operation named once and no redraw at all.
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = Terminal::spawn(
        &[
            "--dry-run",
            "new",
            "demo",
            "--path",
            destination.to_str().expect("utf-8"),
            "--local-domain",
            "demo.test",
            "--example-domain",
            "--seed-data",
            "--container",
            "--local-https",
        ],
        root.path(),
        &[("TERM", "dumb")],
    );
    assert_eq!(terminal.wait(), 0, "{}", terminal.visible());
    let visible = terminal.visible();
    // The per-check names come from the indicator's message, which only a live bar renders.
    assert!(
        !visible.contains("cargo clippy"),
        "a dumb terminal was redrawn: {visible}"
    );
    assert_eq!(
        visible.matches("verifying the generated project").count(),
        1,
        "the fallback must be said once, not once per check: {visible}"
    );
}

// ── 6. JSON MODE ON A TERMINAL ──────────────────────────────────────────────────────────────

#[test]
fn json_mode_on_a_terminal_prompts_on_stderr_and_still_emits_one_document() {
    // THE CLAIM THIS TEST REPLACED WAS FALSE, AND THAT IS WHY THE TEST EXISTS.
    //
    // Contract C-8 originally said "a prompt is never shown in JSON mode". Measuring it showed
    // otherwise: C-1 makes the wizard conditional on `stdin` being a terminal and on nothing else,
    // so `renvor --output json new` on a terminal asks its questions exactly as the human-format
    // run does. A consumer that has left a terminal on `stdin` has asked for that.
    //
    // What actually has to hold is narrower and is what C-2 requires: the questions are on
    // `stderr`, and `stdout` carries exactly one JSON document. Asserting the true property is
    // worth more than asserting the memorable one.
    let root = workspace();
    let destination = root.path().join("demo");
    let mut terminal = Terminal::spawn(
        &[
            "--output",
            "json",
            "--dry-run",
            "new",
            "--path",
            destination.to_str().expect("utf-8"),
        ],
        root.path(),
        // `TERM` MUST BE CAPABLE HERE, AND THAT IS THE WHOLE POINT OF THE TEST.
        //
        // The harness pins `TERM=dumb`, on which the progress library refuses to draw whatever
        // the format says. Left at the default, the final assertion below would have been
        // satisfied by `TERM` rather than by `--output json`, and deleting
        // `&& self.format == Format::Human` from `Reporter::progress_visible` would have left it
        // green. An advisory review caught exactly that.
        &[("TERM", "xterm-256color")],
    );
    // The pty merges the two streams, so the prompt IS visible in the transcript — which is the
    // point: it proves the wizard ran.
    terminal.expect("Project name");
    terminal.send_line("demo");
    // THE WHOLE QUESTION, NOT ITS FIRST WORD.
    //
    // Three of these begin with "Generate", and `expect` scans a transcript that only grows — so
    // matching on the first word made the second and third calls return immediately against text
    // already on screen, and the `enter()` after them was sent before its prompt had been drawn.
    // Three of five synchronisation points were no-ops, and a prompt going missing in the middle
    // would not have been noticed.
    for prompt in [
        "Local development domain",
        "Generate the example domain module?",
        "Generate seed data for it?",
        "Generate container development controls?",
        "Record that local HTTPS is wanted?",
    ] {
        terminal.expect(prompt);
        terminal.enter();
    }
    terminal.enter();
    assert_eq!(terminal.wait(), 0, "{}", terminal.visible());

    // And exactly one document was produced. A pty cannot separate the streams, so the document is
    // found in the transcript and parsed — a second document, or a prompt inside the first, makes
    // this fail.
    let visible = terminal.visible().replace('\r', "");
    let start = visible
        .find('{')
        .unwrap_or_else(|| panic!("no JSON document was emitted\n{visible}"));
    let end = visible
        .rfind('}')
        .unwrap_or_else(|| panic!("the JSON document is unterminated\n{visible}"));
    let document: serde_json::Value = serde_json::from_str(&visible[start..=end])
        .unwrap_or_else(|error| panic!("stdout is not one JSON document: {error}\n{visible}"));
    assert_eq!(document["schemaVersion"], 2);
    assert_eq!(document["status"], "success");
    assert_eq!(
        visible[start..=end].matches("\"schemaVersion\"").count(),
        1,
        "more than one document reached stdout\n{visible}"
    );

    // AND THE PROGRESS INDICATOR IS STILL ABSENT, on a terminal, because the format forbids it.
    // `presentation.rs` asserts the same thing on a pipe, where the stream forbids it anyway —
    // which means that test alone could not tell the two reasons apart. This one can.
    assert!(
        !visible.contains("verifying the generated project"),
        "a progress indicator drew during a JSON run on a terminal\n{visible}"
    );
}
