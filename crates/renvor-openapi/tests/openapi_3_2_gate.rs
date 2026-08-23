//! **The fail-closed OpenAPI 3.2 gate.**
//!
//! Constitution principle V: *"emitted documents MUST NOT claim a version that selected tooling
//! does not correctly implement."* This file is what makes that an observable property rather than
//! an intention.
//!
//! # Why declaring `3.2.0` proves nothing on its own
//!
//! OpenAPI 3.2 is largely backwards compatible with 3.1, so a 3.1 document with its version string
//! edited validates against the 3.2 schema perfectly well. A gate that checked only the version
//! string, or only 3.2 validity, would pass for exactly the relabelling principle V forbids.
//!
//! # What actually discriminates
//!
//! Both official schemas use `unevaluatedProperties: false` throughout. So a document carrying a
//! **3.2-only member** is *structurally* invalid against the 3.1 schema — and if the 3.1 schema's
//! `openapi` version pattern is **neutralised first**, the rejection cannot be attributed to the
//! version string.
//!
//! That is the proof, and PROOF 4 below is its control: a genuinely relabelled 3.1 document
//! **passes** the same neutralised check, so the discriminator is shown to discriminate rather
//! than to reject everything.
//!
//! # Everything is offline
//!
//! The schemas are vendored. A gate that needs the network is a gate that fails when the network
//! does, and `contracts/verification-sequence.md` requires a check that cannot run to be a failure
//! rather than a skip.

use std::collections::BTreeMap;

use renvor_openapi::{
    Document, Info, JSON_MEDIA_TYPE, MediaType, OPENAPI_VERSION, Operation, PathItem, Response, Tag,
};
use serde_json::{Value, json};

const OAS_32: &str = include_str!("schemas/oas-3.2-schema-2025-09-17.json");
const OAS_31: &str = include_str!("schemas/oas-3.1-schema-2022-10-07.json");

fn schema_32() -> Value {
    serde_json::from_str(OAS_32).expect("the vendored 3.2 schema parses")
}

fn schema_31() -> Value {
    serde_json::from_str(OAS_31).expect("the vendored 3.1 schema parses")
}

/// The 3.1 schema with its `openapi` version pattern removed.
///
/// This is what separates "rejected because the version string says 3.2" from "rejected because
/// the document is structurally 3.2". Only the second proves anything.
fn schema_31_without_version_constraint() -> Value {
    let mut schema = schema_31();
    schema["properties"]["openapi"] = json!({"type": "string"});
    schema
}

fn validates(schema: &Value, document: &Value) -> Result<(), String> {
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .build(schema)
        .map_err(|error| format!("the official schema would not compile: {error}"))?;

    if validator.is_valid(document) {
        Ok(())
    } else {
        let first = validator.iter_errors(document).next().map_or_else(
            || "invalid".to_owned(),
            |error| format!("{error} at {}", error.instance_path()),
        );
        Err(first)
    }
}

