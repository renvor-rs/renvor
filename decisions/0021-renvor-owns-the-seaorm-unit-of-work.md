# ADR-0021: Own the pooled connection in the SeaORM unit of work, rather than wrap `DatabaseTransaction`

| Field | Value |
|---|---|
| **ID** | 0021 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-015 |
| **Review date** | 2026-08-24 |
| **Superseded by** | *(not superseded)* |

> **This record is `accepted` under W-015. The review behind it was NOT independent.**
>
> No independent human review of this record has occurred, and none is claimed. Acceptance rests on
> **[W-015](../governance/waivers.md)**, expiring **2027-02-11** or immediately when a qualified
> independent human reviewer becomes available — whichever is first. W-015 covers ADR-0020,
> ADR-0021 and ADR-0022 as one coupled Phase 007 decision, and authorises nothing else.

## Context

Phase 006 established, in ADR-0017, what a Renvor unit of work must guarantee: cancellation commits
nothing, the pool regains its **full configured capacity within a bounded, deterministic time**, a
committed or explicitly rolled-back transaction leaves a **reusable** connection, and cleanup is
bounded so shutdown cannot hang.

The obvious Phase 007 implementation wraps `sea_orm::DatabaseTransaction`. It cannot meet that
contract, and the reason is structural rather than a bug to wait out.

### What the upstream source says

```rust
pub struct DatabaseTransaction {
    conn: Arc<Mutex<InnerConnection>>,
    ..
}

impl Drop for DatabaseTransaction {
    fn drop(&mut self) {
        self.start_rollback().expect("Fail to rollback transaction");
    }
}

fn start_rollback(&mut self) -> Result<(), DbErr> {
    if self.open {
        if let Some(mut conn) = self.conn.try_lock() { .. }
        else {
            //this should never happen
            return Err(conn_err("Dropping a locked Transaction"));
        }
    }
    Ok(())
}
```
— `sea-orm-2.0.2/src/database/transaction.rs`, verbatim, upstream's comment included.

Because the connection is behind an `Arc`, `Drop` has no `&mut` and can only `try_lock`. A
failed try-lock has nowhere to go but `expect`, and **a panic in `Drop` during an unwind aborts
the process.**

It then hands the connection to SQLx's queued-rollback path — the one ADR-0017 traced to an
unbounded `ping()` holding the pool's size guard.

### What was measured

```
NATIVE sea_orm::DatabaseTransaction on mysql:    capacity STRANDED for 9.506036542s
NATIVE sea_orm::DatabaseTransaction on postgres: capacity STRANDED for 9.509529791s
```

Renvor's bound on the same pool, same cancellation, is **2 s**, and it holds on both engines.

**Stated precisely: this is stranding, not leaking.** The slot came back once the abandoned
ten-second sleep finished server-side. An unqualified "the native path leaks" would contradict
ADR-0017, which measured the *permanent* form at 0-in-12 on PostgreSQL. It is a contract failure
regardless — ~9.5 s of denial per cancelled request, scaling with whatever the abandoned statement
was doing, is neither bounded by anything Renvor configures nor deterministic.

## Decision

`SeaOrmUnitOfWork` owns the `PoolConnection` directly and implements
`sea_orm::ConnectionTrait` itself.

- The connection lives in `tokio::sync::Mutex<Option<PoolConnection<DB>>>` — a `Mutex` because
  `ConnectionTrait` takes `&self`, and **not** behind an `Arc`.
- `commit(self)` and `rollback(self)` consume the value and return a **reusable** connection.
- `Drop` reaches the connection with `Mutex::get_mut()`, which takes the `&mut self` `Drop`
  already has, **cannot fail**, and has no error path to `expect` on. It then `detach`es, which
  frees the pool slot synchronously.

That single difference — unique ownership instead of shared — is what removes the panic. It is not
a defensive workaround; it is the property that makes the failure unrepresentable.

Applications still write idiomatic SeaORM, because every SeaORM query method takes
`&impl ConnectionTrait`:

```rust
let uow = database.begin().await?;
let posts = post::Entity::find().all(&uow).await?;
uow.commit().await?;
```

## Why this was reachable at all

Three properties of SeaORM's public API, each checked rather than assumed:

1. **`ConnectionTrait` is not sealed.** Five required methods, all others defaulted.
2. **It is declared `#[async_trait::async_trait]`**, so its futures are boxed **with a `Send`
   bound** — by construction. This is the exact property Phase 006's L-6 turned on, and
   `version.rs` guards it so a migration to native `async fn` in trait fails loudly here rather
   than as a lifetime error deep in the provider.
3. **`From<PgRow> for QueryResult` and `From<PgQueryResult> for ExecResult` are public impls on
   public types.** Without them a row could not be built outside SeaORM and this decision would have
   been impossible.

Value binding uses **`sea-query-sqlx`**, the published crate SeaORM itself binds through
(`SqlxValues`). It is a dependency rather than a reimplementation under principle III: hand-writing
the mapping for every column type is far more code than the ~60-line vendoring ADR-0018 rejected, and
it would have to stay bit-compatible with whatever SeaORM binds. Unlike ADR-0018's `run_direct`,
this is **not** doc-hidden — it is a normal published API, so the coupling is weaker than the one
Phase 006 already accepted. `version.rs` carries a guard naming the coupling, so a two-`sea-query`
resolution fails with a message about the version rather than *"expected `Values`, found `Values`"*.

## Alternatives, and why each was rejected

**Wrap `DatabaseTransaction` and accept the behaviour.** Rejected: it would reclassify a met
Phase 006 guarantee as a limitation, which the phase brief forbids and which would make "the four
rows pass the same contracts" false.

**`ManuallyDrop` around it, to skip SeaORM's `Drop`.** Rejected: the `PoolConnection` inside is
then never returned and never detached, so the slot is lost **permanently** — strictly worse than the
problem.

**Build a `DatabaseConnection` per transaction from a one-connection pool.** Rejected: a fresh TCP
connection per transaction, which is a performance regression disguised as a fix.

**Wait for upstream.** Rejected: unbounded in time, and the defect denies capacity in production.

## Consequences

- SeaORM's `TransactionTrait`, savepoints, and isolation-level configuration are **not** exposed.
  Recorded as a limitation with a target phase rather than half-shipped.
- `execute_unprepared` goes through `sqlx::raw_sql(AssertSqlSafe(..))`. SQLx 0.9 accepts a bare
  string only when it is `&'static str`, precisely so caller-supplied SQL must be marked — which
  makes the escape hatch's last rung visible in the source rather than in a convention.
- The native transaction's behaviour is kept as a **measured** test rather than an asserted one.
  Asserting "the native path fails" would be flaky on PostgreSQL by construction, and `PLAN.md`
  §17 treats a flaky test as a defect. The load-bearing assertion is Renvor's own path recovering
  inside its bound, every round.

### What is not claimed

This bounds Renvor's pool accounting. It does not make the **server** abandon a statement already
running: a cancelled client cannot do that without a separate cancel request, which SQLx does not
issue on drop. A connection cancelled mid-statement stays pinned server-side for the remainder of
that statement — which is correct behaviour, and is recorded so the capacity guarantee is not read as
a stronger claim than it is.
