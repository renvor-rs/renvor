# Phase 007 — Evidence

**Date**: 2026-08-24
**Phase**: 007 — SeaORM parity
**Base**: `28c6da37a29831304498da6da819e4952f2e9ea2` (live main, Phase 006 merged)
**Closure authority**: [W-016](waivers.md). **No independent human review occurred, and none is
claimed.**

---

## 1. What this phase added

A second persistence **programming model**, not a second database stack.

| | Direct SQLx (Phase 006) | SeaORM (Phase 007) |
|---|---|---|
| Crate | `renvor-sqlx` | `renvor-seaorm` |
| Application writes | SQL, every value bound | entities and a query builder |
| Ports implemented | `renvor-database`'s | **the same ones** |
| Migration engine | SQLx | **the same one** |
| Driver | SQLx | **the same one, transitively** |

## 2. The four-row matrix, and why "the same contracts" is checkable

`PLAN.md` §Phase 007 accepts the phase only when *"both SeaORM rows pass the same application
contracts as direct SQLx"*. **The same** is the load-bearing word, and two suites that agree today
diverge the first time one is edited — quietly, because a weakened assertion still passes.

So the assertions live in `renvor_testkit::persistence`, are compiled once, and are called from both
adapters' test binaries. Each adapter supplies only four driver-specific operations.

```
════ PostgreSQL 17.11 + MySQL 8.4.11
   ok    direct SQLx   4 passed
   ok    SeaORM        4 passed
════ PostgreSQL 18.6  + MySQL 9.7.2
   ok    direct SQLx   4 passed
   ok    SeaORM        4 passed
```

**The matrix earned its keep immediately.** The first SeaORM run failed on PostgreSQL and passed on
MySQL, because the insert used `?` for both backends and `Statement::from_sql_and_values` passes SQL
through to the driver rather than rewriting it. A single-engine suite would have shipped that.

## 3. The cancellation gate — measured, and stated precisely

`sea_orm::DatabaseTransaction` cannot satisfy the Phase 006 contract. Measured against real engines:

```
NATIVE sea_orm::DatabaseTransaction on mysql:    capacity STRANDED for 9.506036542s
NATIVE sea_orm::DatabaseTransaction on postgres: capacity STRANDED for 9.509529791s
```

Renvor's bound on the same pool, same cancellation, is **2 s**, and it holds on both.

**This is stranding, not leaking, and the difference is recorded rather than flattened.** The slot
returned once the abandoned ten-second sleep finished server-side. ADR-0017 measured the
*permanent* form at 0-in-12 on PostgreSQL, so an unqualified "the native path leaks" would have
contradicted the project's own earlier evidence. It fails the contract either way: ~9.5 s of denial
per cancelled request is neither bounded by anything Renvor configures nor deterministic.

An earlier version of that test reported "capacity unavailable in 6 of 6 rounds", which was true and
**would have overstated the case**. It was replaced with a duration before it reached this file.

The decision rests on a structural argument rather than the measurement — see
[ADR-0021](../decisions/0021-renvor-owns-the-seaorm-unit-of-work.md). SeaORM keeps its connection in
an `Arc<Mutex<_>>`, so `Drop` can only `try_lock` and its failure path is `expect` — a panic
reachable from `Drop`, which during an unwind aborts. Renvor's unit of work is uniquely owned, so
`Drop` uses `Mutex::get_mut()`, which cannot fail.

## 4. `sea-orm-migration` was evaluated and rejected on a capability gap

Its entire bookkeeping model:

```rust
#[sea_orm(table_name = "seaql_migrations")]
pub struct Model {
    pub version: String,
    pub applied_at: i64,
}
```

**No checksum**, and the string does not appear anywhere in the crate's **source** — only in
its vendored `Cargo.lock`, where it is a registry hash field. `PLAN.md` §12 requires
migrations to be *"ordered, checksummed, observable, and safe under concurrent startup"*.

Renvor runs SQL-file migrations through SQLx's engine for **both** models, so a project has exactly
one history whichever ORM it selected. The cost — no Rust-authored `MigrationTrait` migrations — is
recorded as L-12 rather than absorbed. See
[ADR-0022](../decisions/0022-one-migration-history-on-sqlx-engine.md).

