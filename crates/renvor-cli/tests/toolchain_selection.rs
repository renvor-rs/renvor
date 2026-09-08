//! What selects the compiler, measured against a **second, genuinely different** toolchain.
//!
//! The controls of the brief's §5.4 — C-sel-1 … C-sel-4 — and the identity control that keeps
//! them honest. Each generates a skeleton with the built binary and reads two things: the record
//! the run placed (`.renvor/generated.toml`, §5.2 — `selected_by`, `resolved_rustc_release`, and
//! `[toolchain].pinned`) and the FR-012-8 (1) resolution notice, which is **one line on stderr and
//! never on stdout** (C-1 reserves stdout for the JSON result). The two streams are captured
//! separately, never concatenated: a test that reads them as one cannot tell "printed on stderr"
//! from "printed on stdout", which is the whole of what FR-012-8 promises about them.
//!
//! # Why these need a second toolchain, and how one is found
//!
//! A selection rule is only measurable when there are two things to select between. The second
//! toolchain is named by `RENVOR_TEST_CONTROL_TOOLCHAIN` and, in CI, provisioned deliberately by
//! the `verify` legs (U-10, approved 2026-09-07). **No local toolchain installation is
//! authorised**: when the variable is unset, or names a toolchain this machine does not have,
//! each control prints one `SKIPPED: <test>: <reason>` line and returns — unless
//! `RENVOR_TEST_REQUIRE_TOOLCHAINS=1`, which is what the CI legs set, and which turns every one
//! of those skips into a failure. That is the census pattern `starter_matrix.rs` already uses for
//! its services, applied to a toolchain.
//!
//! Whether the control is installed is read **from the filesystem** — the directories under
//! `$RUSTUP_HOME/toolchains`, else `<home>/.rustup/toolchains`. `rustup toolchain list` is never
//! run, here or anywhere in this crate (SR-012-4), and neither is any other rustup subcommand:
//! the only rustup this file invokes is the `rustc +<name>` proxy shorthand, which is a proxy
//! invocation rather than a subcommand, and which is given `RUSTUP_AUTO_INSTALL=0` so that it
//! cannot provision even if the filesystem check were somehow wrong.
//!
//! # The inherited selection this file has to defeat
//!
//! `cargo` is launched through a rustup proxy, and the proxy exports `RUSTUP_TOOLCHAIN` (and
//! `RUSTUP_TOOLCHAIN_SOURCE`) into the process it starts — so this test binary already runs with
//! the highest-precedence selection rustup knows, and every child would inherit it. Left alone,
//! **every** control below would measure the environment and pass while proving nothing about
//! proximity, files, or overrides. Each control therefore states the child's `RUSTUP_TOOLCHAIN`
//! explicitly, in whichever direction it means to test, and never relies on what it inherited.
//!
//! # What is here, and what is deliberately not
//!
//! - **C-sel-4** (`c_sel_4_the_environment_beats_the_file`, = AC-012-6) and **C-sel-1**
//!   (`c_sel_1_a_closer_toolchain_file_beats_a_farther_directory_override`) run.
//! - **C-sel-2** is present as an `#[ignore]`d test that explains itself. The brief's form of it
//!   needs `rustup override set` on the project directory, which writes a **persistent entry into
//!   the operator's `~/.rustup/settings.toml`** — a change to the machine the suite runs on, which
//!   this suite does not make. The consequence is stated rather than hidden: the
//!   `directory_override` attribution and the notice it produces are **unproven** by this file.
//! - **C-sel-3** is not here. It needs `renvor generate auth` on a legacy tree, which costs a
//!   framework checkout and a full starter build; it is deferred to the starter-matrix leg
//!   elsewhere in this batch, where a built starter already exists. Its negative half — the
//!   divergence check itself — is proved as a unit test
//!   (`a_divergent_scratch_resolution_is_refused`, in `src/commands/generate.rs`), and the
//!   sibling placement FR-012-13 requires is proved by
//!   `a_scratch_copy_is_a_sibling_of_the_project_with_the_residue_name` beside it.
//!
//! # What the controls select *against*, and why it is not always the control toolchain
//!
//! `the_control_toolchain_identity_differs_from_the_legs` is the CI step's own condition: the
//! control must not be the leg's own compiler. It is **necessary and not sufficient**. What these
//! two controls select against is the project's **pin**, and a skeleton pins the generator's own
//! MSRV ([`PIN`], D-L2-4) — the same release on every leg.
//!
//! U-10 provisions the control as the *other* leg's toolchain, so on the MSRV leg the control is
//! current stable and on the stable leg the control **is** that MSRV. A toolchain equal to the pin
//! cannot discriminate: `RUSTUP_TOOLCHAIN=<pin>` still reports `selected_by = "environment"`, but
//! it resolves the pin, so FR-012-8 (1) correctly prints nothing and there is no notice left to
//! assert; and an ancestor file naming the pin cannot be told apart from the project's own.
//!
//! So each control selects against [`distinct_from_the_pin`]: the control toolchain when its
//! release differs from the pin, and otherwise the **leg's own**, which on that leg is the one that
//! differs. Both legs still need two installed toolchains and still require the control to be named
//! and present — what changes is only which of the two plays `Y`, and the rule proved is the same
//! rule either way. When neither differs from the pin, the control skips with a reason that says
//! so, and fails under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1`.
//!
//! # Assertion messages are fixed
//!
//! No assertion below interpolates a value into its message, and none names a path: a path here
//! is a temporary directory that differs on every run and on every machine, and a failure message
//! carrying one teaches a reader nothing they can act on. `assert_eq!`/`assert_ne!` still print
//! their operands, which are releases, commit hashes, and attribution words — all machine
//! -independent, all exactly what a reader needs.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// ─────────────────────────────────────────────────────────── the gate

