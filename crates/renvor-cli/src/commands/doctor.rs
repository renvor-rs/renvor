//! `renvor doctor` — environment readiness.
//!
//! # It reports; it never installs
//!
//! FR-036's principle generalises: a diagnostic that fixes things is a diagnostic nobody can run
//! safely. `doctor` reads versions and reports them. Every remedy is printed for the operator to
//! run, never executed.
//!
//! # No network
//!
//! FR-043. Every probe below runs a local executable with `--version`. Nothing resolves a name or
//! opens a socket, which is why the offline test needs no network stub.

use std::path::Path;

use cap_std::fs::Dir;

use serde::Serialize;

use crate::exit::{CliError, Exit};
use crate::generate::verify::Sealed;
use crate::output::Reporter;
use crate::output::layout::{Mark, Report, Status};

/// One thing checked.
///
/// FR-032 and T065 want three things reported for anything missing **or incompatible**: the
/// required version, the found version, and the corrective action. `required` is deliberately not
/// one of them — it is a boolean meaning "this command needs it", and an earlier version of this
/// struct had it standing in for a version constraint that did not exist.
#[derive(Debug, Clone, Serialize)]
// camelCase because every other key in the C-2 envelope is camelCase — `schemaVersion`, `dryRun`,
// `templateVersion`, `orphanedStaging`. The snake_case default shipped two inconsistent keys
// (`found_version`, `required_version`) into the same document as `orphanedStaging`, which the JSON
// shape snapshot in `tests/cli.rs` caught on the first run.
#[serde(rename_all = "camelCase")]
pub struct Probe {
    /// The executable.
    pub tool: String,
    /// Whether it was found and runnable.
    pub found: bool,
    /// Its reported version string, verbatim, when found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The version parsed out of that string, when one could be.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub found_version: Option<String>,
    /// The minimum version this project needs, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_version: Option<String>,
    /// Whether the command needs it, or merely benefits from it.
    pub required: bool,
    /// `false` only when a minimum is declared, a version was parsed, and it is below the minimum.
    ///
    /// An unparseable version is **not** reported as incompatible. Refusing to run because a tool
    /// printed its version in a shape this parser did not expect would be a diagnostic breaking the
    /// environment it exists to describe.
    pub compatible: bool,
    /// What to do when it is missing or too old. Printed, never run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

/// The Rust version this project requires.
///
/// **Read from the manifest, not restated.** `CARGO_PKG_RUST_VERSION` is `rust-version`, which
/// `renvor-cli` inherits from `[workspace.package]` — the single authoritative declaration (ADR-0002).
/// A hard-coded `"1.94.0"` here would be a second declaration, and second declarations drift.
const REQUIRED_RUST: &str = env!("CARGO_PKG_RUST_VERSION");

/// What `doctor` looks for: the executable, whether it is required, its minimum version if it has
/// one, and the remedy.
///
/// `git` and `docker` carry no minimum deliberately. This phase uses neither for anything version
/// dependent, and inventing a floor would produce a diagnostic that fails an environment which
/// works — which is how a `doctor` command ends up wrapped in `|| true`.
const TOOLS: [(&str, bool, Option<&str>, &str); 3] = [
    (
        "cargo",
        true,
        Some(REQUIRED_RUST),
        "install or update Rust from https://rustup.rs, then `rustup update stable`",
    ),
    (
        "git",
        false,
        None,
        "install git from your platform's package manager",
    ),
    (
        "docker",
        false,
        None,
        "install a container runtime; only `renvor docker` needs it",
    ),
];

/// Extracts a semantic version from a tool's `--version` line.
///
/// Tools disagree about the shape: `cargo 1.94.0 (abc 2026-01-01)`, `git version 2.39.5`,
/// `Docker version 27.0.3, build 7d4bcd8`. So this takes the first whitespace-separated token that
/// parses, after stripping the punctuation tools put around it.
///
/// Two-component versions (`1.94`) are padded to three, because `semver` requires all three and
/// some tools print two. Returning `None` is a normal outcome, not a failure — see [`Probe::compatible`].
fn extract_version(line: &str) -> Option<semver::Version> {
    line.split_whitespace()
        .map(|token| token.trim_matches(|c: char| c == ',' || c == '(' || c == ')' || c == 'v'))
        .find_map(|token| {
            semver::Version::parse(token).ok().or_else(|| {
                // `1.94` → `1.94.0`, but only when it really is two numeric components.
                let parts: Vec<&str> = token.split('.').collect();
                (parts.len() == 2
                    && parts
                        .iter()
                        .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
                .then(|| semver::Version::parse(&format!("{token}.0")).ok())
                .flatten()
            })
        })
}

/// Probes one tool by running it with `--version`.
///
/// Running the executable rather than searching `PATH` is deliberate: a name on `PATH` that is not
/// executable, or is a broken shim, is exactly the case a diagnostic exists to catch, and a `PATH`
/// search reports it as present.
///
/// # Sealed (FR-012-6), in `directory` (FR-012-14)
///
/// `doctor` is named in the requirement's own list
/// of every child the seal covers, and the reason is the same here as everywhere else: a probe
/// that inherited the operator's shell would hand a rustup proxy an install server and whatever
/// `RUSTUP_AUTO_INSTALL` the operator had set, so a *diagnostic* could provision a toolchain.
///
/// `sealed` and `directory` are parameters rather than read here so a test can hand this function
/// an environment of its own — the crate forbids `unsafe`, and mutating this process's variables
/// is the only other way to observe what a child received.
fn probe(
    tool: &str,
    required: bool,
    minimum: Option<&str>,
    remedy: &str,
    sealed: &crate::generate::verify::Sealed,
    directory: &std::path::Path,
) -> Probe {
    let output =
        crate::generate::verify::sealed_command(std::ffi::OsStr::new(tool), sealed, directory)
            .arg("--version")
            .output();
    match output {
        Ok(output) if output.status.success() => {
            let line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let found_version = extract_version(&line);
            // Incompatible ONLY when a minimum is declared AND a version was parsed AND it is
            // below. An unparseable version leaves `compatible` true; see `Probe::compatible`.
            let compatible = compatible(
                minimum
                    .and_then(|m| semver::Version::parse(m).ok())
                    .as_ref(),
                found_version.as_ref(),
            );
            Probe {
                tool: tool.to_owned(),
                found: true,
                version: Some(line),
                found_version: found_version.map(|version| version.to_string()),
                required_version: minimum.map(str::to_owned),
                required,
                compatible,
                // A remedy accompanies an incompatible tool as well as an absent one: "too old" with
                // no instruction is the same dead end as "missing" with no instruction.
                remedy: (!compatible).then(|| remedy.to_owned()),
            }
        }
        _ => Probe {
            tool: tool.to_owned(),
            found: false,
            version: None,
            found_version: None,
            required_version: minimum.map(str::to_owned),
            required,
            compatible: false,
            remedy: Some(remedy.to_owned()),
        },
    }
}

/// Decides whether a found version satisfies a declared minimum.
///
/// # Why this is a separate function
///
/// The rule has an asymmetry worth isolating: **an unparseable or absent version is NOT reported as
/// incompatible**. A tool that prints its version in a shape this parser does not recognise is
/// *unknown*, not *too old*, and refusing to run because of it would be a diagnostic breaking the
/// environment it exists to describe.
///
/// It was inline, and the test named for that rule passed for an unrelated reason: it asserted
/// `probe.compatible || probe.found_version.is_some()`, and cargo's version always parses, so the
/// second disjunct was independently true and `compatible` was never evaluated for truth. Inverting
/// the rule left the test green. Found by an advisory review reading the assertion rather than its
/// name. Extracted so all six combinations can be stated directly.
fn compatible(minimum: Option<&semver::Version>, found: Option<&semver::Version>) -> bool {
    match (minimum, found) {
        (Some(floor), Some(found)) => found >= floor,
        // No minimum declared, or a version we could not read: not a failure.
        _ => true,
    }
}

/// Orphaned staging directories found in the current directory.
///
/// # Why `doctor` reports these and does not remove them
///
/// `Staging`'s `Drop` cleans up on every ordinary failure including a panic, but a `SIGKILL` runs
/// no destructor — so residue survives, by design, named `.renvor-staging-{pid}-…` and placed
/// **beside** the destination rather than inside it. `tests/acceptance.rs` proves that.
///
/// The half that was missing is that nothing helped an operator find it. This does.
///
/// **It does not delete them** (T066, contract C-5). A diagnostic that deletes is a diagnostic
/// nobody can run safely on a directory they care about — and renvor cannot tell an abandoned
/// staging directory from one belonging to a `renvor new` running in another terminal **right
/// now**. The remedy is printed for the operator to run, with the process id visible so they can
/// check before removing anything.
/// What `doctor` reports about the toolchain here (FR-012-11), or `None` outside a project.
///
/// Every field is REPORTED, never acted on: `doctor` installs nothing, sets no default, and runs
/// no listing command (SR-012-4). A value it could not obtain is `None` and renders as *unknown*
/// — it is never filled in from a neighbouring field, because the whole point of the section is
/// to tell the operator what is actually true here rather than what ought to be.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainSection {
    /// The channel `rust-toolchain.toml` pins in this directory.
    pub pinned: Option<String>,
    /// `rust-version` as the project's manifest declares it.
    pub rust_version: Option<String>,
    /// What the provenance record says verified the tree, or `None` for a record that predates
    /// the field or a tree with no record at all.
    pub verified_with: Option<String>,
    /// rustup's own version, when a rustup could be located and asked.
    pub rustup: Option<String>,
    /// The floor below which the pin probes are not run at all.
    pub floor: &'static str,
    /// The compiler that resolves HERE, asked rather than inferred.
    pub resolved: Option<String>,
    /// Why that compiler and not another.
    pub selected_by: Option<String>,
    /// Whether the resolved compiler is a rustup proxy (FR-012-7c).
    pub proxy: bool,
    /// Whether the PIN is installed with the components verification needs.
    pub components: ComponentReport,
}

/// Whether the PINNED toolchain is installed with the components verification needs.
///
/// Three states, and the difference between them is the whole point (FR-012-11, D-L2-7):
/// `probed` means the three commands ran and this is what they said; `not probed` means renvor
/// declined to run them and says why; `not applicable` means `+channel` selects nothing here at
/// all. A blank row would collapse all three into "we don't know", which is the reading the
/// operator would then have to guess at.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentReport {
    /// `probed`, `not probed`, or `not applicable`.
    pub state: &'static str,
    /// Why, in the operator's own words, when the probes did not run.
    pub reason: Option<String>,
    /// Whether each of the three answered, when they were probed.
    pub rustc: Option<bool>,
    /// Whether `rustfmt +<pin> --version` answered.
    pub rustfmt: Option<bool>,
    /// Whether `cargo +<pin> clippy --version` answered.
    pub clippy: Option<bool>,
}

