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
/// **`2` → `3` (Phase 006).** A project generated with `--database` gains `src/persistence.rs` and
/// a reversible `migrations/0001_create_item` pair, and `renvor.toml` gains a `[persistence]`
/// section. `Cargo.toml` is deliberately **still** dependency-free: no Renvor crate is published,
/// so declaring one would emit a project that cannot build.
///
/// **`1` → `2` (Phase 004).** `renvor.toml` gained `transport`, and `README.md` gained the section
/// describing the dependency to add once the framework crates are published. `Cargo.toml` is
/// deliberately **unchanged**: a generated project still declares no dependency, and still builds.
pub const VERSION: &str = "3";

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

/// Added by `--database`. Both migration halves ship together: the pair is what makes the
/// migration REVERSIBLE, and a declared-reversible migration missing its `.down.sql` is refused at
/// rollback rather than discovered half-way through one.
const PERSISTENCE: &[TemplateEntry] = &[
    TemplateEntry {
        path: "src/persistence.rs",
        body: include_str!("../templates/src_persistence.rs.j2"),
    },
    TemplateEntry {
        path: "migrations/0001_create_item.up.sql",
        body: include_str!("../templates/migrations_up.sql.j2"),
    },
    TemplateEntry {
        path: "migrations/0001_create_item.down.sql",
        body: include_str!("../templates/migrations_down.sql.j2"),
    },
];

/// Added by `--container`.
const CONTAINER: &[TemplateEntry] = &[
    TemplateEntry {
        path: "Dockerfile",
        body: include_str!("../templates/Dockerfile.j2"),
    },
    TemplateEntry {
        path: "compose.yaml",
        body: include_str!("../templates/compose.yaml.j2"),
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
    all.extend_from_slice(PERSISTENCE);
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
        entries.extend_from_slice(PERSISTENCE);
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
