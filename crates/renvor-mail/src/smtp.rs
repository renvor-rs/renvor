//! SMTP submission through `lettre` (ADR-0034), behind the `smtp` feature.
//!
//! # TLS is the default and plaintext is a double opt-in
//!
//! `smtps://` is implicit TLS (port 465 unless given); `smtp://` is STARTTLS **required** (port
//! 587 unless given). A plaintext session is built only when the host is loopback **and**
//! [`SmtpSettings::with_allow_insecure_loopback`] was set — both, so a development sink on
//! `127.0.0.1` works and a production relay without TLS is not something anyone falls into
//! (FR-047). A non-loopback host with the flag still gets STARTTLS. Certificates are verified
//! against the native root store with the `ring` provider — the one provider this workspace
//! installs (ADR-0033 decision 6).
//!
//! # The URL is a `Secret`, exposed once
//!
//! [`SmtpSettings`] holds the URL as a [`Secret`]; `connect` reads it exactly once to build the
//! transport, and no error, event, or `Debug` carries it or any part of it.
//!
//! # Bounded, and no retry
//!
//! One timeout over every operation (default 30 s, cap 5 min) and a bounded pool (default 4, cap
//! 64) (FR-048). `send` runs the transport once; a failure comes back as a closed category and the
//! caller — normally a durable job with an idempotency key — decides (FR-050, FR-093).
//!
//! # What the events carry
//!
//! Recipient count, body sizes, the closed outcome, and the duration. Never an address, a subject,
//! a body, or the server's reply (FR-053).

use std::sync::Arc;
use std::time::{Duration, Instant};

use lettre::message::{Mailbox as LettreMailbox, MultiPart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::transport::smtp::extension::ClientId;
use lettre::transport::smtp::{Error as SmtpError, PoolConfig};
use lettre::{AsyncSmtpTransport, AsyncTransport as _, Tokio1Executor};
use renvor_config::Secret;
use renvor_core::observe::entropy::EntropySource;

use crate::port::{
    MAIL_EVENT_TARGET, MailError, MailMetrics, MailRefusal, Mailbox, Mailer, Message, MessageId,
    Receipt,
};

/// The transport label in metrics.
pub const TRANSPORT: &str = "smtp";
/// The default bound on one SMTP operation.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// The floor and cap on the operation bound.
pub const TIMEOUT_RANGE: (Duration, Duration) = (Duration::from_secs(1), Duration::from_secs(300));
/// The default pool size.
pub const DEFAULT_POOL_SIZE: u32 = 4;
/// The cap on the pool size.
pub const MAX_POOL_SIZE: u32 = 64;
/// The default idle timeout of a pooled connection.
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);
/// The most bytes a hostname, EHLO name, or sender domain may carry.
pub const MAX_NAME_BYTES: usize = 253;

/// The transport security a URL selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Security {
    /// `smtps://`: TLS from the first byte.
    ImplicitTls,
    /// `smtp://` to a non-loopback host, or to loopback without the flag: STARTTLS, required.
    StartTls,
    /// `smtp://` to a loopback host with `allow_insecure_loopback`: no TLS.
    PlaintextLoopback,
}

/// Settings for the SMTP transport.
pub struct SmtpSettings {
    url: Secret<String>,
    hello_name: String,
    sender_domain: String,
    allow_insecure_loopback: bool,
    timeout: Duration,
    pool_size: u32,
    idle_timeout: Duration,
}

impl core::fmt::Debug for SmtpSettings {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SmtpSettings")
            .field("hello_name", &self.hello_name)
            .field("sender_domain", &self.sender_domain)
            .field("allow_insecure_loopback", &self.allow_insecure_loopback)
            .field("timeout", &self.timeout)
            .field("pool_size", &self.pool_size)
            .finish_non_exhaustive()
    }
}

/// `[a-z0-9.-]{1,253}`, no leading or trailing dot: a hostname or domain literal.
fn valid_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_NAME_BYTES
        && !text.starts_with('.')
        && !text.ends_with('.')
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
}

