---
description: "Phase 002 task list — transport-independent core kernel"
---

# Tasks: Transport-Independent Core Kernel

**Input**: Design documents from `/specs/002-core-kernel/`

**Prerequisites**: [plan.md](./plan.md) (revision 2), [spec.md](./spec.md), [research.md](./research.md) (revision 2), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md) (revision 2), [checklists/readiness.md](./checklists/readiness.md)

**Tests**: **Included and mandatory.** Tests are optional by default in this template, but this feature *explicitly requires* them: FR-030 and FR-031 make the harness a deliverable, 22 success criteria are stated as assertions, and constitution principle IX requires real-boundary verification with the phase blocked while acceptance evidence is missing.

**Organization**: Tasks are grouped by user story so each can be implemented and tested independently.

> **Revision 2 — post-`/speckit-analyze` remediation.** The first generation carried 100 tasks and
> 8 findings that blocked implementation. This revision carries **110** and closes them. The count
> rose because closing a real dependency required new work; no task was deleted to preserve a round
> number. Changes: a facade manifest task (**T006**) without which T056 could not compile; a facade
> feature-isolation task (**T102**) so the plan's crate-DAG claim is executable rather than merely
> asserted; an FR-022 silent-fallback task (**T045**); an ADR-0003 lockfile reconciliation task
> (**T034**); an SC-013 current-stable task (**T106**); an SC-022 agreement task (**T104**); an
> FR-041/SC-010 evidence task (**T108**); manifests enumerated in **T005**; and **T065** reusing
> **T015**'s fixtures instead of duplicating them.
>
> **Revision 3 — publishable-set correction.** Three further tasks (**T007**, **T008**, **T009**) and a
> reversal of T001–T003's publish status. Phase 002 gives the publishable facade its first
> dependencies, which breaks the Phase 001 release rehearsal in a way no artifact had noticed:
> `cargo publish -p renvor --dry-run` cannot succeed once the facade depends on a crate that is
> not yet on the registry. The fix is a workspace-wide rehearsal and a publishable dependency
> chain. This was found by running the command, not by reading about it — see research §D13.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (US1–US6)
- Include exact file paths in descriptions

## Path Conventions

Rust library workspace (ADR-0002). Crates live at `crates/<name>/`, examples at `examples/`,
governance records at `governance/`, decision records at `decisions/`.

---

## ⚠️ Four gates run before any production implementation

Per the plan and research revision 2, these **block all of Phase 3 onward**:

| Gate | Tasks | Blocks | Why | Outcome |
|---|---|---|---|---|
| Configuration compatibility proof (8 obligations) | T014–T020 | all `renvor-config` implementation | obligations 4 and 6 carry known negative evidence; failing them triggers the recorded fallback | **FAILED, 4 of 8 met** (1, 4, 6, 7 unmet) → `serde` + `toml` partial-layer fallback **triggered**, and therefore **in ADR-0007 scope**. Research §D6 |
| Provider-resolver feasibility and counters | T021–T025 | all resolver implementation | revision 1's design was infeasible against its own budget; this proves revision 2's is not | **PASSED, 8 of 8 met.** Counters exactly 2048 / 8192 / 10240; 1024-chain resolves with ≈ 4× stack headroom in debug. Iterative-SCC fallback **not** triggered, so **nothing added to ADR-0007 scope**. Research §D8 |
| **ADR-0007 governance gate** | T026–T029 | merging **any** custom infrastructure | **neither W-002 nor W-003 authorises accepting a Phase 002 ADR**; W-004 does, under compensating controls | pending |
| Complete resolved transitive dependency inventory | T030–T034 | adoption confirmation | the research table covers **direct candidates only** | **PASSED.** 55 external packages; 0 without a licence, 0 over MSRV 1.94.0, `cargo deny` clean on all four checks. 43 of 55 arrived transitively. `governance/phase-002-dependency-inventory.md` |

The two proof gates disagreed, and that is the outcome working rather than an inconsistency: the
configuration gate failed on evidence the research had already predicted, while the resolver gate
passed. Only the failing one contributes custom infrastructure to ADR-0007.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the crate skeletons and manifests the gates need in order to run at all.

> **The three new crates are `publish = true`, not `publish = false`.** This was corrected after
> an empirical check, and the reason is not stylistic. The facade `renvor` is publishable
> (ADR-0002) and Phase 002 gives it dependencies. `cargo publish -p renvor --dry-run` then
> **cannot** succeed — verified by direct experiment — because the dependency is not on the
> registry; and it fails *regardless* of whether the dependency is marked publishable.
> `cargo publish --dry-run --workspace` **does** succeed, because cargo stages a temporary
> registry for the workspace members — but **only when every member in the chain is
> publishable**. A `publish = false` dependency of a publishable crate is unshippable by
> construction. See research §D13.

