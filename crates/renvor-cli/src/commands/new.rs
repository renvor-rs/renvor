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

use crate::config::model::{Answers, ProjectConfiguration};
use crate::config::prompts;
use crate::exit::{CliError, Exit};
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
struct Context {
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
    fn build(configuration: &ProjectConfiguration) -> Self {
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
            generator_version: env!("CARGO_PKG_VERSION").to_owned(),
            template_version: templates::VERSION.to_owned(),
            modules: module_block(
                configuration.example_domain(),
                configuration.seed_data(),
                configuration.database().is_some(),
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
        }
    }
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
fn module_block(example_domain: bool, seed_data: bool, persistence: bool) -> String {
    let mut modules = Vec::new();
    if example_domain {
        modules.push("mod domain;");
    }
    if persistence {
        modules.push("mod persistence;");
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
    let verified = crate::generate::verify::in_staging(&staging_path, &progress);
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

    /// The persistence module is declared when, and only when, a database was chosen.
    #[test]
    fn the_persistence_module_follows_the_database_choice() {
        assert!(!module_block(false, false, false).contains("mod persistence;"));
        assert!(module_block(false, false, true).contains("mod persistence;"));
        // Ordering matters for `cargo fmt --check`, which the pre-placement verification runs.
        let all = module_block(true, true, true);
        let domain = all.find("mod domain;").expect("domain");
        let persistence = all.find("mod persistence;").expect("persistence");
        let seed = all.find("mod seed;").expect("seed");
        assert!(
            domain < persistence && persistence < seed,
            "modules must stay ordered: {all:?}"
        );
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
        assert_eq!(module_block(false, false, false), "");
        assert_eq!(module_block(true, false, false), "\nmod domain;\n");
        assert_eq!(
            module_block(true, true, false),
            "\nmod domain;\nmod seed;\n"
        );
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
}
