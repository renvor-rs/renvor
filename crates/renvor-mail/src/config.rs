//! The mail capability's typed configuration section (FR-011).
//!
//! # A section, its defaults, its caps, and the phase that refuses it
//!
//! [`MailSection`](crate::config::MailSection) is the shape an operator writes — under `[mail]`
//! in a file, or as `RENVOR_MAIL_*` in the environment — decoded by `renvor-config` against this
//! type before any merging, defaulted from [`DEFAULTS`](crate::config::DEFAULTS), and checked
//! by [`MailSection::settings_from`](crate::config::MailSection::settings_from) against the caps
//! [`crate::smtp`] enforces. A bound above its cap, a required key nobody supplied, a credential
//! of the wrong shape, or a plaintext endpoint the double opt-in refuses fails the kernel's
//! **Validate** phase naming the key, the constraint, and the layer that supplied the value
//! (C-C8) — before any provider is constructed, any task spawned, or any socket opened.
//!
//! # Where the secret goes
//!
//! `password` is decoded as text, because the resolver decodes text; it is wrapped in a
//! [`Secret`](renvor_config::Secret) the moment the section becomes settings and is never
//! rendered by this type's `Debug`.

use std::time::Duration;

use renvor_config::{
    ConfigHandle, ConfigSchema, LayeredResolverBuilder, SchemaSource, Secret, SectionKeys, Table,
};
use renvor_core::KernelError;
use renvor_core::config_port::ResolvedConfig;
use renvor_core::error::context::Constraint;
use serde::Deserialize;

use crate::smtp::{
    MAX_POOL_SIZE, Security, SmtpCredentials, SmtpEndpoint, SmtpSettings, TIMEOUT_RANGE,
};

/// The defaults every key but `host`, `hello_name`, and `sender_domain` carries.
pub const DEFAULTS: &str = r#"
security = "starttls"
allow_insecure_loopback = false
timeout_secs = 30
pool_size = 4
idle_timeout_secs = 60
"#;

/// The `[mail]` section.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailSection {
    /// The submission server's host: a lowercase DNS name or an IP literal. Required.
    pub host: String,
    /// The port; the security's default (465, 587, or 25) when absent.
    pub port: Option<u16>,
    /// `implicit_tls`, `starttls` (the default), or `plaintext` (loopback only, with the flag).
    pub security: String,
    /// The opt-in for a plaintext session to a loopback host (C-C7, FR-047).
    pub allow_insecure_loopback: bool,
    /// The username, when the server authenticates.
    pub username: Option<String>,
    /// The password. Wrapped in a [`Secret`] at the boundary; never rendered.
    pub password: Option<String>,
    /// The name this client announces in `EHLO`. Required.
    pub hello_name: String,
    /// The domain message identifiers are generated over. Required.
    pub sender_domain: String,
    /// The bound on one SMTP operation, in seconds.
    pub timeout_secs: u64,
    /// The connection pool size.
    pub pool_size: u32,
    /// The idle timeout of a pooled connection, in seconds.
    pub idle_timeout_secs: u64,
}

/// Every key but the credential and the address.
impl core::fmt::Debug for MailSection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MailSection")
            .field("security", &self.security)
            .field("allow_insecure_loopback", &self.allow_insecure_loopback)
            .field("authenticated", &self.password.is_some())
            .field("hello_name", &self.hello_name)
            .field("sender_domain", &self.sender_domain)
            .field("timeout_secs", &self.timeout_secs)
            .field("pool_size", &self.pool_size)
            .finish_non_exhaustive()
    }
}

/// The all-optional form one source decodes into (see `renvor_config::ConfigSchema`).
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialMailSection {
    host: Option<String>,
    port: Option<u16>,
    security: Option<String>,
    allow_insecure_loopback: Option<bool>,
    username: Option<String>,
    password: Option<String>,
    hello_name: Option<String>,
    sender_domain: Option<String>,
    timeout_secs: Option<u64>,
    pool_size: Option<u32>,
    idle_timeout_secs: Option<u64>,
}

impl ConfigSchema for MailSection {
    type Partial = PartialMailSection;
}

impl MailSection {
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

    /// A lifecycle source for this section alone, with this section's defaults beneath
    /// `builder`'s layers and the validator that runs at Validate.
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
    pub fn settings_from(resolved: &ResolvedConfig<Self>) -> Result<SmtpSettings, KernelError> {
        resolved.value().settings_at("", resolved)
    }

