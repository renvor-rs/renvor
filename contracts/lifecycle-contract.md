---
description: "Phase 002 contract — lifecycle phase order, rollback, drain, and shutdown semantics"
version: "1.0.0"
status: "unstable — the surface it describes is explicitly unstable under FR-036; this version identifies the contract text, not a stability promise"
---

# Contract: Lifecycle

**Feature**: the phase specification *(internal record)* | **Satisfies**: FR-001…FR-009, FR-042; SC-001, SC-002, SC-006, SC-009, SC-015
**Status**: contract for an **explicitly unstable** surface (FR-036). Breaking changes are permitted while the window is open.

## C-L1 — Phase order

```text
Load → Validate → Register → Boot → Ready → Drain → Stop
```

- The kernel **MUST NOT** expose any ordering in which a later phase runs before an earlier one.
- The observed sequence **MUST** be inspectable by a test **without instrumenting internals**, and
  **MUST** additionally be derivable from the emitted spans (see
  [observability-contract.md](./observability-contract.md)).
- **Acceptance**: SC-001 — **0** runs observe a different order.

## C-L2 — What each phase guarantees on failure

| Phase | On failure |
|---|---|
| `Load` | nothing has started; the error names the source that could not be read |
| `Validate` | **0** providers booted, **0** listeners opened; the error names key, violated constraint, and source layer |
| `Register` | **0** providers booted; ceiling breach, cycle, or missing dependency each produce a distinct diagnostic |
| `Boot` | every already-initialised provider is stopped in **reverse actual initialisation order**; the originating failure is reported with the failing provider identified; `Ready` is not reached |
| `Ready` | n/a — reaching Ready is the success condition |
| `Drain` | the drain ends at its deadline and reports outstanding work; shutdown continues rather than hanging |
| `Stop` | remaining providers are still stopped; **every** failure is reported, not just the first |

## C-L3 — Reverse order is reverse **actual initialisation** order

Dependency resolution may reorder providers relative to registration. The kernel records the
**realised** initialisation sequence and replays it backwards.

> A test asserting against declaration order can pass while the implementation is wrong. Tests
> **MUST** assert against observed initialisation order.

- **Acceptance**: SC-002 — for a failure at position *n* of *k*, shutdown order is the exact
  reverse of initialisation order in **100%** of runs.

## C-L4 — Rollback during rollback

A provider that fails **during** rollback **MUST NOT** abort the remaining rollback. Every
rollback failure is reported **alongside** the original failure; neither masks the other.

## C-L5 — Drain budget

| Property | Value |
|---|---|
| Default | **30 seconds**, documented |
| Overridable | yes, by the author |
| Zero | **skip the drain, stop immediately** |
| Zero is invalid | **no** — it is a valid configuration |
| Zero means wait forever | **no** |

A zero budget with work in flight **MUST** report that work as outstanding, on the **same code
path** a timed-out drain uses.

- **Acceptance**: SC-006 — an over-budget drain reports incomplete in **100%** of runs; **0** are
  reported as clean; the same holds for the zero-budget case.

## C-L6 — Shutdown behaviour

- **Idempotent**: requesting shutdown more than once is safe; `Stop` runs **at most once** per
  provider (FR-008).
- **Before Ready**: shutdown requested before `Ready` still rolls back whatever was initialised,
  in reverse order (FR-009).
- **New work after shutdown begins**: rejected **with an error stating the application is shutting
  down**. It is neither silently dropped nor silently accepted (FR-006).
- **Concurrent double shutdown**: safe; the second request observes the first's outcome.

## C-L7 — Deadlines

Every kernel-owned wait **MUST** be bounded. An unbounded wait in a kernel-owned path is a defect,
not a configuration choice.

- **Acceptance**: SC-015 — **0** unbounded waits exist in kernel-owned paths.

## C-L8 — Cancellation

- A cancellation signal propagates to running work (FR-023).
- Cancellation arriving in **any** phase — including `Boot` — **MUST NOT** leave a provider
  half-initialised (FR-024).

## C-L9 — Failure injection

Failure **MUST** be injectable at **each of the 7 phases**, with `Fail`, `Panic`, and `Hang`
behaviours; `Hang` exercises deadline enforcement without real elapsed time.

- **Acceptance**: SC-009 — **7 of 7** phases injectable, **100%** covered by a test.
