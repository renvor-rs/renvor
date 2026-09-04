# ADR-0036: JSON logs with a Renvor redacting formatter, a bounded metrics port, W3C trace context in the kernel, and OTLP/HTTP behind `observability-otel`

| Field | Value |
|---|---|
| **ID** | 0036 |
| **State** | `proposed` |
| **Reviewer** | *(required to enter `accepted`)* |
| **Review date** | *(required to enter `accepted`)* |
| **Superseded by** | *(not superseded)* |

> **A record MUST NOT be marked `accepted` without a recorded independent review** (spec FR-013).
> Where no independent reviewer exists, acceptance requires a waiver in `governance/waivers.md`
> with an absolute expiry date. **This record carries no authority while `proposed`.**

## Context

PLAN §8 fixes *"OpenTelemetry semantic conventions and W3C Trace Context"* and §8.1 plans *"core
tracing first; OTLP only when selected"*. The Phase 002 observability contract already binds the
kernel: one span per phase, structured fields only, a run identifier on every record, redaction on
every field including span fields (C-O6), and **the library never owns the process-global
subscriber** (C-O7). Phase 009 recorded that *"a closed vocabulary is a property of a domain enum,
not of a logging library"* and left every production adapter to this phase.

Findings that shape the design (`package-decisions.md` §F, `research.md`):

- **No maintained crate redacts by field name.** The observability researcher surveyed eight; the
  only one using the right extension point is days old with no adoption. And the extension point
  matters: `tracing-subscriber`'s JSON event formatter serialises **event** fields through
  `tracing_serde` (`json.rs:272`), bypassing any custom `FormatFields`, which only governs **span**
  fields. A `Layer` cannot rewrite what the formatter prints.
- **No metrics facade bounds cardinality at the port.** `metrics` lets any call site attach any
  label value; `prometheus-client`'s `Family` creates a series per label set.
- **No small crate parses `traceparent` without the OpenTelemetry SDK.**
- **OTLP transport**: `grpc-tonic` + `tls-roots` is clean (+28 packages); `http-proto` +
  `hyper-client` is clean (+23) and `HyperClient<C: Connect>` accepts a caller-built connector;
  every `reqwest`-based path pulls `rustls-platform-verifier` and **fails the licence gate**. The
  SDK's batch processor drives the exporter with `futures_executor::block_on` on its own thread,
  so an async hyper client must be bound to the Tokio runtime by the caller.
- The workspace's `rustls` carries only the `ring` provider (ADR-0033).

## Decision

1. **Logs**: `renvor-observability` returns a subscriber value the author installs through
   `renvor_core::observe::try_init_global`; it never installs. The default output is JSON, one
   object per record, produced by a **Renvor `FormatEvent` and `FormatFields`** that emit closed
   metadata (`timestamp`, `level`, `target`, `run_id`, span chain, fields) and apply redaction to
   event fields **and** span fields — the two paths `tracing-subscriber` handles separately. A
   human-readable format is opt-in for development. `tracing-subscriber`'s `json` feature supplies
   the `tracing-serde` visitor (+1 package); the formatter is Renvor's because the crate's cannot be
   made to redact events.
2. **Redaction**: a closed, case-insensitive field-name denylist (`password`, `passphrase`,
   `secret`, `token`, `authorization`, `cookie`, `set-cookie`, `dsn`, `connection_string`,
   `api_key`, `private_key`, `credential`) rendered as `[REDACTED]`; values over 1024 bytes
   truncated with a marker; the list is additive by configuration and never subtractive. The
   same rule is applied to exported span attributes by a `SpanExporter` decorator under `otel`, so
   both sinks redact.
3. **Metrics**: a Renvor port — counters, gauges, histograms; label **names** closed at
   registration; label **values** ≤ 64 bytes; distinct combinations per instrument capped (default
   1024) with an `overflow` series beyond the cap; a deterministic `MemoryRecorder`; a Prometheus
   text renderer whose output is cross-checked against `prometheus-client`'s encoder in a
   dev-only differential test. The port names no third-party type.
4. **Trace context**: `renvor_core::observe::trace_context` parses and renders `traceparent` and
   `tracestate` per W3C §3.2–§3.3 — total, fail-closed, bounded — with property-based fuzzing.
   `renvor-http` records `trace_id`, `parent_span_id`, `trace_flags` on the handler span from a
   valid inbound header, ignores and counts an invalid one, never echoes it, and never derives the
   `RequestId` from it (ADR-0012 Finding 1 stands). Jobs carry the rendered context into their
   execution span.
