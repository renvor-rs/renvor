//! The file manifest — one artifact, three jobs.
//!
//! It is the `--dry-run` output, the pre-move verification input, and the reproducibility record.
//! Deliberately **one** structure: three would drift, and SC-006 requires the dry run to match
//! reality exactly. Data-model invariants I-9, I-10, I-11.

use std::path::Path;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::exit::{CliError, Code};

/// What a manifest entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
}

/// One path the run created, or would create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// Relative to the project root. Never absolute, never escaping.
    pub path: String,
    /// File or directory.
    pub kind: EntryKind,
    /// Byte length. Files only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Lowercase hex SHA-256. Files only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

/// The complete, ordered description of a generated tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileManifest {
    /// Sorted by path, so the manifest is reproducible and diffable regardless of the order the
    /// filesystem happened to yield entries in (invariant I-9).
    pub entries: Vec<Entry>,
}

/// Lowercase hex, written out because `digest` 0.11 returns a `hybrid_array::Array` rather than a
/// type implementing `LowerHex`. Four lines beats adding a `hex` dependency to format 32 bytes.
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut out, byte| {
        // `write!` to a `String` cannot fail; the result is discarded deliberately rather than
        // unwrapped, so this never panics on a formatting path.
        let _ = write!(out, "{byte:02x}");
        out
    })
}

impl FileManifest {
    /// Walks a rendered tree and describes it.
    ///
    /// # Symbolic links are not followed
    ///
    /// Invariant I-11. The manifest must describe what was **created**, not what a link points at.
    /// Following links would also make the manifest of a tree containing a link to `/etc` include
    /// `/etc`, which is both wrong and a disclosure.
    ///
    /// # Errors
    ///
    /// [`Code::RenderFailed`] if the tree cannot be walked or a file cannot be read. The caller is
    /// still inside staging at this point, so the destination is untouched.
    pub fn describe(root: &Path) -> Result<Self, CliError> {
        let mut entries = Vec::new();

        for item in walkdir::WalkDir::new(root).follow_links(false).min_depth(1) {
            let item = item.map_err(|error| {
                CliError::new(
                    Code::RenderFailed,
                    format!("the generated tree could not be walked: {error}"),
                )
            })?;

            let relative = item
                .path()
                .strip_prefix(root)
                .map_err(|_| {
                    CliError::new(
                        Code::RenderFailed,
                        "a generated path escaped the staging root",
                    )
                })?
                // Forward slashes on every platform, so a manifest generated on Windows and one
                // generated on Linux compare equal. SC-016 would otherwise fail across platforms
                // for a reason that has nothing to do with the generator.
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");

            if item.file_type().is_dir() {
                entries.push(Entry {
                    path: relative,
                    kind: EntryKind::Directory,
                    size: None,
                    digest: None,
                });
            } else if item.file_type().is_file() {
                let bytes = std::fs::read(item.path()).map_err(|error| {
                    CliError::new(
                        Code::RenderFailed,
                        format!("a generated file could not be read back: {error}"),
                    )
                })?;
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                entries.push(Entry {
                    path: relative,
                    kind: EntryKind::File,
                    size: Some(bytes.len() as u64),
                    digest: Some(hex(&hasher.finalize())),
                });
            } else {
                // A symlink or anything else. The generator creates none, so encountering one
                // means something else wrote into staging — refuse rather than describe it.
                return Err(CliError::new(
                    Code::RenderFailed,
                    format!(
                        "the staged tree contains `{relative}`, which is neither a file nor a \
                         directory; the generator creates neither, so something else wrote here"
                    ),
                ));
            }
        }

        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(Self { entries })
    }

    /// The paths, in order. Used by the dry-run comparison in tests and by the human summary.
    #[must_use]
    pub fn paths(&self) -> Vec<&str> {
        self.entries.iter().map(|entry| entry.path.as_str()).collect()
    }

    /// How many regular files the run produced.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.kind == EntryKind::File)
            .count()
    }

    /// Total bytes across all files.
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.entries.iter().filter_map(|entry| entry.size).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("a temporary directory");
        std::fs::create_dir_all(root.path().join("src")).expect("mkdir");
        std::fs::write(root.path().join("src/main.rs"), b"fn main() {}\n").expect("write");
        std::fs::write(root.path().join("Cargo.toml"), b"[package]\n").expect("write");
        root
    }

    #[test]
    fn entries_are_sorted_so_the_manifest_is_reproducible() {
        let root = tree();
        let manifest = FileManifest::describe(root.path()).expect("describes");
        let paths = manifest.paths();
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        assert_eq!(paths, sorted, "the manifest must not depend on traversal order");
    }

    #[test]
    fn describing_the_same_tree_twice_gives_the_same_manifest() {
        // SC-016 in miniature. If this ever fails, reproducibility is decorative.
        let root = tree();
        let first = FileManifest::describe(root.path()).expect("describes");
        let second = FileManifest::describe(root.path()).expect("describes");
        assert_eq!(first, second);
    }

    #[test]
    fn paths_are_relative_and_use_forward_slashes() {
        let root = tree();
        let manifest = FileManifest::describe(root.path()).expect("describes");
        for entry in &manifest.entries {
            assert!(!entry.path.starts_with('/'), "{}", entry.path);
            assert!(!entry.path.contains('\\'), "{}", entry.path);
        }
        assert!(manifest.paths().contains(&"src/main.rs"));
    }

    #[test]
    fn files_carry_a_digest_and_directories_do_not() {
        let root = tree();
        let manifest = FileManifest::describe(root.path()).expect("describes");
        for entry in &manifest.entries {
            match entry.kind {
                EntryKind::File => {
                    assert!(entry.digest.is_some(), "{} has no digest", entry.path);
                    assert_eq!(
                        entry.digest.as_deref().map(str::len),
                        Some(64),
                        "a SHA-256 hex digest is 64 characters"
                    );
                }
                EntryKind::Directory => assert!(entry.digest.is_none()),
            }
        }
        assert_eq!(manifest.file_count(), 2);
    }

    #[test]
    fn a_changed_byte_changes_the_digest() {
        // POSITIVE CONTROL for the digest. Without it, a manifest that recorded a constant would
        // satisfy every assertion above.
        let root = tree();
        let before = FileManifest::describe(root.path()).expect("describes");
        std::fs::write(root.path().join("src/main.rs"), b"fn main() { }\n").expect("write");
        let after = FileManifest::describe(root.path()).expect("describes");
        assert_ne!(before, after, "the manifest did not notice a content change");
    }

    #[test]
    fn a_symlink_in_the_staged_tree_is_refused_rather_than_described() {
        let root = tree();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc", root.path().join("link")).expect("symlink");
        #[cfg(unix)]
        {
            let error = FileManifest::describe(root.path()).unwrap_err();
            assert_eq!(error.code, Code::RenderFailed);
        }
    }
}
