---
description: "Contract C-2 — machine-readable output envelope and error-code registry"
version: "1.0.0"
status: "normative — the wire payload carries its own `schemaVersion`, currently 2, which is independent of this document version. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract C-2 — Machine-readable output

**Status**: defined before implementation. **A compatibility contract** (FR-022, FR-023).

## The rule

With `--output json`, `stdout` carries **exactly one JSON document**, for success and for failure
alike. Not zero on failure. Not two. Not a document followed by a trailing newline of prose.

A command that fails by printing an unstructured error and exiting has **broken this contract**,
because the consumer that asked for JSON receives something it cannot parse precisely when it most
needs to know what went wrong.

## Envelope

```json
{
  "schemaVersion": 2,
  "status": "success",
  "command": "new",
  "result": { }
}
```

```json
{
  "schemaVersion": 2,
  "status": "failure",
  "command": "new",
  "error": {
    "code": "destination_exists",
    "message": "…",
    "details": { }
  }
}
```

| Field | Rule |
|---|---|
| `schemaVersion` | **Integer**, not a string, so comparison needs no parsing. Incremented on any breaking shape change. **Currently `2`** — see *Schema history* below |
| `status` | `success` \| `failure`. Never absent |
| `command` | The command that ran |
| `result` | Present iff `status` is `success` |
| `error` | Present iff `status` is `failure` |
| `error.code` | From the registry below. **Stable** |
| `error.message` | Human-readable. **Not** stable; never parse it |
| `error.details` | Structured, code-specific |

## Error-code registry

A code is a **name that outlives its message**. Renaming one, reusing one for a different meaning,
or removing one is a breaking change requiring a `schemaVersion` bump.

This table is the registry, and it is **closed**: a code not listed here is a protocol error. The
table is not prose about the implementation — `crates/renvor-cli/src/exit.rs`'s unit test
`the_registry_matches_the_published_contract_exactly` parses these rows at compile time and fails
if the emitted set or any exit code disagrees with them, so the document and the binary cannot
drift apart silently.

| Code | Exit | Meaning |
|---|---|---|
| `usage` | 2 | Malformed invocation |
| `unsupported_value` | 3 | A flag value outside the supported set |
| `unsupported_combination` | 3 | Individually valid choices that conflict |
| `reserved_for_later_phase` | 3 | A later-phase flag. `details.phase` names it |
| `invalid_project_name` | 3 | Empty, not a valid package name, or a reserved device name |
| `destination_exists` | 3 | FR-013. The destination already exists, in **any** form. `details.rule` is always `destination_absent`; `details.found` is one of `directory`, `file`, `symlink`, `other`, or `unknown` — the last only when the destination was lost to a concurrent run and could no longer be classified. Both details are emitted identically whether the refusal came from validation or from the moment before the rename |
| `destination_rejected` | 3 | Failed a path-boundary rule. `details.rule` names which — including `destination_unverifiable`, when the destination's state could not be established at all |
| `destination_parent_missing` | 3 | The parent does not exist or does not resolve |
| `manifest_invalid` | 3 | `renvor.toml` failed validation. `details.field` and `details.constraint` |
| `project_verification_failed` | 3 | A project's own checks failed, or could not be run. `details.check` names the check; `details.stage` says where. **Not** a manifest failure and **not** a rendering failure |
| `cancelled` | 4 | The operator cancelled |
| `tool_missing` | 5 | A required tool is absent. `details.tool`, `details.required`, `details.found` |
| `container_runtime_unavailable` | 5 | `details.reason` distinguishes *not installed* from *not running* |
| `container_controls_missing` | 3 | The project has no container controls to drive. `details.expected`, `details.remedy` |
| `generation_conflict` | 3 | `renvor generate` found a target file it may not write, so **nothing was written**: one **changed since generation** — its bytes differ from the render and from the digest `.renvor/generated.toml` recorded, or it was never generated (`details.reason = changed_since_generation`, `details.changed` names them; refused with or without `--overwrite-unchanged`) — or one **regenerable** — differs from the render, digest recorded — without `--overwrite-unchanged` (`details.reason = overwrite_required`, `details.regenerable` names them, `details.flag = "--overwrite-unchanged"`). `details.paths` names every refusing path of both kinds and `details.count` says how many; `reason` is `changed_since_generation` whenever a changed path is among them. A migration import whose version another migration holds carries `details.reason = version_present` and `details.versions`. A file absent is written, a file byte-identical to the render is a no-op, and a regenerable file is replaced only under the flag (FR-048, decided 2026-09-05). Paths, never contents |
| `transport_not_wired` | 3 | Route inspection could not obtain the project's route registry, because the project declares no Renvor transport wiring. `details.transport` names the recorded transport; `details.reason` says why the registry is unreachable. **Never an empty route list and exit `0`** — an empty success is indistinguishable from an application with no routes, and the two mean different things |
| `render_failed` | 3 | Template rendering failed. Destination untouched |
| `bound_exceeded` | 3 | A documented bound was exceeded. `details.bound`, `details.limit` |
| `staging_failed` | 3 | The staging directory could not be created. **Nothing was staged**, so nothing can have been left behind |
| `placement_failed` | 3 | The final move could not be performed atomically |
| `internal` | 1 | **Unclassified. A defect** |

