//! Step 1 of C-C2: decode **one** source against the declared schema, before any merging.
//!
//! # Narrowing, not path tracking
//!
//! When a source fails to decode, C-C3 requires the diagnostic to name the **key**, the **source
//! layer**, and the **expected type**. `serde`'s error carries the expected type and the offending
//! value, but deserializing a `toml::Table` gives no key path — the usual fix is the
//! `serde_path_to_error` crate.
//!
//! Renvor does it without the dependency, and gets a better shape for it. The fast path is a
//! single `deserialize` over the whole source. **Only when that fails** does [`locate_failure`]
//! run, bisecting the source one branch at a time until it finds the smallest sub-tree that still
//! fails. The cost is paid exactly once, on the path that is already about to return an error, and
//! it produces a dotted key path rather than a serde-internal one.
//!
//! # The expected type is reported inside the constraint, and that is a known limitation
//!
//! [`renvor_core::KernelError::Configuration`] carries `expected_type` as a `&'static str`,
//! deliberately: a `String` there could be made to carry a configuration *value*, which C-E3
//! forbids. The adapter has no schema description, so it cannot name the declared type of a
//! specific key as a static — it can only report what `serde` said, which always includes
//! `expected <type>`, and that lands in the constraint text.
//!
//! All three facts C-C3 requires are therefore in the message; two of them are in dedicated
//! fields and the third is in the constraint. This is recorded as an open item rather than
//! presented as a clean pass.

use renvor_core::KernelError;
use renvor_core::config_port::SourceLayer;
use serde::de::DeserializeOwned;
use toml::{Table, Value};

/// What the error message names in place of a per-key declared type.
///
/// See the module documentation: the precise expectation is in the constraint text, because the
/// adapter has no schema description to read a per-key type from.
const SCHEMA_EXPECTATION: &str = "a value matching the declared schema";

/// Decodes one source against `P`, the schema's all-optional form.
///
/// # Errors
///
/// Returns [`KernelError::Configuration`] naming the offending key path, the layer, and what was
/// expected. Nothing is merged, coerced, or dropped: a source that does not decode **fails**.
pub fn decode_source<P: DeserializeOwned>(
    table: &Table,
    layer: &SourceLayer,
) -> Result<(), KernelError> {
    match table.clone().try_into::<P>() {
        Ok(_) => Ok(()),
        Err(error) => {
            let (key, detail) = locate_failure::<P>(table, String::new(), &error);
            Err(KernelError::Configuration {
                key,
                constraint: detail,
                layer: layer.label().to_owned(),
                expected_type: SCHEMA_EXPECTATION,
            })
        }
    }
}

/// Decodes a **single** key path, for callers that already know which key they are testing.
///
/// The environment layer uses this: it knows each variable's key before it decodes anything, so
/// it never needs the bisection above.
///
/// # Errors
///
/// Returns [`KernelError::Configuration`] naming that key.
pub fn decode_single<P: DeserializeOwned>(
    key: &str,
    value: &Value,
    layer: &SourceLayer,
) -> Result<(), KernelError> {
    let table = nest(key, value.clone());
    table
        .try_into::<P>()
        .map(|_: P| ())
        .map_err(|error| KernelError::Configuration {
            key: key.to_owned(),
            constraint: error.message().to_owned(),
            layer: layer.label().to_owned(),
            expected_type: SCHEMA_EXPECTATION,
        })
}

/// Builds `{a: {b: {c: value}}}` from the dotted path `a.b.c`.
pub(crate) fn nest(key: &str, value: Value) -> Table {
    let mut segments: Vec<&str> = key.split('.').collect();
    let mut current = value;
    while let Some(segment) = segments.pop() {
        let mut table = Table::new();
        table.insert(segment.to_owned(), current);
        current = Value::Table(table);
    }
    match current {
        Value::Table(table) => table,
        // Unreachable: `split` always yields at least one segment, so the loop always runs at
        // least once and always produces a table. Handled rather than asserted, because SC-004
        // permits 0 panics and this is a diagnostic path.
        other => {
            let mut table = Table::new();
            table.insert(key.to_owned(), other);
            table
        }
    }
}

