//! Abuse controls: bounded, expiring attempt counters for the six authentication flows.
//!
//! **Read `specs/009-.../evidence/sq-4-bounded-abuse-state.md` and `abuse-control-matrix.md`
//! alongside this module.** They are the normative documents; this is their implementation.
//!
//! # The problem this module exists to solve is cardinality, not secrecy
//!
//! FR-067: *"An attacker MUST NOT be able to grow the key space without bound."*
//!
//! The obvious design — one row per `(dimension, hash(identifier), window)` — fails it, and hashing
//! is not what fixes it. **A digest is a total function on an infinite domain.** `n` distinct
//! forgot-password addresses give `n` distinct digests and therefore `n` rows; hashing bounded the
//! key's *width* and left its *cardinality* untouched. Putting the window in the key multiplies
//! that again, once per window, forever.
//!
//! This is not hypothetical for the network axis either. **IPv6 alone defeats the naive design**:
//! an attacker holding a routine `/64` has 2^64 source addresses, so "one row per address" is
//! unbounded before an email is ever submitted.
//!
//! # What is done instead
//!
//! Every identifier is mapped, **before any storage call**, into a fixed space:
//!
//! ```text
//! bucket = HMAC-SHA256(server_secret, dimension_tag || 0x1F || key_bytes)  &  (buckets - 1)
//! ```
//!
//! and a row is keyed by `(dimension, bucket)` and nothing else. Both components range over finite
//! enumerable sets, so
//!
//! ```text
//! max_rows = |AttemptDimension| × buckets = 10 × 65_536 = 655_360
//! ```
//!
//! **and that holds whether or not [`AttemptRepository::prune`] is ever called.** Pruning reclaims
//! space; it is not what bounds the table. A bound a `DELETE` has to keep winning is not a bound —
//! it is a race against someone who chooses the insert rate.
//!
//! # Three consequences worth stating plainly
//!
//! **The hash is keyed, and that is load-bearing.** With an unkeyed digest an attacker computes
//! which bucket a victim occupies and fills it directly. Under HMAC with a server secret the
//! assignment is unguessable from outside, so a targeted lockout degrades into filling every
//! bucket — `buckets × limit` requests, with the network dimension counting all of them.
//!
//! **Bucketing is lossy, and strangers therefore share limits.** This is the same objection that
//! disqualified `pingora-limits`' Count-Min Sketch during package research, and it is answered
//! rather than dodged: the mapping is keyed (so collisions are accidents, not targets), the refusal
//! is a windowed limit rather than a lockout, and the residual untargeted degradation is priced and
//! recorded. See `package-decisions.md`.
//!
//! **Unknown accounts are counted exactly like known ones.** They must be. The tempting design —
//! key the account axis on a resolved `UserId` and skip unknown accounts — bounds the key space
//! perfectly and builds a complete enumeration oracle: *being rate-limited* is itself the answer to
//! "does this account exist", no matter how generic the response body is.
//!
//! # No early return
//!
//! [`AbuseGuard::admit`] counts **every** dimension for a flow before it returns anything. It does
//! not stop at the first refusal. Two properties follow that a short-circuit would lose: the
//! refusal carries no ordering information (FR-070), and an attacker who has tripped one axis still
//! pays their count on the others.

use core::fmt;

use chrono::{DateTime, Duration, Utc};
use hmac::digest::KeyInit as _;
use hmac::{Hmac, Mac as _};
use renvor_config::Secret;
use renvor_core::identity::ClientIdentity;
use renvor_core::observe::EntropySource;
use renvor_database::DatabaseError;
use sha2::Sha256;

use crate::error::AuthError;
use crate::service::ServiceError;

type HmacSha256 = Hmac<Sha256>;

/// Which of the three axes a dimension counts.
///
/// Exists so the contract matrix is **executable**: a dimension states its axis, a key states its
/// axis, and [`AttemptKeyring::bucket`] refuses a mismatch rather than silently hashing an address
/// into an account dimension's bucket space.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AttemptAxis {
    /// The account identifier **as submitted** — never resolved to a [`crate::UserId`] first.
    Account,
    /// A server-issued identity the caller already holds.
    Client,
    /// The address the runtime attributes the request to.
    Network,
}

/// One of the six flows an abuse control guards.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AttemptFlow {
    /// Password authentication.
    LogIn,
    /// Re-sending a verification mail.
    VerificationResend,
    /// Presenting a verification token.
    VerificationComplete,
    /// Requesting a password-reset mail.
    ForgotPassword,
    /// Presenting a reset token.
    ResetPassword,
    /// Presenting a refresh token for rotation.
    TokenRefresh,
}

impl AttemptFlow {
    /// Every flow. FR-063 names six; this is the list, and its length is asserted in tests.
    pub const ALL: [Self; 6] = [
        Self::LogIn,
        Self::VerificationResend,
        Self::VerificationComplete,
        Self::ForgotPassword,
        Self::ResetPassword,
        Self::TokenRefresh,
    ];

    /// The dimensions this flow counts. **This is the contract matrix, in code.**
    #[must_use]
    pub const fn dimensions(self) -> &'static [AttemptDimension] {
        match self {
            Self::LogIn => &[
                AttemptDimension::LogInAccount,
                AttemptDimension::LogInNetwork,
            ],
            Self::VerificationResend => &[
                AttemptDimension::VerificationResendAccount,
                AttemptDimension::VerificationResendNetwork,
            ],
            // Token-only flows carry no account identifier. Resolving the token to charge an
            // account axis would leave an INVALID token with no account to charge, and that
            // difference in stored state is observable. See the matrix.
            Self::VerificationComplete => &[AttemptDimension::VerificationCompleteNetwork],
            Self::ForgotPassword => &[
                AttemptDimension::ForgotPasswordAccount,
                AttemptDimension::ForgotPasswordNetwork,
            ],
            Self::ResetPassword => &[AttemptDimension::ResetPasswordNetwork],
            Self::TokenRefresh => &[
                AttemptDimension::TokenRefreshClient,
                AttemptDimension::TokenRefreshNetwork,
            ],
        }
    }

    /// The audit action this flow records, on both the permitted and the refused path.
    #[must_use]
    pub const fn audit_action(self) -> crate::audit::AuditAction {
        match self {
            Self::LogIn => crate::audit::AuditAction::LogIn,
            Self::VerificationResend => crate::audit::AuditAction::VerificationResend,
            Self::VerificationComplete => crate::audit::AuditAction::VerificationComplete,
            Self::ForgotPassword => crate::audit::AuditAction::PasswordForgot,
            Self::ResetPassword => crate::audit::AuditAction::PasswordReset,
            Self::TokenRefresh => crate::audit::AuditAction::TokenRefresh,
        }
    }
}

/// A counted dimension: one `(flow, axis)` pair.
///
/// **Closed and fieldless.** `|AttemptDimension|` is one of the two factors in the row bound, so a
/// dimension that could be constructed from caller input would make the bound unprovable. There is
/// no such constructor.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum AttemptDimension {
    /// Login, by submitted account identifier.
    LogInAccount,
    /// Login, by network address.
    LogInNetwork,
    /// Verification resend, by submitted account identifier.
    VerificationResendAccount,
    /// Verification resend, by network address.
    VerificationResendNetwork,
    /// Verification completion, by network address.
    VerificationCompleteNetwork,
    /// Forgot password, by submitted account identifier.
    ForgotPasswordAccount,
    /// Forgot password, by network address.
    ForgotPasswordNetwork,
    /// Reset password, by network address.
    ResetPasswordNetwork,
    /// Token refresh, by refresh family.
    TokenRefreshClient,
    /// Token refresh, by network address.
    TokenRefreshNetwork,
}

impl AttemptDimension {
    /// Every dimension. **Its length is the `|AttemptDimension|` in the row bound.**
    pub const ALL: [Self; 10] = [
        Self::LogInAccount,
        Self::LogInNetwork,
        Self::VerificationResendAccount,
        Self::VerificationResendNetwork,
        Self::VerificationCompleteNetwork,
        Self::ForgotPasswordAccount,
        Self::ForgotPasswordNetwork,
        Self::ResetPasswordNetwork,
        Self::TokenRefreshClient,
        Self::TokenRefreshNetwork,
    ];

    /// How many dimensions there are.
    pub const COUNT: usize = Self::ALL.len();

    /// The value stored in the `dimension` column.
    ///
    /// **Stable.** Changing a code re-points every stored row at a different dimension, which is a
    /// migration, not an edit. A test pins every code to its value for exactly that reason.
    #[must_use]
    pub const fn code(self) -> i16 {
        match self {
            Self::LogInAccount => 1,
            Self::LogInNetwork => 2,
            Self::VerificationResendAccount => 3,
            Self::VerificationResendNetwork => 4,
            Self::VerificationCompleteNetwork => 5,
            Self::ForgotPasswordAccount => 6,
            Self::ForgotPasswordNetwork => 7,
            Self::ResetPasswordNetwork => 8,
            Self::TokenRefreshClient => 9,
            Self::TokenRefreshNetwork => 10,
        }
    }