/// The release a generated skeleton pins: the generator's own MSRV (D-L2-4).
///
/// This crate *is* the generator, so its `rust-version` is the value `renvor new` renders into
/// `rust-toolchain.toml` and records as `[toolchain].pinned`. Each control re-asserts that against
/// the record it reads, so the premise is checked on every run rather than assumed.
const PIN: &str = env!("CARGO_PKG_RUST_VERSION");

/// Why a control cannot run. A closed set, so each refusal below is a fixed message.
#[derive(Clone, Copy, Debug)]
enum Missing {
    /// `RENVOR_TEST_CONTROL_TOOLCHAIN` is unset or empty.
    Unset,
    /// It names a toolchain with no directory under the rustup home.
    NotInstalled,
    /// It names an installed toolchain whose `rustc` did not answer `-vV`.
    Unanswerable,
    /// The control is the pinned release, and the leg's own toolchain cannot stand in for it.
    LegUnusable,
    /// The control is the pinned release, and so is the leg's own: nothing left to select against.
    NeitherDistinct,
}

impl Missing {
    /// The reason, for the `SKIPPED:` line.
    const fn reason(self) -> &'static str {
        match self {
            Self::Unset => "RENVOR_TEST_CONTROL_TOOLCHAIN is not set",
            Self::NotInstalled => {
                "RENVOR_TEST_CONTROL_TOOLCHAIN names a toolchain that is not installed"
            }
            Self::Unanswerable => "the control toolchain's rustc did not answer -vV",
            Self::LegUnusable => {
                "the control toolchain is the release the project pins, and the leg's own could \
                 not be named and probed through RUSTUP_TOOLCHAIN"
            }
            Self::NeitherDistinct => {
                "neither the control toolchain nor the leg's own differs in release from the \
                 release the project pins"
            }
        }
    }
}

/// Whether the gate requires the controls to run rather than skip — what CI sets.
fn required() -> bool {
    std::env::var("RENVOR_TEST_REQUIRE_TOOLCHAINS").is_ok_and(|value| value.trim() == "1")
}

