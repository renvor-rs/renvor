//! One span per lifecycle phase, with structured fields and **0** interpolated messages.
//!
//! # Why every value is a field
//!
//! FR-043 requires all emitted data to use **structured fields, never interpolated message
//! strings**. The distinction is not stylistic. A field can be filtered, indexed, and redacted by a
//! subscriber; a value baked into a message is text, and the only way to act on it later is to
//! parse it back out — which is where log-processing pipelines go to die.
//!
//! So [`phase_span`] emits `phase` and `run_id` as fields, and its message is a constant. Nothing
//! in this module interpolates, which is asserted mechanically rather than merely intended.
//!
//! # The run identifier is on every span
//!
//! FR-043 requires every record to carry a kernel-generated run identifier that is the same for
//! the lifetime of one application run. Attaching it to the phase span means every record emitted
//! **inside** a phase inherits it, so the requirement is met by the span hierarchy rather than by
//! every call site remembering.

use tracing::{Level, Span};

use crate::lifecycle::LifecyclePhase;
use crate::observe::RunIdentifier;

/// The span name every phase span shares.
///
/// One constant, so a subscriber filtering on it and a test asserting on it cannot disagree.
pub const PHASE_SPAN_NAME: &str = "renvor.phase";

/// Creates the span for one lifecycle phase.
///
/// Fields, not a message: `phase` and `run_id` are both recorded as values a subscriber can act on.
#[must_use]
pub fn phase_span(phase: LifecyclePhase, run_id: &RunIdentifier) -> Span {
    tracing::span!(
        Level::INFO,
        PHASE_SPAN_NAME,
        phase = phase.as_str(),
        run_id = %run_id,
    )
}

#[cfg(test)]
mod tests {
    use super::{PHASE_SPAN_NAME, phase_span};
    use crate::lifecycle::LifecyclePhase;
    use crate::observe::{FixedEntropy, RunIdentifier};
    use std::sync::{Arc, Mutex, PoisonError};

    /// A subscriber that records the field names of every span it is told about.
    ///
    /// Written by hand because `renvor-core` deliberately carries no `tracing-subscriber`: this
    /// crate emits records and never formats them. Forty lines here is the price of asserting what
    /// a subscriber **actually receives**, which is stronger than reading a span's metadata —
    /// `Span::metadata` returns `None` when nothing is listening, so a test without a subscriber
    /// would be asserting against a disabled span and passing for the wrong reason. Measured: the
    /// first version of this test did exactly that and failed.
    /// One span's name and the field names it carried.
    type SpanRecord = (String, Vec<String>);

    #[derive(Clone, Default)]
    struct Recorder {
        spans: Arc<Mutex<Vec<SpanRecord>>>,
    }

    impl Recorder {
        fn spans(&self) -> Vec<SpanRecord> {
            self.spans
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    /// Collects field names and their rendered values.
    #[derive(Default)]
    struct Collector(Vec<String>);

    impl tracing::field::Visit for Collector {
        fn record_debug(&mut self, field: &tracing::field::Field, _: &dyn core::fmt::Debug) {
            self.0.push(field.name().to_owned());
        }
    }

    impl tracing::Subscriber for Recorder {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        fn new_span(&self, attributes: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            let mut collector = Collector::default();
            attributes.record(&mut collector);
            self.spans
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((attributes.metadata().name().to_owned(), collector.0));
            tracing::span::Id::from_u64(1)
        }

        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, _: &tracing::Event<'_>) {}
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn a_subscriber_receives_the_phase_and_the_run_identifier_as_fields() {
        let recorder = Recorder::default();
        let run_id = RunIdentifier::generate(&FixedEntropy::new(vec![7; 32])).expect("generates");

        tracing::subscriber::with_default(recorder.clone(), || {
            let _span = phase_span(LifecyclePhase::Boot, &run_id);
        });

        let spans = recorder.spans();
        assert_eq!(spans.len(), 1, "one span per phase");
        assert_eq!(spans[0].0, PHASE_SPAN_NAME);
        assert_eq!(
            spans[0].1,
            vec!["phase".to_owned(), "run_id".to_owned()],
            "both values arrive as fields, not as message text"
        );
    }

    #[test]
    fn one_span_is_emitted_per_phase_and_no_more() {
        // FR-043 says one span **per lifecycle phase**. Emitting every phase and counting is the
        // form of that claim that can fail.
        let recorder = Recorder::default();
        let run_id = RunIdentifier::generate(&FixedEntropy::new(vec![7; 32])).expect("generates");

        tracing::subscriber::with_default(recorder.clone(), || {
            for phase in LifecyclePhase::ALL {
                let _span = phase_span(phase, &run_id);
            }
        });

        assert_eq!(recorder.spans().len(), LifecyclePhase::ALL.len());
        assert!(
            recorder
                .spans()
                .iter()
                .all(|(name, _)| name == PHASE_SPAN_NAME),
            "every phase span shares one name, so a subscriber can filter on it"
        );
    }

    #[test]
    fn this_module_interpolates_nothing() {
        // SC-018 and FR-043: 0 interpolated message strings. A `format!` here would produce text a
        // subscriber cannot filter, index, or redact.
        //
        // Comment lines are stripped before the scan. A first version searched the whole file and
        // matched its own prose — the trap every self-reading test sets for itself, and the second
        // time this phase has walked into it.
        let source = include_str!("spans.rs");
        let production: String = source
            .split("#[cfg(test)]")
            .next()
            .expect("split always yields one part")
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for interpolation in ["format!", "format_args!", "to_string()", "push_str"] {
            assert!(
                !production.contains(interpolation),
                "`{interpolation}` reached the span module, which FR-043 forbids"
            );
        }

        // POSITIVE CONTROL: the scan reads real code and finds what IS there.
        assert!(production.contains("tracing::span!"));
        assert!(
            production.contains("phase = phase.as_str()"),
            "the fields are still declared as fields"
        );
    }
}
