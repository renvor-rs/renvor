---
description: "Phase 004 — the frozen twelve-finding remediation ledger"
status: "working record. The twelve were frozen at head dad8333 before any remediation edit was made; the disposition below was appended after they were fixed and measured"
---

# Phase 004 — frozen finding ledger

**Frozen at**: `dad8333c2efbdf4d88293f5ac2a9ed81f217a0ec`, working tree clean, before any
remediation edit.

**Authority**: maintainer instruction of 2026-08-22 — *"Absorb all twelve remaining substantive
findings into Phase 004. Do not create Phase 004a and do not transfer or downgrade any Phase 004
requirement."*

## Scope rules this ledger operates under

- **No requirement leaves Phase 004.** Nothing here is deferred, re-scoped, or downgraded.
- **No finding is closed by an implementation agent.** `todo` → `coding_done` is the implementer's
  furthest reach; `coding_done` → `validated` is the validation agent's, independently.
- **Every fix is preceded by a demonstration.** Either the failure is executed and recorded, or a
  test is added that fails without the fix. A fix with no prior failing observation is not a fix,
  it is a claim.
- **ADR-0012 stays `proposed`.** No acceptance, no W-009, no merge, no phase closure without
  explicit maintainer authority.

## The twelve

Severity is as recorded at freeze. `L-12` is composite because its parts share one root — the
contract surface documents behaviour the implementation does not fully carry — and splitting it
would have produced seven rows that are each one edit.

### L-01 — `renvor routes` has no success path

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-034, FR-036, SC-019; C-9 *Route inspection*; C-1 `renvor routes` |
| **Files** | `crates/renvor-cli/src/commands/routes.rs` |
| **Reproduction** | Every path through `run()` returns `Err`. A project that declares the dependency and answers the dump still receives `transport_not_wired` / `dump_unavailable`, because the relay is not written |
| **Intended fix** | Implement the explicit versioned project-binary metadata protocol: build the project binary, invoke it with `--renvor-dump-routes`, parse the single C-2 envelope, relay `result.routes`. No second manifest, no source parsing, no automatic binary discovery, no application boot side effects |
| **Regression test** | A fixture binary that answers the dump is relayed to exit `0` with the routes it declared; a binary that answers nothing still fails; a binary that answers a malformed or wrong-version envelope fails with a named reason rather than an empty success |
| **Batch** | G |

### L-02 — `Server::serve` does not bound shutdown

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-032; C-10 *Drain is bounded*; C-L7 |
| **Files** | `crates/renvor-http/src/server.rs` |
| **Reproduction** | `tokio::join!(drain, serving)` waits for **both**. `serving` is `axum::serve(..).with_graceful_shutdown(..)`, whose own wait is unbounded — so one connection that never finishes hangs shutdown for ever, and the bounded drain's outcome is computed but never reached |
| **Intended fix** | Stop admission, stop accepting, drain under the budget, and return the drain's outcome without waiting on the unbounded half. The serving task is abandoned once the budget elapses, and that is reported rather than hidden |
| **Regression test** | Normal completion → `Clean`; deadline expiry with work in flight → `Incomplete { outstanding }`; a connection that never completes does **not** extend shutdown beyond the budget; no request is admitted after the gate closes; cancellation reaches in-flight requests |
| **Batch** | F |

### L-03 — the server is not a kernel provider

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-033; C-10 *The server is a provider* |
| **Files** | `crates/renvor-http/src/server.rs`, new `crates/renvor-http/src/provider.rs` |
| **Reproduction** | No `impl renvor_core::Provider` exists anywhere in the crate. The server binds and serves outside `Boot`/`Ready`/`Drain`/`Stop`, contributes nothing to readiness, and a bind failure therefore cannot roll back as a provider failure |
| **Intended fix** | A `HttpServerProvider` implementing `Provider`: binds in `initialise`, registers a readiness contributor, drains and stops in `stop`, in the order C-L1/C-L3 already fix |
| **Regression test** | The provider boots inside a real `Application`; readiness reports ready only once bound; a bind failure aborts Boot and rolls back; shutdown drains **before** a dependency provider stops, observed by ordering rather than asserted from the contract |
| **Batch** | A |

