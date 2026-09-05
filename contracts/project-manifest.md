---
description: "Contract C-3 — the `renvor.toml` project manifest read by `renvor check`"
version: "1.1.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. 1.1.0 (2026-09-05, Phase 011) records the auth starter under `[project]`, adds the `[framework]`, `[auth]`, and `[capabilities]` tables, and makes `cache_wired_into_application` follow the `cache` capability; every earlier manifest stays valid. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
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
transport    = "rest"          # Phase 004
auth         = "session"       # Phase 011: `none` or `session`, always recorded
local_domain = "commerce.test"
container    = true
local_https  = "off"
seed_data    = false
example_domain = true

[framework]                     # Phase 011: present iff a framework path was given (a STARTER)
source = "path"
path   = "/abs/path/to/framework"

[persistence]                   # Phase 006
database = "postgres"
orm = "sqlx"
driver_feature = "db-postgres"

[auth]                          # Phase 011: present iff auth = "session"
starter = "session"
migrations = "renvor-auth/postgres"
session_cookie = "__Host-rv_session"
mail = "smtp"

[capabilities]                  # Phase 011: always present, five recorded decisions
cache = false
jobs = false
mail = true
storage = false
observability = false

[container]                     # Phase 006, and since Phase 011:
cache_wired_into_application = true   # iff the `cache` capability is selected
```

## Rules

| Rule | Why |
|---|---|
| **Only honoured choices appear.** | FR-031 makes the manifest the input to reproduction. A recorded choice the generator did not act on describes a project that was never generated |
| **No secret, ever.** | FR-018. Structurally guaranteed: the configuration type has no field that can hold one |
| **`generator_version` and `template_version` are mandatory.** | Without both, the project cannot be reproduced (FR-024, FR-031) |
| **Unknown keys are an error.** | Fail-closed. A typo must be a diagnosis, not a silently ignored setting |
| **Validation names the field and the constraint.** | FR-019. "invalid manifest" is not an actionable message |

## The Phase 011 tables, and the compatibility rule they keep

| Table or key | Present when | Read as absent |
|---|---|---|
| `project.auth` | every manifest from template version 7 | "written before the starter was recorded" — never an error |
| `[framework]` | a framework path was given: the project is a **starter** with path dependencies | the dependency-free skeleton |
| `[auth]` | `project.auth = "session"` | no auth starter; `auth = "session"` with no `[auth]` table is refused as inconsistent |
| `[capabilities]` | every manifest from template version 7 | "written before Phase 011" |

A `true` under `[capabilities]`, or `auth = "session"`, needs a `[framework]` table: without one
nothing could have supplied the dependency, and `renvor check` refuses the manifest as describing
a project that was never generated. **No key in any of these tables can hold a secret**: the CSRF
and abuse keys the auth starter needs are read by the generated application from its
environment, and `deny_unknown_fields` refuses a manifest that grew a key field.

Template versions 1, 3, and 6 manifests keep validating, each pinned by a fixture produced by
that version's generator (`crates/renvor-cli/tests/fixtures/`).

## What is deliberately absent

`frontend`, `styling`, `render_mode`, `desktop`.

Those belong to later phases. **They are absent rather than present-and-null**, because a null field
invites a reader to conclude the choice was considered and declined, when in fact the phase could not
act on it at all. `transport`, `orm`, `database`, and `auth` were on this list once each, and left it
in the phase that honoured them.
