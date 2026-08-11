//! # Renvor
//!
//! Facade crate for the Renvor framework.
//!
//! ## This release does nothing
//!
//! Phase 001 of the Renvor programme establishes governance, naming, toolchain, and
//! repository security *before* any runtime code exists. This crate is deliberately
//! empty of capability (spec FR-047): it exists so that the workspace, the package
//! metadata, the licence policy, and the publish rehearsal are exercised against a
//! real crate rather than a hypothetical one.
//!
//! It exposes the declared version and nothing else. There is no framework here yet.
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
/// This is the single runtime-observable fact this release provides.
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
}
