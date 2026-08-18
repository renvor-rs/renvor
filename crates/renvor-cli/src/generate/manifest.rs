//! The file manifest — one artifact, three jobs.
//!
//! It is the `--dry-run` output, the pre-move verification input, and the reproducibility record.
//! Deliberately **one** structure: three would drift, and SC-006 requires the dry run to match
//! reality exactly. Data-model invariants I-9, I-10, I-11.

use cap_std::fs::Dir;
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
    /// # Symbolic links are not followed, and cannot be
    ///
    /// Invariant I-11. The manifest must describe what was **created**, not what a link points at.
    /// Following links would also make the manifest of a tree containing a link to `/etc` include
    /// `/etc`, which is both wrong and a disclosure.
    ///
    /// The walk descends through [`cap_std::fs::Dir`] handles rather than through paths, so this is
    /// not a policy the walk implements — a link out of the tree cannot be opened at all. The
    /// refusal below is therefore a *diagnosis*: it reports that something the generator did not
    /// create is present, which is itself worth failing on.
    ///
    /// # Errors
    ///
    /// [`Code::RenderFailed`] if the tree cannot be walked or a file cannot be read. The caller is
    /// still inside staging at this point, so the destination is untouched.
    pub fn describe(root: &Dir) -> Result<Self, CliError> {
        let mut entries = Vec::new();
        walk(root, "", &mut entries)?;
        // Sorted so the manifest is reproducible and diffable regardless of the order the
        // filesystem happened to yield entries in (invariant I-9). `read_dir` order is not
        // specified by any platform, so without this SC-016 would fail for reasons unrelated to
        // the generator.
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

/// One level of the walk, recursing through directory handles rather than through paths.
///
/// `prefix` is the forward-slashed path relative to the staging root, which is what the manifest
/// records — so a manifest produced on Windows compares equal to one produced on Linux, and SC-016
/// does not fail across platforms for a reason that has nothing to do with the generator.
fn walk(dir: &Dir, prefix: &str, entries: &mut Vec<Entry>) -> Result<(), CliError> {
    let failed = |what: &str, error: &dyn std::fmt::Display| {
        CliError::new(Code::RenderFailed, format!("{what}: {error}"))
    };

    let listing = dir
        .read_dir(".")
        .map_err(|error| failed("the generated tree could not be walked", &error))?;

    for item in listing {
        let item = item.map_err(|error| failed("the generated tree could not be walked", &error))?;
        let name = item.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };

        // `file_type` here comes from the directory entry and does NOT follow links, so a symlink
        // reports as a symlink rather than as whatever it points at.
        let kind = item
            .file_type()
            .map_err(|error| failed(&format!("`{relative}` could not be typed"), &error))?;

        if kind.is_dir() {
            entries.push(Entry { path: relative.clone(), kind: EntryKind::Directory, size: None, digest: None });
            let child = dir
                .open_dir(&name)
                .map_err(|error| failed(&format!("`{relative}` could not be opened"), &error))?;
            walk(&child, &relative, entries)?;
        } else if kind.is_file() {
            let bytes = dir
                .read(&name)
                .map_err(|error| failed(&format!("`{relative}` could not be read back"), &error))?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            entries.push(Entry {
                path: relative,
                kind: EntryKind::File,
                size: Some(bytes.len() as u64),
                digest: Some(hex(&hasher.finalize())),
            });
        } else {
            // A symlink or anything else. The generator creates none, so encountering one means
            // something else wrote into staging — refuse rather than describe it.
            return Err(CliError::new(
                Code::RenderFailed,
                format!(
                    "the staged tree contains `{relative}`, which is neither a file nor a \
                     directory; the generator creates neither, so something else wrote here"
                ),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_std::ambient_authority;

    struct Tree {
        base: tempfile::TempDir,
        dir: Dir,
    }

    fn tree() -> Tree {
        let base = tempfile::tempdir().expect("a temporary directory");
        let dir = Dir::open_ambient_dir(base.path(), ambient_authority()).expect("opens");
        dir.create_dir_all("src").expect("mkdir");
        dir.write("src/main.rs", b"fn main() {}\n").expect("write");
        dir.write("Cargo.toml", b"[package]\n").expect("write");
        Tree { base, dir }
    }

    #[test]
    fn entries_are_sorted_so_the_manifest_is_reproducible() {
        let tree = tree();
        let manifest = FileManifest::describe(&tree.dir).expect("describes");
        let paths = manifest.paths();
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        assert_eq!(paths, sorted, "the manifest must not depend on traversal order");
    }

    #[test]
    fn describing_the_same_tree_twice_gives_the_same_manifest() {
        // SC-016 in miniature. If this ever fails, reproducibility is decorative.
        let tree = tree();
        let first = FileManifest::describe(&tree.dir).expect("describes");
        let second = FileManifest::describe(&tree.dir).expect("describes");
        assert_eq!(first, second);
    }

    #[test]
    fn paths_are_relative_and_use_forward_slashes() {
        let tree = tree();
        let manifest = FileManifest::describe(&tree.dir).expect("describes");
        for entry in &manifest.entries {
            assert!(!entry.path.starts_with('/'), "{}", entry.path);
            assert!(!entry.path.contains('\\'), "{}", entry.path);
        }
        assert!(manifest.paths().contains(&"src/main.rs"));
    }

    #[test]
    fn files_carry_a_digest_and_directories_do_not() {
        let tree = tree();
        let manifest = FileManifest::describe(&tree.dir).expect("describes");
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
        let tree = tree();
        let before = FileManifest::describe(&tree.dir).expect("describes");
        tree.dir.write("src/main.rs", b"fn main() { }\n").expect("write");
        let after = FileManifest::describe(&tree.dir).expect("describes");
        assert_ne!(before, after, "the manifest did not notice a content change");
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_that_escapes_cannot_even_be_created_through_the_handle() {
        // Worth asserting rather than assuming: the containment is not only on reads. A template
        // cannot plant an escaping link for a later step to follow, because creating it fails.
        let tree = tree();
        let error = tree.dir.symlink("/etc", "link").unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied, "{error}");
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_planted_by_something_else_is_refused_rather_than_described() {
        // Planted with the AMBIENT api, because the handle refuses to create it — which is exactly
        // the scenario: the generator creates no symlink, so one being here means something other
        // than the generator wrote into staging.
        let tree = tree();
        std::os::unix::fs::symlink("/etc", tree.base.path().join("link")).expect("plant");
        let error = FileManifest::describe(&tree.dir).unwrap_err();
        assert_eq!(error.code, Code::RenderFailed);
        assert!(
            error.message.contains("link"),
            "the refusal must name the offending entry: {}",
            error.message
        );
    }

    #[test]
    #[cfg(unix)]
    fn a_symlink_that_stays_inside_the_tree_is_still_refused() {
        // `cap-std` permits this one — it does not escape — so the refusal here is THIS module's
        // rule, not the library's, and it needs its own test. A manifest that described it would
        // record a file whose bytes are counted twice and whose identity is a lie.
        let tree = tree();
        std::os::unix::fs::symlink("src", tree.base.path().join("alias")).expect("plant");
        let error = FileManifest::describe(&tree.dir).unwrap_err();
        assert_eq!(error.code, Code::RenderFailed);
        assert!(error.message.contains("alias"), "{}", error.message);
    }

    #[test]
    fn nested_directories_are_walked_to_the_bottom() {
        // The recursion, asserted. A walk that stopped at depth one would satisfy several of the
        // tests above while silently omitting most of a real project.
        let tree = tree();
        tree.dir.create_dir_all("a/b/c").expect("mkdir");
        tree.dir.write("a/b/c/deep.txt", b"deep").expect("write");
        let manifest = FileManifest::describe(&tree.dir).expect("describes");
        assert!(manifest.paths().contains(&"a/b/c/deep.txt"), "{:?}", manifest.paths());
        assert!(manifest.paths().contains(&"a/b/c"));
    }
}
