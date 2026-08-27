# Phase 008 — Limitations, dispositions, and the open finding

**Phase**: 008 — Four-row database hardening
**Date**: 2026-08-26
**Companion to**: [`phase-008-evidence.md`](phase-008-evidence.md)

Every limitation Phase 008 inherited, created, or was asked to close, with an owner and a target for
each. **This file is tracked**, so a reviewer can fetch it from a clean index-only checkout; Phase
008's working notes live in `specs/`, which is deliberately untracked.

## Citation rule: limitations are phase-qualified

**`006/L-7`, `007/L-11`. A bare `L-n` is valid only inside the ledger that defines it.**

`L-11` has been reused across several phases — the identifier is per-phase and each ledger is
internally unambiguous, so nothing in any single ledger is wrong. The ambiguity exists only in an
**unqualified citation**, so the fix belongs to citation rather than to numbering.

Two of the collisions, as worked examples:

| Ledger | `L-11` names |
|---|---|
| Phase 006 | the generated cache service passes `--requirepass` as an argv element, so the local cache password is visible to `docker inspect` and `docker compose config` |
| Phase 007 | `sea_orm::TransactionTrait`, savepoints, and isolation-level configuration are not exposed |

**Closed phases are not renumbered and their evidence is not rewritten.** Phases 004 through 007 are
merged; editing a closed phase's ledger to renumber an entry rewrites a record a reviewer may
already have read, and it does not remove the ambiguity from anything written elsewhere — it moves
it. Phase-qualified references are canonical from Phase 008 onward.

## Inherited limitations retargeted by this phase

Both were targeted at Phase 008 and are **not closed by it**. Both need a public API change, and
both were presented for a decision rather than taken. The decision, recorded here, is the final
authority's.

### `006/L-7` — mixed-direction keyset pagination

| Field | Value |
|---|---|
| **Status** | open, **not** closed by Phase 008 |
| **Retargeted to** | **Phase 013** |
| **Owner** | Ahmed Anbar |
| **Obligation** | Phase 013 **must either** implement mixed-direction keyset pagination **or** explicitly exclude it from REST 1.0 |

`Keyset` refuses a sort plan whose terms do not all run the same direction. The nested-`OR` seek
that would lift the refusal needs `n` bound values on PostgreSQL and `n(n+1)/2` on MySQL, because
PostgreSQL's placeholders are numbered and can repeat by reference while MySQL's `?` is positional.

`seek_predicate` returns a bare `String`, so the binding count is information only it holds.
**Lifting the refusal therefore requires a public API change**: the function must return the binding
plan, not just the predicate. That is a phase-sized piece of work landing on a public surface, which
is why Phase 008 did not take it unilaterally.

### `007/L-11` — transactions, savepoints, isolation levels

| Field | Value |
|---|---|
| **Status** | open, **not** closed by Phase 008 |
| **Retargeted to** | **Phase 013** |
| **Owner** | Ahmed Anbar |
| **Obligation** | Phase 013 **must either** expose a tested transaction/savepoint/isolation API **or** explicitly exclude it from REST 1.0 |

`sea_orm::TransactionTrait`, savepoints, and isolation-level configuration are not exposed by either
adapter.

**What Phase 008 did instead, and why it is not the same thing.** ADR-0023 §3 and contract C-16 §3
measure the consequence of the two engines' differing defaults and turn the absence into a normative
rule: *"an application must not depend on the default… Renvor exposes no isolation-level setter, so
'set it to the one I want' is not available and this rule has no exception."* That is a real
improvement on silence. It is **not** closure: the API is still absent.

`renvor-database`'s own documentation already argues the case — *"a savepoint API whose failure
semantics are untested is worse than none"* — and the same argument applies to an isolation setter.
Shipping one in Phase 008 would have meant shipping a public API whose behaviour across four rows
this phase had not measured, inside the phase whose subject is measuring behaviour across four rows.

### `006/L-11` — the cache password in argv

Untouched by Phase 008, correctly: it belongs to the generated container profile, not to
persistence. Named here only because its identifier collides with Phase 007's.

Phase 008's backup/restore guidance reaches the **same class** of exposure from the other side — it
measures that a create-time `-e` password is recoverable in full from `docker inspect`, and tells
the reader to treat it as public. The two are consistent; neither closes the other.

## Limitations created by Phase 008

| ID | Limitation | Target |
|---|---|---|
| `008/L-1` | Two documentation pages carried `custom_edit_url: null`, suppressing their "Edit this page" link, because the link check cannot pass a page that is not yet on `main` | **CLOSED 2026-08-27.** Both suppressions removed in the closure pull request once the pages were on `main`; both edit links verified to resolve, with a nonexistent-page negative control that 404s. **No lychee exclusion was ever created** |
| `008/L-2` | The portability contract is executed on the **engine** axis; the ORM axis is covered for reachability, not for independent engine facts | none — deliberate, argued in `renvor_testkit::portability` |
| `008/L-3` | No supported engine **version range** is declared, so backup/restore guidance speaks only about the four pinned images | a phase that decides the support range (`F-2`) |
| `008/L-4` | **Transaction-conflict classification is measured on the two direct-SQLx rows only.** `renvor-seaorm` has no deadlock test, so `TransactionConflict` is asserted for one adapter and inferred for the other | a follow-up that writes the SeaORM deadlock test |

### `008/L-4` — the one place the cross-adapter parity claim is inferred rather than measured

**Created by this phase's third correction round, and open.**

| Field | Value |
|---|---|
| **Status** | **open** |
| **Owner** | Ahmed Anbar |
| **Target** | a follow-up that writes the test. Not closable by widening the claim |

