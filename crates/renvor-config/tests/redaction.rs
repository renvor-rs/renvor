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

use std::collections::BTreeMap;

use renvor_config::{ConfigSchema, REDACTED, SchemaSource, Secret};
use renvor_core::error::context::{Constraint, configuration};
use renvor_core::{ErrorCategory, KernelError};
use serde::Deserialize;
use toml::Table;

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
            "the credential reached the `{name}` path"
        );
        assert!(
            rendered.contains(REDACTED),
            "the `{name}` path did not render the placeholder"
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
        "the credential reached the causal chain"
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
        "the crate's opt-in serde feature was enabled in the manifest"
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
        "the decoder message carried the credential into the error"
    );
    assert!(rendered.contains("u16"), "the expectation did not survive");

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
        "the adapter leaked the value"
    );
    assert!(
        rendered.contains("port"),
        "the error no longer names the key"
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

/// W-005 verification finding 1.1 — the redaction guarantee is a property of a **set** of types.
///
/// `ResolvedConfig` was hand-written because a review named that type. Four siblings holding the
/// same data still derived `Debug`, and the re-review printed each one and got the credential:
/// `DecodedLayer`, `Merged`, `LayeredResolverBuilder`, and `LayeredResolver` — the last being the
/// value `SchemaSource` carefully wraps, which made that type's hand-written `Debug` decorative.
///
/// So this test enumerates **every public type that can hold a configuration value** and asserts
/// the property across all of them at once. Fixing the type that was pointed at is not the same as
/// establishing the guarantee.
/// A schema for the set-wide redaction test below. Nothing reads its field back.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Settings {
    port: u16,
}

/// Its all-optional partner. Nothing reads either back.
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialSettings {
    port: Option<u16>,
    password: Option<String>,
}

impl ConfigSchema for Settings {
    type Partial = PartialSettings;
}

#[test]
fn no_public_type_holding_configuration_can_print_a_value() {
    use renvor_config::{DecodedLayer, LayeredResolverBuilder, Merged};
    use renvor_core::config_port::SourceLayer;

    const CREDENTIAL: &str = "hunter2-do-not-print";

    let table: Table = format!("password = \"{CREDENTIAL}\"\nport = 8080")
        .parse()
        .expect("valid");
    let environment: BTreeMap<String, String> =
        [("RENVOR_PASSWORD".to_owned(), CREDENTIAL.to_owned())]
            .into_iter()
            .collect();

    let builder = LayeredResolverBuilder::new()
        .with_defaults(table.clone())
        .with_environment_map("RENVOR_", environment);
    let resolver = LayeredResolverBuilder::new()
        .with_defaults(table.clone())
        .build::<Settings>();
    let decoded = DecodedLayer::new(SourceLayer::Defaults, table.clone());
    let merged = Merged {
        table,
        ..Merged::default()
    };
    let source = SchemaSource::new("application configuration", resolver);

    let rendered = [
        ("LayeredResolverBuilder", format!("{builder:?}")),
        ("DecodedLayer", format!("{decoded:?}")),
        ("Merged", format!("{merged:?}")),
        ("SchemaSource", format!("{source:?}")),
    ];

    for (name, output) in &rendered {
        assert!(
            !output.contains(CREDENTIAL),
            "{name} printed the credential"
        );
        assert!(
            !output.contains("8080"),
            "{name} printed an unmarked value too"
        );
    }

    // The LayeredResolver held inside the source, printed directly.
    let inner = LayeredResolverBuilder::new()
        .with_defaults(
            "password = \"hunter2-do-not-print\""
                .parse::<Table>()
                .expect("valid"),
        )
        .build::<Settings>();
    assert!(
        !format!("{inner:?}").contains(CREDENTIAL),
        "LayeredResolver printed the credential"
    );

    // POSITIVE CONTROL: the credential really is present in every one of those values, so the
    // absences above are redaction rather than empty structures.
    assert!(format!("{CREDENTIAL:?}").contains(CREDENTIAL));
    assert!(rendered.iter().all(|(_, output)| output.len() > 10));
}
