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

/// Undoes the terminal state a prompt or a progress bar may have left behind.
///
/// # Deliberately **not** gated on the colour policy
///
/// Hiding the cursor is not styling. The prompt library hides it whatever `NO_COLOR` and
/// `TERM=dumb` say — measured, not assumed — so restoring it has to be unconditional in exactly
/// the same way. Gating this on the same flag that governs colour would leave the cursor hidden on
/// precisely the terminals least able to cope with it.
///
/// Gated on `stderr` being a terminal, because that is the only stream anything here draws on, and
/// writing a control sequence into a redirected file would be putting the very thing this crate
/// escapes everywhere else into somebody's log.
fn restore_terminal() {
    use std::io::Write as _;
    if std::io::stderr().is_terminal() {
        // DECTCEM set: show the cursor. The one sequence written directly rather than through a
        // style, because it is not a style — and because at this point the reporter may be the
        // thing that panicked.
        let mut stderr = std::io::stderr();
        let _ = stderr.write_all(b"\x1b[?25h");
        let _ = stderr.flush();
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
        // ── THE TERMINAL IS PUT BACK BEFORE THIS PROCESS DISAPPEARS ──────────────────
        //
        // `std::process::exit` does not unwind, so **no destructor runs** — not the progress
        // bar's, not the prompt's. Both of those hide the cursor while they draw, and a hidden
        // cursor outlives the process: the operator's shell keeps drawing without one until they
        // work out that `tput cnorm` is the fix.
        //
        // This is why the guarantee is here rather than in a `Drop` impl. A `Drop` that covers
        // every exit except the one that matters is a guarantee that quietly does not hold.
        restore_terminal();
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

/// Redacts an argument, then escapes it, in that order.
///
/// # Redaction moved here on 2026-08-21, and the reason is contract C-8's ordering rule
///
/// It used to be enough to redact the **assembled** rendering: the credential shape
/// `password=hunter2` was plainly visible in the sentence clap produced, and `redact::line`
/// found it there.
///
/// Styling broke that, and broke it silently. Once `--help` and usage errors carry the parser's
/// own escape sequences, the rendering reads
/// `unexpected argument '\x1b[33mpassword=hunter2\x1b[0m'`, and the scanner that walks backwards
/// from the `=` looking for a key walks into `33m` first — `m` and `3` are perfectly good key
/// characters — and concludes the key is `33mpassword`, which is in no secret list. **The
/// credential came out verbatim**, and `tests/redaction.rs` caught it.
///
/// The fix is not a cleverer scanner. It is C-8's own rule, applied here: **redact first, style
/// second.** The argument is redacted while it is still a bare token with no escape sequence in
/// it, and clap then interpolates something that has no credential left to hide. The redaction of
/// the assembled rendering is kept as well, because two independent passes are worth more than a
/// cleverer single one.
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
    std::ffi::OsString::from(output::redact::detail(&output::redact::line(
        &argument.to_string_lossy(),
    )))
}

/// clap's own rendering of `error`, with every argument it echoes already neutralised.
///
/// Re-parsing from escaped arguments preserves clap's caret diagnostics and its suggestions —
/// the useful half — which reformatting the message by hand would destroy.
fn safe_clap_rendering(error: &clap::Error, styled: bool) -> String {
    let escaped: Vec<std::ffi::OsString> = std::env::args_os()
        .map(|argument| argument_for_display(&argument))
        .collect();
    // `redact::line` on the assembled rendering, not on the arguments: a credential is recognised
    // by its `key=value` shape, and that shape only exists once clap has put the argument back
    // into a sentence. Control-character escaping is the opposite case and is done per-argument
    // above, because after assembly the attacker's newlines are indistinguishable from clap's.
    match Cli::try_parse_from(&escaped) {
        // `.ansi()` RATHER THAN `.to_string()`, AND THE DIFFERENCE IS THE WHOLE PALETTE.
        //
        // `StyledStr`'s `Display` writes the plain text: the styles the parser resolved are
        // present in the value and are dropped on the way out. `--help` therefore came out
        // unstyled no matter what `Command::styles` said, on a terminal as much as in a pipe —
        // which looked like the policy working and was the palette never arriving.
        //
        // ── AND `styled` IS FALSE FOR JSON, WHICH IS NOT A DETAIL ───────────────────────
        //
        // The rest of this crate relies on `AutoStream` stripping escape sequences on every
        // forbidden path. **That mechanism cannot see this one.** In JSON mode the rendering
        // becomes `CliError::message`, and `serde_json` escapes each `ESC` into the six ASCII
        // characters `\u001b` *before* the writer ever sees the document — so there is no ESC
        // byte left to strip, and clap's colours travelled intact into
        // `error.message` on a pipe. A consumer that decodes the document and prints the message
        // gets live escape sequences, including into a file.
        //
        // Found by an advisory review; reproduced with `renvor --output json --nonsense | od -c`.
        // The fix is to not create them on that path at all, because stripping is the mechanism
        // that does not work here.
        Err(safe) if styled => output::redact::line(&safe.render().ansi().to_string()),
        Err(safe) => output::redact::line(&safe.render().to_string()),
        // Escaping cannot turn a rejected invocation into an accepted one: no flag, subcommand, or
        // value this CLI recognises contains a control character, so escaping is a no-op on every
        // argument clap matches against. If that ever stopped holding, fall back to escaping the
        // whole rendering rather than printing the unescaped original.
        // The fallback arm deliberately takes the **plain** rendering. It runs only when escaping
        // somehow turned a rejected invocation into an accepted one, which is a state this program
        // does not understand — and redacting text that carries escape sequences is exactly the
        // hazard documented on `argument_for_display`. An unstyled diagnostic on a path that should
        // never be taken is the right trade.
        Ok(_) => output::redact::detail(&output::redact::line(&error.render().to_string())),
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
    // Through `AutoStream`, with **this program's** answer rather than clap's.
    //
    // clap's `color` feature makes it suppress its own styling for a pipe, for `NO_COLOR`, and for
    // `TERM=dumb` — three of the five policies in C-8, and it gets those right. It cannot know
    // about the fourth: `--no-color` is a Renvor flag that clap has never heard of, and `--output
    // json` is decided after clap has already failed. So `renvor --help --no-color` on a terminal
    // emitted colour, which is the flag not working on the one command most likely to be piped
    // into `less`.
    //
    // Constructed with `ColorChoice::Never` on every forbidden path, which makes `AutoStream`
    // **strip** — so the answer is enforced on the bytes rather than requested of the renderer.
    let permission = help_permission(to_stdout);
    if to_stdout {
        let mut stream = anstream::AutoStream::new(std::io::stdout(), permission.choice());
        write!(stream, "{text}")?;
        stream.flush()
    } else {
        let mut stream = anstream::AutoStream::new(std::io::stderr(), permission.choice());
        write!(stream, "{text}")?;
        stream.flush()
    }
}

/// The styling policy for clap's own rendering, resolved before clap has parsed anything.
///
/// # Why the flag is read from `argv` rather than from the parsed `Cli`
///
/// The same chicken-and-egg problem [`requested_format_from_argv`] solves, for the same reason:
/// `--help` and every usage error are produced by clap **before** a `Cli` exists, and a usage error
/// exists precisely because the command line could not be parsed. A scan of the raw arguments is
/// the honest answer, and it is deliberately narrow — it recognises one exact spelling, the only
/// one clap accepts for a long flag with no value, and decides nothing else.
fn help_permission(to_stdout: bool) -> output::style::Permission {
    let opt_out = std::env::args_os().any(|argument| argument == "--no-color");
    let is_terminal = if to_stdout {
        std::io::stdout().is_terminal()
    } else {
        std::io::stderr().is_terminal()
    };
    output::style::Permission::resolve(opt_out, requested_format_from_argv(), is_terminal)
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
            if let Err(failure) = write_rendering(&safe_clap_rendering(&error, true), true) {
                let reporter = Reporter::new(Format::Human, false);
                let error = CliError::new(Code::Internal, "the help text could not be written")
                    .with("cause", failure.to_string());
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
                    let _ = write_rendering(&safe_clap_rendering(&error, true), false);
                }
                Format::Json => {
                    let reporter = Reporter::new(format, false);
                    let usage = CliError::new(
                        Code::Usage,
                        // clap's rendering is the useful text. It goes in the envelope's `message`,
                        // which C-2 marks explicitly as human-readable and NOT stable — but
                        // "not stable" is not "may contain escape sequences", so it is taken
                        // UNSTYLED. See `safe_clap_rendering`.
                        safe_clap_rendering(&error, false).trim().to_owned(),
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

    // THE PROCESS-GLOBAL PRESENTATION STATE, INSTALLED ONCE, HERE.
    //
    // The prompt library keeps its theme in a global lock and `console` keeps its colour decision
    // in a global flag. Both are installed from this one place, before any prompt can run and
    // while the program is still single-threaded — the same discipline the panic hook above
    // follows, for the same reason: a global mutated later races with whoever is already reading
    // it.
    //
    // It is done here rather than inside `Reporter::new` because `Reporter` is constructed in four
    // places on the error paths above, and installing global state as a side effect of building a
    // value is how it ends up installed four times with three different answers.
    output::prompt::install(reporter.stderr_permission());

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