impl SmtpSettings {
    /// Settings from a URL (`smtp://user:password@host[:port]` or `smtps://…`), the name this
    /// client announces in `EHLO`, and the domain message identifiers are generated over.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`] when a name is not a hostname.
    /// The URL itself is parsed at `connect`, where a refusal names the same category.
    pub fn new(
        url: Secret<String>,
        hello_name: &str,
        sender_domain: &str,
    ) -> Result<Self, MailError> {
        if !valid_name(hello_name) || !valid_name(sender_domain) {
            return Err(MailError::Refused(MailRefusal::SettingsInvalid));
        }
        Ok(Self {
            url,
            hello_name: hello_name.to_owned(),
            sender_domain: sender_domain.to_owned(),
            allow_insecure_loopback: false,
            timeout: DEFAULT_TIMEOUT,
            pool_size: DEFAULT_POOL_SIZE,
            idle_timeout: DEFAULT_IDLE_TIMEOUT,
        })
    }

    /// Permits a plaintext session **to a loopback host only**. Off by default.
    #[must_use]
    pub const fn with_allow_insecure_loopback(mut self, allow: bool) -> Self {
        self.allow_insecure_loopback = allow;
        self
    }

    /// Replaces the operation bound (1 s – 5 min).
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::BoundOutOfRange`].
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self, MailError> {
        if timeout < TIMEOUT_RANGE.0 || timeout > TIMEOUT_RANGE.1 {
            return Err(MailError::Refused(MailRefusal::BoundOutOfRange));
        }
        self.timeout = timeout;
        Ok(self)
    }

    /// Replaces the pool size (1 – 64).
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::BoundOutOfRange`].
    pub fn with_pool_size(mut self, size: u32) -> Result<Self, MailError> {
        if size == 0 || size > MAX_POOL_SIZE {
            return Err(MailError::Refused(MailRefusal::BoundOutOfRange));
        }
        self.pool_size = size;
        Ok(self)
    }

    /// Replaces the idle timeout of a pooled connection (1 s – 1 h).
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::BoundOutOfRange`].
    pub fn with_idle_timeout(mut self, idle: Duration) -> Result<Self, MailError> {
        if idle < Duration::from_secs(1) || idle > Duration::from_secs(60 * 60) {
            return Err(MailError::Refused(MailRefusal::BoundOutOfRange));
        }
        self.idle_timeout = idle;
        Ok(self)
    }

    /// The operation bound.
    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }
}

/// The parts of a transport URL, held only for the length of `connect`.
struct ParsedUrl {
    implicit_tls: bool,
    user: Option<String>,
    password: Option<String>,
    host: String,
    port: Option<u16>,
}

/// Decodes `%XX` escapes; refuses a malformed escape or a control character in the result.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let value = u8::from_str_radix(core::str::from_utf8(hex).ok()?, 16).ok()?;
            out.push(value);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    if decoded.bytes().any(|byte| byte < 0x20 || byte == 0x7f) {
        return None;
    }
    Some(decoded)
}

/// Parses `scheme://[user[:password]@]host[:port][/]`. Anything else is refused.
fn parse_url(raw: &str) -> Result<ParsedUrl, MailError> {
    let refused = || MailError::Refused(MailRefusal::SettingsInvalid);
    if raw.bytes().any(|byte| byte <= 0x20 || byte == 0x7f) {
        return Err(refused());
    }
    let (scheme, rest) = raw.split_once("://").ok_or_else(refused)?;
    let implicit_tls = match scheme {
        "smtp" => false,
        "smtps" => true,
        _ => return Err(refused()),
    };
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.contains('/') || rest.contains('?') || rest.contains('#') {
        return Err(refused());
    }
    let (userinfo, hostport) = match rest.rsplit_once('@') {
        Some((userinfo, hostport)) => (Some(userinfo), hostport),
        None => (None, rest),
    };
    let (user, password) = match userinfo {
        None => (None, None),
        Some(userinfo) => {
            let (user, password) = match userinfo.split_once(':') {
                Some((user, password)) => (user, Some(password)),
                None => (userinfo, None),
            };
            let user = percent_decode(user).ok_or_else(refused)?;
            if user.is_empty() {
                return Err(refused());
            }
            let password = match password {
                Some(password) => Some(percent_decode(password).ok_or_else(refused)?),
                None => None,
            };
            (Some(user), password)
        }
    };
    let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
        // A bracketed IPv6 literal.
        let (literal, after) = rest.split_once(']').ok_or_else(refused)?;
        let port = match after.strip_prefix(':') {
            Some(port) => Some(port.parse::<u16>().map_err(|_| refused())?),
            None if after.is_empty() => None,
            None => return Err(refused()),
        };
        (literal.to_owned(), port)
    } else {
        match hostport.rsplit_once(':') {
            Some((host, port)) => (
                host.to_owned(),
                Some(port.parse::<u16>().map_err(|_| refused())?),
            ),
            None => (hostport.to_owned(), None),
        }
    };
    if host.is_empty() || host.len() > MAX_NAME_BYTES || port == Some(0) {
        return Err(refused());
    }
    Ok(ParsedUrl {
        implicit_tls,
        user,
        password,
        host,
        port,
    })
}

