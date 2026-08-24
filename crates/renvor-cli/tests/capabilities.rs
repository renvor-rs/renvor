//! Capabilities the executable must NOT have (FR-040, FR-043).
//!
//! # Why absence is asserted structurally rather than tested as hardening
//!
//! Contract C-4 settled that every template is embedded, so there is no archive path — and
//! therefore no zip-slip and no decompression-amplification defence, because the capability those
//! defend does not exist. FR-040 requires that absence to be **demonstrated**, not assumed:
//!
//! > *"This MUST be asserted structurally — the built executable MUST carry no archive-extraction
//! > capability — rather than tested as hardening against a code path that does not exist."*
//!
//! Hardening a code path that does not exist produces tests that pass for no reason. Asserting the
//! path cannot exist produces a test that fails the moment somebody adds the dependency.
//!
//! # What this proves, and what it does not
//!
//! It reads the **resolved** lockfile and walks the dependency closure of `renvor-cli`. So it
//! proves no *crate* providing these capabilities is reachable.
//!
//! # Demonstrated firing, not assumed to
//!
//! On 2026-08-18 `flate2 = "1"` was added to `crates/renvor-cli/Cargo.toml` and
//! `the_executable_reaches_no_archive_crate` **failed**, naming `["flate2"]`. The dependency was
//! then removed and `Cargo.lock` restored. A gate nobody has watched fail is a gate nobody knows
//! works.
//!
//! It does **not** prove the binary makes no syscall of its own — hand-written inflate code, or a
//! `libc` call, would not appear as a dependency. That is a real limit and it is stated rather than
//! glossed: the practical risk this defends against is somebody adding `zip` to `Cargo.toml` to
//! solve a problem, not somebody hand-writing DEFLATE.

use std::collections::{BTreeMap, BTreeSet};

const LOCKFILE: &str = include_str!("../../../Cargo.lock");

/// Every package reachable from `root` in the lockfile, `root` included.
fn closure(root: &str) -> BTreeSet<String> {
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for block in LOCKFILE.split("[[package]]").skip(1) {
        let Some(name) = block
            .lines()
            .find_map(|line| line.strip_prefix("name = \""))
            .and_then(|rest| rest.strip_suffix('"'))
        else {
            continue;
        };
        let mut dependencies = Vec::new();
        let mut inside = false;
        for line in block.lines() {
            let trimmed = line.trim();
            if trimmed == "dependencies = [" {
                inside = true;
                continue;
            }
            if inside {
                if trimmed == "]" {
                    break;
                }
                let entry = trimmed.trim_end_matches(',').trim_matches('"');
                if let Some(first) = entry.split_whitespace().next()
                    && !first.is_empty()
                {
                    dependencies.push(first.to_owned());
                }
            }
        }
        graph
            .entry(name.to_owned())
            .or_default()
            .extend(dependencies);
    }

    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_owned()];
    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        if let Some(children) = graph.get(&current) {
            stack.extend(children.iter().cloned());
        }
    }
    seen
}

/// Crates that read or write an archive or a compressed stream.
///
/// Enumerated rather than pattern-matched, so a reader can judge whether the list is complete.
/// Every one of these was checked and found acceptable on licence grounds in
/// [Phase 003 research §D7](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/research.md) —
/// they are excluded on **capability** grounds, not on quality.
const ARCHIVE_CRATES: [&str; 11] = [
    "tar",
    "zip",
    "flate2",
    "zstd",
    "xz2",
    "bzip2",
    "lz4",
    "lz4_flex",
    "brotli",
    "async-compression",
    "sevenz-rust",
];

/// Database driver crates.
///
/// # The CLI must not resolve one, and nothing asserted it
///
/// `renvor-cli` gained a dependency on `renvor-database` in Phase 006 — the **ports**, which name
/// no driver. It must never gain one on `renvor-sqlx`, which names two. FR-063's claim that
/// generation cannot reach a database rests on that, and the claim was checked by hand rather than
/// by this file: neither list below mentioned `sqlx`, so an accidental `renvor-sqlx` edge would
/// have left this suite green.
///
/// `renvor-database` is deliberately absent from this list. It is a dependency **on purpose** —
/// the CLI parses `--database` into `DatabaseKind` so there is one list of database names in the
/// workspace rather than two that agree by coincidence.
const DRIVER_CRATES: [&str; 5] = [
    "sqlx",
    "sqlx-core",
    "sqlx-postgres",
    "sqlx-mysql",
    "renvor-sqlx",
];

