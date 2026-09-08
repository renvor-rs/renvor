//! The embedded template catalogue (FR-024, FR-025, contract C-4).
//!
//! # Embedded, not fetched
//!
//! Every byte below is compiled into the executable. There is no local archive path and no remote
//! one, which is what makes FR-040's structural assertion — the binary carries no archive-extraction
//! capability — true rather than aspirational, and what makes FR-043's offline guarantee hold
//! without a network stub.
//!
//! # Versioning
//!
//! [`VERSION`] is written into every generated `renvor.toml`. Two runs of the same generator
//! version, template version, and configuration produce byte-identical trees (SC-016), so this
//! string is the thing that has to change when a template does.

use crate::config::model::{Capability, ProjectConfiguration};
use crate::generate::render::{TemplateEntry, TemplateSet, VerbatimEntry};

/// The template-set version recorded in every generated project.
///
/// Bumped whenever any body below changes. It is **not** the crate version: a release that changes
/// no template must not claim to have produced a different tree.
///
/// **`7` → `8` (Phase 012, L-2).** Every tree — skeleton and starter — gains `rust-toolchain.toml`
/// (the `TOOLCHAIN` group: an exact release, `components = ["rustfmt", "clippy"]`,
/// `profile = "minimal"`) and a `rust-version` line in `Cargo.toml` (the MSRV, not the pin). A
/// starter's pin is the framework checkout's `[toolchain].channel` and its MSRV the checkout's
/// `[workspace.package].rust-version`, each read on its own at validation; a skeleton's are both
/// the generator's `CARGO_PKG_RUST_VERSION` (D-L2-4). Both READMEs gain a toolchain section
/// (FR-012-3); the Dockerfile's builder tag is derived from the pin and its builder stage sets
/// `ENV RUSTUP_AUTO_INSTALL=0` (FR-012-9). The provenance record — written by `generate::record`,
/// not by a template — moves to `record_version = 2` with `[toolchain]` and `[verified_with]`.
/// The group is gated on the record at `generate auth` (FR-012-10b): a legacy tree is re-rendered
/// through [`select_with_toolchain`] with the group off, so nothing is inserted silently. A
/// project rendered at 8 is **not readable by a generator built before this revision**
/// (FR-012-5c): rebuild the generator, not the project.
///
/// **`6` → `7` (Phase 011).** `renvor.toml` records the auth starter (`auth = …`) and a
/// `[capabilities]` table on every project, a `[framework]` table and — with `session` — an
/// `[auth]` table on a starter, and `cache_wired_into_application` follows the `cache` capability
/// rather than being the constant `false` it was through Phase 010. A project given
/// `--framework-path` is a **starter**: a real application with path dependencies, whose tree the
/// `STARTER_*` groups below render. The skeleton's other files are unchanged, which
/// `the_skeleton_is_unchanged_apart_from_its_recorded_version_and_two_keys` asserts against the
/// template-version-6 fixture.
///
/// **`5` → `6` (post-Phase-007 correction).** No behaviour changed and no file was added or
/// removed. Two generated bodies did: `renvor.toml`'s `[persistence]` comment named
/// `src/persistence.rs` on **both** paths, and `README.md`'s persistence section documented the
/// direct-SQLx module and `renvor-sqlx` to a reader whose project contains neither. Both are now
/// selected by ORM. A generated body changed, so the version does — that is the whole contract this
/// constant carries, and exempting a comment from it would make the version mean "changed, unless
/// we judged it cosmetic".
///
/// **`4` → `5` (Phase 007).** `--orm seaorm` is accepted, and it generates a different tree:
/// `src/entity.rs` and `src/repository.rs` replace `src/persistence.rs`. `Cargo.toml` declares
/// **nothing** on either path — see `PERSISTENCE_SEAORM` below for why a real `sea-orm` dependency
/// was designed and then rejected. `--orm sqlx` is unchanged and still produces exactly the
/// version-4 tree apart from this recorded version, which is what FR-043 requires and what
/// `the_direct_sqlx_tree_is_unchanged_apart_from_its_recorded_version` asserts.
///
/// **`3` → `4` (Phase 006, container scope addition).** `--container` now generates a complete
/// local Compose profile rather than two near-empty files: the selected database service with a
/// pinned image, a named volume, a verified health check, and a localhost-only published port; an
/// optional cache service; `.dockerignore`; `.env.example`; a `.gitignore` that excludes `.env`;
/// and a `[container]` section in `renvor.toml`. The generated tree and the manifest shape both
/// change, which is exactly what this constant exists to record.
///
/// **`2` → `3` (Phase 006).** A project generated with `--database` gains `src/persistence.rs` and
/// a reversible `migrations/0001_create_item` pair, and `renvor.toml` gains a `[persistence]`
/// section. `Cargo.toml` is deliberately **still** dependency-free: no Renvor crate is published,
/// so declaring one would emit a project that cannot build.
///
/// **`1` → `2` (Phase 004).** `renvor.toml` gained `transport`, and `README.md` gained the section
/// describing the dependency to add once the framework crates are published. `Cargo.toml` is
/// deliberately **unchanged**: a generated project still declares no dependency, and still builds.
pub const VERSION: &str = "8";

/// Entries every project gets.
const BASE: &[TemplateEntry] = &[
    TemplateEntry {
        path: "Cargo.toml",
        body: include_str!("../templates/Cargo.toml.j2"),
    },
    TemplateEntry {
        path: "renvor.toml",
        body: include_str!("../templates/renvor.toml.j2"),
    },
    TemplateEntry {
        path: "src/main.rs",
        body: include_str!("../templates/src_main.rs.j2"),
    },
    TemplateEntry {
        path: "README.md",
        body: include_str!("../templates/README.md.j2"),
    },
    TemplateEntry {
        path: ".gitignore",
        body: include_str!("../templates/gitignore.j2"),
    },
];

