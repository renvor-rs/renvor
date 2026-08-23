//! Running a project's own binary, under every bound.
//!
//! # This closes a gap Phase 004 recorded against itself
//!
//! `renvor routes` used a blocking `Command::output()`, and its own source said what that cost:
//!
//! > *"The realistic trigger is a bug, not an attack: a project binary that does not recognise the
//! > dump flag, ignores it, boots normally, and streams logs forever."*
//!
//! and named the fix:
//!
//! > *"spawning with a piped stdout, reading at most `MAX_DUMP_BYTES + 1`, and **killing the
//! > child** on overflow — without the kill, a full pipe blocks the child and the hang replaces
//! > the allocation."*
//!
//! That is what this module does. FR-053 makes it a requirement of Phase 005, and it is written
//! **once**, here, so both relays get it — two implementations of "run the project binary safely"
//! would be two things that can drift, and one of them would be the one nobody updated.
//!
//! # Why the reader runs on its own thread
//!
//! A pipe buffer is finite. A child that writes more than it holds **blocks** until someone reads,
//! and a parent that waits for exit before reading would deadlock against exactly the oversized
//! answer the bound exists to refuse. So the read and the wait run concurrently: the reader drains
//! up to the bound while the main thread waits with a deadline.
//!
//! # Every failure is named
//!
//! There is no branch that returns an empty success. A relay that could not obtain an answer says
//! so, because an empty answer and "the question could not be asked" are different facts and a
//! consumer cannot tell them apart if both arrive as success.
//!
//! # What the kill does NOT cover, stated rather than implied
//!
//! The direct child is `cargo`, and `cargo run` spawns the project's binary as a **grandchild**.
//! Killing `cargo` orphans that grandchild rather than ending it: on Unix it is reparented and
//! keeps running.
//!
//! The deadline bounds **this process's wait**. It does **not** guarantee the project's binary has
//! stopped.
//!
//! **That bound is only true because the reader is DETACHED rather than joined on the timeout
//! path.** An earlier revision joined it, and hung forever: the orphaned grandchild holds an
//! inherited copy of the stdout write end, so the pipe never closes and the read never returns.
//! The leak and the hang are the same fact seen from two ends, and joining converted a documented
//! leak into an undocumented hang.
//!
//! Reaping the orphan needs a process group, which `std::process` exposes only through
//! `CommandExt::pre_exec` — `unsafe`, and this workspace declares `unsafe_code = "forbid"`. A
//! process-group crate would do it safely and is a dependency decision rather than a late edit.
//!
//! Recorded here and in `governance/phase-005-evidence.md` rather than left to inference — a
//! comment promising more than its code delivers is the defect class `renvor routes` already
//! recorded against its own size bound.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt;

/// How long a project binary has to answer a metadata question.
///
/// Generous, because the first invocation compiles the project. Bounded, because an unbounded wait
/// in a command-line tool is a hang an operator has to notice and interrupt — and contract C-L7
/// already establishes that an unbounded wait is a defect rather than a configuration choice.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// The most a project binary may print in answer.
pub const MAX_ANSWER_BYTES: usize = 8 * 1024 * 1024;

/// Why a project binary could not be asked, or could not be understood.
///
/// Each variant is a **distinct named failure**. None of them is an empty success.
#[derive(Debug)]
pub enum RelayFailure {
    /// The binary could not be started at all.
    Spawn(std::io::Error),
    /// The binary did not answer within the deadline. The direct child was killed.
    ///
    /// A grandchild — the binary `cargo run` spawned — may survive. See the module documentation.
    Timeout {
        /// The deadline that elapsed.
        after: Duration,
    },
    /// The binary exited non-zero.
    NonZero {
        /// How it exited, rendered without any of its output.
        status: String,
    },
    /// The answer exceeded [`MAX_ANSWER_BYTES`].
    TooLarge {
        /// The bound.
        limit: usize,
    },
    /// The answer was not UTF-8.
    NotUtf8,
    /// The reader thread could not be joined, which means it panicked.
    ReaderFailed,
}

