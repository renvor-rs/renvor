# ADR-0032: Store durable jobs in the application's own selected database row

| Field | Value |
|---|---|
| **ID** | 0032 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-021. **Not independent** |
| **Review date** | 2026-09-04 |
| **Superseded by** | *(not superseded)* |

> **`accepted` under [W-021](../governance/waivers.md), and the review behind it was NOT
> independent.** No independent human review of this record has occurred, and none is claimed.
> The maintainer authored it and took every measurement it rests on; automated and maintainer
> reviews are **advisory**, never independent.
>
> W-021 covers **ADR-0031 through ADR-0037 as one coupled cluster** — each depends on a boundary
> another draws, so reviewing one alone would review a fragment — and it authorises nothing else.
> It does **not** close Phase 010; [W-022](../governance/waivers.md) is a separate exception on a
> separate axis.
>
> Accepted **2026-09-04** against head `5f26334b394f20ae86b3037ccb77a23705c40ed9`,
> tree `47aeb8d8fda9e07bd5a4520406cef4eada44273c`. W-021 expires **2027-02-11**, or
> immediately when a qualified independent human reviewer becomes available — whichever
> is first.

## Context

PLAN.md §21 item 13 names the decision this record makes: *"Durable job storage without a hidden
database dependency."* §10.1 fixes its constraint: *"A MySQL project must not depend on PostgreSQL
for jobs or any other optional capability."* §12 requires migrations to be *"ordered, checksummed,
observable, and safe under concurrent startup"*, and ADR-0022 already runs both adapters' migrations
on SQLx's engine, with ADR-0025 establishing per-engine migration sets.

Every released third-party job system was **measured against this workspace's resolved graph**
(`specs/010-…/package-decisions.md` §B). None can be used:

| Candidate | What the graph showed |
|---|---|
| `apalis` + `apalis-sql` 0.7.4 | **+42 packages**, including `rsa 0.9.10` (banned, RUSTSEC-2023-0071), a second `sqlx-core`/`sqlx-postgres`/`sqlx-mysql` at **0.8.6** beside 0.9.0, and `webpki-roots` in two versions |
| `apalis-postgres`/`apalis-mysql` 1.0.0-rc.8 | release candidates binding `sqlx ^0.8.1` through a webpki TLS feature; `main` unreleased pending an open sqlx bug |
| `sqlxmq` 0.6.0 | PostgreSQL only, `sqlx ^0.8`, no `SKIP LOCKED`, no dead-letter state |
| `underway` 0.2.0 | PostgreSQL only, hard-coded `runtime-tokio-rustls` (webpki) |
| `apalis-redis` | binds `redis ^0.32` beside 1.6 and puts a broker in every application |
| `background-jobs` | AGPL-3.0 |
| `faktory` | an external Go server — the hidden dependency item 13 forbids |

None knows SeaORM; all want a raw `sqlx::Pool`.

## Decision

1. **`renvor-jobs` defines the port** — `JobStore` (enqueue, claim, complete, fail, release,
   dead-letter, read) and the value types — and **names no driver**. Step 7 asserts it.
2. **The durable store is implemented in `renvor-sqlx` and `renvor-seaorm`**, behind a `jobs`
   feature on each, for PostgreSQL and MySQL, exactly as the authentication repositories are. A
   MySQL application on either programming model therefore resolves no PostgreSQL crate by choosing
   jobs, and step 7 asserts that row.
