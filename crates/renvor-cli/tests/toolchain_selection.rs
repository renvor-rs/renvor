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
//! - **C-sel-2** (`c_sel_2_a_directory_override_on_the_project_beats_its_file`) runs, against a
//!   **real** directory override. A directory override has no on-disk form inside the directory it
//!   governs — it is a row in `$RUSTUP_HOME/settings.toml` — so the question was never whether one
//!   could be created without writing a persistent entry, but **whose file** the entry goes in.
//!   [`PrivateRustup`] answers it: a temporary `RUSTUP_HOME` whose `toolchains` is a symlink to the
//!   real one, so the override is written there, the already-installed toolchains are readable,
//!   nothing is installed, and the operator's `~/.rustup/settings.toml` is never opened — which
//!   that control re-reads and asserts. Unix only, for the symlink.
//! - **C-sel-3** is not here. It needs `renvor generate auth` to run its checks on a legacy tree,
//!   which costs a framework checkout and a full starter build; it lives in `starter_matrix.rs`,
//!   where a built starter already exists. Its negative half — the divergence check itself — is
//!   proved as a unit test (`a_divergent_scratch_resolution_is_refused`, in
//!   `src/commands/generate.rs`) and, on a real cause, by C-sel-2 above; the sibling placement
//!   FR-012-13 requires is proved by
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
    /// A rustup home of this test's own could not be made, or `rustup override set` declined to
    /// write into it — so the control has no override to measure and refuses to invent one.
    NoPrivateRustup,
    /// The legacy tree the auth control plans against could not be prepared: this machine has no
    /// SHA-256 command to re-record the manifest's digest with, or the fixture's record is not the
    /// shape this helper knows. A DIFFERENT cause from [`Missing::NoPrivateRustup`], and named
    /// separately so a failure under the requirement points at the right thing.
    NoAuthTree,
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
            Self::NoPrivateRustup => {
                "a private rustup home with a directory override could not be created, and the \
                 operator's own rustup settings are never written to"
            }
            Self::NoAuthTree => {
                "the legacy tree this control plans against could not be prepared: no sha256 \
                 command, or an unexpected fixture record"
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
            Missing::NoPrivateRustup => panic!(
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but a private rustup home with a directory override could not be created"
            ),
            Missing::NoAuthTree => panic!(
                "RENVOR_TEST_REQUIRE_TOOLCHAINS=1, but the legacy tree this control plans against could not be prepared"
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
/// a toolchain file needs no rustup state at all, and the proximity rule is the half this control
/// is about. **It does not prove the directory-override half** — that a `directory_override` entry
/// exists, is discovered, and is what `selected_by = "directory_override"` reports. That half is
/// `c_sel_2` below, which proves it against a real override written into a rustup home of its own.
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

// ───────────────────────────────────────────────── private rustup state

/// A rustup home of this test's own: real toolchains, a settings file nobody else reads.
///
/// # Why the operator's settings are never touched
///
/// A directory override has no on-disk form inside the directory it governs — it is a row in
/// `$RUSTUP_HOME/settings.toml`, keyed by absolute path. So creating one *does* write a
/// persistent entry, and the question is only **whose file** it is written to. `RUSTUP_HOME` is
/// the whole answer: pointed at a temporary directory, `rustup override set` writes there and the
/// operator's `~/.rustup/settings.toml` is not opened. The temporary directory goes away with the
/// test, and with it the row.
///
/// # Why the toolchains are a symlink and not a copy
///
/// rustup reads installed toolchains from `$RUSTUP_HOME/toolchains`. Copying them would be
/// gigabytes and would be a toolchain installation by another name, which no authorisation covers.
/// A symlink to the real directory makes the already-installed toolchains readable from the
/// private home and installs nothing: `RUSTUP_AUTO_INSTALL=0` is in force for everything the
/// generator runs, and every name used below is checked as installed, on the filesystem, first.
///
/// Unix only: the symlink is what makes this work, and `std`'s Windows equivalent needs a
/// privilege an ordinary CI account does not have. The Windows legs of the matrix do not set
/// `RENVOR_TEST_CONTROL_TOOLCHAIN`, so no control here runs there either way.
#[cfg(unix)]
struct PrivateRustup {
    home: tempfile::TempDir,
}

#[cfg(unix)]
impl PrivateRustup {
    /// A private home whose `toolchains` is the real one, or `None` when there is nothing to
    /// point at.
    fn create() -> Option<Self> {
        let real = toolchains_directory()?;
        if !real.is_dir() {
            return None;
        }
        let home = tempfile::tempdir().ok()?;
        std::os::unix::fs::symlink(&real, home.path().join("toolchains")).ok()?;
        // `version = "12"` is the settings schema rustup 1.29 writes; the default toolchain is
        // named so that a directory with no override and no file still resolves something.
        let default = leg_toolchain_name().unwrap_or_else(|| "stable".to_owned());
        std::fs::write(
            home.path().join("settings.toml"),
            format!(
                "version = \"12\"\ndefault_toolchain = \"{default}\"\nprofile = \
                 \"minimal\"\n\n[overrides]\n"
            ),
        )
        .ok()?;
        Some(Self { home })
    }

    fn path(&self) -> &Path {
        self.home.path()
    }

    /// `rustup override set --path <directory> <toolchain>`, written into **this** home.
    fn override_set(&self, directory: &Path, toolchain: &str) -> bool {
        Command::new("rustup")
            .args(["override", "set", toolchain, "--path"])
            .arg(directory)
            .env("RUSTUP_HOME", self.path())
            .env("RUSTUP_AUTO_INSTALL", "0")
            .env_remove("RUSTUP_TOOLCHAIN")
            .env_remove("RUSTUP_TOOLCHAIN_SOURCE")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    /// Whether the settings file this home owns now names `directory` — read back, so a control
    /// cannot pass on an override rustup declined to write.
    fn governs(&self, directory: &Path) -> bool {
        std::fs::read_to_string(self.home.path().join("settings.toml"))
            .is_ok_and(|text| text.contains(&directory.display().to_string()))
    }
}

/// A file's SHA-256, in the hex form the provenance record uses, from whichever hasher this
/// machine has — or `None`, which makes the control skip rather than guess.
///
/// # Why a child process and not a crate
///
/// The record digests an unmarked file over its own bytes, so this is a plain SHA-256. Adding a
/// hashing dependency to the test build to compute one value in one control is a larger change to
/// the crate's graph than the control is worth, and the lockfile closure is something this project
/// keeps small on purpose. `sha256sum` is coreutils (the Linux legs); `shasum -a 256` is Perl's
/// (macOS). Both print `<hex>  <path>`.
#[cfg(unix)]
fn sha256_of(path: &Path) -> Option<String> {
    for (program, arguments) in [("sha256sum", &[][..]), ("shasum", &["-a", "256"][..])] {
        let Ok(output) = Command::new(program).args(arguments).arg(path).output() else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let hex = text.split_whitespace().next()?.to_owned();
        if hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(hex);
        }
    }
    None
}

/// A starter tree `generate auth` can plan against, copied from the template-7 fixture, or `None`
/// when its record cannot be made to match the copy.
///
/// The fixture is a legacy tree, which is what makes it usable here: a legacy record declares no
/// pin, so the resolution is held to nothing and a control may select any installed toolchain
/// without tripping the MSRV prerequisite. Two edits are made to `renvor.toml` and both are
/// stated — `[framework].path` is pointed at this checkout, because the fixture records the
/// `/opt/renvor` no machine has, and `[capabilities].mail` is turned on, because `generate auth`
/// refuses a session starter without the capability it needs.
///
/// # Why the record is rewritten with it
///
/// `auth` re-renders `renvor.toml`, so an edited manifest whose recorded digest still describes
/// the fixture is `changed_since_generation` and the run refuses **before** it resolves anything
/// — the conflict check comes first precisely so that a refusal costs no build (C-5). Recording
/// the copy's own digest is what makes the edit invisible to that check; it changes what the tree
/// says about itself, never what the generator does with it.
#[cfg(unix)]
fn auth_planning_tree() -> Option<tempfile::TempDir> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("template-7-project");
    let root = tempfile::tempdir().expect("tempdir");
    let destination = root.path().join("legacy-api");
    copy_tree(&fixture, &destination);

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root is two levels above this crate")
        .to_path_buf();
    let escaped = workspace
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let manifest_path = destination.join("renvor.toml");
    let manifest = std::fs::read_to_string(&manifest_path)
        .expect("readable")
        .replace("path = \"/opt/renvor\"", &format!("path = \"{escaped}\""))
        .replace("mail = false", "mail = true");
    std::fs::write(&manifest_path, manifest).expect("writable");

    let digest = sha256_of(&manifest_path)?;
    let record_path = destination.join(".renvor").join("generated.toml");
    let record = std::fs::read_to_string(&record_path).expect("readable");
    let mut lines: Vec<String> = record.lines().map(str::to_owned).collect();
    let manifest_entry = lines
        .iter()
        .position(|line| line == "path = \"renvor.toml\"")?;
    let sha = lines.get_mut(manifest_entry + 1)?;
    if !sha.starts_with("sha256 = ") {
        return None;
    }
    *sha = format!("sha256 = \"{digest}\"");
    std::fs::write(&record_path, format!("{}\n", lines.join("\n"))).expect("writable");
    Some(root)
}

/// Copies a directory tree, files and directories only.
#[cfg(unix)]
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("readable") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// **C-sel-2.** A directory override on the project directory itself beats the project's own
/// `rust-toolchain.toml`, and the generator reports it as `directory_override`.
///
/// # Why this runs `generate auth` and not `renvor new`
///
/// `renvor new` resolves in its **staging** directory, whose name carries the process id and a
/// clock reading (`generate::place::Staging::create`) and cannot be known before the run. rustup
/// prefers a toolchain file to a directory override only when the file is *closer*; the staged
/// tree always holds the freshly rendered `rust-toolchain.toml` at its own level, and any override
/// a test could set is at the parent's. So through `renvor new`, `selected_by =
/// "directory_override"` is unreachable **by construction** — not merely untested. The directory
/// `generate auth` resolves in is the project's own, which a test does choose, and that is where
/// the brief's control lives.
///
/// # Why the run refuses, and why that is the control passing
///
/// FR-012-13 has `generate auth` resolve twice: in the project directory, and in the scratch copy
/// beside it. The override governs the project directory alone; the scratch copy is a sibling and
/// carries the project's `rust-toolchain.toml`, which is then the closest selection it has. The
/// two resolutions therefore differ **because the override won in one of them**, and the run
/// refuses with `toolchain_resolution_diverged` — after printing the FR-012-8 (1) notice for the
/// project directory, which is the assertion this control is about. The refusal is a second
/// measurement in the same run: the divergence check firing on a real cause rather than a
/// constructed one.
///
/// Nothing is written: the refusal precedes every check and every placement, and the command is
/// given `--dry-run` besides.
#[cfg(unix)]
#[test]
fn c_sel_2_a_directory_override_on_the_project_beats_its_file() {
    const TEST: &str = "c_sel_2_a_directory_override_on_the_project_beats_its_file";
    let Some(selected) = distinct_from_the_pin(TEST) else {
        return;
    };
    let Some(private) = PrivateRustup::create() else {
        let _: Option<()> = unavailable(TEST, Missing::NoPrivateRustup);
        return;
    };
    let Some(root) = auth_planning_tree() else {
        let _: Option<()> = unavailable(TEST, Missing::NoAuthTree);
        return;
    };
    let project = root.path().join("legacy-api");
    // The file the override has to beat. It names the pin, which both legs have installed.
    std::fs::write(
        project.join("rust-toolchain.toml"),
        format!("[toolchain]\nchannel = \"{PIN}\"\n"),
    )
    .expect("the project's own pin is written");

    if !private.override_set(&project, &selected.name) || !private.governs(&project) {
        let _: Option<()> = unavailable(TEST, Missing::NoPrivateRustup);
        return;
    }
    // AND THE OPERATOR'S OWN SETTINGS ARE UNTOUCHED. Read from the real home directly: the whole
    // premise of this control is that the override went somewhere else.
    if let Some(real) = toolchains_directory().and_then(|path| path.parent().map(Path::to_path_buf))
        && let Ok(theirs) = std::fs::read_to_string(real.join("settings.toml"))
    {
        assert!(
            !theirs.contains(&project.display().to_string()),
            "the override was written into the operator's own rustup settings"
        );
    }

    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .current_dir(&project)
        .args([
            "generate",
            "auth",
            "--dry-run",
            "--overwrite-unchanged",
            "--output",
            "json",
        ])
        .env("RUSTUP_HOME", private.path())
        .env_remove("RUSTUP_TOOLCHAIN")
        .env_remove("RUSTUP_TOOLCHAIN_SOURCE")
        .output()
        .expect("the generator runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // THE NOTICE, for the project directory: the override's release, attributed to the override.
    assert!(
        stderr.contains(RESOLUTION_NOTICE),
        "no resolution notice was printed:\n{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "rustc {} (directory_override)",
            selected.identity.release
        )),
        "the override did not win, or was not attributed to itself:\n{stderr}"
    );
    assert!(
        !stdout.contains(RESOLUTION_NOTICE),
        "a resolution notice reached stdout, which C-1 reserves for the result"
    );

    // AND THE DIVERGENCE, for the reason above.
    assert!(
        !output.status.success(),
        "the run did not refuse:\n{stdout}"
    );
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(
        document["error"]["details"]["reason"], "toolchain_resolution_diverged",
        "the refusal is not the one the override causes: {document}"
    );
    assert!(
        !project.join(".renvor-scratch").exists(),
        "a scratch directory survived the refusal"
    );
}
