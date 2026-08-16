---
description: "Phase 002 contract — kernel error categories, causal chaining, and redaction guarantees"
version: "1.0.0"
status: "unstable — the surface it describes is explicitly unstable under FR-036; this version identifies the contract text, not a stability promise"
---

# Contract: Error Taxonomy

**Feature**: [../spec.md](../spec.md) | **Satisfies**: FR-019…FR-022, FR-037; SC-004, SC-005, SC-007, SC-016
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
| `LimitExceeded` | provider or edge ceiling breached at Register | the ceiling **and** the observed count |
| `ProviderInit` | a provider fails during Boot | the failing provider |
| `ProviderStop` | a provider fails during Stop or rollback | the failing provider; never masks siblings |
| `Cancelled` | a cancellation signal ended the operation | the phase reached |
| `DeadlineExceeded` | a bounded wait elapsed | the deadline and the operation |
| `ShuttingDown` | work submitted after shutdown began | the rejected operation |
| `Internal` | **resolution work budget exhausted** — a defect in the kernel | the counters observed |

**`Internal` is deliberately distinct.** FR-039(c) forbids reporting a cycle by exhausting the
work budget, so budget exhaustion **MUST NOT** be representable as any author-facing category. If
an author ever sees `Internal`, the kernel is wrong — not their graph.

## C-E2 — Causal chains are preserved

Every error preserves its cause, and each link is attributable. Flattening a chain into a single
message destroys the only information that makes a nested failure diagnosable.

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
