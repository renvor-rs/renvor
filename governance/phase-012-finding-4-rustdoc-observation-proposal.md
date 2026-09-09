# Finding 4 — rustdoc as a separate observation category

**STATUS: APPROVED AND IMPLEMENTED, 2026-09-09. NOT MERGED.**

The maintainer approved option (d) and `record_version = 3` for implementation in pull request #72
on 2026-09-09, stating explicitly that this is **not merge approval and not blanket acceptance of
every detail in this document**. The design below is therefore the approved one; §"As implemented"
at the end records what shipped, what the implementation found that this document did not
anticipate, and the two version questions that were **derived from the contracts' own rules**
rather than decided here.

The text between here and that section is the proposal as it stood when it was approved, kept
unedited so the approval and the record refer to the same words.

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
| **C-5** `:227-229` | **the sentence that must change.** `verification reused cached artifacts for <checks>: no compiler launch observed` names "the checks whose units were all `Fresh`". Today a cached lib project's test check is `mixed` (1 launched, 1 fresh), so it is never named and the line is TRUE as shipped. Under this proposal `checks.test` becomes all-`Fresh` and would be named — while a rustdoc launch **was** observed. The line must exclude the doctest bucket explicitly, or it becomes false on the ordinary cached lib case |
| **C-5** `:229-231` | "every unit … accounted for" is **satisfied** by this design, but must say by what |
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


---

# As implemented (2026-09-09, pull request #72)

Every claim in this section is from the working tree and the test run, not from the design above.

## The defect, reproduced on the current head first

`a_library_bearing_project_records_its_doctest_unit_apart_from_the_compiler_units` was written
against `4cb4709` and failed there, with the defect verbatim:

```
ProjectVerificationFailed
  reason = compiler_identity_unreadable
  query  = rustc -vV
  cause  = the answer is not a `rustc` answer
  check  = cargo test -vv
```

## What shipped, beside the design above

**The `Fresh` measurement was reproduced on this host before the design was built.** cargo 1.94.0,
macOS/aarch64: a cold `cargo test -vv` on a lib+bin package launches three `rustc` units and one
`rustdoc` unit; a fully cached one prints `Fresh probe`, launches **zero** `rustc` units and still
launches **one** `rustdoc` unit. A library with **no doc comments at all** launches one too, so the
trigger is the library target, exactly as the design says.

**`--test` is not the discriminator, and the measurement says so.** Both the `rustc` test-harness
units and the `rustdoc` unit carry `--test`. Deciding by the flag would have moved two compiler
units into the doctest bucket and left the compiler unqueried. Recognition is by the chain's
**trailing executable** (`rustdoc`/`rustdoc.exe`), the discipline `clippy_driver` already uses, so a
wrapper in front of rustdoc does not hide it. `a_rustc_unit_carrying_test_is_still_a_compiler_unit`
pins this.

**A stated bound.** `build.rustdoc` pointed at an executable under another name is not recognised.
That unit stays in the compiler set and fails loudly at `compiler_identity_unreadable`, exactly as
before this change — the failure is loud, and nothing is attributed to a tool that was never asked.
Recognising it would mean guessing which executables are rustdoc.
`a_doctest_unit_under_an_unrecognised_name_stays_a_compiler_unit` is the negative control.

**A rule the design did not anticipate: a doctest launch does not discharge a `Compiling`
announcement.** `Compiling <own>` says a unit of the package was not reused, and Cargo has never
been observed to announce a package for the doctest unit alone (the cached run above announces
nothing and launches rustdoc anyway). Letting a rustdoc launch settle that debt would accept a
stream in which the library was announced and never compiled, so the announcement is still owed a
**compiler** launch. `an_announcement_answered_only_by_a_doctest_launch_is_still_unaccounted` pins
it, and it is now stated in C-5 and FR-012-7d (e).

**A defect the version bump would have introduced, found by a guard written before it.**
`record::render` wrote the generator's own `RECORD_VERSION` for any versioned record, which was
invisible while that constant was `2`. With `RECORD_VERSION = 3` it silently re-labelled a
**carried** version-2 record — one an operation that verifies nothing passes through — as version
3. In version 3's vocabulary an absent doctest table means no doctest unit was launched, so the
re-label would have asserted something a version-2 verification never established: fabricated
historical evidence. The renderer now writes the record's **own** version. Two tests caught it: the
guard `a_carried_version_2_record_is_not_silently_upgraded`, and the shipped FR-012-5a guarantee
`generate_resource_and_migration_leave_verified_with_byte_identical`.

**A surface the design did not list: the human `check` report.** `commands::check::describe`
enumerates the checks for an operator. It now prints a `doctest` row with the observed rustdoc —
**only when the record carries the table**, because a row of zeroes would report a unit that was
never scheduled as one that was reused.
`the_doctest_bucket_reaches_both_the_human_report_and_the_json` covers both directions.