/// Says the control cannot run: a `SKIPPED:` line locally, a failure under the requirement.
///
/// The panic arms are separate literals rather than one interpolated string, so that every
/// message this file can emit is fixed text a reader can grep for.
fn unavailable<T>(test: &str, missing: Missing) -> Option<T> {
    if required() {
        match missing {
            Missing::Unset => panic!(
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but RENVOR_TEST_CONTROL_TOOLCHAIN is not set"
            ),
            Missing::NotInstalled => panic!(
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but RENVOR_TEST_CONTROL_TOOLCHAIN names a toolchain that is not installed"
            ),
            Missing::Unanswerable => panic!(
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but the control toolchain's rustc did not answer -vV"
            ),
            Missing::LegUnusable => panic!(
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but the control toolchain is the release the project pins and the leg's own could not be named and probed through RUSTUP_TOOLCHAIN"
            ),
            Missing::NeitherDistinct => panic!(
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but neither the control toolchain nor the leg's own differs in release from the release the project pins"
            ),
        }
    }
    println!("SKIPPED: {test}: {}", missing.reason());
    None
}

/// The second toolchain a control selects against.
struct Control {
    /// Exactly as `RENVOR_TEST_CONTROL_TOOLCHAIN` spells it — what `+<name>` and
    /// `RUSTUP_TOOLCHAIN` are given.
    name: String,
    /// What that toolchain's `rustc` answers.
    identity: Identity,
}

/// The control toolchain, or `None` after saying why there is none.
fn control(test: &str) -> Option<Control> {
    let name = match std::env::var("RENVOR_TEST_CONTROL_TOOLCHAIN") {
        Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
        _ => return unavailable(test, Missing::Unset),
    };
    if !installed(&name) {
        return unavailable(test, Missing::NotInstalled);
    }
    let Some(identity) = identity_of(&name) else {
        return unavailable(test, Missing::Unanswerable);
    };
    Some(Control { name, identity })
}

/// A toolchain a selection control can select **against**: installed, nameable, and a different
/// release from the one the project pins.
struct Distinct {
    /// A name rustup accepts — for the child's `RUSTUP_TOOLCHAIN` and for a `rust-toolchain.toml`
    /// channel.
    name: String,
    /// What it answers to `-vV`.
    identity: Identity,
}

/// The toolchain these controls select against: **the control when its release is not the pin, and
/// otherwise the leg's own**.
///
/// # Why it is not simply the control toolchain
///
/// U-10 provisions the control as the *other* leg's toolchain. On the MSRV leg that is current
/// stable — distinct from the pin, and the natural `Y` of §5.4's controls. On the stable leg it is
/// the MSRV itself, which is exactly the release a skeleton pins, and a `Y` equal to `X` cannot
/// discriminate: the environment selects what the file would have selected, FR-012-8 (1) correctly
/// stays silent, and a farther toolchain file naming the pin is indistinguishable from the
/// project's own. On that leg the toolchain that differs from the pin is the leg's own, and it is
/// installed, named by `RUSTUP_TOOLCHAIN`, and just as good a `Y`.
///
/// This is a choice of experimental condition, not a fallback around a failure: the control
/// toolchain is still **required** to be named and installed on both legs — that is what makes two
/// toolchains present — and the rule under test is the same rule proved against the same kind of
/// difference. When neither differs from the pin there is nothing to select against, and that is
/// said rather than worked around.
fn distinct_from_the_pin(test: &str) -> Option<Distinct> {
    let control = control(test)?;
    if control.identity.release != PIN {
        return Some(Distinct {
            name: control.name,
            identity: control.identity,
        });
    }
    let Some(name) = leg_toolchain_name() else {
        return unavailable(test, Missing::LegUnusable);
    };
    if !installed(&name) {
        return unavailable(test, Missing::LegUnusable);
    }
    let Some(identity) = identity_of_the_leg() else {
        return unavailable(test, Missing::LegUnusable);
    };
    if identity.release == PIN {
        return unavailable(test, Missing::NeitherDistinct);
    }
    Some(Distinct { name, identity })
}