## 5. Gates that refused something

A gate that has never refused anything is not evidence that it works.

| Gate | What it caught |
|---|---|
| `xtask` publishable-package count | Reported the new crate **before** any manual list was edited — the outcome it was pinned for after Phase 005 and Phase 006 both found it late |
| Generator pre-placement `cargo fmt --check` | Refused the first `src/repository.rs` template on an import block |
| Generator pre-placement `cargo build` | Refused a `main.rs` that declared `mod entity;` against a manifest with no `sea-orm` — which is what forced the offline decision below |
| Wizard question-count guard | Refused the ORM question until it was reviewed against FR-049 and its count updated deliberately |
| `trycmd` command-surface snapshot | Refused the `--orm` help-text change until the recorded contract was updated |
| Four-row matrix | The PostgreSQL placeholder defect in §2 |
| Feature-isolation positive control | A defect in the **measuring harness itself**, which was building its pattern with a doubled space and reporting a control as absent |
| `the_active_waiver_counts_match_the_waiver_table` | Verified red-first: the count was changed to a wrong value, the test failed with the right message, the value was restored |

## 6. A real dependency was designed, built, and withdrawn

The generated SeaORM project was first written to declare `sea-orm = "2.0.2"`, so `src/entity.rs`
and `src/repository.rs` would compile. That is better on its own terms and it was **wrong**, because
generation runs the staged project's own `cargo fmt`, `clippy`, `build`, `test` and `run` before
placing it — so a real dependency puts a registry fetch and a multi-minute compile inside
`renvor new`, and Renvor guarantees offline generation.

The dependency was removed, the two files are generated in full but not declared as modules, and
`seaorm_generation_succeeds_offline` pins the property with `CARGO_NET_OFFLINE=true`. Recorded as
L-13, with the two lines an operator adds to change it.

Because the generator's own formatting gate cannot see undeclared files, a separate test runs
`rustfmt --check` over them. That gap is stated rather than left implicit.

## 7. The governance defect this phase's own audit found

Stage 0 of Phase 007 required a truth audit before implementation. It found that **the active-waiver
count had been wrong since Phase 006**: `governance/waivers.md` said eleven while its table carried
thirteen, and `GOVERNANCE.md` said eleven **and was missing both W-013 and W-014 rows entirely**.

The identical defect was corrected on 2026-08-21, when the headline said six and the table carried
seven. On that occasion `GOVERNANCE.md` was correct and served as the cross-check. This time it was
not, so nothing caught it.

Corrected in its own commit (`827e576`), with no waiver's scope, controls, expiry, or removal plan
touched, and now asserted by a test that was verified to fail before it passed.

**A ledger that miscounts its own waivers is the clearest available evidence that self-review has a
ceiling.** It is recorded against RO-001 for that reason.

## 8. Independence — stated plainly

**No independent human review of Phase 007 occurred.** Five reviews were commissioned and are
**advisory**; they are automated, they are not independent, and they are not counted as the review
`PLAN.md` §6.1 step 10 requires. Their findings and dispositions are in §9.

Phase 007 is the **seventh consecutive** phase closed under a waiver of that same rule, for the same
reason. RO-001's first review date remains **2026-11-19** and has not moved.

## 9. Review findings and dispositions

### The first attempt is recorded as NOT PERFORMED

Five review agents were commissioned and **all five died on an account session limit** before
returning anything. That is recorded here as **NOT PERFORMED**, not as "no findings" — an empty
result is the absence of a review, and Phase 005 already shipped once with three empty reviews
described as if they had run.

They were re-commissioned. The five dimensions the phase brief names — package/dependency,
requirements conformance, security, validation, and governance/evidence truthfulness — were covered
by **four** agents, because package/dependency and governance/evidence were given to one reviewer
together. That is a deviation from "one agent per dimension" and is stated rather than glossed; the
combined brief carries both dimensions in full.

### Validation review — 2 findings, both confirmed and fixed

