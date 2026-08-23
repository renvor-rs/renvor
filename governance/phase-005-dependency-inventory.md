# Phase 005 — Dependency Inventory

**Date**: 2026-08-23
**Phase**: 005 — Validation, Problem Details, and OpenAPI
**Authoritative for**: constitution principle III (package-first boundaries), principle XI
(supply-chain integrity), FR-063, FR-064
**Licence policy**: [`deny.toml`](../deny.toml) is the enforced allow-list. Every selection below is
on it; **no exception is requested**.

> **Every row was established by compiling and running the candidate, or by reading the official
> artifact.** Not one is a README claim. Where a rejection rests on a measurement, the measurement
> is given.

---

## 1. Selected — runtime

| Package | Version | Released | Licence | MSRV | Why |
|---|---|---|---|---|---|
| `schemars` | 1.2.2 | 2026-07-27 | MIT | — | Produces JSON Schema **draft 2020-12**, the exact dialect base OAS 3.2 declares. Verified by running `schema_for!` and reading the emitted `$schema`, and by validating a whole 3.2 document containing its output against the official schema |
| `base64` | 0.23.1 | 2026-08-04 | MIT OR Apache-2.0 | 1.71.0 | URL-safe unpadded cursor encoding. Nothing else in the graph provides it |
| `wait-timeout` | 0.2.1 | 2025-02-03 | MIT/Apache-2.0 | — | **Already present** since Phase 003 for the container probe. Phase 005 adds a second use: the bounded project-binary relay. One bounded-wait mechanism in the program rather than two |

**Net new runtime packages, measured against the pre-Phase-005 lockfile: seven** — `schemars`,
`schemars_derive`, `serde_derive_internals`, `dyn-clone`, `ref-cast`, `ref-cast-impl`, `zmij`.

`serde`, `serde_json`, `proc-macro2`, `quote`, `syn`, `unicode-ident`, `itoa`, and `memchr` were
already resolved; the new crates add **edges**, not packages.

## 2. Selected — development only

| Package | Version | Released | Licence | MSRV | Why its weight is acceptable here |
|---|---|---|---|---|---|
| `jsonschema` | 0.50.1 | 2026-08-22 | MIT | 1.85.0 | **103 packages** — which is exactly why it is not a runtime dependency. As a dev-dependency it earns its weight twice: it validates the generated document against the **official** OAS 3.2 schema (an independent validator, so the generator does not judge itself), and it is the reference implementation the bounded interpreter is differentially tested against |
| `proptest` | 1.11.0 | 2026-03-24 | MIT OR Apache-2.0 | 1.85 | Property testing for the cursor decoder. PLAN.md §17.1 names *"property or fuzz tests for parsers, routing edge cases, pagination cursors, and untrusted formats"* — a cursor is all four |

## 3. Rejected — OpenAPI generation

Re-evaluated on **2026-08-23** against primary sources. PLAN.md §8.1 required the snapshot to be
re-run rather than trusted.

| Package | Version | Released | Licence | Emits | Rejection |
|---|---|---|---|---|---|
| `utoipa` | 5.5.0 | 2026-05-04 | MIT OR Apache-2.0 | **`3.1.0`** | **Measured**: compiled a `#[derive(OpenApi)]` type and printed the `openapi` field |
| `utoipa-axum` | 0.2.0 | 2025-01-16 | MIT OR Apache-2.0 | inherits | Same version; also binds to `axum` |
| `aide` | 0.15.1 | 2026-04-14 | MIT OR Apache-2.0 | 3.1 | Wrong version. 0.16.0 is a pre-release |
| `apistos` | 0.6.1 | 2026-06-08 | MIT OR Apache-2.0 | 3.0/3.1 | Wrong version; actix-bound |
| `oasgen` | 0.25.0 | 2025-02-25 | MIT | 3.0/3.1 | Wrong version; stale since 2025-02 |
| `salvo-oapi` | 0.95.2 | 2026-08-06 | MIT OR Apache-2.0 | 3.1 | Wrong version; salvo-bound |
| `poem-openapi` | 5.1.16 | 2025-07-28 | MIT OR Apache-2.0 | 3.0 | Wrong version; poem-bound |
| `okapi` | 0.7.0 | 2024-01-14 | MIT | 3.0 | Wrong version; unmaintained since 2024-01 |
| `openapiv3` | 2.2.0 | 2025-06-02 | MIT | 3.0 model only | Wrong version |

**Verdict: as of 2026-08-23, no maintained Rust crate emits an `openapi: 3.2.0` document.** See
[ADR-0013](../decisions/0013-openapi-3-2-document-serialiser.md).

## 4. Rejected — validation

