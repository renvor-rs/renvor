//! The SMTP adapter against a real sink (Mailpit), read back through its HTTP API.
//!
//! These tests need `RENVOR_TEST_SMTP_URL` (an `smtp://user:password@127.0.0.1:port` URL to a
//! loopback sink that accepts plaintext authentication) and `RENVOR_TEST_SMTP_API_URL` (the
//! sink's HTTP API). Without them they skip with a printed message; with
//! `RENVOR_TEST_REQUIRE_CAPABILITIES=1` set as well, a missing URL is a **failure**, because a
//! run that silently skipped every real-server test reports the same `ok` as one that passed.
//!
//! The API is read with a hand-written HTTP/1.0 client over `tokio::net`: an HTTP crate would be
//! a dev-dependency with a root store of its own, for two `GET`s and a `DELETE`.

#![cfg(feature = "smtp")]

use std::sync::Arc;
use std::time::Duration;

use renvor_config::Secret;
use renvor_core::observe::OsEntropy;
use renvor_core::provider::ProviderId;
use renvor_core::{ApplicationBuilder, Readiness};
use renvor_mail::port::{Address, Mailbox, Mailer as _, Message};
use renvor_mail::provider::MailProvider;
use renvor_mail::smtp::{SmtpMailer, SmtpSettings};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const URL: &str = "RENVOR_TEST_SMTP_URL";
const API: &str = "RENVOR_TEST_SMTP_API_URL";
const REQUIRE: &str = "RENVOR_TEST_REQUIRE_CAPABILITIES";
const BOUND: Duration = Duration::from_secs(10);

/// The sink is one mailbox; tests that send or read must not interleave.
static SINK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The two URLs, or `None` after printing why — unless the run requires them.
fn urls() -> Option<(String, String)> {
    match (std::env::var(URL), std::env::var(API)) {
        (Ok(url), Ok(api)) if !url.is_empty() && !api.is_empty() => Some((url, api)),
        _ => {
            assert!(
                std::env::var(REQUIRE).is_err(),
                "the run requires the SMTP sink and its URL variables are absent"
            );
            eprintln!("skipping: {URL} or {API} is not set");
            None
        }
    }
}

/// `host:port` of an `http://host:port` API URL.
fn api_authority(api: &str) -> String {
    api.trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned()
}

/// One HTTP/1.0 exchange; returns the status and the body.
async fn http(api: &str, method: &str, path: &str) -> (u16, String) {
    let authority = api_authority(api);
    let exchange = async {
        let mut stream = tokio::net::TcpStream::connect(&authority)
            .await
            .expect("connects");
        let request = format!("{method} {path} HTTP/1.0\r\nHost: {authority}\r\n\r\n");
        stream.write_all(request.as_bytes()).await.expect("writes");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.expect("reads");
        let text = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = text.split_once("\r\n\r\n").expect("a header block");
        let status: u16 = head
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .expect("a status code");
        (status, body.to_owned())
    };
    tokio::time::timeout(BOUND, exchange)
        .await
        .expect("the API answered within the bound")
}

async fn clear(api: &str) {
    let (status, _) = http(api, "DELETE", "/api/v1/messages").await;
    assert_eq!(status, 200, "the sink could not be cleared");
}

async fn messages(api: &str) -> serde_json::Value {
    let (status, body) = http(api, "GET", "/api/v1/messages?limit=50").await;
    assert_eq!(status, 200);
    serde_json::from_str(&body).expect("the sink answered JSON")
}

async fn detail(api: &str, id: &str) -> serde_json::Value {
    let (status, body) = http(api, "GET", &format!("/api/v1/message/{id}")).await;
    assert_eq!(status, 200);
    serde_json::from_str(&body).expect("the sink answered JSON")
}

fn settings(url: &str) -> SmtpSettings {
    SmtpSettings::new(
        Secret::new("mail.url", url.to_owned()),
        "test.renvor.invalid",
        "mail.renvor.invalid",
    )
    .unwrap()
    .with_allow_insecure_loopback(true)
    .with_timeout(Duration::from_secs(5))
    .unwrap()
}

