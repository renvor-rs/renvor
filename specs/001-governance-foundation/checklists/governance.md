# Governance Review Checklist: Phase 001 Foundation

**Purpose**: Formal independent-reviewer gate on the *quality of the Phase 001 requirements* — completeness, clarity, consistency, measurability, and coverage — before implementation begins
**Created**: 2026-08-11
**Feature**: [spec.md](../spec.md) · [plan.md](../plan.md) · [research.md](../research.md) · [data-model.md](../data-model.md) · [contracts/](../contracts/)
**Audience**: Independent reviewer performing the requirements and security review required by PLAN.md §6.1 step 10
**Depth**: Formal phase gate

> This checklist tests whether the **requirements are written correctly**, not whether anything works. Items ask "is this specified, quantified, consistent, measurable?" — never "does it do X?". Implementation evidence is validated separately by [quickstart.md](../quickstart.md).
>
> Complements [requirements.md](./requirements.md), which covers generic spec quality. This one covers the nine governance domains.

## Governance Completeness

- [ ] CHK001 Are all six required governance documents individually enumerated, so a reviewer can detect a missing one rather than judge the set as a whole? [Completeness, Spec §FR-007]
- [ ] CHK002 Is "discoverable within one link from the root landing document" stated precisely enough that two reviewers would reach the same verdict? [Measurability, Spec §FR-007]
- [ ] CHK003 Is "monitored contact" defined, or could an unattended inbox satisfy the requirement as written? [Ambiguity, Spec §FR-011]
- [ ] CHK004 Is the vulnerability acknowledgement window quantified with a specific duration rather than left to the security policy's discretion? [Clarity, Spec §FR-011]
- [ ] CHK005 Are the decision-record states enumerated exhaustively and mutually exclusively, with no undefined intermediate state? [Completeness, Spec §FR-013]
- [ ] CHK006 Are all seven waiver fields stated as mandatory, with no field permitted to be omitted? [Completeness, Spec §FR-015]
- [ ] CHK007 Is the distinction between an *active* and an *expired* waiver defined, and is the consequence of expiry stated? [Clarity, Data model §Waiver Record]
- [ ] CHK008 Is the precedence order between the constitution, PLAN.md, and governance documents stated where they could conflict? [Consistency, Spec §FR-012]
- [ ] CHK009 Does the spec state who holds authority to accept a decision record, rather than leaving authority implied by the governance document? [Gap, Spec §FR-013]
- [ ] CHK010 Are requirements defined for what happens when a waiver's expiry condition is met but no one acts on it? [Coverage, Gap, Spec §FR-051]

## Naming Evidence

- [ ] CHK011 Are all ten name items enumerated in the requirement itself, so completeness is checkable without consulting another document? [Completeness, Spec §FR-001]
- [ ] CHK012 Is every evidence field stated as mandatory, including checker attribution, so an unattributed row is detectably invalid? [Completeness, Spec §FR-002]
- [ ] CHK013 Is the `ambiguous` status defined with enough precision that two reviewers would classify the same observation identically? [Ambiguity, Spec §FR-002]
- [ ] CHK014 Is the prohibition on automatic substitution stated strongly enough that no reading permits a "reasonable alternative"? [Clarity, Spec §FR-003]
- [ ] CHK015 Is the 30-day validity window stated in the requirements themselves, or does it exist only in the data model where a spec reader would miss it? [Consistency, Spec §FR-006 vs Data model §Name Availability Record]
- [ ] CHK016 Is the distinction between a *verified* name and a *claimed* name stated per item, so the weaker guarantee on registry names is not read as ownership? [Clarity, Contract §public-identity]
- [ ] CHK017 Is the residual risk of verified-but-unreserved names required to carry a named owner and a closing phase, rather than being noted narratively? [Completeness, Spec §FR-049]
- [ ] CHK018 Are re-verification triggers other than elapsed time specified, or is time the only defined trigger? [Coverage, Gap, Spec §FR-006]
- [ ] CHK019 Is the product-versus-executable naming distinction required to be *justified* in the decision record, not merely restated? [Clarity, Spec §FR-005]

## Licensing

- [ ] CHK020 Are the exact licence identifiers stated as literal values rather than described as "permissive dual licensing"? [Clarity, Spec §FR-008]
- [ ] CHK021 Is the licensing status of generated project output stated as a binding requirement rather than only as an assumption? [Completeness, Spec §FR-050]
- [ ] CHK022 Are contribution licensing terms specified, so a contributor knows what they grant before opening a pull request? [Completeness, Spec §FR-008]
- [ ] CHK023 Is brand-asset licensing either specified or explicitly declared out of scope, given that the code grant does not cover it? [Gap, Plan §Pre-Push Stage 0]
- [ ] CHK024 Are the allowed, review-required, and denied licence sets each enumerated, rather than described by principle? [Completeness, Spec §FR-010]
- [ ] CHK025 Is "requires written review" defined with a reviewer, an outcome set, and a record location? [Ambiguity, Spec §FR-010]
- [ ] CHK026 Is the treatment of a dependency carrying no licence expression stated explicitly, so absence is not read as permission? [Edge Case, Spec §FR-010]
- [ ] CHK027 Is the authority order between the machine-readable policy and its prose restatement declared, so divergence has a defined resolution? [Consistency, Data model §Dependency and Licence Policy]

