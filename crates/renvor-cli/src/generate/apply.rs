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
//! | present, different, **unchanged since generation** — its digest equals the one `.renvor/generated.toml` recorded — and `--overwrite-unchanged` was given | [`Action::Regenerate`] | replaced: the generator owns it, and the operator said so |
//! | the same, **without** the flag | refused, [`Refusal::OverwriteRequired`] | **nothing at all is written**; the path and the flag are named |
//! | present, different, and changed since generation (or never generated) | refused, [`Refusal::Changed`] | **nothing at all is written**, flag or no flag; every such path is named |
//! | an [`Planned::edit`] of an existing file's managed region | [`Action::Edit`] | the region is written; the rest of the file is kept |
//!
//! The provenance record is what makes "unchanged" decidable without a network, a second copy,
//! or a guess: a digest equal to the recorded one says nobody edited the file since the generator
//! wrote it. That proves ownership; it does not grant permission to replace, which is the flag's
//! (FR-048 as decided on 2026-09-05: a differing file is never overwritten implicitly). A refusal
//! is reported as `generation_conflict` before the first write, so a rerun after a refusal starts
//! from the tree it found — and a dry run classifies exactly as a real run does.
//!
//! Writes are staged as temporary siblings first and renamed into place second, after the whole
//! plan has passed; a failure at either point leaves the project as it was found ([`commit`]).
//! The record is rewritten last, the same way, with the new digests — over the file with its
//! marked block emptied for the two marked files ([`provenance_digest`]).

use cap_std::fs::Dir;
use sha2::{Digest as _, Sha256};

use super::record::{self, GeneratedFile, GeneratedResource, Record, Toolchain, VerifiedWith};
use crate::exit::{CliError, Code};
use crate::toolchain::RECORD_VERSION;

/// One file a generator wants to exist with these bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    /// Relative to the project root, forward slashes, no `..`.
    pub path: String,
    /// The bytes it should hold.
    pub bytes: Vec<u8>,
    /// An **edit** of a file that must already exist — the marked block of `src/routes.rs`, say —
    /// which is written whether or not the user changed the file elsewhere, because the marked
    /// block is the contract. Never a conflict; a missing file is a `render_failed`.
    pub edit: bool,
}

impl Planned {
    /// A file the generator owns.
    #[must_use]
    pub fn file(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            bytes,
            edit: false,
        }
    }

    /// An edit of an existing file's marked block.
    #[must_use]
    pub fn edit(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            path: path.into(),
            bytes,
            edit: true,
        }
    }
}

/// What the plan decided for one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Absent: it will be written.
    Write,
    /// Present and byte-identical: nothing to do.
    Unchanged,
    /// Present, different, unchanged since generation, and `--overwrite-unchanged` given: it
    /// will be replaced.
    Regenerate,
    /// A marked block of an existing file will be edited.
    Edit,
}

/// Why a target path refused the whole plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Present, different, and its digest is not the recorded one — or the record does not list
    /// it: the user changed it, or nobody knows who owns it. Never overwritten, flag or no flag.
    Changed,
    /// Present, different, and unchanged since generation — regenerable — but
    /// [`OVERWRITE_FLAG`] was not given.
    OverwriteRequired,
}

/// The flag that lets a regenerable file be replaced, spelled once.
pub const OVERWRITE_FLAG: &str = "--overwrite-unchanged";

impl Action {
    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Unchanged => "unchanged",
            Self::Regenerate => "regenerate",
            Self::Edit => "edit",
        }
    }
}

/// A plan every path of which passed classification.
#[derive(Debug)]
pub struct Plan {
    /// Every planned path with its decision, in the order given.
    pub decisions: Vec<(Planned, Action)>,
    record: Option<Record>,
    /// Resource definitions the commit records beside the files, replacing a same-named one.
    resources: Vec<GeneratedResource>,
    /// The two evidence tables an operation that **verified** writes in place of what the record
    /// held (FR-012-5a): `generate auth`. `None` for every other operation, which carries the
    /// record's tables through byte-identically.
    replacement: Option<(Toolchain, VerifiedWith)>,
}

impl Plan {
    /// Records `resource` when the plan commits, so a later generator can render it again.
    #[must_use]
    pub fn with_resource(mut self, resource: GeneratedResource) -> Self {
        self.resources.push(resource);
        self
    }

    /// Replaces the record's `[toolchain]` and `[verified_with]` when the plan commits — for an
    /// operation that ran the five checks (`generate auth`, FR-012-5a) and measured
    /// `verified_with` itself. A legacy record becomes a version-2 record. Never for `resource`
    /// or `migration`, which verify nothing and may write no evidence.
    // integration.
    #[must_use]
    pub fn with_verified_with(mut self, toolchain: Toolchain, verified_with: VerifiedWith) -> Self {
        self.replacement = Some((toolchain, verified_with));
        self
    }

