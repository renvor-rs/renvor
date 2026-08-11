# Phase 0 Research: Governance, Names, Toolchain, and Repository Security Foundation

**Feature**: `specs/001-governance-foundation` | **Date**: 2026-08-11 | **Verified against**: primary sources listed in References

All version and capability claims below were checked against primary sources on 2026-08-11. PLAN.md §8.1 requires Phase 001 to re-run these checks rather than trust the planning snapshot; this document is that re-run.

---

## Finding 0 — MSRV is a fixed floor at 1.94.0 *(resolved by maintainer decision, 2026-08-11)*

**Verified**: Current stable Rust is **1.97.1**, released 2026-07-16. Beta is **1.98.0**, promoting to stable on **2026-08-20**. Rust 1.94.0 was released 2026-03-05 (1.94.1 on 2026-03-26). PLAN.md §8.1's claim of "stable 1.97.1" is correct.

**The problem this research surfaced**: PLAN.md proposes MSRV 1.94.0 "because of the database stack." Expressed as a release-count window, 1.94.0 sat three releases behind stable — a defensible N-3 — but would become N-4 on 2026-08-20 with no code change involved. Any policy phrased as "at most N releases behind stable" would have been violated by the calendar before Phase 001 finished. Compounding it, the stated justification does not apply to Phase 001, which has no database dependency.

**Decision (maintainer, 2026-08-11)**: MSRV is **1.94.0**, declared as a **fixed, explicitly versioned support floor** — not N-3, not N-4, not any rolling offset. The 2026-08-20 expiry problem is not solved so much as dissolved: "releases behind stable" is no longer an input to the policy, so a newer Rust stable cannot invalidate the floor.

The governing rules:

| Rule | Effect |
|---|---|
| Fixed floor, explicitly versioned | A new Rust stable release changes nothing |
| CI tests the exact floor **and** current stable | The floor is pinned to 1.94.0; the stable job tracks the channel |
| Raised only in a planned minor or major Renvor release | No drive-by bumps in patch releases |
| Raise requires an accepted ADR naming a concrete dependency, language, or security requirement | "Newer is better" is not a justification |
| Documented in support policy, changelog, and release notes | Three places, so consumers cannot miss it |
| Minimum six-month dwell time per declared floor | Makes "fixed" a real promise rather than a label |
| Quarterly policy review | Records a conclusion; does **not** by itself change `rust-version` |
| Revalidate 1.94.0 against real persistence dependencies before Phase 006 | Closes the gap where PLAN.md's justification referenced a stack that does not exist yet |

**Why this framing is the right one**: the MSRV is a public support commitment under constitution principle V. A rolling window makes that commitment a function of someone else's release calendar; a fixed floor makes it a function of a recorded decision. The six-month dwell time is what stops "fixed" from degrading into "changed whenever an ADR is convenient."

**Alternatives considered and rejected**: pinning MSRV to current stable (every Rust release becomes a breaking change for consumers); keeping a release-count window and widening it to N-4 (the window widens again at every release unless anchored); leaving MSRV undeclared (the constitution requires an explicit MSRV tested in CI).

**Carried forward**: the Phase 006 revalidation is a scheduled obligation with a named owner (FR-061), not an aspiration.

---

## Finding 1 — A virtual workspace does not inherit resolver 3 from edition 2024

**Verified**: Resolver 3 requires Rust 1.84+, and is the **default for `edition = "2024"`**. Its effect is that `resolver.incompatible-rust-versions` defaults to `fallback`, so dependency resolution prefers versions compatible with the declared `rust-version` instead of the newest.

**The trap**: the resolver default is derived from a *package's* edition. Renvor's root manifest is a **virtual workspace** — it has `[workspace]` but no `[package]`, therefore no edition, therefore no inherited resolver. A virtual workspace silently falls back to resolver 1 unless `resolver = "3"` is stated explicitly in `[workspace]`.

**Decision**: State `resolver = "3"` explicitly in the root `[workspace]` table, and add a verification assertion that resolution is MSRV-aware rather than assuming it.

**Why it matters here**: MSRV-aware resolution is what makes the declared MSRV real. Without it, `cargo update` will happily pull a dependency requiring a newer Rust than the declared minimum, and the MSRV CI job fails for reasons unrelated to Renvor's own code.

**Alternatives considered**: making the root a real package to inherit the default (rejected — a facade package at the root confuses crate boundaries and publishing); relying on the edition-2024 default (rejected — does not apply to virtual workspaces).

