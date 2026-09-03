//! The span and field names Renvor records (FR-079).
//!
//! Where the OpenTelemetry semantic conventions define a name, Renvor uses it, spelled here once
//! so every crate records the same literal and the observability crate can assert — when the
//! conventions crate is compiled beside it — that each literal equals the published constant.
//! Where no convention exists, the name is Renvor's own under `renvor.`.
//!
//! The kernel holds these because the crates that record them (`renvor-http`, `renvor-jobs`)
//! depend inward only, and the observability crate that exports them must not be a dependency of
//! either (FR-087).

/// `http.request.method` — the request method.
pub const HTTP_REQUEST_METHOD: &str = "http.request.method";
/// `http.route` — the matched route template, never the raw path.
pub const HTTP_ROUTE: &str = "http.route";
/// `http.response.status_code` — the status sent.
pub const HTTP_RESPONSE_STATUS_CODE: &str = "http.response.status_code";
/// `url.path` — the request path.
pub const URL_PATH: &str = "url.path";
/// `messaging.system` — the messaging system; Renvor records `renvor_jobs`.
pub const MESSAGING_SYSTEM: &str = "messaging.system";
/// `messaging.destination.name` — the queue.
pub const MESSAGING_DESTINATION_NAME: &str = "messaging.destination.name";
/// `messaging.operation.type` — `send`, `process`, and so on.
pub const MESSAGING_OPERATION_TYPE: &str = "messaging.operation.type";
/// `db.system.name` — `postgresql` or `mysql`.
pub const DB_SYSTEM_NAME: &str = "db.system.name";

/// Renvor's own names, where no convention exists.
pub mod renvor {
    /// The inbound trace identifier recorded on a request span from a valid `traceparent`.
    pub const TRACE_ID: &str = "trace_id";
    /// The inbound parent span identifier.
    pub const PARENT_SPAN_ID: &str = "parent_span_id";
    /// The inbound trace flags, two lowercase hex digits.
    pub const TRACE_FLAGS: &str = "trace_flags";
    /// The application run identifier.
    pub const RUN_ID: &str = "run_id";
    /// The request identifier, entropy-only.
    pub const REQUEST_ID: &str = "request_id";
}