/// The leg's own toolchain as a name rustup can select, from `RUSTUP_TOOLCHAIN`.
///
/// The CI legs set that variable at job level, and locally the rustup proxy that launched cargo
/// exports the resolved name into this process. A release string is not a substitute: `stable`
/// resolves to a release whose own number names no installed toolchain, so `rustc +1.98.1` would
/// fail where `rustc +stable` succeeds.
fn leg_toolchain_name() -> Option<String> {
    match std::env::var("RUSTUP_TOOLCHAIN") {
        Ok(value) if !value.trim().is_empty() => Some(value.trim().to_owned()),
        _ => None,
    }
}

/// Where rustup keeps its toolchains: `$RUSTUP_HOME/toolchains`, else `<home>/.rustup/toolchains`.
fn toolchains_directory() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("RUSTUP_HOME") {
        return Some(PathBuf::from(home).join("toolchains"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".rustup").join("toolchains"))
}

/// Whether a toolchain is installed, **read from the filesystem**.
///
/// rustup names a toolchain's directory `<channel>-<host triple>`, and accepts the bare channel
/// on the command line — so `1.95.0` is installed when `1.95.0-aarch64-apple-darwin` is there.
/// A full name matches itself. Nothing here runs rustup: a listing subcommand is exactly what
/// SR-012-4 forbids, and on a rustup that auto-installs, asking is not a read-only question.
fn installed(name: &str) -> bool {
    let Some(directory) = toolchains_directory() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return false;
    };
    let prefix = format!("{name}-");
    entries.filter_map(Result::ok).any(|entry| {
        entry.path().is_dir()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|found| found == name || found.starts_with(prefix.as_str()))
    })
}

// ─────────────────────────────────────────────────────── compiler identity

/// A compiler's answer to `-vV`, in the two fields §5.4's identity control compares.
#[derive(Debug)]
struct Identity {
    release: String,
    commit: String,
}

/// One field of a `-vV` answer (`release: 1.95.0`), or `None` when it carries no such line.
fn field(answer: &str, name: &str) -> Option<String> {
    answer.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key == name).then(|| value.trim().to_owned())
    })
}

