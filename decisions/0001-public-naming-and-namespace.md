# ADR-0001: Name the product `Renvor` and the executable `renover`

| Field | Value |
|---|---|
| **ID** | 0001 |
| **State** | `superseded` |
| **Reviewer** | `Ahmed Anbar — self-review under W-002` |
| **Review date** | 2026-08-12 |
| **Superseded by** | **ADR-0010** *(2026-08-17)* |

> ## Superseded 2026-08-17 by ADR-0010 — the executable is now `renvor`
>
> **The installed executable is `renvor`, not `renover`.** The primary command is
> `renvor new` and the package command is `renvor add`. See
> `decisions/0010-unify-product-and-executable-name.md`.
>
> **Everything below this notice is preserved verbatim as historical evidence.** It records
> what was decided on 2026-08-12 and why, and it is cited by
> `governance/phase-001-evidence.md` and by T026. It is **not** current instruction. No
> sentence below has been edited to agree with the newer decision — superseding a record
> does not entitle anyone to rewrite what it said, and Phase 001 evidence that cites this
> record must stay checkable against the text that was actually accepted.
>
> **What ADR-0010 kept:** the product name `Renvor`, the facade crate `renvor`, the CLI
> package `renvor-cli`, the organization `renvor-rs`, `.renvor/`, and `RENVOR_` — all
> unchanged, and all still decided by the record below. **What ADR-0010 changed:** the
> installed executable and its command names, and nothing else.
>
> **Why**, in one line: ADR-0001 rejected renaming the *product* to `Renover` because
> `Renover`/`renovate` is a closer pair than `Renvor`/`renovate` — measured Levenshtein 3
> against 4 — and then left the *executable*, the string users actually type, at the closer
> spelling. ADR-0010 applies ADR-0001's own argument consistently, at a moment when no
> executable has shipped and the change costs nothing.
>
> The residual risks recorded below have moved: **R-2** (`renovate` confusability) is
> reduced rather than merely re-accepted, and **R-3** (bounded `renover` clearance) is
> retired, because the name it qualified is no longer used.

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

| # | W-002 compensating control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template | ✅ **Met** — five alternatives recorded with rejection reasons, and the accepted costs are stated |
| 2 | Verification against [`checklists/governance.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/001-governance-foundation/checklists/governance.md) | ✅ **Met 2026-08-12** — T086 complete: 77 of 79 items passed, 2 failed as genuine specification gaps (CHK048, CHK050), 0 weakened. No unresolved requirement affects the naming decision — CHK011 through CHK019 all passed. |
| 3 | All required CI and security checks passing | ✅ **Met 2026-08-11** — `verify (1.94.0)` 59s, `verify (stable)` 53s, `security` 43s, `docs` 40s, plus dependency review and CodeQL, all passing on `renvor-rs/renvor` |
| 4 | A dated review record stored with the ADR | ✅ **Met** — this section, dated 2026-08-12 |

**All four controls are met. This record is `accepted`.**

Reviewed by **Ahmed Anbar — self-review under W-002** on **2026-08-12**. This review is
**not independent** and must not be described as such, here or anywhere else. It is a
structured self-review operating under a recorded, time-bounded exception that expires on
2027-02-11 or when a qualified independent reviewer becomes available, whichever is first.
