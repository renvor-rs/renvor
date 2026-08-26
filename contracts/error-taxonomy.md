---
description: "Phase 002 contract — kernel error categories, causal chaining, and redaction guarantees"
version: "1.3.0"
status: "unstable — the surface it describes is explicitly unstable under FR-036; this version identifies the contract text, not a stability promise. 1.3.0 (2026-08-26) scopes C-E1's category table to kernel errors and states what C-E2 requires at a third-party boundary; no category was added, removed, or renamed"
---

# Contract: Error Taxonomy

**Feature**: the phase specification *(internal record)* | **Satisfies**: FR-019…FR-022, FR-037; SC-004, SC-005, SC-007, SC-016
**Status**: contract for an **explicitly unstable** surface (FR-036).

> **Why this exists before adapters do.** An error taxonomy retrofitted after HTTP, GraphQL, and
> persistence adapters exist inherits *their* shapes instead of governing them. Defining it now is
> the point of doing it in the kernel phase.

## C-E1 — Every error carries a matchable category

A category **MUST** be inspectable programmatically, not only by reading message text. Matching on
a message string is not an API; it is a defect waiting for a rewording.

| Category | Raised when | Names |
|---|---|---|
| `Configuration` | a value is missing, ill-typed, out of range, or undecodable | key, violated constraint, source layer |
| `ConfigurationConflict` | incompatible structural shapes across layers | key, **both** layers |
| `StateDuplicate` | a second value of the same type is registered | the type |
| `StateMissing` | an unregistered type is retrieved | the type |
| `DependencyCycle` | providers form a cycle | **every** provider in the cycle |
| `DependencyMissing` | a declared dependency is unprovided | dependent **and** missing capability |
| `CapabilityDuplicate` | two providers offer the same capability | the capability **and both** providers |
| `LimitExceeded` | provider or edge ceiling breached at Register | the ceiling **and** the observed count |
| `ProviderInit` | a provider fails during Boot | the failing provider |
| `ProviderStop` | a provider fails during Stop or rollback | the failing provider; never masks siblings |
| `Cancelled` | a cancellation signal ended the operation | the phase reached |
| `DeadlineExceeded` | a bounded wait elapsed | the deadline and the operation |
| `ShuttingDown` | work submitted after shutdown began | the rejected operation |
| `ResourceUnavailable` | the host refused a resource the kernel asked for | the resource, the operation, the host's own reason |
| `Internal` | **resolution work budget exhausted** — a defect in the kernel | the counters observed |

### This table governs KERNEL errors — added 2026-08-26

`ErrorCategory` is the **kernel's** vocabulary. A crate outside the kernel that has its own closed,
programmatically matchable classification satisfies C-E1 with that classification; it is **not**
required to project onto this table, and **MUST NOT** project onto it lossily.

`renvor_database::DatabaseErrorKind` is the worked example. It carried a `category()` method whose
table named five kinds and sent the other seventeen through `_ => ErrorCategory::Internal` — so a
violated unique key, an absent row, and an edited migration all reported as a **kernel defect**.
This contract says of that category: *"If an author ever sees `Internal`, the kernel is wrong — not
their graph."* The projection made the taxonomy's one unambiguous category the routine answer for
ordinary database outcomes.

No other category was closer. `StatementRejected` is not a `Configuration` error, a
`DependencyMissing`, or a `LimitExceeded`, and re-aiming the arm at one of those to keep the method
would have been the same defect with a quieter symptom. The projection was **removed**, along with
`renvor-database`'s dependency on `renvor-core`, which nothing else used.

`DatabaseErrorKind` remains what C-E1 actually asks for: a category a caller matches on rather than
a message it parses. Two vocabularies with no bridge between them is the honest arrangement — the
bridge was the defect. Enforced twice: a `compile_fail` documentation test proves the method is
gone, and `xtask` step 7 asserts against the resolved graph that the crate dependency is gone, each
with a control.

### Revision 1.2.0 — `ResourceUnavailable` added

Added on **2026-08-16**, during T113, on the same footing as revision 1.1.0: implementation
evidence, not review opinion.

`Load` and `Validate` bound each author-supplied source call by running it on a worker thread, and
a thread that never returns is **leaked** — the cost recorded as open item 15. Leaking threads
means a long-lived process can genuinely run out of them, so the spawn itself has to be able to
fail. `std::thread::spawn` **panics** when the operating system refuses; `std::thread::Builder::spawn`
returns the refusal. Renvor uses the builder form, which produced an outcome with no category:

| Alternative | Why it was rejected |
|---|---|
| Panic (`std::thread::spawn`) | SC-004 requires **0** panics in ordinary use, and exhaustion is foreseeable here rather than hypothetical |
| Carry on without the source | A **silent fallback** — FR-022 — booting on configuration nobody read |
| Report it as `Internal` | It is a **host** failure, not a Renvor defect, and `Internal` says the opposite |
| Report it as `DeadlineExceeded` | **False**: no deadline elapsed; the wait never started |
| Report it as `LimitExceeded` | That names a ceiling **Renvor declared**. Renvor declares no ceiling on OS threads |

