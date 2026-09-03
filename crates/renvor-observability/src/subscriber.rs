//! The subscriber builder: returns a value, never installs one (FR-067, C-O7).
//!
//! # JSON by default, human-readable by choice
//!
//! [`LogFormat::Json`] is Renvor's one-object-per-line format with redaction on event and span
//! fields. [`LogFormat::Human`] is `tracing-subscriber`'s full text format with Renvor's
//! redacting field formatter, for a terminal during development; it is not the default because a
//! sentence cannot be filtered, indexed, or redacted after the fact (C-O2).
//!
//! # The filter is configuration
//!
//! An `EnvFilter` directive that fails to parse is a **Validate** failure naming the configuration
//! key it came from (FR-080). It is not a secret and is attributed like any other key; the
//! directive text itself is not in the error, because a directive can be long and the key is what
//! an operator needs.

use tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;

use crate::json::{JsonEvent, JsonFields};
use crate::redaction::Redaction;

/// The configuration key the filter directive is attributed to by default.
pub const FILTER_KEY: &str = "observability.log.filter";
/// The filter used when configuration names none.
pub const DEFAULT_FILTER: &str = "info";
/// The most bytes a filter directive may carry.
pub const MAX_FILTER_BYTES: usize = 4096;

/// Why the subscriber could not be built. **Closed**; names a key, never a value.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ObservabilityError {
    /// The filter directive under `key` did not parse or exceeded the bound.
    #[error("the log filter under `{key}` is invalid")]
    FilterInvalid {
        /// The configuration key the directive came from.
        key: String,
    },
}

/// How records are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogFormat {
    /// One JSON object per record (the default).
    #[default]
    Json,
    /// A human-readable line per record, for development.
    Human,
}

/// What the subscriber is built from.
#[derive(Clone, Debug)]
pub struct LogSettings {
    format: LogFormat,
    filter: String,
    filter_key: String,
    redaction: Redaction,
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            format: LogFormat::Json,
            filter: DEFAULT_FILTER.to_owned(),
            filter_key: FILTER_KEY.to_owned(),
            redaction: Redaction::new(),
        }
    }
}

impl LogSettings {
    /// JSON, `info`, the built-in redaction set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects the format.
    #[must_use]
    pub const fn with_format(mut self, format: LogFormat) -> Self {
        self.format = format;
        self
    }

    /// Sets the `EnvFilter` directive and the configuration key it is attributed to.
    #[must_use]
    pub fn with_filter(mut self, directive: &str, key: &str) -> Self {
        self.filter = directive.to_owned();
        self.filter_key = key.to_owned();
        self
    }

    /// Adds field names to the redaction set (never removes one).
    #[must_use]
    pub fn with_redaction(mut self, redaction: Redaction) -> Self {
        self.redaction = redaction;
        self
    }

    /// The redaction rule in force.
    #[must_use]
    pub const fn redaction(&self) -> &Redaction {
        &self.redaction
    }

    /// Parses the filter directive.
    ///
    /// # Errors
    ///
    /// [`ObservabilityError::FilterInvalid`] naming the key.
    pub fn filter(&self) -> Result<EnvFilter, ObservabilityError> {
        let invalid = || ObservabilityError::FilterInvalid {
            key: self.filter_key.clone(),
        };
        if self.filter.is_empty() || self.filter.len() > MAX_FILTER_BYTES {
            return Err(invalid());
        }
        EnvFilter::try_new(&self.filter).map_err(|_| invalid())
    }
}