/// HTTP and network client crates.
///
/// This is the structural half of FR-043. The behavioural half — running with every proxy pointed
/// at a closed port — is in `tests/acceptance.rs`, and neither is sufficient alone: this one cannot
/// see a raw socket, and that one cannot see a direct connection that ignores proxy variables.
const NETWORK_CRATES: [&str; 9] = [
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "isahc",
    "attohttpc",
    "surf",
    "http-client",
    "native-tls",
];

/// The generator resolves no database driver.
///
/// # Why this matters beyond tidiness
///
/// A generated project selects exactly one driver, and the generator selects none. If `renvor-cli`
/// took an edge to `renvor-sqlx`, every `renvor new` would carry both drivers' compile cost and
/// the feature-isolation claim would stop being about the framework and start being about the
/// framework-minus-the-CLI.
#[test]
fn the_executable_reaches_no_database_driver() {
    let reachable = closure("renvor-cli");
    let found: Vec<&str> = DRIVER_CRATES
        .iter()
        .copied()
        .filter(|name| reachable.contains(*name))
        .collect();
    assert!(
        found.is_empty(),
        "the generator resolves a database driver: {found:?}"
    );
    // THE POSITIVE CONTROL, and it is the interesting half. `renvor-database` IS a dependency, so
    // finding it proves the closure walk can see a Renvor crate at all — without it, an empty
    // `found` would be satisfied by a walk that returned nothing.
    assert!(
        reachable.contains("renvor-database"),
        "the closure walk cannot see `renvor-database`, so the absence above proves nothing"
    );
}

#[test]
fn the_executable_reaches_no_archive_crate() {
    let reachable = closure("renvor-cli");
    let found: Vec<&str> = ARCHIVE_CRATES
        .iter()
        .copied()
        .filter(|crate_name| reachable.contains(*crate_name))
        .collect();
    assert!(
        found.is_empty(),
        "FR-040 asserts the built executable carries NO archive-extraction capability, and these \
         are now reachable from `renvor-cli`: {found:?}. If an archive path is genuinely wanted, \
         contract C-4 says zip-slip and decompression-amplification defences become that phase's \
         requirement — this test is the trigger for that conversation, not an obstacle to route \
         around"
    );
}

#[test]
fn the_executable_reaches_no_http_client_crate() {
    let reachable = closure("renvor-cli");
    let found: Vec<&str> = NETWORK_CRATES
        .iter()
        .copied()
        .filter(|crate_name| reachable.contains(*crate_name))
        .collect();
    assert!(
        found.is_empty(),
        "FR-043 requires local flows to need no network, and these HTTP clients are now reachable \
         from `renvor-cli`: {found:?}"
    );
}

#[test]
fn the_closure_walk_can_detect_a_crate_that_is_present() {
    // NEGATIVE CONTROL, and the load-bearing test in this file. Without it, a walk that returned
    // an empty set — a renamed root, a lockfile format change, a parsing bug — would satisfy both
    // assertions above while proving nothing at all.
    let reachable = closure("renvor-cli");
    for expected in ["minijinja", "cap-std", "clap", "serde"] {
        assert!(
            reachable.contains(expected),
            "the walk cannot see `{expected}`, which `renvor-cli` certainly depends on, so its \
             silence about archive and network crates means nothing"
        );
    }
    assert!(
        reachable.len() > 20,
        "the closure has {} entries, which is too few to be a real dependency graph",
        reachable.len()
    );
}

#[test]
fn the_forbidden_lists_are_not_empty() {
    // A third way for these tests to pass for no reason: someone empties a list rather than
    // removing a dependency.
    assert!(!ARCHIVE_CRATES.is_empty());
    assert!(!NETWORK_CRATES.is_empty());
}
