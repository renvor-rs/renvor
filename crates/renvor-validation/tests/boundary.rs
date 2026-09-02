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

#[test]
fn a_pathologically_nested_schema_is_refused_rather_than_overflowing_the_stack() {
    // The runtime walker was already depth-bounded because a `$ref` cycle would otherwise abort on
    // a caller's request. This walk follows no `$ref`, so a cycle cannot reach it — but literal
    // nesting can, and a schema arrives as a parsed `Value` that an author may have read from a
    // file. An abort at declaration time is a worse diagnostic than a refusal.
    // 200 deep: comfortably past the 64-frame bound, and comfortably inside what a
    // `serde_json::Value` survives.
    //
    // NOT deeper, and the reason is worth recording: `Value`'s `Drop` is itself recursive, so a
    // 2000-deep value overflows the stack while being FREED — before any Renvor code sees it. A
    // test at that depth measures serde_json, not this guard.
    //
    // Production is bounded before either: `serde_json::from_str` enforces its own recursion limit
    // and returns an error, so a parsed schema never becomes a value this deep. The guard below is
    // for a schema built programmatically, where nothing else would stop it.
    let mut schema = json!({"type": "string"});
    for _ in 0..200 {
        schema = json!({"type": "object", "properties": {"next": schema}});
    }

    // The assertion is that this RETURNS a refusal. A stack overflow aborts, and nothing below
    // would run.
    let result = Declaration::new(schema);
    assert!(result.is_err(), "a 200-deep schema was accepted");

    // POSITIVE CONTROL: ordinary nesting still works, so the refusal is about depth rather than
    // about nesting being rejected wholesale.
    let mut shallow = json!({"type": "string"});
    for _ in 0..10 {
        shallow = json!({"type": "object", "properties": {"next": shallow}});
    }
    assert!(
        Declaration::new(shallow).is_ok(),
        "a 10-deep schema was refused"
    );
}

#[test]
fn a_bounded_body_cannot_buy_an_unbounded_number_of_issues() {
    // THE AMPLIFICATION, as a test.
    //
    // `validate` produced one issue per violating element with no cap on the count. Measured on
    // the unfixed code by security review: a 2 097 001-byte request — inside the 2 MiB body limit —
    // produced 1 048 500 invalid parameters and a 68 MB response, at 210 MB peak resident memory
    // and 416 ms of CPU.
    //
    // The 30-second request timeout could not cancel it: validation and serialisation are
    // synchronous with no `.await`, so there is no yield point, and the work blocks a worker
    // thread outright. At the 1024-request concurrency ceiling that is not a slow request, it is
    // the whole process.
    let declaration = Declaration::new(json!({"type": "array", "items": {"type": "string"}}))
        .expect("a valid declaration");

    // Every element violates. A body of this shape fits well inside the request-body limit.
    let hostile = serde_json::Value::Array((0..200_000).map(|_| json!(1)).collect());

    let started = std::time::Instant::now();
    let issues = declaration.validate(Location::Body, &hostile);
    let elapsed = started.elapsed();

    assert!(
        issues.len() <= renvor_validation::schema::MAX_ISSUES,
        "validation reported {} issues for one request; the cap is {}",
        issues.len(),
        renvor_validation::schema::MAX_ISSUES
    );
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "validation took {elapsed:?} — the walker is not short-circuiting once the budget is spent"
    );

    // TRUNCATION IS ANNOUNCED. A caller told about a hundred problems must be able to tell that
    // from being told these are all of them.
    assert_eq!(
        issues.last().map(|issue| issue.reason),
        Some(Reason::TooManyIssues),
        "a truncated list did not say it was truncated"
    );

    // POSITIVE CONTROL: an ordinary failing body is reported in FULL, so the cap is a bound on the
    // pathological case rather than a ceiling everyone hits.
    let ordinary = json!([1, 2, 3]);
    let few = declaration.validate(Location::Body, &ordinary);
    assert_eq!(few.len(), 3, "an ordinary body was truncated: {few:?}");
    assert!(
        few.iter()
            .all(|issue| issue.reason != Reason::TooManyIssues),
        "an untruncated list claimed truncation"
    );
}

