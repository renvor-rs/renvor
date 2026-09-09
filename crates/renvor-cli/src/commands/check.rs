//! `renvor check` — validate a project without building it.
//!
//! FR-019. Parses `renvor.toml`, validates it, and reports **the field and the constraint** on
//! failure. "Invalid manifest" tells an operator nothing; "`project.name` must start with an ASCII
//! letter" tells them what to edit.

use std::io::Read;

use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};

use crate::exit::{CliError, Code, Exit};
use crate::generate::digest;
use crate::generate::record::{self, Record, Toolchain, VerifiedWith};
use crate::output::Reporter;
use crate::output::layout::{Report, Status};
use crate::paths::validate_project_name;
use crate::toolchain::Observation;

/// The largest `renvor.toml` this command will read.
///
/// FR-042: *"Every input, expansion, retry, and concurrent operation MUST be bounded, and the bound
/// MUST be documented."* An unbounded `read_to_string` on a path the operator names is an
/// out-of-memory waiting for somebody to point `renvor check` at a large file — deliberately or by
/// mistake, since `check` takes a directory from the command line.
///
/// A generated manifest is a few hundred bytes. 64 KiB is three orders of magnitude of headroom for
/// hand-editing and still small enough that reading it can never be the reason a machine runs out
/// of memory.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// Reads a manifest, refusing anything above [`MAX_MANIFEST_BYTES`].
///
/// The size is checked **twice**, and that is not redundancy for its own sake: `metadata` reports
/// the size at one instant, and a file can grow between that call and the read. `take` makes the
/// read itself bounded, so the bound holds even against a file being written concurrently.
fn read_bounded(path: &std::path::Path) -> Result<String, CliError> {
    let unreadable = |detail: String| {
        CliError::new(Code::ManifestInvalid, detail)
            .with("field", "renvor.toml")
            .with("constraint", "must exist, be readable, and be under 64 KiB")
    };

    let file = std::fs::File::open(path).map_err(|error| {
        unreadable(format!(
            "`{}` could not be read: {error}",
            crate::output::redact::path(path)
        ))
    })?;

    if let Ok(metadata) = file.metadata()
        && metadata.len() > MAX_MANIFEST_BYTES
    {
        return Err(CliError::new(
            Code::BoundExceeded,
            format!(
                "`{}` is {} bytes, above the {MAX_MANIFEST_BYTES}-byte limit for a renvor manifest",
                crate::output::redact::path(path),
                metadata.len()
            ),
        )
        .with("bound", "manifest_bytes")
        .with("limit", MAX_MANIFEST_BYTES.to_string()));
    }

    let mut text = String::new();
    // `+ 1` so that a file which grew past the limit between the two checks is still detected here
    // rather than silently truncated into something that might parse.
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| {
            unreadable(format!(
                "`{}` could not be read: {error}",
                crate::output::redact::path(path)
            ))
        })?;

    if text.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(CliError::new(
            Code::BoundExceeded,
            format!(
                "`{}` grew past the {MAX_MANIFEST_BYTES}-byte limit while it was being read",
                crate::output::redact::path(path)
            ),
        )
        .with("bound", "manifest_bytes")
        .with("limit", MAX_MANIFEST_BYTES.to_string()));
    }

    Ok(text)
}

/// The `[renvor]` table.
///
/// # `deny_unknown_fields`, and the trade it makes
///
/// A typo must be a **diagnosis, not a silently ignored setting** (T068). serde's default is to
/// ignore unknown keys, so `local_domian = "app.test"` would be accepted and the operator would be
/// left wondering why their setting did nothing — which is the worst kind of failure, because
/// there is no failure.
///
/// **The cost is forward compatibility, and it is real rather than theoretical.** A future renvor
/// that adds a field writes manifests this version rejects. That is accepted deliberately: a
/// generated manifest carries `renvor.template_version`, so a phase that adds a field has an
/// obvious place to signal it, and the alternative — silently ignoring everything unrecognised —
/// makes every typo permanent and invisible.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RenvorTable {
    pub(crate) generator_version: String,
    pub(crate) template_version: String,
}

/// The `[project]` table — **every key the generator writes**, not merely the validated ones.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectTable {
    pub(crate) name: String,
    pub(crate) target: String,
    /// Added in Phase 004, when the transport capability shipped and `transport` moved from a
    /// reserved input to a defaulted-and-recorded choice.
    ///
    /// # Optional, because Phase 003 projects exist and still have to validate
    ///
    /// Making this required broke **every** project the Phase 003 generator produced: a
    /// `template_version = "1"` manifest has no such key, so it failed deserialisation with
    /// `missing field \`transport\`` and no migration path. A framework that invalidates the
    /// projects it generated one phase earlier has broken its own output.
    ///
    /// `None` therefore means "written before the transport was recorded", which is a fact about
    /// the manifest rather than an error in it.
    #[serde(default)]
    pub(crate) transport: Option<String>,
    /// Added in Phase 011, when the auth starter shipped and `--auth` moved from a reserved input
    /// to an honoured choice. Optional for the reason `transport` is: a version-6 manifest has no
    /// such key, and `None` means "written before the starter was recorded".
    #[serde(default)]
    pub(crate) auth: Option<String>,
    pub(crate) local_domain: String,
    // ── EVERY KEY THE GENERATOR WRITES MUST BE DECLARED HERE ────────────────────────────
    //
    // `deny_unknown_fields` turns an undeclared key into a rejection, so this struct is no longer
    // "the fields `check` validates" — it is **the whole manifest**. Adding a field to
    // `templates/renvor.toml.j2` without adding it here makes `renvor check` reject renvor's own
    // output, which is exactly what happened when `deny_unknown_fields` was introduced and what
    // `tests/acceptance.rs::every_generated_manifest_round_trips_through_renvor_check` caught
    // within the hour.
    //
    // These five are recorded rather than validated: they describe honoured choices, and the
    // generator has already acted on them.
    pub(crate) container: bool,
    pub(crate) local_https: String,
    pub(crate) example_domain: bool,
    pub(crate) seed_data: bool,
}

/// The `[persistence]` table — present only when a database was chosen.
///
/// Added in Phase 006, when persistence shipped and `--database` moved from a reserved input to an
/// honoured choice.
///
/// # Why the whole table is optional rather than its fields
///
/// Because "no persistence" is a real and common answer, and it is recorded by the table's
/// **absence** rather than by three empty strings. A project generated without `--database` has no
/// `src/persistence.rs` and no `migrations/`, so a manifest claiming a database would describe a
/// project that was not generated — the same rule `[project]`'s comment states for every other
/// honoured choice.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistenceTable {
    pub(crate) database: String,
    pub(crate) orm: String,
    pub(crate) driver_feature: String,
}

