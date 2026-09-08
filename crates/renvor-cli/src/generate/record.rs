//! The provenance record every generated project carries: `.renvor/generated.toml`.
//!
//! Contract `template-contract.md` §"The provenance record" (Phase 011, FR-044) and §"Record
//! version 2" (Phase 012, L-2, FR-012-4/5). The generator version, the template version, one
//! `[[file]]` per generated path with its SHA-256 — so a later generator can tell an untouched
//! file from one the user changed **without downloading or evaluating anything** — and, since
//! version 2, `[toolchain]` (what was rendered) and `[verified_with]` (what the five checks
//! observed and what the seal queried). Digests only, never contents; and the record is itself
//! generated, so it appears in the manifest like every other file.
//!
//! # Written after verification, before the manifest walk
//!
//! It lists what the staging tree holds at the moment it is written, which is why it is the last
//! file generation produces: after rendering, after the seeded lockfile, **after verification** —
//! which resolves `Cargo.lock` to the project's own closure, or creates it for a skeleton — and
//! before the manifest. A record written before verification digested a lockfile that no longer
//! existed by the time the project did; one written after the manifest would be absent from it.
//!
//! # `[verified_with]` is measured, never derived (FR-012-4)
//!
//! This module **serialises**; it measures nothing. Every value in [`VerifiedWith`] arrives
//! fully built from the verification that observed it — launch observations from the five checks
//! as they ran under the seal, and the identities the seal queried of the tools those launches
//! named. Nothing here reads `PATH`, the process environment, the pin, an old record, or
//! `.rustc_info.json`; and the reader never fills a missing observed field: a cached check's
//! `rustc_*` stays absent on disk and `None` in memory.
//!
//! # Reader dispatch (FR-012-5b)
//!
//! [`read`] looks at `record_version` first, through a lenient head. **Absent** is a legacy
//! (version 1) record: accepted, with `[toolchain]` and `[verified_with]` reported as unknown.
//! **2** is validated strictly — `deny_unknown_fields` everywhere inside. **Anything else** is
//! refused by name, [`Code::RecordUnsupported`] (exit 3) with `details.record_version` and
//! `details.supported`, before any file is planned or modified.
//!
//! # The incompatibility the other way (FR-012-5c)
//!
//! A `renvor` built from source at or before `7281e4f` has no dispatch: it reads the whole file
//! into the old four-field struct under `#[serde(deny_unknown_fields)]`, so a version-2 record
//! fails there with serde's own *unknown field `record_version`* error inside the existing
//! `manifest_invalid` read failure — a generic parse error, **not** `record_unsupported`, because
//! those binaries do not have this rule. Nothing has been published, but people may have
//! generated projects from source; a project generated at template version 8 or later is not
//! readable by a generator built before this revision — rebuild the generator, not the project.
//! This crate has no library target, so the control lives in the test
//! `an_older_reader_fails_with_serdes_unknown_field_error_not_record_unsupported` rather than in
//! a doc test.

use std::fmt::Write as _;

use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};

use super::manifest::{EntryKind, FileManifest};
use crate::exit::{CliError, Code};
use crate::toolchain::{Observation, RECORD_VERSION, SelectedBy};

/// Where the record lives, relative to the project root.
pub const DIRECTORY: &str = ".renvor";
/// The record's path, relative to the project root, with forward slashes.
pub const PATH: &str = ".renvor/generated.toml";

/// One generated file: its path and the SHA-256 of its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// The path relative to the project root, forward slashes.
    pub path: String,
    /// The hexadecimal SHA-256 of the file as generated.
    pub sha256: String,
}

/// One resource `renvor generate resource` rendered: what it needs to render it again.
///
/// A digest says whether a module was touched; it cannot say what the module was rendered
/// **from**. `renvor generate auth` re-renders every untouched resource module with the session
/// guards the manifest now promises, and for that it needs the name and the fields exactly as
/// they were given (found by the Codex review of Phase 011).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedResource {
    /// The PascalCase type name.
    pub name: String,
    /// The fields as given: `name:type`, in order.
    pub fields: Vec<String>,
}

/// `[toolchain]`: what the templates rendered (FR-012-1), as recorded — never re-derived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Toolchain {
    /// The channel rendered into `rust-toolchain.toml`; `"none"` for a legacy tree that
    /// `generate auth` verified without a pin (FR-012-10a).
    pub pinned: String,
    /// The `rust-version` rendered into `Cargo.toml`; `"none"` for a legacy tree.
    pub rust_version: String,
}

impl Toolchain {
    /// What both fields say when the project declares no toolchain.
    pub const NONE: &'static str = "none";

    /// Whether this record names a pin — the question FR-012-10b's template group switches on.
    ///
    /// # Why the table's existence is not the answer
    ///
    /// `generate auth` writes `[toolchain]` on **every** tree it verifies, because the honest
    /// record of a legacy tree is `none` twice and not an absent table. So "the table exists"
    /// becomes true the first time a legacy project is verified, and a caller reading it as
    /// "declares a pin" renders the pin group into that project on the **next** run — the silent
    /// insertion FR-012-10b forbids, arriving one run late and looking like a fresh decision.
    /// The value is the answer; the table's presence is not.
    #[must_use]
    pub fn declares(&self) -> bool {
        self.pinned != Self::NONE
    }
}

