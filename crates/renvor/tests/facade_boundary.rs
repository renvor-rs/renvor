//! The **facade-root** public API boundary, asserted mechanically.
//!
//! # What this test is for
//!
//! `renvor`'s crate root re-exports the names an application actually reaches for. None of them
//! may expose a third-party transport type — `axum`, `tower`, `hyper` — in a public callable
//! signature. The facade's own rustdoc states the reason: re-exporting a third-party type puts it
//! in Renvor's public API and makes every upstream major version a Renvor breaking change.
//!
//! Until 2026-08-23 that statement was **false in the shipped surface**. `Server` was re-exported
//! at the facade root, and `Server::serve` takes the underlying router **by parameter**. One
//! transport type was therefore reachable through the front door of the public API — and the
//! existing transport scan could not see it, because that scan lives in `renvor-http` and exempts
//! `server.rs` as a module that *is* the transport. The exemption was correct for that scan and
//! blind for this one. This file is the scan that was missing.
//!
//! # Why the root and the `transport` module are judged differently
//!
//! `renvor::transport` **is** `renvor_http`. Naming transport types there is the point. The
//! boundary is not "no transport type anywhere"; it is "not in the names promoted to the root,
//! where an application is expected to reach without knowing which transport it got".
//!
//! # Why it reads signatures rather than the type system
//!
//! There is no bound that expresses "this signature names nothing from these three crates". The
//! type system can express what is allowed only by enumerating it, which is a list that goes
//! stale. Reading the declared signature catches the case the compiler is happy with.

use std::path::{Path, PathBuf};

/// The transport crate names that must not appear in a facade-root public signature.
const TRANSPORT_IDENTIFIERS: [&str; 5] = ["axum", "tower_http", "tower::", "hyper", "Router"];

fn facade_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/lib.rs")
}

fn http_source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../renvor-http/src")
}

/// Every `.rs` file under `root`, as (path-relative-to-root, contents).
fn rust_files(root: &Path) -> Vec<(String, String)> {
    fn walk(directory: &Path, base: &Path, found: &mut Vec<(String, String)>) {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("`{}` is readable: {error}", directory.display()));

        for entry in entries {
            let entry = entry.expect("a directory entry");
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let relative = path
                    .strip_prefix(base)
                    .expect("every path is under the base")
                    .to_string_lossy()
                    .replace('\\', "/");
                let contents = std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("`{relative}` is readable: {error}"));
                found.push((relative, contents));
            }
        }
    }

    let mut found = Vec::new();
    walk(root, root, &mut found);
    found.sort();
    assert!(!found.is_empty(), "the scan found no source files at all");
    found
}

