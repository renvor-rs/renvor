---
description: "Phase 002 contract — lifecycle spans, structured fields, run identifier, health and readiness"
version: "2.0.0"
status: "unstable — the surface it describes is explicitly unstable under FR-036; this version identifies the contract text, not a stability promise"
---

# Contract: Observability, Health, and Readiness

**Feature**: the phase specification *(internal record)* | **Satisfies**: FR-026…FR-029, FR-043; SC-008, SC-018, SC-019
**Status**: contract for an **explicitly unstable** surface (FR-036).

## C-O1 — One span per lifecycle phase

The kernel emits **one span per lifecycle phase**, so the observed phase sequence is assertable
**from the telemetry itself** rather than only through a test-only channel.

- **Acceptance**: SC-018 — **7 of 7** lifecycle phases emit a span.

## C-O2 — Structured fields only

All emitted data **MUST** use structured fields. Interpolated message strings are prohibited —
a field that has been formatted into a sentence cannot be filtered, indexed, or redacted.

- **Acceptance**: SC-018 — **0** emitted records use an interpolated message string in place of
  structured fields.

## C-O3 — Run identifier on every record

Every emitted record carries a kernel-generated **run identifier**, identical for the lifetime of
one application run.

- **Acceptance**: SC-018 — **100%** of emitted records carry it.
- Later phases **MAY** nest request-level identifiers beneath it. This phase **MUST NOT** define
  them.

## C-O4 — The run identifier is opaque by construction

| Property | Requirement |
|---|---|
| Generation sites | **exactly 1** |
| Input | cryptographically secure random bytes **only** |
| Forbidden inputs | hostname, timestamp, process identifier, counter, **any** configuration value |
| Production entropy | the **operating-system CSPRNG** |

An identifier that encodes facts becomes an unreviewed disclosure channel — which is why the
constraint is on the **inputs**, not on the output's appearance.

## C-O5 — Opacity is verified deterministically

| Check | Gating? | What it establishes |
|---|---|---|
| Review of the single generation site | **gating** | **0** inputs other than the entropy source |
| Fixed-entropy purity test | **gating** | the identifier is a **pure function of the supplied bytes**: **0** of its bytes change when hostname, clock, process id, counter, and the entire configuration vary while entropy is held fixed |
| Production wiring test | **gating** | **1 of 1** production entropy sources is the OS CSPRNG |
| Random-sample collision / ordering check | **NON-GATING** | a smoke signal only; **0** release gates depend on it |

> **Why the sample check is not a gate.** A collision or monotonicity assertion over a random
> sample is a **probabilistic** statement about a random source, so it can fail on a correct
> implementation. A gate that fails by chance teaches a team to re-run gates, which costs more
> than the check is worth.
>
> **What none of this proves.** These checks establish that nothing but entropy is *encoded*.
> They cannot prove non-recoverability, and **no black-box test can**.

## C-O6 — Redaction applies to every emitted field

Redaction under FR-018 and FR-037 applies to **every** emitted field **without exception** —
including span fields, which are the path most easily forgotten because they are not "logs".

## C-O7 — The library never owns the process-global subscriber

A process has **exactly one** global tracing subscriber, and installing it is a **process-wide,
once-only, effectively irreversible** decision. It belongs to the binary, not to a library the
binary happens to use.

| API | Required behaviour |
|---|---|
| `Application::build()` | installs **nothing**. It **MUST NOT** register, replace, or initialise a global subscriber, dispatcher, or default. The kernel emits through the tracing facade, which is a **no-op** when nothing is installed — so a kernel that emits and an application that never configured tracing both behave correctly |
| **preferred** bootstrap | **returns** a subscriber, layer, or dispatch **value** for the author to install. Renvor supplies the configuration; the author performs the installation |
| optional global helper | **MAY** exist, but **MUST** be explicitly named and explicitly called, **MUST** document its **process-wide** consequence in its own API documentation, and **MUST** have a **specified deterministic already-installed outcome**: it returns an `AlreadyInstalled` error. It **MUST NOT** panic, **MUST NOT** silently succeed, and **MUST NOT** silently replace an existing subscriber |

**"Safe to attempt more than once" (FR-029) means a *specified* result, not an unspecified one
that happens not to crash.** A helper that quietly returns `Ok(())` on the second call is
indistinguishable, to the caller, from one that installed something — and that ambiguity is the
defect.

> Constitution principle I prohibits opaque runtime behaviour and implicit initialisation, and
> FR-029 requires initialisation never to be a side effect of building an application. Installing a
> subscriber during `build()` would violate both, and would silently override — or silently lose
> to — whatever the author configured.

## C-O8 — Health and readiness are independent

Two questions, two answers:

| Question | Answer |
|---|---|
| "Is this process alive?" | **liveness** |
| "Should it receive work?" | **readiness** |

- They **MUST** be independently queryable and **MUST** be able to disagree. Deriving one from the
  other makes SC-008 unsatisfiable by construction.
- Entering `Drain` **MUST** make readiness report not-ready **while liveness continues to report
  alive**.
- A failing readiness contributor **MUST** be individually identifiable.
- A contributor that **panics** is caught, treated as not-ready, and identified.

- **Acceptance**: SC-008 — health and readiness disagree in at least **1** asserted state.

> Conflating the two causes an unready-but-alive process to be killed, or a draining process to
> keep receiving work. Both are outages caused by the primitive rather than by the application.

---

## Phase 010 additions (2.0.0, 2026-09-04)

The sections above are unchanged. Phase 010 supplies the crate that renders them —
`renvor-observability` — and fixes the following as contract.

### C-O9 — The redaction denylist

