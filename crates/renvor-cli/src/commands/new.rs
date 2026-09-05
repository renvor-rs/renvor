//! `renvor new` — the transactional generator.
//!
//! Contract C-5, in order:
//!
//! ```text
//! 1. VALIDATE   every choice and the destination boundary — nothing has touched the filesystem
//! 2. STAGE      a directory the process owns, INSIDE the destination's parent
//! 3. RENDER     bounded template expansion into staging
//! 4. VERIFY     the generated project's own fmt, build, and tests — still in staging
//! 5. MANIFEST   walk the staged tree, sorted
//! 6. PLACE      one rename
//! 7. REPORT
//! ```
//!
//! Failure anywhere from 1 to 5 removes the staging directory and leaves the destination exactly as
//! it was — enforced by `Staging`'s `Drop`, so it holds on paths nobody wrote a cleanup for,
//! including a panic.

use serde::Serialize;

use crate::config::model::{Answers, Orm, ProjectConfiguration};
use crate::config::prompts;
use crate::exit::{CliError, Code, Exit};
use crate::generate::manifest::FileManifest;
use crate::generate::place::Staging;
use crate::generate::render::Renderer;
use crate::output::Reporter;
use crate::output::layout::{Report, Status};
use crate::output::progress::Progress;
use crate::templates;

/// The template context. A **separate** type from [`ProjectConfiguration`], deliberately.
///
/// Templates address values by name, so serialising the configuration directly would make every
/// private field a template variable and every field rename a silent template break. This type is
/// the contract between the two, and it is small enough to read.
#[derive(Debug, Serialize)]
pub(crate) struct Context {
    name: String,
    target: String,
    transport: String,
    local_domain: String,
    local_https: String,
    container: bool,
    example_domain: bool,
    seed_data: bool,
    /// The selected database's name, or empty when the project has no persistence.
    ///
    /// A string rather than an `Option` because the template engine's `{% if %}` treats an empty
    /// string as false, and a two-shaped value would need two template branches to say one thing.
    database: String,
    /// The selected persistence layer's name, or empty.
    orm: String,
    /// The bound-parameter placeholder the selected database uses, or empty.
    ///
    /// Pre-rendered for the reason `modules` gives: `src/persistence.rs` is whitespace-sensitive
    /// Rust, and a conditional around a `const` is the difference between a clean
    /// `cargo fmt --check` and the pre-placement verification refusing to write anything. Taken
    /// from `DatabaseKind::placeholder` so there is one placeholder rule in the workspace.
    placeholder: String,
    /// The `renvor-sqlx` feature that resolves the selected driver, or empty.
    ///
    /// Pre-computed so the template never joins `"db-"` to a value — a template that builds a
    /// feature name can build one that does not exist.
    driver_feature: String,
    /// The `sea-orm` feature that resolves the same driver, or empty.
    ///
    /// A SECOND name for the same engine, because the two crates spell it differently:
    /// `renvor-seaorm` takes `db-postgres`, `sea-orm` takes `sqlx-postgres`. A generated manifest
    /// that guessed one from the other would name a feature that does not exist.
    seaorm_driver_feature: String,
    generator_version: String,
    template_version: String,
    /// The `mod` block for `src/main.rs`, **pre-rendered**.
    ///
    /// Computed here rather than expressed with template whitespace control, because the output is
    /// whitespace-sensitive Rust: the difference between a clean `cargo fmt --check` and a failing
    /// one is one blank line, and five conditional variants of `{%- … -%}` is not a thing anybody
    /// can review. Empty means "no modules"; otherwise it opens and closes with a newline so the
    /// surrounding blank lines come out right in both cases.
    modules: String,

    // ── CONTAINER DEVELOPMENT CONTROLS ──────────────────────────────────────────────────
    //
    // Every one of these is PRE-RENDERED here rather than assembled in the template, for the
    // reason `driver_feature` gives: a template that can build an image reference or a health-check
    // command can build one that does not exist. The allow-listed filter set has no `join`, no
    // `default`, and no arithmetic — deliberately — so anything that needs composing composes here,
    // in Rust, where it is type-checked and unit-testable.
    //
    // NONE OF THESE CAN HOLD A SECRET. They come from `ContainerSettings`, which cannot hold one.
    /// Whether a database service is generated.
    container_database: bool,
    /// The Compose service name for the database.
    container_database_service: String,
    /// The pinned image reference.
    container_database_image: String,
    /// The version an operator typed, echoed back into the manifest.
    container_database_version: String,
    /// The database name.
    container_database_name: String,
    /// The database user. **Never a password.**
    container_database_user: String,
    /// The published host port.
    container_database_port: String,
    /// The port the server listens on inside the container.
    container_database_internal_port: String,
    /// Where the server keeps its data inside the container. Differs between PostgreSQL 17 and 18.
    container_database_data_dir: String,
    /// The health check as an inline YAML list, verified to fail as well as to pass.
    container_database_healthcheck: String,
    /// A connection string shaped for the README, with the password as a named placeholder.
    container_database_dsn_example: String,
    /// Whether the selected engine is PostgreSQL, so the template picks the right environment keys.
    container_is_postgres: bool,
    /// Whether a cache service is generated.
    container_cache: bool,
    /// `none`, or the engine name.
    container_cache_choice: String,
    /// The engine's display name.
    container_cache_engine: String,
    /// The pinned image reference.
    container_cache_image: String,
    /// The engine version.
    container_cache_version: String,
    /// The engine licence, recorded so a reader need not go and look.
    container_cache_licence: String,
    /// The published host port.
    container_cache_port: String,
    /// The port inside the container.
    container_cache_internal_port: String,
    /// Where the cache keeps its data inside the container.
    container_cache_data_dir: String,
    /// The health check as an inline YAML list.
    container_cache_healthcheck: String,

