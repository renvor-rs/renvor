# Specification Quality Checklist: Governance, Names, Toolchain, and Repository Security Foundation

**Purpose**: Validate specification completeness and quality before proceeding to planning
**Created**: 2026-08-11
**Feature**: [spec.md](../spec.md)
**Validation iterations**: 3 (initial 2026-08-11; re-validated after the clarification session; re-validated after the MSRV maintainer decision)

## Content Quality

- [x] No implementation details (languages, frameworks, APIs) — see Note 1
- [x] Focused on user value and business needs
- [x] Written for non-technical stakeholders
- [x] All mandatory sections completed

## Requirement Completeness

- [x] No [NEEDS CLARIFICATION] markers remain
- [x] Requirements are testable and unambiguous
- [x] Success criteria are measurable
- [x] Success criteria are technology-agnostic (no implementation details)
- [x] All acceptance scenarios are defined
- [x] Edge cases are identified
- [x] Scope is clearly bounded
- [x] Dependencies and assumptions identified

## Feature Readiness

- [x] All functional requirements have clear acceptance criteria
- [x] User scenarios cover primary flows
- [x] Feature meets measurable outcomes defined in Success Criteria
- [x] No implementation details leak into specification

## PLAN.md Phase 001 Acceptance Coverage

Every acceptance criterion from PLAN.md §20 (Phase 001) maps to at least one requirement and one measurable outcome.

- [x] Clean checkout passes formatting/lint/test/doc placeholders → FR-022, FR-023, FR-055 → SC-002, SC-003
- [x] Secrets and build output are ignored → FR-024, FR-025 → SC-004, SC-005
- [x] Workflow permissions are minimal → FR-028, FR-029 → SC-007
- [x] All public names are confirmed → FR-001–FR-006, FR-048, FR-049, FR-052 → SC-001
- [x] No ADR is falsely marked accepted → FR-013, FR-014 → SC-009
- [x] Release dry-run packages a placeholder internal crate without publishing → FR-038–FR-041 → SC-010
- [x] Phase 001 deliverables (governance, naming ADR, workspace, MSRV policy, license, repository policies, secure ignore rules, toolchain/dependency policy, documentation stack ADR, CI skeleton, security and release documents) → FR-007–FR-021, FR-027–FR-037, FR-045, FR-046, FR-050, FR-051, FR-053, FR-054, FR-056–FR-061 → SC-006, SC-008, SC-011, SC-012, SC-014, SC-015, SC-016
- [x] Runtime framework features excluded → FR-047 → SC-013

## Notes

**Note 1 — Deliberate technology references.** The specification names the Rust 2024 edition, Cargo resolver 3, and an explicit minimum supported toolchain version (FR-016, FR-017, FR-019). These are not implementation choices invented by this specification: they are binding constraints from the ratified constitution ("Architecture and Technology Constraints") and PLAN.md §8, and PLAN.md §20 states them as Phase 001 acceptance criteria. Removing them would make the specification fail the phase it describes. Elsewhere the specification deliberately uses capability language ("source-hosting platform", "package registry", "formatting, linting") instead of naming specific vendors or tools, so that tool selection remains a planning decision.

**Note 1b — Technologies named during clarification.** The re-validation deliberately keeps item 1 checked even though the spec now names Docusaurus (FR-035), `MIT OR Apache-2.0` (FR-008), and a candidate Rust version. The checklist item exists to stop a specification from pre-empting choices that belong to planning. Here the opposite is true: PLAN.md §18 states the documentation stack "is selected and recorded in Phase 001 rather than assumed accepted," §4 fixes the naming contract, and §20 assigns license and MSRV to this phase's clarification step. These are the phase's deliverables, not leakage. A reviewer who disagrees should raise it against PLAN.md §18/§20 rather than against the spec.

**Note 2 — Clarifications resolved.** PLAN.md §20 assigned nine topics to `/speckit-clarify`. Five were resolved by direct decision in the session of 2026-08-11 (naming posture, license, branch protection, repository visibility and mandatory security controls, documentation platform) and are recorded in the spec's Clarifications section. Four exceeded the five-question limit and are recorded in Assumptions as **derived** positions, each explicitly labelled as derived rather than user-selected and flagged for confirmation during `/speckit-plan`: MSRV policy, supported operating systems, release ownership, and registry bootstrap ownership. Zero `[NEEDS CLARIFICATION]` markers exist in the spec.

**Note 4 — Requirement ID allocation.** Requirements added during clarification (FR-048 – FR-056) were given new numbers and placed in their thematic subsection rather than renumbering the existing list. IDs are therefore stable across the clarification pass but not in ascending order within the document. Downstream artifacts should reference IDs, not positions.

**Note 6 — MSRV resolved by maintainer decision, not by a question.** The third validation pass followed a maintainer decision supplied directly rather than an asked question, so zero questions were asked in that session. The Clarifications section therefore holds six bullets against five asked questions; the sixth is labelled as a supplied decision. It resolved the item research Finding 0 had flagged as blocking, and closed the last "derived, confirm during planning" marker on MSRV. The remaining derived-and-unconfirmed items are the supported-OS matrix, release ownership, and registry bootstrap ownership.

**Note 7 — Downstream artifacts kept in sync.** The MSRV decision invalidated statements in `plan.md`, `research.md`, `data-model.md`, and `contracts/support-policy.md` that described it as undecided. All four were updated in the same pass, so `/speckit-analyze` should find no spec-versus-plan contradiction on this topic. Residual mentions of "N-3"/"N-4" in those files are deliberate historical context explaining the rejected framing.

**Note 8 — Analyze remediation applied 2026-08-11.** All 16 findings from `/speckit-analyze` were fixed. Three were constitution-alignment issues: an unbounded waiver (now dated), an ADR-acceptance contradiction (now an explicit blocking dependency), and zero task coverage for signed tags and a protected release environment (now T071/T072/T080). The spec gained no new requirement IDs — FR count remains 61 and SC count 16 — because every fix tightened existing requirement text rather than adding obligations. `tasks.md` grew 79 → 88 and was renumbered so IDs stay sequential in execution order; its Remediation Log records the finding-to-fix mapping.

**Note 5 — Two contradictions closed.** The clarification session resolved two internal conflicts present in the initial draft: (a) SC-010's zero-publish criterion conflicted with PLAN.md §19.2's "reserve or verify" language, since reserving a registry name requires publishing — resolved as verify-only with the residual risk tracked (FR-049); (b) FR-027 required review before merge, which a single maintainer cannot satisfy — resolved as a pull-request-and-checks gate with zero approvals under an expiring waiver (FR-051).

**Note 3 — Scope reality check.** The repository has no configured remote and has never been pushed publicly. Acquiring the hosting organization/repository and configuring its protections is treated as in scope (Assumptions → Repository state), which is why FR-027–FR-034 are written against hosting-platform posture rather than assuming it already exists.

- Check items off as completed: `[x]`
- Items marked incomplete require spec updates before `/speckit-clarify` or `/speckit-plan`
