//! Rerun-safe writes into an existing project (Phase 011, FR-048).
//!
//! `renvor new` writes into a directory that did not exist. `renvor generate` writes into one
//! that does, beside files the user may have changed, so every target path is classified before
//! anything is written:
//!
//! | The target path is | Classified as | Effect |
//! |---|---|---|
//! | absent | [`Action::Write`] | written |
//! | present, byte-identical to the render | [`Action::Unchanged`] | nothing |
//! | present, different, **untouched since generation** — its digest equals the one `.renvor/generated.toml` recorded | [`Action::Regenerate`] | overwritten: the generator owns it |
//! | present, different, and changed since generation (or never generated) | a conflict | **nothing at all is written**; every such path is named |
//!
//! The provenance record is what makes "untouched" decidable without a network, a second copy, or
//! a guess: a digest equal to the recorded one says nobody edited the file since the generator
//! wrote it. A conflict is reported as `generation_conflict` before the first write, so a rerun
//! after a refusal starts from the tree it found.
//!
//! Writes go through a temporary sibling and a rename, one file at a time, after the whole plan
//! has passed; the record is rewritten last, the same way, with the new digests.

use cap_std::fs::Dir;
use sha2::{Digest as _, Sha256};

use super::record::{self, GeneratedFile, Record};
use crate::exit::{CliError, Code};

/// One file a generator wants to exist with these bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    /// Relative to the project root, forward slashes, no `..`.
    pub path: String,
    /// The bytes it should hold.
    pub bytes: Vec<u8>,
}

/// What the plan decided for one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Absent: it will be written.
    Write,
    /// Present and byte-identical: nothing to do.
    Unchanged,
    /// Present, different, and untouched since generation: it will be overwritten.
    Regenerate,
}

impl Action {
    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Unchanged => "unchanged",
            Self::Regenerate => "regenerate",
        }
    }
}

/// A plan every path of which passed classification.
#[derive(Debug)]
pub struct Plan {
    /// Every planned path with its decision, in the order given.
    pub decisions: Vec<(Planned, Action)>,
    record: Option<Record>,
}

impl Plan {
    /// The decisions as `(path, action)` pairs.
    #[must_use]
    pub fn summary(&self) -> Vec<(&str, Action)> {
        self.decisions
            .iter()
            .map(|(planned, action)| (planned.path.as_str(), *action))
            .collect()
    }

    /// How many files a commit would write.
    #[must_use]
    pub fn writes(&self) -> usize {
        self.decisions
            .iter()
            .filter(|(_, action)| *action != Action::Unchanged)
            .count()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let digest = Sha256::digest(bytes);
    digest
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}

/// Classifies every planned path against the project tree and its provenance record.
///
/// # Errors
///
/// [`Code::GenerationConflict`] naming every conflicting path, with nothing written;
/// [`Code::ManifestInvalid`] for a provenance record that does not parse; [`Code::RenderFailed`]
/// when a target cannot be read.
pub fn plan(project: &Dir, planned: Vec<Planned>) -> Result<Plan, CliError> {
    let record = record::read(project)?;
    let recorded = |path: &str| -> Option<&str> {
        record
            .as_ref()?
            .files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.sha256.as_str())
    };
    let mut decisions = Vec::with_capacity(planned.len());
    let mut conflicts: Vec<String> = Vec::new();
    for item in planned {
        let current = match project.read(&item.path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(CliError::new(
                    Code::RenderFailed,
                    format!("`{}` could not be read: {error}", item.path),
                ));
            }
        };
        let action = match current {
            None => Action::Write,
            Some(bytes) if bytes == item.bytes => Action::Unchanged,
            Some(bytes) => {
                if recorded(&item.path) == Some(sha256_hex(&bytes).as_str()) {
                    Action::Regenerate
                } else {
                    conflicts.push(item.path.clone());
                    Action::Unchanged
                }
            }
        };
        decisions.push((item, action));
    }
    if !conflicts.is_empty() {
        return Err(CliError::new(
            Code::GenerationConflict,
            format!(
                "{} file(s) exist and were changed since generation, so nothing was written: {}. \
                 Move or revert them, then run again",
                conflicts.len(),
                conflicts.join(", ")
            ),
        )
        .with("count", conflicts.len().to_string())
        .with("paths", conflicts.join(", ")));
    }
    Ok(Plan { decisions, record })
}

fn write_atomically(project: &Dir, path: &str, bytes: &[u8]) -> Result<(), CliError> {
    let failed = |what: &str, error: &dyn std::fmt::Display| {
        CliError::new(Code::RenderFailed, format!("`{path}` {what}: {error}"))
    };
    if let Some((parent, _)) = path.rsplit_once('/') {
        project
            .create_dir_all(parent)
            .map_err(|error| failed("could not have its directory created", &error))?;
    }
    let temporary = format!("{path}.renvor-{}.tmp", std::process::id());
    project
        .write(&temporary, bytes)
        .map_err(|error| failed("could not be staged beside its target", &error))?;
    project.rename(&temporary, project, path).map_err(|error| {
        let _ = project.remove_file(&temporary);
        failed("could not be moved into place", &error)
    })
}

