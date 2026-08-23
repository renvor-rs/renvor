//! Shared harness for the real-database contract suite.
//!
//! # A skipped test is never reported as a gate
//!
//! These tests need a real PostgreSQL and a real MySQL. When the environment does not provide one
//! they **skip with a printed message** naming the variable that would enable them. `PLAN.md` §17
//! forbids describing an ignored test as a gate, so the phase evidence names the command that ran
//! them rather than the command that could have.

#![allow(dead_code)]

use std::time::Duration;

use renvor_database::{ConnectionString, PoolSettings};

/// The variable naming a PostgreSQL instance.
pub const POSTGRES_URL: &str = "RENVOR_TEST_POSTGRES_URL";
/// The variable naming a MySQL instance.
pub const MYSQL_URL: &str = "RENVOR_TEST_MYSQL_URL";

/// Reads a database URL, or explains why the test is being skipped.
///
/// Returns `None` after printing, so the caller returns rather than failing.
pub fn url(variable: &str) -> Option<ConnectionString> {
    match std::env::var(variable) {
        Ok(value) if !value.is_empty() => Some(ConnectionString::new(value)),
        _ => {
            println!("SKIPPED: set {variable} to run this test against a real database");
            None
        }
    }
}

/// Small, fast, bounded settings for tests.
///
/// The pool is deliberately tiny so that exhaustion is reachable in a test rather than theoretical.
pub fn settings() -> PoolSettings {
    PoolSettings::default()
        .with_max_connections(4)
        .expect("bounded")
        .with_acquire_timeout(Duration::from_secs(5))
        .expect("bounded")
        .with_connect_timeout(Duration::from_secs(5))
        .expect("bounded")
}

/// A recognisable value that must never appear in output.
///
/// The same construction Phase 005 used for its redaction proofs: assert the **absence** of a
/// string chosen to be unmistakable, rather than the presence of a redaction marker.
pub const CREDENTIAL_CANARY: &str = "hunter2CanaryDoNotLeak";

/// Drives one provider's `initialise` with a throwaway kernel context.
///
/// A helper rather than six copies: building an `InitContext` needs five kernel values that have
/// nothing to do with what these tests assert, and repeating that in every test would bury the one
/// line that matters.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
pub async fn initialise<P: renvor_core::provider::registry::Provider>(
    provider: &P,
) -> Result<(), renvor_core::error::BoxedCause> {
    use renvor_core::provider::registry::{InitContext, ProviderId};

    let id = ProviderId::new("test");
    let mut state = renvor_core::state::TypedStateMap::new();
    let cancel = renvor_core::cancel::CancelScope::root();
    let work = renvor_core::lifecycle::WorkGate::new();
    let health = renvor_core::HealthState::new();
    let run_id = renvor_core::observe::RunIdentifier::generate(
        &renvor_core::observe::OsEntropy::new(),
    )
    .expect("the OS CSPRNG is available");
    let mut context = InitContext::new(&id, &mut state, &cancel, &work, &health, run_id);
    provider.initialise(&mut context).await
}
