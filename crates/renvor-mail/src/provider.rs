//! The mailer as a kernel provider: verified at Boot, published as state, reported in readiness.
//!
//! # Boot proves the transport answers
//!
//! With `verify_on_boot` on — the default — Boot calls [`Mailer::verify`] under a bound and fails
//! startup if the server does not answer or refuses the credentials (FR-052, principle IV). The
//! failure is a closed [`MailBootError`] naming the phase and the category; the address and the
//! credential are in neither its `Display` nor its `Debug`.
//!
//! # What is published
//!
//! The concrete mailer, as `Arc<M>`, through `register_state`. The port is generic (native
//! `async fn`), so there is no `dyn Mailer` to publish; an application names the mailer type it
//! configured, which is also what makes a substitute an explicit choice (FR-008).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use renvor_core::error::BoxedCause;
use renvor_core::health::{Readiness, ReadinessContributor};
use renvor_core::provider::ProviderId;
use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};

use crate::port::{MailError, Mailer};

/// The capability a mailer offers.
pub const MAIL_CAPABILITY: &str = "mail";
/// How long Boot waits for the verification, over and above the adapter's own bound.
pub const DEFAULT_BOOT_TIMEOUT: Duration = Duration::from_secs(30);

/// The capability identifier as a value.
#[must_use]
pub fn mail_capability() -> CapabilityId {
    CapabilityId::new(MAIL_CAPABILITY)
}

/// Why the mail provider could not boot. **Closed**: each variant names the phase and the
/// category, never the server, the address, or the credential.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum MailBootError {
    /// The server could not be reached at verify.
    #[error("mail boot failed at verify: the server is unreachable; check the host and port")]
    Unreachable,
    /// The server refused the credentials or the session at verify.
    #[error("mail boot failed at verify: the server refused the session; check the credentials")]
    CredentialRefused,
    /// The server did not answer within the bound at verify.
    #[error("mail boot failed at verify: the server did not answer within the bound")]
    Unanswered,
    /// The transport's settings were refused before any connection.
    #[error("mail boot failed at verify: the transport settings were refused")]
    SettingsRefused,
    /// The provider was initialised twice, which the kernel prevents.
    #[error("mail boot failed at register: the provider was initialised twice")]
    AlreadyBooted,
}

impl From<MailError> for MailBootError {
    fn from(error: MailError) -> Self {
        match error {
            MailError::Unavailable => Self::Unreachable,
            MailError::TimedOut => Self::Unanswered,
            MailError::Rejected => Self::CredentialRefused,
            MailError::Refused(_) | MailError::EntropyUnavailable => Self::SettingsRefused,
        }
    }
}

/// Readiness of the mailer: an atomic flipped by Boot and Stop.
#[derive(Debug)]
pub(crate) struct MailReadiness {
    name: String,
    ready: Arc<AtomicBool>,
}

