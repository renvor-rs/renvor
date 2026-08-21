//! Output: stream discipline, format selection, and redaction.
//!
//! # The rule this module exists to make unbreakable
//!
//! Contract C-1: **`stdout` carries the command's result and nothing else.** Prompts, progress,
//! warnings, diagnostics, and error text all go to `stderr`. The reason is concrete rather than
//! stylistic — `renvor new --dry-run --output json | jq .` has to work with no filtering, and one
//! stray `println!` breaks it.
//!
//! So `print!` and `println!` are not used anywhere in this crate. Everything goes through
//! [`Reporter`], which owns both streams and gives each a method whose name says where it lands.
//! `tests/architecture.rs` asserts that mechanically, with a positive control, rather than
//! trusting this paragraph.
//!
//! # Contract C-8 added a second rule, and it is the same shape
//!
//! **No module outside [`style`] names a colour, and none writes an escape sequence.** Human
//! output is built as a [`layout::Report`] — structure and raw values — and the only thing that
//! turns one into text redacts every dynamic field, measures the result, and *then* styles it.
//! That ordering is not a convention a caller can get wrong, because there is no other way to
//! render a report.
//!
//! Styling is resolved **per stream**. `renvor doctor > report.txt` leaves `stderr` a terminal
//! while `stdout` is a file; one answer for both would put escape sequences in the file. Both
//! answers are computed once, here, and handed down.

pub mod json;
pub mod layout;
pub mod progress;
pub mod prompt;
pub mod redact;
pub mod style;

use std::io::{IsTerminal, Write};

use layout::{Report, Status};
use style::Permission;

use crate::exit::{CliError, Code, Exit};

/// The `--output` flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    /// Prose for a person. The default.
    #[default]
    Human,
    /// Exactly one JSON document on `stdout`.
    Json,
}

impl Format {
    /// Parses the flag value.
    ///
    /// # Errors
    ///
    /// [`crate::exit::Code::UnsupportedValue`] naming what was supported, because a message that
    /// says only "invalid" makes the operator guess.
    pub fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            other => Err(CliError::new(
                crate::exit::Code::UnsupportedValue,
                format!("`--output {other}` is not supported; use `human` or `json`"),
            )
            .with("flag", "--output")
            .with("value", other.to_owned())
            .with("supported", "human, json")),
        }
    }
}

/// Owns both streams so that nothing else has to remember which is which.
pub struct Reporter {
    format: Format,
    /// Whether the command's **result** may be styled. Resolved from `stdout`.
    stdout: Permission,
    /// Whether prompts, progress, and diagnostics may be styled. Resolved from `stderr`.
    stderr: Permission,
    /// Whether `stderr` is a terminal. Progress renders to nothing when it is not (C-1).
    stderr_is_terminal: bool,
}

impl std::fmt::Debug for Reporter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Reporter")
            .field("format", &self.format)
            .field("stdout", &self.stdout)
            .field("stderr", &self.stderr)
            .finish()
    }
}

impl Reporter {
    /// Builds a reporter, resolving colour from the flag **and** from whether the stream is a
    /// terminal.
    ///
    /// `std::io::IsTerminal` is in the standard library, so no `atty` or `is-terminal` dependency
    /// is taken for this — verified by compiling and running rather than assumed
    /// ([Phase 003 research §D12](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/research.md)).
    #[must_use]
    pub fn new(format: Format, no_color: bool) -> Self {
        let stderr_is_terminal = std::io::stderr().is_terminal();
        Self {
            format,
            // TWO ANSWERS, NOT ONE. Until C-8 this was a single `colour` computed from `stderr`,
            // which was harmless while nothing was styled and would have been a defect the moment
            // something was: the command's result goes to `stdout`, so styling it on the strength
            // of `stderr` being a terminal writes escape sequences into `renvor doctor > file`.
            stdout: Permission::resolve(no_color, format, std::io::stdout().is_terminal()),
            stderr: Permission::resolve(no_color, format, stderr_is_terminal),
            stderr_is_terminal,
        }
    }

    /// The styling policy for `stderr`, for the parts of the program that draw there themselves.
    ///
    /// The prompt library and the progress library own their own writers. Handing them this rather
    /// than letting them consult the environment is what makes `--no-color` mean the same thing in
    /// every part of one process — see [`style::install_prompt_colour_policy`].
    #[must_use]
    pub const fn stderr_permission(&self) -> Permission {
        self.stderr
    }

