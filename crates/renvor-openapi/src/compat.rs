//! Semantic API compatibility: what breaks a consumer, and what does not.
//!
//! # Classification is by effect, not by text
//!
//! A textual diff of two descriptions reports every reworded sentence as a change and every
//! reordered member as a change. A gate built on one is a gate that fires constantly, and a gate
//! that fires constantly gets weakened. So this compares **meaning**.
//!
//! # The asymmetry IS the classification
//!
//! Requests are contravariant and responses are covariant, which is why these sit on opposite
//! sides of one rule:
//!
//! | Change | Verdict | Why |
//! |---|---|---|
//! | a request constraint **narrows** | **breaking** | input a consumer previously sent successfully is now refused |
//! | a request constraint **widens** | compatible | input a consumer never sent is now accepted |
//! | a **required** request input is added | **breaking** | every existing request lacks it |
//! | an **optional** request input is added | compatible | existing requests are unaffected |
//! | a response member is **removed** | **breaking** | a consumer that read it now finds nothing |
//! | a response member is **added** | compatible | a consumer that ignores it is unaffected |
//!
//! # Comparing values, not typed documents
//!
//! Both sides are compared as `serde_json::Value`. A committed snapshot is a file that may have
//! been written by an older build, and requiring it to deserialise into the *current* model would
//! make the gate fail on exactly the changes it exists to classify.
//!
//! # This gate reads the snapshot from committed history
//!
//! Regenerating both sides would make every comparison trivially pass. `renvor-openapi` supplies
//! the comparison; the caller supplies a baseline it did not just produce. FR-048 is asserted in
//! `tests/compatibility.rs` by attempting the bypass and requiring it to fail.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// Whether a change can break an existing consumer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// An existing consumer keeps working.
    Compatible,
    /// An existing consumer can stop working.
    Breaking,
}

/// What kind of change this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ChangeClass {
    /// An operation the baseline declared is gone.
    OperationRemoved,
    /// An operation's identifier changed.
    OperationIdChanged,
    /// A parameter the baseline declared is gone.
    ParameterRemoved,
    /// A parameter that was optional is now required.
    ParameterBecameRequired,
    /// A new required parameter appeared.
    RequiredParameterAdded,
    /// A request body that was optional is now required.
    RequestBodyBecameRequired,
    /// A schema's declared type changed.
    TypeChanged,
    /// A constraint tightened.
    ConstraintNarrowed,
    /// A declared response status is gone.
    ResponseRemoved,
    /// A public error code the baseline declared is gone.
    ErrorCodeRemoved,
    /// A response member the baseline guaranteed is gone.
    ResponseMemberRemoved,
    /// A declared media type is gone.
    ContentTypeRemoved,
    /// A new operation appeared.
    OperationAdded,
    /// A new optional parameter appeared.
    OptionalParameterAdded,
    /// A constraint loosened.
    ConstraintWidened,
    /// A new response status appeared.
    ResponseAdded,
    /// A new public error code appeared.
    ErrorCodeAdded,
}

impl ChangeClass {
    /// Whether this class of change can break an existing consumer.
    #[must_use]
    pub const fn severity(self) -> Severity {
        match self {
            Self::OperationRemoved
            | Self::OperationIdChanged
            | Self::ParameterRemoved
            | Self::ParameterBecameRequired
            | Self::RequiredParameterAdded
            | Self::RequestBodyBecameRequired
            | Self::TypeChanged
            | Self::ConstraintNarrowed
            | Self::ResponseRemoved
            | Self::ErrorCodeRemoved
            | Self::ResponseMemberRemoved
            | Self::ContentTypeRemoved => Severity::Breaking,
            Self::OperationAdded
            | Self::OptionalParameterAdded
            | Self::ConstraintWidened
            | Self::ResponseAdded
            | Self::ErrorCodeAdded => Severity::Compatible,
        }
    }

