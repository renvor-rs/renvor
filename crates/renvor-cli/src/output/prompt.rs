//! The prompt adapter: the **only** file in this crate that names the prompt library.
//!
//! # Why the boundary is a whole module rather than a convention
//!
//! Three properties have to hold no matter which library draws the questions, and each of them is
//! easy to lose one call site at a time:
//!
//! - **Cancellation is an exit code, not an I/O error.** Ctrl-C and Escape mean the operator said
//!   no, which is exit `4`. A missing terminal means the *invocation* was wrong, which is exit `2`.
//!   Anything else is a defect, which is exit `1`. [`classify`] is the single place that decides,
//!   so the three cannot drift apart across six prompts.
//! - **Prompts are `stderr`.** Contract C-1 reserves `stdout` for the result, and a prompt on
//!   `stdout` breaks `renvor new --dry-run --output json | jq .` for everyone.
//! - **Nothing dynamic reaches the library's own writers unneutralised.** Its logging and note
//!   helpers write straight to the terminal, bypassing this crate's redaction and
//!   control-character neutralisation. Every string the library renders as **chrome** is
//!   `&'static str`, which makes "no application data down that path" a **compile error** rather
//!   than a review comment — and the one string that cannot be a literal, a prompt's suggested
//!   default, is redacted and neutralised by [`text`] before the library ever sees it.
//!
//! # The library was chosen by measurement
//!
//! `cliclack` returns `io::Result<T>` where the previous library returned a typed error enum, and
//! taking that on trust would have been the wrong way to make this change. A prototype outside
//! this tree drove the real crate through a `portable-pty` terminal and recorded the `ErrorKind`
//! for every case in [`classify`], on Rust 1.94.0. `tests/prompt_contract.rs` now asserts the same
//! table against the shipped binary, so the measurement is a gate rather than a memory.

use std::io::{ErrorKind, IsTerminal as _};

use cliclack::{Confirm, Input};

use super::redact;
use super::style::{Permission, Role};
use crate::exit::{CliError, Code};

/// Maps a prompt failure onto the error taxonomy.
///
/// # The three kinds are deliberate signals, not a guess about an I/O error
///
/// This is the property the previous library was chosen for, and it had to survive the swap. It
/// does, because the library raises **distinct** kinds for distinct causes rather than one generic
/// failure:
///
/// | Cause | `ErrorKind` | Code | Exit |
/// |---|---|---|---|
/// | Ctrl-C, Escape | `Interrupted` | [`Code::Cancelled`] | `4` |
/// | no terminal to prompt on | `NotConnected` | [`Code::Usage`] | `2` |
/// | anything else | anything else | [`Code::Internal`] | `1` |
///
/// The last row is not a catch-all for convenience. Exit `1` is **reserved for defects**, so
/// folding an unknown prompt failure into `cancelled` would hide a bug behind an outcome that
/// looks deliberate — and folding it into `usage` would blame the operator for it.
fn classify(error: &std::io::Error) -> CliError {
    match error.kind() {
        ErrorKind::Interrupted => CliError::new(
            Code::Cancelled,
            "cancelled; nothing was written and the destination is unchanged",
        ),
        ErrorKind::NotConnected => CliError::new(
            Code::Usage,
            "there is no terminal to prompt on; supply the answers as flags instead",
        ),
        _ => CliError::new(
            Code::Internal,
            format!("the prompt failed unexpectedly: {error}"),
        ),
    }
}

/// Renvor's prompt theme.
///
/// # It overrides four things, and the rest is the library's
///
/// The four are the rail, the state marker, the answer, and the placeholder — the colours that
/// carry the sequence's structure. Every other method keeps the library's default, which is the
/// layout: the connected rail's shape, the state symbols themselves, the radio cursor, the
/// position of the validation message.
///
/// **That is less than "the palette", and the difference is worth naming.** `radio_symbol` is
/// not overridden, so the `Yes`/`No` markers on every confirmation still take the library's own
/// green and dim rather than [`Role`] — and those markers are on the hot path, since a
/// confirmation is four of the six questions this wizard asks. The hues coincide today, which is
/// precisely the "close is exactly the problem" drift this comment used to claim it had already
/// prevented. An advisory review read the library and found the overstatement.
///
/// What *is* routed through [`Role`] cannot diverge from the rest of the program, and that is the
/// property being bought. The remainder is a known and stated gap, not an oversight.
#[derive(Debug, Clone, Copy)]
struct RenvorTheme;