    /// Whether styling would be active on `stdout`. Used by tests and by nothing else.
    #[cfg(test)]
    #[must_use]
    pub const fn colour(&self) -> bool {
        self.stdout.allowed()
    }

    /// Whether progress should render at all.
    #[must_use]
    pub fn progress_visible(&self) -> bool {
        // Never in JSON mode, and never when `stderr` is not a terminal. A progress bar written
        // into a CI log is thousands of lines of carriage returns.
        self.stderr_is_terminal && self.format == Format::Human
    }

    /// Where a child process's `stdout` must be sent.
    ///
    /// # Why a command may not simply let the child inherit
    ///
    /// `dev` and `docker` run another program in the foreground. A child that inherits this
    /// process's `stdout` writes **straight past** the [`Reporter`], because inheritance is a
    /// file-descriptor fact and not something this crate mediates.
    ///
    /// In [`Format::Human`] that is exactly right: `cargo test`'s progress and `docker compose`'s
    /// table *are* the output the operator asked for, and buffering them through here would
    /// destroy their interactivity for no gain.
    ///
    /// In [`Format::Json`] it breaks contract C-2. `stdout` carries **exactly one JSON document**,
    /// and a child that prepends a libtest summary to it produces a stream no parser accepts — on
    /// the *success* run as much as the failure run, so an operator piping into `jq` can
    /// distinguish nothing but the exit code. Measured before this existed: `renvor --output json
    /// dev | python3 -m json.tool` failed on every invocation.
    ///
    /// The child's output is **redirected, not discarded**: it moves to `stderr`, which C-1
    /// already reserves for diagnostics, and which `docker.rs`'s own failure message
    /// (*"its output is above"*) always assumed it could point at. Discarding it would trade a
    /// contract violation for an information loss.
    ///
    /// # Why a duplicated handle rather than a pipe
    ///
    /// A pipe would need a reader thread. `docker logs` streams without bound, so a
    /// read-after-wait would deadlock the moment the child filled the pipe buffer, and a reader
    /// thread would add concurrency to a path whose whole job is to be transparent. Duplicating
    /// the `stderr` handle hands the child the same destination this process writes to, and the
    /// kernel does the interleaving.
    ///
    /// # Errors
    ///
    /// [`Code::Internal`] if the `stderr` handle cannot be duplicated. This **fails closed**: it
    /// does not silently fall back to `inherit()` (which would reintroduce the contract
    /// violation) or to `null()` (which would silently discard the operator's output).
    pub fn child_stdout(&self) -> Result<std::process::Stdio, CliError> {
        match self.format {
            Format::Human => Ok(std::process::Stdio::inherit()),
            Format::Json => duplicate_stderr().map_err(|error| {
                CliError::new(
                    Code::Internal,
                    "the child process could not be given a `stderr` to write to",
                )
                .with("cause", error.to_string())
            }),
        }
    }

    /// Writes a diagnostic line to `stderr`, redacted.
    ///
    /// Unstyled prose. Anything with structure — a status label, a row, a list — is an
    /// [`Report`] and goes through [`Reporter::emit`].
    pub fn note(&self, message: &str) {
        // `for_terminal` AFTER `line`: redaction first, so a secret is replaced before anything
        // else looks at it, then control-sequence neutralisation on what survives. Both are needed
        // and neither subsumes the other — one hides values, the other stops the text reprogramming
        // the terminal it is printed to.
        //
        // Written through the same styled writer as everything else. This emits no escape
        // sequences of its own, so the only thing that changes is that a sequence which somehow
        // survived redaction is stripped again at the boundary on every path that forbids styling.
        let _ = self.write(&redact::for_terminal(&redact::line(message)), false);
    }

    /// Writes a structured report to `stderr`.
    ///
    /// For everything the command says *about* its work — the plan it is about to carry out, the
    /// review screen, a warning. The result itself goes to `stdout` through
    /// [`Reporter::finish`], because C-1 reserves that stream for it.
    pub fn emit(&self, report: &Report) {
        if report.is_empty() {
            return;
        }
        let _ = self.write(
            &report.render(self.stderr, width_of(&std::io::stderr())),
            false,
        );
    }

