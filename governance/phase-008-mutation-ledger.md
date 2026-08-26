# Phase 008 — Mutation ledger

**Phase**: 008 — Four-row database hardening
**Date**: 2026-08-26
**Companion to**: [`phase-008-evidence.md`](phase-008-evidence.md)

Every mutation Phase 008 ran, including the ones that **survived** and the one hypothesis that was
**wrong**. A mutation is a deliberate defect introduced into a tree that passes, run against the
suite that claims to catch it: *killed* means at least one test failed, *survived* means every test
still passed and the suite does not in fact cover the thing it is named for.

**This file is tracked.** Phase 008's working notes live in `specs/`, which is deliberately
untracked and cannot be fetched from a clean checkout. Anything a reader needs in order to judge
whether this phase should close is here instead, and every citation below resolves in an index-only
clone.

## Why this ledger exists in the governance record

A green suite is evidence that nothing is currently broken. It is **not** evidence that the suite
would notice if something were. The only way to tell the two apart is to break something on purpose,
and the only honest way to report it is to include the mutations that got away.

Two of the twenty-seven below did.

---

## Batch C — error classification

| # | Mutation | Result |
|---|---|---|
| M-1 | delete the `ErrorKind::NotNullViolation` arm | **KILLED** — 2 failed |
| M-2 | change the conflict SQLSTATEs `40001`/`40P01` to `99999` | **KILLED** — 2 failed |
| M-3 | drop `40P01`, keep `40001` | **KILLED**, PostgreSQL only — 1 failed |
| M-4 | `renvor-seaorm`: delete the `ErrorKind::CheckViolation` arm | **KILLED** — 2 failed |
| M-5 | `renvor-seaorm`: revert the D-1 fix (`VersionMissing` → `MigrationIrreversible`) | **KILLED** — 2 failed |

**M-2 survived its first run, and that was the point.** The classification test had been written
against the kind the driver already reported, so changing the SQLSTATE table changed nothing it
looked at. The test was rewritten to drive a real crossed-lock deadlock, and M-2 was re-run: killed.

## Batch B — the shared domain example

| # | Mutation | Result |
|---|---|---|
| M-6 | a row's `insert` silently does nothing, returning `Ok(())` | **KILLED** — both rows fail at the lookup round-trip |
| M-7a | pagination loses `ORDER BY id ASC` | **PostgreSQL KILLED, MySQL SURVIVED** |
| M-7b | `ORDER BY id ASC` → `ORDER BY id DESC` | **KILLED** — both rows |
| M-8 | `set_rank` fakes its affected-row count as `1` | **KILLED** — both rows |

**M-7a survived on MySQL, and it is kept as a survivor rather than deleted.** Rows written `1..7`
came back in that order without an `ORDER BY`, so the assertion held vacuously. Scrambling the
insert order killed it on PostgreSQL and it **still survived on MySQL**, because InnoDB clusters on
the primary key and genuinely returns key order. That mutation is unkillable on MySQL by
construction. M-7b kills both, and both are kept: the pair is what documents the engine difference.

## Batch D — concurrency and idempotency

| # | Mutation | Result |
|---|---|---|
| M-9 | `ErrorKind::UniqueViolation` → `StatementRejected` (production code) | **KILLED** both engines |
| M-11 | `INSERT` → `ON CONFLICT DO NOTHING` / `INSERT IGNORE` | **KILLED** both engines — `4 of them committed` |
| M-12 | commit the three writes before the failing statement | **KILLED** — `row 511 survived a rolled-back transaction` |
| M-13 | retry ceiling raised to `MAX_ATTEMPTS + 2` | **KILLED** — `must stop at its named ceiling` |
| M-14 | `ensure` never observes the winner's row | **KILLED** |

**M-11 is the realistic defect this suite exists for.** Turning the contended `INSERT` into an
upsert made all four concurrent writers report success; the exclusion assertion caught it on both
engines.

## Batch E — portability

| # | Mutation | Result |
|---|---|---|
| M-15 | contract claims PostgreSQL sorts NULLs FIRST | **KILLED** PostgreSQL only |
| M-17 | PostgreSQL probes `::json` instead of `::jsonb` | **KILLED** — returned `{"b":1,"a":2,"a":3}` |
| M-18 | isolation probe drops its snapshot-establishing first read | **KILLED** MySQL |