impl ComponentReport {
    fn not_applicable(reason: &str) -> Self {
        Self {
            state: "not applicable",
            reason: Some(reason.to_owned()),
            rustc: None,
            rustfmt: None,
            clippy: None,
        }
    }

    fn not_probed(reason: String) -> Self {
        Self {
            state: "not probed",
            reason: Some(reason),
            rustc: None,
            rustfmt: None,
            clippy: None,
        }
    }
}

/// One `+channel` probe under the seal. Reports only whether it answered.
///
/// The seal already carries `RUSTUP_AUTO_INSTALL=0`, so rustup answers "is not installed" BY
/// NAME and downloads nothing. That is what makes these probes permissible at all: they answer
/// the operator's question without being able to change the machine, which is the reconciliation
/// D-L2-7 asked for and the reason no listing command is needed.
fn component_answers(program: &str, arguments: &[&str], dir: &Path, sealed: &Sealed) -> bool {
    let mut command =
        crate::generate::verify::sealed_command(std::ffi::OsStr::new(program), sealed, dir);
    command.args(arguments);
    crate::toolchain::isolate::run_bounded(command, crate::toolchain::isolate::PROBE_TIMEOUT)
        .is_ok_and(|answer| answer.status.success())
}

/// The component report for this directory.
fn components_here(dir: &Path, sealed: &Sealed, pinned: Option<&str>) -> ComponentReport {
    let Some(pin) = pinned else {
        return ComponentReport::not_applicable("this directory pins no channel");
    };

    // `identify` is the ONE place the floor and the proxy classification are decided (FR-012-7a).
    // Its refusal already says why in the operator's words — below the floor, an unidentifiable
    // proxy — so the reason is carried through rather than re-derived here, where it would drift.
    match crate::toolchain::identify(sealed) {
        Err(error) => ComponentReport::not_probed(error.message.clone()),
        Ok(crate::toolchain::Classification::Bare) => ComponentReport::not_applicable(
            "no rustup: `+channel` selects nothing, so the pin cannot be probed",
        ),
        Ok(crate::toolchain::Classification::Proxy { .. }) => {
            let channel = format!("+{pin}");
            ComponentReport {
                state: "probed",
                reason: None,
                rustc: Some(component_answers("rustc", &[&channel, "-vV"], dir, sealed)),
                rustfmt: Some(component_answers(
                    "rustfmt",
                    &[&channel, "--version"],
                    dir,
                    sealed,
                )),
                clippy: Some(component_answers(
                    "cargo",
                    &[&channel, "clippy", "--version"],
                    dir,
                    sealed,
                )),
            }
        }
    }
}

