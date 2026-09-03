//! Bounded retry: a validated policy, a pure jittered schedule, and a helper that never hides a
//! failure.
//!
//! # The schedule is a pure function, and that is the design
//!
//! Constitution IV: *"Retries MUST be bounded, observable, safe for the operation, and
//! documented."* Two of those are properties of a **number**, and the number is easier to get
//! wrong than the loop around it. So the delay is computed by [`RetryPolicy::delay`], which takes
//! the policy, the attempt that just failed, and bytes from the kernel's entropy port — and
//! nothing else. No clock, no sleep, no runtime, no random-number generator of its own. Given
//! [`crate::observe::FixedEntropy`] it is deterministic, so a test asserts the exact schedule
//! rather than a distribution, and a property test asserts `delay ≤ max_delay` for every input.
//!
//! `backon` was the best crate in the field and was not taken (ADR-0037): it seeds its own RNG
//! from a `u64`, which would give the kernel a second randomness site beside the entropy port —
//! the thing `run_id` and every opaque identifier in this project exist to avoid.
//!
//! # Retryability is closed
//!
//! Only a failure the caller classifies as [`RetryClass::Retryable`] is retried. A refusal, a
//! validation failure, or a denied credential is [`RetryClass::Terminal`] and costs exactly one
//! attempt, because retrying a refusal is how a bounded attack becomes unbounded work.
//!
//! # Every wait here is bounded
//!
//! Each attempt runs under [`RetryPolicy::attempt_timeout`] through `tokio::time::timeout`, the
//! sleeps are bounded by `max_delay`, the attempts by `max_attempts`, and an optional overall
//! deadline bounds the whole helper. C-L7 makes this a requirement rather than a courtesy:
//! `tests/deadlines.rs` discovers every kernel file that calls author code and requires a bounding
//! construct in it, and this file calls the author's operation.
//!
//! # Observable
//!
//! Every retry emits one structured event on the `renvor.retry` target — operation, attempt,
//! `max_attempts`, the delay as a number of milliseconds, and the closed reason — and increments
//! the counter the caller supplied, if any. The message is a constant; nothing about the failure
//! is interpolated, because the failure is the author's error type and may carry anything.

use core::fmt;
use core::future::Future;
use std::time::Duration;

use crate::observe::entropy::{EntropySource, EntropyUnavailable};
use crate::observe::metrics::Counter;

/// The most attempts a policy may ask for.
pub const MAX_ATTEMPTS_CAP: u32 = 100;
/// The longest delay a policy may ask for between attempts.
pub const MAX_DELAY_CAP: Duration = Duration::from_secs(60 * 60);
/// The longest one attempt may run.
pub const ATTEMPT_TIMEOUT_CAP: Duration = Duration::from_secs(60 * 60);
/// The shortest initial delay.
pub const MIN_INITIAL_DELAY: Duration = Duration::from_millis(1);
/// The multiplier range, inclusive.
pub const MULTIPLIER_RANGE: (f64, f64) = (1.0, 10.0);

/// The tracing target every retry event is emitted on.
pub const RETRY_EVENT_TARGET: &str = "renvor.retry";

/// How the delay is spread.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Jitter {
    /// The bare exponential value. Only for tests that want the closed form.
    None,
    /// Uniform in `[0, base]` — the "full jitter" form that best decorrelates a herd.
    Full,
    /// Uniform in `[base / 2, base]` — keeps at least half the wait.
    Equal,
}

/// Why a policy could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum PolicyError {
    /// `max_attempts` was 0 or above [`MAX_ATTEMPTS_CAP`].
    #[error("max_attempts must be between 1 and {MAX_ATTEMPTS_CAP}")]
    AttemptsOutOfRange,
    /// `initial_delay` was below [`MIN_INITIAL_DELAY`].
    #[error("initial_delay must be at least one millisecond")]
    InitialDelayTooShort,
    /// `max_delay` was above [`MAX_DELAY_CAP`] or below `initial_delay`.
    #[error("max_delay must be between initial_delay and one hour")]
    MaxDelayOutOfRange,
    /// `attempt_timeout` was zero or above [`ATTEMPT_TIMEOUT_CAP`].
    #[error("attempt_timeout must be between one millisecond and one hour")]
    AttemptTimeoutOutOfRange,
    /// The multiplier was outside [`MULTIPLIER_RANGE`] or not a finite number.
    #[error("multiplier must be a finite number between 1.0 and 10.0")]
    MultiplierOutOfRange,
    /// A zero deadline was supplied.
    #[error("deadline must be non-zero")]
    DeadlineZero,
}

