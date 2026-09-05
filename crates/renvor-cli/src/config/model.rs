//! The resolved, validated configuration.
//!
//! # Why validation is the only constructor
//!
//! FR-007 requires validation to complete **before any filesystem write**. That could be satisfied
//! by calling a validator first and remembering to do so every time — which is an ordering
//! discipline, and ordering disciplines are how a fifth caller eventually writes first.
//!
//! Instead the fields are private and [`ProjectConfiguration::resolve`] is the only way to obtain
//! one. A value of this type **is** a validated configuration; there is no invalid state to
//! forget to check. Data-model invariant I-1.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use renvor_database::DatabaseKind;
use serde::Serialize;

use crate::config::container;
use crate::config::container::{
    CacheChoice, ContainerSettings, DEFAULT_DATABASE_USER, DatabaseVersion, Identifier,
};
use crate::exit::{CliError, Code};
use crate::paths::{Destination, validate_project_name};

/// What kind of project to generate.
///
/// Only [`Target::Api`] is honoured in this phase. The others exist so a later phase can add them
/// without changing this type's shape, and so the reserved-flag message can name a real value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// An API-only skeleton. The only value this phase generates.
    Api,
}

impl Target {
    /// Parses a `--target` value.
    ///
    /// # Errors
    ///
    /// [`Code::ReservedForLaterPhase`] for a value a later phase will support, and
    /// [`Code::UnsupportedValue`] for one no phase will.
    pub fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "api" => Ok(Self::Api),
            "full-stack" | "desktop" | "combined" => Err(reserved(
                "--target",
                value,
                "Phase 019 (full-stack architecture) and Phase 024 (desktop)",
            )),
            other => Err(CliError::new(
                Code::UnsupportedValue,
                format!("`{other}` is not a supported target; the supported value is `api`"),
            )
            .with("flag", "--target")
            .with("value", other.to_owned())
            .with("supported", "api")),
        }
    }
}

/// Which delivery transport a generated project records.
///
/// # One value, and therefore defaulted rather than prompted
///
/// Constitution v3.0.0 principle VII, clause 2: *"A choice with only one supported value MAY be
/// defaulted without prompting and MUST be recorded."* Phase 004 ships exactly one transport, so
/// `transport` is **defaulted and recorded** — the same classification `target` already holds, for
/// the same reason. Prompting for a choice with one value asks an operator to decide something
/// already decided.
///
/// It is no longer a *reserved* input: the capability has shipped, so `--transport rest` is
/// **accepted**, and any other value fails as an unsupported value rather than as a later-phase
/// reservation. Reporting "reserved for Phase 004" from inside Phase 004 would be false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    /// REST over HTTP. The only supported value.
    Rest,
}

impl Transport {
    /// The recorded spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rest => "rest",
        }
    }

    /// Parses a `--transport` value.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] for any value other than `rest`. **Not**
    /// `ReservedForLaterPhase`: the transport capability has shipped, and naming a future phase
    /// would be a false statement about when support arrives.
    pub fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "rest" => Ok(Self::Rest),
            other => Err(CliError::new(
                Code::UnsupportedValue,
                format!("`{other}` is not a supported transport; the supported value is `rest`"),
            )
            .with("flag", "--transport")
            .with("value", other.to_owned())
            .with("supported", "rest")),
        }
    }
}

/// The persistence layer a generated project is built around.
///
/// # The enum earned its keep in Phase 007
///
/// Phase 006 shipped one variant on the argument that a later phase would add an ORM rather than
/// turn a flag into a keyword. This is that phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orm {
    /// Direct SQLx. Queries are written by hand; no object mapper is generated.
    Sqlx,
    /// SeaORM. An entity and a repository are generated, and queries are built from them.
    ///
    /// SeaORM is built on SQLx, so selecting this does not remove SQLx from the graph — it changes
    /// the programming model the application is written in. Recorded here because a reader of a
    /// generated `renvor.toml` deserves that fact where the choice is stored.
    SeaOrm,
}

impl Orm {
    /// Every supported value, for prompts and for messages.
    pub const ALL: [Self; 2] = [Self::Sqlx, Self::SeaOrm];

    /// The recorded spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sqlx => "sqlx",
            Self::SeaOrm => "seaorm",
        }
    }

    /// Parses an `--orm` value.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] for any value other than `sqlx`.
    pub fn parse(value: &str) -> Result<Self, CliError> {
        // Matched against `ALL` rather than a second literal list, so a variant added above cannot
        // become unparseable by omission — which is exactly how `seaorm` would have been missed.
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| {
                let supported = Self::ALL
                    .iter()
                    .map(|candidate| candidate.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                CliError::new(
                    Code::UnsupportedValue,
                    format!(
                        "`{value}` is not a supported persistence layer; the supported values are \
                         {supported}"
                    ),
                )
                .with("flag", "--orm")
                .with("value", value.to_owned())
                .with("supported", supported)
            })
    }
}

/// Serialised through [`Orm::as_str`], never derived.
///
/// # A derive here emitted a value the CLI would refuse
///
/// `#[serde(rename_all = "kebab-case")]` turns `SeaOrm` into `"sea-orm"`. The flag accepts
/// `seaorm`, and `renvor.toml` records `seaorm`, so the JSON was naming a third spelling that
/// round-trips through neither — a consumer that read `--output json` and fed the value back to
/// `--orm` would be refused by the tool that produced it.
///
/// Phase 006 hit exactly this on the container types, where the derive produced `"my-sql97"`. The
/// remedy is the same and for the same reason: **one function decides the spelling**, and every
/// output path goes through it.
impl Serialize for Orm {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Parses a `--database` value into the runtime's own vocabulary.
///
/// # Why this returns `renvor_database::DatabaseKind` rather than a CLI enum
///
/// So there is exactly one list of database names in the workspace. Two enums that agree by
/// coincidence drift the first time one of them gains a member, and the drift would show up as a
/// generated project naming a database the runtime cannot parse.
///
/// # Errors
///
/// [`Code::UnsupportedValue`], naming both supported values, for anything else.
pub fn parse_database(value: &str) -> Result<DatabaseKind, CliError> {
    DatabaseKind::parse(value).ok_or_else(|| {
        let supported = DatabaseKind::ALL
            .iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        CliError::new(
            Code::UnsupportedValue,
            format!("`{value}` is not a supported database; the supported values are {supported}"),
        )
        .with("flag", "--database")
        .with("value", value.to_owned())
        .with("supported", supported)
    })
}

/// Whether local HTTPS was asked for.
///
/// **Neither value causes a certificate to be issued or a trust store to be touched in this
/// phase** (FR-036). `Requested` records intent so that the phase which ships a transport can act
/// on it without asking again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocalHttps {
    /// Not requested.
    Off,
    /// Requested, and recorded. Nothing is issued and no trust store is modified.
    Requested,
}

/// The authentication starter a generated project carries — constitution VII's "auth starter",
/// honoured since Phase 011 (W-023).
///
/// # Two values, and why `api` and `full` are not among them
///
/// PLAN §9.1 lists *none, API, browser session, or full*. Phase 009 shipped the cookie-session
/// flows in full and, for tokens, exactly one route — `POST /auth/token/refresh` — with issuance
/// of the first pair left to application code. A generated `api` mode would therefore be a
/// project that cannot mint its first credential, which is a mode invented to fill a list. Both
/// are refused **by name, with the reason**, and not as `reserved_for_later_phase`: no phase is
/// assigned to token issuance, and naming one would be a promise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStarter {
    /// No authentication is generated. The default.
    None,
    /// Cookie sessions: registration, login, logout, the current user, verification, and
    /// password reset, on the selected persistence row, with mail through the `mail` capability.
    Session,
}

impl AuthStarter {
    /// Every supported value, for prompts and for messages.
    pub const ALL: [Self; 2] = [Self::None, Self::Session];

    /// The recorded spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Session => "session",
        }
    }

    /// Parses an `--auth` value.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] naming both supported values — and, for `api` and `full`, the
    /// reason they are not supported.
    pub fn parse(value: &str) -> Result<Self, CliError> {
        let supported = Self::ALL
            .iter()
            .map(|candidate| candidate.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        match value {
            "none" => Ok(Self::None),
            "session" => Ok(Self::Session),
            "api" | "full" => Err(CliError::new(
                Code::UnsupportedValue,
                format!(
                    "`--auth {value}` is not supported: the framework ships no route that issues \
                     a first token pair (only `POST /auth/token/refresh`), so a generated \
                     `{value}` starter could not authenticate anyone. The supported values are \
                     {supported}"
                ),
            )
            .with("flag", "--auth")
            .with("value", value.to_owned())
            .with("supported", supported)
            .with("reason", "no_token_issuance_route")),
            other => Err(CliError::new(
                Code::UnsupportedValue,
                format!(
                    "`{other}` is not a supported authentication starter; the supported values \
                     are {supported}"
                ),
            )
            .with("flag", "--auth")
            .with("value", other.to_owned())
            .with("supported", supported)),
        }
    }
}

