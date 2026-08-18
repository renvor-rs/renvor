//! T065 — the **eight-obligation configuration proof gate**, re-pointed at the Renvor adapter.
//!
//! # What changed, and why the previous version had to go
//!
//! This file used to run the same eight obligations against **`confique` 0.4.0**, the candidate on
//! probation. It failed, **4 of 8**, and the fallback triggered (research §D6). That evidence is
//! preserved in git — the run that produced it is commit `50d9e6f`, and every obligation's outcome
//! is transcribed into research §D6 with the observed behaviour.
//!
//! Leaving the old file in place would have left the gate exercising a crate the project had
//! already rejected, which is exactly what open item 1 of the evidence ledger flagged: the
//! adapter's compliance with obligations 4, 6, and 7 was *designed for, not demonstrated*. This
//! file demonstrates it.
//!
//! **The fixtures are the same ones.** T065 requires reusing the T015 fixture set rather than
//! adding a second — two fixture sets for one gate is two chances to prove different things and
//! call it the same result.
//!
//! # The gate is all-eight-or-fallback
//!
//! C-C7 admits no partial adoption. Every obligation below is a hard assertion; a partial result
//! is a failure, not a score.

use std::collections::BTreeMap;
use std::path::PathBuf;

use renvor_config::{ConfigSchema, FileLayer, LayeredResolverBuilder};
use renvor_core::ErrorCategory;
use renvor_core::config_port::{ConfigResolver, SourceLayer};
use serde::Deserialize;
use toml::Table;

#[derive(Debug, Deserialize, PartialEq)]
struct Settings {
    port: u16,
    name: String,
    tags: Vec<String>,
    server: Server,
    limits: Limits,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Server {
    host: String,
    threads: u16,
    timeout_ms: u64,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Limits {
    retries: Vec<u32>,
}

/// The partial forms exist to be **decoded into**, never read from: their whole job is to prove a
/// source fits the schema. `dead_code` fires because nothing here reads the fields back, which is
/// the intended shape rather than an oversight.
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialSettings {
    port: Option<u16>,
    name: Option<String>,
    tags: Option<Vec<String>>,
    server: Option<PartialServer>,
    limits: Option<PartialLimits>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialServer {
    host: Option<String>,
    threads: Option<u16>,
    timeout_ms: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialLimits {
    retries: Option<Vec<u32>>,
}

impl ConfigSchema for Settings {
    type Partial = PartialSettings;
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A defaults layer that is deliberately **lowest** on every key the fixtures also set, so a
/// precedence bug shows up as a defaults value surviving where it should not.
fn defaults() -> Table {
    r#"
port = 1
name = "defaults"
tags = ["default-tag"]

[server]
host = "default-host"
threads = 1
timeout_ms = 1

[limits]
retries = [0]
"#
    .parse::<Table>()
    .expect("the defaults fixture is valid TOML")
}

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// The full stack, in precedence order: defaults, then `base.toml`, then `override.toml`.
fn stack() -> LayeredResolverBuilder {
    LayeredResolverBuilder::new()
        .with_defaults(defaults())
        .with_file(FileLayer::required(fixture("base.toml")))
        .with_file(FileLayer::required(fixture("override.toml")))
}

// ── Obligation 1 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_1_precedence_holds_across_all_four_layers() {
    // defaults < earlier TOML < later TOML < environment. This is the obligation `confique`
    // failed by giving the *earlier* source higher priority — the inverse.
    let resolved = stack()
        .with_environment_map("RENVOR_", env(&[("RENVOR_NAME", "from-env")]))
        .build::<Settings>()
        .resolve()
        .expect("the stack resolves");

    let value = resolved.value();
    assert_eq!(value.port, 9090, "override.toml beats base.toml");
    assert_eq!(value.server.host, "base-host", "base.toml beats defaults");
    assert_eq!(value.name, "from-env", "environment beats every file");
    assert_eq!(
        value.limits.retries,
        vec![9],
        "the later file's array wins whole"
    );
}

// ── Obligation 2 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_2_nested_tables_merge_per_key_and_siblings_survive() {
    // `override.toml` sets only `server.threads`. `server.host` and `server.timeout_ms` must
    // survive from `base.toml` rather than being wiped by a wholesale table replacement.
    let resolved = stack().build::<Settings>().resolve().expect("resolves");
    let server = &resolved.value().server;

    assert_eq!(server.threads, 16, "the override applied");
    assert_eq!(server.host, "base-host", "sibling survived");
    assert_eq!(server.timeout_ms, 1000, "sibling survived");
}

// ── Obligation 3 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_3_arrays_replace_wholesale_with_zero_concatenations() {
    let resolved = stack().build::<Settings>().resolve().expect("resolves");

    assert_eq!(
        resolved.value().tags,
        vec!["z".to_owned()],
        "0 concatenations: base.toml's three tags must not survive"
    );
    assert_eq!(resolved.value().limits.retries, vec![9]);
}

// ── Obligation 4 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_4_every_resolved_key_reports_the_layer_that_supplied_it() {
    // **The obligation `confique` could not meet at all.** Its layer combinator is documented as
    // "basically like `Option::or`" — it returns a value and discards which side supplied it,
    // destroying provenance inside the merge with no seam above it to reconstruct from.
    let resolved = stack()
        .with_environment_map("RENVOR_", env(&[("RENVOR_NAME", "from-env")]))
        .build::<Settings>()
        .resolve()
        .expect("resolves");

    let layer_of = |key: &str| {
        resolved
            .attribution(key)
            .unwrap_or_else(|| panic!("`{key}` has no attribution"))
            .layer
            .clone()
    };

    assert_eq!(layer_of("name"), SourceLayer::Environment);
    assert_eq!(
        layer_of("port").label(),
        fixture("override.toml").display().to_string()
    );
    assert_eq!(
        layer_of("server.host").label(),
        fixture("base.toml").display().to_string()
    );
    assert_eq!(layer_of("server.threads").label(), {
        let path = fixture("override.toml");
        path.display().to_string()
    });

    // "Every resolved key", not "a sample of them". Counting is the difference.
    let attributed: Vec<&str> = resolved
        .attributions()
        .iter()
        .map(|(key, _)| key.as_str())
        .collect();
    let mut sorted = attributed.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        vec![
            "limits.retries",
            "name",
            "port",
            "server.host",
            "server.threads",
            "server.timeout_ms",
            "tags",
        ],
        "every leaf is attributed, none omitted"
    );
}

