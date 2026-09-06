---
description: "Contract C-4 — template delivery, rendering bounds, and containment"
version: "1.2.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. 1.2.0 (2026-09-05, Phase 011 correction round): the provenance record is written after verification, digests marked files without their block, and carries `[[resource]]` definitions; the snapshot policy pins the paths of `Cargo.lock` and the record but not their digests. 1.1.0 (2026-09-05, Phase 011): adds the starter template groups and the VERBATIM files a starter copies (the framework's embedded migration sets), the snapshot stability policy, and the provenance record `.renvor/generated.toml`; every bound and containment rule is unchanged. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract C-4 — Templates

**Status**: defined before implementation. Governs FR-024 to FR-028 and FR-040.

## Delivery

**Embedded in the executable. There is no archive path, local or remote.**

This is the clarified decision, and it has a consequence worth stating: the zip-slip and
decompression-amplification defences that would otherwise be required here are **not** implemented,
because the capability they defend does not exist. FR-040 asserts that absence **structurally** —
the built executable carries no archive-extraction capability — which is testable. Hardening a code
path that does not exist is not.

If a later phase introduces archives, those defences become that phase's requirement, and this
contract is the trigger.

## Versioning

`TemplateSet::version` is recorded in every generated `renvor.toml`. Two generations from the same
generator version, template version, and configuration produce identical manifests (SC-016).

## Rendering environment

| Property | Rule |
|---|---|
| **Undefined variable** | **Error.** Never an empty rendering (FR-028) |
| **Filesystem access** | Absent from the environment, not disabled in it |
| **Process execution** | Absent |
| **Network access** | Absent (FR-043) |
| **Filters and functions** | Allow-listed by the application. Deny-by-default, per constitution VI |

"Absent rather than disabled" is the load-bearing phrase. A disabled capability is one configuration
mistake away from being enabled; an absent one is not.

## Bounds

Every bound has a documented value and a test that demonstrates it holds (FR-026, SC-013).

| Bound | Applies to |
|---|---|
| Maximum recursion depth | Template inclusion and expansion. **Declared, and unreachable in this feature set**: `multi_template` and `macros` are off, so `{% include %}` is not a statement the compiled grammar knows and an entry using it is refused when the catalogue **loads**. There is therefore no over-bound test, and `render.rs::the_recursion_bound_has_no_reachable_trigger_and_that_is_the_point` fails if either feature is ever enabled. |
| Maximum total output bytes | The whole render |
| Maximum output file count | The whole render |
| Maximum single-file output bytes | Any one rendered file |

Exceeding any bound produces `bound_exceeded` with `details.bound` and `details.limit`, exit `3`, and
**an untouched destination** — the render is still inside the staging directory when it fails.

## Verbatim files (Phase 011)