- [x] T001 Create `crates/renvor-core/Cargo.toml` with `publish = true` and the **complete** publishable metadata set required by `specs/001-governance-foundation/contracts/package-metadata.md` — `description`, `documentation`, `readme`, `keywords`, `categories`, and an explicit `include` — inheriting `version`, `edition`, `rust-version`, `license`, `repository`, `homepage`, `authors` via `.workspace = true` and never restating them (ADR-0002)
- [x] T002 [P] Create `crates/renvor-config/Cargo.toml` with `publish = true` and the same complete publishable metadata set
- [x] T003 [P] Create `crates/renvor-testkit/Cargo.toml` with `publish = true` and the same complete publishable metadata set — it must be publishable because plan DAG property 4 has authors add it under `[dev-dependencies]`, which a `publish = false` crate cannot satisfy
- [x] T004 Add `crates/renvor-core`, `crates/renvor-config`, `crates/renvor-testkit` to `members` in the root `Cargo.toml`
- [x] T005 Declare direct dependencies with explicit feature sets exactly as recorded in `specs/002-core-kernel/research.md` §3 feature-cost table, naming the manifests individually — `crates/renvor-core/Cargo.toml` (`tokio` without `net`/`fs`/`process`, `tokio-util`, `tracing`, `thiserror`, `getrandom`, `petgraph` minimal), `crates/renvor-config/Cargo.toml` (`confique` with `toml` only, `toml`, `serde`, `secrecy`), and `crates/renvor-testkit/Cargo.toml` (`tokio` with **`test-util`** for `pause`/`advance`, `tracing-subscriber` without `json`/`ansi`)
- [x] T006 Declare the facade's dependencies in `crates/renvor/Cargo.toml` — a required dependency on `renvor-core` and an **optional** dependency on `renvor-config`, each written as **`{ path = "…", version = "0.0.0" }`**, plus a `config = ["dep:renvor-config"]` feature and `default = ["config"]` — so the re-export surface in T056 resolves and the plan's core-only path in T102 is reachable. **Both `path` and `version` are mandatory**: Phase 001 FR-040 forbids a **path-*only*** dependency in a publishable package, and cargo needs the `version` to rewrite the dependency at publish time. Update the manifest's "No dependencies" comment, which stops being true here
- [x] T007 Update `.github/workflows/release-dry-run.yml` to rehearse the **workspace** rather than the single crate: replace the `CRATE: renvor` premise with `cargo package --workspace --list`, `cargo package --workspace`, and `cargo publish --dry-run --workspace`, and adjust the SBOM and artifact steps to the multi-crate set while still excluding `xtask` (`publish = false`). Without this the rehearsal fails on every Phase 002 pull request — it triggers on `crates/**`, `Cargo.toml`, and `Cargo.lock`, all of which this phase changes
- [x] T008 Update the rehearsal procedure table in `specs/001-governance-foundation/contracts/package-metadata.md` from the single-crate `-p renvor` commands to the workspace commands, recording that the publishable set grew from 1 crate to 4 in Phase 002 and that the change is a consequence of ADR-0002's own provision that "later phases add implementation crates behind it"
- [x] T009 Draft `decisions/0008-publishable-crate-set-and-workspace-release-rehearsal.md` from `decisions/0000-template.md` in state **`proposed`** — never `accepted` — recording the D13 experiment, the four-crate publishable set, the `{ path, version }` dependency form, and the workspace rehearsal. Constitution §Development and Phase Workflow #4 requires a consequential decision to be *captured as a proposed ADR*; **FR-035 does not require this one to be accepted**, because a packaging decision is not custom infrastructure. **W-004 covers ADR-0007 alone and gives no authority to accept this record**, so it stays `proposed` and is listed as an open item in the phase evidence
- [x] T010 Resolve dependencies and update the tracked `Cargo.lock`, confirming the workspace still builds on the pinned 1.94.0 toolchain
- [x] T011 [P] Create `crates/renvor-core/src/lib.rs`, `crates/renvor-config/src/lib.rs`, `crates/renvor-testkit/src/lib.rs` with crate-level docs satisfying `missing_docs = "warn"`
- [x] T012 [P] Create the `examples/` directory with a `.gitkeep` and a README stating that examples must use no global mutable state (FR-032)
- [x] T013 Run `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo build --workspace` to confirm Gate 0 of `quickstart.md` passes on the new skeleton

**Checkpoint**: manifests and lockfile exist — the four gates can now run.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Prove the two contested designs, inventory the real dependency graph, and clear the governance gate.

**⚠️ CRITICAL**: No user story work begins until Phase 2 completes. No custom infrastructure merges until T029 records an outcome.

### 2A — Configuration compatibility proof gate (8 obligations)

