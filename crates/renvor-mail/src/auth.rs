//! The bridge from `renvor_auth::MailPort` to any [`Mailer`], behind the `auth` feature (FR-051).
//!
//! # The link comes from configuration, never from a request
//!
//! Phase 009's port hands over a kind, a recipient, and a token, and nothing about the request
//! that caused it. This bridge renders the verification and password-reset messages from a
//! **configured** base URL and sender (OWASP Forgot Password: a link built from a request `Host`
//! is a link an attacker chooses). The token is a **code in the body** — never in the subject,
//! which mail clients show in lists and notifications, and never in the link: constitution VI
//! says a secret enters no URL, and a query-string token reaches proxy logs, browser history,
//! and `Referer`. The page the link opens takes the code from the request body.
//!
//! # One failure category
//!
//! Every send failure maps to `renvor_auth::MailError::Undeliverable`, so Phase 009's enumeration
//! property holds unchanged: the service's public answer does not vary with the transport's
//! reason.

use std::sync::Arc;

use renvor_auth::mail::{MailError as AuthMailError, MailKind, MailPort, OutgoingMail};

use crate::port::{Address, MAIL_EVENT_TARGET, MailError, MailRefusal, Mailbox, Mailer, Message};

/// The most bytes a product name may carry.
pub const MAX_PRODUCT_BYTES: usize = 64;
/// The most bytes a base URL or path may carry.
pub const MAX_URL_BYTES: usize = 2048;
/// The default path the verification link points at.
pub const DEFAULT_VERIFY_PATH: &str = "/auth/verify";
/// The default path the password-reset link points at.
pub const DEFAULT_RESET_PATH: &str = "/auth/reset";

/// True when `text` holds a control character or a character that would break out of an HTML
/// text node or attribute.
fn unsafe_text(text: &str) -> bool {
    text.bytes().any(|byte| {
        byte < 0x20 || byte == 0x7f || matches!(byte, b'<' | b'>' | b'&' | b'"' | b'\'')
    })
}

/// What the templates are rendered from.
#[derive(Clone, Debug)]
pub struct AuthMailSettings {
    base_url: String,
    sender: Mailbox,
    product: String,
    verify_path: String,
    reset_path: String,
}

impl AuthMailSettings {
    /// `base_url` is an absolute `https://` or `http://` origin, optionally with a path, without
    /// query, fragment, or trailing slash; `product` names the application in the subject.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`].
    pub fn new(base_url: &str, sender: Mailbox, product: &str) -> Result<Self, MailError> {
        let refused = MailError::Refused(MailRefusal::SettingsInvalid);
        let base_url = base_url.strip_suffix('/').unwrap_or(base_url);
        let valid_url = (base_url.starts_with("https://") || base_url.starts_with("http://"))
            && base_url.len() <= MAX_URL_BYTES
            && !base_url.contains('?')
            && !base_url.contains('#')
            && !unsafe_text(base_url)
            && !base_url.contains(' ')
            && base_url
                .split_once("://")
                .is_some_and(|(_, rest)| !rest.is_empty());
        if !valid_url {
            return Err(refused);
        }
        if product.is_empty() || product.len() > MAX_PRODUCT_BYTES || unsafe_text(product) {
            return Err(refused);
        }
        Ok(Self {
            base_url: base_url.to_owned(),
            sender,
            product: product.to_owned(),
            verify_path: DEFAULT_VERIFY_PATH.to_owned(),
            reset_path: DEFAULT_RESET_PATH.to_owned(),
        })
    }

    /// Replaces the two link paths. Each starts with `/` and carries no query or fragment.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] with [`MailRefusal::SettingsInvalid`].
    pub fn with_paths(mut self, verify: &str, reset: &str) -> Result<Self, MailError> {
        for path in [verify, reset] {
            let valid = path.starts_with('/')
                && path.len() <= MAX_URL_BYTES
                && !path.contains('?')
                && !path.contains('#')
                && !path.contains(' ')
                && !unsafe_text(path);
            if !valid {
                return Err(MailError::Refused(MailRefusal::SettingsInvalid));
            }
        }
        self.verify_path = verify.to_owned();
        self.reset_path = reset.to_owned();
        Ok(self)
    }

    /// The configured base URL, without a trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The page the message points at. **Carries no token**: constitution VI says a secret
    /// enters no URL, and a query-string token is written to proxy and server logs, kept in the
    /// browser's history, and leaked through `Referer` by anything the page loads. The token is
    /// a code in the body (FR-051), entered into the form this page shows; the auth routes read
    /// it from the request body.
    fn link(&self, kind: MailKind) -> String {
        let path = match kind {
            MailKind::Verification => &self.verify_path,
            MailKind::PasswordReset => &self.reset_path,
            _ => &self.verify_path,
        };
        format!("{}{path}", self.base_url)
    }

