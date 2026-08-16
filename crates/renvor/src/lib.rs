//! # Renvor
//!
//! Facade crate for the Renvor framework.
//!
//! ## What this release exposes
//!
//! Phase 001 established governance, naming, toolchain, and repository security before any
//! runtime code existed, and this crate was empty of capability by design. **Phase 002 changes
//! that**: the transport-independent kernel is here, and this facade re-exports it.
//!
//! What is *not* here is equally deliberate. There is no HTTP, no persistence, and no CLI — by
//! requirement rather than omission (spec FR-033). The first real transport arrives in Phase 004.
//!
//! Assembly is synchronous and runs `Load`, `Validate`, and `Register`. `boot()` is `async`,
//! because bringing a provider up is; this example stops at the seam so it needs no runtime.
//!
//! ```
//! use renvor::{ApplicationBuilder, LifecyclePhase};
//!
//! let builder = ApplicationBuilder::new();
//! let phases = builder.phase_log();
//!
//! let application = builder.build().expect("an empty application assembles");
//!
//! assert_eq!(application.phase(), LifecyclePhase::Register);
//! assert_eq!(
//!     phases.entries(),
//!     vec![
//!         LifecyclePhase::Load,
//!         LifecyclePhase::Validate,
//!         LifecyclePhase::Register,
//!     ],
//! );
//! // `application.boot().await` reaches `Ready`, or returns a `BootFailure` having rolled back.
//! ```
//!
//! ## This crate contains no implementation
//!
//! Everything below the metadata constants is a `pub use`. The facade decides **what is public**,
//! never **how it behaves** — so there is no second copy of a behaviour to drift from the first,
//! and no place for a subtly different variant of a kernel type to appear. A test asserts this
//! mechanically rather than trusting the convention.
//!
//! ## Stability
//!
//! **This surface is explicitly unstable** (FR-036). Breaking changes are permitted without a
//! compatibility procedure and no semantic-versioning promise applies while that window is open.
//! It closes when the conditions in the specification's API-instability end gate are met, which
//! cannot happen before Phase 004 has exercised the kernel through a real transport.
//!
//! ## The command is `renover`
//!
//! The product is **Renvor**. The installed executable is **`renover`**. The
//! difference is deliberate and permanent — see `decisions/0001-public-naming-and-namespace.md`.
//! It is not a typographical error.
//!
//! ## Support
//!
//! The minimum supported Rust version is **1.94.0**, a fixed floor rather than a
//! rolling offset from stable. See `SUPPORT.md` for the full policy.
//!
//! ## Licence
//!
//! `MIT OR Apache-2.0`, at your option. Project code generated for you by Renvor
//! tooling carries no Renvor licensing obligation and is yours outright.

/// The version of this crate, taken from the package manifest at compile time.
///
/// ```
/// assert!(!renvor::VERSION.is_empty());
/// ```
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The minimum supported Rust version this crate is verified against.
///
/// A fixed support floor, not an offset from current stable. Raising it requires a
/// planned minor or major release and an accepted decision record — see
/// `decisions/0003-msrv-toolchain-and-dependency-policy.md`.
///
/// ```
/// assert_eq!(renvor::MSRV, "1.94.0");
/// ```
pub const MSRV: &str = "1.94.0";

/// The name of the executable installed by `renvor-cli`.
///
/// Present so that documentation, tests, and diagnostics reference one constant
/// rather than repeating a string that readers mistake for a misspelling.
///
/// ```
/// assert_eq!(renvor::EXECUTABLE, "renover");
/// ```
pub const EXECUTABLE: &str = "renover";

// ---------------------------------------------------------------------------------------------
// The User Story 1 surface: assembling an application, and finding out why one refused to start.
//
// Narrow on purpose. Every name here is one an author writing a provider or starting an
// application needs; nothing is re-exported merely because it is public in the kernel. A facade
// that re-exports everything is not a facade, it is a second spelling of the same crate — and it
// makes every kernel-internal rename a breaking change for consumers who never used the type.
// ---------------------------------------------------------------------------------------------

/// The kernel, for the surface this facade deliberately does not re-export.
///
/// Named `kernel` rather than `core` because an item called `core` at a crate root shadows the
/// `core` library in paths written inside dependent code.
pub use renvor_core as kernel;

