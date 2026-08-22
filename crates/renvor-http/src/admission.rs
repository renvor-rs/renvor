//! Admission control: the work gate and the concurrency ceiling, as one decision.
//!
//! # Why both bounds are taken together
//!
//! A request that passes the concurrency ceiling but is refused by the work gate must not have
//! consumed a concurrency slot it then has to remember to release. Taking both in one call, and
//! releasing both on one drop, removes the ordering question entirely.
//!
//! # Why the permit is a guard rather than a counter
//!
//! [`renvor_core::WorkPermit`] releases on drop, including through an early return and through a
//! panic. [`AdmissionGuard`] holds it and the concurrency permit together, so every exit path from
//! a request releases both — including the paths nobody wrote deliberately.
//!
//! The alternative, an explicit release call, is forgotten on exactly one path: the error path,
//! which is the one that runs when the drain matters most.

use std::sync::Arc;

use renvor_core::{WorkGate, WorkPermit};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::{HttpError, HttpErrorKind};

/// The two bounds a request must pass before reaching a handler.
#[derive(Clone, Debug)]
pub struct Admission {
    gate: WorkGate,
    slots: Arc<Semaphore>,
}

impl Admission {
    /// Builds admission control over `gate`, permitting `max_concurrent` simultaneous requests.
    #[must_use]
    pub fn new(gate: WorkGate, max_concurrent: usize) -> Self {
        Self {
            gate,
            slots: Arc::new(Semaphore::new(max_concurrent)),
        }
    }

    /// Admits one request, or refuses it.
    ///
    /// # Errors
    ///
    /// [`HttpErrorKind::Unavailable`] when the application is draining, or when the concurrency
    /// ceiling is reached. Both are `503`, and both are **errors** rather than a silent drop or a
    /// silent acceptance — the rule contract C-L6 states for the kernel, applied at the transport.
    pub fn admit(&self) -> Result<AdmissionGuard, HttpError> {
        // Concurrency first: it is the cheaper refusal, and a saturated server should spend as
        // little as possible on the request it is about to turn away.
        //
        // `try_acquire_owned` never waits. Waiting here would turn a saturated server into a
        // server with an unbounded queue, which is the same outage arriving later and with more
        // memory consumed on the way.
        let Ok(slot) = Arc::clone(&self.slots).try_acquire_owned() else {
            return Err(HttpError::new(
                HttpErrorKind::Unavailable,
                "concurrency ceiling reached",
            ));
        };

        // Then the gate. Once shutdown begins this refuses, which is what stops new requests from
        // reaching a handler during drain.
        let permit = self
            .gate
            .begin("http request")
            .map_err(|error| HttpError::new(HttpErrorKind::Unavailable, error.to_string()))?;

        Ok(AdmissionGuard {
            _permit: permit,
            _slot: slot,
        })
    }

    /// The gate this admission control draws permits from.
    #[must_use]
    pub const fn gate(&self) -> &WorkGate {
        &self.gate
    }

    /// How many concurrency slots are currently free.
    #[must_use]
    pub fn available_slots(&self) -> usize {
        self.slots.available_permits()
    }
}

/// Holds one request's admission. Releases both bounds on drop.
///
/// Deliberately carries **no** release method: drop is the only release, so there is one path to
/// get right rather than two that can disagree. This mirrors [`renvor_core::WorkPermit`]'s own
/// reasoning.
#[derive(Debug)]
pub struct AdmissionGuard {
    _permit: WorkPermit,
    _slot: OwnedSemaphorePermit,
}

#[cfg(test)]
mod tests {
    use super::Admission;
    use crate::error::HttpErrorKind;
    use renvor_core::WorkGate;

    #[test]
    fn a_request_is_admitted_while_the_gate_is_open() {
        let admission = Admission::new(WorkGate::new(), 4);
        let guard = admission.admit().expect("the gate is open");
        assert_eq!(admission.gate().outstanding(), 1);
        assert_eq!(admission.available_slots(), 3);
        drop(guard);
        assert_eq!(admission.gate().outstanding(), 0);
        assert_eq!(admission.available_slots(), 4);
    }

    #[test]
    fn admission_is_refused_once_the_gate_closes() {
        // FR-030. A request arriving after drain begins does not reach a handler.
        let gate = WorkGate::new();
        let admission = Admission::new(gate.clone(), 4);

        assert!(gate.close(), "the first close closes the gate");

        let error = admission.admit().expect_err("a closed gate must refuse");
        assert_eq!(error.kind(), HttpErrorKind::Unavailable);
        assert_eq!(error.status(), 503);

        // POSITIVE CONTROL: an open gate admits, so the refusal is about the gate's state.
        assert!(Admission::new(WorkGate::new(), 4).admit().is_ok());
    }

    #[test]
    fn a_refused_admission_does_not_consume_a_concurrency_slot() {
        // The ordering hazard this module exists to remove: if the gate refuses AFTER a slot was
        // taken, a closed gate would leak a slot per rejected request.
        let gate = WorkGate::new();
        let admission = Admission::new(gate.clone(), 2);
        gate.close();

        for _ in 0..10 {
            assert!(admission.admit().is_err());
        }
        assert_eq!(
            admission.available_slots(),
            2,
            "rejected requests leaked concurrency slots"
        );
    }

    #[test]
    fn the_concurrency_ceiling_is_enforced_at_its_exact_boundary() {
        // SC-008 at the boundary: the Nth is admitted, the N+1th is not.
        let admission = Admission::new(WorkGate::new(), 2);

        let first = admission.admit().expect("1 of 2");
        let second = admission.admit().expect("2 of 2");
        assert_eq!(admission.available_slots(), 0);

        let error = admission
            .admit()
            .expect_err("the request past the ceiling must be refused");
        assert_eq!(error.kind(), HttpErrorKind::Unavailable);

        // Releasing one frees exactly one slot, so the ceiling is a live bound rather than a
        // one-way latch.
        drop(first);
        assert!(admission.admit().is_ok());
        drop(second);
    }

    #[test]
    fn a_guard_releases_through_an_early_return() {
        // The shape that forgets an explicit release: something unrelated fails, and nothing in
        // the failing path mentions the guard.
        fn handle(admission: &Admission) -> Result<(), &'static str> {
            let _guard = admission.admit().map_err(|_| "refused")?;
            Err("an unrelated failure")
        }

        let admission = Admission::new(WorkGate::new(), 4);
        assert!(handle(&admission).is_err());
        assert_eq!(admission.gate().outstanding(), 0, "the permit leaked");
        assert_eq!(admission.available_slots(), 4, "the slot leaked");
    }
}