    /// Adds an edit of the existing `path` to `bytes` after classification — for a file whose
    /// contents the plan can only know by verifying the rest of it, such as the resolved
    /// lockfile. `Unchanged` when the file already holds these bytes.
    ///
    /// # Errors
    ///
    /// [`Code::RenderFailed`] when `path` cannot be read; an edit needs its file.
    pub fn with_edit(
        mut self,
        project: &Dir,
        path: &str,
        bytes: Vec<u8>,
    ) -> Result<Self, CliError> {
        let current = project.read(path).map_err(|error| {
            CliError::new(
                Code::RenderFailed,
                format!("`{path}` could not be read, so it cannot be edited: {error}"),
            )
        })?;
        let action = if current == bytes {
            Action::Unchanged
        } else {
            Action::Edit
        };
        self.decisions.push((Planned::edit(path, bytes), action));
        Ok(self)
    }

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

/// Files whose marked blocks other generators fill, and the markers that bound them.
///
/// The block between the markers is the generators' shared zone — `renvor generate resource`
/// adds a line to it, `renvor generate auth` carries it across a re-render — so it is **not**
/// part of what the provenance record digests for these two files: see [`provenance_digest`].
pub const MARKED: [(&str, &str, &str); 2] = [
    (
        "src/resources/mod.rs",
        "// renvor:resources:modules:begin",
        "// renvor:resources:modules:end",
    ),
    (
        "src/routes.rs",
        "// renvor:resources:begin",
        "// renvor:resources:end",
    ),
];

/// The bytes a file is digested over for the provenance record: the file itself, or — for a
/// marked file — the file with the lines between its markers removed.
///
/// A marker edit changes only the block, so the digest of a marked file must not change with
/// it; otherwise the edit would record the whole merged file, and a line the user added
/// **outside** the markers would read as generator-owned on the next full re-render and be
/// overwritten (found by the Codex review of Phase 011). With an empty block the result is the
/// file itself, which is why `renvor new`'s record — a plain digest of every rendered file — needs
/// no special case.
fn provenance_bytes<'a>(path: &str, bytes: &'a [u8]) -> std::borrow::Cow<'a, [u8]> {
    use std::borrow::Cow;
    let Some((_, begin, end)) = MARKED.iter().find(|(marked, _, _)| *marked == path) else {
        return Cow::Borrowed(bytes);
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Cow::Borrowed(bytes);
    };
    let Some(start) = text.find(begin) else {
        return Cow::Borrowed(bytes);
    };
    let start = start
        + text[start..]
            .find('\n')
            .map_or(text[start..].len(), |i| i + 1);
    let Some(stop) = text[start..].find(end).map(|i| i + start) else {
        return Cow::Borrowed(bytes);
    };
    let stop = text[..stop].rfind('\n').map_or(stop, |i| i + 1);
    if stop < start {
        return Cow::Borrowed(bytes);
    }
    let mut out = Vec::with_capacity(bytes.len() - (stop - start));
    out.extend_from_slice(&bytes[..start]);
    out.extend_from_slice(&bytes[stop..]);
    Cow::Owned(out)
}

/// The digest the provenance record holds for `path` with these bytes.
#[must_use]
pub fn provenance_digest(path: &str, bytes: &[u8]) -> String {
    sha256_hex(&provenance_bytes(path, bytes))
}

/// Classifies every planned path against the project tree and its provenance record.
///
/// `overwrite_unchanged` is the operator's `--overwrite-unchanged`: with it a regenerable path
/// is [`Action::Regenerate`]; without it the path refuses the plan. Nothing else depends on the
/// flag — a changed file refuses either way, an edit is written either way.
///
/// # Errors
///
/// [`Code::GenerationConflict`] naming every refusing path and, for a regenerable one, the flag —
/// with nothing written; [`Code::ManifestInvalid`] for a provenance record that does not parse;
/// [`Code::RenderFailed`] when a target cannot be read.
pub fn plan(
    project: &Dir,
    planned: Vec<Planned>,
    overwrite_unchanged: bool,
) -> Result<Plan, CliError> {
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
    let mut refusals: Vec<(String, Refusal)> = Vec::new();
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
            None if item.edit => {
                return Err(CliError::new(
                    Code::RenderFailed,
                    format!(
                        "`{}` is missing, so its marked block cannot be edited; generate the \
                         project again or restore the file",
                        item.path
                    ),
                ));
            }
            None => Action::Write,
            Some(bytes) if bytes == item.bytes => Action::Unchanged,
            Some(_) if item.edit => Action::Edit,
            Some(bytes) => {
                let unchanged_since_generation =
                    recorded(&item.path) == Some(provenance_digest(&item.path, &bytes).as_str());
                match (unchanged_since_generation, overwrite_unchanged) {
                    (true, true) => Action::Regenerate,
                    (true, false) => {
                        refusals.push((item.path.clone(), Refusal::OverwriteRequired));
                        Action::Unchanged
                    }
                    (false, _) => {
                        refusals.push((item.path.clone(), Refusal::Changed));
                        Action::Unchanged
                    }
                }
            }
        };
        decisions.push((item, action));
    }
    if !refusals.is_empty() {
        return Err(refused(&refusals, &decisions));
    }
    Ok(Plan {
        decisions,
        record,
        resources: Vec::new(),
        replacement: None,
    })
}

