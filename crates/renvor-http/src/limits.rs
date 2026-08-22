//! The documented resource bounds, in one place.
//!
//! # One declaration site
//!
//! Every value contract C-10 publishes is a constant here and nowhere else. A limit that also
//! appeared as a literal at its use site would be two numbers that can disagree, and the one that
//! disagrees is the one nobody reads.
//!
//! # What Renvor does NOT bound, stated rather than implied
//!
//! **Renvor sets no header-count or header-size limit.** The underlying HTTP implementation applies
//! its own, and the server entry point Renvor uses exposes no control over them — so Renvor makes
//! no promise about their values.
//!
//! This is written here, beside the bounds that *are* Renvor's, because a module listing only the
//! limits that exist invites a reader to conclude the list is exhaustive. Constitution principle
//! XII requires the limitation to be visible.

use core::time::Duration;

/// The default maximum request body, in bytes: **2 MiB**.
///
/// This is the underlying stack's own default, matched deliberately. Publishing a different number
/// would mean Renvor's documented bound and the bound that actually fires were two different
/// values, and only one of them would be true.
///
/// **The boundary is inclusive**: a body of exactly this many bytes is accepted; one byte more is
/// refused with `413`. Stated because the opposite is the more common assumption.
pub const MAX_BODY_BYTES: usize = 2_097_152;

/// The default maximum number of simultaneously admitted requests.
pub const MAX_CONCURRENT_REQUESTS: usize = 1024;

/// The default per-request timeout. Exceeding it produces `408` and cancels the request scope.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// The maximum number of routes one registry may hold.
pub const MAX_ROUTES: usize = 4096;

/// The status returned when the body limit is exceeded.
pub const STATUS_PAYLOAD_TOO_LARGE: u16 = 413;

/// The status returned when a request exceeds [`REQUEST_TIMEOUT`].
pub const STATUS_REQUEST_TIMEOUT: u16 = 408;

/// The status returned when the application is draining or saturated.
pub const STATUS_UNAVAILABLE: u16 = 503;

/// The bounds a server runs with.
///
/// Every field defaults to the published constant above, so an author who configures nothing gets
/// exactly the documented contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Maximum request body in bytes. Inclusive boundary.
    pub max_body_bytes: usize,
    /// Maximum simultaneously admitted requests.
    pub max_concurrent_requests: usize,
    /// Per-request timeout.
    pub request_timeout: Duration,
    /// How long a drain waits for in-flight work before reporting it outstanding.
    pub drain_budget: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_body_bytes: MAX_BODY_BYTES,
            max_concurrent_requests: MAX_CONCURRENT_REQUESTS,
            request_timeout: REQUEST_TIMEOUT,
            // The KERNEL's constant, reused rather than restated. A transport with its own
            // different default would be a second number that can disagree with the first.
            drain_budget: renvor_core::lifecycle::DEFAULT_DRAIN_BUDGET,
        }
    }
}

impl Limits {
    /// The documented defaults.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Limits, MAX_BODY_BYTES, MAX_CONCURRENT_REQUESTS, MAX_ROUTES, REQUEST_TIMEOUT,
        STATUS_PAYLOAD_TOO_LARGE, STATUS_REQUEST_TIMEOUT,
    };
    use core::time::Duration;

    #[test]
    fn the_published_values_are_the_contract_values() {
        // Contract C-10's table, asserted rather than described. If a value changes here without
        // the contract changing, the document and the binary have drifted and this fails.
        assert_eq!(MAX_BODY_BYTES, 2_097_152, "2 MiB");
        assert_eq!(MAX_CONCURRENT_REQUESTS, 1024);
        assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(30));
        assert_eq!(MAX_ROUTES, 4096);
        assert_eq!(STATUS_PAYLOAD_TOO_LARGE, 413);
        assert_eq!(STATUS_REQUEST_TIMEOUT, 408);
    }

    #[test]
    fn the_drain_budget_is_the_kernels_own_constant() {
        // Not merely "30 seconds": the SAME constant. A literal here would be a second declaration
        // that could drift from the kernel's if either moved.
        assert_eq!(
            Limits::default().drain_budget,
            renvor_core::lifecycle::DEFAULT_DRAIN_BUDGET
        );
    }

    #[test]
    fn defaults_are_the_documented_contract() {
        let limits = Limits::new();
        assert_eq!(limits.max_body_bytes, MAX_BODY_BYTES);
        assert_eq!(limits.max_concurrent_requests, MAX_CONCURRENT_REQUESTS);
        assert_eq!(limits.request_timeout, REQUEST_TIMEOUT);
    }
}
