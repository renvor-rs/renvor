//! The `renvor` executable.
//!
//! # This function does three things and nothing else
//!
//! Parse, dispatch, exit. Every command returns an [`exit::Exit`] or a [`exit::CliError`], and the
//! single conversion between them lives here — so a command cannot accidentally exit with a code
//! that disagrees with the error it reported.
//!
//! # Why `stdin`'s terminal-ness is decided here
//!
//! Contract C-1 enters the wizard **only** when `stdin` is a terminal. That is an ambient fact
//! about the process, and reading it here rather than inside the command keeps every command
//! function testable without a pseudo-terminal.

mod commands;
mod config;
mod exit;
mod generate;
mod inject;
mod output;
mod paths;
mod templates;

use std::io::IsTerminal;

use clap::Parser;
use clap::error::ErrorKind;

use config::flags::{Cli, Command, DockerAction, TlsAction};
use exit::{CliError, Code, Exit};
use output::{Format, Reporter};

/// Makes a panic obey the two output contracts instead of Rust's defaults.
///
/// # Two defaults are wrong here, and both matter
///
/// **Rust exits `101` on panic.** C-1 assigns `1` to "unclassified or internal failure — a panic,
/// or an error no other code describes", and reserves it precisely so that anything exiting `1` is
/// a bug report. A binary that exits `101` makes that reservation a fiction.
///
/// **A panic writes nothing to `stdout`.** C-2 requires *exactly one* JSON document "for success
/// and for failure alike. Not zero on failure" — because a consumer that asked for JSON needs a
/// parseable answer precisely when something went wrong.
///
/// Both were measured rather than assumed: a bare `fn main() { panic!() }` exits 101.
/// Whether the panic hook should emit a JSON envelope.
///
/// # Why a static rather than a captured value
///
/// The hook has to be installed as the **first** statement of `main`, because anything that
/// panics before it runs gets Rust's default behaviour: exit 101, which contract C-1 does not
/// define, and no envelope at all for a `--output json` consumer. But the format and the command
/// name are only known after clap has parsed, which happens later.
///
/// Installing twice is not the answer: [`install_panic_hook`] chains to the hook it replaced, so a
/// second install would make the first its `previous` and a panic would emit **two** envelopes.
/// One install that reads a value updated in place has neither problem.
///
/// **Ownership and concurrency.** `set_hook` is process-global and is called from exactly one
/// place, once, on the main thread before any other work; this crate spawns no threads outside
/// `#[cfg(test)]`. The hook only ever *reads* these, and [`PANIC_COMMAND`] is read with
/// `try_lock` so that a panic occurring while the lock is held degrades to the default name
/// instead of deadlocking inside the panic handler.
static PANIC_JSON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The command name the panic envelope reports. See [`PANIC_JSON`].
static PANIC_COMMAND: std::sync::Mutex<&'static str> = std::sync::Mutex::new("renvor");

/// Tells the already-installed hook what it is reporting on.
fn set_panic_context(format: Format, command: &'static str) {
    PANIC_JSON.store(format == Format::Json, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut slot) = PANIC_COMMAND.lock() {
        *slot = command;
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let format = if PANIC_JSON.load(std::sync::atomic::Ordering::Relaxed) {
            Format::Json
        } else {
            Format::Human
        };
        let command = PANIC_COMMAND.try_lock().map_or("renvor", |slot| *slot);
        // The panic message is NOT put in the envelope. It can carry arbitrary values from wherever
        // the panic happened, and redaction is a filter rather than a guarantee — so the stable,
        // structured half says "internal defect" and the unstructured detail goes to `stderr`,
        // which is not the contract surface.
        if format == Format::Json {
            let error = CliError::new(
                Code::Internal,
                "renvor failed with an internal defect; the details are on stderr. This is a bug \
                 in renvor, not a problem with your command",
            );
            let envelope = output::json::Envelope::failure(command, &error);
            if let Ok(text) = serde_json::to_string_pretty(&envelope) {
                use std::io::Write as _;
                let mut stdout = std::io::stdout();
                let _ = writeln!(stdout, "{text}");
                let _ = stdout.flush();
            }
        }
        previous(info);
        std::process::exit(Exit::Unclassified.code());
    }));
}

