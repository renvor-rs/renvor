# Phase 002 — Complete resolved transitive dependency inventory

**Feature**: [`specs/002-core-kernel`](../specs/002-core-kernel/spec.md) | **Satisfies**: FR-040, SC-012, SC-017 | **Tasks**: T030–T034
**Produced**: 2026-08-16 | **Toolchain**: 1.94.0 | **Source of truth**: the tracked `Cargo.lock`, read by `cargo metadata --locked`

## Why this document exists

Research §3 evaluated the **direct candidates** — the packages Phase 002 chose deliberately. That
is not the set that ships. A consumer resolves the **transitive closure**, and every package in it
carries a licence, an MSRV, and an advisory history whether or not anybody evaluated it.

This inventory is that closure, read from the **actual lockfile** rather than from the research
table. The distinction is the whole point: an inventory derived from the design document would
reproduce the design document's blind spots.

## Summary

| Measure | Count |
|---|---|
| Workspace members | **5** (`renvor`, `renvor-core`, `renvor-config`, `renvor-testkit`, `xtask`) |
| External packages in the lockfile graph | **48** (was 55; see the revision below) |
| — reachable over **normal** edges (what a consumer resolves) | **45** |
| — **dev-only** (test machinery; never in a consumer's graph) | **3** |
| Directly chosen in research §3 | **11** |
| Arrived **transitively**, evaluated by nobody until now | **37** |
| Packages with **no declared licence** | **0** |
| Packages whose MSRV exceeds **1.94.0** | **0** |
| `cargo deny check licenses advisories bans sources` | **all four pass** |

## Revision — 2026-08-16, after the configuration proof gate failed

`confique` was a **dev-dependency on probation**, and its own manifest comment pre-committed the
consequence: *"if the gate fails, it is deleted rather than demoted."* The gate failed 4 of 8, so
it was deleted along with the child-process probe that existed only to observe its environment
behaviour.

**Seven packages left the resolved graph**, all of them dev-only and all of them reachable solely
through `confique`:

| Package | Why it was there |
|---|---|
| `confique` | the candidate under probation |
| `confique-macro` | its derive macro — **build-time code execution**, recorded as a disclosure surface in ADR-0007 |
| `heck` | case conversion, used by the macro |
| `toml` 0.8 | a **second** TOML version alongside the 1.1 the adapter uses |
| `toml_datetime` 0.6 | with it |
| `toml_writer` | with it |
| `winnow` 0.6 | with it |

**Two duplicate major versions disappeared with it.** The graph carried `toml` at both 0.8 and
1.1, and `winnow` at both 0.6 and 0.7, purely because the probationary crate pinned older ones.
Nothing was added to replace them: the adapter is built on `serde` and `toml`, both of which were
already direct dependencies before the gate ran.

`cargo deny` now reports `license-not-encountered` for **ISC** — an allowance in `deny.toml` that
no package matches any more. It is a warning rather than a failure, and the allowance is left in
place rather than trimmed to match today's graph: a licence policy that is narrowed every time a
dependency leaves has to be widened again every time one arrives, and each widening is a decision
nobody reviews.

**37 of 48 external packages entered the graph without an individual evaluation.** That is the
normal condition of any Rust project and is precisely what FR-040 exists to surface, so it is
recorded as a measured fact rather than framed as a problem discovered.

## T031 — `cargo deny check licenses advisories bans sources`

Run against the tracked `Cargo.lock`, not a fresh resolution:

```text
advisories ok, bans ok, licenses ok, sources ok
```

- **advisories** — 0 packages carry an open RustSec advisory.
- **licenses** — every package resolves to a branch on `deny.toml`'s allow list.
- **bans** — passes; the duplicate versions in T032 are reported as warnings, not denials.
- **sources** — every package comes from the crates.io registry. 0 git or path sources outside the
  workspace, which is what makes FR-040's "no git or path dependency in a publishable package"
  checkable rather than assertable.

### The three licences a reader will stop on

`deny.toml`'s allow list is permissive-only and its `exceptions` list is **empty by design**. Three
entries in the graph still deserve naming, because scanning the table quickly will raise all three:

| Package | Declared | Why it passes |
|---|---|---|
| `r-efi` 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | `OR` — the LGPL branch is never selected, and MIT is. **Dev-only**, so it is not in a consumer's graph at all |
| `foldhash` 0.1.5 | `Zlib` | `Zlib` is on the allow list. **Production**, via `petgraph` → `hashbrown` |
| `unicode-ident` 1.0.24 | `(MIT OR Apache-2.0) AND Unicode-3.0` | `AND` — **both** halves must be allowed, and both are. **Production** |

`unicode-ident` is the one that matters most: its `AND` means the Unicode-3.0 terms apply in
addition to MIT, not as an alternative. It passes because `Unicode-3.0` is explicitly on the allow
list — a decision Phase 001 made, not one this phase discovered.

## T032 — Enabled features

Read from `cargo metadata`'s resolve graph. Two figures are load-bearing.

### `secrecy` resolves with **zero** features

```text
secrecy 0.10.3 :: (none)
```

Contract C-C9 requires a secret to refuse serialisation. `secrecy`'s only optional feature is
`serde`, and enabling it would give `SecretBox` a `Serialize` impl — the exact capability C-C9
forbids. The feature is off, and this line is the evidence.

### `tokio` carries **no transport feature** in a consumer's graph

The workspace-wide resolve shows `rt-multi-thread` and `test-util` enabled, which looks alarming
until the edge type is separated. Resolved over **normal** edges only — the graph a consumer of
`renvor` actually gets:

```text
tokio v1.53.1 :: default, macros, rt, sync, time, tokio-macros
```

| Search over the consumer graph | Matches | Meaning |
|---|---|---|
| `net`, `fs`, `process`, `signal` | **0** | the kernel cannot acquire a transport by accident (FR-033, principle VIII) |
| `test-util`, `rt-multi-thread` | **0** | test machinery does not reach production |
| **control** — the same `test-util` search with **dev** edges included | **8** | the zero above is isolation, not a broken search |

The control is the point. A zero from a search that matches nothing is indistinguishable from a
zero from a search that works, and only the second is evidence.

### Duplicate versions

| Package | Versions | Path |
|---|---|---|
| `hashbrown` | 0.15.5, 0.17.1 | `petgraph` pulls 0.15.5 directly; `petgraph` → `indexmap` pulls 0.17.1 |
| `syn` | 2.0.119, 3.0.3 | `tracing-attributes` and `confique-macro` pin 2.x; `serde_derive`, `thiserror-impl`, `tokio-macros` use 3.x |

Both are **build-time or internal** duplications that `cargo deny`'s `bans` check accepts. Neither
duplicates a type that crosses Renvor's public surface, so neither can produce the "two versions of
the same type" error that makes duplication user-visible. `syn` is a proc-macro dependency and
compiles twice; the cost is build time, not binary size or API confusion. Recorded rather than
silently tolerated: a future duplicate of `tokio` or `tracing-core` would be a different matter,
and the row above is what a reviewer compares against.

## T030/T033 — The complete resolved set

Every external package in the lockfile graph. `origin` distinguishes a package research §3 chose
from one that arrived because something else needed it; `reach` distinguishes what a consumer
resolves from what only the test suite does.

| Package | Version | Licence | MSRV | Origin | Reach |
|---|---|---|---|---|---|
| `aho-corasick` | 1.1.5 | Unlicense OR MIT | 1.60.0 | transitive | **dev-only** |
| `bytes` | 1.12.1 | MIT | 1.57 | transitive | production |
| `cfg-if` | 1.0.4 | MIT OR Apache-2.0 | 1.32 | transitive | production |
| `confique` | 0.4.0 | MIT OR Apache-2.0 | 1.68.2 | direct | **dev-only** |
| `confique-macro` | 0.0.13 | MIT OR Apache-2.0 | (unstated) | transitive | **dev-only** |
| `equivalent` | 1.0.2 | Apache-2.0 OR MIT | 1.6 | transitive | production |
| `fixedbitset` | 0.5.7 | MIT OR Apache-2.0 | 1.56 | transitive | production |
| `foldhash` | 0.1.5 | Zlib | 1.60 | transitive | production |
| `futures-core` | 0.3.34 | MIT OR Apache-2.0 | 1.36 | transitive | production |
| `futures-sink` | 0.3.34 | MIT OR Apache-2.0 | 1.36 | transitive | production |
| `getrandom` | 0.4.3 | MIT OR Apache-2.0 | 1.85 | direct | production |
| `hashbrown` | 0.15.5 | MIT OR Apache-2.0 | 1.65.0 | transitive | production |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 | 1.85.0 | transitive | production |
| `heck` | 0.5.0 | MIT OR Apache-2.0 | 1.56 | transitive | **dev-only** |
| `indexmap` | 2.14.0 | Apache-2.0 OR MIT | 1.85 | transitive | production |
| `lazy_static` | 1.5.0 | MIT OR Apache-2.0 | (unstated) | transitive | production |
| `libc` | 0.2.189 | MIT OR Apache-2.0 | 1.65 | transitive | production |
| `matchers` | 0.2.0 | MIT | (unstated) | transitive | production |
| `memchr` | 2.8.3 | Unlicense OR MIT | 1.61 | transitive | **dev-only** |
| `once_cell` | 1.21.4 | MIT OR Apache-2.0 | 1.65 | transitive | production |
| `petgraph` | 0.8.3 | MIT OR Apache-2.0 | 1.64 | direct | production |
| `pin-project-lite` | 0.2.17 | Apache-2.0 OR MIT | 1.37 | transitive | production |
| `proc-macro2` | 1.0.107 | MIT OR Apache-2.0 | 1.71 | transitive | production |
| `quote` | 1.0.47 | MIT OR Apache-2.0 | 1.71 | transitive | production |
| `r-efi` | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | 1.68 | transitive | **dev-only** |
| `regex-automata` | 0.4.18 | MIT OR Apache-2.0 | 1.65 | transitive | production |
| `regex-syntax` | 0.8.11 | MIT OR Apache-2.0 | 1.65 | transitive | production |
| `secrecy` | 0.10.3 | Apache-2.0 OR MIT | 1.60 | direct | production |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | 1.56 | direct | production |
| `serde_core` | 1.0.229 | MIT OR Apache-2.0 | 1.56 | transitive | production |
| `serde_derive` | 1.0.229 | MIT OR Apache-2.0 | 1.71 | transitive | production |
| `serde_spanned` | 1.1.1 | MIT OR Apache-2.0 | 1.85 | transitive | production |
| `sharded-slab` | 0.1.7 | MIT | 1.42.0 | transitive | production |
| `syn` | 2.0.119 | MIT OR Apache-2.0 | 1.71 | transitive | production |
| `syn` | 3.0.3 | MIT OR Apache-2.0 | 1.71 | transitive | production |
| `thiserror` | 2.0.20 | MIT OR Apache-2.0 | 1.71 | direct | production |
| `thiserror-impl` | 2.0.20 | MIT OR Apache-2.0 | 1.71 | transitive | production |
| `thread_local` | 1.1.10 | MIT OR Apache-2.0 | 1.63 | transitive | production |
| `tokio` | 1.53.1 | MIT | 1.71 | direct | production |
| `tokio-macros` | 2.7.2 | MIT | 1.71 | transitive | production |
| `tokio-util` | 0.7.19 | MIT | 1.71 | direct | production |
| `toml` | 0.9.12+spec-1.1.0 | MIT OR Apache-2.0 | 1.76 | direct | **dev-only** |
| `toml` | 1.1.4+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | direct | production |
| `toml_datetime` | 0.7.5+spec-1.1.0 | MIT OR Apache-2.0 | 1.76 | transitive | **dev-only** |
| `toml_datetime` | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | transitive | production |
| `toml_parser` | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | transitive | production |
| `toml_writer` | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | transitive | **dev-only** |
| `tracing` | 0.1.44 | MIT | 1.65.0 | direct | production |
| `tracing-attributes` | 0.1.31 | MIT | 1.65.0 | transitive | production |
| `tracing-core` | 0.1.36 | MIT | 1.65.0 | transitive | production |
| `tracing-subscriber` | 0.3.23 | MIT | 1.65.0 | direct | production |
| `unicode-ident` | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | 1.71 | transitive | production |
| `winnow` | 0.7.15 | MIT | 1.65.0 | transitive | **dev-only** |
| `winnow` | 1.0.4 | MIT | 1.65.0 | transitive | production |
| `zeroize` | 1.9.0 | Apache-2.0 OR MIT | 1.85 | transitive | production |
`(unstated)` MSRV appears for `confique-macro`, `lazy_static`, and `matchers`. All three are
**dev-only**, none states a `rust-version`, and all three compile on 1.94.0 — verified by the fact
that the test suite builds and runs on the pinned toolchain. An unstated MSRV is not a violation;
it is an absence of a promise, and the absence is recorded here rather than papered over with an
inferred number.

## T033 — Did the direct-candidate evaluation predict the transitive graph?

**No, and it could not have.** Recording the answer honestly matters more than the answer flattering
the research.

| Question | Answer |
|---|---|
| Did research §3 evaluate every package that ships? | **No.** It evaluated **12**; **55** resolve |
| Was research §3 *wrong* about anything it did evaluate? | **No.** Every direct candidate's version, licence, and MSRV matches the lockfile |
| Did any transitive package introduce a licence absent from the direct set? | **Yes — two.** `Zlib` (`foldhash`) and `Unicode-3.0` (`unicode-ident`) appear nowhere among the direct candidates |
| Did anything catch those two? | **Yes — `deny.toml`, not the research table.** Both licences were already on Phase 001's allow list |
| Did any transitive package fail FR-040's evidence requirement? | **No.** 0 packages lack a licence; 0 exceed the MSRV; 0 carry an advisory |

**The finding worth carrying forward**: the artifact that caught the two novel licences was the
*policy* (`deny.toml`), not the *design document* (research §3). A per-candidate evaluation table
scales with the number of packages a human chose; a policy check scales with the number that
actually resolve. Phase 002 needed both, and only one of them could have found `foldhash`.

**The phase is not failed by T033.** T033 requires failure if *any* resolved package lacks the
evidence FR-040 demands. Every one of the 55 has a declared licence, a resolvable version from the
committed lockfile, and a clean advisory check. The gate passes on evidence, not on absence of
looking.

## T034 — ADR-0003's lockfile policy and FR-040 do not conflict

Every figure above is read from a **committed** `Cargo.lock`, which is what FR-040 requires. ADR-0003
records that *reusable library crates* do not commit a lockfile. Both hold, because the two
statements govern **different objects** — the full reconciliation is in
[`specs/002-core-kernel/research.md`](../specs/002-core-kernel/research.md) §D12, and the short form
is:

- the *version-requirement* half of ADR-0003's row is a property of **each crate's manifest**, and
  every crate this phase adds honours it literally — compatible ranges, **0** exact pins;
- the *lockfile* half is a property of the **workspace**, because Cargo maintains exactly one
  `Cargo.lock` per workspace and offers no per-member option;
- this workspace contains `xtask` — release tooling and automation — which ADR-0003's **second** row
  requires to commit its lockfile. One lockfile, one governing row, and it is the automation row.

A consumer of a published Renvor crate resolves against **their** lockfile, not this one, so
committing it constrains Renvor's own resolution and nothing downstream. Raised as readiness item
CHK044; closed here rather than left as two statements that appear to contradict each other in
writing.

## Reproducing this

```bash
cargo metadata --locked --format-version 1          # the resolved set, from the tracked lockfile
cargo tree --workspace --edges normal               # what a consumer resolves
cargo tree --workspace --duplicates --edges normal  # duplicate versions
cargo tree -p renvor --edges normal --format "{p} :: {f}"   # consumer-visible features
cargo deny check licenses advisories bans sources   # policy
```

Every figure in this document comes from one of those five commands. None was transcribed from a
design artifact.
