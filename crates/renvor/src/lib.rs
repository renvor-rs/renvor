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
//! **Phase 004 adds the first real transport**: a REST and HTTP delivery adapter, behind the
//! **off-by-default** `transport-rest` feature. A build that does not enable it resolves no HTTP
//! server, routing, or middleware crate at all — the kernel still carries none, by requirement
//! rather than omission (spec FR-033).
//!
//! There is still no persistence and no CLI in this crate.
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
//! The two event-gated closure conditions are defined solely by the public API stability
//! contract, `contracts/api-stability.md`, in the
//! [Renvor repository](https://github.com/renvor-rs/renvor).
//! No phase number forms part of either condition.
//!
//! **The window is still open.** Phase 004 satisfies the **first** of the two conditions — a real
//! transport adapter now exercises this surface — and satisfies **neither the second nor both**.
//! Closing it additionally requires an accepted decision record that supersedes ADR-0002, and no
//! such record exists. Shipping a transport is not the same event as closing the window, and this
//! paragraph says so because the first condition being met is exactly when that confusion becomes
//! available.
//!
//! ## The command is `renvor`
//!
//! The product, this crate, and the installed executable share one spelling: **`renvor`**.
//! `cargo install renvor-cli` installs a binary named `renvor` — the package name and the
//! binary name differ, as they do for `ripgrep`/`rg`.
//!
//! This replaces an earlier decision that named the executable `renover`. See
//! `decisions/0010-unify-product-and-executable-name.md`, which supersedes ADR-0001.
//!
//! ## Support
//!
//! The minimum supported Rust version is **1.94.0**, a fixed floor rather than a
//! rolling offset from stable. Linux, macOS, and Windows are supported, with
//! different enforcement levels.
//!
//! The normative policy is `contracts/support-policy.md`; `SUPPORT.md` summarises it.
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
/// Present so that documentation, tests, and diagnostics reference one constant rather than
/// repeating a literal. The package and the binary it installs are deliberately named
/// differently, which is why this constant is not simply the crate name.
///
/// ```
/// assert_eq!(renvor::EXECUTABLE, "renvor");
/// ```
pub const EXECUTABLE: &str = "renvor";

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

/// The REST and HTTP delivery adapter.
///
/// Behind the **off-by-default** `transport-rest` feature. A build without it resolves no HTTP
/// server, routing, or middleware crate — asserted in both directions, because an absence with no
/// positive control proves only that the query returned nothing.
///
/// # A deliberately narrow re-export
///
/// The names below are the ones an author declaring routes and starting a server needs. **No
/// third-party type appears in any of their public signatures** — re-exporting one would put it in
/// Renvor's public API and make every upstream major version a Renvor breaking change. That is
/// asserted mechanically by `tests/facade_boundary.rs`, not merely promised here.
///
/// The consequence is a boundary, not a slogan. The **normal** way to run an HTTP server is the
/// lifecycle-managed [`HttpServerProvider`] with [`HttpServerConfig`] and [`RouteRegistry`]: the
/// application hands Renvor its routes and Renvor owns the bind, the readiness report, the drain
/// budget, and the shutdown ordering. Every type in that path is Renvor's.
///
/// The low-level `Server` is **not** at this root. Its `serve` takes the underlying router by
/// parameter, so promoting it here would place a third-party type in Renvor's public API through
/// the front door. It remains available as [`transport::Server`] — deliberately one level down,
/// under the module that **is** the transport, where naming transport types is the point rather
/// than a leak. Reach for it, and for [`transport::route::build`], when you genuinely need to
/// drive the router yourself and accept the upstream-version coupling that comes with it.
#[cfg(feature = "transport-rest")]
pub use renvor_http as transport;

// The route-declaration and lifecycle surface, promoted to the facade root so that the common case
// reads as `renvor::{RouteRegistry, HttpServerProvider}` rather than `renvor::transport::…`.
//
// `Server` is deliberately ABSENT — see the note above and `tests/facade_boundary.rs`.
#[cfg(feature = "transport-rest")]
pub use renvor_http::{
    Admission, ClientIdentity, CorsPolicy, HostPolicy, HttpServerConfig, HttpServerProvider,
    Limits, Method, RequestContext, Response, Route, RouteGroup, RouteRegistry, TrustedProxies,
};

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
    fn executable_matches_the_product_name() {
        // ADR-0010 unified the two spellings. This previously asserted the opposite — that
        // `EXECUTABLE` must NOT equal "renvor" — to guard ADR-0001's deliberate split against
        // a well-meaning "spelling fix". That guard is inverted rather than deleted, so the
        // constant still cannot drift silently in either direction.
        assert_eq!(EXECUTABLE, "renvor");
        assert_ne!(EXECUTABLE, "renover");
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
