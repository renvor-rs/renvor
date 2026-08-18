//! SC-003 and FR-006: prompt input and flag input produce the same project.
//!
//! # This file exists because the structural argument was not sufficient
//!
//! `prompts::fill` returns the same `Answers` the flag parser returns, and
//! `ProjectConfiguration::resolve` is the only constructor, so the two interfaces **cannot**
//! diverge in the configuration model. That argument is sound and it was, for a while, the whole
//! of the evidence for SC-003.
//!
//! It was not enough. The defect recorded as 2a-0 in `tasks.md` lived entirely **upstream** of the
//! shared model: the wizard computed its own default project name from the raw requested path
//! while `resolve` computed it from the validated destination, so the two interfaces could hand
//! the shared constructor **different answers** and the shared constructor would faithfully turn
//! them into different projects. A test that compares the two interfaces end to end is the only
//! thing that catches that class, and until this file it did not exist.
//!
//! # The wizard here is the real wizard
//!
//! Every assertion below drives `renvor` through a pseudo-terminal, so `stdin.is_terminal()` is
//! true and `prompts::fill` actually runs. `the_wizard_runs_only_because_stdin_is_a_terminal`
//! is the control for that claim: the same arguments with a pipe on stdin never render a prompt.

mod harness;

use std::collections::BTreeMap;
use std::path::Path;

use harness::{Terminal, renvor};

/// Every file in `root`, keyed by its path relative to `root`, with contents.
///
/// Compares **contents rather than hashes**, so a failure prints the differing text instead of two
/// unequal hex strings.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    fn walk(directory: &Path, prefix: &str, files: &mut BTreeMap<String, Vec<u8>>) {
        let mut entries: Vec<_> = std::fs::read_dir(directory)
            .expect("the generated tree is readable")
            .map(|entry| entry.expect("an entry is readable"))
            .collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().expect("a file type is readable").is_dir() {
                walk(&entry.path(), &relative, files);
            } else {
                files.insert(
                    relative,
                    std::fs::read(entry.path()).expect("a file is readable"),
                );
            }
        }
    }
    walk(root, "", &mut files);
    files
}

/// Drives the wizard to completion, accepting every default, and returns the exit code.
///
/// The prompts are named in the order `prompts::fill` asks them. Waiting for each **by its own
/// text** rather than blindly writing seven newlines is deliberate: if a prompt is added, removed,
/// or reordered, this fails at the prompt that went missing and names it, instead of silently
/// feeding an answer to the wrong question.
fn drive_wizard_accepting_defaults(terminal: &mut Terminal) -> i32 {
    terminal.expect("Project name");
    terminal.enter();
    terminal.expect("Local development domain");
    terminal.enter();
    terminal.expect("Generate the example domain module?");
    terminal.enter(); // default yes
    terminal.expect("Generate seed data for it?");
    terminal.enter(); // default no
    terminal.expect("Generate container development controls?");
    terminal.enter(); // default no
    terminal.expect("Record that local HTTPS is wanted?");
    terminal.enter(); // default no
    terminal.expect("Create this project?");
    terminal.enter(); // default yes
    terminal.wait()
}