/// Serialised through [`AuthStarter::as_str`], for the reason [`Orm`]'s impl gives.
impl Serialize for AuthStarter {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// One of the five capabilities Phase 010 shipped — constitution VII's "capabilities", honoured
/// since Phase 011 (W-024).
///
/// Exactly five, because exactly five shipped. There is no `s3`, no broker, and no sixth name
/// waiting for a later phase: an unshipped capability is refused as an unsupported value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// `renvor-cache` with the Valkey adapter.
    Cache,
    /// `renvor-jobs` with the durable store on the selected persistence row.
    Jobs,
    /// `renvor-mail` with the SMTP adapter.
    Mail,
    /// `renvor-storage` with the filesystem adapter.
    Storage,
    /// `renvor-observability`: JSON logs, metrics, health, trace context, OTLP.
    Observability,
}

impl Capability {
    /// Every supported capability, in the recorded order.
    pub const ALL: [Self; 5] = [
        Self::Cache,
        Self::Jobs,
        Self::Mail,
        Self::Storage,
        Self::Observability,
    ];

    /// The recorded spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Jobs => "jobs",
            Self::Mail => "mail",
            Self::Storage => "storage",
            Self::Observability => "observability",
        }
    }

    /// The supported names, joined for a message.
    fn supported() -> String {
        Self::ALL
            .iter()
            .map(|candidate| candidate.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Parses one name.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] naming all five.
    pub fn parse(name: &str) -> Result<Self, CliError> {
        Self::ALL
            .into_iter()
            .find(|candidate| candidate.as_str() == name)
            .ok_or_else(|| {
                CliError::new(
                    Code::UnsupportedValue,
                    format!(
                        "`{name}` is not a supported capability; the supported values are {}",
                        Self::supported()
                    ),
                )
                .with("flag", "--capabilities")
                .with("value", name.to_owned())
                .with("supported", Self::supported())
            })
    }
}

/// The selected capabilities: a set, so `cache,jobs` and `jobs,cache` are one selection.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Capabilities(BTreeSet<Capability>);

impl Capabilities {
    /// The explicit spelling of "none selected".
    pub const NONE: &'static str = "none";

    /// Parses a `--capabilities` value: a comma-separated list of names, or `none`.
    ///
    /// Whitespace around a name is ignored. A name given twice is refused rather than deduplicated
    /// — a list that says one thing twice is a list somebody edited — and `none` beside a name is
    /// refused as a contradiction.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] for an unknown or repeated name or an empty list;
    /// [`Code::UnsupportedCombination`] for `none` with a name.
    pub fn parse(list: &str) -> Result<Self, CliError> {
        let names: Vec<&str> = list
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        if names.is_empty() {
            return Err(CliError::new(
                Code::UnsupportedValue,
                format!(
                    "`--capabilities` names nothing; give a comma-separated list of {} or `none`",
                    Capability::supported()
                ),
            )
            .with("flag", "--capabilities")
            .with("value", list.to_owned())
            .with("supported", Capability::supported()));
        }
        if names.contains(&Self::NONE) {
            if names.len() == 1 {
                return Ok(Self::default());
            }
            return Err(CliError::new(
                Code::UnsupportedCombination,
                "`--capabilities` says `none` beside a capability name; a selection cannot be \
                 both empty and non-empty",
            )
            .with("flags", "--capabilities")
            .with("value", list.to_owned()));
        }
        let mut set = BTreeSet::new();
        for name in names {
            let capability = Capability::parse(name)?;
            if !set.insert(capability) {
                return Err(CliError::new(
                    Code::UnsupportedValue,
                    format!("`--capabilities` names `{name}` twice"),
                )
                .with("flag", "--capabilities")
                .with("value", list.to_owned())
                .with("supported", Capability::supported()));
            }
        }
        Ok(Self(set))
    }

    /// Whether `capability` was selected.
    #[must_use]
    pub fn contains(&self, capability: Capability) -> bool {
        self.0.contains(&capability)
    }

    /// Whether nothing was selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The canonical `--capabilities` value: sorted names, or `none`.
    #[must_use]
    pub fn as_flag_value(&self) -> String {
        if self.0.is_empty() {
            return Self::NONE.to_owned();
        }
        self.0
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Serialised as the five booleans `renvor.toml` records, in [`Capability::ALL`]'s order, so the
/// JSON `configuration` and the manifest say the same thing the same way.
impl Serialize for Capabilities {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        let mut map = serializer.serialize_map(Some(Capability::ALL.len()))?;
        for capability in Capability::ALL {
            map.serialize_entry(capability.as_str(), &self.contains(capability))?;
        }
        map.end()
    }
}

/// The largest framework `Cargo.toml` that will be read while validating a framework path.
///
/// FR-042 of Phase 003: every read is bounded. The workspace manifest is a few kilobytes.
const MAX_FRAMEWORK_MANIFEST_BYTES: u64 = 64 * 1024;

/// Where a generated starter's Renvor crates come from.
///
/// # One variant, and why it is an enum anyway
///
/// No Renvor crate is published (Phase 013), so the only source a generated `Cargo.toml` can
/// name today is a **path** to a checkout. The enum exists so that publication adds a `Registry`
/// variant beside this one without changing the shape of anything that records the source.
///
/// # This is local tooling, not a product choice
///
/// The path says *where the framework is*; it decides nothing about what the project does. It is
/// asked for only when a selection needs the framework, recorded in `renvor.toml` beside the
/// choices it enabled, and validated **before any write** by reading two files and evaluating
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameworkSource {
    /// A checkout of the Renvor workspace, canonical and absolute.
    Path(PathBuf),
}

impl FrameworkSource {
    /// Validates `requested` as a Renvor workspace checkout.
    ///
    /// The rules, each named in `details.rule`:
    ///
    /// | Rule | Requirement |
    /// |---|---|
    /// | `framework_path_utf8` | the path is valid UTF-8 (it is written into `Cargo.toml`) |
    /// | `framework_path_control_character` | no control character anywhere in it |
    /// | `framework_directory` | it names an existing directory that resolves |
    /// | `framework_manifest` | `Cargo.toml` exists there, is readable, and is under 64 KiB |
    /// | `framework_workspace` | that manifest parses as TOML and declares `[workspace]` |
    /// | `framework_facade` | `crates/renvor/Cargo.toml` exists and names package `renvor` |
    /// | `framework_lockfile` | `Cargo.lock` exists there (the starter's resolution is seeded from it) |
    ///
    /// Two files are read. Nothing is executed or evaluated.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] with `details.flag = "--framework-path"` and `details.rule`.
    pub fn validate_path(requested: &Path) -> Result<Self, CliError> {
        let shown = crate::output::redact::path(requested);
        let refuse = |rule: &str, why: String| {
            CliError::new(
                Code::UnsupportedValue,
                format!("`--framework-path {shown}` {why}"),
            )
            .with("flag", "--framework-path")
            .with("value", shown.clone())
            .with("rule", rule.to_owned())
        };

        let Some(text) = requested.to_str() else {
            return Err(refuse(
                "framework_path_utf8",
                "is not valid UTF-8, and the path is written into the generated `Cargo.toml`"
                    .to_owned(),
            ));
        };
        if text.chars().any(char::is_control) {
            return Err(refuse(
                "framework_path_control_character",
                "contains a control character".to_owned(),
            ));
        }

        let canonical = std::fs::canonicalize(requested).map_err(|error| {
            refuse(
                "framework_directory",
                format!("does not resolve to an existing directory: {error}"),
            )
        })?;
        if !canonical.is_dir() {
            return Err(refuse(
                "framework_directory",
                "is not a directory".to_owned(),
            ));
        }
        // Canonicalisation can introduce what the raw path did not have (a symlink target with a
        // control character, or a non-UTF-8 component), so the two rules run again on the result.
        let Some(canonical_text) = canonical.to_str() else {
            return Err(refuse(
                "framework_path_utf8",
                "resolves to a path that is not valid UTF-8".to_owned(),
            ));
        };
        if canonical_text.chars().any(char::is_control) {
            return Err(refuse(
                "framework_path_control_character",
                "resolves to a path containing a control character".to_owned(),
            ));
        }

        let manifest = read_bounded(&canonical.join("Cargo.toml")).map_err(|why| {
            refuse(
                "framework_manifest",
                format!("has no readable `Cargo.toml`: {why}"),
            )
        })?;
        // A DOCUMENT parse. `str::parse::<toml::Value>` reads one VALUE — `[workspace]` parsed as
        // an array literal and then refused the rest of the file — which is the kind of wrong
        // answer a fixture-backed test is for.
        let parsed: toml::Table = toml::from_str(&manifest).map_err(|error| {
            refuse(
                "framework_workspace",
                format!("has a `Cargo.toml` that is not valid TOML: {error}"),
            )
        })?;
        if parsed.get("workspace").is_none() {
            return Err(refuse(
                "framework_workspace",
                "is a package, not the Renvor workspace: its `Cargo.toml` declares no \
                 `[workspace]`"
                    .to_owned(),
            ));
        }

        let facade = read_bounded(&canonical.join("crates").join("renvor").join("Cargo.toml"))
            .map_err(|why| {
                refuse(
                    "framework_facade",
                    format!("holds no `crates/renvor/Cargo.toml`: {why}"),
                )
            })?;
        let facade: toml::Table = toml::from_str(&facade).map_err(|error| {
            refuse(
                "framework_facade",
                format!("has a `crates/renvor/Cargo.toml` that is not valid TOML: {error}"),
            )
        })?;
        let name = facade
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str);
        if name != Some("renvor") {
            return Err(refuse(
                "framework_facade",
                "holds a `crates/renvor/Cargo.toml` that does not name the package `renvor`"
                    .to_owned(),
            ));
        }

