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

pub mod clock;
pub mod concurrency;
pub mod domain;
pub mod harness;
pub mod injection;
pub mod persistence;
pub mod portability;
pub mod upgrade;

pub use clock::TestClock;
pub use concurrency::{CONCURRENT_WRITERS, MAX_ATTEMPTS};
pub use domain::{Widget, WidgetFixture};
pub use harness::{Harness, HarnessRun, Outcome};
pub use injection::{Behaviour, FailureInjectionPoint};
pub use persistence::PersistenceFixture;
pub use portability::PortabilityFixture;
