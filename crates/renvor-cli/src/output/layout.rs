//! The shape of human output, and the responsive layout that renders it.
//!
//! # A [`Report`] cannot be turned into text without being redacted
//!
//! That is the point of the type, and it is why commands no longer build their human output with
//! `format!`. A [`Report`] holds structure and raw values; the only function that produces a
//! `String` from one is [`Report::render`], and it redacts and neutralises **every dynamic field**
//! on the way past. There is no second path, no `Display`, and no accessor that hands out the
//! values — so a command cannot forget, and a future command cannot be written in a way that
//! forgets.
//!
//! The previous design passed a finished `String` to the reporter, which redacted the whole thing
//! at the end. That was correct while nothing was styled. It stops being correct the moment
//! styling exists: escape sequences added before redaction get escaped by it (turning `\x1b[32m`
//! into visible `\x1b[32m`), and escape sequences added after it are indistinguishable from an
//! attacker's. Structure resolves the ordering by construction — redact, then measure, then style.
//!
//! # Ordering, stated once because everything else depends on it
//!
//! 1. **Redact.** `redact::line` replaces credential-shaped values.
//! 2. **Neutralise.** `redact::for_terminal` escapes control characters, so nothing left in the
//!    string can move the cursor, change a colour, or forge a line.
//! 3. **Measure.** Display width is computed on the neutralised text, because that is what will
//!    actually occupy columns — an escaped `\x1b` is four columns, not one.
//! 4. **Style.** Escape sequences are added last, around text that is already safe, and are never
//!    counted as width.
//!
//! Styling trusted punctuation never makes an untrusted value trusted: the value was made safe in
//! step 2, before anything decided what colour to paint the dots beside it.

use unicode_width::UnicodeWidthStr;

use super::redact;
use super::style::{Permission, Role};

/// The narrowest run of dots that still reads as a leader rather than as punctuation.
///
/// One dot is a full stop and two are an ellipsis that lost a dot. Three is the smallest run a
/// reader parses as "this is spacing", which is the property the threshold is really about.
const MINIMUM_LEADERS: usize = 3;

/// The width a stream is assumed to have when it is not a terminal.
///
/// Deliberately fixed rather than absent, so that piped output is **byte-identical** between a
/// developer's machine and CI. The expected-output tests depend on that; so does anyone diffing
/// two captured runs.
const ASSUMED_WIDTH: usize = 80;

/// The widest a leader run is allowed to make a line, however wide the terminal is.
///
/// A 200-column terminal would otherwise produce 180 dots between a label and its value, and a
/// reader tracking a value back to its label across that distance is doing the work the leader was
/// supposed to do for them. Typography caps line measure for the same reason. The test harness
/// runs its pseudo-terminal at 1000 columns, which makes the cap load-bearing rather than
/// cosmetic.
const MAXIMUM_MEASURE: usize = 100;

/// A status label. The word is the signal; the colour is decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Something the reader did not ask about and should know.
    Info,
    /// Something completed.
    Done,
    /// Something is not as expected and the command continued.
    Warn,
    /// Something failed.
    Error,
}

impl Status {
    /// The word. **Never** localised, never abbreviated, never replaced by a symbol.
    const fn word(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Done => "DONE",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }

    const fn role(self) -> Role {
        match self {
            Self::Info => Role::Information,
            Self::Done => Role::Success,
            Self::Warn => Role::Warning,
            Self::Error => Role::Error,
        }
    }
}

/// A state token sitting at the right-hand end of a row.
///
/// # Why this is not [`Status`]
///
/// A status *labels a message*: `INFO  something happened`. A mark *classifies a row's value*:
/// `cargo .......... 1.94.0  OK`. They have different vocabularies because they answer different
/// questions, and folding them together would force one of the two to use a word that does not
/// mean what it says — a missing tool reported as `ERROR` when nothing has failed, or an outdated
/// one reported as `WARN` when the row is not a message at all.
///
/// Every word here is meaningful with no colour, which is the same rule [`Status`] follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Present and usable.
    Ok,
    /// Present and too old to use.
    Outdated,
    /// Required and not present.
    Missing,
    /// Optional and not present. Not a problem — which is why it is muted rather than coloured.
    Absent,
}