// Assembling and running an application.
pub use renvor_core::{
    Application, ApplicationBuilder, BootFailure, BuildError, DrainOutcome, LifecyclePhase,
    PhaseLog, RollbackReport, ShutdownReport, WorkGate, WorkPermit,
};

// Writing a provider.
pub use renvor_core::provider::{
    CapabilityId, InitContext, InitialisationOrder, Provider, ProviderFuture, ProviderId,
    ProviderRegistry, ResolutionReport,
};

// Diagnosing a failure, without leaking what failed.
pub use renvor_core::{ErrorCategory, KernelError};

// Cancellation, typed state, and the observability primitives an author supplies.
pub use renvor_core::{
    CancelScope, EntropySource, OsEntropy, ProviderScope, RunIdentifier, TypedStateMap,
};

// Health and readiness — two questions with two answers (FR-026).
pub use renvor_core::{HealthState, Liveness, Readiness, ReadinessContributor, ReadinessReport};

// The configuration *port*. The implementation is behind the `config` feature below; this is the
// shape the kernel speaks, and it carries no parser.
pub use renvor_core::config_port::{ConfigSource, SourceLayer};

/// Typed, layered configuration.
///
/// Behind the default-on `config` feature. Taking this crate with `default-features = false`
/// resolves **no** parser, derive macro, or secret crate — the whole point of the split, asserted
/// with a positive control in both directions rather than merely intended.
#[cfg(feature = "config")]
pub use renvor_config as config;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_matches_the_manifest() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn msrv_is_the_declared_fixed_floor() {
        assert_eq!(MSRV, "1.94.0");
    }

    #[test]
    fn executable_differs_from_the_product_name() {
        // Guards the ADR-0001 decision against a well-meaning "spelling fix".
        assert_eq!(EXECUTABLE, "renover");
        assert_ne!(EXECUTABLE, "renvor");
    }

    #[test]
    fn the_facade_declares_no_implementation_of_its_own() {
        // ADR-0002. The facade decides what is public, never how it behaves. This reads the
        // crate's own source and stops at the test module, because tests are implementation and
        // are allowed to be.
        let source = include_str!("lib.rs");
        let surface = source
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields at least one part");

        // Line-oriented rather than substring-oriented, because a declaration nested inside a
        // module is indented and a `"\nfn "` search would walk straight past it. `pub const` is
        // permitted: the three metadata constants above are facts about the package, not
        // behaviour.
        let declarations = |text: &str| -> Vec<String> {
            text.lines()
                .map(str::trim_start)
                .filter(|line| !line.starts_with("//"))
                .map(|line| {
                    line.trim_start_matches("pub(crate) ")
                        .trim_start_matches("pub ")
                })
                .filter(|line| {
                    [
                        "fn ",
                        "struct ",
                        "enum ",
                        "trait ",
                        "impl ",
                        "union ",
                        "macro_rules!",
                    ]
                    .iter()
                    .any(|keyword| line.starts_with(keyword))
                })
                .map(str::to_owned)
                .collect()
        };

        assert!(
            declarations(surface).is_empty(),
            "the facade declared behaviour of its own, which belongs in a kernel crate: {:?}",
            declarations(surface)
        );

        // POSITIVE CONTROL: the same scan finds the declarations in the test half — which are
        // indented, and are exactly what the weaker column-0 search missed. Its silence above
        // therefore means "absent" rather than "the scan does not work".
        let tests = &source[surface.len()..];
        assert!(
            declarations(tests).len() >= 4,
            "the scan found {} declarations in the test module, which cannot be right",
            declarations(tests).len()
        );
    }

    #[test]
    fn the_re_exported_kernel_types_are_the_kernel_types() {
        // Not a tautology: if the facade ever shadowed a name with a local definition, this stops
        // compiling. Type identity is the assertion; the value is incidental.
        let _: fn() -> ApplicationBuilder = renvor_core::ApplicationBuilder::new;
        let _: LifecyclePhase = renvor_core::LifecyclePhase::Load;
        let _: ErrorCategory = renvor_core::ErrorCategory::Internal;
    }
}
