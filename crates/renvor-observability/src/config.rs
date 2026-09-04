//! The OTLP exporter's typed configuration section (FR-011), behind the `otel` feature.
//!
//! [`OtlpSection`](crate::config::OtlpSection) is the shape an operator writes — under `[otlp]`
//! in a file, or as `RENVOR_OTLP_*` in the environment — decoded by `renvor-config` against
//! this type before any merging, defaulted from [`DEFAULTS`](crate::config::DEFAULTS), and
//! checked by [`OtlpSection::settings_from`](crate::config::OtlpSection::settings_from) against
//! the caps [`crate::otel`] enforces: an endpoint that is not `https://`, nor `http://` to
//! loopback; a header that is not an HTTP token; a queue, batch, or timeout over its cap; a
//! required key nobody supplied. Each refusal names the key, the constraint, and the layer that
//! supplied the value (C-C8).
//!
//! # Where this runs, honestly
//!
//! The subscriber is installed before the kernel's lifecycle begins — it has to be, or the
//! kernel's own Load and Validate events are lost — so this section is resolved and checked by
//! the application **before** `ApplicationBuilder::build`, through the same resolver and with
//! the same diagnostics, and not inside the kernel's Validate phase. The property FR-011 asks
//! for still holds: the exporter opens no socket until its first export, and a refused section
//! never produces a layer to install. An application that also registers the source with the
//! builder gets the refusal a second time at Validate, which is harmless.
//!
//! Header values are wrapped in a [`Secret`](renvor_config::Secret) at the boundary and never
//! rendered.

use std::collections::BTreeMap;
use std::time::Duration;

use renvor_config::{
    ConfigHandle, ConfigSchema, LayeredResolverBuilder, SchemaSource, Secret, SectionKeys, Table,
};
use renvor_core::KernelError;
use renvor_core::config_port::ResolvedConfig;
use renvor_core::error::context::Constraint;
use serde::Deserialize;

use crate::otel::{MAX_BATCH, MAX_DURATION, MAX_QUEUE, OtlpSettings};

/// The defaults every key but `endpoint` and `service_name` carries.
pub const DEFAULTS: &str = r#"
queue = 2048
batch = 512
export_timeout_ms = 10000
scheduled_delay_ms = 5000
shutdown_timeout_ms = 5000
"#;

/// The `[otlp]` section.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtlpSection {
    /// The collector: `https://…`, or `http://` to a loopback host. Required.
    pub endpoint: String,
    /// The `service.name` resource attribute. Required.
    pub service_name: String,
    /// Headers sent with every export, by name. Values are secrets.
    pub headers: Option<BTreeMap<String, String>>,
    /// Spans waiting for export.
    pub queue: usize,
    /// Spans per export request.
    pub batch: usize,
    /// The bound on one export request, in milliseconds.
    pub export_timeout_ms: u64,
    /// The pause between exports of a partial batch, in milliseconds.
    pub scheduled_delay_ms: u64,
    /// The bound on the shutdown flush, in milliseconds.
    pub shutdown_timeout_ms: u64,
}

/// Every key but the endpoint and the header values.
impl core::fmt::Debug for OtlpSection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("OtlpSection")
            .field("service_name", &self.service_name)
            .field("headers", &self.headers.as_ref().map_or(0, BTreeMap::len))
            .field("queue", &self.queue)
            .field("batch", &self.batch)
            .field("export_timeout_ms", &self.export_timeout_ms)
            .finish_non_exhaustive()
    }
}

/// The all-optional form one source decodes into (see `renvor_config::ConfigSchema`).
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialOtlpSection {
    endpoint: Option<String>,
    service_name: Option<String>,
    headers: Option<BTreeMap<String, String>>,
    queue: Option<usize>,
    batch: Option<usize>,
    export_timeout_ms: Option<u64>,
    scheduled_delay_ms: Option<u64>,
    shutdown_timeout_ms: Option<u64>,
}

impl ConfigSchema for OtlpSection {
    type Partial = PartialOtlpSection;
}

impl OtlpSection {
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

