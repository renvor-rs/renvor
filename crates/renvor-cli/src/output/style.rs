//! Semantic presentation roles, and the single place that decides whether colour is allowed.
//!
//! # Commands name meaning; this module chooses appearance
//!
//! Nothing outside this file may contain a colour name, an escape sequence, or an
//! [`anstyle::Style`] literal. A command asks for [`Role::Success`], not for green — so the day
//! green becomes wrong for a colour-blind reader, or an accessibility review changes the accent,
//! there is exactly one file to change and no grep to get wrong.
//!
//! # Colour is never the only signal
//!
//! Every role that carries meaning also carries a word: `INFO`, `WARN`, `ERROR`, `DONE`. A reader
//! with no colour at all — piping to a file, running under `TERM=dumb`, or simply not perceiving
//! the hue — loses decoration and loses nothing else. That is a requirement of contract C-8 and
//! not a courtesy; it is also why the labels are spelled out rather than drawn as symbols.
//!
//! # No emoji, anywhere
//!
//! Emoji render at one, two, or zero columns depending on the terminal, the font, and the
//! platform, which makes a column-aligned layout unalignable. They are also read aloud by screen
//! readers as their CLDR names. The vocabulary here is ASCII words and — inside the prompt
//! adapter, where the drawing library owns it — box-drawing and geometric characters.

use anstyle::{AnsiColor, Color, Effects, Style};

use super::Format;

/// What a piece of output *means*.
///
/// The appearance is [`Role::style`]'s business and is deliberately not visible to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The active question, the selected choice, a command name the reader should type.
    Accent,
    /// `DONE`, `OK`, and a completed operation.
    Success,
    /// `INFO`. Something happened that the reader did not ask about and should know.
    Information,
    /// `WARN`. Something is not as expected and the command continued.
    Warning,
    /// `ERROR`. Something failed.
    Error,
    /// Secondary text: hints, timestamps, inactive choices, and the leader dots themselves.
    Muted,
    /// A value the reader came for. Deliberately the terminal's own foreground.
    Value,
    /// A section heading.
    Heading,
}

impl Role {
    /// The appearance of this role.
    ///
    /// # Why [`Role::Value`] and [`Role::Heading`] are *not* given a colour
    ///
    /// They render in the terminal's default foreground, whatever the reader configured it to be.
    /// Assigning them "white" would be wrong on a light background and would override a theme the
    /// reader chose on purpose. The heading is separated by weight rather than by hue.
    ///
    /// Contrast is the reason [`Role::Muted`] uses `BrightBlack` **with** `DIMMED` rather than
    /// dimmed default-foreground: a dimmed default is illegible on some light themes, while
    /// bright-black is a palette entry the reader's own theme defines.
    #[must_use]
    pub const fn style(self) -> Style {
        match self {
            Self::Accent => fg(AnsiColor::Cyan),
            Self::Success => fg(AnsiColor::Green),
            Self::Information => fg(AnsiColor::Blue),
            Self::Warning => fg(AnsiColor::Yellow),
            Self::Error => fg(AnsiColor::Red),
            Self::Muted => fg(AnsiColor::BrightBlack).effects(Effects::DIMMED),
            Self::Value => Style::new(),
            Self::Heading => Style::new().effects(Effects::BOLD),
        }
    }

    /// The same role, in the vocabulary the prompt library speaks.
    ///
    /// # Two style types, one palette
    ///
    /// `anstyle::Style` is what this crate writes with; `console::Style` is what `cliclack` draws
    /// with, and there is no conversion between them. The tempting answer is to let the prompt
    /// theme pick its own colours — which is how a project ends up with prompts that are a
    /// slightly different cyan from everything else, and with a palette change that moves half the
    /// output.
    ///
    /// So the mapping lives **here**, beside [`Role::style`], where the two can be read together
    /// and neither can be changed without seeing the other.
    ///
    /// # The one role where the two spellings genuinely differ
    ///
    /// [`Role::Muted`] renders as SGR `90` here and as `38;5;8` through `console`, because
    /// `console` emits **every** bright foreground as `\x1b[38;5;{n+8}m` and offers no way to ask
    /// for SGR 90 (`console-0.16.4/src/utils.rs`). The two are the same palette entry on any
    /// terminal that implements both, and on a terminal that maps its theme onto the aixterm
    /// range but not onto the 256-colour cube they are not.
    ///
    /// This comment previously claimed the opposite — that using `BrightBlack` here **avoided**
    /// `color256(8)`. It does not, and could not: the choice is not available on this side.
    /// An advisory review read the library and caught it. The mapping is still worth having in
    /// one place, because the other five roles do agree exactly and a palette change must move
    /// both.
    #[must_use]
    pub fn prompt_style(self) -> console::Style {
        let style = console::Style::new();
        match self {
            Self::Accent => style.cyan(),
            Self::Success => style.green(),
            Self::Information => style.blue(),
            Self::Warning => style.yellow(),
            Self::Error => style.red(),
            Self::Muted => style.black().bright().dim(),
            Self::Value => style,
            Self::Heading => style.bold(),
        }
    }
}

