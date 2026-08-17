# Phase 003 — Research and package-first evaluation

**Feature**: [`spec.md`](spec.md) · **Created**: 2026-08-17 · **Toolchain**: MSRV 1.94.0 + current stable

**Why this document is a gate, not a survey.** Constitution principle III requires that maintained
ecosystem packages be evaluated **before** any custom infrastructure is written, and FR-044/FR-045
require the evaluation to be recorded and any custom choice to be justified by an accepted decision
record. A phase that implements first and documents afterwards has not satisfied this.

**Every version, date, and licence below was read from the crates.io API on 2026-08-17.** None is
quoted from memory, and none is quoted from `PLAN.md` §8.1's 2026-08-11 snapshot without being
re-checked against the registry.

## Licence policy, applied

`deny.toml` allows: MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception, BSD-2-Clause, BSD-3-Clause,
ISC, Unicode-3.0, Zlib, CC0-1.0. **`exceptions = []`** — an exception is a reviewed decision, never a
widening of the list.

**Every candidate below resolves to a licence on that list.** `Unlicense` itself is not allowed;
`walkdir`, `globset`, and `ignore` each offer MIT in an `OR` expression, which is what cargo-deny
selects.

## Deltas against `PLAN.md` §8.1

The plan's snapshot was taken on 2026-08-11 and §8.1 requires re-checking before writing
requirements. Re-checked:

| Snapshot said | Registry says on 2026-08-17 | Action |
|---|---|---|
| clap 4.6.6 | **4.6.6** | unchanged |
| inquire 0.9.4 | **0.9.4** | unchanged |
| indicatif 0.18.6 | **0.18.6** | unchanged |
| MiniJinja 2.23.0 | **2.24.0** | **snapshot is one release behind**; plan to 2.24.0 |

## D1 — Command-line parsing

**Decision: `clap` 4.6.6**, derive API, `MIT OR Apache-2.0`, released 2026-08-06, MSRV 1.85,
215.5M downloads/month.

**Rationale.** FR-002 makes flag names, `--help` structure, and exit behaviour public contracts.
clap is the only candidate that generates help, validates values, and produces shell completions
from one declaration, so the contract has one source rather than three that drift. Its MSRV is
comfortably under ours. It is the plan's named candidate and is still current.

**Alternatives considered.** `argh` and `bpaf` are smaller but neither generates completions, and
`argh`'s help layout is not configurable enough to be a stable contract. `lexopt` and `pico-args`
are argument *lexers*, not parsers — adopting either means hand-writing validation and help, which
is custom infrastructure in exchange for a smaller dependency, the wrong trade for a public
contract.

## D2 — Interactive prompts

**Decision: `inquire` 0.9.4**, MIT, released 2026-02-24, MSRV 1.80, 5.6M downloads/month.

**Rationale, and it turns on one requirement.** FR-012 and SC-001 require cancellation to be
distinguishable from failure, because cancelling must exit `4` and leave nothing behind, while a
failure must exit `3` or `5`.

**Verified against the published API rather than assumed** — `inquire::error::InquireError` at
0.9.4 carries these variants:

| Variant | Meaning | What this phase needs it for |
|---|---|---|
| `OperationCanceled` | the operator pressed ESC | exit `4`, destination untouched |
| `OperationInterrupted` | the operator pressed Ctrl-C | exit `4`, destination untouched |
| **`NotTTY`** | **stdin is not a terminal** | **exit `2`/`3` naming the missing flags — FR-010, SC-012** |
| `IO`, `InvalidConfiguration`, `Custom` | everything else | exit `5` or `1` |

That is three separately typed outcomes across the two requirements that matter most here, and
`NotTTY` in particular means the non-terminal case is a **named condition** rather than something
this phase has to detect and infer. `dialoguer` surfaces an interrupt as a generic I/O error, which
would force this phase to infer intent from an error kind — exactly the inference constitution IV
calls a silent fallback.

**Alternatives considered.** `dialoguer` 0.12.0 (MIT, 2025-08-23, 15.7M/month) has wider adoption and
a longer history, and was rejected only on the cancellation signal above. `requestty` 0.6.3 (MIT,
2025-12-02) has **27.7k downloads/month**, roughly 0.2% of dialoguer's, which is too thin a
maintenance base for a component on the cancellation path.