The category count moves from **14 to 15**. This is the identical argument the builder module
already makes for `EntropySource` — an operating system that refuses a resource is not a broken
framework — now applied to the second resource the kernel asks the host for.

### Revision 1.1.0 — `CapabilityDuplicate` added

Added on **2026-08-16**, during T048, on implementation evidence rather than review opinion.

Building `ProviderRegistry` surfaced a case no earlier artifact covered: **two providers offering
the same `CapabilityId`**. A dependency on that capability has no single answer, and every way of
not saying so was already prohibited by something the project had agreed:

| Alternative | Why it was rejected |
|---|---|
| Pick a winner (first or last registration) | A **silent fallback** — FR-022 and C-E4 |
| Panic | SC-004 requires **0** panics in ordinary use |
| Report it as `DependencyMissing` | The diagnostic would be **false**: the capability *is* provided, twice |
| Report it as `Internal` | It is an author mistake, not a Renvor defect, and `Internal` says the opposite |

The category count in this contract moved from **13 to 14** at this revision. The `ErrorCategory::ALL`
array and its length assertion moved with it, so a future edit that adds a category without
updating this contract still fails loudly.

**`Internal` was not widened to absorb it.** That would have been the smaller change and the worse
diagnostic — see the note below on why that category means exactly one thing.

**`Internal` is deliberately distinct.** FR-039(c) forbids reporting a cycle by exhausting the
work budget, so budget exhaustion **MUST NOT** be representable as any author-facing category. If
an author ever sees `Internal`, the kernel is wrong — not their graph.

## C-E2 — Causal chains are preserved

Every error preserves its cause, and each link is attributable. Flattening a chain into a single
message destroys the only information that makes a nested failure diagnosable.

### What "preserved" means at a third-party boundary — added 2026-08-26

A chain has two kinds of link, and this contract requires opposite things of them.

| Link | Requirement |
|---|---|
| A **Renvor** cause — an error this workspace constructed, whose fields it controls | **MUST** be preserved and reachable through `Error::source` |
| A **third-party driver's** error, carrying text this workspace neither bounds nor redacts | **MUST NOT** be reachable. The chain terminates at the last safe link |

The second is not an exception to the first. C-E3 forbids emitting a secret through **0** error
output forms, and a driver message routinely contains the offending value, the table and column,
and — for a connection failure — the host. A chain that reached it would satisfy C-E2 by
violating C-E3, so the two are read together: **preserve every link you can vouch for, and stop
at the first one you cannot.**

The driver's own text is not destroyed. It is emitted through `tracing` at `debug`, which reaches
operators rather than callers — the same split this contract already makes between a Problem
Details document and a telemetry span.

**Worked example, and the defect that produced this section.**
`renvor_database::StartupDiagnostic` shipped in Phase 008 holding only a
`DatabaseErrorKind` — it discarded the `DatabaseError` the provider had actually failed with, so
`source` answered `None` and the diagnostic was the whole story. That is a flattened chain, which
this contract forbids, and it was flattened one link **too early**: `DatabaseError` is a Renvor
type holding a single fieldless enum, so nothing about preserving it risks a secret.

It now stores that error, returns it from `source`, and stops there — `DatabaseError` implements
no `source` of its own, so the termination is structural rather than a policy someone applies.
Both halves are asserted: `no_diagnostic_can_render_a_secret` walks the whole chain for every one
of the renderable diagnostics, and asserts the chain is **exactly one link long**, so a `source`
that quietly started answering `None` again would fail rather than making the walk vacuous.

## C-E3 — Redaction is total

**0** error output forms may emit:

1. a **secret-marked configuration value** (FR-018, FR-037a); or
2. the **contents of registered typed state** (FR-037b).

For registered state the kernel **MAY** emit the **type name only**. The kernel **MUST NOT** assume
opaque state is safe to print merely because it was not marked secret — the author may have
registered a credential-bearing value without marking anything.

**Redaction of secret configuration values is a property of Renvor's own boundary type, not of the
underlying secret crate.** That crate supplies a redacted `Debug` and zeroization; it implements
**no `Display` at all**, and it does not reach error messages, error context, tracing fields, or
serialization. Every one of those paths is Renvor's to implement and Renvor's to test — see
[configuration-contract.md](./configuration-contract.md) C-C9. Error construction therefore
**MUST NOT** accept a raw configuration value into a message or a context map; it accepts the key,
the constraint, the layer, and the expected type, which are safe by construction.

- **Acceptance**: SC-016 — asserted by a test that registers a credential-bearing value
  **without** marking it secret and fails if its contents appear in any error, log, trace, or
  diagnostic output.

## C-E4 — No silent fallbacks

A required capability that is unavailable **MUST** fail the operation rather than degrade it.
Substituting an in-memory, insecure, or differently durable implementation is prohibited by
constitution principle III, and reporting success after partial failure is prohibited by
principle IV.

## C-E5 — Errors are not panics

Duplicate state registration and missing state retrieval each produce a **distinct, named error**.

- **Acceptance**: SC-004 — **0** cases produce a panic in ordinary use.

The one place a panic is tolerated is a **readiness contributor written by the author**, which the
kernel catches and treats as not-ready with the contributor identified — a third-party probe must
not take the process down.
