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

pub mod json;
pub mod redact;

use std::io::{IsTerminal, Write};

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
    colour: bool,
    /// Whether `stderr` is a terminal. Progress renders to nothing when it is not (C-1).
    stderr_is_terminal: bool,
}

impl std::fmt::Debug for Reporter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Reporter")
            .field("format", &self.format)
            .field("colour", &self.colour)
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
            // Styling is disabled by the flag OR by the stream not being a terminal. Both, not
            // either — a piped stream with `--no-color` absent must still be unstyled, or every
            // captured log fills with escape sequences.
            colour: !no_color && stderr_is_terminal,
            stderr_is_terminal,
        }
    }

    /// Whether styling would be active.
    ///
    /// # This phase emits no styling at all, and that is stated rather than implied
    ///
    /// `--no-color` is in contract C-1 and is accepted. Its observable requirement — that output
    /// carry no escape sequences — is satisfied **trivially**, because nothing here writes any.
    /// No colour dependency is taken, and none is needed until there is something to colour.
    ///
    /// The resolution logic is still computed and still tested, so that the phase which introduces
    /// styling inherits a correct answer instead of re-deriving one. It is `#[cfg(test)]` because
    /// shipping an accessor no shipped code reads would advertise a capability that does not exist.
    #[cfg(test)]
    #[must_use]
    pub fn colour(&self) -> bool {
        self.colour
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
    pub fn note(&self, message: &str) {
        // `for_terminal` AFTER `line`: redaction first, so a secret is replaced before anything
        // else looks at it, then control-sequence neutralisation on what survives. Both are needed
        // and neither subsumes the other — one hides values, the other stops the text reprogramming
        // the terminal it is printed to.
        let _ = writeln!(
            std::io::stderr(),
            "{}",
            redact::for_terminal(&redact::line(message))
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
    pub fn finish(&self, command: &str, human: &str, result: serde_json::Value) -> Exit {
        let payload = match self.format {
            Format::Human => redact::for_terminal(&redact::line(human)),
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
                self.note(&format!("error: {error}"));
                for (key, value) in &error.details {
                    // ── DETAIL VALUES ARE ESCAPED STRICTLY, AND ONLY HERE ─────────────────
                    //
                    // `details.destination` is a path the operator typed and `details.error` is an
                    // OS message about it — both untrusted, and neither ever legitimately
                    // multi-line. The JSON path deliberately does **not** do this: there the value
                    // stays raw and `serde_json` escapes it, which is what keeps
                    // `details.destination` the exact bytes a consumer needs in order to act on it.
                    self.note(&format!("  {key}: {}", redact::detail(value)));
                }
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
        let mut stdout = std::io::stdout();
        match writeln!(stdout, "{payload}").and_then(|()| stdout.flush()) {
            Ok(()) => Exit::Success,
            // The consumer closed the pipe. It got what it asked for; this is not our failure.
            Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => Exit::Success,
            Err(error) => {
                self.note(&format!(
                    "the result could not be written to stdout: {error}"
                ));
                Exit::Unclassified
            }
        }
    }
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
