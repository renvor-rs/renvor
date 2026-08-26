# Phase 008 — Review record

**Phase**: 008 — Four-row database hardening
**Date**: 2026-08-26
**Companion to**: [`phase-008-evidence.md`](phase-008-evidence.md)

Every review Phase 008 obtained, what each one was, and what came of it. **This file is tracked**,
so a reviewer can fetch it from a clean index-only checkout.

## Summary

| Review | Kind | Outcome |
|---|---|---|
| Commissioned reviewer agent #1 | automated | **NOT PERFORMED** |
| Commissioned reviewer agent #2 | automated | **NOT PERFORMED** |
| Codex review of the reviewed head | automated, advisory, **not independent** | 10 findings, all dispositioned |
| Codex review, round 2, of `0c88b39` | automated, advisory, **not independent** | **REQUEST CHANGES** — 1 P1 and 5 P2, all dispositioned |
| Independent human requirements-and-security review | *required by PLAN §6.1 step 10* | **did not occur** — to be waived at closure, in the post-merge closure pull request |
| Independent human review of ADR-0023 | *required by constitution §Development and Phase Workflow #4* | **did not occur** — waived by **W-017** |

**No independent human review of Phase 008 has occurred, and none is claimed.**

## The two commissioned reviewer agents: NOT PERFORMED

Two reviewer agents were commissioned against the reviewed head. Each returned **twice**, and each
return was a bare idle notification carrying **no findings, no verdict, and no report**. The second
pair arrived immediately after a re-prompt asking for a formatted report, which is consistent with
the agents never having produced one.

**They are recorded as `NOT PERFORMED`, not as a pass.** This project's anti-loop policy states it
plainly: *a reviewer returning no result is `NOT PERFORMED`, never a pass.* A silent reviewer is
indistinguishable from a reviewer that found nothing and from one that never ran, and treating the
three as equivalent is how a review requirement becomes a formality.

No third round was commissioned. The review budget for this phase is fixed, and re-rolling a
reviewer until it answers is not a review — it is sampling until the answer is convenient.

Verified after the fact: neither agent changed the working tree, the branch head, or `origin/main`.

## The Codex review: automated, advisory, and not independent

An automated Codex review was run against the reviewed head. **It is supporting evidence only.**

It is **not** an independent human review and is **not** counted as one, for the same reason the
agents above are not: it was commissioned by the same person who wrote the code, it has no standing
to refuse the phase, and it cannot be held to an obligation. `governance/waivers.md` records the
same exclusion for every phase-level waiver from W-005 onward.

**It was, however, materially useful**, and saying so is not the same as counting it. Ten findings
were raised and all ten were dispositioned by change rather than by argument.

### Findings and dispositions