/// True for `localhost`, `127.0.0.0/8`, and `::1`.
fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// Decides the security a URL gets under the settings (FR-047).
fn security(implicit_tls: bool, host: &str, allow_insecure_loopback: bool) -> Security {
    if implicit_tls {
        Security::ImplicitTls
    } else if allow_insecure_loopback && is_loopback(host) {
        Security::PlaintextLoopback
    } else {
        Security::StartTls
    }
}

/// Maps a transport error onto the closed port error. The error is classified, never rendered.
fn classify(error: &SmtpError) -> MailError {
    if error.is_timeout() {
        MailError::TimedOut
    } else if error.is_permanent() || error.is_response() {
        MailError::Rejected
    } else {
        MailError::Unavailable
    }
}

/// A mailer over an SMTP submission transport.
pub struct SmtpMailer {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    timeout: Duration,
    security: Security,
    sender_domain: String,
    entropy: Arc<dyn EntropySource>,
    metrics: Option<MailMetrics>,
}

impl core::fmt::Debug for SmtpMailer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SmtpMailer")
            .field("security", &self.security)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl SmtpMailer {
    /// Builds the transport. No connection is opened here: [`Mailer::verify`] — which the
    /// provider calls at Boot — is what proves the server answers.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`] when the URL cannot be used.
    pub fn connect(
        settings: &SmtpSettings,
        entropy: Arc<dyn EntropySource>,
    ) -> Result<Self, MailError> {
        let parsed = parse_url(settings.url.expose())?;
        let security = security(
            parsed.implicit_tls,
            &parsed.host,
            settings.allow_insecure_loopback,
        );
        let refused = |_| MailError::Refused(MailRefusal::SettingsInvalid);
        let (tls, default_port) = match security {
            Security::ImplicitTls => (
                Tls::Wrapper(TlsParameters::new(parsed.host.clone()).map_err(refused)?),
                lettre::transport::smtp::SUBMISSIONS_PORT,
            ),
            Security::StartTls => (
                Tls::Required(TlsParameters::new(parsed.host.clone()).map_err(refused)?),
                lettre::transport::smtp::SUBMISSION_PORT,
            ),
            Security::PlaintextLoopback => (Tls::None, 25),
        };
        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(parsed.host)
            .port(parsed.port.unwrap_or(default_port))
            .tls(tls)
            .timeout(Some(settings.timeout))
            .hello_name(ClientId::Domain(settings.hello_name.clone()))
            .pool_config(
                PoolConfig::new()
                    .max_size(settings.pool_size)
                    .idle_timeout(settings.idle_timeout),
            );
        if let Some(user) = parsed.user {
            builder =
                builder.credentials(Credentials::new(user, parsed.password.unwrap_or_default()));
        }
        Ok(Self {
            transport: builder.build(),
            timeout: settings.timeout,
            security,
            sender_domain: settings.sender_domain.clone(),
            entropy,
            metrics: None,
        })
    }

    /// Counts sends and failures in `metrics`.
    #[must_use]
    pub fn with_metrics(mut self, metrics: MailMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// The security the URL and settings selected.
    #[must_use]
    pub const fn security(&self) -> Security {
        self.security
    }

    fn lettre_mailbox(mailbox: &Mailbox) -> Result<LettreMailbox, MailError> {
        let address = mailbox.address();
        let parsed = lettre::Address::new(address.local_part(), address.domain())
            .map_err(|_| MailError::Refused(MailRefusal::AddressInvalid))?;
        Ok(LettreMailbox::new(
            mailbox.display_name().map(str::to_owned),
            parsed,
        ))
    }

    fn render(&self, message: &Message, id: &MessageId) -> Result<lettre::Message, MailError> {
        let mut builder = lettre::Message::builder()
            .from(Self::lettre_mailbox(message.from())?)
            .subject(message.subject())
            .message_id(Some(id.as_str().to_owned()));
        for recipient in message.to() {
            builder = builder.to(Self::lettre_mailbox(recipient)?);
        }
        if let Some(reply_to) = message.reply_to() {
            builder = builder.reply_to(Self::lettre_mailbox(reply_to)?);
        }
        let built = match message.html() {
            Some(html) => builder.multipart(MultiPart::alternative_plain_html(
                message.text().to_owned(),
                html.to_owned(),
            )),
            None => builder.body(message.text().to_owned()),
        };
        built.map_err(|_| MailError::Refused(MailRefusal::SettingsInvalid))
    }

    fn record(&self, message: &Message, started: Instant, outcome: Result<(), MailError>) {
        let outcome_label = match outcome {
            Ok(()) => "sent",
            Err(error) => error.as_str(),
        };
        if let Some(metrics) = &self.metrics {
            match outcome {
                Ok(()) => metrics.sent(TRANSPORT),
                Err(error) => metrics.failed(TRANSPORT, error),
            }
        }
        tracing::info!(
            target: MAIL_EVENT_TARGET,
            transport = TRANSPORT,
            recipients = message.to().len(),
            text_bytes = message.text().len(),
            html_bytes = message.html().map_or(0, str::len),
            outcome = outcome_label,
            duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            "mail send finished"
        );
    }
}

impl Mailer for SmtpMailer {
    async fn send(&self, message: Message) -> Result<Receipt, MailError> {
        let started = Instant::now();
        let id = MessageId::generate(&*self.entropy, &self.sender_domain)?;
        let rendered = self.render(&message, &id)?;
        let outcome = match tokio::time::timeout(self.timeout, self.transport.send(rendered)).await
        {
            Ok(Ok(_response)) => Ok(()),
            Ok(Err(error)) => Err(classify(&error)),
            Err(_elapsed) => Err(MailError::TimedOut),
        };
        self.record(&message, started, outcome);
        outcome.map(|()| Receipt::new(id))
    }

    async fn verify(&self) -> Result<(), MailError> {
        match tokio::time::timeout(self.timeout, self.transport.test_connection()).await {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => Err(MailError::Unavailable),
            Ok(Err(error)) => Err(classify(&error)),
            Err(_elapsed) => Err(MailError::TimedOut),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use renvor_config::Secret;
    use renvor_core::observe::FixedEntropy;

    use super::{
        DEFAULT_TIMEOUT, MAX_POOL_SIZE, Security, SmtpMailer, SmtpSettings, TIMEOUT_RANGE,
        parse_url, security,
    };
    use crate::port::{MailError, MailRefusal};

    fn settings(url: &str) -> SmtpSettings {
        SmtpSettings::new(
            Secret::new("mail.url", url.to_owned()),
            "app.example.test",
            "mail.example.test",
        )
        .unwrap()
    }

    #[test]
    fn urls_are_parsed_strictly_and_credentials_are_decoded() {
        let parsed = parse_url("smtp://user:p%40ss%3Aword@relay.example.test:2525/").unwrap();
        assert!(!parsed.implicit_tls);
        assert_eq!(parsed.user.as_deref(), Some("user"));
        assert_eq!(parsed.password.as_deref(), Some("p@ss:word"));
        assert_eq!(parsed.host, "relay.example.test");
        assert_eq!(parsed.port, Some(2525));
        let six = parse_url("smtps://[::1]:465").unwrap();
        assert!(six.implicit_tls);
        assert_eq!(six.host, "::1");
        assert_eq!(six.port, Some(465));
        assert!(
            parse_url("smtp://relay.example.test")
                .unwrap()
                .port
                .is_none()
        );
        let refused = [
            "http://relay.example.test",
            "smtp://",
            "smtp://relay.example.test/path",
            "smtp://relay.example.test?x=1",
            "smtp://relay.example.test:0",
            "smtp://relay.example.test:99999",
            "smtp://:secret@relay.example.test",
            "smtp://user:p%zz@relay.example.test",
            "smtp://user:p%0aass@relay.example.test",
            "smtp://relay.example .test",
            "smtp://[::1",
        ];
        for (index, bad) in refused.into_iter().enumerate() {
            assert!(
                parse_url(bad).is_err(),
                "rejected url case {index} was accepted"
            );
        }
    }

    #[test]
    fn plaintext_needs_loopback_and_the_flag_together() {
        assert_eq!(
            security(true, "relay.example.test", true),
            Security::ImplicitTls
        );
        assert_eq!(
            security(false, "relay.example.test", false),
            Security::StartTls
        );
        assert_eq!(
            security(false, "relay.example.test", true),
            Security::StartTls,
            "the flag never applies off loopback"
        );
        assert_eq!(
            security(false, "127.0.0.1", false),
            Security::StartTls,
            "loopback without the flag is still TLS"
        );
        assert_eq!(
            security(false, "127.0.0.1", true),
            Security::PlaintextLoopback
        );
        assert_eq!(
            security(false, "localhost", true),
            Security::PlaintextLoopback
        );
        assert_eq!(security(false, "::1", true), Security::PlaintextLoopback);
        assert_eq!(
            security(false, "127.8.8.8", true),
            Security::PlaintextLoopback
        );
        assert_eq!(security(false, "10.0.0.1", true), Security::StartTls);
    }

    // A pooled transport needs a runtime to be built and dropped on: lettre spawns its idle
    // reaper there.
    #[tokio::test]
    async fn a_built_mailer_reports_its_security_and_bounds_are_capped() {
        let entropy = Arc::new(FixedEntropy::new([0x33; 16]));
        let plain = SmtpMailer::connect(
            &settings("smtp://u:p@127.0.0.1:1025").with_allow_insecure_loopback(true),
            entropy.clone(),
        )
        .unwrap();
        assert_eq!(plain.security(), Security::PlaintextLoopback);
        let starttls =
            SmtpMailer::connect(&settings("smtp://u:p@relay.example.test"), entropy.clone())
                .unwrap();
        assert_eq!(starttls.security(), Security::StartTls);
        let implicit =
            SmtpMailer::connect(&settings("smtps://u:p@relay.example.test"), entropy.clone())
                .unwrap();
        assert_eq!(implicit.security(), Security::ImplicitTls);
        assert_eq!(
            SmtpMailer::connect(&settings("ftp://relay.example.test"), entropy).unwrap_err(),
            MailError::Refused(MailRefusal::SettingsInvalid)
        );

        assert_eq!(settings("smtp://h").timeout(), DEFAULT_TIMEOUT);
        assert!(settings("smtp://h").with_timeout(TIMEOUT_RANGE.1).is_ok());
        assert!(
            settings("smtp://h")
                .with_timeout(TIMEOUT_RANGE.1 + Duration::from_secs(1))
                .is_err()
        );
        assert!(
            settings("smtp://h")
                .with_timeout(Duration::from_millis(999))
                .is_err()
        );
        assert!(settings("smtp://h").with_pool_size(MAX_POOL_SIZE).is_ok());
        assert!(
            settings("smtp://h")
                .with_pool_size(MAX_POOL_SIZE + 1)
                .is_err()
        );
        assert!(settings("smtp://h").with_pool_size(0).is_err());
        assert!(
            SmtpSettings::new(Secret::new("k", "smtp://h".to_owned()), "Bad Name", "d").is_err()
        );
        assert!(
            SmtpSettings::new(Secret::new("k", "smtp://h".to_owned()), "ok.name", ".bad").is_err()
        );
    }

    // A pooled transport needs a runtime to be built and dropped on: lettre spawns its idle
    // reaper there.
    #[tokio::test]
    async fn debug_never_carries_the_url() {
        let built = settings("smtp://user:hunter2CanaryDoNotLeak@relay.example.test");
        let rendered = format!("{built:?}");
        assert!(!rendered.contains("hunter2"), "the credential was rendered");
        assert!(
            !rendered.contains("relay.example.test"),
            "the host was rendered"
        );
        let mailer = SmtpMailer::connect(&built, Arc::new(FixedEntropy::new([0x33; 16]))).unwrap();
        let rendered = format!("{mailer:?}");
        assert!(!rendered.contains("hunter2") && !rendered.contains("relay.example.test"));
    }
}
