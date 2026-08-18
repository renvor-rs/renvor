# Phase 003 — Evidence

**Feature**: [`specs/003-interactive-cli`](../specs/003-interactive-cli/spec.md) | **Tasks**: T087–T094
**Produced**: 2026-08-18 | **Branch**: `feat/phase-003-interactive-cli` | **Status**: open, unmerged

This document exists so that a gap appears as an **empty cell rather than as an absence nobody
looked for** (T088). Where something is not done, it says so.

---

## 1. Scope narrowings

`PLAN.md` §20 describes Phase 003 more broadly than what was built. **Two deliberate narrowings**
were made, and they are recorded here so that §20 is not later read as fully delivered.

### 1.1 No certificate issuance and no trust-store modification

`PLAN.md` promises "clean local HTTPS". This phase ships the **consent boundary and the
configuration surface** and nothing behind them: the selection is recorded, the consent prompt and
its non-interactive flag exist and are tested, and the operation they gate is **declared
unavailable**, exiting `3` with `details.phase` naming Phase 004.

**Why**: nothing generated in this phase terminates TLS, so a certificate issued now would protect
nothing while permanently modifying the operator's machine. The boundary is built now precisely so
that it is never built under pressure later (FR-037).

### 1.2 No archive support

Contract C-4 settled that every template is **embedded in the executable**. There is therefore no
archive path, and consequently no zip-slip defence and no decompression-amplification defence,
because the capability those defend does not exist. `tests/capabilities.rs` asserts that absence
structurally and has been **demonstrated firing** by adding `flate2`.

---

## 2. Constitution principle VII — a known non-compliance, not a narrowing

**This has its own heading deliberately.** It is not a scope narrowing; it is a governing-document
conflict, and filing it as a narrowing would understate it.

Principle VII lists **eleven** things the project wizard is to ask about: target, transport,
persistence model, database, auth starter, frontend, render mode, styling, desktop option,
capabilities, and local tooling.

**This wizard asks about three of them** — target, local tooling (container, local HTTPS), and
capabilities (example domain, seed data). The other eight correspond to flags this phase
**reserves and refuses** (FR-005b), because the phases that implement them have not happened.

The tension is real in both directions:

- Asking an operator to choose a database that the generator cannot act on would produce a recorded
  choice that no generated file reflects, which **data-model invariant I-12 forbids outright**.
- Principle VII says **MUST**, and a partially implemented command is still the command the
  principle names.

**This document does not resolve it.** See §7 (T093a), which states the two honest options and
leaves the ruling to the maintainer.

---

## 3. Requirement and success-criterion evidence (T088)

48 functional requirements, 16 success criteria, 64 rows, none omitted.

