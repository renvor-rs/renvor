# Phase 008 — Evidence

**Date**: 2026-08-26
**Phase**: 008 — Four-row database hardening
**Base**: `c3feaa1586ab742fdfa119d3abdf58b5966e0130` (live main, PR #40 merged)
**Closure authority**: **not yet granted.** ADR-0023 is accepted under **W-017**, which is a
record-level waiver and closes nothing. Phase 008's own closure waiver is granted separately, at
closure, in the post-merge closure pull request. **No independent human review occurred, and none
is claimed** — see [`phase-008-review-record.md`](phase-008-review-record.md).

**Companion records, all tracked and fetchable from a clean index-only checkout:**

| Record | What is in it |
|---|---|
| [`phase-008-mutation-ledger.md`](phase-008-mutation-ledger.md) | every mutation, including the two that **survived** and the hypothesis that was **wrong** |
| [`phase-008-limitations.md`](phase-008-limitations.md) | limitation dispositions, the phase-qualified citation rule, and the **F-3** record with owner and deadline |
| [`phase-008-review-record.md`](phase-008-review-record.md) | what each review was, and the disposition of all ten findings |
| [`phase-008-dependency-inventory.md`](phase-008-dependency-inventory.md) | the dependency position |

Phase 008's working notes live in `specs/`, which is deliberately untracked and **cannot** be
fetched from a clean checkout. Nothing a reviewer needs in order to judge closure is cited from
there. `git ls-files specs` returns **0**.

---

## 1. What this phase added

Phase 006 shipped direct SQLx. Phase 007 shipped SeaORM and claimed parity. Neither established
what the **four rows** of `PLAN.md` §10.1 must agree about, nor what they are permitted to
disagree about. That is this phase.

| Deliverable (PLAN §Phase 008) | Where it landed |
|---|---|
| full four-row compatibility workflow | `xtask` step 4's persistence census — **28** (row, test) pairs, and step 1 refuses a run without both databases |
| data-type and migration portability guide | `docs/docs/database-portability.mdx` |
| concurrency/idempotency tests | `renvor_testkit::concurrency` |
| backup/restore test guidance | `docs/docs/backup-restore.mdx` |
| database error normalization | `renvor-database`'s three new kinds and both adapters' classifiers |
| upgrade test fixtures | `renvor_testkit::upgrade` and `tests/migrations-upgrade-base/` |

Plus one contract — **C-16, database portability** — one accepted decision record, **ADR-0023**,
and two new public types: `renvor_database::StartupDiagnostic` and `renvor_database::DatabaseAdapter`.

## 2. The census: what "all four rows" is now measured by

`cargo test` reported `ok` with a row of the matrix deleted — 6 tests became 3, and the suite
still passed. Nothing stated how many rows *should* run.

Step 4 now names every (row, suite) pair it requires and fails when one stops reporting:

```
[4/11] tests (four-row persistence census): ok — all 28 row-suite pairs reported in (4 rows x 7 required tests)
```

Five suites are compiled **once** in `renvor-testkit` and called by every row — ports contract,
domain example, concurrency and idempotency, portability, upgrade path. The sixth, the startup
diagnostic, is per-adapter by necessity — it drives each adapter's own provider — and carries
**two** required tests: a refused socket and a refused credential, which reach different code.

**The count was 24 when this phase was first presented.** The correction cycle added the
server-side refusal row on finding C, so any earlier statement of "24 pairs" describes the reviewed
head rather than the merged one. Stated rather than quietly reconciled.

**Step 1 now refuses a run without the databases.** The census used to print `ok — NOT RUN` when
`RENVOR_TEST_REQUIRE_DATABASE` was absent and return success, so a full `cargo xtask verify` could
exit **0** having executed none of the pairs — a conditional step in a sequence whose contract says
it has none. It is exit **2** with setup instructions now, and nothing is started automatically.

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

It cannot leak **structurally**: four fields, every one of them a fieldless enum — the adapter,
the database, the phase, and a `DatabaseError` that itself holds one fieldless enum. There is no
`String` and no constructor that accepts one. The renderable set is finite, so the redaction test
enumerates **all 264** rather than sampling, and walks each one's whole `source` chain.

**The adapter field was a `&'static str` until the correction cycle**, with a comment saying it
*"must be a literal"*. `'static` is a lifetime, not a provenance: `Box::leak` promotes any runtime
`String`, and a red test rendered a password out of the type. It is `DatabaseAdapter` now, a
two-variant enum, with a `compile_fail` control and a compiling twin.

## 6. Testing discipline

Every batch carries red → green → mutation evidence. **Twenty-seven mutations were run**, and all
of them — kills, survivors, and one wrong hypothesis — are in
[`phase-008-mutation-ledger.md`](phase-008-mutation-ledger.md). Five are worth naming here:

- **M-11** — turning `INSERT` into an upsert made all four concurrent writers "commit"; the
  exclusion assertion caught it on both engines. That is the realistic defect this suite exists for.
- **M-18** — removing the isolation probe's first read made MySQL's repeatable-read snapshot form
  *later*, and the engine difference disappeared. The comment claiming the first read is
  load-bearing is now a measured fact.
- **M-7** — a pagination mutation **survived** on both engines, because rows written 1..7 come back
  in that order anyway. Fixed by scrambling the insert order; it then killed PostgreSQL and still
  survived MySQL, because InnoDB clusters on the primary key. A second mutation kills both. Both
  are kept.
- **M-20** — killed three suites rather than the one predicted. **The hypothesis about it was
  wrong**, and it is recorded as a wrong hypothesis rather than dropped.
- **M-24** — **survived**. Closing `StartupDiagnostic`'s adapter field to an enum stops a *caller*
  passing text; nothing stopped a *maintainer* adding `Custom(&'static str)`. Every test passed
  with the hole reopened. It is killed now, by a test stating which crate names have actually been
  reviewed — but it is recorded as a survivor first, because that order is the evidence.

## 6a. The correction cycle — 2026-08-26

Ten findings from an automated Codex review were dispositioned **by change, not by argument**. The
full table is in [`phase-008-review-record.md`](phase-008-review-record.md). The consequential ones:

- **`StartupDiagnostic` could render caller text.** `&'static str` constrains a *lifetime*, not a
  *provenance*: `Box::leak` promotes any runtime `String`. A red test rendered `hunter2` out of a
  type documented as unable to carry one. Closed with `DatabaseAdapter`, a two-variant enum.
- **The causal chain was flattened.** The diagnostic kept the kind and discarded the
  `DatabaseError`, so `source` answered `None` — contrary to C-E2, and flattened one link *too
  early*. It now preserves the safe cause and terminates before the driver's error.
- **`DatabaseErrorKind::category()` reported ordinary database outcomes as `Internal`**, which
  C-E1 reserves for a kernel defect. Removed rather than re-aimed, taking `renvor-database`'s
  `renvor-core` dependency with it.
- **`cargo xtask verify` could exit 0 without the census.** Now exit 2, with instructions and
  without auto-starting anything.
- **C-16 had no decision record behind it.** **ADR-0023** now carries all seven decisions with
  alternatives and consequences, each bound to its four-row measurement.

## 7. Failures that are part of this record

- The first full verify on the Batch E head **FAILED at step 10**: a new documentation page's
  Docusaurus "Edit this page" link 404s because the page is not on `main` yet. Structural — no new
  documentation page can pass the link check before it merges.
- The first full verify on the Batch G head **FAILED at step 4**: `renvor-core` forbids
  interpolating a rendering into an assertion message in a credential-handling file, and both of my
  new test files did exactly that. On a redaction regression they would have printed the canary into
  the test log.
- The first full verify of the correction cycle **FAILED at step 4** for the *same rule, in a new
  file*: adding a password literal to a documentation example pulled
  `crates/renvor-database/src/startup.rs` into the credential-handling scope, and its existing
  assertion messages interpolated the rendering they were asserting about. Ten diagnostics were
  rewritten to name an index. **The gate caught the same class of mistake twice in one phase**,
  which is the gate working.
- `renvor-cli/tests/tls_consent.rs` failed once and passed unchanged on rerun (**F-3**). Out of
  scope, **unresolved**, and **the rerun does not close it**. It now has an owner (Ahmed Anbar), a
  target (a dedicated follow-up before Phase 009), and a deadline (**2026-09-02**); the test
  remains enabled and release coverage is preserved. See
  [`phase-008-limitations.md`](phase-008-limitations.md).
- One earlier attempt at gate-level red evidence was **abandoned rather than reported**, because
  sources were edited while it ran.

## 8. Limitations and inherited work

Full dispositions: [`phase-008-limitations.md`](phase-008-limitations.md).

Two limitations targeted at Phase 008 are **not closed by it**, and both are now **retargeted to
Phase 013**, owner **Ahmed Anbar**, each with an obligation to implement or explicitly exclude from
REST 1.0:

| Limitation | What is missing |
|---|---|
| `006/L-7` | mixed-direction keyset pagination — lifting the refusal requires a public API change, because the binding count lives inside `seek_predicate`'s `String` |
| `007/L-11` | `TransactionTrait`, savepoints, and isolation-level configuration are not exposed |

`L-11` has been reused across **several** phases, not two. Each ledger is per-phase and internally
unambiguous, so the ambiguity exists only in an unqualified citation — and the fix belongs to
citation, not to numbering. **Phase-qualified references (`006/L-7`, `007/L-11`) are canonical.
Closed phases' evidence is unchanged.**

## 9. What this phase did not do

No crate published. No tag, release, or deployment. No repository setting changed. No CodeQL
dismissal, no admin bypass. **No lychee exclusion** — the one precedent (EX-004, EX-006) would have
allowed was avoided by fixing the link at its source, and the temporary suppression is removed in
the post-merge closure pull request. `git ls-files specs` returns zero. **Phase 009 has not
started.**

**One waiver has been granted so far**, by explicit authority and narrowly:

| Waiver | Waives | Does **not** |
|---|---|---|
| **W-017** | the independent-human-review requirement for accepting **ADR-0023** | close Phase 008 |

It expires **2027-02-11**, or immediately when a qualified independent human reviewer becomes
available — whichever comes first.

**Phase 008 is not closed by this pull request.** Closing it requires a second, phase-level waiver,
which is granted in the post-merge closure pull request alongside the removal of the two temporary
edit-link suppressions. That will take Phase 008 to **exactly two** reviewed exceptions — this
ledger's stated maximum — and will be the **eighth consecutive** phase-level waiver of the same
rule for the same reason. **RO-001's 2026-11-19 review date is unchanged, and no recruitment
progress of any kind has occurred.**
