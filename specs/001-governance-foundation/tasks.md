---

description: "Phase 001 task list — governance, names, toolchain, and repository security"
---

# Tasks: Governance, Names, Toolchain, and Repository Security Foundation

**Input**: Design documents from `/specs/001-governance-foundation/`

**Prerequisites**: [plan.md](./plan.md), [spec.md](./spec.md), [research.md](./research.md), [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)

**Tests**: No separate test tasks. This phase ships no runtime code; the verification sequence in [contracts/verification-sequence.md](./contracts/verification-sequence.md) *is* the test, and it appears as real tasks (T041–T043) rather than as a test suite.

**Organization**: Grouped by user story so each is independently completable and verifiable.

**Revision**: 2026-08-11 — renumbered after `/speckit-analyze`; seven tasks added to close zero-coverage and ordering findings (see Remediation Log at the end).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies on incomplete tasks)
- **[Story]**: US1–US6, mapping to the prioritised stories in spec.md
- Exact file paths included; for actions with no source file, the record that captures the result is named

## Path Conventions

Repository root is the Renvor workspace root. Governance records live in `governance/`, decision records in `decisions/`, per the structure decision in plan.md.

## How to count the tasks in this file

*(Added 2026-08-15. Counting used to rely on Markdown checkboxes alone, which have two states
and cannot express "cancelled", "transferred", or "waived". A checkbox count therefore
disagreed with the prose. **Count by task ID and explicit status marker instead.**)*

**The total is the number of distinct task IDs, not the number of checkboxes.** Every task has
a unique `Tnnn` ID and exactly one status:

| Status | How it is written | Counts as completed? |
|---|---|---|
| **Completed** | `- [X] Tnnn …` with no status marker | **Yes** |
| **Open** | `- [ ] Tnnn …` with no status marker | No |
| **Transferred** | `- [ ] Tnnn …` containing **`TRANSFERRED`** | **No** — the work is real, still owed, and moved to a named future phase or workflow |
| **Waived** | `- [ ] Tnnn …` containing **`WAIVED`** | **No** — the requirement was not met; a recorded waiver explains why |
| **Cancelled** | a non-checkbox line beginning **`**Tnnn — CANCELLED / NOT APPLICABLE**`** | **No** — the task's subject ceased to exist; its requirements were never met |

**Cancelled, transferred, and waived tasks are never completed tasks.** Reporting must keep the
categories separate, for example `108 completed, 1 waived, 1 cancelled, 4 transferred
(114 total)`. **Never report a bare "N completed" figure derived from a checkbox count**, and
never report the total as though it were the completed figure.

Reproducible commands:

```bash
cd specs/001-governance-foundation

# Total — distinct task IDs, checkbox or not. Expect 114.
grep -oE '(^- \[[ xX]\] |^\*\*)T[0-9]{3}' tasks.md | grep -oE 'T[0-9]{3}' | sort -u | wc -l

# Cancelled — non-checkbox records. Expect 1 (T114).
grep -cE '^\*\*T[0-9]{3} — CANCELLED / NOT APPLICABLE\*\*' tasks.md

# Waived and transferred — unchecked, and NOT completed.
grep -E '^- \[ \] T[0-9]{3}' tasks.md | grep -c 'WAIVED'
grep -E '^- \[ \] T[0-9]{3}' tasks.md | grep -c 'TRANSFERRED'

# Completed — checked boxes. No cancelled, waived, or transferred task is ever checked.
grep -cE '^- \[[xX]\] T[0-9]{3}' tasks.md

# Open — unchecked and carrying neither marker.
grep -E '^- \[ \] T[0-9]{3}' tasks.md | grep -vc 'WAIVED\|TRANSFERRED'
```

**To list which IDs are in a category, anchor on the *leading* ID.** A task's body routinely
mentions other task IDs — T102's transfer record names T106, for instance — so
`grep -oE 'T[0-9]{3}'` over a whole line returns every ID it *mentions*, not the one it *is*.
That inflates any ID list while leaving the counts above correct, which makes it easy to miss:

```bash
# Correct — capture only the ID at the start of the line
grep -E '^- \[ \] T[0-9]{3}' tasks.md | grep 'TRANSFERRED' | sed -E 's/^- \[ \] (T[0-9]{3}).*/\1/'

# Every ID must be unique across all five statuses; this must print nothing
{ grep -E '^- \[[ xX]\] T[0-9]{3}' tasks.md | sed -E 's/^- \[[ xX]\] (T[0-9]{3}).*/\1/'
  grep -oE '^\*\*T[0-9]{3}' tasks.md | tr -d '*'; } | sort | uniq -d
```

**Fail closed**: if the five status counts do not sum to the total, the file is inconsistent and
the discrepancy must be resolved before any count is published.

---

## Phase 1: Setup (Shared Infrastructure)

**Purpose**: Create the record scaffolding and install tooling. Nothing here makes a decision or touches anything public.

- [X] T001 [P] Create `governance/` and `decisions/` directories with a `.gitkeep` in each
- [X] T002 [P] Create the decision-record template at `decisions/0000-template.md` with the exact field set from data-model.md §Decision Record (id, title, state, context, decision, alternatives, consequences, reviewer, review_date, superseded_by)
- [X] T003 [P] Create record skeletons with table headers from data-model.md: `governance/name-availability.md`, `governance/waivers.md`, `governance/phase-001-evidence.md`
- [X] T004 [P] Install `cargo-deny`, `gitleaks`, and `lychee` with `--locked`; record exact tool versions in `governance/phase-001-evidence.md`
- [X] T005 [P] Install Rust toolchains `1.94.0` and current stable via rustup; record both versions in `governance/phase-001-evidence.md`

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Resolve the specification contradictions, propose the names that verification will check, decide what may be published, and bring the local repository to a clean, auditable state.

**⚠️ CRITICAL**: No user story work may begin until this phase completes. Everything here is **local only** — no remote exists yet.

### Specification rulings (must precede any work that cites them)

- [X] T006 Rule on the independent-review requirement (analyze finding F1, governance.md CHK074): establish in `GOVERNANCE.md` who qualifies as an independent reviewer, and where none exists, record waiver W-002 in `governance/waivers.md` with an absolute expiry date. **This task blocks every decision-record acceptance in this phase** (T026, T039, T040, T066)
- [X] T007 Rule on the waiver-scope wording (analyze finding F2, governance.md CHK075): confirm the amended FR-030 and SC-008 in `specs/001-governance-foundation/spec.md` read consistently — expected waiver count of exactly one, with any additional waiver treated as a reviewed exception rather than a permitted allowance. **Blocks T055 and T063**
- [X] T008 Propose candidate values for the hosting organization, hosting repository, and documentation domain, and record them in `governance/name-availability.md` as `intended_value` entries with status `unchecked`. Availability cannot be verified without a candidate, so this precedes T019–T022 (analyze finding U1)
- [X] T009 Record publish-set decisions in `governance/phase-001-evidence.md` — include or exclude with a written reason for `PLAN.md`, `RENVOR_FRAMEWORK_DEVELOPMENT_PLAN.md`, `RENVOR_MASTER_IMPLEMENTATION_PLAN.md`, `Branding/`, `specs/`, `.claude/`, and `.specify/`. Silence is not a decision

### Repository cleanup (ordering is load-bearing — see plan.md §Pre-Push Repository Cleanup)

- [X] T010 Rewrite `.gitignore` to cover Rust build output, `node_modules/`, `.docusaurus/`, documentation build output, editor state (`.idea/`, `.vscode/`), OS artifacts (`.DS_Store`, `Thumbs.db`), environment and credential files, agent tool state (`.claude-flow/`, `.playwright-mcp/`, `.remember/`), and everything T009 excluded
- [X] T011 Unstage the four `.idea/` entries from the git index with `git restore --staged .idea/`; confirm via `git ls-tree -r HEAD --name-only` that none was ever committed, so no history rewrite is required (depends on T010)
- [X] T012 Prune unreachable objects (expire unreachable reflog entries, then garbage-collect); record `git count-objects -vH` before and after in `governance/phase-001-evidence.md` (depends on T011, so unstaged blobs are actually collected)
- [X] T013 Run **`gitleaks git .`** (full history) and **`gitleaks dir .`** (working tree); record tool version, date, commit range, and finding count for **both** in `governance/phase-001-evidence.md`. Resolve to zero findings, or record each false positive as a narrow, individually justified allowlist entry in `.gitleaks.toml` — no broad path or rule exclusions (depends on T012, so the scan covers the final state). **Command note:** `gitleaks detect` was removed in Gitleaks 8.x; 8.30.1 exposes only `git`, `dir`, and `stdin`. **Allowlist note:** a `paths` allowlist makes gitleaks skip the file before scanning it — a blanket exclusion. Scope allowlists by `regexes`/`regexTarget` instead, and prove narrowness with a canary-secret injection test.
- [X] T014 Rename the default branch `master` → `main` locally, before any remote exists; record in `governance/phase-001-evidence.md`
- [X] T015 Seed `governance/waivers.md` with waiver W-001 (single-maintainer approval gap) carrying all seven required fields, with an **absolute expiry date of 2027-02-11** alongside the second-maintainer condition, expiring at whichever comes first. Compensating controls MUST be the full verification sequence on every pull request plus the scanning gates — **not** "no direct pushes" or "no administrator bypass", which FR-027 already mandates unconditionally (analyze findings C1, A1)

**Checkpoint**: Local repository is clean and audited, both spec contradictions are ruled on, candidate names exist, and the publish set is decided. Quickstart Gate 0 should now pass. Nothing is public yet.

---

## Phase 3: User Story 1 - Verified public identity before anything is frozen (Priority: P1) 🎯 MVP

**Goal**: Every public name has dated evidence with a definite status, and the hosting organization and repository are under project control.

**Independent Test**: Read `governance/name-availability.md` — all ten rows present, each with location, date, status, and checker; no unconfirmed name in any frozen reference; deliberately unavailable names handled by stopping rather than substituting.