| ID | Requirement, in brief | Evidence | Status |
|---|---|---|---|
| **FR-001** | The executable is named `renvor` | `xtask/src/main.rs` asserts the built binary name, **with a control** that fails if `[[bin]]` is changed | COVERED |
| **FR-002** | Command surface, exit codes, JSON shape are public contracts, documented before completion | `tests/cmd/*.trycmd` (byte-exact surface), `tests/cli.rs` (JSON shapes via `insta`), and **published** at `docs/docs/cli.mdx` | COVERED |
| **FR-003** | Exit-code taxonomy, documented | `src/exit.rs` (`Exit`, `Code::exit` is the registry), `tests/cmd/exit-codes.trycmd`, `docs/docs/cli.mdx` | COVERED |
| **FR-004** | Human output to `stderr`, results to `stdout` | `src/output/mod.rs`; `tests/generated.rs::stdout_carries_only_the_result_so_a_pipeline_needs_no_filtering` | COVERED |
| **FR-005** | Every wizard question has an equivalent flag | `tests/transaction.rs::the_wizard_asks_exactly_these_prompts` enumerates the questions; `tests/parity.rs` proves the flag forms produce the same project | COVERED |
| **FR-005a** | The wizard asks only what this phase can honour | `src/config/prompts.rs`; the prompt census test fails if a prompt is added | COVERED — **and see the principle VII heading below** |
| **FR-005b** | Later-phase flags are reserved, not unknown and not ignored | `src/config/flags.rs::RESERVED`; `tests/generated.rs::a_reserved_flag_exits_three_with_a_parseable_error` | COVERED |
| **FR-006** | One validated configuration value from both interfaces | `ProjectConfiguration::resolve` is the only constructor; `tests/parity.rs` compares a **real pty wizard run** against a flag run byte for byte | COVERED |
| **FR-007** | Validation completes before any filesystem write | `src/config/model.rs::resolve` writes nothing; `tests/hostile.rs` asserts nothing is created for every refusal | COVERED |
| **FR-008** | Unsupported value/combination names supported values or both conflicting choices | `src/config/model.rs`; `details.supported` and `details.flags` asserted in `tests/cli.rs` snapshot and `tests/acceptance.rs` | COVERED |
| **FR-009** | Review screen lists selections, paths, warnings, and the exact equivalent command | `src/config/prompts.rs::review`; `tests/parity.rs::the_equivalent_command_printed_by_the_wizard_actually_reproduces_the_project` **runs** the printed command | COVERED — **defect found and fixed 2026-08-18** |
| **FR-010** | No prompt, no block, no substituted default without a terminal | `tests/parity.rs::the_wizard_runs_only_because_stdin_is_a_terminal` and `…an_answer_nothing_determines_is_refused_rather_than_invented` | COVERED |
| **FR-011** | Rendering in a uniquely named directory inside the destination's parent | `src/generate/place.rs::Staging`; `tests/transaction.rs` residue and cancellation coverage | COVERED |
| **FR-012** | A failure leaves a pre-existing destination exactly as it was | `tests/transaction.rs::a_failure_at_any_mutating_step_leaves_a_pre_existing_empty_destination_exactly_as_it_was` — **demonstrated failing on purpose** | COVERED |
| **FR-013** | A destination that exists and is not empty is refused | `tests/hostile.rs::an_existing_non_empty_destination_is_refused_without_being_touched` | COVERED |
| **FR-014** | Each rejection names the rule that fired | `src/paths.rs`; `tests/hostile.rs::every_project_name_refusal_names_a_distinct_rule` | COVERED — **defect found and fixed 2026-08-18** |
| **FR-015** | Concurrent runs never interleave into a corrupt state | `tests/transaction.rs::concurrent_runs_at_one_destination_produce_one_project_and_no_corruption`; `place.rs` thread race | COVERED |
| **FR-016** | Placement is a single rename, no copy fallback | `src/generate/place.rs` | COVERED |
| **FR-017** | The manifest records only honoured choices | `templates/renvor.toml.j2` carries exactly the seven honoured keys; `tests/acceptance.rs::every_generated_manifest_round_trips_through_renvor_check` | COVERED |
| **FR-018** | No secret is written to the manifest | Structural: `ProjectConfiguration` has **no field capable of holding a secret** (invariant I-3) | COVERED — structurally |
| **FR-019** | `renvor check` names the field and the constraint | `src/commands/check.rs`; `tests/cli.rs` snapshot shows `details.field` and `details.constraint` | COVERED |
| **FR-020** | `--dry-run` performs zero writes | `tests/generated.rs::a_dry_run_writes_nothing_and_its_manifest_matches_the_real_run`; `tests/acceptance.rs::the_global_dry_run_flag_reaches_every_command_that_can_change_anything` | COVERED |
| **FR-021** | Dry-run and real manifests come from one code path | `src/commands/new.rs` renders and verifies on both paths; asserted by the manifest comparison above | COVERED |
| **FR-022** | The JSON envelope | `src/output/json.rs`; `tests/cli.rs` snapshots **19 document shapes** with a control proving the shape function can tell shapes apart | COVERED |
| **FR-023** | One JSON document on failure too | `tests/acceptance.rs` (panic, malformed command line, closed stdout); `tests/cli.rs` insists on exactly one document per case | COVERED |
| **FR-024** | Embedded template set producing a project that formats, compiles, tests, starts | `src/templates.rs`; `tests/generated.rs` | COVERED |
| **FR-025** | Template versioning | `templates::VERSION`, written into every manifest | COVERED |
| **FR-026** | Bounded template expansion | `render.rs::bounds`; over-bound **and** boundary tests for all four reachable bounds | COVERED |
| **FR-027** | Allow-listed filter and function set | `Environment::empty()` plus an explicit allow-list; `no_builtin_filter_is_reachable_unless_it_is_on_the_allow_list` | COVERED |
| **FR-028** | Strict undefined behaviour | `an_undefined_variable_is_an_error_not_an_empty_rendering` | COVERED |
| **FR-029** | Two generations produce identical trees | `tests/generated.rs::generating_the_same_configuration_twice_produces_identical_trees` | COVERED |
| **FR-030** | Pre-placement verification of the generated project | `src/generate/verify.rs`; `tests/generated.rs` | COVERED |
| **FR-031** | The generated project starts | `tests/generated.rs::the_generated_binary_starts_and_names_itself` | COVERED |
| **FR-032** | `doctor` reports what it checked | `src/commands/doctor.rs`; ten unit tests including version comparison and boundary | COVERED |
| **FR-033** | `renvor check` validates without building or modifying | `src/commands/check.rs`; bounded read at 64 KiB, checked twice | COVERED |
| **FR-034** | `renvor dev` surfaces failures | `src/commands/dev.rs` | COVERED |
| **FR-035** | `docker` distinguishes not-installed from not-running | `src/commands/docker.rs`; **three** reasons since 2026-08-18, each with a distinct remedy | COVERED |
| **FR-036** | No certificate issued, no trust store modified | `src/commands/tls.rs`; `tests/tls_consent.rs` — nine tests, trust store snapshotted around every command | COVERED |
| **FR-037** | Consent boundary built now: description, explicit consent, named non-interactive flag | `src/commands/tls.rs`; `the_description_precedes_the_question_and_names_this_platforms_store` | COVERED |
| **FR-038** | Declining leaves the trust store unchanged and does not abort unrelated work | `tests/tls_consent.rs` — consent withheld at the flag **and** at the prompt | COVERED |
| **FR-039** | Hostile destinations refused | `tests/hostile.rs` — traversal (5 spellings), absolute-path injection (4), symlink, 22 reserved names × 3 spellings | COVERED |
| **FR-040** | The executable carries no archive-extraction capability | `tests/capabilities.rs`; **demonstrated firing** by adding `flate2` | COVERED |
| **FR-041** | Redaction across all four output paths | `src/output/redact.rs`, applied in `json.rs` and `output/mod.rs`; `tests/redaction.rs` with a control | COVERED |
| **FR-042** | Every input bounded, bound documented | `check.rs` 64 KiB; `render.rs::bounds` five values; `tests/bounds.rs` header tabulates where each is tested | COVERED |
| **FR-043** | No network needed for local flows | `tests/offline.rs` (proxies blackholed + `CARGO_NET_OFFLINE`) **and** `tests/capabilities.rs` (no HTTP client reachable) | COVERED — with a stated limit |
| **FR-044** | Complete resolved dependency inventory | `governance/phase-003-dependency-inventory.md` — 128 normal + 49 dev, from `cargo metadata --locked` | COVERED |
| **FR-045** | ADR for custom infrastructure chosen over a package | **Not applicable.** `cap-std` was adopted, removing the requirement. T009–T012 withdrawn; no waiver created | N/A — requirement removed |
| **FR-046** | The phase does not assume an independent review waiver is available | Recorded below under "The independent review remains open" | COVERED |
| **SC-001** | Cancelling at any prompt leaves 0 entries, at **every** prompt | `tests/transaction.rs::cancelling_at_each_prompt_exits_four_and_creates_no_destination`, parameterised over all 7 prompts, with a census test that fails if a prompt is added | COVERED |
| **SC-002** | Injected failure at any rendering step leaves 0 modifications, byte-compared | `tests/transaction.rs` — 5 mutating C-5 steps × 2 destination states | COVERED |
| **SC-003** | Prompt and flag runs produce byte-identical manifests | `tests/parity.rs` — a **real pty wizard run** vs a flag run, whole tree compared, with a control proving a different answer differs | COVERED |
| **SC-004** | 100% of unsupported values, combinations, and reserved flags rejected with 0 writes | `tests/hostile.rs`, `tests/generated.rs`, `tests/cli.rs` snapshot | COVERED |
| **SC-005** | The skeleton passes fmt, lint, build, test with 0 warnings, and starts | `tests/generated.rs` over all five variants | COVERED |
| **SC-006** | `--dry-run` 0 writes, manifest matches the real run with 0 differences | `tests/generated.rs::a_dry_run_writes_nothing_and_its_manifest_matches_the_real_run` | COVERED |
| **SC-007** | Exactly 1 JSON document on stdout in 100% of runs, 0 human text on stdout | `tests/cli.rs` (19 shapes, one document insisted on per case); `tests/generated.rs` stdout purity | COVERED |
| **SC-008** | 0 secrets in any output mode, across a secret-shaped corpus | `tests/redaction.rs` — all four paths, **with a control** proving a non-secret marker of the same shape does appear | COVERED |
| **SC-009** | 100% of hostile inputs refused before any write, with a positive control | `tests/hostile.rs` — 8 tests including `an_ordinary_legitimate_destination_still_generates` | COVERED |
| **SC-010** | 0 trust-store modifications, consent given and withheld | `tests/tls_consent.rs` — 9 tests, with **two controls**: the snapshot sees something real, and the snapshot can detect a change | COVERED |
| **SC-011** | 0 network requests during local flows, demonstrated with networking unavailable | `tests/offline.rs` — see the stated limitation in that file's header | COVERED — with a stated limit |
| **SC-012** | 0 commands block, 0 substitute a default for an unsupplied required answer | `tests/parity.rs::without_a_terminal_an_answer_nothing_determines_is_refused_rather_than_invented` and `the_help_text_documents_every_default_the_non_interactive_path_applies` | COVERED |
| **SC-013** | Every bound has a documented value and a test demonstrating it holds | 5 bounds; `tests/bounds.rs` header tabulates each over-bound and boundary test | COVERED |
| **SC-014** | Verification passes on both toolchains on every platform claimed; a platform not exercised is not claimed | See "Toolchain and platform evidence" below | SEE BELOW |
| **SC-015** | Every dependency added appears in the inventory, cross-checked against the lockfile | `governance/phase-003-dependency-inventory.md`; all five Phase 003 additions listed with their decision | COVERED |
| **SC-016** | Two generations from identical inputs produce identical manifests | `tests/generated.rs::generating_the_same_configuration_twice_produces_identical_trees` | COVERED |

