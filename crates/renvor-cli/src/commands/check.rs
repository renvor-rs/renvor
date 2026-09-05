//! `renvor check` — validate a project without building it.
//!
//! FR-019. Parses `renvor.toml`, validates it, and reports **the field and the constraint** on
//! failure. "Invalid manifest" tells an operator nothing; "`project.name` must start with an ASCII
//! letter" tells them what to edit.

use std::io::Read;

use serde::{Deserialize, Serialize};

use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;
use crate::paths::validate_project_name;

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
struct RenvorTable {
    generator_version: String,
    template_version: String,
}

/// The `[project]` table — **every key the generator writes**, not merely the validated ones.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectTable {
    name: String,
    target: String,
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
    transport: Option<String>,
    /// Added in Phase 011, when the auth starter shipped and `--auth` moved from a reserved input
    /// to an honoured choice. Optional for the reason `transport` is: a version-6 manifest has no
    /// such key, and `None` means "written before the starter was recorded".
    #[serde(default)]
    auth: Option<String>,
    local_domain: String,
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
    container: bool,
    local_https: String,
    example_domain: bool,
    seed_data: bool,
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
struct PersistenceTable {
    database: String,
    orm: String,
    driver_feature: String,
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
struct ContainerTable {
    #[serde(default)]
    database_service: Option<String>,
    #[serde(default)]
    database_image: Option<String>,
    #[serde(default)]
    database_version: Option<String>,
    #[serde(default)]
    database_name: Option<String>,
    #[serde(default)]
    database_user: Option<String>,
    #[serde(default)]
    database_port: Option<u16>,
    cache: String,
    #[serde(default)]
    cache_image: Option<String>,
    #[serde(default)]
    cache_version: Option<String>,
    #[serde(default)]
    cache_port: Option<u16>,
    /// Always `false` in this phase. Recorded so the manifest states the limitation rather than
    /// leaving a reader to infer it from the absence of a cache adapter.
    #[serde(default)]
    cache_wired_into_application: Option<bool>,
}

/// The `[framework]` table — present only on a framework-backed **starter** (Phase 011).
///
/// `source` is the kind (`path` is the only one until a crate is published) and `path` the
/// checkout. A path is not a secret; a credential-bearing URL would be, and there is no field for
/// one.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FrameworkTable {
    source: String,
    path: String,
}

/// The `[auth]` table — present only when the session starter was generated (Phase 011).
///
/// Every field is a recorded fact about what generation acted on. **There is no key field, and
/// there cannot be**: the CSRF and abuse keys the generated application needs are read from its
/// environment, and `deny_unknown_fields` refuses a manifest that grew one.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuthTable {
    starter: String,
    migrations: String,
    session_cookie: String,
    mail: String,
}

/// The `[capabilities]` table — written on every version-7 manifest, optional on read.
///
/// Five booleans, one per capability Phase 010 shipped. `false` is a recorded decline, and a
/// `true` means the dependency, the configuration section, the provider, and the wiring exist.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CapabilitiesTable {
    cache: bool,
    jobs: bool,
    mail: bool,
    storage: bool,
    observability: bool,
}

impl CapabilitiesTable {
    /// Whether anything was selected.
    fn any(&self) -> bool {
        self.cache || self.jobs || self.mail || self.storage || self.observability
    }

    /// The selected names, in the recorded order.
    fn selected(&self) -> Vec<&'static str> {
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
struct Manifest {
    renvor: RenvorTable,
    project: ProjectTable,
    /// `None` means the project is the dependency-free skeleton, or was written before version 7.
    #[serde(default)]
    framework: Option<FrameworkTable>,
    /// `None` means no session starter was generated, or the manifest predates version 7.
    #[serde(default)]
    auth: Option<AuthTable>,
    /// `None` means the manifest predates version 7; a version-7 manifest always carries it.
    #[serde(default)]
    capabilities: Option<CapabilitiesTable>,
    /// `None` means the project was generated without persistence, which is not an error.
    #[serde(default)]
    persistence: Option<PersistenceTable>,
    /// `None` means the project was generated without container controls, which is not an error.
    ///
    /// # Backward compatibility
    ///
    /// A manifest written by template version 3 or earlier has no `[container]` section, and
    /// `#[serde(default)]` is what lets `renvor check` keep reading it. The compatibility contract
    /// is that an older manifest stays valid; it is not that an older manifest gains fields.
    #[serde(default)]
    container: Option<ContainerTable>,
}

/// Runs the command against a project directory.
///
/// # Errors
///
/// [`Code::ManifestInvalid`] with `details.field` and `details.constraint`.
pub fn run(reporter: &Reporter, path: &std::path::Path) -> Result<Exit, CliError> {
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
        if auth.migrations.is_empty() {
            return Err(invalid(
                "auth.migrations",
                "must name the migration set that was copied",
            ));
        }
        if auth.session_cookie.is_empty() || auth.mail.is_empty() {
            return Err(invalid(
                "auth.session_cookie",
                "the cookie name and the mail transport must both be recorded",
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

    let human = crate::output::layout::Report::new()
        .status(
            crate::output::layout::Status::Done,
            "The project manifest is valid",
        )
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
}
