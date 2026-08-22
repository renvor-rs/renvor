---
description: "Contract C-11 — middleware order, request identity, client identity, trusted proxies, host validation, and CORS"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-11 — HTTP security defaults

**Status**: defined before implementation, per constitution principle V.
**Applies to**: `renvor-http`, and the `renvor` facade under the `transport-rest` feature.

Every default in this document is **deny-first**. Constitution principle VI requires it, and each
one is the answer to a question an attacker would otherwise get to answer.

## Middleware order

Declared **outermost to innermost**. This ordering is versioned: changing it is a change to this
contract, not an implementation detail.

| # | Layer | Why it sits here |
|---|---|---|
| 1 | **Request context and request ID** | Everything downstream — including every rejection below — must be correlatable. A rejection with no correlation is an incident with no trail |
| 2 | **Host validation** | Fail closed before any work is done. A request for an unconfigured host is not this application's request |
| 3 | **Client identity** | Must resolve before anything that decides using the client's address |
| 4 | **CORS** | A preflight is answered before admission control spends a permit on it |
| 5 | **Concurrency limit** | Saturation is refused cheaply, before the work gate |
| 6 | **Admission (work gate)** | The permit is taken here and released on drop at the end of the request |
| 7 | **Timeout** | Inside admission, so a timed-out request still releases its permit |
| 8 | **Body limit** | The innermost bound, applying to the body a handler will read |
| 9 | **Trace** | Nearest the handler, so the span covers handler execution |

### How this order is proven

**By observable behaviour, never by reading the layer list.** For each adjacent pair there is a case
whose *result differs* depending on which ran first — for example a request that is simultaneously
for a disallowed host **and** over the body limit returns the **host** rejection, which is only
possible if 2 is outside 8.

A textual list of layers is not evidence: it describes what someone wrote, not what runs.

## Request identity

| Rule | Behaviour |
|---|---|
| **Generation** | A request identifier is generated for **every** request |
| **Inbound header** | **Untrusted. Never adopted as the request identity** |
| **Opacity** | Encodes nothing — no hostname, timestamp, process identifier, counter, or configuration value (C-O4) |
| **Nesting** | Nested beneath the run identifier (C-O3) |
| **Response** | The **generated** identifier is what appears in the response and in telemetry |

> **Why replacement rather than preservation.** A framework that adopts a caller-supplied identifier
> lets the caller choose the value under which its own request is logged, correlated, and audited —
> including choosing a value that collides with another tenant's. The upstream middleware for this
> preserves an inbound value and offers no overwrite; that is why Renvor supplies its own. See
> [`ADR-0012`](../decisions/0012-phase-004-custom-http-primitives.md).

An inbound value is **not** echoed into a response header and **not** emitted anywhere it could be
mistaken for the trusted identifier.

## Client identity and trusted proxies

| Property | Default |
|---|---|
| Trusted proxy set | **empty** |
| Forwarding headers (`Forwarded`, `X-Forwarded-For`) | **ignored** unless the **direct peer** is in the explicit trusted set |
| Resolution when untrusted | the **direct peer address** |
| Parsing failure | **fail closed** — resolve to the direct peer, and do **not** attribute the request to the supplied value |

### Fail-closed means these all fail closed

A value is refused, and resolution falls back to the direct peer, when it is malformed, contains a
control character, contains more than one header where one is expected, uses an obfuscated or
`unknown` identifier, or cannot be parsed to an address unambiguously.

**Failing closed here means attributing the request to the peer Renvor actually observed**, which is
always a fact. It never means "attribute it to the value we could not parse".

> The default is empty rather than "the usual private ranges" because a private-range default is
> wrong precisely when it matters: a server reachable from the internet, behind no proxy, would
> honour headers from anyone able to reach it from such a range.

## Host validation

Validated against **explicit configuration**, and **fail closed**. A request is refused when the
host is absent, empty, contains a control character, or is not in the configured set.

