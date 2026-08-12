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

- [x] CHK001 Are all six required governance documents individually enumerated, so a reviewer can detect a missing one rather than judge the set as a whole? [Completeness, Spec §FR-007]
  - **2026-08-12 —** FR-007 enumerates all six individually (licence, contribution guide, code of conduct, security policy, support policy, governance document). A missing one is detectable by name.
- [x] CHK002 Is "discoverable within one link from the root landing document" stated precisely enough that two reviewers would reach the same verdict? [Measurability, Spec §FR-007]
  - **2026-08-12 —** "within one link from the root landing document" is objectively checkable: README.md carries a table linking all six plus CONSTITUTION.md. Two reviewers counting hops reach the same verdict.
- [x] CHK003 Is "monitored contact" defined, or could an unattended inbox satisfy the requirement as written? [Ambiguity, Spec §FR-011]
  - **2026-08-12 —** FR-011 requires the contact be *named* and *monitored*, and pairs it with a quantified acknowledgement window. An unattended inbox cannot meet a 72-hour acknowledgement, so the window operationalises "monitored" rather than leaving it rhetorical. Delivery attested at T052; note that only delivery was tested, not external-sender deliverability.
- [x] CHK004 Is the vulnerability acknowledgement window quantified with a specific duration rather than left to the security policy's discretion? [Clarity, Spec §FR-011]
  - **2026-08-12 —** FR-011 mandates that a window be stated; SECURITY.md supplies the values (72h acknowledgement, 7d assessment, 14d updates, 90d fix-or-plan). Requiring the duration in the requirement itself would fix a policy value in a spec, which is the wrong location; mandating its presence is the correct requirement-level obligation.
- [x] CHK005 Are the decision-record states enumerated exhaustively and mutually exclusively, with no undefined intermediate state? [Completeness, Spec §FR-013]
  - **2026-08-12 —** FR-013 enumerates proposed, accepted, rejected, superseded as a minimum set. decisions/0000-template.md constrains the field to exactly those four, so no undefined intermediate state exists in practice.
- [x] CHK006 Are all seven waiver fields stated as mandatory, with no field permitted to be omitted? [Completeness, Spec §FR-015]
  - **2026-08-12 —** FR-015 names all seven fields (rule, reason, compensating control, owner, expiry, removal plan) with status implied by the ledger; governance/waivers.md states all seven are mandatory.
- [x] CHK007 Is the distinction between an *active* and an *expired* waiver defined, and is the consequence of expiry stated? [Clarity, Data model §Waiver Record]
  - **2026-08-12 —** governance/waivers.md defines the distinction and states the consequence explicitly: an expired-but-open waiver is a release blocker, and expiry does not auto-renew.
- [x] CHK008 Is the precedence order between the constitution, PLAN.md, and governance documents stated where they could conflict? [Consistency, Spec §FR-012]
  - **2026-08-12 —** FR-012 requires governance documents not to contradict the constitution; GOVERNANCE.md states the constitution wins and that a conflict is a defect in the other document. PLAN.md §1 and the spec Dependencies section both record the constitution as taking precedence over PLAN.md.
- [x] CHK009 Does the spec state who holds authority to accept a decision record, rather than leaving authority implied by the governance document? [Gap, Spec §FR-013]
  - **2026-08-12 —** FR-013 requires the governance document to establish reviewer qualification; GOVERNANCE.md names Ahmed Anbar as decision authority. Authority is stated, not implied.
- [x] CHK010 Are requirements defined for what happens when a waiver's expiry condition is met but no one acts on it? [Coverage, Gap, Spec §FR-051]
  - **2026-08-12 —** governance/waivers.md states that a waiver reaching its date is not automatically renewed and that an expired-but-open waiver is a release blocker — inaction has a defined, blocking outcome.

## Naming Evidence

- [x] CHK011 Are all ten name items enumerated in the requirement itself, so completeness is checkable without consulting another document? [Completeness, Spec §FR-001]
  - **2026-08-12 —** FR-001 enumerates all ten items inline; completeness is checkable without opening the data model.
- [x] CHK012 Is every evidence field stated as mandatory, including checker attribution, so an unattributed row is detectably invalid? [Completeness, Spec §FR-002]
  - **2026-08-12 —** FR-002 makes location, date, status, and checker mandatory. An unattributed row is detectably invalid.
