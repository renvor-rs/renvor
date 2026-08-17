# Governance

How decisions get made in Renvor, who makes them, and how this document changes.

## Supreme authority

The [**Renvor Constitution**](CONSTITUTION.md) — **version 1.0.0, ratified 2026-08-11** —
is the highest authority in this project. Where this document and the constitution
conflict, the constitution wins, and the conflict is a defect in this document.

The constitution can only be amended through the process stated in its own Governance
section. It is not amended by practice, precedent, or convenience.

## Roles

| Role | Holder | Responsibility |
|---|---|---|
| Maintainer | **Ahmed Anbar** | Decision authority for the project: merges, releases, and the final word on scope |
| Security contact | **admin@ahmedanbar.dev** (Ahmed Anbar) | Receives and triages private vulnerability reports per [`SECURITY.md`](SECURITY.md) |
| Release approver | **Ahmed Anbar** | Named approver for the protected release environment |
| Registry bootstrap owner | **Ahmed Anbar** | Accountable for the first manual registry publication and the least-scope token lifecycle |

**Renvor currently has one maintainer.** Every role above is held by the same person.
This concentration is the root cause of **every** active waiver, and it is stated plainly
rather than distributed across four rows to look larger than it is.

## Decision records

Substantial architectural decisions are recorded in [`decisions/`](decisions/), using the
template at [`decisions/0000-template.md`](decisions/0000-template.md).

| State | Meaning |
|---|---|
| `proposed` | Written, not yet accepted. Carries no authority. |
| `accepted` | In force. Later work must conform to it or supersede it. |
| `rejected` | Considered and declined. Kept so the reasoning survives. |
| `superseded` | Replaced by a later record, which is named in the `Superseded by` field. |

Numbers are four digits, monotonic, and **never reused**. A rejected or superseded record
keeps its number forever, because references to it in commit messages, issues, and other
records must not silently retarget.

**A decision with no rejected alternatives was not a decision.** The template requires
alternatives with reasons, and a record without them is not ready for review.

## Who qualifies as an independent reviewer

A decision record **MUST NOT** be marked `accepted` without a recorded review
(spec FR-013).

An **independent reviewer** is a person who:

1. did not author the decision record, and
2. did not author the change the record justifies, and
3. is not directed by the record's author in a way that would make declining to approve
   professionally costly.

**Renvor currently has no independent reviewer**, because it has one maintainer. That gap
is not concealed and is not treated as satisfied by a technicality. It is recorded as
waiver **W-002** in [`governance/waivers.md`](governance/waivers.md), with an absolute
expiry date.

### Decision-record review under W-002

The following is the ruling recorded at T006, transcribed here in full as that ruling
requires. While W-002 is active:

- The reviewer field of every Phase 001 decision record reads exactly
  **`Ahmed Anbar — self-review under W-002`**.
- This review **MUST NOT** be described as independent, in the record, in the evidence
  pack, in `GOVERNANCE.md`, or in any public document. It is a structured self-review
  operating under a recorded exception.
- No decision record may reach `accepted` until all four compensating controls listed for
  W-002 have been completed and the review record is dated.
- `GOVERNANCE.md` MUST transcribe this reviewer-qualification ruling verbatim, including
  the prohibition on calling the review independent.

The four compensating controls that must all be met before acceptance:

1. A written alternatives-and-consequences review completed against the ADR template.
2. Verification against `specs/001-governance-foundation/checklists/governance.md`.
3. All required CI and security checks passing.
4. A dated review record stored with the ADR.

**As of 2026-08-11, control 3 was unmet** — the workflows that produce the required checks
did not exist yet, so every Phase 001 decision record remained `proposed`. Marking one
`accepted` then would have asserted a review that had not happened, which is precisely what
W-002 exists to prevent.

**Updated 2026-08-15: control 3 has been met since the workflows landed**, and `main` now
requires `verify (1.94.0)`, `verify (stable)`, `security`, and `docs`, strict, with
administrators included. **All six Phase 001 decision records — ADR-0001 through ADR-0006 —
are `accepted`**, each with reviewer `Ahmed Anbar — self-review under W-002` and a recorded
review date. **None of those reviews is independent**, and none may be described as such.

When a second qualified person joins, W-002 ends immediately, every record accepted under
it is re-reviewed, and the waiver is closed.

## Waivers

The constitution permits exceptions only through a **time-bounded written waiver**
recording all seven fields: the violated rule, the reason, compensating controls, the
owner, the expiry, the removal plan, and the status. Waivers live in
[`governance/waivers.md`](governance/waivers.md).

