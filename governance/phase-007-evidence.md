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

**No checksum**, and the string does not appear anywhere in the crate. `PLAN.md` §12 requires
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

*(Recorded below as each review returns. A review that returns nothing is recorded as NOT PERFORMED,
never as "no findings".)*

## 10. Limitations

See `specs/007-seaorm-parity/evidence/fr-conformance.md` §Limitations. L-11 (no `TransactionTrait`,
savepoints, or isolation levels), L-12 (SQL-file migrations only), L-13 (generated SeaORM sources
uncompiled) are new; L-7 and L-10 are inherited from Phase 006.

## 11. Not done, deliberately

- **Nothing published.** 0 crates, 0 tags, 0 releases, 0 deployments.
- **No generic resource generator.** `renvor generate resource` does not exist; Phase 011 owns it.
- **No cache, jobs, mail, or storage capability.** Phase 010.
- **Phase 008 not started.**
