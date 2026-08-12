<p align="center">
  <img src="assets/renvor-mark-v7.svg" alt="Renvor" width="120">
</p>

<h1 align="center">Renvor</h1>

> ## ⚠️ Pre-release — Renvor does not work yet
>
> **This project ships no runtime capability.** The published `renvor` crate exposes
> three constants and nothing else. There is no framework here to use.
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

**Usage terms for the mark have not been decided yet** and are tracked as open task
**T098**. Until they are recorded, treat the mark as all rights reserved: do not use it to
identify your own project, product, or fork, and do not use it in a way that implies
endorsement or affiliation.

This notice exists because ADR-0005 identified the real risk here as an *unintended*
licensing claim. Stating the exclusion explicitly is what makes the claim intended and
bounded rather than accidental.

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