/// Is there a project in this directory?
///
/// `renvor.toml` OR `rust-toolchain.toml`: a tree generated before template version 8 has the
/// first and not the second, and a hand-written crate may have the second and not the first.
/// Requiring both would omit the section exactly where an operator is most likely to be asking.
/// Builds the toolchain section for `dir`, or `None` when there is no project here.
///
/// The order is deliberate and is the reconciliation D-L2-7 asked for. Everything READ from
/// files comes first and always happens. The probes come second and are **conditional**: below
/// the rustup floor, or on a proxy whose rustup cannot be located, they are not run at all and
/// the operator is told why rather than shown a blank.
///
/// `doctor` never runs `rustup toolchain list`, or any installing, updating, or default-setting
/// command (SR-012-4). The operator's real question — "is the pin usable here, and what will
/// actually run?" — is answered with proxy probes that cannot install anything.
pub fn toolchain_section(dir: &Path, sealed: &Sealed) -> Option<ToolchainSection> {
    if !is_project(dir) {
        return None;
    }

    // ── READ ────────────────────────────────────────────────────────────────────────
    let pinned = crate::toolchain::pin::read_channel(&dir.join("rust-toolchain.toml"))
        .ok()
        .map(|channel| channel.to_string());
    let rust_version = crate::toolchain::pin::read_msrv(&dir.join("Cargo.toml"))
        .ok()
        .map(|version| version.to_string());
    // The record is read through the SAME reader rule every other command uses (FR-012-5b), so a
    // version this generator does not know is refused there rather than reported here as absent.
    // A tree with no record at all is `None` — indistinguishable, deliberately, from a record
    // that predates the field: both mean "this tree cannot tell you", and inventing a
    // distinction the file cannot support is how a report starts lying.
    let verified_with = Dir::open_ambient_dir(dir, cap_std::ambient_authority())
        .ok()
        .and_then(|opened| crate::generate::record::read(&opened).ok().flatten())
        .and_then(|record| record.verified_with)
        .and_then(|verified| {
            let release = verified.rustc_release?;
            Some(match verified.rustc_commit {
                Some(commit) => format!("rustc {release} ({commit})"),
                None => format!("rustc {release}"),
            })
        });

    // ── PROBE ───────────────────────────────────────────────────────────────────────
    //
    // A classification failure is not a doctor failure: the section reports what it could not
    // learn. `doctor` reports and changes nothing, including its own exit code (FR-012-11).
    let pinned_for_probe = pinned.clone();
    let classification = crate::toolchain::identify(sealed).ok();
    let expectations = crate::toolchain::Expectations {
        pinned: pinned.clone(),
        rust_version: None,
    };
    let resolution = classification.as_ref().and_then(|classification| {
        crate::toolchain::resolve(dir, sealed, classification, &expectations).ok()
    });

    Some(ToolchainSection {
        pinned,
        rust_version,
        verified_with,
        rustup: resolution
            .as_ref()
            .and_then(|resolution| resolution.rustup.as_ref())
            .map(std::string::ToString::to_string),
        floor: crate::toolchain::RUSTUP_FLOOR,
        resolved: resolution.as_ref().map(|resolution| {
            format!(
                "rustc {} ({})",
                resolution.rustc.release, resolution.rustc.commit
            )
        }),
        selected_by: resolution
            .as_ref()
            .map(|resolution| resolution.selected_by.as_str().to_owned()),
        proxy: resolution
            .as_ref()
            .is_some_and(|resolution| resolution.proxy),
        components: components_here(dir, sealed, pinned_for_probe.as_deref()),
    })
}

