//! The evidence of FR-012-7d: what Cargo said it launched for the project's own units, and what
//! the launched executables answered when asked who they were — *launch observation plus queried
//! identity*, kept apart and never mistaken for proof of execution (SR-012-3).
//!
//! # Where the evidence comes from
//!
//! The three checks that can launch a compiler — `cargo clippy --all-targets -vv -- -D warnings`,
//! `cargo build -vv`, `cargo test -vv` — run exactly as before with one output flag added. At
//! `-vv` Cargo prints one stderr line per event (measured 2026-09-07 on 1.94.0 and 1.97.1;
//! `governance/phase-012-specification-and-decision-brief.md` FR-012-7d):
//!
//! ```text
//!    Compiling tiny-probe v0.1.0 (/abs/path)                 once per package with a dirty unit
//!     Checking tiny-probe v0.1.0 (/abs/path)                 the same, in check mode
//!      Running `CARGO=… CARGO_PKG_NAME=tiny-probe … /t/bin/rustc --crate-name tiny_probe …`
//!        Fresh tiny-probe v0.1.0 (/abs/path)                 every unit of the package reused
//!     Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.31s
//! ```
//!
//! A **unit** is a `Running` line whose command carries `--crate-name`: a compiler launch. Cargo
//! also prints backticked `Running` lines for a build script it executes and, under `cargo test`,
//! for each test binary it runs; neither carries `--crate-name` and neither is a compiler, so
//! neither is a unit. `Compiling` and `Checking` are per package, not per unit — `cargo test -vv`
//! on a bin package prints one `Compiling` and two `Running` lines — so units are counted from
//! `Running` lines, never from the announcement. `Fresh` is printed once per package, and only
//! when every unit of it was reused.
//!
//! # What is kept, and what is not
//!
//! Each `Running` command is split into tokens — with `shell_words` for the Unix shape, where
//! values are single-quoted (`DYLD_FALLBACK_LIBRARY_PATH='…'`), or with the argument rules Cargo
//! escapes with on Windows (`set NAME=value&& …`, double quotes). The leading environment tokens
//! are read to decide whether the unit is the project's own and then **discarded** — no name and
//! no value, whatever it carried: a `RUSTFLAGS`-derived argument, a `CARGO_PKG_*` value, the
//! manifest's `repository`. The **chain** is the run of executables before the first `-`
//! argument: `[rustc]` for a plain build, `[clippy-driver, rustc]` for clippy, one more in front
//! when a wrapper is configured. The chain stays in memory for the identity queries and enters
//! no record, no JSON, and no stream; [`Chain`]'s `Debug` prints a count and nothing else.
//!
//! # Capture failure is not caching (FR-012-7d (e))
//!
//! A stream without a `Finished` line is truncated; a `Compiling`/`Checking` of the own package
//! with no own `Running` line after it is unaccounted; a `Running` command that does not tokenise
//! is malformed; a stream in which the own package never appears is unaccounted; a stream that
//! reports the own package both `Fresh` and **announced** is contradictory. Each is an
//! [`EvidenceError`], which the caller reports as `project_verification_failed` with
//! `reason = evidence_capture_failed` — never as a cached run.
//!
//! # What this accounting cannot establish, and why
//!
//! Contract C-5 1.2.0 says *"every unit of the project's own package(s) must be accounted for by
//! a `Running` line or a positive `Fresh` report; a missing line … is `evidence_capture_failed`"*.
//! **Cargo's `-vv` output cannot support that sentence, and the ordinary partially-fresh run is
//! already outside it.** Measured 2026-09-08 on cargo 1.97.1, for `build`, `test`, and
//! `clippy --all-targets` alike: a package with one dirty unit and one reused unit prints
//! `Compiling`/`Checking`, the dirty unit's `Running` line, and **nothing whatsoever** for the
//! reused unit. `Fresh <pkg>` is all-or-nothing — it appears only when every unit of the package
//! was reused, and never in the same stream as that package's **announcement** — and no line
//! anywhere carries a unit count or a total. That exclusivity is per package **identity** (name,
//! version, source), so a status line counts as the project's own only when its parenthetical is
//! the directory the check ran in: a dependency sharing the project's name is a different package
//! and may be `Fresh` beside the project's own `Compiling`.
//!
//! `Fresh` beside a *launch* is a different matter and is **legitimate**: a fully cached
//! `cargo test -vv` on a crate with a doctest prints `Fresh` and then a `Running` line for the
//! rustdoc unit, which carries `CARGO_PKG_NAME` and `--crate-name` and so is the project's own by
//! every rule here (measured the same day on the same cargo, on a lib+bin crate with one
//! doctest). The announcement is `Fresh`'s complement; the launch count is not.
//!
//! Two consequences, both stated rather than worked around:
//!
//! 1. A reused unit in a partially-fresh package is accounted for by neither a `Running` line nor
//!    a `Fresh` report. The contract's sentence describes a state Cargo does not report.
//! 2. Removing one of several own `Running` lines produces a stream that is, line for line, the
//!    shape of a legitimate partial rebuild. No parser of this output can tell them apart.
//!
//! So this module establishes the strongest accounting the evidence *does* support — an announced
//! package must launch, a package that appears nowhere is refused, a truncated or malformed stream
//! is refused, and a package reported both reused and **announced** is refused — and claims nothing
//! beyond it. The control that measures the limit rather than asserting past it is
//! `one_of_several_own_launches_may_go_missing_and_this_names_the_limit`. **The contract text is
//! not narrowed here**: whether C-5's sentence is corrected, or the guarantee is bought with
//! evidence Cargo's output does not carry, is a maintainer's decision and is reported as one.
//!
//! # The queries
//!
//! The last executable of a build or test chain is run once with `-vV`, and the `clippy-driver`
//! a clippy chain names is run once with `--version` — under the seal, in the staging directory,
//! with a bounded wait — and the answer is parsed by [`super::grammar`] before anything is kept.
//! An answer outside the grammar is **discarded, not echoed**: it is unparsed text from an
//! executable the environment configured, and the seal's redaction cannot know what it carries.
//! `cargo clippy --version` never fills the driver identity (§4.3.2): the executable Cargo's own
//! `Running` line names is what is asked.
//!
//! Nothing here reads a file: not `.rustc_info.json`, not a fingerprint, not an earlier record.
//! The evidence of a check is the stderr of that check and the answers of that run's queries.

use std::borrow::Cow;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

use super::grammar::{self, GrammarError};
use super::{DriverIdentity, Identity, Observation};
use crate::exit::{CliError, Code};
use crate::generate::verify::{Sealed, sealed_command};

/// `details.reason` when a check's stream cannot be accounted for (FR-012-7d (e)).
pub const REASON_CAPTURE_FAILED: &str = "evidence_capture_failed";

/// `details.reason` when a version query is answered outside FR-012-7e's grammar, not at all, or
/// not in time.
pub const REASON_IDENTITY_UNREADABLE: &str = "compiler_identity_unreadable";

/// The bounded wait for one identity query. `rustc -vV` answers in milliseconds; the bound exists
/// for a wrapper or a shim that does not.
pub const QUERY_TIMEOUT: Duration = Duration::from_secs(60);

/// The most of a query's stdout that is read. `rustc -vV` is under three hundred bytes; an
/// answer past this bound is not an identity and is refused as one.
const MAX_ANSWER_BYTES: u64 = 16 * 1024;

/// The executables of one launched unit, in the order Cargo's `Running` line named them: the
/// wrapper if any, `clippy-driver` for a clippy unit, then the compiler argument.
///
/// Held in memory for the identity queries only. Its `Debug` prints a count: a path here is a
/// wrapper's location or the toolchain's, and neither may reach an error through a `{:?}`.
#[derive(Clone, PartialEq, Eq)]
pub struct Chain {
    executables: Vec<PathBuf>,
}

impl Chain {
    /// How many executables the chain names.
    #[must_use]
    pub fn len(&self) -> usize {
        self.executables.len()
    }

    /// Whether the chain names nothing — never true for a chain [`parse_check`] returned.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.executables.is_empty()
    }
}

impl fmt::Debug for Chain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Chain {{ executables: {} }}",
            self.executables.len()
        )
    }
}

/// What one check's `-vv` stream said about the project's own package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckEvidence {
    /// Own units Cargo launched a compiler for — one per `Running` line with `--crate-name`.
    pub units_launched: u32,
    /// `Fresh` reports for the own package — Cargo prints one when every unit of it was reused.
    pub units_fresh: u32,
    /// The chain of every launched own unit, in stream order.
    pub chains: Vec<Chain>,
}

/// Why a check's stream is not evidence. Each is reported as `evidence_capture_failed`; none is
/// ever reported as a cached run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceError {
    /// No `Finished` line: the stream ended before Cargo did.
    Truncated,
    /// An own `Compiling`/`Checking` with no own `Running` after it, or an own package that
    /// appears nowhere — a unit neither launched nor positively `Fresh`.
    Unaccounted,
    /// A `Running` command that does not tokenise, or one with no executable before its first
    /// argument, or a backtick that never closes.
    Malformed,
    /// A launched clippy unit whose chain names no `clippy-driver`: there is no executable to
    /// ask, and the launcher's answer is not a substitute (§4.3.2).
    NoDriver,
    /// The distinct executables a check's launched units ended in answered with different
    /// identities. One identity per check is representable; a disagreement is not, and picking
    /// one would be a claim about units it was not queried for.
    Disagreement,
    /// One check's stream reports the own package **both** `Fresh` and **announced**. Cargo emits
    /// `Fresh <pkg>` only when every unit of that package was reused, and announces
    /// `Compiling`/`Checking <pkg>` only when at least one was not — so those two never appear for
    /// one package in one stream (measured 2026-09-08 on cargo 1.97.1, for `build`, `test`, and
    /// `clippy --all-targets`, including the partially-fresh case of each). A stream that shows
    /// both is not a stream this parser can account for, and calling it `mixed` would report a
    /// state Cargo never described.
    ///
    /// # `Fresh` beside a *launch* is NOT this, and the distinction is measured
    ///
    /// A fully cached `cargo test -vv` on a crate with a doctest prints `Fresh` and then a
    /// `Running` line for the rustdoc unit, which carries `CARGO_PKG_NAME` and `--crate-name` and
    /// so counts as the project's own. That is an ordinary, legitimate stream. The refusal is
    /// therefore keyed to the ANNOUNCEMENT, which is `Fresh`'s true complement, and never to the
    /// launch count.
    Contradictory,
}

impl EvidenceError {
    /// The wire name for `details.cause`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Truncated => "truncated",
            Self::Unaccounted => "unaccounted",
            Self::Malformed => "malformed",
            Self::NoDriver => "no_driver",
            Self::Disagreement => "disagreement",
            Self::Contradictory => "contradictory",
        }
    }
}

impl fmt::Display for EvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Truncated => "the check's `-vv` stream has no `Finished` line",
            Self::Unaccounted => {
                "a unit of the project's own package is accounted for by neither a `Running` \
                 line nor a `Fresh` report"
            }
            Self::Malformed => "a `Running` line of the check's `-vv` stream does not tokenise",
            Self::NoDriver => "a launched clippy unit names no `clippy-driver` to ask",
            Self::Disagreement => {
                "the executables the check's units were launched with answered with different \
                 identities, and one identity per check is all the record can represent"
            }
            Self::Contradictory => {
                "one check reports the project's own package both reused and launched, which is \
                 not a state Cargo describes"
            }
        })
    }
}

impl std::error::Error for EvidenceError {}

/// Per-unit evidence of a build or test check, as the record keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitEvidence {
    /// Own units launched.
    pub units_launched: u32,
    /// Own `Fresh` reports.
    pub units_fresh: u32,
}

impl From<&CheckEvidence> for UnitEvidence {
    fn from(evidence: &CheckEvidence) -> Self {
        Self {
            units_launched: evidence.units_launched,
            units_fresh: evidence.units_fresh,
        }
    }
}

/// The clippy check's evidence: its counts, and the observed driver's own answer when a unit
/// was launched — `None` when every unit was `Fresh`, never filled from `cargo clippy --version`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClippyEvidence {
    /// Own units launched.
    pub units_launched: u32,
    /// Own `Fresh` reports.
    pub units_fresh: u32,
    /// What the `clippy-driver` executable the chain named answered to `--version`.
    pub driver: Option<DriverIdentity>,
}