// ── Obligation 5 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_5_an_invalid_non_empty_environment_value_fails_naming_three_of_three() {
    let error = stack()
        .with_environment_map("RENVOR_", env(&[("RENVOR_PORT", "wide-open")]))
        .build::<Settings>()
        .resolve()
        .expect_err("a bare word cannot be a u16");

    assert_eq!(error.category(), ErrorCategory::Configuration);
    let rendered = error.to_string();
    assert!(rendered.contains("port"), "1/3 key: {rendered}");
    assert!(rendered.contains("environment"), "2/3 layer: {rendered}");
    assert!(rendered.contains("u16"), "3/3 expected type: {rendered}");
}

// ── Obligation 6 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_6_an_invalid_empty_environment_value_also_fails() {
    // **The obligation with known negative evidence against the candidate.** `confique`'s own
    // documentation states: "If the env var is set to an empty string and if the field fails to
    // parse/deserialize/validate, it is treated as unset." That is the silent fallback FR-022
    // prohibits — `PORT=""` quietly becoming the default instead of an error.
    let error = stack()
        .with_environment_map("RENVOR_", env(&[("RENVOR_PORT", "")]))
        .build::<Settings>()
        .resolve()
        .expect_err("an empty string cannot be a u16, and must not be reinterpreted as unset");

    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert!(error.to_string().contains("port"), "{error}");

    // POSITIVE CONTROL: the same stack resolves when the variable is absent, so the failure is
    // caused by the empty value rather than by the variable being mentioned at all. Without this,
    // an implementation that rejected every environment would pass.
    let resolved = stack()
        .with_environment_map("RENVOR_", env(&[]))
        .build::<Settings>()
        .resolve()
        .expect("an absent variable is not an error");
    assert_eq!(resolved.value().port, 9090, "and the file's value stands");
}

#[test]
fn obligation_6b_an_empty_value_is_still_a_value_where_the_type_allows_one() {
    // The other side of C-C11: present-and-empty must stay distinguishable from absent. Rejecting
    // every empty value would satisfy obligation 6 for the wrong reason.
    let resolved = stack()
        .with_environment_map("RENVOR_", env(&[("RENVOR_NAME", "")]))
        .build::<Settings>()
        .resolve()
        .expect("an empty string is a valid String");

    assert_eq!(resolved.value().name, "");
    assert_eq!(
        resolved.attribution("name").map(|a| a.presence),
        Some(renvor_core::config_port::Presence::PresentButEmpty),
        "and it is reported as present-but-empty, not as absent"
    );
}