/// A validated retry policy. **Cannot be unbounded**: every field is checked at construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetryPolicy {
    max_attempts: u32,
    initial_delay: Duration,
    max_delay: Duration,
    attempt_timeout: Duration,
    multiplier: f64,
    jitter: Jitter,
    deadline: Option<Duration>,
}

impl RetryPolicy {
    /// Builds a policy with multiplier `2.0`, [`Jitter::Full`], and no overall deadline.
    ///
    /// # Errors
    ///
    /// A [`PolicyError`] naming the first bound the arguments violate.
    pub fn new(
        max_attempts: u32,
        initial_delay: Duration,
        max_delay: Duration,
        attempt_timeout: Duration,
    ) -> Result<Self, PolicyError> {
        if max_attempts == 0 || max_attempts > MAX_ATTEMPTS_CAP {
            return Err(PolicyError::AttemptsOutOfRange);
        }
        if initial_delay < MIN_INITIAL_DELAY {
            return Err(PolicyError::InitialDelayTooShort);
        }
        if max_delay > MAX_DELAY_CAP || max_delay < initial_delay {
            return Err(PolicyError::MaxDelayOutOfRange);
        }
        if attempt_timeout < MIN_INITIAL_DELAY || attempt_timeout > ATTEMPT_TIMEOUT_CAP {
            return Err(PolicyError::AttemptTimeoutOutOfRange);
        }
        Ok(Self {
            max_attempts,
            initial_delay,
            max_delay,
            attempt_timeout,
            multiplier: 2.0,
            jitter: Jitter::Full,
            deadline: None,
        })
    }

    /// Replaces the multiplier.
    ///
    /// # Errors
    ///
    /// [`PolicyError::MultiplierOutOfRange`] outside [`MULTIPLIER_RANGE`] or for a non-finite value.
    pub fn with_multiplier(mut self, multiplier: f64) -> Result<Self, PolicyError> {
        if !multiplier.is_finite()
            || !(MULTIPLIER_RANGE.0..=MULTIPLIER_RANGE.1).contains(&multiplier)
        {
            return Err(PolicyError::MultiplierOutOfRange);
        }
        self.multiplier = multiplier;
        Ok(self)
    }

    /// Replaces the jitter.
    #[must_use]
    pub const fn with_jitter(mut self, jitter: Jitter) -> Self {
        self.jitter = jitter;
        self
    }

    /// Bounds the whole helper, attempts and sleeps together.
    ///
    /// # Errors
    ///
    /// [`PolicyError::DeadlineZero`] for a zero deadline.
    pub fn with_deadline(mut self, deadline: Duration) -> Result<Self, PolicyError> {
        if deadline.is_zero() {
            return Err(PolicyError::DeadlineZero);
        }
        self.deadline = Some(deadline);
        Ok(self)
    }

    /// The most attempts the helper will make.
    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// The delay before the second attempt, before jitter.
    #[must_use]
    pub const fn initial_delay(&self) -> Duration {
        self.initial_delay
    }

    /// The ceiling every delay is held under.
    #[must_use]
    pub const fn max_delay(&self) -> Duration {
        self.max_delay
    }

    /// How long one attempt may run.
    #[must_use]
    pub const fn attempt_timeout(&self) -> Duration {
        self.attempt_timeout
    }

    /// The growth factor.
    #[must_use]
    pub const fn multiplier(&self) -> f64 {
        self.multiplier
    }

    /// The jitter.
    #[must_use]
    pub const fn jitter(&self) -> Jitter {
        self.jitter
    }

    /// The overall deadline, if one was set.
    #[must_use]
    pub const fn deadline(&self) -> Option<Duration> {
        self.deadline
    }

