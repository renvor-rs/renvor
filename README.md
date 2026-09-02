<p align="center">
  <img alt="Renvor" src="assets/renvor-lockup-v40-dark.svg" width="360">
</p>

<p align="center">
  <a href="https://github.com/renvor-rs/renvor/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/renvor-rs/renvor/actions/workflows/ci.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/renvor-rs/renvor/actions/workflows/docs.yml"><img alt="Documentation" src="https://github.com/renvor-rs/renvor/actions/workflows/docs.yml/badge.svg?branch=main"></a>
  <a href="https://github.com/renvor-rs/renvor/actions/workflows/security.yml"><img alt="Security" src="https://github.com/renvor-rs/renvor/actions/workflows/security.yml/badge.svg?branch=main"></a>
  <a href="SUPPORT.md"><img alt="MSRV 1.94.0" src="https://img.shields.io/badge/MSRV-1.94.0-orange.svg"></a>
  <a href="#licence"><img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg"></a>
</p>

> ## ⚠️ Pre-release — Renvor does not work yet
>
> **Nothing is published.** Neither `renvor` nor `renvor-cli` exists on crates.io — verified
> against the registry index on 2026-08-16, both HTTP 404. There is **no way to install
> Renvor**, and there will not be one before a `0.1.0` release.
>
> Phase 002 added a working **transport-independent kernel**: a seven-phase application
> lifecycle, a single-pass provider dependency resolver with a counted work budget, layered
> configuration with per-key attribution and total secret redaction, bounded deadlines on
> every call into your code, liveness and readiness as independent answers, and a failure
> injection harness. Phase 003 added the **`renvor` command** and its transactional project
> generator. Phase 004 adds the **first real transport**: a REST and HTTP delivery adapter with
> declarative routing, a versioned middleware order, trusted-proxy client identity, fail-closed
> host validation, deny-by-default CORS, documented limits, and a drain bounded by the kernel's
> own work gate. Phase 005 adds the **validation boundary and the API description**: one schema
> declaration that the runtime enforces and the OpenAPI document publishes — the same value, so
> they cannot disagree — RFC 9457 `application/problem+json` failures with a closed error registry,
> **OpenAPI 3.2.0** generation proven against the official schema, a semantic compatibility gate,
> bounded cursor pagination contracts, and `renvor openapi`. It runs, and it is tested.
>
> **The honest limits, which are several.** There is still no database adapter and no
> authentication. Pagination and filtering define **contracts and ports only** — nothing queries
> anything. The transport lives behind an **off-by-default** feature, so a default build resolves
> none of it. And because **nothing is published**, a project the generator produces cannot yet
> depend on the framework — it records its transport choice and documents the dependency to add
> later, rather than emitting one that would not resolve. `renvor routes` and `renvor openapi`
> therefore succeed against **no generated project**: the relays are implemented and tested
> end to end against a real binary, and their reach across generated projects is zero because
> nothing is published for them to depend on.
>
> **Every API is explicitly unstable**, and Phase 005 does **not** change that. The instability
> window has two closure conditions in [`contracts/api-stability.md`](contracts/api-stability.md);
> a real transport exercising the surface satisfies the **first**. The second requires an accepted
> decision record that supersedes ADR-0002, and none exists. **The window is open.**
> **Do not adopt Renvor for anything yet.**

Renvor is a Rust framework, currently in Phase 005 of its development programme.

## The command is `renvor`

The product, the facade crate, and the installed executable all share one spelling:
**`renvor`**. `cargo install renvor-cli` will install a binary named `renvor`.

An earlier decision named the executable `renover`, deliberately distinct from the product.
That is no longer the case: [ADR-0010](decisions/0010-unify-product-and-executable-name.md)
supersedes [ADR-0001](decisions/0001-public-naming-and-namespace.md) and records why,
including the alternatives that were rejected.

| Thing | Name |
|---|---|
| Product, framework, organization | `Renvor` |
| Facade crate | `renvor` |
| CLI crate | `renvor-cli` |
| **Executable you type** | **`renvor`** |
| Project state directory | `.renvor/` |
| Environment prefix | `RENVOR_` |

## Governance and policy

Everything below is one link from here.

| Document | What it covers |
|---|---|
| [**CONSTITUTION.md**](CONSTITUTION.md) | **Supreme authority — version 3.0.1, ratified 2026-08-11, last amended 2026-08-19.** The principles every other document answers to |
| [GOVERNANCE.md](GOVERNANCE.md) | Who decides what, decision records, the reviewer definition, waivers, and how to amend |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute, the one verification command, and the dependency policy |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Expected behaviour and enforcement |
| [SECURITY.md](SECURITY.md) | Private vulnerability reporting, response times, and disclosure |
| [SUPPORT.md](SUPPORT.md) | Supported Rust versions and platforms, and the rules for changing them — a summary of [`contracts/support-policy.md`](contracts/support-policy.md), which is normative |