fn mailbox(address: &str) -> Mailbox {
    Mailbox::new(Address::new(address).unwrap())
}

fn with_password(url: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap();
    let (userinfo, host) = rest.rsplit_once('@').unwrap();
    let user = userinfo.split_once(':').map_or(userinfo, |(user, _)| user);
    format!("{scheme}://{user}:{password}@{host}")
}

#[tokio::test]
async fn the_provider_boots_and_a_message_arrives_with_exactly_one_to_and_subject() {
    let Some((url, api)) = urls() else {
        return;
    };
    let _sink = SINK.lock().await;
    clear(&api).await;
    let mailer =
        Arc::new(SmtpMailer::connect(&settings(&url), Arc::new(OsEntropy::new())).unwrap());
    let provider = MailProvider::new(ProviderId::new("mail"), Arc::clone(&mailer));
    let mut application = ApplicationBuilder::new()
        .with_provider(Box::new(provider))
        .build()
        .expect("registers")
        .boot()
        .await
        .expect("boot verifies the sink answers");
    let verdict = application
        .health()
        .readiness()
        .contributors
        .iter()
        .find(|verdict| verdict.name == "mail")
        .map(|verdict| verdict.readiness);
    assert_eq!(verdict, Some(Readiness::Ready));

    let message = Message::builder(
        mailbox("sender@renvor.invalid")
            .with_display_name("Renvor Sender")
            .unwrap(),
    )
    .to(mailbox("ada@renvor.invalid"))
    .reply_to(mailbox("reply@renvor.invalid"))
    .subject("Subject hunter2CanaryDoNotLeak-subject")
    .text("Text body hunter2CanaryDoNotLeak-text\n".to_owned())
    .html("<p>HTML body hunter2CanaryDoNotLeak-html</p>".to_owned())
    .build()
    .unwrap();
    let receipt = mailer
        .send(message)
        .await
        .expect("the sink accepts the message");
    assert!(
        receipt.id().as_str().ends_with("@mail.renvor.invalid>"),
        "the identifier is not over the configured sender domain"
    );

    let listed = messages(&api).await;
    let items = listed["messages"].as_array().expect("a message list");
    assert_eq!(items.len(), 1, "exactly one message arrived");
    let item = &items[0];
    let to = item["To"].as_array().expect("recipients");
    assert_eq!(to.len(), 1, "exactly one To");
    assert_eq!(to[0]["Address"], "ada@renvor.invalid");
    assert_eq!(item["From"]["Address"], "sender@renvor.invalid");
    assert_eq!(item["From"]["Name"], "Renvor Sender");
    assert_eq!(item["Subject"], "Subject hunter2CanaryDoNotLeak-subject");
    let id = item["ID"].as_str().expect("an id");
    let full = detail(&api, id).await;
    assert!(
        full["Text"]
            .as_str()
            .unwrap_or("")
            .contains("hunter2CanaryDoNotLeak-text")
    );
    assert!(
        full["HTML"]
            .as_str()
            .unwrap_or("")
            .contains("hunter2CanaryDoNotLeak-html")
    );
    assert_eq!(
        full["MessageID"],
        receipt.id().as_str().trim_matches(|c| c == '<' || c == '>')
    );
    let reply_to = full["ReplyTo"].as_array().expect("reply-to");
    assert_eq!(reply_to.len(), 1);
    assert_eq!(reply_to[0]["Address"], "reply@renvor.invalid");

    application.shutdown().await;
    clear(&api).await;
}

