---
description: "Contract — supported toolchains, platforms, MSRV floor, and change rules"
version: "1.1.1"
status: "normative — a public promise and a release contract under principle V; no release has occurred. 1.1.1 (2026-08-21) corrects the platform-evidence rule, which 1.1.0 stated in a form no commit could satisfy; NO platform is added or withdrawn, and the MSRV is unchanged. 1.1.0 (2026-08-21) added macOS and Windows as supported platforms, named the six platform/toolchain contexts, and stated which of them branch protection actually requires. Governed by ADR-0011, accepted 2026-08-21 under waiver W-002 — a review that is NOT independent. This version identifies the contract text, not a stability promise"
---

# Contract: Support and Version Policy

**Feature**: Phase 001 — governance foundation | **Satisfies**: FR-017 – FR-021 | **Governed by**: **ADR-0011** *(accepted 2026-08-21)*, superseding ADR-0003

> **This is the sole normative current authority** for supported toolchains, supported platforms,
> the MSRV floor, and the rules for changing them. [`SUPPORT.md`](../SUPPORT.md) is the
> human-facing summary and `docs/docs/support-policy.mdx` is the published summary; both link
> here, and **any disagreement resolves in favour of this document**.
>
> The governing record is
> [`ADR-0011`](../decisions/0011-support-linux-macos-and-windows.md), **accepted 2026-08-21 under
> waiver W-002 — a self-review that is NOT independent.** It supersedes ADR-0003, whose decision
> body is preserved verbatim.

This is a public promise. Under constitution principle V it is a release contract, and under principle X no value here may be claimed without a passing verification run behind it.

## Toolchain support

| Field | Value | Verified |
|---|---|---|
| Declared MSRV | **1.94.0** (released 2026-03-05) — a **fixed floor**, not a rolling offset | Must have a passing CI run at exactly this version |
| Declaration site | `[workspace.package] rust-version` at the workspace root; members inherit via `rust-version.workspace = true` | No second independent declaration may exist |
| Edition | **2024**, declared explicitly on every package | Not inherited by a virtual workspace |
| Stable channel tested | **The current stable channel**, resolved and recorded by CI at run time. No version number is stated here: a number written into a document does not float, and would be silently false the day after it was typed | Must have a passing CI run |
| Edition | 2024 (requires ≥ 1.85.0) | Satisfied by MSRV |
| Cargo resolver | 3 (requires ≥ 1.84.0), **declared explicitly** in the virtual workspace | Satisfied by MSRV; explicit declaration verified separately |

**The MSRV is a fixed, explicitly versioned support floor.** It is not N-3, N-4, or any offset from current stable. A new Rust stable release does not invalidate it, does not shorten it, and does not trigger a review. The minimum-version CI job stays pinned at 1.94.0; only the stable job moves.

This framing was chosen deliberately over a release-count window. Rust 1.98.0 promoted to stable on 2026-08-20, which would have pushed 1.94.0 to four releases behind and violated an "N-3" policy through the calendar alone, with no code change involved. A fixed floor removes release count from the policy entirely.

## Platform support

**Linux, macOS, and Windows are supported.** Only platforms with passing evidence are listed,
and **a supported-platform claim is carried by passing CI attached to the exact commit whose
tree is being claimed** — not by a result inherited from an earlier commit, and not by "it
worked last week".

### What "the exact commit" means, and why a branch head qualifies

Evidence binds to a **commit**, because a commit is what a CI result attaches to.

- **A pull-request branch head is valid evidence.** Being on a branch does not disqualify a
  result. Every commit is on some branch — `main` included — so a rule that excluded branches
  would exclude every commit that has ever been reviewed, which is no rule at all.
- **A later commit does not inherit the previous head's evidence.** Each new commit needs its
  own passing contexts before it carries the claim. Until it has them, the claim rests on the
  last head that does, and that head is named rather than implied.
- **On a squash merge**, where the merge commit's tree is verified identical to the reviewed
  head's tree, the reviewed evidence carries to the merge commit; `main`'s own run then
  confirms it on `main`. Tree identity is the thing verified, not the commit hash, which
  necessarily differs.
- **No document is required to name its own commit.** A commit cannot contain its own hash, so
  a rule demanding that a record cite the CI result for the very commit that introduces it is
  unsatisfiable by construction.

**Acceptance evidence and ongoing evidence are different things.** A dated run recorded in a
decision record is *historical acceptance evidence*: it is fixed at the head it names, it
proves the acceptance sequence was followed, and it stays true about that head permanently. The
*ongoing* support claim is carried by the current head's six contexts, which live in CI rather
than in any document. Neither substitutes for the other, and neither expires the other.

**Version 1.1.0 stated this rule as "at the exact head being claimed — not at an earlier head,
not on a branch."** That form was unsatisfiable: it disqualified every pre-merge head for being
on a branch, and demanded a self-reference no commit can contain. It is corrected here, dated,
rather than quietly reworded — see the version history at the end of this contract.

| Platform | Status | Contexts carrying the claim |
|---|---|---|
| Linux (`ubuntu-latest`) | **Supported** | `verify (1.94.0)`, `verify (stable)` |
| macOS (`macos-latest`) | **Supported** | `platform (macos-latest, 1.94.0)`, `platform (macos-latest, stable)` |
| Windows (`windows-latest`) | **Supported** | `platform (windows-latest, 1.94.0)`, `platform (windows-latest, stable)` |

