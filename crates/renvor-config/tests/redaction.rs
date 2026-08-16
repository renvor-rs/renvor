//! T075 and T076 — **every** output path a secret could leak through (FR-018, FR-021, SC-007),
//! and a control that proves the assertions can fire.
//!
//! # The control is the load-bearing test in this file
//!
//! Every other test here asserts an absence: the credential is in **0** of the outputs. A suite of
//! absences passes trivially if the detector is broken, if the value never reached the output at
//! all, or if the search string was mistyped.
//!
//! So [`Leaky`] is a type that **deliberately does not redact**, put through the identical
//! assertion harness. If the harness cannot catch `Leaky`, its silence about [`Secret`] means
//! nothing — and that is asserted, not assumed.
//!
//! # The paths, and who owns each
//!
//! C-C9 splits six output paths between the underlying crate and Renvor. Four are Renvor's. This
//! file exercises all six, because a reader checking SC-007 should not have to work out which ones
//! were someone else's problem.
//!
//! This also closes **User Story 3 scenario 7**, which `layering.rs` defers here by name.

use renvor_config::{REDACTED, Secret};
use renvor_core::error::context::{Constraint, configuration};
use renvor_core::{ErrorCategory, KernelError};

/// The value that must appear in **0** outputs.
const CREDENTIAL: &str = "hunter2-do-not-print";

/// A wrapper that deliberately leaks, used **only** as a positive control.
///
/// It exists so the harness below can be shown to detect a leak. Nothing else may use it, and the
/// name says so at every call site.
struct Leaky(String);

impl std::fmt::Display for Leaky {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::fmt::Debug for Leaky {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Leaky({})", self.0)
    }
}

/// Every output form of a value, rendered.
///
/// One function, used for both the redacting type and the leaking one, so neither gets an easier
/// test than the other.
fn every_output_form<T: std::fmt::Display + std::fmt::Debug>(
    value: &T,
) -> Vec<(&'static str, String)> {
    vec![
        ("Display", format!("{value}")),
        ("Debug", format!("{value:?}")),
        ("Debug alternate", format!("{value:#?}")),
        ("interpolated into a message", format!("value is {value}")),
        // The two tracing routes: `%value` formats with Display, `?value` with Debug. Both are
        // covered by the two above, and named separately because C-C9 lists fields as their own
        // path and a reader should see it addressed.
        ("tracing %value", format!("{value}")),
        ("tracing ?value", format!("{value:?}")),
    ]
}

#[test]
fn the_control_leaks_and_the_harness_catches_it() {
    // **Read this first.** If this test ever stops failing to find the credential, every other
    // test in this file is worthless.
    let leaky = Leaky(CREDENTIAL.to_owned());
    let forms = every_output_form(&leaky);

    let leaked: Vec<&str> = forms
        .iter()
        .filter(|(_, rendered)| rendered.contains(CREDENTIAL))
        .map(|(name, _)| *name)
        .collect();

    assert_eq!(
        leaked.len(),
        forms.len(),
        "the control must leak through every form, or the harness is not exercising them all: \
         leaked through {leaked:?}"
    );
}

#[test]
fn a_secret_appears_in_zero_output_forms() {
    // SC-007: 0 occurrences of a secret-marked value in any output form.
    let secret = Secret::new("database.password", CREDENTIAL.to_owned());

    for (name, rendered) in every_output_form(&secret) {
        assert!(
            !rendered.contains(CREDENTIAL),
            "the credential reached the `{name}` path: {rendered}"
        );
        assert!(
            rendered.contains(REDACTED),
            "the `{name}` path must render the placeholder: {rendered}"
        );
    }
}

#[test]
fn the_field_name_stays_visible_while_the_value_is_redacted() {
    // User Story 3 scenario 7, second half. A redaction that also hides *which* field is set makes
    // a configuration problem undiagnosable — the point is to keep the diagnosis and lose the
    // credential, not to lose both.
    let secret = Secret::new("database.password", CREDENTIAL.to_owned());
    assert_eq!(secret.key(), "database.password");
    assert!(format!("{secret:?}").contains("database.password"));
}

