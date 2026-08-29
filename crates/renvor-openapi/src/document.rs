//! The OpenAPI 3.2.0 document model.
//!
//! # The version is a constant, not a field
//!
//! [`OPENAPI_VERSION`] is `"3.2.0"` and there is **no way to set it**. That is deliberate and it is
//! the constitution's requirement made structural:
//!
//! > *"emitted documents MUST NOT claim a version that selected tooling does not correctly
//! > implement."* — principle V
//!
//! A model with a settable version field is a model that can relabel. This one cannot, so the
//! remaining question is only whether the document's *content* is genuinely 3.2 — and
//! `tests/openapi_3_2_gate.rs` answers that against the official schemas rather than by assertion.
//!
//! # Determinism comes from the shape, not from a flag
//!
//! Members are emitted in **declaration order**, which `serde`'s derive fixes at compile time.
//! Open-ended maps are [`BTreeMap`], which serialises in **sorted key order**. Nothing here reads a
//! clock, a hash seed, an environment variable, or an address.
//!
//! `serde_json`'s `preserve_order` feature is deliberately **not** enabled: it would make output
//! depend on insertion order, which is exactly the non-determinism this avoids.
//!
//! # The 3.2-only members are load-bearing
//!
//! [`Response::summary`] and [`Tag::kind`] exist in OpenAPI 3.2 and **not** in 3.1. They are not
//! decoration: they are what makes the emitted document structurally invalid against the 3.1
//! schema, which is how the gate proves the output is not relabelled 3.1.

use std::collections::BTreeMap;

use renvor_error::ApiErrorCode;
use serde::Serialize;

/// The OpenAPI version this crate emits.
///
/// **Not settable.** See the module documentation.
pub const OPENAPI_VERSION: &str = "3.2.0";

/// The JSON Schema dialect OpenAPI 3.2 declares by default.
///
/// Emitted explicitly rather than relied on as a default, so a reader of the document knows which
/// dialect its schemas are written in without having to know the specification's defaults.
///
/// # The date here is NOT the schema's date, and that is not a mistake
///
/// The vendored **schema** artifacts are `2025-11-23`. The **dialect** and **meta** artifacts are
/// still `2025-09-17` — the OpenAPI Initiative revised one and not the others. Both resolve; they
/// simply carry different dates.
///
/// Written down because the mismatch looks like an oversight and correcting it to `2025-11-23`
/// would emit a dialect URI that does not exist. There is also no `/latest` alias to fall back on:
/// `https://spec.openapis.org/oas/3.2/schema/latest` returns **404**.
pub const OPENAPI_DIALECT: &str = "https://spec.openapis.org/oas/3.2/dialect/2025-09-17";

/// The media type for request and response payloads.
pub const JSON_MEDIA_TYPE: &str = "application/json";

/// The component name under which the shared Problem Details schema is registered.
///
/// **One** component, referenced by every failure response. A schema repeated inline per response
/// is a second copy that can drift, which is the defect this whole phase is shaped against.
pub const PROBLEM_COMPONENT: &str = "ProblemDetails";

/// Document metadata.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Info {
    /// The API's title.
    pub title: String,
    /// The API's version. **This is the API's version, not OpenAPI's.**
    pub version: String,
    /// A description of the API.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A server the API is served from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Server {
    /// The server's URL.
    pub url: String,
    /// What this server is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A tag grouping operations.
///
/// Carries **3.2-only** members. See the module documentation for why that matters.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Tag {
    /// The tag's name.
    pub name: String,
    /// A short summary. **OpenAPI 3.2 only** — the 3.1 Tag Object has no `summary`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A longer description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// What kind of grouping this tag expresses. **OpenAPI 3.2 only.**
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// Where a parameter arrives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterLocation {
    /// A path parameter.
    Path,
    /// A query-string parameter.
    Query,
    /// A header.
    Header,
}

/// One declared parameter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Parameter {
    /// The parameter's name.
    pub name: String,
    /// Where it arrives.
    #[serde(rename = "in")]
    pub location: ParameterLocation,
    /// What it is for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether it must be present.
    ///
    /// Always emitted, never skipped: OpenAPI requires `required: true` for a path parameter, and
    /// a reader of a query parameter should not have to know that absence means optional.
    pub required: bool,
    /// The schema. **The same value the runtime enforces.**
    pub schema: serde_json::Value,
}