/// The `[container]` section a project generated with `--container` carries.
///
/// # Every field is optional except the cache choice
///
/// Containers without persistence is a supported combination — the application image and its
/// network are useful on their own — so the database fields are absent rather than empty in that
/// case. `cache` is always written, because "no cache" is an answer and recording it is what makes
/// the manifest a complete description rather than a partial one.
///
/// # There is no password field, and there cannot be
///
/// `deny_unknown_fields` is what makes that enforceable rather than aspirational: a manifest
/// carrying `database_password` is REFUSED by `renvor check` rather than quietly accepted. A
/// project that grew a credential in its committed manifest fails its own validation.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContainerTable {
    #[serde(default)]
    pub(crate) database_service: Option<String>,
    #[serde(default)]
    pub(crate) database_image: Option<String>,
    #[serde(default)]
    pub(crate) database_version: Option<String>,
    #[serde(default)]
    pub(crate) database_name: Option<String>,
    #[serde(default)]
    pub(crate) database_user: Option<String>,
    #[serde(default)]
    pub(crate) database_port: Option<u16>,
    pub(crate) cache: String,
    #[serde(default)]
    pub(crate) cache_image: Option<String>,
    #[serde(default)]
    pub(crate) cache_version: Option<String>,
    #[serde(default)]
    pub(crate) cache_port: Option<u16>,
    /// Always `false` in this phase. Recorded so the manifest states the limitation rather than
    /// leaving a reader to infer it from the absence of a cache adapter.
    #[serde(default)]
    pub(crate) cache_wired_into_application: Option<bool>,
}

/// The `[framework]` table — present only on a framework-backed **starter** (Phase 011).
///
/// `source` is the kind (`path` is the only one until a crate is published) and `path` the
/// checkout. A path is not a secret; a credential-bearing URL would be, and there is no field for
/// one.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FrameworkTable {
    pub(crate) source: String,
    pub(crate) path: String,
}

/// The `[auth]` table — present only when the session starter was generated (Phase 011).
///
/// Every field is a recorded fact about what generation acted on. **There is no key field, and
/// there cannot be**: the CSRF and abuse keys the generated application needs are read from its
/// environment, and `deny_unknown_fields` refuses a manifest that grew one.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthTable {
    pub(crate) starter: String,
    pub(crate) migrations: String,
    pub(crate) session_cookie: String,
    pub(crate) mail: String,
}

/// The `[capabilities]` table — written on every version-7 manifest, optional on read.
///
/// Five booleans, one per capability Phase 010 shipped. `false` is a recorded decline, and a
/// `true` means the dependency, the configuration section, the provider, and the wiring exist.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapabilitiesTable {
    pub(crate) cache: bool,
    pub(crate) jobs: bool,
    pub(crate) mail: bool,
    pub(crate) storage: bool,
    pub(crate) observability: bool,
}

impl CapabilitiesTable {
    /// Whether anything was selected.
    pub(crate) fn any(&self) -> bool {
        self.cache || self.jobs || self.mail || self.storage || self.observability
    }

    /// The selected names, in the recorded order.
    pub(crate) fn selected(&self) -> Vec<&'static str> {
        [
            ("cache", self.cache),
            ("jobs", self.jobs),
            ("mail", self.mail),
            ("storage", self.storage),
            ("observability", self.observability),
        ]
        .into_iter()
        .filter_map(|(name, selected)| selected.then_some(name))
        .collect()
    }
}

/// A generated `renvor.toml`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    pub(crate) renvor: RenvorTable,
    pub(crate) project: ProjectTable,
    /// `None` means the project is the dependency-free skeleton, or was written before version 7.
    #[serde(default)]
    pub(crate) framework: Option<FrameworkTable>,
    /// `None` means no session starter was generated, or the manifest predates version 7.
    #[serde(default)]
    pub(crate) auth: Option<AuthTable>,
    /// `None` means the manifest predates version 7; a version-7 manifest always carries it.
    #[serde(default)]
    pub(crate) capabilities: Option<CapabilitiesTable>,
    /// `None` means the project was generated without persistence, which is not an error.
    #[serde(default)]
    pub(crate) persistence: Option<PersistenceTable>,
    /// `None` means the project was generated without container controls, which is not an error.
    ///
    /// # Backward compatibility
    ///
    /// A manifest written by template version 3 or earlier has no `[container]` section, and
    /// `#[serde(default)]` is what lets `renvor check` keep reading it. The compatibility contract
    /// is that an older manifest stays valid; it is not that an older manifest gains fields.
    #[serde(default)]
    pub(crate) container: Option<ContainerTable>,
}

