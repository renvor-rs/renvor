//! The redaction rule every emitted field passes through (C-O6, FR-069, FR-070).
//!
//! # A closed denylist, additive by configuration
//!
//! A field whose name matches the built-in list — as a whole, or as its last dot-separated
//! segment, case-insensitively — is emitted as `[REDACTED]`. An author can add names and cannot
//! remove one: the built-in set is what every Renvor crate relies on when it records a field it
//! must never see in a log, and a configuration that could subtract from it would make that
//! reliance conditional.
//!
//! # Values are bounded
//!
//! A rendered value over 1024 bytes is cut at a character boundary and marked, so a body or a
//! blob recorded by mistake cannot become a megabyte of log line.

/// The marker a redacted value is replaced with.
pub const REDACTED: &str = "[REDACTED]";
/// The most bytes of one rendered value that are emitted.
pub const MAX_VALUE_BYTES: usize = 1024;
/// The names that are always redacted. Matched case-insensitively against the whole field name
/// and against its last `.`-separated segment.
pub const BUILT_IN: [&str; 12] = [
    "password",
    "passphrase",
    "secret",
    "token",
    "authorization",
    "cookie",
    "set-cookie",
    "dsn",
    "connection_string",
    "api_key",
    "private_key",
    "credential",
];

/// The rule: the built-in names plus any an author added.
#[derive(Clone, Debug, Default)]
pub struct Redaction {
    extra: Vec<String>,
}

impl Redaction {
    /// The built-in set alone.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a field name to redact. Stored lower-cased; matched like the built-ins.
    #[must_use]
    pub fn with_field(mut self, name: &str) -> Self {
        let name = name.trim().to_ascii_lowercase();
        if !name.is_empty() && !self.extra.contains(&name) {
            self.extra.push(name);
        }
        self
    }

    /// The names added on top of the built-in set.
    #[must_use]
    pub fn extra(&self) -> &[String] {
        &self.extra
    }

    /// Whether a field named `name` is redacted.
    #[must_use]
    pub fn applies_to(&self, name: &str) -> bool {
        let lower = name.to_ascii_lowercase();
        let last = lower.rsplit('.').next().unwrap_or(&lower);
        let matches = |candidate: &str| {
            BUILT_IN.contains(&candidate) || self.extra.iter().any(|added| added == candidate)
        };
        matches(&lower) || matches(last)
    }

    /// The value to emit for field `name` whose rendering is `rendered`: the marker when the name
    /// is redacted, otherwise the rendering bounded to [`MAX_VALUE_BYTES`].
    #[must_use]
    pub fn apply(&self, name: &str, rendered: &str) -> String {
        if self.applies_to(name) {
            return REDACTED.to_owned();
        }
        bounded(rendered)
    }
}

/// `rendered` cut at a character boundary inside [`MAX_VALUE_BYTES`] with a marker, or as is.
#[must_use]
pub fn bounded(rendered: &str) -> String {
    if rendered.len() <= MAX_VALUE_BYTES {
        return rendered.to_owned();
    }
    let mut cut = MAX_VALUE_BYTES;
    while !rendered.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}…[truncated {} bytes]",
        &rendered[..cut],
        rendered.len() - cut
    )
}

#[cfg(test)]
mod tests {
    use super::{BUILT_IN, MAX_VALUE_BYTES, REDACTED, Redaction, bounded};

    #[test]
    fn every_built_in_name_is_redacted_in_any_case_and_as_a_last_segment() {
        let rule = Redaction::new();
        for (index, name) in BUILT_IN.iter().enumerate() {
            assert!(
                rule.applies_to(name),
                "built-in case {index} is not redacted"
            );
            assert!(
                rule.applies_to(&name.to_ascii_uppercase()),
                "built-in case {index} is case-sensitive"
            );
            assert!(
                rule.applies_to(&format!("db.{name}")),
                "built-in case {index} is not matched as a last segment"
            );
        }
        assert!(!rule.applies_to("user_id"));
        assert!(
            !rule.applies_to("tokens_issued"),
            "a superstring is not the name"
        );
        assert_eq!(rule.apply("password", "hunter2CanaryDoNotLeak"), REDACTED);
        assert_eq!(rule.apply("route", "/x"), "/x");
    }

    #[test]
    fn configuration_adds_and_cannot_remove() {
        let rule = Redaction::new().with_field(" Ssn ").with_field("");
        assert!(rule.applies_to("ssn"));
        assert!(rule.applies_to("customer.SSN"));
        assert_eq!(rule.extra(), ["ssn"]);
        // No API removes a built-in; the closest an author can get is adding it again.
        let again = rule.with_field("password");
        assert!(again.applies_to("password"));
    }

    #[test]
    fn values_are_bounded_at_a_character_boundary() {
        let long = "é".repeat(MAX_VALUE_BYTES);
        let cut = bounded(&long);
        assert!(cut.starts_with(&"é".repeat(MAX_VALUE_BYTES / 2 - 1)));
        assert!(cut.contains("…[truncated"));
        assert!(cut.len() < long.len());
        assert_eq!(bounded("short"), "short");
        let exact = "x".repeat(MAX_VALUE_BYTES);
        assert_eq!(bounded(&exact), exact, "the bound is inclusive");
    }
}
