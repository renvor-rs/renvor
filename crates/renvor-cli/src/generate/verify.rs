//! Pre-placement verification (FR-030).
//!
//! > *"Generation MUST verify the project before reporting success, so that a project that does not
//! > build is a generation failure rather than a user's discovery."*
//!
//! # Where this runs, and why that is the whole point
//!
//! **In staging, before the rename.** A project that fails its own checks never reaches the
//! destination, so a failed generation leaves nothing to clean up and nothing to explain.
//! Verifying after placement would turn a generator bug into the operator's problem.
//!
//! # This step deliberately steps outside the capability boundary, and says so
//!
//! Every other filesystem operation in this crate goes through a [`cap_std::fs::Dir`] handle.
//! This one cannot: `std::process::Command` takes a **path** for its working directory, and there
//! is no capability-based process API on any supported platform.
//!
//! So the exception is bounded and stated rather than quietly taken:
//!
//! - the path is `<parent the operator typed>/<staging name this process generated>`, not anything
//!   derived from a template or from configuration;
//! - the programs run are fixed string literals, never interpolated;
//! - no argument comes from user input.
//!
//! # Build output does not become part of the project
//!
//! `CARGO_TARGET_DIR` is redirected to a temporary directory **outside** staging — or, since
//! Phase 011, to an **absolute** `CARGO_TARGET_DIR` the environment already carries; a relative
//! one is refused (see [`target_directory`]). Without that, `target/` would be renamed into the
//! destination along with the project and would appear in the manifest — several hundred
//! megabytes of build artifacts presented as generated source. The test module asserts the
//! manifest is byte-identical before and after verification.
//!
//! # Offline
//!
//! FR-043. The generated skeleton declares **no dependencies**, so `cargo build` resolves nothing
//! from the network. That is a property of the template rather than of a network stub, and the
//! template test suite is what keeps it true.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::exit::{CliError, Code};
use crate::output::progress::Progress;
use crate::toolchain::evidence::{self, CheckEvidence, EvidenceError};
pub use crate::toolchain::evidence::{ClippyEvidence, UnitEvidence, Verified};

/// The checks run before the smoke run, in order, each with the failure it reports.
///
/// Ordered cheapest-first so the common failure is reported in a second rather than in a minute.
/// The fifth check — that the project **starts** — depends on the project's shape and is chosen
/// by [`Smoke`].
///
/// # `-vv` on the three that can launch a compiler (Phase 012, FR-012-7d (a))
///
/// The flag changes what Cargo **prints**, not what it compiles, runs, or requires: at `-vv`
/// every launch is one `Running` line on stderr and every reuse one `Fresh` line, which
/// [`crate::toolchain::evidence`] reads for the record's launch observation. `fmt` launches no
/// compiler and `cargo run --quiet` launches nothing after `build`, so neither carries it.
const CHECKS: [(&str, &[&str], &str); 4] = [
    ("cargo", &["fmt", "--check"], "is not correctly formatted"),
    // FR-029 names FOUR things — "formatting, **linting**, building, and testing" — and until
    // 2026-08-18 this array had three. Nothing lint-checked the generated project: not this
    // verifier, not `tests/generated.rs`, not CI, and not a `[lints]` table in the generated
    // manifest. `grep -rn clippy crates/renvor-cli/` returned nothing, while Phase 003 tasks T036
    // (https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/tasks.md)
    // and Phase 003 quickstart Gate 5
    // (https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/quickstart.md)
    // both stated that clippy ran. An advisory review found the gap by
    // reading the array instead of the prose.
    //
    // `-D warnings` because SC-005 says "0 warnings escalated to errors", which is only meaningful
    // if they are escalated.
    // `--all-targets` since Phase 011: the generated tests are generated code too, and FR-041
    // holds them to the same lints. Found by `renvor generate resource`'s proof running clippy
    // over a placed project's tests, which the binary-only check had never linted.
    (
        "cargo",
        &["clippy", "--all-targets", "-vv", "--", "-D", "warnings"],
        "does not pass its own lints",
    ),
    ("cargo", &["build", "-vv"], "does not compile"),
    ("cargo", &["test", "-vv"], "does not pass its own tests"),
];

/// How the fifth check — FR-029's "and MUST start", contract C-5 step 5 — is run.
///
/// The two generated shapes start differently, and one command cannot prove both:
///
/// - a **skeleton**'s `main` prints its name and exits, so the bare run terminates and its exit
///   status is the proof;
/// - a **starter**'s `main` is a server. Run bare it would block until the deadline, and to serve
///   it needs `RENVOR_DATABASE_URL`, which generation must not require and must never invent. So
///   a starter is sent the inspection request `renvor routes` sends, which it answers from its
///   route registry **before** Boot and without a database — proving the binary starts, builds
///   every provider and route, and exits, with no service and no credential in the picture.
///
/// Phase 011 (FR-011). A template that made either run block would hang generation, which is
/// why the skeleton suite keeps `main` trivial and the starter answers the request before Boot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Smoke {
    /// `cargo run --quiet`: the binary must exit successfully on its own.
    Exits,
    /// `cargo run --quiet -- --renvor-dump-routes`: the binary must answer the route dump.
    AnswersDumpRequest,
}

impl Smoke {
    /// The check this variant runs, in the same shape as [`CHECKS`].
    fn check(self) -> (&'static str, &'static [&'static str], &'static str) {
        match self {
            Self::Exits => ("cargo", &["run", "--quiet"], "does not start"),
            Self::AnswersDumpRequest => (
                "cargo",
                &["run", "--quiet", "--", "--renvor-dump-routes"],
                "does not start, or does not answer the route dump request `renvor routes` sends",
            ),
        }
    }
}

/// Where the verification build lands: never inside staging, either way.
#[derive(Debug)]
pub enum TargetDirectory {
    /// An absolute `CARGO_TARGET_DIR` the environment supplied (Phase 011, FR-007). Honoured
    /// because an operator who set it expects every build to land there, and because the matrix
    /// tests share one so four starters do not pay four cold builds.
    Configured(PathBuf),
    /// A temporary directory this process owns, removed with it. The default.
    Temporary(tempfile::TempDir),
}

impl TargetDirectory {
    /// The path cargo is told about.
    fn path(&self) -> &Path {
        match self {
            Self::Configured(path) => path,
            Self::Temporary(directory) => directory.path(),
        }
    }
}

/// Decides where the verification build lands from the environment's `CARGO_TARGET_DIR`.
///
/// A **relative** value is refused rather than ignored: cargo resolves it against the working
/// directory, which here is staging, so the build output would land inside the project and be
/// renamed into the destination — the exact outcome this module exists to prevent. Ignoring it
/// would be a silent fallback; honouring it would be a silent defect.
///
/// # Errors
///
/// [`Code::ProjectVerificationFailed`] naming `CARGO_TARGET_DIR` for a relative or empty value,
/// or when no temporary directory can be created.
pub fn target_directory(configured: Option<&OsStr>) -> Result<TargetDirectory, CliError> {
    match configured {
        Some(value) if !value.is_empty() && Path::new(value).is_absolute() => {
            Ok(TargetDirectory::Configured(PathBuf::from(value)))
        }
        Some(_) => Err(CliError::new(
            Code::ProjectVerificationFailed,
            "`CARGO_TARGET_DIR` is set to a relative or empty path; generation honours only an \
             absolute one, because a relative one would put the verification build inside the \
             generated project. Unset it or make it absolute",
        )
        .with("check", "CARGO_TARGET_DIR is absolute")
        .with("stage", "pre-placement verification")),
        None => tempfile::tempdir()
            .map(TargetDirectory::Temporary)
            .map_err(|error| {
                CliError::new(
                    Code::ProjectVerificationFailed,
                    format!("a build directory for verification could not be created: {error}"),
                )
                .with("stage", "pre-placement verification")
            }),
    }
}

/// The environment variables verification passes to the staged project's checks.
///
/// Everything else the operator's shell carries is dropped — every `RENVOR_*` in particular.
/// Phase 011 found the generated test honouring the gate's `RENVOR_TEST_REQUIRE_DATABASE=1`
/// **inside generation**, and the same inheritance would have handed a `RENVOR_DATABASE_URL` in
/// the operator's shell to a staged `cargo test`, which is a database generation must never
/// reach. The list is what cargo, rustup, rustc, and a registry fetch need; it is a list rather
/// than a pattern so that a new secret-bearing variable is excluded by default.
const PASSED_THROUGH: &[&str] = &[
    // Locating and running the toolchain.
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "TMPDIR",
    "TEMP",
    "TMP",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TERM",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    // `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` were here until Phase 012 (FR-012-6). They
    // tell rustup where to download from, and the seal never needs a download: it forces
    // `RUSTUP_AUTO_INSTALL=0` (see `Sealed::environment`), so a pinned-but-absent toolchain is a
    // refusal by name rather than an installation from inside generation (SR-012-1). The
    // isolated probes of FR-012-7a set both to an unroutable loopback address themselves.
    "RUSTC",
    "RUSTC_WRAPPER",
    // The operator's trust, like `RUSTC_WRAPPER` (SR-012-3): a build cache or a lint wrapper
    // configured through it must keep working, and its PRESENCE is what the record keeps.
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "CARGO_BUILD_JOBS",
    "CARGO_INCREMENTAL",
    "CARGO_NET_OFFLINE",
    "CARGO_NET_GIT_FETCH_WITH_CLI",
    "CARGO_NET_RETRY",
    "CARGO_HTTP_CAINFO",
    "CARGO_HTTP_PROXY",
    "CARGO_HTTP_TIMEOUT",
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
    "CARGO_TERM_COLOR",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    // Windows.
    "SystemRoot",
    "SystemDrive",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "PATHEXT",
    "COMSPEC",
    "windir",
];

///
/// A pure function of its input, so a test can prove the seal without touching the process
/// environment — which this crate forbids `unsafe` code to do anyway.
/// The proxy variables the seal passes through — every one with its credential removed.
const PROXY_VARIABLES: &[&str] = &[
    "CARGO_HTTP_PROXY",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "http_proxy",
    "https_proxy",
];

/// The variable the seal forces, and its value (FR-012-6): rustup 1.28.0 introduced it, and `0`
/// is what makes a rustup proxy answer "is not installed" instead of downloading.
const FORCED_NO_INSTALL: (&str, &str) = ("RUSTUP_AUTO_INSTALL", "0");

/// The two install-server variables no child ever receives from the seal (FR-012-6). They are
/// not in [`PASSED_THROUGH`], so `seal` never keeps them; [`Sealed::environment`] removes them
/// again so a `Sealed` built by hand cannot carry them either.
const NEVER_PASSED: [&str; 2] = ["RUSTUP_DIST_SERVER", "RUSTUP_UPDATE_ROOT"];

/// The sealed environment, and the credentials the seal removed from it — so the output of a
/// child that somehow learned one can be redacted before it is reported.
pub struct Sealed {
    /// The variables the checks run with: the pass-through, credentials stripped.
    ///
    /// **Not the whole environment a child receives.** The forced `RUSTUP_AUTO_INSTALL=0` is
    /// added by [`Sealed::environment`], which is what [`sealed_command`] applies; a site that
    /// copies this list into a `Command` by hand has bypassed the seal's no-provisioning
    /// guarantee. The list is kept as the pass-through alone so a test can assert exactly what
    /// the operator's shell contributed, in the operator's order.
    pub variables: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    /// Every `user:password` removed from a proxy value.
    pub credentials: Vec<String>,
}

impl Sealed {
    /// Everything a child receives (FR-012-6): the pass-through, then the forced
    /// `RUSTUP_AUTO_INSTALL=0` — appended after the pass-through and replacing any value with
    /// that name — with `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` absent whatever the
    /// caller set.
    #[must_use]
    pub fn environment(&self) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
        let mut environment: Vec<(std::ffi::OsString, std::ffi::OsString)> = self
            .variables
            .iter()
            .filter(|(name, _)| {
                let Some(text) = name.to_str() else {
                    return true;
                };
                !same_variable_name(text, FORCED_NO_INSTALL.0)
                    && !NEVER_PASSED
                        .iter()
                        .any(|never| same_variable_name(text, never))
            })
            .cloned()
            .collect();
        environment.push((
            std::ffi::OsString::from(FORCED_NO_INSTALL.0),
            std::ffi::OsString::from(FORCED_NO_INSTALL.1),
        ));
        environment
    }
}

