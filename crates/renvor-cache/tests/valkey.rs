//! The Valkey adapter against a **real** server (SC-016, FR-095).
//!
//! # A skipped test is never reported as a gate
//!
//! These tests need a running Valkey. Without `RENVOR_TEST_VALKEY_URL` they skip with a printed
//! line — unless `RENVOR_TEST_REQUIRE_CAPABILITIES` is set, in which case a missing URL is a
//! **failure**, for the reason the four-row database suites give: a run that silently skipped
//! every real-server test would report the same `ok` as one that passed them. CI sets it; `xtask`
//! step 1 refuses to run without it.
//!
//! # No sleep is used as synchronisation
//!
//! The race uses a barrier; the TTL check reads the server's own `PTTL` rather than waiting for
//! expiry; the timeout tests bound a connection that will never complete rather than waiting for
//! one that might.

#![cfg(feature = "valkey")]

use std::sync::Arc;
use std::time::Duration;

use renvor_cache::valkey::{ReconnectBounds, ValkeyCache, ValkeyProvider, ValkeySettings};
use renvor_cache::{
    Cache, CacheBootError, CacheBounds, CacheError, CacheKey, CacheValue, Deleted, Namespace,
    Refusal, Stored, Ttl,
};
use renvor_config::Secret;
use renvor_core::provider::ProviderId;
use renvor_core::{ApplicationBuilder, Readiness};

const URL: &str = "RENVOR_TEST_VALKEY_URL";
const REQUIRE: &str = "RENVOR_TEST_REQUIRE_CAPABILITIES";

/// The server URL, or `None` after printing why the test is skipped — or a panic when a server
/// was required.
fn url() -> Option<String> {
    match std::env::var(URL) {
        Ok(value) if !value.is_empty() => Some(value),
        _ => {
            assert!(
                std::env::var(REQUIRE).is_err(),
                "{REQUIRE} is set, so a Valkey server was expected and `{URL}` is empty or absent. \
                 This is a FAILURE rather than a skip on purpose."
            );
            println!("SKIPPED: set {URL} to run this test against a real Valkey server");
            None
        }
    }
}

/// A unique namespace per test, so tests sharing one server cannot see each other's keys.
fn namespace(test: &str) -> Namespace {
    let mut bytes = [0_u8; 4];
    renvor_core::observe::EntropySource::fill(&renvor_core::observe::OsEntropy::new(), &mut bytes)
        .expect("entropy");
    Namespace::new(&format!(
        "t-{test}-{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3]
    ))
    .expect("a valid namespace")
}

fn settings(url: &str, test: &str, bounds: CacheBounds) -> ValkeySettings {
    ValkeySettings::new(
        Secret::new("cache.url", url.to_owned()),
        namespace(test),
        bounds,
    )
}

fn key(text: &str) -> CacheKey {
    CacheKey::new(text).expect("a valid key")
}

fn value(text: &str) -> CacheValue {
    CacheValue::within(text.as_bytes().to_vec(), &CacheBounds::new()).expect("a valid value")
}

fn ttl(secs: u64) -> Ttl {
    Ttl::within(Duration::from_secs(secs), &CacheBounds::new()).expect("a valid ttl")
}

#[tokio::test]
async fn the_provider_boots_against_the_real_server_and_reports_ready() {
    let Some(url) = url() else { return };
    let provider = ValkeyProvider::new(
        ProviderId::new("cache"),
        settings(&url, "boot", CacheBounds::new()),
    );
    let application = ApplicationBuilder::new()
        .with_provider(Box::new(provider))
        .build()
        .expect("register")
        .boot()
        .await
        .expect("boot reaches Ready against a real server");
    let report = application.health().readiness();
    let cache = report
        .contributors
        .iter()
        .find(|verdict| verdict.name == "cache")
        .expect("the cache contributor is registered");
    assert_eq!(cache.readiness, Readiness::Ready);
}

