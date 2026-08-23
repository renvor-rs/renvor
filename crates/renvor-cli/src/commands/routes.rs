//! `renvor routes` — the route table an application would actually serve.
//!
//! # Where the route metadata comes from, stated truthfully
//!
//! This command **cannot dynamically load arbitrary compiled Rust**, and it does not pretend to.
//! It also does not parse the project's source, because a parser and a router are two things that
//! can disagree, and contract C-9 forbids a second route list that can drift.
//!
//! So it asks the **application binary** for its own registry, through an explicit versioned
//! protocol:
//!
//! ```text
//!   renvor routes                                    the project's binary
//!        │                                                    │
//!        │  cargo run --quiet -- --renvor-dump-routes         │
//!        ├───────────────────────────────────────────────────►│
//!        │                                                    │  answer_dump_request(argv, &registry)
//!        │                                                    │  — BEFORE any boot work
//!        │  one C-2 envelope on stdout, protocol 1            │
//!        │◄───────────────────────────────────────────────────┤
//!        │                                                    │
//!   relay result.routes verbatim
//! ```
//!
//! Four properties of that protocol are load-bearing, and each is asserted below:
//!
//! - **One source.** The payload is rendered from the registry that builds the router. There is no
//!   second manifest and no source parsing.
//! - **Versioned.** `result.protocol` is checked before the payload is read. An unrecognised
//!   version is refused **by name** rather than parsed on a best-effort basis.
//! - **No binary discovery.** The invocation is `cargo run` in the project directory — the
//!   project's own declared default binary. Nothing searches `target/` for something executable.
//! - **No boot side effects.** The application answers and exits before it starts anything. A
//!   protocol that required booting would make listing routes bind ports and run migrations.
//!
//! # What still cannot happen today, stated rather than hidden
//!
//! **No Renvor crate is published**, so no project the current generator produces depends on the
//! framework, and none of them can answer the dump. This command therefore succeeds against **none
//! of them today** — not because the relay is missing, but because there is nothing published for a
//! generated project to depend on.
//!
//! It reports that with [`Code::TransportNotWired`], exit `3`, and details naming the reason. It
//! does **not** print an empty route table and exit `0`: an empty success is indistinguishable, to
//! a consumer, from an application that genuinely declares no routes, and the two mean entirely
//! different things.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;
use crate::output::layout::Report;

/// The documented invocation an application binary answers with its route registry.
///
/// **Stated here as a literal on purpose.** `renvor-cli` is `publish = false` and must not depend
/// on `renvor-http`, so it cannot import the library's constant — and the two would then be able to
/// drift. The guard against that is `the_dump_flag_matches_the_librarys_constant` below, which
/// reads the library's source and fails if the two spellings differ.
pub const DUMP_FLAG: &str = "--renvor-dump-routes";

/// The payload version this command understands.
///
/// Guarded against the library's own constant by `the_protocol_version_matches_the_librarys`, for
/// the same reason and by the same mechanism as [`DUMP_FLAG`].
pub const DUMP_PROTOCOL: u32 = 1;

/// The bound on a manifest read, matching `check`'s.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// The bound on what the project binary may print, **measured after the read, not during it**.
///
/// `MAX_ROUTES` is 4096 and a generous row is well under 256 bytes, so this is roughly an order of
/// magnitude above any legitimate answer, and an over-sized answer is refused by name.
///
/// # The limitation, stated rather than implied
///
/// `Command::output()` reads the child's stdout to EOF into memory *before* this comparison can
/// run, so the bound **rejects** an over-sized answer without **preventing** the allocation. An
/// earlier version of this comment claimed it stopped a project "exhausting this process's
/// memory". It does not, and the claim is withdrawn rather than left standing — a constant whose
/// documentation promises more than its code delivers is the same error `server.rs` records under
/// *"the bound has to be applied, not merely computed"*, one register lower.
///
/// The realistic trigger is a **bug, not an attack**: a project binary that does not recognise the
/// dump flag, ignores it, boots normally, and streams logs forever. By the time stdout is read,
/// `cargo run` has already compiled and executed that project's build script and binary, so a
/// hostile project has far more direct capabilities than memory pressure.
///
/// Fixing it properly means spawning with a piped stdout, reading at most `MAX_DUMP_BYTES + 1`,
/// and **killing the child** on overflow — without the kill, a full pipe blocks the child and the
/// hang replaces the allocation. That is a behavioural change with a cross-platform subprocess
/// test behind it, and it is recorded here rather than attempted late. Found by review.
const MAX_DUMP_BYTES: usize = 4 * 1024 * 1024;