/// What pre-placement verification established (FR-012-7d): every check passed, and what was
/// observed and queried while it did. A failed check is an error, never a `Verified` with a
/// `false` in it — the two outcome booleans exist so the record can say `passed` from a value
/// rather than from an assumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verified {
    /// `cargo fmt --check` passed. It launches no compiler and has no counts.
    pub fmt: bool,
    /// The clippy check.
    pub clippy: ClippyEvidence,
    /// The build check.
    pub build: UnitEvidence,
    /// The test check.
    pub test: UnitEvidence,
    /// The smoke run passed. `cargo run --quiet` takes no `-vv` and launches nothing after
    /// `build`.
    pub run: bool,
    /// Whether the build and test units were launched, reused, or both — the source of `rustc`.
    pub observation: Observation,
    /// The queried identity of the compiler the build/test chains ended in; `None` when no unit
    /// was launched, and never filled from anywhere else.
    pub rustc: Option<Identity>,
    /// The checks whose own units were all `Fresh`, for FR-012-7d (d)'s stderr line.
    pub cached_checks: Vec<&'static str>,
    /// A launched chain named more executables than `[rustc]` or `[clippy-driver, rustc]`.
    pub wrapper_observed: bool,
}

/// Parses one check's captured stderr into the evidence of the project's own package.
///
/// `own_package` is the manifest's `[package].name`; `staging` is the directory the check ran in.
/// A unit is the project's own when its environment tokens carried `CARGO_PKG_NAME=<own>` or
/// `CARGO_MANIFEST_DIR=<staging>` (compared canonically), or — when neither token is present, as
/// in a stub — its `--crate-name` is the package name with hyphens as underscores.
///
/// # Errors
///
/// [`EvidenceError`]: truncated, unaccounted, or malformed, as the type documents.
pub fn parse_check(
    stderr: &str,
    own_package: &str,
    staging: &Path,
) -> Result<CheckEvidence, EvidenceError> {
    let staging = Location::of(staging);
    let own_crate = own_package.replace('-', "_");
    let mut evidence = CheckEvidence {
        units_launched: 0,
        units_fresh: 0,
        chains: Vec::new(),
    };
    let mut finished = false;
    let mut announced = false;
    // Whether the own package was EVER announced, as distinct from `announced`, which a launch
    // discharges. The contradiction check below is about the announcement's existence.
    let mut announced_ever = false;
    // SPLIT ON `'\n'`, NOT `str::lines`. `lines` treats `\r\n` as one terminator and drops the
    // `\r`. Every path in this stream comes from the operator's filesystem, a `\r` is legal in a
    // Unix directory name, and Cargo prints the path raw — so a `\r` here is DATA, and `lines`
    // silently deleted it. Deleting it moved the location off the staging path, which lost the
    // announcement on a cold run and refused the same directory as `Unaccounted` on a cached one.
    //
    // This is not a second delimiter heuristic: it is the removal of one. `'\n'` is the only
    // terminator Cargo writes, and whitespace that is genuinely Cargo's — anything after the
    // closing backtick or the closing bracket — is discarded by `whole`, `backticked` and
    // `location_of`, each of which trims its OWN end before testing for its delimiter. That keeps
    // a trailing `\r` from a `\r\n`-terminated stream harmless without treating a `\r` inside a
    // path as a terminator.
    let mut lines = stderr.split('\n');
    while let Some(raw) = lines.next() {
        // THE HEAD IS CARGO'S, THE REST IS THE OPERATOR'S. Colour and indentation are removed
        // from the head only, and the remainder is carried RAW — the discipline
        // [`without_launch_lines`] already follows. Stripping SGR from the WHOLE line deleted any
        // escape byte that was part of a directory name, and `trim` deleted trailing spaces that
        // were part of one; both then failed to equal the staging path. `tests/redaction.rs`
        // creates exactly such a directory, so this is a shape the project already promises to
        // survive rather than a hypothesis.
        let Some((word, rest)) = status_and_rest(raw) else {
            continue;
        };
        if word == "Running" {
            // A DIRECTORY NAME MAY LEGALLY CONTAIN A NEWLINE, and every path Cargo puts in this
            // line comes from the operator's filesystem: `CARGO_MANIFEST_DIR`, the compiler, the
            // output directory. One `Running` command then arrives as several physical lines,
            // and reading only the first finds a backtick that never closes. Rejoining is the
            // only reading that keeps the path intact — trimming or dropping the continuation
            // would hand `shell_words` a command with a hole in it.
            //
            // The continuations are NOT trimmed and NOT stripped of colour: they are the middle
            // of a quoted argument, where a space is data. Only the first line carried Cargo's
            // own indentation.
            let rejoined = rejoin(rest, &mut lines);
            let rest = rejoined.as_deref().unwrap_or(rest);
            let Some(command) = backticked(rest)? else {
                continue;
            };
            let tokens = tokenize(command).ok_or(EvidenceError::Malformed)?;
            if let Some(unit) = launch(&tokens, own_package, &own_crate, &staging)?
                && unit.own
            {
                evidence.units_launched += 1;
                evidence.chains.push(unit.chain);
                announced = false;
            }
        } else if word == "Fresh" {
            // A path may carry a newline, so a status line may be two physical lines — the same
            // cause `rejoin` handles for a launch command.
            let rejoined = rejoin_status(rest, own_package, &staging, &mut lines);
            let rest = rejoined.as_deref().unwrap_or(rest);
            if own_status_line(rest, own_package, &staging) {
                evidence.units_fresh += 1;
            }
        } else if word == "Compiling" || word == "Checking" {
            let rejoined = rejoin_status(rest, own_package, &staging, &mut lines);
            let rest = rejoined.as_deref().unwrap_or(rest);
            if own_status_line(rest, own_package, &staging) {
                announced = true;
                announced_ever = true;
            }
        } else if word == "Finished" {
            finished = true;
        }
    }
    if !finished {
        return Err(EvidenceError::Truncated);
    }
    if announced || (evidence.units_launched == 0 && evidence.units_fresh == 0) {
        return Err(EvidenceError::Unaccounted);
    }
    // `Fresh` is all-or-nothing per package and the ANNOUNCEMENT is its complement: a stream says
    // one of those two about the own package, never both — where "the own package" is the one at
    // the staging directory, which is what `own_status_line` enforces. Cargo's exclusivity is per
    // package IDENTITY, so a dependency that merely shares the name is a different package and may
    // legitimately be `Fresh` in the same stream.
    //
    // THE PREDICATE IS THE ANNOUNCEMENT, NOT THE LAUNCH COUNT, and the difference is a state
    // Cargo really emits. `cargo test -vv` on a fully cached crate with a doctest prints `Fresh`
    // AND a `Running` line — the rustdoc unit, which carries `CARGO_PKG_NAME` and `--crate-name`
    // and is therefore the project's own by every rule above (measured 2026-09-08, cargo 1.97.1,
    // a lib+bin crate with one doctest). A first version of this check compared the launch count
    // and would have refused that run. The measurement that first version rested on was taken on
    // a crate with no doc comments, so the shape never appeared.
    //
    // See [`EvidenceError::Contradictory`], and the module header for what this accounting can
    // and cannot establish.
    if announced_ever && evidence.units_fresh > 0 {
        return Err(EvidenceError::Contradictory);
    }
    Ok(evidence)
}

/// The observation over the build and test checks (FR-012-7d (d), (f)): every own unit launched
/// is `launched`, every one positively `Fresh` is `cached`, both is `mixed`.
///
/// # Errors
///
/// [`EvidenceError::Unaccounted`] when neither check launched nor reused a unit — a state
/// [`parse_check`] never returns, refused again here rather than called cached.
pub fn observation(
    build: &CheckEvidence,
    test: &CheckEvidence,
) -> Result<Observation, EvidenceError> {
    let launched = build.units_launched + test.units_launched;
    let fresh = build.units_fresh + test.units_fresh;
    match (launched, fresh) {
        (0, 0) => Err(EvidenceError::Unaccounted),
        (_, 0) => Ok(Observation::Launched),
        (0, _) => Ok(Observation::Cached),
        _ => Ok(Observation::Mixed),
    }
}

/// The last executable of a chain — the compiler argument of a build or test unit, the one
/// queried with `-vV`. A wrapper or `clippy-driver` in front of it is not the compiler.
#[must_use]
pub fn trailing_compiler(chain: &Chain) -> Option<&Path> {
    chain.executables.last().map(PathBuf::as_path)
}

/// The `clippy-driver` executable a chain names, wherever a wrapper put it — the one queried with
/// `--version`. Its trailing `rustc` is **not** clippy's executing compiler (FR-012-7d (b)).
#[must_use]
pub fn clippy_driver(chain: &Chain) -> Option<&Path> {
    chain
        .executables
        .iter()
        .find(|executable| {
            executable
                .file_name()
                .is_some_and(|name| name == "clippy-driver" || name == "clippy-driver.exe")
        })
        .map(PathBuf::as_path)
}

/// Whether a chain names more executables than the bare shape — `[rustc]` for a build or test
/// unit, `[clippy-driver, rustc]` for a clippy unit — which is a wrapper in front.
#[must_use]
pub fn shows_wrapper(chain: &Chain, is_clippy: bool) -> bool {
    chain.len() > if is_clippy { 2 } else { 1 }
}

/// A check's stderr without its `Running` launch commands, for a failure message.
///
/// A launch command is Cargo's rendering of a whole invocation — every environment variable it
/// set, every `RUSTFLAGS`-derived argument, every path — and a failing check's message embeds the
/// check's output. The diagnostics stay; the launch commands, which exist for [`parse_check`] and
/// say nothing about why the check failed, do not reach a stream.
///
/// # A launch command is not a line
///
/// It is a **logical** command that may span several physical lines, because every path in it
/// comes from the operator's filesystem and a directory name may legally contain a newline —
/// and because a value may carry a backtick of its own, so the first line that *ends* with one
/// has not necessarily ended the command (see `rejoin`). Dropping the first physical line and
/// keeping the rest published the continuations: the middle of a quoted argument, which is
/// precisely the part a long value's newline puts there. This reads the stream with the **same**
/// `rejoin` [`parse_check`] uses, so the two readings of one launch cannot disagree about
/// where it ended.
///
/// An unterminated or truncated command — one still open after `MAX_REJOINED_LINES`, or one the
/// stream ends inside — is dropped whole rather than partly kept. That is the safe direction: a
/// fragment this parser cannot account for is a fragment it cannot say is free of arguments, and
/// [`parse_check`] refuses the same stream as `Malformed` anyway.
#[must_use]
pub fn without_launch_lines(stderr: &str) -> String {
    let mut kept = String::with_capacity(stderr.len());
    let mut lines = stderr.lines();
    while let Some(raw) = lines.next() {
        let line = without_sgr(raw);
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Running ")
            && rest.starts_with('`')
        {
            // Consumes this line and every continuation of it from the iterator; the joined text
            // is deliberately unused — it is the thing being removed.
            let _ = rejoin(rest, &mut lines);
            continue;
        }
        kept.push_str(raw);
        kept.push('\n');
    }
    kept
}

/// Runs `<compiler> -vV` under the seal in `cwd`, bounded, and parses the answer under
/// FR-012-7e's grammar.
///
/// # Errors
///
/// [`Code::ProjectVerificationFailed`] with `reason = compiler_identity_unreadable` when the
/// executable cannot be started, does not exit in time, exits non-zero, answers past the bound,
/// or answers outside the grammar. The answer is never part of the error.
pub fn query_rustc(compiler: &Path, sealed: &Sealed, cwd: &Path) -> Result<Identity, CliError> {
    const QUERY: &str = "rustc -vV";
    let mut command = sealed_command(compiler.as_os_str(), sealed, cwd);
    command.arg("-vV");
    let text = answer(&mut command, QUERY_TIMEOUT).map_err(|failure| unreadable(QUERY, failure))?;
    grammar::parse_rustc_vv(&text).map_err(|error| unreadable(QUERY, QueryFailure::Grammar(error)))
}

/// Runs `<clippy-driver> --version` under the seal in `cwd`, bounded, and parses the one line it
/// prints (`clippy 0.1.94 (4a4ef493e3 2026-03-02)`, measured) under the grammar.
///
/// # Errors
///
/// As [`query_rustc`].
pub fn query_driver(
    driver: &Path,
    sealed: &Sealed,
    cwd: &Path,
) -> Result<DriverIdentity, CliError> {
    const QUERY: &str = "clippy-driver --version";
    let mut command = sealed_command(driver.as_os_str(), sealed, cwd);
    command.arg("--version");
    let text = answer(&mut command, QUERY_TIMEOUT).map_err(|failure| unreadable(QUERY, failure))?;
    grammar::parse_clippy_version(&text)
        .map_err(|error| unreadable(QUERY, QueryFailure::Grammar(error)))
}

/// Why one query produced no identity — a name for the message, never the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryFailure {
    /// The executable could not be started.
    Spawn,
    /// No exit within [`QUERY_TIMEOUT`]; the child was killed.
    Timeout,
    /// A non-zero exit.
    Exit,
    /// More than [`MAX_ANSWER_BYTES`] on stdout.
    Oversized,
    /// The answer is outside the grammar; the error names the piece.
    Grammar(GrammarError),
}