- [X] T016 [P] [US1] Verify package-registry availability for `renvor`, `renvor-cli`, and the `renvor-` prefix; record one row per name in `governance/name-availability.md` with location checked, date, status, checker, and evidence link
- [X] T017 [P] [US1] Verify the `renover` executable name against existing registry binary names for collisions; record row in `governance/name-availability.md`
- [X] T018 [US1] Record the derived-name rows (`Renvor` product name, `.renvor/`, `RENVOR_`, `renover new`) and the 30-day validity window in `governance/name-availability.md`
- [X] T019 [P] [US1] Verify hosting organization name availability using the candidate from T008; record row in `governance/name-availability.md`
- [X] T020 [P] [US1] Verify hosting repository name availability using the candidate from T008; record row in `governance/name-availability.md`
- [X] T021 [P] [US1] Verify documentation domain availability at a registrar using the candidate from T008; record row in `governance/name-availability.md`
- [X] T022 [US1] **STOP GATE**: if any row is `held-by-other` or `ambiguous`, halt the phase and record an explicit naming decision in `decisions/`. Do not select a substitute name, do not commit a partial rename, do not proceed to T023 (depends on T016–T021)
- [X] T023 [US1] Update `specs/001-governance-foundation/contracts/public-identity.md` replacing the three "candidate required" placeholders with the confirmed values
- [X] T024 [US1] Create the public hosting organization and repository using the confirmed names; **do not push any content** (depends on T022)
- [X] T025 [US1] Record ownership confirmation — maintainer, security contact, release approver, registry owner — in `governance/phase-001-evidence.md`
- [X] T026 [US1] Write `decisions/0001-public-naming-and-namespace.md` justifying the deliberate product-versus-executable distinction; set state `accepted` only under the review process settled at **T006**

**Checkpoint**: Public identity is confirmed and the organization is claimed. Registry names are verified but deliberately unreserved; that residual risk is captured at T083.

---

## Phase 4: User Story 2 - A clean checkout that verifies itself (Priority: P2)

**Goal**: A fresh clone passes one documented verification sequence on both toolchains, and leaves the working tree clean.

**Independent Test**: Clone fresh, run `cargo xtask verify` on 1.94.0 and on stable — both exit 0 with every step executed; then run with tooling hidden from `PATH` and confirm exit code 2 with no checks run.

- [X] T027 [US2] Create root `Cargo.toml` as a virtual workspace with `resolver = "3"` **declared explicitly** (a virtual workspace has no package edition to inherit it from — research Finding 1), members `crates/renvor` and `xtask`, and a `[workspace.package]` table declaring `rust-version = "1.94.0"` and `edition = "2024"` as the single authoritative source (FR-016, FR-017)
- [X] T028 [P] [US2] Create `rust-toolchain.toml` pinning the toolchain for contributors and automation
- [X] T029 [P] [US2] Create `rustfmt.toml` and `clippy.toml`
- [X] T030 [US2] Create `crates/renvor/Cargo.toml` inheriting `rust-version.workspace = true` and `edition.workspace = true` rather than restating either, plus the complete publishable metadata from contracts/package-metadata.md — `license = "MIT OR Apache-2.0"`, description, repository, homepage, documentation, readme, keywords, categories, explicit `include`
- [X] T031 [US2] Create `crates/renvor/src/lib.rs` containing crate-level documentation and a version constant only — no runtime framework capability (FR-047)
- [X] T032 [P] [US2] Create `xtask/Cargo.toml` with `publish = false`, inheriting edition and rust-version from the workspace
- [X] T033 [US2] Implement the `verify` subcommand in `xtask/src/main.rs` running the ten ordered steps from contracts/verification-sequence.md with exit codes 0/1/2/3
- [X] T034 [US2] Implement the fail-closed toolchain probe in `xtask/src/main.rs` — exit 2, name each missing tool with its install command, and print an explicit "no checks were run" line so a partial run can never read as success (FR-023, FR-055)
- [X] T035 [US2] Create `deny.toml` with the licence allow / review-required / deny sets, advisory policy, bans, and crates.io-only sources per research Finding 4
- [X] T036 [US2] Commit `Cargo.lock` per the lockfile rule in contracts/support-policy.md
- [X] T037 [US2] Assert that no second independent MSRV declaration exists — `rust-version` appears once at the workspace root and only as inheritance in members; record the check in `governance/phase-001-evidence.md` (FR-017)
- [X] T038 [US2] Assert minimum-version-aware dependency resolution is **in effect**, not merely configured, and record the observation in `governance/phase-001-evidence.md` (SC-016)
- [X] T039 [P] [US2] Write `decisions/0002-workspace-boundaries-and-facade-stability.md`; accept only under the review process settled at **T006**
- [X] T040 [P] [US2] Write `decisions/0003-msrv-toolchain-and-dependency-policy.md` recording the fixed-floor MSRV of 1.94.0, the six-month dwell time, the quarterly review that changes nothing by itself, and the Phase 006 revalidation obligation; accept only under **T006**
- [X] T041 [US2] **(depends on T064, T065, T067 — corrected ordering)** Run `cargo xtask verify` under `1.94.0` and under `stable`; record both runs with platform, operator, date, and result in `governance/phase-001-evidence.md`
- [X] T042 [US2] Run `cargo xtask verify` with the optional tooling removed from `PATH` and confirm exit code 2 with the "no checks were run" line; record the result (FR-023)
- [X] T043 [US2] Confirm `git status --porcelain` is empty after a full verification run, proving the ignore rules from T010 are correct

**Checkpoint**: The repository verifies itself from a clean checkout on both toolchains, and fails closed when it cannot.

---

## Phase 5: User Story 3 - Governance, licensing, and a working security-reporting path (Priority: P3)

**Goal**: A contributor, a legal reviewer, and a security researcher each find what they need within one link of the repository root.

**Independent Test**: From the rendered README, reach all six governance documents in one link; confirm every publishable package declares `MIT OR Apache-2.0`; send a test report through the private path and confirm it arrives.

- [X] T044 [P] [US3] Add the full licence texts as `LICENSE-MIT` and `LICENSE-APACHE`
- [X] T045 [P] [US3] Write `SECURITY.md` with the private reporting path, a named monitored contact, a quantified acknowledgement window, and disclosure expectations
- [X] T046 [P] [US3] Write `CONTRIBUTING.md` including contribution licensing terms, the written dependency update policy (how updates are proposed, reviewed, and landed, and the prohibition on unreviewed floating updates), and a pointer to `deny.toml` as the authoritative machine-readable licence policy (FR-020)
- [X] T047 [P] [US3] Write `CODE_OF_CONDUCT.md`
- [X] T048 [P] [US3] Write `GOVERNANCE.md` naming decision authority, the independent-reviewer definition settled at T006, and the amendment process
- [X] T049 [P] [US3] Write `SUPPORT.md` from contracts/support-policy.md — fixed MSRV floor, tested toolchains, supported platforms, change rules
- [X] T050 [US3] Write `README.md` with the development-status notice required by FR-053, one-link navigation to all six governance documents, and a link to `CONSTITUTION.md` — the authoritative public copy at the repository root — stating its version and ratification date (FR-012, FR-053) (depends on T044–T049)
- [X] T051 [US3] State the generated-output licensing position (FR-050) in `CONTRIBUTING.md` and `README.md` — generated project code carries no Renvor obligation
- [X] T052 [US3] Send a test report through the `SECURITY.md` private path, confirm it reaches the monitored contact, and record the test in `governance/phase-001-evidence.md`

**Checkpoint**: Legal, contribution, governance, and security-reporting posture is complete and discoverable.

---

## Phase 6: User Story 4 - Secure repository defaults and least-privilege automation (Priority: P4)

**Goal**: The default branch cannot change without a pull request and passing checks, automation runs least-privilege with immutably pinned actions, and every free public-tier security control is enabled.

**Independent Test**: Inspect the platform configuration and every workflow — protection requires a PR and the four named checks, no account holds bypass, all scanning controls are on, every third-party action resolves to a 40-character SHA, and a direct push to `main` is refused.

- [X] T053 [US4] **RE-SCAN GATE**: re-run **both** `gitleaks git .` and `gitleaks dir .` over the tree and history as they now stand, and record the fresh dated result of each in `governance/phase-001-evidence.md`. The T013 scan predates roughly twenty file-creating tasks and does not describe the state about to be published (FR-025, analyze finding O1). Re-run the T013 canary-injection test as well, to confirm no allowlist entry has silently widened.
- [X] T054 [US4] **FIRST PUSH GATE**: push initial content to the public repository — permitted only after T022 (names confirmed), T044 (both licence texts), T045 (security contact live), and **T053** (current zero-finding scan) are all complete (FR-052)
- [X] T055 [US4] Configure branch protection on `main`: pull request required, required status checks, **0** approvals under waiver W-001, and **no bypass permission on any account including administrators**. The four required check names are registered at T062 once the workflows producing them exist; until then protection intentionally references checks that are not yet reporting (analyze finding O2)
- [X] T056 [P] [US4] Enable secret scanning with push protection, code scanning (CodeQL, Rust GA since 2025-10-14), dependency graph and alerts, and dependency review — all free on the public tier, so no cost-based waiver is acceptable
- [X] T057 [P] [US4] Create `.github/workflows/ci.yml` running `cargo xtask verify` across the `1.94.0` and `stable` matrix, with top-level `permissions: contents: read`
- [X] T058 [P] [US4] Create `.github/workflows/security.yml` running cargo-deny, dependency review, CodeQL, and clippy→SARIF upload, with `security-events: write` scoped to the upload job only
- [X] T059 [P] [US4] Create `.github/workflows/docs.yml` for the documentation build and link check, with `pages: write` and `id-token: write` scoped to the deploy job only
- [X] T060 [P] [US4] Create `.github/dependabot.yml` covering the `cargo`, `github-actions`, and `npm` ecosystems
- [X] T061 [P] [US4] Create `.github/ISSUE_TEMPLATE/` bug and feature templates plus `config.yml` routing security reports to the private path, and `.github/PULL_REQUEST_TEMPLATE.md` and `.github/RELEASE_TEMPLATE.md`
- [X] T062 [US4] Pin every third-party action in `.github/workflows/` to a full 40-character commit SHA with a trailing `# vX.Y.Z` comment, then register the four required check names from contracts/verification-sequence.md — `verify (1.94.0)`, `verify (stable)`, `security`, `docs` — in branch protection (depends on T055, T057–T059)
- [X] T063 [US4] Record the observed protection baseline in `governance/phase-001-evidence.md` against every field in data-model.md §Repository Protection Baseline, and confirm the waiver count is exactly one (SC-008)

