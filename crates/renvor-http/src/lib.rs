//! REST and HTTP delivery adapter for Renvor — the framework's **first real transport**.
//!
//! # What this crate is for
//!
//! [`renvor_core`] owns an application's lifecycle, provider graph, typed state, cancellation, and
//! health, and it has never spoken HTTP. This crate is the layer that lets it, and it is the layer
//! where every HTTP type stops.
//!
//! ```text
//!   renvor-http          axum · tower · tower-http live HERE and nowhere further in
//!        │
//!        ▼  RequestContext, CancelScope, TypedStateMap  -- no transport type crosses
//!   application services
//!        │
//!        ▼
//!   renvor-core          lifecycle, WorkGate, drain, run identity
//! ```
//!
//! # Security defaults, and why each one is what it is
//!
//! Every default here is deny-first, because each is the answer to a question an attacker would
//! otherwise get to answer:
//!
//! | Default | Consequence of the opposite |
//! |---|---|
//! | **No trusted proxies** | a caller reachable from anywhere chooses its own source address |
//! | **Forwarding headers ignored** unless the direct peer is trusted | the same, one step removed |
//! | **CORS denies** | any origin reads authenticated responses |
//! | **Host validation fails closed** | a host-header attack succeeds against an unconfigured app |
//! | **Request identifiers are always generated** | a caller picks the identifier its own request is audited under |
//!
//! The normative statements are `contracts/http-security.md`, `contracts/http-routing.md`, and
//! `contracts/http-runtime.md`.
//!
//! # Custom primitives, and the record that justifies them
//!
//! Five narrow behaviours are Renvor's own, and **only** because the maintained option was read at
//! the source level and found unfit — not to own the implementation. Each is confined to one
//! module so that retiring it is a deletion and a re-wire. The evaluation, the concrete
//! shortcomings, the ownership cost, and a checkable exit trigger per primitive are in
//! **ADR-0012**.
//!
//! The HTTP engine, the router, request parsing, the CORS protocol, the body-limit and timeout
//! mechanisms, the concurrency limiter, tracing, and serialisation are **all** the selected
//! packages'. Renvor writes none of them.
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** Breaking changes are permitted without a compatibility
//! procedure and no semantic-versioning promise applies while the instability window is open. The
//! two event-gated closure conditions are defined solely by the public API stability contract,
//! `contracts/api-stability.md`, in the
//! [Renvor repository](https://github.com/renvor-rs/renvor).
//! No phase number forms part of either condition.
//!
//! This crate satisfies the **first** of those two conditions — a real transport adapter now
//! exercises the kernel surface. It does **not** satisfy the second, and **does not close the
//! window**.

pub mod cors;
pub mod host;
pub mod identity;
pub mod limits;
pub mod request_id;
pub mod route;

mod context;
mod error;

pub use context::{ClientIdentity, RequestContext};
pub use cors::{CorsPolicy, CorsPolicyError};
pub use error::{HttpError, HttpErrorKind};
pub use host::HostPolicy;
pub use identity::TrustedProxies;
pub use limits::Limits;
pub use route::{Method, Request, Response, Route, RouteError, RouteGroup, RouteRegistry};