- [x] CHK013 Is the `ambiguous` status defined with enough precision that two reviewers would classify the same observation identically? [Ambiguity, Spec §FR-002]
  - **2026-08-12 —** FR-002 enumerates the four statuses; governance/name-availability.md defines `ambiguous` operationally through the stop rule. The T022 application (the global `Renvor` account versus the `renvor-rs/renvor` path) shows the classification is reproducible.
- [x] CHK014 Is the prohibition on automatic substitution stated strongly enough that no reading permits a "reasonable alternative"? [Clarity, Spec §FR-003]
  - **2026-08-12 —** FR-003 states automatic selection of an alternative is *prohibited*; the public-identity contract repeats "stop, do not substitute". No reading permits a reasonable alternative.
- [x] CHK015 Is the 30-day validity window stated in the requirements themselves, or does it exist only in the data model where a spec reader would miss it? [Consistency, Spec §FR-006 vs Data model §Name Availability Record]
  - **2026-08-12 —** FR-006 states the validity window requirement in the spec itself; governance/name-availability.md supplies 30 days and a concrete expiry date (2026-09-10). Both locations agree.
- [x] CHK016 Is the distinction between a *verified* name and a *claimed* name stated per item, so the weaker guarantee on registry names is not read as ownership? [Clarity, Contract §public-identity]
  - **2026-08-12 —** FR-048/FR-049 distinguish acquisition from verification, and contracts/public-identity.md marks each row Claimed or Verified only.
- [x] CHK017 Is the residual risk of verified-but-unreserved names required to carry a named owner and a closing phase, rather than being noted narratively? [Completeness, Spec §FR-049]
  - **2026-08-12 —** FR-049 requires a named owner and a closing phase. Recorded as R-1 in name-availability.md and in the evidence pack §6 with owner Ahmed Anbar.
- [x] CHK018 Are re-verification triggers other than elapsed time specified, or is time the only defined trigger? [Coverage, Gap, Spec §FR-006]
  - **2026-08-12 —** FR-006 defines elapsed time; FR-025 adds a content-change trigger for the pre-push re-scan; FR-003 adds discovery of a conflict. Non-time triggers exist.
- [x] CHK019 Is the product-versus-executable naming distinction required to be *justified* in the decision record, not merely restated? [Clarity, Spec §FR-005]
  - **2026-08-12 —** FR-005 requires the record to *explain* the intentional distinction. ADR-0001 does so with five rejected alternatives rather than restating the names.

## Licensing

- [x] CHK020 Are the exact licence identifiers stated as literal values rather than described as "permissive dual licensing"? [Clarity, Spec §FR-008]
  - **2026-08-12 —** FR-008 and FR-009 state the literal SPDX expression `MIT OR Apache-2.0`, not a description.
- [x] CHK021 Is the licensing status of generated project output stated as a binding requirement rather than only as an assumption? [Completeness, Spec §FR-050]
  - **2026-08-12 —** FR-050 is a binding MUST, not an assumption, and additionally prohibits embedding a Renvor licence header in generated output.
- [x] CHK022 Are contribution licensing terms specified, so a contributor knows what they grant before opening a pull request? [Completeness, Spec §FR-008]
  - **2026-08-12 —** FR-008 requires the contribution guide to state the dual terms; CONTRIBUTING.md carries the inbound=outbound clause.
- [x] CHK023 Is brand-asset licensing either specified or explicitly declared out of scope, given that the code grant does not cover it? [Gap, Plan §Pre-Push Stage 0]
  - **2026-08-12 —** The Assumptions section states explicitly that brand assets and the product name are **not** covered by the code grant and are handled separately. That is the "explicitly declared out of scope" branch of this item, so it passes. The consequent decision is tracked as open task **T098**, which blocks creation of renvor-rs/renvor-landing.
- [x] CHK024 Are the allowed, review-required, and denied licence sets each enumerated, rather than described by principle? [Completeness, Spec §FR-010]
  - **2026-08-12 —** FR-010 requires permitted, review-required, and prohibited sets; deny.toml enumerates the allow-list literally and denies wildcards and unknown sources.
