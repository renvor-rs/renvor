//! The deterministic substitute: a mailbox that records what it is asked to send.
//!
//! Mirrors `renvor_auth::RecordingMailSink` (FR-046): an injectable next failure, a count, and
//! the messages themselves for a test to inspect. Message identifiers come from the injected
//! entropy over a reserved domain, so two runs with the same entropy produce the same receipts.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use renvor_core::observe::entropy::EntropySource;

use crate::port::{MailError, Mailer, Message, MessageId, Receipt};

/// The domain recorded message identifiers carry: reserved by RFC 2606, resolvable by nothing.
pub const RECORDING_DOMAIN: &str = "recording.invalid";

/// A mailer that stores every message instead of sending it.
pub struct RecordingMailbox {
    entropy: Arc<dyn EntropySource>,
    sent: Mutex<Vec<(Message, MessageId)>>,
    fail_next: Mutex<Option<MailError>>,
    fail_verification: AtomicBool,
}

impl core::fmt::Debug for RecordingMailbox {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RecordingMailbox")
            .field("delivered", &self.delivered())
            .finish_non_exhaustive()
    }
}

impl RecordingMailbox {
    /// An empty mailbox drawing identifiers from `entropy`.
    #[must_use]
    pub fn new(entropy: Arc<dyn EntropySource>) -> Self {
        Self {
            entropy,
            sent: Mutex::new(Vec::new()),
            fail_next: Mutex::new(None),
            fail_verification: AtomicBool::new(false),
        }
    }

    /// Makes the next `send` fail with `error` and record nothing.
    pub fn fail_next(&self, error: MailError) {
        *self
            .fail_next
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(error);
    }

    /// Makes every `verify` fail as [`MailError::Unavailable`] until cleared.
    pub fn fail_verification(&self, fail: bool) {
        self.fail_verification.store(fail, Ordering::SeqCst);
    }

    /// How many messages were accepted.
    #[must_use]
    pub fn delivered(&self) -> usize {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Every accepted message with its receipt identifier, oldest first.
    #[must_use]
    pub fn sent(&self) -> Vec<(Message, MessageId)> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The most recently accepted message.
    #[must_use]
    pub fn last(&self) -> Option<Message> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .last()
            .map(|(message, _)| message.clone())
    }

    /// Forgets every accepted message.
    pub fn clear(&self) {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

impl Mailer for RecordingMailbox {
    async fn send(&self, message: Message) -> Result<Receipt, MailError> {
        if let Some(error) = self
            .fail_next
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            return Err(error);
        }
        let id = MessageId::generate(&*self.entropy, RECORDING_DOMAIN)?;
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((message, id.clone()));
        Ok(Receipt::new(id))
    }

    async fn verify(&self) -> Result<(), MailError> {
        if self.fail_verification.load(Ordering::SeqCst) {
            Err(MailError::Unavailable)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use renvor_core::observe::FixedEntropy;

    use super::{RECORDING_DOMAIN, RecordingMailbox};
    use crate::port::{Address, MailError, Mailbox, Mailer as _, Message};

    fn message() -> Message {
        let mailbox = Mailbox::new(Address::new("ada@example.test").unwrap());
        Message::builder(mailbox.clone())
            .to(mailbox)
            .subject("hello")
            .text("body".to_owned())
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn a_failed_send_records_nothing_and_the_next_one_succeeds() {
        let mailbox = RecordingMailbox::new(Arc::new(FixedEntropy::new([0x11; 16])));
        mailbox.fail_next(MailError::Rejected);
        assert_eq!(
            mailbox.send(message()).await.unwrap_err(),
            MailError::Rejected
        );
        assert_eq!(mailbox.delivered(), 0);
        let receipt = mailbox.send(message()).await.unwrap();
        assert_eq!(mailbox.delivered(), 1);
        assert!(
            receipt
                .id()
                .as_str()
                .ends_with(&format!("@{RECORDING_DOMAIN}>"))
        );
        assert_eq!(mailbox.sent()[0].1, *receipt.id());
        assert_eq!(mailbox.last().unwrap().subject(), "hello");
        mailbox.clear();
        assert_eq!(mailbox.delivered(), 0);
    }

    #[tokio::test]
    async fn verification_fails_only_while_told_to() {
        let mailbox = RecordingMailbox::new(Arc::new(FixedEntropy::new([0x11; 16])));
        assert!(mailbox.verify().await.is_ok());
        mailbox.fail_verification(true);
        assert_eq!(mailbox.verify().await.unwrap_err(), MailError::Unavailable);
        mailbox.fail_verification(false);
        assert!(mailbox.verify().await.is_ok());
    }
}
