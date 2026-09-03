//! The job vocabulary: identifiers, bounded names and payloads, states, and outcomes.
//!
//! # Every bound is enforced at construction
//!
//! A [`QueueName`] or [`JobKind`] that exists matches the grammar; a [`JobPayload`] that exists
//! is under the configured ceiling; a [`NewJob`] that exists has attempts in range. The store and
//! the worker therefore never receive an input they must check, which is what lets the four
//! database rows and the memory substitute share one contract without each re-implementing the
//! rules (FR-025, FR-029).
//!
//! # What `Debug` shows, and what it never shows
//!
//! A payload is the application's data and may hold anything. [`JobPayload`]'s `Debug` prints
//! its **length**; it has no `Display` and no `Serialize`. [`Job`]'s hand-written `Debug` prints
//! identity, queue, kind, state, and attempt — never the payload (FR-037). A [`LeaseToken`] is a
//! capability: holding it is the authority to complete or fail the job, so its `Debug` prints
//! nothing but its width.
//!
//! # Identifiers come from the entropy port
//!
//! [`JobId::generate`] and [`LeaseToken::generate`] take the kernel's `EntropySource` and nothing
//! else — no sequence, because a sequence encodes throughput, and no clock, because a clock
//! encodes when (FR-042, the run-identifier reasoning).

use core::fmt;
use std::time::{Duration, SystemTime};

use renvor_core::observe::entropy::{EntropySource, EntropyUnavailable};
use renvor_core::observe::trace_context::TraceContext;

/// The most bytes a queue name or job kind may carry.
pub const MAX_IDENTIFIER_BYTES: usize = 64;
/// The most bytes an idempotency key may carry.
pub const MAX_IDEMPOTENCY_KEY_BYTES: usize = 128;
/// The default ceiling on a payload.
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 64 * 1024;
/// The hard cap on the configurable payload ceiling.
pub const MAX_PAYLOAD_BYTES_CAP: usize = 1024 * 1024;
/// The default `max_attempts` for a job that does not say.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 5;
/// The hard cap on `max_attempts`.
pub const MAX_ATTEMPTS_CAP: u32 = 100;
/// The default lease a claim holds.
pub const DEFAULT_LEASE: Duration = Duration::from_secs(60);
/// The hard cap on a lease.
pub const MAX_LEASE_CAP: Duration = Duration::from_secs(60 * 60);
/// The default per-job handler timeout.
pub const DEFAULT_HANDLER_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// The hard cap on the handler timeout.
pub const MAX_HANDLER_TIMEOUT_CAP: Duration = Duration::from_secs(24 * 60 * 60);
/// The default per-queue depth bound (ready or leased jobs).
pub const DEFAULT_MAX_QUEUE_DEPTH: u64 = 100_000;
/// The most jobs one expired-lease sweep reclaims.
pub const RECLAIM_BATCH: u32 = 100;

/// Why an input was refused before any store was touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum JobRefusal {
    /// The queue name was empty, over 64 bytes, or not `[a-z0-9_.-]`.
    QueueNameInvalid,
    /// The kind was empty, over 64 bytes, or not `[a-z0-9_.-]`.
    KindInvalid,
    /// The idempotency key was empty, over 128 bytes, or held a control character.
    IdempotencyKeyInvalid,
    /// The payload exceeded the configured ceiling.
    PayloadTooLarge,
    /// `max_attempts` was 0 or above [`MAX_ATTEMPTS_CAP`].
    AttemptsOutOfRange,
    /// A configured bound exceeded its cap or fell below its floor.
    BoundOutOfRange,
}

impl JobRefusal {
    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::QueueNameInvalid => "queue_name_invalid",
            Self::KindInvalid => "kind_invalid",
            Self::IdempotencyKeyInvalid => "idempotency_key_invalid",
            Self::PayloadTooLarge => "payload_too_large",
            Self::AttemptsOutOfRange => "attempts_out_of_range",
            Self::BoundOutOfRange => "bound_out_of_range",
        }
    }
}

