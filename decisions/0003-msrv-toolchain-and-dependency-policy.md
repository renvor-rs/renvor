# ADR-0003: Declare Rust 1.94.0 as a fixed MSRV floor, not a rolling offset

| Field | Value |
|---|---|
| **ID** | 0003 |
| **State** | `accepted` |
| **Reviewer** | `Ahmed Anbar — self-review under W-002` |
| **Review date** | 2026-08-12 |
| **Superseded by** | — |

## Context

An MSRV policy is a public promise. Once published, downstream users plan CI matrices and
distribution packaging around it, so the *shape* of the policy matters as much as the
number.

The concrete problem that forced this decision: PLAN.md originally framed the MSRV as a
rolling window ("N-3"). Research established that **Rust 1.98.0 promotes to stable on
2026-08-20**, which would push 1.94.0 to four releases behind. Under a rolling policy the
project's declared MSRV would have become non-compliant **through the calendar alone**,
with no code change, no dependency change, and no decision by anyone. A policy that
breaks itself while the repository sits untouched is not a policy.

A second force: PLAN.md justified 1.94.0 by "the database stack" — a dependency set that
does not exist until Phase 006. The number is therefore currently justified by an
anticipated requirement rather than a measured one.

## Decision

**The MSRV is Rust 1.94.0, a fixed and explicitly versioned support floor.**

It is not N-3, N-4, or any offset from current stable. A new Rust stable release does not
invalidate it, does not shorten it, and does not trigger a review.

### Declaration

- Declared once, at `[workspace.package] rust-version` in the root manifest.
- Members inherit via `rust-version.workspace = true` and never restate it.
- `clippy.toml` sets `msrv = "1.94.0"` so clippy does not suggest fixes that fail to
  compile on the floor.
- `rust-toolchain.toml` pins the channel to `1.94.0` with `components = ["rustfmt", "clippy"]`
  declared explicitly — a minimal-profile toolchain ships neither, and verification steps
  2 and 3 would fail with "component not installed" rather than a real result. This was
  observed on the maintainer's machine at T005.

### Testing

CI tests **exactly two** toolchains: the declared MSRV `1.94.0`, and current stable. The
MSRV job stays pinned; only the stable job moves. Required check names are
`verify (1.94.0)` and `verify (stable)`.

### Raising the MSRV

A raise may occur only:

1. in a **planned minor or major release**, never a patch;
2. after an **accepted decision record** naming the concrete dependency, language, or
   security requirement forcing it — "newer is better" is not a justification;
3. documented in **three** places: `SUPPORT.md`, the changelog, and the release notes;
4. after the outgoing MSRV has been supported **at least six months**;
5. with a passing verification run at the new exact version.

The policy is **reviewed quarterly**. A review records its outcome and by itself changes
nothing; only the process above changes `rust-version`.

### Scheduled obligation

**Rust 1.94.0 must be revalidated against the actual persistence dependencies before
Phase 006 begins** (FR-061). Owner: Ahmed Anbar. The current number is justified by an
anticipated requirement, and that gap is recorded rather than glossed.

### Dependency and lockfile policy

| Artifact kind | Version requirements | Lockfile |
|---|---|---|
| Reusable library crates | Compatible requirements (`1.2`), not exact pins | Not committed |
| Applications, generators, release tooling, automation | As resolved | **Committed** |
| Documentation site | As resolved | **Committed** (`package-lock.json`) |

`deny.toml` is the **authoritative machine-readable licence policy**; prose must point to
it rather than restate it, because a prose copy drifts and reviewers then trust the wrong
list. Wildcard version requirements are denied — a wildcard is the unreviewed floating
update FR-020 prohibits. Updates arrive as reviewable Dependabot pull requests across the
`cargo`, `github-actions`, and `npm` ecosystems.

### Security-advisory response (added 2026-08-12, task T104)

`deny.toml` cannot express a duration, so the **authoritative advisory policy is
`governance/dependency-advisory-policy.md`**. This record incorporates it **by reference**
rather than by copy, precisely so the two cannot drift: there is one set of numbers, in one
file, and every other mention is a summary that resolves in its favour.

Measured from confirmed detection:

| Condition | Triage | Remediate |
|---|---|---|
| Known active exploitation | 24 hours | Begin immediately; decision within 24 hours |
| Critical | 24 hours | 7 calendar days |
| High | 48 hours | 14 calendar days |
| Medium | 5 calendar days | 30 calendar days |
| Low | 10 calendar days | 90 days or the next prerelease, whichever is first |

Binding rules that make the windows meaningful:

- **Severity is not the CVSS base score alone** — reachability, exploit maturity, known
  exploitation, and actual exposure are weighed, and the reasoning is recorded.
- **Absence of an upstream fix does not extend a deadline.** Remove, disable, replace, or
  isolate the dependency, or block the affected release.