/// Builds a subscriber writing to `writer`. The caller installs it — through
/// `renvor_core::observe::try_init_global` for the process, or `tracing::subscriber::set_default`
/// for a scope — and this crate never does.
///
/// # Errors
///
/// [`ObservabilityError::FilterInvalid`] when the directive does not parse.
pub fn build<W>(
    settings: &LogSettings,
    writer: W,
) -> Result<Box<dyn Subscriber + Send + Sync + 'static>, ObservabilityError>
where
    W: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let filter = settings.filter()?;
    let registry = tracing_subscriber::registry().with(filter);
    let rule = settings.redaction.clone();
    match settings.format {
        LogFormat::Json => {
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .fmt_fields(JsonFields::new(rule.clone()))
                .event_format(JsonEvent::new(rule));
            Ok(Box::new(registry.with(layer)))
        }
        LogFormat::Human => {
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .fmt_fields(crate::text::TextFields::new(rule));
            Ok(Box::new(registry.with(layer)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::sync::{Arc, Mutex, PoisonError};

    use tracing_subscriber::fmt::MakeWriter;

    use super::{FILTER_KEY, LogFormat, LogSettings, ObservabilityError, build};
    use crate::redaction::{MAX_VALUE_BYTES, REDACTED, Redaction};

    /// A writer that keeps every line for the test to read back.
    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Capture {
        fn text(&self) -> String {
            String::from_utf8(
                self.0
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .clone(),
            )
            .expect("utf-8")
        }
        fn lines(&self) -> Vec<serde_json::Value> {
            self.text()
                .lines()
                .map(|line| serde_json::from_str(line).expect("one JSON object per line"))
                .collect()
        }
    }

    impl io::Write for Capture {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for Capture {
        type Writer = Self;
        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    fn emit_under(settings: &LogSettings, capture: &Capture, emit: impl FnOnce()) {
        let subscriber = build(settings, capture.clone()).expect("builds");
        let guard = tracing::subscriber::set_default(subscriber);
        emit();
        drop(guard);
    }

    #[test]
    fn json_records_are_one_object_per_line_with_closed_metadata_and_redaction_on_both_paths() {
        let capture = Capture::default();
        emit_under(&LogSettings::new(), &capture, || {
            let phase = tracing::info_span!(
                "renvor.phase",
                phase = "boot",
                run_id = "run-0123456789abcdef",
                token = "hunter2CanaryDoNotLeak-span"
            );
            let _entered = phase.enter();
            let inner = tracing::info_span!("inner", db.password = "hunter2CanaryDoNotLeak-nested");
            let _inner = inner.enter();
            tracing::info!(
                target: "renvor.test",
                password = "hunter2CanaryDoNotLeak-event",
                attempt = 3_u64,
                ok = true,
                ratio = 0.5_f64,
                "job attempt finished"
            );
        });
        let text = capture.text();
        assert!(!text.contains("hunter2"), "a canary reached the output");
        let lines = capture.lines();
        assert_eq!(lines.len(), 1);
        let record = &lines[0];
        assert_eq!(record["level"], "INFO");
        assert_eq!(record["target"], "renvor.test");
        assert_eq!(record["message"], "job attempt finished");
        assert_eq!(record["fields"]["password"], REDACTED);
        assert_eq!(record["fields"]["attempt"], 3);
        assert_eq!(record["fields"]["ok"], true);
        assert_eq!(record["fields"]["ratio"], 0.5);
        assert_eq!(
            record["run_id"], "run-0123456789abcdef",
            "run_id is lifted from the span"
        );
        let spans = record["spans"].as_array().expect("spans");
        assert_eq!(
            spans.len(),
            2,
            "two enclosing spans are expected, outermost first"
        );
        assert_eq!(spans[0]["name"], "renvor.phase");
        assert_eq!(spans[0]["phase"], "boot");
        assert_eq!(spans[0]["token"], REDACTED, "a span field was not redacted");
        assert_eq!(spans[1]["name"], "inner");
        assert_eq!(
            spans[1]["db.password"], REDACTED,
            "a nested span field was not redacted"
        );
        assert!(
            record["timestamp"]
                .as_str()
                .is_some_and(|stamp| stamp.ends_with('Z') && stamp.len() == 24)
        );
    }

    #[test]
    fn a_configured_name_is_redacted_and_a_long_value_is_bounded() {
        let capture = Capture::default();
        let settings = LogSettings::new().with_redaction(Redaction::new().with_field("ssn"));
        let long = "v".repeat(MAX_VALUE_BYTES + 100);
        emit_under(&settings, &capture, || {
            tracing::warn!(ssn = "hunter2CanaryDoNotLeak", blob = %long, "x");
        });
        let record = &capture.lines()[0];
        assert_eq!(record["fields"]["ssn"], REDACTED);
        let blob = record["fields"]["blob"].as_str().unwrap();
        assert!(blob.contains("…[truncated 100 bytes]"));
        assert!(blob.len() < MAX_VALUE_BYTES + 50);
    }

    #[test]
    fn the_human_format_redacts_too_and_is_not_json() {
        let capture = Capture::default();
        let settings = LogSettings::new().with_format(LogFormat::Human);
        emit_under(&settings, &capture, || {
            let span = tracing::info_span!("s", secret = "hunter2CanaryDoNotLeak-span");
            let _entered = span.enter();
            tracing::info!(
                password = "hunter2CanaryDoNotLeak-event",
                user = "ada",
                "hello"
            );
        });
        let text = capture.text();
        assert!(
            !text.contains("hunter2"),
            "a canary reached the human output"
        );
        assert!(text.contains("hello") && text.contains("user=ada"));
        assert!(text.contains(REDACTED));
        assert!(serde_json::from_str::<serde_json::Value>(text.trim()).is_err());
    }

    #[test]
    fn the_filter_applies_and_an_invalid_one_names_the_key() {
        let capture = Capture::default();
        let settings = LogSettings::new().with_filter("renvor.loud=warn", "log.filter");
        emit_under(&settings, &capture, || {
            tracing::info!(target: "renvor.loud", "dropped");
            tracing::warn!(target: "renvor.loud", "kept");
        });
        let lines = capture.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0]["message"], "kept");

        let bad = LogSettings::new().with_filter("this is not = a [directive", "log.filter");
        assert_eq!(
            bad.filter().unwrap_err(),
            ObservabilityError::FilterInvalid {
                key: "log.filter".to_owned()
            }
        );
        let rendered = bad.filter().unwrap_err().to_string();
        assert!(rendered.contains("log.filter") && !rendered.contains("directive"));
        assert!(LogSettings::new().filter().is_ok());
        assert_eq!(FILTER_KEY, "observability.log.filter");
    }
}
