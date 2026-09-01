//! Differential testing against the reference JSON Schema implementation.
//!
//! # What this test exists to catch
//!
//! Renvor interprets a **bounded subset** of JSON Schema at runtime rather than resolving
//! `jsonschema` into the transport's dependency graph. The measurement, and the command that
//! reproduces it, live in `renvor-validation`'s manifest so there is one place to correct when the
//! graphs move; the short version is that it would grow the transport's runtime dependencies by
//! more than half for machinery runtime validation does not use.
//!
//! A bounded subset is only honest while it **agrees** with the standard on the keywords it
//! claims. A subset that quietly diverges publishes a constraint and enforces a different one,
//! which is worse than publishing nothing.
//!
//! So the reference implementation is a **dev-dependency**, where its weight is free, and this
//! test asserts that Renvor's verdict equals it for every pair below. That is what makes "bounded"
//! a boundary rather than an excuse.
//!
//! # Verdicts, not messages
//!
//! The comparison is valid/invalid. It is deliberately **not** a comparison of issue counts or
//! text: the two implementations legitimately group violations differently, and requiring
//! identical output would either fail on a difference that harms nobody or force Renvor to imitate
//! a message format it must never emit — the reference implementation's messages **quote the
//! rejected value**, which is the disclosure this whole phase is built against.

use renvor_error::Location;
use renvor_validation::Declaration;
use serde_json::{Value, json};

fn reference_says_valid(schema: &Value, instance: &Value) -> bool {
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(schema)
        .expect("the reference implementation compiles the schema");
    validator.is_valid(instance)
}

fn renvor_says_valid(schema: &Value, instance: &Value) -> bool {
    Declaration::new(schema.clone())
        .expect("the schema is inside Renvor's declared subset")
        .validate(Location::Body, instance)
        .is_empty()
}

/// Every (schema, instance) pair, with the expected verdict stated so a test that agreed with the
/// reference implementation *because both were wrong* would still be visible.
fn cases() -> Vec<(&'static str, Value, Value, bool)> {
    vec![
        // ---- type ----
        (
            "string accepts a string",
            json!({"type": "string"}),
            json!("x"),
            true,
        ),
        (
            "string refuses a number",
            json!({"type": "string"}),
            json!(1),
            false,
        ),
        (
            "integer accepts a whole float",
            json!({"type": "integer"}),
            json!(1.0),
            true,
        ),
        (
            "integer refuses a fractional float",
            json!({"type": "integer"}),
            json!(1.5),
            false,
        ),
        (
            "number accepts an integer",
            json!({"type": "number"}),
            json!(3),
            true,
        ),
        (
            "boolean refuses a string",
            json!({"type": "boolean"}),
            json!("true"),
            false,
        ),
        (
            "null accepts null",
            json!({"type": "null"}),
            json!(null),
            true,
        ),
        (
            "a nullable type accepts null",
            json!({"type": ["string", "null"]}),
            json!(null),
            true,
        ),
        (
            "a nullable type accepts the base type",
            json!({"type": ["string", "null"]}),
            json!("x"),
            true,
        ),
        (
            "a nullable type refuses a third type",
            json!({"type": ["string", "null"]}),
            json!(7),
            false,
        ),
        // ---- string length, in CODE POINTS ----
        (
            "minLength at the boundary",
            json!({"type": "string", "minLength": 3}),
            json!("abc"),
            true,
        ),
        (
            "minLength below it",
            json!({"type": "string", "minLength": 3}),
            json!("ab"),
            false,
        ),
        (
            "maxLength at the boundary",
            json!({"type": "string", "maxLength": 3}),
            json!("abc"),
            true,
        ),
        (
            "maxLength above it",
            json!({"type": "string", "maxLength": 3}),
            json!("abcd"),
            false,
        ),
        (
            "length counts code points, not bytes",
            json!({"type": "string", "maxLength": 3}),
            // Three code points, six bytes. A byte-counting implementation refuses this.
            json!("ééé"),
            true,
        ),
        (
            "a multi-byte string past the bound is still refused",
            json!({"type": "string", "maxLength": 3}),
            json!("éééé"),
            false,
        ),
        // ---- numeric bounds ----
        (
            "minimum at the boundary",
            json!({"type": "integer", "minimum": 1}),
            json!(1),
            true,
        ),
        (
            "minimum below it",
            json!({"type": "integer", "minimum": 1}),
            json!(0),
            false,
        ),
        (
            "maximum at the boundary",
            json!({"type": "integer", "maximum": 10}),
            json!(10),
            true,
        ),
        (
            "maximum above it",
            json!({"type": "integer", "maximum": 10}),
            json!(11),
            false,
        ),
        (
            "exclusiveMinimum refuses the boundary",
            json!({"type": "integer", "exclusiveMinimum": 1}),
            json!(1),
            false,
        ),
        (
            "exclusiveMinimum accepts above it",
            json!({"type": "integer", "exclusiveMinimum": 1}),
            json!(2),
            true,
        ),
        (
            "exclusiveMaximum refuses the boundary",
            json!({"type": "integer", "exclusiveMaximum": 10}),
            json!(10),
            false,
        ),
        (
            "multipleOf accepts a multiple",
            json!({"type": "number", "multipleOf": 5}),
            json!(15),
            true,
        ),
        (
            "multipleOf refuses a non-multiple",
            json!({"type": "number", "multipleOf": 5}),
            json!(16),
            false,
        ),
        (
            "multipleOf survives binary floating point",
            // 0.3 / 0.1 is 2.9999999999999996 in binary floating point. A truncating
            // implementation calls this a violation; it is not one.
            json!({"type": "number", "multipleOf": 0.1}),
            json!(0.3),
            true,
        ),
        // ---- enum and const ----
        (
            "enum accepts a member",
            json!({"enum": ["a", "b"]}),
            json!("a"),
            true,
        ),
        (
            "enum refuses a non-member",
            json!({"enum": ["a", "b"]}),
            json!("c"),
            false,
        ),
        (
            "const accepts the constant",
            json!({"const": 42}),
            json!(42),
            true,
        ),
        (
            "const refuses anything else",
            json!({"const": 42}),
            json!(43),
            false,
        ),
        // ---- arrays ----
        (
            "minItems at the boundary",
            json!({"type": "array", "minItems": 2}),
            json!([1, 2]),
            true,
        ),
        (
            "minItems below it",
            json!({"type": "array", "minItems": 2}),
            json!([1]),
            false,
        ),
        (
            "maxItems above it",
            json!({"type": "array", "maxItems": 2}),
            json!([1, 2, 3]),
            false,
        ),
        (
            "uniqueItems refuses a repeat",
            json!({"type": "array", "uniqueItems": true}),
            json!([1, 1]),
            false,
        ),
        (
            "uniqueItems accepts distinct items",
            json!({"type": "array", "uniqueItems": true}),
            json!([1, 2]),
            true,
        ),
        (
            "uniqueItems compares structurally, not by reference",
            json!({"type": "array", "uniqueItems": true}),
            json!([{"a": 1}, {"a": 1}]),
            false,
        ),
        (
            "items applies to every element",
            json!({"type": "array", "items": {"type": "integer"}}),
            json!([1, "two", 3]),
            false,
        ),
        // ---- objects ----
        (
            "required member present",
            json!({"type": "object", "required": ["a"], "properties": {"a": {"type": "string"}}}),
            json!({"a": "x"}),
            true,
        ),
        (
            "required member absent",
            json!({"type": "object", "required": ["a"], "properties": {"a": {"type": "string"}}}),
            json!({}),
            false,
        ),
        (
            "a declared member is validated",
            json!({"type": "object", "properties": {"a": {"type": "integer"}}}),
            json!({"a": "not an integer"}),
            false,
        ),
        (
            "additionalProperties false refuses an unknown member",
            json!({
                "type": "object",
                "properties": {"a": {"type": "string"}},
                "additionalProperties": false
            }),
            json!({"a": "x", "b": "y"}),
            false,
        ),
        (
            "unknown members are permitted by default",
            json!({"type": "object", "properties": {"a": {"type": "string"}}}),
            json!({"a": "x", "b": "y"}),
            true,
        ),
        (
            "nesting is validated to depth",
            json!({
                "type": "object",
                "properties": {
                    "inner": {
                        "type": "object",
                        "properties": {"n": {"type": "integer", "minimum": 5}}
                    }
                }
            }),
            json!({"inner": {"n": 1}}),
            false,
        ),
        // ---- $ref ----
        (
            "a local $ref resolves and is enforced",
            json!({
                "$defs": {"Name": {"type": "string", "minLength": 2}},
                "type": "object",
                "properties": {"name": {"$ref": "#/$defs/Name"}}
            }),
            json!({"name": "a"}),
            false,
        ),
        (
            "a local $ref accepts a satisfying value",
            json!({
                "$defs": {"Name": {"type": "string", "minLength": 2}},
                "type": "object",
                "properties": {"name": {"$ref": "#/$defs/Name"}}
            }),
            json!({"name": "ab"}),
            true,
        ),
        // ---- boolean schemas ----
        (
            "`true` accepts anything",
            json!(true),
            json!({"anything": [1, 2]}),
            true,
        ),
        ("`false` accepts nothing", json!(false), json!(null), false),
    ]
}