#[test]
fn a_multiple_of_that_is_not_strictly_positive_is_refused() {
    // A SECOND shape of the same defect the test below covers, and one it missed.
    //
    // The four non-negative integer keywords are refused when negative because `as_u64` rejects
    // them. `multipleOf` is a float, `as_f64` accepts `0` and `-2` happily, and the walker then
    // guarded its division with `step > 0.0` — so the constraint was SKIPPED while `schema()`
    // still published it. Readable, unusable, and silently unenforced: a constraint the API
    // promises and nothing checks.
    //
    // JSON Schema 2020-12 §6.2.1 requires `multipleOf` to be strictly greater than zero.
    //
    // Found by maintainer self-review during the Phase 005 L-14 completion audit, NOT by the
    // security review — which is the point of recording where it came from.
    for probe in [
        json!({"type": "number", "multipleOf": 0}),
        json!({"type": "number", "multipleOf": 0.0}),
        json!({"type": "number", "multipleOf": -2}),
        json!({"type": "integer", "multipleOf": -1}),
    ] {
        assert!(
            Declaration::new(probe.clone()).is_err(),
            "`{probe}` was accepted, and would be published without being enforced"
        );
    }

    // POSITIVE CONTROLS: strictly positive steps still build, so the refusal is about the value
    // and not about `multipleOf` being rejected wholesale.
    for valid in [
        json!({"type": "number", "multipleOf": 2}),
        json!({"type": "number", "multipleOf": 0.5}),
        json!({"type": "integer", "multipleOf": 3}),
    ] {
        assert!(
            Declaration::new(valid.clone()).is_ok(),
            "`{valid}` was refused, but it is well-formed"
        );
    }

    // AND THE CONSTRAINT IS ACTUALLY ENFORCED once accepted — without this the test above would
    // pass against an implementation that refused everything.
    let declaration = Declaration::new(json!({"type": "number", "multipleOf": 2}))
        .expect("a strictly positive step is well-formed");
    assert!(
        !declaration.is_valid(&json!(5)),
        "5 is not a multiple of 2 and was accepted"
    );
    assert!(
        declaration.is_valid(&json!(4)),
        "4 is a multiple of 2 and was refused"
    );
}

#[test]
fn multiple_of_is_decided_exactly_rather_than_against_a_tolerance() {
    // The previous implementation compared `value / step` to the nearest integer against an
    // ABSOLUTE 1e-9. The error in that quotient is RELATIVE, so it grows with the quotient, and a
    // constant was wrong at BOTH ends of the range at once (#50).
    //
    // These cases are asserted here, against Renvor alone, as well as in `tests/differential.rs`
    // against the reference implementation. The differential suite is the stronger oracle, but it
    // only speaks while that dev-dependency is present and correct — and #50 exists precisely
    // because the reference agreed with Renvor for as long as BOTH were wrong. This test keeps the
    // property pinned when the oracle cannot.
    for (step, instance, valid, why) in [
        // ---- the two regressions, one at each end of the quotient range ----
        (
            json!(0.01),
            json!(1070468.14),
            true,
            "107046814 x 0.01; the old quotient error was 1.49e-8, so it was REJECTED",
        ),
        (
            json!(1000000),
            json!(1000000.0001),
            false,
            "not a multiple; the old quotient error was 1e-10, so it was ACCEPTED",
        ),
        // The same step as the first case, on a value the old code happened to get right. This is
        // what makes the defect so hard to notice from the outside: it was value-dependent, not a
        // clean threshold, so a money schema validated most amounts and rejected occasional ones.
        (json!(0.01), json!(12345678.91), true, "1234567891 x 0.01"),
        (
            json!(0.01),
            json!(1070468.145),
            false,
            "a third decimal place against a two-decimal step",
        ),
        // ---- the case the old comment was written for, which must not regress ----
        (
            json!(0.1),
            json!(0.3),
            true,
            "0.3 / 0.1 is 2.9999999999999996 in binary floating point",
        ),
        (json!(0.1), json!(0.35), false, "0.35 is not 0.1 x n"),
        // ---- the extremes, which is where the arithmetic has to justify itself ----
        // Only the numerator scales here, by 600 decimal places. It is answered by reducing under
        // the divisor rather than by computing a 600-digit product.
        (json!(1e-300), json!(1e300), true, "1e300 is 1e600 x 1e-300"),
        // Only the divisor scales, and past `i128`. A divisor larger than any possible numerator
        // cannot divide a non-zero one, so that is an answer rather than a fallback.
        (
            json!(1),
            json!(1e-300),
            false,
            "1e-300 is not a whole number",
        ),
        // ---- zero and negatives ----
        (json!(5), json!(0), true, "zero is a multiple of everything"),
        (json!(5), json!(-15), true, "-15 is 5 x -3"),
        (json!(5), json!(-16), false, "-16 is not 5 x n"),
    ] {
        let declaration = Declaration::new(json!({"type": "number", "multipleOf": step}))
            .expect("a strictly positive step is well-formed");
        let issues = declaration.validate(Location::Body, &instance);

        assert_eq!(
            issues.is_empty(),
            valid,
            "multipleOf {step} against {instance}: expected valid={valid} because {why}"
        );
        if !valid {
            assert_eq!(
                issues[0].reason,
                Reason::NotMultipleOf,
                "multipleOf {step} against {instance} was rejected for the wrong reason"
            );
        }
    }
}

