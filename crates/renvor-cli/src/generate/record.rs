//! The provenance record every generated project carries: `.renvor/generated.toml`.
//!
//! Contract `template-contract.md` §"The provenance record" (Phase 011, FR-044). The generator
//! version, the template version, and one `[[file]]` per generated path with its SHA-256 — so a
//! later generator can tell an untouched file from one the user changed **without downloading or
//! evaluating anything**. Digests only, never contents; and the record is itself generated, so it
//! appears in the manifest like every other file.
//!
//! # Written after verification, before the manifest walk
//!
//! It lists what the staging tree holds at the moment it is written, which is why it is the last
//! file generation produces: after rendering, after the seeded lockfile, **after verification** —
//! which resolves `Cargo.lock` to the project's own closure, or creates it for a skeleton — and
//! before the manifest. A record written before verification digested a lockfile that no longer
//! existed by the time the project did; one written after the manifest would be absent from it.

use std::fmt::Write as _;

use cap_std::fs::Dir;
use serde::{Deserialize, Serialize};

use super::manifest::{EntryKind, FileManifest};
use crate::exit::{CliError, Code};

/// Where the record lives, relative to the project root.
pub const DIRECTORY: &str = ".renvor";
/// The record's path, relative to the project root, with forward slashes.
pub const PATH: &str = ".renvor/generated.toml";

/// One generated file: its path and the SHA-256 of its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// The path relative to the project root, forward slashes.
    pub path: String,
    /// The hexadecimal SHA-256 of the file as generated.
    pub sha256: String,
}

/// One resource `renvor generate resource` rendered: what it needs to render it again.
///
/// A digest says whether a module was touched; it cannot say what the module was rendered
/// **from**. `renvor generate auth` re-renders every untouched resource module with the session
/// guards the manifest now promises, and for that it needs the name and the fields exactly as
/// they were given (found by the Codex review of Phase 011).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeneratedResource {
    /// The PascalCase type name.
    pub name: String,
    /// The fields as given: `name:type`, in order.
    pub fields: Vec<String>,
}

/// The record as read back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// The `renvor` that generated the project.
    pub generator_version: String,
    /// The template version the project was rendered from.
    pub template_version: String,
    /// Every file generation produced, sorted by path; the record itself is not listed.
    #[serde(rename = "file", default)]
    pub files: Vec<GeneratedFile>,
    /// Every resource a generator rendered, by name.
    #[serde(rename = "resource", default)]
    pub resources: Vec<GeneratedResource>,
}

/// Writes the record for the tree under `root`.
///
/// # Errors
///
/// [`Code::RenderFailed`] when the tree cannot be read or the record cannot be written.
pub fn write(root: &Dir, generator_version: &str, template_version: &str) -> Result<(), CliError> {
    let manifest = FileManifest::describe(root)?;
    let files: Vec<GeneratedFile> = manifest
        .entries
        .iter()
        .filter(|entry| entry.kind == EntryKind::File && entry.path != PATH)
        .filter_map(|entry| {
            entry.digest.as_ref().map(|digest| GeneratedFile {
                path: entry.path.clone(),
                sha256: digest.clone(),
            })
        })
        .collect();
    let text = render(generator_version, template_version, &files, &[]);
    root.create_dir_all(DIRECTORY).map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!("the provenance directory `{DIRECTORY}` could not be created: {error}"),
        )
    })?;
    root.write(PATH, text.as_bytes()).map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!("the provenance record `{PATH}` could not be written: {error}"),
        )
    })
}

/// The record's text for these entries: a header, the two versions, one `[[file]]` per entry,
/// and one `[[resource]]` per generated resource.
#[must_use]
pub fn render(
    generator_version: &str,
    template_version: &str,
    files: &[GeneratedFile],
    resources: &[GeneratedResource],
) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "# Written by `renvor new`; read by `renvor generate` to tell a file you changed from one\n\
         # it generated. Digests only. Keep it with the project."
    );
    let _ = writeln!(text, "generator_version = {generator_version:?}");
    let _ = writeln!(text, "template_version = {template_version:?}");
    for file in files {
        let _ = writeln!(
            text,
            "\n[[file]]\npath = {:?}\nsha256 = {:?}",
            file.path, file.sha256
        );
    }
    for resource in resources {
        let _ = writeln!(
            text,
            "\n[[resource]]\nname = {:?}\nfields = {:?}",
            resource.name, resource.fields
        );
    }
    text
}

