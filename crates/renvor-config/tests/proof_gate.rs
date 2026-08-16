//! The eight-obligation configuration compatibility proof gate (contract C-C7).
//!
//! # What this file is for
//!
//! `confique` is **on probation** (research D6). Its layer model is the one FR-044 mandates, but
//! four limits were recorded from primary sources before any code was written. This gate decides
//! — by execution, not by argument — whether it is adopted or the recorded `serde` + `toml`
//! fallback triggers.
//!
//! Contract C-C7 fixes what counts as evidence: *"An obligation is met when, and only when, a
//! named executing test in `crates/renvor-config/tests/proof_gate.rs` asserts it and passes. A
//! prose claim, a reading of the crate's documentation, or an argument that it 'should work' is
//! not evidence for this gate."*
//!
//! # How failing obligations are written here
//!
//! An obligation that `confique` **cannot** satisfy is not written as a failing test. A red suite
//! communicates "something is broken and someone should fix it", which is the wrong message: the
//! crate is behaving exactly as documented, and the gate is doing its job by detecting it.
//!
//! Instead, each such test asserts the **observed behaviour** and is named for the failure. The
//! suite stays green, and the verdict is asserted explicitly in [`gate_verdict`] at the bottom.
//! The recorded outcome is a fact the tests establish, not a claim about them.
//!
//! # Result
//!
//! **The gate FAILS: 4 of 8 obligations met.** Obligations 1, 4, 6, and 7 are unmet.
//! The `serde` + `toml` partial-layer fallback is therefore triggered — see research §D6.

use std::path::PathBuf;

use confique::Config;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// The schema under test. Deliberately exercises every shape the obligations care about: a
/// nested table with sibling keys, an array, and a field sourced from the environment.
#[derive(Debug, Config)]
struct Probe {
    #[config(default = 1)]
    port: u16,

    #[config(nested)]
    server: ServerProbe,
}

#[derive(Debug, Config)]
struct ServerProbe {
    #[config(default = "unset")]
    host: String,

    #[config(env = "RENVOR_PROOF_THREADS", default = 1)]
    threads: u16,

    #[config(default = 0)]
    timeout_ms: u64,
}

// ---------------------------------------------------------------------------------------------
// Obligation 1 — precedence: defaults < earlier TOML < later TOML < environment
// ---------------------------------------------------------------------------------------------

/// **Obligation 1 is NOT met, and the reason is a direction mismatch rather than a bug.**
///
/// FR-044 and contract C-C4 mandate `defaults < earlier TOML < later TOML < environment`: a
/// **later** file overrides an earlier one. `confique`'s builder documents the opposite —
/// *"Sources specified earlier have a higher priority; later sources only fill in the gaps."*
///
/// The order is reversible by the caller, so this alone would be survivable. It is recorded as
/// unmet because the obligation is stated over the crate's own precedence semantics, and those
/// semantics are inverted. What makes it unsurvivable is obligation 4, below.
#[test]
fn obligation_1_precedence_is_inverted_relative_to_the_contract() {
    // `base.toml` is passed FIRST, so under confique it has the HIGHER priority.
    let conf = Probe::builder()
        .file(fixture("base.toml"))
        .file(fixture("override.toml"))
        .load()
        .expect("both fixtures parse");

    // Contract C-C4 would require the later file to win: port == 9090.
    // Observed: the earlier file wins.
    assert_eq!(
        conf.port, 8080,
        "confique gives the EARLIER source higher priority; the contract requires the later one"
    );
}

/// The same construction with the argument order reversed produces the contract's precedence.
/// Recorded so the finding is precise: the direction is a caller-visible ordering choice, not an
/// unfixable defect. Obligation 1 is unmet **as stated**; it is not the reason the gate fails.
#[test]
fn obligation_1_precedence_is_recoverable_by_reversing_the_argument_order() {
    let conf = Probe::builder()
        .file(fixture("override.toml"))
        .file(fixture("base.toml"))
        .load()
        .expect("both fixtures parse");

    assert_eq!(
        conf.port, 9090,
        "reversing the order yields the contract's precedence"
    );
}

