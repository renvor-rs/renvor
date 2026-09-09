---
description: "Contract C-1 — CLI command surface, exit codes, and stream discipline"
version: "1.6.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. 1.6.0 (2026-09-09, Phase 012, finding 4) ADDS, with no exit code, stream rule, or notice string changed: `rustdoc -vV` to the probes whose answer can be `compiler_identity_unreadable`; a narrowing of `evidence_capture_failed`'s disagreement clause to the launched BUILD/TEST units, since a rustdoc identity differing from the compiler's is a legitimate operator override and is recorded, not refused; and `details.supported` moving from `2` to `3` as the record reader gains version 3. 1.5.0 (2026-09-07, Phase 012, L-2) ADDS, with no exit code or stream rule changed: the `tool_missing` details of the toolchain preflight (`details.tool` values `rustup >= 1.28.1`, `rustup toolchain <channel>`, `rustc >= <msrv>`, a component name; `details.remedy`; `details.reason` values `proxy_unidentified` and `no_install_guarantee_unconfirmed`); the `--framework-path` toolchain reasons `toolchain_pin_malformed`, `toolchain_pin_unsupported`, `toolchain_pin_below_msrv`, and `msrv_unreadable`; the `project_verification_failed` reasons `compiler_identity_unreadable`, `evidence_capture_failed`, `toolchain_resolution_diverged`, and `probe_isolation_unavailable`; the new refusal `record_unsupported` (exit 3, U-1 approved 2026-09-07 — an unsupported input record is a validation failure, not a missing tool; the same reason string and `details` keys as C-2's registry, the fixture, and the help text); the two FR-012-8 stderr notices — the resolution notice and the observation notice (each one line, stderr, never stdout) — and the cached-artifacts line; `renvor check`'s two record tables and the `historical` marker; and `renvor generate`'s once-per-run sentence into a legacy tree. `renvor doctor`'s toolchain section is named as a follow-up (B1f), not delivered here. **The two added refusal reasons `probe_isolation_unavailable` and `no_install_guarantee_unconfirmed` are approved** (maintainer decision, 2026-09-08) with the failure codes their rows below carry — `project_verification_failed` (exit 3) and `tool_missing` (exit 5) respectively — and with the property that makes them safe to add: each is a REFUSAL and neither has a fallback. Nothing is retried in a less isolated place, nothing is provisioned, and nothing is staged; the run stops and says which condition it could not establish. It also DOCUMENTS the existing `--framework-path` rule `framework_lockfile` — `Cargo.lock` must exist in the checkout, since the starter's resolution is seeded from it (FR-006) — which the generator has emitted since Phase 011 and this table omitted; documenting it changes no behaviour. 1.4.0 (2026-09-05, FR-048 decided): every `generate` action takes `--overwrite-unchanged`; a target that differs from the render but is unchanged since generation is REGENERABLE and is replaced only under the flag — without it the run is `generation_conflict` naming the flag; a changed file is refused with or without it; a dry run classifies exactly as a real run. No exit code or stream rule changed. 1.3.0 (2026-09-05, Phase 011 correction round): generation into an existing project is one change or none; marked files are untouched outside their markers; `auth` keeps applied migrations and adds the owner column forward, renders recorded resources again, verifies the merged tree, and writes the resolved lockfile; migration versions are allocated past the directory; a name beside `--import`, a version another migration holds, and a bare SQL keyword are refused. 1.2.0 (2026-09-05, Phase 011) HONOURS `--auth` (none|session) and ADDS `--capabilities` and `--framework-path`: a project with any of them is a framework-backed STARTER with real path dependencies, and the reserved table loses `--auth`. No exit code or stream rule changed. 1.1.0 (2026-08-29) CORRECTS the phase the reserved-flag paragraph names — Phase 011 delivers the flag, Phase 009 delivers only the library the flag would generate against — and adds the rule that a reserved message must name the phase that delivers the FLAG. No exit code, stream rule, or flag changed. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract C-1 — Command surface, exit codes, and stream discipline

**Status**: defined before implementation, per constitution principle V and FR-002.
**Everything in this file is a public contract from the first release that ships it.**

## Commands in this phase

| Command | Purpose | In this phase |
|---|---|---|
| `renvor new [NAME]` | Create a project | **Full** |
| `renvor doctor` | Report environment readiness | **Full** |
| `renvor check` | Validate a project without building it | **Full** |
| `renvor routes` | Show the routes a project would serve | **Full.** The relay is implemented; it currently reaches no *generated* project — see below |
| `renvor dev` | Run the local development loop | **Full** |
| `renvor docker up\|down\|status\|logs` | Container development controls | **Full** |
| `renvor generate migration\|resource\|auth` | Add to an existing project — a migration pair or an imported set, a resource, or the auth starter — rerun-safe, see §`renvor generate` | **Full** (Phase 011) |
| `renvor tls trust` | The consent boundary for a trust-store change. **In this phase: consent only — it describes what would change, requires explicit consent, and then declines.** Non-interactive consent is `--i-understand-this-modifies-my-system-trust-store`; `--yes` does not grant it. |

