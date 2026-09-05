//! `renvor generate`: additions to an existing project, rerun-safe (Phase 011).
//!
//! # Into an existing tree, beside the user's files
//!
//! `renvor new` writes where nothing was. Everything here writes where the user already works,
//! so it goes through [`crate::generate::apply`]: every target path is classified against the
//! working tree and the provenance record before the first byte is written, and a file the user
//! changed since generation is a `generation_conflict` that writes nothing. A rerun of the same
//! command is a no-op that says so.
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
        Planned {
            path: format!("migrations/{version}_{name}.up.sql"),
            bytes: up.into_bytes(),
        },
        Planned {
            path: format!("migrations/{version}_{name}.down.sql"),
            bytes: down.into_bytes(),
        },
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
        .map(|(name, contents)| Planned {
            path: format!("migrations/{name}"),
            bytes: contents.as_bytes().to_vec(),
        })
        .collect())
}

/// Runs a generation into the project at `path`.
///
/// # Errors
///
/// [`Code::ManifestInvalid`] when `renvor.toml` does not validate; [`Code::UnsupportedCombination`]
/// when the project has no database to migrate; [`Code::UnsupportedValue`] for a name or an
/// import outside the grammar; [`Code::GenerationConflict`] when a target file was changed since
/// generation — nothing is written; [`Code::RenderFailed`] when a write fails.
pub fn run(
    reporter: &Reporter,
    path: &Path,
    action: Action,
    dry_run: bool,
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
                    (format!("the framework's `{set}` migration set"), planned)
                }
                (Some(name), None) => {
                    validate_migration_name(&name)?;
                    let version = match existing_version(&project, &name)? {
                        Some(version) => version,
                        None => {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .map_err(|_| {
                                    CliError::new(
                                        Code::Internal,
                                        "the system clock is before the Unix epoch",
                                    )
                                })?
                                .as_secs();
                            utc_version(now)
                        }
                    };
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
    };

    let plan = apply::plan(&project, planned)?;
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