/// Just enough of `renvor.toml` to answer "is this a Renvor project, and what transport?".
///
/// Deliberately **not** `deny_unknown_fields` and deliberately not the full manifest type: this
/// command's job is route inspection, not manifest validation. `renvor check` owns that, and
/// duplicating it here would produce two validators that can disagree about the same file.
#[derive(Debug, Deserialize)]
struct ManifestHead {
    project: ProjectHead,
}

#[derive(Debug, Deserialize)]
struct ProjectHead {
    transport: Option<String>,
}

/// One route as the application reported it.
#[derive(Debug, Deserialize, serde::Serialize)]
struct RouteRow {
    method: String,
    path: String,
    group: Option<String>,
}

/// The `result` payload the application printed.
#[derive(Debug, Deserialize, serde::Serialize)]
struct RouteReport {
    protocol: u32,
    routes: Vec<RouteRow>,
}

/// How the project binary is invoked.
///
/// A value rather than a hard-coded command, so the relay can be driven in a test by a program
/// that answers the protocol. The **default** is the documented invocation; a test supplying its
/// own is testing the same parsing, validation, and rendering a real project goes through.
#[derive(Debug, Clone)]
pub struct DumpInvocation {
    program: OsString,
    arguments: Vec<OsString>,
    directory: PathBuf,
}

impl DumpInvocation {
    /// The documented default: the project's own default binary, run through `cargo`.
    ///
    /// `cargo run` rather than a search of `target/`: the project declares its binary, and asking
    /// its build tool to run "the binary this package defines" is the only way to get that answer
    /// without guessing. A package with several binaries and no default gets `cargo`'s own error,
    /// which names the problem better than a guess would.
    #[must_use]
    pub fn for_project(directory: &Path) -> Self {
        Self {
            program: OsString::from("cargo"),
            arguments: vec![
                OsString::from("run"),
                OsString::from("--quiet"),
                OsString::from("--"),
                OsString::from(DUMP_FLAG),
            ],
            directory: directory.to_path_buf(),
        }
    }

    /// Builds an invocation that runs `program` with `arguments` in `directory`.
    ///
    /// `#[cfg(test)]` because production has exactly one invocation — the documented one above.
    /// Leaving it compiled into the shipped binary would offer a way to point route inspection at
    /// an arbitrary program, which is a capability nothing asked for.
    #[cfg(test)]
    #[must_use]
    pub fn new(
        program: impl Into<OsString>,
        arguments: Vec<OsString>,
        directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            program: program.into(),
            arguments,
            directory: directory.into(),
        }
    }
}

/// Runs the command against a project directory.
///
/// # Errors
///
/// - [`Code::ManifestInvalid`] if `renvor.toml` is absent or unreadable.
/// - [`Code::TransportNotWired`] if the project declares no Renvor dependency, if its binary could
///   not be run, if it answered nothing usable, or if it answered a protocol version this command
///   does not understand. Every one of those is a **failure**, never an empty success.
pub fn run(reporter: &Reporter, path: &Path) -> Result<Exit, CliError> {
    run_with(reporter, path, &DumpInvocation::for_project(path))
}

