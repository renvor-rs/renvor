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
                    destination.display_path().display()
                ),
            )
            .with(
                "destination",
                destination.display_path().display().to_string(),
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
        let target = destination.name();
        let named = |code, detail: String| {
            CliError::new(code, detail).with(
                "destination",
                destination.display_path().display().to_string(),
            )
        };

        // Injected failure for the transaction tests. **Before any mutation**, so the meaning is
        // exactly "the place step failed": nothing was moved, nothing was removed, and `self`
        // drops on the way out — which is the cleanup this test is checking. Injecting *between*
        // the removal below and the rename would instead exercise the restore path, and would
        // report an FR-012 violation caused by where the injector sits rather than by the code.
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
                         there and the staged tree has been removed; renvor will not delete or \
                         replace a destination you already have",
                        destination.display_path().display()
                    ),
                )
                .with("rule", "destination_absent")
                .with("found", crate::paths::describe(&metadata)));
            }
            Err(error) => {
                return Err(named(
                    Code::PlacementFailed,
                    format!(
                        "`{}` could not be inspected before placement, so renvor cannot establish \
                         that it is absent: {error}. Nothing was written there and the staged tree \
                         has been removed",
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
                         it first. Nothing was written there and the staged tree has been removed",
                        destination.display_path().display()
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
        assert_eq!(
            offending.len(),
            1,
            "expected exactly one removal in this module — the staging cleanup in `Drop` — and \
             found {}: {offending:?}",
            offending.len()
        );
        assert!(
            offending[0].contains("&self.name"),
            "this module removes something that is not this process's own staging directory: {}",
            offending[0].trim()
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