/// Runs `rustc -vV` and reads the two fields, or `None` when it did not answer.
///
/// `RUSTUP_AUTO_INSTALL=0` on every one of these: the probe exists to observe a toolchain, and an
/// observation that installs what it was looking for is not one (SR-012-1). Installed-ness has
/// already been read from the filesystem before any `+<name>` form reaches this.
fn query(
    toolchain: Option<&str>,
    cwd: &Path,
    keep_environment_selection: bool,
) -> Option<Identity> {
    let mut command = Command::new("rustc");
    if let Some(name) = toolchain {
        command.arg(format!("+{name}"));
    }
    command
        .arg("-vV")
        .current_dir(cwd)
        .env("RUSTUP_AUTO_INSTALL", "0");
    if !keep_environment_selection {
        command.env_remove("RUSTUP_TOOLCHAIN");
        command.env_remove("RUSTUP_TOOLCHAIN_SOURCE");
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let answer = String::from_utf8(output.stdout).ok()?;
    Some(Identity {
        release: field(&answer, "release")?,
        commit: field(&answer, "commit-hash")?,
    })
}

/// `rustc +<toolchain> -vV`: rustup's command-line shorthand, the top of its documented order, so
/// this names the control whatever else is in force.
fn identity_of(toolchain: &str) -> Option<Identity> {
    query(Some(toolchain), &manifest_directory(), true)
}

/// `rustc -vV` under exactly what this test binary runs under — the leg's own compiler.
fn identity_of_the_leg() -> Option<Identity> {
    query(None, &manifest_directory(), true)
}

/// `rustc -vV` in a directory, with the inherited `RUSTUP_TOOLCHAIN` removed: what that
/// directory's own position in the tree selects, and nothing else.
fn identity_from_the_directory(cwd: &Path) -> Option<Identity> {
    query(None, cwd, false)
}

/// This crate's directory: a real path inside the checkout, so `identity_of_the_leg` measures the
/// selection the leg actually verifies under rather than one a temporary directory would give.
fn manifest_directory() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// ─────────────────────────────────────────────────────────── generation

/// The FR-012-8 (1) notice's opening words, for the "printed nowhere" assertions.
const RESOLUTION_NOTICE: &str = "toolchain resolved before verification:";

/// Generates a skeleton at `project`, run from `cwd`, with the child's `RUSTUP_TOOLCHAIN` set to
/// `toolchain` — or removed when it is `None`.
///
/// `--output json` puts the result on stdout, which is what makes the stderr assertions meaningful
/// (C-1). The two streams come back apart, and are never joined.
fn generate(cwd: &Path, project: &Path, name: &str, toolchain: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_renvor"));
    command
        .current_dir(cwd)
        .arg("new")
        .arg(name)
        .arg("--path")
        .arg(project)
        .arg("--yes")
        .arg("--output")
        .arg("json");
    // See the module header: this process inherits a `RUSTUP_TOOLCHAIN` from the rustup proxy that
    // launched cargo, and an inherited one outranks everything these controls are about.
    command.env_remove("RUSTUP_TOOLCHAIN_SOURCE");
    if let Some(selected) = toolchain {
        command.env("RUSTUP_TOOLCHAIN", selected);
    } else {
        command.env_remove("RUSTUP_TOOLCHAIN");
    }
    command.output().expect("the generator runs")
}

/// The placed record, parsed.
fn record(project: &Path) -> toml::Value {
    let text = std::fs::read_to_string(project.join(".renvor").join("generated.toml"))
        .expect("the placed project carries its record");
    toml::from_str(&text).expect("the record parses as TOML")
}

/// A string field of a record table (`[toolchain] pinned`, `[verified_with] selected_by`, …).
fn field_of(record: &toml::Value, table: &str, key: &str) -> String {
    record
        .get(table)
        .and_then(|table| table.get(key))
        .and_then(toml::Value::as_str)
        .expect("the record carries the field")
        .to_owned()
}

/// Asserts the run succeeded and its stdout is the one JSON result C-1 promises.
///
/// The status is read from the parsed document rather than from the exit code alone, so a run that
/// exited zero having reported a failure could not pass as a success.
fn succeeded(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the generator refused; its stderr is above"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout is one JSON document");
    assert_eq!(
        parsed["status"], "success",
        "the JSON result does not report a success"
    );
    stdout
}

// ─────────────────────────────────────────────────────────── the controls

/// The control that keeps every other control honest: the two toolchains must really be two.
///
/// This mirrors the step the CI legs run before the suite (§5.4, U-10) *inside* the suite, so a
/// leg whose control toolchain was provisioned as the same release fails these tests rather than
/// passing them vacuously. The brief's words: a control run against an identical identity proves
/// nothing.
#[test]
fn the_control_toolchain_identity_differs_from_the_legs() {
    let Some(control) = control("the_control_toolchain_identity_differs_from_the_legs") else {
        return;
    };
    let leg = identity_of_the_leg().expect("the leg's own rustc answers -vV");
    assert_ne!(
        leg.release, control.identity.release,
        "the control toolchain has the leg's own release, so it selects nothing new"
    );
    assert_ne!(
        leg.commit, control.identity.commit,
        "the control toolchain has the leg's own commit, so it selects nothing new"
    );
}

