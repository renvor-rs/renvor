//! The cache as a kernel provider: how "a missing required capability fails startup" is made true.
//!
//! # The mechanism is the kernel's, not this crate's
//!
//! An application service that needs a cache depends on the capability [`CACHE_CAPABILITY`]. If no
//! provider offers it, the kernel refuses at **Register** with `DependencyMissing` naming both
//! the dependent and the capability (C-G11) — before anything is booted. This module adds the
//! providers that can offer it; it adds no rule.
//!
//! # Two providers, not one with a switch
//!
//! [`MemoryCacheProvider`] offers the substitute; the Valkey adapter has its own provider behind
//! its feature. There is deliberately no `CacheProvider::new(backend: Backend)` with a
//! configuration-selected variant, because that is the silent fallback the constitution forbids
//! one configuration edit away: an author who wants the substitute constructs it, in code, where a
//! reader sees it (FR-008).
//!
//! # What a booted provider publishes
//!
//! An `Arc<MemoryCache>` in the application's typed state, retrievable by a later provider with
//! `context.state::<Arc<MemoryCache>>()`, and a readiness contributor that reports `Ready` once
//! Boot has published it and `NotReady` before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::ProviderId;
use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};

use crate::memory::MemoryCache;

/// The capability every cache provider offers, and every cache consumer depends on.
pub const CACHE_CAPABILITY: &str = "cache";

/// The capability identifier as a value.
#[must_use]
pub fn cache_capability() -> CapabilityId {
    CapabilityId::new(CACHE_CAPABILITY)
}

/// Readiness of one cache provider: an atomic flipped by Boot.
///
/// Its own type rather than a closure because a readiness probe is driven from outside the
/// application and a contributor that could block would block the probe; reading one atomic
/// cannot (the same shape as the HTTP provider's).
#[derive(Debug)]
pub(crate) struct CacheReadiness {
    name: String,
    ready: Arc<AtomicBool>,
}

impl CacheReadiness {
    pub(crate) fn new(name: &ProviderId, ready: Arc<AtomicBool>) -> Self {
        Self {
            name: name.as_str().to_owned(),
            ready,
        }
    }
}

impl ReadinessContributor for CacheReadiness {
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

/// The substitute as a provider.
pub struct MemoryCacheProvider {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    cache: OnceLock<Arc<MemoryCache>>,
    pending: std::sync::Mutex<Option<MemoryCache>>,
    ready: Arc<AtomicBool>,
}

impl core::fmt::Debug for MemoryCacheProvider {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MemoryCacheProvider")
            .field("id", &self.id)
            .field("booted", &self.cache.get().is_some())
            .finish()
    }
}

impl MemoryCacheProvider {
    /// Declares a provider that will publish `cache` under [`CACHE_CAPABILITY`] at Boot.
    #[must_use]
    pub fn new(id: ProviderId, cache: MemoryCache) -> Self {
        Self {
            id,
            provides: vec![cache_capability()],
            cache: OnceLock::new(),
            pending: std::sync::Mutex::new(Some(cache)),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The published cache, or `None` before Boot has reached this provider.
    #[must_use]
    pub fn cache(&self) -> Option<Arc<MemoryCache>> {
        self.cache.get().cloned()
    }
}

impl Provider for MemoryCacheProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            let cache = self
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
                .map(Arc::new)
                .ok_or_else(|| Box::new(CacheBootError::AlreadyBooted) as BoxedCause)?;
            context
                .register_state(Arc::clone(&cache))
                .map_err(|error| Box::new(error) as BoxedCause)?;
            context.register_readiness(Arc::new(CacheReadiness::new(
                &self.id,
                Arc::clone(&self.ready),
            )));
            let _ = self.cache.set(cache);
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

impl ReadinessContributor for MemoryCacheProvider {
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

/// Which step of bringing a cache up failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BootPhase {
    /// Parsing the configured address.
    Configure,
    /// Opening the connection.
    Connect,
    /// The authenticated `PING`.
    Readiness,
}

impl BootPhase {
    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configure => "configure",
            Self::Connect => "connect",
            Self::Readiness => "readiness",
        }
    }
}

/// Why a cache provider could not boot. **Closed, fieldless where a field could carry text.**
///
/// Names the phase and the category and offers a corrective action, and carries **no** address,
/// credential, or driver message (FR-007, FR-009).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum CacheBootError {
    /// The configured address could not be parsed.
    #[error(
        "cache boot failed at configure: the cache address is not a valid URL. Check the \
         configured `cache.url`; it must be `redis://` or `rediss://` with a host and port"
    )]
    InvalidAddress,
    /// The server could not be reached within the connection timeout.
    #[error(
        "cache boot failed at connect: the cache server did not accept a connection within the \
         timeout. Check that the server is running, that the host and port are reachable from this \
         process, and that TLS is configured on both sides the same way"
    )]
    Unreachable,
    /// The server refused the credential.
    #[error(
        "cache boot failed at connect: the cache server refused the credential. Check the \
         configured password or ACL user; the value itself is never printed"
    )]
    CredentialRefused,
    /// The server accepted the connection but did not answer `PING` in time.
    #[error(
        "cache boot failed at readiness: the cache server accepted the connection but did not \
         answer PING within the operation timeout. The server may be overloaded or the timeout \
         too short for this network"
    )]
    Unanswered,
    /// The provider was initialised twice, which the kernel prevents.
    #[error(
        "cache boot failed at connect: the provider was initialised twice, which the kernel prevents"
    )]
    AlreadyBooted,
}

