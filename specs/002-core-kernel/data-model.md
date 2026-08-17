---
description: "Phase 002 data model — kernel entities, fields, relationships, validation rules, and state transitions"
---

# Data Model: Transport-Independent Core Kernel

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md) | **Research**: [research.md](./research.md)
**Date**: 2026-08-16

> **This is a design model, not an API listing.** Field names are indicative; the binding
> statements are the **validation rules** and **state transitions**, each traced to a requirement.
> The public surface is **explicitly unstable** (FR-036) and every shape here may change without a
> compatibility procedure while that window is open.

## 1. Entity overview

```text
                    ┌──────────────────────┐
                    │ ApplicationBuilder   │  consumes config + providers
                    └──────────┬───────────┘
                               │ build()  →  Application | KernelError
                    ┌──────────▼───────────┐
                    │     Application      │──owns──┐
                    └──────────┬───────────┘        │
                               │                    │
        ┌──────────────┬───────┴───────┬────────────┼──────────────┐
        ▼              ▼               ▼            ▼              ▼
 ProviderRegistry  TypedStateMap  Configuration  HealthState   RunIdentifier
        │                                             │
        │ resolve() → InitialisationOrder      ┌──────┴──────┐
        │              + ResolutionReport      ▼             ▼
        ▼                                  Liveness      Readiness
     Provider (n)                                            │
        │ depends-on (edges ≤ 8192)                          ▼
        └──────────────────────────────────► ReadinessContributor (n)
```

## 2. Entities

### 2.1 Application

The assembled, runnable unit. Owns the lifecycle and everything reachable from it.

| Field | Type | Notes |
|---|---|---|
| `phase` | `LifecyclePhase` | current phase; the only mutable lifecycle field |
| `state` | `TypedStateMap` | registered exactly once per type |
| `config` | `Configuration` | resolved, validated, source-attributed |
| `registry` | `ProviderRegistry` | resolved order retained |
| `initialised` | `Vec<ProviderId>` | **actual** initialisation order, appended as each provider succeeds |
| `cancel` | `CancellationToken` | root token; providers receive children |
| `health` | `HealthState` | liveness and readiness held **independently** |
| `run_id` | `RunIdentifier` | fixed for the process run |
| `drain_budget` | `Duration` | default **30 s**, author-overridable, **0** permitted |

**Validation rules**

- **V-A1** (FR-001): `phase` transitions only along the declared order; a transition to an earlier
  phase is unrepresentable rather than merely rejected.
- **V-A2** (FR-004): rollback replays `initialised` **in reverse**. It is the realised order, not
  the registration order — resolution may reorder, and asserting the wrong one passes while being
  wrong.
- **V-A3** (FR-008): a second shutdown request is accepted and is a no-op; `Stop` runs **at most
  once** per provider.
- **V-A4** (FR-025): every kernel-owned wait carries a deadline. An unbounded wait is a defect.

### 2.2 ApplicationBuilder

The assembly surface. Consumes configuration sources and providers, produces an `Application`
**or** a diagnostic — there is no third outcome (User Story 1).

| Field | Type | Notes |
|---|---|---|
| `config_sources` | `Vec<ConfigSource>` | **ordered**; order is load-bearing (FR-044) |
| `providers` | `Vec<Provider>` | registration order; not necessarily initialisation order |
| `drain_budget` | `Option<Duration>` | `None` resolves to the documented 30 s default |
| `entropy` | `Box<dyn EntropySource>` | `OsEntropy` unless overridden by a test |

**Validation rules**

- **V-B1** (FR-017): `build()` performs Load and Validate. If either fails, **0** providers are
  booted and **0** listeners, tasks, or providers start.
- **V-B2** (FR-039a): provider count > **1024** or declared edges > **8192** fails at **Register**,
  naming the ceiling and the observed count, **before** any traversal.

### 2.3 Provider

A unit of capability with declared dependencies and initialise/stop behaviour.

| Field | Type | Notes |
|---|---|---|
| `id` | `ProviderId` | stable, human-readable; appears verbatim in diagnostics |
| `dependencies` | `Vec<CapabilityId>` | declared, not inferred |
| `provides` | `Vec<CapabilityId>` | what other providers may depend on |

