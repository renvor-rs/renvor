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

pub mod cancel;
pub mod config_port;
pub mod error;
pub mod health;
pub mod lifecycle;
pub mod observe;
pub mod provider;
pub mod state;

pub use cancel::{CancelScope, ProviderScope};
pub use config_port::{ConfigResolver, ConfigSource, ResolvedConfig, SourceLayer};
pub use error::{ErrorCategory, KernelError};
pub use health::{
    ContributorFault, ContributorVerdict, HealthState, Liveness, Readiness, ReadinessContributor,
    ReadinessReport,
};
pub use lifecycle::{
    Application, ApplicationBuilder, BootFailure, BuildError, DrainOutcome, LifecyclePhase,
    PhaseLog, RollbackReport, ShutdownReport, WorkGate, WorkPermit,
};
pub use observe::{EntropySource, OsEntropy, RunIdentifier};
pub use provider::{
    CapabilityId, InitContext, InitialisationOrder, Provider, ProviderId, ProviderRegistry,
    ResolutionReport,
};
pub use state::TypedStateMap;
