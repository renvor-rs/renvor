---
description: "Contract C-1 — CLI command surface, exit codes, and stream discipline"
version: "1.2.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. 1.2.0 (2026-09-05, Phase 011) HONOURS `--auth` (none|session) and ADDS `--capabilities` and `--framework-path`: a project with any of them is a framework-backed STARTER with real path dependencies, and the reserved table loses `--auth`. No exit code or stream rule changed. 1.1.0 (2026-08-29) CORRECTS the phase the reserved-flag paragraph names — Phase 011 delivers the flag, Phase 009 delivers only the library the flag would generate against — and adds the rule that a reserved message must name the phase that delivers the FLAG. No exit code, stream rule, or flag changed. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
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
| absent | written |
| present and byte-identical to the render | nothing (`unchanged`) |
| present, different, and untouched since generation — its digest equals the recorded one | overwritten (`regenerate`): the generator owns it |
| present, different, and changed since generation, or never generated | **`generation_conflict`, exit 3, and nothing at all is written**; `details.paths` names every such path |

Files are committed one at a time through a temporary sibling and a rename after the whole plan
has passed, and the record is rewritten last with the new digests. `--dry-run` reports the plan
and writes nothing; `--output json` carries `result.files[]` with each path's action and
`result.written`. A rerun of a command whose files are in place reports `unchanged` and exit `0`.

| Action | What it writes | Refused when |
|---|---|---|
| `migration <name>` | `migrations/<YYYYMMDDHHMMSS>_<name>.up.sql` and `.down.sql`, the version being the UTC instant; run again for the same name it finds the pair it wrote and leaves it, so a rerun never stacks a second pair | the project has no `[persistence]` (`unsupported_combination`, `details.reason = no_database`); the name is not a lowercase identifier of at most 64 characters (`unsupported_value`) |
| `migration --import auth\|jobs` | the framework's embedded migration set for the project's engine, byte for byte — the same files a starter receives — so a project that adopts the auth starter or the jobs capability later composes both sets in its one directory (Phase 010 limitation L-7) | a set outside the two (`unsupported_value`, `details.flag = --import`) |
| `resource <Name> [field:type …]` | into a **starter** with a database: `src/resources/<snake>.rs` (the type, its repository over the project's persistence model, five handlers, and their OpenAPI declarations), `migrations/<version>_create_<snake>.{up,down}.sql`, `tests/<snake>.rs`, the shared `tests/support/mod.rs` re-rendered, and **two marked edits** — `pub mod <snake>;` between the markers of `src/resources/mod.rs` and `crate::resources::<snake>::declare(&mut routes)?;` between the markers of `src/routes.rs`, which are edited whether or not the file was changed elsewhere. Types: `string`, `text`, `integer`, `boolean`, `float`. Rendered Rust is laid out by the toolchain's `rustfmt` before it is planned, so a user-named type or column never decides the formatting; a missing `rustfmt` is `tool_missing`. Writes need a session when the project has the auth starter | a skeleton (`transport_not_wired`, `details.reason = no_renvor_dependency`); no `[persistence]` (`unsupported_combination`); a name that is not PascalCase of at most 32 characters, a field outside the grammar, `id`, or a duplicate (`unsupported_value`) |
| `auth` | the session authentication starter added to a starter that has none: every generator-owned file rendered again with `auth = "session"` — `renvor.toml`, `Cargo.toml`, `src/main.rs`, `src/app.rs`, `src/routes.rs`, `src/auth.rs`, `config/auth.toml`, the auth migration set, the generated test — with the marked blocks of `src/resources/mod.rs` and `src/routes.rs` carried over; a file the user changed is a `generation_conflict` | exactly what `renvor new --auth session` refuses: no database, no `mail` capability (`unsupported_combination` naming the flag); a skeleton (`transport_not_wired`) |

`routes` **ships in Phase 004**, with the transport it inspects, and is held to the same rule.

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
| `3` | Validation failure | Unsupported value, unsupported combination, reserved later-phase flag, invalid manifest |
| `4` | Cancelled by the operator | Ctrl-C or ESC at a prompt, or declining the review screen |
| `5` | Environment failure | A required tool is missing; the container runtime is not running |

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
a checkout of the Renvor workspace and is validated **before any write** — two files are read,
nothing is evaluated:

| Rule (`details.rule`) | Requirement |
|---|---|
| `framework_path_utf8`, `framework_path_control_character` | the path is UTF-8 and carries no control character (it is written into `Cargo.toml`) |
| `framework_directory` | it resolves to an existing directory; recorded canonical and absolute |
| `framework_manifest`, `framework_workspace` | its `Cargo.toml` exists, is under 64 KiB, parses, and declares `[workspace]` |
| `framework_facade` | `crates/renvor/Cargo.toml` exists and names package `renvor` |

Every refusal is `unsupported_value` with `details.flag = "--framework-path"`.

| Given | Shape generated |
|---|---|
| omitted, and neither `--auth session` nor a capability was asked for | the **skeleton**: the dependency-free tree every earlier phase produced, changed only by its recorded version and the two recorded choices |
| omitted, and one of them was | `unsupported_combination`, flags `--framework-path` — the choice cannot be honoured, so it is refused rather than recorded |
| given | the **starter**: a real Renvor application with path dependencies on exactly the crates the selection needs, verified in staging like any other generation |

Recorded as `[framework] source = "path"`, `path = "<absolute>"`. The wizard asks for it **only**
when a selection needs it. When the crates are published the same model gains a registry source
and the path becomes optional; nothing else moves.

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