### 3.1 A traceability gap, recorded rather than closed by asserting it away

A scan of `crates/renvor-cli/{src,tests}` and `xtask/src` for literal `FR-`/`SC-` citations finds
**40 of the 64 identifiers named in code or tests**. The other 24 are covered — the table above
names the evidence for each — but the evidence is **not labelled with the identifier at the point of
test**, so a future reader grepping for `SC-004` finds nothing and cannot tell coverage from absence.

That is a documentation gap rather than a coverage gap, and it is stated here rather than fixed by
sprinkling identifiers into comments in the same session that wrote the table, which would make the
grep succeed without making the traceability real.

---

## 4. Checklist results (T089)

**T089 says "all 69 items". The three checklists contain 85.** The task text is stale; the figure
below is counted from the files.

| Checklist | Items | Verdict |
|---|---|---|
| [`checklists/requirements.md`](../specs/003-interactive-cli/checklists/requirements.md) | 16 | all pass |
| [`checklists/generation-safety.md`](../specs/003-interactive-cli/checklists/generation-safety.md) | 30 | all pass |
| [`checklists/contracts.md`](../specs/003-interactive-cli/checklists/contracts.md) | 39 | all pass |
| **Total** | **85** | |

These are **requirements-quality** checklists — "unit tests for English" — so each item asks whether
a requirement is complete, clear, consistent, measurable, and covered, not whether code works. Their
verdicts were reconciled against the specification as it now stands, including the FR-010 reading
recorded in §3 and the principle VII conflict in §2.

