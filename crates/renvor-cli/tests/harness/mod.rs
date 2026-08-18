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
//! See `research.md` D15.

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
const EXPECT_TIMEOUT: Duration = Duration::from_secs(300);

/// A live `renvor` process attached to a real terminal.
pub struct Terminal {
    writer: Box<dyn Write + Send>,
    receiver: mpsc::Receiver<u8>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    /// Everything read so far, with escape sequences intact.
    pub transcript: String,
}

impl Terminal {
    /// Spawns `renvor` with `args`, in `directory`, attached to a pty.
    pub fn spawn(args: &[&str], directory: &Path, env: &[(&str, &str)]) -> Self {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 40,
                cols: 120,
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
        // The slave handle must be dropped or the master never sees EOF when the child exits.
        drop(pty.slave);

        let mut reader = pty.master.try_clone_reader().expect("the pty can be read");
        let writer = pty.master.take_writer().expect("the pty can be written");
        // `portable_pty`'s reader blocks, so it lives on its own thread and feeds a channel. That
        // is what makes `expect` able to time out rather than hang the whole test binary.
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 1024];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                for byte in &buffer[..read] {
                    if sender.send(*byte).is_err() {
                        return;
                    }
                }
            }
        });

        Self {
            writer,
            receiver,
            child,
            transcript: String::new(),
        }
    }

    /// Reads until `needle` appears in the transcript.
    ///
    /// Panics with the whole transcript on timeout, because "expected `X`" without what actually
    /// arrived is the least useful failure a terminal test can produce.
    pub fn expect(&mut self, needle: &str) {
        let deadline = Instant::now() + EXPECT_TIMEOUT;
        while !self.visible().contains(needle) {
            let remaining = deadline.saturating_duration_since(Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for {needle:?}\n--- transcript ---\n{}",
                self.visible()
            );
            match self
                .receiver
                .recv_timeout(remaining.min(Duration::from_millis(250)))
            {
                Ok(byte) => self.transcript.push(byte as char),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    assert!(
                        self.visible().contains(needle),
                        "the program exited before {needle:?} appeared\n--- transcript ---\n{}",
                        self.visible()
                    );
                    return;
                }
            }
        }
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
