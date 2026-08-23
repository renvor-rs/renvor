//! The compatibility gate, exercised by mutation.
//!
//! # Both directions are required
//!
//! A gate that fails on every change is not a compatibility gate; it is an obstacle, and an
//! obstacle gets routed around. So the harmless mutations below are as load-bearing as the
//! breaking ones: each one asserts that the gate stays **quiet** for a change that cannot break a
//! consumer.
//!
//! # The mutations are applied to a real document
//!
//! Each starts from the same baseline and changes exactly one thing, so a verdict is attributable
//! to that change rather than to an accumulation.

use std::collections::BTreeMap;

use renvor_openapi::{
    ChangeClass, Document, Info, JSON_MEDIA_TYPE, MediaType, Operation, Parameter,
    ParameterLocation, PathItem, RequestBody, Response, Severity, breaking, compare, is_breaking,
};
use serde_json::{Value, json};

fn baseline() -> Document {
    let mut responses = BTreeMap::new();
    responses.insert(
        "200".to_owned(),
        Response {
            summary: "The item".to_owned(),
            description: Some("The requested item.".to_owned()),
            content: BTreeMap::from([(
                JSON_MEDIA_TYPE.to_owned(),
                MediaType {
                    schema: json!({
                        "type": "object",
                        "required": ["id", "name"],
                        "properties": {
                            "id": {"type": "integer"},
                            "name": {"type": "string"}
                        }
                    }),
                    example: None,
                },
            )]),
        },
    );
    responses.insert(
        "404".to_owned(),
        Response {
            summary: "No such item".to_owned(),
            description: None,
            content: BTreeMap::new(),
        },
    );

    let mut item = PathItem::default();
    item.set(
        "GET",
        Operation {
            operation_id: "getItem".to_owned(),
            summary: Some("Get an item".to_owned()),
            description: None,
            tags: Vec::new(),
            parameters: vec![
                Parameter {
                    name: "id".to_owned(),
                    location: ParameterLocation::Path,
                    description: None,
                    required: true,
                    schema: json!({"type": "integer", "minimum": 1}),
                },
                Parameter {
                    name: "verbose".to_owned(),
                    location: ParameterLocation::Query,
                    description: None,
                    required: false,
                    schema: json!({"type": "boolean"}),
                },
            ],
            request_body: None,
            responses,
        },
    )
    .expect("one operation per method");

    item.set(
        "POST",
        Operation {
            operation_id: "createItem".to_owned(),
            summary: None,
            description: None,
            tags: Vec::new(),
            parameters: Vec::new(),
            request_body: Some(RequestBody {
                description: None,
                required: false,
                content: BTreeMap::from([(
                    JSON_MEDIA_TYPE.to_owned(),
                    MediaType {
                        schema: json!({
                            "type": "object",
                            "required": ["name"],
                            "properties": {
                                "name": {"type": "string", "minLength": 1, "maxLength": 64},
                                "status": {"enum": ["draft", "live"]}
                            }
                        }),
                        example: None,
                    },
                )]),
            }),
            responses: BTreeMap::from([(
                "201".to_owned(),
                Response {
                    summary: "Created".to_owned(),
                    description: None,
                    content: BTreeMap::new(),
                },
            )]),
        },
    )
    .expect("one operation per method");

    let mut document = Document::new(Info {
        title: "Items".to_owned(),
        version: "1.0.0".to_owned(),
        description: None,
    })
    .with_problem_component();
    document.paths.insert("/items/{id}".to_owned(), item);
    document
}

/// Applies `mutate` to a JSON copy of the baseline and returns the classified changes.
fn mutated(mutate: impl FnOnce(&mut Value)) -> Vec<renvor_openapi::Change> {
    let before = baseline().to_value();
    let mut after = before.clone();
    mutate(&mut after);
    compare(&before, &after)
}

fn assert_breaking(name: &str, class: ChangeClass, changes: &[renvor_openapi::Change]) {
    assert!(
        is_breaking(changes),
        "`{name}` was NOT reported as breaking. Changes: {changes:?}"
    );
    assert!(
        changes.iter().any(|change| change.class == class),
        "`{name}` was reported as breaking, but not as `{}`. Changes: {changes:?}",
        class.as_str()
    );
}

fn assert_not_breaking(name: &str, changes: &[renvor_openapi::Change]) {
    assert!(
        !is_breaking(changes),
        "`{name}` was reported as breaking, which is a FALSE POSITIVE. Breaking: {:?}",
        breaking(changes)
    );
}

