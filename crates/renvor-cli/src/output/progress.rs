//! Progress for the one operation that is long enough to need it.
//!
//! # Which operation, and why only that one
//!
//! `generate::verify::in_staging` runs five `cargo` invocations — format, lint, build, test, run —
//! with `.output()`, against a cold target directory. `.output()` **captures**: none of cargo's own
//! output reaches the terminal. So a `renvor new` printed one static line and then produced
//! nothing at all for as long as a cold compile takes, which is the exact shape of a program a
//! reader concludes has hung.
//!
//! Everything else in this crate is either fast or already visible:
//!
//! - Rendering the templates is milliseconds. A spinner that appears and vanishes is worse than
//!   nothing — it draws the eye to a thing that is already over.
//! - `renvor dev` and `renvor docker` hand cargo and docker the terminal
//!   ([`super::Reporter::child_stdout`] returns `inherit()` in human mode), so the child draws its
//!   own progress. A second bar on top of cargo's would be two programs fighting for the same
//!   line.
//!
//! **That is the rule this module exists under, and it is a constraint rather than an
//! observation**: a spinner belongs on work measured in seconds whose output is captured. There is
//! deliberately no artificial delay threshold here, because a threshold would make it *safe* to
//! attach this to fast work, and it is not safe — it is merely invisible, until the day the fast
//! work stops being fast on somebody else's machine.
//!
//! # It cannot render where it must not
//!
//! The draw target is chosen once from [`super::Reporter::progress_visible`], which is already
//! false in JSON mode and whenever `stderr` is not a terminal. When it is false the bar is
//! constructed against [`indicatif::ProgressDrawTarget::hidden`], so the calls still happen and
//! write nowhere. Callers therefore have no `if` to forget.

use std::borrow::Cow;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

use super::redact;
use super::style::Role;

/// How often the spinner advances when nothing has called [`Progress::step`].
///
/// Without a steady tick the animation only moves when the program does, so a five-minute `cargo
/// build` would show a frozen spinner — which says "hung" just as loudly as no spinner at all.
const TICK: std::time::Duration = std::time::Duration::from_millis(120);

/// A live progress indicator, or a silent stand-in for one.
///
/// # Dropping it always clears it
///
/// Every exit from the operation — success, `?` on an error, the operator cancelling — runs
/// [`Drop`], which erases the bar and restores the cursor. That is deliberate rather than
/// incidental: a bar left drawn is a line the next output overwrites halfway, and a cursor left
/// hidden outlives the process and follows the reader into their shell.
///
/// **Panic is the one exit that does not run this.** The panic hook calls `std::process::exit`,
/// which does not unwind and therefore runs no destructors. That case is handled where it has to
/// be — in the hook itself, which restores the cursor before exiting. Relying on `Drop` there
/// would have been a guarantee that quietly did not hold.
#[derive(Debug)]
pub struct Progress {
    bar: ProgressBar,
}

impl Progress {
    /// Starts an indicator for a multi-second operation.
    ///
    /// The reporter decides everything: whether an indicator may render at all, whether it may be
    /// styled, and — when the progress library refuses a terminal it cannot redraw — where the
    /// one-line fallback goes. Callers therefore have no `if` and no policy of their own.
    ///
    /// `title` is `&'static str` for the same reason the prompt wrappers require one: this text
    /// reaches the terminal through the progress library's writer rather than through
    /// [`super::Reporter`], so no redaction stands between it and the screen. A literal has
    /// nothing to redact. Dynamic text belongs in [`Progress::step`], which does redact.
    #[must_use]
    pub fn start(title: &'static str, reporter: &super::Reporter) -> Self {
        let visible = reporter.progress_visible();
        let bar = ProgressBar::new_spinner();
        bar.set_draw_target(if visible {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::hidden()
        });
        // ── THE LIBRARY GETS THE LAST WORD, AND IT SOMETIMES SAYS NO ────────────────────
        //
        // Asking for a visible target is not the same as getting one. The progress library
        // refuses to draw on a terminal it cannot redraw — `TERM=dumb`, and **also `TERM` unset
        // entirely**, which is the state of cron, systemd units, and several embedded terminals.
        //
        // Left there, this change would have been a REGRESSION for exactly those readers: the
        // code it replaced printed one static line unconditionally, and they would have got tens
        // of seconds of total silence through a cold five-check build instead. Measured on a pty
        // against both builds before and after, because "the indicator is absent" and "the
        // operator is told nothing" are not the same sentence.
        //
        // So when the library declines, the line comes back. It goes through the reporter, which
        // owns the stream and the escape-stripping boundary, rather than being written here.
        if visible && bar.is_hidden() {
            reporter.note(title);
        }
        // The template is built from the resolved permission rather than from indicatif's own
        // colour tags, so the one policy in `output::style` governs this too. `{spinner}` and
        // `{msg}` are the only placeholders: no elapsed timer, because a timer on an operation
        // with no known duration invites the reader to compare it against a number that means
        // nothing.
        let template = format!(
            "{} {{spinner}} {{msg}}",
            reporter.stderr_permission().paint(Role::Accent, title)
        );
        // `expect`, NOT `if let Ok`. A template that failed to parse used to be dropped silently
        // and the bar drew with the library's default — a silent fallback, which is the one thing
        // this project's rules forbid outright.
        //
        // It cannot fail: the template is a literal plus `paint`, which emits only SGR sequences
        // and can introduce no `{`. That is exactly why failing loudly costs nothing and why
        // swallowing it bought nothing.
        let style = ProgressStyle::with_template(&template)
            .expect("the progress template is a literal and cannot fail to parse");
        // ASCII frames. The braille spinner most terminals render is invisible in several
        // fixed-width fonts and is announced character by character by some screen readers.
        bar.set_style(style.tick_chars("-\\|/ "));
        bar.enable_steady_tick(TICK);
        Self { bar }
    }