/// The operation whose five checks a `[verified_with]` table describes (FR-012-5a).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    /// `renvor new`, a real run.
    New,
    /// `renvor generate auth`, its scratch verification.
    Auth,
}

impl Operation {
    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Auth => "auth",
        }
    }
}

/// A check that launches no compiler (`fmt`, `run`): its outcome only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    /// `passed`.
    pub outcome: String,
}

/// The clippy check: its outcome, its own units as Cargo reported them, and the observed
/// `clippy-driver` executable's identity — the one Cargo's `Running` line named, from its own
/// version query; **absent** when every clippy unit was `Fresh`, never invented, and never
/// `cargo clippy --version` (A-8 round).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClippyCheck {
    /// `passed`.
    pub outcome: String,
    /// Cargo `Running` lines for the project's own units.
    pub units_launched: u32,
    /// Units Cargo positively reported `Fresh`.
    pub units_fresh: u32,
    /// The observed driver's release (`0.1.94`), or `None` when no clippy unit was launched.
    #[serde(default)]
    pub driver_release: Option<String>,
    /// The observed driver's commit, or `None` likewise.
    #[serde(default)]
    pub driver_commit: Option<String>,
}

/// A check that can launch a compiler (`build`, `test`): its outcome and its own units.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnitCheck {
    /// `passed`.
    pub outcome: String,
    /// Cargo `Running` lines for the project's own units.
    pub units_launched: u32,
    /// Units Cargo positively reported `Fresh`.
    pub units_fresh: u32,
}

/// The five checks, in the order they run (C-5 step 5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checks {
    /// `cargo fmt --check`.
    pub fmt: Outcome,
    /// `cargo clippy --all-targets -vv -- -D warnings`.
    pub clippy: ClippyCheck,
    /// `cargo build -vv`.
    pub build: UnitCheck,
    /// `cargo test -vv`.
    pub test: UnitCheck,
    /// The smoke run.
    pub run: Outcome,
}

/// `[verified_with]`: what the five checks observed and what the seal queried, kept apart
/// (brief §5.2). Three identities are labelled and never conflated:
///
/// - `rustc_*` — **observed**: the launched compiler's answer to `-vV`; absent when the
///   observation is `cached`, the launched units' identity only when `mixed`;
/// - `resolved_rustc_*` — **resolved**: FR-012-7b's `rustc -vV` in the verified directory under
///   the seal; always present; a resolution, never an observation, never copied into `rustc_*`;
///   not Cargo's effective compiler under `RUSTC`, `build.rustc`, or a wrapper (A-8);
/// - `configured_rustc_*` — **configured**: optional and labelled; Cargo's configured
///   resolution queried separately; never an observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedWith {
    /// The operation whose five checks this table describes.
    pub operation: Operation,
    /// RFC 3339, UTC: the instant the five checks passed.
    pub verified_at: String,
    /// The scope rule `tree_digest` was computed under (FR-012-5d).
    pub tree_scope: u32,
    /// `sha256:<hex>` over every in-scope file at that verification.
    pub tree_digest: String,
    /// Over the build and test units of the project's own package(s).
    pub observation: Observation,
    /// Observed: the launched `rustc`'s `release:`; absent when cached.
    #[serde(default)]
    pub rustc_release: Option<String>,
    /// Observed: its `commit-hash:`; absent when cached.
    #[serde(default)]
    pub rustc_commit: Option<String>,
    /// Observed: its `host:`; absent when cached.
    #[serde(default)]
    pub rustc_host: Option<String>,
    /// Resolved: the preflight resolution's `release:`; always present.
    pub resolved_rustc_release: String,
    /// Resolved: its `commit-hash:`; always present.
    pub resolved_rustc_commit: String,
    /// Configured: Cargo's configured resolution, when it was queried.
    #[serde(default)]
    pub configured_rustc_release: Option<String>,
    /// Configured: its commit, when it was queried.
    #[serde(default)]
    pub configured_rustc_commit: Option<String>,
    /// Queried: `cargo -vV` under the seal.
    pub cargo_release: String,
    /// Queried: its `commit-hash:`.
    pub cargo_commit: String,
    /// The located rustup's version, or `"absent"`.
    pub rustup: String,
    /// The resolved `rustc` is a rustup proxy (FR-012-7c).
    pub proxy: bool,
    /// rustup's attribution of the selection.
    pub selected_by: SelectedBy,
    /// `RUSTC` in the sealed environment, or Cargo's `build.rustc` — presence only.
    pub rustc_override: bool,
    /// A wrapper in the sealed environment or Cargo's configuration — presence only.
    pub wrapper: bool,
    /// `RUSTFLAGS` present.
    pub rustflags: bool,
    /// `RUSTDOCFLAGS` present.
    pub rustdocflags: bool,
    /// The per-check tables.
    pub checks: Checks,
}

