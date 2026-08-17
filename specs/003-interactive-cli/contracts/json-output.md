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
  "schemaVersion": 1,
  "status": "success",
  "command": "new",
  "result": { }
}
```

```json
{
  "schemaVersion": 1,
  "status": "failure",
  "command": "new",
  "error": {
    "code": "destination_not_empty",
    "message": "…",
    "details": { }
  }
}
```

| Field | Rule |
|---|---|
| `schemaVersion` | **Integer**, not a string, so comparison needs no parsing. Incremented on any breaking shape change |
| `status` | `success` \| `failure`. Never absent |
| `command` | The command that ran |
| `result` | Present iff `status` is `success` |
| `error` | Present iff `status` is `failure` |
| `error.code` | From the registry below. **Stable** |
| `error.message` | Human-readable. **Not** stable; never parse it |
| `error.details` | Structured, code-specific |

## Error-code registry

A code is a **name that outlives its message**. Renaming one, or reusing one for a different
meaning, is a breaking change requiring a `schemaVersion` bump.

| Code | Exit | Meaning |
|---|---|---|
| `usage` | 2 | Malformed invocation |
| `unsupported_value` | 3 | A flag value outside the supported set |
| `unsupported_combination` | 3 | Individually valid choices that conflict |
| `reserved_for_later_phase` | 3 | A later-phase flag. `details.phase` names it |
| `invalid_project_name` | 3 | Empty, not a valid package name, or a reserved device name |
| `destination_not_empty` | 3 | FR-013 |
| `destination_rejected` | 3 | Failed a path-boundary rule. `details.rule` names which |
| `destination_parent_missing` | 3 | The parent does not exist or does not resolve |
| `manifest_invalid` | 3 | `renvor.toml` failed validation. `details.field` and `details.constraint` |
| `cancelled` | 4 | The operator cancelled |
| `tool_missing` | 5 | A required tool is absent. `details.tool`, `details.required`, `details.found` |
| `container_runtime_unavailable` | 5 | `details.reason` distinguishes *not installed* from *not running* |
| `render_failed` | 3 | Template rendering failed. Destination untouched |
| `bound_exceeded` | 3 | A documented bound was exceeded. `details.bound`, `details.limit` |
| `placement_failed` | 3 | The final move could not be performed atomically |
| `internal` | 1 | **Unclassified. A defect** |

## Redaction

FR-041 applies here in full. The JSON path is not exempt from redaction because it is
machine-readable — a secret in a log a tool writes is a secret in a log.

## `--dry-run` result

`result.manifest` carries the entries from `FileManifest` (see [`../data-model.md`](../data-model.md)),
sorted by path, each with `path`, `kind`, and — for files — `size` and `digest`. SC-006 requires
this to match the real run's created set exactly.