/// The one way a tool child is built in this crate (FR-012-6): `program` in `cwd`, with the
/// parent's environment cleared and exactly [`Sealed::environment`] in its place — the
/// pass-through, `RUSTUP_AUTO_INSTALL=0` forced, the install-server variables absent.
///
/// Every site that runs `cargo`, `rustc`, `rustfmt`, `rustup`, or a toolchain executable goes
/// through here — the five checks, `generate resource`'s `rustfmt`, `doctor`'s probes, and the
/// FR-012-7a/7b/7d probes — so the no-provisioning guarantee is one function rather than a
/// convention each site remembers. The isolated probes of FR-012-7a build on it and add their
/// own directories and the loopback server address.
///
/// `program` is a located path or a fixed literal; nothing here comes from user input.
#[must_use]
pub fn sealed_command(program: &OsStr, sealed: &Sealed, cwd: &Path) -> Command {
    let mut command = Command::new(program);
    command
        .current_dir(cwd)
        .env_clear()
        .envs(sealed.environment());
    command
}

/// Whether two environment-variable names are the same name.
///
/// Case-sensitive everywhere except Windows, whose environment block is case-insensitive and
/// which spells the search path `Path`. A case-sensitive comparison there dropped `PATH` from the
/// seal entirely, so [`crate::toolchain::locate`] could not find a compiler and every generation
/// refused with `tool_missing` — found by the Windows platform legs on 2026-09-08, which the
/// macOS and Linux gates cannot see. The checks themselves had survived it only because
/// `Command::new("cargo")` resolves its program against the *parent's* path, not the child's.
#[must_use]
pub fn same_variable_name(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

/// Seals `parent`: keeps the variables [`PASSED_THROUGH`] names, in order, and strips the
/// `user:password@` a proxy URL may carry.
///
/// # Why the credential goes and the host stays
///
/// A proxy variable passes through so a fetch can route; its URL may carry a credential, and the
/// seal handed that to every build script and dependency the staged project compiles, which
/// contradicted the "no credential" the transaction contract promises (found by the Standards
/// review of Phase 011). Verification needs no registry update — the framework's lockfile seeds
/// resolution (FR-006) — so an authenticated proxy is not something it has to be able to use;
/// the host is kept so a proxy that needs no credential still routes. A proxy value that is not
/// text is dropped rather than guessed at.
#[must_use]
pub fn seal(parent: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>) -> Sealed {
    let mut variables = Vec::new();
    let mut credentials = Vec::new();
    for (name, value) in parent {
        let Some(text_name) = name.to_str() else {
            continue;
        };
        if !PASSED_THROUGH
            .iter()
            .any(|allowed| same_variable_name(allowed, text_name))
        {
            continue;
        }
        if PROXY_VARIABLES
            .iter()
            .any(|proxy| same_variable_name(proxy, text_name))
        {
            let Some(text) = value.to_str() else {
                continue;
            };
            let (stripped, credential) = without_proxy_credential(text);
            if let Some(credential) = credential {
                credentials.push(credential);
            }
            variables.push((name, std::ffi::OsString::from(stripped)));
        } else {
            variables.push((name, value));
        }
    }
    Sealed {
        variables,
        credentials,
    }
}

/// `scheme://user:password@host…` → (`scheme://host…`, `Some("user:password")`); a value without
/// a credential comes back unchanged with `None`.
fn without_proxy_credential(value: &str) -> (String, Option<String>) {
    let (scheme, rest) = match value.find("://") {
        Some(at) => (&value[..at + 3], &value[at + 3..]),
        None => ("", value),
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    match authority.rfind('@') {
        Some(at) => (
            format!("{scheme}{}{}", &authority[at + 1..], &rest[authority_end..]),
            Some(authority[..at].to_owned()),
        ),
        None => (value.to_owned(), None),
    }
}

/// A child's output, fit to report: every `user:password@` in a URL replaced, every credential
/// the seal removed replaced, and every control character escaped so a build script cannot
/// reprogram the operator's terminal.
fn redacted_output(text: &str, credentials: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let authority_end = tail
            .find(|c: char| c == '/' || c.is_whitespace() || c == '"' || c == '\'')
            .unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        match authority.rfind('@') {
            Some(user_end) => {
                out.push_str("<credential removed>@");
                out.push_str(&authority[user_end + 1..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[authority_end..];
    }
    out.push_str(rest);
    for credential in credentials {
        if !credential.is_empty() {
            out = out.replace(credential, "<credential removed>");
            if let Some((_, password)) = credential.split_once(':')
                && !password.is_empty()
            {
                out = out.replace(password, "<credential removed>");
            }
        }
    }
    crate::output::redact::for_terminal(&out)
}

/// Runs the generated project's own checks while it is still in staging, and returns what was
/// observed and queried while they passed (FR-012-7d).
///
/// # Errors
///
/// [`Code::ProjectVerificationFailed`] if any check fails — the project was generated and is
/// **wrong**, which is a generation failure — or [`Code::ToolMissing`] if `cargo` cannot be run at
/// all. Since Phase 012, the same code with `details.reason = evidence_capture_failed` when a
/// passing check's `-vv` stream cannot be accounted for (FR-012-7d (e)), and with
/// `details.reason = compiler_identity_unreadable` when an executable Cargo launched does not
/// answer its version query under FR-012-7e's grammar. Neither is ever reported as a cached run,
/// and nothing is placed either way.
///
/// # Not `render_failed`, which it was until 2026-08-18
///
/// That code is published as *"template rendering failed"*, and rendering is exactly what did
/// **not** fail here: the templates produced a tree, and the tree then failed its own build, lint,
/// format, test, or start check. A consumer matching the registry would have looked for a template
/// defect over a compile error. This is the same class of misreporting as A-R6's three sites, found
/// in the same sweep and corrected with them.
#[cfg(test)]
pub fn in_staging(staging: &Path, progress: &Progress, smoke: Smoke) -> Result<Verified, CliError> {
    in_staging_with(staging, progress, smoke, std::env::vars_os())
}

/// The five checks in `staging`, with the parent environment **given rather than read**.
///
/// Every caller passes one snapshot of `std::env::vars_os()` here and seals the same snapshot for
/// the preflight of FR-012-7a/7b, so the identification, the resolution, and the checks cannot be
/// measuring two different shells; the sealed-environment controls pass a shell they shaped.
///
/// # What comes back, and where each piece is from
///
/// Every check ran and passed. For clippy, build, and test the stderr of that run — and nothing
/// else: no cache file, no earlier record, no earlier stream — was parsed into the project's own
/// units launched and reused; the last executable of the build/test chains was asked `-vV` and
/// the `clippy-driver` of the clippy chains `--version`, each under the same seal in the same
/// directory. What was queried is what the launched executable answered; what was observed is
/// what Cargo printed. Neither is proof that a wrapper executed that compiler (SR-012-3).
///
/// # Errors
///
/// [`Code::ProjectVerificationFailed`] when a check fails, when a passing check's `-vv` stream
/// cannot be accounted for, or when a launched executable does not answer its version query under
/// FR-012-7e's grammar; [`Code::ToolMissing`] when a tool cannot be started. Neither is ever
/// reported as a cached run, and nothing is placed either way.
pub fn in_staging_with(
    staging: &Path,
    progress: &Progress,
    smoke: Smoke,
    parent: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Result<Verified, CliError> {
    let parent: Vec<(std::ffi::OsString, std::ffi::OsString)> = parent.collect();
    let configured_target = parent
        .iter()
        .find(|(name, _)| {
            name.to_str()
                .is_some_and(|text| same_variable_name(text, "CARGO_TARGET_DIR"))
        })
        .map(|(_, value)| value.clone());
    let target = target_directory(configured_target.as_deref())?;
    let sealed = seal(parent.into_iter());
    // The name the `-vv` stream's `CARGO_PKG_NAME` and `Fresh` lines are matched against: the
    // manifest the generator itself rendered, parsed and nothing more.
    let own_package = own_package_name(staging)?;

    let mut captured = Captured::default();
    for (program, arguments, complaint) in CHECKS.into_iter().chain(std::iter::once(smoke.check()))
    {
        // WHICH check is running, not merely THAT something is. `.output()` captures everything
        // cargo says, so without this the operator watches a spinner for a cold `cargo build` with
        // no way to tell a slow compile from a hung one — and no way to know, when it does hang,
        // which of the five to reproduce by hand.
        let label = format!("{program} {}", arguments.join(" "));
        progress.step(&label);
        // SEALED: `sealed_command` is the one way a tool child is built (FR-012-6). The build
        // directory is set explicitly, after the seal, so the operator's `CARGO_TARGET_DIR`
        // reaches cargo through `target` alone.
        let output = sealed_command(OsStr::new(program), &sealed, staging)
            .args(arguments)
            .env("CARGO_TARGET_DIR", target.path())
            .output()
            .map_err(|error| {
                CliError::new(
                    Code::ToolMissing,
                    format!(
                        "`{program}` could not be run to verify the generated project: {error}"
                    ),
                )
                .with("tool", program)
                .with("required", "true")
                .with("found", "false")
            })?;

        if !output.status.success() {
            // The generated project is the defect, not the operator's input. Reporting the tool's
            // own output is what makes that actionable — a bare "verification failed" would send
            // somebody to re-read their flags for a bug in a template.
            //
            // BOTH STREAMS. `rustfmt --check` writes its diff to stdout and `cargo` its errors to
            // stderr; a message built from stderr alone reported a formatting failure with no
            // diff (Phase 011's first starter render), which is the bare message this exists to
            // avoid. Stdout first, because for the one check that uses it the diff is the answer.
            // REDACTED, not raw: a build script's or a compiler's output is text nobody
            // reviewed — every URL credential is removed, every credential the seal took out of
            // a proxy value is removed, every control character is escaped.
            // WITHOUT THE `-vv` LAUNCH LINES (Phase 012): each is Cargo's rendering of a whole
            // command — every variable it set, every `RUSTFLAGS`-derived argument, every path —
            // and says nothing about why the check failed. The diagnostics that do stay.
            let stdout = redacted_output(
                &String::from_utf8_lossy(&output.stdout),
                &sealed.credentials,
            );
            let stderr = redacted_output(
                &evidence::without_launch_lines(&String::from_utf8_lossy(&output.stderr)),
                &sealed.credentials,
            );
            let detail = format!("{}\n{}", stdout.trim(), stderr.trim());
            return Err(CliError::new(
                Code::ProjectVerificationFailed,
                format!(
                    "the generated project {complaint}; nothing was written to the destination. \
                     This is a defect in renvor's templates, not in your command.\n{}",
                    detail.trim()
                ),
            )
            .with("check", label)
            .with("stage", "pre-placement verification"));
        }

        // THE EVIDENCE OF THIS CHECK IS THIS CHECK'S STDERR. Parsed now, before the next check
        // runs, from the bytes the child wrote to this pipe — never from a file.
        if let Some(slot) = captured.slot(arguments.first().copied().unwrap_or("")) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            *slot = Some(
                evidence::parse_check(&stderr, &own_package, staging)
                    .map_err(|error| capture_failed(&label, error))?,
            );
        }
    }

    let (Some(clippy), Some(build), Some(test)) = (captured.clippy, captured.build, captured.test)
    else {
        return Err(CliError::new(
            Code::Internal,
            "verification finished without running every check it records evidence for",
        ));
    };

    // THE DRIVER'S OWN ANSWER (FR-012-7d (b), §4.3.2): the `clippy-driver` executable the
    // `Running` line named, asked `--version` itself. Only when a unit was launched — a cached
    // clippy check records no driver — and never from `cargo clippy --version`.
    let driver = if clippy.units_launched > 0 {
        progress.step("clippy-driver --version");
        let drivers = distinct(
            clippy
                .chains
                .iter()
                .filter_map(evidence::clippy_driver)
                .map(|driver| (driver, CLIPPY_LABEL)),
        );
        if drivers.is_empty() {
            return Err(capture_failed(CLIPPY_LABEL, EvidenceError::NoDriver));
        }
        let mut identities = Vec::with_capacity(drivers.len());
        for (driver, label) in drivers {
            identities.push(
                evidence::query_driver(driver, &sealed, staging)
                    .map_err(|error| error.with("check", label))?,
            );
        }
        if identities.windows(2).any(|pair| pair[0] != pair[1]) {
            return Err(capture_failed(CLIPPY_LABEL, EvidenceError::Disagreement));
        }
        identities.into_iter().next()
    } else {
        None
    };

    // THE QUERIED IDENTITY (FR-012-7d (a)): the last executable of every launched build/test
    // chain, asked `-vV` itself. One identity per check is representable; distinct executables
    // are each asked, and a disagreement is a capture failure rather than a choice.
    let compilers = distinct(
        build
            .chains
            .iter()
            .filter_map(evidence::trailing_compiler)
            .map(|compiler| (compiler, BUILD_LABEL))
            .chain(
                test.chains
                    .iter()
                    .filter_map(evidence::trailing_compiler)
                    .map(|compiler| (compiler, TEST_LABEL)),
            ),
    );
    let rustc = if compilers.is_empty() {
        None
    } else {
        progress.step("rustc -vV");
        let mut identities = Vec::with_capacity(compilers.len());
        for (compiler, label) in compilers {
            identities.push(
                evidence::query_rustc(compiler, &sealed, staging)
                    .map_err(|error| error.with("check", label))?,
            );
        }
        if identities.windows(2).any(|pair| pair[0] != pair[1]) {
            return Err(capture_failed(
                "cargo build -vv, cargo test -vv",
                EvidenceError::Disagreement,
            ));
        }
        identities.into_iter().next()
    };

    let observation = evidence::observation(&build, &test)
        .map_err(|error| capture_failed("cargo build -vv, cargo test -vv", error))?;
    let cached_checks: Vec<&'static str> =
        [("clippy", &clippy), ("build", &build), ("test", &test)]
            .into_iter()
            .filter(|(_, check)| check.units_launched == 0 && check.units_fresh > 0)
            .map(|(name, _)| name)
            .collect();
    let wrapper_observed = clippy
        .chains
        .iter()
        .any(|chain| evidence::shows_wrapper(chain, true))
        || build
            .chains
            .iter()
            .chain(test.chains.iter())
            .any(|chain| evidence::shows_wrapper(chain, false));

    Ok(Verified {
        fmt: true,
        clippy: ClippyEvidence {
            units_launched: clippy.units_launched,
            units_fresh: clippy.units_fresh,
            driver,
        },
        build: UnitEvidence::from(&build),
        test: UnitEvidence::from(&test),
        run: true,
        observation,
        rustc,
        cached_checks,
        wrapper_observed,
    })
}

/// The check labels the evidence errors name, exactly as [`CHECKS`] spells them.
const CLIPPY_LABEL: &str = "cargo clippy --all-targets -vv -- -D warnings";
const BUILD_LABEL: &str = "cargo build -vv";
const TEST_LABEL: &str = "cargo test -vv";

/// The three checks' evidence as the loop fills them, by the check's first argument.
#[derive(Default)]
struct Captured {
    clippy: Option<CheckEvidence>,
    build: Option<CheckEvidence>,
    test: Option<CheckEvidence>,
}

impl Captured {
    /// The slot for a check, or `None` for the two that carry no evidence (`fmt`, `run`).
    fn slot(&mut self, check: &str) -> Option<&mut Option<CheckEvidence>> {
        match check {
            "clippy" => Some(&mut self.clippy),
            "build" => Some(&mut self.build),
            "test" => Some(&mut self.test),
            _ => None,
        }
    }
}

/// The distinct executables of an iterator, first label kept, in first-seen order.
fn distinct<'a>(
    executables: impl Iterator<Item = (&'a Path, &'static str)>,
) -> Vec<(&'a Path, &'static str)> {
    let mut seen: Vec<(&'a Path, &'static str)> = Vec::new();
    for (executable, label) in executables {
        if !seen.iter().any(|(known, _)| *known == executable) {
            seen.push((executable, label));
        }
    }
    seen
}

/// The `evidence_capture_failed` refusal (FR-012-7d (e)) for one check. Fixed vocabulary: the
/// check's name, the failure class, nothing from the stream.
fn capture_failed(check: &str, error: EvidenceError) -> CliError {
    CliError::new(
        Code::ProjectVerificationFailed,
        format!(
            "the verification of the generated project produced no usable launch evidence for \
             `{check}`: {error}; nothing was written to the destination. A stream that cannot \
             be accounted for is a capture failure, never a cached run"
        ),
    )
    .with("reason", evidence::REASON_CAPTURE_FAILED)
    .with("check", check)
    .with("cause", error.as_str())
    .with("stage", "pre-placement verification")
}

/// `[package].name` of the staged manifest — the one name the evidence is matched against.
///
/// The one file read during verification, and the same bounded exception as the process
/// working directory: the path is `<staging>/Cargo.toml`, a file this generator rendered a
/// moment ago, parsed with `toml` and nothing more. Not a cache, not a record, not a stream.
fn own_package_name(staging: &Path) -> Result<String, CliError> {
    #[derive(serde::Deserialize)]
    struct Manifest {
        package: Package,
    }
    #[derive(serde::Deserialize)]
    struct Package {
        name: String,
    }
    let unreadable = |what: &str| {
        CliError::new(
            Code::ProjectVerificationFailed,
            format!(
                "the staged project's `Cargo.toml` {what}, so its checks' evidence cannot be \
                 matched to it; nothing was written to the destination"
            ),
        )
        .with("reason", evidence::REASON_CAPTURE_FAILED)
        .with("check", "Cargo.toml [package].name")
        .with("stage", "pre-placement verification")
    };
    let text = std::fs::read_to_string(staging.join("Cargo.toml"))
        .map_err(|_| unreadable("could not be read"))?;
    let manifest: Manifest =
        toml::from_str(&text).map_err(|_| unreadable("does not parse as a package manifest"))?;
    Ok(manifest.package.name)
}

#[cfg(test)]
mod tests {
    /// An indicator that renders nowhere.
    ///
    /// These tests run `cargo build` for real, so they are slow — but they are not the place to
    /// assert anything about a spinner, and a visible one would interleave with libtest's own
    /// output. `Progress` is deliberately constructible in this state, which is the same state
    /// every JSON and non-terminal run uses.
    fn silent() -> crate::output::progress::Progress {
        // JSON format, so `progress_visible()` is false whatever the terminal is.
        crate::output::progress::Progress::start(
            "verifying",
            &crate::output::Reporter::new(crate::output::Format::Json, true),
        )
    }

    use super::*;

    fn project(main: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            b"[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\n",
        )
        .expect("write");
        std::fs::write(dir.path().join("src/main.rs"), main).expect("write");
        dir
    }

    #[test]
    fn an_absolute_cargo_target_dir_is_honoured_and_a_relative_one_is_refused() {
        // Phase 011 (FR-007). Four starters compiled cold into four temporary directories cost the
        // gate a quarter of an hour; an operator (or a test) that sets `CARGO_TARGET_DIR` gets it
        // honoured — when it is absolute. A RELATIVE one would resolve against the staging
        // directory and put build output INSIDE the project, which is the one thing this module
        // exists to prevent, so it is refused by name rather than silently ignored.
        use std::ffi::OsStr;
        let absolute = if cfg!(windows) {
            r"C:\renvor-target"
        } else {
            "/tmp/renvor-target"
        };
        match target_directory(Some(OsStr::new(absolute))).expect("an absolute directory") {
            TargetDirectory::Configured(path) => assert_eq!(path, std::path::Path::new(absolute)),
            TargetDirectory::Temporary(_) => panic!("an absolute CARGO_TARGET_DIR was ignored"),
        }
        assert!(matches!(
            target_directory(None).expect("a temporary directory"),
            TargetDirectory::Temporary(_)
        ));
        let error = target_directory(Some(OsStr::new("target"))).expect_err("relative is refused");
        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "check" && v.contains("CARGO_TARGET_DIR")),
            "the refusal must name the CARGO_TARGET_DIR check in its details"
        );
        let error = target_directory(Some(OsStr::new(""))).expect_err("empty is refused");
        assert_eq!(error.code, Code::ProjectVerificationFailed);
    }

    #[test]
    fn a_project_that_builds_and_tests_passes() {
        // POSITIVE CONTROL. Without it, a verifier that rejected everything would satisfy every
        // failure test below and make `renvor new` impossible to use.
        let dir = project("fn main() {}\n");
        let verified =
            in_staging(dir.path(), &silent(), Smoke::Exits).expect("a correct project must verify");
        // Phase 012 (FR-012-7d (h), the trivial case (vii)): a private, empty target launches
        // every unit, so the observation is `launched`, the launched compiler and the clippy
        // driver were each asked who they were, and nothing was cached.
        assert_eq!(
            verified.observation,
            Observation::Launched,
            "a private empty target launches every unit"
        );
        assert!(
            verified.rustc.is_some(),
            "a launched build/test unit has a queried compiler identity"
        );
        assert!(
            verified.clippy.driver.is_some(),
            "a launched clippy unit has a queried driver identity"
        );
        assert!(
            verified.build.units_launched >= 1 && verified.test.units_launched >= 1,
            "the build and test checks each launched at least one own unit"
        );
        assert!(
            verified.clippy.units_launched >= 1,
            "the clippy check launched at least one own unit"
        );
        assert!(
            verified.cached_checks.is_empty(),
            "nothing was cached on a private empty target"
        );
        assert!(
            verified.fmt && verified.run,
            "the two outcome booleans say the checks passed"
        );
        assert!(
            !verified.wrapper_observed,
            "no wrapper is configured in this environment"
        );
    }

    #[test]
    fn a_starter_is_started_with_the_route_dump_request_and_must_answer_it() {
        // Phase 011 (FR-011). A starter is a server: `cargo run --quiet` alone would block until
        // the deadline, and it needs `RENVOR_DATABASE_URL` to serve — which generation must not
        // require and must never invent. So the smoke check sends a starter the same inspection
        // request `renvor routes` sends, which the starter answers from its registry before Boot
        // and without a database. POSITIVE CONTROL: a main that answers the request verifies.
        // NEGATIVE CONTROL: a main that refuses exactly that request is a generation failure that
        // names the check — proving the argument is actually passed rather than the bare run.
        // The probe answers by exit status alone: a shipped source file may not carry a
        // stream write, and `tests/presentation.rs` reads this test's text like any other.
        let answers = project(
            "fn main() {\n    if std::env::args().any(|a| a == \"--renvor-dump-routes\") {\n        \
             std::process::exit(0);\n    }\n    std::process::exit(1);\n}\n",
        );
        in_staging(answers.path(), &silent(), Smoke::AnswersDumpRequest)
            .expect("a starter that answers the dump request verifies");
        let refuses = project(
            "fn main() {\n    if std::env::args().any(|a| a == \"--renvor-dump-routes\") {\n        \
             std::process::exit(1);\n    }\n}\n",
        );
        let error = in_staging(refuses.path(), &silent(), Smoke::AnswersDumpRequest).unwrap_err();
        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "check" && v == "cargo run --quiet -- --renvor-dump-routes"),
            "the failing check must name the request it sent"
        );
        // And the skeleton's bare run does not send it: the same refusing main exits 0 without
        // the argument, so it verifies under `Smoke::Exits`.
        in_staging(refuses.path(), &silent(), Smoke::Exits)
            .expect("a skeleton is run bare and this one exits 0 when run bare");
    }

    #[test]
    fn verification_runs_in_a_sealed_environment() {
        // Phase 011. The staged project's `cargo test` inherited the operator's environment, and
        // the gate sets `RENVOR_TEST_REQUIRE_DATABASE=1` for the whole shell — so every generated
        // test that honours that convention failed INSIDE generation. Worse in principle: a
        // `RENVOR_DATABASE_URL` in the operator's shell would have let generation connect to,
        // and migrate, a real database. Verification therefore runs with a sealed environment:
        // what cargo and the toolchain need, and nothing the operator's shell happens to carry.
        // The starter matrix (`tests/starter_matrix.rs`) is the process-level control: its rows
        // generate under the gate's `RENVOR_TEST_REQUIRE_*` variables and their staged tests
        // must skip rather than fail.
        use std::ffi::OsString;
        let parent = [
            ("PATH", "/usr/bin"),
            ("HOME", "/home/operator"),
            ("CARGO_HOME", "/home/operator/.cargo"),
            ("RUSTUP_TOOLCHAIN", "1.94.0"),
            ("RENVOR_DATABASE_URL", "postgres://x:y@127.0.0.1/production"),
            ("RENVOR_TEST_REQUIRE_DATABASE", "1"),
            ("RENVOR_TEST_REQUIRE_CAPABILITIES", "1"),
            ("RENVOR_AUTH_CSRF_KEY", "00"),
            ("AWS_SECRET_ACCESS_KEY", "nope"),
            ("SOME_UNRELATED_VARIABLE", "value"),
        ]
        .map(|(name, value)| (OsString::from(name), OsString::from(value)));
        let sealed = seal(parent.into_iter()).variables;
        let names: Vec<&str> = sealed
            .iter()
            .map(|(name, _)| name.to_str().expect("utf-8"))
            .collect();
        assert_eq!(
            names,
            ["PATH", "HOME", "CARGO_HOME", "RUSTUP_TOOLCHAIN"],
            "only what the toolchain needs passes through, in the parent's order"
        );
    }

    /// Every rendering a diagnostic could leak a secret in.
    fn every_form_of(secret: &str) -> Vec<String> {
        let bytes = secret.as_bytes();
        vec![
            secret.to_owned(),
            format!("{secret:?}"),
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            bytes.iter().map(|b| format!("{b:02X}")).collect::<String>(),
            bytes
                .iter()
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ]
    }

    #[test]
    fn the_seal_keeps_the_search_path_under_the_name_the_platform_spells_it() {
        // FOUND BY THE WINDOWS PLATFORM LEGS (2026-09-08). Windows spells the search path `Path`
        // and matches variable names case-insensitively; a case-sensitive allow-list dropped it,
        // so the sealed environment had no path at all. The checks survived because
        // `Command::new("cargo")` resolves its program against the parent's path — but
        // `toolchain::locate` reads the sealed variables, found nothing, and every generation
        // refused with `tool_missing`. Nothing on Unix changes: there `Path` is a different
        // variable from `PATH` and is dropped, as it always was.
        use std::ffi::OsString;
        let parent = [("Path", "/usr/bin"), ("SYSTEMROOT", "C:\\Windows")]
            .map(|(name, value)| (OsString::from(name), OsString::from(value)));
        let sealed = seal(parent.into_iter());
        let kept: Vec<&str> = sealed
            .variables
            .iter()
            .map(|(name, _)| name.to_str().expect("utf-8"))
            .collect();
        if cfg!(windows) {
            assert_eq!(
                kept,
                ["Path", "SYSTEMROOT"],
                "the platform's own spelling of an allowed name must pass, unchanged"
            );
        } else {
            assert!(
                kept.is_empty(),
                "a case-insensitive match must not widen the allow-list off Windows"
            );
        }
    }

    #[test]
    fn a_proxy_credential_never_reaches_the_sealed_environment() {
        // STANDARDS AXIS (P1). The proxy variables pass through the seal so a fetch can route,
        // and a proxy URL may carry `user:password@`; that credential was handed to every build
        // script and dependency the staged project compiles. The host survives, the credential
        // does not: verification needs no registry update (FR-006), so an authenticated proxy is
        // not something it has to be able to use.
        use std::ffi::OsString;
        let parent = [
            ("PATH", "/usr/bin"),
            (
                "HTTPS_PROXY",
                "http://alice:s3cr3t-proxy-pass@proxy.example:3128",
            ),
            ("http_proxy", "http://bob:hunter2@10.0.0.1:8080/"),
            (
                "CARGO_HTTP_PROXY",
                "socks5://carol:pw-9f@proxy.example:1080",
            ),
            ("HTTP_PROXY", "http://proxy.example:3128"),
            ("NO_PROXY", "localhost,127.0.0.1"),
        ]
        .map(|(name, value)| (OsString::from(name), OsString::from(value)));
        let sealed = seal(parent.into_iter()).variables;
        // Fixed messages throughout, and indices where a case must be named: this file handles
        // credentials, and a failure here is the one run in which printing one matters most.
        let secrets = [
            "s3cr3t-proxy-pass",
            "hunter2",
            "pw-9f",
            "alice",
            "bob",
            "carol",
        ];
        for (variable, (_, value)) in sealed.iter().enumerate() {
            let value = value.to_string_lossy();
            let leaked: Vec<(usize, usize, usize)> = secrets
                .iter()
                .enumerate()
                .flat_map(|(secret, text)| {
                    every_form_of(text)
                        .into_iter()
                        .enumerate()
                        .filter(|(_, form)| value.contains(form))
                        .map(move |(rendering, _)| (variable, secret, rendering))
                        .collect::<Vec<_>>()
                })
                .collect();
            assert_eq!(
                leaked,
                Vec::<(usize, usize, usize)>::new(),
                "a sealed proxy value carries a credential: (variable, secret, rendering) indices"
            );
        }
        let value_of = |wanted: &str| {
            sealed
                .iter()
                .find(|(name, _)| name == wanted)
                .map(|(_, value)| value.to_string_lossy().into_owned())
        };
        assert_eq!(
            value_of("HTTPS_PROXY").as_deref(),
            Some("http://proxy.example:3128"),
            "the host, scheme, and port survive"
        );
        assert_eq!(
            value_of("http_proxy").as_deref(),
            Some("http://10.0.0.1:8080/")
        );
        assert_eq!(
            value_of("CARGO_HTTP_PROXY").as_deref(),
            Some("socks5://proxy.example:1080")
        );
        assert_eq!(
            value_of("HTTP_PROXY").as_deref(),
            Some("http://proxy.example:3128"),
            "a credential-free value passes unchanged"
        );
        assert_eq!(value_of("NO_PROXY").as_deref(), Some("localhost,127.0.0.1"));
    }

    #[test]
    fn a_childs_output_is_reported_without_url_credentials_or_control_characters() {
        // The redaction on its own: the end-to-end control above cannot tell it from the seal,
        // because once the seal has stripped the credential no child can print it. Here a child's
        // text carries one anyway — a URL credential of its own, and the credential the seal
        // removed — and both leave; the host stays; a control character is escaped.
        let removed = vec!["alice:s3cr3t-proxy-pass".to_owned()];
        let text = "warning: proxy http://alice:s3cr3t-proxy-pass@proxy.example:3128/x failed\n\
                    also socks5://bob:hunter2@10.0.0.1:1080 and plain http://host:80/ok\n\
                    and the password s3cr3t-proxy-pass on its own\u{1b}[31m";
        let reported = redacted_output(text, &removed);
        let secrets = ["s3cr3t-proxy-pass", "hunter2", "alice", "bob"];
        let leaked: Vec<(usize, usize)> = secrets
            .iter()
            .enumerate()
            .flat_map(|(secret, text)| {
                every_form_of(text)
                    .into_iter()
                    .enumerate()
                    .filter(|(_, form)| reported.contains(form))
                    .map(move |(rendering, _)| (secret, rendering))
                    .collect::<Vec<_>>()
            })
            .collect();
        assert_eq!(
            leaked,
            Vec::<(usize, usize)>::new(),
            "the reported output carries a credential: (secret, rendering) indices"
        );
        assert!(reported.contains("http://<credential removed>@proxy.example:3128/x"));
        assert!(reported.contains("socks5://<credential removed>@10.0.0.1:1080"));
        assert!(
            reported.contains("http://host:80/ok"),
            "a credential-free URL is untouched"
        );
        assert!(
            reported.contains("\\u{1b}[31m"),
            "the escape sequence must be escaped in the reported output"
        );
        assert!(!reported.contains('\u{1b}'));
    }

    #[test]
    fn the_seal_forces_rustup_auto_install_0_and_drops_the_install_server_variables() {
        // Phase 012 (FR-012-6, SR-012-1). Before this, `RUSTUP_DIST_SERVER` and
        // `RUSTUP_UPDATE_ROOT` passed through the seal — the two variables that tell rustup where
        // to download from — and nothing set `RUSTUP_AUTO_INSTALL`, so a rustup proxy run in a
        // staged project pinned to an absent channel installed that channel from inside
        // generation. Now every child the seal builds gets `RUSTUP_AUTO_INSTALL=0` whatever the
        // caller set, and neither install-server variable at all. `RUSTC_WORKSPACE_WRAPPER`
        // joins `RUSTC_WRAPPER` in the pass-through: the operator's trust, its presence
        // recorded (SR-012-3).
        use std::ffi::{OsStr, OsString};
        let parent = [
            ("PATH", "/usr/bin"),
            ("RUSTUP_AUTO_INSTALL", "1"),
            ("RUSTUP_DIST_SERVER", "https://mirror.example/rustup"),
            ("RUSTUP_UPDATE_ROOT", "https://mirror.example/rustup"),
            ("RUSTC_WORKSPACE_WRAPPER", "/opt/cache/wrapper"),
        ]
        .map(|(name, value)| (OsString::from(name), OsString::from(value)));
        let sealed = seal(parent.into_iter());
        let names: Vec<&str> = sealed
            .variables
            .iter()
            .map(|(name, _)| name.to_str().expect("utf-8"))
            .collect();
        assert!(
            !names.contains(&"RUSTUP_DIST_SERVER") && !names.contains(&"RUSTUP_UPDATE_ROOT"),
            "an install-server variable passed through the seal"
        );
        assert!(
            names.contains(&"RUSTC_WORKSPACE_WRAPPER"),
            "the workspace wrapper is the operator's trust and passes through"
        );
        // THE CHILD'S VIEW, which is the one that matters: the command every tool child is built
        // from carries the forced value — not the caller's `1` — and neither server variable.
        let command = sealed_command(OsStr::new("cargo"), &sealed, Path::new("."));
        let child: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
        assert!(
            child
                .iter()
                .any(|(name, value)| *name == "RUSTUP_AUTO_INSTALL"
                    && *value == Some(OsStr::new("0"))),
            "the child does not receive RUSTUP_AUTO_INSTALL=0"
        );
        assert!(
            child
                .iter()
                .all(|(name, _)| *name != "RUSTUP_DIST_SERVER" && *name != "RUSTUP_UPDATE_ROOT"),
            "the child receives an install-server variable"
        );
        assert_eq!(command.get_current_dir(), Some(Path::new(".")));
        // And end to end, on the platforms with an `env` to ask: the child's whole environment
        // is the sealed variables plus the forced one — `env_clear` is real, not implied.
        #[cfg(unix)]
        {
            let output = sealed_command(OsStr::new("/usr/bin/env"), &sealed, Path::new("/"))
                .output()
                .expect("env runs");
            let text = String::from_utf8_lossy(&output.stdout);
            let seen: Vec<&str> = text
                .lines()
                .filter_map(|line| line.split_once('=').map(|(name, _)| name))
                .collect();
            assert!(
                seen.contains(&"RUSTUP_AUTO_INSTALL"),
                "the forced variable did not reach the child"
            );
            assert!(
                text.lines().any(|line| line == "RUSTUP_AUTO_INSTALL=0"),
                "the forced value is not 0"
            );
            let permitted = ["PATH", "RUSTC_WORKSPACE_WRAPPER", "RUSTUP_AUTO_INSTALL"];
            assert!(
                seen.iter().all(|name| permitted.contains(name)),
                "the child saw a variable the seal did not pass"
            );
        }
    }

    #[test]
    fn a_build_script_cannot_observe_or_print_a_proxy_credential() {
        // STANDARDS AXIS (P1), the end-to-end control: a staged project's build script reads
        // every proxy variable it can see, prints them, and fails — so its output lands in the
        // verification error. Neither the environment it saw nor the error the operator reads
        // may carry the credential in any form; the host survives in both.
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n",
        )
        .expect("write");
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").expect("write");
        // The script is a fixture file beside the test harness: the presentation scan forbids a
        // print macro anywhere under `src/`, fixtures included, and rightly so.
        std::fs::write(
            dir.path().join("build.rs"),
            include_str!("../../tests/harness/proxy_probe_build.rs"),
        )
        .expect("write");
        let secret = "s3cr3t-proxy-pass";
        let mut parent: Vec<(std::ffi::OsString, std::ffi::OsString)> = std::env::vars_os()
            .filter(|(name, _)| {
                !name
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .contains("proxy")
            })
            .collect();
        parent.push((
            "HTTPS_PROXY".into(),
            format!("http://alice:{secret}@127.0.0.1:1").into(),
        ));
        parent.push((
            "http_proxy".into(),
            format!("http://alice:{secret}@127.0.0.1:1/").into(),
        ));
        let error = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect_err("the build script fails on purpose");
        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert!(
            error
                .message
                .contains("proxy variables seen by the build script:"),
            "the build script's output must be embedded, or this control proves nothing"
        );
        assert!(
            error.message.contains("127.0.0.1:1"),
            "the credential-free host must survive in the reported output"
        );
        let leaked: Vec<usize> = every_form_of(secret)
            .into_iter()
            .enumerate()
            .filter(|(_, form)| error.message.contains(form))
            .map(|(rendering, _)| rendering)
            .collect();
        assert_eq!(
            leaked,
            Vec::<usize>::new(),
            "the verification error carries the proxy credential: rendering indices"
        );
        assert!(
            !error.message.contains("alice"),
            "the user name is part of the credential and must not be reported"
        );
    }

    #[test]
    fn a_project_that_does_not_compile_is_a_generation_failure() {
        let dir = project("fn main() { this is not rust }\n");
        let error = in_staging(dir.path(), &silent(), Smoke::Exits).unwrap_err();
        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "stage" && v == "pre-placement verification"),
            "the failure must say it happened before placement"
        );
    }

    #[test]
    fn a_project_that_is_not_formatted_is_a_generation_failure() {
        // The defect that motivated this module: MiniJinja stripping a trailing newline produced a
        // project that compiled and failed `cargo fmt --check`. Compilation alone would have
        // shipped it.
        let dir = project("fn main() {   }");
        let error = in_staging(dir.path(), &silent(), Smoke::Exits).unwrap_err();
        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "check" && v.contains("fmt")),
            "the failing check must be named"
        );
        // AND THE DIFF. `rustfmt --check` writes its diff to STDOUT; a message built from stderr
        // alone said "not correctly formatted" and nothing else, which sends the reader back to
        // regenerate blind. Found by Phase 011's first starter render.
        // The diff's own header, rather than the formatted line: the kernel's diagnostics gate
        // reads a `{}` inside a literal as an interpolation, and a credential-handling file may
        // interpolate nothing into a failure.
        assert!(
            error.message.contains("Diff in"),
            "the formatter's own diff must reach the message"
        );
    }

    #[test]
    fn verification_leaves_no_build_output_in_the_project() {
        // The rename moves whatever is in staging. A `target/` left here would be renamed into the
        // destination and would appear in the manifest as generated source.
        let dir = project("fn main() {}\n");
        in_staging(dir.path(), &silent(), Smoke::Exits).expect("verifies");
        assert!(
            !dir.path().join("target").exists(),
            "verification left build output that placement would have moved into the project"
        );
        let entries: Vec<String> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let mut sorted = entries.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            vec![
                "Cargo.lock".to_owned(),
                "Cargo.toml".to_owned(),
                "src".to_owned()
            ],
            "verification added or removed a file"
        );
    }

    // ── Phase 012, FR-012-7d: launch observation plus queried identity ──────────────

    use std::ffi::OsString;

    use crate::toolchain::{Identity, Observation};

    const EVIDENCE_SOURCE: &str = include_str!("../toolchain/evidence.rs");
    const VERIFY_SOURCE: &str = include_str!("verify.rs");

    /// The test process's environment as `in_staging` would read it, minus any
    /// `CARGO_TARGET_DIR`: every test below decides its own build directory, so an operator's
    /// shared one cannot turn a launch into a `Fresh` report.
    fn environment() -> Vec<(OsString, OsString)> {
        std::env::vars_os()
            .filter(|(name, _)| name != "CARGO_TARGET_DIR")
            .collect()
    }

    /// `parent` with `name` set to `value`, replacing any earlier value.
    fn with(
        mut parent: Vec<(OsString, OsString)>,
        name: &str,
        value: &OsStr,
    ) -> Vec<(OsString, OsString)> {
        parent.retain(|(existing, _)| existing != name);
        parent.push((OsString::from(name), value.to_os_string()));
        parent
    }

    /// The compiler the test's own `PATH` resolves, asked itself — the same resolution the
    /// staged checks get, since the seal passes `PATH` and `RUSTUP_TOOLCHAIN` through.
    fn real_rustc() -> Identity {
        let output = Command::new("rustc")
            .arg("-vV")
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()
            .expect("rustc runs");
        crate::toolchain::grammar::parse_rustc_vv(&String::from_utf8_lossy(&output.stdout))
            .expect("the real compiler answers under the grammar")
    }

    /// `<sysroot>/bin/<name>` of the compiler the test's `PATH` resolves.
    fn toolchain_binary(name: &str) -> PathBuf {
        let output = Command::new("rustc")
            .args(["--print", "sysroot"])
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()
            .expect("rustc runs");
        let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Path::new(&sysroot).join("bin").join(name)
    }

    /// Writes `script` at `path` and marks it executable.
    #[cfg(unix)]
    fn executable(path: &Path, script: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, script).expect("write");
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    /// The production half of a source file: everything before its test module, comment lines
    /// dropped.
    fn production(source: &str) -> String {
        // SPLIT ON THE TEST MODULE, not on the first `#[cfg(test)]` in the file. A single
        // test-only item above the module — `in_staging` became one when the two commands moved
        // to `in_staging_with` — used to truncate the scan there, and the assertions below then
        // passed over an empty string while still counting as green.
        source
            .split("#[cfg(test)]\nmod tests")
            .next()
            .unwrap_or("")
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// An error's message and details as one text, for leak assertions.
    fn rendered(error: &CliError) -> String {
        let details: Vec<String> = error
            .details
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        format!("{}\n{}\n{:?}", error.message, details.join("\n"), error)
    }

    #[test]
    fn a_positively_fresh_run_passes_as_cached_with_no_observed_identity() {
        // FR-012-7d (d), control (ix). A second verification against the SAME absolute target
        // finds every own unit `Fresh`: the checks pass without a compiler launch, the record
        // says `cached`, and the observed identities are ABSENT — never filled from the pin,
        // `PATH`, the earlier run, or a cache file.
        let dir = project("fn main() {}\n");
        let target = tempfile::tempdir().expect("tempdir");
        let parent = with(environment(), "CARGO_TARGET_DIR", target.path().as_os_str());
        let first = in_staging_with(
            dir.path(),
            &silent(),
            Smoke::Exits,
            parent.clone().into_iter(),
        )
        .expect("the first run verifies");
        assert_eq!(
            first.observation,
            Observation::Launched,
            "the first run against an empty target launches"
        );
        let second = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect("the second run verifies");
        assert_eq!(
            second.observation,
            Observation::Cached,
            "every own unit is Fresh the second time"
        );
        assert_eq!(
            second.rustc, None,
            "no launch, no observed compiler identity"
        );
        assert_eq!(
            second.clippy.driver, None,
            "a cached clippy check records no driver identity"
        );
        assert_eq!(
            second.cached_checks,
            ["clippy", "build", "test"],
            "every evidence-bearing check was cached"
        );
        assert!(
            second.build.units_fresh >= 1
                && second.test.units_fresh >= 1
                && second.clippy.units_fresh >= 1,
            "the Fresh reports are counted"
        );
        assert!(
            second.build.units_launched == 0 && second.test.units_launched == 0,
            "nothing was launched"
        );
    }

    #[test]
    fn a_mixed_run_records_both_counts_and_the_identity_for_the_launched_unit_only() {
        // FR-012-7d (f), control (x). A warm target, then a new integration test file: `cargo
        // build` reuses the bin (`Fresh`), `cargo test` launches the new unit. The observation
        // is `mixed`, both counts are kept, and the queried identity belongs to the launched
        // unit — the record never says one observed unit proves every unit used that compiler.
        let dir = project("fn main() {}\n");
        let target = tempfile::tempdir().expect("tempdir");
        let parent = with(environment(), "CARGO_TARGET_DIR", target.path().as_os_str());
        in_staging_with(
            dir.path(),
            &silent(),
            Smoke::Exits,
            parent.clone().into_iter(),
        )
        .expect("the warming run verifies");
        std::fs::create_dir_all(dir.path().join("tests")).expect("mkdir");
        std::fs::write(
            dir.path().join("tests/extra.rs"),
            "#[test]\nfn extra() {}\n",
        )
        .expect("write");
        let mixed = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect("the run with the new test verifies");
        assert_eq!(
            mixed.observation,
            Observation::Mixed,
            "the build is Fresh and the test launches the new unit"
        );
        assert!(
            mixed.build.units_fresh >= 1 && mixed.build.units_launched == 0,
            "the bin was reused by the build check"
        );
        assert!(
            mixed.test.units_launched >= 1,
            "the new integration test was launched by the test check"
        );
        assert!(
            mixed.rustc.is_some(),
            "the launched unit's compiler identity is queried"
        );
        assert_eq!(
            mixed.cached_checks,
            ["build"],
            "only the build check was wholly cached"
        );
    }

    #[test]
    fn a_truncated_or_unaccounted_evidence_stream_is_evidence_capture_failed_never_cached() {
        // FR-012-7d (e), control (xii). At the parser: a stream cut before `Finished`, an
        // announced own package with no launch, an own package that appears nowhere — each an
        // error, none a `Cached`. End to end (Unix): a `cargo` on `PATH` that runs the real one
        // and drops its `Finished` line is `evidence_capture_failed`, naming the check.
        let staging = tempfile::tempdir().expect("tempdir");
        let cut = "   Compiling probe v0.1.0 (/x)\n     Running `CARGO_PKG_NAME=probe \
                   /t/bin/rustc --crate-name probe src/main.rs`\n";
        assert_eq!(
            evidence::parse_check(cut, "probe", staging.path()),
            Err(EvidenceError::Truncated),
            "a stream cut before Finished is truncated"
        );
        let no_running = "   Compiling probe v0.1.0 (/x)\n    Finished `dev` profile in 0.1s\n";
        assert_eq!(
            evidence::parse_check(no_running, "probe", staging.path()),
            Err(EvidenceError::Unaccounted),
            "Compiling with no Running is unaccounted"
        );
        let absent = "       Fresh serde v1.0.0\n    Finished `dev` profile in 0.1s\n";
        assert_eq!(
            evidence::parse_check(absent, "probe", staging.path()),
            Err(EvidenceError::Unaccounted),
            "an own package that appears nowhere is unaccounted, not cached"
        );

        #[cfg(unix)]
        {
            let real_cargo = std::env::var_os("CARGO").expect("cargo sets CARGO for a test binary");
            let stub = tempfile::tempdir().expect("tempdir");
            executable(
                &stub.path().join("cargo"),
                &format!(
                    "#!/bin/sh\ntmp=\"$(mktemp)\"\n\"{}\" \"$@\" 2>\"$tmp\"\nstatus=$?\ngrep -v \
                     'Finished' \"$tmp\" >&2\nrm -f \"$tmp\"\nexit $status\n",
                    real_cargo.to_string_lossy()
                ),
            );
            let mut path = OsString::from(stub.path());
            path.push(":");
            path.push(std::env::var_os("PATH").unwrap_or_default());
            let parent = with(environment(), "PATH", &path);
            let dir = project("fn main() {}\n");
            let error = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
                .expect_err("a truncated stream fails verification");
            assert_eq!(error.code, Code::ProjectVerificationFailed);
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "reason" && v == "evidence_capture_failed"),
                "the reason is evidence_capture_failed"
            );
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "check" && v.contains("clippy")),
                "the first evidence-bearing check is named"
            );
            // The message may SAY "never a cached run"; no detail may REPORT one.
            assert!(
                error
                    .details
                    .iter()
                    .all(|(_, value)| value != "cached" && value != "mixed"),
                "a capture failure must never carry a cached or mixed observation"
            );
        }
    }

    #[test]
    fn stale_evidence_never_fills_a_field() {
        // FR-012-7d (d), (e), control (xi). Two halves. AT THE SOURCE: the evidence path reads
        // no cache file and no earlier stream — evidence.rs performs no file read at all, and
        // the one read in this file is the staged manifest. END TO END: a planted rustc info
        // cache naming another compiler in the build directory changes nothing; the observed
        // identity is what the launched compiler itself answered.
        let cache_file = format!("{}{}", ".rustc_info", ".json");
        let evidence_code = production(EVIDENCE_SOURCE);
        let verify_code = production(VERIFY_SOURCE);
        assert!(
            !evidence_code.is_empty() && !verify_code.is_empty(),
            "the source scan found nothing to scan"
        );
        assert!(
            !evidence_code.contains(&cache_file) && !verify_code.contains(&cache_file),
            "the evidence path names the rustc info cache file"
        );
        assert!(
            !evidence_code.contains("std::fs")
                && !evidence_code.contains("fs::read")
                && !evidence_code.contains("File::open"),
            "evidence.rs reads a file; its only input is the check's own stderr"
        );
        let reads: Vec<&str> = verify_code
            .lines()
            .filter(|line| {
                line.contains("read_to_string(")
                    || line.contains("fs::read(")
                    || line.contains("File::open(")
            })
            .collect();
        assert_eq!(reads.len(), 1, "verify.rs performs exactly one file read");
        assert!(
            reads.iter().all(|line| line.contains("Cargo.toml")),
            "the one read in verify.rs is not the staged manifest"
        );

        let dir = project("fn main() {}\n");
        let target = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            target.path().join(&cache_file),
            "{\"rustc_fingerprint\":1,\"outputs\":{},\"successes\":{},\"release\":\"1.0.0\",\
             \"commit-hash\":\"deadbeefdeadbeef\"}",
        )
        .expect("plant");
        let parent = with(environment(), "CARGO_TARGET_DIR", target.path().as_os_str());
        let verified = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect("verifies with a planted cache file");
        let real = real_rustc();
        let observed = verified
            .rustc
            .expect("a launched unit has an observed identity");
        assert_eq!(
            observed.release, real.release,
            "the observed release is the launched compiler's own answer"
        );
        assert_eq!(
            observed.commit, real.commit,
            "the observed commit is the launched compiler's own answer"
        );
    }

    #[test]
    fn incremental_off_binary_only_clippy_passes_with_the_driver_identity_and_no_artifact_requirement()
     {
        // FR-012-7d (c), control (viii). `CARGO_INCREMENTAL=0` on a bin-only package — the
        // starter's shape under the repository's own harness — writes no artifact marker for
        // clippy at all (T-012-08m extension). The mechanism needs none: the driver's identity is
        // its own answer, and the launch is Cargo's own line.
        let dir = project("fn main() {}\n");
        let parent = with(environment(), "CARGO_INCREMENTAL", OsStr::new("0"));
        let verified = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect("a bin-only project verifies with incremental compilation off");
        assert!(
            verified.clippy.driver.is_some(),
            "the driver identity is recorded without an artifact"
        );
        assert!(
            verified.clippy.units_launched >= 1,
            "the clippy check launched at least one own unit"
        );
        assert_eq!(
            verified.observation,
            Observation::Launched,
            "a private empty target launches every unit"
        );
    }

    #[test]
    fn the_clippy_driver_is_identified_by_its_own_version_query_not_the_trailing_rustc() {
        // FR-012-7d (b), §4.3.2. The recorded driver identity is what the toolchain's own
        // `clippy-driver` answers to `--version` — a `0.1.x` clippy release, not the release of
        // the trailing `rustc` argument, and not the answer of `cargo clippy --version`, which
        // nothing in the evidence path spawns.
        let dir = project("fn main() {}\n");
        let verified = in_staging_with(
            dir.path(),
            &silent(),
            Smoke::Exits,
            environment().into_iter(),
        )
        .expect("verifies");
        let driver = verified
            .clippy
            .driver
            .expect("a launched clippy unit has a driver identity");
        assert!(
            driver.release.starts_with("0.1."),
            "a clippy release, not a compiler release"
        );
        let rustc = verified
            .rustc
            .expect("a launched unit has a compiler identity");
        assert_ne!(
            driver.release, rustc.release,
            "the driver identity was taken from the trailing compiler"
        );
        let binary = toolchain_binary(if cfg!(windows) {
            "clippy-driver.exe"
        } else {
            "clippy-driver"
        });
        let answer = Command::new(&binary)
            .arg("--version")
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()
            .expect("the toolchain's clippy-driver runs");
        let expected = crate::toolchain::grammar::parse_clippy_version(&String::from_utf8_lossy(
            &answer.stdout,
        ))
        .expect("the toolchain's clippy-driver answers under the grammar");
        assert_eq!(
            driver, expected,
            "the recorded driver identity is the executable's own answer"
        );
        // AT THE SOURCE: the one `--version` query in the evidence path is the driver's, and no
        // line spawns `cargo clippy --version`.
        let evidence_code = production(EVIDENCE_SOURCE);
        let verify_code = production(VERIFY_SOURCE);
        let version_queries = evidence_code
            .lines()
            .filter(|line| line.contains("\"--version\""))
            .count();
        assert_eq!(
            version_queries, 1,
            "exactly one --version query in evidence.rs"
        );
        assert!(
            !verify_code.contains("\"--version\""),
            "verify.rs spawns its own --version query"
        );
        for code in [&evidence_code, &verify_code] {
            assert!(
                !code.contains("clippy --version")
                    && !code
                        .lines()
                        .any(|line| line.contains("\"clippy\"") && line.contains("\"--version\"")),
                "the evidence path spawns cargo clippy --version"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_observed_clippy_driver_is_queried_itself_never_through_the_launcher() {
        // FR-012-7d (h), control (xiv), the A-8 round. A stub toolchain whose `cargo clippy
        // --version` answers identity A while the `clippy-driver` executable its `Running` line
        // names answers identity B: the evidence carries B. The launcher's answer is never a
        // substitute for the executable's own.
        let stub = tempfile::tempdir().expect("tempdir");
        let stub_dir = stub.path().display().to_string();
        executable(
            &stub.path().join("cargo"),
            &format!(
                r#"#!/bin/sh
case "$1" in
  clippy)
    if [ "$2" = "--version" ]; then echo "clippy 0.1.90 (aaaaaaaaaa 2025-01-01)"; exit 0; fi
    echo "    Checking probe v0.1.0 ($PWD)" >&2
    echo "     Running \`CARGO={stub}/cargo CARGO_MANIFEST_DIR=$PWD CARGO_PKG_NAME=probe {stub}/clippy-driver {stub}/rustc --crate-name probe --edition=2024 src/main.rs\`" >&2
    echo "    Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.10s" >&2
    exit 0 ;;
  build|test)
    echo "   Compiling probe v0.1.0 ($PWD)" >&2
    echo "     Running \`CARGO={stub}/cargo CARGO_MANIFEST_DIR=$PWD CARGO_PKG_NAME=probe {stub}/rustc --crate-name probe --edition=2024 src/main.rs\`" >&2
    echo "    Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.10s" >&2
    exit 0 ;;
  *) exit 0 ;;
esac
"#,
                stub = stub_dir
            ),
        );
        executable(
            &stub.path().join("clippy-driver"),
            "#!/bin/sh\necho \"clippy 0.1.77 (bbbbbbbbbb 2024-01-01)\"\n",
        );
        executable(
            &stub.path().join("rustc"),
            "#!/bin/sh\nprintf 'rustc 1.94.0 (0123456789 2026-01-01)\\nbinary: rustc\\ncommit-hash: \
             0123456789abcdef0123456789abcdef01234567\\ncommit-date: 2026-01-01\\nhost: \
             x86_64-unknown-linux-gnu\\nrelease: 1.94.0\\n'\n",
        );
        let path = format!("{stub_dir}:/usr/bin:/bin");
        let parent = vec![
            (OsString::from("PATH"), OsString::from(path)),
            (
                OsString::from("HOME"),
                std::env::var_os("HOME").unwrap_or_default(),
            ),
        ];
        let dir = project("fn main() {}\n");
        let verified = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect("the stub toolchain verifies");
        let driver = verified
            .clippy
            .driver
            .expect("a launched clippy unit has a driver identity");
        assert_eq!(
            driver.release, "0.1.77",
            "the driver's own release, not the launcher's 0.1.90"
        );
        assert_eq!(driver.commit, "bbbbbbbbbb", "the driver's own commit");
        let rustc = verified.rustc.expect("the stub compiler was queried");
        assert_eq!(rustc.release, "1.94.0", "the trailing compiler's own -vV");
        assert_eq!(
            verified.observation,
            Observation::Launched,
            "the stub streams launched every unit"
        );
    }

    /// A failing check whose stream carries a launch command split across physical lines: neither
    /// rendering of the refusal — the human message or the JSON envelope, which is the whole of
    /// what leaves this program — carries any part of it.
    ///
    /// The canary sits in the **continuation**, which is where a filter that drops only the first
    /// physical line leaves it, and it is shaped like the thing that would actually be there: a
    /// long `CARGO_PKG_*` value whose newline came from the manifest, and a `RUSTFLAGS`-derived
    /// argument after it. The diagnostic is asserted present in the same breath, so a filter that
    /// passed by deleting everything would fail here.
    #[cfg(unix)]
    #[test]
    fn a_failing_checks_message_and_json_carry_no_part_of_a_multiline_launch_command() {
        let stub = tempfile::tempdir().expect("tempdir");
        let stub_dir = stub.path().display().to_string();
        // `clippy` answers with a well-formed stream so the run reaches `build`, which is the
        // check under test; `build` then fails with the split launch command.
        executable(
            &stub.path().join("cargo"),
            &format!(
                r#"#!/bin/sh
case "$1" in
  clippy)
    echo "    Checking probe v0.1.0 ($PWD)" >&2
    echo "     Running \`CARGO_MANIFEST_DIR=$PWD CARGO_PKG_NAME=probe {stub}/clippy-driver {stub}/rustc --crate-name probe src/main.rs\`" >&2
    echo "    Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 0.10s" >&2
    exit 0 ;;
  build)
    echo "   Compiling probe v0.1.0 ($PWD)" >&2
    printf '     Running `CARGO_PKG_DESCRIPTION='"'"'a description whose\n' >&2
    printf 'renvor_canary_continuation_7c1d second line'"'"' {stub}/rustc --cfg renvor_canary_flag_9e2f --crate-name probe`\n' >&2
    echo "error[E0425]: cannot find value \`renvor_diagnostic_marker\` in this scope" >&2
    exit 101 ;;
  *) exit 0 ;;
