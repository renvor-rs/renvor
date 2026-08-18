//! The machine-readable envelope (contract C-2).
//!
//! **A compatibility contract.** `schemaVersion` is an integer so a consumer can compare it without
//! parsing, and every field's presence rule is expressed in the types rather than in a comment:
//! `result` and `error` are `Option`, and the two constructors are the only way to build an
//! envelope, so "present iff status is success" is not something a caller can get wrong.

use serde::Serialize;

use crate::exit::CliError;

/// Incremented on any breaking shape change. Never a string.
pub const SCHEMA_VERSION: u32 = 1;

/// The `status` field, as a closed set rather than a free string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The command did what it was asked to do.
    Success,
    /// It did not.
    Failure,
}

/// The structured error half of the envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    /// From the registry. **Stable.**
    pub code: String,
    /// Human-readable. **Not** stable; never parse it.
    pub message: String,
    /// Structured, code-specific.
    pub details: serde_json::Map<String, serde_json::Value>,
}

/// Exactly one of these is written to `stdout` per run, for success and failure alike.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// Integer, so comparison needs no parsing.
    pub schema_version: u32,
    /// `success` or `failure`. Never absent.
    pub status: Status,
    /// The command that ran.
    pub command: String,
    /// Present iff `status` is `success`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Present iff `status` is `failure`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl Envelope {
    /// A success envelope carrying a command-specific result.
    ///
    /// # The result is redacted, and it was not until 2026-08-18
    ///
    /// FR-041 requires redaction in **every** output mode — "human output, JSON output, the dry-run
    /// manifest, error messages". [`Envelope::failure`] redacted; this did not. So one input gave
    /// two different answers:
    ///
    /// ```text
    /// human: created 6 files (879 bytes) in /tmp/token=[redacted]
    /// json : "destination": "/tmp/token=abc123secret/demo"
    /// ```
    ///
    /// The destination is operator-supplied and lands in the success result verbatim, so any
    /// credential in a path — a checkout under a token-bearing directory, a CI workspace — reached
    /// `stdout` in JSON mode and, from there, a log file.
    ///
    /// It survived because `tests/redaction.rs` planted its secrets only in inputs that **fail**,
    /// which exercises `failure` and never `success`. An advisory review found it by running the
    /// same input through both modes and diffing the answers.
    #[must_use]
    pub fn success(command: &str, result: serde_json::Value) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            status: Status::Success,
            command: command.to_owned(),
            result: Some(redact_value(result)),
            error: None,
        }
    }

    /// A failure envelope built from the one error type this program produces.
    ///
    /// Going through [`CliError`] rather than accepting a code and a message separately is what
    /// keeps the JSON registry and the exit-code mapping from drifting: there is one source for
    /// both, and it is [`crate::exit::Code`].
    #[must_use]
    pub fn failure(command: &str, error: &CliError) -> Self {
        let mut details = serde_json::Map::new();
        for (key, value) in &error.details {
            details.insert(
                key.clone(),
                serde_json::Value::String(super::redact::line(value)),
            );
        }
        Self {
            schema_version: SCHEMA_VERSION,
            status: Status::Failure,
            command: command.to_owned(),
            result: None,
            error: Some(ErrorBody {
                code: error.code.as_str().to_owned(),
                // FR-041 applies to the JSON path in full. A machine-readable secret is still a
                // secret, and a tool's log file is where it would end up.
                message: super::redact::line(&error.message),
                details,
            }),
        }
    }
}

/// Applies [`super::redact::line`] to every string in a JSON value, recursively.
///
/// **Keys are redacted as well as values.** A key is a name this program chose, so it should never
/// carry a secret — but "should never" is the reasoning that produced the defect above, and walking
/// both costs nothing.
fn redact_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::String(text) => serde_json::Value::String(super::redact::line(&text)),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(redact_value).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| (super::redact::line(&key), redact_value(value)))
                .collect(),
        ),
        // Numbers, booleans, and null cannot carry a credential shape.
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::Code;

    #[test]
    fn a_success_envelope_carries_a_result_and_no_error() {
        let envelope = Envelope::success("new", serde_json::json!({"files": 14}));
        let value = serde_json::to_value(&envelope).expect("serialises");
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["status"], "success");
        assert_eq!(value["command"], "new");
        assert_eq!(value["result"]["files"], 14);
        assert!(
            value.get("error").is_none(),
            "a success envelope carried an error"
        );
    }

    #[test]
    fn a_failure_envelope_carries_an_error_and_no_result() {
        let error = CliError::new(Code::DestinationNotEmpty, "it exists").with("rule", "r");
        let envelope = Envelope::failure("new", &error);
        let value = serde_json::to_value(&envelope).expect("serialises");
        assert_eq!(value["status"], "failure");
        assert_eq!(value["error"]["code"], "destination_not_empty");
        assert_eq!(value["error"]["details"]["rule"], "r");
        assert!(
            value.get("result").is_none(),
            "a failure envelope carried a result"
        );
    }

    #[test]
    fn the_schema_version_is_an_integer_not_a_string() {
        // Stated in the contract, and the kind of thing a serde attribute change breaks silently.
        let value = serde_json::to_value(Envelope::success("doctor", serde_json::json!({})))
            .expect("serialises");
        assert!(
            value["schemaVersion"].is_u64(),
            "schemaVersion must not be a string"
        );
    }

    #[test]
    fn a_secret_shaped_detail_is_redacted_on_the_json_path_too() {
        // FR-041 explicitly covers JSON. This is the assertion that keeps "machine-readable" from
        // becoming an exemption.
        let error = CliError::new(Code::ToolMissing, "failed with password=hunter2")
            .with("command", "psql --password=hunter2");
        let value = serde_json::to_value(Envelope::failure("doctor", &error)).expect("serialises");
        let message = value["error"]["message"].as_str().expect("a message");
        let detail = value["error"]["details"]["command"]
            .as_str()
            .expect("a detail");
        // Fixed messages, deliberately. Interpolating the rendering here would print the
        // credential into the test log on exactly the run where redaction regressed.
        assert!(
            !message.contains("hunter2"),
            "the error message was not redacted"
        );
        assert!(
            !detail.contains("hunter2"),
            "the error detail was not redacted"
        );
        assert!(message.contains(crate::output::redact::REDACTED));
    }

    #[test]
    fn every_registry_code_serialises_to_its_wire_name() {
        // Guards the registry as a whole rather than sampling it: a new variant without a wire
        // name fails here rather than in a consumer.
        for code in Code::ALL {
            let error = CliError::new(code, "x");
            let value = serde_json::to_value(Envelope::failure("new", &error)).expect("serialises");
            assert_eq!(value["error"]["code"], code.as_str());
        }
    }
}