**V-1 (Moderate) — a documented past defect, reproduced.**
`crates/renvor-seaorm/src/version.rs` gated `compile_guard_connection_is_send` on `db-postgres`
only, so a **MySQL-only build compiled no guard at all** for the property the file exists to
protect. `renvor-sqlx/src/migrate.rs:266-271` already records making and fixing this exact mistake:
*"The guard was written once, under `#[cfg(feature = "db-postgres")]`… in a `db-mysql`-only build
the guard was not compiled at all."* Nothing else forces the proof for MySQL — `initialise` reaches
the database through `sqlx::Executor`, never `ConnectionTrait`.

**Disposition: fixed.** Split into `_postgres`/`_mysql` halves. All four feature configurations
still compile.

**V-2 (Moderate) — a shared assertion that passed under the bug it named.**
`renvor_testkit::persistence::a_second_begin_is_separate` asserted only that the outer write was
invisible from a **third** connection. Both engines hide an uncommitted write from every other
session, so a second `begin` that reused the first's session — on PostgreSQL a second `BEGIN` on an
open transaction emits a notice and continues the *same* transaction — would still look correct.
The adapter-specific tests in `renvor-sqlx/tests/ports.rs` and `renvor-seaorm/tests/cancellation.rs`
were already doing this properly; only the **shared** version, the one both adapters rely on, was
weak.

**Disposition: fixed.** `PersistenceFixture` gained `count_within`, which reads through the
transaction's own connection, and the assertion now reads from inside `inner`. A **positive
control** was added — the outer transaction must see its own write — because a `count_within` that
quietly read the pool would otherwise satisfy the real assertion while measuring nothing.

Verified red-first by mutation: `count_within` was rewritten to read the pool, and the control
failed on **both** engines with `left: 0, right: 1`. Restored, green.

**Reported clean, with the checks named:** deadlock/re-entrancy (no `TransactionTrait`/`StreamTrait`
impl, so no re-entrant path to the same mutex); `Drop` via `get_mut()`; `begin()`'s failure path;
`close()` (byte-identical to the shipped `renvor-sqlx` one); `migrate.rs` diffed against
`renvor-sqlx`'s with no behavioural difference; `provider.rs` closing the pool on every failure
path and publishing only after migrations; `run_every_shared_assertion` covering every assertion
bar the deliberately-excluded one; CLI flag combinations; all four feature configurations, run.

The reviewer also **compiled the generated SeaORM project** against real `sea-orm 2.0.2` after adding
the dependency and the two module declarations by hand — it built clean. That substantiates
`templates.rs`'s claim that the two files are real rather than decorative, and it confirms L-13's
shape: the only thing missing is an automated compile gate, which offline generation forbids.

### Requirements conformance review — 22 findings, 3 Critical

All confirmed. The headline verdict was that **"62 of 62 SATISFIED" was not supportable under the
file's own legend**, and it was correct.

| | Finding | Disposition |
|---|---|---|
| **R-1** | Six FRs cited `isolation.sh`, which **existed only in a scratch directory that had already been deleted** | **Fixed.** Rewritten as `persistence_isolation_holds`, an `xtask` step-7 gate plus a fast test. Red-checked both ways: forbidding a present crate fails; requiring an absent control fails with "proves nothing" |
| **R-2** | FR-033/FR-034 claimed seed and pagination parity that **did not exist** — no `page.rs`, no `seed.rs` in `renvor-seaorm` | **Fixed, structurally.** The renderers and the seed types moved into `renvor-database`, so both adapters share **one implementation**; seeding is asserted by `persistence::seeding_honours_scope_and_idempotence` on all four rows |
| **R-3** | SC-003's native-SeaORM half asserts nothing, and a comment claimed a null-probe safety net that does not hold | **Fixed.** The false claim is replaced with what is true, naming ADR-0021's structural argument as what carries the decision |
| R-4 | FR-056 said "exactly three features"; the manifest takes five | **Requirement corrected**, not the manifest — the three type features match `renvor-sqlx`'s |
| R-5..R-11 | FR-006, FR-024, FR-045, FR-048, FR-052, FR-035, FR-001 named checks that did not exist or were too narrow | **Fixed** — each gained a real test; the interpolation scan went from 2 SQL verbs to 12 |
| R-12..R-22 | Weaker evidence relabelled | **Relabelled**, not fixed: the record now reads **53 SATISFIED / 4 STRUCTURAL / 5 ARGUED**, with `ARGUED` defined as *no executable check* |