#[test]
fn a_malformed_keyword_value_is_refused_rather_than_silently_unenforced() {
    // Checking keyword NAMES was not enough. Every read in the walker is `as_u64` / `as_bool` /
    // `as_array`, so a WRONG-TYPED value yielded `None` and the constraint was skipped — while
    // `schema()` handed the same value to OpenAPI, so the description published a rule nothing
    // checked.
    //
    // These are the security review's exact probes. Every one was accepted and unenforced.
    let probes = [
        json!({"type": "object", "required": "name"}),
        json!({"type": "string", "maxLength": "5"}),
        json!({"type": "string", "maxLength": -1}),
        json!({"enum": "a"}),
        json!({"type": "array", "uniqueItems": "true"}),
        json!({"type": "strng"}),
        json!({"type": 42}),
        json!({"type": "integer", "minimum": "5"}),
        json!({"type": "object", "properties": "not an object"}),
    ];
    for probe in probes {
        assert!(
            Declaration::new(probe.clone()).is_err(),
            "`{probe}` was accepted, and would be published without being enforced"
        );
    }

    // POSITIVE CONTROLS: the well-typed forms of every one still build, so the refusals are about
    // the malformed values rather than about these keywords being rejected wholesale.
    for valid in [
        json!({"type": "object", "required": ["name"]}),
        json!({"type": "string", "maxLength": 5}),
        json!({"enum": ["a", "b"]}),
        json!({"type": "array", "uniqueItems": true}),
        json!({"type": ["string", "null"]}),
        json!({"type": "integer", "minimum": 5}),
        json!({"type": "object", "properties": {"a": {"type": "string"}}}),
    ] {
        assert!(
            Declaration::new(valid.clone()).is_ok(),
            "`{valid}` was refused, but it is well-formed"
        );
    }
}

#[test]
fn a_reference_resolving_to_a_non_schema_is_refused() {
    // An UNRESOLVABLE reference already failed closed. One that RESOLVED TO A NON-SCHEMA failed
    // OPEN: the walker found a value, could not read it as a schema, and accepted everything under
    // it. The security review's probe, verbatim.
    let schema = json!({
        "type": "object",
        "properties": {"admin": {"$ref": "#/title"}},
        "title": "Widget"
    });
    assert!(
        Declaration::new(schema).is_err(),
        "a $ref pointing at a string was accepted, and validated nothing beneath it"
    );

    // POSITIVE CONTROL: a reference to a real schema still resolves and still enforces.
    let good = Declaration::new(json!({
        "$defs": {"Name": {"type": "string", "minLength": 2}},
        "type": "object",
        "properties": {"name": {"$ref": "#/$defs/Name"}}
    }))
    .expect("a reference to a real schema must be accepted");
    assert!(
        !good
            .validate(Location::Body, &json!({"name": "a"}))
            .is_empty()
    );
}

#[test]
fn additional_properties_false_with_no_properties_permits_nothing() {
    // In JSON Schema that declaration permits NO members. Renvor accepted everything, because the
    // check was gated on `properties` being present — deny-by-default, inverted.
    let declaration = Declaration::new(json!({"type": "object", "additionalProperties": false}))
        .expect("a valid declaration");

    let issues = declaration.validate(Location::Body, &json!({"injected": "anything"}));
    assert_eq!(
        issues.len(),
        1,
        "a member was accepted against a schema that permits none: {issues:?}"
    );
    assert_eq!(issues[0].reason, Reason::UnknownMember);

    // POSITIVE CONTROL: the empty object satisfies it, so the refusal is about the member rather
    // than about the schema rejecting everything.
    assert!(declaration.validate(Location::Body, &json!({})).is_empty());
}