impl cliclack::Theme for RenvorTheme {
    /// The vertical rail.
    ///
    /// Accent while the question is live; muted once it is answered, so a finished question
    /// recedes and the live one is the only bright thing on screen; error when the sequence was
    /// cancelled or the answer was refused.
    fn bar_color(&self, state: &cliclack::ThemeState) -> console::Style {
        match state {
            cliclack::ThemeState::Active => Role::Accent.prompt_style(),
            cliclack::ThemeState::Cancel => Role::Error.prompt_style(),
            cliclack::ThemeState::Submit => Role::Muted.prompt_style(),
            cliclack::ThemeState::Error(_) => Role::Warning.prompt_style(),
        }
    }

    /// The marker beside the question.
    fn state_symbol_color(&self, state: &cliclack::ThemeState) -> console::Style {
        match state {
            cliclack::ThemeState::Active => Role::Accent.prompt_style(),
            cliclack::ThemeState::Cancel => Role::Error.prompt_style(),
            cliclack::ThemeState::Submit => Role::Success.prompt_style(),
            cliclack::ThemeState::Error(_) => Role::Warning.prompt_style(),
        }
    }

    /// The answer itself.
    ///
    /// **Muted once submitted, never hidden.** An answered question fades; its answer stays
    /// readable, because the reason to look back at the sequence is to check what was answered.
    fn input_style(&self, state: &cliclack::ThemeState) -> console::Style {
        match state {
            cliclack::ThemeState::Active => Role::Value.prompt_style(),
            cliclack::ThemeState::Cancel => Role::Muted.prompt_style(),
            cliclack::ThemeState::Submit => Role::Muted.prompt_style(),
            cliclack::ThemeState::Error(_) => Role::Value.prompt_style(),
        }
    }

    /// The derived default shown before anything is typed. Always secondary — it is a suggestion,
    /// not an answer.
    fn placeholder_style(&self, _state: &cliclack::ThemeState) -> console::Style {
        Role::Muted.prompt_style()
    }
}

/// Installs the process-global prompt state: the colour policy, then the theme.
///
/// # Both of these are process-global, and that is why this is called exactly once
///
/// The library holds its theme in a global lock and `console` holds its colour decision in a
/// global flag. Installing them from `main`, before any prompt and while the program is still
/// single-threaded, is the same discipline the panic hook follows and for the same reason: a
/// global mutated later races with whoever is already reading it.
///
/// The tests never call this. They drive the **real binary** through a pseudo-terminal, so the
/// installation they exercise is the one `main` performs — and no test mutates global theme state
/// underneath another test running in the same process.
pub fn install(stderr: Permission) {
    // Colour first. The theme's styles are evaluated when a prompt draws, and `console` decides at
    // that moment whether to emit them — so the policy has to be in place before the first draw,
    // not merely before the first prompt is constructed.
    super::style::install_prompt_colour_policy(stderr);
    cliclack::set_theme(RenvorTheme);
}

/// Refuses, before anything is drawn, if there is no terminal to draw a prompt on.
///
/// # Why this is a separate check and not just the first prompt failing
///
/// It already failed — `interact()` builds a `Term::stderr()` and returns `NotConnected` — so the
/// exit code was right. What was wrong was the **order**: [`intro`] succeeds first, so
/// `renvor new 2>log` wrote the sequence's opening chrome into the operator's log file and *then*
/// refused. Chrome in a log is exactly what this module exists to prevent.
///
/// # And why the gate is not simply widened to "stdin and stderr"
///
/// That was the obvious fix and it is a worse one. `Interaction::prompt` false means the wizard
/// does not run — and `renvor new --path ./x` with no wizard **succeeds**, generating a project
/// from defaults. So widening the gate would turn `renvor new --path ./x 2>/dev/null` from a
/// refusal that writes nothing into a silent generation the operator never answered for, which is
/// precisely what FR-010 forbids and what `commands::new::Interaction` warns about at length.
/// Measured before choosing: that command exits 0 and creates six files today.
///
/// So the rule is: **prompting needs `stdin` to decide and `stderr` to draw on.** `stdin` alone
/// keeps deciding whether the wizard is eligible; this says the drawing surface is missing, in the
/// same words and with the same exit code as any other missing terminal.
///
/// # Errors
///
/// [`Code::Usage`], with the generic adapter message: it directs the operator to supply the
/// answers as command-line arguments and does **not** enumerate the caller's flags. This function
/// is reached from every wizard and knows none of them, so a command that wants its own flags
/// named needs its own refusal — `renvor tls trust` has one, and names its consent flag there.
pub fn require_drawable() -> Result<(), CliError> {
    if std::io::stderr().is_terminal() {
        return Ok(());
    }
    Err(classify(&std::io::Error::from(ErrorKind::NotConnected)))
}

