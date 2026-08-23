# ADR-0012: Implement five custom HTTP primitives where maintained packages are unfit

| Field | Value |
|---|---|
| **ID** | 0012 |
| **State** | `proposed` |
| **Reviewer** | *(none — see the acceptance note below)* |
| **Review date** | *(none)* |
| **Superseded by** | *(not superseded)* |

> **This record is `proposed` and MUST NOT be marked `accepted` on the current evidence.**
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded
> **independent** review before acceptance. No existing waiver confers that authority here:
> **W-002** covers Phase 001 decision records; **W-004** names **ADR-0007 alone**; **W-006** names
> **ADR-0009 alone**; **W-005** and **W-008** are *phase-level* and their own scope sections state
> they do **not** authorise accepting any decision record. None is extended by reinterpretation.
> A new waiver is **not** created here — that is Ahmed's decision, not this record's.

## Context

Phase 004 delivers Renvor's first real transport: an HTTP/REST adapter over the Phase 002 kernel.

Constitution principle III is binding and specific:

> *"Renvor MUST NOT create a custom runtime, HTTP engine, ORM, cryptographic primitive, queue,
> **parser**, template engine, frontend platform, or desktop security mechanism merely to own the
> implementation. Custom infrastructure requires an accepted ADR documenting evaluated packages,
> their concrete shortcomings, ownership cost, and an exit strategy."*

The phase's package research selected `axum` 0.8.9, `tower` 0.5.3, `tower-http` 0.7.0, `tokio`
1.53.1, `tracing` 0.1.44, `serde` 1.0.229, `serde_json` 1.0.151, and `http-body-util` 0.1.5 — the
whole HTTP engine, router, and middleware framework. **Renvor writes none of that.**

Five narrow behaviours remain, and for each one the maintained option was read **at the source
level** and found unfit. The shortcomings below are quoted from vendored crate source in the local
registry, not recalled.

### Finding 1 — `tower-http`'s request-id layer adopts caller-supplied identifiers

`tower-http-0.7.0/src/request_id.rs:330`:

```rust
fn call(&mut self, mut req: Request<ReqBody>) -> Self::Future {
    if let Some(request_id) = req.headers().get(&self.header_name) {
        if req.extensions().get::<RequestId>().is_none() {
            req.extensions_mut().insert(RequestId::new(request_id.clone()));
        }
    } else if let Some(request_id) = self.make_request_id.make_request_id(&req) {
        ...
    }
```

`SetRequestId` adopts an inbound header as *the* `RequestId` and generates one **only when the
header is absent**. The crate exposes **no overwrite option**. A caller therefore chooses the
identifier under which its own request is logged, correlated, and audited.

### Finding 2 — the CORS misconfiguration guard is a panic, reachable at request time

`tower-http-0.7.0/src/cors/mod.rs:820` implements `ensure_usable_cors_rules` with `assert!`, and it
is invoked from **two** places: `Layer::layer` (line 527) and **`Service::poll_ready` (line 695)**.
A wildcard-plus-credentials configuration therefore panics a **running server**, rather than being
refused when the configuration is built.

*(The same file also confirms the good half: `CorsLayer::new()` **is** deny-by-default —
`OriginInner::default()` is `List(Vec::new())`. The layer is kept. Only the validation is taken
over.)*

### Finding 3 — `axum`'s graceful shutdown waits without any bound

`axum-0.8.9/src/serve/mod.rs`, end of `WithGracefulShutdown::run`:

```rust
drop(close_rx);
drop(listener);
close_tx.closed().await;      // no timeout
```

Contract **C-L7** states: *"Every kernel-owned wait MUST be bounded. An unbounded wait in a
kernel-owned path is a defect, not a configuration choice."* Using this alone would put a defect,
by the project's own definition, on the shutdown path.

### Finding 4 — no maintained crate implements a trusted-proxy model

| Crate | Version | Maintenance | Why unfit |
|---|---|---|---|
| `axum-client-ip` | 1.3.1 | active (2026-01-22), MIT, **no declared `rust-version`** | Its extractors — `RightmostXForwardedFor`, `CfConnectingIp`, `TrueClientIp`, `XRealIp` — parse the header **unconditionally**. There is **no trusted-proxy concept anywhere in the crate**. It answers *"parse this proxy's header"*; Renvor must answer *"should this header be trusted at all?"* |
| `forwarded-header-value` | 0.1.1 | **last released 2021-08-18**, ISC, no declared `rust-version` | RFC 7239 syntax parsing only, again with no trust model. Five years stale and MSRV-undeclared on a **security-critical** path |
| `real_ip` | 0.1.1 | 2025-10-10, low adoption | Same class; no trust model |

Adopting any of them would make Renvor's **default** configuration derive client identity from
attacker-controlled input.

