---
description: "Contract C-3 — the `renvor.toml` project manifest read by `renvor check`"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract C-3 — `renvor.toml`

**Status**: defined before implementation. Read by `renvor check`; the input to reproducibility.

## Shape

```toml
[renvor]
generator_version = "0.0.0"
template_version  = "1"

[project]
name         = "commerce"
target       = "api"
local_domain = "commerce.test"
container    = true
local_https  = "off"
seed_data    = false
example_domain = true
```

## Rules

| Rule | Why |
|---|---|
| **Only honoured choices appear.** | FR-031 makes the manifest the input to reproduction. A recorded choice the generator did not act on describes a project that was never generated |
| **No secret, ever.** | FR-018. Structurally guaranteed: the configuration type has no field that can hold one |
| **`generator_version` and `template_version` are mandatory.** | Without both, the project cannot be reproduced (FR-024, FR-031) |
| **Unknown keys are an error.** | Fail-closed. A typo must be a diagnosis, not a silently ignored setting |
| **Validation names the field and the constraint.** | FR-019. "invalid manifest" is not an actionable message |

## What is deliberately absent

`transport`, `orm`, `database`, `auth`, `frontend`, `styling`, `render_mode`, `desktop`.

Those belong to later phases. **They are absent rather than present-and-null**, because a null field
invites a reader to conclude the choice was considered and declined, when in fact the phase could not
act on it at all.
