# Contract C-1 — Command surface, exit codes, and stream discipline

**Status**: defined before implementation, per constitution principle V and FR-002.
**Everything in this file is a public contract from the first release that ships it.**

## Commands in this phase

| Command | Purpose | In this phase |
|---|---|---|
| `renvor new [NAME]` | Create a project | **Full** |
| `renvor doctor` | Report environment readiness | **Full** |
| `renvor check` | Validate a project without building it | **Full** |
| `renvor dev` | Run the local development loop | **Full** |
| `renvor docker up\|down\|status\|logs` | Container development controls | **Full** |
| `renvor tls trust` | The consent boundary for a trust-store change. **In this phase: consent only — it describes what would change, requires explicit consent, and then declines.** Non-interactive consent is `--i-understand-this-modifies-my-system-trust-store`; `--yes` does not grant it. |

`PLAN.md` §9.3 lists further commands — `generate`, `migrate`, `seed`, `routes`, `openapi`, and the
package-ecosystem surface. **They are not implemented here and are not stubbed.** A stub that exits
zero is worse than an absent command, because it reports success for work that did not happen.

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
| `--no-color` | Disable styling. Styling is also disabled automatically when the stream is not a terminal |

## Reserved flags

Flags for later-phase choices — `--transport`, `--orm`, `--database`, `--auth`, `--frontend`,
`--styling`, `--render-mode`, `--desktop` — **parse successfully and then fail validation** with
exit `3` and a message naming the choice and the phase that will support it.

They are **not** rejected as unknown flags, because "unknown flag" tells a user their command is
wrong while "not supported until Phase 006" tells them when it will be right. They are **not**
silently ignored, because that would let a Phase 003 command line quietly change meaning later.

## Interaction and terminals

- The wizard is entered **only** when `stdin` is a terminal.
- When `stdin` is not a terminal and a required answer was not supplied by a flag, the command exits
  non-zero naming the missing flags. It MUST NOT block, and MUST NOT substitute a default (FR-010).
- Cancellation at any prompt exits `4`, and the destination is untouched.

## `--help`

Structure is a contract: usage line, description, arguments, options grouped consistently, and exit
codes documented. It is asserted as expected output rather than by assertions in code, so a change
to the contract appears as a diff in review.
