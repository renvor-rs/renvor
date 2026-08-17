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

/// The deepest **value** this module will decode, drop, or narrow.
///
/// W-005 security delta findings S2-1 and S2-2. [`MAX_KEY_DEPTH`] bounds the *key*; the recursion
/// runs over the key **and** the value, and nothing bounded the value. A shallow key with a
/// 1,575-deep value aborted the process — `fatal runtime error: stack overflow`, exit 134 — and
/// [`decode_source`] had no depth check of any kind.
///
/// **128 is chosen from two measurements, not from taste.** `toml_parser` refuses its own nesting
/// at **81** (measured: 80 parses, 81 is refused), so no document read from a file and no value
/// decoded from an environment variable can reach this ceiling — nothing that works today starts
/// failing. And 128 is more than an order of magnitude below the ~1,575 at which the descent
/// actually overflows. The gap between those two numbers is the whole reason a caller-constructed
/// `toml::Table` was dangerous: it skips the parser that was doing the bounding.
///
/// Recorded with the phase's other Renvor-chosen bounds as a named open item.
pub const MAX_VALUE_DEPTH: usize = 128;

/// Whether `value` nests deeper than `ceiling`.
///
/// **Iterative on purpose.** A recursive measurement would overflow the stack on exactly the input
/// it exists to refuse, which is the trap the thing being fixed here fell into. This walks an
/// explicit stack on the heap.
///
/// It abandons as soon as the ceiling is passed. A caller only needs to know *whether* the value
/// is too deep, and a 50,000-deep array should not cost a full traversal to reject.
fn exceeds_depth(value: &Value, ceiling: usize) -> bool {
    let mut pending = vec![(value, 1usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > ceiling {
            return true;
        }
        match current {
            Value::Table(table) => pending.extend(table.values().map(|item| (item, depth + 1))),
            Value::Array(items) => pending.extend(items.iter().map(|item| (item, depth + 1))),
            _ => {}
        }
    }
    false
}

/// The diagnostic for a value that nests past [`MAX_VALUE_DEPTH`].
fn too_deep(key: &str, layer: &SourceLayer) -> KernelError {
    configuration(
        key,
        layer.label(),
        "a value nested no deeper than the ceiling",
        &Constraint::TooDeep {
            maximum_depth: MAX_VALUE_DEPTH,
        },
    )
}

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
    // S2-2. This function had **no depth check of any kind**, and it is what
    // `LayeredResolver::resolve()` calls — so the shipped resolver aborted at depth 1,575, exit
    // 134, entirely through public API. `toml_parser` refuses its own nesting at 81, so nothing
    // read from a file or an environment variable could reach it; a caller who builds a
    // `toml::Table` in Rust and hands it to `with_defaults` skips that parser entirely, and
    // `with_defaults` takes a `Table`.
    //
    // Checked before `try_into`'s descent and before `locate_failure`'s narrowing, both of which
    // recurse, and before the `clone` on the next line, which recurses too.
    for (key, value) in table {
        if exceeds_depth(value, MAX_VALUE_DEPTH) {
            return Err(too_deep(key, layer));
        }
    }

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
    if key.split('.').count() > MAX_KEY_DEPTH {
        return Err(configuration(
            key,
            layer.label(),
            "a key nested no deeper than the ceiling",
            &Constraint::TooDeep {
                maximum_depth: MAX_KEY_DEPTH,
            },
        ));
    }

    // S2-1. The guard above counts the KEY. The recursion runs over the key **and** the value, and
    // for one revision only the key was bounded — so `decode_single("a", <1575-deep value>, …)`
    // still aborted, exit 134, through the same public function the key guard had just been added
    // to protect. Checked before `nest` clones the value, because cloning a structure that deep is
    // itself a recursive descent.
    if exceeds_depth(value, MAX_VALUE_DEPTH) {
        return Err(too_deep(key, layer));
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
    use super::{MAX_KEY_DEPTH, MAX_VALUE_DEPTH, decode_single, decode_source, nest};
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
        assert!(rendered.contains("port"), "the key is missing");
        assert!(rendered.contains("base.toml"), "the layer is missing");
        assert!(rendered.contains("u16"), "the expected type is missing");
    }

    #[test]
    fn a_nested_type_error_names_the_full_dotted_path() {
        // What the bisection buys: `server.threads`, not `server`.
        let table = parse("[server]\nthreads = \"many\"");
        let error = decode_source::<Partial>(&table, &SourceLayer::Environment)
            .expect_err("a string cannot be a u16");

        assert!(
            error.to_string().contains("server.threads"),
            "the narrowed path is missing"
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
        assert!(
            rendered.contains("server.threads"),
            "the narrowed path is missing"
        );
        assert!(
            !rendered.contains("server.name"),
            "the healthy sibling was blamed"
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
        assert!(error.to_string().contains("port"), "the key is missing");
        assert!(
            error.to_string().contains("environment"),
            "the layer is missing"
        );

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
        // The expected number is READ FROM THE CONSTANT, not written as a literal. Asserting
        // `"32 levels"` is how the message and the ceiling were able to disagree (S4-1): the
        // literal in the test agreed with the literal in the message, and neither agreed with
        // `MAX_KEY_DEPTH`. Now setting the constant to 8 fails this test, which is the point.
        assert!(
            error
                .to_string()
                .contains(&format!("{MAX_KEY_DEPTH}-level ceiling")),
            "the ceiling must be named, and must be the one the code enforces"
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
            !message.contains("-level ceiling"),
            "a key at the ceiling was refused by the depth check"
        );
    }

    /// Builds a `levels`-deep value **iteratively**, so the fixture cannot overflow while
    /// constructing the input its subject is supposed to refuse.
    fn deep_value(levels: usize) -> Value {
        let mut value = Value::String("x".into());
        for _ in 1..levels {
            value = Value::Array(vec![value]);
        }
        value
    }

    #[test]
    fn a_deeply_nested_value_is_refused_by_both_entry_points() {
        // W-005 security delta findings S2-1 and S2-2, as a regression test.
        //
        // The key ceiling closed one half. The recursion runs over the key AND the value, so a
        // shallow key with a 1,575-deep value still aborted — exit 134 — and `decode_source` had
        // no depth check at all, which put the abort behind `LayeredResolver::resolve()`.
        //
        // 1,575 is the measured aborting depth, used verbatim. Reaching the assertions proves the
        // process survived; the assertions prove it was refused *for depth* rather than happening
        // to fail on the type.
        for levels in [1575, MAX_VALUE_DEPTH + 1] {
            let value = deep_value(levels);

            let single = decode_single::<Partial>("port", &value, &SourceLayer::Environment)
                .expect_err(
                    "a value deeper than the ceiling must be refused by the single-key path",
                );
            assert!(
                single
                    .to_string()
                    .contains(&format!("{MAX_VALUE_DEPTH}-level ceiling")),
                "the single-key path did not name the enforced ceiling"
            );

            let mut table = Table::new();
            table.insert("port".to_owned(), deep_value(levels));
            let source = decode_source::<Partial>(&table, &SourceLayer::Defaults)
                .expect_err("a value deeper than the ceiling must be refused by the source path");
            assert!(
                source
                    .to_string()
                    .contains(&format!("{MAX_VALUE_DEPTH}-level ceiling")),
                "the source path did not name the enforced ceiling"
            );
        }

        // POSITIVE CONTROL: the bound discriminates rather than refusing nesting on sight. A value
        // exactly AT the ceiling must get *past* the depth check — so an error that does NOT name
        // the ceiling is what proves the check let it through, and an ordinary shallow value must
        // decode cleanly.
        let at_ceiling = decode_single::<Partial>(
            "port",
            &deep_value(MAX_VALUE_DEPTH),
            &SourceLayer::Environment,
        )
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
        assert!(
            !at_ceiling.contains("-level ceiling"),
            "a value at the ceiling was refused by the depth check"
        );

        decode_single::<Partial>("port", &Value::Integer(8080), &SourceLayer::Environment)
            .expect("an ordinary value still decodes");
    }

    #[test]
    fn nesting_builds_the_path_it_was_given() {
        let table = nest("a.b.c", Value::Integer(1));
        let leaf = table["a"]["b"]["c"].as_integer();
        assert!(leaf == Some(1), "the nested path does not end at the value");

        // A single segment stays flat rather than gaining a level.
        assert_eq!(
            nest("solo", Value::Integer(2))["solo"].as_integer(),
            Some(2)
        );
    }
}