    /// The stable name of this class.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OperationRemoved => "operation_removed",
            Self::OperationIdChanged => "operation_id_changed",
            Self::ParameterRemoved => "parameter_removed",
            Self::ParameterBecameRequired => "parameter_became_required",
            Self::RequiredParameterAdded => "required_parameter_added",
            Self::RequestBodyBecameRequired => "request_body_became_required",
            Self::TypeChanged => "type_changed",
            Self::ConstraintNarrowed => "constraint_narrowed",
            Self::ResponseRemoved => "response_removed",
            Self::ErrorCodeRemoved => "error_code_removed",
            Self::ResponseMemberRemoved => "response_member_removed",
            Self::ContentTypeRemoved => "content_type_removed",
            Self::OperationAdded => "operation_added",
            Self::OptionalParameterAdded => "optional_parameter_added",
            Self::ConstraintWidened => "constraint_widened",
            Self::ResponseAdded => "response_added",
            Self::ErrorCodeAdded => "error_code_added",
        }
    }
}

/// One classified difference.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Change {
    /// What kind of change it is.
    pub class: ChangeClass,
    /// Where it is, as a human-readable location.
    pub at: String,
}

impl Change {
    /// Whether this change can break an existing consumer.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.class.severity()
    }
}

impl core::fmt::Display for Change {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} at {}", self.class.as_str(), self.at)
    }
}

/// Compares a candidate description against a baseline.
///
/// Returns every classified difference, sorted, so two runs over the same pair produce the same
/// report.
///
/// # A change absent from this list is not a change
///
/// Descriptions, summaries, examples, tags, and server entries are **not** compared. They cannot
/// break a consumer, and reporting them would train a reviewer to skim the gate's output.
#[must_use]
pub fn compare(baseline: &Value, candidate: &Value) -> Vec<Change> {
    let mut changes = Vec::new();

    let before = operations(baseline);
    let after = operations(candidate);

    for (key, old) in &before {
        match after.get(key) {
            None => changes.push(Change {
                class: ChangeClass::OperationRemoved,
                at: key.clone(),
            }),
            Some(new) => compare_operation(key, old, new, &mut changes),
        }
    }
    for key in after.keys() {
        if !before.contains_key(key) {
            changes.push(Change {
                class: ChangeClass::OperationAdded,
                at: key.clone(),
            });
        }
    }

    compare_error_codes(baseline, candidate, &mut changes);

    changes.sort();
    changes.dedup();
    changes
}

/// Whether any change in `changes` can break an existing consumer.
#[must_use]
pub fn is_breaking(changes: &[Change]) -> bool {
    changes
        .iter()
        .any(|change| change.severity() == Severity::Breaking)
}

/// Only the breaking changes.
#[must_use]
pub fn breaking(changes: &[Change]) -> Vec<&Change> {
    changes
        .iter()
        .filter(|change| change.severity() == Severity::Breaking)
        .collect()
}

const METHODS: [&str; 6] = ["get", "put", "post", "delete", "patch", "head"];

/// Every operation, keyed by `METHOD path`.
fn operations(document: &Value) -> BTreeMap<String, &Value> {
    let mut found = BTreeMap::new();
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return found;
    };
    for (path, item) in paths {
        for method in METHODS {
            if let Some(operation) = item.get(method) {
                found.insert(format!("{} {path}", method.to_uppercase()), operation);
            }
        }
    }
    found
}

