# ADR-0008: Grow the publishable crate set to four and rehearse releases workspace-wide

| Field | Value |
|---|---|
| **ID** | 0008 |
| **State** | `proposed` |
| **Reviewer** | *(none — see the acceptance note below)* |
| **Review date** | *(none)* |
| **Superseded by** | — |

> **This record is `proposed` and must not be marked `accepted` in Phase 002.**
>
> Constitution §Development and Phase Workflow #4 requires a consequential decision to be
> *captured as a proposed ADR and reviewed before being treated as accepted*. Capturing it is
> what this file does. Accepting it requires an independent review, and **W-004 covers ADR-0007
> alone** — it confers no authority over this record. Spec **FR-035 does not require acceptance
> here** either, because a packaging decision is not custom infrastructure chosen over a
> maintained package.
>
> Phase 002 therefore implements this decision while the record stays `proposed`, and carries it
> forward as a **named open item** in `governance/phase-002-evidence.md`. That is the honest
> position: the decision is made and visible, and the review it still owes is recorded as owed.

## Context

Through Phase 001 the workspace had exactly **one** publishable package — the facade `renvor` —
and `xtask`, which is `publish = false` internal tooling. `crates/renvor/Cargo.toml` recorded
that state in a comment:

> *"No dependencies. A publishable package may not carry a git or path dependency (FR-040);
> declaring none is the strongest form of compliance."*

That was true, and it stops being true in Phase 002. ADR-0002 anticipated exactly this — *"Later
phases add implementation crates behind it"* — and Phase 002 adds three: `renvor-core`,
`renvor-config`, and `renvor-testkit`. The facade's whole purpose is to re-export them.

The Phase 001 release rehearsal (`.github/workflows/release-dry-run.yml`, and the procedure table
in `contracts/package-metadata.md`) was written around the
one-crate assumption and runs `cargo publish -p renvor --dry-run`. It triggers on `crates/**`,
`Cargo.toml`, and `Cargo.lock` — every path Phase 002 touches.

**The problem was found by running the command, not by reading about it.** A four-case experiment
on cargo 1.94.0:

| Case | Command | Dependency `publish` | Result |
|---|---|---|---|
| 1 | `cargo publish -p facade --dry-run` | `false` | **fails** — *no matching package found, location searched: crates.io index* |
| 2 | `cargo publish -p facade --dry-run` | `true` | **fails identically** |
| 3 | `cargo publish --dry-run --workspace` | `true` | **succeeds** |
| 4 | `cargo publish --dry-run --workspace` | `false` | **fails** |

Case 2 is the one that matters. The obvious fix — "mark the new crates publishable" — does **not**
rescue the single-crate rehearsal, because the failure is about the dependency's *presence on the
registry*, not about its `publish` flag. Reproduced afterwards against the real crates: the old
form fails with `no matching package named renvor-config found`; the new form stages all four and
aborts each upload due to dry run.

## Decision

1. **`renvor-core`, `renvor-config`, and `renvor-testkit` are `publish = true`**, each carrying the
   complete metadata set required by the Phase 001 package-metadata contract — `description`,
   `documentation`, `readme`, `keywords`, `categories`, and an explicit `include`.
2. **The facade depends on them as `{ path, version }`**, never path-only. Phase 001 FR-040 forbids
   a publishable package from depending on a **path-*only*** dependency; `path` + `version` is the
   form cargo rewrites at publish time, and is compliant. Omitting `version` would breach FR-040.
3. **`renvor-config` is optional**, behind a default-on `config` feature, so a consumer can take
   `renvor` with `default-features = false` and resolve none of the parser, derive, or secret
   crates. Asserted with a positive control in both directions.
4. **The release rehearsal moves to workspace-wide commands** — `cargo package -p <crate> --list`
   per crate, `cargo package --workspace`, `cargo publish --dry-run --workspace`.
5. **`xtask` stays `publish = false`** and is excluded automatically. The rehearsal now *asserts*
   that exclusion rather than assuming it, so a future manifest edit that made the verification
   runner shippable would fail the check.
6. **Publication order is dependencies before dependents**: `renvor-core`, `renvor-config`,
   `renvor-testkit`, `renvor`.

**Nothing here publishes anything.** `publish = true` is a manifest attribute stating that a crate
*may* be published. Phase 002 ends with **0** crates, **0** tags, and **0** releases (FR-034),
asserted independently.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Keep the facade dependency-free; let users depend on `renvor-core` directly | ADR-0002 rejects precisely this: *"Every internal reorganisation becomes a breaking change for users."* The facade exists to absorb that churn |
| Mark the three new crates `publish = false` | Case 4 above: a publishable crate cannot depend on an unpublishable one. It would make `renvor` unshippable, and would also make `renvor-testkit` undeliverable, since plan DAG property 4 has authors add it under `[dev-dependencies]` |
| Mark the facade `publish = false` for this phase | Contradicts ADR-0002's publishable table, and hides the problem rather than solving it. The rehearsal would pass by rehearsing nothing |
| Add `--no-verify` to the rehearsal | It would make the rehearsal pass by checking **less** — the exact failure mode the gate exists to prevent |
| Defer the facade re-exports to Phase 003 | Phase 002's public surface would be unusable, and the problem would arrive anyway, one phase later and with more code built on the wrong assumption |
| Supersede ADR-0002 | Nothing in ADR-0002's *decision* is wrong; it explicitly anticipated implementation crates. Only one traceability line — *"the facade declares zero git or path dependencies"* — was a Phase-001-scoped consequence, and it is recorded here as superseded in fact |

## Consequences

**Accepted costs.**

- The publishable surface is now **four crates**, so a release is a four-step ordered publication
  with registry-availability waits between steps, not one command. `RELEASING.md` will need that
  ordering before the first real release.
- Three more crates now carry the full metadata burden, and three more can fail metadata
  validation.
- The rehearsal is slower: it packages and verifies four crates instead of one.
- A consumer taking `renvor` with **default features** gets `renvor-config` and its dependencies.
  Isolation is **opt-in** via `default-features = false`, not the default. This is stated in the
  plan rather than glossed, because the reverse is the natural assumption.

**What becomes locked in.** The crate names `renvor-core`, `renvor-config`, and `renvor-testkit`
become public dependency surface the moment anything is published. Renaming one after that is a
breaking change for every consumer.

**What would have to change to reverse this.** Merging the three crates back into the facade, or
making them private again, would require re-superseding this record and re-writing the rehearsal —
and, after any publication, yanking rather than deleting.

**What stops being true.** ADR-0002's traceability line *"the facade declares zero git or path
dependencies"* is superseded in fact by this record. ADR-0002's decision is untouched.

## Compliance

| Rule | How this satisfies it |
|---|---|
| **ADR-0002** | Implements its own provision that later phases add implementation crates behind the facade; the facade remains re-export-only |
| **Phase 001 FR-040** | No **path-only** dependency in any publishable package — every intra-workspace dependency carries `path` **and** `version` |
| **Phase 001 FR-039** | Every new crate states its shipped file set with an explicit `include` |
| **Phase 002 FR-034** | Nothing is published; `publish = true` is a manifest attribute, and 0 crates, tags, and releases are asserted at T110 |
| **Constitution principle VIII** | The `config` feature makes dependency isolation reachable and testable, with a positive control proving the test can detect the dependencies when they *are* present |
| **Constitution §Workflow #4** | Captured as a **proposed** ADR before being treated as accepted; acceptance deliberately withheld, since W-004 does not reach this record |
| **Constitution principle XII** | The limit of the isolation claim is stated rather than implied |
