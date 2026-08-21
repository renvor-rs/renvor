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

/// How many pseudo-terminals this test binary may hold open at once.
///
/// # This exists because `openpty` ran out, and did so intermittently
///
/// A pseudo-terminal is a **finite operating-system resource** — macOS caps them at
/// `kern.tty.ptmx_max`, and a CI container's limit is usually lower. Four pty-driving test files
/// running their tests in parallel, inside a `cargo test` that also runs those files in parallel,
/// reached the cap during a full verification run and produced
/// `failed to openpty: Os { code: -6 }` in one test while fifteen others passed.
///
/// That is the worst shape a failure can take: it depends on machine load, so it passes locally,
/// passes on a rerun, and fails on someone else's pull request. Capping the concurrency makes the
/// resource use bounded instead of merely usually-sufficient.
///
/// Four, rather than one: these tests are dominated by waiting for a subprocess, so serialising
/// them entirely would multiply the suite's wall-clock time for no benefit.
const CONCURRENT_TERMINALS: usize = 4;

/// The counter behind [`CONCURRENT_TERMINALS`], and the condition variable waiters sleep on.
static IN_FLIGHT: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
static RELEASED: std::sync::Condvar = std::sync::Condvar::new();

/// One terminal's share of the cap, released when the last holder drops it.
///
/// # The reader thread holds a clone, and that is the load-bearing part
///
/// Dropping a [`Terminal`] drops its writer, but the **reader thread still holds a cloned handle
/// to the same pty** and only closes it when it observes end of file, which happens a moment
/// later. A permit released when `Terminal` drops would therefore hand the slot to the next test
/// while the file descriptor was still open — which is the exact race this cap exists to remove.
///
/// So the permit is an `Arc` and the reader thread owns a clone. The slot frees when *both* are
/// gone, which is when the descriptors are actually closed.
struct Permit;

impl Drop for Permit {
    fn drop(&mut self) {
        let mut count = IN_FLIGHT
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *count = count.saturating_sub(1);
        RELEASED.notify_one();
    }
}

/// How long to wait for a slot before giving up on the cap and proceeding anyway.
///
/// # The cap must never be the reason a suite hangs
///
/// The first version of this waited on the condition variable **with no timeout**. A permit is
/// released only once the reader thread has also finished with its handle, and that thread ends
/// when the pty reaches end of file — so anything leaving a child alive holds a slot
/// indefinitely, and after [`CONCURRENT_TERMINALS`] of those every later test blocks forever with
/// no diagnostic at all.
///
/// That is the worst failure a suite can have. A hang produces no test name, no assertion, and no
/// output; on CI it is indistinguishable from a slow machine until the job is killed. An advisory
/// review predicted exactly this, and it then happened on the Windows leg — forty-one minutes of
/// silence and `The operation was canceled`, with nothing in the log naming the test responsible.
///
/// So the cap is now **advisory**. It is an optimisation that avoids exhausting a finite operating
/// system resource; it is not a correctness requirement, and it is never worth a deadlock.
/// Overshooting prints why and carries on — a noisy pass beats a silent hang.
const SLOT_WAIT: Duration = Duration::from_secs(30);

fn acquire_terminal_slot() -> std::sync::Arc<Permit> {
    let mut count = IN_FLIGHT
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let deadline = Instant::now() + SLOT_WAIT;
    while *count >= CONCURRENT_TERMINALS {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            eprintln!(
                "harness: {} pseudo-terminals still in flight after {}s; proceeding without a \
                 slot. An earlier test left a child alive.",
                *count,
                SLOT_WAIT.as_secs()
            );
            break;
        }
        let (guard, _) = RELEASED
            .wait_timeout(count, remaining)
            .unwrap_or_else(|poison| poison.into_inner());
        count = guard;
    }
    *count += 1;
    std::sync::Arc::new(Permit)
}

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
    /// This terminal's share of [`CONCURRENT_TERMINALS`]. Dropped last, by the reader thread.
    _permit: std::sync::Arc<Permit>,
}

impl Terminal {
    /// Spawns `renvor` with `args`, in `directory`, attached to a very wide pty.
    pub fn spawn(args: &[&str], directory: &Path, env: &[(&str, &str)]) -> Self {
        Self::spawn_sized(args, directory, env, 1000)
    }