// ---------------------------------------------------------------------------------------------
// BREAKING mutations — the gate must fail
// ---------------------------------------------------------------------------------------------

#[test]
fn removing_a_route_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]
            .as_object_mut()
            .expect("a path item")
            .remove("get");
    });
    assert_breaking("route removed", ChangeClass::OperationRemoved, &changes);
}

#[test]
fn changing_a_path_is_breaking() {
    let changes = mutated(|document| {
        let paths = document["paths"].as_object_mut().expect("paths");
        let item = paths.remove("/items/{id}").expect("the path");
        paths.insert("/things/{id}".to_owned(), item);
    });
    // Both directions are reported: the old route is gone, and a new one appeared.
    assert_breaking("path changed", ChangeClass::OperationRemoved, &changes);
}

#[test]
fn changing_a_method_is_breaking() {
    let changes = mutated(|document| {
        let item = document["paths"]["/items/{id}"]
            .as_object_mut()
            .expect("a path item");
        let operation = item.remove("get").expect("GET");
        item.insert("patch".to_owned(), operation);
    });
    assert_breaking("method changed", ChangeClass::OperationRemoved, &changes);
}

#[test]
fn changing_an_operation_id_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["operationId"] = json!("fetchItem");
    });
    assert_breaking(
        "operationId changed",
        ChangeClass::OperationIdChanged,
        &changes,
    );
}

#[test]
fn making_an_optional_parameter_required_is_breaking() {
    let changes = mutated(|document| {
        let parameters = document["paths"]["/items/{id}"]["get"]["parameters"]
            .as_array_mut()
            .expect("parameters");
        for parameter in parameters {
            if parameter["name"] == json!("verbose") {
                parameter["required"] = json!(true);
            }
        }
    });
    assert_breaking(
        "optional parameter became required",
        ChangeClass::ParameterBecameRequired,
        &changes,
    );
}

#[test]
fn adding_a_required_parameter_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["parameters"]
            .as_array_mut()
            .expect("parameters")
            .push(json!({
                "name": "tenant",
                "in": "header",
                "required": true,
                "schema": {"type": "string"}
            }));
    });
    assert_breaking(
        "required parameter added",
        ChangeClass::RequiredParameterAdded,
        &changes,
    );
}

#[test]
fn adding_a_required_body_member_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["post"]["requestBody"]["content"][JSON_MEDIA_TYPE]["schema"]
            ["required"] = json!(["name", "status"]);
    });
    assert_breaking(
        "required body member added",
        ChangeClass::RequiredParameterAdded,
        &changes,
    );
}

#[test]
fn making_a_request_body_required_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["post"]["requestBody"]["required"] = json!(true);
    });
    assert_breaking(
        "request body became required",
        ChangeClass::RequestBodyBecameRequired,
        &changes,
    );
}

#[test]
fn changing_a_type_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["parameters"][0]["schema"]["type"] =
            json!("string");
    });
    assert_breaking("type changed", ChangeClass::TypeChanged, &changes);
}

#[test]
fn narrowing_a_constraint_is_breaking() {
    // maxLength FALLING refuses input a consumer previously sent successfully.
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["post"]["requestBody"]["content"][JSON_MEDIA_TYPE]["schema"]
            ["properties"]["name"]["maxLength"] = json!(16);
    });
    assert_breaking(
        "maxLength narrowed",
        ChangeClass::ConstraintNarrowed,
        &changes,
    );
}

#[test]
fn raising_a_minimum_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["parameters"][0]["schema"]["minimum"] = json!(100);
    });
    assert_breaking("minimum raised", ChangeClass::ConstraintNarrowed, &changes);
}

#[test]
fn adding_a_constraint_that_did_not_exist_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["post"]["requestBody"]["content"][JSON_MEDIA_TYPE]["schema"]
            ["properties"]["name"]["minItems"] = json!(1);
    });
    assert_breaking(
        "a new bound appeared",
        ChangeClass::ConstraintNarrowed,
        &changes,
    );
}

#[test]
fn removing_a_declared_response_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["responses"]
            .as_object_mut()
            .expect("responses")
            .remove("404");
    });
    assert_breaking("response removed", ChangeClass::ResponseRemoved, &changes);
}