/// Added by `--example-domain`.
const EXAMPLE_DOMAIN: &[TemplateEntry] = &[TemplateEntry {
    path: "src/domain.rs",
    body: include_str!("../templates/src_domain.rs.j2"),
}];

/// Added by `--seed-data`. Requires the example domain; the configuration validator enforces that,
/// so this set is never selected alone.
const SEED_DATA: &[TemplateEntry] = &[TemplateEntry {
    path: "src/seed.rs",
    body: include_str!("../templates/src_seed.rs.j2"),
}];

/// The migrations, which are the same whichever ORM was selected.
///
/// # One migration history, not two
///
/// Renvor runs SQL-file migrations through SQLx's engine for **both** persistence models, so a
/// project that switches ORM keeps its schema and its `_sqlx_migrations` bookkeeping. The
/// alternative — `sea-orm-migration` for the SeaORM path — would give that project a second
/// history table with no checksum column, and the two would disagree the first time anyone edited
/// an applied migration. See ADR-0022.
///
/// Both halves ship together: the pair is what makes the migration REVERSIBLE, and a
/// declared-reversible migration missing its `.down.sql` is refused at rollback rather than
/// discovered half-way through one.
const MIGRATIONS: &[TemplateEntry] = &[
    TemplateEntry {
        path: "migrations/0001_create_item.up.sql",
        body: include_str!("../templates/migrations_up.sql.j2"),
    },
    TemplateEntry {
        path: "migrations/0001_create_item.down.sql",
        body: include_str!("../templates/migrations_down.sql.j2"),
    },
];

/// Added by `--orm sqlx`.
///
/// Statements and an allowlist, with no dependency: the crate this would need is `renvor-sqlx`,
/// which is not published, so a module that named it would emit a project that does not resolve.
const PERSISTENCE_SQLX: &[TemplateEntry] = &[TemplateEntry {
    path: "src/persistence.rs",
    body: include_str!("../templates/src_persistence.rs.j2"),
}];

/// Added by `--orm seaorm`.
///
/// # Emitted, and NOT compiled by the project that receives them
///
/// `src/main.rs` declares neither module and `Cargo.toml` declares no dependency, so `cargo build`
/// in a generated project builds neither file. That is the shipped behaviour, and stating it here
/// is the point: the earlier wording claimed these two "compile", which described a design that was
/// **considered and rejected** rather than the one that ships.
///
/// It was rejected because generation runs the staged project's own `cargo fmt`, `clippy`, `build`,
/// `test` and `run` before placing it. A real `sea-orm` dependency therefore puts a registry fetch
/// and a multi-minute compile inside `renvor new`, and Renvor guarantees offline generation —
/// pinned by `seaorm_generation_succeeds_offline` with `CARGO_NET_OFFLINE=true`.
///
/// # The evidence that they are code rather than decoration
///
/// They were compiled successfully against real `sea-orm 2.0.2` during Phase 007 verification, with
/// the dependency and the two `mod` declarations added by hand. That is a **manual result recorded
/// as evidence**, not something the generator does or a gate that runs in CI; L-13 in
/// `governance/phase-007-evidence.md` states it in those terms. What runs automatically is
/// `the_uncompiled_seaorm_sources_are_still_rustfmt_clean`, because the generator's own formatting
/// gate cannot see a file no module declares.
const PERSISTENCE_SEAORM: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/entity.rs",
        body: include_str!("../templates/src_entity.rs.j2"),
    },
    TemplateEntry {
        path: "src/repository.rs",
        body: include_str!("../templates/src_repository.rs.j2"),
    },
];

/// Added by `--container`.
///
/// `.env.example` ships and `.env` does not. Generation writes an example with empty placeholders
/// and never a working credential — see `templates/env_example.j2` for why that is a decision
/// rather than an omission.
const CONTAINER: &[TemplateEntry] = &[
    TemplateEntry {
        path: "Dockerfile",
        body: include_str!("../templates/Dockerfile.j2"),
    },
    TemplateEntry {
        path: "compose.yaml",
        body: include_str!("../templates/compose.yaml.j2"),
    },
    TemplateEntry {
        // A build context is uploaded whole and every image layer can read it, so an image built
        // with `.env` present carries the database password — permanently, because layers are
        // additive and a later `rm` does not remove it.
        path: ".dockerignore",
        body: include_str!("../templates/dockerignore.j2"),
    },
    TemplateEntry {
        path: ".env.example",
        body: include_str!("../templates/env_example.j2"),
    },
];

/// The toolchain declaration (Phase 012, FR-012-1): rendered for **both** tree kinds — at
/// `renvor new` always, at `generate auth` only when the record's `[toolchain]` declares a pin
/// (FR-012-10b) — which is why it is a group of its own rather than a line of `BASE`.
///
/// The declaration's other half, the `rust-version` line, lives inside both `Cargo.toml.j2`
/// under `{% if toolchain_declared %}`: a line cannot be an entry, so the same switch that adds
/// this group to a selection also turns that line on in the context.
const TOOLCHAIN: &[TemplateEntry] = &[TemplateEntry {
    path: "rust-toolchain.toml",
    body: include_str!("../templates/rust_toolchain.toml.j2"),
}];