/// A foreground colour, spelled once.
const fn fg(colour: AnsiColor) -> Style {
    Style::new().fg_color(Some(Color::Ansi(colour)))
}

/// Whether styling may be emitted on one stream, and the reasons it may not.
///
/// # Every clause is a veto, and that is the whole design
///
/// There is no clause that *enables* styling. Styling is what is left when nothing has forbidden
/// it, which is why a new reason to suppress it can only ever be added — never accidentally
/// bypassed by a caller that constructs the value a different way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permission {
    allowed: bool,
}

/// The two environment variables that can forbid styling, read once.
///
/// # Why this is a value rather than two calls to `std::env`
///
/// `unsafe_code = "forbid"` applies to the test build as well as the shipped one, and mutating a
/// process environment variable is `unsafe` in edition 2024 — so a test that set `NO_COLOR` and
/// checked the answer **cannot be written here**. That is the right constraint rather than an
/// obstacle: it forces the decision to be a pure function of stated inputs, which is testable at
/// every combination instead of at the few a test process can be talked into.
///
/// The environment is read in exactly one place, [`Environment::current`], and the rules for
/// interpreting each variable are separately testable as functions over the raw value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Environment {
    /// `NO_COLOR` is present and non-empty.
    no_color: bool,
    /// `TERM` is `dumb`.
    dumb: bool,
}

impl Environment {
    /// Reads the real process environment.
    #[must_use]
    pub fn current() -> Self {
        Self {
            no_color: no_color_requested(std::env::var_os("NO_COLOR").as_deref()),
            dumb: dumb_terminal(std::env::var_os("TERM").as_deref()),
        }
    }

    /// An environment that forbids nothing.
    ///
    /// `#[cfg(test)]` for the reason `Code::ALL` is: a shipped constructor that no shipped code
    /// calls reads like an API and is dead weight. Nothing at runtime states its own environment —
    /// [`Environment::current`] is the only source there.
    #[cfg(test)]
    #[must_use]
    pub const fn permissive() -> Self {
        Self {
            no_color: false,
            dumb: false,
        }
    }
}

impl Permission {
    /// Resolves the policy for one stream, reading the process environment.
    ///
    /// `opt_out` is `--no-color`. `is_terminal` is that specific stream's terminal-ness — **not
    /// the process's**, because `renvor doctor > report.txt` leaves `stderr` a terminal while
    /// `stdout` is a file, and a single answer for both would put escape sequences in the file.
    #[must_use]
    pub fn resolve(opt_out: bool, format: Format, is_terminal: bool) -> Self {
        Self::decide(opt_out, format, is_terminal, Environment::current())
    }

    /// The decision itself: a pure function of four stated inputs.
    ///
    /// **`CLICOLOR_FORCE` is not among them, and that is the precedence rule rather than an
    /// omission.** C-8 requires an explicit refusal to beat a force-colour environment variable.
    /// The strongest way to guarantee that is not to consult the variable at all — a rule with no
    /// override cannot be overridden. `console` would honour it, which is exactly why
    /// [`install_prompt_colour_policy`] tells `console` the answer instead of letting it decide.
    #[must_use]
    pub const fn decide(
        opt_out: bool,
        format: Format,
        is_terminal: bool,
        environment: Environment,
    ) -> Self {
        Self {
            allowed: !opt_out
                && !environment.no_color
                && !environment.dumb
                && matches!(format, Format::Human)
                && is_terminal,
        }
    }

