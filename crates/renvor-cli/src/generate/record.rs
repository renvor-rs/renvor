//! The provenance record every generated project carries: `.renvor/generated.toml`.
//!
//! Contract `template-contract.md` §"The provenance record" (Phase 011, FR-044) and §"Record
//! versions 2 and 3" (Phase 012, L-2, FR-012-4/5, and finding 4). The generator version, the template version, one
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
//! **2** and **3** are each validated strictly — `deny_unknown_fields` everywhere inside, under
//! that version's own layout. **Anything else** is refused by name, [`Code::RecordUnsupported`]
//! (exit 3) with `details.record_version` and `details.supported`, before any file is planned or
//! modified.
//!
//! # Version 3, and why version 2 could not simply grow (finding 4)
//!
//! Version 3 adds one optional table, `[verified_with.checks.doctest]`: the units `cargo test`
//! launches with `rustdoc` rather than `rustc`, with the observed rustdoc's own identity. Version
//! 2 could not carry it. Version 2 is validated with `deny_unknown_fields` **everywhere inside**,
//! and that reader is already shipped inside generated projects — so a version-2 record carrying
//! a fourth check table is REFUSED by a `renvor` somebody already has. That is a measured
//! incompatibility, not a matter of taste, and "nothing has been published yet" does not license
//! changing version 2's meaning in place: the constraint is the strict reader in the tree, not the
//! publication status. `a_version_2_record_carrying_a_doctest_table_is_refused_by_name` is the
//! test that proves it rather than asserting it.
//!
//! Version 2 therefore keeps its exact semantics and its own strict check-table layout — the
//! private `ChecksVersion2` below, named here without a link because it is private and this
//! header is public documentation — and every record a NEW verification writes is version 3. A version-2
//! record read here stays version 2 — including when it is re-rendered by an operation that
//! verified nothing, because re-labelling it would claim, in version 3's vocabulary, that no
//! doctest unit was launched during a verification that never looked.
//!
//! # The incompatibility the other way (FR-012-5c)
//!
//! Two generations of it now. A `renvor` built from source **before** this revision — anything
//! from the version-2 dispatch up to and including `4cb4709` — reads version 2 strictly and
//! refuses a version-3 record by name: `record_unsupported`, `details.supported = 2`, which is the
//! disciplined failure this dispatch exists to give. There is **no** backward compatibility to
//! claim for version 3 and none is claimed;
//! `a_reader_whose_newest_version_is_2_refuses_a_version_3_record` reconstructs that reader and
//! shows the refusal rather than leaving a reader to hope.
//!
//! A `renvor` built from source at or before `7281e4f` has no dispatch at all: it reads the whole
//! file into the old four-field struct under `#[serde(deny_unknown_fields)]`, so a version-2 record
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
use crate::toolchain::{Observation, RECORD_VERSION, RECORD_VERSION_2, SelectedBy};

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

/// The doctest units of `cargo test -vv`: their outcome, their counts, and the observed `rustdoc`
/// executable's identity — the one Cargo's `Running` line named, from its own `-vV` query.
///
/// It sits beside [`ClippyCheck`] and for the same reason: the tool that runs these units is not
/// `rustc`, so its identity is its own. `rustdoc_*` is **never** filled from `rustc_*`, from the
/// pin, or from an old record, and the two legitimately differ — `RUSTC` redirects `rustc` and
/// leaves `rustdoc` on the toolchain's own.
///
/// **Version 3 and later only.** A version-2 record carrying this table is refused by the
/// version-2 reader, which is what made the version bump necessary; see the module header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoctestCheck {
    /// `passed` — the units ran inside `cargo test`, which passed.
    pub outcome: String,
    /// Cargo `Running` lines for the project's own doctest units.
    pub units_launched: u32,
    /// Doctest units Cargo positively reported `Fresh`. `0` as Cargo behaves today: a package
    /// reported `Fresh` launches its doctest unit anyway (measured).
    pub units_fresh: u32,
    /// The observed `rustdoc`'s release, or `None` when no doctest unit was launched.
    #[serde(default)]
    pub rustdoc_release: Option<String>,
    /// The observed `rustdoc`'s commit, or `None` likewise.
    #[serde(default)]
    pub rustdoc_commit: Option<String>,
}