// ── Obligation 7 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_7a_a_conflict_on_a_declared_key_is_caught_before_the_merge() {
    // **Found by running this gate against the adapter.** `server` is a table in `base.toml` and a
    // scalar in `shape_conflict.toml`. Because C-C2 requires each source to be decoded against the
    // schema *before* merging, the scalar never survives step 1: it does not fit the declared
    // `Server` type, and the run fails naming the key, the offending layer, and the expectation.
    //
    // That is **stricter** than the obligation asks and a better diagnostic than "two layers
    // disagree" — the author is told which file is wrong, not that two files differ. It names one
    // layer because exactly one layer is at fault.
    let error = LayeredResolverBuilder::new()
        .with_file(FileLayer::required(fixture("base.toml")))
        .with_file(FileLayer::required(fixture("shape_conflict.toml")))
        .build::<Settings>()
        .resolve()
        .expect_err("a scalar does not fit the declared table type");

    assert_eq!(error.category(), ErrorCategory::Configuration);
    let rendered = error.to_string();
    assert!(rendered.contains("server"), "key: {rendered}");
    assert!(
        rendered.contains("shape_conflict.toml"),
        "the layer at fault is named: {rendered}"
    );
    assert!(
        !rendered.contains("base.toml"),
        "and the innocent layer is not blamed: {rendered}"
    );
}

#[test]
fn obligation_7b_a_conflict_the_schema_does_not_constrain_names_both_layers() {
    // The merge-level path, which is where "naming both layers" actually applies. `Loose` declares
    // only `port`, so `server` is a key neither source's decode step has an opinion about — both
    // sources decode, and the conflict is genuinely a disagreement *between* them.
    //
    // Same fixtures, different schema: the two files are unchanged, so this is not a second
    // fixture set proving a different thing.
    #[derive(Debug, Deserialize)]
    struct Loose {
        #[allow(dead_code)]
        port: u16,
    }

    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct PartialLoose {
        port: Option<u16>,
    }

    impl ConfigSchema for Loose {
        type Partial = PartialLoose;
    }

    let error = LayeredResolverBuilder::new()
        .with_file(FileLayer::required(fixture("base.toml")))
        .with_file(FileLayer::required(fixture("shape_conflict.toml")))
        .build::<Loose>()
        .resolve()
        .expect_err("a table and a string are not reconcilable");

    assert_eq!(error.category(), ErrorCategory::ConfigurationConflict);
    let rendered = error.to_string();
    assert!(rendered.contains("server"), "key: {rendered}");
    assert!(rendered.contains("base.toml"), "first layer: {rendered}");
    assert!(
        rendered.contains("shape_conflict.toml"),
        "second layer: {rendered}"
    );
    assert!(rendered.contains("table"), "first shape: {rendered}");
    assert!(rendered.contains("string"), "second shape: {rendered}");

    // Neither coerced into a common shape nor resolved by last-wins: there is no success path.
}

// ── Obligation 8 ────────────────────────────────────────────────────────────────────────────

