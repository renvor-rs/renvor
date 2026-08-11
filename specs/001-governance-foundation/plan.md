# Implementation Plan: Governance, Names, Toolchain, and Repository Security Foundation

**Branch**: `001-governance-foundation` | **Date**: 2026-08-11 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `/specs/001-governance-foundation/spec.md`

## Summary

Establish a Renvor repository that can be trusted, audited, and reproduced before any runtime code exists: verified public names, ratified governance and licensing, a Rust 2024 workspace that verifies itself from a clean checkout, a public repository with every free platform security control enabled, a recorded documentation platform decision, and a release path rehearsed end-to-end without publishing anything.

The technical approach is deliberately package-first and thin. One workspace with one placeholder facade crate and one `xtask` runner; one configuration file (`deny.toml`) enforcing the entire dependency and licence policy; four workflows with read-only default permissions and SHA-pinned actions; one documentation site; and one evidence pack mapping every acceptance criterion to a dated artifact. Nothing in this phase ships runtime capability.

Phase 0 research re-ran PLAN.md §8.1's version snapshot against primary sources and confirmed it, while surfacing three issues the plan handles rather than inherits: a release-count MSRV window would have expired on 2026-08-20 (resolved by the maintainer decision to declare a fixed floor at 1.94.0), a virtual workspace does not inherit resolver 3 from edition 2024, and the existing repository needs an audited cleanup before it can be made public. See [research.md](./research.md).

**Execution boundary**: this document plans only. No code is implemented, no package is published, nothing is pushed or committed, and no name is selected as a substitute for an unavailable one. Unconfirmed public names remain hard blockers (FR-003, FR-004).

## Technical Context

**Language/Version**: Rust, 2024 edition. Toolchain matrix — declared MSRV **1.94.0** (released 2026-03-05), pinned as a **fixed support floor**, plus the current stable channel (**1.97.1** at time of writing). A newer Rust stable does not move the floor. Cargo **resolver 3**, declared explicitly in the virtual workspace manifest rather than inherited. Secondary toolchain: Node.js active LTS, for the documentation site only.

**Primary Dependencies**: None at runtime — this phase ships no runtime code. Tooling dependencies: `cargo-deny` (licence, advisory, ban, and source policy), `gitleaks` (pre-push history secret scan), `lychee` (link checking), `clippy-sarif` + `sarif-fmt` (lint findings into code scanning), Docusaurus 3.10.x (documentation site).

**Storage**: N/A — no persisted application data. The durable artifacts of this phase are records: the name availability record, decision records, the waiver ledger, and the phase completion evidence pack, all stored as version-controlled Markdown.

**Testing**: `cargo test --workspace --all-features` (placeholder suites only in this phase), documentation build and link checking, `cargo package --list` file-list inspection, and `cargo publish --dry-run` as the release rehearsal. All are driven through one entry point, `cargo xtask verify`.

**Target Platform**: Linux (`ubuntu-latest`) is the primary and only required verification platform for this phase, because it contains no platform-sensitive code. macOS and Windows enter the matrix in the phase that first introduces platform-sensitive behaviour, per PLAN.md §17.2. Recorded in the support policy contract so the commitment is explicit rather than implied.

**Project Type**: Multi-crate Rust workspace with an adjacent documentation site — repository infrastructure and governance, not an application.

**Performance Goals**: Not applicable to shipped behaviour. One operational target: the full verification sequence completes in under 10 minutes on a clean checkout on `ubuntu-latest`, so that the required-checks gate does not make small changes expensive.

**Constraints**: The repository must be cleaned and audited before it is created (Pre-Push Repository Cleanup). It is public from creation, so nothing may be pushed before names are confirmed, both licence texts exist, and the security contact is live (FR-052). No package may be published to the registry (FR-038) — and because nothing is published, trusted publishing cannot be configured in this phase at all, so the release path stays a non-publishing dry run by necessity as well as by choice (research Finding 2). No check may be skipped when its toolchain is absent (FR-023). No account, including administrators, may bypass branch protection (FR-027). No long-lived registry credential may exist anywhere (FR-033). An unavailable name halts the phase; no substitute is ever selected automatically (FR-003).

