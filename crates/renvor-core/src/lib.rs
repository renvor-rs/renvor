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
//!
//! # Supported panic strategy
//!
//! **Renvor supports the `unwind` panic strategy only.** `panic = "abort"` is unsupported, and
//! building this crate under it is a compile error rather than a runtime surprise — see the guard
//! immediately below.
//!
//! C-L9 and SC-009 require a panicking provider or readiness contributor to be contained and
//! reported as a *failure*, and both containments are built on [`std::panic::catch_unwind`], which
//! catches a panic that **unwinds**. Under `panic = "abort"` a panic calls the abort handler and
//! the process ends with no unwinding for a landing pad to intercept, so there is nothing to
//! catch — by anyone, in any crate. The containment is a property of the unwinding runtime.

// The ruling, enforced rather than described.
//
// This was a **named limitation** until T147: the documentation said abort removes containment,
// and nothing stopped anyone from building that way. A contract requiring 0 uncontained provider
// panics and a build configuration that guarantees them is a contradiction, and a contradiction
// recorded in prose is still a contradiction — a consumer who never reads this file gets a kernel
// whose central guarantee is silently absent.
//
// `cfg(panic = ...)` has been stable since Rust 1.60, well under the 1.94.0 floor, so the
// contradiction can simply be refused. A consumer who needs `panic = "abort"` needs a kernel that
// does not promise panic containment; that is a different product, not a configuration of this
// one.
#[cfg(panic = "abort")]
compile_error!(
    "renvor-core does not support `panic = \"abort\"`.\n\n\
     C-L9 and SC-009 require a panicking provider or readiness contributor to be contained and \
     reported as a failure. Both containments use `std::panic::catch_unwind`, which catches only \
     panics that UNWIND -- under `panic = \"abort\"` there is nothing to catch, so the kernel \
     would silently lose its central guarantee.\n\n\
     Remove `panic = \"abort\"` from the profile that builds this crate. Renvor sets no `panic` \
     key in any profile, so the default (`unwind`) applies unless something in your workspace \
     overrides it."
);

pub mod cancel;
mod closed_enum;
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
