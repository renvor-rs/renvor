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

use crate::config::model::ProjectConfiguration;
use crate::generate::render::{TemplateEntry, TemplateSet};

/// The template-set version recorded in every generated project.
///
/// Bumped whenever any body below changes. It is **not** the crate version: a release that changes
/// no template must not claim to have produced a different tree.
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
pub const VERSION: &str = "6";

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
/// These two **compile**, unlike the direct-SQLx module, because an entity and a repository need
/// `sea-orm` — which is published — and nothing from Renvor. Emitting them as inert text would
/// have made "generated code uses SeaORM idiomatically" a claim about a comment.
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
    all
}

/// Chooses the entries a configuration actually renders.
///
/// Selection is by **honoured choice**, which is what makes data-model invariant I-12 hold: the
/// manifest records a selection only if generation acted on it, and generation acts on a selection
/// only by adding entries here.
#[must_use]
pub fn select(configuration: &ProjectConfiguration) -> TemplateSet {
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
    }
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
        };
        Renderer::new(set).expect("every embedded template must validate and compile");
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
}