#[test]
fn renvor_agrees_with_the_reference_implementation_on_every_case() {
    let mut disagreements = Vec::new();

    for (name, schema, instance, expected) in cases() {
        let reference = reference_says_valid(&schema, &instance);
        let renvor = renvor_says_valid(&schema, &instance);

        if reference != renvor {
            disagreements.push(format!(
                "  {name}: reference says {reference}, Renvor says {renvor}"
            ));
        }
        // The expectation is stated so that a case where BOTH are wrong is still caught. Two
        // implementations agreeing is not the same as two implementations being right.
        assert_eq!(
            reference, expected,
            "the stated expectation for `{name}` disagrees with the reference implementation — \
             the CASE is wrong, not the code"
        );
    }

    assert!(
        disagreements.is_empty(),
        "Renvor's bounded interpreter disagrees with the standard it publishes:\n{}",
        disagreements.join("\n")
    );
}

#[test]
fn the_comparison_would_notice_a_disagreement() {
    // POSITIVE CONTROL. Without this, a bug that made both helpers return the same constant would
    // pass the test above with no cases actually exercised.
    let schema = json!({"type": "string", "minLength": 5});
    assert!(reference_says_valid(&schema, &json!("abcde")));
    assert!(!reference_says_valid(&schema, &json!("abc")));
    assert!(renvor_says_valid(&schema, &json!("abcde")));
    assert!(!renvor_says_valid(&schema, &json!("abc")));
}

#[test]
fn the_case_list_covers_every_enforced_keyword() {
    // A keyword Renvor claims to enforce but never differentially tests is a claim with no
    // evidence. This fails when a keyword is added to the enforced set without a case.
    let corpus: String = cases()
        .iter()
        .map(|(_, schema, _, _)| schema.to_string())
        .collect();

    for keyword in renvor_validation::ENFORCED_KEYWORDS {
        assert!(
            corpus.contains(keyword),
            "`{keyword}` is enforced but appears in no differential case"
        );
    }
}
