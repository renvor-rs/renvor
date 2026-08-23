//! The declaration boundary: what is refused, what is reported, and what never leaves.

use renvor_error::Location;
use renvor_validation::{Declaration, DeclarationError, Issue, Reason};
use serde_json::json;

/// A recognisable value. If it ever appears in an issue, the search below finds it.
const CANARY: &str = "CANARY-8f3a1c05b72e64d9-REJECTED";

#[test]
fn an_unsupported_keyword_is_refused_at_declaration_time() {
    // Each of these is a real JSON Schema keyword that Renvor does NOT enforce. Publishing one
    // and checking nothing would make the description false exactly where an author relied on it.
    for keyword in [
        "pattern",
        "allOf",
        "anyOf",
        "oneOf",
        "not",
        "if",
        "patternProperties",
        "propertyNames",
        "dependentRequired",
        "contains",
        "prefixItems",
        "unevaluatedProperties",
        "contentEncoding",
    ] {
        let schema = json!({ "type": "object", keyword: json!({}) });
        let Err(error) = Declaration::new(schema) else {
            panic!("`{keyword}` must be refused rather than silently unenforced");
        };
        assert!(
            matches!(error, DeclarationError::UnsupportedKeyword { keyword: ref found, .. } if found == keyword),
            "`{keyword}` produced the wrong error: {error:?}"
        );
    }
}

#[test]
fn every_enforced_and_annotation_keyword_is_accepted() {
    // POSITIVE CONTROL for the test above. Without it, a bug that refused EVERY keyword would
    // pass the refusal test and break every real declaration.
    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://example.test/s",
        "$comment": "a note",
        "title": "Thing",
        "description": "a thing",
        "default": {},
        "deprecated": false,
        "readOnly": false,
        "examples": [{}],
        "type": "object",
        "required": ["a"],
        "additionalProperties": false,
        "properties": {
            "a": {"type": "string", "minLength": 1, "maxLength": 9, "format": "email"},
            "b": {"type": "integer", "minimum": 0, "maximum": 9, "exclusiveMinimum": -1,
                  "exclusiveMaximum": 10, "multipleOf": 1},
            "c": {"type": "array", "items": {"type": "string"}, "minItems": 0, "maxItems": 3,
                  "uniqueItems": true},
            "d": {"enum": ["x", "y"]},
            "e": {"const": 1}
        },
        "$defs": {"N": {"type": "string"}}
    });
    Declaration::new(schema).expect("every documented keyword must be accepted");
}

#[test]
fn a_remote_reference_is_refused_because_resolving_one_would_be_a_network_call() {
    let schema = json!({"$ref": "https://example.test/schema.json"});
    assert!(matches!(
        Declaration::new(schema),
        Err(DeclarationError::UnresolvableRef { .. })
    ));

    // POSITIVE CONTROL: a local reference is accepted.
    let local = json!({"$defs": {"N": {"type": "string"}}, "$ref": "#/$defs/N"});
    assert!(Declaration::new(local).is_ok());
}

#[test]
fn no_issue_carries_the_rejected_value_in_any_field() {
    // POSITIVE CONTROL first: prove the probe finds the canary when it IS there.
    assert!(
        format!("{:?}", json!({"reason": CANARY})).contains(CANARY),
        "the canary probe does not discriminate"
    );

    let declaration = Declaration::new(json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["name"],
        "properties": {"name": {"type": "string", "maxLength": 3}}
    }))
    .expect("a valid declaration");

    // Three ways a canary could reach an issue: as a value, as an unknown member NAME, and as a
    // nested value.
    for instance in [
        json!({"name": CANARY}),
        json!({"name": "ok", CANARY: 1}),
        json!({CANARY: {"name": CANARY}}),
    ] {
        let issues = declaration.validate(Location::Body, &instance);
        assert!(!issues.is_empty(), "{instance} produced no issue at all");

        // The WHOLE issue, debug-rendered — pointer, reason, location, everything.
        let rendered = format!("{issues:?}");
        assert!(
            !rendered.contains(CANARY),
            "the rejected value or an attacker-chosen member name reached an issue: {rendered}"
        );
    }
}