### Finding 5 — `tower-http` ships no Host validation

`tower-http-0.7.0/src/` contains no host validator. `validate_request.rs` is a generic
header-validation helper, not a host policy.

## Decision

**Renvor implements five narrow primitives inside `crates/renvor-http`, and nothing else.**

| # | Primitive | Scope — deliberately minimal |
|---|---|---|
| 1 | **Trusted-proxy client-identity resolver**, including RFC 7239 `Forwarded` and `X-Forwarded-For` parsing | Empty trusted set by default. Forwarding headers ignored unless the direct peer is in the explicit set. Malformed, ambiguous, or control-character-bearing input resolves to the direct peer and is **not** attributed |
| 2 | **Always-generate request-ID layer** | Generates on every request. An inbound header value is never promoted to trusted identity |
| 3 | **Fail-closed Host validation** | Validates against explicit configuration; absent, empty, or unmatched fails closed |
| 4 | **CORS configuration validator** | Returns a **typed error** for wildcard-plus-credentials before any `CorsLayer` is constructed. `tower_http::cors::CorsLayer` still does the protocol work |
| 5 | **Bounded drain wrapper** | Uses `axum`'s shutdown signal for the *stop-accepting* half; bounds the wait with `renvor_core::WorkGate::drain(budget)` and reports `DrainOutcome` |

**What is explicitly NOT custom**: the HTTP engine, the router, request parsing, the CORS protocol
implementation, the body-limit mechanism, the timeout mechanism, the concurrency limiter, tracing,
and serialisation. All are the selected packages'.

## The public surface this decision fixes

**Decided 2026-08-23**, as part of closing the post-remediation requirements review's finding R-10.

The five primitives above are Renvor's. The question this section settles is a different one: which
**names Renvor promotes to its own crate root**, where an application reaches for them without
knowing which transport it was given.

**No facade-root name may expose `axum`, `tower`, or `hyper` in a public callable signature.**

The rule was already written in the facade's rustdoc — *"re-exporting a third-party type would put
it in Renvor's public API and make every upstream major version a Renvor breaking change"* — and it
was **false in the shipped surface**. `Server` sat at the facade root, and `Server::serve` takes the
underlying router **by parameter**. The existing transport scan could not see it: that scan lives in
`renvor-http` and exempts `server.rs` as a module which *is* the transport. The exemption was right
for that scan and blind for this one.

| | |
|---|---|
| **Normal path, at the root** | `HttpServerProvider`, `HttpServerConfig`, `RouteRegistry`, `RouteGroup`, `Route`, `Method`, `Response`, `RequestContext`, `ClientIdentity`, `HostPolicy`, `CorsPolicy`, `TrustedProxies`, `Limits`, `Admission` — every one Renvor-owned. The application hands Renvor its routes; Renvor owns the bind, the readiness report, the drain budget, and the shutdown ordering |
| **Escape hatch, one level down** | `renvor::transport::Server` and `renvor::transport::route::build`. Naming a transport type inside the module that **is** the transport is the module's purpose, not a leak. A caller reaching there is choosing the upstream-version coupling deliberately |
| **Enforced by** | `crates/renvor/tests/facade_boundary.rs` — parses the re-export list from the source rather than hand-listing it, resolves each name to its defining `impl` blocks, and reads the declared signatures |

**The trade-off, stated rather than implied.** Removing `Server` from the root makes the raw path
one segment longer and slightly less discoverable. An author who wants to drive the router directly
must write `renvor::transport::Server` and, in doing so, name the transport they are coupling to.
That friction is the point: the cost of the coupling should be visible at the call site rather than
absorbed by Renvor's semantic version. The alternative — keeping the ergonomic root name — buys a
shorter import and pays for it with a public API whose stability is not Renvor's to promise.

