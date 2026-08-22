//! Rendering the route table — from the **same registry** the router is built from.
//!
//! # Why this takes a reference rather than a snapshot
//!
//! [`render_human`] and [`render_json`] both take `&RouteRegistry`. There is no snapshot type, no
//! builder, and no intermediate manifest — so there is nothing that could be produced once, stored,
//! and then disagree with the router later.
//!
//! Contract C-9 states the prohibition; this signature is what makes it structural rather than a
//! rule someone has to keep.
//!
//! # The output is deterministic
//!
//! Sorted by path, then by method in canonical order. Two runs against the same registry produce
//! byte-identical output, so a diff between two runs means the routes changed.

use serde::Serialize;

use super::RouteRegistry;

/// The documented invocation an application binary answers with its route registry.
///
/// A long, prefixed name on purpose: it must not collide with a flag an application defines for
/// itself, and it must be obvious in a process list that Renvor asked for it.
///
/// This constant is the **contract**. `renvor routes` passes it; [`answer_dump_request`] responds
/// to it. Both read this one value, so the CLI and the application cannot disagree about the
/// spelling.
pub const DUMP_FLAG: &str = "--renvor-dump-routes";

/// One row of the rendered route table.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RouteRow {
    /// The method, canonically uppercased.
    pub method: String,
    /// The full path, group prefix already applied.
    pub path: String,
    /// The owning group, or `null`.
    pub group: Option<String>,
}

/// The `result` payload of the structured form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RouteReport {
    /// Every declared route, sorted.
    pub routes: Vec<RouteRow>,
}

/// Collects the registry into sorted rows.
#[must_use]
pub fn rows(registry: &RouteRegistry) -> Vec<RouteRow> {
    let mut rows: Vec<RouteRow> = registry
        .routes()
        .iter()
        .map(|route| RouteRow {
            method: route.method().as_str().to_owned(),
            path: route.path().to_owned(),
            group: route.group().map(str::to_owned),
        })
        .collect();

    // Path first, because that is what an operator scans for. Method second, so the order within a
    // path is stable rather than dependent on registration.
    rows.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.method.cmp(&right.method))
    });
    rows
}

/// Renders the human table.
///
/// Column widths are computed from the content, so a long path does not push the table out of
/// alignment and a short one does not leave it sparse.
#[must_use]
pub fn render_human(registry: &RouteRegistry) -> String {
    let rows = rows(registry);
    if rows.is_empty() {
        // An empty registry is a fact, and saying so is different from printing an empty table and
        // letting a reader wonder whether the command worked.
        return "no routes are registered\n".to_owned();
    }

    let method_width = rows
        .iter()
        .map(|row| row.method.len())
        .chain(core::iter::once("METHOD".len()))
        .max()
        .unwrap_or(6);
    let path_width = rows
        .iter()
        .map(|row| row.path.len())
        .chain(core::iter::once("PATH".len()))
        .max()
        .unwrap_or(4);

    let mut out = format!(
        "{:<method_width$}  {:<path_width$}  GROUP\n",
        "METHOD", "PATH"
    );
    for row in rows {
        let group = row.group.unwrap_or_else(|| "-".to_owned());
        out.push_str(&format!(
            "{:<method_width$}  {:<path_width$}  {group}\n",
            row.method, row.path
        ));
    }
    out
}

/// Renders the structured form's `result` payload.
///
/// The caller wraps this in the C-2 envelope, so the envelope is written in exactly one place in
/// the workspace rather than once per command.
///
/// # Errors
///
/// Propagates a serialisation failure rather than substituting an empty document — a consumer that
/// asked for JSON must not receive something that parses but says nothing.
pub fn render_json(registry: &RouteRegistry) -> Result<String, serde_json::Error> {
    serde_json::to_string(&RouteReport {
        routes: rows(registry),
    })
}