/// Writes every non-`Unchanged` file through a temporary sibling and a rename, then rewrites the
/// provenance record with the new digests.
///
/// # Errors
///
/// [`Code::RenderFailed`] when a write or the rename fails; the files already committed stay.
pub fn commit(
    project: &Dir,
    plan: Plan,
    generator_version: &str,
    template_version: &str,
) -> Result<Vec<(String, Action)>, CliError> {
    let mut done = Vec::with_capacity(plan.decisions.len());
    let mut files: Vec<GeneratedFile> = plan
        .record
        .as_ref()
        .map(|r| r.files.clone())
        .unwrap_or_default();
    let (mut recorded_generator, mut recorded_template) = plan
        .record
        .as_ref()
        .map(|r| (r.generator_version.clone(), r.template_version.clone()))
        .unwrap_or_else(|| (generator_version.to_owned(), template_version.to_owned()));
    if plan.record.is_none() {
        recorded_generator = generator_version.to_owned();
        recorded_template = template_version.to_owned();
    }
    for (planned, action) in plan.decisions {
        if action != Action::Unchanged {
            write_atomically(project, &planned.path, &planned.bytes)?;
        }
        let digest = sha256_hex(&planned.bytes);
        match files.iter_mut().find(|file| file.path == planned.path) {
            Some(file) => file.sha256 = digest,
            None => files.push(GeneratedFile {
                path: planned.path.clone(),
                sha256: digest,
            }),
        }
        done.push((planned.path, action));
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let text = record::render(&recorded_generator, &recorded_template, &files);
    write_atomically(project, record::PATH, text.as_bytes())?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &[u8])]) -> (tempfile::TempDir, Dir) {
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

    fn planned(path: &str, bytes: &[u8]) -> Planned {
        Planned {
            path: path.to_owned(),
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn absent_identical_untouched_and_modified_are_the_four_cases() {
        let (_keep, dir) = project(&[
            ("same.txt", b"same\n"),
            ("untouched.txt", b"old render\n"),
            ("edited.txt", b"the user's version\n"),
        ]);
        // The record says `untouched.txt` and `edited.txt` were both generated as "old render".
        record::write(&dir, "0.0.0", "7").expect("record");
        let mut rec = record::read(&dir).expect("reads").expect("present");
        let old = sha256_hex(b"old render\n");
        for file in &mut rec.files {
            if file.path == "edited.txt" {
                file.sha256 = old.clone();
            }
        }
        dir.write(
            record::PATH,
            record::render(&rec.generator_version, &rec.template_version, &rec.files).as_bytes(),
        )
        .expect("rewrite");

        let ok = plan(
            &dir,
            vec![
                planned("new.txt", b"new\n"),
                planned("same.txt", b"same\n"),
                planned("untouched.txt", b"new render\n"),
            ],
        )
        .expect("no conflict");
        assert_eq!(
            ok.summary(),
            vec![
                ("new.txt", Action::Write),
                ("same.txt", Action::Unchanged),
                ("untouched.txt", Action::Regenerate),
            ]
        );
        assert_eq!(ok.writes(), 2);

        let error = plan(
            &dir,
            vec![
                planned("new.txt", b"new\n"),
                planned("edited.txt", b"new render\n"),
            ],
        )
        .expect_err("an edited file is a conflict");
        assert_eq!(error.code, Code::GenerationConflict);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "paths" && v == "edited.txt")
        );
        assert!(
            !dir.exists("new.txt"),
            "a conflict must write NOTHING, not even the files that could have been written"
        );
    }

    #[test]
    fn a_file_never_generated_is_a_conflict_even_without_a_record() {
        let (_keep, dir) = project(&[("theirs.txt", b"theirs\n")]);
        let error = plan(&dir, vec![planned("theirs.txt", b"ours\n")]).expect_err("conflict");
        assert_eq!(error.code, Code::GenerationConflict);
    }

    #[test]
    fn a_commit_writes_through_a_sibling_and_records_every_digest() {
        let (keep, dir) = project(&[("same.txt", b"same\n")]);
        let ok = plan(
            &dir,
            vec![
                planned("deep/new.txt", b"new\n"),
                planned("same.txt", b"same\n"),
            ],
        )
        .expect("plans");
        let done = commit(&dir, ok, "0.0.0", "7").expect("commits");
        assert_eq!(
            done,
            vec![
                ("deep/new.txt".to_owned(), Action::Write),
                ("same.txt".to_owned(), Action::Unchanged),
            ]
        );
        assert_eq!(
            std::fs::read(keep.path().join("deep/new.txt")).expect("read"),
            b"new\n"
        );
        let rec = record::read(&dir).expect("reads").expect("present");
        let paths: Vec<&str> = rec.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["deep/new.txt", "same.txt"]);
        assert_eq!(rec.files[0].sha256, sha256_hex(b"new\n"));
        // No temporary sibling survives.
        let leftovers: Vec<_> = std::fs::read_dir(keep.path().join("deep"))
            .expect("read_dir")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, ["new.txt"]);
        // A second commit of the same plan is a no-op that keeps the record.
        let again = plan(&dir, vec![planned("deep/new.txt", b"new\n")]).expect("plans");
        assert_eq!(again.summary(), vec![("deep/new.txt", Action::Unchanged)]);
    }
}
