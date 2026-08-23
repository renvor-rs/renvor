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

use std::path::PathBuf;

use renvor_database::DatabaseKind;
use serde::Serialize;

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
/// # One value, and why it is still an enum
///
/// Phase 006 ships direct SQLx. A later phase adds an ORM, and when it does, this enum gains a
/// variant rather than a `bool` gaining a meaning. The parse below refuses anything else by name,
/// so a project generated against a future value fails at generation rather than at build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Orm {
    /// Direct SQLx. Queries are written by hand; no object mapper is generated.
    Sqlx,
}

impl Orm {
    /// The recorded spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sqlx => "sqlx",
        }
    }

    /// Parses an `--orm` value.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] for any value other than `sqlx`.
    pub fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "sqlx" => Ok(Self::Sqlx),
            other => Err(CliError::new(
                Code::UnsupportedValue,
                format!("`{other}` is not a supported persistence layer; the supported value is `sqlx`"),
            )
            .with("flag", "--orm")
            .with("value", other.to_owned())
            .with("supported", "sqlx")),
        }
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

        let name = match answers.name {
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

        let local_domain = match answers.local_domain {
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
            // A database alone takes the one supported layer. This is not a silent default: `sqlx`
            // is the only value `--orm` accepts in this phase, so there is nothing to choose
            // between and nothing an operator could have meant instead.
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
        }
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
        assert!(error.details.iter().any(|(k, v)| k == "flags" && v.contains("--database")));
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
        } = configuration.clone();

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
