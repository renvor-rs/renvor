//! Observability for Renvor: a JSON subscriber with central redaction, a Prometheus renderer over
//! the kernel's metrics port, liveness and readiness documents, health routes behind `http`, and
//! OTLP trace export behind `otel`.
//!
//! **Pre-release. Nothing here is published and no API is stable.**
//!
//! # This crate returns values; it installs nothing
//!
//! [`build`] returns a subscriber. The author installs it — through
//! `renvor_core::observe::try_init_global` for the process — and this crate never touches the
//! global default (C-O7, FR-067).
//!
//! # Where to start
//!
//! - [`subscriber`] — [`LogSettings`], [`build`], the closed [`ObservabilityError`].
//! - [`redaction`] — the denylist every emitted field passes through.
//! - [`json`] / [`text`] — Renvor's formatters, and why they are Renvor's.
//! - [`prometheus`] — text exposition over `renvor_core::observe::metrics::Snapshot`.
//! - [`health`] — liveness and readiness documents over a cloned `HealthState`.
//! - `http` (feature) — `/healthz` and `/readyz` as a `RouteGroup`.
//! - `otel` (feature) — a bounded OTLP/HTTP exporter and a `tracing` layer.

#![forbid(unsafe_code)]

pub mod health;
pub mod json;
pub mod prometheus;
pub mod redaction;
pub mod subscriber;
pub mod text;

#[cfg(feature = "http")]
pub mod http;

pub use redaction::{MAX_VALUE_BYTES, REDACTED, Redaction};
pub use subscriber::{
    DEFAULT_FILTER, FILTER_KEY, LogFormat, LogSettings, ObservabilityError, build,
};
