//! The cache capability's typed configuration section (FR-011).
//!
//! # A section, its defaults, its caps, and the phase that refuses it
//!
//! [`CacheSection`](crate::config::CacheSection) is the shape an operator writes — under `[cache]` in a file, or as
//! `RENVOR_CACHE_*` in the environment — decoded by `renvor-config` against this type before any
//! merging, defaulted from [`DEFAULTS`](crate::config::DEFAULTS), and checked by [`CacheSection::settings_from`](crate::config::CacheSection::settings_from) against
//! the same caps the builders in [`crate::port`] and [`crate::valkey`] enforce. A bound above
//! its cap, a required key nobody supplied, a credential of the wrong shape, or a plaintext
//! endpoint the double opt-in refuses fails the kernel's **Validate** phase naming the key, the
//! constraint, and the layer that supplied the value (C-C8) — before any provider is
//! constructed, any task spawned, or any socket opened, because `ApplicationBuilder::build`
//! runs Load and Validate and returns before an `Application` exists.
//!
//! # Where the secret goes
//!
//! `password` is decoded as text, because the resolver decodes text; it is wrapped in a
//! [`Secret`](renvor_config::Secret) the moment the section becomes settings and is never rendered by this type's
//! `Debug`. The resolved configuration's own `Debug` prints keys and layers, never values.
//!
//! # The section is the whole schema, or one table of it
//!
//! [`CacheSection::source`](crate::config::CacheSection::source) makes the section a resolver of its own, so an application can keep
//! one resolver per capability with its own defaults, file, and environment prefix. An
//! application with one larger schema nests the section as a field and calls
//! [`CacheSection::settings_at`](crate::config::CacheSection::settings_at) with the field's key prefix, so the diagnostics still name
//! `cache.port` rather than `port`.

use std::time::Duration;

use renvor_config::{
    ConfigHandle, ConfigSchema, LayeredResolverBuilder, SchemaSource, Secret, SectionKeys, Table,
};
use renvor_core::KernelError;
use renvor_core::config_port::ResolvedConfig;
use renvor_core::error::context::Constraint;
use serde::Deserialize;

use crate::port::{
    CacheBounds, MAX_TTL_CAP, MAX_VALUE_BYTES_CAP, MIN_TTL, Namespace, OPERATION_TIMEOUT_CAP,
};
use crate::valkey::{
    MAX_CONNECT_TIMEOUT, MAX_RECONNECT_ATTEMPTS, MAX_RECONNECT_DELAY, ReconnectBounds,
    ValkeyCredentials, ValkeyEndpoint, ValkeySettings,
};

/// The defaults every key but `host` and `namespace` carries. Those two have none: a cache with
/// no address or no namespace is not a cache with a default, it is a missing configuration.
pub const DEFAULTS: &str = r#"
port = 6379
tls = true
allow_insecure_loopback = false
database = 0
max_value_bytes = 1048576
max_ttl_secs = 604800
operation_timeout_ms = 2000
reconnect_attempts = 6
reconnect_min_delay_ms = 100
reconnect_max_delay_ms = 5000
connect_timeout_ms = 5000
"#;

/// The `[cache]` section.
///
/// Every field is public so an application that nests the section in its own schema can read it;
/// the settings are built through [`Self::settings_from`] or [`Self::settings_at`], which are
/// where the caps are enforced.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheSection {
    /// The server's host: a lowercase DNS name or an IP literal. Required.
    pub host: String,
    /// The server's port.
    pub port: u16,
    /// TLS with the native root store (the default), or plaintext (loopback only, with the flag).
    pub tls: bool,
    /// The opt-in for a plaintext session to a loopback host (C-C7).
    pub allow_insecure_loopback: bool,
    /// The logical database.
    pub database: u8,
    /// The ACL username, if the server uses one.
    pub username: Option<String>,
    /// The password. Wrapped in a [`Secret`] at the boundary; never rendered.
    pub password: Option<String>,
    /// The namespace every key is stored under. Required.
    pub namespace: String,
    /// The ceiling on a value's size, in bytes.
    pub max_value_bytes: usize,
    /// The ceiling on a lifetime, in seconds.
    pub max_ttl_secs: u64,
    /// The per-operation timeout, in milliseconds.
    pub operation_timeout_ms: u64,
    /// The most reconnection attempts before the manager gives up.
    pub reconnect_attempts: usize,
    /// The first reconnection delay, in milliseconds.
    pub reconnect_min_delay_ms: u64,
    /// The longest reconnection delay, in milliseconds.
    pub reconnect_max_delay_ms: u64,
    /// The connection timeout, in milliseconds.
    pub connect_timeout_ms: u64,
}