**Checkpoint**: The repository is public, protected, and scanned, with exactly one waiver outstanding.

---

## Phase 7: User Story 5 - A documentation platform chosen on evidence (Priority: P5)

**Goal**: The documentation platform decision is recorded with alternatives and reasons, and a placeholder set builds and link-checks from a clean checkout.

**Independent Test**: Read the decision record for alternatives, criteria, decision, consequences, and owner; then build the documentation set from a clean checkout and run link checking.

- [X] T064 [US5] Scaffold Docusaurus 3.10.x in `docs/` with the local/offline search plugin rather than a hosted index, and commit `docs/package-lock.json` (FR-054)
- [X] T065 [P] [US5] Add `.nvmrc` pinning the Node LTS line, referenced by both contributors and CI
- [X] T066 [US5] Write `decisions/0004-documentation-platform-and-versioning.md` naming Docusaurus, recording mdBook, MkDocs+Material, and Zola as rejected with reasons, the accepted Node toolchain cost, the versioning cadence, and a named owner; accept only under the review process settled at **T006**
- [X] T067 [US5] Create the placeholder documentation set under `docs/docs/` sufficient to exercise build and link checking
- [X] T068 [US5] Wire lychee link checking into `xtask` verify step 9 and into `.github/workflows/docs.yml`
- [X] T069 [US5] Establish the prose↔API documentation cross-link and version stamp so both describe the same contract at the same version (FR-056)

**Checkpoint**: Documentation platform is decided on evidence and builds clean.

---

## Phase 8: User Story 6 - A release rehearsal that provably publishes nothing (Priority: P6)

**Goal**: The release path is exercised end to end from a clean checkout, producing a package artifact and zero publish operations, with release-identity controls configured.

**Independent Test**: Run the rehearsal, confirm an artifact exists, confirm the registry reports zero versions, confirm tag signing and the protected environment are configured, and confirm every acceptance criterion maps to dated evidence.

- [X] T070 [US6] Write `RELEASING.md` covering topological publish order with the index-availability wait, version immutability and yank-and-replace as the sole remedy, the least-scope bootstrap credential procedure with immediate revocation, and the evidence retention period **MUST incorporate `governance/evidence-retention-policy.md` exactly** — reproducing its periods or referencing it as authoritative; a divergent restatement is a defect (T103).
- [X] T071 [US6] Configure commit and tag signing (SSH or GPG), enable vigilant mode, and require signed tags for releases; record the signing identity and verification method in `governance/phase-001-evidence.md` (FR-032, constitution §XI, analyze finding G1)
- [X] T072 [US6] Create a protected release environment on the hosting platform with **named** approvers and a deployment-branch restriction limiting it to release tags; record the environment name and approver list in `governance/phase-001-evidence.md` (FR-032, PLAN.md §19.1)
- [X] T073 [US6] Create `.github/workflows/release-dry-run.yml` with `permissions: contents: read` and **no publish capability**, so it cannot publish even if invoked
- [X] T074 [US6] Run `cargo package -p renvor --list` from a clean checkout and record the exact file list in `governance/phase-001-evidence.md`
- [X] T075 [US6] Inspect that file list for secrets, local configuration, build output, and unintended assets; record the review outcome (FR-039)
- [X] T076 [US6] Run `cargo package -p renvor` and `cargo publish --dry-run -p renvor`; record artifact path and `sha256` in `governance/phase-001-evidence.md`
- [X] T077 [US6] Validate package metadata against contracts/package-metadata.md and confirm no publishable package carries a path-only dependency
- [X] T078 [US6] Query the live registry and record **zero versions** for every intended name — positive evidence of non-publication, not an assertion that nothing was run (SC-010)
- [X] T079 [US6] Confirm no long-lived registry credential exists in the repository, its workflows, or its secrets; record the check (FR-033)
- [X] T080 [US6] Record the complete release-identity control set in `governance/phase-001-evidence.md` — signed tags, protected environment with named approvers, provenance and bill-of-materials plan — each marked configured or covered by a dated waiver (SC-014, analyze finding G2)
- [X] T081 [US6] Wire CycloneDX SBOM generation, checksums, and `actions/attest` provenance into the release path and record what a real release would emit. Trusted publishing itself **cannot** be configured this phase — it requires a package that already exists on the registry, and nothing is published (research Finding 2)

**Checkpoint**: The release path works, its identity controls are configured, and it has published nothing — provably.

---

## Phase 9: Polish & Cross-Cutting Concerns

**Purpose**: Assemble the evidence that gates entry to Phase 002.

