# ADR-0037: A kernel retry policy with a pure jittered schedule, closed retryability, and idempotency stated per capability

| Field | Value |
|---|---|
| **ID** | 0037 |
| **State** | `proposed` |
| **Reviewer** | *(required to enter `accepted`)* |
| **Review date** | *(required to enter `accepted`)* |
| **Superseded by** | *(not superseded)* |

> **A record MUST NOT be marked `accepted` without a recorded independent review** (spec FR-013).
> Where no independent reviewer exists, acceptance requires a waiver in `governance/waivers.md`
> with an absolute expiry date. **This record carries no authority while `proposed`.**

## Context

Constitution IV: *"Retries MUST be bounded, observable, safe for the operation, and documented."*
PLAN §20 Phase 010 accepts the phase only when *"retries are bounded and observable"* and requires
*"retry/idempotency/backoff policies"* as a deliverable. PLAN §16.1 bounds *"retries, and
concurrency"*.

Five crates were evaluated (`package-decisions.md` §C). `backon` 1.6.0 (Apache-2.0, MSRV 1.85, no
advisory) is a pure iterator schedule and the best of them, and it arrives in the graph anyway
under the cache adapter's reconnect. It seeds its own `fastrand` from a `u64`. `backoff` is
unmaintained (RUSTSEC-2025-0012); `exponential-backoff`, `tokio-retry`, and `retry-policies` read a
thread RNG or a wall clock internally.

The kernel has **one** randomness site — `EntropySource` — and every identifier this project
generates comes from it (`run_id`, `Opaque`, `UserId`). Tests use `FixedEntropy`.

## Decision

1. **`renvor_core::retry::RetryPolicy`**: `max_attempts` (1–100), `initial_delay` (≥ 1 ms),
   `multiplier` (1.0–10.0), `max_delay` (≤ 1 h), `jitter` (`None`, `Full`, `Equal`), optional
   `deadline`. Construction validates every bound and refuses an unbounded policy with a closed
   error.
2. **The schedule is a pure function** `delay(policy, attempt, entropy_bytes) -> Duration`: no
   clock, no sleep, no runtime, no RNG of its own; jitter consumes bytes from the kernel's entropy
   port. `delay ≤ max_delay` for every input (property-tested); `FixedEntropy` makes it
   deterministic.
3. **`retry(policy, classify, operation)`** runs at most `max_attempts` times, sleeps through
   `tokio::time` (so paused time controls it), emits one structured event per retry (operation
   label, attempt, delay, closed reason), increments a metric where a recorder is present, and
   returns the **last** failure. It never fabricates success and never swallows an error.
4. **Retryability is a closed classification**: only `Unavailable` and `TimedOut` categories
   retry; `Refused`, `Denied`, `Rejected`, and validation failures never do, because retrying a
   refusal turns a bounded attack into unbounded work.
5. **Who retries**: the jobs worker schedules attempts with a `RetryPolicy`; the mail and storage
   adapters expose no implicit retry (an application composes `retry` visibly); the cache adapter
   never retries.
6. **Idempotency is explicit per capability and tested at the real boundary**: jobs by the
   `(queue, idempotency_key)` unique constraint on all four rows under a barrier race; cache by
   `set_if_absent` against a real server under a barrier race; storage `put` last-writer-wins
   (documented); mail **not idempotent** (documented; the durable-job path is the recommendation).

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **`backon`** | a second RNG (`fastrand`) seeded by a `u64` rather than bytes from the entropy port — two randomness sites in the kernel, the exact thing `run_id`'s single-site rule exists to prevent; the schedule is ~120 lines |
| **`backoff`** | unmaintained (RUSTSEC-2025-0012), which the advisory itself resolves by pointing at `backon` |
| **`tower::retry`** | a service-layer combinator that needs a `tower` dependency in the kernel and cannot express the closed classification without a Renvor policy type anyway |
| **Retry inside every adapter with its own policy** | five policies to bound, five to observe, and mail's non-idempotency would be hidden behind a "helpful" default |
| **An exact queue-depth bound under concurrency** | serialising every enqueue behind one lock for an operational bound; the contract states `≤ bound + writers − 1` instead (ADR-0032) |

## Consequences

- **`renvor-core` gains a `retry` module and a `clock` port** with no new dependency; the clock is
  `SystemTime`-based so the kernel stays free of `chrono`. `renvor_auth::Clock` (chrono-based)
  stays as it is this phase; two clock ports exist and unifying them is recorded as a limitation.
- **Every retry in the workspace is visible in telemetry** with an attempt number and a delay.
- **A caller who wants "retry until it works"** cannot express it: `max_attempts` caps at 100 and
  `deadline` is optional but every attempt is counted.
- **What would reverse this**: a superseding record adopting a crate that consumes the entropy
  port's bytes, which is the only property `backon` lacks.

## Compliance

- **Constitution IV** — bounded, observable, safe (closed retryability), documented.
- **Constitution VI** — bounded retries and concurrency; no amplification under refusal.
- **PLAN §16.1, §20 Phase 010** — the retry/idempotency/backoff deliverable and its acceptance line.
- **Contract C-O4** — one entropy source, extended to jitter.