        // FR-006: the starter's resolution is seeded from this lockfile. A checkout without one
        // cannot seed anything and its starters would need the network to resolve.
        let lockfile = canonical.join("Cargo.lock");
        if !lockfile.is_file() {
            return Err(refuse(
                "framework_lockfile",
                "holds no `Cargo.lock`; a generated starter starts from the framework's lockfile \
                 so that it resolves offline, and a checkout without one cannot supply it"
                    .to_owned(),
            ));
        }

        Ok(Self::Path(canonical))
    }

    /// The checkout.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Path(path) => path,
        }
    }

    /// The recorded source kind.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Path(_) => "path",
        }
    }
}

/// Serialised as `{ "source": "path", "path": … }`, with the path through the redacting
/// renderer every other displayed path goes through.
impl Serialize for FrameworkSource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap as _;
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("source", self.kind())?;
        map.serialize_entry("path", &crate::output::redact::path(self.path()))?;
        map.end()
    }
}

/// Reads a small file, refusing anything above [`MAX_FRAMEWORK_MANIFEST_BYTES`].
fn read_bounded(path: &Path) -> Result<String, String> {
    use std::io::Read as _;
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    if let Ok(metadata) = file.metadata()
        && metadata.len() > MAX_FRAMEWORK_MANIFEST_BYTES
    {
        return Err(format!(
            "{} bytes, above the {MAX_FRAMEWORK_MANIFEST_BYTES}-byte bound",
            metadata.len()
        ));
    }
    let mut text = String::new();
    file.take(MAX_FRAMEWORK_MANIFEST_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    if text.len() as u64 > MAX_FRAMEWORK_MANIFEST_BYTES {
        return Err(format!(
            "grew past the {MAX_FRAMEWORK_MANIFEST_BYTES}-byte bound while being read"
        ));
    }
    Ok(text)
}

/// The single input to generation.
///
/// Every field is private. Construct with [`ProjectConfiguration::resolve`].
///
/// # No field can hold a secret
///
/// Data-model invariant I-3, and it is why FR-018 needs no filter at write time: a value that
/// cannot be held cannot be serialized. If a later phase adds a credential-bearing choice, it must
/// not be added to this struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectConfiguration {
    name: String,
    #[serde(skip)]
    /// **For display and for the generated manifest only.** The authority to write there is
    /// the [`Destination`] that [`ProjectConfiguration::resolve`] returns alongside this value —
    /// a path is not a capability, and keeping the two separate is what stops a later caller from
    /// re-deriving one from the other.
    destination: PathBuf,
    local_domain: String,
    target: Target,
    transport: Transport,
    container: bool,
    local_https: LocalHttps,
    seed_data: bool,
    example_domain: bool,
    /// `None` when the project was generated without persistence.
    orm: Option<Orm>,
    /// `None` when the project was generated without persistence.
    #[serde(serialize_with = "serialize_database")]
    database: Option<DatabaseKind>,
    /// `None` when container controls were not selected.
    container_settings: Option<ContainerSettings>,
    /// The authentication starter. `None` is a recorded choice, not an absence.
    auth: AuthStarter,
    /// The selected capabilities. Empty is a recorded choice, not an absence.
    capabilities: Capabilities,
    /// Where the framework crates come from, or `None` for the dependency-free skeleton.
    ///
    /// **Not skipped**, unlike `destination`: the source is a recorded fact of the manifest and a
    /// consumer scripting `renvor new` needs it back. Its path is serialised through the
    /// redacting renderer.
    framework: Option<FrameworkSource>,
}

/// The unvalidated answers, from either interface.
///
/// This is the **only** shape the wizard and the flag parser produce. Neither builds a
/// [`ProjectConfiguration`] directly, which is what makes SC-003 a test of one code path rather
/// than of two that agree.
#[derive(Debug, Clone)]
pub struct Answers {
    /// Project name. `None` means "derive from the destination".
    pub name: Option<String>,
    /// Where to create it.
    pub destination: PathBuf,
    /// Local development domain. `None` means "derive from the name".
    pub local_domain: Option<String>,
    /// `--target` as typed, so an unsupported value can be reported verbatim.
    pub target: String,
    /// `--transport` as typed, or `None` to take the single supported value.
    pub transport: Option<String>,
    /// Generate container development controls.
    pub container: bool,
    /// Request local HTTPS. Records intent only.
    pub local_https: bool,
    /// Generate seed data.
    pub seed_data: bool,
    /// Generate the example domain module.
    pub example_domain: bool,
    /// `--orm` as typed, so an unsupported value can be reported verbatim.
    pub orm: Option<String>,
    /// `--database` as typed. `None` means "generate without persistence".
    pub database: Option<String>,
    /// `--database-version` as typed.
    pub database_version: Option<String>,
    /// `--database-name` as typed.
    pub database_name: Option<String>,
    /// `--database-user` as typed. **Never a password**; see [`ContainerSettings`].
    pub database_user: Option<String>,
    /// `--database-port` as typed, so `70000` is refused by renvor with renvor's diagnosis.
    pub database_port: Option<String>,
    /// `--container-cache` as typed.
    pub container_cache: Option<String>,
    /// `--cache-port` as typed.
    pub cache_port: Option<String>,
    /// `--auth` as typed, or `None` for the default `none`.
    pub auth: Option<String>,
    /// `--capabilities` as typed, or `None` for the default `none`.
    pub capabilities: Option<String>,
    /// `--framework-path` as typed. Validated, never written, before anything is staged.
    pub framework_path: Option<PathBuf>,
}

/// Resolves the container development controls, or refuses an incoherent combination.
///
/// # Every refusal here is a COMBINATION, not a value
///
/// The values themselves are checked by [`crate::config::container`]. What this adds is the four
/// ways a set of individually-valid answers can still not describe a project:
///
/// 1. a container database field without `--container` — there is no Compose file to put it in;
/// 2. a container database field without `--database` — there is no database service to configure;
/// 3. `--cache-port` with no cache — a port for a service that will not exist;
/// 4. `--container-cache` without `--container` — same as (1).
///
/// Each is refused rather than ignored. A flag that parses and does nothing leaves the operator
/// believing they configured something the generated tree does not reflect, and that belief
/// survives until whatever they configured turns out to matter.
fn resolve_container(
    answers: &Answers,
    container: bool,
    database: Option<DatabaseKind>,
    project_name: &str,
    wants_cache: bool,
) -> Result<Option<ContainerSettings>, CliError> {
    // Listed rather than tested one at a time, so a flag added to `Answers` and forgotten here is
    // visible as a missing row rather than invisible as a missing branch — the same construction
    // `RESERVED` uses for the reserved flags.
    let database_fields: [(&str, bool); 4] = [
        ("--database-version", answers.database_version.is_some()),
        ("--database-name", answers.database_name.is_some()),
        ("--database-user", answers.database_user.is_some()),
        ("--database-port", answers.database_port.is_some()),
    ];
    let cache_fields: [(&str, bool); 2] = [
        ("--container-cache", answers.container_cache.is_some()),
        ("--cache-port", answers.cache_port.is_some()),
    ];

    if !container {
        for (flag, present) in database_fields.iter().chain(cache_fields.iter()) {
            if *present {
                return Err(CliError::new(
                    Code::UnsupportedCombination,
                    format!(
                        "`{flag}` needs `--container`: it configures the generated `compose.yaml`, \
                         and without `--container` there is no `compose.yaml` to configure"
                    ),
                )
                .with("flags", format!("{flag}, --container")));
            }
        }
        return Ok(None);
    }

    // ── the database service ────────────────────────────────────────────────────────────
    let database_settings = match database {
        None => {
            for (flag, present) in database_fields {
                if present {
                    return Err(CliError::new(
                        Code::UnsupportedCombination,
                        format!(
                            "`{flag}` needs `--database`: it configures the database service, and \
                             a project generated without persistence has no database service"
                        ),
                    )
                    .with("flags", format!("{flag}, --database")));
                }
            }
            None
        }
        Some(kind) => {
            let version = match answers.database_version.as_deref() {
                Some(value) => DatabaseVersion::parse(kind, value)?,
                // DEFAULTED AND RECORDED, not silently chosen: the manifest writes the version out,
                // so what ran is readable without knowing what the default was on the day.
                None => DatabaseVersion::newest_for(kind),
            };
            let name = match answers.database_name.as_deref() {
                Some(value) => Identifier::database_name(value)?,
                None => Identifier::derive_database_name(project_name)?,
            };
            let user = match answers.database_user.as_deref() {
                Some(value) => Identifier::database_user(value)?,
                None => Identifier::database_user(DEFAULT_DATABASE_USER)?,
            };
            let port = match answers.database_port.as_deref() {
                Some(value) => container::parse_port(value, "--database-port")?,
                None => version.default_host_port(),
            };
            Some((version, name, user, port))
        }
    };

    // ── the cache service ───────────────────────────────────────────────────────────────
    let cache = match answers.container_cache.as_deref() {
        Some(value) => {
            let choice = CacheChoice::parse(value)?;
            // A `cache` capability with `--container-cache none` is a contradiction, not an
            // override: the application would read a service the Compose file does not run.
            if wants_cache && choice.engine().is_none() {
                return Err(CliError::new(
                    Code::UnsupportedCombination,
                    "`--container-cache none` contradicts the `cache` capability: the generated \
                     application reads a cache, and `--container` was asked to run its local \
                     services",
                )
                .with("flags", "--container-cache, --capabilities"));
            }
            choice
        }
        // The cache service follows the `cache` capability the way the database service follows
        // `--database`: generated when the application will read it, opt-in otherwise. Recorded
        // either way, so a reader of `renvor.toml` sees what was decided.
        None if wants_cache => CacheChoice::Engine(container::CacheEngine::Valkey),
        None => CacheChoice::None,
    };
    let cache_port = match (cache.engine(), answers.cache_port.as_deref()) {
        (Some(_), Some(value)) => Some(container::parse_port(value, "--cache-port")?),
        (Some(engine), None) => Some(engine.default_host_port()),
        (None, Some(_)) => {
            return Err(CliError::new(
                Code::UnsupportedCombination,
                "`--cache-port` needs `--container-cache`: a port for a service that will not be \
                 generated configures nothing",
            )
            .with("flags", "--cache-port, --container-cache"));
        }
        (None, None) => None,
    };

    // ── one port, one service ───────────────────────────────────────────────────────────
    //
    // Refused rather than shifted to the next free port. Choosing a different port silently would
    // mean the manifest records one number, the running container publishes another, and the
    // README tells you to connect to a third.
    if let (Some((_, _, _, database_port)), Some(cache_port)) = (&database_settings, cache_port)
        && *database_port == cache_port
    {
        return Err(CliError::new(
            Code::UnsupportedCombination,
            format!(
                "`--database-port` and `--cache-port` are both {cache_port}; two services \
                     cannot publish the same host port. It is refused rather than moved to the \
                     next free port, because a port chosen for you is a port the manifest and the \
                     README would both describe wrongly"
            ),
        )
        .with("flags", "--database-port, --cache-port")
        .with("port", cache_port.to_string()));
    }

    Ok(Some(ContainerSettings {
        database_version: database_settings.as_ref().map(|(version, ..)| *version),
        database_name: database_settings.as_ref().map(|(_, name, ..)| name.clone()),
        database_user: database_settings
            .as_ref()
            .map(|(_, _, user, _)| user.clone()),
        database_port: database_settings.as_ref().map(|(.., port)| *port),
        cache,
        cache_port,
    }))
}

