//! Individual readiness contributors, and what happens when one misbehaves.
//!
//! # A contributor that panics is a contributor that is not ready
//!
//! FR-028 requires each contributor to be **individually identified**, and a panicking contributor
//! is the case that makes that requirement matter. The two obvious implementations both fail it:
//!
//! - **Let the panic propagate.** One misbehaving readiness check takes down the process that was
//!   asking whether it was healthy — the health endpoint becomes the outage.
//! - **Catch it and report the whole set as not-ready.** Safe, and useless: an operator learns the
//!   application is not ready and nothing about which of twelve checks broke.
//!
//! So a panic is caught, attributed to **that** contributor by name, and reported as its
//! readiness. The remaining contributors are still asked.
//!
//! `catch_unwind` needs no `unsafe`, which matters here: this workspace declares
//! `unsafe_code = "forbid"`, so an approach requiring it would not have been available.
//!
//! It also only catches panics that **unwind**. Under `panic = "abort"` there is nothing to catch
//! and a broken contributor would end the process — which is why `panic = "abort"` is
//! **unsupported** and refused at compile time in this crate's root (T147). See
//! [`crate::provider::contain`] for the full statement of the ruling, which applies identically
//! here.

use core::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use crate::health::Readiness;

/// One thing that has an opinion about whether the application should receive work.
///
/// `Send + Sync` because readiness is asked from whatever task handles a probe.
pub trait ReadinessContributor: Send + Sync {
    /// This contributor's name, which appears in the report (FR-028).
    fn name(&self) -> &str;

    /// Whether this contributor considers the application ready.
    ///
    /// A panic here is caught and reported as [`Readiness::NotReady`] for **this** contributor.
    fn readiness(&self) -> Readiness;
}

impl fmt::Debug for dyn ReadinessContributor {
    /// Renders **without calling a single author method**.
    ///
    /// This impl used to call `name()`. That is author code, and formatting is the one path in the
    /// kernel that cannot bound it — no deadline, no `catch_unwind` whose verdict could be
    /// reported, and no channel for a failure except the output being produced. A contributor
    /// whose `name()` blocked would hang whatever formatted it, and a readiness probe is driven
    /// from outside, so "whatever formatted it" is reachable by anyone who can call the probe.
    ///
    /// See [`crate::provider::registry`]'s `impl fmt::Debug for dyn Provider` for the full
    /// reasoning; it is identical, and both were found together by the W-005 requirements review.
    ///
    /// The name is still reported — from [`crate::health::ReadinessReport`], which holds the names
    /// Renvor captured when it asked each contributor **inside** a bounded call. That is a cached
    /// Renvor-owned fact; this is a trait object with nothing but author methods on it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadinessContributor")
            .finish_non_exhaustive()
    }
}

/// The most readiness workers one [`crate::HealthState`] may have outstanding at once.
///
/// **Renvor's number.** A healthy contributor's worker lives for microseconds, so reaching this
/// ceiling means workers are not returning — and the honest response to that is to stop making
/// more of them, not to keep asking.
///
/// # Per application, not per process
///
/// The counter lives in the `HealthState`, so **one application's hung contributor cannot refuse
/// another application's probes.** It was a crate-level `static` for exactly one commit, and the
/// W-005 verification re-review (N12, N13) found what that cost: a process-global budget couples
/// independent applications, and it was reproduced as a **test-suite failure** — one test
/// saturating the ceiling made a later test in the same binary receive the refusal fallback under
/// `--test-threads=1`.
///
/// That reproduction is the useful part. Test binaries stood in for applications and demonstrated
/// the cross-application coupling directly, on a defect whose disclosure said only "bounded".
pub const MAX_OUTSTANDING_PROBES: usize = 64;

/// How many readiness workers one health state has outstanding.
pub(crate) type Outstanding = Arc<AtomicUsize>;

/// Decrements the counter on drop, including while unwinding.
///
/// A guard rather than a decrement at the end of the worker: the worker's body runs author code
/// inside `catch_unwind`, and an early return or an unwind past the guard would otherwise lose the
/// slot permanently — turning the ceiling into a slow leak of its own.
struct Release(Outstanding);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Claims a slot, or reports that the ceiling is reached.
///
/// `fetch_update` rather than load-then-add. The first version checked and then incremented as two
/// operations, so concurrent callers could all observe 63 and all proceed — making the true bound
/// `64 + concurrent callers` while the documentation called it a hard ceiling (W-005 N14). A probe
/// is externally driven and plausibly concurrent, which is precisely when that gap opens.
fn claim(outstanding: &Outstanding) -> bool {
    outstanding
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            (current < MAX_OUTSTANDING_PROBES).then_some(current + 1)
        })
        .is_ok()
}

/// Why a verdict is not the contributor's own answer.
///
/// An enum rather than a set of booleans. Two independent flags would allow the impossible state
/// "panicked **and** timed out", and an operator reading a report needs exactly one reason.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ContributorFault {
    /// The contributor answered. The verdict is its own.
    #[default]
    None,
    /// It panicked instead of answering.
    Panicked,
    /// It did not answer inside the readiness deadline, and is still running.
    TimedOut,
    /// The host refused a thread, so it was never asked at all.
    NotAsked,
}

