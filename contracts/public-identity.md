---
description: "Contract — frozen public identity values: product, crate, and executable names"
version: "1.0.0"
status: "normative — amended 2026-08-17 by ADR-0010, which supersedes ADR-0001. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract: Public Identity

**Feature**: Phase 001 — governance foundation | **Status**: **Confirmed 2026-08-11**, **amended 2026-08-17 by ADR-0010** — every row of the Name Availability Record reads `available` or `owned-by-project` | **Satisfies**: FR-001 – FR-006, FR-048, FR-049, FR-052

> **Amended 2026-08-17 — the executable is `renvor`.** ADR-0010 supersedes ADR-0001 and
> unifies the product, facade-crate, and executable spellings. The rows and binding rules
> below carry the current values; the superseded `renover` values, and the reasoning that
> chose them, are preserved in ADR-0001 and in `governance/name-availability.md` §Superseded.
> This amendment is the "superseding ADR and impact analysis" that the Consumers section at
> the foot of this contract requires. Nothing else in this contract changed: `Renvor`,
> `renvor`, `renvor-cli`, `renvor-rs`, `.renvor/`, `RENVOR_`, and `renvor.dev` are all
> unaffected.

This is the contract every later phase, document, template, and example must conform to. It was provisional until every row of `governance/name-availability.md` reached `available` or `owned-by-project`; that condition was met at **T022 on 2026-08-11**, so the values below are now confirmed.

**Confirmation expires 2026-09-10.** The underlying evidence carries a 30-day validity window (FR-006). Re-verification before the first content push is mandatory (T053/T054), and this status line reverts to provisional if that window lapses unre-verified.

## The names

| Item | Value | Where it was verified | Status (2026-08-11) | Claimed in Phase 001? |
|---|---|---|---|---|
| Product and framework | `Renvor` | Derived; hosting organization is the operative control | `available` (derived) | Organization claimed |
| Package name prefix | `renvor-` | crates.io search — 0 crates contain "renvor" | `available` | Verified only |
| Facade package | `renvor` | crates.io — HTTP 404 | `available` | Verified only |
| CLI package | `renvor-cli` | crates.io — HTTP 404 (`renvor_cli` also free) | `available` | Verified only |
| Installed executable | `renvor` | 8 probes, each with a positive control — see ADR-0010 §Name-availability evidence | `available` (**bounded**), re-verified **2026-08-17** | Verified only |
| Primary command | `renvor new` | — (derived) | derived | Derived |
| Package command | `renvor add` | — (derived) | derived | Derived |
| Project state directory | `.renvor/` | — (derived) | derived | Derived |
| Environment prefix | `RENVOR_` | — (derived) | derived | Derived |
| Hosting organization | **`renvor-rs`** | GitHub — `/users` and `/orgs` both 404, no redirect | `available` | **To be claimed at T024** |
| Hosting repository | **`renvor`** (path `renvor-rs/renvor`) | GitHub — `/repos/renvor-rs/renvor` 404 | `available` | **To be claimed at T024** |
| Documentation domain | **`renvor.dev`** | Public RDAP + maintainer attestation | **`owned-by-project`** | Already owned |

Full evidence — exact URLs, UTC timestamps, and observations — is in `governance/name-availability.md`.

"Verified only" reflects the Q1 clarification: package-registry names are checked but **not** reserved by publishing. No placeholder crate was published to hold any name. See the residual-risk note below.

### Two qualifications on the confirmed values

1. **`renvor` clearance is bounded, not exhaustive.** No global registry of executable names exists. Five package registries, Debian sources, public Rust manifests, and the local `PATH` were checked on **2026-08-17**, each with a positive control; other Linux distributions, BSD ports, Windows package managers, and privately distributed binaries were not. One probe — public Rust manifests — is further bounded: GitHub code search does not index this repository, so its `0` result bounds rather than proves absence. See ADR-0010 §Name-availability evidence and the scope table in `governance/name-availability.md`.

2. **`Renvor` has not been trademark-cleared.** This contract previously named a trademark/common-law search as the verification method for the product-name row. That search was outside the authorised scope of the verification pass and has **not** been performed. The row is recorded as derived, not as cleared, and the gap is tracked as residual risk R-4.

### The global GitHub account `Renvor` is not a collision

A pre-existing GitHub **user** account holds the global login `Renvor` (id 206448205, created 2025-04-06). It occupies only that global login. The project's paths — `github.com/renvor-rs` and `github.com/renvor-rs/renvor` — are unoccupied and were verified independently. Do not conflate the two.

## Binding rules

1. **The executable is `renvor`.** The product, the facade crate, and the command share one spelling. Every document, test, example, help text, and error message uses `renvor` when referring to the command. `cargo install renvor-cli` installs a binary named `renvor`; the package name and the binary name differ, which is declared in that crate's manifest when Phase 003 creates it. **This rule previously required the opposite** — the executable was `renover`, deliberately distinct — and ADR-0010 records why that was reversed.

2. **No frozen reference before confirmation.** Manifests, templates, documentation, and examples may not be treated as final while any row is unconfirmed (FR-004). **This condition was satisfied on 2026-08-11**, so references may now be frozen against the values above — subject to the 2026-09-10 expiry. The repository may exist publicly before this point; its *first content push* may not (FR-052).

3. **Stop, do not substitute.** If a name is `held-by-other` or `ambiguous`, work halts for an explicit recorded naming decision. No alternative is selected automatically, and no partially-renamed state is committed (FR-003).

4. **Registered-but-unused counts as unavailable.** A placeholder or squatted name is treated as taken. Ownership must be transferred or a decision recorded — "it looks abandoned" is not a status.

5. **Evidence expires.** Rows older than 30 days are re-verified before the first content push (FR-006).

## Residual risk accepted

Because package-registry names are verified rather than reserved, any of them may be claimed by a third party between verification and the project's first publication. This is an accepted, tracked limitation (FR-049), not an oversight. It must appear in the known-limitations list of `governance/phase-001-evidence.md` with a named owner and the phase that closes it — the phase that performs the first publication.

The exposure is bounded by the 30-day re-verification rule and by the fact that a conflict discovered later triggers the same stop-and-decide rule as one discovered now.

## Consumers

Every phase from 002 onward reads this contract. Changing any value after Phase 001 closes requires a superseding ADR and an impact analysis covering manifests, templates, documentation, published links, and any external reference.
