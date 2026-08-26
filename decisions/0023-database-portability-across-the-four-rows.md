# ADR-0023: What PostgreSQL and MySQL are permitted to disagree about

| Field | Value |
|---|---|
| **ID** | 0023 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-017 |
| **Review date** | 2026-08-26 |
| **Superseded by** | *(not superseded)* |

> **This record is `accepted` under W-017. The review behind it was NOT independent.**
>
> No independent human review of this record has occurred, and none is claimed. Acceptance rests on
> **[W-017](../governance/waivers.md)**, expiring **2027-02-11** or immediately when a qualified
> independent human reviewer becomes available — whichever is first. W-017 covers **this record
> only** and authorises nothing else. It does **not** close Phase 008.

## Context

`PLAN.md` §10.1 makes four rows first-class by REST 1.0 — direct SQLx and SeaORM, each on
PostgreSQL and MySQL — and states one requirement about their differences:

> Database-specific behavior must be isolated behind adapters and documented. Identifiers,
> timestamps, isolation levels, upserts, pagination order, JSON capabilities, and migration syntax
> require cross-database contract tests.

PLAN names the seven topics and requires them to be **measured**. It does not decide what the
answers are, and neither did Phase 006 or Phase 007. Each shipped one persistence model and
asserted the other engine worked; neither stated what an application is permitted to assume across
both.

Phase 008 measured all seven and wrote `contracts/database-portability.md` (C-16). That contract is
**normative** — it forbids constructs that compile and run — and the choices behind it are
consequential and were reversible when they were made. A contract states a rule; it does not record
the alternatives that were rejected or the cost of the one that was taken. That is what this record
is for, and it is why C-16 alone was not sufficient authority.

## Decision

Seven decisions. Each is bound to the measurement that produced it, taken inside the images CI pins
— `postgres:17.11-trixie` and `mysql:8.4.11` — and each is executable in
`renvor_testkit::portability` against **all four rows**, not argued from documentation.

### 1. Identifiers — the smaller limit governs, and lowercase is required

**Decision.** Every identifier a Renvor schema or migration writes is at most **63 bytes**,
lowercase, and legal unquoted.

**Measured.** PostgreSQL's limit is 63 bytes and it **truncates** past it, creating the object under
a name the author did not write. MySQL's is 64 characters and it **refuses**, `ER_TOO_LONG_IDENT`
(1059). Unquoted identifiers fold to lowercase on PostgreSQL and are preserved on MySQL.

**Alternatives.**

| Alternative | Why rejected |
|---|---|
| Take 64, MySQL's limit | PostgreSQL would silently truncate, and the failure surfaces later at a statement naming the identifier in full. The **smaller** limit is the portable one |
| Permit mixed case and quote everywhere | A mixed-case identifier is one name on one engine and a different name on the other. Quoting hides that rather than removing it |
| Normalise case in the adapter | Renvor would own an identifier rewriter, and every hand-written migration would disagree with it |

**Consequence.** Long descriptive names are unavailable; `quote_identifier` remains a syntax helper
and is explicitly **not** a licence to use identifiers that need it.

### 2. Timestamps — precision is declared, and 2038 is stated rather than avoided

**Decision.** A timestamp column declares its precision explicitly (`TIMESTAMP(6)` / `DATETIME(6)`).
A column that may hold a date after 2038 does not use MySQL's `TIMESTAMP`. UTC is stored and
converted at the edge.

**Measured.** Default sub-second digits: PostgreSQL **6**, MySQL **0**. MySQL's `TIMESTAMP` ends at
**2038-01-19 UTC**. MySQL's `TIMESTAMP` converts on read using the **session** zone, so one row read
by two sessions yields two values.

**Alternatives.**

| Alternative | Why rejected |
|---|---|
| Default to microseconds in the adapter | Renvor does not write the application's DDL. A default the migration file does not state is a default the file contradicts |
| Use MySQL `TIMESTAMP` for zone-awareness | It ends in 2038 and no library can supply the missing years. This is a difference that **cannot** be eliminated, so it is named |
| Store local time and a zone column | Two columns that can disagree, and MySQL's session-zone conversion still applies to one of them |

**Consequence.** Schemas are more verbose. Truncation to whole seconds on MySQL — silent, and
unrecoverable once written — is what the verbosity buys.