**Validation rules**

- **V-P1** (FR-012): initialisation follows dependency order, not declaration order.
- **V-P2** (FR-014): a dependency nobody provides fails at Register naming **both** the dependent
  and the missing capability.
- **V-P3** (FR-005): a failure **during** rollback does not abort the remaining rollback; every
  rollback failure is reported alongside the original.

### 2.4 ProviderRegistry, InitialisationOrder, ResolutionReport

| Entity | Purpose |
|---|---|
| `ProviderRegistry` | the ordered, cycle-checked set |
| `InstrumentedGraph` | Renvor-owned adapter over a compact adjacency list; implements the three traits the SCC algorithm is generic over, and **holds the counters** |
| `InitialisationOrder` | the resolved sequence — the flattened output of a **single** strongly-connected-components pass, whose components come back in reverse topological order |
| `ResolutionReport` | the **observable examination counters** SC-021 asserts against |

Edges are directed **dependent → dependency**, so reverse topological order lists dependencies
first and **is** the initialisation order — no reversal pass, no second traversal.

`ResolutionReport` fields — these exist **because SC-021 requires them to be assertable**, not as
incidental telemetry. Counters are incremented inside `InstrumentedGraph`, so they are **exact
observations at a boundary Renvor owns**:

| Field | Type | Allowance at maximum graph | **Expected observed** |
|---|---|---|---|
| `providers_examined` | `u32` | ≤ **2048** (2 per accepted provider) | **2048** — outer scan + at-most-one visit |
| `edges_examined` | `u32` | ≤ **16384** (2 per accepted edge) | **8192** — each edge iterated exactly once |
| `work_units` | `u32` | ≤ **18432** (their sum) | **10240** |

> **Revision 2**: revision 1 specified Kahn's algorithm for ordering plus a separate pass for cycle
> reporting. Kahn alone consumes 2 examinations per node **and** 2 per edge — the whole allowance —
> so the second pass exceeded the budget. The single-pass design fits with the edge axis at half
> its allowance. The budget was **not** raised.

**Validation rules**

- **V-R1** (FR-013): a cycle fails with a diagnostic naming **every** provider in it — the
  strongly-connected component, not one representative node.
- **V-R2** (FR-039c): a cycle within the ceilings is reported **as a cycle**, never by exhausting
  the budget. Budget exhaustion is a **distinct internal-error variant** that no author-facing
  diagnostic can be mistaken for.
- **V-R3** (FR-039b): counters are compared against examination limits, **never** against elapsed
  time. **0** assertions in this area depend on wall-clock duration.

### 2.5 TypedStateMap

| Field | Type | Notes |
|---|---|---|
| `entries` | `HashMap<TypeId, StateEntry>` | keyed by type |
| `StateEntry.value` | `Box<dyn Any + Send + Sync>` | **opaque** to the kernel |
| `StateEntry.type_name` | `&'static str` | retained **solely** so diagnostics can name a type without printing a value |

**Validation rules**

- **V-S1** (FR-011): registering a second value of the same type is an error **naming the type**.
- **V-S2** (FR-010): retrieving an unregistered type produces a startup-time error, **never a
  runtime panic in ordinary use**.
- **V-S3** (FR-037b): the kernel emits `type_name` **only**. Contents never reach an error, log,
  trace, or diagnostic. Absence of a secret marking is **not** evidence that a value is safe to
  print.

### 2.6 Configuration

A typed value resolved from ordered layers, carrying source attribution and secret markings.

| Field | Type | Notes |
|---|---|---|
| `value` | `T: DeserializeOwned` | the author's declared schema |
| `sources` | `Map<KeyPath, SourceLayer>` | which layer won each key (FR-016) — **produced by Renvor's merge; the candidate crate's `Option::or`-style combinator discards it** |
| secret fields | `Secret<T>` — **a Renvor type** | wraps the secret crate for access control and zeroization, and **owns every output path the crate does not**: `Display`, error message and context, tracing fields, and serialization (which is refused) |