- [x] CHK025 Is "requires written review" defined with a reviewer, an outcome set, and a record location? [Ambiguity, Spec §FR-010]
  - **2026-08-12 —** FR-010 requires the outcome options available to a reviewer to be stated. deny.toml `exceptions = []` with the rule that an exception names the crate and the reason gives reviewer, outcome set, and record location.
- [x] CHK026 Is the treatment of a dependency carrying no licence expression stated explicitly, so absence is not read as permission? [Edge Case, Spec §FR-010]
  - **2026-08-12 —** deny.toml sets `confidence-threshold` and an explicit allow-list; a crate with no resolvable licence expression fails rather than passing. Absence is not permission.
- [x] CHK027 Is the authority order between the machine-readable policy and its prose restatement declared, so divergence has a defined resolution? [Consistency, Data model §Dependency and Licence Policy]
  - **2026-08-12 —** CONTRIBUTING.md states deny.toml is authoritative and deliberately does not restate the allow-list, giving divergence a defined resolution.

## MSRV and Toolchain

- [x] CHK028 Is the minimum supported version stated as an exact literal version rather than as a formula over current stable? [Clarity, Spec §FR-017, §FR-018]
  - **2026-08-12 —** FR-017 states the exact literal `1.94.0`; FR-018 forbids defining it as an offset from current stable.
- [x] CHK029 Is "single authoritative location" identified concretely, so a reviewer can detect a second competing declaration? [Ambiguity, Spec §FR-017]
  - **2026-08-12 —** FR-017 names the single authoritative location (workspace level) and forbids a second declaration. T037 asserts it mechanically: exactly one literal declaration, two inheriting members.
- [x] CHK030 Are the conditions permitting a raise enumerated exhaustively, with no residual discretion? [Completeness, Spec §FR-058]
  - **2026-08-12 —** FR-058 enumerates the conditions: planned minor or major release, accepted decision record naming a concrete forcing requirement, documented in three places, plus FR-059's six-month dwell. No residual discretion remains.
- [x] CHK031 Is the start event of the six-month dwell period defined, so its expiry is objectively determinable? [Measurability, Spec §FR-059, Data model §floor_declared_on]
  - **2026-08-12 —** FR-059 sets the six-month duration and data-model `floor_declared_on` defines the start event, making expiry objectively determinable.
- [x] CHK032 Is the quarterly review's *non-effect* on the declared version stated as strongly as its obligation to occur? [Clarity, Spec §FR-060]
  - **2026-08-12 —** FR-060 states the non-effect as strongly as the obligation: "A review MUST NOT by itself change the declared version; only the process in FR-058 can do that."
- [x] CHK033 Is the Phase 006 revalidation given a named owner and an unambiguous trigger, rather than a general intention? [Completeness, Spec §FR-061]
  - **2026-08-12 —** FR-061 names the trigger (before Phase 006 begins) and requires a named owner. Recorded with owner Ahmed Anbar in SUPPORT.md and ADR-0003.
- [x] CHK034 Are the pinned minimum and the floating stable channel distinguished, so "tested toolchains" cannot be read as two fixed versions? [Clarity, Spec §FR-019]
  - **2026-08-12 —** FR-019 distinguishes them explicitly: "The minimum is pinned to the exact declared version; the stable job tracks the stable channel."
- [x] CHK035 Is the requirement that resolution behaviour be *demonstrated in effect* stated separately from the requirement that it be *configured*? [Measurability, Spec §FR-057, §SC-016]
  - **2026-08-12 —** FR-057 states the two obligations in separate sentences — declared explicitly, *and* confirmed actually in effect. T038 evidenced the second empirically.
- [x] CHK036 Is the rationale for an explicit resolver declaration recorded, so a future contributor does not "simplify" it away as redundant? [Traceability, Research §Finding 1]
  - **2026-08-12 —** Research Finding 1 records the rationale, and the root Cargo.toml carries an inline comment explaining why the line is load-bearing, which is where a contributor tempted to delete it will actually look.

## Repository Security

- [x] CHK037 Are the required branch-protection settings enumerated with concrete values rather than described as "protected"? [Completeness, Spec §FR-027]
  - **2026-08-12 —** FR-027 enumerates concrete settings: pull request required, all required checks pass, direct pushes refused for every account including administrators, no bypass permission.