impl ContributorFault {
    /// Whether the contributor is broken rather than merely reporting not-ready.
    #[must_use]
    pub const fn is_fault(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// What one contributor answered, and whether it answered at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContributorVerdict {
    /// Which contributor.
    ///
    /// The contributor's own name when it supplied one, and its registration position when it
    /// could not — asking for the name is one of the calls that can fail.
    pub name: String,
    /// What it reported, or [`Readiness::NotReady`] if it did not report.
    pub readiness: Readiness,
    /// Why the answer is not the contributor's own, if it is not.
    ///
    /// Separate from [`Self::readiness`] rather than a special `Readiness` variant: an operator
    /// needs "this check is broken" to look different from "this check says no", and folding them
    /// into one value would make a defect indistinguishable from a working negative answer.
    pub fault: ContributorFault,
}

impl ContributorVerdict {
    /// Whether this contributor panicked rather than answering.
    #[must_use]
    pub const fn panicked(&self) -> bool {
        matches!(self.fault, ContributorFault::Panicked)
    }
}

/// Asks one contributor on a worker thread, bounded by `deadline`.
///
/// # Why a thread, and not just `catch_unwind`
///
/// `catch_unwind` contains a panic and does nothing at all about a contributor that never returns
/// — and a readiness probe that blocks for ever is the more likely of the two in production, since
/// the usual body of a readiness check is a network round trip. FR-025 prohibits an unbounded wait
/// in any kernel-owned path, and this is one. Same mechanism as the `Load` and `Validate` phases,
/// for the same reason, with the same honest cost: **a hung contributor's thread is leaked.**
///
/// # `name()` is asked inside the guard, not outside it
///
/// An earlier version read `contributor.name()` before entering `catch_unwind`, so a contributor
/// whose *name* panicked took the process down while the guard watched. Both calls now happen
/// inside, and `position` supplies the identity when neither is available.
pub(crate) fn ask(
    contributor: &Arc<dyn ReadinessContributor>,
    position: usize,
    deadline: Duration,
    outstanding: &Outstanding,
) -> ContributorVerdict {
    let fallback = format!("<contributor at registration position {position}>");

    // Refuse before spawning once too many workers are already stuck. Without this the leak is
    // **unbounded**: a contributor that never returns leaks one thread per probe, and a probe runs
    // on a timer somebody else controls — so a single hung check turns a readiness endpoint into a
    // thread-exhaustion vector that grows for as long as the process is monitored. Found by the
    // W-005 security review (finding 5.1).
    //
    // The leak does not go away; no Rust API can interrupt a blocked thread. It becomes **bounded**,
    // which is the difference between a recorded limitation and a denial of service.
    if !claim(outstanding) {
        return ContributorVerdict {
            name: fallback,
            readiness: Readiness::NotReady,
            fault: ContributorFault::NotAsked,
        };
    }

    let handle = Arc::clone(contributor);
    let release = Release(Arc::clone(outstanding));
    let (sender, receiver) = channel();

    let spawned = std::thread::Builder::new()
        .name("renvor-readiness".to_owned())
        .spawn(move || {
            // Decremented by the worker itself, so a thread that eventually returns gives its slot
            // back. One that never returns keeps its slot for ever — which is exactly the fact the
            // ceiling is accounting for.
            let _release = release;
            // Two guards, not one. `readiness` panicking is the likely case and `name` panicking
            // is the pathological one; guarding them together would throw away a perfectly good
            // name every time the likely case happened.
            //
            // `AssertUnwindSafe` because the handle is shared and both calls take `&self`: a panic
            // cannot leave a partially-mutated value visible through it.
            let named = catch_unwind(AssertUnwindSafe(|| handle.name().to_owned())).ok();
            let answered = catch_unwind(AssertUnwindSafe(|| handle.readiness())).ok();
            drop(sender.send((named, answered)));
        });

    if spawned.is_err() {
        // No manual decrement: `Builder::spawn` takes the closure by value, so a failed spawn
        // drops it, and dropping it runs the `Release` guard it captured. Decrementing here as
        // well would return the slot twice and wrap the counter.
        // Fail closed: an application whose readiness could not be established is not ready.
        return ContributorVerdict {
            name: fallback,
            readiness: Readiness::NotReady,
            fault: ContributorFault::NotAsked,
        };
    }

    match receiver.recv_timeout(deadline) {
        Ok((Some(name), Some(readiness))) => ContributorVerdict {
            name,
            readiness,
            fault: ContributorFault::None,
        },
        // Either call panicked. Fail closed on the readiness answer even when only `name` broke: a
        // contributor that cannot say what it is called is not one to take a `Ready` from.
        Ok((named, _)) => ContributorVerdict {
            name: named.unwrap_or(fallback),
            readiness: Readiness::NotReady,
            fault: ContributorFault::Panicked,
        },
        Err(RecvTimeoutError::Timeout) => ContributorVerdict {
            name: fallback,
            readiness: Readiness::NotReady,
            fault: ContributorFault::TimedOut,
        },
        // The worker unwound outside the guard, which the guard's placement makes unreachable.
        // Reported as a panic rather than asserted: SC-004 allows 0 panics, and a diagnostic that
        // is merely imprecise beats one that aborts the process.
        Err(RecvTimeoutError::Disconnected) => ContributorVerdict {
            name: fallback,
            readiness: Readiness::NotReady,
            fault: ContributorFault::Panicked,
        },
    }
}
