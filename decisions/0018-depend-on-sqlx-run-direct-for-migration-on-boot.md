# ADR-0018: Depend on `Migrator::run_direct`, a `#[doc(hidden)]` item, so that migrations can run on boot

| Field | Value |
|---|---|
| **ID** | 0018 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-013 |
| **Review date** | 2026-08-24 |
| **Superseded by** | *(not superseded)* |

> **This record is `accepted` under W-013. The review behind it was NOT independent.**
>
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded **independent**
> review before acceptance. **No independent human review of this record has occurred, and none is
> claimed.** Acceptance rests on **[W-013](../governance/waivers.md)**, a time-bounded written
> waiver granted on 2026-08-24, owned by Ahmed Anbar, expiring **2027-02-11** or **immediately**
> when a qualified independent human reviewer becomes available — whichever is first.
>
> **Automated review is advisory and does not satisfy the requirement.** W-013 covers this record,
> ADR-0016, ADR-0017 and ADR-0019 as one coupled Phase 006 decision; it authorises **nothing else**
> — not phase closure, which is W-014, and not any publication, tag, release or deployment.

## Context

`PLAN.md` §12 permits a deployment to declare `MigrationPolicy::OnBoot`, and FR-021 requires that
declaring it **applies** migrations before readiness is reported. Phase 006 shipped it as a
*recorded* intent that nothing performed: `migrates_on_boot()` answered `true` and the provider
migrated nothing.

That is the worst of the three possible behaviours. An operator reads a `true`, deploys, and gets a
running process against an un-migrated schema — with the framework's own manifest telling them the
migration ran.

The limitation recorded at the time (L-6) gave the reason as *"sqlx's migration future is not
`Send`"*. **That diagnosis was wrong**, and the wrongness mattered: it pointed at a property nobody
can change, so it read as unfixable.

### What the obstacle actually is

```rust
pub async fn run<'a, A>(&self, migrator: A) -> Result<(), MigrateError>
where A: Acquire<'a>, <A::Connection as Deref>::Target: Migrate,
```

`'a` appears **only inside a trait bound**. Coercing the resulting future into the kernel's
`ProviderFuture` — a `Pin<Box<dyn Future + Send + 'a>>` — erases the region, so the obligation must
be discharged universally. sqlx implements `impl<'c, DB> Acquire<'c> for &'c mut DB::Connection`,
which holds for one region at a time. The compiler says exactly that:

```text
error: implementation of `sqlx::Acquire` is not general enough
  = note: `sqlx::Acquire<'_>` would have to be implemented for the type
            `&'0 mut MySqlConnection`, for any lifetime `'0`...
  = note: ...but `sqlx::Acquire<'1>` is actually implemented for the type
            `&'1 mut MySqlConnection`, for some specific lifetime `'1`
```

Four probes are preserved in the phase evidence with verbatim output. The decisive one is probe 3:
**making the driver concrete does not help.** Probe 4 shows that boxing an `async fn` wrapper does
not help either — the obligation travels with the opaque type.

## Decision

Call `Migrator::run_direct`, which is `#[doc(hidden)]`, from exactly one place in the workspace.

```rust
// Getting around the annoying "implementation of `Acquire` is not general enough" error
#[doc(hidden)]
pub async fn run_direct<C>(&self, target: Option<i64>, conn: &mut C, skip: bool)
    -> Result<(), MigrateError>
where C: Migrate,
```

`C: Migrate` mentions no lifetime, so nothing is quantified over a region and the future boxes.

Two facts make this a smaller step than it looks. First, the comment above the declaration is
**upstream's own**, and it names this exact error — this is sqlx's escape hatch for the problem
sqlx has, not a hole found by poking at its internals. Second, `Migrator::run` *is* two lines:
`acquire()`, then `run_direct(None, &mut *conn, false)`. Renvor already holds the connection, so
calling `run_direct` with it is an exact behavioural equivalence.

## Alternatives, and why each was rejected

**Remove `Send` from `ProviderFuture`.** Rejected: it is the kernel's contract, every provider in
the workspace satisfies it, and relaxing a kernel invariant to accommodate one adapter inverts the
dependency rule the whole architecture is built on.

**Run migrations on a thread, a `LocalSet`, or `spawn_blocking`.** Rejected explicitly. The kernel
cannot observe such a task's failure, so a migration that failed would leave Boot reporting
success — which is the silent fallback principle IV prohibits, reintroduced to fix a problem
caused by reporting things that did not happen.

**Require the application to call a separate migration function before building the provider
graph.** This was the shipped behaviour, and it is what L-6 described. Rejected: it makes a
recorded policy a lie, and it relies on every application author remembering a step whose omission
is silent.

**Vendor the migration loop.** Rejected under constitution principle III. It is ~60 lines, and it
is ~60 lines of checksum comparison and dirty-state detection that must stay bit-compatible with
whatever wrote `_sqlx_migrations` — including a future sqlx that changes it.

**Fork or replace sqlx.** Disproportionate to a lifetime-inference limitation with a supported
workaround in the same file.

## Consequences

### The cost, stated rather than absorbed

`run_direct` is `#[doc(hidden)]` and therefore **semver-exempt**: sqlx may change or remove it in a
patch release without breaking its own promise. Three things bound that:

1. **One call site.** `crates/renvor-sqlx/src/migrate.rs`, inside the per-driver `runner!` macro.
   Every other migration path in the workspace goes through it.
2. **A compile guard.** `compile_guard` is never called and always type-checked. It asserts the
   item exists, takes `(Option<i64>, &mut PgConnection, bool)`, and returns a **`Send`** future —
   the last being the property the whole decision rests on. A signature that still accepted the
   call while meaning something else would be caught; so would a future that quietly stopped being
   `Send`.
3. **This record.** If the item disappears, the fallback is the rejected alternative that costs
   least: migrations move out of the provider graph and FR-021 reverts to a recorded limitation,
   with the manifest and `migrates_on_boot()` changed to say so **in the same commit**. Vendoring
   the loop is the second fallback and needs its own ADR.

### What else this decision forced

Owning the connection means owning the lock. `MigrationSettings::lock_timeout` existed, was
validated, and was **enforced by nothing**, while the module documentation claimed bounding it was
what the wrapper added — and both engines wait forever by default (`GET_LOCK(?, -1)` and
`pg_advisory_lock`). Renvor now takes the lock itself under that deadline and sets
`Migrator::set_locking(false)`.

That last line is not defensive tidiness and cannot be caught behaviourally: both engines' locks
are **re-entrant within one session**, so a `run_direct` that locked again on a connection Renvor
had already locked would succeed instantly and the entire real-database suite would still pass.
Mutation testing found it, and it is pinned by a white-box unit test.

A run that exceeds its deadline now reports `DeadlineExceeded` rather than `MigrationLockTimeout`.
The two mean opposite things on call — *another process is migrating* versus *your migration is too
slow* — and one kind for both sent an operator to the wrong place.

### What is not claimed

The migration session is ended on every path, including cancellation, via
`PoolConnection::close_on_drop`. That bounds cleanup; it does **not** make a cancelled migration
release its lock quickly. Measured: a 30s migration cancelled at 1.5s recovers after 28.5s on both
engines, because PostgreSQL processes a Terminate only after the current statement finishes and
sqlx issues no separate cancel request on drop. That is inherent, it is recorded in the test that
would otherwise appear to assert something stronger, and no code here claims otherwise.
