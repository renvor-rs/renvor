//! HTTP routes, Problem Details, and OpenAPI security schemes for Renvor's authentication flows.
//!
//! # Why this crate exists
//!
//! `renvor-auth` names no transport. `renvor-http` names no domain. Something has to join them,
//! and `plan.md` §1 originally said that something was `renvor-http` itself.
//!
//! **The gate refuted it** — which is the outcome that paragraph anticipated, in the sentence right
//! after it. `renvor-auth` depends on `renvor-config` for `Secret<T>`, and verification step 7
//! forbids `renvor-http` from resolving `renvor-config`: *"the transport depends inward only"*.
//!
//! So this crate stands to the transport exactly as `renvor-sqlx` and `renvor-seaorm` stand to the
//! persistence ports — an adapter that depends on both sides and that neither side depends on.
//! The three rejected alternatives (an optional feature that hides from the gate, logic in the
//! pure-`pub use` facade, and moving `Secret<T>` into the kernel that exists precisely not to hold
//! one) are recorded in `specs/009-.../evidence/batch-j-placement.md`.
//!
//! # What it guarantees
//!
//! | | |
//! |---|---|
//! | **Six flows are bounded** | not by convention: each operation takes an `Admitted`, and only `AbuseGuard::admit` makes one |
//! | **No failure leaks** | every error path goes through [`problem`], which chooses a code from the variant alone |
//! | **No response carries a credential** | except the two a rotation must hand over, in the one type named for it |
//! | **The document describes what is served** | [`openapi::security_schemes`] declares two schemes, and the bearer one only under `tokens` |
//!
//! # What it does not do
//!
//! It implements no authentication logic. Every decision — whether a password matches, whether a
//! session is live, whether a policy permits — belongs to `renvor-auth`, and this crate would be
//! the wrong place to have a second opinion.
//!
//! It also parses no forwarding header. The `ClientIdentity` the abuse controls count arrives
//! already resolved on `RequestContext`, from the Phase 004 layer that knows which peers are
//! trusted (FR-065).
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** See
//! [`contracts/api-stability.md`](https://github.com/renvor-rs/renvor/blob/main/contracts/api-stability.md).

pub mod dto;
pub mod openapi;
pub mod problem;
pub mod routes;

pub use openapi::{SESSION_SCHEME, no_credential_required, security_schemes, session_required};
pub use problem::{PROBLEM_MEDIA_TYPE, classify, classify_refusal, render};
pub use routes::{AuthEndpoints, DeferredEndpoints, SharedEndpoints, routes, routes_deferred};

#[cfg(feature = "tokens")]
pub use openapi::{BEARER_SCHEME, bearer_required};
#[cfg(feature = "tokens")]
pub use routes::{TokenEndpoints, token_routes};