/// The record as read back, whatever its version.
///
/// A legacy record (no `record_version`) reads with `toolchain` and `verified_with` both `None`
/// — unknown, never filled in. A version-2 record carries `record_version = Some(2)` and both
/// tables. Every record this generator writes is version 2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// `None` for a legacy record; `Some(2)` for one this generator wrote.
    pub record_version: Option<u32>,
    /// The `renvor` that generated the project.
    pub generator_version: String,
    /// The template version the project was rendered from.
    pub template_version: String,
    /// `[toolchain]`, or `None` for a legacy record.
    pub toolchain: Option<Toolchain>,
    /// `[verified_with]`, or `None` for a legacy record.
    pub verified_with: Option<VerifiedWith>,
    /// Every file generation produced, sorted by path; the record itself is not listed.
    pub files: Vec<GeneratedFile>,
    /// Every resource a generator rendered, by name.
    pub resources: Vec<GeneratedResource>,
}

/// The lenient head: only the version is read, so a record of any version can say which it is.
#[derive(Deserialize)]
struct Head {
    #[serde(default)]
    record_version: Option<u32>,
}

/// The version-1 layout, exactly as every record before this revision was read.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Legacy {
    generator_version: String,
    template_version: String,
    #[serde(rename = "file", default)]
    files: Vec<GeneratedFile>,
    #[serde(rename = "resource", default)]
    resources: Vec<GeneratedResource>,
}

/// The version-2 layout, validated strictly within the version.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Version2 {
    #[allow(dead_code)] // Read by the head; kept so the strict layout names every key.
    record_version: u32,
    generator_version: String,
    template_version: String,
    toolchain: Toolchain,
    #[serde(default)]
    verified_with: Option<VerifiedWith>,
    #[serde(rename = "file", default)]
    files: Vec<GeneratedFile>,
    #[serde(rename = "resource", default)]
    resources: Vec<GeneratedResource>,
}

/// Writes the version-2 record for the tree under `root`.
///
/// `toolchain` is what the templates rendered; `verified_with` is what the verification measured,
/// fully built by the caller — `None` only when the operation verified nothing, which no shipped
/// operation that writes a record does. Nothing here derives a value.
///
/// # Errors
///
/// [`Code::RenderFailed`] when the tree cannot be read or the record cannot be written.
pub fn write(
    root: &Dir,
    generator_version: &str,
    template_version: &str,
    toolchain: &Toolchain,
    verified_with: Option<&VerifiedWith>,
    resources: &[GeneratedResource],
) -> Result<(), CliError> {
    let manifest = FileManifest::describe(root)?;
    let files: Vec<GeneratedFile> = manifest
        .entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File && entry.path != PATH)
        .filter_map(|entry| {
            entry.digest.as_ref().map(|digest| GeneratedFile {
                path: entry.path.clone(),
                sha256: digest.clone(),
            })
        })
        .collect();
    let record = Record {
        record_version: Some(RECORD_VERSION),
        generator_version: generator_version.to_owned(),
        template_version: template_version.to_owned(),
        toolchain: Some(toolchain.clone()),
        verified_with: verified_with.cloned(),
        files,
        resources: resources.to_vec(),
    };
    let text = render(&record);
    root.create_dir_all(DIRECTORY).map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!("the provenance directory `{DIRECTORY}` could not be created: {error}"),
        )
    })?;
    root.write(PATH, text.as_bytes()).map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!("the provenance record `{PATH}` could not be written: {error}"),
        )
    })
}

/// The record's text: hand-written and deterministic — the same [`Record`] renders to the same
/// bytes every time, which is what lets `generate resource` and `generate migration` carry
/// `[toolchain]` and `[verified_with]` through byte-identically (FR-012-5a).
///
/// A record with `record_version` renders the version-2 layout of the brief's §5.2: the version,
/// the two versions, `[toolchain]`, `[verified_with]` with its labelled field groups, the five
/// `[verified_with.checks.*]` tables, then `[[file]]` and `[[resource]]`. A legacy record renders
/// exactly as before this revision.
#[must_use]
pub fn render(record: &Record) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "# Written by `renvor new`; read by `renvor generate` to tell a file you changed from one\n\
         # it generated. Digests only. Keep it with the project."
    );
    let versioned = record.record_version.is_some();
    if versioned {
        let _ = writeln!(text, "record_version = {RECORD_VERSION}");
    }
    let _ = writeln!(text, "generator_version = {:?}", record.generator_version);
    let _ = writeln!(text, "template_version = {:?}", record.template_version);
    if versioned {
        if let Some(toolchain) = &record.toolchain {
            let _ = writeln!(
                text,
                "\n[toolchain]\npinned = {:?}\nrust_version = {:?}",
                toolchain.pinned, toolchain.rust_version
            );
        }
        if let Some(verified) = &record.verified_with {
            render_verified_with(&mut text, verified);
        }
    }
    for file in &record.files {
        let _ = writeln!(
            text,
            "\n[[file]]\npath = {:?}\nsha256 = {:?}",
            file.path, file.sha256
        );
    }
    for resource in &record.resources {
        let _ = writeln!(
            text,
            "\n[[resource]]\nname = {:?}\nfields = {:?}",
            resource.name, resource.fields
        );
    }
    text
}