/// The relay, with the invocation supplied.
pub(crate) fn run_with(
    reporter: &Reporter,
    path: &Path,
    invocation: &DumpInvocation,
) -> Result<Exit, CliError> {
    let transport = read_transport(path)?;

    // The question this command has to answer before it can run anything: does the project depend
    // on the framework at all? Without that dependency there is no registry, and no binary that
    // understands the dump flag.
    if !declares_renvor_dependency(path) {
        return Err(CliError::new(
            Code::TransportNotWired,
            "this project declares no Renvor dependency, so it has no route registry to report. \
             Route inspection asks the application binary for its own registry — the only source \
             that cannot drift from the router — and a project that does not depend on the \
             framework has none. See the `Serving HTTP` section of the project's README",
        )
        .with("transport", transport)
        .with("reason", "no_renvor_dependency"));
    }

    let report = obtain_dump(invocation, &transport)?;

    let mut human = Report::new();
    if report.routes.is_empty() {
        // A registry that genuinely holds no routes is a fact, and saying so is different from
        // printing an empty table. This is still exit `0`: the application ANSWERED, and its answer
        // was "none". That is not the same as SC-019's forbidden case, which is exiting `0` when
        // the registry could not be obtained at all.
        human = human.item("the application declares no routes");
    } else {
        for route in &report.routes {
            human = human.row(
                format!("{} {}", route.method, route.path),
                route.group.clone().unwrap_or_else(|| "-".to_owned()),
            );
        }
    }

    // The payload is relayed as the application produced it. Re-deriving it here would be the
    // second source C-9 prohibits, one process further along.
    let result = serde_json::to_value(&report).map_err(|error| {
        CliError::new(
            Code::Internal,
            format!("the relayed route report could not be re-encoded: {error}"),
        )
    })?;

    Ok(reporter.finish("routes", &human, result))
}

/// Reads the recorded transport, or reports why the manifest could not be read.
fn read_transport(path: &Path) -> Result<String, CliError> {
    let manifest_path = path.join("renvor.toml");

    let text = std::fs::read_to_string(&manifest_path).map_err(|error| {
        CliError::new(
            Code::ManifestInvalid,
            format!(
                "`{}` could not be read: {error}",
                crate::output::redact::path(&manifest_path)
            ),
        )
        .with("field", "renvor.toml")
        .with("constraint", "must exist and be readable")
    })?;

    if text.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(CliError::new(
            Code::BoundExceeded,
            format!("`renvor.toml` is above the {MAX_MANIFEST_BYTES}-byte limit"),
        )
        .with("bound", "manifest_bytes")
        .with("limit", MAX_MANIFEST_BYTES.to_string()));
    }

    let manifest: ManifestHead = toml::from_str(&text).map_err(|error| {
        CliError::new(
            Code::ManifestInvalid,
            format!("`renvor.toml` is not a valid renvor manifest: {error}"),
        )
        .with("field", "renvor.toml")
        .with("constraint", "must parse as a renvor manifest")
    })?;

    Ok(manifest
        .project
        .transport
        .unwrap_or_else(|| "none".to_owned()))
}

