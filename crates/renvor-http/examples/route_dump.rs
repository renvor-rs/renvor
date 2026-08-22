//! A minimal application that answers the route-dump invocation.
//!
//! # Why this exists as an example rather than only as a test
//!
//! `renvor routes` relays what an **application binary** prints. Every test of that relay needs a
//! binary that speaks the protocol, and a hand-written fixture string is not one — it would prove
//! the CLI parses what the test author believed the library emits, which is exactly the kind of
//! second source contract C-9 prohibits.
//!
//! This is a real binary, built from the real library, answering through the real
//! [`renvor_http::route::inspect::answer_dump_request`]. Running it is running the protocol.
//!
//! # It boots nothing
//!
//! The dump is answered **before** anything else happens. A protocol that required booting the
//! application to list its routes would make `renvor routes` start databases, bind ports, and run
//! migrations as a side effect of asking a question.

use renvor_http::route::{Request, Response, RouteGroup, RouteRegistry, inspect};

fn main() {
    let registry = build_registry();

    // FIRST, before any other work. Answering and exiting is the whole protocol.
    if let Some(document) = inspect::answer_dump_request(std::env::args(), &registry) {
        println!("{document}");
        return;
    }

    // An ordinary run would serve `registry` here. Nothing above this line depends on that, which
    // is what makes the dump free of boot side effects.
    println!("this example only demonstrates the route-dump protocol");
}

async fn ok(_: Request) -> Response {
    Response::text("ok")
}

fn build_registry() -> RouteRegistry {
    let group = RouteGroup::new("api-v1", "/api/v1")
        .expect("a valid prefix")
        .get("/health", ok)
        .expect("a valid route")
        .post("/items", ok)
        .expect("a valid route");

    let mut registry = RouteRegistry::new();
    registry.group(group).expect("the group registers");
    registry.get("/", ok).expect("the root route registers");
    registry
}
