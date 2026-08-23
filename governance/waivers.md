# Waiver Ledger

**Status**: **9 active waivers** — W-001 (approval gap, seeded at T015), W-002 (ADR review gap), **W-003 (Phase 001 independent-review gap, granted 2026-08-15 at T088)**, **W-004 and W-005 (Phase 002 review gaps, granted 2026-08-16)**, **W-006 (ADR-0009 review gap, granted 2026-08-17)**, **W-008 (Phase 003 review gap, granted 2026-08-19)**, and **W-009 and W-010 (Phase 004 ADR-0012 and phase review gaps, granted 2026-08-23)**. W-006 is the **third** explicit reviewed exception in Phase 002 and therefore **exceeds this ledger's own expected maximum of two per phase**; that departure is recorded at [§The third Phase 002 exception](#the-third-phase-002-exception--an-acknowledged-departure) rather than absorbed silently. W-008 trips the ledger's **trend guard** — three consecutive phases waiving the same rule for the same reason — which is recorded at [§The trend guard is TRIPPED](#the-trend-guard-is-tripped-and-this-is-the-entry-that-says-so) together with the **RO-001** obligation granted with it.
**Satisfies**: spec FR-015, FR-051; constitution §Governance
**Schema**: [`data-model.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/data-model.md) §Waiver Record

> **This headline read “6 active waivers” and omitted W-008 until 2026-08-21.** The table below
> has carried seven rows since W-008 was granted on 2026-08-19; only this summary and the two
> category summaries below were left behind. `GOVERNANCE.md` said **Seven** throughout and was
> already correct. Corrected without changing the scope, controls, expiry, or removal plan of any
> waiver — the count was wrong, not the grants.

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
| **W-001** | spec FR-027 and constitution §Development and Phase Workflow — a pull request MUST carry an approving review from someone other than its author | The project has a single maintainer. No second person can approve, so the required-reviewer rule cannot be satisfied without blocking all work | (1) The **complete verification sequence** defined by `contracts/verification-sequence.md` — every step it defines, none conditional, none skipped — passes on **every** pull request, with `verify (1.94.0)`, `verify (stable)`, `security`, and `docs` all required to succeed before merge; (2) the **scanning gates** all report clean on every pull request: secret scanning with push protection, CodeQL, dependency review, `cargo-deny` (licences, advisories, bans, sources), and `gitleaks` over history and tree | Ahmed Anbar | **2027-02-11**, or immediately when a second maintainer with merge rights joins the project — whichever comes first | Add the second maintainer to the repository, enable the required-approving-review setting, re-review any change merged under this waiver, and close the waiver | `active` |
| **W-002** | spec FR-013 and constitution §Development and Phase Workflow #4 — a decision record MUST NOT be accepted without a recorded **independent** review | The project has a single maintainer. No second person qualifies as independent, so no genuinely independent review of a Phase 001 decision record is available | (1) Written alternatives-and-consequences review completed against the ADR template before acceptance; (2) verification against [`checklists/governance.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/checklists/governance.md); (3) all required CI and security checks passing; (4) a dated review record stored with the ADR | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent reviewer becomes available — whichever comes first | Raise the review requirement to a genuinely independent reviewer as soon as one is available, re-review every ADR accepted under this waiver, and close the waiver | `active` |
| **W-003** | `PLAN.md` §6.1 step 10 and constitution §Development and Phase Workflow #7 — a phase MUST obtain an **independent requirements and security review** before it closes, comparing implementation evidence against the specification, constitution, compatibility matrix, and security checklist. Tracked as **T088** | The project has a single maintainer. No second person qualifies as independent under the `GOVERNANCE.md` definition, so no genuinely independent human review of Phase 001 is available. **W-002 covers decision-record review only (FR-013) and does not reach this phase-level gate** | (1) **Two clean-context advisory reviews per pull request closing Phase 001** — one requirements, one security — each run against an explicit written requirement list, each producing findings that are recorded and **individually dispositioned** (fixed, or refused with a stated reason) in `governance/phase-001-evidence.md`, and each labelled **NON-INDEPENDENT and ADVISORY**; (2) **every claim of current external state verified read-only against the system that owns it**, with the verification method **and its limits** recorded — including claims that could not be verified and why; (3) **a written adversarial pass whose sole objective is to falsify the phase's own claims**, recorded with what it found; (4) **Phase 001 closes with its open gaps transferred and visible rather than closed** — the four deployment gates stay non-completed and enumerated. *(CI, gitleaks, CodeQL, `cargo-deny`, dependency review, and secret scanning are **already mandated unconditionally** and are therefore **not** cited here — a control another rule already requires compensates for nothing.)* | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — **whichever occurs first** | The first qualified independent reviewer re-reviews Phase 001 **in full** against the specification, constitution, compatibility matrix, and security checklist; T088 is then satisfied properly rather than waived; the phase record is updated with the outcome; W-003 closes | `active` |
| **W-004** | `PLAN.md` §6.1 and constitution §Development and Phase Workflow #4 — a decision record MUST NOT be accepted without a recorded **independent** review. Applied here to **ADR-0007** (Phase 002 custom kernel primitives), which spec FR-035 makes a blocking prerequisite for merging any custom infrastructure | The project has a single maintainer, who authored the record. Under the definition in [`research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md) §D11 a qualified independent reviewer must be **a person**, **not the author**, **competent in the subject**, and **able to reject without the author's consent**; no available person satisfies criteria 1, 2, and 4. **W-002 covers Phase 001 decision records only and does not reach a Phase 002 ADR** | **Counted — these exist only because of this waiver:** (1) the **executable configuration proof gate (8 obligations) and provider-resolver counter proof, completed and recorded *before* ADR acceptance**, so the record's scope is fixed by measurement rather than by the author's prediction; (2) **two clean-context advisory reviews of ADR-0007** — one architecture, one security, run in separate contexts against distinct written checklists — each labelled **NON-INDEPENDENT and ADVISORY**, and each **producing a recorded result: either enumerated findings or an explicit written "no findings" statement naming what was checked**. A review that returns nothing is recorded as **not performed**, never as passed — an empty result is the failure mode this clause exists to catch, and it has already been observed in practice; (3) **every finding individually dispositioned** (fixed, or refused with a stated reason) in the decision record, so a finding cannot be absorbed silently; (4) **no custom infrastructure merges until controls 1–3 are recorded.** *(Narrowed: that custom infrastructure needs an accepted ADR at all is already required by FR-035 and principle III and is **not** counted. What is novel — and counted — is that the merge additionally waits on **this waiver's own controls** being on the record, so the ADR cannot be accepted on the strength of the record existing while the compensating evidence does not.)* **Preconditions — restated for completeness, and deliberately NOT counted**, because the ledger holds that a control another rule already mandates unconditionally compensates for nothing: the **alternatives-and-consequences analysis** and the **package-first evaluation of every custom primitive** are already required by **spec FR-035** and **constitution principle III**; the **accepted-ADR prerequisite for custom infrastructure** is required by the same two; and the required **CI, dependency, licence, advisory, secret-scanning, and code-quality gates** already run unconditionally on every pull request. They must all hold; none of them is what this waiver buys. | Ahmed Anbar | **2027-02-16**, or **immediately** when a qualified independent reviewer becomes available — **whichever occurs first** | The first qualified independent reviewer re-reviews **ADR-0007 in full**, including the alternatives it rejects; the record's review evidence is updated with the outcome; W-004 closes | `active` |
| **W-005** | `PLAN.md` §6.1 step 10 and constitution §Development and Phase Workflow #7 — a phase MUST obtain an **independent requirements and security review** before it closes. Applied here to **Phase 002** | The project has a single maintainer. No available person qualifies as independent under the `GOVERNANCE.md` definition, so no genuinely independent human requirements-and-security review of Phase 002 is available. **W-003 covers Phase 001 only**, and a waiver is not extended by reinterpretation | **Counted — these exist only because of this waiver:** (1) **separate clean-context requirements and security advisory reviews**, each run against an **explicit written checklist** rather than open-ended judgement, each labelled **NON-INDEPENDENT and ADVISORY** wherever its results appear, and each **producing a recorded result — enumerated findings, or an explicit "no findings" statement naming what was checked**. A review that returns nothing is recorded as **not performed**, never as passed; (2) **every finding individually dispositioned** (fixed, or refused with a stated reason) in `governance/phase-002-evidence.md`, so nothing is absorbed silently; (3) a **complete FR-001…FR-044 and SC-001…SC-022 evidence mapping**, so a gap appears as an empty cell rather than as an absence nobody looked for. **Preconditions — restated for completeness, and deliberately NOT counted**: running **the cross-artifact analysis step and resolving its inconsistencies** is already mandated for every phase by `PLAN.md` §6.1 step 7; keeping **unresolved limitations visible** and not claiming more than was measured is already mandated by **constitution principle IX and principle XII** and by the workflow rule that a phase stays open while acceptance gaps exist; and the quickstart sequence, MSRV-and-stable runs, dependency/licence/advisory evidence, property-and-fuzz coverage, documentation, and scope isolation are already mandated by **SC-012, SC-013, SC-017, FR-040**, and **principle IX**. They must all hold; none of them is what this waiver buys. | Ahmed Anbar | **2027-02-16**, or **immediately** when a qualified independent reviewer becomes available — **whichever occurs first** | The first qualified independent reviewer re-reviews **Phase 002 in full** against its specification, the constitution, the compatibility matrix, the security checklist, the implementation, and the evidence pack; the result is recorded; W-005 closes | `active` |
| **W-006** | `PLAN.md` §6.1 and constitution §Development and Phase Workflow #4 — a decision record MUST NOT be accepted without a recorded **independent** review. Applied here to **ADR-0009** (removal of `image-size` from the documentation site by vendoring a no-op replacement), which spec FR-035 and constitution principle III make a blocking prerequisite for merging that custom infrastructure | The project has a single maintainer, who authored the record and the change it justifies. Under `GOVERNANCE.md` and [`research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md) §D11 a qualified independent reviewer must be **a person**, **not the author**, **competent in the subject**, and **able to reject without the author's consent**; no available person satisfies criteria 1, 2, and 4. **W-004 covers ADR-0007 alone and confers no authority here**, exactly as it confers none over ADR-0008; **W-005 is phase-level and does not authorise accepting any decision record.** Neither is extended by reinterpretation | **Counted — these exist only because of this waiver:** (1) **two clean-context advisory reviews of ADR-0009 specifically** — one requirements/package-governance, one security/supply-chain — run in separate contexts against distinct written scopes, each labelled **NON-INDEPENDENT** and **ADVISORY** wherever its results appear, and each **producing a recorded result: enumerated findings, or an explicit written "no findings" statement naming what was checked**. A review that returns nothing is recorded as **not performed**, never as passed — an empty result is the failure mode this clause exists to catch, and it has already been observed on this project; (2) **every finding individually dispositioned** (fixed, or refused with a stated reason) in `governance/phase-002-evidence.md`, so no finding is absorbed into a group row; (3) **every Critical, High, *and Medium* finding fixed before acceptance** — the **Medium** half is what this control buys, because `PLAN.md` §17.3 already blocks Critical and High unconditionally and stops there; (4) **the custom replacement does not merge until controls 1–3 *and* seven named record elements are on the record**: the two reviews, the dispositions, the executable dependency proof, the fail-closed image guard, the capability-loss statement, the ownership cost, and the removal condition. **Preconditions — restated for completeness, and deliberately NOT counted**, because this ledger holds that a control another rule already mandates unconditionally compensates for nothing: fixing **Critical and High** findings is already required by **PLAN.md §17.3**, which admits no waiver; the **alternatives-and-consequences analysis** and the **package-first evaluation** are already required by **spec FR-035** and **constitution principle III**; stating the **capability loss and the lockfile-readability hazard** is already required by **principle XII**; recording the change in the **dependency inventory** is already required by **FR-040**; and the **CI, dependency-review, licence, advisory, secret-scanning, and code-quality gates** already run unconditionally on every pull request. They must all hold; none of them is what this waiver buys | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent reviewer becomes available — **whichever occurs first**. *(This inherits the **earliest open expiry** for the same single-maintainer review gap rather than restarting the clock, applying the ratchet rule this ledger records below. W-006 is the first waiver to which that rule has been applied.)* | The first qualified independent reviewer re-reviews **ADR-0009 in full** — the record, the vendored replacement, the override, the lockfile, the guard, and the alternatives it rejects; the record's review evidence is updated with the outcome; W-006 closes | `active` |
| **W-008** | `PLAN.md` §6.1 step 10 and constitution §Development and Phase Workflow #7 — a phase MUST obtain an **independent requirements and security review** before it closes. Applied here to **Phase 003** (the interactive CLI) | The project has a single maintainer, who authored every line of Phase 003 and the evidence describing it. Under `GOVERNANCE.md` and [`research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md) §D11 a qualified independent reviewer must be **a person**, **not the author**, **competent in the subject**, and **able to reject without the author's consent**; no available person satisfies criteria 1, 2, and 4. **W-003 and W-005 are phase-level waivers for Phase 001 and Phase 002 and confer no authority here**, and neither is extended by reinterpretation | **Counted — these exist only because of this waiver:** (1) the **commissioned independent-review packet** (`governance/phase-003-independent-review-packet.md`), whose §-1.3 sign-off log is left **OPEN and visible** rather than closed, so the missing review is a standing, addressable item and not a silence; (2) a **five-area adversarial defect audit** of the shipped CLI — argv and terminal injection, panic payload disclosure, staging cleanup, Windows name and path semantics, and output reliability — in which every reported finding was independently re-run by a separate reviewer instructed to **refute** it, and the refutations recorded: 22 claims examined, **14 confirmed and fixed, 8 refuted with stated grounds**; (3) **every confirmed finding carries a regression test that was observed to FAIL before its fix and pass after**, and each guard was **mutation-tested** — the fix removed, the test observed to fail, the fix restored — which caught two of this phase's own tests passing for the wrong reason. **Preconditions — restated for completeness and deliberately NOT counted**, because this ledger holds that a control another rule already mandates unconditionally compensates for nothing: fixing **Critical and High** findings is already required by **PLAN.md §17.3**; the CI platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, and the format, clippy, and rustdoc gates already run unconditionally on every pull request; the dependency inventory is already required by **FR-040**; and `cargo xtask verify` is already required by the verification-sequence contract | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** for the same single-maintainer review gap under this ledger's ratchet rule rather than restarting the clock. W-006 was the first application; this is the second.)* | A qualified independent reviewer performs the requirements-and-security review of **Phase 003 in full**, against the exact merged content named in the packet's §-1.1 head binding, **including Windows coverage**; the outcome is recorded in the packet's §-1.3 sign-off log; the evidence pack is updated with the findings; W-008 closes | `active` |
| **W-009** | `PLAN.md` §6.1 and constitution §Development and Phase Workflow #4 — *"Consequential decisions MUST be captured as proposed ADRs and reviewed before being treated as accepted"*, and spec FR-013, which requires that review to be **independent**. Applied here to **ADR-0012** (Phase 004 custom HTTP primitives), which spec FR-035 and constitution principle III make a blocking prerequisite for merging custom infrastructure | The project has a single maintainer, who authored the record and the code it justifies. Under `GOVERNANCE.md` a qualified independent reviewer must be **a person**, **not the author**, **competent in the subject**, and **able to reject without the author's consent**; no available person satisfies criteria 1, 2 and 4. **W-002 covers Phase 001 decision records, W-004 names ADR-0007 alone, and W-006 names ADR-0009 alone; none reaches ADR-0012.** **W-005 and W-008 are phase-level and their own scope sections state they do not authorise accepting any decision record.** None is extended by reinterpretation | **Counted — these exist only because of this waiver:** (1) **the public surface this record decides is bounded by an executable guard, not by the record's own prose** — `crates/renvor/tests/facade_boundary.rs` parses the facade-root re-export list from the source and reads each name's declared signatures, and was **mutation-proven** by restoring the removed `Server` and observing the failure; (2) **every finding of the closing post-remediation requirements review individually dispositioned** in `governance/phase-004-evidence.md`, with each one **independently reproduced before being recorded** — three settled by mutation — so no finding is accepted on a reviewer's assertion; (3) **the five custom primitives are enumerated and the surface is asserted to be exactly those five**, so the record cannot quietly widen after acceptance; (4) **no custom infrastructure merges until controls 1–3 are on the record.** **Preconditions — restated and deliberately NOT counted**, because a control another rule already mandates unconditionally compensates for nothing: the alternatives-and-consequences analysis and the package-first evaluation are already required by **spec FR-035** and **principle III**; the accepted-ADR prerequisite for custom infrastructure is required by the same two; and the CI, dependency, licence, advisory, secret-scanning and code-quality gates already run unconditionally on every pull request | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under this ledger's ratchet rule rather than restarting the clock. W-006 was the first application, W-008 the second, this the third.)* | A qualified independent reviewer re-reviews **ADR-0012 in full** — the five primitives, the public-surface decision, and the alternatives it rejects; the record's review evidence is updated with the outcome; W-009 closes | `active` |
| **W-010** | `PLAN.md` §6.1 step 10 and constitution §Development and Phase Workflow #7 — *"An independent review MUST compare implementation evidence with the specification, constitution, compatibility matrix, and security checklist."* Applied here to **Phase 004** (the REST/HTTP runtime) | The project has a single maintainer, who authored every line of Phase 004 and the evidence describing it. No available person satisfies the `GOVERNANCE.md` criteria. **W-003, W-005 and W-008 are phase-level waivers for Phases 001, 002 and 003 and confer no authority here**, and none is extended by reinterpretation. **This is the fourth consecutive phase to waive this rule for this reason** | **Counted — these exist only because of this waiver:** (1) **a post-remediation requirements review covering FR-001…FR-049 and SC-001…SC-020 in full**, delivered against a named head, which **executed rather than only read** — re-running `cargo xtask verify`, running the `#[ignore]`d end-to-end relay test, and re-measuring crate isolation in both directions; (2) **every one of its findings independently reproduced by the maintainer before being recorded**, three of them by mutation, and **none recorded on the review's assertion alone** — one recorded finding was disproved on reproduction and corrected rather than filed; (3) **every entry it returned as less than SATISFIED closed with executable evidence** — thirteen of them — with **no requirement weakened and no PARTIAL relabelled without a new test**; (4) **every new guard mutation-tested**: the implementation it covers was broken, the test observed to fail, and the implementation restored. That practice found four defects nothing else did, including **a gate whose verdict depended on build state** and **a comment that switched a gate off for three of five publishable packages**. **Preconditions — restated and deliberately NOT counted**: fixing Critical and High findings is already required by **PLAN.md §17.3**; the CI platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, and the format, clippy and rustdoc gates already run unconditionally; the dependency inventory is already required by **FR-040**; and `cargo xtask verify` is already required by the verification-sequence contract | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under the ratchet rule rather than restarting the clock. Fourth application.)* | A qualified independent reviewer performs the requirements-and-security review of **Phase 004 in full**, against the exact merged content named below, including the Windows platform legs; the outcome is recorded in `governance/phase-004-evidence.md`; W-010 closes | `active` |
### W-003 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-15.** The following limits are part of the grant, not
commentary on it:

- **W-003 waives only the independent-human-review requirement for Phase 001.**
- **It does not waive any finding, failed check, missing evidence, acceptance criterion, or
  security blocker.** Anything that fails remains failed; anything unevidenced remains
  unevidenced. A waiver of *who reviews* is not a waiver of *what must be true*.
- **No independent human review of Phase 001 has occurred.**
- **Agent and self-review are advisory and explicitly non-independent**, and must never be
  described as independent — in this record, in the evidence pack, in `GOVERNANCE.md`, or in
  any public document. The advisory reviews performed were genuinely useful and found real
  defects; that does not make them independent.
- **Security release blockers are never waived.** The constitution's prohibition is unchanged
  and W-003 does not touch it.
- **Phase 001 must receive genuine independent re-review before any public release.**
- It expires on **2027-02-11** or when a qualified reviewer becomes available, **whichever
  occurs first**.

**T088 is recorded as `WAIVED / NOT MET`, never as completed.** It is not a completed task and
must not be counted as one.

### W-004 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-16.** The following limits are part of the grant, not
commentary on it:

- **W-004 waives the independent-review requirement for `ADR-0007` and for nothing else.** It does
  not reach ADR-0008, any future Phase 002 record, or any record in any other phase. A second
  Phase 002 decision record needs its own justification, not this one extended.
- **It does not waive any finding, failed check, missing evidence, acceptance criterion, or
  security blocker.** Anything that fails remains failed. A waiver of *who reviews* is not a
  waiver of *what must be true*.
- **No independent human review of ADR-0007 has occurred.**
- **Agent and self-review are advisory and explicitly non-independent**, and must never be
  described as independent — here, in the decision record, in the evidence pack, in
  `GOVERNANCE.md`, or in any public document. A waiver removes the *requirement*; it never
  changes the *fact*.
- **The reviewer field of ADR-0007 reads exactly `Ahmed Anbar — self-review under W-004`.**
- **ADR-0007 must not reach `accepted` until all four counted compensating controls are complete,
  every restated precondition holds, and the review record is dated.** Controls 1 and 4 are what
  make this more than paperwork: the ADR's scope is fixed by two *executed* proof gates rather
  than by prediction, and no custom infrastructure merges until the evidence exists.
- **Three of the seven controls originally proposed were reclassified as preconditions**, because
  the ledger holds that a control another rule already mandates compensates for nothing. The
  alternatives analysis and the package-first evaluation are already required by FR-035 and
  principle III; the CI, dependency, licence, advisory, secret, and code-quality gates already run
  unconditionally. Keeping them listed as compensating controls would have made this waiver look
  stronger than it is, which is the specific failure the rule exists to prevent.
- **Security release blockers are never waived.** The constitution's prohibition is unchanged.
- It expires on **2027-02-16** or when a qualified reviewer becomes available, **whichever
  occurs first**.

### W-005 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-16.** The following limits are part of the grant, not
commentary on it:

- **W-005 waives only the independent-human-review requirement for Phase 002.** It does not reach
  Phase 003 or any later phase.
- **W-005 does not authorise accepting any decision record**, in Phase 002 or anywhere else. It is
  a *phase-level* waiver. ADR-0007's authority is **W-004** and nothing else; a second Phase 002
  decision record would need its own waiver or a genuine independent review. This is the mirror of
  the limit W-003 places on W-002, and it is stated so that "Phase 002" cannot be read as
  swallowing the records inside it.
- **It does not waive any finding, failed check, missing evidence, acceptance criterion, or
  security blocker.** Anything unevidenced remains unevidenced.
- **No independent human requirements-and-security review of Phase 002 has occurred**, and
  Phase 002 must receive one before any public release.
- **Every review performed inside Phase 002 is advisory and non-independent** and must be
  labelled so wherever its results appear.
- **Phase 002 closes with its open gaps transferred and visible rather than closed**, on the same
  terms W-003 set for Phase 001 — including the four Phase 001 deployment gates, which remain
  non-completed and enumerated.
- **Security release blockers are never waived.**
- It expires on **2027-02-16** or when a qualified reviewer becomes available, **whichever
  occurs first**.

- **W-005 counts 3 compensating controls, not 7.** Four of the seven originally proposed were
  reclassified as preconditions on the same ledger rule that trimmed W-004: running
  the cross-artifact analysis step is already mandated by `PLAN.md` §6.1 step 7; keeping limitations visible is
  already mandated by principles IX and XII; and the quickstart, MSRV-and-stable, dependency, fuzz,
  documentation, and scope-isolation evidence is already mandated by SC-012, SC-013, SC-017, and
  FR-040. What remains counted is the part that exists **only** because there is no independent
  reviewer: two checklist-driven reviews that produce a recorded result, individual disposition of
  every finding, and a complete FR/SC evidence map.
- **Phase 002 must not be described as reviewed.** Three counted controls are a smaller claim than
  seven, and stating the smaller true number is the point.

**The Phase 002 phase-level review task is recorded as `WAIVED / NOT MET`, never as completed.**

### W-006 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-17.** The following limits are part of the grant, not
commentary on it:

- **W-006 waives only the independent-human-review requirement for `ADR-0009`, and nothing else.**
  It does not reach ADR-0007, ADR-0008, any future Phase 002 record, or any record in any other
  phase. A fourth Phase 002 decision record would need its own justification, not this one
  extended.
- **It does not waive** any Critical, High, Medium, or other security finding; any CI, dependency,
  licence, advisory, testing, documentation, or acceptance gate; Phase 002's phase-level review
  requirement; review of any ADR other than ADR-0009; or any release or publication restriction.
  Anything that fails remains failed. A waiver of *who reviews* is not a waiver of *what must be
  true*.
- **No independent human review of ADR-0009 has occurred.**
- **Agent and self-review are advisory and explicitly non-independent**, and must never be
  described as independent — here, in the decision record, in the evidence pack, in
  `GOVERNANCE.md`, in the documentation site, or in any public document. A waiver removes the
  *requirement*; it never changes the *fact*.
- **The reviewer field of ADR-0009 reads exactly `Ahmed Anbar — self-review under W-006`.**
- **ADR-0009 must not reach `accepted`** until all four counted compensating controls are complete,
  every restated precondition holds, and the review record is dated.
- **Security release blockers are never waived.** The constitution's prohibition is unchanged.
- It expires on **2027-02-11** or when a qualified reviewer becomes available, **whichever occurs
  first**.

- **W-006 counts 4 compensating controls, not 5.** The maintainer's grant listed five. The fifth —
  *"the first qualified independent reviewer must re-review ADR-0009 in full and close W-006"* — is
  recorded in this ledger's **removal-plan** column, where it belongs, rather than counted twice as
  a control that compensates for the gap while the gap is open. It is a promise about the future,
  not a control operating now. Of the remaining four, the parts that another rule already mandates
  were reclassified as preconditions on the same ledger rule that trimmed W-004 from 7 to 4 and
  W-005 from 7 to 3: fixing **Critical and High** findings is already required unconditionally by
  `PLAN.md` §17.3; the alternatives analysis and package-first evaluation by FR-035 and principle
  III; the capability-loss statement by principle XII; the inventory entry by FR-040. **What
  survives as counted is the part that exists only because there is no independent reviewer**: two
  clean-context reviews of this record specifically that must produce a recorded result, individual
  disposition of every finding, a **Medium** fix bar that §17.3 does not impose, and a merge gated
  on this waiver's own evidence being on the record.
- **ADR-0009 must not be described as reviewed.** Four counted controls are a smaller claim than
  five, and stating the smaller true number is the point.

### The third Phase 002 exception — an acknowledged departure

This ledger sets an expected maximum of **at most 2 explicit reviewed exceptions per phase**, and
requires that a third *"must be justified against this line explicitly, as an acknowledged
departure."* **W-006 is the third in Phase 002.** This section is that justification. It is
recorded here rather than absorbed by extending W-004 or W-005, both of which were deliberately
written so that they *cannot* stretch to cover it — W-004 names ADR-0007 alone, and W-005 states
in its own scope that a phase-level waiver does not authorise accepting a decision record.

**Why a third was needed.** ADR-0009 did not exist when W-004 and W-005 were granted on
2026-08-16. It was written on 2026-08-17 because a round-four advisory review (finding R4-8) showed
that the vendored `image-size` replacement is custom infrastructure, which FR-035 and principle III
require to carry an accepted decision record. The alternative to a third waiver was to leave two
**High** advisories open, which `PLAN.md` §17.3 forbids carrying into acceptance and which no
waiver may cover.

**What this departure does and does not mean:**

- It is **not** evidence that the design failed. It is evidence that the corrective work in Phase
  002 produced a new decision record late, and that the ledger's own rules refused to let an
  existing waiver be stretched over it.
- **The underlying problem is unchanged and is not a process defect: the project has no second
  qualified human reviewer.** One problem has now been recorded **five** times across two phases.
  The remedy is a second qualified person, not a seventh waiver.
- The **per-phase guard** is now breached once, visibly, with a reason. The count is 3 for
  Phase 002 against an expected 2.
- The **trend guard** — *"the third consecutive phase waiving the same rule for the same underlying
  reason is a release blocker unless a dated, tracked reviewer-recruitment obligation shows visible
  progress"* — is **not yet tripped**: Phase 001 and Phase 002 are two consecutive phases, not
  three. **Phase 003 is now one waiver away from tripping it.** That is stated here so it is not
  discovered later. *(Outcome, recorded 2026-08-19: Phase 003 did waive it, as **W-008**, and the
  trend guard tripped. This paragraph is left exactly as written on 2026-08-17 because it is a
  dated prediction that came true, and rewriting it would erase the only evidence that the ledger
  saw this coming.)*
- **W-006 is the first waiver to which the expiry-ratchet rule has been applied.** The rule, added
  after adversarial review, says a future waiver for this same single-maintainer gap *"should
  inherit the earliest open expiry — 2027-02-11 — rather than restart the clock."* W-006 expires
  **2027-02-11**, five days earlier than W-004 and W-005, and does not extend any deadline.

### W-008 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-19.** The following limits are part of the grant, not
commentary on it:

- **W-008 waives only the independent-human-review requirement for Phase 003.** It does not reach
  Phase 004 or any later phase, and a waiver for one phase is not standing permission for the next.
- **W-008 does not authorise accepting any decision record.** It is a *phase-level* waiver, on the
  same terms W-005 sets for Phase 002. A Phase 003 decision record would need its own waiver or a
  genuine independent review — which is exactly the reasoning that ended with **no** waiver being
  created for ADR-0011 (see the identifier note below).
- **It does not waive any finding, failed check, missing evidence, acceptance criterion, test,
  platform, or security blocker.** Anything unevidenced remains unevidenced.
- **No independent human requirements-and-security review of Phase 003 has occurred**, and
  Phase 003 must receive one before any public release.
- **Every review performed inside Phase 003 is advisory and non-independent** and must be labelled
  so wherever its results appear. This includes the maintainer's own review and every automated
  non-person review: **an automated reviewer is not a person and is therefore not independent
  under any reading of the criteria.**
- **The maintainer's acceptance is a human maintainer decision. It is not independent review**, and
  must never be described as one — not in this ledger, not in the evidence pack, not in the
  documentation site, and not in any public document.
- **Windows coverage was obtained from CI, which is automated platform evidence and not human
  review.** The packet's §-1.2 lists five Windows behaviours no person has examined.
- **Phase 003 closes with its open review gap transferred and visible rather than closed**, on the
  same terms W-003 set for Phase 001 and W-005 for Phase 002.
- **Security release blockers are never waived.**
- It expires on **2027-02-11** or when a qualified reviewer becomes available, **whichever occurs
  first**.

#### The identifier is W-008, and W-007 does not exist

The next free number would ordinarily be W-007. It is **not used**, and this is not an oversight.

`W-007` appears fifteen times in this repository, and every one of them asserts that it **does not
exist** — in `governance/constitution-amendment-3.0.0.md`, in `decisions/0010`, in
[`spec.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/spec.md), in [`tasks.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/tasks.md) T093a, and in the
Phase 003 evidence pack. Those statements record a **maintainer ruling** — *"Do not create
W-007"* — and form part of the rationale for the constitutional amendment from 2.0.0 to 3.0.0,
which chose to fix a wrong rule rather than grant a time-bounded exception to it.

Numbering a live waiver W-007 now would make all fifteen statements false, including the reasoning
inside a constitutional amendment. **This ledger does not falsify its own history to reclaim an
integer.** W-007 is permanently retired as a burned identifier; W-008 is the next waiver.

#### The trend guard is TRIPPED, and this is the entry that says so

The trend guard reads: *"the third consecutive phase waiving the same rule for the same underlying
reason is a release blocker unless a dated, tracked reviewer-recruitment obligation shows visible
progress."*

Phase 001 waived it as **W-003**. Phase 002 waived it as **W-005**. Phase 003 waives it as
**W-008**. That is **three consecutive phases, the same rule, the same reason.** The Phase 002
entry above predicted this in writing — *"Phase 003 is now one waiver away from tripping it"* — and
the prediction has come true rather than been avoided.

**The consequence, stated exactly.** This is a **release blocker**, not a merge blocker. It blocks
publishing a crate, cutting a tag, creating a release, and deploying — none of which Phase 003
does. It does **not** block merging Phase 003 into `main`, because merging publishes nothing.
Recording the distinction is not a softening: it is the difference between the two things the guard
actually says.

**The per-phase guard is NOT breached.** Phase 003 holds one waiver in this category against an
expected maximum of two. Phase 002's three remains the only breach.

**Six waivers across three phases is one problem recorded six times: the project has one person.**
The remedy is a second qualified person, not a seventh recording of the same fact.

#### RO-001 — the reviewer-recruitment obligation

Created 2026-08-19 as the condition on which W-008 is granted, because the trend guard makes a bare
waiver a release blocker.

| Field | Value |
|---|---|
| **Owner** | Ahmed Anbar |
| **Created** | 2026-08-19 |
| **First review date** | **2026-11-19** |
| **Obligation** | By the review date, record in this section either (a) the role, source, and availability of at least one candidate reviewer approached, or (b) a written statement of what was attempted and why it did not succeed |
| **Failure condition** | If neither entry exists by the review date, **every open waiver in this category is treated as expired**, and an expired-but-open waiver is a release blocker by this ledger's own rule |
| **Closes when** | A qualified independent reviewer is available, at which point W-003, W-005, and W-008 all close and this obligation is discharged |

**Progress log.** Entries are appended, never rewritten.

| Date | Entry |
|---|---|
| 2026-08-19 | Obligation created. **No candidate has yet been approached.** Recorded as the honest starting state rather than as progress |
| 2026-08-23 | **Still no candidate approached.** Two further waivers granted against this same gap — **W-009** (ADR-0012) and **W-010** (Phase 004) — taking the category to eight and making Phase 004 the **fourth consecutive** phase-level waiver. This entry exists to record that the gap widened while the obligation did not move. **It is not progress**, and the 2026-11-19 review date and failure condition are unchanged and now govern four phase-level waivers |

**This obligation is tracked and dated. It is not yet progress**, and nothing in this section should
be read as claiming otherwise. What it buys is that the gap now has an owner, a date, and a failure
condition, instead of being renewed silently a fourth time.

### W-009 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-23.** The following limits are part of the grant, not
commentary on it:

- **W-009 waives only the independent-human-review requirement for accepting ADR-0012.** It reaches
  no other record and no later phase.
- **W-009 does not authorise closing Phase 004.** It is a *record-level* waiver, on the same terms
  W-004 sets for ADR-0007 and W-006 for ADR-0009. Phase closure is **W-010**, granted separately for
  exactly the reason this ledger already records: *"a record-level waiver does not authorise closing
  a phase, and a phase-level waiver does not authorise accepting a decision record."*
- **It confers nothing on Phase 005** and is not standing permission for the next ADR.
- **It does not waive any finding, failed check, missing evidence, acceptance criterion, test,
  platform, or security blocker.** Anything unevidenced remains unevidenced.
- **ADR-0012's custom surface stays exactly the five named primitives** — trusted-proxy client
  identity, always-generate request ID, fail-closed host validation, CORS configuration validation,
  bounded drain. Widening it needs a new record, not a broader reading of this one.
- **No independent human review of ADR-0012 has occurred.** Every review performed inside Phase 004
  is **advisory and non-independent** and must be labelled so wherever its results appear —
  including the maintainer's own review and **every automated reviewer, which is not a person and is
  therefore not independent under any reading of the criteria.**
- **The maintainer's acceptance is a human maintainer decision. It is not independent review**, and
  must never be described as one — not here, not in the evidence pack, not in `GOVERNANCE.md`, and
  not in any public document.
- It expires **2027-02-11** or when a qualified reviewer becomes available, **whichever occurs
  first**.

#### The identifier is W-009, and it is not the W-009 that ADR-0011 names

[ADR-0011](../decisions/0011-support-linux-macos-and-windows.md) contains the string `W-009`. It
uses it as the label for a **hypothetical** waiver that would be required under a reading of W-002's
scope which that record **examines and rejects**, concluding *"W-009 would restate W-002 verbatim
against a different record number."* **No such waiver was ever created.** The number was therefore
free, and this record is not the one ADR-0011 describes: it covers a different record (ADR-0012),
in a different phase, under different controls, and it exists precisely because no existing waiver
reaches it. Recorded here so a reader meeting the string twice is not left to guess.

### W-010 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-23.** The following limits are part of the grant:

- **W-010 waives only the independent-human-review requirement for closing Phase 004.**
- **W-010 does not authorise accepting any decision record**, on the same terms W-005 sets for
  Phase 002 and W-008 for Phase 003. ADR-0012's acceptance is **W-009**.
- **It confers nothing on Phase 005.** A waiver for one phase is not standing permission for the
  next, and Phase 005 has not started.
- **It does not waive any product defect, missing test, missing functionality, publication rule, CI
  failure, or security blocker.** Where a requirement was unevidenced, the remedy applied was a
  test, not this waiver — thirteen such entries closed with executable evidence before this grant.
- **Phase 004 closes with its open review gap transferred and visible rather than closed**, on the
  same terms W-003, W-005 and W-008 set for the phases before it.
- **Security release blockers are never waived.**
- **The named limitations Phase 004 carries stay named**: the panic payload reaching process stderr,
  the header bounds Renvor does not set, no crate published so `renvor routes` reaches zero
  generated projects, registry names verified rather than reserved, and the macOS trust-store test's
  machine sensitivity. Each is permitted by its governing requirement. **None is waived by this
  record**, and this record must never be cited as covering them.
- It expires **2027-02-11** or when a qualified reviewer becomes available, **whichever occurs
  first**.

#### The head and tree this waiver is bound to

| Field | Value |
|---|---|
| **Branch** | `feat/phase-004-rest-http-runtime` |
| **Verified implementation and evidence head** | `a7085eef65db8eeb8ec62a727a98d0bdbf8492d6` |
| **Tree** | `f30b7898b0d87d27ca11c017a2acb3b108a1c39d` |
| **Decision record** | **ADR-0012** |
| **Gate result at that head** | `cargo xtask verify` 11/11 on **1.94.0** and on **stable**; 866 passed / 0 failed / 1 ignored / 55 suites on both toolchains and serial; worktree clean; `git ls-files specs` = 0 |

The re-review this waiver's removal plan requires is against **that content**, not against whatever
`main` holds when it is performed.

#### This is the FOURTH consecutive phase-level waiver, and the trend guard is already tripped

W-003 (Phase 001), W-005 (Phase 002), W-008 (Phase 003) and now **W-010 (Phase 004)** waive the same
rule for the same reason in four consecutive phases. The guard was already **TRIPPED** at three; a
fourth does not trip it again, it deepens it.

**What that means concretely, stated rather than softened:** the condition keeping this from being
an unaddressed release blocker is **RO-001**, the dated reviewer-recruitment obligation created with
W-008. RO-001's first review date is **2026-11-19** and has **not** yet arrived, so nothing about
this grant discharges it and nothing here should be read as progress against it. Its failure
condition is unchanged and now governs four waivers rather than three: if neither a candidate nor a
written account of what was attempted is recorded by that date, **every open waiver in this category
is treated as expired**, and an expired-but-open waiver is a release blocker by this ledger's own
rule.

**Six waivers across three phases was one problem recorded six times. Eight across four is the same
problem recorded eight times.** The remedy is a second qualified person, not a ninth recording.

## Waiver categories and expected counts

Three categories are tracked separately. They are **not** interchangeable, and a waiver in
one category does not consume the allowance of another.

| Category | Expected count | Waivers |
|---|---|---|
| Repository **approval** waivers | exactly **1** | **W-001** — single-maintainer approval gap *(seeded 2026-08-11 at T015)*. **Unchanged by W-003, W-004, W-005, and W-006** |
| **Control-unavailability** waivers | **0** | none expected — research Finding 3 confirmed every required repository control is free on the public tier, so cost or plan tier is never an accepted reason |
| **Explicit reviewed exceptions** | **at most 2 per phase** — **breached once, in Phase 002, which holds 3** *(see [§The third Phase 002 exception](#the-third-phase-002-exception--an-acknowledged-departure))*. Phase 001 holds 2, Phase 003 holds 1, **Phase 004 holds 2 — at the limit, not over it** | **W-002** — ADR independent-review gap (Phase 001); **W-003** — Phase 001 independent requirements-and-security-review gap *(granted 2026-08-15)*; **W-004** — ADR-0007 independent-review gap *(granted 2026-08-16)*; **W-005** — Phase 002 independent requirements-and-security-review gap *(granted 2026-08-16)*; **W-006** — ADR-0009 independent-review gap *(granted 2026-08-17)*; **W-008** — Phase 003 independent requirements-and-security-review gap *(granted 2026-08-19)*; **W-009** — ADR-0012 independent-review gap *(granted 2026-08-23)*; **W-010** — Phase 004 independent requirements-and-security-review gap *(granted 2026-08-23)* |

**W-002, W-003, W-004, W-005, W-006, W-008, W-009, and W-010 are explicit reviewed exceptions, not
part of the normal expected waiver count.** Each was granted by a recorded maintainer decision — W-002 on
2026-08-11, W-003 on 2026-08-15, W-004 and W-005 on 2026-08-16, W-006 on 2026-08-17, W-008 on
2026-08-19, and W-009 and W-010 on 2026-08-23 — rather than arising from a design shortfall, and none indicates that anything in the
design failed to work.
**All eight exist for the same underlying reason: the project has one person.** They are separate
waivers because they cover different rules, at different levels, in different phases:

| Waiver | Level | Phase |
|---|---|---|
| **W-002** | decision record (FR-013) | Phase 001 |
| **W-003** | phase-level review (`PLAN.md` §6.1 step 10) | Phase 001 |
| **W-004** | decision record — **ADR-0007 only** | Phase 002 |
| **W-005** | phase-level review | Phase 002 |
| **W-006** | decision record — **ADR-0009 only** | Phase 002 |
| **W-008** | phase-level review | Phase 003 |
| **W-009** | decision record — **ADR-0012 only** | Phase 004 |
| **W-010** | phase-level review | Phase 004 |

**The two axes are deliberately not collapsed.** A record-level waiver does not authorise closing
a phase, and a phase-level waiver does not authorise accepting a decision record — which is
exactly why Phase 002 needed three new waivers rather than an extension of the two that existed,
and why **W-005 could not be stretched over ADR-0009** even though ADR-0009 lives inside Phase 002.
A waiver is amended by re-justification and re-dating, never by reinterpretation.

**None consumes another's allowance, and none raises the approval-waiver count, which stays
exactly 1 (W-001), or the control-unavailability count, which stays 0.**

### What disposition does and does not buy (W-004 and W-005)

An adversarial review of these two waivers made a point sharp enough to belong in the grant
rather than in a review file:

> *"The disposition mechanism reads as a quality gate but is an audit log — the author can refuse
> 100% of findings from both reviews and remain fully, provably compliant."*

That is accurate. "Individually dispositioned" permits *refused with a stated reason*, the refusal
is decided by the same person who wrote the artifact and granted the waiver, and no one currently
has standing to overrule it. A reader who sees "two advisory reviews, every finding dispositioned"
and concludes *the problems were fixed* has read more than the text promises.

**This is therefore added to the grant, and it binds W-004, W-005, and W-006:**

- A finding of severity **HIGH or above** that is **refused** rather than fixed **MUST** be
  recorded as a **named open item** carried forward to the first qualified independent reviewer,
  in `governance/phase-002-evidence.md`, with the refusal reason.
- Refused high-severity findings therefore **accumulate visibly** instead of resolving. The count
  of them is itself a reportable figure.
- This closes the gap **without** requiring independence, which is the only reason it is
  achievable today.

### The growth of this category is itself a tracked risk

"Explicit reviewed exceptions" sit **outside** the expected counts, which means the table imposes
**no numeric ceiling on them**. Phase 001 needed two; Phase 002 needed **three** more of the same
shape. Nothing in this ledger mechanically stops Phase 003 from adding W-007 and W-008, and it
would be dishonest to present the classification as if it did.

**Updated 2026-08-17.** The prediction in this paragraph was written when four waivers existed and
guessed that the next two would be W-006 and W-007 *in Phase 003*. What actually happened is
worse than the guess: **W-006 arrived inside Phase 002**, before Phase 003 began, breaching the
per-phase guard the same section had just introduced. The guard worked — it forced the departure
to be written down instead of absorbed — but it did not prevent the departure, and nothing here
claims it could.

What the classification *does* require is that **each exception be justified on its own terms** —
its own violated rule, its own reason, its own compensating controls, its own expiry. None may be
granted by pointing at a predecessor. That is a real constraint, and until now it was the only one
here. Adversarial review called the uncapped category *"dishonest as a control — three governed
categories where the only one that grows is the ungoverned one."* Two guards are therefore added:

| Guard | Rule |
|---|---|
| **Expected count** | **at most 2 per phase** in this category. A third in one phase is not forbidden, but it **must** be justified against this line explicitly, as an acknowledged departure |
| **Trend, not instances** | the **third consecutive phase** waiving the *same* rule for the *same* underlying reason is a **release blocker** unless a dated, tracked reviewer-recruitment obligation shows visible progress |

The second guard is the one that matters. Counting waivers per phase measures instances; the real
signal is that **one** problem — the project has one person — has now been recorded **six** times
across **three** phases. A ledger that records instances without recording the trend lets a
permanent condition look like a series of temporary ones.

Two things follow, and both are stated rather than left implied:

- **The pattern is the signal.** Six review-gap waivers across three phases is not six independent
  problems; it is **one** problem — the project has one person — recorded six times. The remedy is
  a second qualified person, not a seventh waiver. **The per-phase guard is breached** (Phase 002
  holds three against an expected two; Phase 003 holds one), and **the trend guard is now TRIPPED**
  — W-003, W-005, and W-008 waive the same rule for the same reason in three consecutive phases.
  The condition that keeps it from being an unaddressed release blocker is **RO-001**, the dated
  reviewer-recruitment obligation created with W-008. See the W-008 section above for what that
  obligation does and does not claim.
- **An expired-but-open waiver is a release blocker**, and today nothing *automatically* detects
  one reaching its date. Until a mechanical check exists, that detection depends on a human reading
  this file. Recorded as a known gap, with the expiry dates written in bold in three places so the
  reading is at least easy.
- **The expiry horizon ratchets, and that is worth naming.** W-002 and W-003 expire **2027-02-11**;
  W-004 and W-005 expire **2027-02-16** — five days later, for the **same underlying gap**. Each
  new waiver for one unchanged problem therefore buys a slightly later date than the one before.
  The maintainer set 2027-02-16 deliberately, and it stands; what is recorded here is that
  **re-granting must not become a way to extend a deadline**. Any *future* waiver for this same
  single-maintainer gap should inherit the **earliest** open expiry — **2027-02-11** — rather than
  restart the clock. Raised by adversarial review; kept visible rather than resolved by moving a
  date the maintainer chose. **Applied for the first time on 2026-08-17: W-006 expires 2027-02-11**,
  not 2027-02-16, so the horizon did not move for the fifth recording of the same gap. **Applied
  again on 2026-08-19 (W-008) and on 2026-08-23 (W-009 and W-010).** Eight waivers now share one
  expiry date, which is the rule working: the horizon has not moved once since 2027-02-11 was set,
  and re-granting has bought no additional time.

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

## Decision-record review under W-004 (ruling of 2026-08-16)

While W-004 is active, and **for ADR-0007 alone**:

- The reviewer field of ADR-0007 reads exactly **`Ahmed Anbar — self-review under W-004`**.
- This review **MUST NOT** be described as independent, in the record, in the evidence pack, in
  `GOVERNANCE.md`, or in any public document. It is a structured self-review operating under a
  recorded exception.
- ADR-0007 may not reach `accepted` until **all four counted** compensating controls listed for
  W-004 are complete, **every restated precondition holds**, and the review record is dated.
  *(This read "all six" in the first draft of this record — a leftover from before three drafted
  controls were reclassified as preconditions. Two advisory reviewers independently flagged it as
  a blocker, because a number matching nothing in the row leaves the acceptance gate
  unenforceable.)*
- **What "qualified independent review" means** is recorded in
  [`research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md) §D11: a reviewer must be **a person**, **not the author of
  the artifact**, **competent in the subject**, and **able to reject without the author's
  consent**. W-004 exists because criteria 1, 2, and 4 cannot be met by anyone currently
  available — a staffing fact, not a process defect.

## Decision-record review under W-006 (ruling of 2026-08-17)

While W-006 is active, and **for ADR-0009 alone**:

- The reviewer field of ADR-0009 reads exactly **`Ahmed Anbar — self-review under W-006`**.
- This review **MUST NOT** be described as independent, in the record, in the evidence pack, in
  `GOVERNANCE.md`, in the documentation site, or in any public document. It is a structured
  self-review operating under a recorded exception.
- ADR-0009 may not reach `accepted` until **all four counted** compensating controls listed for
  W-006 are complete, **every restated precondition holds**, and the review record is dated. The
  counted four are: the two clean-context advisory reviews with recorded results; individual
  disposition of every finding; **every Critical, High, and Medium finding fixed**; and the merge
  gated on those three plus the seven named record elements (executable dependency proof,
  fail-closed image guard, capability-loss statement, ownership cost, removal condition, and the
  reviews and dispositions themselves).
- **The Medium bar is deliberately higher than `PLAN.md` §17.3**, which blocks Critical and High
  and stops there. Under W-006 a Medium finding on ADR-0009 also blocks acceptance. This is the
  clearest thing the waiver actually buys, and it is stated so that "every finding fixed" is not
  read as a restatement of a rule that already existed.
- **What "qualified independent review" means** is recorded in `GOVERNANCE.md` and in
  [`research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md) §D11. W-006 exists because criteria 1, 2, and 4 cannot be
  met by anyone currently available — a staffing fact, not a process defect.
- **W-006 is the third explicit reviewed exception in Phase 002**, exceeding this ledger's expected
  maximum of two per phase. That departure is recorded at
  [§The third Phase 002 exception](#the-third-phase-002-exception--an-acknowledged-departure) and
  **must not** be hidden by extending W-004 or W-005.

## Closed and expired waivers

| ID | Closed on | Outcome |
|---|---|---|
| *(none)* | | |

A waiver reaching its date without its condition being met is **not** automatically
renewed. It must be re-justified and re-dated, or the underlying rule complied with. An
expired-but-open waiver is a release blocker.
