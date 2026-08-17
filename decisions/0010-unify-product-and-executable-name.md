# ADR-0010: Name the executable `renvor`, matching the product

| Field | Value |
|---|---|
| **ID** | 0010 |
| **State** | `proposed` |
| **Reviewer** | *(required to enter `accepted`)* |
| **Review date** | *(required to enter `accepted`)* |
| **Superseded by** | — |
| **Supersedes** | **ADR-0001** |

> **A record MUST NOT be marked `accepted` without a recorded independent review**
> (spec FR-013). This record is `proposed`. It is a **Phase 001 public-identity decision
> record** and therefore falls inside the live literal scope of **W-002**; the scope analysis
> that establishes this is in §Waiver authority below, and it is stated explicitly rather
> than assumed, because a waiver applied one record wider than it was granted is a governance
> failure dressed as a formality.

## Context

Phase 001 froze the public identity in **ADR-0001**, which chose two permanently distinct
strings: the product `Renvor`, and the installed executable **`renover`**. That record is
careful, states its reasoning, and was accepted under W-002 on 2026-08-12. Nothing about it
was careless, and this record does not treat it as such.

Two things have changed since.

**First, the evidence has accumulated.** ADR-0001 named the `renover`/`renovate` proximity as
residual risk **R-2** and accepted it. It also rejected the alternative *"rename the product to
`Renover` so product and command match"* with this reason:

> *"moves the collision rather than removing it, since `Renover`/`renovate` is a closer pair
> than `Renvor`/`renovate`."*

That claim is correct, and it is now measured rather than asserted. Levenshtein distance,
computed 2026-08-17:

| Pair | Distance |
|---|---|
| `renover` ↔ `renovate` | **3** |
| `renvor` ↔ `renovate` | **4** |
| `renvor` ↔ `renover` | 2 |

**ADR-0001 used this exact comparison to protect the product name, and then left the
executable — the string a user actually types, dozens of times a day — sitting one step
*closer* to the collision it had just identified.** The argument was applied to the name that
appears in prose and withheld from the name that appears in a shell. That is an internal
inconsistency in ADR-0001, not a new fact, and it is the strongest single ground for this
record.

**Second, the cost of changing is currently zero, and will never be this low again.** ADR-0001
recorded, as an accepted cost, that *"any future consolidation to a single name is now a
breaking change for installed commands and published documentation."* Measured 2026-08-17,
there are no installed commands and no published documentation:

| Fact | Evidence |
|---|---|
| No executable has ever shipped | `crates/renvor-cli` does not exist; Phase 003 is the phase that creates it, and it has not started |
| `renvor-cli` is not on crates.io | `GET https://crates.io/api/v1/crates/renvor-cli` → **HTTP 404** |
| No release or tag exists | `renvor-rs/renvor` has **0** tags and **0** releases |
| No documentation site is deployed | `docs.renvor.dev` → **HTTP 404**; `renvor-rs/renvor-docs` is commit-empty |
| No landing site is deployed | `renvor.dev` → **HTTP 404** at the time this record was written |

So the migration this record performs is a **pre-release source migration**. There is no
installed user to break, and consequently **no compatibility bridge, alias, deprecation
shim, or transition period is provided** — providing one would mean supporting a spelling
that has never existed in anyone's `PATH`.

**Third — and this is the plainest argument — the explanation burden ADR-0001 accepted is
avoidable.** ADR-0001 wrote it down honestly:

> *"A permanent explanation burden. Every new contributor and user may read `renover` as a
> misspelling."*

A permanent cost is worth paying for a permanent benefit. The benefit ADR-0001 named was
"the distinction between the framework you depend on and the tool you run". That distinction
is not one the Rust and adjacent ecosystems consider worth a second string: `cargo`,
`rustup`, `deno`, `bun`, `uv`, `ruff`, and `just` are each simultaneously a project name, a
package name, and a command. Users disambiguate from context — `cargo add renvor` and
`renvor new` are no more ambiguous than `cargo install cargo-edit` and `cargo add`.

## Decision

**The product, the facade crate, and the installed executable share one spelling: `renvor`.**

| Role | Value | Appears in |
|---|---|---|
| Product, framework, organization | **`Renvor`** | Prose, branding, the GitHub organization `renvor-rs`, the domain `renvor.dev` |
| Facade package | **`renvor`** | crates.io, `use renvor::…` |
| CLI package | **`renvor-cli`** | crates.io |
| **Installed executable** | **`renvor`** | The command a user types; all help text, examples, error messages, and documentation |
| Primary command | **`renvor new`** | Documentation and templates |
| Package command | **`renvor add`** | Documentation and templates |
| Project state directory | **`.renvor/`** | Generated projects |
| Environment prefix | **`RENVOR_`** | Configuration |

