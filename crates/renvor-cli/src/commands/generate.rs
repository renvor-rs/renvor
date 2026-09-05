//! `renvor generate`: additions to an existing project, rerun-safe (Phase 011).
//!
//! # Into an existing tree, beside the user's files
//!
//! `renvor new` writes where nothing was. Everything here writes where the user already works,
//! so it goes through [`crate::generate::apply`]: every target path is classified against the
//! working tree and the provenance record before the first byte is written, and a file the user
//! changed since generation is a `generation_conflict` that writes nothing. A file that differs
//! from the render but is unchanged since generation is *regenerable*, and is replaced only
//! under `--overwrite-unchanged` (FR-048 as decided on 2026-09-05); without the flag it refuses
//! the run the same way, naming the flag. A rerun of the same command is a no-op that says so,
//! and a dry run classifies exactly as a real run does.
//!
//! # Migrations
//!
//! `migration <name>` writes a reversible pair, versioned by the UTC instant, into `migrations/`;
//! run twice for the same name it finds the pair it wrote and leaves it, so a rerun does not
//! stack a second pair. `migration --import auth|jobs` copies the framework's embedded set for
//! the project's engine byte for byte — the same files `renvor new` copies for a starter that
//! selects the auth starter or the jobs capability — which is how a project that adopts either
//! later composes the two sets in its one directory (Phase 010 limitation L-7).

use std::path::Path;

use cap_std::fs::Dir;

use crate::commands::check;
use crate::exit::{CliError, Code, Exit};
use crate::generate::apply::{self, Action as Applied, Planned};
use crate::output::Reporter;
use crate::output::layout::{Report, Status};

/// What to generate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// A migration pair named `name`, or the framework's `import` set.
    Migration {
        /// The migration's name; `None` only with an import.
        name: Option<String>,
        /// `auth` or `jobs`.
        import: Option<String>,
    },
    /// A resource `name` with `fields` as `name:type`.
    Resource {
        /// The PascalCase type name.
        name: String,
        /// The columns.
        fields: Vec<String>,
    },
    /// The session authentication starter, added to a starter that has none.
    Auth,
}

/// The sets `--import` knows, and nothing else.
const IMPORTS: [&str; 2] = ["auth", "jobs"];

/// The name grammar: a lowercase identifier, at most 64 characters.
fn validate_migration_name(name: &str) -> Result<(), CliError> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if valid {
        return Ok(());
    }
    Err(CliError::new(
        Code::UnsupportedValue,
        format!(
            "`{name}` is not a migration name: lowercase ASCII letters, digits, and `_`, starting \
             with a letter, at most 64 characters"
        ),
    )
    .with("flag", "name")
    .with("supported", "a lowercase identifier"))
}

/// `YYYYMMDDHHMMSS` in UTC for `seconds` since the Unix epoch — the migration version.
///
/// Standard library only, so the generator gains no dependency for one timestamp: the civil
/// date is Howard Hinnant's `civil_from_days`.
#[must_use]
pub fn utc_version(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(i64::MAX);
    let remainder = seconds % 86_400;
    let (hours, minutes, secs) = (remainder / 3600, (remainder % 3600) / 60, remainder % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}{month:02}{day:02}{hours:02}{minutes:02}{secs:02}")
}

/// The version of an existing pair named `name`, if the directory holds one.
fn existing_version(project: &Dir, name: &str) -> Result<Option<String>, CliError> {
    let suffix = format!("_{name}.up.sql");
    let entries = match project.read_dir("migrations") {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CliError::new(
                Code::RenderFailed,
                format!("`migrations/` could not be read: {error}"),
            ));
        }
    };
    let mut found: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            CliError::new(
                Code::RenderFailed,
                format!("`migrations/` could not be read: {error}"),
            )
        })?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if let Some(version) = file.strip_suffix(&suffix) {
            found.push(version.to_owned());
        }
    }
    found.sort();
    Ok(found.into_iter().next())
}

/// Every version the directory holds, whichever name follows it: `0001`, `20260901000001`, …
///
/// Read so a version is never handed out twice. SQLx keys its ledger by version, so two files
/// that share one — two names generated within the same second, or an imported set colliding
/// with a migration the user wrote — make migration loading fail at the next Boot (found by the
/// Codex review of Phase 011).
fn existing_versions(project: &Dir) -> Result<std::collections::BTreeSet<String>, CliError> {
    let entries = match project.read_dir("migrations") {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(std::collections::BTreeSet::new());
        }
        Err(error) => {
            return Err(CliError::new(
                Code::RenderFailed,
                format!("`migrations/` could not be read: {error}"),
            ));
        }
    };
    let mut versions = std::collections::BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            CliError::new(
                Code::RenderFailed,
                format!("`migrations/` could not be read: {error}"),
            )
        })?;
        let file = entry.file_name().to_string_lossy().into_owned();
        if let Some(version) = version_of(&file) {
            versions.insert(version.to_owned());
        }
    }
    Ok(versions)
}

/// The version prefix of a migration file name: the digits before the first `_`, when the name
/// is `<digits>_<name>.<up|down>.sql`.
fn version_of(file: &str) -> Option<&str> {
    let (version, rest) = file.split_once('_')?;
    let numeric = !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit());
    let migration = rest.ends_with(".up.sql") || rest.ends_with(".down.sql");
    (numeric && migration).then_some(version)
}

/// The UTC instant `now` as a version, moved forward second by second past every version in
/// `taken`, so a pair generated in the same second as another gets the next free second rather
/// than the same number.
fn allocate_version(taken: &std::collections::BTreeSet<String>, now: u64) -> String {
    let mut candidate = now;
    loop {
        let version = utc_version(candidate);
        if !taken.contains(&version) {
            return version;
        }
        candidate += 1;
    }
}

/// The version for the pair named `name`: the one it already has, so a rerun finds its pair, or
/// a fresh one past every version the directory holds.
fn version_for(project: &Dir, name: &str) -> Result<String, CliError> {
    if let Some(version) = existing_version(project, name)? {
        return Ok(version);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| CliError::new(Code::Internal, "the system clock is before the Unix epoch"))?
        .as_secs();
    Ok(allocate_version(&existing_versions(project)?, now))
}

/// The versions in `planned` that another file in the directory already holds — an import that
/// would land beside a user's migration with the same version.
fn colliding_versions(project: &Dir, planned: &[Planned]) -> Result<Vec<String>, CliError> {
    let taken = existing_versions(project)?;
    let mut collisions = Vec::new();
    for item in planned {
        let Some(file) = item.path.strip_prefix("migrations/") else {
            continue;
        };
        let Some(version) = version_of(file) else {
            continue;
        };
        if taken.contains(version) && !project.exists(&item.path) {
            let version = version.to_owned();
            if !collisions.contains(&version) {
                collisions.push(version);
            }
        }
    }
    Ok(collisions)
}

fn planned_pair(version: &str, name: &str) -> Vec<Planned> {
    let up = format!(
        "-- {name}: the forward migration.\n\
         -- Applied on Boot by the database provider, in version order, and recorded in\n\
         -- `_sqlx_migrations` by its checksum: edit it before it has run anywhere, never after.\n\
         -- Keep it reversible in `{version}_{name}.down.sql` beside this file.\n"
    );
    let down = format!(
        "-- {name}: reverses `{version}_{name}.up.sql` exactly, so a rollback leaves the schema\n\
         -- as it was before the forward migration ran.\n"
    );
    vec![
        Planned::file(
            format!("migrations/{version}_{name}.up.sql"),
            up.into_bytes(),
        ),
        Planned::file(
            format!("migrations/{version}_{name}.down.sql"),
            down.into_bytes(),
        ),
    ]
}

fn planned_import(engine: &str, set: &str) -> Result<Vec<Planned>, CliError> {
    let files: Vec<(String, &'static str)> = match set {
        "auth" => renvor_auth::migrations::for_engine(engine)
            .map(|set| {
                set.files()
                    .iter()
                    .map(|file| (file.name().to_owned(), file.contents()))
                    .collect()
            })
            .unwrap_or_default(),
        "jobs" => renvor_jobs::migrations::for_engine(engine)
            .map(|set| {
                set.files()
                    .iter()
                    .map(|file| (file.name().to_owned(), file.contents()))
                    .collect()
            })
            .unwrap_or_default(),
        other => {
            return Err(CliError::new(
                Code::UnsupportedValue,
                format!(
                    "`--import {other}` is not a set this version can copy; supported: {}",
                    IMPORTS.join(", ")
                ),
            )
            .with("flag", "--import")
            .with("supported", IMPORTS.join(", ")));
        }
    };
    if files.is_empty() {
        return Err(CliError::new(
            Code::UnsupportedValue,
            format!("the framework ships no `{set}` migration set for `{engine}`"),
        )
        .with("flag", "--import")
        .with("supported", IMPORTS.join(", ")));
    }
    Ok(files
        .into_iter()
        .map(|(name, contents)| {
            Planned::file(format!("migrations/{name}"), contents.as_bytes().to_vec())
        })
        .collect())
}

