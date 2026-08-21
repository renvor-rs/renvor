# ADR-0002: Structure Renvor as a virtual workspace behind a thin facade crate

| Field | Value |
|---|---|
| **ID** | 0002 |
| **State** | `accepted` |
| **Reviewer** | `Ahmed Anbar — self-review under W-002` |
| **Review date** | 2026-08-12 |
| **Superseded by** | — |

## Context

Renvor will grow into many crates across many phases. The structural decision has to be
made now, because the workspace layout determines what the first published artifact
promises, and a published artifact's shape is far harder to change than an unpublished
one.

Three forces:

1. **A virtual workspace has no package of its own**, so it has no `edition` from which
   `resolver = "3"` can be inherited. Omitting an explicit `resolver` silently falls back
   to resolver 1, and MSRV-aware dependency resolution never engages — the failure is
   invisible until a dependency resolves to a version that will not build on the declared
   MSRV (research Finding 1).
2. **Phase 001 ships no runtime capability** (FR-047), yet the packaging, licence policy,
   metadata validation, and publish rehearsal all need a *real* crate to act on. A
   rehearsal against a hypothetical package proves nothing (research Finding 9).
3. **The verification runner must not be publishable.** It is internal tooling; shipping
   it to a registry would expose an implementation detail as a public interface.

## Decision

The repository is a **virtual workspace** rooted at `Cargo.toml`, with:

| Member | Role | Publishable |
|---|---|---|
| `crates/renvor` | The facade. The single public entry point users depend on. | **Yes** |
| `xtask` | The verification runner. Internal tooling. | **No** — `publish = false` |

**`resolver = "3"` is declared explicitly** at the workspace root and must remain so, with
the comment explaining why intact. Removing it is a silent regression, not a cleanup.

**`[workspace.package]` is the single authoritative declaration site** for `version`,
`edition`, `rust-version`, `license`, `repository`, `homepage`, and `authors`. Members
inherit with `<key>.workspace = true` and **must not restate any of them**. A second
literal declaration is a defect, asserted mechanically at T037.

**The facade is thin by contract.** `crates/renvor` re-exports; it does not implement.
For Phase 001 it exposes three constants (`VERSION`, `MSRV`, `EXECUTABLE`) and no
capability. Later phases add implementation crates behind it, and the facade's job is to
keep the public surface stable while the crates behind it change.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Single crate, no workspace | Cannot host `xtask` without either publishing the verification runner or bolting it on as a non-Cargo script. It also forces a later, disruptive split once implementation crates appear. |
| Workspace with a **root package** rather than a virtual workspace | The root package would inherit `resolver` from its edition, hiding the resolver trap rather than removing it — and it makes the repository root simultaneously a package and a workspace, which confuses `cargo` path resolution and publishing. |
| Publish `xtask` too | Exposes internal tooling as a public interface with a support obligation nobody asked for, and adds a package to the release topology for no user benefit. |
| No facade — users depend on implementation crates directly | Every internal reorganisation becomes a breaking change for users. The facade exists precisely to absorb that churn. |
| Ship the facade with real capability now | Violates FR-047 and constitution principle X: Phase 001 has verified no runtime behaviour, so shipping any would be a claim exceeding measurement. |

## Consequences

**Accepted costs:**

- **A near-empty published crate**, once it is published — **nothing is published today**.
  `renvor 0.0.0` does nothing. Anyone who finds it on the registry would learn only that
  the project exists. The crate description and README say
  so explicitly rather than implying capability.
- **`resolver = "3"` is load-bearing and easy to delete.** It looks redundant to anyone
  who knows edition 2024 defaults to resolver 3 — which is exactly the misunderstanding
  that causes its removal. Mitigated by the comment and by T038's empirical assertion.
- **Inheritance indirection.** A reader of `crates/renvor/Cargo.toml` cannot see the MSRV
  without opening the root manifest. This is the deliberate trade: one authoritative
  value that cannot drift, at the cost of one extra file to open.

**What is locked in:** the facade name `renvor` is the public dependency surface. Changing
it after publication breaks every dependant.

**To reverse this**, a superseding ADR must address the published facade's compatibility
obligation and the migration path for dependants.

## Compliance

| Authority | How this record satisfies it |
|---|---|
| FR-016, FR-017 | Single authoritative `edition` and `rust-version`, inherited by members, asserted at T037 |
| FR-047 | Facade ships no runtime capability in Phase 001 |
| FR-057 | `resolver = "3"` declared explicitly in the virtual workspace |
| FR-040 | `xtask` exempt from publishable-metadata rules via `publish = false`; the facade declares zero git or path dependencies |
| Constitution principle X | No capability is claimed that has not been verified |
| Research Finding 1, Finding 9 | Resolver trap avoided explicitly; rehearsal targets a real crate |

## Acceptance gate

| # | W-002 compensating control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template | ✅ **Met** — five alternatives recorded with rejection reasons, and the accepted costs are stated |
| 2 | Verification against [`checklists/governance.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/checklists/governance.md) | ✅ **Met 2026-08-12** — T086 complete: 77 of 79 items passed, 2 failed as genuine specification gaps (CHK048, CHK050), 0 weakened. No unresolved requirement affects the workspace decision — CHK028 through CHK036 all passed. |
| 3 | All required CI and security checks passing | ✅ **Met 2026-08-11** — `verify (1.94.0)` 59s, `verify (stable)` 53s, `security` 43s, `docs` 40s, plus dependency review and CodeQL, all passing on `renvor-rs/renvor` |
| 4 | A dated review record stored with the ADR | ✅ **Met** — this section, dated 2026-08-12 |

**All four controls are met. This record is `accepted`.**

Reviewed by **Ahmed Anbar — self-review under W-002** on **2026-08-12**. This review is
**not independent** and must not be described as such, here or anywhere else. It is a
structured self-review operating under a recorded, time-bounded exception that expires on
2027-02-11 or when a qualified independent reviewer becomes available, whichever is first.