/// The checks, in the order they run (C-5 step 5).
///
/// `doctest` is **optional within the version**: a project with no library target launches no
/// doctest unit, so the table is absent, and its absence is not a defect. Every starter this
/// generator ships is that shape.
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
    /// The doctest units of `cargo test -vv`; absent when none was launched. Version 3 and later.
    #[serde(default)]
    pub doctest: Option<DoctestCheck>,
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
/// — unknown, never filled in. A versioned record carries the version it declares and both tables.
/// Every record a NEW verification writes is `RECORD_VERSION`; a record an operation that verifies
/// nothing carries forward keeps the version it was read at, which is why this field holds the
/// document's own number rather than the constant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// `None` for a legacy record; otherwise the version the document declares — `Some(2)` for one
    /// an earlier generator wrote and this one carried, `Some(RECORD_VERSION)` for one a fresh
    /// verification wrote here.
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

/// The layout of a record that declares a version, validated strictly within it.
///
/// Versions 2 and 3 share this shape; they differ by exactly one optional table,
/// `[verified_with.checks.doctest]`, which version 3 defines and version 2 does not.
/// [`refuse_a_checks_table_version_2_does_not_define`] enforces that difference for version 2.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Versioned {
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

/// Version 2's `[verified_with.checks]` table: exactly the five checks that version defines.
///
/// It exists so that SERDE, not a hand-written list of forbidden keys, decides what version 2
/// admits. [`Checks`] is version 3's shape and is deliberately lenient about `doctest`; parsing a
/// version-2 document through it alone would silently accept a table that version does not have.
/// A hand-written guard would work today and rot on the day someone adds a sixth check and does
/// not think of this function — this struct refuses that field too, without being edited.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChecksVersion2 {
    #[allow(dead_code)]
    fmt: Outcome,
    #[allow(dead_code)]
    clippy: ClippyCheck,
    #[allow(dead_code)]
    build: UnitCheck,
    #[allow(dead_code)]
    test: UnitCheck,
    #[allow(dead_code)]
    run: Outcome,
}

/// Lenient everywhere but the one table whose shape is version-specific.
#[derive(Deserialize)]
struct VerifiedWithChecksOnly {
    #[allow(dead_code)]
    checks: ChecksVersion2,
}

/// Lenient everywhere but `[verified_with]`, for the same reason.
#[derive(Deserialize)]
struct DocumentChecksOnly {
    #[serde(default)]
    #[allow(dead_code)]
    verified_with: Option<VerifiedWithChecksOnly>,
}

/// Refuses a version-2 document whose `[verified_with.checks]` table names a key version 2 does
/// not define — `doctest` today, and whatever a later version adds without being told to.
fn refuse_a_checks_table_version_2_does_not_define(text: &str) -> Result<(), CliError> {
    toml::from_str::<DocumentChecksOnly>(text)
        .map(|_| ())
        .map_err(|error| unparseable(&error))
}

