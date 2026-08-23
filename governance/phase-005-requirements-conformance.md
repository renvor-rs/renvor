# Phase 005 — Requirements Conformance

**Date**: 2026-08-23
**Head**: `d0f92cf4de82183aade5d6d8ac8e08271da796c2` **plus the closing remediation** (D-9, D-10 and
three documentation corrections); the final head is recorded in `governance/waivers.md` under W-012.
**Scope**: FR-001…FR-066 and SC-001…SC-022 from
[`specs/005-validation-problem-openapi/spec.md`](../specs/005-validation-problem-openapi/spec.md).

> **This mapping is maintainer self-review.**
>
> The validation review commissioned to produce it returned no report, twice — see
> §9a of [`phase-005-evidence.md`](phase-005-evidence.md). It is **not** agent validation and it is
> **not** independent human review. Its gap is waived under **W-012**, not closed by it.

**Result: 66/66 FR and 22/22 SC satisfied. 0 PARTIAL, 0 NOT_MET, 0 UNTESTED.**

Two entries reached this state only because the closing audit fixed them — FR-043 and FR-044 were
**NOT MET** at `d0f92cf` and are satisfied at the final head. That is recorded here rather than
presented as though they were always green.

---

## Functional requirements

| FR | Status | Evidence |
|---|---|---|
| FR-001 | SATISFIED | `renvor-validation` names no transport type; `tests/boundary.rs` asserts it and `xtask` step 7 resolves the crate's graph with a positive control |
| FR-002 | SATISFIED | `renvor_error::Location` — `Path`, `Query`, `Header`, `Body`. Cookie deliberately absent (L-4, Phase 009) |
| FR-003 | SATISFIED | `Issue` carries location, `Pointer` (RFC 6901), and `Reason`; `reason.rs` tests assert 19 distinct lower_snake_case names |
| FR-004 | SATISFIED | **Structural**: `InvalidParam` has no value field and `Reason::as_str` returns `&'static str`, which cannot hold a formatted string. `renvor-http/tests/problem_details.rs` canary suite with positive controls |
| FR-005 | SATISFIED | Multi-issue collection with `MAX_ISSUES = 100` and a `TooManyIssues` marker so truncation is stated, not silent |
| FR-006 | SATISFIED | Deterministic walk order; asserted in `renvor-validation/tests/boundary.rs` |
| FR-007 | SATISFIED | One `Declaration` holds one `serde_json::Value`; `validate()` interprets it and `schema()` publishes it. ADR-0014. No second registry exists |
| FR-008 | SATISFIED | `RequestRejection` is a distinct outcome from constraint violation; `renvor-http/tests/router.rs` |
| FR-009 | SATISFIED | `ApiErrorCode`, 12 codes, closed, `API_ERROR_REGISTRY_VERSION = 1`; contract-parsing test in `code.rs` |
| FR-010 | SATISFIED | [`contracts/problem-details.md` §Adding, retiring, versioning](../contracts/problem-details.md) — verified present 2026-08-23 |
| FR-011 | SATISFIED | `mapping.rs` holds `CLI_ERROR_CODES: [&str; 20]` and asserts **disjointness**; there is deliberately no conversion function |
| FR-012 | SATISFIED | `problem.rs` + `route/build.rs`; `application/problem+json` on every failure path including the 404 fallback |
| FR-013 | SATISFIED | `type`/`title`/`status` per RFC 9457; status equality asserted in `problem_details.rs` |
| FR-014 | SATISFIED | `code` is a documented extension member, not an overloaded standard member |
| FR-015 | SATISFIED | One `RequestId`, three renderings — body, header, telemetry — asserted equal (SC-018) |
| FR-016 | SATISFIED | `invalidParams` entries carry location, pointer, reason and **nothing else**; bounded in the walker after D-5 |
| FR-017 | SATISFIED | `ApiErrorCode::InternalError.detail()` is the fixed string *"The request could not be completed."* |
| FR-018 | SATISFIED | Leak suite with canaries **and positive controls** proving the search would find a leak if one existed |
| FR-019 | SATISFIED | **Structural**: `detail()` is a `const fn` returning `&'static str` per code — no runtime value can inhabit it |
| FR-020 | SATISFIED | Causal chains preserved in telemetry; `renvor-http/tests/telemetry.rs` |
| FR-021 | SATISFIED | `OPENAPI_VERSION: &str = "3.2.0"`, a `pub const` with **no setter**; proof 1 |
| FR-022 | SATISFIED | Proof 2 — validates against the vendored **official** 3.2 schema via `jsonschema`, a validator independent of the generator. Schema verified **byte-identical to upstream** 2026-08-23 |
| FR-023 | SATISFIED | Proof 3 (structural 3.1 rejection with the version pattern neutralised) + proof 4 (the control: a relabelled 3.1 document passes that same check) |
| FR-024 | SATISFIED | `describe::document` asserts route completeness **in both directions**; `renvor-http/tests/describe.rs` |
| FR-025 | SATISFIED | `DescribeError::DuplicateOperationId`, with a positive control that distinct ids still generate |
| FR-026 | SATISFIED | Content types declared and asserted against what the router accepts and produces |
| FR-027 | SATISFIED | Differential test against `jsonschema` over generated pairs (SC-004) — exercises the runtime rather than comparing two documents |
| FR-028 | SATISFIED | Shared `ProblemDetails` component built from the **same** `ApiErrorCode::ALL` the runtime raises, so it cannot fall behind |
| FR-029 | SATISFIED | Examples validated against their own schemas, with a positive control; `an_example_that_contradicts_its_schema_fails_generation` |
| FR-030 | SATISFIED | Every open-ended map is a `BTreeMap`; `generation_is_deterministic_to_the_byte` and `open_ended_maps_are_emitted_in_sorted_order` |
| FR-031 | SATISFIED | Generation runs with **no async runtime at all**, and every forbidden operation needs a reactor. Asserted observably |
| FR-032 | SATISFIED | Same as FR-031 — nothing in the document model reads server state |
| FR-033 | SATISFIED | Schemas come from `schemars` derive, never from source parsing; no second manifest is read (FR-051 shares the assertion) |
| FR-034 | SATISFIED | `Cursor`, `CURSOR_VERSION = 1`, base64url-unpadded, **version byte first** |
| FR-035 | SATISFIED | `CursorError` distinguishes version-unsupported by name; `cursor_property.rs` |
| FR-036 | SATISFIED | `proptest` over generated and hostile input — **0 panics** (SC-015) |
| FR-037 | SATISFIED | Declared default/min/max; out-of-bounds is **refused naming the bound**, never clamped; `collection.rs` |
| FR-038 | SATISFIED | Total ordering required and asserted; `contracts/collection-reads.md` |
| FR-039 | SATISFIED | Deny-by-default allowlists for filter, sort, include and field selection |
| FR-040 | SATISFIED | Duplicate query key is `Reason::DuplicateKey` — never last-wins or first-wins |
| FR-041 | SATISFIED | Closed operator set; term/include/field counts each bounded; `RESERVED_PARAMETERS: [&str; 5]` |
| FR-042 | SATISFIED | No persistence crate in any Phase 005 graph; asserted by `xtask` and by the dependency inventory |
| FR-043 | **SATISFIED — fixed in the closing audit** | Was **NOT MET** at `d0f92cf`: no snapshot existed anywhere. Now `crates/renvor-openapi/tests/snapshots/public-description.json` is committed and read with `include_str!`; owner and refresh procedure stated in C-14. See **D-10** |
| FR-044 | **SATISFIED — fixed in the closing audit** | Was met only as a library function at `d0f92cf`, with nothing committed to compare against. Now `the_generated_description_matches_the_committed_snapshot` compares generated against committed, **semantically** |
| FR-045 | SATISFIED | 16 breaking mutations, each asserted to fail; `tests/compatibility.rs` |
| FR-046 | SATISFIED | 7 harmless mutations, each asserted to pass — as load-bearing as the breaking ones |
| FR-047 | SATISFIED | [C-14 §Declaring an intended break](../contracts/openapi.md) — new path version prefix with the previous version retained |
| FR-048 | SATISFIED | `a_regenerated_snapshot_cannot_approve_its_own_breaking_diff`, **now comparing against the real committed file** rather than an in-memory twin. The refresh generator only prints; it cannot write the file it would approve |
| FR-049 | SATISFIED | Inherited from the verification-sequence contract; `xtask` step 1 exits non-zero on missing tooling and runs no steps |
| FR-050 | SATISFIED | `renvor openapi`, versioned bounded metadata-free protocol; `crates/renvor-cli/src/commands/openapi.rs` |
| FR-051 | SATISFIED | No source parsing, no second manifest; the answer comes from the application's own declarations |
| FR-052 | SATISFIED | Protocol version checked **before** the payload is read; unrecognised version refused by name |
| FR-053 | SATISFIED | Bounded by timeout via `wait-timeout`; `RelayFailure::Timeout` is distinct. Fixed under D-6 |
| FR-054 | SATISFIED | Six distinct named failures, none producing an empty success; `renvor-cli/tests` relay matrix (SC-017) |
| FR-055 | SATISFIED | The application answers and exits **before** boot; asserted observably |
| FR-056 | SATISFIED | A project with no framework dependency fails with the registered code and a non-zero exit |
| FR-057 | SATISFIED | Exactly one document in the established envelope on `stdout`, success and failure alike |
| FR-058 | SATISFIED | `renvor-validation` and `renvor-error` resolve **0** HTTP/router/middleware crates, with a positive control (SC-013) |
| FR-059 | SATISFIED | `renvor-openapi` does not depend on `renvor-http`; the dependency runs the other way |
| FR-060 | SATISFIED | Facade boundary guard parses the re-export list from source and reads declared signatures; **mutation-proven** in Phase 004 |
| FR-061 | SATISFIED | `xtask` step 7 asserts the crate DAG with positive controls; verified again 2026-08-23 — 8 publishable crates, valid topological publish order |
| FR-062 | SATISFIED | Feature isolation asserted **in both directions** with a positive control |
| FR-063 | SATISFIED | `xtask` step 7 asserts complete package metadata for **exactly 8** publishable packages; `include` lists are explicit |
| FR-064 | SATISFIED | `RELEASING.md` ordering updated for all three new crates; the facade row was **corrected** in the closing audit to list all six workspace dependencies |
| FR-065 | SATISFIED | `missing_docs = "warn"` with `-D warnings` in clippy and rustdoc, so it is denied in the gate; contracts C-12…C-15 published |
| FR-066 | SATISFIED | 17 limitations, each with owner and target phase; L-10 and L-14 **closed** by the closing audit rather than carried |

