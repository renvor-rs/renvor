# Phase 012 — Evidence

**Phase**: 012 — REST documentation and production examples; the L-1 (TLS) and L-2 (toolchain)
carry-overs from Phase 011
**Companion to**: [`phase-012-specification-and-decision-brief.md`](phase-012-specification-and-decision-brief.md)
(the specification; every `FR-012-n`, `SR-012-n`, `AC-012-n`, `U-n`, and `A-n` below is defined
there) · [`phase-012-task-plan.md`](phase-012-task-plan.md) (the requirements → tasks →
acceptance → evidence matrix; every implementation batch appends here) ·
[`phase-012-security-carryover.md`](phase-012-security-carryover.md) ·
[`phase-011-limitations.md`](phase-011-limitations.md) (L-1 and L-2, the rows this phase works
against — both **open**)
**Drafted**: 2026-09-07, as a skeleton, by the B1 documentation task (T-012-02), on branch
`feat/phase-012-b1-toolchain`, stacked on `docs/phase-012-decision-brief` (pull request #65,
unmerged) — the planning branch's head at the time of drafting is `6c0d780`; `origin/main` is
`7281e4f91aeb56695d6eceb322065e5f5fca04ef` (the squash of pull request #64)
**Revised**: 2026-09-08, when B1 reached its review checkpoint with every required check green on
`3369e09`; §1's fact table, §1.1, and §1.2 are filled from that run.
**Status**: **B1 at its review checkpoint — not merged.** Nothing here is closed. No limitation is
closed, no decision record is accepted (ADR-0038 is `proposed`), no waiver is created or granted,
nothing is tagged, published, or deployed. A row is filled by the batch that earns it, with the
head, the run identifiers, and the compiler identities of the legs that proved it — and a row
whose measurement exists but whose transport does not says **not captured**, and names what would
capture it, rather than claiming a value.
**Convention**: a head is a full commit SHA with its tree; a run is a GitHub Actions run
identifier with the job and leg named; a compiler identity is the leg's `rustc -vV` release,
commit, and host and its `cargo -vV` release and commit — three fields and two, never one string.
Evidence is described as what it is: a **launch observation** (Cargo's `Running` line), a
**queried identity** (a binary's own `-vV` or `--version` answer), or a retained measurement — never
as proof of execution through a wrapper.

## 1. L-2 — B1 (T-012-01 … T-012-12): the toolchain activation unit

| Item | Value |
|---|---|
| Branch | `feat/phase-012-b1-toolchain`, stacked on `docs/phase-012-decision-brief` (#65) |
| Head at the review checkpoint | `3369e0981f3ad5cf555ee289b7761708d993765d` (tree `65829420dd50f0c4c36345b9139cdf4357e08320`), pull request #72 |
| CI run identifiers | `34184656201` (`ci`), `34184656203`, `34184656226`, `34184656250` (`docs`, `security`, `release-dry-run`) — **all green**: `verify (1.94.0)`, `verify (stable)`, the four `platform (…)` legs, `security`, `docs`, `dependency-review`, and `package and verify without publishing` |
| Compiler identity, MSRV leg (`verify (1.94.0)`) | `rustc 1.94.0` commit `4a4ef493e3a1488c6e321570238084b38948f6db` host `x86_64-unknown-linux-gnu`; `cargo 1.94.0` commit `85eff7c80277b57f78b11e28d14154ab12fcf643` |
| Compiler identity, stable leg (`verify (stable)`) | `rustc 1.98.1` commit `48a229ceaefd4985c50990b14116b6d856af0985` host `x86_64-unknown-linux-gnu`; `cargo 1.98.1` commit `797e8a9bca276c1c9f9f738d2a20f484fa4eea9d` — the release CI resolved for `stable` on 2026-09-08 |
| Control toolchain per leg (U-10) | the MSRV leg's control is `stable`; the stable leg's control is `1.94.0`; the `Control toolchain identity` step proves the two differ in release **and** commit — run identifiers and the two identities: on the stable leg, leg `rustc 1.98.1 (48a229cea…)` against control `rustc 1.94.0 (4a4ef493e3a…)`, the control carrying `rustfmt 1.8.0-stable` and `clippy 0.1.94 (4a4ef493e3 2026-03-02)`, and the checkout still resolving the leg after the second install; the MSRV leg is the mirror image. Run `34184656201` |
| What the batch delivers | the contract revisions C-4 1.3.0, C-5 1.2.0, C-1 1.5.0, C-2 (additive), `support-policy.md` 1.2.0 (T-012-02); ADR-0038 `proposed` (T-012-01); templates at version 8 with the `toolchain` group (T-012-03); the framework-checkout reads (T-012-04); the version-2 record with `[toolchain]` and `[verified_with]` and the reader dispatch (T-012-05); the seal that forces `RUSTUP_AUTO_INSTALL=0` and drops the install-server pair (T-012-06); the identify-before-invoking preflight (T-012-07); the evidence mechanism as amended (T-012-08); the two notices (T-012-09); selection across staging and placement (T-012-10); legacy trees (T-012-11); no provisioning, offline preserved (T-012-12) |
| What the batch does **not** deliver | `doctor`'s toolchain section, xtask step 1's identity line, the census assertion, the dated `SUPPORT.md`/`rust-toolchain.toml` sentences, and the limitations ledger — all **B1f** (T-012-13 … T-012-17); `verification-sequence.md` 2.4.0 moves with B1f, because its step-1 text describes an xtask change B1 does not make |
| Acceptance tests, RED then GREEN | the tests named in `phase-012-task-plan.md` §1.1 for every requirement above; the refusal envelopes are asserted in place by the tests that produce them rather than retained as files — every refusal this batch adds is checked for its code, its `details.reason`, and the absence of anything staged, in the test that provokes it; the `SKIPPED:` lines observed locally for the two-toolchain controls and their required runs on both CI legs under `RENVOR_TEST_REQUIRE_TOOLCHAINS=1` |
| Contract numbers confirmed against the brief's §8 | the revision texts are diffed against the brief's §8 in the pull request body of #72; the old-reader/new-record item is **still the maintainer's**: C-4 **1.3.0** is written on the recommendation, and **2.0.0** is the alternative if source-built readers are inside the compatibility promise |
| Out-of-repo evidence cited | §2 below; `/Users/ahmedanbar/Documents/renvor/renvor-t-012-08m-evidence/2026-09-07-eedd9ed/` |

### 1.1 `clippy-driver` identity query

The observed `clippy-driver` **executable** — the one Cargo's `Running` line names — is queried
itself with its supported version query, `clippy-driver --version`, under the seal and after
FR-012-7a's identification safeguards (brief §4.3.2, the clippy correction; FR-012-7d (b)). The
query has to parse under FR-012-7e's grammar (`clippy <release> (<hex> <date>)`) on every
supported toolchain, or the observed driver cannot be identified.

**Measurement, recorded verbatim as stated by the B1 orchestrating session (2026-09-07):**
`clippy-driver --version` was run on the installed toolchains **1.90.0, 1.94.0, 1.95.0, and
1.97.1**, in an isolated directory, and answered under the grammar with exit 0; on **1.94.0** the
answer is

```text
clippy 0.1.94 (4a4ef493e3 2026-03-02)
```

which parses to `driver_release = "0.1.94"`, `driver_commit = "4a4ef493e3"`.

What is and is not established by this:

- The retained T-012-08m directory does **not** carry the raw output of this query. It carries
  the **launcher's** answer — `cargo clippy --version` via the pin — in
  `validator-extension/remeasure.out` (line 34: `clippy 0.1.94 (4a4ef493e3 2026-03-02)`), the same
  string on 1.94.0. That the two agree on a real toolchain is exactly why control (xiv) of
  FR-012-7d uses a stub whose launcher and driver answers differ: the record must be shown to
  carry the driver's.
- The answers on 1.90.0, 1.95.0, and 1.97.1 are not quoted here because they were not handed
  over as strings; they are retained by T-012-08's per-leg test
  `clippy_driver_version_answers_under_the_grammar_on_this_toolchain`, whose output on both CI
  legs is **not separately captured**, and the reason is worth stating rather than leaving as a
  gap: the gate runs `cargo test`, and `libtest` prints a passing test's stdout to nobody. What
  the green legs do establish is narrower and real — every starter row on both legs placed a
  record carrying `driver_release` and `driver_commit`, and a driver answer outside FR-012-7e's
  grammar is a refusal (`compiler_identity_unreadable`), not a blank field. So the query parsed
  under the grammar on `1.94.0` and on `1.98.1`; the strings themselves are quoted above only for
  the four toolchains measured by hand. Capturing them per leg costs one `--nocapture` run of that
  test in the gate, which is an `xtask` change and therefore **B1f**.
- Nothing about a `clippy-driver` built from another channel, or about a toolchain outside the
  four, is claimed.

### 1.2 Actual-generator cache-path measurement (FR-012-7d (j))

Carried into implementation by the maintainer's U-2 amendment, not another feasibility round:
whether `renvor new`'s staging and `generate auth`'s scratch copy (`std::fs::copy`, which
**preserves the source mtime on macOS** — measured, §2) verify **all-`Fresh`** against a shared
absolute `CARGO_TARGET_DIR`, on each supported platform leg. The result decides nothing about
correctness — a cached run is a supported, explicitly recorded outcome — but it is the fact the
cached-representation rule was written for, and it must be measured rather than assumed.

| Platform leg | `renvor new` staging against a shared target: `observation` | `generate auth` scratch copy against a shared target: `observation` | Run identifier | Notes |
|---|---|---|---|---|
| local: `macos`, 1.94.0 | **`launched`**, `units_fresh = 0` for clippy, build, and test | not measured — needs a framework checkout and a full starter build | — (local, 2026-09-07) | `fs::copy` preserves mtime here (§2) |
| `ubuntu-latest`, 1.94.0 | not captured | not captured | `34184656201` | |
| `ubuntu-latest`, stable | not captured | not captured | `34184656201` | |
| `macos-latest`, 1.94.0 | not captured | not captured | `34184656201` | |
| `macos-latest`, stable | not captured | not captured | `34184656201` | |
| `windows-latest`, 1.94.0 | not captured | not captured | `34184656201` | |
| `windows-latest`, stable | not captured | not captured | `34184656201` | |

**Local measurement (macOS 14, `aarch64-apple-darwin`, rustc 1.94.0, 2026-09-07).** Two
consecutive `renvor new` runs against one absolute `CARGO_TARGET_DIR` both recorded
`observation = "launched"`, with `units_fresh = 0` for clippy, build, and test in each. The
mechanism is the staging name: C-5 stages under `<destination parent>/.renvor-staging-<pid>-<nanos>-<counter>`,
a different absolute path on every run, and Cargo's fingerprints are path-sensitive, so the
second run's units are not the first run's units and nothing is `Fresh`. On this platform, by
this path, the generator does not reach the cached case — which is a fact about the staging
naming, **not** a claim that the cached case cannot arise (a caller who arranges an identical
staging path, or a future stable staging name, would reach it, and the record represents it).

The per-leg cells above are filled from `tests/generated.rs`'s
`a_second_generation_against_a_shared_build_directory_records_its_observation`, which asserts only
the invariant that an observation and an identity agree — a launch means an identity was queried,
no launch means the identity is absent rather than filled in — and prints the measurement with the
prefix `MEASUREMENT renvor-new-shared-target:` so each platform leg's log carries the answer.
`generate auth`'s scratch copy is not measured locally: it needs a framework checkout and a full
starter build, and the local disk budget did not allow one. Its cells are filled from the CI legs.

**Why the per-leg cells say *not captured* rather than a value.** The test that measures this,
`tests/generated.rs::a_second_generation_against_a_shared_build_directory_records_its_observation`,
runs and passes on every leg — it asserts the invariant that an observation and an identity agree —
but it reports the measurement by printing it, and `libtest` shows a passing test's stdout to
nobody. The gate runs plain `cargo test`. Two changes would fill these cells, both **B1f**: a
`--nocapture` run of that one test in `xtask`'s step 4, or AC-012-5's census assertion, which reads
`observation` out of every placed record on every leg. Recording the cells as *not captured*, with
the mechanism named, is the honest state: the measurement exists, the transport does not.

Each cell is `launched`, `cached`, or `mixed`, read from the placed record's `[verified_with]`,
with the `units_launched`/`units_fresh` counts of the build and test checks beside it when the
row is filled. The controlled, non-cached tests of T-012-08 (a private, empty target per control)
are a separate matter and are **not** what this table records.

## 2. T-012-08m — the evidence-mechanism feasibility round (complete 2026-09-07; cited, not copied)

T-012-08m measured four candidate evidence mechanisms against FR-012-7d's requirements and
controls, was validated in three rounds by a separate agent, and was extended once under the
maintainer's authorisation (the clippy check on a binary-only project with `CARGO_INCREMENTAL=0`).
Its evidence is retained **out of repo** at

```text
/Users/ahmedanbar/Documents/renvor/renvor-t-012-08m-evidence/2026-09-07-eedd9ed/
```

and is cited from here, from the brief (§4.3.1), from the task plan (§1.1 FR-012-7d), and from
ADR-0038 §Evidence. It is not copied into the tree: it is measurement data from one machine (Darwin
25.3.0 arm64, cargo 1.94.0, APFS), retained as research evidence, and the artifact analysis in it
is explicitly **not** a production mechanism (U-2: no artifact scanning in production, no
mandatory witness).

| File | What it holds |
|---|---|
| `00-toolchains.txt` | the bare-binary identities of 1.94.0, 1.95.0, and 1.97.1 (`rustc -vV`, `cargo -vV`); rustup 1.29.0's `--version` under FR-012-7a step (2)'s isolation (`info: no `rustc` is currently active`; the isolated `RUSTUP_HOME` received only `settings.toml`) |
| `01-runs.jsonl`, `03-phase2.jsonl`, `runs/`, `obs/` | the raw runs and observations of the four candidates |
| `02-summary.json` | the sentinels (`probe/keep`, `foreign/marker`) unchanged before and after |
| `07-comparison.md` (and `07-comparison-v1-superseded.md`) | the comparison of the candidates, with a dated erratum on the `std::fs::copy` sentence |
| `08-validation-round-1.md`, `08-validation-round-2.md` | the separate agent's validation of the comparison |
| `09-clippy-identity.txt` | the clippy launch chains and producer markers under ordinary selection and under `RUSTC` = 1.95.0: the chain's trailing `rustc` names the override while the driver's artifacts carry 1.94.0's marker |
| `10-clippy-bin-only-incremental-off.{txt,jsonl}`, `11-extension-clippy-witness.md`, `12-validation-extension.md`, `validator-extension/` | the extension: with incremental off, the clippy check on a binary-only crate writes only 0-byte `.rmeta` and `.d` files — no eligible marker — so the mandatory per-check witness **refuses a supported configuration**; validated **CONFIRMED** |
| `13-fscopy-mtime.txt`, `validator-extension/fscopy-probe` | `std::fs::copy` preserves the source mtime on macOS on both code paths (measured 2026-09-07T11:28:09Z) |
| `scripts/` | the measurement scripts |

What the round decided, and what it did not: it led to the maintainer's U-2 policy amendment
(brief §4.3.1) — launch observation plus queried identity from the actual checks, cached checks
represented explicitly, no mandatory artifact witness — and to A-8 and the clippy correction
(§4.3.2). It implemented nothing, and the actual generator's cache paths (§1.2) were deliberately
left to T-012-08 rather than to another feasibility round.

## 3. The B1 correction round (2026-09-08)

Six source-review findings, verified against the tree before anything was changed. Each entry says
what was **reproduced** (a failing test written first, and watched fail against the code as it
stood), what was fixed, and — where the reviewer's diagnosis and the measurement disagreed — which
one the evidence supports. Two of the six are not what the review said they were, and both are
recorded that way rather than quietly re-scoped.

| # | Finding | Reproduced? | Disposition |
|---|---|---|---|
| 1 | `resolve.rs` invokes `PATH` `rustfmt` with no identification | **Yes**, and worse than reported | `identify()` covered `rustc` and `cargo` only. A mixed layout — bare `rustc`/`cargo`, `rustfmt` a proxy of a rustup nothing can locate — reaches `rustfmt --version` **inside the pinned directory** with the floor unchecked. `generate resource` was worse still: it formats in the project directory and called `identify()` **not at all**. Fixed by giving `Tool` a `Rustfmt` variant, probing it in `identify()`, and identifying once per `generate` run before the first tool child. Control: `a_rustfmt_proxy_beside_bare_tools_is_identified_before_it_runs_in_a_pinned_directory`, whose stub installs whenever it runs where a toolchain file is |
| 2 | `without_launch_lines` removes only the first physical line | **Yes** | It dropped one physical line where `parse_check` rejoins a whole logical command, so a launch command split by a newline in a value — or by a backtick inside a quoted one — left its continuation in a failure message. Fixed by reading the stream with the same `rejoin`; an unterminated command is dropped whole. Controls: `no_continuation_of_a_launch_command_reaches_a_failure_message` (three canaries: newline, embedded backtick, truncation) and, end to end, `a_failing_checks_message_and_json_carry_no_part_of_a_multiline_launch_command`, which asserts over the human message **and** the serialised JSON envelope |
| 3 | The capture threads are joined without a deadline | **Yes**, measured | A child that exits leaving a descendant on the pipe made the "bounded" run take **3.01 s against a 300 ms deadline** and return `Ok` — the deadline bounded the exit and not the collection. Fixed by a channel and `recv_timeout` against one deadline over both phases, in `isolate::run_bounded` and `evidence::answer`. Controls: `a_child_that_exits_leaving_a_descendant_on_the_pipe_is_bounded_too` and `a_query_whose_descendant_holds_the_pipe_is_bounded_by_the_deadline`, each with a short-lived descendant of the test's own |
| 4 | `found.toolchain.is_some()` reads "declares a pin" | **Yes**, at `auth` | The first legacy `auth` writes `[toolchain]` `none` twice, so the predicate flips and the **next** `auth` plans the pin group into a tree that asked for none. `generate resource` was never affected — it plans no pin file — so the reproduction is at `plan_auth`, not at the surface first tried. Fixed by `Toolchain::declares()`, one predicate shared with `auth_expectations`. Controls: `a_second_auth_on_a_verified_legacy_tree_plans_no_pin_and_no_rust_version` (fails under the old predicate) and, live, C-sel-3 below |
| 5 | The result is built after the transaction commits | **Yes**, measured | `provenance_json` walks the tree scope and can fail; run after `commit` returned, its failure is a reported failure with every file already rewritten. Fixed by `apply::commit_with`, which runs the caller's result construction with the record's previous bytes remembered, so a failure rolls back through the path a failed rename takes; `commit` is now `#[cfg(test)]`, so no shipped caller can end the transaction early. Control: `generate-result` added to `a_failure_at_every_placement_boundary_leaves_the_project_byte_identical`, which compares the whole project byte for byte — with the boundary at the old position it fails with *"a failure at `generate-result` left the project changed"* |
| 6 | `parse_check` clears its announced flag after the first own launch | **Partly — the conclusion holds, the mechanism does not** | See §3.1 |

### 3.1 Finding 6: the accounting C-5 promises is not available from Cargo's output

The reported mechanism is **not** the cause. The flag is a latch: an announcement arms it and a
launch discharges it, Cargo emits one announcement per package per check, and never clearing it
would fail every ordinary stream. Removing the reset changes nothing.

The conclusion is right for a different and larger reason. Measured 2026-09-08 on cargo 1.97.1, for
`build`, `test`, and `clippy --all-targets` alike, on a package with a lib and a bin:

| State | What Cargo prints for the own package |
|---|---|
| every unit dirty | `Compiling`/`Checking`, one `Running` per unit, **no** `Fresh` |
| every unit reused | one `Fresh`, **no** announcement — and, for `test` on a crate with a doctest, a `Running` line for the rustdoc unit |
| **one unit dirty, one reused** | `Compiling`/`Checking`, **one** `Running` — and **nothing at all** for the reused unit |

So: `Fresh` is all-or-nothing per package and never shares a stream with that package's
**announcement**; no line anywhere carries a unit count or a total. Two consequences —

1. C-5 1.2.0's sentence *"every unit of the project's own package(s) must be accounted for by a
   `Running` line or a positive `Fresh` report"* **describes a state Cargo does not report**. The
   ordinary partially-fresh run is already outside it.
2. Removing one of several own `Running` lines yields a stream that is, line for line, the shape of
   a legitimate partial rebuild. No parser of this output can tell them apart.

**The contract text is not narrowed here.** What was done instead: the strongest accounting the
evidence *does* support was established — an announced package must launch, a package that appears
nowhere is refused, a truncated or malformed stream is refused, and (new) a stream reporting the own
package **both** `Fresh` and **announced** is refused as `EvidenceError::Contradictory` rather than
recorded as `mixed`, a shape Cargo never emits. The control
`one_of_several_own_launches_may_go_missing_and_this_names_the_limit` asserts the limit and the four
refusals together, so a later reading of C-5 finds the measurement rather than an assumption.

**A correction to this section, from the independent validation.** The refusal was first keyed to
the **launch count** — `units_launched > 0 && units_fresh > 0` — and this record first claimed Cargo
never prints `Fresh` beside a launch. That is **false**, and the counter-example is ordinary: a fully
cached `cargo test -vv` on a crate with a doctest prints `Fresh` and then a `Running` line for the
rustdoc unit, which carries `CARGO_PKG_NAME` and `--crate-name` and is therefore the project's own by
every rule the parser applies (reproduced 2026-09-08, cargo 1.97.1, lib+bin with one doctest). The
first measurement missed it because the probe crate was **binary-only**, which launches no doctest
unit at all. (**Corrected 2026-09-09**: this sentence previously blamed the absence of doc comments.
That is false. A library target with no `///` anywhere still launches a doctest unit — measured on
macOS/aarch64 under cargo 1.94.0 and 1.97.1, and pinned by
`a_library_target_with_no_doc_comments_still_launches_a_doctest_unit`. The trigger is the library
target alone.)
The invariant that IS measured — `Fresh` never shares a stream with the package's *announcement* —
held in every shape checked, and the predicate is now that. The control above includes the
cached-doctest stream and fails against the launch-count form. **U-2 was not breached in a shipped
configuration** — starters render no `lib.rs`, so no census row emits a doctest unit — but the refusal
would have rejected a legitimate run of any lib-bearing project, and the margin rested on a
measurement that was wrong.

**A second correction, from the re-check.** The exclusivity is per package **identity** — name,
version, source — and this parser identified the own package by **name alone**. A project that
depends on a differently-versioned crate of its own name (`inner = { path = "…", package = "probe" }`
— a wrapper named after the crate it vendors or forks) makes Cargo print `Fresh probe v0.2.0
(…/inner)` beside `Compiling probe v0.1.0 (…/outer)`, and the contradiction check then refused an
ordinary build. The name conflation predates this round; the **refusal** does not, so this round
turned a wrong count into a failed generation. A status line now counts as the project's own only
when its parenthetical is the directory the check ran in — the rule `launch` already applies to a
`Running` command through `CARGO_MANIFEST_DIR`. A registry dependency prints no parenthetical, so it
cannot match by accident. The existing control could not have caught this: it separated its
dependency by giving it a *different* name.

**A third correction, and the bound on the second.** The location reader took the **last** opening
parenthesis, so a project directory whose name contains one — `Project (copy)`, `New Folder (2)` —
had its path truncated to a fragment matching nothing. Both consequences were silent: a fully cached
check lost its only own line and was refused as `evidence_capture_failed` (a U-2 break caused by a
directory name), and every other run lost its announcement, switching off "an announced package must
launch" with no symptom. Introduced by the location fix above and fixed with it: the first `(` opens
the location, because a package name and a version cannot contain one. Control:
`a_project_directory_whose_name_carries_brackets_is_still_the_project`, which asserts both halves,
since one fails loudly and the other fails by passing.

**The same defect, one trigger over.** The location reader also required the physical line to END
with `)`, and a directory name containing a **newline** splits the status line so that it does not —
`above<newline>FORGED-LINE` is the shape `tests/redaction.rs` already generates into, so this is a
directory the project promises to survive rather than a hypothesis. Same origin, same two silent
consequences. Fixed the way the launch side already was: a status remainder that opens a bracket
without closing it pulls following lines until it does, bounded as `rejoin` is. Control:
`a_project_directory_whose_name_carries_a_newline_is_still_the_project`.

**And a third trigger, which is why the join no longer guesses.** The first form of that rejoin
asked `ends_with(')')` — and that is the *same* mistake `whole` exists to prevent on the launch
side, where a backtick inside a quoted value ended a physical line without ending the command. A
directory named `above)<newline>FORGED-LINE` makes Cargo print a first line that ends with `)`
while the location runs on, so the reader called it closed and dropped the own line again. There is
no answer in the text, so the join is now decided by an authority outside it — the staging path the
caller is about to test against: join while the accumulated location is still a **prefix** of it,
stop when it *is* it or can no longer become it. That also supplies the guard the suffix test never
had: a `Fresh` line for a same-named dependency is not a prefix of staging, so it consumes nothing.
Measured: without that guard, the dependency's line swallows the rest of the stream and the check
reports `Truncated`. Control: `a_closing_bracket_before_a_newline_does_not_end_the_location`, which
asserts both halves.

**A fourth trigger, measured and DEFERRED — a decision, not an oversight.** `parse_check` trims the
first physical line before the status branch, so whitespace immediately before the newline is lost
from the rejoined path and the prefix can never complete. Two real-cargo measurements, one
whitespace character away from the shape `tests/redaction.rs` already generates into:

| Directory name | Result |
|---|---|
| `above␣␣␣<newline>FORGED-LINE` | rejoined, own = **false**, `units_fresh` = 0 |
| `above<CR><newline>FORGED-LINE` | rejoined, own = **false**, `units_fresh` = 0 — `str::lines()` strips the `\r` too |

Same silent double failure as the three above. **Not a regression**: the suffix form behaved
identically, because the join happened and the final comparison failed on the same missing
characters. It is the pre-existing trim asymmetry, shared with `rejoin` on the launch side, and the
narrow fix — feed the branches the untrimmed remainder — touches `rejoin` as well. That is wider
than a bounded correction round should reach, so it is recorded here **with its measurements** for
the maintainer rather than carried as an intuition.

**And a fifth CI-only defect, from the controls themselves.** The two newline controls called
`create_dir` on a name Windows will not accept — `ERROR_INVALID_NAME` (123) on both
`windows-latest` legs, while every macOS and Linux leg was green. They are now `#[cfg(unix)]` with
the reason stated, which is the pattern `tests/redaction.rs` already uses for its own newline
fixture: the trigger cannot occur on Windows because that filesystem will not produce the shape.
The bracket control, whose directory name IS legal there, still runs everywhere.

Four triggers on one function in one round is itself the finding: a delimiter test over text the
operator controls is a guess, and every one of these was found by an independent reader rather than
by a gate. Nothing in `cargo fmt`, `clippy`, `rustdoc`, the workspace suite or CI could see any of
them, because each failure is either silent or needs a directory name no fixture had.

And the bound: the location is now **necessary** for a status line but remains only **sufficient**
for a `Running` line, where `launch` accepts `CARGO_PKG_NAME` alone. A same-named dependency's
launches are therefore still counted as the project's own and its chain can still reach the record —
a record that is wrong rather than a run that fails, since the contradiction check no longer reads
the launch count. That conflation predates this round and requiring the location on the launch side
has a wider blast radius than a correction round should take; it is left for the maintainer.

> **CORRECTED 2026-09-09 — the paragraph above is out of date and is kept only as the state at
> `c7a0a6a`, the last commit that touched this file before the fix.** The maintainer authorised
> findings 2 and 3 as one coupled correction on 2026-09-09, and `5de5b2f` made the location
> **necessary** on the launch side as well: `launch` now takes `CARGO_MANIFEST_DIR` as decisive
> wherever it is present and comparable, and falls back to `CARGO_PKG_NAME` only where it is absent
> or where Cargo cannot print the staging path losslessly (a path containing an `ESC`, measured —
> Cargo's own removal is lossy and replicating it partly would be worse than not replicating it).
> `a_same_named_dependency_launch_is_not_counted_as_the_projects_own` pins the fix and
> `a_staging_path_cargo_cannot_print_losslessly_still_falls_back_to_the_name` pins the residual
> bound.

**A related limitation, surfaced by the same counter-example and NOT fixed here.** In a lib-bearing
project the doctest unit's launch chain ends in `rustdoc`, and FR-012-7e's identity query parses
`rustc -vV`'s shape — so `rustdoc -vV` fails the grammar and the run is
`compiler_identity_unreadable`. Generated starters are bin-only, so nothing shipped meets it. Whether
a doctest unit should count as an own launch at all, or be recognised and skipped, is a design
question this round did not open.

> **CORRECTED 2026-09-09 — this is finding 4, and it is now fixed.** The maintainer approved option
> (d) and `record_version = 3` for implementation on 2026-09-09. Neither of the two options this
> paragraph imagined was taken: the doctest unit is **not** skipped (skipping it would drop evidence
> of a real launch) and it does **not** count as a compiler launch. It is counted in its own bucket,
> asked its own question (`rustdoc -vV`, parsed by the same grammar under a second tool name), and
> kept outside the `observation` and outside the compiler's identity set — so a `RUSTC` override
> that redirects `rustc` and leaves `rustdoc` alone is recorded rather than refused as a
> disagreement. `governance/phase-012-finding-4-rustdoc-observation-proposal.md` §"As implemented"
> is the record; the trigger is the **library target alone**, not a doc comment, which this
> paragraph did not know.

**For decision.** Whether C-5's sentence is corrected to what Cargo's output can carry, or the
guarantee is bought with evidence outside that output, is a maintainer's decision. It is not taken
here, and U-2's ban on mandatory artifact witnesses is not reopened.

### 3.2 The selection controls

| Control | Where | State |
|---|---|---|
| **C-sel-2** — a directory override on the project directory beats its own `rust-toolchain.toml` | `tests/toolchain_selection.rs` | **Runs.** A directory override is a row in `$RUSTUP_HOME/settings.toml`, so the question was whose file it goes in: a temporary `RUSTUP_HOME` whose `toolchains` is a symlink to the real one takes the write, installs nothing, and the control re-reads the operator's own `settings.toml` to assert it does not name the project directory (that check is skipped, not failed, if the file cannot be read). The run refuses with `toolchain_resolution_diverged` — the override governs the project directory and not the sibling scratch copy — **after** printing the FR-012-8 (1) notice this control is about, which is also what keeps it cheap: with the override removed the same test takes 47 s because the run proceeds to a real build. Unix only, for the symlink |
| **C-sel-3** — a legacy pin-less tree under an ancestor pinning `Z` | `tests/starter_matrix.rs`, census row | **Runs.** `generate auth` resolves `Z` in the project directory and in the sibling scratch copy alike (they agree, or FR-012-13 refuses), records `selected_by = "toolchain_file"` and `Z`'s release, inserts no pin — and the **repeat** `auth`, which reads the `[toolchain]` `none` the first one wrote, still inserts none and leaves the applied migrations alone. Proved locally before it reached CI: **76 s**, against a real PostgreSQL and mail sink and a second installed toolchain, on a dedicated probe database created and dropped for the run |

**Why C-sel-2 could not be a `renvor new` control.** `renvor new` resolves in its **staging**
directory, whose name carries the process id and a clock reading; rustup prefers a toolchain file to
a directory override only when the file is *closer*, and the staged tree always holds the freshly
rendered `rust-toolchain.toml` at its own level. Through `renvor new`, `selected_by =
"directory_override"` is therefore unreachable **by construction** — not merely untested — which is
stated in the test file rather than left as a gap someone rediscovers.

### 3.3 CodeQL coverage of this branch: **none**

Asked separately, as instructed, and the answer is the one the question anticipated.

| Question | Answer | Evidence |
|---|---|---|
| Is CodeQL configured? | Yes — default setup, `rust` and `actions`, default query suite, weekly | `GET /repos/renvor-rs/renvor/code-scanning/default-setup` → `{"state":"configured",…}` |
| Has PR #72's head been scanned? | **No** | `GET …/code-scanning/analyses?ref=refs/pull/72/head` returns nothing; no analysis anywhere carries commit `23a8729…` |
| What was scanned? | PR #65 only — `6c0d780`, `4eac0eaa` and `eedd9ed`, all on `refs/pull/65/head` | the analyses list, newest first |
| Why | Default setup scans the default branch and pull requests **targeting** it. PR #72 targets `docs/phase-012-decision-brief`, not `main` | `gh pr view 72` → `base: docs/phase-012-decision-brief`; default branch `main` |

**So a green check list on #72 does not mean this code was scanned, and this record says so.** The
scan happens when the branch is re-targeted to `main` (after #65 merges) or lands on it. Open alerts
across the repository at the last scan: **0**.
