//! Renvor's JSON event and field formatters (FR-068, FR-069).
//!
//! # Why the formatter is Renvor's
//!
//! `tracing-subscriber`'s JSON event format serialises event fields straight through
//! `tracing-serde` (`fmt/format/json.rs`, the `flatten_event` branch and the `field_map()` entry
//! beside it) and never calls the layer's `FormatFields`. Span fields, by contrast, are formatted
//! through `FormatFields` into each span's `FormattedFields` extension when the span is created.
//! A redacting `FormatFields` alone therefore covers spans and misses events — measured, and the
//! reason [`JsonEvent`] formats event fields itself with the same rule.
//!
//! # One object per record, fields only
//!
//! ```json
//! {"timestamp":"2026-09-04T10:00:00.000Z","level":"INFO","target":"renvor.jobs",
//!  "message":"job attempt finished","fields":{"attempt":1},
//!  "run_id":"…","spans":[{"name":"renvor.phase","phase":"boot","run_id":"…"}]}
//! ```
//!
//! `message` is the event's `message` field when present. Every other event field is under
//! `fields`; every enclosing span is in `spans`, outermost first, with its fields inline; `run_id`
//! is lifted from the nearest span that carries one (C-O3). Values are the `Debug` renderings
//! `tracing` hands a visitor — numbers and booleans as JSON numbers and booleans, everything else
//! as a JSON string — after redaction and the length bound.

use core::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields, FormattedFields};
use tracing_subscriber::registry::LookupSpan;

use crate::redaction::Redaction;

/// Collects a record's fields into a JSON map under the redaction rule.
struct Collector<'a> {
    rule: &'a Redaction,
    fields: Map<String, Value>,
    message: Option<String>,
}

impl<'a> Collector<'a> {
    fn new(rule: &'a Redaction) -> Self {
        Self {
            rule,
            fields: Map::new(),
            message: None,
        }
    }

    fn put(&mut self, field: &Field, value: Value) {
        let name = field.name();
        if self.rule.applies_to(name) {
            self.fields.insert(
                name.to_owned(),
                Value::String(crate::redaction::REDACTED.to_owned()),
            );
            return;
        }
        let value = match value {
            Value::String(text) => Value::String(crate::redaction::bounded(&text)),
            other => other,
        };
        if name == "message" {
            if let Value::String(text) = value {
                self.message = Some(text);
            }
            return;
        }
        self.fields.insert(name.to_owned(), value);
    }
}

impl Visit for Collector<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, Value::String(format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::String(value.to_owned()));
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(
            field,
            serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number),
        );
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, Value::Bool(value));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        // The closed message of a Renvor error; a foreign error's text is bounded like any value.
        self.put(field, Value::String(value.to_string()));
    }
}

/// Formats span fields as the inner pairs of a JSON object (`"a":1,"b":"x"`), redacted.
///
/// Stored in each span's `FormattedFields`; [`JsonEvent`] wraps the pairs in braces to read them
/// back, so a span with no fields stores an empty string.
#[derive(Clone, Debug, Default)]
pub struct JsonFields {
    rule: Redaction,
}

impl JsonFields {
    /// Fields under `rule`.
    #[must_use]
    pub const fn new(rule: Redaction) -> Self {
        Self { rule }
    }

    fn pairs(&self, fields: &Map<String, Value>) -> String {
        let mut out = String::new();
        for (index, (name, value)) in fields.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&Value::String(name.clone()).to_string());
            out.push(':');
            out.push_str(&value.to_string());
        }
        out
    }
}

impl<'writer> FormatFields<'writer> for JsonFields {
    fn format_fields<R: tracing_subscriber::field::RecordFields>(
        &self,
        mut writer: Writer<'writer>,
        fields: R,
    ) -> fmt::Result {
        let mut collector = Collector::new(&self.rule);
        fields.record(&mut collector);
        if let Some(message) = collector.message {
            collector
                .fields
                .insert("message".to_owned(), Value::String(message));
        }
        writer.write_str(&self.pairs(&collector.fields))
    }