**`cargo install renvor-cli` will install a binary named `renvor`.** The package name and the
binary name differ, which is ordinary Rust packaging (`cargo install ripgrep` installs `rg`;
`cargo install cargo-edit` installs three binaries named after none of it), and is declared in
`crates/renvor-cli/Cargo.toml` with a `[[bin]] name = "renvor"` entry when that crate is
created in Phase 003. **This record does not create that crate.**

**`renover` is retired as an active name.** Existing occurrences are migrated wherever they
state a current fact, and retained only where they record what was previously decided —
in ADR-0001, in the Phase 001 evidence ledger, and in the name-availability record — each
with explicit supersession context so a reader cannot mistake history for instruction.

**ADR-0001 becomes `superseded`, superseded by this record.** Its decision body is preserved
verbatim as historical evidence. Superseding a record does not entitle anyone to rewrite what
it said; the Phase 001 evidence that cites it must remain checkable.

## Waiver authority

**This record is covered by W-002, and by nothing else.** The reasoning is set out here rather
than assumed, because the maintainer's instruction was explicit that W-002 applies *only if its
live literal scope still covers Phase 001 public-identity decision records*.

W-002's live scope, read from `governance/waivers.md`:

| Where | What it says |
|---|---|
| Waiver row, Reason | *"no genuinely independent review of a **Phase 001 decision record** is available"* |
| Axis table | `W-002 \| decision record (FR-013) \| **Phase 001**` |
| Ruling of 2026-08-11 | *"The reviewer field of **every Phase 001 decision record**…"* |
| W-004's own row | *"**W-002 covers Phase 001 decision records only** and does not reach a Phase 002 ADR"* |

The question is therefore exactly one thing: **is ADR-0010 a Phase 001 decision record?**

It is, and the Phase 001 contract says so itself. `specs/001-governance-foundation/contracts/public-identity.md`
closes with:

> *"Changing any value after Phase 001 closes requires a **superseding ADR** and an impact
> analysis covering manifests, templates, documentation, published links, and any external
> reference."*

**A superseding ADR is the instrument the Phase 001 contract names for this purpose.** This
record is that instrument, amending that contract, in the FR-013 domain, superseding a Phase 001
record, and deciding nothing outside the Phase 001 public-identity contract. Its phase
attribution follows its subject matter — which is how the ledger's own axis table attributes
every other record — and not the calendar date on which it was typed. ADR-0006 is the
precedent: a Phase 001 record accepted under W-002 on **2026-08-15**, days after Phase 001's
implementation work had finished.

**The counter-argument, stated so it is on the record rather than omitted:** one could read
"Phase 001 decision record" temporally, as "a record written during Phase 001", which would
exclude this one and require a new waiver. That reading is rejected because it would also have
excluded ADR-0006, because it makes the contract's own "superseding ADR" clause unusable
without a fresh waiver every time, and because the ledger consistently scopes waivers by
subject and phase rather than by authorship date. **The reading is recorded, not hidden, so a
future independent reviewer can overturn it if they disagree.**

**No new waiver is created.** W-007 does not exist. W-004 (ADR-0007 only) and W-006 (ADR-0009
only) are each scoped to a single named record and confer no authority here; neither is
extended, reinterpreted, or borrowed. The waiver count stays at **six**, and the Phase 002
exception count stays at **three** — this record adds nothing to either, because it is a
Phase 001 record operating under a Phase 001 waiver that has been open since 2026-08-11.

**W-002's four compensating controls apply unchanged**, and the acceptance gate below records
each one against measured evidence rather than intent.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Keep `renover`** — leave ADR-0001 in force | Preserves a permanent explanation burden that ADR-0001 itself named, in exchange for a distinction the ecosystem does not draw, while leaving the typed command **measurably closer to `renovate`** (3) than the product name is (4) — the precise harm ADR-0001 invoked to reject a different alternative. The cost of removal will never be lower than it is today, with zero installed users. |
| **Rename the product to `Renover`** so both match on that spelling | Discards `renvor.dev` (owned, registered to 2027-08-11), `renvor-rs`, the `renvor-` crate prefix, four existing crate names, and the entire v21 brand identity. Moves the collision *toward* `renovate` rather than away. This was ADR-0001's own rejection and it remains correct. |
| **Executable named `renvor-cli`**, matching the package | Verbatim from ADR-0001, and still right: long to type for the project's most frequent interaction, and it leaks packaging detail into the user interface. Users type commands, not package names. |
| **A short unrelated executable** (`rvr`, `rnv`) | Breaks the association between product and command entirely; short names collide far more readily in `PATH`; and neither string has been verified free. Verifying and defending a third identity buys nothing that `renvor` does not already provide. |
| **Ship both `renvor` and `renover`**, one aliasing the other | Two commands to document, test, and support, permanently, so that neither reads as authoritative — and it *preserves* the near-homograph pair inside the project rather than removing it. There is also nothing to be compatible *with*: no `renover` binary has ever been installed by anyone. |
| **Defer to Phase 003**, when the CLI is actually built | Phase 003's specification, tasks, and templates are written against whichever name is current when they are generated. Deciding after those artifacts exist means migrating them too, and doing it while Phase 003 is mid-flight. The cheapest moment to change a name is before the thing it names exists — that moment is now, and it closes when Phase 003 opens. |

