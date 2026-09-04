//! The storage capability's typed configuration section (FR-011).
//!
//! [`StorageSection`](crate::config::StorageSection) is the shape an operator writes — under
//! `[storage]` in a file, or as `RENVOR_STORAGE_*` in the environment — decoded by
//! `renvor-config` against this type before any merging, defaulted from
//! [`DEFAULTS`](crate::config::DEFAULTS), and checked by
//! [`StorageSection::settings_from`](crate::config::StorageSection::settings_from) against the
//! caps [`crate::port`] and [`crate::filesystem`] enforce. A bound above its cap or a required
//! key nobody supplied fails the kernel's **Validate** phase naming the key, the constraint, and
//! the layer that supplied the value (C-C8) — before any provider is constructed. Whether the
//! root exists and is writable is Boot's question (the provider's probe), not Validate's: a
//! directory is a fact about the machine, not about the configuration.

use std::time::Duration;

use renvor_config::{
    ConfigHandle, ConfigSchema, LayeredResolverBuilder, SchemaSource, SectionKeys, Table,
};
use renvor_core::KernelError;
use renvor_core::config_port::ResolvedConfig;
use renvor_core::error::context::Constraint;
use serde::Deserialize;

use crate::filesystem::{FilesystemSettings, TIMEOUT_RANGE};
use crate::port::{MAX_OBJECT_BYTES_CAP, StorageBounds};

/// The most bytes a root path may carry.
pub const MAX_ROOT_BYTES: usize = 4096;

/// The defaults every key but `root` carries.
pub const DEFAULTS: &str = r#"
max_object_bytes = 67108864
timeout_secs = 30
"#;

/// The `[storage]` section.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageSection {
    /// The directory objects live under. Required; must exist at Boot.
    pub root: String,
    /// The ceiling on an object's size, in bytes, on write and on read.
    pub max_object_bytes: u64,
    /// The bound on one operation, in seconds.
    pub timeout_secs: u64,
}

/// Every key but the root, which is an operator's path.
impl core::fmt::Debug for StorageSection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StorageSection")
            .field("max_object_bytes", &self.max_object_bytes)
            .field("timeout_secs", &self.timeout_secs)
            .finish_non_exhaustive()
    }
}

/// The all-optional form one source decodes into (see `renvor_config::ConfigSchema`).
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartialStorageSection {
    root: Option<String>,
    max_object_bytes: Option<u64>,
    timeout_secs: Option<u64>,
}

impl ConfigSchema for StorageSection {
    type Partial = PartialStorageSection;
}

impl StorageSection {
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
    pub fn settings_from(
        resolved: &ResolvedConfig<Self>,
    ) -> Result<FilesystemSettings, KernelError> {
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
    ) -> Result<FilesystemSettings, KernelError> {
        let keys = SectionKeys::new(prefix, resolved);
        if self.root.is_empty() {
            return Err(keys.refuse(
                "root",
                "a directory path",
                &Constraint::TooShort { minimum: 1 },
            ));
        }
        if self.root.len() > MAX_ROOT_BYTES || self.root.bytes().any(|byte| byte == 0) {
            return Err(keys.refuse(
                "root",
                "a directory path",
                &Constraint::TooLong {
                    maximum: MAX_ROOT_BYTES,
                },
            ));
        }
        keys.range(
            "max_object_bytes",
            u128::from(self.max_object_bytes),
            1,
            u128::from(MAX_OBJECT_BYTES_CAP),
        )?;
        keys.range(
            "timeout_secs",
            u128::from(self.timeout_secs),
            u128::from(TIMEOUT_RANGE.0.as_secs()),
            u128::from(TIMEOUT_RANGE.1.as_secs()),
        )?;
        let bounds = StorageBounds::new()
            .with_max_object_bytes(self.max_object_bytes)
            .map_err(|_| {
                keys.rule(
                    "max_object_bytes",
                    "a bound within its cap",
                    "the bound was refused by the store",
                )
            })?;
        FilesystemSettings::new(&self.root, bounds)
            .with_timeout(Duration::from_secs(self.timeout_secs))
            .map_err(|_| {
                keys.rule(
                    "timeout_secs",
                    "a bound within its cap",
                    "the bound was refused by the store",
                )
            })
    }
}