`PLAN.md` §9.3 lists further commands — `migrate`, `seed`, and the package-ecosystem surface.
**They are not implemented here and are not stubbed.** A stub that exits zero is worse than an
absent command, because it reports success for work that did not happen. `generate` ships in
Phase 011 with the actions listed below and no other; an action it does not list is a `usage`
refusal from the argument parser.

### `renvor generate` — into an existing project, rerun-safe

Every `generate` action classifies each target path **before** writing anything, against the
working tree and the project's provenance record `.renvor/generated.toml`
([`template-contract.md`](template-contract.md) §"The provenance record"):

| The target path is | Effect |
|---|---|
| absent | written (`write`) |
| present and byte-identical to the render | nothing (`unchanged`) |
| present, different, and **unchanged since generation** — its digest equals the recorded one — with `--overwrite-unchanged` | replaced (`regenerate`): the generator owns it, and the operator said so |
| the same, **without** `--overwrite-unchanged` | **regenerable, refused**: `generation_conflict`, exit 3, nothing at all is written; `details.reason = overwrite_required`, `details.regenerable` names every such path, `details.flag` names the flag |
| present, different, and changed since generation, or never generated | **`generation_conflict`, exit 3, and nothing at all is written**, with or without the flag; `details.reason = changed_since_generation`, `details.changed` names every such path |
| an existing file's managed region — a marked block, or the lockfile the merged build resolves | edited (`edit`): the region is written, the rest of the file is kept; no flag needed, and never a conflict |

`details.paths` and `details.count` cover every refusing path of both kinds, in plan order; the
two sub-lists split them, and `reason` is `changed_since_generation` whenever a changed path is
among them, because the flag alone would not help. A refusal also lists what the plan would have
done beside it — `details.write` (created), `details.edit`, and, under the flag,
`details.regenerate` — each present only when non-empty, so a refused `--dry-run` reports the
whole classification: created, regenerated, edited, refused. **A differing file is never overwritten
implicitly** (FR-048, SR-009, decided 2026-09-05): the recorded digest proves the generator owns
the file, and `--overwrite-unchanged` is the permission to replace it. The flag waives nothing
else — not validation, not a conflict, not a verification failure.

Files are committed as **one change or none** (2026-09-05, Phase 011 correction round): every
file is first staged as a temporary sibling, then every sibling is renamed into place, and the
record is rewritten last with the new digests. A failure while staging removes the siblings; a
failure while placing, or in the record write, puts every placed path back the way it was, newest
first, and the message names any path the rollback could not restore. `--dry-run` classifies
**exactly** as the real run does and writes nothing: the same `result.files[]` when the run would
succeed, the same refusal with the same `details` when it would not. `--output json` carries
`result.files[]` with each path's action and `result.written`. A rerun of a command whose files are
in place reports `unchanged` and exit `0`, and needs no flag. Paths are named; contents never are.

For the two **marked** files — `src/resources/mod.rs` and `src/routes.rs` — "untouched" means
untouched **outside the markers**: the record digests them with the lines between the markers
removed, so an edit of the block never claims the rest of the file, and a line the user added
outside the block is a conflict for the next full re-render (found by the Codex review).

**The record is read before anything is planned** (1.5.0, Phase 012, FR-012-5b). Every `generate`
action reads `.renvor/generated.toml`'s `record_version` first: absent is a legacy record and is
accepted; `2` is validated strictly; a version **newer than this generator reads** is refused by
name — `record_unsupported`, exit `3`, `details.record_version` (the version found) and
`details.supported` (the highest this generator reads, `2`) — **before any file is planned or
modified**, and the working tree is byte-identical afterwards. The message says which versions
and that the remedy is to *rebuild the generator, not the project*. A legacy record — a project
generated at template version 7 or earlier, with no `record_version`, no pin, and no
`[verified_with]` — does **not** refuse on that account (FR-012-10a); every `generate` action into
such a tree states **once per run**, on stderr, that *this project was generated before template
version 8 and pins no toolchain; no `renvor generate toolchain` action exists — a new project's
README shows the two files to add by hand*. No pin is inserted into a legacy tree: `auth`'s
re-render of `Cargo.toml` carries no `rust-version` and plans no `rust-toolchain.toml`
(FR-012-10b). Which actions verify and write evidence is [`template-contract.md`](template-contract.md)
§"Record versions 2 and 3": `auth` verifies its scratch copy and rewrites `[verified_with]` with
`operation = "auth"`; `resource` and `migration` run `rustfmt` or nothing, print no toolchain
notice, and leave `[verified_with]` byte-identical. `auth`'s scratch copy is staged **beside the
project** — [`generation-transaction.md`](generation-transaction.md) §"Residue" — and its
resolution must equal the project directory's, or the run is `project_verification_failed`,
`details.reason = toolchain_resolution_diverged`, nothing written.

| Flag | On | Effect |
|---|---|---|
| `--overwrite-unchanged` | every `generate` action | replace the regenerable targets — those that differ from the render and are unchanged since generation. Never a file you changed; never a waiver of validation or of a conflict |

