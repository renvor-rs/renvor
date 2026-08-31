# ADR-0025: One auth migration set per engine, not one portable set

| Field | Value |
|---|---|
| **ID** | 0025 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-019. **Not independent** |
| **Review date** | 2026-08-31 |
| **Superseded by** | *(not superseded)* |

> **`accepted` under [W-019](../governance/waivers.md), and the review behind it was NOT
> independent.** No independent human review of this record has occurred, and none is claimed.
> The maintainer authored it and took every measurement it rests on; automated and maintainer
> reviews are **advisory**, never independent.
>
> W-019 covers **ADR-0024 through ADR-0030 as one coupled cluster** — each depends on a boundary
> another draws, so reviewing one alone would review a fragment — and it authorises nothing else.
> It does **not** close Phase 009; [W-020](../governance/waivers.md) is a separate exception on a
> separate axis.
>
> Accepted **2026-08-31** against head `0090c6784acdbfac863fc966e449245201a2b1fd`,
> tree `dd1b27e32f79a41efaaaa6abc2e4d477262f326d`. W-019 expires **2027-02-11**, or
> immediately when a qualified independent human reviewer becomes available — whichever
> is first.

## Context

Phase 009 adds seven tables that must exist on **all four rows** — direct SQLx and SeaORM, each on
PostgreSQL and MySQL. Every previous migration in this repository has been portable across both
engines in a single file, because the only columns needed so far were `BIGINT` and `VARCHAR`:

```sql
CREATE TABLE rv_widget (id BIGINT PRIMARY KEY, name VARCHAR(100) NOT NULL);
```

Authentication needs two column shapes that have **no portable spelling**, and
`contracts/database-portability.md` is the record of why:

| Need | PostgreSQL | MySQL | Portable? |
|---|---|---|---|
| an instant, microsecond precision, valid past 2038 | `TIMESTAMPTZ` | `DATETIME(6)` | **no** |
| 16 opaque bytes | `BYTEA` | `BINARY(16)` | **no** |

The contract's §2 already writes the first as a slash — *"`TIMESTAMP(6)` / `DATETIME(6)`"* — which
is the two-spelling problem stated without being resolved. §2 also rules out the one spelling both
engines accept: MySQL's `TIMESTAMP` **ends at 2038-01-19**, and `expires_at` is precisely a column
that may hold a later instant.

## Decision

**Two migration directories, `migrations/postgres` and `migrations/mysql`**, selected by the caller.

This requires **no framework change**. `Migrations::load(directory, settings)` already takes the
directory as a parameter, and both adapters migrate on SQLx's engine (ADR-0022), so one *pair* of
directories serves all four rows — the split is on the **engine** axis, which is where the
difference actually is, not on the adapter axis, where it is not.

### Every migration contains exactly one schema statement

`contracts/database-portability.md` §7, normative:

> *"A portable migration **must contain exactly one schema statement**. On MySQL that is the only way
> to guarantee it has no partial state to be in."*

This is not a style rule. MySQL forces an implicit commit on DDL, so a two-statement file that fails
halfway leaves the first statement committed, SQLx records the version dirty, and **every later run
is refused outright** — §7's 2026-08-27 correction measured that there is no "run the rest" recovery
and that no SQLx command clears the dirty row. Seven tables plus their indexes therefore become many
small migrations rather than a few convenient ones.

## Alternatives rejected

| Alternative | Why not |
|---|---|
| one portable set using `TIMESTAMP(6)` | MySQL's `TIMESTAMP` ends **2038-01-19**; `expires_at` outlives it |
| one portable set storing instants as `BIGINT` epoch microseconds | portable and 2038-safe, but makes every timestamp unreadable in a console, unusable in a `WHERE` against a date literal, and pushes conversion into every one of the four adapters. The cost lands on every query to avoid a difference that lands on one file per engine |
| identifiers as `CHAR(32)` hex | genuinely portable, and rejected on cost: it doubles the key width of every primary key and every foreign key in the schema, on both engines, to avoid two lines of DDL |
| runtime `if kind == Postgres` inside one file | migrations are SQL files, not code; there is nowhere to put the branch |
| generating the DDL from a schema builder | that is SeaORM's migration model, which ADR-0022 already decided against — both adapters migrate on SQLx's engine, precisely so there is **one** migration history rather than two |

## Consequences

- **The two sets must not drift.** A table added to one engine and not the other passes three of the
  four rows. The four-row suite is what catches it, and the census (FR-084) is what stops the suite
  from being quietly deleted.
- A reviewer reads two files per table instead of one. That is the price, and it is paid once per
  table rather than once per query.