#[tokio::test]
async fn a_wrong_password_fails_boot_as_credential_refused_without_rendering_it() {
    let Some((url, _api)) = urls() else {
        return;
    };
    let _sink = SINK.lock().await;
    let wrong = with_password(&url, "hunter2CanaryDoNotLeak");
    let mailer =
        Arc::new(SmtpMailer::connect(&settings(&wrong), Arc::new(OsEntropy::new())).unwrap());
    // The category, at the port: the server's refusal is `Rejected`, which Boot maps to
    // `CredentialRefused`. No rendering carries the password or the address.
    let error = mailer
        .verify()
        .await
        .expect_err("the sink refuses the credential");
    assert_eq!(error, renvor_mail::MailError::Rejected);
    let category = renvor_mail::MailBootError::from(error);
    assert_eq!(category, renvor_mail::MailBootError::CredentialRefused);
    for rendered in [
        error.to_string(),
        category.to_string(),
        format!("{mailer:?}"),
    ] {
        assert!(!rendered.contains("hunter2"), "the password was rendered");
        assert!(
            !rendered.contains('@'),
            "an address or the URL was rendered"
        );
    }
    // And Boot itself refuses (SC-001).
    let provider = MailProvider::new(ProviderId::new("mail"), mailer);
    let outcome = ApplicationBuilder::new()
        .with_provider(Box::new(provider))
        .build()
        .expect("registers")
        .boot()
        .await;
    assert!(
        outcome.is_err(),
        "boot reached Ready with a refused credential"
    );
}

#[tokio::test]
async fn an_unreachable_port_fails_within_the_bound() {
    let Some((url, _api)) = urls() else {
        return;
    };
    let (prefix, _) = url.rsplit_once(':').expect("a port");
    let unreachable = format!("{prefix}:1");
    let mailer = SmtpMailer::connect(&settings(&unreachable), Arc::new(OsEntropy::new())).unwrap();
    let started = std::time::Instant::now();
    let error = mailer
        .verify()
        .await
        .expect_err("nothing listens on port 1");
    assert_eq!(error, renvor_mail::MailError::Unavailable);
    assert!(
        started.elapsed() < BOUND,
        "the connect attempt was not bounded"
    );
}

#[tokio::test]
async fn a_listener_that_never_answers_fails_within_the_bound() {
    let Some((url, _api)) = urls() else {
        return;
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let stall = tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            held.push(socket);
        }
    });
    let (prefix, _) = url.rsplit_once(':').expect("a port");
    let silent = format!("{prefix}:{port}");
    let settings = settings(&silent)
        .with_timeout(Duration::from_secs(1))
        .unwrap();
    let mailer = SmtpMailer::connect(&settings, Arc::new(OsEntropy::new())).unwrap();
    let started = std::time::Instant::now();
    let error = mailer
        .verify()
        .await
        .expect_err("the listener never greets");
    assert_eq!(error, renvor_mail::MailError::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the handshake wait was not bounded"
    );
    stall.abort();
}

#[cfg(feature = "auth")]
#[tokio::test]
async fn the_auth_bridge_delivers_through_the_real_sink_with_the_token_only_in_the_body() {
    use renvor_auth::mail::{MailKind, MailPort as _, OutgoingMail};
    use renvor_auth::opaque::{Opaque, OpaqueKind};
    use renvor_mail::auth::{AuthMailBridge, AuthMailSettings};

    let Some((url, api)) = urls() else {
        return;
    };
    let _sink = SINK.lock().await;
    clear(&api).await;
    let mailer =
        Arc::new(SmtpMailer::connect(&settings(&url), Arc::new(OsEntropy::new())).unwrap());
    let bridge = AuthMailBridge::new(
        mailer,
        AuthMailSettings::new(
            "https://app.renvor.invalid",
            mailbox("no-reply@renvor.invalid"),
            "Renvor",
        )
        .unwrap(),
    );
    let token = Opaque::generate(OpaqueKind::PasswordReset, &OsEntropy::new()).unwrap();
    let exposed = token.expose();
    bridge
        .deliver(OutgoingMail::new(
            MailKind::PasswordReset,
            "ada@renvor.invalid".to_owned(),
            token,
        ))
        .await
        .expect("delivered");
    let listed = messages(&api).await;
    let items = listed["messages"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(!items[0]["Subject"].as_str().unwrap().contains(&exposed));
    assert_eq!(items[0]["To"].as_array().unwrap().len(), 1);
    let full = detail(&api, items[0]["ID"].as_str().unwrap()).await;
    let link = format!("https://app.renvor.invalid/auth/reset?token={exposed}");
    assert!(full["Text"].as_str().unwrap().contains(&link));
    assert!(full["HTML"].as_str().unwrap().contains(&link));
    clear(&api).await;
}