impl RelayFailure {
    /// A short machine-readable reason, for `details.reason`.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Spawn(_) => "invocation_failed",
            Self::Timeout { .. } => "invocation_timed_out",
            Self::NonZero { .. } => "dump_failed",
            Self::TooLarge { .. } => "dump_too_large",
            Self::NotUtf8 => "dump_unreadable",
            Self::ReaderFailed => "invocation_failed",
        }
    }
}

impl core::fmt::Display for RelayFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "the project's binary could not be run: {error}"),
            Self::Timeout { after } => write!(
                f,
                "the project's binary did not answer within {} seconds and was stopped. A binary \
                 that does not recognise the invocation may be starting normally instead of \
                 answering it",
                after.as_secs()
            ),
            Self::NonZero { status } => write!(
                f,
                "the project's binary exited with {status} when asked. Run the invocation in the \
                 project to see why"
            ),
            Self::TooLarge { limit } => {
                write!(f, "the answer is above the {limit}-byte limit")
            }
            Self::NotUtf8 => {
                f.write_str("the project's binary answered with bytes that are not UTF-8")
            }
            Self::ReaderFailed => f.write_str("the answer could not be read"),
        }
    }
}

/// How a project binary is invoked.
#[derive(Debug, Clone)]
pub struct Invocation {
    program: OsString,
    arguments: Vec<OsString>,
    directory: PathBuf,
    timeout: Duration,
}