/// Finds the smallest failing sub-tree, returning its dotted path and `serde`'s message.
///
/// Called only after a whole-source decode has already failed, so the recursion never runs on a
/// healthy configuration.
fn locate_failure<P: DeserializeOwned>(
    table: &Table,
    prefix: String,
    fallback: &toml::de::Error,
) -> (String, String) {
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };

        let Err(error) = nest(&path, value.clone()).try_into::<P>() else {
            continue;
        };

        // This branch is the culprit. If it is a table, a narrower answer may exist inside it.
        if let Value::Table(inner) = value {
            let (deeper, detail) = locate_failure::<P>(inner, path.clone(), &error);
            if deeper != path {
                return (deeper, detail);
            }
        }
        return (path, error.message().to_owned());
    }

    // Every branch decoded on its own, so the failure is a property of the source as a whole —
    // a `deny_unknown_fields` rejection, for instance. Reported against the source itself rather
    // than blamed on an arbitrary key.
    (
        if prefix.is_empty() {
            "<source>".to_owned()
        } else {
            prefix
        },
        fallback.message().to_owned(),
    )
}

#[cfg(test)]
mod tests {
    use super::{decode_single, decode_source, nest};
    use renvor_core::ErrorCategory;
    use renvor_core::config_port::SourceLayer;
    use serde::Deserialize;
    use toml::{Table, Value};

    /// The partial forms exist to be **decoded into**, never read from: proving a source fits the
    /// schema is their whole job. `dead_code` fires because nothing reads the fields back, which is
    /// the intended shape rather than an oversight.
    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct Partial {
        host: Option<String>,
        port: Option<u16>,
        server: Option<PartialServer>,
    }

    #[allow(dead_code)]
    #[derive(Debug, Default, Deserialize)]
    struct PartialServer {
        threads: Option<u16>,
        name: Option<String>,
    }

    fn parse(text: &str) -> Table {
        text.parse::<Table>().expect("the fixture is valid TOML")
    }

    #[test]
    fn a_source_that_sets_only_some_keys_decodes() {
        // The whole reason the partial exists: a source is not required to be complete.
        let table = parse("host = \"localhost\"");
        decode_source::<Partial>(&table, &SourceLayer::File("base.toml".into()))
            .expect("a partial source is valid");
    }

    #[test]
    fn an_empty_source_decodes() {
        // C-C11: a source contributing 0 keys is not an error.
        decode_source::<Partial>(&Table::new(), &SourceLayer::Defaults).expect("empty is valid");
    }

    #[test]
    fn a_top_level_type_error_names_the_key_the_layer_and_the_expectation() {
        let table = parse("port = \"not-a-number\"");
        let error = decode_source::<Partial>(&table, &SourceLayer::File("base.toml".into()))
            .expect_err("a string cannot be a u16");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        let rendered = error.to_string();
        assert!(rendered.contains("port"), "key: {rendered}");
        assert!(rendered.contains("base.toml"), "layer: {rendered}");
        assert!(rendered.contains("u16"), "expected type: {rendered}");
    }

    #[test]
    fn a_nested_type_error_names_the_full_dotted_path() {
        // What the bisection buys: `server.threads`, not `server`.
        let table = parse("[server]\nthreads = \"many\"");
        let error = decode_source::<Partial>(&table, &SourceLayer::Environment)
            .expect_err("a string cannot be a u16");

        assert!(
            error.to_string().contains("server.threads"),
            "the narrowed path is missing: {error}"
        );
    }

    #[test]
    fn a_healthy_sibling_is_not_blamed_for_its_neighbour() {
        // POSITIVE CONTROL for the bisection: it must find the *failing* branch, not the first
        // branch it happens to look at.
        let table = parse("[server]\nname = \"web\"\nthreads = \"many\"");
        let error =
            decode_source::<Partial>(&table, &SourceLayer::Defaults).expect_err("threads is wrong");
        let rendered = error.to_string();
        assert!(rendered.contains("server.threads"), "{rendered}");
        assert!(
            !rendered.contains("server.name"),
            "the healthy sibling was blamed: {rendered}"
        );
    }

    #[test]
    fn a_single_key_decode_names_that_key() {
        let error = decode_single::<Partial>(
            "port",
            &Value::String("nope".into()),
            &SourceLayer::Environment,
        )
        .expect_err("a string cannot be a u16");
        assert!(error.to_string().contains("port"), "{error}");
        assert!(error.to_string().contains("environment"), "{error}");

        // POSITIVE CONTROL: the same call succeeds for a value of the declared type.
        decode_single::<Partial>("port", &Value::Integer(8080), &SourceLayer::Environment)
            .expect("8080 is a u16");
    }

    #[test]
    fn nesting_builds_the_path_it_was_given() {
        let table = nest("a.b.c", Value::Integer(1));
        let leaf = table["a"]["b"]["c"].as_integer();
        assert_eq!(leaf, Some(1), "{table:?}");

        // A single segment stays flat rather than gaining a level.
        assert_eq!(
            nest("solo", Value::Integer(2))["solo"].as_integer(),
            Some(2)
        );
    }
}
