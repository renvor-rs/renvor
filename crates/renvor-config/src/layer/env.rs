//! The environment layer: highest precedence, and a **layer** rather than a per-field annotation.
//!
//! # Why this is a layer
//!
//! C-C4 states it directly: *"Environment is a layer with a precedence position, not a per-field
//! opt-in annotation."* Both rejected candidates take the annotation approach, where a field
//! nobody remembered to mark is silently unreachable from the environment. Here the whole
//! environment is read, mapped to key paths, and merged like any other source.
//!
//! # Turning text into typed values without a schema description
//!
//! Every environment value is a string. C-C3 permits decoding `8080` into an integer, because the
//! target type comes from the author's declaration rather than from reconciling two layers — but
//! the adapter has no per-key type description to consult.
//!
//! So each variable is offered to the schema in **at most two forms**, and the one that decodes
//! wins:
//!
//! 1. the text parsed as a TOML scalar, when it parses at all — `8080` → integer, `true` → boolean;
//! 2. the text as a plain string.
//!
//! **This is not a fallback in the sense FR-022 prohibits.** Both candidates are the *same value*
//! offered to the *same declared type*, and if neither decodes the result is a hard error naming
//! the key, the layer, and the expectation. Nothing falls through to a lower layer and nothing
//! becomes a default.
//!
//! # There is no empty-string exemption
//!
//! `PORT=""` produces exactly one candidate — the empty string — because `""` does not parse as a
//! TOML scalar. If the declared field cannot hold an empty string, resolution **fails**. It is not
//! reinterpreted as unset and it does not fall through (C-C3, obligation 6). That is the precise
//! behaviour the rejected candidate got wrong, and it is asserted in both directions here.

use std::collections::BTreeMap;

use renvor_core::KernelError;
use renvor_core::config_port::SourceLayer;
use renvor_core::error::context::{Constraint, configuration};
use serde::de::DeserializeOwned;
use toml::{Table, Value};

use crate::layer::decode::{decode_single, nest};

/// The separator that introduces a nesting level in a variable name.
///
/// Two underscores, so a single underscore stays available inside a key name: `LOG_LEVEL` is one
/// key called `log_level`, while `SERVER__PORT` is `server.port`.
pub const NESTING_SEPARATOR: &str = "__";

/// The deepest key a single environment variable may describe.
///
/// # This bound exists because its absence aborted the process
///
/// Found by the W-005 security review (finding 3.1) and **measured**: one variable named
/// `RENVOR_A__A__A__…` with about 3,000 separators — a name of roughly 9 KB, far under the ~128 KiB
/// an operating system permits per environment string — overflowed the stack and aborted. Nesting
/// a `toml::Table` recurses once per level, deserializing it recurses again, and dropping it
/// recurses a third time.
///
/// **A stack overflow is not a panic.** `catch_unwind` has nothing to catch, and running the work
/// on a worker thread does not help — an overflow on any thread aborts the whole process. So no
/// containment Renvor has could have caught this; only refusing to build the structure can.
///
/// The file layer never had the defect, and not by design: `toml_parser` enforces its own recursion
/// limit, measured between 80 and 90 levels. The environment layer does not go through the TOML
/// **document** parser, so it inherited no limit. One untrusted surface was protected by a
/// dependency and the other by nothing, and nothing in the code or the tests noticed the asymmetry.
///
/// 32 is Renvor's number — generous for configuration nobody writes by hand past three or four
/// levels, and an order of magnitude under the measured threshold. Recorded as an open item with
/// the phase's other chosen bounds.
pub const MAX_KEY_DEPTH: usize = 32;

