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

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

/// How long any single `expect` waits before declaring the program hung.
///
/// Generous, because a run that reaches the review screen has already run `cargo build` and
/// `cargo test` on the generated project. A hang is still a failure rather than a timeout that
/// gets retried — see the module note in `transaction.rs`.
const EXPECT_TIMEOUT: Duration = Duration::from_secs(120);

/// How long [`Terminal::await_input_readiness`] waits for the child to put the terminal into raw
/// mode.
///
/// Generous against a loaded CI runner and irrelevant in the ordinary case: the measured gap
/// between the prompt's bytes arriving and `ISIG` clearing is **583ns**. What matters is that it
/// is finite, and that running out is a named failure rather than a key sent into the dark.
const READINESS_TIMEOUT: Duration = Duration::from_secs(30);

/// How often [`Terminal::await_input_readiness`] asks the kernel.
///
/// A bare spin would burn a core while the child is doing real work; a millisecond would add a
/// millisecond to every cancellation test. This is a `tcgetattr` — a cheap ioctl, not a syscall
/// that sleeps — so the loop costs one of them per interval and nothing in between.
const READINESS_POLL: Duration = Duration::from_micros(100);

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
    /// The master end, kept so that [`Terminal::await_input_readiness`] can ask the kernel what
    /// mode the terminal is in. Reading and writing go through the cloned handles above; this is
    /// held for its `termios`, not for its bytes.
    master: Box<dyn MasterPty + Send>,
    /// Everything read so far, with escape sequences intact.
    pub transcript: String,
    /// How far [`Terminal::expect_new`] has already matched, as an offset into [`Terminal::visible`].
    ///
    /// Monotonic. This is the whole difference between `expect` and `expect_new`: without it, a
    /// second expectation for text the transcript already contains returns immediately and
    /// synchronises nothing. See [`Terminal::expect_new`] for the measurement.
    matched: usize,
    /// The signal that killed the child, if one did. Recorded by [`Terminal::wait`].
    signal: Option<String>,
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
            master: pty.master,
            transcript: String::new(),
            matched: 0,
            signal: None,
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

    /// Reads until `needle` appears in output that has **not already been matched**.
    ///
    /// # Why `expect` is not enough, with the measurement
    ///
    /// [`Terminal::expect`] searches the whole transcript from its first byte. That is right for
    /// "has this ever appeared", and wrong for "has this appeared *now*" — and the cancellation
    /// tests wanted the second. `wizard()` waited for `Project name`; each test then waited for
    /// `Project name` again as its barrier before sending a key. The second wait was a **no-op**:
    /// measured at 458ns having read **0 new bytes**, because the text it was waiting for had
    /// arrived before the function was called. A barrier that cannot block is not a barrier.
    ///
    /// The cursor is an offset into [`Terminal::visible`] rather than into the raw transcript,
    /// because that is the text callers match on. Escape stripping is left to right, so bytes
    /// arriving later never change how earlier bytes were read, and the prefix behind the cursor
    /// is stable.
    pub fn expect_new(&mut self, needle: &str) {
        let deadline = Instant::now() + EXPECT_TIMEOUT;
        loop {
            self.answer_status_reports();
            let visible = self.visible();
            if let Some(offset) = visible
                .get(self.matched..)
                .and_then(|rest| rest.find(needle))
            {
                self.matched += offset + needle.len();
                return;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let matched = self.matched;
                panic!(
                    "timed out waiting for {needle:?} in output after offset {matched}{}",
                    self.diagnosis()
                );
            }
            match self
                .receiver
                .recv_timeout(remaining.min(Duration::from_millis(250)))
            {
                Ok(byte) => self.transcript.push(byte as char),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let visible = self.visible();
                    let found = visible
                        .get(self.matched..)
                        .and_then(|rest| rest.find(needle));
                    let Some(offset) = found else {
                        panic!(
                            "the program exited before {needle:?} appeared{}",
                            self.diagnosis()
                        );
                    };
                    // Advance here too. Returning without moving the cursor would make a second
                    // wait for the same needle match the same text — which is precisely the
                    // no-op this method exists to abolish, reintroduced on the exit path.
                    self.matched += offset + needle.len();
                    return;
                }
            }
        }
    }

    /// How far [`Terminal::expect_new`] has matched, as an offset into [`Terminal::visible`].
    ///
    /// Exposed so a test can assert **which** occurrence of a needle was matched, rather than only
    /// that some bytes arrived. See
    /// `terminal.rs::a_barrier_may_not_be_satisfied_by_output_that_already_arrived`.
    pub fn matched_offset(&self) -> usize {
        self.matched
    }

    /// Blocks until the terminal is in a mode where a keypress is **data**, not a signal.
    ///
    /// # This is a state probe, and every text-based barrier before it was a guess
    ///
    /// The prompt library reads one key at a time through `console::Term::read_key_raw`, and that
    /// function switches the terminal into raw mode **inside** the read: `tcsetattr(RAW)`, read
    /// one key, `tcsetattr(original)`. The library's own loop renders, writes the frame, flushes,
    /// and *then* calls it. So the order on the wire is
    ///
    /// ```text
    ///   render → write → flush → [ tcsetattr(RAW) → read → tcsetattr(canonical) ]
    ///                     ↑                ↑
    ///          the prompt becomes     raw mode actually
    ///          visible here           begins here
    /// ```
    ///
    /// Waiting for the drawn prompt therefore lands a test at the **start** of the canonical
    /// window, not past it. In that window `ISIG` is still set, so the line discipline turns
    /// `\x03` into `SIGINT` for the foreground process group as the byte arrives — the program
    /// never sees a key at all, and dies from the signal instead of exiting `4`.
    ///
    /// A pty's `termios` is one kernel object shared by both ends, so the master can simply ask
    /// what mode the slave is in. That is a fact about state rather than an inference from output,
    /// and it is why this cannot be fooled by stale bytes the way a transcript match can.
    ///
    /// **What it establishes is `ISIG` clear, which is narrower than "raw mode".** `ISIG` is the
    /// flag that decides whether the line discipline converts a control character into a signal,
    /// and it is the only one this test needs. `console` also clears `ECHO`, `ECHONL`, `ICANON`
    /// and `IEXTEN` in the same call, and restores `c_oflag` untouched — so `OPOST` is never off.
    /// None of that is asserted here, because none of it decides the outcome.
    ///
    /// Observing `ISIG` clear places the child between its `tcsetattr` and its `read`, which is at
    /// or just before the read rather than provably inside it. The guarantee that matters survives
    /// either way: nothing between those two points changes the mode again, and the read cannot
    /// return until a byte arrives.
    ///
    /// **That is a precondition on the caller, not a law.** It holds while the pty's input queue
    /// is empty, which is the case whenever the previous key has already been consumed. Every
    /// caller here satisfies it: each either sends nothing before the probe, or waits with
    /// [`Terminal::expect_new`] for the redraw that only happens *after* the library's read
    /// returned. A caller that wrote a key and did not wait for its effect could observe raw mode
    /// that is about to end.
    ///
    /// # Measured
    ///
    /// Ten runs waiting on this probe and then sending `\x03`: exit `4` every time. Ten runs with
    /// the terminal forced back to canonical at the same point: exit `1` with `Interrupt: 2` every
    /// time — the CI failure, on demand.
    ///
    /// # Panics
    ///
    /// If the mode cannot be read, or is still canonical at [`READINESS_TIMEOUT`]. Returning
    /// quietly would put the guess back, which is the defect this replaces.
    #[cfg(unix)]
    pub fn await_input_readiness(&mut self) {
        use nix::sys::termios::LocalFlags;

        let deadline = Instant::now() + READINESS_TIMEOUT;
        loop {
            let Some(termios) = self.master.get_termios() else {
                panic!(
                    "the terminal's mode could not be read, so raw mode cannot be proven{}",
                    self.diagnosis()
                );
            };
            if !termios.local_flags.contains(LocalFlags::ISIG) {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "the terminal was still in canonical mode ({}s) — a keypress sent now would \
                     be delivered as a signal, not as input{}",
                    READINESS_TIMEOUT.as_secs(),
                    self.diagnosis()
                );
            }
            std::thread::sleep(READINESS_POLL);
        }
    }

    /// Whether the terminal is currently in a mode that delivers an interrupt character as a
    /// **signal** rather than as input.
    ///
    /// Exists so that a test can assert the boundary rather than merely rely on it. See
    /// `terminal.rs::a_key_that_arrives_before_raw_mode_is_delivered_as_a_signal`.
    #[cfg(unix)]
    pub fn interrupts_are_signals(&self) -> bool {
        use nix::sys::termios::LocalFlags;

        self.master
            .get_termios()
            .expect("the terminal's mode can be read")
            .local_flags
            .contains(LocalFlags::ISIG)
    }

    /// Puts the terminal back into the mode it occupies between the prompt's flush and the
    /// library's `tcsetattr` — canonical, with signal generation on.
    ///
    /// # This is a control, and nothing else may call it
    ///
    /// The race this file exists to close is microseconds wide, so a test that tries to *catch*
    /// it is a test that fails to catch it almost every time. This widens it to the width of a
    /// test instead.
    ///
    /// It restores **`ISIG`** — the flag that decides whether the line discipline turns an
    /// interrupt character into a signal — along with `ICANON`, and deliberately nothing else.
    ///
    /// Everything else `console`'s one-key read changes is left exactly as the child set it. That
    /// read clears `ECHO`, `ECHONL`, `ICANON`, `ISIG` and `IEXTEN` from `c_lflag`; `ECHOE` is not
    /// among them. It changes no output flag at all in the mode it installs: `make_raw` clears
    /// `OPOST`, and the caller then copies the original `c_oflag` back over it before the
    /// `tcsetattr`, so `OPOST` is never off in the mode the child runs under and there is nothing
    /// here to put back.
    ///
    /// What that leaves is the mode that decides the property under test, not a faithful copy of
    /// the child's inter-frame state — and describing it as one would be the same kind of
    /// confident half-truth that cost this test three CI runs.
    ///
    /// It touches only this test's own pty, and it changes nothing about the program under test —
    /// no delay is inserted into it, and it is not told it is being tested. That is the whole
    /// reason it is preferred to a hook or a sleep.
    ///
    /// The slave end is opened through [`open_slave`], which records what `O_NOCTTY` does and does
    /// not buy here rather than repeating it.
    #[cfg(unix)]
    pub fn force_canonical_mode(&mut self) {
        let name = self
            .master
            .tty_name()
            .expect("the pty knows its slave's name");
        let tty = open_slave(&name);
        set_signal_generation(&tty, true);
    }

    /// Widens the render-to-readiness window to `window`, and lets it close on its own.
    ///
    /// # This is the regression harness, and the duration is not load-bearing
    ///
    /// The real window is the handful of instructions between the prompt library's `flush` and
    /// its `tcsetattr` — too narrow to lose against on purpose, which is exactly why the defect
    /// survived three CI failures and three green reruns. This makes the *same* window wide
    /// enough to lose against every time, and then closes it, so a barrier that genuinely waits
    /// for raw mode still ends up sending its key into raw mode.
    ///
    /// The duration is a floor, not a timing assumption: a barrier that waits simply waits longer,
    /// and one that does not wait fails whatever the number is. Nothing is inserted into the
    /// program under test — this changes the mode of the test's own pty and nothing else.
    ///
    /// # The restoring thread holds a descriptor, not a pathname
    ///
    /// It would be natural to let the thread reopen the slave by name when it wakes. That is a
    /// **cross-test hazard**: if the test panics before the returned guard is dropped, the
    /// `Terminal` is dropped first, the pty closes, and `/dev/pts/N` becomes available again — to
    /// one of the other [`CONCURRENT_TERMINALS`] running alongside. The sleeping thread would then
    /// reopen that name and clear `ISIG` on **somebody else's terminal**, turning one failure into
    /// a second, unrelated, and much harder one.
    ///
    /// Opening once and moving the descriptor in removes the possibility: the thread can only ever
    /// affect the terminal it was given, and if that terminal has since closed the restore fails
    /// harmlessly instead of finding a new victim.
    #[cfg(unix)]
    #[must_use = "the guard joins the restoring thread; dropping it immediately defeats that"]
    pub fn widen_the_readiness_window(&mut self, window: Duration) -> ClosingWindow {
        let name = self
            .master
            .tty_name()
            .expect("the pty knows its slave's name");
        let tty = open_slave(&name);
        set_signal_generation(&tty, true);
        ClosingWindow(Some(std::thread::spawn(move || {
            std::thread::sleep(window);
            // Best effort, and deliberately so. If the test has already failed its `Terminal` may
            // be gone and this descriptor dead; panicking here would report a second failure from
            // a thread nobody is watching. It cannot hide a real problem: a restore that silently
            // failed during a healthy run leaves the child waiting for a key it can never receive,
            // and `Terminal::wait` names that as a hang against its own deadline.
            let _ = try_set_signal_generation(&tty, false);
        })))
    }

    /// Whether a signal killed the child. Meaningful only after [`Terminal::wait`].
    pub fn was_signalled(&self) -> bool {
        self.signal.is_some()
    }

    /// On Windows there is no `termios` to ask, and this is a **documented gap**, not a fix.
    ///
    /// `console` scopes the equivalent change — clearing `ENABLE_PROCESSED_INPUT` so that Ctrl-C
    /// arrives as a key rather than as a console control event — to the same one-key read, so the
    /// window exists here too. What does not exist is a way to observe it: ConPTY exposes the
    /// child's console mode to the child, not to whoever holds the other end of the pipe.
    ///
    /// So Windows keeps exactly the barrier it had — the caller's wait for the drawn prompt — and
    /// keeps its residual exposure with it. This returns rather than panicking because failing the
    /// Windows leg outright would trade a rare flake for a certain failure, and it is named here
    /// rather than left as a silent difference in behaviour between the legs.
    #[cfg(not(unix))]
    pub fn await_input_readiness(&mut self) {}

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

    /// How the child ended, in words rather than as a number.
    ///
    /// Empty until [`Terminal::wait`] has run. See the note there: an exit code alone cannot say
    /// whether the program chose its exit or a signal chose it for the program.
    pub fn outcome(&self) -> String {
        match &self.signal {
            Some(signal) => format!(
                "the child was KILLED BY {signal} — it did not choose this exit, and \
                 `portable-pty` reports a signalled child as code 1"
            ),
            None => "the child exited on its own".to_owned(),
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
        let signal = match &self.signal {
            Some(signal) => format!("killed by {signal}"),
            None => "no signal recorded".to_owned(),
        };
        format!(
            "\n  platform: {}\n  {child}\n  {reader}\n  {signal}\n  bytes received: {}\n  cursor-position queries seen: {queries}, answered: {}\n--- transcript ---\n{}",
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
            // ── AND ANSWER THE TERMINAL'S QUESTIONS. THIS IS THE OTHER HALF. ────────────
            //
            // ConPTY asks where the cursor is (`\x1b[6n`) and **blocks until something replies**.
            // `expect` has always answered those; `wait` never did, because until this branch no
            // test reached `wait` without going through `expect` first.
            //
            // The failure this produced is worth recording exactly, because it looked like a
            // product defect and was not. The diagnostic said:
            //
            //     child EXITED with code 1
            //     reader still running
            //     bytes received: 4
            //     cursor-position queries seen: 1, answered: 0
            //
            // Four bytes and one unanswered question: the child had written `\x1b[6n`, was
            // waiting for a reply that only `expect` knew how to send, and sat there until the
            // 300-second deadline killed it. Two Windows jobs were cancelled at forty-five
            // minutes before the deadline existed to say so.
            self.answer_status_reports();
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
        // ── RECORD THE SIGNAL, BECAUSE DISCARDING IT COST THREE MISREADINGS. ────────────
        //
        // `portable-pty` reports a signalled child as `code: 1, signal: Some(..)`, because
        // `std::process::ExitStatus::code()` is `None` for one and its `From` impl falls back to
        // `unwrap_or(1)`. Returning only the code therefore turns "killed by SIGINT" into a bare
        // `1` — indistinguishable from `Code::Internal`, which is how three CI failures of
        // `control_c_at_a_prompt_is_a_cancellation` came to be read as a prompt-library
        // classification bug when the program had never run far enough to classify anything.
        self.signal = status.signal().map(str::to_owned);
        // Whatever is still in flight, so a failure message shows the final screen.
        while let Ok(byte) = self.receiver.recv_timeout(Duration::from_millis(200)) {
            self.transcript.push(byte as char);
        }
        i32::try_from(status.exit_code()).unwrap_or(-1)
    }
}

/// Opens the slave end of a pty by name, for reading and changing its mode.
///
/// # `O_NOCTTY`, and an honest account of what it buys
///
/// A session leader with no controlling terminal that opens a terminal **acquires it** as its
/// controlling terminal. In this harness that acquisition cannot actually happen: `portable-pty`
/// makes the child a session leader and claims the slave with `TIOCSCTTY` before any of this runs,
/// so the terminal already belongs to a session and is not available to be claimed again.
///
/// The flag is kept because the alternative is a test helper whose safety depends on a spawn
/// detail in another crate, and because a test binary on CI genuinely can be a session leader with
/// no controlling terminal of its own. It is defensive rather than load-bearing, which is a
/// smaller claim than the one this comment used to make.
#[cfg(unix)]
fn open_slave(name: &Path) -> std::fs::File {
    use std::os::unix::fs::OpenOptionsExt;

    use nix::fcntl::OFlag;

    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(OFlag::O_NOCTTY.bits())
        .open(name)
        .unwrap_or_else(|error| panic!("the slave end {name:?} can be opened: {error}"))
}

/// Turns the line discipline's signal generation on or off for an already-open terminal.
///
/// `on` is the canonical, ordinary-shell state, where the driver converts the `VINTR` character
/// into `SIGINT` as it arrives. `off` is what the prompt library establishes for the duration of
/// each one-key read, where the same byte is delivered as input.
///
/// This toggles `ISIG` and `ICANON` and **nothing else** — not a reconstruction of the child's
/// inter-frame mode, which also has `ECHO`, `ECHONL` and `IEXTEN` set. Those two are the flags
/// that decide whether an interrupt character becomes a signal or a byte, which is the only
/// property the tests using this care about.
#[cfg(unix)]
fn set_signal_generation(tty: &std::fs::File, on: bool) {
    try_set_signal_generation(tty, on).expect("the terminal's mode can be changed");
}

/// [`set_signal_generation`] without the panic, for the one caller that runs after a test may
/// already have failed. See [`Terminal::widen_the_readiness_window`].
#[cfg(unix)]
fn try_set_signal_generation(tty: &std::fs::File, on: bool) -> nix::Result<()> {
    use nix::sys::termios::{LocalFlags, SetArg, tcgetattr, tcsetattr};

    let mut termios = tcgetattr(tty)?;
    let flags = LocalFlags::ISIG | LocalFlags::ICANON;
    if on {
        termios.local_flags.insert(flags);
    } else {
        termios.local_flags.remove(flags);
    }
    tcsetattr(tty, SetArg::TCSANOW, &termios)
}

/// Joins the thread that closes a widened readiness window.
///
/// Exists so the join happens on **every** path out of a test, including a panicking one — a
/// detached thread that outlives its `Terminal` is the hazard described on
/// [`Terminal::widen_the_readiness_window`].
#[cfg(unix)]
pub struct ClosingWindow(Option<std::thread::JoinHandle<()>>);

#[cfg(unix)]
impl Drop for ClosingWindow {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            // The result is discarded on purpose: this runs during unwinding when a test has
            // failed, and panicking while panicking aborts the process, which would replace a
            // named test failure with no diagnosis at all.
            let _ = handle.join();
        }
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