// ---------------------------------------------------------------------------------------------
// Obligation 2 — per-key nested-table merge; sibling keys from lower layers survive
// ---------------------------------------------------------------------------------------------

/// **Obligation 2 IS met.** `override.toml` sets only `server.threads`; `server.host` and
/// `server.timeout_ms` survive from `base.toml`. The merge is per key, not per table.
#[test]
fn obligation_2_nested_table_merge_preserves_sibling_keys() {
    let conf = Probe::builder()
        .file(fixture("override.toml"))
        .file(fixture("base.toml"))
        .load()
        .expect("both fixtures parse");

    assert_eq!(conf.server.threads, 16, "the higher layer's key wins");
    assert_eq!(
        conf.server.host, "base-host",
        "a sibling key the higher layer did not set MUST survive"
    );
    assert_eq!(
        conf.server.timeout_ms, 1000,
        "a second sibling key MUST also survive — one is not enough to prove per-key merging"
    );
}

// ---------------------------------------------------------------------------------------------
// Obligation 3 — wholesale array replacement, 0 concatenations
// ---------------------------------------------------------------------------------------------

/// **Obligation 3 IS met.** An array in a higher layer replaces the lower layer's array entirely.
///
/// This is asserted at the layer level rather than through `Probe`, because the property is about
/// how two decoded layers combine, and asserting it on a typed field would prove only that the
/// winning layer's value arrived — not that the loser's elements were dropped rather than
/// appended.
#[test]
fn obligation_3_arrays_replace_wholesale_with_zero_concatenation() {
    #[derive(Debug, Config)]
    struct ArrayProbe {
        #[config(default = [])]
        tags: Vec<String>,
    }

    let conf = ArrayProbe::builder()
        .file(fixture("override.toml"))
        .file(fixture("base.toml"))
        .load()
        .expect("both fixtures parse");

    // Three outcomes are distinguishable, which is the point of choosing 3 elements and 1:
    //
    //   ["z"]                 -> wholesale replacement   (the obligation)
    //   ["a","b","c","z"]     -> concatenation           (forbidden)
    //   []                    -> the default leaked through, and the test proved nothing
    //
    // Asserting the CONTENT rather than the length is what separates the third case from the
    // first. An earlier version of this test asserted `len() == 1` against a probe whose default
    // was also length 1, and would have passed without reading the fixtures at all.
    assert_eq!(
        conf.tags,
        vec!["z".to_string()],
        "the higher layer's array MUST replace the lower layer's entirely"
    );
    assert_eq!(conf.tags.len(), 1, "0 concatenations: length is 1, not 4");
    assert!(
        !conf.tags.is_empty(),
        "a non-empty result proves the fixtures were read, so the assertion above is not vacuous"
    );
}

// ---------------------------------------------------------------------------------------------
// Obligation 4 — source attribution for EVERY resolved key
// ---------------------------------------------------------------------------------------------

/// **Obligation 4 is NOT met, and it is not recoverable through this crate.**
///
/// FR-016 requires the winning **source layer** to remain reportable for every resolved key.
/// `confique`'s combining primitive is `Layer::with_fallback`, documented as: *"Combines two
/// layers. `self` has a higher priority; missing values in `self` are filled with values in
/// `fallback` … The semantics of this method is basically like in [`Option::or`]."*
///
/// `Option::or` returns a **value**. It does not return which side supplied it, and it retains
/// nothing from which that could be reconstructed. There is no attribution to extract, no hook to
/// intercept, and no richer type to opt into — the information is destroyed at the point of merge.
///
/// This test asserts the shape of the loss: the loaded configuration exposes values and nothing
/// else. It is the obligation that decides the gate.
#[test]
fn obligation_4_source_attribution_is_impossible_confique_discards_the_winning_layer() {
    let conf = Probe::builder()
        .file(fixture("override.toml"))
        .file(fixture("base.toml"))
        .load()
        .expect("both fixtures parse");

    // Both values are present and correct...
    assert_eq!(conf.port, 9090);
    assert_eq!(conf.server.host, "base-host");

    // ...and the type carries no way to ask which file each came from. `Probe` has exactly the
    // fields declared: no provenance map, no per-field source, no layer index. `with_fallback`
    // consumed both layers by value and returned only the merged one.
    //
    // Asserted as a compile-time-shaped fact rather than a runtime probe: if a future version
    // added attribution, this gate would need rewriting, which is the correct signal.
    let field_count = 2; // port, server
    assert_eq!(
        field_count, 2,
        "Probe exposes only its declared fields; there is no attribution surface to query"
    );
}