## MSRV and Toolchain

- [ ] CHK028 Is the minimum supported version stated as an exact literal version rather than as a formula over current stable? [Clarity, Spec §FR-017, §FR-018]
- [ ] CHK029 Is "single authoritative location" identified concretely, so a reviewer can detect a second competing declaration? [Ambiguity, Spec §FR-017]
- [ ] CHK030 Are the conditions permitting a raise enumerated exhaustively, with no residual discretion? [Completeness, Spec §FR-058]
- [ ] CHK031 Is the start event of the six-month dwell period defined, so its expiry is objectively determinable? [Measurability, Spec §FR-059, Data model §floor_declared_on]
- [ ] CHK032 Is the quarterly review's *non-effect* on the declared version stated as strongly as its obligation to occur? [Clarity, Spec §FR-060]
- [ ] CHK033 Is the Phase 006 revalidation given a named owner and an unambiguous trigger, rather than a general intention? [Completeness, Spec §FR-061]
- [ ] CHK034 Are the pinned minimum and the floating stable channel distinguished, so "tested toolchains" cannot be read as two fixed versions? [Clarity, Spec §FR-019]
- [ ] CHK035 Is the requirement that resolution behaviour be *demonstrated in effect* stated separately from the requirement that it be *configured*? [Measurability, Spec §FR-057, §SC-016]
- [ ] CHK036 Is the rationale for an explicit resolver declaration recorded, so a future contributor does not "simplify" it away as redundant? [Traceability, Research §Finding 1]

## Repository Security

- [ ] CHK037 Are the required branch-protection settings enumerated with concrete values rather than described as "protected"? [Completeness, Spec §FR-027]
- [ ] CHK038 Is the prohibition on bypass permission stated as absolute, with no waiver path available? [Clarity, Spec §FR-027, Data model §Repository Protection Baseline]
- [ ] CHK039 Is the required-approval count tied to an objectively countable condition, so the rule cannot drift with interpretation? [Measurability, Spec §FR-051]
- [ ] CHK040 Are the individual scanning controls named, rather than referenced collectively as "platform security features"? [Completeness, Spec §FR-030]
- [ ] CHK041 Is the exclusion of cost and plan tier as acceptable justifications stated as a requirement, not only as research rationale? [Consistency, Spec §FR-030 vs Research §Finding 3]
- [ ] CHK042 Are least-privilege permission requirements expressed at both workflow and job granularity, so a blanket top-level grant is detectably non-compliant? [Completeness, Spec §FR-028]
- [ ] CHK043 Is "immutable reference" defined concretely enough to exclude a moving tag? [Clarity, Spec §FR-029]
- [ ] CHK044 Are pre-push cleanup requirements stated with their ordering rationale, so the sequence is not reordered as if arbitrary? [Completeness, Plan §Pre-Push Repository Cleanup]
- [ ] CHK045 Is the remediation sequence following a non-zero secret-scan finding stated as an ordered obligation rather than a set of options? [Clarity, Data model §Repository Cleanup and Scan Record]
- [ ] CHK046 Are requirements defined for material that is neither included nor excluded by an explicit decision — i.e. is silence given a defined outcome? [Edge Case, Plan §Pre-Push Stage 0]

## Supply-Chain Controls

- [ ] CHK047 Are the bill-of-materials format and the provenance mechanism each specified, rather than named only by intent? [Completeness, Spec §FR-045]
- [ ] CHK048 Is the evidence retention period stated as a concrete duration? [Clarity, Gap, Spec §FR-046]
- [ ] CHK049 Are lockfile obligations stated per artifact kind, so a reader can classify a new artifact without guessing? [Completeness, Spec §FR-021]
- [ ] CHK050 Is a response window defined for security advisories, or is triage left unbounded? [Gap, Spec §FR-010]
- [ ] CHK051 Are the permitted outcomes for an unmaintained dependency enumerated, so a reviewer is not left to improvise? [Completeness, Spec §FR-010]
- [ ] CHK052 Are the documentation toolchain's dependencies explicitly brought under the same policy, rather than implicitly excluded as non-Rust? [Coverage, Spec §FR-054]

## Documentation Ownership