/// `[verified_with]` and its five check tables, in the brief's order, with one comment line per
/// labelled identity group. An absent optional field is not written — never filled in.
fn render_verified_with(text: &mut String, verified: &VerifiedWith) {
    let _ = writeln!(text, "\n[verified_with]");
    let _ = writeln!(text, "operation = {:?}", verified.operation.as_str());
    let _ = writeln!(text, "verified_at = {:?}", verified.verified_at);
    let _ = writeln!(text, "tree_scope = {}", verified.tree_scope);
    let _ = writeln!(text, "tree_digest = {:?}", verified.tree_digest);
    let _ = writeln!(text, "observation = {:?}", verified.observation.as_str());
    let _ = writeln!(text, "# observed: absent when cached");
    optional(text, "rustc_release", verified.rustc_release.as_deref());
    optional(text, "rustc_commit", verified.rustc_commit.as_deref());
    optional(text, "rustc_host", verified.rustc_host.as_deref());
    let _ = writeln!(text, "# resolved: a resolution, never an observation");
    let _ = writeln!(
        text,
        "resolved_rustc_release = {:?}",
        verified.resolved_rustc_release
    );
    let _ = writeln!(
        text,
        "resolved_rustc_commit = {:?}",
        verified.resolved_rustc_commit
    );
    let _ = writeln!(text, "# configured: optional, labelled");
    optional(
        text,
        "configured_rustc_release",
        verified.configured_rustc_release.as_deref(),
    );
    optional(
        text,
        "configured_rustc_commit",
        verified.configured_rustc_commit.as_deref(),
    );
    let _ = writeln!(text, "cargo_release = {:?}", verified.cargo_release);
    let _ = writeln!(text, "cargo_commit = {:?}", verified.cargo_commit);
    let _ = writeln!(text, "rustup = {:?}", verified.rustup);
    let _ = writeln!(text, "proxy = {}", verified.proxy);
    let _ = writeln!(text, "selected_by = {:?}", verified.selected_by.as_str());
    let _ = writeln!(text, "rustc_override = {}", verified.rustc_override);
    let _ = writeln!(text, "wrapper = {}", verified.wrapper);
    let _ = writeln!(text, "rustflags = {}", verified.rustflags);
    let _ = writeln!(text, "rustdocflags = {}", verified.rustdocflags);

    let checks = &verified.checks;
    let _ = writeln!(
        text,
        "\n[verified_with.checks.fmt]\noutcome = {:?}",
        checks.fmt.outcome
    );
    let _ = writeln!(
        text,
        "\n[verified_with.checks.clippy]\noutcome = {:?}\nunits_launched = {}\nunits_fresh = {}",
        checks.clippy.outcome, checks.clippy.units_launched, checks.clippy.units_fresh
    );
    optional(
        text,
        "driver_release",
        checks.clippy.driver_release.as_deref(),
    );
    optional(
        text,
        "driver_commit",
        checks.clippy.driver_commit.as_deref(),
    );
    for (name, unit) in [("build", &checks.build), ("test", &checks.test)] {
        let _ = writeln!(
            text,
            "\n[verified_with.checks.{name}]\noutcome = {:?}\nunits_launched = {}\nunits_fresh = {}",
            unit.outcome, unit.units_launched, unit.units_fresh
        );
    }
    let _ = writeln!(
        text,
        "\n[verified_with.checks.run]\noutcome = {:?}",
        checks.run.outcome
    );
}

fn optional(text: &mut String, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        let _ = writeln!(text, "{key} = {value:?}");
    }
}

/// The refusal for a record whose version this generator does not read.
fn unsupported(found: u32) -> CliError {
    CliError::new(
        Code::RecordUnsupported,
        format!(
            "the provenance record `{PATH}` is version {found}; this renvor reads up to version \
             {RECORD_VERSION} — rebuild the generator, not the project"
        ),
    )
    .with("record_version", found.to_string())
    .with("supported", RECORD_VERSION.to_string())
    .with("field", PATH)
}

fn unparseable(error: &dyn std::fmt::Display) -> CliError {
    CliError::new(
        Code::ManifestInvalid,
        format!("the provenance record `{PATH}` does not parse: {error}"),
    )
    .with("field", PATH)
}

