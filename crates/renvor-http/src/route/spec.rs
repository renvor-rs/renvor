//! What an operation declares — the value both enforcement and description read.
//!
//! # This is the third consumer, not a second registry
//!
//! Contract C-9 prohibits a second route manifest and makes the prohibition structural: routing and
//! inspection both take `&RouteRegistry`, so there is no other source they could read. An
//! [`OperationSpec`] lives **on a route inside that registry**, which extends the same structure
//! rather than escaping it:
//!
//! ```text
//!                        RouteRegistry
//!                       (one value)
//!             ╱               │              ╲
//!     build::router    inspect::render    describe::document
//!      dispatch          route table        OpenAPI 3.2.0
//!            │                                    │
//!            └────── the SAME Declaration ────────┘
//! ```
//!
//! A [`Declaration`] holds one schema. [`OperationSpec::validate`] enforces it; `describe` embeds
//! it. "Runtime validation and published schemas agree" is an identity here, not a test result.
//!
//! # Coercion, stated rather than assumed
//!
//! Path, query, and header values arrive as **text**. A schema declaring `integer` therefore
//! cannot be applied to the raw string without a conversion, and the conversion is driven by the
//! **declared type** — never guessed from the value's shape.
//!
//! Guessing would make `"01"` an integer in one request and a string in another depending on
//! nothing the caller controls. A value the declared type cannot accept is left as text and fails
//! the type check, which reports the mistake the caller actually made.

use std::collections::{BTreeMap, BTreeSet};

use renvor_error::{ApiErrorCode, Location};
use renvor_validation::{Declaration, Issue};
use serde_json::Value;

/// One declared parameter.
#[derive(Clone, Debug)]
pub struct ParameterSpec {
    /// Where it arrives.
    pub location: Location,
    /// Its name.
    pub name: String,
    /// Whether it must be present.
    pub required: bool,
    /// What it must be. **The same value the description publishes.**
    pub schema: Declaration,
    /// What it is for.
    pub description: Option<String>,
}

/// A declared request body.
#[derive(Clone, Debug)]
pub struct BodySpec {
    /// Whether a body must be sent.
    pub required: bool,
    /// What it must be.
    pub schema: Declaration,
    /// A validated example.
    pub example: Option<Value>,
}

/// A declared response.
#[derive(Clone, Debug)]
pub struct ResponseSpec {
    /// The status.
    pub status: u16,
    /// A short summary. Emitted as OpenAPI 3.2's `summary` member.
    pub summary: String,
    /// What the payload is, when there is one.
    pub schema: Option<Declaration>,
    /// A validated example.
    pub example: Option<Value>,
}

/// Everything one operation declares.
#[derive(Clone, Debug, Default)]
pub struct OperationSpec {
    /// The stable identifier. Derived from method and path when absent.
    pub operation_id: Option<String>,
    /// A short summary.
    pub summary: Option<String>,
    /// A longer description.
    pub description: Option<String>,
    /// The tags grouping this operation.
    pub tags: Vec<String>,
    /// The declared parameters.
    pub parameters: Vec<ParameterSpec>,
    /// The declared request body.
    pub body: Option<BodySpec>,
    /// The declared success responses.
    pub responses: Vec<ResponseSpec>,
    /// The public error codes this operation can produce **beyond** the framework's own.
    pub errors: BTreeSet<ApiErrorCode>,
}

impl OperationSpec {
    /// An empty specification.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the stable identifier.
    #[must_use]
    pub fn id(mut self, operation_id: impl Into<String>) -> Self {
        self.operation_id = Some(operation_id.into());
        self
    }

    /// Sets the summary.
    #[must_use]
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Adds a tag.
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Declares a parameter.
    #[must_use]
    pub fn parameter(
        mut self,
        location: Location,
        name: impl Into<String>,
        required: bool,
        schema: Declaration,
    ) -> Self {
        self.parameters.push(ParameterSpec {
            location,
            name: name.into(),
            required,
            schema,
            description: None,
        });
        self
    }

    /// Declares the request body.
    #[must_use]
    pub fn body(mut self, required: bool, schema: Declaration) -> Self {
        self.body = Some(BodySpec {
            required,
            schema,
            example: None,
        });
        self
    }

    /// Attaches an example to the declared request body.
    ///
    /// Validated against its own schema when the description is generated. An example that
    /// contradicts its schema is a reported error, not a warning.
    #[must_use]
    pub fn body_example(mut self, example: Value) -> Self {
        if let Some(body) = &mut self.body {
            body.example = Some(example);
        }
        self
    }