| Action | What it writes | Refused when |
|---|---|---|
| `migration <name>` | `migrations/<YYYYMMDDHHMMSS>_<name>.up.sql` and `.down.sql`, the version being the UTC instant **moved forward, second by second, past every version the directory already holds** — two names generated within one second get two versions, because SQLx keys its ledger by version; run again for the same name it finds the pair it wrote and leaves it, so a rerun never stacks a second pair | the project has no `[persistence]` (`unsupported_combination`, `details.reason = no_database`); the name is not a lowercase identifier of at most 64 characters (`unsupported_value`); a name **beside** `--import` (a parser conflict, exit `2`: the two ask for different work, and neither is silently dropped) |
| `migration --import auth\|jobs` | the framework's embedded migration set for the project's engine, byte for byte — the same files a starter receives — so a project that adopts the auth starter or the jobs capability later composes both sets in its one directory (Phase 010 limitation L-7) | a set outside the two (`unsupported_value`, `details.flag = --import`); a version of the set already held by **another** migration in the directory (`generation_conflict`, `details.reason = version_present`, `details.versions`), with nothing written |
| `resource <Name> [field:type …]` | into a **starter** with a database: `src/resources/<snake>.rs` (the type, its repository over the project's persistence model, five handlers, and their OpenAPI declarations), `migrations/<version>_create_<snake>.{up,down}.sql`, `tests/<snake>.rs`, the shared `tests/support/mod.rs` re-rendered, and **two marked edits** — `pub mod <snake>;` between the markers of `src/resources/mod.rs` and `crate::resources::<snake>::declare(&mut routes)?;` between the markers of `src/routes.rs`, which are edited whether or not the file was changed elsewhere. Types: `string`, `text`, `integer`, `boolean`, `float`. Rendered Rust is laid out by the toolchain's `rustfmt` before it is planned, so a user-named type or column never decides the formatting; a missing `rustfmt` is `tool_missing`. Writes need a session when the project has the auth starter. The record gains one `[[resource]]` with the name and the fields as given, which is what lets `auth` render the module again | a skeleton (`transport_not_wired`, `details.reason = no_renvor_dependency`); no `[persistence]` (`unsupported_combination`); a name that is not PascalCase of at most 32 characters, a field outside the grammar, `id`, or a duplicate (`unsupported_value`); a name or a field that would be a **bare SQL keyword** on PostgreSQL or MySQL — `Order`, `key` — (`unsupported_value`, `details.reason = reserved_identifier`), since the generated SQL uses the identifiers unquoted |
| `auth` | the session authentication starter added to a starter that has none: every generator-owned file rendered again with `auth = "session"` — `renvor.toml`, `Cargo.toml`, `src/main.rs`, `src/app.rs`, `src/routes.rs`, `src/auth.rs`, `config/auth.toml`, the auth migration set, the generated test — with the marked blocks of `src/resources/mod.rs` and `src/routes.rs` carried over. **Applied migrations are never re-planned**: the project's `0001_create_item` pair stays byte-identical (its checksum is in the ledger), and the owner column arrives by a new forward pair `migrations/<version>_add_item_owner.{up,down}.sql` — rows that existed before belong to nobody, the all-zero identifier, as the seeds mark theirs. **Every recorded resource** (`[[resource]]`) is rendered again with the session guards its writes now need; one the user edited is a conflict, so the starter is refused rather than added beside a public write. The merged tree is **verified in a scratch copy** — the same five checks `renvor new` runs — before anything is committed, and the `Cargo.lock` that build resolves is written as an `edit`, so `cargo build --locked` passes on the tree the command leaves. A file the user changed is a `generation_conflict`. **Every generator-owned file it renders again is regenerable on a placed starter, so the action needs `--overwrite-unchanged`**; without it the run is refused naming the flag, and nothing is written | regenerable targets without `--overwrite-unchanged` (`generation_conflict`, `details.reason = overwrite_required`); exactly what `renvor new --auth session` refuses: no database, no `mail` capability (`unsupported_combination` naming the flag); a skeleton (`transport_not_wired`); a merged tree that does not build, lint, format, test, or start (`project_verification_failed`, nothing written) |

`routes` **ships in Phase 004**, with the transport it inspects, and is held to the same rule.

### `renvor check` — the record's two tables (1.5.0, Phase 012)

`check` reads the record through the dispatch rule above and prints, beside the manifest it
already reports, the record's **`[toolchain]`** table (`pinned`, `rust_version`) and its
**`[verified_with]`** table (every field, the per-check tables included) — or *unknown* for a
legacy record, never a value filled in. The `doctest` check is printed **only when the record
carries it**: a project with no library target launched no doctest unit and has no table, and a
row of zeroes there would report a unit that was never scheduled as one that was reused. It then applies the freshness rule of
[`template-contract.md`](template-contract.md) §"Record versions 2 and 3": it recomputes the tree digest
over the current working tree under the recorded `tree_scope` and prints either
`verified_with: current` or `verified_with: historical — the tree verified at <verified_at>
(<operation>) is not the current tree; not proof of the current tree`. A `tree_scope` it does not
know is `record_unsupported`, exit `3`. If `rust-toolchain.toml` is present it reports whether the
file's channel still equals `[toolchain].pinned` — an author's edit is **reported, never refused**.
It prints **no count of operations** since the verification: nothing in the tree records one. With
`--output json` the two tables are `result.toolchain` and `result.verified_with`, the marker is
`result.verified_with.historical`, per [`json-output.md`](json-output.md) §"`result.toolchain` and
`result.verified_with`". `check` still builds nothing and runs no tool.

### `renvor routes` — where its data comes from, and what it cannot do

It runs the **application binary** and asks it for its own route registry, through an explicit
versioned invocation the binary answers by printing the registry as the `result` payload of the C-2
envelope. That registry is the same value that builds the router, which is what makes the listing
and the router agree by construction rather than by maintenance.

| Property | Rule |
|---|---|
| Invocation | `cargo run --quiet -- --renvor-dump-routes`, in the project directory |
| Binary selection | the project's **own declared default binary**. Nothing searches `target/` for something executable |
| Payload version | `result.protocol`, currently **`1`**, checked **before** the payload is read |
| Unknown version | refused **by name**, never parsed on a best-effort basis |
| Boot side effects | **none.** The application answers and exits before it starts anything |
| Streams | `stdout` is captured and must carry exactly the envelope; `stderr` is inherited, so a build's progress reaches the operator |

It does **not** parse the project's source, and it does **not** read a second manifest. Contract
[`http-routing.md`](http-routing.md) prohibits a second route list that can drift, and a source
parser would be one.

**Every failure is named.** `details.reason` is one of `no_renvor_dependency`,
`invocation_failed`, `dump_failed`, `dump_unreadable`, `protocol_unstated`, or
`protocol_unsupported` — so a consumer can tell "the binary would not build" from "the binary
answered something I cannot read".

**Dated limitation — 2026-08-22, narrowed 2026-09-05.** No Renvor crate is published, so no
**skeleton** the generator produces depends on the framework, and a skeleton cannot answer the
invocation; it reports that with `transport_not_wired`, exit `3`, and
`details.reason = no_renvor_dependency`. Since Phase 011 a **starter** — a project generated with
`--framework-path` — depends on the framework by path and answers the invocation, so the command
succeeds against every starter and against no skeleton.

**It never prints an empty route table and exits `0` when the registry could not be obtained.** An
empty success is indistinguishable, to a consumer, from an application that genuinely declares no
routes, and the two mean different things. An application that **answers** with an empty registry
is a different fact, and that *is* reported as a success saying so.

## Exit codes

| Code | Meaning | Example |
|---|---|---|
| `0` | Success | The project was created |
| `1` | **Unclassified or internal failure** | A panic, or an error no other code describes |
| `2` | Usage error | Unknown flag, missing required argument |
| `3` | Validation failure | Unsupported value, unsupported combination, reserved later-phase flag, invalid manifest; a provenance record newer than the generator reads (`record_unsupported`, 1.5.0); a framework checkout whose toolchain pin is malformed, an alias, or below its own MSRV (1.5.0) |
| `4` | Cancelled by the operator | Ctrl-C or ESC at a prompt, or declining the review screen |
| `5` | Environment failure | A required tool is missing; the container runtime is not running; rustup below 1.28.1, a rustup proxy whose rustup cannot be located, a pinned toolchain that is not installed, a resolved compiler below the MSRV, or a missing `rustfmt`/`clippy` component (`tool_missing`, 1.5.0) |

**`1` is reserved on purpose.** A taxonomy without it absorbs unclassified failures into a general
error code, and an unclassified failure is a **defect** rather than an outcome. Anything exiting `1`
is a bug report.

## Stream discipline

| Stream | Carries |
|---|---|
| `stdout` | **The command's result, and nothing else.** With `--output json`, exactly one JSON document |
| `stderr` | Prompts, progress, warnings, diagnostics, and error text |

Consequences that are part of the contract:

- `renvor new --dry-run --output json | jq .` MUST work with no filtering.
- Progress rendering MUST degrade to nothing when `stderr` is not a terminal.
- A closed `stdout` (`| head -1`) MUST NOT produce a panic; it exits `0` if the result was already
  written, and otherwise reports the write failure.
- The toolchain notices of Phase 012 (§"Toolchain preflight, evidence, and notices" below) are
  **diagnostics**: each is one line on `stderr`, never on `stdout`, so `renvor new --output json`
  still carries exactly one JSON document and a shell script reading `stdout` sees nothing new
  (1.5.0).

## Global flags

| Flag | Effect |
|---|---|
| `--output <human\|json>` | Result format. Default `human` |
| `--yes` | Waive **confirmation only**. It never waives validation |
| `--dry-run` | Compute and report; write nothing (FR-020) |
| `--no-color` | Disable styling. Styling is also disabled automatically when the stream is not a terminal, under `TERM=dumb`, when `NO_COLOR` is set to a non-empty value, and in `--output json`. An explicit refusal beats any force-colour environment variable. The full policy, the semantic roles, and the layout rules are [`terminal-presentation.md`](terminal-presentation.md) |

## `--transport`

**No longer reserved.** Phase 004 ships the transport capability, so `--transport` is a real choice:

| Value | Behaviour |
|---|---|
| `rest` | **accepted** — the only supported value |
| anything else | `unsupported_value`, exit `3`, naming the supported value |
| omitted | **defaulted to `rest`** and **recorded** in `renvor.toml` |

It is **not** `reserved_for_later_phase`. Reporting "reserved for Phase 004" from inside Phase 004
would be a false statement about when support arrives.

The wizard does **not** ask about it. Constitution v3.0.0 principle VII clause 2 permits a choice
with **one** supported value to be defaulted without prompting provided it is recorded — the same
treatment `--target` already receives, which amendment 3.0.0 §4 records as complying.

## `--orm` and `--database`

**No longer reserved.** Phase 006 ships persistence and Phase 007 adds a second persistence model,
so both are real choices with real alternatives:

| Flag | Value | Behaviour |
|---|---|---|
| `--orm` | `sqlx` | **accepted** — hand-written SQL, no object mapper |
| `--orm` | `seaorm` | **accepted** — an entity and a repository are generated |
| `--orm` | anything else | `unsupported_value`, exit `3`, naming **both** supported values |
| `--orm` | omitted, with `--database` given | **defaulted to `sqlx`** and **recorded** — see below |
| `--orm` | given, `--database` omitted | `unsupported_combination`, exit `3` |
| `--database` | `postgres`, `mysql` | **accepted** |
| `--database` | anything else | `unsupported_value`, exit `3`, naming both supported values |
| `--database` | omitted | **no persistence** — no persistence sources, no `migrations/`, and no `[persistence]` table |

### The `--orm` default is a compatibility promise, not an absence of alternatives

Until Phase 007 this row was justified by *"`sqlx` is the only value `--orm` accepts, so there is
nothing to choose between"*. **That reasoning expired when `seaorm` was added**, and the behaviour
was kept anyway for a different and stronger reason: every `renvor new --database postgres` written
against Phase 006 must keep producing the project it produced. Omission is therefore a
**documented compatibility default**, stated here and in `--help`, and an operator who wants the
other model names it.

The wizard **does** now ask, because there are two values to choose between. The question is asked
**inside** the persistence gate — an operator who declines a database is never asked which ORM they
are not using.

### What each selection generates

| | `--orm sqlx` | `--orm seaorm` |
|---|---|---|
| Sources | `src/persistence.rs` | `src/entity.rs`, `src/repository.rs` |
| Migrations | `migrations/0001_create_item.{up,down}.sql` | **identical** |
| Declared as modules in `src/main.rs` | yes | **no** — see below |
| `Cargo.toml` dependencies | none | none |

`renvor.toml` records `database`, `orm`, and `driver_feature` under `[persistence]` either way.

**`Cargo.toml` declares no dependency in both cases — for the skeleton.** *(Since Phase 011 a
project given `--framework-path` is a starter whose `Cargo.toml` declares path dependencies on
exactly the crates the selection needs; the paragraph below describes the skeleton, which is
unchanged.)* For the SeaORM path the reason is stronger than "the crate is unpublished". `sea-orm` *is* published, so it could be declared — but
generation runs the staged project's own `cargo fmt`, `clippy --all-targets`, `build`, `test` and `run` **before**
placing it (a skeleton is run bare and must exit; a starter is sent `--renvor-dump-routes`, the
request `renvor routes` sends, and must answer it before Boot, without a database), so a real dependency would make `renvor new` resolve and compile SeaORM and SQLx from
the registry. Renvor guarantees offline generation. One ORM choice is not a reason to withdraw it.

Consequently `src/entity.rs` and `src/repository.rs` are generated **in full and idiomatic** but are
not declared as modules, because declaring a module nothing can compile emits a project that does
not build. `Cargo.toml` names the four lines to add and the two declarations to make.

## `--auth`

**No longer reserved.** Phase 011 ships the authenticated starter, so `--auth` is a real choice
(W-023's removal plan):

| Value | Behaviour |
|---|---|
| `none` | **accepted**, and the default when omitted — recorded as `auth = "none"` |
| `session` | **accepted** — cookie sessions: registration, login, logout, the current user, verification, and password reset, on the selected persistence row; the item example gains ownership and a deny-by-default policy |
| `api`, `full` | `unsupported_value`, naming `none, session` and the reason: the framework ships no route that issues a first token pair (only `POST /auth/token/refresh`), so a generated `api` starter could not authenticate anyone. **Not** `reserved_for_later_phase` — no phase is assigned to issuance, and naming one would be a promise |
| anything else | `unsupported_value`, naming both supported values |

`session` needs `--database` (`unsupported_combination`, flags `--auth, --database`), the `mail`
capability (`unsupported_combination`, flags `--auth, --capabilities` — a starter whose
verification mail went nowhere would be the silent fallback constitution III and IV forbid), and
`--framework-path` (below). The wizard **asks**: two supported values, so clause 2 of principle
VII does not apply.

**History.** Reserved from Phase 003, the flag named Phase 013, then Phase 009, then — corrected by
Phase 009 itself (its FR-085) — Phase 011, the phase that delivers a generated project rather than
the library one uses. Phase 011 honoured it on 2026-09-05 (W-023).

## `--capabilities`

A comma-separated subset of the five capabilities Phase 010 shipped, or `none` (W-024's removal
plan):

| Value | Behaviour |
|---|---|
| `cache`, `jobs`, `mail`, `storage`, `observability`, in any order and combination | **accepted**; recorded as five booleans under `[capabilities]`; each selected one changes the generated dependencies and features, the typed configuration section, the provider registration and lifecycle, and the application wiring; each **unselected** one appears nowhere |
| `none` | **accepted**, and the default when omitted |
| an unknown name | `unsupported_value`, naming the five |
| a name given twice, or an empty list | `unsupported_value` |
| `none` beside a name | `unsupported_combination` |

`jobs` needs `--database` (the durable store is the application's own row, ADR-0032). Any
capability needs `--framework-path`. With `--container`, the `cache` capability generates the
cache service the way `--database` generates the database service, and `[container]` records
`cache_wired_into_application = true`; `--container-cache none` beside it is refused as a
contradiction. The wizard asks for the list by name.

## `--framework-path`

**Local tooling: where the framework is, not what the project does.** No Renvor crate is published
(Phase 013), so a generated project can depend on the framework only by **path**. The value names
a checkout of the Renvor workspace and is validated **before any write** — two files are read
(`Cargo.toml` and `crates/renvor/Cargo.toml`; a third, `rust-toolchain.toml`, since 1.5.0), one
is checked for existence (`Cargo.lock`), and nothing is evaluated:

| Rule (`details.rule`) | Requirement |
|---|---|
| `framework_path_utf8`, `framework_path_control_character` | the path is UTF-8 and carries no control character (it is written into `Cargo.toml`) |
| `framework_directory` | it resolves to an existing directory; recorded canonical and absolute |
| `framework_manifest`, `framework_workspace` | its `Cargo.toml` exists, is under 64 KiB, parses, and declares `[workspace]` |
| `framework_facade` | `crates/renvor/Cargo.toml` exists and names package `renvor` |
| `framework_lockfile` *(emitted since Phase 011; documented in 1.5.0)* | `Cargo.lock` exists there. A generated starter starts from the framework's lockfile so that it resolves offline (FR-006), and a checkout without one cannot supply it — refused at validation with its own rule rather than discovered when staging tries to copy it |
| `framework_toolchain` *(1.5.0, Phase 012, FR-012-1)* | its `rust-toolchain.toml` `[toolchain].channel` is an **exact release** `X.Y.Z`, its `Cargo.toml` `[workspace.package].rust-version` is `X.Y.Z`, and the channel is at or above that MSRV — **each parsed on its own, neither evaluated**, then compared; the pin becomes the starter's `rust-toolchain.toml` channel and the MSRV its `rust-version`. A refusal carries `details.reason`, one of `toolchain_pin_malformed` (not `X.Y.Z`; the file unreadable or missing the key), `toolchain_pin_unsupported` (a channel alias — `stable`, `beta`, `nightly`, a dated `nightly-YYYY-MM-DD` — a custom toolchain name, or a `path`: none is resolved to a version, silently or otherwise), `toolchain_pin_below_msrv` (`X.Y.Z` below the MSRV), or `msrv_unreadable` (the manifest key absent or not `X.Y.Z`), and `details.file` naming the file. A refused checkout is the framework's inconsistency, refused **before anything is staged** |

Every refusal is `unsupported_value` with `details.flag = "--framework-path"`. *(The four
`framework_toolchain` reasons are the Phase 012 brief's, FR-012-1; the rule name is this
revision's, chosen to sit in the `framework_*` family the brief places them in.)*

| Given | Shape generated |
|---|---|
| omitted, and neither `--auth session` nor a capability was asked for | the **skeleton**: the dependency-free tree every earlier phase produced, changed only by its recorded version and the two recorded choices |
| omitted, and one of them was | `unsupported_combination`, flags `--framework-path` — the choice cannot be honoured, so it is refused rather than recorded |
| given | the **starter**: a real Renvor application with path dependencies on exactly the crates the selection needs, verified in staging like any other generation |

Recorded as `[framework] source = "path"`, `path = "<absolute>"`. The wizard asks for it **only**
when a selection needs it. When the crates are published the same model gains a registry source
and the path becomes optional; nothing else moves.

## Toolchain preflight, evidence, and notices (1.5.0, Phase 012, L-2)

Every generated tree declares its toolchain — `rust-toolchain.toml` and `rust-version` — and
`renvor new` and `renvor generate auth` verify it with the compiler that **resolves** in the
directory being verified, after a preflight that identifies the tools before it invokes them and
provisions nothing ([`generation-transaction.md`](generation-transaction.md) §"What 'verify before
placing' means"). What the operator meets on this surface — the refusals, their details, and the
two notices — is listed here; the mechanism is C-5's. **No exit code changes, no stream rule
changes, and nothing is refused for being different**: an override the operator or CI chose
proceeds and is said aloud.

### `tool_missing` (exit `5`) — the preflight's refusals

| `details.tool` | When | `details.remedy` |
|---|---|---|
| `rustup >= 1.28.1` | the located `rustup`'s `--version` (run only under isolation, never in a pinned directory) is unparseable or below the floor — `details.found` carries the version or `unparseable`; **no proxy has run** | update rustup to 1.28.1 or later |
| `rustup >= 1.28.1`, with `details.reason = proxy_unidentified` | `rustc`/`cargo` are rustup proxies whose rustup could not be located: the isolated identification probe answered in rustup's own words; **refused before any proxy runs in the pinned directory**, nothing downloaded | put the `rustup` that owns these proxies first on `PATH` |
| `rustup >= 1.28.1`, with `details.reason = no_install_guarantee_unconfirmed` | an identified proxy at or above the floor, asked for a name that cannot be installed, answered anything but "is not installed" — the in-run witness of the no-install guarantee failed, so nothing more is run through it | the same as the row above; report the rustup version |
| `rustup toolchain <channel>` | the project's pin is not installed: the resolution probe met rustup's "is not installed" text; **refused before any check, nothing staged, no download** | `rustup toolchain install <channel> --component rustfmt --component clippy --profile minimal` |
| `rustc >= <msrv>` | the compiler the preflight resolution names is older than the project's `rust-version` — the generator's prerequisite check of the resolved compiler, **not** proof of Cargo's effective compiler under `RUSTC`, `build.rustc`, or a wrapper, and not a claim about what Cargo would do later | install or select a compiler at or above the MSRV |
| `rustfmt`, `clippy` | the resolved toolchain lacks the component (`rustfmt --version`, `cargo clippy --version` under the seal) | `rustup component add <component>` (with `--toolchain <channel>` when the project pins one) |

`details.found` and `details.required` keep the meanings [`json-output.md`](json-output.md)'s
registry gives them. `<channel>` is taken from rustup's own message and sanitized to
`[A-Za-z0-9._-]{1,64}` before it enters any stream.

### `project_verification_failed` (exit `3`) — the added reasons

| `details.reason` | Meaning |
|---|---|
| `compiler_identity_unreadable` | a `rustc -vV`, `rustdoc -vV`, `cargo -vV`, or `clippy-driver --version` answer outside the identity grammar (release `X.Y.Z[-pre]`, commit `[0-9a-f]{7,40}` or `unknown`, host `[A-Za-z0-9_.-]{1,64}`), or the isolated identification probe's timeout or unrecognised answer; the output is redacted, nothing placed |
| `evidence_capture_failed` | a unit of the project's own package(s) accounted for by neither a `Running` line nor a positive `Fresh` report; a truncated, malformed, or unparseable `-vv` stream; or two launched **build/test** units whose queried identities disagree — or two launched **doctest** units whose identities disagree with each other. A rustdoc identity that differs from the compiler's is **not** this: `RUSTC` redirects `rustc` and leaves `rustdoc` on the toolchain's own, so the two legitimately differ, and the difference is recorded in separate fields rather than refused. **Never** reported as cached; nothing placed |
| `toolchain_resolution_diverged` | `generate auth`: the resolution in the project directory and in its sibling scratch copy differ in release, commit, or attribution; nothing written |
| `probe_isolation_unavailable` | no directory could be found under which to create the isolation the identification probe needs — one with no `rust-toolchain.toml` or `rust-toolchain` in any ancestor (the system temporary directory, then the home directory, are tried); nothing is probed and nothing is staged |

`details.check` and `details.stage` keep their registry meanings where a check itself failed.

### `record_unsupported` (exit `3`) — an unsupported input record

A `.renvor/generated.toml` whose `record_version` is newer than this generator reads is a
**validation** failure of an input, not a missing environment tool: `record_unsupported`, exit
`3`, `details.record_version` (the version found), `details.supported` (the highest this generator
reads, `3`); refused **before any plan**, the working tree untouched (U-1, approved 2026-09-07).
The same refusal covers a `tree_scope` `check` does not know. The reason string and the two
`details` keys are one text here, in C-2's registry, in `tests/json/record_unsupported.json`, and
in the help and README sentences that name them. The message ends with the remedy: *rebuild the
generator, not the project.*

### The two notices, and the cached-artifacts line — `stderr`, never `stdout`

Nothing here falls back silently, and nothing here refuses a difference the operator chose. Each
line is a diagnostic on `stderr`, printed once, never on `stdout`; nothing at all is printed when
the pin resolved, a launch was observed, and the observed identity matched.

| Notice | Printed when | The line, verbatim |
|---|---|---|
| **Resolution notice** (FR-012-8 (1)) | the preflight resolution is not the pin — an override is active, rustup is absent, or the tree pins nothing. It describes the preflight resolution and **nothing else**: not Cargo's effective compiler | `toolchain resolved before verification: rustc <release> (<selected_by>); the project pins <channel>` — a bare toolchain: `toolchain resolved before verification: rustc <release> (no_rustup); the project pins <channel>, which cannot be selected here` — a legacy tree: `toolchain resolved before verification: rustc <release> (<selected_by>); the project pins nothing` |
| **Observation notice** (FR-012-8 (2)) | a launch was observed and the observed identity differs from the resolved one in release or commit (`RUSTC`, `build.rustc`; a substituting wrapper is not visible to the observation) — **whether or not the resolution equals the pin**: a `PATH` probe equal to the pin never conceals an observed override that differs | `verification launched rustc <release> (<commit>), not the resolved rustc <release> (<commit>): launch observation plus queried identity` |
| **Cached-artifacts line** (FR-012-7d (d)) | a successful check's relevant units were all positively `Fresh`, so no compiler launch was observed for it; the checks named are exactly those. A cached run prints this line and **no** observation notice, because no observation exists. **The doctest check is never among the names, and that is a fact rather than an omission**: its table exists only when a doctest unit was launched, so `units_launched == 0` is never true of it. A cached library-bearing project therefore prints this line naming `clippy, build, test` — truthfully, because none of those launched a unit of its own — while `checks.doctest` on the record shows the rustdoc launch that did happen | `verification reused cached artifacts for <checks>: no compiler launch observed` |

`<selected_by>` is one of `environment`, `directory_override`, `toolchain_file`, `default`,
`no_rustup`, `unknown` — taken from `rustup show active-toolchain`'s attribution text, `unknown`
when the text is not one a test pins, never a guess. The record carries the pin, the resolution,
and the observation, each labelled, so the notices and `.renvor/generated.toml` agree.

### `renvor doctor` — a follow-up, not this revision

The toolchain section of `renvor doctor` (the pin, the rustup floor, what resolves here and why,
whether the pin is installed with both components, proxy detection — all without any listing or
installing command) is specified in the Phase 012 brief §5.7 and is delivered by the follow-up
batch B1f (FR-012-11). This revision names it so the JSON key is reserved
([`json-output.md`](json-output.md)) and states nothing about its behaviour.

## Reserved flags

Flags for later-phase choices — `--frontend`, `--styling`, `--render-mode`, `--desktop` —
**parse successfully and then fail validation** with exit `3` and a message naming the choice and
the phase that will support it.

They are **not** rejected as unknown flags, because "unknown flag" tells a user their command is
wrong while "not supported until Phase 011" tells them when it will be right. They are **not**
silently ignored, because that would let a Phase 003 command line quietly change meaning later.

> **The phase a message names must be the phase that delivers the flag, not the phase that delivers
> the subject.** The authentication flag named Phase 009 until Phase 009 corrected it (its history
> is under its own heading above): Phase 009 shipped the library, and a flag that asks for a
> **generated project** belongs to the phase that generates. Naming the library's phase would have
> made the message expire the day that phase merged — an operator would read it, try the flag, and
> find it still refused.

## Interaction and terminals

- The wizard is entered **only** when `stdin` is a terminal.
- It additionally needs somewhere to **draw**: prompts are written to `stderr`, so if `stdin` is a
  terminal and `stderr` is not, the command exits `2` and directs the operator to supply the
  answers as command-line arguments instead. That is the **generic prompt adapter's** refusal: it
  names the kind of input to supply rather than enumerating the caller's specific flags, which it
  has no way to know. It refuses **before drawing anything**, so a redirected `stderr` receives the
  diagnostic and nothing else.

  `stdin` still decides *eligibility* and `stderr` only decides *drawability*, and the two are
  deliberately not merged: treating a redirected `stderr` as "no wizard" would make
  `renvor new --path ./x 2>log` generate a project from defaults nobody was asked for, which
  FR-010 forbids.
- When `stdin` is not a terminal and a required answer was not supplied by a flag, the command exits
  non-zero naming the missing flags. It MUST NOT block, and MUST NOT substitute a default (FR-010).
- Cancellation at any prompt exits `4`, and the destination is untouched.

## `--help`

Structure is a contract: usage line, description, arguments, options grouped consistently, and exit
codes documented. It is asserted as expected output rather than by assertions in code, so a change
to the contract appears as a diff in review.

The **content** is generated by the argument parser from the same declaration that parses the
command line, so it cannot drift from what is actually accepted. Colour is applied to that
rendering and never replaces it; see [`terminal-presentation.md`](terminal-presentation.md).

## Human presentation

How the human-facing output *looks* — semantic roles, when colour is permitted, how a label and a
value are laid out at a given width, and what a prompt does — is
[`terminal-presentation.md`](terminal-presentation.md) (contract C-8).

**C-8 governs nothing in this document.** Command names, flags, defaults, argument semantics, exit
codes, stream ownership, and cancellation classification are defined here and are unaffected by it.
Where the two appear to disagree, this one wins.
