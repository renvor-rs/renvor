//! Staging and placement — the two halves of the transaction that touch the destination.
//!
//! See `contracts/generation-transaction.md`.

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
    /// Set once a failure path has already run the cleanup explicitly, so `drop` does not run it a
    /// second time and fail for a reason nobody wants reported.
    discarded: bool,
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
    /// [`Code::StagingFailed`] if the directory cannot be created — which usually means the parent
    /// is not writable, and is worth saying plainly rather than failing later.
    ///
    /// **Not** [`Code::PlacementFailed`], which it was until 2026-08-18. That code's published
    /// meaning is *"the final move could not be performed"*, and at this point there is nothing to
    /// move: nothing has been staged, so nothing can have been left behind. A consumer matching
    /// the published meaning would have concluded the opposite (A-R6).
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
                Code::StagingFailed,
                format!(
                    "a staging directory could not be created beside the destination in `{}`: \
                     {error}. Nothing was written; this usually means the destination's parent is \
                     not writable",
                    crate::output::redact::path(&destination.display_path())
                ),
            )
            .with(
                "destination",
                destination.display_path().display().to_string(),
            )
        };

        parent.create_dir(&name).map_err(failed)?;

        // ── THE DIRECTORY EXISTS NOW, AND `Drop` CANNOT CLEAN IT UP YET ────────────────
        //
        // `Drop for Staging` removes the staging tree on every failure path — but it only runs once
        // a `Staging` exists, and between the `create_dir` above and the `Ok(Self)` below there is
        // none. So a failing `open_dir` used to return through `?` and leave the directory it had
        // just created **orphaned in the operator's parent**, with nothing to remove it.
        //
        // ── AND THE CLEANUP ITSELF CAN FAIL, WHICH IS THE POINT OF THIS BLOCK ──────────
        //
        // The first version of this fix wrote `let _ = parent.remove_dir_all(&name)` and kept the
        // message "Nothing was written". An advisory review pointed out the obvious: if the same
        // condition that broke `open_dir` also breaks the removal, the directory survives and the
        // message is a **false statement about the filesystem** — the narrower recurrence of the
        // very bug being fixed.
        //
        // So the two outcomes are reported differently, and the residue case names the **exact
        // path** that is still there, because "something may be left somewhere" is not something an
        // operator can act on.
        let opened = match crate::inject::io_failure("staging-open") {
            Some(injected) => Err(injected),
            None => parent.open_dir(&name),
        };
        let handle = match opened {
            Ok(handle) => handle,
            Err(open_error) => {
                // The removal can only ever name OUR OWN just-created directory: `create_dir`
                // refuses an existing name, so its success two lines up proves nothing else had it.
                let cleanup = match crate::inject::io_failure("staging-cleanup") {
                    Some(injected) => Err(injected),
                    None => parent.remove_dir_all(&name),
                };
                return Err(match cleanup {
                    // Created, then unopenable, then successfully removed. "Nothing was written" is
                    // true here — but `failed`'s wording says "could not be created", which is not,
                    // so this case gets its own sentence rather than borrowing an inaccurate one.
                    Ok(()) => CliError::new(
                        Code::StagingFailed,
                        format!(
                            "a staging directory was created beside the destination and could not \
                             then be opened: {open_error}. It has been removed and nothing was \
                             written to `{}`",
                            crate::output::redact::path(&destination.display_path())
                        ),
                    )
                    .with(
                        "destination",
                        destination.display_path().display().to_string(),
                    )
                    .with("openError", open_error.to_string()),
                    Err(cleanup_error) => {
                        let residue = destination.parent_display().join(&name);
                        CliError::new(
                            Code::StagingFailed,
                            format!(
                                "a staging directory was created beside the destination and then \
                                 could not be opened: {open_error}. Removing it ALSO failed: \
                                 {cleanup_error}. **It is still there**, at `{}`, and renvor could \
                                 not clean it up — remove it by hand. Nothing was written to the \
                                 destination itself",
                                crate::output::redact::path(&residue)
                            ),
                        )
                        .with(
                            "destination",
                            destination.display_path().display().to_string(),
                        )
                        .with("residue", residue.display().to_string())
                        .with("openError", open_error.to_string())
                        .with("cleanupError", cleanup_error.to_string())
                    }
                });
            }
        };
        Ok(Self {
            parent,
            name,
            handle: Some(handle),
            placed: false,
            discarded: false,
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
    /// The one thing the rename can still replace is an **empty directory created by another
    /// process** after STEP 1's check — POSIX `rename(2)` replaces one silently. See invariant
    /// I-17 in `paths.rs`, which states that residual and why closing it is not portable.
    ///
    /// # Errors
    ///
    /// [`Code::DestinationExists`] if the destination appeared while we were rendering — the
    /// concurrency case, converted into a clean failure rather than an overwrite — or
    /// [`Code::PlacementFailed`] for anything else. **The destination is unchanged either way.**
    pub fn place(mut self, destination: &Destination) -> Result<(), CliError> {
        match self.place_steps(destination) {
            Ok(()) => Ok(()),
            // EVERY failure inside `place_steps` comes back through here, so the cleanup runs
            // once, explicitly, and the message states what actually happened to the staged tree.
            //
            // It used to be left to `Drop`, which cannot work: `Drop` runs *after* the error has
            // been built and cannot return anything, so three of these messages asserted "and the
            // staged tree has been removed" before any removal had been attempted. Whenever the
            // removal then failed — the ordinary Windows case, where a scanner or an open Explorer
            // window holds a handle renvor does not own — renvor reported a removal that had not
            // happened, and named no residue for `doctor` to find.
            Err(error) => Err(self.report_cleanup(error, destination)),
        }
    }

    /// Removes the staged tree and folds the outcome into the failure being reported.
    ///
    /// Sets `discarded` so `Drop` does not attempt a second removal, which would fail for a reason
    /// nobody wants to read (the path is already gone).
    fn report_cleanup(&mut self, mut error: CliError, destination: &Destination) -> CliError {
        drop(self.handle.take());
        let outcome = match crate::inject::io_failure("staging-drop-cleanup") {
            Some(injected) => Err(injected),
            None => self.parent.remove_dir_all(&self.name),
        };
        self.discarded = true;

        match outcome {
            Ok(()) => {
                error.message.push_str(" The staged tree has been removed.");
                error
            }
            Err(cleanup) => {
                let residue = destination.parent_display().join(&self.name);
                error.message.push_str(&format!(
                    " The staged tree could NOT be removed and is still at `{}`: {cleanup}",
                    crate::output::redact::path(&residue)
                ));
                error
                    .with("residue", residue.display().to_string())
                    .with("cleanupError", cleanup.to_string())
            }
        }
    }

    fn place_steps(&mut self, destination: &Destination) -> Result<(), CliError> {
        let target = destination.name();
        let named = |code, detail: String| {
            CliError::new(code, detail).with(
                "destination",
                destination.display_path().display().to_string(),
            )
        };

        // Injected failure for the transaction tests. **Before any mutation**, so the meaning is
        // exactly "the place step failed": nothing was moved and `self` drops on the way out,
        // which is the cleanup those tests check.
        //
        // Its placement used to matter more than it does now. While `place` removed an existing
        // empty destination, injecting *after* that removal exercised a restore path and would
        // have reported an FR-012 violation caused by where the injector sat rather than by the
        // code. **There is no longer any position in this method that removes anything**, so the
        // injector's placement is now a straightforward choice rather than a load-bearing one.
        crate::inject::fail_at("place")?;

        // ── STEP 1: refuse an existing destination. NOTHING IS REMOVED HERE ─────────────
        //
        // ── WHAT THIS BLOCK USED TO DO, AND WHY IT NO LONGER DOES ───────────────────────
        //
        // It used to `remove_dir` an existing **empty** destination to make room, and restore it
        // if the rename then failed. The 2026-08-18 maintainer ruling removed both halves:
        //
        //   - the removal replaced a directory the operator owned with a fresh one carrying this
        //     process's mode and ownership (A-R8);
        //   - the restore branch was reachable from no test, and swallowed its own error (A-R9).
        //
        // `renvor new` is now a program that creates a destination and never removes one. The
        // deleted code is not replaced by a safer deletion; it is replaced by a refusal.
        //
        // `Destination::open` already refused every existing destination before anything was
        // staged. This is the same rule applied a second time at the last possible moment, because
        // between those two points a whole render, verification, and manifest pass has run.
        //
        // Errors OTHER than an authoritative `NotFound` refuse too — the same fail-closed rule as
        // `paths.rs` RULE 4, for the same reason: an unknown filesystem state is not absence.
        match self.parent.symlink_metadata(target) {
            // The ONLY arm that proceeds.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(metadata) => {
                // The SAME `details` as `Destination::open`'s refusal, from the shared classifier.
                // Contract C-2 publishes `rule` and `found` for `destination_exists`, and a code
                // that carries them at one emit site and not the other makes a consumer's handling
                // depend on which moment the check happened to fire in.
                return Err(named(
                    Code::DestinationExists,
                    format!(
                        "`{}` appeared while the project was being generated. Nothing was written \
                         there; renvor will not delete or \
                         replace a destination you already have",
                        crate::output::redact::path(&destination.display_path())
                    ),
                )
                .with("rule", "destination_absent")
                .with("found", crate::paths::describe(&metadata)));
            }
            Err(error) => {
                // `destination_rejected` with `rule = "destination_unverifiable"` — the SAME code
                // and the same rule `paths.rs` RULE 4 uses for the same condition, and NOT
                // `placement_failed`.
                //
                // `placement_failed` is published as *"the final move could not be performed
                // atomically"*, and no move has been attempted here: the rename is two steps away.
                // Reporting it would be the identical category error that finding A-R6 was about,
                // reintroduced in the commit that fixed A-R6 — which is the kind of thing that
                // happens when a code is chosen for being nearby rather than for being true.
                return Err(named(
                    Code::DestinationRejected,
                    format!(
                        "`{}` could not be inspected before placement, so renvor cannot establish \
                         that it is absent: {error}. Nothing was written there",
                        crate::output::redact::path(&destination.display_path())
                    ),
                )
                .with("rule", "destination_unverifiable")
                .with("error", error.to_string()));
            }
        }

        // ── STEP 2: close our handle BEFORE renaming ────────────────────────────────────
        //
        // Windows refuses to rename or delete a directory that has an open handle, with
        // `os error 32`. On Unix this is harmless. Measured, not assumed: seven tests failed on
        // both Windows toolchains before this line existed.
        drop(self.handle.take());

        // ── STEP 3: the rename, and INVARIANT I-17 ──────────────────────────────────────
        //
        // **Time-of-check to time-of-use is narrowed here, not eliminated**, and this is the line
        // where that matters. Step 1 observed the destination; this step acts on it. Another
        // process can create the destination in between, and nothing below prevents that.
        //
        // What holds instead: the rename **refuses an existing destination**, so losing the race is
        // a clean failure rather than an overwrite, and both the observation and the action go
        // through the same open parent handle rather than re-resolving a path string twice.
        //
        // Eliminating the window needs an atomic create-or-fail rename. POSIX `rename(2)` silently
        // replaces an empty destination directory; `renameat2(RENAME_NOREPLACE)` would do it and is
        // Linux-only. See `paths.rs` for the full statement of I-17.
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

            // NOTHING IS RESTORED HERE, because nothing was removed. The block that used to put
            // back a destination this method had deleted is gone with the deletion — see STEP 1.
            // FR-012's "left exactly as it was" is now satisfied by never having touched it.

            return Err(if lost_the_race {
                // `found` is BEST EFFORT here and says so by having an `unknown` value. Unlike
                // STEP 1, this branch has no metadata in hand — the rename failed, and asking the
                // filesystem again is a fresh observation that the winner may already have changed.
                // It is a diagnostic detail, not the classification: the decision above comes from
                // the kernel's own `ErrorKind`, deliberately, and is not revisited here.
                let found = self
                    .parent
                    .symlink_metadata(target)
                    .as_ref()
                    .map_or("unknown", crate::paths::describe);
                named(
                    Code::DestinationExists,
                    format!(
                        "`{}` appeared while the project was being generated — another run reached \
                         it first. Nothing was written there",
                        crate::output::redact::path(&destination.display_path())
                    ),
                )
                .with("rule", "destination_absent")
                .with("found", found)
            } else {
                named(
                    Code::PlacementFailed,
                    format!(
                        "the generated project could not be moved into place atomically: {error}. \
                         Nothing was written to `{}`",
                        crate::output::redact::path(&destination.display_path())
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
        if !self.placed && !self.discarded {
            // `staging-drop-cleanup` makes the removal fail without touching the filesystem, which
            // is the only way a test can reach this arm: `staging-cleanup` injects into
            // `Staging::create` and never gets here.
            let outcome = match crate::inject::io_failure("staging-drop-cleanup") {
                Some(injected) => Err(injected),
                None => self.parent.remove_dir_all(&self.name),
            };
            // `Drop` cannot return, so it cannot put this in the error the caller is building —
            // which is exactly why `place` does its own cleanup through `report_cleanup` instead of
            // relying on this. What is left here are the failures that happen *before* placement
            // (rendering, verification, manifest writing), and for those this is the only place the
            // residue can be mentioned at all.
            //
            // Silence was the previous behaviour and is the one thing that must not happen: a
            // staged tree survives, nothing says so, and the operator finds a `.renvor-staging-*`
            // directory later with no idea what left it. `stderr` is the diagnostic stream in both
            // output modes, so writing here cannot disturb the single JSON document on `stdout`.
            if let Err(cleanup) = outcome {
                use std::io::Write as _;
                let residue = std::path::Path::new(&self.name);
                let _ = writeln!(
                    std::io::stderr(),
                    "warning: the staged tree could not be removed and is still at `{}`: {cleanup}",
                    crate::output::redact::path(residue)
                );
            }
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
    fn an_empty_destination_that_appears_mid_run_is_refused_and_not_replaced() {
        // THE TEST THAT FAILS WITHOUT THE 2026-08-18 CORRECTION.
        //
        // Until then this method deliberately `remove_dir`-ed an existing empty destination and
        // let the rename create a fresh one, so this scenario SUCCEEDED and the operator's
        // directory was silently replaced — different inode, this process's mode and ownership.
        //
        // The empty case is the one that distinguishes the old behaviour from the new. A non-empty
        // directory was refused before and is refused now, so a test using one proves nothing
        // about the change.
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        staging.dir().write("ours", b"ours").expect("write");

        // Someone else creates the destination — EMPTY — while we were rendering.
        std::fs::create_dir(base.path().join("demo")).expect("mkdir");
        let before = std::fs::metadata(base.path().join("demo")).expect("metadata");

        let error = staging.place(&target).unwrap_err();
        assert_eq!(error.code, Code::DestinationExists);

        let after = std::fs::metadata(base.path().join("demo")).expect("the directory was removed");
        assert!(
            after.is_dir(),
            "the directory was replaced by something else"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            assert_eq!(
                (before.ino(), before.mode(), before.uid(), before.gid()),
                (after.ino(), after.mode(), after.uid(), after.gid()),
                "the operator's directory was replaced: renvor must not delete, recreate, chmod, \
                 or chown a path it did not make"
            );
        }
        #[cfg(not(unix))]
        let _ = before;

        assert!(
            !base.path().join("demo/ours").exists(),
            "our tree leaked in"
        );
    }

    #[test]
    fn a_non_empty_destination_that_appears_mid_run_is_a_clean_failure_not_an_overwrite() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        staging.dir().write("ours", b"ours").expect("write");

        // Someone else gets there first, and puts something we must not destroy.
        std::fs::create_dir(base.path().join("demo")).expect("mkdir");
        std::fs::write(base.path().join("demo/theirs"), b"theirs").expect("write");

        let error = staging.place(&target).unwrap_err();
        assert_eq!(error.code, Code::DestinationExists);
        assert_eq!(
            std::fs::read_to_string(base.path().join("demo/theirs")).expect("read"),
            "theirs",
            "the other party's file was destroyed; this is the overwrite the whole contract exists \
             to prevent"
        );
        assert!(
            !base.path().join("demo/ours").exists(),
            "our tree leaked in"
        );
    }

    #[test]
    fn a_file_that_appears_at_the_destination_mid_run_is_refused_and_left_alone() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        staging.dir().write("ours", b"ours").expect("write");

        std::fs::write(base.path().join("demo"), b"theirs").expect("write");

        let error = staging.place(&target).unwrap_err();
        assert_eq!(error.code, Code::DestinationExists);
        assert_eq!(
            std::fs::read_to_string(base.path().join("demo")).expect("read"),
            "theirs",
            "the operator's file was destroyed"
        );
    }

    #[test]
    fn both_emit_sites_of_destination_exists_carry_the_published_details() {
        // Contract C-2 publishes `details.rule` and `details.found` for `destination_exists`, and
        // this code is emitted from TWO places at two different moments: `Destination::open`, before
        // anything is staged, and `Staging::place` STEP 1, immediately before the rename.
        //
        // Until 2026-08-18 the second site carried neither, so the shape of a failure depended on
        // which moment the destination happened to appear in — a difference a consumer cannot
        // predict and the contract does not mention. Found by re-reading the diff against the
        // registry table rather than by a test failing.
        let base = tempfile::tempdir().expect("a temporary directory");

        // Site 1: `Destination::open`.
        std::fs::create_dir(base.path().join("early")).expect("mkdir");
        let from_open = Destination::open(&base.path().join("early")).unwrap_err();

        // Site 2: `Staging::place` STEP 1 — the destination appears after validation.
        let target = destination(base.path(), "late");
        let staging = Staging::create(&target).expect("creates");
        std::fs::create_dir(base.path().join("late")).expect("mkdir");
        let from_place = staging.place(&target).unwrap_err();

        for (site, error) in [("open", &from_open), ("place", &from_place)] {
            assert_eq!(error.code, Code::DestinationExists, "{site}");
            let detail = |key: &str| {
                error
                    .details
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.as_str())
                    .unwrap_or_else(|| {
                        panic!("`{site}` emitted no `details.{key}`: {:?}", error.details)
                    })
            };
            assert_eq!(detail("rule"), "destination_absent", "{site}");
            assert_eq!(detail("found"), "directory", "{site}");
            assert!(!detail("destination").is_empty(), "{site}");
        }
    }

    #[test]
    fn both_fail_closed_arms_report_the_same_code_and_rule() {
        // The fail-closed condition — "the destination's state could not be established" — is
        // reached from two places, `Destination::open` RULE 4 and `Staging::place` STEP 1, and both
        // must answer with the same code and the same rule.
        //
        // `place`'s arm reported `placement_failed` when first written, which is published as *"the
        // final move could not be performed atomically"* while no move had been attempted: the
        // identical category error finding A-R6 was about, reintroduced inside the commit that
        // fixed A-R6. This test exists so it cannot come back quietly.
        //
        // Asserted from the SOURCE for `place`'s arm rather than by provoking it, because provoking
        // it needs the parent to become unreadable *after* staging was created inside it — which
        // makes the staging cleanup in `Drop` fail too, and turns the test into a tempdir the
        // harness cannot remove. The behavioural half of the same rule is covered on the `paths.rs`
        // side by `a_destination_whose_state_cannot_be_established_fails_closed`.
        let source = include_str!("place.rs");
        let step_one = source
            .split("── STEP 1")
            .nth(1)
            .expect("STEP 1 is still marked in the source")
            .split("── STEP 2")
            .next()
            .expect("STEP 2 follows STEP 1");

        assert!(
            step_one.contains("Code::DestinationRejected"),
            "STEP 1's fail-closed arm must refuse with `destination_rejected`, the same code              `paths.rs` RULE 4 uses for the same condition"
        );
        assert!(
            step_one.contains("\"destination_unverifiable\""),
            "STEP 1's fail-closed arm must name the same rule as `paths.rs` RULE 4"
        );
        assert!(
            !step_one.contains("Code::PlacementFailed"),
            "STEP 1 attempts no move, so `placement_failed` would be a false statement about what              was tried"
        );
        // POSITIVE CONTROL: the slice must actually be STEP 1 and not an empty string.
        assert!(
            step_one.contains("symlink_metadata"),
            "the extracted STEP 1 slice does not contain the check it is supposed to be about"
        );
    }

    #[test]
    fn no_production_path_removes_the_destination() {
        // ── ASSERTED AGAINST THE SOURCE, DELIBERATELY ───────────────────────────────────
        //
        // The behavioural tests above prove that the paths they walk do not delete the
        // destination. They cannot prove that no *other* path does, and the maintainer ruling is
        // about the whole module, not about the cases somebody thought to write.
        //
        // So this reads this file. `remove_dir` and `remove_dir_all` may appear exactly once each,
        // both in `Drop`, and both applied to `self.name` — the staging directory this process
        // created. Any occurrence naming `target` or `destination` is the deletion the ruling
        // removed, coming back.
        //
        // A source-text assertion is a blunt instrument and is used knowingly: the alternative is
        // trusting that a future edit will remember, and the deleted code was written by someone
        // who also believed it was safe.
        // Assembled with `concat!` so that THIS TEST'S OWN SOURCE does not contain the string it
        // searches for. Spelling it literally makes the scan match its own machinery, which is a
        // self-inflicted failure that teaches a future reader to loosen the check.
        const REMOVAL: &str = concat!("remove_", "dir");
        let source = include_str!("place.rs");

        let offending: Vec<&str> = source
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .filter(|code| code.contains(REMOVAL))
            .collect();

        // POSITIVE CONTROL FIRST: a scan that matched nothing would pass every assertion below
        // while verifying nothing — which is precisely the defect an advisory review found in a
        // different test this same phase.
        //
        // THREE removals are expected, and every one removes **this process's own staging
        // directory** — named by `self.name`, which `create_dir` refused to reuse:
        //
        //   1. `Drop`, the cleanup for failures that happen before placement is attempted;
        //   2. `Staging::create`'s `open_dir` failure arm, which is the window BEFORE a `Staging`
        //      exists and therefore before `Drop` can run;
        //   3. `report_cleanup`, which every failure inside `place_steps` routes through so the
        //      message can state what actually happened instead of asserting a removal that had
        //      not been attempted.
        //
        // The second was added on 2026-08-18 and the third on 2026-08-19, and **this test caught
        // both**, failing with *"the number of removals in this module changed"*. That is the test
        // doing its job: a new removal here has to be looked at and justified rather than merged
        // quietly. The count is raised deliberately, with the justification above, and not
        // loosened into "however many there are".
        assert_eq!(
            offending.len(),
            3,
            "the number of removals in this module changed. Every one must be this process's own \
             staging directory, and a new one needs a reason written down here: {offending:?}"
        );
        for line in &offending {
            assert!(
                line.contains("&self.name") || line.contains("(&name)"),
                "this module removes something that is not this process's own staging directory: \
                 {}",
                line.trim()
            );
        }
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
                    Code::DestinationExists,
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
