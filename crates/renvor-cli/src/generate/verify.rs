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

/// Keeps, in order, the variables of `parent` that [`PASSED_THROUGH`] names.
///
/// A pure function of its input, so a test can prove the seal without touching the process
/// environment — which this crate forbids `unsafe` code to do anyway.
pub fn sealed_environment(
    parent: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    parent
        .filter(|(name, _)| {
            name.to_str()
                .is_some_and(|name| PASSED_THROUGH.contains(&name))
        })
        .collect()
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
    let target = target_directory(std::env::var_os("CARGO_TARGET_DIR").as_deref())?;

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
            .envs(sealed_environment(std::env::vars_os()))
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
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
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
        let sealed = sealed_environment(parent.into_iter());
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
