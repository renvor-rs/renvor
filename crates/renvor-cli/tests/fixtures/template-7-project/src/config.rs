//! Configuration: one typed section per concern, each a lifecycle source the kernel validates
//! before any provider constructs.
//!
//! # Where values come from
//!
//! Each section reads, lowest precedence first: its built-in defaults, its optional file under
//! `config/<section>.toml`, then the environment under `RENVOR_<SECTION>_…`. The environment
//! always wins, which is where every secret lives: a key or a password is never written to a
//! file this project ships, and `.env.example` names each one with an empty value.
//!
//! # A section that fails validation stops the application at Validate
//!
//! Naming the key, the constraint, and the layer that supplied the value — before a socket is
//! opened or a task spawned. There is no fallback to a default for a value that was supplied and
//! wrong.

use renvor_config::{ConfigSchema, FileLayer, LayeredResolverBuilder, SchemaSource, Table};
use serde::Deserialize;

/// A builder carrying the two layers every section shares: its file, then the environment.
pub fn layers(section: &str) -> LayeredResolverBuilder {
    LayeredResolverBuilder::new()
        .with_file(FileLayer::optional(format!("config/{section}.toml")))
        .with_environment(format!("RENVOR_{}_", section.to_ascii_uppercase()))
}

/// The `[http]` section: where the server listens and which host it answers to.
#[derive(Debug, Deserialize)]
pub struct HttpSection {
    /// The bind address, `host:port`.
    pub address: String,
    /// The local development domain the host policy allows, beside loopback.
    pub local_domain: String,
}

/// The all-optional form `renvor-config` decodes each layer into before merging.
#[allow(dead_code, reason = "decoded, never read back; see `ConfigSchema`")]
#[derive(Debug, Default, Deserialize)]
pub struct PartialHttpSection {
    address: Option<String>,
    local_domain: Option<String>,
}

impl ConfigSchema for HttpSection {
    type Partial = PartialHttpSection;
}

/// The `[http]` defaults.
fn http_defaults() -> Table {
    String::from("address = \"127.0.0.1:3000\"\nlocal_domain = \"legacy-api.test\"\n")
        .parse()
        .expect("the defaults are valid TOML")
}

/// The `[http]` source.
pub fn http_source() -> SchemaSource<HttpSection> {
    let resolver = layers("http")
        .with_defaults(http_defaults())
        .build::<HttpSection>();
    SchemaSource::new("http", resolver).with_validator(|resolved| {
        let address = &resolved.value().address;
        address
            .parse::<std::net::SocketAddr>()
            .map(|_| ())
            .map_err(|_| {
                renvor::kernel::error::context::configuration(
                    "address",
                    renvor_config::layer_of(resolved, "address").as_str(),
                    "host:port",
                    &renvor::kernel::error::context::Constraint::Rule(
                        "must be a socket address such as 127.0.0.1:3000",
                    ),
                )
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_http_defaults_parse_and_name_the_local_domain() {
        // Bound to a name first so the line's width does not depend on the project's name.
        let expected = "legacy-api.test";
        let defaults = http_defaults();
        assert_eq!(defaults["local_domain"].as_str(), Some(expected));
    }
}
