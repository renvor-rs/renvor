//! No assertion in a credential-handling file may print what it is testing.
//!
//! # Why this gate exists
//!
//! A redaction test's failure message is the one place a credential is *most* likely to be
//! printed, because the obvious way to write the assertion puts it there:
//!
//! ```ignore
//! assert!(!rendered.contains(CREDENTIAL), "{rendered}");   // prints the leak
//! assert_eq!(secret().to_string(), REDACTED);              // prints both operands
//! assert!(!format!("{password}").contains(NEEDLE));        // prints the expression source
//! ```
//!
//! All three are silent on a pass. All three publish the credential on exactly the run that
//! proves a leak exists — which is the run whose output somebody reads, pastes into an issue, and
//! ships to a CI log. **The one run that matters is the one that leaks.**
//!
//! # Why it is a gate rather than a fix
//!
//! Because the fix was applied twice and was incomplete both times.
//!
//! The W-005 security review named `ResolvedConfig<T>`; four sibling types held the same data and
//! kept printing it. The security *delta* review then named three files whose diagnostics leaked;
//! fixing those three left **24 sites of the identical defect in five other files**, proven live
//! by breaking `Secret`'s `Display` and watching the credential appear in the failure output.
//!
//! Fixing the instances somebody pointed at is not the same as establishing the property. This
//! test establishes the property, over a set it **discovers** rather than a list anyone maintains.
//!
//! # What is checked, and what is deliberately not
//!
//! A file is in scope if it mentions a credential needle — that is what makes its diagnostics
//! dangerous. Within those files, every `assert*!`/`panic!`/`expect` diagnostic must be a fixed
//! string, or interpolate only an **allowlisted** name: a label, an index, a count. The allowlist
//! is deliberately the fail-closed direction. A denylist of "rendering-ish" names would let the
//! next binding nobody thought of through, which is precisely how this recurred.

use std::path::{Path, PathBuf};

/// Substrings that mark a file as handling a synthetic credential.
///
/// Assembled from fragments so this file does not match its own list — the trap every
/// self-reading test sets for itself, and one this project has already sprung twice.
fn credential_needles() -> Vec<String> {
    vec![
        format!("{}{}", "hunt", "er2"),
        format!("{}{}", "s3cr3t", "-token"),
        format!("{}{}", "LEAKED", "-TAIL"),
        format!("{}{}", "do-not", "-print"),
        // THE NAME OF THE SHARED TEST CONSTANT, not only the value behind it. Every needle above
        // is a literal, so a file was in scope only if it *inlined* a canary — and the persistence
        // suites do not: they reference the constant that `tests/support` exports. Four files that
        // plant a password and render the refusal were therefore invisible to this gate, one of
        // them printing the secret itself on failure. Found in Phase 007 by widening the needle
        // set and reading what appeared; the offences it exposed are fixed in the same change.
        //
        // SPLIT, like the others, because a needle written whole would put this file into its own
        // scope — where the synthetic controls below, which interpolate on purpose, are offences.
        format!("{}{}", "CREDENTIAL", "_CANARY"),
    ]
}

/// Interpolations permitted inside a diagnostic: labels, indices, and counts.
///
/// Not a rendering among them. An allowlist rather than a denylist because the failure mode being
/// prevented is *a name nobody anticipated*, and only an allowlist fails closed on one.
const PERMITTED: &[&str] = &[
    "name",
    "route",
    "variant",
    "index",
    // A loop counter under another name. Admitted rather than renaming the variable at each call
    // site: `round` is a count of iterations and can carry nothing else.
    "round",
    "payload_index",
    "needle_index",
    "label",
    "phase",
    "position",
    "count",
    "levels",
    "depth",
    "expected",
    "minimum",
    "maximum",
];

/// Every `.rs` file under the workspace's crates, as `(relative path, source)`.
fn workspace_sources() -> Vec<(String, String)> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate directory has a parent")
        .to_path_buf();

    let mut found = Vec::new();
    let mut pending = vec![crates.clone()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path: PathBuf = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "target") {
                    continue;
                }
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "rs")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                let relative = path
                    .strip_prefix(&crates)
                    .map(|rest| rest.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                found.push((relative, text));
            }
        }
    }
    found.sort();
    found
}

