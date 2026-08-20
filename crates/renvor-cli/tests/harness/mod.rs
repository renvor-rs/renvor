//! Shared test harness: a real terminal, and the plain subprocess runner beside it.
//!
//! # Why a pseudo-terminal at all
//!
//! `renvor new` decides whether to run the wizard from `stdin.is_terminal()`. A test that pipes
//! stdin therefore exercises the **flag** path no matter what it writes into the pipe — which is
//! how a suite can assert prompt behaviour at length and never once run a prompt. Driving the real
//! binary through a pty is the only way the wizard code is the code under test.
//!
//! # The harness runs on Windows too, and that decided which crate
//!
//! `rexpect` is the obvious expect-style choice and it is **Unix-only**. Five of the ten defects
//! this phase found were caught by the platform matrix, so coverage that silently vanishes on
//! Windows is coverage that would have missed half of them. `portable-pty` wraps ConPTY on Windows
//! and `openpty` elsewhere behind one interface, so these tests run on every matrix leg.
//! See [Phase 003 research §D15](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/research.md).

#![allow(dead_code)] // Each integration test binary uses a different subset of this module.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// How long any single `expect` waits before declaring the program hung.
///
/// Generous, because a run that reaches the review screen has already run `cargo build` and
/// `cargo test` on the generated project. A hang is still a failure rather than a timeout that
/// gets retried — see the module note in `transaction.rs`.
const EXPECT_TIMEOUT: Duration = Duration::from_secs(120);

/// A live `renvor` process attached to a real terminal.
pub struct Terminal {
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<u8>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Held open deliberately. On Windows the slave and the master share the `PsuedoCon` behind an
    /// `Arc`, so dropping the slave does not close it — but the ordering is load-bearing enough on
    /// both platforms that it is kept rather than relied upon.
    _slave: Box<dyn portable_pty::SlavePty + Send>,
    /// Everything read so far, with escape sequences intact.
    pub transcript: String,
    /// Why the reader thread stopped, if it has. Used to make a timeout diagnostic rather than
    /// merely a timeout.
    reader_ended: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    /// How many cursor-position reports we have already answered. See [`Terminal::answer_status_reports`].
    reports_answered: usize,
}