    /// The settings this section describes as the table `prefix` of a larger resolved schema.
    ///
    /// # Errors
    ///
    /// [`KernelError::Configuration`] naming the key, the constraint, and the layer.
    pub fn settings_at<T>(
        &self,
        prefix: &str,
        resolved: &ResolvedConfig<T>,
    ) -> Result<SmtpSettings, KernelError> {
        let keys = SectionKeys::new(prefix, resolved);

        let security = match self.security.as_str() {
            "implicit_tls" => Security::ImplicitTls,
            "starttls" => Security::StartTls,
            "plaintext" => Security::PlaintextLoopback,
            _ => {
                return Err(keys.rule(
                    "security",
                    "one of implicit_tls, starttls, plaintext",
                    "must be one of implicit_tls, starttls, or plaintext",
                ));
            }
        };
        let mut endpoint = SmtpEndpoint::new(&self.host, security).map_err(|_| {
            keys.rule(
                "host",
                "a lowercase DNS name or IP literal",
                "must be a lowercase DNS name or an IP literal",
            )
        })?;
        if let Some(port) = self.port {
            endpoint = endpoint
                .with_port(port)
                .map_err(|_| keys.range("port", 0, 1, 65_535).unwrap_err())?;
        }

        // The double opt-in at Validate, naming the key and the layer that asked for plaintext.
        let plaintext_permitted = self.allow_insecure_loopback && endpoint.is_loopback();
        if security == Security::PlaintextLoopback && !plaintext_permitted {
            return Err(keys.rule(
                "security",
                "plaintext only with a loopback host and allow_insecure_loopback = true",
                "a plaintext session is accepted only to a loopback host and only with \
                 allow_insecure_loopback = true; use starttls or implicit_tls for anything else",
            ));
        }

        let credentials = match (&self.username, &self.password) {
            (None, None) => None,
            (Some(_), None) => {
                return Err(keys.refuse(
                    "password",
                    "a password beside the username",
                    &Constraint::Missing,
                ));
            }
            (None, Some(_)) => {
                return Err(keys.refuse(
                    "username",
                    "a username beside the password",
                    &Constraint::Missing,
                ));
            }
            (Some(username), Some(password)) => {
                if password.is_empty() {
                    return Err(keys.refuse(
                        "password",
                        "a non-empty password",
                        &Constraint::TooShort { minimum: 1 },
                    ));
                }
                Some(
                    SmtpCredentials::new(
                        username,
                        Secret::new(keys.key("password"), password.clone()),
                    )
                    .map_err(|_| {
                        keys.rule(
                            "username",
                            "1 to 256 bytes with no control character or whitespace",
                            "must be 1 to 256 bytes with no control character or whitespace",
                        )
                    })?,
                )
            }
        };

        keys.range(
            "timeout_secs",
            u128::from(self.timeout_secs),
            u128::from(TIMEOUT_RANGE.0.as_secs()),
            u128::from(TIMEOUT_RANGE.1.as_secs()),
        )?;
        keys.range(
            "pool_size",
            u128::from(self.pool_size),
            1,
            u128::from(MAX_POOL_SIZE),
        )?;
        keys.range(
            "idle_timeout_secs",
            u128::from(self.idle_timeout_secs),
            1,
            3600,
        )?;

        let settings =
            SmtpSettings::new(endpoint, credentials, &self.hello_name, &self.sender_domain)
                .map_err(|_| {
                    let name = if crate::smtp::valid_name(&self.hello_name) {
                        "sender_domain"
                    } else {
                        "hello_name"
                    };
                    keys.rule(
                        name,
                        "a lowercase DNS name",
                        "must be a lowercase DNS name of at most 253 bytes",
                    )
                })?
                .with_allow_insecure_loopback(self.allow_insecure_loopback)
                .with_timeout(Duration::from_secs(self.timeout_secs))
                .and_then(|settings| settings.with_pool_size(self.pool_size))
                .and_then(|settings| {
                    settings.with_idle_timeout(Duration::from_secs(self.idle_timeout_secs))
                })
                .map_err(|_| {
                    keys.rule(
                        "timeout_secs",
                        "bounds within their caps",
                        "the bounds were refused by the transport",
                    )
                })?;
        Ok(settings)
    }
}

