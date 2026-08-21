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
//! remembering to look. [Phase 003 research §D6](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/research.md)
//! records the measurements that reversed the decision.
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
const RESERVED_DEVICE_NAMES: [&str; 28] = [
    "CON",
    "PRN",
    "AUX",
    "NUL",
    "COM1",
    "COM2",
    "COM3",
    "COM4",
    "COM5",
    "COM6",
    "COM7",
    "COM8",
    "COM9",
    "LPT1",
    "LPT2",
    "LPT3",
    "LPT4",
    "LPT5",
    "LPT6",
    "LPT7",
    "LPT8",
    "LPT9",
    // Microsoft's "Naming Files, Paths, and Namespaces" lists the superscript spellings alongside
    // the ASCII ones: `COM¹`, `COM²`, `COM³`, `LPT¹`, `LPT²`, `LPT³` alias the same devices, so
    // `echo test > COM\u{B9}` fails on Windows exactly as `> COM1` does. Written as escapes rather
    // than as literal characters so a reviewer can see which code point each one is, and so they
    // survive a copy through a terminal or an editor that normalises them.
    "COM\u{B9}",
    "COM\u{B2}",
    "COM\u{B3}",
    "LPT\u{B9}",
    "LPT\u{B2}",
    "LPT\u{B3}",
];

