# Phase 1 Data Model: Governance Records

**Feature**: `specs/001-governance-foundation` | **Date**: 2026-08-11

This phase persists no application data. What it does persist are **governance records** — version-controlled Markdown whose structure has to be stable, because later phases, reviewers, and the phase gate all read them. Treating them as a data model is what makes them auditable instead of prose.

All records live under `governance/` or `decisions/` and change only through the protected pull-request path, so every mutation carries an author, a date, and a review trail for free.

---

## Entity: Name Availability Record

**File**: `governance/name-availability.md` | **Satisfies**: FR-001, FR-002, FR-003, FR-006, FR-048, FR-049

One row per public name Renvor intends to occupy.

| Field | Type | Rules |
|---|---|---|
| `item` | enum | One of: product name, package prefix, facade package, CLI package, executable, state directory, environment prefix, hosting organization, hosting repository, documentation domain |
| `intended_value` | string | The exact string being claimed, e.g. `renvor`, `renover`, `.renvor/`, `RENVOR_` |
| `location_checked` | string | Where availability was observed — registry URL, organization URL, registrar lookup |
| `date_checked` | date | ISO 8601. Required. |
| `status` | enum | `available` \| `owned-by-project` \| `held-by-other` \| `ambiguous` |
| `checked_by` | string | Named person. Required — an unattributed check is not evidence. |
| `evidence` | string | Link or quoted observation supporting the status |
| `decision` | string | Required when status is `held-by-other` or `ambiguous`; links the naming decision that resolved it |

**Validation rules**

- Every one of the ten `item` values must be present. A missing row is a blocker, not an omission (FR-001).
- `status` of `held-by-other` or `ambiguous` **stops the phase** until `decision` is populated. No substitute value may be written into `intended_value` automatically (FR-003).
- `date_checked` older than the validity window invalidates the row (FR-006).
- The record is complete only when every row is `available` or `owned-by-project`.

**Validity window**: **30 days**. Chosen because it is long enough to cover the phase's realistic duration and short enough that a name claimed by someone else in the interim is caught before the first push freezes it.

**State transitions**

```text
                 ┌──────────────────────────────────────────┐
                 │                                          │
  (unchecked) ──▶ checked ──▶ available ──▶ owned-by-project │
                    │                            │          │
                    ├──▶ held-by-other ──┐       │          │
                    └──▶ ambiguous ──────┴──▶ BLOCKED       │
                                              │             │
                                              ▼             │
                                     explicit naming        │
                                        decision ───────────┘

  Any row older than 30 days ──▶ back to (unchecked)
```

**Note on the Q1 clarification**: reaching `available` does not progress to `owned-by-project` for package-registry names in this phase, because names are verified rather than reserved. Those rows terminate at `available` with a linked known limitation (FR-049). Only the hosting organization and repository reach `owned-by-project` here (FR-048).

---

## Entity: Decision Record

**Files**: `decisions/NNNN-*.md` | **Satisfies**: FR-013, FR-014, FR-036

| Field | Type | Rules |
|---|---|---|
| `id` | integer | Four digits, monotonic, never reused |
| `title` | string | Imperative and specific |
| `state` | enum | `proposed` \| `accepted` \| `rejected` \| `superseded` |
| `context` | prose | The forces that make a decision necessary |
| `decision` | prose | What was chosen, stated unambiguously |
| `alternatives` | list | Each with the reason it was rejected. Required — a decision with no rejected alternatives was not a decision. |
| `consequences` | prose | Including the costs accepted, not only the benefits |
| `reviewer` | string | Required to enter `accepted` |
| `review_date` | date | Required to enter `accepted` |
| `superseded_by` | id | Required to enter `superseded` |

**Validation rules**

- `state: accepted` without both `reviewer` and `review_date` is a **defect in this phase's own acceptance** (FR-013, SC-009), not a paperwork lapse.
- Phase 001 must end with exactly four records in `accepted`: 0001 naming, 0002 workspace boundaries, 0003 MSRV and dependency policy, 0004 documentation platform (FR-014).
- A superseded record is never edited or deleted; the successor links back.