**Maintenance note, recorded rather than glossed.** `inquire`'s last release is 2026-02-24 — about
six months old. That is not stale, but it is the least recently released of the three prompt
candidates, and it sits on a safety-relevant path. **Exit condition**: if `inquire` is unmaintained
at the Phase 004 dependency review, re-evaluate against `dialoguer` and absorb the cancellation
inference behind this phase's own boundary.

## D3 — Terminal output and progress

**Decision: `indicatif` 0.18.6** (MIT, 2026-07-01, MSRV 1.85) for progress, **`anstream` 1.0.0**
(`MIT OR Apache-2.0`) for stream-aware colour.

**Rationale.** FR-004 requires human output on `stderr` and results on `stdout`, and the edge cases
include a zero-width terminal, no terminal at all, and a `stdout` pipe that closes early. `indicatif`
degrades to no-op rendering when the stream is not a terminal, which is the behaviour FR-010 needs.
`anstream` strips styling when the destination is not a terminal rather than emitting escape codes
into a pipe.

**Alternatives considered.** `console` 0.16.4 is `indicatif`'s own dependency and would be adopted
transitively regardless; declaring it directly buys nothing here. `owo-colors` 4.3.0 styles strings
but does not decide whether styling is appropriate for the destination, which is the actual problem.

## D4 — Templating

**Decision: `MiniJinja` 2.24.0**, Apache-2.0, released 2026-08-12, MSRV 1.70, 9.7M downloads/month.

**Rationale.** FR-026 requires bounded expansion; FR-027 forbids a template reaching the filesystem
or the network; FR-028 requires an undefined variable to be an error rather than an empty rendering.
MiniJinja is the only candidate that offers all three as **configuration rather than convention**: it
exposes an explicit expansion limit, its function and filter set is allow-listed by the embedding
application rather than ambient, and it has a strict undefined-behaviour mode. Its MSRV is well
under ours and it is actively released.

**Correction found while wiring the manifest, and it sharpens the claim rather than weakening it.**
The bounded-expansion mechanism is the **`fuel` feature, and `fuel` is NOT in MiniJinja's `default`
feature set** — verified against the registry's feature map for 2.24.0, whose default is
`builtins, debug, deserialization, macros, multi_template, adjacent_loop_items, std_collections,
serde`. So a project that adopts MiniJinja the obvious way gets **no expansion bound at all**.

This is exactly the difference between a bound that exists and a bound that is believed to exist,
and it is one word in a manifest. Two consequences are now recorded rather than assumed:

1. `crates/renvor-cli/Cargo.toml` enables `fuel` explicitly, with a comment saying why.
2. `crates/renvor-cli/tests/bounds.rs` **asserts the bound behaviourally**, so the declaration
   cannot silently regress. A feature flag is a claim; the test is the evidence.

**Alternatives considered.** `Tera` 2.1.1 (MIT, 2026-08-11) is close in capability but ships built-in
functions the application must remember to remove rather than opt into — a deny-list posture where
constitution VI wants deny-by-default. `Handlebars` 6.4.4 renders an undefined variable as empty by
default, directly against FR-028. `askama` 0.16.0 compiles templates into the binary at build time,
which conflicts with FR-024's requirement that a template *version* be a runtime fact recorded in the
manifest. `upon` 0.11.0 (75.4k/month) is elegant and small but has no bounded-expansion control.

## D5 — Transactional generation: staging and atomic placement

**Decision: `tempfile` 3.27.0** (`MIT OR Apache-2.0`, 2026-03-11, MSRV 1.63, 165.9M/month) for the
staging directory, and **`std::fs::rename`** for the final move. **No crate is adopted for the
transaction itself, because none exists.**

**Rationale.** This is the safety core of the phase, so the honest finding matters more than a tidy
one: **the maintained ecosystem has no directory-level transactional placement primitive.** The two
candidates that sound like one are file-level:

| Candidate | What it actually does | Why it does not fit |
|---|---|---|
| `atomicwrites` 0.4.4 (MIT) | atomic replacement of **one file** | A generated project is a tree. Also **last released 2024-09-19, ~23 months ago** |
| `atomic-write-file` 0.3.1 (BSD-3-Clause, 2026-08-11) | atomic replacement of **one file**, actively maintained | Same shape mismatch |

