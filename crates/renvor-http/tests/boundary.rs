//! The transport boundary, asserted mechanically **over the whole crate**.
//!
//! # What this test is for
//!
//! SC-012: **0** transport types appear in application- or domain-facing interfaces. A handler
//! receives `renvor_http::Request` and returns `renvor_http::Response`, and neither may grow an
//! `axum`, `tower`, or `http` type in its signature.
//!
//! # Why it reads the source rather than the type system
//!
//! A trait bound can express "does not name axum" only by enumerating what it *does* allow, which
//! is a list that goes stale. Reading the source catches the case the type system cannot: a
//! `pub` item added to an application-facing module that happens to name a transport type.
//!
//! # Why it walks the directory rather than listing files
//!
//! The earlier version held a **four-file hand-list**. Every module outside that list was
//! unscanned, and a new application-facing module was unscanned **silently** — the test kept
//! passing and its coverage kept shrinking, which is the worst combination a guard can have.
//!
//! It now walks `src/` and scans everything it finds, with a short list of modules that are
//! *permitted* to name the transport because they **are** the transport. That list is asserted to
//! match real files, so deleting or renaming one of them fails here rather than quietly widening
//! the exemption.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Modules that are allowed to name transport types, because they are where the transport lives.
///
/// Requiring these to be transport-free would be requiring them not to exist.
const TRANSPORT_MODULES: [&str; 3] = ["route/build.rs", "server.rs", "provider.rs"];

/// The transport crate names that must not appear.
const TRANSPORT_IDENTIFIERS: [&str; 4] = ["axum", "tower_http", "tower::", "hyper"];

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Every `.rs` file under `root`, as (path-relative-to-root, contents).
///
/// Recursive and total: there is no place in the tree a module can hide.
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
    found
}

/// Finds transport identifiers in real code, ignoring documentation.
///
/// Documentation is excluded on purpose: these modules **explain** why they do not use the
/// transport types, and a scan that flagged the explanation would force the explanation to be
/// deleted — which is the opposite of what this test is protecting.
fn transport_identifiers_in_code(source: &str) -> Vec<String> {
    source
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .filter(|line| {
            TRANSPORT_IDENTIFIERS
                .iter()
                .any(|identifier| line.contains(identifier))
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn no_transport_type_appears_outside_the_transport_modules() {
    let permitted: BTreeSet<&str> = TRANSPORT_MODULES.into_iter().collect();
    let mut scanned = 0_usize;

    for (name, source) in rust_files(&source_root()) {
        if permitted.contains(name.as_str()) {
            continue;
        }
        scanned += 1;

        let found = transport_identifiers_in_code(&source);
        assert!(
            found.is_empty(),
            "{name} names a transport type in code, which would put it in an \
             application-facing signature: {found:?}"
        );
    }

    // A scan that found no files would report every module clean.
    assert!(
        scanned >= 8,
        "only {scanned} modules were scanned, which is fewer than this crate has — the walk is \
         not finding the source tree"
    );
}

#[test]
fn every_exempted_module_exists() {
    // An exemption for a file that is no longer there is an exemption that silently covers
    // nothing — and, worse, hides that the code it covered moved somewhere unexempted.
    for module in TRANSPORT_MODULES {
        let path = source_root().join(module);
        assert!(
            path.is_file(),
            "`{module}` is exempted from the transport-boundary scan but does not exist"
        );
    }
}

#[test]
fn the_scan_finds_a_transport_type_in_a_newly_added_file() {
    // POSITIVE CONTROL, and specifically for the failure the hand-list had: a module that did not
    // exist when the guard was written must still be scanned.
    //
    // A temporary tree is used rather than writing into `src/`, so the control proves the WALK
    // discovers new files without leaving anything behind if it fails.
    let temporary = tempfile::tempdir().expect("tempdir");
    let root = temporary.path();

    std::fs::create_dir_all(root.join("nested")).expect("mkdir");
    std::fs::write(root.join("clean.rs"), "pub fn f(x: u8) -> u8 { x }\n").expect("write");
    std::fs::write(
        root.join("nested").join("brand_new_module.rs"),
        "pub fn handler(request: axum::extract::Request) -> Response { todo!() }\n",
    )
    .expect("write");

    let files = rust_files(root);
    assert_eq!(
        files.len(),
        2,
        "the walk did not find both files: {files:?}"
    );

    let offending: Vec<&String> = files
        .iter()
        .filter(|(_, source)| !transport_identifiers_in_code(source).is_empty())
        .map(|(name, _)| name)
        .collect();

    assert_eq!(
        offending,
        vec!["nested/brand_new_module.rs"],
        "the scan did not flag a transport type in a newly added nested file"
    );
}

#[test]
fn the_scan_finds_a_transport_type_where_the_transport_actually_lives() {
    // The second half of the control: real code in this workspace that the scan must flag. If it
    // reported `route/build.rs` clean, its silence about every other module would mean nothing.
    let build = std::fs::read_to_string(source_root().join("route/build.rs")).expect("readable");
    assert!(
        !transport_identifiers_in_code(&build).is_empty(),
        "the scan found no transport identifier in route/build.rs, where the transport lives"
    );
}

#[test]
fn documentation_may_discuss_the_transport_without_failing_the_scan() {
    // The modules above explain why they avoid these types. If the scan flagged prose, the only
    // way to pass would be to delete the explanation.
    let documented = "//! This module never names axum, and here is why.\n/// Not tower::Service.";
    assert!(transport_identifiers_in_code(documented).is_empty());
}

/// `Span::enter` must not be used in this crate's async request path; `Instrument` is the form.
///
/// # Why this is a source scan rather than a behavioural test
///
/// The defect it guards is real and was shipped: `let _entered = span.enter();` held a thread-local
/// guard across the handler `.await`. On a multi-thread runtime the future yields with the span
/// still on that worker's stack, the executor polls a DIFFERENT request on the same thread, and
/// that request's events are parented to the wrong span — carrying the wrong `request_id`.
///
/// Reproducing that needs two requests interleaved on one worker at a chosen yield point, which is
/// a race rather than a test. What CAN be checked deterministically is that the construct is not
/// present. That is weaker evidence than an observation and is recorded as such — but it is
/// stronger than the nothing that guarded it before, and it fails loudly on reintroduction.
#[test]
fn the_async_request_path_never_holds_a_span_guard_across_an_await() {
    let files = rust_files(&source_root());
    let offending: Vec<&String> = files
        .iter()
        .filter(|(_, contents)| holds_a_span_guard(contents))
        .map(|(path, _)| path)
        .collect();

    assert!(
        offending.is_empty(),
        "`Span::enter` returns a thread-local guard and must not be held across an await;          use `Instrument` instead. Found in: {offending:?}"
    );
}

#[test]
fn the_span_guard_scan_would_notice_the_construct_coming_back() {
    // POSITIVE CONTROL: the scan above passes trivially if it cannot recognise what it forbids.
    let root = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        root.path().join("reintroduced.rs"),
        "async fn handler() {\n    let _entered = span.enter();\n    work().await;\n}\n",
    )
    .expect("write");

    let files = rust_files(root.path());
    let found = files
        .iter()
        .filter(|(_, contents)| holds_a_span_guard(contents))
        .count();

    assert_eq!(
        found, 1,
        "the scan cannot recognise the construct it forbids"
    );
}

/// Whether `source` ENTERS a span outside a comment.
///
/// Comment lines are skipped for the same reason the transport scan skips them: the fix for this
/// defect has to be able to explain itself in prose without tripping the check that guards it.
fn holds_a_span_guard(source: &str) -> bool {
    source
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .any(|line| line.contains(".enter()"))
}
