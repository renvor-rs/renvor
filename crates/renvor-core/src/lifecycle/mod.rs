//! Application lifecycle: the phase sequence and the guarantees each phase makes on failure.
//!
//! The phase order itself lives in [`phase`], separated because it is the one part of the
//! lifecycle other modules depend on. The error taxonomy names a phase, and so do spans; neither
//! should have to depend on the lifecycle *runner* to say which phase it is talking about.
//!
//! # The shape of a start
//!
//! ```text
//!   ApplicationBuilder ──build()──► Application ──boot()──► Application
//!            │                          │                      (Ready)
//!            │ BuildError               │ BootFailure
//!            ▼                          ▼
//!   nothing started              rolled back in reverse
//!   (0 providers booted)         actual initialisation order
//! ```
//!
//! Two calls rather than one, because they fail differently and the difference is the point.
//! `build()` cannot have started a provider — the code that starts one lives in `boot()` — so
//! FR-017's "0 providers, 0 listeners" is a property of where the code is, not a claim a test has
//! to keep re-checking. `boot()` can, so its failure type carries a rollback report.

pub mod application;
pub mod builder;
pub mod phase;
pub mod rollback;

pub use application::{
    Application, DEFAULT_DRAIN_BUDGET, InitialisedProvider, PhaseCursor, PhaseLog,
};
pub use builder::{ApplicationBuilder, BuildError};
pub use phase::LifecyclePhase;
pub use rollback::{BootFailure, RollbackReport};