The reviewer also verified independently that the six properties behind R-1 **all hold** — only the
cited evidence was missing. And it reported its own methodology defect: its first `cargo tree` pass
used `timeout`, which does not exist on macOS, so every command failed and the piped `grep -c`
reported `0` — indistinguishable from "the forbidden crate is absent". It caught and re-ran them.

### Security review — 4 Medium, 4 Low, no Critical or High

| | Finding | Disposition |
|---|---|---|
| **S-3** | The `AssertSqlSafe` justification was **factually wrong** — it claimed `Statement.sql` is built by SeaQuery, but `Statement::from_string` takes arbitrary text with no values | **Fixed** in three places, and the escape-hatch ladder now names `from_string` and the stacked-statement blast radius |
| **S-4** | The crate's only redaction test **could pass vacuously** — an unextractable password made it a no-op — and `CREDENTIAL_CANARY` was referenced by no test | **Fixed.** Fails closed on an unextractable password, wires the canary, and renders all five `Debug` impls rather than one |
| S-2 | The `run_direct` guard cannot detect a repurposed `skip` bool | **Claim narrowed**, and the gap measured: flipping the literal is **killed by six tests** on both engines |
| S-7 | `unwrap_or_default()` on the bookkeeping read **fabricated migration reports** | **Fixed** — SQLSTATE `42P01`/`42S02` distinguished from real failures |
| S-1, S-5, S-6, S-8 | A claimed gate; telemetry scope; understated blast radius; narrow template scan | S-1 was already fixed before the report arrived; the rest widened |

It confirmed by tracing rather than assertion that **no credential reaches even the `tracing::debug!`
line**, and that `Drop` has no abort path — while correctly noting the *stated* reason was
incomplete, since `detach()` reaches an `expect` in SQLx that is unreachable for a different reason.

### Dependency and governance review — 11 findings, 1 Critical

| | Finding | Disposition |
|---|---|---|
| **D-1** | **The publication order was topologically invalid, and this branch broke it.** Making room for `renvor-testkit` moved `renvor-database` past `renvor-validation`, which it depends on | **Fixed**, and gated: `publication_order_is_topological` reads `cargo tree`, red-checked against the exact broken list. The workflow's own assertion **sorts both sides**, so it could never have caught this |
| **D-4** | W-015/W-016 cited compensating controls that run unconditionally — which this ledger's own rule forbids, and which W-012 had explicitly excluded | **Fixed.** Those controls are replaced with ones specific to the review gap, and the omitted preconditions clause is restored |
| **D-5** | W-015/W-016 had **no scope sections**, unlike every waiver from W-003 to W-014 | **Fixed** — both now state what they do and do not authorise |
| **D-6** | `RELEASING.md` said "Eight publishable packages" while its own table listed eleven — **the third occurrence** of this project's recurring failure, in the one file the count test did not cover | **Fixed and gated** by `publishable_package_count_is_stated_correctly`, red-checked |
| D-8, D-9, D-10, D-11 | "78 entries" was the wrong unit (87 entries, 78 names); "anywhere in the crate" overreached by one vendored file; a stale phrase; comment ordering | All corrected |
| D-2, D-3, D-7 | Closed by work already in the tree | Confirmed |

It verified the ADR-0021 and ADR-0022 upstream quotes **line by line** against vendored source and
found them verbatim, confirmed the stranded-versus-leaked distinction agrees exactly with ADR-0017,
and confirmed the waiver counts are now internally consistent across both files.

**One structural point it raised that outlives this phase.** `.gitignore` excludes `/specs/`, so the
entire Phase 007 evidence pack is untracked and **no reviewer can fetch it from the repository**.
That is the standing instruction for this project and is not changed here — but a waiver should not
rest on a file nobody else can read, so W-016's control now says so, and the tracked summary is this
document.

### Found after the review round, during the final gates

Two defects surfaced during Stage 9 rather than from any review. Both are recorded here because
finding them late is not a reason to report the phase as though the reviews had found everything.