- **Known Critical and High vulnerabilities are public-release blockers and cannot be
  waived.**
- **An ignored advisory without a narrowly scoped, dated record is prohibited**, which in
  practice keeps `deny.toml`'s ignore list empty unless each entry points at such a record.
- Every advisory carries a dated **Advisory Record** with the ten mandatory fields defined
  in the data model.

These deadlines are **Renvor policy decisions**, informed by CVSS, FIRST, RustSec, and NIST
SP 800-218 but mandated by none of them. This policy governs advisories against
**dependencies**; the inbound private-report timetable in `SECURITY.md` is separate and
unchanged.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **N-3 rolling window** (the original PLAN.md framing) | Self-invalidating: 1.98.0 ships 2026-08-20 and would have broken the policy with no code change. Also makes the promise unknowable to users, who would have to compute the floor from a release calendar. |
| Track latest stable only | Excludes every distribution and enterprise user who cannot adopt a compiler within weeks. It also makes every Rust release a potential breaking change for downstreams. |
| A much older floor (e.g. 1.85.0, the edition-2024 minimum) | Wider compatibility, but the number would be unverified against the persistence stack that motivates the project's real constraint, and lowering later is easy while raising is a breaking change — so an unjustified low floor is a promise made blind. |
| No declared MSRV at all | Every dependency bump becomes a silent, unannounced breaking change for someone. Also incompatible with MSRV-aware resolution, which needs the field to function. |
| Declare it, but test only stable | The declared floor would never be exercised. An untested promise is a claim exceeding measurement (constitution principle X). |

## Consequences

**Accepted costs:**

- **Two CI matrix entries forever**, roughly doubling verification wall-clock.
- **The floor will age.** A fixed floor drifts further behind stable over time, and
  raising it requires deliberate process rather than passive drift. That friction is the
  intended trade: users get a promise that only changes when someone decides it should.
- **The current number is not yet evidence-backed.** It rests on an anticipated Phase 006
  requirement. Recorded as a scheduled obligation rather than presented as measured.
- **Six-month dwell time constrains scheduling.** A dependency that needs a newer compiler
  cannot be adopted immediately if the current floor is young.

**What is locked in:** the published `rust-version` is a compatibility promise for at
least six months from each declaration.

**To reverse this** — to move to a rolling policy — requires a superseding ADR and a
migration announcement, because downstream CI configurations depend on the current shape.

## Compliance

| Authority | How this record satisfies it |
|---|---|
| FR-017 | Single authoritative MSRV declaration, inherited, asserted at T037 |
| FR-018, FR-019 | Toolchain pinned with explicit components; both toolchains tested |
| FR-020 | Written dependency update policy; wildcards denied; `deny.toml` authoritative for licences and sources |
| FR-010 | Advisory response windows bounded and incorporated by authoritative reference (T104) |
| FR-021 | Change rules, documentation obligations, and six-month dwell time stated |
| FR-061 | Phase 006 revalidation recorded with a named owner |
| SC-016 | MSRV-aware resolution empirically asserted at T038, not merely configured |
| Constitution principle X | The unmeasured basis for 1.94.0 is disclosed, not concealed |

## Acceptance gate

Re-reviewed 2026-08-12 after T104 closed the gap that previously blocked control 2.

| # | W-002 compensating control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template | ✅ **Met** — five alternatives with rejection reasons; accepted costs stated, including that the current MSRV rests on an anticipated rather than measured requirement |
| 2 | Verification against [`checklists/governance.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/checklists/governance.md) | ✅ **Met 2026-08-12** — T086 final re-review: **79 of 79 passed**. CHK050, which previously blocked this record, is resolved by T104 and verified present in every authoritative location. CHK048 is resolved by T103 and never concerned this record's scope |
| 3 | All required CI and security checks passing | ✅ **Met** — `verify (1.94.0)`, `verify (stable)`, `security`, `docs` all passing on `renvor-rs/renvor`, including on the pull request carrying this change |
| 4 | A dated review record stored with the ADR | ✅ **Met** — this section, dated 2026-08-12 |

**All four controls are met. This record is `accepted`.**

### Review history

- **2026-08-12, first review — not accepted.** Control 2 failed: governance checklist
  CHK050 found that the dependency policy this record sets defined no advisory response
  window. Accepting it then would have put an accepted record's name behind a policy with a
  hole in it. Recorded rather than waived.
- **2026-08-12, re-review after T104 — accepted.** The advisory policy now exists with
  bounded windows, incorporated above by authoritative reference so it cannot drift.

Reviewed by **Ahmed Anbar — self-review under W-002** on **2026-08-12**. **This review is
not independent** and must not be described as such, here or anywhere else. It is a
structured self-review under a recorded, time-bounded exception expiring 2027-02-11 or when
a qualified independent reviewer becomes available, whichever comes first.