/// The starter's files (Phase 011): a framework-backed application replacing the skeleton's
/// `Cargo.toml`, `src/main.rs`, `README.md`, and `.gitignore`, and adding the modules the
/// selection needs. Selected only when a framework path was given.
const STARTER_BASE: &[TemplateEntry] = &[
    TemplateEntry {
        path: "Cargo.toml",
        body: include_str!("../templates/starter/Cargo.toml.j2"),
    },
    TemplateEntry {
        path: "renvor.toml",
        body: include_str!("../templates/renvor.toml.j2"),
    },
    TemplateEntry {
        path: "src/main.rs",
        body: include_str!("../templates/starter/src_main.rs.j2"),
    },
    TemplateEntry {
        path: "src/app.rs",
        body: include_str!("../templates/starter/src_app.rs.j2"),
    },
    TemplateEntry {
        path: "src/config.rs",
        body: include_str!("../templates/starter/src_config.rs.j2"),
    },
    TemplateEntry {
        path: "src/routes.rs",
        body: include_str!("../templates/starter/src_routes.rs.j2"),
    },
    TemplateEntry {
        path: "config/http.toml",
        body: include_str!("../templates/starter/config_http.toml.j2"),
    },
    TemplateEntry {
        path: "README.md",
        body: include_str!("../templates/starter/README.md.j2"),
    },
    TemplateEntry {
        path: ".gitignore",
        body: include_str!("../templates/starter/gitignore.j2"),
    },
    TemplateEntry {
        path: ".env.example",
        body: include_str!("../templates/starter/env_example.j2"),
    },
    TemplateEntry {
        path: "tests/starter.rs",
        body: include_str!("../templates/starter/tests_starter.rs.j2"),
    },
    TemplateEntry {
        path: "tests/support/mod.rs",
        body: include_str!("../templates/starter/tests_support_mod.rs.j2"),
    },
];

/// The starter's example domain: the item type, its handlers, and — with `--seed-data` — seeds.
const STARTER_EXAMPLE_DOMAIN: &[TemplateEntry] = &[TemplateEntry {
    path: "src/domain.rs",
    body: include_str!("../templates/starter/src_domain.rs.j2"),
}];

const STARTER_SEED_DATA: &[TemplateEntry] = &[TemplateEntry {
    path: "src/seed.rs",
    body: include_str!("../templates/starter/src_seed.rs.j2"),
}];

/// The starter's item migration, which gains an owner column with the auth starter.
/// With a database: the migration directory exists even when no set is copied into it, because
/// the provider loads it at Boot and an absent directory is a Boot failure.
const STARTER_DATABASE: &[TemplateEntry] = &[
    TemplateEntry {
        path: "migrations/README.md",
        body: include_str!("../templates/starter/migrations_README.md.j2"),
    },
    TemplateEntry {
        path: "src/resources/mod.rs",
        body: include_str!("../templates/starter/src_resources_mod.rs.j2"),
    },
];

const STARTER_MIGRATIONS: &[TemplateEntry] = &[
    TemplateEntry {
        path: "migrations/0001_create_item.up.sql",
        body: include_str!("../templates/starter/migrations_item_up.sql.j2"),
    },
    TemplateEntry {
        path: "migrations/0001_create_item.down.sql",
        body: include_str!("../templates/starter/migrations_item_down.sql.j2"),
    },
];

const STARTER_PERSISTENCE_SQLX: &[TemplateEntry] = &[TemplateEntry {
    path: "src/persistence.rs",
    body: include_str!("../templates/starter/src_persistence.rs.j2"),
}];

const STARTER_PERSISTENCE_SEAORM: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/entity.rs",
        body: include_str!("../templates/starter/src_entity.rs.j2"),
    },
    TemplateEntry {
        path: "src/repository.rs",
        body: include_str!("../templates/starter/src_repository.rs.j2"),
    },
];

/// The session auth starter (W-023).
const STARTER_AUTH: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/auth.rs",
        body: include_str!("../templates/starter/src_auth.rs.j2"),
    },
    TemplateEntry {
        path: "config/auth.toml",
        body: include_str!("../templates/starter/config_auth.toml.j2"),
    },
];

/// The capabilities module root, present when any capability is selected.
const STARTER_CAPABILITIES: &[TemplateEntry] = &[TemplateEntry {
    path: "src/capabilities/mod.rs",
    body: include_str!("../templates/starter/src_capabilities_mod.rs.j2"),
}];

const STARTER_CACHE: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/capabilities/cache.rs",
        body: include_str!("../templates/starter/src_capabilities_cache.rs.j2"),
    },
    TemplateEntry {
        path: "config/cache.toml",
        body: include_str!("../templates/starter/config_cache.toml.j2"),
    },
];

const STARTER_JOBS: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/capabilities/jobs.rs",
        body: include_str!("../templates/starter/src_capabilities_jobs.rs.j2"),
    },
    TemplateEntry {
        path: "config/jobs.toml",
        body: include_str!("../templates/starter/config_jobs.toml.j2"),
    },
];

const STARTER_MAIL: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/capabilities/mail.rs",
        body: include_str!("../templates/starter/src_capabilities_mail.rs.j2"),
    },
    TemplateEntry {
        path: "config/mail.toml",
        body: include_str!("../templates/starter/config_mail.toml.j2"),
    },
];

const STARTER_STORAGE: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/capabilities/storage.rs",
        body: include_str!("../templates/starter/src_capabilities_storage.rs.j2"),
    },
    TemplateEntry {
        path: "config/storage.toml",
        body: include_str!("../templates/starter/config_storage.toml.j2"),
    },
];

const STARTER_OBSERVABILITY: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/capabilities/observability.rs",
        body: include_str!("../templates/starter/src_capabilities_observability.rs.j2"),
    },
    TemplateEntry {
        path: "config/otlp.toml.example",
        body: include_str!("../templates/starter/config_otlp.toml.example.j2"),
    },
];

