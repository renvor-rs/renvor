//! Mail for Renvor: a narrow port whose messages are bounded and header-injection-free by
//! construction, a recording substitute, an SMTP submission adapter behind `smtp`, and a bridge
//! to the authentication mail port behind `auth`.
//!
//! **Pre-release. Nothing here is published and no API is stable.**
//!
//! # Where to start
//!
//! - [`port`] — [`Mailer`], [`Message`], [`Address`], the closed [`MailError`].
//! - [`recording`] — [`RecordingMailbox`], the deterministic substitute.
//! - [`provider`] — [`MailProvider`], which verifies the transport at Boot.
//! - `smtp` (feature) — `SmtpMailer` over `lettre`, TLS by default.
//! - `auth` (feature) — `AuthMailBridge`, `renvor_auth::MailPort` for any mailer.
//!
//! # Sending is not idempotent
//!
//! The port makes no retry. An application that needs at-least-once delivery enqueues a durable
//! job with an idempotency key and sends from the handler (ADR-0034, ADR-0037).

#![forbid(unsafe_code)]

pub mod port;
pub mod provider;
pub mod recording;

#[cfg(feature = "auth")]
pub mod auth;
#[cfg(feature = "smtp")]
pub mod smtp;

pub use port::{
    Address, MAX_ADDRESS_OCTETS, MAX_BODY_BYTES, MAX_RECIPIENTS, MAX_SUBJECT_BYTES, MailError,
    MailMetrics, MailRefusal, Mailbox, Mailer, Message, MessageBuilder, MessageId, Receipt,
};
pub use provider::{MAIL_CAPABILITY, MailBootError, MailProvider, mail_capability};
pub use recording::RecordingMailbox;
