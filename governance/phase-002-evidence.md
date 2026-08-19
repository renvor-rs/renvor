# Phase 002 — transport-independent core kernel: evidence summary

**Status**: closed · **Merged**: 2026-08-17 · **Independently reviewed**: **no**

This is the **public summary**. The full working ledger is an internal working record and is
**retained in the project's private records rather than in this repository**. Git history is
unchanged: earlier commits and pull requests still contain it.

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

## Carried forward

- **No independent human requirements-and-security review of Phase 002 has occurred** (W-005).
- The public kernel surface remains **explicitly unstable** — see
  [`contracts/api-stability.md`](../contracts/api-stability.md) for the conditions that close it.
