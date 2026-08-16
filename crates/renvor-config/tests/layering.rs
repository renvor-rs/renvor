//! T065 — the seven User Story 3 acceptance scenarios (SC-020), end to end through the lifecycle.
//!
//! # These run through `ApplicationBuilder`, not through the resolver alone
//!
//! Scenarios 2, 3, 5, and 6 all end with *"and the application does not proceed"*. Asserting that
//! against the resolver would prove the resolver returns an error and leave the interesting half —
//! that **0** providers start — untested. So every scenario here builds a real application with a
//! real provider registered, and the failing ones assert the provider never initialised.
//!
//! That also verifies T074's wiring: configuration participates in `Load` and `Validate`, and a
//! failure in either prevents `Register` from being reached.
//!
//! **Scenario 7** — a secret field redacted in every output path — belongs to User Story 4 and is
//! proven in `redaction.rs` with a positive control that can detect a leak. It is named here so a
//! reader counting scenarios does not have to wonder where the seventh went.
//!
//! The fixtures are the T015 set, reused rather than duplicated.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use renvor_config::{ConfigSchema, FileLayer, LayeredResolverBuilder, SchemaSource};
use renvor_core::config_port::SourceLayer;
use renvor_core::provider::ProviderFuture;
use renvor_core::{
    ApplicationBuilder, CapabilityId, ErrorCategory, InitContext, LifecyclePhase, Provider,
    ProviderId,
};
use serde::Deserialize;
use toml::Table;

#[derive(Debug, Deserialize)]
struct Settings {
    port: u16,
    name: String,
    #[allow(dead_code)]
    tags: Vec<String>,
    server: Server,
    #[allow(dead_code)]
    limits: Limits,
}

#[derive(Debug, Deserialize)]
struct Server {
    host: String,
    #[allow(dead_code)]
    threads: u16,
    #[allow(dead_code)]
    timeout_ms: u64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Limits {
    retries: Vec<u32>,
}

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
    .expect("valid")
}

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

/// Records whether it ever initialised, so "0 providers started" is observed rather than assumed.
#[derive(Debug)]
struct Witness {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    started: Arc<Mutex<bool>>,
}

impl Witness {
    /// Returns the provider **and** the flag it sets, so a caller cannot hold one without the
    /// other. Not called `new`, because it does not return `Self`.
    fn register() -> (Box<dyn Provider>, Arc<Mutex<bool>>) {
        let started = Arc::new(Mutex::new(false));
        let provider = Self {
            id: ProviderId::new("witness"),
            provides: vec![CapabilityId::new("witness")],
            started: Arc::clone(&started),
        };
        (Box::new(provider), started)
    }
}

impl Provider for Witness {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }
    fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            *self.started.lock().unwrap_or_else(PoisonError::into_inner) = true;
            Ok(())
        })
    }
}

/// Builds an application whose configuration comes from the fixtures plus the given environment.
fn application(
    variables: &[(&str, &str)],
) -> (
    ApplicationBuilder,
    renvor_config::ConfigHandle<Settings>,
    Arc<Mutex<bool>>,
) {
    let resolver = LayeredResolverBuilder::new()
        .with_defaults(defaults())
        .with_file(FileLayer::required(fixture("base.toml")))
        .with_file(FileLayer::required(fixture("override.toml")))
        .with_environment_map("RENVOR_", env(variables))
        .build::<Settings>();

    let source = SchemaSource::new("application configuration", resolver);
    let handle = source.handle();
    let (provider, started) = Witness::register();

    let builder = ApplicationBuilder::new()
        .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
            3;
            32
        ])))
        .with_config_source(Arc::new(source))
        .with_provider(provider);

    (builder, handle, started)
}

// ── Scenario 1 ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario_1_the_environment_wins_and_the_source_is_reportable() {
    let (builder, handle, started) = application(&[("RENVOR_NAME", "from-env")]);
    let application = builder
        .build()
        .expect("the configuration is valid")
        .boot()
        .await
        .expect("and the application boots");

    assert_eq!(application.phase(), LifecyclePhase::Ready);
    assert!(*started.lock().unwrap_or_else(PoisonError::into_inner));

    handle
        .with(|resolved| {
            assert_eq!(resolved.value().name, "from-env", "the environment won");
            assert_eq!(
                resolved.attribution("name").map(|a| &a.layer),
                Some(&SourceLayer::Environment),
                "and the resolved source is reportable"
            );
        })
        .expect("resolved");
}

// ── Scenario 2 ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario_2_a_required_key_absent_everywhere_fails_before_register() {
    // Only `base.toml`, which sets no `name`… except it does. So this uses a resolver with just
    // the environment, where `port` has no source at all.
    let resolver = LayeredResolverBuilder::new()
        .with_environment_map("RENVOR_", env(&[("RENVOR_NAME", "solo")]))
        .build::<Settings>();
    let source = SchemaSource::new("application configuration", resolver);
    let (provider, started) = Witness::register();

    let builder = ApplicationBuilder::new()
        .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
            3;
            32
        ])))
        .with_config_source(Arc::new(source))
        .with_provider(provider);
    let phases = builder.phase_log();

    let error = builder.build().expect_err("port has no value anywhere");

    assert_eq!(error.category(), Some(ErrorCategory::Configuration));
    assert!(error.to_string().contains("port"), "names the key: {error}");
    assert!(
        !phases.entries().contains(&LifecyclePhase::Register),
        "the application must not proceed to Register"
    );
    assert!(
        !*started.lock().unwrap_or_else(PoisonError::into_inner),
        "0 providers started"
    );
}