/// Reads and validates a project's `renvor.toml`, for this command and for `renvor generate`,
/// which must know a project's shape before it writes into it.
///
/// # Errors
///
/// As [`run`]: [`Code::ManifestInvalid`] naming the field and the constraint.
pub(crate) fn load(path: &std::path::Path) -> Result<Manifest, CliError> {
    let manifest_path = path.join("renvor.toml");
    let text = read_bounded(&manifest_path)?;

    let manifest: Manifest = toml::from_str(&text).map_err(|error| {
        CliError::new(
            Code::ManifestInvalid,
            format!(
                "`{}` is not a valid renvor manifest: {error}",
                crate::output::redact::path(&manifest_path)
            ),
        )
        .with("field", "renvor.toml")
        .with("constraint", "must parse as a renvor manifest")
    })?;

    let invalid = |field: &str, constraint: &str| {
        CliError::new(
            Code::ManifestInvalid,
            format!("`{field}` is invalid: {constraint}"),
        )
        .with("field", field.to_owned())
        .with("constraint", constraint.to_owned())
    };

    validate_project_name(&manifest.project.name)
        .map_err(|error| invalid("project.name", &error.message))?;

    if manifest.project.target != "api" {
        return Err(invalid(
            "project.target",
            "must be `api`; no other target is generated by this version",
        ));
    }
    // Validated when PRESENT, for the same reason `target` is: a manifest naming a transport this
    // version does not ship describes a project it could not have generated. Absent is not
    // invalid — see the field's documentation.
    if let Some(transport) = manifest.project.transport.as_deref()
        && transport != "rest"
    {
        return Err(invalid(
            "project.transport",
            "must be `rest`; no other transport is generated by this version",
        ));
    }
    if manifest.project.local_domain.is_empty() {
        return Err(invalid("project.local_domain", "must not be empty"));
    }
    if manifest.renvor.template_version.is_empty() {
        return Err(invalid("renvor.template_version", "must not be empty"));
    }

    // ── PHASE 011: present-and-wrong is refused by name; absent is "written before version 7".
    if let Some(auth) = manifest.project.auth.as_deref()
        && auth != "none"
        && auth != "session"
    {
        return Err(invalid(
            "project.auth",
            "must be `none` or `session`; no other starter is generated by this version",
        ));
    }
    if let Some(framework) = &manifest.framework {
        if framework.source != "path" {
            return Err(invalid(
                "framework.source",
                "must be `path`; no crate is published, so a path is the only source this \
                 version records",
            ));
        }
        if framework.path.is_empty() {
            return Err(invalid("framework.path", "must not be empty"));
        }
    }
    if let Some(auth) = &manifest.auth {
        if auth.starter != "session" {
            return Err(invalid(
                "auth.starter",
                "must be `session`; it is the only starter that carries an `[auth]` table",
            ));
        }
        // FR-034: each value against the supported set, not merely non-empty. `renvor check`
        // reporting `migrations = "garbage"` as valid described a project this generator could
        // never have produced (found by the Codex review of Phase 011).
        let expected_set = manifest
            .persistence
            .as_ref()
            .map(|persistence| format!("renvor-auth/{}", persistence.database));
        if expected_set.as_deref() != Some(auth.migrations.as_str()) {
            return Err(invalid(
                "auth.migrations",
                &format!(
                    "must be `renvor-auth/<engine>` for the recorded `[persistence].database`{}",
                    expected_set
                        .map(|set| format!(", which is `{set}`"))
                        .unwrap_or_else(|| "; no database is recorded".to_owned())
                ),
            ));
        }
        if auth.session_cookie != "__Host-rv_session" {
            return Err(invalid(
                "auth.session_cookie",
                "must be `__Host-rv_session`, the only cookie name the session starter sets",
            ));
        }
        if auth.mail != "smtp" {
            return Err(invalid(
                "auth.mail",
                "must be `smtp`, the only transport the session starter bridges",
            ));
        }
    }
    // Consistency across tables. The manifest is the reproducibility record, and a table that
    // claims a starter the other tables could not have produced describes a project that was not
    // generated.
    let session_recorded = manifest.project.auth.as_deref() == Some("session");
    if session_recorded && manifest.auth.is_none() {
        return Err(invalid(
            "auth",
            "`project.auth = \"session\"` needs an `[auth]` table recording what was generated",
        ));
    }
    if manifest.auth.is_some() && !session_recorded {
        return Err(invalid(
            "auth",
            "an `[auth]` table is present but `project.auth` is not `session`",
        ));
    }
    let needs_framework = session_recorded
        || manifest
            .capabilities
            .as_ref()
            .is_some_and(CapabilitiesTable::any);
    if needs_framework && manifest.framework.is_none() {
        return Err(invalid(
            "framework",
            "a starter (an auth starter or a selected capability) needs a `[framework]` table; \
             without one nothing could have supplied the dependency",
        ));
    }

    Ok(manifest)
}

/// What the provenance record says, read once for the report and the JSON alike.
///
/// The freshness verdict is **computed by the reading command, never stored** (FR-012-5d): the
/// tree digest is recomputed over the working tree under the recorded `tree_scope` and compared
/// with the recorded one. The count of operations since the verification is not printed, because
/// nothing in the tree records one.
struct Provenance {
    /// The record, or `None` when the project carries none.
    record: Option<Record>,
    /// `Some(true)` when the recorded digest is not the current tree's; `None` without evidence.
    historical: Option<bool>,
    /// What `rust-toolchain.toml` says, parsed and never evaluated.
    pin_file: PinFile,
}

/// `rust-toolchain.toml` as `renvor check` reads it: only the channel, only if it parses.
#[derive(Debug, PartialEq, Eq)]
enum PinFile {
    /// No such file.
    Absent,
    /// The file's `[toolchain].channel`, a channel name under the pin grammar.
    Channel(String),
    /// Present, but its channel could not be read as one — nothing else about the file is shown.
    Unreadable,
}

/// The lenient shape of `rust-toolchain.toml`: everything but the channel is ignored.
#[derive(Deserialize)]
struct PinFileToml {
    #[serde(default)]
    toolchain: Option<PinTable>,
}

#[derive(Deserialize)]
struct PinTable {
    #[serde(default)]
    channel: Option<String>,
}