/// Answers the route-dump invocation, if that is what this process was asked for.
///
/// An application calls this at the top of `main` and exits when it returns `Some`:
///
/// ```
/// # use renvor_http::route::{RouteRegistry, inspect};
/// # fn build_registry() -> RouteRegistry { RouteRegistry::new() }
/// let registry = build_registry();
/// if let Some(document) = inspect::answer_dump_request(std::env::args(), &registry) {
///     println!("{document}");
///     return;
/// }
/// # ()
/// ```
///
/// # Why this exists rather than leaving applications to implement the convention
///
/// A convention every application re-implements is a convention every application implements
/// slightly differently, and `renvor routes` would then be parsing a shape that varies by project.
/// This is the one implementation, so the envelope, the sorting, and the flag spelling are the
/// same everywhere.
///
/// Returns `None` when the invocation was not a dump request, so an ordinary run is unaffected.
///
/// # Errors
///
/// A serialisation failure is rendered as the C-2 **failure** envelope rather than propagated:
/// this function's caller is a `main` that is about to print and exit, and giving it a `Result`
/// would invite it to print nothing at all. A consumer that asked for JSON receives JSON either
/// way, which is what contract C-2 requires.
#[must_use]
pub fn answer_dump_request<I, S>(arguments: I, registry: &RouteRegistry) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if !arguments
        .into_iter()
        .any(|argument| argument.as_ref() == DUMP_FLAG)
    {
        return None;
    }

    Some(match render_json(registry) {
        Ok(result) => format!(
            r#"{{"schemaVersion":2,"status":"success","command":"routes","result":{result}}}"#
        ),
        Err(_) => concat!(
            r#"{"schemaVersion":2,"status":"failure","command":"routes","#,
            r#""error":{"code":"internal","message":"the route registry could not be serialised","#,
            r#""details":{}}}"#
        )
        .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{DUMP_FLAG, answer_dump_request, render_human, render_json, rows};
    use crate::route::{Request, Response, RouteGroup, RouteRegistry};

    async fn ok(_: Request) -> Response {
        Response::text("ok")
    }

    fn registry() -> RouteRegistry {
        let group = RouteGroup::new("api-v1", "/api/v1")
            .expect("valid prefix")
            .get("/health", ok)
            .expect("valid route")
            .post("/items", ok)
            .expect("valid route");

        let mut registry = RouteRegistry::new();
        registry.group(group).expect("group registers");
        registry.get("/", ok).expect("root registers");
        registry
    }

    #[test]
    fn inspection_reports_every_registered_route() {
        let registry = registry();
        let rows = rows(&registry);
        assert_eq!(rows.len(), registry.len());
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn output_is_sorted_and_therefore_deterministic() {
        let registry = registry();
        let first = render_json(&registry).expect("serialises");
        let second = render_json(&registry).expect("serialises");
        assert_eq!(first, second, "two renders of one registry differed");

        let rows = rows(&registry);
        let paths: Vec<&str> = rows.iter().map(|row| row.path.as_str()).collect();
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        assert_eq!(paths, sorted, "rows are not sorted by path");
    }

    #[test]
    fn a_route_added_to_the_registry_appears_without_a_second_manifest_being_touched() {
        // FR-007 / SC-014. The only edit is to the registry.
        let mut registry = registry();
        let before = rows(&registry).len();

        registry.get("/added", ok).expect("registers");

        let after = rows(&registry);
        assert_eq!(after.len(), before + 1);
        assert!(
            after.iter().any(|row| row.path == "/added"),
            "the new route did not reach inspection"
        );
    }

    #[test]
    fn the_group_is_reported() {
        let registry = registry();
        let health = rows(&registry)
            .into_iter()
            .find(|row| row.path == "/api/v1/health")
            .expect("the grouped route is present");
        assert_eq!(health.group.as_deref(), Some("api-v1"));

        // POSITIVE CONTROL: an ungrouped route reports no group, so the value above is read from
        // the route rather than being a constant.
        let root = rows(&registry)
            .into_iter()
            .find(|row| row.path == "/")
            .expect("the root route is present");
        assert_eq!(root.group, None);
    }

    #[test]
    fn an_empty_registry_says_so_rather_than_printing_an_empty_table() {
        let registry = RouteRegistry::new();
        let rendered = render_human(&registry);
        assert!(rendered.contains("no routes"), "{rendered}");
        assert!(!rendered.contains("METHOD"), "{rendered}");
    }

    #[test]
    fn the_dump_request_is_answered_only_when_it_is_asked_for() {
        let registry = registry();

        // An ordinary invocation is unaffected.
        assert!(answer_dump_request(["my-app", "--port", "8080"], &registry).is_none());
        assert!(answer_dump_request(Vec::<String>::new(), &registry).is_none());

        // POSITIVE CONTROL: the documented flag IS answered.
        let answered = answer_dump_request(["my-app", DUMP_FLAG], &registry)
            .expect("the documented invocation must be answered");
        assert!(answered.contains(r#""command":"routes""#), "{answered}");
        assert!(answered.contains(r#""schemaVersion":2"#), "{answered}");
        assert!(answered.contains(r#""status":"success""#), "{answered}");
    }

    #[test]
    fn the_dump_reports_the_same_routes_the_registry_holds() {
        // FR-045 and SC-014. The dump is not a second source: it renders the SAME value.
        let registry = registry();
        let answered = answer_dump_request(["app", DUMP_FLAG], &registry).expect("answered");
        let payload = render_json(&registry).expect("serialises");

        assert!(
            answered.contains(&payload),
            "the dump payload is not the registry's own rendering"
        );

        for route in registry.routes() {
            assert!(
                answered.contains(route.path()),
                "`{}` is in the registry but not in the dump",
                route.path()
            );
        }
    }

    #[test]
    fn the_dump_is_a_single_parseable_json_document() {
        // Contract C-2: exactly one JSON document, for success and failure alike.
        let registry = registry();
        let answered = answer_dump_request(["app", DUMP_FLAG], &registry).expect("answered");
        let parsed: serde_json::Value =
            serde_json::from_str(&answered).expect("the dump must be one JSON document");
        assert_eq!(parsed["schemaVersion"], 2);
        assert_eq!(parsed["status"], "success");
        assert_eq!(
            parsed["result"]["routes"]
                .as_array()
                .expect("routes is an array")
                .len(),
            registry.len()
        );
    }

    #[test]
    fn the_human_table_carries_a_header_and_one_row_per_route() {
        let registry = registry();
        let rendered = render_human(&registry);
        assert!(rendered.starts_with("METHOD"), "{rendered}");
        assert_eq!(rendered.lines().count(), registry.len() + 1);
    }
}