#[test]
fn removing_a_public_error_code_is_breaking() {
    let changes = mutated(|document| {
        let codes =
            document["components"]["schemas"]["ProblemDetails"]["properties"]["code"]["enum"]
                .as_array_mut()
                .expect("the code enum");
        codes.retain(|code| code != &json!("not_found"));
    });
    assert_breaking(
        "error code removed",
        ChangeClass::ErrorCodeRemoved,
        &changes,
    );
}

#[test]
fn removing_a_guaranteed_response_member_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["responses"]["200"]["content"][JSON_MEDIA_TYPE]["schema"]
            ["required"] = json!(["id"]);
    });
    assert_breaking(
        "guaranteed response member removed",
        ChangeClass::ResponseMemberRemoved,
        &changes,
    );
}

#[test]
fn removing_a_content_type_is_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["responses"]["200"]["content"]
            .as_object_mut()
            .expect("content")
            .remove(JSON_MEDIA_TYPE);
    });
    assert_breaking(
        "content type removed",
        ChangeClass::ContentTypeRemoved,
        &changes,
    );
}

// ---------------------------------------------------------------------------------------------
// HARMLESS mutations — the gate must stay quiet
// ---------------------------------------------------------------------------------------------

#[test]
fn changing_a_description_is_not_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["responses"]["200"]["description"] =
            json!("A completely rewritten description of the same thing.");
    });
    assert_not_breaking("description changed", &changes);
}

#[test]
fn changing_a_summary_is_not_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["summary"] = json!("Reworded");
        document["paths"]["/items/{id}"]["get"]["responses"]["200"]["summary"] = json!("Reworded");
    });
    assert_not_breaking("summary changed", &changes);
}

#[test]
fn adding_an_example_is_not_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["responses"]["200"]["content"][JSON_MEDIA_TYPE]["example"] =
            json!({"id": 1, "name": "widget"});
    });
    assert_not_breaking("example added", &changes);
}

#[test]
fn adding_an_optional_parameter_is_not_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["parameters"]
            .as_array_mut()
            .expect("parameters")
            .push(json!({
                "name": "expand",
                "in": "query",
                "required": false,
                "schema": {"type": "string"}
            }));
    });
    assert_not_breaking("optional parameter added", &changes);
}

#[test]
fn widening_a_constraint_is_not_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["post"]["requestBody"]["content"][JSON_MEDIA_TYPE]["schema"]
            ["properties"]["name"]["maxLength"] = json!(256);
    });
    assert_not_breaking("maxLength widened", &changes);
    assert!(
        changes
            .iter()
            .any(|change| change.class == ChangeClass::ConstraintWidened),
        "the widening was not detected at all, so the test above proves nothing: {changes:?}"
    );
}

#[test]
fn adding_an_enum_member_is_not_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["post"]["requestBody"]["content"][JSON_MEDIA_TYPE]["schema"]
            ["properties"]["status"]["enum"] = json!(["draft", "live", "archived"]);
    });
    assert_not_breaking("enum member added", &changes);
}

#[test]
fn adding_a_response_or_an_operation_or_an_error_code_is_not_breaking() {
    let changes = mutated(|document| {
        document["paths"]["/items/{id}"]["get"]["responses"]["500"] =
            json!({"summary": "Internal error"});
    });
    assert_not_breaking("response added", &changes);

    let changes = mutated(|document| {
        document["paths"]["/other"] = json!({
            "get": {"operationId": "listOther", "responses": {"200": {"summary": "ok"}}}
        });
    });
    assert_not_breaking("operation added", &changes);

    let changes = mutated(|document| {
        document["components"]["schemas"]["ProblemDetails"]["properties"]["code"]["enum"]
            .as_array_mut()
            .expect("the code enum")
            .push(json!("a_new_code"));
    });
    assert_not_breaking("error code added", &changes);
}

// ---------------------------------------------------------------------------------------------
// The gate's own integrity
// ---------------------------------------------------------------------------------------------

#[test]
fn an_unchanged_document_produces_no_changes_at_all() {
    let document = baseline().to_value();
    let changes = compare(&document, &document);
    assert!(
        changes.is_empty(),
        "comparing a document with itself produced changes: {changes:?}"
    );
}