/// Parses a record's text under the reader rule of FR-012-5b.
///
/// # Errors
///
/// [`Code::RecordUnsupported`] for a `record_version` this generator does not read — anything
/// but absent (legacy) or `2`; [`Code::ManifestInvalid`] when the document does not parse under
/// its version's layout.
pub fn parse(text: &str) -> Result<Record, CliError> {
    let head: Head = toml::from_str(text).map_err(|error| unparseable(&error))?;
    match head.record_version {
        None => {
            let legacy: Legacy = toml::from_str(text).map_err(|error| unparseable(&error))?;
            Ok(Record {
                record_version: None,
                generator_version: legacy.generator_version,
                template_version: legacy.template_version,
                toolchain: None,
                verified_with: None,
                files: legacy.files,
                resources: legacy.resources,
            })
        }
        Some(RECORD_VERSION) => {
            let current: Version2 = toml::from_str(text).map_err(|error| unparseable(&error))?;
            Ok(Record {
                record_version: Some(RECORD_VERSION),
                generator_version: current.generator_version,
                template_version: current.template_version,
                toolchain: Some(current.toolchain),
                verified_with: current.verified_with,
                files: current.files,
                resources: current.resources,
            })
        }
        Some(other) => Err(unsupported(other)),
    }
}

/// Reads the record under `root`, or `None` when the project carries none.
///
/// # Errors
///
/// [`Code::RecordUnsupported`] for a version this generator does not read (FR-012-5b);
/// [`Code::ManifestInvalid`] when the record exists and does not parse.
pub fn read(root: &Dir) -> Result<Option<Record>, CliError> {
    let text = match root.read_to_string(PATH) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CliError::new(
                Code::ManifestInvalid,
                format!("the provenance record `{PATH}` could not be read: {error}"),
            )
            .with("field", PATH));
        }
    };
    parse(&text).map(Some)
}