#[test]
fn a_wizard_run_and_a_flag_run_with_equivalent_answers_produce_byte_identical_projects() {
    // T024, SC-003, FR-006. Both destinations are named `demo` under different parents, because
    // the project name is an ANSWER and the parent is not: comparing `w/demo` with `f/demo`
    // isolates the interface from the path.
    let root = tempfile::tempdir().expect("a temporary directory");
    let wizard_parent = root.path().join("w");
    let flag_parent = root.path().join("f");
    std::fs::create_dir_all(&wizard_parent).expect("the parent is created");
    std::fs::create_dir_all(&flag_parent).expect("the parent is created");

    let mut terminal = Terminal::spawn(
        &[
            "new",
            "--path",
            wizard_parent.join("demo").to_str().expect("utf-8"),
        ],
        root.path(),
        &[],
    );
    let wizard_exit = drive_wizard_accepting_defaults(&mut terminal);
    assert_eq!(
        wizard_exit,
        0,
        "the wizard run succeeds\n--- transcript ---\n{}",
        terminal.visible()
    );

    // The equivalent non-interactive command. `--example-domain` because the wizard's default for
    // that prompt is yes; every other prompt defaults to no, which is the flag being absent.
    let (flag_exit, _, stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            flag_parent.join("demo").to_str().expect("utf-8"),
            "--example-domain",
        ],
        root.path(),
        &[],
    );
    assert_eq!(flag_exit, 0, "the flag run succeeds: {stderr}");

    let from_prompts = tree(&wizard_parent.join("demo"));
    let from_flags = tree(&flag_parent.join("demo"));

    assert_eq!(
        from_prompts.keys().collect::<Vec<_>>(),
        from_flags.keys().collect::<Vec<_>>(),
        "both interfaces produce the same file list"
    );
    for (path, prompt_bytes) in &from_prompts {
        let flag_bytes = &from_flags[path];
        assert_eq!(
            String::from_utf8_lossy(prompt_bytes),
            String::from_utf8_lossy(flag_bytes),
            "{path} differs between the wizard and the flags"
        );
    }
    // Named separately so a failure says *which* promise broke. `renvor.toml` is the file SC-003
    // is actually about — it is the record of what was chosen.
    assert_eq!(
        from_prompts["renvor.toml"], from_flags["renvor.toml"],
        "the recorded manifest is byte-identical"
    );
}

#[test]
fn a_different_wizard_answer_really_does_produce_a_different_project() {
    // The control for the test above. Without it, a bug that made both interfaces produce the
    // same WRONG project — or that made `tree` return nothing — would satisfy the comparison.
    let root = tempfile::tempdir().expect("a temporary directory");
    let defaults_parent = root.path().join("d");
    let answered_parent = root.path().join("a");
    std::fs::create_dir_all(&defaults_parent).expect("the parent is created");
    std::fs::create_dir_all(&answered_parent).expect("the parent is created");

    let mut defaults = Terminal::spawn(
        &[
            "new",
            "--path",
            defaults_parent.join("demo").to_str().expect("utf-8"),
        ],
        root.path(),
        &[],
    );
    assert_eq!(drive_wizard_accepting_defaults(&mut defaults), 0);

    // The same wizard, answering the container prompt YES instead of accepting the default.
    let mut answered = Terminal::spawn(
        &[
            "new",
            "--path",
            answered_parent.join("demo").to_str().expect("utf-8"),
        ],
        root.path(),
        &[],
    );
    answered.expect("Project name");
    answered.enter();
    answered.expect("Local development domain");
    answered.enter();
    answered.expect("Generate the example domain module?");
    answered.enter();
    answered.expect("Generate seed data for it?");
    answered.enter();
    answered.expect("Generate container development controls?");
    answered.send_line("y");
    answered.expect("Record that local HTTPS is wanted?");
    answered.enter();
    answered.expect("Create this project?");
    answered.enter();
    let exit = answered.wait();
    assert_eq!(
        exit,
        0,
        "the answered run succeeds\n--- transcript ---\n{}",
        answered.visible()
    );

    let from_defaults = tree(&defaults_parent.join("demo"));
    let from_answers = tree(&answered_parent.join("demo"));
    assert_ne!(
        from_defaults, from_answers,
        "answering differently must produce a different project"
    );
    assert!(
        !from_defaults.contains_key("compose.yaml") && from_answers.contains_key("compose.yaml"),
        "the container answer is what differs: {:?} vs {:?}",
        from_defaults.keys().collect::<Vec<_>>(),
        from_answers.keys().collect::<Vec<_>>()
    );
}

