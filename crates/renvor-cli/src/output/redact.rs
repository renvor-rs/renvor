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

/// Escapes a path for a **human** stream: **every** control character, `\n` and `\t` included,
/// via [`escape_every_control_character`].
///
/// Applied at the point a path is interpolated into a message, because that is where the program
/// knows which half of the text came from outside it. [`for_terminal`] is a second, more permissive
/// pass over the assembled line; it is a backstop, not this.
///
/// # The finding this exists for, reproduced before it was fixed
///
/// An advisory security review found that `--path` components other than the **final** one reach
/// the terminal with their bytes intact. `Destination::open`'s RULE 1b refuses a control character
/// in the directory renvor is about to *create*, but a directory that already exists two levels up
/// — planted, extracted from an archive, or on a shared mount — is opened by RULE 3 and its name is
/// then interpolated into the success message, the `destination_exists` message, and the
/// `destination_parent_missing` message.
///
/// Measured with `od -c`, not inferred:
///
/// ```text
/// c r e a t e d   6   f i l e s   ( 8 7 1   b y t e s )   i n   o
/// u t 2 / 033 [ 3 1 m R E D 033 [ 0 m / d e m o \n
/// ```
///
/// Those `033` bytes are `ESC`. In a real terminal that line renders in red rather than showing the
/// text, and other sequences can move the cursor, erase what is above them, or — with OSC — reach
/// the window title and, on some terminals, the clipboard. So the operator can be shown a
/// destination that is not the one on disk, which is **precisely** the reasoning RULE 1b's own
/// message gives for refusing control characters. The program already agreed this mattered; it
/// enforced it in one place and not the other.
///
/// # Newline and tab are escaped here, and that is the point
///
/// The first fix for this finding exempted them, to keep multi-line output readable, and that left
/// a real hole: a newline in an ancestor directory ends the line the operator is reading and starts
/// one an attacker wrote, which forges a log entry as effectively as any escape sequence.
///
/// Nothing had to be traded to close it. The exemption belongs on the **assembled line**, where
/// multi-line output legitimately lives, not on the **path**, which is never legitimately
/// multi-line. So this escapes everything, [`for_terminal`] keeps the exemption, and a dry run's
/// file list and cargo's embedded diagnostics still print as they should.
///
/// # Why not in [`line()`]
///
/// [`line()`] is applied to the JSON path as well (see [`super::json`]), where `serde_json` already
/// escapes these bytes correctly. Escaping them a second time would put a literal `\u{1b}` into a
/// JSON string and then escape *its* backslash — corrupting the contract in order to fix a problem
/// that path does not have. On the machine-readable side the bytes stay raw, which is what keeps
/// `details.destination` a path a consumer can act on.
///
/// # Why escaped rather than stripped
///
/// Stripping makes the displayed path silently different from the real one, which is the same lie
/// told more quietly. `\u{1b}` shows the operator exactly what is there.
#[must_use]
pub fn path(path: &std::path::Path) -> String {
    escape_every_control_character(&path.display().to_string())
}

/// Escapes a `details` value for a **human** stream: **every** control character, `\n` and `\t`
/// included, via [`escape_every_control_character`] — the same mechanism [`path`] uses.
///
/// Strict for the same reason: a detail value is a path, a rule name, an OS error, or a flag —
/// never legitimately multi-line, so there is nothing to trade away. The JSON path does **not** use
/// this; there the value stays raw and `serde_json` escapes it, which is what keeps
/// `details.destination` the exact bytes a consumer needs.
#[must_use]
pub fn detail(value: &str) -> String {
    escape_every_control_character(value)
}

/// Escapes **every** control character, newline and tab included.
///
/// Used for values that come from outside the program — a path the operator typed, or a name a
/// directory on disk happens to have. None of them is ever legitimately multi-line, so there is
/// nothing to trade away, and a newline is as good a forgery tool as an escape sequence: it ends
/// the line the operator is reading and starts one the attacker wrote.
fn escape_every_control_character(input: &str) -> String {
    if !input.chars().any(char::is_control) {
        return input.to_owned();
    }
    input
        .chars()
        .flat_map(|character| {
            let escaped: Box<dyn Iterator<Item = char>> = if character.is_control() {
                Box::new(character.escape_debug())
            } else {
                Box::new(std::iter::once(character))
            };
            escaped
        })
        .collect()
}