/// A document exercising the constructs a real API uses, plus the 3.2-only members.
fn representative_document() -> Document {
    let item_schema = json!({
        "type": "object",
        "required": ["id", "name"],
        "properties": {
            "id": {"type": "integer", "minimum": 1},
            "name": {"type": "string", "minLength": 1, "maxLength": 64}
        }
    });

    let mut responses = BTreeMap::new();
    responses.insert(
        "200".to_owned(),
        Response {
            // 3.2-ONLY. The 3.1 Response Object has no `summary`.
            summary: "A page of items".to_owned(),
            description: Some("The items this caller may see.".to_owned()),
            content: BTreeMap::from([(
                JSON_MEDIA_TYPE.to_owned(),
                MediaType {
                    schema: json!({"type": "array", "items": item_schema}),
                    example: Some(json!([{"id": 1, "name": "widget"}])),
                },
            )]),
        },
    );
    responses.insert(
        "400".to_owned(),
        Response {
            summary: "The request was refused".to_owned(),
            // Deliberately ABSENT. 3.1 REQUIRES `description` on a Response Object; 3.2 removed
            // that requirement — a second genuine 3.2 relaxation this document exercises.
            description: None,
            content: BTreeMap::from([(
                "application/problem+json".to_owned(),
                MediaType {
                    schema: json!({"$ref": "#/components/schemas/ProblemDetails"}),
                    example: None,
                },
            )]),
        },
    );

    let mut path = PathItem::default();
    path.set(
        "GET",
        Operation {
            operation_id: "listItems".to_owned(),
            summary: Some("List items".to_owned()),
            description: None,
            tags: vec!["items".to_owned()],
            parameters: vec![renvor_openapi::Parameter {
                name: "page_size".to_owned(),
                location: renvor_openapi::ParameterLocation::Query,
                description: Some("How many items to return.".to_owned()),
                required: false,
                schema: json!({"type": "integer", "minimum": 1, "maximum": 100}),
            }],
            request_body: None,
            responses,
        },
    )
    .expect("one operation per method");

    let mut document = Document::new(Info {
        title: "Representative API".to_owned(),
        version: "1.0.0".to_owned(),
        description: None,
    })
    // 3.2-ONLY. The 3.1 root object has no `$self`.
    .with_self_uri("https://example.test/openapi.json")
    .with_problem_component();

    document.tags.push(Tag {
        name: "items".to_owned(),
        // 3.2-ONLY, both of these. The 3.1 Tag Object has neither.
        summary: Some("Items".to_owned()),
        description: Some("Operations over items.".to_owned()),
        kind: Some("nav".to_owned()),
    });
    document.paths.insert("/items".to_owned(), path);
    document
}

// ---------------------------------------------------------------------------------------------
// PROOF 1 — the document declares exactly 3.2.0, and the version is not settable
// ---------------------------------------------------------------------------------------------

#[test]
fn proof_1_the_declared_version_is_exactly_3_2_0() {
    assert_eq!(OPENAPI_VERSION, "3.2.0");

    let value = representative_document().to_value();
    assert_eq!(
        value.get("openapi").and_then(Value::as_str),
        Some("3.2.0"),
        "the emitted document does not declare 3.2.0"
    );
}

// ---------------------------------------------------------------------------------------------
// PROOF 2 — it validates against the OFFICIAL 3.2 schema
// ---------------------------------------------------------------------------------------------

#[test]
fn proof_2_the_document_validates_against_the_official_3_2_schema() {
    let document = representative_document().to_value();
    validates(&schema_32(), &document)
        .unwrap_or_else(|error| panic!("the emitted document is not valid OpenAPI 3.2: {error}"));
}

// ---------------------------------------------------------------------------------------------
// PROOF 3 — the SAME document is STRUCTURALLY rejected by the official 3.1 schema
// ---------------------------------------------------------------------------------------------

#[test]
fn proof_3_the_document_is_structurally_rejected_by_the_official_3_1_schema() {
    let document = representative_document().to_value();

    // With the version pattern intact, 3.1 rejects it — but that could be the version string alone.
    assert!(
        validates(&schema_31(), &document).is_err(),
        "the 3.1 schema accepted a document declaring 3.2.0"
    );

    // WITH THE VERSION CONSTRAINT NEUTRALISED. If it is still rejected, the rejection is about the
    // document's STRUCTURE — which is what proves the output is genuinely 3.2 and not relabelled.
    let error = validates(&schema_31_without_version_constraint(), &document).expect_err(
        "the document passed the 3.1 schema once the version constraint was removed, which \
             means it is 3.1 output wearing a 3.2 version string",
    );

    assert!(
        error.contains("Unevaluated")
            || error.contains("unevaluated")
            || error.contains("required"),
        "the 3.1 rejection was not structural: {error}"
    );
}

// ---------------------------------------------------------------------------------------------
// PROOF 4 — the CONTROL: a genuinely relabelled 3.1 document PASSES the neutralised check
// ---------------------------------------------------------------------------------------------