---

## Finding 2 — Trusted publishing cannot be configured until after a manual first publish

**Verified**: crates.io trusted publishing exchanges a GitHub OIDC identity token for a short-lived crates.io token via `rust-lang/crates-io-auth-action`, requires `id-token: write` permission, writes the credential to `CARGO_REGISTRY_TOKEN`, and auto-revokes it in the action's post step. It is currently GitHub Actions only. **A crate must already exist on the registry before trusted publishing can be linked to it** — so the first release of any new crate name requires a manually-created token.

**Decision**: Phase 001 **builds** the release workflow and **documents** the bootstrap procedure, but does not execute either against the live registry. The rehearsal is `cargo package` plus `cargo publish --dry-run` only.

The bootstrap procedure recorded in `RELEASING.md` must specify: a least-scope, single-crate publish token; created immediately before use; never written to the repository or to Actions secrets; revoked immediately after the first publish verifies; revocation recorded with a timestamp in the waiver/evidence ledger. Trusted publishing is then configured and every subsequent release uses it.

**Interaction with the Q1 clarification**: because the project decided to verify names rather than reserve them by publishing, no crate exists on the registry at the end of Phase 001, so trusted publishing cannot be configured in this phase at all. This is expected, not a gap — the workflow is written and dry-run-verified now, and activated in the first publishing phase.

**Alternatives considered**: publishing placeholder versions in Phase 001 to enable trusted publishing immediately (rejected by the Q1 clarification — permanent, immutable registry entries for names not yet backed by code); storing a long-lived registry token (prohibited by constitution principle XI).

---

## Finding 3 — Every required repository control is free on a public repository; zero cost waivers are needed

**Verified** for public repositories:

| Control | Availability | Note |
|---|---|---|
| Secret scanning + push protection | Free, no licence, no configuration | Push protection blocks the secret *before* it lands |
| Code scanning (CodeQL) | Free; **Rust GA since 2025-10-14** | Was public preview from 2025-06-30; supports default and advanced setup |
| Dependency graph, Dependabot alerts, dependency review | Free | Dependency review runs as a PR gate |
| Artifact attestations / build provenance | Free **on public repos only** for Free/Pro/Team plans | Private/internal require Enterprise Cloud |

**Decision**: enable all four. The Q4 clarification (public repository from creation) is what makes this achievable without spending money or writing waivers — SC-008's "0 waived for cost" is satisfiable exactly because of that choice. Artifact attestations in particular are *only* free because the repository is public.

**Complement, not substitute**: CodeQL's Rust queries target injection, cryptographic misuse, and unsafe data handling. They do not cover lint-class correctness findings. Pipe `cargo clippy --message-format=json` through `clippy-sarif` and upload the SARIF to code scanning so both classes of finding land in one review surface.

**Residual gap**: GitHub's secret scanning begins working when the repository exists and receives pushes. FR-052 gates the first push on names, licences, and the security contact being correct — so the *pre-push* history must be scanned by something else. See Finding 6.