/// How much of a project the scratch copy will take: the bound keeps a mistaken path — a home
/// directory, a monorepo — from being copied wholesale.
const MERGE_MAX_FILES: usize = 20_000;
const MERGE_MAX_BYTES: u64 = 512 * 1024 * 1024;

/// Copies the project into `into`, without `target/`, `.git/`, or any symbolic link, applies the
/// plan's writes on top, runs the same verification `renvor new` runs on a starter, and returns
/// the lockfile the build resolved.
///
/// # Errors
///
/// [`Code::BoundExceeded`] past the copy bound; [`Code::ProjectVerificationFailed`] when the
/// merged project does not build, lint, format, test, or start; [`Code::StagingFailed`] when the
/// scratch copy cannot be made.
fn verify_merged(
    reporter: &Reporter,
    project_path: &Path,
    plan: &apply::Plan,
) -> Result<Vec<u8>, CliError> {
    let scratch = tempfile::tempdir().map_err(|error| {
        CliError::new(
            Code::StagingFailed,
            format!("a scratch directory could not be created: {error}"),
        )
    })?;
    let merged = scratch.path().join("merged");
    copy_project(project_path, &merged)?;
    for (planned, action) in &plan.decisions {
        if *action == Applied::Unchanged {
            continue;
        }
        let target = merged.join(&planned.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CliError::new(
                    Code::StagingFailed,
                    format!(
                        "`{}` could not be staged for verification: {error}",
                        planned.path
                    ),
                )
            })?;
        }
        std::fs::write(&target, &planned.bytes).map_err(|error| {
            CliError::new(
                Code::StagingFailed,
                format!(
                    "`{}` could not be staged for verification: {error}",
                    planned.path
                ),
            )
        })?;
    }
    let progress = crate::output::progress::Progress::start(
        "verifying the project with the auth starter",
        reporter,
    );
    let verified = crate::generate::verify::in_staging(
        &merged,
        &progress,
        crate::generate::verify::Smoke::AnswersDumpRequest,
    );
    progress.finish();
    verified?;
    std::fs::read(merged.join("Cargo.lock")).map_err(|error| {
        CliError::new(
            Code::ProjectVerificationFailed,
            format!("the verified project has no `Cargo.lock` to record: {error}"),
        )
        .with("check", "Cargo.lock is resolved")
    })
}

/// Copies every regular file under `from` to `into`, skipping `target` and `.git` at the root
/// and every symbolic link, within [`MERGE_MAX_FILES`] and [`MERGE_MAX_BYTES`].
fn copy_project(from: &Path, into: &Path) -> Result<(), CliError> {
    let failed = |what: String| CliError::new(Code::StagingFailed, what);
    let mut files = 0_usize;
    let mut bytes = 0_u64;
    let mut stack: Vec<std::path::PathBuf> = vec![std::path::PathBuf::new()];
    while let Some(relative) = stack.pop() {
        let directory = from.join(&relative);
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            failed(format!(
                "`{}` could not be read for verification: {error}",
                crate::output::redact::path(&directory)
            ))
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                failed(format!(
                    "the project could not be read for verification: {error}"
                ))
            })?;
            let name = entry.file_name();
            if relative.as_os_str().is_empty() && (name == "target" || name == ".git") {
                continue;
            }
            let kind = entry.file_type().map_err(|error| {
                failed(format!(
                    "the project could not be read for verification: {error}"
                ))
            })?;
            let child = relative.join(&name);
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(child);
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            files += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            if files > MERGE_MAX_FILES || bytes > MERGE_MAX_BYTES {
                return Err(CliError::new(
                    Code::BoundExceeded,
                    format!(
                        "the project exceeds the verification copy bound ({MERGE_MAX_FILES} \
                         files, {MERGE_MAX_BYTES} bytes); is `--path` the project root?"
                    ),
                )
                .with("bound", "verification copy"));
            }
            let target = into.join(&child);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    failed(format!("the scratch copy could not be made: {error}"))
                })?;
            }
            std::fs::copy(entry.path(), &target)
                .map_err(|error| failed(format!("the scratch copy could not be made: {error}")))?;
        }
    }
    Ok(())
}

/// The pieces `main` hands to [`run`] for a parsed `renvor generate` action: the project path,
/// what to generate, and whether regenerable files may be replaced — one place, so every action
/// carries `--overwrite-unchanged` the same way.
#[must_use]
pub fn parts(action: crate::config::flags::GenerateAction) -> (std::path::PathBuf, Action, bool) {
    use crate::config::flags::GenerateAction;
    match action {
        GenerateAction::Migration {
            name,
            import,
            path,
            overwrite_unchanged,
        } => (
            path,
            Action::Migration { name, import },
            overwrite_unchanged,
        ),
        GenerateAction::Resource {
            name,
            fields,
            path,
            overwrite_unchanged,
        } => (path, Action::Resource { name, fields }, overwrite_unchanged),
        GenerateAction::Auth {
            path,
            overwrite_unchanged,
        } => (path, Action::Auth, overwrite_unchanged),
    }
}

