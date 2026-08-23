# Phase 005 — Evidence

**Phase**: 005 — Validation, Problem Details, and OpenAPI
**Date**: 2026-08-23
**Branch**: `feat/phase-005-validation-problem-openapi`
**Base**: `731d28dcd1b3796f799293996801e109e7681e9c` (live `main`)
**Status**: **open** — see §8. This phase has **no** independent human review and **no** waiver.

---

## 1. What this phase delivers

PLAN.md §Phase 005's acceptance criteria, and where each is met:

| Criterion | Where |
|---|---|
| *runtime validation and published schemas agree* | §3 — an **identity**, not a test result |
| *production errors never expose sensitive internals* | §4 |
| *every public route appears in OpenAPI* | §5 |
| *breaking API changes fail the compatibility gate unless explicitly versioned* | §6 |

Three crates were added: `renvor-error`, `renvor-validation`, `renvor-openapi`.

---

## 2. The OpenAPI 3.2.0 gate — the fail-closed proof

Constitution principle V forbids claiming a version the tooling does not implement, and PLAN.md
§11.1 says the phase *"remains blocked until selected tooling can emit and validate the promised
version correctly."*

**No maintained Rust crate emits 3.2.0.** `utoipa` 5.5.0 emits `"3.1.0"` — established by compiling
a `#[derive(OpenApi)]` type and printing the field, not by reading its documentation. The full
matrix is in [`phase-005-dependency-inventory.md`](phase-005-dependency-inventory.md) §3.

Renvor therefore serialises the document ([ADR-0013](../decisions/0013-openapi-3-2-document-serialiser.md)),
and the claim is proven rather than asserted:

| # | Proof | Result |
|---|---|---|
| 1 | The document declares `3.2.0`, and the version is a **constant** with no setter | **pass** |
| 2 | It validates against the **official** OAS 3.2 schema, via `jsonschema` 0.50.1 — independent of the generator | **pass** |
| 3 | The **same** document is rejected by the **official** 3.1 schema **with 3.1's version pattern neutralised** — so the rejection is structural, not a version-string mismatch | **pass** |
| 4 | **Control** — a genuinely relabelled 3.1 document *passes* that neutralised check, proving the discriminator discriminates rather than rejecting everything | **pass** |
| 5 | **Controls** — four malformed documents are rejected by the 3.2 schema, proving the validator is not vacuous | **pass** |

Proof 3 is the one that matters. OpenAPI 3.2 is largely backwards compatible, so a relabelled 3.1
document validates against the 3.2 schema perfectly well — a gate checking only proofs 1 and 2 would
pass for exactly the relabelling principle V forbids.

It works because **both** official schemas use `unevaluatedProperties: false`, so a document
carrying a 3.2-only member is structurally invalid against 3.1. Generated documents therefore always
carry 3.2-only constructs: `$self`, Response `summary`, Tag `summary`/`kind`, and Response objects
with **no** `description` — which 3.1 requires and 3.2 does not.

**Schemas are vendored, unmodified**, so the gate runs offline. Sources and dates in the dependency
inventory §9.

**Suite**: `crates/renvor-openapi/tests/openapi_3_2_gate.rs` — 12 tests.

---

## 3. Runtime validation and the published schema are one value

Not two documents that agree. **The same `serde_json::Value`**, reached from both directions:

```
                    RouteRegistry  (one value)
           ╱               │              ╲
   build::router    inspect::render   describe::document
     dispatch         route table       OpenAPI 3.2.0
           │                                  │
           └──────── the SAME schema ─────────┘
```

`describe::document` takes `&RouteRegistry`, exactly as the other two do — there is no other source
it could read. Phase 005 added a **third consumer** without adding a second collection.

**Asserted** in `crates/renvor-http/tests/describe.rs`: the published schema for a parameter is
compared by equality against the registry's own copy, and then the same declaration is exercised at
runtime to prove the equality is between two *used* values rather than two unused ones.

### The interpreter agrees with the standard

Renvor interprets a bounded JSON Schema subset rather than resolving a validator into the transport
— measured at **103 packages against 65**
([ADR-0014](../decisions/0014-schema-as-single-source-and-bounded-interpreter.md)).

`crates/renvor-validation/tests/differential.rs` asserts that Renvor's verdict **equals**
`jsonschema` 0.50.1's over a corpus covering **every** enforced keyword, including the cases a naive
implementation gets wrong: code-point string lengths, `1.0` as an integer, `0.3` as a multiple of
`0.1` despite binary floating point, and structural `uniqueItems`. A further test fails if a keyword
is added to the enforced set without a differential case.

**A declaration using an unsupported keyword is refused at declaration time**, so an unenforceable
constraint never ships.

---

## 4. Production errors expose nothing

Three guarantees hold **by construction**, not by review:

| Guarantee | Mechanism |
|---|---|
| `detail` cannot carry runtime data | it is `&'static str` |
| An invalid parameter cannot carry the rejected value | the type has **no field** one could occupy |
| A reason cannot be a validator's message | it is `&'static str` |

The third exists because a real validator produced `"not an object" is not of type "object"` during
this phase's tooling work — the rejected value is inside the message text.

**Suite**: `crates/renvor-http/tests/problem_details.rs` — 13 tests through a **real router**, every
negative search paired with a positive control proving the probe discriminates. Canaries are sent as
values, as attacker-chosen member names, and inside panic payloads. A separate test sweeps every
failure path for `src/`, `.rs:`, `panic`, `unwrap`, `backtrace`, `thread '`, SQL keywords, and
filesystem prefixes.

**Correlation**: the document's `correlationId`, the `x-request-id` header, and the telemetry field
are rendered from **one** `RequestId`. Asserted by equality across all three.