**State transitions**

```text
  proposed ──▶ accepted ──▶ superseded
      │            ▲
      │            │ requires reviewer + review_date
      └──▶ rejected
```

---

## Entity: Waiver Record

**File**: `governance/waivers.md` | **Satisfies**: FR-015, FR-051, and the constitution's exception mechanism

| Field | Type | Rules |
|---|---|---|
| `id` | string | `W-NNN` |
| `violated_rule` | string | The exact rule, cited by document and section |
| `reason` | prose | Why compliance is not currently possible |
| `compensating_controls` | list | What reduces the risk in the meantime. Required — a waiver with none is just a rule being ignored. |
| `owner` | string | A named person, not a team |
| `expiry` | date **and** optional condition | An absolute date is **mandatory**. A release condition (such as "a second maintainer joins") may accompany it, and the waiver ends at whichever arrives first. A condition alone is not permitted — the constitution requires waivers to be *time-bounded*, and a condition that never occurs would never expire. |
| `removal_plan` | prose | What must happen for the waiver to close |
| `status` | enum | `active` \| `closed` \| `expired` |

**Validation rules**

- No field is optional. The constitution requires all seven, and requires the waiver to be *time-bounded* — which is why `expiry` cannot be a bare condition.
- `expired` (past its date or condition, still open) is a **release blocker**, distinct from `active`.
- A waiver reaching its date without its condition being met is not automatically renewed. It must be re-justified and re-dated, or the underlying rule complied with.
- Security release blockers cannot be waived for a public release, per the constitution's governance section.

**Expected waivers at the end of Phase 001** — exactly one:

| id | Rule | Compensating controls | Expiry |
|---|---|---|---|
| W-001 | Independent human review before merge (FR-013, FR-027 intent) | Full verification sequence passing on every pull request; dependency, licence, and secret scanning gates. **Not** "no direct pushes" or "no administrator bypass" — FR-027 mandates both unconditionally, so they compensate for nothing | **2027-02-11** or a second maintainer joining, whichever is first |

Any additional waiver is a signal that something in the design did not work as planned and needs review before the phase closes. In particular, **no cost-based or availability-based waiver is expected**, because research Finding 3 confirmed every required control is free on a public repository (SC-008).

---

## Entity: Support and Version Policy

**File**: `SUPPORT.md`, contract in [`contracts/support-policy.md`](./contracts/support-policy.md) | **Satisfies**: FR-017, FR-018, FR-019

| Field | Type | Rules |
|---|---|---|
| `msrv` | version | Single authoritative value; `rust-version` in package manifests must match |
| `tested_toolchains` | list | Must contain `msrv` and current stable, both actually exercised in CI |
| `supported_os` | list | What is verified, not what is assumed to work |
| `msrv_change_rule` | prose | Which release kinds may raise the MSRV |
| `notice_period` | prose | How a raise is announced |
| `effective_date` | date | When this row of the policy took effect |

**Validation rules**

