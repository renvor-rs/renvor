//! Rendering the OpenAPI 3.2.0 description — from the **same registry** the router is built from.
//!
//! # Why this takes a reference, exactly as inspection does
//!
//! [`document`] takes `&RouteRegistry`. There is no snapshot type, no builder, and no intermediate
//! manifest, so there is nothing that could be produced once, stored, and then disagree with the
//! router later.
//!
//! `inspect::render_json` already established this signature and its reasoning. This is the third
//! consumer of the same value, on the same terms:
//!
//! ```text
//!                        RouteRegistry
//!               ╱              │             ╲
//!       build::router   inspect::render   describe::document
//! ```
//!
//! A route cannot reach dispatch without reaching the description, and cannot reach the description
//! without reaching dispatch. That is FR-024 and SC-003 in both directions, satisfied structurally
//! rather than by a test that counts them — though `tests/describe.rs` counts them anyway, because
//! a structural argument that nobody checked is an argument.
//!
//! # Generation has no side effects
//!
//! Nothing here binds a socket, opens a connection, runs a migration, makes a network call, or
//! starts a provider. It reads a value and writes JSON. FR-031.

use std::collections::BTreeMap;

use renvor_error::{ApiErrorCode, Location};
use renvor_openapi::{
    Document, Info, JSON_MEDIA_TYPE, MediaType, Operation, PROBLEM_COMPONENT, Parameter,
    ParameterLocation, RequestBody, Response, Tag,
};

use super::{Method, Route, RouteRegistry};

/// The media type failures are served as.
pub const PROBLEM_MEDIA_TYPE: &str = renvor_error::PROBLEM_MEDIA_TYPE;

/// Why a description could not be produced.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DescribeError {
    /// Two operations claimed the same identifier.
    DuplicateOperationId {
        /// The identifier.
        operation_id: String,
    },
    /// A specification declared a parameter in the body location, which is not a parameter.
    BodyDeclaredAsParameter {
        /// The operation.
        operation_id: String,
    },
    /// A declared example did not satisfy its own schema.
    ExampleInvalid {
        /// Where.
        at: String,
    },
    /// A method that OpenAPI does not name as a Path Item member was registered.
    UnsupportedMethod {
        /// The method.
        method: String,
    },
}

impl core::fmt::Display for DescribeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateOperationId { operation_id } => write!(
                f,
                "`{operation_id}` is claimed by two operations. OpenAPI requires operation \
                 identifiers to be unique within a document, and writing both would silently keep \
                 one — the same failure a duplicate route registration refuses"
            ),
            Self::BodyDeclaredAsParameter { operation_id } => write!(
                f,
                "`{operation_id}` declares a parameter in the body location. A body is a request \
                 body, not a parameter; declaring one as a parameter would publish a constraint \
                 that nothing enforces"
            ),
            Self::ExampleInvalid { at } => write!(
                f,
                "the example at `{at}` does not satisfy its own schema. An example that \
                 contradicts its schema is worse than no example, because it is copied"
            ),
            Self::UnsupportedMethod { method } => write!(
                f,
                "`{method}` is not a method OpenAPI names as a Path Item member"
            ),
        }
    }
}

impl core::error::Error for DescribeError {}

/// Builds the OpenAPI 3.2.0 description of `registry`.
///
/// # Errors
///
/// - [`DescribeError::DuplicateOperationId`] for a collision, whether the identifiers were supplied
///   or derived.
/// - [`DescribeError::ExampleInvalid`] for an example that contradicts its schema.
/// - [`DescribeError::BodyDeclaredAsParameter`] for a specification mistake an author can fix.
///
/// # Routes without a specification still appear
///
/// A route registered with no [`super::OperationSpec`] is described as an operation with no
/// declared inputs and an unconstrained success response. It is **not** omitted: a missing route is
/// the failure FR-024 exists to prevent, and "the author declared no inputs" is not a reason to
/// describe a different API from the one being served.
pub fn document(registry: &RouteRegistry, info: Info) -> Result<Document, DescribeError> {
    let mut openapi = Document::new(info).with_problem_component();

    // Tags are collected from the routes, so a tag cannot exist in the description without an
    // operation that uses it.
    let mut tag_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for route in registry.routes() {
        let method = openapi_method(route.method())?;
        let operation = operation_for(route)?;
        tag_names.extend(operation.tags.iter().cloned());

        openapi
            .paths
            .entry(route.path().to_owned())
            .or_default()
            .set(method, operation)
            .map_err(|_| DescribeError::DuplicateOperationId {
                operation_id: format!("{method} {}", route.path()),
            })?;
    }

    openapi.tags = tag_names
        .into_iter()
        .map(|name| Tag {
            summary: Some(name.clone()),
            name,
            description: None,
            // OpenAPI 3.2 only. Emitted for every tag, so a generated document always carries a
            // 3.2-only construct even when it declares no `$self`.
            kind: Some("nav".to_owned()),
        })
        .collect();

    openapi.check_operation_ids().map_err(|error| match error {
        renvor_openapi::DocumentError::DuplicateOperationId { operation_id } => {
            DescribeError::DuplicateOperationId { operation_id }
        }
        other => DescribeError::ExampleInvalid {
            at: other.to_string(),
        },
    })?;

    renvor_openapi::validate_examples(&openapi).map_err(|error| match error {
        renvor_openapi::DocumentError::ExampleInvalid { at } => {
            DescribeError::ExampleInvalid { at }
        }
        other => DescribeError::ExampleInvalid {
            at: other.to_string(),
        },
    })?;

    Ok(openapi)
}

