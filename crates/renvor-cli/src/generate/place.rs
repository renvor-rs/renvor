//! Staging and placement — the two halves of the transaction that touch the destination.
//!
//! See `specs/003-interactive-cli/contracts/generation-transaction.md`.

use cap_std::fs::Dir;

use crate::exit::{CliError, Code};
use crate::paths::Destination;

/// A directory the process owns, beside the destination, that becomes the project.
///
/// # Why it lives in the destination's parent
///
/// Not the system temporary directory. FR-016 forbids falling back to a non-atomic copy when the
/// rename cannot be atomic, and on most Linux containers `/tmp` is a **different filesystem** from
/// the working tree — so staging there would make the forbidden fallback the ordinary case rather
/// than the exceptional one. A rule that fires on every run is a rule that gets deleted.
///
/// Staging in the parent makes the rename same-filesystem **by construction**. The cross-device
/// case is not handled; it is made unreachable.
///
/// # Residue
///
/// If the process is killed between construction and [`Staging::place`], the directory survives.
/// That is unavoidable without a supervisor and is specified rather than ignored: the name carries
/// `.renvor-staging-` and the process id, so residue is identifiable, and it is **beside** the
/// destination rather than inside it, so residue never becomes part of a project.
pub struct Staging<'a> {
    parent: &'a Dir,
    name: String,
    /// `Option` so it can be **closed before** the directory is renamed or removed.
    ///
    /// This is not stylistic. On Windows a directory with an open handle cannot be deleted, and
    /// may not be renamable either — so a `Drop` that removed the directory while still holding a
    /// handle on it would silently leave residue on one platform and work on the others. Taking
    /// the handle first is correct everywhere and costs nothing.
    ///
    /// `dir()` unwraps it, which is safe because the only two places that clear it — `place` and
    /// `drop` — both consume or end the value.
    handle: Option<Dir>,
    placed: bool,
}

impl std::fmt::Debug for Staging<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Staging")
            .field("name", &self.name)
            .field("placed", &self.placed)
            .finish()
    }
}

impl<'a> Staging<'a> {
    /// Creates the staging directory beside the destination and opens a handle on it.
    ///
    /// # Errors
    ///
    /// [`Code::PlacementFailed`] if the directory cannot be created — which usually means the
    /// parent is not writable, and is worth saying plainly rather than failing later.
    pub fn create(destination: &'a Destination) -> Result<Self, CliError> {
        let parent = destination.parent();
        let name = format!(
            ".renvor-staging-{}-{}",
            std::process::id(),
            // A monotonic-ish discriminator so two runs in one process never collide. Not
            // security-relevant; uniqueness within a parent directory is all that is needed.
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        );

        let failed = |error: std::io::Error| {
            CliError::new(
                Code::PlacementFailed,
                format!(
                    "a staging directory could not be created beside the destination in `{}`: \
                     {error}",
                    destination.display_path().display()
                ),
            )
        };

        parent.create_dir(&name).map_err(failed)?;
        let handle = parent.open_dir(&name).map_err(failed)?;
        Ok(Self {
            parent,
            name,
            handle: Some(handle),
            placed: false,
        })
    }

    /// The staging directory's name inside the parent.
    ///
    /// Needed only by [`crate::generate::verify`], which runs a subprocess and therefore cannot
    /// use the handle — see that module for why that step steps outside the boundary deliberately.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The capability every render and manifest operation goes through.
    ///
    /// Returning the `Dir` rather than a path is the whole point: a caller cannot accidentally
    /// perform an ambient operation with this, because it is not a path.
    #[must_use]
    pub fn dir(&self) -> &Dir {
        self.handle
            .as_ref()
            .expect("the staging handle is only cleared when the value is consumed or dropped")
    }