/// Why a store operation failed. **Closed; no variant carries text.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum JobError {
    /// The store did not answer, refused the connection, or failed the statement.
    #[error("the job store is unavailable")]
    Unavailable,
    /// The operation ran past its bound.
    #[error("the job store operation timed out")]
    TimedOut,
    /// An input was refused before any I/O.
    #[error("the job store refused an input: {}", .0.as_str())]
    Refused(JobRefusal),
    /// The queue's depth bound was reached.
    #[error("the queue is full")]
    QueueFull,
    /// The lease token names no job this caller holds: released, reclaimed, or never issued.
    #[error("the lease is not held")]
    LeaseNotHeld,
    /// The identifier names no job.
    #[error("no such job")]
    NotFound,
    /// The entropy port could not supply an identifier.
    #[error("entropy is unavailable")]
    EntropyUnavailable,
}

impl JobError {
    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::TimedOut => "timed_out",
            Self::Refused(_) => "refused",
            Self::QueueFull => "queue_full",
            Self::LeaseNotHeld => "lease_not_held",
            Self::NotFound => "not_found",
            Self::EntropyUnavailable => "entropy_unavailable",
        }
    }
}

impl From<EntropyUnavailable> for JobError {
    fn from(_: EntropyUnavailable) -> Self {
        Self::EntropyUnavailable
    }
}

/// The bounds an application configured for its job store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JobBounds {
    max_payload_bytes: usize,
    max_queue_depth: u64,
}

impl Default for JobBounds {
    fn default() -> Self {
        Self {
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_queue_depth: DEFAULT_MAX_QUEUE_DEPTH,
        }
    }
}

impl JobBounds {
    /// The documented defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the payload ceiling. Refused at zero or above [`MAX_PAYLOAD_BYTES_CAP`].
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::BoundOutOfRange`].
    pub fn with_max_payload_bytes(mut self, bytes: usize) -> Result<Self, JobError> {
        if bytes == 0 || bytes > MAX_PAYLOAD_BYTES_CAP {
            return Err(JobError::Refused(JobRefusal::BoundOutOfRange));
        }
        self.max_payload_bytes = bytes;
        Ok(self)
    }

    /// Replaces the per-queue depth bound. Refused at zero.
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::BoundOutOfRange`].
    pub fn with_max_queue_depth(mut self, depth: u64) -> Result<Self, JobError> {
        if depth == 0 {
            return Err(JobError::Refused(JobRefusal::BoundOutOfRange));
        }
        self.max_queue_depth = depth;
        Ok(self)
    }

    /// The payload ceiling.
    #[must_use]
    pub const fn max_payload_bytes(&self) -> usize {
        self.max_payload_bytes
    }

    /// The per-queue depth bound.
    #[must_use]
    pub const fn max_queue_depth(&self) -> u64 {
        self.max_queue_depth
    }
}

/// `[a-z0-9_.-]{1,64}`.
fn valid_identifier(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_IDENTIFIER_BYTES
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.' | b'-')
        })
}

/// A validated queue name.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct QueueName(String);

impl QueueName {
    /// Validates a queue name against `[a-z0-9_.-]{1,64}`.
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::QueueNameInvalid`].
    pub fn new(name: &str) -> Result<Self, JobError> {
        if !valid_identifier(name) {
            return Err(JobError::Refused(JobRefusal::QueueNameInvalid));
        }
        Ok(Self(name.to_owned()))
    }

    /// The name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for QueueName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for QueueName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "QueueName({})", self.0)
    }
}

/// A validated job kind: the handler selector.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobKind(String);

impl JobKind {
    /// Validates a kind against `[a-z0-9_.-]{1,64}`.
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::KindInvalid`].
    pub fn new(kind: &str) -> Result<Self, JobError> {
        if !valid_identifier(kind) {
            return Err(JobError::Refused(JobRefusal::KindInvalid));
        }
        Ok(Self(kind.to_owned()))
    }

    /// The kind.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for JobKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JobKind({})", self.0)
    }
}

