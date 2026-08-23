//! `renvor openapi` — the OpenAPI description this project would publish.
//!
//! # Where the description comes from, stated truthfully
//!
//! The same way `renvor routes` obtains a route table, and for the same reasons. This command
//! **cannot dynamically load arbitrary compiled Rust**, and it does not pretend to. It also does
//! not parse the project's source, because a parser and a router are two things that can disagree.
//!
//! So it asks the **application binary** for its own description, through an explicit versioned
//! protocol:
//!
//! ```text
//!   renvor openapi                                    the project's binary
//!        │                                                     │
//!        │  cargo run --quiet -- --renvor-dump-openapi         │
//!        ├────────────────────────────────────────────────────►│
//!        │                                                     │  answer_openapi_request(argv, &registry, info)
//!        │                                                     │  — BEFORE any boot work
//!        │  one C-2 envelope on stdout, protocol 1             │
//!        │◄────────────────────────────────────────────────────┤
//!        │                                                     │
//!   relay result.document verbatim
//! ```
//!
//! Five properties are load-bearing, and each is asserted:
//!
//! - **One source.** The description is rendered from the registry that builds the router.
//! - **Versioned.** `result.protocol` is checked before the payload is read. An unrecognised
//!   version is refused **by name** rather than parsed on a best-effort basis.
//! - **No binary discovery.** The project's own declared default binary, through its build tool.
//! - **No boot side effects.** The application answers and exits before it starts anything.
//! - **Bounded in time.** The invocation has a deadline and the child is killed when it elapses —
//!   see [`super::relay`], which closes a gap `renvor routes` recorded against itself.
//!
//! # When the project has no transport wiring
//!
//! It **fails**, with the registered code, exit `3`, and details naming the reason. It does not
//! print an empty description and exit `0`: an empty description and "this project cannot be
//! asked" are entirely different facts.
//!
//! > **Current, dated limitation — 2026-08-23.** No Renvor crate is published, so **no project the
//! > current generator produces depends on the framework**, and `renvor openapi` therefore
//! > succeeds against **none** of them. This is recorded rather than smoothed over: the relay
//! > **is** implemented and **is** asserted end to end against a real binary answering through the
//! > real library — its reach across *generated* projects is what is zero, and it is zero because
//! > nothing is published for them to depend on. This is the same limitation `renvor routes`
//! > carries, in the same words, because it is the same limitation.

use std::path::Path;

use serde::Deserialize;

use super::relay::{Invocation, RelayFailure};
use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;
use crate::output::layout::Report;

/// The documented invocation an application binary answers with its description.
///
/// **Stated here as a literal on purpose.** `renvor-cli` is `publish = false` and must not depend
/// on `renvor-http`, so it cannot import the library's constant — and the two would then be able
/// to drift. The guard against that is `the_dump_flag_matches_the_librarys_constant` below, which
/// reads the library's source and fails if the two spellings differ. Same mechanism, same reason,
/// as `renvor routes`.
pub const OPENAPI_FLAG: &str = "--renvor-dump-openapi";

/// The payload version this command understands.
pub const OPENAPI_PROTOCOL: u32 = 1;

/// The bound on a manifest read, matching `check`'s.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// Just enough of `renvor.toml` to answer "is this a Renvor project, and what transport?".
#[derive(Debug, Deserialize)]
struct ManifestHead {
    project: ProjectHead,
}

#[derive(Debug, Deserialize)]
struct ProjectHead {
    transport: Option<String>,
}

/// Runs the command against a project directory.
///
/// # Errors
///
/// - [`Code::ManifestInvalid`] if `renvor.toml` is absent or unreadable.
/// - [`Code::TransportNotWired`] if the project declares no Renvor dependency, if its binary could
///   not be run, if it did not answer in time, if it answered nothing usable, or if it answered a
///   protocol version this command does not understand. Every one of those is a **failure**, never
///   an empty success.
pub fn run(reporter: &Reporter, path: &Path) -> Result<Exit, CliError> {
    run_with(reporter, path, &Invocation::for_project(path, OPENAPI_FLAG))
}

