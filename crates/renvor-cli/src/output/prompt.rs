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
//! - **Nothing dynamic reaches the library's own writers.** Its logging and note helpers write
//!   straight to the terminal, bypassing this crate's redaction and control-character
//!   neutralisation. The wrappers here take `&'static str` for every string the library will
//!   render as chrome, which makes "no application data down that path" a **compile error**
//!   rather than a review comment.
//!
//! # The library was chosen by measurement
//!
//! `cliclack` returns `io::Result<T>` where the previous library returned a typed error enum, and
//! taking that on trust would have been the wrong way to make this change. A prototype outside
//! this tree drove the real crate through a `portable-pty` terminal and recorded the `ErrorKind`
//! for every case in [`classify`], on Rust 1.94.0. `tests/prompt_contract.rs` now asserts the same
//! table against the shipped binary, so the measurement is a gate rather than a memory.

use std::io::ErrorKind;

use cliclack::{Confirm, Input};

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
/// # It overrides colour and nothing else
///
/// Every method not listed here keeps the library's default, which is the layout: the connected
/// rail, the state markers, the radio cursor, the position of the validation message. Those are
/// the parts that make the sequence readable and there is no Renvor-specific improvement to make
/// to them.
///
/// What is *not* left to the library is the palette. Its defaults are close to Renvor's — near
/// enough that overriding them looks like a formality — but "close" is exactly the problem: it
/// produces prompts in a slightly different cyan from every other line, and a palette change that
/// silently moves only half the output. Routing through [`Role`] means the two cannot diverge.
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
    let mut prompt = Input::new(question).default_input(default);
    if let Some(hint) = hint {
        prompt = prompt.placeholder(hint);
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
