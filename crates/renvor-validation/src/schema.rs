//! The declaration — one schema value, enforced at runtime and published in the description.
//!
//! # The single source, made structural
//!
//! A [`Declaration`] holds exactly one `serde_json::Value`. [`Declaration::validate`] interprets
//! it; [`Declaration::schema`] hands the **same value** to the OpenAPI serialiser. There is no
//! second representation, no compilation step that could diverge, and no place a "validation rule"
//! could live separately from a "schema".
//!
//! This is the argument `contracts/http-routing.md` already makes for routes — *"Both functions
//! take `&RouteRegistry`. Neither has access to any other source"* — applied to input rules.
//!
//! # The subset is BOUNDED, and a declaration outside it is REFUSED
//!
//! Renvor interprets the keywords listed in [`ENFORCED_KEYWORDS`]. A schema using anything else is
//! rejected by [`Declaration::new`] **at declaration time**, with the offending keyword named.
//!
//! That refusal is the whole difference between a bounded subset and a partial implementation. A
//! partial implementation ignores what it does not understand, publishes the constraint anyway,
//! and enforces nothing — so the description becomes a lie at exactly the point an author was
//! relying on it. Refusing at declaration time means an unenforceable constraint never ships.
//!
//! # Why Renvor interprets rather than delegating at runtime
//!
//! Measured on 2026-08-23: `jsonschema` 0.50.1 resolves **103 packages**, against
//! `renvor-http`'s current **65**. Validating a request body would more than double the
//! transport's dependency surface — for the ICU stack, `fancy-regex`, `num-bigint`, and
//! `wasm-bindgen` among others.
//!
//! It is a **dev-dependency** instead, and `tests/differential.rs` asserts that Renvor's verdict
//! equals the reference implementation's over generated schema/instance pairs. A bounded subset
//! that silently disagrees with the standard it publishes is the failure mode that matters, and
//! that test is what catches it.

use renvor_error::{InvalidParam, Location, Pointer};
use serde_json::{Map, Value};

use crate::reason::Reason;

/// The keywords Renvor **enforces**.
///
/// Each appears in the published description and is checked at runtime. The two lists cannot
/// disagree, because there is one schema value.
pub const ENFORCED_KEYWORDS: [&str; 18] = [
    "type",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "uniqueItems",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "multipleOf",
    "enum",
    "const",
    "$ref",
];

/// The keywords Renvor **carries into the description without enforcing**.
///
/// These are annotations, not assertions. `format` is here because JSON Schema 2020-12 makes it an
/// annotation by default — the format-annotation vocabulary — so publishing it and not enforcing
/// it is what the standard says it means, not a shortcut. Stated here rather than left to
/// inference, because the opposite is the more common assumption.
pub const ANNOTATION_KEYWORDS: [&str; 12] = [
    "$schema",
    "$id",
    "$comment",
    "$defs",
    "title",
    "description",
    "examples",
    "example",
    "default",
    "deprecated",
    "readOnly",
    "format",
];

/// Why a schema could not be accepted as a declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclarationError {
    /// The schema used a keyword outside the enforced and annotation sets.
    UnsupportedKeyword {
        /// The keyword.
        keyword: String,
        /// Where in the schema it appeared, as a JSON Pointer.
        at: String,
    },
    /// The schema was not a JSON object or boolean.
    NotASchema {
        /// Where in the schema the non-schema appeared.
        at: String,
    },
    /// A keyword's VALUE was of a type the interpreter cannot read.
    ///
    /// Refused rather than skipped: a keyword the walker cannot read is one it silently ignores,
    /// while the description publishes it regardless.
    MalformedKeyword {
        /// The keyword.
        keyword: String,
        /// Where in the schema it appeared.
        at: String,
    },
    /// A `$ref` pointed somewhere this crate does not resolve.
    UnresolvableRef {
        /// The reference.
        reference: String,
    },
}