    /// Declares a response.
    #[must_use]
    pub fn response(
        mut self,
        status: u16,
        summary: impl Into<String>,
        schema: Option<Declaration>,
    ) -> Self {
        self.responses.push(ResponseSpec {
            status,
            summary: summary.into(),
            schema,
            example: None,
        });
        self
    }

    /// Attaches an example to the most recently declared response.
    #[must_use]
    pub fn response_example(mut self, example: Value) -> Self {
        if let Some(response) = self.responses.last_mut() {
            response.example = Some(example);
        }
        self
    }

    /// Declares an additional public error code this operation can produce.
    #[must_use]
    pub fn error(mut self, code: ApiErrorCode) -> Self {
        self.errors.insert(code);
        self
    }

    /// Validates one request against this specification, reporting **every** violation.
    ///
    /// `body` is the raw request bytes. It is parsed as JSON only when a body is declared — an
    /// operation that declares none does not pay for a parse, and a caller sending an unread body
    /// to such an operation is not refused for its contents.
    ///
    /// # The unreadable case is separate
    ///
    /// A body that is not JSON at all returns [`RequestRejection::MalformedBody`] rather than a
    /// list of issues. FR-008 requires that distinction: a document that never parsed has no
    /// fields to point at, and telling a caller to correct a field in it would be nonsense.
    pub fn validate(
        &self,
        path_params: &BTreeMap<String, String>,
        query: &str,
        headers: &BTreeMap<String, String>,
        body: &[u8],
    ) -> Result<(), RequestRejection> {
        let mut issues = Vec::new();
        let query_params = parse_query(query);

        for parameter in &self.parameters {
            let raw = match parameter.location {
                Location::Path => path_params.get(&parameter.name),
                Location::Query => query_params.get(&parameter.name),
                Location::Header => headers.get(&parameter.name.to_ascii_lowercase()),
                // A body is not a parameter, and neither is any location added later. A
                // specification declaring one is an author mistake, and it cannot be reported to a
                // caller, who did nothing wrong. It is skipped here and REFUSED by `describe`,
                // where the author sees it — so it is not silently tolerated, only reported to the
                // right audience.
                _ => continue,
            };

            match raw {
                None if parameter.required => issues.push(Issue {
                    location: parameter.location,
                    pointer: pointer(&parameter.name),
                    reason: renvor_validation::Reason::RequiredMissing,
                }),
                None => {}
                Some(text) => {
                    let value = coerce(parameter.schema.schema(), text);
                    for mut issue in parameter.schema.validate(parameter.location, &value) {
                        // A parameter's pointer is its NAME, not a JSON Pointer into a document.
                        // Without this, every scalar parameter would report the empty pointer.
                        if issue.pointer.as_str().is_empty() {
                            issue.pointer = pointer(&parameter.name);
                        }
                        issues.push(issue);
                    }
                }
            }
        }

        if let Some(declared) = &self.body {
            if body.is_empty() {
                if declared.required {
                    return Err(RequestRejection::MissingBody);
                }
            } else {
                let parsed: Value = match serde_json::from_slice(body) {
                    Ok(value) => value,
                    // The parse error is NOT propagated: `serde_json`'s message quotes the input.
                    Err(_) => return Err(RequestRejection::MalformedBody),
                };
                issues.extend(declared.schema.validate(Location::Body, &parsed));
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            issues.sort_by(|left, right| {
                left.location
                    .as_str()
                    .cmp(right.location.as_str())
                    .then_with(|| left.pointer.as_str().cmp(right.pointer.as_str()))
                    .then_with(|| left.reason.as_str().cmp(right.reason.as_str()))
            });
            issues.dedup();
            Err(RequestRejection::Invalid(issues))
        }
    }

    /// Every public error code an operation with this specification can produce.
    ///
    /// The framework's own failures are included **unconditionally**: the transport can refuse a
    /// request before a handler runs, and a description that omitted those would be describing the
    /// handler rather than the API.
    #[must_use]
    pub fn all_error_codes(&self) -> BTreeSet<ApiErrorCode> {
        let mut codes: BTreeSet<ApiErrorCode> = self.errors.clone();
        codes.extend([
            ApiErrorCode::HostRejected,
            ApiErrorCode::PayloadTooLarge,
            ApiErrorCode::RequestTimeout,
            ApiErrorCode::Unavailable,
            ApiErrorCode::InternalError,
        ]);
        if !self.parameters.is_empty() || self.body.is_some() {
            codes.insert(ApiErrorCode::ValidationFailed);
        }
        if self.body.is_some() {
            codes.insert(ApiErrorCode::MalformedBody);
            codes.insert(ApiErrorCode::UnsupportedMediaType);
            codes.insert(ApiErrorCode::MissingBody);
        }
        codes
    }
}

/// Why a request was refused before it reached a handler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestRejection {
    /// One or more inputs violated a declared constraint.
    Invalid(Vec<Issue>),
    /// The body was not readable as the declared media type.
    MalformedBody,
    /// A required body was absent.
    MissingBody,
    /// The request's media type is not one the operation accepts.
    UnsupportedMediaType,
}

