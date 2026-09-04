//! The jobs capability's typed configuration section (FR-011).
//!
//! [`JobsSection`](crate::config::JobsSection) is the shape an operator writes — under `[jobs]`
//! in a file, or as `RENVOR_JOBS_*` in the environment — decoded by `renvor-config` against
//! this type before any merging, defaulted from [`DEFAULTS`](crate::config::DEFAULTS), and
//! checked by [`JobsSection::worker_from`](crate::config::JobsSection::worker_from) against the
//! caps [`crate::job`] and [`crate::worker`] enforce. A bound above its cap or a required key
//! nobody supplied fails the kernel's **Validate** phase naming the key, the constraint, and the
//! layer that supplied the value (C-C8) — before any provider is constructed or any task
//! spawned.
//!
//! The section describes the **bounds and the worker**; the store and the handlers are code,
//! because a store is a database connection the application already owns and a handler is a
//! function. What the section yields is a [`JobBounds`] for the store and a [`WorkerConfig`]
//! for the worker.

use std::time::Duration;

use renvor_config::{
    ConfigHandle, ConfigSchema, LayeredResolverBuilder, SchemaSource, SectionKeys, Table,
};
use renvor_core::KernelError;
use renvor_core::config_port::ResolvedConfig;
use renvor_core::retry::{MAX_ATTEMPTS_CAP, MAX_DELAY_CAP, RetryPolicy};
use serde::Deserialize;

use crate::job::{
    JobBounds, MAX_HANDLER_TIMEOUT_CAP, MAX_LEASE_CAP, MAX_PAYLOAD_BYTES_CAP, QueueName,
};
use crate::worker::{MAX_CONCURRENCY, MAX_STOP_GRACE, POLL_INTERVAL_RANGE, WorkerConfig};

/// The defaults every key but `queue` carries.
pub const DEFAULTS: &str = r#"
concurrency = 4
poll_interval_ms = 500
lease_secs = 60
handler_timeout_secs = 300
stop_grace_ms = 5000
max_payload_bytes = 65536
max_queue_depth = 100000
retry_max_attempts = 5
retry_initial_delay_ms = 1000
retry_max_delay_secs = 300
"#;

/// The `[jobs]` section.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobsSection {
    /// The queue the worker serves. Required.
    pub queue: String,
    /// How many jobs run at once.
    pub concurrency: usize,
    /// The pause between empty claim attempts, in milliseconds.
    pub poll_interval_ms: u64,
    /// How long a claim is leased, in seconds.
    pub lease_secs: u64,
    /// How long one handler may run, in seconds.
    pub handler_timeout_secs: u64,
    /// How long Stop waits for running jobs before aborting them, in milliseconds.
    pub stop_grace_ms: u64,
    /// The ceiling on a payload, in bytes.
    pub max_payload_bytes: usize,
    /// The ceiling on a queue's depth (ready + leased).
    pub max_queue_depth: u64,
    /// The most attempts a job is given.
    pub retry_max_attempts: u32,
    /// The delay before the second attempt, in milliseconds.
    pub retry_initial_delay_ms: u64,
    /// The ceiling every retry delay is held under, in seconds.
    pub retry_max_delay_secs: u64,
}

impl core::fmt::Debug for JobsSection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("JobsSection")
            .field("queue", &self.queue)
            .field("concurrency", &self.concurrency)
            .field("poll_interval_ms", &self.poll_interval_ms)
            .field("lease_secs", &self.lease_secs)
            .field("handler_timeout_secs", &self.handler_timeout_secs)
            .field("max_queue_depth", &self.max_queue_depth)
            .finish_non_exhaustive()
    }
}

/// The all-optional form one source decodes into (see `renvor_config::ConfigSchema`).
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialJobsSection {
    queue: Option<String>,
    concurrency: Option<usize>,
    poll_interval_ms: Option<u64>,
    lease_secs: Option<u64>,
    handler_timeout_secs: Option<u64>,
    stop_grace_ms: Option<u64>,
    max_payload_bytes: Option<usize>,
    max_queue_depth: Option<u64>,
    retry_max_attempts: Option<u32>,
    retry_initial_delay_ms: Option<u64>,
    retry_max_delay_secs: Option<u64>,
}

impl ConfigSchema for JobsSection {
    type Partial = PartialJobsSection;
}

/// What the section yields: the store's bounds and the worker's configuration.
#[derive(Clone, Debug)]
pub struct JobsSettings {
    /// The bounds the store validates payloads and depth against.
    pub bounds: JobBounds,
    /// The worker's queue, concurrency, timers, and retry policy.
    pub worker: WorkerConfig,
}

