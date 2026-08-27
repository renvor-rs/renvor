---
description: "Contract C-16 — what PostgreSQL and MySQL are permitted to disagree about, and what an application may assume across both"
version: "1.2.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. 1.2.0 (2026-08-27) NARROWS the JSON guarantee from every document to a measured subset, and REPLACES a false MySQL recovery instruction with the refusal that actually happens; both were review findings and both changed a rule. 1.1.0 (2026-08-26) cited ADR-0023 alongside PLAN §10.1 as source of authority; no rule changed. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-16 — Database portability

**Status**: defined against measurements, per constitution principle V.
**Applies to**: `renvor-database`, `renvor-sqlx`, `renvor-seaorm`, and every application schema
generated or migrated by them.
**Source of authority**: `PLAN.md` §10.1 **and** [ADR-0023](../decisions/0023-database-portability-across-the-four-rows.md).

PLAN §10.1 requires that *"identifiers, timestamps, isolation levels, upserts, pagination order,
JSON capabilities, and migration syntax require cross-database contract tests"*. It names the seven
topics and requires them to be measured; it does not decide what the answers are.

**Those decisions are ADR-0023's, and this contract states them.** The distinction is not
bookkeeping: the rules below are normative — they forbid constructs that compile and run — and each
was reversible when it was made. A contract records a rule; it does not record the alternatives that
were rejected or the cost of the one taken. Read this document for what is required, and ADR-0023
for why, what else was considered, and what each choice costs.

This contract adds no requirement PLAN does not already place, and it settles each topic against a
measurement rather than a reading of the manuals.

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

**Normative:** a portable JSON column **uses `jsonb` on PostgreSQL and `JSON` on MySQL**.

PostgreSQL's `json` **must not** be used in a portable schema. It preserves the received text,
which MySQL has no way to reproduce. An application that needs the exact bytes it was sent should
store them as text and say so.

### The guarantee is a measured subset, not the whole type — corrected 2026-08-27

**The rule that used to stand here said the two types "normalise identically"** — keys sorted,
duplicates last-wins, whitespace discarded — **and that a value written on one engine could be
compared on the other**, on the evidence of the single duplicate-key probe above. A review found a
counterexample; measuring the rest of the space to answer it found two more the review had not
named. All four are now in the table and all four execute.

A value is portable when **both engines accept it** and **both return the same text**. Measured
against PostgreSQL 17.11 and MySQL 8.4.11:

| Portable | What both engines do |
|---|---|
| objects and arrays, to any depth | keys sorted, duplicates last-wins, whitespace discarded |
| strings of any Unicode scalar value **except** `U+0000` | identical, including outside the basic plane |
| the escapes for quote, backslash, tab, and a control character | re-emitted identically |
| `true`, `false`, `null` | identical |
| **integers** in plain notation, within the signed 64-bit range | exact on both, including past 2^53 |
| top-level scalars, `{}`, `[]` | a document need not be an object |

**Excluded, each because it was measured to differ:**

| Excluded | PostgreSQL `jsonb` | MySQL `JSON` |
|---|---|---|
| a string holding `U+0000` (the `u0000` escape) | **refused** — PostgreSQL text cannot represent NUL | stored |
| exponent notation, `1E2` | `100` | `100.0` |
| a trailing zero, `1.50` | `1.50` | `1.5` |
| a non-integer past double precision | exact, `0.12345678901234567890123456789` | **rounded**, `0.12345678901234568` |

The last is **data loss**, not formatting: MySQL stores non-integer JSON numbers as IEEE-754
doubles and PostgreSQL stores every JSON number as `numeric`. An unpaired surrogate is refused by
*both* engines, so it is a malformed document rather than a portability difference.

**Normative, and every clause is one of those measurements:**

- An application **must not** persist a value outside the subset in a portable JSON column. Values
  that may fall outside it must be validated before persistence, or the original text stored under
  an application contract of its own.
- Raw byte representation, whitespace, key order and duplicate-key source text are **not**
  preserved by either engine and **must not** be relied on. What is portable is the value.
- A value inside the subset is **byte-identical** across the two engines, and that is asserted as
  an equality rather than described.