/// The framework's migration sets a starter copies, byte for byte, beside its own.
///
/// One directory, one ledger: the item migration is `0001`, the auth set `20260901…`, the jobs set
/// `20260904…`, applied in version order by the one `Migrations::load`.
fn verbatim_migrations(configuration: &ProjectConfiguration) -> Vec<VerbatimEntry> {
    let Some(database) = configuration.database() else {
        return Vec::new();
    };
    let mut files = Vec::new();
    let mut copy = |set: Option<&'static renvor_auth::migrations::EngineSet>| {
        if let Some(set) = set {
            for file in set.files() {
                files.push(VerbatimEntry {
                    path: format!("migrations/{}", file.name()),
                    body: file.contents(),
                });
            }
        }
    };
    if configuration.auth() == crate::config::model::AuthStarter::Session {
        copy(renvor_auth::migrations::for_engine(database.as_str()));
    }
    if configuration.capabilities().contains(Capability::Jobs)
        && let Some(set) = renvor_jobs::migrations::for_engine(database.as_str())
    {
        for file in set.files() {
            files.push(VerbatimEntry {
                path: format!("migrations/{}", file.name()),
                body: file.contents(),
            });
        }
    }
    files
}

/// Every entry that can ship, for the catalogue-wide validation test.
///
/// This exists so the load-time guarantee covers the **whole** binary rather than whichever subset
/// a particular run selected. A malformed entry in a rarely-selected group would otherwise reach a
/// user before it reached a test.
///
/// `#[cfg(test)]` because nothing at runtime renders the union — a run renders a selection. Shipping
/// it would be an unreachable code path claiming to be a capability.
#[cfg(test)]
fn catalogue() -> Vec<TemplateEntry> {
    let mut all = Vec::new();
    all.extend_from_slice(BASE);
    all.extend_from_slice(EXAMPLE_DOMAIN);
    all.extend_from_slice(SEED_DATA);
    all.extend_from_slice(MIGRATIONS);
    all.extend_from_slice(PERSISTENCE_SQLX);
    all.extend_from_slice(PERSISTENCE_SEAORM);
    all.extend_from_slice(CONTAINER);
    all.extend_from_slice(TOOLCHAIN);
    all
}

/// Every starter entry that can ship, for the same load-time guarantee.
///
/// Separate from [`catalogue`] because the two share output paths on purpose (`Cargo.toml`,
/// `src/main.rs`, …): the starter REPLACES those skeleton files, so a duplicate-path check over
/// the union would fail for the right reason and prove the wrong thing.
#[cfg(test)]
fn starter_catalogue() -> Vec<TemplateEntry> {
    let mut all = Vec::new();
    all.extend_from_slice(STARTER_BASE);
    all.extend_from_slice(STARTER_EXAMPLE_DOMAIN);
    all.extend_from_slice(STARTER_SEED_DATA);
    all.extend_from_slice(STARTER_MIGRATIONS);
    all.extend_from_slice(STARTER_PERSISTENCE_SQLX);
    all.extend_from_slice(STARTER_PERSISTENCE_SEAORM);
    all.extend_from_slice(STARTER_AUTH);
    all.extend_from_slice(STARTER_CAPABILITIES);
    all.extend_from_slice(STARTER_CACHE);
    all.extend_from_slice(STARTER_JOBS);
    all.extend_from_slice(STARTER_MAIL);
    all.extend_from_slice(STARTER_STORAGE);
    all.extend_from_slice(STARTER_OBSERVABILITY);
    all.extend(container_for_starter());
    all.extend_from_slice(TOOLCHAIN);
    all
}

/// Chooses the entries a configuration actually renders.
///
/// Selection is by **honoured choice**, which is what makes data-model invariant I-12 hold: the
/// manifest records a selection only if generation acted on it, and generation acts on a selection
/// only by adding entries here.
#[must_use]
pub fn select(configuration: &ProjectConfiguration) -> TemplateSet {
    select_with_toolchain(configuration, true)
}

/// [`select`] with the toolchain group's switch exposed (Phase 012, FR-012-10b).
///
/// `renvor new` always declares, which is what [`select`] says. `generate auth` re-renders a
/// tree whose record may predate the declaration: with `toolchain_declared = false` the
/// `TOOLCHAIN` group is left out, so no `rust-toolchain.toml` is planned — and the context
/// built with the same flag renders `Cargo.toml` without its `rust-version` line. A pin is
/// never inserted into a tree that did not declare one.
#[must_use]
pub fn select_with_toolchain(
    configuration: &ProjectConfiguration,
    toolchain_declared: bool,
) -> TemplateSet {
    let mut set = if configuration.is_starter() {
        select_starter(configuration)
    } else {
        select_skeleton(configuration)
    };
    if toolchain_declared {
        set.entries.extend_from_slice(TOOLCHAIN);
    }
    set
}

/// The skeleton's selection: every group the configuration honours, and nothing else.
fn select_skeleton(configuration: &ProjectConfiguration) -> TemplateSet {
    let mut entries = BASE.to_vec();
    if configuration.example_domain() {
        entries.extend_from_slice(EXAMPLE_DOMAIN);
    }
    if configuration.seed_data() {
        entries.extend_from_slice(SEED_DATA);
    }
    if configuration.database().is_some() {
        entries.extend_from_slice(MIGRATIONS);
    }
    match configuration.orm() {
        Some(crate::config::model::Orm::Sqlx) => entries.extend_from_slice(PERSISTENCE_SQLX),
        Some(crate::config::model::Orm::SeaOrm) => entries.extend_from_slice(PERSISTENCE_SEAORM),
        None => {}
    }
    if configuration.container() {
        entries.extend_from_slice(CONTAINER);
    }
    TemplateSet {
        version: VERSION,
        entries,
        verbatim: Vec::new(),
        trim_blocks: false,
    }
}