fn openapi_method(method: Method) -> Result<&'static str, DescribeError> {
    match method {
        Method::Get => Ok("GET"),
        Method::Post => Ok("POST"),
        Method::Put => Ok("PUT"),
        Method::Patch => Ok("PATCH"),
        Method::Delete => Ok("DELETE"),
        Method::Head => Ok("HEAD"),
        // Unreachable through the registry, which refuses `OPTIONS` at registration. Reported
        // rather than assumed away: an unreachable branch that panics is a defect waiting for the
        // day it becomes reachable.
        Method::Options => Err(DescribeError::UnsupportedMethod {
            method: method.as_str().to_owned(),
        }),
    }
}

fn operation_for(route: &Route) -> Result<Operation, DescribeError> {
    let method = route.method();
    let operation_id = route
        .spec()
        .and_then(|spec| spec.operation_id.clone())
        .unwrap_or_else(|| derive_operation_id(method, route.path()));

    let Some(spec) = route.spec() else {
        // A route with no specification: described honestly as an operation whose inputs are
        // undeclared, with the framework failures it can still produce.
        return Ok(Operation {
            operation_id,
            summary: None,
            description: None,
            tags: Vec::new(),
            parameters: Vec::new(),
            request_body: None,
            responses: framework_responses(&default_error_codes()),
        });
    };

    let mut parameters = Vec::new();
    for parameter in &spec.parameters {
        let location = match parameter.location {
            Location::Path => ParameterLocation::Path,
            Location::Query => ParameterLocation::Query,
            Location::Header => ParameterLocation::Header,
            // A body is not a parameter, and neither is any location added later: an unrecognised
            // location cannot be given an OpenAPI `in` value without inventing one, and inventing
            // one would publish a parameter the runtime does not check.
            _ => {
                return Err(DescribeError::BodyDeclaredAsParameter { operation_id });
            }
        };
        parameters.push(Parameter {
            name: parameter.name.clone(),
            location,
            description: parameter.description.clone(),
            // OpenAPI requires a path parameter to be required. A specification saying otherwise
            // is corrected here rather than emitting an invalid document.
            required: parameter.required || location == ParameterLocation::Path,
            // THE SAME VALUE the runtime enforces.
            schema: parameter.schema.schema().clone(),
        });
    }
    parameters.sort_by(|left, right| {
        left.location
            .cmp(&right.location)
            .then_with(|| left.name.cmp(&right.name))
    });

    let request_body = spec.body.as_ref().map(|body| RequestBody {
        description: None,
        required: body.required,
        content: BTreeMap::from([(
            JSON_MEDIA_TYPE.to_owned(),
            MediaType {
                schema: body.schema.schema().clone(),
                example: body.example.clone(),
            },
        )]),
    });

    let mut responses = framework_responses(&spec.all_error_codes());
    for declared in &spec.responses {
        responses.insert(
            declared.status.to_string(),
            Response {
                summary: declared.summary.clone(),
                description: None,
                content: declared
                    .schema
                    .as_ref()
                    .map_or_else(BTreeMap::new, |schema| {
                        BTreeMap::from([(
                            JSON_MEDIA_TYPE.to_owned(),
                            MediaType {
                                schema: schema.schema().clone(),
                                example: declared.example.clone(),
                            },
                        )])
                    }),
            },
        );
    }

    Ok(Operation {
        operation_id,
        summary: spec.summary.clone(),
        description: spec.description.clone(),
        tags: spec.tags.clone(),
        parameters,
        request_body,
        responses,
    })
}

