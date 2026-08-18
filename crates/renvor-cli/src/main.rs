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

use config::flags::{Cli, Command, DockerAction};
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
fn install_panic_hook(format: Format, command: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
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
    let mut arguments = std::env::args();
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

fn main() {
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
            let _ = error.print();
            std::process::exit(Exit::Success.code());
        }
        Err(error) => {
            let format = requested_format_from_argv();
            match format {
                // In human mode, print clap's own error exactly as clap would have. Reformatting it
                // would lose the caret diagnostics and the suggestions, which are the useful part.
                Format::Human => {
                    let _ = error.print();
                }
                Format::Json => {
                    let reporter = Reporter::new(format, false);
                    let usage = CliError::new(
                        Code::Usage,
                        // clap's rendering is the useful text. It goes in the envelope's `message`,
                        // which C-2 marks explicitly as human-readable and NOT stable.
                        error.render().to_string().trim().to_owned(),
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
    };

    install_panic_hook(format, command_name);

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
    }
}
