//! Test harness for Renvor applications.
//!
//! Lets a test start a real application, inject a failure at a chosen lifecycle phase, and assert
//! on the order that actually happened — with no HTTP client, no port, and no database. Deadlines
//! and drain budgets are exercised without real elapsed time.
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
pub mod clock;
pub mod concurrency;
pub mod domain;
pub mod harness;
pub mod injection;
pub mod persistence;
pub mod portability;
/// The refresh-rotation contract, behind `tokens` because it names `renvor-auth`'s API token half.
///
/// A crate that does not use API tokens resolves neither this module nor the JWT dependency
/// behind it — the same rule the adapters apply to their drivers.
#[cfg(feature = "tokens")]
pub mod refresh;
pub mod upgrade;

pub use clock::TestClock;
pub use concurrency::{CONCURRENT_WRITERS, MAX_ATTEMPTS};
pub use domain::{Widget, WidgetFixture};
pub use harness::{Harness, HarnessRun, Outcome};
pub use injection::{Behaviour, FailureInjectionPoint};
pub use persistence::PersistenceFixture;
pub use portability::PortabilityFixture;
#[cfg(feature = "tokens")]
pub use refresh::{RefreshFixture, StoredRefreshToken};
