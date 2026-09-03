//! The mail port: a message that exists is a message that can be sent, and nothing about it
//! reaches a log.
//!
//! # Header injection is unrepresentable, not filtered
//!
//! An [`Address`], a display name, and a subject are the fields that become SMTP headers. Each is
//! validated at construction to hold no CR, LF, or NUL — and no other control character — so a
//! value that could split a header cannot exist in this crate (FR-044, T-2). The adapters receive
//! only values that already passed, which is why none of them re-checks.
//!
//! # Every bound is a constant here
//!
//! At most 254 octets per address (RFC 5321 §4.5.3.1.3 as amended), 32 recipients, 998 bytes of
//! subject (RFC 5322 §2.1.1), 1 MiB per body (FR-043). A message over any bound is refused with
//! the closed [`MailRefusal`] naming the bound, before any transport is touched.
//!
//! # What `Debug` shows
//!
//! A [`Message`] prints recipient **count** and body **lengths**. It has no `Display` and no
//! `Serialize` (FR-045). An [`Address`] prints its length. A [`Receipt`] carries a message
//! identifier and nothing else.
//!
//! # Sending is not idempotent
//!
//! [`Mailer::send`] makes no retry of its own, because a retried `DATA` is a duplicate mail and the
//! caller cannot tell a timeout from a delivery. At-least-once delivery is a durable job carrying
//! an idempotency key (FR-050, ADR-0037).

use core::fmt;
use core::future::Future;
use std::sync::Arc;

use renvor_core::observe::entropy::{EntropySource, EntropyUnavailable};
use renvor_core::observe::metrics::{Counter, MetricsError, Registry};

/// The most octets an address may carry.
pub const MAX_ADDRESS_OCTETS: usize = 254;
/// The most bytes a display name may carry.
pub const MAX_DISPLAY_NAME_BYTES: usize = 128;
/// The most `to` recipients one message may carry.
pub const MAX_RECIPIENTS: usize = 32;
/// The most bytes a subject may carry.
pub const MAX_SUBJECT_BYTES: usize = 998;
/// The most bytes a text or HTML body may carry.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;
/// The tracing target every mail event is emitted on.
pub const MAIL_EVENT_TARGET: &str = "renvor.mail";

/// Why an input was refused before any transport was touched. **Closed and fieldless.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MailRefusal {
    /// Over 254 octets, not exactly one `@`, an empty half, or a forbidden character.
    AddressInvalid,
    /// Over 128 bytes or holding a control or header-structure character.
    DisplayNameInvalid,
    /// Over 998 bytes or holding a control character.
    SubjectInvalid,
    /// No `to` recipient at all.
    NoRecipients,
    /// More than 32 `to` recipients.
    TooManyRecipients,
    /// A body over 1 MiB.
    BodyTooLarge,
    /// A configured bound exceeded its cap or fell below its floor.
    BoundOutOfRange,
    /// A transport setting that cannot be used: an unparseable URL, a non-loopback plaintext
    /// host, an invalid EHLO name or sender domain.
    SettingsInvalid,
}

impl MailRefusal {
    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AddressInvalid => "address_invalid",
            Self::DisplayNameInvalid => "display_name_invalid",
            Self::SubjectInvalid => "subject_invalid",
            Self::NoRecipients => "no_recipients",
            Self::TooManyRecipients => "too_many_recipients",
            Self::BodyTooLarge => "body_too_large",
            Self::BoundOutOfRange => "bound_out_of_range",
            Self::SettingsInvalid => "settings_invalid",
        }
    }
}

/// Why a send failed. **Closed; no variant carries text**, so a server's reply never travels
/// (FR-049).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum MailError {
    /// The server could not be reached, the connection failed, or the transport is down.
    #[error("the mail transport is unavailable")]
    Unavailable,
    /// The operation ran past its bound.
    #[error("the mail operation timed out")]
    TimedOut,
    /// The server refused the envelope, the content, or the credentials.
    #[error("the mail server rejected the message")]
    Rejected,
    /// A Renvor bound refused the input before any I/O.
    #[error("the mail port refused an input: {}", .0.as_str())]
    Refused(MailRefusal),
    /// The entropy port could not supply a message identifier.
    #[error("entropy is unavailable")]
    EntropyUnavailable,
}

