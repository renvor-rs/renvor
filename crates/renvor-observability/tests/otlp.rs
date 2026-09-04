//! The OTLP exporter end to end: a real HTTP request to a local receiver (FR-077, FR-078).
//!
//! A hand-written HTTP/1.1 receiver on loopback accepts `POST /v1/traces`, records the headers
//! and the body, and answers `200`. The test builds the layer through the public API with a
//! secret header, emits a span carrying a canary in a redacted field, shuts the exporter down,
//! and reads back what arrived: the span name, the header value, and no canary.

#![cfg(feature = "otel")]

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use renvor_config::Secret;
use renvor_core::observe::metrics::Registry;
use renvor_observability::otel::{OtlpSettings, layer};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tracing_subscriber::layer::SubscriberExt as _;

/// One received request: the raw head and the body bytes.
type Received = (String, Vec<u8>);

async fn receiver(store: Arc<Mutex<Vec<Received>>>) -> u16 {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let store = Arc::clone(&store);
            tokio::spawn(async move {
                let mut raw = Vec::new();
                let mut buffer = [0_u8; 4096];
                // Read the head, then exactly `Content-Length` body bytes.
                let (head, body) = loop {
                    let n = socket.read(&mut buffer).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&buffer[..n]);
                    if let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&raw[..split]).into_owned();
                        let length: usize = head
                            .lines()
                            .find_map(|line| {
                                let (name, value) = line.split_once(':')?;
                                name.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse().ok())
                                    .flatten()
                            })
                            .unwrap_or(0);
                        let mut body = raw[split + 4..].to_vec();
                        while body.len() < length {
                            let n = socket.read(&mut buffer).await.unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            body.extend_from_slice(&buffer[..n]);
                        }
                        break (head, body);
                    }
                };
                store
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push((head, body));
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                    .await;
            });
        }
    });
    port
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_span_reaches_a_local_receiver_with_the_secret_header_and_without_the_canary() {
    let store = Arc::new(Mutex::new(Vec::new()));
    let port = receiver(Arc::clone(&store)).await;
    let settings = OtlpSettings::new(
        &format!("http://127.0.0.1:{port}/v1/traces"),
        "otel.endpoint",
        "renvor-test",
    )
    .unwrap()
    .with_header(
        "authorization",
        Secret::new("otel.header", "Bearer header-secret-value".to_owned()),
        "otel.header",
    )
    .unwrap()
    .with_queue(64, 8)
    .unwrap()
    .with_timeouts(
        Duration::from_secs(5),
        Duration::from_millis(50),
        Duration::from_secs(5),
    )
    .unwrap();
    let registry = Registry::new();
    let (layer, handle) = layer(&settings, &registry).expect("the layer builds");
    let subscriber = tracing_subscriber::registry().with(layer);
    {
        let _guard = tracing::subscriber::set_default(subscriber);
        let span = tracing::info_span!(
            "renvor.otlp.roundtrip",
            password = "hunter2CanaryDoNotLeak",
            http.route = "/roundtrip"
        );
        let _entered = span.enter();
        tracing::info!(token = "hunter2CanaryDoNotLeak-event", "inside");
    }
    handle.shutdown().await.expect("shutdown flushes");

    let received = store.lock().unwrap_or_else(PoisonError::into_inner).clone();
    assert!(
        !received.is_empty(),
        "no export request reached the receiver"
    );
    let (head, body) = &received[0];
    assert!(
        head.starts_with("POST /v1/traces HTTP/1.1"),
        "unexpected request line"
    );
    assert!(
        head.to_ascii_lowercase()
            .contains("authorization: bearer header-secret-value"),
        "the configured header did not arrive"
    );
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: application/x-protobuf"),
        "not binary protobuf"
    );
    let body_text = String::from_utf8_lossy(body);
    assert!(
        body_text.contains("renvor.otlp.roundtrip"),
        "the span name is not in the payload"
    );
    assert!(
        body_text.contains("/roundtrip"),
        "a plain attribute was lost"
    );
    assert!(
        body_text.contains("renvor-test"),
        "the service name resource is absent"
    );
    assert!(
        !body_text.contains("hunter2"),
        "a canary reached the wire in a span or event attribute"
    );
    assert!(
        body_text.contains("[REDACTED]"),
        "the redaction marker is absent"
    );
    assert_eq!(handle_dropped_after(&registry), 0.0);
}

fn handle_dropped_after(registry: &Registry) -> f64 {
    registry
        .snapshot()
        .families
        .iter()
        .filter(|family| family.name == "renvor_otel_spans_dropped_total")
        .flat_map(|family| family.series.iter())
        .map(|series| match series.value {
            renvor_core::observe::metrics::SeriesValue::Scalar(value) => value,
            renvor_core::observe::metrics::SeriesValue::Histogram { .. } => 0.0,
        })
        .sum()
}

#[tokio::test]
async fn a_plaintext_endpoint_off_loopback_is_refused_before_anything_is_built() {
    let refused = OtlpSettings::new(
        "http://collector.example.test:4318/v1/traces",
        "otel.endpoint",
        "svc",
    );
    assert!(refused.is_err());
}
