---
description: "Contract — supported toolchains, platforms, MSRV floor, and change rules"
version: "1.0.0"
status: "normative — a public promise and a release contract under principle V; no release has occurred. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract: Support and Version Policy

**Feature**: Phase 001 — governance foundation | **Satisfies**: FR-017 – FR-021 | **Set by**: ADR-0003

This is a public promise. Under constitution principle V it is a release contract, and under principle X no value here may be claimed without a passing verification run behind it.

## Toolchain support

| Field | Value | Verified |
|---|---|---|
| Declared MSRV | **1.94.0** (released 2026-03-05) — a **fixed floor**, not a rolling offset | Must have a passing CI run at exactly this version |
| Declaration site | `[workspace.package] rust-version` at the workspace root; members inherit via `rust-version.workspace = true` | No second independent declaration may exist |
| Edition | **2024**, declared explicitly on every package | Not inherited by a virtual workspace |
| Stable channel tested | Current stable, **1.97.1** at time of writing (released 2026-07-16) | Must have a passing CI run |
| Edition | 2024 (requires ≥ 1.85.0) | Satisfied by MSRV |
| Cargo resolver | 3 (requires ≥ 1.84.0), **declared explicitly** in the virtual workspace | Satisfied by MSRV; explicit declaration verified separately |

**The MSRV is a fixed, explicitly versioned support floor.** It is not N-3, N-4, or any offset from current stable. A new Rust stable release does not invalidate it, does not shorten it, and does not trigger a review. The minimum-version CI job stays pinned at 1.94.0; only the stable job moves.

This framing was chosen deliberately over a release-count window. Rust 1.98.0 promotes to stable on 2026-08-20, which would have pushed 1.94.0 to four releases behind and violated an "N-3" policy through the calendar alone, with no code change involved. A fixed floor removes release count from the policy entirely.

## Platform support

| Platform | Status in Phase 001 | Rationale |
|---|---|---|
| Linux (`ubuntu-latest`) | **Verified and supported** | Primary verification platform |
| macOS | Not yet claimed | No platform-sensitive code exists to verify |
| Windows | Not yet claimed | Same |

Only platforms with passing evidence are listed as supported. macOS and Windows enter the matrix in the phase that introduces platform-sensitive behaviour (PLAN.md §17.2). Claiming them now would violate constitution principle X's prohibition on claims exceeding measurement.

## MSRV change rules

- A raise may occur only in a **planned minor or major Renvor release**, never a patch.
- A raise requires an **accepted decision record** naming the concrete dependency, language, or security requirement that forces it. "Newer is better" is not a justification.
- Every raise is documented in **three** places: this support policy, the changelog, and the release notes.
- Each newly declared MSRV is supported for **at least six months** before it may be raised again.
- The policy is **reviewed quarterly**, and the review records its outcome. A review does **not** by itself change `rust-version`; only the process above does.
- The declared MSRV is never published without a passing run at that exact version.

**Scheduled obligation**: PLAN.md justified 1.94.0 by "the database stack," which does not exist until Phase 006. Rust 1.94.0 must be revalidated against the actual persistence dependencies **before Phase 006 begins**, with a named owner (FR-061).

## Dependency and lockfile rules

| Artifact kind | Version requirements | Lockfile |
|---|---|---|
| Reusable library crates | Compatible requirements (`1.2`), not exact pins | Not committed |
| Applications, generators, release tooling, automation | As resolved | **Committed** |
| Documentation site | As resolved | **Committed** (`package-lock.json`) |

Dependency updates arrive as reviewable pull requests through Dependabot across the `cargo`, `github-actions`, and `npm` ecosystems. Unreviewed floating updates are prohibited in generated output (FR-020).

### Security advisories against dependencies

Bounded response windows apply, measured from confirmed detection. **Triage** — severity, affected versions, named owner — within **24 hours** for known active exploitation or Critical, **48 hours** for High, **5 calendar days** for Medium, **10 calendar days** for Low. **Remediation** within **7 calendar days** for Critical, **14** for High, **30** for Medium, and **90 days or the next scheduled prerelease** for Low, whichever comes first.

Absence of an upstream fix does **not** extend a deadline: the dependency is removed, disabled, replaced, or isolated, or the affected release is blocked. Known **Critical and High vulnerabilities are release blockers and cannot be waived**. Silently ignoring an advisory without a dated record is prohibited.

The authoritative policy is `governance/dependency-advisory-policy.md`; the above is a summary, and any disagreement resolves in favour of that document. This governs advisories against **dependencies** and is distinct from the inbound private-report timetable in `SECURITY.md`, which is unchanged.

## Licence

`MIT OR Apache-2.0` at the recipient's choice, for Renvor's source and documentation. Contributions are accepted under the same dual terms. **Project code generated for a user carries no Renvor licensing obligation** and is owned outright by that user; generated output must not embed a Renvor licence header implying otherwise (FR-050).

## Change control

This contract changes only through a superseding ADR with an impact analysis covering published packages, documentation, the compatibility matrix, and any downstream consumer relying on the current promise.