/// Diagnostics in `source` that interpolate something other than an allowlisted name.
///
/// Returns `(line number, the offending line)`. A pure function of the text, so the controls
/// below can feed it synthetic input rather than planting files in the repository.
fn offending_diagnostics(source: &str) -> Vec<(usize, String)> {
    let mut offences = Vec::new();
    let mut depth: i32 = 0;
    let mut inside = false;

    for (number, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        // Comments describe diagnostics; they are not diagnostics.
        if trimmed.starts_with("//") {
            continue;
        }

        if !inside
            && (trimmed.contains("assert!(")
                || trimmed.contains("assert_eq!(")
                || trimmed.contains("assert_ne!(")
                || trimmed.contains("panic!("))
        {
            inside = true;
            depth = 0;
        }
        if !inside {
            continue;
        }

        for fragment in message_literals(line) {
            for binding in interpolations(&fragment) {
                if !PERMITTED.contains(&binding.as_str()) {
                    offences.push((number + 1, line.trim().to_owned()));
                }
            }
        }

        depth += i32::try_from(line.matches('(').count()).unwrap_or(0);
        depth -= i32::try_from(line.matches(')').count()).unwrap_or(0);
        if depth <= 0 {
            inside = false;
        }
    }
    offences
}

/// The string literals on one line that are in **message** position.
///
/// A literal directly following `format!(`, `write!(`, `writeln!(`, `print!(` or `println!(` is
/// building the *subject* of the assertion, not reporting it — `assert!(!format!("{secret:?}")
/// .contains(CREDENTIAL), "…")` renders the secret on purpose, in order to check it. Flagging
/// those would make the gate unsatisfiable for exactly the tests it exists to protect, and an
/// unsatisfiable gate gets deleted rather than obeyed.
///
/// Escapes are not interpreted: a diagnostic needing an escaped brace is not a shape this project
/// writes.
fn message_literals(line: &str) -> Vec<String> {
    const SUBJECT_MACROS: &[&str] = &["format!(", "write!(", "writeln!(", "print!(", "println!("];

    let mut literals = Vec::new();
    let mut current = String::new();
    let mut open = false;
    let mut previous = '\0';
    let mut start = 0usize;
    for (offset, character) in line.char_indices() {
        if character == '"' && previous != '\\' {
            if open {
                let preceding = &line[..start];
                let is_subject = SUBJECT_MACROS
                    .iter()
                    .any(|macro_name| preceding.trim_end().ends_with(macro_name));
                if !is_subject {
                    literals.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            } else {
                start = offset;
            }
            open = !open;
        } else if open {
            current.push(character);
        }
        previous = character;
    }
    literals
}

/// The interpolated binding names in a format string: `{name}` and `{name:?}` alike.
///
/// `{}` and `{0}` are ignored: a positional argument is an expression at the call site, and this
/// gate reads names rather than evaluating expressions. The project writes named captures, which
/// is what makes the check tractable.
fn interpolations(literal: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = literal;
    while let Some(start) = rest.find('{') {
        rest = &rest[start + 1..];
        if rest.starts_with('{') {
            rest = &rest[1..];
            continue;
        }
        let Some(end) = rest.find('}') else { break };
        let inner = &rest[..end];
        let name = inner.split(':').next().unwrap_or("");
        if name.is_empty() {
            // A POSITIONAL argument: `{}` or `{:?}`. Found by the round-four security delta review
            // (S4-5), which noticed that this batch introduced the first ones into a
            // credential-handling file and that the gate was blind to them.
            //
            // A positional slot consumes an expression from the argument list, and this gate reads
            // *text*: it cannot see what that expression is, so it cannot tell `rendered.len()`
            // from `rendered`. An allowlist that fails closed on a name it does not recognise must
            // also fail closed on an argument it cannot name at all — otherwise the way to evade
            // it is to delete the identifier, which is one keystroke.
            //
            // Reported under a name that can never appear in PERMITTED, so it is always an
            // offence.
            names.push("<positional argument>".to_owned());
        } else if name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            && !name.chars().next().is_some_and(|c| c.is_ascii_digit())
        {
            names.push(name.to_owned());
        }
        rest = &rest[end + 1..];
    }
    names
}

#[test]
fn no_credential_handling_file_prints_what_it_asserts_about() {
    let needles = credential_needles();
    let sources = workspace_sources();

    // POSITIVE CONTROL 1: the walk found the workspace. A walk returning nothing would make every
    // assertion below vacuously true — the exact failure this whole gate is modelled on.
    assert!(
        sources.len() > 30,
        "the source walk found only {} files; it is not reading the workspace",
        sources.len()
    );

    let in_scope: Vec<_> = sources
        .iter()
        .filter(|(_, text)| needles.iter().any(|needle| text.contains(needle.as_str())))
        .collect();

    // POSITIVE CONTROL 2: the needles match. If none did, the scope would be empty and this test
    // would pass while checking nothing.
    assert!(
        in_scope.len() >= 8,
        "only {} files were found to handle a credential; the needles have gone stale",
        in_scope.len()
    );

    // POSITIVE CONTROL 4: the scope reaches a file that names the shared constant WITHOUT inlining
    // a canary literal. A count floor cannot guard this — 21 files matched the literals alone,
    // well clear of the floor above — so deleting the constant needle would silently shrink the
    // scope by the four files it was added for and every assertion here would still pass.
    let constant = format!("{}{}", "CREDENTIAL", "_CANARY");
    let literals = &needles[..4];
    let by_constant_only = in_scope
        .iter()
        .filter(|(_, text)| {
            text.contains(constant.as_str())
                && !literals.iter().any(|needle| text.contains(needle.as_str()))
        })
        .count();
    assert!(
        by_constant_only >= 3,
        "only {by_constant_only} files reach this gate through the shared constant alone; the \
         needle that puts them in scope has gone stale, and they plant passwords"
    );

    let mut failures = Vec::new();
    for (path, text) in &in_scope {
        for (number, line) in offending_diagnostics(text) {
            failures.push(format!("  {path}:{number}\n    {line}"));
        }
    }

    assert!(
        failures.is_empty(),
        "a diagnostic in a credential-handling file interpolates a rendering. On a redaction \
         regression this prints the credential into the test log — the one run where that matters \
         most. Use a fixed message naming which check failed, or an index identifying which case.\
         \n{}",
        failures.join("\n")
    );
}

#[test]
fn the_check_detects_the_shapes_it_exists_to_prevent() {
    // POSITIVE CONTROL 3, and the one that makes the test above mean something. Every shape here
    // was live in this workspace and was found by a reviewer rather than by a check.
    let offending = [
        r#"assert!(!rendered.contains(CREDENTIAL), "{rendered}");"#,
        r#"assert!(rendered.contains(REDACTED), "the value was: {rendered}");"#,
        r#"assert!(x, "{debug_output}");"#,
        r#"panic!("leaked: {output}");"#,
        "assert!(\n    ok,\n    \"a value reached the error: {described}\"\n);",
        // POSITIONAL arguments (S4-5). The gate was blind to these until T159: `interpolations`
        // required a non-empty name, so `{}` and `{:?}` produced nothing and the whole diagnostic
        // read as clean. A text scan cannot see which expression fills the slot, so it cannot tell
        // a length from the rendering itself — and deleting the identifier was a one-keystroke
        // evasion of an allowlist built to fail closed.
        r#"assert!(ok, "the value was {}", rendered);"#,
        r#"assert!(ok, "the value was {:?}", secret);"#,
        r#"panic!("leaked: {}", output);"#,
    ];
    for source in offending {
        assert!(
            !offending_diagnostics(source).is_empty(),
            "the check missed a leaking diagnostic: {source}"
        );
    }

    // NEGATIVE CONTROL: it must not fire on the forms the fix produced, or the gate would be
    // unsatisfiable and would be turned off rather than obeyed.
    let permitted = [
        r#"assert!(!rendered.contains(CREDENTIAL), "the credential was rendered");"#,
        r#"assert!(ok, "field route {route} leaked the value");"#,
        r#"assert!(ok, "the credential reached the `{name}` path");"#,
        r#"let rendered = format!("{secret:?}");"#,
        r#"// assert!(x, "{rendered}");"#,
        r#"println!("env  ok: payload {payload_index}");"#,
        // Subject expressions, not diagnostics: these RENDER the secret deliberately, to check
        // it. Flagging them would make the gate unsatisfiable for the tests it protects.
        r#"assert!(format!("{secret:?}").contains("database.password"), "the key is missing");"#,
        r#"assert!(!format!("{password}{password:?}").contains(&needle), "it leaked");"#,
    ];
    for source in permitted {
        assert!(
            offending_diagnostics(source).is_empty(),
            "the check fired on a permitted form: {source}"
        );
    }
}
