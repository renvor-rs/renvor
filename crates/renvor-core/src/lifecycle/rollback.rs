//! Unwinding a partial start: reverse **actual initialisation** order, and every failure reported.
//!
//! # Two guarantees that pull in opposite directions
//!
//! C-L3 says rollback replays the **realised** initialisation order backwards — not the
//! registration order, because resolution may reorder and a test asserting the wrong one passes
//! while the implementation is wrong.
//!
//! C-L4 says a provider that fails **during** rollback must not abort the rest of it. Those pull
//! against each other in the obvious implementation: `?` on the first `stop()` failure is both the
//! shortest code and a direct violation, because it strands every provider that had not been
//! stopped yet — the ones with the *most* state to release, since they initialised first.
//!
//! So the rollback routine collects rather than returning early. It stops every provider it was
//! given, and reports every failure alongside the original.
//!
//! # Why the original failure and the rollback failures are separate fields
//!
//! [`BootFailure::origin`] is *why the application is not running*. [`BootFailure::rollback`] is
//! *what went wrong while giving up*. Flattening them into one list would make the first
//! indistinguishable from the rest, and the first is the one the author has to fix.

use core::fmt;
use std::time::Duration;

use crate::error::KernelError;
use crate::lifecycle::LifecyclePhase;
use crate::lifecycle::application::InitialisedProvider;
use crate::lifecycle::application::deadline_millis;
use crate::provider::{ProviderId, ProviderRegistry};

/// What happened while unwinding a partial start.
#[derive(Debug, Default)]
pub struct RollbackReport {
    stopped: Vec<ProviderId>,
    failures: Vec<KernelError>,
}

impl RollbackReport {
    /// Every provider whose `stop` was called, in the order it was called.
    ///
    /// This is the sequence C-L3's acceptance criterion asserts against, and it lists **every**
    /// provider that was stopped — including the ones whose stop failed, because "we tried and it
    /// refused" and "we never tried" are different facts.
    #[must_use]
    pub fn stopped(&self) -> &[ProviderId] {
        &self.stopped
    }

    /// Every failure raised during rollback, one per failing provider (C-L4).
    #[must_use]
    pub fn failures(&self) -> &[KernelError] {
        &self.failures
    }

    /// Whether every provider stopped cleanly.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Stops every initialised provider in reverse initialisation order, each within `deadline`.
///
/// Drains `initialised`, so the caller cannot roll the same provider back twice — `Stop` runs **at
/// most once** per provider (FR-008, V-A3), and here that is a consequence of the collection being
/// emptied rather than of a flag somebody has to check.
pub(crate) async fn roll_back(
    registry: &ProviderRegistry,
    initialised: &mut Vec<InitialisedProvider>,
    deadline: Duration,
) -> RollbackReport {
    let mut report = RollbackReport::default();

    // `.rev()` over the realised order: reverse **actual initialisation** order (C-L3, FR-004).
    for entry in initialised.drain(..).rev() {
        let Some(provider) = registry.get(entry.position()) else {
            // Unreachable — the position came from this registry. Recorded rather than asserted,
            // for the SC-004 reason: a panic here would replace the diagnostic the author needs.
            report.failures.push(KernelError::ProviderStop {
                provider: entry.id().to_string(),
                source: "the provider is no longer registered".into(),
            });
            continue;
        };

        report.stopped.push(entry.id().clone());

        // Bounded, because a provider that hangs while stopping would hang shutdown itself —
        // an unbounded wait in a kernel-owned path (FR-025, C-L7). Its deadline is reported as a
        // deadline rather than as a refusal: not answering and refusing are different faults.
        match tokio::time::timeout(deadline, provider.stop()).await {
            Ok(Ok(())) => {}
            // Collected, never returned early: a failure here must not strand the providers that
            // have not been stopped yet (C-L4).
            Ok(Err(source)) => report.failures.push(KernelError::ProviderStop {
                provider: entry.id().to_string(),
                source,
            }),
            Err(_) => report.failures.push(KernelError::DeadlineExceeded {
                operation: format!("stop provider `{}`", entry.id()),
                deadline_ms: deadline_millis(deadline),
            }),
        }

        // The provider's scope is cancelled once it has been stopped, so anything it left running
        // observes cancellation rather than outliving the provider that owns it.
        entry.scope().cancel();
    }

    report
}

/// Why an application failed to start, and what happened while unwinding.
///
/// There is no `Application` alongside this — [`crate::lifecycle::Application::boot`] consumes
/// itself, so a failed start leaves nothing half-built to be observed or used.
#[derive(Debug)]
pub struct BootFailure {
    origin: KernelError,
    rollback: RollbackReport,
    phases: Vec<LifecyclePhase>,
}

impl BootFailure {
    /// Packages a boot failure with its rollback outcome and the phases the run entered.
    pub(crate) const fn new(
        origin: KernelError,
        rollback: RollbackReport,
        phases: Vec<LifecyclePhase>,
    ) -> Self {
        Self {
            origin,
            rollback,
            phases,
        }
    }

    /// The failure that stopped the start. This is the one the author has to fix.
    #[must_use]
    pub const fn origin(&self) -> &KernelError {
        &self.origin
    }

    /// What happened while unwinding.
    #[must_use]
    pub const fn rollback(&self) -> &RollbackReport {
        &self.rollback
    }

    /// The phases this run entered before failing (FR-002).
    #[must_use]
    pub fn phases(&self) -> &[LifecyclePhase] {
        &self.phases
    }
}

impl fmt::Display for BootFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "application failed to start: {}", self.origin)?;
        if !self.rollback.is_clean() {
            write!(
                f,
                " ({} provider(s) also failed to stop during rollback)",
                self.rollback.failures().len()
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for BootFailure {
    /// The **originating** failure, not a rollback failure.
    ///
    /// A caller walking `source()` is asking why the application is not running. Pointing them at
    /// a rollback failure would answer a question they did not ask and hide the one they did —
    /// the rollback failures remain reachable through [`Self::rollback`].
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.origin)
    }
}