**Two items are worth calling out because their verdicts changed during implementation:**

- *"Is every documented bound given a stated value?"* — passed on the specification and **failed in
  practice** until 2026-08-18: three of the five bounds were enforced in code and exercised by
  nothing. A stated value that no test reaches is a value, not a bound.
- *"Is the refusal for each hostile input attributable to a named rule?"* — passed on the
  specification and **failed in practice**: `validate_project_name` emitted no `details.rule` at all,
  so five distinct refusals were indistinguishable on the wire.

Both are now true in code as well as in prose. That is the argument for working checklists against
the implementation rather than against the document that describes it.

---

## 5. Toolchain and platform evidence (T090, T091, SC-014)

### 5.1 Both toolchains

| Toolchain | `cargo xtask verify` | Where |
|---|---|---|
| **1.94.0** (declared MSRV) | 11/11 checks pass | CI job `verify (1.94.0)`, and locally |
| **stable** | 11/11 checks pass | CI job `verify (stable)` |

The eleven checks are: toolchain probe, formatting, lint, tests, API documentation
(`RUSTDOCFLAGS=-D warnings`), dependency and licence policy, architecture invariants, secret scan
(history **and** working tree), documentation site (install and build), link check, and working-tree
cleanliness.

**Locally the default toolchain *is* 1.94.0**, so a local run of "both" is one toolchain run twice.
The two-toolchain claim rests on CI, where they are genuinely different jobs. Said explicitly
because a local `rustc --version` showing `1.94.0` for both is not evidence of anything.