/// Reads the process environment into a prefixed map, refusing rather than panicking.
///
/// # Why not `std::env::vars`
///
/// `std::env::vars` **panics** on the first entry whose name or value is not valid Unicode, and
/// the entry that trips it need not be Renvor's — one stray variable set by any other program in
/// the process's ancestry took the whole application down before it reached `Boot`. That is a
/// panic in ordinary operation, which SC-004 allows **0** of, reached through no fault of the
/// author's configuration. `vars_os` yields the same entries without deciding they must be
/// Unicode, which moves the decision here where it can be made per-variable.
///
/// # What each case does
///
/// | Entry | Outcome |
/// |---|---|
/// | unrelated name, unrepresentable | **skipped** — somebody else's variable is not Renvor's problem |
/// | prefixed name, unrepresentable | refused, naming the lossy form of the name |
/// | prefixed name, unrepresentable value | refused, naming the key and **withholding the value** |
///
/// A name that is not Unicode cannot be compared exactly, so its lossy form decides whether it is
/// ours. That direction is deliberate and fails closed: replacement characters can only ever pull
/// **more** variables into the check, never fewer, so a prefixed variable cannot slip past by
/// being unrepresentable.
///
/// # Errors
///
/// Returns [`KernelError::Configuration`] naming the variable. A value is **never** reproduced in
/// the message: an environment variable is the most common place a credential lives, and an error
/// that quotes one has published it to every log that catches the failure.
pub fn read_process_environment(prefix: &str) -> Result<BTreeMap<String, String>, KernelError> {
    let mut variables = BTreeMap::new();

    for (name, value) in std::env::vars_os() {
        let name = match name.into_string() {
            Ok(name) => name,
            Err(raw) => {
                let lossy = raw.to_string_lossy().into_owned();
                if lossy.starts_with(prefix) {
                    return Err(configuration(
                        lossy,
                        SourceLayer::Environment.label(),
                        "a variable name that is valid Unicode",
                        &Constraint::Rule(
                            "the variable's name is not valid Unicode; it is shown with \
                             replacement characters, and its value is withheld",
                        ),
                    ));
                }
                continue;
            }
        };

        if !name.starts_with(prefix) {
            continue;
        }

        let Ok(value) = value.into_string() else {
            return Err(configuration(
                name,
                SourceLayer::Environment.label(),
                "a variable value that is valid Unicode",
                &Constraint::Rule(
                    "the variable's value is not valid Unicode and is withheld from this message",
                ),
            ));
        };

        variables.insert(name, value);
    }

    Ok(variables)
}

/// Reads the environment into a layer, decoding each variable against the schema.
///
/// Only variables starting with `prefix` are considered. The prefix is stripped, the remainder is
/// lowercased, and [`NESTING_SEPARATOR`] introduces nesting.
///
/// # Errors
///
/// Returns [`KernelError::Configuration`] naming the key, the layer, and the expectation for the
/// first variable that cannot decode. There is no empty-string exemption.
pub fn read_environment<P: DeserializeOwned>(
    prefix: &str,
    variables: &BTreeMap<String, String>,
) -> Result<Table, KernelError> {
    let mut table = Table::new();

    for (name, text) in variables {
        let Some(remainder) = name.strip_prefix(prefix) else {
            continue;
        };
        if remainder.is_empty() {
            continue;
        }

        let key = remainder.to_lowercase().replace(NESTING_SEPARATOR, ".");

        // Checked BEFORE `nest` builds anything. Every recursion this bound protects — nesting,
        // deserializing, and dropping — happens inside the calls below, so a check afterwards
        // would run after the overflow it exists to prevent.
        let depth = key.split('.').count();
        if depth > MAX_KEY_DEPTH {
            return Err(configuration(
                key,
                SourceLayer::Environment.label(),
                "a key nested no deeper than the ceiling",
                &Constraint::Rule(
                    "the variable name describes a key deeper than 32 levels; building it would \
                     recurse deeply enough to overflow the stack, which no Rust guard can catch",
                ),
            ));
        }

        let value = decode_variable::<P>(&key, text)?;
        merge_into(&mut table, &key, value);
    }

    Ok(table)
}

