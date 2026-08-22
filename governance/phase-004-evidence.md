# Phase 004 evidence — REST routing and HTTP runtime

**Base**: `10da854736598d99218d1627c3ad79866a2f7f89` · **Branch**: `feat/phase-004-rest-http-runtime`
**Date**: 2026-08-22

> **Phase 004 is NOT closed, and the implementation is NOT complete.** This is the evidence pack,
> not a completion record.
>
> **Three advisory reviews found defects against the implementation** — an independent-model review
> (Codex) and two agent reviews that eventually delivered. Between them: **19 + 13 + 16 findings**,
> heavily overlapping. **Eight were verified by executing them**, **eight were fixed**, and
> **twelve substantive findings remain open**.
>
> Several requirement rows below were **corrected downward** as a result: this document previously
> claimed requirements met that the reviews disproved. The corrections are in
> [§Findings against the implementation](#findings-against-the-implementation).

## What the phase delivered

Renvor's **first real transport**: `renvor-http`, a REST and HTTP delivery adapter over the Phase
002 kernel. It carries a single authoritative route registry that feeds both the real router and
route inspection, a versioned middleware order proven by behaviour, trusted-proxy client identity,
fail-closed host validation, deny-by-default CORS validated at configuration time, documented limits
asserted at their exact boundaries, request cancellation reaching application services with no
transport type, and a drain bounded by the kernel's own work gate.

The public contracts are [`http-runtime.md`](../contracts/http-runtime.md),
[`http-routing.md`](../contracts/http-routing.md), and
[`http-security.md`](../contracts/http-security.md).

## What it did NOT do, stated first

| Not done | Why it matters |
|---|---|
| **The API instability window is NOT closed** | Contract C-S1 has two conditions. A real transport exercising the surface satisfies the **first**. The second needs an accepted record superseding ADR-0002; none exists |
| **Nothing published, tagged, released, or deployed** | 0 crates on crates.io, 0 tags, 0 releases. Verified against the registry API, not assumed |
| **No generated project depends on the framework** | Nothing is published, so a dependency would not resolve. The generator records the transport choice and documents the wiring instead |
| **`renvor routes` reaches no generated project** | Follows from the row above. It fails with `transport_not_wired`, exit 3, naming the reason |
| **No RFC 9457, no OpenAPI, no validation boundary** | Phase 005 scope, excluded deliberately |
| **ADR-0012 is `proposed`, not accepted** | No waiver authorises accepting a Phase 004 decision record |

## Requirements coverage

Every row names the code and the test. A row with no test is a gap, and is marked as one.

| FR | Implementation | Test |
|---|---|---|
| FR-001 crate separate, depends inward | `crates/renvor-http/Cargo.toml` | `xtask` step 7 CLAIM 3 + CONTROL 3 |
| FR-002 kernel resolves no HTTP dep | — (absence) | `xtask` step 7 CLAIM 1 + CONTROL 1, `--all-features` |
| FR-003 minimal builds carry no HTTP | `crates/renvor/Cargo.toml` feature | `xtask` step 7 CLAIM 2, 2b + CONTROL 2 |
| FR-004 no transport type in app interfaces | `route/mod.rs` | `tests/boundary.rs` ×3, incl. positive control. **Qualified 2026-08-22 (ledger L-12f)**: the scan reads a **four-file hand-list**, so the assertion covers those four files and not the crate. A newly added application-facing module is silently unscanned |
| FR-005 publish metadata | `crates/renvor-http/Cargo.toml` | `cargo package -p renvor-http --list`; `xtask` publishable-dependencies check |
| FR-006 single authoritative registry | `route/registry.rs` | `route::registry::tests::*` |
| FR-007 router and inspection share it | `route/build.rs`, `route/inspect.rs` | `a_route_added_to_the_registry_appears_without_a_second_manifest_being_touched` |
| FR-008 route groups | **partial** — `route/mod.rs::RouteGroup` supplies the **prefix** only | `a_group_prefixes_every_route_it_holds`, `a_group_prefix_reaches_the_real_router`. **Group-scoped middleware is NOT implemented**, and nested groups cannot compose — `RouteGroup` stores a name, a prefix, and routes, and nothing else. `contracts/http-routing.md` promises both |
| FR-009 duplicate is an error | `route/registry.rs::push` | `a_duplicate_is_refused_and_the_first_route_survives` |
| FR-010 route ceiling | `limits::MAX_ROUTES` | `the_route_ceiling_is_enforced_at_its_exact_boundary` |
| FR-011 404, and 405 with `Allow` | **partial** — `route/build.rs` for matched routes; the 404 and 405 responses are axum's own fallbacks | `an_undeclared_path_is_404`, `an_undeclared_method_on_a_declared_path_is_405_and_names_the_allowed_methods`. **Corrected 2026-08-22 (ledger L-07)**: this row read as fully implemented. The status codes are right, but they are produced **without** host validation, identity, request-ID generation, CORS, limits, timeout, or admission — verified: `GET /missing` with `Host: evil.example` → `404`, `x-request-id` **absent** |
| FR-012 state bridge | **NOT IMPLEMENTED** | — · **corrected 2026-08-22.** This row claimed the bridge existed "via `RequestContext`". It does not: neither `RouterConfig`, `RequestContext`, nor `Request` carries a `TypedStateMap` or exposes a typed lookup, so a handler cannot reach registered application state at all |
| FR-013 missing state is explicit | **partial** — `error.rs::StateUnavailable` exists and is used for absent connection info only | `a_request_without_connection_information_fails_closed`. **The state-lookup failure it was supposed to cover does not exist, because the lookup does not exist** |
| FR-014 request id nested under run id | **partial** — `context.rs`, `route/build.rs` | `every_response_carries_a_generated_request_identifier`. **Corrected 2026-08-22 (ledger L-07)**: the test's name overstates its reach — it drives **matched** routes only. A 404 or 405 carries **no** identifier, so "every request" is not yet true |
| FR-015 inbound id untrusted | `request_id.rs` | `a_caller_supplied_request_identifier_is_never_adopted` |
| FR-016 id is opaque | `context.rs::RequestId` | `a_request_id_is_a_pure_function_of_its_bytes`, `generation_does_not_consult_any_inbound_value` |
| FR-017 trusted set empty by default | `identity/trusted.rs` | `the_default_trusts_nobody` |
| FR-018 forwarding ignored unless trusted | `identity/mod.rs::resolve` | `under_the_default_configuration_hostile_headers_cannot_forge_identity`, `hostile_forwarding_headers_cannot_forge_client_identity_by_default` |
| FR-019 parsing fails closed | `identity/forwarded.rs` | 8 tests incl. `every_hostile_x_forwarded_for_fails_closed`, `a_quoted_value_containing_a_separator_is_not_split_inside_the_quotes` |
| FR-020 host validation fails closed | `host.rs` | 8 tests incl. `every_malformed_form_fails_closed` |
| FR-021 CORS deny-by-default | **partial** — `cors.rs` refuses disallowed origins | `the_default_policy_allows_no_origin`, `cors_denies_by_default`. **An ALLOWED origin receives no `Access-Control-Allow-Origin` header**, verified by execution: `status=200, acao=None`. `CorsLayer` is never applied, so a browser blocks every allowed response and preflights get ordinary routing |
| FR-022 wildcard+credentials refused at config time | `cors.rs::validate`, `route/build.rs::router` | `a_wildcard_with_credentials_is_refused_when_the_policy_is_built`, `a_router_cannot_be_built_for_an_unsafe_cors_configuration` |
| FR-023 body limit at exact boundary | `limits.rs`, `route/build.rs` | `a_body_at_the_limit_passes_and_one_byte_more_does_not` |
| FR-024 concurrency limit | `admission.rs` | `the_concurrency_ceiling_is_enforced_at_its_exact_boundary` |
| FR-025 timeout | **partial** — `route/build.rs` | `a_timed_out_request_is_408_and_cancels_its_scope`. **The timeout starts AFTER the body is read**, so a stalled body holds a concurrency slot and a work permit indefinitely. The declared order places timeout **outside** the body limit; the code does the reverse |
| FR-026 unexposed bounds NAMED | `limits.rs` module docs, `http-runtime.md` | prose; **no test, and none is possible** — see gaps below |
| FR-027 middleware order in a versioned contract | `contracts/http-security.md` | — |
| FR-028 order proven by behaviour | **partial** — `route/build.rs::dispatch` | 4 adjacent-pair tests in `tests/lifecycle.rs`. **The order applies only to MATCHED routes.** Axum's built-in 404 and 405 paths never call `dispatch`, so they bypass request-ID, host validation, identity, CORS, admission, and timeout — verified: `GET /missing` with a disallowed `Host` returns `404` with **no** `x-request-id`, instead of the host rejection |
| FR-029 admission through the work gate | `admission.rs` | `a_served_request_holds_a_permit_and_releases_it` |
| FR-030 drain refuses new requests | `admission.rs`, `route/build.rs` | `once_drain_begins_a_new_request_does_not_reach_a_handler` + control |
| FR-031 cancellation reaches services | **partial** — `context.rs`, `route/build.rs` | `application_shutdown_cancels_an_in_flight_request_without_any_transport_type` + control. **Client disconnect does not cancel the request scope** — only the timeout branch cancels, and dropping a `CancelScope` does not cancel it. A service holding a clone never observes a disconnect |
| FR-032 bounded drain, truthful outcome | **partial** — `server.rs` computes the outcome correctly | the drain tests exercise `drain_when_closed` **in isolation**. `Server::serve` uses `tokio::join!`, which waits for **both** the bounded drain **and** the unbounded shutdown future — so in exactly the over-budget case the bound exists for, `serve` still does not return. **No test drives `Server::serve` end to end**, which is why this passed review here and failed review there |
| FR-033 provider shutdown ordering | **NOT IMPLEMENTED** · **corrected 2026-08-22 (ledger L-03)** | This row read "inherited from C-L1/C-L3". Inheritance requires participation, and there is **no `impl renvor_core::Provider` anywhere in `renvor-http`** — the server binds and serves entirely outside `Boot`/`Ready`/`Drain`/`Stop`, contributes nothing to readiness, and a bind failure cannot roll back as a provider failure. C-10 opens with *"the server is a provider"*; it is not one |
| FR-034 route inspection, both forms | **partial** — `route/inspect.rs` renders both forms | 6 tests. **Qualified 2026-08-22 (ledger L-01)**: the rendering is complete and tested, but `renvor routes` has **no success path**, so no operator can obtain either form through the documented command |
| FR-035 structured output follows C-2 | **partial** — `route/inspect.rs` emits the envelope; `commands/routes.rs` emits only failure envelopes | `the_dump_is_a_single_parseable_json_document`. **Qualified 2026-08-22 (ledger L-01)**: the **success** envelope is asserted at the library, never through the command, because the command cannot succeed |
| FR-036 truthful about its source | `contracts/command-surface.md`, `commands/routes.rs` docs | `a_project_without_a_renvor_dependency_is_refused_by_name` |
| FR-037 `--transport` resolved and recorded | `config/flags.rs`, `config/model.rs`, `templates/renvor.toml.j2` | `every_governed_choice_of_principle_seven_is_classified` |
| FR-038 generated project still builds | `templates/Cargo.toml.j2` unchanged | measured: `cargo build` + `cargo run` in a fresh generated project |
| FR-039 evidence separation | this document, dependency inventory | prose; the distinction is stated wherever workspace evidence appears |
| FR-040 correct now-false statements | `tls.rs`, `dev.rs`, `README.md`, `renvor/src/lib.rs` | `consent_granted_still_modifies_nothing_because_the_operation_is_unavailable` now **forbids** the string "Phase 004" |
| FR-041 no disclosure in responses | `error.rs` | `a_public_message_cannot_carry_the_detail`, `an_unconfigured_host_is_refused_and_the_reason_is_not_disclosed`, `debug_never_prints_a_body` |
| FR-042 structured telemetry, redacted | `route/build.rs` `tracing::` calls | inherited redaction; **no new test** — see gaps |
| FR-043 no Phase 005 scope | — (absence) | grep over every `.rs` file the branch changes: **1** occurrence of `openapi`, in `commands/mod.rs`'s doc comment listing commands that are **not** implemented. **0** occurrences of RFC 9457, Problem Details, `application/problem`, or any validation boundary |
| FR-044 no stability claim, no release | `renvor/src/lib.rs`, `contracts/api-stability.md` untouched | `xtask` step 7 instability-wording check; 0 tags measured |
| FR-045 binary answers the dump | `route/inspect.rs::answer_dump_request` | `the_dump_request_is_answered_only_when_it_is_asked_for`, `the_dump_reports_the_same_routes_the_registry_holds` |
| FR-046 `routes` relays or fails by name | **partial** — the failure paths are implemented and tested | 5 tests incl. positive control. **The relay itself is not implemented**: a project that *does* declare the dependency still reaches an unconditional error, so the command has **no success path** |
| FR-047 code added to registry AND test | `exit.rs`, `contracts/json-output.md` | `the_registry_matches_the_published_contract_exactly` |
| FR-048 `--transport rest` accepted | `config/model.rs::Transport::parse` | `the_supported_transport_is_accepted_rather_than_reserved`, `an_unsupported_transport_is_an_unsupported_value_not_a_reservation` |
| FR-049 no wizard question, value recorded | `config/prompts.rs` unchanged, `renvor.toml.j2` | measured: generated `renvor.toml` carries `transport = "rest"` |

### Requirements with a gap, named rather than absorbed

| FR | Gap |
|---|---|
| **FR-026** | The **unexposed** header bounds are documented but cannot be tested — a bound Renvor does not set has no boundary Renvor can assert. The claim is that Renvor makes **no promise**, and the evidence for a non-promise is the documentation itself |
| **FR-033** | Provider shutdown ordering is **inherited** from contract C-L1/C-L3 and is tested in the kernel, not re-tested through the transport. The transport introduces no phase and reorders none, so there is nothing new to order — but this phase adds no test that observes the two together |
| **FR-042** | Redaction of the adapter's own span fields relies on the kernel's existing mechanism. This phase adds **no new test** that a secret placed in a request cannot reach a span |

## Success criteria

| SC | Result |
|---|---|
| SC-001 real router, 0 imitations | **met** — every routing test drives `axum::Router` via `ServiceExt::oneshot`; 0 mock services exist |
| SC-002 404/405, `Allow` in 100% | **partially met** · **corrected 2026-08-22 (ledger L-07)** — the **status codes and `Allow` header** hold in 100% of asserted cases. The responses carrying them bypass every request control, so the criterion is met on its literal wording and not on what it exists to establish |
| SC-003 groups, state, middleware by behaviour | **NOT MET** — group *prefixes* and middleware *ordering on matched routes* are asserted; **group middleware and state access do not exist** |
| SC-004 0 forged identities by default | **met** — asserted at the resolver and through the real router |
| SC-005 0 adopted caller identifiers | **met** |
| SC-006 fail-closed in 100% of malformed cases | **met** — 8 host cases, 11 forwarding cases |
| SC-007 CORS denies in 100%; wildcard+credentials refused in 100% | **partially met** — denial and the wildcard+credentials refusal hold. **Allowing does not**: no `Access-Control-Allow-Origin` is emitted, so an allowed origin is blocked by the browser |
| SC-008 every limit at boundary and boundary+1 | **met** for body, concurrency, route ceiling. Timeout is asserted at the threshold only — a timeout has no "one unit under" that is meaningfully different |
| SC-009 cancellation 100% / 0% | **partially met** — application shutdown and timeout cancel; **client disconnect does not** |
| SC-010 0 requests reach a handler after drain | **met** — the handler records whether it ran |
| SC-011 over-budget reports outstanding 100%, clean 0% | **met for the drain computation, NOT met end to end** — `Server::serve` does not return in the over-budget case (`tokio::join!` waits on the unbounded future) |
| SC-012 0 transport types in app interfaces | **met**, with a positive control |
| SC-013 kernel resolves 0 HTTP crates, both directions | **met**, control exercised by hand (see dependency inventory) |
| SC-014 one registry, by construction | **met** — `inspect` takes `&RouteRegistry` and has no other source |
| SC-015 full verification on both toolchains | **met** — 11/11 on 1.94.0 and on 1.97.1 |
| SC-016 0 tags, releases, publications, deployments | **met**, measured |
| SC-017 every negative check has a positive control | **met** |
| SC-018 generated project builds | **met** — `cargo build` and `cargo run` both succeed |
| SC-019 0 empty-table-and-exit-0 | **met, vacuously** · **qualified 2026-08-22 (ledger L-01)** — 0 such cases exist because the command has **no exit-0 path at all**. A criterion that a missing feature satisfies is recorded as satisfied-by-absence rather than as passed |
| SC-020 workspace evidence labelled as such | **met** — stated here and in the dependency inventory |

## Verification

`cargo xtask verify` — **11 of 11 steps ran and passed**, on **both**:

| Toolchain | Result |
|---|---|
| 1.94.0 (pinned MSRV) | all 11 steps passed |
| 1.97.1 (current stable) | all 11 steps passed |

`cargo deny check` reported `advisories ok, bans ok, licenses ok, sources ok`.
Working tree clean and HEAD unchanged after each run.

### One CI check failed on the first push, and this is what it was

**`package and verify without publishing` failed** on head `cdd9d01`. It is recorded here rather
than quietly fixed, because a phase evidence pack that lists only the runs that passed is not
evidence.

```text
publishable set changed — update CRATES or the manifest that changed
expected: renvor renvor-config renvor-core renvor-testkit
actual:   renvor renvor-config renvor-core renvor-http renvor-testkit
```

**The guard was working.** `.github/workflows/release-dry-run.yml` pins the expected publishable set
in a `CRATES` variable **precisely so that growing it is a decision somebody records**, rather than
a silent consequence of adding a crate. Phase 004 added a fifth, and the assertion said so.

The fix is the one the guard asks for — the pinned list now names `renvor-http` — and it is **not** a
weakening: the assertion still compares an exact set and still fails if `xtask` or `renvor-cli` ever
becomes publishable.

**Why the local run did not catch it.** That assertion is a **step in the release-dry-run workflow**,
not a step in `cargo xtask verify`, so a green local verification could never have covered it. That
gap is real and is recorded: the verification contract's fail-closed rule governs the eleven steps
`xtask` runs, and this check is outside them.

The remaining 14 checks passed on that head, including all six platform/toolchain contexts.

## Findings against the implementation

An **independent-model review** (OpenAI Codex, run against `main` on head `49859f7`) produced **19
findings, 12 of them P1**. It is **advisory and NOT independent in the governance sense** — the
`GOVERNANCE.md` definition requires a **person** — but it is a genuinely different model reading the
same code, and it found real defects that this repository's own tests did not.

**Four findings were verified by execution** before being accepted, because an agent's claim is not
evidence:

| Verified claim | Observed |
|---|---|
| 404 bypasses the middleware order | `GET /missing` + `Host: evil.example` → **404**, `x-request-id` **absent** |
| CORS emits no allow-origin header | allowed origin → **200**, `access-control-allow-origin` **absent** |
| Malformed `Forwarded` falls through to `X-Forwarded-For` | trusted peer → `ViaTrustedProxy{198.51.100.7}` instead of `DirectPeer` |
| Unbalanced quote accepted | `for="198.51.100.7` → `Some(198.51.100.7)` instead of `None` |

### Corroboration across reviewers

The Codex review and the agent security review were run **independently and against the same head**,
and they converged on the same top four: fallback routing bypassing every control, the body read
sitting outside the timeout, `tokio::join!` not bounding the drain, and CORS never emitting a header.
Two reviewers reaching the same four findings by different routes is stronger evidence than either
alone, and it is why those four are treated as settled rather than as claims.

The agent reviews additionally found **five defects Codex did not**, four of which were verified by
execution and fixed. That is the argument for running more than one reviewer, recorded here because
the opposite conclusion — "the second review found nothing new, so skip it next time" — is the one a
reader would otherwise reach.

### Fixed on this branch

| # | Finding | Fix |
|---|---|---|
| 1 | Malformed `Forwarded` fell through to `X-Forwarded-For` — **security** | Presence now selects the header; parsing decides the answer. A present-but-unparseable standard header fails closed instead of deferring |
| 2 | An unbalanced quote in `Forwarded` was accepted — **security** | Quotes must balance before anything is split |
| 3 | An unreadable (non-UTF-8) header value was silently dropped, collapsing a repeated header past the repeated-header refusal — **security** | An unreadable value is preserved as a placeholder no parser accepts, so the count stays truthful |
| 4 | **A URL pasted into the host allow-list configured the host `https`** — verified: `allow("https://example.com")` allowed `Host: https` and **refused `example.com`** — **security** | `allow` refuses any value containing `://`, `@`, or `/`. The likeliest operator mistake now errors instead of admitting an arbitrary host |
| 5 | **Trusted proxies were silently inoperative on a dual-stack listener** — verified: `trust(10.0.0.1)` did not match a peer arriving as `::ffff:10.0.0.1` | Both sides canonicalise. Failed closed, but the natural operator response to "my proxy is not trusted" is to widen the configuration, which is how a silent misconfiguration becomes a deliberate one |
| 6 | **A default-configured application answered 400 to its own frontend's writes** — verified: browsers send `Origin` on same-origin `POST`, and deny-by-default refused every one | A same-origin request is not a cross-origin request. Compared against the **validated** host, so it cannot be satisfied without also satisfying host validation |
| 7 | A handler could emit a **second** `x-request-id`, its own first — verified: 2 values returned | `insert` rather than append, so "the generated identifier is what appears" is a property rather than a convention |
| 8 | **`renvor check` rejected every Phase 003 project** — verified: `missing field \`transport\``, no migration path. A framework that invalidates the projects it generated one phase earlier has broken its own output | `transport` is optional and validated only when present. Absent means "written before the transport was recorded" |

### Open, and blocking

**Twelve findings remain unfixed.** They are listed rather than deferred quietly, because the
requirement rows above depend on them.

**They are frozen as a numbered ledger** in
[`phase-004-finding-ledger.md`](phase-004-finding-ledger.md), recorded at head `dad8333` **before
any remediation edit was made**, with a requirement, affected files, severity, reproduction,
intended fix, and required regression test per row. The maintainer instruction of 2026-08-22
absorbs all twelve into Phase 004: **none is transferred to a later phase and none is downgraded.**
The table below is the summary; the ledger is the record.

| Severity | Finding |
|---|---|
| P1 | `renvor routes` has **no success path** — the relay to the project binary is not implemented |
| P1 | `Server::serve` uses `tokio::join!`, so the bounded drain does not actually bound shutdown |
| P1 | No `renvor_core::Provider` implementation — the server is not in the kernel lifecycle at all, so `Boot`/`Ready`/`Drain`/`Stop` ordering and readiness contribution are unwired |
| P1 | `TypedStateMap` is not bridged into requests — **FR-012 and FR-013 are unimplemented** |
| P1 | Group-scoped middleware does not exist |
| P1 | CORS emits no headers for allowed origins; preflights are routed normally |
| P1 | Router fallbacks (404/405) bypass every request control |
| P1 | The request timeout starts **after** the body is read, so a stalled body holds admission |
| P1 | Client disconnect does not cancel the request scope |
| P2 | Handler panics are not contained — no `catch_unwind` boundary |
| P2 | Path parameters are never populated; `path_param` always returns `None` |
| P2 | Route-pattern validation is too weak — a malformed pattern registers and then panics at router construction |
| P2 | Nested groups, the handler trace span, run IDs on refusal telemetry, and validation of response status/header metadata |
| P2 | The boundary scan in `tests/boundary.rs` is a four-file hand-list, so a new application-facing module is silently unscanned |
| P2 | `registry::methods_for` is documented as producing the `Allow` header and **has no caller** — the router produces it |
| LOW | Every body error reports `413`, including IO errors and client disconnects |

**What this means for the phase.** The contracts published in this branch describe behaviour the
implementation does not yet fully have. That is the defect class this project treats most seriously,
and it is recorded here rather than left for a reader to discover. **Phase 004 is not ready to
merge on requirements grounds, independently of the governance gate below.**

## Review status — stated plainly

**No independent human review of Phase 004 has occurred.**

Automated and agent reviews are **advisory and explicitly NON-INDEPENDENT**, and must never be
described otherwise — here, in the pull request, in `GOVERNANCE.md`, or in any public document.
**Five advisory agent runs were commissioned. Three returned nothing; two delivered late.**

| Run | Outcome |
|---|---|
| Package research | **NOT PERFORMED** — hit an account usage limit and reported nothing. The research was carried out directly against vendored crate source instead; that evidence is in the plan and in ADR-0012 |
| Plan requirements review | **NOT PERFORMED** — same limit |
| Plan security review | **NOT PERFORMED** — same limit |
| Implementation security review | **DELIVERED**, late — 13 findings, S1–S9 complete, with an explicit "checked and holding" section naming what it verified and found sound |
| Implementation requirements review | **DELIVERED**, late — 16 findings, R1–R10 complete, including a full FR-001…FR-049 mapping |

Under this ledger's rule, *"a review that returns nothing is recorded as NOT PERFORMED, never as
passed"* — the first three are recorded that way. **An earlier revision of this document said all
five had returned nothing. Two had simply not yet been given time to reply, and that statement was
corrected as soon as they did.**

**The mechanism of the three failures is worth recording.** A subagent's final text does not reach
the caller unless the subagent explicitly sends it. Three runs ended with their findings sitting in
their own transcripts and nothing delivered — which is indistinguishable, from the caller's side,
from having found nothing. That is the exact failure mode this ledger's "empty result" rule exists
to catch, arriving by a route nobody had anticipated.
vendored crate source, and that evidence is in the plan and in ADR-0012.

### The open governance gate

**ADR-0012 is `proposed` and cannot be accepted on current authority.** Constitution §Development
and Phase Workflow #4 requires a recorded **independent** review before acceptance, and no existing
waiver reaches a Phase 004 decision record:

- **W-002** covers Phase 001 decision records;
- **W-004** names **ADR-0007 alone**;
- **W-006** names **ADR-0009 alone**;
- **W-005** and **W-008** are *phase-level*, and their own scope sections state they do **not**
  authorise accepting any decision record.

None is extended by reinterpretation. **No new waiver has been created**, and creating one is
Ahmed's decision rather than this phase's.

**A fourth consecutive single-maintainer review waiver would trip the ledger's trend guard again**,
which already fired at W-008 and carries obligation RO-001. That consequence belongs in the decision
and is stated here so it is not discovered afterwards.

Until an independent review or an explicit authorisation exists:

- Phase 004 is **not** complete;
- ADR-0012 is **not** accepted;
- the Phase 004 pull request does **not** merge;
- Phase 005 does **not** start.