    fn subject(&self, kind: MailKind) -> String {
        match kind {
            MailKind::PasswordReset => format!("Reset your password for {}", self.product),
            _ => format!("Verify your email address for {}", self.product),
        }
    }

    fn text(&self, kind: MailKind, link: &str, token: &str) -> String {
        match kind {
            MailKind::PasswordReset => format!(
                "Hello,\n\nA password reset was requested for your {product} account. Open this \
                 page to choose a new password:\n\n{link}\n\nand enter this code:\n\n{token}\n\n\
                 If you did not request this, you can ignore this message; your password is \
                 unchanged.\n",
                product = self.product
            ),
            _ => format!(
                "Hello,\n\nConfirm your email address for {product} by opening this page:\n\n\
                 {link}\n\nand entering this code:\n\n{token}\n\nIf you did not create an \
                 account, you can ignore this message.\n",
                product = self.product
            ),
        }
    }

    fn html(&self, kind: MailKind, link: &str, token: &str) -> String {
        // `product` and `link` were validated to hold no `<`, `>`, `&`, `"`, or `'`; the token
        // is hexadecimal. Nothing here needs escaping, and nothing here is escaped, so a future
        // field that could need it must be validated the same way or this must change.
        let (lead, tail) = match kind {
            MailKind::PasswordReset => (
                format!(
                    "A password reset was requested for your {} account. Open this page to choose a new password, and enter the code below:",
                    self.product
                ),
                "If you did not request this, you can ignore this message; your password is unchanged.",
            ),
            _ => (
                format!(
                    "Confirm your email address for {} by opening this page and entering the code below:",
                    self.product
                ),
                "If you did not create an account, you can ignore this message.",
            ),
        };
        format!(
            "<!doctype html><html><body><p>Hello,</p><p>{lead}</p><p><a href=\"{link}\">{link}</a></p><p>Your code: <code>{token}</code></p><p>{tail}</p></body></html>"
        )
    }
}

/// Implements `renvor_auth::MailPort` over any [`Mailer`].
#[derive(Debug)]
pub struct AuthMailBridge<M> {
    mailer: Arc<M>,
    settings: AuthMailSettings,
}

impl<M: Mailer> AuthMailBridge<M> {
    /// Bridges `mailer` with `settings`.
    #[must_use]
    pub const fn new(mailer: Arc<M>, settings: AuthMailSettings) -> Self {
        Self { mailer, settings }
    }

    /// Renders the message for `mail` without sending it.
    ///
    /// # Errors
    ///
    /// [`MailError::Refused`] when the recipient is not an address the port accepts.
    pub fn render(&self, mail: &OutgoingMail) -> Result<Message, MailError> {
        let recipient = Mailbox::new(Address::new(mail.recipient())?);
        let link = self.settings.link(mail.kind());
        let token = mail.token().expose();
        Message::builder(self.settings.sender.clone())
            .to(recipient)
            .subject(&self.settings.subject(mail.kind()))
            .text(self.settings.text(mail.kind(), &link, &token))
            .html(self.settings.html(mail.kind(), &link, &token))
            .build()
    }
}