/// The starter's selection (Phase 011): every group the configuration honours, and nothing else.
///
/// The same rule as the skeleton — a file exists iff the manifest records the choice that
/// produced it — applied to a larger tree. `select` dispatches here when a framework path was
/// given, which is the one fact that separates the two shapes.
fn select_starter(configuration: &ProjectConfiguration) -> TemplateSet {
    let mut entries = STARTER_BASE.to_vec();
    if configuration.example_domain() {
        entries.extend_from_slice(STARTER_EXAMPLE_DOMAIN);
    }
    if configuration.seed_data() {
        entries.extend_from_slice(STARTER_SEED_DATA);
    }
    if configuration.database().is_some() {
        entries.extend_from_slice(STARTER_DATABASE);
    }
    // The item repository, its entity, and its migration exist for the example domain: without
    // it there is no table to reach, and a repository over nothing would be an inert file.
    if configuration.example_domain() {
        entries.extend_from_slice(STARTER_MIGRATIONS);
        match configuration.orm() {
            Some(crate::config::model::Orm::Sqlx) => {
                entries.extend_from_slice(STARTER_PERSISTENCE_SQLX);
            }
            Some(crate::config::model::Orm::SeaOrm) => {
                entries.extend_from_slice(STARTER_PERSISTENCE_SEAORM);
            }
            None => {}
        }
    }
    if configuration.auth() == crate::config::model::AuthStarter::Session {
        entries.extend_from_slice(STARTER_AUTH);
    }
    let capabilities = configuration.capabilities();
    if !capabilities.is_empty() {
        entries.extend_from_slice(STARTER_CAPABILITIES);
    }
    if capabilities.contains(Capability::Cache) {
        entries.extend_from_slice(STARTER_CACHE);
    }
    if capabilities.contains(Capability::Jobs) {
        entries.extend_from_slice(STARTER_JOBS);
    }
    if capabilities.contains(Capability::Mail) {
        entries.extend_from_slice(STARTER_MAIL);
    }
    if capabilities.contains(Capability::Storage) {
        entries.extend_from_slice(STARTER_STORAGE);
    }
    if capabilities.contains(Capability::Observability) {
        entries.extend_from_slice(STARTER_OBSERVABILITY);
    }
    if configuration.container() {
        entries.extend(container_for_starter());
    }
    TemplateSet {
        version: VERSION,
        entries,
        verbatim: verbatim_migrations(configuration),
        trim_blocks: true,
    }
}