- [x] T014 Create `crates/renvor-config/tests/proof_gate.rs` with the eight-obligation harness skeleton and fixture TOML files under `crates/renvor-config/tests/fixtures/`
- [x] T015 [P] Add fixtures covering: defaults, two ordered TOML files, environment overrides, a nested table with sibling keys, an array present in two layers, and a key that is a table in a file and a scalar in the environment, in `crates/renvor-config/tests/fixtures/` — this is the **single** fixture set for the crate; T065 reuses it rather than defining its own
- [x] T016 Prove obligations 1–3 in `crates/renvor-config/tests/proof_gate.rs`: precedence `defaults < earlier TOML < later TOML < environment`; per-key nested-table merge preserving sibling keys; wholesale array replacement with 0 concatenations
- [x] T017 Prove obligation 4 in `crates/renvor-config/tests/proof_gate.rs`: source attribution is reportable for **every** resolved key — **known negative evidence**, the candidate crate's field-by-field combinator discards the winning layer
- [x] T018 Prove obligations 5–6 in `crates/renvor-config/tests/proof_gate.rs`: an invalid **non-empty** environment value fails at Validate naming key, layer, and expected type; an invalid **empty** value (`KEY=""`) **also fails** with 0 fall-throughs — **known negative evidence**, the candidate documents "treated as unset". Obligation 6 is also the configuration-layer instance of FR-022: falling through to a lower layer because a value failed to decode is a silent fallback
- [x] T019 Prove obligations 7–8 in `crates/renvor-config/tests/proof_gate.rs` and via `cargo tree`: structural conflicts fail naming **both** layers; 0 JSON/YAML features appear in the resolved graph
- [x] T020 Record the gate outcome in `specs/002-core-kernel/research.md` §D6 with the per-obligation result, and **if any obligation failed**, state that the `serde` + `toml` partial-layer fallback is triggered and add it to the ADR-0007 scope in T026

### 2B — Provider-resolver feasibility and counter proof

- [x] T021 Create `crates/renvor-core/src/provider/graph.rs` with the instrumented adapter implementing `IntoNodeIdentifiers`, `IntoNeighbors`, and `NodeIndexable` over a compact adjacency list, carrying `Cell<u32>` counters
- [x] T022 Create `crates/renvor-core/tests/resolver_proof.rs` proving a single SCC pass yields both complete cycle membership and a usable order, with edges directed **dependent → dependency** so reverse topological order is the initialisation order
- [x] T023 Prove the counters in `crates/renvor-core/tests/resolver_proof.rs`: at 1024 providers and 8192 edges the observed values are **2048 / 8192 / 10240** against allowances of **2048 / 16384 / 18432**, and counters scale within `2 × providers` and `2 × edges` across at least 3 graph sizes (SC-021)
- [x] T024 Prove recursion depth in `crates/renvor-core/tests/resolver_proof.rs`: a **1024-node linear chain** resolves without exhausting the stack, executed on a **Tokio worker thread** whose default stack is smaller than the main thread's
- [x] T025 Record the resolver proof outcome in `specs/002-core-kernel/research.md` §D8; **if T024 fails**, record that the iterative-SCC fallback is triggered, note that it is then custom infrastructure under FR-035, and add it to the ADR-0007 scope in T026

### 2C — ADR-0007 governance gate

- [ ] T026 Draft `decisions/0007-phase-002-custom-kernel-primitives.md` from `decisions/0000-template.md`, scoping it to the typed-state map plus any custom infrastructure triggered by T020 or T025, and recording packages evaluated, their concrete shortcomings, ownership cost, testing burden, exit triggers, and a replacement strategy (FR-035)
- [ ] T027 Record explicitly in `decisions/0007-phase-002-custom-kernel-primitives.md` that **W-002 covers Phase 001 decision records only** and **W-003 covers Phase 001's phase-level review only**, and that **neither authorises accepting this record** — the authority is **W-004** and nothing else
- [ ] T028 Discharge the independent-review requirement under **W-004** (which must already be merged to `main` in `governance/waivers.md`): run two clean-context advisory reviews of ADR-0007 — one architecture, one security — label both **NON-INDEPENDENT** and **ADVISORY**, disposition every finding individually in `decisions/0007-phase-002-custom-kernel-primitives.md`, and record in `governance/phase-002-evidence.md` that all **four counted** W-004 compensating controls passed **and** that every one of the **three restated preconditions** holds. An agent review is **never** independent; W-004 is what makes a non-independent review sufficient, and only for this ADR. **Corrected 2026-08-16: this task read "all seven compensating controls", a count that survived from before three of the seven drafted controls were reclassified as preconditions in the merged waiver. `governance/waivers.md` on `main` is the authority, and it says four counted plus three preconditions. A number matching nothing in the ledger row leaves the acceptance gate unenforceable — the identical defect two advisory reviewers flagged as a blocker in the waiver itself**
- [ ] T029 Record the ADR-0007 outcome (accepted under W-004 with the controls evidenced, or blocked) in `decisions/0007-phase-002-custom-kernel-primitives.md` and `governance/phase-002-evidence.md`; **while this is unresolved, no task that merges custom infrastructure may proceed**

### 2D — Complete resolved transitive dependency inventory

