//! The command surface as **expected output** (contract C-1, research D14).
//!
//! # Why the contract is asserted as text rather than in code
//!
//! C-1 makes `--help`'s structure part of the public contract: usage line, description, arguments,
//! options grouped consistently, and exit codes documented. Asserting that with
//! `assert!(help.contains("--dry-run"))` would pass while the help became unreadable, and would
//! make a contract change invisible in review.
//!
//! The files in `tests/cmd/` are the contract. A change to the surface appears as a **diff in those
//! files**, which a reviewer sees and has to agree to. That is the whole reason for the indirection.
//!
//! # `[EXE]` in the usage lines is not a wildcard
//!
//! Windows renders the usage line as `renvor.exe`. `[EXE]` is trycmd's own portability
//! placeholder: it expands to `.exe` there and to nothing elsewhere, so the expectation stays
//! **exact on every platform** rather than being loosened to `[..]`. Caught by the Windows matrix
//! after the files were seeded on macOS, which is the second time this session that the advisory
//! platform jobs found something the required ones did not.
//!
//! Regenerate deliberately with `TRYCMD=overwrite cargo test -p renvor-cli --test cli`, and read
//! the resulting diff before committing it — an auto-updated expectation that nobody looked at is
//! not a contract.

#[test]
fn the_command_surface_matches_its_recorded_contract() {
    trycmd::TestCases::new()
        .case("tests/cmd/*.trycmd")
        .default_bin_name("renvor");
}

// ── The JSON documents, as SHAPES (contract C-2, research D14) ───────────────────────────────
//
// `tests/cmd/*.trycmd` above records the command surface as byte-exact expected output. That is
// the right instrument for `--help` and for exit codes, and the wrong one for JSON: a document
// containing a temporary path, a byte count, and a toolchain version changes on every run and on
// every machine, so a byte-exact expectation would either be `[..]` everywhere — which asserts
// nothing — or would break constantly for reasons that are not contract changes.
//
// What C-2 actually fixes is the **shape**: `schemaVersion` is an integer, `status` is one of two
// strings, `result` and `error` are exclusive, and each error carries a stable `code` and a set of
// `details` keys. So the snapshot below keeps the contract-bearing values verbatim and replaces
// everything else with its TYPE. A renamed key, a removed field, a code that changed spelling, or a
// number that became a string all show up as a diff. A different temporary directory does not.

mod harness;

use harness::renvor;
use serde_json::{Value, json};

/// Values that are part of the contract and are therefore kept verbatim.
///
/// Everything else becomes its type name. `code` is here because C-2's registry is a closed set of
/// strings; `status` because it is `success` xor `failure`; `command` because a consumer routes on
/// it; `schemaVersion` because its whole job is to be compared.
const VERBATIM_KEYS: [&str; 4] = ["schemaVersion", "status", "command", "code"];

/// Replaces every non-contract scalar with the name of its type.
fn shape(value: &Value, key: Option<&str>) -> Value {
    if key.is_some_and(|key| VERBATIM_KEYS.contains(&key)) {
        return value.clone();
    }
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), shape(value, Some(key))))
                .collect(),
        ),
        // Arrays collapse to their element shape. The LENGTH is deliberately not asserted: the file
        // list of a generated project is a fact about the templates, not about the JSON contract,
        // and pinning it here would make every template change look like a schema change.
        Value::Array(items) => match items.first() {
            Some(first) => json!([shape(first, None)]),
            None => json!([]),
        },
        Value::String(_) => json!("<string>"),
        Value::Number(number) if number.is_i64() || number.is_u64() => json!("<integer>"),
        Value::Number(_) => json!("<number>"),
        Value::Bool(_) => json!("<boolean>"),
        Value::Null => Value::Null,
    }
}

/// Runs `renvor` and returns the shape of the single JSON document it produced.
///
/// Insists on **exactly one** document, which is C-2's other requirement and the one most often
/// missed: "one document for success and for failure alike. Not zero on failure."
fn document_shape(arguments: &[&str], directory: &std::path::Path) -> Value {
    let (_, stdout, stderr) = renvor(arguments, directory, &[]);
    let parsed: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!("{arguments:?} produced no parseable JSON document: {error}\nstdout: {stdout}\nstderr: {stderr}")
    });
    shape(&parsed, None)
}