### 5.2 Platforms actually exercised

| Platform | Toolchains | CI job | Required? |
|---|---|---|---|
| `ubuntu-latest` | 1.94.0, stable | `verify` | **required** |
| `macos-latest` | 1.94.0, stable | `platform` | advisory |
| `windows-latest` | 1.94.0, stable | `platform` | advisory |

**Platforms NOT exercised, and therefore NOT claimed**: every other one. No BSD, no illumos, no
`aarch64` Linux, no `musl` target, no 32-bit target, and no macOS or Windows version other than
whatever `macos-latest` and `windows-latest` resolve to on the day. SC-014 says a platform not
exercised in CI is not claimed; this is that statement.

**The `platform` jobs are advisory, not required** — and this phase is the reason to read that
carefully rather than as a formality. Two defects were caught **only** there:

- `Staging` held an open `cap_std::fs::Dir` on the directory its `Drop` removed and its `place`
  renamed. Unix does not mind; **Windows refuses both with `os error 32`**, so the transaction's
  central guarantee was false on Windows while every Unix test passed.
- A lost placement race was misclassified as `placement_failed`; the window was too narrow for an
  idle local machine and wide enough for a loaded runner.

Treating an advisory failure as ignorable would have shipped both.

---

## 6. Advisory reviews (T092) and their disposition (T093)

### 6.1 Standing label

**Both reviews below are NON-INDEPENDENT and ADVISORY.** They were performed by AI agents in clean
context — each was given the repository, the specification, and an adversarial brief, and neither was
given this session's history or any account of what the author believed to be true. That design makes
them useful. It does not make them independent, and they do **not** discharge the independent-review
requirement. See §8.

Each was instructed to report **what it observed**, not what it inferred, and to state explicitly if
it found nothing at a severity and what it checked in reaching that.

PLACEHOLDER_FINDINGS

## 7. T093a — Constitution principle VII, referred to the maintainer

**This section states the question and does not answer it.** T093a says: *"Record the ruling; do not
make it."*

### 7.1 The exact text

Constitution `.specify/memory/constitution.md` §VII, *Deterministic and Safe Generation*, second
sentence of the first paragraph:

> The wizard **MUST** ask for target, transport, persistence model, database, auth starter,
> frontend, compatible render mode, styling profile where applicable, desktop option, capabilities,
> and local tooling.

That is **eleven** items, and the modal verb is **MUST**.

### 7.2 What the wizard actually asks

Source: `crates/renvor-cli/src/config/prompts.rs::fill`, enumerated and asserted by
`tests/transaction.rs::the_wizard_asks_exactly_these_prompts`, which fails if a prompt is added or
removed.

| # | Prompt as the operator sees it | Principle VII item |
|---|---|---|
| 1 | `Project name` | — not in VII's list |
| 2 | `Local development domain` | — not in VII's list |
| 3 | `Generate the example domain module?` | **capabilities** |
| 4 | `Generate seed data for it?` | **capabilities** |
| 5 | `Generate container development controls?` | **local tooling** |
| 6 | `Record that local HTTPS is wanted?` | **local tooling** |
| 7 | `Create this project?` | — the FR-009 confirmation, not a choice |

