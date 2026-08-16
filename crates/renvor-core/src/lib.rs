//! Transport-independent application kernel for Renvor.
//!
//! This crate owns the parts of an application that have nothing to do with how it talks to the
//! outside world: the lifecycle, the provider graph, typed state, cancellation, health, and the
//! error taxonomy. There is no HTTP here, no persistence, and no CLI — by requirement, not by
//! omission (spec FR-033).
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** Breaking changes are permitted without a
//! compatibility procedure, and no semantic-versioning promise applies while the instability
//! window is open. See the `renvor` facade documentation for the conditions that close it.

pub mod config_port;
pub mod error;
pub mod lifecycle;
pub mod observe;
pub mod provider;

pub use config_port::{ConfigResolver, ResolvedConfig, SourceLayer};
pub use error::{ErrorCategory, KernelError};
pub use lifecycle::LifecyclePhase;
pub use observe::{EntropySource, OsEntropy, RunIdentifier};