**G-1 (Medium) — the credential-diagnostic gate could not see four of the files it exists for.**
`renvor-core/tests/diagnostics.rs` selects "credential-handling" files by searching for one of four
**literal** canary strings. The persistence suites do not inline a canary; they reference the shared
constant that `tests/support` exports. Four files that plant a password and render the refusal were
therefore outside the scan entirely — `renvor-seaorm/tests/migration.rs`,
`renvor-sqlx/tests/{connectivity,provider,seed}.rs`. The allowlist inside the gate fails **closed**
on an unanticipated name; its scope selector failed **open** on an unanticipated file.

Widening the needle set exposed six real offences, one of which printed the secret itself
(`provider.rs`: `"the boot failure leaked \`{secret}\`: {rendered}"`). Two were introduced by this
phase; the other four are inherited from Phase 006 and were never in scope to be caught.

**Disposition: fixed.** The constant's *name* is now a needle, the six diagnostics report an index
or a fixed message, and `round` joined the allowlist as the loop counter it is. A fourth positive
control asserts that at least three files reach the gate **through the constant alone** — a count
floor could not do this, because 21 files already matched the literals and the existing floor is 8,
so deleting the new needle would have silently restored the blind spot with every assertion still
passing. Red-checked: with the needle removed the control fails with *"only 0 files reach this gate
through the shared constant alone"*.

One consequence worth stating: the needle had to be written as a split literal, like the four
before it, because spelling it whole put the gate file into its own scope — where the synthetic
controls that interpolate on purpose are offences.

**G-2 (Medium) — a wrong password was reported as a rejected statement, in both adapters.**
`SeaOrmDatabase::connect` and `SqlxDatabase::connect` both document
*"`ConnectFailed` when the connection could not be established"*. Both returned
**`StatementRejected`**. A server that refuses the handshake — wrong password, unknown database,
connection limit — reports it as `sqlx::Error::Database`, and the general mapper classifies that by
SQLSTATE, landing on `StatementRejected`: a statement the caller never sent, sending an operator to
look for SQL when the answer is a credential.

Found by mistyping a DSN while re-running the suites, not by a review. It is a **documented-contract
violation** rather than a missing nicety, and it was **inherited** — `renvor-sqlx` shipped it in
Phase 006 and `renvor-seaorm` reproduced it, so the two adapters agreed with each other and
disagreed with their own documentation. The existing wrong-credential control asserted only that
the credential does not leak, never which kind came back, which is why nothing caught it.

**Disposition: fixed in both adapters**, symmetrically, so parity is preserved. `classify_connect_error`
maps a server-side refusal at the *connect call site* to `ConnectFailed` and defers everything else
to the general mapper. The distinction is deliberately the call site and not the SQLSTATE: mid-session
an authorization error really is a rejected statement, so this is not folded into `classify_error`.
Red-checked on both rows — `left: StatementRejected, right: ConnectFailed` before, green after.

**G-3 (Low) — the link check enforces a post-merge invariant on a pre-merge tree.**
Three links failed on the candidate head, all for the same reason: they point at files that exist
only on this branch. Two were **authored** citations of ADR-0021 and ADR-0022 written as
`blob/main/decisions/...`; one is **generated** — Docusaurus builds every page's *"Edit this page"*
link from `editUrl`, so a page added by a pull request 404s its own source until that request merges.

**Disposition: split by cause, following this repository's own two precedents.** The authored links
were fixed at the source and are now repository paths in code form, which is what `lychee.toml`'s
retired **EX-005** records as the right answer — an exemption for an authored link would enter
`main` and permanently stop checking it. The generated link cannot be rewritten from inside the
page, so it is exempted as **EX-006**, anchored at both ends to that one URL. That is **EX-004
again**: the same exemption existed for `cli.mdx`, was added by PR #28, and was removed on
2026-08-21 once the page reached `main`.

The exclusion is measured rather than asserted: the run went from 40 excluded URLs to **41**, so
exactly one link left the check, and every other page's edit link — `cli.mdx` included — is still
required to answer 200.

> **EX-006 is the first post-merge cleanup owed by this phase.** Its removal condition is
> `docs/docs/persistence.mdx` resolving 200 on `main`, verified against a negative control. It is
> recorded here so it is not carried by a comment in a config file alone — which is the specific
> failure EX-005's own note warns about.

## 10. Limitations

See `specs/007-seaorm-parity/evidence/fr-conformance.md` §Limitations. L-11 (no `TransactionTrait`,
savepoints, or isolation levels), L-12 (SQL-file migrations only), L-13 (generated SeaORM sources
uncompiled) are new; L-7 and L-10 are inherited from Phase 006.