fn default_error_codes() -> std::collections::BTreeSet<ApiErrorCode> {
    [
        ApiErrorCode::HostRejected,
        ApiErrorCode::PayloadTooLarge,
        ApiErrorCode::RequestTimeout,
        ApiErrorCode::Unavailable,
        ApiErrorCode::InternalError,
    ]
    .into_iter()
    .collect()
}

/// The failure responses every operation carries, one per status the codes map to.
///
/// Each references the **shared** Problem Details component rather than repeating its schema. A
/// repeated inline schema is a second copy that can drift.
fn framework_responses(
    codes: &std::collections::BTreeSet<ApiErrorCode>,
) -> BTreeMap<String, Response> {
    let mut by_status: BTreeMap<u16, Vec<ApiErrorCode>> = BTreeMap::new();
    for code in codes {
        by_status.entry(status_for(*code)).or_default().push(*code);
    }

    by_status
        .into_iter()
        .map(|(status, codes)| {
            let names: Vec<&str> = codes.iter().map(|code| code.as_str()).collect();
            (
                status.to_string(),
                Response {
                    // OpenAPI 3.2's `summary`. Names the codes this status can carry, so a reader
                    // can match a response to the registry without opening another document.
                    summary: format!("Problem: {}", names.join(", ")),
                    description: None,
                    content: BTreeMap::from([(
                        PROBLEM_MEDIA_TYPE.to_owned(),
                        MediaType {
                            schema: serde_json::json!({
                                "$ref": format!("#/components/schemas/{PROBLEM_COMPONENT}")
                            }),
                            example: None,
                        },
                    )]),
                },
            )
        })
        .collect()
}

/// The HTTP status a public error code is answered with.
///
/// **This mapping lives here, in the transport**, and not in `renvor-error`. A status code is
/// transport semantics, and putting one in the transport-independent registry would make the
/// registry unusable without HTTP.
#[must_use]
pub const fn status_for(code: ApiErrorCode) -> u16 {
    match code {
        ApiErrorCode::ValidationFailed
        | ApiErrorCode::MalformedBody
        | ApiErrorCode::MissingBody
        | ApiErrorCode::HostRejected
        | ApiErrorCode::OriginRejected => 400,
        ApiErrorCode::UnsupportedMediaType => 415,
        ApiErrorCode::NotFound => 404,
        ApiErrorCode::MethodNotAllowed => 405,
        ApiErrorCode::PayloadTooLarge => 413,
        ApiErrorCode::RequestTimeout => 408,
        ApiErrorCode::Unavailable => 503,
        ApiErrorCode::InternalError => 500,
        // `ApiErrorCode` is `#[non_exhaustive]`, so this arm is required by the language rather
        // than chosen. 500 is the safe answer for a code this build does not recognise — but it is
        // NOT a silent fallback, because `every_public_error_code_has_an_explicit_status` below
        // enumerates the registry and fails the moment a code reaches this arm.
        _ => 500,
    }
}

