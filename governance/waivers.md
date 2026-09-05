# Waiver Ledger

**Status**: **21 active waivers** — W-001 (approval gap, seeded at T015), W-002 (ADR review gap), **W-003 (Phase 001 independent-review gap, granted 2026-08-15 at T088)**, **W-004 and W-005 (Phase 002 review gaps, granted 2026-08-16)**, **W-006 (ADR-0009 review gap, granted 2026-08-17)**, **W-008 (Phase 003 review gap, granted 2026-08-19)**, **W-009 and W-010 (Phase 004 ADR-0012 and phase review gaps, granted 2026-08-23)**, **W-011 and W-012 (Phase 005 ADR-cluster and phase review gaps, granted 2026-08-23)**, **W-013 and W-014 (Phase 006 ADR-cluster and phase review gaps, granted 2026-08-24)**, **W-015 and W-016 (Phase 007 ADR-cluster and phase review gaps, granted 2026-08-24)**, **W-017 (ADR-0023 review gap, granted 2026-08-26)**, **W-018 (Phase 008 independent requirements-and-security-review gap, granted 2026-08-27)**, **W-019 and W-020 (Phase 009 ADR-cluster and phase review gaps, granted 2026-08-31)**, **W-021 and W-022 (Phase 010 ADR-cluster and phase review gaps, granted 2026-09-04)**, and **W-023 and W-024 (constitution principle VII's generator obligation, left unmet by Phase 009's auth starter and by Phase 010's capabilities; granted 2026-09-04; NOT review-gap waivers) — CLOSED 2026-09-05, the first closures in this ledger, when Phase 011 proved the generator support against head `5eff451c435c8676aaa3cd231ccfc7d2e5ec5ba0`, tree `d1cab4cb7b1a1a18e387689e6ad3fdd0f6a628f9`; their rows stay, marked `closed`, and are not counted above**. **W-017 is a record-level waiver and closes nothing**; **W-018 is the phase-level waiver that closes Phase 008**, and the two are separate exceptions on separate axes. **W-019 is a record-level waiver covering ADR-0024…ADR-0030 as one cluster and closes nothing**; **W-020 is the phase-level waiver that closes Phase 009**. **W-021 is a record-level waiver covering ADR-0031…ADR-0037 as one cluster and closes nothing**; **W-022 is the phase-level waiver that closes Phase 010**. **W-022 is the TENTH consecutive phase-level waiver of the same rule for the same reason**; the trend guard was tripped at three and has deepened every phase since, recorded at [§This is the FIFTH consecutive phase-level waiver](#this-is-the-fifth-consecutive-phase-level-waiver-and-the-trend-guard-is-already-tripped). W-006 is the **third** explicit reviewed exception in Phase 002 and therefore **exceeds this ledger's own expected maximum of two per phase**; that departure is recorded at [§The third Phase 002 exception](#the-third-phase-002-exception--an-acknowledged-departure) rather than absorbed silently. **W-023 is the third explicit reviewed exception in Phase 009 and W-024 the third in Phase 010** — the per-phase maximum is exceeded twice more, on the same day, and neither is hidden by extending another waiver; both departures are recorded at [§The third Phase 009 and Phase 010 exceptions](#the-third-phase-009-exception-and-the-third-phase-010-exception--acknowledged-departures). W-008 trips the ledger's **trend guard** — three consecutive phases waiving the same rule for the same reason — which is recorded at [§The trend guard is TRIPPED](#the-trend-guard-is-tripped-and-this-is-the-entry-that-says-so) together with the **RO-001** obligation granted with it.
**Satisfies**: spec FR-015, FR-051; constitution §Governance
**Schema**: [`data-model.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/data-model.md) §Waiver Record

> **This headline read “6 active waivers” and omitted W-008 until 2026-08-21.** The table below
> has carried seven rows since W-008 was granted on 2026-08-19; only this summary and the two
> category summaries below were left behind. `GOVERNANCE.md` said **Seven** throughout and was
> already correct. Corrected without changing the scope, controls, expiry, or removal plan of any
> waiver — the count was wrong, not the grants.

> **AND IT HAPPENED AGAIN.** This headline read “11 active waivers” and omitted **W-013 and W-014**
> from 2026-08-24, when Phase 006 granted them, until they were found during Phase 007's
> preconditions audit. The table below has carried thirteen rows the whole time. This time
> `GOVERNANCE.md` was **also** stale — it said “Eleven” *and* its table was missing both rows, so
> the cross-check that caught the last occurrence did not catch this one.
>
> Corrected here without changing the scope, controls, expiry, or removal plan of any waiver: the
> counts were wrong, not the grants. `the_active_waiver_count_matches_the_table` now asserts the
> headline, the category summary, and `GOVERNANCE.md` against the table, so the third occurrence
> fails a test instead of waiting for someone to read carefully.

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
| **W-011** | `PLAN.md` §6.1 and constitution §Development and Phase Workflow #4 — *"Consequential decisions MUST be captured as proposed ADRs and reviewed before being treated as accepted"*, and spec FR-013, which requires that review to be **independent**. Applied to **ADR-0013, ADR-0014 and ADR-0015** as **one Phase 005 decision cluster**: exact OpenAPI 3.2 serialisation, schema-as-single-source validation, and stable API errors with RFC 9457 mapping. They are tightly coupled — 0013 emits what 0014 declares, and both publish what 0015 names — so a record-level waiver covers the cluster rather than three near-identical waivers covering one record each | The project has a single maintainer, who authored all three records and the code they justify. Under `GOVERNANCE.md` a qualified independent reviewer must be **a person**, **not the author**, **competent in the subject**, and **able to reject without the author's consent**; no available person satisfies criteria 1, 2 and 4. **W-002 covers Phase 001 records; W-004 names ADR-0007 alone; W-006 names ADR-0009 alone; W-009 names ADR-0012 alone. None reaches 0013–0015.** **W-005, W-008 and W-010 are phase-level and their own scope sections state they do not authorise accepting any decision record.** None is extended by reinterpretation | **Counted — these exist only because of this waiver:** (1) **each ADR's alternatives and consequences challenged separately**, not as a cluster — nine OpenAPI candidates compiled and run for 0013, four validation approaches measured for 0014, five Problem Details crates for 0015, each rejection resting on a measurement rather than a README; (2) **the package-research record corrected against itself** — the flat claim that no crate emits 3.2.0 was **false**, `salvo-oapi` 0.95.2 does, and the correction is dated in all three places it appeared rather than quietly rewritten; (3) **the OpenAPI 3.2 claim proven against the official schema with an anti-relabel negative control** — proof 3 rejects the document under 3.1 **with the version pattern neutralised**, so the rejection is structural, and proof 4 confirms a genuinely relabelled 3.1 document passes that same check, so the discriminator discriminates; the vendored schemas were verified **byte-identical to upstream** by `sha256` on 2026-08-23, and a deliberate tamper was **caught by proof 5**; (4) **single-source agreement executable rather than asserted** — one `Declaration` value is both interpreted and published, differentially tested against `jsonschema`; (5) **the error registry and its redaction proven structurally** — `detail()` returns `&'static str`, so no runtime value can inhabit it, with canary tests carrying positive controls; (6) **every ADR-related finding individually dispositioned**, including two the maintainer's own closing audit found after the security review had reported — **D-9** (a fail-open `multipleOf`) and **D-10** (a compatibility gate with no committed side, described by a contract that said otherwise) — both fixed and **mutation-proven**; (7) **every verified Critical, High and Medium finding fixed** before acceptance; (8) **no ADR moved to `accepted` until controls 1–7 were on the record.** **Preconditions — restated and deliberately NOT counted**, because a control another rule already mandates compensates for nothing: the alternatives-and-consequences analysis and the package-first evaluation are already required by **spec FR-035** and **principle III**; the accepted-ADR prerequisite for custom infrastructure is required by the same two; and the CI, dependency, licence, advisory, secret-scanning and code-quality gates already run unconditionally on every pull request | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under this ledger's ratchet rule rather than restarting the clock. W-006 was the first application, W-008 the second, W-009 the third, this the fourth.)* | A qualified independent human reviewer re-reviews **all three records in full** — the 3.2 serialisation decision and its bounded scope, the enforced-subset boundary and what it refuses, and the error vocabulary and its CLI-code disjointness — against the exact content bound below; each record's review field is updated with the outcome; W-011 closes | `active` |
| **W-012** | `PLAN.md` §6.1 step 10 and constitution §Development and Phase Workflow #7 — *"An independent review MUST compare implementation evidence with the specification, constitution, compatibility matrix, and security checklist."* Applied here to **Phase 005** (validation, Problem Details, and OpenAPI) | The project has a single maintainer, who authored every line of Phase 005 and the evidence describing it. No available person satisfies the `GOVERNANCE.md` criteria. **W-003, W-005, W-008 and W-010 are phase-level waivers for Phases 001–004 and confer no authority here**, and none is extended by reinterpretation. **This is the fifth consecutive phase to waive this rule for this reason.** Three reviews were commissioned to close this phase and **none delivered a report**; they are recorded as **NOT PERFORMED**, not as passes, and the work was done by the maintainer instead | **Counted — these exist only because of this waiver:** (1) **a complete FR-001…FR-066 and SC-001…SC-022 mapping** in [`phase-005-requirements-conformance.md`](phase-005-requirements-conformance.md), every entry carrying executable evidence rather than a document's assertion, and **0 PARTIAL, 0 NOT_MET, 0 UNTESTED** at the bound head; (2) **the final task validation outcome recorded** — all 42 tasks `validated`, moved only after that mapping existed, and **none** moved to `complete` by any agent; (3) **the security review and the L-14 completion audit both on the record at their true status** — the security review is **automated, therefore NON-INDEPENDENT and ADVISORY**, and the L-14 audit covering the ten areas it never reached is **maintainer self-review**, labelled as such in its own first sentence; (4) **every verified finding individually dispositioned**, ten defects across the phase, each reproduced before being recorded — including one hypothesis (schema tampering) that **reproduction disproved and which was therefore not filed**; (5) **executable evidence behind every SATISFIED claim**, with FR-043 and FR-044 recorded as having been **NOT MET at `d0f92cf`** and fixed in the closing audit rather than presented as always-green; (6) **the named limitations retained rather than silently absorbed** — seventeen, each with an owner and a target phase, with L-10 and L-14 **closed by work** rather than by this waiver; (7) **no phase closure until controls 1–6 were on the record.** **Preconditions — restated and deliberately NOT counted**: fixing Critical and High findings is already required by **PLAN.md §17.3**; the CI platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, and the format, clippy and rustdoc gates already run unconditionally; the dependency inventory is already required by **FR-040**; and `cargo xtask verify` is already required by the verification-sequence contract | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under the ratchet rule rather than restarting the clock. Fifth application.)* | A qualified independent human reviewer performs the requirements-and-security review of **Phase 005 in full**, against the exact merged content bound below, including the Windows platform legs and the ten L-14 areas the maintainer audited alone; the outcome is recorded in `governance/phase-005-evidence.md`; W-012 closes | `active` |
| **W-013** | constitution §Development and Phase Workflow #4 and spec FR-013 — a decision record MUST carry a recorded **independent** review before it is accepted. Applied here to **ADR-0016, ADR-0017, ADR-0018 and ADR-0019**, the four Phase 006 persistence and container records | The project has a single maintainer, who authored every line the records describe. No second qualified person exists to review them, so the rule cannot be satisfied without leaving four accepted decisions unrecorded — which is worse, because an unrecorded decision is not reviewable at all | (1) Each record states, in its own header, that no independent review occurred and that none is claimed; (2) every claim in each record is asserted by a test that fails if the property stops holding, not by the record itself — 1284 workspace tests on Rust 1.94.0 **and** 1.97.1, at default parallelism, twice; (3) **ADR-0018's central claim was mutation-tested**, **7 of 10** mutants killed with the three survivors argued as equivalent mutants from upstream source rather than waved through, and the pass found three real test defects plus one spurious kill, all corrected. This entry read "8 of 10 … two survivors" until an advisory review compared it against the cited evidence; the figure was taken from an intermediate run and is corrected here rather than quietly restated; (4) the container profile was validated against a **real Docker daemon** across five rows, 77 assertions, including a negative control proving the published port is unreachable off loopback; (5) every health check was verified to **fail** as well as to pass, which is how `valkey-cli ping` was found to be unusable; (6) `cargo xtask verify` passes steps 1–10 — formatting, lint, tests, API documentation, dependency and licence policy, architecture invariants with controls, secret scan over history and tree, documentation site, and link check | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent human reviewer becomes available — whichever comes first | A qualified independent reviewer reviews **each of the four records in full** — not the cluster in summary — against the merged implementation; findings are recorded and dispositioned; W-013 closes | `active` |
| **W-014** | `PLAN.md` §6.1 step 10 and constitution §Development and Phase Workflow #7 — *"An independent review MUST compare implementation evidence with the specification, constitution, compatibility matrix, and security checklist."* Applied here to **Phase 006** (persistence, migrations, and the container scope addition) | The project has a single maintainer, who authored every line of Phase 006 and the evidence for it. Automated reviews were commissioned and are **advisory**; they are not independent and are not counted as the required review | (1) 63 functional requirements each mapped to the test that asserts them, in `specs/006-persistence-sqlx/evidence/fr-conformance.md`, with **L-6 withdrawn rather than re-scoped** and the wrong diagnosis behind it corrected in writing; (2) four real database engines — PostgreSQL 17.11 and 18.6, MySQL 8.4.11 and 9.7.2 — exercised at the boundary rather than mocked; (3) feature isolation measured **with positive controls**, because a count of zero proves nothing without proof the walk can see what is there; (4) the real-database suites were made correct at **any** parallelism, removing a `--test-threads=1` dependency that nothing in the code stated and that would have failed the first time anyone ran `cargo test` normally; (5) generation proven offline both behaviourally and structurally — `renvor-cli` resolves neither `sqlx` nor any HTTP client; (6) no credential can reach a flag, a manifest, a generated file, or any output, and `renvor check` **refuses** a manifest that grew one | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent human reviewer becomes available — whichever comes first | A qualified independent reviewer performs the requirements-and-security review of **Phase 006 in full**, against the exact merged content, including the container profile and the `#[doc(hidden)]` dependency ADR-0018 records; the outcome is recorded in `governance/phase-006-evidence.md`; W-014 closes | `active` |
| **W-015** | constitution §Development and Phase Workflow #4 and spec FR-013 — a decision record MUST carry a recorded **independent** review before it is accepted. Applied here to **ADR-0020, ADR-0021 and ADR-0022**, the three Phase 007 SeaORM records | The project has a single maintainer, who authored every line the records describe. No second qualified person exists to review them, so the rule cannot be satisfied without leaving three accepted decisions unrecorded — which is worse, because an unrecorded decision is not reviewable at all | (1) Each record states, in its own header, that no independent review occurred and that none is claimed; (2) **ADR-0021's central claim was measured against real engines rather than argued** — `sea_orm::DatabaseTransaction` denies the pool its configured capacity for **9.506 s (MySQL 8.4.11)** and **9.510 s (PostgreSQL 17.11)** after one mid-statement cancellation, against Renvor's 2 s bound, and the record states plainly that this is *stranding* rather than the permanent leak ADR-0017 measured, because an unqualified claim would have contradicted ADR-0017 on PostgreSQL; (3) **ADR-0022's central claim is quoted from upstream source** — `sea-orm-migration` 2.0.2's whole bookkeeping model is `version: String, applied_at: i64`, and the string `checksum` appears nowhere in the crate; (4) the parity these records claim is **structural rather than asserted**: the pagination renderers and the seed types were moved into `renvor-database` so both adapters share one implementation, after a review found FR-033/FR-034 resting on an argument that was false; (5) feature isolation measured **with positive controls in both directions**, including the sibling assertion that neither adapter resolves the other — and the control caught a defect in the measuring harness itself before it could report a false pass; (6) the **package-first evaluation was recorded as a rejection with a reason**: `sea-orm-migration` was read, its bookkeeping model quoted, and the absence of a checksum column made the deciding fact — so a reader can check the decision against upstream source rather than against the author's summary of it **Preconditions — restated and deliberately NOT counted**: `cargo deny`, the format, clippy and rustdoc gates, the CI platform matrix, secret scanning, dependency review and CodeQL all run unconditionally on every branch. This ledger's own rule at §Compensating controls forbids citing them here, and W-013 and W-014 cited them anyway — a dependency review caught the regression against W-012, which had stated this exclusion explicitly | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent human reviewer becomes available — whichever comes first | A qualified independent reviewer reviews **each of the three records in full** — not the cluster in summary — against the merged implementation; findings are recorded and dispositioned; W-015 closes | `active` |
| **W-016** | `PLAN.md` §6.1 step 10 and constitution §Development and Phase Workflow #7 — *"An independent review MUST compare implementation evidence with the specification, constitution, compatibility matrix, and security checklist."* Applied here to **Phase 007** (SeaORM parity) | The project has a single maintainer, who authored every line of Phase 007 and the evidence for it. Automated reviews were commissioned and are **advisory**; they are not independent and are not counted as the required review | (1) Every functional requirement mapped to the evidence for it, and **re-scored honestly after a review falsified the original claim**: the record read "62 of 62 SATISFIED" and six requirements cited a file that existed only in a deleted scratch directory. It now reads 53 SATISFIED / 4 STRUCTURAL / 5 ARGUED, with `ARGUED` defined as *no executable check*. **That record lives in `specs/`, which is deliberately untracked, so a reviewer cannot fetch it from the repository** — the tracked summary is `governance/phase-007-evidence.md`, and this limitation is stated rather than left for a reviewer to discover; (2) the four-row matrix **found two real defects that a single-engine suite would have shipped** — a `?`-versus-`$1` placeholder in the contract fixture and the same mistake again in the seed ledger, each passing on one engine and failing on the other; (3) the cancellation feasibility gate was run **red-first against the native SeaORM transaction** and the result reported as a measured duration rather than a verdict, because a boolean assertion would have been flaky on PostgreSQL by construction and PLAN.md §17 treats a flaky test as a defect; (4) the generator's own pre-placement verification **refused** the first SeaORM templates twice — once on formatting, once on a module that could not compile — which is the gate working rather than being worked around; (5) offline generation proven with `CARGO_NET_OFFLINE=true` rather than asserted, after a real `sea-orm` dependency was designed, implemented, and then **withdrawn** on discovering it would put a registry fetch inside `renvor new`; (6) the governance defect this phase's own preconditions audit found — two waivers missing from every count and from `GOVERNANCE.md`'s table entirely — was corrected in a separate commit and is now asserted by a test that was verified to fail before it passed **Preconditions — restated and deliberately NOT counted**: the unconditional gates listed under W-015 compensate for nothing here and are not cited | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent human reviewer becomes available — whichever comes first | A qualified independent reviewer performs the requirements-and-security review of **Phase 007 in full**, against the exact merged content, including ADR-0021's departure from SeaORM's own transaction type; the outcome is recorded in `governance/phase-007-evidence.md`; W-016 closes | `active` |
| **W-017** | constitution §Development and Phase Workflow #4 and spec FR-013 — a decision record MUST carry a recorded **independent** review before it is accepted. Applied here to **ADR-0023**, the Phase 008 database-portability decision | The project has a single maintainer, who authored every line of the record and took every measurement in it. No second qualified person exists to review it, so the rule cannot be satisfied without leaving seven consequential normative choices recorded only as a contract — which is worse, because a contract states a rule without recording the alternatives that were rejected or the cost of the one taken | **Counted — these exist only because of this waiver:** (1) **the record states the differences it cannot remove rather than claiming to have removed them** — MySQL's `TIMESTAMP` ends in 2038, `ON DUPLICATE KEY UPDATE` cannot be scoped to one key, and MySQL DDL commits implicitly — because an adapter claiming otherwise would be reporting success after partial failure, which principle IV forbids; (2) **one claim in it was proven by mutation rather than argued**: M-18 removed the isolation probe's first read and the engine difference disappeared, so *"a MySQL transaction takes its snapshot at its first read, not at `BEGIN`"* is measured; (3) **a survivor is recorded as a survivor** — M-7a is unkillable on MySQL by construction, and it is kept and explained rather than deleted, which is what makes the rest of the ledger readable as evidence; (4) **the assertions key on `DatabaseKind` with no catch-all arm**, so a third engine cannot be added without a measurement — the panic reads *"has never been measured against this contract"*; (5) **two rounds of automated review were run against the reviewed tree and every finding was dispositioned by change rather than by argument** — sixteen in total, including one that falsified a safety claim published in `contracts/error-taxonomy.md`, one that falsified a mutation-kill claim in this phase's own evidence, and **two that falsified statements in this ledger** — a compensating control that produced corrections to the waiver justifying it. **Those reviews are automated, and are therefore ADVISORY and NON-INDEPENDENT**; they are counted as a control, never as the review this waiver waives. **Preconditions — restated and deliberately NOT counted**: (a) `cargo deny`, the format, clippy and rustdoc gates, the CI platform matrix, secret scanning, dependency review and CodeQL all run unconditionally on every branch; (b) **binding every decision to a measurement across all four rows**, which **PLAN.md §10.1** already makes first-class and mandatory; (c) **recording what each rejected alternative costs**, which **spec FR-035** and **principle III** already require of every decision record. **(b) and (c) were cited as counted controls until a review applied this ledger's own rule to them**: *"A control that another requirement already mandates unconditionally compensates for nothing and may not be cited."* They are restated here because the work was done and is real; they are not counted because they were owed anyway | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent human reviewer becomes available — whichever comes first | A qualified independent reviewer reviews **ADR-0023 in full** — the seven decisions and the alternatives each rejects — against the merged implementation and the measurements it cites; findings are recorded and dispositioned; W-017 closes | `active` |
| **W-018** | spec FR-027 and `PLAN.md` §6.1 step 10 — a phase MUST NOT close without a recorded **independent** requirements-and-security review. Applied here to **Phase 008** as a whole | The project has a single maintainer, who wrote every line under review and took every measurement it rests on. No second qualified person exists, and three separately commissioned reviewer agents returned no result at all | **Counted — these exist only because of this waiver:** (1) **twenty findings were raised against the reviewed tree across three automated rounds and all twenty were dispositioned by change**, not by argument — including one constitutional violation the gates had passed twice, two false published safety claims, a mutation this ledger had recorded as *killed* that had in fact survived, and a test that passed without exercising the property it named; (2) **the four-row persistence census is executable and derived from the suites it censuses**, so a deleted or feature-gated row fails step 4 — proven by two controls that fail at different gates; (3) **forty-five mutations were run and the two that survived are recorded as survivors**, with the false kill-claim corrected in place rather than erased; (4) **every normative claim narrowed this phase was narrowed to a measurement** — the JSON portable subset and the dirty-ledger recovery path each execute their own exclusions. **NOT counted:** the automated reviews themselves are advisory and non-independent, and the gate suite is required of every pull request regardless | Ahmed Anbar | **2027-02-11**, or immediately when a qualified independent human reviewer becomes available — whichever comes first | Commission the independent requirements-and-security review of Phase 008, act on its findings, and close the waiver | `active` |
| **W-019** | constitution §Development and Phase Workflow #7 and `GOVERNANCE.md` — an architecture decision record MUST NOT reach `accepted` without an **independent** review. Applied here to the **seven Phase 009 records ADR-0024 … ADR-0030** as ONE coupled cluster, because each depends on the boundary another draws and reviewing one in isolation would review a fragment | The project has a single maintainer, who authored all seven records and took every measurement they rest on. No second qualified person exists. **W-011/W-013/W-015 are ADR-cluster waivers for Phases 005, 006 and 007 and confer no authority here**, and none is extended by reinterpretation | **Counted — these exist only because of this waiver:** (1) **ADR-0029's central claim is derived and executed, not asserted** — the storage bound is `|AttemptDimension| × buckets = 12 × 65 536 = 786 432` rows, enforced structurally by `PRIMARY KEY (dimension, bucket)` over a masked keyed HMAC, and it holds **whether or not cleanup runs**; an attack test drives 400 distinct identifiers through every dimension on **all four persistence rows** and asserts the stored row count never exceeds the documented bound; (2) **ADR-0030's placement decision was REFUTED before it was recorded** — `plan.md` §1 named `renvor-http` as the host, and step 7's CLAIM 3 disproved it against the resolved dependency graph; the three rejected alternatives are written down with the reason each fails, so the record documents a decision that survived a disproof rather than one that was never tested; (3) **ADR-0024's justification was corrected against the primary source after a research agent contradicted it** — the record claimed NIST SP 800-63B-4 "does not define 'character'", the standard contains a `SHALL` that does, and the text was re-fetched and confirmed before four files were corrected; the implementation was already right, which is the dangerous shape: nothing fails and the record quietly claims discretion where a mandate exists; (4) **each record's operative decision is pinned by an executable test rather than by its own prose** — including `compile_fail` doctests with compiling controls for the single-use capability types ADR-0027 and ADR-0029 depend on. **Preconditions — restated and deliberately NOT counted**: the CI platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, and the format, clippy and rustdoc gates all run unconditionally; `cargo xtask verify` is already required by the verification-sequence contract; package metadata is already required by FR-063 | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under the ratchet rule rather than restarting the clock.)* | A qualified independent reviewer reviews **all seven records as one cluster** against the exact head and tree named below, the outcome is recorded in `governance/phase-009-evidence.md`, and W-019 closes | `active` |
| **W-020** | spec FR-027 and `PLAN.md` §6.1 step 10 — a phase MUST NOT close without a recorded **independent** requirements-and-security review. Applied here to **Phase 009** as a whole | The project has a single maintainer, who wrote every line under review and took every measurement it rests on. No second qualified person exists. **This is the NINTH consecutive phase-level waiver of this rule for this reason** (W-003, W-005, W-008, W-010, W-012, W-014, W-016, W-018, and now W-020). The trend guard was tripped at three and has deepened every phase since. **Three of four delegated agents returned nothing at all this phase**, and the required Codex review went idle twice and is recorded `NOT PERFORMED` rather than inferred clean | **Counted — these exist only because of this waiver:** (1) **two commissioned validators DELIVERED against a named head and found fourteen findings between them, four of which broke published security claims**; nine were fixed in code and the remainder recorded with the reason each was not; (2) **the security review found a defect that had been fixed the previous day on a sibling type and then re-introduced** — `Admitted` derived `Copy`, exactly what commit `705d34d` had removed from `Authorized` one commit earlier, making one counted attempt admit unlimited calls and falsifying FR-063's structural claim plus three shipped sentences; self-review had passed over it twice; (3) **every finding fixed carries a NEW executable pin**, not a corrected sentence — a `compile_fail` doctest spending an admission twice with a compiling control, and a test asserting three calls cost three counted attempts; (4) **114 controlled mutations were run this phase and every survivor was investigated to a conclusion rather than explained away** — `G-M3` survived and the first harness wrongly called its test decorative; a second experiment proved the issuer and audience checks are individually redundant and jointly load-bearing, and the false claim was corrected in place rather than erased; (5) **the four-row persistence census is derived from the suites it censuses and was extended to every new abuse-control suite**, so a deleted or feature-gated row fails step 4; (6) **the repository's own gates caught five further defects after both reviews had reported, and every one was diagnosed to a proven root cause and pinned with a NEW executable control rather than retried until green** — an error code with no declared status, a stale committed OpenAPI snapshot, ten credential-handling diagnostics that printed what they asserted about, a publishable-package count that disagreed with its own table, and a defect in the VERIFICATION APPARATUS ITSELF: step 4's end-to-end relay charged a cold `cargo` build against a 300-second deadline written to bound how long a binary takes to ANSWER, and the `--all-features` step immediately before it structurally guaranteed the cache miss, so a correctly wired transport was reported as `TransportNotWired`. It was reproduced under instrumentation, fixed by compiling before relaying with the deadline left untouched at 300 seconds, and pinned by two guards whose discrimination was proved by three negative controls — including a tripwire that fails if a future maintainer answers a recurrence by widening the deadline. **Preconditions — restated and deliberately NOT counted**: fixing Critical and High findings is already required by `PLAN.md` §17.3; the CI platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, and the format, clippy and rustdoc gates already run unconditionally; the dependency inventory is already required by FR-040; and `cargo xtask verify` is already required by the verification-sequence contract | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under the ratchet rule rather than restarting the clock. Ninth application.)* | A qualified independent reviewer performs the requirements-and-security review of **Phase 009 in full**, against the exact head and tree named below, including the Windows platform legs; the outcome is recorded in `governance/phase-009-evidence.md`; W-020 closes | `active` |
| **W-021** | constitution §Development and Phase Workflow #7 and `GOVERNANCE.md` — an architecture decision record MUST NOT reach `accepted` without an **independent** review. Applied here to the **seven Phase 010 records ADR-0031 … ADR-0037** as ONE coupled cluster — the port-and-substitute shape (0031), the job storage decision (0032), the four adapter selections (0033–0036), and the retry policy (0037) each depend on a boundary another draws, so reviewing one in isolation would review a fragment | The project has a single maintainer, who authored all seven records and took every measurement they rest on. No second qualified person exists. **W-019 is Phase 009's ADR-cluster waiver and confers no authority here**, and none is extended by reinterpretation | **Counted — these exist only because of this waiver:** (1) **every package decision is measured against the real lockfile (490 → 528 packages), not asserted** — each candidate's additions, its licences across every target `deny.toml` evaluates, its advisories with positive controls, and its feature isolation against `cargo tree` (`phase-010-dependency-inventory.md`); (2) **ADR-0035's central claim is a refusal measured four ways** — every S3 candidate failed a named gate, and the three routes a later phase could take are written down rather than one taken quietly; (3) **ADR-0033's single-provider rule is executable** — `xtask` step 7 walks the feature edges and refuses a second `rustls` provider, and that walk found `renvor-cache` shipping with **none**, which was corrected before the record was relied on; (4) **ADR-0036's "the formatter must be Renvor's" claim was verified against the crate's source** and is pinned by a test planting a canary in an event field, a span field, and a nested span field; (5) **each record's operative decision is pinned by an executable test** — **136 controlled mutations** across the phase (88 in the phase, 40 in the correction round, 8 in the L-16 correction) with every survivor investigated to a conclusion, the two survivors recorded as predicted-equivalent with the reason (`phase-010-mutation-ledger.md`); (6) **ADR-0031's compliance claim against constitution principle VII was withdrawn in place when the maintainer's review rejected it** — the record now says the obligation is unmet, and acceptance under this waiver accepts the decisions the record makes and not the reading it withdrew; the obligation is carried visibly as L-14 under W-024, not as an accepted record's silent assertion. **Preconditions, restated and deliberately NOT counted**: the CI platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, the format, clippy and rustdoc gates, and `cargo xtask verify` all run unconditionally already | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under the ratchet rule rather than restarting the clock.)* | A qualified independent reviewer reviews **all seven records as one cluster** against the exact head and tree named below, the outcome is recorded in `governance/phase-010-evidence.md`, and W-021 closes | `active` |
| **W-022** | spec FR-027 and `PLAN.md` §6.1 step 10 — a phase MUST NOT close without a recorded **independent** requirements-and-security review. Applied here to **Phase 010** as a whole | The project has a single maintainer, who wrote every line under review and took every measurement it rests on. No second qualified person exists. **This is the TENTH consecutive phase-level waiver of this rule for this reason** (W-003, W-005, W-008, W-010, W-012, W-014, W-016, W-018, W-020, and now W-022). The trend guard was tripped at three and has deepened every phase since. The three commissioned research agents delivered on the first commission this phase and the required Codex review was performed — by the maintainer, on the pull request head, so it is advisory and is not independent | **Counted — these exist only because of this waiver:** (1) **the repository's own gates, the real servers, and the pull request's platform legs found ten defects after batches were green, plus three further findings** (a secret-scanner false positive with the injection proof its policy demands, a licence allow-list out of step with `deny.toml`, and two test fixtures CodeQL read as hard-coded passwords) — thirteen rows of `phase-010-review-record.md` §2, each fixed at the root and pinned, none retried until green; (2) **the maintainer's Codex review on `1328dd3` returned sixteen findings, every one verified against the tree before a change was made; fifteen were corrected at the root in one bounded round with a RED→GREEN test and a controlled mutation each and nothing weakened; the sixteenth (constitution VII, finding 5) was confirmed, recorded as L-14 with ADR-0031's compliance claim withdrawn in place, and is now deferred under W-023 and W-024 rather than closed** (§3); (3) **the maintainer then ruled the round's retained L-16 a correctness blocker and it was reproduced by five tests written first and corrected the same day** — the handler's own task aborted and joined before its lease is released, a lease under a handler that cannot be dropped withheld and reported — with eight mutations, eight killed (§3b); (4) **two Phase 009 limitations are closed by measurement, not prose** — L-4 by a transport guard comparing the full effective origin, driven nine ways through the PostgreSQL-backed flow with a valid token, L-11 by a recorded event asserted field by field; (5) **every adapter is exercised against a real server** — Valkey 9.1.1, PostgreSQL 17, MySQL 8.4, Mailpit 1.29.1, a local OTLP receiver, a real filesystem — each with a redaction canary sweep, and the census requires the four job rows to report in (a misspelled row was proved to fail it); (6) **136 controlled mutations with every survivor investigated** — 88 in the phase, 40 in the correction round, 8 in the L-16 correction; two predicted-equivalent survivors recorded with the reason; one killed only by the harness wall clock and recorded as a hang; (7) **both `cargo xtask verify` legs are green on the head named below — 2065 passed / 0 failed / 5 ignored, census 67/67 — and the pull request's checks on its final head include both Windows legs**; (8) **the tree's "arrives in Phase 010" promises were corrected with their pinning tests**. **Preconditions, restated and deliberately NOT counted**: fixing Critical and High findings is already required by `PLAN.md` §17.3; the platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, the format, clippy and rustdoc gates, the dependency inventory (FR-040), and `cargo xtask verify` are all already required | Ahmed Anbar | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — **whichever occurs first**. *(Inherits the **earliest open expiry** under the ratchet rule rather than restarting the clock. Tenth application.)* | A qualified independent reviewer performs the requirements-and-security review of **Phase 010 in full**, against the exact head and tree named below, including the Windows platform legs; the outcome is recorded in `governance/phase-010-evidence.md`; W-022 closes | `active` |
| **W-023** | constitution principle VII (Deterministic and Safe Generation) — *"The governed choice set is target, transport, persistence model, database, **auth starter**, frontend, compatible render mode, styling profile where applicable, desktop option, capabilities, and local tooling; each becomes mandatory in both interfaces on the day its capability ships."* Applied here to **Phase 009's authentication capability**: `renvor-auth` and `renvor-auth-http` shipped on 2026-08-31 (W-020), and `renvor new` still neither asks for nor honours an auth starter — `--auth` is a reserved flag, parsed and then refused with exit 3 naming Phase 011 | Phase 009 read "ships" as "generated projects gain the wiring", recorded `--auth` as reserved until Phase 011, and stated the gap against `PLAN.md` §6.2 rather than ticking it off (`phase-009-evidence.md` §6). The maintainer's Phase 010 review rejected that reading (finding 5; `phase-010-limitations.md` L-14), and on 2026-09-04 ruled that the meaning of "ships" is **not reinterpreted or weakened**: the obligation is real and unmet. It cannot be met by the narrowest literal change — a mandatory `--auth` choice recorded in `renvor.toml` that the generator cannot honour would be exactly the inert choice the same principle forbids — and honouring it is the generator scope `PLAN.md` assigns to Phase 011. **This is not a review-gap waiver and its cause is different from every other exception in this ledger**: nobody was missing; a rule was read too narrowly for one phase and the reading was rejected in the next. **W-023 begins on 2026-09-04 and is not retroactive**: it does not make Phase 009's closure on 2026-08-31 compliant with principle VII, and W-020 never covered this rule | **Counted — these exist only because of this waiver, and each is specific to the gap:** (1) **unsupported auth inputs continue to fail explicitly rather than being recorded as inert choices** — `--auth` is parsed and then refused with exit 3 naming Phase 011 in every place that states it (`renvor-cli`'s `RESERVED` table; pinned by `a_reserved_flag_parses_and_then_fails_validation`, `the_reserved_auth_phase_is_stated_identically_everywhere`, and `every_reserved_flag_in_the_table_is_declared_on_the_struct`), and the governed-choice test classifies "auth starter" as `Reserved`, so a silently defaulted or recorded-but-inert auth choice cannot be introduced without failing it; (2) **Phase 011 MUST implement wizard and non-interactive parity for the auth starter, validated `renvor.toml` persistence of the choice, actual dependency and project wiring of `renvor-auth` and `renvor-auth-http` into the generated project, and generated-project compile and start tests** — the generated project must build and start with the starter selected before this waiver can close; (3) **the relevant four-row and auth combinations MUST be exercised before the waiver closes** — the starter generated against each of the four persistence rows (direct SQLx and SeaORM, each on PostgreSQL and MySQL), with the auth migrations applied and the authenticated flow driven end to end, censused like every other four-row suite; (4) **no tag, release, deployment, or crate publication while W-023 remains active** — an unmet constitutional obligation is a release blocker, and this waiver does not lift it. **Preconditions, not counted**: the verification sequence, the platform matrix, the scanners, and the four-row census already run unconditionally | Ahmed Anbar | **2026-10-04**, or **earlier** when Phase 011 implements and proves the auth-starter generator support named in the removal plan — **whichever occurs first**. *(The ratchet rule does not apply: this is not the single-maintainer gap. The date is earlier than every review-gap expiry, so the horizon does not move.)* | Phase 011 ships `renvor new --auth` and its wizard question with controls (2) and (3) proven by executable tests on all four rows; the proof is recorded in Phase 011's evidence against a named head and tree; L-14 is then closed with the measurement, and W-023 closes. If 2026-10-04 arrives first, W-023 is expired-but-open — a release blocker by this ledger's own rule — until it is re-justified and re-dated or the rule is complied with | `closed` — **closed 2026-09-05**, before its expiry, against head `5eff451c435c8676aaa3cd231ccfc7d2e5ec5ba0`, tree `d1cab4cb7b1a1a18e387689e6ad3fdd0f6a628f9`: `renvor new` asks for and honours the auth starter in the wizard and in `--auth`, persists it in `renvor.toml`, wires `renvor-auth` and `renvor-auth-http` into the generated project, and the starter is generated, built, migrated, started, authenticated, and authorized on all four persistence rows by executable tests censused with every other four-row suite (86 rows) — controls (2) and (3); control (1) holds (`api`/`full` refused as `unsupported_value`, the governed-choice test pins `Honoured`); the three negative controls fired. Recorded in `phase-011-evidence.md` §2 and §4 and reviewed in `phase-011-review-record.md` §2; L-14 is closed with the measurement (`phase-010-limitations.md`, `phase-011-limitations.md`). **Not retroactive**: Phase 009's closure on 2026-08-31 stays recorded as non-compliant |
| **W-024** | constitution principle VII (Deterministic and Safe Generation) — *"… **capabilities** … each becomes mandatory in both interfaces on the day its capability ships."* Applied here to **Phase 010's five capabilities**: `renvor-cache`, `renvor-jobs`, `renvor-mail`, `renvor-storage`, and `renvor-observability` shipped on 2026-09-04 (W-022) as library crates behind the facade's `capability-*` features, and `renvor new` neither asks for nor honours a capabilities choice for them — no such flag or wizard question exists, and the generator's existing "capabilities" row covers `--example-domain`, `--seed-data`, and `--container` only | ADR-0031 read "ships" the way Phase 009 had and made no generator change; the maintainer's review rejected that reading (finding 5; `phase-010-limitations.md` L-14) and ADR-0031's compliance claim was withdrawn in place. On 2026-09-04 the maintainer ruled that the meaning of "ships" is **not reinterpreted or weakened**: the obligation is real and unmet. The narrowest literal implementation — a mandatory `--capabilities` choice recorded in `renvor.toml` — would solicit and record a choice the generator cannot honour (a generated project declares no Renvor dependency until one is publishable), which the same principle forbids; honouring it is Phase 011's generator scope. **This is not a review-gap waiver and its cause is different from every review-gap exception in this ledger.** It is Phase 010's third explicit reviewed exception, and it is granted as its own waiver because a waiver covers one rule, at one level, in one phase — W-023 covers Phase 009's auth starter and cannot be stretched over this | **Counted — these exist only because of this waiver, and each is specific to the gap:** (1) **unsupported capability inputs continue to fail explicitly rather than being recorded as inert choices** — no `--capabilities` flag and no wizard question for the five capabilities exists, so none can be recorded as an inert choice; an unknown `--capabilities` input is refused by the argument parser rather than accepted; and the governed-choice test pins the row's current classification, so a silently defaulted or recorded-but-inert capabilities choice cannot be introduced without failing it; (2) **Phase 011 MUST implement wizard and non-interactive parity for the capabilities choice, validated `renvor.toml` persistence of the choice, actual dependency and project wiring — the facade's `capability-*` features and each selected adapter feature — into the generated project, and generated-project compile and start tests** — the generated project must build and start with each selected capability before this waiver can close; (3) **the relevant four-row and capability combinations MUST be exercised before the waiver closes** — at minimum the jobs capability generated against each of the four persistence rows with its migrations applied, and each other capability's generated wiring compiled and started against its real server (Valkey, an SMTP sink, a filesystem root, an OTLP receiver), censused like every other four-row suite; (4) **no tag, release, deployment, or crate publication while W-024 remains active** — an unmet constitutional obligation is a release blocker, and this waiver does not lift it. **Preconditions, not counted**: the verification sequence, the platform matrix, the scanners, step 7's feature-isolation rows, and the four-row census already run unconditionally | Ahmed Anbar | **2026-10-04**, or **earlier** when Phase 011 implements and proves the capabilities generator support named in the removal plan — **whichever occurs first**. *(The ratchet rule does not apply: this is not the single-maintainer gap. The date is earlier than every review-gap expiry, so the horizon does not move.)* | Phase 011 ships the capabilities choice in `renvor new` and its wizard question with controls (2) and (3) proven by executable tests; the proof is recorded in Phase 011's evidence against a named head and tree; L-14 is then closed with the measurement, and W-024 closes. If 2026-10-04 arrives first, W-024 is expired-but-open — a release blocker by this ledger's own rule — until it is re-justified and re-dated or the rule is complied with | `closed` — **closed 2026-09-05**, before its expiry, against head `5eff451c435c8676aaa3cd231ccfc7d2e5ec5ba0`, tree `d1cab4cb7b1a1a18e387689e6ad3fdd0f6a628f9`: `renvor new` asks for and honours the capabilities choice in the wizard and in `--capabilities`, persists it in `renvor.toml`, wires the facade's `capability-*` features and each selected adapter feature into the generated project, and each capability is generated, built, and started against its real server — `jobs` with its migrations on all four persistence rows, `cache` against Valkey, `mail` against an SMTP sink, `storage` against a filesystem root, `observability` against an OTLP receiver — by executable tests censused with every other four-row suite (86 rows) — controls (2) and (3); control (1) holds (an unknown, duplicated, or `none`-beside-a-name input is refused; the governed-choice test pins `Honoured`; the lock closure walked from the runtime dependencies holds no unselected capability crate); the three negative controls fired. Recorded in `phase-011-evidence.md` §2 and §4 and reviewed in `phase-011-review-record.md` §2; L-14 is closed with the measurement |

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
| 2026-08-23 | **Still no candidate approached** — no recruitment activity of any kind has occurred since this obligation was created on 2026-08-19. Two further waivers granted for Phase 005: **W-011** (ADRs 0013–0015 as one cluster) and **W-012** (Phase 005 closure), taking the category to **ten** and making Phase 005 the **fifth consecutive** phase-level waiver. **The gap widened materially this time rather than merely repeating**: Phase 005 is the largest phase to date, all three commissioned closing reviews returned **nothing** and are recorded as NOT PERFORMED, and the maintainer's own audit found a **compatibility gate that could not fail** while a normative contract asserted it could — a defect that survived 1039 tests and three reviews, and exactly what an independent reviewer exists to catch. **It is not progress.** The 2026-11-19 review date and failure condition are unchanged and now govern **five** phase-level waivers |
| 2026-08-24 | **Still no candidate approached** — five days after this obligation was created, and no recruitment activity of any kind has occurred. Two further waivers granted for Phase 006: **W-013** (ADRs 0016–0019 as one cluster) and **W-014** (Phase 006 closure), taking the category to **twelve** and making Phase 006 the **sixth consecutive** phase-level waiver. **The gap widened materially again**: a limitation shipped in this phase (L-6) rested on a diagnosis that was simply wrong, a documented bound was enforced by nothing while the documentation claimed otherwise, and mutation testing found three test defects plus one test that appeared to prove a guard worked while measuring nothing — all four found by the author auditing his own work, which is the arrangement this obligation exists to end. **It is not progress.** The 2026-11-19 review date and failure condition are unchanged and now govern **six** phase-level waivers |
| 2026-08-24 | **Still no candidate approached.** Two further waivers granted for Phase 007: **W-015** (ADRs 0020–0022 as one cluster) and **W-016** (Phase 007 closure), taking the category to **fourteen** and making Phase 007 the **SEVENTH consecutive** phase-level waiver. **What widened the gap this time is what the audit found in the governance record itself**: the active-waiver count had been wrong since Phase 006 — the headline said eleven while the table carried thirteen, and `GOVERNANCE.md` was missing both rows entirely — so the cross-check that caught the *identical* defect on 2026-08-21 did not catch its recurrence. A ledger that miscounts its own waivers is the clearest possible evidence that self-review has a ceiling. It was found by the author auditing his own work, again, which is the arrangement this obligation exists to end. **It is not progress.** The 2026-11-19 review date and failure condition are unchanged and now govern **seven** phase-level waivers |
| 2026-08-31 | **Still no candidate approached** — **twelve days** after this obligation was created, and **no recruitment activity of any kind has occurred**. Two further waivers granted for Phase 009: **W-019** (ADR-0024–ADR-0030 as one coupled cluster) and **W-020** (Phase 009 closure), taking the category to **eighteen** and making Phase 009 the **NINTH consecutive** phase-level waiver. **This log skipped Phase 008 entirely.** W-017 and W-018 were granted 2026-08-27 and no entry was appended here, so the eighth consecutive phase-level waiver was never recorded against this obligation; the omission was found while appending this entry, by the author auditing his own work. It is not corrected by a backdated row — entries are appended, never rewritten — so the gap between the 2026-08-24 row and this one is left visible. **The gap is now legible in the delegation record as well as the review record**: **four of seven** commissioned agents returned nothing at all this phase, the required Codex review went idle twice and is recorded `NOT PERFORMED` rather than inferred clean, and the two validators that did report found **fourteen** findings — four breaking published security claims, one of them a `Copy` derive that had been removed from a sibling type the previous day and re-introduced here, which self-review passed over twice. A sixth defect this phase was in the **verification apparatus itself**: step 4's relay charged a cold build against a deadline meant for answering and reported a correctly wired transport as broken — a gate that rejects correct work is the failure mode that trains a maintainer to re-run until green. **It is not progress.** The 2026-11-19 review date and failure condition are unchanged and now govern **nine** phase-level waivers |
| 2026-09-04 | **Still no candidate approached** — **sixteen days** after this obligation was created, and **no recruitment activity of any kind has occurred**. Two further waivers granted against this gap for Phase 010: **W-021** (ADR-0031–ADR-0037 as one coupled cluster) and **W-022** (Phase 010 closure), taking the review-gap category to **twenty** and making Phase 010 the **TENTH consecutive** phase-level waiver. **Two other waivers granted the same day — W-023 and W-024 — are deliberately not counted here**: they waive the timing of constitution principle VII's generator obligation for Phase 009's auth starter and Phase 010's capabilities, their cause is a rule read too narrowly and then read correctly, and attributing them to this obligation would make the staffing gap look like the cause of something it did not cause. **What widened the gap this time is what self-review missed twice in a row**: the Codex review the maintainer ran on the pull request head returned **sixteen** findings after the checkpoint's own gates were green — two of them security-relevant at the transport and the credential surfaces, one a false clause in the phase's own acceptance table, one a specification requirement with no implementation — and the bounded round that corrected fifteen of them recorded a detached handler task as a *retained limitation* that the maintainer then had to rule a correctness blocker before it was fixed. Each was found by the author reviewing his own work with a tool, which is the arrangement this obligation exists to end. **It is not progress.** The 2026-11-19 review date and failure condition are unchanged and now govern **ten** phase-level waivers |

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

### W-011 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-23.** The following limits are part of the grant:

- **W-011 waives only the independent-human-review requirement for accepting ADR-0013, ADR-0014 and
  ADR-0015.** It reaches no other record, in this phase or any other.
- **W-011 does not authorise closing Phase 005.** It is a *record-level* waiver, on the same terms
  W-004 sets for ADR-0007, W-006 for ADR-0009 and W-009 for ADR-0012. Phase closure is **W-012**,
  granted separately for that purpose.
- **One waiver covers three records because they are one decision.** ADR-0013 emits what ADR-0014
  declares, and both publish the vocabulary ADR-0015 defines. Splitting them would produce three
  near-identical waivers over one coupled decision and inflate the ledger without adding scrutiny.
  **It does not widen the scope**: each record was challenged separately, and the removal plan
  requires each to be re-reviewed in full.
- **It confers nothing on Phase 006, and nothing on any release.** A waiver for one phase's records
  is not standing permission for the next.
- **It does not waive any product defect, missing test, missing functionality, publication rule, CI
  failure, or security blocker.** Where a requirement was unevidenced the remedy applied was a test:
  **FR-043 and FR-044 were NOT MET** at `d0f92cf` and were closed with a committed snapshot and
  three guarding tests, not with this waiver.
- **Security release blockers are never waived.**
- **Automated review is NON-INDEPENDENT and ADVISORY.** The security review performed in this phase
  was thorough, measured rather than argued, and found six defects a 1039-test suite had missed —
  and it is still automated. Its value is not in question; its *category* is.
- **Reviews that returned nothing are NOT PERFORMED**, never passes. Three closing reviews returned
  nothing; see `governance/phase-005-evidence.md` §9a.
- It expires **2027-02-11** or when a qualified independent human reviewer becomes available,
  **whichever occurs first**.
- **It authorises no publication, no tag, no release, no deployment, no repository settings change,
  and no admin bypass.**

### W-012 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-23.** The following limits are part of the grant:

- **W-012 waives only the independent-human-review requirement for closing Phase 005.**
- **W-012 does not authorise accepting any decision record**, on the same terms W-005 sets for
  Phase 002, W-008 for Phase 003 and W-010 for Phase 004. The acceptance of ADRs 0013–0015 is
  **W-011**.
- **It confers nothing on Phase 006.** Phase 006 has not started, and this record grants it nothing.
- **It authorises no publication, no tag, no release, no deployment, no repository settings change,
  and no admin bypass.** Zero crates are published, zero tags and zero releases exist, and zero
  deployments have been made — verified against the live registry and the live repository on
  2026-08-23, not assumed.
- **It does not waive any product defect, missing test, missing functionality, publication rule, CI
  failure, or security blocker.**
- **Phase 005 closes with its open review gap transferred and visible rather than closed**, on the
  same terms W-003, W-005, W-008 and W-010 set for the phases before it.
- **The named limitations Phase 005 carries stay named**: `pattern` and the composition keywords
  refused rather than silently unenforced (Phase 012), cookies and security schemes (Phase 009),
  `format` published as an annotation per JSON Schema 2020-12, pagination as contracts-only (Phase
  006), idempotency and conditional requests (Phase 006+), `renvor openapi` reaching no *generated*
  project because nothing is published, the relay deadline bounding this process rather than the
  orphaned grandchild (Phase 012), `$ref`-typed parameters never coerced and failing closed (Phase
  012), `uniqueItems` transient memory (Phase 012), `compat::compare`'s unbounded recursion (Phase
  012), property testing covering only the cursor decoder (Phase 012), no live concurrent load test
  of the relay path (Phase 012), and **`AGENTS.md` not existing** in the tree or in history. Each is
  permitted by its governing requirement. **None is waived by this record**, and this record must
  never be cited as covering them.
- It expires **2027-02-11** or when a qualified independent human reviewer becomes available,
  **whichever occurs first**.

#### The head and tree these waivers are bound to

| Field | Value |
|---|---|
| **Branch** | `feat/phase-005-validation-problem-openapi` |
| **Verified implementation and evidence head** | `b2bcd06f8115f380b3b66aba95280d4a23a513d0` |
| **Tree** | `355b0f45e9a7140a975bc7c339dd8dc280b4c935` |
| **Decision records (W-011)** | **ADR-0013, ADR-0014, ADR-0015** |
| **Gate result at that head** | `cargo xtask verify` **11/11** on **1.94.0** and on **stable**; 1042 passed / 0 failed / 2 ignored; worktree clean; `git ls-files specs .specify/feature.json` = 0 |

The final pull-request head adds **only** the two waiver records above and the three ADR status
changes they authorise — no implementation and no evidence change. That is verifiable rather than
asserted: `git diff b2bcd06..<final head>` touches `governance/waivers.md`, `GOVERNANCE.md`,
`governance/phase-005-evidence.md`, and the three decision records, and nothing else.

The re-review both removal plans require is against **that content**, not against whatever `main`
holds when it is performed.

#### This is the FIFTH consecutive phase-level waiver, and the trend guard is already tripped

> **Updated 2026-08-24 (Phase 007).** It is now the **SEVENTH**. The heading keeps its original
> wording because links point at its anchor, and rewriting it each phase would quietly erase how
> long this has been true. Phase 006 made it six; Phase 007 makes it seven.
>
> **Updated 2026-08-31 (Phase 009).** It is now the **NINTH**. Phase 008 made it eight (W-018);
> Phase 009 makes it nine (W-020). Four consecutive phases have now been added to this note
> without the underlying gap narrowing once. RO-001 remains open, and Phase 009 is the phase in
> which **four of seven commissioned agents returned nothing at all** and the required Codex
> review went idle twice and is recorded `NOT PERFORMED` rather than inferred clean.
>
> The trend guard was tripped at three. It has now been tripped for five consecutive phases without
> the underlying condition changing, and **RO-001 has produced no recruitment activity of any kind**
> since it was created on 2026-08-19. Nothing here reduces that; this note exists so that the
> deepening is recorded rather than absorbed.
>
> **Updated 2026-09-04 (Phase 010).** It is now the **TENTH**. Phase 010 makes it ten (W-022).
> Five consecutive phases have now been added to this note. This phase's shape differs from Phase
> 009's in one respect and not in the one that matters: every commissioned agent delivered and the
> required Codex review was performed — by the maintainer, whose sixteen findings were corrected in
> one bounded round — but no second person read a line, and RO-001 still records no recruitment
> activity, sixteen days after it was created.

W-003 (Phase 001), W-005 (Phase 002), W-008 (Phase 003), W-010 (Phase 004) and now **W-012 (Phase
005)** waive the same rule for the same reason in five consecutive phases. The guard was already
**TRIPPED** at three; a fifth does not trip it again, it deepens it.

**The gap widened while the obligation did not move.** Phase 005 is the largest phase so far — three
new crates, 66 requirements, 1042 tests — and it is the phase in which the maintainer's own audit
found a **compatibility gate that could not fail**, described by a contract asserting that it could.
That defect survived a 1039-test suite and three commissioned reviews. It is precisely the class of
thing an independent reviewer exists to catch, and precisely the class of thing five consecutive
waivers have deferred.

**What that means concretely, stated rather than softened:** the condition keeping this from being
an unaddressed release blocker is **RO-001**, the dated reviewer-recruitment obligation created with
W-008. RO-001's first review date is **2026-11-19** and has **not** yet arrived, so nothing about
this grant discharges it and nothing here should be read as progress against it. Its failure
condition is unchanged and now governs **five** phase-level waivers rather than four: if neither a
candidate nor a written account of what was attempted is recorded by that date, **every open waiver
in this category is treated as expired**, and an expired-but-open waiver is a release blocker by
this ledger's own rule.

**Eight waivers across four phases was one problem recorded eight times. Ten across five is the same
problem recorded ten times.** The remedy is a second qualified person, not an eleventh recording.

### W-013 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-24.** The following limits are part of the grant:

- **W-013 waives only the independent-human-review requirement for accepting ADR-0016, ADR-0017,
  ADR-0018 and ADR-0019.** It reaches no other record, in this phase or any other.
- **W-013 does not authorise closing Phase 006.** It is a *record-level* waiver, on the same terms
  W-004 sets for ADR-0007, W-006 for ADR-0009, W-009 for ADR-0012 and W-011 for ADRs 0013–0015.
  Phase closure is **W-014**, granted separately.
- **One waiver covers four records because they are one decision.** ADR-0016 chooses the driver and
  the ports; ADR-0017 fixes what owning a pooled connection costs; ADR-0018 is the consequence of
  needing that connection reachable from the kernel's boxed future; ADR-0019 generates the thing an
  operator runs all three against. Splitting them would produce four near-identical waivers over one
  coupled decision. **It does not widen the scope**: each record was challenged separately, and the
  removal plan requires each to be re-reviewed in full.
- **ADR-0018 records a dependency on a `#[doc(hidden)]`, semver-exempt upstream item.** That is the
  single most consequential thing in this cluster, and it is the thing an independent reviewer would
  most obviously be asked to weigh. It is bounded by one call site, a compile guard asserting both
  the signature and the `Send` bound, and a written fallback — and none of that is a substitute for
  the review.
- **It confers nothing on Phase 007, and nothing on any release.**
- **It does not waive any product defect, missing test, missing functionality, publication rule, CI
  failure, or security blocker.** Where a requirement was unevidenced the remedy applied was a test:
  **FR-021 was NOT MET** and was closed with a 30-test suite observed red before the fix and green
  after — not with this waiver. **L-6 was withdrawn, not renamed.**
- **Security release blockers are never waived.**
- **Automated review is NON-INDEPENDENT and ADVISORY.**
- **Reviews that returned nothing are NOT PERFORMED**, never passes.
- It expires **2027-02-11** or when a qualified independent human reviewer becomes available,
  **whichever occurs first**.
- **It authorises no publication, no tag, no release, no deployment, and no repository settings
  change.**

### W-014 — scope, stated as narrowly as it was granted

**Approved by Ahmed Anbar on 2026-08-24.** The following limits are part of the grant:

- **W-014 waives only the independent phase-level review required to close Phase 006.**
- **It is not a review.** Requirements, security, dependency, and validation reviews were
  commissioned for this phase. They are **automated**, therefore **advisory**, and are recorded as
  such. Any that returned nothing is recorded as **NOT PERFORMED**, never as a pass.
- **It does not close the ADRs.** That is W-013.
- **It waives no defect and no gate.** Every objective gate passed before this waiver was written:
  1284 tests on two toolchains at default parallelism, `cargo xtask verify` steps 1–10, feature
  isolation with controls, licence and advisory policy, secret scan over history and tree, and a
  real-Docker matrix.
- **It confers nothing on Phase 007 and authorises no publication, tag, release, or deployment.**
- It expires **2027-02-11** or when a qualified independent human reviewer becomes available,
  **whichever occurs first**.

#### This is the SIXTH consecutive phase-level waiver, and it deepens the trend rather than resetting it

W-003 (Phase 001), W-005 (Phase 002), W-008 (Phase 003), W-010 (Phase 004), W-012 (Phase 005) and
now **W-014 (Phase 006)** waive the same rule for the same reason in six consecutive phases. The
guard defined below was already **TRIPPED** at three; six is not a new trip, it is the same
untreated condition three phases further on.

**Phase 006 widened the gap in a specific and recorded way.** The maintainer's own work found that a
limitation shipped in this phase — L-6 — rested on a **wrong diagnosis**, and that a documented
bound (`lock_timeout`) was enforced by nothing while the module documentation claimed the opposite.
Both survived the phase's own tests and its own review. Mutation testing then found three test
defects and one **spurious kill** — a test that appeared to prove a guard worked and was measuring
nothing. Every one of those is exactly what an independent reviewer exists to catch, and every one
was caught by the author auditing his own work, which is the arrangement this waiver exists because
of.

**It is not progress.** The condition that keeps six consecutive waivers from being an unaddressed
release blocker is **RO-001**, the dated reviewer-recruitment obligation created with W-008.
RO-001's first review date is **2026-11-19** and has **not** arrived, so nothing about this grant
discharges it. Its failure condition is unchanged and now governs **six** phase-level waivers: if
neither a candidate nor a written account of what was attempted is recorded by that date, **every
open waiver in this category is treated as expired**, and an expired-but-open waiver is a release
blocker by this ledger's own rule.

**Ten waivers across five phases was one problem recorded ten times. Twelve across six is the same
problem recorded twelve times.** The remedy is a second qualified person, not a thirteenth
recording.

### W-015 — scope, stated as narrowly as it was granted

- **W-015 waives only the independent-human-review requirement for accepting ADR-0020, ADR-0021
  and ADR-0022**, the three Phase 007 SeaORM decision records. It waives nothing about their
  content, and nothing about any other record.
- **W-015 does not authorise closing Phase 007.** It is a *record-level* waiver. Phase closure is
  **W-016**, granted separately, on the same terms and for the same reason — the two axes are not
  collapsed, for the reason §W-005 records.
- **It authorises no publication, tag, release or deployment.** None has occurred.
- **It does not waive any defect, failed check, missing acceptance criterion, or security
  blocker.** Every review finding in Phase 007 was dispositioned by fixing it or by relabelling the
  evidence honestly; none was waived to merge.

### W-016 — scope, stated as narrowly as it was granted

- **W-016 waives only the independent phase-level review required to close Phase 007.**
- **It does not close the ADRs.** That is W-015.
- **It authorises no publication, tag, release or deployment**, and does not begin Phase 008.
- **It does not waive a single finding.** Four advisory reviews returned findings — including three
  Critical and several High — and every one was reproduced and fixed or explicitly relabelled. A
  waiver that had absorbed them would be the loophole this ledger's rules exist to prevent.

### W-017 — scope, stated as narrowly as it was granted

- **W-017 waives only the independent-human-review requirement for accepting ADR-0023**, the Phase
  008 database-portability decision. It waives nothing about the record's content, and nothing
  about any other record.
- **W-017 does not close Phase 008.** It is a *record-level* waiver. Its closure needed a separate
  phase-level waiver, and that is **W-018**, granted 2026-08-27 — the two axes are not collapsed,
  for the reason §W-005 records. This entry named a specific future identifier as *"granted
  separately"* until a review pointed out that a ledger naming a waiver it has not issued reads,
  to anyone auditing it, as one that has. It is named here now because it exists.
- **It authorises no publication, tag, release or deployment.** None has occurred.
- **It does not waive any defect, failed check, missing acceptance criterion, or security blocker.**
  **Twenty** findings were raised against the reviewed tree across **three** automated review
  rounds and **all twenty were dispositioned by change**, not by argument — including two that made a
  published safety claim false, one that showed a mutation this phase recorded as *killed* still
  survived, and two that falsified statements in this ledger. None was waived to merge. The
  dispositions are in [`phase-008-review-record.md`](phase-008-review-record.md).
- **The automated reviews are advisory supporting evidence only and are NOT counted as the
  independent review.** **Three** separately commissioned reviewer agents returned **no result at
  all**, twice each, and are recorded as **NOT PERFORMED** — never as a pass. That distinction is this
  ledger's own rule, and it was applied rather than quietly relaxed.

> **W-017 was granted after the reviews, not before.** The evidence it rests on includes what the
> reviews changed, which is why the controls above name defects found rather than gates passed.
>
> This read *"Both were granted"* — wording inherited from the W-015/W-016 section it used to
> follow, and left standing when the W-017 section was inserted above it. One waiver exists here,
> not two.

### W-018 — scope, stated as narrowly as it was granted

- **W-018 waives only the independent-human-review requirement for closing Phase 008.** It waives
  nothing about the phase's content, no defect, no failed check, no missing acceptance criterion,
  and no security blocker.
- **It is separate from W-017 and does not extend it.** W-017 is record-level and covers **ADR-0023
  only**; W-018 is phase-level and covers **Phase 008 only**. Neither authorises the other's scope,
  for the reason §W-005 records — a waiver is amended by re-justification and re-dating, never by
  reinterpretation.
- **Phase 008 now holds exactly two explicit reviewed exceptions, W-017 and W-018.** That is this
  ledger's stated maximum: **no third exception exists for this phase, and none is anticipated.**
  The next identifier is deliberately not named here — this ledger's own guard refuses any
  reference to a waiver its table does not grant, because naming one reads, to anyone auditing it,
  as having issued it. That guard is `the_active_waiver_counts_match_the_waiver_table`, it was
  added after a review found this ledger doing exactly that, and it caught this sentence's first
  draft.
- **It authorises no publication, tag, release or deployment.** None has occurred, and `crates.io`
  returns 404 for every crate in the workspace.
- **The automated reviews are advisory supporting evidence and are NOT the review this waives.**
  Three Codex rounds ran; all three were automated, non-independent, and are labelled so wherever
  they are counted.

> **This is the EIGHTH consecutive phase-level waiver of the same rule for the same reason.**
> Phases 001 through 008 have each closed without an independent requirements-and-security review.
> The trend guard in this ledger was tripped at **three** and has deepened every phase since.
>
> **RO-001, the dated reviewer-recruitment obligation created with W-008, has produced no
> recruitment progress of any kind.** Its first review date is unchanged at **2026-11-19**. Eight
> consecutive waivers with no recruitment activity is the fact this entry exists to keep visible:
> the condition is not being worked on, and each waiver is granted knowing that.

### What the three review rounds did and did not establish

Phase 008 obtained more review than any phase before it, and none of it was independent. Stated
plainly so the volume is not mistaken for the thing it substitutes for:

| Round | Findings | What it was |
|---|---|---|
| Codex #1 | 10 | automated, advisory, non-independent |
| Codex #2 | 1 P1 + 5 P2 | automated, advisory, non-independent |
| Codex #3 | 4 P2 | automated, advisory, non-independent |
| Three commissioned reviewer agents | **none** | **NOT PERFORMED** — each returned twice with no result |

**Twenty findings, all dispositioned by change.** The ones worth naming, because they are what a
reader should weigh when deciding how much the gates were worth:

- a **constitutional violation** — raw driver text in telemetry — that passed a 13/13 CI run and a
  full first review round;
- **two published safety claims that were false**: that PostgreSQL `jsonb` and MySQL `JSON` accept
  the same documents, and that a partially-failed MySQL migration can be recovered by running the
  rest;
- a **mutation this ledger recorded as killed that had in fact survived**;
- a **concurrency test that passed without ever exercising a race**;
- a **census that did not cover the deliverable it was built to protect**.

None of those is the kind of defect a gate catches, and none was caught by twenty-two gates run on
two toolchains. That is the argument for the review this waiver waives, and it is recorded here
rather than in the closure announcement.

### W-019 — scope, stated as narrowly as it was granted

- **W-019 waives only the independent-human-review requirement for accepting ADR-0024, ADR-0025,
  ADR-0026, ADR-0027, ADR-0028, ADR-0029 and ADR-0030**, the seven Phase 009 authentication
  records, reviewed as **one coupled cluster** because each depends on a boundary another draws.
  It waives nothing about their content, and nothing about any other record.
- **W-019 does not authorise closing Phase 009.** It is a *record-level* waiver. Phase closure is
  **W-020**, granted separately, on the same terms and for the same reason — the two axes are not
  collapsed, for the reason §W-005 records.
- **It authorises no publication, tag, release or deployment.** None has occurred, and no Renvor
  crate is published.
- **It does not waive any defect, failed check, missing acceptance criterion, or security
  blocker.** Every finding was dispositioned by change or recorded as a retained limitation with
  an owner and a target phase; none was waived to merge.

### W-020 — scope, stated as narrowly as it was granted

- **W-020 waives only the independent phase-level requirements-and-security review required to
  close Phase 009.**
- **It does not accept the ADRs.** That is W-019, and the two are separate exceptions on separate
  axes.
- **It authorises no publication, tag, release or deployment**, and does not begin Phase 010.
- **It does not close a single retained limitation.** Twenty-three are carried forward, each with
  an owner and a target phase. A waiver that had absorbed them would be the loophole this
  ledger's rules exist to prevent.
- **It does not waive a single finding.** Two commissioned validators returned fourteen findings
  between them, four breaking published security claims; nine were fixed in code and the rest
  recorded with the reason each was not.

#### The head and tree these waivers are bound to

| Field | Value |
|---|---|
| **Branch** | `009-authentication-sessions-tokens-policies` |
| **Verified implementation and evidence head** | `0090c6784acdbfac863fc966e449245201a2b1fd` |
| **Tree** | `dd1b27e32f79a41efaaaa6abc2e4d477262f326d` |
| **Decision records (W-019)** | **ADR-0024, ADR-0025, ADR-0026, ADR-0027, ADR-0028, ADR-0029, ADR-0030** |
| **Gate result at that head** | `cargo xtask verify` **11/11** on **1.94.0** and on **stable**, run twice on two platforms: locally on macOS 26.3 and on ubuntu in CI. 117 test binaries, **1804 passed / 0 failed / 5 ignored** on each leg; census **63/63**; worktree clean; `git ls-files specs` = 0 |

The final pull-request head adds **only** the two waiver records above, the seven ADR status
changes they authorise, and this phase's `governance/phase-009-*.md` records — no implementation
and no evidence change. That is verifiable rather than asserted: `git diff 0090c67..<final head>`
touches `governance/waivers.md`, `GOVERNANCE.md`, the five `governance/phase-009-*.md` files, and
the seven decision records, and nothing else.

The re-review both removal plans require is against **that content**, not against whatever `main`
holds when it is performed.

### W-021 — scope, stated as narrowly as it was granted

- **W-021 waives only the independent-human-review requirement for accepting ADR-0031, ADR-0032,
  ADR-0033, ADR-0034, ADR-0035, ADR-0036 and ADR-0037**, the seven Phase 010 capability records,
  reviewed as **one coupled cluster** because each depends on a boundary another draws. It waives
  nothing about their content, and nothing about any other record.
- **It accepts the decisions the records make, not the reading ADR-0031 withdrew.** ADR-0031's
  claim of compliance with constitution principle VII was withdrawn in place on 2026-09-04 and the
  record says so; accepting the record under this waiver does not reinstate the claim. That
  obligation is W-024's subject, and it is deferred, not met.
- **W-021 does not authorise closing Phase 010.** It is a *record-level* waiver. Phase closure is
  **W-022**, granted separately, on the same terms and for the same reason — the two axes are not
  collapsed, for the reason §W-005 records.
- **It authorises no publication, tag, release or deployment.** None has occurred, and no Renvor
  crate is published.
- **It does not waive any defect, failed check, missing acceptance criterion, or security
  blocker.** Every finding was dispositioned by change or recorded as a retained limitation with
  an owner and a target; none was waived to merge.

### W-022 — scope, stated as narrowly as it was granted

- **W-022 waives only the independent phase-level requirements-and-security review required to
  close Phase 010.**
- **It does not accept the ADRs.** That is W-021, and the two are separate exceptions on separate
  axes.
- **It does not cover constitution principle VII.** The generator obligation Phase 010 left unmet is
  a different rule, waived — for its timing only — by W-024, and Phase 009's by W-023. A phase-level
  review waiver that quietly absorbed a constitutional obligation would be the loophole this
  ledger's rules exist to prevent.
- **It authorises no publication, tag, release or deployment**, and does not begin Phase 011.
- **It does not close a single retained limitation.** Sixteen are carried forward, each with an
  owner and a target (`phase-010-limitations.md`); L-14 is deferred under W-023 and W-024 and stays
  visible as unmet; L-15 and L-17 stay open with their Phase 011 ownership.
- **It does not waive a single finding.** The maintainer's Codex review returned sixteen; fifteen
  were corrected at the root and the sixteenth is L-14. The round's own retained L-16 was then ruled
  a correctness blocker and corrected the same day rather than carried.

### W-023 and W-024 — scope, stated as narrowly as they were granted

- **They waive only the *timing* of principle VII's mandatory-choice rule** for the auth starter
  (W-023, Phase 009) and for the five capabilities (W-024, Phase 010): the choice does not become
  mandatory in `renvor new` on the day the capability shipped, but by the absolute expiry or when
  Phase 011 proves the support, whichever is first.
- **They do not waive the rest of principle VII.** The prohibition on soliciting or recording an
  unsupported choice is complied with, not waived — the reserved `--auth` flag fails explicitly, no
  capabilities flag exists, and the governed-choice test pins both — and that compliance is one of
  the counted controls, not a precondition.
- **They do not reinterpret "ships".** The maintainer ruled on 2026-09-04 that the constitutional
  meaning stands. A library-only phase that ships a capability incurs the obligation on that day.
- **W-023 is not retroactive.** It begins 2026-09-04. It does not make Phase 009's closure on
  2026-08-31 compliant, and this ledger does not claim that it does; Phase 009 closed with the
  obligation unmet and unnoticed, and that is recorded rather than repaired.
- **They do not close L-14.** L-14 is *deferred* under them and remains a retained limitation with
  the status "unmet", not "fixed" or "closed", until Phase 011's proof is recorded.
- **They authorise no publication, tag, release or deployment.** An unmet constitutional obligation
  is a release blocker while either waiver is active, and each says so in its own controls.

**Closed on 2026-09-05, before expiry — the first closures in this ledger.** Phase 011 implemented
the wizard and non-interactive parity for both choices, the validated `renvor.toml` persistence,
the dependency and project wiring, and the generated-project compile and start tests, and proved
them on all four persistence rows and against every capability's real server, censused with every
other four-row suite (86 rows), with the three negative controls fired — all against head
`5eff451c435c8676aaa3cd231ccfc7d2e5ec5ba0`, tree `d1cab4cb7b1a1a18e387689e6ad3fdd0f6a628f9` (`phase-011-evidence.md` §2 and §4,
`phase-011-review-record.md` §2). L-14 is closed with that measurement. The rows above stay in the
table marked `closed` and are not counted as active; nothing about W-023's non-retroactivity
changes, and the release block they carried ends with them — the review-gap waivers' own blocks do
not.

#### The head and tree these waivers are bound to

| Field | Value |
|---|---|
| **Branch** | `feat/phase-010-operational-capabilities` |
| **Verified implementation and evidence head** | `5f26334b394f20ae86b3037ccb77a23705c40ed9` |
| **Tree** | `47aeb8d8fda9e07bd5a4520406cef4eada44273c` |
| **Reviewed source head** | `8b2758034a9f5b5d85559430c0ea0c2254e30278` — the L-16 correction's source commit; the implementation is unchanged between it and the head above, which adds one test-table extension in `xtask` (the waiver-count test could not spell twenty-three, and now counts the review-gap exceptions separately) and the governance text of 2026-09-04 |
| **Decision records (W-021)** | **ADR-0031, ADR-0032, ADR-0033, ADR-0034, ADR-0035, ADR-0036, ADR-0037** |
| **Gate result at that head** | `cargo xtask verify` **9/9** on **1.94.0** and on **stable**, sequentially, `CARGO_INCREMENTAL=0`, against live PostgreSQL 17, MySQL 8.4, Valkey 9.1.1 and Mailpit 1.29.1; **2065 passed / 0 failed / 5 ignored** on each leg; census **67/67**; worktree clean; `git ls-files specs` = 0 (`phase-010-evidence.md` §3a″) |

The final pull-request head adds **only** the four waiver records above, the seven ADR status
changes W-021 authorises, this phase's `governance/phase-010-*.md` records, a dated cross-reference
note in `governance/phase-009-evidence.md`, `GOVERNANCE.md`, the one README sentence that states the
phase's state, and one stale version cross-reference in `contracts/capabilities-contract.md` that
the validation pass found — no implementation, test, or manifest change. That is verifiable rather
than asserted: `git diff 5f26334..<final head> --name-only` lists `governance/waivers.md`,
`GOVERNANCE.md`, `README.md`, `contracts/capabilities-contract.md`, the `governance/phase-009-evidence.md`
and `governance/phase-010-*.md` files, and the seven decision records, and nothing else. No commit
can name its own hash, so the binding is to the head above and the final head is identified by the
pull request's merge record.

The re-review both removal plans require is against **that content**, not against whatever `main`
holds when it is performed.

#### This is the TENTH consecutive phase-level waiver, and the trend guard is already tripped

W-003 (Phase 001), W-005 (Phase 002), W-008 (Phase 003), W-010 (Phase 004), W-012 (Phase 005),
W-014 (Phase 006), W-016 (Phase 007), W-018 (Phase 008), W-020 (Phase 009) and now **W-022 (Phase
010)** waive the same rule for the same reason in ten consecutive phases. The guard was already
**TRIPPED** at three; a tenth does not trip it again, it deepens it.

**What that means concretely, stated rather than softened:** the condition keeping this from being
an unaddressed release blocker is **RO-001**, the dated reviewer-recruitment obligation created with
W-008. RO-001's first review date is **2026-11-19** and has **not** yet arrived, so nothing about
this grant discharges it and nothing here should be read as progress against it. Its failure
condition is unchanged and now governs **ten** phase-level waivers: if neither a candidate nor a
written account of what was attempted is recorded by that date, **every open waiver in this category
is treated as expired**, and an expired-but-open waiver is a release blocker by this ledger's own
rule.

**Eighteen review-gap waivers across nine phases was one problem recorded eighteen times. Twenty
across ten is the same problem recorded twenty times.** The remedy is a second qualified person, not
a twenty-first recording.

### The third Phase 009 exception and the third Phase 010 exception — acknowledged departures

This ledger sets an expected maximum of **at most 2 explicit reviewed exceptions per phase**, and
requires that a third *"must be justified against this line explicitly, as an acknowledged
departure."* On 2026-09-04 two phases came to hold three: **W-023 is the third in Phase 009**
(after W-019 and W-020) and **W-024 is the third in Phase 010** (after W-021 and W-022). This
section is both justifications. Neither is absorbed by extending another waiver: W-020 and W-022
each state in their own scope that they do not cover principle VII, and one waiver covers one
rule, at one level, in one phase, so a single "generator obligation" waiver spanning two phases
was refused too.

**Why a third was needed, twice.** Constitution principle VII makes each governed choice mandatory
in `renvor new` *"on the day its capability ships"*. Phase 009 shipped authentication and Phase 010
shipped five capabilities, both as libraries, and both read "ships" as "generated projects gain the
wiring" — which no phase before Phase 011 can deliver — and made no generator change. The
maintainer's Phase 010 review rejected that reading (finding 5, recorded as L-14 with ADR-0031's
compliance claim withdrawn in place), and the maintainer ruled on 2026-09-04 that the meaning of
"ships" is not reinterpreted or weakened. The obligation is therefore real, unmet for two phases,
and not meetable by the narrowest literal change — soliciting and recording a choice the generator
cannot honour is exactly what the same principle forbids. What remained was a waiver of the
*timing*, per phase, with an absolute expiry and controls specific to the gap.

