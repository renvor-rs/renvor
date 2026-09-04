//! SMTP submission through `lettre` (ADR-0034), behind the `smtp` feature.
//!
//! # TLS is the default and plaintext is a double opt-in
//!
//! An [`SmtpEndpoint`] names the host, the port, and the [`Security`] the session gets:
//! implicit TLS (port 465 unless given), STARTTLS **required** (port 587 unless given), or
//! plaintext (port 25 unless given). A plaintext endpoint is accepted only when the host is
//! loopback **and** [`SmtpSettings::with_allow_insecure_loopback`] was set — both, so a
//! development sink on `127.0.0.1` works and a production relay without TLS is not something
//! anyone falls into (FR-047). Anything else that asks for plaintext is refused at the settings
//! boundary, before a socket exists. Certificates are verified against the native root store
//! with the `ring` provider — the one provider this workspace installs (ADR-0033 decision 6).
//!
//! # The credential is a field, never part of an address
//!
//! [`SmtpCredentials`] carries the username and a [`Secret`] password beside the endpoint, not
//! inside a URL: constitution VI says a secret enters no URL, and `smtp://user:password@host`
//! is one whatever type wraps it. `connect` exposes the password exactly once, into the
//! transport's credentials, and no error, event, or `Debug` carries it or the address.
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
/// The most bytes a username may carry.
pub const MAX_USERNAME_BYTES: usize = 256;

/// The transport security an endpoint selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Security {
    /// TLS from the first byte (submissions, port 465).
    ImplicitTls,
    /// STARTTLS, required (submission, port 587).
    StartTls,
    /// No TLS. Accepted only to a loopback host with `allow_insecure_loopback` (C-C7).
    PlaintextLoopback,
}

impl Security {
    /// The port the security uses when none is given.
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Self::ImplicitTls => lettre::transport::smtp::SUBMISSIONS_PORT,
            Self::StartTls => lettre::transport::smtp::SUBMISSION_PORT,
            Self::PlaintextLoopback => 25,
        }
    }
}

/// True for `localhost`, `127.0.0.0/8`, and `::1`.
fn is_loopback(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// `[a-z0-9.-]{1,253}` with no leading or trailing dot, or an IP literal.
fn valid_host(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok() || valid_name(host)
}

/// Where the server is and how the session is secured. **No credential lives here.**
#[derive(Clone, PartialEq, Eq)]
pub struct SmtpEndpoint {
    host: String,
    port: u16,
    security: Security,
}

impl core::fmt::Debug for SmtpEndpoint {
    /// The security only. The host and port are an operator's address.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SmtpEndpoint")
            .field("security", &self.security)
            .finish_non_exhaustive()
    }
}

impl SmtpEndpoint {
    /// `host` with `security`, on the security's default port until [`Self::with_port`].
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`] for a host that is not a
    /// lowercase DNS name or an IP literal.
    pub fn new(host: &str, security: Security) -> Result<Self, MailError> {
        if !valid_host(host) {
            return Err(MailError::Refused(MailRefusal::SettingsInvalid));
        }
        Ok(Self {
            host: host.to_owned(),
            port: security.default_port(),
            security,
        })
    }

    /// Replaces the port. Zero is refused.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`].
    pub fn with_port(mut self, port: u16) -> Result<Self, MailError> {
        if port == 0 {
            return Err(MailError::Refused(MailRefusal::SettingsInvalid));
        }
        self.port = port;
        Ok(self)
    }

    /// The host.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }

    /// The security.
    #[must_use]
    pub const fn security(&self) -> Security {
        self.security
    }

    /// Whether the host is a loopback address or `localhost`.
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        is_loopback(&self.host)
    }
}

/// What the client authenticates with.
pub struct SmtpCredentials {
    username: String,
    password: Secret<String>,
}

impl core::fmt::Debug for SmtpCredentials {
    /// Nothing but the type: the username identifies an account and the password is a secret.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SmtpCredentials").finish_non_exhaustive()
    }
}