/// The container group minus `.env.example`, which the starter's own template supersedes: the
/// starter's example names every key the application reads — the container passwords included —
/// where the skeleton's names the container passwords alone.
fn container_for_starter() -> impl Iterator<Item = TemplateEntry> {
    CONTAINER
        .iter()
        .copied()
        .filter(|entry| entry.path != ".env.example")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::render::Renderer;

    #[test]
    fn the_whole_catalogue_loads_and_compiles() {
        // THE LOAD-TIME GUARANTEE, applied to every shippable entry rather than to a selection.
        // I-5 claims a malformed entry "cannot exist in a shipped binary"; this is what makes that
        // claim true rather than true-of-whatever-was-exercised.
        let set = TemplateSet {
            version: VERSION,
            entries: catalogue(),
            verbatim: Vec::new(),
            trim_blocks: false,
        };
        Renderer::new(set).expect("every embedded template must validate and compile");
    }

    #[test]
    fn the_whole_starter_catalogue_loads_and_compiles() {
        // The same load-time guarantee for the Phase 011 groups.
        let set = TemplateSet {
            version: VERSION,
            entries: starter_catalogue(),
            verbatim: Vec::new(),
            trim_blocks: true,
        };
        Renderer::new(set).expect("every embedded starter template must validate and compile");
    }

    #[test]
    fn no_two_starter_groups_declare_the_same_output_path() {
        let all = starter_catalogue();
        let mut paths: Vec<&str> = all.iter().map(|entry| entry.path).collect();
        let total = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(
            paths.len(),
            total,
            "two starter entries share an output path"
        );
    }

    #[test]
    fn the_starter_copies_exactly_the_migration_sets_its_selection_needs() {
        // The verbatim list follows the selection: no auth set without `session`, no jobs set
        // without `jobs`, and the engine's set, not the other engine's.
        let base = tempfile::tempdir().expect("tempdir");
        let framework = fake_checkout(base.path(), "1.94.0", "1.94.0");
        let answers =
            |auth: &str, capabilities: &str, database: &str| crate::config::model::Answers {
                name: Some("demo".to_owned()),
                destination: base.path().join("demo"),
                local_domain: None,
                target: "api".to_owned(),
                transport: None,
                container: false,
                local_https: false,
                seed_data: false,
                example_domain: false,
                orm: None,
                database: Some(database.to_owned()),
                database_version: None,
                database_name: None,
                database_user: None,
                database_port: None,
                container_cache: None,
                cache_port: None,
                auth: Some(auth.to_owned()),
                capabilities: Some(capabilities.to_owned()),
                framework_path: Some(framework.clone()),
            };
        let paths = |auth: &str, capabilities: &str, database: &str| -> Vec<String> {
            let (configuration, _) =
                ProjectConfiguration::resolve(answers(auth, capabilities, database))
                    .expect("resolves");
            select(&configuration)
                .verbatim
                .into_iter()
                .map(|entry| entry.path)
                .collect()
        };
        assert!(paths("none", "none", "postgres").is_empty());
        let auth_only = paths("session", "mail", "mysql");
        assert!(
            auth_only.iter().all(|path| path.contains("_auth_")),
            "{auth_only:?}"
        );
        assert_eq!(auth_only.len(), 16, "MySQL's auth set is eight pairs");
        let both = paths("session", "mail,jobs", "postgres");
        assert_eq!(
            both.len(),
            18 + 10,
            "PostgreSQL's nine auth pairs and five jobs pairs"
        );
        assert!(both.iter().all(|path| path.starts_with("migrations/")));
    }

    #[test]
    fn no_two_groups_declare_the_same_output_path() {
        // Two groups claiming one path would mean the later silently overwrote the earlier — and
        // which one wins would depend on selection order, which is not a thing a generator should
        // have.
        let all = catalogue();
        let mut paths: Vec<&str> = all.iter().map(|entry| entry.path).collect();
        let total = paths.len();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(
            paths.len(),
            total,
            "two catalogue entries share an output path"
        );
    }

    #[test]
    fn the_version_is_recorded_and_is_not_the_crate_version() {
        // A template version that tracked the crate version would claim a different tree on every
        // release, defeating SC-016's reproducibility comparison.
        assert!(!VERSION.is_empty());
        assert_ne!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    // ── Phase 012, L-2: the toolchain group (T-012-03) ────────────────────────────────────
    //
    // Every test below was written BEFORE `select_with_toolchain`, the `TOOLCHAIN` group, and
    // the four context keys existed (RED observed as a compile failure naming them), then made
    // green by the implementation above and in `commands/new.rs`.

    use std::path::{Path, PathBuf};

    use crate::commands::new::Context;
    use crate::config::model::Answers;

    /// A skeleton's answers: nothing selected.
    fn skeleton_answers(destination: PathBuf) -> Answers {
        Answers {
            name: Some("demo".to_owned()),
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

    /// The six skeleton variants the snapshot suite pins, as answers, so the in-module proof
    /// covers every tree `select` can produce for the skeleton.
    fn skeleton_variants(base: &Path) -> Vec<(&'static str, Answers)> {
        let variant = |tag: &'static str, edit: fn(&mut Answers)| {
            let mut answers = skeleton_answers(base.join(tag));
            edit(&mut answers);
            (tag, answers)
        };
        vec![
            variant("bare", |_| {}),
            variant("domain", |a| a.example_domain = true),
            variant("seeded", |a| {
                a.example_domain = true;
                a.seed_data = true;
            }),
            variant("seeded-container", |a| {
                a.example_domain = true;
                a.seed_data = true;
                a.container = true;
            }),
            variant("postgres", |a| {
                a.example_domain = true;
                a.database = Some("postgres".to_owned());
            }),
            variant("mysql-seaorm", |a| {
                a.example_domain = true;
                a.database = Some("mysql".to_owned());
                a.orm = Some("seaorm".to_owned());
            }),
        ]
    }

    /// A framework checkout with exactly what `FrameworkSource::validate_path` reads — the
    /// workspace manifest with its MSRV, the facade's manifest, a lockfile, and the pin.
    fn fake_checkout(base: &Path, channel: &str, msrv: &str) -> PathBuf {
        let root = base.join("framework");
        std::fs::create_dir_all(root.join("crates/renvor")).expect("mkdir");
        std::fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace]\nmembers = [\"crates/renvor\"]\n[workspace.package]\nrust-version = \
                 \"{msrv}\"\n"
            ),
        )
        .expect("write");
        std::fs::write(
            root.join("crates/renvor/Cargo.toml"),
            "[package]\nname = \"renvor\"\nversion = \"0.0.0\"\n",
        )
        .expect("write");
        std::fs::write(root.join("Cargo.lock"), "version = 4\n").expect("write");
        std::fs::write(
            root.join("rust-toolchain.toml"),
            format!("[toolchain]\nchannel = \"{channel}\"\n"),
        )
        .expect("write");
        root
    }

    /// A starter's answers against `framework`: a database, the example domain, and — when
    /// asked — the container profile, so the Dockerfile renders.
    fn starter_answers(destination: PathBuf, framework: PathBuf, container: bool) -> Answers {
        let mut answers = skeleton_answers(destination);
        answers.database = Some("postgres".to_owned());
        answers.example_domain = true;
        answers.container = container;
        answers.framework_path = Some(framework);
        answers
    }

    /// Renders `configuration` through the real renderer into a fresh directory under `base`
    /// and returns that directory. `declared` is the toolchain-group switch of FR-012-10b.
    fn render(
        base: &Path,
        tag: &str,
        configuration: &ProjectConfiguration,
        declared: bool,
    ) -> PathBuf {
        let root = base.join(format!("render-{tag}"));
        std::fs::create_dir_all(&root).expect("mkdir");
        let dir =
            cap_std::fs::Dir::open_ambient_dir(&root, cap_std::ambient_authority()).expect("opens");
        let context = Context::build_with_toolchain(configuration, declared);
        let renderer = Renderer::new(select_with_toolchain(configuration, declared))
            .expect("the selection compiles");
        renderer.render_into(&dir, &context).expect("renders");
        root
    }

    fn read(root: &Path, path: &str) -> Option<String> {
        std::fs::read_to_string(root.join(path)).ok()
    }

    /// The rendered pin, read the way the generator reads a checkout's: by the same parser.
    fn rendered_channel(root: &Path) -> String {
        crate::toolchain::pin::read_channel(&root.join("rust-toolchain.toml"))
            .expect("the rendered pin file is an exact release the generator itself accepts")
            .to_string()
    }

    /// `[package].rust-version` of a rendered manifest, if the line is there.
    fn rendered_rust_version(root: &Path) -> Option<String> {
        let manifest = read(root, "Cargo.toml").expect("Cargo.toml is rendered");
        let document: toml::Value = toml::from_str(&manifest).expect("the manifest is TOML");
        document
            .get("package")?
            .get("rust-version")?
            .as_str()
            .map(str::to_owned)
    }

    #[test]
    fn every_generated_tree_declares_a_pin_and_a_rust_version() {
        // FR-012-1 and FR-012-2: every skeleton variant the snapshot suite pins, and a starter,
        // render `rust-toolchain.toml` with an exact release and `Cargo.toml` with the MSRV.
        let base = tempfile::tempdir().expect("tempdir");
        for (tag, answers) in skeleton_variants(base.path()) {
            let (configuration, _) = ProjectConfiguration::resolve(answers).expect("resolves");
            let root = render(base.path(), tag, &configuration, true);
            let pin = read(&root, "rust-toolchain.toml").expect("the pin file is rendered");
            assert!(
                pin.contains("components = [\"rustfmt\", \"clippy\"]")
                    && pin.contains("profile = \"minimal\""),
                "the pin file declares the two components and the minimal profile"
            );
            assert_eq!(
                rendered_channel(&root),
                env!("CARGO_PKG_RUST_VERSION"),
                "a skeleton pins the generator's MSRV"
            );
            assert_eq!(
                rendered_rust_version(&root).as_deref(),
                Some(env!("CARGO_PKG_RUST_VERSION")),
                "a skeleton's rust-version is the generator's MSRV"
            );
        }
        let framework = fake_checkout(base.path(), "1.95.0", "1.94.0");
        let (configuration, _) = ProjectConfiguration::resolve(starter_answers(
            base.path().join("starter"),
            framework,
            false,
        ))
        .expect("resolves");
        let root = render(base.path(), "starter", &configuration, true);
        assert_eq!(rendered_channel(&root), "1.95.0");
        assert_eq!(rendered_rust_version(&root).as_deref(), Some("1.94.0"));
    }

    #[test]
    fn a_starter_pin_is_the_checkouts_channel_and_its_msrv_is_the_manifests() {
        // FR-012-1's starter rule at the render: the two values come from two files, each read
        // on its own, and neither is the other.
        let base = tempfile::tempdir().expect("tempdir");
        let framework = fake_checkout(base.path(), "1.95.0", "1.94.0");
        let (configuration, _) = ProjectConfiguration::resolve(starter_answers(
            base.path().join("starter"),
            framework,
            false,
        ))
        .expect("resolves");
        let recorded = configuration
            .framework()
            .expect("a starter records its source");
        assert_eq!(recorded.toolchain_channel(), "1.95.0");
        assert_eq!(recorded.rust_version(), "1.94.0");
        let root = render(base.path(), "starter", &configuration, true);
        assert_eq!(rendered_channel(&root), "1.95.0");
        assert_eq!(rendered_rust_version(&root).as_deref(), Some("1.94.0"));
        let readme = read(&root, "README.md").expect("README.md is rendered");
        assert!(
            readme.contains("1.95.0") && readme.contains("1.94.0"),
            "the starter README states the pin and the MSRV as two numbers"
        );
    }

    #[test]
    fn a_skeletons_pin_and_rust_version_are_the_generators_msrv() {
        // D-L2-4: the generator's MSRV is the skeleton's default exact pin AND its
        // `rust-version` — the chosen default. `doctor::REQUIRED_RUST` is the same constant,
        // asserted equal to `CARGO_PKG_RUST_VERSION` in its own test.
        let base = tempfile::tempdir().expect("tempdir");
        let (configuration, _) =
            ProjectConfiguration::resolve(skeleton_answers(base.path().join("demo")))
                .expect("resolves");
        let root = render(base.path(), "skeleton", &configuration, true);
        let msrv = env!("CARGO_PKG_RUST_VERSION");
        assert_eq!(rendered_channel(&root), msrv);
        assert_eq!(rendered_rust_version(&root).as_deref(), Some(msrv));
        let readme = read(&root, "README.md").expect("README.md is rendered");
        assert!(
            readme.contains(msrv),
            "the skeleton README states the generator's MSRV"
        );
    }

    #[test]
    fn a_legacy_render_declares_no_pin_and_no_rust_version() {
        // FR-012-10b: with the group off — `generate auth` on a record without `[toolchain]` —
        // no `rust-toolchain.toml` is planned and `Cargo.toml` carries no `rust-version`, for
        // BOTH tree kinds. Nothing is inserted silently.
        let base = tempfile::tempdir().expect("tempdir");
        let (skeleton, _) =
            ProjectConfiguration::resolve(skeleton_answers(base.path().join("demo")))
                .expect("resolves");
        let framework = fake_checkout(base.path(), "1.95.0", "1.94.0");
        let (starter, _) = ProjectConfiguration::resolve(starter_answers(
            base.path().join("starter"),
            framework,
            true,
        ))
        .expect("resolves");
        for (tag, configuration) in [("skeleton", &skeleton), ("starter", &starter)] {
            assert!(
                select_with_toolchain(configuration, false)
                    .entries
                    .iter()
                    .all(|entry| entry.path != "rust-toolchain.toml"),
                "a legacy selection plans no pin file"
            );
            assert!(
                select_with_toolchain(configuration, true)
                    .entries
                    .iter()
                    .any(|entry| entry.path == "rust-toolchain.toml"),
                "a declaring selection plans the pin file"
            );
            let root = render(base.path(), tag, configuration, false);
            assert!(
                read(&root, "rust-toolchain.toml").is_none(),
                "a legacy render writes no pin file"
            );
            assert_eq!(
                rendered_rust_version(&root),
                None,
                "a legacy render's manifest carries no rust-version"
            );
            let manifest = read(&root, "Cargo.toml").expect("Cargo.toml is rendered");
            assert!(
                !manifest.contains("\n\n\n"),
                "the absent line leaves no blank-line artefact"
            );
            let readme = read(&root, "README.md").expect("README.md is rendered");
            assert!(
                readme.contains("pins no toolchain"),
                "a legacy README says the project pins nothing"
            );
        }
        // The starter's Dockerfile still names a concrete builder: the generator's MSRV, which
        // is the figure template version 7 hard-coded, now derived rather than restated.
        let root = render(base.path(), "starter-docker", &starter, false);
        let dockerfile = read(&root, "Dockerfile").expect("Dockerfile is rendered");
        assert!(
            dockerfile.contains(&format!(
                "FROM docker.io/library/rust:{}-slim AS build",
                env!("CARGO_PKG_RUST_VERSION")
            )),
            "a legacy Dockerfile's builder is the generator's MSRV"
        );
    }

    #[test]
    fn the_readme_names_the_pin_the_msrv_the_rustup_floor_and_the_bare_toolchain_rule() {
        // FR-012-3, on both rendered READMEs: the two numbers, the rustup floor, the inert-
        // without-rustup rule with `rust-version` still refusing, and FR-012-5c's incompatibility
        // sentence.
        let base = tempfile::tempdir().expect("tempdir");
        let (skeleton, _) =
            ProjectConfiguration::resolve(skeleton_answers(base.path().join("demo")))
                .expect("resolves");
        let framework = fake_checkout(base.path(), "1.95.0", "1.94.0");
        let (starter, _) = ProjectConfiguration::resolve(starter_answers(
            base.path().join("starter"),
            framework,
            false,
        ))
        .expect("resolves");
        let msrv = env!("CARGO_PKG_RUST_VERSION");
        for (tag, configuration, pin, rust_version) in [
            ("skeleton", &skeleton, msrv, msrv),
            ("starter", &starter, "1.95.0", "1.94.0"),
        ] {
            let root = render(base.path(), tag, configuration, true);
            let readme = read(&root, "README.md").expect("README.md is rendered");
            assert!(
                readme.contains("## Toolchain"),
                "the README has a toolchain section"
            );
            assert!(
                readme.contains(&format!("`{pin}`"))
                    && readme.contains(&format!("`{rust_version}`")),
                "the README states the pin and the MSRV as two numbers"
            );
            for needle in [
                crate::toolchain::RUSTUP_FLOOR,
                "inert without rustup",
                "`rust-version`",
                "rebuild the generator, not the project",
                "`rust-toolchain.toml`",
                "cargo clippy --all-targets -- -D warnings",
                "cargo test",
                "-D warnings",
                "launch observation plus queried identity",
                "`.renvor/generated.toml`",
                "renvor generate toolchain",
                "template version 8",
            ] {
                assert!(
                    readme.contains(needle),
                    "the README carries every required sentence"
                );
            }
        }
    }

    #[test]
    fn the_dockerfile_builder_tag_equals_the_rendered_channel() {
        // FR-012-9: the builder tag is derived from the pin, on both tree kinds, and the
        // comment names the file the tag now comes from.
        let base = tempfile::tempdir().expect("tempdir");
        let mut skeleton_answers = skeleton_answers(base.path().join("demo"));
        skeleton_answers.container = true;
        let (skeleton, _) = ProjectConfiguration::resolve(skeleton_answers).expect("resolves");
        let framework = fake_checkout(base.path(), "1.95.0", "1.94.0");
        let (starter, _) = ProjectConfiguration::resolve(starter_answers(
            base.path().join("starter"),
            framework,
            true,
        ))
        .expect("resolves");
        for (tag, configuration) in [("skeleton", &skeleton), ("starter", &starter)] {
            let root = render(base.path(), tag, configuration, true);
            let channel = rendered_channel(&root);
            let dockerfile = read(&root, "Dockerfile").expect("Dockerfile is rendered");
            assert!(
                dockerfile.contains(&format!(
                    "FROM docker.io/library/rust:{channel}-slim AS build"
                )),
                "the builder tag is the rendered channel"
            );
            assert!(
                dockerfile.contains("`rust-toolchain.toml`"),
                "the comment names the pin file"
            );
        }
        assert_ne!(
            rendered_channel(&render(base.path(), "starter-again", &starter, true)),
            env!("CARGO_PKG_RUST_VERSION"),
            "the starter control pins a channel that is not the generator's MSRV"
        );
    }

    #[test]
    fn the_builder_sets_rustup_auto_install_0() {
        // FR-012-9: the builder stage forbids rustup from installing, so a pin the image lacks
        // fails by name and nothing downloads another compiler inside the build.
        let base = tempfile::tempdir().expect("tempdir");
        let mut answers = skeleton_answers(base.path().join("demo"));
        answers.container = true;
        let (configuration, _) = ProjectConfiguration::resolve(answers).expect("resolves");
        let root = render(base.path(), "skeleton", &configuration, true);
        let dockerfile = read(&root, "Dockerfile").expect("Dockerfile is rendered");
        let builder = dockerfile.find(" AS build\n").expect("a builder stage");
        let env = dockerfile
            .find("\nENV RUSTUP_AUTO_INSTALL=0\n")
            .expect("the builder sets RUSTUP_AUTO_INSTALL=0");
        let runtime = dockerfile
            .find("\nFROM gcr.io/distroless/")
            .expect("a runtime stage");
        assert!(
            builder < env && env < runtime,
            "the variable is set in the builder stage, before the runtime stage"
        );
    }
}