/// Reads the record under `root`, or `None` when the project carries none.
///
/// # Errors
///
/// [`Code::ManifestInvalid`] when the record exists and does not parse.
pub fn read(root: &Dir) -> Result<Option<Record>, CliError> {
    let text = match root.read_to_string(PATH) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CliError::new(
                Code::ManifestInvalid,
                format!("the provenance record `{PATH}` could not be read: {error}"),
            )
            .with("field", PATH));
        }
    };
    toml::from_str(&text).map(Some).map_err(|error| {
        CliError::new(
            Code::ManifestInvalid,
            format!("the provenance record `{PATH}` does not parse: {error}"),
        )
        .with("field", PATH)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(files: &[(&str, &[u8])]) -> (tempfile::TempDir, Dir) {
        let temporary = tempfile::tempdir().expect("tempdir");
        for (path, bytes) in files {
            let full = temporary.path().join(path);
            std::fs::create_dir_all(full.parent().expect("a parent")).expect("mkdir");
            std::fs::write(full, bytes).expect("write");
        }
        let dir =
            Dir::open_ambient_dir(temporary.path(), cap_std::ambient_authority()).expect("opens");
        (temporary, dir)
    }

    #[test]
    fn the_record_names_every_file_with_its_digest_and_not_itself() {
        let (_keep, dir) = tree(&[
            ("Cargo.toml", b"[package]\n"),
            ("src/main.rs", b"fn main() {}\n"),
        ]);
        write(&dir, "0.0.0", "7").expect("written");
        let record = read(&dir).expect("reads").expect("present");
        assert_eq!(record.generator_version, "0.0.0");
        assert_eq!(record.template_version, "7");
        let paths: Vec<&str> = record.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            ["Cargo.toml", "src/main.rs"],
            "sorted, and never the record"
        );
        // The digests are the manifest's, so a reader agrees with `renvor new --output json`.
        let manifest = FileManifest::describe(&dir).expect("manifest");
        for file in &record.files {
            let entry = manifest
                .entries
                .iter()
                .find(|entry| entry.path == file.path)
                .expect("listed");
            assert_eq!(entry.digest.as_deref(), Some(file.sha256.as_str()));
        }
        assert!(
            manifest.entries.iter().any(|entry| entry.path == PATH),
            "the record is itself part of the tree the manifest describes"
        );
    }

    #[test]
    fn the_record_carries_the_resources_a_generator_defined() {
        // FOUND BY THE CODEX REVIEW (P1). A resource module is rendered from its name and its
        // fields; a later `renvor generate auth` has to render it again with the session guards,
        // and digests cannot say what the fields were. The record therefore carries one
        // `[[resource]]` per generated resource, and reads it back.
        let resources = vec![GeneratedResource {
            name: "Post".to_owned(),
            fields: vec!["title:string".to_owned(), "published:boolean".to_owned()],
        }];
        let text = render("0.0.0", "7", &[], &resources);
        assert!(text.contains("[[resource]]"), "{text}");
        let (_keep, dir) = tree(&[]);
        dir.create_dir_all(DIRECTORY).expect("mkdir");
        dir.write(PATH, text.as_bytes()).expect("write");
        let record = read(&dir).expect("reads").expect("present");
        assert_eq!(record.resources, resources);
        // A record without the table — every record `renvor new` writes — reads as none.
        dir.write(PATH, render("0.0.0", "7", &[], &[]).as_bytes())
            .expect("write");
        assert!(
            read(&dir)
                .expect("reads")
                .expect("present")
                .resources
                .is_empty()
        );
    }

    #[test]
    fn a_tree_without_a_record_reads_as_none_and_a_broken_one_is_refused() {
        let (_keep, dir) = tree(&[("Cargo.toml", b"[package]\n")]);
        assert!(read(&dir).expect("reads").is_none());
        dir.create_dir_all(DIRECTORY).expect("mkdir");
        dir.write(PATH, b"generator_version = 1\n").expect("write");
        let error = read(&dir).expect_err("a record that does not parse is refused");
        assert_eq!(error.code, Code::ManifestInvalid);
    }
}