## Consequences

**What this buys:**

- One spelling to learn, type, search, and document. The question *"is that a typo?"* stops
  being asked, so the canonical answer stops needing to exist.
- Residual risk **R-2** is reduced, not merely re-accepted: the typed string moves from
  distance 3 to distance 4 from `renovate`.
- Residual risk **R-3** (bounded clearance of `renover`) is **retired outright**. The name it
  qualified is no longer used, so the unbounded-search caveat that attached to it no longer
  attaches to anything active.

**Accepted costs:**

- **`renvor` names three things** — the product, the facade crate, and the command. Prose must
  occasionally say "the `renvor` crate" or "the `renvor` command" where it previously needed no
  qualifier. This is the cost ADR-0001 declined to pay, and it is paid here deliberately; it
  is a small ongoing wording cost in exchange for removing a permanent explanation burden.
- **The executable name is now cleared only to the same bounded standard.** The eight-probe
  search re-run for `renvor` on 2026-08-17 is exactly as bounded as the original: no global
  registry of executable names exists. One probe's limits are recorded explicitly below.
- **Phase 001 artifacts now carry a supersession layer.** ADR-0001, the public-identity
  contract, and `governance/name-availability.md` each describe a decision that is no longer
  operative, annotated rather than rewritten. A reader must read the annotation to get the
  current answer. That is the honest cost of not falsifying history.
- **Documentation prose loses a distinction it leaned on.** Any sentence that worked by
  contrasting "the framework `renvor`" with "the tool `renover`" must be rewritten to carry
  its meaning some other way.

**What becomes harder:** nothing that has shipped, because nothing has. After Phase 003
publishes `renvor-cli`, a further rename becomes a genuine breaking change for installed
commands — the cost ADR-0001 anticipated, arriving one phase later against a different name.

**What is locked in:** `specs/001-governance-foundation/contracts/public-identity.md` and
`renvor::EXECUTABLE`, which every phase from 002 onward reads.

**To reverse this**, a superseding ADR must record the new name, an impact analysis, and — if
`renvor-cli` has by then been published — a migration path for anyone who installed `renvor`.

## Impact analysis

Required by `contracts/public-identity.md` for any change to a frozen identity value.

| Surface | Impact | Status |
|---|---|---|
| **Public API** | `renvor::EXECUTABLE` changes value from `"renover"` to `"renvor"`. It is a `pub const &str` on an **explicitly unstable** surface (FR-036), so no compatibility procedure is owed. Its doctest and the `executable_differs_from_the_product_name` unit test both assert the old value and are updated with it | Migrated in this change |
| **Crate names** | None. `renvor`, `renvor-core`, `renvor-config`, `renvor-testkit` are unaffected; `renvor-cli` keeps its package name | No change |
| **Generated projects** | `.renvor/` and `RENVOR_` are unchanged — both were already derived from the product name, not the executable | No change |
| **Templates** | None exist. Phase 003 creates them | Not applicable |
| **Documentation** | `README.md`, `crates/renvor/README.md`, `docs/docs/intro.mdx`, `docs/docs/api-reference.mdx`, `docs/src/pages/index.js`, `SECURITY.md` carry active command text | Migrated in this change |
| **Program plan** | `PLAN.md` carries the executable name, the full command surface for Phases 003/025/028, and the Phase 003 `/speckit-specify` prompts | Migrated in this change |
| **Constitution** | Principles VII and XIII name `renover new` and `renover add` **normatively** | Amended to **2.0.0** in this change. `CONSTITUTION.md` is the **only tracked copy** — `.specify/memory/constitution.md` exists for the specification tooling but `.specify/` is gitignored, so it is a local working copy and not a repository mirror. Both were changed together and the amendment record lives in the tracked file |
| **Published links** | None. No crate, release, tag, or deployed site references either spelling | No external reference exists |
| **External references** | None known. The name has never been published or announced | No action |
| **Security** | None. The change is a string; it grants no capability, alters no trust boundary, and touches no dependency | No impact |
| **Future phases** | Phases 003, 025, and 028 own the CLI, generator, and package commands. Each reads the migrated `PLAN.md` and constitution | Migrated ahead of Phase 003 |