impl fmt::Display for QueryFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn => formatter.write_str("it could not be started"),
            Self::Timeout => write!(
                formatter,
                "it did not exit within {} seconds",
                QUERY_TIMEOUT.as_secs()
            ),
            Self::Exit => formatter.write_str("it exited with a failure status"),
            Self::Oversized => formatter.write_str("its answer is longer than an identity"),
            Self::Grammar(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

/// The `compiler_identity_unreadable` failure for one query. The message names the query and the
/// failure class; the answer is not in it.
fn unreadable(query: &'static str, failure: QueryFailure) -> CliError {
    CliError::new(
        Code::ProjectVerificationFailed,
        format!(
            "an executable Cargo launched to verify the generated project did not answer \
             `{query}` in the form FR-012-7e accepts ({failure}); nothing was written to the \
             destination. The answer itself is not shown: it is unparsed text from a tool the \
             environment configured"
        ),
    )
    .with("reason", REASON_IDENTITY_UNREADABLE)
    .with("query", query)
    .with("cause", failure.to_string())
    .with("stage", "pre-placement verification")
}

/// Runs a query with stdin closed and stderr discarded, reads at most the bound from stdout on
/// its own thread, and waits at most `timeout` — the reader-and-deadline shape `commands::relay`
/// uses, so a child that never exits or never stops writing is bounded either way.
///
/// `timeout` is a **deadline over both phases**: the child's exit and the collection of what it
/// wrote. The two are separate hazards — a child that never exits, and a child that exits leaving
/// a descendant holding the write end of its pipe — and a bound on the first alone leaves the
/// second unbounded on the success path. Every shipped caller passes [`QUERY_TIMEOUT`]; the
/// parameter exists so a control can prove the bound in under a second.
fn answer(command: &mut Command, timeout: Duration) -> Result<String, QueryFailure> {
    let deadline = std::time::Instant::now() + timeout;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| QueryFailure::Spawn)?;
    let stdout = child.stdout.take();
    // A channel, not a `JoinHandle`: a join has no deadline, and the collection below needs one.
    let (sender, reader) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut collected = Vec::new();
        if let Some(mut stream) = stdout {
            // One byte past the bound so an over-long answer is detectable, then the rest is
            // drained and discarded so the child is never blocked on a full pipe.
            let mut limited = (&mut stream).take(MAX_ANSWER_BYTES + 1);
            let _ = limited.read_to_end(&mut collected);
            let _ = std::io::copy(&mut stream, &mut std::io::sink());
        }
        let _ = sender.send(collected);
    });
    let status = match child.wait_timeout(super::isolate::remaining(deadline)) {
        Ok(Some(status)) => status,
        Ok(None) | Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            // Detached, not joined: a child that forked a grandchild holding the pipe would hold
            // the join for as long as the grandchild runs (see `commands::relay`).
            drop(reader);
            return Err(QueryFailure::Timeout);
        }
    };
    // AND THE SAME DEADLINE OVER THE COLLECTION. The child has exited; a descendant of it may
    // still hold the write end, and reading until that descendant ends is a wait this program
    // does not control. Bounding only the exit left the hazard one step further on, reached
    // through the success path.
    let Ok(collected) = reader.recv_timeout(super::isolate::remaining(deadline)) else {
        return Err(QueryFailure::Timeout);
    };
    if !status.success() {
        return Err(QueryFailure::Exit);
    }
    if u64::try_from(collected.len()).unwrap_or(u64::MAX) > MAX_ANSWER_BYTES {
        return Err(QueryFailure::Oversized);
    }
    Ok(String::from_utf8_lossy(&collected).into_owned())
}

/// A directory to compare `CARGO_MANIFEST_DIR` against: as given, and canonical when the
/// filesystem can say (`/tmp` is `/private/tmp` on macOS; Cargo may print either).
struct Location {
    raw: PathBuf,
    canonical: Option<PathBuf>,
}

impl Location {
    fn of(path: &Path) -> Self {
        Self {
            raw: path.to_path_buf(),
            canonical: path.canonicalize().ok(),
        }
    }

    /// Whether either spelling of this directory **begins with** `candidate` — the test that says
    /// a partly-read path is still a viable prefix of it rather than some other package's.
    fn begins_with(&self, candidate: &str) -> bool {
        let starts = |path: &Path| path.to_string_lossy().starts_with(candidate);
        starts(&self.raw) || self.canonical.as_deref().is_some_and(starts)
    }

    /// Whether the location Cargo PRINTS can be compared with this one at all.
    ///
    /// Cargo removes ANSI escape sequences from a path before printing it, and the removal is
    /// lossy. Measured 2026-09-09 on cargo 1.97.1, one directory per shape:
    ///
    /// | on disk | Cargo prints |
    /// |---|---|
    /// | `col<TAB>umn` | `col<TAB>umn` — raw |
    /// | `above<NEWLINE>FORGED` | raw, which is why the rejoining above exists |
    /// | `ESC[31mREDESC[0m` | `RED` |
    /// | `esc<ESC>and<TAB>tab` | `escnd<TAB>tab` — the `ESC` **and the byte after it** are gone |
    ///
    /// The grammar of that removal is Cargo's, not ours, and the last row shows it is not merely
    /// the SGR form this module already knows how to strip. Replicating it partly would be worse
    /// than not replicating it: the comparison would succeed for one escape shape and fail for
    /// another, with no principle a reader could apply.
    ///
    /// So the question is asked of OUR path, which is known exactly, rather than of Cargo's text:
    /// a staging path containing an `ESC` has no reliable printed spelling, and the location
    /// cannot discriminate for it. [`launch`] then falls back to the package name, as it did for
    /// every path before this correction.
    fn is_comparable(&self) -> bool {
        let printable = |path: &Path| !path.to_string_lossy().contains('\u{1b}');
        printable(&self.raw) && self.canonical.as_deref().is_none_or(printable)
    }

    fn matches(&self, candidate: &str) -> bool {
        let candidate = Path::new(candidate);
        if candidate == self.raw {
            return true;
        }
        match &self.canonical {
            Some(canonical) => candidate
                .canonicalize()
                .is_ok_and(|resolved| &resolved == canonical),
            None => false,
        }
    }
}

/// One `Running` line that carried `--crate-name`: whose it is, and what it launched.
struct Unit {
    own: bool,
    chain: Chain,
}

/// Reads the tokens of one `Running` command. `Ok(None)` when the command is not a compiler
/// launch — no `--crate-name`: a build script being run, a test binary being run.
fn launch(
    tokens: &[String],
    own_package: &str,
    own_crate: &str,
    staging: &Location,
) -> Result<Option<Unit>, EvidenceError> {
    let mut index = 0;
    let mut package_name: Option<&str> = None;
    let mut manifest_dir: Option<&str> = None;
    while let Some((name, value, consumed)) = environment_token(&tokens[index..]) {
        match name {
            "CARGO_PKG_NAME" => package_name = Some(value),
            "CARGO_MANIFEST_DIR" => manifest_dir = Some(value),
            _ => {}
        }
        index += consumed;
    }
    let arguments = &tokens[index..];
    let chain = Chain {
        executables: arguments
            .iter()
            .take_while(|token| !token.starts_with('-'))
            .map(PathBuf::from)
            .collect(),
    };
    if chain.is_empty() {
        return Err(EvidenceError::Malformed);
    }
    let Some(crate_name) = crate_name_of(arguments) else {
        return Ok(None);
    };
    // THE LOCATION IS NECESSARY WHEN IT IS PRESENT — the same test [`own_status_line`] applies,
    // now applied the same way round. A package's identity is its name, version and source; the
    // name alone is shared, and a renamed same-named path dependency
    // (`inner = { path = "…", package = "probe" }`) carries `CARGO_PKG_NAME=<own>` on its own
    // launch. Accepting the name as SUFFICIENT counted that dependency's units against the
    // project and pushed its chain into the identity set, so a foreign compiler could be queried
    // and a count that is not the project's could reach the provenance record.
    //
    // Two packages cannot share a manifest directory, so when `CARGO_MANIFEST_DIR` is present it
    // is both necessary and sufficient, and the name adds nothing. The name and the crate name
    // remain the fallbacks for a launch that carries no location — they are not removed, they are
    // demoted to the case where the discriminator is absent.
    // The location decides WHEN IT IS PRESENT AND COMPARABLE. See [`Location::is_comparable`]:
    // a staging path carrying an `ESC` has no reliable printed spelling, so for that narrow case
    // the name remains the only available test and finding 2's bound persists there — stated
    // rather than silently reintroduced.
    let own = match (manifest_dir, package_name) {
        (Some(dir), _) if staging.is_comparable() => staging.matches(dir),
        (_, Some(name)) => name == own_package,
        (_, None) => crate_name == own_crate,
    };
    Ok(Some(Unit { own, chain }))
}

/// The `--crate-name` argument, in either spelling Cargo could use.
fn crate_name_of(arguments: &[String]) -> Option<&str> {
    let mut iterator = arguments.iter();
    while let Some(argument) = iterator.next() {
        if argument == "--crate-name" {
            return iterator.next().map(String::as_str);
        }
        if let Some(value) = argument.strip_prefix("--crate-name=") {
            return Some(value);
        }
    }
    None
}

/// The environment assignment at the head of `tokens`, in either shape Cargo prints:
/// `NAME=value` (Unix) or `set NAME=value&&` (Windows) — `(name, value, tokens consumed)`.
fn environment_token(tokens: &[String]) -> Option<(&str, &str, usize)> {
    let first = tokens.first()?;
    if first == "set" {
        let second = tokens.get(1)?;
        let (name, value) = assignment(second)?;
        let value = value.strip_suffix("&&").unwrap_or(value);
        return Some((name, value, 2));
    }
    assignment(first).map(|(name, value)| (name, value, 1))
}

/// `NAME=value` where `NAME` matches `^[A-Za-z_][A-Za-z0-9_-]*$` — hyphens included, because
/// Cargo sets `CARGO_BIN_EXE_<name>` with the target's own name.
fn assignment(token: &str) -> Option<(&str, &str)> {
    let (name, value) = token.split_once('=')?;
    let mut bytes = name.bytes();
    let head = bytes.next()?;
    if !(head.is_ascii_alphabetic() || head == b'_') {
        return None;
    }
    if bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        Some((name, value))
    } else {
        None
    }
}

/// The text between the backticks of `` `…` ``; `Ok(None)` when the line is not backticked at all
/// (`Running unittests src/main.rs (…)` is a status line, not a command).
/// How many physical lines one `Running` command may span before the stream is treated as
/// something other than a command with newlines in its paths.
///
/// A path may contain newlines; a `Running` line made of a hundred of them is a stream this
/// parser should stop consuming rather than swallow the rest of the output looking for a
/// backtick. Sixteen is well past any real path and well short of a whole build log.
const MAX_REJOINED_LINES: usize = 16;

/// Rejoins a `Running` remainder whose backtick-quoted command was split across physical lines
/// by a newline inside one of its arguments.
///
/// `None` when the first line is already a whole command, which is every ordinary stream — the
/// caller then keeps borrowing the line it already has. An unterminated command that runs past
/// [`MAX_REJOINED_LINES`] comes back as it stands and fails [`backticked`] or [`tokenize`] as
/// `Malformed`, which is the honest answer for a stream this parser cannot account for.
///
/// # Why the end of the line is not the end of the command
///
/// Cargo wraps the command in backticks and quotes each argument, but neither the backtick nor
/// the newline is escaped inside a quoted value — both are ordinary characters there. So a line
/// that *ends* with a backtick has not necessarily ended the command: `tracing-serde`'s
/// `CARGO_PKG_DESCRIPTION` is the multi-line TOML string *"A compatibility layer for serializing
/// trace data with `` `serde` ``"*, whose own backtick lands at the end of the first physical
/// line while the command runs on. Reading that line alone yields a command with an unclosed
/// single quote, and the whole starter row is refused as a capture failure — which is exactly
/// what the census found. Completeness is therefore decided by [`whole`]: the candidate must be
/// backticked **and** tokenise, which no truncated prefix of a quoted argument does.
fn rejoin<'a>(rest: &str, lines: &mut impl Iterator<Item = &'a str>) -> Option<String> {
    if !rest.starts_with('`') || whole(rest) {
        return None;
    }
    let mut joined = rest.to_owned();
    for _ in 0..MAX_REJOINED_LINES {
        let Some(next) = lines.next() else { break };
        joined.push('\n');
        // RAW. A continuation is the middle of a path; Cargo colours only the status word,
        // so anything escape-like here is the operator's data.
        joined.push_str(next);
        if whole(&joined) {
            break;
        }
    }
    Some(joined)
}