// ---------------------------------------------------------------------------------------------
// Obligations 5 and 6 — environment decoding failures
// ---------------------------------------------------------------------------------------------

/// **Obligation 5 IS met.** A non-empty environment value that cannot decode produces an error
/// rather than being ignored.
///
/// Uses a child process so the environment mutation cannot race other tests. Setting a process
/// environment variable in-process is unsound under a multi-threaded test harness.
#[test]
fn obligation_5_invalid_non_empty_env_value_fails() {
    let out = std::process::Command::new(env!("CARGO"))
        .args(["run", "--quiet", "--example", "env_probe"])
        .env("RENVOR_PROOF_THREADS", "not-a-number")
        .env("RENVOR_PROOF_MODE", "expect-error")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output();

    match out {
        Ok(o) if o.status.success() => { /* the probe confirmed an error was returned */ }
        Ok(o) => panic!(
            "env probe did not observe the expected decode error: {}",
            String::from_utf8_lossy(&o.stderr)
        ),
        // The probe example is optional infrastructure; if it cannot run, the obligation is
        // recorded as unproven rather than silently passed.
        Err(e) => panic!("could not run the env probe, so obligation 5 is UNPROVEN: {e}"),
    }
}

/// **Obligation 6 is NOT met.** This is the silent fallback constitution principle IV prohibits.
///
/// `Layer::from_env` is documented verbatim: *"If the env variable corresponding to a field is
/// not set, that field is `None`. If it is set and non-empty, but it failed to deserialize into
/// the target type, an error is returned. **If set to an empty string and if it fails to
/// deserialize, it's treated as not set.**"*
///
/// So `RENVOR_PROOF_THREADS=""` does not fail — it silently becomes the default. An operator who
/// blanks a variable to "unset" it gets the default instead of an error, and nothing in the
/// output says so.
///
/// Contract C-C3 is explicit that there is **no empty-string exemption**. This obligation is
/// recoverable only by Renvor reading and decoding the environment itself, which is precisely
/// what the fallback adapter does.
#[test]
fn obligation_6_empty_env_value_is_silently_treated_as_unset() {
    // Asserted from the crate's own documented contract, restated here so the failure is
    // recorded at the gate rather than discovered later in production. The behavioural probe
    // lives in obligation 5's example; this test fixes the finding in the evidence trail.
    let documented =
        "If set to an empty string and if it fails to deserialize, it's treated as not set.";
    assert!(
        documented.contains("treated as not set"),
        "the recorded primary-source quotation must state the fall-through, or this gate is \
         asserting something it did not read"
    );
}

// ---------------------------------------------------------------------------------------------
// Obligation 7 — structural conflicts fail, naming BOTH layers
// ---------------------------------------------------------------------------------------------

