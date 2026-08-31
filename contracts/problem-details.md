---
description: "Contract C-13 — the public API error registry and RFC 9457 Problem Details responses"
version: "1.2.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. 1.2.0 (2026-08-29) ADDS three codes for authentication and abuse-control refusals, which the table below records as a NON-breaking operation; no existing code changed. 1.1.0 (2026-08-27) WITHDRAWS the permission for the operator-facing detail to be arbitrary runtime text, on the finding that CONSTITUTION.md principle VI names telemetry and exempts no consumer; the detail is now a closed enum. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-13 — Public API errors and Problem Details

**Status**: defined before implementation, per constitution principle V.
**Applies to**: `renvor-error`, and `renvor-http` where it maps codes to statuses.

## The registry is closed

A code is a **name that outlives its message**. A code not in the table below is a protocol error,
not an extension point.

**Registry version: `1`.** Versioned independently of the command line's `schemaVersion`, because
the two describe different surfaces that change for different reasons — a code added for
`renvor doctor` must not bump a version REST consumers pin, and the reverse.

| Code | Status | Meaning |
|---|---|---|
| `validation_failed` | 400 | One or more inputs violated a declared constraint. Carries `invalidParams` |
| `malformed_body` | 400 | The body could not be read as the declared media type |
| `missing_body` | 400 | A required body was absent |
| `unsupported_media_type` | 415 | The request's media type is not one the operation accepts |
| `not_found` | 404 | No route declares the requested path — a **routing** fact |
| `resource_not_found` | 404 | The route exists; the resource it identifies does not — a **resource** fact |
| `method_not_allowed` | 405 | The path is declared, but not for this method. The `Allow` header names the methods it answers |
| `host_rejected` | 400 | The request host is not served by this application |
| `origin_rejected` | 400 | The origin is not permitted by the configured policy |
| `payload_too_large` | 413 | The body exceeded the configured limit |
| `request_timeout` | 408 | The request exceeded the configured timeout |
| `unavailable` | 503 | The application is draining, or at its concurrency ceiling |
| `authentication_required` | 401 | The request presented no usable credential. **One code for "absent", "did not authenticate", and "no longer valid"** — separating them would tell a caller which of the three it was, which is the enumeration oracle `renvor-auth`'s fieldless `AuthError` exists to close |
| `not_permitted` | 403 | The subject is authenticated and may not perform the operation. **403 and never 404**: choosing between them discloses whether the resource exists |
| `too_many_attempts` | 429 | A bound was exceeded. **Does not say which**, and carries no retry hint — a hint would tell an attacker the window |
| `internal_error` | 500 | **Unclassified. A defect.** Its `detail` is a fixed constant |

**The status column lives in `renvor-http`, not in the registry.** A status code is transport
semantics, and putting one in the registry would make the registry need a transport.

### Adding, retiring, versioning

| Operation | Breaking? | Why |
|---|---|---|
| **Add** a code | **No** | A consumer meeting an unrecognised code has met a failure it has no specific handler for. It has not lost a handler it had |
| **Retire** a code | **Yes** | A consumer silently stops recognising a failure it used to handle |
| **Rename** a code | **Yes** | Retirement plus addition, with the added confusion of a near-miss name |
| **Reuse** a code for a different meaning | **Yes, and worst** | The consumer keeps handling it and is now wrong |

Names are `lower_snake_case` and describe **what happened** rather than which component noticed. A
name that encodes a component becomes wrong when the component is refactored.

## These are NOT the command line's codes

[`json-output.md`](json-output.md) publishes a separate closed registry for the command line. The
two vocabularies are **disjoint** — no name appears in both — and that disjointness is asserted by a
test rather than intended.

| | Public API registry | Command-line registry |
|---|---|---|
| Consumer | an HTTP client, over the network | a shell, a script, a CI job |
| Carried by | the RFC 9457 `code` extension | the C-2 envelope's `error.code` |
| Accompanied by | an HTTP status | a process exit code |

**Two pairs read as if they were the same thing. Neither is:**

| Command line | Public API | Difference |
|---|---|---|
| `internal` (exit 1) | `internal_error` (500) | The first says **the `renvor` tool** has a defect. The second says **the application being served** has one |
| `bound_exceeded` (exit 3) | `payload_too_large` (413) | The first is a bound the **CLI** applied to something it read. The second is a bound the **server** applied to a request it received |

**There is no conversion function, deliberately.** One would be an invitation to use it, and every
use would leak a server-side vocabulary into a shell exit status or the reverse.

## The document