    /// Reports which part of the operation is running now.
    ///
    /// The label is redacted and terminal-neutralised on the way in. Today's only caller passes a
    /// literal from a constant table, so the pass is a no-op — which is the point: the guarantee
    /// has to be in the function, not in the caller's current behaviour. A message that could move
    /// the cursor would be drawn by a library that redraws its own line, and the damage would not
    /// be confined to that line.
    pub fn step(&self, label: &str) {
        // `detail`, not `for_terminal`: the strict escape, newline and tab included. A progress
        // message is one line by definition — the bar redraws it in place — so a newline here does
        // not start a paragraph, it desynchronises the bar from the terminal it is redrawing.
        self.bar
            .set_message(Cow::Owned(redact::detail(&redact::line(label))));
    }

    /// Ends the indicator, leaving the line as it was found.
    ///
    /// Explicit on the success path so the ordering is visible at the call site, rather than
    /// happening at a closing brace next to whatever is printed afterwards.
    pub fn finish(self) {
        // `drop(self)` rather than a second `finish_and_clear`: one place clears, and it is the
        // same one that runs on every other exit.
        drop(self);
    }
}

impl Drop for Progress {
    fn drop(&mut self) {
        // `finish_and_clear` erases the line and restores the cursor. Both matter: an abandoned
        // bar is a half-overwritten line, and a hidden cursor outlives the process.
        self.bar.finish_and_clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reporter that permits no indicator at all.
    ///
    /// JSON format, so `progress_visible()` is false whatever the terminal underneath happens to
    /// be. These tests assert the calls are safe when nothing renders; a visible bar here would
    /// interleave with libtest's own output.
    fn silent_reporter() -> super::super::Reporter {
        super::super::Reporter::new(super::super::Format::Json, true)
    }

    #[test]
    fn an_invisible_indicator_draws_nothing_and_still_accepts_every_call() {
        // The property that lets callers have no `if`. If this ever panicked or blocked, every
        // JSON and non-terminal run of `renvor new` would too.
        let progress = Progress::start("verifying", &silent_reporter());
        progress.step("cargo build");
        progress.step("cargo test");
        assert!(progress.bar.is_hidden());
        progress.finish();
    }

    #[test]
    fn a_hostile_label_cannot_reach_the_terminal_as_control_characters() {
        // The bar redraws its own line, so a label that could move the cursor would not be
        // confined to that line. Asserted on the message the bar actually holds.
        let progress = Progress::start("verifying", &silent_reporter());
        progress.step("cargo\u{1b}[2Jbuild\nforged");
        let message = progress.bar.message();
        assert!(!message.contains('\u{1b}'), "{message:?}");
        assert!(!message.contains('\n'), "{message:?}");
        assert!(message.contains("cargo"), "{message:?}");
    }

    #[test]
    fn dropping_clears_the_line_even_when_nobody_called_finish() {
        // The early-return path: `in_staging` returns `Err` through `?` on a failing check, and
        // nothing calls `finish`. A bar left drawn there would be overwritten halfway by the error
        // message that follows.
        let progress = Progress::start("verifying", &silent_reporter());
        progress.step("cargo clippy");
        drop(progress);
        // Reaching here without a hang or a panic is the assertion; `finish_and_clear` on an
        // already-finished bar is what a double drop would attempt, and this proves the single
        // path.
    }

    #[test]
    fn a_denied_permission_puts_no_escape_sequence_in_the_title() {
        // The template carries the title, and the title is the one piece of the bar this crate
        // styles. Rendered through the bar's own formatter, because that is what reaches the
        // terminal.
        let denied = Progress::start("verifying", &silent_reporter());
        denied.step("cargo build");
        assert!(!denied.bar.message().contains('\u{1b}'));
    }
}