Supporting records: the [waiver ledger](governance/waivers.md), the
[decision records](decisions/), and the [name availability evidence](governance/name-availability.md).

## Verification

One command, identical locally and in CI:

```sh
cargo xtask verify
```

The ordered steps are defined by [`contracts/verification-sequence.md`](contracts/verification-sequence.md),
which is the normative list — this page does not keep a second copy of the numbering. The
categories it covers: toolchain probe, formatting, lint, tests, API documentation, dependency
and licence policy, architecture invariants, secret scanning, documentation build, link
checking, and working-tree cleanliness.

**A check that cannot run is a failure, never a skip.** If required tooling is missing the
command exits `2`, names every missing tool with its install command, and prints
`no checks were run` — because a partial run reported as success is the failure mode the
whole sequence exists to prevent.

| Exit code | Meaning |
|---|---|
| `0` | Every step ran and passed |
| `1` | A step ran and failed |
| `2` | Required tooling missing — no steps ran |
| `3` | Working tree dirty after an otherwise successful run |

## Support

**MSRV: 1.94.0** — a fixed support floor, not a rolling offset from stable. A new Rust
release does not invalidate it. CI tests exactly two toolchains: the pinned `1.94.0` and
the current stable channel, resolved by CI at run time.

**Supported platforms: Linux, macOS, and Windows.** Six platform/toolchain contexts run on
every pull request. **Only the two Linux contexts — `verify (1.94.0)` and `verify (stable)` —
are required by branch protection**; the four `platform (…)` contexts are executed evidence,
not enforced gates.

The **normative** statement is [`contracts/support-policy.md`](contracts/support-policy.md).
[SUPPORT.md](SUPPORT.md) is the human-facing summary; any disagreement resolves in favour of
the contract.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### The Renvor marks are not covered by that grant

**The brand assets in `assets/` are NOT licensed under `MIT OR Apache-2.0`.** They are
present so this repository can identify itself, and for no other purpose:

| File | Role |
|---|---|
| `assets/renvor-lockup-v40-dark.svg` | The approved v40 lockup shown at the top of this file |
| `assets/renvor-lockup-v21-light.svg` | The superseded v21 lockup for a light background |
| `assets/renvor-lockup-v21-dark.svg` | The superseded v21 lockup for a dark background |
| `assets/renvor-mark-v7.svg` | The superseded v7 mark. **Retained deliberately**: `governance/phase-001-evidence.md` and ADR-0006 record its presence in this repository as dated evidence, and deleting it would make a checkable claim uncheckable |

The dual licence above covers Renvor's **source and documentation**. It does not grant any
right to use, reproduce, or modify the mark. **No implied trademark or brand licence is
granted**, and the presence of the file in a permissively licensed repository must not be
read as one.

**Usage terms were decided on 2026-08-12 (T098): all rights reserved, under a written brand
usage policy.** The mark, the name **Renvor**, and Renvor's wordmarks,
illustrations, and visual identity are excluded from the code licences above.

**Permitted without asking**: truthful nominative references ("built with Renvor",
"compatible with Renvor"), links to the official project, use of the unmodified mark as a link
back to it, screenshots, tutorials and reviews — **including criticism** — and community
discussion.

**Ask first** (`admin@ahmedanbar.dev`): naming a fork or derivative "Renvor" or something
confusingly similar, confusingly similar logos, endorsement or official-status claims,
merchandise, modifying the marks, and company or product names incorporating Renvor.

The test: *could a reasonable person conclude your project **is** Renvor, is an official part
of it, or is endorsed by it?* If yes, ask first.

**You may fork the code — give the fork its own name.** Saying a fork is "based on Renvor" is
truthful and permitted; calling it Renvor is not.

This notice exists because ADR-0005 identified the real risk here as an *unintended*
licensing claim. Stating the exclusion explicitly is what makes the claim intended and
bounded rather than accidental. Apache-2.0 §6 already withholds trademark rights; this states
plainly what that means in practice.

### Code Renvor generates for you is yours

**Project code generated for you by Renvor tooling carries no Renvor licensing
obligation.** It is yours outright, to license however you choose — including
commercially, including under a proprietary licence. Generated output must not embed a
Renvor licence header implying otherwise.

The dual licence above governs **Renvor's own source and documentation**, not the
applications you build with it.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for
inclusion in this project by you, as defined in the Apache-2.0 licence, shall be dual
licensed as above, without any additional terms or conditions.
