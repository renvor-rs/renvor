//! `renvor routes` — the route table an application would actually serve.
//!
//! # Where the route metadata comes from, stated truthfully
//!
//! This command **cannot dynamically load arbitrary compiled Rust**, and it does not pretend to.
//! It also does not parse the project's source, because a parser and a router are two things that
//! can disagree, and contract C-9 forbids a second route list that can drift.
//!
//! So it asks the **application binary** for its own registry, through a documented convention:
//! a Renvor application answers [`DUMP_FLAG`] by printing its route registry as the `result`
//! payload of the C-2 envelope. The registry that answers is the same value that built the
//! router, which is what makes agreement structural rather than maintained.
//!
//! # The limitation this command currently has, and why it is not hidden
//!
//! **No Renvor crate is published**, so no project the current generator produces depends on the
//! framework, and none of them can answer the dump. This command therefore succeeds against
//! **none** of them today.
//!
//! It reports that with [`Code::TransportNotWired`], exit `3`, and details naming the reason. It
//! does **not** print an empty route table and exit `0`: an empty success is indistinguishable, to
//! a consumer, from an application that genuinely declares no routes, and the two mean entirely
//! different things.

use std::path::Path;

use serde::Deserialize;

use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;

/// The documented invocation an application binary answers with its route registry.
///
/// A long, prefixed name on purpose: it must not collide with a flag an application defines for
/// itself, and it must be obvious in a process list that Renvor asked for it.
pub const DUMP_FLAG: &str = "--renvor-dump-routes";

/// The bound on a manifest read, matching `check`'s.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

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

/// Runs the command against a project directory.
///
/// # Errors
///
/// - [`Code::ManifestInvalid`] if `renvor.toml` is absent or unreadable.
/// - [`Code::TransportNotWired`] if the project declares no Renvor dependency, and therefore has
///   no route registry to report.
pub fn run(reporter: &Reporter, path: &Path) -> Result<Exit, CliError> {
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

    let transport = manifest
        .project
        .transport
        .unwrap_or_else(|| "none".to_owned());

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

    // Reaching here means the project DOES depend on the framework, so asking its binary is the
    // next step. Obtaining the dump is deliberately NOT implemented as a fallback to parsing the
    // project's source: a source parser and a router are two things that can disagree, which is
    // exactly what contract C-9 forbids. If the registry cannot be obtained, that is reported.
    let _ = reporter;
    Err(CliError::new(
        Code::TransportNotWired,
        format!(
            "this project depends on Renvor, but its route registry could not be obtained. Run \
             `cargo run -- {DUMP_FLAG}` in the project to see why"
        ),
    )
    .with("transport", transport)
    .with("reason", "dump_unavailable")
    .with("invocation", DUMP_FLAG))
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
    use super::{DUMP_FLAG, declares_renvor_dependency, run};
    use crate::exit::Code;
    use crate::output::{Format, Reporter};

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

    #[test]
    fn a_project_without_a_renvor_dependency_is_refused_by_name() {
        // The headline behaviour of this command today, and it is a FAILURE rather than an empty
        // success — because an empty success is indistinguishable from "this app has no routes".
        let dir = project(MANIFEST, NO_DEPENDENCY);
        let error = run(&reporter(), dir.path()).expect_err("must be refused");

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
    fn a_project_that_does_depend_on_renvor_is_not_refused_for_that_reason() {
        // POSITIVE CONTROL. Without it, a command that always reported `no_renvor_dependency`
        // would pass the test above and the detection would be unproven.
        let with_dependency =
            "[package]\nname = \"demo\"\n\n[dependencies]\nrenvor = { version = \"0.1\" }\n";
        let dir = project(MANIFEST, with_dependency);
        let error = run(&reporter(), dir.path()).expect_err("the dump is not reachable in a test");

        assert_eq!(error.code, Code::TransportNotWired);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "reason" && v == "dump_unavailable"),
            "a project WITH the dependency reported the wrong reason: {:?}",
            error.details
        );
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "invocation" && v == DUMP_FLAG)
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

    #[test]
    fn a_missing_cargo_manifest_fails_closed() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(!declares_renvor_dependency(dir.path()));
    }

    #[test]
    fn a_directory_that_is_not_a_renvor_project_is_reported_as_such() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = run(&reporter(), dir.path()).expect_err("must be refused");
        assert_eq!(error.code, Code::ManifestInvalid);
    }
}