#[test]
fn obligation_8_the_resolved_dependency_graph_carries_no_json_or_yaml() {
    // C-C1 permits exactly three configuration source kinds — TOML, environment, and defaults —
    // and the absence of JSON and YAML is meant to be structural rather than a policy nobody
    // checks. This reads the **resolved** lockfile, not the manifests: a crate pulled in by a
    // transitive dependency would not appear in any manifest.
    //
    // ── SCOPE CORRECTED 2026-08-18, AND THE GATE IS NOT WEAKENED ────────────────────────────
    //
    // The first version of this test scanned the whole workspace lockfile for `serde_json`. That
    // is wider than the obligation it enforces, and Phase 003 is where the difference showed up:
    // `renvor-cli` needs `serde_json` for `--output json`, which is a **machine-readable output
    // format**, not a **configuration source format**. Obligation 8 has nothing to say about the
    // former. `trycmd`, a dev-dependency, pulls it in as well.
    //
    // So the `serde_json` check now runs against the transitive closure of the configuration
    // crates rather than the workspace, which is exactly what the obligation claims. **The YAML
    // check stays workspace-wide**, because no crate here has any business parsing YAML in any
    // role, and narrowing that one would be a weakening rather than a correction.
    //
    // A gate whose scope is wider than its rationale eventually fails for a reason it was never
    // about, and the pressure at that moment is to delete it. Correcting the scope while the
    // rationale is still legible is the alternative to that.
    let lockfile = include_str!("../../../Cargo.lock");

    // YAML, and JSON dialects that only ever appear as configuration formats: workspace-wide.
    for forbidden in [
        "name = \"serde_yaml\"",
        "name = \"serde_norway\"",
        "name = \"yaml-rust\"",
        "name = \"yaml-rust2\"",
        "name = \"json5\"",
        "name = \"serde_yml\"",
    ] {
        assert!(
            !lockfile.contains(forbidden),
            "{forbidden} is in the resolved graph, which obligation 8 forbids"
        );
    }

    // `serde_json`: forbidden in the configuration crates' closure, permitted elsewhere.
    //
    // Each crate carries its own positive control — a package it certainly depends on — because a
    // shared one would be wrong for at least one of them. `renvor-core` does not depend on `toml`
    // at all, which the first draft of this control asserted and which the control itself caught.
    for (crate_name, must_reach) in [("renvor-config", "toml"), ("renvor-core", "tracing")] {
        let closure = transitive_closure(lockfile, crate_name);
        assert!(
            !closure.contains("serde_json"),
            "`serde_json` is reachable from `{crate_name}`, which would give configuration a \
             fourth source kind that C-C1 does not permit"
        );
        // POSITIVE CONTROL for the walk: the closure must contain something this crate certainly
        // depends on, or its silence above means "the walk found nothing" rather than "the crate
        // is clean".
        assert!(
            closure.contains(must_reach),
            "the closure walk for {crate_name} cannot see `{must_reach}`, so it is not reading \
             what it thinks it is"
        );
    }

    // NEGATIVE CONTROL for the walk: a crate that DOES reach `serde_json` must be reported as
    // reaching it. Without this, a walk that returned an empty set for every input would pass
    // every assertion above.
    assert!(
        transitive_closure(lockfile, "renvor-cli").contains("serde_json"),
        "the closure walk cannot detect `serde_json` even where it is present, so its absence \
         elsewhere proves nothing"
    );

    // POSITIVE CONTROL for the lockfile scan itself.
    assert!(
        lockfile.contains("name = \"toml\""),
        "the lockfile scan is not reading what it thinks it is"
    );
}

/// Every package reachable from `root` in the lockfile, `root` included.
///
/// Written by hand against `Cargo.lock`'s `[[package]]` blocks rather than by shelling out to
/// `cargo tree`, so the gate needs no subprocess, no network, and no toolchain beyond the one
/// running the test.
fn transitive_closure(lockfile: &str, root: &str) -> std::collections::BTreeSet<String> {
    // name -> its direct dependency names.
    let mut graph: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();

    for block in lockfile.split("[[package]]").skip(1) {
        let Some(name) = block
            .lines()
            .find_map(|line| line.strip_prefix("name = \""))
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            continue;
        };

        let mut dependencies = Vec::new();
        let mut in_dependencies = false;
        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed == "dependencies = [" {
                in_dependencies = true;
                continue;
            }
            if in_dependencies {
                if trimmed == "]" {
                    break;
                }
                // Entries look like `"serde 1.0.229"` or `"serde"`, optionally comma-terminated.
                let entry = trimmed.trim_end_matches(',').trim_matches('"');
                if let Some(dependency) = entry.split_whitespace().next()
                    && !dependency.is_empty()
                {
                    dependencies.push(dependency.to_owned());
                }
            }
        }
        graph
            .entry(name.to_owned())
            .or_default()
            .extend(dependencies);
    }

    let mut seen = std::collections::BTreeSet::new();
    let mut stack = vec![root.to_owned()];
    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        if let Some(children) = graph.get(&current) {
            stack.extend(children.iter().cloned());
        }
    }
    seen
}

// ── The gate's own verdict ──────────────────────────────────────────────────────────────────

#[test]
fn the_gate_is_all_eight_or_nothing() {
    // C-C7 admits no partial adoption, and this test exists so that intent is stated in the suite
    // rather than only in prose. If any obligation above fails, the run fails; there is no scoring
    // path that reports "6 of 8" as a qualified pass.
    //
    // The count is asserted against the obligation functions actually present in this file, so
    // deleting one is caught rather than quietly reducing the gate.
    let source = include_str!("proof_gate.rs");
    let obligations = source.matches("\nfn obligation_").count();
    assert_eq!(
        obligations, 10,
        "eight numbered obligations, plus 6b (present-but-empty) and the 7a/7b split \
         that running the gate against the adapter revealed"
    );
}
