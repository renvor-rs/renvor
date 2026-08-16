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
//! single `deserialize` over the whole source. **Only when that fails** does a bisection
//! run, narrowing the source one branch at a time until it finds the smallest sub-tree that still
//! fails. The cost is paid exactly once, on the path that is already about to return an error, and
//! it produces a dotted key path rather than a serde-internal one.
//!
//! # The decoder's message is stripped before it becomes a constraint
//!
//! **`serde` quotes the offending value in its message**, measured rather than assumed:
//! deserializing `port = "hunter2"` into a `u16` reports `invalid type: string "hunter2", expected
//! u16`. Forwarding that into an error puts a possibly-secret configuration value into every
//! output form, which C-E3 allows **0** of.
//!
//! So no error here is built by hand. Every one goes through
//! [`renvor_core::error::context::Constraint::from_decoder`], which keeps the expectation and the
//! shape and discards everything the decoder may have quoted. The `Configuration` variant is
//! `#[non_exhaustive]`, so this is not a convention — building the error any other way does not
//! compile from outside `renvor-core`.
//!
//! The expected type still reaches the message through the constraint rather than through the
//! dedicated `expected_type` field, because the adapter has no schema description to read a
//! per-key type from. That remains a recorded open item.

use renvor_core::KernelError;
use renvor_core::config_port::SourceLayer;
use renvor_core::error::context::{Constraint, configuration};
use serde::de::DeserializeOwned;
use toml::{Table, Value};

use super::env::MAX_KEY_DEPTH;

/// What the error message names in place of a per-key declared type.
///
/// See the module documentation: the precise expectation is in the constraint text, because the
/// adapter has no schema description to read a per-key type from.
const SCHEMA_EXPECTATION: &str = "a value matching the declared schema";

/// The structural shape of a TOML value, for a constraint that must not name its contents.
const fn shape_of(value: &Value) -> &'static str {
    match value {
        Value::Table(_) => "a table",
        Value::Array(_) => "an array",
        Value::String(_) => "a string",
        Value::Integer(_) => "an integer",
        Value::Float(_) => "a float",
        Value::Boolean(_) => "a boolean",
        Value::Datetime(_) => "a datetime",
    }
}

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
            let (key, constraint) = locate_failure::<P>(table, String::new(), &error);
            Err(configuration(
                key,
                layer.label(),
                SCHEMA_EXPECTATION,
                &constraint,
            ))
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
    // W-005 security re-review SV-N1. The depth ceiling that closed the original 3.1 finding was
    // placed in `read_environment`, one layer above — so it guarded the SHIPPED path and left
    // this one, which is `pub` and documented, reaching the same structures unguarded. Called
    // directly with a 3000-segment key it aborted the process: `fatal runtime error: stack
    // overflow`, exit 134, reproduced before this check was written.
    //
    // The guard belongs at the public boundary rather than in `nest`, because `nest` is not where
    // the recursion is. `nest` is a `pop` loop and is iterative; the depth is consumed by
    // `try_into`'s recursive descent and again by the nested table's recursive `Drop`. Bounding
    // the constructor would have protected nothing.
    let depth = key.split('.').count();
    if depth > MAX_KEY_DEPTH {
        return Err(configuration(
            key,
            layer.label(),
            "a key nested no deeper than the ceiling",
            &Constraint::Rule(
                "the key is nested deeper than 32 levels; decoding and then dropping a structure \
                 that deep recurses far enough to overflow the stack, which no Rust guard can \
                 catch",
            ),
        ));
    }

    let table = nest(key, value.clone());
    table.try_into::<P>().map(|_: P| ()).map_err(|error| {
        configuration(
            key,
            layer.label(),
            SCHEMA_EXPECTATION,
            &Constraint::from_decoder(error.message(), shape_of(value)),
        )
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
) -> (String, Constraint) {
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
        return (
            path,
            Constraint::from_decoder(error.message(), shape_of(value)),
        );
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
        Constraint::from_decoder(fallback.message(), "a table"),
    )
}

#[cfg(test)]
mod tests {
    use super::{MAX_KEY_DEPTH, decode_single, decode_source, nest};
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
    fn a_deeply_nested_key_is_refused_here_too_rather_than_overflowing_the_stack() {
        // W-005 security re-review SV-N1, as a regression test.
        //
        // Finding 3.1's ceiling was placed in `read_environment`, so it guarded the shipped path
        // and left THIS one — `decode_single` is `pub`, in a `pub mod`, and documented. Called
        // directly with the same 3,000-segment key it aborted the process: `fatal runtime error:
        // stack overflow`, exit 134. Measured against the public API from outside the crate
        // before this check was written.
        //
        // 3,000 is the measured crashing depth, used verbatim rather than rounded, so this
        // reproduces the original scenario. An abort is not catchable, so reaching the assertions
        // at all is what proves the process survived; the assertions prove it was *refused*.
        let deep = vec!["a"; 3000].join(".");
        let error =
            decode_single::<Partial>(&deep, &Value::String("1".into()), &SourceLayer::Environment)
                .expect_err("a key deeper than the ceiling must be refused");
        assert!(
            error.to_string().contains("32 levels"),
            "the ceiling must be named: {error}"
        );

        // POSITIVE CONTROL: the bound discriminates rather than refusing nesting on sight. A key
        // exactly AT the ceiling must get past the depth check and be judged on its type — so a
        // decode error here, and not a depth error, is what proves the check let it through.
        let at_ceiling = vec!["a"; MAX_KEY_DEPTH].join(".");
        let message = decode_single::<Partial>(
            &at_ceiling,
            &Value::String("1".into()),
            &SourceLayer::Environment,
        )
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
        assert!(
            !message.contains("32 levels"),
            "a key at the ceiling was refused by the depth check: {message}"
        );
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