impl ReadinessContributor for MailReadiness {
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

/// A provider that publishes a mailer as the `mail` capability.
pub struct MailProvider<M> {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    mailer: Arc<M>,
    verify_on_boot: bool,
    boot_timeout: Duration,
    ready: Arc<AtomicBool>,
}

impl<M> core::fmt::Debug for MailProvider<M> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MailProvider")
            .field("id", &self.id)
            .field("verify_on_boot", &self.verify_on_boot)
            .field("ready", &self.ready.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl<M: Mailer + 'static> MailProvider<M> {
    /// Declares a provider publishing `mailer`, verifying it at Boot.
    #[must_use]
    pub fn new(id: ProviderId, mailer: Arc<M>) -> Self {
        Self {
            id,
            provides: vec![mail_capability()],
            mailer,
            verify_on_boot: true,
            boot_timeout: DEFAULT_BOOT_TIMEOUT,
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Whether Boot verifies the transport (default `true`). Off is a visible choice.
    #[must_use]
    pub const fn with_verify_on_boot(mut self, verify: bool) -> Self {
        self.verify_on_boot = verify;
        self
    }

    /// The bound on the whole verification at Boot.
    #[must_use]
    pub const fn with_boot_timeout(mut self, timeout: Duration) -> Self {
        self.boot_timeout = timeout;
        self
    }

    /// The mailer this provider publishes.
    #[must_use]
    pub fn mailer(&self) -> Arc<M> {
        Arc::clone(&self.mailer)
    }
}

impl<M: Mailer + 'static> ReadinessContributor for MailProvider<M> {
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

impl<M: Mailer + 'static> Provider for MailProvider<M> {
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
                return Err(Box::new(MailBootError::AlreadyBooted) as BoxedCause);
            }
            if self.verify_on_boot {
                let verified = tokio::time::timeout(self.boot_timeout, self.mailer.verify()).await;
                let outcome = match verified {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(error)) => Err(MailBootError::from(error)),
                    Err(_elapsed) => Err(MailBootError::Unanswered),
                };
                if let Err(error) = outcome {
                    tracing::warn!(
                        target: crate::port::MAIL_EVENT_TARGET,
                        provider = self.id.as_str(),
                        category = ?error,
                        "the mail transport failed verification at boot"
                    );
                    return Err(Box::new(error) as BoxedCause);
                }
            }
            context
                .register_state(Arc::clone(&self.mailer))
                .map_err(|error| Box::new(error) as BoxedCause)?;
            context.register_readiness(Arc::new(MailReadiness {
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

    use renvor_core::observe::FixedEntropy;
    use renvor_core::provider::ProviderId;
    use renvor_core::provider::registry::{CapabilityId, InitContext, Provider, ProviderFuture};
    use renvor_core::{ApplicationBuilder, ErrorCategory, Readiness, ReadinessContributor as _};

    use super::{MAIL_CAPABILITY, MailBootError, MailProvider, mail_capability};
    use crate::port::{MailError, Mailer as _};
    use crate::recording::RecordingMailbox;

    /// A provider that needs a mailer and reaches it through state.
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
                    .state::<Arc<RecordingMailbox>>()
                    .map(|_| ())
                    .map_err(|error| Box::new(error) as renvor_core::error::BoxedCause)
            })
        }
        fn stop(&self) -> ProviderFuture<'_> {
            Box::pin(async { Ok(()) })
        }
    }

    fn mailbox() -> Arc<RecordingMailbox> {
        Arc::new(RecordingMailbox::new(Arc::new(FixedEntropy::new(
            [0x22; 16],
        ))))
    }

    #[test]
    fn a_consumer_with_no_mail_provider_fails_at_register_naming_both_ends() {
        let error = ApplicationBuilder::new()
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-mail"),
                needs: vec![mail_capability()],
            }))
            .build()
            .expect_err("no provider offers `mail`");
        let kernel = error.kernel().expect("a kernel error");
        assert_eq!(kernel.category(), ErrorCategory::DependencyMissing);
        let rendered = kernel.to_string();
        assert!(rendered.contains("needs-mail") && rendered.contains(MAIL_CAPABILITY));
    }

    #[tokio::test]
    async fn a_verified_mailer_boots_ready_and_the_consumer_reaches_it() {
        let provider = MailProvider::new(ProviderId::new("mail"), mailbox());
        assert_eq!(provider.readiness(), Readiness::NotReady);
        let mut application = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .with_provider(Box::new(Consumer {
                id: ProviderId::new("needs-mail"),
                needs: vec![mail_capability()],
            }))
            .build()
            .expect("registers")
            .boot()
            .await
            .expect("boots");
        let report = application.health().readiness();
        assert!(
            report.blocking().is_empty(),
            "a contributor blocks readiness"
        );
        let verdict = report
            .contributors
            .iter()
            .find(|verdict| verdict.name == "mail")
            .expect("the mail contributor is registered");
        assert_eq!(verdict.readiness, Readiness::Ready);
        application.shutdown().await;
        let after = application.health().readiness();
        let verdict = after
            .contributors
            .iter()
            .find(|verdict| verdict.name == "mail")
            .map(|verdict| verdict.readiness);
        assert_eq!(verdict, Some(Readiness::NotReady));
    }

    #[tokio::test]
    async fn a_transport_that_fails_verification_fails_boot_with_the_category() {
        let mailbox = mailbox();
        mailbox.fail_verification(true);
        // The category, at the port: what Boot maps.
        let category = MailBootError::from(mailbox.verify().await.unwrap_err());
        assert_eq!(category, MailBootError::Unreachable);
        // And Boot itself refuses (SC-001): a mailer that does not answer is not `Ready`.
        let provider = MailProvider::new(ProviderId::new("mail"), Arc::clone(&mailbox));
        let outcome = ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .build()
            .expect("registers")
            .boot()
            .await;
        assert!(
            outcome.is_err(),
            "boot reached Ready on a failed verification"
        );
    }

    #[tokio::test]
    async fn verification_can_be_declined_visibly() {
        let mailbox = mailbox();
        mailbox.fail_verification(true);
        let provider =
            MailProvider::new(ProviderId::new("mail"), mailbox).with_verify_on_boot(false);
        ApplicationBuilder::new()
            .with_provider(Box::new(provider))
            .build()
            .expect("registers")
            .boot()
            .await
            .expect("boots without verifying, because the author said so");
    }

    #[test]
    fn every_port_error_maps_to_a_boot_category() {
        assert_eq!(
            MailBootError::from(MailError::Unavailable),
            MailBootError::Unreachable
        );
        assert_eq!(
            MailBootError::from(MailError::TimedOut),
            MailBootError::Unanswered
        );
        assert_eq!(
            MailBootError::from(MailError::Rejected),
            MailBootError::CredentialRefused
        );
        for error in [MailBootError::Unreachable, MailBootError::CredentialRefused] {
            let rendered = error.to_string();
            assert!(rendered.contains("at verify"), "the phase is not named");
            assert!(!rendered.contains('@') && !rendered.contains("://"));
        }
    }
}
