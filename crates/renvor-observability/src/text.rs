//! The human-readable field formatter: `name=value` pairs under the same redaction rule as JSON.
//!
//! Used with `tracing-subscriber`'s full text event format, which — unlike its JSON format —
//! formats event fields through the layer's `FormatFields`, so one formatter covers events and
//! spans here.

use core::fmt;

use tracing::field::{Field, Visit};
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::{FormatFields, FormattedFields};

use crate::redaction::Redaction;

/// `name=value` pairs, space-separated, redacted and bounded.
#[derive(Clone, Debug, Default)]
pub struct TextFields {
    rule: Redaction,
}

impl TextFields {
    /// Fields under `rule`.
    #[must_use]
    pub const fn new(rule: Redaction) -> Self {
        Self { rule }
    }
}

struct TextVisitor<'a, 'w> {
    rule: &'a Redaction,
    writer: Writer<'w>,
    first: bool,
    result: fmt::Result,
}

impl TextVisitor<'_, '_> {
    fn put(&mut self, field: &Field, rendered: &str) {
        if self.result.is_err() {
            return;
        }
        let value = self.rule.apply(field.name(), rendered);
        let separator = if self.first { "" } else { " " };
        self.first = false;
        self.result = if field.name() == "message" {
            write!(self.writer, "{separator}{value}")
        } else {
            write!(self.writer, "{separator}{}={value}", field.name())
        };
    }
}

impl Visit for TextVisitor<'_, '_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.put(field, &format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, value);
    }
}

impl<'writer> FormatFields<'writer> for TextFields {
    fn format_fields<R: RecordFields>(&self, writer: Writer<'writer>, fields: R) -> fmt::Result {
        let mut visitor = TextVisitor {
            rule: &self.rule,
            writer,
            first: true,
            result: Ok(()),
        };
        fields.record(&mut visitor);
        visitor.result
    }

    fn add_fields(
        &self,
        current: &'writer mut FormattedFields<Self>,
        fields: &tracing::span::Record<'_>,
    ) -> fmt::Result {
        let mut buffer = String::new();
        {
            let writer = Writer::new(&mut buffer);
            let mut visitor = TextVisitor {
                rule: &self.rule,
                writer,
                first: true,
                result: Ok(()),
            };
            fields.record(&mut visitor);
            visitor.result?;
        }
        if buffer.is_empty() {
            return Ok(());
        }
        if !current.fields.is_empty() {
            current.fields.push(' ');
        }
        current.fields.push_str(&buffer);
        Ok(())
    }
}