    /// Spawns `renvor` attached to a pty of a chosen width.
    ///
    /// # Why the width is a parameter at all
    ///
    /// Contract C-8's layout rules are **about** the width: leaders appear when a run of them
    /// fits and the row stacks when it does not, and the threshold is derived from the content
    /// rather than from a column count. A harness fixed at one width can assert neither branch.
    ///
    /// [`Terminal::spawn`] keeps 1000 for everything else, for the reason recorded below.
    pub fn spawn_sized(
        args: &[&str],
        directory: &Path,
        env: &[(&str, &str)],
        columns: u16,
    ) -> Self {
        // Before `openpty`, not after: the cap is only a cap if it is taken while the resource is
        // still free.
        let permit = acquire_terminal_slot();
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
                cols: columns,
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
        // matrix legs. The prompt library still draws its prompts; only the colouring is
        // suppressed. `TERM=dumb` and `NO_COLOR` are two of the five refusals in contract C-8,
        // so a transcript captured here carries no SGR sequences at all — which is what lets
        // these tests assert on text rather than on styled text.
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
        // The thread's own clone of the permit. See [`Permit`]: the slot is not free until this
        // thread has finished with its handle to the pty, which is later than `Terminal::drop`.
        let held = std::sync::Arc::clone(&permit);
        std::thread::spawn(move || {
            let _held = held;
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
            _permit: permit,
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
    /// `crossterm` — which the prompt library used to render through — asked the terminal where
    /// the cursor is by
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
    /// the prompt libraries emit (CSI `\x1b[…<final>` and the two-byte `\x1b<char>`), and a
    /// general-purpose stripper would be a larger surface for one screenful of text.
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

    /// How long to wait for the child to exit before declaring it hung and naming the test.
    ///
    /// Generous — a run that reaches the review screen has already run `cargo build` and
    /// `cargo test` on the generated project, and the Windows runner is the slowest of the three.
    /// What matters is that it is **finite**.
    const EXIT_TIMEOUT: Duration = Duration::from_secs(300);

    /// Sends one keypress, with **no** carriage return after it.
    ///
    /// # Why this is not [`Terminal::send_line`]
    ///
    /// A confirmation now **submits on the keypress**: `y` answers yes and `n` answers no, without
    /// an Enter. That is the behaviour of the prompt library this crate adopted on 2026-08-21, and
    /// it differs from the previous one, which read `y`/`n` as text and waited for Enter.
    ///
    /// `send_line("y")` therefore writes one keystroke too many. The confirmation consumes the `y`
    /// and the stray carriage return falls through to the **next** prompt, silently accepting its
    /// default — which is exactly what happened to two tests here when the library changed, and
    /// exactly what will happen to an operator with the old muscle memory. It is recorded as a
    /// known interaction difference rather than hidden behind a harness that papers over it.
    pub fn key(&mut self, key: &str) {
        write!(self.writer, "{key}").expect("the pty accepts input");
        self.writer.flush().expect("the pty flushes");
    }

    /// Sends ESC, which cancels a prompt.
    pub fn escape(&mut self) {
        write!(self.writer, "\u{1b}").expect("the pty accepts input");
        self.writer.flush().expect("the pty flushes");
    }

    /// Waits for exit and returns the code.
    pub fn wait(&mut self) -> i32 {
        // ── DRAIN WHILE WAITING. THIS LOOP IS THE LOAD-BEARING PART. ────────────────────
        //
        // This used to be a bare `child.wait()`, which reads nothing until the child has already
        // exited. Every pty test written before `tests/terminal.rs` calls `expect` first, so the
        // terminal was always being read continuously and nobody noticed. Seven newer tests go
        // straight from `spawn` to `wait` — a shape the harness had never been exercised with —
        // and on ConPTY that **deadlocks**: the console host stops accepting writes once its
        // buffer is full, so the child blocks mid-write and never exits, while `wait` blocks on a
        // child that is waiting to be read.
        //
        // It cost two forty-five-minute Windows jobs to find, because a deadlock prints nothing:
        // both were cancelled at the job timeout with the log ending mid-suite and no test named.
        // Linux and macOS never showed it — their pty buffers are larger than anything these
        // tests write.
        //
        // Draining here makes the calling shape stop mattering, which is better than a rule that
        // every test must remember to `expect` before it waits.
        let deadline = Instant::now() + Self::EXIT_TIMEOUT;
        loop {
            while let Ok(byte) = self.receiver.try_recv() {
                self.transcript.push(byte as char);
            }
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    // A NAMED FAILURE, NOT A HANG. The whole reason the two Windows jobs taught
                    // nothing is that a blocked `wait` produces no test name, no assertion, and no
                    // output — on CI it is indistinguishable from a slow machine until the job is
                    // killed.
                    let _ = self.child.kill();
                    panic!(
                        "the child did not exit within {}s and was killed{}",
                        Self::EXIT_TIMEOUT.as_secs(),
                        self.diagnosis()
                    );
                }
                Err(error) => panic!("the child could not be waited on: {error}"),
            }
        }
        let status = self.child.wait().expect("the child is waitable");
        // Whatever is still in flight, so a failure message shows the final screen.
        while let Ok(byte) = self.receiver.recv_timeout(Duration::from_millis(200)) {
            self.transcript.push(byte as char);
        }
        i32::try_from(status.exit_code()).unwrap_or(-1)
    }
}

impl Drop for Terminal {
    /// Kills the child, so that a failed test releases its pty slot.
    ///
    /// # Without this, one timeout wedges the whole binary
    ///
    /// The slot is held by an `Arc<Permit>` shared with the reader thread, and that thread only
    /// ends when the pty reaches end of file — which only happens when the child exits. A test
    /// whose `expect` times out panics while its child is still blocked on a prompt nobody will
    /// answer, so the child never exits, the reader never ends, and the permit is never released.
    ///
    /// Four such failures exhaust [`CONCURRENT_TERMINALS`], and `acquire_terminal_slot` then
    /// blocks on a condition variable with no timeout. Every remaining pty test hangs with no
    /// diagnostic at all — and a suite that hangs is strictly worse than one reporting four
    /// failures, because the four failures name themselves.
    ///
    /// `kill` on a child that has already exited fails harmlessly, which is the common case.
    fn drop(&mut self) {
        let _ = self.child.kill();
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