### L-04 — `TypedStateMap` is never bridged

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-012, FR-013; SC-003 |
| **Files** | `crates/renvor-http/src/route/{mod,build}.rs`, `crates/renvor-http/src/context.rs` |
| **Reproduction** | Neither `RouterConfig`, `RequestContext`, nor `Request` carries a `TypedStateMap` or exposes a typed lookup. A handler cannot reach registered application state at all, so FR-012 has no implementation and FR-013 has nothing to report on |
| **Intended fix** | Application state reaches the router as a shared `TypedStateMap`; `Request::state::<T>()` returns `Result`, surfacing the kernel's own `StateMissing` as a reported failure. Never a panic, never a silent default |
| **Regression test** | A registered value is retrievable through a real request; a missing type is an explicit error with the type named; a type-incompatible lookup is the same error rather than a wrong value; **no `axum` type appears** in the state path |
| **Batch** | A |

### L-05 — group-scoped middleware does not exist

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-008; SC-003; C-9 *Group middleware*, *Group prefix* |
| **Files** | `crates/renvor-http/src/route/mod.rs`, `crates/renvor-http/src/route/build.rs` |
| **Reproduction** | `RouteGroup` stores a name, a prefix, and routes, and nothing else. C-9 promises *"a group's middleware applies to every route it contains, and to no route outside it"* and that *"nested groups compose left to right"*. Neither exists |
| **Intended fix** | A transport-neutral group middleware seam, applied to exactly the group's routes, composing outward-to-inward, with nested groups composing their prefixes and their middleware in declaration order |
| **Regression test** | Middleware runs for a route inside the group and **not** for one outside it; two nested groups apply both, outer first, observed by an ordering effect on a real `Router`; a middleware that refuses short-circuits before the handler |
| **Batch** | C |

### L-06 — the CORS protocol is not implemented

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-021, FR-022; SC-007; C-11 *CORS* |
| **Files** | `crates/renvor-http/src/cors.rs`, `crates/renvor-http/src/route/build.rs` |
| **Reproduction** | Verified at freeze: an **allowed** origin receives `200` with **no** `access-control-allow-origin` header, so a browser blocks the response the policy permits. No `CorsLayer` is ever constructed; a preflight `OPTIONS` is routed as an ordinary request or 405s |
| **Intended fix** | Emit the actual response headers for an allowed origin, answer preflight before admission spends a permit on it, honour credentials exactly as configured, keep deny-by-default and exact matching, and keep the same-origin write carve-out |
| **Regression test** | Allowed origin → `200` **with** `access-control-allow-origin` matching exactly; disallowed origin → refused, header absent; preflight answered with allowed methods and headers; credentialed policy emits `access-control-allow-credentials`, wildcard policy never does; same-origin write with no CORS configuration still succeeds; `Vary: Origin` present so a cache cannot serve one origin's response to another |
| **Batch** | B |

### L-07 — router fallbacks bypass every request control

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-011, FR-027, FR-028; C-11 *Middleware order*; C-11 *Fail-open / fail-closed summary* |
| **Files** | `crates/renvor-http/src/route/build.rs` |
| **Reproduction** | Verified at freeze: `GET /missing` with `Host: evil.example` → **404** with `x-request-id` **absent**. The 404 and 405 paths are axum's own fallbacks, reached without host validation, identity resolution, request-ID generation, CORS, limits, timeout, or admission |
| **Intended fix** | Fail closed: every response leaves through the same controlled path, so an unmatched path or method is decided **after** the controls, not instead of them. `Allow` on 405 is preserved |
| **Regression test** | An unknown path with a disallowed host returns the **host** rejection, not 404; an unknown path with an allowed host returns 404 **carrying** `x-request-id`; a 405 carries both `Allow` and the identifier; a fallback response is refused during drain like any other |
| **Batch** | D |

### L-08 — the timeout does not bound body processing

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-025; C-11 *Middleware order* rows 7 and 8; C-10 *Limits and timeouts* |
| **Files** | `crates/renvor-http/src/route/build.rs` |
| **Reproduction** | `to_bytes(..)` is awaited **before** `tokio::time::timeout(..)` wraps the handler. A client that opens a request and then sends its body one byte per minute holds an admission permit and a concurrency slot indefinitely, and no documented bound fires |
| **Intended fix** | The timeout bounds the **complete** request path — body read and handler together — as C-11's ordering states, while remaining inside admission so a timed-out request still releases its permit |
| **Regression test** | A stalled body times out at the documented bound with `408` and releases both bounds; a body that arrives inside the bound still succeeds; the timeout still cancels the request scope; over-limit bodies still return `413` rather than `408` |
| **Batch** | E |

### L-09 — client disconnect does not cancel the request scope