- [x] T030 Create `governance/phase-002-dependency-inventory.md` listing **every** resolved package from `cargo tree --workspace --edges normal` — transitive included — with version, licence, MSRV compatibility, and advisory status (FR-040, SC-012, SC-017)
- [x] T031 Run `cargo deny check licenses advisories bans sources` against the **actual** `Cargo.lock` and record the result in `governance/phase-002-dependency-inventory.md`
- [x] T032 [P] Record enabled features (`cargo tree --edges features`) and duplicate-version findings (`cargo tree --duplicates`) in `governance/phase-002-dependency-inventory.md`
- [x] T033 Fail the phase if **any** resolved package lacks the evidence FR-040 requires, and record in `governance/phase-002-dependency-inventory.md` whether the direct-candidate evaluation in research §3 accurately predicted the transitive graph (SC-012, SC-017)
- [x] T034 Reconcile ADR-0003's dependency policy with FR-040 in `specs/002-core-kernel/research.md` §D12: ADR-0003 records that **reusable library crates do not commit a lockfile**, FR-040 requires versions resolvable **from a committed lockfile**, and this workspace tracks `Cargo.lock`. State which rule governs here and why, so the two are not left contradicting each other in writing (readiness CHK044)

### 2E — Shared kernel primitives (unblocked once 2A–2D report)

- [x] T035 Implement `KernelError` and `ErrorCategory` in `crates/renvor-core/src/error/mod.rs` with `thiserror`, covering all 13 categories in `contracts/error-taxonomy.md` C-E1
- [x] T036 [P] Implement the `LifecyclePhase` enum and its ordering invariant in `crates/renvor-core/src/lifecycle/phase.rs` so a backwards transition is unrepresentable rather than merely rejected (FR-001)
- [ ] T037 [P] Implement `CancellationToken` scoping in `crates/renvor-core/src/cancel/mod.rs` wrapping `tokio_util::sync::CancellationToken` with per-provider child scopes (FR-023, FR-024)
- [x] T038 [P] Implement the `EntropySource` trait, `OsEntropy` over `getrandom`, and the fixed-byte test source in `crates/renvor-core/src/observe/entropy.rs`, with **exactly one** generation site
- [x] T039 Implement `RunIdentifier` in `crates/renvor-core/src/observe/run_id.rs` as a pure function of supplied entropy bytes, encoding 0 host, clock, process, counter, or configuration inputs
- [ ] T040 Implement the `ConfigResolver` and `ResolvedConfig` port types in `crates/renvor-core/src/config_port/mod.rs` with **no** `serde`, `toml`, or parser dependency
- [ ] T041 Implement `TypedStateMap` in `crates/renvor-core/src/state/mod.rs` as `HashMap<TypeId, StateEntry>` retaining `&'static str` type names — **must not merge before T029 records the ADR-0007 outcome** (FR-010)

**Checkpoint**: gates reported, primitives available — user story implementation can begin.

---

## Phase 3: User Story 1 — An application starts, or refuses to, with a reason (Priority: P1) 🎯 MVP

**Goal**: An author assembles an application and either gets a running application or a diagnostic naming what was wrong. There is no third outcome.

**Independent Test**: Build an application with a deliberately invalid configuration value and a deliberately missing provider dependency; confirm each produces a distinct, actionable error and that no listener, task, or provider is left running.

**⚠️ Reaches its checkpoint only after T029.** T041 is blocked by the ADR-0007 outcome and T054 builds on T041, so the MVP increment terminates at the governance gate. This is stated here rather than left to be discovered.

- [ ] T042 [P] [US1] Write the lifecycle-order test in `crates/renvor-core/tests/lifecycle.rs` asserting the sequence `Load → Validate → Register → Boot → Ready` with 0 deviating runs (SC-001) (FR-001, FR-002)
- [ ] T043 [P] [US1] Write the rollback-order test in `crates/renvor-core/tests/lifecycle.rs` asserting reverse **actual initialisation** order, plus the negative control that asserting against registration order fails on a reordering graph (SC-002) (FR-004)
- [ ] T044 [P] [US1] Write the cycle and missing-dependency tests in `crates/renvor-core/tests/provider_graph.rs` asserting every provider in the cycle is named and 0 cases reach Boot (SC-005) (FR-013, FR-014)
- [ ] T045 [P] [US1] Write the silent-fallback prohibition test in `crates/renvor-core/tests/no_silent_fallback.rs` (FR-022): a required capability that is unavailable **fails the operation** — 0 runs boot a degraded application, 0 substitute a default for a missing required provider, and 0 downgrade a hard failure to a warning; include a positive control that a deliberately degrading provider **is** detected. The configuration-layer instance of FR-022 is proven separately at T018 obligation 6 and T069
- [ ] T046 [US1] Implement `Application` and its phase machine in `crates/renvor-core/src/lifecycle/application.rs`, recording the realised initialisation order (FR-001, FR-002)
- [ ] T047 [US1] Implement `ApplicationBuilder` in `crates/renvor-core/src/lifecycle/builder.rs` accepting ordered config resolvers, providers, and an entropy source
- [ ] T048 [US1] Implement `Provider`, `CapabilityId`, and `ProviderRegistry` in `crates/renvor-core/src/provider/registry.rs` with declared dependencies (FR-012)
- [ ] T049 [US1] Implement the declared-size ceilings in `crates/renvor-core/src/provider/registry.rs` — reject at Register on declared counts alone, naming ceiling and observed count (FR-039a) (SC-021)
- [ ] T050 [US1] Wire the instrumented resolver from T021 into Register in `crates/renvor-core/src/provider/mod.rs`, producing `InitialisationOrder` and `ResolutionReport` (FR-012, SC-021)
- [ ] T051 [US1] Implement `Internal` budget-exhaustion reporting in `crates/renvor-core/src/provider/graph.rs`, distinct from every author-facing diagnostic (FR-039c)
- [ ] T052 [US1] Implement Boot-failure rollback in `crates/renvor-core/src/lifecycle/rollback.rs`, replaying realised order backwards and reporting the originating failure
- [ ] T053 [US1] Implement rollback-during-rollback handling in `crates/renvor-core/src/lifecycle/rollback.rs` so every rollback failure is reported alongside the original (FR-005)
- [ ] T054 [US1] Add the duplicate-registration and missing-state error paths in `crates/renvor-core/src/state/mod.rs` with 0 panics in ordinary use (SC-004) — **depends on T041, therefore on T029** (FR-010, FR-011)
- [ ] T055 [P] [US1] Write the edge-case tests in `crates/renvor-core/tests/lifecycle_edges.rs` for cancellation during Boot and duplicate state registration (FR-024)
- [ ] T056 [US1] Re-export the US1 surface deliberately narrowly from `crates/renvor/src/lib.rs` using `pub use` only, with 0 implementation items, gating every `renvor-config` re-export behind `#[cfg(feature = "config")]` (ADR-0002) — **depends on T006 for the dependency declarations**