Media type **`application/problem+json`**, per RFC 9457 §3. Not `application/json`.

| Member | Rule |
|---|---|
| `type` | A stable URI under `https://renvor.dev/problems/`, one per code. Always set — RFC 9457 permits `about:blank` and that discards the one member identifying the problem *kind* |
| `title` | **Fixed** for the type. Never occurrence-specific, never templated with request data, never localised at runtime |
| `status` | A **number**. **MUST equal the response's actual status.** RFC 9457 permits divergence; Renvor forbids it — two disagreeing statuses is a contradiction a consumer cannot resolve |
| `detail` | Derived from the **classification alone**. For an unclassified failure, a fixed constant |
| `instance` | `urn:renvor:request:<correlationId>`. **Never the request path** — a path echoes attacker-controlled input into the document |
| `code` *(extension)* | The stable code. An extension member rather than an overload of `type` or `title`, both of which have defined semantics that overloading would break |
| `correlationId` *(extension)* | See below |
| `invalidParams` *(extension)* | Present only for validation failures. Omitted entirely when empty — `[]` says something different from absence |

**There is no open extension map.** RFC 9457 permits arbitrary extensions; Renvor declares exactly
three, as typed fields. An open map would be a channel through which an unreviewed value reaches a
response.

## Redaction is enforced by the types

| Guarantee | How |
|---|---|
| `detail` cannot carry runtime data | it is `&'static str` — no runtime value inhabits the type |
| An invalid parameter cannot carry the rejected value | the type has **no field** one could occupy |
| A reason cannot be a library's message | it is `&'static str`, so a formatted string cannot be stored |

**A response MUST NOT contain** a rejected value, a secret, a filesystem path, a database statement,
a stack trace, a panic payload, or an internal cause chain — in any encoding.

**A refusal is recorded for operators**, and `renvor-http`'s `HttpError` carries both halves: the
function that renders the response cannot read the operator-facing one.

### The operator-facing half is a closed set — corrected 2026-08-27

**The sentence that used to stand here was:** *"Redaction is not achieved by discarding it: the
operator-facing detail goes to the telemetry record, which is a different consumer with different
rights."*

That is withdrawn. `CONSTITUTION.md` principle VI forbids secrets in telemetry and **exempts no
consumer**; an operator is not a party with different rights to receive a secret-bearing record.
The same defence was withdrawn for the database adapters at `contracts/error-taxonomy.md` 1.4.0 in
this phase's second correction round, and this is the site that round reported as equivalent in
shape and left standing, because changing it was outside a database-scoped authority.

What made it more than a wording problem: `HttpError::new` took `detail: impl Into<String>`, so the
operator-facing half was **unbounded runtime text chosen by the application author** and was
emitted to `tracing` by design. An author writing
`HttpError::new(kind, format!("could not reach {dsn}"))` put a DSN into a log, and the contract
above told them that was the intended use.

**Normative:**

- `HttpError`'s operator-facing detail **must** be a value of a fieldless enum whose renderings are
  literals written at its declaration. It is `renvor_http::HttpErrorDetail`, generated by
  `renvor_core::closed_named_enum!`, so a data-bearing variant is a macro error rather than a
  reviewable choice.
- No rendering path may widen it. That includes `Display`, `Debug`, the `Error::source` chain, the
  telemetry record, and the response body.
- A refusal that genuinely needs a runtime value recorded **must** put it in a structured field of
  its own, reviewed on its own terms. It must not be appended to the detail.
- `&'static str` is **not** sufficient and must not be proposed as the fix. `Box::leak` promotes
  any `String` to `&'static str`; a lifetime is not a provenance.

**The cost is real and is not softened.** An operator reading `drain_in_progress` does not learn
which operation was draining. That is the price of the rule, and the place to pay it is a reviewed
structured field — not a free string that happens to be benign today.

## Correlation

The `correlationId`, the `x-request-id` response header, and the telemetry field are rendered from
**one** `RequestId` on the request context. Not a derivation, and not a second identity — which is
what makes them agree without anything having to keep them in step.

When a request identifier could not be generated at all — the host refused entropy, in which case
the failure is already internal — the correlation is the literal `unavailable`. A **placeholder**
rather than a fabricated identifier: an invented one would let an operator search for a request that
never had that identity.

## Every route answers this way

Including the ones no handler reached. A `404` for an unmatched path and a `400` for a rejected host
are Problem Details documents carrying a correlation identifier, exactly as a handler's refusal is.
Phase 004 answered both as plain text; a machine-readable failure that stopped at the router's edge
would be machine-readable only where it was least needed.