    /// The dimension a stored code names, or `None`.
    ///
    /// Fail-closed: an unrecognised code is refused rather than defaulted. A default would silently
    /// merge an unknown dimension's rows into a known one's counters.
    #[must_use]
    pub const fn from_code(code: i16) -> Option<Self> {
        match code {
            1 => Some(Self::LogInAccount),
            2 => Some(Self::LogInNetwork),
            3 => Some(Self::VerificationResendAccount),
            4 => Some(Self::VerificationResendNetwork),
            5 => Some(Self::VerificationCompleteNetwork),
            6 => Some(Self::ForgotPasswordAccount),
            7 => Some(Self::ForgotPasswordNetwork),
            8 => Some(Self::ResetPasswordNetwork),
            9 => Some(Self::TokenRefreshClient),
            10 => Some(Self::TokenRefreshNetwork),
            _ => None,
        }
    }

    /// Which axis this counts.
    #[must_use]
    pub const fn axis(self) -> AttemptAxis {
        match self {
            Self::LogInAccount | Self::VerificationResendAccount | Self::ForgotPasswordAccount => {
                AttemptAxis::Account
            }
            Self::TokenRefreshClient => AttemptAxis::Client,
            Self::LogInNetwork
            | Self::VerificationResendNetwork
            | Self::VerificationCompleteNetwork
            | Self::ForgotPasswordNetwork
            | Self::ResetPasswordNetwork
            | Self::TokenRefreshNetwork => AttemptAxis::Network,
        }
    }

    /// The **domain-separation tag** mixed into the HMAC.
    ///
    /// Without it, one identifier would land in the same bucket index in every dimension, so
    /// filling a login bucket would fill the forgot-password bucket for the same address. The tag
    /// makes each dimension's assignment independent.
    #[must_use]
    const fn tag(self) -> &'static str {
        match self {
            Self::LogInAccount => "login/account",
            Self::LogInNetwork => "login/network",
            Self::VerificationResendAccount => "verification-resend/account",
            Self::VerificationResendNetwork => "verification-resend/network",
            Self::VerificationCompleteNetwork => "verification-complete/network",
            Self::ForgotPasswordAccount => "forgot-password/account",
            Self::ForgotPasswordNetwork => "forgot-password/network",
            Self::ResetPasswordNetwork => "reset-password/network",
            Self::TokenRefreshClient => "token-refresh/client",
            Self::TokenRefreshNetwork => "token-refresh/network",
        }
    }
}

/// How many buckets each dimension is divided into.
///
/// A **power of two**, so the reduction is a mask rather than a modulo and there is no modulo bias
/// pulling extra identifiers into the low buckets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttemptBuckets(u32);

impl AttemptBuckets {
    /// The smallest permitted count.
    pub const MIN: u32 = 256;
    /// The largest permitted count. At the ceiling the table can hold 10 × 2^20 rows.
    pub const MAX: u32 = 1_048_576;
    /// The shipped default: 2^16.
    pub const DEFAULT: u32 = 65_536;

    /// Builds a bucket count.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `count` is not a power of two, or is outside
    /// `[MIN, MAX]`. Refused rather than rounded: an operator who asked for 100_000 buckets and
    /// silently got 65_536 would have a row bound they did not configure.
    pub const fn new(count: u32) -> Result<Self, AuthError> {
        if !count.is_power_of_two() || count < Self::MIN || count > Self::MAX {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self(count))
    }

    /// The configured count.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The maximum number of rows the abuse table can ever hold at this bucket count.
    ///
    /// `|AttemptDimension| × buckets`. Stated as a function so the formula is checkable rather than
    /// a number in a comment.
    #[must_use]
    pub const fn max_rows(self) -> u64 {
        (AttemptDimension::COUNT as u64) * (self.0 as u64)
    }
}

impl Default for AttemptBuckets {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

/// A bucket index: `0 .. buckets`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct AttemptBucket(u32);

impl AttemptBucket {
    /// The index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// What is being counted.
///
/// A **closed enum**, which is what confines arbitrary caller text to one place: the account
/// variant. That text is normalised, hashed under the server key, reduced to a bucket index, and
/// then dropped. It never reaches storage, a log, an error, or an audit event.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttemptKey<'a> {
    /// The account identifier as the requester submitted it.
    Account(&'a str),
    /// A server-issued credential family — the refresh family for token refresh.
    Client([u8; 16]),
    /// The address the runtime resolved, **not** a header this crate parsed.
    Network(ClientIdentity),
}

impl AttemptKey<'_> {
    /// Which axis this key can be counted on.
    #[must_use]
    pub const fn axis(&self) -> AttemptAxis {
        match self {
            Self::Account(_) => AttemptAxis::Account,
            Self::Client(_) => AttemptAxis::Client,
            Self::Network(_) => AttemptAxis::Network,
        }
    }
}

/// The server secret that makes bucket assignment unguessable, plus the bucket count.
///
/// # Rotating the key re-randomises every assignment
///
/// A deliberate, recorded consequence rather than a bug: accumulated counts are discarded and the
/// control fails **open** for one window. It costs exactly what restarting with an empty table
/// costs, and it is the operator's decision.
pub struct AttemptKeyring {
    key: Secret<[u8; 32]>,
    buckets: AttemptBuckets,
}

impl AttemptKeyring {
    /// Generates a keyring from the entropy port.
    ///
    /// # Errors
    ///
    /// [`AuthError::EntropyUnavailable`]. **No fallback** — an unkeyed or weakly-keyed bucket
    /// assignment is a computable one, which is the whole property this key exists to deny.
    pub fn generate(
        source: &dyn EntropySource,
        buckets: AttemptBuckets,
    ) -> Result<Self, AuthError> {
        let mut bytes = [0_u8; 32];
        source
            .fill(&mut bytes)
            .map_err(|_| AuthError::EntropyUnavailable)?;
        Ok(Self {
            key: Secret::new("abuse-bucket-key", bytes),
            buckets,
        })
    }

    /// Rebuilds a keyring an operator supplied, so a fleet buckets alike.
    ///
    /// Every process must agree: two processes with different keys would put one identifier in two
    /// buckets and each would see half the evidence.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 32], buckets: AttemptBuckets) -> Self {
        Self {
            key: Secret::new("abuse-bucket-key", bytes),
            buckets,
        }
    }

    /// The configured bucket count.
    #[must_use]
    pub const fn buckets(&self) -> AttemptBuckets {
        self.buckets
    }

    /// Maps a key into `dimension`'s bucket space.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when the key's axis is not the dimension's axis — an
    /// address counted as an account, or an account counted as a network. Fail-closed, because the
    /// alternative is a silently mis-attributed counter that still looks like it is working.
    pub fn bucket(
        &self,
        dimension: AttemptDimension,
        key: AttemptKey<'_>,
    ) -> Result<AttemptBucket, AuthError> {
        if dimension.axis() != key.axis() {
            return Err(AuthError::PolicyMisconfigured);
        }

        let mut mac = HmacSha256::new_from_slice(self.key.expose().as_slice())
            .map_err(|_| AuthError::PolicyMisconfigured)?;
        // Domain separation: the tag, then a separator that cannot occur in any tag, then the key
        // bytes. Without the separator "login/account" + "x" and "login/accountx" + "" would hash
        // alike.
        mac.update(dimension.tag().as_bytes());
        mac.update(&[0x1F]);
        match key {
            // Normalisation happens HERE and nowhere else, so "Ada@Example.COM " and
            // "ada@example.com" share a counter — which they must, or the control is bypassed by
            // changing the case of a letter.
            AttemptKey::Account(identifier) => {
                mac.update(identifier.trim().to_lowercase().as_bytes());
            }
            AttemptKey::Client(id) => mac.update(&id),
            AttemptKey::Network(identity) => {
                // A tagged encoding, so a v4 address and a v4-mapped v6 address are one key rather
                // than two — and so the two families cannot collide by sharing a byte pattern.
                match identity.address() {
                    std::net::IpAddr::V4(v4) => {
                        mac.update(&[4]);
                        mac.update(&v4.octets());
                    }
                    std::net::IpAddr::V6(v6) => {
                        // A v4-mapped v6 address is folded to its v4 form deliberately: an attacker
                        // who could switch between the two representations would otherwise hold two
                        // buckets for one address.
                        if let Some(v4) = v6.to_ipv4_mapped() {
                            mac.update(&[4]);
                            mac.update(&v4.octets());
                        } else {
                            mac.update(&[6]);
                            mac.update(&v6.octets());
                        }
                    }
                }
            }
        }

        let digest = mac.finalize().into_bytes();
        let mut head = [0_u8; 4];
        head.copy_from_slice(&digest[..4]);
        // A MASK, not a modulo. `buckets` is a power of two by construction, so this is uniform
        // over the bucket space and has no bias toward low indices.
        Ok(AttemptBucket(
            u32::from_be_bytes(head) & (self.buckets.get() - 1),
        ))
    }
}