## Name-availability evidence

Re-verified **2026-08-17T08:45:34Z** for the executable name `renvor`, using the same eight
probes ADR-0001 used for `renover`, **each with a positive control** — a probe that cannot
find a name that does exist proves nothing about a name that does not.

| # | Probe | Target: `renvor` | Positive control | Control result |
|---|---|---|---|---|
| 1 | crates.io | HTTP **404** | `serde` | HTTP 200 |
| 2 | Homebrew formula | HTTP **404** | `git` | HTTP 200 |
| 3 | Homebrew cask | HTTP **404** | `firefox` | HTTP 200 |
| 4 | npm registry | HTTP **404** | `react` | HTTP 200 |
| 5 | PyPI | HTTP **404** | `requests` | HTTP 200 |
| 6 | Debian sources | **0** exact, **0** other | `bash` | 1 exact, 20 other |
| 7 | Public Rust manifests | **0** | `serde filename:Cargo.toml` | **829,440** |
| 8 | This machine's `PATH` | not present | `ls` | present |

Also re-verified: `renvor-cli` → **404**, `renvor_cli` → **404** on crates.io.

**Probe 7 is bounded, and the bound was found by the control.** A first attempt returned `0`
for both target *and* control; the control exposed it as a **rate-limit artifact (HTTP 403)**,
not a result, and the probe was re-run after the limit reset. Separately, GitHub code search
returns **0** for `renvor filename:Cargo.toml` even though `renvor-rs/renvor` is public and
contains four such manifests — so the index does not cover this repository, and probe 7's `0`
**bounds** the absence rather than proving it. An independent second source (grep.app) also
returned no results. Recorded as a limit of the method, not smoothed over.

**Clearance is `available` (bounded)** on exactly the terms ADR-0001 recorded for `renover`:
no global registry of executable names exists, and BSD ports, Windows package managers, other
Linux distributions, and privately distributed binaries were not checked.

## Compliance

| Authority | How this record satisfies it |
|---|---|
| Constitution — decisions recorded with alternatives | Six alternatives, each with a stated rejection reason, including a verbatim re-affirmation of two of ADR-0001's own |
| Constitution principle X — no claim exceeds measurement | Every availability claim carries a dated probe **and a positive control**; probe 7's limits are stated; the `renovate` proximity claim is computed, not asserted |
| Constitution principle XI — simplicity | Removes one of two names for one concept |
| Constitution §Governance — amendment process | The amendment to 2.0.0 carries a written proposal (this record), an impact analysis (above), a migration plan (this record's Decision), maintainer approval, an updated version and date, and synchronisation of the tooling working copy. The amendment record — including why the bump is MAJOR rather than MINOR or PATCH — is in the tracked `CONSTITUTION.md` itself |
| FR-013 | State, reviewer, and date recorded; acceptance gated on W-002's four controls |
| FR-003 (stop, do not substitute) | The new value was verified free **before** adoption, not assumed |
| FR-049 | `renvor` is verified, not reserved; no placeholder crate was published |
| `contracts/public-identity.md` | Changed by a superseding ADR carrying the impact analysis that contract requires |
| PLAN.md §20 Phase 003 | Decided **before** Phase 003 opens, so no CLI artifact is generated against a name that is about to change |

## Acceptance gate

Acceptance is a **separate, later commit**. This record is pushed `proposed`, W-002's controls
are run against it, and only then is a follow-up signed commit made that sets `accepted`.
Recording acceptance in the same commit that proposes the decision would assert that controls
had passed before they were run — an error made once already in this project, in Phase 002, and
not repeated here.

| # | W-002 compensating control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review completed against the ADR template before acceptance | *(pending)* |
| 2 | Verification against `specs/001-governance-foundation/checklists/governance.md` | *(pending)* |
| 3 | All required CI and security checks passing | *(pending)* |
| 4 | A dated review record stored with the ADR | *(pending)* |

**On acceptance the reviewer field reads exactly `Ahmed Anbar — self-review under W-002`.**
That review is **not independent** and must not be described as such — here, in the evidence
pack, in `GOVERNANCE.md`, or in any public document. It is a structured self-review operating
under a recorded, time-bounded exception that expires **2027-02-11**, or immediately when a
qualified independent reviewer becomes available, whichever comes first. When W-002 closes, the
first qualified independent reviewer re-reviews this record in full, alongside every other
record accepted under it.