The error-classification suites exist to make one assertion checkable: that an application swapping
`renvor-sqlx` for `renvor-seaorm` does not have to rewrite its error handling. For unique,
foreign-key, not-null and check violations that assertion is **measured on all four rows** — the
same `DatabaseErrorKind` constants, provoked against real servers through each adapter's own
vocabulary.

`TransactionConflict` is the exception. Provoking a deadlock takes two sessions holding two row
locks in opposite orders, which `renvor-sqlx/tests/error_classification.rs` arranges directly over
`sqlx::Pool`; the SeaORM suite has no equivalent, so the census carries **two** entries for it and
not four.

The inference is well-founded — a SeaORM deadlock reaches `DbErr::RuntimeErr(SqlxError)` and then
the same driver-level mapping the direct rows exercise, because `SqlErr` has no variant for it —
and it is still an inference. It is recorded here because the census's new entries make the
asymmetry visible, and a reader counting eleven tests on one row and ten on another is entitled to
know which test is missing and why rather than to discover it.

`008/L-1` is deliberately **not** a waiver. Precedent (EX-004, EX-006) would have allowed a lychee
exclusion; a permanent exclusion for a temporary condition is how a suppression outlives its reason,
so the link is fixed at its source instead and the suppression is removed the moment it stops being
needed.

---

## F-3 — `tls_consent` has an unsound observation window on macOS

**An unresolved defect. Not fixed in this branch, not waived, and not closed by a passing rerun.**

| Field | Value |
|---|---|
| **Status** | **open** |
| **Owner** | Ahmed Anbar |
| **Target** | a dedicated follow-up, **before Phase 009 begins** |
| **Deadline** | **2026-09-02** |
| **Test state** | **remains enabled.** Not ignored, not skipped, not quarantined |
| **Release coverage** | **preserved.** The CI macOS leg still runs it |

### What happened

```
test no_command_in_this_phase_modifies_the_trust_store ... FAILED
panicked at crates/renvor-cli/tests/tls_consent.rs:154:5
test result: FAILED. 8 passed; 1 failed
```

### It recurred on 2026-08-27, and the recurrence is recorded rather than absorbed

The HttpError correction's gate suite hit it again, on the same test and the same line:

```text
GATE FAIL [1.94.0 serial tests]
test no_command_in_this_phase_modifies_the_trust_store ... FAILED
panicked at crates/renvor-cli/tests/tls_consent.rs:154:5
test result: FAILED. 8 passed; 1 failed
```

**The stable-toolchain serial run, on the same machine, passed the same test** — 102 suites, 1459
passed, 0 failed, with the trust-store test present and passing. Two runs disagreeing about an
unchanged tree is the shape this entry already describes.

The trigger is very likely this session's own: two Docker containers were running throughout, for
the four-row database environment, and Docker Desktop is named below as one of the processes that
writes the login keychain. That **strengthens** the diagnosis rather than excusing the failure —
the test asserts a property of Renvor and is being decided by something else on the machine.

**Nothing about F-3 changes.** It is not fixed, not waived, not quarantined, and the deadline is
unmoved at **2026-09-02**. The failed run is part of the HttpError correction's evidence.

That correction's tree then changed — a control was added — and the full suite was re-run on the
final head, where **both** serial legs passed the trust-store test. That is now three passing runs
against one failure, and it closes nothing: an unchanged rerun passing is the definition of a
flake. The failure is retained, the deadline is unmoved, and no run is offered as a resolution.

An **unchanged rerun passed**. That is recorded as a fact about the second run and **not** as a
resolution: an unchanged rerun passing is the definition of a flake, not evidence against one, and
`PLAN.md` §17.2 treats a flaky test as a defect. The failed first run stays in this record.

### Exact reason, measured

`unchanged_by` (`crates/renvor-cli/tests/tls_consent.rs:150-156`) takes a snapshot before running a
command and asserts it is unchanged afterwards. On macOS `trust_store_paths()` (`:60-73`) includes
`~/Library/Keychains/login.keychain-db`, which is written by **anything on the machine that touches
the login keychain** — a credential helper, Docker Desktop, a browser, a background macOS service.

Measured immediately after the failure:

```
mtime = 2026-08-26 01:19:10   size = 366632
now   = 2026-08-26 01:19:13
```

The keychain had been rewritten **three seconds earlier**, inside the window in which the test was
running `renvor new` as a subprocess and comparing snapshots. The test attributed that write to
`renvor new`.

### Why CI has never caught it

On Linux `trust_store_paths()` selects `/etc/ssl/certs/ca-certificates.crt` and two anchor
directories, stable inside a container. The macOS leg runs on a fresh, idle runner where nothing
else opens the login keychain. The flake needs a **developer's** machine — where it is most annoying
and least visible to the gate.

### It is not caused by Phase 008

Phase 008 touches `renvor-database`, `renvor-sqlx`, and `renvor-seaorm`. `renvor-cli`'s trust-store
test has no dependency on any of them.

### The assertion is sound; the observation window is not

`no_command_in_this_phase_modifies_the_trust_store` is a good test — SC-010 wants breadth across
every command, and the file-level control `the_snapshot_sees_something_real` is well designed. The
defect is narrow: on macOS the snapshot includes a file the test cannot hold still.

A plausible fix is to drop `login.keychain-db` from the macOS path set and rely on
`/etc/ssl/cert.pem` plus a `security` query scoped to what Renvor could actually modify. Choosing it
belongs to whoever owns the TLS consent contract, which is why this is a dated obligation rather
than a change made inside a persistence phase.