/// A validated idempotency key: 1–128 bytes, no control characters.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// Validates a key.
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::IdempotencyKeyInvalid`].
    pub fn new(key: &str) -> Result<Self, JobError> {
        let bytes = key.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= MAX_IDEMPOTENCY_KEY_BYTES
            && bytes.iter().all(|byte| *byte >= 0x20 && *byte != 0x7f);
        if !valid {
            return Err(JobError::Refused(JobRefusal::IdempotencyKeyInvalid));
        }
        Ok(Self(key.to_owned()))
    }

    /// The key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Renders the **length**: a key is routinely an order number or an account-derived value.
impl fmt::Debug for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdempotencyKey")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// A validated payload: at most the configured ceiling.
#[derive(Clone, PartialEq, Eq)]
pub struct JobPayload(Vec<u8>);

impl JobPayload {
    /// Validates a payload against `bounds`.
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::PayloadTooLarge`].
    pub fn within(bytes: impl Into<Vec<u8>>, bounds: &JobBounds) -> Result<Self, JobError> {
        let bytes = bytes.into();
        if bytes.len() > bounds.max_payload_bytes() {
            return Err(JobError::Refused(JobRefusal::PayloadTooLarge));
        }
        Ok(Self(bytes))
    }

    /// Rebuilds a payload from bytes a store already holds, re-checked against `bounds`.
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::PayloadTooLarge`] — a row written under a larger
    /// bound is refused rather than handed to a process whose bound is smaller.
    pub fn from_stored(bytes: Vec<u8>, bounds: &JobBounds) -> Result<Self, JobError> {
        Self::within(bytes, bounds)
    }

    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// How many bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the payload is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Renders the **length**, never the bytes (FR-037).
impl fmt::Debug for JobPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobPayload")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Sixteen entropy bytes, rendered as lowercase hex.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId([u8; 16]);

impl JobId {
    /// **The single generation site.** Sixteen bytes from `source` and nothing else.
    ///
    /// # Errors
    ///
    /// [`EntropyUnavailable`], propagated rather than defaulted.
    pub fn generate(source: &dyn EntropySource) -> Result<Self, EntropyUnavailable> {
        let mut bytes = [0_u8; 16];
        source.fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// Rebuilds an identifier a store already holds.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// The 32 lowercase hex characters.
    #[must_use]
    pub fn encode(&self) -> String {
        hex(&self.0)
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.encode())
    }
}

impl fmt::Debug for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "JobId({})", self.encode())
    }
}

/// The authority to complete, fail, or release one claimed job.
///
/// Sixteen entropy bytes. Compared exactly, held by the worker that claimed the job, validated
/// by the store on every transition (FR-039).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct LeaseToken([u8; 16]);

impl LeaseToken {
    /// Generates a token from `source` and nothing else.
    ///
    /// # Errors
    ///
    /// [`EntropyUnavailable`].
    pub fn generate(source: &dyn EntropySource) -> Result<Self, EntropyUnavailable> {
        let mut bytes = [0_u8; 16];
        source.fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// Rebuilds a token a store already holds.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Renders nothing but the width: a lease token is a capability.
impl fmt::Debug for LeaseToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LeaseToken(16 bytes)")
    }
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[usize::from(byte >> 4)] as char);
        out.push(DIGITS[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// Where a job is in its life. A closed set; the numbers are what the rows store.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum JobState {
    /// Waiting to be claimed once `run_at` arrives.
    Ready,
    /// Claimed and under a lease.
    Leased,
    /// Finished successfully.
    Completed,
    /// Attempts exhausted, or abandoned by its handler. Never claimed again on its own.
    Dead,
}

impl JobState {
    /// The stored code.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Ready => 0,
            Self::Leased => 1,
            Self::Completed => 2,
            Self::Dead => 3,
        }
    }

    /// The state for a stored code, or `None` for a code this version does not know.
    #[must_use]
    pub const fn from_u8(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Ready),
            1 => Some(Self::Leased),
            2 => Some(Self::Completed),
            3 => Some(Self::Dead),
            _ => None,
        }
    }

    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Leased => "leased",
            Self::Completed => "completed",
            Self::Dead => "dead",
        }
    }
}

/// Why an attempt failed. A closed set with no field that could carry the handler's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FailureKind {
    /// The handler returned a retryable failure.
    HandlerFailed,
    /// The handler ran past its timeout.
    TimedOut,
    /// The handler panicked.
    Panicked,
    /// The lease expired without a transition, so the job was reclaimed.
    LeaseExpired,
    /// The handler abandoned the job: a terminal failure that must not be retried.
    Abandoned,
}

impl FailureKind {
    /// The stored code.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::HandlerFailed => 1,
            Self::TimedOut => 2,
            Self::Panicked => 3,
            Self::LeaseExpired => 4,
            Self::Abandoned => 5,
        }
    }

    /// The kind for a stored code.
    #[must_use]
    pub const fn from_u8(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::HandlerFailed),
            2 => Some(Self::TimedOut),
            3 => Some(Self::Panicked),
            4 => Some(Self::LeaseExpired),
            5 => Some(Self::Abandoned),
            _ => None,
        }
    }

    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HandlerFailed => "handler_failed",
            Self::TimedOut => "timed_out",
            Self::Panicked => "panicked",
            Self::LeaseExpired => "lease_expired",
            Self::Abandoned => "abandoned",
        }
    }

    /// Whether this failure ends the job regardless of attempts remaining.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Abandoned)
    }
}

/// A job an application wants run.
#[derive(Clone, Debug)]
pub struct NewJob {
    queue: QueueName,
    kind: JobKind,
    payload: JobPayload,
    idempotency_key: Option<IdempotencyKey>,
    max_attempts: u32,
    run_at: Option<SystemTime>,
    trace: Option<TraceContext>,
}

impl NewJob {
    /// A job with [`DEFAULT_MAX_ATTEMPTS`], no idempotency key, to run as soon as it is enqueued.
    #[must_use]
    pub const fn new(queue: QueueName, kind: JobKind, payload: JobPayload) -> Self {
        Self {
            queue,
            kind,
            payload,
            idempotency_key: None,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            run_at: None,
            trace: None,
        }
    }

    /// Sets the idempotency key: a second enqueue with the same `(queue, key)` is a duplicate.
    #[must_use]
    pub fn with_idempotency_key(mut self, key: IdempotencyKey) -> Self {
        self.idempotency_key = Some(key);
        self
    }

    /// Sets `max_attempts` (1–100).
    ///
    /// # Errors
    ///
    /// [`JobError::Refused`] with [`JobRefusal::AttemptsOutOfRange`].
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Result<Self, JobError> {
        if max_attempts == 0 || max_attempts > MAX_ATTEMPTS_CAP {
            return Err(JobError::Refused(JobRefusal::AttemptsOutOfRange));
        }
        self.max_attempts = max_attempts;
        Ok(self)
    }

    /// Schedules the job: it is never claimed before `run_at`.
    #[must_use]
    pub const fn scheduled_at(mut self, run_at: SystemTime) -> Self {
        self.run_at = Some(run_at);
        self
    }

    /// Carries the enqueuing operation's trace context into the job (FR-038).
    #[must_use]
    pub fn with_trace(mut self, trace: TraceContext) -> Self {
        self.trace = Some(trace);
        self
    }

    /// The queue.
    #[must_use]
    pub const fn queue(&self) -> &QueueName {
        &self.queue
    }

    /// The kind.
    #[must_use]
    pub const fn kind(&self) -> &JobKind {
        &self.kind
    }

    /// The payload.
    #[must_use]
    pub const fn payload(&self) -> &JobPayload {
        &self.payload
    }

    /// The idempotency key, if any.
    #[must_use]
    pub const fn idempotency_key(&self) -> Option<&IdempotencyKey> {
        self.idempotency_key.as_ref()
    }

    /// The attempt bound.
    #[must_use]
    pub const fn max_attempts(&self) -> u32 {
        self.max_attempts
    }

    /// When the job may first be claimed, or `None` for "at enqueue".
    #[must_use]
    pub const fn run_at(&self) -> Option<SystemTime> {
        self.run_at
    }

    /// The carried trace context.
    #[must_use]
    pub const fn trace(&self) -> Option<&TraceContext> {
        self.trace.as_ref()
    }
}

/// A stored job as a store reports it.
#[derive(Clone)]
pub struct Job {
    /// The identifier.
    pub id: JobId,
    /// The queue.
    pub queue: QueueName,
    /// The kind.
    pub kind: JobKind,
    /// The payload.
    pub payload: JobPayload,
    /// The state.
    pub state: JobState,
    /// Attempts made so far (a claim counts).
    pub attempts: u32,
    /// The attempt bound.
    pub max_attempts: u32,
    /// When the job may next be claimed.
    pub run_at: SystemTime,
    /// The idempotency key, if any.
    pub idempotency_key: Option<IdempotencyKey>,
    /// The most recent failure, if any.
    pub last_failure: Option<FailureKind>,
    /// The carried trace context, if any.
    pub trace: Option<TraceContext>,
    /// When the job was enqueued.
    pub created_at: SystemTime,
    /// When the job last changed.
    pub updated_at: SystemTime,
    /// When the job completed or died, if it has.
    pub finished_at: Option<SystemTime>,
}

/// Identity, queue, kind, state, attempts — **never the payload** (FR-037).
impl fmt::Debug for Job {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Job")
            .field("id", &self.id)
            .field("queue", &self.queue)
            .field("kind", &self.kind)
            .field("state", &self.state)
            .field("attempts", &self.attempts)
            .field("max_attempts", &self.max_attempts)
            .field("payload_bytes", &self.payload.len())
            .finish_non_exhaustive()
    }
}

/// A job a worker has claimed, with the lease that authorises its transitions.
#[derive(Clone, Debug)]
pub struct ClaimedJob {
    /// The job.
    pub job: Job,
    /// The lease.
    pub lease: LeaseToken,
    /// When the lease expires if no transition is made.
    pub lease_expires_at: SystemTime,
}

/// What `enqueue` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enqueued {
    /// A new job was stored.
    Created(JobId),
    /// A job with the same `(queue, idempotency_key)` already exists; nothing was written.
    Duplicate(JobId),
}

impl Enqueued {
    /// The job the caller should track, whichever branch was taken.
    #[must_use]
    pub const fn id(self) -> JobId {
        match self {
            Self::Created(id) | Self::Duplicate(id) => id,
        }
    }
}

/// What `complete` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    /// The job is now completed.
    Completed,
    /// The job was already completed under this lease; nothing changed.
    AlreadyCompleted,
}

/// What `fail` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureOutcome {
    /// The job is ready again at `run_at`, with `attempts` so far.
    Rescheduled {
        /// When it may next be claimed.
        run_at: SystemTime,
        /// Attempts made so far.
        attempts: u32,
    },
    /// The job is dead after `attempts`.
    DeadLettered {
        /// Attempts made in total.
        attempts: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_MAX_ATTEMPTS, FailureKind, IdempotencyKey, JobBounds, JobError, JobId, JobKind,
        JobPayload, JobRefusal, JobState, LeaseToken, MAX_ATTEMPTS_CAP, MAX_IDEMPOTENCY_KEY_BYTES,
        MAX_IDENTIFIER_BYTES, MAX_PAYLOAD_BYTES_CAP, NewJob, QueueName,
    };
    use renvor_core::observe::FixedEntropy;

    #[test]
    fn identifiers_follow_the_grammar() {
        for (index, bad) in [
            "",
            "Mail",
            "a b",
            "a:b",
            &"q".repeat(MAX_IDENTIFIER_BYTES + 1),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                QueueName::new(bad).unwrap_err(),
                JobError::Refused(JobRefusal::QueueNameInvalid),
                "rejected queue-name case {index} was accepted"
            );
            assert_eq!(
                JobKind::new(bad).unwrap_err(),
                JobError::Refused(JobRefusal::KindInvalid),
                "rejected kind case {index} was accepted"
            );
        }
        assert!(QueueName::new("mail.outbound-v2_eu").is_ok());
        assert!(JobKind::new(&"k".repeat(MAX_IDENTIFIER_BYTES)).is_ok());
    }

    #[test]
    fn idempotency_keys_are_bounded_and_printable() {
        assert!(IdempotencyKey::new("order-42").is_ok());
        assert!(IdempotencyKey::new(&"k".repeat(MAX_IDEMPOTENCY_KEY_BYTES)).is_ok());
        for bad in [
            "",
            &"k".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1),
            "a\nb",
            "a\u{7f}",
        ] {
            assert_eq!(
                IdempotencyKey::new(bad).unwrap_err(),
                JobError::Refused(JobRefusal::IdempotencyKeyInvalid)
            );
        }
        let rendered = format!(
            "{:?}",
            IdempotencyKey::new("hunter2CanaryDoNotLeak").unwrap()
        );
        assert!(
            !rendered.contains("hunter2"),
            "the key leaked through Debug"
        );
        assert!(rendered.contains("bytes: 22"));
    }

    #[test]
    fn payloads_are_bounded_at_construction_and_on_read() {
        let bounds = JobBounds::new().with_max_payload_bytes(4).unwrap();
        assert!(JobPayload::within(vec![0; 4], &bounds).is_ok());
        assert_eq!(
            JobPayload::within(vec![0; 5], &bounds).unwrap_err(),
            JobError::Refused(JobRefusal::PayloadTooLarge)
        );
        assert_eq!(
            JobPayload::from_stored(vec![0; 5], &bounds).unwrap_err(),
            JobError::Refused(JobRefusal::PayloadTooLarge)
        );
        assert!(
            JobBounds::new()
                .with_max_payload_bytes(MAX_PAYLOAD_BYTES_CAP + 1)
                .is_err()
        );
        assert!(JobBounds::new().with_max_payload_bytes(0).is_err());
        assert!(JobBounds::new().with_max_queue_depth(0).is_err());
        let rendered = format!(
            "{:?}",
            JobPayload::within(b"hunter2CanaryDoNotLeak".to_vec(), &JobBounds::new()).unwrap()
        );
        assert!(
            !rendered.contains("hunter2"),
            "the payload leaked through Debug"
        );
        assert!(rendered.contains("bytes: 22"));
    }

    #[test]
    fn a_new_job_has_bounded_attempts_and_the_defaults() {
        let job = NewJob::new(
            QueueName::new("q").unwrap(),
            JobKind::new("k").unwrap(),
            JobPayload::within(Vec::new(), &JobBounds::new()).unwrap(),
        );
        assert_eq!(job.max_attempts(), DEFAULT_MAX_ATTEMPTS);
        assert!(job.run_at().is_none());
        assert!(job.idempotency_key().is_none());
        assert!(job.clone().with_max_attempts(MAX_ATTEMPTS_CAP).is_ok());
        assert_eq!(
            job.clone().with_max_attempts(0).unwrap_err(),
            JobError::Refused(JobRefusal::AttemptsOutOfRange)
        );
        assert_eq!(
            job.with_max_attempts(MAX_ATTEMPTS_CAP + 1).unwrap_err(),
            JobError::Refused(JobRefusal::AttemptsOutOfRange)
        );
    }

    #[test]
    fn identifiers_and_leases_are_pure_functions_of_entropy() {
        let a = JobId::generate(&FixedEntropy::new([0xab; 16])).unwrap();
        let b = JobId::generate(&FixedEntropy::new([0xab; 16])).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.encode(), "ab".repeat(16));
        assert_ne!(a, JobId::generate(&FixedEntropy::new([0xac; 16])).unwrap());
        let lease = LeaseToken::generate(&FixedEntropy::new([0x01; 16])).unwrap();
        assert_eq!(format!("{lease:?}"), "LeaseToken(16 bytes)");
        assert_eq!(*lease.as_bytes(), [0x01; 16]);
    }

    #[test]
    fn states_and_failure_kinds_round_trip_their_stored_codes() {
        for state in [
            JobState::Ready,
            JobState::Leased,
            JobState::Completed,
            JobState::Dead,
        ] {
            assert_eq!(JobState::from_u8(state.as_u8()), Some(state));
        }
        assert_eq!(
            JobState::from_u8(9),
            None,
            "an unknown code is refused, not defaulted"
        );
        for kind in [
            FailureKind::HandlerFailed,
            FailureKind::TimedOut,
            FailureKind::Panicked,
            FailureKind::LeaseExpired,
            FailureKind::Abandoned,
        ] {
            assert_eq!(FailureKind::from_u8(kind.as_u8()), Some(kind));
        }
        assert_eq!(FailureKind::from_u8(0), None);
        assert!(FailureKind::Abandoned.is_terminal());
        assert!(!FailureKind::HandlerFailed.is_terminal());
    }
}
