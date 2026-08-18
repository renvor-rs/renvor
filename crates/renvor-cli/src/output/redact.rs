//! Redaction (FR-041).
//!
//! # The honest framing, before any code
//!
//! Phase 003 handles **no secret material**. The configuration is a project name, a destination, a
//! local domain, and a handful of booleans; `renvor.toml` is forbidden from carrying a credential
//! by FR-018, and that is enforced upstream by the configuration type having no field that could
//! hold one.
//!
//! So this module is not defending a known leak. It exists because FR-041 says redaction applies to
//! **every** output mode, and the way that requirement usually fails is not that somebody forgets
//! to redact — it is that a *later* change introduces a value nobody classified, into a code path
//! nobody re-checked.
//!
//! What that means for how this is written: the value is in the **test**, which asserts that every
//! configuration field is inert, and which fails when a new field is added without being classified.
//! A redaction function that never fires proves nothing; a test that notices a new field does.
//!
//! **That test is `config::model::tests::every_configuration_field_is_inert_and_a_new_one_cannot_be_added_unclassified`,
//! and until 2026-08-18 it did not exist** — this paragraph described a guard nobody had written,
//! which an advisory review found by going to look for it. It exists now, and it works by
//! exhaustively destructuring `ProjectConfiguration`, so adding a field is a **compile error**
//! until somebody classifies it.
//!
//! # What is redacted
//!
//! Anything matching a credential-shaped pattern in text bound for `stdout` or `stderr`. The
//! patterns are deliberately few and specific — a broad heuristic that mangles ordinary output
//! trains operators to ignore it, which is worse than not redacting.

/// The marker substituted for redacted material. One string, so a consumer can grep for it.
pub const REDACTED: &str = "[redacted]";

/// Key names whose *value* is replaced wherever a `key=value` or `key: value` pair appears.
///
/// Matched case-insensitively on the whole key. Substring matching was rejected: it turns a field
/// called `token_bucket_size` into `[redacted]` and teaches people the redaction is noise.
const SECRET_KEYS: [&str; 8] = [
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "private_key",
    "authorization",
];

/// Redacts credential-shaped material in a line of output.
///
/// Handles `key=value` and `key: value`, which is what a configuration dump, an environment
/// listing, a command line, and a connection string all reduce to.
///
/// # Why this is a hand-written scanner and not a regex
///
/// The pattern is four lines of index arithmetic and takes no dependency. A regex crate would be
/// 1.3 MB of compiled matcher to find `=` — and the package-first rule asks whether a package is
/// *better*, not merely whether one exists. Recorded here rather than left as an unexplained
/// absence.
///
/// # Why keys are matched whole rather than by substring
///
/// Substring matching turns `token_bucket_size=64` into `[redacted]`, and a redactor that mangles
/// ordinary output gets ignored. Leading `-` is stripped first so `--password=x` on a command line
/// is caught.
#[must_use]
pub fn line(input: &str) -> String {
    /// Characters that may appear in a key. ASCII only, which is what makes the byte indexing
    /// below land on character boundaries: every byte of a multi-byte UTF-8 sequence is >= 0x80,
    /// so none of these ever matches inside one.
    fn is_key_char(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
    }
    /// Characters that end a value.
    fn ends_value(byte: u8) -> bool {
        byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b'"' | b'\'')
    }

    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut cursor = 0usize;

    while cursor < bytes.len() {
        let byte = bytes[cursor];
        if byte != b'=' && byte != b':' {
            cursor += 1;
            continue;
        }

        // The key is the run of key characters immediately before the separator.
        let mut key_start = cursor;
        while key_start > 0 && is_key_char(bytes[key_start - 1]) {
            key_start -= 1;
        }
        let key = input[key_start..cursor].trim_start_matches('-');

        if !SECRET_KEYS
            .iter()
            .any(|secret| secret.eq_ignore_ascii_case(key))
        {
            cursor += 1;
            continue;
        }

        // Everything up to and including the separator survives verbatim.
        out.push_str(&input[..=cursor]);
        let mut value_start = cursor + 1;
        // `key: value` — the space after the separator is part of the formatting, not the secret.
        while value_start < bytes.len() && bytes[value_start] == b' ' {
            value_start += 1;
        }
        out.push_str(&input[cursor + 1..value_start]);
        out.push_str(REDACTED);

        let mut value_end = value_start;
        while value_end < bytes.len() && !ends_value(bytes[value_end]) {
            value_end += 1;
        }

        // Recurse on the remainder so a line carrying two secrets loses both.
        return out + &line(&input[value_end..]);
    }

    input.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_credential_shaped_pair_is_redacted() {
        assert_eq!(line("password=hunter2"), format!("password={REDACTED}"));
        assert_eq!(line("TOKEN: abc123"), format!("TOKEN: {REDACTED}"));
        assert_eq!(
            line("connecting with api_key=sk-live-9 and going on"),
            format!("connecting with api_key={REDACTED} and going on")
        );
    }

    #[test]
    fn ordinary_output_is_left_exactly_alone() {
        // POSITIVE CONTROL, and the more important half. A redactor that mangles normal output
        // gets ignored, and an ignored redactor protects nothing.
        for line_in in [
            "created 14 files in ./commerce",
            "target=api-only",
            "token_bucket_size=64",
            "https://renvor.dev/plan?x=1",
            "bound=template_fuel limit=1000000",
        ] {
            assert_eq!(line(line_in), line_in, "redaction damaged ordinary output");
        }
    }

    #[test]
    fn a_flag_form_and_a_second_secret_on_one_line_are_both_caught() {
        assert_eq!(
            line("psql --password=hunter2 --host=db"),
            format!("psql --password={REDACTED} --host=db")
        );
        assert_eq!(
            line("token=a secret=b"),
            format!("token={REDACTED} secret={REDACTED}")
        );
    }

    #[test]
    fn multibyte_text_survives_intact() {
        // The scanner indexes bytes. Every byte of a multi-byte UTF-8 sequence is >= 0x80, so an
        // ASCII separator can never appear inside one — asserted rather than reasoned about, since
        // getting this wrong is a panic on a slice boundary rather than a wrong answer.
        let input = "créé 3 fichiers — token=sk-1 — terminé";
        let out = line(input);
        // Fixed messages: this input carries a credential, and a failure here means redaction is
        // broken, which is the worst moment to interpolate the value into a log.
        assert!(
            out.contains("créé 3 fichiers"),
            "text before the secret was damaged"
        );
        assert!(out.contains("terminé"), "text after the secret was damaged");
        assert!(!out.contains("sk-1"), "the secret survived redaction");
    }

    #[test]
    fn the_marker_is_a_single_greppable_string() {
        assert_eq!(REDACTED, "[redacted]");
    }
}
