# ADR-0020: Add SeaORM as a sibling adapter behind the same ports, not as a layer on the first one

| Field | Value |
|---|---|
| **ID** | 0020 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-015 |
| **Review date** | 2026-08-24 |
| **Superseded by** | *(not superseded)* |

> **This record is `accepted` under W-015. The review behind it was NOT independent.**
>
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded **independent**
> review before acceptance. **No independent human review of this record has occurred, and none is
> claimed.** Acceptance rests on **[W-015](../governance/waivers.md)**, a time-bounded written
> waiver granted on 2026-08-24, owned by Ahmed Anbar, expiring **2027-02-11** or **immediately**
> when a qualified independent human reviewer becomes available — whichever is first.
>
> **Automated review is advisory and does not satisfy the requirement.** W-015 covers ADR-0020,
> ADR-0021 and ADR-0022 as one coupled Phase 007 decision; it authorises **nothing else** — not
> phase closure, which is W-016, and no publication, tag, release or deployment.

## Context

`PLAN.md` §Phase 007 requires *"SeaORM adapter and templates for PostgreSQL and MySQL"*, accepted
only when *"both SeaORM rows pass the same application contracts as direct SQLx"* and *"choosing
SeaORM does not expose direct-SQLx application APIs by accident"*.

ADR-0016 already decided the shape persistence takes: `renvor-database` declares ports that name no
driver, and an adapter implements them. The open question was where the second adapter sits relative
to the first.

## Decision

**A sibling crate.** `renvor-seaorm` implements the same `renvor-database` ports that
`renvor-sqlx` implements. Neither depends on the other, and `xtask` step 7 asserts both
directions with a positive control.

**The shared contract lives in `renvor-testkit`.** `renvor_testkit::persistence` holds the
assertions; each adapter supplies four driver-specific operations by implementing
`PersistenceFixture`. Both test binaries call the same compiled functions.

**Neither adapter is in the facade.** Phase 006 put persistence outside `renvor` deliberately, and
Phase 007 does not change that. It makes the accident `PLAN.md` names structurally impossible: a
facade that reaches neither adapter has nowhere to leak one from.

## Alternatives, and why each was rejected

**`renvor-seaorm` depends on `renvor-sqlx`.** Tempting, because the migration runner is then
written once. Rejected: it puts a direct-SQLx crate into every SeaORM application's graph, and the
migration type would have to cross `renvor-seaorm`'s public API — which is precisely the accidental
exposure the acceptance criterion names. The duplication it would have avoided is one macro-generated
runner around a **public** SQLx API, which two independent adapters may reasonably each wrap.

**One crate with a feature switch.** Rejected: `--all-features` would resolve both ORMs, and the
feature-isolation guarantee that a project resolves exactly what it selected would be untestable.

**Two contract suites that assert the same things.** Rejected, and this is the one worth naming.
"The same contracts" is the acceptance criterion; two files that agree today diverge the first time
one is edited, and a *weakened* assertion still passes. Compiling one set of functions into both
binaries makes the claim checkable.

**Put the shared contract in `renvor-database`.** Rejected: the ports crate would gain test-harness
machinery that every application resolves. `renvor-testkit` is a dev-dependency by design.

## Consequences

- `renvor-testkit` gains a dependency on `renvor-database` and **moves from release position 2 to
  position 4**. A crate cannot publish before something it depends on.
- The publishable set grows to **eleven**. The `xtask` count assertion reported it before any list
  was edited — the outcome it was pinned for after Phase 005 and Phase 006 both found it late.
- `PersistenceFixture::insert` takes `&mut`, which SeaORM does not need and direct SQLx does. The
  stricter of the two signatures is what lets one function serve both rows.
- An adapter whose unit of work is not an `Executor` can still use most of the harness: that bound
  sits on the two functions that need it rather than on the trait, so a port trait was not widened
  for a test harness's convenience.
