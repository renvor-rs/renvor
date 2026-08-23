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

**Net new runtime packages, measured against the pre-Phase-005 lockfile: seven** — `base64`,
`dyn-clone`, `ref-cast`, `ref-cast-impl`, `schemars`, `schemars_derive`, `serde_derive_internals`.

`serde`, `serde_json`, `proc-macro2`, `quote`, `syn`, `unicode-ident`, `itoa`, and `memchr` were
already resolved; the new crates add **edges**, not packages.

> **Corrected 2026-08-23.** This enumeration previously ended `..., ref-cast-impl, zmij` and
> omitted `base64`. **`zmij` is not new.** It is a pre-existing transitive of `serde_json`, present
> in the base lockfile at `731d28dc` with the identical checksum
> `29666d0abbfad1e3dc4dcf6144730dd3a3ab225bbbdac83319345b1b44ccfc1b`; `base64` is absent from that
> lockfile entirely. **The count of seven was and is correct — one name in it was wrong**, and the
> document contradicted its own §1 table two paragraphs above, which lists `base64` as a selected
> runtime package. Measured by differencing `git show 731d28dc:Cargo.lock` against the runtime
> closure (`cargo tree -e normal,build`) of every publishable crate. Found by maintainer
> self-review during the Phase 005 closing audit.

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
| `aide` | 0.15.1 | 2026-04-14 | MIT OR Apache-2.0 | 3.1 | Wrong version; 0.16.0 is a pre-release; **axum-bound**, `axum`/`axum-extra` non-optional |
| `apistos` | 0.6.1 | 2026-06-08 | MIT OR Apache-2.0 | 3.0/3.1 | Wrong version; actix-bound |
| `oasgen` | 0.25.0 | 2025-02-25 | MIT | 3.0/3.1 | Wrong version; stale since 2025-02 |
| `salvo-oapi` | 0.95.2 | 2026-08-06 | **Apache-2.0 only** | **`3.2.0` opt-in; `$self` only** | Salvo-bound (unusable from axum); single-licensed; 3.2 version string over a 3.1 object model |
| `poem-openapi` | 5.1.16 | 2025-07-28 | MIT OR Apache-2.0 | 3.0 | Wrong version; poem-bound |
| `okapi` | 0.7.0 | 2024-01-14 | MIT | 3.0 | Wrong version; unmaintained since 2024-01 |
| `openapiv3` | 2.2.0 | 2025-06-02 | MIT | 3.0 model only | Wrong version |

**No maintained Rust crate emits a document with genuine OpenAPI 3.2 semantics.** One published
crate emits the version string: **`salvo-oapi` 0.95.2** offers `Version3_2` as an opt-in and writes
`"3.2.0"`. It implements **`$self` and nothing else** from the 3.2 object model — no
`additionalOperations`, no `querystring`, no `itemSchema`, no `deviceAuthorization`, no
`mediaTypes`, and a `Tag` carrying only `name`/`description`. It is also hard-bound to the Salvo
framework (`salvo_core`, `impl Handler for OpenApi`), so it is unusable from axum, and it is
**Apache-2.0 only** while the rest of this workspace is dual-licensed.

That crate is the clearest possible illustration of why this phase's gate has a **negative half**:
it produces a document that is *schema-valid against OAS 3.2* while carrying no 3.2 semantics —
exactly the relabelling constitution principle V forbids, and exactly what proof 3 detects.

Complete 3.2 object models are **merged but unreleased** in both `utoipa` master
([PR #1555](https://github.com/juhaku/utoipa/pull/1555), merged 2026-08-09) and `salvo-oapi` main
([PR #1688](https://github.com/salvo-rs/salvo/pull/1688), merged 2026-08-06 — twenty hours *after*
0.95.2 was cut, which is the whole explanation for that release's half-support). utoipa master
still carries `version = "5.5.0"` with no release announced, and `Version31` remains `#[default]`
there — despite that PR's own description claiming it changed the default, which did not survive
review.

See [ADR-0013](../decisions/0013-openapi-3-2-document-serialiser.md).

> **Corrected 2026-08-23.** This section previously read *"no maintained Rust crate emits an
> `openapi: 3.2.0` document"*, without qualification. That was false. The corrected statement is
> narrower and the conclusion is unchanged.

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
| `crates/renvor-openapi/tests/schemas/oas-3.2-schema-2025-11-23.json` | <https://spec.openapis.org/oas/3.2/schema/2025-11-23> | 2026-08-23 | **No** |
| `crates/renvor-openapi/tests/schemas/oas-3.1-schema-2025-11-23.json` | <https://spec.openapis.org/oas/3.1/schema/2025-11-23> | 2026-08-23 | **No** |

**Re-pinned from `2025-09-17` / `2022-10-07` on 2026-08-23**, after package research established
that both were superseded. The 3.2 change is `$defs.styles-for-form` becoming
`$defs.explode-for-form`, which drops `required: ["style"]` from its `if` so the `explode: true`
default also applies when `style` is omitted — an **annotation-default fix, not a pass/fail
change**. Every proof in the gate returns the same verdict under both; pinning the older one would
have frozen the gate on a schema with a known-fixed defaulting bug.

Two traps worth recording: **there is no `/latest` alias** (`.../schema/latest` returns 404, so
every reference must name a date), and **the dialect and meta artifacts carry a different date**
than the schema — they remain `2025-09-17`. `OPENAPI_DIALECT` therefore does **not** match the
schema's date, and correcting it to match would emit a URI that does not exist.

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