/// # Invariant I-17 — time-of-check to time-of-use is **narrowed, not eliminated**
///
/// Between [`Destination::open`] returning a handle and `Staging::place` performing the rename,
/// another process with write access to that directory can still create the destination. Nothing in
/// this module prevents that, and this invariant does not claim it does.
///
/// What the design gives instead is two properties, both weaker than "impossible" and both worth
/// having:
///
/// 1. **The window is narrower than a path-based design's.** Every check and the rename itself go
///    through **one open handle** rather than being re-resolved from a string each time. A design
///    that re-`stat`s a path and then renames by path resolves the name twice, and an attacker who
///    wins between those two resolutions gets a different directory. Here there is one resolution
///    and it happened at `open`.
/// 2. **Losing the race is a clean failure, never an overwrite.** The rename refuses an existing
///    destination — see `Staging::place`, which classifies from the kernel's own `ErrorKind` rather
///    than by re-observing the filesystem, because a re-observation is itself racy and is exactly
///    how a lost race used to be misreported as `placement_failed`.
///
/// # The one case the rename itself can still replace, stated plainly
///
/// POSIX `rename(2)` **silently replaces an existing empty destination directory**. So if another
/// process creates an empty directory at the destination in the window between `Staging::place`'s
/// own absence check and the rename, that directory is replaced rather than preserved. This is a
/// property of the system call, not a decision this program makes, and it is the *entire* residual
/// after the 2026-08-18 ruling: renvor no longer deletes or replaces a destination deliberately
/// anywhere.
///
/// Closing the window entirely needs an atomic create-directory-or-fail rename. POSIX does not
/// provide one — `renameat2(RENAME_NOREPLACE)` is Linux-only — and the obvious portable
/// substitute, `create_dir` the destination first and rename onto it, is not portable either:
/// Windows `MoveFileEx` refuses a rename onto an existing directory, so the trick that closes the
/// window on Unix would break the command on Windows. The residual is therefore stated rather than
/// designed around, and `tests/transaction.rs` asserts the *consequence* — that concurrent runs
/// produce exactly one project and clean failures for the rest — rather than asserting an
/// impossibility that does not hold.
///
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
    /// [`Code::DestinationParentMissing`], or [`Code::DestinationExists`]. Every rejection names
    /// the rule that fired in `details.rule`, so a refusal is a diagnosis rather than a verdict.
    pub fn open(requested: &Path) -> Result<Self, CliError> {
        let rejected = |rule: &'static str, why: &str| {
            CliError::new(Code::DestinationRejected, why.to_owned())
                .with("rule", rule)
                .with("destination", requested.display().to_string())
        };

        // RULE 0 — NO TRAVERSAL COMPONENT ANYWHERE IN THE REQUESTED PATH.
        //
        // ── THIS RULE WAS DROPPED AND RESTORED, AND THE REASON IS WORTH KEEPING ────────────
        //
        // The first cap-std version of this module removed it, reasoning that the operator typed
        // the parent path so honouring `..` in it is correct. That reasoning is defensible for a
        // shell and wrong here: FR-039 and SC-009 require traversal to be **refused**, and
        // `renvor new --path ../escape` was accepted with exit 0 until an outside-in acceptance
        // test caught it.
        //
        // The unit tests missed it because they only covered a **trailing** `..`. The rule is
        // about `..` anywhere, and those are not the same check.
        //
        // Note this is a *policy* rule rather than a containment one — containment below the
        // destination is structural, via the `Dir` handle. This exists so the destination itself
        // cannot land somewhere the operator did not mean.
        if requested
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(rejected(
                "no_traversal",
                "the destination contains a `..` component, which could place the project outside \
                 the directory you named; give the path you mean without `..`",
            ));
        }

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

        // RULE 1b — the directory name renvor is about to CREATE is hygienic.
        //
        // ── FOUND BY AN ADVISORY SECURITY REVIEW, 2026-08-18 ──────────────────────────────
        //
        // `validate_project_name` refuses control characters and odd punctuation — but it guards the
        // **project name**, and when the operator supplies a NAME separately the directory name comes
        // from `--path`'s final component instead and reached no such check. Measured, not reasoned:
        //
        //   renvor new x --path '/tmp/out/weird!@#name'   -> exit 0, directory `weird!@#name`
        //   renvor new x --path $'/tmp/out/inject\nLINE'  -> exit 0, directory with a literal newline
        //   renvor new x --path '/tmp/out/trailing. '     -> exit 0, directory `trailing. `
        //
        // while the same strings in the NAME position were correctly refused. The module's own doc
        // comment says a project name "becomes both a directory name and a Rust package name", and
        // that was true of one of the two ways to supply it.
        //
        // ── WHY THIS IS NARROWER THAN `validate_project_name` ────────────────────────────
        //
        // It rejects **control characters** and a **trailing dot or space**, and nothing else. The
        // full ASCII-alphanumeric rule is right for a package name and wrong here: an operator may
        // legitimately write `--path ./my.project` or a path with a space in it, and refusing that
        // would break ordinary use to prevent nothing. What these two classes have in common is that
        // they make the name **lie**:
        //
        //   - a control character can rewrite a terminal line or forge a log entry, so the directory
        //     an operator is told about is not the one on disk;
        //   - Windows silently strips a trailing dot or space, so `trailing. ` is created as
        //     `trailing` — and this program refuses reserved device names on every platform for
        //     exactly the same reason: a tree made on one platform is opened on another.
        if let Some(offending) = name.chars().find(|c| c.is_control()) {
            return Err(rejected(
                "name_control_character",
                &format!(
                    "the destination's final component contains a control character \
                     (U+{:04X}); a directory name that can rewrite a terminal line or a log entry \
                     is refused",
                    offending as u32
                ),
            ));
        }
        if name.ends_with('.') || name.ends_with(' ') {
            return Err(rejected(
                "name_trailing_dot_or_space",
                "the destination's final component ends in a `.` or a space; Windows strips those \
                 silently, so the directory created there would not be the one you named",
            ));
        }

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
                    crate::output::redact::path(&parent_input)
                ),
            )
            .with("rule", "parent_opens")
            .with("parent", parent_input.display().to_string())
            })?;

        // RULE 4 — THE DESTINATION MUST BE ABSENT. Every existing entry is refused, and an entry
        // whose state cannot be established is refused too.
        //
        // ── THIS RULE WAS TIGHTENED BY MAINTAINER RULING, 2026-08-18 ──────────────────────
        //
        // The previous rule accepted an existing **empty** directory, and `Staging::place` then
        // deleted it and let the rename create a fresh one. Three things were wrong with that, and
        // all three were found by advisory review rather than by a test:
        //
        //   - **It replaced a path the operator owned.** The new directory has a different inode,
        //     the process's own umask-derived mode, and the process's ownership. An operator who
        //     had `mkdir -m 0700 out` got `0755` back, silently (A-R8).
        //   - **The restore-after-failed-rename branch was unreachable from any test**, and its
        //     one recovery action ignored its own error (A-R9).
        //   - It made `renvor new` a program that can delete a directory. It no longer is.
        //
        // The replacement is not a softer check; it is a *broader* one. Empty directory, non-empty
        // directory, regular file, symbolic link, and anything else all fail here, before a single
        // byte is staged.
        //
        // ── WHY `symlink_metadata` AND WHY IT IS THE ONLY CALL ───────────────────────────
        //
        // It reports **any** entry, and it does **not follow a symbolic link** — `metadata` would
        // report the link's target and would answer `NotFound` for a dangling link, which is the
        // one form of "existing entry" most worth catching. One call replaces the old pair, so
        // there is no longer a second observation whose disagreement with the first has to be
        // reasoned about.
        //
        // ── FAIL CLOSED, AND WHY THE OLD CODE DID NOT ────────────────────────────────────
        //
        // The old `Err(_)` arm asked `symlink_metadata` a second question and, **if that also
        // failed, fell through to `Ok`** — so a destination whose state could not be read at all
        // was treated as absent, and generation proceeded. Any error that is not an authoritative
        // `NotFound` now refuses, and the original OS error is carried into the message and into
        // `details.error` rather than being discarded. An unknown filesystem state is not absence.
        match parent.symlink_metadata(&name) {
            // The ONLY arm that proceeds.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(metadata) => {
                let found = describe(&metadata);
                return Err(CliError::new(
                    Code::DestinationExists,
                    format!(
                        "`{}` already exists (it is a {found}); renvor generates into a path that \
                         does not exist yet, and will not delete, replace, or merge into one you \
                         already have. Choose a different destination, or remove that one yourself",
                        crate::output::redact::path(&parent_input.join(&name))
                    ),
                )
                .with("rule", "destination_absent")
                .with("found", found)
                .with(
                    "destination",
                    parent_input.join(&name).display().to_string(),
                ));
            }
            Err(error) => {
                // FAIL CLOSED. Reported as `destination_rejected` rather than as
                // `destination_exists`, because claiming something is there would be as
                // unevidenced as claiming it is not. What is known is that renvor could not tell,
                // and that is what the message and `details.error` say.
                return Err(CliError::new(
                    Code::DestinationRejected,
                    format!(
                        "`{}` could not be inspected, so renvor cannot establish that it is \
                         absent: {error}. Generation is refused rather than risking writing over \
                         something that is there",
                        crate::output::redact::path(&parent_input.join(&name))
                    ),
                )
                .with("rule", "destination_unverifiable")
                .with("error", error.to_string())
                .with(
                    "destination",
                    parent_input.join(&name).display().to_string(),
                ));
            }
        }

        Ok(Self {
            parent,
            parent_display: parent_input,
            name,
        })
    }
}