#[tokio::test]
async fn set_get_delete_round_trip_and_the_namespace_is_applied() {
    let Some(url) = url() else { return };
    let cache = ValkeyCache::connect(&settings(&url, "rt", CacheBounds::new()))
        .await
        .expect("connects");
    assert_eq!(cache.get(&key("a")).await.unwrap(), None);
    cache.set(&key("a"), value("one"), ttl(60)).await.unwrap();
    assert_eq!(
        cache.get(&key("a")).await.unwrap().unwrap().as_bytes(),
        b"one"
    );
    cache.set(&key("a"), value("two"), ttl(60)).await.unwrap();
    assert_eq!(
        cache.get(&key("a")).await.unwrap().unwrap().as_bytes(),
        b"two"
    );

    // The stored key carries the namespace: proven with a raw client, not through the port.
    let client = redis::Client::open(url.as_str()).expect("a valid url");
    let mut raw = client
        .get_multiplexed_async_connection()
        .await
        .expect("raw connection");
    let qualified = cache.namespace().qualify(&key("a")).unwrap();
    let exists: i64 = redis::cmd("EXISTS")
        .arg(&qualified)
        .query_async(&mut raw)
        .await
        .unwrap();
    assert_eq!(exists, 1, "the key is stored under its namespace");
    assert!(
        qualified.starts_with("t-rt-"),
        "the qualified key does not carry the namespace prefix"
    );

    assert_eq!(cache.delete(&key("a")).await.unwrap(), Deleted::Removed);
    assert_eq!(cache.delete(&key("a")).await.unwrap(), Deleted::Absent);
    assert_eq!(cache.get(&key("a")).await.unwrap(), None);
}

#[tokio::test]
async fn the_server_receives_the_ttl_read_back_without_waiting() {
    let Some(url) = url() else { return };
    let cache = ValkeyCache::connect(&settings(&url, "ttl", CacheBounds::new()))
        .await
        .expect("connects");
    cache.set(&key("t"), value("v"), ttl(30)).await.unwrap();
    let client = redis::Client::open(url.as_str()).expect("a valid url");
    let mut raw = client
        .get_multiplexed_async_connection()
        .await
        .expect("raw connection");
    let qualified = cache.namespace().qualify(&key("t")).unwrap();
    let remaining_ms: i64 = redis::cmd("PTTL")
        .arg(&qualified)
        .query_async(&mut raw)
        .await
        .unwrap();
    assert!(
        (1..=30_000).contains(&remaining_ms),
        "the server's TTL is not within the 30 s that was set"
    );
    cache.delete(&key("t")).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_set_if_absent_admits_exactly_one_writer_on_the_real_server() {
    // FR-094, FR-095: the single-writer primitive at the real boundary, with a barrier.
    let Some(url) = url() else { return };
    let cache = Arc::new(
        ValkeyCache::connect(&settings(&url, "race", CacheBounds::new()))
            .await
            .expect("connects"),
    );
    let barrier = Arc::new(tokio::sync::Barrier::new(4));
    let mut handles = Vec::new();
    for racer in 0..4_u8 {
        let cache = Arc::clone(&cache);
        let barrier = Arc::clone(&barrier);
        handles.push(tokio::spawn(async move {
            barrier.wait().await;
            cache
                .set_if_absent(&key("lock"), value(&racer.to_string()), ttl(30))
                .await
                .expect("the server answers")
        }));
    }
    let mut stored = 0;
    for handle in handles {
        if handle.await.expect("no panic") == Stored::Stored {
            stored += 1;
        }
    }
    assert_eq!(stored, 1, "exactly one racer may store");
    // And the value is one of the racers', untouched by the losers.
    let held = cache
        .get(&key("lock"))
        .await
        .unwrap()
        .expect("the winner's value");
    assert!(
        held.as_bytes().len() == 1 && (b'0'..=b'3').contains(&held.as_bytes()[0]),
        "the stored value is not a racer's"
    );
    cache.delete(&key("lock")).await.unwrap();
}

#[tokio::test]
async fn a_foreign_value_over_this_process_bound_is_refused_not_handed_over() {
    let Some(url) = url() else { return };
    let bounds = CacheBounds::new().with_max_value_bytes(8).unwrap();
    let cache = ValkeyCache::connect(&settings(&url, "bound", bounds))
        .await
        .expect("connects");
    let client = redis::Client::open(url.as_str()).expect("a valid url");
    let mut raw = client
        .get_multiplexed_async_connection()
        .await
        .expect("raw connection");
    let qualified = cache.namespace().qualify(&key("big")).unwrap();
    let _: String = redis::cmd("SET")
        .arg(&qualified)
        .arg(vec![0_u8; 9])
        .arg("EX")
        .arg(30)
        .query_async(&mut raw)
        .await
        .unwrap();
    assert_eq!(
        cache.get(&key("big")).await.unwrap_err(),
        CacheError::Refused(Refusal::ValueTooLarge),
        "another writer's larger bound must not raise this process's"
    );
    // POSITIVE CONTROL: at the bound, it is handed over.
    let _: String = redis::cmd("SET")
        .arg(&qualified)
        .arg(vec![0_u8; 8])
        .arg("EX")
        .arg(30)
        .query_async(&mut raw)
        .await
        .unwrap();
    assert_eq!(cache.get(&key("big")).await.unwrap().unwrap().len(), 8);
    cache.delete(&key("big")).await.unwrap();
}

/// A wrong credential built at run time: the test proves a refused credential fails closed and
/// is never rendered, and nothing in this file may itself be a hard-coded password.
fn wrong_credential() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_nanos();
    format!("wrong-{nanos:x}-DoNotLeak")
}