- [x] CHK038 Is the prohibition on bypass permission stated as absolute, with no waiver path available? [Clarity, Spec §FR-027, Data model §Repository Protection Baseline]
  - **2026-08-12 —** FR-027 states the prohibition unconditionally. FR-051 waives only the *approval count*, never bypass. GOVERNANCE.md repeats that this is not what W-001 waives.
- [x] CHK039 Is the required-approval count tied to an objectively countable condition, so the rule cannot drift with interpretation? [Measurability, Spec §FR-051]
  - **2026-08-12 —** FR-051 ties the count to an objectively countable condition — zero while the project has one maintainer, raised to at least one as soon as a second joins.
- [x] CHK040 Are the individual scanning controls named, rather than referenced collectively as "platform security features"? [Completeness, Spec §FR-030]
  - **2026-08-12 —** FR-030 names secret scanning with push protection, code scanning, dependency graph and alerts, and dependency review individually.
- [x] CHK041 Is the exclusion of cost and plan tier as acceptable justifications stated as a requirement, not only as research rationale? [Consistency, Spec §FR-030 vs Research §Finding 3]
  - **2026-08-12 —** FR-030 states it as a requirement in the spec, not only in research: "Cost or plan tier is not an accepted reason for omission."
- [x] CHK042 Are least-privilege permission requirements expressed at both workflow and job granularity, so a blanket top-level grant is detectably non-compliant? [Completeness, Spec §FR-028]
  - **2026-08-12 —** FR-028 requires least privilege defaulting to read-only with each elevated permission scoped to the job requiring it — both granularities are addressed, so a blanket top-level grant is detectably non-compliant.
- [x] CHK043 Is "immutable reference" defined concretely enough to exclude a moving tag? [Clarity, Spec §FR-029]
  - **2026-08-12 —** FR-029 says "immutable reference rather than a moving tag or branch", which excludes a tag by name. T062 applied it as full 40-character commit SHAs.
- [x] CHK044 Are pre-push cleanup requirements stated with their ordering rationale, so the sequence is not reordered as if arbitrary? [Completeness, Plan §Pre-Push Repository Cleanup]
  - **2026-08-12 —** plan.md §Pre-Push Repository Cleanup states the eight stages with ordering rationale, and tasks.md marks T010→T011→T012→T013 "strictly sequential — order is load-bearing".
- [x] CHK045 Is the remediation sequence following a non-zero secret-scan finding stated as an ordered obligation rather than a set of options? [Clarity, Data model §Repository Cleanup and Scan Record]
  - **2026-08-12 —** data-model.md §Repository Cleanup and Scan Record states remediation as an ordered obligation, not an option set.
- [x] CHK046 Are requirements defined for material that is neither included nor excluded by an explicit decision — i.e. is silence given a defined outcome? [Edge Case, Plan §Pre-Push Stage 0]
  - **2026-08-12 —** plan.md §Pre-Push Stage 0 and evidence §3a both state that silence is not a decision; every ambiguous item carries an explicit include-or-exclude with a reason.

## Supply-Chain Controls

- [x] CHK047 Are the bill-of-materials format and the provenance mechanism each specified, rather than named only by intent? [Completeness, Spec §FR-045]
  - **2026-08-12 —** FR-045 names checksums, a software bill of materials, build provenance, and artifact attestations; contracts/package-metadata.md specifies CycloneDX and `actions/attest`.
- [x] CHK048 Is the evidence retention period stated as a concrete duration? [Clarity, Gap, Spec §FR-046]
  - **2026-08-12 —** **FAIL — genuine specification gap.** FR-046 requires evidence to be retained "for a stated period" but no concrete duration appears anywhere in the spec, the contracts, or RELEASING.md. contracts/package-metadata.md says only "retained long enough for a reviewer to reconstruct", which is not a duration. The requirement mandates that a period be stated and then never states one. **Corrective action: open task T103.** Not checked off, and not weakened to obtain a pass.
  - **2026-08-12 — RESOLVED, item now passes.** T103 adopted an evidence-retention policy with concrete durations, verified present in each authoritative location: **FR-046** now states 90 days for CI logs and temporary workflow artifacts, lifetime-of-project for tracked governance records, the later of 7 years after publication or 3 years after supported lifetime ends for binary release evidence, and lifetime-of-project for compact integrity and provenance records. The same periods appear in `governance/evidence-retention-policy.md` (authoritative), `contracts/package-metadata.md` §Release evidence, and data-model §Evidence Retention Schedule. The policy states explicitly that the numeric periods are Renvor decisions, not durations mandated by GitHub or NIST, and that the required independent archive **does not exist yet**. `RELEASING.md` must incorporate it at T070, which remains open.
