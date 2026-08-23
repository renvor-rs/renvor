# Phase 004 evidence — REST routing and HTTP runtime

**Base**: `10da854736598d99218d1627c3ad79866a2f7f89` · **Branch**: `feat/phase-004-rest-http-runtime`
**Date**: 2026-08-22

> **Phase 004 is NOT closed.** This is the evidence pack, not a completion record. The blocker is
> now **governance**, not implementation: ADR-0012 is `proposed` and no authority exists to accept
> it. See [§The open governance gate](#the-open-governance-gate).
>
> **Three advisory reviews found defects against the implementation** — an independent-model review
> (Codex) and two agent reviews that eventually delivered. Between them: **19 + 13 + 16 findings**,
> heavily overlapping. **Eight were verified by executing them**, **eight were fixed immediately**,
> and **twelve substantive findings were absorbed into this phase and have since been fixed.**
>
> Requirement rows below were **corrected downward** when the reviews disproved them, and have now
> been **restated from executable results**. Every row carrying a `Reconciled 2026-08-22` marker was
> re-measured rather than re-asserted. The dispositions are in
> [§The twelve](#the-twelve--every-one-closed-on-this-branch).
>
> **All twelve are `validated`, and none is `complete`.** Three independent validation rounds moved
> them there; the third closed L-11. `complete` is a human decision that has not been taken.
> *(Corrected 2026-08-23 — this line previously read "every one of the twelve is `coding_done`",
> which was true when written and stale by three rounds.)*

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
| **`renvor routes` reaches no generated project** | Follows from the row above. The relay **is** implemented and is asserted end to end against a real binary; what is zero is its reach across *generated* projects, because nothing is published for them to depend on. Against such a project it fails with `transport_not_wired`, exit 3, naming the reason |
| **No RFC 9457, no OpenAPI, no validation boundary** | Phase 005 scope, excluded deliberately |
| **ADR-0012 is `proposed`, not accepted** | No waiver authorises accepting a Phase 004 decision record |

## Requirements coverage

Every row names the code and the test. A row with no test is a gap, and is marked as one.

| FR | Implementation | Test |
|---|---|---|
| FR-001 crate separate, depends inward | `crates/renvor-http/Cargo.toml` | `xtask` step 7 CLAIM 3 + CONTROL 3 |
| FR-002 kernel resolves no HTTP dep | — (absence) | `xtask` step 7 CLAIM 1 + CONTROL 1, `--all-features` |
| FR-003 minimal builds carry no HTTP | `crates/renvor/Cargo.toml` feature | `xtask` step 7 CLAIM 2, 2b + CONTROL 2 |
| FR-004 no transport type in app interfaces | `route/mod.rs` | `tests/boundary.rs` ×3, incl. positive control. **Re-qualified 2026-08-23**: the four-file hand-list is gone — `tests/boundary.rs:44` now walks `src/` recursively with a tempfile control proving it discovers a newly created nested file. The real limit is narrower and sharper: the scan exempts `server.rs`, and `Server::serve` takes `axum::Router` in a **public** signature re-exported at the facade root. See finding R-6 |
| FR-005 publish metadata | `crates/renvor-http/Cargo.toml` | `cargo package -p renvor-http --list`; `xtask` publishable-dependencies check |
| FR-006 single authoritative registry | `route/registry.rs` | `route::registry::tests::*` |
| FR-007 router and inspection share it | `route/build.rs`, `route/inspect.rs` | `a_route_added_to_the_registry_appears_without_a_second_manifest_being_touched` |
| FR-008 route groups | `route/mod.rs::RouteGroup` — prefix **and** `layer()`; `group()` composes nested groups | `a_group_prefixes_every_route_it_holds`, `a_group_prefix_reaches_the_real_router`, plus 5 in `tests/groups.rs` incl. `nested_groups_apply_both_layers_with_the_outer_one_outermost` and `a_route_with_no_group_middleware_reaches_its_handler_unchanged` as the control. **Reconciled 2026-08-22 (ledger L-05)** |
| FR-009 duplicate is an error | `route/registry.rs::push` | `a_duplicate_is_refused_and_the_first_route_survives` |
| FR-010 route ceiling | `limits::MAX_ROUTES` | `the_route_ceiling_is_enforced_at_its_exact_boundary` |
| FR-011 404, and 405 with `Allow` | `route/build.rs` — the router is the layer stack's `fallback_service`, so 404 and 405 are produced **inside** every control | `an_undeclared_path_is_404`, `an_undeclared_method_on_a_declared_path_is_405_and_names_the_allowed_methods`, `an_unknown_path_with_a_disallowed_host_returns_the_host_rejection_not_404`, `an_unknown_path_with_an_allowed_host_is_404_and_carries_the_request_identifier`, `a_405_carries_both_allow_and_the_request_identifier`. **Reconciled 2026-08-22 (ledger L-07)** |
| FR-012 state bridge | `route/mod.rs::Request::state::<T>()` over a shared `Arc<TypedStateMap>`; wired in `route/build.rs` and `provider.rs` | `registered_state_reaches_a_handler_through_a_real_router`. **Reconciled 2026-08-22 (ledger L-04)** |
| FR-013 missing state is explicit | `error.rs::StateUnavailable`, returned by the typed lookup and by absent connection info | `a_missing_state_entry_is_an_explicit_reported_failure`, `a_handler_reading_state_from_an_empty_map_reports_rather_than_panics`, `a_request_without_connection_information_fails_closed`. **Reconciled 2026-08-22 (ledger L-04)** |
| FR-014 request id nested under run id | `context.rs`, `route/build.rs` — generated outermost, attached at **one** exit site | `every_response_carries_a_generated_request_identifier`, `every_refusal_carries_a_request_identifier_because_correlation_is_outermost`, and the 404/405 cases above. **Reconciled 2026-08-22 (ledger L-07)** |
| FR-015 inbound id untrusted | `request_id.rs` | `a_caller_supplied_request_identifier_is_never_adopted` |
| FR-016 id is opaque | `context.rs::RequestId` | `a_request_id_is_a_pure_function_of_its_bytes`, `generation_does_not_consult_any_inbound_value` |
| FR-017 trusted set empty by default | `identity/trusted.rs` | `the_default_trusts_nobody` |
| FR-018 forwarding ignored unless trusted | `identity/mod.rs::resolve` | `under_the_default_configuration_hostile_headers_cannot_forge_identity`, `hostile_forwarding_headers_cannot_forge_client_identity_by_default` |
| FR-019 parsing fails closed | `identity/forwarded.rs` | 8 tests incl. `every_hostile_x_forwarded_for_fails_closed`, `a_quoted_value_containing_a_separator_is_not_split_inside_the_quotes` |
| FR-020 host validation fails closed | `host.rs` | 8 tests incl. `every_malformed_form_fails_closed` |
| FR-021 CORS deny-by-default | `cors.rs` validates; `route/build.rs::cors_layer` applies the selected library's `CorsLayer` | `the_default_policy_allows_no_origin`, `cors_denies_by_default`, `an_allowed_origin_receives_the_allow_origin_header`, `a_disallowed_origin_is_refused_and_carries_no_allow_origin_header`, `a_preflight_is_answered_rather_than_routed`, `a_same_origin_write_succeeds_under_the_default_deny_all_policy`. **Reconciled 2026-08-22 (ledger L-06)** |
| FR-022 wildcard+credentials refused at config time | `cors.rs::validate`, `route/build.rs::router` | `a_wildcard_with_credentials_is_refused_when_the_policy_is_built`, `a_router_cannot_be_built_for_an_unsafe_cors_configuration` |
| FR-023 body limit at exact boundary | `limits.rs`, `route/build.rs` | `a_body_at_the_limit_passes_and_one_byte_more_does_not` |
| FR-024 concurrency limit | `admission.rs` | `the_concurrency_ceiling_is_enforced_at_its_exact_boundary` |
| FR-025 timeout | `route/build.rs::admit_and_bound` — the timeout wraps routing, body read **and** handler, inside admission | `a_timed_out_request_is_408_and_cancels_its_scope`, `a_body_that_never_arrives_times_out_rather_than_holding_admission_for_ever`, `a_body_that_arrives_inside_the_bound_still_succeeds` as the control. **Reconciled 2026-08-22 (ledger L-08)** |
| FR-026 unexposed bounds NAMED | `limits.rs` module docs, `http-runtime.md` | prose; **no test, and none is possible** — see gaps below |
| FR-027 middleware order in a versioned contract | `contracts/http-security.md` | — |
| FR-028 order proven by behaviour | `route/build.rs` layer stack | 4 adjacent-pair tests in `tests/lifecycle.rs` plus `an_unknown_path_with_a_disallowed_host_returns_the_host_rejection_not_404` and `an_unmatched_path_is_refused_during_drain_like_any_other_request`, which extend the proof to **unmatched** paths. **Reconciled 2026-08-22 (ledger L-07)** |
| FR-029 admission through the work gate | `admission.rs` | `a_served_request_holds_a_permit_and_releases_it` |
| FR-030 drain refuses new requests | `admission.rs`, `route/build.rs` | `once_drain_begins_a_new_request_does_not_reach_a_handler` + control |
| FR-031 cancellation reaches services | `context.rs`, `route/build.rs::CancelOnDrop` | `application_shutdown_cancels_an_in_flight_request_without_any_transport_type`, `a_timed_out_request_is_408_and_cancels_its_scope`, `dropping_a_request_in_flight_cancels_its_scope`, `a_request_that_completes_normally_is_not_cancelled` as the control. **Reconciled 2026-08-22 (ledger L-09)** |
| FR-032 bounded drain, truthful outcome | `server.rs::Server::serve` — gate close, stop accepting, bounded drain, cancel, then the **remainder** of the budget for connection tasks | 4 tests in `tests/shutdown.rs` driving `Server::serve` over a real socket, incl. `a_handler_that_never_finishes_does_not_extend_shutdown_beyond_the_budget`. **Reconciled 2026-08-22 (ledger L-02)** |
| FR-033 provider shutdown ordering | `provider.rs::HttpServerProvider` implements `renvor_core::Provider` and contributes `ServerReadiness` | 5 tests in `tests/provider.rs`, incl. `the_server_drains_before_the_provider_it_depends_on_stops` and `a_bind_failure_aborts_boot_and_rolls_the_other_providers_back`, with `boot_succeeds_on_a_free_port_which_is_why_the_failure_above_is_about_the_port` as the control. **Reconciled 2026-08-22 (ledger L-03)** |
| FR-034 route inspection, both forms | `route/inspect.rs` renders both forms; `commands/routes.rs` relays them through the project binary | 6 rendering tests plus 14 relay tests, incl. the end-to-end `the_relay_reads_what_the_real_library_actually_prints`. **Reconciled 2026-08-22 (ledger L-01)** |
| FR-035 structured output follows C-2 | `route/inspect.rs` emits the envelope; `commands/routes.rs` relays it and emits failure envelopes | `the_dump_is_a_single_parseable_json_document`, plus the success envelope asserted **through the command**. **Reconciled 2026-08-22 (ledger L-01)** |
| FR-036 truthful about its source | `contracts/command-surface.md`, `commands/routes.rs` docs | `a_project_without_a_renvor_dependency_is_refused_before_anything_is_run` |
| FR-037 `--transport` resolved and recorded | `config/flags.rs`, `config/model.rs`, `templates/renvor.toml.j2` | `every_governed_choice_of_principle_seven_is_classified` |
| FR-038 generated project still builds | `templates/Cargo.toml.j2` unchanged | measured: `cargo build` + `cargo run` in a fresh generated project |
| FR-039 evidence separation | this document, dependency inventory | prose; the distinction is stated wherever workspace evidence appears |
| FR-040 correct now-false statements | `tls.rs`, `dev.rs`, `README.md`, `renvor/src/lib.rs` | `consent_granted_still_modifies_nothing_because_the_operation_is_unavailable` now **forbids** the string "Phase 004" |
| FR-041 no disclosure in responses | `error.rs` | `a_public_message_cannot_carry_the_detail`, `an_unconfigured_host_is_refused_and_the_reason_is_not_disclosed`, `debug_never_prints_a_body` |
| FR-042 structured telemetry, redacted | `route/build.rs` `tracing::` calls | inherited redaction; **no new test** — see gaps |
| FR-043 no Phase 005 scope | — (absence) | grep over every `.rs` file the branch changes: **1** occurrence of `openapi`, in `commands/mod.rs`'s doc comment listing commands that are **not** implemented. **0** occurrences of RFC 9457, Problem Details, `application/problem`, or any validation boundary |
| FR-044 no stability claim, no release | `renvor/src/lib.rs`, `contracts/api-stability.md` untouched | `xtask` step 7 instability-wording check; 0 tags measured |
| FR-045 binary answers the dump | `route/inspect.rs::answer_dump_request` | `the_dump_request_is_answered_only_when_it_is_asked_for`, `the_dump_reports_the_same_routes_the_registry_holds` |
| FR-046 `routes` relays or fails by name | `commands/routes.rs` — the relay plus six named failure reasons | 14 tests: one per reason (`no_renvor_dependency`, `invocation_failed`, `dump_failed`, `dump_unreadable`, `protocol_unstated`, `protocol_unsupported`), the success path, and the end-to-end proof. **Reconciled 2026-08-22 (ledger L-01)** |
| FR-047 code added to registry AND test | `exit.rs`, `contracts/json-output.md` | `the_registry_matches_the_published_contract_exactly` |
| FR-048 `--transport rest` accepted | `config/model.rs::Transport::parse` | `the_supported_transport_is_accepted_rather_than_reserved`, `an_unsupported_transport_is_an_unsupported_value_not_a_reservation` |
| FR-049 no wizard question, value recorded | `config/prompts.rs` unchanged, `renvor.toml.j2` | measured: generated `renvor.toml` carries `transport = "rest"` |

### Requirements with a gap, named rather than absorbed

| FR | Gap |
|---|---|
| **FR-026** | The **unexposed** header bounds are documented but cannot be tested — a bound Renvor does not set has no boundary Renvor can assert. The claim is that Renvor makes **no promise**, and the evidence for a non-promise is the documentation itself |
| **FR-042** | The adapter now opens its own handler span (`renvor.http.handler`, `route/build.rs`), and redaction of its fields relies on the kernel's existing mechanism. This phase adds **no new test** that a secret placed in a request cannot reach that span. The gap is narrower than it was and is **not** closed |

## Success criteria

| SC | Result |
|---|---|
| SC-001 real router, 0 imitations | **met** — every routing test drives `axum::Router` via `ServiceExt::oneshot`; 0 mock services exist |
| SC-002 404/405, `Allow` in 100% | **met** — status codes and `Allow` hold, and the responses carrying them now pass through every control. **Reconciled 2026-08-22 (ledger L-07)** |
| SC-003 groups, state, middleware by behaviour | **met** — group middleware, nested composition, and typed state access are each asserted on a real `Router`. **Reconciled 2026-08-22 (ledger L-04, L-05)** |
| SC-004 0 forged identities by default | **met** — asserted at the resolver and through the real router |
| SC-005 0 adopted caller identifiers | **met** |
| SC-006 fail-closed in 100% of malformed cases | **met** — 8 host cases, 11 forwarding cases |
| SC-007 CORS denies in 100%; wildcard+credentials refused in 100% | **met** — denial, allowance, preflight, credentials, wildcard, and the same-origin carve-out are all asserted. **Reconciled 2026-08-22 (ledger L-06)** |
| SC-008 every limit at boundary and boundary+1 | **met** for body, concurrency, route ceiling. Timeout is asserted at the threshold only — a timeout has no "one unit under" that is meaningfully different |
| SC-009 cancellation 100% / 0% | **met** — application shutdown, timeout, **and** client disconnect all cancel; normal completion does not. **Reconciled 2026-08-22 (ledger L-09)** |
| SC-010 0 requests reach a handler after drain | **met** — the handler records whether it ran |
| SC-011 over-budget reports outstanding 100%, clean 0% | **met end to end** — `tests/shutdown.rs` drives `Server::serve` over a real socket in both the clean and the over-budget case. **Reconciled 2026-08-22 (ledger L-02)** |
| SC-012 0 transport types in app interfaces | **met**, with a positive control |
| SC-013 kernel resolves 0 HTTP crates, both directions | **met**, and the control is **automated**, not manual: `xtask/src/main.rs:783` runs CONTROL 2 inside verify step 7 on every run. Independently re-measured by the requirements review: kernel 0, lean facade 0, default facade 0, `transport-rest` 6. *(Corrected 2026-08-23 — this row said "control exercised by hand", understating it.)* |
| SC-014 one registry, by construction | **met** — `inspect` takes `&RouteRegistry` and has no other source |
| SC-015 full verification on both toolchains | **met** — 11/11 on 1.94.0 and on 1.97.1 |
| SC-016 0 tags, releases, publications, deployments | **met**, measured |
| SC-017 every negative check has a positive control | **met** |
| SC-018 generated project builds | **met** — `cargo build` and `cargo run` both succeed |
| SC-019 0 empty-table-and-exit-0 | **met** — an exit-0 path exists and is asserted, and every failure path exits `3` naming a reason. No longer satisfied by absence. **Reconciled 2026-08-22 (ledger L-01)** |
| SC-020 workspace evidence labelled as such | **met** — stated here and in the dependency inventory |

## Verification

`cargo xtask verify` — **11 of 11 steps ran and passed**, on **both**:

| Toolchain | Result | Tests |
|---|---|---|
| 1.94.0 (pinned MSRV) | all 11 steps passed | **837 passed, 0 failed, 1 ignored, 54 suites** |
| 1.97.1 (current stable) | all 11 steps passed | **837 passed, 0 failed, 1 ignored, 54 suites** |

`cargo deny check` reported `advisories ok, bans ok, licenses ok, sources ok`.
Working tree clean and HEAD unchanged after each run.

### Gates run explicitly, outside the eleven

The verification sequence does not cover these, so they are run and recorded separately rather than
assumed. **The single ignored test is the end-to-end route relay**, and it is run — not skipped.

| Gate | Result |
|---|---|
| End-to-end route relay (`--ignored`) | **passed** — 1 passed, 0 failed |
| Workspace tests **serial** (`--test-threads=1`, the CI platform job) | **passed** — 837 passed, 0 failed, identical to the parallel run, so no test depends on ordering |
| Facade with `--no-default-features --all-targets` | **passed** |
| Workspace default features, all targets | **passed** |
| Workspace all features, all targets | **passed** |
| `cargo metadata --locked` | **passed** |
| Publishable-set assertion | **passed** — exactly `renvor renvor-config renvor-core renvor-http renvor-testkit` |
| `cargo package --workspace` | **passed** |
| `cargo publish --dry-run --workspace` | **passed** — 5 uploads aborted by the dry run, 0 errors, **nothing published** |

### The package rehearsal failed first, and the reason is worth recording

`cargo package --workspace` failed with `no method named run_id found for &mut InitContext<'_>` —
a method that demonstrably **exists** in the source, in the generated tarball, and in the extracted
registry copy. It was reproducible across three runs.

**The cause was the shared `target/` directory, not the code.** Package verification compiles into
the workspace's own `target/debug/deps`, which had accumulated **77** `librenvor_core-*.rmeta`
files over several days and 4.6 GB. Cargo handed the packaged `renvor-http` an rmeta dated *before*
`InitContext::run_id` was added, rather than the one it had just compiled. `cargo clean` followed by
the same command passed.

Two wrong explanations were tested and rejected before this one: a stale extraction under
`~/.cargo/registry/src` (cleared — still failed) and a stale checksum pin (the pinned checksum
matched the freshly built tarball exactly).

**This is recorded rather than quietly cleaned** because it is a live hazard for anyone running the
release rehearsal locally: the crate version is permanently `0.0.0`, so nothing about a stale
artifact ever looks stale. CI runs on a fresh runner and cannot hit it, which is precisely why it
would otherwise go unnoticed until someone trusted a local failure — or, worse, a local pass.

### CI caught a defect that no local run could

**Both Windows platform jobs failed** on head `07804ff` — `platform (windows-latest, 1.94.0)` and
`platform (windows-latest, stable)` — with 6 of the 14 route-relay tests reporting `os error 3`,
"the system cannot find the path specified".

**The cause was mine.** The test helpers that stand in for a project binary hard-coded `/bin/sh`,
which Windows has no equivalent for. Every local run in this session is macOS, so no amount of local
verification could have found it; the Windows job is the only thing that could, and did.

The fix (`56cb86c`) splits the helpers per platform and routes the payload through a **file** rather
than interpolating it into a shell command. That also removes a latent quoting bug: these tests
exist to feed the relay malformed input, and a payload containing a quote would have broken the
command it was embedded in rather than testing what it claimed to test. Windows CI passes on the
fixed head.

> This is recorded rather than quietly fixed because it is the second time in this session that a
> gate found something local verification structurally could not. The first was the formatting gate;
> this one is stronger evidence, because it shows a *platform* the developer does not run is doing
> real work rather than duplicating the Linux job.

### One test is sensitive to the developer's machine, and that is worth naming

`no_command_in_this_phase_modifies_the_trust_store` (`crates/renvor-cli/tests/tls_consent.rs`)
failed once on Rust 1.94.0 at head `07804ff`, and passed on stable at the same head, in the serial
run, in the 1.94.0 re-run, and in three earlier full runs of identical code.

**It was not the code.** The test snapshots the **real system trust store** around every command.
On macOS that set includes the developer's live `~/Library/Keychains/login.keychain-db`, which the
operating system rewrites whenever any process performs a keychain operation — including TLS
certificate validation by unrelated tools. The failure reported an **unchanged file size** with a
changed content hash, from a `docker down --dry-run` that has no trust-store code path at all,
while this session was concurrently running `gh` over TLS.

On Linux the same function snapshots `/etc/ssl/certs/ca-certificates.crt` and two directories —
static files nothing rewrites mid-run. **That asymmetry is why CI cannot see this and a macOS
developer can.**

The test is **not weakened, skipped, or marked flaky.** It measures a globally mutable system
resource, so it cannot distinguish "renvor modified it" from "anything else on this machine did".
Recorded so that a future macOS failure is diagnosed rather than either dismissed or believed.

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

### The twelve — every one closed on this branch

**All twelve were absorbed into Phase 004** under the maintainer instruction of 2026-08-22: none was
transferred to a later phase, none was downgraded, and no Phase 004a was created. They were frozen
as a numbered ledger in [`phase-004-finding-ledger.md`](phase-004-finding-ledger.md) at head
`dad8333` **before any remediation edit was made**, with a requirement, affected files, severity,
reproduction, intended fix, and required regression test per row.

**Red was verified before green.** With the remediation stashed, the new tests were run against the
pre-fix code: **12 failed and 1 hung** — the hang being L-08's unbounded body read, which is the
defect expressed as a symptom rather than an assertion. Four tests passed on both sides; those are
positive controls and are supposed to.

| # | Finding | Disposition | Evidence |
|---|---|---|---|
| L-01 | `renvor routes` has no success path | **fixed** | 14 tests in `commands/routes.rs`, incl. the end-to-end `the_relay_reads_what_the_real_library_actually_prints` against a real binary |
| L-02 | `Server::serve` does not bound shutdown | **fixed** | 4 tests in `tests/shutdown.rs` over a real socket |
| L-03 | the server is not a kernel provider | **fixed** | 5 tests in `tests/provider.rs`, ordering observed through recorded events |
| L-04 | `TypedStateMap` is never bridged | **fixed** | 3 tests in `tests/state.rs` |
| L-05 | group-scoped middleware does not exist | **fixed** | 5 tests in `tests/groups.rs`; nesting proven as an onion |
| L-06 | the CORS protocol is not implemented | **fixed** | 7 CORS tests in `tests/controls.rs` |
| L-07 | router fallbacks bypass every request control | **fixed** | 4 tests in `tests/controls.rs`; the router is now the stack's `fallback_service` |
| L-08 | the timeout does not bound body processing | **fixed** | 2 tests in `tests/controls.rs`, one of them the control |
| L-09 | client disconnect does not cancel the request scope | **fixed** | `dropping_a_request_in_flight_cancels_its_scope` |
| L-10 | handler panics are not contained | **fixed** | 2 tests in `tests/controls.rs`, one of them the control |
| L-11 | path parameters are never populated | **fixed** | `a_path_parameter_reaches_the_handler` |
| L-12 | contract-surface hygiene (7 parts) | **fixed** | pattern validation (8 malformed + 7 valid), status validation, whole-tree boundary scan with an injected-file control, handler span, nested groups, `body_unreadable` distinct from `413` |

**Every row is `validated`, and none is `complete`.** Under the five-status rule an implementation
agent's furthest reach is `coding_done`; three independent validation rounds moved every row to
`validated`; `complete` is a human decision that has not been taken. *(Corrected 2026-08-23 — this
paragraph previously asserted every row was still `coding_done`, contradicting the review-status
table in the same document.)*

### Three defects this remediation exposed, recorded because they were not in the twelve

| Found | Why it is here |
|---|---|
| `InitContext` carried no run identifier | A provider had no way to nest its telemetry under the run identity (C-O3), and generating its own would have created **two** run identities for one run. `InitContext::run_id()` was added to the kernel |
| An application `OPTIONS` route was **silently shadowed** | Verified: the CORS layer answered every `OPTIONS` as a preflight — empty `200`, handler never ran. A route that appears in every listing and never runs is the failure C-9 names as worse than refusal, so registration is now a reported error |
| Preflight returns **200**, not `204` | The first assertion was written against `204`. The selected library returns `200`, and ADR-0012 records that Renvor writes none of the CORS protocol — so the **test** was corrected, not the code |


### The security review of the remediated head, and what it found

One security review was run against head `07804ff` and re-verified at `4be9758`. It is **advisory
and NOT independent in the governance sense** — the definition requires a person — but it read
vendored `hyper`, `axum`, and `tower-http` source to check what the upstream layers actually do
rather than what they are assumed to do, and it found two real defects this repository's own tests
did not.

**No BLOCKER.** The headline promises hold: the trusted-proxy set is empty by default and the trust
decision is taken from the socket before any header is read; host validation has no allow-any path;
wildcard-plus-credentials is refused before a listener exists.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | MAJOR | **Repeated `Host` defeated by an undecodable second value.** The Host read filtered undecodable values out and *then* counted, so two headers with one decodable value looked like one | **Fixed.** Demonstrated first: with two `Host` headers, one undecodable, the request was served **200**. Routed through `header_values`, which keeps an unusable placeholder so the count stays truthful. Test asserts `400` |
| 2 | MAJOR | **`span.enter()` held across `.await`.** On a multi-thread runtime another request polled on the same worker inherits the span and the wrong `request_id` | **Fixed.** Replaced with `.instrument(span)`. Guarded by a source scan with a positive control, because reproducing the interleaving is a race rather than a test — weaker evidence than an observation, and recorded as such |
| 3 | MINOR | **A non-ASCII `Origin` failed open**, contradicting C-11's own fail-closed table | **Fixed.** Now refused. Proven red against the previous form |
| 4 | MINOR | **`MAX_DUMP_BYTES` measured after the read it claims to bound** — `Command::output()` collects all of stdout first | **Documented, not fixed.** The doc comment claimed it prevented memory exhaustion; that claim is **withdrawn**. The correct fix needs a piped read plus a child `kill()`, and a cross-platform subprocess test — recorded rather than attempted late. See the constant's own documentation |
| 5 | MINOR | The same-origin carve-out ignores scheme and port, so a co-hosted origin on another port is treated as same-origin | **Accepted, and it is what C-11 says.** Passing the carve-out only skips Renvor's supplementary `400`; `Access-Control-Allow-Origin` is still withheld, so the page cannot read the response. The boundary is unpinned in either direction — a named test gap |
| 6 | MINOR | CORS methods and request headers are unconditionally mirrored, with no configuration surface | **Accepted.** C-11 promises nothing here, and the reason for `mirror_request` was verified against upstream source. It is the one place the posture is permit-by-default, and it is now said out loud |
| 7 | MINOR | **Latent:** HTTP/2 would refuse every request, because `hyper`'s h2 path never synthesises `Host` from `:authority` | **Not reachable, recorded.** `h2` is absent from `Cargo.lock` and the facade pins `http1` only. The hazard is feature unification enabling it silently. Fail-closed, but it would present as a host misconfiguration — the worst possible diagnostic |
| 8 | MINOR | The panic payload still reaches **stderr** via Rust's default hook | **Documented.** Installing a process-global hook would swallow panics from threads Renvor does not own — a worse defect. Now a named limitation in C-11 |
| 9 | INFO | `renvor routes` runs `cargo run`, which is arbitrary code execution against an untrusted project directory by design; its stderr bypasses the redaction pipeline | **Accepted and named.** True of every Rust build tool; recorded so the "no binary discovery" bullet is not quoted as a stronger security claim than it is |

**Two of the nine were fixed by first reproducing the failure.** Finding 1's test was rewritten
twice before it was trustworthy: the first version passed because the host it used was not the
allowed one, and the second passed because the test helper already set a `Host`, giving two
decodable values. Only the third — one decodable value, and it the allowed host — actually put the
defect under the assertion. **A passing test is not evidence until you know why it passes.**

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
| Post-remediation validation run | **DELIVERED**, two rounds — round 1: 6 `validated`, 6 `needs_fixes`, **all six real**, two proven by mutation. Round 2: 5 of the 6 `validated`; **L-11 failed again**, because the test written to close it observed a constant handler rather than the parameter map. Round 3: L-11 `validated`. **Final: 12 `validated`, 0 `needs_fixes`, 0 `complete`.** See the [ledger](phase-004-finding-ledger.md) |
| Post-remediation security review | **DELIVERED** — 9 findings, 0 BLOCKER, 2 MAJOR. Both MAJOR findings were fixed; one was a reachable security defect |
| Post-remediation requirements review | **DELIVERED 2026-08-23**, against head `c078385`, after returning nothing across three explicit requests and six idle notifications. Full 69-item sweep: FR-001…FR-049 and SC-001…SC-020, with `cargo xtask verify` re-run on 1.94.0 (11/11, 837 passed), the `#[ignore]`d relay test executed, and crate isolation independently re-measured. **41 SATISFIED · 5 IMPLEMENTED_UNTESTED · 3 PARTIAL · 0 NOT_MET** across the FRs; **15 · 1 · 4 · 0** across the SCs. Ten findings; see §The post-remediation requirements review below |

Under this ledger's rule, *"a review that returns nothing is recorded as NOT PERFORMED, never as
passed"* — the first three are recorded that way. **An earlier revision of this document said all
five had returned nothing. Two had simply not yet been given time to reply, and that statement was
corrected as soon as they did.**

**The mechanism of the three failures is worth recording.** A subagent's final text does not reach
the caller unless the subagent explicitly sends it. Three runs ended with their findings sitting in
their own transcripts and nothing delivered — which is indistinguishable, from the caller's side,
from having found nothing. That is the exact failure mode this ledger's "empty result" rule exists
to catch, arriving by a route nobody had anticipated.

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

## The post-remediation requirements review — recorded 2026-08-23

Commissioned once, against head `c078385` exactly, read-only. It executed rather than only read:
`cargo xtask verify` on pinned 1.94.0 (11/11 steps, 837 passed / 0 failed / 1 ignored), the
`#[ignore]`d relay test run explicitly, and crate isolation re-measured in both directions
(kernel 0, lean facade 0, default facade 0, `transport-rest` 6).

**FR-001…FR-049** — 41 SATISFIED · 5 IMPLEMENTED_UNTESTED · 3 PARTIAL · **0 NOT_MET**.
**SC-001…SC-020** — 15 SATISFIED · 1 IMPLEMENTED_UNTESTED · 4 PARTIAL · **0 NOT_MET**.

Every finding below was independently re-verified before being recorded here. Three were settled by
mutation, one by direct execution, the rest by reading the cited line. Nothing is recorded on the
review's assertion alone.

| # | Finding | Verified how | Disposition |
|---|---|---|---|
| R-1 | The evidence pack and the ledger each asserted **both** that all twelve were `coding_done` **and** that all twelve were `validated`. A reader could not tell which was current | Read: pack lines 20 and 325, ledger lines 262 and 279 | **Fixed 2026-08-23.** Both documents now state `validated`, with the stale wording quoted so the correction is auditable |
| R-2 | FR-004's row was stale in the **under**-claiming direction — it described a four-file hand-list the scan no longer uses | Read `tests/boundary.rs:44` — a recursive walk with a tempfile control | **Fixed.** The row now names the real, narrower limit: the `server.rs` exemption |
| R-3 | SC-013's row said its control was "exercised by hand". It is automated in verify step 7 | Read `xtask/src/main.rs:783`; re-measured | **Fixed** |
| R-4 | FR-036's evidence cited `a_project_without_a_renvor_dependency_is_refused_by_name` — **no test of that name exists** | Grep: the real test is `..._refused_before_anything_is_run` | **Fixed.** A citation that does not resolve is not evidence |
| R-5 | `a_served_request_holds_a_permit_and_releases_it` **cannot fail** for the property its name states. It asserts `outstanding() == 0` before and after and never observes the permit held | **Mutation**: deleting `shared.admission.admit()` left it green. Six other tests did catch that mutation, so FR-029/FR-030 stand | **Fixed** — see below |
| R-6 | `Request::query()` is read by **no test in the repository**. A router that always handed handlers an empty query string would pass the entire suite | Grep: the sole occurrence workspace-wide is the producer at `route/build.rs:611` | **Fixed** — see below |
| R-7 | The `Defaulted` arm of `every_governed_choice_of_principle_seven_is_classified` hard-codes `target` while its failure messages interpolate `{choice}`. For the `transport` row it proves only that `--transport` is absent from the reserved table | Read `config/flags.rs:103` | **Fixed** — see below. Same failure class as L-11, surviving in the CLI crate |
| R-8 | `docs/docs/cli.mdx` — the **published** site — lists `routes` among commands "later phases will add — absent, not stubbed", under a heading reading "Everything below is implemented and tested". Phase 004 shipped it. The same file republishes the C-2 registry with 19 codes, omitting `transport_not_wired` | Read `cli.mdx:42`; grep: 0 occurrences in `cli.mdx`, 2 in `contracts/json-output.md` | **Fixed** |
| R-9 | `tests/router.rs:356` carries a comment stating "the CORS protocol is not implemented: the policy denies, and never grants". L-06 closed that; `an_allowed_origin_receives_the_allow_origin_header` passes | Read, and the cited test passes | **Fixed** |
| R-10 | `Server::serve` takes **`axum::Router`** in a public signature, and `Server` is re-exported at the facade root — while `server.rs` sits inside the boundary scan's own exemption list. The facade documents that re-exporting a third-party type "would make every upstream major version a Renvor breaking change"; this signature does exactly that | Read `server.rs:144`, `renvor/src/lib.rs:184`, `boundary.rs:32` | **RECORDED, NOT FIXED.** Changing a public signature is an ADR decision, not a validation edit. Raised for Ahmed |

### Recorded and deliberately not fixed

Three more are real and are **not** closed here, because closing them would expand scope past
Phase 004 validation:

1. **FR-005 / C-package-metadata.** `contracts/package-metadata.md:12` states *"A missing field
   fails metadata validation (FR-040)."* **That sentence is false.** Proven by mutation: deleting
   `keywords`, `categories`, `documentation` and `homepage` from `renvor-http`'s manifest and running
   `cargo package --workspace` exits **0** with **0** error lines — a warning only. Nothing validates
   them: not `cargo package`, not `cargo publish --dry-run`, not `xtask verify`, not any test.
   FR-040 belongs to an earlier phase and building a new gate mid-validation is precisely the
   undocumented change this sequence forbids. **Ahmed's call**: build the gate, or correct the
   contract sentence.
2. **C-11's stated method of proof.** The contract says *"For each adjacent pair there is a case
   whose result differs depending on which ran first."* Pairs **3↔4** (client identity ↔ CORS) and
   **8↔9** (body limit ↔ trace) have no such case. The implemented order is correct — `build.rs:196`
   nests it structurally — but the document claims a proof it does not fully carry. Its preflight row
   likewise asserts an empty `200` and no permit spent; the test asserts neither clause.
3. **The route-dump protocol's only end-to-end proof runs in no gate.** It is `#[ignore]`d;
   `xtask verify` does not pass `--ignored` and CI does not either. Every other relay test uses a
   hand-written fixture, so a change to the envelope shape would break the real protocol and leave
   the suite green. It was run manually at this head and passes.

## Closure of the five pre-merge items — 2026-08-23

The post-remediation requirements review at `c078385` is the **closing** review. Its findings are
dispositioned here; no further review round was commissioned.

### A — the public API boundary (R-10)

`Server` is **removed** from the `renvor` facade root. The normal path is the lifecycle-managed
`HttpServerProvider` / `HttpServerConfig` / `RouteRegistry`, all Renvor-owned. The raw path remains
as `renvor::transport::Server`, one level down, under the module that **is** the transport. Neither
the router nor the route registry is duplicated and no second manifest exists. Recorded in ADR-0012
under *The public surface this decision fixes*, with the trade-off stated.

Enforced by `crates/renvor/tests/facade_boundary.rs`, which parses the re-export list **from the
source** rather than hand-listing it, resolves each name to its `impl` blocks, and reads the declared
signatures. **Mutation-proven**: restoring `Server` to the root fails the guard, quoting
`pub async fn serve<F>(self, router: Router, …)`. Its positive control caught two real bugs in the
scanner itself — a byte/char offset mismatch and a `pub fn` search that skipped `async` methods —
which had made the guard pass while reading nothing.

### B — required package metadata

`cargo xtask verify` step 7 now validates all eleven required fields plus the explicit file set, for
every publishable package, resolving `workspace = true` inheritance against `[workspace.package]`.
Eight tests: a table-driven negative per field, a positive control, an `exclude`-instead-of-`include`
control, an unresolvable-inheritance case, an exemption case, and the real workspace. **Mutation-
proven**: deleting `keywords` from `renvor-http` fails the gate by name.

**A second defect surfaced while building it.** Both this check and the pre-existing FR-040
dependency scan tested `text.contains("publish = false")` against the raw manifest. `renvor-core`,
`renvor-http` and `renvor-testkit` each *discuss* `publish = false` in a leading comment explaining
why they are **not** marked that way — so all three were read as unpublishable and skipped. **Three
of the five publishable packages went unexamined by both gates, which reported success.** A comment
was switching off a gate. Fixed by a comment-aware `is_publishable`, with a regression test and two
controls. `cargo metadata` says five; the scan now says five.

### C — C-11 ordering evidence

Both missing adjacent pairs now have discriminating cases, and the contract's claim was **made true
rather than narrowed**:

- **8 ↔ 9**: an over-limit body is refused **without** the handler span being opened. Mutation-
  proven by moving the span outside the body limit.
- **3 ↔ 4**: a preflight is answered **beneath** the context block and therefore still carries a
  request identifier. Mutation-proven by moving the CORS layer outside the context layer.

C-11 now also states what the nine rows **structurally are** — layers 1–3 are one tower layer, so
their internal order is not independently observable — and tabulates the discriminator for every
boundary. A reader looking for a 1 ↔ 2 composition discriminator would previously have concluded the
evidence was missing when it was the model that was wrong.

The preflight test asserts all three previously unobserved clauses: status `200`, an **empty** body,
and **no permit spent**.

### D — the route-relay end-to-end gate

`cargo xtask verify` step 4 now invokes the exact `#[ignore]`d test explicitly. `#[ignore]` is
retained on its merits: the test spawns a nested `cargo run` to build a second crate's example, which
would contend with the build lock held by the process that spawned it. CI runs `cargo xtask verify`,
so it now runs in CI through the normal entry point. **The sequence remains eleven steps** — this is
a second command inside step 4, not a twelfth step. The three envelope-incompatibility controls are
preserved, and no fixture replaced the real protocol test.

### E — every non-SATISFIED entry, dispositioned

Thirteen entries. **None was relabelled without new executable evidence, and no requirement was
weakened.**

| ID | Was | Requirement, in brief | Disposition |
|---|---|---|---|
| FR-005 | IMPLEMENTED_UNTESTED | Complete package metadata, and appears in the release ordering | **SATISFIED** — step 7 validation, 8 tests, mutation-proven |
| FR-026 | IMPLEMENTED_UNTESTED | Header bounds documented where exposed; **named as unexposed** otherwise | **SATISFIED** — `renvor_claims_no_header_bound_it_does_not_set` asserts `Limits` publishes no header field and C-10 still names the gap |
| FR-027 | IMPLEMENTED_UNTESTED | Middleware order defined in a **versioned public contract** | **SATISFIED** — `the_middleware_order_is_a_versioned_contract_that_matches_the_code`: frontmatter version, nine ordered rows, and the code's own diagram agreeing |
| FR-039 | IMPLEMENTED_UNTESTED | Workspace evidence distinguished from generated-project evidence | **SATISFIED** — `the_phase_evidence_distinguishes_workspace_from_generated_project_evidence` |
| FR-043 | IMPLEMENTED_UNTESTED | No Problem Details, no OpenAPI, no Phase 005 boundary | **SATISFIED** — comment-stripped source scan with a two-sided control |
| FR-036 | PARTIAL | Truthful about where route metadata comes from | **SATISFIED** — `cli.mdx` corrected; two guards added, both mutation-proven |
| FR-042 | PARTIAL | Structured fields, **subject to the redaction rule without exception** | **SATISFIED** — `no_request_supplied_value_reaches_the_adapters_telemetry`; mutation-proven by adding the query string to the span |
| FR-049 | PARTIAL | Wizard **must not** gain a transport question | **SATISFIED** — the question set is enumerated from source and pinned at six, with a control |
| SC-003 | PARTIAL | Routing surface exercised through the real router | **SATISFIED** — `query()` had no reader; two tests added, mutation-proven |
| SC-008 | PARTIAL | Every limit asserted at its **exact boundary and boundary + 1** | **SATISFIED** — decided under a paused clock. The review called it undecidable for a `Duration`; it is decidable once the clock is. **The boundary was measured, not assumed**: a handler finishing at exactly the budget succeeds, and one nanosecond beyond times out. The first draft asserted the opposite and its failure established the real semantics |
| SC-012 | PARTIAL | **0** transport types in application-facing interfaces | **SATISFIED** — see A |
| SC-017 | PARTIAL | Every negative check carries a positive control | **SATISFIED** — the `Defaulted` arm asserted `target` for the `transport` row; each row now asserts its own type |
| SC-020 | IMPLEMENTED_UNTESTED | No document claims workspace integration as generated-project evidence | **SATISFIED** — same test as FR-039 |

**Reconciled totals: FR-001…FR-049 — 49 SATISFIED, 0 PARTIAL, 0 IMPLEMENTED_UNTESTED, 0 NOT_MET.
SC-001…SC-020 — 20 SATISFIED, 0 PARTIAL, 0 IMPLEMENTED_UNTESTED, 0 NOT_MET.**

### Limitations retained, honestly

Unchanged and still true: the panic payload reaching process stderr; the header bounds Renvor does
not set; no crate published, so `renvor routes` reaches zero generated projects; registry names
verified rather than reserved; the macOS trust-store test's machine sensitivity. Each is permitted by
its governing requirement and none is waived.

## A gate that was not a gate — found and fixed 2026-08-23

The complete exact-head sequence returned **`FINAL_FAIL=1`**: `cargo xtask verify` **passed** on
1.94.0 and **failed** on stable, at step 8, over an **unchanged tree**. HEAD and tree were identical
before and after; the worktree held 0 entries.

**Root cause.** The working-tree secret scan (`gitleaks dir .`) reads **generated build output**.
`target/` is ignored by tracked `.gitignore` line 10 and appears in 0 commits, but `gitleaks dir`
does not honour that, so the scan's verdict depended on what had been compiled:

| Scanned | Result |
|---|---|
| 190 MB | clean |
| 301 MB | clean |
| 376 MB | clean |
| **531 MB** | **`square-access-token` in `target/doc/search.index/<hash>.js`** |
| **7.07 MB** *(after the fix)* | clean, and now matching the repository's actual content |

The finding is `cargo doc`'s compressed search index: one chunk begins `EAAA`, the literal prefix the
Square rule keys on. It is regenerated with different bytes — and a different file name, which is a
content hash — on every documentation build. It only appeared once a **second** toolchain's
`cargo doc` had run, which is precisely why the first `verify` passed and the second did not.

**This was fixed, not re-run.** A gate whose verdict depends on build state is not a gate, and the
sequence that produced it — run `verify` twice over one tree — is the sequence this project uses.
Re-running until it passed would have buried a non-deterministic control.

**The fix is narrower than the failure and was verified in three parts**, because this repository's
own gitleaks policy records an incident where a `paths` allowlist caused `scanned ~0 bytes` and let
an injected canary through:

1. the false positive is gone and the scan now reads 7.07 MB rather than 531 MB;
2. a canary AWS key and secret planted in an untracked `canary_check.rs` at the repository root **is
   still reported** (`aws-access-token`) with the entry active;
3. the same canary placed **under `target/`** is not reported — the entry doing exactly its job and
   nothing wider.

Recorded because it produced a false reading first time: **`AKIAIOSFODNN7EXAMPLE` is not a usable
canary.** It is AWS's own documentation example and gitleaks allowlists it by default, so the first
canary run reported "not detected" and would have been read as a broken scanner rather than a bad
test input.

The committed-history scanner (`gitleaks git`) in the same step is untouched and still reads all
335 commits.

## Authority — recorded 2026-08-23

**There is no independent human reviewer.** Nothing below claims otherwise. Ahmed Anbar, Claude,
Codex, and every subagent are the author or automated; **an automated reviewer is not a person and
is therefore not independent under any reading of the `GOVERNANCE.md` criteria.** Every review
performed inside Phase 004 is **advisory and non-independent evidence only**.

Two waivers were granted on Ahmed's explicit authorisation, at two levels, because the waiver ledger
does not collapse those axes:

| Waiver | Waives | Applied to | Expiry |
|---|---|---|---|
| **W-009** | constitution §Development and Phase Workflow **#4** and spec FR-013 — a decision record MUST NOT be accepted without a recorded **independent** review | **ADR-0012 only** | **2027-02-11** |
| **W-010** | constitution §Development and Phase Workflow **#7** and `PLAN.md` §6.1 step 10 — a phase MUST obtain an independent requirements-and-security review before it closes | **Phase 004 only** | **2027-02-11** |

**Why two and not one.** The ledger states it as a grant term: *"A record-level waiver does not
authorise closing a phase, and a phase-level waiver does not authorise accepting a decision record
— which is exactly why Phase 002 needed three new waivers rather than an extension of the two that
existed."* One record covering both gates would have been a reinterpretation, which the same
paragraph forbids.

**The expiry did not move.** Both inherit **2027-02-11**, the earliest open expiry, under the
ledger's ratchet rule. Eight waivers now share that date; the horizon has not moved once since it
was set, which is the rule working as intended.

**The count is at the limit, not over it.** "Explicit reviewed exceptions" permits at most two per
phase. Phase 004 holds exactly two. The approval-waiver count stays 1 (W-001) and
control-unavailability stays 0.

**W-010 is the fourth consecutive phase-level waiver** — W-003, W-005, W-008, W-010. The trend guard
was already **TRIPPED** at three and a fourth deepens rather than re-trips it. **RO-001 has not
moved**: no candidate reviewer has been approached, its first review date of **2026-11-19** has not
arrived, and its failure condition now governs four phase-level waivers. Its progress log carries a
dated entry saying exactly that, and nothing in this pack should be read as progress against it.

**What these waivers do not do.** They confer nothing on Phase 005. They waive no product defect, no
missing test, no missing functionality, no publication rule, and no CI failure. Where a requirement
was unevidenced the remedy applied was a test — thirteen entries closed that way **before** the
grant, not by it. The named limitations Phase 004 carries are permitted by their governing
requirements and are **not** waived by either record.

The constitution was not amended.
