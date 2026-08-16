//! T112 — a non-Unicode environment entry is refused or ignored, and **never** panics.
//!
//! # Why these tests need a second process
//!
//! The failure being tested is a property of the *real* process environment, and this workspace
//! declares `unsafe_code = "forbid"` while `std::env::set_var` is `unsafe` in edition 2024. There
//! is no safe way to put a non-Unicode variable into this process. So each test runs twice: once
//! as the **parent**, which re-executes this same test binary with the hostile variable attached,
//! and once as the **child**, which does the actual reading and asserts.
//!
//! # The control that makes the rest mean anything
//!
//! `libtest` exits **0** when a filter matches **zero** tests. A child that ran nothing at all
//! would therefore look exactly like a child that passed, and every assertion below would be
//! vacuous. [`assert_child_ran`] reads the child's own summary line and requires `1 passed`, so a
//! renamed test fails loudly instead of silently proving nothing.
//!
//! # Platform
//!
//! `#![cfg(unix)]`, not `cfg(target_os = "linux")`. Linux is the claimed platform and Ubuntu CI is
//! the authority, but building a non-Unicode `OsString` is a `unix` capability — so these also run
//! on the maintainer's macOS workstation, where they were developed and first observed to fail
//! against `std::env::vars`.

#![cfg(unix)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Output};

use renvor_config::{ConfigSchema, LayeredResolverBuilder, SchemaSource};
use renvor_core::ErrorCategory;
use renvor_core::config_port::{ConfigResolver as _, ConfigSource as _};
use serde::Deserialize;
use toml::Table;

/// Set on the child only. Deliberately outside [`PREFIX`] so the child's own marker is not itself
/// read as configuration.
const ROLE: &str = "RENVOR_CHILD_ROLE";

/// The prefix under test. Distinctive enough that nothing in a CI environment collides with it.
const PREFIX: &str = "RENVOR_ENVTEST_";

/// Stands in for a credential. Valid ASCII, so an implementation that leaked the value really
/// would put these bytes in the message — which is what makes the redaction assertion a test
/// rather than a restatement of "invalid bytes cannot be printed".
const SECRET: &str = "hunter2-do-not-print";

#[derive(Debug, Deserialize)]
struct Settings {
    port: u16,
}

/// The partial form exists to be decoded into, never read from.
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialSettings {
    port: Option<u16>,
}

impl ConfigSchema for Settings {
    type Partial = PartialSettings;
}

/// Whether this process is the child doing the reading.
fn is_child() -> bool {
    std::env::var_os(ROLE).is_some()
}

/// Builds an `OsString` from raw bytes that are not valid UTF-8.
fn raw(bytes: &[u8]) -> OsString {
    OsString::from_vec(bytes.to_vec())
}

/// Re-executes this test binary, running only `test_name`, with `hostile` in its environment.
fn spawn_child(test_name: &str, hostile: &[(OsString, OsString)]) -> Output {
    let executable = std::env::current_exe().expect("a test binary knows its own path");
    let mut command = Command::new(executable);
    command
        .args(["--exact", "--nocapture", "--test-threads=1", test_name])
        .env(ROLE, "child");
    for (name, value) in hostile {
        command.env(name, value);
    }
    command.output().expect("the child test binary runs")
}

/// Requires the child to have passed **and to have actually run one test**.
///
/// The second half is the control described in the module documentation.
fn assert_child_ran(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child process failed\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("1 passed"),
        "the child matched no test, so it proved nothing — check the test name passed to \
         `spawn_child`\n--- stdout ---\n{stdout}"
    );
}

/// A resolver over [`PREFIX`], reading the **real** process environment.
fn resolver() -> renvor_config::LayeredResolver<Settings> {
    LayeredResolverBuilder::new()
        .with_defaults("port = 80".parse::<Table>().expect("valid"))
        .with_environment(PREFIX)
        .build::<Settings>()
}