| | |
|---|---|
| **Severity** | P1 |
| **Requirement** | FR-031; SC-009; C-10 *Cancellation reaches the application* |
| **Files** | `crates/renvor-http/src/route/build.rs` |
| **Reproduction** | Only the timeout path calls `scope.cancel()`. C-10 states *"client disconnect and request timeout both cancel that scope, so an application service sees one mechanism rather than two"*; a disconnected client leaves the handler running with an uncancelled scope |
| **Intended fix** | Dropping the dispatch future cancels the request scope, so a disconnect and a timeout reach an application service through the identical mechanism |
| **Regression test** | A dispatch future dropped mid-flight leaves the scope cancelled; a completed request does **not** leave it cancelled; the observing application service sees a kernel `CancelScope` and no transport type |
| **Batch** | I |

### L-10 — handler panics are not contained

| | |
|---|---|
| **Severity** | P2 |
| **Requirement** | C-10 *Errors and telemetry* — *"a handler panic is caught, contained, and reported as a failure. It is never a hang, and its payload never reaches a response"* |
| **Files** | `crates/renvor-http/src/route/build.rs` |
| **Reproduction** | No `catch_unwind` boundary exists. A panicking handler aborts the connection task; the caller sees a dropped connection rather than a reported failure, and admission accounting depends on unwinding reaching the guard |
| **Intended fix** | A catch boundary around the handler call, reporting a contained failure with the request identifier and **without** the panic payload in the response |
| **Regression test** | A panicking handler yields a `500` carrying the identifier; the payload appears in **no** response byte; the admission permit and concurrency slot are both released; a non-panicking handler is unaffected |
| **Batch** | I |

### L-11 — path parameters are never populated

| | |
|---|---|
| **Severity** | P2 |
| **Requirement** | C-9 *Registration* / *Path shape*; `Request::path_param`'s own documented contract |
| **Files** | `crates/renvor-http/src/route/build.rs`, `crates/renvor-http/src/route/mod.rs` |
| **Reproduction** | `Request::new(context, collected, query, BTreeMap::new())` — the map is always empty, so `path_param` returns `None` for every parameter of every route, including ones whose pattern declares them |
| **Intended fix** | Capture the matched parameters from the real router and hand them to the request |
| **Regression test** | A route declaring a parameter receives its captured value through a real `Router`; a route declaring none receives an empty map; a parameter value is **not** trusted as anything but a string |
| **Batch** | I |

### L-12 — contract-surface hygiene (composite)

| | |
|---|---|
| **Severity** | P2 (one LOW part, marked) |
| **Requirement** | C-9 *Path shape*; C-11 *Middleware order* row 9; C-11 *Response and telemetry content*; C-10 *Errors and telemetry*; SC-012, SC-017 |
| **Files** | `crates/renvor-http/src/route/{mod,registry,build}.rs`, `crates/renvor-http/tests/boundary.rs` |
| **Reproduction** | Seven parts, each observed at freeze: (a) `validate_path` accepts a malformed pattern that then panics at router construction; (b) nested groups do not compose; (c) no trace span covers handler execution, so C-11 layer 9 is unimplemented; (d) refusal telemetry omits the run identifier on some paths; (e) `Response` status and header metadata are unvalidated, so a handler can return a status no HTTP stack accepts; (f) `tests/boundary.rs` scans a four-file hand-list, so a new application-facing module is silently unscanned; (g) **LOW** — every body error reports `413`, including IO errors and client disconnects |
| **Intended fix** | (a) validate the pattern the router will actually receive; (b) compose nested groups; (c) a span around the handler, structured fields only; (d) the run identifier on every refusal record; (e) validate status and header metadata at the only construction site; (f) scan by directory rather than by hand-list; (g) distinguish over-limit from IO failure |
| **Regression test** | One per part, each with a positive control: a malformed pattern is a registration error rather than a construction panic; two nested groups produce the composed prefix; the span is observable; a refusal record carries the run id; an out-of-range status is refused at construction; the boundary scan finds a transport type injected into a **newly added** file; an IO error is not reported as `413` |
| **Batch** | J |

## Batch H — regression coverage for an already-fixed defect

Not one of the twelve. Defect **F-08** (`renvor check` rejected every Phase 003 project) was fixed
in `bba4546` and is covered by a unit test over a synthetic manifest. The maintainer instruction
requires a regression test **using an actual Phase 003 fixture**, which the current test is not.

| | |
|---|---|
| **Requirement** | FR-037, FR-038; C-1 `--transport` |
| **Files** | `crates/renvor-cli/src/commands/check.rs`, `crates/renvor-cli/tests/` |
| **Intended fix** | No production change expected. If the fixture proves otherwise, that is a new finding and is recorded as one |
| **Regression test** | A manifest generated by the Phase 003 generator — recovered from the Phase 003 tag or template, not hand-written to match — passes `renvor check`. A Phase 004 manifest also passes, as a positive control |