/// Serialises a database as its stable name.
///
/// # Why a helper rather than `#[derive(Serialize)]` on `DatabaseKind`
///
/// Because that would put `serde` into `renvor-database`, and the whole point of that crate is
/// that it carries nothing an application does not need. The name written here is the same
/// `as_str()` the runtime parses back, so there is still exactly one spelling.
fn serialize_database<S>(value: &Option<DatabaseKind>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        Some(kind) => serializer.serialize_some(kind.as_str()),
        None => serializer.serialize_none(),
    }
}

/// The project name a destination implies, when the operator did not give one.
///
/// # Why this is a function and not two copies of a `format!`
///
/// SC-003 requires the wizard and the flags to resolve to the **same** configuration. Until
/// 2026-08-18 this rule existed twice — once in `resolve` and once in `prompts::fill` as the
/// prompt's default — and the two were **not equivalent**: the wizard read `file_name()` off the
/// raw requested path with a hard-coded `"app"` fallback, while `resolve` used the *validated*
/// destination's name. For an ordinary path they agreed. For a path with a trailing separator or a
/// `.` component they need not have.
///
/// A duplicated default is the specific way SC-003 fails: nothing declares the two interfaces
/// different, they just drift. There is now one copy, and both callers use it.
#[must_use]
pub fn derive_project_name(destination: &std::path::Path) -> String {
    destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app")
        .to_owned()
}

/// The local development domain a project name implies, when the operator did not give one.
///
/// Same reasoning as [`derive_project_name`].
#[must_use]
pub fn derive_local_domain(name: &str) -> String {
    format!("{name}.test")
}

impl ProjectConfiguration {
    /// Validates answers into a configuration and the capability to write it, or refuses.
    ///
    /// **Performs no filesystem writes.** It reads the filesystem — the destination boundary has
    /// to — and opens one directory handle, but creates nothing, which is what lets FR-007 hold.
    ///
    /// # Why this returns two values
    ///
    /// The configuration is data: serialisable, comparable, and the thing SC-003 asserts is
    /// identical between the wizard and the flags. The [`Destination`] is a **capability** holding
    /// an open directory handle, and it is neither serialisable nor comparable. Fusing them would
    /// mean either weakening the configuration to hold a handle or weakening the capability to a
    /// path. They are returned as a pair instead.
    ///
    /// # Errors
    ///
    /// Any of the validation codes. Every one is `Exit::Validation` except
    /// [`Code::InvalidProjectName`], which is also validation. Nothing here can produce a
    /// cancellation or an environment failure.
    pub fn resolve(answers: Answers) -> Result<(Self, Destination), CliError> {
        // A NAME THE OPERATOR SUPPLIED IS CHECKED BEFORE THE FILESYSTEM IS CONSULTED.
        //
        // Both orders refuse the same inputs, so this is not a security change — it is a
        // diagnosis change, and the old order produced diagnoses that depended on the machine.
        // `renvor new /tmp/x` reported "a project name is a single directory name, not a path";
        // `renvor new /absolute/path` reported "the destination's parent does not exist" — the
        // same mistake by the same operator, explained two different ways, because `/tmp` happens
        // to exist and `/absolute` happens not to.
        //
        // Checking the supplied name first makes the answer deterministic. It touches no
        // filesystem, so FR-007's "validation before any write" is unaffected: both checks are
        // pre-write, and this one is now the cheaper of the two as well as the more specific.
        //
        // A *derived* name still has to wait, because it cannot exist until the destination has
        // been resolved. That asymmetry is inherent rather than an oversight.
        if let Some(name) = answers.name.as_deref() {
            validate_project_name(name)?;
        }

        let destination = Destination::open(&answers.destination)?;

        // CLONED so that `answers` stays whole. `resolve_container` below needs the container
        // fields, and a partial move here would take the name out from under it — the alternative
        // being to thread six `Option<&str>` through a second signature for the sake of one short
        // string.
        let name = match answers.name.clone() {
            Some(name) => name,
            // Derived through the SHARED function, from the validated destination's name — which
            // is the same string the wizard offers as its default, because the wizard calls this.
            None => derive_project_name(std::path::Path::new(destination.name())),
        };
        validate_project_name(&name)?;

        let target = Target::parse(&answers.target)?;

        // Defaulted rather than prompted, and RECORDED. See `Transport`'s documentation for the
        // constitutional clause this implements.
        let transport = match answers.transport.as_deref() {
            Some(value) => Transport::parse(value)?,
            None => Transport::Rest,
        };

        let local_domain = match answers.local_domain.clone() {
            Some(domain) => domain,
            None => derive_local_domain(&name),
        };
        validate_local_domain(&local_domain)?;

        // Cross-choice constraints. There is exactly one in this phase, and it is stated rather
        // than left implicit: seed data describes an example domain's data, so asking for it
        // without the example domain asks for data with nothing to put it in.
        if answers.seed_data && !answers.example_domain {
            return Err(CliError::new(
                Code::UnsupportedCombination,
                "`--seed-data` needs `--example-domain`: seed data populates the example domain, \
                 and without it there is nothing to seed",
            )
            .with("flags", "--seed-data, --example-domain"));
        }

        // Persistence is a PAIR. `--orm` without `--database` names a layer with nothing to talk
        // to, and `--database` without `--orm` leaves the query style undeclared. Rather than
        // default the missing half — which is the silent fallback principle IV prohibits — both
        // directions are refused with the flag that would complete the pair.
        let (orm, database) = match (answers.orm.as_deref(), answers.database.as_deref()) {
            (None, None) => (None, None),
            (Some(orm), Some(database)) => {
                (Some(Orm::parse(orm)?), Some(parse_database(database)?))
            }
            // A database alone takes SQLx. THIS IS NOW A REAL DEFAULT, and the justification the
            // line carried in Phase 006 — "`sqlx` is the only value `--orm` accepts, so there is
            // nothing to choose between" — stopped being true the moment `seaorm` was added.
            //
            // The behaviour is kept because Phase 006 shipped it: a command that generated a
            // working project must keep generating the same one, and FR-040 requires exactly that.
            // What changes is the honesty of the reason. It is a DOCUMENTED COMPATIBILITY DEFAULT,
            // stated in the CLI reference and in `--help`, not an absence of alternatives — and an
            // operator who wants the other model names it.
            (None, Some(database)) => (Some(Orm::Sqlx), Some(parse_database(database)?)),
            (Some(orm), None) => {
                // Parsed first, so an operator who typed both a bad layer AND omitted the database
                // hears about the value they got wrong rather than the flag they omitted.
                Orm::parse(orm)?;
                return Err(CliError::new(
                    Code::UnsupportedCombination,
                    "`--orm` needs `--database`: a persistence layer with no database selected \
                     has nothing to generate against",
                )
                .with("flags", "--orm, --database")
                .with("supported", "postgres, mysql"));
            }
        };

        // ── THE AUTH STARTER AND THE CAPABILITIES (Phase 011, W-023 and W-024) ──────────
        //
        // Parsed, then checked against each other and against persistence, in ONE place for both
        // interfaces. Every refusal names the flag that would complete the selection; nothing is
        // defaulted into a state the generator could not honour.
        let auth = match answers.auth.as_deref() {
            Some(value) => AuthStarter::parse(value)?,
            None => AuthStarter::None,
        };
        let capabilities = match answers.capabilities.as_deref() {
            Some(value) => Capabilities::parse(value)?,
            None => Capabilities::default(),
        };
        if auth == AuthStarter::Session && database.is_none() {
            return Err(CliError::new(
                Code::UnsupportedCombination,
                "`--auth session` needs `--database`: sessions, credentials, and tokens live in \
                 the selected persistence row, and without one there is nowhere to keep them",
            )
            .with("flags", "--auth, --database")
            .with("supported", "postgres, mysql"));
        }
        if auth == AuthStarter::Session && !capabilities.contains(Capability::Mail) {
            return Err(CliError::new(
                Code::UnsupportedCombination,
                "`--auth session` needs the `mail` capability: registration, verification, and \
                 password reset send mail, and a starter whose mail went nowhere would be the \
                 silent fallback the constitution forbids. Add `mail` to `--capabilities`",
            )
            .with("flags", "--auth, --capabilities"));
        }
        if capabilities.contains(Capability::Jobs) && database.is_none() {
            return Err(CliError::new(
                Code::UnsupportedCombination,
                "the `jobs` capability needs `--database`: the durable job store is the \
                 application's own persistence row (ADR-0032), and without one there is no store",
            )
            .with("flags", "--capabilities, --database")
            .with("supported", "postgres, mysql"));
        }

        // ── THE FRAMEWORK SOURCE ────────────────────────────────────────────────────────
        //
        // Validated before anything is staged: two files read, nothing evaluated. Required only
        // when a selection needs the framework — and then REQUIRED, because recording a starter
        // choice that no `Cargo.toml` could honour is exactly what W-023 and W-024 forbid.
        let framework = match answers.framework_path.as_deref() {
            Some(path) => Some(FrameworkSource::validate_path(path)?),
            None => None,
        };
        let needs_framework = auth != AuthStarter::None || !capabilities.is_empty();
        if needs_framework && framework.is_none() {
            let asked = match (auth, capabilities.is_empty()) {
                (AuthStarter::Session, true) => "`--auth session`".to_owned(),
                (AuthStarter::Session, false) => {
                    format!(
                        "`--auth session` and `--capabilities {}`",
                        capabilities.as_flag_value()
                    )
                }
                (AuthStarter::None, _) => {
                    format!("`--capabilities {}`", capabilities.as_flag_value())
                }
            };
            return Err(CliError::new(
                Code::UnsupportedCombination,
                format!(
                    "{asked} needs `--framework-path`: no Renvor crate is published, so a \
                     generated project can depend on the framework only by a path to a checkout \
                     of it. Without one the choice could not be honoured, and a choice the \
                     generator cannot honour is refused rather than recorded"
                ),
            )
            .with("flags", "--framework-path"));
        }

        // ── CONTAINER CONTROLS ──────────────────────────────────────────────────────────
        //
        // Resolved through ONE path, reached identically by the wizard and by the flags. The
        // wizard fills `Answers` and calls this; it does not build a `ContainerSettings` of its
        // own. That is what makes "interactive and non-interactive resolve identically" a property
        // of the code rather than of two implementations that currently agree.
        let container_settings = resolve_container(
            &answers,
            answers.container,
            database,
            &name,
            capabilities.contains(Capability::Cache),
        )?;

        let configuration = Self {
            name,
            destination: destination.display_path(),
            local_domain,
            target,
            transport,
            container: answers.container,
            local_https: if answers.local_https {
                LocalHttps::Requested
            } else {
                LocalHttps::Off
            },
            seed_data: answers.seed_data,
            example_domain: answers.example_domain,
            orm,
            database,
            container_settings,
            auth,
            capabilities,
            framework,
        };
        Ok((configuration, destination))
    }

