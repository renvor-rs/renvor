//! Application lifecycle: the phase sequence and the guarantees each phase makes on failure.
//!
//! The phase order itself lives in [`phase`], separated because it is the one part of the
//! lifecycle other modules depend on. The error taxonomy names a phase, and so do spans; neither
//! should have to depend on the lifecycle *runner* to say which phase it is talking about.

pub mod phase;

pub use phase::LifecyclePhase;
