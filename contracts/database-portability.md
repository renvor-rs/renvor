---
description: "Contract C-16 — what PostgreSQL and MySQL are permitted to disagree about, and what an application may assume across both"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-16 — Database portability

**Status**: defined against measurements, per constitution principle V.
**Applies to**: `renvor-database`, `renvor-sqlx`, `renvor-seaorm`, and every application schema
generated or migrated by them.
**Source of authority**: `PLAN.md` §10.1, which requires that *"identifiers, timestamps, isolation
levels, upserts, pagination order, JSON capabilities, and migration syntax require cross-database
contract tests"*. This contract is those seven topics; it adds no requirement PLAN does not already
place, and it settles each one against a measurement rather than a reading of the manuals.

> **Numbering.** C-6 and C-7 are unassigned in this repository. This contract takes **C-16**,
> the next number after the highest in use, rather than filling a gap whose history is not
> recorded here.

## The rule this contract exists to enforce

**A difference between the engines is either eliminated, or named.** Nothing may be left to be
discovered by an application that was developed against one engine and deployed on the other.

Renvor does not claim the two engines are the same. Several of the differences below **cannot** be
removed by an adapter — MySQL's `TIMESTAMP` genuinely ends in 2038, and no library can supply the
missing years. So each is stated, and each is attached to an executable assertion in
`renvor_testkit::portability`, run against both engines through both adapters. When an engine
changes, the suite fails and this document is what has to be corrected.

## Every statement below was measured

Against `postgres:17.11-trixie` and `mysql:8.4.11`, the images CI pins. No claim here is recalled
from documentation.

---

## 1. Identifiers

| | PostgreSQL | MySQL |
|---|---|---|
| Limit | 63 **bytes** | 64 **characters** |
| Past the limit | **truncated**, object created under the shortened name | **refused**, `ER_TOO_LONG_IDENT` |
| Unquoted case | folded to lowercase | preserved |
| Quoting | `"name"` | `` `name` `` |

**Normative:** every identifier a Renvor schema or migration writes **must be at most 63 bytes**,
lowercase, and must not require quoting to be legal.

The limit is 63 rather than 64 because the *smaller* of the two limits is the portable one. The
lowercase requirement exists because PostgreSQL folds and MySQL does not, so a mixed-case
identifier is one name on one engine and a different name on the other.

The truncation row is why this is a *requirement* rather than advice: PostgreSQL **succeeds** with
a name the author did not write, and the failure surfaces later, at a statement that names the
identifier in full.

`DatabaseKind::quote_identifier` emits the correct quoting for each engine. It is a syntax helper,
not a licence to use identifiers that need it.

## 2. Timestamps

| | PostgreSQL | MySQL |
|---|---|---|
| Zone-aware | `TIMESTAMPTZ` | `TIMESTAMP` (stored UTC) |
| Zone-naive | `TIMESTAMP` | `DATETIME` |
| Default sub-second digits | **6** | **0** |
| Upper bound | year 294276 | **2038-01-19 UTC** |

**Normative:**

- A timestamp column **must declare its precision explicitly** — `TIMESTAMP(6)` /
  `DATETIME(6)`. A column written without one silently keeps microseconds on PostgreSQL and
  discards them on MySQL.
- A column that may hold a date after 2038 **must not** use MySQL's `TIMESTAMP`. Use `DATETIME(6)`
  and store UTC.
- Storing UTC and converting at the edge is the only arrangement both engines represent the same
  way. MySQL's `TIMESTAMP` converts on read using the *session* zone, so the same row read by two
  sessions yields two values.

## 3. Isolation levels

| | Default |
|---|---|
| PostgreSQL | `READ COMMITTED` |
| MySQL / InnoDB | `REPEATABLE READ` |

Measured consequence: with a transaction open and one read already taken, a second read of the same
table **sees** another session's commit on PostgreSQL and **does not** on MySQL.

**Normative:** an application **must not depend on the default**. A read-modify-write that must be
atomic states it — either by locking the row it read, or by writing a condition that fails if the
row changed. Renvor exposes no isolation-level setter, so "set it to the one I want" is not
available and this rule has no exception.

> A MySQL transaction takes its snapshot at its **first read**, not at `BEGIN`. A transaction that
> writes before it reads therefore does not behave like the repeatable-read example above. This is
> measured, and it is the reason the contract's test performs a read before the concurrent commit.