#[test]
fn an_unknown_member_is_reported_without_naming_it() {
    let declaration = Declaration::new(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {"declared": {"type": "string"}}
    }))
    .expect("a valid declaration");

    let issues = declaration.validate(Location::Body, &json!({"attacker_chosen": 1}));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].reason, Reason::UnknownMember);
    // The pointer names the CONTAINING object — the root — not the member.
    assert_eq!(issues[0].pointer.as_str(), "");
}

#[test]
fn a_missing_required_member_is_named_because_the_name_is_the_schemas_word() {
    let declaration = Declaration::new(json!({
        "type": "object",
        "required": ["quantity"],
        "properties": {"quantity": {"type": "integer"}}
    }))
    .expect("a valid declaration");

    let issues = declaration.validate(Location::Body, &json!({}));
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].reason, Reason::RequiredMissing);
    // Naming it IS safe here: `quantity` came from the declaration, not from the request.
    assert_eq!(issues[0].pointer.as_str(), "/quantity");
}

#[test]
fn every_violation_is_reported_not_only_the_first() {
    // FR-005: a caller must be able to correct an input in ONE round trip.
    let declaration = Declaration::new(json!({
        "type": "object",
        "required": ["a", "b", "c"],
        "properties": {
            "a": {"type": "string"},
            "b": {"type": "string"},
            "c": {"type": "string"}
        }
    }))
    .expect("a valid declaration");

    let issues = declaration.validate(Location::Body, &json!({}));
    assert_eq!(issues.len(), 3, "validation stopped early: {issues:?}");
}

