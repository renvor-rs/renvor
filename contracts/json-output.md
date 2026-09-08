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
| `generation_conflict` | 3 | `renvor generate` found a target file it may not write, so **nothing was written**: one **changed since generation** — its bytes differ from the render and from the digest `.renvor/generated.toml` recorded, or it was never generated (`details.reason = changed_since_generation`, `details.changed` names them; refused with or without `--overwrite-unchanged`) — or one **regenerable** — differs from the render, digest recorded — without `--overwrite-unchanged` (`details.reason = overwrite_required`, `details.regenerable` names them, `details.flag = "--overwrite-unchanged"`). `details.paths` names every refusing path of both kinds and `details.count` says how many; `reason` is `changed_since_generation` whenever a changed path is among them; `details.write`, `details.edit`, and (under the flag) `details.regenerate` list what the plan would have done beside the refusal, each present only when non-empty. A migration import whose version another migration holds carries `details.reason = version_present` and `details.versions`. A file absent is written, a file byte-identical to the render is a no-op, and a regenerable file is replaced only under the flag (FR-048, decided 2026-09-05). Paths, never contents |
| `record_unsupported` | 3 | `.renvor/generated.toml` carries a `record_version` newer than this generator reads, or a `tree_scope` it does not know — an unsupported **input** record, a validation failure, not a missing environment tool (U-1, approved 2026-09-07). `details.record_version` is the version found; `details.supported` is the highest this generator reads (`2`). Refused **before any file is planned or modified**; the working tree is untouched. The remedy is to rebuild the generator, not the project. The reason string and the two `details` keys are the same text in [`command-surface.md`](command-surface.md), in `tests/json/record_unsupported.json`, and in the help and README sentences |
| `transport_not_wired` | 3 | Route inspection could not obtain the project's route registry, because the project declares no Renvor transport wiring. `details.transport` names the recorded transport; `details.reason` says why the registry is unreachable. **Never an empty route list and exit `0`** — an empty success is indistinguishable from an application with no routes, and the two mean different things |
| `render_failed` | 3 | Template rendering failed. Destination untouched |
| `bound_exceeded` | 3 | A documented bound was exceeded. `details.bound`, `details.limit` |
| `staging_failed` | 3 | The staging directory could not be created. **Nothing was staged**, so nothing can have been left behind |
| `placement_failed` | 3 | The final move could not be performed atomically |
| `internal` | 1 | **Unclassified. A defect** |

**There is no row for exit `0`**, and that is not an omission: this is the registry of *failures*, and success carries no error code. `0` is defined in [`command-surface.md`](command-surface.md).

### Added in Phase 012, without a version bump

`record_unsupported` was **added** for the provenance record's version dispatch (Phase 012, L-2,
FR-012-5b). Adding a code is not a breaking change, by the same reasoning as the two additions
below, so neither `schemaVersion` nor this document's version moves. Its exit code is `3` — a
validation failure of an unsupported *input* — not `5`, because nothing in the environment is
missing (U-1, approved 2026-09-07). Two `project_verification_failed` reasons join it as
`details.reason` values, `compiler_identity_unreadable` and `evidence_capture_failed` (and, for
`renvor generate auth`, `toolchain_resolution_diverged`; for the isolation the preflight needs,
`probe_isolation_unavailable`), and `tool_missing` gains `details.remedy` and the `details.reason`
values `proxy_unidentified` and `no_install_guarantee_unconfirmed`; a new `details` key or reason
value under an existing code is additive under the same rule. The success payloads gain the
fields of §"`result.toolchain` and `result.verified_with`" below, additive likewise.

**The TOML record's `record_version` is a different axis.** It is the format version of a file
the generator owns, governed by [`template-contract.md`](template-contract.md) §"Record version 2",
and its move from absent to `2` is not a `schemaVersion` change: the envelope's shape did not
change, and a consumer pinned to `2` reads every document this revision emits.

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

## `result.toolchain` and `result.verified_with` (Phase 012, L-2 — additive)

`renvor new --output json` (a real run), `renvor generate … --output json`, and `renvor check
--output json` carry the provenance record's two new tables under the success payload. The Phase
012 brief (§5.2, §6.4) names them `data.toolchain` and `data.verified_with`; `data` there is the
success payload, which this envelope has always called `result`, so the paths are
`result.toolchain` and `result.verified_with`. Their fields are named as the brief names them —
`snake_case`, mirroring the TOML record they are read from — and every value is **measured or
read back, never derived** ([`template-contract.md`](template-contract.md) §"Record version 2").
Both are `null` for a legacy record (no `record_version`), never filled in; `renvor new --dry-run`
carries neither, because a dry run verifies nothing and writes no record.