/// Every key, never the password.
impl core::fmt::Debug for CacheSection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CacheSection")
            .field("tls", &self.tls)
            .field("allow_insecure_loopback", &self.allow_insecure_loopback)
            .field("database", &self.database)
            .field("authenticated", &self.password.is_some())
            .field("namespace", &self.namespace)
            .field("max_value_bytes", &self.max_value_bytes)
            .field("max_ttl_secs", &self.max_ttl_secs)
            .field("operation_timeout_ms", &self.operation_timeout_ms)
            .finish_non_exhaustive()
    }
}

/// The all-optional form one source decodes into (see `renvor_config::ConfigSchema`).
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialCacheSection {
    host: Option<String>,
    port: Option<u16>,
    tls: Option<bool>,
    allow_insecure_loopback: Option<bool>,
    database: Option<u8>,
    username: Option<String>,
    password: Option<String>,
    namespace: Option<String>,
    max_value_bytes: Option<usize>,
    max_ttl_secs: Option<u64>,
    operation_timeout_ms: Option<u64>,
    reconnect_attempts: Option<usize>,
    reconnect_min_delay_ms: Option<u64>,
    reconnect_max_delay_ms: Option<u64>,
    connect_timeout_ms: Option<u64>,
}

impl ConfigSchema for CacheSection {
    type Partial = PartialCacheSection;
}

impl CacheSection {
    /// The defaults as a table for `LayeredResolverBuilder::with_defaults`.
    ///
    /// # Panics
    ///
    /// Never: [`DEFAULTS`] is a constant this module's tests parse.
    #[must_use]
    pub fn defaults() -> Table {
        DEFAULTS
            .parse()
            .expect("the defaults constant is valid TOML")
    }

    /// A lifecycle source for this section alone: `builder`'s layers, this section's defaults
    /// beneath them, and the validator that runs at Validate.
    ///
    /// The builder's own defaults, if it set any, are replaced by [`Self::defaults`]; pass the
    /// file and environment layers only.
    #[must_use]
    pub fn source(name: &str, builder: LayeredResolverBuilder) -> SchemaSource<Self> {
        let resolver = builder.with_defaults(Self::defaults()).build::<Self>();
        SchemaSource::new(name, resolver)
            .with_validator(|resolved| Self::settings_from(resolved).map(|_| ()))
    }

    /// The settings a resolved section of its own describes, or the first rule it breaks.
    ///
    /// # Errors
    ///
    /// [`KernelError::Configuration`] naming the key, the constraint, and the layer.
    pub fn settings_from(resolved: &ResolvedConfig<Self>) -> Result<ValkeySettings, KernelError> {
        resolved.value().settings_at("", resolved)
    }