    /// Writes the command's result to `stdout` and returns the exit code.
    ///
    /// In `human` mode `human` is written; in `json` mode the envelope is. **Exactly one of the
    /// two**, so a caller cannot accidentally emit both.
    ///
    /// # A closed `stdout` is not a panic
    ///
    /// `renvor new --dry-run --output json | head -1` closes the pipe. `println!` panics on that;
    /// this does not. C-1 requires exit `0` when the result was already written and a reported
    /// write failure otherwise — and since we cannot distinguish a partial write from a complete
    /// one, a broken pipe is treated as success: the consumer got what it asked for and left.
    pub fn finish(&self, command: &str, human: &Report, result: serde_json::Value) -> Exit {
        let payload = match self.format {
            // The report redacts every dynamic field itself, on the way to text. The reporter no
            // longer redacts a finished string, because by then a styled line and an attacker's
            // line are the same bytes.
            Format::Human => human.render(self.stdout, width_of(&std::io::stdout())),
            Format::Json => {
                match serde_json::to_string_pretty(&json::Envelope::success(command, result)) {
                    Ok(text) => text,
                    Err(error) => {
                        self.note(&format!("the result could not be serialised: {error}"));
                        return Exit::Unclassified;
                    }
                }
            }
        };
        self.write_stdout(&payload)
    }

    /// Reports a failure on the correct stream and returns its exit code.
    ///
    /// In `human` mode the message goes to **`stderr`** and `stdout` stays empty. In `json` mode
    /// the envelope goes to `stdout`, because C-2 requires exactly one document on failure too —
    /// a consumer that asked for JSON needs a parseable answer precisely when something failed.
    pub fn fail(&self, command: &str, error: &CliError) -> Exit {
        match self.format {
            Format::Human => {
                // ── DETAIL VALUES ARE ESCAPED STRICTLY, AND THAT IS THE REPORT'S JOB NOW ──
                //
                // `details.destination` is a path the operator typed and `details.error` is an OS
                // message about it — both untrusted, and neither ever legitimately multi-line.
                // Every field of a report is escaped that strictly, so this no longer has to
                // remember to do it. The JSON path deliberately does **not**: there the value stays
                // raw and `serde_json` escapes it, which is what keeps `details.destination` the
                // exact bytes a consumer needs in order to act on it.
                //
                // The message may be several sentences — a failed `cargo build` arrives with the
                // compiler's own output attached — so it is split into one report line per line of
                // text rather than being handed over as a block. A report line is single-line by
                // construction, and this is where that becomes the caller's business.
                let mut report = Report::new();
                let mut lines = error.message.lines();
                report = report.status(
                    Status::Error,
                    lines.next().unwrap_or(&error.message).to_owned(),
                );
                for line in lines {
                    report = report.text(line.to_owned());
                }
                // A blank line between the message and the details, because they are different
                // kinds of thing: the message is prose an operator reads, the details are
                // structured values a script would match on. Run together they read as one
                // paragraph, which is what the multi-line messages — a TOML parse error brings its
                // own caret diagram — made obvious.
                if !error.details.is_empty() {
                    report = report.blank();
                }
                for (key, value) in &error.details {
                    report = report.row(key.clone(), value.clone());
                }
                self.emit(&report);
                error.exit()
            }
            Format::Json => {
                match serde_json::to_string_pretty(&json::Envelope::failure(command, error)) {
                    Ok(text) => {
                        self.write_stdout(&text);
                        error.exit()
                    }
                    Err(serialisation) => {
                        self.note(&format!(
                            "the error could not be serialised: {serialisation}"
                        ));
                        Exit::Unclassified
                    }
                }
            }
        }
    }

    fn write_stdout(&self, payload: &str) -> Exit {
        match self.write(payload, true) {
            Ok(()) => Exit::Success,
            // The consumer closed the pipe. It got what it asked for; this is not our failure.
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Exit::Success,
            Err(error) => {
                // Reported with `note`, which writes to the OTHER stream — so a stdout that has
                // gone away cannot swallow the report that it has gone away.
                self.note(&format!(
                    "the result could not be written to stdout: {error}"
                ));
                Exit::Unclassified
            }
        }
    }