- [X] T082 Complete `governance/phase-001-evidence.md` with one row per PLAN.md Phase 001 acceptance criterion and per SC-001 through SC-016, each carrying evidence link, command or action, platform, operator, date, and result **Completed 2026-08-15.** `governance/phase-001-evidence.md` §4 now carries **7 of 7 PLAN.md Phase 001 acceptance criteria** (§4.1) and **16 of 16 success criteria, SC-001 through SC-016** (§4.2). Every row carries an evidence link, a command or action, a platform, an operator, a date, and a result. **0 unevidenced rows.** Rows whose result is qualified say so in the result cell — SC-001 carries residual risks R-1 and R-3, SC-014 carries the open key-storage obligation R-13, and SC-016 carries R-7, the requirement to revalidate the MSRV floor against real persistence dependencies before Phase 006. **The table is explicitly labelled as self-recorded and reviewed under W-002 and W-003 as a NON-INDEPENDENT self-review.**
- [X] T083 Record known limitations in `governance/phase-001-evidence.md` with named owner and target phase — including the FR-049 residual risk that verified-but-unreserved package names remain claimable, and the FR-061 obligation to revalidate MSRV 1.94.0 against real persistence dependencies before Phase 006 **Completed 2026-08-15.** `governance/phase-001-evidence.md` §6 now carries **15 limitation rows**, each with a named owner and a target phase. R-1 records the FR-049 residual risk that `renvor` and `renvor-cli` are verified but **unreserved** and remain claimable by a third party before first publication; **R-7** records the FR-061 obligation to revalidate MSRV 1.94.0 against real persistence dependencies before Phase 006, and states why the floor is currently proven against nothing — the only Phase 001 crate has **zero dependencies**. R-8 through R-15 were added in this pass and record, respectively: `renvor-site` not requiring signed commits or linear history; `renvor-infra` having protection but no CI and therefore no verification; `renvor-docs` being commit-empty, unprotected, and without secret scanning; the **T095–T097 approval anchors being unreproducible** because §3aq never recorded the hashing method; GHCR being unenumerable with the current token scope; the signing key pair residing in a third-party-synchronised directory; absolute local paths deliberately retained in §3af; and Phase 001's reliance on a **non-independent** review. R-6 (stale local `stable` toolchain) is struck through as resolved. **No limitation is closed by being recorded.**
- [X] T084 Create the recurring-obligations register in `governance/phase-001-evidence.md` with owner and first due date for each: the quarterly MSRV policy review (FR-060), the W-001 waiver expiry on 2027-02-11, and the pre-Phase-006 MSRV revalidation (analyze finding G6) **Completed 2026-08-15.** `governance/phase-001-evidence.md` §7 now carries the register in four parts — waiver expiries, toolchain and dependency obligations, evidence and release obligations, and security actions — with **owner, first due date, early trigger, and removal condition** on every row. It includes the three the task names — the quarterly MSRV policy review (FR-060, first due **2026-11-11**), the **W-001** expiry on **2027-02-11**, and the pre-Phase-006 MSRV revalidation (finding G6) — and adds the **W-002** expiry on **2027-02-11**, the **T108** and **T109** reassessments on **2026-09-11** (T109's explicitly marked as **not yet performed**), the advisory triage windows (24 h / 48 h / 5 d / 10 d), the evidence-retention periods (90 days, seven years, project lifetime), the `renvor.dev` renewal on **2027-08-11**, the unreserved-package-name watch, and three security actions: **signing-key storage hardening**, controls before `renvor-docs` receives its first commit, and required checks for `renvor-infra` before its first manifest. Obligations with no end are marked **permanent** rather than given a false due date.
- [X] T085 [P] Run the full quickstart.md gate sequence 0 through 8 and record every outcome in `governance/phase-001-evidence.md` **Completed 2026-08-15.** Gates 0 through 8 were run **from a fresh `git clone --no-local` into a scratch directory outside the work tree**, with the clone's HEAD asserted equal to `ab75494639960adc063ccfb97150cbe0f53b316d` before any gate ran — **no historical result was reused**. `cargo xtask verify` returned **10/10 under both `rustc 1.94.0` and `rustc 1.97.1`**; both `gitleaks git` and `gitleaks dir` exited **0** at Gate 0 and again at Gate 4; the documentation build succeeded with **225 links OK and 0 errors**; the release rehearsal produced **1 artifact and 0 publish operations**, with the registry re-checked to **HTTP 404** for all three names against a `serde` **200** control. **Gate 5 is read-only and no push was attempted.** **The run found a defect in the gate itself**: Gate 3's licence assertion demanded a literal `license` key in `crates/renvor/Cargo.toml`, which **ADR-0002 forbids** — a crate that passed it would have been violating the ADR. Corrected to assert that the workspace declares the terms and the member inherits them. **SC-016 verified across all four MSRV-stating locations with 0 mismatches.** Evidence §3ba
- [X] T086 [P] Work through `specs/001-governance-foundation/checklists/governance.md` (79 items) and record findings inline
- [X] T087 Confirm no runtime framework capability was implemented and no unshipped capability is described as available anywhere in the repository, reviewing against the FR-047 exclusion list and FR-044; record the result (SC-013) **Completed 2026-08-15.** **SC-013 passes**: the workspace contains **2** Rust source files, `crates/renvor` exposes exactly **3 public items — all `pub const &str`** (`VERSION`, `MSRV`, `EXECUTABLE`) over **3 lines** of non-comment library code, has **zero dependencies**, and contains **0** of every category on the FR-047 exclusion list; `xtask` is `publish = false` internal tooling. Enumerated by **two independent methods that agreed** — rustdoc item pages and a `pub` grep with a proven control; `cargo public-api` is not installed and was not claimed. **The FR-044 half initially FAILED and was corrected**: **four** documents asserted that the `renvor` crate is **published** — `SUPPORT.md`, `SECURITY.md`, `docs/docs/intro.mdx`, and `docs/src/pages/index.js` — when `crates.io` returns **HTTP 404** for `renvor`, `renvor-cli`, and `renover` (verified against a `serde` **200** control). **Root cause: the identical sentence was corrected in `README.md` alone on 2026-08-12 and the fix was never generalised**, and link checking cannot catch a false factual claim. Generalised in this pass — three further instances corrected — plus a stale claim that every ADR remained `proposed` and a dead `renvor.dev` link that ships inside the published crate. Evidence §3ax
- [ ] T088 Confirm zero open blockers and obtain the independent requirements and security review required by PLAN.md §6.1 step 10 **WAIVED / NOT MET 2026-08-15 under W-003 — this is not a completed task.** **No independent human requirements and security review of Phase 001 has occurred.** The project has one maintainer, and `GOVERNANCE.md` defines an independent reviewer as a **person** who did not author the record, did not author the change, and is not directed by the author. **W-002 covers decision-record review only (FR-013) and does not reach this phase-level gate**, so a separate waiver was required. **W-003 was granted by Ahmed Anbar on 2026-08-15** and is recorded in `governance/waivers.md` with all seven mandatory fields. **W-003 waives only the independent-human-review requirement. It does not waive any finding, failed check, missing evidence, acceptance criterion, or security blocker**, and **security release blockers are never waived**. **Phase 001 must receive genuine independent re-review before any public release.** W-003 expires **2027-02-11** or **immediately** when a qualified independent reviewer becomes available, whichever occurs first. **The reviews that were performed are advisory and explicitly NON-INDEPENDENT**: clean-context agent reviews for requirements and for security, each against a written requirement list, each finding individually dispositioned — they found real defects, including false statements about repository visibility, deployment status, and a published crate, and that usefulness does **not** make them independent. Evidence §3az. **The first half of this task — confirm zero open blockers — is separately satisfied**: every remaining item is explicitly categorised as transferred (T102, T108, T109, T111) or cancelled (T114), each with an owner and a destination, and none is closed by rewording.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — start immediately
- **Foundational (Phase 2)**: Depends on Setup — **blocks every user story**. Entirely local; no remote exists
- **US1 (Phase 3)**: Depends on Foundational, and specifically on T008 for candidate names. **Gates every public action** — nothing may be pushed or published before T022
- **US2 (Phase 4)**: Depends on Foundational. Local work; may run concurrently with US1
- **US3 (Phase 5)**: Depends on Foundational. Local work; may run concurrently with US1 and US2
- **US4 (Phase 6)**: Depends on US1 (names), US2 (checks to require), and US3 (licences, security contact) — because T054 is the first public push
- **US5 (Phase 7)**: Depends on US2 (the xtask runner it extends)
- **US6 (Phase 8)**: Depends on US2 (the crate it packages), US3 (the metadata licence it declares), and US4 (the repository the protected environment lives in)
- **Polish (Phase 9)**: Depends on all stories

### Cross-cutting blocking dependencies

| Blocker | Blocks | Why |
|---|---|---|
| **T006** (independent-reviewer ruling) | T026, T039, T040, T066 | Constitution §Dev Workflow #4 forbids accepting a decision record without recorded independent review. Accepting any ADR before the ruling violates a MUST |
| **T007** (waiver-scope ruling) | T055, T063 | Both cite the waiver-count expectation |
| **T008** (candidate names) | T019, T020, T021 | Availability cannot be checked without a candidate value |
| **T053** (fresh secret scan) | T054 | The push must be authorised by a scan of the state actually being pushed |

### The four hard gates

```text
governance/name-availability.md complete  ──▶ T024  organization creation   (FR-003)
T022 + T044 + T045 + T053                 ──▶ T054  first content push      (FR-052)
T006                                      ──▶ every ADR acceptance          (constitution)
T082 + T088                               ──▶ Phase 002                     (SC-011)
```

Everything else is schedulable around these four.

### Note on ordering versus the original request

The requested sequence placed workspace creation before repository cleanup. Cleanup sits in Phase 2 instead, because it is a blocking prerequisite for anything public and because the prune step must follow unstaging to be effective. Both still precede the first push, so no constraint is weakened.

### Parallel Opportunities

- **Phase 1**: T001–T005 all parallel
- **Phase 2**: T006–T009 parallel with each other; T010→T011→T012→T013 strictly sequential — order is load-bearing
- **CORRECTED 2026-08-11 — T064, T065, T067 MUST precede T041.** Verification step 8 builds `docs/`, so `cargo xtask verify` cannot reach exit 0 until the documentation package exists. As originally numbered, T041 sat in Phase 4 while T064 sat in Phase 7 behind the T054 push gate, making T041 unreachable. The documentation check was **not** made optional and was **not** skipped; the task order was corrected instead.
- **US1**: T016, T017 parallel; T019–T021 parallel once T008 lands
- **US2**: T028, T029, T032 parallel; T039, T040 parallel
- **US3**: T044–T049 all parallel (six separate documents)
- **US4**: T056–T061 parallel after T054
- **Phase 9**: T085, T086 parallel

### Parallel Example: User Story 3

```bash
# Six independent governance documents, no shared file:
Task: "Add LICENSE-MIT and LICENSE-APACHE full texts"
Task: "Write SECURITY.md with private reporting path and acknowledgement window"
Task: "Write CONTRIBUTING.md with contribution and dependency update policy"
Task: "Write CODE_OF_CONDUCT.md"
Task: "Write GOVERNANCE.md naming decision authority"
Task: "Write SUPPORT.md from contracts/support-policy.md"
```

---

## Implementation Strategy

### MVP scope

**Phase 1 + Phase 2 + US1 (T001–T026).** That yields a repository with a confirmed, defensible public identity and a clean audited local state — the minimum that makes every later decision safe to build on. It is also the point at which a naming conflict, the one failure that would invalidate everything downstream, has been ruled out.

### Incremental delivery

1. Setup + Foundational → local state clean, rulings made, candidate names proposed, publish set decided
2. US1 → identity confirmed, organization claimed **(MVP — stop and validate)**
3. US2 → workspace verifies itself on both toolchains and fails closed without tooling
4. US3 → governance, licensing, security contact complete
5. US4 → fresh scan, first public push, protections and scanning live
6. US5 → documentation platform decided and building
7. US6 → release rehearsal with identity controls, publishing nothing
8. Polish → evidence pack complete, recurring obligations registered, phase gate reviewed

### Sequencing note for a single maintainer

US2, US3, and US5 are local and can be batched while waiting on external responses during US1 (registrar lookups, organization creation). US4 is the synchronisation point where everything becomes public at once.

---

## Notes

- **T022 is a stop, not a checkpoint.** If a name is unavailable the phase halts for an explicit decision; no substitute may be chosen and no partial rename committed (FR-003).
- **T006 blocks four ADR acceptances.** Accepting a decision record before the independent-review question is settled violates a constitution MUST, not merely a preference.
- **T013 permits documented false positives**, each narrowly justified in `.gitleaks.toml`. Broad path or rule exclusions defeat the control and are not acceptable.
- **T053 exists because scans have timestamps.** The Phase 2 scan describes a repository that no longer exists by the time of the push.
- **T054 is the irreversible one.** The repository is public from creation, so anything pushed is world-visible immediately and removing it later does not remove it from clones, forks, or caches.
- Commit after each task or logical group; every commit flows through the protected pull-request path once T055 is live.

---

## Remediation Log (2026-08-11)

Applied after `/speckit-analyze`. Seven tasks added, IDs renumbered so they remain sequential in execution order.

| Finding | Severity | Resolution |
|---|---|---|
| C1 waiver not time-bounded | CRITICAL | T015 now sets an absolute expiry date alongside the condition; data-model.md requires a date in all cases |
| F1 ADR review contradiction | CRITICAL | T006 is now an explicit blocking dependency of T026, T039, T040, T066, listed in the cross-cutting table |
| G1 signed tags / protected environment uncovered | CRITICAL | **New T071** (tag signing) and **T072** (protected environment with named approvers) |
| G2 SC-014 partially covered | HIGH | **New T080** records the full release-identity control set |
| O1 stale scan gating the push | HIGH | **New T053** re-scan gate immediately before T054 |
| U1 no candidate names to verify | HIGH | **New T008** proposes candidates; T019–T021 now depend on it; T023 writes confirmed values back to the contract |
| F2 FR-030 vs SC-008 | HIGH | Spec amended; T007 confirms consistency and blocks T055, T063 |
| G3 constitution not discoverable | MEDIUM | Folded into T050 |
| G4 edition 2024 unset | MEDIUM | T027 declares it in `[workspace.package]`; T030, T032 inherit |
| G5 MSRV single source | MEDIUM | T027 declares once at workspace level; **new T037** asserts no second declaration exists |
| G6 quarterly review has no trigger | MEDIUM | **New T084** recurring-obligations register with owner and first due date |
| A1 W-001 compensating controls | MEDIUM | T015 restates them, excluding FR-027 baselines |
| D1 terminology drift | LOW | Spec FR-017 designates **MSRV** canonical |
| G7 FR-044 weakly covered | LOW | Folded into T087 |
| G8 dependency update policy unwritten | LOW | Folded into T046 |
| O2 protection before checks exist | LOW | Documented as intentional in T055; registration moved into T062 |

### Pass 2 (2026-08-11) — artifact-sync drift caused by the pass 1 rewrite

| Finding | Severity | Resolution |
|---|---|---|
| N1 quickstart Gate 6 claimed SC-014 without checking signing, environment, or approvers | HIGH | Gate renumbered to 7 and extended with tag-signature, protected-environment, named-approver, and provenance checks |
| N2 no quickstart gate for the pre-push re-scan | MEDIUM | **New Gate 4** (push authorisation); gates 4–7 renumbered to 5–8; summary map now names four blocking gates |
| N3 FR-030 / SC-008 denominators ambiguous | MEDIUM | Both now say **control-unavailability waivers = 0** and **approval waivers = 1** explicitly |
| N4 Pre-Push Cleanup Record (renamed at P5 to **Repository Cleanup and Scan Record**) held one scan field set for two scans | MEDIUM | `scans` is now a repeating group with `pre_creation` and `pre_push` entries |
| N5 Gate 2 did not assert single-source MSRV | LOW | Gate 2 now asserts one literal declaration at the workspace root and inheritance in members |
| N6 plan.md structure tree omitted `.gitleaks.toml` | LOW | Added to the tree |

### Pass 3 (2026-08-11) — header-versus-body drift

| Finding | Severity | Resolution |
|---|---|---|
| P1 Gate 2 header omitted SC-016 | MEDIUM | Added; the gate body already checked it |
| P3 SC-005 measured one scan while two are required | MEDIUM | SC-005 now requires **both** scans to report zero, each separately dated and scoped |
| P2 Gate 8 header omitted SC-009 | LOW | Added |
| P4 cleanup record attributed to Gate 0 only | LOW | Now "Gates 0 and 4" |
| P5 entity name narrower than contents | LOW | Renamed **Repository Cleanup and Scan Record**; all five references updated |
| P6 Gate 0 / Gate 1 blocking overlap | LOW | Both now stated as joint preconditions of organization and repository creation; blocking-gate table replaces the prose sentence |
| *(surfaced by P5)* data model said "three gates" | LOW | Corrected to four, matching quickstart |

### Pass 4 (2026-08-11) — stale plan metadata

| Finding | Severity | Resolution |
|---|---|---|
| Q1 plan.md Phase Status still showed tasks ungenerated | MEDIUM | Phase 2 checked with the current task count; an analysis row and an implementation row added |
| Q2 plan.md artifact listing omitted `checklists/governance.md` | LOW | Added |
| Q3 relationship diagram implied the scan and name records were sequential | LOW | Redrawn as two joint-precondition pairs; the ADR block was also realigned and corrected — it had shown ADR-0001 feeding the dependency policy, which ADR-0003 sets |

---

## Phase 10: Web properties and deployment topology (added 2026-08-11)

**Purpose**: `PLAN.md` §26 and ADR-0005/ADR-0006 created new Phase 001 obligations. These are recorded as concrete tasks rather than left in prose. **Every external action below sits behind its own approval gate.**

**Scope note**: Phase 001 *records* topology, ownership, security boundaries, and the deployment decision process. It does **not** create repositories, provision infrastructure, change DNS, or deploy.

- [X] T089 Record the four-repository web-property topology in `PLAN.md` §26 without renumbering the thirty framework phases, and cross-reference it from §18, §19, and phases 001/012/013
- [X] T090 Write `decisions/0005-web-properties-and-deployment-topology.md` covering repository layouts, privacy rationale, transparency cost, release coupling, documentation source-of-truth, rollback, ownership, rejected alternatives, and the `docs/` migration gate; state `proposed` under **T006**
- [X] T091 Perform a read-only production-server audit over `ssh hostinger` and record it in `governance/phase-001-evidence.md` §3u with sensitive values redacted; change nothing on the server
- [X] T092 Write `decisions/0006-production-hosting-and-edge-architecture.md` after the audit, comparing Kubernetes distributions, ingress versus Cloudflare Tunnel, TLS mode, GitOps versus scoped workflow, registry, secrets, backup, rollout/rollback, monitoring, and disaster recovery; state `proposed` under **T006**
- [X] T093 Write `governance/web-properties-migration-plan.md` defining the V7 landing and documentation migrations, inclusion/exclusion lists, preserved properties, and required links
- [X] T094 Audit the V7 landing page against actual release state and record the result; **the audit FAILED** — see T095–T097
- [X] T095 **RELEASE-HONESTY GATE**: add a prominent development-status notice to the V7 landing page and re-word every present-tense capability claim to state its actual status (blocks any public landing deployment; `PLAN.md` §26.6). **APPROVED 2026-08-12 by maintainer attestation.** Ahmed Anbar personally reviewed the rendered production build at `http://localhost:4891/` on desktop and mobile, in both light and dark themes, and approved the landing-page presentation, the truthful development-status disclosure, the planned-version labels, the non-installable CLI demonstrations, the GitHub-only links, the responsive layout, the accessibility behaviour, the reduced-motion behaviour, and the animations. **Reviewed state**: `renvor-rs/renvor-site` HEAD `8b68e326867e3c296b2117fd13ca855b478cd036` **plus the uncommitted working-tree corrections** — source-set SHA-256 `b4c0856f0f03c85870d421557d19b09330ef4d69d9022b982aa397746c3bc59b`, build output SHA-256 `ae95f1e5c6e9c935a7693d4fbedbbd3260abe7fa676dc9fd8dfe9c1fb815698f` (21 files, 908 KB). **This is a visual and product approval of the page's presentation and truthfulness. It is NOT evidence that the framework has been released, that any crate is published, or that the site may be deployed** — deployment remains blocked by T101, T102, T106, T108, and T111. Evidence §3aq
- [X] T096 Remove or clearly mark the `renover new` / `renover add` installation commands on the V7 landing page until the referenced crates are publicly installable (depends on T095). **APPROVED 2026-08-12 by maintainer attestation.** Ahmed Anbar personally reviewed the rendered production build at `http://localhost:4891/` on desktop and mobile, in both light and dark themes, and approved the landing-page presentation, the truthful development-status disclosure, the planned-version labels, the non-installable CLI demonstrations, the GitHub-only links, the responsive layout, the accessibility behaviour, the reduced-motion behaviour, and the animations. **Reviewed state**: `renvor-rs/renvor-site` HEAD `8b68e326867e3c296b2117fd13ca855b478cd036` **plus the uncommitted working-tree corrections** — source-set SHA-256 `b4c0856f0f03c85870d421557d19b09330ef4d69d9022b982aa397746c3bc59b`, build output SHA-256 `ae95f1e5c6e9c935a7693d4fbedbbd3260abe7fa676dc9fd8dfe9c1fb815698f` (21 files, 908 KB). **This is a visual and product approval of the page's presentation and truthfulness. It is NOT evidence that the framework has been released, that any crate is published, or that the site may be deployed** — deployment remains blocked by T101, T102, T106, T108, and T111. Evidence §3aq
- [X] T097 Repoint every V7 CTA at a resolving destination — `crates.io/crates/renvor` is HTTP 404, `docs.renvor.dev/getting-started` does not resolve, `renvor-rs/renvor` is empty — or remove the CTA (depends on T095). **APPROVED 2026-08-12 by maintainer attestation.** Ahmed Anbar personally reviewed the rendered production build at `http://localhost:4891/` on desktop and mobile, in both light and dark themes, and approved the landing-page presentation, the truthful development-status disclosure, the planned-version labels, the non-installable CLI demonstrations, the GitHub-only links, the responsive layout, the accessibility behaviour, the reduced-motion behaviour, and the animations. **Reviewed state**: `renvor-rs/renvor-site` HEAD `8b68e326867e3c296b2117fd13ca855b478cd036` **plus the uncommitted working-tree corrections** — source-set SHA-256 `b4c0856f0f03c85870d421557d19b09330ef4d69d9022b982aa397746c3bc59b`, build output SHA-256 `ae95f1e5c6e9c935a7693d4fbedbbd3260abe7fa676dc9fd8dfe9c1fb815698f` (21 files, 908 KB). **This is a visual and product approval of the page's presentation and truthfulness. It is NOT evidence that the framework has been released, that any crate is published, or that the site may be deployed** — deployment remains blocked by T101, T102, T106, T108, and T111. Evidence §3aq
- [X] T098 Decide and record the **website-code licence** and the **brand-asset usage terms** for V7. **DECIDED 2026-08-12 — maintainer selected option B (split):** website source code under **`MIT OR Apache-2.0`**; Renvor names, logos, marks, illustrations, and brand assets **all rights reserved** under a concise brand-usage policy. The code licences grant **no** trademark or brand-identity rights (Apache-2.0 §6 withholds them explicitly; MIT is silent rather than granting). Permitted without asking: truthful nominative reference, links to the official project, screenshots, and community discussion including criticism. Prior permission required for: confusingly similar branding, endorsement or official-status claims, merchandise, and branding a fork as official Renvor. Published file set validated in `renvor-rs/renvor-site`: `LICENSE-MIT` and `LICENSE-APACHE` (both **byte-identical** to the framework's, SHA-256 compared), `BRAND-POLICY.md`, and a rewritten `README.md` licence section; every internal link target and all three named brand assets resolve; no file is excluded by `.gitignore`. References updated in `framework/README.md`, `docs/README.md`, and `infra/README.md`. **The archived historical branding directories are outside every repository, unpublished, and deliberately left unlicensed and uncovered.** Evidence §3ak
- [X] T099 Decide and record the **container registry**, including the credential model. **DECIDED 2026-08-12 — maintainer selected GitHub Container Registry (`ghcr.io`)**, rejecting the GitLab registry already on the VPS on two grounds: publishing to it from GitHub Actions would require a long-lived cross-system credential, and a registry living on the origin is unavailable in exactly the recovery scenario ADR-0006 D9 depends on it for. **Credential model**: GitHub Actions publishes with the run's **short-lived `GITHUB_TOKEN`** under least privilege — **`contents: read` and `packages: write` on the image-publishing job only**. **No PAT, deploy token, repository secret, or long-lived registry credential is created.** **This is not OIDC** — `GITHUB_TOKEN` is an installation token scoped to the run; an earlier draft of ADR-0006 described it as OIDC and that error is corrected. The **deployment image is publicly pullable**, so the k3s host stores **no `imagePullSecret`**; package visibility is independent of repository visibility, and the image carries only already-public static content. Images are addressed and deployed **by immutable digest**, never by a mutable tag alone. Recorded in ADR-0006 D7 and its Alternatives table, `PLAN.md` §26.4, evidence §3al. **Nothing was configured**: no package, workflow, image, credential, or infrastructure change. **Only the registry decision is complete — registry configuration, image publication, deployment workflows, and production deployment all remain blocked** (ADR-0006 unresolved question 1)
- [X] T100 Add a `.nvmrc` pinning **Node 24** to the landing migration checklist; the framework repository pins Node 22 and the two must not be conflated (depends on T098). **Completed 2026-08-13** in `renvor-rs/renvor-site` via pull request **#2**, merged as `fe0e468e8ed6b54d211423b056e0d44a0669b66c` from reviewed signed source commit `78b2e0fa48212c3a8c5e7eba782e2648df7adf93`. Changed path `.nvmrc`, content exactly `24\n`; diff scope **one file, one insertion**. The framework's own `.nvmrc` is untouched and still pins Node 22, so the two policies remain separate rather than conflated. **Validation**: Node `v24.19.0`, pnpm `11.21.0`, `pnpm install --frozen-lockfile`, `pnpm run typecheck`, and the production build all passed, with `pnpm-lock.yaml` byte-for-byte unchanged. **Boundary**: T100 proves only the repository-local Node 24 pin. **It does not prove CI, branch protection, deployment readiness, or deployment** — the site repository has no repository-owned CI, and GitHub reports `main` as unprotected (REST `protected: false`, GraphQL zero branch-protection rules and no rule on `main`, and a direct push previously succeeded; the rulesets REST endpoint is unavailable for this private repository on the current plan). Tracked as **T113**. Evidence §3as
- [X] T101 Verify CSP compatibility with the V7 landing implementation (GSAP, self-hosted variable fonts) before Cloudflare security headers are enabled (ADR-0006 unresolved question 4). **The original wording is preserved as written, but the owner it names has changed**: T110 ruled on 2026-08-12 that Cloudflare is authoritative DNS only with the proxy off, so security headers are no longer a Cloudflare Transform Rule — **actual ownership moved to the origin, as a Traefik middleware the project must write and maintain itself** (ADR-0006 D5, D11). **Completed 2026-08-14.** Verified against `renvor-rs/renvor-site` pull request **#3**, merged as `206cefdff74399d96f723a75d961fb8d700e0fd5` from base `fe0e468e8ed6b54d211423b056e0d44a0669b66c` and audited signed source `f8f1786a02c2d921859068fbd487b5d5e57a764c`. **The source tree and the merge tree are both `e7fbc9d1438eaf58dee2c7d634dac4003b8664ec`**, so integration added no content beyond the audited source tree. **Compatibility was verified locally, against that immutable build**, under a local enforcement harness. **Results**: the negative-control Enforcement preflight passed **3/3**, one per Chromium, Firefox, and WebKit, proving the harness actually blocks; the final exact-head r4 Enforcement matrix passed **48/48** across Chromium, Firefox, and WebKit × landing and genuine HTTP 404 × desktop and mobile × light and dark × normal and reduced motion, with zero application CSP events or refusals, page errors, failed requests, third-party requests, duplicate events, or collector transport errors. **GSAP ran without `unsafe-inline` or `unsafe-eval`.** `Outfit Variable` and `Geist Mono Variable` were fetched from same-origin `/assets/fonts/...` resources under `font-src 'self'`; the candidate policy also allowed `data:` fonts, but r4 did not establish that allowance as necessary. **The local harness did configure and serve the enforcement header — that is what made the run an Enforcement test.** What did not happen is production: **no production response header was configured or enabled, no Traefik middleware was written, configured, or enabled, and no live-server access or production-infrastructure action occurred** — no deployment, image publication, DNS, server, Kubernetes, credential, Cloudflare, or other infrastructure action. **ADR-0006 remains `proposed`** — its one remaining internal unresolved question, **T106**, is still open. Evidence §3at
- [ ] T102 Re-verify the server audit immediately before any deployment; the host is shared with unrelated production workloads and the 2026-08-11 facts can go stale (ADR-0006 additional gate) **TRANSFERRED 2026-08-15 to the future deployment workflow — NOT COMPLETED.** T102 requires re-verifying the server audit **immediately before** any deployment. No deployment is scheduled or authorised, so running it now would produce evidence that is stale by the time it matters — which is precisely the failure mode the task exists to prevent. **The read-only inspection performed on 2026-08-15 for T106 does NOT satisfy T102**: it was not immediately before a deployment, it was scoped to the backup ruling, and the host is shared with unrelated production workloads whose facts change without notice. The task keeps its ID, its full text, and its open status, and it must be executed by the deployment workflow before the first Renvor deployment.

**Checkpoint**: The topology, hosting architecture, and migration plan are recorded and reviewable. *(Historical, 2026-08-11: at the time this checkpoint was written no repository existed, no DNS record existed, and nothing was deployed. As of 2026-08-12 the maintainer has manually created the three private repositories and the three temporary DNS-only A records — see `governance/phase-001-evidence.md` §3af. No infrastructure is provisioned and nothing is deployed.)*

---

## Phase 11: Governance review findings (added 2026-08-12)

**Purpose**: T086 reviewed all 79 governance checklist items and found two genuine specification gaps. They are recorded as tasks rather than checked off prematurely or resolved by weakening the requirement.

- [X] T103 Define a concrete evidence retention period. FR-046 requires build evidence to be retained "for a stated period" but no duration exists in the specification, the contracts, or the release documentation. State the duration, its start event, and where retained evidence lives (governance checklist CHK048)
- [X] T104 Define a security-advisory response window. FR-010 requires the policy to state how advisories are handled but sets no triage or response deadline, so an advisory could remain unactioned indefinitely without violating any rule. This is distinct from the SECURITY.md windows, which govern inbound reports rather than advisories against dependencies. **Blocks acceptance of ADR-0003** (governance checklist CHK050)
- [X] T105 Decide whether the `www.renvor.dev` permanent redirect is served by Cloudflare or by Traefik, and record it. **Decided 2026-08-12: Cloudflare, HTTP 301, preserving path and query string** — recorded as ADR-0006 D6 with its consequences, including that the rule needs a proxied record and therefore cannot function while `www` is DNS-only. The rule was **not created**. ADR-0006 remains `proposed`: T099, T101, and T106 are still unresolved (ADR-0006 unresolved question 2)
- [X] T106 Record the maintainer ruling on the shared server's absent backups — whether the gap blocks Renvor deployment, given that both Renvor properties are stateless while five unrelated production namespaces are not. **Blocks acceptance of ADR-0006** (ADR-0006 unresolved question 3) **RESOLVED 2026-08-15 by maintainer ruling.** **The ruling**: *The absence of shared-cluster backups does not block future deployment of Renvor's stateless landing and documentation properties. This does not remediate, accept ownership of, or make guarantees for the unrelated stateful namespaces. Any future stateful Renvor workload remains blocked until separately reviewed backup and restore controls exist. A Renvor deployment must remain additive, isolated, resource-bounded, digest-addressed, and reversible without modifying unrelated workloads.* **Two clauses were added on the live evidence.** **(1) Resource-bounding and isolation must be created, not inherited** — the cluster contains **zero** `ResourceQuota`, **zero** `LimitRange`, and **zero** `NetworkPolicy` objects, so Renvor must supply its own, and **NetworkPolicy enforcement must be verified on this CNI before it is relied upon**; a NetworkPolicy no controller enforces is decoration. **(2) The absence of backups is total, which is why the ruling is narrow** — there is no backup of any kind, not merely no cluster backups, and node loss is total loss for all 57 GiB of unrelated stateful data. **Renvor is exempt only because it is stateless and reconstructible from public GitHub plus a registry; that exemption ends the moment any Renvor workload holds state.** Based on a **read-only** inspection of the live shared host on 2026-08-15 (evidence §3ay): no backup or snapshot mechanism of any kind, 5 unrelated PVCs totalling 57 GiB, **no Renvor namespace or object**, and ample headroom. **T106 unblocks ADR-0006 acceptance and nothing else — it authorises no deployment**, and T102, T108, and T111 remain non-completed. Evidence §3ay

**Checkpoint**: Every governance checklist item has a defensible recorded outcome, and every discovered gap is either corrected or tracked as an explicit open task.

---

## Phase 12: Documentation dependency advisories (added 2026-08-12)

**Purpose**: five advisories were raised against `docs/package-lock.json` on 2026-08-11. Two were closed by a tested override at T107; three have no compatible fix and are tracked here rather than silently accepted. Existing task numbers are unchanged — these are appended.

- [X] T107 Close `GHSA-5c6j-r48x-rmvq` (high, RCE, CVSS 8.1) and `GHSA-qj8w-gfj5-8c6v` (moderate) by overriding `serialize-javascript` to `^7.1.0` in `docs/package.json`. Docusaurus 3.10.2 pins `copy-webpack-plugin ^11` and `css-minimizer-webpack-plugin ^5`, both requiring `serialize-javascript ^6`, so no ordinary compatible update exists. Prove the override with a frozen install, a production build, a link check, and a CommonJS load test
- [ ] T108 **DOCUMENTATION DEPLOYMENT GATE — resolve `image-size`**: `GHSA-w3rx-r6r6-pgpr` and `GHSA-5p2g-fcmc-qvqq` (both high, CVSS 7.5, infinite-loop DoS in the ICNS, JXL, and HEIF parsers). **No fixed version exists** — 2.0.2 is simultaneously the affected and the latest published version — and **the upstream repository is archived**, so no fix is coming from upstream at all. Reached only through `@docusaurus/mdx-loader`. **Time-bounded exception recorded 2026-08-12** (evidence §3am) on proven reachability, with a fail-closed guard implemented at `docs/scripts/check-image-inputs.mjs` and wired as `prebuild`/`prestart`, tested in both directions. **The advisories are NOT suppressed, dismissed, or described as fixed.** **T108 REMAINS OPEN**: two compensating controls cannot be objectively verified because their subjects do not exist — **no container definition exists (0 found)**, so absence from the production runtime container is unverifiable, and **no SBOM artifact exists (0 found)**, so absence from the runtime SBOM is unverifiable. Both must be verified when the deployment image and its SBOM are first produced. Owner Ahmed Anbar; reassess **2026-09-11**, or earlier if Docusaurus ships a maintained replacement or fixed release **TRANSFERRED 2026-08-15 — NOT COMPLETED, and the advisories remain open.** **`GHSA-w3rx-r6r6-pgpr` and `GHSA-5p2g-fcmc-qvqq` (both High, CVSS 7.5) are unresolved**: no fixed version exists, 2.0.2 is simultaneously the affected and the latest published version, and the upstream repository is archived, so no upstream fix is coming. **They are NOT suppressed, dismissed, waived, or described as fixed.** The Phase 001 fail-closed guard at `docs/scripts/check-image-inputs.mjs`, wired as `prebuild`/`prestart` and tested in both directions, **stays in force and is not weakened by this transfer**. The two compensating controls that cannot be verified because their subjects do not exist — **absence from the production runtime container** and **absence from the runtime SBOM** — **transfer to Phase 012 (documentation deployment)**, to be verified when that image and that SBOM are first produced. The **2026-09-11 reassessment stands** and is registered in the recurring-obligations register (evidence §7.2). Owner Ahmed Anbar.
- [ ] T109 Reassess `GHSA-w5hq-g745-h8pq` (moderate, `uuid` < 11.1.1). Not reachable today: `sockjs` calls only `uuid.v4()` with no `buf` argument while the advisory affects v3/v5/v6 **with** `buf`, and `sockjs` arrives via `webpack-dev-server`, which runs only for `docusaurus start` and never in the production build. `sockjs` 0.3.24 is the latest release and pins `uuid ^8.3.2`, so no compatible update exists. Do **not** force a three-major override into a path CI never exercises. Owner Ahmed Anbar; reassess **2026-09-11**, or immediately if `sockjs` ships a fix or the dev server enters a deployed path **TRANSFERRED 2026-08-15 to the recurring-obligations register (evidence §7.2) — NOT COMPLETED.** **Its 2026-09-11 reassessment has NOT occurred and must never be recorded as having occurred early.** `GHSA-w5hq-g745-h8pq` (moderate, `uuid` < 11.1.1) keeps its **advisory** status unchanged: not reachable today because `sockjs` calls only `uuid.v4()` with no `buf` argument while the advisory affects v3/v5/v6 **with** `buf`, and `sockjs` arrives through `webpack-dev-server`, which runs only for `docusaurus start` and never in the production build. `sockjs` 0.3.24 remains the latest release and pins `uuid ^8.3.2`, so no compatible update exists, and **no three-major override is forced into a path CI never exercises**. The register records owner **Ahmed Anbar**, first due **2026-09-11**, early triggers (`sockjs` shipping a fix, or the dev server entering a deployed path), and the removal condition.

**Checkpoint**: every advisory has a dated record, a named owner, and either a proven fix or an explicit gate. None is silently ignored.

---

## Phase 13: Edge-architecture correction (added 2026-08-12)

**Purpose**: the maintainer ruled that Cloudflare is authoritative DNS only and the proxy will not be enabled. ADR-0006 as written assumed a proxied edge, so several records described protections that are not in the request path. Existing task numbers are unchanged — this is appended, and **T105 is neither reopened nor rewritten**.

- [X] T110 Record the corrective hosting decision: **Cloudflare is authoritative DNS only; the proxy stays off; no Tunnel; no Origin CA; no Authenticated Origin Pulls; no wildcard records; public TLS issued at the origin by the existing cert-manager and Let's Encrypt; the permanent `www.renvor.dev` → `https://renvor.dev` 301 redirect served by Traefik, preserving path and query string.** Supersedes the proxied-edge model in ADR-0006 D3/D4/D5/D10 and the Cloudflare `www` rule decided at T105, which stays complete as the correct decision under the architecture then in force. Record the consequences without softening them: the origin IP is public and is the only server answering; **no WAF, edge rate limiting, bot management, or DDoS absorption is in the request path**; TLS, redirect behaviour, availability, resource limits, and origin security are the operator's responsibility; observability drops to origin-side only. Rewritten in `decisions/0006-production-hosting-and-edge-architecture.md` (D3, D4, D5, D10, new D11; duplicate `D6` heading corrected), `PLAN.md` §26.3, `infra/README.md`, and `governance/phase-001-evidence.md` §3ai. **ADR-0006 remains `proposed`** — T099, T101, and T106 are still unresolved

**Checkpoint**: no governance document claims an edge protection that is not in the request path, and the superseded reasoning is preserved rather than deleted.

---

## Phase 14: Certificate-authority authorisation (added 2026-08-12)

**Purpose**: with the Cloudflare proxy off (T110), every certificate for every Renvor hostname is issued at the origin by cert-manager. Nothing currently constrains **which** CA may issue for `renvor.dev` — a read-only check on 2026-08-12 found **no `CAA` record**. ADR-0006 D5 already requires one. Existing task numbers are unchanged — this is appended.

- [ ] T111 Create `CAA` records for `renvor.dev` restricting issuance to **Let's Encrypt only**, with an `iodef` contact. **Policy decided 2026-08-12; the DNS change is NOT authorised and was NOT made.** Proposed records, with `flags` `0` (non-critical) so an unrecognised tag cannot break issuance: `renvor.dev. CAA 0 issue "letsencrypt.org"` — permits Let's Encrypt to issue single-name certificates; `renvor.dev. CAA 0 issuewild ";"` — **forbids every CA from issuing a wildcard**, since no wildcard certificate is planned and no wildcard DNS record exists; `renvor.dev. CAA 0 iodef "mailto:admin@ahmedanbar.dev"` — requests notification of policy violations. **Effects**: `issue` and `issuewild` are inherited by every subdomain unless that subdomain sets its own `CAA`, so one record set at the apex covers `docs.` and `www.`; `issuewild ";"` is the explicit deny form and is required because an `issue` record alone would otherwise permit wildcard issuance by the named CA. **Sequencing matters**: cert-manager's HTTP-01 challenges must be working *before* these are added, because a mistyped domain name silently blocks all issuance. Verify after creation with a CAA lookup against both authoritative nameservers, then with a test issuance. **Keep this task open until the maintainer separately authorises the DNS change and verifies it.** Owner Ahmed Anbar **TRANSFERRED 2026-08-15 to the future deployment workflow — NOT COMPLETED, and it remains deployment-blocking.** The CAA **policy** was decided 2026-08-12 and the exact records were drafted (evidence §3an); **the DNS change is NOT authorised and was NOT made**, and no DNS record was created, modified, or deleted at any point. T111 stays blocked until **both** conditions hold: **cert-manager HTTP-01 issuance is proven** for the affected hostnames, and **Ahmed Anbar separately authorises the exact CAA writes**. Issuing a CAA record before HTTP-01 is proven risks locking out the very issuer the project depends on, which is why the order is not negotiable. Owner Ahmed Anbar.

**Checkpoint**: the certificate-authority policy is written down and reviewable before any DNS record is touched, and no record was created in the pass that wrote it.

---

## Phase 15: Link-check transport determinism (added 2026-08-12)

**Purpose**: verification step 9 failed twice on pull request #11 without any link being broken, first with HTTP 503 responses and then with HTTP/2 protocol errors. A required check that fails on network conditions rather than on content trains reviewers to re-run it until it passes, which is indistinguishable from disabling it. Existing task numbers are unchanged — this is appended.

- [X] T112 Make the `github.com` link check deterministic without weakening it. **Diagnosed 2026-08-12**: every external link in the built site targets one host, and `lychee` 0.24.2 defaults to 10 concurrent requests per host at 50 ms intervals, so the checker opened enough simultaneous HTTP/2 streams on a single connection for GitHub to reset them — surfacing as `HTTP/2 protocol error` with **no HTTP status at all**, and reproducing on one matrix job while the other passed on the same commit with `0 Errors`. Fixed with options verified against the installed binary and its source rather than assumed: `[hosts."github.com"] concurrency = 1` and `request_interval = "250ms"` in `lychee.toml`, `strategy.max-parallel: 1` in `.github/workflows/ci.yml`, and the run's built-in `GITHUB_TOKEN` supplied to `lychee`'s documented environment interface under the existing `contents: read`. **HTTP 429 removed from `accept`** — a rate-limited response no longer counts as a working link. **The check was not weakened**: `github.com` is not excluded, no link was removed or rewritten, HTTP/2, TLS, timeout, 429 and 5xx failures are not accepted, no `continue-on-error`, no failure downgraded to a warning, retries stay bounded at 2, and both toolchains and all ten steps are unchanged. Proven in both directions locally — five consecutive runs exit 0 with identical counts, while fabricated GitHub URLs still fail with `lychee` exit 2 and real 404s. Recorded in `governance/phase-001-evidence.md` §3ar, which preserves both original failures rather than rewriting them

**Checkpoint**: step 9 fails when a link is broken and passes when it is not, and neither outcome depends on how many requests happened to be in flight.

---

## Phase 16: Landing repository CI and branch protection (added 2026-08-13)

**Purpose** *(as written 2026-08-13; the state it describes was resolved 2026-08-14, see the T113 completion record below)*: T100 pinned the site repository's Node version, but a pin only binds what actually reads it. `renvor-rs/renvor-site` has **no repository-owned CI**, so nothing enforces the pin and nothing runs the checks the framework requires of itself. GitHub also reports `main` as **unprotected**: REST reports `protected: false`, GraphQL reports zero branch-protection rules and no rule attached to `main`, and a direct push to `main` previously succeeded — the rulesets REST endpoint is unavailable for this private repository on the current plan, so rulesets were not inspected. The migration plan states the landing repository's CI must consume its own `.nvmrc`; that half is unbuilt. Existing task numbers are unchanged — this is appended.

*(Two statements above are dated, not current. `renvor-rs/renvor-site` **is no longer private** — it became public on 2026-08-14 — and it **is no longer unprotected**. Both were true when written on 2026-08-13.)*

- [X] T113 **LANDING PRE-DEPLOYMENT GATE**: Configure `renvor-rs/renvor-site` CI to consume its repository-local `.nvmrc`; run the applicable frozen-install, typecheck, production-build, accessibility, link-check, and container-scan checks; and configure protected `main` to require pull requests and the resulting required checks with no administrator bypass. Pin every third-party action to a full 40-character commit SHA with a version comment. Keep image publication, environments, credentials, deployment, server access, and DNS changes outside this task unless separately authorised. **Completed 2026-08-14** in `renvor-rs/renvor-site` via pull request **#4**, merged 2026-08-14T18:28:38Z as `d3575e5e8b5b8c16f21c6dde1578d8e9993422c4` from reviewed head `b6ed04d219c27a4f69526751c17a9db5aba2c575`; that merge commit is the current `main`. **Closed 2026-08-15 on read-only re-verification of the live repository, not on the pull request alone** — every element below was re-read from GitHub on that date. **Workflow `landing-ci`**: `.nvmrc` consumed via `node-version-file` in every Node job; `pnpm install --frozen-lockfile`; `pnpm run typecheck`; production build from cleaned `build/` and `.docusaurus/` with `index.html` and `404.html` asserted; accessibility run against the **built artifact** from the build job rather than a fresh local build; `lychee` link check over `build/**/*.html`; **dependency scan** (`trivy` filesystem, `HIGH,CRITICAL`, `exit-code: 1`); **container scan** (`trivy` image, `vuln,secret`, `HIGH,CRITICAL`, `exit-code: 1`) after a read-only, all-capabilities-dropped, non-root smoke test asserting `/health` 200, `/` 200, and a missing path 404; **two SBOMs** — a dependency SBOM and a runtime SBOM generated from the image with `syft` pinned to `v1.51.0`, both SPDX-JSON and uploaded with explicit retention and `if-no-files-found: error`. **All 9 actions pinned to full 40-character commit SHAs with trailing version comments** (verified mechanically: 9 of 9). **Protection on `main`**: pull request required, **5 required status checks** — `build`, `accessibility`, `links`, `dependencies`, `container` — `strict: true`, **`enforce_admins: true`** (no administrator bypass), conversation resolution required, force pushes blocked, deletions blocked. Top-level workflow permission is `contents: read` with **no job-level elevation anywhere**, no registry login, and no `packages: write`, so the task's exclusions hold: **no image was published, no environment, credential, deployment, server access, or DNS change was made.** **Boundary**: T113 proves the landing repository's own CI and protection. **It does not prove deployment readiness and deploys nothing** — T102, T106, T108, and T111 remain open. **Observed gaps, recorded rather than glossed**: `required_signatures` is **false** and `required_linear_history` is **false** on `renvor-site`, neither of which T113 required; the framework repository enforces both. Evidence §3aw

**Checkpoint**: ✅ **Met 2026-08-14, re-verified 2026-08-15.** The landing repository enforces its own declared toolchain and quality gates, rather than relying on a contributor remembering to run them locally.

---

## Hybrid source-control topology — infrastructure cutover gate (added 2026-08-14) — **SUPERSEDED 2026-08-15 by ADR-0006 D13; the gate below is CANCELLED**

> **T114 is deliberately not a checkbox.** *(Restructured 2026-08-15 by maintainer decision.)*
> Markdown checkboxes carry two states and this task needs a third, so writing it `- [X]` made
> a mechanical `grep` count a cancelled disaster-recovery gate as **completed** — the one place
> in this record where a machine check returned the wrong answer, about the highest-stakes
> subject in it. **T114 is now recorded below as an explicit non-checkbox
> `CANCELLED / NOT APPLICABLE` entry.** Its ID, its full original gate text, and its complete
> cancellation explanation are preserved unchanged. **T114 must never be counted as completed.**
> See "How to count the tasks in this file" at the top of this document.

**Purpose** *(as written 2026-08-14; superseded 2026-08-15)*: **This section is dated history. ADR-0006 D13 now makes public GitHub canonical for all four repositories, and T114 below is cancelled, not passed.** As originally written: ADR-0006 **D12** moves infrastructure source from GitHub to a private self-hosted GitLab instance at `gitlab.ahmedanbar.dev`, while the three application repositories stay on GitHub as public. That move puts infrastructure history on **the same VPS the infrastructure describes** — unavailable in exactly the recovery scenario ADR-0006 D9 depends on it for, which is the same reasoning that rejected the GitLab registry under T099. A destination existing is not a canonical repository. Existing task numbers are unchanged — this is appended.

**T114 — CANCELLED / NOT APPLICABLE** *(explicit non-checkbox record; not a completed task and not an open task)*: **GitLab canonical cutover abandoned (2026-08-15).** **THE CHECKED BOX MEANS CANCELLED, NOT COMPLETED. The gate as a whole never passed, and no restore was ever proven.** *(Corrected 2026-08-15: an earlier wording of this line said "none of the recovery requirements (a)–(e) below was satisfied", which contradicted the cancellation record immediately after it. Precisely: **(a) was performed and then undone** — an encrypted off-VPS backup was created on 2026-08-14 and the maintainer intentionally deleted it on 2026-08-15, so nothing from it is preserved; **(b), (c), (d), and (e) were never completed at all**. A gate passes only when every element holds simultaneously and stays held, so the gate did not pass.)* This gate was conditional on a private-GitLab canonical cutover that did not happen; ADR-0006 **D13** supersedes **D12** and makes public GitHub canonical for all four repositories, which removes the gate's subject rather than satisfying its requirements. *(Original gate text preserved verbatim below as the record of what was required.)* **GITLAB BACKUP/RESTORE AND INFRA-CANONICAL-CUTOVER GATE**: Before `renvor-infra` on `gitlab.ahmedanbar.dev` may be called canonical, and before any infrastructure content is pushed to it, establish and evidence **all** of the following. **(a) Encrypted off-VPS backup** of the GitLab application data and configuration, stored on a system that does not share the VPS's failure domain, with the encryption method recorded and **no key material committed or printed**. **(b) Exact-version isolated restore proof**: restore into an isolated environment running the **same GitLab version** as the source — CE 19.0.1 at the time of writing — with the restore performed from the backup alone and the isolation recorded. **(c) Matching repository refs and hashes**: every branch, tag, and commit SHA in the restored copy compared against the original and shown identical, with the comparison recorded rather than asserted. **(d) Retention, RPO, and RTO** recorded as explicit figures with the measurement that produced the RTO, not an estimate. **(e) Separate human approval** from Ahmed Anbar for the cutover itself, granted after (a)–(d) are evidenced and recorded as its own dated decision. Until every element passes, the GitLab project is a **destination only**, the GitHub `renvor-infra` repository stays preserved, private, and empty as a temporary recovery placeholder, and the local `infra` README and assets stay uncommitted. Keep server administration, Rails internals, credential extraction, registry use, deployment, and DNS changes outside this task unless separately authorised. **CANCELLATION RECORD (2026-08-15).** **(a)** An encrypted off-VPS backup **was created** on 2026-08-14. **(b)** The exact-version isolated restore proof **never completed**, and **no restore result was accepted**. **(c)** Matching restored repository refs and hashes were **never proven**. **(d)** **No RPO or RTO figure was measured**, and no GitLab RPO or RTO guarantee is claimed. **(e)** The separate human cutover approval was **never granted**, because the cutover was cancelled. The maintainer subsequently **abandoned the private-GitLab canonical-source plan**, and on **2026-08-15 Ahmed Anbar intentionally deleted** the local Phase 3 and Phase 4 GitLab backup and evidence directory — **the maintainer's local backup directory** *(absolute path withheld 2026-08-15)*. **None of those local backup artifacts is preserved.** This statement is scoped to that directory only and makes no claim about any unrelated backup held elsewhere. **Public GitHub now provides failure-domain separation for Git repository content**, which is what this gate existed to protect. **GitHub does not preserve GitLab-specific issues, variables, users, logs, packages, registry content, or other GitLab metadata**, and no claim is made that it does. **Self-hosted GitLab was not deleted, decommissioned, or modified.** **This entry is the cancellation of an obsolete conditional gate. It is not successful completion of its recovery requirements, and must never be counted as one.**

**Original checkpoint — SUPERSEDED 2026-08-15, never met**: ~~infrastructure history survives the loss of the machine it describes, proven by a restore rather than by a backup job's exit code.~~ **No restore was ever performed or proven, so this checkpoint was never satisfied and must not be read as a current statement that recovery was demonstrated.** It is retained as the record of what the cancelled gate would have required. Under ADR-0006 **D13** the checkpoint no longer applies: infrastructure history lives on public GitHub and in local clones rather than on the machine it describes, so the failure domain it guarded against is avoided by construction rather than closed by a restore proof. **That is a change of architecture, not evidence of recovery** — no Renvor GitLab restore, RPO, or RTO is claimed.