impl Terminal {
    /// Spawns `renvor` with `args`, in `directory`, attached to a pty.
    pub fn spawn(args: &[&str], directory: &Path, env: &[(&str, &str)]) -> Self {
        let pty = native_pty_system()
            // DELIBERATELY VERY WIDE, AND THIS IS NOT COSMETIC.
            //
            // A pty wraps output at its column count, and ConPTY mirrors the resulting **screen**
            // rather than the bytes the program wrote. At 120 columns a Windows temporary path
            // pushed the review screen's "equivalent command" line past the edge, so the transcript
            // contained a wrapped and partly merged rendering of a command that the program had
            // emitted as one line. A test that then parsed it got a truncated command.
            //
            // 1000 columns is wider than anything these tests print, so no wrapping decision by the
            // emulator can change what the transcript says. The alternative — teaching the harness
            // to un-wrap — means modelling the screen, which is writing a terminal emulator.
            .openpty(PtySize {
                rows: 200,
                cols: 1000,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("a pseudo-terminal can be opened");

        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_renvor"));
        for argument in args {
            command.arg(argument);
        }
        command.cwd(directory);
        // Keep the prompt rendering stable and free of hyperlink/styling variation between the
        // matrix legs. `inquire` still draws its prompts; only the colouring is suppressed.
        command.env("NO_COLOR", "1");
        command.env("TERM", "dumb");
        for (key, value) in env {
            command.env(key, value);
        }

        let child = pty
            .slave
            .spawn_command(command)
            .expect("renvor starts on the pty");

        let mut reader = pty.master.try_clone_reader().expect("the pty can be read");
        let writer = pty.master.take_writer().expect("the pty can be written");
        // `portable_pty`'s reader blocks, so it lives on its own thread and feeds a channel. That
        // is what makes `expect` able to time out rather than hang the whole test binary.
        let (sender, receiver) = mpsc::channel();
        let reader_ended = std::sync::Arc::new(std::sync::Mutex::new(None));
        let ended = std::sync::Arc::clone(&reader_ended);
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 1024];
            let mut total = 0_usize;
            let reason = loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break format!("clean end of file after {total} bytes"),
                    Ok(read) => {
                        total += read;
                        for byte in &buffer[..read] {
                            if sender.send(*byte).is_err() {
                                return;
                            }
                        }
                    }
                    // A signal arriving mid-read is not the end of the stream. Treating it as one
                    // turns an unrelated interruption into "the program exited", which is a
                    // diagnosis that sends somebody to look in the wrong place.
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(error) => break format!("read failed after {total} bytes: {error}"),
                }
            };
            *ended.lock().expect("the reason mutex is not poisoned") = Some(reason);
        });

        Self {
            writer,
            receiver,
            child,
            _slave: pty.slave,
            transcript: String::new(),
            reader_ended,
            reports_answered: 0,
        }
    }

    /// Reads until `needle` appears in the transcript.
    ///
    /// Panics with the whole transcript on timeout, because "expected `X`" without what actually
    /// arrived is the least useful failure a terminal test can produce.
    pub fn expect(&mut self, needle: &str) {
        let deadline = Instant::now() + EXPECT_TIMEOUT;
        loop {
            self.answer_status_reports();
            if self.visible().contains(needle) {
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                panic!("timed out waiting for {needle:?}{}", self.diagnosis());
            }
            match self
                .receiver
                .recv_timeout(remaining.min(Duration::from_millis(250)))
            {
                Ok(byte) => self.transcript.push(byte as char),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    assert!(
                        self.visible().contains(needle),
                        "the program exited before {needle:?} appeared{}",
                        self.diagnosis()
                    );
                    return;
                }
            }
        }
    }

    /// Answers the terminal's cursor-position query (`ESC [ 6 n`).
    ///
    /// # Without this, the Windows leg deadlocks, and the reason is worth stating
    ///
    /// `crossterm` — which `inquire` renders through — asks the terminal where the cursor is by
    /// writing a **Device Status Report** and then **blocking until the terminal answers**. A real
    /// terminal emulator replies `ESC [ <row> ; <col> R`. A test harness that only *reads* the pty
    /// is not a terminal emulator, so nobody ever answers and the child waits forever.
    ///
    /// This was not a guess. The first Windows run reported, from the harness's own diagnosis:
    /// *child is STILL RUNNING, reader still running, **bytes received: 4***. Four bytes is exactly
    /// `ESC [ 6 n`. The prompt was never drawn because the program was still waiting to be told
    /// where it was drawing.
    ///
    /// It does not reproduce on Unix, where `crossterm` obtains the position without a round trip.
    /// That asymmetry is the whole reason the Windows matrix leg is worth running: the harness was
    /// **incomplete on every platform** and only one platform said so.
    ///
    /// The reply is always `1;1`. These tests match on prompt text, never on layout, so a
    /// truthful cursor position would buy nothing — and tracking one would mean implementing a
    /// screen model, which is a terminal emulator, which is not what this file should become.
    fn answer_status_reports(&mut self) {
        const QUERY: &str = "\u{1b}[6n";
        let asked = self.transcript.matches(QUERY).count();
        while self.reports_answered < asked {
            // Written directly rather than through `send_line`: this is a protocol reply, not
            // operator input, and it must not carry a carriage return.
            let _ = write!(self.writer, "\u{1b}[1;1R");
            let _ = self.writer.flush();
            self.reports_answered += 1;
        }
    }

    /// Everything known about why a wait failed.
    ///
    /// A bare "timed out waiting for X" with an empty transcript is the least useful failure a
    /// terminal test can produce, and it is exactly what the first Windows run produced. This adds
    /// the three facts that separate the plausible causes: whether the child is still running,
    /// whether the reader thread is still going and why it stopped if not, and how many bytes ever
    /// arrived.
    fn diagnosis(&mut self) -> String {
        let child = match self.child.try_wait() {
            Ok(Some(status)) => format!("child EXITED with code {}", status.exit_code()),
            Ok(None) => "child is STILL RUNNING".to_owned(),
            Err(error) => format!("child status unavailable: {error}"),
        };
        let reader = match self
            .reader_ended
            .lock()
            .expect("the reason mutex is not poisoned")
            .clone()
        {
            Some(reason) => format!("reader STOPPED: {reason}"),
            None => "reader still running".to_owned(),
        };
        let queries = self.transcript.matches("\u{1b}[6n").count();
        format!(
            "\n  platform: {}\n  {child}\n  {reader}\n  bytes received: {}\n  cursor-position queries seen: {queries}, answered: {}\n--- transcript ---\n{}",
            std::env::consts::OS,
            self.transcript.len(),
            self.reports_answered,
            self.visible()
        )
    }

    /// The transcript with ANSI escape sequences removed.
    ///
    /// Hand-written rather than pulled in as a dependency: this recognises exactly the two forms
    /// `inquire` emits (CSI `\x1b[…<final>` and the two-byte `\x1b<char>`), and a general-purpose
    /// stripper would be a larger surface for one screenful of text.
    pub fn visible(&self) -> String {
        let mut out = String::with_capacity(self.transcript.len());
        let mut characters = self.transcript.chars().peekable();
        while let Some(character) = characters.next() {
            if character != '\u{1b}' {
                out.push(character);
                continue;
            }
            match characters.peek() {
                Some('[') => {
                    characters.next();
                    for inner in characters.by_ref() {
                        if inner.is_ascii_alphabetic() || inner == '~' {
                            break;
                        }
                    }
                }
                Some(_) => {
                    characters.next();
                }
                None => {}
            }
        }
        out
    }

    /// Sends `line` followed by a carriage return, the way a terminal delivers Enter.
    pub fn send_line(&mut self, line: &str) {
        write!(self.writer, "{line}\r").expect("the pty accepts input");
        self.writer.flush().expect("the pty flushes");
    }

    /// Sends Enter alone, accepting a prompt's default.
    pub fn enter(&mut self) {
        self.send_line("");
    }

    /// Sends ESC — `inquire`'s cancellation key.
    pub fn escape(&mut self) {
        write!(self.writer, "\u{1b}").expect("the pty accepts input");
        self.writer.flush().expect("the pty flushes");
    }

    /// Waits for exit and returns the code.
    pub fn wait(&mut self) -> i32 {
        let status = self.child.wait().expect("the child is waitable");
        // Drain whatever is still in flight so failure messages show the final screen.
        while let Ok(byte) = self.receiver.recv_timeout(Duration::from_millis(200)) {
            self.transcript.push(byte as char);
        }
        i32::try_from(status.exit_code()).unwrap_or(-1)
    }
}

/// Runs `renvor` as an ordinary subprocess and returns (exit code, stdout, stderr).
///
/// `stdin` is not a terminal here, which is exactly what makes this the **flag** path.
pub fn renvor(args: &[&str], directory: &Path, env: &[(&str, &str)]) -> (i32, String, String) {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_renvor"));
    command.args(args).current_dir(directory);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("renvor runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}
