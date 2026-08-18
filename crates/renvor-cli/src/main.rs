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
mod output;
mod paths;
mod templates;

use std::io::IsTerminal;

use clap::Parser;

use config::flags::{Cli, Command, DockerAction};
use exit::{CliError, Exit};
use output::{Format, Reporter};

fn main() {
    // clap handles `--help`, `--version`, and malformed invocations itself, exiting 2 — which is
    // the code C-1 assigns to a usage error, so no translation is needed.
    let cli = Cli::parse();

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
        Command::Dev => commands::dev::run(reporter, std::path::Path::new(".")),
        Command::Docker { action } => {
            let action = match action {
                DockerAction::Up => commands::docker::Action::Up,
                DockerAction::Down => commands::docker::Action::Down,
                DockerAction::Status => commands::docker::Action::Status,
                DockerAction::Logs => commands::docker::Action::Logs,
            };
            commands::docker::run(reporter, std::path::Path::new("."), action)
        }
    }
}