/// Reads `--output` straight from `argv`, before clap has parsed anything.
///
/// # Why this scan exists
///
/// clap reports a malformed invocation by printing prose and exiting **before** any of our code
/// runs. C-2 is explicit that this is a contract violation:
///
/// > *"A command that fails by printing an unstructured error and exiting has broken this
/// > contract, because the consumer that asked for JSON receives something it cannot parse
/// > precisely when it most needs to know what went wrong."*
///
/// To honour that we have to know the requested format **while the command line is still
/// unparseable**, which is a chicken-and-egg problem no parser can solve for us. A scan of the raw
/// arguments is the honest answer.
///
/// It is deliberately narrow: it recognises only the two spellings clap accepts, and it makes no
/// other decision. Everything else still goes through clap.
fn requested_format_from_argv() -> Format {
    // `args_os`, not `args`. `std::env::args()` **panics** on an argument that is not valid
    // Unicode, and this function runs before the panic hook is installed — so a single stray byte
    // in argv produced exit 101 (a code C-1 does not define) with zero bytes on stdout, leaving a
    // `--output json` consumer with no document at all.
    //
    // The lossy conversion is safe *here* because this function only ever answers "did the
    // operator ask for JSON": U+FFFD cannot spell `human` or `json`, so an ill-formed argument
    // falls through to the default exactly as an unrecognised one does.
    let mut arguments = std::env::args_os().map(|argument| argument.to_string_lossy().into_owned());
    while let Some(argument) = arguments.next() {
        let value = if argument == "--output" {
            arguments.next()
        } else {
            argument.strip_prefix("--output=").map(str::to_owned)
        };
        if let Some(value) = value {
            return Format::parse(&value).unwrap_or_default();
        }
    }
    Format::default()
}

/// Escapes every control character in one argument, so a diagnostic may echo it back.
///
/// # Why the escaping happens HERE and not on the rendered message
///
/// clap interpolates the offending argument into its own output (`error: unexpected argument
/// '<here>' found`). By the time a rendering exists, the attacker's bytes and clap's layout are
/// one string, and a filter applied to that string has to choose: escape newlines and destroy the
/// usage block, or exempt them and let an argument containing a newline forge a line the operator
/// reads as renvor's own.
///
/// Escaping the untrusted **value** before it is interpolated is the same rule
/// [`output::redact::path`] and [`output::redact::detail`] already follow everywhere else in this
/// crate. It is applied to `argv[0]` too, which is echoed into clap's `Usage:` line on the
/// *success* path of `--help` as well as the failure path.
fn argument_for_display(argument: &std::ffi::OsStr) -> std::ffi::OsString {
    std::ffi::OsString::from(output::redact::detail(&argument.to_string_lossy()))
}

/// clap's own rendering of `error`, with every argument it echoes already neutralised.
///
/// Re-parsing from escaped arguments preserves clap's caret diagnostics and its suggestions —
/// the useful half — which reformatting the message by hand would destroy.
fn safe_clap_rendering(error: &clap::Error) -> String {
    let escaped: Vec<std::ffi::OsString> = std::env::args_os()
        .map(|argument| argument_for_display(&argument))
        .collect();
    // `redact::line` on the assembled rendering, not on the arguments: a credential is recognised
    // by its `key=value` shape, and that shape only exists once clap has put the argument back
    // into a sentence. Control-character escaping is the opposite case and is done per-argument
    // above, because after assembly the attacker's newlines are indistinguishable from clap's.
    match Cli::try_parse_from(&escaped) {
        Err(safe) => output::redact::line(&safe.render().to_string()),
        // Escaping cannot turn a rejected invocation into an accepted one: no flag, subcommand, or
        // value this CLI recognises contains a control character, so escaping is a no-op on every
        // argument clap matches against. If that ever stopped holding, fall back to escaping the
        // whole rendering rather than printing the unescaped original.
        Ok(_) => output::redact::line(&output::redact::detail(&error.render().to_string())),
    }
}

/// Writes a rendered diagnostic and reports a write failure instead of pretending it succeeded.
///
/// # Errors
///
/// The `io::Error` from the write or the flush. `--help` previously discarded this and exited 0,
/// so on a full filesystem it wrote nothing, said nothing, and reported success.
fn write_rendering(text: &str, to_stdout: bool) -> std::io::Result<()> {
    use std::io::Write as _;
    if to_stdout {
        let mut stream = std::io::stdout();
        write!(stream, "{text}")?;
        stream.flush()
    } else {
        let mut stream = std::io::stderr();
        write!(stream, "{text}")?;
        stream.flush()
    }
}

