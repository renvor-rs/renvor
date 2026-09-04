---
description: "Contract — the durable job store: transitions, the claim statement, leases and reclaim, the depth bound, the worker's bounds, and what is emitted"
version: "1.2.0"
status: "unstable — the surface it describes is explicitly unstable (pre-release, `0.0.0`); this version identifies the contract text, not a stability promise. 1.2.0 (2026-09-04, the L-16 correction): at the stop grace the handler's own task is aborted and joined under `ABORT_JOIN_TIMEOUT` before its lease is released; a handler whose task does not terminate keeps its lease (`release_withheld`) and the provider's stop reports it. 1.1.0 (2026-09-04, the Phase 010 correction round): the depth bound is enforced under concurrency by a per-queue lock row (`rv_job_queue`, migration 5) so the guarantee is `depth ≤ bound` rather than `depth ≤ bound + writers − 1`; the shared contract gains an eight-racer depth race (17 assertions); the worker proves its store answers at Boot; lease releases at Stop are bounded, counted, and reported through the provider's stop error"
---

# Contract: Durable Jobs

**Feature**: Phase 010 | **Satisfies**: FR-022…FR-042, FR-076, FR-083, FR-089…FR-095 | **Decision**:
`decisions/0032` (jobs live in the application's own database), `decisions/0037` (retry policy)

One shared contract — `renvor_testkit::jobs`, **17 assertions** including two four-racer barrier
races and one eight-racer depth race — runs against the memory substitute and all four
persistence rows (PostgreSQL and MySQL through `renvor-sqlx` and `renvor-seaorm`). The census in
`xtask` step 4 requires each row to report in.

## C-J1 — States and transitions

`ready → leased → completed | ready (reschedule) | dead`. Codes stored: `0 ready`, `1 leased`,
`2 completed`, `3 dead`. Failure kinds stored: `1 handler_failed`, `2 timed_out`, `3 panicked`,
`4 lease_expired`, `5 abandoned`. Every transition is validated against the **lease token**
(FR-039): `complete`, `fail`, and `release` name a token, and a token that is not held is
`LeaseNotHeld` — released, reclaimed, or never issued look the same.

## C-J2 — Identifiers come from the entropy port

A job identifier and a lease token are sixteen bytes from `EntropySource` — never a sequence
(which encodes throughput) and never a clock (which encodes when). `JobId` renders as 32 lowercase
hex characters; a lease token's `Debug` prints nothing but its width.

## C-J3 — The claim

In one transaction on the rows: a bounded reclaim of expired leases (**as its own autocommit
statement before the transaction** — inside it, InnoDB's gap locks on an empty range deadlocked
four concurrent claimers, measured), then

```sql
SELECT … FROM rv_job WHERE queue = ? AND state = 0 AND run_at <= ?
ORDER BY run_at, id LIMIT 1 FOR UPDATE SKIP LOCKED
```

then `UPDATE rv_job SET state = 1, attempts = attempts + 1, lease_token = ?, lease_expires_at = ?
… WHERE id = ? AND state = 0`. The `UPDATE` re-checks the state so a surprising lock semantics
cannot double-claim; `rows_affected = 0` rolls back and answers "nothing claimable now". The
memory substitute makes the same transition under one lock. Four racers claim one ready job:
**exactly one** succeeds, on every store.

## C-J4 — Leases and reclaim

A claim holds a lease (default 60 s, cap 1 h). A lease that expires is reclaimed at the next
claim, in batches of at most 100, with `lease_expired` recorded and the attempt already counted.
**At the last attempt an expired lease dead-letters** instead of returning to ready, so a handler
that outlives its lease every time is bounded like any other failure (FR-092). `complete` is
idempotent: the token stays on a completed row, so a second `complete` is `AlreadyCompleted`,
not an error. `release` returns a leased job to ready at `now` and keeps the attempt counted.

## C-J5 — Idempotency and depth

`UNIQUE (queue, idempotency_key)`: concurrent enqueues with one key store **one** row and every
caller learns the same identifier (a unique violation under the race resolves to the existing
id). NULL keys never collide. The queue depth (ready + leased) is counted **under a per-queue
row lock**: every enqueue upserts the queue's row in `rv_job_queue` as its own autocommit
statement, then takes `SELECT queue FROM rv_job_queue WHERE queue = ? FOR UPDATE` as the
**first** statement of its transaction, then reads the key, counts, and inserts. Enqueues on one
queue therefore serialise, the count is taken after every earlier writer committed, and the
guarantee is `depth ≤ bound` on both engines — proven by an eight-racer barrier race against a
bound of 3, three rounds, half the racers keyed. The lock is the first statement on purpose:
under InnoDB's REPEATABLE READ a consistent read taken before it pins a snapshot that predates
the previous holder's commit, and a count from that snapshot admits one job too many (measured:
with the lock after the key read, MySQL held 5–6 against 3 in three of three runs while
PostgreSQL passed). 1.0.0 stated `depth ≤ bound + writers − 1`; that was the defect, not a
property.

## C-J6 — Bounds