Rules that make a waiver a waiver rather than a loophole:

- **An absolute expiry date is mandatory.** A release condition may accompany it, and the
  waiver ends at whichever arrives first. A condition alone is not permitted — a condition
  that never occurs would never expire, which is not time-bounded.
- **Compensating controls must be specific to the gap.** A control that another
  requirement already mandates unconditionally compensates for nothing and may not be
  cited.
- **A waiver that reaches its date is not renewed automatically.** It must be re-justified
  and re-dated, or the underlying rule complied with. An expired-but-open waiver is a
  release blocker.
- **Security release blockers cannot be waived** for a public release.

**Six** waivers are currently active, all traceable to the same single-maintainer gap. Each
covers one rule, at one level, in one phase — and none is extended to another by
reinterpretation:

| ID | Gap | Level | Phase | Expiry |
|---|---|---|---|---|
| **W-001** | no second person can approve a pull request | repository approval | all | **2027-02-11** |
| **W-002** | no independent reviewer for a decision record | decision record | Phase 001 | **2027-02-11** |
| **W-003** | no independent requirements-and-security review | phase level | Phase 001 | **2027-02-11** |
| **W-004** | no independent reviewer for **ADR-0007** | decision record | Phase 002 | **2027-02-16** |
| **W-005** | no independent requirements-and-security review | phase level | Phase 002 | **2027-02-16** |
| **W-006** | no independent reviewer for **ADR-0009** | decision record | Phase 002 | **2027-02-11** |

Each expires on the date shown **or** immediately when a qualified second person becomes
available — whichever comes first. Full text, compensating controls, and scope limits are in
[`governance/waivers.md`](governance/waivers.md).

**W-006 is the third explicit reviewed exception in Phase 002**, which exceeds the waiver
ledger's own expected maximum of two per phase. That departure is recorded explicitly in the
ledger rather than hidden by extending W-004 or W-005, and the underlying problem is unchanged:
**this project has no second qualified human reviewer.** One problem has now been recorded five
times across two phases. W-006 also expires **2027-02-11** rather than a later date, because the
ledger's ratchet rule requires a new waiver for this same gap to inherit the earliest open
expiry rather than restart the clock.

### Decision-record review under W-006

While W-006 is active, and **for ADR-0009 alone**:

- The reviewer field of ADR-0009 reads exactly **`Ahmed Anbar — self-review under W-006`**.
- This review **MUST NOT** be described as independent, in the record, in the evidence pack, in
  this document, in the documentation site, or in any public document. It is a structured
  self-review operating under a recorded exception. **No independent human review of ADR-0009
  has occurred.**
- ADR-0009 may not reach `accepted` until all **four counted** compensating controls are
  complete, every restated precondition holds, and the review record is dated.
- **W-006 waives the independent-review requirement for ADR-0009 and nothing else.** It does not
  waive any security finding, any CI or acceptance gate, Phase 002's phase-level review
  requirement, review of any other ADR, or any release restriction. W-004 remains confined to
  ADR-0007; a record-level waiver never authorises closing a phase, and the phase-level W-005
  never authorises accepting a decision record.

## Changes to the default branch

`main` is protected. A change reaches it only through a pull request with all required
checks passing: `verify (1.94.0)`, `verify (stable)`, `security`, and `docs`.

**No account holds bypass permission, including administrators.** This is unconditional
and is not something W-001 waives — W-001 covers the *approving review* requirement only.

## Amending this document

1. Open a pull request describing what changes and why.
2. If the change touches decision authority, the reviewer definition, the waiver rules, or
   the branch-protection rules, it additionally requires a decision record in
   `decisions/`.
3. If the change would conflict with [`CONSTITUTION.md`](CONSTITUTION.md), it is
   **out of order** — amend the constitution first, through the constitution's own
   amendment process, or withdraw the change.
4. All required checks must pass, as for any other change.

## Related documents

| Document | Covers |
|---|---|
| [`CONSTITUTION.md`](CONSTITUTION.md) | Supreme authority — principles, v1.0.0, ratified 2026-08-11 |
| [`CONTRIBUTING.md`](CONTRIBUTING.md) | How to contribute, verification, dependency policy, licensing of contributions |
| [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) | Expected behaviour and enforcement |
| [`SECURITY.md`](SECURITY.md) | Private vulnerability reporting and response commitments |
| [`SUPPORT.md`](SUPPORT.md) | Supported Rust versions, platforms, and change rules |
| [`governance/waivers.md`](governance/waivers.md) | The waiver ledger |
| [`decisions/`](decisions/) | Architecture decision records |
