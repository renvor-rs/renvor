//! Route completeness, runtime-versus-schema agreement, and generation side effects.
//!
//! Every assertion here runs against the **same registry** the router is built from. That is the
//! point: a test that built a second registry for the description would be testing the thing
//! contract C-9 prohibits.

use std::collections::BTreeMap;

use renvor_error::{ApiErrorCode, Location};
use renvor_http::route::{
    Method, OperationSpec, Request, Response, RouteGroup, RouteRegistry, describe,
};
use renvor_openapi::Info;
use renvor_validation::Declaration;
use serde_json::json;

async fn ok(_: Request) -> Response {
    Response::json("{}")
}

fn info() -> Info {
    Info {
        title: "Test API".to_owned(),
        version: "1.0.0".to_owned(),
        description: None,
    }
}

/// A registry exercising groups, parameters, bodies, and undeclared routes.
fn registry() -> RouteRegistry {
    let group = RouteGroup::new("api-v1", "/api/v1")
        .expect("a valid prefix")
        .get("/items", ok)
        .expect("a valid route")
        .post("/items", ok)
        .expect("a valid route");

    let mut registry = RouteRegistry::new();
    registry.group(group).expect("the group registers");

    registry
        .get("/health", ok)
        .expect("a valid route")
        .describe(
            OperationSpec::new()
                .id("health")
                .summary("Liveness")
                .tag("ops")
                .response(200, "The application is alive", None),
        )
        .expect("a route to describe");

    registry
        .route(Method::Get, "/items/{id}", ok)
        .expect("a valid route")
        .describe(
            OperationSpec::new()
                .id("getItem")
                .parameter(
                    Location::Path,
                    "id",
                    true,
                    Declaration::new(json!({"type": "integer", "minimum": 1}))
                        .expect("a valid declaration"),
                )
                .parameter(
                    Location::Query,
                    "expand",
                    false,
                    Declaration::new(json!({"type": "string", "maxLength": 32}))
                        .expect("a valid declaration"),
                )
                .response(200, "The item", None)
                .error(ApiErrorCode::NotFound),
        )
        .expect("a route to describe");

    // A route with NO declarations. It must still appear.
    registry
        .route(Method::Delete, "/items/{id}", ok)
        .expect("a valid route");

    registry
}

#[test]
fn every_registered_route_appears_in_the_description_and_nothing_else_does() {
    let registry = registry();
    let document = describe::document(&registry, info()).expect("the description generates");

    // FROM the registry.
    let mut registered: Vec<(String, String)> = registry
        .routes()
        .iter()
        .map(|route| (route.method().as_str().to_owned(), route.path().to_owned()))
        .collect();
    registered.sort();

    // FROM the description.
    let described = document.method_paths();

    assert_eq!(
        registered, described,
        "the description and the registry disagree about which routes exist"
    );

    // And the set is non-empty, so the equality above is not two empty lists agreeing.
    assert!(registered.len() >= 5, "only {} routes", registered.len());
}

#[test]
fn a_route_registered_without_declarations_still_appears() {
    let registry = registry();
    let document = describe::document(&registry, info()).expect("the description generates");

    let deletes = document
        .method_paths()
        .into_iter()
        .filter(|(method, _)| method == "DELETE")
        .count();

    assert_eq!(
        deletes, 1,
        "the undeclared route was omitted; a missing route is the failure this rule prevents"
    );
}

#[test]
fn a_route_added_after_generation_changes_the_description() {
    // Proves the description is READ from the registry rather than produced once and cached — the
    // failure mode a second manifest would have.
    let mut registry = registry();
    let before = describe::document(&registry, info())
        .expect("generates")
        .method_paths()
        .len();

    registry
        .route(Method::Put, "/items/{id}", ok)
        .expect("a valid route");

    let after = describe::document(&registry, info())
        .expect("generates")
        .method_paths()
        .len();

    assert_eq!(
        after,
        before + 1,
        "the description did not follow the registry"
    );
}

#[test]
fn the_published_constraint_is_the_one_the_runtime_enforces() {
    // SC-004. Not "two documents agree" — the SAME VALUE, reached from both directions.
    let registry = registry();
    let document = describe::document(&registry, info()).expect("generates");
    let value = document.to_value();

    let published = value
        .pointer("/paths/~1items~1{id}/get/parameters")
        .and_then(|parameters| parameters.as_array())
        .expect("the operation declares parameters")
        .iter()
        .find(|parameter| parameter["name"] == json!("id"))
        .expect("the `id` parameter is published")
        .get("schema")
        .expect("it has a schema")
        .clone();

    // The registry's own copy.
    let route = registry
        .routes()
        .iter()
        .find(|route| route.method() == Method::Get && route.path() == "/items/{id}")
        .expect("the route exists");
    let enforced = route
        .spec()
        .expect("it declares a spec")
        .parameters
        .iter()
        .find(|parameter| parameter.name == "id")
        .expect("the `id` parameter is declared")
        .schema
        .schema()
        .clone();

    assert_eq!(published, enforced, "the published schema is a second copy");

    // And it genuinely enforces: `minimum: 1` refuses 0 at RUNTIME, which is what makes the
    // equality above meaningful rather than an equality between two unused values.
    let spec = route.spec().expect("a spec");
    let mut params = BTreeMap::new();
    params.insert("id".to_owned(), "0".to_owned());
    assert!(
        spec.validate(&params, "", &BTreeMap::new(), b"").is_err(),
        "the published `minimum: 1` is not enforced at runtime"
    );

    params.insert("id".to_owned(), "1".to_owned());
    assert!(
        spec.validate(&params, "", &BTreeMap::new(), b"").is_ok(),
        "a satisfying value was refused"
    );
}