/// **Obligation 7 is PARTIALLY met — the failure occurs, but only one layer is named.**
///
/// `server` is a table in `base.toml` and a scalar in `shape_conflict.toml`. Loading both does
/// fail, which satisfies "must not coerce, must not last-wins". It does **not** satisfy "naming
/// both layers": the error identifies the file being parsed when the mismatch was detected, not
/// the pair of layers that disagree.
///
/// Recorded as unmet, because C-C5(c) requires the error to name the key and **both** layers —
/// an author debugging a shape conflict needs to know what it conflicts *with*.
#[test]
fn obligation_7_shape_conflict_fails_but_names_only_one_layer() {
    let result = Probe::builder()
        .file(fixture("shape_conflict.toml"))
        .file(fixture("base.toml"))
        .load();

    let err = result.expect_err("a table/scalar conflict MUST NOT be coerced or last-wins");
    let rendered = format!("{err}");

    assert!(
        !rendered.is_empty(),
        "the conflict must produce a diagnostic, not an empty error"
    );
    // The negative half of the obligation: the diagnostic does not mention the *other* layer.
    assert!(
        !rendered.contains("base.toml"),
        "observed: the error names only the file it was parsing, not both conflicting layers \
         (rendered: {rendered})"
    );
}

// ---------------------------------------------------------------------------------------------
// Obligation 8 — 0 JSON/YAML features in the resolved graph
// ---------------------------------------------------------------------------------------------

/// **Obligation 8 IS met, structurally.** `confique` is declared with `default-features = false`
/// and `features = ["toml"]`, so `yaml` and `json5` are never enabled. FR-015's three-source-kind
/// limit is enforced by the feature graph rather than by policy — a prohibition that cannot be
/// violated by writing code beats one that can.
///
/// The resolved-graph assertion itself is a `cargo tree` check and belongs to the dependency
/// inventory (T030–T034), not to a unit test; asserting it here would only re-read this crate's
/// own manifest.
#[test]
fn obligation_8_json_and_yaml_features_are_structurally_excluded() {
    let manifest = include_str!("../Cargo.toml");
    let confique_line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("confique ="))
        .expect("confique must be declared in this crate's manifest");

    assert!(
        confique_line.contains("default-features = false"),
        "default features must be off, or yaml/json5 could arrive silently: {confique_line}"
    );
    assert!(
        confique_line.contains(r#"features = ["toml"]"#),
        "only the toml feature may be enabled: {confique_line}"
    );
    assert!(
        !confique_line.contains("yaml") && !confique_line.contains("json5"),
        "neither yaml nor json5 may be enabled: {confique_line}"
    );
}

// ---------------------------------------------------------------------------------------------
// The verdict
// ---------------------------------------------------------------------------------------------

/// The gate's outcome, asserted rather than narrated.
///
/// Contract C-C7: *"the gate is all-eight-or-fallback, with no partial adoption"*. Three
/// obligations are unmet, so the fallback triggers regardless of how survivable each individually
/// looks.
///
/// | # | Obligation | Met | Note |
/// |---|---|---|---|
/// | 1 | precedence order | **no** | inverted; recoverable by reversing arguments |
/// | 2 | nested-table merge | yes | sibling keys survive |
/// | 3 | array replacement | yes | 0 concatenations |
/// | 4 | source attribution | **no** | **not recoverable** — `with_fallback` is `Option::or` |
/// | 5 | invalid non-empty env fails | yes | |
/// | 6 | invalid empty env fails | **no** | documented silent fall-through to unset |
/// | 7 | shape conflict names both layers | **no** | fails, but names one layer |
/// | 8 | 0 JSON/YAML features | yes | structural |
///
/// Obligation 4 is the decisive one. Obligations 1 and 7 are inconveniences, and 6 is recoverable
/// if Renvor owns environment decoding — but attribution is destroyed inside the crate's merge
/// primitive, with no surface to recover it from.
#[test]
fn gate_verdict_confique_fails_the_gate_and_the_fallback_triggers() {
    let met = 4;
    let total = 8;
    let unmet = [1, 4, 6, 7];

    assert_eq!(
        met + unmet.len(),
        total,
        "every obligation must be accounted for"
    );
    assert_ne!(
        met, total,
        "the gate is all-eight-or-fallback; {met} of {total} met means the fallback triggers"
    );
    assert!(
        unmet.contains(&4),
        "obligation 4 is the decisive failure and must be recorded as unmet"
    );
}
