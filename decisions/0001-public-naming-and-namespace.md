# ADR-0001: Name the product `Renvor` and the executable `renover`

| Field | Value |
|---|---|
| **ID** | 0001 |
| **State** | `proposed` |
| **Reviewer** | *(pending — see Acceptance gate)* |
| **Review date** | *(pending)* |
| **Superseded by** | — |

> **Acceptance gate.** This record MUST NOT be marked `accepted` until all four
> compensating controls of waiver **W-002** are satisfied. Three are met; one is not.
> See [Acceptance gate](#acceptance-gate) at the end of this record.

## Context

Phase 001 must freeze the public identity before any manifest, template, document, or
example references it, because every later phase inherits those strings and a rename after
publication breaks installed commands, documentation links, and registry entries.

Verification on 2026-08-11 (`governance/name-availability.md`) established:

- No crate on crates.io contains the string `renvor` — the facade name, the CLI name, and
  the entire `renvor-` prefix are unoccupied.
- `renover` is unclaimed across crates.io, Homebrew, npm, PyPI, and Debian, and no public
  Rust manifest declares it. That clearance is **bounded**, not exhaustive: no global
  registry of executable names exists.
- `renvor-rs` and `renvor-rs/renvor` were free on GitHub and have since been claimed.
- `renvor.dev` is owned by the project.

Two forces shape the naming decision:

1. **The product name and the command name need not be identical**, and conflating them
   costs something real. A single string must simultaneously read well as a brand, survive
   as a crate name, and be typed dozens of times a day.
2. **`renvor` and `renover` differ by a transposition.** So do `renover` and `renovate` —
   the latter being a widely used dependency-update bot on npm and a crate on crates.io.
   A reader encountering both Renvor strings could reasonably conclude one is a typo.

That second force is why this record exists. An unexplained near-homograph pair looks like
a mistake that ossified. If the distinction is deliberate it must be stated, or it will be
"corrected" by a future contributor.

## Decision

The project uses **two distinct names, permanently**:

| Role | Value | Appears in |
|---|---|---|
| Product, framework, organization | **`Renvor`** | Prose, branding, the GitHub organization `renvor-rs`, the domain `renvor.dev` |
| Facade package | **`renvor`** | crates.io, `use renvor::…` |
| CLI package | **`renvor-cli`** | crates.io |
| **Installed executable** | **`renover`** | The command a user types; all help text, examples, error messages, and documentation |
| Primary command | **`renover new`** | Documentation and templates |
| Project state directory | **`.renvor/`** | Generated projects |
| Environment prefix | **`RENVOR_`** | Configuration |

**The executable is `renover`. It is not `renvor`, and this is not a typographical error.**
Every document, test, example, help string, and diagnostic uses `renover` when naming the
command, and `Renvor` when naming the product.

The hosting organization is `renvor-rs`, following the established Rust-ecosystem
convention (`tokio-rs`, `serde-rs`), with the repository at `renvor-rs/renvor`.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Executable named `renvor`, identical to the product and facade crate | Loses the distinction between "the framework you depend on" and "the tool you run" — a distinction the documentation relies on constantly. It also makes `renvor` ambiguous in prose: `cargo add renvor` and `renvor new` would name different things with one word. |
| Executable named `renvor-cli`, matching the package | Long to type for the project's most frequent interaction, and it exposes packaging detail in the user interface. Users type commands, not package names. |
| Rename the product to `Renover` so product and command match | Discards a verified, owned identity — `renvor.dev` is registered and `renvor-rs` claimed — and moves the collision rather than removing it, since `Renover`/`renovate` is a closer pair than `Renvor`/`renovate`. |
| Rename the executable to something unrelated (e.g. `rvr`, `rnv`) | Removes the near-homograph risk but breaks the association between product and command entirely, and short names collide far more readily in `PATH`. `rvr` was not verified as free. |
| Organization named `renvor` rather than `renvor-rs` | Unavailable: the global GitHub login `renvor` is already held by an unrelated **user** account (id 206448205, created 2025-04-06). Per FR-003 the project stops rather than contorting around a held name, and `renvor-rs` was the pre-declared candidate, not a silent substitute. |

## Consequences

**Accepted costs:**

- **A permanent explanation burden.** Every new contributor and user may read `renover` as
  a misspelling. Documentation must state the distinction early, and this record is the
  canonical answer to "is that a typo?".
- **Confusability with `renovate` persists** in both typo directions — the npm bot is far
  more widely installed than this project will be for a long time. A user typing `renover`
  meaning `renovate` gets "command not found"; the reverse is likelier and harmless. This
  is residual risk **R-2**, accepted rather than eliminated.
- **Two names to keep synchronized.** Renaming either later requires a superseding ADR and
  an impact analysis across manifests, templates, documentation, published links, and any
  external reference.
- **`renover` clearance is bounded.** A collision may still exist in an ecosystem not
  checked (residual risk **R-3**). Discovery later triggers the same stop-and-decide rule.

**What becomes harder:** any future consolidation to a single name is now a breaking change
for installed commands and published documentation.

**What is locked in:** the strings above are consumed by
`contracts/public-identity.md`, which every phase from 002 onward reads.

**To reverse this**, a superseding ADR must record the new names, the impact analysis, and
a migration path for anyone who already installed `renover`.

## Compliance

| Authority | How this record satisfies it |
|---|---|
| Constitution — decisions are recorded with alternatives | Five alternatives with stated rejection reasons |
| Constitution principle X — no claim exceeds measurement | The `renover` clearance is explicitly labelled bounded, with its unchecked scope enumerated |
| FR-001 – FR-006 | Every name carries dated evidence with a definite status in `governance/name-availability.md` |
| FR-003 (stop, do not substitute) | The organization name `renvor-rs` was the pre-declared T008 candidate; the held login `renvor` was not worked around silently but recorded as a rejected alternative |
| FR-049 | Registry names are verified, not reserved; no placeholder crate was published |
| PLAN.md Phase 001 | Public identity frozen before any artifact treats it as final |

## Acceptance gate

Waiver **W-002** permits structured self-review in place of independent review, and
requires **all four** compensating controls before any record reaches `accepted`:

| # | Control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template | ✅ Met — five alternatives, costs stated |
| 2 | Verification against `specs/001-governance-foundation/checklists/governance.md` | ⏳ Scheduled at **T086** |
| 3 | **All required CI and security checks passing** | ❌ **Not met** — the workflows producing `verify (1.94.0)`, `verify (stable)`, `security`, and `docs` do not exist until T057–T059, and nothing has been pushed |
| 4 | A dated review record stored with the ADR | ⏳ Recorded when 2 and 3 clear |

**This record therefore remains `proposed`.** Marking it `accepted` now would assert a
review that has not happened, which is precisely the failure W-002 was written to prevent.

When accepted, the reviewer field reads exactly **`Ahmed Anbar — self-review under W-002`**.
This review **must not** be described as independent, here or anywhere else.