    /// The settings this section describes when it is the table `prefix` of a larger resolved
    /// schema (`prefix = "cache"` names `cache.port`), or the first rule it breaks.
    ///
    /// # Errors
    ///
    /// [`KernelError::Configuration`] naming the key, the constraint, and the layer.
    pub fn settings_at<T>(
        &self,
        prefix: &str,
        resolved: &ResolvedConfig<T>,
    ) -> Result<ValkeySettings, KernelError> {
        let keys = SectionKeys::new(prefix, resolved);

        // The endpoint: the host's shape is the endpoint constructor's rule.
        let endpoint = if self.tls {
            ValkeyEndpoint::tls(&self.host, self.port)
        } else {
            ValkeyEndpoint::plaintext(&self.host, self.port)
        }
        .map_err(|_| {
            let name = if self.port == 0 { "port" } else { "host" };
            keys.refuse(
                name,
                "a lowercase DNS name or IP literal, and a non-zero port",
                &Constraint::Rule(
                    "the host must be a lowercase DNS name or an IP literal, and the port non-zero",
                ),
            )
        })?
        .with_database(self.database);

        // The double opt-in, at Validate rather than at Boot, so the refusal names the key and
        // the layer that asked for plaintext (C-C7).
        let plaintext_permitted = self.allow_insecure_loopback && endpoint.is_loopback();
        if !self.tls && !plaintext_permitted {
            return Err(keys.refuse(
                "tls",
                "true, or false with a loopback host and allow_insecure_loopback = true",
                &Constraint::Rule(
                    "a plaintext session is accepted only to a loopback host and only with \
                     allow_insecure_loopback = true; use TLS for anything else",
                ),
            ));
        }

        // The credential: present-and-empty is not absent (C-C11), and a username has a shape.
        let credentials = match (&self.username, &self.password) {
            (None, None) => None,
            (Some(_), None) => {
                return Err(keys.refuse(
                    "password",
                    "a password beside the username",
                    &Constraint::Missing,
                ));
            }
            (username, Some(password)) => {
                if password.is_empty() {
                    return Err(keys.refuse(
                        "password",
                        "a non-empty password",
                        &Constraint::TooShort { minimum: 1 },
                    ));
                }
                let credentials = ValkeyCredentials::password(Secret::new(
                    keys.key("password"),
                    password.clone(),
                ));
                Some(match username {
                    Some(username) => credentials.with_username(username).map_err(|_| {
                        keys.refuse(
                            "username",
                            "1 to 256 bytes with no control character or whitespace",
                            &Constraint::Rule(
                                "must be 1 to 256 bytes with no control character or whitespace",
                            ),
                        )
                    })?,
                    None => credentials,
                })
            }
        };

        let namespace = Namespace::new(&self.namespace).map_err(|_| {
            keys.refuse(
                "namespace",
                "1 to 64 characters of [a-z0-9_.-]",
                &Constraint::Rule("must be 1 to 64 characters of [a-z0-9_.-]"),
            )
        })?;

        // The bounds, each against its cap, with the range stated in the diagnostic.
        keys.range(
            "max_value_bytes",
            self.max_value_bytes as u128,
            1,
            MAX_VALUE_BYTES_CAP as u128,
        )?;
        keys.range(
            "max_ttl_secs",
            u128::from(self.max_ttl_secs),
            u128::from(MIN_TTL.as_secs()),
            u128::from(MAX_TTL_CAP.as_secs()),
        )?;
        keys.range(
            "operation_timeout_ms",
            u128::from(self.operation_timeout_ms),
            1,
            OPERATION_TIMEOUT_CAP.as_millis(),
        )?;
        let bounds = CacheBounds::new()
            .with_max_value_bytes(self.max_value_bytes)
            .and_then(|bounds| bounds.with_max_ttl(Duration::from_secs(self.max_ttl_secs)))
            .and_then(|bounds| {
                bounds.with_operation_timeout(Duration::from_millis(self.operation_timeout_ms))
            })
            .map_err(|_| {
                keys.refuse(
                    "max_value_bytes",
                    "bounds within their caps",
                    &Constraint::Rule("the bounds were refused by the cache"),
                )
            })?;

        keys.range(
            "reconnect_attempts",
            self.reconnect_attempts as u128,
            1,
            MAX_RECONNECT_ATTEMPTS as u128,
        )?;
        keys.range(
            "reconnect_min_delay_ms",
            u128::from(self.reconnect_min_delay_ms),
            1,
            u128::from(self.reconnect_max_delay_ms).max(1),
        )?;
        keys.range(
            "reconnect_max_delay_ms",
            u128::from(self.reconnect_max_delay_ms),
            u128::from(self.reconnect_min_delay_ms),
            MAX_RECONNECT_DELAY.as_millis(),
        )?;
        keys.range(
            "connect_timeout_ms",
            u128::from(self.connect_timeout_ms),
            1,
            MAX_CONNECT_TIMEOUT.as_millis(),
        )?;
        let reconnect = ReconnectBounds::with(
            self.reconnect_attempts,
            Duration::from_millis(self.reconnect_min_delay_ms),
            Duration::from_millis(self.reconnect_max_delay_ms),
            Duration::from_millis(self.connect_timeout_ms),
        )
        .map_err(|_| {
            keys.refuse(
                "reconnect_attempts",
                "reconnection bounds within their caps",
                &Constraint::Rule("the reconnection bounds were refused by the cache"),
            )
        })?;

        Ok(
            ValkeySettings::new(endpoint, credentials, namespace, bounds)
                .with_reconnect(reconnect)
                .with_allow_insecure_loopback(self.allow_insecure_loopback),
        )
    }
}