impl JobsSection {
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
            .with_validator(|resolved| Self::worker_from(resolved).map(|_| ()))
    }

    /// The bounds and worker configuration a resolved section of its own describes, or the
    /// first rule it breaks.
    ///
    /// # Errors
    ///
    /// [`KernelError::Configuration`] naming the key, the constraint, and the layer.
    pub fn worker_from(resolved: &ResolvedConfig<Self>) -> Result<JobsSettings, KernelError> {
        resolved.value().worker_at("", resolved)
    }

    /// The bounds and worker configuration this section describes as the table `prefix` of a
    /// larger resolved schema.
    ///
    /// # Errors
    ///
    /// [`KernelError::Configuration`] naming the key, the constraint, and the layer.
    pub fn worker_at<T>(
        &self,
        prefix: &str,
        resolved: &ResolvedConfig<T>,
    ) -> Result<JobsSettings, KernelError> {
        let keys = SectionKeys::new(prefix, resolved);
        let queue = QueueName::new(&self.queue).map_err(|_| {
            keys.rule(
                "queue",
                "1 to 64 characters of [a-z0-9_.-]",
                "must be 1 to 64 characters of [a-z0-9_.-]",
            )
        })?;
        keys.range(
            "concurrency",
            self.concurrency as u128,
            1,
            MAX_CONCURRENCY as u128,
        )?;
        keys.range(
            "poll_interval_ms",
            u128::from(self.poll_interval_ms),
            POLL_INTERVAL_RANGE.0.as_millis(),
            POLL_INTERVAL_RANGE.1.as_millis(),
        )?;
        keys.range(
            "lease_secs",
            u128::from(self.lease_secs),
            1,
            u128::from(MAX_LEASE_CAP.as_secs()),
        )?;
        keys.range(
            "handler_timeout_secs",
            u128::from(self.handler_timeout_secs),
            1,
            u128::from(MAX_HANDLER_TIMEOUT_CAP.as_secs()),
        )?;
        keys.range(
            "stop_grace_ms",
            u128::from(self.stop_grace_ms),
            0,
            MAX_STOP_GRACE.as_millis(),
        )?;
        keys.range(
            "max_payload_bytes",
            self.max_payload_bytes as u128,
            1,
            MAX_PAYLOAD_BYTES_CAP as u128,
        )?;
        keys.range(
            "max_queue_depth",
            u128::from(self.max_queue_depth),
            1,
            u128::from(u64::MAX),
        )?;
        keys.range(
            "retry_max_attempts",
            u128::from(self.retry_max_attempts),
            1,
            u128::from(MAX_ATTEMPTS_CAP),
        )?;
        keys.range(
            "retry_initial_delay_ms",
            u128::from(self.retry_initial_delay_ms),
            1,
            MAX_DELAY_CAP.as_millis(),
        )?;
        keys.range(
            "retry_max_delay_secs",
            u128::from(self.retry_max_delay_secs),
            self.retry_initial_delay_ms.div_ceil(1000).max(1).into(),
            u128::from(MAX_DELAY_CAP.as_secs()),
        )?;

        let bounds = JobBounds::new()
            .with_max_payload_bytes(self.max_payload_bytes)
            .and_then(|bounds| bounds.with_max_queue_depth(self.max_queue_depth))
            .map_err(|_| {
                keys.rule(
                    "max_payload_bytes",
                    "bounds within their caps",
                    "the bounds were refused by the store",
                )
            })?;
        let retry = RetryPolicy::new(
            self.retry_max_attempts,
            Duration::from_millis(self.retry_initial_delay_ms),
            Duration::from_secs(self.retry_max_delay_secs),
            Duration::from_secs(self.handler_timeout_secs),
        )
        .map_err(|_| {
            keys.rule(
                "retry_max_delay_secs",
                "a retry policy within its caps",
                "the retry policy was refused: the maximum delay must be at least the initial \
                 delay and at most one hour",
            )
        })?;
        let worker = WorkerConfig::new(queue)
            .with_concurrency(self.concurrency)
            .and_then(|config| {
                config.with_poll_interval(Duration::from_millis(self.poll_interval_ms))
            })
            .and_then(|config| config.with_lease(Duration::from_secs(self.lease_secs)))
            .and_then(|config| {
                config.with_handler_timeout(Duration::from_secs(self.handler_timeout_secs))
            })
            .and_then(|config| config.with_stop_grace(Duration::from_millis(self.stop_grace_ms)))
            .map_err(|_| {
                keys.rule(
                    "concurrency",
                    "worker bounds within their caps",
                    "the worker bounds were refused",
                )
            })?
            .with_retry(retry);
        Ok(JobsSettings { bounds, worker })
    }
}