**M-18 turned a comment into a measurement.** Removing the probe's first read made MySQL's
repeatable-read snapshot form later, and the engine difference disappeared: *"a second read inside
one transaction SAW a row another session committed"*. The claim that a MySQL transaction takes its
snapshot at its **first read** rather than at `BEGIN` is therefore a measured fact, and it is why
ADR-0023 §3 states it.

## Batch F — upgrade path

| # | Mutation | Result |
|---|---|---|
| M-19 | append one comment line to the base migration, so it is no longer byte-identical | **KILLED** — `MigrationChecksumMismatch` |
| M-20 | remove `DEFAULT 0` from the second migration | **KILLED** — but see below |

**M-19 isolates.** Re-run without a test filter, `the_upgrade_path_holds` failed while the other
suites passed, so the upgrade assertion is carrying its own weight rather than riding on a
neighbour.

**M-20 did not isolate, and the hypothesis about it was wrong.** It was predicted to kill only the
upgrade suite. It killed three, because without `DEFAULT 0` any `INSERT` omitting `rank_value`
violates `NOT NULL`. Recorded as a wrong hypothesis rather than dropped.

## Batch G — isolation and diagnostics

| # | Mutation | Result |
|---|---|---|
| M-21 | re-introduce the missing `#[cfg]` gate, full `cargo xtask verify` | **KILLED at step 7** — gate-level red for the new per-driver compile check |
| M-22 | the provider returns the bare driver error again | **KILLED** — `the provider must return a StartupDiagnostic, not a bare error` |
| M-23 | `AcquireTimeout`'s advice becomes "check the logs" | **KILLED** — `advice defers to the logs instead of naming a next step` |

**M-21's first attempt was abandoned, not reported.** Sources were edited while it ran, so it was
no longer testing the tree it started on. It was re-run on a clean tree and is reported from that
run.

---

## Correction cycle — 2026-08-26

Four mutations required by the correction authority: the startup enum boundary, the source chain,
the census fail-closed behaviour, and the category removal.

**M-24's result in this table is `SURVIVED, then KILLED`. That was an overstatement**, corrected
below under §The kill claim above was too broad: the mutation was killed for the variant that was
run and survives for the variant that was not.

| # | Mutation | Result |
|---|---|---|
| M-24 | add `DatabaseAdapter::Custom(&'static str)`, a variant carrying caller text | **SURVIVED**, then **KILLED** — see below |
| M-25 | `StartupDiagnostic::source` returns `None` again | **KILLED** — 2 unit tests and 4 integration tests, across both adapters |
| M-26a | the census's environment guard is disabled, so it reports `ok` without running | **KILLED** — `the census reported success without running` |
| M-26b | `prerequisites_gate` always returns `None`, so step 1 never refuses | **KILLED** — `a_missing_database_prerequisite_can_never_yield_exit_zero` |
| M-27a | `DatabaseErrorKind::category()` is restored | **KILLED** — the `compile_fail` documentation test compiles and therefore fails |
| M-27b | `renvor-database` depends on `renvor-core` again, method absent | **KILLED** — `renvor-database (--all-features) resolves \`renvor-core\`, which persistence feature isolation forbids` |

### M-24 survived, and closing it is the finding

Correction A replaced `StartupDiagnostic`'s `adapter: &'static str` with a closed enum, because
`'static` constrains a **lifetime, not a provenance**: `Box::leak` promotes any runtime `String` to
`&'static str`, and a red test demonstrated the type rendering `hunter2` out of a field documented
as unable to carry one.

M-24 then asked the next question — does anything stop a **maintainer** re-opening it? — and the
answer was no. Adding `Custom(&'static str)` to the enum, to `ALL`, and to `as_str` left every test
in the file passing. `as_str`'s catch-all-free match forced the author to *handle* the variant, and
returning the string satisfied it; the exhaustive redaction enumeration then covered the new variant
and found nothing, because the literal chosen for the mutation was benign.

The gap is closed by `no_adapter_can_render_anything_but_its_own_crate_name`, which states the
reviewed set of crate names the enum is permitted to render. A genuine third adapter costs one
reviewed line; a variant that carries caller text cannot be made to pass. **M-24 was re-run against
the strengthened suite and killed**: *"an adapter was added without a decision about what it is
allowed to render"*.

This is recorded as a survivor first and a kill second, in that order, because the order is the
evidence: the mutation found something the correction had missed.

### The kill claim above was too broad, and a review proved it — 2026-08-26

**The paragraph above is left standing rather than rewritten, because it is the false claim and
the record of it is the point.** What it got wrong is the sentence *"a variant that carries caller
text cannot be made to pass"*.