    /// A source for this section alone, with this section's defaults beneath `builder`'s
    /// layers and the validator attached.
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
    pub fn settings_from(resolved: &ResolvedConfig<Self>) -> Result<OtlpSettings, KernelError> {
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
    ) -> Result<OtlpSettings, KernelError> {
        let keys = SectionKeys::new(prefix, resolved);
        let endpoint_key = keys.key("endpoint");
        let mut settings = OtlpSettings::new(&self.endpoint, &endpoint_key, &self.service_name)
            .map_err(|error| match error {
                crate::otel::OtelError::EndpointRefused { .. } => keys.rule(
                    "endpoint",
                    "an https URL, or an http URL to a loopback host",
                    "must be an https:// URL, or http:// to a loopback host, of at most 2048 bytes",
                ),
                _ => keys.refuse(
                    "service_name",
                    "1 to 128 bytes",
                    &Constraint::TooLong { maximum: 128 },
                ),
            })?;
        if let Some(headers) = &self.headers {
            for (name, value) in headers {
                let key = keys.key(&format!("headers.{name}"));
                settings = settings
                    .with_header(name, Secret::new(key.clone(), value.clone()), &key)
                    .map_err(|_| {
                        keys.rule(
                            "headers",
                            "HTTP tokens as names and values without control characters",
                            "every header name must be an HTTP token and every value free of \
                             control characters",
                        )
                    })?;
            }
        }
        keys.range("queue", self.queue as u128, 1, MAX_QUEUE as u128)?;
        keys.range("batch", self.batch as u128, 1, MAX_BATCH as u128)?;
        for (name, value) in [
            ("export_timeout_ms", self.export_timeout_ms),
            ("scheduled_delay_ms", self.scheduled_delay_ms),
            ("shutdown_timeout_ms", self.shutdown_timeout_ms),
        ] {
            keys.range(name, u128::from(value), 1, MAX_DURATION.as_millis())?;
        }
        settings
            .with_queue(self.queue, self.batch)
            .and_then(|settings| {
                settings.with_timeouts(
                    Duration::from_millis(self.export_timeout_ms),
                    Duration::from_millis(self.scheduled_delay_ms),
                    Duration::from_millis(self.shutdown_timeout_ms),
                )
            })
            .map_err(|_| {
                keys.rule(
                    "batch",
                    "a batch no larger than the queue, within the caps",
                    "the batch must not exceed the queue and every bound must be within its cap",
                )
            })
    }
}

/// Reads the settings a validated section resolved to.
///
/// # Errors
///
/// [`KernelError::Configuration`].
pub fn settings_from_handle(
    handle: &ConfigHandle<OtlpSection>,
) -> Result<OtlpSettings, KernelError> {
    handle.with(OtlpSection::settings_from)?
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use renvor_config::LayeredResolverBuilder;
    use renvor_core::config_port::ConfigSource as _;

    use super::OtlpSection;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn source(pairs: &[(&str, &str)]) -> renvor_config::SchemaSource<OtlpSection> {
        let mut all = vec![
            (
                "RENVOR_OTLP_ENDPOINT",
                "https://otlp.example.test/v1/traces",
            ),
            ("RENVOR_OTLP_SERVICE_NAME", "app"),
        ];
        all.extend_from_slice(pairs);
        OtlpSection::source(
            "otlp",
            LayeredResolverBuilder::new().with_environment_map("RENVOR_OTLP_", env(&all)),
        )
    }

    fn refusal(pairs: &[(&str, &str)]) -> String {
        let source = source(pairs);
        source.load().expect("decodes");
        source
            .validate()
            .expect_err("the validator refuses")
            .to_string()
    }

    #[test]
    fn a_complete_section_becomes_settings() {
        let source = source(&[(
            "RENVOR_OTLP_HEADERS__AUTHORIZATION",
            "Bearer hunter2CanaryDoNotLeak",
        )]);
        source.load().expect("resolves");
        source.validate().expect("validates");
        let settings = source
            .handle()
            .with(OtlpSection::settings_from)
            .expect("resolved")
            .expect("valid");
        let rendered = format!("{settings:?}");
        assert!(rendered.contains("headers: 1"), "the key is not named");
        assert!(!rendered.contains("hunter2"), "a header value leaked");
    }

    #[test]
    fn every_cap_and_the_endpoint_rule_are_refused_by_name_with_the_layer() {
        let rendered = refusal(&[("RENVOR_OTLP_QUEUE", "65537")]);
        assert!(rendered.contains("`queue`"), "the key is not named");
        assert!(rendered.contains("environment"), "the layer is not named");
        assert!(
            rendered.contains("between 1 and 65536"),
            "the constraint is not named"
        );
        for (key, value, name) in [
            ("RENVOR_OTLP_BATCH", "4097", "`batch`"),
            (
                "RENVOR_OTLP_EXPORT_TIMEOUT_MS",
                "60001",
                "`export_timeout_ms`",
            ),
            (
                "RENVOR_OTLP_SHUTDOWN_TIMEOUT_MS",
                "0",
                "`shutdown_timeout_ms`",
            ),
            (
                "RENVOR_OTLP_ENDPOINT",
                "http://otlp.example.test",
                "`endpoint`",
            ),
            ("RENVOR_OTLP_SERVICE_NAME", "", "`service_name`"),
            ("RENVOR_OTLP_HEADERS__BAD_NAME", "x\u{1}", "`headers`"),
        ] {
            let rendered = refusal(&[(key, value)]);
            assert!(
                rendered.contains(name),
                "a cap case is not named by its key"
            );
        }
        // The endpoint is never rendered in the refusal.
        let rendered = refusal(&[("RENVOR_OTLP_ENDPOINT", "http://otlp.example.test")]);
        assert!(
            !rendered.contains("otlp.example.test"),
            "the value was rendered"
        );
    }

    #[test]
    fn a_missing_required_key_is_refused_at_load() {
        let source = OtlpSection::source(
            "otlp",
            LayeredResolverBuilder::new()
                .with_environment_map("RENVOR_OTLP_", env(&[("RENVOR_OTLP_SERVICE_NAME", "app")])),
        );
        let rendered = source
            .load()
            .expect_err("endpoint has no default")
            .to_string();
        assert!(rendered.contains("endpoint"), "the key is not named");
    }
}