impl<M: Mailer> MailPort for AuthMailBridge<M> {
    async fn deliver(&self, mail: OutgoingMail) -> Result<(), AuthMailError> {
        let kind = mail.kind();
        let outcome = match self.render(&mail) {
            Ok(message) => self.mailer.send(message).await.map(|_| ()),
            Err(error) => Err(error),
        };
        match outcome {
            Ok(()) => Ok(()),
            Err(error) => {
                tracing::warn!(
                    target: MAIL_EVENT_TARGET,
                    kind = ?kind,
                    category = error.as_str(),
                    "an authentication mail could not be delivered"
                );
                Err(AuthMailError::Undeliverable)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use renvor_auth::mail::{MailError as AuthMailError, MailKind, MailPort as _, OutgoingMail};
    use renvor_auth::opaque::{Opaque, OpaqueKind};
    use renvor_core::observe::FixedEntropy;

    use super::{AuthMailBridge, AuthMailSettings};
    use crate::port::{Address, MailError, Mailbox};
    use crate::recording::RecordingMailbox;

    fn sender() -> Mailbox {
        Mailbox::new(Address::new("no-reply@example.test").unwrap())
            .with_display_name("Example App")
            .unwrap()
    }

    fn token() -> Opaque {
        Opaque::generate(OpaqueKind::Verification, &FixedEntropy::new([0xC0; 32])).unwrap()
    }

    fn bridge() -> (Arc<RecordingMailbox>, AuthMailBridge<RecordingMailbox>) {
        let mailbox = Arc::new(RecordingMailbox::new(Arc::new(FixedEntropy::new(
            [0x44; 16],
        ))));
        let settings =
            AuthMailSettings::new("https://app.example.test/", sender(), "Example").unwrap();
        (Arc::clone(&mailbox), AuthMailBridge::new(mailbox, settings))
    }

    #[tokio::test]
    async fn the_link_is_built_from_configuration_and_the_token_is_only_in_the_body() {
        let (mailbox, bridge) = bridge();
        let token = token();
        let exposed = token.expose();
        bridge
            .deliver(OutgoingMail::new(
                MailKind::Verification,
                "ada@example.test".to_owned(),
                token,
            ))
            .await
            .unwrap();
        let message = mailbox.last().expect("one message was recorded");
        assert_eq!(message.to().len(), 1);
        assert_eq!(message.to()[0].address().as_str(), "ada@example.test");
        assert_eq!(message.from().address().as_str(), "no-reply@example.test");
        assert!(
            !message.subject().contains(&exposed),
            "the token is in the subject"
        );
        // The link carries NO token: constitution VI says a secret enters no URL, and a
        // verification or reset secret in a query string is logged by proxies, kept in browser
        // history, and sent in `Referer`. The token is a code in the body, entered into the form
        // the link opens (the auth routes take it from the request body).
        let link = "https://app.example.test/auth/verify";
        assert!(
            message.text().contains(link),
            "the text body lacks the configured link"
        );
        assert!(
            message
                .html()
                .is_some_and(|html| html.contains(&format!("href=\"{link}\""))),
            "the HTML body lacks the configured link"
        );
        for body in [
            message.text().to_owned(),
            message.html().unwrap().to_owned(),
        ] {
            assert!(body.contains(&exposed), "the body lacks the code");
            assert!(
                !body.contains(&format!("?token={exposed}"))
                    && !body.contains(&format!("/{exposed}"))
                    && !body.contains(&format!("#{exposed}")),
                "the token is inside a URL"
            );
            // Every URL in the body is the bare link: no query, no fragment.
            for word in body.split(|c: char| c.is_whitespace() || c == '"' || c == '>' || c == '<')
            {
                if word.starts_with("https://") {
                    assert_eq!(word, link, "a URL other than the bare link was rendered");
                }
            }
        }
        assert!(message.subject().contains("Verify"));
        // The trailing slash on the base URL did not double up.
        assert!(!message.text().contains("test//auth"));
    }

    #[tokio::test]
    async fn a_password_reset_uses_its_own_path_and_subject() {
        let (mailbox, bridge) = bridge();
        bridge
            .deliver(OutgoingMail::new(
                MailKind::PasswordReset,
                "ada@example.test".to_owned(),
                token(),
            ))
            .await
            .unwrap();
        let message = mailbox.last().unwrap();
        assert!(message.subject().contains("Reset your password"));
        assert!(
            message
                .text()
                .contains("https://app.example.test/auth/reset\n")
        );
        assert!(!message.text().contains("?token="));
    }

    #[tokio::test]
    async fn every_failure_is_undeliverable_and_nothing_is_recorded() {
        let (mailbox, bridge) = bridge();
        mailbox.fail_next(MailError::Rejected);
        let outcome = bridge
            .deliver(OutgoingMail::new(
                MailKind::Verification,
                "ada@example.test".to_owned(),
                token(),
            ))
            .await;
        assert_eq!(outcome.unwrap_err(), AuthMailError::Undeliverable);
        assert_eq!(mailbox.delivered(), 0);
        // A recipient the port refuses is the same public answer.
        let outcome = bridge
            .deliver(OutgoingMail::new(
                MailKind::Verification,
                "ada@example.test\r\nBcc: eve@example.test".to_owned(),
                token(),
            ))
            .await;
        assert_eq!(outcome.unwrap_err(), AuthMailError::Undeliverable);
        assert_eq!(mailbox.delivered(), 0);
    }

    #[test]
    fn settings_refuse_what_a_template_could_not_hold_safely() {
        for (index, bad) in [
            "app.example.test",
            "ftp://app.example.test",
            "https://",
            "https://app.example.test/?next=x",
            "https://app.example.test/#frag",
            "https://app.example.test/<script>",
            "https://app.example .test",
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                AuthMailSettings::new(bad, sender(), "Example").is_err(),
                "rejected base-url case {index} was accepted"
            );
        }
        for (index, bad) in ["", "Ex<ample", "Ex&ample", "Ex\"ample", "Ex\nample"]
            .into_iter()
            .enumerate()
        {
            assert!(
                AuthMailSettings::new("https://app.example.test", sender(), bad).is_err(),
                "rejected product case {index} was accepted"
            );
        }
        let settings =
            AuthMailSettings::new("https://app.example.test", sender(), "Example").unwrap();
        assert!(settings.clone().with_paths("/v", "/r").is_ok());
        assert!(settings.clone().with_paths("v", "/r").is_err());
        assert!(settings.with_paths("/v?x", "/r").is_err());
    }
}