**Scale/Scope**: One workspace, two workspace members (`renvor` facade placeholder, `xtask` runner), four accepted decision records, four workflows, six governance documents, one documentation site skeleton, and one evidence pack covering 56 functional requirements and 15 success criteria.

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

**Initial evaluation (pre-research): PASS with one item to resolve in research.**
**Post-design re-evaluation (post-Phase 1): PASS. One justified complexity, recorded below.**

| Principle | Applies | Assessment |
|---|---|---|
| I. Cohesive, Explicit Rust | Partially | No runtime code, and no hidden machinery introduced. The one piece of tooling (`xtask`) is ordinary, readable Rust. **PASS** |
| II. Transport-Independent Core | No | No transports exist yet. |
| III. Package-First Boundaries | Yes | Every capability is a maintained package: `cargo-deny`, `gitleaks`, `lychee`, Docusaurus, `sarif-rs`. The only custom code is the `xtask` orchestrator, justified in Complexity Tracking. **PASS** |
| IV. Deterministic Lifecycle | No | No runtime lifecycle in this phase. |
| V. Contract-First Compatibility | Yes | Public identity, support policy, verification sequence, and package metadata are all defined as explicit contracts in `contracts/`. **PASS** |
| VI. Security, Privacy, Fail-Closed | Yes | Fail-closed verification (FR-023, FR-055); deny-by-default licence policy; no branch-protection bypass; push protection blocks secrets before they land; pre-push history scan. **PASS** |
| VII. Deterministic and Safe Generation | No | No generation in this phase. |
| VIII. Feature and Platform Isolation | Yes | No feature flags exist yet; the workspace is structured so optional capability can be isolated later. **PASS** |
| IX. Real-Boundary Verification | Yes | Verification runs against a real clean checkout, a real `cargo package`, a real documentation build, and a real link check — not mocks. **PASS** |
| X. Documentation Is a Release Artifact | Yes | Documentation platform selected on recorded evidence; placeholder set builds and link-checks in the required sequence; prose and API docs must cross-link (FR-056). **PASS** |
| XI. Supply-Chain and Release Integrity | Yes | Committed lockfiles; SHA-pinned actions; least-privilege permissions; SBOM and provenance wired; trusted publishing designed with a documented, revocable bootstrap; no long-lived credentials. **PASS** |
| XII. Simplicity, Phasing, Honest Scope | Yes | Development-status notice required until first release (FR-053); no capability claimed that does not exist (FR-044); scope explicitly bounded by FR-047. **PASS** |
| XIII. Independent Installable Packages | No | The package ecosystem begins at product 4.0. |
| Architecture constraint: Rust 2024, resolver 3, explicit MSRV in CI | Yes | Satisfied, with the virtual-workspace resolver trap handled explicitly (research Finding 1). **PASS** |
| Development and Phase Workflow | Yes | Specify → clarify → plan executed in order; decisions recorded as ADRs requiring review before acceptance (FR-013). **PASS** |

**Item resolved**: the constitution requires "an explicit MSRV tested in continuous integration." Research Finding 0 established that a release-count window would have been violated by the calendar on 2026-08-20. The maintainer decision of 2026-08-11 resolves this by declaring the MSRV a **fixed, explicitly versioned floor** at 1.94.0, removing release count from the policy entirely. ADR-0003 now records that decision rather than making it. The gate passes: the MSRV is explicit, tested at the exact version, and governed by a written change process with a six-month dwell time.

**Security controls claimed for this phase** (constitution "Security and Privacy Requirements" requires each spec to record which controls apply): secrets kept out of repository and history; least-privilege automation; supply-chain review of dependencies, licences, and advisories; SBOM and provenance; fail-closed verification. Abuse cases considered: credential committed to history before the first public push; compromised third-party action executing with repository write; a dependency introduced under an incompatible licence; a defective release published irrevocably.

## Project Structure

### Documentation (this feature)

```text
specs/001-governance-foundation/
├── plan.md              # This file
├── research.md          # Phase 0 output — version and capability verification
├── data-model.md        # Phase 1 output — record structures and their lifecycles
├── quickstart.md        # Phase 1 output — how to validate the phase end to end
├── contracts/           # Phase 1 output
│   ├── public-identity.md
│   ├── support-policy.md
│   ├── verification-sequence.md
│   └── package-metadata.md
├── checklists/
│   ├── requirements.md  # Spec quality checklist (24/24 passing)
│   └── governance.md    # Formal reviewer checklist, 79 items across 10 domains
└── tasks.md             # Phase 2 output (/speckit-tasks), 88 tasks
```