impl core::fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedKeyword { keyword, at } => write!(
                f,
                "`{keyword}` at `{at}` is outside the subset Renvor enforces. It is refused here, \
                 at declaration time, rather than published as a constraint nothing checks — a \
                 description that promises an unenforced rule is worse than one that omits it. \
                 Enforced keywords: {}",
                ENFORCED_KEYWORDS.join(", ")
            ),
            Self::NotASchema { at } => write!(
                f,
                "the value at `{at}` is not a schema; a schema is an object or a boolean"
            ),
            Self::MalformedKeyword { keyword, at } => write!(
                f,
                "`{keyword}` at `{at}` has a value this interpreter cannot read. It is refused \
                 rather than skipped — a keyword the walker cannot read is one it silently \
                 ignores, while the description publishes it anyway, so the API would promise a \
                 constraint nothing checks"
            ),
            Self::UnresolvableRef { reference } => write!(
                f,
                "`{reference}` could not be resolved. Renvor resolves only local references of \
                 the form `#/$defs/<name>`; a remote reference would make generating a \
                 description perform a network call"
            ),
        }
    }
}

impl core::error::Error for DeclarationError {}

/// One input that failed, before it becomes an [`InvalidParam`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Issue {
    /// Where the input arrived.
    pub location: Location,
    /// Which input, structurally. Built from **declared** names only.
    pub pointer: Pointer,
    /// Why it was refused.
    pub reason: Reason,
}

impl Issue {
    /// Renders this issue as the wire form.
    #[must_use]
    pub fn into_invalid_param(self) -> InvalidParam {
        InvalidParam {
            location: self.location,
            pointer: self.pointer,
            reason: self.reason.as_str(),
        }
    }
}

/// A validated schema, usable both as a runtime rule and as a published description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    root: Value,
}

impl Declaration {
    /// Accepts `schema` as a declaration, or reports why it cannot be one.
    ///
    /// # Errors
    ///
    /// [`DeclarationError::UnsupportedKeyword`] naming the first keyword outside the subset, and
    /// where it appeared. [`DeclarationError::NotASchema`] for a non-schema in a schema position.
    pub fn new(schema: Value) -> Result<Self, DeclarationError> {
        check_subset(&schema, &schema, "", 0)?;
        Ok(Self { root: schema })
    }

    /// Derives a declaration from a type's `schemars` schema.
    ///
    /// # Errors
    ///
    /// As [`Declaration::new`]. A type whose schema uses an unsupported construct — a
    /// data-carrying enum, for instance, which `schemars` renders with `oneOf` — is refused here
    /// rather than silently unenforced.
    pub fn of<T: schemars::JsonSchema>() -> Result<Self, DeclarationError> {
        let schema = serde_json::to_value(schemars::schema_for!(T))
            .expect("a schemars schema is always representable as JSON");
        Self::new(schema)
    }

    /// The schema value. **This is what the description embeds.**
    #[must_use]
    pub const fn schema(&self) -> &Value {
        &self.root
    }

    /// Validates `instance`, reporting **every** violation it can determine.
    ///
    /// # Every issue, not the first
    ///
    /// FR-005 requires a caller to be able to correct an input in one round trip. Stopping at the
    /// first violation turns one round trip into as many as the caller has mistakes.
    ///
    /// Ordering is deterministic: issues are produced by a depth-first walk in which object
    /// members are visited in sorted key order, so the same input always yields the same sequence.
    #[must_use]
    pub fn validate(&self, location: Location, instance: &Value) -> Vec<Issue> {
        let mut issues = Vec::new();
        let mut walker = Walker {
            root: &self.root,
            location,
            issues: &mut issues,
            depth: 0,
        };
        walker.check(&self.root, instance, "");
        issues
    }

    /// Whether `instance` satisfies this declaration.
    #[must_use]
    pub fn is_valid(&self, instance: &Value) -> bool {
        self.validate(Location::Body, instance).is_empty()
    }
}

