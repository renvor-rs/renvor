//! Deterministic time: exercising deadlines without waiting for them.
//!
//! # Why this exists as a named thing rather than a note in a README
//!
//! FR-031 requires deadline and drain behaviour to be exercisable **without real elapsed time**.
//! `tokio` already provides that through `pause`/`advance`, so the value here is not the
//! mechanism — it is making the requirement discoverable and hard to get subtly wrong:
//!
//! - `tokio::time::pause` **panics both ways**: outside a `current_thread` runtime, *and* when the
//!   clock is already paused — the second with the message `time is already frozen`, which says
//!   nothing about what to do. Since `#[tokio::test(start_paused = true)]` is the idiom used
//!   throughout this workspace, a clock helper that called `pause` itself would panic in the
//!   common case. **Measured, not assumed**: the first version of this module did exactly that and
//!   its own tests failed.
//! - So [`TestClock::new`] **does not pause**. It attaches to a runtime that is already paused,
//!   and [`TestClock::pausing`] exists for the other case.
//! - A paused runtime **auto-advances** when nothing is runnable, so most tests need no explicit
//!   advance at all. Tests that add one anyway are the ones that later look flaky.
//!
//! # This is not a wall clock, and deliberately provides no way to read one
//!
//! There is no `now()` here. FR-039(b) and SC-021 require the resolver's bounds to be asserted
//! against **counted work, never elapsed time**, and a clock that could be read is an invitation to
//! write the assertion the requirement prohibits.

use std::time::Duration;

/// Controls a paused runtime's clock.
///
/// Requires a `current_thread` runtime with time paused — `#[tokio::test(start_paused = true)]` is
/// the usual way in.
#[derive(Debug, Default)]
pub struct TestClock {
    advanced: Duration,
}

impl TestClock {
    /// Attaches to a runtime whose clock is **already paused**.
    ///
    /// Use with `#[tokio::test(start_paused = true)]`, which is the idiom throughout this
    /// workspace. This deliberately does **not** call `tokio::time::pause` — doing so on an
    /// already-paused runtime panics with `time is already frozen`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pauses a runtime whose clock is still running, then attaches to it.
    ///
    /// # Panics
    ///
    /// Panics if the current runtime is not a `current_thread` runtime, or if its clock is
    /// **already** paused — in which case [`Self::new`] is the constructor you want.
    #[must_use]
    pub fn pausing() -> Self {
        tokio::time::pause();
        Self::default()
    }

    /// Moves the clock forward by `duration`, costing **0** real time.
    pub async fn advance(&mut self, duration: Duration) {
        tokio::time::advance(duration).await;
        self.advanced = self.advanced.saturating_add(duration);
    }

    /// How far this clock has been advanced in total.
    ///
    /// The elapsed **virtual** time, which is a fact about the test rather than about the machine
    /// — so an assertion on it is reproducible where an assertion on real elapsed time is not.
    #[must_use]
    pub const fn advanced(&self) -> Duration {
        self.advanced
    }
}

#[cfg(test)]
mod tests {
    use super::TestClock;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn advancing_costs_no_real_time() {
        // A full hour of virtual time. If this took an hour, the requirement would be unmet and
        // the suite would be unrunnable — which is the point of asserting it rather than assuming.
        let real_start = std::time::Instant::now();

        let mut clock = TestClock::new();
        clock.advance(Duration::from_secs(3600)).await;

        assert_eq!(clock.advanced(), Duration::from_secs(3600));
        assert!(
            real_start.elapsed() < Duration::from_secs(1),
            "an hour of virtual time cost {:?} of real time",
            real_start.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_sleep_completes_when_the_clock_passes_it() {
        // POSITIVE CONTROL: the clock actually drives timers, rather than merely counting.
        let mut clock = TestClock::new();
        let sleeper = tokio::spawn(async { tokio::time::sleep(Duration::from_secs(30)).await });

        assert!(!sleeper.is_finished(), "still waiting before the advance");
        clock.advance(Duration::from_secs(31)).await;
        sleeper.await.expect("the sleeper woke");
    }
}