/// Derives a stable identifier from a method and path.
///
/// Deterministic: the same route always yields the same identifier, on every platform and in every
/// process. A derivation **can** collide — two paths differing only in a parameter's *name*
/// normalise to the same string — so [`document`] checks the final set regardless of whether
/// identifiers were supplied or derived.
#[must_use]
pub fn derive_operation_id(method: Method, path: &str) -> String {
    let mut out = method.as_str().to_ascii_lowercase();
    for segment in path.split('/').filter(|segment| !segment.is_empty()) {
        let cleaned: String = segment
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        let mut chars = cleaned.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.extend(chars);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{derive_operation_id, status_for};
    use crate::route::Method;
    use renvor_error::ApiErrorCode;

    #[test]
    fn derivation_is_deterministic_and_readable() {
        assert_eq!(derive_operation_id(Method::Get, "/items"), "getItems");
        assert_eq!(
            derive_operation_id(Method::Post, "/api/v1/items"),
            "postApiV1Items"
        );
        assert_eq!(
            derive_operation_id(Method::Get, "/items/{id}"),
            "getItemsId"
        );
        assert_eq!(derive_operation_id(Method::Get, "/"), "get");

        // Twice, because "deterministic" is the claim.
        assert_eq!(
            derive_operation_id(Method::Get, "/items/{id}"),
            derive_operation_id(Method::Get, "/items/{id}")
        );
    }

    #[test]
    fn derivation_can_collide_which_is_why_the_document_checks_the_final_set() {
        // Two DIFFERENT routes, one derived identifier. Stated as a test rather than a comment,
        // because it is the reason `check_operation_ids` exists.
        assert_eq!(
            derive_operation_id(Method::Get, "/items/{id}"),
            derive_operation_id(Method::Get, "/items/{i-d}")
        );
    }

    #[test]
    fn every_public_error_code_has_an_explicit_status() {
        // The mapping's wildcard arm answers 500 for anything unrecognised, which the language
        // requires because `ApiErrorCode` is `#[non_exhaustive]`. This is what stops that arm from
        // becoming a silent fallback: every code in the registry is listed with the status it is
        // MEANT to have, so a code added without updating the mapping fails here rather than
        // quietly becoming an internal error.
        let expected = [
            (ApiErrorCode::ValidationFailed, 400_u16),
            (ApiErrorCode::MalformedBody, 400),
            (ApiErrorCode::UnsupportedMediaType, 415),
            (ApiErrorCode::MissingBody, 400),
            (ApiErrorCode::NotFound, 404),
            (ApiErrorCode::MethodNotAllowed, 405),
            (ApiErrorCode::HostRejected, 400),
            (ApiErrorCode::OriginRejected, 400),
            (ApiErrorCode::PayloadTooLarge, 413),
            (ApiErrorCode::RequestTimeout, 408),
            (ApiErrorCode::Unavailable, 503),
            (ApiErrorCode::InternalError, 500),
        ];

        assert_eq!(
            expected.len(),
            ApiErrorCode::ALL.len(),
            "the registry gained a code with no declared status; add it above"
        );
        for (code, status) in expected {
            assert_eq!(
                status_for(code),
                status,
                "`{code}` maps to the wrong status"
            );
            assert!((100..=599).contains(&status));
        }
    }
}

/// The documented invocation an application binary answers with its OpenAPI description.
///
/// A long, prefixed name on purpose: it must not collide with a flag an application defines for
/// itself, and it must be obvious in a process list that Renvor asked for it. The same reasoning —
/// and the same shape — as the route-dump flag it sits beside.
pub const OPENAPI_DUMP_FLAG: &str = "--renvor-dump-openapi";

/// The version of the OpenAPI-dump payload shape.
///
/// # Versioned separately from the document it carries
///
/// `openapi` inside the payload is the **document's** version, fixed at `3.2.0`. This is the
/// **protocol's** version — the shape of the envelope around it — and the two change for entirely
/// different reasons. `renvor openapi` checks this before reading the payload and refuses a
/// version it does not understand **by name**, for the reason `contracts/http-routing.md` already
/// states about the route dump: parsing an unknown version on a best-effort basis is how a
/// description silently loses a field.
pub const OPENAPI_DUMP_PROTOCOL: u32 = 1;

/// Answers the OpenAPI-dump invocation, if that is what this process was asked for.
///
/// An application calls this at the top of `main` and exits when it returns `Some`:
///
/// ```
/// # use renvor_http::route::{RouteRegistry, describe};
/// # use renvor_openapi::Info;
/// # fn build_registry() -> RouteRegistry { RouteRegistry::new() }
/// let registry = build_registry();
/// let info = Info { title: "My API".into(), version: "1.0.0".into(), description: None };
///
/// if let Some(document) = describe::answer_openapi_request(std::env::args(), &registry, info) {
///     println!("{document}");
///     return;
/// }
/// # ()
/// ```
///
/// # It answers before anything starts
///
/// Called first, the description is rendered and the process exits — so asking an application what
/// its API looks like does not bind a port, run a migration, or open a connection. FR-055.
///
/// # A generation failure is reported, never smoothed over
///
/// A duplicate operation identifier or an example contradicting its schema produces the **failure**
/// envelope with a registered code. It does **not** produce an empty description and a success:
/// an empty description and "this application could not describe itself" are different facts, and
/// a consumer cannot tell them apart if both arrive as success.
///
/// Returns `None` when the invocation was not a dump request, so an ordinary run is unaffected.
#[must_use]
pub fn answer_openapi_request<I, S>(
    arguments: I,
    registry: &RouteRegistry,
    info: Info,
) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if !arguments
        .into_iter()
        .any(|argument| argument.as_ref() == OPENAPI_DUMP_FLAG)
    {
        return None;
    }

    // The failure envelopes are CONSTANTS, not formatted strings.
    //
    // A generation error can name a path, an operation identifier, or an example, and this string
    // reaches a terminal and a log. Interpolating the underlying error would put author data into
    // both. Two fixed classifications say which kind of failure occurred without saying what was
    // in it — and being constants, they cannot be malformed JSON, which a nested `format!` of
    // braces very easily can.
    const DESCRIBE_FAILED: &str = concat!(
        r#"{"schemaVersion":2,"status":"failure","command":"openapi","#,
        r#""error":{"code":"internal","#,
        r#""message":"the application's declarations could not be described","#,
        r#""details":{}}}"#
    );
    const SERIALISE_FAILED: &str = concat!(
        r#"{"schemaVersion":2,"status":"failure","command":"openapi","#,
        r#""error":{"code":"internal","#,
        r#""message":"the description could not be serialised","#,
        r#""details":{}}}"#
    );

    Some(match document(registry, info) {
        // The document is embedded as ALREADY-SERIALISED text rather than re-encoded through a
        // `Value`. A `serde_json::Value` sorts object members, which would reorder the document
        // and make the relayed bytes differ from `Document::to_json`'s — two spellings of the same
        // description, which is exactly the drift this phase refuses.
        Ok(openapi) => match serde_json::to_string(&openapi) {
            Ok(rendered) => format!(
                r#"{{"schemaVersion":2,"status":"success","command":"openapi","result":{{"protocol":{OPENAPI_DUMP_PROTOCOL},"document":{rendered}}}}}"#
            ),
            Err(_) => SERIALISE_FAILED.to_owned(),
        },
        Err(_) => DESCRIBE_FAILED.to_owned(),
    })
}