    /// Moves the staged tree to the destination with a single rename.
    ///
    /// # Atomicity, per platform rather than claimed uniformly
    ///
    /// On POSIX, `renameat(2)` onto a non-existent path within one directory is atomic. On Windows
    /// the nearest equivalent onto a non-existent path is used, and **POSIX-equivalent atomicity is
    /// not claimed**. The destination is guaranteed not to already exist (FR-013), which is what
    /// makes the weaker guarantee sufficient here.
    ///
    /// Both sides of the rename are the **same** `Dir` handle, so this cannot cross a filesystem
    /// boundary and cannot name anything outside it.
    ///
    /// # Errors
    ///
    /// [`Code::DestinationNotEmpty`] if the destination appeared while we were rendering — the
    /// concurrency case, converted into a clean failure rather than an overwrite — or
    /// [`Code::PlacementFailed`] for anything else. **The destination is unchanged either way.**
    pub fn place(mut self, destination: &Destination) -> Result<(), CliError> {
        let target = destination.name();

        // ── THE EXISTING-EMPTY-DESTINATION CASE ─────────────────────────────────────────
        //
        // FR-013 refuses a destination that "exists and is **not empty**", which means an existing
        // *empty* one must work. `Destination::open` accepts it — and until 2026-08-18 this method
        // refused it, so `renvor new existing-empty-dir` validated, rendered, ran the full
        // pre-placement verification, and then failed with a message that was **actively false**:
        // "appeared while the project was being generated", when it had been there all along.
        //
        // Two components disagreeing about what a valid destination is, with the disagreement only
        // observable seconds into a run.
        //
        // `remove_dir` is what makes the fix safe. **The kernel refuses to remove a non-empty
        // directory**, so there is no window in which this could delete somebody's files — the
        // emptiness check and the removal are one atomic operation rather than a check followed by
        // a hopeful delete. If the directory gained an entry since `open`, the removal fails and
        // the run stops with the destination untouched.
        match self.parent.symlink_metadata(target) {
            // Nothing there. The ordinary path.
            Err(_) => {}
            Ok(metadata) if metadata.is_dir() => {
                self.parent.remove_dir(target).map_err(|error| {
                    CliError::new(
                        Code::DestinationNotEmpty,
                        format!(
                            "`{}` exists and could not be replaced: {error}. Nothing was written \
                             there; if it has contents, renvor will not merge into it",
                            destination.display_path().display()
                        ),
                    )
                    .with(
                        "destination",
                        destination.display_path().display().to_string(),
                    )
                })?;
            }
            Ok(_) => {
                // A file, or a symlink that appeared after validation.
                return Err(CliError::new(
                    Code::DestinationNotEmpty,
                    format!(
                        "`{}` exists and is not a directory; nothing was written there and the \
                         staged tree has been removed",
                        destination.display_path().display()
                    ),
                )
                .with(
                    "destination",
                    destination.display_path().display().to_string(),
                ));
            }
        }

        self.parent
            .rename(&self.name, self.parent, target)
            .map_err(|error| {
                CliError::new(
                    Code::PlacementFailed,
                    format!(
                        "the generated project could not be moved into place atomically: {error}. \
                         Nothing was written to `{}`",
                        destination.display_path().display()
                    ),
                )
                .with(
                    "destination",
                    destination.display_path().display().to_string(),
                )
            })?;

        self.placed = true;
        Ok(())
    }
}