/// The **backstop**: neutralises terminal control sequences in an already-assembled human line.
///
/// # Second line of defence, and deliberately more permissive than [`path`]
///
/// Untrusted values are escaped where they are interpolated — [`path`] for a path, [`detail`] for a
/// `details` value. This runs afterwards, over the whole line, and catches anything a site forgot;
/// `no_production_site_interpolates_a_raw_path_into_a_message` is what stops a site forgetting in
/// the first place.
///
/// **`\n` and `\t` survive here, on purpose.** By this point the line may legitimately be
/// multi-line: a dry run lists the files it would create, and `generate::verify` embeds cargo's own
/// diagnostics verbatim. Flattening those would make the backstop the thing that ruins ordinary
/// output, which is how a safety measure gets switched off. The exemption is safe precisely because
/// it is scoped to the assembled line — the untrusted *values* inside it have already had their
/// newlines and tabs escaped by [`path`] and [`detail`].
#[must_use]
pub fn for_terminal(input: &str) -> String {
    // Fast path: the overwhelming majority of lines have nothing to escape, and allocating a new
    // string for every one of them to change nothing is waste with no upside.
    if !input.chars().any(needs_escaping) {
        return input.to_owned();
    }
    input
        .chars()
        .flat_map(|character| {
            // `escape_debug` renders ESC as `\u{1b}` and is `char`-aware, so the C1 range
            // (U+0080–U+009F) — which `is_control` also covers — is handled without byte fiddling.
            let escaped: Box<dyn Iterator<Item = char>> = if needs_escaping(character) {
                Box::new(character.escape_debug())
            } else {
                Box::new(std::iter::once(character))
            };
            escaped
        })
        .collect()
}