/// Picks the form of `text` that decodes, or reports that neither does.
fn decode_variable<P: DeserializeOwned>(key: &str, text: &str) -> Result<Value, KernelError> {
    let mut candidates: Vec<Value> = Vec::with_capacity(2);
    if let Some(scalar) = parse_scalar(text) {
        candidates.push(scalar);
    }
    candidates.push(Value::String(text.to_owned()));

    let mut last_error = None;
    for candidate in candidates {
        match decode_single::<P>(key, &candidate, &SourceLayer::Environment) {
            Ok(()) => return Ok(candidate),
            Err(error) => last_error = Some(error),
        }
    }

    // Neither form fits the declared type. Reported, never reinterpreted as unset.
    Err(last_error.unwrap_or_else(|| {
        configuration(
            key,
            SourceLayer::Environment.label(),
            "a value matching the declared schema",
            &Constraint::Rule("the value could not be decoded in any accepted form"),
        )
    }))
}

/// Parses `text` as a TOML scalar, or returns `None` if it is not one.
///
/// Bare words, empty strings, and anything else TOML does not recognise as a value return `None`
/// and are offered as plain strings instead.
fn parse_scalar(text: &str) -> Option<Value> {
    let document = format!("value = {text}");
    let table = document.parse::<Table>().ok()?;
    table.get("value").cloned()
}

/// Inserts a dotted key into a table, creating intermediate tables.
fn merge_into(table: &mut Table, key: &str, value: Value) {
    let nested = nest(key, value);
    for (name, sub) in nested {
        graft(table, name, sub);
    }
}