**Version 2's strictness is enforced by serde, not by a hand-written list.** `ChecksVersion2` names
exactly the five checks version 2 defines, and the version-2 arm parses the document through it. A
hand-written guard would work today and rot the day someone adds a sixth check without thinking of
it; this struct refuses that field too, without being edited.

## The two version questions, derived rather than decided

The maintainer's instruction was to derive contract bumps from the contracts' own compatibility
rules and to stop and ask if a further product decision were needed. Neither of these needed one.

**`template_version` stays at 8.** C-4's own table distinguishes the axes: `template_version`
identifies the shape of the **tree** a generation rendered; `record_version` the shape of the
**record file**. Version 3 adds a table to a file the generator owns and changes no file a
generation renders. Conflating them would force every project rendered at template 8 to be
re-rendered for a change none of its files felt.

**`schemaVersion` stays at 2.** C-2 states that the fields of §"`result.toolchain` and
`result.verified_with`" are additive, and that the TOML record's `record_version` is *"a different
axis … not a `schemaVersion` change"*. `checks.doctest` is an added nullable field; a consumer
pinned to `2` reads every document this revision emits.

Contract **text** versions moved as minor revisions under each document's own policy: C-1 1.6.0,
C-2 1.1.0, C-4 1.4.0, C-5 1.3.0.

## The eight compatibility tests

All eight are implemented and green; 1–6 and the reader-direction control are in
`generate/record.rs`, 7 and 8 are end-to-end in `generate/verify.rs` against a real toolchain.

1. `a_version_2_record_carrying_a_doctest_table_is_refused_by_name` — **the test that makes the
   bump a measurement.** Had it passed, the table could have gone into version 2 in place.
2. `a_version_3_record_round_trips_with_every_doctest_field_preserved` — and the fixture's rustdoc
   release differs from its `rustc_*`, so a reader that filled one from the other fails here.
3. `a_version_2_record_without_the_table_still_reads_unchanged`.
4. `a_legacy_record_still_reads_as_legacy_under_the_version_3_reader`.
5. `the_supported_version_is_three_and_a_version_4_record_is_refused_by_name` — exit 3 through the
   approved unsupported-version path.
6. `a_version_3_record_for_a_binary_only_project_writes_no_doctest_table`.
7. `a_cached_library_bearing_project_still_launches_its_doctest_unit` — `observation = "cached"`,
   `rustc_*` absent, `rustdoc_*` present. **This is the measurement `units_fresh = 0` rests on**: if
   Cargo ever stops launching that unit for a cached package, this test fails and that zero is what
   has to change.
8. `a_rustdoc_identity_differing_from_the_compilers_is_recorded_not_refused` — a `RUSTC` shim
   answering another release while rustdoc stays the toolchain's own. Recorded in two fields, not
   refused. Under option (a) this ordinary run would have been refused as a disagreement.

Plus the reader-direction control `a_reader_whose_newest_version_is_2_refuses_a_version_3_record`,
which reconstructs the pre-`4cb4709` dispatch and shows the refusal rather than describing it, with
positive controls that the same reader still reads version 2 and legacy records.

## One number in the proposal above does not add up, and is not repeated

The design section says the doctest unit never reaching `Fresh` was **"measured ten times: five on
macOS, and on Linux three runs each on 1.94.0 and 1.98.1"**. Five plus three plus three is eleven.
The proposal text is left as the maintainer approved it, but no code comment, contract or
requirement written for this implementation repeats the count. They name the platforms instead —
*macOS/aarch64 under cargo 1.94.0, and Linux/aarch64 under 1.94.0 and 1.98.1* — which is what the
retained evidence supports, and the behaviour was reproduced again on macOS/aarch64 on 2026-09-09
before the design was built.

## Untested boundaries, stated

- **A Cargo that reports a doctest unit `Fresh`** has never been observed, so
  `checks.doctest.units_fresh` has only ever been `0` in a real run. If that changed, a doctest
  bucket with `units_launched = 0` and `units_fresh = 0` would be indistinguishable from "no
  library target", because the table is absent in both. Test 7 is the tripwire.
- **A doctest unit launched by a check other than `cargo test`.** The implementation aggregates
  doctest chains from all three `-vv` checks so such a unit would be recorded rather than dropped,
  but only `cargo test` has ever been observed to launch one, so the other two paths are
  unexercised by any measurement.
- **`Path::file_name` on a foreign separator.** A `C:\…\rustdoc.exe` chain is split correctly on
  Windows and not on Unix, and the reverse; the parser only ever reads a stream produced on its own
  host, so this is a property of `Path` rather than a gap here. The portable half — the `.exe` name
  and the Windows `set NAME=value&&` environment shape — is asserted on every platform.
- **A wrapper in front of rustdoc** is recognised and counted toward `wrapper_observed`, but no
  measurement of a real rustdoc wrapper exists; cargo has no `RUSTDOC_WRAPPER`.