    /// The project name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The persistence layer, or `None` when the project was generated without one.
    #[must_use]
    pub const fn orm(&self) -> Option<Orm> {
        self.orm
    }

    /// The selected database, or `None` when the project was generated without persistence.
    ///
    /// This is what template selection reads, so a project carries persistence sources only when a
    /// database was actually chosen — data-model invariant I-12.
    #[must_use]
    pub const fn database(&self) -> Option<DatabaseKind> {
        self.database
    }

    /// The destination as it will be reported, **for display only**. Not a capability.
    ///
    /// Control characters are escaped, because "for display only" is exactly the position in which
    /// a path the operator typed gets printed to a terminal.
    #[must_use]
    pub fn destination_display(&self) -> String {
        crate::output::redact::path(&self.destination)
    }

    /// The local development domain.
    #[must_use]
    pub fn local_domain(&self) -> &str {
        &self.local_domain
    }

    /// The recorded transport.
    #[must_use]
    pub const fn transport(&self) -> Transport {
        self.transport
    }

    /// Whether container controls are generated.
    #[must_use]
    pub fn container(&self) -> bool {
        self.container
    }

    /// The container development controls, or `None` when `--container` was not selected.
    ///
    /// A reference rather than a copy: [`ContainerSettings`] owns two `Identifier`s, and a
    /// `Copy` type holding a validated name is a type somebody can build a second way.
    #[must_use]
    pub const fn container_settings(&self) -> Option<&ContainerSettings> {
        self.container_settings.as_ref()
    }

    /// Whether local HTTPS was requested. **Nothing is issued either way.**
    #[must_use]
    pub fn local_https(&self) -> LocalHttps {
        self.local_https
    }

    /// Whether the example domain is generated.
    #[must_use]
    pub fn example_domain(&self) -> bool {
        self.example_domain
    }

    /// Whether seed data is generated.
    #[must_use]
    pub fn seed_data(&self) -> bool {
        self.seed_data
    }

    /// The target.
    #[must_use]
    pub fn target(&self) -> Target {
        self.target
    }

    /// The authentication starter.
    #[must_use]
    pub const fn auth(&self) -> AuthStarter {
        self.auth
    }

    /// The selected capabilities.
    #[must_use]
    pub const fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// Where the framework crates come from, or `None` for the dependency-free skeleton.
    #[must_use]
    pub const fn framework(&self) -> Option<&FrameworkSource> {
        self.framework.as_ref()
    }

    /// Whether this project is a framework-backed **starter** rather than the skeleton.
    #[must_use]
    pub const fn is_starter(&self) -> bool {
        self.framework.is_some()
    }

    /// Whether the generated application reads the cache a `--container` cache service runs.
    ///
    /// `true` iff the `cache` capability was selected; the recorded
    /// `cache_wired_into_application` follows this rather than being the constant `false` it was
    /// through Phase 010.
    #[must_use]
    pub fn cache_wired_into_application(&self) -> bool {
        self.capabilities.contains(Capability::Cache)
    }