/// Runs the project binary and reads its answer.
///
/// Every failure below is reported with the reason named in `details.reason`, so a consumer can
/// tell "the binary would not build" from "the binary answered something I cannot read". Both used
/// to be indistinguishable, because neither was reachable.
fn obtain_dump(invocation: &DumpInvocation, transport: &str) -> Result<RouteReport, CliError> {
    let refuse = |reason: &'static str, message: String| {
        CliError::new(Code::TransportNotWired, message)
            .with("transport", transport.to_owned())
            .with("reason", reason)
            .with("invocation", DUMP_FLAG)
    };

    let output = Command::new(&invocation.program)
        .args(&invocation.arguments)
        .current_dir(&invocation.directory)
        // `stderr` is INHERITED, not captured. A build takes time and prints progress, and
        // swallowing it would leave an operator watching a silent command. C-1 reserves `stdout`
        // for the result, which is exactly what is captured here and nothing else.
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|error| {
            refuse(
                "invocation_failed",
                format!(
                    "the project's binary could not be run: {error}. Route inspection runs the \
                     application and asks it for its own registry"
                ),
            )
        })?;

    if !output.status.success() {
        return Err(refuse(
            "dump_failed",
            format!(
                "the project's binary exited with {} when asked for its routes. Run `cargo run -- \
                 {DUMP_FLAG}` in the project to see why",
                output.status
            ),
        ));
    }

    if output.stdout.len() > MAX_DUMP_BYTES {
        return Err(CliError::new(
            Code::BoundExceeded,
            format!("the route dump is above the {MAX_DUMP_BYTES}-byte limit"),
        )
        .with("bound", "dump_bytes")
        .with("limit", MAX_DUMP_BYTES.to_string()));
    }

    let text = String::from_utf8(output.stdout).map_err(|_| {
        refuse(
            "dump_unreadable",
            "the project's binary answered with bytes that are not UTF-8".to_owned(),
        )
    })?;

    let envelope: serde_json::Value = serde_json::from_str(text.trim()).map_err(|error| {
        refuse(
            "dump_unreadable",
            format!(
                "the project's binary did not answer with one JSON document: {error}. It must \
                 print exactly the envelope `renvor_http::route::inspect::answer_dump_request` \
                 returns, and nothing else, on stdout"
            ),
        )
    })?;

    if envelope.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return Err(refuse(
            "dump_failed",
            "the project's binary reported a failure rather than its routes".to_owned(),
        ));
    }

    let payload = envelope.get("result").ok_or_else(|| {
        refuse(
            "dump_unreadable",
            "the answer carried no `result` payload".to_owned(),
        )
    })?;

    // THE VERSION CHECK, BEFORE THE PAYLOAD IS READ. An unrecognised version is refused rather
    // than parsed leniently: a route table that silently lost a column is precisely the quietly
    // wrong output this command exists not to produce.
    let declared = payload
        .get("protocol")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            refuse(
                "protocol_unstated",
                "the answer did not state which route-dump protocol it speaks".to_owned(),
            )
        })?;

    if declared != u64::from(DUMP_PROTOCOL) {
        return Err(refuse(
            "protocol_unsupported",
            format!(
                "the project answered route-dump protocol {declared}; this `renvor` understands \
                 {DUMP_PROTOCOL}. Refusing rather than guessing at a shape it does not know"
            ),
        ));
    }

    serde_json::from_value(payload.clone()).map_err(|error| {
        refuse(
            "dump_unreadable",
            format!("the route payload could not be read: {error}"),
        )
    })
}

/// Whether the project's `Cargo.toml` names `renvor` as a dependency.
///
/// A line-oriented scan rather than a full manifest parse. The question is narrow — "does the word
/// `renvor` appear as a dependency key?" — and a full parse would pull the whole dependency-table
/// shape into a command that has no other use for it.
///
/// **Fails closed**: an unreadable or absent `Cargo.toml` answers `false`, which produces a
/// reported failure rather than an attempt to run a binary that may not exist.
fn declares_renvor_dependency(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path.join("Cargo.toml")) else {
        return false;
    };

    let mut in_dependencies = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_dependencies = line == "[dependencies]" || line.starts_with("[dependencies.");
            // `[dependencies.renvor]` is a dependency declaration in its own right.
            if line == "[dependencies.renvor]" {
                return true;
            }
            continue;
        }
        if in_dependencies
            && let Some((key, _)) = line.split_once('=')
            && key.trim() == "renvor"
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        DUMP_FLAG, DUMP_PROTOCOL, DumpInvocation, declares_renvor_dependency, obtain_dump, run_with,
    };
    use crate::exit::Code;
    use crate::output::{Format, Reporter};
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    fn reporter() -> Reporter {
        Reporter::new(Format::Human, true)
    }

    fn project(manifest: &str, cargo: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("renvor.toml"), manifest).expect("write");
        std::fs::write(dir.path().join("Cargo.toml"), cargo).expect("write");
        dir
    }

    const MANIFEST: &str = r#"
[renvor]
generator_version = "0.0.0"
template_version = "2"