/// Reads the settings a validated section resolved to. Validate already refused anything this
/// could refuse, so a failure here is the handle being read before `build`.
///
/// # Errors
///
/// [`KernelError::Configuration`].
pub fn settings_from_handle(
    handle: &ConfigHandle<JobsSection>,
) -> Result<JobsSettings, KernelError> {
    handle.with(JobsSection::worker_from)?
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

    use super::JobsSection;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn builder(pairs: &[(&str, &str)]) -> LayeredResolverBuilder {
        let mut all = vec![("RENVOR_JOBS_QUEUE", "mail")];
        all.extend_from_slice(pairs);
        LayeredResolverBuilder::new().with_environment_map("RENVOR_JOBS_", env(&all))
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
        let source = JobsSection::source("jobs", builder(pairs));
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
    fn a_complete_section_becomes_bounds_and_a_worker_with_the_defaults() {
        let source = JobsSection::source("jobs", builder(&[]));
        source.load().expect("resolves");
        source.validate().expect("validates");
        let settings = source
            .handle()
            .with(JobsSection::worker_from)
            .expect("resolved")
            .expect("valid");
        assert_eq!(settings.bounds.max_queue_depth(), 100_000);
        assert_eq!(settings.bounds.max_payload_bytes(), 64 * 1024);
        assert_eq!(settings.worker.queue().as_str(), "mail");
        assert_eq!(settings.worker.concurrency(), 4);
        assert_eq!(settings.worker.lease().as_secs(), 60);
        assert_eq!(settings.worker.handler_timeout().as_secs(), 300);
    }

    #[test]
    fn a_bound_over_its_cap_fails_validate_naming_key_constraint_and_layer_before_any_boot() {
        let (outcome, booted) = build(&[("RENVOR_JOBS_CONCURRENCY", "1025")]);
        let rendered = outcome.expect_err("1024 is the cap");
        assert!(rendered.contains("`concurrency`"), "the key is not named");
        assert!(rendered.contains("environment"), "the layer is not named");
        assert!(
            rendered.contains("between 1 and 1024"),
            "the constraint is not named"
        );
        assert_eq!(booted, 0);
        for (key, value, name) in [
            ("RENVOR_JOBS_POLL_INTERVAL_MS", "9", "`poll_interval_ms`"),
            ("RENVOR_JOBS_LEASE_SECS", "3601", "`lease_secs`"),
            (
                "RENVOR_JOBS_HANDLER_TIMEOUT_SECS",
                "86401",
                "`handler_timeout_secs`",
            ),
            ("RENVOR_JOBS_STOP_GRACE_MS", "25001", "`stop_grace_ms`"),
            (
                "RENVOR_JOBS_MAX_PAYLOAD_BYTES",
                "1048577",
                "`max_payload_bytes`",
            ),
            ("RENVOR_JOBS_MAX_QUEUE_DEPTH", "0", "`max_queue_depth`"),
            (
                "RENVOR_JOBS_RETRY_MAX_ATTEMPTS",
                "101",
                "`retry_max_attempts`",
            ),
            (
                "RENVOR_JOBS_RETRY_MAX_DELAY_SECS",
                "3601",
                "`retry_max_delay_secs`",
            ),
            ("RENVOR_JOBS_QUEUE", "Mail Queue", "`queue`"),
        ] {
            let (outcome, booted) = build(&[(key, value)]);
            let rendered = outcome.expect_err(key);
            assert!(
                rendered.contains(name),
                "a cap case is not named by its key"
            );
            assert_eq!(booted, 0, "a provider booted on a refused cap case");
        }
    }

    #[test]
    fn a_missing_required_key_fails_before_any_boot() {
        let count = Arc::new(AtomicU32::new(0));
        let source = JobsSection::source(
            "jobs",
            LayeredResolverBuilder::new().with_environment_map("RENVOR_JOBS_", env(&[])),
        );
        let outcome = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(Counting(
                Arc::clone(&count),
                ProviderId::new("counting"),
            )))
            .build();
        let Err(error) = outcome else {
            panic!("queue has no default and was not supplied, yet the build succeeded");
        };
        let rendered = error.kernel().expect("a kernel error").to_string();
        assert!(rendered.contains("queue"), "the key is not named");
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}
