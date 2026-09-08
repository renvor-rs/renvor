//! The tree digest: the verified tree, compared by contents (Phase 012, FR-012-5d, D-L2-10).
//!
//! A `[verified_with]` table describes the five checks as they ran over one tree. The moment a
//! file those checks compiled or read changes, the evidence describes a tree that no longer
//! exists — so the record carries a digest of that tree, and `renvor check` recomputes it over
//! the working tree and says *current* or *historical*. The comparison is by **contents**, never
//! by a counter: nothing in the tree records how many operations ran since the verification, and
//! a number nothing supports is not printed.
//!
//! # The scope is a pattern over the tree, not the `[[file]]` list
//!
//! Scope 1 is what the five checks compile or read: the six top-level files of [`SCOPE_FILES`]
//! and everything under the five directories of [`SCOPE_DIRECTORIES`]. A file added under `src/`
//! or deleted from `tests/` therefore changes the digest, whether or not the generator ever knew
//! it. Everything else — `README.md`, `Dockerfile`, `.dockerignore`, `.gitignore`, `.env*`, the
//! `.renvor/` directory (the record and the manifest themselves), `target/` — is outside it.
//!
//! # Freshness is not ownership
//!
//! Contract C-4's managed blocks decide what the generator owns and may rewrite. For freshness
//! the whole file's bytes count: an edit inside a managed block and an edit outside one both
//! change compiled source, and both make the evidence historical. Ownership never hides a change.
//!
//! # The line format
//!
//! One line per regular file in scope, `<relative path>\0<sha256 of the bytes>\n`, forward
//! slashes; a symlink in scope contributes `<relative path>\0symlink:<link text>\n` and is never
//! followed. The lines are sorted bytewise and concatenated, and the digest is `sha256:` followed
//! by the hexadecimal SHA-256 of that text. The walk goes through [`cap_std::fs::Dir`] handles,
//! so nothing outside the project root can be read even by a symlink that points there.

use std::fmt::Write as _;

use cap_std::fs::Dir;
use sha2::{Digest as _, Sha256};

use crate::exit::{CliError, Code};
use crate::generate::record;
use crate::toolchain::TREE_SCOPE;

/// The scope rule this generator computes and reads (FR-012-5d).
pub const SCOPE: u32 = TREE_SCOPE;

/// The top-level files of scope 1: what the checks read without walking.
pub const SCOPE_FILES: [&str; 6] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "build.rs",
    "renvor.toml",
    ".cargo/config.toml",
];

/// The directories of scope 1, walked in full.
pub const SCOPE_DIRECTORIES: [&str; 5] = ["src", "tests", "benches", "examples", "migrations"];

/// The scope-1 digest of the tree under `root`: `sha256:<hex>`.
///
/// # Errors
///
/// [`Code::RenderFailed`] when a path in scope cannot be read.
pub fn tree(root: &Dir) -> Result<String, CliError> {
    tree_under(root, SCOPE)
}

/// The digest of the tree under `root` computed under the scope rule `scope`.
///
/// # Errors
///
/// [`Code::RecordUnsupported`] for a scope this generator does not know — a record written by a
/// newer generator, refused by name (`details.tree_scope`, `details.supported`);
/// [`Code::RenderFailed`] when a path in scope cannot be read.
pub fn tree_under(root: &Dir, scope: u32) -> Result<String, CliError> {
    if scope != SCOPE {
        return Err(CliError::new(
            Code::RecordUnsupported,
            format!(
                "the provenance record `{}` was digested under tree scope {scope}; this renvor \
                 knows scope {SCOPE} — rebuild the generator, not the project",
                record::PATH
            ),
        )
        .with("tree_scope", scope.to_string())
        .with("supported", SCOPE.to_string())
        .with("field", record::PATH));
    }
    let mut lines = Vec::new();
    for path in SCOPE_FILES {
        entry(root, path, &mut lines)?;
    }
    for directory in SCOPE_DIRECTORIES {
        entry(root, directory, &mut lines)?;
    }
    lines.sort();
    let mut hasher = Sha256::new();
    for line in &lines {
        hasher.update(line);
    }
    Ok(format!("sha256:{}", hex(&hasher.finalize())))
}

