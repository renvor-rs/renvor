//! The capability ports name no infrastructure type (FR-001, FR-100).
//!
//! `facade_boundary.rs` proves the transport root names expose no `axum`/`hyper`/`tower` type.
//! This does the same for the five capability crates: every public signature in a **port** file —
//! the port, its value types, its substitute, its provider — is scanned for the identifiers of the
//! adapters' dependencies. An adapter module (Valkey, SMTP, the filesystem store, OTLP) may name
//! its own crate behind its own feature; a port may not, because a port is what an application
//! and the substitutes are written against.

use std::path::{Path, PathBuf};

/// Identifiers that mark an infrastructure type. Matched as substrings of a public signature.
const FORBIDDEN: [&str; 12] = [
    "redis::",
    "lettre::",
    "cap_std::",
    "cap_tempfile::",
    "opentelemetry",
    "sqlx::",
    "sea_orm::",
    "hyper",
    "tracing_subscriber::",
    "rustls",
    "axum",
    "tower",
];

/// The port files: every `src` file of each capability crate except its adapter modules.
const PORT_FILES: [(&str, &str); 19] = [
    ("renvor-cache", "src/port.rs"),
    ("renvor-cache", "src/memory.rs"),
    ("renvor-cache", "src/provider.rs"),
    ("renvor-jobs", "src/job.rs"),
    ("renvor-jobs", "src/store.rs"),
    ("renvor-jobs", "src/memory.rs"),
    ("renvor-jobs", "src/worker.rs"),
    ("renvor-jobs", "src/provider.rs"),
    ("renvor-mail", "src/port.rs"),
    ("renvor-mail", "src/recording.rs"),
    ("renvor-mail", "src/provider.rs"),
    ("renvor-storage", "src/port.rs"),
    ("renvor-storage", "src/memory.rs"),
    ("renvor-storage", "src/provider.rs"),
    ("renvor-observability", "src/redaction.rs"),
    ("renvor-observability", "src/subscriber.rs"),
    ("renvor-observability", "src/health.rs"),
    ("renvor-observability", "src/prometheus.rs"),
    ("renvor-observability", "src/text.rs"),
];

fn crates_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the facade crate has a parent")
        .to_path_buf()
}

fn without_comments(line: &str) -> &str {
    line.find("//").map_or(line, |at| &line[..at])
}

/// Every public signature in `source`, joined across lines up to the body brace or semicolon.
fn public_signatures(source: &str) -> Vec<String> {
    let mut signatures = Vec::new();
    let mut current: Option<String> = None;
    let mut depth_test = false;
    for raw in source.lines() {
        let line = without_comments(raw);
        if line.trim_start().starts_with("#[cfg(test)]") {
            depth_test = true;
        }
        if depth_test {
            continue;
        }
        let trimmed = line.trim_start();
        let starts = trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub async fn ")
            || trimmed.starts_with("pub const fn ")
            || trimmed.starts_with("pub struct ")
            || trimmed.starts_with("pub enum ")
            || trimmed.starts_with("pub trait ")
            || trimmed.starts_with("pub type ")
            // Trait methods are public by the trait's visibility.
            || (trimmed.starts_with("fn ") && !trimmed.starts_with("fn fmt("));
        if starts && current.is_none() {
            current = Some(String::new());
        }
        if let Some(buffer) = current.as_mut() {
            buffer.push_str(line.trim());
            buffer.push(' ');
            if line.contains('{') || line.trim_end().ends_with(';') {
                signatures.push(buffer.clone());
                current = None;
            }
        }
    }
    signatures
}

#[test]
fn no_capability_port_signature_names_an_infrastructure_type() {
    let root = crates_root();
    let mut scanned = 0;
    let mut offences = Vec::new();
    for (krate, file) in PORT_FILES {
        let path = root.join(krate).join(file);
        let source =
            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{krate}/{file} is readable"));
        for signature in public_signatures(&source) {
            scanned += 1;
            for forbidden in FORBIDDEN {
                if signature.contains(forbidden) {
                    offences.push(format!("{krate}/{file}: {forbidden} in `{signature}`"));
                }
            }
        }
    }
    assert!(
        scanned > 150,
        "only {scanned} public signatures were found across the port files; the scanner has drifted"
    );
    assert!(
        offences.is_empty(),
        "a port names an infrastructure type:\n{}",
        offences.join("\n")
    );
}

#[test]
fn the_scanner_sees_a_planted_offence() {
    // POSITIVE CONTROL: the scan must be able to fail.
    let planted = "pub fn connect(client: redis::Client) -> Self {\n";
    let found = public_signatures(planted)
        .iter()
        .any(|signature| FORBIDDEN.iter().any(|f| signature.contains(f)));
    assert!(found, "the scanner did not see a planted redis type");
    let clean = "pub fn connect(settings: &Settings) -> Result<Self, Error> {\n";
    assert!(
        !public_signatures(clean)
            .iter()
            .any(|signature| FORBIDDEN.iter().any(|f| signature.contains(f)))
    );
}

#[test]
fn every_capability_feature_is_declared_off_by_default() {
    let manifest =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
            .expect("the facade manifest is readable");
    let default_line = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("default = "))
        .expect("a default feature line");
    for feature in [
        "capability-cache",
        "capability-jobs",
        "capability-mail",
        "capability-storage",
        "observability",
        "observability-otel",
    ] {
        assert!(
            manifest.contains(&format!("{feature} = [")),
            "the facade does not declare `{feature}`"
        );
        assert!(
            !default_line.contains(feature),
            "`{feature}` is on by default, which PLAN §7.4 forbids"
        );
    }
}
