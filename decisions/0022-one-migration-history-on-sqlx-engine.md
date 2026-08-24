# ADR-0022: Run SeaORM projects' migrations on SQLx's engine, because `sea-orm-migration` has no checksum

| Field | Value |
|---|---|
| **ID** | 0022 |
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

`PLAN.md` §12 requires migrations to be *"ordered, checksummed, observable, and safe under
concurrent startup"*. The Phase 007 brief adds: *"Do not silently run two competing migration
histories."*

`sea-orm-migration` is the obvious choice for a SeaORM project, and constitution principle III
requires it to be evaluated before anything is written rather than after.

## The evaluation, and what it found

`sea-orm-migration` 2.0.2's bookkeeping table is, in full:

```rust
#[sea_orm(table_name = "seaql_migrations")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub version: String,
    pub applied_at: i64,
}
```

Two columns. **There is no checksum**, and the string `checksum` does not appear anywhere in the
crate. A migration whose body is edited after it has been applied is therefore undetectable: the
version is still recorded, the run reports success, and the schema silently disagrees with the
source that claims to describe it.

SQLx's `_sqlx_migrations` carries `checksum` and `success` and enforces both. Phase 006 already
built the wrapper around it — two deadlines, a lock Renvor owns, and a session that always ends —
and proved it with a real-database suite on four engines.

## Decision

**One migration history, on SQLx's engine, for both persistence models.**

`renvor-seaorm` calls `sqlx::migrate::Migrator` through its own runner, structurally identical to
`renvor-sqlx`'s and for the reasons ADR-0018 records, including `Migrator::run_direct`.

Generated projects get SQL-file migrations whichever ORM they selected, and `templates::MIGRATIONS`
is one group both paths include.

## Alternatives, and why each was rejected

**`sea-orm-migration`, accepting no checksum.** Rejected: it would withdraw a guarantee `PLAN.md`
states and Phase 006 shipped, for a project that chose the *other* ORM — so the four rows would stop
being comparable in the one dimension a schema mistake is unrecoverable.

**`sea-orm-migration` with a Renvor-built tamper gate on top.** Rejected under principle III in the
other direction: hashing migration bodies into a side table, keeping it consistent with someone
else's bookkeeping, and doing it under a lock is a migration engine. Renvor would own a second one.

**Both engines, whichever the project prefers.** Rejected explicitly — this is the "two competing
histories" case. A project that ran one and then the other would have `seaql_migrations` and
`_sqlx_migrations` each describing a partial truth, and no way to tell which was current.

**Share `renvor-sqlx`'s runner.** Rejected in ADR-0020: it would put a direct-SQLx crate into every
SeaORM application's graph.

## Consequences

**The cost, stated rather than absorbed.** Migrations are **SQL files**, not Rust `MigrationTrait`
implementations. A team that wants SeaORM's Rust-authored migrations — schema built from
`SchemaManager` calls, refactored with the compiler's help — does not get them here. That is a real
trade. It buys a tamper gate the alternative does not have at all, and it means switching ORM is not
a re-migration.

**Two wrappers around one engine.** `renvor-sqlx` and `renvor-seaorm` each wrap
`sqlx::migrate::Migrator`, so there are two places the properties can stop holding. Both have their
own real-database migration suite; neither is a copy for symmetry.

**`set_locking(false)` is needed twice.** Both engines' migration locks are re-entrant within one
session, so a `run_direct` that locked again on a connection Renvor had already locked would succeed
instantly and every real-database test would still pass. Phase 006 found that by mutation testing;
`renvor-seaorm` carries its own white-box test because it calls `run_direct` itself.

**Generated projects stay offline.** A SQL-file migration set needs no dependency. That matters more
than it looks: generation runs the staged project's own `cargo build` before placing it, so a
migration engine that had to be a *crate* would put a registry fetch inside `renvor new`.