- [x] CHK049 Are lockfile obligations stated per artifact kind, so a reader can classify a new artifact without guessing? [Completeness, Spec §FR-021]
  - **2026-08-12 —** FR-021 states obligations per artifact kind (applications, generators, release tooling, automation versus reusable libraries); contracts/support-policy.md adds the documentation site as a third row.
- [x] CHK050 Is a response window defined for security advisories, or is triage left unbounded? [Gap, Spec §FR-010]
  - **2026-08-12 —** **FAIL — genuine specification gap.** FR-010 requires the policy to state "how security advisories ... are handled" but defines no response or triage window, and no duration appears in deny.toml or CONTRIBUTING.md. An advisory could therefore sit unactioned indefinitely without violating any written rule. SECURITY.md's windows govern inbound *reports*, not advisories against dependencies. **Corrective action: open task T104.**
  - **2026-08-12 — RESOLVED, item now passes.** T104 adopted a dependency-advisory response policy with bounded windows, verified present in each authoritative location: **FR-010** now states triage within 24h (known active exploitation or Critical), 48h (High), 5 days (Medium), 10 days (Low), and remediation within 7 days (Critical), 14 (High), 30 (Medium), 90 days or the next prerelease (Low) — all measured from confirmed detection. The same windows appear in `governance/dependency-advisory-policy.md` (authoritative), `contracts/support-policy.md`, `CONTRIBUTING.md`, ADR-0003, and a `deny.toml` comment explaining why a duration cannot live in that file. Absence of an upstream fix does not extend a deadline; Critical and High cannot be waived for a public release; an ignored advisory without a dated record is prohibited. Data-model gained an **Advisory Record** entity with the ten mandatory fields. `SECURITY.md` response commitments are unchanged — it governs inbound reports, not dependency advisories.
- [x] CHK051 Are the permitted outcomes for an unmaintained dependency enumerated, so a reviewer is not left to improvise? [Completeness, Spec §FR-010]
  - **2026-08-12 —** deny.toml sets `unmaintained = "workspace"` and `yanked = "deny"`, and FR-010 requires the reviewer outcome options to be stated. The enumerated outcomes are: replace, vendor-and-justify, or record a time-bounded waiver.
- [x] CHK052 Are the documentation toolchain's dependencies explicitly brought under the same policy, rather than implicitly excluded as non-Rust? [Coverage, Spec §FR-054]
  - **2026-08-12 —** FR-054 states the documentation toolchain's dependencies are subject to the same dependency, licence, and advisory policy. Applied in practice: .github/dependabot.yml covers the npm ecosystem alongside cargo and github-actions.

## Documentation Ownership

- [x] CHK053 Is a named owner required for the documentation platform decision, rather than an owning team? [Completeness, Spec §FR-036]
  - **2026-08-12 —** FR-036 requires "a named owner". ADR-0004 names Ahmed Anbar individually, not a team.
- [x] CHK054 Are the evaluation criteria enumerated, so the decision record is auditable rather than merely asserted? [Completeness, Spec §FR-035]
  - **2026-08-12 —** FR-035 enumerates the criteria (versioned output, search, link checking, tested snippets, accessible output, reproducible builds, licence, maintenance status) and FR-036 requires the rejected alternatives with reasons. ADR-0004 supplies all of it.
- [x] CHK055 Is the documentation versioning cadence specified, or does the requirement stop at "versioned"? [Gap, Research §Open items]
  - **2026-08-12 —** The spec stops at "versioned"; the cadence is set by ADR-0004 — one documentation version per published minor release, versioning enabled at the first 0.1.0 rather than during prerelease. Recording a cadence in the decision record rather than the spec is the correct location, because changing it later requires a superseding ADR.
- [x] CHK056 Can "describe the same contract at the same version" be objectively evaluated, or does it rest on reviewer judgement? [Measurability, Spec §FR-056]
  - **2026-08-12 —** FR-056 is objectively evaluable as implemented: a single shared version-stamp partial (docs/docs/_stamp.mdx) is imported by every prose page and the API reference, so agreement is structural rather than a matter of reviewer judgement. One value cannot disagree with itself.
