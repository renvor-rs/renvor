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
        Action::Resource { name, fields } => plan_resource(&project, &manifest, &name, &fields)?,
        Action::Auth => plan_auth(&project, &manifest)?,
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

fn validate_type_name(name: &str) -> Result<(), CliError> {
    let valid = !name.is_empty()
        && name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    if valid {
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

/// Plans a resource: its module, its migration pair, its test, the support module it shares, and
/// the two marker edits.
fn plan_resource(
    project: &Dir,
    manifest: &check::Manifest,
    name: &str,
    field_specs: &[String],
) -> Result<(String, Vec<Planned>), CliError> {
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
    let mut fields = Vec::with_capacity(field_specs.len());
    for spec in field_specs {
        let parsed = field(spec, &persistence.database)?;
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
    let auth_session = manifest.project.auth.as_deref() == Some("session");
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
    let up = crate::generate::render::render_body(RESOURCE_UP, &context, true)?;
    let down = crate::generate::render::render_body(RESOURCE_DOWN, &context, true)?;
    let migration_name = format!("create_{}", context.snake);
    let version = match existing_version(project, &migration_name)? {
        Some(version) => version,
        None => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| {
                    CliError::new(Code::Internal, "the system clock is before the Unix epoch")
                })?
                .as_secs();
            utc_version(now)
        }
    };

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
    for entry in &rendered.entries {
        if entry.kind != crate::generate::manifest::EntryKind::File {
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
    Ok(("the session authentication starter".to_owned(), planned))
}

#[cfg(test)]
mod auth_tests {
    use super::carry_marked_block;

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