/// The pin grammar of FR-012-1, applied before anything from the file is printed: a channel is
/// ASCII letters, digits, `.`, `_`, and `-`, at most 64 of them.
fn is_channel_name(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.len() <= 64
        && candidate
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

impl Provenance {
    /// Reads the record under `path` through the reader rule of FR-012-5b, recomputes the
    /// digest, and reads the pin file.
    fn read(path: &std::path::Path) -> Result<Self, CliError> {
        let dir = Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|error| {
            CliError::new(
                Code::ManifestInvalid,
                format!(
                    "`{}` could not be opened: {error}",
                    crate::output::redact::path(path)
                ),
            )
            .with("field", record::PATH)
        })?;
        let record = record::read(&dir)?;
        let historical = match record
            .as_ref()
            .and_then(|found| found.verified_with.as_ref())
        {
            Some(verified) => {
                let current = digest::tree_under(&dir, verified.tree_scope)?;
                Some(current != verified.tree_digest)
            }
            None => None,
        };
        Ok(Self {
            record,
            historical,
            pin_file: Self::pin_file(&dir),
        })
    }

    /// `rust-toolchain.toml`, bounded like the manifest and parsed like Cargo config: never
    /// evaluated, and never shown except as its parsed channel.
    fn pin_file(dir: &Dir) -> PinFile {
        let text = match dir.metadata("rust-toolchain.toml") {
            Ok(metadata) if metadata.len() > MAX_MANIFEST_BYTES => return PinFile::Unreadable,
            Ok(_) => match dir.read_to_string("rust-toolchain.toml") {
                Ok(text) => text,
                Err(_) => return PinFile::Unreadable,
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return PinFile::Absent,
            Err(_) => return PinFile::Unreadable,
        };
        match toml::from_str::<PinFileToml>(&text) {
            Ok(PinFileToml {
                toolchain:
                    Some(PinTable {
                        channel: Some(channel),
                    }),
            }) if is_channel_name(&channel) => PinFile::Channel(channel),
            _ => PinFile::Unreadable,
        }
    }

    fn toolchain(&self) -> Option<&Toolchain> {
        self.record.as_ref()?.toolchain.as_ref()
    }

    fn verified_with(&self) -> Option<&VerifiedWith> {
        self.record.as_ref()?.verified_with.as_ref()
    }

    /// Why a table is unknown: the tree has no record, or a record written before version 2.
    fn unknown(&self) -> &'static str {
        if self.record.is_some() {
            "unknown (a record written before template version 8)"
        } else {
            "unknown (no provenance record)"
        }
    }

    /// What the pin file says beside the recorded pin: reported, never refused.
    fn pin_file_row(&self) -> String {
        match (&self.pin_file, self.toolchain()) {
            (PinFile::Absent, _) => "absent".to_owned(),
            (PinFile::Unreadable, _) => {
                "present, but its channel could not be parsed as a channel name".to_owned()
            }
            (PinFile::Channel(channel), Some(toolchain)) if *channel == toolchain.pinned => {
                format!("channel {channel} equals the recorded pin")
            }
            (PinFile::Channel(channel), Some(toolchain)) => format!(
                "the file's channel {channel} differs from the recorded pin {}",
                toolchain.pinned
            ),
            (PinFile::Channel(channel), None) => {
                format!("channel {channel}; the record declares no pin")
            }
        }
    }

    /// The freshness line of FR-012-5d.
    fn freshness_line(&self) -> Option<String> {
        let verified = self.verified_with()?;
        Some(if self.historical == Some(true) {
            format!(
                "verified_with: historical — the tree verified at {} ({}) is not the current \
                 tree; not proof of the current tree",
                verified.verified_at,
                verified.operation.as_str()
            )
        } else {
            "verified_with: current".to_owned()
        })
    }

    /// The two tables, as rows, after the manifest's own.
    fn describe(&self, mut human: Report) -> Report {
        human = human.blank().text("[toolchain]");
        human = match self.toolchain() {
            Some(toolchain) => human
                .row("pinned", toolchain.pinned.clone())
                .row("rust_version", toolchain.rust_version.clone()),
            None => human.text(self.unknown()),
        };
        human = human.row("rust-toolchain.toml", self.pin_file_row());
        human = human.blank().text("[verified_with]");
        let Some(verified) = self.verified_with() else {
            return human.text(self.unknown());
        };
        let identity = |release: Option<&str>, commit: Option<&str>| match (release, commit) {
            (Some(release), Some(commit)) => format!("{release} ({commit})"),
            _ => "unavailable".to_owned(),
        };
        let observed = match (
            verified.rustc_release.as_deref(),
            verified.rustc_commit.as_deref(),
            verified.rustc_host.as_deref(),
        ) {
            (Some(release), Some(commit), Some(host)) => format!("{release} ({commit}, {host})"),
            _ if verified.observation == Observation::Cached => "unavailable (cached)".to_owned(),
            _ => "unavailable".to_owned(),
        };
        let checks = &verified.checks;
        let units = |outcome: &str, launched: u32, fresh: u32| {
            format!("{outcome} ({launched} launched, {fresh} fresh)")
        };
        let driver = identity(
            checks.clippy.driver_release.as_deref(),
            checks.clippy.driver_commit.as_deref(),
        );
        human = human
            .row("operation", verified.operation.as_str())
            .row("verified_at", verified.verified_at.clone())
            .row("tree_scope", verified.tree_scope.to_string())
            .row("tree_digest", verified.tree_digest.clone())
            .row("observation", verified.observation.as_str())
            .row("rustc (observed)", observed)
            .row(
                "rustc (resolved before verification)",
                identity(
                    Some(&verified.resolved_rustc_release),
                    Some(&verified.resolved_rustc_commit),
                ),
            )
            .row(
                "rustc (configured)",
                identity(
                    verified.configured_rustc_release.as_deref(),
                    verified.configured_rustc_commit.as_deref(),
                ),
            )
            .row(
                "cargo",
                identity(Some(&verified.cargo_release), Some(&verified.cargo_commit)),
            )
            .row("rustup", verified.rustup.clone())
            .row("proxy", verified.proxy.to_string())
            .row("selected_by", verified.selected_by.as_str())
            .row("rustc_override", verified.rustc_override.to_string())
            .row("wrapper", verified.wrapper.to_string())
            .row("rustflags", verified.rustflags.to_string())
            .row("rustdocflags", verified.rustdocflags.to_string())
            .row("fmt", checks.fmt.outcome.clone())
            .row(
                "clippy",
                format!(
                    "{}; driver {driver}",
                    units(
                        &checks.clippy.outcome,
                        checks.clippy.units_launched,
                        checks.clippy.units_fresh
                    )
                ),
            )
            .row(
                "build",
                units(
                    &checks.build.outcome,
                    checks.build.units_launched,
                    checks.build.units_fresh,
                ),
            )
            .row(
                "test",
                units(
                    &checks.test.outcome,
                    checks.test.units_launched,
                    checks.test.units_fresh,
                ),
            );
        // THE DOCTEST ROW EXISTS ONLY WHEN THE RECORD HAS ONE (finding 4). A project with no
        // library target launched no doctest unit and carries no table, so naming the check with
        // zeroes would report a unit that was reused when none was ever scheduled. The row sits
        // where the units ran: inside `cargo test`, after it and before the smoke run.
        if let Some(doctest) = &checks.doctest {
            human = human.row(
                "doctest",
                format!(
                    "{}; rustdoc {}",
                    units(
                        &doctest.outcome,
                        doctest.units_launched,
                        doctest.units_fresh
                    ),
                    identity(
                        doctest.rustdoc_release.as_deref(),
                        doctest.rustdoc_commit.as_deref(),
                    )
                ),
            );
        }
        human = human.row("run", checks.run.outcome.clone());
        match self.freshness_line() {
            Some(line) => human.text(line),
            None => human,
        }
    }

    /// `result.toolchain`: the table, or `null`.
    fn toolchain_json(&self) -> Result<serde_json::Value, CliError> {
        self.toolchain()
            .map_or(Ok(serde_json::Value::Null), |toolchain| {
                serde_json::to_value(toolchain).map_err(unserialisable)
            })
    }

    /// `result.verified_with`: every field with `null` for an absent one, plus `historical`.
    fn verified_with_json(&self) -> Result<serde_json::Value, CliError> {
        let Some(verified) = self.verified_with() else {
            return Ok(serde_json::Value::Null);
        };
        let mut value = serde_json::to_value(verified).map_err(unserialisable)?;
        if let Some(object) = value.as_object_mut() {
            object.insert(
                "historical".to_owned(),
                serde_json::Value::Bool(self.historical == Some(true)),
            );
        }
        Ok(value)
    }
}

fn unserialisable(error: serde_json::Error) -> CliError {
    CliError::new(
        Code::Internal,
        format!("the provenance record could not be serialised: {error}"),
    )
}