/// Every control character except the two that carry meaning in renvor's own output.
fn needs_escaping(character: char) -> bool {
    character.is_control() && character != '\n' && character != '\t'
}

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
    ///
    /// The backtick is here because renvor frames the offending text in backticks
    /// (``error: `--output token=abc` is not supported``). Without it the scan ran past the
    /// closing delimiter and swallowed it, so a redacted message came out with unbalanced framing.
    fn ends_value(byte: u8) -> bool {
        byte.is_ascii_whitespace() || matches!(byte, b',' | b'}' | b'"' | b'\'' | b'`')
    }

    /// Whether a byte opens (and therefore closes) a quoted value.
    fn is_quote(byte: &u8) -> bool {
        matches!(byte, b'"' | b'\'')
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

        // A value that BEGINS with a quote is a quoted value, not an empty one. Falling through to
        // the terminator scan below would match that opening quote immediately, consume nothing,
        // and hand the untouched secret to the recursion — which then printed it verbatim, after
        // the `[redacted]` marker had already been emitted. A line carrying the marker *and* the
        // secret is worse than an unredacted one: it is what a log scrubber treats as clean.
        //
        // `password="hunter2"` is the ordinary shape of a connection string, a shell echo, and a
        // TOML or JSON fragment, so this was the common case rather than the exotic one.
        let mut value_end = value_start;
        if let Some(quote) = bytes.get(value_start).copied().filter(is_quote) {
            // Past the opening quote, then to the matching close — or to the end of the line if
            // the quote is never closed, which still leaves no residue.
            value_end += 1;
            while value_end < bytes.len() && bytes[value_end] != quote {
                value_end += 1;
            }
            if value_end < bytes.len() {
                value_end += 1;
            }
        } else {
            while value_end < bytes.len() && !ends_value(bytes[value_end]) {
                value_end += 1;
            }
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
    fn a_quoted_credential_value_leaves_no_residue() {
        // The marker was emitted while the secret was still on the line. `ends_value` counts a
        // quote as a terminator, so for a value that *begins* with one the scan consumed nothing,
        // and the recursion re-printed the untouched secret immediately after `[redacted]`.
        //
        // That is worse than no redaction at all: an operator, or a log scrubber grepping for the
        // marker, reads it as proof the line was cleaned and forwards it with the secret intact.
        for quoted in [
            "password=\"hunter2\"",
            "password='hunter2'",
            "TOKEN: \"hunter2\"",
        ] {
            let redacted = line(quoted);
            assert!(
                !redacted.contains("hunter2"),
                "the secret survived redaction of a quoted value"
            );
            assert!(redacted.contains(REDACTED), "the marker is missing");
        }

        assert_eq!(line("token=\"abc\" and more"), format!("token={REDACTED} and more"));
        // Unterminated quote: consume to the end rather than leaving a tail behind.
        assert!(!line("password=\"hunter2").contains("hunter2"));

        // POSITIVE CONTROL. Redaction must still leave an ordinary quoted value alone, or these
        // assertions would be satisfied by a function that redacted everything.
        assert_eq!(line("token_bucket_size=\"64\""), "token_bucket_size=\"64\"");
    }

    #[test]
    fn redaction_preserves_the_message_delimiters() {
        // renvor frames the offending text in backticks. A backtick was not a value terminator, so
        // the scan ran past the closing one and swallowed it, unbalancing the message.
        let framed = "error: `--output token=abc` is not supported";
        let redacted = line(framed);
        assert_eq!(
            redacted.matches('`').count(),
            framed.matches('`').count(),
            "redaction changed the number of framing backticks: {redacted}"
        );
        assert!(!redacted.contains("abc"));
    }

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

    #[test]
    fn a_terminal_escape_sequence_is_neutralised() {
        // THE REPRODUCTION, as a test. Measured before the fix with `od -c`: the raw ESC bytes
        // reached the terminal, so the destination rendered in red rather than as text.
        let hostile = "created 6 files in out/\u{1b}[31mRED\u{1b}[0m/demo";
        let safe = for_terminal(hostile);
        // ── THE MESSAGES IN THIS MODULE'S TESTS DELIBERATELY PRINT NOTHING BACK ──────
        //
        // `renvor-core`'s `diagnostics` suite refuses an interpolated rendering in an assertion
        // message inside a credential-handling file, and it caught these tests when they were first
        // written. Its reasoning is exact: on a redaction regression the value being asserted about
        // IS the leaked credential, so the failure message would print it into the test log — the
        // one run where that matters most. Fixed text, or an index, instead.
        assert!(
            !safe.contains('\u{1b}'),
            "an ESC byte survived into human output"
        );
        assert!(
            safe.contains("\\u{1b}[31m"),
            "the sequence must be shown, not silently removed — an operator who cannot see it \
             cannot tell the path is not what it looks like"
        );
    }

    #[test]
    fn the_whole_control_range_is_covered_not_just_escape() {
        for (index, (character, _name)) in [
            ('\u{1b}', "ESC"),
            ('\r', "CR"),
            ('\u{7}', "BEL"),
            ('\u{8}', "backspace"),
            ('\u{b}', "vertical tab"),
            ('\u{c}', "form feed"),
            ('\u{0}', "NUL"),
            ('\u{9b}', "C1 CSI"),
        ]
        .into_iter()
        .enumerate()
        {
            let out = for_terminal(&format!("a{character}b"));
            // The INDEX, not the rendering — see the note in the test above. The names are kept in
            // the table so a reader can see WHAT is covered without the failure printing it.
            assert!(
                !out.contains(character),
                "the control character at index {index} of this list survived"
            );
        }
    }

    #[test]
    fn newline_and_tab_survive_because_renvors_own_messages_use_them() {
        // NEGATIVE CONTROL, and a deliberate limitation rather than an oversight. `generate::verify`
        // embeds cargo's stderr verbatim; escaping `\n` would turn a compiler error into one
        // unreadable line. A neutraliser that mangles ordinary output gets switched off.
        let multiline = "the generated project does not compile:\nerror[E0425]:\n\tnot found";
        assert_eq!(
            for_terminal(multiline),
            multiline,
            "newline and tab must pass through untouched"
        );
    }

    #[test]
    fn ordinary_output_is_returned_unchanged() {
        // POSITIVE CONTROL. A function that rewrote everything would satisfy the assertions above
        // and would make every message unreadable.
        for (index, ordinary) in [
            "created 6 files (879 bytes) in /tmp/demo",
            "next: cd demo && cargo run",
            "error: `/tmp/demo` already exists (it is a directory)",
            "a path with spaces and punctuation: my.project-2_final!",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                for_terminal(ordinary),
                ordinary,
                "the ordinary line at index {index} of this list was altered"
            );
        }
    }

    #[test]
    fn redaction_and_neutralisation_compose_in_the_order_the_reporter_uses() {
        // Both are needed and neither subsumes the other: one hides a value, the other stops the
        // text reprogramming the terminal. This asserts the composition the `Reporter` performs.
        let hostile = "token=secret\u{1b}[31m";
        let out = for_terminal(&line(hostile));
        // Printing `out` here would print the secret on exactly the failure that matters.
        assert!(out.contains(REDACTED), "the secret was not redacted");
        assert!(!out.contains('\u{1b}'), "the escape sequence survived");
    }

    #[test]
    fn no_production_site_interpolates_a_raw_path_into_a_message() {
        // ── THE GUARD THAT STOPS G/S-1 COMING BACK ────────────────────────────────────
        //
        // The fix for G/S-1 was applied at seventeen interpolation sites. A guard that only checked
        // those seventeen would be a list of the places somebody already thought of; what matters is
        // the eighteenth, added later by someone who has not read this file.
        //
        // So this scans the crate's own production source for `.display()` and requires every use to
        // be one of two things:
        //
        //   - `redact::path(...)`, which escapes it — the message case;
        //   - `.with(...)` / a JSON value, where the path stays RAW on purpose, because
        //     `details.destination` must be the bytes a consumer can act on and `serde_json`
        //     escapes them itself.
        //
        // A source scan is a blunt instrument and is used knowingly. It cannot see a path that
        // reaches a message through a variable, so it is a backstop for the common shape rather
        // than a proof. `Reporter`'s `for_terminal` is the other backstop, and the end-to-end tests
        // in `tests/redaction.rs` are what actually establish the property.
        const SOURCES: [(&str, &str); 8] = [
            ("paths.rs", include_str!("../paths.rs")),
            ("generate/place.rs", include_str!("../generate/place.rs")),
            ("generate/render.rs", include_str!("../generate/render.rs")),
            ("commands/new.rs", include_str!("../commands/new.rs")),
            ("commands/check.rs", include_str!("../commands/check.rs")),
            ("commands/dev.rs", include_str!("../commands/dev.rs")),
            ("commands/docker.rs", include_str!("../commands/docker.rs")),
            ("config/model.rs", include_str!("../config/model.rs")),
        ];

        let mut inspected = 0usize;
        let mut offending = Vec::new();

        for (name, source) in SOURCES {
            // Production code only. Everything from `#[cfg(test)]` onward is a test module, where a
            // raw path in a panic message is fine — nobody is attacking their own fixture.
            let production = source.split("#[cfg(test)]").next().unwrap_or(source);
            for (number, text) in production.lines().enumerate() {
                let code = text.split("//").next().unwrap_or("");
                // BOTH forms count as an inspected site. Counting only `.display()` would make the
                // control weaken itself as the fix spread: every site converted to `redact::path`
                // stops containing `.display()`, so the tally fell as coverage rose. Caught by the
                // control firing the first time this test ran.
                let is_site = code.contains(".display()") || code.contains("redact::path(");
                if !is_site {
                    continue;
                }
                inspected += 1;
                // Raw is correct in a `details` value or a JSON field: see the note above.
                if code.contains("redact::path") {
                    continue;
                }
                let raw_is_intended = code.contains(".with(")
                    || code.contains("\"destination\":")
                    || code.contains(".to_string()");
                if raw_is_intended {
                    continue;
                }
                offending.push(format!("{name}:{}: {}", number + 1, text.trim()));
            }
        }

        // POSITIVE CONTROL, FIRST. A scan that matched nothing would pass the assertion below while
        // verifying nothing — the exact defect an advisory review found elsewhere in this phase.
        assert!(
            inspected >= 15,
            "the scan only inspected {inspected} `.display()` sites across {} files, which is far \
             fewer than this crate has — it is no longer looking at what it claims to",
            SOURCES.len()
        );
        assert!(
            offending.is_empty(),
            "a path is interpolated into a message without `redact::path`, which is how a control \
             character from a directory name reaches the operator's terminal (finding G/S-1). \
             Offending sites: {offending:#?}"
        );
    }
}