**L-13 is narrowed post-merge — see §13.** "Uncompiled" was true of the generator and false of the
sources. The limitation is that nothing **automated** compiles them: not the offline generator, and
not any generated-project gate in CI. They were compiled successfully against real `sea-orm 2.0.2`
during post-merge tutorial verification, by hand. The pre-merge wording above is left standing
because it is what this document said at merge; this paragraph is the correction, not a rewrite of
it.

## 11. A Phase 006 claim corrected

The Phase 006 closing summary reported the local blog cheat sheet as **1077 lines with all 28
documented commands executed**. Both halves were wrong. The file is **936 lines**, and its own
verification record states plainly that the four test engines were left stopped and that the
runtime database, migration, seed, and curl CRUD sequence was therefore **not executed** — only
generation, compilation, linting, `renvor routes`, and OpenAPI inspection were.

The file was honest. The summary of it was not, and it was the summary that was reported. Phase 007
executes the runtime path against a real engine rather than repeating the claim.

## 12. Not done, deliberately

- **Nothing published.** 0 crates, 0 tags, 0 releases, 0 deployments.
- **No generic resource generator.** `renvor generate resource` does not exist; Phase 011 owns it.
- **No cache, jobs, mail, or storage capability.** Phase 010.
- **Phase 008 not started.**

## 13. Post-merge correction — 2026-08-25

Recorded after Phase 007 merged as `ed6f26287c5198ca402fb73939560fc07d9bf888`. This section is what
changed afterwards.

**The pre-merge account above is left as it was written, with exactly one addition: the L-13
paragraph in §10**, which marks itself a post-merge correction in its first words and points here.
An earlier draft of this heading claimed nothing above it had changed at all, which a review
correctly called false — the §10 paragraph is above it. Naming the one exception is the honest form
of the claim; deleting the §10 paragraph to make the absolute version true would have left the
narrowed limitation stated only in a section a reader of §10 has no reason to reach.

### EX-006 was satisfied, and retired

Its removal condition was `docs/docs/persistence.mdx` resolving 200 on `main`, verified against a
negative control. Measured 2026-08-25:

| Probe | Result |
| --- | --- |
| `.../tree/main/docs/docs/persistence.mdx` | **200**, one redirect to `blob/main/...` |
| `.../tree/main/docs/docs/no-such-page.mdx` (control) | **404**, no redirect |

The control is what makes the 200 mean something: a probe that accepted everything would report 200
for both. The active exclusion is removed from `lychee.toml` and **no replacement exemption was
added** — that page's edit link is checked again like every other page's. EX-006 is recorded under
retired exclusions so the number is never reused, alongside EX-004 and EX-005.

The removal is measured the way §9 measured the addition, and is its mirror image. Adding EX-006
took the run from 40 excluded URLs to **41**; retiring it takes the run back to **40**, on both
toolchains:

```
388 Total  100 Unique  348 OK  0 Errors  40 Excluded  7 Redirects
```

Exactly one URL left the exclusion list and re-entered the check, and it answered — `0 Errors`. A
retirement that had quietly broken the gate would show an error here, and one that had exempted
something else in its place would not show 40.

This was the first post-merge cleanup this phase owed, and §9 named it so it would not be carried by
a comment in a config file alone. It was not.

### A documentation defect the phase's own gates could not see

Found while executing the tutorial against the merged tree, not by a reviewer. The **generated**
project's own documentation described the wrong persistence model on the SeaORM path:

- `renvor.toml`'s `[persistence]` comment said "`src/persistence.rs` and `migrations/` exist" on
  **both** ORM paths. On `--orm seaorm` that file does not exist; the tree holds `src/entity.rs`
  and `src/repository.rs`. The manifest's own opening rule is *"A choice appears here only if a
  generated file reflects it"* — broken in the one file whose entire purpose is to be believed.
- `README.md`'s persistence section documented `src/persistence.rs`, its bound statements, and the
  `renvor-sqlx` dependency to add — to a reader whose project contains none of them. Following it
  produces a project that does not resolve.
