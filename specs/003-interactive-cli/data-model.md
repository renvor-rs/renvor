# Phase 003 — Data model

**Feature**: [`spec.md`](spec.md) · **Plan**: [`plan.md`](plan.md) · **Created**: 2026-08-17

Five entities. None is persisted to a database; four live for the duration of one command, and one —
the project manifest — is written into the generated project and read back later.

Every invariant below is stated so that its violation is **detectable**, because an invariant nobody
can check is a comment.

## 1. `ProjectConfiguration` — the one validated model

The single input to generation. **Produced identically by the wizard and by flags**, which is
constitution VII's requirement and FR-006's.

| Field | Type | Constraint |
|---|---|---|
| `name` | project name | Non-empty; a valid Rust package name; not a platform-reserved device name; no path separator |
| `destination` | absolute path | Validated by §5's rules before construction succeeds |
| `local_domain` | hostname | Valid DNS label sequence; defaults from `name` |
| `target` | enum | **`api` only in this phase.** Other variants are reserved (FR-005b) |
| `container` | bool | Whether container development controls are generated |
| `local_https` | enum | `off` \| `requested`. **`requested` records intent and issues nothing** (FR-036) |
| `seed_data` | bool | |
| `example_domain` | bool | |

### Invariants

- **I-1 — Construction implies validity.** There is no way to hold a `ProjectConfiguration` that has
  not passed every individual and cross-field check. Validation is not a method that callers must
  remember to call; it is the only constructor. This is what makes FR-007's "before any filesystem
  write" true structurally rather than by ordering discipline.
- **I-2 — Serialization is total and canonical.** Two configurations that are equal serialize
  byte-identically, and the serialization does not depend on how the values were obtained. **SC-003
  is a direct test of this invariant**, not of the two interfaces separately.
- **I-3 — No secret may be held.** The type carries no password, token, key, or credential field.
  FR-018 is therefore enforced by the type's shape rather than by a filter at write time.
- **I-4 — Reserved choices are representable but not constructible.** A later-phase value parses
  from a flag and is rejected during validation with the phase named. It never reaches generation
  and never reaches the manifest.

## 2. `TemplateSet` — versioned, embedded, inert

| Field | Type | Constraint |
|---|---|---|
| `version` | version string | Recorded in the generated manifest (FR-024) |
| `entries` | list of template entries | Each has a relative output path and a body |

### Invariants

- **I-5 — Every output path is relative and contained.** A template entry whose rendered output path
  escapes the staging root is a **load-time** error, not a render-time one, so a malicious entry
  cannot exist in a built binary.
- **I-6 — No ambient capability.** The rendering environment exposes an allow-listed set of filters
  and functions. Filesystem access, process execution, and network access are absent rather than
  disabled (FR-027).
- **I-7 — Undefined is an error.** A referenced variable that is not provided fails the render
  (FR-028). An empty rendering is not an acceptable result.
- **I-8 — Expansion is bounded.** Recursion depth, total output bytes, and output file count all
  have documented limits (FR-026, SC-013).

## 3. `FileManifest` — one artifact, three jobs

Used for `--dry-run` output, for verification before the final move, and for reproducibility. It is
deliberately **one** structure; see [`plan.md`](plan.md) §Complexity Tracking for why three would
drift.

| Field | Type | Constraint |
|---|---|---|
| `entries` | ordered list | Sorted by path, so two runs produce identical output regardless of traversal order |
| `entry.path` | relative path | Relative to the project root; never absolute; never escaping |
| `entry.kind` | enum | `file` \| `directory` |
| `entry.size` | bytes | Files only |
| `entry.digest` | SHA-256 | Files only |

### Invariants

- **I-9 — Order is total and content-independent.** Sorting is by path, so the manifest is
  reproducible (SC-016) and diffable.
- **I-10 — The dry-run manifest and the real manifest are the same type.** SC-006 requires them to
  match exactly; producing them from one code path is what makes that testable rather than hopeful.
- **I-11 — The manifest describes what was created, not what links point at.** Traversal does not
  follow symbolic links.

## 4. `ProjectManifest` — `renvor.toml`

The record written into the generated project.

| Field | Type | Constraint |
|---|---|---|
| `renvor.generator_version` | version | The `renvor` version that generated the project |
| `renvor.template_version` | version | From `TemplateSet::version` |
| `project.*` | the honoured selections | **Only choices the generator acted on** |

### Invariants

- **I-12 — Every recorded choice was honoured.** A selection appears here only if generation acted
  on it. This is what makes FR-031's "reproducible from the manifest" true; a manifest recording an
  unhonoured choice would describe a project that was never generated.
- **I-13 — No secret, ever.** FR-018. Enforced by I-3 upstream: a value that cannot be held cannot be
  written.
- **I-14 — Round-trips.** `renvor check` parses it and reports the field and the constraint on
  failure (FR-019).

## 5. `DestinationPath` — the boundary

Not a path, but a path that has passed the boundary. **The component that constructs it is gated on
the D6 decision record** ([`research.md`](research.md)).

### Validation rules, each with a rejection reason

| Rule | Rejects |
|---|---|
| No traversal component | `../` and its platform variants |
| No absolute path where relative is expected | absolute-path injection |
| Final component is a single name | a name containing a separator |
| Not a platform-reserved device name | `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9` on Windows |
| Parent exists and canonicalises | a destination under a path that does not resolve |
| Destination does not exist **at all** | FR-013 — an empty directory, a non-empty directory, a regular file, and a symbolic link are all refused, and so is an entry whose state cannot be established |
| Canonical destination is inside the canonical parent | symlink escape |

### Invariants

- **I-15 — Rejection precedes creation.** No filesystem entry is created before every rule above has
  passed (FR-039, SC-009).
- **I-16 — The boundary is structural, not checked.** *(Revised 2026-08-18 with research.md D6
  revision 2.)* Containment comes from `cap_std::fs::Dir` handles, so a code path that forgets to
  validate **cannot** escape — there is no ambient path API in scope to escape with. The earlier
  version of this invariant recorded the opposite as a deliberate weakening; that weakening no
  longer exists and the invariant is stated in the direction that is now true. What remains checked
  is *name* validation — Windows reserved device names and package-name characters — because those
  are not resolution questions and no capability can decide them. **Exactly one ambient call exists**
  (`Dir::open_ambient_dir` on the destination's parent), and it is deliberate: the operator typed
  that path.
- **I-17 — Time-of-check to time-of-use is narrowed, not eliminated.** Between opening the parent
  handle and the final rename, another process holding write access to that directory can still
  create the destination. The rename refuses an existing destination, which converts the race into a
  **clean failure rather than an overwrite**. The window is narrower than a path-based design's,
  because both the check and the rename go through one open handle rather than being re-resolved
  from a string each time — but it is not zero, and this invariant does not claim it is.

## Entity relationships

```text
flags ─┐
       ├─▶ ProjectConfiguration ──▶ render(TemplateSet) ──▶ staging dir ──▶ FileManifest
prompts┘            │                                            │              │
                    │                                            │              ├─▶ --dry-run output
                    └──────────────▶ ProjectManifest ────────────┘              ├─▶ pre-move verification
                                     (written into staging)                     └─▶ reproducibility check
                                                                 │
                                          DestinationPath ───────┴──▶ one rename ──▶ destination
```

**The single arrow into `ProjectConfiguration` from two sources is the whole design.** Everything
downstream sees one value and cannot tell which interface produced it — which is exactly what SC-003
asserts.
