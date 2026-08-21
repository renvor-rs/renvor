# Phase 001 — governance foundation: evidence summary

**Status**: closed · **Merged**: 2026-08-12 · **Independently reviewed**: **no**

This is the **public summary** of Phase 001's evidence. The full working ledger — several hundred
pages of dated measurements, per-task records, and superseded intermediate states — is an internal
working record and is **retained in the project's private records rather than in this repository**.
Everything below is the durable conclusion; nothing that a reader needs in order to understand,
build, verify, or audit the shipped result has been left out.

**The full ledger is still readable.** Git history was not rewritten, so it remains at merge commit
`01327b1`: [`phase-001-evidence.md` as of 01327b1](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/governance/phase-001-evidence.md). Citations elsewhere in this
repository that name a numbered section — `§3u`, `§3av`, `§6`, and the rest — link there directly,
because those sections exist in the ledger and not in this summary.

**Git history is unchanged.** Earlier commits and the pull requests that carried them still contain
the full ledger, and this summary does not remove it from the public history.

## What the phase delivered

The repository's governance foundation: licensing, security and support policy, contribution rules,
release procedure, branch protection, the verification sequence, the decision-record process, and
the waiver ledger.

## Task outcome

**114 tasks** — **108 completed**, **1 waived**, **1 cancelled**, **4 transferred**.

| Outcome | Tasks | Note |
|---|---|---|
| Waived | T088 | `WAIVED / NOT MET` under **W-003**. No independent human review of Phase 001 occurred |
| Cancelled | T114 | GitLab cutover abandoned; its recovery requirements were never met |
| Transferred, still non-completed | T102, T108, T109, T111 | The four deployment gates, each with a named destination |

## Decision records

**Seven records: six `accepted`, one `superseded`.** ADR-0002 through ADR-0006 and **ADR-0010**
(accepted 2026-08-17 under W-002) are accepted; **ADR-0001 is `superseded`** by ADR-0010 and does
**not** currently govern. Each was reviewed as a **non-independent self-review** under W-002.

Stated by state rather than as a total, because a reader scoping current decision authority must
not treat the superseded record as live — `GOVERNANCE.md` is authoritative on this split.

## Verification

Governance checklist **79/79**. `cargo xtask verify` passes on both the pinned MSRV and current
stable with exit 0. The release procedure is documented and was rehearsed **without publishing**.

## Publication state

At Phase 001 closure **no crate, package, container image, release, or tag had been published**,
verified read-only on 2026-08-15 against the crates.io sparse index with a positive control.

**This changed on 2026-08-17**: the landing site is deployed and served over a valid certificate
from a published, digest-pinned image. `docs.renvor.dev` remains undeployed.

One measurement could not be made: **GHCR was never independently enumerated**, because the
available token lacked `read:packages` and anonymous GHCR returns HTTP 403 without distinguishing
*absent* from *private*. The no-image statement therefore rests on the absence of any publishing
workflow or run, **not** on a registry listing.

## Evidence index (PLAN.md §6.2)

`PLAN.md` §6.2 requires every completed phase to link eight classes of evidence. Each row below
points at a **current public source** wherever one exists; a commit-pinned link is used only where
no current public equivalent does. **A class with nothing to show says so** rather than being
omitted — an absent row would read as an oversight instead of a fact.

| # | Evidence class | Where it is |
|---|---|---|
| 1 | Accepted ADRs | [`decisions/`](../decisions/) — ADR-0002 … ADR-0006 and ADR-0010 `accepted`; **ADR-0001 `superseded`** by ADR-0010 |
| 2 | Package versions and licence review | **No runtime dependency was introduced by this phase**; it ships no runtime code. The licence and advisory gates that would review one are [`deny.toml`](../deny.toml) and [`governance/dependency-advisory-policy.md`](dependency-advisory-policy.md), both active from this phase onward, over the committed [`Cargo.lock`](../Cargo.lock) |
| 3 | Verification commands and platforms | [`contracts/verification-sequence.md`](../contracts/verification-sequence.md) — the ordered sequence `cargo xtask verify` runs; [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) for the platforms it runs on |
| 4 | Compatibility rows exercised | MSRV floor and tested toolchains. **This phase's declared row is Linux on the pinned MSRV and the current stable channel**; macOS and Windows entered in a later phase. *(Historical. The current claim is three platforms, carried by [ADR-0011](../decisions/0011-support-linux-macos-and-windows.md) in [`contracts/support-policy.md`](../contracts/support-policy.md), which is the sole current authority. This row records Phase 001 and is not rewritten to match.)* |
| 5 | Security checklist evidence | [`.github/workflows/security.yml`](../.github/workflows/security.yml), [`deny.toml`](../deny.toml), [`.gitleaks.toml`](../.gitleaks.toml), and [`SECURITY.md`](../SECURITY.md). Checklist verdicts: [`checklists/governance.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/checklists/governance.md) — 79/79 |
| 6 | Generated-project smoke tests | **None, and none were possible.** No generator existed until Phase 003; the first generated-project smoke tests are `crates/renvor-cli/tests/generated.rs`, added there |
| 7 | Documentation and migration notes | [`README.md`](../README.md), [`GOVERNANCE.md`](../GOVERNANCE.md), [`RELEASING.md`](../RELEASING.md), [`SUPPORT.md`](../SUPPORT.md), and the documentation site under `docs/` |
| 8 | Known limitations, with owner and target | [`governance/waivers.md`](waivers.md) — **W-001, W-002, W-003**, owner Ahmed Anbar, expiry 2027-02-11. Plus the four transferred deployment gates T102, T108, T109, T111, listed above with their destinations |

## Carried forward

- **No independent human requirements-and-security review of Phase 001 has occurred** (W-003).
- The four deployment gates above remain non-completed.
- **No Renvor 1.0 claim is made or implied.**