/// Rejoins a `Fresh`/`Compiling`/`Checking` remainder whose parenthesised path was split across
/// physical lines by a newline inside it.
///
/// `None` when nothing needs joining, which is every ordinary stream; the caller then keeps the
/// line it has.
///
/// # Why a status line needs this at all
///
/// A directory name may contain a newline, and Cargo prints the path raw, so the status line
/// arrives in two pieces with the closing bracket on the second. [`rejoin`] has done this for
/// `Running` commands since the census found the shape. `renvor`'s own `tests/redaction.rs`
/// generates into a directory named `above<newline>FORGED-LINE`, so this is a shape the project
/// already promises to survive rather than a hypothesis.
///
/// # The staging path is the oracle, because the text cannot be one
///
/// "Does this line end the location?" has no answer in the text. A first version asked
/// `ends_with(')')`, and that is the **same mistake** `whole` exists to prevent on the launch
/// side: a `)` inside the path, immediately before the newline, closes the physical line without
/// closing the location — `above)<newline>FORGED-LINE` produces exactly that, measured. `whole`
/// answers the question by handing the candidate to `tokenize`, an authority outside the text;
/// here the authority is the staging directory the caller is about to test against.
///
/// So: join while the accumulated location is still a **prefix** of the staging path, and stop the
/// moment it *is* the staging path — or stops being able to become it. Two properties follow that
/// a suffix test could not give:
///
/// - a line for a **different package** is never joined (the name is checked first), and neither
///   is one for a same-named dependency whose path is not a prefix of staging — so a `Fresh` line
///   for a dependency can never swallow the lines after it;
/// - the loop cannot run away on a line that will never match, because viability fails on the
///   first join.
fn rejoin_status<'a>(
    rest: &str,
    own_package: &str,
    staging: &Location,
    lines: &mut impl Iterator<Item = &'a str>,
) -> Option<String> {
    if package_of(rest) != own_package {
        return None;
    }
    // Already the staging path, closing bracket and all: the ordinary case, nothing consumed.
    if location_of(rest).is_some_and(|at| staging.matches(at)) {
        return None;
    }
    // Not even the beginning of it: a dependency, or a package this parser has no business
    // joining. Nothing consumed.
    if !open_location_of(rest).is_some_and(|open| staging.begins_with(open)) {
        return None;
    }
    let mut joined = rest.to_owned();
    for _ in 0..MAX_REJOINED_LINES {
        let Some(next) = lines.next() else { break };
        joined.push('\n');
        // RAW. A continuation is the middle of a path; Cargo colours only the status word,
        // so anything escape-like here is the operator's data.
        joined.push_str(next);
        if location_of(&joined).is_some_and(|at| staging.matches(at)) {
            break;
        }
        if !open_location_of(&joined).is_some_and(|open| staging.begins_with(open)) {
            break;
        }
    }
    Some(joined)
}

/// Whether a backtick-opened remainder is a complete, tokenisable command.
fn whole(rest: &str) -> bool {
    // Trailing whitespace is Cargo's, not the command's: the command ends at its closing
    // backtick, and anything after that — including the `\r` of a `\r\n`-terminated stream — is
    // padding. Trimming HERE, for the test only, is what lets `parse_check` stop trimming the
    // physical line, where the same characters may be part of a directory name. `location_of`
    // has always done exactly this for the closing bracket.
    let rest = rest.trim_end();
    rest.len() > 1 && rest.ends_with('`') && tokenize(&rest[1..rest.len() - 1]).is_some()
}

fn backticked(rest: &str) -> Result<Option<&str>, EvidenceError> {
    // The command's own end, not the line's — see [`whole`].
    let rest = rest.trim_end();
    let Some(opened) = rest.strip_prefix('`') else {
        return Ok(None);
    };
    opened
        .strip_suffix('`')
        .map(Some)
        .ok_or(EvidenceError::Malformed)
}

/// The package name at the head of a `Fresh`/`Compiling`/`Checking` remainder
/// (`tiny-probe v0.1.0 (/abs/path)`).
fn package_of(rest: &str) -> &str {
    rest.split_whitespace().next().unwrap_or("")
}

/// The parenthesised location a status line ends with, if it has one.
///
/// Cargo prints `<name> v<version> (<path>)` for a package it reads from a directory, and
/// `<name> v<version>` — no parenthetical — for one it reads from a registry.
///
/// # The FIRST parenthesis, not the last
///
/// A directory name may contain parentheses — `Project (copy)`, `New Folder (2)` — and Cargo
/// prints the path raw, brackets and all. Searching backwards takes the innermost `(` and returns
/// a fragment: `…/paren (dir) test` comes back as `dir) test`, which matches no staging directory.
/// The consequences were both silent and both bad: a fully cached check lost its only own line and
/// was refused as `evidence_capture_failed`, and every other run lost its announcement, which
/// switches off "an announced package must launch" with no symptom at all.
///
/// Searching forwards is correct because of what precedes the path: a package name and a version,
/// neither of which can contain `(`. So the first `(` opens the location and everything to the
/// closing bracket is the path, brackets included. (Measured 2026-09-08 in a directory named
/// `paren (dir) test`; found by the independent validation of this round, against the backwards
/// form this function shipped with.)
/// Everything after the first `(`, **without** requiring the closing bracket — a location that
/// may still be mid-read, for the viability test in [`rejoin_status`].
fn open_location_of(rest: &str) -> Option<&str> {
    rest.find('(').map(|open| &rest[open + 1..])
}

fn location_of(rest: &str) -> Option<&str> {
    let inner = rest.trim_end().strip_suffix(')')?;
    let open = inner.find('(')?;
    Some(&inner[open + 1..])
}

/// Whether a `Fresh`/`Compiling`/`Checking` remainder is about **the project being verified**.
///
/// # Why the name is not enough
///
/// A package's identity is its name, version, and source; the name alone is shared. A project may
/// depend on a differently-versioned or differently-sourced crate of its own name — a wrapper named
/// after the crate it vendors or forks, declared as
/// `inner = { path = "…", package = "probe" }` — and Cargo then prints, in one stream:
///
/// ```text
///        Fresh probe v0.2.0 (/…/inner)
///    Compiling probe v0.1.0 (/…/outer)
/// ```
///
/// Cargo's own exclusivity is per identity and is intact there; it is *this* reading that
/// conflated the two. Counting both against `own_package` inflated the unit counts, and once the
/// contradiction check existed it turned an ordinary build into `evidence_capture_failed`
/// (measured 2026-09-08 on cargo 1.97.1; found by the independent validation of this round).
///
/// The location is what separates them: the project being verified is the one whose parenthetical
/// is the directory the check ran in. A registry dependency has no parenthetical at all, so it
/// cannot match by accident.
///
/// # The same test as [`launch`]'s, applied the other way round
///
/// [`launch`] also compares the staging directory — against a `Running` command's
/// `CARGO_MANIFEST_DIR` — but as one arm of a **disjunction**, where the location is *sufficient*
/// and `CARGO_PKG_NAME` alone is enough on its own. Here the location is *necessary*. The
/// difference is not cosmetic and is stated because it bounds what this fix achieved: a status
/// line for a same-named dependency is now excluded, while that dependency's `Running` lines are
/// still counted as the project's own, because they carry `CARGO_PKG_NAME=<own>`. Its unit counts
/// and its chain can therefore still reach the record. That is a **record that is wrong**, not a
/// run that fails — the contradiction check no longer reads the launch count — and it predates
/// this round; requiring the location on the launch side too is a change with a wider blast
/// radius than this correction round opened.
fn own_status_line(rest: &str, own_package: &str, staging: &Location) -> bool {
    package_of(rest) == own_package && location_of(rest).is_some_and(|at| staging.matches(at))
}

/// Splits a `Running` command the way the platform's Cargo quoted it.
fn tokenize(command: &str) -> Option<Vec<String>> {
    if cfg!(windows) {
        windows_tokens(command)
    } else {
        unix_tokens(command)
    }
}

/// The Unix shape: POSIX shell words, which is what Cargo's `shell-escape` produces there.
#[must_use]
pub fn unix_tokens(command: &str) -> Option<Vec<String>> {
    shell_words::split(command).ok()
}

/// The Windows shape: the argument rules Cargo's `shell-escape` produces there — a token is
/// double-quoted when it needs to be, `2n` backslashes before a quote are `n` backslashes, `2n+1`
/// are `n` backslashes and a literal quote, and a backslash anywhere else is a backslash.
///
/// `None` for a quote that never closes.
#[must_use]
pub fn windows_tokens(command: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_token = false;
    let mut quoted = false;
    let mut characters = command.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                let mut backslashes = 1;
                while characters.peek() == Some(&'\\') {
                    characters.next();
                    backslashes += 1;
                }
                if characters.peek() == Some(&'"') {
                    characters.next();
                    current.extend(std::iter::repeat_n('\\', backslashes / 2));
                    if backslashes % 2 == 1 {
                        current.push('"');
                    } else {
                        quoted = !quoted;
                    }
                } else {
                    current.extend(std::iter::repeat_n('\\', backslashes));
                }
                in_token = true;
            }
            '"' => {
                quoted = !quoted;
                in_token = true;
            }
            character if character.is_whitespace() && !quoted => {
                if in_token {
                    tokens.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            character => {
                current.push(character);
                in_token = true;
            }
        }
    }
    if quoted {
        return None;
    }
    if in_token {
        tokens.push(current);
    }
    Some(tokens)
}

/// `text` without terminal colour sequences (`ESC [ … m`), which Cargo emits around the status
/// word when `CARGO_TERM_COLOR=always` passes through the seal.
/// The status word at the head of a physical line, and the RAW remainder after it.
///
/// Cargo colours the status word and only the status word —
/// `ESC[1mESC[32m       Fresh ESC[0m probe v0.1.0 (…)` — so the colour, and Cargo's indentation,
/// are removed from the head while everything after is returned untouched.
///
/// # Why not strip the whole line
///
/// An `ESC` is a legal character in a Unix directory name, and Cargo prints the path raw. Removing
/// SGR sequences from the whole line therefore deleted part of the operator's own path, the
/// location stopped equalling the staging directory, and the run was refused as
/// `evidence_capture_failed` — the same failure `str::trim` caused with trailing spaces and
/// `str::lines` caused with a `\r`. All three were the parser destroying operator-controlled text
/// before comparing it. `renvor`'s own `tests/redaction.rs` creates a directory named
/// `ESC[31mREDESC[0m`, so the shape is measured, not imagined.
///
/// `None` when the line has no leading word, which is every continuation and every diagnostic.
fn status_and_rest(line: &str) -> Option<(&str, &str)> {
    let head = past_colour_and_indent(line);
    let end = head.find(|character: char| !character.is_ascii_alphabetic())?;
    if end == 0 {
        return None;
    }
    let (word, after) = head.split_at(end);
    Some((word, past_colour_and_indent(after)))
}

/// Skips Cargo's own indentation and any SGR sequences at the head of `text`.
fn past_colour_and_indent(text: &str) -> &str {
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        match past_one_sgr(trimmed) {
            Some(after) => rest = after,
            None => return trimmed,
        }
    }
}

/// One complete `ESC[…m` sequence at the head of `text`, if there is one. A bare `ESC` that opens
/// no sequence is data and is left alone.
fn past_one_sgr(text: &str) -> Option<&str> {
    let parameters = text.strip_prefix('\u{1b}')?.strip_prefix('[')?;
    let end = parameters.find('m')?;
    Some(&parameters[end + 1..])
}