#[test]
fn the_wizard_runs_only_because_stdin_is_a_terminal() {
    // The control for the whole file. If this ever fails, every test above is exercising the flag
    // path while claiming to exercise the wizard — the exact failure mode a piped-stdin "prompt
    // test" has, and the reason this suite needs a pty rather than a pipe.
    //
    // Both halves run the SAME arguments. The only difference is what is on stdin.
    let root = tempfile::tempdir().expect("a temporary directory");
    let arguments = ["new", "--path"];

    let with_terminal = root.path().join("t");
    std::fs::create_dir_all(&with_terminal).expect("the parent is created");
    let mut terminal = Terminal::spawn(
        &[
            arguments[0],
            arguments[1],
            with_terminal.join("demo").to_str().expect("utf-8"),
        ],
        root.path(),
        &[],
    );
    terminal.expect("Project name");
    assert!(
        terminal.visible().contains("Project name"),
        "a terminal must produce the wizard's first prompt"
    );
    terminal.escape();
    let cancelled = terminal.wait();
    assert_eq!(cancelled, 4, "cancelling the wizard exits 4");

    let without_terminal = root.path().join("p");
    std::fs::create_dir_all(&without_terminal).expect("the parent is created");
    let (exit, stdout, stderr) = renvor(
        &[
            arguments[0],
            arguments[1],
            without_terminal.join("demo").to_str().expect("utf-8"),
        ],
        root.path(),
        &[],
    );
    assert_eq!(
        exit, 0,
        "the same command without a terminal completes rather than waiting: {stderr}"
    );
    assert!(
        !stdout.contains("Project name") && !stderr.contains("Project name"),
        "FR-010: no prompt may be rendered when stdin is not a terminal\nstdout: {stdout}\nstderr: {stderr}"
    );
}

#[test]
fn without_a_terminal_an_answer_nothing_determines_is_refused_rather_than_invented() {
    // T047 and FR-010's third clause, which is the one worth being precise about.
    //
    // FR-010 forbids the wizard substituting "defaults for answers that were not supplied". The
    // values `renvor` derives without a terminal — the project name from the destination's final
    // component, the local domain from the name — are **not** that: they are stated in `--help` as
    // the flag's documented default, they are derived from input the operator did supply, and the
    // SAME derivation is what the wizard offers as its prompt default (one owner each; see the
    // 2a-0 record in `tasks.md`). This test pins that distinction to the case where the
    // distinction bites: when NOTHING the operator typed determines the answer, there is no
    // default to document and the command must refuse rather than invent one.
    let root = tempfile::tempdir().expect("a temporary directory");
    let (exit, stdout, stderr) = renvor(&["new", "--output", "json"], root.path(), &[]);
    assert_eq!(exit, 2, "a usage error, not a guess: {stderr}");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["error"]["code"], "usage");
    assert_eq!(
        document["error"]["details"]["missing"], "NAME or --path",
        "the refusal names what was missing rather than saying only that something was"
    );
    assert!(
        std::fs::read_dir(root.path())
            .expect("readable")
            .next()
            .is_none(),
        "nothing was created"
    );
}

#[test]
fn the_help_text_documents_every_default_the_non_interactive_path_applies() {
    // The evidence for the reading above, asserted rather than argued. If a derived default is
    // ever added without documenting it, FR-010's third clause stops being satisfiable by the
    // "documented contract" reading and this fails — which is the point.
    let root = tempfile::tempdir().expect("a temporary directory");
    let (exit, stdout, _) = renvor(&["new", "--help"], root.path(), &[]);
    assert_eq!(exit, 0);
    for documented in [
        "Defaults to the destination's final component",
        "Defaults to `./<name>`",
        "Defaults to `<name>.test`",
    ] {
        assert!(
            stdout.contains(documented),
            "`--help` must document: {documented}\n{stdout}"
        );
    }
}

/// Splits a printed command the way a POSIX shell would, for the single-quoting `shell_quote` uses.
///
/// Deliberately small and deliberately strict: it understands exactly the two forms
/// `equivalent_command` can emit — a bare word, and a single-quoted word with `'\''` escapes — and
/// panics on anything else rather than silently mis-splitting. A general shell parser here would
/// be able to *accept* output that a real shell would reject, which is the opposite of what this
/// test is for.
fn split_command(line: &str) -> Vec<String> {
    let mut arguments = Vec::new();
    let mut current = String::new();
    let mut characters = line.chars().peekable();
    let mut started = false;
    while let Some(character) = characters.next() {
        match character {
            ' ' if !started => {}
            ' ' => {
                arguments.push(std::mem::take(&mut current));
                started = false;
            }
            '\'' => {
                started = true;
                loop {
                    match characters.next() {
                        Some('\'') => break,
                        Some(inner) => current.push(inner),
                        None => panic!("unterminated quote in {line:?}"),
                    }
                }
            }
            other => {
                started = true;
                current.push(other);
            }
        }
    }
    if started {
        arguments.push(current);
    }
    arguments
}

