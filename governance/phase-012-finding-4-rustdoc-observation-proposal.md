# Finding 4 — rustdoc as a separate observation category

**STATUS: PROPOSAL. NOT APPROVED, NOT IMPLEMENTED, NOT ACTIVATED.**

No contract, schema, requirement, notice or code in this repository has been changed by this
document. It exists so the maintainer can approve or reject one design before any of that happens.
The maintainer's disposition of 2026-09-09 named option (d) the preferred **direction**, explicitly
not implementation approval.

## The defect this would close

A `cargo test -vv` stream for a project with a `src/lib.rs` ends its doctest unit's chain in
**rustdoc**, not rustc. `toolchain::grammar::parse_rustc_vv` parses `-vV` output under the name
`rustc`, so the queried identity of that unit is refused and the whole generation fails with
`compiler_identity_unreadable` / `cause = "the answer is not a rustc answer"` at
`cargo test -vv`. Reproduced end to end on macOS/aarch64, Linux/aarch64 on 1.94.0, and
Linux/aarch64 on 1.98.1 (CI's stable by release and commit).

The trigger is only that the project has a library target. No doc comments are required. Every
shipped starter is binary-only, so no gate observes it.

## The design

Treat rustdoc exactly as clippy is already treated: **its own check bucket, its own queried
identity, and outside the `observation`**.

### Fields

A fourth check table, beside `checks.clippy`, `checks.build` and `checks.test`:

```toml
[verified_with.checks.doctest]
outcome = "passed"
units_launched = 1          # Cargo `Running` lines for the project's own doctest units
units_fresh = 0             # units Cargo positively reported `Fresh`
rustdoc_release = "1.98.1"  # the observed rustdoc EXECUTABLE, from its own `-vV` query;
rustdoc_commit = "48a229cea" #   ABSENT when every doctest unit was `Fresh`
```

`rustdoc_release`/`rustdoc_commit` sit exactly where `driver_release`/`driver_commit` sit for
clippy, with the same ABSENT rule.

### Identity query

`rustdoc -vV`, parsed by `parse_vv(text, "rustdoc")` — the existing generic parser under a second
tool name, not a second parser. Run once, under the same seal and the same FR-012-7a safeguards as
every other identity query.

### Observation rules

`observation` continues to summarise **the build and test checks only**. This is not a new concept:
`evidence::observation(build, test)` already takes only those two and deliberately excludes clippy.
The doctest bucket is excluded the same way, so:

- `rustc_release`/`rustc_commit`/`rustc_host` are filled from launched **build/test** units only;
- rustdoc never enters `distinct`, so it cannot produce a `Disagreement`.

### The four cases

| case | `observation` | `rustc_*` | `checks.doctest` | `rustdoc_*` |
|---|---|---|---|---|
| **build/test only** (binary-only project — every current starter) | as today | as today | **table absent entirely** | absent |
| **doctest-bearing, cold** | `launched` | present | launched 1, fresh 0 | present |
| **mixed** | `mixed` | launched units' identity | launched 1, fresh 0 | present |
| **cached** (all build/test units `Fresh`) | `cached` | **absent** | launched 1, fresh 0 | **present** |

The last row is the one that needs a maintainer's eye. **The doctest unit never goes `Fresh`** —
measured ten times: five on macOS, and on Linux three runs each on 1.94.0 and 1.98.1, where a fully
cached lib project prints `Fresh probe v0.1.0`, launches **zero** rustc and still launches **one**
rustdoc. So a cached run legitimately records `observation = "cached"` with `rustc_*` absent and
`rustdoc_*` present, and the clippy analogy transfers in rule shape but **not in trigger**: clippy's
ABSENT case is "every unit `Fresh`", a state the doctest unit has never been observed to reach.

### RUSTC / rustdoc disagreement, without false attribution

`RUSTC` redirects **rustc only**; rustdoc keeps the toolchain's own. Measured on both platforms:
with `cargo` on 1.94.0 and `RUSTC` pointing at 1.98.1's rustc, one run launches
`…/1.94.0/bin/rustdoc` and `…/1.98.1/bin/rustc`.

The rule is therefore:

- rustdoc's identity is recorded **only** in `rustdoc_*`, and rustc's **only** in `rustc_*`.
  Neither is ever written into the other's field, and neither is inferred from the other.
- A difference between them is **recorded, not refused**. It is the truthful result of a
  legitimate operator override, not a contradiction.
- `Disagreement` keeps its current meaning and is narrowed to say so: two launched **build/test**
  units whose queried identities disagree.

Under option (a) — parsing rustdoc under its own name while leaving it inside `distinct` — this
same measured case produces a `Disagreement` and refuses an ordinary run. That is why (a) is
behaviourally incomplete, and it is measured, not argued.

## Every affected contract, row, notice and requirement

| where | change |
|---|---|
| **C-1** `contracts/command-surface.md:373` | the `compiler_identity_unreadable` probe list gains `rustdoc -vV` |
| **C-1** `:374` | `evidence_capture_failed` — "two launched units whose queried identities disagree" narrows to **build/test** units |
| **C-1** `details.supported` | the highest record version this generator reads changes from `2` |
| **C-5** `contracts/generation-transaction.md:217-218` | "the last binary of the build/test chain" must carve out the doctest chain |
| **C-5** `:223-229` | must state what the doctest bucket reports when the build/test checks are cached |
| **C-5** `:228` | **the sentence that must change.** `verification reused cached artifacts for <checks>: no compiler launch observed` names "the checks whose units were all `Fresh`". Today a cached lib project's test check is `mixed` (1 launched, 1 fresh), so it is never named and the line is TRUE as shipped. Under this proposal `checks.test` becomes all-`Fresh` and would be named — while a rustdoc launch **was** observed. The line must exclude the doctest bucket explicitly, or it becomes false on the ordinary cached lib case |
| **C-5** `:229-230` | "every unit … accounted for" is **satisfied** by this design, but must say by what |
| **C-4** `contracts/template-contract.md:220` | "one table per check that can launch a compiler **(clippy, build, test)**" — the enumeration gains `doctest` |
| **C-4** `:202-226` | gains `rustdoc_release`/`rustdoc_commit` with the ABSENT rule beside `driver_*` |
| **C-4** `:162` | `record_version` — see below |
| **`json-output.md`** `:223` | `observation` — restate that it covers build and test units |
| **`json-output.md`** `:224` | the `rustc_*` null rule |
| **`json-output.md`** `:233` | a new `checks.doctest` row beside `checks.clippy` |
| **`json-output.md`** `:234` | `checks.build`/`checks.test` |
| **FR-012-7d** | clauses **(a)**, **(d)**, **(e)**, **(f)**, **(h)** |
| **FR-012-7e** `:292` | the probe-line enumeration gains `rustdoc -vV` |

## Reader/writer compatibility, and the version

**`record_version` MUST become 3. This is a measured incompatibility, not a judgement call.**

`generate/record.rs:32` states that version **2 is validated strictly — `deny_unknown_fields`
everywhere inside** — and the reader dispatches on `record_version` (C-4:162). So:

- **old reader, new record written as v2** → the unknown `[verified_with.checks.doctest]` table is
  **REFUSED** by serde. A project generated by a newer generator would fail `renvor check` under
  the current one. This is the incompatible direction and it is real.
- **new reader, old v2 record** → compatible; the table is simply absent, exactly as it is for a
  binary-only project.
- **new reader, legacy record** (no `record_version`) → unchanged; still read and reported as
  legacy.

**Do not read "nothing has been published yet" as permission to change version-2 semantics in
place.** The constraint here is not publication, it is the strict reader already in the tree and
already shipped in generated projects: a v2 record with a fourth check table is refused by it. The
recommendation is therefore explicit — **introduce `record_version = 3`**, keep the version-2
reader intact and passing, and raise `details.supported` to `3`.

### Concrete compatibility tests to require

1. A v2 record carrying a `[verified_with.checks.doctest]` table is **refused** by the version-2
   reader, by name — this is the test that proves the bump is necessary rather than assumed.
2. A v3 record round-trips: written, then read, with every field preserved.
3. A v2 record without the table still reads under the new reader, unchanged.
4. A legacy record (no `record_version`) still reads as legacy, unchanged.
5. `details.supported` reports `3`, and a v4 record is refused by name with
   `Code::RecordUnsupported`.
6. A binary-only project under v3 writes **no** `checks.doctest` table — the table is optional
   within the version, so its absence must not be read as a defect.
7. A cached lib-bearing project records `observation = "cached"` with `rustc_*` absent and
   `rustdoc_*` present.
8. A lib-bearing project under `RUSTC` records differing `rustc_*` and `rustdoc_*` and does **not**
   report `Disagreement`.

## What is measured and what is reasoned

**Measured**: the failure and its message on three platform/compiler combinations; the doctest
unit never reaching `Fresh` (ten runs); `RUSTC` not redirecting rustdoc; the strict version-2
reader; every contract line cited above, read in the tree at PR #72's head.

**Reasoned, not measured**: the whole of this design. No part of it has been implemented, and the
census impact (`verification-sequence.md` 2.3.0 counts 86 (row, test) pairs, 19 of them starter
rows) is expected to be nil but has not been recomputed.
