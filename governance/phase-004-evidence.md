# Phase 004 evidence — REST routing and HTTP runtime

**Base**: `10da854736598d99218d1627c3ad79866a2f7f89` · **Branch**: `feat/phase-004-rest-http-runtime`
**Date**: 2026-08-22

> **Phase 004 is NOT closed.** This is the evidence pack, not a completion record. The governance
> gate that remains open is stated in [§Review status](#review-status--stated-plainly) and is the
> reason nothing here should be read as a closure claim.

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
| FR-004 no transport type in app interfaces | `route/mod.rs` | `tests/boundary.rs` ×3, incl. positive control |
| FR-005 publish metadata | `crates/renvor-http/Cargo.toml` | `cargo package -p renvor-http --list`; `xtask` publishable-dependencies check |
| FR-006 single authoritative registry | `route/registry.rs` | `route::registry::tests::*` |
| FR-007 router and inspection share it | `route/build.rs`, `route/inspect.rs` | `a_route_added_to_the_registry_appears_without_a_second_manifest_being_touched` |
| FR-008 route groups | `route/mod.rs::RouteGroup` | `a_group_prefixes_every_route_it_holds`, `a_group_prefix_reaches_the_real_router` |
| FR-009 duplicate is an error | `route/registry.rs::push` | `a_duplicate_is_refused_and_the_first_route_survives` |
| FR-010 route ceiling | `limits::MAX_ROUTES` | `the_route_ceiling_is_enforced_at_its_exact_boundary` |
| FR-011 404, and 405 with `Allow` | `route/build.rs` | `an_undeclared_path_is_404`, `an_undeclared_method_on_a_declared_path_is_405_and_names_the_allowed_methods` |
| FR-012 state bridge | `renvor_core::TypedStateMap` via `RequestContext` | kernel's own suite |
| FR-013 missing state is explicit | `error.rs::StateUnavailable` | `a_request_without_connection_information_fails_closed` |
| FR-014 request id nested under run id | `context.rs`, `route/build.rs` | `every_response_carries_a_generated_request_identifier` |
| FR-015 inbound id untrusted | `request_id.rs` | `a_caller_supplied_request_identifier_is_never_adopted` |
| FR-016 id is opaque | `context.rs::RequestId` | `a_request_id_is_a_pure_function_of_its_bytes`, `generation_does_not_consult_any_inbound_value` |
| FR-017 trusted set empty by default | `identity/trusted.rs` | `the_default_trusts_nobody` |
| FR-018 forwarding ignored unless trusted | `identity/mod.rs::resolve` | `under_the_default_configuration_hostile_headers_cannot_forge_identity`, `hostile_forwarding_headers_cannot_forge_client_identity_by_default` |
| FR-019 parsing fails closed | `identity/forwarded.rs` | 8 tests incl. `every_hostile_x_forwarded_for_fails_closed`, `a_quoted_value_containing_a_separator_is_not_split_inside_the_quotes` |
| FR-020 host validation fails closed | `host.rs` | 8 tests incl. `every_malformed_form_fails_closed` |
| FR-021 CORS deny-by-default | `cors.rs` | `the_default_policy_allows_no_origin`, `cors_denies_by_default` |
| FR-022 wildcard+credentials refused at config time | `cors.rs::validate`, `route/build.rs::router` | `a_wildcard_with_credentials_is_refused_when_the_policy_is_built`, `a_router_cannot_be_built_for_an_unsafe_cors_configuration` |
| FR-023 body limit at exact boundary | `limits.rs`, `route/build.rs` | `a_body_at_the_limit_passes_and_one_byte_more_does_not` |
| FR-024 concurrency limit | `admission.rs` | `the_concurrency_ceiling_is_enforced_at_its_exact_boundary` |
| FR-025 timeout | `route/build.rs` | `a_timed_out_request_is_408_and_cancels_its_scope` |
| FR-026 unexposed bounds NAMED | `limits.rs` module docs, `http-runtime.md` | prose; **no test, and none is possible** — see gaps below |
| FR-027 middleware order in a versioned contract | `contracts/http-security.md` | — |
| FR-028 order proven by behaviour | `route/build.rs::dispatch` | 4 adjacent-pair tests in `tests/lifecycle.rs` |
| FR-029 admission through the work gate | `admission.rs` | `a_served_request_holds_a_permit_and_releases_it` |
| FR-030 drain refuses new requests | `admission.rs`, `route/build.rs` | `once_drain_begins_a_new_request_does_not_reach_a_handler` + control |
| FR-031 cancellation reaches services | `context.rs`, `route/build.rs` | `application_shutdown_cancels_an_in_flight_request_without_any_transport_type` + control |
| FR-032 bounded drain, truthful outcome | `server.rs` | `draining_waits_for_the_gate_to_close_and_then_bounds_the_wait`, zero-budget test, + control |
| FR-033 provider shutdown ordering | inherited from C-L1/C-L3 | kernel's own suite; **not re-tested here** — see gaps |
| FR-034 route inspection, both forms | `route/inspect.rs` | 6 tests |
| FR-035 structured output follows C-2 | `route/inspect.rs`, `commands/routes.rs` | `the_dump_is_a_single_parseable_json_document` |
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
| FR-046 `routes` relays or fails by name | `commands/routes.rs` | 5 tests incl. positive control |
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
| SC-002 404/405, `Allow` in 100% | **met** |
| SC-003 groups, state, middleware by behaviour | **met** |
| SC-004 0 forged identities by default | **met** — asserted at the resolver and through the real router |
| SC-005 0 adopted caller identifiers | **met** |
| SC-006 fail-closed in 100% of malformed cases | **met** — 8 host cases, 11 forwarding cases |
| SC-007 CORS denies in 100%; wildcard+credentials refused in 100% | **met** |
| SC-008 every limit at boundary and boundary+1 | **met** for body, concurrency, route ceiling. Timeout is asserted at the threshold only — a timeout has no "one unit under" that is meaningfully different |
| SC-009 cancellation 100% / 0% | **met**, with the normal-completion control |
| SC-010 0 requests reach a handler after drain | **met** — the handler records whether it ran |
| SC-011 over-budget reports outstanding 100%, clean 0% | **met** |
| SC-012 0 transport types in app interfaces | **met**, with a positive control |
| SC-013 kernel resolves 0 HTTP crates, both directions | **met**, control exercised by hand (see dependency inventory) |
| SC-014 one registry, by construction | **met** — `inspect` takes `&RouteRegistry` and has no other source |
| SC-015 full verification on both toolchains | **met** — 11/11 on 1.94.0 and on 1.97.1 |
| SC-016 0 tags, releases, publications, deployments | **met**, measured |
| SC-017 every negative check has a positive control | **met** |
| SC-018 generated project builds | **met** — `cargo build` and `cargo run` both succeed |
| SC-019 0 empty-table-and-exit-0 | **met** |
| SC-020 workspace evidence labelled as such | **met** — stated here and in the dependency inventory |

## Verification

`cargo xtask verify` — **11 of 11 steps ran and passed**, on **both**:

| Toolchain | Result |
|---|---|
| 1.94.0 (pinned MSRV) | all 11 steps passed |
| 1.97.1 (current stable) | all 11 steps passed |

`cargo deny check` reported `advisories ok, bans ok, licenses ok, sources ok`.
Working tree clean and HEAD unchanged after each run.

## Review status — stated plainly

**No independent human review of Phase 004 has occurred.**

Automated and agent reviews are **advisory and explicitly NON-INDEPENDENT**, and must never be
described otherwise — here, in the pull request, in `GOVERNANCE.md`, or in any public document.

**Three advisory agent runs commissioned during this phase returned nothing** — one research run and
two plan reviews — after hitting an account usage limit. Under this ledger's own rule, *"a review
that returns nothing is recorded as NOT PERFORMED, never as passed."* They are recorded as **not
performed**. The package research they were to produce was instead carried out directly against
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