impl Mark {
    /// The word.
    const fn word(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Outdated => "TOO OLD",
            Self::Missing => "MISSING",
            Self::Absent => "ABSENT",
        }
    }

    const fn role(self) -> Role {
        match self {
            Self::Ok => Role::Success,
            Self::Outdated => Role::Warning,
            Self::Missing => Role::Error,
            Self::Absent => Role::Muted,
        }
    }
}

/// The width the mark column is padded to, so a run of rows ends in one straight edge.
///
/// `MISSING` is the longest word at seven columns. A shorter one is padded on the **left**, so the
/// words end together rather than starting together — the same reason a column of numbers is
/// right-aligned.
const MARK_COLUMN: usize = 7;

/// The column at which a status message begins.
///
/// `ERROR` is the longest word at five columns, plus two spaces. Every message therefore starts in
/// the same column whichever label precedes it, which is what makes a run of them scannable.
const STATUS_COLUMN: usize = 7;

/// One line of human output.
#[derive(Debug, Clone)]
enum Line {
    /// A blank line, used for grouping. Carries nothing, so nothing to redact.
    Blank,
    /// A labelled status message.
    Status(Status, String),
    /// A label and its value, joined by leader dots when the width allows.
    Row {
        label: String,
        value: String,
        mark: Option<Mark>,
    },
    /// An indented item in a list.
    Item(String),
    /// Prose. Used where a sentence is the right shape and a row is not.
    Text(String),
}

/// A structured human result, ready to be rendered by whoever owns the stream.
///
/// # There is deliberately no `heading`
///
/// [`Role::Heading`] exists and is used — the argument parser's help styles use it — but nothing
/// in this crate's own output has a section to head. A status line already introduces its group,
/// and a heading beside it would be two ways of saying the same thing. The variant was written,
/// found to have one caller, that caller was better as a plain line, and it was removed rather
/// than kept in case somebody wants it.
#[derive(Debug, Clone, Default)]
pub struct Report {
    lines: Vec<Line>,
}

impl Report {
    /// An empty report.
    #[must_use]
    pub const fn new() -> Self {
        Self { lines: Vec::new() }
    }

    /// Adds a blank line.
    #[must_use]
    pub fn blank(mut self) -> Self {
        self.lines.push(Line::Blank);
        self
    }

    /// Adds a labelled status message.
    #[must_use]
    pub fn status(mut self, status: Status, message: impl Into<String>) -> Self {
        self.lines.push(Line::Status(status, message.into()));
        self
    }

    /// Adds a label/value row.
    #[must_use]
    pub fn row(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.lines.push(Line::Row {
            label: label.into(),
            value: value.into(),
            mark: None,
        });
        self
    }

    /// Adds a label/value row that ends in a state token.
    #[must_use]
    pub fn row_marked(
        mut self,
        label: impl Into<String>,
        value: impl Into<String>,
        mark: Mark,
    ) -> Self {
        self.lines.push(Line::Row {
            label: label.into(),
            value: value.into(),
            mark: Some(mark),
        });
        self
    }

    /// Adds an indented list item.
    #[must_use]
    pub fn item(mut self, item: impl Into<String>) -> Self {
        self.lines.push(Line::Item(item.into()));
        self
    }