**Target** is not prompted: `--target` exists, defaults to `api`, and `api` is the only value this
phase generates, so a prompt offering one option would be a question with one answer.

### 7.3 The eight that are missing

| Principle VII item | Status in this phase |
|---|---|
| transport | `--transport` reserved; refused with `details.phase` = Phase 004 |
| persistence model | `--orm` reserved; Phase 009 |
| database | `--database` reserved; Phase 009 |
| auth starter | `--auth` reserved; Phase 013 |
| frontend | `--frontend` reserved; Phase 019 |
| compatible render mode | `--render-mode` reserved; Phase 019 |
| styling profile *where applicable* | `--styling` reserved; Phase 019 |
| desktop option | `--desktop` reserved; Phase 024 |

So: **three of eleven asked, eight reserved-and-refused.** Counting generously, "target" is honoured
as a flag with a single legal value, which would make it four of eleven.

### 7.4 Why they are not asked, stated as an argument rather than as a defence

Asking an operator to choose a database that the generator cannot act on produces a **recorded choice
that no generated file reflects**, and data-model invariant I-12 forbids that outright: the manifest
records only honoured choices. The alternatives are worse in different ways — writing the choice into
`renvor.toml` describes a project that was not generated; discarding it silently loses an answer the
operator gave.

FR-005a encodes that reasoning as a requirement: *"The wizard MUST ask only the questions this phase
can honour."* **So the specification and the constitution are in direct tension, and the
specification is the junior document.**

### 7.5 The two honest rulings

**This is a MUST, so "not yet subject to it" is not available as a silent default.** A partially
implemented command is still `renvor new`, and principle VII names `renvor new`.

**Option A — keep Phase 003 open until compliant.**
Phase 003 does not close until the wizard asks all eleven, which in practice means until the phases
that honour them exist. Honest, and it makes the phase's completion depend on Phases 004–024.

**Option B — accept a narrowly scoped, time-bounded waiver naming the violated clause.**
A waiver against principle VII's wizard clause specifically — not against VII as a whole — bounded to
Phase 003, expiring when the phase that honours each remaining item ships, and recorded in
`governance/waivers.md` with the usual review requirements.

**A third framing exists and should be named rather than smuggled in:** amend principle VII so the
wizard must ask for every choice *the build can honour*, which is what FR-005a already says. That is a
**constitution amendment**, not a waiver, and it goes through the amendment process rather than
through this document.

> **Nothing has been created.** No waiver has been drafted, no ledger entry added, and no
> constitutional text changed. Waiver W-007 does **not** exist. This phase does not assume a waiver is
> available (FR-046).

### 7.6 One relevant fact for whichever ruling is taken

The waiver ledger currently records **six** waivers, all for the same single-maintainer independent-
review gap. T013 recorded that this is a trend and that Phase 003 was the first occasion it was
**reduced rather than extended** — the ADR-0011 gate was discharged by adopting a maintained package,
so no seventh waiver was created. Creating one now for a different reason would not undo that, but it
is worth deciding with the trend in view rather than in isolation.

---

## 8. The independent review remains open (T094, FR-046)

**Phase 003 is not closed, and this is the reason.**

The two reviews recorded in §6 are **NON-INDEPENDENT and ADVISORY**. They were performed by AI agents
in clean context. They are useful — they read code rather than comments and they were told to be
adversarial — and they are **not** an independent human review. Specifically:

- An agent spawned by, prompted by, and reporting to the same process that wrote the code is not
  independent of it in any sense the requirement cares about.
- Neither reviewer can be held accountable for a missed defect, which is part of what a review is.
- The maintainer's own review is also not independent — this is a single-maintainer repository, and
  that is the standing condition the six existing waivers all describe.

**What is still required to close the phase:**

1. A **qualified independent human** requirements review.
2. A **qualified independent human** security review, with particular attention to
   `crates/renvor-cli/src/paths.rs` and `crates/renvor-cli/src/generate/place.rs`.
3. The T093a ruling (§7).
4. A decision on whether the missed failing-first ordering (T008, T015–T024) blocks closure.

**No Phase 003 phase-level waiver has been created, and this document does not assume one is
available.** A self-contained packet for an independent reviewer is prepared and referenced in the
checkpoint that accompanies this document.
