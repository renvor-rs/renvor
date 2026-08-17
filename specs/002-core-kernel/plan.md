# Implementation Plan: Transport-Independent Core Kernel

**Branch**: `feat/phase-002-core-kernel` | **Date**: 2026-08-16 *(revision 2)* | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/002-core-kernel/spec.md`

**Note**: This template is filled in by the `/speckit-plan` command; its definition describes the execution workflow.

> **Branch naming**: `.specify/scripts/bash/setup-plan.sh` reports `BRANCH: 002-core-kernel`,
> derived from the feature directory. The actual git branch is **`feat/phase-002-core-kernel`**.
> The script's value is a directory-derived label, not the checked-out branch.

> **Revision 2** corrects nine items in revision 1. The substantive ones: the provider resolver was
> **infeasible against its own budget** and is redesigned; `petgraph` was **wrongly rejected** and
> is now adopted; `secrecy` does **not** supply `Display` redaction; `confique` carries four
> limits revision 1 did not record; and the platform claim was **unevidenced**. Full disposition
> in [research.md](./research.md) §0.

## Summary

Phase 002 turns Renvor from a governed but inert repository into something that runs. It delivers
the application builder, typed state, the provider registry, the deterministic lifecycle
`Load → Validate → Register → Boot → Ready → Drain → Stop` with reverse-order rollback, a
cancellation and deadline model, typed layered configuration, a redaction-safe error taxonomy,
independent health and readiness primitives, tracing bootstrap, and a test harness — with **no
transport, no persistence, and no publication** (FR-033, FR-034).

The approach is package-first. Eleven maintained crates carry the load
([research.md](./research.md) §3). **Dependency resolution is a single `petgraph::algo::tarjan_scc`
pass over a Renvor-owned instrumented adapter**: one call returns every node of each cycle
(FR-013) *and* the components in reverse topological order (FR-012), at 2 provider examinations
per provider and 1 edge examination per edge — **2048 / 8192 / 10240** observed at the ceilings
against allowances of **2048 / 16384 / 18432**. Exactly **one** primitive is
written in-house — the typed-state map, because an any-map has nowhere to put the type-name
metadata FR-011 and FR-037(b) require — and it is gated on **ADR-0007**, which needs a **human**
review that neither existing waiver supplies.

Configuration is the phase's open question. `confique` is **on probation**: its layer model is the
one FR-044 mandates, but it supplies **no source attribution**, treats an unparseable **empty**
environment value as **unset**, and does not document array replacement. An **eight-obligation
proof gate** decides it, with the fallback and its ADR requirement written down in advance.

## Technical Context

**Language/Version**: Rust, edition 2024, MSRV **1.94.0** (ADR-0003), resolver 3, `unsafe_code = "forbid"`

**Primary Dependencies**: `thiserror` 2.0.20, `serde` 1.0.229, `toml` 1.1.4, `confique` 0.4.0
*(probation)*, **`petgraph` 0.8.3**, `tokio` 1.53.1, `tokio-util` 0.7.19, `tracing` 0.1.44,
`tracing-subscriber` 0.3.23, `getrandom` 0.4.3, `secrecy` 0.10.3 (+ `zeroize` 1.9.0).
**This is a *direct-dependency candidate* list, not the resolved inventory** — see
[research.md](./research.md) §3 and the inventory task.

**Storage**: N/A — no persistence in this phase (FR-033). Configuration files are read, never written.

**Testing**: `cargo test` (unit, integration, doc); `tokio` `test-util` for time control (FR-031);
failure injection at all seven phases via `renvor-testkit` (FR-030); property/fuzz testing of TOML
input for FR-038, per principle IX.

**Target Platform**:

| Platform | Status | Evidence |
|---|---|---|
| **Linux** (`ubuntu-latest`) | **supported and tested** | every workflow in `.github/workflows/` runs `ubuntu-latest`; the CI matrix varies **toolchain only** (`1.94.0`, `stable`) |
| **macOS** | **unclaimed** | no CI runner; the maintainer's local machine is macOS, which is incidental exercise, not evidence |
| **Windows** | **unclaimed** | no CI runner, no local exercise |

Portability is an **implementation objective** — nothing in this phase deliberately depends on
Linux-specific behaviour — but an objective is not a support promise. Principle XII prohibits
compatibility claims without matrix evidence, and there is no matrix. *(Revision 1 said "any
Tokio-supported platform", which asserted a matrix that does not exist.)*

**Project Type**: Rust library workspace — a facade crate plus implementation crates behind it (ADR-0002).

**Performance Goals**: **none declared, deliberately.** No transport exists to measure against, and
principle XII prohibits performance claims without measurement. The only performance-shaped
requirement is FR-039's work budget, which is a **correctness** bound counted in examinations.

**Constraints**: ≤ **1024** providers, ≤ **8192** declared edges (fail at Register); resolution
≤ **2048** provider examinations, ≤ **16384** edge examinations, ≤ **18432** work units; drain
budget default **30 s**, overridable, **0** meaning stop immediately while still reporting
outstanding work; **0** unbounded waits; **0** secret or registered-state contents in any output.

**Scale/Scope**: 44 functional requirements, 22 success criteria, 26 acceptance scenarios, 13 edge
cases, 6 user stories. Three new implementation crates; the facade gains its first real re-exports.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

Constitution v1.0.0, ratified 2026-08-11.

| Principle | Applies | Pre-Phase-0 | Post-Phase-1 | Evidence |
|---|---|---|---|---|
| I Cohesive, explicit Rust | yes | PASS | PASS | `unsafe_code = "forbid"`; FR-032 forbids hidden global mutable state; **D4 — the library never installs a process-global subscriber**, which is the specific opaque-runtime-behaviour risk this principle names |
| II Transport-independent core | yes | PASS | PASS | FR-033 excludes every transport; D9 rejects `http::Extensions` to keep an HTTP crate out of the core; the crate DAG points **inward** |
| III Package-first boundaries | yes | PASS | PASS **with 1 ADR obligation** | 12 evaluations recorded with version, licence, MSRV, advisories, feature cost. **petgraph is adopted**, removing revision 1's second custom primitive. Only the typed-state map is custom → **ADR-0007**, gated by D11 |
| IV Deterministic lifecycle | yes | PASS | PASS | FR-001…FR-009, FR-042; reverse **actual initialisation** order; FR-039(c) forbids reporting a cycle by budget exhaustion. **D6 records confique's empty-env `Ok(None)` as a silent fallback** and makes rejecting it a proof obligation |
| V Contract-first compatibility | partial | PASS | PASS | [contracts/](./contracts/) written before implementation. OpenAPI, RFC 9457, GraphQL, CLI clauses **not applicable** |
| VI Security, privacy, fail-closed | yes | PASS | PASS | FR-037…FR-041; **D2 — Renvor owns every output path `secrecy` does not cover, including `Display`, which `secrecy` does not implement at all**; FR-038 fails closed; run identifier uses OS randomness |
| VII Deterministic generation | no | N/A | N/A | `renover` is Phase 003 — *renamed `renvor` 2026-08-17, ADR-0010* |
| VIII Feature and platform isolation | yes | PASS | PASS | `confique` `toml` only; `petgraph` minimal features; `tokio` without `net`/`fs`/`process`; `test-util` dev-only; **the crate DAG below is the proof, and its limit is stated** |
| IX Real-boundary verification | yes | PASS | PASS | Failure injection at **7 of 7** phases; property/fuzz on TOML; **a 1024-node linear chain test for the recursive `tarjan_scc`**, so the stack claim is evidence |
| X Documentation is a release artifact | yes | PASS | PASS | `missing_docs = "warn"`; examples compile and run; FR-036 requires the instability statement in the surface's own published documentation |
| XI Supply-chain and release integrity | yes | PASS | **PASS, pending inventory** | `Cargo.lock` tracked; `deny.toml` unchanged and now load-bearing; **the complete transitive inventory is unmeasured until manifests exist** and is a blocking task; **no publication** (FR-034) |
| XII Simplicity, phasing, honest scope | yes | PASS | PASS | Three crates justified below; FR-036 declares the surface unstable; **0** performance claims; **the platform claim was corrected from an unevidenced one** |
| XIII Independent installable packages | no | N/A | N/A | Package ecosystem follows product 3.0 |

**Security and Privacy Requirements**: the specification carries abuse cases, sensitive-data
classification, authorization impact (**explicitly none**, FR-041), resource bounds, and failure
behaviour — **5 of 5** present. Logs and telemetry use structured fields, a correlation identifier,
and centrally tested redaction (FR-043).

### Governance gate — ADR-0007 cannot be self-accepted

**Neither active waiver authorises accepting a Phase 002 decision record.**

| Waiver | Covers | Does **not** cover |
|---|---|---|
| **W-002** | independent review of **Phase 001 decision records** (spec FR-013) | any **Phase 002** ADR |
| **W-003** | **Phase 001's phase-level** independent requirements-and-security review (Phase 001 `001-T088`) | any decision record, in any phase |

W-003's grant states it "waives only the independent-human-review requirement for Phase 001" and
"does not waive any finding, failed check, missing evidence, acceptance criterion, or security
blocker". **ADR-0007 therefore requires a qualified independent review, or a separately proposed
and separately approved waiver carrying all seven mandatory fields**, before any custom
infrastructure merges. Every review produced inside this phase is **advisory and non-independent**
and must never be described otherwise.

**Result: no unjustified violations. Complexity Tracking is empty by design.**

## Project Structure

### Documentation (this feature)

```text
specs/002-core-kernel/
├── plan.md              # This file (/speckit-plan command output)
├── research.md          # Phase 0 output (/speckit-plan command)
├── data-model.md        # Phase 1 output (/speckit-plan command)
├── quickstart.md        # Phase 1 output (/speckit-plan command)
├── contracts/           # Phase 1 output (/speckit-plan command)
│   ├── lifecycle-contract.md
│   ├── configuration-contract.md
│   ├── error-taxonomy.md
│   ├── provider-graph-contract.md
│   └── observability-contract.md
├── checklists/
│   └── requirements.md  # Specification-quality checklist (16/16)
└── tasks.md             # Phase 2 output (/speckit-tasks command - NOT created by /speckit-plan)
```

### Source Code (repository root)

```text
crates/
├── renvor/                     # Facade. Publishable. Re-exports only; implements nothing (ADR-0002).
├── renvor-core/                # Lifecycle, providers, typed state, errors, cancellation,
│   └── src/                    # health/readiness, observability, run identifier
│       ├── lifecycle/          # phase machine, rollback, drain (FR-001…FR-009, FR-042)
│       ├── provider/           # registry + instrumented petgraph adapter (FR-012…FR-014, FR-039)
│       ├── state/              # TypeId map with type-name retention (FR-010, FR-011, FR-037b)
│       ├── error/              # categorised, chainable, redaction-safe (FR-019…FR-022)
│       ├── cancel/             # CancellationToken scopes and deadlines (FR-023…FR-025)
│       ├── health/             # independent health and readiness (FR-026…FR-028)
│       ├── observe/            # spans, structured fields, run identifier (FR-029, FR-043)
│       └── config_port/        # ConfigResolver / ResolvedConfig traits — NO parser, NO serde
├── renvor-config/              # Typed layered configuration (FR-015…FR-018, FR-044)
│   └── src/
│       ├── layer/              # per-source decoding, ordered merge, source attribution
│       └── secret/             # Renvor Secret<T> — owns Display, error, tracing, serde paths
└── renvor-testkit/             # Failure injection and time control (FR-030, FR-031)