3. **One table, `rv_job`**, in per-engine migration sets under `crates/renvor-jobs/migrations/`,
   one statement per file (ADR-0025's rule). Its shape is in the phase data model: 16-byte
   entropy identifiers, bounded `queue`/`kind`/`payload`, an optional `idempotency_key` under
   `UNIQUE (queue, idempotency_key)`, a closed `state`, `attempts`/`max_attempts`, `run_at`, a lease
   token and expiry, a closed `last_failure`, and a bounded `trace_parent`.
4. **Claiming is one transaction**: `SELECT … WHERE queue = ? AND state = ready AND run_at <= ?
   ORDER BY run_at, id LIMIT 1 FOR UPDATE SKIP LOCKED`, then `UPDATE … SET state = leased,
   attempts = attempts + 1, lease_token = ?, lease_expires_at = ? WHERE id = ? AND state = ready`.
   The re-check on `state` means an engine whose lock semantics surprise cannot double-claim;
   `rows_affected = 0` is "lost" and rolls back. Both engines support `SKIP LOCKED` (PostgreSQL
   ≥ 9.5, MySQL ≥ 8.0), so the statement is portable and there is no per-engine branch.
5. **A claim is a lease** with a bounded duration; an expired lease is reclaimed by a bounded sweep
   at the next claim, with the attempt counted and `last_failure = LeaseExpired`. A crashed worker
   loses at most one attempt's work.
6. **Idempotency is the database's decision**: the unique constraint, never a check-then-insert
   (`contracts/database-portability.md` §3 — the engines' default isolation levels differ). The
   loser receives `Enqueued::Duplicate(existing)`.
7. **Every bound is enforced before the row**: payload ≤ 1 MiB (default 64 KiB), queue depth per
   queue (default 100 000) counted inside the insert transaction, `max_attempts` 1–100, lease
   ≤ 1 h, handler timeout ≤ 24 h, worker concurrency ≤ 1024.
8. **The worker is generic over the store** and runs the memory substitute and the four rows
   through the **same shared contract** in `renvor_testkit::jobs`; the census requires the four
   rows.
9. **Exit strategy**: re-evaluate `apalis-postgres`/`apalis-mysql` when a release binds `sqlx
   ^0.9`, can be built without any webpki feature, and SeaORM parity is either supported or no
   longer required. The port is narrow enough that an adapter over apalis could implement it without
   changing application code.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **`apalis` family** | the measured graph above: banned `rsa`, a duplicate driver stack, `webpki-roots`; release candidates only on the 0.9 line |
| **A Valkey/Redis-backed queue** | a broker in every application that wants a background task, and durability that is *"differently durable"* from the database the application already trusts (constitution III) |
| **An embedded file or SQLite queue** | a third datastore with its own backup, its own failure modes, and no place in the four-row matrix |
| **Polling without `SKIP LOCKED` (optimistic `UPDATE … WHERE state = ready`)** | correct but serialises every worker on the same row lock under contention; `SKIP LOCKED` is supported by both engines Renvor claims, so the cost buys nothing |
| **`LISTEN/NOTIFY` wake-ups** | PostgreSQL only; a MySQL row would need a different mechanism, and §10.1 forbids the divergence. Bounded polling is portable |
| **Serialising enqueues to make the depth bound exact** | one row lock in front of every enqueue for a bound that is operational, not a security invariant; the contract states the exact guarantee (`≤ bound + writers − 1`) instead |

## Consequences

- **Four new census rows** (63 → 67); removing one fails step 4 by mutation-proven construction.
- **Two adapters gain an optional `renvor-jobs` edge**, which moves `renvor-jobs` ahead of them in
  the publication order.
- **Bounded polling latency**: a job scheduled for "now" waits up to one poll interval. The
  interval is configuration with a floor and a cap.
- **The depth bound is approximate under concurrency** by at most the number of concurrent
  writers minus one — stated in the contract rather than claimed atomic.
- **What would reverse this**: a superseding record selecting a third-party store once the exit
  conditions in decision 9 hold.

## Compliance

- **Constitution III** — custom infrastructure with an accepted ADR documenting evaluated packages,
  their concrete shortcomings (measured), ownership cost (one table, two adapter modules, one shared
  contract), and an exit strategy (decision 9).
- **Constitution V** — the four rows pass a shared contract suite.
- **Constitution VI** — bounded queues, payloads, retries, and concurrency; parameter binding
  throughout; identifiers from the entropy port.
- **PLAN §10.1, §12, §21 item 13**.
- **ADR-0022, ADR-0025** — migrations on the SQLx engine, per-engine sets.