    fn add_fields(
        &self,
        current: &'writer mut FormattedFields<Self>,
        fields: &tracing::span::Record<'_>,
    ) -> fmt::Result {
        let mut collector = Collector::new(&self.rule);
        fields.record(&mut collector);
        if let Some(message) = collector.message {
            collector
                .fields
                .insert("message".to_owned(), Value::String(message));
        }
        let pairs = self.pairs(&collector.fields);
        if pairs.is_empty() {
            return Ok(());
        }
        if !current.fields.is_empty() {
            current.fields.push(',');
        }
        current.fields.push_str(&pairs);
        Ok(())
    }
}

/// Formats one event as one JSON object on one line.
#[derive(Clone, Debug, Default)]
pub struct JsonEvent {
    rule: Redaction,
}

impl JsonEvent {
    /// Events under `rule`.
    #[must_use]
    pub const fn new(rule: Redaction) -> Self {
        Self { rule }
    }
}

/// The span fields stored by [`JsonFields`], read back as a JSON object.
fn span_fields(stored: &str) -> Map<String, Value> {
    if stored.is_empty() {
        return Map::new();
    }
    serde_json::from_str::<Value>(&format!("{{{stored}}}"))
        .ok()
        .and_then(|value| match value {
            Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

impl<S, N> FormatEvent<S, N> for JsonEvent
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        let mut collector = Collector::new(&self.rule);
        event.record(&mut collector);

        let mut record = Map::new();
        record.insert("timestamp".to_owned(), Value::String(rfc3339_now()));
        record.insert("level".to_owned(), Value::String(meta.level().to_string()));
        record.insert("target".to_owned(), Value::String(meta.target().to_owned()));
        if let Some(message) = collector.message {
            record.insert("message".to_owned(), Value::String(message));
        }
        record.insert("fields".to_owned(), Value::Object(collector.fields));

        let mut spans = Vec::new();
        let mut run_id = None;
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                let mut entry = Map::new();
                entry.insert("name".to_owned(), Value::String(span.name().to_owned()));
                let extensions = span.extensions();
                if let Some(stored) = extensions.get::<FormattedFields<N>>() {
                    for (name, value) in span_fields(&stored.fields) {
                        if name == "run_id"
                            && let Value::String(id) = &value
                        {
                            run_id = Some(id.clone());
                        }
                        entry.insert(name, value);
                    }
                }
                spans.push(Value::Object(entry));
            }
        }
        if let Some(run_id) = run_id {
            record.insert("run_id".to_owned(), Value::String(run_id));
        }
        if !spans.is_empty() {
            record.insert("spans".to_owned(), Value::Array(spans));
        }
        writer.write_str(&Value::Object(record).to_string())?;
        writer.write_char('\n')
    }
}

/// The current instant as RFC 3339 UTC with millisecond precision, without a date crate.
fn rfc3339_now() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    rfc3339(now.as_secs(), now.subsec_millis())
}

/// Civil date from days since the epoch (Howard Hinnant's algorithm), then the clock.
fn rfc3339(unix_seconds: u64, millis: u32) -> String {
    let days = i64::try_from(unix_seconds / 86_400).unwrap_or(i64::MAX);
    let seconds_of_day = unix_seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        seconds_of_day / 3_600,
        (seconds_of_day % 3_600) / 60,
        seconds_of_day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::rfc3339;

    #[test]
    fn the_date_formatter_agrees_with_known_instants() {
        assert_eq!(rfc3339(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(951_782_400, 5), "2000-02-29T00:00:00.005Z");
        assert_eq!(rfc3339(1_788_480_000, 999), "2026-09-04T00:00:00.999Z");
        assert_eq!(rfc3339(1_788_504_000, 0), "2026-09-04T06:40:00.000Z");
        assert_eq!(rfc3339(4_102_444_799, 0), "2099-12-31T23:59:59.000Z");
    }
}