Every emitted field passes through one rule (C-O6). A field whose name matches a built-in name —
as a whole or as its last `.`-separated segment, case-insensitively — is emitted as `[REDACTED]`:

`password`, `passphrase`, `secret`, `token`, `authorization`, `cookie`, `set-cookie`, `dsn`,
`connection_string`, `api_key`, `private_key`, `credential`.

Configuration **adds** names and cannot remove one. A rendered value over **1024 bytes** is cut at
a character boundary and marked `…[truncated N bytes]`. The rule applies to event fields, span
fields, and — behind `otel` — exported span and event attributes.

### C-O10 — The JSON record

One object per line: `timestamp` (RFC 3339 UTC, milliseconds), `level`, `target`, `message` (the
event's `message` field when present), `fields` (every other event field), `run_id` (lifted from
the nearest enclosing span that carries one), `spans` (outermost first, each `{name, …fields}`).
Values are numbers, booleans, or strings; never an interpolated sentence carrying a value.

The formatter is Renvor's because `tracing-subscriber`'s JSON event format serialises event
fields through `tracing-serde` and never calls the layer's field formatter — measured in
`fmt/format/json.rs` 0.3.23 — so a redacting field formatter alone would redact spans and miss
events.

### C-O11 — Metrics

The port is `renvor_core::observe::metrics`: counters, gauges, histograms; label **names** closed
at registration; label **values** ≤ 64 bytes; distinct label combinations per family capped
(default 1024) with an `overflow` series beyond the cap. The Prometheus text renderer in
`renvor-observability` is cross-checked sample for sample against `prometheus-client`'s encoder.

The families Renvor emits, with their label sets:

| Family | Labels |
|---|---|
| `renvor_jobs_enqueued_total`, `renvor_jobs_claimed_total`, `renvor_jobs_released_total` | `queue` |
| `renvor_jobs_attempts_total` | `queue`, `kind`, `outcome` (`completed`, `retried`, `dead_lettered`) |
| `renvor_jobs_store_errors_total` | `queue`, `category` |
| `renvor_jobs_duration_seconds` (histogram) | `queue`, `kind` |
| `renvor_cache_hits_total`, `renvor_cache_misses_total` | `backend` |
| `renvor_cache_errors_total` | `backend`, `category` |
| `renvor_mail_sent_total` | `transport` |
| `renvor_mail_failed_total` | `transport`, `category` |
| `renvor_storage_operations_total` | `backend`, `op`, `outcome` |
| `renvor_trace_context_inbound_invalid_total` | — |
| `renvor_otel_spans_dropped_total` | — |
| `renvor_otel_exports_total` | `outcome` (`ok`, `failed`, `timed_out`) |

### C-O12 — Health documents and routes

Liveness: `{"status":"alive"|"dead"}`. Readiness: `{"status":"ready"|"not_ready",
"draining":bool,"contributors":[{"name","readiness","fault"}]}` with `fault` in `none`,
`panicked`, `timed_out`, `not_asked`. Contributor names are bounded to **64 bytes** with control
characters replaced. Behind `http`: `/healthz` and `/readyz` answer `200` when yes and `503` when
no, the document either way, over a cloned `HealthState`.

### C-O13 — Inbound trace context

`traceparent` and `tracestate` are **untrusted bounded input** parsed by the kernel (W3C
§3.2–§3.3): exactly `00-<32 hex>-<16 hex>-<2 hex>`, lowercase, non-zero trace and parent
identifiers, at most 55 bytes; `tracestate` ≤ 512 bytes and ≤ 32 members. A valid `traceparent`
is recorded on the handler span as `trace_id`, `parent_span_id`, `trace_flags`. An invalid
`traceparent` is ignored, counted (`renvor_trace_context_inbound_invalid_total`, when the
application publishes a `Registry` in state), and never echoed. An invalid or oversized
`tracestate` is dropped **alone**; the `traceparent` verdict is unaffected. The request identifier
is never derived from either.

### C-O14 — Names

Where the OpenTelemetry semantic conventions define a name Renvor uses it, spelled once in
`renvor_core::observe::semconv` and asserted equal to `opentelemetry-semantic-conventions`
0.32.1 when both compile: `http.request.method`, `http.route`, `http.response.status_code`,
`url.path`, `db.system.name`. The messaging names (`messaging.system`,
`messaging.destination.name`, `messaging.operation.type`) are **experimental** in that crate
release and are pinned to their published spelling rather than asserted against a feature Renvor
does not enable. Renvor's own names are `renvor.*` spans and the fields `run_id`, `request_id`,
`trace_id`, `parent_span_id`, `trace_flags`.

### C-O15 — OTLP export bounds

Behind `observability-otel`: OTLP/HTTP binary protobuf over `hyper` with `rustls` native roots
and the `ring` provider. An `http://` endpoint only to loopback. Header values are `Secret`s.
Queue default 2048 (cap 65 536); batch default 512 (cap 4096, ≤ queue); export, scheduled-delay,
and shutdown bounds each 1 ms…60 s. A full queue **drops** the span, counts it, and emits a
closed-field event; it never blocks the request. The processor is Renvor's, on the Tokio runtime;
`force_flush` from the SDK is a no-op and the handle's `shutdown` is the bounded flush.

### C-O16 — Capability events

Every adapter emits closed-field events on its own target — `renvor.jobs`, `renvor.cache`
(errors), `renvor.mail`, `renvor.storage`, `renvor.otel`, `renvor.auth` (the abuse guard's
store-failure event: `correlation`, `flow`, `database_error_kind`) — carrying counts, sizes,
categories, and durations, and never a key, address, subject, body, payload, path, credential,
or driver text.
