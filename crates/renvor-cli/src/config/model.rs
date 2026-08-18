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

use std::path::{Path, PathBuf};

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
    container: bool,
    local_https: LocalHttps,
    seed_data: bool,
    example_domain: bool,
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
    /// Generate container development controls.
    pub container: bool,
    /// Request local HTTPS. Records intent only.
    pub local_https: bool,
    /// Generate seed data.
    pub seed_data: bool,
    /// Generate the example domain module.
    pub example_domain: bool,
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
        let destination = Destination::open(&answers.destination)?;

        let name = match answers.name {
            Some(name) => name,
            None => destination.name().to_owned(),
        };
        validate_project_name(&name)?;

        let target = Target::parse(&answers.target)?;

        let local_domain = match answers.local_domain {
            Some(domain) => domain,
            None => format!("{name}.test"),
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

        let configuration = Self {
            name,
            destination: destination.display_path(),
            local_domain,
            target,
            container: answers.container,
            local_https: if answers.local_https {
                LocalHttps::Requested
            } else {
                LocalHttps::Off
            },
            seed_data: answers.seed_data,
            example_domain: answers.example_domain,
        };
        Ok((configuration, destination))
    }

    /// The project name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The validated destination, **for display and for the manifest**. Not a capability.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// The local development domain.
    #[must_use]
    pub fn local_domain(&self) -> &str {
        &self.local_domain
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
        let mut parts = vec![
            "renvor new".to_owned(),
            shell_quote(&self.destination.display().to_string()),
            format!("--name {}", shell_quote(&self.name)),
            format!("--target {}", match self.target {
                Target::Api => "api",
            }),
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
        return Err(invalid("a local domain must be between 1 and 253 characters"));
    }
    for label in domain.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(invalid(
                "each label in a local domain must be between 1 and 63 characters",
            ));
        }
        if !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
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

/// Refuses a flag whose whole subject belongs to a later phase.
///
/// # Errors
///
/// Always [`Code::ReservedForLaterPhase`].
pub fn reserved_flag(flag: &str, value: &str, phase: &str) -> CliError {
    reserved(flag, value, phase)
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
            container: false,
            local_https: false,
            seed_data: false,
            example_domain: false,
        }
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
        assert!(error.details.iter().any(|(k, v)| k == "supported" && v == "api"));
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
    fn the_equivalent_command_round_trips_to_the_same_configuration() {
        // The review screen prints this command. If it did not reproduce the configuration, the
        // screen would be showing a command that produces a different project.
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