/// The maximum schema nesting this interpreter follows.
///
/// A `$ref` cycle — a type that contains itself — would otherwise recurse without end. The bound
/// is a **refusal**, reported as a type mismatch at the point it is reached, rather than a stack
/// overflow, because an unbounded wait or an abort in a kernel-owned path is a defect and not a
/// configuration choice (contract C-L7, applied one register lower).
const MAX_DEPTH: usize = 64;

/// The most violations one validation reports.
///
/// # Why a bound on the COUNT, and why here rather than at render time
///
/// Every other bound in this phase is real; this one was missing, and it was the expensive one.
/// `validate` produces one issue per violating element, so a schema of
/// `{"type":"array","items":{"type":"string"}}` and a body of `[1,1,1,…]` — well inside the 2 MiB
/// request-body limit — yields over a million issues. Measured on the real code: a 2 097 001-byte
/// request produced **1 048 500** invalid parameters and a **68 MB** response, at 210 MB peak
/// resident memory and 416 ms of CPU.
///
/// Two facts compounded it. The concurrency ceiling is 1024, and the 30-second request timeout
/// **cannot cancel this** — validation and serialisation are synchronous with no `.await`, so
/// there is no yield point for a timeout to act on and the work blocks a worker thread outright.
///
/// The cap is applied **in the walker**, not when the response is rendered. Capping at render
/// would still allocate a million `Pointer` strings first, which is most of the cost.
///
/// A truncated list ends with [`Reason::TooManyIssues`], so a caller can tell "these are all your
/// mistakes" from "these are the first hundred". Reported by security review.
pub const MAX_ISSUES: usize = 100;

struct Walker<'a> {
    root: &'a Value,
    location: Location,
    issues: &'a mut Vec<Issue>,
    depth: usize,
}

