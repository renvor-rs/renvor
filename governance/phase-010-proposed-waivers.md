# Phase 010 — Proposed Waivers (not granted)

**Companion to**: [`phase-010-evidence.md`](phase-010-evidence.md) · [`waivers.md`](waivers.md)
**Drafted**: 2026-09-04, by the implementing session
**Status**: **PROPOSED. Neither row is granted, and neither carries any authority.**

This file exists because the waiver ledger's own rule — enforced by an `xtask` test that re-derives
the headline from the table and refuses any waiver number the table has not granted — counts only
waivers the maintainer has granted. A draft therefore cannot live in the ledger. If the maintainer
grants either row at the merge-authority checkpoint, it moves into `waivers.md` under the next
free numbers (W-021 and W-022 at the time of drafting), the ledger headline is updated, and the
RO-001 obligation log receives its Phase 010 entry. Until then:

- **ADR-0031 … ADR-0037 remain `proposed` and carry no authority.**
- **Phase 010 is not closed.** Its evidence pack is written and its gates are recorded; closure
  is the maintainer's decision.
- If granted, the same limits as W-019 and W-020 apply verbatim: a waiver of *who reviews* is not
  a waiver of *what must be true*; agent and self-review are advisory and non-independent and
  must never be described as independent; security release blockers are never waived; the phase
  must receive genuine independent re-review before any public release; the expiry is
  2027-02-11 or a qualified reviewer, whichever occurs first.

## Proposed row 1 — the ADR cluster

| Rule waived | Why | Compensating controls (counted) | Approver | Expiry | Closure condition |
|---|---|---|---|---|---|
| constitution §Development and Phase Workflow #7 and `GOVERNANCE.md` — an architecture decision record MUST NOT reach `accepted` without an **independent** review. Applied to the **seven Phase 010 records ADR-0031 … ADR-0037** as ONE coupled cluster (the port-and-substitute shape, the job storage decision, the four adapter selections, and the retry policy each depend on a boundary another draws) | The project has a single maintainer, who authored all seven records and took every measurement they rest on. No second qualified person exists. **W-019 is Phase 009's ADR-cluster waiver and confers no authority here** | (1) **every package decision is measured against the real lockfile (490 → 528 packages), not asserted** — each candidate's additions, licences across every target `deny.toml` evaluates, advisories with positive controls, and feature isolation against `cargo tree`; (2) **ADR-0035's central claim is a refusal measured four ways** — every S3 candidate failed a named gate, and the three routes a later phase could take are written down rather than one taken quietly; (3) **ADR-0033's single-provider rule is executable** — `xtask` step 7 walks feature edges and refuses a second `rustls` provider, and that walk found `renvor-cache` shipping with **none**; (4) **ADR-0036's "the formatter must be Renvor's" claim was verified against the crate's source** and pinned by a test planting a canary in an event field, a span field, and a nested span field; (5) **each record's operative decision is pinned by an executable test** — 88 controlled mutations across the phase with every survivor investigated. **Preconditions, not counted**: the CI platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, the format, clippy and rustdoc gates, and `cargo xtask verify` all run unconditionally already | Ahmed Anbar (if granted) | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — whichever first, inheriting the earliest open expiry under the ratchet rule | A qualified independent reviewer reviews **all seven records as one cluster** against the head and tree named in `phase-010-evidence.md`; the outcome is recorded there |

## Proposed row 2 — phase closure

| Rule waived | Why | Compensating controls (counted) | Approver | Expiry | Closure condition |
|---|---|---|---|---|---|
| spec FR-027 and `PLAN.md` §6.1 step 10 — a phase MUST NOT close without a recorded **independent** requirements-and-security review. Applied to **Phase 010** as a whole | The project has a single maintainer, who wrote every line under review and took every measurement it rests on. No second qualified person exists. **This would be the TENTH consecutive phase-level waiver of this rule for this reason** (W-003, W-005, W-008, W-010, W-012, W-014, W-016, W-018, W-020). The trend guard was tripped at three and has deepened every phase since | (1) **the repository's own gates found eight defects after batches were green and each was fixed at the root and pinned** (`phase-010-review-record.md` §2); (2) **two Phase 009 limitations are closed by measurement, not prose** — L-4 by a transport guard driven four ways through the PostgreSQL-backed flow with a valid token, L-11 by a recorded event asserted field by field; (3) **every adapter is exercised against a real server** — Valkey, PostgreSQL, MySQL, Mailpit, a local OTLP receiver, a real filesystem — each with a redaction canary sweep, and the census requires the four job rows to report in (a misspelled row was proved to fail it); (4) **88 controlled mutations with every survivor investigated**, including one killed only by the harness wall clock and recorded as a hang; (5) **the tree's "arrives in Phase 010" promises were corrected with their pinning tests**. **Preconditions, not counted**: fixing Critical and High findings is already required by `PLAN.md` §17.3; the platform matrix, `cargo deny`, CodeQL, dependency review, secret scanning, the format, clippy and rustdoc gates, the dependency inventory (FR-040), and `cargo xtask verify` are all already required | Ahmed Anbar (if granted) | **2027-02-11**, or **immediately** when a qualified independent human reviewer becomes available — whichever first, inheriting the earliest open expiry under the ratchet rule | A qualified independent reviewer performs the requirements-and-security review of **Phase 010 in full**, against the head and tree named in `phase-010-evidence.md`, including the Windows platform legs; the outcome is recorded there |