    /// A permission that forbids everything, for callers that already know the answer is no.
    ///
    /// `#[cfg(test)]`: at runtime every permission is *resolved* from the stream it governs, and a
    /// shortcut that skips the resolution is a way for a caller to get a different answer from the
    /// policy.
    #[cfg(test)]
    #[must_use]
    pub const fn denied() -> Self {
        Self { allowed: false }
    }

    /// Whether styling is permitted.
    #[must_use]
    pub const fn allowed(self) -> bool {
        self.allowed
    }

    /// Applies a role, or returns the text untouched.
    ///
    /// The text is expected to be **already redacted and already terminal-neutralised**. This
    /// function adds escape sequences; it does not remove them, and it cannot tell a sequence it
    /// wrote from one that arrived in the argument. Contract C-8 states the ordering as a rule
    /// because it is the only thing keeping a styled label from laundering an untrusted value.
    #[must_use]
    pub fn paint(self, role: Role, text: &str) -> String {
        if !self.allowed || text.is_empty() {
            return text.to_owned();
        }
        let style = role.style();
        format!("{style}{text}{style:#}")
    }

    /// The [`anstream::ColorChoice`] this permission corresponds to.
    ///
    /// `Never` makes `AutoStream` **strip** ANSI from everything written through it, which is the
    /// second half of the guarantee: the policy declines to emit sequences, and the writer removes
    /// any that got there another way.
    #[must_use]
    pub const fn choice(self) -> anstream::ColorChoice {
        if self.allowed {
            anstream::ColorChoice::Always
        } else {
            anstream::ColorChoice::Never
        }
    }
}

/// `NO_COLOR`, per <https://no-color.org>: **present and non-empty**, whatever its value.
///
/// The empty-string exemption is in the specification and is not a detail to round off. `NO_COLOR=`
/// is how a shell script *unsets* the request for one command, and treating it as set would make
/// that impossible to express.
fn no_color_requested(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// `TERM=dumb`, which promises no escape-sequence support at all.
fn dumb_terminal(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|term| term == "dumb")
}