/// Fully built records for tests across the crate: a pin, a launched verification, and a cached
/// one. Builders, not measurements — nothing here observes anything.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// A pin, as `renvor new` records one.
    pub(crate) fn toolchain() -> Toolchain {
        Toolchain {
            pinned: "1.94.0".to_owned(),
            rust_version: "1.94.0".to_owned(),
        }
    }

    /// A fully observed verification: every optional field present.
    pub(crate) fn launched() -> VerifiedWith {
        VerifiedWith {
            operation: Operation::New,
            verified_at: "2026-09-07T00:00:00Z".to_owned(),
            tree_scope: 1,
            tree_digest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
                .to_owned(),
            observation: Observation::Launched,
            rustc_release: Some("1.94.0".to_owned()),
            rustc_commit: Some("4a4ef493e".to_owned()),
            rustc_host: Some("aarch64-apple-darwin".to_owned()),
            resolved_rustc_release: "1.94.0".to_owned(),
            resolved_rustc_commit: "4a4ef493e".to_owned(),
            configured_rustc_release: Some("1.94.0".to_owned()),
            configured_rustc_commit: Some("4a4ef493e".to_owned()),
            cargo_release: "1.94.0".to_owned(),
            cargo_commit: "85eff7c80".to_owned(),
            rustup: "1.29.0".to_owned(),
            proxy: true,
            selected_by: SelectedBy::ToolchainFile,
            rustc_override: false,
            wrapper: false,
            rustflags: false,
            rustdocflags: false,
            checks: Checks {
                fmt: Outcome {
                    outcome: "passed".to_owned(),
                },
                clippy: ClippyCheck {
                    outcome: "passed".to_owned(),
                    units_launched: 2,
                    units_fresh: 0,
                    driver_release: Some("0.1.94".to_owned()),
                    driver_commit: Some("4a4ef493e3".to_owned()),
                },
                build: UnitCheck {
                    outcome: "passed".to_owned(),
                    units_launched: 1,
                    units_fresh: 0,
                },
                test: UnitCheck {
                    outcome: "passed".to_owned(),
                    units_launched: 2,
                    units_fresh: 0,
                },
                run: Outcome {
                    outcome: "passed".to_owned(),
                },
            },
        }
    }

    /// A verification whose every own unit was `Fresh`: no observed identity anywhere.
    pub(crate) fn cached() -> VerifiedWith {
        let mut verified = launched();
        verified.operation = Operation::Auth;
        verified.observation = Observation::Cached;
        verified.rustc_release = None;
        verified.rustc_commit = None;
        verified.rustc_host = None;
        verified.configured_rustc_release = None;
        verified.configured_rustc_commit = None;
        verified.checks.clippy.units_launched = 0;
        verified.checks.clippy.units_fresh = 2;
        verified.checks.clippy.driver_release = None;
        verified.checks.clippy.driver_commit = None;
        verified.checks.build.units_launched = 0;
        verified.checks.build.units_fresh = 1;
        verified.checks.test.units_launched = 0;
        verified.checks.test.units_fresh = 2;
        verified
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::exit::Exit;

    fn tree(files: &[(&str, &[u8])]) -> (tempfile::TempDir, Dir) {
        let temporary = tempfile::tempdir().expect("tempdir");
        for (path, bytes) in files {
            let full = temporary.path().join(path);
            std::fs::create_dir_all(full.parent().expect("a parent")).expect("mkdir");
            std::fs::write(full, bytes).expect("write");
        }
        let dir =
            Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority()).expect("opens");
        (temporary, dir)
    }

    fn detail<'a>(error: &'a CliError, key: &str) -> Option<&'a str> {
        error
            .details
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn version_2(verified: VerifiedWith) -> Record {
        Record {
            record_version: Some(RECORD_VERSION),
            generator_version: "0.0.0".to_owned(),
            template_version: "8".to_owned(),
            toolchain: Some(toolchain()),
            verified_with: Some(verified),
            files: vec![GeneratedFile {
                path: "src/main.rs".to_owned(),
                sha256: "ab".repeat(32),
            }],
            resources: vec![GeneratedResource {
                name: "Post".to_owned(),
                fields: vec!["title:string".to_owned()],
            }],
        }
    }

    fn legacy() -> Record {
        Record {
            record_version: None,
            generator_version: "0.0.0".to_owned(),
            template_version: "7".to_owned(),
            toolchain: None,
            verified_with: None,
            files: vec![GeneratedFile {
                path: "src/main.rs".to_owned(),
                sha256: "ab".repeat(32),
            }],
            resources: Vec::new(),
        }
    }

    #[test]
    fn the_record_names_every_file_with_its_digest_and_not_itself() {
        let (_keep, dir) = tree(&[
            ("Cargo.toml", b"[package]\n"),
            ("src/main.rs", b"fn main() {}\n"),
        ]);
        write(&dir, "0.0.0", "8", &toolchain(), Some(&launched()), &[]).expect("written");
        let record = read(&dir).expect("reads").expect("present");
        assert_eq!(record.generator_version, "0.0.0");
        assert_eq!(record.template_version, "8");
        assert_eq!(record.record_version, Some(2));
        let paths: Vec<&str> = record.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            ["Cargo.toml", "src/main.rs"],
            "sorted, and never the record"
        );
        // The digests are the manifest's, so a reader agrees with `renvor new --output json`.
        let manifest = FileManifest::describe(&dir).expect("manifest");
        for file in &record.files {
            let entry = manifest
                .entries
                .iter()
                .find(|entry| entry.path == file.path)
                .expect("listed");
            assert_eq!(entry.digest.as_deref(), Some(file.sha256.as_str()));
        }
        assert!(
            manifest.entries.iter().any(|entry| entry.path == PATH),
            "the record is itself part of the tree the manifest describes"
        );
    }

    #[test]
    fn every_record_this_generator_writes_carries_record_version_2_and_both_tables() {
        let (keep, dir) = tree(&[("Cargo.toml", b"[package]\n")]);
        write(&dir, "0.0.0", "8", &toolchain(), Some(&launched()), &[]).expect("written");
        let text = std::fs::read_to_string(keep.path().join(PATH)).expect("read");
        assert!(
            text.starts_with("# Written by `renvor new`"),
            "the header is kept"
        );
        assert!(
            text.contains("\nrecord_version = 2\n"),
            "the version is written"
        );
        let record = read(&dir).expect("reads").expect("present");
        assert_eq!(record.toolchain, Some(toolchain()));
        assert_eq!(record.verified_with, Some(launched()));
    }

    #[test]
    fn the_record_carries_the_resources_a_generator_defined() {
        // FOUND BY THE CODEX REVIEW (P1). A resource module is rendered from its name and its
        // fields; a later `renvor generate auth` has to render it again with the session guards,
        // and digests cannot say what the fields were. The record therefore carries one
        // `[[resource]]` per generated resource, and reads it back.
        let resources = vec![GeneratedResource {
            name: "Post".to_owned(),
            fields: vec!["title:string".to_owned(), "published:boolean".to_owned()],
        }];
        let (_keep, dir) = tree(&[("Cargo.toml", b"[package]\n")]);
        write(
            &dir,
            "0.0.0",
            "8",
            &toolchain(),
            Some(&launched()),
            &resources,
        )
        .expect("written");
        let record = read(&dir).expect("reads").expect("present");
        assert_eq!(record.resources, resources);
        // A record without the table — every record `renvor new` writes — reads as none.
        write(&dir, "0.0.0", "8", &toolchain(), Some(&launched()), &[]).expect("written");
        assert!(
            read(&dir)
                .expect("reads")
                .expect("present")
                .resources
                .is_empty()
        );
    }

    #[test]
    fn a_tree_without_a_record_reads_as_none_and_a_broken_one_is_refused() {
        let (_keep, dir) = tree(&[("Cargo.toml", b"[package]\n")]);
        assert!(read(&dir).expect("reads").is_none());
        dir.create_dir_all(DIRECTORY).expect("mkdir");
        dir.write(PATH, b"generator_version = 1\n").expect("write");
        let error = read(&dir).expect_err("a record that does not parse is refused");
        assert_eq!(error.code, Code::ManifestInvalid);
    }

    #[test]
    fn a_legacy_record_without_a_version_is_accepted_and_reports_unknown() {
        // FR-012-5b: absent means legacy. The text is what every generator before this revision
        // wrote — the header, the two versions, the entries — and it reads with both tables
        // unknown, never filled in.
        let text = render(&legacy());
        assert!(
            !text.contains("record_version"),
            "a legacy render has no version"
        );
        assert!(!text.contains("[toolchain]"), "and no toolchain table");
        let record = parse(&text).expect("a legacy record is accepted");
        assert_eq!(record.record_version, None);
        assert_eq!(record.toolchain, None, "unknown, not invented");
        assert_eq!(record.verified_with, None, "unknown, not invented");
        assert_eq!(record, legacy());
        // Still strict within its version: a key the legacy layout does not know is refused —
        // at the top level, where the layout is the struct's (a key after the last `[[file]]`
        // would belong to that entry).
        let with_key = text.replace(
            "template_version = \"7\"\n",
            "template_version = \"7\"\nnonsense = 1\n",
        );
        assert_ne!(with_key, text, "the fixture must change");
        let error = parse(&with_key).expect_err("unknown key");
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(error.message.contains("nonsense"), "the key is named");
    }

    #[test]
    fn a_version_2_record_is_validated_strictly() {
        let text = render(&version_2(launched()));
        assert_eq!(parse(&text).expect("parses"), version_2(launched()));
        // An unknown key inside `[verified_with]` is refused, naming it.
        let inside = text.replace("\nproxy = true\n", "\nproxy = true\nsurprise = \"value\"\n");
        assert_ne!(inside, text, "the fixture must change");
        let error = parse(&inside).expect_err("an unknown key inside [verified_with]");
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(error.message.contains("surprise"), "the key is named");
        // Inside a check table, likewise.
        let in_check = text.replace(
            "[verified_with.checks.build]\n",
            "[verified_with.checks.build]\nsurprise = 1\n",
        );
        assert_ne!(in_check, text, "the fixture must change");
        assert_eq!(
            parse(&in_check)
                .expect_err("unknown key in a check table")
                .code,
            Code::ManifestInvalid
        );
        // At the top level, likewise.
        let top = text.replace(
            "template_version = \"8\"\n",
            "template_version = \"8\"\nsurprise = 1\n",
        );
        assert_ne!(top, text, "the fixture must change");
        assert_eq!(
            parse(&top).expect_err("unknown top-level key").code,
            Code::ManifestInvalid
        );
        // A version-2 record without its `[toolchain]` is not one.
        let without = text.replace(
            "\n[toolchain]\npinned = \"1.94.0\"\nrust_version = \"1.94.0\"\n",
            "\n",
        );
        assert_ne!(without, text, "the fixture must change");
        assert_eq!(
            parse(&without).expect_err("toolchain is required").code,
            Code::ManifestInvalid
        );
    }

    #[test]
    fn a_newer_record_is_refused_by_name_before_any_plan() {
        // FR-012-5b, U-1: exit 3, the reason string, and both details — read from the head, so
        // a layout this reader has never seen (a table it does not know) cannot turn the refusal
        // into a parse error.
        let text = render(&version_2(launched()))
            .replace("record_version = 2\n", "record_version = 3\n")
            .replace("\n[toolchain]\n", "\n[future]\nkey = 1\n\n[toolchain]\n");
        let error = parse(&text).expect_err("version 3 is not read");
        assert_eq!(error.code, Code::RecordUnsupported);
        assert_eq!(error.code.exit(), Exit::Validation, "exit 3");
        assert_eq!(detail(&error, "record_version"), Some("3"));
        assert_eq!(detail(&error, "supported"), Some("2"));
        assert_eq!(detail(&error, "field"), Some(PATH));
        assert_eq!(
            error.message,
            "the provenance record `.renvor/generated.toml` is version 3; this renvor reads up \
             to version 2 — rebuild the generator, not the project"
        );
        // Through `read` too, and nothing else about the tree is consulted.
        let (_keep, dir) = tree(&[("Cargo.toml", b"[package]\n")]);
        dir.create_dir_all(DIRECTORY).expect("mkdir");
        dir.write(PATH, text.as_bytes()).expect("write");
        assert_eq!(
            read(&dir).expect_err("refused").code,
            Code::RecordUnsupported
        );
        // Only ABSENT means legacy: an explicit 0 or 1 is a version this reader does not write
        // or read, and is refused the same way rather than guessed at.
        for version in ["0", "1"] {
            let explicit = render(&version_2(launched())).replace(
                "record_version = 2\n",
                &format!("record_version = {version}\n"),
            );
            let error = parse(&explicit).expect_err("an explicit version below 2 is refused");
            assert_eq!(error.code, Code::RecordUnsupported);
            assert_eq!(detail(&error, "record_version"), Some(version));
        }
    }

    #[test]
    fn rendering_the_same_record_twice_is_byte_identical() {
        // FR-012-5a rests on this: `generate resource` and `generate migration` re-render the
        // parsed record, and the tables must come out byte for byte.
        for record in [version_2(launched()), version_2(cached()), legacy()] {
            let first = render(&record);
            assert_eq!(
                first,
                render(&record),
                "the same struct renders the same bytes"
            );
            let parsed = parse(&first).expect("parses");
            assert_eq!(parsed, record, "the round trip loses nothing");
            assert_eq!(render(&parsed), first, "and renders back to the same bytes");
        }
    }

    #[test]
    fn the_layout_follows_the_contract_example_in_order() {
        let text = render(&version_2(launched()));
        let expected = [
            "# Written by `renvor new`",
            "\nrecord_version = 2\n",
            "generator_version = \"0.0.0\"\n",
            "template_version = \"8\"\n",
            "\n[toolchain]\npinned = \"1.94.0\"\nrust_version = \"1.94.0\"\n",
            "\n[verified_with]\noperation = \"new\"\nverified_at = \"2026-09-07T00:00:00Z\"\ntree_scope = 1\n",
            "\nobservation = \"launched\"\n# observed: absent when cached\nrustc_release = \"1.94.0\"\n",
            "\n# resolved: a resolution, never an observation\nresolved_rustc_release = \"1.94.0\"\n",
            "\n# configured: optional, labelled\nconfigured_rustc_release = \"1.94.0\"\n",
            "\nselected_by = \"toolchain_file\"\n",
            "\n[verified_with.checks.fmt]\noutcome = \"passed\"\n",
            "\n[verified_with.checks.clippy]\noutcome = \"passed\"\nunits_launched = 2\nunits_fresh = 0\ndriver_release = \"0.1.94\"\ndriver_commit = \"4a4ef493e3\"\n",
            "\n[verified_with.checks.build]\noutcome = \"passed\"\nunits_launched = 1\nunits_fresh = 0\n",
            "\n[verified_with.checks.test]\noutcome = \"passed\"\nunits_launched = 2\nunits_fresh = 0\n",
            "\n[verified_with.checks.run]\noutcome = \"passed\"\n",
            "\n[[file]]\n",
            "\n[[resource]]\n",
        ];
        let mut position = 0;
        for (index, needle) in expected.iter().enumerate() {
            let found = text[position..]
                .find(needle)
                .unwrap_or_else(|| panic!("a section is missing or out of order; index: {index}"));
            position += found + needle.len();
        }
        // The operation of a scratch verification is spelled `auth`.
        assert!(render(&version_2(cached())).contains("\noperation = \"auth\"\n"));
    }

    #[test]
    fn a_cached_record_renders_no_observed_identity_and_reads_back_as_none() {
        // FR-012-4: a check whose units were positively `Fresh` is recorded as cached with the
        // observed identity UNAVAILABLE. The writer writes no line; the reader fills nothing in.
        let text = render(&version_2(cached()));
        assert!(text.contains("\nobservation = \"cached\"\n"));
        // Anchored at the line start: `resolved_rustc_release =` must not satisfy a search for
        // `rustc_release =`.
        for absent in [
            "\nrustc_release =",
            "\nrustc_commit =",
            "\nrustc_host =",
            "\nconfigured_rustc_release =",
            "\nconfigured_rustc_commit =",
            "\ndriver_release =",
            "\ndriver_commit =",
        ] {
            assert!(
                !text.contains(absent),
                "an unavailable identity is not written"
            );
        }
        // The labelled resolution is still there — it is a resolution, not an observation.
        assert!(text.contains("\nresolved_rustc_release = \"1.94.0\"\n"));
        assert!(text.contains("\nunits_fresh = 2\n"));
        let record = parse(&text).expect("parses");
        let verified = record.verified_with.expect("present");
        assert_eq!(verified.observation, Observation::Cached);
        assert_eq!(verified.rustc_release, None, "never filled in");
        assert_eq!(verified.rustc_commit, None);
        assert_eq!(verified.rustc_host, None);
        assert_eq!(verified.checks.clippy.driver_release, None);
        assert_eq!(verified.checks.clippy.driver_commit, None);
        assert_eq!(verified, cached());
    }

    #[test]
    fn an_older_reader_fails_with_serdes_unknown_field_error_not_record_unsupported() {
        // FR-012-5c, documented rather than claimed away. This is the four-field struct every
        // generator built at or before `7281e4f` reads the whole file into: no head, no
        // dispatch, `deny_unknown_fields`. A version-2 record fails there on the first key it
        // meets — serde's own message — inside the generic read failure. Those binaries cannot
        // say `record_unsupported`; only a rebuilt generator can.
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        #[allow(dead_code)]
        struct BeforeThisRevision {
            generator_version: String,
            template_version: String,
            #[serde(rename = "file", default)]
            files: Vec<GeneratedFile>,
            #[serde(rename = "resource", default)]
            resources: Vec<GeneratedResource>,
        }
        let text = render(&version_2(launched()));
        let error = toml::from_str::<BeforeThisRevision>(&text)
            .err()
            .expect("the old layout refuses a version-2 record");
        let message = error.to_string();
        assert!(
            message.contains("unknown field") && message.contains("record_version"),
            "the old reader names the field it does not know"
        );
        assert!(
            !message.contains("record_unsupported"),
            "the old reader has no such rule"
        );
        // POSITIVE CONTROL: the same old layout reads a legacy record.
        assert!(toml::from_str::<BeforeThisRevision>(&render(&legacy())).is_ok());
    }
}