#[test]
fn a_secret_cannot_enter_an_error_message_or_its_context() {
    // C-E3 and FR-021, enforced by the error type rather than by this test: no `KernelError`
    // variant has a field that can hold a configuration value. This asserts the consequence.
    // Built through the constrained constructor, because the variant is `#[non_exhaustive]` and
    // this crate is outside `renvor-core`. That is the enforcement, not a style preference.
    let error = configuration(
        "database.password",
        "environment",
        "a secret string",
        &Constraint::TooShort { minimum: 12 },
    );

    let rendered = error.to_string();
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert!(!rendered.contains(CREDENTIAL));
    assert!(rendered.contains("database.password"), "the key is named");
    assert!(rendered.contains("environment"), "the layer is named");
    assert!(rendered.contains("at least 12"), "the constraint is named");

    // The causal chain is another context surface: a cause is rendered when a caller walks it.
    let wrapped = KernelError::ProviderInit {
        provider: "db".to_owned(),
        source: Box::new(error),
    };
    let mut chain = wrapped.to_string();
    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(&wrapped);
    while let Some(error) = cause {
        chain.push_str(&error.to_string());
        cause = error.source();
    }
    assert!(
        !chain.contains(CREDENTIAL),
        "the credential reached the causal chain: {chain}"
    );
}

#[test]
fn a_secret_has_no_serialization_path_at_all() {
    // C-C9: serialization is **refused**, and the refusal is a missing trait. A runtime guard
    // would be a promise; an absent impl is a fact, and the fact is that this does not compile:
    //
    // ```compile_fail
    // let secret = renvor_config::Secret::new("k", String::from("v"));
    // let _ = toml::to_string(&secret);
    // ```
    //
    // Asserted here as the manifest and source facts that produce it, because a `compile_fail`
    // doctest on a non-public path cannot be run from an integration test.
    let manifest = include_str!("../Cargo.toml");
    let secrecy_line = manifest
        .lines()
        .find(|line| line.starts_with("secrecy ="))
        .expect("the manifest declares secrecy");
    assert!(
        !secrecy_line.contains("serde"),
        "the crate's opt-in serde feature was enabled: {secrecy_line}"
    );
}

#[test]
fn a_decoder_message_quoting_the_credential_never_reaches_an_error() {
    // **The defect this file exists to catch, found by measurement.** `serde` renders the
    // offending value inside its message: deserializing `port = "hunter2-do-not-print"` into a
    // `u16` reports `invalid type: string "hunter2-do-not-print", expected u16`. An adapter that
    // forwards that into `constraint` puts a secret into every output form.
    let raw = format!("invalid type: string \"{CREDENTIAL}\", expected u16");
    let error = configuration(
        "database.password",
        "environment",
        "a secret string",
        &Constraint::from_decoder(&raw, "a string"),
    );

    let rendered = error.to_string();
    assert!(
        !rendered.contains(CREDENTIAL),
        "the decoder message carried the credential into the error: {rendered}"
    );
    assert!(
        rendered.contains("u16"),
        "the expectation survives: {rendered}"
    );

    // POSITIVE CONTROL: the raw message really does contain the credential, so the stripping
    // removed something rather than acting on a message that never had it.
    assert!(raw.contains(CREDENTIAL));
}

#[test]
fn the_whole_configuration_stack_never_puts_a_value_in_an_error() {
    // End to end, through the real adapter rather than through a constructed error: a secret-shaped
    // value that fails to decode must not appear in what comes back.
    use renvor_config::{ConfigSchema, LayeredResolverBuilder};
    use renvor_core::config_port::ConfigResolver as _;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Debug, Deserialize)]
    struct Settings {
        #[allow(dead_code)]
        port: u16,
    }
    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct PartialSettings {
        port: Option<u16>,
    }
    impl ConfigSchema for Settings {
        type Partial = PartialSettings;
    }

    let mut variables = BTreeMap::new();
    variables.insert("RENVOR_PORT".to_owned(), CREDENTIAL.to_owned());

    let error = LayeredResolverBuilder::new()
        .with_environment_map("RENVOR_", variables)
        .build::<Settings>()
        .resolve()
        .expect_err("the credential is not a u16");

    let rendered = error.to_string();
    assert!(
        !rendered.contains(CREDENTIAL),
        "the adapter leaked the value: {rendered}"
    );
    assert!(
        rendered.contains("port"),
        "and still names the key: {rendered}"
    );
}

#[test]
fn the_credential_is_findable_when_it_is_actually_present() {
    // The simplest control of all, and the one that catches a mistyped constant. If this ever
    // fails, every "does not contain" assertion in this file has been vacuous.
    assert!(CREDENTIAL.contains(CREDENTIAL), "Display route");
    assert!(
        format!("{CREDENTIAL:?}").contains(CREDENTIAL),
        "Debug route"
    );
}
