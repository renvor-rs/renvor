# Phase 002 — Complete resolved transitive dependency inventory

**Feature**: [`specs/002-core-kernel`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/spec.md) | **Satisfies**: FR-040, SC-012, SC-017 | **Tasks**: T030–T034
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
| Directly chosen, declared in a workspace manifest | **11** |
| Arrived **transitively** (declared in no workspace manifest) | **37** |
| Never **individually evaluated** by research §3 | **37** |
| Packages with **no declared licence** | **0** |
| Packages whose MSRV exceeds **1.94.0** | **0** |
| `cargo deny check licenses advisories bans sources` | **all four pass** |

**Why 38 and 37 are both right, and why the row above used to say only one of them.** They measure
different things and differ by exactly one package. *Transitive* means "declared in no workspace
manifest" — 38 rows. *Never individually evaluated* means "absent from research §3's candidate
table" — 37 rows. The difference is **`zeroize`**, which research §3 evaluated explicitly (as
`secrecy`'s dependency) but which no manifest of ours declares. Until 2026-08-16 this table carried
one row reading *"Arrived transitively, evaluated by nobody until the inventory | 38"*, which
attached the second label to the first measure and so overstated the unevaluated set by one. Every
figure in this section is reproduced by the Gate 15 comparison in
[`quickstart.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/quickstart.md), which now reads the prose as well as the
table.

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

**37 of 48 external packages entered the graph without an individual evaluation** — 48 resolved
rows, minus the 11 of research §3's 12 candidates that are still in the graph. That is the normal
condition of any Rust project and is precisely what FR-040 exists to surface, so it is recorded as
a measured fact rather than framed as a problem discovered.

## T143 — FR-040 evaluation: `libc` promoted from transitive to direct

FR-035 requires a recorded evaluation for **every selected external dependency**, covering version,
licence, maintenance status, MSRV compatibility, advisories, and feature cost. `libc` was already
in the graph and already inventoried; T143 made it a *chosen* dependency of `renvor-config`, so it
now needs an evaluation rather than only a row.

| Field | Finding |
|---|---|
| Version requirement | `0.2.189`, a compatible requirement — not an exact pin (SUPPORT.md's rule for library crates) |
| Resolved | `0.2.189` — **the same version already in the lockfile** |
| Licence | `MIT OR Apache-2.0`; both branches on `deny.toml`'s allow list |
| MSRV | **1.65**, comfortably under the 1.94.0 floor |
| Maintenance | `rust-lang/libc`, maintained by the Rust project's libs team |
| Advisories | none open; `cargo deny check advisories` passes |
| Feature cost | `default-features = false`. Nothing is enabled beyond the constant used |
| **New packages added to the resolved graph** | **0** |

### Why the resolved set did not change

`libc` was already reachable as `renvor-core → getrandom 0.4.3 → libc 0.2.189`. Declaring it in
`renvor-config` promotes a node that was already present; it adds none. Measured rather than
assumed: the lockfile held **48** external packages before this change and holds **48** after, and
the entire `Cargo.lock` diff is one line adding `libc` to `renvor-config`'s dependency list.

### What was considered instead

| Option | New packages | `unsafe` in Renvor | Verdict |
|---|---|---|---|
| **`libc` + `std`'s `OpenOptionsExt::custom_flags`** | **0** | **none** | **chosen** |
| `rustix` (`fs` feature) | 3 — `bitflags`, `errno`, `linux-raw-sys` | none | rejected: three new packages, three more licences to clear, and three more FR-040 rows, to obtain a constant `libc` already exposes and which `std` already knows how to apply |
| `nix` | several | none | rejected: a much larger surface for one flag |
| Hand-rolled `open(2)` via FFI | 0 | **required** | rejected outright: the workspace declares `unsafe_code = "forbid"` |

Only the **integer constant** `O_NONBLOCK` is taken from `libc`. The open itself goes through
`std::fs::OpenOptions`, so no `unsafe` block and no FFI call appears anywhere in Renvor's source.

The dependency is target-gated under `[target.'cfg(unix)'.dependencies]`: `O_NONBLOCK` is a POSIX
flag and `libc` does not define it on Windows. The **TOCTOU** half of the fix — taking metadata
from the open descriptor instead of re-resolving the pathname — is platform-independent and applies
on every platform.

## T160 — FR-040 evaluation: the documentation site's npm changes (ADR-0009)

**Added 2026-08-17.** Until now this document was **Cargo-only**, and that was a real gap rather
than a scoping choice: ADR-0009's Compliance table claimed *"the change is recorded in the
dependency inventory alongside the resolved graph it produces"*, and **it was not recorded here at
all**. FR-040 says *"every external dependency introduced by this phase"* without restricting itself
to one package manager, and Phase 002 changed two npm dependencies. The claim is made true here
rather than narrowed away.

**Scope statement, so the boundary is explicit.** Every count in the Summary above and in T030/T033
below describes the **Cargo** closure and is unchanged by this section. The documentation site is a
separate npm graph that ships **no** code to a consumer of the Renvor crates — it produces a static
website. It is inventoried because FR-040 says so, not because it reaches a crate consumer.

| Package | Version | Licence | Maintenance | Engines / MSRV analogue | Advisories | How it entered |
|---|---|---|---|---|---|---|
| `image-size` → **`vendor/image-size-disabled`** | `3.0.0-renvor.1` (local, **not published**) | MIT | **First-party.** Maintained by this project; six files tracking the real package's export map | `node >= 16.x`; `.nvmrc` pins 22 | **None** — it contains no parser. It replaces a package carrying `GHSA-w3rx-r6r6-pgpr` and `GHSA-5p2g-fcmc-qvqq`, both **high**, CVSS 7.5, **no fixed version at any release** | `overrides: {"image-size": "$image-size"}` paired with a root `dependencies` entry `"image-size": "file:./vendor/image-size-disabled"`. **ADR-0009** |
| `uuid` | **11.1.1** | MIT / Apache-2.0 | Actively maintained | `node >= 16` | The Medium finding against `< 11.1.1` is **fixed by upgrade**, not waived | `overrides: {"uuid": "^11.1.1"}`, in the same change |

**Resolved-graph proof, read from the committed `docs/package-lock.json` rather than asserted:**

```
image-size@3.0.0-renvor.1 deduped -> ./vendor/image-size-disabled   (via @docusaurus/mdx-loader)
image-size@3.0.0-renvor.1 overridden -> ./vendor/image-size-disabled (root)
```

`node_modules/image-size` is a **symlink** to `../vendor/image-size-disabled`; no `image-size`
directory containing real parser code exists anywhere under `node_modules`, and no bundled copy
exists inside another package. `npm audit` reports **0 findings**, down from 21.

**Why a vendored package is inventoried at all.** It is not an external dependency, which is exactly
why it is easy to miss: it passes through no registry, no licence scanner, and no advisory feed. It
is **first-party build-time code that executes during `npm run build`**, and the only gates that see
it are this project's own — `cargo-deny` does not cover npm, and `npm audit` has nothing to report
about a local package. That asymmetry is recorded here rather than left implicit, and it is the
clearest ongoing cost of the removal strategy ADR-0009 chose.

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
| `syn` | 2.0.119, 3.0.3 | `tracing-attributes` pins 2.x; `serde_derive`, `thiserror-impl`, `tokio-macros` use 3.x |

`confique-macro` was named here as a second cause of the `syn` duplication. It left the graph with
the rest of the `confique` tree; `tracing-attributes` is now the sole reason 2.x is still resolved,
verified with `cargo tree -i syn@2.0.119`.

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
| `equivalent` | 1.0.2 | Apache-2.0 OR MIT | 1.6 | transitive | production |
| `fixedbitset` | 0.5.7 | MIT OR Apache-2.0 | 1.56 | transitive | production |
| `foldhash` | 0.1.5 | Zlib | 1.60 | transitive | production |
| `futures-core` | 0.3.34 | MIT OR Apache-2.0 | 1.36 | transitive | production |
| `futures-sink` | 0.3.34 | MIT OR Apache-2.0 | 1.36 | transitive | production |
| `getrandom` | 0.4.3 | MIT OR Apache-2.0 | 1.85 | direct | production |
| `hashbrown` | 0.15.5 | MIT OR Apache-2.0 | 1.65.0 | transitive | production |
| `hashbrown` | 0.17.1 | MIT OR Apache-2.0 | 1.85.0 | transitive | production |
| `indexmap` | 2.14.0 | Apache-2.0 OR MIT | 1.85 | transitive | production |
| `lazy_static` | 1.5.0 | MIT OR Apache-2.0 | (unstated) | transitive | production |
| `libc` | 0.2.189 | MIT OR Apache-2.0 | 1.65 | **direct** (was transitive; promoted at T143) | production |
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
| `toml` | 1.1.4+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | direct | production |
| `toml_datetime` | 1.1.1+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | transitive | production |
| `toml_parser` | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | 1.85 | transitive | production |
| `tracing` | 0.1.44 | MIT | 1.65.0 | direct | production |
| `tracing-attributes` | 0.1.31 | MIT | 1.65.0 | transitive | production |
| `tracing-core` | 0.1.36 | MIT | 1.65.0 | transitive | production |
| `tracing-subscriber` | 0.3.23 | MIT | 1.65.0 | direct | production |
| `unicode-ident` | 1.0.24 | (MIT OR Apache-2.0) AND Unicode-3.0 | 1.71 | transitive | production |
| `winnow` | 1.0.4 | MIT | 1.65.0 | transitive | production |
| `zeroize` | 1.9.0 | Apache-2.0 OR MIT | 1.85 | transitive | production |
`(unstated)` MSRV appears for exactly **two** packages: `lazy_static` and `matchers`. Both arrive
transitively through `tracing-subscriber`, and both are therefore **production**, not dev-only.
Neither states a `rust-version`; both compile on 1.94.0, verified by the workspace building and
testing on the pinned toolchain. An unstated MSRV is not a violation — it is an absence of a
promise, and the absence is recorded here rather than papered over with an inferred number.

> **Corrected 2026-08-16 (T122).** This paragraph previously named **three** packages, including
> `confique-macro`, and stated that *"all three are dev-only"*. `confique-macro` had already been
> deleted from the graph, and `lazy_static` and `matchers` were never dev-only — they reach a
> consumer through `tracing-subscriber`. The sentence understated the production surface by two
> packages, in the direction that makes an inventory look safer than it is.

## T033 — Did the direct-candidate evaluation predict the transitive graph?

**No, and it could not have.** Recording the answer honestly matters more than the answer flattering
the research.

| Question | Answer |
|---|---|
| Did research §3 evaluate every package that ships? | **No.** It evaluated **12**; **48** resolve, and **11** of the 12 are among them (`confique` was deleted when its proof gate failed) |
| Was research §3 *wrong* about anything it did evaluate? | **No.** Every direct candidate's version, licence, and MSRV matches the lockfile |
| Did any transitive package introduce a licence absent from the direct set? | **Yes — two.** `Zlib` (`foldhash`) and `Unicode-3.0` (`unicode-ident`) appear nowhere among the direct candidates |
| Did anything catch those two? | **Yes — `deny.toml`, not the research table.** Both licences were already on Phase 001's allow list |
| Did any transitive package fail FR-040's evidence requirement? | **No.** 0 packages lack a licence; 0 exceed the MSRV; 0 carry an advisory |

**The finding worth carrying forward**: the artifact that caught the two novel licences was the
*policy* (`deny.toml`), not the *design document* (research §3). A per-candidate evaluation table
scales with the number of packages a human chose; a policy check scales with the number that
actually resolve. Phase 002 needed both, and only one of them could have found `foldhash`.

**The phase is not failed by T033.** T033 requires failure if *any* resolved package lacks the
evidence FR-040 demands. Every one of the 48 has a declared licence, a resolvable version from the
committed lockfile, and a clean advisory check. The gate passes on evidence, not on absence of
looking.

## T034 — ADR-0003's lockfile policy and FR-040 do not conflict

Every figure above is read from a **committed** `Cargo.lock`, which is what FR-040 requires. ADR-0003
records that *reusable library crates* do not commit a lockfile. Both hold, because the two
statements govern **different objects** — the full reconciliation is in
[`specs/002-core-kernel/research.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md) §D12, and the short form
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