## Batch map

| Batch | Findings | Theme |
|---|---|---|
| A | L-03, L-04 | Provider implementation and application-state bridge |
| B | L-06 | CORS behaviour |
| C | L-05 | Group middleware and observable order |
| D | L-07 | Remove control-bypassing fallbacks |
| E | L-08 | Timeout bounds the complete path |
| F | L-02 | Genuinely bounded shutdown |
| G | L-01 | `renvor routes` metadata protocol |
| H | — | Phase 003 compatibility regression coverage |
| I | L-09, L-10, L-11 | Request-lifetime correctness |
| J | L-12 | Contract-surface hygiene |

Batches I and J exist because the maintainer instruction names eight batches and twelve findings.
The four findings the lettered batches do not name are **not** dropped and **not** deferred; they
are grouped here so every one of the twelve has a home.

## Status

Every row started at `todo`. The disposition below records where each one actually ended.

## Disposition — recorded 2026-08-22

**Measured, not asserted.** Every row was demonstrated red before it was fixed: with the remediation
stashed, the new tests were run against the pre-fix code and **12 failed, 1 hung** (L-08's unbounded
body read — the defect expressed as a symptom rather than an assertion). Four tests passed on both
sides; those are positive controls and are supposed to.

> An earlier claim that all twelve were red was **wrong**. It came from a `timeout` invocation that
> does not exist on this shell, so the command failed and its non-zero exit was misread as a test
> result. The figure above comes from a re-run with a working harness.

| # | Severity | Batch | Task | Status | Commit | Regression evidence |
|---|---|---|---|---|---|---|
| L-01 | P1 | G | #48 | `coding_done` | route-dump relay | 14 tests, incl. end-to-end `the_relay_reads_what_the_real_library_actually_prints` |
| L-02 | P1 | F | #49 | `coding_done` | bounded shutdown | 4 tests in `tests/shutdown.rs` over a real socket |
| L-03 | P1 | A | #50 | `coding_done` | `HttpServerProvider` | 5 tests in `tests/provider.rs`; ordering read from recorded events |
| L-04 | P1 | A | #51 | `coding_done` | state bridge | 3 tests in `tests/state.rs` |
| L-05 | P1 | C | #52 | `coding_done` | group middleware | 5 tests in `tests/groups.rs` |
| L-06 | P1 | B | #53 | `coding_done` | CORS behaviour | 7 CORS tests in `tests/controls.rs` |
| L-07 | P1 | D | #54 | `coding_done` | fallback removal | 4 tests in `tests/controls.rs` |
| L-08 | P1 | E | #55 | `coding_done` | timeout placement | 2 tests, one the control |
| L-09 | P1 | I | #56 | `coding_done` | `CancelOnDrop` | `dropping_a_request_in_flight_cancels_its_scope` |
| L-10 | P2 | I | #57 | `coding_done` | panic boundary | 2 tests, one the control |
| L-11 | P2 | I | #58 | `coding_done` | path parameters | `a_path_parameter_reaches_the_handler` |
| L-12 | P2 | J | #59 | `coding_done` | contract hygiene | one per part, each with a control |

**Batch H** (not one of the twelve) is covered by 4 tests in `crates/renvor-cli/tests/legacy_compatibility.rs`
against a fixture generated by the **Phase 003 CLI itself**, built from `10da854`, with its
provenance recorded beside it. The test was proven to catch its regression by temporarily restoring
`transport` to required and observing the exact historical failure.

### `coding_done` is the ceiling reached here

No row above is `validated`, and none is `complete`. Under the five-status rule an implementation
agent's furthest reach is `coding_done`; only an independent validation run may move a row onward,
and `complete` is a human decision that has not been taken.

### Three defects the remediation exposed

Not among the twelve, found while fixing them, and recorded rather than folded in silently:

| Found | Disposition |
|---|---|
| `InitContext` carried no run identifier | Added `InitContext::run_id()` to the kernel — a provider otherwise had no way to nest telemetry under the run identity (C-O3), and minting its own would have created two identities for one run |
| An application `OPTIONS` route was silently shadowed by the CORS layer | Verified: empty `200`, handler never ran. Registration is now a reported error (`MethodReservedByTransport`), and C-9 records the rule |
| Preflight returns `200`, not `204` | The **test** was corrected. ADR-0012 records that Renvor writes none of the CORS protocol, so the status is the selected library's |
