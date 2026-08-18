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
        // THE NAME HAS THREE PARTS, AND THE THIRD IS NOT DECORATION.
        //
        // `pid` separates processes. The clock reading separates runs across processes that reuse
        // a pid. **The counter separates threads within one process**, which the clock does not:
        // an earlier version used only pid + nanoseconds and claimed in a comment that "two runs
        // in one process never collide" — which is false, because two threads can read the same
        // nanosecond, and a sixteen-thread race on macOS proved it by failing to create a staging
        // directory whose name was already taken.
        //
        // The CLI is single-threaded, so this was not reachable through `renvor new`. It was still
        // a false statement in a comment about a uniqueness property, and the fix makes the
        // statement true rather than deleting it.
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let name = format!(
            ".renvor-staging-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
        let named = |code, detail: String| {
            CliError::new(code, detail).with(
                "destination",
                destination.display_path().display().to_string(),
            )
        };

        // ── STEP 1: make room, or refuse ────────────────────────────────────────────────
        //
        // FR-013 refuses a destination that "exists and is **not empty**", so an existing *empty*
        // one must work — `Destination::open` accepts it, and this method must agree. An earlier
        // version did not, so `renvor new` into an empty directory validated, rendered, ran the
        // full pre-placement verification, and only then failed with a message claiming the
        // directory had "appeared" mid-run.
        //
        // `remove_dir` is what makes this safe: **the kernel refuses to remove a non-empty
        // directory**, so the emptiness check and the removal are one atomic operation rather than
        // a check followed by a hopeful delete.
        let mut removed_a_pre_existing_directory = false;
        match self.parent.symlink_metadata(target) {
            // Absent. The ordinary path.
            Err(_) => {}
            Ok(metadata) if metadata.is_dir() => {
                self.parent.remove_dir(target).map_err(|error| {
                    named(
                        Code::DestinationNotEmpty,
                        format!(
                            "`{}` exists and could not be replaced: {error}. Nothing was written \
                             there; renvor does not merge into an existing project",
                            destination.display_path().display()
                        ),
                    )
                })?;
                removed_a_pre_existing_directory = true;
            }
            // A file, or a symlink that appeared after validation.
            Ok(_) => {
                return Err(named(
                    Code::DestinationNotEmpty,
                    format!(
                        "`{}` exists and is not a directory; nothing was written there and the \
                         staged tree has been removed",
                        destination.display_path().display()
                    ),
                ));
            }
        }

        // ── STEP 2: close our handle BEFORE renaming ────────────────────────────────────
        //
        // Windows refuses to rename or delete a directory that has an open handle, with
        // `os error 32`. On Unix this is harmless. Measured, not assumed: seven tests failed on
        // both Windows toolchains before this line existed.
        drop(self.handle.take());

        // ── STEP 3: the rename ──────────────────────────────────────────────────────────
        if let Err(error) = self.parent.rename(&self.name, self.parent, target) {
            // CLASSIFY FROM THE KERNEL'S OWN ANSWER, not from a second observation.
            //
            // An earlier version decided by re-stating the destination, which is itself racy: with
            // sixteen threads contending, a loser could fail with `ENOTEMPTY` and then find the
            // path absent a moment later, and get reported as `placement_failed` — "the move
            // mechanism broke", which sends an operator to debug their filesystem when a second
            // `renvor new` simply beat them to it.
            //
            // The error already carries the answer. The re-stat is kept only as a fallback for
            // kinds this match does not name.
            let lost_the_race = matches!(
                error.kind(),
                std::io::ErrorKind::DirectoryNotEmpty | std::io::ErrorKind::AlreadyExists
            ) || self.parent.symlink_metadata(target).is_ok();

            // Put the operator's directory back. FR-012 requires a failure to leave the
            // destination exactly as it was, and FR-014 forbids deleting what renvor did not
            // create — so removing it in step 1 and then failing must not be observable.
            if removed_a_pre_existing_directory && !lost_the_race {
                let _ = self.parent.create_dir(target);
            }

            return Err(if lost_the_race {
                named(
                    Code::DestinationNotEmpty,
                    format!(
                        "`{}` appeared while the project was being generated — another run reached \
                         it first. Nothing was written there and the staged tree has been removed",
                        destination.display_path().display()
                    ),
                )
            } else {
                named(
                    Code::PlacementFailed,
                    format!(
                        "the generated project could not be moved into place atomically: {error}. \
                         Nothing was written to `{}`",
                        destination.display_path().display()
                    ),
                )
            });
        }

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
    fn staging_names_are_unique_within_one_process() {
        // THE REGRESSION TEST. `pid` + nanoseconds alone is not unique across threads — two can
        // read the same nanosecond, which a sixteen-thread race on macOS CI demonstrated by
        // failing to create a directory whose name was already taken. Asserted directly rather
        // than by racing, so the guarantee is checked deterministically instead of probabilistically.
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let mut names = std::collections::BTreeSet::new();
        let mut held = Vec::new();
        for _ in 0..256 {
            let staging = Staging::create(&target).expect("creates");
            assert!(
                names.insert(staging.name.clone()),
                "two staging directories in one process share the name `{}`",
                staging.name
            );
            held.push(staging);
        }
        assert_eq!(names.len(), 256);
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
    fn racing_placements_produce_one_winner_and_correctly_classified_losers() {
        // THE PRECISE VERSION OF THE CONCURRENCY PROPERTY.
        //
        // `tests/acceptance.rs` races whole processes, which is the honest end-to-end check — but
        // each of those runs a full `cargo build` for pre-placement verification, so racing twelve
        // of them stresses cargo far more than it stresses this rename. **That is a test-design
        // flaw, not a stronger test.**
        //
        // This races the placement itself: sixteen threads, no subprocess, no build. It hits the
        // window orders of magnitude more often and finishes in milliseconds.
        use std::sync::{Arc, Barrier};

        let base = tempfile::tempdir().expect("a temporary directory");
        let threads = 16;
        // A barrier so every thread reaches `place` at the same instant. Without it they queue up
        // behind their own setup and the race is never contended.
        let barrier = Arc::new(Barrier::new(threads));

        let handles: Vec<_> = (0..threads)
            .map(|index| {
                let barrier = Arc::clone(&barrier);
                let root = base.path().to_path_buf();
                std::thread::spawn(move || {
                    let target = Destination::open(&root.join("contended")).expect("opens");
                    let staging = Staging::create(&target).expect("creates");
                    staging
                        .dir()
                        .write("f", index.to_string().as_bytes())
                        .expect("write");
                    barrier.wait();
                    staging
                        .place(&target)
                        .map_err(|error| (error.code, error.message))
                })
            })
            .collect();

        let mut winners = 0;
        for handle in handles {
            match handle.join().expect("thread") {
                Ok(()) => winners += 1,
                Err((code, message)) => assert_eq!(
                    code,
                    Code::DestinationNotEmpty,
                    "a loser must report that someone else got there, not that the move mechanism \
                     broke: {message}"
                ),
            }
        }
        assert_eq!(winners, 1, "exactly one placement must win");

        // The survivor is one whole tree, not a merge: exactly one file, whose contents are one
        // thread's index rather than a mixture.
        let placed: Vec<_> = std::fs::read_dir(base.path().join("contended"))
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            placed,
            vec!["f".to_owned()],
            "the placed tree is a merge: {placed:?}"
        );

        // And no staging residue from the fifteen losers.
        let residue: Vec<String> = std::fs::read_dir(base.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with(".renvor-staging-"))
            .collect();
        assert!(
            residue.is_empty(),
            "losers left staging behind: {residue:?}"
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
