# ADR-0017: Own the pooled connection in the unit of work, so cancellation cannot shrink the pool

| Field | Value |
|---|---|
| **ID** | 0017 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-013 |
| **Review date** | 2026-08-24 |
| **Superseded by** | *(not superseded)* |

> **This record is `accepted` under W-013. The review behind it was NOT independent.**
>
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded **independent**
> review before acceptance. **No independent human review of this record has occurred, and none is
> claimed.** Acceptance rests on **[W-013](../governance/waivers.md)**, expiring **2027-02-11** or
> immediately when a qualified independent human reviewer becomes available — whichever is first.
> W-013 covers this record, ADR-0016, ADR-0018 and ADR-0019 as one coupled Phase 006 decision.

## Context

A request that is cancelled mid-transaction — a client disconnect, a shutdown signal, a deadline —
drops the future. FR-004 and FR-005 require that this commits nothing **and** that the pool
recovers its full configured capacity.

Using `sqlx::Transaction` directly, it did not. The pool permanently lost a slot per cancellation,
and enough cancellations exhausted it.

### The cause, traced rather than guessed

```
future dropped mid-statement (response still in flight)
  └─ Transaction::drop → start_rollback
       ├─ pushes an EXTRA Waiting::Result
       └─ resets stream.sequence_id = 0 mid-conversation
  └─ PoolConnection::drop → spawn(return_to_pool)
       └─ ping() → wait_until_ready() → recv_packet().await   [UNBOUNDED]
            └─ hangs forever holding DecrementSizeGuard
                 → Pool::size() never decrements → no replacement → slot lost permanently
```

sqlx's own source calls the queued rollback *"a band-aid as SQLx-next connections should be able to
recover from cancellations"*. The close-on-drop path **is** bounded at five seconds; the ordinary
return path is not.

Measured on pure SQLx with no Renvor code: PostgreSQL 0 leaks in 12 trials, MySQL 8.4.11 two,
MySQL 9.7.2 six. **There is no 9.7-specific regression** — an earlier claim that there was rested
on a probe with a fixed 200µs deadline landing at three different points, and it was withdrawn.
PostgreSQL survives more often because it resynchronises at `ReadyForQuery`; the MySQL wire
protocol has no resynchronisation marker, so a client must track packet counts exactly. The same
race is survivable on one engine and fatal on the other.

## Decision

`SqlxUnitOfWork` owns the `PoolConnection` instead of wrapping `sqlx::Transaction`.

- `begin` acquires a connection directly and issues `BEGIN` through the **text** protocol
  (`Executor::execute` with a bare string). `sqlx::query` would prepare it, and MySQL's
  prepared-statement protocol does not accept `BEGIN`.
- `commit(self)` and `rollback(self)` consume the value, so post-commit use is a compile error.
  Both return a **reusable** connection to the pool on success.
- Any failure, and `Drop`, call `PoolConnection::detach()` — which frees the pool slot
  **synchronously**, so the slot is released whether or not the socket ever recovers.

## Alternatives, and why each was rejected

**Wait for an upstream fix.** Rejected: it is unbounded in time, and the defect exhausts a
production pool.

**Close every connection after every transaction.** Rejected explicitly. It would make the
symptom disappear while turning a per-request connection reuse into a per-request reconnect — a
performance regression disguised as a fix, and precisely the "unnoticed fallback" the phase brief
prohibits. Two tests exist to keep it rejected: normal commit and explicit rollback both assert a
healthy connection is reused.

**A background reaper that replaces lost slots.** Rejected under principle VI: unbounded orphaned
work, and it treats a permanent leak as something to be swept up rather than not created.

**A self-referential struct holding both the pool and a borrowed transaction.** Rejected: it needs
either unsafe code or a self-referential crate, and the phase forbids both.

## Consequences

- Nesting is unrepresentable, because `UnitOfWork` has no `begin`. A second `begin` on the database
  yields a **separate** session, and the test asserts both that the sessions differ and that the
  outer write is invisible to the inner one.
- `detach()` is the load-bearing call, not `close()`. Freeing the slot synchronously is what makes
  capacity recovery deterministic rather than dependent on a socket that may never answer.
- A failed **rollback** discards the connection rather than returning it: a rollback that did not
  complete leaves the session's state unknown, and an unknown session in a pool is worse than a
  missing one.
- Verified across all four engines: 4 red before the fix, 0 after; PostgreSQL 17.11 + MySQL 8.4.11
  and PostgreSQL 18.6 + MySQL 9.7.2 both green, and mutation-tested in both directions.

### What is not claimed

This bounds Renvor's own pool accounting. It does not make the **server** abandon a statement that
is already running: a cancelled client cannot do that without a separate cancel request, which
sqlx does not issue on drop. A connection cancelled mid-statement stays pinned server-side for the
remainder of that statement — measured at 9.61s (PostgreSQL) and 9.64s (MySQL) for a ten-second
statement, which is correct behaviour rather than a leak, and is recorded so nobody reads the
capacity guarantee as a stronger claim than it is.