impl MailError {
    /// A stable label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::TimedOut => "timed_out",
            Self::Rejected => "rejected",
            Self::Refused(_) => "refused",
            Self::EntropyUnavailable => "entropy_unavailable",
        }
    }
}

impl From<EntropyUnavailable> for MailError {
    fn from(_: EntropyUnavailable) -> Self {
        Self::EntropyUnavailable
    }
}

/// True when `byte` may never appear in a header-bound field.
///
/// Every control character, not only CR, LF, and NUL: a header is a line, and the only bytes that
/// can end or reshape one are exactly these.
const fn is_control(byte: u8) -> bool {
    byte < 0x20 || byte == 0x7f
}

/// True when `byte` would give an address or a display name header structure it must not have.
const fn is_structural(byte: u8) -> bool {
    matches!(byte, b'<' | b'>' | b'"' | b',' | b';' | b' ')
}

/// An email address: at most 254 octets, exactly one `@`, both halves non-empty, no control or
/// structural character anywhere.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Address(String);

impl Address {
    /// Validates `text` as an address.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::AddressInvalid`].
    pub fn new(text: &str) -> Result<Self, MailError> {
        let bytes = text.as_bytes();
        let valid = !bytes.is_empty()
            && bytes.len() <= MAX_ADDRESS_OCTETS
            && bytes.iter().filter(|byte| **byte == b'@').count() == 1
            && !bytes
                .iter()
                .any(|byte| is_control(*byte) || is_structural(*byte))
            && text.split_once('@').is_some_and(|(local, domain)| {
                !local.is_empty()
                    && !domain.is_empty()
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
            });
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(MailError::Refused(MailRefusal::AddressInvalid))
        }
    }

    /// The address as text, for an adapter to hand to a transport.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The part before the `@`.
    #[must_use]
    pub fn local_part(&self) -> &str {
        self.0.split_once('@').map_or("", |(local, _)| local)
    }

    /// The part after the `@`.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.split_once('@').map_or("", |(_, domain)| domain)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // An address is personal data; the length is enough to tell two apart in a test log.
        write!(f, "Address({} octets)", self.0.len())
    }
}

/// An address with an optional display name.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Mailbox {
    address: Address,
    display_name: Option<String>,
}

impl Mailbox {
    /// A mailbox with no display name.
    #[must_use]
    pub const fn new(address: Address) -> Self {
        Self {
            address,
            display_name: None,
        }
    }

    /// Adds a display name: at most 128 bytes, no control or structural character.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::DisplayNameInvalid`].
    pub fn with_display_name(mut self, name: &str) -> Result<Self, MailError> {
        let bytes = name.as_bytes();
        // A space is structural in an address, not in a name; `"` `<` `>` `,` `;` still are.
        let valid = !bytes.is_empty()
            && bytes.len() <= MAX_DISPLAY_NAME_BYTES
            && !bytes
                .iter()
                .any(|byte| is_control(*byte) || (is_structural(*byte) && *byte != b' '));
        if !valid {
            return Err(MailError::Refused(MailRefusal::DisplayNameInvalid));
        }
        self.display_name = Some(name.to_owned());
        Ok(self)
    }

    /// The address.
    #[must_use]
    pub const fn address(&self) -> &Address {
        &self.address
    }

    /// The display name, if any.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }
}

impl fmt::Debug for Mailbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox")
            .field("address", &self.address)
            .field("named", &self.display_name.is_some())
            .finish()
    }
}

/// A message identifier, `<32 hex characters@domain>`.
///
/// Generated from the entropy port over a configured domain — never a hostname, never a clock
/// (FR-054): an identifier that encodes facts is a disclosure channel.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct MessageId(String);