    /// The exact non-interactive command that reproduces this configuration.
    ///
    /// Printed on the review screen and when confirmation is declined, so the answers survive a
    /// change of mind (FR-009, US1 acceptance scenario 3).
    #[must_use]
    pub fn equivalent_command(&self) -> String {
        // THE ORDER AND SPELLING HERE ARE THE FLAG SURFACE'S, NOT A DESCRIPTION OF IT.
        //
        // An earlier version emitted `renvor new <destination> --name <name>`, which reads
        // plausibly and **is not a command**: `NewArgs` takes the project name as the positional
        // argument and the destination as `--path`, and declares no `--name` flag at all. So the
        // review screen's "exact equivalent command" (FR-009) failed with clap's *"unexpected
        // argument '--name'"* if anyone pasted it.
        //
        // It survived because nothing ran the string it produced. `tests/parity.rs` now does:
        // `the_equivalent_command_printed_by_the_wizard_actually_reproduces_the_project` executes
        // this output verbatim and compares the result byte for byte. A command that is printed
        // but never run is a claim, not a contract.
        let mut parts = vec![
            "renvor new".to_owned(),
            shell_quote(&self.name),
            // ── ESCAPED, EVEN THOUGH THIS COMMAND IS MEANT TO BE RUNNABLE ──────────────
            //
            // `shell_quote` makes the path safe for a *shell*. It does nothing about a terminal:
            // single quotes do not stop an `ESC` byte inside them from being interpreted when the
            // line is printed, and this line is printed to the operator's review screen.
            //
            // So there is a real trade here and it is taken deliberately. If the destination
            // contains a control character, the printed command is **no longer runnable verbatim** —
            // it carries `\u{1b}` as literal text — and the operator has to retype that part. The
            // alternative is a command that runs and, on the way past, reprograms their terminal.
            // A command that is visibly wrong beats one that is invisibly wrong.
            //
            // This costs nothing in the ordinary case: an escape of a path with no control
            // characters is the identical string, and `tests/parity.rs` proves it by **executing**
            // this command and comparing the generated tree byte for byte. RULE 1b already refuses
            // a control character in the final component, so only an ancestor directory can put one
            // here at all.
            format!(
                "--path {}",
                shell_quote(&crate::output::redact::path(&self.destination))
            ),
            format!(
                "--target {}",
                match self.target {
                    Target::Api => "api",
                }
            ),
            format!("--local-domain {}", shell_quote(&self.local_domain)),
        ];
        if self.container {
            parts.push("--container".to_owned());
        }
        if self.local_https == LocalHttps::Requested {
            parts.push("--local-https".to_owned());
        }
        if self.example_domain {
            parts.push("--example-domain".to_owned());
        }
        if self.seed_data {
            parts.push("--seed-data".to_owned());
        }
        // ── PERSISTENCE. THESE WERE MISSING, and the omission was a real defect. ────────
        //
        // Until the Phase 006 container work, a wizard run that selected a database printed a
        // command that reproduced everything EXCEPT the database — so pasting it produced a
        // project with no `src/persistence.rs` and no `migrations/`, silently. FR-009 calls this
        // string the "exact equivalent command", and it was not.
        //
        // `tests/parity.rs` executes this output and compares the tree byte for byte, which is
        // exactly the test that should have caught it — and did not, because no parity case
        // selected a database. One now does.
        if let Some(database) = self.database {
            parts.push(format!("--database {}", database.as_str()));
        }
        if let Some(orm) = self.orm {
            parts.push(format!("--orm {}", orm.as_str()));
        }
        // ── PHASE 011. Always spelled out, defaults included, for the reason the container
        // block gives: a command that omitted a defaulted value reproduces this project only for
        // as long as the default stays the same.
        parts.push(format!("--auth {}", self.auth.as_str()));
        parts.push(format!(
            "--capabilities {}",
            self.capabilities.as_flag_value()
        ));
        if let Some(framework) = &self.framework {
            parts.push(format!(
                "--framework-path {}",
                shell_quote(&crate::output::redact::path(framework.path()))
            ));
        }
        // ── CONTAINER CONTROLS ─────────────────────────────────────────────────────────
        //
        // EVERY non-default answer, including the ones that happen to equal the default. A command
        // that omitted a defaulted value would reproduce this project only for as long as the
        // default stayed the same — and the whole point of printing it is that somebody pastes it
        // into a script that outlives this release.
        if let Some(settings) = &self.container_settings {
            if let Some(version) = settings.database_version {
                parts.push(format!("--database-version {}", version.as_str()));
            }
            if let Some(name) = &settings.database_name {
                parts.push(format!("--database-name {}", shell_quote(name.as_str())));
            }
            if let Some(user) = &settings.database_user {
                parts.push(format!("--database-user {}", shell_quote(user.as_str())));
            }
            if let Some(port) = settings.database_port {
                parts.push(format!("--database-port {port}"));
            }
            if settings.cache.engine().is_some() {
                parts.push(format!("--container-cache {}", settings.cache.as_str()));
            }
            if let Some(port) = settings.cache_port {
                parts.push(format!("--cache-port {port}"));
            }
        }
        parts.push("--yes".to_owned());
        parts.join(" ")
    }
}

/// Quotes a shell argument only when it needs it, so the printed command stays readable.
fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-/".contains(c))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

/// Rejects a local development domain that is not a usable hostname.
fn validate_local_domain(domain: &str) -> Result<(), CliError> {
    let invalid = |why: &str| {
        CliError::new(Code::UnsupportedValue, why.to_owned())
            .with("flag", "--local-domain")
            .with("value", domain.to_owned())
    };
    if domain.is_empty() || domain.len() > 253 {
        return Err(invalid(
            "a local domain must be between 1 and 253 characters",
        ));
    }
    for label in domain.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(invalid(
                "each label in a local domain must be between 1 and 63 characters",
            ));
        }
        if !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            || label.starts_with('-')
            || label.ends_with('-')
        {
            return Err(invalid(
                "a local domain label may contain only ASCII letters, digits, and interior \
                 hyphens",
            ));
        }
    }
    Ok(())
}

/// Builds the reserved-flag refusal.
///
/// **Not** an unknown-flag error, and **not** silently ignored. FR-005b: "unknown flag" tells a
/// user their command is wrong; this tells them when it will be right.
fn reserved(flag: &str, value: &str, phase: &str) -> CliError {
    CliError::new(
        Code::ReservedForLaterPhase,
        format!(
            "`{flag} {value}` is reserved for a later phase and is not supported yet. It is \
             accepted by the command grammar so that this command line keeps its meaning when \
             {phase} implements it, rather than becoming an unknown-flag error"
        ),
    )
    .with("flag", flag.to_owned())
    .with("value", value.to_owned())
    .with("phase", phase.to_owned())
}

#[cfg(test)]
mod tests {
    /// Every serialised ORM value must be one `--orm` accepts.
    ///
    /// The gap this closes is not hypothetical: the derive emitted `"sea-orm"`, which is a third
    /// spelling of a value with two, and `--orm sea-orm` is refused.
    #[test]
    fn every_serialised_orm_round_trips_through_the_flag() {
        for orm in Orm::ALL {
            let json = serde_json::to_string(&orm).expect("serialises");
            let bare = json.trim_matches('"');
            assert_eq!(
                Orm::parse(bare).map(|parsed| parsed.as_str()).ok(),
                Some(orm.as_str()),
                "`{bare}` is emitted in JSON but `--orm {bare}` is refused"
            );
        }
    }

    use super::*;

    fn answers(destination: PathBuf) -> Answers {
        Answers {
            name: None,
            destination,
            local_domain: None,
            target: "api".to_owned(),
            transport: None,
            container: false,
            local_https: false,
            seed_data: false,
            example_domain: false,
            orm: None,
            database: None,
            database_version: None,
            database_name: None,
            database_user: None,
            database_port: None,
            container_cache: None,
            cache_port: None,
            auth: None,
            capabilities: None,
            framework_path: None,
        }
    }