/// Reads the settings a validated section resolved to. Validate already refused anything this
/// could refuse, so a failure here is the handle being read before `build`.
///
/// # Errors
///
/// [`KernelError::Configuration`].
pub fn settings_from_handle(
    handle: &ConfigHandle<StorageSection>,
) -> Result<FilesystemSettings, KernelError> {
    handle.with(StorageSection::settings_from)?
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

    use super::StorageSection;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn builder(pairs: &[(&str, &str)]) -> LayeredResolverBuilder {
        let mut all = vec![("RENVOR_STORAGE_ROOT", "/var/lib/app/objects")];
        all.extend_from_slice(pairs);
        LayeredResolverBuilder::new().with_environment_map("RENVOR_STORAGE_", env(&all))
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
        let source = StorageSection::source("storage", builder(pairs));
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
    fn a_complete_section_becomes_settings_with_the_defaults() {
        let source = StorageSection::source("storage", builder(&[]));
        source.load().expect("resolves");
        source.validate().expect("validates");
        let settings = source
            .handle()
            .with(StorageSection::settings_from)
            .expect("resolved")
            .expect("valid");
        let rendered = format!("{settings:?}");
        assert!(rendered.contains("67108864"), "the key is not named");
        assert!(!rendered.contains("/var/lib"), "the root leaked");
    }

    #[test]
    fn a_bound_over_its_cap_fails_validate_naming_key_constraint_and_layer_before_any_boot() {
        let (outcome, booted) = build(&[("RENVOR_STORAGE_MAX_OBJECT_BYTES", "1073741825")]);
        let rendered = outcome.expect_err("1 GiB is the cap");
        assert!(
            rendered.contains("`max_object_bytes`"),
            "the key is not named"
        );
        assert!(rendered.contains("environment"), "the layer is not named");
        assert!(
            rendered.contains("between 1 and 1073741824"),
            "the constraint is not named"
        );
        assert_eq!(booted, 0);
        let (outcome, _) = build(&[("RENVOR_STORAGE_TIMEOUT_SECS", "601")]);
        assert!(
            outcome
                .expect_err("600 s is the cap")
                .contains("`timeout_secs`")
        );
        let (outcome, _) = build(&[("RENVOR_STORAGE_ROOT", "")]);
        assert!(outcome.expect_err("an empty root").contains("`root`"));
    }

    #[tokio::test]
    async fn the_provider_opens_the_root_at_boot_from_the_validated_section() {
        // FR-011 end to end: the section validates, the provider opens the root at Boot and
        // probes it; a root that does not exist is Boot's refusal (the machine), not Validate's
        // (the configuration).
        use crate::provider::StorageProvider;
        let root =
            std::env::temp_dir().join(format!("renvor-storage-section-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("creates the root");
        let source = StorageSection::source(
            "storage",
            LayeredResolverBuilder::new().with_environment_map(
                "RENVOR_STORAGE_",
                env(&[("RENVOR_STORAGE_ROOT", root.to_str().expect("utf-8"))]),
            ),
        );
        let provider = StorageProvider::from_config(ProviderId::new("storage"), source.handle());
        assert!(provider.store().is_none(), "nothing is open before Boot");
        let application = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(provider))
            .build()
            .expect("the section validates")
            .boot()
            .await
            .expect("boot opens and probes the root");
        let verdict = application
            .health()
            .readiness()
            .contributors
            .iter()
            .find(|verdict| verdict.name == "storage")
            .map(|verdict| verdict.readiness);
        assert_eq!(verdict, Some(renvor_core::Readiness::Ready));
        drop(application);
        let _ = std::fs::remove_dir_all(&root);

        // The absent root: Validate passes, Boot refuses.
        let missing =
            std::env::temp_dir().join(format!("renvor-storage-absent-{}", std::process::id()));
        let source = StorageSection::source(
            "storage",
            LayeredResolverBuilder::new().with_environment_map(
                "RENVOR_STORAGE_",
                env(&[("RENVOR_STORAGE_ROOT", missing.to_str().expect("utf-8"))]),
            ),
        );
        let provider = StorageProvider::from_config(ProviderId::new("storage"), source.handle());
        let outcome = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(provider))
            .build()
            .expect("an absent directory is not a configuration defect")
            .boot()
            .await;
        let Err(error) = outcome else {
            panic!("boot reached Ready on an absent root");
        };
        assert_eq!(error.origin().category(), ErrorCategory::ProviderInit);
    }

    #[test]
    fn a_missing_required_key_fails_before_any_boot() {
        let count = Arc::new(AtomicU32::new(0));
        let source = StorageSection::source(
            "storage",
            LayeredResolverBuilder::new().with_environment_map("RENVOR_STORAGE_", env(&[])),
        );
        let outcome = ApplicationBuilder::new()
            .with_config_source(Arc::new(source))
            .with_provider(Box::new(Counting(
                Arc::clone(&count),
                ProviderId::new("counting"),
            )))
            .build();
        let Err(error) = outcome else {
            panic!("root has no default and was not supplied, yet the build succeeded");
        };
        let rendered = error.kernel().expect("a kernel error").to_string();
        assert!(rendered.contains("root"), "the key is not named");
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}
