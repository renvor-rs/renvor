//! Shared fixtures for the User Story 1 integration tests.
//!
//! `#![allow(dead_code)]` because each test binary in `tests/` is a **separate crate** that pulls
//! this module in whole and uses a subset of it. Without the allow, every test binary warns about
//! the fixtures the other binaries use. The alternative — four near-identical copies of the same
//! provider double — is how fixtures drift apart until two tests disagree about what "a failing
//! provider" means.

#![allow(dead_code)]

use std::sync::{Arc, Mutex, PoisonError};

use renvor_core::observe::FixedEntropy;
use renvor_core::provider::ProviderFuture;
use renvor_core::{
    ApplicationBuilder, CapabilityId, InitContext, KernelError, Provider, ProviderId,
};

/// One thing a provider did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// `"init"`, `"stop"`, or `"degraded"`.
    pub action: &'static str,
    /// Which provider.
    pub provider: String,
}

/// What every provider did, in the order it actually happened.
///
/// This is the observation channel C-L3 requires: the tests assert against the **realised**
/// sequence, never against the order the providers were registered in. A test that asserted the
/// registration order could pass while the implementation was wrong, which is the exact failure
/// the contract calls out.
#[derive(Clone, Debug, Default)]
pub struct Journal {
    events: Arc<Mutex<Vec<Event>>>,
}

impl Journal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, action: &'static str, provider: &ProviderId) {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Event {
                action,
                provider: provider.as_str().to_owned(),
            });
    }

    pub fn events(&self) -> Vec<Event> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The provider names for one action, in the order it happened.
    pub fn names(&self, action: &str) -> Vec<String> {
        self.events()
            .into_iter()
            .filter(|event| event.action == action)
            .map(|event| event.provider)
            .collect()
    }

    pub fn inits(&self) -> Vec<String> {
        self.names("init")
    }

    pub fn stops(&self) -> Vec<String> {
        self.names("stop")
    }

    /// Providers that silently carried on without a capability they needed.
    ///
    /// FR-022's whole subject. A non-empty result on a kernel path is a defect.
    pub fn degraded(&self) -> Vec<String> {
        self.names("degraded")
    }
}

/// A value a provider registers so another can find it.
#[derive(Debug, PartialEq, Eq)]
pub struct Marker(pub &'static str);

/// What a scripted provider does when asked to initialise or stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Behaviour {
    /// Initialises and stops cleanly.
    Succeed,
    /// Fails to initialise.
    FailInit,
    /// Initialises, then fails to stop — the C-L4 case.
    FailStop,
    /// Registers a [`Marker`] in typed state, then succeeds.
    RegisterMarker,
    /// Waits until its own scope is cancelled, then reports cancellation (FR-024).
    AwaitCancellation,
    /// **Never answers while initialising.** The C-L9 `Hang` behaviour: it exercises deadline
    /// enforcement, and unlike `AwaitCancellation` it ignores cancellation entirely — which is
    /// what makes a deadline necessary rather than merely tidy.
    Hang,
    /// Initialises cleanly, then never answers while stopping.
    HangOnStop,
    /// **Panics after an await** while initialising — the case a `catch_unwind` around the call
    /// site could never reach, because the panic happens on a later poll.
    PanicOnInit,
    /// Initialises cleanly, then panics after an await while stopping.
    PanicOnStop,
    /// **The FR-022 anti-pattern, on purpose.** Carries on without the [`Marker`] it needs,
    /// recording that it degraded. Used as a positive control: the journal must be able to catch
    /// a degrading provider, or its silence on the kernel's own paths proves nothing.
    DegradeWithoutMarker,
}

/// A provider whose behaviour and declarations are supplied by the test.
#[derive(Debug)]
pub struct Scripted {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    dependencies: Vec<CapabilityId>,
    behaviour: Behaviour,
    journal: Journal,
}

impl Scripted {
    /// A provider that declares nothing and succeeds.
    pub fn new(journal: &Journal, id: &str) -> Self {
        Self {
            id: ProviderId::new(id),
            provides: Vec::new(),
            dependencies: Vec::new(),
            behaviour: Behaviour::Succeed,
            journal: journal.clone(),
        }
    }

    #[must_use]
    pub fn provides(mut self, capabilities: &[&str]) -> Self {
        self.provides
            .extend(capabilities.iter().map(|name| CapabilityId::new(*name)));
        self
    }

    #[must_use]
    pub fn needs(mut self, capabilities: &[&str]) -> Self {
        self.dependencies
            .extend(capabilities.iter().map(|name| CapabilityId::new(*name)));
        self
    }

    #[must_use]
    pub fn behaving(mut self, behaviour: Behaviour) -> Self {
        self.behaviour = behaviour;
        self
    }

    /// Boxes this provider for registration.
    #[must_use]
    pub fn boxed(self) -> Box<dyn Provider> {
        Box::new(self)
    }
}

impl Provider for Scripted {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn dependencies(&self) -> &[CapabilityId] {
        &self.dependencies
    }

    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            match self.behaviour {
                Behaviour::Succeed => {
                    self.journal.record("init", &self.id);
                    Ok(())
                }
                Behaviour::FailInit => Err("this provider was scripted to fail".into()),
                Behaviour::FailStop => {
                    self.journal.record("init", &self.id);
                    Ok(())
                }
                Behaviour::RegisterMarker => {
                    // Propagated, not swallowed: a duplicate registration is a real failure and
                    // the test needs to see it arrive as one.
                    context.register_state(Marker("registered"))?;
                    self.journal.record("init", &self.id);
                    Ok(())
                }
                Behaviour::Hang => {
                    std::future::pending::<()>().await;
                    unreachable!("a pending future never resolves")
                }
                Behaviour::HangOnStop | Behaviour::PanicOnStop => {
                    self.journal.record("init", &self.id);
                    Ok(())
                }
                Behaviour::PanicOnInit => {
                    tokio::task::yield_now().await;
                    panic!("this provider panics on purpose while initialising");
                }
                Behaviour::AwaitCancellation => {
                    context.cancel().cancelled().await;
                    Err(KernelError::Cancelled {
                        phase: renvor_core::LifecyclePhase::Boot,
                    }
                    .into())
                }
                Behaviour::DegradeWithoutMarker => {
                    if context.state::<Marker>().is_err() {
                        self.journal.record("degraded", &self.id);
                    }
                    self.journal.record("init", &self.id);
                    Ok(())
                }
            }
        })
    }

    fn stop(&self) -> ProviderFuture<'_> {
        Box::pin(async move {
            self.journal.record("stop", &self.id);
            if self.behaviour == Behaviour::HangOnStop {
                std::future::pending::<()>().await;
            }
            if self.behaviour == Behaviour::PanicOnStop {
                tokio::task::yield_now().await;
                panic!("this provider panics on purpose while stopping");
            }
            if self.behaviour == Behaviour::FailStop {
                return Err("this provider was scripted to fail while stopping".into());
            }
            Ok(())
        })
    }
}

/// A builder with entropy pinned, so the run identifier never varies between runs.
pub fn builder() -> ApplicationBuilder {
    ApplicationBuilder::new().with_entropy(Box::new(FixedEntropy::new(vec![0x5a; 32])))
}