**Checkpoint**: an application starts deterministically or refuses with a reason — MVP is deliverable, once T029 clears.

---

## Phase 4: User Story 2 — An application stops without losing work or hanging (Priority: P2)

**Goal**: Shutdown refuses new work, drains within a bounded period, stops providers in reverse order, and reports honestly whether the drain completed.

**Independent Test**: Start an application with a task that outlives the drain budget; confirm the deadline is enforced, the forced stop is reported as such, and providers still stop in reverse order.

- [ ] T057 [P] [US2] Write drain tests in `crates/renvor-core/tests/drain.rs` for clean drain, over-budget drain, and **zero** budget with work in flight, asserting 0 clean reports for the latter two (SC-006) (FR-007)
- [ ] T058 [P] [US2] Write the double-shutdown and shutdown-before-Ready tests in `crates/renvor-core/tests/drain.rs` (FR-008, FR-009)
- [ ] T059 [US2] Implement `DrainOutcome` with `Clean` and `Incomplete { outstanding }` in `crates/renvor-core/src/lifecycle/drain.rs` (FR-007)
- [ ] T060 [US2] Implement the drain budget in `crates/renvor-core/src/lifecycle/drain.rs` with a documented 30 s default, author override, and **zero meaning stop immediately** while still reporting outstanding work on the same code path as a timeout (FR-042)
- [ ] T061 [US2] Implement new-work rejection after shutdown begins in `crates/renvor-core/src/lifecycle/drain.rs`, returning `ShuttingDown` rather than silently dropping or accepting (FR-006)
- [ ] T062 [US2] Implement idempotent shutdown in `crates/renvor-core/src/lifecycle/application.rs` so `Stop` runs at most once per provider under concurrent requests (FR-008)
- [ ] T063 [US2] Implement Stop-failure aggregation in `crates/renvor-core/src/lifecycle/rollback.rs` so the first failure never masks the rest (FR-005)
- [ ] T064 [P] [US2] Assert 0 unbounded waits in kernel-owned paths in `crates/renvor-core/tests/deadlines.rs` (SC-015) (FR-025)

**Checkpoint**: shutdown is bounded, honest, and reverse-ordered.

---

## Phase 5: User Story 3 — Configuration is typed, layered, and explains itself (Priority: P3)

**Goal**: Values arrive from defaults, TOML files, and environment variables; each source is decoded before layering; errors name key, layer, and expectation.

**Independent Test**: Supply the same key from two layers and confirm the documented precedence wins; supply a typed field from the environment as text and confirm it decodes, while undecodable text fails at Validate naming key, layer, and expected type.

**⚠️ Blocked by T020.** If the proof gate passed, T071 wraps the candidate crate. If **any** obligation failed, this phase implements the recorded `serde` + `toml` partial-layer fallback instead — which is custom infrastructure under FR-035 and is **therefore additionally blocked by T029**. The branch taken is decided at T020 and nowhere else; it is not rediscovered here.