## Success criteria

| SC | Status | Evidence |
|---|---|---|
| SC-001 | SATISFIED | Canary suite with a positive control proving the search discriminates |
| SC-002 | SATISFIED | Same suite; secrets, paths, SQL, stack traces, panic payloads and cause chains all asserted absent |
| SC-003 | SATISFIED | Both directions asserted — no route missing, no operation invented |
| SC-004 | SATISFIED | Differential test against `jsonschema`, exercising the runtime |
| SC-005 | SATISFIED | Proofs 1, 2 and 5 — declares 3.2.0, validates against the official schema, and rejects four malformed documents |
| SC-006 | SATISFIED | Proof 3 with the version constraint neutralised, so the rejection is structural |
| SC-007 | SATISFIED | Every declared example validated against its own schema |
| SC-008 | SATISFIED | Byte-identical across runs; now also across the **committed** snapshot |
| SC-009 | SATISFIED | No runtime present during generation, so no listener, connection, migration, network call or provider can start |
| SC-010 | SATISFIED | 16/16 breaking mutations fail; 0/7 harmless mutations fail |
| SC-011 | SATISFIED | The bypass is attempted and required to fail, against the real committed file |
| SC-012 | SATISFIED | 0 third-party HTTP/OpenAPI types in facade-root signatures |
| SC-013 | SATISFIED | 0 server/router/middleware crates resolved, with a positive control proving the query works |
| SC-014 | SATISFIED | 0 cycles; asserted by `xtask` |
| SC-015 | SATISFIED | 0 panics across generated and hostile cursor input |
| SC-016 | SATISFIED | 0 silent clamps and 0 silent last-wins |
| SC-017 | SATISFIED | All six failure modes produce distinct named outcomes |
| SC-018 | SATISFIED | Correlation identifier equal in body, header and telemetry |
| SC-019 | SATISFIED | Every load-bearing guard added in this phase was mutated and observed to fail — including both guards added in the closing audit (D-9 and D-10) |
| SC-020 | SATISFIED | `cargo xtask verify` 11/11 on 1.94.0 and stable, 0 skipped steps |
| SC-021 | SATISFIED | Minimal, default, REST and all-feature builds each succeed |
| SC-022 | SATISFIED | 0 untracked or modified files after a full run |
