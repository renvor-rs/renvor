---
description: "Contract C-4 — template delivery, rendering bounds, and containment"
version: "1.3.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. 1.3.0 (2026-09-07, Phase 012, L-2): every generated tree declares its toolchain — `rust-toolchain.toml` (an exact release, the framework checkout's channel for a starter, the generator's MSRV for a skeleton) and `rust-version` (the MSRV) — as a template group gated on the record; the provenance record gains `record_version = 2`, `[toolchain]`, and `[verified_with]` (measured after verification, never derived); readers dispatch on `record_version`; the generator now reads THREE files from the framework checkout it validates (`Cargo.toml`, `rust-toolchain.toml`, and `Cargo.lock`) and one it copies (`Cargo.lock`); a checkout whose pin is malformed, an alias, or below its own MSRV is refused by name; the Dockerfile builder tag derives from the pin. A project rendered at template version 8 is not readable by a generator built before this revision. 1.2.0 (2026-09-05, Phase 011 correction round): the provenance record is written after verification, digests marked files without their block, and carries `[[resource]]` definitions; the snapshot policy pins the paths of `Cargo.lock` and the record but not their digests. 1.1.0 (2026-09-05, Phase 011): adds the starter template groups and the VERBATIM files a starter copies (the framework's embedded migration sets), the snapshot stability policy, and the provenance record `.renvor/generated.toml`; every bound and containment rule is unchanged. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
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