Queue name and kind `[a-z0-9_.-]{1,64}`; idempotency key ≤ 128 bytes, no control character;
payload ≤ configured ceiling (default 64 KiB, cap 1 MiB) at construction **and on read**;
`max_attempts` 1…100 (default 5); depth bound configurable (default 100 000). A payload never
reaches `Debug`, `Display`, a log, or a span; a job's `Debug` prints identity, queue, kind,
state, and attempt.

## C-J7 — The worker

Concurrency 1…1024 (default 4); poll 10 ms…60 s; handler timeout 1 s…24 h (default 5 min); stop
grace ≤ 25 s. Every running job holds a kernel `WorkPermit`, so `Drain` sees it (FR-032). A
handler runs in its own task under a child cancel scope: a panic is contained as `panicked` and
its payload reaches no row or record; a timeout aborts the task as `timed_out`. Retries follow
the kernel's `RetryPolicy` (`delay(attempt)` sets the next `run_at`); `HandlerError::Abandon`
dead-letters at once. **Boot proves the store answers**: before the loop is spawned or any
readiness contributor registered, the provider reads the queue's depth under
`STORE_PROBE_TIMEOUT` (10 s) and fails Boot with `WorkerBootError::StoreNotAnswering(category)`
when the store refuses or does not answer — bad credentials, a missing schema, a permission, or
no route fail at Boot, not as an endless warning loop. At stop, a job still running past the
grace is handled in a fixed order: its scope is cancelled, **the handler's own task is aborted**
(never only the wrapper task around it, whose abort would drop the handle and leave the handler
running with no owner), that task is **joined** under `ABORT_JOIN_TIMEOUT` (2 s) — its
`JoinHandle` resolves only once the future has been dropped — and only then is its lease
released. A handler that holds its thread inside a poll cannot be dropped by anything and does
not terminate within the bound; its lease is **kept**, to expire and be reclaimed with the
attempt counted (C-J4) as if the worker had died, and counted as `release_withheld` — a release
under a handler that may still be running would hand the job to a second execution while the
first is alive. The same abort-then-join order applies at the handler timeout, before the
attempt is recorded. The releases run **concurrently, each under `RELEASE_TIMEOUT` (2 s)**, so
N leases against a silent store cost one bound; `MAX_STOP_GRACE + ABORT_JOIN_TIMEOUT +
RELEASE_TIMEOUT ≤ 30 s` (the kernel's provider deadline) is pinned at compile time. A release
the store refuses or does not answer is counted (`WorkerReport::{released, release_failed,
release_timed_out, release_withheld}`), never marks the lease released, and is reported by the
provider's Stop as `WorkerBootError::LeasesNotReleased { failed, timed_out, withheld }` — an
unclean stop the kernel records, never a swallowed one (FR-012). A job claimed as shutdown
begins is given back the same bounded, counted way and the loop ends — it does not spin
re-claiming with a closed gate.

## C-J8 — Trace context

An enqueued job carries the current W3C trace context (rendered from validated fields only); the
execution span `renvor.job` records `trace_id`, `parent_span_id`, `trace_flags` from it.

## C-J9 — What is emitted

Per attempt, one event on `renvor.jobs`: `job_id`, `queue`, `kind`, `attempt`, `max_attempts`,
`outcome` (`completed`, `retried`, `dead_lettered`), `failure` (the closed kind), `next_run_at_unix_ms`
(absent when none), `duration_ms`. Counters `renvor_jobs_{enqueued,claimed,released}_total{queue}`
(`released` counts **confirmed** releases only), `renvor_jobs_attempts_total{queue,kind,outcome}`,
`renvor_jobs_store_errors_total{queue,category}` (a refused release by its category, a release
that did not answer as `timed_out`), and the histogram `renvor_jobs_duration_seconds{queue,kind}`
(the code registered `renvor_job_duration_seconds` until the correction round; the contract's
name is the family's and is now the code's). Also on `renvor.jobs`: one `info` event when the
store answers the Boot probe (`queue`, `depth`), one `warn` per lease not released at stop
(`queue`, `category`), one `warn` per lease withheld at stop under a handler that did not
terminate (`queue`), one `warn` when a timed-out handler does not terminate after its abort
(`job_id`, `queue`, `kind`), and the `worker stopped` event with `claimed`, `aborted`,
`released`, `release_failed`, `release_timed_out`, `release_withheld`.

## C-J10 — The migration set

`crates/renvor-jobs/migrations/{postgres,mysql}`: five up/down pairs, one statement per file
(`database-portability.md` §7), versions `20260904000001…5` — the fifth creates `rv_job_queue`,
the per-queue lock row C-J5 takes. An application has **one** migration
set and one ledger: copy the engine's files beside your own; a second `Migrations::load` at this
directory is refused by SQLx's ledger check.

## Where this is enforced

- `renvor_testkit::jobs` on `MemoryJobStore` and the four rows (`tests/jobs.rs` in each adapter);
- `renvor-jobs` unit tests (exact retry counts, panic containment, timeout, shutdown release, the
  claim/permit window, the Boot probe against a refusing and a hanging store, bounded and counted
  releases, the provider's stop error) and `tests/worker_events.rs` (the subscriber's view);
- `xtask` step 4 census rows `jobs::{postgres,mysql}::the_shared_jobs_contract_holds` × 2 adapters.