fn failed(relative: &str, what: &str, error: &dyn std::fmt::Display) -> CliError {
    CliError::new(
        Code::RenderFailed,
        format!("the project tree could not be digested: `{relative}` {what}: {error}"),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// One line for the regular file or symlink at `relative`, or the walk of the directory there;
/// nothing for a path that is absent or is neither.
fn entry(root: &Dir, relative: &str, lines: &mut Vec<Vec<u8>>) -> Result<(), CliError> {
    // `symlink_metadata` does not follow links, so a symlink is seen as one.
    let metadata = match root.symlink_metadata(relative) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(failed(relative, "could not be examined", &error)),
    };
    let kind = metadata.file_type();
    if kind.is_symlink() {
        // `read_link_contents`, not `read_link`: the latter resolves and refuses a target outside
        // the sandbox, and the digest records the link TEXT, never where it leads.
        let target = root
            .read_link_contents(relative)
            .map_err(|error| failed(relative, "is a symlink that could not be read", &error))?;
        lines.push(symlink_line(relative, &target.to_string_lossy()));
    } else if kind.is_dir() {
        let directory = root
            .open_dir(relative)
            .map_err(|error| failed(relative, "could not be opened", &error))?;
        walk(&directory, relative, lines)?;
    } else if kind.is_file() {
        let bytes = root
            .read(relative)
            .map_err(|error| failed(relative, "could not be read", &error))?;
        lines.push(file_line(relative, &bytes));
    }
    Ok(())
}

fn file_line(relative: &str, bytes: &[u8]) -> Vec<u8> {
    format!("{relative}\0{}\n", hex(&Sha256::digest(bytes))).into_bytes()
}

fn symlink_line(relative: &str, target: &str) -> Vec<u8> {
    format!("{relative}\0symlink:{target}\n").into_bytes()
}

/// One level of the walk, through directory handles rather than paths.
fn walk(dir: &Dir, prefix: &str, lines: &mut Vec<Vec<u8>>) -> Result<(), CliError> {
    let listing = dir
        .read_dir(".")
        .map_err(|error| failed(prefix, "could not be walked", &error))?;
    for item in listing {
        let item = item.map_err(|error| failed(prefix, "could not be walked", &error))?;
        let name = item.file_name().to_string_lossy().into_owned();
        let relative = format!("{prefix}/{name}");
        // From the directory entry: a symlink reports as a symlink, not as its target.
        let kind = item
            .file_type()
            .map_err(|error| failed(&relative, "could not be typed", &error))?;
        if kind.is_symlink() {
            let target = dir.read_link_contents(&name).map_err(|error| {
                failed(&relative, "is a symlink that could not be read", &error)
            })?;
            lines.push(symlink_line(&relative, &target.to_string_lossy()));
        } else if kind.is_dir() {
            let child = dir
                .open_dir(&name)
                .map_err(|error| failed(&relative, "could not be opened", &error))?;
            walk(&child, &relative, lines)?;
        } else if kind.is_file() {
            let bytes = dir
                .read(&name)
                .map_err(|error| failed(&relative, "could not be read", &error))?;
            lines.push(file_line(&relative, &bytes));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree {
        base: tempfile::TempDir,
        dir: Dir,
    }

    impl Tree {
        fn write(&self, path: &str, bytes: &[u8]) {
            let full = self.base.path().join(path);
            std::fs::create_dir_all(full.parent().expect("a parent")).expect("mkdir");
            std::fs::write(full, bytes).expect("write");
        }

        fn remove(&self, path: &str) {
            std::fs::remove_file(self.base.path().join(path)).expect("remove");
        }

        fn digest(&self) -> String {
            tree(&self.dir).expect("digests")
        }
    }

    /// A small project: the six scope files that a starter has, sources, a test, a migration,
    /// and the out-of-scope files beside them.
    fn project() -> Tree {
        let base = tempfile::tempdir().expect("tempdir");
        let dir = Dir::open_ambient_dir(base.path(), cap_std::ambient_authority()).expect("opens");
        let tree = Tree { base, dir };
        tree.write("Cargo.toml", b"[package]\nname = \"demo\"\n");
        tree.write("Cargo.lock", b"# lock\n");
        tree.write(
            "rust-toolchain.toml",
            b"[toolchain]\nchannel = \"1.94.0\"\n",
        );
        tree.write("renvor.toml", b"[renvor]\n");
        tree.write(".cargo/config.toml", b"[build]\n");
        tree.write(
            "src/main.rs",
            b"// renvor:resources:begin\n// renvor:resources:end\nfn main() {}\n",
        );
        tree.write("src/routes.rs", b"pub fn routes() {}\n");
        tree.write("tests/smoke.rs", b"#[test]\nfn smoke() {}\n");
        tree.write("migrations/0001_init.up.sql", b"CREATE TABLE t (id INT);\n");
        tree.write("README.md", b"# demo\n");
        tree.write("Dockerfile", b"FROM scratch\n");
        tree.write(".gitignore", b"target\n");
        tree.write(".env.example", b"KEY=\n");
        tree.write(".renvor/generated.toml", b"generator_version = \"0.0.0\"\n");
        tree.write("target/debug/demo", b"binary\n");
        tree
    }

    #[test]
    fn the_scope_list_is_pinned() {
        // FR-012-5d names the scope exactly; a change here is a change to the contract and to
        // every digest ever written, so it is spelled out rather than sampled.
        assert_eq!(SCOPE, 1, "scope 1 is the only rule this generator knows");
        assert_eq!(
            SCOPE, TREE_SCOPE,
            "the record and the digest agree on the scope"
        );
        assert_eq!(
            SCOPE_FILES,
            [
                "Cargo.toml",
                "Cargo.lock",
                "rust-toolchain.toml",
                "build.rs",
                "renvor.toml",
                ".cargo/config.toml",
            ],
            "the six top-level files of scope 1"
        );
        assert_eq!(
            SCOPE_DIRECTORIES,
            ["src", "tests", "benches", "examples", "migrations"],
            "the five directories of scope 1"
        );
    }

    #[test]
    fn the_digest_matches_an_independent_computation() {
        // Computed OUTSIDE this crate (Python's hashlib, 2026-09-07) over the two lines
        // `Cargo.toml\0sha256("a\n")\n` and `src/main.rs\0sha256("b\n")\n` in sorted order. A
        // digest test that only compares the function to itself would pass for any function.
        let base = tempfile::tempdir().expect("tempdir");
        let dir = Dir::open_ambient_dir(base.path(), cap_std::ambient_authority()).expect("opens");
        let tree = Tree { base, dir };
        tree.write("Cargo.toml", b"a\n");
        tree.write("src/main.rs", b"b\n");
        assert_eq!(
            tree.digest(),
            "sha256:584b998ca98c2158cdba27d63249f94670aeb44300bf999fd6aabbdcd3385cd3",
            "the digest is the SHA-256 of the sorted, NUL-separated path/digest lines"
        );
    }

    #[test]
    fn an_untouched_tree_digests_stably() {
        let tree = project();
        let first = tree.digest();
        assert!(
            first.starts_with("sha256:"),
            "the digest names its algorithm"
        );
        assert_eq!(first.len(), "sha256:".len() + 64, "a hexadecimal SHA-256");
        assert_eq!(first, tree.digest(), "the same tree digests the same twice");
    }

    #[test]
    fn one_byte_inside_a_managed_block_changes_the_digest() {
        // Freshness is not ownership: the whole file's bytes count, managed block or not.
        let tree = project();
        let before = tree.digest();
        tree.write(
            "src/main.rs",
            b"// renvor:resources:begin\nx\n// renvor:resources:end\nfn main() {}\n",
        );
        assert_ne!(
            before,
            tree.digest(),
            "an edit inside a managed block is a change"
        );
    }

    #[test]
    fn one_byte_outside_a_managed_block_changes_the_digest() {
        let tree = project();
        let before = tree.digest();
        tree.write(
            "src/main.rs",
            b"// renvor:resources:begin\n// renvor:resources:end\nfn main() { }\n",
        );
        assert_ne!(
            before,
            tree.digest(),
            "an edit outside a managed block is a change"
        );
    }

    #[test]
    fn a_file_added_under_src_changes_the_digest() {
        let tree = project();
        let before = tree.digest();
        tree.write("src/extra.rs", b"pub fn extra() {}\n");
        assert_ne!(
            before,
            tree.digest(),
            "the scope is a pattern, not a file list"
        );
    }

    #[test]
    fn a_file_deleted_from_tests_changes_the_digest() {
        let tree = project();
        let before = tree.digest();
        tree.remove("tests/smoke.rs");
        assert_ne!(before, tree.digest(), "a deletion is a change");
    }

    #[test]
    fn every_scope_file_and_directory_counts() {
        // Each of the eleven scope entries, one at a time — so a name dropped from the lists
        // above fails here by name (by index), not silently.
        let files = [
            ("Cargo.toml", "Cargo.toml"),
            ("Cargo.lock", "Cargo.lock"),
            ("rust-toolchain.toml", "rust-toolchain.toml"),
            ("build.rs", "build.rs"),
            ("renvor.toml", "renvor.toml"),
            (".cargo/config.toml", ".cargo/config.toml"),
            ("src", "src/added.rs"),
            ("tests", "tests/added.rs"),
            ("benches", "benches/added.rs"),
            ("examples", "examples/added.rs"),
            ("migrations", "migrations/0002_added.up.sql"),
        ];
        for (index, (_, path)) in files.iter().enumerate() {
            let tree = project();
            let before = tree.digest();
            tree.write(path, b"changed\n");
            assert_ne!(
                before,
                tree.digest(),
                "a scope entry did not count; see the table by index: {index}"
            );
        }
    }

    #[test]
    fn a_readme_edit_does_not_change_the_digest() {
        let tree = project();
        let before = tree.digest();
        tree.write("README.md", b"# demo, edited\n");
        assert_eq!(before, tree.digest(), "README.md is outside the scope");
    }

    #[test]
    fn the_record_the_target_directory_and_the_dotfiles_are_outside_the_scope() {
        let tree = project();
        let before = tree.digest();
        tree.write(".renvor/generated.toml", b"generator_version = \"9.9.9\"\n");
        tree.write(".renvor/manifest.json", b"{}\n");
        tree.write("target/debug/demo", b"other binary\n");
        tree.write("Dockerfile", b"FROM other\n");
        tree.write(".dockerignore", b"target\n");
        tree.write(".gitignore", b"target\n.env\n");
        tree.write(".env", b"SECRET=1\n");
        tree.write(".env.example", b"KEY=value\n");
        tree.write("notes.txt", b"not compiled\n");
        assert_eq!(before, tree.digest(), "nothing outside the scope counts");
    }

    #[test]
    fn a_missing_scope_file_is_not_an_error() {
        // A skeleton has no `build.rs`, no `.cargo/`, no `benches/`; absence is a fact about the
        // tree, not a failure of the digest.
        let base = tempfile::tempdir().expect("tempdir");
        let dir = Dir::open_ambient_dir(base.path(), cap_std::ambient_authority()).expect("opens");
        let tree = Tree { base, dir };
        tree.write("Cargo.toml", b"[package]\n");
        assert!(tree.digest().starts_with("sha256:"));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_recorded_by_its_link_text_and_never_followed() {
        let tree = project();
        let before = tree.digest();
        std::os::unix::fs::symlink("../README.md", tree.base.path().join("src/linked.rs"))
            .expect("symlink");
        let with_link = tree.digest();
        assert_ne!(before, with_link, "a symlink in scope counts");
        // The TARGET's contents are not what is recorded: editing the file the link points at
        // (outside the scope) leaves the digest alone …
        tree.write("README.md", b"# demo, edited\n");
        assert_eq!(with_link, tree.digest(), "the link is never followed");
        // … while a different link text is a different line.
        std::fs::remove_file(tree.base.path().join("src/linked.rs")).expect("unlink");
        std::os::unix::fs::symlink("../Dockerfile", tree.base.path().join("src/linked.rs"))
            .expect("symlink");
        assert_ne!(
            with_link,
            tree.digest(),
            "the link text is what is recorded"
        );
        // A link that leaves the project is recorded by its text too, and cannot be read through.
        std::fs::remove_file(tree.base.path().join("src/linked.rs")).expect("unlink");
        std::os::unix::fs::symlink("/etc/hostname", tree.base.path().join("src/linked.rs"))
            .expect("symlink");
        assert!(
            tree.digest().starts_with("sha256:"),
            "an escaping link is text, not a read"
        );
    }

    #[test]
    fn an_unknown_scope_is_refused_by_name() {
        let tree = project();
        let error = tree_under(&tree.dir, 7).expect_err("scope 7 is not known");
        assert_eq!(error.code, Code::RecordUnsupported);
        let detail = |key: &str| {
            error
                .details
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(detail("tree_scope"), Some("7"));
        assert_eq!(detail("supported"), Some("1"));
        assert_eq!(detail("field"), Some(record::PATH));
        assert!(
            error
                .message
                .contains("rebuild the generator, not the project"),
            "the remedy is named"
        );
        // POSITIVE CONTROL: the known scope is the plain digest.
        assert_eq!(
            tree_under(&tree.dir, SCOPE).expect("scope 1"),
            tree.digest()
        );
    }
}
