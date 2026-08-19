---
description: "Contract C-4 — template delivery, rendering bounds, and containment"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
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

## Output paths

Every template entry's output path is relative and contained. An entry whose rendered path would
escape the staging root is a **load-time** error, so such an entry cannot exist in a shipped binary.