- [ ] T065 [P] [US3] Write the precedence and merge tests in `crates/renvor-config/tests/layering.rs` covering all four merge behaviours and the 7 acceptance scenarios of User Story 3 (SC-020), **reusing the fixtures from T015** rather than adding a second fixture set
- [ ] T066 [US3] Implement per-source decoding in `crates/renvor-config/src/layer/decode.rs` — every source decoded against the declared schema **before** any merging (FR-015, FR-044)
- [ ] T067 [US3] Implement the ordered merge in `crates/renvor-config/src/layer/merge.rs`: tables merge per key, arrays replace wholesale, incompatible structural shapes fail naming **both** layers (FR-044)
- [ ] T068 [US3] Implement source attribution in `crates/renvor-config/src/layer/attribution.rs` producing the winning `SourceLayer` for **every** resolved key (FR-016)
- [ ] T069 [US3] Implement the environment layer in `crates/renvor-config/src/layer/env.rs` as an orderable layer at highest precedence, failing on any undecodable value **including the empty string**, with 0 unset-reinterpretation (FR-022) (FR-015)
- [ ] T070 [US3] Implement the TOML file layer in `crates/renvor-config/src/layer/file.rs` preserving spans so errors can name the key (FR-015)
- [ ] T071 [US3] Implement `ConfigResolver` for the typed resolver in `crates/renvor-config/src/resolver.rs`, satisfying the core port from T040, on whichever of the two branches T020 selected
- [ ] T072 [P] [US3] Write hostile-input tests in `crates/renvor-config/tests/hostile.rs` for malformed, truncated, and oversized TOML with 0 panics and bounded memory (FR-038)
- [ ] T073 [P] [US3] Add property or fuzz testing of the TOML boundary in `crates/renvor-config/fuzz/` or via a property-test harness, per constitution principle IX
- [ ] T074 [US3] Wire configuration into Load and Validate in `crates/renvor-core/src/lifecycle/application.rs` so invalid configuration prevents Boot with 0 providers started (SC-003) (FR-003, FR-017)

**Checkpoint**: configuration is typed, layered, attributed, and fails closed.

---

## Phase 6: User Story 4 — Failures are diagnosable without leaking secrets (Priority: P4)

**Goal**: An error says what failed, where, and what to try; a log reader learns the same without learning a credential.

**Independent Test**: Construct an error carrying a secret-bearing value; confirm the secret appears in no output form while the error remains identifiable.

- [ ] T075 [P] [US4] Write redaction tests in `crates/renvor-config/tests/redaction.rs` exercising **every** output path — `Display`, `Debug`, error message, error context, structured log fields, span fields, and serialization (FR-018, FR-021, SC-007)
- [ ] T076 [P] [US4] Add the **positive-control leaking wrapper** to `crates/renvor-config/tests/redaction.rs` — a type that deliberately does not redact and **must** be detected, proving the assertions can fire (SC-007)
- [ ] T077 [US4] Implement the Renvor `Secret<T>` boundary type in `crates/renvor-config/src/secret/mod.rs` wrapping `secrecy` for access control and zeroization (FR-018)
- [ ] T078 [US4] Implement `Display` for `Secret<T>` in `crates/renvor-config/src/secret/mod.rs` — the underlying crate provides **none**, so this path is entirely Renvor's (FR-018, FR-021)
- [ ] T079 [US4] Refuse serialization for `Secret<T>` in `crates/renvor-config/src/secret/mod.rs` by deliberately not implementing the crate's opt-in serialisation marker (FR-018)
- [ ] T080 [US4] Constrain error construction in `crates/renvor-core/src/error/context.rs` so a raw configuration value cannot enter a message or context map — only key, constraint, layer, and expected type (FR-021)
- [ ] T081 [P] [US4] Write the opaque-state test in `crates/renvor-core/tests/redaction.rs` that registers a **credential-bearing value without marking it secret** and fails if its contents appear anywhere (SC-016)
- [ ] T082 [US4] Implement type-name-only emission for registered state in `crates/renvor-core/src/state/mod.rs` (FR-037b)
- [ ] T083 [US4] Implement causal-chain preservation and category inspection in `crates/renvor-core/src/error/mod.rs` (FR-019, FR-020)

**Checkpoint**: no output path leaks a secret or opaque state, proven by a control that can detect a leak.

---

## Phase 7: User Story 5 — Health and readiness answer different questions (Priority: P5)

**Goal**: "Is this process alive?" and "should it receive work?" get two independent answers.

**Independent Test**: Drive the application to a state where it is alive but not ready and confirm the two answers differ.

- [ ] T084 [P] [US5] Write health/readiness disagreement tests in `crates/renvor-core/tests/health.rs` including the Drain state (SC-008) (FR-026)
- [ ] T085 [P] [US5] Write the panicking-contributor test in `crates/renvor-core/tests/health.rs` asserting it is caught, treated as not-ready, and identified
- [ ] T086 [US5] Implement `HealthState` with independent `Liveness` and `Readiness` in `crates/renvor-core/src/health/mod.rs`, with neither derived from the other (FR-026)
- [ ] T087 [US5] Implement `ReadinessContributor` registration and individual identification in `crates/renvor-core/src/health/contributor.rs` (FR-028)
- [ ] T088 [US5] Wire Drain to set readiness not-ready while liveness stays alive in `crates/renvor-core/src/lifecycle/drain.rs` (FR-027)

**Checkpoint**: health and readiness are provably independent.

---

## Phase 8: User Story 6 — The kernel is testable without a transport (Priority: P6)

**Goal**: An author starts a real application, injects a failure at a chosen phase, and asserts on observed order — with no HTTP client, port, or database.

