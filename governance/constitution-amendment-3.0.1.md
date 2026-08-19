# Constitution amendment 3.0.0 → 3.0.1 — vendor-neutral wording in Development and Phase Workflow #1

| | |
|---|---|
| **From** | **3.0.0** (2026-08-18) |
| **To** | **3.0.1** (2026-08-19) |
| **Class** | **PATCH** — *"clarifies wording without changing required behavior"* |
| **Authority** | Maintainer decision, Ahmed Anbar, 2026-08-19 |
| **Waiver** | **None.** No waiver was created, requested, or relied on |
| **Prompted by** | An automated review of PR #29 found the wording had been changed **without an amendment**. This document is the correction: the change is retained and made compliant, rather than reverted |

## 1. Written proposal

### The text that changes

**Development and Phase Workflow, clause 1.**

| | |
|---|---|
| **Was (3.0.0)** | Work MUST follow the numbered phases in `/PLAN.md`; **one Spec Kit feature directory** represents one active phase. |
| **Now (3.0.1)** | Work MUST follow the numbered phases in `/PLAN.md`; **one feature directory** represents one active phase. |

Two words are removed. Nothing else in the clause changes.

### Why

The clause named a specific third-party planning tool inside a normative rule. The rule's
substance is the **one-active-phase constraint** — that phase work is serialised, one feature
directory at a time — which is a property of how this project is governed, not of which tool
produces the directory. Naming a vendor in a MUST clause means a change of tooling would appear to
require a constitutional amendment even though the governing behaviour is untouched.

### Why this is PATCH and not MINOR or MAJOR

The constitution's own definitions:

- **MAJOR** removes or redefines a governing principle or compatibility promise. Nothing is removed
  or redefined; the MUST, its subject, and its constraint are identical.
- **MINOR** adds a principle or materially expands mandatory governance. Nothing is added and no
  obligation is widened.
- **PATCH** clarifies wording without changing required behavior. **This.** The set of states that
  satisfy the clause before and after is the same set.

**The one-active-phase rule is not weakened.** "One feature directory represents one active phase"
carries the same MUST, the same cardinality, and the same subject as before. A reading in which more
than one phase is active at a time is refused by both wordings identically.

### What is NOT an amendment, stated so it is not mistaken for one

The same commit also edited the **HTML comment** above the `# Renvor Constitution` heading, which
described where a local tooling working copy lives. That comment is **editorial metadata, not
governed text** — the comment itself sets the boundary, instructing that the copies be kept
identical *"from the `# Renvor Constitution` heading onward"*. Text before that heading is outside
the constitution body and outside this amendment. It is recorded here so a reader diffing the commit
sees both changes accounted for.

## 2. Impact analysis

| Area | Impact |
|---|---|
| **Public APIs** | **None.** The clause governs authoring workflow, not any API |
| **Generated projects** | **None.** No template, default, or generated file references this clause |
| **Security** | **None.** No security property depends on the tool's name |
| **Compatibility** | **None.** No compatibility promise, matrix row, or contract cites this clause |
| **Documentation** | The version string appears in `README.md`, `GOVERNANCE.md` (three places), and the `CONSTITUTION.md` footer and history block. All are updated in this change |
| **Active phases** | **None.** Phase 003 is merged and closed. No phase is open. Phase 004 has not started, and this clause governs it identically before and after |
| **Waivers** | **None affected.** No waiver cites this clause. W-008 cites `PLAN.md` §6.1 step 10 and *Development and Phase Workflow #7*, neither of which changes |

## 3. Migration plan

**No migration is required, and this is a finding rather than an assertion.**

The condition that would require one — existing behaviour or evidence changing — does not hold:
no artifact's compliance status changes, no evidence is invalidated, no document must be rewritten
to remain true, and no phase must be re-run. A reader of the 3.0.0 text and a reader of the 3.0.1
text are obliged to do the same thing.

The only work is the version synchronisation in §5, which is part of this change.

## 4. Maintainer approval

**Approved by Ahmed Anbar, project maintainer, 2026-08-19.**

This is a **maintainer decision and not independent review.** The maintainer authored the change
being approved. No independent human review of this amendment has occurred, and none is claimed.
An automated review pass is what surfaced the missing amendment; automated review is **not** a
person and cannot be independent.

Under `governance/waivers.md`, W-002 covers Phase 001 decision records and W-006 covers ADR-0009;
**neither reaches a constitutional amendment**, and neither is extended by reinterpretation here.
A constitutional amendment is not a decision record, so no ADR waiver applies. No new waiver is
created: the constitution's amendment procedure requires maintainer approval, which is present, and
does not require independent review of the amendment itself.

## 5. Version, date, and synchronised guidance

| Location | Updated |
|---|---|
| `CONSTITUTION.md` footer | **Version:** 3.0.1 · **Last Amended:** 2026-08-19 |
| `CONSTITUTION.md` AMENDMENT HISTORY | 3.0.1 entry added above the 3.0.0 entry |
| `GOVERNANCE.md` header | version 3.0.1, last amended 2026-08-19 |
| `GOVERNANCE.md` amendment table | 3.0.1 row added |
| `GOVERNANCE.md` document index | version and date updated |
| `README.md` document index | version and date updated |
| Local specification-tooling working copy | Synchronised and byte-verified from the `# Renvor Constitution` heading onward |

## 6. Verification

- Every tracked file citing a constitution version states **3.0.1 / 2026-08-19**, checked by scan.
- No tracked file still cites `3.0.0` as the *current* version; the 3.0.0 amendment record and the
  history entry retain it as **historical**, which is correct and must not be rewritten.
- `cargo xtask verify` passes on both required toolchains.