#[test]
fn issue_order_is_deterministic_for_the_same_input() {
    let declaration = Declaration::new(json!({
        "type": "object",
        "properties": {
            "zulu": {"type": "integer"},
            "alpha": {"type": "integer"},
            "mike": {"type": "integer"}
        }
    }))
    .expect("a valid declaration");

    // A `serde_json::Map` preserves INSERTION order, which for a request is the caller's order.
    // Two instances with the same members in different orders must produce the same sequence.
    let first: serde_json::Value =
        serde_json::from_str(r#"{"zulu":"x","alpha":"x","mike":"x"}"#).expect("parses");
    let second: serde_json::Value =
        serde_json::from_str(r#"{"mike":"x","zulu":"x","alpha":"x"}"#).expect("parses");

    let pointers = |issues: Vec<Issue>| -> Vec<String> {
        issues
            .into_iter()
            .map(|issue| issue.pointer.as_str().to_owned())
            .collect()
    };

    let left = pointers(declaration.validate(Location::Body, &first));
    let right = pointers(declaration.validate(Location::Body, &second));

    assert_eq!(left, vec!["/alpha", "/mike", "/zulu"]);
    assert_eq!(
        left, right,
        "issue order followed the request's member order, so it is caller-controlled"
    );
}

#[test]
fn a_pointer_segment_containing_a_slash_is_escaped() {
    // RFC 6901: `/` becomes `~1` and `~` becomes `~0`. Without escaping, a member named `a/b`
    // would produce a pointer that reads as two segments and points somewhere else entirely.
    let declaration = Declaration::new(json!({
        "type": "object",
        "required": ["a/b", "c~d"],
        "properties": {"a/b": {"type": "string"}, "c~d": {"type": "string"}}
    }))
    .expect("a valid declaration");

    let pointers: Vec<String> = declaration
        .validate(Location::Body, &json!({}))
        .into_iter()
        .map(|issue| issue.pointer.as_str().to_owned())
        .collect();

    assert!(pointers.contains(&"/a~1b".to_owned()), "got {pointers:?}");
    assert!(pointers.contains(&"/c~0d".to_owned()), "got {pointers:?}");
}

#[test]
fn a_recursive_reference_terminates_rather_than_overflowing_the_stack() {
    // A type that contains itself. Without the depth bound this recurses until the process aborts,
    // and an abort in a request path is a denial of service a caller gets to trigger.
    let declaration = Declaration::new(json!({
        "$defs": {"Node": {"type": "object", "properties": {"next": {"$ref": "#/$defs/Node"}}}},
        "$ref": "#/$defs/Node"
    }))
    .expect("a valid declaration");

    // Build a deeply nested instance.
    let mut instance = json!({});
    for _ in 0..200 {
        instance = json!({"next": instance});
    }

    // The assertion is that this RETURNS. A stack overflow aborts the process and no assertion
    // below it would ever run.
    let issues = declaration.validate(Location::Body, &instance);
    let _ = issues;
}

#[test]
fn the_declaration_and_the_published_schema_are_the_same_value() {
    // The single-source property, asserted rather than described: the value `schema()` returns for
    // the description is byte-identical to the one `validate()` interprets, because there is one.
    let source = json!({"type": "string", "minLength": 2});
    let declaration = Declaration::new(source.clone()).expect("a valid declaration");
    assert_eq!(declaration.schema(), &source);

    // And it genuinely drives enforcement, so the equality above is not about an unused field.
    assert!(!declaration.validate(Location::Body, &json!("a")).is_empty());
    assert!(
        declaration
            .validate(Location::Body, &json!("ab"))
            .is_empty()
    );
}

#[test]
fn a_schemars_derived_type_is_inside_the_subset() {
    #[derive(serde::Serialize, schemars::JsonSchema)]
    #[allow(dead_code)]
    struct CreateItem {
        #[schemars(length(min = 1, max = 64))]
        name: String,
        #[schemars(range(min = 1, max = 1000))]
        quantity: u32,
        note: Option<String>,
        tags: Vec<String>,
    }

    let declaration = Declaration::of::<CreateItem>()
        .expect("a schemars schema for an ordinary struct must be inside the subset");

    // The constraints declared with derive attributes ARE enforced — which is the whole point of
    // one declaration serving both purposes.
    let issues = declaration.validate(
        Location::Body,
        &json!({"name": "", "quantity": 0, "tags": []}),
    );
    let reasons: Vec<Reason> = issues.iter().map(|issue| issue.reason).collect();
    assert!(reasons.contains(&Reason::TooShort), "got {issues:?}");
    assert!(reasons.contains(&Reason::OutOfRange), "got {issues:?}");

    // And a satisfying instance passes, so the above is not a validator that refuses everything.
    assert!(
        declaration
            .validate(
                Location::Body,
                &json!({"name": "widget", "quantity": 3, "note": null, "tags": ["a"]})
            )
            .is_empty()
    );
}

#[test]
fn unique_items_does_not_degrade_quadratically() {
    // The pairwise version this replaced was O(n²), and `uniqueItems` is reachable WITHOUT
    // `maxItems` — `schemars` emits exactly that for a set-typed field. An array that fits inside
    // the 2 MiB body limit could then buy a caller 10¹¹ comparisons.
    //
    // 50 000 distinct items is the worst case for the check (nothing matches until the end).
    // O(n log n) finishes in milliseconds; O(n²) is 2.5 × 10⁹ comparisons and does not.
    let declaration = Declaration::new(json!({"type": "array", "uniqueItems": true}))
        .expect("a valid declaration");

    let distinct: Vec<serde_json::Value> = (0..50_000).map(|n| json!(n)).collect();
    let instance = serde_json::Value::Array(distinct);

    let started = std::time::Instant::now();
    let issues = declaration.validate(Location::Body, &instance);
    let elapsed = started.elapsed();

    assert!(
        issues.is_empty(),
        "distinct items were reported as repeated"
    );
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "uniqueItems took {elapsed:?} for 50 000 distinct items, which is the quadratic shape"
    );

    // POSITIVE CONTROL: it still DETECTS a repeat, so the speed above is not from skipping work.
    let mut with_repeat: Vec<serde_json::Value> = (0..1_000).map(|n| json!(n)).collect();
    with_repeat.push(json!(500));
    let found = declaration.validate(Location::Body, &serde_json::Value::Array(with_repeat));
    assert_eq!(found.len(), 1, "a repeat was not detected");
    assert_eq!(found[0].reason, Reason::NotUnique);
}