#[cfg(test)]
mod protocol_tests {
    use super::{OPENAPI_DUMP_FLAG, OPENAPI_DUMP_PROTOCOL, answer_openapi_request};
    use crate::route::{OperationSpec, Request, Response, RouteRegistry};
    use renvor_openapi::Info;

    async fn ok(_: Request) -> Response {
        Response::json("{}")
    }

    fn info() -> Info {
        Info {
            title: "T".to_owned(),
            version: "1.0.0".to_owned(),
            description: None,
        }
    }

    #[test]
    fn an_ordinary_run_is_unaffected() {
        let registry = RouteRegistry::new();
        assert!(
            answer_openapi_request(["myapp", "serve"], &registry, info()).is_none(),
            "an ordinary invocation was answered as a dump request"
        );
    }

    #[test]
    fn the_success_envelope_is_well_formed_and_carries_the_document() {
        let mut registry = RouteRegistry::new();
        registry.get("/items", ok).expect("a valid route");

        let answer = answer_openapi_request(["myapp", OPENAPI_DUMP_FLAG], &registry, info())
            .expect("the dump flag is answered");

        let value: serde_json::Value =
            serde_json::from_str(&answer).unwrap_or_else(|e| panic!("not JSON ({e}): {answer}"));

        assert_eq!(value["status"], serde_json::json!("success"));
        assert_eq!(value["command"], serde_json::json!("openapi"));
        assert_eq!(value["schemaVersion"], serde_json::json!(2));
        assert_eq!(
            value["result"]["protocol"],
            serde_json::json!(OPENAPI_DUMP_PROTOCOL)
        );
        assert_eq!(
            value["result"]["document"]["openapi"],
            serde_json::json!("3.2.0")
        );
    }

    #[test]
    fn a_generation_failure_is_a_well_formed_failure_envelope_not_an_empty_success() {
        // Two operations claiming one identifier. The envelope must say FAILURE — an empty
        // description and "this application could not describe itself" are different facts.
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

        let answer = answer_openapi_request(["myapp", OPENAPI_DUMP_FLAG], &registry, info())
            .expect("the dump flag is answered");

        let value: serde_json::Value =
            serde_json::from_str(&answer).unwrap_or_else(|e| panic!("not JSON ({e}): {answer}"));

        assert_eq!(value["status"], serde_json::json!("failure"));
        assert!(value.get("result").is_none(), "a failure carried a result");
        assert_eq!(value["error"]["code"], serde_json::json!("internal"));

        // And it names NO operation identifier, so author data did not reach the envelope.
        assert!(
            !answer.contains("same"),
            "the envelope named the operation: {answer}"
        );
    }

    #[test]
    fn the_relayed_document_is_byte_identical_to_the_libraries_own_rendering() {
        // If these differed, the same description would have two spellings — one served by the
        // library and one relayed by the command — and a snapshot taken through either would fail
        // against the other.
        let mut registry = RouteRegistry::new();
        registry.get("/items", ok).expect("a valid route");

        let answer = answer_openapi_request(["myapp", OPENAPI_DUMP_FLAG], &registry, info())
            .expect("answered");
        let direct = serde_json::to_string(&super::document(&registry, info()).expect("generates"))
            .expect("serialises");

        assert!(
            answer.contains(&direct),
            "the relayed document is not the library's own rendering"
        );
    }
}
