# ADR-0015: Publish a transport-independent API error registry, and write the RFC 9457 model in Renvor

| Field | Value |
|---|---|
| **ID** | 0015 |
| **State** | `proposed` |
| **Reviewer** | *(none — required to enter `accepted`)* |
| **Review date** | *(none — required to enter `accepted`)* |
| **Superseded by** | *(not superseded)* |

> **This record is `proposed` and nothing else.**
>
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded **independent**
> review before acceptance. No independent human review has occurred, and none is claimed. **No
> Phase 005 waiver exists**, and none is created here.

## Context

PLAN.md §11.1 requires *"RFC 9457 `application/problem+json` errors with stable Renvor error codes,
correlation identifiers, safe detail, and invalid-parameter extensions."*

Two questions follow, and they are separable:

1. **Where does the code vocabulary live?** A stable error code is a compatibility promise. The
   project already has one closed registry, for the command line, in
   [`contracts/json-output.md`](../contracts/json-output.md).
2. **Who writes the RFC 9457 document model?**

## The package research

| Candidate | Version | Date | License | Verdict |
|---|---|---|---|---|
| `problem_details` | 0.10.0 | 2026-08-16 | MIT OR Apache-2.0 | **Rejected** — see below |
| `problemdetails` | 0.7.0 | 2026-04-23 | MIT | Rejected — axum-bound by design |
| `http-api-problem` | 0.60.0 | 2025-01-06 | Apache-2.0/MIT | Rejected — RFC **7807**; last release 19 months ago |
| `http-problem` | 0.3.0 | 2023-05-11 | MIT OR Apache-2.0 | Rejected — unmaintained since 2023 |
| `rfc9457` | 0.1.0 | 2026-05-06 | MIT | Rejected — 17 downloads; no evidence of use |

`problem_details` 0.10.0 is maintained, correctly licensed, and small — four new packages with
`default-features = false`. It was rejected on **one measured fact**:

```rust
let p = problem_details::ProblemDetails::new().with_status(http::StatusCode::BAD_REQUEST);
let _: Option<http::StatusCode> = p.status;   // compiles
```

Its core `status` member **is** `Option<http::StatusCode>`, and the crate depends on `http`
unconditionally even with default features off.

## Decision

### 1. The public API error registry is its own crate, and it names no transport

`renvor-error` depends on `serde` and `serde_json`, and on nothing else. **No status-code type, no
header type, no router, and no kernel type either.**

A code is a promise about the API. A promise that lives in the HTTP adapter is a promise about HTTP,
and constitution principle II forbids a transport type reaching inward.

The mapping from a code to an **HTTP status** lives in `renvor-http`, because a status code *is*
transport semantics.

**Release consequence, worth stating**: `renvor-error` depends on no Renvor crate at all, so it sits
at **position 1 in the publication order, beside the kernel**. That is not a coincidence — it is
what "transport-independent" means when it is true rather than intended.

### 2. The registry versions independently of the command line's

Twelve codes, version `1`. The two vocabularies are **disjoint** — no name appears in both — and a
test asserts that rather than an author remembering it.

Sharing one version integer would mean a code added for `renvor doctor` bumped a version REST
consumers pin. Sharing one *vocabulary* would be worse: it would tie an HTTP response shape to a
shell exit convention, so neither could change without the other.

Two pairs read as if they were the same thing, and are named in the contract because they are not:
`internal` (the **tool** has a defect) against `internal_error` (the **application** does), and
`bound_exceeded` (a bound the **CLI** applied) against `payload_too_large` (a bound the **server**
applied).

**There is no conversion function, deliberately.** One would be an invitation to use it, and every
use would leak one vocabulary into the other.

### 3. The RFC 9457 model is roughly 150 lines of Renvor `serde` code

With **zero** new dependencies, and three guarantees the type system enforces rather than review:

| Guarantee | How |
|---|---|
| `detail` cannot carry runtime data | it is `&'static str` — no runtime value inhabits the type |
| An invalid parameter cannot carry the rejected value | the type has **no field** one could occupy |
| A reason cannot be a library's message | it is `&'static str`, so a formatted string cannot be stored |

The third exists because a real validator produced `"not an object" is not of type "object"` during
this phase's tooling work — the rejected value is inside the message. A generic extension map, which
every candidate crate offers, would be a channel through which exactly that reaches a response.

## Consequences

### Redaction does not discard the cause

`renvor-http`'s `HttpError` already carries an operator-facing `detail` alongside a caller-facing
kind, and the function that renders the response **cannot read** the operator-facing half. The cause
reaches telemetry; the caller gets a classification. Both audiences are served, neither by accident.

### Every route answers this way, including the ones no handler reached

A `404` for an unmatched path and a `400` for a rejected host are Problem Details documents carrying
a correlation identifier. Phase 004 answered both as plain text; a machine-readable failure that
stopped at the router's edge would be machine-readable only where it was least needed.

### The ownership cost, stated

Roughly 150 lines of document model plus a 12-row table. It must track RFC 9457 if the RFC is
revised — which, being a Proposed Standard published in 2023, is not imminent.

### The exit strategy

**Deletion trigger.** If a maintained RFC 9457 crate appears whose core model names **no** transport
type and whose extension mechanism is typed rather than an open map, Renvor's model is replaced and
this record is superseded.

The acceptance test for a replacement is the existing conformance and leak suite in
`crates/renvor-http/tests/problem_details.rs`, which asserts the wire shape, the status agreement,
the correlation identity, and the absence of a canary from every failure path.

**The registry itself is not subject to that trigger.** It is a Renvor compatibility surface, and no
third party can own it.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| `problem_details` 0.10.0 | Its core `status` is `Option<http::StatusCode>` — measured by compiling it. A transport type in a crate that must work with no transport present |
| Put the codes in `renvor-http` | Makes a compatibility promise about the API into a promise about HTTP, and makes the registry unusable by a future GraphQL adapter that must produce parity errors (PLAN.md §11.2) |
| Extend the command-line registry | Ties an HTTP response shape to a shell exit convention. Neither could then change without the other |
| An open extension map | Every candidate crate offers one, and it is the channel through which an unreviewed value reaches a response |
| `detail` as a `String` | Makes interpolating a rejected value *possible*, so the guarantee would rest on nobody ever doing it. `&'static str` makes it impossible |