#[test]
fn the_equivalent_command_printed_by_the_wizard_actually_reproduces_the_project() {
    // FR-009 requires the review screen to show "the exact equivalent non-interactive command".
    //
    // ── THIS TEST FOUND A REAL DEFECT ──────────────────────────────────────────────────
    //
    // The printed command was `renvor new <destination> --name <name> …`, which is not a command:
    // `NewArgs` takes the name positionally and the destination as `--path`, and declares no
    // `--name` flag. Pasting the wizard's own output produced clap's *"unexpected argument
    // '--name'"*. Every existing test asserted the string CONTAINED things; none ran it. So the
    // one requirement the string exists to satisfy was the one nothing checked.
    //
    // The shape below is what makes that checkable: decline the review, so the wizard prints the
    // command and creates nothing, then run the command it printed and compare the result against
    // a wizard run that accepted.
    let root = tempfile::tempdir().expect("a temporary directory");
    let from_command = root.path().join("a");
    let from_wizard = root.path().join("b");
    std::fs::create_dir_all(&from_command).expect("the parent is created");
    std::fs::create_dir_all(&from_wizard).expect("the parent is created");

    // 1. Drive the wizard and DECLINE, so nothing is written and the command is printed anyway.
    let mut declined = Terminal::spawn(
        &[
            "new",
            "--path",
            from_command.join("demo").to_str().expect("utf-8"),
        ],
        root.path(),
        &[],
    );
    for prompt in [
        "Project name",
        "Local development domain",
        "Generate the example domain module?",
        "Generate seed data for it?",
        "Generate container development controls?",
        "Record that local HTTPS is wanted?",
    ] {
        declined.expect(prompt);
        declined.enter();
    }
    declined.expect("Create this project?");
    declined.send_line("n");
    assert_eq!(declined.wait(), 4, "declining exits 4");
    assert!(
        !from_command.join("demo").exists(),
        "declining must create nothing — otherwise step 3 below is not running the printed command"
    );

    let visible = declined.visible();
    // FOUND BY SUBSTRING, NOT BY LINE.
    //
    // ConPTY mirrors the screen rather than the bytes written, and on Windows it merged the review
    // screen's last file-list entry with the line after it — `    src/main.rs  equivalent command:
    // renvor new …`. No line *started* with the prefix, so a line-oriented search found nothing
    // while the text was plainly there. The program's output was correct; the harness's assumption
    // about line structure was not.
    let start = visible
        .find("equivalent command: ")
        .unwrap_or_else(|| panic!("no equivalent command was printed\n{visible}"))
        + "equivalent command: ".len();
    let printed = visible[start..]
        .split(['\r', '\n'])
        .next()
        .expect("a first segment always exists")
        .trim()
        .to_owned();
    assert!(
        !printed.is_empty(),
        "the equivalent command was empty\n{visible}"
    );

    // 2. It must be OUR command, in the surface's own spelling.
    let arguments = split_command(&printed);
    assert_eq!(
        arguments[0], "renvor",
        "the printed command names the executable: {printed}"
    );
    assert_eq!(arguments[1], "new", "{printed}");
    assert!(
        !arguments.iter().any(|argument| argument == "--name"),
        "`--name` is not a flag this program has; the printed command must use the real surface: \
         {printed}"
    );

    // 3. RUN IT, verbatim, with nothing edited.
    let (exit, _, stderr) = renvor(
        &arguments[1..]
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        root.path(),
        &[],
    );
    assert_eq!(
        exit, 0,
        "the wizard's own equivalent command must run: `{printed}`\n{stderr}"
    );

    // 4. And produce exactly what accepting the wizard would have produced.
    let mut accepted = Terminal::spawn(
        &[
            "new",
            "--path",
            from_wizard.join("demo").to_str().expect("utf-8"),
        ],
        root.path(),
        &[],
    );
    assert_eq!(
        drive_wizard_accepting_defaults(&mut accepted),
        0,
        "{}",
        accepted.visible()
    );

    assert_eq!(
        tree(&from_command.join("demo")),
        tree(&from_wizard.join("demo")),
        "the printed command produced a different project from the wizard it claims to reproduce"
    );
}