/// **C-sel-4 (= AC-012-6).** `RUSTUP_TOOLCHAIN` beats the project's own `rust-toolchain.toml`.
///
/// rustup's documented order puts the environment variable second, above both a directory override
/// and a toolchain file, and §5.8's second row says what that produces: `environment`, the selected
/// toolchain's release resolved, the pin recorded unchanged, and the FR-012-8 (1) resolution notice
/// — on stderr, once, and not on stdout.
///
/// `Y` is [`distinct_from_the_pin`] rather than the control toolchain itself; the two coincide on
/// the MSRV leg and differ on the stable leg, for the reason given there.
#[test]
fn c_sel_4_the_environment_beats_the_file() {
    let Some(selected) = distinct_from_the_pin("c_sel_4_the_environment_beats_the_file") else {
        return;
    };
    let workspace = tempfile::tempdir().expect("tempdir");
    let project = workspace.path().join("environment-wins");

    let output = generate(
        workspace.path(),
        &project,
        "environment-wins",
        Some(selected.name.as_str()),
    );
    let stdout = succeeded(&output);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let record = record(&project);
    let pinned = field_of(&record, "toolchain", "pinned");
    // THE PREMISE OF THE CHOICE ABOVE. `distinct_from_the_pin` compared against `PIN`, this crate's
    // own `rust-version`; if a skeleton ever stopped pinning that, the toolchain it picked would
    // have been chosen against the wrong release and every assertion below would be measuring
    // something else.
    assert_eq!(
        pinned, PIN,
        "the skeleton does not pin the generator's own MSRV, which is what the selected toolchain \
         was chosen to differ from"
    );
    // AND SO THE CONTROL DISCRIMINATES: were the two the same release, everything below would hold
    // for a run in which nothing was overridden at all.
    assert_ne!(
        pinned, selected.identity.release,
        "the selected toolchain is the release the project pins, so the environment selects exactly \
         what the file would have and there is no notice to assert"
    );

    assert_eq!(
        field_of(&record, "verified_with", "selected_by"),
        "environment",
        "the environment did not win the selection"
    );
    assert_eq!(
        field_of(&record, "verified_with", "resolved_rustc_release"),
        selected.identity.release,
        "the resolution is not the selected toolchain's release"
    );

    let expected = format!(
        "toolchain resolved before verification: rustc {} (environment); the project pins {pinned}",
        selected.identity.release
    );
    assert!(
        stderr.lines().any(|line| line == expected),
        "the FR-012-8 (1) resolution notice is not on stderr in its exact wording"
    );
    assert!(
        !stdout.contains(RESOLUTION_NOTICE),
        "the resolution notice reached stdout, which C-1 reserves for the result"
    );
}