/// Writes the record for the tree under `root`, at [`RECORD_VERSION`] — this is the fresh-
/// verification path, so it always stamps the current version rather than carrying one.
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
/// A record with `record_version` renders the versioned layout of the brief's §5.2: **the version
/// the record itself declares**, the two versions, `[toolchain]`, `[verified_with]` with its
/// labelled field groups, its `[verified_with.checks.*]` tables, then `[[file]]` and
/// `[[resource]]`. A legacy record renders exactly as before this revision.
///
/// The five CHECKS are still five commands; the check TABLES are five or six, because the doctest
/// units of `cargo test` have a table of their own and it is written only when there is one.
#[must_use]
pub fn render(record: &Record) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "# Written by `renvor new`; read by `renvor generate` to tell a file you changed from one\n\
         # it generated. Digests only. Keep it with the project."
    );
    // THE RECORD'S OWN VERSION, never this generator's constant. An operation that verifies
    // nothing carries the version it found (`apply.rs`: "a legacy record stays legacy"), and
    // writing `RECORD_VERSION` here re-labelled a carried version-2 record as version 3 — an
    // assertion its verification never made. Invisible while the constant was 2; a defect the
    // moment it was not.
    let versioned = record.record_version;
    if let Some(version) = versioned {
        let _ = writeln!(text, "record_version = {version}");
    }
    let _ = writeln!(text, "generator_version = {:?}", record.generator_version);
    let _ = writeln!(text, "template_version = {:?}", record.template_version);
    if versioned.is_some() {
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

/// `[verified_with]` and its check tables, in the brief's order, with one comment line per
/// labelled identity group. An absent optional field is not written — never filled in, and that
/// includes the whole `doctest` table: a project with no library target launched no doctest unit
/// and gets no table, rather than a table of zeroes that would read as a reused unit.
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
    // AFTER `test`, BECAUSE THAT IS WHERE THEY RAN. Absent entirely when no doctest unit was
    // launched — never a zeroed table, so its absence cannot be read as "reused".
    if let Some(doctest) = &checks.doctest {
        let _ = writeln!(
            text,
            "\n[verified_with.checks.doctest]\noutcome = {:?}\nunits_launched = {}\nunits_fresh \
             = {}",
            doctest.outcome, doctest.units_launched, doctest.units_fresh
        );
        optional(text, "rustdoc_release", doctest.rustdoc_release.as_deref());
        optional(text, "rustdoc_commit", doctest.rustdoc_commit.as_deref());
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
        // EXPLICIT PER VERSION. Each supported version is validated strictly under its own
        // layout; a version this reader does not know is refused by name before anything is
        // planned or modified, and never read leniently "because it is probably close enough".
        Some(version @ (RECORD_VERSION_2 | RECORD_VERSION)) => {
            if version == RECORD_VERSION_2 {
                refuse_a_checks_table_version_2_does_not_define(text)?;
            }
            let current: Versioned = toml::from_str(text).map_err(|error| unparseable(&error))?;
            Ok(Record {
                // THE VERSION THE DOCUMENT DECLARES, not this generator's. A version-2 record
                // read here is a version-2 record, and re-rendering it must not re-label it: in
                // version 3 an absent doctest table means none was launched, which a version-2
                // verification never established.
                record_version: Some(version),
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
                doctest: None,
                run: Outcome {
                    outcome: "passed".to_owned(),
                },
            },
        }
    }

    /// A verification of a project WITH a library target: the doctest bucket is present, and its
    /// rustdoc identity deliberately differs from `rustc_*`, so a record that confused the two
    /// would be visible in an assertion rather than merely possible.
    pub(crate) fn library_bearing() -> VerifiedWith {
        let mut verified = launched();
        verified.checks.doctest = Some(DoctestCheck {
            outcome: "passed".to_owned(),
            units_launched: 1,
            units_fresh: 0,
            rustdoc_release: Some("1.98.1".to_owned()),
            rustdoc_commit: Some("48a229cea".to_owned()),
        });
        verified
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

    /// A record at **the version this generator writes** — `RECORD_VERSION`, which is 3 since
    /// finding 4. It was called `version_2` while that constant was 2; the name is now the
    /// constant's meaning rather than a literal, so it cannot go stale again, and a test that
    /// wants a specific older version sets `record_version` explicitly after calling this.
    fn at_the_current_version(verified: VerifiedWith) -> Record {
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
        assert_eq!(record.record_version, Some(RECORD_VERSION));
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
    fn every_record_this_generator_writes_carries_its_version_and_both_tables() {
        let (keep, dir) = tree(&[("Cargo.toml", b"[package]\n")]);
        write(&dir, "0.0.0", "8", &toolchain(), Some(&launched()), &[]).expect("written");
        let text = std::fs::read_to_string(keep.path().join(PATH)).expect("read");
        assert!(
            text.starts_with("# Written by `renvor new`"),
            "the header is kept"
        );
        assert!(
            text.contains(&format!("\nrecord_version = {RECORD_VERSION}\n")),
            "the version this generator writes"
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
        let text = render(&at_the_current_version(launched()));
        assert_eq!(
            parse(&text).expect("parses"),
            at_the_current_version(launched())
        );
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
        // 4, BECAUSE 3 IS NOW READ. The number here is "the first version this reader does not
        // know", not a fixed literal, and it moved with `RECORD_VERSION`.
        let text = render(&at_the_current_version(launched()))
            .replace(
                &format!("record_version = {RECORD_VERSION}\n"),
                "record_version = 4\n",
            )
            .replace("\n[toolchain]\n", "\n[future]\nkey = 1\n\n[toolchain]\n");
        let error = parse(&text).expect_err("version 4 is not read");
        assert_eq!(error.code, Code::RecordUnsupported);
        assert_eq!(error.code.exit(), Exit::Validation, "exit 3");
        assert_eq!(detail(&error, "record_version"), Some("4"));
        assert_eq!(detail(&error, "supported"), Some("3"));
        assert_eq!(detail(&error, "field"), Some(PATH));
        assert_eq!(
            error.message,
            "the provenance record `.renvor/generated.toml` is version 4; this renvor reads up \
             to version 3 — rebuild the generator, not the project"
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
            let explicit = render(&at_the_current_version(launched())).replace(
                &format!("record_version = {RECORD_VERSION}\n"),
                &format!("record_version = {version}\n"),
            );
            let error = parse(&explicit).expect_err("an explicit version below 2 is refused");
            assert_eq!(error.code, Code::RecordUnsupported);
            assert_eq!(detail(&error, "record_version"), Some(version));
        }
    }

    // --- FINDING 4: version 3, and the compatibility it does and does not have ----------------

    /// COMPATIBILITY TEST 1, and the one that makes the version bump a measurement rather than an
    /// assumption. A version-2 record carrying `[verified_with.checks.doctest]` is REFUSED by the
    /// version-2 reader — the reader already shipped inside generated projects. Had this passed,
    /// the table could have been added to version 2 in place and no bump would have been needed.
    ///
    /// It is written by rendering a version-3 record and re-labelling only its version line, so
    /// the document is otherwise exactly what this generator emits.
    #[test]
    fn a_version_2_record_carrying_a_doctest_table_is_refused_by_name() {
        let text = render(&at_the_current_version(library_bearing()));
        assert!(
            text.contains("[verified_with.checks.doctest]"),
            "the fixture really carries the table"
        );
        let relabelled = text.replace(
            &format!("record_version = {RECORD_VERSION}\n"),
            &format!("record_version = {RECORD_VERSION_2}\n"),
        );
        assert!(
            relabelled.contains(&format!("record_version = {RECORD_VERSION_2}\n")),
            "the fixture was re-labelled"
        );
        let error = parse(&relabelled).expect_err("version 2 does not define a doctest table");
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(
            error.message.contains("doctest"),
            "the refusal names the table version 2 does not define: {}",
            error.message
        );
    }

    /// COMPATIBILITY TEST 2. A version-3 record round-trips: written, read, and every field of the
    /// doctest bucket preserved — including the identity, which must never be confused with
    /// `rustc_*`.
    #[test]
    fn a_version_3_record_round_trips_with_every_doctest_field_preserved() {
        let record = at_the_current_version(library_bearing());
        let text = render(&record);
        let read = parse(&text).expect("a version-3 record parses");
        assert_eq!(read.record_version, Some(RECORD_VERSION));
        assert_eq!(read, record, "every field survives the round trip");
        let doctest = read
            .verified_with
            .as_ref()
            .and_then(|verified| verified.checks.doctest.as_ref())
            .expect("the doctest table is present");
        assert_eq!(doctest.units_launched, 1);
        assert_eq!(doctest.units_fresh, 0);
        assert_eq!(doctest.rustdoc_release.as_deref(), Some("1.98.1"));
        // AND IT IS NOT THE COMPILER'S. The fixture's rustdoc is a different release from its
        // `rustc_*`, so a reader that filled one from the other would fail here.
        let verified = read.verified_with.as_ref().expect("verified_with");
        assert_eq!(verified.rustc_release.as_deref(), Some("1.94.0"));
        assert_ne!(
            verified.rustc_release, doctest.rustdoc_release,
            "the two identities are kept apart"
        );
        assert_eq!(render(&read), text, "and re-rendering is byte-identical");
    }

    /// COMPATIBILITY TEST 3. A version-2 record WITHOUT the table still reads under the new
    /// reader, unchanged, and stays version 2 in memory and on re-render.
    #[test]
    fn a_version_2_record_without_the_table_still_reads_unchanged() {
        let mut record = at_the_current_version(launched());
        record.record_version = Some(RECORD_VERSION_2);
        let text = render(&record);
        let read = parse(&text).expect("a version-2 record still parses");
        assert_eq!(read.record_version, Some(RECORD_VERSION_2));
        assert_eq!(read, record, "nothing about it changed");
        assert_eq!(
            read.verified_with
                .as_ref()
                .and_then(|verified| verified.checks.doctest.as_ref()),
            None,
            "no table, and none invented"
        );
    }

    /// COMPATIBILITY TEST 4. A legacy record (no `record_version`) still reads as legacy: both
    /// tables unknown, never filled in, and no doctest evidence fabricated for a verification that
    /// predates the concept entirely.
    #[test]
    fn a_legacy_record_still_reads_as_legacy_under_the_version_3_reader() {
        let text = render(&legacy());
        assert!(!text.contains("record_version"), "a legacy record has none");
        let read = parse(&text).expect("a legacy record still parses");
        assert_eq!(read.record_version, None);
        assert_eq!(read.toolchain, None, "unknown, never filled in");
        assert_eq!(read.verified_with, None, "unknown, never filled in");
        assert_eq!(read, legacy());
    }

    /// COMPATIBILITY TEST 5. `details.supported` reports 3, and the first version this reader does
    /// NOT know is refused by name through the approved unsupported-version path — exit 3, before
    /// any file is planned or modified.
    #[test]
    fn the_supported_version_is_three_and_a_version_4_record_is_refused_by_name() {
        let text = render(&at_the_current_version(launched())).replace(
            &format!("record_version = {RECORD_VERSION}\n"),
            "record_version = 4\n",
        );
        let error = parse(&text).expect_err("version 4 is not read");
        assert_eq!(error.code, Code::RecordUnsupported);
        assert_eq!(detail(&error, "record_version"), Some("4"));
        assert_eq!(
            detail(&error, "supported"),
            Some("3"),
            "the reader reports how far it reads"
        );
        assert_eq!(error.code.exit(), Exit::Validation, "exit 3");
    }

    /// COMPATIBILITY TEST 6. A binary-only project's version-3 record writes NO doctest table.
    /// Optional within the version: its absence means no doctest unit was launched, and must not
    /// be read as a defect or as a reused unit.
    #[test]
    fn a_version_3_record_for_a_binary_only_project_writes_no_doctest_table() {
        let text = render(&at_the_current_version(launched()));
        assert!(
            text.contains(&format!("record_version = {RECORD_VERSION}\n")),
            "a fresh verification writes version 3"
        );
        assert!(
            !text.contains("[verified_with.checks.doctest]"),
            "no doctest unit, no table"
        );
        assert!(
            !text.contains("rustdoc_release"),
            "and no identity for a tool that was never launched"
        );
        parse(&text).expect("and it is a valid version-3 record");
    }

    /// THE OTHER DIRECTION, RECONSTRUCTED RATHER THAN CLAIMED. There is no backward compatibility
    /// for version 3 and none is asserted. This is the dispatch every generator between the
    /// version-2 reader and `4cb4709` runs — a lenient head, then version 2's own strict layout —
    /// applied to a version-3 record, so the refusal is demonstrated rather than described.
    ///
    /// It follows `an_older_reader_fails_with_serdes_unknown_field_error_not_record_unsupported`,
    /// which does the same for the generation before that one.
    #[test]
    fn a_reader_whose_newest_version_is_2_refuses_a_version_3_record() {
        /// The dispatch as it stood before this revision: `2` is the newest version read.
        fn read_as_a_version_2_reader(text: &str) -> Result<(), CliError> {
            let head: Head = toml::from_str(text).map_err(|error| unparseable(&error))?;
            match head.record_version {
                None => Ok(()),
                Some(RECORD_VERSION_2) => {
                    toml::from_str::<Versioned>(text)
                        .map(|_| ())
                        .map_err(|error| unparseable(&error))?;
                    toml::from_str::<DocumentChecksOnly>(text)
                        .map(|_| ())
                        .map_err(|error| unparseable(&error))
                }
                Some(other) => Err(CliError::new(
                    Code::RecordUnsupported,
                    format!(
                        "the provenance record `{PATH}` is version {other}; this renvor reads up \
                         to version {RECORD_VERSION_2} — rebuild the generator, not the project"
                    ),
                )
                .with("record_version", other.to_string())
                .with("supported", RECORD_VERSION_2.to_string())),
            }
        }

        // A version-3 record: refused by NAME at the head, before any layout is tried.
        let error = read_as_a_version_2_reader(&render(&at_the_current_version(library_bearing())))
            .expect_err("a version-2 reader does not read version 3");
        assert_eq!(error.code, Code::RecordUnsupported);
        assert_eq!(detail(&error, "record_version"), Some("3"));
        assert_eq!(
            detail(&error, "supported"),
            Some("2"),
            "and it says how far it reads, which is what tells an operator to rebuild"
        );

        // POSITIVE CONTROLS: that same reader still reads what it always read.
        let mut older = at_the_current_version(launched());
        older.record_version = Some(RECORD_VERSION_2);
        read_as_a_version_2_reader(&render(&older)).expect("a version-2 record still reads");
        read_as_a_version_2_reader(&render(&legacy())).expect("a legacy record still reads");

        // AND THE TABLE ITSELF IS WHAT IT WOULD CHOKE ON, had the version not moved: the same
        // reader, given version 3's body under a version-2 label, refuses the unknown table by
        // name. This is the measurement the whole bump rests on, asserted from the other side.
        let mislabelled = render(&at_the_current_version(library_bearing())).replace(
            &format!("record_version = {RECORD_VERSION}\n"),
            &format!("record_version = {RECORD_VERSION_2}\n"),
        );
        let error = read_as_a_version_2_reader(&mislabelled)
            .expect_err("version 2 does not define a doctest table");
        assert!(
            error.message.contains("doctest"),
            "the refusal names the table: {}",
            error.message
        );
    }

    #[test]
    fn a_carried_version_2_record_is_not_silently_upgraded() {
        // FR-012-5a, and the defect the `record_version` bump would otherwise introduce. An
        // operation that verifies nothing — `generate resource`, `generate migration` — carries
        // the record's version, `[toolchain]` and `[verified_with]` through UNCHANGED
        // (`apply.rs`: "a legacy record stays legacy"). The renderer used to write the
        // generator's own `RECORD_VERSION` for any versioned record, which was invisible while
        // that constant was 2 and the only versioned value was 2.
        //
        // Re-labelling a version-2 record as version 3 would ASSERT something its verification
        // never established: in version 3 an absent `[verified_with.checks.doctest]` table means
        // no doctest unit was launched, while a version-2 verification never looked for one.
        // That is fabricating historical evidence, so the renderer writes the record's OWN
        // version.
        let mut carried = at_the_current_version(launched());
        carried.record_version = Some(2);
        let text = render(&carried);
        assert!(
            text.contains("\nrecord_version = 2\n"),
            "a carried version-2 record renders as version 2, not as this generator's version"
        );
        // AND IT STILL READS BACK AS WHAT IT IS.
        let read = parse(&text).expect("a version-2 record still parses");
        assert_eq!(read.record_version, Some(2));
        assert_eq!(
            render(&read),
            text,
            "reading and re-rendering a carried record is byte-identical"
        );
    }

    #[test]
    fn rendering_the_same_record_twice_is_byte_identical() {
        // FR-012-5a rests on this: `generate resource` and `generate migration` re-render the
        // parsed record, and the tables must come out byte for byte.
        for record in [
            at_the_current_version(launched()),
            at_the_current_version(cached()),
            legacy(),
        ] {
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
        let text = render(&at_the_current_version(launched()));
        let expected = [
            "# Written by `renvor new`",
            "\nrecord_version = 3\n",
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
        assert!(render(&at_the_current_version(cached())).contains("\noperation = \"auth\"\n"));

        // AND THE DOCTEST TABLE, WHEN THERE IS ONE, SITS AFTER `test` AND BEFORE `run` — where
        // the units ran. The record above is binary-only and correctly has none, so the order is
        // asserted on a library-bearing one.
        let with_doctest = render(&at_the_current_version(library_bearing()));
        let ordered = [
            "\n[verified_with.checks.test]\n",
            "\n[verified_with.checks.doctest]\noutcome = \"passed\"\nunits_launched = 1\nunits_fresh = 0\nrustdoc_release = \"1.98.1\"\nrustdoc_commit = \"48a229cea\"\n",
            "\n[verified_with.checks.run]\n",
        ];
        let mut at = 0;
        for (index, needle) in ordered.iter().enumerate() {
            let found = with_doctest[at..].find(needle).unwrap_or_else(|| {
                panic!("the doctest table is missing or out of order; index: {index}")
            });
            at += found + needle.len();
        }
    }

    #[test]
    fn a_cached_record_renders_no_observed_identity_and_reads_back_as_none() {
        // FR-012-4: a check whose units were positively `Fresh` is recorded as cached with the
        // observed identity UNAVAILABLE. The writer writes no line; the reader fills nothing in.
        let text = render(&at_the_current_version(cached()));
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
        let text = render(&at_the_current_version(launched()));
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