| # | Finding | Disposition |
|---|---|---|
| A | `StartupDiagnostic`'s public constructor accepted arbitrary `&'static str` and rendered it, so the claimed finite, structurally secret-free set was **false**: `Box::leak` promotes runtime text | **FIXED.** Closed enum `DatabaseAdapter` with `ALL` and `as_str`; the constructor accepts the enum; a `compile_fail` documentation test with a compiling twin is the negative control. Mutation **M-24** then found the residual gap and it was closed too |
| B | `StartupDiagnostic` discarded the `DatabaseError` and kept only its kind, so `source` answered `None` — a flattened chain, contrary to C-E2 | **FIXED.** The safe normalised error is stored and returned from `Error::source`; the raw driver error stays unreachable. C-E2 now distinguishes preserving a safe framework cause from terminating an unsafe driver chain |
| C | `ConnectFailed` covers unreachable servers, wrong credentials, unknown databases, and connection-limit refusals, but its advice named only reachability | **FIXED.** The advice names all five causes and prints none of their values. A real **server-side** refusal test was added per row — with a control proving the same server accepts the same user with the real password — so the census now requires 28 pairs rather than 24 |
| D | `DatabaseErrorKind::category()` mapped seventeen of twenty-two kinds through `_ => ErrorCategory::Internal`, which C-E1 reserves for a kernel defect | **FIXED by removal.** The projection is gone, `renvor-database`'s `renvor-core` dependency is gone with it, and both absences are asserted — a `compile_fail` test and an `xtask` step 7 resolved-graph check, each with a control |
| E | `the_four_rows_all_ran` printed `ok — NOT RUN` and returned `true`, so `cargo xtask verify` could exit **0** without executing the census | **FIXED.** Step 1 now refuses the run with exit **2** and setup instructions; nothing is auto-started; the census fails closed as defence in depth; three `xtask` tests including the negative one and a positive control |
| F | F-3 was recorded as an unresolved flake without an owner or a deadline | **FIXED.** Owner, target, deadline **2026-09-02**, exact reason, test-still-enabled and coverage-preserved statements — see [`phase-008-limitations.md`](phase-008-limitations.md). Not labelled fixed, not waived |
| G | Tracked governance evidence cited ignored `specs/` paths, so closure-critical material could not be fetched from a clean checkout | **FIXED.** The mutation ledger, limitation dispositions, F-3 record, and this review record are tracked files. `specs/` stays local and untracked; `git ls-files specs` returns 0 |
| H | `PLAN.md` §7.4 listed `orm-sqlx` and `orm-seaorm`, features that were never implemented and that accepted ADR-0020 settled the other way | **FIXED.** Removed, with ADR-0020 quoted and the change recorded as reconciliation with an accepted decision rather than a new one. `db-postgres` and `db-mysql` are preserved |
| I | C-16 made consequential normative choices that PLAN §10.1 required to be *measured* but did not itself decide | **FIXED.** **ADR-0023** records all seven decisions with alternatives and consequences, each bound to its four-row measurement; C-16 now cites PLAN §10.1 **and** ADR-0023 |
| J | `006/L-7` and `007/L-11` were left targeted at a phase that did not close them | **FIXED.** Both retargeted to **Phase 013**, owner Ahmed Anbar, each with an implement-or-explicitly-exclude obligation. Closed phases' records are unchanged |

## The second Codex round: REQUEST CHANGES, and what it found

A second automated round was run against the pushed head `0c88b39`, whose CI was 13/13 green. It
returned **REQUEST CHANGES — do not merge**, with one P1 and five P2 findings. **Every one was
verified against the tree before being accepted**, in keeping with this phase's rule that a
reviewer's assertion is not evidence; all six reproduced.

The same exclusion applies as to round 1: **automated, advisory, and not independent.** It is
counted as a compensating control under W-017 and never as the review W-017 waives.

That a clean 13/13 CI run and a first review round both passed a tree carrying a P1 is itself the
finding worth keeping: **green gates measure what they were written to measure.** Two of the six
were false statements in documents this cycle had just written, and one was a false claim in this
phase's own mutation evidence.

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | **P1** | Both adapters emitted `driver_error = %error` to `tracing`, and `contracts/error-taxonomy.md` — text written **in this cycle** — explicitly permitted it. `CONSTITUTION.md` principle VI forbids secrets in telemetry and exempts no consumer | **FIXED.** Every raw-error field is removed. Each adapter has **one** `record(kind)` helper that all classification paths funnel through and that takes a `DatabaseErrorKind` and nothing else, so there is no parameter a driver message could arrive in. It emits `adapter` (closed enum), `database_error_kind`, and `transient`. C-E2 is corrected at **1.4.0**, withdrawing the permission. New `telemetry_redaction.rs` in both adapters installs a capturing subscriber and proves it. Mutations **M-30/31/32** |
| 2 | **P2** | M-24 still survives: `ALL` is hand-written, so adding `Custom(&'static str)` while **omitting it from `ALL`** leaves every test green. The mutation-closure claim is false | **FIXED, and the false claim corrected rather than erased.** M-24b was re-run and **survived 59 of 59 tests**, confirming the reviewer. `closed_named_enum!` now declares the enum, `ALL` and `as_str` from one list for `DatabaseAdapter`, `StartupPhase` **and** `DatabaseKind`; M-24b now fails at macro expansion. Both mandated controls added — a `compile_fail` and its compiling twin differing in one token sequence |
| 3 | **P2** | `xtask` still documented the removed *"reported as not-run … a usable `cargo xtask verify`"* behaviour | **FIXED.** The comment states the refusal, names exit 2, and records that it previously described behaviour that had been removed |
| 4 | **P2** | W-017 counted controls that PLAN and ADR governance already mandate, contrary to `GOVERNANCE.md`'s own rule | **FIXED.** The four-row binding and the alternatives table are moved to **explicitly uncounted preconditions**, with the requirements that already mandate them named. Four genuinely specific controls remain, plus the advisory rounds themselves — labelled automated and non-independent |
| 5 | **P2** | The ledger said *"Both were granted"* where one waiver exists, named a **W-018** that had not been granted, and said *"All fourteen"* against fifteen | **FIXED.** All three corrected, with what each said recorded. `the_active_waiver_counts_match_the_waiver_table` now also binds the prose count and **refuses any reference to a waiver its own table does not grant** (W-007, documented as permanently burned, is the one exemption). Mutations **M-28/M-29** |
| 6 | **P2** | `contracts/verification-sequence.md` still defined exit 2 as toolchain-only | **FIXED at 1.2.1.** Exit 2 is a missing **mandatory verification prerequisite** — a tool *or* the four-row database environment. The fail-closed sample output was also stale and now matches what the command prints. A new test binds the contract, `CONTRIBUTING.md`, and `EXIT_TOOLING_MISSING`'s own doc comment |

### What the second round did not cover, and what the sweep found anyway

Finding 1 named three lines. **The mandated workspace sweep found five**, and the two the review did
not name are the migration loaders — `renvor-sqlx/src/migrate.rs` and `renvor-seaorm/src/migrate.rs`
both logged `migrate_error = %inner`, whose text names the migration directory the function's own
contract promises not to carry. Both are fixed on the same helper.

Two further sites were examined and are **out of scope, reported rather than left silent**:

| Site | Finding |
|---|---|
| `crates/renvor-http/src/route/build.rs:831` — `reason = %error` | **Not equivalent.** The value is an `http::Error`, whose `Display` for every variant is a fixed string compiled into the `http` crate — `"failed to parse header value"` and siblings. Verified in `http-1.4.0`'s source. No caller, server, or configuration text can reach it |
| `crates/renvor-http/src/route/build.rs:894` and `:901` — `detail = error.detail()` | **Equivalent in shape, different in provenance, and NOT fixed here.** `HttpError::new(kind, detail: impl Into<String>)` accepts an unbounded runtime `String` from the **application author**, and it is emitted to telemetry by design. An author writing `HttpError::new(kind, format!("could not reach {dsn}"))` puts a DSN into telemetry. This is a **Phase 004** design, stated in `contracts/problem-details.md` (*"the operator-facing detail goes to the telemetry record, which is a different consumer with different rights"*) and in `renvor-error/src/code.rs`. Changing it is a contract change outside the six findings and outside a database-scoped authority, so it is **raised for decision rather than altered** |

A third observation, unrelated to telemetry: **`renvor-testkit` declares `tracing-subscriber` 0.3.23
as a production dependency with the comment *"Capturing subscriber for assertions about emitted
records"*, and nothing in the crate uses it.** `governance/phase-002-dependency-inventory.md`
classifies it as a direct production dependency with transitive consequences, so removing it is a
dependency-inventory change rather than a tidy-up. Not touched; recorded here. The capturing
subscribers this round added are hand-written, following the precedent
`crates/renvor-http/tests/telemetry.rs` set for exactly this reason.

### One correction to the review's own wording

Finding J's premise described `L-11` as having been reused in *two* phases. It has been reused in
**several**. The disposition uses the accurate wording; the phase-qualified citation rule is
unchanged by the correction, and no closed evidence was rewritten to accommodate it.

## The citation audit, and a pre-existing gap it found

Finding G required that **every tracked citation resolve in a clean index-only checkout**. That was
verified by extracting `git archive HEAD` into an empty directory — no working tree, no ignored
files, no `specs/` — and resolving every relative link in `governance/`, `contracts/`, `decisions/`,
`docs/docs/`, and the top-level documents.

| Measure | Result |
|---|---|
| Relative links checked across all tracked documentation | **266** |
| Genuinely broken | **0** |
| Relative links inside Phase 008's own records | **15**, **0 broken** |
| `specs/` present in the index-only extraction | **no** — `git ls-files specs` returns 0 |

Six were reported by the audit and all six were confirmed **false positives**:

- Three are in `governance/deferred-verification-work.md` lines 50–52, which is a table
  *documenting exactly these naive-link-parser failure modes* — an escaped bracket, a destination
  containing balanced parentheses, and an angle-bracket destination. The audit reproduced all three,
  which is a control on the audit rather than a defect in the document.
- Two are Docusaurus **site-absolute routes** in `docs/docs/persistence.mdx` (`/docs/...`), which
  are not filesystem paths and are validated by verification step 9's build and step 10's link
  check.
- One is the `specs/` citation described below.

### Pre-existing, out of scope, and stated rather than absorbed

Seven backticked `specs/`-shaped paths appear in **tracked** text and **do not resolve** in an
index-only checkout:

| Record | Reference |
|---|---|
| `governance/phase-005-requirements-conformance.md` | `specs/005-validation-problem-openapi/spec.md` |
| `governance/phase-006-evidence.md` | `specs/006-persistence-sqlx/evidence/fr-conformance.md` |
| `governance/phase-007-evidence.md` | `specs/007-seaorm-parity/evidence/fr-conformance.md` |
| `governance/waivers.md` (W-014's controls) | `specs/006-persistence-sqlx/evidence/fr-conformance.md` |
| `decisions/0001`, `decisions/0003` | `specs/...` (elided, illustrative) |
| `decisions/0011` | `specs/001-governance-foundation/checklists/governance.md` |

**None was introduced by Phase 008, and none is corrected here.** They belong to closed phases'
records, and the same authority that required this audit also required that closed evidence not be
rewritten. W-016's own text already states the problem for Phase 007 — *"that record lives in
`specs/`, which is deliberately untracked, so a reviewer cannot fetch it from the repository"* — so
the condition is disclosed rather than newly discovered.

It is named here so it is not mistaken for a clean result. **A reviewer of Phase 005, 006, or 007
cannot fetch part of the evidence those phases cite.** Phase 008 fixed this for itself and for
nothing else.

## What the review process did not establish

An automated review reads a diff. It does not:

- run the four rows against real engines;
- decide whether a waiver should have been granted;
- carry any obligation if it is wrong.

Those are the properties that make a review *independent*, and none of them is present here. That is
the whole content of **W-017**, and of the phase-level waiver granted at closure, and it is why
both are stated as gaps rather than as satisfied requirements.

**RO-001**, the dated reviewer-recruitment obligation created with W-008, has produced **no
recruitment progress of any kind**. Its first review date remains **2026-11-19**, unchanged. The
phase-level waiver granted at closure will be the **eighth consecutive** waiver of the same rule
for the same reason.