examples/                       # Ordinary Rust, no global mutable state (FR-032, SC-014)
xtask/                          # Existing verification runner. Unchanged in scope.
```

### Crate dependency DAG

```text
                       ┌────────────────────────┐
                       │  renvor  (facade)      │  re-export only, no impl code
                       │  feature "config" (on) │
                       └───────┬────────┬───────┘
                               │        │ (feature-gated)
                               ▼        ▼
                   ┌────────────────┐  ┌──────────────────┐
                   │  renvor-core   │◀─│  renvor-config   │
                   └────────────────┘  └──────────────────┘
                            ▲                   deps: serde, toml,
                            │                   confique*, secrecy, zeroize
                   ┌────────────────┐
                   │ renvor-testkit │   users add to [dev-dependencies] only
                   └────────────────┘

  renvor-core  deps: thiserror, petgraph, tokio, tokio-util, tracing,
                     tracing-subscriber, getrandom
  Arrows point from dependent to dependency.  * confique on probation (research D6).
```

**Every edge points inward, toward `renvor-core`. There is no cycle.**

The five properties this DAG must prove, each with its limit stated:

1. **Separating `renvor-config` genuinely keeps parsers, derive macros, and secret types out of
   core-only consumers.** `renvor-core` declares **none** of `serde`, `toml`, `confique`,
   `secrecy`, or `zeroize`. A consumer depending on `renvor-core` directly, or on `renvor` with
   `default-features = false`, resolves **0** of them.
   **Limit, stated rather than glossed**: a consumer taking `renvor` with default features gets
   `renvor-config` and therefore its dependencies. The separation buys isolation for the
   core-only path, **not** for the default path. There is no third reading: `config` is a
   **default-on** feature, so *isolation is opt-in via `default-features = false`* and the
   out-of-the-box dependency footprint is the wider one.

   **This is an executable claim, not a stated one.** `crates/renvor/Cargo.toml` declares the
   feature at **T006**, and **T102** asserts both directions — `--no-default-features` resolves
   **0** of `confique`, `toml`, `serde`, `secrecy`, with a **positive control** confirming the
   same query *with* default features **does** resolve them. Without that control the assertion
   could pass because the query was wrong rather than because the isolation holds.

2. **`renvor-core` does not depend outward on `renvor-config`.** The dependency runs
   `renvor-config → renvor-core`, never the reverse. Core defines the **port**; config implements
   it. This is the inward direction principle II mandates.

3. **The facade remains re-export-only.** `crates/renvor/src/lib.rs` contains `pub use` statements
   and documentation, and **0** function bodies, `impl` blocks, or type definitions of its own —
   as ADR-0002 requires ("the facade re-exports; it does not implement"). It re-exports a
   **deliberately narrow** subset, not everything public behind it (FR-036).

4. **`renvor-testkit` never enters a production graph.** Nothing in `renvor`, `renvor-core`, or
   `renvor-config` depends on it. Authors add it under `[dev-dependencies]`, which is the only way
   FR-030's harness can be available to *users' tests* without being available to their binaries.

5. **Configuration participates in Load/Validate with no cycle and no facade implementation.**
   `renvor-core` defines two dependency-free port types:

   ```text
   trait ConfigResolver: Send + Sync {
       fn resolve(&self) -> Result<ResolvedConfig, KernelError>;
   }
   struct ResolvedConfig {
       value:       Box<dyn Any + Send + Sync>,   // opaque to core
       type_name:   &'static str,                 // FR-037b: name only, never contents
       attribution: BTreeMap<KeyPath, SourceLayer>, // FR-016
   }
   ```

   The builder accepts a `Box<dyn ConfigResolver>`. During **Load** the kernel calls `resolve()`;
   during **Validate** it surfaces any error; it then registers `value` into the typed-state map.
   Core needs `Any`, `BTreeMap`, and its own error type — **no `serde`, no `toml`, no parser**.
   `renvor-config` supplies the implementing type. The facade re-exports both and implements
   neither.

**Structure Decision**: three implementation crates behind the unchanged facade, exactly as
ADR-0002 anticipates ("later phases add implementation crates behind it"). Three rather than one,
because principle XII requires justifying each boundary:

- **`renvor-config` is separate** because it is the only crate needing a TOML parser, a derive
  macro, and a secret type — property 1 above is what that separation buys.
- **`renvor-testkit` is separate** because FR-030 makes the harness a capability *users* need in
  *their* tests, so it cannot be a `#[cfg(test)]` module; and it must not reach production graphs,
  so it cannot live in the core.
- **Not more than three.** Errors, cancellation, health, and observability stay inside
  `renvor-core`: each is small, all four are used by the lifecycle, and splitting them would
  create boundaries with no independent consumer.

**No crate is published** (FR-034). `renvor-core`, `renvor-config`, and `renvor-testkit` are
created with `publish = false`, so FR-034 holds by configuration rather than by remembering.

## Complexity Tracking

> **Fill ONLY if Constitution Check has violations that must be justified**

**Empty — the Constitution Check records no unjustified violation.**

Revision 1 listed **two** in-house primitives. Adopting `petgraph` removes one. The remaining one —
the typed-state map (D9) — is not a constitution violation: principle III permits custom
infrastructure with an **accepted** ADR recording evaluated packages, their shortcomings,
ownership cost, and an exit strategy. That record is **ADR-0007**, and per **D11** its acceptance
is a **human governance gate** that blocks the merge, not a formality this phase can discharge.