esac
"#,
                stub = stub_dir
            ),
        );
        executable(
            &stub.path().join("clippy-driver"),
            "#!/bin/sh\necho \"clippy 0.1.94 (4a4ef493e3 2026-03-02)\"\n",
        );
        executable(
            &stub.path().join("rustc"),
            "#!/bin/sh\nprintf 'rustc 1.94.0 (0123456789 2026-01-01)\\nbinary: rustc\\ncommit-hash: \
             0123456789abcdef0123456789abcdef01234567\\ncommit-date: 2026-01-01\\nhost: \
             x86_64-unknown-linux-gnu\\nrelease: 1.94.0\\n'\n",
        );
        let parent = vec![
            (
                OsString::from("PATH"),
                OsString::from(format!("{}:/usr/bin:/bin", stub.path().display())),
            ),
            (
                OsString::from("HOME"),
                std::env::var_os("HOME").unwrap_or_default(),
            ),
        ];
        let dir = project("fn main() {}\n");
        let error = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect_err("the stub build fails");

        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "check" && value.contains("build")),
            "the refusal under test is the failing build, not an earlier check"
        );
        assert!(
            error.message.contains("renvor_diagnostic_marker"),
            "the diagnostic the operator needs was dropped with the launch command"
        );
        let json = serde_json::to_string(&crate::output::json::Envelope::failure(
            "generate", &error,
        ))
        .expect("the failure envelope serialises");
        for rendering in [error.message.as_str(), json.as_str()] {
            assert!(
                !rendering.contains("renvor_canary_continuation"),
                "a launch command's continuation reached a rendering of the refusal"
            );
            assert!(
                !rendering.contains("renvor_canary_flag"),
                "an argument of a launch command reached a rendering of the refusal"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_unreadable_compiler_identity_is_project_verification_failed_and_redacted() {
        // FR-012-7e. `RUSTC` names a shim that answers `-vV` with a commit hash carrying terminal
        // control bytes and a credential — a shape Cargo itself accepts — and otherwise runs the
        // real compiler, so every check passes. The identity query then fails the grammar:
        // `compiler_identity_unreadable`, and neither the escape byte nor the credential, in any
        // rendering, is anywhere in the error. The shim is named `rustc` because clippy-driver
        // recognises its compiler argument by file name.
        let real = real_rustc();
        let real_binary = toolchain_binary("rustc");
        let shim = tempfile::tempdir().expect("tempdir");
        let shim_path = shim.path().join("rustc");
        executable(
            &shim_path,
            &format!(
                "#!/bin/sh\nif [ \"$#\" -eq 1 ] && [ \"$1\" = \"-vV\" ]; then\n  printf 'rustc \
                 {release} (0000000 2026-01-01)\\nbinary: rustc\\ncommit-hash: \
                 s3cr3t-proxy-pass\\033[31m\\ncommit-date: 2026-01-01\\nhost: {host}\\nrelease: \
                 {release}\\n'\n  exit 0\nfi\nexec \"{real}\" \"$@\"\n",
                release = real.release,
                host = real.host,
                real = real_binary.display()
            ),
        );
        let parent = with(environment(), "RUSTC", shim_path.as_os_str());
        let dir = project("fn main() {}\n");
        let error = in_staging_with(dir.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect_err("an unreadable identity fails verification");
        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "compiler_identity_unreadable"),
            "the reason is compiler_identity_unreadable"
        );
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "stage" && v == "pre-placement verification"),
            "the failure says it happened before placement"
        );
        let text = rendered(&error);
        let leaked: Vec<usize> = every_form_of("s3cr3t-proxy-pass")
            .into_iter()
            .enumerate()
            .filter(|(_, form)| text.contains(form))
            .map(|(rendering, _)| rendering)
            .collect();
        assert_eq!(
            leaked,
            Vec::<usize>::new(),
            "the error carries the shim's credential: rendering indices"
        );
        assert!(
            !text.contains('\u{1b}') && !text.contains("\\u{1b}") && !text.contains("[31m"),
            "the error carries the escape byte in some rendering"
        );
    }

    #[test]
    fn no_flag_value_enters_the_record() {
        // FR-012-7e, control: `RUSTFLAGS` carrying a marker and a manifest `repository` carrying
        // another. Both reach the `-vv` stream — as a `--cfg` argument and as a `CARGO_PKG_*`
        // value — and neither reaches the evidence, its Debug rendering, or a failure message.
        let manifest = "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
                        publish = false\nrepository = \"https://marker-7a6b5c.example/\"\n\n\
                        [dependencies]\n";
        let dir = project("fn main() {}\n");
        std::fs::write(dir.path().join("Cargo.toml"), manifest).expect("write");
        let parent = with(
            environment(),
            "RUSTFLAGS",
            OsStr::new("--cfg renvor_marker_9f8e7d"),
        );
        let verified = in_staging_with(
            dir.path(),
            &silent(),
            Smoke::Exits,
            parent.clone().into_iter(),
        )
        .expect("verifies with RUSTFLAGS set");
        let text = format!("{verified:?}").to_ascii_lowercase();
        assert!(
            !text.contains("renvor_marker_9f8e7d") && !text.contains("marker-7a6b5c"),
            "a flag value or a manifest value reached the evidence"
        );
        let broken = project("fn main() { this is not rust }\n");
        std::fs::write(broken.path().join("Cargo.toml"), manifest).expect("write");
        let error = in_staging_with(broken.path(), &silent(), Smoke::Exits, parent.into_iter())
            .expect_err("a broken project fails");
        let text = rendered(&error).to_ascii_lowercase();
        assert!(
            !text.contains("renvor_marker_9f8e7d") && !text.contains("marker-7a6b5c"),
            "a flag value or a manifest value reached the failure message"
        );
        assert!(
            error.message.contains("error"),
            "the compiler's own diagnostic still reaches the failure message"
        );
    }

    #[test]
    fn a_pre_existing_probe_keep_survives_byte_identical() {
        // FR-012-7d (h), control (xiii). This mechanism uses no temporary storage in the
        // project: a file that was there before verification is there after, unchanged, alone.
        let dir = project("fn main() {}\n");
        let probe = dir.path().join(".renvor").join("probe");
        std::fs::create_dir_all(&probe).expect("mkdir");
        let bytes: Vec<u8> = vec![0x6b, 0x65, 0x65, 0x70, 0x0a, 0x00, 0xff, 0x1b];
        std::fs::write(probe.join("keep"), &bytes).expect("write");
        in_staging_with(
            dir.path(),
            &silent(),
            Smoke::Exits,
            environment().into_iter(),
        )
        .expect("verifies");
        assert_eq!(
            std::fs::read(probe.join("keep")).expect("read"),
            bytes,
            "the keep file changed"
        );
        let entries = std::fs::read_dir(&probe).expect("read_dir").count();
        assert_eq!(entries, 1, "verification left a file beside the keep");
    }
}
