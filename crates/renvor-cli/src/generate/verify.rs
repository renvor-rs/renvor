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

/// The checks run before the smoke run, in order, each with the failure it reports.
///
/// Ordered cheapest-first so the common failure is reported in a second rather than in a minute.
/// The fifth check — that the project **starts** — depends on the project's shape and is chosen
/// by [`Smoke`].
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
        &["clippy", "--all-targets", "--", "-D", "warnings"],
        "does not pass its own lints",
    ),
    ("cargo", &["build"], "does not compile"),
    ("cargo", &["test"], "does not pass its own tests"),
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
    "RUSTUP_DIST_SERVER",
    "RUSTUP_UPDATE_ROOT",
    "RUSTC",
    "RUSTC_WRAPPER",
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

/// The sealed environment, and the credentials the seal removed from it — so the output of a
/// child that somehow learned one can be redacted before it is reported.
pub struct Sealed {
    /// The variables the checks run with.
    pub variables: Vec<(std::ffi::OsString, std::ffi::OsString)>,
    /// Every `user:password` removed from a proxy value.
    pub credentials: Vec<String>,
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
        if !PASSED_THROUGH.contains(&text_name) {
            continue;
        }
        if PROXY_VARIABLES.contains(&text_name) {
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

/// Runs the generated project's own checks while it is still in staging.
///
/// # Errors
///
/// [`Code::ProjectVerificationFailed`] if any check fails — the project was generated and is
/// **wrong**, which is a generation failure — or [`Code::ToolMissing`] if `cargo` cannot be run at
/// all.
///
/// # Not `render_failed`, which it was until 2026-08-18
///
/// That code is published as *"template rendering failed"*, and rendering is exactly what did
/// **not** fail here: the templates produced a tree, and the tree then failed its own build, lint,
/// format, test, or start check. A consumer matching the registry would have looked for a template
/// defect over a compile error. This is the same class of misreporting as A-R6's three sites, found
/// in the same sweep and corrected with them.
pub fn in_staging(staging: &Path, progress: &Progress, smoke: Smoke) -> Result<(), CliError> {
    in_staging_with(staging, progress, smoke, std::env::vars_os())
}

/// [`in_staging`] with the parent environment given rather than read — the seam the
/// sealed-environment controls use to hand the checks a shell they can shape.
///
/// # Errors
///
/// As [`in_staging`].
pub fn in_staging_with(
    staging: &Path,
    progress: &Progress,
    smoke: Smoke,
    parent: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Result<(), CliError> {
    let parent: Vec<(std::ffi::OsString, std::ffi::OsString)> = parent.collect();
    let configured_target = parent
        .iter()
        .find(|(name, _)| name == "CARGO_TARGET_DIR")
        .map(|(_, value)| value.clone());
    let target = target_directory(configured_target.as_deref())?;
    let sealed = seal(parent.into_iter());

    for (program, arguments, complaint) in CHECKS.into_iter().chain(std::iter::once(smoke.check()))
    {
        // WHICH check is running, not merely THAT something is. `.output()` captures everything
        // cargo says, so without this the operator watches a spinner for a cold `cargo build` with
        // no way to tell a slow compile from a hung one — and no way to know, when it does hang,
        // which of the five to reproduce by hand.
        progress.step(&format!("{program} {}", arguments.join(" ")));
        let output = Command::new(program)
            .args(arguments)
            .current_dir(staging)
            // SEALED: see `PASSED_THROUGH`. The build directory is set explicitly, after the
            // seal, so the operator's `CARGO_TARGET_DIR` reaches cargo through `target` alone.
            .env_clear()
            .envs(sealed.variables.iter().cloned())
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
            let stdout = redacted_output(
                &String::from_utf8_lossy(&output.stdout),
                &sealed.credentials,
            );
            let stderr = redacted_output(
                &String::from_utf8_lossy(&output.stderr),
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
            .with("check", format!("{program} {}", arguments.join(" ")))
            .with("stage", "pre-placement verification"));
        }
    }

    Ok(())
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
            "{:?}",
            error.details
        );
        let error = target_directory(Some(OsStr::new(""))).expect_err("empty is refused");
        assert_eq!(error.code, Code::ProjectVerificationFailed);
    }

    #[test]
    fn a_project_that_builds_and_tests_passes() {
        // POSITIVE CONTROL. Without it, a verifier that rejected everything would satisfy every
        // failure test below and make `renvor new` impossible to use.
        let dir = project("fn main() {}\n");
        in_staging(dir.path(), &silent(), Smoke::Exits).expect("a correct project must verify");
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
            "the failing check must name the request it sent: {:?}",
            error.details
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
        for (name, value) in &sealed {
            let value = value.to_string_lossy();
            for secret in [
                "s3cr3t-proxy-pass",
                "hunter2",
                "pw-9f",
                "alice",
                "bob",
                "carol",
            ] {
                for form in every_form_of(secret) {
                    assert!(
                        !value.contains(&form),
                        "{} carries {secret:?} as {form:?}: {value}",
                        name.to_string_lossy()
                    );
                }
            }
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
        for secret in ["s3cr3t-proxy-pass", "hunter2", "alice", "bob"] {
            for form in every_form_of(secret) {
                assert!(
                    !reported.contains(&form),
                    "{secret} as {form:?}: {reported}"
                );
            }
        }
        assert!(reported.contains("http://<credential removed>@proxy.example:3128/x"));
        assert!(reported.contains("socks5://<credential removed>@10.0.0.1:1080"));
        assert!(
            reported.contains("http://host:80/ok"),
            "a credential-free URL is untouched"
        );
        assert!(
            reported.contains("\\u{1b}[31m"),
            "the escape is escaped: {reported}"
        );
        assert!(!reported.contains('\u{1b}'));
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
        std::fs::write(
            dir.path().join("build.rs"),
            "fn main() {\n    let mut seen: Vec<String> = std::env::vars()\n        .filter(|(name, _)| name.to_ascii_lowercase().contains(\"proxy\"))\n        .map(|(name, value)| format!(\"{name}={value}\"))\n        .collect();\n    seen.sort();\n    eprintln!(\n        \"proxy variables seen by the build script: {}\",\n        seen.join(\" \")\n    );\n    std::process::exit(1);\n}\n",
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
            "the build script's output must be embedded, or this control proves nothing:\n{}",
            error.message
        );
        assert!(
            error.message.contains("127.0.0.1:1"),
            "the credential-free host survives:\n{}",
            error.message
        );
        for form in every_form_of(secret) {
            assert!(
                !error.message.contains(&form),
                "the verification error carries the proxy credential as {form:?}:\n{}",
                error.message
            );
        }
        assert!(
            !error.message.contains("alice"),
            "the user name is part of the credential:\n{}",
            error.message
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
        assert!(
            error.message.contains("fn main() {}"),
            "the formatter's own diff must reach the message: {}",
            error.message
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
}