/// Opens the grouped prompt sequence with a title.
///
/// # `&'static str`, and that is the security boundary
///
/// This writes through the library's own writer, which does not pass through [`super::Reporter`]
/// and therefore performs no redaction and no control-character neutralisation. A literal cannot
/// carry a credential or an escape sequence from anywhere, and requiring one makes that a
/// property the compiler checks. Anything derived from a path, a flag, a manifest, or the
/// environment must go through the reporter instead.
///
/// # Errors
///
/// [`Code::Internal`] if the terminal could not be written to. Not silently ignored: a title that
/// vanished means the sequence below it is missing its frame, and the reader is looking at
/// something other than what this program believes it drew.
pub fn intro(title: &'static str) -> Result<(), CliError> {
    cliclack::intro(title).map_err(|error| classify(&error))
}

/// Closes the grouped prompt sequence. See [`intro`] for why the argument is `&'static str`.
///
/// # Errors
///
/// [`Code::Internal`] if the terminal could not be written to.
pub fn outro(message: &'static str) -> Result<(), CliError> {
    cliclack::outro(message).map_err(|error| classify(&error))
}

/// Asks for a line of text, offering `default` when the operator just presses Enter.
///
/// `hint` is shown as a placeholder: guidance about the shape of a valid answer, not a second
/// question.
///
/// # This does **not** validate
///
/// Deliberately, and it is the one place where the obvious improvement is the wrong change. The
/// project name is validated by `config::model`, once, for both the wizard and the flag surface —
/// which is what makes "prompt and flag inputs serialize to identical configuration" a property of
/// the type graph rather than a test that happens to pass. Rejecting a name *here* would put a
/// second validator on one of the two paths, and an operator would then get a retry loop from the
/// wizard and exit `3` from the flags for the same input.
///
/// # Errors
///
/// See [`classify`].
pub fn text(
    question: &'static str,
    default: &str,
    hint: Option<&'static str>,
) -> Result<String, CliError> {
    // ── THE SUGGESTED DEFAULT IS THE ONE STRING HERE THAT CANNOT BE A LITERAL ────────────
    //
    // It is **derived from operator input**: the project name comes from `argv`, and the domain
    // comes from the project name. The prompt library draws it through its own writer, which
    // never sees `crate::output::redact`.
    //
    // Left raw, `renvor new $'de\x1b[31mmo'` rendered the escape sequence **into the terminal**,
    // where it recoloured everything after it — and a longer sequence clears the screen or moves
    // the cursor. Measured, on a real pty, against both this build and the one before it: the
    // previous prompt library had the same hole, so this is an old defect that the type signature
    // above claimed did not exist.
    //
    // Escaping it makes the sequence visible as text instead of executable. That also changes what
    // pressing Enter returns — the escaped form rather than the raw one — and that is the correct
    // trade rather than a side effect: **what the operator sees is what they get.** A value that
    // needed escaping was never going to survive validation, so the run ends either way; the only
    // question is whether it ends with a clear refusal or with a reprogrammed terminal.
    let default = redact::detail(&redact::line(default));
    let mut prompt = Input::new(question).default_input(&default);
    if let Some(hint) = hint {
        // ── THE HINT MUST NOT REPLACE THE VALUE ENTER ACCEPTS ───────────────────────
        //
        // `placeholder` overrides what `default_input` would have drawn. Passing the hint alone
        // therefore showed the *guidance* and hid the *value* — so "Project name" displayed
        // "ASCII letters, digits, `-`, and `_`…" while Enter silently returned `demo`. The
        // operator committed to a value that was never on screen at the moment they decided,
        // which is the opposite of the rule the escaping above is justified by: **what the reader
        // sees is what they get.**
        //
        // Found by a review reading the library rather than the diff. Both are shown now, value
        // first, because the value is the thing being accepted and the guidance is about it.
        prompt = prompt.placeholder(&format!("{default} — {hint}"));
    }
    prompt.interact().map_err(|error| classify(&error))
}