impl MessageId {
    /// Sixteen entropy bytes over `domain`.
    ///
    /// # Errors
    ///
    /// [`MailError::EntropyUnavailable`].
    pub fn generate(entropy: &dyn EntropySource, domain: &str) -> Result<Self, MailError> {
        let mut bytes = [0_u8; 16];
        entropy.fill(&mut bytes)?;
        let mut id = String::with_capacity(34 + domain.len());
        id.push('<');
        for byte in bytes {
            id.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
            id.push(char::from(b"0123456789abcdef"[usize::from(byte & 0x0f)]));
        }
        id.push('@');
        id.push_str(domain);
        id.push('>');
        Ok(Self(id))
    }

    /// The identifier as it appears in the `Message-ID` header, angle brackets included.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Random over a configured domain: safe to print, and useful for correlating a delivery.
        f.write_str(&self.0)
    }
}

/// What a successful send returns: the message identifier and nothing else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    id: MessageId,
}

impl Receipt {
    /// Wraps an identifier.
    #[must_use]
    pub const fn new(id: MessageId) -> Self {
        Self { id }
    }

    /// The identifier the message was sent with.
    #[must_use]
    pub const fn id(&self) -> &MessageId {
        &self.id
    }
}

/// A bounded, injection-free message. Built through [`Message::builder`].
#[derive(Clone)]
pub struct Message {
    from: Mailbox,
    to: Vec<Mailbox>,
    reply_to: Option<Mailbox>,
    subject: String,
    text: String,
    html: Option<String>,
}

impl Message {
    /// Starts a message from `from`.
    #[must_use]
    pub fn builder(from: Mailbox) -> MessageBuilder {
        MessageBuilder {
            from,
            to: Vec::new(),
            reply_to: None,
            subject: String::new(),
            text: String::new(),
            html: None,
        }
    }

    /// The sender.
    #[must_use]
    pub const fn from(&self) -> &Mailbox {
        &self.from
    }

    /// The recipients, at least one and at most 32.
    #[must_use]
    pub fn to(&self) -> &[Mailbox] {
        &self.to
    }

    /// The reply-to mailbox, if any.
    #[must_use]
    pub const fn reply_to(&self) -> Option<&Mailbox> {
        self.reply_to.as_ref()
    }

    /// The subject.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The text body.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The HTML body, if any.
    #[must_use]
    pub fn html(&self) -> Option<&str> {
        self.html.as_deref()
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("recipients", &self.to.len())
            .field("subject_bytes", &self.subject.len())
            .field("text_bytes", &self.text.len())
            .field("html_bytes", &self.html.as_ref().map_or(0, String::len))
            .finish()
    }
}

/// Builds a [`Message`], refusing at `build` anything over a bound.
#[derive(Clone, Debug)]
pub struct MessageBuilder {
    from: Mailbox,
    to: Vec<Mailbox>,
    reply_to: Option<Mailbox>,
    subject: String,
    text: String,
    html: Option<String>,
}

impl MessageBuilder {
    /// Adds a recipient.
    #[must_use]
    pub fn to(mut self, mailbox: Mailbox) -> Self {
        self.to.push(mailbox);
        self
    }

    /// Sets the reply-to mailbox.
    #[must_use]
    pub fn reply_to(mut self, mailbox: Mailbox) -> Self {
        self.reply_to = Some(mailbox);
        self
    }

    /// Sets the subject.
    #[must_use]
    pub fn subject(mut self, subject: &str) -> Self {
        self.subject = subject.to_owned();
        self
    }

    /// Sets the text body.
    #[must_use]
    pub fn text(mut self, text: String) -> Self {
        self.text = text;
        self
    }

    /// Sets the HTML body.
    #[must_use]
    pub fn html(mut self, html: String) -> Self {
        self.html = Some(html);
        self
    }

    /// Checks every bound and produces the message.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] naming the first bound that failed.
    pub fn build(self) -> Result<Message, MailError> {
        if self.to.is_empty() {
            return Err(MailError::Refused(MailRefusal::NoRecipients));
        }
        if self.to.len() > MAX_RECIPIENTS {
            return Err(MailError::Refused(MailRefusal::TooManyRecipients));
        }
        if self.subject.len() > MAX_SUBJECT_BYTES || self.subject.bytes().any(is_control) {
            return Err(MailError::Refused(MailRefusal::SubjectInvalid));
        }
        if self.text.len() > MAX_BODY_BYTES
            || self
                .html
                .as_ref()
                .is_some_and(|html| html.len() > MAX_BODY_BYTES)
        {
            return Err(MailError::Refused(MailRefusal::BodyTooLarge));
        }
        Ok(Message {
            from: self.from,
            to: self.to,
            reply_to: self.reply_to,
            subject: self.subject,
            text: self.text,
            html: self.html,
        })
    }
}