fn is_project(dir: &Path) -> bool {
    dir.join("renvor.toml").is_file() || dir.join("rust-toolchain.toml").is_file()
}

fn orphaned_staging() -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(".") else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".renvor-staging-"))
        .collect();
    // Sorted so the report is stable between runs and diffable.
    found.sort();
    found
}

/// Runs the command.
///
/// # Errors
///
/// [`crate::exit::Code::ToolMissing`] (exit `5`) when a **required** tool is absent. An optional
/// tool being absent is reported and is not a failure — exiting non-zero for something the
/// operator does not need is how a diagnostic gets wrapped in `|| true`.
pub fn run(reporter: &Reporter) -> Result<Exit, CliError> {
    // Sealed once for every probe, and reported for the current directory (FR-012-14) — the
    // directory whose readiness the operator asked about.
    let sealed = crate::generate::verify::seal(std::env::vars_os());
    let here = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let probes: Vec<Probe> = TOOLS
        .iter()
        .map(|(tool, required, minimum, remedy)| {
            probe(tool, *required, *minimum, remedy, &sealed, &here)
        })
        .collect();

    // MISSING **OR INCOMPATIBLE** (T065). An out-of-date required toolchain is not a warning: it
    // produces a generated project that fails its own pre-placement verification, and the operator
    // is then debugging a template for a `rustup update`.
    if let Some(bad) = probes
        .iter()
        .find(|probe| probe.required && (!probe.found || !probe.compatible))
    {
        let why = if bad.found {
            format!(
                "`{}` is {}, below the required {}",
                bad.tool,
                bad.found_version
                    .clone()
                    .unwrap_or_else(|| "an unknown version".to_owned()),
                bad.required_version.clone().unwrap_or_default()
            )
        } else {
            format!(
                "`{}` is required and was not found or could not be run",
                bad.tool
            )
        };
        return Err(CliError::new(crate::exit::Code::ToolMissing, why)
            .with("tool", bad.tool.clone())
            .with("required", "true")
            .with("found", bad.found.to_string())
            .with(
                "foundVersion",
                bad.found_version.clone().unwrap_or_default(),
            )
            .with(
                "requiredVersion",
                bad.required_version.clone().unwrap_or_default(),
            )
            .with("remedy", bad.remedy.clone().unwrap_or_default()));
    }

    let orphans = orphaned_staging();
    let toolchain = toolchain_section(&here, &sealed);

    // ── THE READINESS TABLE ─────────────────────────────────────────────────────────
    //
    // One row per probe: the tool on the left, what was found on the right, and a state token at
    // the end. The token is the load-bearing part — it is a WORD, so the table is readable with no
    // colour at all, which is the whole reason the palette is decoration rather than information.
    //
    // A remedy is an indented line under its row rather than a fourth column. Remedies are
    // sentences; a sentence in a column either wraps and destroys the alignment or gets truncated
    // and stops being a remedy.
    //
    // `version` is ANOTHER PROGRAM'S output. It used to be escaped here, at the point it was
    // interpolated, because a newline in it ended renvor's row and began one the tool had written
    // — which renvor then presented as its own finding while exiting 0. It is **still** escaped;
    // what changed is where. Every field of a report is escaped strictly on the way out, newline
    // included, so the guarantee now covers every value on this screen rather than the one value
    // somebody remembered to wrap.
    let mut human = Report::new()
        .status(Status::Info, "Environment readiness")
        .blank();

    // ── ONLY TWO OF THE FOUR MARKS ARE REACHABLE TODAY, AND THAT IS WORTH SAYING ────
    //
    // `Mark::Missing` needs a tool that is **required and absent** — and that returns
    // `Code::ToolMissing` above, before this table is built. `Mark::Outdated` needs a tool that is
    // present and **incompatible**, which requires a declared minimum: only `cargo` has one, and
    // an incompatible `cargo` takes the same early return.
    //
    // So a real run emits `OK` and `ABSENT` and nothing else. The other two arms are not dead
    // weight — they are what makes this `match` total, and they become reachable the moment
    // `TOOLS` gains an optional tool with a minimum version — but a reader comparing this code
    // with a screenshot should know which rows the program can actually produce. An advisory
    // review found the contract publishing a `MISSING` example that the binary cannot emit.
    for probe in &probes {
        let (value, mark) = match (probe.version.as_deref(), probe.compatible, probe.required) {
            (Some(version), true, _) => (version.to_owned(), Mark::Ok),
            (Some(version), false, _) => (version.to_owned(), Mark::Outdated),
            (None, _, true) => (
                probe.required_version.as_ref().map_or_else(
                    || "not found".to_owned(),
                    |required| format!(">= {required}"),
                ),
                Mark::Missing,
            ),
            (None, _, false) => ("optional".to_owned(), Mark::Absent),
        };
        human = human.row_marked(probe.tool.clone(), value, mark);

        // The remedy, and — for a tool that is present but too old — what it has to be. Both are
        // the actionable half of the row, so neither is dropped just because the row above is now
        // aligned.
        if mark == Mark::Outdated
            && let Some(required) = &probe.required_version
        {
            human = human.item(format!("needs {required}"));
        }
        if mark != Mark::Ok
            && let Some(remedy) = &probe.remedy
        {
            human = human.item(remedy.clone());
        }
    }

    // ── THE TOOLCHAIN SECTION (FR-012-11) ───────────────────────────────────────────
    //
    // Omitted entirely outside a project. An operator running `doctor` in their home directory
    // asked about their tooling, not about a pin that does not exist there, and a section of
    // *unknown* rows would be noise dressed as information.
    if let Some(section) = &toolchain {
        let unknown = || "unknown".to_owned();
        human = human
            .blank()
            .status(Status::Info, "Toolchain")
            .row(
                "pinned".to_owned(),
                section.pinned.clone().unwrap_or_else(unknown),
            )
            .row(
                "rust-version".to_owned(),
                section.rust_version.clone().unwrap_or_else(unknown),
            )
            .row(
                "verified with".to_owned(),
                section.verified_with.clone().unwrap_or_else(unknown),
            )
            .row(
                "resolves here".to_owned(),
                section.resolved.clone().unwrap_or_else(unknown),
            )
            .row(
                "selected by".to_owned(),
                section.selected_by.clone().unwrap_or_else(unknown),
            )
            .row(
                "rustup".to_owned(),
                section.rustup.clone().map_or_else(
                    || "not found".to_owned(),
                    |version| format!("{version} (floor {})", section.floor),
                ),
            );
        // The components row NEVER goes blank. `probed` lists what answered; the other two
        // states give the operator the reason, because "we did not look" and "we looked and it
        // is missing" call for completely different next actions.
        let components = &section.components;
        human = human.row(
            "pin installed".to_owned(),
            match (components.rustc, components.rustfmt, components.clippy) {
                (Some(rustc), Some(rustfmt), Some(clippy)) => {
                    let mark = |present: bool, name: &str| {
                        if present {
                            name.to_owned()
                        } else {
                            format!("{name} ABSENT")
                        }
                    };
                    format!(
                        "{}, {}, {}",
                        mark(rustc, "rustc"),
                        mark(rustfmt, "rustfmt"),
                        mark(clippy, "clippy")
                    )
                }
                _ => format!(
                    "{}: {}",
                    components.state,
                    components.reason.as_deref().unwrap_or("no reason recorded")
                ),
            },
        );

        if section.proxy {
            human = human.item(
                "the resolved compiler is a rustup proxy, so what it runs depends on the \
                 selection above"
                    .to_owned(),
            );
        }
    }

    if !orphans.is_empty() {
        human = human.blank().status(
            Status::Warn,
            format!(
                "{} orphaned staging {} in this directory, left by a run that was killed",
                orphans.len(),
                if orphans.len() == 1 {
                    "directory"
                } else {
                    "directories"
                }
            ),
        );
        for orphan in &orphans {
            human = human.item(orphan.clone());
        }
        human = human.text(
            "renvor has NOT removed them: one may belong to a `renvor new` running right now. \
             Check the process id in the name, then remove them yourself.",
        );
    }

    Ok(reporter.finish(
        "doctor",
        &human,
        serde_json::json!({
            "probes": probes,
            "orphanedStaging": orphans,
            "toolchain": toolchain,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The environment a unit test hands [`probe`]: whatever the caller lists, sealed.
    fn sealed(variables: &[(&str, &str)]) -> crate::generate::verify::Sealed {
        crate::generate::verify::seal(variables.iter().map(|(name, value)| {
            (
                std::ffi::OsString::from(*name),
                std::ffi::OsString::from(*value),
            )
        }))
    }

    /// The environment a probe of a real tool needs: this process's own, sealed.
    fn inherited() -> crate::generate::verify::Sealed {
        crate::generate::verify::seal(std::env::vars_os())
    }

    /// The directory a unit test's probe runs in.
    fn here() -> std::path::PathBuf {
        std::env::current_dir().expect("a working directory")
    }

    /// The components row states WHY it did not probe rather than going blank.
    ///
    /// Three states that must stay distinguishable. `not applicable` means `+channel` selects
    /// nothing here — there is no question to answer. `not probed` means renvor declined to run
    /// the probes and says so. `probed` is the only one carrying a measurement. Collapsing them
    /// into a blank row would leave the operator guessing which of three very different
    /// situations they are in, and only one of them calls for `rustup component add`.
    #[test]
    fn the_components_row_says_why_it_did_not_probe_rather_than_going_blank() {
        // A project with no pin: nothing to probe, and that is not a failure.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("renvor.toml"), "").expect("write");

        let section = super::toolchain_section(dir.path(), &inherited()).expect("a project");
        assert_eq!(section.components.state, "not applicable");
        assert!(
            section
                .components
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("pins no channel")),
            "an unpinned project is told why, not shown a blank: {:?}",
            section.components.reason
        );

        // Whatever the state, a report that names no measurement must carry a reason, and one
        // that names a measurement must not pretend to a reason it does not have.
        let measured = section.components.rustc.is_some();
        assert_ne!(
            measured,
            section.components.reason.is_some(),
            "exactly one of `a measurement` and `a reason` is present, never both or neither"
        );
    }

    /// Outside a project the section is omitted entirely, and the JSON carries `null`.
    ///
    /// An operator running `doctor` in their home directory asked about their tooling. A
    /// toolchain section there would be six rows of *unknown* — noise that reads like a finding.
    /// FR-012-11 omits it, and `data.doctor.toolchain` is `null` rather than an empty object,
    /// so a consumer can tell "no project here" from "a project that told us nothing".
    #[test]
    fn doctor_outside_a_project_omits_the_section() {
        let empty = tempfile::tempdir().expect("tempdir");
        assert!(
            super::toolchain_section(empty.path(), &inherited()).is_none(),
            "a directory with neither `renvor.toml` nor `rust-toolchain.toml` is not a project"
        );

        // Either file alone IS a project: a tree generated before template version 8 has the
        // first and not the second, and a hand-written crate may have the second and not the
        // first. Requiring both would omit the section exactly where it is most wanted.
        for marker in ["renvor.toml", "rust-toolchain.toml"] {
            let dir = tempfile::tempdir().expect("tempdir");
            std::fs::write(dir.path().join(marker), "").expect("write");
            assert!(
                super::toolchain_section(dir.path(), &inherited()).is_some(),
                "`{marker}` alone marks a project"
            );
        }
    }

    /// The section reports the pin it READ, and reports nothing it could not read.
    ///
    /// The pin is read from the file rather than inferred from what resolved: those two differ
    /// exactly when the operator most needs to see both — an override, a stale environment
    /// variable, a pin that is not installed.
    #[test]
    fn the_section_reports_the_pin_it_read_and_leaves_the_rest_unknown() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.94.0\"\ncomponents = [\"rustfmt\", \"clippy\"]\n",
        )
        .expect("write");

        let section = super::toolchain_section(dir.path(), &inherited()).expect("a project");
        assert_eq!(section.pinned.as_deref(), Some("1.94.0"));

        // No manifest and no record in this tree, so both stay `None` — never filled in from the
        // pin beside them. A record that says a compiler verified this tree is a measurement;
        // copying the pin into that field would manufacture one.
        assert_eq!(section.rust_version, None, "no manifest was read");
        assert_eq!(section.verified_with, None, "no record was read");
        assert_eq!(section.floor, crate::toolchain::RUSTUP_FLOOR);
    }

    /// FR-012-6 names `doctor`'s probes in its list of sealed children, and this is the assertion
    /// that says so — by handing a probe an environment carrying the two install-server variables
    /// and `RUSTUP_AUTO_INSTALL=1`, and reading back what the child actually received.
    ///
    /// The shim prints those three names to stdout, which is the line `Probe::version` keeps
    /// verbatim. Sealed, they must read `0`, absent, and absent.
    #[test]
    #[cfg(unix)]
    fn doctor_probes_run_under_the_seal() {
        use std::os::unix::fs::PermissionsExt as _;

        let base = tempfile::tempdir().expect("tempdir");
        let bin = base.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        let shim = bin.join("renvor-seal-probe");
        std::fs::write(
            &shim,
            "#!/bin/sh\nprintf 'auto=%s dist=%s update=%s\\n' \
             \"${RUSTUP_AUTO_INSTALL-unset}\" \"${RUSTUP_DIST_SERVER-unset}\" \
             \"${RUSTUP_UPDATE_ROOT-unset}\"\n",
        )
        .expect("write");
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let path = bin.display().to_string();
        let probed = probe(
            "renvor-seal-probe",
            false,
            None,
            "unreachable",
            &sealed(&[
                ("PATH", path.as_str()),
                ("RUSTUP_AUTO_INSTALL", "1"),
                ("RUSTUP_DIST_SERVER", "http://example.invalid"),
                ("RUSTUP_UPDATE_ROOT", "http://example.invalid"),
            ]),
            &here(),
        );

        assert!(probed.found, "the shim did not run");
        assert_eq!(
            probed.version.as_deref(),
            Some("auto=0 dist=unset update=unset"),
            "a doctor probe did not run under the seal"
        );
    }

    /// The negative control for the assertion above: the same shim, run with the three variables
    /// simply passed through, reports them — so the sealed reading is the seal's doing and not
    /// the shim's.
    #[test]
    #[cfg(unix)]
    fn the_seal_is_what_changes_those_three_values() {
        use std::os::unix::fs::PermissionsExt as _;

        let base = tempfile::tempdir().expect("tempdir");
        let bin = base.path().join("bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        let shim = bin.join("renvor-seal-probe");
        std::fs::write(
            &shim,
            "#!/bin/sh\nprintf 'auto=%s dist=%s update=%s\\n' \
             \"${RUSTUP_AUTO_INSTALL-unset}\" \"${RUSTUP_DIST_SERVER-unset}\" \
             \"${RUSTUP_UPDATE_ROOT-unset}\"\n",
        )
        .expect("write");
        std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let output = std::process::Command::new(&shim)
            .env_clear()
            .env("RUSTUP_AUTO_INSTALL", "1")
            .env("RUSTUP_DIST_SERVER", "http://example.invalid")
            .env("RUSTUP_UPDATE_ROOT", "http://example.invalid")
            .output()
            .expect("the shim runs");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "auto=1 dist=http://example.invalid update=http://example.invalid",
            "the shim does not report the three variables it was given"
        );
    }

    #[test]
    fn a_probe_for_something_that_does_not_exist_reports_absent_with_a_remedy() {
        let probe = probe(
            "renvor-definitely-not-a-real-executable",
            false,
            None,
            "do the thing",
            &inherited(),
            &here(),
        );
        assert!(!probe.found);
        assert!(probe.version.is_none());
        assert_eq!(probe.remedy.as_deref(), Some("do the thing"));
    }

    #[test]
    fn a_probe_for_something_that_does_exist_reports_its_version() {
        // POSITIVE CONTROL. `cargo` is present wherever this test runs, by construction.
        let probe = probe("cargo", true, None, "install Rust", &inherited(), &here());
        assert!(probe.found, "cargo must be runnable in a cargo test");
        assert!(probe.version.is_some_and(|v| v.contains("cargo")));
    }

    #[test]
    fn an_optional_tool_being_absent_is_not_a_required_failure() {
        // The rule that keeps `doctor` runnable in a container without docker.
        let optional: Vec<_> = TOOLS
            .iter()
            .filter(|(_, required, _, _)| !*required)
            .collect();
        assert!(!optional.is_empty(), "at least one tool must be optional");
    }

    #[test]
    fn every_tool_carries_a_remedy_the_operator_can_run_themselves() {
        // FR-036's principle: report, never install. A remedy with no text is a dead end.
        for (tool, _, _, remedy) in TOOLS {
            assert!(!remedy.is_empty(), "{tool} has no remedy text");
        }
    }

    // ── T065: required version, found version, corrective action ────────────────────────

    #[test]
    fn a_version_is_extracted_from_every_shape_a_real_tool_prints() {
        // Measured against the actual output of the three tools this command probes, rather than
        // against an idealised `x.y.z`. Each of these disagrees about punctuation.
        for (line, expected) in [
            ("cargo 1.94.0 (a1b2c3d4e 2026-01-05)", "1.94.0"),
            ("git version 2.39.5", "2.39.5"),
            ("Docker version 27.0.3, build 7d4bcd8", "27.0.3"),
            (
                "cargo 1.95.0-nightly (deadbeef 2026-02-01)",
                "1.95.0-nightly",
            ),
            // Two components padded to three, which some tools print.
            ("something 1.94", "1.94.0"),
        ] {
            assert_eq!(
                extract_version(line).map(|v| v.to_string()).as_deref(),
                Some(expected),
                "failed on {line:?}"
            );
        }
    }

    #[test]
    fn the_compatibility_rule_is_stated_over_all_six_combinations() {
        // The rule itself, with no disjunct able to carry the assertion. An inverted implementation
        // fails these rows; the test it replaces passed under inversion.
        let low = semver::Version::parse("1.0.0").expect("parses");
        let high = semver::Version::parse("2.0.0").expect("parses");

        assert!(
            compatible(Some(&low), Some(&high)),
            "above the floor is compatible"
        );
        assert!(
            compatible(Some(&low), Some(&low)),
            "exactly the floor is compatible"
        );
        assert!(
            !compatible(Some(&high), Some(&low)),
            "below the floor is NOT compatible"
        );

        // THE ASYMMETRIC ROW, and the one no real tool could produce here: the only tool with a
        // declared minimum is cargo, whose version always parses. Unreachable through `probe`,
        // reachable directly, and the rule the doc comment is actually about.
        assert!(
            compatible(Some(&high), None),
            "an unreadable version is UNKNOWN, not too old — a diagnostic must not break the \
             environment it describes"
        );
        assert!(
            compatible(None, Some(&low)),
            "no declared minimum is always compatible"
        );
        assert!(compatible(None, None), "neither known is compatible");
    }

    #[test]
    fn a_version_that_cannot_be_parsed_is_not_reported_as_incompatible() {
        // The rule that keeps this diagnostic from breaking the environment it describes. A tool
        // that prints something this parser does not understand is unknown, not too old.
        assert!(extract_version("some-tool built from source").is_none());
        let probe = probe(
            "cargo",
            true,
            Some("1.94.0"),
            "update",
            &inherited(),
            &here(),
        );
        assert!(
            probe.compatible || probe.found_version.is_some(),
            "{probe:?}"
        );
    }

    #[test]
    fn a_tool_below_its_required_version_is_incompatible_and_carries_a_remedy() {
        // The case T065 exists for, and the one that cannot be produced by running a real tool:
        // `cargo` on this machine is not going to be old. Driven through `probe` with an
        // impossible floor instead, so the comparison itself is what is under test.
        let probe = probe(
            "cargo",
            true,
            Some("999.0.0"),
            "install Rust from https://rustup.rs",
            &inherited(),
            &here(),
        );
        assert!(probe.found, "cargo must be runnable");
        assert!(
            !probe.compatible,
            "cargo cannot satisfy a 999.0.0 floor: {probe:?}"
        );
        assert_eq!(probe.required_version.as_deref(), Some("999.0.0"));
        assert!(
            probe.found_version.is_some(),
            "the found version must be reported too"
        );
        assert!(
            probe
                .remedy
                .as_deref()
                .is_some_and(|remedy| !remedy.is_empty()),
            "an incompatible tool needs a remedy as much as a missing one: {probe:?}"
        );
    }

    #[test]
    fn a_tool_at_exactly_its_required_version_is_compatible() {
        // The boundary. `>=` rather than `>`, asserted rather than assumed — an off-by-one here
        // rejects precisely the toolchain the project declares as its minimum.
        let exact = semver::Version::parse(REQUIRED_RUST).expect("the declared MSRV is a version");
        assert!(exact >= semver::Version::parse(REQUIRED_RUST).expect("parses"));
        let probe = probe(
            "cargo",
            true,
            Some(REQUIRED_RUST),
            "update",
            &inherited(),
            &here(),
        );
        assert!(
            probe.compatible,
            "the toolchain running these tests is below the declared MSRV: {probe:?}"
        );
    }

    #[test]
    fn the_required_rust_version_comes_from_the_manifest_rather_than_a_second_declaration() {
        // ADR-0002: `rust-version` is declared once, in `[workspace.package]`. This asserts the
        // constant is that value rather than a copy that can drift from it.
        assert_eq!(REQUIRED_RUST, env!("CARGO_PKG_RUST_VERSION"));
        assert!(
            semver::Version::parse(REQUIRED_RUST).is_ok(),
            "the declared MSRV must be a comparable version, not a range"
        );
    }

    #[test]
    fn every_declared_minimum_is_a_parseable_version() {
        // A floor that cannot be parsed silently disables the comparison it was added for, because
        // `compatible` falls back to true. That would be a bound nobody enabled.
        for (tool, _, minimum, _) in TOOLS {
            if let Some(minimum) = minimum {
                assert!(
                    semver::Version::parse(minimum).is_ok(),
                    "{tool}'s minimum {minimum:?} does not parse, so it is not enforced"
                );
            }
        }
    }
}