### 3. Isolation — no application may depend on the default, and Renvor exposes no setter

**Decision.** An application states the atomicity it needs, by locking the row it read or writing a
condition that fails if the row changed. Renvor exposes **no isolation-level setter**, so this rule
has no exception.

**Measured.** Defaults differ: PostgreSQL `READ COMMITTED`, InnoDB `REPEATABLE READ`. With a
transaction open and one read already taken, a second read of the same table **sees** another
session's commit on PostgreSQL and **does not** on MySQL. A MySQL transaction takes its snapshot at
its **first read**, not at `BEGIN` — proven by mutation M-18, which removed the probe's first read
and made the engine difference disappear.

**Alternatives.**

| Alternative | Why rejected |
|---|---|
| Set one isolation level on every connection | It changes MySQL's replication and locking behaviour globally, on the strength of a portability argument. Renvor would be choosing an operator's setting for them |
| Expose a setter now | An isolation API whose failure semantics are untested is worse than none. Recorded as limitation `007/L-11` with a target phase, not half-shipped |
| Document the defaults and stop | An application that read the documentation still writes a read-modify-write that is correct on one engine |

**Consequence.** Application code is more explicit than it would be on a single engine. Renvor has
no isolation surface at all until a phase tests one.

### 4. Upserts — one unique key, and the affected-row count is not an answer

**Decision.** A portable upsert targets a table with **exactly one unique key**, its primary key. An
application does not infer insert-versus-update from the affected-row count.

