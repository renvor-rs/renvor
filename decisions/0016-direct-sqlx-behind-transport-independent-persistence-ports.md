# ADR-0016: Use SQLx directly, behind ports that name no driver

| Field | Value |
|---|---|
| **ID** | 0016 |
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
> W-013 covers this record, ADR-0017, ADR-0018 and ADR-0019 as one coupled Phase 006 decision. It
> authorises **nothing else** — not phase closure, which is W-014, and no publication or release.

## Context

`PLAN.md` §Phase 006 requires persistence for PostgreSQL and MySQL. Two questions had to be
answered together, because answering either alone constrains the other: **what talks to the
database**, and **what the rest of the framework sees**.

Constitution principle III forbids reimplementing what a maintained package already does.
Principle II requires dependencies to point inward. Principle VIII requires selecting one database
to resolve one driver.

## Decision

**Two crates, and the dependency points one way.**

`renvor-database` declares the ports — `Database`, `UnitOfWork`, `Executor`, `Keyset`,
`MigrationPolicy`, `DatabaseError` — and names no driver. `renvor-sqlx` implements them against
SQLx 0.9.0. Nothing in `renvor-database` depends on `renvor-sqlx`; `xtask` step 7 asserts it with a
control.

**SQLx directly, with no ORM.** Queries are written as SQL and every value is bound.

**Neither driver is a default feature.** `db-postgres` and `db-mysql` each resolve exactly one, and
there is deliberately no `all-databases` convenience feature.

## Alternatives, and why each was rejected

**An ORM (SeaORM, Diesel).** Rejected for this phase. An object mapper is a second schema
description that must agree with the migrations, and the failure mode is a query that compiles and
returns the wrong shape. `Orm` is an enum with one variant rather than a `bool` precisely so a
later phase can add one without turning a flag into a keyword.

**`sqlx::query!` macros.** Rejected. They require either a live `DATABASE_URL` at **compile** time
or a committed `.sqlx` cache. The first makes the build depend on a running server — including in
the offline generation path Phase 004 guarantees; the second is a generated artefact that goes
stale silently. `derive` alone gives `FromRow`, which is what is actually used.

**One crate.** Rejected: an application service that could reach a `sqlx::Pool` would make
principle II unenforceable, because there would be no boundary for `xtask` to check.

**`sqlx`'s `any` driver.** Rejected. It resolves the runtime-dispatch layer and both drivers,
defeating feature isolation, and it flattens the type differences the ports exist to keep visible.

## Consequences

**One TLS feature was chosen against the obvious name.** `tls-rustls` aliases to
`tls-rustls-ring-webpki`, which resolves `webpki-roots` — **CDLA-Permissive-2.0**, which is not on
`deny.toml`'s allow-list. The obvious feature name fails this repository's own licence gate.
`tls-rustls-ring-native-roots` uses the platform certificate store, keeps the same `rustls` and
`ring`, and passes. `mysql-rsa` is absent for the same class of reason: it resolves `rsa`, carrying
RUSTSEC-2023-0071 with `patched = []`. The consequence is stated in `connect_mysql`'s
documentation rather than discovered: without RSA key exchange, MySQL's `caching_sha2_password`
cannot complete a **first** authentication over a plaintext channel.

**Sorting is an allowlist, not an escape.** A column name cannot be a bound parameter, so the only
safe construction is one where every possible value is written in the source. An unknown sort field
maps to no column and is **refused** rather than silently replaced with a default.

**Keyset pagination refuses mixed directions.** Row-value comparison `(a, b) > ($1, $2)` is
evaluated identically by both engines and cannot express `a ASC, b DESC`. The nested-`OR` form can;
it is not implemented, and the refusal is explicit rather than a wrong page. Recorded as L-7.

**Errors carry one field.** `DatabaseError` holds a kind and nothing else, so a driver message
cannot survive translation — structurally, not by filtering.

**Feature isolation is measured, with positive controls.** Under `db-postgres`: zero `sqlx-mysql`,
one `sqlx-postgres`. Under `db-mysql`: the reverse. Under neither: zero of both. Zero
`webpki-roots` and zero `rsa` across the workspace with `--all-features`. A count of zero proves
nothing without a control that proves the walk can see what is there.
