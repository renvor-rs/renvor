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
//! is malformed; a stream in which the own package never appears is unaccounted. Each is an
//! [`EvidenceError`], which the caller reports as `project_verification_failed` with
//! `reason = evidence_capture_failed` — never as a cached run.
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
    let mut lines = stderr.lines();
    while let Some(raw) = lines.next() {
        let line = without_sgr(raw);
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Running ") {
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
        } else if let Some(rest) = line.strip_prefix("Fresh ") {
            if package_of(rest) == own_package {
                evidence.units_fresh += 1;
            }
        } else if let Some(rest) = line
            .strip_prefix("Compiling ")
            .or_else(|| line.strip_prefix("Checking "))
        {
            if package_of(rest) == own_package {
                announced = true;
            }
        } else if line.starts_with("Finished ") {
            finished = true;
        }
    }
    if !finished {
        return Err(EvidenceError::Truncated);
    }
    if announced || (evidence.units_launched == 0 && evidence.units_fresh == 0) {
        return Err(EvidenceError::Unaccounted);
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

/// A check's stderr without its `Running` launch lines, for a failure message.
///
/// A launch line is Cargo's rendering of a whole command — every environment variable it set,
/// every `RUSTFLAGS`-derived argument, every path — and a failing check's message embeds the
/// check's output. The diagnostics stay; the launch lines, which exist for [`parse_check`] and
/// say nothing about why the check failed, do not reach a stream.
#[must_use]
pub fn without_launch_lines(stderr: &str) -> String {
    let mut kept = String::with_capacity(stderr.len());
    for raw in stderr.lines() {
        let line = without_sgr(raw);
        let trimmed = line.trim_start();
        if trimmed
            .strip_prefix("Running ")
            .is_some_and(|rest| rest.starts_with('`'))
        {
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
    let text = answer(&mut command).map_err(|failure| unreadable(QUERY, failure))?;
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
    let text = answer(&mut command).map_err(|failure| unreadable(QUERY, failure))?;
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
/// its own thread, and waits at most [`QUERY_TIMEOUT`] — the reader-and-deadline shape
/// `commands::relay` uses, so a child that never exits or never stops writing is bounded either
/// way.
fn answer(command: &mut Command) -> Result<String, QueryFailure> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| QueryFailure::Spawn)?;
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut collected = Vec::new();
        if let Some(mut stream) = stdout {
            // One byte past the bound so an over-long answer is detectable, then the rest is
            // drained and discarded so the child is never blocked on a full pipe.
            let mut limited = (&mut stream).take(MAX_ANSWER_BYTES + 1);
            let _ = limited.read_to_end(&mut collected);
            let _ = std::io::copy(&mut stream, &mut std::io::sink());
        }
        collected
    });
    let status = match child.wait_timeout(QUERY_TIMEOUT) {
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
    let collected = reader.join().map_err(|_| QueryFailure::Spawn)?;
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
    let own = package_name == Some(own_package)
        || manifest_dir.is_some_and(|dir| staging.matches(dir))
        || (package_name.is_none() && manifest_dir.is_none() && crate_name == own_crate);
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
        joined.push_str(without_sgr(next).as_ref());
        if whole(&joined) {
            break;
        }
    }
    Some(joined)
}

/// Whether a backtick-opened remainder is a complete, tokenisable command.
fn whole(rest: &str) -> bool {
    rest.len() > 1 && rest.ends_with('`') && tokenize(&rest[1..rest.len() - 1]).is_some()
}

fn backticked(rest: &str) -> Result<Option<&str>, EvidenceError> {
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

    #[test]
    fn a_fresh_own_package_is_counted_and_a_fresh_dependency_is_not() {
        let staging = tempfile::tempdir().expect("tempdir");
        let text = "       Fresh serde v1.0.0\n       Fresh probe v0.1.0 (/x)\n    Finished \
                    `dev` profile [unoptimized + debuginfo] target(s) in 0.00s\n";
        let evidence = parse_check(text, "probe", staging.path()).expect("parses");
        assert_eq!(evidence.units_fresh, 1, "one own fresh report");
        assert_eq!(evidence.units_launched, 0, "no launch");
        assert!(evidence.chains.is_empty(), "no chain");
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
        let text = "\u{1b}[1m\u{1b}[32m       Fresh\u{1b}[0m probe v0.1.0 (/x)\n\u{1b}[1m\u{1b}[32m    \
                    Finished\u{1b}[0m `dev` profile in 0.0s\n";
        let evidence = parse_check(text, "probe", staging.path()).expect("parses");
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
}