### Source Code (repository root)

```text
Cargo.toml                       # Virtual workspace; resolver = "3" stated explicitly
Cargo.lock                       # Committed
rust-toolchain.toml              # Pins the toolchain for contributors and CI
rustfmt.toml
clippy.toml
deny.toml                        # Licence, advisory, ban, and source policy — FR-010
.gitleaks.toml                   # Narrow, individually justified false-positive allowlist
.gitignore                       # Corrected: build output, editor, OS, env, secrets
.gitattributes
.nvmrc                           # Node LTS pin for the documentation site

LICENSE-APACHE                   # FR-008
LICENSE-MIT                      # FR-008
README.md                        # Development-status notice — FR-053
SECURITY.md                      # Private reporting path — FR-011
CONTRIBUTING.md                  # Contribution terms and dependency policy — FR-007
CODE_OF_CONDUCT.md
GOVERNANCE.md                    # Decision authority — FR-007
SUPPORT.md                       # Support and version policy — FR-007
RELEASING.md                     # Publish order, bootstrap, yank-and-replace — FR-041
CHANGELOG.md

crates/
└── renvor/                      # Placeholder facade crate — FR-026, research Finding 9
    ├── Cargo.toml               # Complete publishable metadata — FR-040
    ├── README.md
    └── src/lib.rs               # Crate docs and a version constant only

xtask/
├── Cargo.toml                   # publish = false
└── src/main.rs                  # Single verification entry point — FR-055

decisions/                       # Architecture decision records — FR-013, FR-014
├── 0000-template.md
├── 0001-public-naming-and-namespace.md
├── 0002-workspace-boundaries-and-facade-stability.md
├── 0003-msrv-toolchain-and-dependency-policy.md
└── 0004-documentation-platform-and-versioning.md

governance/
├── name-availability.md         # Dated evidence record — FR-001, FR-002
├── waivers.md                   # Time-bounded exception ledger — FR-015
└── phase-001-evidence.md        # Completion record — FR-042

docs/                            # Docusaurus 3.10.x site
├── package.json
├── package-lock.json            # Committed — FR-054
├── docusaurus.config.ts
└── docs/                        # Placeholder documentation set — FR-037

.github/
├── workflows/
│   ├── ci.yml                   # fmt, clippy, test, doc — MSRV and stable
│   ├── security.yml             # cargo-deny, dependency review, CodeQL, clippy SARIF
│   ├── docs.yml                 # Documentation build and link check
│   └── release-dry-run.yml      # Package, inspect, publish --dry-run — FR-038
├── ISSUE_TEMPLATE/
│   ├── bug_report.yml
│   ├── feature_request.yml
│   └── config.yml               # Routes security reports to the private path
├── PULL_REQUEST_TEMPLATE.md
├── RELEASE_TEMPLATE.md
└── dependabot.yml               # cargo, github-actions, npm ecosystems
```

**Structure Decision**: A virtual workspace at the repository root with two members under `crates/` and `xtask/`. The root is deliberately *not* a package: a facade at the root blurs crate boundaries and complicates publishing, and the cost — that resolver 3 is not inherited — is paid once by stating `resolver = "3"` explicitly (research Finding 1).

Governance records live in `governance/` and decisions in `decisions/`, both as version-controlled Markdown, so that evidence is reviewed through the same protected pull-request path as code. The documentation site is a sibling directory rather than a workspace member, because it belongs to a different toolchain; `xtask` is what bridges them so contributors still have one command.

## Pre-Push Repository Cleanup

**Blocking predecessor to repository creation and the first content push.** The Q4 decision makes the first push immediately world-visible, and removing a file from a public repository does not remove it from clones, forks, or caches. Everything here happens locally, before a remote exists.

Audited state as of 2026-08-11 is recorded in research Finding 11. The short version: nothing sensitive is committed, `.idea/` is staged but never committed, and the 66 MB `.git` is unreachable objects that `git push` would never transmit. This is a tidy-up, not a rescue.