impl Walker<'_> {
    /// Whether the issue budget is spent.
    ///
    /// Checked before every recursion as well as before every push, so a pathological instance
    /// stops being WALKED rather than merely stopping being recorded.
    fn full(&self) -> bool {
        self.issues.len() >= MAX_ISSUES
    }

    fn push(&mut self, pointer: &str, reason: Reason) {
        if self.full() {
            return;
        }
        // The LAST slot is spent saying the list is truncated, so a caller is never told a
        // hundred problems are all of them.
        if self.issues.len() == MAX_ISSUES - 1 {
            let root = Pointer::new("").expect("the empty pointer is always representable");
            self.issues.push(Issue {
                location: self.location,
                pointer: root,
                reason: Reason::TooManyIssues,
            });
            return;
        }
        // A declared name that cannot be represented in a pointer falls back to the ROOT pointer.
        // Less information, never more — the fail-safe direction.
        let pointer = Pointer::new(pointer).unwrap_or_else(|_| {
            Pointer::new("").expect("the empty pointer is always representable")
        });
        self.issues.push(Issue {
            location: self.location,
            pointer,
            reason,
        });
    }

    fn check(&mut self, schema: &Value, instance: &Value, pointer: &str) {
        // SHORT-CIRCUIT. Once the budget is spent there is nothing left to record, so continuing
        // to walk a million-element array buys only the walk.
        if self.full() {
            return;
        }
        if self.depth > MAX_DEPTH {
            self.push(pointer, Reason::TypeMismatch);
            return;
        }

        // A boolean schema: `true` accepts anything, `false` accepts nothing.
        if let Some(accepts) = schema.as_bool() {
            if !accepts {
                self.push(pointer, Reason::TypeMismatch);
            }
            return;
        }
        let Some(object) = schema.as_object() else {
            // A value in a schema position that is neither an object nor a boolean is not a
            // schema. Returning here ACCEPTED ANYTHING, which is the wrong direction — a
            // declaration Renvor cannot interpret must refuse the input, not wave it through.
            //
            // `Declaration::new` now refuses these, so this branch should be unreachable. It fails
            // CLOSED anyway, because "should be unreachable" is not a bound. Found by security
            // review.
            self.push(pointer, Reason::TypeMismatch);
            return;
        };

        // `$ref` FIRST, and it REPLACES the schema rather than merging with it. That is the
        // 2020-12 behaviour schemars relies on: it emits `$ref` alongside annotations only.
        if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
            match resolve(self.root, reference) {
                Some(target) => {
                    self.depth += 1;
                    self.check(target, instance, pointer);
                    self.depth -= 1;
                }
                // A reference that does not resolve is a declaration defect, not a caller defect.
                // `Declaration::new` refuses those, so reaching here means the schema changed
                // underneath — reported rather than ignored.
                None => self.push(pointer, Reason::TypeMismatch),
            }
            return;
        }

        if !self.check_type(object, instance, pointer) {
            // Once the type is wrong, every other keyword would report a second failure about the
            // same mistake. One issue per mistake keeps `invalidParams` readable.
            return;
        }

        self.check_enum_and_const(object, instance, pointer);

        match instance {
            Value::String(text) => self.check_string(object, text, pointer),
            Value::Number(number) => self.check_number(object, number, pointer),
            Value::Array(items) => self.check_array(object, items, pointer),
            Value::Object(members) => self.check_object(object, members, pointer),
            Value::Bool(_) | Value::Null => {}
        }
    }

    /// Returns whether the instance's type is acceptable.
    fn check_type(&mut self, schema: &Map<String, Value>, instance: &Value, pointer: &str) -> bool {
        let Some(declared) = schema.get("type") else {
            return true;
        };

        let matches = match declared {
            Value::String(name) => type_matches(name, instance),
            // A type ARRAY is how `schemars` spells nullable: `["string", "null"]`.
            Value::Array(names) => names
                .iter()
                .filter_map(Value::as_str)
                .any(|name| type_matches(name, instance)),
            _ => true,
        };

        if !matches {
            self.push(pointer, Reason::TypeMismatch);
        }
        matches
    }

    fn check_enum_and_const(
        &mut self,
        schema: &Map<String, Value>,
        instance: &Value,
        pointer: &str,
    ) {
        if let Some(Value::Array(allowed)) = schema.get("enum")
            && !allowed.contains(instance)
        {
            self.push(pointer, Reason::NotInEnum);
        }
        if let Some(expected) = schema.get("const")
            && expected != instance
        {
            self.push(pointer, Reason::NotConstant);
        }
    }

    fn check_string(&mut self, schema: &Map<String, Value>, text: &str, pointer: &str) {
        // JSON Schema counts CODE POINTS, not bytes. `"é".len()` is 2 and its length is 1; using
        // bytes would refuse a legitimate value and would do it inconsistently by alphabet.
        let length = text.chars().count() as u64;
        if let Some(minimum) = schema.get("minLength").and_then(Value::as_u64)
            && length < minimum
        {
            self.push(pointer, Reason::TooShort);
        }
        if let Some(maximum) = schema.get("maxLength").and_then(Value::as_u64)
            && length > maximum
        {
            self.push(pointer, Reason::TooLong);
        }
    }

    fn check_number(
        &mut self,
        schema: &Map<String, Value>,
        number: &serde_json::Number,
        pointer: &str,
    ) {
        let Some(value) = number.as_f64() else {
            return;
        };

        let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
        if let Some(minimum) = bound("minimum")
            && value < minimum
        {
            self.push(pointer, Reason::OutOfRange);
        }
        if let Some(maximum) = bound("maximum")
            && value > maximum
        {
            self.push(pointer, Reason::OutOfRange);
        }
        if let Some(minimum) = bound("exclusiveMinimum")
            && value <= minimum
        {
            self.push(pointer, Reason::OutOfRange);
        }
        if let Some(maximum) = bound("exclusiveMaximum")
            && value >= maximum
        {
            self.push(pointer, Reason::OutOfRange);
        }
        if let Some(step) = bound("multipleOf")
            && step > 0.0
        {
            let quotient = value / step;
            // A tolerance is unavoidable in binary floating point: 0.3 / 0.1 is 2.9999999999999996.
            // The comparison is against the NEAREST integer rather than the truncated one, so a
            // value just below a multiple is not reported as a violation of a rule it satisfies.
            if (quotient - quotient.round()).abs() > 1e-9 {
                self.push(pointer, Reason::NotMultipleOf);
            }
        }
    }

    fn check_array(&mut self, schema: &Map<String, Value>, items: &[Value], pointer: &str) {
        let length = items.len() as u64;
        if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64)
            && length < minimum
        {
            self.push(pointer, Reason::TooFewItems);
        }
        if let Some(maximum) = schema.get("maxItems").and_then(Value::as_u64)
            && length > maximum
        {
            self.push(pointer, Reason::TooManyItems);
        }
        if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
            // O(n log n), NOT the obvious O(n²) pairwise scan.
            //
            // The pairwise version is what this was first written as, with a comment claiming
            // `maxItems` bounded it. It does not: `uniqueItems` is reachable WITHOUT `maxItems` —
            // `schemars` emits exactly that for a `HashSet` or `BTreeSet` field — and the only
            // remaining bound is the 2 MiB request-body limit. An array of ~500 000 small integers
            // fits inside it, and 500 000² is 2.5 × 10¹¹ comparisons of a caller's choosing.
            //
            // That is an algorithmic-complexity denial of service, and the bound that was supposed
            // to prevent it was one the schema does not have to declare.
            //
            // Serialising each item is exact rather than approximate: `serde_json::Map` is ordered,
            // so two values are equal if and only if their renderings are. `Value` implements
            // neither `Hash` nor `Ord`, which is why the set is keyed by the rendering.
            let mut seen = std::collections::BTreeSet::new();
            let repeated = items.iter().any(|item| !seen.insert(item.to_string()));
            if repeated {
                self.push(pointer, Reason::NotUnique);
            }
        }
        if let Some(item_schema) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                self.depth += 1;
                self.check(item_schema, item, &format!("{pointer}/{index}"));
                self.depth -= 1;
            }
        }
    }

    fn check_object(
        &mut self,
        schema: &Map<String, Value>,
        members: &Map<String, Value>,
        pointer: &str,
    ) {
        let properties = schema.get("properties").and_then(Value::as_object);

        if let Some(Value::Array(required)) = schema.get("required") {
            for name in required.iter().filter_map(Value::as_str) {
                if !members.contains_key(name) {
                    // The name is DECLARED, so naming it is safe — it is the schema's word, not
                    // the caller's.
                    self.push(
                        &format!("{pointer}/{}", escape(name)),
                        Reason::RequiredMissing,
                    );
                }
            }
        }

        // Undeclared members. The member NAME is attacker-chosen, so it is NOT echoed: the issue
        // points at the containing object and says `unknown_member`.
        //
        // `additionalProperties: false` is what `schemars` emits for a `deny_unknown_fields`
        // struct. Absent, unknown members are permitted, which is the JSON Schema default.
        // An ABSENT `properties` means the empty declared set, not "skip the check".
        //
        // The earlier version was gated on `let Some(declared) = properties`, so
        // `{"additionalProperties": false}` with no `properties` — which in JSON Schema permits NO
        // members at all — accepted everything. Deny-by-default, inverted. Found by security
        // review.
        if schema.get("additionalProperties").and_then(Value::as_bool) == Some(false) {
            let declared = properties;
            let unknown = members
                .keys()
                .any(|name| declared.is_none_or(|d| !d.contains_key(name)));
            if unknown {
                self.push(pointer, Reason::UnknownMember);
            }
        }

        // SORTED, so the issue sequence is deterministic for the same input. A `serde_json::Map`
        // preserves insertion order by default, which is the request's order — attacker-chosen.
        if let Some(declared) = properties {
            let mut names: Vec<&String> = declared.keys().collect();
            names.sort();
            for name in names {
                if let Some(value) = members.get(name) {
                    let child = format!("{pointer}/{}", escape(name));
                    self.depth += 1;
                    self.check(&declared[name], value, &child);
                    self.depth -= 1;
                }
            }
        }
    }
}