/// Reads the settings a validated section resolved to. Validate already refused anything this
/// could refuse, so a failure here is the handle being read before `build`.
///
/// # Errors
///
/// [`KernelError::Configuration`].
pub fn settings_from_handle(
    handle: &ConfigHandle<MailSection>,
) -> Result<SmtpSettings, KernelError> {
    handle.with(MailSection::settings_from)?
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use renvor_config::LayeredResolverBuilder;
    use renvor_core::config_port::ConfigSource as _;
    use renvor_core::provider::ProviderId;
    use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};
    use renvor_core::{ApplicationBuilder, ErrorCategory};

    use super::MailSection;
    use crate::smtp::Security;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn builder(pairs: &[(&str, &str)]) -> LayeredResolverBuilder {
        let mut all = vec![
            ("RENVOR_MAIL_HOST", "relay.example.test"),
            ("RENVOR_MAIL_HELLO_NAME", "app.example.test"),
            ("RENVOR_MAIL_SENDER_DOMAIN", "mail.example.test"),
        ];
        all.extend_from_slice(pairs);
        LayeredResolverBuilder::new().with_environment_map("RENVOR_MAIL_", env(&all))
    }

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

    fn build(pairs: &[(&str, &str)]) -> (Result<(), String>, u32) {
        let count = Arc::new(AtomicU32::new(0));
        let source = MailSection::source("mail", builder(pairs));
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
    fn a_complete_section_becomes_settings_with_the_securitys_default_port() {
        let source =
            MailSection::source("mail", builder(&[("RENVOR_MAIL_SECURITY", "implicit_tls")]));
        source.load().expect("resolves");
        source.validate().expect("validates");
        let settings = source
            .handle()
            .with(MailSection::settings_from)
            .expect("resolved")
            .expect("valid");
        assert_eq!(settings.endpoint().security(), Security::ImplicitTls);
        assert_eq!(settings.endpoint().port(), 465);
        assert_eq!(settings.timeout().as_secs(), 30);
        let source = MailSection::source(
            "mail",
            builder(&[
                ("RENVOR_MAIL_PORT", "2525"),
                ("RENVOR_MAIL_USERNAME", "app"),
                ("RENVOR_MAIL_PASSWORD", "hunter2CanaryDoNotLeak"),
            ]),
        );
        source.load().expect("resolves");
        let settings = source
            .handle()
            .with(MailSection::settings_from)
            .expect("resolved")
            .expect("valid");
        assert_eq!(settings.endpoint().port(), 2525);
        assert!(!format!("{settings:?}").contains("hunter2"));
    }

    #[test]
    fn a_bound_over_its_cap_fails_validate_naming_key_constraint_and_layer_before_any_boot() {
        let (outcome, booted) = build(&[("RENVOR_MAIL_TIMEOUT_SECS", "301")]);
        let rendered = outcome.expect_err("300 s is the cap");
        assert!(rendered.contains("`timeout_secs`"), "the key is not named");
        assert!(rendered.contains("environment"), "the layer is not named");
        assert!(
            rendered.contains("between 1 and 300"),
            "the constraint is not named"
        );
        assert_eq!(booted, 0);
        let (outcome, _) = build(&[("RENVOR_MAIL_POOL_SIZE", "65")]);
        assert!(outcome.expect_err("64 is the cap").contains("`pool_size`"));
        let (outcome, _) = build(&[("RENVOR_MAIL_SECURITY", "tls")]);
        assert!(outcome.expect_err("not a security").contains("`security`"));
    }

    #[test]
    fn a_missing_required_key_fails_before_any_boot() {
        let count = Arc::new(AtomicU32::new(0));
        let source = MailSection::source(
            "mail",
            LayeredResolverBuilder::new().with_environment_map(
                "RENVOR_MAIL_",
                env(&[("RENVOR_MAIL_HOST", "relay.example.test")]),
            ),
        );
        let outcome = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(Counting(
                Arc::clone(&count),
                ProviderId::new("counting"),
            )))
            .build();
        let Err(error) = outcome else {
            panic!("hello_name has no default and was not supplied, yet the build succeeded");
        };
        let rendered = error.kernel().expect("a kernel error").to_string();
        assert!(
            rendered.contains("hello_name"),
            "the missing key is not named"
        );
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_malformed_credential_fails_validate_naming_the_key_and_never_the_value() {
        let (outcome, booted) = build(&[
            ("RENVOR_MAIL_USERNAME", "app user"),
            ("RENVOR_MAIL_PASSWORD", "hunter2CanaryDoNotLeak"),
        ]);
        let rendered = outcome.expect_err("a username with whitespace");
        assert!(rendered.contains("`username`"), "the key is not named");
        assert!(
            !rendered.contains("hunter2") && !rendered.contains("app user"),
            "the value was rendered"
        );
        assert_eq!(booted, 0);
        let (outcome, _) = build(&[
            ("RENVOR_MAIL_USERNAME", "app"),
            ("RENVOR_MAIL_PASSWORD", ""),
        ]);
        assert!(
            outcome
                .expect_err("an empty password")
                .contains("`password`")
        );
        let (outcome, _) = build(&[("RENVOR_MAIL_PASSWORD", "x")]);
        assert!(
            outcome
                .expect_err("a password without a username")
                .contains("`username`")
        );
    }

    #[test]
    fn plaintext_off_loopback_is_refused_at_validate_naming_security() {
        let (outcome, booted) = build(&[
            ("RENVOR_MAIL_SECURITY", "plaintext"),
            ("RENVOR_MAIL_ALLOW_INSECURE_LOOPBACK", "true"),
        ]);
        let rendered = outcome.expect_err("plaintext to relay.example.test");
        assert!(rendered.contains("`security`"), "the key is not named");
        assert!(rendered.contains("loopback"), "the rule is not named");
        assert_eq!(booted, 0);
        // POSITIVE CONTROL: loopback with the flag validates.
        let (outcome, _) = build(&[
            ("RENVOR_MAIL_HOST", "127.0.0.1"),
            ("RENVOR_MAIL_SECURITY", "plaintext"),
            ("RENVOR_MAIL_ALLOW_INSECURE_LOOPBACK", "true"),
        ]);
        assert!(outcome.is_ok());
    }
}