/// Runs a generation into the project at `path`.
///
/// `overwrite_unchanged` is the operator's `--overwrite-unchanged`: a target that differs from
/// the render but is unchanged since generation is replaced with it and refuses the run without
/// it. It waives nothing else: a changed file refuses the run either way, and a dry run
/// classifies exactly as a real run does.
///
/// # Errors
///
/// [`Code::ManifestInvalid`] when `renvor.toml` does not validate; [`Code::UnsupportedCombination`]
/// when the project has no database to migrate; [`Code::UnsupportedValue`] for a name or an
/// import outside the grammar; [`Code::GenerationConflict`] when a target file was changed since
/// generation, or is regenerable and the flag was not given — nothing is written;
/// [`Code::RenderFailed`] when a write fails.
pub fn run(
    reporter: &Reporter,
    path: &Path,
    action: Action,
    dry_run: bool,
    overwrite_unchanged: bool,
) -> Result<Exit, CliError> {
    let manifest = check::load(path)?;
    let project = Dir::open_ambient_dir(path, cap_std::ambient_authority()).map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!(
                "`{}` could not be opened: {error}",
                crate::output::redact::path(path)
            ),
        )
    })?;

    let mut resource: Option<crate::generate::record::GeneratedResource> = None;
    let verifies = matches!(action, Action::Auth);
    let (what, planned) = match action {
        Action::Migration { name, import } => {
            let Some(persistence) = manifest.persistence.as_ref() else {
                return Err(CliError::new(
                    Code::UnsupportedCombination,
                    "this project has no database (`renvor.toml` has no `[persistence]` table), \
                     so there is nothing to migrate; generate a project with `--database` to get \
                     a migration directory",
                )
                .with("flags", "migration")
                .with("reason", "no_database"));
            };
            match (name, import) {
                (_, Some(set)) => {
                    let planned = planned_import(&persistence.database, &set)?;
                    let collisions = colliding_versions(&project, &planned)?;
                    if !collisions.is_empty() {
                        return Err(CliError::new(
                            Code::GenerationConflict,
                            format!(
                                "{} version(s) of the `{set}` set are already held by another \
                                 migration in `migrations/`, so nothing was written: {}. SQLx \
                                 keys its ledger by version; rename or renumber yours, then run \
                                 again",
                                collisions.len(),
                                collisions.join(", ")
                            ),
                        )
                        .with("reason", "version_present")
                        .with("count", collisions.len().to_string())
                        .with("versions", collisions.join(", ")));
                    }
                    (format!("the framework's `{set}` migration set"), planned)
                }
                (Some(name), None) => {
                    validate_migration_name(&name)?;
                    let version = version_for(&project, &name)?;
                    (
                        format!("migration `{version}_{name}`"),
                        planned_pair(&version, &name),
                    )
                }
                (None, None) => {
                    return Err(CliError::new(
                        Code::Usage,
                        "`generate migration` needs a name, or `--import auth|jobs`",
                    ));
                }
            }
        }
        Action::Resource { name, fields } => {
            let (what, planned, definition) = plan_resource(&project, &manifest, &name, &fields)?;
            resource = Some(definition);
            (what, planned)
        }
        Action::Auth => plan_auth(&project, &manifest)?,
    };

    let mut plan = apply::plan(&project, planned, overwrite_unchanged)?;
    if let Some(definition) = resource {
        plan = plan.with_resource(definition);
    }
    if verifies {
        // AFTER the conflict check, so a refusal costs no build; BEFORE the commit — and on a
        // dry run too — so the plan reports the lockfile it will write. The auth render adds
        // dependencies, and a lockfile left as it was fails `cargo build --locked` the moment the
        // command has reported success (found by the Codex review of Phase 011). The merged tree
        // is built and tested in a scratch copy, which is also what proves a resource module
        // rendered again with its guards still compiles beside everything the user wrote.
        let lock = verify_merged(reporter, path, &plan)?;
        plan = plan.with_edit(&project, "Cargo.lock", lock)?;
    }
    let decisions: Vec<(String, Applied)> = plan
        .summary()
        .into_iter()
        .map(|(path, action)| (path.to_owned(), action))
        .collect();
    let writes = plan.writes();

    let files: Vec<serde_json::Value> = decisions
        .iter()
        .map(|(path, action)| serde_json::json!({ "path": path, "action": action.as_str() }))
        .collect();
    let mut human = Report::new();
    if dry_run {
        human = human.status(
            Status::Info,
            format!("Dry run: {what} — {writes} file(s) would be written"),
        );
    } else {
        apply::commit(
            &project,
            plan,
            env!("CARGO_PKG_VERSION"),
            &manifest.renvor.template_version,
        )?;
        human = human.status(
            Status::Done,
            if writes == 0 {
                format!("Nothing to do: {what} is already in place")
            } else {
                format!("Generated {what}: {writes} file(s) written")
            },
        );
    }
    human = human.blank();
    for (path, action) in &decisions {
        human = human.row(action.as_str(), path.clone());
    }
    Ok(reporter.finish(
        "generate",
        &human,
        serde_json::json!({
            "dryRun": dry_run,
            "project": crate::output::redact::path(path),
            "files": files,
            "written": if dry_run { 0 } else { writes },
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_utc_instant_to_the_second() {
        // 2023-11-14T22:13:20Z.
        assert_eq!(utc_version(1_700_000_000), "20231114221320");
        assert_eq!(utc_version(0), "19700101000000");
        // A leap day, and the last second of a year.
        assert_eq!(utc_version(951_782_400), "20000229000000");
        assert_eq!(utc_version(1_704_067_199), "20231231235959");
    }

    #[test]
    fn a_migration_name_is_a_lowercase_identifier() {
        for name in ["add_index", "a", "v2_users", "x".repeat(64).as_str()] {
            validate_migration_name(name).expect(name);
        }
        for name in [
            "",
            "Add",
            "1st",
            "add-index",
            "add index",
            "x".repeat(65).as_str(),
        ] {
            let error = validate_migration_name(name).expect_err(name);
            assert_eq!(error.code, Code::UnsupportedValue);
        }
    }

    #[test]
    fn a_version_is_allocated_past_every_version_the_directory_holds() {
        // FOUND BY THE CODEX REVIEW (P1). Two names generated within one second received one
        // version, because the lookup searched by name; SQLx keys its ledger by version.
        let mut taken = std::collections::BTreeSet::new();
        assert_eq!(allocate_version(&taken, 1_700_000_000), "20231114221320");
        taken.insert("20231114221320".to_owned());
        taken.insert("20231114221321".to_owned());
        assert_eq!(
            allocate_version(&taken, 1_700_000_000),
            "20231114221322",
            "the next free second, not the same one"
        );
        // The version prefix is what is compared, whichever name and direction follow it.
        assert_eq!(version_of("0001_create_item.up.sql"), Some("0001"));
        assert_eq!(
            version_of("20260901000001_create_auth_user.down.sql"),
            Some("20260901000001")
        );
        assert_eq!(version_of("README.md"), None);
        assert_eq!(version_of("notes_20260901.sql"), None);
        assert_eq!(version_of("_x.up.sql"), None);
    }

    #[test]
    fn every_generate_action_carries_the_overwrite_flag_and_it_is_off_by_default() {
        // FR-048 AS DECIDED: the flag is per action, so `renvor generate auth
        // --overwrite-unchanged` reads naturally, and `parts` is the one place that hands it on
        // — a dispatch that dropped it for one action would drop it here, where this test looks.
        use clap::Parser as _;
        for argv in [
            vec!["renvor", "generate", "migration", "add_x"],
            vec!["renvor", "generate", "resource", "Post", "title:string"],
            vec!["renvor", "generate", "auth"],
        ] {
            let cli = crate::config::flags::Cli::try_parse_from(&argv).expect("parses");
            let crate::config::flags::Command::Generate { action } = cli.command else {
                panic!("not a generate command")
            };
            let (path, _, overwrite) = parts(action);
            assert!(!overwrite, "{argv:?}: on by default");
            assert_eq!(path, std::path::PathBuf::from("."));
            let mut with = argv.clone();
            with.extend(["--overwrite-unchanged", "--dry-run", "--path", "elsewhere"]);
            let cli = crate::config::flags::Cli::try_parse_from(&with).expect("parses");
            assert!(cli.dry_run, "{with:?}: a dry run may carry the flag");
            let crate::config::flags::Command::Generate { action } = cli.command else {
                panic!("not a generate command")
            };
            let (path, _, overwrite) = parts(action);
            assert!(overwrite, "{with:?}: the flag was dropped");
            assert_eq!(path, std::path::PathBuf::from("elsewhere"));
        }
    }

    #[test]
    fn an_import_is_one_of_the_two_shipped_sets_for_the_engine() {
        assert_eq!(planned_import("postgres", "auth").expect("auth").len(), 18);
        assert_eq!(planned_import("mysql", "auth").expect("auth").len(), 16);
        assert_eq!(planned_import("postgres", "jobs").expect("jobs").len(), 10);
        let error = planned_import("postgres", "s3").expect_err("unknown");
        assert_eq!(error.code, Code::UnsupportedValue);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "supported" && v == "auth, jobs")
        );
    }
}

// ---- resources -------------------------------------------------------------------------------

/// A column of a generated resource.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Field {
    /// The column and struct field name: a lowercase identifier.
    pub name: String,
    /// The Rust type.
    pub rust_type: &'static str,
    /// The column type for the project's engine.
    pub sql_type: &'static str,
    /// SeaORM's column type attribute, when the Rust type alone would not say it.
    pub sea_column_type: &'static str,
    /// How the column is bound: by reference for an owned `String`, by value for a `Copy` type.
    pub bind: String,
    /// A JSON sample for the generated test, and a second one that differs.
    pub sample_a: &'static str,
    /// See `sample_a`.
    pub sample_b: &'static str,
}

/// The closed set of column types, and nothing else.
const FIELD_TYPES: [&str; 5] = ["string", "text", "integer", "boolean", "float"];

fn field(spec: &str, engine: &str) -> Result<Field, CliError> {
    let refused = |detail: String| {
        CliError::new(Code::UnsupportedValue, detail)
            .with("flag", "FIELD:TYPE")
            .with("supported", FIELD_TYPES.join(", "))
    };
    let Some((name, kind)) = spec.split_once(':') else {
        return Err(refused(format!(
            "`{spec}` is not a field: write `name:type` with type one of {}",
            FIELD_TYPES.join(", ")
        )));
    };
    let valid_name = !name.is_empty()
        && name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if !valid_name || name == "id" {
        return Err(refused(format!(
            "`{name}` is not a field name: a lowercase identifier of at most 32 characters, and \
             not `id`, which every resource carries"
        )));
    }
    if is_reserved_sql_word(name) {
        return Err(refused(format!(
            "`{name}` is a word PostgreSQL or MySQL reserves as a keyword, and the generated SQL \
             uses column names bare; choose another, such as `{name}_value`"
        ))
        .with("reason", "reserved_identifier"));
    }
    let mysql = engine == "mysql";
    let (rust_type, sql_type, sea_column_type, sample_a, sample_b) = match kind {
        "string" => (
            "String",
            "VARCHAR(255)",
            "String(StringLen::N(255))",
            "\"alpha\"",
            "\"beta\"",
        ),
        "text" => ("String", "TEXT", "Text", "\"lorem ipsum\"", "\"dolor sit\""),
        "integer" => ("i64", "BIGINT", "", "1", "2"),
        "boolean" => ("bool", "BOOLEAN", "", "true", "false"),
        "float" => (
            "f64",
            if mysql { "DOUBLE" } else { "DOUBLE PRECISION" },
            "",
            "1.5",
            "2.5",
        ),
        other => {
            return Err(refused(format!(
                "`{other}` is not a column type; one of {}",
                FIELD_TYPES.join(", ")
            )));
        }
    };
    Ok(Field {
        name: name.to_owned(),
        bind: if rust_type == "String" {
            format!("&input.{name}")
        } else {
            format!("input.{name}")
        },
        rust_type,
        sql_type,
        sea_column_type,
        sample_a,
        sample_b,
    })
}