So the transaction is composed here from two primitives that *are* maintained: a uniquely named
directory (`tempfile`) and a rename (`std`). Per the clarified decision, the staging directory is
created **inside the destination's parent**, which makes the rename same-filesystem by construction
and removes the cross-device case entirely rather than handling it.

**This composition is small and is not an ADR trigger on its own** — it is roughly "make a directory,
write into it, rename it" — but see **D6**, which is.

**Platform limit, stated rather than assumed.** On POSIX, `rename(2)` onto a non-existent path is
atomic. On Windows, `MoveFileEx` without `MOVEFILE_REPLACE_EXISTING` onto a non-existent path is the
closest equivalent, and **this phase does not claim it is atomic in the POSIX sense**. FR-013
guarantees the destination does not already exist, which is what makes the weaker Windows guarantee
sufficient. FR-016 requires the limit be documented per platform.

## D6 — Path containment and the destination boundary

**Decision: compose `std::fs::canonicalize` with explicit component validation, and DO NOT adopt
`cap-std`. This choice requires an accepted decision record before the component merges.**

**Rationale.** FR-039 and SC-009 require traversal, absolute-path injection, symlink escape, and
platform-reserved names to be refused before any write. `cap-std` 4.0.2 solves this at the strongest
possible level — capability-based `Dir` handles that make escape structurally impossible rather than
checked — and its licence (`Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT`) is acceptable.

It is nonetheless not adopted here, for one reason worth stating plainly: **`cap-std` requires every
filesystem operation in the generator to be expressed through its handles**, including those inside
the template renderer, and it brings a substantial transitive tree that must be inventoried under
FR-044. Adopting it partially would be worse than not adopting it, because a checked boundary beside
an unchecked one is an unchecked boundary.

**Choosing a checked boundary over a structural one is a deliberate weakening, and this document
does not pretend otherwise.** That is why it is recorded as requiring an ADR rather than decided
here.

> **GATE — ADR REQUIRED.** Under constitution principle III and FR-045, selecting hand-composed path
> containment in preference to the maintained capability-based package requires an **accepted**
> decision record naming `cap-std`, its concrete shortcomings for this use, the ownership cost of the
> alternative, and an exit strategy. **The path-validation component MUST NOT merge before that
> record is accepted.** This mirrors how ADR-0007 gated Phase 002's custom typed-state map.

**Supporting packages evaluated.** `normpath` 1.5.1 (`MIT OR Apache-2.0`, 2026-05-05) for
Windows-aware normalisation and `dunce` 1.0.5 for UNC de-verbatim are both plausible and both under
consideration for the ADR. `path-clean` 1.0.1 is **purely lexical** — it resolves `..` textually
without touching the filesystem, so it cannot detect a symlink escape at all — and its last release
was **2023-02-24, roughly 42 months ago**. It is rejected on both counts.

## D7 — Archives

**Decision: none. No archive is read in this phase.**

**Rationale.** The clarification session settled that every template is embedded. `tar` 0.4.46,
`zip` 8.6.0, and `flate2` 1.1.9 were all checked and all carry acceptable licences, but adopting any
of them would add a zip-slip and decompression-amplification surface to guard a capability nothing
uses. FR-040 therefore asserts the **absence** of the capability structurally, which is testable,
rather than hardening a code path that does not exist, which is not.

## D8 — Local certificate issuance

**Decision: none in this phase.** `rcgen` 0.14.9 (`MIT OR Apache-2.0`, 2026-08-10, MSRV 1.88) is the
right package when the time comes and is recorded here so the evaluation is not repeated.

**Rationale.** The clarification session settled that this phase ships the consent boundary and no
certificate. Adding `rcgen` now would put a certificate-generation capability in a binary that has
nothing to serve, which enlarges the trusted surface for no user-visible benefit.

## D9 — Secret redaction

**Decision: reuse `secrecy` 0.10.3**, already resolved through `renvor-config`, and own the **output
contract** here.

**Rationale.** Phase 002 established that `secrecy` provides access control and zeroization but
**supplies no `Display`**, so the redaction contract is Renvor's. FR-041 extends that obligation to
four output paths this phase introduces — human output, JSON output, the dry-run manifest, and error
messages — and a type that refuses to print itself does not automatically make a manifest safe.
No new dependency is warranted.

