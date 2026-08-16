//! Layered configuration with per-key attribution, and a secret that is redacted everywhere.
//!
//! ```text
//! cargo run --example configuration
//! ```
//!
//! # What this demonstrates
//!
//! Three layers — built-in defaults, a TOML file, and environment variables — combined in
//! precedence order, with **every resolved key reporting which layer supplied it**. That report is
//! the reason this adapter exists: the candidate crate the project evaluated first could not
//! produce it, because its merge discards which side a value came from.
//!
//! The password is a `Secret`, and the example prints it deliberately to show that printing it
//! yields a placeholder. Reaching the value takes one conspicuously-named method, which is the
//! only way through — there is no `Deref` and no `Into`.
//!
//! # The two structs
//!
//! An author writes the schema **and** an all-optional form of it. That is a real cost, stated
//! plainly: decoding each source *before* merging needs a target that a partial source can decode
//! into, and Renvor has no derive macro to generate one. See `ConfigSchema`'s documentation.

use std::collections::BTreeMap;
use std::io::Write as _;

use renvor::config::{ConfigSchema, FileLayer, LayeredResolverBuilder, Secret};
use renvor::kernel::config_port::ConfigResolver as _;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Settings {
    host: String,
    port: u16,
    password: String,
    features: Vec<String>,
}

/// The all-optional form. Nothing reads its fields back — its job is to prove a source fits the
/// schema before any merging — so `dead_code` fires on every one of them. That is the shape, not
/// an oversight.
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialSettings {
    host: Option<String>,
    port: Option<u16>,
    password: Option<String>,
    features: Option<Vec<String>>,
}

impl ConfigSchema for Settings {
    type Partial = PartialSettings;
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A file layer, written next to the build output so the example needs no fixtures on disk.
    let path = std::env::temp_dir().join("renvor-example-configuration.toml");
    let mut file = std::fs::File::create(&path)?;
    file.write_all(b"port = 8080\nfeatures = [\"tracing\", \"metrics\"]\n")?;

    let defaults = r#"
host = "localhost"
port = 80
password = "unset"
features = []
"#
    .parse()?;

    // The environment is a **layer with a precedence position**, not a per-field annotation.
    // Supplied explicitly rather than read from the process, so the example is reproducible.
    let mut environment = BTreeMap::new();
    environment.insert("RENVOR_HOST".to_owned(), "example.internal".to_owned());
    environment.insert("RENVOR_PASSWORD".to_owned(), "hunter2".to_owned());

    let resolved = LayeredResolverBuilder::new()
        .with_defaults(defaults)
        .with_file(FileLayer::required(&path))
        .with_environment_map("RENVOR_", environment)
        .build::<Settings>()
        .resolve()?;

    println!("resolved configuration");
    println!("  host     : {}", resolved.value().host);
    println!("  port     : {}", resolved.value().port);
    println!("  features : {:?}", resolved.value().features);
    println!();

    println!("where each key came from");
    for (key, attribution) in resolved.attributions() {
        println!("  {key:<10} <- {}", attribution.layer.label());
    }
    println!();

    // The password is marked secret at the boundary. Every output form below renders a
    // placeholder; the value is reachable only through `expose`.
    let password = Secret::new("password", resolved.value().password.clone());

    println!("the secret, printed on purpose");
    println!("  Display  : {password}");
    println!("  Debug    : {password:?}");
    println!("  in a msg : the password is {password}");
    println!("  expose() : {} characters", password.expose().len());

    assert!(!format!("{password}{password:?}").contains("hunter2"));

    std::fs::remove_file(&path)?;
    Ok(())
}