/// The names re-exported at the facade **root** from `renvor_http`.
///
/// Parsed from the source rather than hand-listed, so adding a name to the re-export brings it
/// under this guard automatically. A hand-list is how the `renvor-http` scan silently shrank.
fn facade_root_names() -> Vec<String> {
    let source = std::fs::read_to_string(facade_source()).expect("the facade source is readable");

    // The root re-export, not `pub use renvor_http as transport;`.
    let marker = "pub use renvor_http::{";
    let start = source
        .find(marker)
        .expect("the facade root re-exports names from renvor_http");
    let rest = &source[start + marker.len()..];
    let end = rest.find("};").expect("the re-export block is terminated");

    let names: Vec<String> = rest[..end]
        .split(',')
        .map(|name| name.trim().trim_end_matches('}').trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect();

    assert!(
        names.len() > 5,
        "the facade root re-export parsed as {names:?}, which is too short to be the real list — \
         the parser has drifted from the source and would pass while checking nothing"
    );
    names
}

/// Strips `//`-comments so a transport name discussed in prose is not read as a signature.
fn without_comments(line: &str) -> &str {
    line.find("//").map_or(line, |at| &line[..at])
}

/// Every `pub fn` / `pub async fn` signature declared in an `impl` block for `type_name`,
/// plus the type's own declaration line.
///
/// Signatures span lines, so each is collected from `pub fn` up to the `{` that opens the body or
/// the `;` that ends a trait declaration.
fn public_signatures_of(type_name: &str) -> Vec<(String, String)> {
    let mut signatures = Vec::new();

    for (path, contents) in rust_files(&http_source_root()) {
        // BYTE offsets throughout. An earlier version computed the offset from `line.len()` (bytes)
        // and then indexed a `Vec<char>`, so every file containing a non-ASCII character — this
        // codebase is full of em-dashes — sliced at the wrong place and found nothing. The positive
        // control below is what caught it.
        let bytes = contents.as_bytes();
        let lines: Vec<&str> = contents.lines().collect();

        for (index, line) in lines.iter().enumerate() {
            let code = without_comments(line);
            let trimmed = code.trim_start();

            // An impl block FOR this type: `impl Name {`, `impl<T> Name {`, `impl Trait for Name {`.
            let opens_impl = trimmed.starts_with("impl")
                && (trimmed.contains(&format!(" {type_name} "))
                    || trimmed.contains(&format!(" {type_name}<"))
                    || trimmed.contains(&format!(" {type_name}{{"))
                    || trimmed.trim_end().ends_with(&format!(" {type_name} {{"))
                    || trimmed.trim_end().ends_with(&format!("for {type_name} {{")));

            if !opens_impl {
                continue;
            }

            // Walk the block by brace depth from this line onward.
            let block_start: usize = lines[..index].iter().map(|l| l.len() + 1).sum();
            let mut depth = 0i32;
            let mut position = block_start;
            let mut seen_open = false;

            while position < bytes.len() {
                match bytes[position] {
                    b'{' => {
                        depth += 1;
                        seen_open = true;
                    }
                    b'}' => {
                        depth -= 1;
                        if seen_open && depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                position += 1;
            }

            let block = &contents[block_start..position.min(bytes.len())];

            let mut remaining = block;
            // Whichever comes FIRST. An earlier version used `find("pub fn ").or_else(…)`, which
            // only looked for async methods when the block contained no synchronous one at all.
            while let Some(at) = [remaining.find("pub fn "), remaining.find("pub async fn ")]
                .into_iter()
                .flatten()
                .min()
            {
                let from = &remaining[at..];
                let stop = from.find('{').map_or_else(
                    || from.find(';').unwrap_or(from.len()),
                    |brace| from.find(';').map_or(brace, |semi| semi.min(brace)),
                );
                let signature: String = from[..stop]
                    .lines()
                    .map(without_comments)
                    .collect::<Vec<_>>()
                    .join(" ");
                signatures.push((path.clone(), signature));
                remaining = &from[stop.max(1)..];
            }
        }
    }

    signatures
}

/// The transport identifiers named by any public signature of `type_name`.
fn transport_types_exposed_by(type_name: &str) -> Vec<(String, String, &'static str)> {
    let mut found = Vec::new();
    for (path, signature) in public_signatures_of(type_name) {
        for identifier in TRANSPORT_IDENTIFIERS {
            if signature.contains(identifier) {
                found.push((path.clone(), signature.clone(), identifier));
            }
        }
    }
    found
}

// ── the assertion ────────────────────────────────────────────────────────────────────────────

#[test]
fn no_facade_root_name_exposes_a_transport_type_in_a_public_signature() {
    let names = facade_root_names();

    let mut violations = Vec::new();
    for name in &names {
        for (path, signature, identifier) in transport_types_exposed_by(name) {
            violations.push(format!(
                "`renvor::{name}` exposes `{identifier}` in a public signature \
                 ({path}): {}",
                signature.split_whitespace().collect::<Vec<_>>().join(" ")
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "the facade root promotes {} name(s) whose public API names a third-party transport type. \
         Re-exporting one makes every upstream major version a Renvor breaking change, which the \
         facade's own rustdoc says it will not do.\n{}",
        violations.len(),
        violations.join("\n")
    );
}

#[test]
fn the_scan_detects_the_signature_that_was_actually_removed() {
    // POSITIVE CONTROL, and a real one rather than a synthetic string.
    //
    // `Server` was re-exported at the facade root until 2026-08-23. `Server::serve` takes the
    // underlying router BY PARAMETER, which is exactly the construct the test above forbids. It
    // still exists — one level down, as `renvor::transport::Server`, where naming a transport type
    // is the module's purpose — so the scan can be pointed at it and must report it.
    //
    // Without this control, a scan that silently found nothing would pass the test above forever.
    let exposed = transport_types_exposed_by("Server");

    assert!(
        !exposed.is_empty(),
        "the scan reported that `Server` exposes no transport type. It does — `serve` takes the \
         router by parameter — so the scan is not reading signatures and the test above proves \
         nothing"
    );

    assert!(
        exposed.iter().any(|(path, _, _)| path == "server.rs"),
        "the scan found a transport type for `Server` somewhere other than `server.rs`: {exposed:?}"
    );
}

#[test]
fn server_is_not_among_the_facade_root_names() {
    // The boundary decision itself, asserted rather than described. `Server` must stay reachable
    // under `transport`, and must not return to the root.
    let names = facade_root_names();

    assert!(
        !names.iter().any(|name| name == "Server"),
        "`Server` is back at the facade root. Its `serve` takes the router by parameter, so this \
         puts a third-party type in Renvor's public API through the front door. It belongs under \
         `renvor::transport`, which is the transport"
    );

    // And the lifecycle-managed path — the one an application is meant to use — IS at the root.
    for required in ["HttpServerProvider", "HttpServerConfig", "RouteRegistry"] {
        assert!(
            names.iter().any(|name| name == required),
            "`{required}` is missing from the facade root. Removing `Server` is only correct if \
             the Renvor-owned path it replaces is actually reachable without reaching into \
             `transport`"
        );
    }
}
