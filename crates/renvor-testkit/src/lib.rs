//! Test harness for Renvor applications.
//!
//! Lets a test start a real application, inject a failure at a chosen lifecycle phase, and assert
//! on the order that actually happened — with no HTTP client, no port, and no database. Deadlines
//! and drain budgets are exercised without real elapsed time. Phase 011 adds deterministic
//! factories, a socket-free test application (behind `http`), and — only behind `client`, for a
//! test that spawns a real binary — a blocking loopback HTTP client.
//!
//! Add this crate under `[dev-dependencies]`. Nothing in `renvor`, `renvor-core`, or
//! `renvor-config` depends on it, which is what keeps its time-control machinery out of a
//! production binary.
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** See the `renvor` facade documentation.

/// The bounded abuse-control contract. Needs `renvor-auth` but **not** its `tokens` half.
#[cfg(feature = "auth")]
pub mod abuse;
/// The socket-free test application, behind `http` (Phase 011).
#[cfg(feature = "http")]
pub mod app;
/// The blocking loopback client for tests that spawn a binary, behind `client` (Phase 011).
#[cfg(feature = "client")]
pub mod client;
pub mod clock;
pub mod concurrency;
pub mod domain;
/// Deterministic fixtures and factories (Phase 011). Driver-free.
pub mod factory;
pub mod harness;
pub mod injection;
/// The job-store contract, behind `jobs`. Runs against the memory substitute and all four rows.
#[cfg(feature = "jobs")]
pub mod jobs;
pub mod persistence;
pub mod portability;
/// The refresh-rotation contract, behind `tokens` because it names `renvor-auth`'s API token half.
///
/// A crate that does not use API tokens resolves neither this module nor the JWT dependency
/// behind it — the same rule the adapters apply to their drivers.
#[cfg(feature = "tokens")]
pub mod refresh;
pub mod upgrade;

#[cfg(feature = "http")]
pub use app::{Dispatched, ShutdownOutcome, TestApplication};
pub use clock::TestClock;
pub use concurrency::{CONCURRENT_WRITERS, MAX_ATTEMPTS};
pub use domain::{Widget, WidgetFixture};
pub use factory::{Factory, ItemDraft, ItemFactory, Sequence, UserDraft, UserFactory};
pub use harness::{Harness, HarnessRun, Outcome};
pub use injection::{Behaviour, FailureInjectionPoint};
pub use persistence::PersistenceFixture;
pub use portability::PortabilityFixture;
#[cfg(feature = "tokens")]
pub use refresh::{RefreshFixture, StoredRefreshToken};

/// Every rendering a diagnostic could leak `secret` in: the text itself, its `Debug` escape, its
/// bytes as hexadecimal (both cases), and its bytes as decimal (comma-separated, with and without
/// a space). A negative control for a failure message asserts the message contains none of them.
#[must_use]
pub fn every_rendering_of(secret: &str) -> Vec<String> {
    let bytes = secret.as_bytes();
    let decimal: Vec<String> = bytes.iter().map(ToString::to_string).collect();
    vec![
        secret.to_owned(),
        format!("{secret:?}"),
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        bytes.iter().map(|b| format!("{b:02X}")).collect::<String>(),
        decimal.join(", "),
        decimal.join(","),
    ]
}