/// RFC 6901 pointer escaping: `~` becomes `~0` and `/` becomes `~1`.
///
/// Without this, a declared member named `a/b` would produce a pointer that reads as two segments.
fn escape(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn type_matches(name: &str, instance: &Value) -> bool {
    match name {
        "null" => instance.is_null(),
        "boolean" => instance.is_boolean(),
        "object" => instance.is_object(),
        "array" => instance.is_array(),
        "number" => instance.is_number(),
        // JSON Schema: an integer is a number with zero fractional part — `1.0` IS an integer.
        "integer" => instance.as_f64().is_some_and(|v| v.fract() == 0.0),
        "string" => instance.is_string(),
        _ => true,
    }
}

/// Resolves a local `#/$defs/<name>` reference.
///
/// Only local references. A remote one would make generating a description perform a network
/// call, which FR-031 forbids.
fn resolve<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    let path = reference.strip_prefix("#/")?;
    let mut current = root;
    for segment in path.split('/') {
        let segment = segment.replace("~1", "/").replace("~0", "~");
        current = current.get(&segment)?;
    }
    Some(current)
}

/// Walks a schema and refuses the first keyword outside the supported sets.
///
/// # Depth-bounded, like the validator
///
/// [`Walker::check`] bounds its recursion because a `$ref` cycle would otherwise abort the process
/// on a caller's request. This walk follows no `$ref`, so a cycle cannot reach it — but literal
/// nesting can, and a schema arrives as a `serde_json::Value` that an author may have parsed from
/// a file. An abort at declaration time is a worse diagnostic than a refusal, and "an author would
/// not do that" is not a bound.
fn check_subset(
    schema: &Value,
    root: &Value,
    at: &str,
    depth: usize,
) -> Result<(), DeclarationError> {
    if depth > MAX_DEPTH {
        return Err(DeclarationError::NotASchema { at: at.to_owned() });
    }
    if schema.is_boolean() {
        return Ok(());
    }
    let Some(object) = schema.as_object() else {
        return Err(DeclarationError::NotASchema { at: at.to_owned() });
    };

    // SORTED, so the keyword reported for a schema with several unsupported keywords is stable
    // rather than dependent on the order serde happened to parse them in.
    let mut keys: Vec<&String> = object.keys().collect();
    keys.sort();
    for key in keys {
        let supported = ENFORCED_KEYWORDS.contains(&key.as_str())
            || ANNOTATION_KEYWORDS.contains(&key.as_str());
        if !supported {
            return Err(DeclarationError::UnsupportedKeyword {
                keyword: key.clone(),
                at: at.to_owned(),
            });
        }
    }

    // EVERY KEYWORD'S VALUE IS TYPE-CHECKED, not only its name.
    //
    // Checking names alone was the gap: every read in the walker is `and_then(Value::as_u64)` or
    // `as_bool`, so a WRONG-TYPED value yields `None` and the constraint is silently skipped —
    // while `schema()` hands the same value to OpenAPI, so the description publishes a rule
    // nothing enforces. `{"maxLength": "5"}`, `{"required": "name"}`, `{"type": 42}`, and
    // `{"uniqueItems": "true"}` were all accepted and all unenforced.
    //
    // That is the direct counterexample to this module's own claim that "refusing at declaration
    // time means an unenforceable constraint never ships". It held for unknown keyword NAMES and
    // failed for every malformed VALUE. Found by security review.
    check_keyword_values(object, at)?;

    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        if !reference.starts_with("#/") {
            return Err(DeclarationError::UnresolvableRef {
                reference: reference.to_owned(),
            });
        }
        // A reference that RESOLVES TO A NON-SCHEMA is worse than one that does not resolve: the
        // walker found a value, could not read it as a schema, and accepted everything under it.
        // `{"$ref": "#/title"}` pointing at a string was accepted, and validated nothing.
        match resolve(root, reference) {
            Some(target) if target.is_object() || target.is_boolean() => {}
            _ => {
                return Err(DeclarationError::UnresolvableRef {
                    reference: reference.to_owned(),
                });
            }
        }
    }

    for container in ["properties", "$defs"] {
        if let Some(map) = object.get(container).and_then(Value::as_object) {
            let mut names: Vec<&String> = map.keys().collect();
            names.sort();
            for name in names {
                check_subset(
                    &map[name],
                    root,
                    &format!("{at}/{container}/{}", escape(name)),
                    depth + 1,
                )?;
            }
        }
    }
    if let Some(items) = object.get("items") {
        check_subset(items, root, &format!("{at}/items"), depth + 1)?;
    }
    if let Some(additional) = object.get("additionalProperties")
        && !additional.is_boolean()
    {
        check_subset(
            additional,
            root,
            &format!("{at}/additionalProperties"),
            depth + 1,
        )?;
    }

    Ok(())
}