/// Tells `console` — and therefore the prompt library drawing through it — what this process
/// decided, overriding its own environment sniffing.
///
/// # Why this call has to exist
///
/// `console` resolves colour from `CLICOLOR_FORCE`, `CLICOLOR`, and `NO_COLOR` on its own. Left to
/// do that, `CLICOLOR_FORCE=1 renvor new --no-color` produced unstyled Renvor output beside styled
/// prompts: one process disagreeing with itself about a flag the operator set explicitly. C-8
/// requires that an explicit refusal beat a force-colour environment variable, and this is where
/// that happens.
///
/// # BOTH FLAGS, AND THE FIRST ATTEMPT SET ONLY ONE
///
/// `console` keeps two answers, one per stream, and a `console::Style` consults the **`stdout`**
/// one unless it was built with `Style::for_stderr()`. The prompt library builds its theme with
/// plain `console::Style::new()` and then draws the result on `stderr` — so the styles are
/// stderr-bound but ask the stdout flag.
///
/// Setting only the `stderr` flag therefore left the prompts consulting `console`'s own
/// environment sniffing, and the two answers diverged exactly where this contract promises they
/// cannot: with `NO_COLOR=` **empty**, Renvor styled its own lines (the empty-string rule, per
/// <https://no-color.org>) while the prompts came out plain, because `console` does not implement
/// that rule. One process, one flag, two answers — which is the failure this function exists to
/// prevent. Measured on a pty, not reasoned about.
///
/// Both flags now carry the `stderr` answer, because every `console::Style` in this program is
/// drawn to `stderr` whatever flavour it was constructed with. Nothing here uses `console` for
/// `stdout` at all.
///
/// # Called exactly once, before any prompt
///
/// This is process-global state. It is installed from `main` while the program is still
/// single-threaded, which is the same discipline the panic hook follows and for the same reason.
pub fn install_prompt_colour_policy(stderr: Permission) {
    console::set_colors_enabled_stderr(stderr.allowed());
    console::set_colors_enabled(stderr.allowed());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    /// Every combination of the four inputs, exhaustively.
    ///
    /// Four booleans is sixteen cases, which is small enough to enumerate — so the policy is
    /// asserted **completely** rather than sampled. A veto added later without a matching clause
    /// here fails, because the expectation below is written as the rule and not as a table of
    /// remembered answers.
    #[test]
    fn styling_is_permitted_exactly_when_nothing_forbids_it() {
        for opt_out in [false, true] {
            for no_color in [false, true] {
                for dumb in [false, true] {
                    for is_terminal in [false, true] {
                        for format in [Format::Human, Format::Json] {
                            let environment = Environment { no_color, dumb };
                            let permission =
                                Permission::decide(opt_out, format, is_terminal, environment);
                            let expected = !opt_out
                                && !no_color
                                && !dumb
                                && format == Format::Human
                                && is_terminal;
                            assert_eq!(
                                permission.allowed(),
                                expected,
                                "opt_out={opt_out} no_color={no_color} dumb={dumb} \
                                 terminal={is_terminal} format={format:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_single_clause_is_enough_to_refuse_on_its_own() {
        // The same property stated the way a reader checks it: start from the one permitting case
        // and turn each veto on alone.
        let permissive = Environment::permissive();
        assert!(Permission::decide(false, Format::Human, true, permissive).allowed());

        assert!(!Permission::decide(true, Format::Human, true, permissive).allowed());
        assert!(!Permission::decide(false, Format::Json, true, permissive).allowed());
        assert!(!Permission::decide(false, Format::Human, false, permissive).allowed());
        assert!(
            !Permission::decide(
                false,
                Format::Human,
                true,
                Environment {
                    no_color: true,
                    dumb: false
                }
            )
            .allowed()
        );
        assert!(
            !Permission::decide(
                false,
                Format::Human,
                true,
                Environment {
                    no_color: false,
                    dumb: true
                }
            )
            .allowed()
        );
    }

    #[test]
    fn an_empty_no_color_is_not_a_request_for_no_colour() {
        // <https://no-color.org>: "when present and not an empty string". `NO_COLOR=` is how a
        // script cancels an inherited request, and treating it as set makes that inexpressible.
        assert!(!no_color_requested(None));
        assert!(!no_color_requested(Some(OsStr::new(""))));
        assert!(no_color_requested(Some(OsStr::new("1"))));
        // "whatever its value" — including a value that looks like a refusal.
        assert!(no_color_requested(Some(OsStr::new("0"))));
        assert!(no_color_requested(Some(OsStr::new("false"))));
    }

    #[test]
    fn only_dumb_is_dumb() {
        assert!(dumb_terminal(Some(OsStr::new("dumb"))));
        assert!(!dumb_terminal(Some(OsStr::new("xterm-256color"))));
        assert!(!dumb_terminal(Some(OsStr::new("dumb-something"))));
        assert!(!dumb_terminal(None));
    }

    #[test]
    fn an_explicit_refusal_beats_a_force_colour_environment() {
        // C-8's precedence rule, asserted as a property of the SIGNATURE rather than of a value:
        // `decide` takes four inputs and `CLICOLOR_FORCE` is not one of them, so there is nothing
        // for a force-colour variable to override. `Environment::current` reads two variables and
        // that is not one of them either.
        //
        // The end-to-end half — that the prompt library, which WOULD honour it, is told this
        // answer instead — is asserted against the real binary in `tests/presentation.rs` on a
        // pipe and in `tests/terminal.rs` on a terminal that could have had colour.
        let forced = Environment::permissive();
        assert!(!Permission::decide(true, Format::Human, true, forced).allowed());
    }

    #[test]
    fn a_denied_permission_emits_the_text_and_nothing_else() {
        let permission = Permission::denied();
        assert_eq!(permission.paint(Role::Error, "ERROR"), "ERROR");
        assert!(!permission.paint(Role::Error, "ERROR").contains('\u{1b}'));
        assert_eq!(permission.choice(), anstream::ColorChoice::Never);
    }

    #[test]
    fn a_granted_permission_wraps_the_text_and_closes_what_it_opened() {
        let permission = Permission::decide(false, Format::Human, true, Environment::permissive());
        let painted = permission.paint(Role::Success, "DONE");
        assert!(painted.contains("DONE"));
        assert!(painted.starts_with('\u{1b}'), "{painted:?}");
        assert!(
            painted.ends_with("\u{1b}[0m"),
            "an unterminated style bleeds into every following line: {painted:?}"
        );
        assert_eq!(permission.choice(), anstream::ColorChoice::Always);
    }

    #[test]
    fn painting_nothing_produces_nothing() {
        // An empty styled string is a pair of escape sequences around no glyphs — invisible, and
        // still counted by any test asserting the absence of ANSI. Cheaper to never emit it.
        let permission = Permission::decide(false, Format::Human, true, Environment::permissive());
        assert_eq!(permission.paint(Role::Accent, ""), "");
    }

    #[test]
    fn every_role_that_carries_meaning_is_distinguishable_without_colour() {
        // The check is on the vocabulary rather than on the palette: two roles MAY share a colour
        // (a palette has few entries), but no role may depend on colour to be understood. The
        // words live at the call sites, so what this asserts is the property that makes that
        // possible — `Value` and `Heading` take no colour at all, so a reader who sees only
        // uncoloured text still sees the same words in the same places.
        assert_eq!(Role::Value.style(), Style::new());
        assert_eq!(
            Role::Heading.style().get_fg_color(),
            None,
            "a heading must not override the reader's foreground"
        );
    }

    #[test]
    fn muted_is_the_one_role_whose_two_spellings_differ_and_it_differs_only_in_spelling() {
        // A GAP THE PREVIOUS TEST LEFT, AND THE REASON IT MATTERED.
        //
        // `the_two_style_vocabularies_agree_on_every_role` checked five roles and skipped three.
        // One of the three skipped — `Muted` — is the only one that actually diverges, so the
        // test exercised every case except the one it existed to catch.
        //
        // The divergence is forced: `console` emits a bright foreground as `38;5;{n+8}` and has
        // no spelling for SGR 90. What must hold is that both are **bright black and dimmed** —
        // the same palette entry, the same attenuation — and that is what is asserted.
        let own = format!("{}x{:#}", Role::Muted.style(), Role::Muted.style());
        assert!(
            own.contains("90"),
            "the anstyle side is not bright black: {own:?}"
        );
        assert!(own.contains('2'), "the anstyle side is not dimmed: {own:?}");

        let prompt = format!(
            "{}",
            Role::Muted.prompt_style().force_styling(true).apply_to("x")
        );
        assert!(
            prompt.contains("38;5;8"),
            "the console side is not bright black: {prompt:?}"
        );
        assert!(
            prompt.contains("[2m"),
            "the console side is not dimmed: {prompt:?}"
        );
    }

    #[test]
    fn value_and_heading_take_no_colour_in_either_vocabulary() {
        // The other two roles the agreement test skipped. Both must leave the reader's own
        // foreground alone, in both spellings — a heading that overrode it would be exactly the
        // thing `Role::style`'s doc comment says it must not do.
        assert_eq!(Role::Value.style(), Style::new());
        assert!(
            !format!(
                "{}",
                Role::Value.prompt_style().force_styling(true).apply_to("x")
            )
            .contains("38;5;"),
            "the value role must not set a colour"
        );
        assert_eq!(Role::Heading.style().get_fg_color(), None);
    }

    #[test]
    fn the_two_style_vocabularies_agree_on_every_coloured_role() {
        // The palette lives in one file precisely so these cannot drift. This asserts they have
        // not: each role's prompt colour is the one its `anstyle` colour names.
        //
        // `Muted`, `Value`, and `Heading` are covered by the two tests above — `Muted` because it
        // is the one genuine divergence, the other two because they take no colour at all. Every
        // role is therefore checked somewhere, which was not true before.
        for (role, expected) in [
            (Role::Accent, "36"),
            (Role::Success, "32"),
            (Role::Information, "34"),
            (Role::Warning, "33"),
            (Role::Error, "31"),
        ] {
            let prompt = format!("{}", role.prompt_style().force_styling(true).apply_to("x"));
            assert!(
                prompt.contains(expected),
                "{role:?} renders {prompt:?}, which does not use SGR {expected}"
            );
            let own = format!("{}x{:#}", role.style(), role.style());
            assert!(
                own.contains(expected),
                "{role:?} renders {own:?}, which does not use SGR {expected}"
            );
        }
    }
}