#[test]
fn every_declared_error_code_appears_in_the_description() {
    let registry = registry();
    let value = describe::document(&registry, info())
        .expect("generates")
        .to_value();

    let codes = value
        .pointer("/components/schemas/ProblemDetails/properties/code/enum")
        .and_then(|codes| codes.as_array())
        .expect("the shared component enumerates the codes");

    for code in ApiErrorCode::ALL {
        assert!(
            codes
                .iter()
                .any(|value| value.as_str() == Some(code.as_str())),
            "`{code}` can be raised but is absent from the description"
        );
    }

    // The operation that declared `not_found` carries a 404 response.
    assert!(
        value
            .pointer("/paths/~1items~1{id}/get/responses/404")
            .is_some(),
        "a declared error code produced no response"
    );
}

#[test]
fn generation_is_deterministic_across_runs() {
    let registry = registry();
    let first = describe::document(&registry, info())
        .expect("generates")
        .to_json()
        .expect("serialises");
    let second = describe::document(&registry, info())
        .expect("generates")
        .to_json()
        .expect("serialises");
    assert_eq!(first, second);
}

#[test]
fn generation_binds_nothing_and_starts_nothing() {
    // FR-031, SC-009. The strongest available observable assertion in-process: generation runs
    // with NO async runtime at all.
    //
    // Binding a listener, opening a connection, or starting a provider requires a reactor.
    // `tokio::net`, `tokio::time`, and every provider boot path panic with "there is no reactor
    // running" outside one. This test is a plain `#[test]` — no `#[tokio::test]`, no runtime — so
    // if generation attempted any of them, this would panic rather than pass.
    let registry = registry();
    let document = describe::document(&registry, info()).expect("generation needs no runtime");
    assert!(!document.paths.is_empty());

    // POSITIVE CONTROL: prove that the absence of a runtime IS detectable here, so the assertion
    // above is not passing because nothing would have failed anyway.
    let attempted = std::panic::catch_unwind(|| {
        let _ = tokio::runtime::Handle::current();
    });
    assert!(
        attempted.is_err(),
        "a tokio runtime is present, so this test could not have detected one being started"
    );
}

#[test]
fn a_duplicate_operation_id_is_refused() {
    let mut registry = RouteRegistry::new();
    registry
        .get("/a", ok)
        .expect("a valid route")
        .describe(OperationSpec::new().id("same"))
        .expect("describes");
    registry
        .get("/b", ok)
        .expect("a valid route")
        .describe(OperationSpec::new().id("same"))
        .expect("describes");

    assert!(
        matches!(
            describe::document(&registry, info()),
            Err(describe::DescribeError::DuplicateOperationId { .. })
        ),
        "two operations shared an identifier and the document was produced anyway"
    );

    // POSITIVE CONTROL: distinct identifiers generate.
    let mut fine = RouteRegistry::new();
    fine.get("/a", ok)
        .expect("a valid route")
        .describe(OperationSpec::new().id("first"))
        .expect("describes");
    fine.get("/b", ok)
        .expect("a valid route")
        .describe(OperationSpec::new().id("second"))
        .expect("describes");
    assert!(describe::document(&fine, info()).is_ok());
}

#[test]
fn describing_before_registering_a_route_is_refused() {
    let mut registry = RouteRegistry::new();
    assert!(
        registry
            .describe(OperationSpec::new().id("nowhere"))
            .is_err(),
        "a declaration with no route was accepted and silently dropped"
    );
}

#[test]
fn an_example_that_contradicts_its_schema_fails_generation() {
    let mut registry = RouteRegistry::new();
    registry
        .post("/items", ok)
        .expect("a valid route")
        .describe(
            OperationSpec::new()
                .id("createItem")
                .body(
                    true,
                    Declaration::new(json!({
                        "type": "object",
                        "required": ["name"],
                        "properties": {"name": {"type": "string", "minLength": 3}}
                    }))
                    .expect("a valid declaration"),
                )
                // `name` is two characters, and the schema requires three.
                .body_example(json!({"name": "ab"})),
        )
        .expect("describes");

    assert!(
        matches!(
            describe::document(&registry, info()),
            Err(describe::DescribeError::ExampleInvalid { .. })
        ),
        "an example contradicting its schema was published"
    );

    // POSITIVE CONTROL: a satisfying example generates.
    let mut fine = RouteRegistry::new();
    fine.post("/items", ok)
        .expect("a valid route")
        .describe(
            OperationSpec::new()
                .id("createItem")
                .body(
                    true,
                    Declaration::new(json!({
                        "type": "object",
                        "required": ["name"],
                        "properties": {"name": {"type": "string", "minLength": 3}}
                    }))
                    .expect("a valid declaration"),
                )
                .body_example(json!({"name": "abc"})),
        )
        .expect("describes");
    assert!(describe::document(&fine, info()).is_ok());
}

#[test]
fn the_generated_document_declares_openapi_3_2_0() {
    let value = describe::document(&registry(), info())
        .expect("generates")
        .to_value();
    assert_eq!(value.get("openapi").and_then(|v| v.as_str()), Some("3.2.0"));
}
