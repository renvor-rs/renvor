<p align="center">
  <img src="assets/renvor-mark-v7.svg" alt="Renvor" width="120">
</p>

<h1 align="center">Renvor</h1>

> ## ⚠️ Pre-release — Renvor does not work yet
>
> **This project ships no runtime capability, and nothing is published.** Neither `renvor`
> nor `renvor-cli` exists on crates.io — verified against the registry index on 2026-08-12,
> both HTTP 404. The `renvor` crate in this repository exposes three constants and nothing
> else. There is no framework here to use, and **no way to install one.**
>
> What exists today is the *foundation*: governance, verified public names, a pinned
> toolchain, a licence policy, a fail-closed verification sequence, and repository
> security controls — deliberately built before any code, so that the first line of
> functionality lands into a project that can already verify and govern itself.
>
> **Do not adopt Renvor for anything yet.** Nothing here is stable, and no compatibility
> promise applies before a `0.1.0` release.

Renvor is a Rust framework, currently in Phase 001 of its development programme.

## The command is `renover`

The product is **Renvor**. The installed executable is **`renover`**.

That difference is deliberate and permanent — it is **not** a typographical error. The
reasoning, along with the alternatives that were rejected, is recorded in
[ADR-0001](decisions/0001-public-naming-and-namespace.md).

| Thing | Name |
|---|---|
| Product, framework, organization | `Renvor` |
| Facade crate | `renvor` |
| CLI crate | `renvor-cli` |
| **Executable you type** | **`renover`** |
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

### The Renvor mark is not covered by that grant

**`assets/renvor-mark-v7.svg` — the Renvor mark shown at the top of this file — is a brand
asset and is NOT licensed under `MIT OR Apache-2.0`.** It is present so this repository can
identify itself, and for no other purpose.

The dual licence above covers Renvor's **source and documentation**. It does not grant any
right to use, reproduce, or modify the mark. **No implied trademark or brand licence is
granted**, and the presence of the file in a permissively licensed repository must not be
read as one.

**Usage terms were decided on 2026-08-12 (T098): all rights reserved, under a written brand
usage policy.** The mark, the names **Renvor** and **`renover`**, and Renvor's wordmarks,
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