/// `Post` → `post`; `BlogPost` → `blog_post`.
fn snake_case(type_name: &str) -> String {
    let mut out = String::with_capacity(type_name.len() + 4);
    for (index, c) in type_name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `post` → `posts`; `category` → `categories`; `box` → `boxes`.
fn plural(snake: &str) -> String {
    if let Some(stem) = snake.strip_suffix('y')
        && !stem.ends_with(['a', 'e', 'i', 'o', 'u'])
    {
        return format!("{stem}ies");
    }
    if snake.ends_with("s")
        || snake.ends_with("x")
        || snake.ends_with("z")
        || snake.ends_with("ch")
        || snake.ends_with("sh")
    {
        return format!("{snake}es");
    }
    format!("{snake}s")
}

/// The words PostgreSQL and MySQL reserve, lowercase and sorted, so an unquoted table or column
/// identifier derived from a user's name is never one of them.
///
/// The union of PostgreSQL's reserved key words (Appendix C of its manual) and MySQL 8.4's
/// reserved words (marked `(R)` in its keyword list). The generated SQL uses the identifiers
/// bare, so `Order` would become `FROM order` and fail on both engines after generation had
/// reported success (found by the Codex review of Phase 011). Refusing is the fail-closed choice
/// the constitution's allowlist rule for dynamic identifiers asks for; quoting would make every
/// generated statement engine-specific.
const RESERVED_SQL_WORDS: &[&str] = &[
    "accessible",
    "add",
    "all",
    "alter",
    "analyse",
    "analyze",
    "and",
    "any",
    "array",
    "as",
    "asc",
    "asensitive",
    "asymmetric",
    "authorization",
    "before",
    "between",
    "bigint",
    "binary",
    "blob",
    "both",
    "by",
    "call",
    "cascade",
    "case",
    "cast",
    "change",
    "char",
    "character",
    "check",
    "collate",
    "collation",
    "column",
    "concurrently",
    "condition",
    "constraint",
    "continue",
    "convert",
    "create",
    "cross",
    "cube",
    "cume_dist",
    "current_catalog",
    "current_date",
    "current_role",
    "current_schema",
    "current_time",
    "current_timestamp",
    "current_user",
    "cursor",
    "database",
    "databases",
    "day_hour",
    "day_microsecond",
    "day_minute",
    "day_second",
    "dec",
    "decimal",
    "declare",
    "default",
    "deferrable",
    "delayed",
    "delete",
    "dense_rank",
    "desc",
    "describe",
    "deterministic",
    "distinct",
    "distinctrow",
    "div",
    "do",
    "double",
    "drop",
    "dual",
    "each",
    "else",
    "elseif",
    "empty",
    "enclosed",
    "end",
    "escaped",
    "except",
    "exists",
    "exit",
    "explain",
    "false",
    "fetch",
    "first_value",
    "float",
    "float4",
    "float8",
    "for",
    "force",
    "foreign",
    "freeze",
    "from",
    "full",
    "fulltext",
    "function",
    "generated",
    "get",
    "grant",
    "group",
    "grouping",
    "groups",
    "having",
    "high_priority",
    "hour_microsecond",
    "hour_minute",
    "hour_second",
    "if",
    "ignore",
    "ilike",
    "in",
    "index",
    "infile",
    "initially",
    "inner",
    "inout",
    "insensitive",
    "insert",
    "int",
    "int1",
    "int2",
    "int3",
    "int4",
    "int8",
    "integer",
    "intersect",
    "interval",
    "into",
    "io_after_gtids",
    "io_before_gtids",
    "is",
    "isnull",
    "iterate",
    "join",
    "json_table",
    "key",
    "keys",
    "kill",
    "lag",
    "last_value",
    "lateral",
    "lead",
    "leading",
    "leave",
    "left",
    "like",
    "limit",
    "linear",
    "lines",
    "load",
    "localtime",
    "localtimestamp",
    "lock",
    "long",
    "longblob",
    "longtext",
    "loop",
    "low_priority",
    "manual",
    "master_bind",
    "match",
    "maxvalue",
    "mediumblob",
    "mediumint",
    "mediumtext",
    "middleint",
    "minute_microsecond",
    "minute_second",
    "mod",
    "modifies",
    "natural",
    "no_write_to_binlog",
    "not",
    "notnull",
    "nth_value",
    "ntile",
    "null",
    "numeric",
    "of",
    "offset",
    "on",
    "only",
    "optimize",
    "optimizer_costs",
    "option",
    "optionally",
    "or",
    "order",
    "out",
    "outer",
    "outfile",
    "over",
    "overlaps",
    "parallel",
    "partition",
    "percent_rank",
    "placing",
    "precision",
    "primary",
    "procedure",
    "purge",
    "qualify",
    "range",
    "rank",
    "read",
    "read_write",
    "reads",
    "real",
    "recursive",
    "references",
    "regexp",
    "release",
    "rename",
    "repeat",
    "replace",
    "require",
    "resignal",
    "restrict",
    "return",
    "returning",
    "revoke",
    "right",
    "rlike",
    "row",
    "row_number",
    "rows",
    "schema",
    "schemas",
    "second_microsecond",
    "select",
    "sensitive",
    "separator",
    "session_user",
    "set",
    "show",
    "signal",
    "similar",
    "smallint",
    "some",
    "spatial",
    "specific",
    "sql",
    "sql_big_result",
    "sql_calc_found_rows",
    "sql_small_result",
    "sqlexception",
    "sqlstate",
    "sqlwarning",
    "ssl",
    "starting",
    "stored",
    "straight_join",
    "symmetric",
    "system",
    "system_user",
    "table",
    "tablesample",
    "terminated",
    "then",
    "tinyblob",
    "tinyint",
    "tinytext",
    "to",
    "trailing",
    "trigger",
    "true",
    "undo",
    "union",
    "unique",
    "unlock",
    "unsigned",
    "update",
    "usage",
    "use",
    "user",
    "using",
    "utc_date",
    "utc_time",
    "utc_timestamp",
    "values",
    "varbinary",
    "varchar",
    "varcharacter",
    "variadic",
    "varying",
    "verbose",
    "virtual",
    "when",
    "where",
    "while",
    "window",
    "with",
    "write",
    "xor",
    "year_month",
    "zerofill",
];

/// Whether a lowercase identifier is a word one of the two engines reserves.
fn is_reserved_sql_word(identifier: &str) -> bool {
    RESERVED_SQL_WORDS.binary_search(&identifier).is_ok()
}

fn validate_type_name(name: &str) -> Result<(), CliError> {
    let valid = !name.is_empty()
        && name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    if valid {
        let table = snake_case(name);
        if is_reserved_sql_word(&table) {
            return Err(CliError::new(
                Code::UnsupportedValue,
                format!(
                    "`{name}` would name the table `{table}`, which PostgreSQL or MySQL reserves \
                     as a keyword; the generated SQL uses the name bare, so choose another, such \
                     as `{name}Record`"
                ),
            )
            .with("flag", "name")
            .with("reason", "reserved_identifier")
            .with("supported", "PascalCase, not an SQL keyword"));
        }
        return Ok(());
    }
    Err(CliError::new(
        Code::UnsupportedValue,
        format!(
            "`{name}` is not a resource name: PascalCase ASCII letters and digits, starting with \
             an upper-case letter, at most 32 characters"
        ),
    )
    .with("flag", "name")
    .with("supported", "PascalCase, such as `Post` or `BlogPost`"))
}

/// The context the resource templates render with.
#[derive(Debug, serde::Serialize)]
struct ResourceContext {
    type_name: String,
    snake: String,
    plural: String,
    table: String,
    database: String,
    orm: String,
    auth_session: bool,
    sqlx_row: &'static str,
    fields: Vec<Field>,
    select_all_sql: String,
    select_one_sql: String,
    insert_sql: String,
    update_sql: String,
    delete_sql: String,
    sample_a: String,
    sample_b: String,
}

impl ResourceContext {
    fn build(
        type_name: &str,
        fields: Vec<Field>,
        database: &str,
        orm: &str,
        auth_session: bool,
    ) -> Self {
        let snake = snake_case(type_name);
        let postgres = database == "postgres";
        let placeholder = |n: usize| {
            if postgres {
                format!("${n}")
            } else {
                "?".to_owned()
            }
        };
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        let columns = std::iter::once("id")
            .chain(names.iter().copied())
            .collect::<Vec<_>>()
            .join(", ");
        let values = (1..=fields.len())
            .map(placeholder)
            .collect::<Vec<_>>()
            .join(", ");
        let sets = names
            .iter()
            .enumerate()
            .map(|(i, name)| format!("{name} = {}", placeholder(i + 1)))
            .collect::<Vec<_>>()
            .join(", ");
        let insert_sql = if postgres {
            format!(
                "INSERT INTO {snake} ({}) VALUES ({values}) RETURNING {columns}",
                names.join(", ")
            )
        } else {
            format!(
                "INSERT INTO {snake} ({}) VALUES ({values})",
                names.join(", ")
            )
        };
        let sample = |pick: fn(&Field) -> &'static str| {
            let pairs: Vec<String> = fields
                .iter()
                .map(|f| format!("\"{}\":{}", f.name, pick(f)))
                .collect();
            format!("{{{}}}", pairs.join(","))
        };
        Self {
            type_name: type_name.to_owned(),
            plural: plural(&snake),
            table: snake.clone(),
            database: database.to_owned(),
            orm: orm.to_owned(),
            auth_session,
            sqlx_row: if postgres {
                "sqlx::postgres::PgRow"
            } else {
                "sqlx::mysql::MySqlRow"
            },
            insert_sql,
            select_all_sql: format!("SELECT {columns} FROM {snake} ORDER BY id"),
            select_one_sql: format!(
                "SELECT {columns} FROM {snake} WHERE id = {}",
                placeholder(1)
            ),
            update_sql: format!(
                "UPDATE {snake} SET {sets} WHERE id = {}",
                placeholder(fields.len() + 1)
            ),
            delete_sql: format!("DELETE FROM {snake} WHERE id = {}", placeholder(1)),
            sample_a: sample(|f| f.sample_a),
            sample_b: sample(|f| f.sample_b),
            fields,
            snake,
        }
    }
}