/// Type-checks every keyword's VALUE, not only its name.
///
/// A keyword whose value the walker cannot read is a keyword the walker silently skips — while the
/// description publishes it. Refusing here is what makes "an unenforceable constraint never ships"
/// true of values as well as names.
fn check_keyword_values(object: &Map<String, Value>, at: &str) -> Result<(), DeclarationError> {
    /// The JSON types a keyword's value may take.
    fn wrong(keyword: &str, at: &str) -> DeclarationError {
        DeclarationError::MalformedKeyword {
            keyword: keyword.to_owned(),
            at: at.to_owned(),
        }
    }

    // Non-negative integers.
    for keyword in ["minLength", "maxLength", "minItems", "maxItems"] {
        if let Some(value) = object.get(keyword)
            && value.as_u64().is_none()
        {
            return Err(wrong(keyword, at));
        }
    }

    // Numbers.
    for keyword in [
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
    ] {
        if let Some(value) = object.get(keyword)
            && value.as_f64().is_none()
        {
            return Err(wrong(keyword, at));
        }
    }

    // Booleans.
    if let Some(value) = object.get("uniqueItems")
        && !value.is_boolean()
    {
        return Err(wrong("uniqueItems", at));
    }

    // `required` is an array OF STRINGS. `{"required": "name"}` reads as a single name and is not.
    if let Some(value) = object.get("required") {
        let Some(names) = value.as_array() else {
            return Err(wrong("required", at));
        };
        if names.iter().any(|name| !name.is_string()) {
            return Err(wrong("required", at));
        }
    }

    // `enum` is a non-empty array.
    if let Some(value) = object.get("enum") {
        match value.as_array() {
            Some(members) if !members.is_empty() => {}
            _ => return Err(wrong("enum", at)),
        }
    }

    // `type` is a KNOWN name, or an array of known names. `{"type": "strng"}` was accepted and
    // matched everything, because the walker's fallback for an unrecognised name is `true`.
    let known = |name: &str| {
        matches!(
            name,
            "null" | "boolean" | "object" | "array" | "number" | "integer" | "string"
        )
    };
    if let Some(value) = object.get("type") {
        match value {
            Value::String(name) if known(name) => {}
            Value::Array(names) if !names.is_empty() => {
                let ok = names.iter().all(|name| name.as_str().is_some_and(known));
                if !ok {
                    return Err(wrong("type", at));
                }
            }
            _ => return Err(wrong("type", at)),
        }
    }

    // Containers that must be objects.
    for keyword in ["properties", "$defs"] {
        if let Some(value) = object.get(keyword)
            && !value.is_object()
        {
            return Err(wrong(keyword, at));
        }
    }

    // `$ref` is a string.
    if let Some(value) = object.get("$ref")
        && !value.is_string()
    {
        return Err(wrong("$ref", at));
    }

    Ok(())
}
