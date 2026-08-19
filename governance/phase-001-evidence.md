# Phase 001 — governance foundation: evidence summary

**Status**: closed · **Merged**: 2026-08-12 · **Independently reviewed**: **no**

This is the **public summary** of Phase 001's evidence. The full working ledger — several hundred
pages of dated measurements, per-task records, and superseded intermediate states — is an internal
working record and is **retained in the project's private records rather than in this repository**.
Everything below is the durable conclusion; nothing that a reader needs in order to understand,
build, verify, or audit the shipped result has been left out.

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

## Carried forward

- **No independent human requirements-and-security review of Phase 001 has occurred** (W-003).
- The four deployment gates above remain non-completed.
- **No Renvor 1.0 claim is made or implied.**