/// Something that sends mail.
///
/// Native `async fn` rather than a boxed future: the port is generic at every call site, and the
/// kernel's `Provider` is where `dyn` is needed, not here.
pub trait Mailer: Send + Sync {
    /// Sends `message` once. No retry; see the module documentation.
    fn send(&self, message: Message) -> impl Future<Output = Result<Receipt, MailError>> + Send;

    /// Proves the transport answers, for Boot (FR-052). The substitute answers at once.
    fn verify(&self) -> impl Future<Output = Result<(), MailError>> + Send {
        async { Ok(()) }
    }
}

impl<T> Mailer for Arc<T>
where
    T: Mailer + ?Sized,
{
    fn send(&self, message: Message) -> impl Future<Output = Result<Receipt, MailError>> + Send {
        (**self).send(message)
    }

    fn verify(&self) -> impl Future<Output = Result<(), MailError>> + Send {
        (**self).verify()
    }
}

/// The mail counters (FR-083): `renvor_mail_sent_total{transport}` and
/// `renvor_mail_failed_total{transport, category}`.
#[derive(Clone, Debug)]
pub struct MailMetrics {
    sent: Counter,
    failed: Counter,
}

impl MailMetrics {
    /// Registers the two families, or returns the existing ones.
    ///
    /// # Errors
    ///
    /// [`MetricsError`] when a family of the same name is registered with another shape.
    pub fn register(registry: &Registry) -> Result<Self, MetricsError> {
        Ok(Self {
            sent: registry.counter(
                "renvor_mail_sent_total",
                "Messages accepted by the transport.",
                &["transport"],
            )?,
            failed: registry.counter(
                "renvor_mail_failed_total",
                "Sends that failed, by closed category.",
                &["transport", "category"],
            )?,
        })
    }

    /// Counts one accepted send.
    pub fn sent(&self, transport: &str) {
        self.sent.increment(&[("transport", transport)], 1);
    }

    /// Counts one failed send by its closed category.
    pub fn failed(&self, transport: &str, error: MailError) {
        self.failed
            .increment(&[("transport", transport), ("category", error.as_str())], 1);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Address, MAX_ADDRESS_OCTETS, MAX_BODY_BYTES, MAX_RECIPIENTS, MAX_SUBJECT_BYTES, MailError,
        MailRefusal, Mailbox, Message, MessageId,
    };
    use renvor_core::observe::FixedEntropy;

    fn address(text: &str) -> Address {
        Address::new(text).unwrap()
    }

    #[test]
    fn addresses_follow_every_rule() {
        assert!(Address::new("ada@example.test").is_ok());
        assert!(Address::new("a.b+tag@sub.example.test").is_ok());
        let longest = format!("{}@{}", "l".repeat(64), "d".repeat(MAX_ADDRESS_OCTETS - 65));
        assert_eq!(longest.len(), MAX_ADDRESS_OCTETS);
        assert!(Address::new(&longest).is_ok(), "the boundary is inclusive");
        let refused = [
            "",
            "ada",
            "@example.test",
            "ada@",
            "ada@@example.test",
            "ada@.example.test",
            "ada@example.test.",
            "ada @example.test",
            "ada<@example.test",
            "\"ada\"@example.test",
            "ada,b@example.test",
            "ada@example.test\r\nBcc: eve@example.test",
            "ada@example.test\n",
            "ada@example\0.test",
            "ada@exam\x7fple.test",
        ];
        for (index, bad) in refused.into_iter().enumerate() {
            assert_eq!(
                Address::new(bad).unwrap_err(),
                MailError::Refused(MailRefusal::AddressInvalid),
                "rejected address case {index} was accepted"
            );
        }
        let over = format!("{}@{}", "l".repeat(64), "d".repeat(MAX_ADDRESS_OCTETS - 64));
        assert_eq!(over.len(), MAX_ADDRESS_OCTETS + 1);
        assert!(Address::new(&over).is_err());
        assert_eq!(address("ada@example.test").local_part(), "ada");
        assert_eq!(address("ada@example.test").domain(), "example.test");
    }