#[test]
fn proof_4_the_discriminator_actually_discriminates() {
    // A plain 3.1 document with nothing but its version string changed. If the neutralised 3.1
    // schema rejected this too, PROOF 3 would prove nothing — it would just mean the schema
    // rejects everything.
    let relabelled = json!({
        "openapi": "3.2.0",
        "info": {"title": "Relabelled", "version": "1.0.0"},
        "paths": {
            "/items": {
                "get": {
                    "operationId": "listItems",
                    "responses": {"200": {"description": "ok"}}
                }
            }
        }
    });

    validates(&schema_31_without_version_constraint(), &relabelled).unwrap_or_else(|error| {
        panic!(
            "the control failed: a relabelled 3.1 document should pass the neutralised 3.1 \
             schema, so PROOF 3's rejection would be meaningless. Error: {error}"
        )
    });

    // And it validates as 3.2 as well — which is exactly why declaring 3.2.0 proves nothing on
    // its own, and why PROOF 3 exists.
    validates(&schema_32(), &relabelled)
        .expect("a relabelled 3.1 document is valid 3.2 — this is the hazard, stated");
}

// ---------------------------------------------------------------------------------------------
// PROOF 5 — the validator is not vacuous
// ---------------------------------------------------------------------------------------------

#[test]
fn proof_5_the_official_3_2_schema_rejects_malformed_documents() {
    let malformed: [(&str, Value); 4] = [
        (
            "info missing the required version",
            json!({"openapi": "3.2.0", "info": {"title": "x"}, "paths": {}}),
        ),
        (
            "paths is not an object",
            json!({"openapi": "3.2.0", "info": {"title": "x", "version": "1"}, "paths": "nope"}),
        ),
        (
            "an unknown root member",
            json!({
                "openapi": "3.2.0",
                "info": {"title": "x", "version": "1"},
                "paths": {},
                "notAnOpenApiMember": true
            }),
        ),
        (
            "a 3.1 version string",
            json!({"openapi": "3.1.0", "info": {"title": "x", "version": "1"}, "paths": {}}),
        ),
    ];

    let schema = schema_32();
    for (name, document) in malformed {
        assert!(
            validates(&schema, &document).is_err(),
            "the official 3.2 schema accepted a malformed document ({name}), so every other \
             proof in this file is vacuous"
        );
    }

    // POSITIVE CONTROL: a minimal WELL-FORMED 3.2 document is accepted, so the rejections above
    // are about those documents rather than about the validator refusing everything.
    validates(
        &schema,
        &json!({"openapi": "3.2.0", "info": {"title": "x", "version": "1"}, "paths": {}}),
    )
    .expect("a minimal well-formed 3.2 document must be accepted");
}

// ---------------------------------------------------------------------------------------------
// Supporting properties
// ---------------------------------------------------------------------------------------------

#[test]
fn the_document_carries_at_least_one_genuinely_3_2_only_member() {
    // If a refactor ever dropped every 3.2-only construct, PROOF 3 would start failing for a
    // confusing reason. This says why, directly.
    let rendered = representative_document().to_value().to_string();
    let markers = ["$self", "\"summary\"", "\"kind\""];
    let present: Vec<&str> = markers
        .iter()
        .copied()
        .filter(|marker| rendered.contains(marker))
        .collect();
    assert!(
        present.len() >= 3,
        "the document carries too few 3.2-only members: found {present:?}"
    );
}

#[test]
fn generation_is_deterministic_to_the_byte() {
    let first = representative_document()
        .to_json()
        .expect("the document serialises");
    let second = representative_document()
        .to_json()
        .expect("the document serialises");
    assert_eq!(first, second, "two generations differ");

    // And member order is FIXED rather than incidental: `openapi` leads, and `paths` follows the
    // metadata. A map that serialised in insertion order would not guarantee this.
    let openapi_at = first.find("\"openapi\"").expect("openapi is present");
    let info_at = first.find("\"info\"").expect("info is present");
    let paths_at = first.find("\"paths\"").expect("paths is present");
    assert!(
        openapi_at < info_at && info_at < paths_at,
        "member order drifted"
    );
}