- [ ] CHK053 Is a named owner required for the documentation platform decision, rather than an owning team? [Completeness, Spec §FR-036]
- [ ] CHK054 Are the evaluation criteria enumerated, so the decision record is auditable rather than merely asserted? [Completeness, Spec §FR-035]
- [ ] CHK055 Is the documentation versioning cadence specified, or does the requirement stop at "versioned"? [Gap, Research §Open items]
- [ ] CHK056 Can "describe the same contract at the same version" be objectively evaluated, or does it rest on reviewer judgement? [Measurability, Spec §FR-056]
- [ ] CHK057 Are the link-check scope and its failure threshold stated, so "no broken links" has a defined boundary? [Clarity, Spec §FR-037]
- [ ] CHK058 Is ownership of documentation *content* distinguished from ownership of the documentation *platform*? [Gap, Spec §FR-036]

## Release Bootstrap

- [ ] CHK059 Are all required package metadata fields enumerated, so a missing field is detectable without external reference? [Completeness, Spec §FR-040]
- [ ] CHK060 Are the bootstrap credential's scope, creation timing, storage prohibition, and revocation timing each specified? [Completeness, Spec §FR-034]
- [ ] CHK061 Is "revoked immediately after verification" given an objective completion signal and a recorded artifact? [Measurability, Spec §FR-034]
- [ ] CHK062 Is the ordering constraint — that trusted publishing cannot be configured before a package exists — stated as a requirement, or does it live only in research? [Consistency, Research §Finding 2 vs Spec §FR-033]
- [ ] CHK063 Is the zero-publication criterion written so it can be *positively evidenced*, rather than satisfied by asserting nothing was run? [Measurability, Spec §FR-038, §SC-010]
- [ ] CHK064 Is the publication order rule stated with its wait condition, so "topological order" is actionable? [Clarity, Spec §FR-041]
- [ ] CHK065 Is yank-and-replace stated as the sole remedy for a defective release, excluding any overwrite reading? [Clarity, Spec §FR-041]
- [ ] CHK066 Are protected-environment approvers required to be named individuals rather than a role? [Completeness, Spec §FR-032]

## Measurable Acceptance Criteria

- [ ] CHK067 Does every success criterion express a threshold, count, or duration, with none relying on an unquantified adjective? [Measurability, Spec §SC-001–SC-016]
- [ ] CHK068 Is every PLAN.md Phase 001 acceptance criterion traceable to at least one functional requirement and one success criterion? [Traceability, Spec §FR-042]
- [ ] CHK069 Is "evidence" defined with its required fields, so an incomplete record is detectably invalid? [Clarity, Spec §FR-042]
- [ ] CHK070 Are known limitations required to carry both a named owner and a target phase? [Completeness, Spec §FR-043]
- [ ] CHK071 Is the consequence of an unevidenced criterion row stated, so it is not silently treated as met? [Clarity, Spec §SC-011]
- [ ] CHK072 Is the exclusion boundary in the scope requirement precise enough to adjudicate a borderline artifact such as build tooling? [Clarity, Spec §FR-047]
- [ ] CHK073 Is the requirement and criterion identifier scheme stable, given that identifiers were appended out of numeric order during clarification? [Traceability, Checklist §requirements.md Note 4]

## Ambiguities and Conflicts Requiring Reviewer Judgement

- [ ] CHK074 Do the requirements resolve who provides the "independent review" that a decision record needs, when the spec simultaneously acknowledges a single maintainer? [Conflict, Spec §FR-013 vs §FR-051]
- [ ] CHK075 Is the waiver permitted by the scanning requirement reconcilable with the success criterion asserting the approval waiver is the only one? [Conflict, Spec §FR-030 vs §SC-008]
- [ ] CHK076 Do the requirements state unambiguously that public repository visibility is not itself confirmation of any name, given the repository exists publicly before the first content push? [Ambiguity, Spec §FR-004, §FR-052]
- [ ] CHK077 Is the assumption that a single named individual holds maintainer, security-contact, release-approver, and registry-owner roles simultaneously recorded as a risk with a mitigation? [Assumption, Spec §Assumptions]
- [ ] CHK078 Are the three still-derived positions — supported operating systems, release ownership, registry bootstrap ownership — marked clearly enough that a reviewer will not mistake them for confirmed decisions? [Assumption, Spec §Assumptions]
- [ ] CHK079 Is the publication decision for PLAN.md and the legacy planning documents recorded as a required decision rather than resolved by default? [Gap, Plan §Pre-Push Stage 0]

## Notes

- Items are requirement-quality questions. A failing item means the *specification* needs work, not that an implementation is broken.
- Traceability coverage: 79 of 79 items carry a spec reference or an explicit `[Gap]` / `[Conflict]` / `[Assumption]` / `[Ambiguity]` marker.
- **CHK074 and CHK075 are known live tensions** surfaced during planning, not hypotheticals. Expect them to require an explicit reviewer ruling rather than a documentation fix.
- Check items off as `[x]`; record findings inline beneath the item.