impl RequestRejection {
    /// The public error code for this rejection.
    #[must_use]
    pub const fn code(&self) -> ApiErrorCode {
        match self {
            Self::Invalid(_) => ApiErrorCode::ValidationFailed,
            Self::MalformedBody => ApiErrorCode::MalformedBody,
            Self::MissingBody => ApiErrorCode::MissingBody,
            Self::UnsupportedMediaType => ApiErrorCode::UnsupportedMediaType,
        }
    }

    /// The status this rejection is answered with.
    #[must_use]
    pub const fn status(&self) -> u16 {
        match self {
            Self::UnsupportedMediaType => 415,
            _ => 400,
        }
    }
}

fn pointer(name: &str) -> renvor_error::Pointer {
    renvor_error::Pointer::new(name)
        .unwrap_or_else(|_| renvor_error::Pointer::new("").expect("the empty pointer"))
}

/// Percent-decoded query pairs.
///
/// A duplicate key keeps the **first** value here and is separately refused by the collection
/// contract where duplicates are a declared concern. This function's job is reading, not policy.
fn parse_query(query: &str) -> BTreeMap<String, String> {
    let mut pairs = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        pairs
            .entry(percent_decode(key))
            .or_insert_with(|| percent_decode(value));
    }
    pairs
}

/// Decodes `%XX` escapes and `+` as a space.
///
/// Written here rather than taken from a package because the whole behaviour is twenty lines and
/// the alternative resolves a URL crate into the transport for one function. An invalid escape is
/// left **literal** rather than dropped: dropping bytes changes the value silently, and a caller
/// who sent a stray `%` should see a value that fails validation rather than one that quietly
/// became something else.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &text[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(b'%');
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Converts a text parameter to the JSON type its schema declares.
///
/// Driven by the **declared** type, never by the value's shape. See the module documentation.
fn coerce(schema: &Value, text: &str) -> Value {
    let declared = match schema.get("type") {
        Some(Value::String(name)) => name.as_str(),
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .find(|name| *name != "null")
            .unwrap_or("string"),
        _ => "string",
    };

    match declared {
        "integer" | "number" => text.parse::<f64>().map_or_else(
            // A value the declared type cannot accept stays TEXT, so the type check reports the
            // mistake the caller actually made rather than a substituted default.
            |_| Value::String(text.to_owned()),
            |number| {
                serde_json::Number::from_f64(number)
                    .map_or_else(|| Value::String(text.to_owned()), Value::Number)
            },
        ),
        "boolean" => match text {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            other => Value::String(other.to_owned()),
        },
        _ => Value::String(text.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::{coerce, parse_query, percent_decode};
    use serde_json::json;

    #[test]
    fn coercion_follows_the_declared_type_not_the_values_shape() {
        // `"01"` is a plausible integer AND a plausible string. The declared type decides.
        assert_eq!(coerce(&json!({"type": "integer"}), "01"), json!(1.0));
        assert_eq!(coerce(&json!({"type": "string"}), "01"), json!("01"));

        // A value the declared type cannot accept stays TEXT, so the type check reports it.
        assert_eq!(coerce(&json!({"type": "integer"}), "many"), json!("many"));
        assert_eq!(coerce(&json!({"type": "boolean"}), "yes"), json!("yes"));

        // Nullable types coerce to the non-null member.
        assert_eq!(
            coerce(&json!({"type": ["integer", "null"]}), "7"),
            json!(7.0)
        );
    }

    #[test]
    fn query_parsing_decodes_and_keeps_the_first_of_a_duplicate() {
        let pairs = parse_query("a=1&b=hello+world&a=2&c=%2Fpath&d");
        assert_eq!(pairs.get("a").map(String::as_str), Some("1"));
        assert_eq!(pairs.get("b").map(String::as_str), Some("hello world"));
        assert_eq!(pairs.get("c").map(String::as_str), Some("/path"));
        assert_eq!(pairs.get("d").map(String::as_str), Some(""));
    }

    #[test]
    fn an_invalid_escape_is_left_literal_rather_than_dropped() {
        // Dropping bytes changes a value silently. A caller who sent a stray `%` should see a
        // value that fails validation, not one that quietly became something else.
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("a%2"), "a%2");
    }
}
