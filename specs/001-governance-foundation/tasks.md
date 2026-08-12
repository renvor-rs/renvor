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

- [ ] T082 Complete `governance/phase-001-evidence.md` with one row per PLAN.md Phase 001 acceptance criterion and per SC-001 through SC-016, each carrying evidence link, command or action, platform, operator, date, and result
- [ ] T083 Record known limitations in `governance/phase-001-evidence.md` with named owner and target phase — including the FR-049 residual risk that verified-but-unreserved package names remain claimable, and the FR-061 obligation to revalidate MSRV 1.94.0 against real persistence dependencies before Phase 006
- [ ] T084 Create the recurring-obligations register in `governance/phase-001-evidence.md` with owner and first due date for each: the quarterly MSRV policy review (FR-060), the W-001 waiver expiry on 2027-02-11, and the pre-Phase-006 MSRV revalidation (analyze finding G6)
- [ ] T085 [P] Run the full quickstart.md gate sequence 0 through 8 and record every outcome in `governance/phase-001-evidence.md`
- [X] T086 [P] Work through `specs/001-governance-foundation/checklists/governance.md` (79 items) and record findings inline
- [ ] T087 Confirm no runtime framework capability was implemented and no unshipped capability is described as available anywhere in the repository, reviewing against the FR-047 exclusion list and FR-044; record the result (SC-013)
- [ ] T088 Confirm zero open blockers and obtain the independent requirements and security review required by PLAN.md §6.1 step 10

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
- [ ] T095 **RELEASE-HONESTY GATE**: add a prominent development-status notice to the V7 landing page and re-word every present-tense capability claim to state its actual status (blocks any public landing deployment; `PLAN.md` §26.6)
- [ ] T096 Remove or clearly mark the `renover new` / `renover add` installation commands on the V7 landing page until the referenced crates are publicly installable (depends on T095)
- [ ] T097 Repoint every V7 CTA at a resolving destination — `crates.io/crates/renvor` is HTTP 404, `docs.renvor.dev/getting-started` does not resolve, `renvor-rs/renvor` is empty — or remove the CTA (depends on T095)
- [ ] T098 Decide and record the **website-code licence** and the **brand-asset usage terms** for V7; brand assets are NOT covered by the framework `MIT OR Apache-2.0` grant. **Blocks creation of `renvor-rs/renvor-landing`**
- [ ] T099 Decide and record the **container registry** — GitHub Container Registry versus the GitLab registry already on the VPS — including the credential model. **Blocks creation of the private repositories** (ADR-0006 unresolved question 1)
- [ ] T100 Add a `.nvmrc` pinning **Node 24** to the landing migration checklist; the framework repository pins Node 22 and the two must not be conflated (depends on T098)
- [ ] T101 Verify CSP compatibility with the V7 landing implementation (GSAP, self-hosted variable fonts) before Cloudflare security headers are enabled (ADR-0006 unresolved question 4)
- [ ] T102 Re-verify the server audit immediately before any deployment; the host is shared with unrelated production workloads and the 2026-08-11 facts can go stale (ADR-0006 additional gate)

**Checkpoint**: The topology, hosting architecture, and migration plan are recorded and reviewable. No repository has been created, no infrastructure provisioned, no DNS changed, and nothing deployed.

---

## Phase 11: Governance review findings (added 2026-08-12)

**Purpose**: T086 reviewed all 79 governance checklist items and found two genuine specification gaps. They are recorded as tasks rather than checked off prematurely or resolved by weakening the requirement.

- [X] T103 Define a concrete evidence retention period. FR-046 requires build evidence to be retained "for a stated period" but no duration exists in the specification, the contracts, or the release documentation. State the duration, its start event, and where retained evidence lives (governance checklist CHK048)
- [X] T104 Define a security-advisory response window. FR-010 requires the policy to state how advisories are handled but sets no triage or response deadline, so an advisory could remain unactioned indefinitely without violating any rule. This is distinct from the SECURITY.md windows, which govern inbound reports rather than advisories against dependencies. **Blocks acceptance of ADR-0003** (governance checklist CHK050)
- [ ] T105 Decide whether the `www.renvor.dev` permanent redirect is served by Cloudflare or by Traefik, and record it. **Blocks acceptance of ADR-0006** (ADR-0006 unresolved question 2)
- [ ] T106 Record the maintainer ruling on the shared server's absent backups — whether the gap blocks Renvor deployment, given that both Renvor properties are stateless while five unrelated production namespaces are not. **Blocks acceptance of ADR-0006** (ADR-0006 unresolved question 3)

**Checkpoint**: Every governance checklist item has a defensible recorded outcome, and every discovered gap is either corrected or tracked as an explicit open task.

---

## Phase 12: Documentation dependency advisories (added 2026-08-12)

**Purpose**: five advisories were raised against `docs/package-lock.json` on 2026-08-11. Two were closed by a tested override at T107; three have no compatible fix and are tracked here rather than silently accepted. Existing task numbers are unchanged — these are appended.

- [X] T107 Close `GHSA-5c6j-r48x-rmvq` (high, RCE, CVSS 8.1) and `GHSA-qj8w-gfj5-8c6v` (moderate) by overriding `serialize-javascript` to `^7.1.0` in `docs/package.json`. Docusaurus 3.10.2 pins `copy-webpack-plugin ^11` and `css-minimizer-webpack-plugin ^5`, both requiring `serialize-javascript ^6`, so no ordinary compatible update exists. Prove the override with a frozen install, a production build, a link check, and a CommonJS load test
- [ ] T108 **DOCUMENTATION DEPLOYMENT GATE — resolve `image-size`**: `GHSA-w3rx-r6r6-pgpr` and `GHSA-5p2g-fcmc-qvqq` (both high, CVSS 7.5, infinite-loop DoS in the ICNS, JXL, and HEIF parsers). **No fixed version exists** — 2.0.2 is simultaneously the affected and the latest published version. Reached only through `@docusaurus/mdx-loader`. Resolve by upstream fix, by a Docusaurus release that drops or replaces the dependency, or by a reviewed removal or isolation. **Blocks public documentation deployment** while open (dependency advisory policy §6, §7). Owner Ahmed Anbar; reassess **2026-08-26**
- [ ] T109 Reassess `GHSA-w5hq-g745-h8pq` (moderate, `uuid` < 11.1.1). Not reachable today: `sockjs` calls only `uuid.v4()` with no `buf` argument while the advisory affects v3/v5/v6 **with** `buf`, and `sockjs` arrives via `webpack-dev-server`, which runs only for `docusaurus start` and never in the production build. `sockjs` 0.3.24 is the latest release and pins `uuid ^8.3.2`, so no compatible update exists. Do **not** force a three-major override into a path CI never exercises. Owner Ahmed Anbar; reassess **2026-09-11**, or immediately if `sockjs` ships a fix or the dev server enters a deployed path

**Checkpoint**: every advisory has a dated record, a named owner, and either a proven fix or an explicit gate. None is silently ignored.
