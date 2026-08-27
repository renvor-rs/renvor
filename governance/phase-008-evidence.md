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
[4/11] tests (four-row persistence census): ok — all 42 row-suite pairs reported in (11 required tests on each direct-SQLx row, 10 on each SeaORM row)
```

Five suites are compiled **once** in `renvor-testkit` and called by every row — ports contract,
domain example, concurrency and idempotency, portability, upgrade path. Two are per-adapter by
necessity, because they drive each adapter's own code: the startup diagnostic, which carries **two**
required tests (a refused socket and a refused credential reach different code), and **error
classification**.

**The rows are not symmetric, and the asymmetry is measured rather than overlooked.** Each
direct-SQLx row carries eleven required tests and each SeaORM row ten: provoking a deadlock needs
two sessions taking two row locks in opposite orders, which the direct-SQLx suite arranges over
`sqlx::Pool` and the SeaORM suite has no equivalent of. Two entries, therefore, and not four.
Writing the missing pair is work, and it is recorded as absent in §8 rather than implied by a
symmetrical-looking table.

**The count has changed twice, and both statements of it are still findable.** It was **24** when
this phase was first presented; the first correction cycle took it to **28** on finding C, adding
the server-side refusal row; the third took it to **42**, adding the error-classification rows on
the finding that `PLAN.md` §819's *"database error normalization"* deliverable had its only
real-database coverage outside the census entirely. Any earlier statement of "24 pairs" or "28
pairs" describes the head it was written against. Stated rather than quietly reconciled.

**The entries are derived from the suites, not counted by hand.** A new `xtask` unit test,
`every_error_classification_test_is_censused`, parses both `error_classification.rs` files and
fails if a test has no census row, or if a row names a test that is not there. That closes the
failure mode the census gap actually was: not a wrong number, but a table nothing compared against
the code.

**Step 1 now refuses a run without the databases.** The census used to print `ok — NOT RUN` when
`RENVOR_TEST_REQUIRE_DATABASE` was absent and return success, so a full `cargo xtask verify` could
exit **0** having executed none of the pairs — a conditional step in a sequence whose contract says
it has none. It is exit **2** with setup instructions now, and nothing is started automatically.

**Control.** With one row removed, `[4/11] tests: ok — passed` while
`[4/11] tests (four-row persistence census): FAILED`. The census caught what the suite could not.

**Re-run against the extended table** (M-38), with an error-classification row `cfg`-gated out of
its binary:

```text
[4/11] tests: ok — passed
[4/11] tests (four-row persistence census): FAILED — row
`renvor-seaorm::postgres::a_violation_never_carries_the_seaorm_text` did not report in.
```

And with the same test **deleted** rather than gated (M-37),
`cargo test -p renvor-seaorm --test error_classification` reports **`ok. 4 passed`** where it had
reported 6 — the disappearance, reported as success, in the suites backing a named deliverable.

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
  failed migration leaves a repairable database on one engine and a half-migrated one on the other
  — **and on MySQL every subsequent run is refused**, because SQLx writes its ledger row before the
  body and the implicit commit makes that row permanent. C-16 1.2.0 states the manual repair;
  §6c records why the instruction that stood there before was wrong.
- **Pagination.** NULLs sort to opposite ends on the two engines, so a cursor over a nullable
  column resumes elsewhere.
- **JSON.** A portable answer exists **for a measured subset**, not for the type. Inside that
  subset PostgreSQL `jsonb` and MySQL `JSON` normalise to **byte-identical** text, asserted as an
  equality with PostgreSQL's `json` as the control. Outside it the two engines disagree in three
  measured ways, one of which is silent data loss. C-16 1.2.0 carries the boundary; §6c records
  how much of it the review found and how much the measurement did.

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
- **M-24b** — the mutation M-24 did **not** run, found by the second review. Adding
  `Custom(&'static str)` and **omitting it from `ALL`** survived **59 of 59** tests, because `ALL`
  was a hand-maintained restatement of the variant list and every guard reads `ALL`. **This crate's
  claim that M-24 was killed was therefore too broad**, and the correction is in the declaration:
  `closed_named_enum!` generates the enum, `ALL` and `as_str` from one list, so M-24b now fails at
  macro expansion. The overstated claim is left in the ledger with its correction beside it.

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

## 6b. The second correction round — 2026-08-26

A second automated Codex review, run against the pushed head `0c88b39` **whose CI was 13/13 green**,
returned **REQUEST CHANGES** with one P1 and five P2 findings. All six were reproduced against the
tree before being accepted, and all six were dispositioned by change. The table is in
[`phase-008-review-record.md`](phase-008-review-record.md).

The consequential ones:

- **Raw driver text was reaching telemetry, and the contract written in this cycle said it could.**
  Both adapters emitted `driver_error = %error`; `contracts/error-taxonomy.md` 1.3.0 — text this
  cycle added — defended it as reaching *"operators rather than callers"*. `CONSTITUTION.md`
  principle VI names telemetry and exempts no consumer. The permission is **withdrawn at 1.4.0**,
  every raw-error field is removed, and each adapter now has one `record(kind)` helper with no
  parameter a driver message could arrive in. The mandated sweep found **five** such sites, not the
  three cited.
- **The M-24 kill claim was false.** See above. The evidence carrying it is corrected in place
  rather than rewritten.
- **Three documents stated things that were not true**: `xtask` still described the removed
  skip behaviour, `contracts/verification-sequence.md` still defined exit 2 as toolchain-only, and
  `governance/waivers.md` named an ungranted waiver, said *"Both were granted"* of one, and
  undercounted its own exception set by one.
- **W-017 counted two controls that PLAN §10.1 and FR-035 already mandate**, contrary to
  `GOVERNANCE.md`'s own rule. Both moved to explicitly uncounted preconditions.

**What this round says about the gates.** A 13/13 CI run and a first review round both passed a tree
carrying a P1 constitutional violation. Green gates measure what they were written to measure —
there was no test reading back what the adapters emitted, so nothing could notice. There is one now,
in both adapters, and it is mutation-proven against silence as well as against leakage.

## 6c. The third correction round — 2026-08-27

The final permitted automated round, run against the pushed head `ca479fc` **whose CI was 13/13
green**, returned **four P2 findings and no P1**. All four were reproduced before being accepted.
**None of round 2's six was raised again.** The table is in
[`phase-008-review-record.md`](phase-008-review-record.md).

Three of the four are against phase work the round-2 cycle never touched — the review reads the
whole branch diff — so they are findings about Phase 008 rather than regressions from correcting
it. The fourth is an absence in a file that cycle did touch, and the absence predates it.

### What the measurement found that the review did not

The JSON finding named one counterexample: PostgreSQL `jsonb` refuses a NUL escape that MySQL
stores. Answering it properly meant measuring the space rather than patching the sentence, so
**fourteen documents were run against both engines**. The counterexample is confirmed, and **two
further divergences appeared that the review had not named**:

| Document | PostgreSQL `jsonb` | MySQL `JSON` |
|---|---|---|
| `{"e":1E2}` | `100` | `100.0` |
| `{"n":1.50}` | `1.50` | `1.5` |
| `{"n":0.12345678901234567890123456789}` | exact | `0.12345678901234568` |

The third is **data loss**, not formatting: MySQL keeps non-integer JSON numbers as IEEE-754
doubles and PostgreSQL keeps every JSON number as `numeric`. An application moving a decimal
between the two supported engines gets a different number back, and the contract that this phase
shipped said the two types were interchangeable.

**Integers are exact on both**, right past 2^53 — measured, not assumed — which is why the portable
subset admits them and nothing else numeric.

The exclusions are asserted, not merely written down. `JSON_PROBES` executes every excluded
document with its measured divergence as the expected answer, so an engine that stops diverging
fails the gate and sends someone back to re-measure rather than leaving the contract quietly
narrower than the engines require.

### A test that passed for a reason other than the one it claimed

The concurrency deliverable was proven by four `ensure` callers driven with `tokio::join!`, and
`join!` orders nothing. The finding was that all four could return "found it" without any of them
racing — and every assertion the suite made would still hold.

That was reproduced as a fact rather than argued about. The four callers were run **strictly one
after another**, which is a total order `join!` is free to produce, and the **pre-fix assertions
passed 6 of 6 on both engines**: one creator, three observers, exactly one row, everyone inside the
retry bound. The correction is a `tokio::sync::Barrier` taken *after* the first `find` misses — not
before it, which would synchronise the wrong instant — plus three assertions about the **path** each
caller took. Against the same mutation those fail on both engines, naming
`[Created@1, Observed@1, Observed@1, Observed@1]` with zero refusals between them.

### An absence has no line number

`ROW_EVIDENCE` had no entry for either `error_classification` suite. Those suites are the only
real-database coverage of `PLAN.md` §819's *"database error normalization"* deliverable — where
not-null, check-violation and transaction-conflict classification are measured against servers
rather than against a fabricated driver error — so the census that exists to stop a row
disappearing did not cover the deliverable it was built for.

Fourteen rows were added, taking the census from 28 pairs to **42**, and a new `xtask` unit test
now derives the requirement from the suites themselves rather than from a number somebody typed.
The asymmetry it exposes is documented as `008/L-4` rather than papered over.

### What this round says about the gates, again

Nineteen gates on two toolchains, and a 13/13 CI run, passed a tree containing all four. None of
them is the kind of defect a gate catches: two are **false prose standing beside a passing test**,
one is a test that passes for the wrong reason, and one is a gap. The guards added here — a prose
guard, a probe table that executes its own exclusions, and a census derived from the code it
censuses — are each narrower than the class they belong to, and are recorded as such.

## 6d. The HttpError telemetry correction — 2026-08-27

Merged separately from PR #41, on its own branch, because it is a **contract change** and the
authority that governed the four-row corrections was database-scoped.

### What was wrong

`HttpError::new` took `detail: impl Into<String>`, and `refuse` emitted that string to `tracing`.
Both were **by design**: `contracts/problem-details.md` stated that *"the operator-facing detail
goes to the telemetry record, which is a different consumer with different rights"*, and
`renvor-error/src/code.rs` and `renvor-http/src/problem.rs` said the same in their own words.

`CONSTITUTION.md` principle VI forbids secrets in telemetry and **exempts no consumer**. This
phase's second correction round withdrew precisely that defence for the database adapters at
`contracts/error-taxonomy.md` 1.4.0, and reported this site as equivalent in shape and left it
standing. This closes it.

The exposure was not hypothetical in shape: an application author writing
`HttpError::new(kind, format!("could not reach {dsn}"))` put a DSN into a log, and the contract told
them that was the intended use. One site in the crate already rendered a runtime value —
`admission.rs` called `error.to_string()` on a `KernelError::ShuttingDown` whose `operation: String`
is chosen by whoever called the gate. That error is now **discarded**.

### The fix is at the declaration, not at the constructor

`HttpErrorDetail` is a fieldless enum generated by `renvor_core::closed_named_enum!`, with one
variant per refusal site in the crate. A caller cannot pass runtime text because there is no
parameter for it, and a maintainer cannot re-open the channel because a data-bearing variant does
not match `$variant:ident`.

`&'static str` was considered and is **not** sufficient: `Box::leak` promotes any `String` to
`&'static str`, so a lifetime is not a provenance. That is the same finding this phase recorded
against `DatabaseAdapter`.

**The macro is declared in `renvor-http` itself, and the first attempt to share it failed a gate.**
`renvor-database` carries the same construction and the two cannot be shared: it declares no
dependency on the kernel, deliberately, and `xtask` step 7 asserts that absence against the
resolved graph **with a positive control**, while `renvor-http` reaching `renvor-database` would
breach the transport/persistence isolation the same step asserts. There is no crate below both.

The macro was therefore placed in `renvor-core` and imported as `renvor_core::closed_named_enum!`.
That **compiles, tests and lints clean in a workspace build** — 103 suites, 1462 tests — and
**fails `cargo package --workspace` verification**:

```text
error[E0433]: failed to resolve: could not find `closed_named_enum` in `renvor_core`
  --> src/error.rs:39:14
```

What was established before changing the design, rather than after:

| Question | Answer |
|---|---|
| Is the macro itself sound? | **Yes.** A scratch crate `path`-depending on the packaged `renvor-core` compiles it |
| Is the tarball missing the file? | **No.** `closed_enum.rs` is in the `.crate`, and the packaged `lib.rs` declares the module |
| Is it a stale extraction? | **No.** Reproduced from a fully cleared `target/package`, registry `src/` and `cache/` |
| Does it resolve through the registry extraction by path? | **Yes**, which is what makes the failure specific to the packaged registry dependency |

The interaction was **not identified**. What was decided is what to do about it: the declaration
moved into the crate that uses it, where it needs no cross-crate resolution at all. That is also
the better design on its own terms — it keeps a `#[macro_export]` out of the kernel's public
surface for the sake of one consumer.

**The gate was not suppressed, and the failure is not a mystery left in place.** It is a design
that depended on a mechanism which does not survive the publication rehearsal, replaced by one
that does. The duplication is now two copies in two crates that architecturally cannot share,
generating independent types, so they cannot disagree about a value — only about which features
each macro offers. Stated in both files.

### What is proven, and by what

| Property | Proven by |
|---|---|
| the declaration admits no data-bearing variant | the `compile_fail` pair on the macro, and **M-41** |
| caller-supplied values reach no field of any emitted record | `a_refusal_records_nothing_the_caller_supplied`, with host, origin and path canaries driven through the **real router**; **M-42** |
| the record still carries its reviewed fields | `a_refusal_still_records_its_code_and_a_reviewed_detail`; **M-43** |
| `Display` renders only closed values, and still renders them | `the_detail_is_still_present_distinct_and_reaches_display`; **M-44** |
| the source chain terminates and adds nothing | `the_error_chain_terminates_and_adds_nothing`; **M-45** |
| the response body echoes nothing the caller supplied | `a_refusal_body_carries_nothing_the_caller_supplied` |

### Gates for this correction

Run against the final head `3e101b0` on **rustc 1.94.0** and **rustc 1.97.1**, sequentially, with
the real four-row database environment.

**22 gates, 22 passed, 0 failed.**

| Gate | 1.94.0 | stable |
|---|---|---|
| `cargo xtask verify` | **11/11** | **11/11** |
| workspace tests, all features | **102 suites, 1460 passed, 0 failed** | **102 suites, 1460 passed, 0 failed** |
| serial workspace tests | **102 suites, 1460 passed, 0 failed** | **102 suites, 1460 passed, 0 failed** |
| no-default / default / all-features | pass | pass |
| rustdoc, warnings denied | pass | pass |
| `cargo deny check` | ok | ok |
| package + publish rehearsal, **no publication** | 11 uploads, **11 dry-run aborts** | 11 uploads, **11 dry-run aborts** |
| `git diff --check` | pass | pass |

The census reports **42 row-suite pairs** on both toolchains, unchanged by this correction.

#### The run before it reported 21 of 22, and that run is not erased

Head `015950a` — the same tree without the compiling control — failed one gate:

```text
GATE FAIL [1.94.0 serial tests]
test no_command_in_this_phase_modifies_the_trust_store ... FAILED
panicked at crates/renvor-cli/tests/tls_consent.rs:154:5
test result: FAILED. 8 passed; 1 failed
```

That is **F-3**, the open defect in `renvor-cli`'s trust-store snapshot, with nothing to do with
`HttpError`. Its stable leg passed the same test on the same machine, and both serial legs of the
final run passed it too — the trust-store test is present and `ok` in all of them.

**Three passing runs do not close F-3, and none is offered as doing so.** This phase's rule is that
an unchanged rerun passing is the definition of a flake rather than evidence against one. F-3 stays
open, unwaived, un-quarantined, with its **2026-09-02** deadline unmoved, and the recurrence is
recorded in [`phase-008-limitations.md`](phase-008-limitations.md).

The rerun happened because the tree **changed** — the compiling control was added after the first
run — so reporting the first run's numbers for the final head would have been reporting a
source-modified run. It is the added test that makes the count 1460 rather than 1459.

### An earlier gate run failed, and it changed the design

The first attempt reported **14 of 22**, with `verify`, `rustdoc`, `package` and `publish` failing
on both toolchains. Two distinct causes, both real:

- a doc link in the kernel macro pointed at a private module — rustdoc's
  `private_intra_doc_links` under `-D warnings`, fixed;
- the cross-crate macro did not resolve in the packaging rehearsal, which is what moved the
  declaration into `renvor-http`. The investigation is above.

**The test counts moved with that decision**, and the difference is accounted for rather than left
to look like drift: the exported version added three doctests to `renvor-core` (a syntax example
and the two controls), so the workspace read 103 suites / 1462 tests. Making the macro private
removed them — doctests do not run for private items — giving 102 / 1459. The compiling control was
re-added as a unit test, `a_unit_variant_reaches_all_on_its_own`, and the failing control is **M-41**
in the ledger. Neither half of the pair was dropped; one of them stopped being automatic, and that
is stated on the macro itself.

### Two things found on the way, both reported rather than absorbed

**A 404 emits no telemetry at all.** `not_found` answers through `problem::from_http_error` and is
the one refusal in the crate that does not pass through `refuse`. A test assumption of this
correction's was that it did, and the test failed on it. The case is **excluded from the tracing
assertions on that measurement**, named in the test file, and kept in the response-body assertion
where it still applies. It is pre-existing behaviour, outside this authority, and is not fixed
here.

**The credential-diagnostics gate caught three violations in these very tests** — assertion messages
interpolating `{detail:?}` and a bare positional count. That is the **fourth** time in this phase
that gate has caught this class, and the first time in a file whose whole subject is redaction. The
messages now name the case by index. A redaction test that printed the offending rendering would
copy a leaked value into CI output on exactly the run where a leak had just been found.

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
- The first pre-merge gate run of the correction cycle **FAILED at rustdoc** (`-D warnings`):
  `redundant explicit link target` in `crates/renvor-database/src/startup.rs`, twice. Importing
  `DatabaseError` into that module — which correction B required, so the diagnostic could hold its
  cause — put the type in scope, and `[`DatabaseError`](crate::DatabaseError)` became redundant.
  One of the two had been there since the module was written and only became an error when the
  import landed. Fixed by removing both explicit targets. **The failed run stays in this record**;
  the rerun passing is not a reason to delete it.
- **The second correction round's first gate run FAILED on both toolchains**, at two independent
  gates, and both failures were in the files added to *fix* the P1:
  - **Step 4 / `renvor-core`'s credential-diagnostics gate.** Both new
    `telemetry_redaction.rs` files plant `hunter2CanaryDoNotLeak`, which puts them in the gate's
    scope, and six of their assertion messages broke its rules — four interpolating `event_index`,
    a name absent from the `PERMITTED` allowlist, and two using a **bare positional `{}`**, which
    the gate always refuses because it reads text and cannot see what expression fills the slot.
    **This is the THIRD time in this phase that this gate has caught this class**, and the first
    time it caught the positional form. Fixed by renaming to `position` and naming the count.
  - **Rustdoc, `-D warnings`.** `redundant explicit link target` at `lib.rs:87`. Removing the
    explicit target everywhere then produced the opposite error — `unresolved link` — at
    `startup.rs:69` and `:88`. The asymmetry is real: a doc comment passed **through a macro
    invocation** resolves a bare `[`closed_named_enum`]` at the crate root but not in a submodule,
    while an ordinary doc comment in the same submodule resolves it fine. The explicit path is kept
    exactly where the expansion needs it and dropped where it is redundant.

  **Both failures stay in this record.** The rerun on the corrected tree is reported separately;
  the run that failed is not deleted because it passed the second time.
- **The second gate attempt FAILED at step 4 for reasons that were entirely the test ENVIRONMENT**,
  and it is recorded because a reader comparing logs would otherwise see an unexplained red run:
  - Eight MySQL migration tests reported `ConnectFailed`. The suites `CREATE DATABASE` per test,
    and the container had been started with the official image's `MYSQL_USER`, which holds rights
    on `MYSQL_DATABASE` only. The create was refused and the connect to the absent database is what
    surfaced.
  - `postgres::a_failed_boot_publishes_nothing_and_leaks_no_credential` reported *"the password
    reached a diagnostic"* — and **nothing had leaked.** The container password was `renvor`, the
    test extracts the real password from the DSN and substring-searches the rendered diagnostic,
    and that diagnostic legitimately renders `renvor-seaorm`. A false positive, and the test is
    right to fail closed on it: a substring search cannot distinguish a leak from a password that
    happens to be a substring of a safe token.

  **Neither was a defect in the tree**, and both are now documented in `CONTRIBUTING.md` so the
  next person setting up the four rows does not spend a gate run rediscovering them. The run is
  reported as failed rather than quietly rerun.
- **One gate run was killed mid-step-4 by the harness and is DISCARDED, not reported.** Nothing was
  edited and the tree was unchanged, so it is neither a pass nor a failure — it is an incomplete
  measurement, and the same treatment M-27b's interrupted first attempt received.
- **The first review round and a fully green CI both passed a P1.** The tree at `0c88b39` had
  13/13 CI checks green, ten round-one findings dispositioned, and both adapters copying driver
  error text into telemetry against a constitutional prohibition. It is recorded here rather than
  only in the review record because the lesson is about this evidence pack: a phase that reports
  "all gates green" has reported what the gates cover, and this one did not cover telemetry output
  until the second round forced it to.
- The packaging rehearsal **FAILED twice with a cargo internal panic** on the first two gate runs,
  and the cause was **the gate, not the tree**. It had been written as `cargo package -p <crate>`
  per crate, which cannot work here: an unpublished path dependency has nothing to resolve against,
  and `release-dry-run.yml` had already recorded exactly that — *"`cargo publish -p renvor
  --dry-run` fails with no matching package"*. The failed per-crate attempts then left conflicting
  artifacts in `target/package`, and the corrected workspace-form command panicked on **them**.
  From a cleared `target/package` the workspace forms exit **0** on both toolchains. Recorded
  because a reader comparing run logs would otherwise see two unexplained `101`s.
- `renvor-cli/tests/tls_consent.rs` failed once and passed unchanged on rerun (**F-3**). Out of
  scope, **unresolved**, and **the rerun does not close it**. It now has an owner (Ahmed Anbar), a
  target (a dedicated follow-up before Phase 009), and a deadline (**2026-09-02**); the test
  remains enabled and release coverage is preserved. See
  [`phase-008-limitations.md`](phase-008-limitations.md).
- One earlier attempt at gate-level red evidence was **abandoned rather than reported**, because
  sources were edited while it ran.

## 7a. Pre-merge gates, both toolchains

Run on **rustc 1.94.0** (the MSRV floor) and **rustc 1.97.1** (stable at the time of the run),
against the two pinned images `postgres:17.11-trixie` and `mysql:8.4.11`.

| Gate | 1.94.0 | stable |
|---|---|---|
| focused tests for every correction | pass | pass |
| `cargo test -p xtask` | pass | pass |
| full workspace tests, all features | **99 suites, 1442 passed, 0 failed** | **99 suites, 1442 passed, 0 failed** |
| serial workspace tests (`--test-threads=1`) | pass | pass |
| no-default / default / all-features checks | pass | pass |
| per-driver adapter compiles (4 combinations) | pass | pass |
| rustdoc, warnings denied | pass | pass |
| `cargo deny check` | advisories, bans, licenses, sources all ok | same |
| package + publish rehearsal, **no publication** | pass | pass |
| `git diff --check` (tree and index) | pass | pass |
| `cargo xtask verify`, real four-row environment | **all 11 steps ran and passed** | **all 11 steps ran and passed** |

The census line, from the verify run on each toolchain:

```
[4/11] tests (four-row persistence census): ok — all 28 row-suite pairs reported in (4 rows x 7 required tests)
```

The previous head reported **98 suites / 1428 passed**; the corrections added fourteen tests, four
of them the new server-side refusal rows.

## 7b. Pre-merge gates, third correction round — 2026-08-27

Run against head `42d6122` on **rustc 1.94.0** (the MSRV floor) and **rustc 1.97.1** (stable at the
time of the run), sequentially, against the two pinned images `postgres:17.11-trixie` and
`mysql:8.4.11`.

**22 gates, 22 passed, 0 failed.**

| Gate | 1.94.0 | stable |
|---|---|---|
| `cargo xtask verify`, real four-row environment | **all 11 steps ran and passed** | **all 11 steps ran and passed** |
| workspace tests, all features | **101 suites, 1454 passed, 0 failed** | **101 suites, 1454 passed, 0 failed** |
| serial workspace tests (`--test-threads=1`) | **101 suites, 1454 passed, 0 failed** | **101 suites, 1454 passed, 0 failed** |
| no-default / default / all-features checks | pass | pass |
| rustdoc, warnings denied | pass | pass |
| `cargo deny check` | advisories, bans, licenses, sources all ok | same |
| `cargo package --workspace` | pass | pass |
| `cargo publish --dry-run --workspace`, **no publication** | 11 uploads, **11 dry-run aborts** | 11 uploads, **11 dry-run aborts** |
| `git diff --check` | pass | pass |

The census line, from the verify run on **each** toolchain:

```text
[4/11] tests (four-row persistence census): ok — all 42 row-suite pairs reported in (11 required tests on each direct-SQLx row, 10 on each SeaORM row)
```

The previous head reported **101 suites / 1450 passed**. This round adds four tests: the partial-
migration assertion on each of the two engines, and two `xtask` guards — the census-derived-from-
the-suites check and the recovery-prose check.

**Neither `package` nor `publish --dry-run` was run with `--allow-dirty`.** Every "Uploading" line
in both dry runs is followed by `aborting upload due to dry run`, 11 for 11 on each toolchain, and
`crates.io` returns **404** for the crates. Nothing was published.

### The two runs before this one, which are part of the record

**Attempt 1 — DISCARDED, not reported.** It was launched with `--allow-dirty` on `package` and
`publish --dry-run`, which would have let a dirty tree past two gates that exist to catch one. It
was stopped about a minute in, at step 1 of the first toolchain, before producing any result. A
run that was stopped is an **incomplete measurement**, and it is recorded as discarded rather than
counted either way — the same disposition M-21 and M-27b's first attempts received.

**Attempt 2 — FAILED, and the failure was real.** Step 5 refused the tree:

```text
error: public documentation for `concurrency` links to private item `race_for`
error: public documentation for `concurrency` links to private item `ensure`
   = note: `-D rustdoc::private-intra-doc-links` implied by `-D warnings`
[5/11] API documentation: FAILED — `cargo` exited with 101
```

The module documentation added for finding 3 linked to two private functions. Fixed in `42d6122`
by anchoring the note on the two **public** assertions instead, and the run was restarted from the
beginning rather than resumed. **The failure stays in this record even though the rerun passed**,
per this phase's own rule that a failed first run is not erased by a green second one.

Attempt 2 did reach step 4 before failing, and its census line already read **42 row-suite pairs**
— so finding 4's fix was measured on a tree that then failed a different gate. That is stated
rather than used: the reported result is attempt 3's, in full, on both toolchains.

## 8. Limitations and inherited work

Full dispositions: [`phase-008-limitations.md`](phase-008-limitations.md).

Two limitations targeted at Phase 008 are **not closed by it**, and both are now **retargeted to
Phase 013**, owner **Ahmed Anbar**, each with an obligation to implement or explicitly exclude from
REST 1.0:

| Limitation | What is missing |
|---|---|
| `006/L-7` | mixed-direction keyset pagination — lifting the refusal requires a public API change, because the binding count lives inside `seek_predicate`'s `String` |
| `007/L-11` | `TransactionTrait`, savepoints, and isolation-level configuration are not exposed |

A third is **created by this phase's final correction round and left open deliberately**:

| Limitation | What is missing |
|---|---|
| `008/L-4` | **transaction-conflict classification is measured on the two direct-SQLx rows only.** `renvor-seaorm/tests/error_classification.rs` has no deadlock test, so `TransactionConflict` is asserted for the SQLx adapter and inferred for the SeaORM one. The census records two entries rather than four, and this records why |

It is a limitation rather than a defect: the classification path a SeaORM deadlock takes is
`DbErr` → `RuntimeErr::SqlxError` → the same driver-level mapping the direct rows exercise, so the
inference is well-founded. It is nevertheless an inference, and the parity claim these suites exist
to make is *measured* everywhere else. Owner **Ahmed Anbar**; to be closed by writing the test, not
by widening the claim.

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

**Two waivers have been granted**, by explicit authority and each narrowly:

| Waiver | Waives | Does **not** |
|---|---|---|
| **W-017** | the independent-human-review requirement for accepting **ADR-0023** | close Phase 008 |
| **W-018** | the independent requirements-and-security review required to **close Phase 008** | cover ADR-0023, or any defect, failed check, or missing acceptance criterion |

Both expire **2027-02-11**, or immediately when a qualified independent human reviewer becomes
available — whichever comes first.

That is **exactly two** reviewed exceptions for Phase 008 — this ledger's stated maximum, at the
limit and not over it — and W-018 is the **eighth consecutive** phase-level waiver of the same rule
for the same reason. **RO-001's 2026-11-19 review date is unchanged, and no recruitment progress of
any kind has occurred.**

The two temporary edit-link suppressions are removed in the same closure pull request, and both
pages' edit links were verified to resolve with a nonexistent-page negative control. **No lychee
exclusion was ever created for them**, which is what the suppression existed to avoid.
