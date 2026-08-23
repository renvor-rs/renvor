//! An API that describes itself, from the declarations that validate it.
//!
//! Run it:
//!
//! ```text
//! cargo run -p renvor --features transport-rest --example described_api
//! ```
//!
//! # What this example is actually showing
//!
//! One `Declaration` per input. The runtime enforces it; the OpenAPI document publishes it. They
//! are the **same value** — so "the description matches the implementation" is not a thing anyone
//! has to maintain, and not a thing that can quietly stop being true.
//!
//! The last section runs the description through the same registry that would build the router,
//! and prints what a consumer would receive.

use renvor::openapi::Info;
use renvor::transport::route::describe;
use renvor::{Declaration, Location, OperationSpec, Request, Response, RouteRegistry};
use serde_json::json;

/// What `POST /items` accepts.
///
/// The constraints are declared with `schemars` attributes, which is the whole trick: they become
/// JSON Schema keywords, and JSON Schema keywords are what Renvor enforces. There is no second
/// place to write "name is between 1 and 64 characters".
#[derive(serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct CreateItem {
    /// The item's display name.
    #[schemars(length(min = 1, max = 64))]
    name: String,
    /// How many.
    #[schemars(range(min = 1, max = 1000))]
    quantity: u32,
    /// An optional note.
    note: Option<String>,
}

async fn list_items(_: Request) -> Response {
    Response::json(r#"{"items":[]}"#)
}

async fn create_item(request: Request) -> Response {
    // The body reaching here has ALREADY satisfied the declaration. A handler that re-checked
    // would be writing a second rule, and a second rule can disagree with the published one.
    let received = request.body().len();
    Response::json(format!(r#"{{"created":true,"bytes":{received}}}"#))
}

fn registry() -> RouteRegistry {
    let mut registry = RouteRegistry::new();

    registry
        .get("/items", list_items)
        .expect("a valid route")
        .describe(
            OperationSpec::new()
                .id("listItems")
                .summary("List items")
                .tag("items")
                .parameter(
                    Location::Query,
                    "page_size",
                    false,
                    Declaration::new(json!({"type": "integer", "minimum": 1, "maximum": 100}))
                        .expect("a valid declaration"),
                )
                .response(200, "A page of items", None),
        )
        .expect("a route to describe");

    registry
        .post("/items", create_item)
        .expect("a valid route")
        .describe(
            OperationSpec::new()
                .id("createItem")
                .summary("Create an item")
                .tag("items")
                // THE SAME declaration the runtime enforces.
                .body(
                    true,
                    Declaration::of::<CreateItem>()
                        .expect("the type is inside the enforced subset"),
                )
                // Validated against its own schema when the description is generated. An example
                // that contradicted its schema would be a reported error, not a warning.
                .body_example(json!({"name": "widget", "quantity": 3, "note": null}))
                .response(201, "The item was created", None),
        )
        .expect("a route to describe");

    registry
}

fn main() {
    let registry = registry();

    // An application answers the metadata invocations FIRST, before anything starts. That is what
    // makes `renvor routes` and `renvor openapi` free of boot side effects: they ask a question and
    // the process answers it and exits, without binding a port or opening a connection.
    let info = Info {
        title: "Items API".to_owned(),
        version: "1.0.0".to_owned(),
        description: Some("A small API that describes itself.".to_owned()),
    };

    if let Some(answer) =
        describe::answer_openapi_request(std::env::args(), &registry, info.clone())
    {
        println!("{answer}");
        return;
    }
    if let Some(answer) =
        renvor::transport::route::inspect::answer_dump_request(std::env::args(), &registry)
    {
        println!("{answer}");
        return;
    }

    // ---- the declaration enforces, at runtime ----
    let declaration =
        Declaration::of::<CreateItem>().expect("the type is inside the enforced subset");

    println!("A body the declaration accepts:");
    let good = json!({"name": "widget", "quantity": 3, "note": null});
    println!("  {good}");
    println!(
        "  issues: {}\n",
        declaration.validate(Location::Body, &good).len()
    );

    println!("A body it refuses, with every violation reported at once:");
    let bad = json!({"name": "", "quantity": 5000, "unexpected": true});
    println!("  {bad}");
    for issue in declaration.validate(Location::Body, &bad) {
        // Location, pointer, reason. NEVER the value — the type has nowhere to put one.
        println!(
            "  {} {} -> {}",
            issue.location,
            issue.pointer,
            issue.reason.as_str()
        );
    }

    // ---- and the SAME declaration is what the description publishes ----
    let document = describe::document(&registry, info).expect("the description generates");

    println!("\nThe description built from the same registry:");
    println!("  openapi:    {}", document.openapi);
    println!("  operations: {}", document.operation_ids().join(", "));

    let published = document.to_value();
    let body_schema = published
        .pointer("/paths/~1items/post/requestBody/content/application~1json/schema")
        .expect("the request body schema is published");

    println!("\nThe published schema and the enforced one are the same value:");
    println!("  equal: {}", body_schema == declaration.schema());

    println!(
        "\nRun with `--renvor-dump-openapi` to print the whole document, or \
         `--renvor-dump-routes` for the route table."
    );
}