/// Asks a yes/no question.
///
/// # Errors
///
/// See [`classify`].
pub fn confirm(question: &'static str, default: bool) -> Result<bool, CliError> {
    Confirm::new(question)
        .initial_value(default)
        .interact()
        .map_err(|error| classify(&error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::Exit;

    #[test]
    fn cancellation_and_interruption_both_exit_four() {
        // C-1 and the behaviour the previous library was chosen for. Ctrl-C and Escape are
        // different events with the same meaning — the operator said no — and the library reports
        // both as `Interrupted`. Distinguishing them in the exit code would make every script
        // handle two cases for one outcome.
        let mapped = classify(&std::io::Error::from(ErrorKind::Interrupted));
        assert_eq!(mapped.code, Code::Cancelled);
        assert_eq!(mapped.exit(), Exit::Cancelled);
        assert!(
            mapped.message.contains("nothing was written"),
            "a cancellation must say the destination is untouched: {}",
            mapped.message
        );
    }

    #[test]
    fn a_prompt_refuses_when_there_is_nowhere_to_draw_it() {
        // FOUND BY THE CODEX REVIEW. `stdin` decides the wizard is eligible; `stderr` is where it
        // gets drawn, and with `stderr` redirected the library cannot draw. The exit code was
        // already right — what was wrong was the ORDER, because `intro` succeeded first and put
        // the sequence's opening chrome into the operator's log before the refusal.
        //
        // # Why this is asserted here and not end to end
        //
        // Reaching it needs `stdin` to be a terminal and `stderr` not to be, simultaneously — and
        // the pty harness attaches both streams to the same pty, so it cannot construct that
        // combination. A plain subprocess cannot either: a non-terminal `stdin` means the wizard
        // never starts, and the command takes the documented non-interactive path instead. That
        // was measured, not assumed — the first version of this test asserted exit 2 and got
        // exit 0 with a generated project.
        //
        // So the reachable half is asserted here, and the end-to-end behaviour was measured by
        // hand through a pty with only `stderr` redirected: exit 2, 78 bytes on the log, zero
        // box-drawing characters, nothing created. That measurement is recorded in the pull
        // request rather than claimed here.
        //
        // Under `cargo test` `stderr` is captured and is not a terminal, which is exactly the
        // condition under test.
        if std::io::stderr().is_terminal() {
            // Running with `--nocapture` on a real terminal. The condition does not hold, so
            // there is nothing to assert; saying so beats asserting something vacuous.
            return;
        }
        let refused = require_drawable().expect_err("a captured stderr is not a terminal");
        assert_eq!(refused.code, Code::Usage);
        assert_eq!(refused.exit(), Exit::Usage);
        assert!(
            refused.message.contains("flags"),
            "the refusal must direct the operator to supply the answers as arguments: {}",
            refused.message
        );
    }

    #[test]
    fn a_missing_terminal_is_a_usage_error_and_not_a_cancellation() {
        // `NotConnected` means the invocation was wrong, not that anybody declined — and the
        // message has to say what to do instead, or the operator is told only that they cannot do
        // what they tried.
        let mapped = classify(&std::io::Error::from(ErrorKind::NotConnected));
        assert_eq!(mapped.code, Code::Usage);
        assert_eq!(mapped.exit(), Exit::Usage);
        assert!(mapped.message.contains("flags"), "{}", mapped.message);
    }

    #[test]
    fn an_unexpected_prompt_failure_is_a_defect_not_a_cancellation() {
        // Exit 1 is reserved for defects. Folding an unknown failure into `cancelled` would hide a
        // bug behind an outcome that looks deliberate.
        for kind in [
            ErrorKind::BrokenPipe,
            ErrorKind::PermissionDenied,
            ErrorKind::Other,
            ErrorKind::UnexpectedEof,
        ] {
            let mapped = classify(&std::io::Error::from(kind));
            assert_eq!(mapped.code, Code::Internal, "{kind:?}");
            assert_eq!(mapped.exit(), Exit::Unclassified, "{kind:?}");
        }
    }

    #[test]
    fn the_theme_never_lets_an_answered_question_become_unreadable() {
        // The one theme property that is a requirement rather than a preference: an answered
        // question is muted, and its ANSWER stays legible. A theme that dimmed both would make the
        // record of what was chosen unreadable exactly when the reader scrolls back to check it.
        use cliclack::Theme as _;
        let theme = RenvorTheme;
        let submitted = cliclack::ThemeState::Submit;
        assert_eq!(
            theme.bar_color(&submitted).apply_to("x").to_string(),
            Role::Muted.prompt_style().apply_to("x").to_string(),
            "an answered question's rail must recede"
        );
        assert_eq!(
            theme
                .state_symbol_color(&submitted)
                .apply_to("x")
                .to_string(),
            Role::Success.prompt_style().apply_to("x").to_string(),
            "an answered question's marker must read as answered"
        );
    }

    #[test]
    fn a_live_question_is_the_accent_and_a_refusal_is_the_error_colour() {
        use cliclack::Theme as _;
        let theme = RenvorTheme;
        assert_eq!(
            theme
                .bar_color(&cliclack::ThemeState::Active)
                .apply_to("x")
                .to_string(),
            Role::Accent.prompt_style().apply_to("x").to_string()
        );
        assert_eq!(
            theme
                .bar_color(&cliclack::ThemeState::Cancel)
                .apply_to("x")
                .to_string(),
            Role::Error.prompt_style().apply_to("x").to_string()
        );
    }
}