#[test]
fn an_integer_bound_beyond_f64_precision_still_holds() {
    // Comparing through f64 rounds above 2^53, so these two are the SAME f64 and the bound was
    // silently satisfied. For a query or path parameter that is a real parser differential: the
    // validator sees the rounded value while the handler re-parses the ORIGINAL text, so a handler
    // can observe a value validation never saw. Found by security review.
    let declaration =
        Declaration::new(json!({"type": "integer", "maximum": 9_007_199_254_740_992_i64}))
            .expect("a valid declaration");

    let over = serde_json::from_str::<serde_json::Value>("9007199254740993").expect("parses");
    let issues = declaration.validate(Location::Body, &over);
    assert_eq!(
        issues.len(),
        1,
        "a value one above the maximum was accepted: {issues:?}"
    );
    assert_eq!(issues[0].reason, Reason::OutOfRange);

    // POSITIVE CONTROL: the boundary itself is still accepted, so the check is a bound rather than
    // a blanket refusal of large integers.
    let at = serde_json::from_str::<serde_json::Value>("9007199254740992").expect("parses");
    assert!(
        declaration.validate(Location::Body, &at).is_empty(),
        "the value AT the maximum was refused"
    );
}

#[test]
fn a_reordered_object_is_still_a_duplicate() {
    // `uniqueItems` compares items by their RENDERING, which is exact only while `serde_json`'s
    // `preserve_order` is off. With it on, `Map` becomes an `IndexMap`, two EQUAL objects whose
    // members arrived in different orders render differently, and duplicate detection silently
    // stops working — a fail-open nothing else would catch.
    //
    // Asserted here rather than trusted to the manifests, because a transitive dependency can
    // enable that feature without this crate's manifest changing. Raised by security review.
    let declaration = Declaration::new(json!({"type": "array", "uniqueItems": true}))
        .expect("a valid declaration");

    // The SAME object, members supplied in opposite orders. Parsed from text so the ordering is
    // the input's, not a literal's.
    let instance: serde_json::Value =
        serde_json::from_str(r#"[{"a":1,"b":2},{"b":2,"a":1}]"#).expect("parses");

    let issues = declaration.validate(Location::Body, &instance);
    assert_eq!(
        issues.len(),
        1,
        "two renderings of the same object were treated as distinct — `preserve_order` is on \
         somewhere in the graph, and uniqueItems has silently stopped working: {issues:?}"
    );
    assert_eq!(issues[0].reason, Reason::NotUnique);
}

#[test]
fn the_issue_cap_holds_at_its_exact_boundary() {
    // `contracts/http-runtime.md` sets the standard: every bound is asserted "at its exact
    // boundary and at boundary + 1". `MAX_ISSUES` is a new bound and did not meet it.
    //
    // Written rather than reasoned about, because the two defects this phase's review found were
    // both off-by-one-shaped and both survived being read carefully.
    let cap = renvor_validation::schema::MAX_ISSUES;
    let declaration = Declaration::new(json!({"type": "array", "items": {"type": "string"}}))
        .expect("a valid declaration");

    let violating = |n: usize| serde_json::Value::Array((0..n).map(|_| json!(1)).collect());
    let truncated = |issues: &[renvor_validation::Issue]| {
        issues.last().map(|issue| issue.reason) == Some(Reason::TooManyIssues)
    };

    // ONE BELOW the cap: every violation reported, and NOT announced as truncated.
    let below = declaration.validate(Location::Body, &violating(cap - 1));
    assert_eq!(below.len(), cap - 1, "a sub-cap request was truncated");
    assert!(!truncated(&below), "a complete list claimed truncation");

    // AT the cap: the list is full, and the last slot is spent saying so — one real violation is
    // given up to carry that signal, which is the right trade. A caller told about a hundred
    // problems must be able to tell that from being told these are all of them.
    let at = declaration.validate(Location::Body, &violating(cap));
    assert_eq!(at.len(), cap, "the list overran the cap");
    assert!(truncated(&at), "a truncated list did not say so");

    // BOUNDARY + 1, and far beyond it: still exactly the cap, still announced.
    for count in [cap + 1, cap * 10] {
        let over = declaration.validate(Location::Body, &violating(count));
        assert_eq!(
            over.len(),
            cap,
            "{count} violations produced {} issues",
            over.len()
        );
        assert!(
            truncated(&over),
            "{count} violations were truncated silently"
        );
    }
}
