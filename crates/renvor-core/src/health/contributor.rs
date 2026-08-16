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

use core::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};

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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadinessContributor")
            .field("name", &self.name())
            .finish()
    }
}

/// What one contributor answered, and whether it answered at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContributorVerdict {
    /// Which contributor.
    pub name: String,
    /// What it reported, or [`Readiness::NotReady`] if it panicked.
    pub readiness: Readiness,
    /// Whether the answer came from a panic rather than from the contributor.
    ///
    /// A separate field rather than a special `Readiness` variant: an operator reading a report
    /// needs "this check is broken" to look different from "this check says no", and folding them
    /// into one value would make a defect indistinguishable from a working negative answer.
    pub panicked: bool,
}

/// Asks one contributor, converting a panic into an attributed not-ready.
pub(crate) fn ask(contributor: &dyn ReadinessContributor) -> ContributorVerdict {
    let name = contributor.name().to_owned();

    // `AssertUnwindSafe` because `&dyn ReadinessContributor` is a shared reference and the call
    // takes `&self`: a panic cannot leave a partially-mutated value visible through it.
    match catch_unwind(AssertUnwindSafe(|| contributor.readiness())) {
        Ok(readiness) => ContributorVerdict {
            name,
            readiness,
            panicked: false,
        },
        Err(_) => ContributorVerdict {
            name,
            readiness: Readiness::NotReady,
            panicked: true,
        },
    }
}