**A real gap was found by these tests and fixed**: the router's 404 fallback still answered in plain
text. It now emits a Problem Details document carrying a correlation identifier, as every other
refusal does.

---

## 5. Route completeness, both directions

`crates/renvor-http/tests/describe.rs` compares the registry's `(method, path)` set against the
description's, by equality. Both directions in one assertion: no route missing, no operation that no
route declares.

A route registered **without** declarations still appears, described as an operation with no
declared inputs — a missing route is the failure the rule exists to prevent, and "the author
declared no inputs" is not a reason to describe a different API from the one being served.

**Determinism**: two generations produce byte-identical output; open-ended maps are emitted sorted.

**No side effects**: generation runs with **no async runtime at all**, and a positive control proves
the absence of a runtime is detectable in that test. Binding a listener, opening a connection, or
starting a provider all require a reactor.

---

## 6. The compatibility gate

`crates/renvor-openapi/tests/compatibility.rs` — 27 tests.

| | Count | Result |
|---|---|---|
| Breaking mutations that **must** fail the gate | 16 | all detected, each with its specific classification |
| Harmless mutations that **must not** fail it | 7 | **zero** false positives |
| Gate-integrity tests | 4 | pass |

Both directions are required. A gate that failed on every change would be an obstacle, and an
obstacle gets routed around.

**FR-048** — that a regenerated snapshot cannot approve its own breaking diff — is asserted by
**attempting the bypass**: a self-comparison is shown to be silent, then the same broken document is
compared against the committed baseline and the break is caught.

---

## 7. Verification

| Gate | Result |
|---|---|
| Workspace tests, `--all-features` | **1024 passed, 0 failed, 1 ignored** |
| `cargo fmt --all --check` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `renvor-http` suite | 205 passed |
| `renvor-cli` suite | 236 passed |
| `renvor-validation` suite | 45 passed |
| `renvor-openapi` suite | 39 passed |
| `renvor-error` suite | 21 passed |

The full `cargo xtask verify` sequence on both toolchains is recorded in the pull request.

### The Phase 004 gap this phase closed

`renvor routes` used a blocking `Command::output()`, and its own source recorded the consequence:
*"a project binary that does not recognise the dump flag, ignores it, boots normally, and streams
logs forever."* FR-053 made the fix a requirement.

`crates/renvor-cli/src/commands/relay.rs` spawns with a piped stdout, drains it on a **concurrent
thread** (a parent that waited for exit before reading would deadlock against the very oversized
answer the bound refuses), bounds the wait with a deadline, and **kills the child** when it elapses.

Written **once**, so both `renvor routes` and `renvor openapi` use it — two implementations of "run
the project binary safely" would be two things that can drift.

Asserted by `a_binary_that_never_answers_is_stopped_at_the_deadline` and
`a_binary_that_streams_forever_is_stopped_rather_than_filling_memory`.

---

## 8. Limitations — with an owner and a target phase

| # | Limitation | Owner | Target |
|---|---|---|---|
| L-1 | **No independent human review has occurred.** Constitution §Development and Phase Workflow #7 requires one. ADRs 0013–0015 are `proposed` and **no waiver exists** | Ahmed | before merge |
| L-2 | `pattern` is not enforced. It needs a regex engine on untrusted input, and `regex` is not in the runtime graph. A declaration using it is **refused**, not silently unenforced | framework | 012 |
| L-3 | `allOf`/`anyOf`/`oneOf`/`not` are not enforced. `schemars` emits them for data-carrying enums; those declarations are refused | framework | 012 |
| L-4 | Cookie parameters are not validated. A cookie is an authentication carrier and Phase 009 owns authentication | framework | 009 |
| L-5 | Security schemes are absent from the description, for the same reason | framework | 009 |
| L-6 | `format` is published and **not enforced**. This is JSON Schema 2020-12's own default — the format-annotation vocabulary — and is stated rather than left to inference | — | standard behaviour |
| L-7 | Pagination and filtering define **validated public contracts and ports only**. Nothing queries a database; FR-042 forbids it and it is asserted | framework | 006 |
| L-8 | Idempotency keys, conditional requests, and ETags are named in PLAN.md §11.1 for REST 1.0 and require storage | framework | 006+ |
| L-9 | **`renvor openapi` succeeds against no generated project**, because no Renvor crate is published and no generated project depends on the framework. The relay **is** implemented and **is** asserted end to end against a real binary answering through the real library. Its reach across *generated* projects is what is zero. Carried forward from Phase 004 in the same words, because it is the same limitation | framework | first publication |
| L-10 | The API snapshot mechanism is implemented and no snapshot is committed, because the framework declares no public API of its own — the gate compares an **application's** description | framework | 012 |
| L-11 | **`AGENTS.md` does not exist** in the working tree or anywhere in Git history, though the phase brief named it as required reading. Equivalent authority was taken from `CONSTITUTION.md`, `GOVERNANCE.md`, `PLAN.md`, and `contracts/`. Recorded rather than silently skipped | Ahmed | — |

---

## 9. What this phase deliberately did not do

- **No persistence of any kind.** No repository port implementation, no SQLx, no SeaORM, no
  migrations, no database-backed pagination.
- **No authentication or authorization.**
- **No publication.** Zero crates published, zero tags, zero releases, zero deployments — verified
  against the live registry, not assumed.
- **No waiver created.** W-011 does not exist and was not created.
- **Phase 006 was not started.**

---

## 10. Automated review is not independent review

Any automated review performed during this phase is **advisory**. Constitution §Development and
Phase Workflow #7 requires an independent review comparing implementation evidence with the
specification, constitution, compatibility matrix, and security checklist.

**That review has not occurred, and this record does not claim it has.** The phase remains **open**
until it does, or until a waiver is recorded with an owner, an expiry, and a removal plan — which is
Ahmed's decision and not this record's.