    /// **The schedule.** The delay to wait after `failed_attempt` (1-based) failed, before the
    /// next attempt.
    ///
    /// A pure function of this policy, the attempt number, and the bytes `entropy` supplies.
    /// The result is **always** `≤ max_delay`, for every attempt number and every byte sequence —
    /// asserted by a property test — and `Jitter::None` yields the closed form
    /// `min(initial × multiplier^(attempt − 1), max_delay)`.
    ///
    /// # Errors
    ///
    /// [`EntropyUnavailable`] if the source cannot supply jitter bytes. Propagated, never
    /// defaulted: a schedule that silently lost its jitter would herd every retrier onto the same
    /// instant, which is the failure jitter exists to prevent.
    pub fn delay(
        &self,
        failed_attempt: u32,
        entropy: &dyn EntropySource,
    ) -> Result<Duration, EntropyUnavailable> {
        let exponent = i32::try_from(failed_attempt.saturating_sub(1)).unwrap_or(i32::MAX);
        // `powi` on a finite multiplier ≥ 1 is finite or +inf; `min` against the cap handles both,
        // and `from_secs_f64` is given a finite, non-negative value by construction.
        let grown = self.initial_delay.as_secs_f64() * self.multiplier.powi(exponent);
        let base_secs = grown.min(self.max_delay.as_secs_f64());
        let base = Duration::from_secs_f64(base_secs.max(0.0));

        let jittered = match self.jitter {
            Jitter::None => base,
            Jitter::Full => scale(base, fraction(entropy)?),
            Jitter::Equal => {
                let half = base / 2;
                half + scale(base - half, fraction(entropy)?)
            }
        };
        Ok(jittered.min(self.max_delay))
    }
}

/// Eight entropy bytes as a fraction in `[0, 1]`.
fn fraction(entropy: &dyn EntropySource) -> Result<f64, EntropyUnavailable> {
    let mut bytes = [0_u8; 8];
    entropy.fill(&mut bytes)?;
    // u64 → f64 loses low bits above 2^53, which is precision, not range: the result is still
    // in [0, 1] and still a function of the bytes only.
    Ok(u64::from_le_bytes(bytes) as f64 / u64::MAX as f64)
}

/// `duration × fraction`, saturating rather than panicking on an out-of-range float.
fn scale(duration: Duration, fraction: f64) -> Duration {
    Duration::try_from_secs_f64(duration.as_secs_f64() * fraction.clamp(0.0, 1.0))
        .unwrap_or(duration)
}

/// Whether a failure may be retried. The caller decides; the helper never guesses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RetryClass {
    /// Transient — an unavailable dependency, a timeout. Worth another attempt.
    Retryable,
    /// A refusal, a validation failure, a denied credential. Retrying it is work for nothing.
    Terminal,
}

impl RetryClass {
    /// A stable label for a metric or a structured field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Terminal => "terminal",
        }
    }
}

/// Why the helper stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StopReason {
    /// Every attempt the policy permits was made.
    AttemptsExhausted,
    /// The last failure was classified [`RetryClass::Terminal`].
    Terminal,
    /// The overall deadline elapsed before another attempt could start.
    DeadlineExceeded,
    /// The last attempt ran past [`RetryPolicy::attempt_timeout`].
    AttemptTimedOut,
    /// The entropy port could not supply jitter bytes.
    EntropyUnavailable,
}

impl StopReason {
    /// A stable label for a metric or a structured field.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AttemptsExhausted => "attempts_exhausted",
            Self::Terminal => "terminal",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::AttemptTimedOut => "attempt_timed_out",
            Self::EntropyUnavailable => "entropy_unavailable",
        }
    }
}

/// The helper gave up. Carries the **last** failure the operation produced, if it produced one.
///
/// `last` is `None` only when the stop was not caused by the operation — a deadline that elapsed
/// before the first attempt, or an entropy failure — so a caller can always tell whether the
/// operation ever ran.
#[derive(Debug)]
pub struct RetryError<E> {
    attempts: u32,
    reason: StopReason,
    last: Option<E>,
}

impl<E> RetryError<E> {
    /// How many attempts were made.
    #[must_use]
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Why the helper stopped.
    #[must_use]
    pub const fn reason(&self) -> StopReason {
        self.reason
    }

    /// The last failure the operation produced, if any.
    #[must_use]
    pub const fn last(&self) -> Option<&E> {
        self.last.as_ref()
    }

    /// Takes the last failure.
    #[must_use]
    pub fn into_last(self) -> Option<E> {
        self.last
    }
}

impl<E> fmt::Display for RetryError<E> {
    /// Names the reason and the count. **Not** the last error: it is the author's type and may
    /// carry anything.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "gave up after {} attempt(s): {}",
            self.attempts,
            self.reason.as_str()
        )
    }
}

impl<E: fmt::Debug> std::error::Error for RetryError<E> {}