impl SmtpCredentials {
    /// A username and its password.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`] for an empty username, one
    /// over [`MAX_USERNAME_BYTES`], or one holding a control character or whitespace.
    pub fn new(username: &str, password: Secret<String>) -> Result<Self, MailError> {
        let valid = !username.is_empty()
            && username.len() <= MAX_USERNAME_BYTES
            && !username
                .chars()
                .any(|character| character.is_control() || character.is_whitespace());
        if !valid {
            return Err(MailError::Refused(MailRefusal::SettingsInvalid));
        }
        Ok(Self {
            username: username.to_owned(),
            password,
        })
    }
}

/// Settings for the SMTP transport.
pub struct SmtpSettings {
    endpoint: SmtpEndpoint,
    credentials: Option<SmtpCredentials>,
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
            .field("endpoint", &self.endpoint)
            .field("authenticated", &self.credentials.is_some())
            .field("hello_name", &self.hello_name)
            .field("sender_domain", &self.sender_domain)
            .field("allow_insecure_loopback", &self.allow_insecure_loopback)
            .field("timeout", &self.timeout)
            .field("pool_size", &self.pool_size)
            .finish_non_exhaustive()
    }
}

/// `[a-z0-9.-]{1,253}`, no leading or trailing dot: a hostname or domain literal.
pub(crate) fn valid_name(text: &str) -> bool {
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
    /// Settings for `endpoint`, authenticating with `credentials` when given, announcing
    /// `hello_name` in `EHLO`, and generating message identifiers over `sender_domain`.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`] when a name is not a hostname.
    /// Whether the endpoint's security is permitted is [`Self::validate`]'s question, which
    /// `connect` asks first.
    pub fn new(
        endpoint: SmtpEndpoint,
        credentials: Option<SmtpCredentials>,
        hello_name: &str,
        sender_domain: &str,
    ) -> Result<Self, MailError> {
        if !valid_name(hello_name) || !valid_name(sender_domain) {
            return Err(MailError::Refused(MailRefusal::SettingsInvalid));
        }
        Ok(Self {
            endpoint,
            credentials,
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

    /// The endpoint.
    #[must_use]
    pub const fn endpoint(&self) -> &SmtpEndpoint {
        &self.endpoint
    }

    /// The settings rule, applied before any socket: a plaintext endpoint is accepted only when
    /// its host is loopback **and** the opt-in was given (C-C7, FR-047).
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::PlaintextNotPermitted`].
    pub fn validate(&self) -> Result<(), MailError> {
        let plaintext_permitted = self.allow_insecure_loopback && self.endpoint.is_loopback();
        if self.endpoint.security == Security::PlaintextLoopback && !plaintext_permitted {
            return Err(MailError::Refused(MailRefusal::PlaintextNotPermitted));
        }
        Ok(())
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
    /// [`MailError::Refused`] with [`MailRefusal::PlaintextNotPermitted`] when the settings rule
    /// refuses the endpoint, or [`MailRefusal::SettingsInvalid`] when the TLS parameters cannot
    /// be built for the host.
    pub fn connect(
        settings: &SmtpSettings,
        entropy: Arc<dyn EntropySource>,
    ) -> Result<Self, MailError> {
        // The settings rule first, before any transport exists (C-C7).
        settings.validate()?;
        let endpoint = &settings.endpoint;
        let security = endpoint.security;
        let refused = |_| MailError::Refused(MailRefusal::SettingsInvalid);
        let tls = match security {
            Security::ImplicitTls => {
                Tls::Wrapper(TlsParameters::new(endpoint.host.clone()).map_err(refused)?)
            }
            Security::StartTls => {
                Tls::Required(TlsParameters::new(endpoint.host.clone()).map_err(refused)?)
            }
            Security::PlaintextLoopback => Tls::None,
        };
        let mut builder =
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(endpoint.host.clone())
                .port(endpoint.port)
                .tls(tls)
                .timeout(Some(settings.timeout))
                .hello_name(ClientId::Domain(settings.hello_name.clone()))
                .pool_config(
                    PoolConfig::new()
                        .max_size(settings.pool_size)
                        .idle_timeout(settings.idle_timeout),
                );
        if let Some(credentials) = &settings.credentials {
            // The one exposure of the password: into the transport that will send it.
            builder = builder.credentials(Credentials::new(
                credentials.username.clone(),
                credentials.password.expose().clone(),
            ));
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

    /// The security the endpoint selected.
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
        DEFAULT_TIMEOUT, MAX_POOL_SIZE, Security, SmtpCredentials, SmtpEndpoint, SmtpMailer,
        SmtpSettings, TIMEOUT_RANGE,
    };
    use crate::port::{MailError, MailRefusal};

    fn endpoint(host: &str, security: Security) -> SmtpEndpoint {
        SmtpEndpoint::new(host, security).unwrap()
    }

    fn credentials() -> SmtpCredentials {
        SmtpCredentials::new(
            "user",
            Secret::new("mail.password", "hunter2CanaryDoNotLeak".to_owned()),
        )
        .unwrap()
    }

    fn settings(endpoint: SmtpEndpoint) -> SmtpSettings {
        SmtpSettings::new(
            endpoint,
            Some(credentials()),
            "app.example.test",
            "mail.example.test",
        )
        .unwrap()
    }

    #[test]
    fn endpoints_default_their_port_by_security_and_refuse_bad_hosts_and_ports() {
        assert_eq!(
            endpoint("relay.example.test", Security::ImplicitTls).port(),
            465
        );
        assert_eq!(
            endpoint("relay.example.test", Security::StartTls).port(),
            587
        );
        assert_eq!(
            endpoint("127.0.0.1", Security::PlaintextLoopback).port(),
            25
        );
        assert_eq!(
            endpoint("::1", Security::PlaintextLoopback)
                .with_port(1025)
                .unwrap()
                .port(),
            1025
        );
        assert_eq!(
            endpoint("h", Security::StartTls).with_port(0).unwrap_err(),
            MailError::Refused(MailRefusal::SettingsInvalid)
        );
        for (index, bad) in [
            "",
            "Relay.Example.Test",
            "relay.example .test",
            ".relay",
            "relay\u{1}",
            "user@relay.example.test",
            &"h".repeat(254),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                SmtpEndpoint::new(bad, Security::StartTls).unwrap_err(),
                MailError::Refused(MailRefusal::SettingsInvalid),
                "host case {index} was accepted"
            );
        }
        for (index, bad) in ["", "a b", "a\u{1}", &"u".repeat(257)]
            .into_iter()
            .enumerate()
        {
            assert_eq!(
                SmtpCredentials::new(bad, Secret::new("k", "p".to_owned())).unwrap_err(),
                MailError::Refused(MailRefusal::SettingsInvalid),
                "username case {index} was accepted"
            );
        }
    }

    #[test]
    fn plaintext_needs_loopback_and_the_flag_together() {
        // C-C7, FR-047: a double opt-in. The flag alone does not open a plaintext session to a
        // host that is not loopback, and loopback alone does not open one without the flag.
        let refused = MailError::Refused(MailRefusal::PlaintextNotPermitted);
        for (index, host) in ["relay.example.test", "10.0.0.1", "192.0.2.10"]
            .into_iter()
            .enumerate()
        {
            let with_flag = settings(endpoint(host, Security::PlaintextLoopback))
                .with_allow_insecure_loopback(true);
            assert_eq!(
                with_flag.validate().unwrap_err(),
                refused,
                "the flag applied off loopback for case {index}"
            );
        }
        for (index, host) in ["127.0.0.1", "localhost", "::1", "127.8.8.8"]
            .into_iter()
            .enumerate()
        {
            let without_flag = settings(endpoint(host, Security::PlaintextLoopback));
            assert_eq!(
                without_flag.validate().unwrap_err(),
                refused,
                "loopback case {index} was accepted without the flag"
            );
            // POSITIVE CONTROL: loopback AND the flag.
            let both = settings(endpoint(host, Security::PlaintextLoopback))
                .with_allow_insecure_loopback(true);
            assert!(
                both.validate().is_ok(),
                "loopback case {index} with the flag was refused"
            );
        }
        // TLS needs no opt-in anywhere.
        assert!(
            settings(endpoint("relay.example.test", Security::StartTls))
                .validate()
                .is_ok()
        );
        assert!(
            settings(endpoint("relay.example.test", Security::ImplicitTls))
                .validate()
                .is_ok()
        );
    }

    // A pooled transport needs a runtime to be built and dropped on: lettre spawns its idle
    // reaper there.
    #[tokio::test]
    async fn a_built_mailer_reports_its_security_and_bounds_are_capped() {
        let entropy = Arc::new(FixedEntropy::new([0x33; 16]));
        let plain = SmtpMailer::connect(
            &settings(
                endpoint("127.0.0.1", Security::PlaintextLoopback)
                    .with_port(1025)
                    .unwrap(),
            )
            .with_allow_insecure_loopback(true),
            entropy.clone(),
        )
        .unwrap();
        assert_eq!(plain.security(), Security::PlaintextLoopback);
        let starttls = SmtpMailer::connect(
            &settings(endpoint("relay.example.test", Security::StartTls)),
            entropy.clone(),
        )
        .unwrap();
        assert_eq!(starttls.security(), Security::StartTls);
        let implicit = SmtpMailer::connect(
            &settings(endpoint("relay.example.test", Security::ImplicitTls)),
            entropy.clone(),
        )
        .unwrap();
        assert_eq!(implicit.security(), Security::ImplicitTls);
        // A refused plaintext endpoint never reaches the transport builder.
        assert_eq!(
            SmtpMailer::connect(
                &settings(endpoint("relay.example.test", Security::PlaintextLoopback))
                    .with_allow_insecure_loopback(true),
                entropy
            )
            .unwrap_err(),
            MailError::Refused(MailRefusal::PlaintextNotPermitted)
        );

        let plain_settings = || settings(endpoint("h", Security::StartTls));
        assert_eq!(plain_settings().timeout(), DEFAULT_TIMEOUT);
        assert!(plain_settings().with_timeout(TIMEOUT_RANGE.1).is_ok());
        assert!(
            plain_settings()
                .with_timeout(TIMEOUT_RANGE.1 + Duration::from_secs(1))
                .is_err()
        );
        assert!(
            plain_settings()
                .with_timeout(Duration::from_millis(999))
                .is_err()
        );
        assert!(plain_settings().with_pool_size(MAX_POOL_SIZE).is_ok());
        assert!(plain_settings().with_pool_size(MAX_POOL_SIZE + 1).is_err());
        assert!(plain_settings().with_pool_size(0).is_err());
        assert!(
            SmtpSettings::new(endpoint("h", Security::StartTls), None, "Bad Name", "d").is_err()
        );
        assert!(
            SmtpSettings::new(endpoint("h", Security::StartTls), None, "ok.name", ".bad").is_err()
        );
    }

    // A pooled transport needs a runtime to be built and dropped on: lettre spawns its idle
    // reaper there.
    #[tokio::test]
    async fn debug_never_carries_the_credential_or_the_host() {
        let built = settings(endpoint("relay.example.test", Security::StartTls));
        let rendered = format!("{built:?}");
        assert!(!rendered.contains("hunter2"), "the password was rendered");
        assert!(!rendered.contains("user"), "the username was rendered");
        assert!(
            !rendered.contains("relay.example.test"),
            "the host was rendered"
        );
        // POSITIVE CONTROL: the security and whether a credential is set are shown.
        assert!(rendered.contains("StartTls") && rendered.contains("authenticated: true"));
        let mailer = SmtpMailer::connect(&built, Arc::new(FixedEntropy::new([0x33; 16]))).unwrap();
        let rendered = format!("{mailer:?}");
        assert!(!rendered.contains("hunter2") && !rendered.contains("relay.example.test"));
    }
}