#[test]
fn a_regenerated_snapshot_cannot_approve_its_own_breaking_diff() {
    // FR-048. The hazard: if the gate regenerated BOTH sides, every comparison would be a document
    // against itself and would always pass — including for a change that removes a route.
    let mut broken = baseline();
    broken.paths.remove("/items/{id}");
    let broken = broken.to_value();

    // Self-comparison: silent. This is exactly the bypass, demonstrated.
    assert!(
        compare(&broken, &broken).is_empty(),
        "the self-comparison did not behave as described, so the demonstration below is unsound"
    );

    // Against the COMMITTED snapshot — the real file, read from the repository, not a value this
    // test just built. The break is caught. The gate's correctness depends on the baseline side
    // being one the caller did not just produce, which is exactly why it is read from committed
    // history rather than regenerated.
    let committed: serde_json::Value =
        serde_json::from_str(COMMITTED).expect("the committed snapshot is valid JSON");
    let changes = compare(&committed, &broken);
    assert!(
        is_breaking(&changes),
        "comparing against the committed snapshot missed a removed route"
    );
}

#[test]
fn the_report_is_deterministic() {
    let before = baseline().to_value();
    let mut after = before.clone();
    after["paths"]["/items/{id}"]["get"]["operationId"] = json!("renamed");
    after["paths"]["/items/{id}"]["get"]["parameters"][0]["schema"]["minimum"] = json!(50);

    let first = compare(&before, &after);
    let second = compare(&before, &after);
    assert_eq!(first, second, "two comparisons of the same pair differ");
}

#[test]
fn every_change_class_declares_a_severity_consistent_with_its_name() {
    // A guard against a future variant being added to the enum without a considered severity.
    for (class, expected) in [
        (ChangeClass::OperationRemoved, Severity::Breaking),
        (ChangeClass::OperationAdded, Severity::Compatible),
        (ChangeClass::ConstraintNarrowed, Severity::Breaking),
        (ChangeClass::ConstraintWidened, Severity::Compatible),
        (ChangeClass::ResponseRemoved, Severity::Breaking),
        (ChangeClass::ResponseAdded, Severity::Compatible),
        (ChangeClass::ErrorCodeRemoved, Severity::Breaking),
        (ChangeClass::ErrorCodeAdded, Severity::Compatible),
    ] {
        assert_eq!(
            class.severity(),
            expected,
            "`{}` has the wrong severity",
            class.as_str()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The committed snapshot — FR-043, FR-044
// ---------------------------------------------------------------------------------------------

/// The snapshot, read from the repository rather than regenerated.
///
/// `include_str!` resolves at compile time against the committed file, so the baseline side of
/// every comparison below comes from **committed history**. That is the property FR-048 depends
/// on: a gate that regenerated both sides would compare a document with itself and pass for any
/// change at all.
const COMMITTED: &str = include_str!("snapshots/public-description.json");

#[test]
fn the_generated_description_matches_the_committed_snapshot() {
    // FR-044. `baseline()` stands in for an application's declarations: it is the generated side.
    // `COMMITTED` is the recorded side. A breaking drift between them fails here.
    let committed: serde_json::Value =
        serde_json::from_str(COMMITTED).expect("the committed snapshot is valid JSON");
    let generated = baseline().to_value();

    let changes = compare(&committed, &generated);
    assert!(
        !is_breaking(&changes),
        "the generated description breaks the committed snapshot: {:?}",
        breaking(&changes)
    );
}

#[test]
fn the_committed_snapshot_is_byte_identical_to_what_generation_produces() {
    // Stronger than the semantic check above, and deliberately separate from it: this one fails
    // for ANY drift, including harmless ones, so the committed file cannot quietly go stale while
    // the semantic gate keeps passing.
    let regenerated = baseline().to_json().expect("serialises");
    assert_eq!(
        regenerated.trim(),
        COMMITTED.trim(),
        "the committed snapshot is stale — refresh it with \
         `cargo test -p renvor-openapi --test compatibility refresh_the_committed_snapshot \
         -- --ignored --nocapture` and commit the result"
    );
}

/// How the snapshot is updated — the procedure FR-043 requires be stated.
///
/// Prints the current description. A maintainer copies it into
/// `tests/snapshots/public-description.json` and commits it **as a reviewed change**. It is
/// `#[ignore]`d so it never runs in the gate, and it only prints — it cannot write the file it
/// would be approving, which is what keeps FR-048 intact.
#[test]
#[ignore = "generator — run with --ignored to print a refreshed snapshot"]
fn refresh_the_committed_snapshot() {
    println!(
        "---BEGIN---\n{}\n---END---",
        baseline().to_json().expect("serialises")
    );
}