    #[test]
    fn display_names_and_subjects_cannot_carry_a_line_break() {
        let mailbox = Mailbox::new(address("ada@example.test"));
        assert!(mailbox.clone().with_display_name("Ada Lovelace").is_ok());
        for (index, bad) in [
            "",
            "Ada\r\nBcc: x",
            "Ada\n",
            "Ada <x>",
            "Ada, Bob",
            "\"Ada\"",
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                mailbox.clone().with_display_name(bad).unwrap_err(),
                MailError::Refused(MailRefusal::DisplayNameInvalid),
                "rejected display-name case {index} was accepted"
            );
        }
        let injected = Message::builder(mailbox.clone())
            .to(mailbox.clone())
            .subject("Hello\r\nBcc: eve@example.test")
            .build();
        assert_eq!(
            injected.unwrap_err(),
            MailError::Refused(MailRefusal::SubjectInvalid)
        );
        let long = Message::builder(mailbox.clone())
            .to(mailbox.clone())
            .subject(&"s".repeat(MAX_SUBJECT_BYTES))
            .build();
        assert!(long.is_ok(), "the subject boundary is inclusive");
        let over = Message::builder(mailbox.clone())
            .to(mailbox)
            .subject(&"s".repeat(MAX_SUBJECT_BYTES + 1))
            .build();
        assert!(over.is_err());
    }

    #[test]
    fn recipient_and_body_bounds_are_inclusive() {
        let mailbox = Mailbox::new(address("ada@example.test"));
        assert_eq!(
            Message::builder(mailbox.clone()).build().unwrap_err(),
            MailError::Refused(MailRefusal::NoRecipients)
        );
        let mut full = Message::builder(mailbox.clone());
        for _ in 0..MAX_RECIPIENTS {
            full = full.to(mailbox.clone());
        }
        assert!(full.clone().build().is_ok());
        assert_eq!(
            full.to(mailbox.clone()).build().unwrap_err(),
            MailError::Refused(MailRefusal::TooManyRecipients)
        );
        let at_bound = Message::builder(mailbox.clone())
            .to(mailbox.clone())
            .text("b".repeat(MAX_BODY_BYTES))
            .html("h".repeat(MAX_BODY_BYTES))
            .build();
        assert!(at_bound.is_ok());
        let over = Message::builder(mailbox.clone())
            .to(mailbox.clone())
            .html("h".repeat(MAX_BODY_BYTES + 1))
            .build();
        assert_eq!(
            over.unwrap_err(),
            MailError::Refused(MailRefusal::BodyTooLarge)
        );
    }

    #[test]
    fn debug_shows_counts_and_lengths_only() {
        let message = Message::builder(
            Mailbox::new(address("sender@example.test"))
                .with_display_name("Sender Person")
                .unwrap(),
        )
        .to(Mailbox::new(address("ada@example.test")))
        .to(Mailbox::new(address("bob@example.test")))
        .subject("hunter2CanaryDoNotLeak subject")
        .text("hunter2CanaryDoNotLeak body".to_owned())
        .build()
        .unwrap();
        let rendered = format!("{message:?}");
        assert!(
            !rendered.contains("hunter2"),
            "a body or subject was rendered"
        );
        assert!(
            !rendered.contains("example.test"),
            "an address was rendered"
        );
        assert!(!rendered.contains("Sender Person"), "a name was rendered");
        assert!(rendered.contains("recipients: 2"));
        assert!(rendered.contains("text_bytes: 27"));
    }

    #[test]
    fn message_ids_are_entropy_over_the_configured_domain() {
        let id = MessageId::generate(&FixedEntropy::new([0xab; 16]), "mail.example.test").unwrap();
        assert_eq!(
            id.as_str(),
            "<abababababababababababababababab@mail.example.test>"
        );
    }
}