#[test]
fn every_json_document_matches_its_recorded_shape() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let mut shapes = serde_json::Map::new();

    // ── Success documents ───────────────────────────────────────────────────────────
    shapes.insert(
        "new (dry run)".into(),
        document_shape(
            &["new", "dry", "--yes", "--dry-run", "--output", "json"],
            base.path(),
        ),
    );
    shapes.insert(
        "new".into(),
        document_shape(
            &[
                "new",
                "made",
                "--yes",
                "--example-domain",
                "--output",
                "json",
            ],
            base.path(),
        ),
    );
    shapes.insert(
        "new (container)".into(),
        document_shape(
            &["new", "boxed", "--yes", "--container", "--output", "json"],
            base.path(),
        ),
    );
    let project = base.path().join("made");
    let containerised = base.path().join("boxed");
    shapes.insert(
        "doctor".into(),
        document_shape(&["doctor", "--output", "json"], base.path()),
    );
    shapes.insert(
        "check".into(),
        document_shape(&["check", "--output", "json"], &project),
    );
    shapes.insert(
        "dev (dry run)".into(),
        document_shape(&["dev", "--dry-run", "--output", "json"], &project),
    );
    shapes.insert(
        "docker status (dry run)".into(),
        document_shape(
            &["docker", "status", "--dry-run", "--output", "json"],
            &containerised,
        ),
    );
    shapes.insert(
        "tls trust (dry run)".into(),
        document_shape(
            &["tls", "trust", "--dry-run", "--output", "json"],
            base.path(),
        ),
    );

    // ── Failure documents, one per error code reachable without a terminal ──────────
    shapes.insert(
        "usage (nothing to go on)".into(),
        document_shape(&["new", "--output", "json"], base.path()),
    );
    shapes.insert(
        "usage (malformed command line)".into(),
        document_shape(
            &["new", "demo", "--nonsense", "--output", "json"],
            base.path(),
        ),
    );
    shapes.insert(
        "unsupported_value".into(),
        document_shape(
            &[
                "new", "demo", "--target", "nope", "--yes", "--output", "json",
            ],
            base.path(),
        ),
    );
    shapes.insert(
        "unsupported_combination".into(),
        document_shape(
            &["new", "demo", "--seed-data", "--yes", "--output", "json"],
            base.path(),
        ),
    );
    shapes.insert(
        "reserved_for_later_phase".into(),
        document_shape(
            &[
                "new",
                "demo",
                "--database",
                "postgres",
                "--yes",
                "--output",
                "json",
            ],
            base.path(),
        ),
    );
    shapes.insert(
        "invalid_project_name".into(),
        document_shape(&["new", "CON", "--yes", "--output", "json"], base.path()),
    );
    shapes.insert(
        "destination_rejected".into(),
        document_shape(
            &[
                "new",
                "demo",
                "--path",
                "../escape",
                "--yes",
                "--output",
                "json",
            ],
            base.path(),
        ),
    );
    shapes.insert(
        "destination_not_empty".into(),
        document_shape(&["new", "made", "--yes", "--output", "json"], base.path()),
    );
    shapes.insert(
        "manifest_invalid".into(),
        document_shape(&["check", "--output", "json"], base.path()),
    );
    shapes.insert(
        "tls consent required".into(),
        document_shape(&["tls", "trust", "--output", "json"], base.path()),
    );
    shapes.insert(
        "tls consent given, operation unavailable".into(),
        document_shape(
            &[
                "tls",
                "trust",
                "--i-understand-this-modifies-my-system-trust-store",
                "--output",
                "json",
            ],
            base.path(),
        ),
    );

    insta::assert_json_snapshot!("json-document-shapes", Value::Object(shapes));
}

#[test]
fn the_shape_function_can_tell_two_different_shapes_apart() {
    // The control. `shape` erases values on purpose, and an over-eager version that erased KEYS
    // too — or that returned a constant — would make the snapshot above pass through any schema
    // change at all, which is the one thing it exists to catch.
    let original =
        json!({"schemaVersion": 1, "status": "success", "result": {"files": 7, "path": "/tmp/x"}});

    // A different VALUE is the same shape. This is the property that makes the snapshot stable.
    let same =
        json!({"schemaVersion": 1, "status": "success", "result": {"files": 9, "path": "/other"}});
    assert_eq!(
        shape(&original, None),
        shape(&same, None),
        "a value change must not read as a schema change"
    );

    // A renamed key is a different shape.
    let renamed = json!({"schemaVersion": 1, "status": "success", "result": {"fileCount": 7, "path": "/tmp/x"}});
    assert_ne!(
        shape(&original, None),
        shape(&renamed, None),
        "a renamed key must be visible"
    );

    // A type change is a different shape.
    let retyped = json!({"schemaVersion": 1, "status": "success", "result": {"files": "7", "path": "/tmp/x"}});
    assert_ne!(
        shape(&original, None),
        shape(&retyped, None),
        "an integer becoming a string must be visible"
    );

    // A changed contract-bearing VALUE is a different shape, because those are kept verbatim.
    let recoded =
        json!({"schemaVersion": 2, "status": "success", "result": {"files": 7, "path": "/tmp/x"}});
    assert_ne!(
        shape(&original, None),
        shape(&recoded, None),
        "a schemaVersion bump must be visible"
    );
    let refailed =
        json!({"schemaVersion": 1, "status": "failure", "result": {"files": 7, "path": "/tmp/x"}});
    assert_ne!(
        shape(&original, None),
        shape(&refailed, None),
        "a status change must be visible"
    );
}