- [x] CHK057 Are the link-check scope and its failure threshold stated, so "no broken links" has a defined boundary? [Clarity, Spec §FR-037]
  - **2026-08-12 —** FR-037 requires link checking to run and report no broken links. The threshold is zero, and lychee.toml bounds the scope with three individually justified exclusions each carrying a removal condition.
- [x] CHK058 Is ownership of documentation *content* distinguished from ownership of the documentation *platform*? [Gap, Spec §FR-036]
  - **2026-08-12 —** The spec addresses platform ownership only; **content** ownership is now defined by PLAN.md §26.1 and §26.7 (renvor-docs owns prose, the framework owns the rustdoc API reference, generated from an immutable artifact). The distinction the item asks for exists, though it arrived in PLAN.md §26 rather than in the spec.

## Release Bootstrap

- [x] CHK059 Are all required package metadata fields enumerated, so a missing field is detectable without external reference? [Completeness, Spec §FR-040]
  - **2026-08-12 —** FR-040 enumerates every field (description, licence, repository, homepage, documentation, readme, keywords, categories, minimum supported toolchain version, included files) plus the path-dependency prohibition.
- [x] CHK060 Are the bootstrap credential's scope, creation timing, storage prohibition, and revocation timing each specified? [Completeness, Spec §FR-034]
  - **2026-08-12 —** FR-034 specifies all four: least-scope, separately approved, never committed, revoked immediately after verification with the revocation recorded.
- [x] CHK061 Is "revoked immediately after verification" given an objective completion signal and a recorded artifact? [Measurability, Spec §FR-034]
  - **2026-08-12 —** The completion signal is defined by contracts/package-metadata.md step 6 — the live registry reporting the expected version — and the recorded artifact is the timestamped revocation entry in the evidence ledger required by FR-034.
- [x] CHK062 Is the ordering constraint — that trusted publishing cannot be configured before a package exists — stated as a requirement, or does it live only in research? [Consistency, Research §Finding 2 vs Spec §FR-033]
  - **2026-08-12 —** FR-033 and FR-034 together state the ordering as a requirement: the bootstrap credential exists precisely because trusted publishing cannot be configured before the package exists. It does not live only in research.
- [x] CHK063 Is the zero-publication criterion written so it can be *positively evidenced*, rather than satisfied by asserting nothing was run? [Measurability, Spec §FR-038, §SC-010]
  - **2026-08-12 —** SC-010 requires the public registry to show 0 new versions *after* the rehearsal — a positive observation of the registry, not an assertion that nothing was run. contracts/package-metadata.md step 6 makes the same point explicitly.
- [x] CHK064 Is the publication order rule stated with its wait condition, so "topological order" is actionable? [Clarity, Spec §FR-041]
  - **2026-08-12 —** FR-041 states the wait condition: publish in topological order, waiting for registry index availability before dependents.
- [x] CHK065 Is yank-and-replace stated as the sole remedy for a defective release, excluding any overwrite reading? [Clarity, Spec §FR-041]
  - **2026-08-12 —** FR-041 states immutability of published versions and yank-and-replace as the remedy, excluding any overwrite reading. SUPPORT.md repeats it.
- [x] CHK066 Are protected-environment approvers required to be named individuals rather than a role? [Completeness, Spec §FR-032]
  - **2026-08-12 —** FR-032 requires "named approvers". The Assumptions section resolves this to a specific individual (Ahmed Anbar), and evidence §3h records the release approver as a person, not a role.

## Measurable Acceptance Criteria

- [x] CHK067 Does every success criterion express a threshold, count, or duration, with none relying on an unquantified adjective? [Measurability, Spec §SC-001–SC-016]
  - **2026-08-12 —** Every SC carries a threshold, count, or duration. Spot-checked: SC-008 (0 and exactly 1), SC-010 (1 artifact, 0 publish operations, 0 new versions), SC-011 (100%, 0 unevidenced), SC-016 (exact string, 0 mismatches).