`SourceLayer` is ordered lowest to highest: `Defaults` → `File(index)` → `Environment`.
**`Environment` is a layer with a precedence position, not a per-field annotation.**

`ResolvedConfig` is the **port type** `renvor-core` defines so configuration can participate in
Load and Validate without core depending on a parser:

| Field | Type | Notes |
|---|---|---|
| `value` | `Box<dyn Any + Send + Sync>` | opaque to core; registered into the typed-state map |
| `type_name` | `&'static str` | FR-037(b) — name only, never contents |
| `attribution` | `BTreeMap<KeyPath, SourceLayer>` | FR-016 |

**Validation rules**

- **V-C1** (FR-044 step 1): **each source is decoded against the declared schema before any
  merging**. Textual environment decoding into a declared type is permitted and is **not**
  cross-layer coercion.
- **V-C2** (FR-044 step 1): an environment value that cannot decode fails at **Validate**, naming
  the key, the source layer, and the expected type — **3 of 3** elements, always. **There is no
  empty-string exemption**: `KEY=""` for a field that cannot decode `""` fails, and is never
  reinterpreted as unset. *(The candidate crate documents the opposite — "treated as unset" — which
  is a silent fallback; whichever implementation is selected must not inherit it.)*
- **V-C7** (FR-016): **attribution is produced by Renvor's merge step**, because the candidate
  crate's field-by-field combinator discards which layer won. An implementation that cannot report
  the winning layer for every key fails the proof gate.
- **V-C3** (FR-044a): tables merge per key; sibling keys from lower layers survive.
- **V-C4** (FR-044b): arrays **replace wholesale**. Never concatenated — element-wise merging
  would make removing an entry impossible.
- **V-C5** (FR-044c): incompatible structural shapes across layers **fail**, naming the key and
  both layers. Not coerced, not last-wins.
- **V-C6** (FR-015): exactly three source kinds. **0** JSON and **0** YAML sources — enforced by
  not enabling those `confique` features, so the prohibition is structural.

### 2.7 LifecyclePhase

```text
Load ──▶ Validate ──▶ Register ──▶ Boot ──▶ Ready ──▶ Drain ──▶ Stop
  │          │            │          │        │         │
  └──────────┴────────────┴──────────┴────────┴─────────┴──▶ Rollback(reverse initialised)
```

| Phase | Entry condition | Failure behaviour |
|---|---|---|
| `Load` | sources supplied | fail; nothing started |
| `Validate` | sources decoded | fail naming key, constraint, layer; **0** providers booted |
| `Register` | config valid | fail on ceiling breach, cycle, or missing dependency; **0** providers booted |
| `Boot` | order resolved | roll back initialised providers in reverse **actual** order |
| `Ready` | all providers initialised | readiness may still report not-ready |
| `Drain` | shutdown requested | bounded by budget; outstanding work reported honestly |
| `Stop` | drain ended | every provider stopped; **all** failures reported, not just the first |

**Validation rules**

- **V-L1** (FR-002): the observed sequence is inspectable by a test **without instrumenting
  internals** — it is also derivable from the emitted spans (FR-043).
- **V-L2** (FR-009): shutdown before `Ready` still rolls back whatever was initialised.
- **V-L3** (edge case): a failure during rollback while already rolling back does not abandon the
  remainder.

### 2.8 DrainOutcome

| Variant | Meaning |
|---|---|
| `Clean` | drain completed with **0** outstanding work |
| `Incomplete { outstanding }` | budget elapsed **or** budget was zero with work in flight |

**Validation rules**

- **V-D1** (FR-007): an incomplete drain is **never** reported as clean.
- **V-D2** (FR-042): a **zero** budget with work in flight produces `Incomplete` on the **same
  code path** as a timeout. This is what stops an immediate stop from silently reading as a clean
  one.
- **V-D3** (FR-006): new work submitted after shutdown begins is **rejected with an error saying
  so** — not silently dropped and not silently accepted.

### 2.9 KernelError

