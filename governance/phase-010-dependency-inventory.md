# Phase 010 — Dependency Inventory

**Date**: 2026-09-04
**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities
**Authoritative for**: constitution principle III (package-first boundaries), principle VIII
(feature isolation), principle XI (supply-chain integrity)
**Licence policy**: [`deny.toml`](../deny.toml) is the enforced allow-list. **This phase adds two
entries**, each named with the crate that needs it: `BSL-1.0` (`xxhash-rust`, mandatory under
`redis`) and `0BSD` (`quoted_printable`, under `lettre`'s `builder`). Both are OSI-approved
permissive licences; neither widens copyleft or data-licence exposure. `exceptions = []`, still.
**Lockfile**: 490 → 528 packages.

Every candidate was measured against the real workspace lockfile — additions to the 290-package
baseline, licences under `cargo deny` across every target `deny.toml` evaluates, RustSec and
GitHub advisories with positive controls, duplicates, native toolchains, MSRV against 1.94.0, and
feature isolation against `cargo tree` — and the measurements are in the phase's private
`package-decisions.md`. The verdicts are mirrored here.

---

## 1. Selected — runtime, behind off-by-default features

| Crate | Version | Feature | For | Why this one |
|---|---|---|---|---|
| `redis` | 1.6.0 | `renvor-cache/valkey` | RESP client for Valkey/Redis | `tokio-rustls-comp` + `connection-manager`; loads the native root store; the only path to `webpki-roots` (`tls-rustls-webpki-roots`) is never enabled. **Renvor names `rustls` with `ring` on the same feature** — the client enables `rustls` with no provider, which step 7 found (ADR-0033) |
| `lettre` | 0.11.23 | `renvor-mail/smtp` | SMTP submission | `tokio1-rustls, ring, rustls-native-certs, smtp-transport, pool, builder`; never `rustls-tls` (webpki-roots), `dkim` (`rsa`), `native-tls`, `boring-tls`; RUSTSEC-2026-0141 is `boring-tls`-only and patched below this version (ADR-0034) |
| `cap-std`, `cap-tempfile` | 4.0.3 | `renvor-storage/filesystem` | directory capability, atomic temporary files | `cap-std` was already present (the CLI); `cap-tempfile` is the one new package; GHSA-hp8f-xmx4-4qrg patched at 4.0.3, which `deny.toml` floors (ADR-0035) |
| `tracing-subscriber` (`json`) | 0.3.23 | `renvor-observability` | the subscriber stack, `tracing-serde` visitor | +1 package; the JSON *event* format is Renvor's because the crate's bypasses a custom field formatter (measured, ADR-0036) |
| `serde_json` | 1.0.151 | `renvor-observability` | the JSON record and health documents | already in the graph |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` (`http-proto`, `trace`, `hyper-client`), `opentelemetry-http`, `opentelemetry-semantic-conventions`, `tracing-opentelemetry` | 0.32.0 / 0.32.1 / 0.32.0 / 0.32.0 / 0.32.1 / 0.33.0 | `renvor-observability/otel` | OTLP/HTTP trace export | +23 packages; no `tonic`/`prost` gRPC stack; `reqwest` routes fail the all-target licence gate through `rustls-platform-verifier`; the batch processor is Renvor's (bounded, on the Tokio runtime) |
| `hyper`, `hyper-util`, `hyper-rustls` (`native-tokio`, `http1`, `tls12`, `ring`), `http`, `http-body-util`, `bytes`, `rustls` (`ring`, `std`, `tls12`) | 1.7.0 / 0.1.19 / 0.27.9 / 1.3.1 / 0.1.5 / 1.10.1 / 0.23.43 | `renvor-observability/otel` | the OTLP HTTP client with native roots and one provider | `with_provider_and_native_roots(ring)` |
| `renvor-jobs` | workspace | `renvor-sqlx/jobs`, `renvor-seaorm/jobs`, `renvor-testkit/jobs` | the job port the stores implement | names no driver, so a MySQL application acquires no PostgreSQL crate |
| `tracing` | 0.1.44 | `renvor-auth` | the one infrastructure event FR-084 requires | already in the graph through `renvor-core`; this names the edge |

## 2. Selected — development only

| Crate | Version | Where | For |
|---|---|---|---|
| `prometheus-client` | 0.25.1 | `renvor-observability` | the differential test of the Prometheus renderer against a reference encoder |
| `serde_json` | 1.0.151 | `renvor-mail` | reading the Mailpit API in the SMTP suite |
| `tempfile` | 3.27.0 | `renvor-storage` | temporary roots for the filesystem suite |
| `tokio` (`net`, `io-util`) | 1.53.1 | `renvor-mail`, `renvor-observability` | the hand-written HTTP clients and receivers in the real-server suites |
| `tracing` | 0.1.44 | `renvor` (examples) | the observability example |

## 3. Evaluated and rejected

| Crate | Purpose | Rejected because |
|---|---|---|
| `fred`, `rustis` | RESP client | `rustis` carries `webpki-roots 1.0.9` (banned); `fred` adds more packages for the same operations |
| `apalis`, `apalis-sql`, `sqlxmq`, `aide-de-camp` | durable jobs | `apalis-sql`: +42 packages including `rsa`, a duplicate `sqlx 0.8` stack, and `webpki-roots`; PostgreSQL-only or no SeaORM row; PLAN §21 puts jobs on the application's own database |
| `backon`, `backoff`, `tokio-retry`, `retry-policies`, `exponential-backoff` | retry | a second RNG (FR-090 names one randomness site), or unmaintained; the pure schedule is ~150 lines |
| `mail-send` | SMTP | `rustls-platform-verifier` unconditional → `webpki-root-certs` (CDLA-Permissive-2.0) on wasm32 fails the all-target licence gate; no pool |
| `async-smtp` | SMTP | not reached — no TLS integration, no pool, no timeouts |
| `object_store` | S3 | `reqwest` → `rustls-platform-verifier` → CDLA on wasm32 (**licences FAILED**); `humantime 2.4.0` (RUSTSEC-2025-0014, unmaintained) mandatory |
| `opendal` | S3 | +37 packages; the same platform verifier; duplicates |
| `aws-sdk-s3` | S3 | MSRV **1.94.1**, above the 1.94.0 floor |
| `rust-s3` | S3 | `webpki-roots` (banned), MPL-2.0 (`attohttpc`), CDLA, `quick-xml 0.38` with RUSTSEC-2026-0194/0195 |
| `metrics`, `metrics-exporter-prometheus` | metrics facade | any call site may attach any label value; cardinality cannot be bounded at the port |
| `prometheus-client` | metrics port | `Family` creates a series per label set — the same objection; taken as a dev dependency only |
| OTLP over `grpc-tonic` | export | +5 more packages (`tonic`, `tonic-prost`, `tonic-types`, `prost-types`, `hyper-timeout`) for the same wire protocol; OTLP/HTTP is the common local-collector path |
| `opentelemetry-appender-tracing` | logs bridge | logs stay on the JSON subscriber; only traces export |
| `lettre` + `aws-lc-rs` | crypto provider | a second provider beside `ring` turns the cache adapter's `ClientConfig::builder()` into a panic |

## 4. Custom code, and the requirement that forces each

| Custom | Forced by | Alternatives measured |
|---|---|---|
| Job store on the application's rows | PLAN §21 item 13, §10.1 (four rows) | every SQL job crate (above) |
| Pure retry schedule | FR-090 (one randomness site, the entropy port) | five crates (above) |
| Bounded-cardinality metrics port | FR-071 | three crates (above) |
| W3C trace-context parser | FR-074 (total, fail-closed, no SDK) | the OpenTelemetry propagator needs the SDK's context types |
| Redacting JSON formatter | FR-069 (events **and** spans) | `tracing-subscriber`'s JSON format bypasses `FormatFields` for events — measured |
| Bounded OTLP span processor | FR-078 (counted drops, on the runtime) | the SDK's batch processor drives its exporter from its own thread with `block_on` and hides drops in its own logs |
| Origin / Sec-Fetch-Site guard | FR-085 | a transport policy, not a library |

## 5. Unverified, stated

- MSRV of `cap-tempfile 4.0.3`, `prometheus-client 0.25.1`, and `hyper-util` (undeclared
  `rust-version`) is proven by the workspace compiling on 1.94.0 in the gate, not by a declaration.
- Real TLS handshakes against Valkey, SMTP, and an OTLP collector are not exercised (no trusted
  CA on a runner); the plaintext-loopback paths run against real servers and the TLS
  configuration is source-verified.
- `cap-tempfile::TempFile::replace` on Windows (rename over an existing file) is proven by the
  platform legs, not locally.