**Independent Test**: Force a failure at each lifecycle phase in turn and assert the resulting rollback order, with no network or filesystem dependency.

> **Wording note.** `PLAN.md` §20 phrases the examples gate as "ordinary Rust"; FR-032 phrases it as
> "ordinary language constructs". The divergence is deliberate — the specification is kept
> technology-agnostic — and the two impose the same obligation.

- [ ] T089 [US6] Implement `FailureInjectionPoint` with `Fail`, `Panic`, and `Hang` behaviours in `crates/renvor-testkit/src/injection.rs`
- [ ] T090 [US6] Implement injection hooks for **all 7** lifecycle phases in `crates/renvor-testkit/src/harness.rs` (FR-030)
- [ ] T091 [US6] Implement time control in `crates/renvor-testkit/src/clock.rs` over `tokio::time::pause`/`advance` so deadlines and drain budgets run with 0 real elapsed time (FR-031)
- [ ] T092 [P] [US6] Write injection coverage tests in `crates/renvor-testkit/tests/injection.rs` asserting 7 of 7 phases are covered (SC-009)
- [ ] T093 [P] [US6] Write `examples/minimal.rs` — a working application with no global mutable state, no transport, no port, and no database (SC-014)
- [ ] T094 [P] [US6] Write `examples/providers.rs` demonstrating dependency ordering and rollback, using ordinary language constructs only
- [ ] T095 [P] [US6] Write `examples/configuration.rs` demonstrating layered configuration and a redacted secret

**Checkpoint**: the kernel is exercisable end to end without any transport.

---

## Phase 9: Polish & Cross-Cutting Concerns

- [ ] T096 [P] Implement the observability bootstrap in `crates/renvor-core/src/observe/bootstrap.rs` **returning** a subscriber or layer for the author to install; `Application::build()` installs nothing
- [ ] T097 Implement the optional `try_init_global()` helper in `crates/renvor-core/src/observe/bootstrap.rs` documenting its process-wide consequence and returning `AlreadyInstalled` on a second call — 0 panics, 0 silent successes, 0 silent replacements (FR-029)
- [ ] T098 [P] Write the tracing-ownership test in `crates/renvor-core/tests/observe_bootstrap.rs` proving `build()` installed nothing, by successfully installing a subscriber afterwards
- [ ] T099 Emit one span per lifecycle phase with structured fields and the run identifier in `crates/renvor-core/src/observe/spans.rs`, with 0 interpolated message strings (SC-018, FR-043)
- [ ] T100 [P] Write the run-identifier opacity tests in `crates/renvor-core/tests/run_id.rs` — deterministic fixed-entropy purity, single generation site, OS CSPRNG production wiring; label any random-sample check **non-gating** (SC-019)
- [ ] T101 [P] Add the crate-DAG assertions from `quickstart.md` Gate 13 to `xtask/src/main.rs` as a repeatable verification step, including all 5 positive controls
- [ ] T102 [P] Add the facade feature-isolation assertion to `xtask/src/main.rs` and `quickstart.md` Gate 13: `cargo tree -p renvor --no-default-features --edges normal` resolves **0** of `confique`, `toml`, `serde`, `secrecy`, with a **positive control** confirming that the same query **with** default features **does** resolve them — so the plan's stated limit (default-feature consumers still get the config dependencies) is proven true rather than assumed
- [ ] T103 [P] Document the **explicitly unstable** surface in `crates/renvor/src/lib.rs` crate-level docs, stating it in the published surface's own documentation and making no semantic-versioning promise (FR-036)
- [ ] T104 [P] Add the SC-022 agreement check to `xtask/src/main.rs`: the instability-closure sentence is **byte-identical** across the 3 normative locations (the 2026-08-16 clarification record, FR-036, and the Dependencies section) with 0 wording variations, and 0 phase numbers appear inside FR-036's normative closure clause
- [ ] T105 [P] Write rustdoc for every public item in `crates/renvor-core/src/`, `crates/renvor-config/src/`, and `crates/renvor-testkit/src/` so `missing_docs` produces 0 warnings (constitution principle X)
- [ ] T106 Run the complete verification sequence on **both** the pinned 1.94.0 toolchain **and** current stable, recording both in `governance/phase-002-evidence.md` with **0** failing and **0** silently skipped checks — every skip must be counted and named, not inferred from a passing exit status (SC-013)
- [ ] T107 Run the full `quickstart.md` gate sequence 0–15 and record each result in `governance/phase-002-evidence.md`, including any gate that could not be verified and why
- [ ] T108 Record the scope and authorization statements in `governance/phase-002-evidence.md`: **FR-041** — the phase implements no authentication, authorization, or identity, so authorization impact is none — and **FR-033 / SC-010** — 0 runtime capabilities outside declared scope, confirmed by review naming what was checked
- [ ] T109 Record the four transferred **Phase 001** deployment gates — written **`001-T102`, `001-T108`, `001-T109`, `001-T111`** — as still **non-completed and untouched** in `governance/phase-002-evidence.md`. The phase prefix is mandatory here: Phase 002 has its own T105, and an unprefixed "T105" in a Phase 002 evidence record reads as this phase's rustdoc task
- [ ] T110 Verify FR-034 by confirming 0 crates published, 0 tags, and 0 releases via `quickstart.md` Gate 14, treating any HTTP status other than 200 or 404 as a failure (SC-011)