/// Grafts one branch onto a table, merging tables rather than replacing them.
fn graft(table: &mut Table, name: String, value: Value) {
    match (table.get_mut(&name), value) {
        (Some(Value::Table(existing)), Value::Table(incoming)) => {
            for (inner_name, inner) in incoming {
                graft(existing, inner_name, inner);
            }
        }
        (_, value) => {
            table.insert(name, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NESTING_SEPARATOR, read_environment};
    use renvor_core::ErrorCategory;
    use serde::Deserialize;
    use std::collections::BTreeMap;

    /// The partial forms exist to be **decoded into**, never read from: proving a source fits the
    /// schema is their whole job. `dead_code` fires because nothing reads the fields back, which is
    /// the intended shape rather than an oversight.
    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct Partial {
        host: Option<String>,
        port: Option<u16>,
        debug: Option<bool>,
        server: Option<PartialServer>,
    }

    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct PartialServer {
        threads: Option<u16>,
        name: Option<String>,
    }

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn textual_values_decode_into_their_declared_types() {
        // C-C3: permitted, and not cross-layer coercion — the target type is the author's.
        let table = read_environment::<Partial>(
            "RENVOR_",
            &env(&[
                ("RENVOR_PORT", "8080"),
                ("RENVOR_DEBUG", "true"),
                ("RENVOR_HOST", "localhost"),
            ]),
        )
        .expect("every value fits its declared type");

        assert_eq!(
            table["port"].as_integer(),
            Some(8080),
            "text became a number"
        );
        assert_eq!(table["debug"].as_bool(), Some(true));
        assert_eq!(
            table["host"].as_str(),
            Some("localhost"),
            "a bare word stays a string"
        );
    }

    #[test]
    fn a_numeric_looking_value_stays_a_string_when_the_field_is_one() {
        // The two-candidate rule doing its job: `8080` parses as an integer, but the declared
        // field is a `String`, so the string form is the one that decodes.
        let table = read_environment::<Partial>("RENVOR_", &env(&[("RENVOR_HOST", "8080")]))
            .expect("the string form fits");
        assert_eq!(table["host"].as_str(), Some("8080"));
    }

    #[test]
    fn double_underscore_introduces_nesting() {
        let table = read_environment::<Partial>(
            "RENVOR_",
            &env(&[
                ("RENVOR_SERVER__THREADS", "8"),
                ("RENVOR_SERVER__NAME", "web"),
            ]),
        )
        .expect("both fit");

        let server = table["server"].as_table().expect("a table");
        assert_eq!(server["threads"].as_integer(), Some(8));
        assert_eq!(
            server["name"].as_str(),
            Some("web"),
            "the second variable must not replace the first's table"
        );
    }

    #[test]
    fn variables_without_the_prefix_are_ignored() {
        let table = read_environment::<Partial>(
            "RENVOR_",
            &env(&[("PATH", "/usr/bin"), ("RENVOR_PORT", "1")]),
        )
        .expect("only the prefixed variable is read");
        assert_eq!(table.len(), 1);
        assert!(table.contains_key("port"));
    }

    #[test]
    fn an_undecodable_value_fails_naming_key_layer_and_expectation() {
        // Obligation 5.
        let error = read_environment::<Partial>("RENVOR_", &env(&[("RENVOR_PORT", "wide-open")]))
            .expect_err("a bare word cannot be a u16");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        let rendered = error.to_string();
        assert!(rendered.contains("port"), "key: {rendered}");
        assert!(rendered.contains("environment"), "layer: {rendered}");
        assert!(rendered.contains("u16"), "expected type: {rendered}");
    }

    #[test]
    fn an_empty_value_fails_rather_than_being_treated_as_unset() {
        // Obligation 6, and the exact behaviour the rejected candidate gets wrong: its
        // documentation says an empty value that fails to parse "is treated as unset".
        let error = read_environment::<Partial>("RENVOR_", &env(&[("RENVOR_PORT", "")]))
            .expect_err("an empty string cannot be a u16");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert!(error.to_string().contains("port"), "{error}");
    }

    #[test]
    fn an_empty_value_is_accepted_where_the_field_can_hold_one() {
        // POSITIVE CONTROL for the test above: empty is rejected because it does not fit the
        // declared type, not because empty is rejected on sight. C-C11 requires present-and-empty
        // to remain a real, distinguishable value.
        let table = read_environment::<Partial>("RENVOR_", &env(&[("RENVOR_HOST", "")]))
            .expect("an empty string is a valid String");
        assert_eq!(table["host"].as_str(), Some(""));
    }

    #[test]
    fn a_variable_naming_a_deeply_nested_key_is_refused_rather_than_overflowing_the_stack() {
        // W-005 security finding 3.1, as a regression test. 3,000 levels aborted the process
        // before this bound existed — measured, not theorised — and an abort is not catchable, so
        // the only test that can exist is one that proves the structure is never built.
        //
        // 3,000 is the measured crashing depth, used verbatim so this reproduces the original
        // scenario rather than an approximation of it. Reaching the assertions at all proves the
        // process survived; the assertions prove it was *refused* rather than merely tolerated.
        let deep = format!("RENVOR_{}", vec!["A"; 3000].join(NESTING_SEPARATOR));
        let error = read_environment::<Partial>("RENVOR_", &env(&[(deep.as_str(), "1")]))
            .expect_err("a key deeper than the ceiling must be refused");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert!(
            error.to_string().contains("32 levels"),
            "the ceiling must be named: {error}"
        );
    }

    #[test]
    fn a_key_at_the_ceiling_is_accepted() {
        // POSITIVE CONTROL: the bound discriminates rather than rejecting nesting on sight.
        // Without this, `MAX_KEY_DEPTH = 0` would satisfy the test above perfectly.
        //
        // `Partial` has no field this deep, so decoding is what would fail — which is the point:
        // reaching a *decode* error rather than a *depth* error proves the depth check passed.
        let at_ceiling = format!("RENVOR_{}", vec!["a"; 32].join(NESTING_SEPARATOR));
        let outcome = read_environment::<Partial>("RENVOR_", &env(&[(at_ceiling.as_str(), "1")]));
        let message = outcome
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(
            !message.contains("32 levels"),
            "a key exactly at the ceiling must not be refused for depth: {message}"
        );
    }

    #[test]
    fn an_empty_environment_produces_an_empty_layer() {
        // C-C11: a source present with 0 keys is not an error.
        let table =
            read_environment::<Partial>("RENVOR_", &BTreeMap::new()).expect("empty is valid");
        assert!(table.is_empty());
    }
}