```json
{
  "schemaVersion": 2,
  "status": "success",
  "command": "check",
  "result": {
    "toolchain": { "pinned": "1.94.0", "rust_version": "1.94.0" },
    "verified_with": {
      "operation": "new",
      "verified_at": "2026-09-07T00:00:00Z",
      "tree_scope": 1,
      "tree_digest": "sha256:…",
      "historical": false,
      "observation": "launched",
      "rustc_release": "1.94.0",
      "rustc_commit": "4a4ef493e",
      "rustc_host": "aarch64-apple-darwin",
      "resolved_rustc_release": "1.94.0",
      "resolved_rustc_commit": "4a4ef493e",
      "configured_rustc_release": null,
      "configured_rustc_commit": null,
      "cargo_release": "1.94.0",
      "cargo_commit": "85eff7c80",
      "rustup": "1.29.0",
      "proxy": true,
      "selected_by": "toolchain_file",
      "rustc_override": false,
      "wrapper": false,
      "rustflags": false,
      "rustdocflags": false,
      "checks": {
        "fmt":    { "outcome": "passed" },
        "clippy": { "outcome": "passed", "units_launched": 2, "units_fresh": 0, "driver_release": "0.1.94", "driver_commit": "4a4ef493e3" },
        "build":  { "outcome": "passed", "units_launched": 1, "units_fresh": 0 },
        "test":   { "outcome": "passed", "units_launched": 2, "units_fresh": 0 },
        "run":    { "outcome": "passed" }
      }
    }
  }
}
```

| Field | Rule |
|---|---|
| `toolchain.pinned`, `toolchain.rust_version` | strings; `"none"` for a tree `generate auth` verified without a pin (a legacy tree) |
| `verified_with.operation` | `new` \| `auth` — the operation whose five checks the table describes |
| `verified_with.verified_at` | RFC 3339, UTC |
| `verified_with.tree_scope`, `verified_with.tree_digest` | the scope number and the digest of FR-012-5d |
| `verified_with.historical` | **boolean, computed by the reading command**, not stored: `true` when the tree digest recomputed over the current working tree under `tree_scope` differs from `tree_digest` — the evidence is of a tree that is not the current one; `false` when they are equal. `renvor new` and `generate auth` report `false` for the tree they just verified |
| `verified_with.observation` | `launched` \| `cached` \| `mixed`, over the build and test units of the project's own package(s) |
| `verified_with.rustc_release`, `rustc_commit`, `rustc_host` | the **observed** launched compiler's `-vV` answer — **`null` when `observation` is `cached`** (observed compiler identity unavailable), the launched units' identity only when `mixed`. Never filled from the pin, `PATH`, an old record, or `.rustc_info.json` |
| `verified_with.resolved_rustc_release`, `resolved_rustc_commit` | the preflight resolution's identity, **always present**; a queried resolution, never an observation, never copied into `rustc_*`; not Cargo's effective compiler under `RUSTC`, `build.rustc`, or a wrapper |
| `verified_with.configured_rustc_release`, `configured_rustc_commit` | **nullable** and labelled: Cargo's configured resolution, queried separately when it was; never an observation |
| `verified_with.cargo_release`, `cargo_commit` | `cargo -vV` under the seal |
| `verified_with.rustup` | the located rustup's version, or `"absent"` |
| `verified_with.proxy` | boolean: the resolved `rustc` is a rustup proxy |
| `verified_with.selected_by` | `environment` \| `directory_override` \| `toolchain_file` \| `default` \| `no_rustup` \| `unknown` |
| `verified_with.rustc_override`, `wrapper`, `rustflags`, `rustdocflags` | booleans, **presence only** — no value, path, or flag text is ever carried |
| `verified_with.checks.fmt`, `checks.run` | `outcome` only |
| `verified_with.checks.clippy` | `outcome`, `units_launched`, `units_fresh`, and `driver_release`/`driver_commit` — the observed `clippy-driver` executable's own `--version` answer, **`null` when every clippy unit was `Fresh`**, never filled from `cargo clippy --version` |
| `verified_with.checks.build`, `checks.test` | `outcome`, `units_launched`, `units_fresh` |

Every string is parsed under the identity grammar before it is emitted; the redaction rule above
applies. **`result.toolchain` of `renvor doctor` is reserved** (the brief's `data.doctor.toolchain`):
the toolchain section of `doctor` is delivered by the follow-up batch B1f (FR-012-11), and until
then the key is absent from `doctor`'s payload; when it arrives it is `null` outside a project.
*(Key spelling: the fields are named here exactly as the brief names them, in `snake_case`,
mirroring the TOML record; the envelope's existing keys are `camelCase` (`schemaVersion`,
`templateVersion`, `orphanedStaging`). The implementation pull request that carries these fields
confirms the spelling with its `tests/json` fixtures, and this section moves with it — the fields'
meanings do not.)*
