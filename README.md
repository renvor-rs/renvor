<p align="center">
  <img src="assets/renvor-mark-v7.svg" alt="Renvor" width="120">
</p>

<h1 align="center">Renvor</h1>

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
> injection harness. It runs, and it is tested.
>
> It is also **transport-independent**, which is the honest limit: there is no HTTP server,
> no database adapter, no CLI, and no way to receive a request. You can start and stop an
> application; you cannot yet serve anything with one.
>
> **Every API is explicitly unstable (FR-036)** and will change once the first real transport
> adapter exercises it. **Do not adopt Renvor for anything yet.**

Renvor is a Rust framework, currently in Phase 002 of its development programme.

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
| [**CONSTITUTION.md**](CONSTITUTION.md) | **Supreme authority — version 1.0.0, ratified 2026-08-11.** The principles every other document answers to |
| [GOVERNANCE.md](GOVERNANCE.md) | Who decides what, decision records, the reviewer definition, waivers, and how to amend |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to contribute, the one verification command, and the dependency policy |
| [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) | Expected behaviour and enforcement |
| [SECURITY.md](SECURITY.md) | Private vulnerability reporting, response times, and disclosure |
| [SUPPORT.md](SUPPORT.md) | Supported Rust versions and platforms, and the rules for changing them |

Supporting records: the [waiver ledger](governance/waivers.md), the
[decision records](decisions/), and the [name availability evidence](governance/name-availability.md).

## Verification

One command, identical locally and in CI:

```sh
cargo xtask verify
```

Ten ordered steps: toolchain probe, formatting, lint, tests, API documentation, dependency
and licence policy, secret scanning, documentation build, link checking, and working-tree
cleanliness.

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

## Supported Rust versions

**MSRV: 1.94.0** — a fixed support floor, not a rolling offset from stable. A new Rust
release does not invalidate it. CI tests exactly two toolchains: `1.94.0` and current
stable. Full policy in [SUPPORT.md](SUPPORT.md).

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
| `assets/renvor-lockup-v21-light.svg` | The v21 lockup shown at the top of this file on a light background |
| `assets/renvor-lockup-v21-dark.svg` | The same lockup for a dark background |
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