**Not done, deliberately**: the router and the route registry are **not** duplicated, and no second
route manifest exists. Wrapping `Router` in a Renvor-owned opaque newtype was considered and
rejected — no normative requirement asks for a facade-root `Server`, so the wrapper would add a type
whose only purpose is to make an unnecessary re-export legal.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Adopt `axum-client-ip` for client identity | Finding 4. It has no trusted-proxy model, so the **default** configuration would trust caller-supplied headers — the exact defect FR-017–FR-019 exist to prevent. This is a security regression, not a trade-off |
| Adopt `forwarded-header-value` for RFC 7239 parsing only | Finding 4. Unmaintained since 2021 and declares no MSRV. A stale, MSRV-undeclared dependency on the security-critical parsing path carries more risk than the ~200 lines it saves, and ADR-0003's dependency policy weighs maintenance explicitly |
| Use `SetRequestIdLayer` and overwrite the header before it | Finding 1. It would work only by adding a layer that strips the header first — which *is* a custom primitive, one that additionally depends on an upstream ordering guarantee that is not documented and could change |
| Rely on `tower-http`'s CORS assertion as the refusal | Finding 2. It is a panic, and it is reachable from `poll_ready` — i.e. at request time on a running server. FR-022 requires refusal **at configuration time** |
| Use `with_graceful_shutdown` alone | Finding 3. C-L7 defines an unbounded wait in a kernel-owned path as a defect. Also `DrainOutcome::Incomplete { outstanding }` could never be produced, so FR-007's prohibition on reporting an incomplete drain as clean would be unobservable |
| Contribute the trust model upstream and wait | Correct long-term, and it is the exit strategy below — but it blocks Phase 004 on a third party's review cycle. The phase would stall with no transport, leaving C-S1 condition (a) unsatisfied indefinitely |
| Ship without a trust model and document the hazard | Prohibited. Constitution principle VI requires deny-by-default, and a framework whose default lets a caller pick its own source address produces authorization and audit decisions from attacker-supplied data. Principle III also forbids letting a package failure be silently absorbed |
| Skip Host validation because tower-http has none | Absence upstream is not an argument that the control is unnecessary. FR-020 requires it, fail-closed |

## Consequences

### Accepted costs

- **Renvor owns a security-critical parser.** RFC 7239 and `X-Forwarded-For` parsing is now Renvor's
  to maintain, test, and fix. Parsing bugs here are **security** bugs, and this is the single
  largest cost in this record.
- **Five more surfaces can fail**, and five more must be tested at their boundaries.
- **Upstream improvements do not arrive for free.** If `tower-http` later gains a trust model,
  Renvor will not benefit until it migrates deliberately.
- **The public API grows** by the configuration types these primitives need, and that surface is
  under the C-S1 instability window — which this record does **not** close.

### What becomes harder

Changing the identity model later touches Renvor code rather than a dependency bump. Any future
contributor changing `identity/forwarded.rs` is changing a security boundary and must be told so —
the module documentation says it in the file itself, not only here.

### Exit strategy

Each primitive is retired independently, and each has a **named, checkable trigger**:

| # | Retire when |
|---|---|
| 1 | A maintained crate (released within 18 months, **declaring a `rust-version` ≤ Renvor's MSRV**, on the `deny.toml` allow-list) implements trusted-proxy-aware resolution where the trust decision is made from the **direct peer** |
| 2 | `tower-http` gains a documented always-overwrite mode for `SetRequestId` |
| 3 | `tower-http` ships a fail-closed Host validation layer |
| 4 | `tower-http` returns a `Result` from CORS configuration instead of asserting, and removes the `poll_ready` assertion |
| 5 | `axum::serve` accepts a bounded shutdown deadline and reports outstanding work |

Each primitive is confined to one module so that retiring it is a deletion plus a re-wire, not an
excavation. The trigger conditions are written as **observable upstream facts**, so this record can
be checked rather than argued.

## Compliance

| Authority | How this satisfies it |
|---|---|
| **Principle III — package-first** | Eight packages selected and used for everything they cover. Seven candidates evaluated and rejected **with concrete, source-quoted shortcomings**. Ownership cost stated. Exit strategy has checkable triggers |
| **Principle VI — deny by default** | Empty trusted-proxy set, deny-by-default CORS, fail-closed host validation — the reason four of the five primitives exist |
| **Principle IX — evidence** | Every finding cites a file and line in vendored source. Every primitive gets boundary tests with positive controls |
| **Principle XII — visible limitations** | The ownership cost is stated above rather than only the benefit. The header-bound limitation Renvor does **not** control is named in the runtime contract |
| **Contract C-L7** | Finding 3 is the reason primitive 5 exists |
| **Contract C-S1** | This record does **not** close the instability window and does **not** supersede ADR-0002. Condition (b) is untouched |
| **FR-035 / spec FR-013** | This record is the required ADR. It stays `proposed` until an independent review exists or Ahmed grants explicit authority |

## Acceptance note

**No independent human review of this record has occurred.** Automated agents and Codex reviews are
**advisory and explicitly non-independent**, and must never be described otherwise — in this record,
in the evidence pack, in `GOVERNANCE.md`, or in any public document.

Acceptance requires **one** of:

1. a genuine qualified independent human requirements-and-security review — a person, not the
   author, competent in the subject, able to reject without the author's consent; or
2. Ahmed's **explicit** authorisation for a new, truthful, time-bounded waiver carrying compensating
   controls, the earliest applicable expiry under this ledger's ratchet rule, a removal plan, and
   the trend-guard consequences that a **fourth** consecutive single-maintainer review waiver
   entails.

Until one exists, this record stays `proposed`, the Phase 004 pull request does **not** merge, and
Phase 004 is **not** marked complete.