There is **no** "allow any host" default. An application that has not said which hosts are its own
has not been configured, and guessing on its behalf is how a host-header attack succeeds.

## CORS

| Property | Default |
|---|---|
| Allowed origins | **none** |
| Matching | **exact origins only** — no wildcard, no suffix matching, no pattern |
| Wildcard origin with credentials | **refused** |
| Allowed origin | receives `Access-Control-Allow-Origin` with **that exact origin**, plus `Vary: Origin` |
| Wildcard policy | answers `Access-Control-Allow-Origin: *` and **never** `Access-Control-Allow-Credentials` |
| Credentials | `Access-Control-Allow-Credentials: true` is emitted **only** when the policy sets it |
| Preflight | **answered before admission** spends a permit on it, with an empty **`200`** |

### Renvor refuses a disallowed origin; the specification only asks it to stay quiet

CORS as specified is enforced by the **browser**: a server emits headers and the browser decides
what a page may read. Renvor is stricter. A request carrying an `Origin` that is present,
**cross-origin**, and not permitted is **refused with `400`** — so a disallowed origin gets no
response body at all, including from a non-browser caller that would ignore the headers entirely.

Constitution principle VI requires the deny-first posture; this is what it looks like applied to
CORS.

### The same-origin carve-out

Browsers send `Origin` on same-origin `POST`, `PUT`, `PATCH`, and `DELETE` as well as on
cross-origin requests. A request whose `Origin` matches the host it was addressed to is by
definition **not** cross-origin, and CORS governs cross-origin access only — so it is not refused.

The comparison is against the **validated** host, the value host validation already accepted. An
attacker therefore cannot satisfy the carve-out without first satisfying host validation, which is
what keeps it from being a bypass.

### `OPTIONS` cannot carry an application route

The CORS implementation answers **every** `OPTIONS` request as a preflight. An application route on
`OPTIONS` would therefore appear in every route listing and never run, which contract
[`http-routing.md`](http-routing.md) names as the worse failure. Registration is refused instead.

### Refused at configuration time

A wildcard-plus-credentials configuration is refused with a **typed error when the configuration is
built** — before any request is served.

This is Renvor's own validation. The underlying library detects the same condition with an assertion
that can fire **while serving a request**, which turns a configuration mistake into a runtime panic
in production. Refusing at configuration time means the process never starts in that state. See
[`ADR-0012`](../decisions/0012-phase-004-custom-http-primitives.md).

**Renvor validates; the selected library implements.** The protocol itself — which headers to emit,
how to answer a preflight, what goes in `Vary` — is the library's, and Renvor writes none of it.
The preflight status is therefore **the library's `200`**, not a number Renvor chose. Renvor's
configuration is constrained so that no policy it accepts can reach the upstream assertion at all:
the wildcard-with-credentials combination is refused, and the methods and headers it configures are
request-mirroring rather than the literal `*` the assertion also refuses.

## Response and telemetry content

No response body and no emitted record carries a secret, an internal filesystem path, a
source-error chain, a configuration value, or a panic payload. Redaction applies to **span fields**
as well as log fields (C-O6) — spans are the path most easily forgotten, because they are not
"logs".

## Fail-open / fail-closed summary

Stated explicitly so a reader does not have to derive it.

| Control | On failure |
|---|---|
| Host validation | **closed** — request refused |
| Trusted-proxy resolution | **closed** — direct peer used, supplied value discarded |
| Forwarding-header parsing | **closed** — direct peer used |
| CORS origin matching | **closed** — not allowed |
| CORS configuration validation | **closed** — configuration refused, process does not start |
| Request-ID generation | **closed** — a request with no identifier is refused rather than served uncorrelated |
| Admission (work gate) | **closed** — refused with 503 |
| Body limit | **closed** — 413 |
| Timeout | **closed** — 408 |

**No control in this table fails open.**