/// A request or response payload for one media type.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MediaType {
    /// The schema. **The same value the runtime enforces.**
    pub schema: serde_json::Value,
    /// A validated example.
    ///
    /// Every example is checked against its own schema at generation time. An example that
    /// contradicts its schema is worse than no example, because it is copied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
}

/// A declared request body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RequestBody {
    /// What the body is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether a body must be sent.
    pub required: bool,
    /// The payload, by media type.
    pub content: BTreeMap<String, MediaType>,
}

/// A declared response.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Response {
    /// A short summary. **OpenAPI 3.2 only** — the 3.1 Response Object has no `summary`.
    ///
    /// Emitted for every response, which is what guarantees that a generated document always
    /// carries at least one 3.2-only construct and is therefore structurally rejected by the 3.1
    /// schema.
    pub summary: String,
    /// A longer description.
    ///
    /// Optional here and **required** in 3.1 — another genuine 3.2 relaxation, and one the
    /// document exercises.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The payload, by media type.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub content: BTreeMap<String, MediaType>,
}

/// One operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Operation {
    /// The operation's stable identifier. **Unique within a document.**
    #[serde(rename = "operationId")]
    pub operation_id: String,
    /// A short summary.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// A longer description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The tags grouping this operation.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The declared parameters.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    /// The declared request body.
    #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    /// The declared responses, keyed by status. Sorted, because it is a [`BTreeMap`].
    pub responses: BTreeMap<String, Response>,
    /// The security requirements. **Any one** of them satisfies the operation.
    ///
    /// An empty vector is serialised as absent, which in OpenAPI means "inherit the document's
    /// top-level requirement" — not "no security". An operation that must be reachable without a
    /// credential declares an explicit empty [`SecurityRequirement`] instead.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub security: Vec<SecurityRequirement>,
}

/// One path and the operations it answers.
///
/// Members are named for their methods, in the fixed order below.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PathItem {
    /// A summary of the path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// `GET`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    /// `PUT`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    /// `POST`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    /// `DELETE`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    /// `PATCH`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    /// `HEAD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
}

impl PathItem {
    /// Places `operation` under `method`, reporting a collision rather than overwriting.
    ///
    /// # Errors
    ///
    /// [`DocumentError::DuplicateOperation`] if this method is already declared for this path. A
    /// silent overwrite would make the document describe whichever registration happened to be
    /// last, which is ordering nobody wrote down.
    pub fn set(&mut self, method: &str, operation: Operation) -> Result<(), DocumentError> {
        let slot = match method {
            "GET" => &mut self.get,
            "PUT" => &mut self.put,
            "POST" => &mut self.post,
            "DELETE" => &mut self.delete,
            "PATCH" => &mut self.patch,
            "HEAD" => &mut self.head,
            other => {
                return Err(DocumentError::UnsupportedMethod {
                    method: other.to_owned(),
                });
            }
        };
        if slot.is_some() {
            return Err(DocumentError::DuplicateOperation {
                method: method.to_owned(),
            });
        }
        *slot = Some(operation);
        Ok(())
    }

    /// Every operation this path declares, with its method.
    #[must_use]
    pub fn operations(&self) -> Vec<(&'static str, &Operation)> {
        [
            ("GET", self.get.as_ref()),
            ("PUT", self.put.as_ref()),
            ("POST", self.post.as_ref()),
            ("DELETE", self.delete.as_ref()),
            ("PATCH", self.patch.as_ref()),
            ("HEAD", self.head.as_ref()),
        ]
        .into_iter()
        .filter_map(|(method, operation)| operation.map(|operation| (method, operation)))
        .collect()
    }
}

/// Where an API key travels.
///
/// **A closed set, and deliberately smaller than OpenAPI's.** The specification also permits
/// `query`, and Renvor does not implement it: a credential in a query string is written to every
/// access log, proxy log, and browser history entry between the client and the server. A variant
/// here would be a way to declare something the transport must never do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyLocation {
    /// A cookie. Renvor's session credential.
    Cookie,
    /// A request header.
    Header,
}