---

## Dependencies

### Blocking order

```text
Phase 1 (Setup, T001–T013)
        │
        ▼
Phase 2 (Foundational, T014–T041)
   ├── 2A config proof gate    T014–T020 ──┐
   ├── 2B resolver proof       T021–T025 ──┤
   ├── 2C ADR-0007 gate        T026–T029 ──┼──▶ blocks Phase 3+
   ├── 2D dependency inventory T030–T034 ──┤
   └── 2E shared primitives    T035–T041 ──┘   (T041 additionally blocked by T029)
        │
        ▼
Phase 3 US1 (P1) ──▶ Phase 4 US2 (P2) ──▶ Phase 7 US5 (P5)
        │
        ├──▶ Phase 5 US3 (P3)  [blocked by T020; by T029 too if the fallback triggered]
        │           │
        │           ▼
        └──▶ Phase 6 US4 (P4)  [needs T077–T079 in US3's crate]
        │
        └──▶ Phase 8 US6 (P6)
                    │
                    ▼
              Phase 9 Polish (T096–T110)
```

### Story dependencies

| Story | Depends on | Reason |
|---|---|---|
| US1 (P1) | Phase 2 | needs the resolver, error taxonomy, and state map |
| US2 (P2) | US1 | an application must start before it can stop |
| US3 (P3) | Phase 2A outcome | the implementation differs depending on the gate result |
| US4 (P4) | US3 | the secret type lives in the configuration crate |
| US5 (P5) | US2 | readiness must react to Drain |
| US6 (P6) | US1, US2 | the harness injects into a real lifecycle |

### Hard blocks

- **T041 (typed-state map) must not merge before T029** records the ADR-0007 outcome — FR-035 requires an accepted ADR for custom infrastructure, and neither W-002 nor W-003 authorises accepting it. **W-004** does, under its compensating controls.
- **T054 depends on T041**, so the MVP checkpoint at the end of Phase 3 also waits on T029. Stated so it is not discovered late.
- **The iterative-SCC fallback, if T024 triggers it, is also custom infrastructure** and falls under the same T029 block.
- **The `serde` + `toml` configuration fallback, if T020 triggers it, is also custom infrastructure** and falls under the same T029 block.
- **Phase 5 must not begin before T020** reports the configuration gate outcome.
- **T033 must pass before adoption is confirmed** — the research table covers direct candidates only.
- **T056 depends on T006** — the facade cannot re-export crates its manifest does not depend on.
- **T102 depends on T006** — the `config` feature must exist before its isolation can be measured.
- **T007 must land in the same change as T006.** The moment the facade gains a dependency, the
  single-crate release rehearsal becomes unrunnable. Splitting them across commits leaves a
  knowingly-red workflow on the branch, which trains reviewers to ignore a red check.
- **T001–T003 must be `publish = true` before T006.** A publishable crate cannot depend on an
  unpublishable one; the chain fails at the first `publish = false` member (research §D13, case 4).

## Parallel Execution Examples

**Phase 1**: T002, T003 in parallel; then T011, T012 in parallel.

**Phase 2**: 2A (T014–T020), 2B (T021–T025), and 2D (T030–T034) are **independent and run in parallel** — different crates, different files. 2C (T026–T029) starts once 2A and 2B report, because their outcomes define the ADR's scope.

**Phase 3 (US1)**: T042, T043, T044, T045 in parallel (four test files); T055 in parallel with implementation.

**Phase 6 (US4)**: T075, T076, T081 in parallel — different test files.

**Phase 9**: T096, T098, T100, T101, T102, T103, T104, T105 in parallel — different files.

## Implementation Strategy

**MVP scope**: **Phase 1 + Phase 2 + Phase 3 (User Story 1)**. That delivers an application that
starts deterministically or refuses with an actionable reason — the phase's reason to exist. Every
later story assumes it.

**The MVP includes a governance decision, not only code.** Phase 2 contains T029, and Phase 3's
T054 depends on it through T041. An implementer who plans the MVP as pure engineering work will
stall at the gate. Under **W-004** the gate is dischargeable by the maintainer with compensating
controls; without W-004 it is not dischargeable at all in a single-maintainer project.

**Incremental delivery**: each user story phase is independently testable and ends at a checkpoint.
US1 alone is a coherent increment; US1+US2 gives a complete lifecycle; adding US3 gives configured
applications; US4 makes them safe to operate; US5 makes them operable; US6 makes them testable.

**Sequencing note**: the four gates in Phase 2 are deliberately front-loaded because **two of them
can change the design**. Discovering the configuration fallback after implementing against the
candidate crate, or discovering the resolver's stack limit after wiring it into Boot, would waste
the work in Phases 3 and 5. Front-loading them is the difference between a proof and a rewrite.