5. **Health**: liveness and readiness JSON documents over a cloned `HealthState`, naming
   contributors and verdicts and nothing else; `/healthz` and `/readyz` as a `RouteGroup` behind
   `http`.
6. **OTLP export** behind `observability-otel`: `opentelemetry` 0.32.0, `opentelemetry_sdk`
   0.32.1, `opentelemetry-otlp` 0.32.0 (`http-proto`, `trace`, `hyper-client`),
   `tracing-opentelemetry` 0.33.0, `opentelemetry-semantic-conventions` 0.32.1, `hyper-rustls`
   0.27.9 (`native-tokio`, `ring`), `hyper-util`. Renvor builds the connector with
   `with_provider_and_native_roots(ring)`. **Corrected 2026-09-04 (the Phase 010 correction
   round)**: the shipped design does **not** bridge the client to the SDK's batch thread; the
   processor is Renvor's own bounded channel drained by a Tokio task on the application's
   runtime, which is what the module documentation always said — the sentence this replaces
   described a route that was considered and not taken. Bounded queue (2048), batch, export
   timeout, and shutdown flush; a full queue is a counted drop and a closed-field event; a
   shutdown flush that misses its bound aborts and joins the drain, counts the unexported spans
   (`renvor_otel_spans_unexported_total`), and returns `OtelShutdownError::FlushTimedOut`; a
   plaintext endpoint to a non-loopback host is refused at Validate; header values are `Secret`.
7. **Names** follow the semantic conventions where one exists; Renvor names are `renvor.*`; a test
   asserts the literals equal the crate's constants when both compile.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **`tracing-subscriber`'s own JSON format with a custom `FormatFields`** | measured: event fields bypass it (`json.rs:272`); redaction would cover spans and miss events, which is worse than none because it looks like coverage |
| **A `Layer` that redacts** | a layer observes and cannot rewrite what a formatter prints; the researcher confirmed the one crate claiming to do this is a no-op |
| **`metrics` facade + Prometheus exporter** | cardinality cannot be bounded at the port; the exporter's defaults pull a hyper server, `ipnet`, and a second TLS stack |
| **`prometheus-client` as the port** | its typed label sets bound names, not values from configuration such as queue names; a cap layer would still be Renvor's. Taken as the dev-only differential control instead |
| **OTLP over gRPC (`tonic`)** | five more packages for the same protocol; OTLP/HTTP to a local collector or agent is the common deployment; `tonic`'s server DoS advisory history is irrelevant to a client but its MSRV moved to 1.88 in May |
| **OTLP over `reqwest`** | fails the licence gate on every configuration (platform verifier) |
| **The SDK's `TraceContextPropagator` for parsing** | it needs the SDK's context types in the kernel and the transport; the kernel would resolve OpenTelemetry without the feature |
| **OTLP logs and metrics export in this phase** | JSON stdout already covers logs and the Prometheus renderer covers metrics; both exports are deferred with the bridge crates named |
| **Installing the subscriber from the observability crate** | forbidden by C-O7; the helper returns a value |

## Consequences

- **`renvor-core` gains two modules** (trace context, and the clock and retry of ADR-0037) with no
  new kernel dependency.
- **`renvor-http` gains three span fields and a bounded `FetchMetadata`**; its public surface is
  explicitly unstable (C-S1).
- **Twenty-three packages** enter only under `otel`; the lean facade and every non-otel build
  resolve no `opentelemetry*` crate, asserted with controls.
- **Redaction is exact-name matching, not value scanning**: a secret placed in a field named
  `note` is not caught. The type-level guarantees (`Secret`, hand-written `Debug`) are the primary
  control; the denylist is defence in depth, and the contract says so.
- **What would reverse this**: a superseding record; the metrics port's narrowness makes an
  exporter swap an adapter change.

## Compliance

- **Constitution I** — no hidden global: the subscriber is installed by the binary.
- **Constitution VI** — redaction on every field; trace headers treated as untrusted bounded
  input; bounded export.
- **PLAN §8, §8.1, §16.2** (W3C Trace Context, OpenTelemetry conventions).
- **Contracts C-O2, C-O3, C-O6, C-O7** — honoured; the observability contract is raised to 2.0.0
  to add trace context, redaction, metrics, and health rendering.
- **ADR-0012 Finding 1** — the request identifier remains entropy-only.