impl fmt::Debug for AttemptKeyring {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AttemptKeyring")
            .field("key", &"[redacted]")
            .field("buckets", &self.buckets.get())
            .finish()
    }
}

/// The limit and window for one dimension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttemptLimit {
    limit: u32,
    window_seconds: i64,
}

impl AttemptLimit {
    /// The shortest permitted window.
    pub const MIN_WINDOW_SECONDS: i64 = 1;
    /// The longest permitted window: 24 hours. A window longer than a day makes a transient
    /// mistake outlive the day it was made in.
    pub const MAX_WINDOW_SECONDS: i64 = 86_400;

    /// Builds a limit.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] when `limit` is zero, when `window` is not a whole
    /// number of seconds, or when it is outside `[1s, 24h]`.
    ///
    /// A whole number of seconds is required because window boundaries are computed by integer
    /// division from the Unix epoch; a fractional window would put different processes in
    /// different windows.
    pub fn new(limit: u32, window: Duration) -> Result<Self, AuthError> {
        let seconds = window.num_seconds();
        if limit == 0
            || window != Duration::seconds(seconds)
            || !(Self::MIN_WINDOW_SECONDS..=Self::MAX_WINDOW_SECONDS).contains(&seconds)
        {
            return Err(AuthError::PolicyMisconfigured);
        }
        Ok(Self {
            limit,
            window_seconds: seconds,
        })
    }

    /// How many attempts are permitted per window.
    #[must_use]
    pub const fn limit(self) -> u32 {
        self.limit
    }

    /// The window length.
    #[must_use]
    pub const fn window(self) -> Duration {
        Duration::seconds(self.window_seconds)
    }

    /// The start of the window containing `now`.
    ///
    /// Anchored at the **Unix epoch**, so every process in a fleet computes the same boundaries
    /// without coordinating. `div_euclid` rather than `/` so instants before 1970 floor correctly
    /// instead of truncating toward zero.
    #[must_use]
    pub fn window_start(self, now: DateTime<Utc>) -> DateTime<Utc> {
        let seconds = now.timestamp().div_euclid(self.window_seconds) * self.window_seconds;
        DateTime::from_timestamp(seconds, 0).unwrap_or(now)
    }

    /// Whether `state` puts the caller over this limit.
    ///
    /// # The weighted estimate, and why it is not decoration
    ///
    /// A pure fixed window admits `2 × limit` requests across a boundary: `limit` at the end of one
    /// window and `limit` at the start of the next. Charging the tail of the previous window bounds
    /// the burst at roughly `limit` over any span of one window length, and it costs one integer
    /// column rather than one extra row.
    ///
    /// All integer arithmetic, in `u128`, deliberately: a float in a security decision is a
    /// rounding mode nobody reviewed.
    #[must_use]
    pub fn exceeded_by(self, state: &AttemptState, now: DateTime<Utc>) -> bool {
        let window = self.window_seconds.max(1) as u128;
        let elapsed = (now - state.window_start)
            .num_seconds()
            .clamp(0, self.window_seconds) as u128;
        let remaining = window.saturating_sub(elapsed);
        let weighted = u128::from(state.previous)
            .saturating_mul(remaining)
            .saturating_div(window);
        weighted.saturating_add(u128::from(state.current)) > u128::from(self.limit)
    }
}

/// The whole contract matrix: a limit for every dimension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AbuseContract {
    limits: [AttemptLimit; AttemptDimension::COUNT],
}

impl AbuseContract {
    /// The limit for one dimension.
    #[must_use]
    pub fn get(&self, dimension: AttemptDimension) -> AttemptLimit {
        self.limits[Self::index(dimension)]
    }

    /// Replaces one dimension's limit.
    #[must_use]
    pub fn with(mut self, dimension: AttemptDimension, limit: AttemptLimit) -> Self {
        self.limits[Self::index(dimension)] = limit;
        self
    }

    /// The position of a dimension in the array. Derived from `ALL`, so it cannot drift from it.
    fn index(dimension: AttemptDimension) -> usize {
        AttemptDimension::ALL
            .iter()
            .position(|candidate| *candidate == dimension)
            .unwrap_or(0)
    }
}

impl Default for AbuseContract {
    /// The shipped matrix, matching `evidence/abuse-control-matrix.md` §2 exactly.
    ///
    /// The mail-amplifying flows — resend and forgot-password — carry the tightest account limits
    /// in the table, because the cost of exceeding them is borne by a third party's inbox. They
    /// share limits with each other on purpose: they are interchangeable to an attacker, so
    /// different limits would just mean using the looser one.
    fn default() -> Self {
        let minutes = |n: i64| Duration::seconds(n * 60);
        let limit = |count, window| {
            AttemptLimit::new(count, window).expect("the shipped matrix is within bounds")
        };
        Self {
            limits: [
                limit(20, minutes(15)),  // LogInAccount
                limit(100, minutes(15)), // LogInNetwork
                limit(5, minutes(60)),   // VerificationResendAccount
                limit(30, minutes(60)),  // VerificationResendNetwork
                limit(60, minutes(15)),  // VerificationCompleteNetwork
                limit(5, minutes(60)),   // ForgotPasswordAccount
                limit(30, minutes(60)),  // ForgotPasswordNetwork
                limit(60, minutes(15)),  // ResetPasswordNetwork
                limit(60, minutes(15)),  // TokenRefreshClient
                limit(300, minutes(15)), // TokenRefreshNetwork
            ],
        }
    }
}

/// A row's counters, as they stand after an observation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttemptState {
    /// Attempts in the current window, **including the one just counted**.
    pub current: u64,
    /// Attempts in the immediately preceding window.
    pub previous: u64,
    /// The start of the current window.
    pub window_start: DateTime<Utc>,
}

impl AttemptState {
    /// The saturation ceiling.
    ///
    /// `i64::MAX`, not `u64::MAX`: the column is `BIGINT`, and a value that could not round-trip
    /// through it must never be written. Saturating here rather than in SQL is what keeps the
    /// engines from raising an overflow error — PostgreSQL `22003`, MySQL `1264` — inside a
    /// rate-limit check on an unauthenticated endpoint.
    pub const CEILING: u64 = i64::MAX as u64;
}

/// One increment request, fully computed by the caller so the adapter decides nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttemptObservation {
    /// Which counter.
    pub dimension: AttemptDimension,
    /// Which bucket within it.
    pub bucket: AttemptBucket,
    /// The start of the window `now` falls in.
    pub window_start: DateTime<Utc>,
    /// The start of the window before it — what a stored row must match to have its count rolled
    /// rather than discarded.
    pub previous_window_start: DateTime<Utc>,
    /// When the row stops being worth keeping: `window_start + 2 × window`.
    pub expires_at: DateTime<Utc>,
}

/// What an observation did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttemptOutcome {
    /// The attempt was counted; the row now reads like this.
    Counted(AttemptState),
    /// The stored window start is **in the future**, so nothing was written.
    ///
    /// Either the clock moved backwards or the row is not ours. Writing would erase evidence, so
    /// the caller refuses instead.
    ClockRegressed,
}

/// Where attempt counters live. FR-069 — proven on all four rows.
///
/// # Atomicity is the port's requirement, not the caller's
///
/// [`Self::observe`] must increment **and** return the resulting state as one atomic step. A
/// separate read-then-write, however carefully ordered, lets two concurrent attempts both read
/// `limit - 1` and both proceed. Every implementation locks the row it reads, per
/// `contracts/database-portability.md` §3.
pub trait AttemptRepository: Send + Sync {
    /// Counts one attempt and returns the row as it now stands.
    ///
    /// # Errors
    ///
    /// [`DatabaseError`]. **The caller must fail closed** — see [`AbuseGuard::admit`].
    fn observe(
        &self,
        observation: AttemptObservation,
    ) -> impl core::future::Future<Output = Result<AttemptOutcome, DatabaseError>> + Send;

    /// Deletes expired rows in the half-open bucket range `[from, from + count)`.
    ///
    /// Bounded by construction: at most `count` rows, because `(dimension, bucket)` is unique.
    /// A `LIMIT` was deliberately not used — PostgreSQL has none on `DELETE`, and MySQL refuses one
    /// inside an `IN` subquery, so the portable-looking forms are not portable.
    ///
    /// **The row bound does not depend on this ever being called.**
    ///
    /// # Errors
    ///
    /// [`DatabaseError`].
    fn prune(
        &self,
        dimension: AttemptDimension,
        from: u32,
        count: u32,
        now: DateTime<Utc>,
    ) -> impl core::future::Future<Output = Result<u64, DatabaseError>> + Send;
}