**There is no row for exit `0`**, and that is not an omission: this is the registry of *failures*, and success carries no error code. `0` is defined in [`command-surface.md`](command-surface.md).

### Added in Phase 011, without a version bump

`generation_conflict` was **added** for `renvor generate` (FR-048). Adding a code is not a
breaking change, by the same reasoning as the Phase 004 addition below.

### Added in Phase 004, without a version bump

`transport_not_wired` was **added** to the closed registry. That is **not** a breaking change and
therefore **not** a `schemaVersion` bump.

The rule above is precise about which operations break a consumer: *"Renaming one, reusing one for
a different meaning, or removing one is a breaking change."* A consumer pinned to `2` that meets an
unrecognised code has met a failure it does not have a specific handler for; a consumer that meets a
**removed** code has silently stopped recognising a failure it used to handle. The `1 → 2` bump was
caused by the **removal** of `destination_not_empty`, not by the four codes added alongside it.

## Schema history

### `2` — 2026-08-18

Authority: maintainer ruling of 2026-08-18, items 4 and 6. One code **removed**, four **added**.
Removing a code from a closed set is a breaking change, which is what makes this a bump rather than
an addition.

| Change | Code | Why |
|---|---|---|
| removed | `destination_not_empty` | FR-013 now refuses **every** existing destination, including an empty directory. A code whose name says "not empty" would be a false statement about the case the change introduced, so it was replaced rather than redefined |
| added | `destination_exists` | Its replacement, with `details.found` naming what was there |
| added | `project_verification_failed` | `renvor dev` reported a failing `cargo test` as `manifest_invalid`, and pre-placement verification reported a failing build as `render_failed`. Neither published meaning covers a project failing its own checks |
| added | `container_controls_missing` | `renvor docker` reported a missing `compose.yaml` as `manifest_invalid`, with `details.field = "compose.yaml"` — a field name that appears in no manifest |
| added | `staging_failed` | A staging directory that could not be **created** reported `placement_failed`, whose published meaning is that the final move failed. A consumer reading the registry would conclude a move had been attempted |

A consumer pinned to `1` that matched `destination_not_empty` would otherwise have silently stopped
recognising the most common `renvor new` failure. The version is the only thing that tells it so.

### `1` — initial

The envelope, the registry, and the redaction rule as first defined for Phase 003.

## Redaction

FR-041 applies here in full. The JSON path is not exempt from redaction because it is
machine-readable — a secret in a log a tool writes is a secret in a log.

## `--dry-run` result

`result.manifest` carries the entries from `FileManifest` (see the phase data model *(internal record)*),
sorted by path, each with `path`, `kind`, and — for files — `size` and `digest`. SC-006 requires
this to match the real run's created set exactly.