/// One security scheme.
///
/// # The vocabulary is closed because FR-082 is a claim about what is *implemented*
///
/// *"OpenAPI declares the security schemes the transport actually implements, and no others."*
/// An open `type` field would let a document announce OAuth2 flows, OpenID discovery, or mutual
/// TLS that nothing serves — and a client that trusted the document would build against them.
///
/// Two variants, because Renvor's transport authenticates two ways: a session cookie, and a bearer
/// token behind `renvor-auth`'s `tokens` feature. `openIdConnect`, `oauth2` and `mutualTLS` are
/// absent because they are not implemented, not because they were forgotten.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum SecurityScheme {
    /// A credential carried in a named cookie or header.
    #[serde(rename = "apiKey")]
    ApiKey {
        /// The cookie or header name.
        name: String,
        /// Where it travels.
        #[serde(rename = "in")]
        location: ApiKeyLocation,
        /// What it is, for a human reading the document.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
    /// An `Authorization:` scheme, per RFC 9110.
    #[serde(rename = "http")]
    Http {
        /// The authentication scheme, lowercase — `bearer`.
        scheme: String,
        /// A hint at the bearer token's format.
        #[serde(rename = "bearerFormat", skip_serializing_if = "Option::is_none")]
        bearer_format: Option<String>,
        /// What it is, for a human reading the document.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

/// One security requirement: scheme names to the scopes each must grant.
///
/// A `Vec` of these on an operation means **any one of them** satisfies it; the entries *within*
/// one requirement must **all** hold. That asymmetry is OpenAPI's, and it is stated here because
/// getting it backwards produces a document that says the opposite of what is enforced.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct SecurityRequirement(pub BTreeMap<String, Vec<String>>);

impl SecurityRequirement {
    /// A requirement naming one scheme and the scopes it must grant.
    #[must_use]
    pub fn new(scheme: impl Into<String>, scopes: Vec<String>) -> Self {
        let mut map = BTreeMap::new();
        map.insert(scheme.into(), scopes);
        Self(map)
    }
}

/// Reusable components.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Components {
    /// Schemas, sorted by name.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub schemas: BTreeMap<String, serde_json::Value>,
    /// Security schemes, sorted by name.
    #[serde(rename = "securitySchemes", skip_serializing_if = "BTreeMap::is_empty")]
    pub security_schemes: BTreeMap<String, SecurityScheme>,
}

/// Why a document could not be built.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocumentError {
    /// Two operations claimed the same identifier.
    DuplicateOperationId {
        /// The identifier.
        operation_id: String,
    },
    /// The same method was declared twice for one path.
    DuplicateOperation {
        /// The method.
        method: String,
    },
    /// A method outside the set OpenAPI names as Path Item members.
    UnsupportedMethod {
        /// The method.
        method: String,
    },
    /// A declared example did not satisfy its own schema.
    ExampleInvalid {
        /// Where the example was declared.
        at: String,
    },
}

impl core::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateOperationId { operation_id } => write!(
                f,
                "`{operation_id}` is claimed by two operations. OpenAPI requires an operation \
                 identifier to be unique within a document, and a serialiser that wrote both would \
                 silently keep one — the same failure a duplicate route registration refuses"
            ),
            Self::DuplicateOperation { method } => write!(
                f,
                "`{method}` is already declared for this path; a second declaration is refused \
                 rather than replacing the first"
            ),
            Self::UnsupportedMethod { method } => write!(
                f,
                "`{method}` is not a method OpenAPI names as a Path Item member"
            ),
            Self::ExampleInvalid { at } => write!(
                f,
                "the example at `{at}` does not satisfy its own schema. An example that \
                 contradicts its schema is worse than no example, because it is copied"
            ),
        }
    }
}

impl core::error::Error for DocumentError {}

/// A complete OpenAPI 3.2.0 description.
///
/// Field order below **is** the emitted member order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Document {
    /// Always [`OPENAPI_VERSION`]. See the module documentation for why this is not settable.
    pub openapi: &'static str,

    /// **OpenAPI 3.2 only.** The document's own identity.
    ///
    /// The 3.1 root object has no `$self`, so a document carrying one is structurally invalid
    /// against the 3.1 schema.
    #[serde(rename = "$self", skip_serializing_if = "Option::is_none")]
    pub self_uri: Option<String>,

    /// Document metadata.
    pub info: Info,

    /// The dialect this document's schemas are written in.
    #[serde(rename = "jsonSchemaDialect")]
    pub json_schema_dialect: &'static str,

    /// The servers this API is reachable at.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub servers: Vec<Server>,

    /// The tags grouping operations.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,

    /// The paths, sorted. A [`BTreeMap`], so two runs emit them in the same order.
    pub paths: BTreeMap<String, PathItem>,

    /// Reusable components.
    #[serde(skip_serializing_if = "components_are_empty")]
    pub components: Components,
}

