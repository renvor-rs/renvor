# Phase 002 — transport-independent core kernel: evidence summary

**Status**: closed · **Merged**: 2026-08-17 · **Independently reviewed**: **no**

This is the **public summary**. The full working ledger is an internal working record and is
**retained in the project's private records rather than in this repository**. Git history is
unchanged: earlier commits and pull requests still contain it, and it remains readable at merge
commit `01327b1`: [`phase-002-evidence.md` as of 01327b1](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/governance/phase-002-evidence.md). Citations that name a
numbered section — `§W-006` and the rest — link there directly, because those sections exist in
the ledger and not in this summary.

## What the phase delivered

The transport-independent core kernel — configuration, provider graph and resolution, lifecycle,
typed state, error taxonomy, and observability — with no transport and no network surface.

## Integration

Pull request **#19** was squash-merged into `main` on **2026-08-17T07:23:53Z** as
**`8abc0f4168300f04b86e01b0dc0a4bfff2a15af9`**, from the reviewed head `fae251c0`.

## Waivers

| Waiver | Scope | Independent review |
|---|---|---|
| **W-004** | ADR-0007 alone | none |
| **W-005** | Phase 002, phase level | none |
| **W-006** | ADR-0009 alone | none |

**W-006 was the third explicit reviewed exception in Phase 002**, exceeding the ledger's expected
maximum of two per phase. That departure is recorded as an acknowledged breach in
[`waivers.md`](waivers.md) rather than hidden by extending W-004 or W-005.

## Verification

Full workspace tests, `cargo xtask verify` on both toolchains, dependency and licence gates, secret
scanning, and CodeQL — all green at the merged head. The resolver work budget, the graph-size
ceilings, and the configuration proof gate are enforced by executable tests, not by assertion in
prose.

## What the record shows about its own accuracy

The status paragraph of the original ledger named the open task set **three times and was wrong
twice**. It was corrected, and then the naming was removed entirely rather than repaired again: a
summary of a moving boundary drifts every time the boundary moves. The task ledger was the single
authority.

That is preserved here because it is the useful lesson, not because it is flattering.

## Evidence index (PLAN.md §6.2)

`PLAN.md` §6.2 requires every completed phase to link eight classes of evidence. Each row below
points at a **current public source** wherever one exists; a commit-pinned link is used only where
no current public equivalent does. **A class with nothing to show says so** rather than being
omitted — an absent row would read as an oversight instead of a fact.

| # | Evidence class | Where it is |
|---|---|---|
| 1 | Accepted ADRs | [`decisions/`](../decisions/) — **ADR-0007** (custom kernel primitives), **ADR-0008** (publishable crate set), **ADR-0009** (vendored `image-size` replacement) |
| 2 | Package versions and licence review | [`governance/phase-002-dependency-inventory.md`](phase-002-dependency-inventory.md) — every dependency with version, licence, maintenance status, MSRV compatibility, and advisories, resolved against the committed [`Cargo.lock`](../Cargo.lock) |
| 3 | Verification commands and platforms | [`contracts/verification-sequence.md`](../contracts/verification-sequence.md); [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) |
| 4 | Compatibility rows exercised | The pinned MSRV **and** the current stable channel, both required to pass, on Linux. *(Historical. The macOS and Windows jobs were added later, at T150; the current claim is three platforms, carried by [ADR-0011](../decisions/0011-support-linux-macos-and-windows.md) in [`contracts/support-policy.md`](../contracts/support-policy.md), which is the sole current authority. This row records Phase 002 and is not rewritten to match.)* |
| 5 | Security checklist evidence | [`.github/workflows/security.yml`](../.github/workflows/security.yml) (advisories, licences, bans, sources), CodeQL and dependency review in [`ci.yml`](../.github/workflows/ci.yml), and [`governance/dependency-advisory-policy.md`](dependency-advisory-policy.md). Secret redaction is contract-tested — [`contracts/configuration-contract.md`](../contracts/configuration-contract.md) and [`contracts/error-taxonomy.md`](../contracts/error-taxonomy.md) |
| 6 | Generated-project smoke tests | **None.** This phase ships a library kernel and no generator. The first generated-project smoke tests arrive in Phase 003 |
| 7 | Documentation and migration notes | The five Phase 002 contracts in [`contracts/`](../contracts/) — configuration, error taxonomy, lifecycle, observability, provider graph — plus the API reference on the documentation site |
| 8 | Known limitations, with owner and target | [`governance/waivers.md`](waivers.md) — **W-004, W-005, W-006**, owner Ahmed Anbar. The kernel surface is **explicitly unstable**: [`contracts/api-stability.md`](../contracts/api-stability.md) states the two conditions that close the window |

## Carried forward

- **No independent human requirements-and-security review of Phase 002 has occurred** (W-005).
- The public kernel surface remains **explicitly unstable** — see
  [`contracts/api-stability.md`](../contracts/api-stability.md) for the conditions that close it.