    /// Writes one line to a stream, through a writer that enforces the colour policy.
    ///
    /// # `AutoStream` is doing two jobs, and both are load-bearing
    ///
    /// **It strips.** Constructed with `ColorChoice::Never` — which is what every forbidden path
    /// resolves to — it removes escape sequences from whatever is written through it. The policy
    /// already declines to emit them; this removes any that arrived another way. Two independent
    /// mechanisms, so a defect in either one is not a leak.
    ///
    /// **It translates.** On Windows, where virtual-terminal processing is unavailable, it turns
    /// the sequences into console API calls. No amount of care with `format!` can do that, and it
    /// is the reason this is a dependency rather than a `writeln!`.
    fn write(&self, payload: &str, to_stdout: bool) -> std::io::Result<()> {
        if to_stdout {
            let mut stream = anstream::AutoStream::new(std::io::stdout(), self.stdout.choice());
            writeln!(stream, "{payload}")?;
            stream.flush()
        } else {
            let mut stream = anstream::AutoStream::new(std::io::stderr(), self.stderr.choice());
            // A diagnostic that cannot be written is not worth failing the command over — the
            // command's own outcome is the thing the caller asked about, and `stderr` failing is
            // usually the terminal going away underneath a process that is already finishing.
            let _ = writeln!(stream, "{payload}");
            let _ = stream.flush();
            Ok(())
        }
    }
}

/// How wide a stream is, or [`None`] when it is not a terminal.
///
/// `terminal_size_of` takes the specific handle rather than assuming `stdout`, because the human
/// result and the diagnostics go to different streams and either may be redirected on its own.
///
/// # Written twice, for the same reason [`duplicate_stderr`] is
///
/// The *name* of a stream handle is platform-specific — a file descriptor on Unix, a `HANDLE` on
/// Windows — and there is no portable trait covering both. Both arms do the same thing; the
/// duplication is in the spelling.
#[cfg(unix)]
fn width_of<Handle: std::os::fd::AsFd>(handle: &Handle) -> Option<usize> {
    terminal_size::terminal_size_of(handle).map(|(width, _)| usize::from(width.0))
}

/// The Windows spelling of [`width_of`].
#[cfg(windows)]
fn width_of<Handle: std::os::windows::io::AsHandle>(handle: &Handle) -> Option<usize> {
    terminal_size::terminal_size_of(handle).map(|(width, _)| usize::from(width.0))
}

/// Duplicates this process's `stderr` into something a child process can be handed.
///
/// # Why this is written twice
///
/// `Stdio` can be built from an owned handle, but the *name* of that handle is
/// platform-specific: a file descriptor on Unix, a `HANDLE` on Windows. There is no portable
/// `From<Stderr> for Stdio`. Both arms do the same thing — borrow this process's `stderr`, clone
/// it into an owned handle, and give the clone away — so the duplication is in the spelling, not
/// in the behaviour.
///
/// The clone is what makes this safe: the child gets its own handle to the same destination, so
/// the child closing it on exit does not close this process's `stderr`.
#[cfg(unix)]
fn duplicate_stderr() -> std::io::Result<std::process::Stdio> {
    use std::os::fd::AsFd;
    Ok(std::process::Stdio::from(
        std::io::stderr().as_fd().try_clone_to_owned()?,
    ))
}

/// The Windows spelling of [`duplicate_stderr`].
#[cfg(windows)]
fn duplicate_stderr() -> std::io::Result<std::process::Stdio> {
    use std::os::windows::io::AsHandle;
    Ok(std::process::Stdio::from(
        std::io::stderr().as_handle().try_clone_to_owned()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::Code;

    #[test]
    fn the_output_flag_accepts_exactly_two_values() {
        assert_eq!(Format::parse("human").expect("human"), Format::Human);
        assert_eq!(Format::parse("json").expect("json"), Format::Json);
        let error = Format::parse("yaml").unwrap_err();
        assert_eq!(error.code, Code::UnsupportedValue);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "supported" && v == "human, json"),
            "an unsupported value must name what IS supported"
        );
    }

    #[test]
    fn the_default_format_is_human() {
        assert_eq!(Format::default(), Format::Human);
    }

    #[test]
    fn colour_is_off_when_the_flag_says_so() {
        assert!(!Reporter::new(Format::Human, true).colour());
    }

    #[test]
    fn progress_never_renders_in_json_mode() {
        // Even on a terminal. A progress bar interleaved with a JSON document on the same terminal
        // is unreadable, and in a pipeline it is worse.
        assert!(!Reporter::new(Format::Json, false).progress_visible());
    }

    #[test]
    fn a_failure_exit_code_comes_from_the_error_and_not_from_the_format() {
        // The same failure must exit the same way whether a human or a machine asked. If these
        // ever diverge, `--output json` becomes a different program.
        for code in Code::ALL {
            assert_eq!(
                code.exit(),
                CliError::new(code, "x").exit(),
                "{code} maps inconsistently"
            );
        }
    }
}
