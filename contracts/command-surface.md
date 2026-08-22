---
description: "Contract C-1 — CLI command surface, exit codes, and stream discipline"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
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
| `renvor tls trust` | The consent boundary for a trust-store change. **In this phase: consent only — it describes what would change, requires explicit consent, and then declines.** Non-interactive consent is `--i-understand-this-modifies-my-system-trust-store`; `--yes` does not grant it. |

`PLAN.md` §9.3 lists further commands — `generate`, `migrate`, `seed`, `openapi`, and the
package-ecosystem surface. **They are not implemented here and are not stubbed.** A stub that exits
zero is worse than an absent command, because it reports success for work that did not happen.

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

**Dated limitation — 2026-08-22.** No Renvor crate is published, so no project the current
generator produces depends on the framework, and none of them can answer the invocation. The
command therefore succeeds against **none of them today** — not because the relay is missing, but
because there is nothing published for a generated project to depend on. It reports that with
`transport_not_wired`, exit `3`, and `details.reason = no_renvor_dependency`.

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

## Reserved flags

Flags for later-phase choices — `--orm`, `--database`, `--auth`, `--frontend`,
`--styling`, `--render-mode`, `--desktop` — **parse successfully and then fail validation** with
exit `3` and a message naming the choice and the phase that will support it.

They are **not** rejected as unknown flags, because "unknown flag" tells a user their command is
wrong while "not supported until Phase 006" tells them when it will be right. They are **not**
silently ignored, because that would let a Phase 003 command line quietly change meaning later.

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