The generator still reads **nothing** from the framework checkout it was pointed at except three
files it validates (`Cargo.toml` and `rust-toolchain.toml`, both parsed and neither evaluated;
`Cargo.lock`) and one it copies (`Cargo.lock`, so the starter resolves from the framework's own
pins). It evaluates nothing. *(1.3.0, Phase 012: `rust-toolchain.toml` is the third file — its
`[toolchain].channel` is the pin a starter inherits, and `Cargo.toml`'s
`[workspace.package].rust-version` is the MSRV it declares, each parsed on its own and then
compared; a checkout whose pin is malformed, a channel alias, or below its own MSRV is refused by
name before anything is staged, per [`command-surface.md`](command-surface.md) §`--framework-path`.
Until 1.3.0 the read bound was two files validated and one copied; the widening is stated here
rather than absorbed.)*

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
| toolchain | at `renvor new`, or when the record declares a pin — **never** for a legacy record (Phase 012, FR-012-10b: a template ≤ 7 tree has no `[toolchain]`, so `generate auth`'s re-render of `Cargo.toml` on it carries no `rust-version` and plans no `rust-toolchain.toml`; nothing is inserted silently) | `rust-toolchain.toml` (`channel = "<pin>"`, `components = ["rustfmt", "clippy"]`, `profile = "minimal"`), and the `rust-version = "<msrv>"` line of `Cargo.toml`. The group renders for **both** tree kinds: a starter's pin is the framework checkout's channel and its MSRV the checkout's `rust-version`; a skeleton's pin and MSRV are both the generator's own `CARGO_PKG_RUST_VERSION`. Template version **8** (1.3.0) |

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
stated in the file. That `rustfmt` runs **under the sealed environment** of
[`generation-transaction.md`](generation-transaction.md) 1.2.0 (Phase 012, FR-012-6): a `rustfmt`
that is a rustup proxy, run in a pinned directory with the operator's environment, could otherwise
install the pinned toolchain; under the seal `RUSTUP_AUTO_INSTALL=0` is forced and the install
variables are absent, so it answers "is not installed" by name instead.

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
| the record's `[toolchain]` table **joins the pinned set** (Phase 012, FR-012-5e) | its two values — `pinned` and `rust_version` — are rendered, not resolved: they are the channel written into `rust-toolchain.toml` and the `rust-version` written into `Cargo.toml`, so the snapshot suite asserts them per template version, and a drift in either fails it. The record's whole-file digest stays excluded as the row above says |
| the record's `[verified_with]` table is **outside the pinned set**, beside `Cargo.lock`'s digest; the record's path stays pinned (Phase 012, FR-012-5e) | its values differ by machine and by run — the compiler that resolved, the instant, the tree digest, the launch counts — the way the lockfile differs by machine; pinning them would make every snapshot machine-specific |

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

### Record version 2 (Phase 012, L-2)

The record gains a format version of its own — **`record_version`**, a different axis from
[`json-output.md`](json-output.md)'s `schemaVersion`: it versions a file the generator owns, and
this contract governs it. Version 2 adds `[toolchain]` (what was rendered) and `[verified_with]`
(what the five checks observed and what the seal queried). The layout, with every field's meaning
beside it:

```toml
record_version = 2               # absent in every record written before this revision (FR-012-5b)
generator_version = "0.0.0"
template_version = "8"

[toolchain]
pinned = "1.94.0"                # the channel rendered into rust-toolchain.toml; "none" for a legacy tree (§5.5)
rust_version = "1.94.0"          # the rust-version rendered into Cargo.toml; "none" for a legacy tree

[verified_with]
operation = "new"                # new | auth — the operation whose five checks this table describes
verified_at = "2026-09-07T00:00:00Z"   # RFC 3339, UTC: the instant the five checks passed
tree_scope = 1                   # the scope rule of FR-012-5d that tree_digest was computed under
tree_digest = "sha256:…"         # over the contents of every in-scope file at that verification (FR-012-5d)
observation = "launched"         # launched | cached | mixed — for the build and test units of the project's own package(s) (FR-012-7d)
rustc_release = "1.98.1"         # the launched rustc's answer to `-vV` (three fields, never one string, D-L2-3); ABSENT when observation = cached;
rustc_commit = "48a229cea"       #   for mixed, the launched units' identity only. Never filled from the pin, PATH, an old record, or .rustc_info.json
rustc_host = "x86_64-unknown-linux-gnu"
resolved_rustc_release = "1.98.1"     # queried: FR-012-7b's `rustc -vV` in the verified directory under the seal — a resolution identity (what rustup/PATH
resolved_rustc_commit = "48a229cea"    #   resolved; the source of the FR-012-8 resolution notice); always present; never an observation, never copied into rustc_*;
                                       #   NOT Cargo's effective compiler under RUSTC, build.rustc, or a wrapper (A-8)
configured_rustc_release = "1.98.1"   # OPTIONAL and labelled: Cargo's configured resolution queried separately (`cargo rustc --bin <name> -- -vV`
configured_rustc_commit = "…"          #   under the seal); a queried configuration identity, never an observation, never copied into rustc_*
cargo_release = "1.98.1"         # queried: `cargo -vV` under the seal
cargo_commit = "…"
rustup = "1.29.0"                # or "absent"
proxy = true                     # the resolved rustc is a rustup proxy (FR-012-7c)
selected_by = "environment"      # environment | directory_override | toolchain_file | default | no_rustup | unknown
rustc_override = false           # RUSTC in the sealed environment, or Cargo's `build.rustc` (presence only)
wrapper = false                  # RUSTC_WRAPPER/RUSTC_WORKSPACE_WRAPPER in the sealed environment, or Cargo's build.rustc-wrapper/build.rustc-workspace-wrapper (presence only)
rustflags = false                # RUSTFLAGS present
rustdocflags = false             # RUSTDOCFLAGS present

[verified_with.checks.clippy]    # one table per check that can launch a compiler (clippy, build, test); fmt and run carry `outcome` only
outcome = "passed"
units_launched = 2               # Cargo `Running` lines for the project's own units
units_fresh = 0                  # units Cargo positively reported `Fresh`
driver_release = "0.1.98"        # the observed clippy-driver EXECUTABLE (the one Cargo's Running line names), from its own supported version query
                                 #   (`clippy-driver --version` under the seal, after FR-012-7a's safeguards); ABSENT when every clippy unit was Fresh;
driver_commit = "48a229cea"      #   NOT the trailing rustc argument of the clippy launch chain (not clippy's executing compiler), and NOT
                                 #   `cargo clippy --version` (FR-012-7b's component query, which never fills these fields — A-8 round)

[verified_with.checks.build]
outcome = "passed"
units_launched = 1
units_fresh = 0

[verified_with.checks.test]
outcome = "passed"
units_launched = 2
units_fresh = 0
```

The `[[file]]` and `[[resource]]` entries follow, unchanged from 1.2.0. The section references in
the comments (`§5.5`, `FR-012-*`, `D-L2-3`, `A-8`) are those of the Phase 012 brief,
`governance/phase-012-specification-and-decision-brief.md`, whose §5.2 this layout is quoted from.
Four rules govern the table:

| Rule | Statement |
|---|---|
| **Write rule** (FR-012-5a) — which operations write `[verified_with]` | `renvor new` (a real run; a dry run writes nothing) and `renvor generate auth` (its scratch verification) — the two operations that run the five checks — write `[verified_with]` with their `operation`. `renvor generate resource` and `renvor generate migration` run no build; they **leave `[verified_with]` byte-identical** and rewrite only the `[[file]]`/`[[resource]]` entries they own. The table is **measured, never derived** (FR-012-4): its values are what the five checks reported and what the seal queried, recorded separately — launch observations from the checks themselves as they ran in the staged (or scratch) tree under the sealed environment, and queried identities of the tools those launches named; nothing in it is inferred from `PATH`, from the generator's own process environment, from the pin, from a previous record, or from a configuration value the checks did not act on. A check whose units were positively `Fresh` is recorded as cached with the observed identity **unavailable**, never filled in. What the table carries is **launch observation plus queried identity**, never proof of fresh compiler execution through a wrapper |
| **Reader rule** (FR-012-5b, D-L2-9) — dispatch on `record_version` | A reader reads `record_version` first: **absent** → a legacy (version 1) record: accepted; `[toolchain]` and `[verified_with]` are reported as *unknown*, never filled in; **2** → the whole document is validated strictly (`deny_unknown_fields` within the version); **greater than the reader knows** → refused by name — `record_unsupported`, `details.record_version`, `details.supported = 2` — **before any file is planned or modified**, **exit 3** ([`command-surface.md`](command-surface.md)'s row 3, a validation failure: an unsupported *input* record, not a missing environment tool; U-1, approved 2026-09-07). A `tree_scope` the reader does not know is the same refusal. The reason string and the two `details` keys are one text across C-1's table, C-2's registry, the `tests/json/record_unsupported.json` fixture, and the help and README sentences that name them |
| **The incompatibility the other way** (FR-012-5c) — documented, not claimed away | A `renvor` built from source at or before `7281e4f` reads a version-2 record through `#[serde(deny_unknown_fields)]` and fails with serde's unknown-field error inside the existing read failure — a generic parse error, **not** `record_unsupported`, because those binaries do not have this rule. Nothing has been published, but people may have generated projects from source, so this contract's revision text and the README of every new project both say: *a project generated at template version 8 or later is not readable by a generator built before this revision; rebuild the generator, not the project* |
| **Freshness rule** (FR-012-5d, D-L2-10) — the verified tree, compared by contents | At the verification — after the five checks pass, before the manifest is written — the generator computes `tree_digest` = SHA-256 over the sorted lines `<relative path>\0<sha256 of the file's bytes>\n` for **every regular file in scope found by walking the staged (or scratch) tree**, and records it with `tree_scope = 1` and `verified_at`. **Scope (`tree_scope = 1`)** — what the five checks compile or read: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `build.rs`, `renvor.toml`, `.cargo/config.toml`, and every file under `src/`, `tests/`, `benches/`, `examples/`, and `migrations/`; **excluded**: `README.md`, `Dockerfile`, `.dockerignore`, `.gitignore`, `.env*`, the `.renvor/` directory (the record and the manifest themselves), `target/`, and every other path. A symlink in scope is recorded as `<path>\0symlink:<link text>\n`, never followed. **Additions and deletions count**: the scope is a pattern over the tree, not the `[[file]]` list, so a file added under `src/` or deleted from `tests/` changes the digest. **Freshness is not ownership**: the managed blocks above decide what the generator owns and may rewrite; for freshness the whole file's bytes count — an edit inside a managed block and an edit outside one both change compiled source, and both make the evidence **historical**; ownership never hides a change. `renvor check` recomputes the digest over the current working tree under the recorded `tree_scope` and compares: equal → `verified_with: current`; different → `verified_with: historical — the tree verified at <verified_at> (<operation>) is not the current tree; not proof of the current tree`, and `--output json` carries `historical = true`. **No count of operations is printed**: nothing in the tree records how many operations ran since the verification, and a number nothing supports is not printed. Earlier evidence is **retained**, never deleted and never re-dated. Because the files `generate resource` and `generate migration` add are inside the scope, the evidence becomes historical the moment they run — by the tree digest, not by a counter — and stays historical until an operation that verifies (`generate auth`, or a fresh `renvor new`) writes new evidence (FR-012-10c) |

## Output paths

Every template entry's output path is relative and contained. An entry whose rendered path would
escape the staging root is a **load-time** error, so such an entry cannot exist in a shipped binary.