**Alternatives considered**: private repository plus paid GitHub Secret Protection / Code Security (rejected by Q4 — spends money for a window that closes at phase end); deferring scanning to Phase 002 (rejected — credential exposure is this phase's highest-consequence failure).

---

## Finding 4 — cargo-deny is the single enforcement point for the dependency and licence policy

**Verified**: cargo-deny performs four checks in one tool — **licences** (acceptable terms), **advisories** (RustSec database), **bans** (denied crates, duplicate versions), and **sources** (only trusted registries). Actively maintained by Embark Studios; configured via `deny.toml`; has an official `cargo-deny-action` for CI and a pre-commit hook.

**Decision**: `deny.toml` at the repository root is the machine-readable form of FR-010, with the prose policy in `CONTRIBUTING.md` pointing at it as authoritative. Proposed initial configuration:

- **Allowed**: `MIT`, `Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `Unicode-3.0`, `Zlib`
- **Requires written review**: `MPL-2.0`, `CDDL-1.0`, dual licences with an unfamiliar half, any unrecognised expression
- **Denied**: `GPL-*`, `AGPL-*`, `LGPL-*`, and any crate with no licence expression — a copyleft dependency is incompatible with the `MIT OR Apache-2.0` promise made to users of generated code
- **Sources**: crates.io only; no git or path dependencies in any publishable crate
- **Bans**: duplicate major versions are a warning in Phase 001 and become an error once the runtime dependency set exists

**Rationale**: one tool, one config, one CI job covering four of the constitution's principle XI obligations. The alternative is three tools with three failure modes.

**Alternatives considered**: `cargo-audit` alone (advisories only — no licence, bans, or source control); manual dependency review (not repeatable, not enforceable, degrades silently); `cargo-vet` (valuable for supply-chain *auditing* at scale, but it is a review-tracking system that needs an established audit corpus — premature for a project with one placeholder crate).

---

## Finding 5 — Docusaurus 3.10.2, with local search rather than a hosted index

**Verified**: Docusaurus latest is **3.10.2** (released 2026-07-10). Requires **Node.js 18+**; LTS recommended. Multi-version documentation is a first-class feature via the `versioned_docs` directory structure.

**Decisions**:

1. **Version**: pin 3.10.x with `package-lock.json` committed, per FR-054 and the constitution's lockfile rule.
2. **Node**: pin the active LTS line via `.nvmrc` and reference the same value in CI, so contributors and automation agree.
3. **Search**: start with the **local/offline search plugin**, not Algolia DocSearch. DocSearch requires an external service, an application/approval step, and a crawler pointed at a live public site — none of which exist during Phase 001, and all of which make the documentation build non-reproducible and dependent on a third party. Local search keeps FR-037 verifiable from a clean checkout. Revisit DocSearch once the site is publicly hosted and its index is worth the dependency.
4. **Link checking**: **lychee** — a Rust tool, so it needs no additional toolchain beyond the one already pinned, runs offline against the built output, and has a maintained GitHub Action.
5. **rustdoc coexistence**: `cargo doc` output is a separate artifact. FR-056 requires the prose site and the API reference to cross-link and agree on version. Phase 001 wires the cross-link and the version stamp; it does not yet have public API to document.

**Alternatives considered**: mdBook (no built-in versioning — the requirement that decided it); MkDocs + Material (adds a Python toolchain the program does not otherwise need); Zola (versioning, search, and docs navigation all hand-built). All three recorded in ADR-0004 with these reasons.

---

## Finding 6 — The pre-push secret scan needs its own tool

**Context**: FR-025 requires the tree *and history* to be provably free of credentials, with a dated recorded result. FR-052 gates the first public push. GitHub's own secret scanning cannot help before that push exists.

**Decision**: run **gitleaks** over the full history locally and in CI before the first push, and keep it in the CI matrix afterwards as defence in depth alongside GitHub's scanning. Record the scan date, tool version, commit range, and finding count in the evidence pack.

**Note on the existing repository**: the working tree currently contains `.DS_Store`, an `.idea/` directory, a `.playwright-mcp/` directory, and a `Branding/` directory, and the current `.gitignore` covers some but not all of these. The single existing commit must be scanned before it becomes public, and the ignore rules corrected first, or the first push publishes editor and OS artefacts.

**Alternatives considered**: trusting GitHub's post-push historical scan (rejected — it detects the leak after the world can see it); manual review (rejected — not repeatable, and history review does not scale).

---

## Finding 7 — Pin actions by commit SHA, and let Dependabot maintain them

**Decision**: every third-party action is referenced by full 40-character commit SHA with a trailing `# vX.Y.Z` comment. `.github/dependabot.yml` enables the `github-actions` ecosystem so updates arrive as reviewable pull requests with the version comment maintained automatically.

`permissions: contents: read` is declared at workflow top level in every workflow; individual jobs elevate narrowly — `id-token: write` only in the publishing job, `security-events: write` only in the SARIF upload job.

**Note on provenance tooling**: `actions/attest-build-provenance` is, from v4, a thin wrapper over `actions/attest`, and new implementations are directed to use `actions/attest` directly. Phase 001 wires `actions/attest`.

**Alternatives considered**: floating major tags such as `@v4` (rejected — mutable; a compromised tag silently changes what runs with repository permissions); vendoring action source (rejected — high maintenance for little gain over SHA pinning).

---

## Finding 8 — `cargo xtask` is the single verification entry point

**Context**: FR-022 requires one documented verification sequence; FR-023 requires a check that cannot run to fail rather than skip; FR-055 requires a missing toolchain to produce an actionable message. Phase 001 mixes a Rust toolchain and a Node toolchain, so "just run cargo test" is not sufficient.

**Decision**: `cargo xtask verify` — an unpublished workspace member (`publish = false`) that orchestrates formatting, lint, tests, doc build, `cargo deny`, the documentation site build, and link checking. It probes for each required toolchain first and exits non-zero with a message naming what is missing and how to install it. CI calls the same entry point, so local and CI behaviour cannot drift.

**Alternatives considered**: a `Makefile` (friction on Windows, which the support matrix will eventually include); `just` (requires contributors to install another tool before they can verify anything); shell scripts duplicated into workflow steps (the duplication *is* the drift — CI and local silently diverge, which is how a "skipped check reported as pass" happens in practice).

**Complexity note**: `xtask` is the closest thing in this plan to custom infrastructure, which constitution principle III scrutinises. It is a widely-used Rust ecosystem convention rather than a new abstraction, it ships no runtime capability, and it exists specifically to satisfy a stated requirement (FR-055) that no off-the-shelf package satisfies. Recorded in the plan's Complexity Tracking table.

---

## Finding 9 — Rehearse packaging with the real facade crate, not a throwaway

**Decision**: the placeholder crate required by FR-026 is `crates/renvor/` — the real facade crate name — containing only crate-level documentation and a version constant. Its manifest carries the complete publishable metadata FR-040 requires. The rehearsal runs `cargo package -p renvor --list` (inspect exact shipped file list), `cargo package -p renvor` (build the artifact), and `cargo publish --dry-run -p renvor` (full verification without publication).

**Rationale**: rehearsing with a throwaway crate validates metadata that will never ship. Rehearsing with the facade crate proves the thing that actually has to work, and surfaces metadata and file-inclusion mistakes while they are still free to fix.

**Safety**: `cargo publish --dry-run` performs no network write. The release workflow additionally runs on a tag trigger it will not receive during Phase 001, and the publishing job's credential step is the only path to a real publish.

**Alternatives considered**: a `renvor-meta` throwaway crate (rejected — proves less); no placeholder at all (rejected — then the packaging path is untested and the Phase 001 acceptance criterion cannot be met).

---

## Finding 10 — The default branch must be renamed before the first push

**Observation**: the working repository is on `master`; PLAN.md and the program treat `main` as the default. The Q4 decision makes the repository public from creation, so the branch name becomes publicly visible at the first push and is awkward to change afterwards (open pull requests, external clones, documentation links).

**Decision**: rename to `main` locally before the repository is created, create the public repository with `main` as its default, then apply protection. Ordering matters: protection applied to a branch that is later renamed does not follow the rename cleanly.

---

## Verification sequence design (satisfies FR-022, FR-023, FR-055)

| Step | Tool | Required toolchain | On missing toolchain |
|---|---|---|---|
| Formatting | `cargo fmt --all --check` | Rust (pinned) | Fail: name the missing component |
| Lint | `cargo clippy --all-targets --all-features -- -D warnings` | Rust (pinned) | Fail |
| Tests | `cargo test --workspace --all-features` | Rust (pinned) | Fail |
| API docs | `cargo doc --workspace --no-deps` with `-D warnings` | Rust (pinned) | Fail |
| Dependency/licence policy | `cargo deny check` | `cargo-deny` | Fail with install instruction |
| Secret scan | `gitleaks detect` | `gitleaks` | Fail with install instruction |
| Documentation site | `npm ci && npm run build` in `docs/` | Node LTS | Fail with install instruction |
| Link check | `lychee` over built output | `lychee` | Fail with install instruction |

No step is conditional. No step is skipped when its tool is absent — that is the FR-023 fail-closed rule made concrete.

---

## Finding 11 — Repository state audit before the first public push

The Q4 decision makes the first content push immediately world-visible, so the current repository state was audited on 2026-08-11 rather than assumed clean. Findings, in order of consequence:

**A. `.idea/` is staged but never committed.** Four JetBrains files (`.idea/.gitignore`, `.idea/modules.xml`, `.idea/renvor.iml`, `.idea/vcs.xml`) sit in the index with status `A`, but `git ls-tree -r HEAD` confirms none is in any commit. **Remedy is unstaging, not history rewriting** — a materially smaller operation than it first appears. `.gitignore` itself is also staged-new and modified; HEAD currently contains no `.gitignore` at all.

**B. The 66 MB `.git` directory is unreachable objects, not committed content.** `git count-objects -vH` reports 313 loose objects at 65.81 MiB, while a reachable-object walk finds nothing larger than PLAN.md at 0.14 MB. `git fsck --unreachable` accounts for the gap: **125 unreachable blobs, 24 unreachable commits, 73 unreachable trees**, with individual blobs up to 7.05 MB — the `Branding/` assets, added to the index at some point and later unstaged.

This is **local dead weight, not a public-exposure risk**: `git push` transmits only objects reachable from the refs being pushed, so unreachable objects are never sent and never appear in a clone. It should still be pruned so the pre-push state is unambiguous and nobody later resurrects it from the reflog.

**C. The actual public surface is 30 files.** HEAD contains the Spec Kit skill definitions under `.claude/skills/`, the `.specify/` tooling and constitution, and `PLAN.md`. Small enough to review by hand, which is the only review that counts before a repository goes public.

**D. Untracked material needs an explicit include-or-ignore decision** — not silence. Present: `.claude-flow/`, `.claude/.proven-config-version`, `.claude/proven-config.json`, `RENVOR_FRAMEWORK_DEVELOPMENT_PLAN.md`, `RENVOR_MASTER_IMPLEMENTATION_PLAN.md`, `Branding/`, `.playwright-mcp/`, `.DS_Store`, and `specs/`.

**E. `PLAN.md` publication is a decision, not a defect.** It contains the full 30-phase roadmap and is already committed. Publishing it is defensible — it is the program's execution authority and openness about direction serves the project — but it should be a recorded choice rather than an accident of what happened to be committed first. PLAN.md §1 also notes the two legacy root documents may be archived after Phase 001; that archival decision and this one are the same conversation.

**Decision**: repository cleanup is a **blocking predecessor** to repository creation and the first push, sequenced in the plan's Pre-Push Repository Cleanup section and validated by quickstart Gate 0. No cleanup step rewrites published history, because nothing has been published.

**Alternatives considered**: pushing first and cleaning up afterwards (rejected — the Q4 decision means the first push is public, and removing a file from a public repository does not remove it from clones, forks, or caches); a full history rewrite with `git filter-repo` (rejected as unnecessary — the audit shows nothing sensitive in HEAD and nothing large in reachable history, so a rewrite would add risk without removing any).

## Open items carried into ADRs

| Item | Owner | Resolved in | Status |
|---|---|---|---|
| MSRV value and policy framing | Maintainer | ADR-0003 | **Decided 2026-08-11** — fixed floor at 1.94.0; ADR-0003 records the decision rather than making it |
| Workspace crate boundaries and which crates are publishable | Maintainer | ADR-0002 | Open |
| Documentation versioning cadence and when the first version is cut | Maintainer | ADR-0004 | Open |
| Supported operating systems beyond Linux for Phase 001 CI | Maintainer | ADR-0003 (support table) | Open — Linux only for this phase |
| Revalidate MSRV 1.94.0 against real persistence dependencies | Maintainer | Before Phase 006 | Scheduled (FR-061) |

---

## References

Checked 2026-08-11:

- Rust release index — <https://releases.rs/>
- Rust version support dates — <https://endoflife.date/rust>
- Cargo resolver versions and MSRV-aware resolution — <https://doc.rust-lang.org/cargo/reference/resolver.html>
- Cargo publishing reference — <https://doc.rust-lang.org/cargo/reference/publishing.html>
- crates.io trusted publishing RFC 3691 — <https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html>
- crates.io development update, trusted publishing and first-release bootstrap — <https://blog.rust-lang.org/2025/07/11/crates-io-development-update-2025-07/>
- crates.io auth action — <https://github.com/rust-lang/crates-io-auth-action>
- CodeQL Rust general availability — <https://github.blog/changelog/2025-10-14-codeql-scanning-rust-and-c-c-without-builds-is-now-generally-available/>
- CodeQL supported languages — <https://codeql.github.com/docs/codeql-overview/supported-languages-and-frameworks/>
- GitHub artifact attestations — <https://docs.github.com/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-to-establish-provenance-for-builds>
- `actions/attest` — <https://github.com/actions/attest>
- cargo-deny — <https://github.com/EmbarkStudios/cargo-deny>
- sarif-rs / clippy-sarif — <https://github.com/psastras/sarif-rs>
- Docusaurus versioning — <https://docusaurus.io/docs/versioning>
- Docusaurus package releases — <https://www.npmjs.com/package/@docusaurus/core>
