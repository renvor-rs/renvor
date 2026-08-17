//! Staging and placement — the two halves of the transaction that touch the destination.
//!
//! See `specs/003-interactive-cli/contracts/generation-transaction.md`.

use std::path::{Path, PathBuf};

use crate::exit::{CliError, Code};
use crate::paths::DestinationPath;

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
#[derive(Debug)]
pub struct Staging {
    path: PathBuf,
    placed: bool,
}

impl Staging {
    /// Creates the staging directory beside the destination.
    ///
    /// # Errors
    ///
    /// [`Code::PlacementFailed`] if the directory cannot be created — which usually means the
    /// parent is not writable, and is worth saying plainly rather than failing later.
    pub fn create(destination: &DestinationPath) -> Result<Self, CliError> {
        let unique = format!(
            ".renvor-staging-{}-{}",
            std::process::id(),
            // A monotonic-ish discriminator so two runs in one process never collide. Not
            // security-relevant; uniqueness within a parent directory is all that is needed.
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        );
        let path = destination.parent().join(unique);
        std::fs::create_dir(&path).map_err(|error| {
            CliError::new(
                Code::PlacementFailed,
                format!(
                    "a staging directory could not be created beside the destination in `{}`: \
                     {error}",
                    destination.parent().display()
                ),
            )
            .with("parent", destination.parent().display().to_string())
        })?;
        Ok(Self { path, placed: false })
    }

    /// Where to render.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Moves the staged tree to the destination with a single rename.
    ///
    /// # Atomicity, per platform rather than claimed uniformly
    ///
    /// On POSIX, `rename(2)` onto a non-existent path within one filesystem is atomic. On Windows
    /// the nearest equivalent onto a non-existent path is used, and **POSIX-equivalent atomicity
    /// is not claimed**. The destination is guaranteed not to already exist (FR-013), which is
    /// what makes the weaker guarantee sufficient here.
    ///
    /// # Errors
    ///
    /// [`Code::DestinationNotEmpty`] if the destination appeared while we were rendering — the
    /// concurrency case, converted into a clean failure rather than an overwrite — or
    /// [`Code::PlacementFailed`] for anything else. **The destination is unchanged either way.**
    pub fn place(mut self, destination: &DestinationPath) -> Result<PathBuf, CliError> {
        let target = destination.as_path();

        // Re-check immediately before the rename. This does not close the race — see the module
        // docs on `crate::paths` — but it converts the common case of a concurrent run into a
        // named failure instead of letting `rename` decide.
        if target.exists() {
            return Err(CliError::new(
                Code::DestinationNotEmpty,
                format!(
                    "`{}` appeared while the project was being generated; nothing was written \
                     there and the staged tree has been removed",
                    target.display()
                ),
            )
            .with("destination", target.display().to_string()));
        }

        std::fs::rename(&self.path, target).map_err(|error| {
            CliError::new(
                Code::PlacementFailed,
                format!(
                    "the generated project could not be moved into place atomically: {error}. \
                     Nothing was written to `{}`",
                    target.display()
                ),
            )
            .with("destination", target.display().to_string())
        })?;

        self.placed = true;
        Ok(target.to_path_buf())
    }
}

impl Drop for Staging {
    /// Removes the staging directory unless it was successfully placed.
    ///
    /// This is what makes "cancellation or failure leaves the destination unchanged" true on
    /// **every** path including a panic, rather than only on the paths somebody remembered to
    /// write a cleanup for.
    fn drop(&mut self) {
        if !self.placed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn destination(base: &Path, name: &str) -> DestinationPath {
        DestinationPath::validate(&base.join(name)).expect("an ordinary destination")
    }

    #[test]
    fn staging_is_created_beside_the_destination_not_inside_it() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        assert_eq!(
            staging.path().parent(),
            Some(target.parent()),
            "staging must be a sibling of the destination, so residue never becomes part of a \
             project"
        );
        assert!(!staging.path().starts_with(target.as_path()));
    }

    #[test]
    fn the_staging_name_is_identifiable_as_renvors() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        let name = staging
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a name");
        assert!(name.starts_with(".renvor-staging-"), "{name}");
        assert!(name.contains(&std::process::id().to_string()), "{name}");
    }

    #[test]
    fn dropping_without_placing_removes_the_staged_tree() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let path = {
            let staging = Staging::create(&target).expect("creates");
            std::fs::write(staging.path().join("f"), b"x").expect("write");
            staging.path().to_path_buf()
        };
        assert!(!path.exists(), "the staging directory outlived its owner");
        assert!(!target.as_path().exists(), "the destination was created anyway");
    }

    #[test]
    fn placing_moves_the_tree_and_leaves_no_staging_behind() {
        // POSITIVE CONTROL for the two tests above: cleanup that also destroyed successful runs
        // would satisfy them and be useless.
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        let staged = staging.path().to_path_buf();
        std::fs::write(staging.path().join("f"), b"x").expect("write");
        let placed = staging.place(&target).expect("places");
        assert!(placed.join("f").exists(), "the tree did not arrive");
        assert!(!staged.exists(), "the staging directory was left behind");
    }

    #[test]
    fn a_destination_that_appears_mid_run_is_a_clean_failure_not_an_overwrite() {
        let base = tempfile::tempdir().expect("a temporary directory");
        let target = destination(base.path(), "demo");
        let staging = Staging::create(&target).expect("creates");
        std::fs::write(staging.path().join("ours"), b"ours").expect("write");

        // Someone else gets there first, and puts something we must not destroy.
        std::fs::create_dir(target.as_path()).expect("mkdir");
        std::fs::write(target.as_path().join("theirs"), b"theirs").expect("write");

        let error = staging.place(&target).unwrap_err();
        assert_eq!(error.code, Code::DestinationNotEmpty);
        assert!(
            target.as_path().join("theirs").exists(),
            "the other run's file was destroyed; this is the overwrite the whole contract exists \
             to prevent"
        );
        assert!(!target.as_path().join("ours").exists(), "our tree leaked in");
    }
}