fn components_are_empty(components: &Components) -> bool {
    // BOTH maps. The single-field version omitted `components` entirely whenever `schemas` was
    // empty, which would have dropped a document's security schemes on the floor — silently, and
    // only for documents that declare security without declaring a schema.
    components.schemas.is_empty() && components.security_schemes.is_empty()
}

impl Document {
    /// An empty document with the fixed version and dialect.
    #[must_use]
    pub fn new(info: Info) -> Self {
        Self {
            openapi: OPENAPI_VERSION,
            self_uri: None,
            info,
            json_schema_dialect: OPENAPI_DIALECT,
            servers: Vec::new(),
            tags: Vec::new(),
            paths: BTreeMap::new(),
            components: Components::default(),
        }
    }

    /// Sets the document's own identity — the 3.2-only `$self`.
    #[must_use]
    pub fn with_self_uri(mut self, uri: impl Into<String>) -> Self {
        self.self_uri = Some(uri.into());
        self
    }

    /// Registers the shared Problem Details schema and its response components.
    ///
    /// Called once. Every failure response then references the same component, so a change to the
    /// failure shape is one change rather than one per operation.
    #[must_use]
    pub fn with_problem_component(mut self) -> Self {
        self.components
            .schemas
            .insert(PROBLEM_COMPONENT.to_owned(), problem_schema());
        self
    }

    /// Every declared operation identifier, in document order.
    #[must_use]
    pub fn operation_ids(&self) -> Vec<&str> {
        self.paths
            .values()
            .flat_map(PathItem::operations)
            .map(|(_, operation)| operation.operation_id.as_str())
            .collect()
    }

    /// Every declared (method, path) pair, sorted.
    #[must_use]
    pub fn method_paths(&self) -> Vec<(String, String)> {
        let mut pairs: Vec<(String, String)> = self
            .paths
            .iter()
            .flat_map(|(path, item)| {
                item.operations()
                    .into_iter()
                    .map(move |(method, _)| (method.to_owned(), path.clone()))
            })
            .collect();
        pairs.sort();
        pairs
    }

    /// Checks that no two operations share an identifier.
    ///
    /// # Errors
    ///
    /// [`DocumentError::DuplicateOperationId`] naming the collision.
    pub fn check_operation_ids(&self) -> Result<(), DocumentError> {
        let mut seen = std::collections::BTreeSet::new();
        for id in self.operation_ids() {
            if !seen.insert(id) {
                return Err(DocumentError::DuplicateOperationId {
                    operation_id: id.to_owned(),
                });
            }
        }
        Ok(())
    }

    /// Renders the document.
    ///
    /// Deterministic to the byte: two calls against the same document produce identical output,
    /// and so do two processes on two platforms.
    ///
    /// # Errors
    ///
    /// Propagates a serialisation failure rather than substituting an empty document.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Renders the document as a value, for validation without a round trip through text.
    #[must_use]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("an OpenAPI document is always representable as JSON")
    }
}

/// The schema for an RFC 9457 Problem Details document, with Renvor's declared extensions.
///
/// Built from the **same** [`ApiErrorCode`] registry the runtime raises, so the enumerated codes
/// cannot fall behind the ones an application can actually produce.
#[must_use]
pub fn problem_schema() -> serde_json::Value {
    let codes: Vec<serde_json::Value> = ApiErrorCode::ALL
        .iter()
        .map(|code| serde_json::Value::String(code.as_str().to_owned()))
        .collect();

    serde_json::json!({
        "type": "object",
        "title": "Problem Details",
        "description":
            "An RFC 9457 problem document. `code`, `correlationId`, and `invalidParams` are \
             documented extension members.",
        "required": ["type", "title", "status", "code", "correlationId"],
        "properties": {
            "type": {"type": "string", "format": "uri-reference"},
            "title": {"type": "string"},
            "status": {"type": "integer", "minimum": 100, "maximum": 599},
            "detail": {"type": "string"},
            "instance": {"type": "string", "format": "uri-reference"},
            "code": {"type": "string", "enum": codes},
            "correlationId": {"type": "string"},
            "invalidParams": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["location", "pointer", "reason"],
                    "properties": {
                        "location": {"type": "string", "enum": ["path", "query", "header", "body"]},
                        "pointer": {"type": "string"},
                        "reason": {"type": "string"}
                    }
                }
            }
        }
    })
}