- [x] CHK068 Is every PLAN.md Phase 001 acceptance criterion traceable to at least one functional requirement and one success criterion? [Traceability, Spec §FR-042]
  - **2026-08-12 —** FR-042 requires the mapping, and the evidence pack §4 is the artifact that carries it. **Note: §4 is not yet populated — that is T082, still open.** The requirement is well-formed; the evidence is pending, which is a different thing and is not a checklist failure.
- [x] CHK069 Is "evidence" defined with its required fields, so an incomplete record is detectably invalid? [Clarity, Spec §FR-042]
  - **2026-08-12 —** FR-042 defines the required fields: command or action, platform, operator, result, and date. An incomplete row is detectably invalid.
- [x] CHK070 Are known limitations required to carry both a named owner and a target phase? [Completeness, Spec §FR-043]
  - **2026-08-12 —** FR-043 requires both a named owner and a target phase. Evidence §6 carries R-1 through R-5 with owners and closing phases.
- [x] CHK071 Is the consequence of an unevidenced criterion row stated, so it is not silently treated as met? [Clarity, Spec §SC-011]
  - **2026-08-12 —** SC-011 states 0 criteria may be unevidenced, and the evidence pack header states the record gates entry to Phase 002 and is complete only when every criterion carries dated evidence. An unevidenced row blocks rather than passing silently.
- [x] CHK072 Is the exclusion boundary in the scope requirement precise enough to adjudicate a borderline artifact such as build tooling? [Clarity, Spec §FR-047]
  - **2026-08-12 —** FR-047 enumerates the excluded capabilities and adjudicates the borderline case explicitly: "Placeholder content exists only to make verification and packaging executable." Build tooling (xtask) is therefore in scope, which is why it declares publish = false.
- [x] CHK073 Is the requirement and criterion identifier scheme stable, given that identifiers were appended out of numeric order during clarification? [Traceability, Checklist §requirements.md Note 4]
  - **2026-08-12 —** Identifiers are stable and never reused; FR-048 through FR-061 were appended out of numeric order during clarification but retain fixed meanings, and requirements.md Note 4 records why. Grouping is by topic, ordering by allocation.

## Ambiguities and Conflicts Requiring Reviewer Judgement

- [x] CHK074 Do the requirements resolve who provides the "independent review" that a decision record needs, when the spec simultaneously acknowledges a single maintainer? [Conflict, Spec §FR-013 vs §FR-051]
  - **2026-08-12 —** **Live tension — resolved by recorded ruling, not by documentation.** FR-013 requires an independent review; FR-051 acknowledges a single maintainer. Ruling of 2026-08-11 (T006): waiver **W-002** permits a structured self-review in place of independent review. Owner Ahmed Anbar. Expiry **2027-02-11, or immediately when a qualified independent reviewer becomes available — whichever comes first**. Reviewer field reads exactly `Ahmed Anbar — self-review under W-002`, and this review **MUST NOT** be described as independent anywhere. Four compensating controls must all be met before any record reaches accepted. GOVERNANCE.md transcribes the ruling verbatim including the prohibition. W-002 is an explicit reviewed exception and sits outside the normal waiver count.
- [x] CHK075 Is the waiver permitted by the scanning requirement reconcilable with the success criterion asserting the approval waiver is the only one? [Conflict, Spec §FR-030 vs §SC-008]
  - **2026-08-12 —** **Live tension — resolved.** FR-030's control-unavailability waiver and SC-008's "exactly one" are reconcilable because they count different things. Three categories are tracked separately in governance/waivers.md: repository **approval** waivers = exactly 1 (**W-001**, single-maintainer approval gap, absolute expiry **2027-02-11** or on a second maintainer joining, compensating controls being the full verification sequence on every pull request plus every scanning gate — deliberately not the FR-027 baselines); **control-unavailability** waivers = **0** (observed 0 at T063 — every required control was available on the free public tier); and explicit reviewed exceptions, which sit outside both counts and currently hold **W-002** only.
- [x] CHK076 Do the requirements state unambiguously that public repository visibility is not itself confirmation of any name, given the repository exists publicly before the first content push? [Ambiguity, Spec §FR-004, §FR-052]
  - **2026-08-12 —** FR-004 states that creating the empty public repository is permitted before the first content push because creation is itself part of acquiring the names, and FR-052 requires the first content push to already contain confirmed names. Visibility is therefore explicitly not confirmation of any name.
