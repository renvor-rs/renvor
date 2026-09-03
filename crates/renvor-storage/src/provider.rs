//! The object store as a kernel provider: probed at Boot, published as state, in readiness.
//!
//! Boot calls [`ObjectStore::probe`] under a bound and fails startup if the backend cannot be
//! written (FR-060, principle IV). The failure is a closed [`StorageBootError`] naming the phase
//! and the category — never the root path, the bucket, or the backend's message.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::ProviderId;
use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};

use crate::port::{ObjectStore, StorageError};

/// The capability an object store offers.
pub const STORAGE_CAPABILITY: &str = "storage";
/// How long Boot waits for the probe, over and above the adapter's own bound.
pub const DEFAULT_BOOT_TIMEOUT: Duration = Duration::from_secs(30);

/// The capability identifier as a value.
#[must_use]
pub fn storage_capability() -> CapabilityId {
    CapabilityId::new(STORAGE_CAPABILITY)
}

/// Why the storage provider could not boot. **Closed**: phase and category, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum StorageBootError {
    /// The backend could not be reached or the root does not exist.
    #[error("storage boot failed at probe: the backend is unreachable; check the root or endpoint")]
    Unreachable,
    /// The backend refused the probe write.
    #[error("storage boot failed at probe: the backend denied the write; check permissions")]
    Denied,
    /// The probe did not finish within the bound.
    #[error("storage boot failed at probe: the backend did not answer within the bound")]
    Unanswered,
    /// The backend has no room for even the probe.
    #[error("storage boot failed at probe: the backend is at capacity")]
    Capacity,
    /// The adapter's settings were refused.
    #[error("storage boot failed at probe: the settings were refused")]
    SettingsRefused,
    /// The provider was initialised twice, which the kernel prevents.
    #[error("storage boot failed at register: the provider was initialised twice")]
    AlreadyBooted,
}

impl From<StorageError> for StorageBootError {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::Unavailable => Self::Unreachable,
            StorageError::TimedOut => Self::Unanswered,
            StorageError::Denied => Self::Denied,
            StorageError::Capacity => Self::Capacity,
            StorageError::Refused(_) => Self::SettingsRefused,
        }
    }
}

/// Readiness of the store: an atomic flipped by Boot and Stop.
#[derive(Debug)]
struct StorageReadiness {
    name: String,
    ready: Arc<AtomicBool>,
}

impl ReadinessContributor for StorageReadiness {
    fn name(&self) -> &str {
        &self.name
    }

    fn readiness(&self) -> Readiness {
        if self.ready.load(Ordering::Acquire) {
            Readiness::Ready
        } else {
            Readiness::NotReady
        }
    }
}

/// A provider that publishes an object store as the `storage` capability.
pub struct StorageProvider<S> {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    store: Arc<S>,
    boot_timeout: Duration,
    ready: Arc<AtomicBool>,
}