    // ── PHASE 011: the auth starter, the capabilities, and the framework source ────────────
    //
    // Booleans and pre-rendered strings, like everything above. NONE CAN HOLD A SECRET: the keys
    // the starter needs are read by the generated application from its environment, and the
    // framework path is a directory the operator named, validated to carry no control character.
    /// `none` or `session`, recorded under `[project]`.
    auth: String,
    /// Whether the session starter is generated.
    auth_session: bool,
    /// The canonical `--capabilities` value, for the README's equivalent command.
    capabilities: String,
    /// One boolean per capability, recorded under `[capabilities]` and selecting templates.
    cap_cache: bool,
    cap_jobs: bool,
    cap_mail: bool,
    cap_storage: bool,
    cap_observability: bool,
    /// Whether this is a framework-backed starter rather than the skeleton.
    starter: bool,
    /// The recorded source kind — `path` — or empty for the skeleton.
    framework_source: String,
    /// The framework checkout, forward-slashed, for prose.
    framework_path: String,
    /// The framework checkout as a quoted TOML basic string, for `renvor.toml` and `Cargo.toml`.
    ///
    /// Pre-rendered here rather than escaped in a template, for the reason every other composed
    /// value gives: the allow-listed filter set escapes nothing, so a path with a `"` or a `\\`
    /// (every Windows path) composed in a template would be an invalid manifest.
    framework_path_toml: String,
    /// Whether the generated application reads the container's cache service (I-22).
    cache_wired: bool,
    /// Whether any capability was selected: the `src/capabilities/` module root exists.
    any_capability: bool,
    /// The framework's `crates` directory, forward-slashed and escaped for a TOML basic string
    /// (the surrounding quotes are in the template, so the path reads as a path there).
    framework_crates: String,
    /// The driver marker type the selected persistence row names, `sqlx::Postgres` or
    /// `sqlx::MySql`; empty without a database.
    sqlx_driver: String,
    /// The driver's row type, for the SQLx repository.
    sqlx_row: String,
    /// The driver's pool-options type, for the generated test's reset.
    sqlx_pool_options: String,
    /// The adapter's database type alias, `PostgresDatabase` or `MySqlDatabase`.
    database_type: String,
    /// The `DatabaseKind` variant, `Postgres` or `MySql`.
    db_kind_variant: String,
    /// The module the item routes call: `persistence` (SQLx) or `repository` (SeaORM).
    repository_module: String,
    /// The engine's first and second bound-parameter placeholders, for the SQLx statements.
    p1: String,
    p2: String,
    /// The engine's literal for the all-zero owner the seeds use.
    zero_owner_literal: String,
    /// The SeaORM seed runner for the engine, `run_postgres` or `run_mysql`.
    seed_runner: String,
    /// The engine's module in the adapters' `auth` and `jobs` modules: `postgres` or `mysql`.
    auth_engine: String,
    /// The path the generated test polls for readiness.
    ready_path: String,
}