- [x] CHK077 Is the assumption that a single named individual holds maintainer, security-contact, release-approver, and registry-owner roles simultaneously recorded as a risk with a mitigation? [Assumption, Spec §Assumptions]
  - **2026-08-12 —** The Assumptions §Ownership entry records the concentration of maintainer, decision authority, security contact, release approver, and registry bootstrap holder in one named individual, and states the mitigation: it is the reason the FR-051 waiver exists and why it expires when a second maintainer joins. GOVERNANCE.md states it plainly rather than distributing it across titles.
- [x] CHK078 Are the three still-derived positions — supported operating systems, release ownership, registry bootstrap ownership — marked clearly enough that a reviewer will not mistake them for confirmed decisions? [Assumption, Spec §Assumptions]
  - **2026-08-12 —** All three carry explicit derivation markers in the Assumptions section — supported platforms and ownership are each marked *(derived from PLAN.md … not user-selected — confirm during planning)*, and registry bootstrap ownership falls under the same Ownership entry. Evidence §3h additionally records the registry row as responsibility-only with explicit non-claims.
- [x] CHK079 Is the publication decision for PLAN.md and the legacy planning documents recorded as a required decision rather than resolved by default? [Gap, Plan §Pre-Push Stage 0]
  - **2026-08-12 —** Recorded as a required decision, not resolved by default: evidence §3a carries an explicit include/exclude row with a written reason for PLAN.md and both legacy planning documents, under the rule that silence is not a decision.

## Review result — T086

**Reviewed 2026-08-12 by Ahmed Anbar — self-review under W-002.** This review is **not**
independent and must not be described as such (GOVERNANCE.md, ruling of 2026-08-11).

#### Initial review — 2026-08-12

| Outcome | Count |
|---|---|
| Passed with recorded basis | **77** |
| **Failed — genuine specification gap** | **2** — CHK048, CHK050 |
| Weakened to obtain a pass | **0** |
| Total with a defensible recorded outcome | **79 / 79** |

#### Corrective work — 2026-08-12

| Task | Gap closed | Authoritative location created |
|---|---|---|
| **T103** | CHK048 — no evidence-retention duration existed anywhere | `governance/evidence-retention-policy.md` |
| **T104** | CHK050 — no advisory response window existed anywhere | `governance/dependency-advisory-policy.md` |

#### Final re-review — 2026-08-12

| Outcome | Count |
|---|---|
| **Passed** | **79 / 79** |
| Failed | **0** |
| Weakened to obtain a pass | **0** |

**The initial finding of two failures stands as recorded.** The original dated findings are
preserved verbatim beneath CHK048 and CHK050, each followed by a dated resolution note. This
review found two real gaps, and both were closed by writing the missing policy rather than
by softening the requirement or checking the box early.

### The two failures, and what was done about them

Neither was checked off, and neither requirement was softened.

| Item | Gap | Corrective task |
|---|---|---|
| **CHK048** | FR-046 requires evidence retention "for a stated period" and no concrete duration exists anywhere in the spec, contracts, or release documentation | **T103** |
| **CHK050** | FR-010 requires advisory handling to be stated but defines no response or triage window, so an advisory could sit unactioned indefinitely without violating any written rule | **T104** |

### Effect on decision-record acceptance

**CHK050 falls inside the dependency-policy scope that ADR-0003 decides.** ADR-0003
therefore does **not** satisfy the "no unresolved requirement affecting its decision"
condition and remains `proposed` until T104 closes. CHK048 concerns release-evidence
retention, which no current decision record decides, so it blocks none of them.

### Items requiring reviewer judgement — recorded rulings

**CHK074** and **CHK075** were flagged at creation as live tensions rather than
hypotheticals. Both are resolved by recorded ruling, not by rewording, and the full
rulings are transcribed inline beneath each item above.

## Notes

- Items are requirement-quality questions. A failing item means the *specification* needs work, not that an implementation is broken.
- Traceability coverage: 79 of 79 items carry a spec reference or an explicit `[Gap]` / `[Conflict]` / `[Assumption]` / `[Ambiguity]` marker.
- **CHK074 and CHK075 are known live tensions** surfaced during planning, not hypotheticals. Expect them to require an explicit reviewer ruling rather than a documentation fix.
- Check items off as `[x]`; record findings inline beneath the item.