- `msrv` appears in exactly one authoritative location; every other reference derives from it (FR-017).
- A value may not be listed in `tested_toolchains` unless a passing run exists at that exact version (FR-019, and constitution X's prohibition on claims exceeding measurement).
- `supported_os` lists only platforms with passing evidence. Phase 001 lists Linux only.

**Resolved 2026-08-11**: `msrv` is **1.94.0**, a fixed explicitly-versioned floor. `msrv_change_rule` is: raised only in a planned minor or major release, only after an accepted ADR names a concrete forcing requirement, documented in support policy + changelog + release notes, with a minimum six-month dwell time per declared floor. Quarterly review records an outcome but never changes the version by itself. ADR-0003 records this decision rather than making it.

An additional field applies as a result:

| Field | Type | Rules |
|---|---|---|
| `last_reviewed` | date | Set by each quarterly review; changing it must not change `msrv` |
| `floor_declared_on` | date | Start of the six-month dwell period for the current floor |

---

## Entity: Dependency and Licence Policy

**File**: `deny.toml` (machine-readable, authoritative) with prose in `CONTRIBUTING.md` | **Satisfies**: FR-009, FR-010, FR-021

| Field | Type | Rules |
|---|---|---|
| `allowed_licenses` | list | SPDX expressions permitted without review |
| `review_required_licenses` | list | Permitted only with a recorded written review |
| `denied_licenses` | list | Never permitted |
| `allowed_sources` | list | Registries a dependency may come from |
| `advisory_policy` | prose | How RustSec advisories are triaged and within what window |
| `unmaintained_policy` | prose | Outcome options: replace, waive with expiry, or block |
| `lockfile_rule` | prose | Which artifact kinds commit lockfiles |

**Validation rules**

- The machine-readable form is authoritative; prose that disagrees with `deny.toml` is a defect.
- A crate with no licence expression is denied — absence is not permission.
- No publishable crate may carry a git or path dependency (FR-040).

---

## Entity: Repository Protection Baseline

**File**: `governance/phase-001-evidence.md` (observed state) | **Satisfies**: FR-027 – FR-034, FR-051

| Field | Type | Rules |
|---|---|---|
| `default_branch` | string | `main` |
| `visibility` | enum | `public` — from creation (Q4 clarification) |
| `requires_pull_request` | boolean | `true` |
| `required_approvals` | integer | `0` while single-maintainer, under waiver W-001 |
| `required_checks` | list | The exact check names that must pass |
| `admin_bypass` | boolean | `false` — no account holds bypass |
| `secret_scanning`, `push_protection`, `code_scanning`, `dependency_review` | boolean | All `true` before the first content push |
| `workflow_default_permissions` | enum | `read` |
| `actions_pinned_by_sha` | boolean | `true` for every third-party action |

**Validation rules**

- `required_approvals` must equal `0` only while the maintainer count is 1 and W-001 is `active`; it must rise to ≥ 1 when that condition ends (FR-051).
- Every boolean scanning field must be `true` with no cost-based waiver permitted (FR-030, SC-008).
- `admin_bypass: true` is a hard failure with no waiver path — it defeats the control entirely.

---

## Entity: Release Rehearsal Evidence

**File**: `governance/phase-001-evidence.md` | **Satisfies**: FR-038, FR-039, FR-040

| Field | Type | Rules |
|---|---|---|
| `date`, `operator`, `platform`, `toolchain_version` | — | All required |
| `packaged_crate` | string | `renvor` |
| `artifact_path`, `artifact_sha256` | string | The produced package and its checksum |
| `file_list` | list | The exact contents `cargo package --list` reported |
| `metadata_validation` | pass/fail | Against [`contracts/package-metadata.md`](./contracts/package-metadata.md) |
| `publish_operations_performed` | integer | **Must be `0`** |
| `registry_versions_after` | integer | **Must be `0`** — absence of publication proven, not assumed |

**Validation rules**

- `publish_operations_performed != 0` fails the phase outright (SC-010).
- `file_list` must contain no secret, local configuration, build output, or unintended asset (FR-039).
- `registry_versions_after` is checked against the live registry, because proving a negative requires looking.

---

## Entity: Repository Cleanup and Scan Record

**File**: `governance/phase-001-evidence.md` | **Satisfies**: FR-024, FR-025, FR-052 | **Produced by**: quickstart Gates 0 **and** 4

The evidence that the repository was safe to make public. Opened before the repository is created and closed at the first content push, because it spans two scans taken at two different moments.

| Field | Type | Rules |
|---|---|---|
| `publish_decisions` | list | One entry per ambiguous item — `PLAN.md`, the legacy root planning documents, `Branding/`, `specs/`, the tooling directories — each `include` or `exclude` with a reason. Silence is not a decision. |
| `tracked_set_reviewed` | boolean | The exact file list that would become public, reviewed item by item |
| `index_clean` | boolean | Nothing staged that Stage 0 excluded; no editor, OS, or agent tool state |
| `worktree_clean` | boolean | `git status --porcelain` empty |
| `objects_before`, `objects_after` | size | Before and after pruning unreachable objects |
| `scans` | repeating group | **Two scans are required**, each recording `scan_tool`, `scan_version`, `scan_date`, `scan_scope`, `findings`, and `purpose`. A single set of fields cannot represent both |
| `scans[pre_creation]` | scan record | Run before the repository is created. `findings` **must be 0** to proceed |
| `scans[pre_push]` | scan record | Re-run immediately before the first content push, covering the state actually being pushed. `findings` **must be 0** to proceed |
| `default_branch` | string | Must read `main` before any remote exists |

**Validation rules**

- `findings != 0` on the pre-creation scan **blocks repository creation**; on the pre-push scan it **blocks the push**. The remedy sequence is rotate → revoke → purge → record → re-scan, in that order, and only then may the gate be re-attempted.
- The pre-push scan is not optional even when the pre-creation scan was clean. Roughly twenty file-creating tasks separate them, so the earlier scan describes a repository state that no longer exists (FR-025).
- Every field is required. A cleanup record with an unrecorded publish decision means something shipped without anyone choosing to ship it.
- `default_branch` must be correct *before* the remote exists — protection applied to a branch that is later renamed does not follow the rename cleanly.

**Note on scope**: research Finding 11 established that unreachable objects are never transmitted by `git push`, so `objects_before`/`objects_after` measure local hygiene rather than exposure. They are recorded because an unambiguous starting state is worth having, not because 65 MB of unreachable blobs were ever going to reach the public.

---

## Entity: Phase Completion Record

**File**: `governance/phase-001-evidence.md` | **Satisfies**: FR-042, FR-043, and PLAN.md §6.2

| Field | Type | Rules |
|---|---|---|
| `criterion` | string | One row per PLAN.md Phase 001 acceptance criterion and per spec success criterion SC-001…SC-015 |
| `evidence_link` | string | Artifact, log, or record |
| `command_or_action`, `platform`, `operator`, `date`, `result` | — | All required |
| `open_blockers` | list | Non-empty means the phase stays open |
| `known_limitations` | list | Each with a named owner and target phase |

**Validation rules**

- Coverage must be 100% — every criterion has a row (SC-011).
- A row without `evidence_link` counts as unevidenced, which is indistinguishable from unmet.
- The known-limitations list must include the FR-049 residual risk: verified-but-unreserved package names remain claimable.

---

## Relationships

```text
Cleanup and Scan Record (pre-creation scan) ──┐
                                              ├── jointly block ──▶ organization + repository creation
Name Availability Record (all rows confirmed) ┘

Cleanup and Scan Record (pre-push scan) ──────┐
                                              ├── jointly block ──▶ first content push (FR-025, FR-052)
Name Availability Record (still within 30 d)  ┘

Name Availability Record ──feeds──▶ ADR-0001 (naming)

ADR-0003 (MSRV, toolchain, dependencies) ──sets──▶ Support and Version Policy
                                         └─sets──▶ Dependency and Licence Policy

Support and Version Policy ──constrains──▶ Repository Protection Baseline
                                             (supplies the required check names)

Waiver Record (W-001) ──referenced by──▶ Repository Protection Baseline

Release Rehearsal Evidence ─────┐
Repository Protection Baseline ─┼──▶ Phase Completion Record ──gates──▶ Phase 002
Name Availability Record ───────┘
```

**Four gates in strict order**: the Repository Cleanup and Scan Record's **pre-creation scan** and the **Name Availability Record** jointly gate organization and repository creation; the same record's **pre-push scan** gates the first content push; and the **Phase Completion Record** gates Phase 002. Everything else is produced between them.

That is the dependency order `/speckit-tasks` must preserve — and the reason cleanup cannot be scheduled as tidy-up work at the end. It is also why the scan appears twice: the first proves the local state is safe to expose, the second proves the state actually being pushed is, and roughly twenty file-creating tasks separate them.