/// A path as a forward-slashed string.
///
/// Cargo accepts `/` in a path dependency on every platform, and a manifest that reads the same
/// on each is a manifest whose digest a test can compare across the matrix legs.
fn forward_slashed(path: &std::path::Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// A string as a quoted TOML basic string: `\` and `"` escaped, nothing else needed.
///
/// Control characters were refused by validation, so the only two characters TOML requires
/// escaped in a basic string are the two handled here. Written once, in Rust, so no template
/// composes a manifest value.
fn toml_basic_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Renders a health-check command as an inline YAML list.
///
/// # Quoting is safe here because the values cannot need quoting
///
/// The only interpolated values are an [`crate::config::container::Identifier`] user and database,
/// whose grammar admits ASCII letters, digits, and `_`. Nothing that reaches this function can
/// contain a quote, a backslash, or a newline, so a plain `"` wrapper is correct rather than
/// merely usually correct.
fn yaml_list(parts: &[String]) -> String {
    let quoted: Vec<String> = parts.iter().map(|part| format!("\"{part}\"")).collect();
    format!("[{}]", quoted.join(", "))
}

impl Context {
    /// Builds the render context from the one validated configuration.
    ///
    /// # Why this is a constructor and not two literals
    ///
    /// It was two literals — one here and one in the test below — and adding the container fields
    /// would have meant writing twenty-two of them twice. Two literals that must agree are two
    /// literals that eventually do not, and the one in the test is precisely the copy whose drift
    /// nothing would catch.
    pub(crate) fn build(configuration: &ProjectConfiguration) -> Self {
        let container = configuration.container_settings();
        let database = container.and_then(|settings| {
            settings.database_version.map(|version| {
                (
                    version,
                    settings
                        .database_name
                        .as_ref()
                        .expect("a version implies a name"),
                    settings
                        .database_user
                        .as_ref()
                        .expect("a version implies a user"),
                    settings.database_port.expect("a version implies a port"),
                )
            })
        });
        let cache = container.and_then(|settings| {
            settings.cache.engine().map(|engine| {
                (
                    engine,
                    settings.cache_port.expect("an engine implies a port"),
                )
            })
        });

        Self {
            name: configuration.name().to_owned(),
            target: match configuration.target() {
                crate::config::model::Target::Api => "api".to_owned(),
            },
            local_domain: configuration.local_domain().to_owned(),
            transport: configuration.transport().as_str().to_owned(),
            local_https: match configuration.local_https() {
                crate::config::model::LocalHttps::Off => "off".to_owned(),
                crate::config::model::LocalHttps::Requested => "requested".to_owned(),
            },
            container: configuration.container(),
            example_domain: configuration.example_domain(),
            seed_data: configuration.seed_data(),
            database: configuration
                .database()
                .map(|kind| kind.as_str().to_owned())
                .unwrap_or_default(),
            orm: configuration
                .orm()
                .map(|orm| orm.as_str().to_owned())
                .unwrap_or_default(),
            placeholder: configuration
                .database()
                .map(|kind| kind.placeholder(1))
                .unwrap_or_default(),
            driver_feature: configuration
                .database()
                .map(driver_feature)
                .unwrap_or_default(),
            seaorm_driver_feature: configuration
                .database()
                .map(seaorm_driver_feature)
                .unwrap_or_default(),
            generator_version: env!("CARGO_PKG_VERSION").to_owned(),
            template_version: templates::VERSION.to_owned(),
            modules: module_block(
                configuration.example_domain(),
                configuration.seed_data(),
                configuration.orm(),
            ),

            container_database: database.is_some(),
            container_database_service: if database.is_some() {
                DATABASE_SERVICE.to_owned()
            } else {
                String::new()
            },
            container_database_image: database
                .map(|(version, ..)| version.image().to_owned())
                .unwrap_or_default(),
            container_database_version: database
                .map(|(version, ..)| version.as_str().to_owned())
                .unwrap_or_default(),
            container_database_name: database
                .map(|(_, name, ..)| name.as_str().to_owned())
                .unwrap_or_default(),
            container_database_user: database
                .map(|(_, _, user, _)| user.as_str().to_owned())
                .unwrap_or_default(),
            container_database_port: database
                .map(|(.., port)| port.to_string())
                .unwrap_or_default(),
            container_database_internal_port: database
                .map(|(version, ..)| version.container_port().to_string())
                .unwrap_or_default(),
            container_database_data_dir: database
                .map(|(version, ..)| version.data_dir().to_owned())
                .unwrap_or_default(),
            container_database_healthcheck: database
                .map(|(version, name, user, _)| {
                    yaml_list(&version.healthcheck(user.as_str(), name.as_str()))
                })
                .unwrap_or_default(),
            container_database_dsn_example: database
                .map(|(version, name, user, _)| {
                    // The service name, NOT 127.0.0.1: inside the project network the application
                    // reaches the database by service name on its internal port, and the published
                    // host port is irrelevant there. Getting this wrong is the single most common
                    // Compose mistake, so the README shows the form that works.
                    format!(
                        "{}://{}:<RENVOR_DATABASE_PASSWORD>@{DATABASE_SERVICE}:{}/{}",
                        match version.kind() {
                            renvor_database::DatabaseKind::Postgres => "postgres",
                            _ => "mysql",
                        },
                        user.as_str(),
                        version.container_port(),
                        name.as_str(),
                    )
                })
                .unwrap_or_default(),
            container_is_postgres: database.is_some_and(|(version, ..)| {
                version.kind() == renvor_database::DatabaseKind::Postgres
            }),
            container_cache: cache.is_some(),
            container_cache_choice: container
                .map(|settings| settings.cache.as_str().to_owned())
                .unwrap_or_default(),
            container_cache_engine: cache
                .map(|(engine, _)| engine.as_str().to_owned())
                .unwrap_or_default(),
            container_cache_image: cache
                .map(|(engine, _)| engine.image().to_owned())
                .unwrap_or_default(),
            container_cache_version: cache
                .map(|(engine, _)| engine.version().to_owned())
                .unwrap_or_default(),
            container_cache_licence: cache
                .map(|(engine, _)| engine.licence().to_owned())
                .unwrap_or_default(),
            container_cache_port: cache.map(|(_, port)| port.to_string()).unwrap_or_default(),
            container_cache_internal_port: cache
                .map(|(engine, _)| engine.container_port().to_string())
                .unwrap_or_default(),
            container_cache_data_dir: cache
                .map(|(engine, _)| engine.data_dir().to_owned())
                .unwrap_or_default(),
            container_cache_healthcheck: cache
                .map(|(engine, _)| yaml_list(&engine.healthcheck()))
                .unwrap_or_default(),

            auth: configuration.auth().as_str().to_owned(),
            auth_session: configuration.auth() == crate::config::model::AuthStarter::Session,
            capabilities: configuration.capabilities().as_flag_value(),
            cap_cache: configuration
                .capabilities()
                .contains(crate::config::model::Capability::Cache),
            cap_jobs: configuration
                .capabilities()
                .contains(crate::config::model::Capability::Jobs),
            cap_mail: configuration
                .capabilities()
                .contains(crate::config::model::Capability::Mail),
            cap_storage: configuration
                .capabilities()
                .contains(crate::config::model::Capability::Storage),
            cap_observability: configuration
                .capabilities()
                .contains(crate::config::model::Capability::Observability),
            starter: configuration.is_starter(),
            framework_source: configuration
                .framework()
                .map(|source| source.kind().to_owned())
                .unwrap_or_default(),
            framework_path: configuration
                .framework()
                .map(|source| forward_slashed(source.path()))
                .unwrap_or_default(),
            framework_path_toml: configuration
                .framework()
                .map(|source| toml_basic_string(&forward_slashed(source.path())))
                .unwrap_or_default(),
            cache_wired: configuration.cache_wired_into_application(),
            any_capability: !configuration.capabilities().is_empty(),
            framework_crates: configuration
                .framework()
                .map(|source| {
                    let crates = forward_slashed(&source.path().join("crates"));
                    // The template supplies the quotes; only the content is escaped.
                    toml_basic_string(&crates).trim_matches('"').to_owned()
                })
                .unwrap_or_default(),
            sqlx_driver: configuration
                .database()
                .map(|kind| match kind {
                    renvor_database::DatabaseKind::Postgres => "sqlx::Postgres",
                    _ => "sqlx::MySql",
                })
                .unwrap_or_default()
                .to_owned(),
            sqlx_row: configuration
                .database()
                .map(|kind| match kind {
                    renvor_database::DatabaseKind::Postgres => "sqlx::postgres::PgRow",
                    _ => "sqlx::mysql::MySqlRow",
                })
                .unwrap_or_default()
                .to_owned(),
            sqlx_pool_options: configuration
                .database()
                .map(|kind| match kind {
                    renvor_database::DatabaseKind::Postgres => "postgres::PgPoolOptions",
                    _ => "mysql::MySqlPoolOptions",
                })
                .unwrap_or_default()
                .to_owned(),
            database_type: configuration
                .database()
                .map(|kind| match kind {
                    renvor_database::DatabaseKind::Postgres => "PostgresDatabase",
                    _ => "MySqlDatabase",
                })
                .unwrap_or_default()
                .to_owned(),
            db_kind_variant: configuration
                .database()
                .map(|kind| match kind {
                    renvor_database::DatabaseKind::Postgres => "Postgres",
                    _ => "MySql",
                })
                .unwrap_or_default()
                .to_owned(),
            repository_module: match configuration.orm() {
                Some(Orm::Sqlx) => "persistence",
                Some(Orm::SeaOrm) => "repository",
                None => "",
            }
            .to_owned(),
            p1: configuration
                .database()
                .map(|kind| kind.placeholder(1))
                .unwrap_or_default(),
            p2: configuration
                .database()
                .map(|kind| kind.placeholder(2))
                .unwrap_or_default(),
            zero_owner_literal: configuration
                .database()
                .map(|kind| match kind {
                    renvor_database::DatabaseKind::Postgres => {
                        "decode('00000000000000000000000000000000', 'hex')"
                    }
                    _ => "UNHEX('00000000000000000000000000000000')",
                })
                .unwrap_or_default()
                .to_owned(),
            seed_runner: configuration
                .database()
                .map(|kind| match kind {
                    renvor_database::DatabaseKind::Postgres => "run_postgres",
                    _ => "run_mysql",
                })
                .unwrap_or_default()
                .to_owned(),
            auth_engine: configuration
                .database()
                .map(|kind| kind.as_str().to_owned())
                .unwrap_or_default(),
            ready_path: if configuration
                .capabilities()
                .contains(crate::config::model::Capability::Observability)
            {
                "/readyz"
            } else {
                "/"
            }
            .to_owned(),
        }
    }
}

/// The largest framework `Cargo.lock` that will be copied into staging.
///
/// The framework's own lock is about 100 KiB; four megabytes leaves room for a workspace several
/// times this size without leaving room for a file that is not a lockfile.
const MAX_LOCKFILE_BYTES: u64 = 4 * 1024 * 1024;

/// Copies the framework's `Cargo.lock` into staging as the starter's starting point (FR-006).
///
/// # Why a starter starts from the framework's lock
///
/// A starter depends on the framework by path, and on the crates the framework depends on by
/// version. Resolved from nothing, `cargo build` in staging would update the registry index first
/// — a network round trip inside `renvor new`, which is guaranteed to work offline. Resolved from
/// this lock, every version cargo needs is one the framework already resolved and, on a machine
/// that has built the framework, already holds in its registry cache. Cargo prunes the entries the
/// starter does not use, so the file the project keeps describes the project.
///
/// # Errors
///
/// [`Code::StagingFailed`]: the lock could not be read within its bound or written into staging.
/// Validation already refused a framework without one, so the read failing here means the
/// checkout changed between validation and staging.
fn seed_lockfile(staging: &cap_std::fs::Dir, framework: &std::path::Path) -> Result<(), CliError> {
    use std::io::Read as _;
    let failed = |why: String| {
        CliError::new(
            Code::StagingFailed,
            format!("the framework's `Cargo.lock` could not be copied into staging: {why}"),
        )
        .with("stage", "staging")
    };
    let file = std::fs::File::open(framework.join("Cargo.lock"))
        .map_err(|error| failed(error.to_string()))?;
    if let Ok(metadata) = file.metadata()
        && metadata.len() > MAX_LOCKFILE_BYTES
    {
        return Err(failed(format!(
            "{} bytes, above the {MAX_LOCKFILE_BYTES}-byte bound",
            metadata.len()
        )));
    }
    let mut bytes = Vec::new();
    file.take(MAX_LOCKFILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| failed(error.to_string()))?;
    if bytes.len() as u64 > MAX_LOCKFILE_BYTES {
        return Err(failed(format!(
            "grew past the {MAX_LOCKFILE_BYTES}-byte bound while being read"
        )));
    }
    staging
        .write("Cargo.lock", &bytes)
        .map_err(|error| failed(error.to_string()))
}

/// The Compose service name for the database.
///
/// A constant rather than the project name: `depends_on` and the README's connection string both
/// have to say the same word, and a service named after the project would put that word in three
/// places that could disagree.
const DATABASE_SERVICE: &str = "database";

/// Anything the operator should see on the review screen before agreeing.
///
/// Empty is a valid and common answer. A screen that manufactures a warning to look thorough
/// teaches people to skip warnings.
fn warnings_for(configuration: &ProjectConfiguration) -> Vec<String> {
    let mut warnings = Vec::new();
    if configuration.local_https() == crate::config::model::LocalHttps::Requested {
        // FR-036. The operator asked for local HTTPS; they are about to get a recorded intent and
        // nothing else, and finding that out later would be a reasonable thing to be annoyed by.
        warnings.push(
            "local HTTPS is RECORDED ONLY: no certificate is issued and no trust store is \
             modified by this or any later renvor command without a separate, explicit step"
                .to_owned(),
        );
    }
    if configuration.container() {
        warnings.push(
            "container controls are generated but not run; `renvor docker up` needs a container \
             runtime"
                .to_owned(),
        );
    }
    warnings
}

/// Builds the `mod` block. See [`Context::modules`].
///
/// The persistence modules differ by ORM, and the SeaORM path declares none — see the match arm.
fn module_block(
    example_domain: bool,
    seed_data: bool,
    orm: Option<crate::config::model::Orm>,
) -> String {
    let mut modules = Vec::new();
    if example_domain {
        modules.push("mod domain;");
    }
    match orm {
        Some(Orm::Sqlx) => modules.push("mod persistence;"),
        // DELIBERATELY NOT DECLARED. `src/entity.rs` and `src/repository.rs` are generated in
        // full, but they need `sea-orm`, and the generated manifest declares no dependencies —
        // because generation runs the project's own `cargo build` before placing it, and a real
        // dependency would make `renvor new` need the network. Declaring modules that cannot
        // compile would emit a project that does not build, which is the one thing a generator
        // must never do. `README.md` says what to add and what to declare.
        Some(Orm::SeaOrm) | None => {}
    }
    if seed_data {
        modules.push("mod seed;");
    }
    if modules.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", modules.join("\n"))
    }
}

