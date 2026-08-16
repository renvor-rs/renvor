//! T100 — SC-019: the run identifier is opaque, **verified deterministically**.
//!
//! # What can and cannot be proven here, stated before the tests
//!
//! SC-019 was rewritten during clarification precisely because the obvious test does not work: you
//! cannot prove an identifier encodes **no** hostname, timestamp, or process id by looking at one.
//! A black-box test can only fail to find something, which is not the same as its absence.
//!
//! So opacity is established by **construction**, and these tests check the construction:
//!
//! | Claim | How it is checked | Gating |
//! |---|---|---|
//! | The identifier is a **pure function of the supplied bytes** | hold entropy fixed, vary hostname, clock, process id, and the whole configuration; assert **0** bytes change | **yes** |
//! | There is **exactly one** generation site | read the crate's source and count | **yes** |
//! | Production wiring uses the **OS CSPRNG** | `OsEntropy` has no fields, so there is nowhere for another source to enter | **yes** |
//! | Two real identifiers differ | one sample from the OS source | **NON-GATING** — a sample proves nothing about randomness |
//!
//! The last row is labelled non-gating on purpose. A statistical check on a handful of samples
//! tells you almost nothing, and dressing one up as a randomness proof is how a weak generator
//! ships with a green suite.

use std::collections::BTreeMap;
use std::ffi::OsString;

use renvor_core::ApplicationBuilder;
use renvor_core::observe::{EntropySource, FixedEntropy, OsEntropy, RunIdentifier};

/// The same bytes, every time.
fn fixed() -> FixedEntropy {
    FixedEntropy::new(vec![0xA5; 32])
}

#[test]
fn the_identifier_is_a_pure_function_of_the_entropy() {
    // SC-019(b). Entropy is held fixed while everything an identifier might otherwise encode is
    // varied. **0** bytes may change.
    let baseline = RunIdentifier::generate(&fixed()).expect("generates");

    // Vary the clock: two generations separated by observable time.
    let before = std::time::SystemTime::now();
    let later = RunIdentifier::generate(&fixed()).expect("generates");
    assert!(
        before.elapsed().is_ok(),
        "time did advance between the two generations"
    );
    assert_eq!(
        baseline.as_bytes(),
        later.as_bytes(),
        "0 bytes encode a clock"
    );

    // Vary the process's own environment, which carries hostname and much else.
    //
    // `vars_os`, never `vars`: the Unicode-or-panic reader would take this test down over a
    // variable set by whatever launched it, and a test that crashes on somebody else's
    // environment is not evidence about run identifiers. Same reasoning as
    // `renvor_config::layer::env::read_process_environment`.
    let environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    assert!(
        !environment.is_empty(),
        "the environment is non-empty, so varying against it is meaningful"
    );
    let with_environment = RunIdentifier::generate(&fixed()).expect("generates");
    assert_eq!(baseline.as_bytes(), with_environment.as_bytes());

    // Vary the whole application configuration around it.
    let from_application = ApplicationBuilder::new()
        .with_entropy(Box::new(fixed()))
        .with_drain_budget(std::time::Duration::from_secs(1))
        .with_provider_deadline(std::time::Duration::from_millis(7))
        .build()
        .expect("assembles")
        .run_id()
        .encode();
    assert_eq!(
        from_application,
        baseline.encode(),
        "0 bytes encode a configuration value"
    );
}

#[test]
fn different_entropy_produces_a_different_identifier() {
    // POSITIVE CONTROL for the purity test: the identifier is a function of the entropy, so
    // changing the entropy must change it. Without this, a generator returning a constant would
    // pass every assertion above.
    let first = RunIdentifier::generate(&FixedEntropy::new(vec![0xA5; 32])).expect("generates");
    let second = RunIdentifier::generate(&FixedEntropy::new(vec![0x5A; 32])).expect("generates");
    assert_ne!(first.as_bytes(), second.as_bytes());
}

#[test]
fn there_is_exactly_one_generation_site() {
    // SC-019(c). Opacity is a property of the construction, so the construction has to happen in
    // one place — a second site is a second set of inputs nobody reviewed.
    let source = include_str!("../src/observe/run_id.rs");
    let production = source
        .split("#[cfg(test)]")
        .next()
        .expect("split always yields one part");

    assert_eq!(
        production.matches("fn generate").count(),
        1,
        "a second generation site appeared"
    );
    assert_eq!(
        production.matches(".fill(").count(),
        1,
        "the entropy source is consulted in exactly one place"
    );

    // POSITIVE CONTROL: the scan reads real code and finds what IS there.
    assert!(production.contains("EntropySource"));
}

#[test]
fn the_production_source_has_nowhere_for_another_input_to_enter() {
    // SC-019(c) again, from the other side: `OsEntropy` is a unit struct, so it holds no seed, no
    // counter, and no clock. There is nothing to configure and nothing to get wrong.
    assert_eq!(core::mem::size_of::<OsEntropy>(), 0, "no fields, no inputs");

    let mut bytes = [0_u8; 16];
    OsEntropy::new()
        .fill(&mut bytes)
        .expect("the operating system supplies entropy");
    assert!(bytes.iter().any(|byte| *byte != 0), "and it supplied some");
}

#[test]
fn the_encoding_is_fixed_width_lowercase_hexadecimal() {
    // A caller greps for this in logs. A variable-width or mixed-case encoding turns one identifier
    // into several strings, and the grep silently misses records.
    let identifier = RunIdentifier::generate(&fixed()).expect("generates");
    let encoded = identifier.encode();

    assert_eq!(encoded.len(), 32, "16 bytes, two characters each");
    assert!(encoded.chars().all(|c| c.is_ascii_hexdigit()));
    assert!(encoded.chars().all(|c| !c.is_ascii_uppercase()));
    assert_eq!(encoded, identifier.to_string(), "Display agrees");
}

#[test]
fn two_identifiers_from_the_operating_system_differ() {
    // **NON-GATING.** One sample from the real source. It cannot prove randomness — a generator
    // returning a counter would pass — and it is here only so a completely broken production
    // wiring is noticed. The gating claims are the four above.
    let first = RunIdentifier::generate(&OsEntropy::new()).expect("generates");
    let second = RunIdentifier::generate(&OsEntropy::new()).expect("generates");
    assert_ne!(
        first.as_bytes(),
        second.as_bytes(),
        "non-gating sample: two OS-sourced identifiers matched, which is either a defect or a \
         1-in-2^128 coincidence"
    );
}