/// Formats rendered Rust with the toolchain's `rustfmt`, so a module whose line widths follow
/// the user's names is laid out the way `cargo fmt --check` will demand, deterministically.
///
/// # Errors
///
/// [`Code::ToolMissing`] when `rustfmt` cannot be run; [`Code::RenderFailed`] when it rejects
/// the rendered source, which is a defect in the template.
fn rustfmt(source: &str) -> Result<String, CliError> {
    use std::io::Write as _;
    let mut child = std::process::Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout", "--quiet"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| {
            CliError::new(
                Code::ToolMissing,
                format!("`rustfmt` could not be run to format the generated module: {error}"),
            )
            .with("tool", "rustfmt")
            .with("required", "true")
            .with("found", "false")
        })?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(source.as_bytes()).map_err(|error| {
            CliError::new(
                Code::RenderFailed,
                format!("`rustfmt` did not accept the generated module: {error}"),
            )
        })?;
    }
    let output = child.wait_with_output().map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!("`rustfmt` did not finish: {error}"),
        )
    })?;
    if !output.status.success() {
        return Err(CliError::new(
            Code::RenderFailed,
            format!(
                "`rustfmt` rejected the generated module; this is a defect in renvor's template:\n{}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| {
        CliError::new(
            Code::RenderFailed,
            "`rustfmt` produced output that is not UTF-8",
        )
    })
}

/// Inserts `line` before the marker `end` inside `text`, unless it is already there.
fn insert_before_marker(text: &str, file: &str, end: &str, line: &str) -> Result<String, CliError> {
    if text.lines().any(|existing| existing.trim() == line.trim()) {
        return Ok(text.to_owned());
    }
    let Some(at) = text.find(end) else {
        return Err(CliError::new(
            Code::RenderFailed,
            format!(
                "`{file}` has no `{}` marker, so the generated resource cannot be registered; \
                 restore the marker pair `renvor generate resource` relies on",
                end.trim()
            ),
        ));
    };
    let line_start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let mut out = String::with_capacity(text.len() + line.len() + 1);
    out.push_str(&text[..line_start]);
    out.push_str(line);
    out.push('\n');
    out.push_str(&text[line_start..]);
    Ok(out)
}

/// The starter's own answers, read back from its manifest, so its generator-owned files can be
/// rendered again exactly as `renvor new` rendered them.
fn answers_from_manifest(
    manifest: &check::Manifest,
    destination: std::path::PathBuf,
) -> Result<crate::config::model::Answers, CliError> {
    let Some(framework) = manifest.framework.as_ref() else {
        return Err(CliError::new(
            Code::TransportNotWired,
            "this project is a dependency-free skeleton, and a skeleton has nothing a generated \
             resource could be wired into; generate a starter with `--framework-path` instead",
        )
        .with(
            "transport",
            manifest.project.transport.clone().unwrap_or_default(),
        )
        .with("reason", "no_renvor_dependency"));
    };
    let container = manifest.container.as_ref();
    Ok(crate::config::model::Answers {
        name: Some(manifest.project.name.clone()),
        destination,
        local_domain: Some(manifest.project.local_domain.clone()),
        target: manifest.project.target.clone(),
        transport: manifest.project.transport.clone(),
        container: manifest.project.container,
        local_https: manifest.project.local_https == "on",
        seed_data: manifest.project.seed_data,
        example_domain: manifest.project.example_domain,
        orm: manifest.persistence.as_ref().map(|p| p.orm.clone()),
        database: manifest.persistence.as_ref().map(|p| p.database.clone()),
        database_version: container.and_then(|c| c.database_version.clone()),
        database_name: container.and_then(|c| c.database_name.clone()),
        database_user: container.and_then(|c| c.database_user.clone()),
        database_port: container.and_then(|c| c.database_port.map(|p| p.to_string())),
        container_cache: container.map(|c| c.cache.clone()),
        cache_port: container.and_then(|c| c.cache_port.map(|p| p.to_string())),
        auth: manifest.project.auth.clone(),
        capabilities: manifest.capabilities.as_ref().map(|c| {
            let selected = c.selected();
            if selected.is_empty() {
                "none".to_owned()
            } else {
                selected.join(",")
            }
        }),
        framework_path: Some(std::path::PathBuf::from(&framework.path)),
    })
}

/// Renders the starter's generator-owned file at `path` exactly as `renvor new` would today.
fn rerender_starter_file(manifest: &check::Manifest, path: &str) -> Result<Vec<u8>, CliError> {
    let scratch = tempfile::tempdir().map_err(|error| {
        CliError::new(
            Code::StagingFailed,
            format!("a scratch directory could not be created: {error}"),
        )
    })?;
    let answers = answers_from_manifest(manifest, scratch.path().join(&manifest.project.name))?;
    let (configuration, _destination) =
        crate::config::model::ProjectConfiguration::resolve(answers)?;
    let context = crate::commands::new::Context::build(&configuration);
    let renderer =
        crate::generate::render::Renderer::new(crate::templates::select(&configuration))?;
    let render_root = scratch.path().join("render");
    std::fs::create_dir_all(&render_root).map_err(|error| {
        CliError::new(
            Code::StagingFailed,
            format!("a scratch directory could not be created: {error}"),
        )
    })?;
    let dir =
        Dir::open_ambient_dir(&render_root, cap_std::ambient_authority()).map_err(|error| {
            CliError::new(
                Code::StagingFailed,
                format!("a scratch directory could not be opened: {error}"),
            )
        })?;
    renderer.render_into(&dir, &context)?;
    dir.read(path).map_err(|error| {
        CliError::new(
            Code::RenderFailed,
            format!("`{path}` was not rendered by the starter templates: {error}"),
        )
    })
}

const RESOURCE_SQLX: &str = include_str!("../../templates/generate/resource_sqlx.rs.j2");
const RESOURCE_SEAORM: &str = include_str!("../../templates/generate/resource_seaorm.rs.j2");
const RESOURCE_UP: &str = include_str!("../../templates/generate/resource_migration_up.sql.j2");
const RESOURCE_DOWN: &str = include_str!("../../templates/generate/resource_migration_down.sql.j2");
const RESOURCE_TEST: &str = include_str!("../../templates/generate/resource_test.rs.j2");
const ITEM_OWNER_UP: &str = include_str!("../../templates/generate/item_owner_up.sql.j2");
const ITEM_OWNER_DOWN: &str = include_str!("../../templates/generate/item_owner_down.sql.j2");

/// The parsed columns, each name once.
fn parse_fields(field_specs: &[String], engine: &str) -> Result<Vec<Field>, CliError> {
    let mut fields = Vec::with_capacity(field_specs.len());
    for spec in field_specs {
        let parsed = field(spec, engine)?;
        if fields.iter().any(|f: &Field| f.name == parsed.name) {
            return Err(CliError::new(
                Code::UnsupportedValue,
                format!("`{}` is given twice", parsed.name),
            )
            .with("flag", "FIELD:TYPE")
            .with("supported", "each field once"));
        }
        fields.push(parsed);
    }
    Ok(fields)
}

/// Renders a resource's module and test for `auth_session` — the same two files whether they
/// are generated for the first time or rendered again when the auth starter is added.
fn render_resource(
    persistence: &check::PersistenceTable,
    name: &str,
    fields: Vec<Field>,
    auth_session: bool,
) -> Result<(ResourceContext, String, String), CliError> {
    let context = ResourceContext::build(
        name,
        fields,
        &persistence.database,
        &persistence.orm,
        auth_session,
    );
    let module_template = if persistence.orm == "seaorm" {
        RESOURCE_SEAORM
    } else {
        RESOURCE_SQLX
    };
    let module = rustfmt(&crate::generate::render::render_body(
        module_template,
        &context,
        true,
    )?)?;
    let test = rustfmt(&crate::generate::render::render_body(
        RESOURCE_TEST,
        &context,
        true,
    )?)?;
    Ok((context, module, test))
}

/// Plans a resource: its module, its migration pair, its test, the support module it shares, and
/// the two marker edits — and the definition the record keeps so `generate auth` can render the
/// module again.
fn plan_resource(
    project: &Dir,
    manifest: &check::Manifest,
    name: &str,
    field_specs: &[String],
) -> Result<
    (
        String,
        Vec<Planned>,
        crate::generate::record::GeneratedResource,
    ),
    CliError,
> {
    validate_type_name(name)?;
    let Some(persistence) = manifest.persistence.as_ref() else {
        return Err(CliError::new(
            Code::UnsupportedCombination,
            "this project has no database (`renvor.toml` has no `[persistence]` table), so a \
             resource has nowhere to live; generate a project with `--database`",
        )
        .with("flags", "resource")
        .with("reason", "no_database"));
    };
    if manifest.framework.is_none() {
        return Err(CliError::new(
            Code::TransportNotWired,
            "this project is a dependency-free skeleton, and a skeleton has nothing a generated \
             resource could be wired into; generate a starter with `--framework-path` instead",
        )
        .with(
            "transport",
            manifest.project.transport.clone().unwrap_or_default(),
        )
        .with("reason", "no_renvor_dependency"));
    }
    if field_specs.is_empty() {
        return Err(CliError::new(
            Code::Usage,
            "a resource needs at least one field, as `name:type`",
        ));
    }
    let fields = parse_fields(field_specs, &persistence.database)?;
    let auth_session = manifest.project.auth.as_deref() == Some("session");
    let (context, module, test) = render_resource(persistence, name, fields, auth_session)?;
    let up = crate::generate::render::render_body(RESOURCE_UP, &context, true)?;
    let down = crate::generate::render::render_body(RESOURCE_DOWN, &context, true)?;
    let migration_name = format!("create_{}", context.snake);
    let version = version_for(project, &migration_name)?;

    let read = |path: &str| -> Result<String, CliError> {
        project.read_to_string(path).map_err(|error| {
            CliError::new(
                Code::RenderFailed,
                format!("`{path}` could not be read: {error}"),
            )
        })
    };
    let modules = insert_before_marker(
        &read("src/resources/mod.rs")?,
        "src/resources/mod.rs",
        "// renvor:resources:modules:end",
        &format!("pub mod {};", context.snake),
    )?;
    let routes = insert_before_marker(
        &read("src/routes.rs")?,
        "src/routes.rs",
        "    // renvor:resources:end",
        &format!(
            "    crate::resources::{}::declare(&mut routes)?;",
            context.snake
        ),
    )?;
    let support = rerender_starter_file(manifest, "tests/support/mod.rs")?;

    let what = format!("resource `{name}` at `/{}`", context.plural);
    let definition = crate::generate::record::GeneratedResource {
        name: name.to_owned(),
        fields: field_specs.to_vec(),
    };
    Ok((
        what,
        vec![
            Planned::file(
                format!("src/resources/{}.rs", context.snake),
                module.into_bytes(),
            ),
            Planned::file(
                format!("migrations/{version}_{migration_name}.up.sql"),
                up.into_bytes(),
            ),
            Planned::file(
                format!("migrations/{version}_{migration_name}.down.sql"),
                down.into_bytes(),
            ),
            Planned::file(format!("tests/{}.rs", context.snake), test.into_bytes()),
            Planned::file("tests/support/mod.rs", support),
            Planned::edit("src/resources/mod.rs", modules.into_bytes()),
            Planned::edit("src/routes.rs", routes.into_bytes()),
        ],
        definition,
    ))
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    #[test]
    fn names_are_pascal_case_and_columns_are_the_closed_set() {
        for name in ["Post", "BlogPost", "A1"] {
            validate_type_name(name).expect(name);
        }
        for name in ["post", "blog_post", "", "Post-1", &"P".repeat(33)] {
            assert_eq!(
                validate_type_name(name).expect_err(name).code,
                Code::UnsupportedValue
            );
        }
        assert_eq!(
            field("title:string", "postgres").expect("ok").sql_type,
            "VARCHAR(255)"
        );
        assert_eq!(
            field("ratio:float", "mysql").expect("ok").sql_type,
            "DOUBLE"
        );
        assert_eq!(
            field("ratio:float", "postgres").expect("ok").sql_type,
            "DOUBLE PRECISION"
        );
        for spec in [
            "title",
            "title:uuid",
            "id:integer",
            "Title:string",
            ":string",
        ] {
            assert_eq!(
                field(spec, "postgres").expect_err(spec).code,
                Code::UnsupportedValue
            );
        }
    }

    #[test]
    fn a_reserved_sql_word_is_refused_as_a_table_or_a_column() {
        // FOUND BY THE CODEX REVIEW (P2). `Order` passed the PascalCase check and became the
        // bare identifier `order` in every generated statement, which neither engine parses;
        // the failure surfaced at the project's first query, after generation had reported
        // success. Columns are bound bare too, so a field named `key` is the same defect.
        assert!(
            RESERVED_SQL_WORDS.windows(2).all(|pair| pair[0] < pair[1]),
            "sorted and unique, for the binary search"
        );
        for name in ["Order", "User", "Group", "Select", "Table", "Key", "Window"] {
            let error = validate_type_name(name).expect_err(name);
            assert_eq!(error.code, Code::UnsupportedValue, "{name}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "reason" && v == "reserved_identifier"),
                "{name}: {:?}",
                error.details
            );
        }
        for spec in [
            "key:string",
            "order:integer",
            "user:string",
            "index:integer",
        ] {
            let error = field(spec, "postgres").expect_err(spec);
            assert_eq!(error.code, Code::UnsupportedValue, "{spec}");
            assert!(
                error
                    .details
                    .iter()
                    .any(|(k, v)| k == "reason" && v == "reserved_identifier"),
                "{spec}: {:?}",
                error.details
            );
        }
        // POSITIVE CONTROL: a two-word name is never a reserved word, and ordinary names pass.
        for name in ["BlogPost", "OrderLine", "Post", "Item"] {
            validate_type_name(name).expect(name);
        }
        for spec in ["title:string", "order_index:integer", "user_name:string"] {
            field(spec, "mysql").expect(spec);
        }
    }

    #[test]
    fn the_route_path_is_the_plural_of_the_snake_case_name() {
        assert_eq!(snake_case("BlogPost"), "blog_post");
        assert_eq!(plural("post"), "posts");
        assert_eq!(plural("category"), "categories");
        assert_eq!(plural("day"), "days");
        assert_eq!(plural("box"), "boxes");
        assert_eq!(plural("address"), "addresses");
    }

    #[test]
    fn the_sql_is_engine_specific_and_the_samples_differ() {
        let fields = vec![
            field("title:string", "postgres").expect("ok"),
            field("published:boolean", "postgres").expect("ok"),
        ];
        let context = ResourceContext::build("Post", fields.clone(), "postgres", "sqlx", true);
        assert_eq!(
            context.insert_sql,
            "INSERT INTO post (title, published) VALUES ($1, $2) RETURNING id, title, published"
        );
        assert_eq!(
            context.update_sql,
            "UPDATE post SET title = $1, published = $2 WHERE id = $3"
        );
        assert_ne!(context.sample_a, context.sample_b);
        assert_eq!(context.sample_a, r#"{"title":"alpha","published":true}"#);
        let mysql = ResourceContext::build("Post", fields, "mysql", "sqlx", false);
        assert_eq!(
            mysql.insert_sql,
            "INSERT INTO post (title, published) VALUES (?, ?)"
        );
        assert_eq!(
            mysql.select_one_sql,
            "SELECT id, title, published FROM post WHERE id = ?"
        );
    }

    #[test]
    fn a_marker_insertion_is_idempotent_and_a_missing_marker_is_refused() {
        let text = "a\n    // renvor:resources:end\nb\n";
        let once = insert_before_marker(text, "f", "    // renvor:resources:end", "    x();")
            .expect("inserts");
        assert_eq!(once, "a\n    x();\n    // renvor:resources:end\nb\n");
        let twice = insert_before_marker(&once, "f", "    // renvor:resources:end", "    x();")
            .expect("idempotent");
        assert_eq!(twice, once);
        assert_eq!(
            insert_before_marker(
                "no marker\n",
                "f",
                "    // renvor:resources:end",
                "    x();"
            )
            .expect_err("refused")
            .code,
            Code::RenderFailed
        );
    }
}

// ---- the auth starter, added later --------------------------------------------------------

/// Splices the marked block `begin`…`end` of `current` into the same block of `rendered`, so a
/// re-render of a generator-owned file keeps the lines other generators added to it.
fn carry_marked_block(rendered: &str, current: &str, begin: &str, end: &str) -> String {
    let block = |text: &str| -> Option<(usize, usize)> {
        let start = text.find(begin)?;
        let start = start
            + text[start..]
                .find('\n')
                .map_or(text[start..].len(), |i| i + 1);
        let stop = text[start..].find(end)? + start;
        let stop = text[..stop].rfind('\n').map_or(stop, |i| i + 1);
        Some((start, stop))
    };
    match (block(rendered), block(current)) {
        (Some((rs, re)), Some((cs, ce))) => {
            let mut out = String::with_capacity(rendered.len() + (ce - cs));
            out.push_str(&rendered[..rs]);
            out.push_str(&current[cs..ce]);
            out.push_str(&rendered[re..]);
            out
        }
        _ => rendered.to_owned(),
    }
}

/// Files whose marked blocks other generators fill, and the markers that bound them.
const MARKED: [(&str, &str, &str); 2] = [
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

/// Plans the session authentication starter for a starter that has none: every generator-owned
/// file rendered again with `auth = "session"`, marked blocks carried over.
fn plan_auth(
    project: &Dir,
    manifest: &check::Manifest,
) -> Result<(String, Vec<Planned>), CliError> {
    let scratch = tempfile::tempdir().map_err(|error| {
        CliError::new(
            Code::StagingFailed,
            format!("a scratch directory could not be created: {error}"),
        )
    })?;
    let mut answers = answers_from_manifest(manifest, scratch.path().join(&manifest.project.name))?;
    // The same rules, the same refusals, as `renvor new --auth session`: a database and the
    // `mail` capability, or `unsupported_combination` naming the flag.
    answers.auth = Some("session".to_owned());
    let (configuration, _destination) =
        crate::config::model::ProjectConfiguration::resolve(answers)?;
    let context = crate::commands::new::Context::build(&configuration);
    let renderer =
        crate::generate::render::Renderer::new(crate::templates::select(&configuration))?;
    let render_root = scratch.path().join("render");
    std::fs::create_dir_all(&render_root).map_err(|error| {
        CliError::new(
            Code::StagingFailed,
            format!("a scratch directory could not be created: {error}"),
        )
    })?;
    let dir =
        Dir::open_ambient_dir(&render_root, cap_std::ambient_authority()).map_err(|error| {
            CliError::new(
                Code::StagingFailed,
                format!("a scratch directory could not be opened: {error}"),
            )
        })?;
    renderer.render_into(&dir, &context)?;
    let rendered = crate::generate::manifest::FileManifest::describe(&dir)?;
    let mut planned = Vec::with_capacity(rendered.entries.len());
    let mut item_table_exists = false;
    for entry in &rendered.entries {
        if entry.kind != crate::generate::manifest::EntryKind::File {
            continue;
        }
        // APPLIED MIGRATIONS ARE NEVER RE-PLANNED. The auth render produces the item migration
        // with `owner_id`; the project's copy is applied and recorded by its checksum, and a
        // rewrite would make SQLx refuse the ledger at the next Boot (found by the Codex review
        // of Phase 011). The column arrives by the forward migration planned below.
        if entry.path.starts_with("migrations/0001_") && project.exists(&entry.path) {
            item_table_exists = true;
            continue;
        }
        let mut bytes = dir.read(&entry.path).map_err(|error| {
            CliError::new(
                Code::RenderFailed,
                format!("`{}` could not be read back: {error}", entry.path),
            )
        })?;
        if let Some((_, begin, end)) = MARKED.iter().find(|(path, _, _)| *path == entry.path)
            && let Ok(current) = project.read_to_string(&entry.path)
            && let Ok(text) = String::from_utf8(bytes.clone())
        {
            bytes = carry_marked_block(&text, &current, begin, end).into_bytes();
        }
        planned.push(Planned::file(entry.path.clone(), bytes));
    }
    let Some(persistence) = manifest.persistence.as_ref() else {
        // `resolve` above refuses a session starter without a database; unreachable in practice,
        // reported rather than assumed.
        return Err(CliError::new(
            Code::UnsupportedCombination,
            "the session starter needs a database, and this project records none",
        )
        .with("flags", "--auth session")
        .with("reason", "no_database"));
    };
    if item_table_exists && manifest.project.example_domain {
        #[derive(serde::Serialize)]
        struct OwnerContext<'a> {
            database: &'a str,
        }
        let owner = OwnerContext {
            database: &persistence.database,
        };
        let version = version_for(project, "add_item_owner")?;
        planned.push(Planned::file(
            format!("migrations/{version}_add_item_owner.up.sql"),
            crate::generate::render::render_body(ITEM_OWNER_UP, &owner, true)?.into_bytes(),
        ));
        planned.push(Planned::file(
            format!("migrations/{version}_add_item_owner.down.sql"),
            crate::generate::render::render_body(ITEM_OWNER_DOWN, &owner, true)?.into_bytes(),
        ));
    }
    // EVERY RECORDED RESOURCE IS RENDERED AGAIN with the session guards the manifest now
    // promises; a module the user changed is a conflict like any other file, so the auth starter
    // is refused rather than added beside a public write route (found by the Codex review of
    // Phase 011).
    if let Some(record) = crate::generate::record::read(project)? {
        for resource in &record.resources {
            validate_type_name(&resource.name)?;
            let fields = parse_fields(&resource.fields, &persistence.database)?;
            let (context, module, test) =
                render_resource(persistence, &resource.name, fields, true)?;
            planned.push(Planned::file(
                format!("src/resources/{}.rs", context.snake),
                module.into_bytes(),
            ));
            planned.push(Planned::file(
                format!("tests/{}.rs", context.snake),
                test.into_bytes(),
            ));
        }
    }
    Ok(("the session authentication starter".to_owned(), planned))
}

#[cfg(test)]
mod auth_tests {
    use super::*;

    /// A workspace-shaped directory `--framework-path` accepts; nothing in it is built.
    fn fake_framework(base: &std::path::Path) -> std::path::PathBuf {
        let root = base.join("framework");
        std::fs::create_dir_all(root.join("crates/renvor")).expect("mkdir");
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nresolver = \"3\"\nmembers = [\"crates/renvor\"]\n",
        )
        .expect("write");
        std::fs::write(
            root.join("crates/renvor/Cargo.toml"),
            "[package]\nname = \"renvor\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
        )
        .expect("write");
        std::fs::write(root.join("Cargo.lock"), "# lock\nversion = 4\n").expect("write");
        root
    }

    /// A starter rendered without the auth starter, with its record, as `renvor new` leaves it —
    /// minus the build. `example_domain` decides whether the item table exists.
    fn starter_without_auth(
        base: &std::path::Path,
        example_domain: bool,
    ) -> (std::path::PathBuf, Dir) {
        let framework = fake_framework(base);
        let destination = base.join("demo");
        let answers = crate::config::model::Answers {
            name: Some("demo".to_owned()),
            destination: destination.clone(),
            local_domain: None,
            target: "api".to_owned(),
            transport: None,
            container: false,
            local_https: false,
            seed_data: false,
            example_domain,
            orm: Some("sqlx".to_owned()),
            database: Some("postgres".to_owned()),
            database_version: None,
            database_name: None,
            database_user: None,
            database_port: None,
            container_cache: None,
            cache_port: None,
            auth: None,
            capabilities: Some("mail".to_owned()),
            framework_path: Some(framework),
        };
        let (configuration, _) =
            crate::config::model::ProjectConfiguration::resolve(answers).expect("resolves");
        let context = crate::commands::new::Context::build(&configuration);
        let renderer =
            crate::generate::render::Renderer::new(crate::templates::select(&configuration))
                .expect("builds");
        std::fs::create_dir_all(&destination).expect("mkdir");
        let dir = Dir::open_ambient_dir(&destination, cap_std::ambient_authority()).expect("opens");
        renderer.render_into(&dir, &context).expect("renders");
        crate::generate::record::write(&dir, "0.0.0", crate::templates::VERSION).expect("record");
        (destination, dir)
    }

    #[test]
    fn adding_auth_keeps_the_applied_item_migration_and_adds_the_owner_forward() {
        // FOUND BY THE CODEX REVIEW (P1). The auth render produces `0001_create_item.up.sql`
        // WITH `owner_id`; the project's copy — already applied, its checksum in the ledger —
        // matched the record, so it was overwritten, and SQLx refused the changed checksum at
        // the next Boot. An applied migration is never re-planned; the column arrives by a new
        // forward migration.
        let base = tempfile::tempdir().expect("tempdir");
        let (path, dir) = starter_without_auth(base.path(), true);
        let manifest = check::load(&path).expect("loads");
        let item_up = dir
            .read_to_string("migrations/0001_create_item.up.sql")
            .expect("the item migration");
        assert!(!item_up.contains("owner_id"), "{item_up}");

        let (_, planned) = plan_auth(&dir, &manifest).expect("plans");
        let paths: Vec<&str> = planned.iter().map(|p| p.path.as_str()).collect();
        assert!(
            !paths.iter().any(|p| p.starts_with("migrations/0001_")),
            "an applied migration was planned again: {paths:?}"
        );
        let up = planned
            .iter()
            .find(|p| {
                p.path.starts_with("migrations/") && p.path.ends_with("_add_item_owner.up.sql")
            })
            .unwrap_or_else(|| panic!("no forward migration adds the owner column: {paths:?}"));
        let sql = String::from_utf8(up.bytes.clone()).expect("utf-8");
        assert!(
            sql.contains("ALTER TABLE item ADD COLUMN owner_id"),
            "{sql}"
        );
        assert!(
            paths
                .iter()
                .any(|p| p.ends_with("_add_item_owner.down.sql")),
            "{paths:?}"
        );
        let version = &up.path["migrations/".len().."migrations/".len() + 14];
        assert!(version.bytes().all(|b| b.is_ascii_digit()), "{}", up.path);
        // The plan classifies the project's own migration as untouched — it is not in the plan —
        // and the rest as regenerated or new; nothing conflicts under the flag.
        let plan = apply::plan(&dir, planned, true).expect("no conflict");
        assert!(
            plan.summary()
                .iter()
                .all(|(p, _)| !p.starts_with("migrations/0001_"))
        );
        assert_eq!(
            dir.read_to_string("migrations/0001_create_item.up.sql")
                .expect("still there"),
            item_up,
            "the applied migration must stay byte-identical"
        );

        // POSITIVE CONTROL on the condition: without the example domain there is no item table,
        // so nothing is added forward.
        let base2 = tempfile::tempdir().expect("tempdir");
        let (path2, dir2) = starter_without_auth(base2.path(), false);
        let manifest2 = check::load(&path2).expect("loads");
        let (_, planned2) = plan_auth(&dir2, &manifest2).expect("plans");
        assert!(
            !planned2.iter().any(|p| p.path.contains("_add_item_owner")),
            "a forward migration for a table that does not exist"
        );
    }

    #[test]
    fn adding_auth_renders_every_recorded_resource_again_with_its_guards() {
        // FOUND BY THE CODEX REVIEW (P1). A resource generated before the auth starter was
        // rendered with `auth_session = false`, and the auth plan re-rendered only the starter's
        // own files, so the resource's POST, PUT, and DELETE stayed public under a manifest that
        // promised session writes. The record now carries each resource's definition, and the
        // auth plan renders every one again; an edited module is a conflict like any other.
        let base = tempfile::tempdir().expect("tempdir");
        let (path, dir) = starter_without_auth(base.path(), true);
        let manifest = check::load(&path).expect("loads");
        let (_, planned, definition) =
            plan_resource(&dir, &manifest, "Post", &["title:string".to_owned()])
                .expect("plans the resource");
        let plan = apply::plan(&dir, planned, false)
            .expect("a resource creates and edits: no flag needed")
            .with_resource(definition);
        apply::commit(&dir, plan, "0.0.0", crate::templates::VERSION).expect("commits");
        let before = dir.read_to_string("src/resources/post.rs").expect("module");
        assert!(!before.contains("require_session"), "{before}");

        let (_, planned) = plan_auth(&dir, &manifest).expect("plans");
        let module = planned
            .iter()
            .find(|p| p.path == "src/resources/post.rs")
            .expect("the recorded resource is rendered again");
        let text = String::from_utf8(module.bytes.clone()).expect("utf-8");
        assert!(text.contains("require_session"), "{text}");
        assert!(
            planned.iter().any(|p| p.path == "tests/post.rs"),
            "the resource's test is rendered again too"
        );
        assert!(
            !planned.iter().any(|p| p.path.contains("_create_post.")),
            "the resource's migration does not depend on auth and is not re-planned"
        );
        let plan = apply::plan(&dir, planned, true).expect("an untouched module regenerates");
        assert!(
            plan.summary()
                .iter()
                .any(|(p, a)| *p == "src/resources/post.rs" && *a == Applied::Regenerate),
            "{:?}",
            plan.summary()
        );

        // A module the user edited is a conflict, and nothing is written.
        let mut edited = before.clone();
        edited.push_str("\n// mine\n");
        dir.write("src/resources/post.rs", edited.as_bytes())
            .expect("write");
        let (_, planned) = plan_auth(&dir, &manifest).expect("plans");
        let error = apply::plan(&dir, planned, true).expect_err("an edited module is a conflict");
        assert_eq!(error.code, Code::GenerationConflict);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "paths" && v.contains("src/resources/post.rs")),
            "{:?}",
            error.details
        );
    }

    /// One detail of an error, by key.
    fn detail<'a>(error: &'a CliError, key: &str) -> Option<&'a str> {
        error
            .details
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The lines between the resources markers of `src/routes.rs`.
    fn resources_block(text: &str) -> &str {
        let (_, begin, end) = MARKED[1];
        let start = text.find(begin).expect("begin marker");
        let start = start + text[start..].find('\n').expect("newline") + 1;
        let stop = text[start..].find(end).expect("end marker") + start;
        &text[start..stop]
    }

    #[test]
    fn adding_auth_needs_the_flag_and_replaces_only_what_the_generator_owns() {
        // FR-048 AS DECIDED (2026-09-05). `generate auth` renders every generator-owned file
        // again, so on a starter nobody edited each of them is regenerable: without
        // `--overwrite-unchanged` the run is refused, names the flag, and writes nothing; with
        // it they are replaced — and a marked file's block, the generators' shared zone, is
        // carried across the re-render, so only the bytes the generator owns change.
        let base = tempfile::tempdir().expect("tempdir");
        let (path, dir) = starter_without_auth(base.path(), true);
        let manifest = check::load(&path).expect("loads");
        let (_, planned, definition) =
            plan_resource(&dir, &manifest, "Post", &["title:string".to_owned()])
                .expect("plans the resource");
        let plan = apply::plan(&dir, planned, false)
            .expect("a resource creates and edits: no flag needed")
            .with_resource(definition);
        apply::commit(&dir, plan, "0.0.0", crate::templates::VERSION).expect("commits");
        let routes_before = dir.read_to_string("src/routes.rs").expect("routes");
        assert!(
            resources_block(&routes_before).contains("crate::resources::post::declare"),
            "{routes_before}"
        );
        let main_before = dir.read_to_string("src/main.rs").expect("main");

        let (_, planned) = plan_auth(&dir, &manifest).expect("plans");
        let error = apply::plan(&dir, planned, false).expect_err("regenerable files need the flag");
        assert_eq!(error.code, Code::GenerationConflict);
        assert_eq!(detail(&error, "reason"), Some("overwrite_required"));
        assert_eq!(detail(&error, "flag"), Some(apply::OVERWRITE_FLAG));
        let regenerable = detail(&error, "regenerable").expect("the regenerable paths are named");
        for owned in ["src/main.rs", "src/routes.rs", "Cargo.toml", "renvor.toml"] {
            assert!(
                regenerable.contains(owned),
                "{owned} missing from: {regenerable}"
            );
        }
        assert_eq!(detail(&error, "changed"), None, "{:?}", error.details);
        assert!(!dir.exists("src/auth.rs"), "a refusal wrote the starter");
        assert_eq!(
            dir.read_to_string("src/main.rs").expect("main"),
            main_before,
            "a refusal replaced a regenerable file"
        );

        let (_, planned) = plan_auth(&dir, &manifest).expect("plans");
        let plan =
            apply::plan(&dir, planned, true).expect("with the flag, regenerable regenerates");
        assert!(
            plan.summary()
                .iter()
                .any(|(p, a)| *p == "src/routes.rs" && *a == Applied::Regenerate),
            "{:?}",
            plan.summary()
        );
        apply::commit(&dir, plan, "0.0.0", crate::templates::VERSION).expect("commits");
        let routes_after = dir.read_to_string("src/routes.rs").expect("routes");
        assert_ne!(
            routes_after, routes_before,
            "the generator-owned bytes changed"
        );
        assert_eq!(
            resources_block(&routes_after),
            resources_block(&routes_before),
            "the block — the shared zone — is carried verbatim"
        );
        assert!(dir.exists("src/auth.rs"));
        assert_ne!(
            dir.read_to_string("src/main.rs").expect("main"),
            main_before
        );
    }

    #[test]
    fn a_line_outside_the_markers_survives_an_edit_and_refuses_a_re_render_with_the_flag() {
        // FR-048 AS DECIDED: the flag replaces what is unchanged since generation and nothing
        // else. A line the user added to a marked file outside its markers survives a resource's
        // marker edit — the edit touches the block only — and makes the auth re-render a
        // conflict, flag or no flag, so it is never overwritten.
        let base = tempfile::tempdir().expect("tempdir");
        let (path, dir) = starter_without_auth(base.path(), true);
        let manifest = check::load(&path).expect("loads");
        let mine = "// the user's own registration lives here\n";
        let original = dir.read_to_string("src/routes.rs").expect("routes");
        dir.write("src/routes.rs", format!("{original}{mine}"))
            .expect("write");
        let (_, planned, definition) =
            plan_resource(&dir, &manifest, "Post", &["title:string".to_owned()])
                .expect("plans the resource");
        let plan = apply::plan(&dir, planned, false)
            .expect("an edit of the block is not a conflict")
            .with_resource(definition);
        assert!(
            plan.summary()
                .iter()
                .any(|(p, a)| *p == "src/routes.rs" && *a == Applied::Edit),
            "{:?}",
            plan.summary()
        );
        apply::commit(&dir, plan, "0.0.0", crate::templates::VERSION).expect("commits");
        let edited = dir.read_to_string("src/routes.rs").expect("routes");
        assert!(
            edited.ends_with(mine),
            "the user's line outside the markers was lost:\n{edited}"
        );
        assert!(
            resources_block(&edited).contains("crate::resources::post::declare(&mut routes)?;"),
            "{edited}"
        );

        for flag in [false, true] {
            let (_, planned) = plan_auth(&dir, &manifest).expect("plans");
            let error =
                apply::plan(&dir, planned, flag).expect_err("a changed file refuses the re-render");
            assert_eq!(error.code, Code::GenerationConflict, "flag = {flag}");
            assert_eq!(
                detail(&error, "reason"),
                Some("changed_since_generation"),
                "flag = {flag}"
            );
            let changed = detail(&error, "changed").expect("the changed file is named");
            assert!(
                changed.contains("src/routes.rs"),
                "flag = {flag}: {changed}"
            );
            assert_eq!(
                dir.read_to_string("src/routes.rs").expect("routes"),
                edited,
                "flag = {flag}: the user's file was touched"
            );
            assert!(!dir.exists("src/auth.rs"), "flag = {flag}");
        }
    }

    #[test]
    fn a_re_render_keeps_what_other_generators_put_between_the_markers() {
        let rendered = "head\n    // begin\n    // end\ntail\n";
        let current = "old head\n    // begin\n    added();\n    // end\nold tail\n";
        assert_eq!(
            carry_marked_block(rendered, current, "// begin", "// end"),
            "head\n    // begin\n    added();\n    // end\ntail\n"
        );
        // A file without the markers is taken as rendered.
        assert_eq!(
            carry_marked_block("plain\n", current, "// begin", "// end"),
            "plain\n"
        );
    }
}