/// The relay, with the invocation supplied.
pub(crate) fn run_with(
    reporter: &Reporter,
    path: &Path,
    invocation: &Invocation,
) -> Result<Exit, CliError> {
    let transport = read_transport(path)?;

    if !super::routes::declares_renvor_dependency(path) {
        return Err(CliError::new(
            Code::TransportNotWired,
            "this project declares no Renvor dependency, so it has no declarations to describe. \
             The description is asked of the application binary — the only source that cannot \
             drift from the router — and a project that does not depend on the framework has \
             none. See the `Serving HTTP` section of the project's README",
        )
        .with("transport", transport)
        .with("reason", "no_renvor_dependency"));
    }

    let document = obtain(invocation, &transport)?;

    // The human form is a SUMMARY, not the document. A description is thousands of lines, and
    // printing it to a terminal by default would bury the one fact an operator wanted. The
    // document itself goes to `--output json`, or to a file.
    let mut human = Report::new();
    human = human.row(
        "openapi".to_owned(),
        document
            .get("openapi")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
            .to_owned(),
    );
    let operations = count_operations(&document);
    human = human.row("operations".to_owned(), operations.to_string());

    Ok(reporter.finish("openapi", &human, document))
}

/// How many operations the description declares.
fn count_operations(document: &serde_json::Value) -> usize {
    const METHODS: [&str; 6] = ["get", "put", "post", "delete", "patch", "head"];
    document
        .get("paths")
        .and_then(serde_json::Value::as_object)
        .map_or(0, |paths| {
            paths
                .values()
                .map(|item| METHODS.iter().filter(|m| item.get(*m).is_some()).count())
                .sum()
        })
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
/// Every failure is reported with the reason named in `details.reason`, so a consumer can tell
/// "the binary would not build" from "the binary answered something I cannot read" from "the
/// binary never answered".
fn obtain(invocation: &Invocation, transport: &str) -> Result<serde_json::Value, CliError> {
    let refuse = |reason: &'static str, message: String| {
        CliError::new(Code::TransportNotWired, message)
            .with("transport", transport.to_owned())
            .with("reason", reason)
            .with("invocation", OPENAPI_FLAG)
    };

    let text = invocation.run().map_err(|failure| match failure {
        // An over-sized answer is a BOUND failure, not a wiring failure: the project answered, and
        // what it said was too big. Reporting it as `transport_not_wired` would tell an operator
        // to check a dependency that is present.
        RelayFailure::TooLarge { limit } => CliError::new(
            Code::BoundExceeded,
            format!("the description is above the {limit}-byte limit"),
        )
        .with("bound", "dump_bytes")
        .with("limit", limit.to_string()),
        other => {
            let reason = other.reason();
            refuse(reason, other.to_string())
        }
    })?;

    let envelope: serde_json::Value = serde_json::from_str(text.trim()).map_err(|error| {
        refuse(
            "dump_unreadable",
            format!(
                "the project's binary did not answer with one JSON document: {error}. It must \
                 print exactly the envelope \
                 `renvor_http::route::describe::answer_openapi_request` returns, and nothing \
                 else, on stdout"
            ),
        )
    })?;

    if envelope.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return Err(refuse(
            "dump_failed",
            "the project's binary reported a failure rather than its description. Its \
             declarations could not be described — most often a duplicate operation identifier, \
             or an example that does not satisfy its own schema"
                .to_owned(),
        ));
    }

    let payload = envelope.get("result").ok_or_else(|| {
        refuse(
            "dump_unreadable",
            "the answer carried no `result` payload".to_owned(),
        )
    })?;

    // THE VERSION CHECK, BEFORE THE PAYLOAD IS READ. An unrecognised version is refused rather
    // than parsed leniently: a description that silently lost a field is precisely the quietly
    // wrong output this command exists not to produce.
    let declared = payload
        .get("protocol")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            refuse(
                "protocol_unstated",
                "the answer did not state which OpenAPI-dump protocol it speaks".to_owned(),
            )
        })?;

    if declared != u64::from(OPENAPI_PROTOCOL) {
        return Err(refuse(
            "protocol_unsupported",
            format!(
                "the project answered OpenAPI-dump protocol {declared}; this `renvor` understands \
                 {OPENAPI_PROTOCOL}. Refusing rather than guessing at a shape it does not know"
            ),
        ));
    }

    payload.get("document").cloned().ok_or_else(|| {
        refuse(
            "dump_unreadable",
            "the answer carried no description".to_owned(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{OPENAPI_FLAG, OPENAPI_PROTOCOL, run_with};
    use crate::exit::Code;
    use crate::output::{Format, Reporter};
    use std::ffi::OsString;
    use std::time::Duration;

    use crate::commands::relay::Invocation;

    fn reporter() -> Reporter {
        Reporter::new(Format::Json, false)
    }

    /// A project directory that looks wired, so the relay actually runs.
    fn project(answer: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("renvor.toml"),
            "[project]\nname = \"t\"\ntransport = \"rest\"\n",
        )
        .expect("writes");
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"t\"\n\n[dependencies]\nrenvor = \"0.0.0\"\n",
        )
        .expect("writes");
        let _ = answer;
        dir
    }

    fn answering(dir: &Path, script: &str) -> Invocation {
        Invocation::new(
            "sh",
            vec![OsString::from("-c"), OsString::from(script)],
            dir,
        )
    }

    use std::path::Path;

    #[test]
    fn a_project_with_no_renvor_dependency_fails_rather_than_printing_an_empty_description() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(
            dir.path().join("renvor.toml"),
            "[project]\nname = \"t\"\ntransport = \"rest\"\n",
        )
        .expect("writes");
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"t\"\n").expect("writes");

        let error = super::run(&reporter(), dir.path())
            .expect_err("a project with no dependency must fail");
        assert_eq!(error.exit(), Code::TransportNotWired.exit());
        assert_ne!(error.exit().code(), 0, "an empty success was reported");
    }

    #[test]
    fn a_well_formed_answer_is_relayed() {
        let dir = project("");
        let document = r#"{"openapi":"3.2.0","info":{"title":"t","version":"1"},"paths":{"/a":{"get":{"operationId":"x","responses":{}}}}}"#;
        let envelope = format!(
            r#"{{"schemaVersion":2,"status":"success","command":"openapi","result":{{"protocol":{OPENAPI_PROTOCOL},"document":{document}}}}}"#
        );
        let invocation = answering(dir.path(), &format!("printf '%s' '{envelope}'"));

        let exit = run_with(&reporter(), dir.path(), &invocation).expect("the answer is relayed");
        assert_eq!(exit.code(), 0);
    }

    #[test]
    fn an_unsupported_protocol_is_refused_by_name() {
        let dir = project("");
        let envelope = r#"{"schemaVersion":2,"status":"success","command":"openapi","result":{"protocol":99,"document":{}}}"#;
        let invocation = answering(dir.path(), &format!("printf '%s' '{envelope}'"));

        let error = run_with(&reporter(), dir.path(), &invocation)
            .expect_err("an unknown protocol must be refused");
        assert_eq!(error.exit(), Code::TransportNotWired.exit());
        assert!(
            error.to_string().contains("99"),
            "the refusal did not name the version it met: {error}"
        );
    }

    #[test]
    fn a_malformed_answer_is_refused() {
        let dir = project("");
        let invocation = answering(dir.path(), "printf '%s' 'not json at all'");
        let error =
            run_with(&reporter(), dir.path(), &invocation).expect_err("malformed must be refused");
        assert_eq!(error.exit(), Code::TransportNotWired.exit());
    }

    #[test]
    fn a_failure_envelope_is_not_treated_as_a_description() {
        let dir = project("");
        let envelope = r#"{"schemaVersion":2,"status":"failure","command":"openapi","error":{"code":"internal","message":"m","details":{}}}"#;
        let invocation = answering(dir.path(), &format!("printf '%s' '{envelope}'"));
        let error = run_with(&reporter(), dir.path(), &invocation)
            .expect_err("a failure envelope must not become a success");
        assert_eq!(error.exit(), Code::TransportNotWired.exit());
    }

    #[test]
    fn a_binary_that_never_answers_is_refused_at_the_deadline() {
        let dir = project("");
        let invocation = answering(dir.path(), "sleep 60").with_timeout(Duration::from_millis(400));
        let error = run_with(&reporter(), dir.path(), &invocation)
            .expect_err("a binary that never answers must be refused");
        assert_eq!(error.exit(), Code::TransportNotWired.exit());
    }

    #[test]
    fn a_non_zero_exit_is_refused() {
        let dir = project("");
        let invocation = answering(dir.path(), "exit 7");
        let error = run_with(&reporter(), dir.path(), &invocation)
            .expect_err("a non-zero exit must be refused");
        assert_eq!(error.exit(), Code::TransportNotWired.exit());
    }

    #[test]
    fn the_dump_flag_matches_the_librarys_constant() {
        // `renvor-cli` cannot depend on `renvor-http`, so the flag is a literal here — and the two
        // could drift. This reads the library's source and fails if the spellings differ, which is
        // the same guard `renvor routes` uses for its own flag.
        let library = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("crates/")
                .join("renvor-http/src/route/describe.rs"),
        )
        .expect("the library source is readable");

        assert!(
            library.contains(&format!(r#"OPENAPI_DUMP_FLAG: &str = "{OPENAPI_FLAG}""#)),
            "the flag this command sends is not the one the library answers"
        );
        assert!(
            library.contains(&format!("OPENAPI_DUMP_PROTOCOL: u32 = {OPENAPI_PROTOCOL}")),
            "the protocol version this command understands is not the one the library speaks"
        );
    }
}