- The executable form of this subset is `renvor_testkit::portability::JSON_PROBES`, which carries
  every document above with the answer each engine gave. **The exclusions are asserted too**: an
  engine that stopped diverging fails the gate and sends someone back to re-measure, rather than
  leaving the contract narrower than the engines require.

## 7. Migration syntax

| | `BEGIN; CREATE TABLE …; ROLLBACK;` |
|---|---|
| PostgreSQL | table is **gone** — DDL is transactional |
| MySQL | table **remains** — DDL forces an implicit commit |

**Normative:**

- A portable migration **must contain exactly one schema statement**. On MySQL that is the only way
  to guarantee it has no partial state to be in.
- A migration **must not** rely on rollback to undo earlier statements in the same file. On MySQL
  every statement before a failure is already committed and no rollback can reach it.
- A migration **must not** be written expecting the framework to resume it after a partial failure.
  It does not, and the section below is what it does instead.

### A partial MySQL failure is refused on the next run, not resumed — corrected 2026-08-27

**The instruction that used to stand here was false.** It said a migration "must be safe to re-run
after a partial failure" because on MySQL *"the recovery path is 'run the rest', not 'run it again
from the start'"*. A review found that wrong and the measurement settles it. In the pinned SQLx
**0.9.0**:

1. SQLx inserts the version into `_sqlx_migrations` with `success = FALSE` **before** it runs the
   migration body — its own comment says this is how it detects the MySQL case at all.
2. MySQL's implicit commit makes that row permanent the moment the first DDL statement executes,
   along with every statement up to the failure.
3. `run_direct` calls `dirty_version` at the top of **every** subsequent run and returns
   `MigrateError::Dirty`, which Renvor reports as `DatabaseErrorKind::MigrationDirty`. **No
   statement is sent at all.** There is no "rest" to run, and a deploy that retries loops.

PostgreSQL never reaches that state: the ledger row and the migration body share one transaction,
so a failure removes both and the next run genuinely starts over. Both halves are asserted, on both
engines, by `a_partial_migration_is_refused_on_the_next_run_rather_than_resumed`.

**No SQLx command clears the dirty row.** Verified against sqlx-cli 0.9.0: `sqlx migrate run`,
`sqlx migrate revert` and `sqlx migrate override skip` each call `dirty_version` first and refuse —
`override skip` is `run` with a flag, and inherits its guard. Recovery is therefore an explicit
operator action, in this order:

1. **Inspect what committed.** `_sqlx_migrations` names the version; the migration file says which
   statements precede the failure; the catalogue says which of those objects exist.
2. **Repair the schema to one of two known states** — the version fully applied, or fully absent.
   Not "close enough": the checksum already recorded in the dirty row is the one SQLx validates
   against on the next run.
3. **Reconcile the ledger to the state step 2 established**, and only to that state:
   - fully applied → `UPDATE _sqlx_migrations SET success = TRUE WHERE version = <version>`. The
     row already carries the right checksum and description, so this is the whole change.
   - fully absent → `DELETE FROM _sqlx_migrations WHERE version = <version>`, returning the version
     to pending.
4. **Re-run** the migration.

Step 3 is the only reconciliation this contract sanctions, and it is safe **only** after step 2.
Editing the ledger to get a deploy moving, without first establishing which state the schema is
actually in, exchanges a stopped deploy for a schema the framework believes is something it is not
— and the next migration is written against that belief.

Take a backup before step 2. A migration that failed partway is precisely the case where the schema
and the operator's model of it have already diverged.

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
| Partial-failure recovery | `a_partial_migration_is_refused_on_the_next_run_rather_than_resumed` |

The first seven live in `renvor_testkit::portability`, are compiled once, and are executed by
`renvor-sqlx/tests/portability.rs` and `renvor-seaorm/tests/portability.rs` against both engines.
`xtask` step 4's persistence census requires every one of those runs to report in, so a row that
stopped executing fails the gate rather than disappearing quietly.

The eighth is in `renvor-sqlx/tests/migration.rs` rather than the shared suite, because what it
measures is not a property of the server alone: it is what SQLx's **runner** does with the server's
behaviour, and reaching it requires a migration set built to fail. Both adapters run that runner
(ADR-0022), so measuring it once measures it for both.
