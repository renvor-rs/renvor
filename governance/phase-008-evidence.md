# Phase 008 — Evidence

**Date**: 2026-08-26
**Phase**: 008 — Four-row database hardening
**Base**: `c3feaa1586ab742fdfa119d3abdf58b5966e0130` (live main, PR #40 merged)
**Closure authority**: **not yet granted.** This phase is presented for review; nothing here claims
it is accepted. **No independent human review occurred, and none is claimed.**

---

## 1. What this phase added

Phase 006 shipped direct SQLx. Phase 007 shipped SeaORM and claimed parity. Neither established
what the **four rows** of `PLAN.md` §10.1 must agree about, nor what they are permitted to
disagree about. That is this phase.

| Deliverable (PLAN §Phase 008) | Where it landed |
|---|---|
| full four-row compatibility workflow | `xtask` step 4's persistence census — 24 (row, suite) pairs |
| data-type and migration portability guide | `docs/docs/database-portability.mdx` |
| concurrency/idempotency tests | `renvor_testkit::concurrency` |
| backup/restore test guidance | `docs/docs/backup-restore.mdx` |
| database error normalization | `renvor-database`'s three new kinds and both adapters' classifiers |
| upgrade test fixtures | `renvor_testkit::upgrade` and `tests/migrations-upgrade-base/` |

Plus one contract: **C-16, database portability**, and one new public type,
`renvor_database::StartupDiagnostic`.

## 2. The census: what "all four rows" is now measured by

`cargo test` reported `ok` with a row of the matrix deleted — 6 tests became 3, and the suite
still passed. Nothing stated how many rows *should* run.

Step 4 now names every (row, suite) pair it requires and fails when one stops reporting:

```
[4/11] tests (four-row persistence census): ok — all 24 row-suite pairs reported in (4 rows x 6 suites)
```

Five suites are compiled **once** in `renvor-testkit` and called by every row — ports contract,
domain example, concurrency and idempotency, portability, upgrade path. The sixth, the startup
diagnostic, is per-adapter by necessity: it drives each adapter's own provider.

**Control.** With one row removed, `[4/11] tests: ok — passed` while
`[4/11] tests (four-row persistence census): FAILED`. The census caught what the suite could not.

## 3. Defects found and fixed

| # | Defect | How it was found |
|---|---|---|
| D-1 | `renvor-seaorm` classified a missing applied migration as `MigrationIrreversible`, disagreeing with `renvor-sqlx` | writing the cross-adapter classification matrix |
| D-2 | not-null and check violations were discarded into `StatementRejected` on both adapters | same |
| D-3 | no retryable-conflict kind existed; a lost deadlock was indistinguishable from a rejected statement | same |
| D-4 | `renvor-sqlx` **did not compile** with `db-mysql` alone — `tests/provider.rs`'s `boot_deadline` module named `sqlx::Postgres` from behind an `any(...)` gate | the new per-driver compile gate |

D-4 is the one worth naming: `cargo tree` reported the driver isolation intact, and it was.
Resolving and compiling are different questions, and only the first had ever been asked.

## 4. What the two engines are permitted to disagree about

Contract **C-16** settles the seven topics `PLAN.md` §10.1 names, each against a measurement taken
inside the pinned images. The consequential ones:

- **Upserts.** Given `w(id PRIMARY KEY, tag UNIQUE, v)` holding `(1,'x',1)`, a statement
  inserting `(2,'x',9)` scoped to `id`: PostgreSQL **refuses**; MySQL **updates row 1**, which
  the statement never named. C-16 therefore requires a portable upsert to target a table with
  exactly one unique key.
- **Migrations.** PostgreSQL rolls DDL back with the transaction; MySQL commits it implicitly. A
  failed migration leaves a repairable database on one engine and a half-migrated one on the other.
- **Pagination.** NULLs sort to opposite ends on the two engines, so a cursor over a nullable
  column resumes elsewhere.
- **JSON.** One portable answer exists: PostgreSQL `jsonb` and MySQL `JSON` normalise to
  **byte-identical** text. Asserted as an equality, with PostgreSQL's `json` as the control.

## 5. Startup diagnostics

A failed boot returned the bare `DatabaseError` — safe, but naming neither which database failed,
at which point, nor what to do. `StartupDiagnostic` names all three and adds a corrective action.

It cannot leak **structurally**: four fields, all `&'static str` or fieldless enum, no `String`
and no constructor accepting one. The renderable set is finite, so the redaction test enumerates
**all 264** rather than sampling.

## 6. Testing discipline

Every batch carries red → green → mutation evidence in
`specs/008-four-row-hardening/evidence/`. Twenty mutations were run. Four are worth reporting:

- **M-11** — turning `INSERT` into an upsert made all four concurrent writers "commit"; the
  exclusion assertion caught it on both engines. That is the realistic defect this suite exists for.
- **M-18** — removing the isolation probe's first read made MySQL's repeatable-read snapshot form
  *later*, and the engine difference disappeared. The comment claiming the first read is
  load-bearing is now a measured fact.
- **M-7** — a pagination mutation **survived** on both engines, because rows written 1..7 come back
  in that order anyway. Fixed by scrambling the insert order; it then killed PostgreSQL and still
  survived MySQL, because InnoDB clusters on the primary key. A second mutation kills both. Both
  are kept.
- **M-20** — killed three suites rather than the one predicted. **My hypothesis about it was
  wrong**, and it is recorded as a wrong hypothesis rather than dropped.

## 7. Failures that are part of this record

- The first full verify on the Batch E head **FAILED at step 10**: a new documentation page's
  Docusaurus "Edit this page" link 404s because the page is not on `main` yet. Structural — no new
  documentation page can pass the link check before it merges.
- The first full verify on the Batch G head **FAILED at step 4**: `renvor-core` forbids
  interpolating a rendering into an assertion message in a credential-handling file, and both of my
  new test files did exactly that. On a redaction regression they would have printed the canary into
  the test log.
- `renvor-cli/tests/tls_consent.rs` failed once and passed unchanged on rerun (**F-3**). Out of
  scope, unresolved, and **the rerun does not close it**.
- One earlier attempt at gate-level red evidence was **abandoned rather than reported**, because
  sources were edited while it ran.

## 8. Limitations and inherited work

See `specs/008-four-row-hardening/evidence/batch-h-ledger.md`.

Two limitations targeted at Phase 008 are **not closed by it**, stated plainly rather than left to
be inferred: `006/L-7` (mixed-direction keyset pagination) and `007/L-11` (`TransactionTrait`,
savepoints, isolation-level configuration). Both need a public API change; both are presented for a
decision rather than taken.

`L-11` names two different limitations in two closed phases. Phase 008 adopts **phase-qualified
citation** (`006/L-11`, `007/L-11`) rather than renumbering a closed phase's evidence.

## 9. What this phase did not do

No crate published. No tag, release, or deployment. No repository setting changed. No CodeQL
dismissal, no admin bypass, **no new waiver** — including the lychee exclusion that precedent
(EX-004, EX-006) would have allowed, which was avoided by fixing the link at its source. No new
ADR accepted. `git ls-files specs` returns zero. Phase 009 has not started.