| Field | Type | Notes |
|---|---|---|
| `category` | `ErrorCategory` | matchable enum, not message text (FR-019) |
| `source` | `Option<Box<dyn Error>>` | causal chain preserved (FR-020) |
| `context` | structured fields | key, layer, provider id, limit, observed count |

**Validation rules**

- **V-E1** (FR-021): **0** error output forms emit a secret-bearing value.
- **V-E2** (FR-022): a required capability that is unavailable **fails the operation**. Degrading
  it is prohibited.

Categories are enumerated in [contracts/error-taxonomy.md](./contracts/error-taxonomy.md).

### 2.10 HealthState, ReadinessContributor

| Field | Type | Notes |
|---|---|---|
| `liveness` | `Liveness` | "is this process alive?" |
| `readiness` | `Readiness` | "should it receive work?" |
| `contributors` | `Vec<ReadinessContributor>` | each individually identifiable |

**Validation rules**

- **V-H1** (FR-026): the two are **independently queryable and able to disagree**. Deriving one
  from the other makes SC-008 unsatisfiable by construction.
- **V-H2** (FR-027): entering `Drain` makes readiness not-ready **while liveness stays alive**.
- **V-H3** (FR-028): a failing contributor is named.
- **V-H4** (edge case): a contributor that **panics** is caught and treated as not-ready with the
  contributor identified — a panicking probe must not take down the process.

### 2.11 RunIdentifier and EntropySource

| Field | Type | Notes |
|---|---|---|
| `RunIdentifier` | 16 opaque bytes, text-encoded | attached to **every** emitted record |
| `EntropySource` | trait | `OsEntropy` (production) / fixed-byte source (tests) |

**Validation rules**

- **V-N1** (FR-043, SC-019a): produced at **exactly 1** generation site from cryptographically
  secure random bytes and **no other input** — **0** host, clock, process, counter, or
  configuration values.
- **V-N2** (SC-019b): with a fixed entropy source the encoded identifier is a **pure function of
  exactly those bytes** — deterministic, with **0** probabilistic assertions.
- **V-N3** (SC-019c): **1 of 1** production entropy sources is the operating-system CSPRNG.
- **V-N4** (SC-019d): any random-sample collision or ordering check is **non-gating**; **0**
  release gates depend on it.

### 2.12 FailureInjectionPoint *(testkit)*

| Field | Type | Notes |
|---|---|---|
| `phase` | `LifecyclePhase` | one of **7** |
| `behaviour` | `Fail(KernelError)` \| `Panic` \| `Hang` | `Hang` exercises deadline enforcement |

**Validation rules**

- **V-F1** (FR-030, SC-009): injection is available at **7 of 7** phases, each covered by a test.
- **V-F2** (FR-031): deadline and drain behaviour are exercised with **0** real elapsed time.

## 3. Relationships and cardinality

| From | To | Cardinality | Constraint |
|---|---|---|---|
| Application | Provider | 1 → 0..1024 | ceiling enforced at Register (V-B2) |
| Provider | Provider (depends-on) | 0..8192 edges total | ceiling enforced at Register |
| Application | TypedState | 1 → 0..n | **exactly one value per type** (V-S1) |
| Application | RunIdentifier | 1 → 1 | fixed for the run |
| Configuration | SourceLayer | 1 → 1..n | ordered; `Environment` always highest |
| Application | ReadinessContributor | 1 → 0..n | each individually identifiable |

## 4. Invariants that must hold in every state

1. **I-1** — `initialised` is a prefix-consistent record of realised order; rollback is its exact
   reverse (FR-004, SC-002 at **100%**).
2. **I-2** — **0** secret-marked values and **0** registered-state contents appear in any output
   (SC-007, SC-016).
3. **I-3** — the phase sequence never runs backwards (SC-001, **0** deviating runs).
4. **I-4** — resolution never exceeds its examination budget for a graph within the ceilings, and
   never uses time as a bound (SC-021).
5. **I-5** — an incomplete drain is never reported as clean, including the zero-budget case
   (SC-006, **0** clean reports).
6. **I-6** — **0** unbounded waits exist in kernel-owned paths (SC-015).