/// `renvor check`.
///
/// Validates `renvor.toml`; then reads the provenance record through the reader rule of
/// FR-012-5b and prints its `[toolchain]` and `[verified_with]` tables — or *unknown* for a
/// legacy record, never a value filled in — applies the freshness rule of FR-012-5d over the
/// working tree, and reports whether `rust-toolchain.toml`'s channel still equals the recorded
/// pin (an author's edit is reported, never refused). Nothing is built and no tool runs.
///
/// # Errors
///
/// [`Code::ManifestInvalid`] naming the field and the constraint; [`Code::RecordUnsupported`]
/// for a record version or a tree scope this generator does not know — exit 3, the versions in
/// the details.
pub fn run(reporter: &Reporter, path: &std::path::Path) -> Result<Exit, CliError> {
    let manifest = load(path)?;
    let provenance = Provenance::read(path)?;
    let human = Report::new()
        .status(Status::Done, "The project manifest is valid")
        .row("Project", manifest.project.name.clone())
        .row("Target", manifest.project.target.clone())
        .row(
            "Transport",
            manifest
                .project
                .transport
                .clone()
                // Said plainly rather than shown blank: the operator should be able to tell
                // "generated before transports were recorded" from "recorded as nothing".
                .unwrap_or_else(|| "not recorded (generated before Phase 004)".to_owned()),
        )
        .row("Template version", manifest.renvor.template_version.clone())
        .row(
            "Auth starter",
            manifest
                .project
                .auth
                .clone()
                .unwrap_or_else(|| "not recorded (generated before Phase 011)".to_owned()),
        )
        .row(
            "Capabilities",
            match &manifest.capabilities {
                Some(table) if table.any() => table.selected().join(", "),
                Some(_) => "none".to_owned(),
                None => "not recorded (generated before Phase 011)".to_owned(),
            },
        )
        .row(
            "Framework",
            match &manifest.framework {
                Some(framework) => format!("{} {}", framework.source, framework.path),
                None => "none (dependency-free skeleton)".to_owned(),
            },
        );
    let human = provenance.describe(human);
    Ok(reporter.finish(
        "check",
        &human,
        serde_json::json!({
            "name": manifest.project.name,
            "target": manifest.project.target,
            "transport": manifest.project.transport,
            "templateVersion": manifest.renvor.template_version,
            "auth": manifest.project.auth,
            "capabilities": manifest.capabilities.as_ref().map(|table| table.selected()),
            "framework": manifest.framework.as_ref().map(|framework| serde_json::json!({
                "source": framework.source,
                "path": crate::output::redact::path(std::path::Path::new(&framework.path)),
            })),
            "toolchain": provenance.toolchain_json()?,
            "verified_with": provenance.verified_with_json()?,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Format;

    fn reporter() -> Reporter {
        Reporter::new(Format::Human, true)
    }

    fn write(text: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("renvor.toml"), text).expect("write");
        dir
    }

    const VALID: &str = r#"
[renvor]
generator_version = "0.0.0"
template_version = "2"

[project]
name = "commerce"
target = "api"
transport = "rest"
local_domain = "commerce.test"
container = false
local_https = "off"
example_domain = false
seed_data = false
"#;

    #[test]
    fn a_manifest_written_before_phase_004_still_validates() {
        // THE REGRESSION THIS GUARDS. Making `transport` required broke every project the Phase
        // 003 generator produced — `renvor check` rejected renvor's own earlier output with
        // "missing field `transport`" and offered no migration path.
        let phase_003 = VALID
            .replace("template_version = \"2\"", "template_version = \"1\"")
            .replace("transport = \"rest\"\n", "");
        assert!(
            !phase_003.contains("transport"),
            "the fixture must actually lack the key"
        );

        let dir = write(&phase_003);
        assert_eq!(
            run(&reporter(), dir.path()).expect("a Phase 003 manifest must still validate"),
            Exit::Success
        );

        // POSITIVE CONTROL: an unsupported transport is still refused when it IS present, so
        // optionality did not disable the validation.
        let bad = VALID.replace("transport = \"rest\"", "transport = \"grpc\"");
        assert!(run(&reporter(), write(&bad).path()).is_err());
    }

    #[test]
    fn a_valid_manifest_passes() {
        // POSITIVE CONTROL for every rejection below.
        let dir = write(VALID);
        assert_eq!(run(&reporter(), dir.path()).expect("checks"), Exit::Success);
    }

    #[test]
    fn every_rejection_names_the_field_and_the_constraint() {
        // FR-019. A failure that says only "invalid" makes the operator bisect their own file.
        let cases = [
            (VALID.replace("commerce\"", "1bad\""), "project.name"),
            (VALID.replace("\"api\"", "\"full-stack\""), "project.target"),
            (VALID.replace("commerce.test", ""), "project.local_domain"),
            (
                VALID.replace("template_version = \"2\"", "template_version = \"\""),
                "renvor.template_version",
            ),
            (
                VALID.replace("transport = \"rest\"", "transport = \"grpc\""),
                "project.transport",
            ),
        ];
        for (text, field) in cases {
            let dir = write(&text);
            let error = run(&reporter(), dir.path()).unwrap_err();
            assert_eq!(error.code, Code::ManifestInvalid, "{field}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "field" && v == field),
                "expected field {field}, got {:?}",
                error.details
            );
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "constraint" && !v.is_empty()),
                "{field} reported no constraint"
            );
        }
    }

    #[test]
    fn a_typo_in_a_key_is_a_diagnosis_rather_than_a_silently_ignored_setting() {
        // T068. serde's default ignores unknown keys, so `local_domian` would be accepted and the
        // operator would be left wondering why their setting did nothing — a failure with no
        // failure, which is the hardest kind to debug.
        let typo = VALID.replace("local_domain", "local_domian");
        let dir = write(&typo);
        let error = run(&reporter(), dir.path()).unwrap_err();
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(
            error.message.contains("local_domian"),
            "the diagnosis must name the offending key: {}",
            error.message
        );
    }

    #[test]
    fn an_extra_unrecognised_key_is_refused_in_every_table() {
        // Both tables, because `deny_unknown_fields` is per-struct and forgetting one leaves half
        // the manifest silently permissive.
        for injected in [
            ("[renvor]", "[renvor]\nnonsense = 1"),
            ("[project]", "[project]\nnonsense = 1"),
        ] {
            let dir = write(&VALID.replace(injected.0, injected.1));
            let error = run(&reporter(), dir.path()).unwrap_err();
            assert_eq!(
                error.code,
                Code::ManifestInvalid,
                "an unknown key in {} was accepted",
                injected.0
            );
        }
    }

    /// A version-7 manifest, as the Phase 011 generator writes one for a session starter.
    const VERSION_7_STARTER: &str = r#"
[renvor]
generator_version = "0.0.0"
template_version = "7"

[project]
name = "commerce"
target = "api"
transport = "rest"
auth = "session"
local_domain = "commerce.test"
container = false
local_https = "off"
example_domain = true
seed_data = true

[framework]
source = "path"
path = "/opt/renvor"

[persistence]
database = "postgres"
orm = "sqlx"
driver_feature = "db-postgres"

[auth]
starter = "session"
migrations = "renvor-auth/postgres"
session_cookie = "__Host-rv_session"
mail = "smtp"

[capabilities]
cache = false
jobs = false
mail = true
storage = false
observability = false
"#;

    #[test]
    fn a_version_7_starter_manifest_validates() {
        // RED before Phase 011's tables existed: `deny_unknown_fields` refused `auth`,
        // `[framework]`, `[auth]`, and `[capabilities]` — which is exactly what it is for, and
        // exactly why every key the generator writes must be declared here.
        let dir = write(VERSION_7_STARTER);
        assert_eq!(
            run(&reporter(), dir.path()).expect("validates"),
            Exit::Success
        );
    }

    #[test]
    fn a_version_6_manifest_without_the_phase_011_keys_still_validates() {
        // The compatibility rule, one version on: absent keys are "written before version 7",
        // never an error. The real fixture is exercised by `tests/legacy_compatibility.rs`; this
        // is the unit-level statement.
        let dir = write(VALID);
        assert_eq!(
            run(&reporter(), dir.path()).expect("validates"),
            Exit::Success
        );
    }

    #[test]
    fn every_phase_011_value_is_validated_when_present() {
        // Present-and-wrong is refused by name; absent is not (see above). Each case names the
        // field, so a manifest edited by hand fails on the key that was edited.
        let cases = [
            (
                VERSION_7_STARTER.replace("auth = \"session\"", "auth = \"api\""),
                "project.auth",
            ),
            (
                VERSION_7_STARTER.replace("source = \"path\"", "source = \"git\""),
                "framework.source",
            ),
            (
                VERSION_7_STARTER.replace("path = \"/opt/renvor\"", "path = \"\""),
                "framework.path",
            ),
            (
                VERSION_7_STARTER.replace("starter = \"session\"", "starter = \"full\""),
                "auth.starter",
            ),
            (
                VERSION_7_STARTER
                    .replace("migrations = \"renvor-auth/postgres\"", "migrations = \"\""),
                "auth.migrations",
            ),
        ];
        for (text, field) in cases {
            let dir = write(&text);
            let error = run(&reporter(), dir.path()).unwrap_err();
            assert_eq!(error.code, Code::ManifestInvalid, "{field}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "field" && v == field),
                "expected field {field}, got {:?}",
                error.details
            );
        }
    }

    #[test]
    fn the_phase_011_tables_refuse_unknown_keys_too() {
        // `deny_unknown_fields` is per struct; a new table that forgot it would be the one place
        // a `database_password` could hide.
        for (table, injected) in [
            ("[framework]", "[framework]\nnonsense = 1"),
            ("[auth]", "[auth]\nnonsense = 1"),
            ("[capabilities]", "[capabilities]\nnonsense = 1"),
        ] {
            let dir = write(&VERSION_7_STARTER.replace(table, injected));
            let error = run(&reporter(), dir.path()).unwrap_err();
            assert_eq!(
                error.code,
                Code::ManifestInvalid,
                "an unknown key in {table} was accepted"
            );
        }
    }

    #[test]
    fn the_auth_table_holds_the_values_the_generator_writes_and_nothing_else() {
        // FOUND BY THE CODEX REVIEW (P2). `migrations = "garbage"`, an arbitrary cookie name,
        // or `mail = "anything"` passed, because the checks asked only for non-empty strings;
        // `renvor check` reported as valid manifests this generator could never have produced.
        // FR-034: every present value is validated against the supported set.
        for (from, to, field) in [
            (
                "migrations = \"renvor-auth/postgres\"",
                "migrations = \"garbage\"",
                "auth.migrations",
            ),
            (
                // The set must be the one for the project's engine, not merely a valid name.
                "migrations = \"renvor-auth/postgres\"",
                "migrations = \"renvor-auth/mysql\"",
                "auth.migrations",
            ),
            (
                "session_cookie = \"__Host-rv_session\"",
                "session_cookie = \"sid\"",
                "auth.session_cookie",
            ),
            ("mail = \"smtp\"", "mail = \"anything\"", "auth.mail"),
        ] {
            let manifest = VERSION_7_STARTER.replace(from, to);
            assert_ne!(manifest, VERSION_7_STARTER, "the fixture must change: {to}");
            let error = match run(&reporter(), write(&manifest).path()) {
                Ok(_) => panic!("`{to}` was accepted"),
                Err(error) => error,
            };
            assert_eq!(error.code, Code::ManifestInvalid, "{to}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "field" && v == field),
                "`{to}` must be refused naming `{field}`: {:?}",
                error.details
            );
        }
    }

    #[test]
    fn a_session_starter_manifest_must_carry_its_auth_table_and_a_framework() {
        // The manifest is the reproducibility record, and `auth = "session"` with no `[auth]`
        // table — or with no `[framework]` — describes a project that could not have been
        // generated. Present-and-inconsistent is refused, naming the table.
        let without_auth_table = VERSION_7_STARTER
            .split("[auth]")
            .next()
            .expect("prefix")
            .to_owned()
            + "[capabilities]\ncache = false\njobs = false\nmail = true\nstorage = false\nobservability = false\n";
        let error = run(&reporter(), write(&without_auth_table).path()).unwrap_err();
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "field" && v == "auth"),
            "{:?}",
            error.details
        );

        let without_framework = VERSION_7_STARTER.replace(
            "[framework]\nsource = \"path\"\npath = \"/opt/renvor\"\n",
            "",
        );
        assert!(!without_framework.contains("[framework]"));
        let error = run(&reporter(), write(&without_framework).path()).unwrap_err();
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "field" && v == "framework"),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn a_manifest_above_the_documented_bound_is_refused_rather_than_read() {
        // FR-042. `check` takes a directory from the command line, so an unbounded read is an
        // out-of-memory anybody can trigger by pointing it at a large file.
        let dir = tempfile::tempdir().expect("tempdir");
        let oversized = "x".repeat((MAX_MANIFEST_BYTES + 1024) as usize);
        std::fs::write(dir.path().join("renvor.toml"), oversized).expect("write");
        let error = run(&reporter(), dir.path()).unwrap_err();
        assert_eq!(error.code, Code::BoundExceeded);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "bound" && v == "manifest_bytes")
        );
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "limit" && v == "65536")
        );
    }

    #[test]
    fn an_ordinary_manifest_is_comfortably_inside_the_bound() {
        // POSITIVE CONTROL, and a check that the bound is not so tight it rejects real files. A
        // generated manifest is a few hundred bytes against a 64 KiB ceiling.
        assert!(
            (VALID.len() as u64) < MAX_MANIFEST_BYTES / 100,
            "a generated manifest is {} bytes, which is not comfortably under the {MAX_MANIFEST_BYTES}-byte bound",
            VALID.len()
        );
    }

    #[test]
    fn a_missing_manifest_is_reported_as_such_rather_than_as_a_parse_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = run(&reporter(), dir.path()).unwrap_err();
        assert_eq!(error.code, Code::ManifestInvalid);
        assert!(
            error.message.contains("could not be read"),
            "{}",
            error.message
        );
    }

    #[test]
    fn check_never_builds_anything() {
        // FR-019 says "without building it". Asserted by the absence of any build artifact after
        // a successful check — a check that shelled out to cargo would leave `target/`.
        let dir = write(VALID);
        run(&reporter(), dir.path()).expect("checks");
        assert!(
            !dir.path().join("target").exists(),
            "check produced build output"
        );
    }

    // ── PHASE 012: the record's two tables, the freshness verdict, and the pin file ─────

    use crate::generate::record::fixtures;

    fn open(dir: &tempfile::TempDir) -> Dir {
        Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).expect("opens")
    }

    /// A skeleton tree with a valid manifest, one source file, and one test.
    fn project() -> tempfile::TempDir {
        let dir = write(VALID);
        std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        std::fs::create_dir_all(dir.path().join("tests")).expect("mkdir");
        std::fs::write(
            dir.path().join("src/main.rs"),
            "// renvor:resources:begin\n// renvor:resources:end\nfn main() {}\n",
        )
        .expect("write");
        std::fs::write(
            dir.path().join("tests/smoke.rs"),
            "#[test]\nfn smoke() {}\n",
        )
        .expect("write");
        dir
    }

    /// The `[verified_with]` section of the human report as text, so an assertion can say what an
    /// operator would read rather than what the JSON holds.
    fn rendered_report(provenance: &Provenance) -> String {
        provenance
            .describe(Report::new())
            .render(crate::output::style::Permission::denied(), Some(120))
    }

    /// A record at the version this generator writes, whose digest is the tree's own, so the
    /// evidence reads as current.
    fn write_current_record(dir: &tempfile::TempDir, verified: &mut record::VerifiedWith) {
        let root = open(dir);
        verified.tree_digest = crate::generate::digest::tree(&root).expect("digests");
        record::write(
            &root,
            "0.0.0",
            "8",
            &fixtures::toolchain(),
            Some(verified),
            &[],
        )
        .expect("record");
    }

    fn detail<'a>(error: &'a CliError, key: &str) -> Option<&'a str> {
        error
            .details
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn a_tree_without_a_record_and_a_legacy_record_both_report_unknown_tables() {
        // FR-012-5b: absent means legacy, and unknown is never filled in.
        let dir = project();
        assert_eq!(run(&reporter(), dir.path()).expect("checks"), Exit::Success);
        let provenance = Provenance::read(dir.path()).expect("reads");
        assert!(provenance.record.is_none());
        assert_eq!(provenance.historical, None);
        assert_eq!(provenance.unknown(), "unknown (no provenance record)");
        assert_eq!(
            provenance.toolchain_json().expect("json"),
            serde_json::Value::Null
        );
        assert_eq!(
            provenance.verified_with_json().expect("json"),
            serde_json::Value::Null
        );
        assert_eq!(provenance.freshness_line(), None, "no evidence, no verdict");

        let legacy = record::render(&Record {
            record_version: None,
            generator_version: "0.0.0".to_owned(),
            template_version: "7".to_owned(),
            toolchain: None,
            verified_with: None,
            files: Vec::new(),
            resources: Vec::new(),
        });
        std::fs::create_dir_all(dir.path().join(record::DIRECTORY)).expect("mkdir");
        std::fs::write(dir.path().join(record::PATH), legacy).expect("write");
        assert_eq!(run(&reporter(), dir.path()).expect("checks"), Exit::Success);
        let provenance = Provenance::read(dir.path()).expect("reads");
        assert!(provenance.record.is_some());
        assert_eq!(provenance.toolchain(), None, "unknown, not invented");
        assert_eq!(provenance.verified_with(), None);
        assert_eq!(
            provenance.unknown(),
            "unknown (a record written before template version 8)"
        );
        assert_eq!(
            provenance.verified_with_json().expect("json"),
            serde_json::Value::Null
        );
    }

    #[test]
    fn a_current_tree_is_current_and_any_in_scope_change_makes_it_historical() {
        // FR-012-5d, computed by the reading command from the tree's contents.
        let dir = project();
        let mut verified = fixtures::launched();
        write_current_record(&dir, &mut verified);
        assert_eq!(run(&reporter(), dir.path()).expect("checks"), Exit::Success);
        let provenance = Provenance::read(dir.path()).expect("reads");
        assert_eq!(provenance.historical, Some(false));
        assert_eq!(
            provenance.freshness_line().as_deref(),
            Some("verified_with: current")
        );
        let json = provenance.verified_with_json().expect("json");
        assert_eq!(json["historical"], false);
        assert_eq!(json["operation"], "new");
        assert_eq!(json["rustc_release"], "1.94.0");
        assert_eq!(json["checks"]["clippy"]["driver_release"], "0.1.94");
        assert_eq!(json["checks"]["fmt"]["outcome"], "passed");
        assert_eq!(
            provenance.toolchain_json().expect("json"),
            serde_json::json!({ "pinned": "1.94.0", "rust_version": "1.94.0" })
        );

        // One byte inside the managed block, record untouched → historical; exit still 0.
        std::fs::write(
            dir.path().join("src/main.rs"),
            "// renvor:resources:begin\nx\n// renvor:resources:end\nfn main() {}\n",
        )
        .expect("write");
        assert_eq!(
            run(&reporter(), dir.path()).expect("reported, never refused"),
            Exit::Success
        );
        let provenance = Provenance::read(dir.path()).expect("reads");
        assert_eq!(provenance.historical, Some(true));
        assert_eq!(
            provenance.freshness_line().as_deref(),
            Some(
                "verified_with: historical — the tree verified at 2026-09-07T00:00:00Z (new) is \
                 not the current tree; not proof of the current tree"
            )
        );
        assert_eq!(
            provenance.verified_with_json().expect("json")["historical"],
            true
        );
        // A README edit alone would not have done that: outside the scope.
        let dir = project();
        let mut verified = fixtures::launched();
        write_current_record(&dir, &mut verified);
        std::fs::write(dir.path().join("README.md"), "# edited\n").expect("write");
        assert_eq!(
            Provenance::read(dir.path()).expect("reads").historical,
            Some(false)
        );
    }

    #[test]
    fn a_cached_record_reports_the_observed_identity_as_unavailable_and_null() {
        let dir = project();
        let mut verified = fixtures::cached();
        write_current_record(&dir, &mut verified);
        let provenance = Provenance::read(dir.path()).expect("reads");
        let json = provenance.verified_with_json().expect("json");
        assert_eq!(json["observation"], "cached");
        assert_eq!(
            json["rustc_release"],
            serde_json::Value::Null,
            "null, never filled in"
        );
        assert_eq!(json["rustc_host"], serde_json::Value::Null);
        assert_eq!(json["configured_rustc_release"], serde_json::Value::Null);
        assert_eq!(
            json["checks"]["clippy"]["driver_release"],
            serde_json::Value::Null
        );
        assert_eq!(
            json["resolved_rustc_release"], "1.94.0",
            "the resolution stays labelled"
        );
        assert_eq!(run(&reporter(), dir.path()).expect("checks"), Exit::Success);
    }

    #[test]
    fn the_doctest_bucket_reaches_both_the_human_report_and_the_json() {
        // FINDING 4. The record carries the doctest units; a reader that enumerates the checks
        // and silently omits one is reporting less than the record holds. The human table names
        // the check and the observed rustdoc, and the JSON carries the object.
        let dir = project();
        let mut verified = fixtures::library_bearing();
        write_current_record(&dir, &mut verified);
        let provenance = Provenance::read(dir.path()).expect("reads");

        let json = provenance.verified_with_json().expect("json");
        assert_eq!(json["checks"]["doctest"]["units_launched"], 1);
        assert_eq!(json["checks"]["doctest"]["units_fresh"], 0);
        assert_eq!(json["checks"]["doctest"]["rustdoc_release"], "1.98.1");
        // AND IT IS NOT THE COMPILER'S: the fixture's rustdoc differs from its `rustc_*`, so a
        // reader that filled one from the other would fail here.
        assert_eq!(json["rustc_release"], "1.94.0");

        let human = rendered_report(&provenance);
        assert!(
            human.contains("doctest"),
            "the human report names the check:\n{human}"
        );
        assert!(
            human.contains("1.98.1"),
            "and the observed rustdoc identity:\n{human}"
        );

        // A BINARY-ONLY RECORD NAMES NEITHER. The row is absent, not zeroed — its absence must
        // not read as a doctest unit that was reused.
        let dir = project();
        let mut binary_only = fixtures::launched();
        write_current_record(&dir, &mut binary_only);
        let provenance = Provenance::read(dir.path()).expect("reads");
        assert!(provenance.verified_with_json().expect("json")["checks"]["doctest"].is_null());
        assert!(
            !rendered_report(&provenance).contains("doctest"),
            "a project with no library target names no doctest check"
        );
    }

    #[test]
    fn a_newer_record_is_refused_by_name_with_exit_3() {
        let dir = project();
        let mut verified = fixtures::launched();
        write_current_record(&dir, &mut verified);
        let path = dir.path().join(record::PATH);
        let text = std::fs::read_to_string(&path).expect("read");
        // 4, BECAUSE 3 IS NOW READ (finding 4): the number is "the first version this reader
        // does not know", and it moved with `RECORD_VERSION`.
        std::fs::write(
            &path,
            text.replace(
                &format!("record_version = {}\n", crate::toolchain::RECORD_VERSION),
                "record_version = 4\n",
            ),
        )
        .expect("write");
        let error = run(&reporter(), dir.path()).unwrap_err();
        assert_eq!(error.code, Code::RecordUnsupported);
        assert_eq!(error.code.exit(), Exit::Validation);
        assert_eq!(detail(&error, "record_version"), Some("4"));
        assert_eq!(detail(&error, "supported"), Some("3"));
    }

    #[test]
    fn an_unknown_tree_scope_is_record_unsupported() {
        // A scope this generator does not know cannot be recomputed, so the verdict cannot be
        // given — refused by name rather than guessed at.
        let dir = project();
        let mut verified = fixtures::launched();
        write_current_record(&dir, &mut verified);
        let path = dir.path().join(record::PATH);
        let text = std::fs::read_to_string(&path).expect("read");
        std::fs::write(&path, text.replace("tree_scope = 1\n", "tree_scope = 9\n")).expect("write");
        let error = run(&reporter(), dir.path()).unwrap_err();
        assert_eq!(error.code, Code::RecordUnsupported);
        assert_eq!(error.code.exit(), Exit::Validation);
        assert_eq!(detail(&error, "tree_scope"), Some("9"));
        assert_eq!(detail(&error, "supported"), Some("1"));
    }

    #[test]
    fn the_pin_file_is_compared_with_the_recorded_pin_and_only_its_channel_is_shown() {
        let dir = project();
        let mut verified = fixtures::launched();
        write_current_record(&dir, &mut verified);
        assert_eq!(
            Provenance::read(dir.path()).expect("reads").pin_file_row(),
            "absent"
        );

        let pin = dir.path().join("rust-toolchain.toml");
        std::fs::write(
            &pin,
            "[toolchain]\nchannel = \"1.94.0\"\ncomponents = [\"rustfmt\"]\n",
        )
        .expect("write");
        let provenance = Provenance::read(dir.path()).expect("reads");
        assert_eq!(provenance.pin_file, PinFile::Channel("1.94.0".to_owned()));
        assert_eq!(
            provenance.pin_file_row(),
            "channel 1.94.0 equals the recorded pin"
        );

        // An author's edit is reported, never refused.
        std::fs::write(&pin, "[toolchain]\nchannel = \"1.97.1\"\n").expect("write");
        assert_eq!(
            run(&reporter(), dir.path()).expect("reported"),
            Exit::Success
        );
        assert_eq!(
            Provenance::read(dir.path()).expect("reads").pin_file_row(),
            "the file's channel 1.97.1 differs from the recorded pin 1.94.0"
        );

        // Nothing but the parsed channel is ever shown: a channel outside the grammar, a file
        // without one, and a file that is not TOML all read as unreadable.
        for text in [
            "[toolchain]\nchannel = \"stable\u{1b}[31m\"\n",
            "[toolchain]\nchannel = \"a channel with spaces\"\n",
            "[toolchain]\nprofile = \"minimal\"\n",
            "not toml at all = = =\n",
        ] {
            std::fs::write(&pin, text).expect("write");
            let provenance = Provenance::read(dir.path()).expect("reads");
            assert_eq!(provenance.pin_file, PinFile::Unreadable);
            assert_eq!(
                provenance.pin_file_row(),
                "present, but its channel could not be parsed as a channel name"
            );
        }

        // A legacy tree with a pin file added by hand: the channel, and that the record pins
        // nothing.
        let dir = project();
        std::fs::write(
            dir.path().join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.94.0\"\n",
        )
        .expect("write");
        assert_eq!(
            Provenance::read(dir.path()).expect("reads").pin_file_row(),
            "channel 1.94.0; the record declares no pin"
        );
    }

    #[test]
    fn check_reads_the_record_but_still_builds_nothing() {
        let dir = project();
        let mut verified = fixtures::launched();
        write_current_record(&dir, &mut verified);
        run(&reporter(), dir.path()).expect("checks");
        assert!(
            !dir.path().join("target").exists(),
            "check produced build output"
        );
    }
}