## 4. Upserts

The engines disagree about **which row an upsert writes**.

Given `w(id PRIMARY KEY, tag UNIQUE, v)` holding `(1, 'x', 1)`, a statement inserting `(2, 'x', 9)`
and scoped to `id`:

| Engine | Result |
|---|---|
| PostgreSQL | `ERROR: duplicate key value violates unique constraint "w_tag_key"` |
| MySQL | succeeds; **row `id = 1` is updated**; row 2 is never created |

MySQL's `ON DUPLICATE KEY UPDATE` cannot be scoped to one key. It fires on whichever unique key
conflicted, and updates *that* row.

**Normative:**

- A portable upsert **targets a table with exactly one unique key** — its primary key. On a table
  with a second unique constraint, an upsert is refused on PostgreSQL and rewrites an unrelated row
  on MySQL, and neither outcome is what the statement asked for.
- An application **must not** infer insert-versus-update from the affected-row count:

  | Case | PostgreSQL | MySQL |
  |---|---|---|
  | Inserted | 1 | 1 |
  | Updated | 1 | **2** |
  | Matched, value unchanged | 1 | **0** |

  Read the row back, or return it from the statement.

## 5. Pagination order

NULLs sort to **opposite ends**:

| | `ORDER BY v ASC` | `ORDER BY v DESC` |
|---|---|---|
| PostgreSQL | NULLS **LAST** | NULLS **FIRST** |
| MySQL | NULLS **FIRST** | NULLS **LAST** |

**Normative:** a keyset cursor under contract C-15 **must order on a non-nullable total key**, and
the final term must be unique. A sort over a nullable column is permitted only as a *preceding*
term, and the ordering must then state NULL placement explicitly rather than inherit it.

Ordering on a nullable column without a unique tiebreaker produces a cursor that resumes from a
different row on each engine, which is a wrong answer rather than a slow one.

## 6. JSON

| Type | `{"b":1,"a":2,"a":3}` becomes |
|---|---|
| PostgreSQL `jsonb` | `{"a": 3, "b": 1}` |
| MySQL `JSON` | `{"a": 3, "b": 1}` |
| PostgreSQL `json` | `{"b":1,"a":2,"a":3}` — verbatim |

**Normative:** a portable JSON column **uses `jsonb` on PostgreSQL and `JSON` on MySQL**. The two
normalise identically — keys sorted, duplicates resolved last-wins, whitespace discarded — which is
asserted as a byte equality rather than described.

PostgreSQL's `json` **must not** be used in a portable schema. It preserves the received text,
which MySQL has no way to reproduce. An application that needs the exact bytes it was sent should
store them as text and say so.

## 7. Migration syntax

| | `BEGIN; CREATE TABLE …; ROLLBACK;` |
|---|---|
| PostgreSQL | table is **gone** — DDL is transactional |
| MySQL | table **remains** — DDL forces an implicit commit |

**Normative:**

- A migration **must be safe to re-run after a partial failure**. On MySQL every statement before
  the failure is already committed and no rollback can reach it, so the recovery path is "run the
  rest", not "run it again from the start".
- Prefer **one schema change per migration**. A migration with a single statement has no partial
  state to be in.
- A migration **must not** rely on rollback to undo earlier statements in the same file.

`MigrationPolicy` and the migration ledger are unaffected: both adapters migrate on SQLx's engine
(ADR-0022), so both inherit the same behaviour on each engine.

---

## Where this is enforced

| Topic | Assertion |
|---|---|
| Identifiers | `an_oversized_identifier_is_refused_or_shortened` |
| Timestamps | `default_timestamp_precision_is_engine_specific` |
| Isolation | `repeated_reads_differ_by_default_isolation` |
| Upserts | `an_unnamed_unique_key_is_not_a_portable_upsert_target` |
| Pagination order | `nulls_sort_to_the_documented_end` |
| JSON | `json_normalisation_agrees_across_engines` |
| Migration syntax | `ddl_transactionality_is_engine_specific` |

All seven live in `renvor_testkit::portability`, are compiled once, and are executed by
`renvor-sqlx/tests/portability.rs` and `renvor-seaorm/tests/portability.rs` against both engines.
`xtask` step 4's persistence census requires every one of those runs to report in, so a row that
stopped executing fails the gate rather than disappearing quietly.