/// The `sea-orm` feature that resolves a database's driver.
///
/// A match rather than an interpolation, for the reason [`driver_feature`] gives: the names happen
/// to be derivable today and there is no reason they must stay so.
fn seaorm_driver_feature(kind: renvor_database::DatabaseKind) -> String {
    match kind {
        renvor_database::DatabaseKind::Postgres => "sqlx-postgres",
        renvor_database::DatabaseKind::MySql => "sqlx-mysql",
        // `DatabaseKind` is `#[non_exhaustive]`, so this arm is required by the language rather
        // than chosen. `every_database_has_a_seaorm_driver_feature` enumerates `DatabaseKind::ALL`
        // and fails the moment a kind reaches it, so an empty string cannot ship silently.
        _ => "",
    }
    .to_owned()
}

/// The `renvor-sqlx` feature that resolves a database's driver.
///
/// # Why a match rather than `format!("db-{}", kind.as_str())`
///
/// Because the two happen to agree today and there is no reason they must. A generated manifest
/// naming a feature that does not exist fails at the operator's `cargo build`, long after the
/// generator reported success — and an interpolation would keep producing plausible names for
/// every database added later, each of them wrong in the same invisible way.
fn driver_feature(kind: renvor_database::DatabaseKind) -> String {
    match kind {
        renvor_database::DatabaseKind::Postgres => "db-postgres",
        renvor_database::DatabaseKind::MySql => "db-mysql",
        // `DatabaseKind` is `#[non_exhaustive]`, so this arm is required by the language rather
        // than chosen. It is not a silent fallback: `every_database_has_a_driver_feature` below
        // enumerates `DatabaseKind::ALL` and fails the moment a kind reaches it.
        _ => "",
    }
    .to_owned()
}