**Ordering is load-bearing.** Unstage before pruning, so the unstaged blobs become unreachable and are actually collected. Scan after pruning, so the scan covers the final state. Rename the branch before a remote exists, so protection is never applied to a name that then changes.

| Stage | Action | Verifies |
|---|---|---|
| 0 | **Decide what is public.** Record an explicit include-or-exclude decision for `PLAN.md`, the two legacy root planning documents, `Branding/`, `specs/`, and the `.claude/` + `.specify/` tooling. Silence is not a decision. | Nothing is published by accident |
| 1 | **Correct the ignore rules.** Cover Rust build output, Node output and `node_modules/`, editor state (`.idea/`, `.vscode/`), OS artifacts (`.DS_Store`), environment and credential files, agent tool state (`.claude-flow/`, `.playwright-mcp/`, `.remember/`), and anything Stage 0 excluded. | FR-024 |
| 2 | **Unstage what must not ship.** The four `.idea/` files are in the index but in no commit, so `git restore --staged` suffices — no history rewrite is needed or wanted. | Index matches Stage 0's decisions |
| 3 | **Prune unreachable objects.** Expire unreachable reflog entries and garbage-collect. Confirm `git count-objects -vH` drops from ~65.8 MiB to the size of the reachable set. | Local state is unambiguous |
| 4 | **Scan tree and history.** `gitleaks` over the full history and working tree. Record date, tool version, commit range, and finding count. | FR-025, SC-005 |
| 5 | **Rename the default branch** `master` → `main`, before any remote exists. | FR-027, research Finding 10 |
| 6 | **Review the exact publish set** file by file — 30 files in HEAD today, small enough to read. Confirm `git status --porcelain` is empty. | FR-052, SC-004 |
| 7 | **Gate.** Only now create the public repository, apply protections and scanning controls, then push. | FR-030, FR-052 |

**The scan must be current at the moment of the push.** Stage 4 runs early, but roughly twenty file-creating tasks follow it before the first push — manifests, source, policy configuration, six governance documents, four workflows. A scan performed before that content existed says nothing about the content actually being published, so it is re-run immediately before the push as a distinct, separately recorded gate (FR-025).

**No history rewrite is planned.** The audit found nothing sensitive in HEAD and nothing large in reachable history, so `git filter-repo` would add risk without removing exposure. If Stage 4 finds a credential, that changes — and the remedy is rotate, revoke, purge, record, then re-scan, in that order, before any push.

**Decisions Stage 0 must record**, none of which may be resolved silently:

- **`PLAN.md`** — currently committed, contains the full 30-phase roadmap. Publishing it is defensible and arguably good practice, but it should be a recorded choice.
- **The two legacy root planning documents** — PLAN.md §1 designates them research references that may be archived after Phase 001. Publish, archive, or exclude.
- **`Branding/`** — brand assets are explicitly *not* covered by the `MIT OR Apache-2.0` grant, which covers source and documentation. If they ship, they need their own terms.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `xtask` custom runner (constitution III prefers packages over custom infrastructure) | FR-055 requires the verification sequence to detect a missing toolchain and fail with an actionable message, across a mixed Rust + Node toolchain. No off-the-shelf package does this for this combination. | `make` adds Windows friction for a matrix that will include Windows; `just` requires installing a tool before you can verify anything, which is the wrong first experience; shell steps duplicated into workflows are the drift that produces "skipped check reported as pass" — the exact failure FR-023 exists to prevent. `xtask` is an established Rust convention, ships no runtime capability, and is one file. |

## Phase Status

- [x] Phase 0: Research complete — [research.md](./research.md), 12 findings, all version claims verified against primary sources, repository state audited
- [x] Phase 1: Design complete — [data-model.md](./data-model.md), [contracts/](./contracts/), [quickstart.md](./quickstart.md)
- [x] Constitution check re-evaluated post-design — PASS with one recorded complexity
- [x] Phase 2: Tasks generated by `/speckit-tasks` — [tasks.md](./tasks.md), 88 tasks across 9 phases, 4 blocking gates
- [x] Cross-artifact analysis — four `/speckit-analyze` passes; 0 critical and 0 high findings outstanding, remediation logs in [tasks.md](./tasks.md)
- [ ] Implementation — `/speckit-implement`, not started