| Package | Version | Released | Licence | Rejection |
|---|---|---|---|---|
| `validator` | 0.21.0 | 2026-07-27 | MIT | Validates; emits **no** JSON Schema. Pairing it with `schemars` declares every constraint **twice**, which FR-007 forbids |
| `garde` | 0.23.0 | 2026-05-23 | MIT OR Apache-2.0 | Same architectural mismatch |
| `jsonschema` **at runtime** | 0.50.1 | 2026-08-22 | MIT | **Measured: 103 packages against `renvor-http`'s 65.** More than doubles the transport's dependency surface — the whole ICU stack, `fancy-regex`, `num-bigint`, `uuid-simd`, `email_address`, `wasm-bindgen` — to check a request body |

See [ADR-0014](../decisions/0014-schema-as-single-source-and-bounded-interpreter.md).

## 5. Rejected — RFC 9457 Problem Details

| Package | Version | Released | Licence | Rejection |
|---|---|---|---|---|
| `problem_details` | 0.10.0 | 2026-08-16 | MIT OR Apache-2.0 | **Measured by compilation**: its core `status` member is `Option<http::StatusCode>`, and it depends on `http` unconditionally even with default features off. `renvor-error` must work with **no transport present** |
| `problemdetails` | 0.7.0 | 2026-04-23 | MIT | axum-bound by design |
| `http-api-problem` | 0.60.0 | 2025-01-06 | Apache-2.0/MIT | RFC **7807**, not 9457; last release 19 months ago |
| `http-problem` | 0.3.0 | 2023-05-11 | MIT OR Apache-2.0 | Unmaintained since 2023 |
| `rfc9457` | 0.1.0 | 2026-05-06 | MIT | 17 downloads; no evidence of use |

See [ADR-0015](../decisions/0015-public-api-error-registry.md).

## 6. Rejected — JSON Schema validation

| Package | Version | Released | Licence | Rejection |
|---|---|---|---|---|
| `boon` | 0.6.1 | 2025-01-07 | MIT OR Apache-2.0 | Last release **19 months** before this phase. `deny.toml` sets `unmaintained = "workspace"`, so adopting it would fail the licence-and-advisory gate |
| `valico` | 4.0.0 | 2023-05-13 | MIT | Unmaintained since 2023 |

## 7. Advisories

`cargo deny check` runs as verification step 6 and covers advisories, licences, bans, and sources.
`deny.toml` carries **no** ignored advisory (`ignore = []`), so a new advisory against any selection
here fails the gate rather than being waived. Response windows are in
[`dependency-advisory-policy.md`](dependency-advisory-policy.md).

## 8. Unsafe Rust

The workspace declares `unsafe_code = "forbid"` at the workspace level, inherited by all three new
crates. **No `unsafe` block exists in any Phase 005 crate**, and the lint makes adding one a
compile error rather than a review question.

## 9. Vendored artifacts

| File | Source | Retrieved | Modified? |
|---|---|---|---|
| `crates/renvor-openapi/tests/schemas/oas-3.2-schema-2025-09-17.json` | <https://spec.openapis.org/oas/3.2/schema/2025-09-17> | 2026-08-23 | **No** |
| `crates/renvor-openapi/tests/schemas/oas-3.1-schema-2022-10-07.json` | <https://spec.openapis.org/oas/3.1/schema/2022-10-07> | 2026-08-23 | **No** |

Vendored so verification runs **offline and deterministically** — a gate that needs the network
fails when the network does, and
[`verification-sequence.md`](../contracts/verification-sequence.md) requires a check that cannot run
to be a failure rather than a skip.

They are **upstream artifacts and are not edited**. A local edit would make the gate judge Renvor's
opinion of the standard rather than the standard. They are test fixtures and are **not** in any
crate's `include` list, so they ship to nobody.

## 10. Publishable set

**Eight** packages, up from five. The count is asserted against the actual manifests by `xtask`
step 7, so a package added without appearing in
[`RELEASING.md`](../RELEASING.md)'s ordering fails verification rather than being discovered at
publication time.

| Added | Position | Depends on |
|---|---|---|
| `renvor-error` | **1**, beside the kernel | *(no Renvor crate at all)* |
| `renvor-validation` | 2 | `renvor-error` |
| `renvor-openapi` | 3 | `renvor-validation`, `renvor-error` |

`renvor-http` moved from position 2 to **4**, because it now adapts all three.

`renvor-error` sharing position 1 with the kernel is worth stating: it is what
"transport-independent" looks like when it is true rather than intended — nothing in the registry or
the RFC 9457 model names a Renvor type outside its own crate.

## 11. Publication status

**Zero.** Queried on 2026-08-23: every `renvor*` name returns HTTP 404 from the crates.io API. No
crate is published, no tag exists, and no release exists.