fn without_sgr(text: &str) -> Cow<'_, str> {
    if !text.contains('\u{1b}') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' && characters.peek() == Some(&'[') {
            characters.next();
            for parameter in characters.by_ref() {
                if parameter == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(character);
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Unix stream of one own launch, in the measured shape.
    fn launched_stream(own: &str, staging: &str) -> String {
        format!(
            "   Compiling {own} v0.1.0 ({staging})\n     Running `CARGO=/t/bin/cargo \
             CARGO_MANIFEST_DIR={staging} CARGO_PKG_NAME={own} CARGO_PKG_REPOSITORY='' \
             DYLD_FALLBACK_LIBRARY_PATH='/t/lib:/x y' /t/bin/rustc --crate-name probe \
             --edition=2024 src/main.rs --cfg renvor_marker_9f8e7d`\n    Finished `dev` profile \
             [unoptimized + debuginfo] target(s) in 1.31s\n"
        )
    }

    /// A directory name may legally contain a newline, and every path in a `Running` line comes
    /// from the operator's filesystem — so one command can arrive as several physical lines.
    ///
    /// Found by `tests/redaction.rs`'s hostile-ancestor case: generating into
    /// `above<newline>FORGED-LINE/` refused with `evidence_capture_failed`, because the first
    /// physical line ended in a backtick that never closed. A legal directory name is not a
    /// capture failure.
    /// Unix-shaped: the stream fixture below is quoted the way Cargo quotes on Unix, and
    /// [`tokenize`] follows the platform's rules. The Windows shape has its own tests —
    /// [`the_windows_shape_is_tokenised_under_its_own_rules`] for the tokeniser and
    /// [`a_backtick_inside_a_quoted_argument_does_not_end_a_windows_command`] for the stream.
    #[cfg(unix)]
    #[test]
    fn a_running_command_split_by_a_newline_in_a_path_is_rejoined() {
        let staging = tempfile::tempdir().expect("tempdir");
        let hostile = staging.path().join("above\nFORGED-LINE");
        std::fs::create_dir_all(&hostile).expect("mkdir");
        let quoted = format!("'{}'", hostile.display());
        let text = format!(
            "   Compiling probe v0.1.0 ({0})\n     Running `CARGO_MANIFEST_DIR={1} \
             CARGO_PKG_NAME=probe /t/bin/rustc --crate-name probe --edition=2024 src/main.rs`\n \
             Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.31s\n",
            hostile.display(),
            quoted
        );
        assert!(
            text.matches('\n').count() > 3,
            "the fixture does not actually split the Running line"
        );
        let evidence = parse_check(&text, "probe", &hostile).expect("a legal path is not a defect");
        assert_eq!(evidence.units_launched, 1, "the rejoined launch was lost");
        assert_eq!(
            trailing_compiler(&evidence.chains[0]),
            Some(Path::new("/t/bin/rustc")),
            "the rejoined chain does not end in the compiler"
        );
    }

    /// A quoted argument whose own text ends a physical line with a backtick does not end the
    /// command (found by the census on the `observeonly` and `mysea` starter rows, 2026-09-07).
    ///
    /// `tracing-serde`'s description is a multi-line TOML string containing `` `serde` ``, so
    /// Cargo's one `Running` line arrives as two physical lines, the first ending in that
    /// backtick. Reading only the first leaves an unclosed single quote, and the row is refused
    /// as `evidence_capture_failed` — a real generation blocked by a dependency's prose.
    /// Unix-shaped: the stream fixture below is quoted the way Cargo quotes on Unix, and
    /// [`tokenize`] follows the platform's rules. The Windows shape has its own tests —
    /// [`the_windows_shape_is_tokenised_under_its_own_rules`] for the tokeniser and
    /// [`a_backtick_inside_a_quoted_argument_does_not_end_a_windows_command`] for the stream.
    #[cfg(unix)]
    #[test]
    fn a_backtick_inside_a_quoted_argument_does_not_end_the_command() {
        let stream = concat!(
            "   Compiling probe v0.1.0 (/s)\n",
            "     Running `CARGO_PKG_DESCRIPTION='trace data with `serde`\n",
            "' CARGO_CRATE_NAME=probe /t/rustc --crate-name probe /s/src/main.rs`\n",
            "    Finished `dev` profile\n",
        );
        let evidence = parse_check(stream, "probe", Path::new("/s")).expect("one launched unit");
        assert_eq!(evidence.units_launched, 1, "the launch is observed");
        assert_eq!(evidence.units_fresh, 0, "nothing was fresh");
        assert_eq!(
            evidence.chains[0].executables.len(),
            1,
            "the chain is the compiler alone, with no argument mistaken for one"
        );
    }

    /// A backtick that never closes at all is still `Malformed`: the rejoin is bounded, and a
    /// stream this parser cannot account for is a capture failure rather than a silent success.
    /// The same defect as the Unix case above, in the shape Windows Cargo prints: a value quoted
    /// with double quotes whose own text ends the physical line with a backtick.
    #[cfg(windows)]
    #[test]
    fn a_backtick_inside_a_quoted_argument_does_not_end_a_windows_command() {
        let stream = concat!(
            "   Compiling probe v0.1.0 (C:\\s)\n",
            "     Running `CARGO_PKG_DESCRIPTION=\"trace data with `serde`\n",
            "\" CARGO_PKG_NAME=probe C:\\t\\rustc.exe --crate-name probe C:\\s\\src\\main.rs`\n",
            "    Finished `dev` profile\n",
        );
        let evidence = parse_check(stream, "probe", Path::new("C:\\s")).expect("one launched unit");
        assert_eq!(evidence.units_launched, 1, "the launch is observed");
        assert_eq!(evidence.units_fresh, 0, "nothing was fresh");
        assert_eq!(
            evidence.chains[0].executables.len(),
            1,
            "the chain is the compiler alone, with no argument mistaken for one"
        );
    }

    #[test]
    fn an_unterminated_running_command_is_still_malformed() {
        let staging = tempfile::tempdir().expect("tempdir");
        let mut text = String::from("     Running `CARGO_PKG_NAME=probe /t/bin/rustc");
        for _ in 0..(MAX_REJOINED_LINES + 4) {
            text.push_str("\nstill going");
        }
        text.push_str("\n    Finished `dev` profile in 1.31s\n");
        let error = parse_check(&text, "probe", staging.path()).expect_err("no closing backtick");
        assert_eq!(error, EvidenceError::Malformed);
    }

    /// Unix-shaped: the stream fixture below is quoted the way Cargo quotes on Unix, and
    /// [`tokenize`] follows the platform's rules. The Windows shape has its own tests —
    /// [`the_windows_shape_is_tokenised_under_its_own_rules`] for the tokeniser and
    /// [`a_backtick_inside_a_quoted_argument_does_not_end_a_windows_command`] for the stream.
    #[cfg(unix)]
    #[test]
    fn a_launched_own_unit_is_counted_with_its_chain_and_nothing_else_is_kept() {
        let staging = tempfile::tempdir().expect("tempdir");
        let text = launched_stream("probe", &staging.path().display().to_string());
        let evidence = parse_check(&text, "probe", staging.path()).expect("parses");
        assert_eq!(evidence.units_launched, 1, "one own launch");
        assert_eq!(evidence.units_fresh, 0, "nothing fresh");
        assert_eq!(evidence.chains.len(), 1, "one chain");
        assert_eq!(
            trailing_compiler(&evidence.chains[0]),
            Some(Path::new("/t/bin/rustc")),
            "the chain ends in the compiler argument"
        );
        assert!(
            clippy_driver(&evidence.chains[0]).is_none(),
            "a build chain names no driver"
        );
        assert!(
            !shows_wrapper(&evidence.chains[0], false),
            "a bare chain is not a wrapper"
        );
        let rendered = format!("{evidence:?}");
        assert!(
            !rendered.contains("rustc") && !rendered.contains("renvor_marker"),
            "the Debug rendering carries a path or an argument"
        );
        assert!(
            rendered.contains("executables: 1"),
            "the Debug rendering counts the chain"
        );
    }

    #[test]
    fn a_dependency_unit_and_a_non_compiler_running_line_are_not_units() {
        let staging = tempfile::tempdir().expect("tempdir");
        let dir = staging.path().display().to_string();
        let text = format!(
            "   Compiling serde v1.0.0\n     Running `CARGO=/t/bin/cargo \
             CARGO_MANIFEST_DIR=/home/x/.cargo/registry/src/serde-1.0.0 CARGO_PKG_NAME=serde \
             /t/bin/rustc --crate-name serde src/lib.rs`\n   Compiling probe v0.1.0 ({dir})\n     \
             Running `CARGO=/t/bin/cargo CARGO_MANIFEST_DIR={dir} CARGO_PKG_NAME=probe \
             /t/bin/rustc --crate-name build_script_build build.rs`\n     Running \
             `CARGO=/t/bin/cargo CARGO_MANIFEST_DIR={dir} CARGO_PKG_NAME=probe \
             {dir}/target/debug/build/probe-1/build-script-build`\n     Running \
             `CARGO=/t/bin/cargo CARGO_MANIFEST_DIR={dir} CARGO_PKG_NAME=probe /t/bin/rustc \
             --crate-name probe src/main.rs`\n    Finished `test` profile in 0.1s\n     Running \
             `CARGO=/t/bin/cargo CARGO_MANIFEST_DIR={dir} {dir}/target/debug/deps/probe-2`\n     \
             Running unittests src/main.rs ({dir}/target/debug/deps/probe-2)\n"
        );
        let evidence = parse_check(&text, "probe", staging.path()).expect("parses");
        assert_eq!(
            evidence.units_launched, 2,
            "the build script compile and the bin are own units; the executions are not"
        );
        assert_eq!(evidence.chains.len(), 2, "two chains");
    }

    /// A path whose pre-newline segment ENDS WITH `)` — the shape a suffix test calls closed.
    ///
    /// `above)<newline>FORGED-LINE` makes Cargo print a first physical line that ends with `)`
    /// while the location runs on (measured 2026-09-08). A `ends_with(')')` test reads it as
    /// complete and extracts `…/above`, which matches no staging directory — the same silent
    /// double failure as the two triggers before it. This is the third, and it is why the join is
    /// decided by the staging path rather than by punctuation.
    ///
    /// The second half is the guard that makes the oracle safe: a `Fresh` line for a **same-named
    /// dependency**, whose path is not a prefix of staging, must consume nothing — or a rejoin
    /// would eat the project's own lines that follow it.    ///
    /// # Unix only, and not as a shrug
    ///
    /// Windows refuses to create a directory whose name contains a newline —
    /// `ERROR_INVALID_NAME` (123), measured on both `windows-latest` legs. So the trigger cannot
    /// occur there: the shape this guards against is one that platform's filesystem will not
    /// produce. `tests/redaction.rs` excludes its own newline fixture for the same reason and says
    /// so. The parsing itself is platform-independent, and the bracket control beside this one —
    /// whose directory name IS legal on Windows — runs everywhere.
    #[cfg(unix)]
    #[test]
    fn a_closing_bracket_before_a_newline_does_not_end_the_location() {
        let root = tempfile::tempdir().expect("tempdir");
        let staging = root.path().join("above)\nFORGED-LINE");
        std::fs::create_dir(&staging).expect("brackets and newlines are legal in a directory name");
        let at = staging.display();

        let cached =
            format!("       Fresh probe v0.1.0 ({at})\n    Finished `dev` profile in 0.0s\n");
        let evidence = parse_check(&cached, "probe", &staging)
            .expect("the first line ends with `)` and the location is not closed");
        assert_eq!(evidence.units_fresh, 1);

        // THE GUARD. A same-named dependency's line is not a prefix of staging, so it joins
        // nothing — the project's own lines after it survive.
        // The launch carries `CARGO_PKG_NAME` and no manifest directory: the newline belongs in
        // the path under test, not inside a command, where it would be `rejoin`'s subject instead.
        let with_a_dependency = format!(
            "       Fresh probe v0.2.0 (/elsewhere/inner)\n   Compiling probe v0.1.0 ({at})\n     \
             Running `CARGO_PKG_NAME=probe /t/bin/rustc --crate-name probe src/main.rs`\n    \
             Finished `dev` profile in 0.4s\n"
        );
        let evidence = parse_check(&with_a_dependency, "probe", &staging)
            .expect("a dependency line consumed the lines after it");
        assert_eq!(evidence.units_fresh, 0, "the dependency is not the project");
        assert_eq!(
            evidence.units_launched, 1,
            "the project's own unit survived the rejoin"
        );
    }

    /// A project directory whose NAME CONTAINS A NEWLINE is still the project's own.
    ///
    /// The shape `tests/redaction.rs` already generates into — `above<newline>FORGED-LINE` — so
    /// this is not a hypothesis about hostile input but a directory this project promises to
    /// survive. Cargo prints the path raw, so the status line arrives as two physical lines with
    /// the closing bracket on the second, and a location test that reads one line finds no
    /// bracket, matches nothing, and drops the own line. Same consequences as the bracketed name:
    /// a cached check is refused, and every other run silently loses its announcement.
    ///
    /// `rejoin` has done this for launch commands since the census found the shape; this is the
    /// status line's half of it.    ///
    /// # Unix only, and not as a shrug
    ///
    /// Windows refuses to create a directory whose name contains a newline —
    /// `ERROR_INVALID_NAME` (123), measured on both `windows-latest` legs. So the trigger cannot
    /// occur there: the shape this guards against is one that platform's filesystem will not
    /// produce. `tests/redaction.rs` excludes its own newline fixture for the same reason and says
    /// so. The parsing itself is platform-independent, and the bracket control beside this one —
    /// whose directory name IS legal on Windows — runs everywhere.
    #[cfg(unix)]
    #[test]
    fn a_project_directory_whose_name_carries_a_newline_is_still_the_project() {
        let root = tempfile::tempdir().expect("tempdir");
        let staging = root.path().join("above\nFORGED-LINE");
        std::fs::create_dir(&staging).expect("a newline is a legal directory name");
        let at = staging.display();

        // (a) Cached: the one own line spans two physical lines and must still count.
        let cached =
            format!("       Fresh probe v0.1.0 ({at})\n    Finished `dev` profile in 0.0s\n");
        let evidence = parse_check(&cached, "probe", &staging)
            .expect("a cached check in a newline-bearing directory is ordinary evidence");
        assert_eq!(evidence.units_fresh, 1);

        // (b) The announcement still announces, so an announced package with no launch is refused.
        let announced_only =
            format!("   Compiling probe v0.1.0 ({at})\n    Finished `dev` profile in 0.4s\n");
        assert_eq!(
            parse_check(&announced_only, "probe", &staging),
            Err(EvidenceError::Unaccounted),
            "the split announcement was not recognised, so its guarantee was switched off"
        );

        // (c) THE SPELLING CARGO ACTUALLY USES, ACROSS THE SPLIT. The two above build the stream
        // from the same `staging` the caller passes, so both sides agree and the raw-vs-canonical
        // question never arises — while a real macOS run has Cargo printing `/private/var/…` and
        // the caller holding `/var/…`. That combination is what needs `begins_with` to try both
        // spellings: a PARTIAL path cannot be canonicalised, so a single-spelling viability test
        // would refuse every legitimate own line whose directory also carried a newline.
        let canonical = staging
            .canonicalize()
            .expect("the staging directory resolves");
        let cached_canonically = format!(
            "       Fresh probe v0.1.0 ({})\n    Finished `dev` profile in 0.0s\n",
            canonical.display()
        );
        let evidence = parse_check(&cached_canonically, "probe", &staging)
            .expect("the canonical spelling of a split path is still the project's own");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// A project directory whose NAME CONTAINS PARENTHESES is still the project's own.
    ///
    /// `Project (copy)` and `New Folder (2)` are ordinary directory names, and Cargo prints the
    /// path raw. Reading the location backwards from the closing bracket returns `dir) test` for
    /// `…/paren (dir) test`, which matches nothing — and the damage is silent both ways: a fully
    /// cached check loses its only own line and is refused, and every other run quietly loses its
    /// announcement, which is what "an announced package must launch" rests on.
    ///
    /// Both halves are asserted here because they fail differently: the cached stream fails loudly
    /// as `Unaccounted`, and the announced one fails by **passing** where it should refuse.
    #[test]
    fn a_project_directory_whose_name_carries_brackets_is_still_the_project() {
        let root = tempfile::tempdir().expect("tempdir");
        let staging = root.path().join("paren (dir) test");
        std::fs::create_dir(&staging).expect("a bracketed directory is an ordinary directory");
        let at = staging.display();

        // (a) The cached stream: its one own line must count, or the check is refused.
        let cached =
            format!("       Fresh probe v0.1.0 ({at})\n    Finished `dev` profile in 0.0s\n");
        let evidence = parse_check(&cached, "probe", &staging)
            .expect("a cached check in a bracketed directory is ordinary evidence");
        assert_eq!(evidence.units_fresh, 1);

        // (b) The announcement: it must still be an announcement, so an announced package with no
        // own launch is still refused. Read backwards, this stream passes.
        let announced_only =
            format!("   Compiling probe v0.1.0 ({at})\n    Finished `dev` profile in 0.4s\n");
        assert_eq!(
            parse_check(&announced_only, "probe", &staging),
            Err(EvidenceError::Unaccounted),
            "the announcement was not recognised, so the guarantee it carries was switched off"
        );
    }

    /// The own status line counts when Cargo spells the staging directory **differently from the
    /// caller** — which it does on every macOS run.
    ///
    /// Cargo prints the canonical path in the parenthetical; `std::env::temp_dir()` hands the
    /// generator `/var/folders/…` and Cargo answers `/private/var/folders/…` (both measured
    /// 2026-09-08). `Location::matches` resolves the candidate before comparing, so the two meet.
    ///
    /// # Why this is asserted through `Fresh` and not through `Compiling`
    ///
    /// A missed status line is **silent**: an unmatched `Compiling` leaves `announced_ever` false,
    /// which discharges nothing and refuses nothing. And it cannot be observed through the launch
    /// count either — `launch` recognises an own unit by `CARGO_PKG_NAME` alone, so a `Running`
    /// line counts whatever its paths say. `units_fresh` is the one field a status-line match
    /// moves on its own, so `Fresh` is what this asks about. (The first version of this test used
    /// `Compiling` plus a launch, and passed with the resolution deliberately removed.)
    #[test]
    fn an_own_status_line_counts_when_cargo_spells_the_directory_canonically() {
        let staging = tempfile::tempdir().expect("tempdir");
        let raw = staging.path().to_path_buf();
        let canonical = raw.canonicalize().expect("the staging directory resolves");
        let cached = |at: &std::path::Path| {
            format!(
                "       Fresh probe v0.1.0 ({})\n    Finished `dev` profile in 0.0s\n",
                at.display()
            )
        };

        // The spelling Cargo actually uses. Unmatched, this is `Unaccounted` — the own package
        // would appear nowhere — so the `expect` is the assertion.
        let evidence = parse_check(&cached(&canonical), "probe", &raw)
            .expect("the canonical spelling is the project's own");
        assert_eq!(evidence.units_fresh, 1);

        // And the caller's own spelling, whether or not it differs from the above.
        let evidence = parse_check(&cached(&raw), "probe", &raw)
            .expect("the caller's spelling is the project's own");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// A `Fresh` line is the project's own when it names the package **and the directory the
    /// check ran in** — not when it merely shares the name.
    ///
    /// The dependency cases are the point. `serde` is separated by its name, which any reading
    /// manages. The second is not: a project may depend on a differently-versioned crate **of its
    /// own name** (`inner = { path = "…", package = "probe" }`), and Cargo then prints `Fresh probe
    /// v0.2.0 (…/inner)` beside `Compiling probe v0.1.0 (…/outer)` — measured on cargo 1.97.1. A
    /// name-only reading counted both against the project, which inflated the counts and, once the
    /// contradiction check existed, refused an ordinary build outright.
    #[test]
    fn a_fresh_own_package_is_counted_and_a_fresh_dependency_is_not() {
        let staging = tempfile::tempdir().expect("tempdir");
        let at = staging.path().display();
        let text = format!(
            "       Fresh serde v1.0.0\n       Fresh probe v0.1.0 ({at})\n    Finished `dev` \
             profile [unoptimized + debuginfo] target(s) in 0.00s\n"
        );
        let evidence = parse_check(&text, "probe", staging.path()).expect("parses");
        assert_eq!(evidence.units_fresh, 1, "one own fresh report");
        assert_eq!(evidence.units_launched, 0, "no launch");
        assert!(evidence.chains.is_empty(), "no chain");

        // THE SAME NAME, A DIFFERENT PACKAGE. The dependency is reused and the project is built:
        // an ordinary stream, and one a name-only reading called a contradiction.
        let shared_name = format!(
            "       Fresh probe v0.2.0 (/elsewhere/inner)\n   Compiling probe v0.1.0 ({at})\n                  Running `CARGO_MANIFEST_DIR={at} CARGO_PKG_NAME=probe /t/bin/rustc --crate-name probe \
             src/main.rs`\n    Finished `dev` profile in 0.4s\n"
        );
        let evidence = parse_check(&shared_name, "probe", staging.path())
            .expect("a same-named dependency is not the project, and the stream is ordinary");
        assert_eq!(
            evidence.units_fresh, 0,
            "a dependency that shares the project's name was counted as the project"
        );
        assert_eq!(
            evidence.units_launched, 1,
            "the project's own unit launched"
        );
    }

    #[test]
    fn a_stream_without_finished_is_truncated() {
        let staging = tempfile::tempdir().expect("tempdir");
        let text = launched_stream("probe", &staging.path().display().to_string());
        let cut = text.split("    Finished").next().expect("the head");
        assert_eq!(
            parse_check(cut, "probe", staging.path()),
            Err(EvidenceError::Truncated),
            "no Finished line is truncated"
        );
    }

    #[test]
    fn an_announced_own_package_with_no_launch_is_unaccounted() {
        let staging = tempfile::tempdir().expect("tempdir");
        let text = "   Compiling probe v0.1.0 (/x)\n    Finished `dev` profile in 0.1s\n";
        assert_eq!(
            parse_check(text, "probe", staging.path()),
            Err(EvidenceError::Unaccounted),
            "Compiling without Running is unaccounted"
        );
        let checking = "    Checking probe v0.1.0 (/x)\n    Finished `dev` profile in 0.1s\n";
        assert_eq!(
            parse_check(checking, "probe", staging.path()),
            Err(EvidenceError::Unaccounted),
            "Checking without Running is unaccounted"
        );
    }

    #[test]
    fn an_absent_own_package_is_unaccounted_never_cached() {
        let staging = tempfile::tempdir().expect("tempdir");
        let text = "       Fresh serde v1.0.0\n    Finished `dev` profile in 0.0s\n";
        assert_eq!(
            parse_check(text, "probe", staging.path()),
            Err(EvidenceError::Unaccounted),
            "an own package that appears nowhere is unaccounted"
        );
    }

    /// Unix-shaped: the stream fixture below is quoted the way Cargo quotes on Unix, and
    /// [`tokenize`] follows the platform's rules. The Windows shape has its own tests —
    /// [`the_windows_shape_is_tokenised_under_its_own_rules`] for the tokeniser and
    /// [`a_backtick_inside_a_quoted_argument_does_not_end_a_windows_command`] for the stream.
    #[cfg(unix)]
    #[test]
    fn a_running_line_that_does_not_tokenise_is_malformed() {
        let staging = tempfile::tempdir().expect("tempdir");
        let unclosed_quote = "     Running `CARGO_PKG_NAME=probe X='oops /t/bin/rustc \
                              --crate-name probe`\n    Finished `dev` profile in 0.1s\n";
        assert_eq!(
            parse_check(unclosed_quote, "probe", staging.path()),
            Err(EvidenceError::Malformed),
            "an unclosed quote is malformed"
        );
        let unclosed_backtick = "     Running `CARGO_PKG_NAME=probe /t/bin/rustc --crate-name \
                                 probe\n    Finished `dev` profile in 0.1s\n";
        assert_eq!(
            parse_check(unclosed_backtick, "probe", staging.path()),
            Err(EvidenceError::Malformed),
            "an unclosed backtick is malformed"
        );
        let no_executable = "     Running `CARGO_PKG_NAME=probe --crate-name probe`\n    \
                             Finished `dev` profile in 0.1s\n";
        assert_eq!(
            parse_check(no_executable, "probe", staging.path()),
            Err(EvidenceError::Malformed),
            "a command with no executable is malformed"
        );
    }

    #[test]
    fn the_own_unit_is_recognised_by_manifest_directory_or_crate_name_when_the_name_is_absent() {
        let staging = tempfile::tempdir().expect("tempdir");
        let dir = staging.path().display().to_string();
        let by_manifest = format!(
            "     Running `CARGO_MANIFEST_DIR={dir} /t/bin/rustc --crate-name other \
             src/main.rs`\n    Finished `dev` profile in 0.1s\n"
        );
        let evidence = parse_check(&by_manifest, "my-probe", staging.path()).expect("parses");
        assert_eq!(
            evidence.units_launched, 1,
            "the manifest directory identifies the unit"
        );
        let by_crate = "     Running `/t/bin/rustc --crate-name my_probe src/main.rs`\n    \
                        Finished `dev` profile in 0.1s\n";
        let evidence = parse_check(by_crate, "my-probe", staging.path()).expect("parses");
        assert_eq!(
            evidence.units_launched, 1,
            "with no environment tokens the crate name identifies the unit, hyphens as underscores"
        );
        let other_name = "     Running `CARGO_PKG_NAME=other /t/bin/rustc --crate-name my_probe \
                          src/main.rs`\n    Finished `dev` profile in 0.1s\n";
        assert_eq!(
            parse_check(other_name, "my-probe", staging.path()),
            Err(EvidenceError::Unaccounted),
            "a present package name that differs is not overridden by the crate name"
        );
    }

    #[test]
    fn the_clippy_chain_names_the_driver_first_and_a_wrapper_in_front_is_seen() {
        let staging = tempfile::tempdir().expect("tempdir");
        let text = "    Checking probe v0.1.0 (/x)\n     Running `CARGO_PKG_NAME=probe \
                    /opt/cache/wrapper /t/bin/clippy-driver /t/bin/rustc --crate-name probe \
                    src/main.rs`\n    Finished `dev` profile in 0.1s\n";
        let evidence = parse_check(text, "probe", staging.path()).expect("parses");
        let chain = &evidence.chains[0];
        assert_eq!(chain.len(), 3, "wrapper, driver, compiler");
        assert_eq!(
            clippy_driver(chain),
            Some(Path::new("/t/bin/clippy-driver")),
            "the driver is found behind the wrapper"
        );
        assert_eq!(
            trailing_compiler(chain),
            Some(Path::new("/t/bin/rustc")),
            "the trailing compiler is the argument"
        );
        assert!(
            shows_wrapper(chain, true),
            "three executables is a wrapper for clippy"
        );
        let bare = "    Checking probe v0.1.0 (/x)\n     Running `CARGO_PKG_NAME=probe \
                    /t/bin/clippy-driver.exe /t/bin/rustc.exe --crate-name probe src/main.rs`\n    \
                    Finished `dev` profile in 0.1s\n";
        let evidence = parse_check(bare, "probe", staging.path()).expect("parses");
        assert!(
            !shows_wrapper(&evidence.chains[0], true),
            "driver plus compiler is not a wrapper"
        );
        assert!(
            clippy_driver(&evidence.chains[0]).is_some(),
            "the Windows spelling of the driver is recognised"
        );
    }

    #[test]
    fn the_windows_shape_is_tokenised_under_its_own_rules() {
        let tokens = windows_tokens(
            r#"set CARGO=C:\t\bin\cargo.exe&& set CARGO_PKG_DESCRIPTION="a \"quoted\" value"&& set CARGO_PKG_NAME=probe&& C:\t\bin\rustc.exe --crate-name probe "C:\Users\x y\src\main.rs""#,
        )
        .expect("tokenises");
        assert_eq!(tokens[0], "set", "the set keyword");
        assert_eq!(tokens[1], r"CARGO=C:\t\bin\cargo.exe&&", "backslashes stay");
        assert_eq!(
            tokens[3], r#"CARGO_PKG_DESCRIPTION=a "quoted" value&&"#,
            "escaped quotes are quotes and the token spans the space"
        );
        assert_eq!(tokens[6], r"C:\t\bin\rustc.exe", "the executable");
        assert_eq!(
            tokens[9], r"C:\Users\x y\src\main.rs",
            "a quoted path with a space"
        );
        assert!(
            windows_tokens(r#"set A="open&& rustc"#).is_none(),
            "an unclosed quote does not tokenise"
        );
        let staging = tempfile::tempdir().expect("tempdir");
        let launched = launch(&tokens, "probe", "probe", &Location::of(staging.path()))
            .expect("a launch")
            .expect("a compiler launch");
        assert!(launched.own, "the set-prefixed name identifies the unit");
        assert_eq!(launched.chain.len(), 1, "one executable");
    }

    #[test]
    fn colour_sequences_around_the_status_word_are_ignored() {
        let staging = tempfile::tempdir().expect("tempdir");
        // The escape lives in a binding: `\u{1b}` is a lexer escape, so it cannot be written
        // inside a `format!` literal that also needs braces of its own.
        let esc = "\u{1b}";
        let text = format!(
            "{esc}[1m{esc}[32m       Fresh{esc}[0m probe v0.1.0 ({at})\n{esc}[1m{esc}[32m    \
             Finished{esc}[0m `dev` profile in 0.0s\n",
            at = staging.path().display()
        );
        let evidence = parse_check(&text, "probe", staging.path()).expect("parses");
        assert_eq!(evidence.units_fresh, 1, "the coloured Fresh line counts");
    }

    #[test]
    fn the_observation_is_launched_cached_or_mixed_and_never_from_nothing() {
        let launched = CheckEvidence {
            units_launched: 1,
            units_fresh: 0,
            chains: Vec::new(),
        };
        let fresh = CheckEvidence {
            units_launched: 0,
            units_fresh: 1,
            chains: Vec::new(),
        };
        let nothing = CheckEvidence {
            units_launched: 0,
            units_fresh: 0,
            chains: Vec::new(),
        };
        assert_eq!(
            observation(&launched, &launched),
            Ok(Observation::Launched),
            "all launched"
        );
        assert_eq!(
            observation(&fresh, &fresh),
            Ok(Observation::Cached),
            "all fresh"
        );
        assert_eq!(
            observation(&fresh, &launched),
            Ok(Observation::Mixed),
            "both"
        );
        assert_eq!(
            observation(&nothing, &nothing),
            Err(EvidenceError::Unaccounted),
            "nothing at all is refused, not cached"
        );
    }

    #[test]
    fn launch_lines_are_dropped_from_a_failure_message_and_diagnostics_stay() {
        let text = "   Compiling probe v0.1.0 (/x)\n     Running `RUSTFLAGS_MARK=renvor_marker_9f8e7d \
                    /t/bin/rustc --crate-name probe`\nerror[E0425]: cannot find value `x`\n     \
                    Running unittests src/main.rs (/x/deps/probe-1)\n";
        let kept = without_launch_lines(text);
        assert!(
            !kept.contains("renvor_marker"),
            "the launch line reached the failure text"
        );
        assert!(kept.contains("error[E0425]"), "the diagnostic was dropped");
        assert!(
            kept.contains("Running unittests"),
            "a status line that is not a command stays"
        );
    }

    /// What the accounting establishes, and what it cannot — the control C-5's missing-unit
    /// sentence asks for, with the answer it actually produces.
    ///
    /// **(a)** Removing exactly one of several own launches is **not** detectable, and the
    /// measurement says why: a package with one dirty unit and one reused unit prints
    /// `Compiling`/`Checking`, the dirty unit's `Running` line, and **nothing at all** for the
    /// reused one — no `Fresh`, no count, no total (measured 2026-09-08 on cargo 1.97.1 for
    /// `build`, `test`, and `clippy --all-targets`). A stream with a `Running` line removed is
    /// therefore indistinguishable, line for line, from a legitimate partially-fresh run. This
    /// asserts the limit rather than a guarantee, so that a later reading of C-5's sentence finds
    /// the measurement instead of an assumption.
    ///
    /// **(b)** What *is* established, from the same evidence: an announced own package must
    /// launch, an own package that appears nowhere is refused, a stream without `Finished` is
    /// refused, and — new in this round — a stream claiming the own package both `Fresh` and
    /// launched is refused rather than recorded as `mixed`.
    #[test]
    fn one_of_several_own_launches_may_go_missing_and_this_names_the_limit() {
        let launch = |unit: &str| {
            format!(
                "     Running `CARGO_PKG_NAME=probe /t/bin/rustc --crate-name {unit} src/lib.rs`\n"
            )
        };
        let stream = |units: &[&str]| {
            format!(
                "   Compiling probe v0.1.0 (/x)\n{}    Finished `dev` profile\n",
                units.iter().map(|unit| launch(unit)).collect::<String>()
            )
        };

        let whole = parse_check(&stream(&["probe", "probe_bin"]), "probe", Path::new("/x"))
            .expect("the whole stream is evidence");
        assert_eq!(whole.units_launched, 2);

        // (a) THE LIMIT. One launch removed, and the stream still parses — because that is the
        // shape Cargo prints when the unit was reused rather than removed.
        let tampered = parse_check(&stream(&["probe"]), "probe", Path::new("/x"))
            .expect("a stream with one launch removed is indistinguishable from a partial rebuild");
        assert_eq!(
            tampered.units_launched, 1,
            "the count is what the stream says, and the stream cannot say what is missing"
        );

        // (b) WHAT IS ESTABLISHED. Every own launch removed, with the announcement left: refused.
        assert_eq!(
            parse_check(&stream(&[]), "probe", Path::new("/x")),
            Err(EvidenceError::Unaccounted),
            "an announced package with no launch at all passed"
        );
        // And a stream claiming both states at once: refused, never recorded as `mixed`.
        let both = format!(
            "   Compiling probe v0.1.0 (/x)\n{}       Fresh probe v0.1.0 (/x)\n    Finished `dev` \
             profile\n",
            launch("probe")
        );
        assert_eq!(
            parse_check(&both, "probe", Path::new("/x")),
            Err(EvidenceError::Contradictory),
            "a package reported both reused and announced was accepted"
        );

        // AND THE SHAPE THAT IS NOT THAT, which a launch-count predicate refuses by mistake.
        //
        // A fully cached `cargo test -vv` on a crate with a doctest prints `Fresh` and then a
        // `Running` line for the rustdoc unit — own by `CARGO_PKG_NAME` and `--crate-name`, and
        // with no announcement anywhere. Measured 2026-09-08 on cargo 1.97.1; the first version of
        // the check above compared `units_launched` and would have refused this ordinary run,
        // because the measurement it rested on was taken on a crate that had no doc comments.
        let cached_with_a_doctest = concat!(
            "       Fresh probe v0.1.0 (/x)
",
            "    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.00s
",
            "     Running `CARGO_PKG_NAME=probe /t/bin/rustdoc --edition=2021 --crate-name probe              src/lib.rs`
",
        );
        let evidence = parse_check(cached_with_a_doctest, "probe", Path::new("/x"))
            .expect("a cached run with a doctest unit is ordinary evidence, not a contradiction");
        assert_eq!(evidence.units_fresh, 1);
        assert_eq!(
            evidence.units_launched, 1,
            "the rustdoc doctest unit is the project's own by every rule this module applies"
        );
    }

    /// A child that exits at once while a descendant of it holds the write end of the pipe. The
    /// process is gone, so the exit deadline is satisfied immediately; the read is not, and only
    /// a deadline over the collection as well ends the wait.
    ///
    /// The descendant is the test's own, sleeps for a fixed short time, and ends on its own —
    /// nothing here kills a process it did not start, and nothing is left running past it. The
    /// bound under test is a fraction of the descendant's life, so the elapsed-time assertion
    /// distinguishes "bounded" from "waited for the descendant" rather than merely observing an
    /// error.
    #[cfg(unix)]
    #[test]
    fn a_query_whose_descendant_holds_the_pipe_is_bounded_by_the_deadline() {
        const DESCENDANT_SECONDS: u64 = 3;
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(format!("( sleep {DESCENDANT_SECONDS} ) &\nexit 0\n"));
        let started = std::time::Instant::now();
        let failure = answer(&mut command, Duration::from_millis(300))
            .expect_err("the collection is bounded");
        let elapsed = started.elapsed();

        assert_eq!(failure, QueryFailure::Timeout);
        assert!(
            elapsed < Duration::from_secs(DESCENDANT_SECONDS - 1),
            "the collection waited for the descendant instead of the deadline"
        );
    }

    /// The three shapes a launch command takes when it is not one physical line, each carrying a
    /// canary in the part a first-line-only filter left behind: a newline inside a quoted value,
    /// a backtick inside one (which ends a physical line without ending the command), and a
    /// command the stream ends inside.
    ///
    /// The canaries are distinct so a failure names which shape leaked.
    #[cfg(unix)]
    #[test]
    fn no_continuation_of_a_launch_command_reaches_a_failure_message() {
        // EVERY NEWLINE BELOW IS WRITTEN `\n`. A `\` at the end of a Rust string literal eats
        // the newline *and* the next line's indentation, so a fixture laid out to look multiline
        // is one physical line and asserts nothing — which is how the first draft of this test
        // passed against the defect it exists to catch.

        // (a) A newline inside a quoted value: the continuation is the middle of that value.
        let multiline = concat!(
            "   Compiling probe v0.1.0 (/x)\n",
            "     Running `CARGO_PKG_DESCRIPTION='first\n",
            "renvor_canary_newline_a1b2 second' /t/bin/rustc --crate-name probe`\n",
            "error[E0425]: cannot find value `x`\n",
        );
        let kept = without_launch_lines(multiline);
        assert!(
            !kept.contains("renvor_canary_newline"),
            "a continuation of a multiline launch command reached the failure text"
        );
        assert!(kept.contains("error[E0425]"), "the diagnostic was dropped");

        // (b) A backtick inside a quoted value, landing at the end of the first physical line —
        // the `tracing-serde` shape the census found. The first line looks complete and is not.
        let backticked = concat!(
            "     Running `CARGO_PKG_DESCRIPTION='A layer for `\n",
            "renvor_canary_backtick_c3d4`' /t/bin/rustc --crate-name probe`\n",
            "warning: unused variable\n",
        );
        let kept = without_launch_lines(backticked);
        assert!(
            !kept.contains("renvor_canary_backtick"),
            "a backtick inside a quoted argument ended the command early and published the rest"
        );
        assert!(
            kept.contains("warning: unused"),
            "the diagnostic was dropped"
        );

        // (c) A command the stream ends inside: nothing closes it, so nothing of it is kept.
        //
        // The variable is `CARGO_PKG_DESCRIPTION` like the two above, and not the `SECRET=` the
        // first draft used. `SECRET=` followed by a high-entropy word is the shape gitleaks'
        // `generic-api-key` rule keys on, and it failed step 8 of the gate — a test fixture that
        // looks like a credential costs a scan finding for as long as the commit lives, whatever
        // it actually holds. See FP-006 in `.gitleaks.toml`.
        let truncated = "     Running `CARGO_PKG_DESCRIPTION='renvor_canary_truncated_e5f6\n";
        let kept = without_launch_lines(truncated);
        assert!(
            !kept.contains("renvor_canary_truncated"),
            "a truncated launch command was published in pieces"
        );

        // And the two readings of one stream agree about where the launch ended: `parse_check`
        // accounts for (a) as one own unit, which is only possible if it rejoined the same text
        // this dropped.
        let evidence = parse_check(
            &format!("{multiline}    Finished `dev` profile\n"),
            "probe",
            Path::new("/x"),
        )
        .expect("the multiline launch is one accounted unit");
        assert_eq!(evidence.units_launched, 1);
    }

    // ---------------------------------------------------------------------------------------
    // The coupled correction of findings 2 and 3 (PR #72 review round).
    //
    // These fixtures are PLATFORM-NEUTRAL and deliberately ungated. They never touch the
    // filesystem, so a directory name that could not exist on Windows is still exercised there
    // AS PARSER INPUT — which is the distinction that matters: `\n`, `\r` and trailing spaces
    // are invalid in a *Windows path*, but they are ordinary bytes in a *stream this parser
    // reads*, and the parser must agree about them on every platform. `Location::of` on a path
    // that does not exist leaves `canonical` as `None`, and `matches` then compares the raw
    // spelling — a string comparison every platform answers identically. The filesystem-bound
    // counterparts stay `#[cfg(unix)]`, as the tests above already are.
    // ---------------------------------------------------------------------------------------

    /// FINDING 3, status side, WHITESPACE BEFORE A NEWLINE. `str::trim` on the physical line ate
    /// the trailing spaces, so the rejoined location no longer equalled staging: the own
    /// announcement was lost silently on a cold run, and the same directory was refused as
    /// `Unaccounted` on a cached one.
    #[test]
    fn a_status_line_whose_path_ends_with_spaces_before_a_newline_keeps_them() {
        let staging = Path::new("/s/above   \nFORGED-LINE");
        let at = staging.display();
        let cached =
            format!("       Fresh probe v0.1.0 ({at})\n    Finished `dev` profile in 0.0s\n");

        let evidence = parse_check(&cached, "probe", staging)
            .expect("spaces before the newline are part of the name, not Cargo's padding");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// FINDING 3, status side, CARRIAGE RETURN. `str::lines` treats `\r\n` as one terminator and
    /// drops the `\r`, so a `\r` INSIDE a directory name was eaten and the location no longer
    /// equalled staging. Splitting on `'\n'` alone keeps it, because in this stream a `\r` is
    /// operator-controlled data, not a delimiter.
    #[test]
    fn a_status_line_whose_path_contains_a_carriage_return_keeps_it() {
        let staging = Path::new("/s/above\r\nFORGED-LINE");
        let at = staging.display();
        let cached =
            format!("       Fresh probe v0.1.0 ({at})\n    Finished `dev` profile in 0.0s\n");

        let evidence = parse_check(&cached, "probe", staging)
            .expect("a carriage return in a directory name is data, not a line ending");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// FINDING 3, launch side, whitespace before a newline.
    #[test]
    fn a_running_command_whose_path_ends_with_spaces_before_a_newline_keeps_them() {
        let staging = Path::new("/s/above   \nFORGED-LINE");
        let at = staging.display();
        let stream = format!(
            "   Compiling probe v0.1.0 ({at})\n     Running `CARGO_MANIFEST_DIR=\"{at}\" \
             CARGO_PKG_NAME=probe /t/bin/rustc --crate-name probe src/main.rs`\n    Finished \
             `dev` profile in 0.0s\n"
        );

        let evidence = parse_check(&stream, "probe", staging)
            .expect("a legal directory name is not a capture failure");
        assert_eq!(evidence.units_launched, 1);
    }

    /// FINDING 3, launch side, carriage return.
    #[test]
    fn a_running_command_whose_path_contains_a_carriage_return_keeps_it() {
        let staging = Path::new("/s/above\r\nFORGED-LINE");
        let at = staging.display();
        let stream = format!(
            "   Compiling probe v0.1.0 ({at})\n     Running `CARGO_MANIFEST_DIR=\"{at}\" \
             CARGO_PKG_NAME=probe /t/bin/rustc --crate-name probe src/main.rs`\n    Finished \
             `dev` profile in 0.0s\n"
        );

        let evidence = parse_check(&stream, "probe", staging)
            .expect("a carriage return in a directory name is data");
        assert_eq!(evidence.units_launched, 1);
    }

    /// FINDING 2. A same-named renamed path dependency
    /// (`inner = { path = "…", package = "probe" }`) carries `CARGO_PKG_NAME=<own>` on its own
    /// launch. The name arm alone made that the project's own unit, so its count and its chain
    /// reached the provenance record. The location is the discriminator, and it is present here.
    #[test]
    fn a_same_named_dependency_launch_is_not_counted_as_the_projects_own() {
        let staging = Path::new("/s/outer");
        let stream = "   Compiling probe v0.1.0 (/s/outer)\n     Running \
                      `CARGO_MANIFEST_DIR=/s/inner CARGO_PKG_NAME=probe /t/bin/rustc \
                      --crate-name probe src/lib.rs`\n     Running \
                      `CARGO_MANIFEST_DIR=/s/outer CARGO_PKG_NAME=probe /t/bin/rustc \
                      --crate-name probe src/main.rs`\n    Finished `dev` profile in 0.0s\n";

        let evidence = parse_check(stream, "probe", staging).expect("an ordinary build");
        assert_eq!(
            evidence.units_launched, 1,
            "the dependency's launch is not the project's own unit"
        );
        assert_eq!(
            evidence.chains.len(),
            1,
            "and its chain is not queried for identity"
        );
    }

    /// THE INTERACTION, and the reason the two corrections ship together. Finding 2's name arm is
    /// what currently rescues a whitespace-bearing directory whose location no longer matches:
    /// removing it without finding 3's fix turns that silent miscount into `Unaccounted`. Both
    /// causes are present in this one stream.
    #[test]
    fn removing_the_name_arm_does_not_refuse_a_whitespace_bearing_directory() {
        let staging = Path::new("/s/above   \nFORGED-LINE");
        let at = staging.display();
        let stream = format!(
            "   Compiling probe v0.1.0 ({at})\n     Running `CARGO_MANIFEST_DIR=/s/inner \
             CARGO_PKG_NAME=probe /t/bin/rustc --crate-name probe src/lib.rs`\n     Running \
             `CARGO_MANIFEST_DIR=\"{at}\" CARGO_PKG_NAME=probe /t/bin/rustc --crate-name probe \
             src/main.rs`\n    Finished `dev` profile in 0.0s\n"
        );

        let evidence = parse_check(&stream, "probe", staging)
            .expect("finding 3's fix must carry the case finding 2's name arm used to rescue");
        assert_eq!(evidence.units_launched, 1);
    }

    // --- NEGATIVE CONTROLS: what the correction must NOT change -------------------------------

    /// The trailing trim is removed, so this proves whitespace AFTER the closing bracket is still
    /// tolerated — it is Cargo's, not the path's, and `location_of` trims its own end.
    #[test]
    fn trailing_space_after_a_location_is_still_not_part_of_the_path() {
        let cached = "       Fresh probe v0.1.0 (/s)   \n    Finished `dev` profile in 0.0s\n";

        let evidence = parse_check(cached, "probe", Path::new("/s")).expect("an ordinary stream");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// The location is necessary WHEN PRESENT — not always present. A launch that carries the
    /// name but no `CARGO_MANIFEST_DIR` must still be the project's own.
    #[test]
    fn a_launch_without_a_manifest_dir_still_falls_back_to_the_package_name() {
        let stream = "   Compiling probe v0.1.0 (/s)\n     Running `CARGO_PKG_NAME=probe \
                      /t/bin/rustc --crate-name probe src/main.rs`\n    Finished `dev` profile \
                      in 0.0s\n";

        let evidence = parse_check(stream, "probe", Path::new("/s")).expect("an ordinary stream");
        assert_eq!(evidence.units_launched, 1);
    }

    /// And with neither, the crate name still decides.
    #[test]
    fn a_launch_with_neither_name_nor_location_still_falls_back_to_the_crate_name() {
        let stream = "   Compiling probe v0.1.0 (/s)\n     Running `/t/bin/rustc --crate-name \
                      probe src/main.rs`\n    Finished `dev` profile in 0.0s\n";

        let evidence = parse_check(stream, "probe", Path::new("/s")).expect("an ordinary stream");
        assert_eq!(evidence.units_launched, 1);
    }

    /// FINDING 3, third sub-cause, found by this correction rather than before it: an `ESC` is a
    /// legal character in a Unix directory name, and stripping SGR from the WHOLE line deleted it
    /// from the path as well as from Cargo's colour.
    ///
    /// This is a PARSER-FIDELITY test, and deliberately so: the fixture carries the escape in the
    /// stream, which is the one thing this parser controls. Real Cargo does not print it — see
    /// [`Location::is_comparable`] for the measurement — so the shape below is what the parser
    /// must not corrupt if it is ever handed it, not what Cargo was observed to emit.
    #[test]
    fn a_status_line_whose_path_contains_an_escape_sequence_keeps_it() {
        let esc = "\u{1b}";
        let staging = PathBuf::from(format!("/s/{esc}[31mRED{esc}[0m"));
        let at = staging.display();
        let cached =
            format!("       Fresh probe v0.1.0 ({at})\n    Finished `dev` profile in 0.0s\n");

        let evidence = parse_check(&cached, "probe", &staging)
            .expect("an escape sequence in a directory name is data, not colour");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// And the two causes together: a path carrying BOTH a newline and an escape sequence, so the
    /// escape lands in a CONTINUATION line, which is why continuations are joined raw.
    #[test]
    fn a_path_with_both_a_newline_and_an_escape_sequence_survives_rejoining() {
        let esc = "\u{1b}";
        let staging = PathBuf::from(format!("/s/above\nFORGED{esc}[31mRED{esc}[0m"));
        let at = staging.display();
        let cached =
            format!("       Fresh probe v0.1.0 ({at})\n    Finished `dev` profile in 0.0s\n");

        let evidence = parse_check(&cached, "probe", &staging)
            .expect("a continuation is data and is joined without stripping");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// THE CONTROL FOR ALL THREE. Cargo's own colour, which brackets the status word, must still
    /// be removed — otherwise "keep the operator's bytes" would have broken every coloured
    /// stream. Paired with [`colour_sequences_around_the_status_word_are_ignored`].
    #[test]
    fn cargos_own_colour_around_the_status_word_is_still_removed() {
        let esc = "\u{1b}";
        let cached = format!(
            "{esc}[1m{esc}[32m       Fresh{esc}[0m probe v0.1.0 (/s)\n{esc}[1m{esc}[32m    \
             Finished{esc}[0m `dev` profile in 0.0s\n"
        );

        let evidence = parse_check(&cached, "probe", Path::new("/s"))
            .expect("colour around the status word is Cargo's, not the path's");
        assert_eq!(evidence.units_fresh, 1);
    }

    /// The narrow case the correction does NOT close, asserted so it cannot regress silently.
    /// A staging path carrying an `ESC` has no reliable printed spelling, so the location cannot
    /// discriminate and the package name decides — which is what every path did before this
    /// change. Finding 2's bound therefore persists here, and only here.
    #[test]
    fn a_staging_path_cargo_cannot_print_losslessly_still_falls_back_to_the_name() {
        let esc = "\u{1b}";
        let staging = PathBuf::from(format!("/s/{esc}[31mRED{esc}[0m"));
        // Cargo prints the sanitised spelling, which equals no path on disk.
        let stream = "   Compiling probe v0.1.0 (/s/RED)\n     Running \
                      `CARGO_MANIFEST_DIR=/s/RED CARGO_PKG_NAME=probe /t/bin/rustc --crate-name \
                      probe src/main.rs`\n    Finished `dev` profile in 0.0s\n";

        let evidence = parse_check(stream, "probe", &staging)
            .expect("the name is the only test available for an unprintable staging path");
        assert_eq!(evidence.units_launched, 1);
    }

    /// A Windows-shaped location goes through the same rules, on every platform.
    #[test]
    fn a_windows_shaped_location_is_read_by_the_same_rules() {
        let cached =
            "       Fresh probe v0.1.0 (C:\\s\\project)\n    Finished `dev` profile in 0.0s\n";

        let evidence = parse_check(cached, "probe", Path::new("C:\\s\\project"))
            .expect("a Windows-shaped path is an ordinary location");
        assert_eq!(evidence.units_fresh, 1);
    }
}
