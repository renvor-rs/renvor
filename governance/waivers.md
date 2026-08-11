# Waiver Ledger

**Status**: 2 active waivers — W-001 (approval gap, seeded at T015) and W-002 (ADR review gap).
**Satisfies**: spec FR-015, FR-051; constitution §Governance
**Schema**: `specs/001-governance-foundation/data-model.md` §Waiver Record

> **All seven fields are mandatory.** The constitution permits exceptions only through a
> time-bounded written waiver naming the violated rule, reason, compensating controls,
> owner, expiry, and removal plan.

> **`expiry` must include an absolute date.** A release condition may accompany it, and
> the waiver ends at whichever arrives first. A condition alone is not permitted — a
> condition that never occurs would never expire, which is not time-bounded.

> **Compensating controls must be specific to the gap.** A control that another
> requirement already mandates unconditionally compensates for nothing and may not be
> cited.

> **Security release blockers cannot be waived** for a public release.

## Active waivers

| ID | Violated rule | Reason | Compensating controls | Owner | Expiry (date / condition) | Removal plan | Status |
|---|---|---|---|---|---|---|---|
| **W-001** | spec FR-027 and constitution §Development and Phase Workflow — a pull request MUST carry an approving review from someone other than its author | The project has a single maintainer. No second person can approve, so the required-reviewer rule cannot be satisfied without blocking all work | (1) The **complete verification sequence** of `specs/001-governance-foundation/contracts/verification-sequence.md` — all 10 steps — passes on **every** pull request, with `verify (1.94.0)`, `verify (stable)`, `security`, and `docs` all required to succeed before merge; (2) the **scanning gates** all report clean on every pull request: secret scanning with push protection, CodeQL, dependency review, `cargo-deny` (licences, advisories, bans, sources), and `gitleaks` over history and tree | Ahmed Anbar | **2027-02-11**, or immediately when a second maintainer with merge rights joins the project — whichever comes first | Add the second maintainer to the repository, enable the required-approving-review setting, re-review any change merged under this waiver, and close the waiver | `active` |
| **W-002** | spec FR-013 and constitution §Development and Phase Workflow #4 — a decision record MUST NOT be accepted without a recorded **independent** review | The project has a single maintainer. No second person qualifies as independent, so no genuinely independent review of a Phase 001 decision record is available | (1) Written alternatives-and-consequences review completed against the ADR template before acceptance; (2) verification against `specs/001-governance-foundation/checklists/governance.md`; (3) all required CI and security checks passing; (4) a dated review record stored with the ADR | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent reviewer becomes available — whichever comes first | Raise the review requirement to a genuinely independent reviewer as soon as one is available, re-review every ADR accepted under this waiver, and close the waiver | `active` |

## Waiver categories and expected counts

Three categories are tracked separately. They are **not** interchangeable, and a waiver in
one category does not consume the allowance of another.

| Category | Expected count | Waivers |
|---|---|---|
| Repository **approval** waivers | exactly **1** | **W-001** — single-maintainer approval gap *(seeded 2026-08-11 at T015)* |
| **Control-unavailability** waivers | **0** | none expected — research Finding 3 confirmed every required repository control is free on the public tier, so cost or plan tier is never an accepted reason |
| **Explicit reviewed exceptions** | outside the counts above | **W-002** — ADR independent-review gap |

**W-002 is an explicit reviewed exception, not part of the normal expected waiver count.**
It was granted by a recorded maintainer decision on 2026-08-11 rather than arising from a
design shortfall, and it does not indicate that anything in the design failed to work.

## Decision-record review under W-002 (ruling of 2026-08-11, T006)

While W-002 is active:

- The reviewer field of every Phase 001 decision record reads exactly
  **`Ahmed Anbar — self-review under W-002`**.
- This review **MUST NOT** be described as independent, in the record, in the evidence
  pack, in `GOVERNANCE.md`, or in any public document. It is a structured self-review
  operating under a recorded exception.
- No decision record may reach `accepted` until all four compensating controls listed for
  W-002 have been completed and the review record is dated.
- `GOVERNANCE.md` (written at **T048**) MUST transcribe this reviewer-qualification ruling
  verbatim, including the prohibition on calling the review independent.

## Closed and expired waivers

| ID | Closed on | Outcome |
|---|---|---|
| *(none)* | | |

A waiver reaching its date without its condition being met is **not** automatically
renewed. It must be re-justified and re-dated, or the underlying rule complied with. An
expired-but-open waiver is a release blocker.