/// **C-sel-1, in the half a suite may prove without changing the machine.** A `rust-toolchain.toml`
/// closer to the working directory beats one further away.
///
/// # Which half this is
///
/// The brief's C-sel-1 sets a **directory override** (`rustup override set`) on the parent and a
/// pin in the project, and asserts the project's file wins because it is closer. Two facts hold
/// that control up: rustup's **proximity** rule ("these two override methods are discovered by
/// walking up the directory tree… and a `rust-toolchain.toml` file that is closer to the current
/// directory will be preferred over a directory override that is further away"), and the existence
/// of the directory-override mechanism itself.
///
/// **This test proves the proximity half.** The farther selection is written as a
/// `rust-toolchain.toml` in an ancestor directory rather than as a directory override, because
/// `rustup override set` writes a persistent entry into the operator's rustup settings and this
/// suite does not change the machine it runs on. **It does not prove the directory-override half**
/// — that a `directory_override` entry exists, is discovered by walking up, and is what
/// `selected_by = "directory_override"` reports. That half is `c_sel_2` below, which is deferred
/// for the same reason and says so.
///
/// The ancestor file is proved live before the project is generated, by asking what that directory
/// alone selects. Without that step a passing result would be indistinguishable from an ancestor
/// file rustup never read. The child is run with `RUSTUP_TOOLCHAIN` **removed**: an inherited
/// environment selection outranks both files, and the proximity rule would never be exercised.
///
/// The ancestor names [`distinct_from_the_pin`] rather than the control toolchain itself; the two
/// coincide on the MSRV leg and differ on the stable leg, for the reason given there.
#[test]
fn c_sel_1_a_closer_toolchain_file_beats_a_farther_directory_override() {
    let Some(selected) =
        distinct_from_the_pin("c_sel_1_a_closer_toolchain_file_beats_a_farther_directory_override")
    else {
        return;
    };
    let ancestor = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        ancestor.path().join("rust-toolchain.toml"),
        format!("[toolchain]\nchannel = \"{}\"\n", selected.name),
    )
    .expect("the ancestor toolchain file is written");

    // THE ANCESTOR FILE IS LIVE. Asked from the ancestor directory itself, with the inherited
    // environment selection removed, rustup answers with the farther selection — so the file below
    // is one the project's own pin has to beat, not an inert decoration.
    let from_the_ancestor =
        identity_from_the_directory(ancestor.path()).expect("rustc answers in the ancestor");
    assert_eq!(
        from_the_ancestor.release, selected.identity.release,
        "the ancestor's toolchain file selects nothing, so the proximity rule is untested"
    );

    let project = ancestor.path().join("closer-wins");
    // `None`: the child must run with no `RUSTUP_TOOLCHAIN` at all, or the environment outranks
    // both files and neither the ancestor's nor the project's would decide anything.
    let output = generate(ancestor.path(), &project, "closer-wins", None);
    let stdout = succeeded(&output);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let record = record(&project);
    let pinned = field_of(&record, "toolchain", "pinned");
    assert_eq!(
        pinned, PIN,
        "the skeleton does not pin the generator's own MSRV, which is what the ancestor's channel \
         was chosen to differ from"
    );
    assert_ne!(
        pinned, selected.identity.release,
        "the ancestor's channel is the release the project pins, so the closer file cannot be told \
         from the farther one"
    );

    assert_eq!(
        field_of(&record, "verified_with", "selected_by"),
        "toolchain_file",
        "the closer toolchain file did not win the selection"
    );
    assert_eq!(
        field_of(&record, "verified_with", "resolved_rustc_release"),
        pinned,
        "the resolution is not the project's own pin"
    );

    // NO NOTICE, ON EITHER STREAM: FR-012-8 (1) speaks only when the resolution is not the pin,
    // and here it is the pin. §5.8's fourth row: "nothing extra".
    assert!(
        !stderr.contains(RESOLUTION_NOTICE),
        "a resolution notice was printed for a run that resolved the pin"
    );
    assert!(
        !stdout.contains(RESOLUTION_NOTICE),
        "a resolution notice reached stdout, which C-1 reserves for the result"
    );
}

/// **C-sel-2 — NOT RUN, and this is what it would have proved.**
///
/// The brief's control sets a directory override on the project directory itself with a pin
/// already in it, and asserts rustup's order puts the override first: the control's release
/// resolved, `selected_by = "directory_override"`, and the FR-012-8 (1) resolution notice with
/// `(directory_override)`.
///
/// It is not run because the only way to create a directory override is `rustup override set`,
/// which writes a **persistent entry into `~/.rustup/settings.toml`** — the operator's own rustup
/// configuration, on the machine the suite happens to be running on. That entry outlives the test,
/// outlives the temporary directory it names, and would silently change how every later command in
/// that directory resolves. This suite reads the machine; it does not edit it. There is no
/// non-mutating equivalent: unlike a toolchain file, a directory override has no on-disk form
/// inside the directory it governs.
///
/// What that costs is stated rather than hidden: **`directory_override` is unproven end to end.**
/// The attribution string rustup prints for it is pinned by a unit test of the grammar
/// (`parse_active_toolchain_attribution`, `src/toolchain/grammar.rs`), and the notice it produces
/// is pinned by the table test in `src/toolchain/notice.rs` — but that the two meet, on a real
/// override, in a real generation, is not something this file establishes. Running it by hand,
/// with `--ignored`, is not enough either: it would still have to create the override, which is
/// the thing being avoided.
#[test]
#[ignore = "creating a directory override writes a persistent entry into the operator's rustup settings"]
fn c_sel_2_a_directory_override_on_the_project_beats_its_file() {
    println!(
        "SKIPPED: c_sel_2_a_directory_override_on_the_project_beats_its_file: this control is \
         not run — it needs `rustup override set`, which writes a persistent entry into the \
         operator's rustup settings; `directory_override` is therefore unproven end to end"
    );
}