## D10 — Machine-readable output

**Decision: `serde_json` 1.0.151** (`MIT OR Apache-2.0`, 2026-07-20, MSRV 1.71).

**Rationale.** FR-022 requires exactly one JSON document on `stdout` for success and failure alike,
with an integer `schemaVersion`. `serde` is already a workspace dependency; `serde_json` is its
canonical companion and adds no new serialization model.

## D11 — Content identity in the file manifest

**Decision: `sha2` 0.11.0** (`MIT OR Apache-2.0`, 2026-03-25, MSRV 1.85).

**Rationale.** FR-031 and SC-016 require a manifest that identifies content reproducibly.
Constitution VI forbids creating a cryptographic primitive, so this is a package decision, not an
implementation one. SHA-256 is what the rest of this project already uses for artifact identity —
image digests, CSP hashes, release checksums — so a second algorithm would mean two vocabularies for
one idea.

**Alternative considered.** `blake3` 1.8.6 is faster and its licence is acceptable, but it **declares
no `rust-version`**, which makes MSRV compatibility an assumption rather than a fact, and speed is
not a constraint for hashing a few dozen small files.

## D12 — Terminal detection

**Decision: `std::io::IsTerminal`. No dependency.**

**Rationale.** FR-010 and SC-012 require the wizard to not prompt when `stdin` is not a terminal.
This was historically the job of `atty` or `is-terminal`; it is now in the standard library, far
below our 1.94.0 floor. **Verified by compiling and running it** rather than taken from
documentation: a program using `std::io::IsTerminal` builds and correctly reports `false` for a
redirected stdin. Adopting a crate for this would add a dependency to re-export a `std` trait.

Recorded because "no package needed" is a package-first **outcome**, and it belongs in the inventory
as a deliberate finding rather than as an omission somebody later mistakes for an oversight.

**This is the decision to prompt at all, and it composes with D2's `NotTTY` rather than duplicating
it**: `IsTerminal` decides whether the wizard is even entered, and `NotTTY` is the backstop if a
prompt is somehow reached anyway. Two independent checks, because FR-010 forbids both hanging and
silently defaulting, and a single check that regressed would produce one of those.

## D13 — Directory traversal for the manifest

**Decision: `walkdir` 2.5.0** (`Unlicense/MIT` → MIT selected, 133.7M/month).

**Rationale.** Building the manifest means walking the staged tree in a deterministic order.
`walkdir` provides sorted, symlink-aware traversal with explicit control over whether links are
followed — which matters here, because the manifest must describe what was created, not what a link
points at. Its last release is 2024-03-01, which reflects a finished API rather than an abandoned
one; it is one of the most depended-upon crates in the ecosystem.

## D14 — Command-line testing

**Decision: `trycmd` 1.2.1 and `snapbox` 1.2.2** for command contracts, **`insta` 1.48.0** for
structured snapshots. All `MIT OR Apache-2.0` except `insta` (Apache-2.0). All acceptable.

**Rationale.** FR-002 makes `--help`, exit codes, `stdout`/`stderr` split, and JSON shape public
contracts. `trycmd` tests exactly that surface as data files rather than as assertions buried in
Rust, which makes a contract change visible as a diff. `insta` covers the JSON documents, where a
structural diff is more useful than a string one.

**Alternatives considered.** `assert_cmd` 2.2.2 with `predicates` 3.1.4 is the conventional pairing
and remains available for cases needing imperative control; it is not the primary choice because a
contract expressed as assertions is harder to review than one expressed as expected output.

## Unresolved and carried forward

1. **The D6 decision record is a blocking gate**, not a documentation task. Nothing in the path
   containment component may merge before it is accepted.
2. **`inquire`'s release cadence** is on watch, with the exit condition stated in D2.
3. **Windows rename atomicity** is documented as weaker than POSIX rather than claimed equal, and the
   claim must be checked against what CI actually exercises before any platform is listed as
   supported.
4. **The complete resolved dependency inventory** must be produced from the real `Cargo.lock` after
   implementation, not from this document. This document evaluates candidates; only the lockfile
   records what was actually resolved, and FR-044/SC-015 are satisfied by the latter.
