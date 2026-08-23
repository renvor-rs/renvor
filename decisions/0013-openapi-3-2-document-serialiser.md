# ADR-0013: Serialise the OpenAPI 3.2.0 document in Renvor, because no maintained package emits it

| Field | Value |
|---|---|
| **ID** | 0013 |
| **State** | `proposed` |
| **Reviewer** | *(none — required to enter `accepted`)* |
| **Review date** | *(none — required to enter `accepted`)* |
| **Superseded by** | *(not superseded)* |

> **This record is `proposed` and nothing else.**
>
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded **independent**
> review before acceptance. No independent human review of this record has occurred, and none is
> claimed. **No Phase 005 waiver exists**, and none is created here — that is Ahmed's decision, not
> this record's. Automated review is advisory and does not satisfy the requirement.

## Context

Constitution principle V is binding and specific:

> *"REST MUST use the current approved OpenAPI standard and RFC 9457 Problem Details. The initial
> target is OpenAPI 3.2.0, and emitted documents MUST NOT claim a version that selected tooling does
> not correctly implement."*

PLAN.md §11.1 repeats it and adds the consequence: *"Phase 005 remains blocked until selected tooling
can emit and validate the promised version correctly."*

OpenAPI Specification **3.2.0** was released on **2025-09-19**. PLAN.md §8.1's package snapshot named
`utoipa`, `aide`, and `schemars` as candidates and required the snapshot to be re-run rather than
trusted.

## The research, re-run on 2026-08-23

Every row below was established by **compiling and running** the candidate, or by reading the
official artifact. No row is a README claim.

| Candidate | Version | Date | License | Emits | Verdict |
|---|---|---|---|---|---|
| `utoipa` | 5.5.0 | 2026-05-04 | MIT OR Apache-2.0 | **`3.1.0`** | **Rejected** — measured by compiling a `#[derive(OpenApi)]` type and printing the field |
| `utoipa-axum` | 0.2.0 | 2025-01-16 | MIT OR Apache-2.0 | inherits `utoipa` | **Rejected** — same version, and binds to `axum` |
| `aide` | 0.15.1 | 2026-04-14 | MIT OR Apache-2.0 | 3.1 | **Rejected** — wrong version; 0.16.0 is a pre-release |
| `apistos` | 0.6.1 | 2026-06-08 | MIT OR Apache-2.0 | 3.0/3.1 | **Rejected** — wrong version; actix-bound |
| `oasgen` | 0.25.0 | 2025-02-25 | MIT | 3.0/3.1 | **Rejected** — wrong version; stale since 2025-02 |
| `salvo-oapi` | 0.95.2 | 2026-08-06 | MIT OR Apache-2.0 | 3.1 | **Rejected** — wrong version; salvo-bound |
| `poem-openapi` | 5.1.16 | 2025-07-28 | MIT OR Apache-2.0 | 3.0 | **Rejected** — wrong version; poem-bound |
| `okapi` | 0.7.0 | 2024-01-14 | MIT | 3.0 | **Rejected** — wrong version; unmaintained since 2024-01 |
| `openapiv3` | 2.2.0 | 2025-06-02 | MIT | a 3.0 data model only | **Rejected** — wrong version |

**No maintained Rust crate emits an `openapi: 3.2.0` document.**

Relabelling 3.1 output as 3.2 is exactly what principle V forbids, and it would not be a private
shortcut: the version string is what tells every consumer's tooling how to read the document.

## Decision

**Renvor owns a bounded OpenAPI 3.2.0 document model and serialiser**, in `renvor-openapi`.

The responsibility is bounded, and the bound is the decision:

| | |
|---|---|
| **It does** | emit the document envelope and its operations, deterministically |
| **It does not** | implement JSON Schema — **`schemars` 1.2.2** does, and it emits draft 2020-12, the exact dialect base OAS 3.2 declares |
| | parse OpenAPI documents written by anyone else |
| | resolve remote references |
| | **judge validity** — the **official published schema** does that, checked by an independent validator |

The `openapi` member is a **constant**. There is no field, argument, or configuration that sets it,
so relabelling is not something the model can express.

## Why this is not "a custom OpenAPI implementation"

The prohibition this record answers to is principle III's: Renvor must not build custom
infrastructure *"merely to own the implementation."*

The largest part of an OpenAPI generator is **schema production**, and Renvor writes none of it —
`schemars` does, and its output was verified to embed as a **valid OAS 3.2 Schema Object** by
validating a whole document containing one against the official schema. What remains is an envelope
and its operations: a data model with a `Serialize` implementation.

It is also not built to own anything. It is built because the alternative is emitting `3.1.0` or
lying about it.

## Consequences

### The gate is fail-closed and does not trust this crate

Five proofs run against the **vendored official schemas**, offline:

1. the document declares `3.2.0`;
2. it validates against the official **3.2** schema, via `jsonschema` 0.50.1 — a validator
   independent of the generator;
3. it is rejected by the official **3.1** schema **with 3.1's version pattern neutralised**, so the
   rejection is **structural** rather than a version-string mismatch;
4. **the control** — a genuinely relabelled 3.1 document *passes* that neutralised check, proving
   the discriminator discriminates rather than rejecting everything;
5. **the controls** — four malformed documents are rejected, proving the validator is not vacuous.

Proof 3 works because both official schemas use `unevaluatedProperties: false`, so a document
carrying a 3.2-only member is structurally invalid against 3.1. Generated documents therefore always
carry 3.2-only constructs — `$self`, Response `summary`, Tag `summary`/`kind`, and Response objects
with no `description`, which 3.1 requires and 3.2 does not.

### The ownership cost, stated

`renvor-openapi`'s document model is roughly 450 lines including documentation. It must be extended
whenever Renvor describes a construct it does not yet emit — security schemes in Phase 009,
callbacks and webhooks later — and each extension must keep passing the five proofs.

That cost is real and it is **smaller than the alternative**, which is either a false version string
or an indefinitely blocked phase.

### The exit strategy, executable rather than aspirational

**Deletion trigger.** When a maintained Rust crate emits a document whose `openapi` field is `3.2.x`
**and** which passes proofs 1–5 above, Renvor's serialiser is replaced by it and this record is
superseded.

The trigger is executable because **the proof harness that would judge a replacement is the one
already in `crates/renvor-openapi/tests/openapi_3_2_gate.rs`**. Evaluating a candidate costs a
dependency swap and a test run, not a fresh investigation.

The most likely candidate is `utoipa`, which is actively maintained and has the largest share of the
ecosystem. This record does not predict when.

### What is NOT decided here

- Which JSON Schema keywords Renvor **enforces** — ADR-0014.
- The public API error vocabulary — ADR-0015.
- Anything about security schemes, callbacks, or webhooks, none of which this phase emits.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| Emit 3.1 with `utoipa` and set the version string to `3.2.0` | Precisely what principle V forbids. It would also be undetectable to a reviewer reading the manifest, and detectable to every consumer's tooling |
| Emit 3.1 honestly and defer 3.2 | PLAN.md §11.1 and §8 name 3.2.0 as the target. Deferring means REST 1.0 ships describing itself with a superseded version |
| Wait for upstream 3.2 support | No candidate has an announced 3.2 milestone. Waiting blocks Phase 005 and every phase that depends on it, for an unknown duration, on someone else's roadmap |
| Fork `utoipa` and add 3.2 | A fork is ownership with none of the boundedness: the whole schema generator comes with it, and the exit strategy becomes "merge upstream", which is harder than replacing a serialiser |
| Generate the document from a template | A template cannot check operation-identifier uniqueness, validate examples, or be compared semantically. It moves the problem into a place with no type system |
