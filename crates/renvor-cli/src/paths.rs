//! The destination boundary.
//!
//! # GATED ON ADR-0011 — DO NOT MERGE THIS FILE BEFORE THAT RECORD IS ACCEPTED
//!
//! `specs/003-interactive-cli/research.md` D6 selects hand-composed containment in preference to
//! `cap-std`'s capability handles. That is **custom infrastructure chosen over a maintained
//! package**, which constitution principle III and FR-045 make conditional on an accepted decision
//! record. The record is `decisions/0011-path-containment-without-capability-handles.md`.
//!
//! # The weakening this file represents, stated rather than implied
//!
//! `cap-std` makes escape **structurally impossible**: every operation goes through a directory
//! handle, so there is no path for a caller to get wrong. This module makes escape **checked**: a
//! code path that forgets to call [`DestinationPath::validate`] is unprotected.
//!
//! Those are not the same guarantee, and this comment exists so that nobody reading the tests —
//! which pass — concludes that they are.
//!
//! # What this does not close
//!
//! Between validation here and the final rename in [`crate::generate::place`], an attacker with
//! write access to the destination's parent can change what the destination names. The rename
//! refuses an existing destination, which converts that race into a **clean failure rather than an
//! overwrite**. It does **not** eliminate it. Eliminating it needs handles held across the whole
//! operation — which is `cap-std`, which is what ADR-0011 declines.

use std::path::{Component, Path, PathBuf};

use crate::exit::{CliError, Code};

/// Names Windows refuses as file or directory names, in any case and with any extension.
///
/// Enumerated rather than described as a class, so a reader can tell whether the list is complete.
/// Checked on **every** platform: a project generated on Linux may well be checked out on Windows,
/// and discovering the name is unusable at that point is worse than refusing it here.
const RESERVED_DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A destination that has passed every boundary rule.
///
/// The type exists so that "validated" is a property a value carries rather than a step a caller
/// remembers. Nothing constructs one except [`DestinationPath::validate`].
#[derive(Debug, Clone)]
pub struct DestinationPath {
    parent: PathBuf,
    name: String,
    full: PathBuf,
}

impl DestinationPath {
    /// The canonical parent directory. The staging directory is created here, which is what makes
    /// the final rename same-filesystem by construction.
    #[must_use]
    pub fn parent(&self) -> &Path {
        &self.parent
    }

    /// The final path component.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The full destination path.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.full
    }

    /// Applies every rule, in order, and refuses before anything is created.
    ///
    /// # Errors
    ///
    /// [`Code::InvalidProjectName`], [`Code::DestinationRejected`],
    /// [`Code::DestinationParentMissing`], or [`Code::DestinationNotEmpty`]. Every rejection names
    /// the rule that fired in `details.rule`, so a refusal is a diagnosis rather than a verdict.
    pub fn validate(requested: &Path) -> Result<Self, CliError> {
        let rejected = |rule: &str, why: &str| {
            CliError::new(Code::DestinationRejected, why.to_owned())
                .with("rule", rule)
                .with("destination", requested.display().to_string())
        };

        // Rule 1 — no traversal component, checked structurally rather than by string search.
        // A textual `..` check misses platform-specific spellings; `Component::ParentDir` does not.
        if requested
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(rejected(
                "no_traversal",
                "the destination contains a `..` component, which could place the project outside \
                 the directory you named",
            ));
        }

        // Rule 3 — the final component is a single name.
        let Some(name) = requested.file_name().and_then(|name| name.to_str()) else {
            return Err(rejected(
                "final_component_is_a_name",
                "the destination has no final path component to use as the project directory",
            ));
        };
        if name.is_empty() || name == "." || name == ".." {
            return Err(rejected(
                "final_component_is_a_name",
                "the destination's final component is not a usable directory name",
            ));
        }

        // Rule 4 — not a platform-reserved device name.
        let stem = name.split('.').next().unwrap_or(name);
        if RESERVED_DEVICE_NAMES
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(stem))
        {
            return Err(CliError::new(
                Code::InvalidProjectName,
                format!(
                    "`{name}` is a reserved device name on Windows and cannot be used as a \
                     directory name; a project created with it here would be unusable on a \
                     Windows checkout"
                ),
            )
            .with("rule", "reserved_device_name")
            .with("name", name.to_owned()));
        }

        // Rule 5 — the parent exists and canonicalises. Canonicalisation is what turns a symlinked
        // parent into the real directory the rename will actually target.
        let requested_parent = requested.parent().filter(|parent| !parent.as_os_str().is_empty());
        let parent_input = match requested_parent {
            Some(parent) => parent.to_path_buf(),
            None => std::env::current_dir().map_err(|error| {
                CliError::new(
                    Code::DestinationParentMissing,
                    format!("the current directory could not be resolved: {error}"),
                )
            })?,
        };
        let parent = parent_input.canonicalize().map_err(|error| {
            CliError::new(
                Code::DestinationParentMissing,
                format!(
                    "the destination's parent directory `{}` does not exist or cannot be \
                     resolved: {error}",
                    parent_input.display()
                ),
            )
            .with("rule", "parent_resolves")
            .with("parent", parent_input.display().to_string())
        })?;
        if !parent.is_dir() {
            return Err(rejected(
                "parent_resolves",
                "the destination's parent is not a directory",
            ));
        }

        let full = parent.join(name);

        // Rule 7 — the destination is not a symbolic link. Checked with `symlink_metadata`, which
        // does NOT follow the link; `metadata` would report the target and defeat the whole check.
        if let Ok(metadata) = std::fs::symlink_metadata(&full)
            && metadata.file_type().is_symlink()
        {
            return Err(rejected(
                "not_a_symlink",
                "the destination is a symbolic link; writing through it would create the project \
                 somewhere other than where you asked",
            ));
        }

        // Rule 6 — the destination does not exist, or exists and is empty.
        match std::fs::read_dir(&full) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    return Err(CliError::new(
                        Code::DestinationNotEmpty,
                        format!(
                            "`{}` already exists and is not empty; merging into an existing \
                             project is not supported",
                            full.display()
                        )
                        .to_string(),
                    )
                    .with("rule", "destination_absent_or_empty")
                    .with("destination", full.display().to_string()));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                // Exists but is not a readable directory — a regular file, most likely.
                if full.exists() {
                    return Err(rejected(
                        "destination_absent_or_empty",
                        "the destination exists and is not a directory",
                    ));
                }
            }
        }

        // Rule 2 and Rule 8 together — the resolved destination is inside the resolved parent.
        //
        // This is the check that catches an absolute path smuggled in as a "name" and a symlinked
        // ancestor, and it is done on CANONICAL paths so that it reasons about where writes will
        // actually land rather than about the text the user typed.
        if !full.starts_with(&parent) {
            return Err(rejected(
                "contained_by_parent",
                "the destination resolves outside the directory that contains it",
            ));
        }

        Ok(Self { parent, name: name.to_owned(), full })
    }
}