    /// Adds a line of prose.
    #[must_use]
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.lines.push(Line::Text(text.into()));
        self
    }

    /// Whether this report would render to nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Renders to text: redacted, neutralised, laid out, and then styled.
    ///
    /// `available` is the stream's width, or [`None`] when it is not a terminal.
    #[must_use]
    pub fn render(&self, permission: Permission, available: Option<usize>) -> String {
        let width = measure(available);
        self.lines
            .iter()
            .map(|line| render_line(line, permission, width))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The width the layout will actually use.
fn measure(available: Option<usize>) -> usize {
    available.unwrap_or(ASSUMED_WIDTH).min(MAXIMUM_MEASURE)
}

/// The width `text` will occupy once printed.
///
/// **Not** `len()`, which counts bytes, and **not** `chars().count()`, which counts scalar values.
/// `日本語` is three characters and six columns. A combining acute accent is one character and
/// zero columns, because it is drawn on top of the letter before it. Aligning a column with either
/// of the wrong answers produces a visibly ragged edge on exactly the input a reader is least able
/// to work around.
fn width_of(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Redacts and neutralises one dynamic value. Everything dynamic goes through here, once.
///
/// # `detail`, not `for_terminal`, and the difference is a newline
///
/// [`redact::for_terminal`] deliberately leaves `\n` and `\t` alone, because it is applied to
/// whole messages where a newline is the author's own paragraph break. **Every field of a
/// [`Report`] is single-line by construction**, so here a newline can only have come from a value,
/// and a value that ends the reader's line and starts one of its own is the whole forgery. So this
/// uses the strict escape.
///
/// A caller that genuinely needs two lines adds two [`Line::Text`] entries. That is more typing
/// and it is the point: the multi-line case becomes a decision somebody made rather than a
/// property some value turned out to have.
///
/// Redaction runs **first**, on the raw text, because a credential is recognised by its
/// `key=value` shape — and escaping the control characters first can change that shape.
fn safe(value: &str) -> String {
    redact::detail(&redact::line(value))
}

fn render_line(line: &Line, permission: Permission, width: usize) -> String {
    match line {
        Line::Blank => String::new(),
        Line::Status(status, message) => {
            let label = permission.paint(status.role(), status.word());
            // Padding is computed from the WORD, not from the painted string: escape sequences
            // occupy no columns, so measuring the painted text would indent by the length of the
            // colour code.
            let padding = STATUS_COLUMN.saturating_sub(width_of(status.word()));
            format!("{label}{:padding$}{}", "", safe(message))
        }
        Line::Row { label, value, mark } => {
            row(&safe(label), &safe(value), *mark, permission, width)
        }
        Line::Item(item) => format!("  {}", permission.paint(Role::Muted, &safe(item))),
        Line::Text(text) => safe(text),
    }
}

/// Lays out one label/value pair, with leaders when they fit and stacked when they do not.
///
/// # The threshold is derived from the content, not from a column count
///
/// A fixed "80 columns" rule is wrong in both directions: a short label and a short value align
/// happily at 40, and a long path with a long label does not fit at 100. What actually has to hold
/// is that the label, the value, and a run of dots long enough to read as a leader all fit — so
/// that is what is checked.
///
/// # Nothing is ever truncated
///
/// The fallback is to stack, never to cut. A value here may be a destination path or a tool
/// version, and a path shortened to fit is a path that looks like a different path — which is the
/// specific failure this branch exists to avoid. A stacked value that is still too long is left
/// for the terminal to wrap, which is lossless.
fn row(
    label: &str,
    value: &str,
    mark: Option<Mark>,
    permission: Permission,
    width: usize,
) -> String {
    // The mark is padded on the left to a fixed column so a run of rows ends in one straight edge,
    // and it is measured with its padding because that is what occupies the terminal.
    let (mark_text, mark_width) = match mark {
        None => (String::new(), 0),
        Some(mark) => {
            let word = width_of(mark.word());
            let padding = MARK_COLUMN.saturating_sub(word);
            (
                format!(
                    "  {:padding$}{}",
                    "",
                    permission.paint(mark.role(), mark.word())
                ),
                // `max`, NOT the constant. Every word today is at most `MARK_COLUMN` wide, so the
                // two agree — but a longer one would make the padding zero while this still
                // claimed `MARK_COLUMN`, and the leader run would be computed from space that is
                // not there. The row would overflow the terminal **silently**: no panic, just a
                // wrapped line and a broken column.
                2 + MARK_COLUMN.max(word),
            )
        }
    };

    let label_width = width_of(label);
    let value_width = width_of(value);
    // One space on each side of the leaders, plus the shortest run that reads as a leader.
    let required = label_width
        .saturating_add(value_width)
        .saturating_add(mark_width)
        .saturating_add(2)
        .saturating_add(MINIMUM_LEADERS);

    if width < required {
        // Stacked. The label keeps its own line and the value is indented under it, so the pairing
        // survives even at a width where nothing can share a line. The mark stays with the value,
        // because a state token separated from what it classifies classifies nothing.
        return format!(
            "{}\n  {}{}",
            permission.paint(Role::Value, label),
            permission.paint(Role::Value, value),
            mark_text
        );
    }

    // Guaranteed to be at least `MINIMUM_LEADERS` by the branch above; saturating anyway, because
    // a future edit to the branch must not be able to turn this into a panic on a narrow terminal.
    let leaders = width
        .saturating_sub(label_width)
        .saturating_sub(value_width)
        .saturating_sub(mark_width)
        .saturating_sub(2);
    format!(
        "{} {} {}{}",
        permission.paint(Role::Value, label),
        permission.paint(Role::Muted, &".".repeat(leaders)),
        permission.paint(Role::Value, value),
        mark_text
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Styling off, which is what every layout assertion wants: the geometry, not the colours.
    fn plain() -> Permission {
        Permission::denied()
    }

    #[test]
    fn a_row_fills_exactly_the_available_width() {
        let rendered = Report::new()
            .row("Project name", "demo")
            .render(plain(), Some(40));
        assert_eq!(width_of(&rendered), 40, "{rendered:?}");
        assert!(rendered.starts_with("Project name ."));
        assert!(rendered.ends_with(". demo"));
    }

    #[test]
    fn a_row_stacks_rather_than_truncating_when_it_cannot_fit() {
        // 20 columns cannot hold a 12-column label, a 24-column value, and a leader.
        let value = "/very/long/destination/path";
        let rendered = Report::new()
            .row("Project name", value)
            .render(plain(), Some(20));
        assert_eq!(rendered, format!("Project name\n  {value}"));
        assert!(
            rendered.contains(value),
            "the value must survive intact — a shortened path is a different path"
        );
    }

    #[test]
    fn security_relevant_values_are_never_shortened_to_fit() {
        // The rule stated as a property rather than as an example: for every width from 1 to 120,
        // the whole value appears in the output. A layout that ever truncates fails here.
        let value = "/home/operator/.config/renvor/credentials.toml";
        for width in 1_usize..=120 {
            let rendered = Report::new()
                .row("Destination", value)
                .render(plain(), Some(width));
            assert!(
                rendered.contains(value),
                "width {width} truncated the value: {rendered:?}"
            );
        }
    }

    #[test]
    fn no_width_from_zero_upward_panics_or_underflows() {
        // The arithmetic subtracts widths from widths. At width 0 every one of those subtractions
        // is negative, and `usize` has no negative — so this is the test that the saturation is
        // real rather than assumed.
        for width in 0_usize..=8 {
            let rendered = Report::new()
                .row("A very long label indeed", "and a very long value indeed")
                .status(Status::Error, "boom")
                .item("item")
                .render(plain(), Some(width));
            assert!(!rendered.is_empty());
        }
        // And the same for a width the layout will cap.
        let wide = Report::new()
            .row("x", "y")
            .render(plain(), Some(usize::MAX));
        assert_eq!(width_of(&wide), MAXIMUM_MEASURE);
    }

    #[test]
    fn wide_characters_are_measured_in_columns_and_not_in_characters() {
        // `日本語ドメイン` is 7 characters and 14 columns. A layout counting characters would
        // overshoot the right edge by exactly 7.
        let value = "日本語ドメイン";
        assert_eq!(value.chars().count(), 7);
        assert_eq!(width_of(value), 14);
        let rendered = Report::new().row("Domain", value).render(plain(), Some(50));
        assert_eq!(width_of(&rendered), 50, "{rendered:?}");
    }

    #[test]
    fn combining_marks_occupy_no_columns_of_their_own() {
        // "é" spelled as `e` + U+0301 is two characters and one column. A layout counting
        // characters would undershoot and leave the value hanging off the edge.
        let value = "cafe\u{301}.test";
        assert_eq!(value.chars().count(), 10);
        assert_eq!(width_of(value), 9);
        let rendered = Report::new().row("Domain", value).render(plain(), Some(40));
        assert_eq!(width_of(&rendered), 40, "{rendered:?}");
    }

    #[test]
    fn a_leader_is_never_shorter_than_a_leader() {
        // The threshold's whole job: if there is not room for a run that reads as spacing, stack
        // instead of emitting one or two dots that read as punctuation.
        for width in 0_usize..=200 {
            let rendered = Report::new()
                .row("Label", "value")
                .render(plain(), Some(width));
            if let Some(dots) = rendered.split(' ').nth(1)
                && dots.starts_with('.')
            {
                assert!(
                    dots.len() >= MINIMUM_LEADERS,
                    "width {width} produced {dots:?}"
                );
            }
        }
    }

    #[test]
    fn a_non_terminal_stream_lays_out_at_a_fixed_width() {
        // Byte-identical output between a laptop and CI is what the expected-output tests rest on.
        let rendered = Report::new().row("Label", "value").render(plain(), None);
        assert_eq!(width_of(&rendered), ASSUMED_WIDTH);
    }

    #[test]
    fn status_messages_all_begin_in_the_same_column() {
        for status in [Status::Info, Status::Done, Status::Warn, Status::Error] {
            let rendered = Report::new()
                .status(status, "message")
                .render(plain(), None);
            assert_eq!(
                rendered.find("message"),
                Some(STATUS_COLUMN),
                "{status:?} misaligned: {rendered:?}"
            );
        }
    }

    #[test]
    fn a_status_label_is_readable_with_no_colour_at_all() {
        // The accessibility rule, asserted rather than asserted-about: with styling denied, the
        // word survives and the meaning with it.
        let rendered = Report::new()
            .status(Status::Error, "it failed")
            .render(plain(), None);
        assert!(rendered.starts_with("ERROR"), "{rendered:?}");
        assert!(!rendered.contains('\u{1b}'));
    }

    #[test]
    fn every_dynamic_field_is_neutralised_on_the_way_out() {
        // The property the type exists for. An escape sequence in a value must not reach the
        // terminal as an escape sequence, whichever line kind carried it — and there is no way to
        // render a report that skips this.
        let hostile = "value\u{1b}[31mred\nforged";
        let rendered = Report::new()
            .row("Label", hostile)
            .status(Status::Info, hostile)
            .item(hostile)
            .text(hostile)
            .render(plain(), Some(200));
        assert!(
            !rendered.contains('\u{1b}'),
            "an escape survived: {rendered:?}"
        );
        assert_eq!(
            rendered.lines().count(),
            4,
            "four report lines in, four lines out — a value cannot add one: {rendered:?}"
        );
    }

    #[test]
    fn styling_wraps_the_text_without_changing_the_geometry() {
        // Escape sequences occupy no columns. If the layout ever measured the painted string, the
        // styled row would be shorter than the plain one by the length of the colour codes.
        struct Allowed;
        impl Allowed {
            fn permission() -> Permission {
                // Resolved rather than constructed, so the test exercises the real policy.
                super::super::style::Permission::resolve(false, super::super::Format::Human, true)
            }
        }
        let permission = Allowed::permission();
        if !permission.allowed() {
            // The environment running the suite forbids colour (`NO_COLOR`, or `TERM=dumb` as the
            // pty harness sets). Nothing to compare; the plain path is covered above.
            return;
        }
        let styled = Report::new()
            .row("Label", "value")
            .render(permission, Some(60));
        let plain_text = Report::new()
            .row("Label", "value")
            .render(plain(), Some(60));
        assert!(styled.contains('\u{1b}'));
        assert_eq!(
            strip(&styled),
            plain_text,
            "styling must not move a single column"
        );
    }

    /// Removes every escape sequence, so a styled line can be compared with a plain one.
    #[cfg(test)]
    fn strip(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars();
        while let Some(character) = chars.next() {
            if character == '\u{1b}' {
                for inner in chars.by_ref() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(character);
            }
        }
        out
    }
}