impl Drop for Staging<'_> {
    /// Removes the staging directory unless it was successfully placed.
    ///
    /// This is what makes "cancellation or failure leaves the destination unchanged" true on
    /// **every** path including a panic, rather than only on the paths somebody remembered to
    /// write a cleanup for.
    fn drop(&mut self) {
        // Close the handle first, unconditionally. On Windows an open handle blocks the removal
        // below, which would turn "cleanup on failure" into "residue on failure" on exactly one
        // platform — the kind of difference that shows up as a mysterious CI failure rather than
        // as a bug report.
        drop(self.handle.take());
        if !self.placed {
            let _ = self.parent.remove_dir_all(&self.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn destination(base: &Path, name: &str) -> Destination {
        Destination::open(&base.join(name)).expect("an ordinary destination")
    }

    #[test]
    fn staging_is_created_beside_the_destination_not_inside_it() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        let name = staging.name.clone();
        assert!(base.path().join(&name).is_dir(), "staging is not a sibling");
        assert!(
            !base.path().join("demo").exists(),
            "the destination was created early"
        );
    }

    #[test]
    fn the_staging_name_is_identifiable_as_renvors() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        assert!(
            staging.name.starts_with(".renvor-staging-"),
            "{}",
            staging.name
        );
        assert!(
            staging.name.contains(&std::process::id().to_string()),
            "{}",
            staging.name
        );
    }

    #[test]
    fn dropping_without_placing_removes_the_staged_tree() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let name = {
            let staging = Staging::create(&target).expect("creates");
            staging.dir().write("f", b"x").expect("write");
            staging.name.clone()
        };
        assert!(
            !base.path().join(&name).exists(),
            "the staging directory outlived its owner"
        );
        assert!(
            !base.path().join("demo").exists(),
            "the destination was created anyway"
        );
    }

    #[test]
    fn placing_moves_the_tree_and_leaves_no_staging_behind() {
        // POSITIVE CONTROL for the two tests above: cleanup that also destroyed successful runs
        // would satisfy them and be useless.
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        let staged = staging.name.clone();
        staging.dir().write("f", b"x").expect("write");
        staging.place(&target).expect("places");
        assert!(
            base.path().join("demo/f").exists(),
            "the tree did not arrive"
        );
        assert!(
            !base.path().join(&staged).exists(),
            "the staging directory was left behind"
        );
    }

    #[test]
    fn an_existing_empty_destination_is_generated_into() {
        // FR-013 refuses a destination that exists and is **not empty**, so an existing empty one
        // must work. Until 2026-08-18 `Destination::open` accepted it and this method refused it,
        // seconds later, with a message claiming it had "appeared" mid-run.
        let base = tempfile::tempdir().expect("a temporary directory");
        std::fs::create_dir(base.path().join("demo")).expect("mkdir");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        staging.dir().write("f", b"x").expect("write");
        staging
            .place(&target)
            .expect("an existing EMPTY destination must be generated into");
        assert!(
            base.path().join("demo/f").exists(),
            "the tree did not arrive"
        );
    }

    #[test]
    fn a_destination_that_gained_contents_after_validation_is_refused_by_the_kernel() {
        // The safety of the fix above rests on `remove_dir` refusing a non-empty directory — the
        // emptiness check and the removal are ONE atomic operation, not a check followed by a
        // hopeful delete. This asserts that guarantee rather than assuming it.
        let base = tempfile::tempdir().expect("a temporary directory");
        std::fs::create_dir(base.path().join("demo")).expect("mkdir");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        staging.dir().write("ours", b"ours").expect("write");

        // It was empty at validation; now it is not.
        std::fs::write(base.path().join("demo/theirs"), b"theirs").expect("write");

        let error = staging.place(&target).unwrap_err();
        assert_eq!(error.code, Code::DestinationNotEmpty);
        assert_eq!(
            std::fs::read_to_string(base.path().join("demo/theirs")).expect("read"),
            "theirs",
            "the other party's file was destroyed"
        );
        assert!(
            !base.path().join("demo/ours").exists(),
            "our tree leaked in"
        );
    }

    #[test]
    fn a_destination_that_appears_mid_run_is_a_clean_failure_not_an_overwrite() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        staging.dir().write("ours", b"ours").expect("write");

        // Someone else gets there first, and puts something we must not destroy.
        std::fs::create_dir(base.path().join("demo")).expect("mkdir");
        std::fs::write(base.path().join("demo/theirs"), b"theirs").expect("write");

        let error = staging.place(&target).unwrap_err();
        assert_eq!(error.code, Code::DestinationNotEmpty);
        assert!(
            base.path().join("demo/theirs").exists(),
            "the other run's file was destroyed; this is the overwrite the whole contract exists \
             to prevent"
        );
        assert!(
            !base.path().join("demo/ours").exists(),
            "our tree leaked in"
        );
    }

    #[test]
    fn the_handle_is_closed_before_the_directory_is_removed() {
        // Asserted through the observable consequence rather than by inspecting the field: after a
        // drop, the directory must be gone. On Windows this fails if the handle is still open, and
        // this test is why that is caught here rather than in CI on one platform.
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let name = {
            let staging = Staging::create(&target).expect("creates");
            staging.dir().create_dir_all("deep/nested").expect("mkdir");
            staging.dir().write("deep/nested/f", b"x").expect("write");
            staging.name.clone()
        };
        assert!(
            !base.path().join(&name).exists(),
            "a non-empty staging directory survived its owner"
        );
    }

    #[test]
    fn the_staging_handle_cannot_write_outside_itself() {
        // The transaction's containment, asserted rather than assumed.
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        assert!(staging.dir().write("../escaped", b"x").is_err());
        assert!(staging.dir().write("/tmp/renvor-escaped", b"x").is_err());
        assert!(!base.path().join("escaped").exists());
        // POSITIVE CONTROL.
        staging
            .dir()
            .write("legitimate", b"x")
            .expect("ordinary writes must still work");
    }
}