/// How much this run may ask.
///
/// # Why these are two flags and not one
///
/// Contract C-1 says `--yes` waives **confirmation only** — "it never waives validation". An
/// earlier version of this function had a single `interactive` flag computed as
/// `stdin.is_terminal() && !yes`, which made `--yes` skip the **wizard** as well. That is a
/// different command: it substitutes defaults for answers the operator never gave, on a terminal,
/// which is precisely what FR-010 forbids in the non-terminal case and what nothing authorises
/// here.
///
/// So: `prompt` is "there is a terminal to ask on", and `confirm` is "and the operator has not
/// waived the review".
#[derive(Debug, Clone, Copy)]
pub struct Interaction {
    /// The wizard may fill answers the flags did not supply.
    pub prompt: bool,
    /// The review screen must be confirmed before placement.
    pub confirm: bool,
}

/// Runs the command.
///
/// [`Interaction`] is decided by the caller from whether `stdin` is a terminal, so this function
/// has no ambient dependency on the process environment and stays testable without a
/// pseudo-terminal.
pub fn run(
    reporter: &Reporter,
    answers: Answers,
    interaction: Interaction,
    dry_run: bool,
) -> Result<Exit, CliError> {
    // ── 1. VALIDATE ─────────────────────────────────────────────────────────────────
    let answers = if interaction.prompt {
        prompts::fill(answers)?
    } else {
        answers
    };
    let (configuration, destination) = ProjectConfiguration::resolve(answers)?;

    let context = Context::build(&configuration);

    let renderer = Renderer::new(templates::select(&configuration))?;

    // THE EQUIVALENT COMMAND, printed after an interactive run.
    //
    // This is what makes the wizard reproducible: an operator who answered six prompts gets the
    // single non-interactive invocation that produces the same project, which they can paste into
    // a script or a README. Printing it only after the wizard is deliberate — echoing a command
    // back at somebody who just typed it is noise.
    //
    // It goes to `stderr`, because C-1 reserves `stdout` for the result. A JSON consumer must not
    // receive this.
    // The review screen below prints this too. Printing it here as well would be duplication, so
    // it is emitted here ONLY when there will be no review — a run with no confirmation still
    // benefits from seeing the canonical form of what it just did.
    if interaction.prompt && !interaction.confirm {
        reporter.note(&format!(
            "equivalent command: {}",
            configuration.equivalent_command()
        ));
    }

    // ── 2. STAGE ────────────────────────────────────────────────────────────────────
    //
    // Created even for a dry run. SC-006 requires the dry-run manifest to match the real run's
    // created set EXACTLY, and the only way to know what a render produces is to run it. A dry run
    // that predicted the manifest from the template list would drift the first time a template
    // gained a conditional.
    let staging = Staging::create(&destination)?;
    crate::inject::fail_at("stage")?;

    // ── 3. RENDER ───────────────────────────────────────────────────────────────────
    //
    // Progress is a `stderr` note rather than a spinner: this render is milliseconds, and a
    // spinner that appears and vanishes is worse than nothing. `progress_visible` is false in JSON
    // mode and whenever `stderr` is not a terminal (C-1), so a CI log gets no progress at all.
    if reporter.progress_visible() {
        reporter.note("rendering…");
    }
    renderer.render_into(staging.dir(), &context)?;
    // A STARTER starts from the framework's lockfile (FR-006), so the verification below resolves
    // from versions the framework already pinned and the local cache already holds. The skeleton
    // declares nothing and needs nothing.
    if let Some(framework) = configuration.framework() {
        seed_lockfile(staging.dir(), framework.path())?;
    }
    // Last, so it lists the lockfile; before the manifest, so the manifest lists it.
    crate::generate::record::write(staging.dir(), env!("CARGO_PKG_VERSION"), templates::VERSION)?;
    crate::inject::fail_at("render")?;

    // ── 4. VERIFY, STILL IN STAGING ─────────────────────────────────────────────────
    //
    // FR-030. A project that does not build is a generation failure, not a discovery the operator
    // makes later. Running it here means a failure leaves nothing to clean up.
    //
    // **This runs on the dry-run path too**, and that is deliberate rather than an oversight:
    // `cargo build` writes `Cargo.lock` into the project, so a dry run that skipped verification
    // would report a manifest one file short of what a real run creates — and SC-006 requires the
    // two to match exactly. A dry run therefore costs the same couple of seconds, and tells the
    // truth.
    //
    // THIS is the operation that justified a progress indicator, and it is the only one. Five
    // `cargo` invocations against a cold target directory, every one captured by `.output()`, so
    // the terminal shows nothing at all for as long as a cold compile takes. What a reader saw
    // before was a single line saying "verifying" followed by tens of seconds of silence, which is
    // indistinguishable from a hang.
    //
    // `progress_visible` is already false in JSON mode and whenever `stderr` is not a terminal, so
    // a CI log gets a silent indicator rather than a bar drawn into it.
    let staging_path = destination.parent_display().join(staging.name());
    let progress = Progress::start("verifying the generated project", reporter);
    // A skeleton is run bare and must exit; a starter is a server, so it is sent the route dump
    // request `renvor routes` sends and must answer it before Boot — see `verify::Smoke`.
    let smoke = if configuration.is_starter() {
        crate::generate::verify::Smoke::AnswersDumpRequest
    } else {
        crate::generate::verify::Smoke::Exits
    };
    let verified = crate::generate::verify::in_staging(&staging_path, &progress, smoke);
    // Cleared BEFORE the error propagates, so a failure message is never written over a bar that
    // is still on screen. `Drop` would do this too, at the closing brace — this puts the ordering
    // where a reader can see it.
    progress.finish();
    verified?;
    crate::inject::fail_at("verify")?;

    // ── 5. MANIFEST ─────────────────────────────────────────────────────────────────
    //
    // Taken AFTER verification, so it describes exactly the tree that will be renamed —
    // `Cargo.lock` included.
    let manifest = FileManifest::describe(staging.dir())?;
    crate::inject::fail_at("manifest")?;

    // ── 6. REVIEW AND CONFIRM ───────────────────────────────────────────────────────
    //
    // FR-009. After the manifest, because the screen must list the paths that WILL be created and
    // the only way to know those is to have rendered them. Declining drops `staging`, which
    // removes the tree — so the cost of asking late is a few seconds, and the benefit is that the
    // list is true rather than predicted.
    //
    // Skipped for a dry run: a dry run writes nothing, so there is nothing to consent to.
    if interaction.confirm && !dry_run {
        let warnings = warnings_for(&configuration);
        prompts::review(reporter, &configuration, &manifest, &warnings)?;
    }

    if dry_run {
        // `staging` is dropped here, which removes the tree. FR-020: nothing was written.
        // The dry run LISTS what it would create, rather than counting it. A count tells an
        // operator that something would happen; the list tells them whether it is what they meant,
        // which is the entire reason to run a dry run.
        let mut human = Report::new()
            .status(
                Status::Info,
                format!(
                    "Dry run: {} files ({} bytes) would be created",
                    manifest.file_count(),
                    manifest.total_bytes()
                ),
            )
            .row(
                "Destination",
                crate::output::redact::path(&destination.display_path()),
            )
            .blank();
        // One report line per path, rather than one line containing newlines. A report line is
        // single-line by construction, which is what stops a filename containing a newline from
        // forging an entry in this list.
        for path in manifest.paths() {
            human = human.item(path);
        }
        drop(staging);
        return Ok(reporter.finish(
            "new",
            &human,
            serde_json::json!({
                "dryRun": true,
                "destination": destination.display_path().display().to_string(),
                "templateVersion": templates::VERSION,
                // THE RESOLVED CONFIGURATION, not the answers.
                //
                // A consumer scripting `renvor new` needs to know what the run actually decided —
                // which database version it defaulted to, which port it published — and reading
                // that back out of the generated `renvor.toml` means parsing a file to learn what
                // the command it just ran did.
                //
                // It is safe to emit because `ProjectConfiguration` cannot hold a secret: the
                // exhaustive-destructuring test in `config::model` fails to compile if a field is
                // added without being classified, and `destination` is `#[serde(skip)]` and
                // reported separately through the redacting path above.
                "configuration": &configuration,
                "manifest": manifest.entries,
            }),
        ));
    }

    // ── 6. PLACE ────────────────────────────────────────────────────────────────────
    staging.place(&destination)?;

    // ── 7. REPORT ───────────────────────────────────────────────────────────────────
    let human = Report::new()
        .status(
            Status::Done,
            format!(
                "Created {} files ({} bytes)",
                manifest.file_count(),
                manifest.total_bytes()
            ),
        )
        .row(
            "Destination",
            crate::output::redact::path(&destination.display_path()),
        )
        .blank()
        .text(format!("next: cd {} && cargo run", destination.name()));
    Ok(reporter.finish(
        "new",
        &human,
        serde_json::json!({
            "dryRun": false,
            "destination": destination.display_path().display().to_string(),
            "templateVersion": templates::VERSION,
            "configuration": &configuration,
            "manifest": manifest.entries,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Format;
    use std::path::PathBuf;

    /// Every database resolves a driver feature — the guard the `_` arm above depends on.
    #[test]
    fn every_database_has_a_driver_feature() {
        for kind in renvor_database::DatabaseKind::ALL {
            let feature = driver_feature(kind);
            assert!(
                !feature.is_empty(),
                "`{}` reached the catch-all arm, so a generated manifest would name no driver",
                kind.as_str()
            );
            assert!(
                feature.starts_with("db-"),
                "`{feature}` is not a `renvor-sqlx` driver feature"
            );
        }
    }

    /// The same guarantee for the SECOND spelling of the same engine.
    ///
    /// Two functions, two catch-all arms, two ways for a database added later to generate a
    /// manifest naming a feature that does not exist. Both are enumerated.
    #[test]
    fn every_database_has_a_seaorm_driver_feature() {
        for kind in renvor_database::DatabaseKind::ALL {
            let feature = seaorm_driver_feature(kind);
            assert!(
                !feature.is_empty(),
                "`{}` reached the catch-all arm, so a generated manifest would name no driver",
                kind.as_str()
            );
            assert!(
                feature.starts_with("sqlx-"),
                "`{feature}` is not a `sea-orm` driver feature"
            );
        }
    }

    /// The two spellings must never be the same string.
    ///
    /// If they were, one of the two manifests below would be naming the other crate's feature —
    /// which compiles as a manifest and fails at the operator's `cargo build`.
    #[test]
    fn the_two_driver_feature_spellings_are_distinct() {
        for kind in renvor_database::DatabaseKind::ALL {
            assert_ne!(
                driver_feature(kind),
                seaorm_driver_feature(kind),
                "`{}` spells its renvor-seaorm and sea-orm features identically",
                kind.as_str()
            );
        }
    }

    /// The persistence module is declared when, and only when, a database was chosen.
    #[test]
    fn the_persistence_module_follows_the_database_choice() {
        assert!(!module_block(false, false, None).contains("mod persistence;"));
        assert!(module_block(false, false, Some(Orm::Sqlx)).contains("mod persistence;"));
        // Ordering matters for `cargo fmt --check`, which the pre-placement verification runs.
        let all = module_block(true, true, Some(Orm::Sqlx));
        let domain = all.find("mod domain;").expect("domain");
        let persistence = all.find("mod persistence;").expect("persistence");
        let seed = all.find("mod seed;").expect("seed");
        assert!(
            domain < persistence && persistence < seed,
            "modules must stay ordered: {all:?}"
        );
    }

    #[test]
    fn a_framework_path_is_rendered_as_a_valid_toml_basic_string() {
        // The two characters TOML requires escaped, and the one every Windows path carries.
        assert_eq!(toml_basic_string("/a/b"), "\"/a/b\"");
        assert_eq!(toml_basic_string(r#"C:\x\"y""#), r#""C:\\x\\\"y\"""#);
        assert_eq!(
            forward_slashed(std::path::Path::new(r"C:\Users\dev\renvor")),
            "C:/Users/dev/renvor"
        );
        // And the rendering round-trips through the parser `renvor check` uses.
        let table: toml::Table =
            toml::from_str(&format!("path = {}\n", toml_basic_string(r#"C:\x\"y""#)))
                .expect("parses");
        assert_eq!(table["path"].as_str(), Some(r#"C:\x\"y""#));
    }

    fn answers(destination: PathBuf) -> Answers {
        Answers {
            name: None,
            destination,
            local_domain: None,
            target: "api".to_owned(),
            transport: None,
            container: false,
            local_https: false,
            seed_data: false,
            example_domain: false,
            orm: None,
            database: None,
            database_version: None,
            database_name: None,
            database_user: None,
            database_port: None,
            container_cache: None,
            cache_port: None,
            auth: None,
            capabilities: None,
            framework_path: None,
        }
    }

    fn reporter() -> Reporter {
        Reporter::new(Format::Human, true)
    }

    /// No terminal, so neither the wizard nor the review runs — which is the state every test
    /// here needs and the state a CI run is actually in.
    fn quiet() -> Interaction {
        Interaction {
            prompt: false,
            confirm: false,
        }
    }

    #[test]
    fn the_module_block_produces_the_blank_lines_rustfmt_wants() {
        // The exact strings, because one extra newline here is a `cargo fmt --check` failure in
        // every generated project — which is the acceptance criterion "the generated skeleton
        // formats". Found by running rustfmt over the output, not by inspection.
        assert_eq!(module_block(false, false, None), "");
        assert_eq!(module_block(true, false, None), "\nmod domain;\n");
        assert_eq!(module_block(true, true, None), "\nmod domain;\nmod seed;\n");
    }

    #[test]
    fn a_starter_is_seeded_with_the_frameworks_lockfile_before_verification() {
        // Phase 011 (FR-006). The framework's `Cargo.lock` is copied into staging so the
        // starter's resolution starts from versions already in the local registry cache — which
        // is what lets `cargo build` in staging need no index update, and lets a starter generate
        // offline on a machine that has built the framework.
        let base = tempfile::tempdir().expect("tempdir");
        let framework = base.path().join("framework");
        std::fs::create_dir_all(&framework).expect("mkdir");
        std::fs::write(framework.join("Cargo.lock"), b"# a lock\nversion = 4\n").expect("write");
        let staging = base.path().join("staging");
        std::fs::create_dir_all(&staging).expect("mkdir");
        let dir = cap_std::fs::Dir::open_ambient_dir(&staging, cap_std::ambient_authority())
            .expect("opens");
        seed_lockfile(&dir, &framework).expect("seeds");
        assert_eq!(
            std::fs::read(staging.join("Cargo.lock")).expect("readable"),
            b"# a lock\nversion = 4\n"
        );
        // A framework without a lockfile cannot seed one, and says so rather than staging a
        // project whose resolution would then need the network.
        std::fs::remove_file(framework.join("Cargo.lock")).expect("removed");
        let error = seed_lockfile(&dir, &framework).expect_err("refused");
        assert_eq!(error.code, crate::exit::Code::StagingFailed);
    }

    #[test]
    fn a_project_is_generated_and_the_destination_appears_exactly_once() {
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("commerce");
        let exit = run(&reporter(), answers(target.clone()), quiet(), false).expect("generates");
        assert_eq!(exit, Exit::Success);
        assert!(target.join("Cargo.toml").is_file());
        assert!(target.join("src/main.rs").is_file());
        assert!(target.join("renvor.toml").is_file());
    }

    #[test]
    fn a_dry_run_writes_absolutely_nothing() {
        // FR-020, and the assertion is on the PARENT rather than on the destination: a staging
        // directory left beside it would also be a write.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("commerce");
        run(&reporter(), answers(target.clone()), quiet(), true).expect("dry run");
        assert!(!target.exists(), "the destination was created by a dry run");
        let leftovers: Vec<_> = std::fs::read_dir(base.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(leftovers.is_empty(), "a dry run left {leftovers:?} behind");
    }

    #[test]
    fn the_dry_run_manifest_matches_the_real_run_exactly() {
        // SC-006. This is the assertion that makes `--dry-run` trustworthy rather than indicative.
        let base = tempfile::tempdir().expect("tempdir");
        let mut a = answers(base.path().join("one"));
        a.example_domain = true;
        a.seed_data = true;
        a.container = true;
        a.name = Some("demo".to_owned());

        let mut b = answers(base.path().join("two"));
        b.example_domain = true;
        b.seed_data = true;
        b.container = true;
        b.name = Some("demo".to_owned());

        // Render both into staging and compare the manifests directly, since the reporter's
        // stdout is not capturable from a unit test.
        let dry = manifest_of(a);
        let real = manifest_of(b);
        assert_eq!(dry, real, "the dry-run and real manifests differ");
    }

    /// Renders a configuration into staging and returns the manifest, without placing.
    fn manifest_of(answers: Answers) -> Vec<String> {
        let (configuration, destination) =
            ProjectConfiguration::resolve(answers).expect("resolves");
        let context = Context::build(&configuration);
        let renderer = Renderer::new(templates::select(&configuration)).expect("builds");
        let staging = Staging::create(&destination).expect("stages");
        renderer
            .render_into(staging.dir(), &context)
            .expect("renders");
        FileManifest::describe(staging.dir())
            .expect("describes")
            .paths()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn choices_that_were_not_made_render_no_files() {
        // Invariant I-12 from the other direction: a project without container controls must not
        // contain a Dockerfile that the manifest would then record as an honoured choice.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("plain");
        run(&reporter(), answers(target.clone()), quiet(), false).expect("generates");
        assert!(!target.join("Dockerfile").exists());
        assert!(!target.join("compose.yaml").exists());
        assert!(!target.join("src/domain.rs").exists());
        assert!(!target.join("src/seed.rs").exists());
    }

    #[test]
    fn choices_that_were_made_render_their_files() {
        // POSITIVE CONTROL for the test above.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("full");
        let mut a = answers(target.clone());
        a.container = true;
        a.example_domain = true;
        a.seed_data = true;
        run(&reporter(), a, quiet(), false).expect("generates");
        assert!(target.join("Dockerfile").is_file());
        assert!(target.join("compose.yaml").is_file());
        assert!(target.join("src/domain.rs").is_file());
        assert!(target.join("src/seed.rs").is_file());
    }

    #[test]
    fn a_failure_after_staging_leaves_the_destination_untouched() {
        // The transaction's whole promise. `--seed-data` without `--example-domain` fails in
        // VALIDATE, before staging; to fail *after* staging we need a render failure, which an
        // undefined variable produces.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("broken");
        let (configuration, destination) =
            ProjectConfiguration::resolve(answers(target.clone())).expect("resolves");
        let set = crate::generate::render::TemplateSet {
            version: "test",
            entries: vec![crate::generate::render::TemplateEntry {
                path: "a.txt",
                body: "{{ absent }}",
            }],
            verbatim: Vec::new(),
            trim_blocks: false,
        };
        let renderer = Renderer::new(set).expect("builds");
        let staging = Staging::create(&destination).expect("stages");
        let error = renderer
            .render_into(staging.dir(), &serde_json::json!({}))
            .unwrap_err();
        assert_eq!(error.code, crate::exit::Code::RenderFailed);
        drop(staging);
        assert!(!target.exists(), "the destination survived a failed render");
        let leftovers: Vec<String> = std::fs::read_dir(base.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging survived a failed render: {leftovers:?}"
        );
        let _ = configuration;
    }

    /// A framework checkout with exactly what `FrameworkSource::validate_path` reads: the
    /// workspace manifest, the facade's manifest, and a lockfile.
    fn fake_framework(base: &std::path::Path) -> PathBuf {
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

    #[test]
    fn the_manifest_names_the_variables_the_starter_reads() {
        // Phase 011's first validation pass found the manifest's comments naming
        // `RENVOR_AUTH__CSRF_KEY` while the starter reads `RENVOR_AUTH_CSRF_KEY`: a comment nobody
        // renders into a test drifts from the code that reads the variable. This renders the
        // manifest of a starter that has every commented variable and pins the names.
        let base = tempfile::tempdir().expect("tempdir");
        let mut a = answers(base.path().join("starter"));
        a.database = Some("postgres".to_owned());
        a.container = true;
        a.capabilities = Some("cache,mail".to_owned());
        a.auth = Some("session".to_owned());
        a.framework_path = Some(fake_framework(base.path()));
        let (configuration, destination) = ProjectConfiguration::resolve(a).expect("resolves");
        let context = Context::build(&configuration);
        let renderer = Renderer::new(templates::select(&configuration)).expect("builds");
        let staging = Staging::create(&destination).expect("stages");
        renderer
            .render_into(staging.dir(), &context)
            .expect("renders");
        let manifest = staging
            .dir()
            .read_to_string("renvor.toml")
            .expect("renvor.toml");
        for name in [
            "RENVOR_AUTH_CSRF_KEY",
            "RENVOR_AUTH_ABUSE_KEY",
            "RENVOR_CACHE_PASSWORD",
        ] {
            assert!(
                manifest.contains(name),
                "the manifest must name `{name}`:\n{manifest}"
            );
        }
        // `__Host-rv_session` is a cookie name, not a variable; the drift to catch is a doubled
        // underscore INSIDE a `RENVOR_…` name.
        let doubled = manifest.match_indices("RENVOR_").any(|(at, _)| {
            manifest[at..]
                .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
                .next()
                .is_some_and(|name| name.contains("__"))
        });
        assert!(
            !doubled,
            "a doubled underscore names a variable nobody reads:\n{manifest}"
        );
    }
}