/// Rejects a project name before it becomes a path.
///
/// Separate from [`DestinationPath::validate`] because the name is also written into `renvor.toml`
/// and used as a Rust package name, so it has constraints a directory name does not.
///
/// # Errors
///
/// [`Code::InvalidProjectName`], naming the constraint that failed.
pub fn validate_project_name(name: &str) -> Result<(), CliError> {
    let invalid = |why: &str| {
        CliError::new(Code::InvalidProjectName, why.to_owned()).with("name", name.to_owned())
    };

    if name.is_empty() {
        return Err(invalid("a project name cannot be empty"));
    }
    if name.len() > 64 {
        return Err(invalid(
            "a project name is limited to 64 characters, so that it remains usable as a directory \
             name and a package name on every supported platform",
        ));
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err(invalid(
            "a project name may contain only ASCII letters, digits, `-`, and `_`, because it \
             becomes both a directory name and a Rust package name",
        ));
    }
    if !name
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
    {
        return Err(invalid("a project name must start with an ASCII letter"));
    }
    if RESERVED_DEVICE_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
    {
        return Err(invalid(
            "this is a reserved device name on Windows and cannot be used as a directory name",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reserved_device_name_is_refused_on_every_platform() {
        // Deliberately not `#[cfg(windows)]`. A project generated on Linux gets checked out on
        // Windows, and finding out then is worse.
        for name in ["con", "CON", "Nul", "com1", "LPT9"] {
            assert!(
                validate_project_name(name).is_err(),
                "{name} must be refused"
            );
        }
    }

    #[test]
    fn ordinary_names_are_accepted() {
        // POSITIVE CONTROL. Every assertion above is a refusal, and a validator that refused
        // everything would satisfy all of them.
        for name in ["commerce", "my-app", "app_2", "a"] {
            assert!(
                validate_project_name(name).is_ok(),
                "{name} must be accepted, or the refusals above prove nothing"
            );
        }
    }

    #[test]
    fn names_that_are_not_usable_as_package_names_are_refused() {
        for name in ["", "1app", "-app", "my app", "my/app", "app!", &"x".repeat(65)] {
            assert!(
                validate_project_name(name).is_err(),
                "{name:?} must be refused"
            );
        }
    }

    #[test]
    fn traversal_is_refused_structurally() {
        let error = DestinationPath::validate(Path::new("../escape")).unwrap_err();
        assert_eq!(error.code, Code::DestinationRejected);
        assert!(error.details.iter().any(|(key, value)| key == "rule" && value == "no_traversal"));
    }

    #[test]
    fn a_missing_parent_is_named_as_such() {
        let error =
            DestinationPath::validate(Path::new("/definitely/not/here/at/all/x")).unwrap_err();
        assert_eq!(error.code, Code::DestinationParentMissing);
    }
}