/// The refusal for these paths, in plan order: paths and reasons, never contents.
///
/// `details.paths` and `details.count` cover every refusing path; `details.changed` and
/// `details.regenerable` split them; `details.flag` names [`OVERWRITE_FLAG`] whenever a
/// regenerable path is among them; `details.reason` is `changed_since_generation` when any path
/// was changed — the flag alone would not help — and `overwrite_required` otherwise. What the
/// plan would have done beside the refusal is listed too — `details.write`, `details.edit`, and
/// `details.regenerate` (the last only under the flag) — so a refused dry run reports the whole
/// classification, not just the refusal.
fn refused(refusals: &[(String, Refusal)], decisions: &[(Planned, Action)]) -> CliError {
    let of = |wanted: Refusal| -> Vec<&str> {
        refusals
            .iter()
            .filter(|(_, why)| *why == wanted)
            .map(|(path, _)| path.as_str())
            .collect()
    };
    let changed = of(Refusal::Changed);
    let regenerable = of(Refusal::OverwriteRequired);
    let paths: Vec<&str> = refusals.iter().map(|(path, _)| path.as_str()).collect();
    let message = match (changed.is_empty(), regenerable.is_empty()) {
        (false, true) => format!(
            "{} file(s) exist and were changed since generation (or were never generated), so \
             nothing was written: {}. Move or revert them, then run again",
            changed.len(),
            changed.join(", ")
        ),
        (true, false) => format!(
            "{} file(s) differ from the render but are unchanged since generation, so nothing \
             was written: {}. They are regenerable: run again with `{OVERWRITE_FLAG}` to replace \
             them",
            regenerable.len(),
            regenerable.join(", ")
        ),
        _ => format!(
            "{} file(s) would be overwritten, so nothing was written. Changed since generation, \
             move or revert them: {}. Unchanged since generation, replaced only with \
             `{OVERWRITE_FLAG}`: {}",
            paths.len(),
            changed.join(", "),
            regenerable.join(", ")
        ),
    };
    let mut error = CliError::new(Code::GenerationConflict, message)
        .with(
            "reason",
            if changed.is_empty() {
                "overwrite_required"
            } else {
                "changed_since_generation"
            },
        )
        .with("count", paths.len().to_string())
        .with("paths", paths.join(", "));
    if !changed.is_empty() {
        error = error.with("changed", changed.join(", "));
    }
    if !regenerable.is_empty() {
        error = error
            .with("regenerable", regenerable.join(", "))
            .with("flag", OVERWRITE_FLAG);
    }
    for action in [Action::Write, Action::Regenerate, Action::Edit] {
        let would: Vec<&str> = decisions
            .iter()
            .filter(|(_, decided)| *decided == action)
            .map(|(planned, _)| planned.path.as_str())
            .collect();
        if !would.is_empty() {
            error = error.with(action.as_str(), would.join(", "));
        }
    }
    error
}

fn failed(path: &str, what: &str, error: &dyn std::fmt::Display) -> CliError {
    CliError::new(Code::RenderFailed, format!("`{path}` {what}: {error}"))
}

/// Writes `bytes` to a temporary sibling of `path` and returns the sibling's path. Nothing at
/// `path` changes.
fn stage(project: &Dir, path: &str, bytes: &[u8]) -> Result<String, CliError> {
    if let Some((parent, _)) = path.rsplit_once('/') {
        project
            .create_dir_all(parent)
            .map_err(|error| failed(path, "could not have its directory created", &error))?;
    }
    let temporary = format!("{path}.renvor-{}.tmp", std::process::id());
    project
        .write(&temporary, bytes)
        .map_err(|error| failed(path, "could not be staged beside its target", &error))?;
    Ok(temporary)
}

/// Renames the staged sibling into place.
fn place(project: &Dir, temporary: &str, path: &str) -> Result<(), CliError> {
    project.rename(temporary, project, path).map_err(|error| {
        let _ = project.remove_file(temporary);
        failed(path, "could not be moved into place", &error)
    })
}

/// Writes `bytes` at `path` through a sibling and a rename.
fn write_atomically(project: &Dir, path: &str, bytes: &[u8]) -> Result<(), CliError> {
    let temporary = stage(project, path, bytes)?;
    place(project, &temporary, path)
}

/// What a path held before it was placed over, so a failure can put it back.
struct Placed {
    path: String,
    previous: Option<Vec<u8>>,
}

/// Puts every placed path back the way it was, newest first, and reports what could not be
/// restored beside the failure that started the rollback.
fn roll_back(project: &Dir, placed: &[Placed], error: CliError) -> CliError {
    let mut error = error;
    for entry in placed.iter().rev() {
        let outcome = match &entry.previous {
            Some(bytes) => write_atomically(project, &entry.path, bytes),
            None => project.remove_file(&entry.path).map_err(|failure| {
                failed(
                    &entry.path,
                    "could not be removed during the rollback",
                    &failure,
                )
            }),
        };
        if let Err(failure) = outcome {
            error.message = format!(
                "{}; and the rollback could not restore `{}`: {}",
                error.message, entry.path, failure.message
            );
        }
    }
    error
}