/// Names what is sitting at a destination, in the vocabulary contract C-2 publishes for
/// `destination_exists`'s `details.found`.
///
/// Shared with [`crate::generate::place`] rather than written twice, because the two sites report
/// the **same** refusal at two different moments — `Destination::open` before anything is staged,
/// and `Staging::place` immediately before the rename — and a consumer that had to handle two
/// vocabularies for one code would be handling an accident of where the check ran.
///
/// # Why the symlink test comes first, separating what is measured from what is reasoned
///
/// **Measured, on Unix**: `symlink_metadata` reports the link itself, so a symbolic link to a
/// directory answers `is_symlink() == true` and `is_dir() == false`, and the order would not matter
/// there. `every_kind_of_existing_destination_is_refused` asserts `found == "symlink"` for both a
/// live and a dangling link, so this is checked rather than assumed.
///
/// **Reasoned, on Windows, and not measured here**: a directory symbolic link is a reparse point
/// that also carries the directory attribute, so `is_dir()` may well be true for it. If the
/// directory test came first, the same entry would be reported as `directory` on one platform and
/// `symlink` on another — a difference an operator would have no way to account for.
///
/// Ordering the symlink test first makes the answer the same everywhere and keeps the more useful
/// half: that the entry is a link. The Windows half is stated as reasoning, not as a measurement,
/// because no test in this repository observes it.
pub fn describe(metadata: &cap_std::fs::Metadata) -> &'static str {
    if metadata.is_symlink() {
        "symlink"
    } else if metadata.is_dir() {
        "directory"
    } else if metadata.is_file() {
        "file"
    } else {
        "other"
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
    // EVERY refusal names the rule that fired, the same way `Destination::open` does.
    //
    // It did not until 2026-08-18: all five refusals below shared the code `invalid_project_name`
    // and carried no `details.rule` at all, so a consumer could tell that a name was rejected and
    // not which of five unrelated reasons applied. FR-014 and T049 ask for the rule, and
    // `Destination::open` had it while this function — reached by the same command — did not.
    let invalid = |rule: &'static str, why: &str| {
        CliError::new(Code::InvalidProjectName, why.to_owned())
            .with("name", name.to_owned())
            .with("rule", rule)
    };

    if name.is_empty() {
        return Err(invalid("name_not_empty", "a project name cannot be empty"));
    }
    if name.len() > 64 {
        return Err(invalid(
            "name_length",
            "a project name is limited to 64 characters, so that it remains usable as a directory \
             name and a package name on every supported platform",
        ));
    }
    // ABSOLUTE-PATH INJECTION AND SEPARATORS, CHECKED BEFORE THE GENERAL CHARACTER SET.
    //
    // This is data-model §5 rule 2 — "no absolute path where relative is expected" — and until
    // now it had no implementation of its own. `renvor new /etc/renvor-injected` *was* refused,
    // which is why no test caught it, but it was refused by the character-set rule below as though
    // `/` were an unusual punctuation choice. The diagnosis matters: a name is one path component,
    // and somebody who typed a path into the name position needs to be told that, not told their
    // punctuation is unsupported.
    if name.contains('/') || name.contains('\\') || name.contains(':') {
        return Err(invalid(
            "single_path_component",
            "a project name is a single directory name, not a path; it may not contain `/`, `\\`, \
             or `:`. To choose where the project goes, use `--path`",
        ));
    }
    // RESERVED DEVICE NAMES, MATCHED ON THE STEM, AND CHECKED BEFORE THE CHARACTER SET.
    //
    // Windows resolves `CON.txt` to the console device, so the extension does not make the name
    // safe — which is why this matches the part before the first `.`, exactly as the
    // destination-side check in `Destination::open` does.
    //
    // It runs before the character-set rule because `.` is not in the allowed set, so a
    // character-set check placed first answers `CON.txt` with "unsupported punctuation". That is
    // true and useless; the operator's actual problem is that the name is a device.
    let stem = name.split('.').next().unwrap_or(name);
    if RESERVED_DEVICE_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
    {
        return Err(invalid(
            "reserved_device_name",
            "this is a reserved device name on Windows and cannot be used as a directory name, \
             with or without an extension",
        ));
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-' || character == '_')
    {
        return Err(invalid(
            "name_character_set",
            "a project name may contain only ASCII letters, digits, `-`, and `_`, because it \
             becomes both a directory name and a Rust package name",
        ));
    }
    if !name
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
    {
        return Err(invalid(
            "name_starts_with_letter",
            "a project name must start with an ASCII letter",
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
    fn a_reserved_device_name_is_refused_as_a_destination_directory() {
        // RULE 2 lives in `Destination::open`, and **nothing drove it with a reserved name**.
        //
        // That gap hid a real defect for the whole phase. In the NAME position `COM\u{B9}` is
        // refused by the character-set rule — superscripts are not ASCII alphanumerics — so a
        // corpus that only calls `validate_project_name` passes whether or not the reserved list
        // contains it. `renvor new demo --path COM\u{B9}` therefore *succeeded*, creating a
        // directory Windows cannot open, while every reserved-name test in the suite was green.
        //
        // Asserting the RULE and not merely the refusal is what makes this test able to fail: it
        // is the same lesson `tests/hostile.rs` records in its header about the `..` rule.
        let base = tempfile::tempdir().expect("tempdir");
        for name in ["CON", "com1", "LPT9", "COM\u{B9}", "com\u{B2}", "LPT\u{B3}"] {
            let error = Destination::open(&base.path().join(name))
                .expect_err("a reserved device name must be refused as a destination");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(key, value)| key == "rule" && value == "reserved_device_name"),
                "`{name}` was refused for the wrong reason: {:?}",
                error.details
            );
        }

        // POSITIVE CONTROL. Every assertion above is a refusal, and a validator that refused
        // everything would satisfy all of them.
        Destination::open(&base.path().join("ordinary"))
            .expect("an ordinary destination name is still accepted");
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
    fn a_traversal_component_anywhere_is_refused() {
        // THE REGRESSION TEST. `../escape` was accepted with exit 0 by the first cap-std version
        // of this module, because the only traversal test covered a path *ending* in `..` — and a
        // trailing `..` is caught by a different rule, so it kept passing while the real one was
        // gone. A test that passes for the wrong reason is worse than no test.
        for case in ["../escape", "a/../b", "../../x", "./../y", "some/where/.."] {
            let error = match Destination::open(Path::new(case)) {
                Err(error) => error,
                Ok(accepted) => panic!("`{case}` was accepted as `{}`", accepted.name()),
            };
            assert_eq!(error.code, Code::DestinationRejected, "{case}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(key, value)| key == "rule" && value == "no_traversal"),
                "`{case}` was refused by a different rule than the one meant to catch it: {:?}",
                error.details
            );
        }
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

    /// The `details.rule` a refusal named, or `None`.
    fn rule(error: &CliError) -> Option<&str> {
        error
            .details
            .iter()
            .find(|(key, _)| key == "rule")
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn every_kind_of_existing_destination_is_refused() {
        // THE MAINTAINER RULING OF 2026-08-18, ASSERTED OVER ITS WHOLE SURFACE.
        //
        // The ruling enumerates five classes, and this enumerates the same five. Before it, the
        // first of them — an existing **empty** directory — was ACCEPTED, and `Staging::place`
        // then deleted it. So this test fails against the previous implementation on its very
        // first case, which is what makes it a test of the change rather than a description of it.
        let base = tempfile::tempdir().expect("tempdir");

        let empty = base.path().join("empty");
        std::fs::create_dir(&empty).expect("mkdir");

        let full = base.path().join("full");
        std::fs::create_dir(&full).expect("mkdir");
        std::fs::write(full.join("f"), b"x").expect("write");

        let file = base.path().join("file");
        std::fs::write(&file, b"x").expect("write");

        let mut cases = vec![(empty, "directory"), (full, "directory"), (file, "file")];

        #[cfg(unix)]
        {
            // A symbolic link to a directory, and a DANGLING one. The dangling case is the reason
            // this check uses `symlink_metadata`: `metadata` follows the link and would answer
            // `NotFound`, so a dangling link would read as an absent destination and generation
            // would proceed onto a name that is already taken.
            let elsewhere = base.path().join("elsewhere");
            std::fs::create_dir(&elsewhere).expect("mkdir");
            let link = base.path().join("link");
            std::os::unix::fs::symlink(&elsewhere, &link).expect("symlink");
            let dangling = base.path().join("dangling");
            std::os::unix::fs::symlink(base.path().join("nothing-here"), &dangling)
                .expect("symlink");
            cases.push((link, "symlink"));
            cases.push((dangling, "symlink"));

            // A UNIX-DOMAIN SOCKET — neither directory, file, nor link.
            //
            // Added after probing the shipped binary against a FIFO and a socket: both are refused
            // as `found = "other"`, which is the contract's fifth value and had no test. Without
            // this case the `other` arm of `describe` is reachable from nothing, and an arm no test
            // reaches is an arm that can be wrong for as long as nobody looks.
            //
            // `UnixListener` is `std`, so this needs no dependency. A FIFO would need `mkfifo`,
            // which does, so the socket is the one that earns its place.
            //
            // `bind` fails if the path exceeds ~104 bytes, which a temporary directory can. That is
            // a limit of the test's fixture and not of the code, so the case is **skipped** rather
            // than failed — and skipped by simply not being pushed, rather than by an assertion
            // that quietly always holds.
            let socket = base.path().join("s");
            if std::os::unix::net::UnixListener::bind(&socket).is_ok() {
                cases.push((socket, "other"));
            }
        }

        for (path, expected) in cases {
            let error = match Destination::open(&path) {
                Err(error) => error,
                Ok(_) => panic!("`{}` was accepted", path.display()),
            };
            assert_eq!(
                error.code,
                Code::DestinationExists,
                "`{}` was refused with the wrong code",
                path.display()
            );
            assert_eq!(
                rule(&error),
                Some("destination_absent"),
                "{}",
                path.display()
            );
            assert!(
                error
                    .details
                    .iter()
                    .any(|(key, value)| key == "found" && value == expected),
                "`{}` should have reported found={expected}: {:?}",
                path.display(),
                error.details
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn a_destination_whose_state_cannot_be_established_fails_closed() {
        // THE FAIL-CLOSED RULE, WITH A REAL UNREADABLE PARENT RATHER THAN A SIMULATION.
        //
        // The old code asked `read_dir` first and, when that failed, asked `symlink_metadata` — and
        // **if that failed too it fell through to `Ok`**, treating "I could not tell" as "it is not
        // there". A parent directory with no execute permission produces exactly that: every
        // lookup inside it returns `PermissionDenied`, never `NotFound`.
        //
        use std::os::unix::fs::PermissionsExt as _;

        let base = tempfile::tempdir().expect("tempdir");
        let opaque = base.path().join("opaque");
        std::fs::create_dir(&opaque).expect("mkdir");
        // `rw-` — readable, and **not traversable**. A lookup inside answers `PermissionDenied`,
        // which is the "not NotFound" case the fall-through mishandled.
        std::fs::set_permissions(&opaque, std::fs::Permissions::from_mode(0o600))
            .expect("chmod 600");

        // Root ignores the permission bits, and so do some filesystems and container mounts. Probe
        // rather than guess: if the denial did not take effect, this test can assert nothing, and
        // pretending otherwise would make it a test that passes without checking anything.
        let denied = matches!(
            std::fs::symlink_metadata(opaque.join("demo")),
            Err(ref error) if error.kind() != std::io::ErrorKind::NotFound
        );
        if !denied {
            std::fs::set_permissions(&opaque, std::fs::Permissions::from_mode(0o700))
                .expect("chmod");
            return;
        }

        let outcome = Destination::open(&opaque.join("demo"));

        // Restore before asserting, so a failing assertion still leaves a removable tempdir.
        std::fs::set_permissions(&opaque, std::fs::Permissions::from_mode(0o700)).expect("chmod");

        let error = match outcome {
            Err(error) => error,
            Ok(_) => panic!("an unreadable destination was treated as absent"),
        };
        // Either refusal is correct and both fail closed: on some platforms the parent cannot even
        // be OPENED, which RULE 3 catches first. What must never happen is `Ok`.
        assert!(
            matches!(
                error.code,
                Code::DestinationRejected | Code::DestinationParentMissing
            ),
            "unexpected code {}: {}",
            error.code,
            error.message
        );
        if error.code == Code::DestinationRejected {
            assert_eq!(rule(&error), Some("destination_unverifiable"));
            assert!(
                error.details.iter().any(|(key, _)| key == "error"),
                "the original OS error must be preserved, not discarded: {:?}",
                error.details
            );
        }
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
