//! Stable public API error codes and RFC 9457 Problem Details for the Renvor framework.
//!
//! # This crate names no transport
//!
//! A public API error code is a compatibility promise, and a promise that lives in the HTTP
//! adapter is a promise about HTTP. The dependency list is the assertion: `serde` and
//! `serde_json`, and nothing else. There is no status-code type here, no header type, and no
//! router. Mapping a code to an HTTP status is `renvor-http`'s job, because a status code **is**
//! transport semantics.
//!
//! # Redaction is a property of the types
//!
//! Three guarantees hold by construction rather than by review:
//!
//! - [`code::ApiErrorCode::detail`] returns `&'static str`, so no runtime value can inhabit it.
//! - [`problem::InvalidParam`] has **no field** a rejected value could occupy.
//! - [`problem::InvalidParam::reason`] is `&'static str`, so a third-party validator's message —
//!   which quotes the offending input — cannot be stored in it.
//!
//! The third exists because a real validator produced `"not an object" is not of type "object"`
//! during this phase's tooling work. Any design that rendered a validator's `Display` into a
//! response would have leaked the rejected value with it.
//!
//! # Example
//!
//! ```
//! use renvor_error::{ApiErrorCode, InvalidParam, Location, Pointer, ProblemDetails};
//!
//! let problem = ProblemDetails::new(ApiErrorCode::ValidationFailed, 400, "0123456789abcdef")
//!     .with_invalid_params(vec![InvalidParam {
//!         location: Location::Body,
//!         pointer: Pointer::new("/quantity")?,
//!         reason: "out_of_range",
//!     }]);
//!
//! let json = problem.to_json()?;
//! assert!(json.contains(r#""code":"validation_failed""#));
//! assert!(json.contains(r#""status":400"#));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Stability
//!
//! This surface is **explicitly unstable**. See
//! [`contracts/api-stability.md`](https://github.com/renvor-rs/renvor/blob/main/contracts/api-stability.md).

pub mod code;
pub mod mapping;
pub mod problem;

pub use code::{API_ERROR_REGISTRY_VERSION, ApiErrorCode, PROBLEM_TYPE_BASE};
pub use mapping::{CLI_ERROR_CODES, is_api_error_code, is_cli_error_code};
pub use problem::{
    InvalidParam, Location, MAX_POINTER_BYTES, PROBLEM_MEDIA_TYPE, Pointer, PointerError,
    ProblemDetails,
};