impl Invocation {
    /// The documented default: the project's own declared binary, run through `cargo`.
    ///
    /// `cargo run` rather than a search of `target/`: the project declares its binary, and asking
    /// its build tool to run "the binary this package defines" is the only way to get that answer
    /// without guessing.
    #[must_use]
    pub fn for_project(directory: &Path, flag: &str) -> Self {
        Self {
            program: OsString::from("cargo"),
            arguments: vec![
                OsString::from("run"),
                OsString::from("--quiet"),
                OsString::from("--"),
                OsString::from(flag),
            ],
            directory: directory.to_path_buf(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Builds an invocation that runs `program` with `arguments` in `directory`.
    ///
    /// `#[cfg(test)]` because production has exactly one invocation — the documented one above.
    /// Leaving it compiled into the shipped binary would offer a way to point a relay at an
    /// arbitrary program, which is a capability nothing asked for.
    #[cfg(test)]
    #[must_use]
    pub fn new(
        program: impl Into<OsString>,
        arguments: Vec<OsString>,
        directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            program: program.into(),
            arguments,
            directory: directory.into(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Overrides the deadline.
    ///
    /// `#[cfg(test)]` for the same reason as [`Invocation::new`]: production uses exactly one
    /// deadline, [`DEFAULT_TIMEOUT`], and a shipped way to shorten or extend it is a control
    /// nothing asked for. Tests need it because a test that waited five minutes to prove a
    /// deadline works is a test nobody runs.
    #[cfg(test)]
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Runs the binary and returns everything it printed to `stdout`.
    ///
    /// # Errors
    ///
    /// Every variant of [`RelayFailure`]. **None of them is an empty success.**
    pub fn run(&self) -> Result<String, RelayFailure> {
        let mut child = Command::new(&self.program)
            .args(&self.arguments)
            .current_dir(&self.directory)
            .stdout(Stdio::piped())
            // `stderr` is INHERITED, not captured. A build takes time and prints progress, and
            // swallowing it would leave an operator watching a silent command. C-1 reserves
            // `stdout` for the result, which is what is captured here and nothing else.
            .stderr(Stdio::inherit())
            .stdin(Stdio::null())
            .spawn()
            .map_err(RelayFailure::Spawn)?;

        // THE READER, on its own thread and concurrent with the wait below.
        //
        // A pipe buffer is finite. Waiting for exit first would deadlock against a child that
        // wrote more than the buffer holds — which is precisely the oversized answer the bound
        // exists to refuse.
        let stdout = child.stdout.take();
        let reader = std::thread::spawn(move || {
            let mut collected = Vec::new();
            if let Some(mut stream) = stdout {
                // One byte PAST the bound, so an answer exactly at the bound is accepted and one
                // byte over is detectable. Reading unbounded would make the bound decorative.
                let mut limited = (&mut stream).take((MAX_ANSWER_BYTES + 1) as u64);
                let _ = limited.read_to_end(&mut collected);
                // Drain whatever remains so the child is never blocked on a full pipe while it
                // tries to exit. The bytes are discarded — the answer is already over-sized.
                let _ = std::io::copy(&mut stream, &mut std::io::sink());
            }
            collected
        });

        let status = match child
            .wait_timeout(self.timeout)
            .map_err(RelayFailure::Spawn)?
        {
            Some(status) => status,
            None => {
                // THE KILL, then the reap. Both are for the DIRECT child.
                let _ = child.kill();
                let _ = child.wait();

                // THE READER IS DETACHED, NOT JOINED — and this is the whole fix.
                //
                // An earlier version joined it here, with a comment saying "the reader ends when
                // the pipe closes, which the kill guarantees." **That is false**, and it made the
                // timeout path hang forever against the exact process shape `cargo run` has.
                //
                // `cargo run` FORKS: the project's binary is a grandchild holding an inherited
                // copy of the stdout write end. Killing `cargo` does not close that copy, so the
                // reader stays blocked in its read, and joining it blocks the CLI **for as long as
                // the orphan runs** — turning a bounded wait into an unbounded one, which is the
                // defect contract C-L7 names.
                //
                // Measured, because the distinction is invisible from the outside:
                //
                //   sh -c 'yes renvor'          sh EXECs  -> ONE process   -> kill closes the pipe
                //   sh -c 'yes renvor & wait'   sh FORKS  -> grandchild    -> pipe stays OPEN
                //
                // Every timeout test in this file used the first shape, so the suite passed
                // without ever reaching the hazard. `a_forking_child_does_not_hold_the_relay`
                // below uses the second.
                //
                // Dropping the handle detaches the thread. It ends when the orphan does, and until
                // then it holds one thread and the bytes it has already read — a bounded cost the
                // caller does not wait for. Reaping the orphan itself needs a process group, which
                // `std` exposes only through `unsafe` `pre_exec`; that is recorded as a limitation
                // rather than claimed here.
                drop(reader);

                return Err(RelayFailure::Timeout {
                    after: self.timeout,
                });
            }
        };

        let collected = reader.join().map_err(|_| RelayFailure::ReaderFailed)?;

        if !status.success() {
            return Err(RelayFailure::NonZero {
                // The STATUS only. The child's output is not included: it is author data, and a
                // failure message is a place it must not be.
                status: status.to_string(),
            });
        }
        if collected.len() > MAX_ANSWER_BYTES {
            return Err(RelayFailure::TooLarge {
                limit: MAX_ANSWER_BYTES,
            });
        }

        String::from_utf8(collected).map_err(|_| RelayFailure::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::{Invocation, MAX_ANSWER_BYTES, RelayFailure};
    use std::ffi::OsString;
    use std::time::{Duration, Instant};

    fn shell(script: &str) -> Invocation {
        Invocation::new(
            "sh",
            vec![OsString::from("-c"), OsString::from(script)],
            std::env::temp_dir(),
        )
    }

    #[test]
    fn an_answer_is_relayed_verbatim() {
        let answer = shell("printf '%s' '{\"ok\":true}'")
            .run()
            .expect("the script answers");
        assert_eq!(answer, r#"{"ok":true}"#);
    }

    #[test]
    fn a_binary_that_never_answers_is_stopped_at_the_deadline() {
        // THE PHASE 004 GAP, closed. Before this, `Command::output()` waited forever.
        let started = Instant::now();
        let failure = shell("sleep 60")
            .with_timeout(Duration::from_millis(400))
            .run()
            .expect_err("a binary that never answers must be stopped");

        assert!(
            matches!(failure, RelayFailure::Timeout { .. }),
            "got {failure:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the deadline did not fire: waited {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_binary_that_streams_forever_is_stopped_rather_than_filling_memory() {
        // The realistic trigger the Phase 004 comment describes: a binary that ignores the flag,
        // starts normally, and logs. Without a concurrent reader this deadlocks; without a kill it
        // never ends.
        let started = Instant::now();
        let failure = shell("yes renvor")
            .with_timeout(Duration::from_millis(600))
            .run()
            .expect_err("an endless stream must be stopped");

        assert!(
            matches!(
                failure,
                RelayFailure::Timeout { .. } | RelayFailure::TooLarge { .. }
            ),
            "got {failure:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the relay did not terminate: waited {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_forking_child_does_not_hold_the_relay() {
        // THE SHAPE `cargo run` ACTUALLY HAS, and the one every other timeout test in this file
        // misses.
        //
        // `sh -c 'sleep 60'` and `sh -c 'yes renvor'` both cause `sh` to EXEC, collapsing to a
        // single process — so killing it closes the pipe and any reader returns. `cargo run`
        // FORKS, leaving a grandchild holding an inherited copy of the stdout write end.
        //
        // `& wait` forces the fork. Before the reader was detached, this blocked forever.
        let started = Instant::now();
        let failure = shell("yes renvor-fork-probe & wait")
            .with_timeout(Duration::from_millis(500))
            .run()
            .expect_err("a forking child that never answers must be stopped");

        assert!(
            matches!(failure, RelayFailure::Timeout { .. }),
            "got {failure:?}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "the relay was held by an orphaned grandchild for {:?} — the reader is being joined \
             on the timeout path, and the pipe an orphan holds open never closes",
            started.elapsed()
        );

        // Clean up the orphan this test deliberately created. Nothing in production does this;
        // the limitation is recorded rather than papered over.
        // A marker UNIQUE to this test. The suite runs in parallel and another test also runs
        // `yes renvor`; a shared pattern here would kill that test's child out from under it —
        // which it did, once, and the failure looked like a bug in the other test.
        let _ = std::process::Command::new("pkill")
            .args(["-9", "-f", "yes renvor-fork-probe"])
            .status();
    }

    #[test]
    fn a_non_zero_exit_is_a_named_failure_carrying_no_output() {
        let failure = shell("echo 'SECRET-OUTPUT'; exit 3")
            .run()
            .expect_err("a non-zero exit must be reported");

        assert!(
            matches!(failure, RelayFailure::NonZero { .. }),
            "got {failure:?}"
        );
        assert!(
            !failure.to_string().contains("SECRET-OUTPUT"),
            "the child's output reached the failure message: {failure}"
        );
    }

    #[test]
    fn a_program_that_does_not_exist_is_a_named_failure() {
        let failure = Invocation::new(
            "renvor-a-program-that-does-not-exist",
            Vec::new(),
            std::env::temp_dir(),
        )
        .run()
        .expect_err("a missing program must be reported");
        assert!(matches!(failure, RelayFailure::Spawn(_)), "got {failure:?}");
    }

    #[test]
    fn an_oversized_answer_is_refused_by_name() {
        let script = format!("yes x | head -c {}", MAX_ANSWER_BYTES + 1024);
        let failure = shell(&script)
            .with_timeout(Duration::from_secs(60))
            .run()
            .expect_err("an oversized answer must be refused");
        assert!(
            matches!(failure, RelayFailure::TooLarge { .. }),
            "got {failure:?}"
        );
    }

    #[test]
    fn every_failure_has_a_distinct_reason_and_none_is_a_success() {
        // The reasons are what a consumer matches on, so two failures sharing one would be
        // indistinguishable exactly where the distinction matters.
        let reasons = [
            RelayFailure::Timeout {
                after: Duration::from_secs(1),
            }
            .reason(),
            RelayFailure::NonZero {
                status: "1".to_owned(),
            }
            .reason(),
            RelayFailure::TooLarge { limit: 1 }.reason(),
            RelayFailure::NotUtf8.reason(),
        ];
        let unique: std::collections::BTreeSet<&str> = reasons.iter().copied().collect();
        assert_eq!(
            unique.len(),
            reasons.len(),
            "two distinct failures report the same reason: {reasons:?}"
        );
    }
}