**Measured.** Given `w(id PRIMARY KEY, tag UNIQUE, v)` holding `(1,'x',1)`, a statement inserting
`(2,'x',9)` scoped to `id`: PostgreSQL **refuses** (`duplicate key value violates unique constraint
"w_tag_key"`); MySQL **succeeds and updates row 1** — a row the statement never named. Affected-row
counts: inserted 1/1, updated 1/**2**, matched-unchanged 1/**0**.

**Alternatives.**

| Alternative | Why rejected |
|---|---|
| Emit engine-specific upsert SQL per engine | MySQL's `ON DUPLICATE KEY UPDATE` **cannot be scoped to one key**. There is no MySQL statement with PostgreSQL's semantics to emit |
| Pre-check with a `SELECT` | Two statements with a race between them. A silent lost update is worse than a refused one |
| Normalise the row count in the adapter | Renvor would have to know which of the three cases occurred, which is the thing the count fails to tell it |

**Consequence.** Tables with a second unique constraint have no portable upsert. That is stated
rather than worked around, because both available outcomes — a refusal and an unrelated rewrite —
are wrong.

### 5. Pagination order — a keyset orders on a non-nullable total key

**Decision.** A keyset cursor under contract C-15 orders on a non-nullable key whose final term is
unique. A nullable column may appear only as a preceding term, with NULL placement stated.

**Measured.** NULLs sort to **opposite ends**: PostgreSQL is NULLS LAST ascending and NULLS FIRST
descending; MySQL is the reverse.

**Alternatives.**

| Alternative | Why rejected |
|---|---|
| Emit `NULLS LAST` everywhere | MySQL does not support the clause. The rewrite is an `IS NULL` expression that changes the index the planner can use |
| Let the cursor encode NULL placement | The cursor is already opaque; this would make its correctness depend on the engine that issued it, so a restored dump on the other engine resumes elsewhere |

**Consequence.** Sorting on a nullable column requires a tiebreaker. Without one the cursor resumes
from a different row on each engine — a **wrong answer**, not a slow one.

### 6. JSON — `jsonb` and `JSON`, and PostgreSQL's `json` is excluded

**Decision.** A portable JSON column is `jsonb` on PostgreSQL and `JSON` on MySQL. PostgreSQL's
`json` is not used in a portable schema.

**Measured.** `{"b":1,"a":2,"a":3}` becomes `{"a": 3, "b": 1}` on **both** `jsonb` and MySQL `JSON`
— byte-identical, keys sorted, duplicates last-wins, whitespace discarded. PostgreSQL's `json`
returns the text verbatim. This is the one topic of the seven with an exact portable answer, and it
is asserted as a **byte equality**, with PostgreSQL's `json` as the control.

**Alternatives.**

| Alternative | Why rejected |
|---|---|
| Allow `json` for exactness | MySQL has no way to reproduce it, so a schema using it is not portable at all |
| Store JSON as text everywhere | Discards indexing and containment operators on both engines to solve a problem neither has |

**Consequence.** An application that needs the exact bytes it was sent stores them as text and says
so. It does not get that from a JSON column.

### 7. Migration syntax — a migration is safe to resume, never to re-run from the start

**Decision.** A migration is safe to re-run after a partial failure by running the rest. One schema
change per migration is preferred. A migration does not rely on rollback to undo earlier statements
in the same file.

**Measured.** `BEGIN; CREATE TABLE …; ROLLBACK;` — the table is **gone** on PostgreSQL, because DDL
is transactional; it **remains** on MySQL, because DDL forces an implicit commit.

**Alternatives.**

| Alternative | Why rejected |
|---|---|
| Wrap each migration in a transaction and claim atomicity | It is **false on MySQL**, and a false guarantee about schema state is worse than a stated absence |
| Synthesise compensating statements for MySQL | Renvor would own a schema-diffing rollback engine — an ORM boundary, forbidden by constitution principle III |
| One migration engine per engine | Explicitly the "two competing histories" case ADR-0022 rejected |

**Consequence.** The recovery path after a failed migration differs by engine, and an operator must
know which they are on. `MigrationPolicy` and the ledger are unaffected: both adapters migrate on
SQLx's engine under ADR-0022, so both inherit the same behaviour on each engine.

## Alternatives to the whole approach

**Eliminate every difference inside the adapters.** Rejected, and the reason is not effort: MySQL's
`TIMESTAMP` genuinely ends in 2038, and MySQL's `ON DUPLICATE KEY UPDATE` genuinely cannot be scoped
to one key. An adapter that claimed to have removed those differences would be claiming something
false, and constitution principle IV forbids reporting success after partial failure.

**Support one engine well and treat the other as best-effort.** Rejected: `PLAN.md` §10.1 makes all
four rows first-class, and a best-effort row is one nobody measures.

**Leave the differences to documentation.** Rejected: this is what Phases 006 and 007 effectively
did. An application developed against one engine and deployed on the other discovers each
difference in production. Every rule above is attached to an executable assertion that runs on all
four rows, and `xtask` step 4's census fails when one stops reporting.

## Consequences

- **C-16 becomes normative.** It forbids constructs that compile and run on one engine.
- **Seven executable assertions**, compiled once in `renvor_testkit::portability` and run by both
  adapters against both engines — 4 of the census's 28 required (row, test) pairs.
- **A difference that cannot be eliminated is named rather than hidden.** Three of the seven are in
  that class: the 2038 bound, the upsert scoping, and DDL transactionality.
- **When an engine changes, the suite fails and this record and C-16 are what must be corrected.**
  The assertions key on `DatabaseKind` with no catch-all arm, so a third engine cannot be added
  without a measurement: the panic reads *"has never been measured against this contract"*.

## Acceptance

**This record was `proposed` when it was written, and is `accepted` as of 2026-08-26.**

Acceptance required an independent human review, which the project cannot supply — it has a single
maintainer, who authored every line this record describes and took every measurement in it.

It is therefore accepted under **W-017**, which waives *only* that requirement, *only* for this
record, and expires **2027-02-11** or immediately when a qualified independent human reviewer becomes
available — whichever comes first.

**It was accepted after the advisory findings were dispositioned, not before.** Ten findings were
raised against the reviewed head, two of them against safety claims this workspace had published as
true, and all ten were dispositioned **by change rather than by argument**. Their table is in
`governance/phase-008-review-record.md`. Accepting the record first and dispositioning afterwards
would have made the acceptance a formality.

**No independent review has occurred and none is claimed.** Automated reviews were commissioned and
are **advisory**; they are not independent and are not counted. Two commissioned reviewer agents
returned no result at all and are recorded as **NOT PERFORMED** in
`governance/phase-008-evidence.md`.

W-017 does **not** close Phase 008. Phase closure requires a separate, phase-level waiver, granted
at closure — the two axes are not collapsed, for the reason `governance/waivers.md` §W-005 records.