#[tokio::test]
async fn a_refused_credential_fails_boot_by_category_and_never_prints_it() {
    let Some(url) = url() else { return };
    // Replace the password with a wrong one, keeping host and port. The canary is the value that
    // must not appear in the error.
    let canary = wrong_credential();
    let wrong = rewrite_password(&url, &canary);
    let error = ValkeyCache::connect(&settings(&wrong, "auth", CacheBounds::new()))
        .await
        .expect_err("a wrong password must not connect");
    assert_eq!(error, CacheBootError::CredentialRefused);
    let rendered = error.to_string();
    assert!(!rendered.contains(&canary), "the credential was rendered");
    assert!(!rendered.contains("127.0.0.1"), "the address was rendered");
}

#[tokio::test]
async fn an_unreachable_server_fails_boot_within_the_connect_timeout() {
    // Needs no running server: port 1 on loopback refuses. Bounded by a short real timeout,
    // which is a bound rather than a sleep.
    let settings = ValkeySettings::new(
        Secret::new("cache.url", "redis://127.0.0.1:1/0".to_owned()),
        Namespace::new("unreachable").unwrap(),
        CacheBounds::new(),
    )
    .with_reconnect(
        ReconnectBounds::with(
            1,
            Duration::from_millis(1),
            Duration::from_millis(10),
            Duration::from_millis(500),
        )
        .unwrap(),
    );
    let started = std::time::Instant::now();
    let error = ValkeyCache::connect(&settings)
        .await
        .expect_err("nothing listens on port 1");
    assert_eq!(error, CacheBootError::Unreachable);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the connect attempt was not bounded"
    );
}

#[tokio::test]
async fn a_server_that_accepts_and_never_answers_fails_boot_within_the_bound() {
    // A listener that accepts the TCP connection and then says nothing: the handshake can never
    // complete, so the connect timeout is the only thing that ends it.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let stall = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        // Hold the socket open without ever writing.
        std::future::pending::<()>().await;
        drop(socket);
    });
    let settings = ValkeySettings::new(
        Secret::new("cache.url", format!("redis://{address}/0")),
        Namespace::new("stall").unwrap(),
        CacheBounds::new(),
    )
    .with_reconnect(
        ReconnectBounds::with(
            1,
            Duration::from_millis(1),
            Duration::from_millis(10),
            Duration::from_millis(500),
        )
        .unwrap(),
    );
    let started = std::time::Instant::now();
    let error = ValkeyCache::connect(&settings)
        .await
        .expect_err("a silent server must not pass boot");
    assert!(
        matches!(
            error,
            CacheBootError::Unreachable | CacheBootError::Unanswered
        ),
        "the boot error is neither unreachable nor unanswered"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the handshake wait was not bounded"
    );
    stall.abort();
}

/// Replaces the password in a `redis://[user]:password@host` URL.
fn rewrite_password(url: &str, password: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("a scheme");
    let (credentials, host) = rest.rsplit_once('@').expect("credentials in the test url");
    let user = credentials.split_once(':').map_or("", |(user, _)| user);
    format!("{scheme}://{user}:{password}@{host}")
}