impl CacheBootError {
    /// The phase the failure belongs to.
    #[must_use]
    pub const fn phase(self) -> BootPhase {
        match self {
            Self::InvalidAddress => BootPhase::Configure,
            Self::Unreachable | Self::CredentialRefused | Self::AlreadyBooted => BootPhase::Connect,
            Self::Unanswered => BootPhase::Readiness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CACHE_CAPABILITY, CacheBootError, MemoryCacheProvider, cache_capability};
    use crate::memory::MemoryCache;
    use crate::port::{CacheBounds, Namespace};
    use renvor_core::error::BoxedCause;
    use renvor_core::provider::ProviderId;
    use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};
    use renvor_core::{
        ApplicationBuilder, ErrorCategory, KernelError, Readiness, ReadinessContributor,
    };

    /// A provider that needs a cache and offers nothing.
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
                // Reaching the dependency's state is what the ordering guarantee is for.
                context
                    .state::<std::sync::Arc<MemoryCache>>()
                    .map(|_| ())
                    .map_err(|error| Box::new(error) as BoxedCause)
            })
        }
        fn stop(&self) -> ProviderFuture<'_> {
            Box::pin(async { Ok(()) })
        }
    }

    fn memory_cache() -> MemoryCache {
        MemoryCache::new(Namespace::new("app").unwrap(), CacheBounds::new())
    }

    #[test]
    fn a_consumer_with_no_cache_provider_fails_at_register_naming_both_ends() {
        // SC-001, the "missing required capability" half. Nothing is booted.
        let error = ApplicationBuilder::new()
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-cache"),
                needs: vec![cache_capability()],
            }))
            .build()
            .expect_err("no provider offers `cache`");
        let kernel = error
            .kernel()
            .expect("a kernel error, not a phase-less failure");
        assert_eq!(kernel.category(), ErrorCategory::DependencyMissing);
        let rendered = kernel.to_string();
        assert!(
            rendered.contains("needs-cache"),
            "the dependent is not named"
        );
        assert!(
            rendered.contains(CACHE_CAPABILITY),
            "the capability is not named"
        );
    }

    #[tokio::test]
    async fn with_a_memory_provider_the_consumer_boots_and_readiness_is_ready() {
        // POSITIVE CONTROL for the test above, and the substitute's own contract: it offers
        // `cache`, publishes the state, and reports ready only after Boot.
        let provider = MemoryCacheProvider::new(ProviderId::new("cache"), memory_cache());
        assert_eq!(
            provider.readiness(),
            Readiness::NotReady,
            "not ready before Boot"
        );
        let application = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-cache"),
                needs: vec![cache_capability()],
            }))
            .build()
            .expect("register succeeds")
            .boot()
            .await
            .expect("boot reaches Ready");
        let report = application.health().readiness();
        assert!(
            report.blocking().is_empty(),
            "a contributor blocks readiness: {report:?}"
        );
        assert!(
            report
                .contributors
                .iter()
                .any(|verdict| verdict.name == "cache"),
            "the cache contributor is not in the report"
        );
    }

    #[test]
    fn every_boot_error_names_its_phase_and_never_an_address() {
        for error in [
            CacheBootError::InvalidAddress,
            CacheBootError::Unreachable,
            CacheBootError::CredentialRefused,
            CacheBootError::Unanswered,
            CacheBootError::AlreadyBooted,
        ] {
            let rendered = error.to_string();
            assert!(
                rendered.contains(error.phase().as_str()),
                "phase missing for {error:?}"
            );
            // No credential separator and no digit: an address, a port, or a password would
            // carry one of those. The scheme hint in `InvalidAddress` carries neither.
            assert!(
                !rendered.contains('@') && !rendered.chars().any(|c| c.is_ascii_digit()),
                "a boot error rendered something address-shaped for {error:?}"
            );
        }
        // Each non-trivial error offers a corrective action.
        assert!(
            CacheBootError::Unreachable
                .to_string()
                .contains("Check that")
        );
        assert!(
            CacheBootError::CredentialRefused
                .to_string()
                .contains("never printed")
        );
    }

    #[test]
    fn the_kernel_error_type_is_what_this_module_maps_to() {
        // Type-identity guard: the consumer's `state` refusal is a `KernelError`.
        let _: fn(KernelError) -> BoxedCause = |error| Box::new(error);
    }
}