fn compare_operation(key: &str, old: &Value, new: &Value, changes: &mut Vec<Change>) {
    let id = |operation: &Value| {
        operation
            .get("operationId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    if id(old) != id(new) {
        changes.push(Change {
            class: ChangeClass::OperationIdChanged,
            at: key.to_owned(),
        });
    }

    compare_parameters(key, old, new, changes);
    compare_request_body(key, old, new, changes);
    compare_responses(key, old, new, changes);
}

fn parameters(operation: &Value) -> BTreeMap<(String, String), &Value> {
    operation
        .get("parameters")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|parameter| {
                    let location = parameter.get("in")?.as_str()?.to_owned();
                    let name = parameter.get("name")?.as_str()?.to_owned();
                    Some(((location, name), parameter))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn is_required(value: &Value) -> bool {
    value.get("required").and_then(Value::as_bool) == Some(true)
}

fn compare_parameters(key: &str, old: &Value, new: &Value, changes: &mut Vec<Change>) {
    let before = parameters(old);
    let after = parameters(new);

    for (identity, previous) in &before {
        let at = format!("{key} parameter {}:{}", identity.0, identity.1);
        match after.get(identity) {
            None => changes.push(Change {
                class: ChangeClass::ParameterRemoved,
                at,
            }),
            Some(current) => {
                if !is_required(previous) && is_required(current) {
                    changes.push(Change {
                        class: ChangeClass::ParameterBecameRequired,
                        at: at.clone(),
                    });
                }
                compare_schema(
                    &at,
                    previous.get("schema"),
                    current.get("schema"),
                    Variance::Request,
                    changes,
                );
            }
        }
    }

    for (identity, current) in &after {
        if before.contains_key(identity) {
            continue;
        }
        let at = format!("{key} parameter {}:{}", identity.0, identity.1);
        changes.push(Change {
            class: if is_required(current) {
                ChangeClass::RequiredParameterAdded
            } else {
                ChangeClass::OptionalParameterAdded
            },
            at,
        });
    }
}

fn compare_request_body(key: &str, old: &Value, new: &Value, changes: &mut Vec<Change>) {
    let previous = old.get("requestBody");
    let current = new.get("requestBody");

    match (previous, current) {
        (None, Some(body)) if is_required(body) => changes.push(Change {
            class: ChangeClass::RequiredParameterAdded,
            at: format!("{key} requestBody"),
        }),
        (Some(before), Some(after)) => {
            if !is_required(before) && is_required(after) {
                changes.push(Change {
                    class: ChangeClass::RequestBodyBecameRequired,
                    at: format!("{key} requestBody"),
                });
            }
            compare_content(
                &format!("{key} requestBody"),
                before.get("content"),
                after.get("content"),
                Variance::Request,
                changes,
            );
        }
        _ => {}
    }
}

fn compare_responses(key: &str, old: &Value, new: &Value, changes: &mut Vec<Change>) {
    let before = responses_of(old);
    let after = responses_of(new);

    for (status, previous) in &before {
        let at = format!("{key} response {status}");
        match after.get(status) {
            None => changes.push(Change {
                class: ChangeClass::ResponseRemoved,
                at,
            }),
            Some(current) => compare_content(
                &at,
                previous.get("content"),
                current.get("content"),
                Variance::Response,
                changes,
            ),
        }
    }
    for status in after.keys() {
        if !before.contains_key(status) {
            changes.push(Change {
                class: ChangeClass::ResponseAdded,
                at: format!("{key} response {status}"),
            });
        }
    }
}

fn compare_content(
    at: &str,
    old: Option<&Value>,
    new: Option<&Value>,
    variance: Variance,
    changes: &mut Vec<Change>,
) {
    let media_types = |content: Option<&Value>| -> BTreeSet<String> {
        content
            .and_then(Value::as_object)
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    };
    let before = media_types(old);
    let after = media_types(new);

    for media_type in before.difference(&after) {
        changes.push(Change {
            class: ChangeClass::ContentTypeRemoved,
            at: format!("{at} content {media_type}"),
        });
    }
    for media_type in before.intersection(&after) {
        compare_schema(
            &format!("{at} content {media_type}"),
            old.and_then(|content| content.get(media_type)?.get("schema")),
            new.and_then(|content| content.get(media_type)?.get("schema")),
            variance,
            changes,
        );
    }
}

/// Which direction a change has to travel to be safe.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Variance {
    /// Requests: widening is safe, narrowing breaks.
    Request,
    /// Responses: adding is safe, removing breaks.
    Response,
}

fn compare_schema(
    at: &str,
    old: Option<&Value>,
    new: Option<&Value>,
    variance: Variance,
    changes: &mut Vec<Change>,
) {
    let (Some(before), Some(after)) = (old, new) else {
        return;
    };
    if before == after {
        return;
    }

    if before.get("type") != after.get("type") {
        changes.push(Change {
            class: ChangeClass::TypeChanged,
            at: at.to_owned(),
        });
    }

    // A LOWER bound rising narrows; falling widens. An UPPER bound falling narrows; rising widens.
    for (keyword, tightens_upward) in [
        ("minLength", true),
        ("minimum", true),
        ("minItems", true),
        ("maxLength", false),
        ("maximum", false),
        ("maxItems", false),
    ] {
        let read = |value: &Value| value.get(keyword).and_then(Value::as_f64);
        if let (Some(previous), Some(current)) = (read(before), read(after))
            && (previous - current).abs() > f64::EPSILON
        {
            let narrowed = if tightens_upward {
                current > previous
            } else {
                current < previous
            };
            changes.push(Change {
                class: if narrowed {
                    ChangeClass::ConstraintNarrowed
                } else {
                    ChangeClass::ConstraintWidened
                },
                at: format!("{at} {keyword}"),
            });
        } else if read(before).is_none() && read(after).is_some() {
            // A bound that did not exist and now does can only be a narrowing.
            changes.push(Change {
                class: ChangeClass::ConstraintNarrowed,
                at: format!("{at} {keyword}"),
            });
        } else if read(before).is_some() && read(after).is_none() {
            changes.push(Change {
                class: ChangeClass::ConstraintWidened,
                at: format!("{at} {keyword}"),
            });
        }
    }

    // An enum losing a member narrows; gaining one widens.
    let members = |value: &Value| -> Option<BTreeSet<String>> {
        Some(
            value
                .get("enum")?
                .as_array()?
                .iter()
                .map(ToString::to_string)
                .collect(),
        )
    };
    if let (Some(previous), Some(current)) = (members(before), members(after)) {
        if previous.difference(&current).count() > 0 {
            changes.push(Change {
                class: ChangeClass::ConstraintNarrowed,
                at: format!("{at} enum"),
            });
        }
        if current.difference(&previous).count() > 0 {
            changes.push(Change {
                class: ChangeClass::ConstraintWidened,
                at: format!("{at} enum"),
            });
        }
    }

    compare_required(at, before, after, variance, changes);
    compare_properties(at, before, after, variance, changes);
}

fn compare_required(
    at: &str,
    before: &Value,
    after: &Value,
    variance: Variance,
    changes: &mut Vec<Change>,
) {
    let required = |value: &Value| -> BTreeSet<String> {
        value
            .get("required")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|name| name.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let previous = required(before);
    let current = required(after);

    match variance {
        // A REQUEST gaining a required member refuses every existing request.
        Variance::Request => {
            for name in current.difference(&previous) {
                changes.push(Change {
                    class: ChangeClass::RequiredParameterAdded,
                    at: format!("{at} required {name}"),
                });
            }
        }
        // A RESPONSE losing a guaranteed member breaks a consumer that read it.
        Variance::Response => {
            for name in previous.difference(&current) {
                changes.push(Change {
                    class: ChangeClass::ResponseMemberRemoved,
                    at: format!("{at} required {name}"),
                });
            }
        }
    }
}

fn compare_properties(
    at: &str,
    before: &Value,
    after: &Value,
    variance: Variance,
    changes: &mut Vec<Change>,
) {
    let previous = properties_of(before);
    let current = properties_of(after);

    for (name, old) in &previous {
        match current.get(name) {
            Some(new) => compare_schema(
                &format!("{at}/{name}"),
                Some(old),
                Some(new),
                variance,
                changes,
            ),
            None if variance == Variance::Response => changes.push(Change {
                class: ChangeClass::ResponseMemberRemoved,
                at: format!("{at}/{name}"),
            }),
            None => {}
        }
    }
}

/// Compares the enumerated public error codes in the shared Problem Details component.
///
/// Removing a code a consumer handled is a break; adding one is not — the same rule the public API
/// error registry states, applied to the document that publishes it.
fn compare_error_codes(baseline: &Value, candidate: &Value, changes: &mut Vec<Change>) {
    let codes = |document: &Value| -> BTreeSet<String> {
        document
            .pointer("/components/schemas/ProblemDetails/properties/code/enum")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let before = codes(baseline);
    let after = codes(candidate);

    for code in before.difference(&after) {
        changes.push(Change {
            class: ChangeClass::ErrorCodeRemoved,
            at: format!("error code {code}"),
        });
    }
    for code in after.difference(&before) {
        changes.push(Change {
            class: ChangeClass::ErrorCodeAdded,
            at: format!("error code {code}"),
        });
    }
}

/// The declared responses, keyed by status.
///
/// A named function rather than a closure: the returned map borrows from `operation`, and a
/// closure cannot state that relation — it infers two independent lifetimes and then cannot prove
/// one outlives the other.
fn responses_of(operation: &Value) -> BTreeMap<String, &Value> {
    operation
        .get("responses")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(status, response)| (status.clone(), response))
                .collect()
        })
        .unwrap_or_default()
}

/// The declared properties of a schema. Named for the same reason as [`responses_of`].
fn properties_of(schema: &Value) -> BTreeMap<String, &Value> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(name, value)| (name.clone(), value))
                .collect()
        })
        .unwrap_or_default()
}