A starter copies the framework's authentication and job-store migration sets into its
`migrations/` directory. Those files are **SQL, not templates**: they are embedded in the crates
that own them (`renvor_auth::migrations`, `renvor_jobs::migrations`, each proven equal to the
files on disk by that crate's own test) and written **byte for byte**, never through the template
engine — a `{{` in a SQL comment must not be a parse error, and an undefined name must not be a
refusal. They obey every rule in this contract that a rendered entry obeys: the same path rules at
load time, the same file and byte bounds, the same manifest entry.

The generator still reads **nothing** from the framework checkout it was pointed at except two
files it validates (`Cargo.toml`, to prove it is the Renvor workspace) and one it copies
(`Cargo.lock`, so the starter resolves from the framework's own pins). It evaluates nothing.

## Starter sets (Phase 011)

A starter is rendered from template **groups** selected by the configuration, so the tree carries
exactly what the selection needs and nothing inert:

| Group | Present when | Carries |
|---|---|---|
| base | always | `Cargo.toml`, `src/main.rs`, `src/app.rs`, `src/config.rs`, `src/routes.rs`, `config/http.toml`, `.env.example`, `.gitignore`, `README.md`, `tests/starter.rs` |
| database | a database is selected | `migrations/README.md`, so the directory the provider loads at Boot exists even before the first migration |
| example domain | `--example-domain` | `src/domain.rs`, the item repository (`src/persistence.rs`, or `src/entity.rs` and `src/repository.rs` under SeaORM), and the item migration pair — a repository over no table would be an inert file |
| seed data | `--seed-data` | `src/seed.rs`: the seeds and the provider that applies them at Boot, after the database and before the HTTP server |
| auth | `--auth session` | `src/auth.rs`, `config/auth.toml`, and the framework's authentication migration set, verbatim |
| capabilities | any capability | `src/capabilities/mod.rs`, then one module, one `config/<section>.toml`, and for `jobs` the job-store migration set, per selected capability |
| container | `--container` | the container profile, minus the `.env.example` the base already carries |

Starter groups render with block trimming (a line holding only a block tag leaves no line
behind), so a conditional block costs no blank line. Three rules keep the rendered tree
byte-stable and `rustfmt`-clean for **every** selection and **every** valid name:

- **No Rust line's width depends on the project name.** The name is bound once per file
  (`const NAME`) and used by reference; the generated test locates its binary beside itself
  rather than through a `CARGO_BIN_EXE_<name>` literal. Names are bounded at 64 characters, and
  every line that carries the literal fits at that bound.
- **A construct that `rustfmt` lays out differently when a branch is absent is written once per
  variant** — a signature that loses a parameter, a match arm that loses a statement, a list that
  fits on one line — rather than assembled from fragments.
- **No blanket allowance silences a lean variant.** A helper whose callers are all conditional is
  emitted under the exact condition of its callers; an unused parameter is renamed, not allowed.

### Generated-on-demand files (`renvor generate`)

A resource module, its migration pair, and its test are rendered from the `generate` templates
with the user's names in the context, so their line widths are not the template author's to
decide. Those Rust files are laid out by the toolchain's `rustfmt` **at generation**, before they
are planned, and a missing `rustfmt` is `tool_missing`; the starter templates above stay
hand-formatted, and `cargo fmt --check` at generation stays their proof. The generated
`tests/support/mod.rs` is compiled into every test binary that declares it and each uses a
subset, so it carries a reasoned `#![allow(dead_code)]` — the one allowance in a generated tree,
stated in the file.

## Snapshot stability policy (Phase 011)

A generated tree's **manifest** — its sorted paths and digests — is the thing a snapshot pins,
per template version, in `crates/renvor-cli/tests/snapshots/`. The policy:

| Rule | Consequence |
|---|---|
| a snapshot changes **only** together with a `templates::VERSION` bump | a body edit that leaves the version alone fails the snapshot, and the failure names the version constant |
| CI runs with `INSTA_UPDATE=no` and `INSTA_FORCE_PASS` unset | a drift fails; nothing rewrites a snapshot on a runner |
| `cargo insta review` is the one update path | a reviewer sees the old and the new manifest side by side before either is accepted |
| `Cargo.lock`'s digest is excluded from the pinned set; its path is pinned | it is resolved, not rendered, and differs by machine |
| `.renvor/generated.toml`'s digest is excluded from the pinned set; its path is pinned | the record lists `Cargo.lock`'s digest, so it differs by machine the way the lockfile does; a template drift still fails through the digest of the file that drifted (2026-09-05, Phase 011 correction round) |

## The provenance record (Phase 011)

Every generated project carries `.renvor/generated.toml`: the generator and template versions and,
for every file generation produced, its path and SHA-256. It is itself generated and appears in the
manifest. It exists so a later generator — `renvor generate`, or an upgrade — can tell an untouched
file from one the user changed **without downloading or evaluating anything**, and it records
digests only, never contents.

Three rules were made explicit by the Phase 011 correction round (2026-09-05):

| Rule | Why |
|---|---|
| The record is written **after** verification and before the manifest | verification resolves `Cargo.lock` — pruning a starter's seeded lock, creating a skeleton's — and the record must digest the lockfile that is placed |
| For the two **marked** files (`src/resources/mod.rs`, `src/routes.rs`) the digest is taken over the file with the lines between its markers removed | the block is the generators' shared zone; a marker edit must not turn the user's lines outside it into generator-owned bytes, and a filled block must not read as a user change |
| One `[[resource]]` per `renvor generate resource` run: `name` and `fields` exactly as given | a digest cannot say what a module was rendered from, and `renvor generate auth` renders every recorded resource again with the session guards |

## Output paths

Every template entry's output path is relative and contained. An entry whose rendered path would
escape the staging root is a **load-time** error, so such an entry cannot exist in a shipped binary.
