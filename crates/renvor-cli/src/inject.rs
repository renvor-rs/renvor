//! Deliberate failure injection for the transaction tests. **Absent from release builds.**
//!
//! # Why this exists rather than a mock filesystem
//!
//! Contract C-5's guarantee is *"a failure anywhere before placement leaves the destination exactly
//! as it was and leaves no staging behind"*. The only honest way to test that is to make a **real**
//! run of the **real** binary fail at a chosen step and then look at the real filesystem. A mock
//! proves the mock cleans up.
//!
//! # Why the body is `cfg(debug_assertions)` rather than a hidden flag
//!
//! The same reasoning as `RENVOR_PANIC_FOR_TESTS` in `main.rs`: a hidden flag that ships is test
//! scaffolding in production code, and an operator who discovers it has a supported way to make
//! generation fail. With the body compiled out, a release binary never reads the variable — the
//! machinery is **not present**, rather than present and switched off.
//!
//! The distinction matters and is measured: `tests/transaction.rs` asserts that a run with
//! `RENVOR_FAIL_AT` set to a step name **succeeds** when the injector is compiled out, which is
//! what a release build does.

use crate::exit::{CliError, Code};

/// Fails if `RENVOR_FAIL_AT` names this step.
///
/// Called at the end of each mutating step of contract C-5, so the injected failure happens with
/// that step's effects already in place — which is the state the cleanup guarantee is about.
///
/// # Errors
///
/// [`Code::Internal`] naming the step, only ever in a debug build with the variable set.
pub fn fail_at(step: &str) -> Result<(), CliError> {
    #[cfg(debug_assertions)]
    {
        if std::env::var("RENVOR_FAIL_AT").is_ok_and(|requested| requested == step) {
            return Err(CliError::new(
                Code::Internal,
                format!("deliberate failure injected at `{step}` by RENVOR_FAIL_AT"),
            )
            .with("injected", step));
        }
    }
    let _ = step;
    Ok(())
}

/// An injected `io::Error` for a step that fails at the **filesystem** level rather than at the
/// contract level.
///
/// # Why this is separate from [`fail_at`]
///
/// [`fail_at`] returns a [`CliError`], which is the right shape for "this step of contract C-5
/// failed". `Staging::create`'s double-failure path is not that: it needs `open_dir` to fail the
/// way the operating system fails it, and then the **cleanup** to fail as well, so that the two
/// errors can be reported together. Only an `io::Error` can stand in for either.
///
/// Reachable steps: `staging-open` and `staging-cleanup`. Both are `cfg(debug_assertions)`, so a
/// release binary never reads the variable.
///
/// # Why this one accepts a COMMA-SEPARATED list and [`fail_at`] does not
///
/// The case worth testing is the **double** failure — `open_dir` fails *and* the cleanup that
/// follows it fails too — because that is the one where a staging directory survives and the
/// message has to say so. One environment variable holding one step name cannot express it, so
/// this splits on `,` and matches any element: `RENVOR_FAIL_AT=staging-open,staging-cleanup`.
///
/// [`fail_at`] keeps exact-match semantics deliberately. Its own control test asserts that an
/// unrecognised step is inert, and loosening it to substring or list matching would weaken that.
#[must_use]
pub fn io_failure(step: &str) -> Option<std::io::Error> {
    #[cfg(debug_assertions)]
    {
        if std::env::var("RENVOR_FAIL_AT").is_ok_and(|requested| {
            requested
                .split(',')
                .any(|requested| requested.trim() == step)
        }) {
            return Some(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("deliberate `{step}` failure injected by RENVOR_FAIL_AT"),
            ));
        }
    }
    let _ = step;
    None
}