/// A shared reference to a repository is itself a repository.
///
/// Without this, [`AbuseGuard`] would have to **own** its store, and a caller that already holds
/// one — a four-row fixture, an application that shares one pool across several guards — would
/// have to clone it or wrap it in an `Arc` purely to satisfy a bound. Forwarding is the honest
/// alternative: it adds no behaviour, so there is nothing here for a test to accidentally measure
/// instead of the real implementation.
impl<T> AttemptRepository for &T
where
    T: AttemptRepository + ?Sized,
{
    fn observe(
        &self,
        observation: AttemptObservation,
    ) -> impl core::future::Future<Output = Result<AttemptOutcome, DatabaseError>> + Send {
        (**self).observe(observation)
    }

    fn prune(
        &self,
        dimension: AttemptDimension,
        from: u32,
        count: u32,
        now: DateTime<Utc>,
    ) -> impl core::future::Future<Output = Result<u64, DatabaseError>> + Send {
        (**self).prune(dimension, from, count, now)
    }
}

/// The keys a flow's dimensions are counted on.
#[derive(Clone, Copy, Debug)]
pub struct FlowKeys<'a> {
    /// The submitted account identifier, for flows with an account axis.
    pub account: Option<&'a str>,
    /// The server-issued family, for flows with a client axis.
    pub client: Option<[u8; 16]>,
    /// The resolved address. Always required — every flow has a network axis.
    pub network: ClientIdentity,
}

/// Proof that an attempt was counted and admitted.
///
/// # There is no public constructor, and that is the enforcement
///
/// FR-063 requires the six flows to be **bounded**. A guard the caller is merely *expected* to
/// invoke is bounded only in the flows somebody remembered — and the flow that gets forgotten is
/// the one nobody is looking at.
///
/// So the flows take one of these, and the only thing that makes one is [`AbuseGuard::admit`].
/// [`crate::service::AuthenticationService::log_in`] cannot be called without a counted attempt,
/// because there is no value of this type that did not come from counting one.
///
/// This is the same shape [`crate::policy::Authorized`] uses for FR-061, reused rather than
/// reinvented: a capability whose absence of a constructor is the guarantee.
///
/// It names the flow it admitted, so an admission earned on a cheap flow cannot be spent on an
/// expensive one — a forgot-password admission will not open a login.
///
/// # The claim, pinned from outside this crate
///
/// The field is private and there is no constructor, so no other crate can make one:
///
/// ```compile_fail
/// use renvor_auth::abuse::{Admitted, AttemptFlow};
/// // `flow` is private, so this is not a value another crate can assemble.
/// let forged = Admitted { flow: AttemptFlow::LogIn };
/// ```
///
/// The control below **does** compile, which is what stops the block above passing for the wrong
/// reason — a renamed type or a moved module would fail `compile_fail` just as happily as a private
/// field does. Naming the type is fine; making one is not.
///
/// ```
/// use renvor_auth::abuse::{Admitted, AttemptFlow};
///
/// fn a_flow_that_requires_admission(_: Admitted) {}
/// let _ = a_flow_that_requires_admission;
/// let _ = AttemptFlow::LogIn;
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[must_use = "an admission that is not passed to a flow counted an attempt for nothing"]
pub struct Admitted {
    flow: AttemptFlow,
}

impl Admitted {
    /// Which flow this admits.
    #[must_use]
    pub const fn flow(self) -> AttemptFlow {
        self.flow
    }

    /// Builds an admission **without counting anything**. Test-only, and crate-internal.
    ///
    /// # Why this exists and why it does not weaken the guarantee
    ///
    /// This crate's own tests for the five mailed and password flows are about those flows, not
    /// about the abuse control, and threading a real guard and a fake store through each of them
    /// would measure the wrong thing.
    ///
    /// It is behind `#[cfg(test)]`, so **it does not exist in any build a consumer ever compiles**
    /// — not in a release build, not in a debug build, not with every feature on. And it is
    /// `pub(crate)`, so it is not reachable from `renvor-sqlx`, `renvor-seaorm`, `renvor-http`, or
    /// an application. The structural claim FR-063 rests on is about code outside this crate, and
    /// the doctest on [`Admitted`] pins it from exactly there.
    #[cfg(test)]
    pub(crate) const fn for_test(flow: AttemptFlow) -> Self {
        Self { flow }
    }

    /// Refuses unless this admits `expected`.
    ///
    /// # Errors
    ///
    /// [`AuthError::PolicyMisconfigured`] — a configuration mistake at a call site, never
    /// something a requester can cause, so it does not need to be indistinguishable from anything.
    pub const fn expect(self, expected: AttemptFlow) -> Result<(), AuthError> {
        if self.flow as u8 == expected as u8 {
            Ok(())
        } else {
            Err(AuthError::PolicyMisconfigured)
        }
    }
}

/// The guard the application operations call.
///
/// # It owns the audit sink, and that is where the six flows' records come from
///
/// `evidence/abuse-control-matrix.md` §6 requires **one audit event per attempt, on both the
/// permitted and the refused path**, for every one of the six flows. Emitting only on refusal would
/// make the presence of a record the oracle the refusal itself is not.
///
/// Putting the sink here rather than in each flow method is what makes that structural: every
/// attempt at every guarded flow passes through [`Self::admit`], because [`Admitted`] has no other
/// source. There is no flow that can be reached without a record having been attempted.
#[derive(Debug)]
pub struct AbuseGuard<R, A> {
    repository: R,
    keyring: AttemptKeyring,
    contract: AbuseContract,
    audit: A,
}