/// Reads the settings a validated section resolved to. For a provider constructed from a
/// [`ConfigHandle`]: Validate already refused anything this could refuse, so a failure here is
/// the handle being read before `build`.
///
/// # Errors
///
/// [`KernelError::Configuration`].
pub fn settings_from_handle(
    handle: &ConfigHandle<CacheSection>,
) -> Result<ValkeySettings, KernelError> {
    handle.with(CacheSection::settings_from)?
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use renvor_config::{FileLayer, LayeredResolverBuilder};
    use renvor_core::config_port::{ConfigResolver as _, ConfigSource as _};
    use renvor_core::provider::ProviderId;
    use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};
    use renvor_core::{ApplicationBuilder, ErrorCategory};

    use super::{CacheSection, DEFAULTS};

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn builder(pairs: &[(&str, &str)]) -> LayeredResolverBuilder {
        let mut all = vec![
            ("RENVOR_CACHE_HOST", "cache.internal"),
            ("RENVOR_CACHE_NAMESPACE", "app"),
        ];
        all.extend_from_slice(pairs);
        LayeredResolverBuilder::new().with_environment_map("RENVOR_CACHE_", env(&all))
    }

    /// A provider that counts how often the kernel reached its Boot.
    struct Counting(Arc<AtomicU32>, ProviderId);

    impl Provider for Counting {
        fn id(&self) -> &ProviderId {
            &self.1
        }
        fn provides(&self) -> &[CapabilityId] {
            &[]
        }
        fn initialise<'a>(&'a self, _: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(()) })
        }
        fn stop(&self) -> ProviderFuture<'_> {
            Box::pin(async { Ok(()) })
        }
    }

    /// Builds an application from the section's source and one counting provider; returns the
    /// build error and how many times a provider was initialised.
    fn build(pairs: &[(&str, &str)]) -> (Result<(), String>, u32) {
        let count = Arc::new(AtomicU32::new(0));
        let source = CacheSection::source("cache", builder(pairs));
        let outcome = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(Counting(
                Arc::clone(&count),
                ProviderId::new("counting"),
            )))
            .build();
        let outcome = match outcome {
            Ok(_) => Ok(()),
            Err(error) => {
                let kernel = error.kernel().expect("a kernel error");
                assert_eq!(kernel.category(), ErrorCategory::Configuration);
                Err(kernel.to_string())
            }
        };
        (outcome, count.load(Ordering::SeqCst))
    }

    #[test]
    fn the_defaults_parse_and_a_complete_section_becomes_settings() {
        let resolved = CacheSection::source(
            "cache",
            builder(&[
                ("RENVOR_CACHE_TLS", "false"),
                ("RENVOR_CACHE_HOST", "127.0.0.1"),
                ("RENVOR_CACHE_ALLOW_INSECURE_LOOPBACK", "true"),
                ("RENVOR_CACHE_PASSWORD", "hunter2CanaryDoNotLeak"),
            ]),
        );
        resolved.load().expect("resolves");
        resolved.validate().expect("a complete section validates");
        let settings = resolved
            .handle()
            .with(CacheSection::settings_from)
            .expect("resolved")
            .expect("valid");
        assert_eq!(settings.endpoint().host(), "127.0.0.1");
        assert_eq!(settings.endpoint().port(), 6379);
        assert!(!settings.endpoint().is_tls());
        assert_eq!(settings.bounds().max_value_bytes(), 1024 * 1024);
        assert_eq!(settings.bounds().max_ttl().as_secs(), 7 * 24 * 60 * 60);
        assert!(settings.validate().is_ok());
        let rendered = format!("{settings:?}");
        assert!(!rendered.contains("hunter2"), "the password leaked");
        assert!(!DEFAULTS.contains("host"), "host has no default");
    }

    #[test]
    fn a_bound_over_its_cap_fails_validate_naming_key_constraint_and_layer_before_any_boot() {
        // FR-011: the cap is a Validate failure, and 0 providers were initialised.
        let (outcome, booted) = build(&[("RENVOR_CACHE_MAX_TTL_SECS", "9999999999")]);
        let rendered = outcome.expect_err("30 days is the cap");
        assert!(rendered.contains("`max_ttl_secs`"), "the key is not named");
        assert!(rendered.contains("environment"), "the layer is not named");
        assert!(
            rendered.contains("between 1 and 2592000"),
            "the constraint is not named"
        );
        assert_eq!(
            booted, 0,
            "a provider was initialised on invalid configuration"
        );
        // The same for a zero timeout and a reconnect count over its cap.
        let (outcome, _) = build(&[("RENVOR_CACHE_OPERATION_TIMEOUT_MS", "0")]);
        let rendered = outcome.expect_err("zero is below the floor");
        assert!(
            rendered.contains("`operation_timeout_ms`"),
            "the key is not named"
        );
        let (outcome, _) = build(&[("RENVOR_CACHE_RECONNECT_ATTEMPTS", "101")]);
        assert!(
            outcome
                .expect_err("100 is the cap")
                .contains("`reconnect_attempts`")
        );
    }

    #[test]
    fn a_missing_required_key_fails_before_any_boot() {
        let count = Arc::new(AtomicU32::new(0));
        let source = CacheSection::source(
            "cache",
            LayeredResolverBuilder::new()
                .with_environment_map("RENVOR_CACHE_", env(&[("RENVOR_CACHE_NAMESPACE", "app")])),
        );
        let outcome = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(Counting(
                Arc::clone(&count),
                ProviderId::new("counting"),
            )))
            .build();
        let Err(error) = outcome else {
            panic!("host has no default and was not supplied, yet the build succeeded");
        };
        let rendered = error.kernel().expect("a kernel error").to_string();
        assert!(rendered.contains("host"), "the missing key is not named");
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_malformed_credential_fails_validate_naming_the_key_and_never_the_value() {
        let (outcome, booted) = build(&[
            ("RENVOR_CACHE_USERNAME", "app user"),
            ("RENVOR_CACHE_PASSWORD", "hunter2CanaryDoNotLeak"),
        ]);
        let rendered = outcome.expect_err("a username with whitespace");
        assert!(rendered.contains("`username`"), "the key is not named");
        assert!(rendered.contains("environment"), "the layer is not named");
        assert!(
            !rendered.contains("hunter2") && !rendered.contains("app user"),
            "the value was rendered"
        );
        assert_eq!(booted, 0);
        // Present-and-empty is not absent (C-C11): an empty password is refused by name.
        let (outcome, _) = build(&[("RENVOR_CACHE_PASSWORD", "")]);
        let rendered = outcome.expect_err("an empty password");
        assert!(rendered.contains("`password`"), "the key is not named");
        // A username needs a password beside it.
        let (outcome, _) = build(&[("RENVOR_CACHE_USERNAME", "app")]);
        assert!(
            outcome
                .expect_err("a username without a password")
                .contains("`password`")
        );
    }

    #[test]
    fn plaintext_off_loopback_is_refused_at_validate_naming_tls_and_its_layer() {
        // The layer is a FILE here, so the diagnostic names the file rather than the environment.
        let path =
            std::env::temp_dir().join(format!("renvor-cache-section-{}.toml", std::process::id()));
        std::fs::write(
            &path,
            "host = \"cache.internal\"\nnamespace = \"app\"\ntls = false\nallow_insecure_loopback = true\n",
        )
        .expect("writes");
        let count = Arc::new(AtomicU32::new(0));
        let source = CacheSection::source(
            "cache",
            LayeredResolverBuilder::new().with_file(FileLayer::required(&path)),
        );
        let outcome = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(Counting(
                Arc::clone(&count),
                ProviderId::new("counting"),
            )))
            .build();
        let _ = std::fs::remove_file(&path);
        let Err(error) = outcome else {
            panic!("plaintext to a non-loopback host was accepted");
        };
        let rendered = error.kernel().expect("a kernel error").to_string();
        assert!(rendered.contains("`tls`"), "the key is not named");
        assert!(rendered.contains("loopback"), "the rule is not named");
        assert!(
            rendered.contains("renvor-cache-section"),
            "the file layer is not named"
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_nested_section_names_its_prefix() {
        // An application with one schema nests the section; the diagnostic must still say
        // which table the key is in.
        #[derive(serde::Deserialize)]
        struct App {
            cache: CacheSection,
        }
        #[allow(dead_code)]
        #[derive(Default, serde::Deserialize)]
        struct PartialApp {
            cache: Option<super::PartialCacheSection>,
        }
        impl renvor_config::ConfigSchema for App {
            type Partial = PartialApp;
        }
        let mut defaults = renvor_config::Table::new();
        defaults.insert("cache".to_owned(), CacheSection::defaults().into());
        let resolved = LayeredResolverBuilder::new()
            .with_defaults(defaults)
            .with_environment_map(
                "RENVOR_",
                env(&[
                    ("RENVOR_CACHE__HOST", "cache.internal"),
                    ("RENVOR_CACHE__NAMESPACE", "app"),
                    ("RENVOR_CACHE__MAX_VALUE_BYTES", "0"),
                ]),
            )
            .build::<App>()
            .resolve()
            .expect("decodes");
        let error = resolved
            .value()
            .cache
            .settings_at("cache", &resolved)
            .expect_err("zero bytes is below the floor");
        let rendered = error.to_string();
        assert!(
            rendered.contains("`cache.max_value_bytes`"),
            "the prefixed key is not named"
        );
        assert!(rendered.contains("environment"), "the layer is not named");
    }
}