#[test]
fn an_unrelated_non_unicode_variable_is_ignored_by_both_entry_points() {
    if is_child() {
        // FR-025/SC-004: somebody else's malformed variable is not Renvor's problem, and must not
        // be Renvor's crash either. Reaching the assertions at all proves nothing panicked.
        let resolved = resolver()
            .resolve()
            .expect("an unrelated variable must not affect resolution");
        assert_eq!(resolved.value().port, 80, "the defaults layer still wins");

        // The second entry point, asserted separately because it is a different call path: the
        // lifecycle reaches the environment through `SchemaSource::load`, not through `resolve`.
        let source = SchemaSource::new("application configuration", resolver());
        source
            .load()
            .expect("the lifecycle path must not panic either");
        return;
    }

    let output = spawn_child(
        "an_unrelated_non_unicode_variable_is_ignored_by_both_entry_points",
        &[
            (raw(b"UNRELATED_NAME_\xFF"), raw(b"\xFF\xFE")),
            (raw(b"ANOTHER_ONE"), raw(b"value-\xC3\x28")),
        ],
    );
    assert_child_ran(&output);
}

#[test]
fn a_prefixed_variable_with_an_unrepresentable_value_is_refused_without_exposing_it() {
    if is_child() {
        let error = resolver()
            .resolve()
            .expect_err("a prefixed variable that cannot be represented must be refused");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        let rendered = error.to_string();
        assert!(
            rendered.contains("RENVOR_ENVTEST_PORT"),
            "the error must name the variable so it is actionable: {rendered}"
        );
        assert!(
            !rendered.contains(SECRET),
            "the value reached the error message: {rendered}"
        );

        // The lifecycle path refuses the same way rather than panicking.
        let source = SchemaSource::new("application configuration", resolver());
        let error = source.load().expect_err("the lifecycle path refuses too");
        assert!(!error.to_string().contains(SECRET), "{error}");
        return;
    }

    let mut value = SECRET.as_bytes().to_vec();
    value.push(0xFF);
    let output = spawn_child(
        "a_prefixed_variable_with_an_unrepresentable_value_is_refused_without_exposing_it",
        &[(OsString::from("RENVOR_ENVTEST_PORT"), raw(&value))],
    );
    assert_child_ran(&output);
}

#[test]
fn a_prefixed_variable_with_an_unrepresentable_name_is_refused() {
    if is_child() {
        let error = resolver()
            .resolve()
            .expect_err("a prefixed name that cannot be represented must be refused");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        let rendered = error.to_string();
        assert!(
            rendered.contains("RENVOR_ENVTEST_"),
            "the readable part of the name must survive so the operator can find it: {rendered}"
        );
        assert!(
            !rendered.contains(SECRET),
            "the value reached the error message: {rendered}"
        );
        return;
    }

    let mut name = b"RENVOR_ENVTEST_PORT".to_vec();
    name.push(0xFF);
    let output = spawn_child(
        "a_prefixed_variable_with_an_unrepresentable_name_is_refused",
        &[(raw(&name), raw(SECRET.as_bytes()))],
    );
    assert_child_ran(&output);
}

#[test]
fn a_well_formed_prefixed_variable_still_resolves() {
    // POSITIVE CONTROL for all three tests above. Without it, a reader cannot tell whether the
    // refusals are about malformed bytes or whether this resolver simply never succeeds — and
    // whether the child harness can distinguish a pass from a failure at all.
    if is_child() {
        let resolved = resolver().resolve().expect("a valid variable resolves");
        assert_eq!(
            resolved.value().port,
            8080,
            "the environment layer must still win over the defaults"
        );
        return;
    }

    let output = spawn_child(
        "a_well_formed_prefixed_variable_still_resolves",
        &[(
            OsString::from("RENVOR_ENVTEST_PORT"),
            OsString::from("8080"),
        )],
    );
    assert_child_ran(&output);
}
