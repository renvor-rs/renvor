//! The destination boundary.
//!
//! # This boundary is structural, not checked — and that is a change from the first draft
//!
//! An earlier version of this module composed `std::fs::canonicalize` with hand-written component
//! validation, and carried a long comment admitting that escape was only *checked*: a code path
//! that forgot to call the validator was unprotected. That version needed an accepted decision
//! record before it could merge, because constitution principle III and FR-045 make custom
//! infrastructure conditional on justifying it over a maintained package.
//!
//! It is gone. Everything the generator writes goes through a [`cap_std::fs::Dir`] handle, and
//! inside a handle there is **no ambient path API to escape with**. Traversal, absolute-path
//! injection, and symlinks that leave the tree are refused by `cap-std`, not by this program
//! remembering to look. `research.md` D6 records the measurements that reversed the decision.
//!
//! The practical difference, stated so nobody has to take it on faith: the old boundary rejected a
//! symlink **at** the destination and would have followed a symlink **inside** the rendered tree.
//! A `Dir` handle refuses both, because it refuses any resolution step that leaves.
//!
//! # The one ambient call, named on purpose
//!
//! [`Destination::open`] calls `Dir::open_ambient_dir` on the destination's **parent**. That is the
//! single point where authority enters this program, and it is deliberate: the operator typed that
//! path, and honouring `..` in it is correct — they asked for it.
//!
//! Everything below that point is confined. So the trust statement is exact: **renvor trusts the
//! path the operator typed, and nothing else** — not a template's output path, not a project name,
//! not a symlink it encounters while walking.
//!
//! # What is still checked rather than structural
//!
//! Two rules cannot come from a capability, because they are about *names*, not about resolution:
//! Windows reserved device names, and the characters that make a string usable as a Rust package
//! name. Both are below, and both are ordinary validation.

use std::path::{Component, Path, PathBuf};

use cap_std::ambient_authority;
use cap_std::fs::Dir;

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

/// An opened parent directory plus the single name the project will occupy inside it.
///
/// The `Dir` is the capability. Holding one of these *is* the authority to write the project, and
/// there is no constructor that produces one without passing every rule in [`Destination::open`].
pub struct Destination {
    parent: Dir,
    parent_display: PathBuf,
    name: String,
}

impl std::fmt::Debug for Destination {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `Dir` formats as a raw file descriptor, which is noise in a log and meaningless to a
        // reader. The path and name are the useful half.
        formatter
            .debug_struct("Destination")
            .field("parent", &self.parent_display)
            .field("name", &self.name)
            .finish()
    }
}

impl Destination {
    /// The capability every write goes through.
    #[must_use]
    pub fn parent(&self) -> &Dir {
        &self.parent
    }

    /// The final path component the project will occupy.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The parent directory as the operator named it, **for messages and for process execution
    /// only**. Not a capability; nothing in this crate opens it.
    #[must_use]
    pub fn parent_display(&self) -> &Path {
        &self.parent_display
    }

    /// The full path, **for messages only**.
    ///
    /// Deliberately not usable as a capability: nothing in this crate opens it. It exists so an
    /// error can say where the project would have gone.
    #[must_use]
    pub fn display_path(&self) -> PathBuf {
        self.parent_display.join(&self.name)
    }

    /// Applies every rule and opens the parent, refusing before anything is created.
    ///
    /// # Errors
    ///
    /// [`Code::InvalidProjectName`], [`Code::DestinationRejected`],
    /// [`Code::DestinationParentMissing`], or [`Code::DestinationNotEmpty`]. Every rejection names
    /// the rule that fired in `details.rule`, so a refusal is a diagnosis rather than a verdict.
    pub fn open(requested: &Path) -> Result<Self, CliError> {
        let rejected = |rule: &'static str, why: &str| {
            CliError::new(Code::DestinationRejected, why.to_owned())
                .with("rule", rule)
                .with("destination", requested.display().to_string())
        };

        // RULE 1 — the final component is a single ordinary name.
        //
        // Checked structurally with `Component`, not by searching the string: a textual `..` test
        // misses platform-specific spellings, and `Component::ParentDir` does not.
        let Some(last) = requested.components().next_back() else {
            return Err(rejected(
                "final_component_is_a_name",
                "the destination has no final path component to use as the project directory",
            ));
        };
        let name = match last {
            Component::Normal(name) => match name.to_str() {
                Some(name) => name.to_owned(),
                None => {
                    return Err(rejected(
                        "final_component_is_a_name",
                        "the destination's final component is not valid UTF-8",
                    ));
                }
            },
            Component::ParentDir | Component::CurDir => {
                return Err(rejected(
                    "final_component_is_a_name",
                    "the destination ends in `.` or `..`, which is not a usable directory name",
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(rejected(
                    "final_component_is_a_name",
                    "the destination is a filesystem root, not a directory that can be created",
                ));
            }
        };

        // RULE 2 — not a platform-reserved device name. A name, not a path, so no capability can
        // decide it.
        let stem = name.split('.').next().unwrap_or(&name);
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
            .with("name", name.clone()));
        }