[project]
name = "demo"
target = "api"
transport = "rest"
local_domain = "demo.test"
container = false
local_https = "off"
example_domain = false
seed_data = false
"#;

    const NO_DEPENDENCY: &str = "[package]\nname = \"demo\"\n\n[dependencies]\n";
    const WITH_DEPENDENCY: &str =
        "[package]\nname = \"demo\"\n\n[dependencies]\nrenvor = { version = \"0.1\" }\n";

    /// The workspace root, from this crate's manifest directory.
    fn workspace_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("the workspace root resolves")
    }

    /// Writes `payload` where [`answering`] reads it back, and returns the **bare** file name.
    ///
    /// The payload goes through a file rather than being interpolated into a shell command. A
    /// payload containing a quote would otherwise break the command it was embedded in — and these
    /// tests exist precisely to feed the relay malformed input. The name is bare, and the
    /// invocation already runs in `directory`, so no temporary path is ever re-parsed by a shell.
    fn stage_payload(directory: &Path, payload: &str) -> &'static str {
        const FILE: &str = "dump.json";
        std::fs::write(directory.join(FILE), payload).expect("the payload is written");
        FILE
    }

    // These helpers are split per platform because **`/bin/sh` does not exist on Windows**, and an
    // earlier revision hard-coded it. Six tests passed on macOS and failed on Windows CI with
    // `os error 3`. The split is here rather than hidden behind a shell abstraction so that the
    // next person to add a helper sees the constraint instead of rediscovering it.

    /// An invocation that answers with `payload` and exits zero.
    #[cfg(not(windows))]
    fn answering(directory: &Path, payload: &str) -> DumpInvocation {
        let file = stage_payload(directory, payload);
        DumpInvocation::new("/bin/cat", vec![OsString::from(file)], directory)
    }

    /// An invocation that answers with `payload` and exits zero.
    #[cfg(windows)]
    fn answering(directory: &Path, payload: &str) -> DumpInvocation {
        let file = stage_payload(directory, payload);
        DumpInvocation::new(
            "cmd",
            vec![
                OsString::from("/C"),
                OsString::from("type"),
                OsString::from(file),
            ],
            directory,
        )
    }

    /// An invocation that exits non-zero without answering.
    #[cfg(not(windows))]
    fn failing(directory: &Path) -> DumpInvocation {
        DumpInvocation::new(
            "/bin/sh",
            vec![OsString::from("-c"), OsString::from("exit 7")],
            directory,
        )
    }

    /// An invocation that exits non-zero without answering.
    #[cfg(windows)]
    fn failing(directory: &Path) -> DumpInvocation {
        DumpInvocation::new(
            "cmd",
            vec![OsString::from("/C"), OsString::from("exit 7")],
            directory,
        )
    }

    fn envelope(result: &str) -> String {
        format!(r#"{{"schemaVersion":2,"status":"success","command":"routes","result":{result}}}"#)
    }

    #[test]
    fn a_binary_that_answers_the_protocol_is_relayed_to_a_successful_exit() {
        // THE SUCCESS PATH. Every route through this command used to return `Err`, so the command
        // could not succeed against any input whatsoever — the relay was not written.
        let dir = project(MANIFEST, WITH_DEPENDENCY);
        let payload =
            r#"{"protocol":1,"routes":[{"method":"GET","path":"/health","group":"api"}]}"#;

        let exit = run_with(
            &reporter(),
            dir.path(),
            &answering(dir.path(), &envelope(payload)),
        )
        .expect("a binary that answers the protocol must be relayed");

        assert_eq!(exit.code(), 0, "the relayed answer did not exit 0");
    }

    #[test]
    fn the_relayed_answer_carries_the_routes_the_binary_declared() {
        // The test above asserts exit 0 — which a relay that printed NOTHING would also satisfy.
        // An empty table and a successful exit is precisely the outcome C-9 forbids, so exit code
        // alone is not evidence that the relay relayed anything. This asserts the payload.
        let dir = project(MANIFEST, WITH_DEPENDENCY);
        let payload =
            r#"{"protocol":1,"routes":[{"method":"GET","path":"/health","group":"api"}]}"#;

        let report = obtain_dump(&answering(dir.path(), &envelope(payload)), "rest")
            .expect("a binary that answers the protocol must be relayed");

        assert_eq!(report.protocol, DUMP_PROTOCOL);
        assert_eq!(report.routes.len(), 1, "the declared route was dropped");
        assert_eq!(report.routes[0].method, "GET");
        assert_eq!(report.routes[0].path, "/health");
        assert_eq!(report.routes[0].group.as_deref(), Some("api"));
    }

    #[test]
    fn an_application_declaring_no_routes_still_succeeds_and_says_so() {
        // An application that ANSWERED "none" is not the forbidden case. SC-019 forbids exiting `0`
        // when the registry could not be OBTAINED — the two are different facts and this command
        // must not conflate them.
        let dir = project(MANIFEST, WITH_DEPENDENCY);
        let payload = r#"{"protocol":1,"routes":[]}"#;

        let exit = run_with(
            &reporter(),
            dir.path(),
            &answering(dir.path(), &envelope(payload)),
        )
        .expect("an explicit empty answer is still an answer");
        assert_eq!(exit.code(), 0);
    }

    #[test]
    fn a_newer_protocol_is_refused_by_name_rather_than_parsed_leniently() {
        let dir = project(MANIFEST, WITH_DEPENDENCY);
        let payload = r#"{"protocol":99,"routes":[{"method":"GET","path":"/x","group":null}]}"#;

        let error = run_with(
            &reporter(),
            dir.path(),
            &answering(dir.path(), &envelope(payload)),
        )
        .expect_err("an unknown protocol must be refused");

        assert_eq!(error.code, Code::TransportNotWired);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "protocol_unsupported"),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn an_answer_with_no_protocol_field_is_refused() {
        let dir = project(MANIFEST, WITH_DEPENDENCY);
        let payload = r#"{"routes":[]}"#;

        let error = run_with(
            &reporter(),
            dir.path(),
            &answering(dir.path(), &envelope(payload)),
        )
        .expect_err("an unversioned answer must be refused");

        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "protocol_unstated"),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn an_unparseable_answer_is_refused_rather_than_treated_as_no_routes() {
        // The failure mode this command exists to avoid: something that is not a route table must
        // never become an empty route table.
        let dir = project(MANIFEST, WITH_DEPENDENCY);

        let error = run_with(
            &reporter(),
            dir.path(),
            &answering(dir.path(), "not json at all"),
        )
        .expect_err("unparseable output must be refused");

        assert_eq!(error.code, Code::TransportNotWired);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "dump_unreadable"),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn a_binary_that_exits_non_zero_is_refused_by_name() {
        let dir = project(MANIFEST, WITH_DEPENDENCY);

        let error = run_with(&reporter(), dir.path(), &failing(dir.path()))
            .expect_err("a failing binary must be refused");

        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "dump_failed"),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn a_binary_that_cannot_be_run_at_all_is_refused_by_name() {
        let dir = project(MANIFEST, WITH_DEPENDENCY);
        let missing = DumpInvocation::new(
            "/nonexistent/renvor-test-binary",
            Vec::new(),
            dir.path().to_path_buf(),
        );

        let error = run_with(&reporter(), dir.path(), &missing)
            .expect_err("an unrunnable binary must be refused");

        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "invocation_failed"),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn a_project_without_a_renvor_dependency_is_refused_before_anything_is_run() {
        // Checked BEFORE the binary is invoked: a project with no framework dependency has no
        // registry, and running it would be running an unrelated program.
        let dir = project(MANIFEST, NO_DEPENDENCY);
        let error =
            run_with(&reporter(), dir.path(), &failing(dir.path())).expect_err("must be refused");

        assert_eq!(error.code, Code::TransportNotWired);
        assert_eq!(error.code.exit().code(), 3);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "no_renvor_dependency"),
            "{:?}",
            error.details
        );
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "transport" && v == "rest"),
            "the refusal must name the recorded transport: {:?}",
            error.details
        );
    }

    #[test]
    fn dependency_detection_reads_the_dependencies_table_only() {
        let dir = tempfile::tempdir().expect("tempdir");

        // A mention outside `[dependencies]` is not a dependency.
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"renvor\"\n\n[dependencies]\n",
        )
        .expect("write");
        assert!(
            !declares_renvor_dependency(dir.path()),
            "a package NAMED renvor was mistaken for a dependency on it"
        );

        // A dev-dependency is not a dependency the binary can use at runtime.
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n\n[dev-dependencies]\nrenvor = \"0.1\"\n",
        )
        .expect("write");
        assert!(!declares_renvor_dependency(dir.path()));

        // POSITIVE CONTROLS: both declaration forms ARE detected.
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n\n[dependencies]\nrenvor = \"0.1\"\n",
        )
        .expect("write");
        assert!(declares_renvor_dependency(dir.path()));

        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n\n[dependencies.renvor]\nversion = \"0.1\"\n",
        )
        .expect("write");
        assert!(declares_renvor_dependency(dir.path()));
    }

    /// Reads a constant's value out of the library's source.
    ///
    /// Located by the declaration's NAME and then by the next literal, rather than by assuming the
    /// whole declaration sits on one line. A `rustfmt` wrap used to make this panic with "the
    /// library still declares DUMP_FLAG", which reads as deletion rather than as reformatting — a
    /// guard whose failure message misidentifies the cause is worse than a brittle guard.
    fn library_constant(source: &str, declaration: &str, opener: char, closer: char) -> String {
        let after_name = source
            .split_once(declaration)
            .unwrap_or_else(|| panic!("the library still declares `{declaration}`"))
            .1;
        let start = after_name
            .find(opener)
            .expect("the declaration assigns a literal");
        let rest = &after_name[start + 1..];
        rest[..rest.find(closer).expect("the literal is terminated")].to_owned()
    }

    const LIBRARY: &str = include_str!("../../../renvor-http/src/route/inspect.rs");

    #[test]
    fn the_dump_flag_matches_the_librarys_constant() {
        // TWO DECLARATIONS, ONE VALUE. The CLI cannot import from `renvor-http` — it is
        // `publish = false` and depending on the transport would put the transport in the CLI's
        // graph — so the constant is stated twice. This reads the library's source at compile time
        // and fails if they ever differ, which is the whole reason a second declaration is
        // tolerable at all.
        assert_eq!(
            library_constant(LIBRARY, "pub const DUMP_FLAG: &str", '"', '"'),
            DUMP_FLAG,
            "the CLI and the library disagree about the route-dump invocation"
        );
    }

    #[test]
    fn the_protocol_version_matches_the_librarys() {
        // The same guard applied to the version. Without it the CLI could refuse every real
        // application as speaking an unsupported protocol, which is the worst possible outcome of
        // a version check.
        let declared = library_constant(LIBRARY, "pub const ROUTE_DUMP_PROTOCOL: u32", '=', ';');
        assert_eq!(
            declared.trim().parse::<u32>().expect("a numeric literal"),
            DUMP_PROTOCOL,
            "the CLI and the library disagree about the route-dump protocol version"
        );
    }

    #[test]
    fn a_missing_cargo_manifest_fails_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!declares_renvor_dependency(dir.path()));
    }

    #[test]
    fn a_directory_that_is_not_a_renvor_project_is_reported_as_such() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error =
            run_with(&reporter(), dir.path(), &failing(dir.path())).expect_err("must be refused");
        assert_eq!(error.code, Code::ManifestInvalid);
    }

    #[test]
    #[ignore = "builds and runs a real example binary; run with --ignored or in the full gate"]
    fn the_relay_reads_what_the_real_library_actually_prints() {
        // THE END-TO-END PROOF, and the reason the fixtures above are not the whole story.
        //
        // Every other test in this file feeds the relay a payload a HUMAN wrote. This one runs the
        // real `renvor-http` example, which answers through the real
        // `inspect::answer_dump_request`, and relays what it actually printed. If the library's
        // envelope shape ever changes, this fails and the hand-written fixtures do not.
        //
        // `#[ignore]` because it compiles a second crate's example; the verification gate runs it
        // explicitly.
        let dir = project(MANIFEST, WITH_DEPENDENCY);

        let real = DumpInvocation::new(
            "cargo",
            vec![
                OsString::from("run"),
                OsString::from("--quiet"),
                OsString::from("-p"),
                OsString::from("renvor-http"),
                OsString::from("--example"),
                OsString::from("route_dump"),
                OsString::from("--"),
                OsString::from(DUMP_FLAG),
            ],
            workspace_root(),
        );

        let exit = run_with(&reporter(), dir.path(), &real)
            .expect("the real library's answer must be relayed");
        assert_eq!(exit.code(), 0);
    }
}