impl<S> core::fmt::Debug for StorageProvider<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StorageProvider")
            .field("id", &self.id)
            .field("ready", &self.ready.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl<S: ObjectStore + 'static> StorageProvider<S> {
    /// Declares a provider publishing `store`, probing it at Boot.
    #[must_use]
    pub fn new(id: ProviderId, store: Arc<S>) -> Self {
        Self {
            id,
            provides: vec![storage_capability()],
            store,
            boot_timeout: DEFAULT_BOOT_TIMEOUT,
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The bound on the whole probe at Boot.
    #[must_use]
    pub const fn with_boot_timeout(mut self, timeout: Duration) -> Self {
        self.boot_timeout = timeout;
        self
    }

    /// The store this provider publishes.
    #[must_use]
    pub fn store(&self) -> Arc<S> {
        Arc::clone(&self.store)
    }
}

impl<S: ObjectStore + 'static> ReadinessContributor for StorageProvider<S> {
    fn name(&self) -> &str {
        self.id.as_str()
    }

    fn readiness(&self) -> Readiness {
        if self.ready.load(Ordering::Acquire) {
            Readiness::Ready
        } else {
            Readiness::NotReady
        }
    }
}

impl<S: ObjectStore + 'static> Provider for StorageProvider<S> {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn dependencies(&self) -> &[CapabilityId] {
        &[]
    }

    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            if self.ready.load(Ordering::Acquire) {
                return Err(Box::new(StorageBootError::AlreadyBooted) as BoxedCause);
            }
            let probed = tokio::time::timeout(self.boot_timeout, self.store.probe()).await;
            let outcome = match probed {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(StorageBootError::from(error)),
                Err(_elapsed) => Err(StorageBootError::Unanswered),
            };
            if let Err(error) = outcome {
                tracing::warn!(
                    target: crate::port::STORAGE_EVENT_TARGET,
                    provider = self.id.as_str(),
                    category = ?error,
                    "the object store failed its probe at boot"
                );
                return Err(Box::new(error) as BoxedCause);
            }
            context
                .register_state(Arc::clone(&self.store))
                .map_err(|error| Box::new(error) as BoxedCause)?;
            context.register_readiness(Arc::new(StorageReadiness {
                name: self.id.as_str().to_owned(),
                ready: Arc::clone(&self.ready),
            }));
            self.ready.store(true, Ordering::Release);
            Ok(())
        })
    }

    fn stop(&self) -> ProviderFuture<'_> {
        Box::pin(async move {
            self.ready.store(false, Ordering::Release);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use renvor_core::provider::ProviderId;
    use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};
    use renvor_core::{ApplicationBuilder, ErrorCategory, Readiness, ReadinessContributor as _};

    use super::{STORAGE_CAPABILITY, StorageBootError, StorageProvider, storage_capability};
    use crate::memory::MemoryStore;
    use crate::port::{StorageBounds, StorageError};

    struct Consumer {
        id: ProviderId,
        needs: Vec<CapabilityId>,
    }

    impl Provider for Consumer {
        fn id(&self) -> &ProviderId {
            &self.id
        }
        fn provides(&self) -> &[CapabilityId] {
            &[]
        }
        fn dependencies(&self) -> &[CapabilityId] {
            &self.needs
        }
        fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
            Box::pin(async move {
                context
                    .state::<Arc<MemoryStore>>()
                    .map(|_| ())
                    .map_err(|error| Box::new(error) as renvor_core::error::BoxedCause)
            })
        }
        fn stop(&self) -> ProviderFuture<'_> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn a_consumer_with_no_storage_provider_fails_at_register_naming_both_ends() {
        let error = ApplicationBuilder::new()
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-storage"),
                needs: vec![storage_capability()],
            }))
            .build()
            .expect_err("no provider offers `storage`");
        let kernel = error.kernel().expect("a kernel error");
        assert_eq!(kernel.category(), ErrorCategory::DependencyMissing);
        let rendered = kernel.to_string();
        assert!(rendered.contains("needs-storage") && rendered.contains(STORAGE_CAPABILITY));
    }

    #[tokio::test]
    async fn a_probed_store_boots_ready_and_the_consumer_reaches_it() {
        let store = Arc::new(MemoryStore::new(StorageBounds::new()));
        let provider = StorageProvider::new(ProviderId::new("storage"), store);
        assert_eq!(provider.readiness(), Readiness::NotReady);
        let mut application = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-storage"),
                needs: vec![storage_capability()],
            }))
            .build()
            .expect("registers")
            .boot()
            .await
            .expect("boots");
        let verdict = application
            .health()
            .readiness()
            .contributors
            .iter()
            .find(|verdict| verdict.name == "storage")
            .map(|verdict| verdict.readiness);
        assert_eq!(verdict, Some(Readiness::Ready));
        application.shutdown().await;
        let verdict = application
            .health()
            .readiness()
            .contributors
            .iter()
            .find(|verdict| verdict.name == "storage")
            .map(|verdict| verdict.readiness);
        assert_eq!(verdict, Some(Readiness::NotReady));
    }

    #[test]
    fn every_port_error_maps_to_a_boot_category_that_names_the_phase() {
        let cases = [
            (StorageError::Unavailable, StorageBootError::Unreachable),
            (StorageError::TimedOut, StorageBootError::Unanswered),
            (StorageError::Denied, StorageBootError::Denied),
            (StorageError::Capacity, StorageBootError::Capacity),
        ];
        for (index, (error, expected)) in cases.into_iter().enumerate() {
            assert_eq!(
                StorageBootError::from(error),
                expected,
                "mapping case {index} is wrong"
            );
            let rendered = expected.to_string();
            assert!(rendered.contains("at probe"), "the phase is not named");
            assert!(!rendered.contains('/'), "a path-like rendering");
        }
    }
}