        // RULE 3 — the parent opens. THIS IS THE ONE AMBIENT CALL IN THE PROGRAM.
        //
        // From here on nothing takes a path from outside; every operation is relative to `parent`.
        let parent_input = match requested.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(parent) => parent.to_path_buf(),
            None => PathBuf::from("."),
        };
        let parent =
            Dir::open_ambient_dir(&parent_input, ambient_authority()).map_err(|error| {
                CliError::new(
                Code::DestinationParentMissing,
                format!(
                    "the destination's parent directory `{}` does not exist or cannot be opened: \
                     {error}",
                    parent_input.display()
                ),
            )
            .with("rule", "parent_opens")
            .with("parent", parent_input.display().to_string())
            })?;

        // RULE 4 — the destination is not a symbolic link.
        //
        // `symlink_metadata` does NOT follow the link; `metadata` would report the target and
        // defeat the check entirely. Note that this is now belt-and-braces: writing *through* the
        // link would be refused by the handle anyway. It is kept because a clear message beats an
        // opaque `PermissionDenied` three steps later.
        if let Ok(metadata) = parent.symlink_metadata(&name)
            && metadata.is_symlink()
        {
            return Err(rejected(
                "not_a_symlink",
                "the destination is a symbolic link; renvor will not write through it",
            ));
        }

        // RULE 5 — the destination is absent, or present and empty.
        match parent.read_dir(&name) {
            Ok(mut entries) => {
                if entries.next().is_some() {
                    return Err(CliError::new(
                        Code::DestinationNotEmpty,
                        format!(
                            "`{}` already exists and is not empty; merging into an existing \
                             project is not supported",
                            parent_input.join(&name).display()
                        ),
                    )
                    .with("rule", "destination_absent_or_empty")
                    .with(
                        "destination",
                        parent_input.join(&name).display().to_string(),
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                // Present, and not a readable directory — a regular file, most likely.
                if parent.symlink_metadata(&name).is_ok() {
                    return Err(rejected(
                        "destination_absent_or_empty",
                        "the destination exists and is not a directory",
                    ));
                }
            }
        }

        Ok(Self {
            parent,
            parent_display: parent_input,
            name,
        })
    }
}

/// Rejects a project name before it becomes a path.
///
/// Separate from [`Destination::open`] because the name is also written into `renvor.toml` and used
/// as a Rust package name, so it has constraints a directory name does not.
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
        for name in [
            "",
            "1app",
            "-app",
            "my app",
            "my/app",
            "app!",
            &"x".repeat(65),
        ] {
            assert!(
                validate_project_name(name).is_err(),
                "{name:?} must be refused"
            );
        }
    }

    #[test]
    fn a_destination_ending_in_a_traversal_component_is_refused() {
        let error = Destination::open(Path::new("some/where/..")).unwrap_err();
        assert_eq!(error.code, Code::DestinationRejected);
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "rule" && value == "final_component_is_a_name")
        );
    }

    #[test]
    fn a_missing_parent_is_named_as_such() {
        let error = Destination::open(Path::new("/definitely/not/here/at/all/x")).unwrap_err();
        assert_eq!(error.code, Code::DestinationParentMissing);
    }

    #[test]
    fn an_ordinary_destination_opens() {
        // POSITIVE CONTROL for every refusal above.
        let base = tempfile::tempdir().expect("tempdir");
        let destination = Destination::open(&base.path().join("demo")).expect("opens");
        assert_eq!(destination.name(), "demo");
    }

    #[test]
    fn a_non_empty_destination_is_refused() {
        let base = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(base.path().join("demo")).expect("mkdir");
        std::fs::write(base.path().join("demo/f"), b"x").expect("write");
        let error = Destination::open(&base.path().join("demo")).unwrap_err();
        assert_eq!(error.code, Code::DestinationNotEmpty);
    }

    #[test]
    #[cfg(unix)]
    fn a_symlinked_destination_is_refused_by_name_before_it_is_refused_by_the_handle() {
        let base = tempfile::tempdir().expect("tempdir");
        let elsewhere = base.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).expect("mkdir");
        std::os::unix::fs::symlink(&elsewhere, base.path().join("demo")).expect("symlink");
        let error = Destination::open(&base.path().join("demo")).unwrap_err();
        assert_eq!(error.code, Code::DestinationRejected);
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "rule" && value == "not_a_symlink")
        );
    }

    #[test]
    #[cfg(unix)]
    fn the_handle_refuses_an_escape_that_no_rule_in_this_module_checks_for() {
        // THIS IS THE TEST THAT JUSTIFIES THE DEPENDENCY.
        //
        // No rule above looks at a symlink *inside* the tree. The previous hand-written boundary
        // would have written straight through this one. The capability refuses it with no rule
        // written for it at all, which is the difference between structural and checked.
        let base = tempfile::tempdir().expect("tempdir");
        let outside = base.path().join("outside");
        std::fs::create_dir_all(&outside).expect("mkdir");
        let inside = base.path().join("inside");
        std::fs::create_dir_all(&inside).expect("mkdir");
        std::os::unix::fs::symlink(&outside, inside.join("link")).expect("symlink");

        let handle = Dir::open_ambient_dir(&inside, ambient_authority()).expect("opens");
        assert!(
            handle.write("link/planted", b"x").is_err(),
            "wrote through a symlink"
        );
        assert!(
            handle.write("../planted", b"x").is_err(),
            "wrote through a traversal"
        );
        assert!(
            !outside.join("planted").exists(),
            "a file escaped the boundary"
        );

        // POSITIVE CONTROL: the same handle must still be able to do its job.
        handle
            .write("legitimate", b"x")
            .expect("an ordinary write must still work");
    }
}