**What this departure does and does not mean:**

- **The cause is different, and this ledger says so rather than letting the count imply otherwise.**
  Every other explicit reviewed exception exists because the project has one person. W-023 and
  W-024 exist because a rule was read too narrowly for one phase and the reading was rejected in the
  next. Nobody was missing. They are counted in the per-phase guard because they are explicit
  reviewed exceptions, and they are **not** counted in the trend guard, the RO-001 obligation, or
  the "same underlying reason" sentence, because none of those describes them. The waiver-count test
  now derives the review-gap set from each row's violated rule so that sentence cannot absorb them.
- **W-023 is not retroactive.** It begins on the day it was granted. Phase 009 closed on 2026-08-31
  with this obligation unmet and unnoticed; W-020 never covered it; this waiver does not make that
  closure compliant after the fact, and no record is rewritten to suggest that it does.
- **The per-phase guard is now breached three times** — Phase 002 (W-006), Phase 009 (W-023), and
  Phase 010 (W-024) — each visibly, with a reason.
- **The expiry does not ratchet.** The ratchet rule binds waivers of the single-maintainer gap.
  W-023 and W-024 expire **2026-10-04**, or earlier when Phase 011 proves the support — more than
  four months before every review-gap expiry — so the horizon has not moved, and Phase 011 is
  bound to a date rather than to an intention.