/// Runs `operation` at most `policy.max_attempts()` times.
///
/// - Each attempt runs under `policy.attempt_timeout()`; a timed-out attempt counts as a
///   [`RetryClass::Retryable`] failure with no error value and stops the helper only when it was
///   the last attempt.
/// - Between attempts the helper sleeps for [`RetryPolicy::delay`], through `tokio::time`, so a
///   paused runtime controls it.
/// - `classify` decides retryability; a [`RetryClass::Terminal`] failure stops immediately.
/// - Every retry emits one event on [`RETRY_EVENT_TARGET`] and increments `retries`, if given,
///   with the closed reason as its `reason` label.
/// - The **last** failure is returned; nothing is swallowed and nothing is invented.
///
/// # Errors
///
/// [`RetryError`] with the stop reason, the attempt count, and the last failure.
pub async fn retry<T, E, F, Fut, C>(
    policy: &RetryPolicy,
    entropy: &dyn EntropySource,
    operation: &'static str,
    retries: Option<&Counter>,
    classify: C,
    mut run: F,
) -> Result<T, RetryError<E>>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    C: Fn(&E) -> RetryClass,
{
    let started = tokio::time::Instant::now();
    let deadline = policy.deadline().map(|budget| started + budget);
    let mut last: Option<E> = None;

    for attempt in 1..=policy.max_attempts() {
        if let Some(deadline) = deadline
            && tokio::time::Instant::now() >= deadline
        {
            return Err(RetryError {
                attempts: attempt - 1,
                reason: StopReason::DeadlineExceeded,
                last,
            });
        }

        let outcome = tokio::time::timeout(policy.attempt_timeout(), run(attempt)).await;
        let (class, reason_label) = match outcome {
            Ok(Ok(value)) => return Ok(value),
            Ok(Err(error)) => {
                let class = classify(&error);
                last = Some(error);
                (class, class.as_str())
            }
            Err(_elapsed) => (RetryClass::Retryable, StopReason::AttemptTimedOut.as_str()),
        };

        if class == RetryClass::Terminal {
            tracing::warn!(
                target: RETRY_EVENT_TARGET,
                operation,
                attempt,
                max_attempts = policy.max_attempts(),
                reason = reason_label,
                "operation failed and will not be retried"
            );
            return Err(RetryError {
                attempts: attempt,
                reason: StopReason::Terminal,
                last,
            });
        }
        if attempt == policy.max_attempts() {
            let reason = if last.is_none() {
                StopReason::AttemptTimedOut
            } else {
                StopReason::AttemptsExhausted
            };
            tracing::warn!(
                target: RETRY_EVENT_TARGET,
                operation,
                attempt,
                max_attempts = policy.max_attempts(),
                reason = reason.as_str(),
                "operation failed on its last attempt"
            );
            return Err(RetryError {
                attempts: attempt,
                reason,
                last,
            });
        }

        let delay = match policy.delay(attempt, entropy) {
            Ok(delay) => delay,
            Err(_) => {
                return Err(RetryError {
                    attempts: attempt,
                    reason: StopReason::EntropyUnavailable,
                    last,
                });
            }
        };
        // A sleep that would cross the deadline is not started; the deadline check at the top of
        // the next iteration reports it. Sleeping past a deadline to then report the deadline is
        // work the caller did not ask for.
        let delay = match deadline {
            Some(deadline) => {
                delay.min(deadline.saturating_duration_since(tokio::time::Instant::now()))
            }
            None => delay,
        };
        tracing::warn!(
            target: RETRY_EVENT_TARGET,
            operation,
            attempt,
            max_attempts = policy.max_attempts(),
            delay_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
            reason = reason_label,
            "operation failed; retrying after a delay"
        );
        if let Some(counter) = retries {
            counter.increment(&[("operation", operation), ("reason", reason_label)], 1);
        }
        tokio::time::sleep(delay).await;
    }

    // Unreachable: the loop returns on the last attempt. Written as an error rather than a panic
    // so a future edit that removes the last-attempt return degrades to a diagnostic (SC-004).
    Err(RetryError {
        attempts: policy.max_attempts(),
        reason: StopReason::AttemptsExhausted,
        last,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        ATTEMPT_TIMEOUT_CAP, Jitter, MAX_ATTEMPTS_CAP, MAX_DELAY_CAP, PolicyError, RetryClass,
        RetryPolicy, StopReason, retry,
    };
    use crate::observe::entropy::{EntropySource, EntropyUnavailable, FixedEntropy};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    fn policy(attempts: u32) -> RetryPolicy {
        RetryPolicy::new(
            attempts,
            Duration::from_millis(100),
            Duration::from_secs(5),
            Duration::from_secs(1),
        )
        .expect("valid")
    }

    #[test]
    fn every_bound_is_refused_by_name() {
        let ms = Duration::from_millis;
        assert_eq!(
            RetryPolicy::new(0, ms(1), ms(10), ms(10)).unwrap_err(),
            PolicyError::AttemptsOutOfRange
        );
        assert_eq!(
            RetryPolicy::new(MAX_ATTEMPTS_CAP + 1, ms(1), ms(10), ms(10)).unwrap_err(),
            PolicyError::AttemptsOutOfRange
        );
        assert_eq!(
            RetryPolicy::new(1, Duration::ZERO, ms(10), ms(10)).unwrap_err(),
            PolicyError::InitialDelayTooShort
        );
        assert_eq!(
            RetryPolicy::new(1, ms(20), ms(10), ms(10)).unwrap_err(),
            PolicyError::MaxDelayOutOfRange
        );
        assert_eq!(
            RetryPolicy::new(1, ms(1), MAX_DELAY_CAP + ms(1), ms(10)).unwrap_err(),
            PolicyError::MaxDelayOutOfRange
        );
        assert_eq!(
            RetryPolicy::new(1, ms(1), ms(10), ATTEMPT_TIMEOUT_CAP + ms(1)).unwrap_err(),
            PolicyError::AttemptTimeoutOutOfRange
        );
        assert_eq!(
            policy(3).with_multiplier(0.5).unwrap_err(),
            PolicyError::MultiplierOutOfRange
        );
        assert_eq!(
            policy(3).with_multiplier(f64::NAN).unwrap_err(),
            PolicyError::MultiplierOutOfRange
        );
        assert_eq!(
            policy(3).with_deadline(Duration::ZERO).unwrap_err(),
            PolicyError::DeadlineZero
        );
        // POSITIVE CONTROL: the boundary values themselves are accepted.
        assert!(
            RetryPolicy::new(MAX_ATTEMPTS_CAP, ms(1), MAX_DELAY_CAP, ATTEMPT_TIMEOUT_CAP).is_ok()
        );
        assert!(policy(3).with_multiplier(10.0).is_ok());
    }

    #[test]
    fn the_closed_form_schedule_grows_and_caps() {
        let policy = policy(10).with_jitter(Jitter::None);
        let entropy = FixedEntropy::new([0x00]);
        let delays: Vec<Duration> = (1..=8)
            .map(|attempt| policy.delay(attempt, &entropy).unwrap())
            .collect();
        assert_eq!(
            delays,
            vec![
                Duration::from_millis(100),
                Duration::from_millis(200),
                Duration::from_millis(400),
                Duration::from_millis(800),
                Duration::from_millis(1_600),
                Duration::from_millis(3_200),
                Duration::from_secs(5),
                Duration::from_secs(5),
            ]
        );
    }

    #[test]
    fn jitter_is_a_pure_function_of_the_entropy_bytes() {
        let policy = policy(5);
        let a = policy.delay(3, &FixedEntropy::new([0x80; 8])).unwrap();
        let b = policy.delay(3, &FixedEntropy::new([0x80; 8])).unwrap();
        assert_eq!(a, b, "the same bytes gave two different delays");
        // POSITIVE CONTROL: different bytes reach the output.
        let c = policy.delay(3, &FixedEntropy::new([0x01; 8])).unwrap();
        assert_ne!(a, c);
        // Full jitter with all-ones bytes is the whole base; with all-zero bytes it is zero.
        assert_eq!(
            policy.delay(3, &FixedEntropy::new([0xff; 8])).unwrap(),
            Duration::from_millis(400)
        );
        assert_eq!(
            policy.delay(3, &FixedEntropy::new([0x00; 8])).unwrap(),
            Duration::ZERO
        );
        // Equal jitter keeps at least half.
        let equal = policy.with_jitter(Jitter::Equal);
        assert_eq!(
            equal.delay(3, &FixedEntropy::new([0x00; 8])).unwrap(),
            Duration::from_millis(200)
        );
    }

    #[test]
    fn an_absurd_attempt_number_still_stays_under_the_cap() {
        // `powi` overflows to +inf long before u32::MAX; the cap must still hold.
        let policy = policy(5).with_jitter(Jitter::None);
        assert_eq!(
            policy.delay(u32::MAX, &FixedEntropy::new([0x00])).unwrap(),
            policy.max_delay()
        );
    }

    #[derive(Debug)]
    struct Failing;

    impl EntropySource for Failing {
        fn fill(&self, _: &mut [u8]) -> Result<(), EntropyUnavailable> {
            Err(EntropyUnavailable::new("no bytes"))
        }
    }

    #[test]
    fn a_failed_entropy_source_is_propagated_not_defaulted() {
        let policy = policy(5);
        assert!(policy.delay(2, &Failing).is_err());
        // POSITIVE CONTROL: `Jitter::None` needs no bytes and succeeds against the same source.
        assert!(policy.with_jitter(Jitter::None).delay(2, &Failing).is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn the_helper_makes_exactly_max_attempts_and_returns_the_last_error() {
        let calls = AtomicU32::new(0);
        let entropy = FixedEntropy::new([0x00]);
        let outcome: Result<(), _> = retry(
            &policy(3),
            &entropy,
            "always_fails",
            None,
            |_: &u32| RetryClass::Retryable,
            |attempt| {
                calls.fetch_add(1, Ordering::SeqCst);
                async move { Err::<(), u32>(attempt) }
            },
        )
        .await;
        let error = outcome.unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(error.attempts(), 3);
        assert_eq!(error.reason(), StopReason::AttemptsExhausted);
        assert_eq!(error.last(), Some(&3), "the LAST error, not the first");
    }

    #[tokio::test(start_paused = true)]
    async fn a_terminal_failure_costs_exactly_one_attempt() {
        let calls = AtomicU32::new(0);
        let entropy = FixedEntropy::new([0x00]);
        let outcome: Result<(), _> = retry(
            &policy(5),
            &entropy,
            "refused",
            None,
            |_: &&str| RetryClass::Terminal,
            |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err::<(), &str>("refused") }
            },
        )
        .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(outcome.unwrap_err().reason(), StopReason::Terminal);
    }

    #[tokio::test(start_paused = true)]
    async fn success_after_failures_returns_the_value_and_no_more_attempts() {
        let calls = AtomicU32::new(0);
        let entropy = FixedEntropy::new([0x00]);
        let value = retry(
            &policy(5),
            &entropy,
            "flaky",
            None,
            |_: &()| RetryClass::Retryable,
            |attempt| {
                calls.fetch_add(1, Ordering::SeqCst);
                async move { if attempt == 3 { Ok(42) } else { Err(()) } }
            },
        )
        .await
        .expect("succeeds on the third attempt");
        assert_eq!(value, 42);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn a_hanging_attempt_is_bounded_by_the_attempt_timeout() {
        // No real elapsed time: the paused clock auto-advances to the timeout.
        let entropy = FixedEntropy::new([0x00]);
        let outcome: Result<(), _> = retry(
            &policy(2),
            &entropy,
            "hangs",
            None,
            |_: &()| RetryClass::Retryable,
            |_| async { std::future::pending::<Result<(), ()>>().await },
        )
        .await;
        let error = outcome.unwrap_err();
        assert_eq!(error.attempts(), 2);
        assert_eq!(error.reason(), StopReason::AttemptTimedOut);
        assert!(error.last().is_none(), "a timeout produces no error value");
    }

    #[tokio::test(start_paused = true)]
    async fn the_deadline_stops_the_helper_before_the_attempts_run_out() {
        let calls = AtomicU32::new(0);
        let entropy = FixedEntropy::new([0xff; 8]); // full delays
        let policy = policy(100)
            .with_deadline(Duration::from_millis(350))
            .expect("valid");
        let outcome: Result<(), _> = retry(
            &policy,
            &entropy,
            "slow",
            None,
            |_: &()| RetryClass::Retryable,
            |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                async { Err::<(), ()>(()) }
            },
        )
        .await;
        let error = outcome.unwrap_err();
        assert_eq!(error.reason(), StopReason::DeadlineExceeded);
        // 100 ms + 200 ms of delays fits under 350 ms; the third delay would cross it.
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert!(
            error.attempts() < 100,
            "the deadline did not bound anything"
        );
    }

    #[test]
    fn the_display_form_names_the_reason_and_never_the_error() {
        let error = super::RetryError {
            attempts: 2,
            reason: StopReason::Terminal,
            last: Some("hunter2CanaryDoNotLeak"),
        };
        let rendered = error.to_string();
        assert!(
            rendered.contains("terminal"),
            "Display did not name the stop reason"
        );
        assert!(
            !rendered.contains("hunter2"),
            "Display rendered the author's error value"
        );
    }
}
