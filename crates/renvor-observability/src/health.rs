//! Liveness and readiness as JSON documents over a cloned `HealthState` (FR-073, FR-088).
//!
//! Two documents, because the two questions are independent (C-O8): an orchestrator that
//! restarts a process that is alive but not ready is doing the wrong thing, and the documents
//! must not let it conflate them.
//!
//! # Bounded names, and nothing else
//!
//! A readiness document names each contributor, its verdict, and its fault kind — and nothing
//! else: no message, no error, no address. Contributor names are bounded to
//! [`MAX_CONTRIBUTOR_NAME_BYTES`] and stripped of control characters before they are emitted, so
//! a contributor registered under a long or odd name cannot shape the document. The kernel does
//! not bound the name at registration; this renderer is where the bound lives.

use renvor_core::HealthState;
use renvor_core::health::{ContributorFault, Liveness, Readiness};
use serde_json::{Map, Value};

/// The most bytes of a contributor name that are emitted.
pub const MAX_CONTRIBUTOR_NAME_BYTES: usize = 64;

/// A contributor name as emitted: control characters replaced, cut at a character boundary.
#[must_use]
pub fn bounded_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect();
    if cleaned.len() <= MAX_CONTRIBUTOR_NAME_BYTES {
        return cleaned;
    }
    let mut cut = MAX_CONTRIBUTOR_NAME_BYTES;
    while !cleaned.is_char_boundary(cut) {
        cut -= 1;
    }
    cleaned[..cut].to_owned()
}

fn liveness_label(liveness: Liveness) -> &'static str {
    match liveness {
        Liveness::Alive => "alive",
        Liveness::Dead => "dead",
    }
}

fn readiness_label(readiness: Readiness) -> &'static str {
    match readiness {
        Readiness::Ready => "ready",
        Readiness::NotReady => "not_ready",
    }
}

fn fault_label(fault: ContributorFault) -> &'static str {
    match fault {
        ContributorFault::None => "none",
        ContributorFault::Panicked => "panicked",
        ContributorFault::TimedOut => "timed_out",
        ContributorFault::NotAsked => "not_asked",
    }
}

/// Whether the liveness document should be served as healthy.
#[must_use]
pub fn is_alive(health: &HealthState) -> bool {
    health.liveness() == Liveness::Alive
}

/// The liveness document: `{"status":"alive"}` or `{"status":"dead"}`.
#[must_use]
pub fn liveness_document(health: &HealthState) -> String {
    let mut document = Map::new();
    document.insert(
        "status".to_owned(),
        Value::String(liveness_label(health.liveness()).to_owned()),
    );
    Value::Object(document).to_string()
}

/// Whether the readiness document should be served as healthy.
#[must_use]
pub fn is_ready(health: &HealthState) -> bool {
    health.readiness().readiness == Readiness::Ready
}

/// The readiness document: overall status, whether the process is draining, and one entry per
/// contributor with its bounded name, verdict, and fault kind.
#[must_use]
pub fn readiness_document(health: &HealthState) -> String {
    let report = health.readiness();
    let mut document = Map::new();
    document.insert(
        "status".to_owned(),
        Value::String(readiness_label(report.readiness).to_owned()),
    );
    document.insert("draining".to_owned(), Value::Bool(report.draining));
    let contributors: Vec<Value> = report
        .contributors
        .iter()
        .map(|verdict| {
            let mut entry = Map::new();
            entry.insert(
                "name".to_owned(),
                Value::String(bounded_name(&verdict.name)),
            );
            entry.insert(
                "readiness".to_owned(),
                Value::String(readiness_label(verdict.readiness).to_owned()),
            );
            entry.insert(
                "fault".to_owned(),
                Value::String(fault_label(verdict.fault).to_owned()),
            );
            Value::Object(entry)
        })
        .collect();
    document.insert("contributors".to_owned(), Value::Array(contributors));
    Value::Object(document).to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use renvor_core::HealthState;
    use renvor_core::health::{Liveness, Readiness, ReadinessContributor};

    use super::{
        MAX_CONTRIBUTOR_NAME_BYTES, bounded_name, is_alive, is_ready, liveness_document,
        readiness_document,
    };

    #[derive(Debug)]
    struct Fixed {
        name: String,
        readiness: Readiness,
    }

    impl ReadinessContributor for Fixed {
        fn name(&self) -> &str {
            &self.name
        }
        fn readiness(&self) -> Readiness {
            self.readiness
        }
    }

    #[test]
    fn the_documents_carry_verdicts_and_bounded_names_and_nothing_else() {
        let health = HealthState::new();
        health.register(Arc::new(Fixed {
            name: "database".to_owned(),
            readiness: Readiness::Ready,
        }));
        let long = format!("cache-{}\u{7}{}", "x".repeat(80), "hunter2CanaryDoNotLeak");
        health.register(Arc::new(Fixed {
            name: long,
            readiness: Readiness::NotReady,
        }));
        assert!(is_alive(&health));
        assert!(!is_ready(&health));
        let liveness: serde_json::Value =
            serde_json::from_str(&liveness_document(&health)).unwrap();
        assert_eq!(liveness, serde_json::json!({"status": "alive"}));
        let readiness: serde_json::Value =
            serde_json::from_str(&readiness_document(&health)).unwrap();
        assert_eq!(readiness["status"], "not_ready");
        assert_eq!(readiness["draining"], false);
        let contributors = readiness["contributors"].as_array().unwrap();
        assert_eq!(contributors.len(), 2);
        assert_eq!(contributors[0]["name"], "database");
        assert_eq!(contributors[0]["readiness"], "ready");
        assert_eq!(contributors[0]["fault"], "none");
        let name = contributors[1]["name"].as_str().unwrap();
        assert!(name.len() <= MAX_CONTRIBUTOR_NAME_BYTES);
        assert!(
            !name.contains("hunter2"),
            "the tail of a long name reached the document"
        );
        assert!(
            !name.contains('\u{7}'),
            "a control character reached the document"
        );
        assert_eq!(contributors[1]["readiness"], "not_ready");
        let keys: Vec<&String> = contributors[0].as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            ["fault", "name", "readiness"],
            "an extra key appeared"
        );

        health.set_liveness(Liveness::Dead);
        assert!(!is_alive(&health));
        assert!(liveness_document(&health).contains("dead"));
        health.begin_draining();
        let readiness: serde_json::Value =
            serde_json::from_str(&readiness_document(&health)).unwrap();
        assert_eq!(readiness["draining"], true);
        assert_eq!(readiness["status"], "not_ready");
    }

    #[test]
    fn bounded_name_cuts_at_a_character_boundary() {
        let name = "é".repeat(MAX_CONTRIBUTOR_NAME_BYTES);
        let cut = bounded_name(&name);
        assert!(cut.len() <= MAX_CONTRIBUTOR_NAME_BYTES);
        assert!(cut.chars().all(|c| c == 'é'));
        assert_eq!(bounded_name("plain"), "plain");
    }
}
