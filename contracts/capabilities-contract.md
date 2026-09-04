---
description: "Contract — the cache, mail, and storage capability ports, their substitutes, bounds, closed errors, idempotency statements, and feature isolation"
version: "1.0.0"
status: "unstable — the surface it describes is explicitly unstable (pre-release, `0.0.0`); this version identifies the contract text, not a stability promise"
---

# Contract: Capabilities — Cache, Mail, Storage

**Feature**: Phase 010 — cache, jobs, mail, storage, and observability | **Satisfies**: FR-001…FR-021,
FR-043…FR-066, FR-093, FR-094, FR-102 | **Jobs**: see `jobs-contract.md` | **Observability**: see
`observability-contract.md` 2.0.0

Every capability is a **narrow port** — a trait plus its value types — in its own crate, with a
**deterministic substitute** usable outside `cfg(test)` and an adapter behind an off-by-default
feature. No public signature of a port names a third-party infrastructure type
(`crates/renvor/tests/capability_boundary.rs` scans every port file for the adapters'
identifiers, with a planted-offence control).

## C-C1 — No silent fallback

A configured adapter that cannot be reached fails **Boot**, rolls back what booted before it,
and reports a closed category naming the phase — never a substitute standing in, never a
degraded pass-through, never a queued mail or a stored object that was not persisted
(FR-008). A substitute is used only where the author constructed it.

## C-C2 — Every bound is enforced at construction

A value that exists is a value within its bound. The adapters receive only values that already
passed and do not re-check.

| Port | Bounds |
|---|---|
| Cache | key ≤ 512 bytes, no control character, no whitespace; namespace `[a-z0-9_.-]{1,64}` counted toward the key bound; value ≤ configured ceiling (default 1 MiB); TTL 1 s … configured ceiling (default 24 h); operation timeout ≤ 30 s; reconnect attempts and delays capped |
| Mail | address ≤ 254 octets, exactly one `@`, non-empty halves, no control or structural character; display name ≤ 128 bytes; ≥ 1 and ≤ 32 recipients; subject ≤ 998 bytes, no control character; text and HTML bodies ≤ 1 MiB; SMTP timeout 1 s…5 min; pool 1…64 |
| Storage | key 1–1024 bytes, `/`-separated segments, none empty, `.` or `..`, no control character, no `\ : * ? " < > \|`, no segment ending in `.` or space, no Windows reserved device name as a stem; object ≤ configured ceiling (default 64 MiB, cap 1 GiB) on write **and on read**; content type an RFC 9110 media type ≤ 255 bytes |

## C-C3 — Errors are closed and carry no text

| Port | Error |
|---|---|
| Cache | `Unavailable`, `TimedOut`, `Refused(reason)`, `Capacity` |
| Mail | `Unavailable`, `TimedOut`, `Rejected`, `Refused(reason)`, `EntropyUnavailable` |
| Storage | `Unavailable`, `TimedOut`, `Refused(reason)`, `Denied`, `Capacity` |

The server's reply, the driver's error, a path, a bucket, an address: none travels. Each adapter
classifies its dependency's error into the closed set and never renders it (FR-009, FR-063).

## C-C4 — What `Debug` shows

A cache key or value, an address, a message, an object key, and an object print **lengths and
counts**; a lease or a secret prints nothing but its width. No port type implements `Display` or
`Serialize` for a value that could carry user data.

## C-C5 — Idempotency is explicit, per capability

| Capability | Statement | Where it is proved |
|---|---|---|
| Cache | `set_if_absent` stores exactly once while the key lives; `set` and `delete` are idempotent | four barrier racers on the real server → exactly one `Stored` |
| Mail | **not idempotent**; the port makes no retry; at-least-once delivery is a durable job with an idempotency key | the contract text and the adapter's single `send` |
| Storage | `put` is last-writer-wins, whole, never interleaved (atomic write); `delete` of an absent key is `Absent`, not an error | the filesystem suite (atomic rename, no temporary file survives) |
| Jobs | `(queue, idempotency_key)` unique; concurrent enqueues store one row | `jobs-contract.md` |

## C-C6 — No implicit retry

The cache adapter never retries an operation (a retry under a failing backend amplifies
latency). The mail and storage adapters expose no retry. An application that wants one composes
`renvor_core::retry` visibly (FR-093).

## C-C7 — TLS by default; plaintext is a double opt-in

The cache adapter connects with rustls and the native root store; the SMTP adapter uses
implicit TLS for `smtps://` and required STARTTLS for `smtp://`, and accepts a plaintext session
only when the host is loopback **and** the configuration says `allow_insecure_loopback`; the
OTLP exporter accepts `http://` only to loopback. Exactly one crypto provider (`ring`) is
compiled into `rustls` anywhere in the graph, asserted by `xtask` step 7; each adapter names it
on its own feature so a consumer building the crate alone gets the same.

## C-C8 — Boot proves the backend

The cache provider connects and pings; the mail provider runs `EHLO`/`NOOP` after
authentication (`verify_on_boot`, default on); the storage provider writes and removes a probe
file. Readiness reflects the outcome; a missing required capability fails at Register naming
the dependent and the capability (kernel rule, FR-007).

## C-C9 — Feature vocabulary and isolation

Facade features `capability-cache`, `capability-jobs`, `capability-mail`, `capability-storage`,
`observability`, `observability-otel`, each off by default. Adapter features on the crates:
`renvor-cache/valkey`, `renvor-mail/smtp`, `renvor-mail/auth`, `renvor-storage/filesystem`,
`renvor-observability/http`, `renvor-observability/otel`, `renvor-sqlx/jobs`,
`renvor-seaorm/jobs`.

`xtask` step 7 asserts, against the resolved graph with a control per row: a port crate without
its adapter feature resolves no adapter; an adapter feature resolves its adapter and none of
`webpki-roots`, `rsa`, `native-tls`, `openssl`, `reqwest`, `rustls-platform-verifier`;
`renvor-core` and `renvor-auth` resolve no adapter; `renvor-sqlx`/`renvor-seaorm` with
`db-mysql,jobs` resolve no PostgreSQL driver; the facade resolves exactly the capability crate
it asked for.

## C-C10 — No object-storage service adapter in this phase

Every S3-compatible candidate failed a licence, advisory, root-store, or MSRV gate
(`decisions/0035`). The port is what makes that a later adapter rather than a later port; the
routes a later phase may take are recorded in the ADR and the phase limitations.

## Where this is enforced

- `crates/renvor-cache`, `crates/renvor-mail`, `crates/renvor-storage` unit and real-server
  suites (Valkey, Mailpit, a temporary directory), each with a redaction canary sweep;
- `crates/renvor/tests/capability_boundary.rs`;
- `xtask` step 7 capability rows and the single-provider check;
- `xtask` step 1, which refuses to run without the capability endpoints and the require flag.