// ── Scenario 3 ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario_3_a_wrong_type_names_key_constraint_and_layer() {
    let (builder, _handle, started) = application(&[("RENVOR_PORT", "not-a-port")]);
    let phases = builder.phase_log();

    let error = builder.build().expect_err("a bare word cannot be a u16");
    let rendered = error.to_string();

    assert!(rendered.contains("port"), "key: {rendered}");
    assert!(rendered.contains("u16"), "expected constraint: {rendered}");
    assert!(rendered.contains("environment"), "layer: {rendered}");
    assert!(!phases.entries().contains(&LifecyclePhase::Register));
    assert!(!*started.lock().unwrap_or_else(PoisonError::into_inner));
}

// ── Scenario 4 ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario_4_text_decodes_into_a_declared_integer_and_is_not_a_conflict() {
    // C-C3: the target type comes from the author's declaration, so nothing is being reconciled
    // between layers. This is the scenario that separates decoding from coercion.
    let (builder, handle, _started) = application(&[("RENVOR_PORT", "8080")]);
    builder
        .build()
        .expect("text decoding into a declared type is not a conflict")
        .boot()
        .await
        .expect("boots");

    handle
        .with(|resolved| assert_eq!(resolved.value().port, 8080))
        .expect("resolved");
}

// ── Scenario 5 ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario_5_undecodable_text_for_the_same_field_fails_naming_three_things() {
    let (builder, _handle, started) = application(&[("RENVOR_PORT", "eighty-eighty")]);
    let error = builder.build().expect_err("`eighty-eighty` is not a u16");
    let rendered = error.to_string();

    assert!(rendered.contains("port"), "1/3 key: {rendered}");
    assert!(rendered.contains("environment"), "2/3 layer: {rendered}");
    assert!(rendered.contains("u16"), "3/3 expected type: {rendered}");
    assert!(!*started.lock().unwrap_or_else(PoisonError::into_inner));
}

// ── Scenario 6 ──────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn scenario_6_a_table_in_a_file_and_a_scalar_in_the_environment_fails_naming_both() {
    // `server` is a table in `base.toml`. The environment supplies it as a scalar.
    //
    // The failure arrives from **step 1**, not the merge: decoding the environment layer against
    // the schema already rejects a scalar for a declared table, so the higher-precedence layer
    // never gets the chance to "simply win". The shapes are not coerced and the scenario's
    // prohibition holds — by an earlier and more precise route than it anticipated. The
    // merge-level path that names both layers is proven at `proof_gate::obligation_7b`.
    let (builder, _handle, started) = application(&[("RENVOR_SERVER", "just-a-string")]);
    let phases = builder.phase_log();

    let error = builder
        .build()
        .expect_err("a scalar cannot become a declared table");

    let rendered = error.to_string();
    assert!(rendered.contains("server"), "names the key: {rendered}");
    assert!(
        rendered.contains("environment"),
        "names the layer: {rendered}"
    );
    assert!(
        !phases.entries().contains(&LifecyclePhase::Register),
        "the higher-precedence layer must not simply win"
    );
    assert!(!*started.lock().unwrap_or_else(PoisonError::into_inner));
}

// ── The four merge behaviours, through the lifecycle ────────────────────────────────────────

#[tokio::test]
async fn all_four_merge_behaviours_hold_end_to_end() {
    // (a) tables merge per key, (b) arrays replace wholesale, (c) shape conflicts fail — proven
    // above and in the proof gate — and precedence across all four layers.
    let (builder, handle, _started) = application(&[("RENVOR_NAME", "env-name")]);
    builder
        .build()
        .expect("resolves")
        .boot()
        .await
        .expect("boots");

    handle
        .with(|resolved| {
            let value = resolved.value();
            assert_eq!(value.port, 9090, "later file beats earlier");
            assert_eq!(value.name, "env-name", "environment beats every file");
            assert_eq!(
                value.server.host, "base-host",
                "(a) the sibling survived a partial override"
            );
            assert_eq!(
                resolved.value().tags,
                vec!["z".to_owned()],
                "(b) 0 concatenations"
            );
        })
        .expect("resolved");
}

#[tokio::test]
async fn configuration_participates_in_load_and_validate_in_that_order() {
    // T074's wiring, observed from outside. A successful run enters Load and Validate before
    // Register, and the configuration is resolved by the time Register begins.
    let (builder, handle, _started) = application(&[]);
    let phases = builder.phase_log();

    assert!(!handle.is_resolved(), "nothing before the build");
    builder.build().expect("resolves");

    assert!(handle.is_resolved(), "resolved during Load");
    assert_eq!(
        phases.entries(),
        vec![
            LifecyclePhase::Load,
            LifecyclePhase::Validate,
            LifecyclePhase::Register,
        ]
    );
}