M-24 as run added `Custom(&'static str)` to the enum, **to `ALL`**, and to `as_str`. That is the
variant `no_adapter_can_render_anything_but_its_own_crate_name` catches, because that test reads
`ALL`. The mutation that **omits the variant from `ALL`** was never run, and it is the one that
matters:

| # | Mutation | Result |
|---|---|---|
| M-24b | add `DatabaseAdapter::Custom(&'static str)` and **omit it from `ALL`** | **SURVIVED** — 59 of 59 tests in `renvor-database` passed with a data-bearing variant present |
| M-24c | add an unreviewed **unit** variant `Turso => "renvor-turso"` | **KILLED** — `the adapter at index 2 renders a name nobody reviewed` |

M-24b survived for a structural reason, not a missing assertion: `ALL` was a **hand-maintained
restatement** of the variant list, and every guard in the file reads `ALL`. A variant absent from
`ALL` was a variant no assertion could reach. `as_str`'s catch-all-free match forces the author to
*handle* the variant, not to handle it safely; `#[non_exhaustive]` does not apply within the
declaring crate.

**The correction is in the declaration, not in the test.** `closed_named_enum!` generates the enum,
`ALL` and `as_str` from one list, so "present in the enum, absent from `ALL`" is no longer
expressible, and a data-bearing variant does not match `$variant:ident`. `DatabaseAdapter`,
`StartupPhase` and `DatabaseKind` — every rendered field of `StartupDiagnostic` — are declared
through it.

**Re-run against the declared form**, M-24b fails at macro expansion with *"no rules expected
`(`"*, before any test runs. M-24c is still killed, and is now the mutation the guard test exists
for: a genuine third adapter reaches `ALL` on its own and fails until somebody reviews the name.

The lesson generalises past this variant. **A mutation that a guard catches does not license the
claim that the guard catches the class.** M-24 was reported as closing the maintainer-reopens-it
hole; it closed one instance of it, and the ledger stated the general form.

## Second correction round — 2026-08-26

Mutations run against the six findings of the second automated review.

| # | Mutation | Result |
|---|---|---|
| M-28 | the ledger names a waiver identifier its own table does not grant | **KILLED** — `ledger refers to W-018, which its own table does not grant` |
| M-29 | the exception-count sentence understates the set by one, as it did | **KILLED** — `the ledger does not say 'All fifteen exist for the same underlying reason'` |
| M-30 | `renvor-sqlx::classify_error` emits `driver_error = %error` again | **KILLED** — `case 0 (classify_error/protocol) event 0 field 0 carried a planted secret into telemetry` |
| M-31 | the safe `adapter` field is dropped from the telemetry record | **KILLED** — `case 0 (classify_error/protocol) emitted no 'adapter' field` |
| M-32 | telemetry reports a kind other than the one returned | **KILLED** — `reported a kind in telemetry that differs from the one it returned` |

M-31 and M-32 are the **anti-vacuity** pair. A redaction test that only asserts absence passes
trivially against a function that emits nothing, and passes against one that emits a field
unrelated to what it returned. Both had to fail for the absence assertions to mean anything.

### Where M-27b actually fired, which is not where it was expected to

M-27b was predicted to fail **step 7**. It failed **step 4**, and the distinction is worth
recording: the resolved-graph check has an `xtask` unit test of its own, so `cargo test --workspace`
runs it before the sequence ever reaches step 7. The step-7 message was printed from inside step 4.

The mutation is killed either way, and killed **earlier** than predicted, which is the better
outcome. It is recorded as a corrected prediction rather than silently reported as the step-7 kill
it was expected to be.

**M-27b's first attempt is discarded, not reported.** Files were edited while it ran, so it was no
longer testing the tree it started on — the same mistake M-21's first attempt made, made again in
the same phase. It was re-run on a committed, unchanging tree and is reported from that run. That
first attempt did surface something real before it was discarded, and that finding is in
`phase-008-evidence.md` §7: adding a password literal to a documentation example pulled
`startup.rs` into the credential-handling scope, and ten of its assertion messages had to be
rewritten to name an index instead of a rendering.

### The `compile_fail` controls, and why each has a twin

M-27a is killed by a `compile_fail` documentation test. That construction has a well-known weakness:
it passes when compilation fails for **any** reason, including a typo in the snippet. Every
`compile_fail` block added in this cycle is therefore paired with a block that **compiles and
runs**, differing only in the thing under test — the argument type for the startup constructor, the
method name for the category projection. Without the twin, a `compile_fail` test is a green tick
that proves nothing.