### The six contexts

Six platform/toolchain contexts run on **every** pull request — three platforms × the pinned MSRV 1.94.0 and the current stable channel:

`verify (1.94.0)` · `verify (stable)` · `platform (macos-latest, 1.94.0)` · `platform (macos-latest, stable)` · `platform (windows-latest, 1.94.0)` · `platform (windows-latest, stable)`

### Running is not the same as being required

**Of those six, only the two Linux contexts are required by branch protection.** `main`'s required-status-check list is exactly `verify (1.94.0)`, `verify (stable)`, `security`, `docs` — and `security` and `docs` are not platform contexts.

| Platform | Runs on every pull request | **Required** by branch protection |
|---|---|---|
| Linux | yes | **yes** — `verify (1.94.0)`, `verify (stable)` |
| macOS | yes | **no** |
| Windows | yes | **no** |

The four `platform (…)` contexts are **executed evidence, not enforced gates.** Their failure is visible and a maintainer is expected to act on it, but branch protection alone would not block a merge on them — so **the macOS and Windows claims rest on review practice, not on an enforced gate.** Making them required is a repository-settings change and has not been made.

This is stated rather than assumed because the distinction decays: adding a job feels like adding a gate, and it is not one until the protection rule names it. **The six contexts must never be described collectively as "required checks."**

### What each job actually runs

| Job | Runs |
|---|---|
| `verify` (Linux) | The **complete** sequence in [`verification-sequence.md`](./verification-sequence.md) — every step, none conditional |
| `platform` (macOS, Windows) | `cargo test --workspace --all-features -- --test-threads=1` and `cargo check -p renvor --no-default-features --all-targets` — **and nothing else** |

`platform` is a **separate job from `verify` deliberately.** Adding an `os` dimension to `verify`'s matrix would rename its contexts to `verify (ubuntu-latest, 1.94.0)` and silently empty the branch-protection rule, which matches contexts by name.

The platform jobs omit gitleaks, lychee, the commit-history scan, and the documentation build. Those are properties of the **repository**, not of the platform.

### Known platform evidence limitations

Recorded rather than dropped now that the platforms are claimed:

- **Two behaviours are `#[cfg(unix)]`-gated** and therefore exercised on Linux and macOS only: the FIFO refusal, and the test driving the non-Unicode environment-name path. The FIFO case cannot arise on Windows in that form. The **non-Unicode name** case *can* — a Windows environment name is WTF-8 and may contain unpaired surrogates — so what is unix-gated is the **test**, not the code path.
- **`a_destination_whose_state_cannot_be_established_fails_closed` is `#[cfg(unix)]`**: the fail-closed destination check has no Windows-specific test.
- **Windows has had no adversarial review.** Every advisory review of Phase 003 ran on macOS. CI exercises Windows and is green, but CI runs the tests the *author* wrote.
- Several path rules in `crates/renvor-cli/src/paths.rs` — reserved device names, trailing dot or space — are enforced on every platform but were **reasoned from Windows behaviour never observed on Windows**.

**Support does not imply that every platform-specific behaviour has received independent human review.** None has. W-003, W-005, and W-008 remain open, and nothing here narrows them.

### Historical: the Phase 001 platform table

**Superseded. Retained as history, not as policy.** Phase 001 recorded:

| Platform | Status in Phase 001 | Rationale given then |
|---|---|---|
| Linux (`ubuntu-latest`) | Verified and supported | Primary verification platform |
| macOS | Not yet claimed | *"No platform-sensitive code exists to verify"* |
| Windows | Not yet claimed | Same |

**That rationale stopped being true during Phase 002** and nobody revisited it. The configuration layer resolves filesystem paths, refuses non-regular files by type from an open descriptor, opens files with a platform-specific flag, and reads `OsString` environment names that are arbitrary bytes on unix and WTF-8 on Windows. The correction was a verification job, added at T150, not a rewording — and this contract is the last document to catch up with it.

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

This contract changes only through a superseding ADR with an impact analysis covering published packages, documentation, the compatibility matrix, and any downstream consumer relying on the current promise. **That rule is unchanged by this revision, and this revision was made through it**: the platform change above is carried by [`ADR-0011`](../decisions/0011-support-linux-macos-and-windows.md), which supersedes ADR-0003 on acceptance.

Version history of this contract text:

| Version | Date | Change | Governing record |
|---|---|---|---|
| 1.0.0 | 2026-08-19 | First explicit version assigned to the existing text; earlier revisions are in public Git history | ADR-0003 |
| **1.1.0** | **2026-08-21** | Adds macOS and Windows as supported platforms, names the six contexts, states the required-versus-running distinction and the known evidence limitations, and replaces the fixed stable version number with the floating channel. **Additive; no MSRV change** | **ADR-0011** *(`accepted`)* |
| **1.1.1** | **2026-08-21** | Corrects the platform-evidence rule. 1.1.0 required evidence "at the exact head being claimed — not at an earlier head, not on a branch", which no commit can satisfy: every head is on a branch, and no commit can cite its own hash. Replaced with commit-attached evidence that a branch head satisfies, plus the squash-merge tree-identity rule and the separation of acceptance evidence from ongoing evidence. **No platform added or withdrawn; no MSRV change** | **ADR-0011** *(`accepted`)* |