#[test]
fn open_ended_maps_are_emitted_in_sorted_order() {
    let mut document = representative_document();
    // Insert paths in reverse order. A sorted map emits them sorted regardless.
    let mut zebra = PathItem::default();
    zebra
        .set(
            "GET",
            Operation {
                operation_id: "getZebra".to_owned(),
                summary: None,
                description: None,
                tags: Vec::new(),
                parameters: Vec::new(),
                request_body: None,
                responses: BTreeMap::from([(
                    "200".to_owned(),
                    Response {
                        summary: "ok".to_owned(),
                        description: None,
                        content: BTreeMap::new(),
                    },
                )]),
            },
        )
        .expect("one operation");
    document.paths.insert("/zebra".to_owned(), zebra);

    let rendered = document.to_json().expect("serialises");
    let items_at = rendered.find("\"/items\"").expect("/items present");
    let zebra_at = rendered.find("\"/zebra\"").expect("/zebra present");
    assert!(
        items_at < zebra_at,
        "paths were not emitted in sorted order, so output depends on insertion order"
    );
}

#[test]
fn the_error_registry_reaches_the_document() {
    // Every code the runtime can raise must be enumerated in the published component, or the
    // description omits failures an application genuinely produces.
    let value = representative_document().to_value();
    let codes = value
        .pointer("/components/schemas/ProblemDetails/properties/code/enum")
        .and_then(Value::as_array)
        .expect("the Problem Details component enumerates the codes");

    for code in renvor_error::ApiErrorCode::ALL {
        assert!(
            codes
                .iter()
                .any(|value| value.as_str() == Some(code.as_str())),
            "`{code}` can be raised at runtime but is absent from the published document"
        );
    }
    assert_eq!(codes.len(), renvor_error::ApiErrorCode::ALL.len());
}

#[test]
fn a_declared_example_is_validated_against_its_own_schema() {
    // The representative document's example satisfies its schema.
    renvor_openapi::validate_examples(&representative_document())
        .expect("the representative document's examples are valid");

    // POSITIVE CONTROL: a WRONG example is refused. Without this the check above would pass for a
    // function that validated nothing.
    let mut document = representative_document();
    let path = document.paths.get_mut("/items").expect("/items");
    let operation = path.get.as_mut().expect("GET");
    let response = operation.responses.get_mut("200").expect("200");
    let content = response.content.get_mut(JSON_MEDIA_TYPE).expect("json");
    // The schema says an array of objects with a required integer `id`.
    content.example = Some(json!([{"id": "not an integer", "name": "x"}]));

    let error = renvor_openapi::validate_examples(&document)
        .expect_err("an example contradicting its schema must be refused");
    assert!(matches!(
        error,
        renvor_openapi::DocumentError::ExampleInvalid { .. }
    ));
}

#[test]
fn a_duplicate_operation_id_is_refused_rather_than_silently_kept() {
    let mut document = representative_document();

    let mut clash = PathItem::default();
    clash
        .set(
            "GET",
            Operation {
                // The SAME identifier the existing operation uses.
                operation_id: "listItems".to_owned(),
                summary: None,
                description: None,
                tags: Vec::new(),
                parameters: Vec::new(),
                request_body: None,
                responses: BTreeMap::from([(
                    "200".to_owned(),
                    Response {
                        summary: "ok".to_owned(),
                        description: None,
                        content: BTreeMap::new(),
                    },
                )]),
            },
        )
        .expect("one operation");
    document.paths.insert("/other".to_owned(), clash);

    assert!(
        document.check_operation_ids().is_err(),
        "a duplicate operation identifier was accepted"
    );

    // POSITIVE CONTROL: the document without the clash passes.
    assert!(representative_document().check_operation_ids().is_ok());
}

#[test]
fn declaring_the_same_method_twice_for_one_path_is_refused() {
    let mut path = PathItem::default();
    let operation = || Operation {
        operation_id: "x".to_owned(),
        summary: None,
        description: None,
        tags: Vec::new(),
        parameters: Vec::new(),
        request_body: None,
        responses: BTreeMap::new(),
    };
    path.set("GET", operation())
        .expect("the first registration");
    assert!(
        path.set("GET", operation()).is_err(),
        "a second GET replaced the first silently"
    );
    // POSITIVE CONTROL: a different method is not a duplicate.
    assert!(path.set("POST", operation()).is_ok());
}