- **L-14 stays visible as unmet.** It is deferred, not fixed and not closed, and it will close only
  with Phase 011's measurement recorded against a named head and tree.
- **Release is blocked while either is active**, in each waiver's own controls. Merging publishes
  nothing; a tag, a release, a deployment, or a crate publication would.

## Waiver categories and expected counts

Three categories are tracked separately. They are **not** interchangeable, and a waiver in
one category does not consume the allowance of another.

| Category | Expected count | Waivers |
|---|---|---|
| Repository **approval** waivers | exactly **1** | **W-001** — single-maintainer approval gap *(seeded 2026-08-11 at T015)*. **Unchanged by W-003, W-004, W-005, and W-006** |
| **Control-unavailability** waivers | **0** | none expected — research Finding 3 confirmed every required repository control is free on the public tier, so cost or plan tier is never an accepted reason |
| **Explicit reviewed exceptions** | **at most 2 per phase** — **breached three times: Phase 002 holds 3** *(see [§The third Phase 002 exception](#the-third-phase-002-exception--an-acknowledged-departure))*, and **on 2026-09-04 Phase 009 and Phase 010 came to hold 3 each** *(see [§The third Phase 009 and Phase 010 exceptions](#the-third-phase-009-exception-and-the-third-phase-010-exception--acknowledged-departures))*. Phase 001 holds 2, Phase 003 holds 1, **Phase 004, Phase 005, Phase 006 and Phase 007 each hold 2 — at the limit, not over it**, and **Phase 008 holds W-017 and W-018 — at the limit, not over it**. **Twenty of the twenty-two are review-gap waivers; two (W-023, W-024) waive a different rule for a different reason** | **W-002** — ADR independent-review gap (Phase 001); **W-003** — Phase 001 independent requirements-and-security-review gap *(granted 2026-08-15)*; **W-004** — ADR-0007 independent-review gap *(granted 2026-08-16)*; **W-005** — Phase 002 independent requirements-and-security-review gap *(granted 2026-08-16)*; **W-006** — ADR-0009 independent-review gap *(granted 2026-08-17)*; **W-008** — Phase 003 independent requirements-and-security-review gap *(granted 2026-08-19)*; **W-009** — ADR-0012 independent-review gap *(granted 2026-08-23)*; **W-010** — Phase 004 independent requirements-and-security-review gap *(granted 2026-08-23)*; **W-011** — ADR-0013/0014/0015 independent-review gap *(granted 2026-08-23)*; **W-012** — Phase 005 independent requirements-and-security-review gap *(granted 2026-08-23)*; **W-013** — ADR-0016/0017/0018/0019 independent-review gap *(granted 2026-08-24)*; **W-014** — Phase 006 independent requirements-and-security-review gap *(granted 2026-08-24)*; **W-015** — ADR-0020/0021/0022 independent-review gap *(granted 2026-08-24)*; **W-016** — Phase 007 independent requirements-and-security-review gap *(granted 2026-08-24)*; **W-017** — ADR-0023 independent-review gap *(granted 2026-08-26)*; **W-018** — Phase 008 independent requirements-and-security-review gap *(granted 2026-08-27)*; **W-019** — ADR-0024…ADR-0030 independent-review gap *(granted 2026-08-31)*; **W-020** — Phase 009 independent requirements-and-security-review gap *(granted 2026-08-31)* *(W-019 and W-020 were omitted from this cell when granted and added on 2026-09-04, found while adding the rows below; the omission is stated rather than backdated)*; **W-021** — ADR-0031…ADR-0037 independent-review gap *(granted 2026-09-04)*; **W-022** — Phase 010 independent requirements-and-security-review gap *(granted 2026-09-04)*; **W-023** — Phase 009 auth-starter generator obligation, constitution principle VII *(granted 2026-09-04; not a review-gap waiver; **closed 2026-09-05**)*; **W-024** — Phase 010 capabilities generator obligation, constitution principle VII *(granted 2026-09-04; not a review-gap waiver; **closed 2026-09-05**)* |

**W-002 through W-006 and W-008 through W-024 are explicit reviewed
exceptions, not part of the normal expected waiver count.** Each was granted by a recorded maintainer decision — W-002 on
2026-08-11, W-003 on 2026-08-15, W-004 and W-005 on 2026-08-16, W-006 on 2026-08-17, W-008 on
2026-08-19, W-009 and W-010 on 2026-08-23, W-011 and W-012 on 2026-08-23, W-013, W-014, W-015 and W-016 on 2026-08-24, W-017 on 2026-08-26, W-018 on 2026-08-27, W-019 and W-020 on 2026-08-31, and W-021, W-022, W-023 and W-024 on 2026-09-04 — rather than arising from a design shortfall, and none indicates that anything in the
design failed to work.
Twenty of the twenty-two are review-gap waivers. **All twenty exist for the same underlying reason: the project has one person.** **The other two, W-023 and W-024, exist for a different reason**: they waive the timing of constitution principle VII's generator obligation for the authentication and capability crates that two library-only phases shipped without a generator change, and their cause is a rule read too narrowly and then read correctly — nobody was missing. *(Both were closed on 2026-09-05, when Phase 011 proved the support — the first closures in this ledger; the twenty review-gap waivers remain.)* They are the first exceptions in this ledger with a cause other than the single-maintainer gap, and since 2026-09-04 `the_active_waiver_counts_match_the_waiver_table` derives the review-gap set from each row's violated rule, so the sentence before this one cannot silently absorb them. (That sentence
read “All eight” while listing ten names, corrected on 2026-08-24; it then read “All fourteen”
while listing fifteen, found by review and corrected on 2026-08-26. Twice by hand is twice too
many, so it is now asserted by `the_active_waiver_counts_match_the_waiver_table` alongside the
headline it always guarded. **On 2026-08-31 that assertion earned its place**: Phase 009 updated
the headline, both tables and `GOVERNANCE.md` and still left this sentence reading “All sixteen”
while the set held eighteen. The gate refused the change; no human noticed. A third hand
correction is exactly what it was written to prevent, and this is the first occurrence it caught
rather than recorded after the fact.) They are separate
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
| **W-011** | decision records — **ADR-0013, ADR-0014, ADR-0015 only** | Phase 005 |
| **W-012** | phase-level review | Phase 005 |
| **W-013** | decision records — **ADR-0016, ADR-0017, ADR-0018, ADR-0019 only** | Phase 006 |
| **W-014** | phase-level review | Phase 006 |
| **W-015** | decision records — **ADR-0020, ADR-0021, ADR-0022 only** | Phase 007 |
| **W-016** | phase-level review | Phase 007 |
| **W-017** | decision records — **ADR-0023 only** | Phase 008 |
| **W-018** | phase-level review (`PLAN.md` §6.1 step 10) | Phase 008 |
| **W-019** | decision records — **ADR-0024 … ADR-0030 only** | Phase 009 |
| **W-020** | phase-level review (`PLAN.md` §6.1 step 10) | Phase 009 |
| **W-021** | decision records — **ADR-0031 … ADR-0037 only** | Phase 010 |
| **W-022** | phase-level review (`PLAN.md` §6.1 step 10) | Phase 010 |
| **W-023** | constitution principle VII — the auth-starter choice's timing only; **not a review waiver** | Phase 009 — **closed 2026-09-05** |
| **W-024** | constitution principle VII — the capabilities choice's timing only; **not a review waiver** | Phase 010 — **closed 2026-09-05** |

**The two axes are deliberately not collapsed, and a third rule is a third waiver.** A record-level waiver does not authorise closing
a phase, a phase-level waiver does not authorise accepting a decision record, and neither reaches a constitutional obligation on the generator — which is why Phase 009 and Phase 010 each carry a separate principle VII waiver (W-023, W-024) rather than a stretched W-020 or W-022 — which is
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
  again on 2026-08-19 (W-008) and on 2026-08-23 (W-009, W-010, W-011 and W-012).** Ten waivers now share one
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