- `templates.rs` claimed the version-5 tree made `Cargo.toml` "declare a real `sea-orm` dependency
  so both compile", and that the two SeaORM entries "**compile**". Both describe the design that §6
  records as **designed, built, and withdrawn** — not the one that ships. The same doc comment cited
  `a_phase_006_project_still_generates_identically` as the test holding FR-043; no test by that name
  exists, and the real one is
  `the_direct_sqlx_tree_is_unchanged_apart_from_its_recorded_version`.

Why the gates missed it: nothing asserted on the generated **persistence** prose. Generated prose is
not unasserted in general — `the_manifest_resolves_only_the_selected_driver` reads the generated
`Cargo.toml`'s comment and `the_cache_says_it_is_not_wired_into_the_application` reads the generated
`README.md` — but every persistence test asserted on file sets, manifest fields, and source code
instead. `cargo fmt`, `clippy` and the generated project's own build cannot read a comment, and the
SeaORM sources they would have read are the two files nothing compiles.

### What the follow-up changed, and what it did not

Corrected in one focused pull request: both generated bodies are now selected by ORM, the
implementation documentation states the shipped behaviour, and the template version is **6**. Tests
pin each branch — that the SQLx README still names `src/persistence.rs` and `renvor-sqlx` and never
presents SeaORM steps; that the SeaORM README names `src/entity.rs` and `src/repository.rs`, never
names `renvor-sqlx` or `src/persistence.rs`, and states the compilation boundary; and that every
`src/…` path either rendered manifest names is a file that tree actually contains. Each was
negative-control mutated and observed to fail before being accepted.

**No runtime behaviour changed and no Phase 007 acceptance result changed.** No adapter, kernel, or
CLI code path was touched; the generated file **sets** are identical; the direct-SQLx `README.md`'s
persistence section is byte-identical to Phase 006's, which `the_sqlx_readme_is_unchanged_by_the_seaorm_split` now compares literally rather than by a memorable substring. The whole difference in a generated tree is
`renvor.toml`: its recorded version, and a comment that now names the files that are there.

L-13 is narrowed accordingly — see §10.

### The follow-up's own review — 5 findings, all confirmed and fixed

A focused review of the correction PR found five defects **in the correction itself**. All five were
verified against the code and fixed; none was waived.

| Severity | Finding | Disposition |
|---|---|---|
| High | The new SeaORM README section claimed "every sortable column is on an allowlist", an unknown-field refusal, and "`id` is in the allowlist as a tiebreaker" — all three carried over from the SQLx bullets. `page_after` takes **no sort field at all** and always orders by `id` ascending. | Rewritten to describe what the code does. `the_seaorm_readme_does_not_claim_a_sort_api_the_repository_lacks` now pins it, asserting the repository's lack of a sort surface as its premise and using the SQLx README as a control. |
| High | `the_sqlx_readme_is_unchanged_by_the_seaorm_split` asserted one paragraph was **present**, while `templates.rs` and this document described it as holding byte identity. Every other byte of the section could be rewritten and it still passed. | The test now compares the **whole persistence section** with `assert_eq!`. Red-checked by changing one hyphen in the section's last bullet — the exact byte the old form missed. |
| High | §13 opened by claiming everything above it was unchanged, while §10 had gained a post-merge L-13 paragraph. | The claim now names its one exception. The §10 paragraph was kept rather than deleted: a narrowed limitation belongs where a reader of §10 will find it. |
| Medium | The SeaORM README said the two files' "formatting is checked on every run", which a reader takes to mean their own `renvor new` or `cargo` run. The check is a test in **Renvor's** repository, and it returns without checking when `rustfmt` cannot start. | Reworded to say where the check lives and what it does not cover. |
| Medium | This section said no generated-project test asserted on prose. Two did — `the_manifest_resolves_only_the_selected_driver` reads the generated `Cargo.toml`'s comment, and `the_cache_says_it_is_not_wired_into_the_application` reads the generated `README.md`. | Narrowed to the persistence prose, naming both counterexamples. |

The first finding is the one worth recording plainly: a pull request written to delete false claims
about generated files **introduced two new ones**, by copying bullets from the branch it was
splitting. The lesson is the one the phase already learned once — prose is not checked by the
compiler, and a claim moved between contexts stops being true without changing a character.