fn main() {
    // FIRST, before anything that could panic. Everything below this line — the `--output`
    // pre-scan, clap's own parsing, format selection — used to run with Rust's default hook in
    // place, so a panic there exited 101 with no envelope. A non-UTF-8 argument reached exactly
    // that window.
    set_panic_context(requested_format_from_argv(), "renvor");
    install_panic_hook();

    // `try_parse`, not `parse`. clap's own error path prints prose and exits, which breaks C-2 for
    // a `--output json` consumer — see `requested_format_from_argv`.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // `--help` and `--version` arrive here as "errors" and are **successes**: they print to
        // stdout and exit 0. Folding them into the failure path below would turn `renvor --help`
        // into a usage error — the kind of fix that breaks the thing it was meant to protect.
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            // `error.print()` wrote clap's rendering straight through, including an `argv[0]` an
            // attacker controls by naming (or symlinking) the binary — on **stdout**, at exit 0,
            // which no test covered. It also discarded the write `Result`: on a full filesystem
            // `--help` produced no output, no diagnostic, and reported success.
            if let Err(failure) = write_rendering(&safe_clap_rendering(&error), true) {
                let reporter = Reporter::new(Format::Human, false);
                let error = CliError::new(Code::Internal, "the help text could not be written")
                    .with("cause", &failure.to_string());
                std::process::exit(reporter.fail("renvor", &error).code());
            }
            std::process::exit(Exit::Success.code());
        }
        Err(error) => {
            let format = requested_format_from_argv();
            match format {
                // In human mode, print clap's own error exactly as clap would have. Reformatting it
                // would lose the caret diagnostics and the suggestions, which are the useful part.
                Format::Human => {
                    // Was `error.print()`, which handed clap's rendering — attacker-controlled
                    // argument text included — to the terminal unfiltered, bypassing the
                    // neutralisation `Reporter` applies to every other human line.
                    let _ = write_rendering(&safe_clap_rendering(&error), false);
                }
                Format::Json => {
                    let reporter = Reporter::new(format, false);
                    let usage = CliError::new(
                        Code::Usage,
                        // clap's rendering is the useful text. It goes in the envelope's `message`,
                        // which C-2 marks explicitly as human-readable and NOT stable.
                        safe_clap_rendering(&error).trim().to_owned(),
                    );
                    reporter.fail("renvor", &usage);
                }
            }
            std::process::exit(Exit::Usage.code());
        }
    };

    // `--output` is parsed by hand rather than by clap's `value_parser`, so that an unsupported
    // value produces `unsupported_value` (exit 3) with `details.supported`, rather than clap's
    // exit 2. A bad *value* is not a malformed *invocation*.
    let format = match Format::parse(&cli.output) {
        Ok(format) => format,
        Err(error) => {
            // Reported in human form: the operator asked for a format we do not have, so we cannot
            // honour their request to emit it.
            let reporter = Reporter::new(Format::Human, cli.no_color);
            std::process::exit(reporter.fail("renvor", &error).code());
        }
    };
    let reporter = Reporter::new(format, cli.no_color);

    let command_name = match &cli.command {
        Command::New(_) => "new",
        Command::Doctor => "doctor",
        Command::Check { .. } => "check",
        Command::Dev => "dev",
        Command::Docker { .. } => "docker",
        Command::Tls { .. } => "tls",
    };

    set_panic_context(format, command_name);

    // A deliberate panic, for the test that proves the hook works. `debug_assertions` is off in a
    // release build, so this path **cannot exist** in a shipped binary — the alternative, a hidden
    // flag that ships, would be test scaffolding in production code.
    #[cfg(debug_assertions)]
    if std::env::var_os("RENVOR_PANIC_FOR_TESTS").is_some() {
        panic!("deliberate panic requested by RENVOR_PANIC_FOR_TESTS");
    }

    let outcome = dispatch(cli, &reporter);
    let exit = match outcome {
        Ok(exit) => exit,
        Err(error) => reporter.fail(command_name, &error),
    };
    std::process::exit(exit.code());
}

/// Routes to a command. Split out so `main` has no branching left to get wrong.
///
/// Takes `cli` **by value** so `NewArgs` can be consumed by `into_answers`. Borrowing here would
/// force either a `Clone` on `NewArgs` that nothing else needs, or a second `Cli::parse()` — and
/// parsing the same argv twice is a bug waiting for the day the two parses disagree.
fn dispatch(cli: Cli, reporter: &Reporter) -> Result<Exit, CliError> {
    let yes = cli.yes;
    let dry_run = cli.dry_run;
    match cli.command {
        Command::New(args) => {
            let answers = (*args).into_answers()?;
            // C-1, exactly: the wizard is entered ONLY when stdin is a terminal, and `--yes`
            // waives **confirmation** rather than requesting non-interactivity. Folding these into
            // one flag would make `--yes` substitute defaults for answers nobody gave.
            let terminal = std::io::stdin().is_terminal();
            let interaction = commands::new::Interaction {
                prompt: terminal,
                confirm: terminal && !yes,
            };
            commands::new::run(reporter, answers, interaction, dry_run)
        }
        Command::Doctor => commands::doctor::run(reporter),
        Command::Check { path } => commands::check::run(reporter, &path),
        Command::Dev => commands::dev::run(reporter, std::path::Path::new("."), dry_run),
        Command::Docker { action } => {
            let action = match action {
                DockerAction::Up => commands::docker::Action::Up,
                DockerAction::Down => commands::docker::Action::Down,
                DockerAction::Status => commands::docker::Action::Status,
                DockerAction::Logs => commands::docker::Action::Logs,
            };
            commands::docker::run(reporter, std::path::Path::new("."), action, dry_run)
        }
        Command::Tls { action } => match action {
            // `terminal` is `stdin.is_terminal()` and NOT `!yes`. Contract C-1 scopes `--yes` to
            // the review screen; a general-purpose "assume yes" that also installs a certificate
            // authority is precisely the accident `commands::tls` exists to prevent, so `yes` is
            // deliberately not consulted here.
            TlsAction::Trust { consent } => {
                commands::tls::trust(reporter, consent, std::io::stdin().is_terminal(), dry_run)
            }
        },
    }
}
