//! The mail **port**, and the deterministic sink that stands in for a transport.
//!
//! # Phase 009 ships a port; Phase 010 ships the adapter
//!
//! PLAN.md §20 gives operational mail/cache/jobs/observability adapters to **Phase 010**. This
//! module defines the narrow boundary and a recording sink for tests. Shipping an SMTP client here
//! would take Phase 010's decision without Phase 010's review — and, structurally, it could not
//! live here anyway: every candidate pulls an SMTP client and a TLS stack, and `xtask` step 7
//! asserts `renvor-auth` resolves no transport.
//!
//! # The message carries a token, and that is the whole difficulty
//!
//! A verification mail is *only* useful because it contains a secret. So the message type cannot
//! avoid holding one, and the guarantee has to be about what can be **done** with it:
//!
//! | Path | Behaviour |
//! |---|---|
//! | `Debug` | prints the kind. **Not the recipient, not the token** |
//! | `Display` | not implemented — there is no "render this mail" that could reach a log |
//! | `Serialize` | not implemented, so a message cannot be written to a queue or a log sink |
//! | reaching the token | [`OutgoingMail::token`], one conspicuous method |
//!
//! The recipient is redacted alongside the token. An address is personal data, and a mail sink that
//! logs recipients is a privacy leak even when it keeps the secret.
//!
//! # A link is built by the adapter, never here
//!
//! FR-054: the port exposes **template data**, not a rendered link. That is not tidiness — OWASP's
//! Forgot Password guidance is explicit that the reset URL must not be built from the request's
//! `Host` header, and a port that accepted a finished URL would make that impossible to enforce in
//! one place. The adapter composes the link from configuration it owns.

use core::fmt;

use crate::opaque::Opaque;

/// Which mail this is.
///
/// A closed set: one kind per purpose, matching [`crate::opaque::OpaqueKind`], so a verification
/// token cannot be delivered in a reset template.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[non_exhaustive]
pub enum MailKind {
    /// Confirm control of an email address.
    Verification,
    /// Begin a password reset.
    PasswordReset,
}

/// Why a delivery did not happen.
///
/// **Fieldless**, for the reason [`crate::AuthError`] is: an adapter's failure text is written by
/// somebody else's SMTP library and would otherwise travel wherever this error travels.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MailError {
    /// The mail could not be handed to a transport.
    #[error("the message could not be delivered")]
    Undeliverable,
}

/// A message the adapter should render and send.
#[derive(Clone)]
pub struct OutgoingMail {
    kind: MailKind,
    recipient: String,
    token: Opaque,
}

impl OutgoingMail {
    /// Builds a message.
    #[must_use]
    pub const fn new(kind: MailKind, recipient: String, token: Opaque) -> Self {
        Self {
            kind,
            recipient,
            token,
        }
    }

    /// Which mail this is.
    #[must_use]
    pub const fn kind(&self) -> MailKind {
        self.kind
    }

    /// Who it is for.
    ///
    /// Named plainly, and deliberately **not** shown by `Debug`: an adapter needs the address, and
    /// a log does not.
    #[must_use]
    pub fn recipient(&self) -> &str {
        &self.recipient
    }

    /// The token the template must embed.
    ///
    /// Conspicuous by name, like [`crate::opaque::Opaque::expose`], so every place a secret leaves
    /// this type is visible at its call site in review.
    #[must_use]
    pub const fn token(&self) -> &Opaque {
        &self.token
    }
}

/// Prints the kind. **Never the recipient and never the token.**
impl fmt::Debug for OutgoingMail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OutgoingMail")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// Hands a message to a transport.
pub trait MailPort: Send + Sync {
    /// Delivers `mail`.
    ///
    /// # Errors
    ///
    /// [`MailError::Undeliverable`]. **The caller must not turn this into a public response** —
    /// see [`crate::service::DispatchOutcome`] for why a delivery failure that reaches the
    /// requester is an account-enumeration oracle.
    fn deliver(
        &self,
        mail: OutgoingMail,
    ) -> impl core::future::Future<Output = Result<(), MailError>> + Send;
}

/// A sink that records what it was asked to send.
///
/// Available outside `cfg(test)` on purpose, for the same reason
/// `renvor_core::observe::entropy::FixedEntropy` is: an application author testing their own
/// verification flow needs one too.
#[derive(Debug, Default)]
pub struct RecordingMailSink {
    sent: std::sync::Mutex<Vec<OutgoingMail>>,
    fail_next: std::sync::atomic::AtomicBool,
}

impl RecordingMailSink {
    /// Creates an empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next delivery fail, so a caller can test the failure path deterministically.
    pub fn fail_next_delivery(&self) {
        self.fail_next
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// How many messages were delivered.
    #[must_use]
    pub fn delivered(&self) -> usize {
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }

    /// The most recently delivered message, if any.
    #[must_use]
    pub fn last(&self) -> Option<OutgoingMail> {
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .last()
            .cloned()
    }
}

impl MailPort for RecordingMailSink {
    async fn deliver(&self, mail: OutgoingMail) -> Result<(), MailError> {
        if self
            .fail_next
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            // The message is NOT recorded on the failure path — a transport that failed did not
            // send anything, and a sink that recorded it anyway would make the tests lie.
            return Err(MailError::Undeliverable);
        }
        self.sent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(mail);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MailKind, OutgoingMail, RecordingMailSink};
    use crate::opaque::{Opaque, OpaqueKind};
    use renvor_core::observe::entropy::FixedEntropy;

    fn mail() -> OutgoingMail {
        let token = Opaque::generate(
            OpaqueKind::Verification,
            &FixedEntropy::new(vec![0xAB, 0xCD]),
        )
        .expect("entropy");
        OutgoingMail::new(MailKind::Verification, "ada@example.test".to_owned(), token)
    }

    #[test]
    fn debug_reveals_neither_the_token_nor_the_recipient() {
        // FR-054 and FR-026. A mail struct logged in an error context is one of the likeliest ways
        // a reset token reaches a log aggregator.
        let mail = mail();
        let rendered = format!("{mail:?}");
        assert!(
            !rendered.contains(&mail.token().expose()),
            "Debug rendered the token: {rendered}"
        );
        assert!(
            !rendered.contains("ada@example.test"),
            "Debug rendered the recipient, which is personal data: {rendered}"
        );
        // POSITIVE CONTROL: the kind IS shown, so the redaction is targeted rather than a Debug
        // that prints nothing and would hide a real diagnostic.
        assert!(rendered.contains("Verification"), "{rendered}");
    }

    #[tokio::test]
    async fn a_failed_delivery_records_nothing() {
        use super::MailPort as _;
        let sink = RecordingMailSink::new();
        sink.fail_next_delivery();
        assert!(sink.deliver(mail()).await.is_err());
        assert_eq!(
            sink.delivered(),
            0,
            "a failed delivery must not be recorded as sent"
        );

        // POSITIVE CONTROL: the next one succeeds, so `fail_next` is one-shot rather than sticky.
        assert!(sink.deliver(mail()).await.is_ok());
        assert_eq!(sink.delivered(), 1);
    }
}