/// Writes every non-`Unchanged` file, then rewrites the provenance record with the new digests
/// — as one change to the project, or none.
///
/// # The evidence tables are carried, not rewritten (FR-012-5a)
///
/// The record's `record_version`, `[toolchain]`, and `[verified_with]` are rendered again from
/// the parsed record through the deterministic renderer, so they come out byte for byte: an
/// operation that verified nothing writes no evidence, and a legacy record stays legacy. Only a
/// plan built with [`Plan::with_verified_with`] replaces the two tables.
///
/// # Transactional, in two phases
///
/// Every file is first **staged** as a temporary sibling; a failure there removes the siblings
/// and nothing at a target path has changed. Then every sibling is **placed** by a rename, with
/// what the path held remembered; a failure there — or in the record write after it — puts every
/// placed path back, newest first. The project is therefore either the tree the plan describes or
/// the tree the plan found, never a mixture (the correction round of Phase 011 made this a rule
/// rather than a hope: the first version wrote one file at a time and kept what it had written).
///
/// # Errors
///
/// [`Code::RenderFailed`] when a write, a rename, or the record write fails, naming the path; if
/// the rollback itself could not restore a path, the message says which.
pub fn commit(
    project: &Dir,
    plan: Plan,
    generator_version: &str,
    template_version: &str,
) -> Result<Vec<(String, Action)>, CliError> {
    let mut files: Vec<GeneratedFile> = plan
        .record
        .as_ref()
        .map(|r| r.files.clone())
        .unwrap_or_default();
    let mut resources: Vec<GeneratedResource> = plan
        .record
        .as_ref()
        .map(|r| r.resources.clone())
        .unwrap_or_default();
    let (recorded_generator, recorded_template) = plan.record.as_ref().map_or_else(
        || (generator_version.to_owned(), template_version.to_owned()),
        |r| (r.generator_version.clone(), r.template_version.clone()),
    );
    for resource in plan.resources {
        match resources
            .iter_mut()
            .find(|known| known.name == resource.name)
        {
            Some(known) => *known = resource,
            None => resources.push(resource),
        }
    }
    resources.sort_by(|left, right| left.name.cmp(&right.name));

    // ── 1. STAGE ─────────────────────────────────────────────────────────────────────
    let mut staged: Vec<(usize, String)> = Vec::new();
    let unstage = |staged: &[(usize, String)]| {
        for (_, temporary) in staged {
            let _ = project.remove_file(temporary);
        }
    };
    for (index, (planned, action)) in plan.decisions.iter().enumerate() {
        if *action == Action::Unchanged {
            continue;
        }
        match stage(project, &planned.path, &planned.bytes) {
            Ok(temporary) => staged.push((index, temporary)),
            Err(error) => {
                unstage(&staged);
                return Err(error);
            }
        }
        // A boundary per staged file: `RENVOR_FAIL_AT=generate-stage-<decision index>`.
        if let Err(error) = crate::inject::fail_at(&format!("generate-stage-{index}")) {
            unstage(&staged);
            return Err(error);
        }
    }

    // ── 2. PLACE ─────────────────────────────────────────────────────────────────────
    let mut placed: Vec<Placed> = Vec::with_capacity(staged.len());
    for (position, (index, temporary)) in staged.iter().enumerate() {
        let path = &plan.decisions[*index].0.path;
        let previous = match project.read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                unstage(&staged[position..]);
                return Err(roll_back(
                    project,
                    &placed,
                    failed(path, "could not be read before it was replaced", &error),
                ));
            }
        };
        if let Err(error) = place(project, temporary, path) {
            unstage(&staged[position + 1..]);
            return Err(roll_back(project, &placed, error));
        }
        placed.push(Placed {
            path: path.clone(),
            previous,
        });
        // A boundary per placed file: `RENVOR_FAIL_AT=generate-place-<decision index>`.
        if let Err(error) = crate::inject::fail_at(&format!("generate-place-{index}")) {
            unstage(&staged[position + 1..]);
            return Err(roll_back(project, &placed, error));
        }
    }
    // The last boundary: everything placed, the record not yet rewritten.
    if let Err(error) = crate::inject::fail_at("generate-record") {
        return Err(roll_back(project, &placed, error));
    }

    // ── 3. RECORD ────────────────────────────────────────────────────────────────────
    //
    // An edit of a MARKED file changes only its block, which the digest of a marked file does
    // not cover, so the record keeps what it had for that path: a marker edit must not turn the
    // rest of the file into generator-owned bytes. An edit of any other file — the resolved
    // lockfile — replaced the whole file, and is recorded as such.
    let mut done = Vec::with_capacity(plan.decisions.len());
    for (planned, action) in plan.decisions {
        let marked = MARKED.iter().any(|(path, _, _)| *path == planned.path);
        if !(action == Action::Edit && marked) {
            let digest = provenance_digest(&planned.path, &planned.bytes);
            match files.iter_mut().find(|file| file.path == planned.path) {
                Some(file) => file.sha256 = digest,
                None => files.push(GeneratedFile {
                    path: planned.path.clone(),
                    sha256: digest,
                }),
            }
        }
        done.push((planned.path, action));
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    let (record_version, toolchain, verified_with) = match plan.replacement {
        Some((toolchain, verified_with)) => {
            (Some(RECORD_VERSION), Some(toolchain), Some(verified_with))
        }
        None => plan.record.as_ref().map_or((None, None, None), |found| {
            (
                found.record_version,
                found.toolchain.clone(),
                found.verified_with.clone(),
            )
        }),
    };
    let text = record::render(&Record {
        record_version,
        generator_version: recorded_generator,
        template_version: recorded_template,
        toolchain,
        verified_with,
        files,
        resources,
    });
    if let Err(error) = write_atomically(project, record::PATH, text.as_bytes()) {
        return Err(roll_back(project, &placed, error));
    }
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
        Planned::file(path, bytes.to_vec())
    }

    /// A version-2 record for the tree, as `renvor new` writes one.
    fn write_record(dir: &Dir) {
        record::write(
            dir,
            "0.0.0",
            "8",
            &record::fixtures::toolchain(),
            Some(&record::fixtures::launched()),
            &[],
        )
        .expect("record");
    }

    /// The bytes from `[toolchain]` through the last check table — everything FR-012-5a says a
    /// non-verifying operation must leave alone.
    fn evidence_span(text: &str) -> &str {
        let start = text.find("\n[toolchain]\n").expect("a [toolchain] table");
        let end = text.find("\n[[file]]\n").expect("a [[file]] entry");
        &text[start..end]
    }

    #[test]
    fn an_edit_is_written_even_when_the_file_was_changed_and_needs_the_file_to_exist() {
        let (_keep, dir) = project(&[("routes.rs", b"// begin\n// end\n")]);
        let ok = plan(
            &dir,
            vec![Planned::edit(
                "routes.rs",
                b"// begin\nnew\n// end\n".to_vec(),
            )],
            false,
        )
        .expect("an edit never conflicts, and needs no flag");
        assert_eq!(ok.summary(), vec![("routes.rs", Action::Edit)]);
        let same = plan(
            &dir,
            vec![Planned::edit("routes.rs", b"// begin\n// end\n".to_vec())],
            false,
        )
        .expect("plans");
        assert_eq!(same.summary(), vec![("routes.rs", Action::Unchanged)]);
        let error = plan(
            &dir,
            vec![Planned::edit("missing.rs", b"x".to_vec())],
            false,
        )
        .expect_err("an edit needs its file");
        assert_eq!(error.code, Code::RenderFailed);
    }

    /// One detail of an error, by key.
    fn detail<'a>(error: &'a CliError, key: &str) -> Option<&'a str> {
        error
            .details
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn absent_identical_untouched_and_modified_are_the_four_cases() {
        let (_keep, dir) = project(&[
            ("same.txt", b"same\n"),
            ("untouched.txt", b"old render\n"),
            ("edited.txt", b"the user's version\n"),
        ]);
        // The record says `untouched.txt` and `edited.txt` were both generated as "old render".
        write_record(&dir);
        let mut rec = record::read(&dir).expect("reads").expect("present");
        let old = sha256_hex(b"old render\n");
        for file in &mut rec.files {
            if file.path == "edited.txt" {
                file.sha256 = old.clone();
            }
        }
        dir.write(record::PATH, record::render(&rec).as_bytes())
            .expect("rewrite");

        // FR-048 AS DECIDED (2026-09-05): the untouched file is regenerable, and without the
        // flag that refuses the plan — naming the path and the flag — and writes nothing.
        let items = || {
            vec![
                planned("new.txt", b"new\n"),
                planned("same.txt", b"same\n"),
                planned("untouched.txt", b"new render\n"),
            ]
        };
        let error = plan(&dir, items(), false).expect_err("regenerable needs the flag");
        assert_eq!(error.code, Code::GenerationConflict);
        assert_eq!(detail(&error, "reason"), Some("overwrite_required"));
        assert_eq!(detail(&error, "flag"), Some(OVERWRITE_FLAG));
        assert_eq!(detail(&error, "regenerable"), Some("untouched.txt"));
        assert_eq!(detail(&error, "paths"), Some("untouched.txt"));
        assert_eq!(detail(&error, "count"), Some("1"));
        assert_eq!(detail(&error, "changed"), None);
        assert!(error.message.contains(OVERWRITE_FLAG), "{}", error.message);
        assert_eq!(
            detail(&error, "write"),
            Some("new.txt"),
            "the refusal reports what the plan would have created"
        );
        assert_eq!(detail(&error, "regenerate"), None, "not without the flag");
        assert!(
            !dir.exists("new.txt"),
            "a refusal must write NOTHING, not even the file that could have been written"
        );
        // With the flag: the four cases, and the regenerable one is replaced.
        let ok = plan(&dir, items(), true).expect("no conflict");
        assert_eq!(
            ok.summary(),
            vec![
                ("new.txt", Action::Write),
                ("same.txt", Action::Unchanged),
                ("untouched.txt", Action::Regenerate),
            ]
        );
        assert_eq!(ok.writes(), 2);

        // An edited file refuses with the flag and without it; the flag waives nothing.
        for flag in [false, true] {
            let error = plan(
                &dir,
                vec![
                    planned("new.txt", b"new\n"),
                    planned("edited.txt", b"new render\n"),
                    planned("untouched.txt", b"new render\n"),
                ],
                flag,
            )
            .expect_err("an edited file is a conflict");
            assert_eq!(error.code, Code::GenerationConflict, "flag = {flag}");
            assert_eq!(
                detail(&error, "reason"),
                Some("changed_since_generation"),
                "flag = {flag}"
            );
            assert_eq!(
                detail(&error, "changed"),
                Some("edited.txt"),
                "flag = {flag}"
            );
            // Without the flag the regenerable path is refused beside it; with it, it is not.
            let expected = if flag {
                ("edited.txt", "1", None, None)
            } else {
                (
                    "edited.txt, untouched.txt",
                    "2",
                    Some("untouched.txt"),
                    Some(OVERWRITE_FLAG),
                )
            };
            assert_eq!(detail(&error, "paths"), Some(expected.0), "flag = {flag}");
            assert_eq!(detail(&error, "count"), Some(expected.1), "flag = {flag}");
            assert_eq!(detail(&error, "regenerable"), expected.2, "flag = {flag}");
            assert_eq!(detail(&error, "flag"), expected.3, "flag = {flag}");
            assert_eq!(detail(&error, "write"), Some("new.txt"), "flag = {flag}");
            assert_eq!(
                detail(&error, "regenerate"),
                if flag { Some("untouched.txt") } else { None },
                "flag = {flag}: with the flag the regenerable file would be regenerated"
            );
            assert!(
                !dir.exists("new.txt"),
                "a conflict must write NOTHING, not even the files that could have been written"
            );
            assert_eq!(
                std::fs::read(_keep.path().join("untouched.txt")).expect("read"),
                b"old render\n",
                "flag = {flag}: a refused plan replaced the regenerable file"
            );
        }
    }

    #[test]
    fn a_file_never_generated_is_a_conflict_even_without_a_record() {
        let (_keep, dir) = project(&[("theirs.txt", b"theirs\n")]);
        for flag in [false, true] {
            let error =
                plan(&dir, vec![planned("theirs.txt", b"ours\n")], flag).expect_err("conflict");
            assert_eq!(error.code, Code::GenerationConflict, "flag = {flag}");
            assert_eq!(
                detail(&error, "reason"),
                Some("changed_since_generation"),
                "unknown ownership is never a licence to overwrite (flag = {flag})"
            );
            assert_eq!(detail(&error, "changed"), Some("theirs.txt"));
        }
    }

    #[test]
    fn a_marker_edit_never_claims_the_rest_of_the_file() {
        // FOUND BY THE CODEX REVIEW (P1). A resource generation edits the marked block of
        // `src/routes.rs` and kept the user's lines outside it — then recorded the digest of the
        // whole merged file, so a later full re-render (the auth generator) read the file as
        // untouched and overwrote the user's lines. The marked block is the generators' shared
        // zone; the bytes around it belong to whoever last rendered the file, and a marker edit
        // must not transfer them.
        let generated = "head\n// renvor:resources:begin\n// renvor:resources:end\ntail\n";
        let (_keep, dir) = project(&[("src/routes.rs", generated.as_bytes())]);
        write_record(&dir);
        // The user adds a line of their own, outside the markers.
        let edited = "head\n// renvor:resources:begin\n// renvor:resources:end\ntail\nmine();\n";
        dir.write("src/routes.rs", edited).expect("write");
        // A resource generator fills the block and keeps the user's line.
        let with_block =
            "head\n// renvor:resources:begin\nadded();\n// renvor:resources:end\ntail\nmine();\n";
        let edit = plan(
            &dir,
            vec![Planned::edit(
                "src/routes.rs",
                with_block.as_bytes().to_vec(),
            )],
            false,
        )
        .expect("an edit never conflicts");
        commit(&dir, edit, "0.0.0", "7").expect("commits");
        assert_eq!(
            std::fs::read_to_string(_keep.path().join("src/routes.rs")).expect("read"),
            with_block,
            "the edit wrote the block and kept the user's line outside it"
        );
        // A later full re-render carries the block but not the user's line: refused with the
        // flag and without it.
        let rerender =
            "new head\n// renvor:resources:begin\nadded();\n// renvor:resources:end\ntail\n";
        for flag in [false, true] {
            let error = plan(
                &dir,
                vec![Planned::file("src/routes.rs", rerender.as_bytes().to_vec())],
                flag,
            )
            .expect_err("the user's line outside the markers must not be overwritten");
            assert_eq!(error.code, Code::GenerationConflict, "flag = {flag}");
            assert_eq!(
                detail(&error, "changed"),
                Some("src/routes.rs"),
                "flag = {flag}"
            );
            assert_eq!(
                std::fs::read_to_string(_keep.path().join("src/routes.rs")).expect("read"),
                with_block,
                "a conflict writes nothing"
            );
        }

        // POSITIVE CONTROL: the same sequence WITHOUT the user's line is regenerable, because the
        // block's contents are the generators' and do not count against the file — refused
        // without the flag, regenerated with it.
        let (_keep2, dir2) = project(&[("src/routes.rs", generated.as_bytes())]);
        write_record(&dir2);
        let filled = "head\n// renvor:resources:begin\nadded();\n// renvor:resources:end\ntail\n";
        let edit = plan(
            &dir2,
            vec![Planned::edit("src/routes.rs", filled.as_bytes().to_vec())],
            false,
        )
        .expect("plans");
        commit(&dir2, edit, "0.0.0", "7").expect("commits");
        let error = plan(
            &dir2,
            vec![Planned::file("src/routes.rs", rerender.as_bytes().to_vec())],
            false,
        )
        .expect_err("regenerable, but the flag was not given");
        assert_eq!(detail(&error, "reason"), Some("overwrite_required"));
        assert_eq!(detail(&error, "regenerable"), Some("src/routes.rs"));
        let ok = plan(
            &dir2,
            vec![Planned::file("src/routes.rs", rerender.as_bytes().to_vec())],
            true,
        )
        .expect("untouched outside the markers: regenerated");
        assert_eq!(ok.summary(), vec![("src/routes.rs", Action::Regenerate)]);
    }

    #[test]
    fn the_recorded_digest_of_a_marked_file_ignores_its_block() {
        // The rule the test above depends on, stated directly: for the two marked files the
        // provenance digest is taken over the file with its block emptied, so `renvor new`'s
        // record (an empty block) and a later edit's record (a filled block) agree.
        let empty = b"a\n// renvor:resources:begin\n// renvor:resources:end\nb\n";
        let filled = b"a\n// renvor:resources:begin\nx();\ny();\n// renvor:resources:end\nb\n";
        assert_eq!(
            provenance_digest("src/routes.rs", empty),
            provenance_digest("src/routes.rs", filled)
        );
        assert_eq!(
            provenance_digest("src/routes.rs", empty),
            sha256_hex(empty),
            "an empty block leaves the digest equal to the plain one, so `renvor new` needs no \
             special case"
        );
        // Bytes outside the block count.
        assert_ne!(
            provenance_digest("src/routes.rs", empty),
            provenance_digest(
                "src/routes.rs",
                b"a\n// renvor:resources:begin\n// renvor:resources:end\nb\nc\n"
            )
        );
        // An unmarked path is digested whole.
        assert_ne!(
            provenance_digest("src/other.rs", empty),
            provenance_digest("src/other.rs", filled)
        );
        // The modules file has its own marker pair.
        assert_eq!(
            provenance_digest(
                "src/resources/mod.rs",
                b"// renvor:resources:modules:begin\n// renvor:resources:modules:end\n"
            ),
            provenance_digest(
                "src/resources/mod.rs",
                b"// renvor:resources:modules:begin\npub mod post;\n// renvor:resources:modules:end\n"
            )
        );
    }

    #[test]
    fn generate_resource_and_migration_leave_verified_with_byte_identical() {
        // FR-012-5a. Neither operation verifies, so neither may touch the evidence: the parsed
        // tables go back through the deterministic renderer and come out byte for byte, while
        // the `[[file]]`/`[[resource]]` entries they own are rewritten around them.
        let (keep, dir) = project(&[("src/main.rs", b"fn main() {}\n")]);
        write_record(&dir);
        let before = std::fs::read_to_string(keep.path().join(record::PATH)).expect("read");
        assert!(before.contains("\nrecord_version = 2\n"));
        assert!(
            evidence_span(&before)
                .ends_with("\n[verified_with.checks.run]\noutcome = \"passed\"\n"),
            "the span reaches the last check table"
        );
        // A migration adds two files; a resource adds a module and a `[[resource]]`.
        let ok = plan(
            &dir,
            vec![
                planned("migrations/0002_add.up.sql", b"ALTER TABLE t ADD c INT;\n"),
                planned("migrations/0002_add.down.sql", b"ALTER TABLE t DROP c;\n"),
                planned("src/resources/post.rs", b"pub struct Post;\n"),
            ],
            false,
        )
        .expect("plans")
        .with_resource(GeneratedResource {
            name: "Post".to_owned(),
            fields: vec!["title:string".to_owned()],
        });
        commit(&dir, ok, "9.9.9", "99").expect("commits");
        let after = std::fs::read_to_string(keep.path().join(record::PATH)).expect("read");
        assert_ne!(before, after, "the entries changed");
        assert_eq!(
            evidence_span(&before),
            evidence_span(&after),
            "[toolchain] through [verified_with.checks.run] is byte-identical"
        );
        assert!(
            after.contains("\nrecord_version = 2\n"),
            "the version is carried"
        );
        assert!(
            after.contains("\ngenerator_version = \"0.0.0\"\n"),
            "the recorded generator is kept, not the committing one"
        );
        assert!(after.contains("path = \"migrations/0002_add.up.sql\""));
        assert!(after.contains("[[resource]]\nname = \"Post\""));
        let parsed = record::read(&dir).expect("reads").expect("present");
        assert_eq!(parsed.verified_with, Some(record::fixtures::launched()));
        assert_eq!(parsed.toolchain, Some(record::fixtures::toolchain()));

        // A LEGACY record stays legacy (FR-012-10a): no version, no tables, nothing invented.
        let (keep, dir) = project(&[("src/main.rs", b"fn main() {}\n")]);
        let legacy = record::render(&Record {
            record_version: None,
            generator_version: "0.0.0".to_owned(),
            template_version: "7".to_owned(),
            toolchain: None,
            verified_with: None,
            files: Vec::new(),
            resources: Vec::new(),
        });
        dir.create_dir_all(record::DIRECTORY).expect("mkdir");
        dir.write(record::PATH, legacy.as_bytes()).expect("write");
        let ok = plan(&dir, vec![planned("migrations/0002.up.sql", b"x\n")], false).expect("plans");
        commit(&dir, ok, "9.9.9", "99").expect("commits");
        let after = std::fs::read_to_string(keep.path().join(record::PATH)).expect("read");
        assert!(
            !after.contains("record_version"),
            "a legacy record stays legacy"
        );
        assert!(!after.contains("[toolchain]"));
        assert!(!after.contains("[verified_with]"));
        assert!(after.contains("path = \"migrations/0002.up.sql\""));

        // Only an operation that VERIFIED replaces the tables — and that upgrades a legacy
        // record to version 2 with the toolchain it was given, `"none"` for a tree without a pin.
        let none = Toolchain {
            pinned: "none".to_owned(),
            rust_version: "none".to_owned(),
        };
        let ok = plan(
            &dir,
            vec![planned("src/auth.rs", b"pub fn auth() {}\n")],
            false,
        )
        .expect("plans")
        .with_verified_with(none.clone(), record::fixtures::cached());
        commit(&dir, ok, "9.9.9", "99").expect("commits");
        let upgraded = record::read(&dir).expect("reads").expect("present");
        assert_eq!(upgraded.record_version, Some(2));
        assert_eq!(upgraded.toolchain, Some(none));
        assert_eq!(upgraded.verified_with, Some(record::fixtures::cached()));
        let text = std::fs::read_to_string(keep.path().join(record::PATH)).expect("read");
        assert!(text.contains("\noperation = \"auth\"\n"));
        assert!(text.contains("\npinned = \"none\"\n"));
    }

    #[test]
    fn a_rollback_restores_what_was_placed_newest_first_and_names_what_it_could_not() {
        // The path a rename failure takes: `roll_back` is what both the rename branch and the
        // injected failure call, so it is tested directly — a rename after a successful stage in
        // the same directory fails only on a cross-device move, which no test can arrange.
        let (keep, dir) = project(&[
            ("kept.txt", b"the user's bytes\n"),
            ("gone.txt", b"was absent, then written\n"),
        ]);
        dir.write("kept.txt", b"overwritten\n").expect("write");
        let placed = vec![
            Placed {
                path: "kept.txt".to_owned(),
                previous: Some(b"the user's bytes\n".to_vec()),
            },
            Placed {
                path: "gone.txt".to_owned(),
                previous: None,
            },
        ];
        let error = roll_back(
            &dir,
            &placed,
            CliError::new(Code::RenderFailed, "the second rename failed"),
        );
        assert_eq!(
            std::fs::read(keep.path().join("kept.txt")).expect("read"),
            b"the user's bytes\n",
            "the overwritten file is put back"
        );
        assert!(
            !keep.path().join("gone.txt").exists(),
            "the file that was absent is removed"
        );
        assert_eq!(error.message, "the second rename failed");

        // A path the rollback cannot restore is named beside the original failure.
        let missing = vec![Placed {
            path: "never/there.txt".to_owned(),
            previous: None,
        }];
        let error = roll_back(&dir, &missing, CliError::new(Code::RenderFailed, "first"));
        assert!(
            error
                .message
                .starts_with("first; and the rollback could not restore `never/there.txt`"),
            "{}",
            error.message
        );
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
            false,
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
        let again = plan(&dir, vec![planned("deep/new.txt", b"new\n")], false).expect("plans");
        assert_eq!(again.summary(), vec![("deep/new.txt", Action::Unchanged)]);
    }
}