    /// The equivalent command names the ORM, so re-running it reproduces the same project.
    ///
    /// Asserted here rather than on generated output because the command is printed only after an
    /// INTERACTIVE run — echoing a command back at somebody who just typed it is noise — and a
    /// test cannot answer prompts.
    #[test]
    fn the_equivalent_command_names_a_non_default_orm() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut input = answers(base.path().join("demo"));
        input.name = Some("demo".to_owned());
        input.orm = Some("seaorm".to_owned());
        input.database = Some("postgres".to_owned());
        let (configuration, _destination) = ProjectConfiguration::resolve(input).expect("resolves");
        let command = configuration.equivalent_command();
        assert!(
            command.contains("--orm seaorm") || command.contains("--orm=seaorm"),
            "the equivalent command omits the ORM, so re-running it produces the SQLx project \
             instead: {command}"
        );
    }

    // ── Phase 006 persistence selection (FR-032, FR-045, FR-047) ───────────────────────

    #[test]
    fn an_unsupported_persistence_layer_is_refused_by_name() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut answers = answers(base.path().join("demo"));
        answers.orm = Some("diesel".to_owned());
        answers.database = Some("postgres".to_owned());
        let error = ProjectConfiguration::resolve(answers).expect_err("refuses");
        assert_eq!(error.code, Code::UnsupportedValue);
        assert!(
            error.message.contains("sqlx"),
            "the refusal must name the supported value: {}",
            error.message
        );
    }

    #[test]
    fn an_unsupported_database_is_refused_with_both_supported_values() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut answers = answers(base.path().join("demo"));
        answers.database = Some("sqlite".to_owned());
        let error = ProjectConfiguration::resolve(answers).expect_err("refuses");
        assert_eq!(error.code, Code::UnsupportedValue);
        // SQLite is the one an operator is most likely to try, and Phase 006 does not ship it.
        // Naming both supported values is what makes the refusal actionable rather than a wall.
        for supported in ["postgres", "mysql"] {
            assert!(
                error.message.contains(supported),
                "the refusal must name `{supported}`: {}",
                error.message
            );
        }
    }

    #[test]
    fn a_persistence_layer_without_a_database_is_refused_rather_than_defaulted() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut answers = answers(base.path().join("demo"));
        answers.orm = Some("sqlx".to_owned());
        let error = ProjectConfiguration::resolve(answers).expect_err("refuses");
        assert_eq!(error.code, Code::UnsupportedCombination);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "flags" && v.contains("--database"))
        );
    }

    #[test]
    fn a_database_alone_takes_the_single_supported_layer() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut answers = answers(base.path().join("demo"));
        answers.database = Some("mysql".to_owned());
        let (configuration, _capability) =
            ProjectConfiguration::resolve(answers).expect("resolves");
        assert_eq!(configuration.database(), Some(DatabaseKind::MySql));
        assert_eq!(configuration.orm(), Some(Orm::Sqlx));
    }

    #[test]
    fn no_persistence_is_recorded_as_no_persistence() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let (configuration, _capability) =
            ProjectConfiguration::resolve(answers(base.path().join("demo"))).expect("resolves");
        assert_eq!(configuration.database(), None);
        assert_eq!(configuration.orm(), None);
    }

    #[test]
    fn a_reserved_target_names_the_phase_rather_than_reporting_an_unknown_flag() {
        let error = Target::parse("full-stack").unwrap_err();
        assert_eq!(error.code, Code::ReservedForLaterPhase);
        assert!(
            error.details.iter().any(|(k, _)| k == "phase"),
            "the refusal must name the phase; without it the message is just a rejection"
        );
    }

    #[test]
    fn an_unsupported_target_lists_what_is_supported() {
        let error = Target::parse("banana").unwrap_err();
        assert_eq!(error.code, Code::UnsupportedValue);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "supported" && v == "api")
        );
    }

    #[test]
    fn resolution_is_pure_with_respect_to_the_destination() {
        // FR-007: validation writes nothing. Resolving into a directory that does not exist yet
        // must not create it.
        let base = tempfile::tempdir().expect("a temporary directory");
        let destination = base.path().join("demo");
        let config = ProjectConfiguration::resolve(answers(destination.clone()))
            .expect("an ordinary destination resolves")
            .0;
        assert_eq!(config.name(), "demo");
        assert!(
            !destination.exists(),
            "resolving a configuration created the destination; FR-007 requires validation to \
             write nothing"
        );
    }

    #[test]
    fn the_local_domain_defaults_from_the_name() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let config = ProjectConfiguration::resolve(answers(base.path().join("commerce")))
            .expect("resolves")
            .0;
        assert_eq!(config.local_domain(), "commerce.test");
    }

    #[test]
    fn seed_data_without_the_example_domain_is_refused() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut a = answers(base.path().join("demo"));
        a.seed_data = true;
        let error = ProjectConfiguration::resolve(a).unwrap_err();
        assert_eq!(error.code, Code::UnsupportedCombination);
    }

    #[test]
    fn seed_data_with_the_example_domain_is_accepted() {
        // POSITIVE CONTROL for the cross-choice rule above.
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut a = answers(base.path().join("demo"));
        a.seed_data = true;
        a.example_domain = true;
        assert!(ProjectConfiguration::resolve(a).is_ok());
    }

    #[test]
    fn every_configuration_field_is_inert_and_a_new_one_cannot_be_added_unclassified() {
        // THE TEST `output/redact.rs` HAS ALWAYS CLAIMED TO HAVE, AND DID NOT.
        //
        // Its module header said: *"the value is in the **test**, which asserts that every
        // configuration field is inert, and which fails when a new field is added without being
        // classified."* No such test existed. `tests/redaction.rs` plants a secret in the
        // **destination** and nowhere else, so it exercised one field of eight. An advisory review
        // found the claim by looking for the test it described.
        //
        // The load-bearing line is the destructuring below. It is **exhaustive**, so adding a field
        // to `ProjectConfiguration` stops this compiling until somebody comes here and says which
        // of the two categories the new field is in. That is the guard the comment promised: not a
        // runtime assertion that can be true by accident, but a compile error.
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut answers = answers(base.path().join("demo"));
        answers.container = true;
        answers.local_https = true;
        answers.example_domain = true;
        answers.seed_data = true;
        let (configuration, _capability) =
            ProjectConfiguration::resolve(answers).expect("resolves");

        let ProjectConfiguration {
            // ── CATEGORY 1: cannot hold a secret because it cannot hold a string ────────
            //
            // A `bool` and a two-variant enum have no room for a credential. Invariant I-3 is
            // structural for these.
            container: _,
            local_https: _,
            seed_data: _,
            example_domain: _,
            target: _,
            // Phase 004. A single-variant enum has even less room for a credential than a `bool`.
            transport: _,
            // Phase 006. Both are closed enums behind an `Option`, so the only values they can
            // hold are the ones this crate names. A database CONNECTION string would be a
            // secret-bearing field — which is exactly why the configuration holds a database
            // KIND and never a DSN. Where the application gets its connection string from is a
            // runtime concern, and keeping it out of this struct is what keeps invariant I-3
            // structural rather than a rule someone has to remember.
            orm: _,
            database: _,

            // ── CATEGORY 2: strings, and therefore checked ─────────────────────────────
            name,
            local_domain,
            destination,
            // Phase 006 container scope. `ContainerSettings` holds two `Identifier`s, which ARE
            // strings — so it is category 2 and is checked below rather than waved through. Its
            // other fields are two `u16`s and two closed enums, which have no room for a
            // credential. There is deliberately no password field, and adding one would have to
            // pass this test.
            container_settings,
            // Phase 011. A two-variant enum and a set of a five-variant enum: category 1. The
            // keys the auth starter needs are read by the GENERATED application from its
            // environment; there is no field here that could carry one, and adding one would
            // have to pass this test.
            auth: _,
            capabilities: _,
            // Phase 011. A path the operator typed, like `destination` — category 2, checked
            // below. Validation refuses a control character anywhere in it, and the JSON
            // rendering goes through the redacting path renderer.
            framework,
        } = configuration.clone();

        assert!(
            framework.is_none(),
            "this configuration selected no framework; the framework path is the one Phase 011 \
             field redaction coverage must exercise, and `tests/redaction.rs` plants a value in it"
        );

        // The identifier grammar admits ASCII letters, digits, and `_`. `=` and `:` are therefore
        // unrepresentable, which is the property that makes these inert rather than a filter.
        if let Some(settings) = &container_settings {
            for (field, value) in [
                ("database_name", settings.database_name.as_ref()),
                ("database_user", settings.database_user.as_ref()),
            ] {
                if let Some(value) = value {
                    assert!(
                        !value.as_str().contains('=') && !value.as_str().contains(':'),
                        "{field} can carry a key=value shape, so it needs redaction coverage"
                    );
                }
            }
        }

        // `name` and `local_domain` are validated to character sets that exclude `=` and `:`, so
        // they cannot carry a `key=value` shape at all. Asserted rather than assumed, because that
        // is the property doing the work.
        for (field, value) in [("name", &name), ("local_domain", &local_domain)] {
            assert!(
                !value.contains('=') && !value.contains(':'),
                "{field} can carry a key=value shape, so it needs redaction coverage"
            );
        }

        // `destination` is the one field an operator can put anything into, and it is the one
        // `tests/redaction.rs` plants a secret in — on both a failing run and, since 2026-08-18, a
        // successful one.
        assert!(
            !destination.as_os_str().is_empty(),
            "the destination is the field redaction coverage must exercise"
        );
    }

    // ── Phase 011: the auth starter, the capabilities, and the framework source ──────────
    //
    // Every test below was written BEFORE the types existed (RED observed as a compile failure
    // naming `AuthStarter`, `Capability`, `Capabilities`, and `FrameworkSource`), then made
    // green by the implementation above. Recorded in `specs/011-…/evidence/batch-a.md`.

    /// A directory shaped like a Renvor workspace, for the framework-path rules.
    fn fake_framework(base: &std::path::Path) -> PathBuf {
        let root = base.join("framework");
        std::fs::create_dir_all(root.join("crates/renvor")).expect("mkdir");
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"3\"\nmembers = [\"crates/renvor\"]\n",
        )
        .expect("write");
        std::fs::write(
            root.join("crates/renvor/Cargo.toml"),
            "[package]\nname = \"renvor\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )
        .expect("write");
        std::fs::write(root.join("Cargo.lock"), "# lock\nversion = 4\n").expect("write");
        root
    }

    #[test]
    fn auth_defaults_to_none_and_is_recorded() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let (configuration, _capability) =
            ProjectConfiguration::resolve(answers(base.path().join("demo"))).expect("resolves");
        assert_eq!(configuration.auth(), AuthStarter::None);
        assert!(configuration.capabilities().is_empty());
        assert!(configuration.framework().is_none());
    }

    #[test]
    fn api_and_full_are_unsupported_values_not_reservations() {
        // C-2 of the clarifications: Phase 009 shipped refresh, not issuance, so a generated
        // `api` mode would be a project that cannot mint its first credential. Refused by name,
        // and NOT as a later phase's — no phase is assigned to issuance and naming one is a
        // promise.
        for value in ["api", "full"] {
            let error = AuthStarter::parse(value).expect_err("refused");
            assert_eq!(error.code, Code::UnsupportedValue, "{value}");
            assert_ne!(error.code, Code::ReservedForLaterPhase, "{value}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "supported" && v == "none, session"),
                "{value}: {:?}",
                error.details
            );
            assert!(
                error.message.contains("issu"),
                "the refusal must say WHY: {}",
                error.message
            );
        }
        // POSITIVE CONTROL: the two real values parse.
        assert_eq!(
            AuthStarter::parse("none").expect("parses"),
            AuthStarter::None
        );
        assert_eq!(
            AuthStarter::parse("session").expect("parses"),
            AuthStarter::Session
        );
    }

    #[test]
    fn a_session_starter_without_a_database_is_refused() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut a = answers(base.path().join("demo"));
        a.auth = Some("session".to_owned());
        a.capabilities = Some("mail".to_owned());
        a.framework_path = Some(fake_framework(base.path()));
        let error = ProjectConfiguration::resolve(a).expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedCombination);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "flags" && v.contains("--database") && v.contains("--auth")),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn a_session_starter_without_the_mail_capability_is_refused() {
        // C-3: a starter whose verification mail goes nowhere is the silent fallback the
        // constitution forbids, so `session` needs `mail` and says so.
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut a = answers(base.path().join("demo"));
        a.auth = Some("session".to_owned());
        a.database = Some("postgres".to_owned());
        a.framework_path = Some(fake_framework(base.path()));
        let error = ProjectConfiguration::resolve(a).expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedCombination);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "flags" && v.contains("--capabilities")),
            "{:?}",
            error.details
        );
        assert!(error.message.contains("mail"), "{}", error.message);
    }

    #[test]
    fn jobs_without_a_database_is_refused() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut a = answers(base.path().join("demo"));
        a.capabilities = Some("jobs".to_owned());
        a.framework_path = Some(fake_framework(base.path()));
        let error = ProjectConfiguration::resolve(a).expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedCombination);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "flags" && v.contains("--database")),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn a_selection_that_needs_the_framework_without_a_path_is_refused_not_recorded() {
        // THE W-023 / W-024 CONTROL: an unsupported input keeps failing explicitly rather than
        // being recorded as an inert choice. Without a framework path there is nothing a
        // starter could depend on, so the choice cannot be honoured — and an unhonoured choice
        // must not reach the manifest.
        let base = tempfile::tempdir().expect("a temporary directory");
        for (auth, capabilities) in [
            (Some("session"), Some("mail")),
            (None, Some("cache")),
            (None, Some("observability")),
        ] {
            let mut a = answers(base.path().join("demo"));
            a.auth = auth.map(str::to_owned);
            a.capabilities = capabilities.map(str::to_owned);
            a.database = Some("postgres".to_owned());
            let error = ProjectConfiguration::resolve(a).expect_err("refused");
            assert_eq!(
                error.code,
                Code::UnsupportedCombination,
                "{auth:?} {capabilities:?}"
            );
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "flags" && v.contains("--framework-path")),
                "{:?}",
                error.details
            );
        }
    }

    #[test]
    fn an_unknown_capability_is_refused_naming_the_five() {
        let error = Capabilities::parse("s3").expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedValue);
        let supported = error
            .details
            .iter()
            .find(|(k, _)| k == "supported")
            .map(|(_, v)| v.clone())
            .expect("names the supported set");
        for name in ["cache", "jobs", "mail", "storage", "observability"] {
            assert!(supported.contains(name), "{supported}");
        }
        assert!(!supported.contains("s3"));
    }

    #[test]
    fn a_duplicate_capability_is_refused_and_none_with_a_name_is_refused() {
        assert_eq!(
            Capabilities::parse("cache,cache")
                .expect_err("refused")
                .code,
            Code::UnsupportedValue
        );
        assert_eq!(
            Capabilities::parse("none,cache").expect_err("refused").code,
            Code::UnsupportedCombination
        );
        assert_eq!(
            Capabilities::parse("").expect_err("refused").code,
            Code::UnsupportedValue
        );
        // POSITIVE CONTROLS.
        assert!(Capabilities::parse("none").expect("parses").is_empty());
        let set = Capabilities::parse("storage, cache").expect("parses with a space");
        assert!(set.contains(Capability::Cache) && set.contains(Capability::Storage));
        assert!(!set.contains(Capability::Jobs));
        assert_eq!(set.as_flag_value(), "cache,storage", "sorted, canonical");
    }

    #[test]
    fn a_framework_path_that_is_not_a_workspace_is_refused_before_any_write() {
        let base = tempfile::tempdir().expect("a temporary directory");
        // A directory with no Cargo.toml.
        let empty = base.path().join("empty");
        std::fs::create_dir_all(&empty).expect("mkdir");
        // A package that is not the Renvor workspace.
        let other = base.path().join("other");
        std::fs::create_dir_all(&other).expect("mkdir");
        std::fs::write(
            other.join("Cargo.toml"),
            "[package]\nname = \"other\"\nversion = \"0.1.0\"\n",
        )
        .expect("write");
        // A workspace whose `crates/renvor` is missing.
        let workspace = base.path().join("workspace");
        std::fs::create_dir_all(&workspace).expect("mkdir");
        std::fs::write(workspace.join("Cargo.toml"), "[workspace]\nmembers = []\n").expect("write");

        for (candidate, why) in [
            (base.path().join("absent"), "absent"),
            (empty, "no Cargo.toml"),
            (other, "not a workspace"),
            (workspace, "no crates/renvor"),
        ] {
            let mut a = answers(base.path().join("demo"));
            a.database = Some("postgres".to_owned());
            a.capabilities = Some("cache".to_owned());
            a.framework_path = Some(candidate);
            let error = ProjectConfiguration::resolve(a).expect_err(why);
            assert_eq!(error.code, Code::UnsupportedValue, "{why}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "flag" && v == "--framework-path"),
                "{why}: {:?}",
                error.details
            );
            assert!(
                !base.path().join("demo").exists(),
                "{why}: the destination was created by a refused framework path"
            );
        }
    }

    #[test]
    fn a_framework_without_a_lockfile_is_refused_before_any_write() {
        // FR-006: the starter's resolution is seeded from the framework's `Cargo.lock`; a
        // checkout without one cannot seed anything, so it is refused at validation with its own
        // rule rather than discovered when staging tries to copy it.
        let base = tempfile::tempdir().expect("a temporary directory");
        let framework = fake_framework(base.path());
        std::fs::remove_file(framework.join("Cargo.lock")).expect("removed");
        let error = FrameworkSource::validate_path(&framework).expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedValue);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "rule" && v == "framework_lockfile"),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn a_valid_starter_selection_resolves_and_records_everything() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let framework = fake_framework(base.path());
        let mut a = answers(base.path().join("demo"));
        a.database = Some("mysql".to_owned());
        a.auth = Some("session".to_owned());
        a.capabilities = Some("mail,jobs".to_owned());
        a.framework_path = Some(framework.clone());
        let (configuration, _capability) = ProjectConfiguration::resolve(a).expect("resolves");
        assert_eq!(configuration.auth(), AuthStarter::Session);
        assert!(configuration.capabilities().contains(Capability::Mail));
        assert!(configuration.capabilities().contains(Capability::Jobs));
        assert!(!configuration.capabilities().contains(Capability::Cache));
        let recorded = configuration.framework().expect("recorded");
        assert_eq!(recorded.kind(), "path");
        assert_eq!(
            recorded.path(),
            framework.canonicalize().expect("canonical").as_path(),
            "the path is recorded canonical and absolute"
        );

        let command = configuration.equivalent_command();
        assert!(command.contains("--auth session"), "{command}");
        assert!(command.contains("--capabilities jobs,mail"), "{command}");
        assert!(command.contains("--framework-path "), "{command}");
    }

    #[test]
    fn the_cache_capability_with_containers_wires_the_container_cache() {
        // The container database is generated whenever `--container` meets `--database`; the
        // cache service follows the same rule when `--container` meets the `cache` capability,
        // and an explicit `--container-cache none` beside it is a contradiction, not an override.
        let base = tempfile::tempdir().expect("a temporary directory");
        let framework = fake_framework(base.path());
        let mut a = answers(base.path().join("demo"));
        a.container = true;
        a.capabilities = Some("cache".to_owned());
        a.framework_path = Some(framework.clone());
        let (configuration, _capability) = ProjectConfiguration::resolve(a).expect("resolves");
        let settings = configuration.container_settings().expect("containers");
        assert!(
            settings.cache.engine().is_some(),
            "the cache service is generated"
        );
        assert!(configuration.cache_wired_into_application());

        let mut contradiction = answers(base.path().join("demo2"));
        contradiction.container = true;
        contradiction.capabilities = Some("cache".to_owned());
        contradiction.container_cache = Some("none".to_owned());
        contradiction.framework_path = Some(framework);
        let error = ProjectConfiguration::resolve(contradiction).expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedCombination);
    }

    #[test]
    fn the_equivalent_command_carries_every_answered_choice() {
        // NAMED FOR WHAT IT CHECKS. It was called `…_round_trips_to_the_same_configuration`, and
        // it does not round-trip anything — it asserts that four substrings are present. The
        // difference is not pedantry: while this test was passing under that name, the printed
        // command was `renvor new <destination> --name <name> …`, which this program cannot parse
        // at all, and the name of this test was a large part of why nobody looked.
        //
        // The actual round trip is
        // `tests/parity.rs::the_equivalent_command_printed_by_the_wizard_actually_reproduces_the_project`,
        // which runs the string and compares the resulting tree.
        let base = tempfile::tempdir().expect("a temporary directory");
        let mut a = answers(base.path().join("demo"));
        a.container = true;
        a.example_domain = true;
        let (config, _capability) = ProjectConfiguration::resolve(a).expect("resolves");
        let printed = config.equivalent_command();
        assert!(printed.contains("--container"), "{printed}");
        assert!(printed.contains("--example-domain"), "{printed}");
        assert!(printed.contains("--yes"), "{printed}");
        assert!(!printed.contains("--seed-data"), "{printed}");
    }
}