impl<R, A> AbuseGuard<R, A>
where
    R: AttemptRepository,
    A: crate::audit::AuditSink,
{
    /// Builds a guard.
    #[must_use]
    pub const fn new(
        repository: R,
        keyring: AttemptKeyring,
        contract: AbuseContract,
        audit: A,
    ) -> Self {
        Self {
            repository,
            keyring,
            contract,
            audit,
        }
    }

    /// The configured matrix.
    #[must_use]
    pub const fn contract(&self) -> &AbuseContract {
        &self.contract
    }

    /// The keyring, for a caller that needs to prune.
    #[must_use]
    pub const fn keyring(&self) -> &AttemptKeyring {
        &self.keyring
    }

    /// Counts one attempt on **every** dimension of `flow`, then decides.
    ///
    /// # There is no early return, and no counter is ever cleared
    ///
    /// Every dimension is observed before anything is returned. A short-circuit would let a
    /// refusal disclose which axis tripped by which counters moved (FR-070), and would let an
    /// attacker who has already tripped one axis make free requests against the others.
    ///
    /// Success does not decrement, reset, or delete anything (FR-068). There is no code path from
    /// this function's caller back to the store, which is what makes that structural rather than a
    /// convention.
    ///
    /// # Errors
    ///
    /// - [`ServiceError::Refused`] with [`AuthError::TooManyAttempts`] when any dimension is over
    ///   its limit, or when the stored clock regressed. **The same fieldless value in every case**,
    ///   so it names neither the dimension nor the flow.
    /// - [`ServiceError::Refused`] with [`AuthError::PolicyMisconfigured`] when a flow's required
    ///   key is absent.
    /// - [`ServiceError::Storage`] when the store fails. **Fail closed**: the guarded operation
    ///   does not run. There is no in-memory fallback and no permissive path, because a rate
    ///   limiter that stops limiting when its store is unavailable is an availability-triggered
    ///   authentication bypass.
    pub async fn admit(
        &self,
        flow: AttemptFlow,
        keys: FlowKeys<'_>,
        correlation: crate::audit::CorrelationId,
        now: DateTime<Utc>,
    ) -> Result<Admitted, ServiceError> {
        let mut refused = false;
        for dimension in flow.dimensions() {
            let key = match dimension.axis() {
                AttemptAxis::Account => AttemptKey::Account(
                    keys.account
                        .ok_or(ServiceError::Refused(AuthError::PolicyMisconfigured))?,
                ),
                AttemptAxis::Client => AttemptKey::Client(
                    keys.client
                        .ok_or(ServiceError::Refused(AuthError::PolicyMisconfigured))?,
                ),
                AttemptAxis::Network => AttemptKey::Network(keys.network),
            };
            let bucket = self.keyring.bucket(*dimension, key)?;
            let limit = self.contract.get(*dimension);
            let window_start = limit.window_start(now);

            let outcome = self
                .repository
                .observe(AttemptObservation {
                    dimension: *dimension,
                    bucket,
                    window_start,
                    previous_window_start: window_start - limit.window(),
                    expires_at: window_start + limit.window() + limit.window(),
                })
                .await?;

            // `|=`, never `return`. See the doc comment.
            refused |= match outcome {
                AttemptOutcome::Counted(state) => limit.exceeded_by(&state, now),
                AttemptOutcome::ClockRegressed => true,
            };
        }

        // THE AUDIT RECORD, on BOTH paths, with the same shape and the same fields.
        //
        // The actor is anonymous and the subject is unspecified for every one of these flows, and
        // that is deliberate rather than lazy: naming the account a forgot-password request asked
        // about would put "this account exists" into the audit trail, where it is exactly as
        // readable as it would be in a response body. The trail records THAT a bounded flow was
        // attempted and what was decided — never who it was about.
        self.audit
            .record(crate::audit::AuditEvent::new(
                flow.audit_action(),
                if refused {
                    crate::audit::AuditOutcome::Refused
                } else {
                    crate::audit::AuditOutcome::Permitted
                },
                crate::audit::AuditActor::Anonymous,
                crate::audit::AuditSubject::Unspecified,
                correlation,
                now,
            ))
            .await
            // FAIL CLOSED, and identically on both paths. A sink that failed for one and not the
            // other would make its own health the answer to a question the refusal refuses.
            .map_err(|_| ServiceError::Refused(AuthError::TooManyAttempts))?;

        if refused {
            return Err(ServiceError::Refused(AuthError::TooManyAttempts));
        }
        Ok(Admitted { flow })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AbuseContract, AbuseGuard, AttemptAxis, AttemptBuckets, AttemptDimension, AttemptFlow,
        AttemptKey, AttemptKeyring, AttemptLimit, AttemptObservation, AttemptOutcome,
        AttemptRepository, AttemptState, FlowKeys,
    };
    use crate::audit::{AuditAction, AuditOutcome, CorrelationId, RecordingAuditSink};
    use crate::error::AuthError;
    use crate::service::ServiceError;
    use chrono::{DateTime, Duration, TimeZone as _, Utc};
    use renvor_core::identity::ClientIdentity;
    use renvor_database::DatabaseError;
    use std::collections::{BTreeSet, HashMap};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use std::sync::Mutex;

    // A FAKE. Its async methods contain no `.await`, so `tokio::join!` on two of them runs them
    // one after the other and never attempts an interleaving.
    //
    // **This fake therefore cannot fail a concurrency test, and no concurrency claim is made from
    // it.** Batch G2 shipped a HIGH race under exactly that mistake: a green unit suite over a fake
    // with no suspension point. Atomicity is proven in the four-row suite
    // (`renvor_testkit::abuse`) or nowhere.
    #[derive(Default)]
    struct FakeAttempts {
        rows: Mutex<HashMap<(i16, u32), AttemptState>>,
        observations: Mutex<Vec<AttemptDimension>>,
        fail: bool,
    }

    impl FakeAttempts {
        fn failing() -> Self {
            Self {
                fail: true,
                ..Self::default()
            }
        }
        fn seen(&self) -> Vec<AttemptDimension> {
            self.observations.lock().expect("unpoisoned").clone()
        }
        fn rows(&self) -> usize {
            self.rows.lock().expect("unpoisoned").len()
        }
    }

    impl AttemptRepository for FakeAttempts {
        async fn observe(
            &self,
            observation: AttemptObservation,
        ) -> Result<AttemptOutcome, DatabaseError> {
            if self.fail {
                return Err(renvor_database::DatabaseError::new(
                    renvor_database::DatabaseErrorKind::ConnectFailed,
                ));
            }
            self.observations
                .lock()
                .expect("unpoisoned")
                .push(observation.dimension);
            let mut rows = self.rows.lock().expect("unpoisoned");
            let key = (observation.dimension.code(), observation.bucket.get());
            let state = rows.entry(key).or_insert(AttemptState {
                current: 0,
                previous: 0,
                window_start: observation.window_start,
            });
            if state.window_start > observation.window_start {
                return Ok(AttemptOutcome::ClockRegressed);
            }
            if state.window_start == observation.window_start {
                state.current = state.current.saturating_add(1).min(AttemptState::CEILING);
            } else if state.window_start == observation.previous_window_start {
                state.previous = state.current;
                state.current = 1;
                state.window_start = observation.window_start;
            } else {
                state.previous = 0;
                state.current = 1;
                state.window_start = observation.window_start;
            }
            Ok(AttemptOutcome::Counted(*state))
        }

        async fn prune(
            &self,
            _dimension: AttemptDimension,
            _from: u32,
            _count: u32,
            _now: DateTime<Utc>,
        ) -> Result<u64, DatabaseError> {
            Ok(0)
        }
    }

    fn at(minute: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0)
            .single()
            .expect("a real instant")
            + Duration::seconds(minute * 60)
    }

    /// A FIXED key, so every bucket index below is a deterministic fact rather than a sample.
    fn keyring(buckets: u32) -> AttemptKeyring {
        AttemptKeyring::from_bytes(
            [0xA5; 32],
            AttemptBuckets::new(buckets).expect("a legal bucket count"),
        )
    }

    fn correlation() -> CorrelationId {
        CorrelationId::from_bytes([9, 9, 9, 9, 1, 2, 3, 4])
    }

    fn network(last: u8) -> ClientIdentity {
        ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::new(203, 0, 113, last)))
    }

    // ---- the vocabulary is closed and its codes are stable ------------------------------------

    #[test]
    fn every_dimension_has_a_distinct_stable_code_that_round_trips() {
        // A CHANGED CODE IS A MIGRATION, not an edit: it re-points every stored row at a different
        // dimension. The literals are pinned here so that change cannot happen quietly.
        let expected: [(AttemptDimension, i16); 10] = [
            (AttemptDimension::LogInAccount, 1),
            (AttemptDimension::LogInNetwork, 2),
            (AttemptDimension::VerificationResendAccount, 3),
            (AttemptDimension::VerificationResendNetwork, 4),
            (AttemptDimension::VerificationCompleteNetwork, 5),
            (AttemptDimension::ForgotPasswordAccount, 6),
            (AttemptDimension::ForgotPasswordNetwork, 7),
            (AttemptDimension::ResetPasswordNetwork, 8),
            (AttemptDimension::TokenRefreshClient, 9),
            (AttemptDimension::TokenRefreshNetwork, 10),
        ];
        for (dimension, code) in expected {
            assert_eq!(dimension.code(), code);
            assert_eq!(AttemptDimension::from_code(code), Some(dimension));
        }
        assert_eq!(AttemptDimension::ALL.len(), 10);
        assert_eq!(AttemptDimension::COUNT, 10);

        // FAIL CLOSED: an unrecognised code is refused, never defaulted. Defaulting would merge an
        // unknown dimension's rows into a known one's counters.
        for absent in [i16::MIN, -1, 0, 11, 12, i16::MAX] {
            assert_eq!(AttemptDimension::from_code(absent), None);
        }
    }

    #[test]
    fn the_matrix_in_code_matches_the_matrix_in_the_contract() {
        // Three account axes, one client axis, six network axes = 10. Those are the numbers the
        // row bound is computed from, so they are asserted rather than described.
        let count = |axis| {
            AttemptDimension::ALL
                .iter()
                .filter(|d| d.axis() == axis)
                .count()
        };
        assert_eq!(count(AttemptAxis::Account), 3);
        assert_eq!(count(AttemptAxis::Client), 1);
        assert_eq!(count(AttemptAxis::Network), 6);

        // Six flows, and EVERY one carries a network axis — that is what bounds the token-only
        // flows, which deliberately have no account axis.
        assert_eq!(AttemptFlow::ALL.len(), 6);
        let mut used = BTreeSet::new();
        for flow in AttemptFlow::ALL {
            let dimensions = flow.dimensions();
            assert!(
                dimensions.iter().any(|d| d.axis() == AttemptAxis::Network),
                "{flow:?} has no network axis"
            );
            for dimension in dimensions {
                assert!(AttemptDimension::ALL.contains(dimension));
                assert!(used.insert(*dimension), "{dimension:?} is claimed twice");
            }
        }

        // Every dimension belongs to exactly one flow: none is orphaned, none is shared.
        assert_eq!(used.len(), AttemptDimension::COUNT);
    }

    #[test]
    fn the_shipped_limits_are_the_ones_the_contract_publishes() {
        // `evidence/abuse-control-matrix.md` §2 states ten limits, and an operator reads that table
        // to decide whether the defaults suit them. Nothing but this test connects the two: a
        // changed literal in `AbuseContract::default` would leave the document describing a
        // control that no longer exists, and a document nobody can trust is worse than none.
        //
        // The pairs below ARE the matrix. Changing one is a documentation change too.
        let contract = AbuseContract::default();
        let expected: [(AttemptDimension, u32, i64); 10] = [
            (AttemptDimension::LogInAccount, 20, 15),
            (AttemptDimension::LogInNetwork, 100, 15),
            (AttemptDimension::VerificationResendAccount, 5, 60),
            (AttemptDimension::VerificationResendNetwork, 30, 60),
            (AttemptDimension::VerificationCompleteNetwork, 60, 15),
            (AttemptDimension::ForgotPasswordAccount, 5, 60),
            (AttemptDimension::ForgotPasswordNetwork, 30, 60),
            (AttemptDimension::ResetPasswordNetwork, 60, 15),
            (AttemptDimension::TokenRefreshClient, 60, 15),
            (AttemptDimension::TokenRefreshNetwork, 300, 15),
        ];
        for (dimension, limit, minutes) in expected {
            let shipped = contract.get(dimension);
            assert_eq!(shipped.limit(), limit, "{dimension:?} limit");
            assert_eq!(
                shipped.window(),
                Duration::seconds(minutes * 60),
                "{dimension:?} window"
            );
        }
        assert_eq!(expected.len(), AttemptDimension::COUNT);

        // THE AMPLIFIER RULE, asserted rather than described: resend and forgot-password are
        // interchangeable to an attacker, so different limits would just mean using the looser one.
        assert_eq!(
            contract.get(AttemptDimension::VerificationResendAccount),
            contract.get(AttemptDimension::ForgotPasswordAccount)
        );
        assert_eq!(
            contract.get(AttemptDimension::VerificationResendNetwork),
            contract.get(AttemptDimension::ForgotPasswordNetwork)
        );

        // POSITIVE CONTROL: `with` really does change a limit, so the equalities above are about
        // the shipped defaults rather than about a getter that always returns the same value.
        let widened = contract.with(
            AttemptDimension::LogInAccount,
            AttemptLimit::new(999, Duration::seconds(900)).expect("legal"),
        );
        assert_eq!(widened.get(AttemptDimension::LogInAccount).limit(), 999);
        assert_eq!(contract.get(AttemptDimension::LogInAccount).limit(), 20);
    }

    // ---- SQ-4: the key space is bounded --------------------------------------------------------

    #[test]
    fn a_hundred_thousand_distinct_identifiers_cannot_create_more_rows_than_there_are_buckets() {
        // THE ATTACK, executed. An attacker submits forgot-password requests for arbitrary
        // addresses; each is a fresh, never-before-seen identifier.
        //
        // Under the design this replaced — one row per (dimension, digest, window) — this loop
        // would produce 100_000 distinct keys. Under bucketing it cannot exceed `buckets`.
        const BUCKETS: u32 = 256;
        let ring = keyring(BUCKETS);
        let mut seen = BTreeSet::new();
        for n in 0..100_000_u32 {
            let identifier = format!("attacker-{n}@example.test");
            let bucket = ring
                .bucket(
                    AttemptDimension::ForgotPasswordAccount,
                    AttemptKey::Account(&identifier),
                )
                .expect("the axes match");
            assert!(bucket.get() < BUCKETS, "a bucket escaped its space");
            seen.insert(bucket.get());
        }
        assert!(
            seen.len() <= BUCKETS as usize,
            "100_000 identifiers produced {} distinct keys",
            seen.len()
        );

        // POSITIVE CONTROL: the mapping is not collapsing everything to one bucket, which would
        // also satisfy the bound and would be useless. With 100_000 identifiers over 256 buckets,
        // every bucket should be reached.
        assert_eq!(seen.len(), BUCKETS as usize, "the mapping is not spreading");
    }

    #[test]
    fn the_row_bound_is_the_stated_formula() {
        let default = AttemptBuckets::default();
        assert_eq!(default.get(), 65_536);
        assert_eq!(default.max_rows(), 10 * 65_536);
        assert_eq!(default.max_rows(), 655_360);

        // The formula, not the constant: it tracks both factors.
        for count in [256_u32, 1024, 65_536, 1_048_576] {
            let buckets = AttemptBuckets::new(count).expect("a legal count");
            assert_eq!(
                buckets.max_rows(),
                (AttemptDimension::COUNT as u64) * u64::from(count)
            );
        }
    }

    #[test]
    fn a_bucket_count_that_would_bias_the_mapping_is_refused() {
        // A power of two, so the reduction is a MASK. A modulo over a non-power-of-two would pull
        // extra identifiers into the low buckets, and an attacker who knew that would aim there.
        for refused in [0_u32, 1, 255, 257, 1000, 100_000, 1_048_577, u32::MAX] {
            assert_eq!(
                AttemptBuckets::new(refused),
                Err(AuthError::PolicyMisconfigured),
                "accepted {refused}"
            );
        }
        // POSITIVE CONTROL: the legal ones are accepted.
        for accepted in [256_u32, 512, 65_536, 1_048_576] {
            assert!(AttemptBuckets::new(accepted).is_ok(), "refused {accepted}");
        }
    }

    // ---- the mapping itself --------------------------------------------------------------------

    #[test]
    fn one_identifier_lands_in_a_different_bucket_in_every_dimension() {
        // DOMAIN SEPARATION. Without the tag, "ada@example.test" would occupy the same bucket
        // INDEX in every dimension, so filling a login bucket would fill the forgot-password
        // bucket for the same address — one attack for the price of none.
        //
        // The key is fixed, so these are computed facts, not samples.
        let ring = keyring(65_536);
        let account_dimensions = [
            AttemptDimension::LogInAccount,
            AttemptDimension::VerificationResendAccount,
            AttemptDimension::ForgotPasswordAccount,
        ];
        let buckets: BTreeSet<u32> = account_dimensions
            .iter()
            .map(|d| {
                ring.bucket(*d, AttemptKey::Account("ada@example.test"))
                    .expect("the axes match")
                    .get()
            })
            .collect();
        assert_eq!(
            buckets.len(),
            account_dimensions.len(),
            "two dimensions share a bucket index for one identifier"
        );
    }

    #[test]
    fn case_and_surrounding_space_do_not_buy_a_second_counter() {
        // Otherwise the control is bypassed by holding shift.
        let ring = keyring(65_536);
        let canonical = ring
            .bucket(
                AttemptDimension::LogInAccount,
                AttemptKey::Account("ada@example.test"),
            )
            .expect("the axes match");
        for variant in [
            "ADA@EXAMPLE.TEST",
            "Ada@Example.Test",
            "  ada@example.test  ",
            "\tada@example.test\n",
        ] {
            assert_eq!(
                ring.bucket(AttemptDimension::LogInAccount, AttemptKey::Account(variant))
                    .expect("the axes match"),
                canonical,
                "{variant:?} got its own counter"
            );
        }

        // POSITIVE CONTROL: a genuinely different address does NOT share the counter, so the
        // equalities above are about normalisation rather than about everything colliding.
        assert_ne!(
            ring.bucket(
                AttemptDimension::LogInAccount,
                AttemptKey::Account("grace@example.test")
            )
            .expect("the axes match"),
            canonical
        );
    }

    #[test]
    fn a_v4_mapped_v6_address_is_the_same_counter_as_its_v4_form() {
        // An attacker who could switch representations would otherwise hold two buckets for one
        // address, and half their attempts would be free.
        let ring = keyring(65_536);
        let v4 = ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5)));
        let mapped = ClientIdentity::DirectPeer(IpAddr::V6(Ipv6Addr::new(
            0, 0, 0, 0, 0, 0xffff, 0xcb00, 0x7105,
        )));
        assert_eq!(
            ring.bucket(AttemptDimension::LogInNetwork, AttemptKey::Network(v4)),
            ring.bucket(AttemptDimension::LogInNetwork, AttemptKey::Network(mapped)),
        );

        // POSITIVE CONTROL: a real v6 address is its own counter.
        let genuine =
            ClientIdentity::DirectPeer(IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1)));
        assert_ne!(
            ring.bucket(AttemptDimension::LogInNetwork, AttemptKey::Network(genuine))
                .expect("the axes match"),
            ring.bucket(AttemptDimension::LogInNetwork, AttemptKey::Network(v4))
                .expect("the axes match"),
        );
    }

    #[test]
    fn an_entire_ipv6_prefix_still_cannot_grow_the_key_space() {
        // The network axis is unbounded for the SAME reason the account axis is, and it does not
        // need an email to be: a routine /64 gives an attacker 2^64 source addresses.
        const BUCKETS: u32 = 256;
        let ring = keyring(BUCKETS);
        let mut seen = BTreeSet::new();
        for n in 0..50_000_u32 {
            let address = ClientIdentity::DirectPeer(IpAddr::V6(Ipv6Addr::new(
                0x2001,
                0xdb8,
                0,
                0,
                (n >> 16) as u16,
                (n & 0xffff) as u16,
                0,
                1,
            )));
            seen.insert(
                ring.bucket(AttemptDimension::LogInNetwork, AttemptKey::Network(address))
                    .expect("the axes match")
                    .get(),
            );
        }
        assert!(seen.len() <= BUCKETS as usize);
        assert_eq!(seen.len(), BUCKETS as usize, "the mapping is not spreading");
    }

    #[test]
    fn counting_an_address_as_an_account_is_refused_rather_than_mis_attributed() {
        let ring = keyring(65_536);
        assert_eq!(
            ring.bucket(
                AttemptDimension::LogInAccount,
                AttemptKey::Network(network(1))
            ),
            Err(AuthError::PolicyMisconfigured)
        );
        assert_eq!(
            ring.bucket(
                AttemptDimension::LogInNetwork,
                AttemptKey::Account("ada@example.test")
            ),
            Err(AuthError::PolicyMisconfigured)
        );
        assert_eq!(
            ring.bucket(
                AttemptDimension::TokenRefreshClient,
                AttemptKey::Network(network(1))
            ),
            Err(AuthError::PolicyMisconfigured)
        );

        // POSITIVE CONTROL: matching axes are accepted.
        assert!(
            ring.bucket(
                AttemptDimension::LogInNetwork,
                AttemptKey::Network(network(1))
            )
            .is_ok()
        );
    }

    #[test]
    fn the_keyring_never_renders_its_key() {
        let rendered = format!("{:?}", keyring(65_536));
        assert!(rendered.contains("[redacted]"));
        assert!(
            !rendered.contains("165"),
            "a key byte reached the rendering"
        );
        assert!(!rendered.contains("a5"), "a key byte reached the rendering");
    }

    // ---- windows, limits, and saturation --------------------------------------------------------

    #[test]
    fn window_boundaries_are_anchored_at_the_epoch_so_every_process_agrees() {
        let limit = AttemptLimit::new(10, Duration::seconds(900)).expect("legal");
        // 00:07 and 00:14 are the same 15-minute window; 00:15 is the next.
        assert_eq!(limit.window_start(at(7)), limit.window_start(at(14)));
        assert_ne!(limit.window_start(at(14)), limit.window_start(at(15)));
        assert_eq!(limit.window_start(at(15)), at(15));

        // Before the epoch the division must FLOOR, not truncate toward zero — otherwise instants
        // just before 1970 land in the window after themselves.
        let before = DateTime::from_timestamp(-1, 0).expect("a real instant");
        assert!(limit.window_start(before) <= before);
        assert_eq!(limit.window_start(before).timestamp(), -900);
    }

    #[test]
    fn a_limit_outside_the_permitted_shape_is_refused() {
        for (count, window) in [
            (0_u32, Duration::seconds(900)),       // no limit at all
            (10, Duration::seconds(0)),            // no window
            (10, Duration::seconds(-900)),         // a negative window
            (10, Duration::seconds(86_401)),       // longer than a day
            (10, Duration::milliseconds(900_500)), // not a whole number of seconds
        ] {
            assert_eq!(
                AttemptLimit::new(count, window),
                Err(AuthError::PolicyMisconfigured),
                "accepted {count}/{window}"
            );
        }
        assert!(AttemptLimit::new(1, Duration::seconds(1)).is_ok());
        assert!(AttemptLimit::new(u32::MAX, Duration::seconds(86_400)).is_ok());
    }

    #[test]
    fn the_limit_permits_exactly_its_count_and_refuses_the_next() {
        let limit = AttemptLimit::new(3, Duration::seconds(900)).expect("legal");
        let start = limit.window_start(at(0));
        for (current, over) in [(1_u64, false), (2, false), (3, false), (4, true)] {
            let state = AttemptState {
                current,
                previous: 0,
                window_start: start,
            };
            assert_eq!(
                limit.exceeded_by(&state, start),
                over,
                "current={current} decided wrongly"
            );
        }
    }

    #[test]
    fn the_previous_window_is_charged_so_a_boundary_burst_is_not_free() {
        // WITHOUT the weighting, `current` alone decides and this state is under the limit — which
        // is exactly the 2x boundary burst a fixed window admits.
        let limit = AttemptLimit::new(10, Duration::seconds(900)).expect("legal");
        let start = limit.window_start(at(15));

        // One second into the new window, carrying a full previous window: essentially all of the
        // previous count is still charged.
        let state = AttemptState {
            current: 3,
            previous: 10,
            window_start: start,
        };
        assert!(limit.exceeded_by(&state, start + Duration::seconds(1)));

        // POSITIVE CONTROL: near the END of the window the previous count has decayed away, so the
        // same counters are admitted. That is the decay doing work rather than a constant refusal.
        assert!(!limit.exceeded_by(&state, start + Duration::seconds(899)));
    }

    #[test]
    fn a_saturated_counter_stays_refused_and_never_wraps() {
        let limit = AttemptLimit::new(10, Duration::seconds(900)).expect("legal");
        let start = limit.window_start(at(0));
        let state = AttemptState {
            current: AttemptState::CEILING,
            previous: AttemptState::CEILING,
            window_start: start,
        };
        assert!(limit.exceeded_by(&state, start));

        // The ceiling is i64::MAX, because the column is BIGINT. A value that could not round-trip
        // through the column must never be written.
        assert_eq!(AttemptState::CEILING, i64::MAX as u64);
        assert!(i64::try_from(AttemptState::CEILING).is_ok());
        assert!(
            AttemptState::CEILING.checked_add(1).is_some(),
            "not at u64 max"
        );
    }

    // ---- the guard ------------------------------------------------------------------------------

    #[tokio::test]
    async fn every_dimension_is_counted_before_any_refusal_is_returned() {
        // NO EARLY RETURN. A short-circuit would let the refusal disclose which axis tripped by
        // which counters moved, and would give an attacker who has tripped one axis free requests
        // against the others.
        let contract = AbuseContract::default()
            .with(
                AttemptDimension::LogInAccount,
                AttemptLimit::new(1, Duration::seconds(900)).expect("legal"),
            )
            .with(
                AttemptDimension::LogInNetwork,
                AttemptLimit::new(1000, Duration::seconds(900)).expect("legal"),
            );
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            contract,
            RecordingAuditSink::new(),
        );
        let keys = FlowKeys {
            account: Some("ada@example.test"),
            client: None,
            network: network(1),
        };

        assert!(
            guard
                .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
                .await
                .is_ok()
        );
        // The second trips the account axis and NOT the network one.
        assert!(matches!(
            guard
                .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
                .await,
            Err(ServiceError::Refused(AuthError::TooManyAttempts))
        ));

        // Both dimensions were observed on BOTH calls — four observations, not three.
        let seen = guard.repository.seen();
        assert_eq!(seen.len(), 4, "a refusal skipped a dimension: {seen:?}");
        assert_eq!(
            seen.iter()
                .filter(|d| **d == AttemptDimension::LogInNetwork)
                .count(),
            2,
            "the network axis was not charged for the refused attempt"
        );
    }

    #[tokio::test]
    async fn success_never_clears_a_counter() {
        // FR-068. If success cleared the network counter, an attacker filling it would interleave
        // one login to an account they own and it would never fill.
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        );
        let keys = FlowKeys {
            account: Some("ada@example.test"),
            client: None,
            network: network(1),
        };
        for _ in 0..5 {
            // `let _` rather than a bare call: `Admitted` is `#[must_use]` precisely so an
            // admission cannot be earned and forgotten. Here the admission is not the subject —
            // what is stored is.
            let _admission = guard
                .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
                .await
                .expect("under the limit");
        }
        // Five successful admissions, and the counter reads five.
        let rows = guard.repository.rows.lock().expect("unpoisoned");
        let account = rows
            .values()
            .map(|state| state.current)
            .max()
            .expect("a row exists");
        assert_eq!(account, 5, "a successful attempt cleared evidence");
    }

    #[tokio::test]
    async fn a_store_failure_fails_closed_and_does_not_admit() {
        // No in-memory fallback and no permissive path: a rate limiter that stops limiting when
        // its store is unavailable is an availability-triggered authentication bypass.
        let guard = AbuseGuard::new(
            FakeAttempts::failing(),
            keyring(65_536),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        );
        let outcome = guard
            .admit(
                AttemptFlow::LogIn,
                FlowKeys {
                    account: Some("ada@example.test"),
                    client: None,
                    network: network(1),
                },
                correlation(),
                at(0),
            )
            .await;
        assert!(
            matches!(outcome, Err(ServiceError::Storage(_))),
            "expected a storage refusal, got {outcome:?}"
        );

        // POSITIVE CONTROL: the same call against a working store is admitted, so the refusal
        // above is about the store rather than about the arguments.
        let working = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        );
        assert!(
            working
                .admit(
                    AttemptFlow::LogIn,
                    FlowKeys {
                        account: Some("ada@example.test"),
                        client: None,
                        network: network(1)
                    },
                    correlation(),
                    at(0),
                )
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn a_known_and_an_unknown_account_are_indistinguishable_to_the_control() {
        // Nothing in this module knows whether an account exists, and that is the point: there is
        // no lookup, so there is no difference to observe. Both identifiers travel the same path,
        // create the same kind of row, and produce the same refusal at the same count.
        let contract = AbuseContract::default().with(
            AttemptDimension::ForgotPasswordAccount,
            AttemptLimit::new(2, Duration::seconds(3600)).expect("legal"),
        );

        let mut refusals = Vec::new();
        for identifier in ["ada@example.test", "nobody-at-all@example.test"] {
            let guard = AbuseGuard::new(
                FakeAttempts::default(),
                keyring(65_536),
                contract,
                RecordingAuditSink::new(),
            );
            let keys = FlowKeys {
                account: Some(identifier),
                client: None,
                network: network(1),
            };
            let mut outcomes = Vec::new();
            for _ in 0..4 {
                outcomes.push(
                    guard
                        .admit(AttemptFlow::ForgotPassword, keys, correlation(), at(0))
                        .await
                        .map_err(|error| format!("{error}")),
                );
            }
            refusals.push(outcomes);
        }
        assert_eq!(
            refusals[0], refusals[1],
            "a known and an unknown account behaved differently"
        );
        // And the shape is the one the contract promises: two admitted, then refused.
        assert!(refusals[0][0].is_ok() && refusals[0][1].is_ok());
        assert!(refusals[0][2].is_err() && refusals[0][3].is_err());
    }

    #[tokio::test]
    async fn a_flow_missing_a_required_key_is_refused_rather_than_counted_on_a_default() {
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        );
        // Token refresh needs a client key; without one there is no safe substitute.
        let outcome = guard
            .admit(
                AttemptFlow::TokenRefresh,
                FlowKeys {
                    account: None,
                    client: None,
                    network: network(1),
                },
                correlation(),
                at(0),
            )
            .await;
        assert!(matches!(
            outcome,
            Err(ServiceError::Refused(AuthError::PolicyMisconfigured))
        ));

        // POSITIVE CONTROL: supplying the key admits it.
        assert!(
            guard
                .admit(
                    AttemptFlow::TokenRefresh,
                    FlowKeys {
                        account: None,
                        client: Some([3; 16]),
                        network: network(1)
                    },
                    correlation(),
                    at(0),
                )
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn the_refusal_names_neither_the_dimension_nor_the_flow() {
        // FR-070, asserted on the VALUE, its Display and its Debug — the three places a name could
        // reach a caller.
        let contract = AbuseContract::default().with(
            AttemptDimension::LogInAccount,
            AttemptLimit::new(1, Duration::seconds(900)).expect("legal"),
        );
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            contract,
            RecordingAuditSink::new(),
        );
        let keys = FlowKeys {
            account: Some("ada@example.test"),
            client: None,
            network: network(1),
        };
        let _admission = guard
            .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
            .await
            .expect("first");
        let error = guard
            .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
            .await
            .expect_err("second");

        let rendered = format!("{error} {error:?}");
        for forbidden in [
            "LogInAccount",
            "LogInNetwork",
            "account",
            "network",
            "dimension",
            "bucket",
            "login",
            "ada@example.test",
            "203.0.113",
        ] {
            assert!(
                !rendered.contains(forbidden),
                "the refusal disclosed {forbidden:?}: {rendered}"
            );
        }
        assert!(matches!(
            error,
            ServiceError::Refused(AuthError::TooManyAttempts)
        ));
    }

    #[tokio::test]
    async fn every_attempt_is_audited_on_both_the_admitted_and_the_refused_path() {
        // `evidence/abuse-control-matrix.md` §6. Emitting only on refusal would make the PRESENCE
        // of a record the oracle the refusal itself is not; emitting only on success would make
        // the trail an account of what worked.
        let sink = RecordingAuditSink::new();
        let contract = AbuseContract::default().with(
            AttemptDimension::LogInAccount,
            AttemptLimit::new(1, Duration::seconds(900)).expect("legal"),
        );
        let guard = AbuseGuard::new(FakeAttempts::default(), keyring(65_536), contract, sink);
        let keys = FlowKeys {
            account: Some("ada@example.test"),
            client: None,
            network: network(1),
        };

        let _admission = guard
            .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
            .await
            .expect("the first attempt is admitted");
        let refused = guard
            .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
            .await;
        assert!(refused.is_err());

        let events = guard.audit.events();
        assert_eq!(events.len(), 2, "one record per attempt, on both paths");
        assert_eq!(events[0].action(), AuditAction::LogIn);
        assert_eq!(events[0].outcome(), AuditOutcome::Permitted);
        assert_eq!(events[1].action(), AuditAction::LogIn);
        assert_eq!(events[1].outcome(), AuditOutcome::Refused);

        // NEITHER record names the account. Putting it in the trail would answer "does this
        // account exist" for anyone who can read it, which is the question the refusal refuses.
        for event in &events {
            let rendered = format!("{event:?}");
            assert!(!rendered.contains("ada@example.test"));
            assert!(!rendered.contains("203.0.113"));
        }

        // Every one of the six flows maps to its own action, so the trail distinguishes them.
        let mut actions: Vec<AuditAction> =
            AttemptFlow::ALL.iter().map(|f| f.audit_action()).collect();
        let count = actions.len();
        actions.sort_unstable();
        actions.dedup();
        assert_eq!(actions.len(), count, "two flows share an audit action");
    }

    #[tokio::test]
    async fn an_audit_sink_failure_refuses_the_attempt_identically_on_both_paths() {
        // FAIL CLOSED, and not an oracle. A sink outage must not turn "admitted" and "refused"
        // into two distinguishable failures, or the sink's health becomes the answer.
        let contract = AbuseContract::default().with(
            AttemptDimension::LogInAccount,
            AttemptLimit::new(1, Duration::seconds(900)).expect("legal"),
        );
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            contract,
            RecordingAuditSink::failing(),
        );
        let keys = FlowKeys {
            account: Some("ada@example.test"),
            client: None,
            network: network(1),
        };

        // The first attempt WOULD have been admitted; the second WOULD have been refused.
        let would_admit = guard
            .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
            .await;
        let would_refuse = guard
            .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
            .await;

        assert!(matches!(
            would_admit,
            Err(ServiceError::Refused(AuthError::TooManyAttempts))
        ));
        assert_eq!(
            format!("{:?}", would_admit.err()),
            format!("{:?}", would_refuse.err()),
            "a sink outage told the caller which decision had been made"
        );

        // POSITIVE CONTROL: with a working sink the first attempt IS admitted, so the refusal
        // above is about the sink rather than about the limit.
        let working = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            contract,
            RecordingAuditSink::new(),
        );
        assert!(
            working
                .admit(AttemptFlow::LogIn, keys, correlation(), at(0))
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn an_admission_for_one_flow_does_not_open_another() {
        // The capability names its flow, so a cheap admission cannot be spent on an expensive one.
        // Without this, `Admitted` would only prove that SOME control ran — and a caller could
        // earn one on a 300-per-window refresh limit and spend it on a 20-per-window login.
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        );
        let admission = guard
            .admit(
                AttemptFlow::ForgotPassword,
                FlowKeys {
                    account: Some("ada@example.test"),
                    client: None,
                    network: network(1),
                },
                correlation(),
                at(0),
            )
            .await
            .expect("admitted for forgot-password");

        assert_eq!(admission.flow(), AttemptFlow::ForgotPassword);
        for other in AttemptFlow::ALL {
            let outcome = admission.expect(other);
            if other == AttemptFlow::ForgotPassword {
                assert!(outcome.is_ok(), "the flow it was earned on was refused");
            } else {
                assert_eq!(
                    outcome,
                    Err(AuthError::PolicyMisconfigured),
                    "a forgot-password admission opened {other:?}"
                );
            }
        }
    }

    #[tokio::test]
    async fn a_backwards_clock_is_refused_without_erasing_what_is_stored() {
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(65_536),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        );
        let keys = FlowKeys {
            account: None,
            client: None,
            network: network(1),
        };
        let _admission = guard
            .admit(AttemptFlow::ResetPassword, keys, correlation(), at(60))
            .await
            .expect("the first attempt is admitted");
        let before = guard.repository.rows.lock().expect("unpoisoned").clone();

        // The clock moves an hour backwards.
        let outcome = guard
            .admit(AttemptFlow::ResetPassword, keys, correlation(), at(0))
            .await;
        assert!(matches!(
            outcome,
            Err(ServiceError::Refused(AuthError::TooManyAttempts))
        ));
        assert_eq!(
            *guard.repository.rows.lock().expect("unpoisoned"),
            before,
            "a backwards clock rewrote the row"
        );
    }

    #[tokio::test]
    async fn the_row_count_never_exceeds_the_bound_however_many_identifiers_arrive() {
        // The bound, end to end through the guard rather than through the mapping alone — and with
        // NO pruning, which is the property that separates this design from the one it replaced.
        const BUCKETS: u32 = 256;
        let guard = AbuseGuard::new(
            FakeAttempts::default(),
            keyring(BUCKETS),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        );
        for n in 0..20_000_u32 {
            let identifier = format!("attacker-{n}@example.test");
            let _ = guard
                .admit(
                    AttemptFlow::ForgotPassword,
                    FlowKeys {
                        account: Some(&identifier),
                        client: None,
                        network: network((n % 251) as u8),
                    },
                    correlation(),
                    at(0),
                )
                .await;
        }
        // Two dimensions for this flow, so at most 2 x 256 rows — from 20_000 distinct
        // identifiers and 251 distinct addresses.
        assert!(
            guard.repository.rows() <= 2 * BUCKETS as usize,
            "{} rows from 20_000 identifiers",
            guard.repository.rows()
        );
    }
}
