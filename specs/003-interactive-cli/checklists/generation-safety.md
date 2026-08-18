# Generation Safety Requirements Checklist: Interactive CLI, templates, and local runtime

**Purpose**: Validate the **quality of the requirements** governing transactional generation, the
path-containment boundary, and bounds — before implementation begins. These are unit tests for the
English, not for the code.
**Created**: 2026-08-17
**Feature**: [spec.md](../spec.md) · **Depth**: formal release gate · **Audience**: reviewer

## Transactional Generation — Falsifiability

- [x] CHK001 Is "no partial destination" stated as a condition that could be **observed to be false**, rather than as a property asserted about the design? [Measurability, Contract §C-5]
- [x] CHK002 Are the failure points at which the destination must remain unchanged **enumerated** (validation, staging, render, manifest, verify, place), or is failure described only in general terms? [Completeness, Contract §C-5]
- [x] CHK003 Do the requirements distinguish "the destination is unchanged" from "the destination does not exist"? A destination that pre-existed and one that did not are different post-conditions. [Clarity, Spec §FR-012]
- [x] CHK004 Is the pre-existing-destination case required to be compared **byte for byte** before and after, or only inspected? [Measurability, Spec §SC-002]
- [x] CHK005 Do the requirements state that cancellation must be tested at **every** prompt rather than at a representative one? [Coverage, Spec §SC-001]
- [ ] CHK006 Is there a stated requirement for a **positive control** on the cancellation and injected-failure suites, so a harness that refuses everything cannot pass? [Measurability, Gap-check] **— GAP.** SC-009 requires a positive control for the hostile-destination corpus, and nothing requires one for the **cancellation** or **injected-failure** suites. `tasks.md` T017 supplies it as a task, so the code has one; the *requirement* does not demand it, which is exactly what this item asks. Recorded rather than closed by editing the spec to match what was built.
- [x] CHK007 Are the requirements for the verification step (formats, compiles, tests, starts) written so that failure is a **generation failure**, not a user discovery? [Clarity, Spec §FR-030]

## Transactional Generation — The Cases That Get Skipped

- [x] CHK008 Is the residue case (process killed between staging and placement) **specified with required properties** — identifiable, located beside the destination, never inside it — or only acknowledged as possible? [Completeness, Contract §C-5]
- [x] CHK009 Do the requirements state what must happen to discovered residue, including whether anything may delete it automatically? [Clarity, Contract §C-5]
- [x] CHK010 Is the concurrency requirement stated as a **guarantee about outcomes** ("at most one succeeds, the other fails cleanly") rather than as an absence of corruption? [Measurability, Spec §FR-015]
- [x] CHK011 Is the time-of-check-to-time-of-use race described with its **residual risk stated**, rather than implied to be closed — including after D6 revision 2 narrowed it? [Clarity, Data-model §I-17]
- [x] CHK012 Do the requirements say what the TOCTOU race is **converted into** (a clean failure) rather than only that it is mitigated? [Clarity, Contract §C-5]
- [x] CHK013 Is the cross-filesystem case addressed by making it **unreachable** rather than by handling it, and is that reasoning recorded where a later reader would find it? [Consistency, Contract §C-5 / Research §D5]
- [ ] CHK014 Are the disk-full and destination-becomes-non-empty-mid-run edge cases carried into a requirement, or do they appear only in the spec's Edge Cases list? [Coverage, Spec §Edge Cases] **— PARTIAL.** "The destination becomes non-empty between validation and the final rename" is carried into FR-013 and FR-015, and since FR-013 was rewritten on 2026-08-18 it covers the destination becoming **anything** between validation and the rename, not only non-empty. **"Disk fills during rendering" is not carried into any requirement** — it appears only in the Edge Cases list. In practice a full disk fails the render and the transaction cleans up, but no requirement says so.

## Path Containment — Strength of the Claim

- [x] CHK015 Now that a **capability** boundary is adopted (D6 revision 2), do the requirements state plainly which rules the capability decides and which remain **checked** name validation, rather than describing the whole design as uniformly structural? [Clarity, Data-model §I-16]
- [ ] CHK016 Is the D6 decision-record gate written as **blocking a merge**, using language that cannot be read as advisory? [Clarity, Research §D6] **— SUPERSEDED.** `research.md` D6 **revision 2** withdrew the decision-record gate entirely by adopting `cap-std`; there is no gate left to word as blocking. T009–T012 are withdrawn, not waived.
- [ ] CHK017 Does the gate name what the record must contain — the package evaluated, its concrete shortcomings, the ownership cost, and an exit strategy? [Completeness, Research §D6] **— SUPERSEDED with CHK016.** There is no record for the gate to require, because there is no gate.
- [x] CHK018 Is every path-rejection rule paired with the **specific attack or mistake it rejects**, so a reader can tell whether the list is complete? [Completeness, Data-model §5]
- [x] CHK019 Are platform-reserved device names enumerated explicitly rather than referred to as a class? [Clarity, Data-model §5]
- [x] CHK020 Do the requirements state that rejection must precede **any** filesystem creation, and is "any" defined to include the staging directory? [Ambiguity, Spec §FR-039]
- [x] CHK021 Is the requirement for a positive control in the hostile-path corpus stated, so that refusing all inputs cannot satisfy it? [Measurability, Spec §SC-009]

## Bounds — Values, Not Adjectives

- [x] CHK022 Does every bound named in the requirements have a **stated value**, or is any left as "bounded" without a number? [Clarity, Spec §FR-042]
- [x] CHK023 Are the four template bounds (recursion depth, total output bytes, output file count, single-file bytes) each required to have a demonstrating test? [Measurability, Contract §C-4]
- [x] CHK024 Is the required behaviour on exceeding a bound specified — which error code, which details, and what state the destination is left in? [Completeness, Contract §C-4]
- [x] CHK025 Do the requirements state that a bound's value is itself documented, so a later change is visible rather than silent? [Traceability, Spec §SC-013]
- [x] CHK026 Is "undefined variable is an error" stated as a requirement on the **rendering environment**, distinguishable from a convention the templates happen to follow? [Clarity, Spec §FR-028]
- [x] CHK027 Are template capabilities required to be **absent** rather than **disabled**, and is that distinction made explicit? [Clarity, Contract §C-4]

## Platform Claims

- [x] CHK028 Is the atomicity guarantee stated **per platform** rather than claimed uniformly? [Clarity, Contract §C-5]
- [x] CHK029 Do the requirements state which precondition makes the weaker Windows guarantee sufficient, rather than asserting equivalence? [Clarity, Contract §C-5]
- [x] CHK030 Is there a requirement that a platform not exercised in CI must not be claimed as supported? [Consistency, Spec §SC-014]

## Notes

- Check items off as completed: `[x]`
- Every item asks whether something is **written correctly**, not whether the code works. Items about
  code belong in [quickstart.md](../quickstart.md), which holds the executable gates.
- **CHK006, CHK021** exist because this project has repeatedly found its own gates to be fail-open —
  a suite of refusal assertions is satisfied by refusing everything, and only a positive control
  distinguishes a working boundary from a broken one.
- **CHK015, CHK016** are the highest-risk items here. If the requirements let a checked boundary read
  as equivalent to a capability boundary, the D6 record becomes a formality rather than a decision.
  **Revision 2 note (2026-08-18):** the capability boundary was adopted, so this risk inverts — the
  new failure mode is describing the design as *uniformly* structural when name validation is still
  checked and one ambient call still exists.
