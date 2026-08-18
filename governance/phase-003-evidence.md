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

## 2. Constitution principle VII — a real non-compliance, RESOLVED BY AMENDMENT on 2026-08-18

**This has its own heading deliberately.** It was not a scope narrowing; it was a governing-document
conflict, and filing it as a narrowing would have understated it.

### 2.1 What was true, for the whole of Phase 003 up to 2026-08-18

Constitution **v2.0.0** principle VII listed **eleven** things the project wizard is to ask about:
target, transport, persistence model, database, auth starter, frontend, render mode, styling,
desktop option, capabilities, and local tooling.

**This wizard asks about three of them** — target (defaulted, single-valued), local tooling
(container, local HTTPS, local domain), and capabilities (example domain, seed data). The other eight
correspond to flags this phase **reserves and refuses** (FR-005b), because the phases that implement
them have not happened.

The tension was real in both directions:

- Asking an operator to choose a database that the generator cannot act on would produce a recorded
  choice that no generated file reflects, which **data-model invariant I-12 forbids outright**.
- Principle VII said **MUST**, and a partially implemented command is still the command the
  principle names.

**Phase 003 was in violation of a MUST, structurally.** No implementation work inside this phase
could have satisfied that sentence without breaking another one.

### 2.2 The ruling

The maintainer ruled on 2026-08-18. **Neither of the two options §7 offered was taken.** The ruling
was that the rule itself was wrong:

> *"Amend Constitution Principle VII to be compatible with staged delivery. The present rule requires
> questions for capabilities that do not exist and conflicts with the requirement not to solicit or
> record choices the generator cannot honour."*

| | |
|---|---|
| **Instrument** | **MAJOR constitutional amendment, 2.0.0 → 3.0.0**, dated 2026-08-18 |
| **Not** | A waiver. **W-007 does not exist** and was explicitly forbidden. A waiver is a time-bounded exception to a rule that stays correct; this rule was not correct |
| **Not** | A weakening. All eleven governed choices are **preserved**; each binds when its capability ships, and none may be dropped by an implementation that has not shipped it |
| **Record** | [`constitution-amendment-3.0.0.md`](constitution-amendment-3.0.0.md) — written proposal, impact analysis across APIs, generated projects, security, compatibility, documentation, and active phases; migration plan; recorded maintainer approval; clause-by-clause compliance verdict |
| **Canonical text** | [`../CONSTITUTION.md`](../CONSTITUTION.md), version 3.0.0, Last Amended 2026-08-18. The local `.specify/memory/constitution.md` working copy was synchronised and verified identical from the `# Renvor Constitution` heading onward by SHA-256 |
| **Phase 003 status** | **COMPLIANT** under v3.0.0. Enforced by `config::flags::tests::every_governed_choice_of_principle_seven_is_classified`, which fails if any of the eleven stops being either honoured, single-valued-and-defaulted, or reserved-with-a-named-phase |

§7 is retained as the referral it was, with the ruling appended. It is not rewritten to look as
though the question was never open.

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
| **FR-012** | A failure leaves a pre-existing destination exactly as it was | `tests/transaction.rs::a_pre_existing_empty_destination_is_refused_before_any_step_can_be_injected` (inode, mode, uid, gid compared before and after, for every injection point) and `::a_failure_at_any_mutating_step_leaves_an_absent_destination_absent` — the earlier form was **demonstrated failing on purpose** | COVERED — **test rewritten 2026-08-18**, because the destination ruling made its predecessor pass vacuously |
| **FR-013** | A destination that **already exists is refused, in every form** — empty directory, non-empty directory, file, symlink, and an entry whose state cannot be established | `src/paths.rs::every_kind_of_existing_destination_is_refused` (5 cases); `::a_destination_whose_state_cannot_be_established_fails_closed`; `src/generate/place.rs::an_empty_destination_that_appears_mid_run_is_refused_and_not_replaced`, `::no_production_path_removes_the_destination`; `tests/hostile.rs::an_existing_non_empty_destination_is_refused_without_being_touched`; `tests/transaction.rs::a_pre_existing_empty_destination_is_refused_before_any_step_can_be_injected` | COVERED — **requirement rewritten and tightened 2026-08-18 by maintainer ruling, item 4** |
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

**A correction, made after checking rather than before.** An earlier draft of this section claimed
*"T089 says 69 items; the checklists contain 85; the task text is stale"*. **That was wrong.** The
three checklists contain 85 items, of which `requirements.md`'s 16 were completed at specification
time. The **69** T089 refers to are exactly `generation-safety.md`'s 30 plus `contracts.md`'s 39 —
the ones that were outstanding. T089 was correct and the correction was not.

It is recorded because it is the same failure this phase keeps finding in its own tests: a number
asserted from a plausible reading instead of counted from the thing itself.

| Checklist | Items | Pass | Not pass |
|---|---|---|---|
| [`requirements.md`](../specs/003-interactive-cli/checklists/requirements.md) | 16 | 16 | 0 |
| [`generation-safety.md`](../specs/003-interactive-cli/checklists/generation-safety.md) | 30 | 26 | **4** |
| [`contracts.md`](../specs/003-interactive-cli/checklists/contracts.md) | 39 | 38 | **1** |
| **Total** | **85** | **80** | **5** |

These are **requirements-quality** checklists — "unit tests for the English" — so each item asks
whether a requirement is complete, clear, consistent, measurable, and covered. They are answered
against the specification, not against the code.

### 4.1 The five that do not pass, individually

| Item | Verdict | Why |
|---|---|---|
| **CHK006** | **GAP** | SC-009 requires a positive control for the hostile-destination corpus. **Nothing requires one for the cancellation or injected-failure suites.** `tasks.md` T017 supplies it as a task, so the code has one — but the *requirement* does not demand it, and a future rewrite could drop it without failing anything. |
| **CHK014** | **PARTIAL** | "Destination becomes non-empty mid-run" is carried into FR-013 and FR-015, and since 2026-08-18 FR-013 covers a destination becoming **anything** mid-run, not only non-empty. **"Disk fills during rendering" is carried into no requirement** — it appears only in the Edge Cases list. |
| **CHK016** | **SUPERSEDED** | D6 revision 2 withdrew the decision-record gate by adopting `cap-std`. There is no gate left to word as blocking. |
| **CHK017** | **SUPERSEDED** | With CHK016. No gate, so no record for it to require. |
| **CHK060** | **PARTIAL** | The shorter-wizard narrowing is recorded in `spec.md` §Clarifications and in §1–§2 here. **It is not in `PLAN.md` §20 itself**, so a reader who opens `PLAN.md` and stops there still does not encounter it. Closing it means editing `PLAN.md`, which is outside this phase's scope. |

**None of the five was closed by editing the specification to match what was built.** CHK006 in
particular would have been trivial to "pass" by adding a sentence to SC-002; it is left open because
the gap it names is real.

### 4.2 Two items whose verdict changed between the specification and the code

Worth stating, because they are the argument for working checklists against the implementation as
well as against the document:

- *"Does every bound named in the requirements have a stated value?"* (CHK022) — passes on the
  specification, and **three of the five bounds were enforced in code and exercised by no test at
  all** until 2026-08-18. A stated value that nothing reaches is a value, not a bound.
- *"Is every path-rejection rule paired with the specific attack it rejects?"* (CHK018) — passes on
  the specification, and `validate_project_name` emitted **no `details.rule` whatsoever**, so five
  distinct refusals were indistinguishable to a consumer.

Both are now true in code as well as in prose.

## 5. Toolchain and platform evidence (T090, T091, SC-014)

### 5.1 Both toolchains

| Toolchain | `cargo xtask verify` | Where |
|---|---|---|
| **1.94.0** (declared MSRV) | 11/11 checks pass | CI job `verify (1.94.0)`, and locally — this is the pinned toolchain, so a bare `cargo` is 1.94.0 |
| **stable** (1.97.1 as of 2026-08-18) | 11/11 checks pass | CI job `verify (stable)`, and locally via `cargo +stable xtask verify` |

The eleven checks are: toolchain probe, formatting, lint, tests, API documentation
(`RUSTDOCFLAGS=-D warnings`), dependency and licence policy, architecture invariants, secret scan
(history **and** working tree), documentation site (install and build), link check, and working-tree
cleanliness.

**`rust-toolchain.toml` pins 1.94.0**, so a bare `cargo xtask verify` is the MSRV run, not the
stable run. Until 2026-08-18 that meant a local run of "both" was one toolchain run twice, and the
two-toolchain claim rested on CI alone. It no longer does: the stable run is now performed locally
with an **explicit** `cargo +stable`, and `rustc +stable --version` was captured as `1.97.1` in the
same invocation so the number is measured rather than assumed. Both remain separate CI jobs as well.

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

### 5.3 The pseudo-terminal suite runs on all three platforms, and getting there found three more things

The wizard tests drive a real pty (`portable-pty`, research D15). Making them pass on Windows took
three fixes, **all of them in the harness and none in the product**, and each is recorded where it
was made:

1. **`crossterm` blocks on an unanswered cursor-position query.** It writes `ESC [ 6 n` and waits for
   the terminal to reply. A harness that only *reads* is not a terminal emulator, so nobody answered
   and the child waited forever. Diagnosed from the harness's own failure output — *child STILL
   RUNNING, reader still running, **bytes received: 4*** — and four bytes is exactly `ESC [ 6 n`.
2. **ConPTY mirrors the screen, not the bytes written**, and merges lines unpredictably: the review
   screen's last file-list entry arrived joined to the line after it. Two tests that parsed transcript
   line structure had to stop doing so.
3. **The pty wrapped long lines at 120 columns**, so a Windows temporary path split the printed
   equivalent command mid-string. The pty is now 1000 columns wide, which removes the emulator's
   layout decisions from the evidence entirely.

**None of these was a defect in `renvor`.** They are recorded because a reader comparing the Unix and
Windows runs would otherwise conclude the product behaves differently on Windows, and it does not.

**Final CI state at branch head**: every check green, including `platform (windows-latest, 1.94.0)`
and `platform (windows-latest, stable)`.

---

### 5.4 The quickstart gate sweep, run 2026-08-18 after the rulings

`quickstart.md` is the phase's success-criterion harness. Advisory finding A-R1 established that six
of its gates ran **zero tests and exited 0**, so the gates are now run through **Gate 0**, which
extracts every gate command from the document and fails any that reports no tests.

Gate 0 itself had a defect, found by running it: its pattern `*"0 passed"*` also matches
`10 passed`, so it raised a false alarm on `--test hostile`. A gate that cries wolf gets ignored, so
the pattern is now `*". 0 passed;"*` and the discrimination is demonstrated on `0 passed`,
`10 passed`, and `100 passed`.

Result of the full sweep — **15 commands, 15 ran something, 0 failures**:

| Gate command | Tests run |
|---|---|
| `--bins generate::place::tests` | 12 |
| `--bins paths::tests` | 9 |
| `--test bounds` | 2 |
| `--test cli -- every_json_document` | 1 |
| `--test generated -- a_reserved_flag_exits_three` | 1 |
| `--test generated -- every_generated_variant` | 1 |
| `--test generated -- generating_the_same_configuration_twice` | 1 |
| `--test hostile` | 10 |
| `--test offline` | 4 |
| `--test parity` | 6 |
| `--test redaction` | 4 |
| `--test tls_consent` | 9 |
| `--test transaction -- a_failure_at_any_mutating_step` | 1 |
| `--test transaction -- a_pre_existing_empty_destination_is_refused` | 1 |
| `--test transaction -- cancelling_at_each_prompt` | 1 |

The **positive controls** these gates depend on, each of which would make its gate vacuous if
removed: `an_ordinary_legitimate_destination_still_generates` and
`an_ordinary_punctuated_directory_name_is_still_accepted` (hostile), `an_uninjected_run_into_the
_same_fixtures_succeeds` and `an_unrecognised_injection_point_is_not_a_failure` (transaction),
`a_project_that_builds_and_tests_passes` (verify), `ordinary_names_are_accepted` and
`an_ordinary_destination_opens` (paths), the legitimate write in
`the_handle_refuses_an_escape_that_no_rule_in_this_module_checks_for`, and
`the_shape_function_can_tell_two_different_shapes_apart` (cli).

## 6. Advisory reviews (T092) and their disposition (T093)

### 6.1 Standing label

**All three reviews below are NON-INDEPENDENT and ADVISORY.** They were performed by AI agents in
clean context — each was given the repository, the specification, and an adversarial brief, and none
was given this session's history or any account of what the author believed to be true. That design
is what made them useful. It does **not** make them independent, and they do **not** discharge the
independent-review requirement. See §8.

Each was instructed to report what it **observed** rather than what it inferred, to attempt attacks
rather than describe them, and to state explicitly if it found nothing at a severity and what it had
checked in reaching that.

| Review | Scope | Findings |
|---|---|---|
| **A — requirements** | spec, contracts C-1…C-5, data-model I-1…I-17, quickstart, research D1–D15, tasks.md, all of `src/` and `tests/`, templates, docs, workflows | **16** (0 Critical, 4 High, 6 Medium, 6 Low) |
| **B — requirements** | 13 requirements, prioritising "a test that cannot fail"; ran mutations | **3** (0 Critical, 0 High, 1 Medium, 2 Low) |
| **C — security** | containment, data loss, redaction, TLS consent; attacks run against the real binary | **1** (0 Critical, 0 High, 1 Low-Medium) |

**No Critical finding in any review.** Review A specifically probed for and could not fault: the
transactional core, the `cap-std` containment boundary, the SC-010 trust-store guarantee ("the
best-tested area of the phase"), FR-013, FR-018/I-3, FR-021/SC-006, the panic hook and broken-pipe
handling, and FR-040/FR-043's structural assertions. Review C attacked containment, data loss,
redaction, and the consent gate and reported all of them held.

### 6.2 The finding that matters most, in the reviewers' own framing

Review A's summary: **R1, R2, R3, R5 and R11 are one defect recurring, not five — the evidence layer
is weaker than the implementation layer.** The behaviour is largely correct; what was unreliable is
the machinery that would report it *stopping* being correct.

That is the same defect this phase had already found twice in its own history, and it had it three
more times. It is the argument for the review having happened.

### 6.3 Disposition — every finding, individually (T093)

**All four High findings are FIXED**, each verified by observing the failure first and demonstrating
the fix second.

| # | Sev | Finding | Disposition |
|---|---|---|---|
| **A-R1** | **High** | **Six of sixteen quickstart gates used a `cargo test` filter matching no test** — each ran 0 tests and exited 0, so six success criteria were "verified" by commands that verified nothing | **FIXED.** Reproduced (`--test transaction -- cancellation` → *running 0 tests … ok*), all six filters corrected to real test names and each confirmed to run ≥1 test. Added **Gate 0**, a gate-of-gates that fails on any `0 passed`. |
| **A-R2** | **High** | `no_shipped_template_can_write_outside_the_destination` **could not fail** — the walk was seeded `directory == allowed == destination`, so its `outside` vector was provably always empty | **FIXED.** Confirmed by reading. Rewritten to snapshot the **parent** before and after and require that only the destination appeared. **Demonstrated failing** by making `place` write a stray file into the parent: *"wrote outside its destination `plain`: [\"stray-file-outside-the-destination\"]"*. This test was written by the author earlier the same day; the review caught it within hours. |
| **A-R3** | **High** | **Nothing lint-checked the generated project**, while FR-029 says "formatting, **linting**, building, and testing" and both `tasks.md` T036 and quickstart gate 5 claimed clippy ran | **FIXED.** Confirmed (`grep -rn clippy crates/renvor-cli/` → 0). `cargo clippy -- -D warnings` added to `verify::CHECKS`. **Demonstrated firing** by planting a lintable construct in a template: `render_failed`, `details.check = "cargo clippy -- -D warnings"`, destination absent. |
| **A-R4** | **High** | **The JSON success path did not redact.** `Envelope::failure` redacted; `Envelope::success` did not, so one input gave two answers | **FIXED.** Reproduced: same destination rendered `token=abc123secret` in JSON and `token=[redacted]` in human. `redact_value` now walks the whole result. A test drives a **successful** run through both modes, and **was demonstrated failing** with the fix reverted. |
| **A-R5** | Medium | `availability()` — the function making FR-035's not-installed/not-running distinction — was **called by no test** | **FIXED.** Split into `classify(client, daemon)`, doing the deciding without the I/O, with all six combinations asserted directly. |
| **A-R6** | Medium | Three stable registry codes emitted outside their published meaning (`manifest_invalid` for a failing `cargo test` and a missing `compose.yaml`; `placement_failed` when staging cannot be **created**) | **FIXED 2026-08-18 by maintainer ruling, item 6**, after being carried as ACCEPTED. Three accurate codes added rather than three published meanings widened: `project_verification_failed`, `container_controls_missing`, `staging_failed`. A fourth site found while fixing — pre-placement verification reporting a failing build as `render_failed`, published as *"template rendering failed"* — is corrected with them. `schemaVersion` bumped **1 → 2** and documented in C-2's *Schema history*. `exit.rs::the_registry_matches_the_published_contract_exactly` now parses C-2's registry table at compile time, so document and binary cannot drift; demonstrated failing by renaming one row and by deleting another. |
| **A-R7** | Medium | Pre-placement verification never **started** the generated binary, though C-5 step 5 and FR-029 both say it must | **FIXED** with A-R3: `cargo run --quiet` added to `CHECKS`. |
| **A-R8** | Medium | An existing **empty** destination is deleted and replaced; mode `0700` → `0755` and the inode changes — recorded in no document | **FIXED 2026-08-18 by maintainer ruling, item 4**, after being carried as PARTIALLY FIXED. The metadata is not preserved — the destination is **refused**, so there is nothing to preserve. `remove_dir` on the destination is gone from `place.rs`, and `no_production_path_removes_the_destination` reads the module's own source and fails if any removal names anything but this process's own staging directory. Asserted end to end by comparing inode, mode, uid, and gid before and after a refused run. |
| **A-R9** | Medium | The restore branch that puts back a removed empty destination after a failed rename is **unreachable from any test** | **FIXED 2026-08-18 by maintainer ruling, item 4**, after being carried as ACCEPTED — and fixed by **deletion**, not by a new injection point. The branch existed to undo a removal that no longer happens, so an untested recovery path and the silently ignored `let _ = create_dir` error both cease to exist rather than becoming tested. |
| **A-R10** | Medium | `renvor new --help` published an internal note about Rust enum memory layout as its description, frozen into the trycmd contract | **FIXED.** The boxing note is now a `//` comment; `New` has a real description. Contract regenerated **and read** — which caught the regeneration silently replacing `renvor[EXE]` with `renvor`, a Windows regression the file's own header warns about. |
| **A-R11** | Low | The doctor test named for "unparseable ≠ incompatible" passed via an unrelated disjunct; the rule was untested | **FIXED.** Extracted `compatible(minimum, found)` and stated all six combinations, including the `(Some, None)` row no real tool can produce. |
| **A-R12** | Low | `prompts.rs` claimed the wizard asks about **target**; it does not — overstating principle VII compliance by one of eleven | **FIXED.** Corrected to "two of them", with a note that this number feeds the §7 referral. |
| **A-R13** | Low | A fourth `details.reason` value, `command_failed`, emitted as a bare literal outside the enum the docs call a closed set | **FIXED.** `Unavailable::CommandFailed` added, with its own remedy and covered by the distinctness test. |
| **A-R14** | Low | `renvor tls trust` absent from contract C-1's command table | **FIXED.** Row added, naming the consent flag and that this phase is consent-only. |
| **A-R15** | Low | C-4's recursion-depth row describes `{% include %}`, which this build does not understand; quickstart gate 13 misdescribed what `--test bounds` runs | **FIXED.** Both corrected to say what is true. |
| **A-R16** | Low | `tasks.md` T036 claimed clippy and start; "107 tests" against a measured 200 | **FIXED.** T036's claim is now true (see A-R3/A-R7); the count is re-measured. |
| **B-R1** | Low | *Confirmation*: disabled RULE 0 and the traversal test failed correctly | **NO ACTION.** Independently reproduces the author's own mutation result. |
| **B-R2** | Medium | `redact.rs` claimed a test asserting "every configuration field is inert … fails when a new field is added". **No such test existed** | **FIXED.** Written as an **exhaustive destructuring** of `ProjectConfiguration`, so adding a field is a compile error. **Demonstrated**: adding an `auth_token` field produced `error[E0027]: pattern does not mention field`. |
| **B-R3** | Low | The redaction "corpus" is two values through one injection point; no dry-run-specific case | **PARTIALLY FIXED.** A successful-run case through both output modes is added (this is what caught A-R4). Broadening to more `SECRET_KEYS` and an explicit dry-run case is not done. |
| **C-S1** | Low-Med | The **path-derived** directory name reached no character check, so `--path $'…/inject\nLINE'` created a directory with an embedded newline and `…/trailing. ' created one Windows silently renames — while the same strings in the NAME position were correctly refused | **FIXED.** Control characters and a trailing dot or space are refused on the destination's final component, with distinct rules. Deliberately **narrower** than the package-name rule, with a control test proving `my.project`, `my project`, and `v1.2.3` still work. |

**Totals after the 2026-08-18 rulings: 20 findings — 18 FIXED, 1 PARTIALLY FIXED (B-R3), 0 ACCEPTED
and unfixed, 1 no action.**

Before those rulings the totals were 15 FIXED, 2 PARTIALLY FIXED, 2 ACCEPTED, 1 no action. The
maintainer took all three carried findings — A-R6, A-R8, A-R9 — and directed fixes; §6.3 records the
earlier disposition alongside the later one rather than overwriting it, so the sequence is legible.

**B-R3 remains PARTIALLY FIXED** and is the only finding not closed: the redaction corpus is
broadened by one successful-run case through both output modes, and broadening it to more
`SECRET_KEYS` values with an explicit dry-run case is still not done. It is stated as open, not
argued away.

Nothing was suppressed, dismissed, or closed by editing a requirement to match the code.

### 6.4 What the reviews could not do

Review C listed what it did **not** reach: the TOCTOU race in `place.rs` (reasoned from code, not
raced); the TLS gate at the OS/keychain level (read, not fuzzed); template and subprocess injection
in `render.rs`/`verify.rs` beyond reading; Windows-specific behaviour, because it ran on macOS; and
the container surface. Review B listed 20 requirements it did not check.

**Those gaps are why §8 exists.** An advisory review that names its own scope is more useful than one
that does not, and still not a substitute for the independent one.

### 6.5 The author's own mutation pass, filed separately on purpose

Run before the reviews arrived, and **not a review**: it was produced by the author with full
knowledge of the code, so it can only find tests that fail to fail, never tests nobody wrote. Seven
load-bearing guards were deliberately broken and all seven were caught, the working tree verified
clean after each: RULE 0 traversal, secret redaction, the TLS consent gate, FR-012's restore
promise, the prompt census, FR-040's no-archive assertion (earlier, via `flate2`), and FR-009's
equivalent command. Review B independently reproduced the first of these.

### 6.6 Mutation evidence for the 2026-08-18 corrections

Every gate added or changed by the rulings was **broken on purpose and observed failing** before
being restored, and the working tree was verified clean after each. Same caveat as §6.5: this can
only find tests that fail to fail.

| Guard | Mutation applied | Observed |
|---|---|---|
| The destination policy, at the unit level | RULE 4 given back its old arm accepting an existing **empty** directory | `paths::tests::every_kind_of_existing_destination_is_refused` **FAILED** on its first case; the other 8 tests in the module still passed, so the mutation was specific |
| The destination policy, end to end | same mutation | `transaction.rs::a_pre_existing_empty_destination_is_refused_before_any_step_can_be_injected` **FAILED**: *"an existing empty destination must be refused, whatever is injected: left: 1 right: 3"* — exit 1 rather than 3, because with the old policy the run got as far as the injected step, which is precisely what the test forbids |
| The registry-versus-contract gate, code set | one row deleted from C-2's registry table | **FAILED**: *"parsed 18 rows from the contract's registry table for 19 codes"* |
| The registry-versus-contract gate, exit column | `staging_failed` published as exit 5 | **FAILED**: *"`staging_failed` is published as exit 5 and exits 3"* |
| The Principle VII compliance gate | `--database` renamed in the reserved table, row count preserved so it still compiles | **FAILED**: *"`database` is a governed choice this phase does not ship, so `--database` must be a reserved input — dropping it from the reserved table drops the choice from the governed set, which the constitution forbids"* |
| Quickstart Gate 0's own pattern | — | Not a mutation but a **defect found by running it**: `*"0 passed"*` matches `10 passed`, so the gate reported a false alarm on its own suite. Corrected to `*". 0 passed;"*` and the discrimination demonstrated on `0 passed`, `10 passed`, and `100 passed` |

### 6.7 One defect found by re-reading the diff against the contract

Not by a test, and worth recording for that reason. `destination_exists` is emitted from **two**
sites — `Destination::open`, before anything is staged, and `Staging::place` STEP 1, immediately
before the rename. The contract publishes `details.rule` and `details.found` for that code, and only
the first site carried them. A consumer's handling would therefore have depended on **which moment
the destination happened to appear in**, which is a difference it cannot predict and the contract
does not mention.

Nothing failed. Every test passed, the registry gate passed — it checks the code set and the exit
column, not the details — and the smoke run looked right, because the smoke run hit the first site.

Fixed by extracting `paths::describe` and using it at both sites, with
`both_emit_sites_of_destination_exists_carry_the_published_details` asserting the two are identical.
The lost-race branch, which has no metadata in hand, reports `found = "unknown"` rather than
inventing a value or omitting the key; `unknown` is published in the contract as a possible value.

A second one, found the same way an hour later and worth recording because of **where** it was:
`place.rs` STEP 1's fail-closed arm reported `placement_failed` — published as *"the final move
could not be performed atomically"* — at a point where no move has been attempted and the rename is
two steps away. That is the identical category error finding A-R6 was about, **reintroduced inside
the commit that fixed A-R6**, because the code was chosen for being nearby rather than for being
true. It now reports `destination_rejected` with `rule = "destination_unverifiable"`, the same code
and rule `paths.rs` RULE 4 uses for the same condition, and
`both_fail_closed_arms_report_the_same_code_and_rule` fails if `placement_failed` returns to that
arm — demonstrated by putting it back.

Neither defect was found by a test. Both were found by re-reading the diff against the published
contract, which is the check the registry gate cannot perform: it verifies the code **set** and the
**exit** column, and says nothing about which code a given site emits or which details it carries.
That limitation is worth stating plainly rather than leaving a reader to assume the gate is stronger
than it is.

`no_production_path_removes_the_destination` is **not** in the §6.6 table, and that is deliberate: it
carries its own inline positive control — it asserts the scan matched exactly one line before
asserting anything about that line — because a source-text scan that matched nothing would otherwise
pass while checking nothing.

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

> **Under the amended principle (v3.0.0), this table is the compliant state rather than the gap.**
> Every row is *"exposed as a reserved input that fails explicitly with the phase that will introduce
> support"*, which is what the amended clause requires of a choice the current generator cannot
> honour. The eight are not dropped: each becomes mandatory in both interfaces on the day its
> capability ships, and `config::flags::tests::every_governed_choice_of_principle_seven_is_classified`
> fails if any of them stops being reserved without being implemented.

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

### 7.7 THE RULING, 2026-08-18

**The maintainer took the third framing**, which §7.5 named and deliberately did not smuggle in.

> *"Amend Constitution Principle VII to be compatible with staged delivery. … Treat this as a MAJOR
> constitutional amendment from 2.0.0 to 3.0.0. … Do not create W-007. … Do not weaken any other
> Principle VII requirement."*

| Option stated in §7.5 | Outcome |
|---|---|
| **A** — keep Phase 003 open until compliant | **Not taken.** It makes this phase's completion depend on Phases 004–024 |
| **B** — a narrowly scoped, time-bounded waiver | **Not taken, and explicitly forbidden.** W-007 was not created |
| **Third framing** — amend principle VII | **TAKEN.** Constitution 2.0.0 → 3.0.0, MAJOR, through the amendment process, not through this document |

The amendment ran the constitution's own six-item process: written proposal, impact analysis,
migration plan, recorded maintainer approval, updated version and date, and synchronisation of the
canonical text with the local tooling working copy (verified by SHA-256). All of it is in
[`constitution-amendment-3.0.0.md`](constitution-amendment-3.0.0.md).

**On §7.6's trend:** the waiver ledger still records **six** waivers. Phase 003 created none. The
one occasion it might have — this one — was resolved by fixing the rule rather than by excusing the
violation, which is the stronger outcome of the two.

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

**What is still required to close the phase — revised 2026-08-18:**

1. A **qualified independent human** requirements review. **STILL OPEN.**
2. A **qualified independent human** security review, with particular attention to
   `crates/renvor-cli/src/paths.rs` and `crates/renvor-cli/src/generate/place.rs`. **STILL OPEN.**
   The maintainer directed that this review examine the **final head after the 2026-08-18
   corrections**, not the earlier one the first packet named.
3. ~~The T093a ruling (§7).~~ **RULED** — principle VII amended to 3.0.0. See §7.7.
4. ~~A decision on whether the missed failing-first ordering blocks closure.~~ **RULED** — it does
   **not** block closure; the eleven tasks stay permanently MISSED. See `tasks.md`.

So **two** of the four remain, and both are the same thing: a qualified independent human who is not
the author, not the maintainer, and not an agent.

**No Phase 003 phase-level waiver has been created, and this document does not assume one is
available.** A self-contained packet for an independent reviewer is prepared and referenced in the
checkpoint that accompanies this document.

---

## 9. Final task counts

Counted against each task's own acceptance wording, not asserted. The definitions are in
[`tasks.md`](../specs/003-interactive-cli/tasks.md); the checkboxes there agree with this table.

| Status | Count | Which |
|---|---|---|
| **COMPLETED** | **79** | Full acceptance wording met, with something that fails if it stops being met |
| **WITHDRAWN** | **4** | T009–T012 — the requirement was removed by adopting `cap-std`, not waived |
| **MISSED** | **11** | T008, T015–T024 — the behaviour is built and tested; the **failing-first ordering** did not happen and cannot be created retrospectively. **Permanent by ruling**: these never become COMPLETED |
| **HUMAN-GATED** | **1** | T093a — **the gate was reached and discharged**: referred, and ruled on 2026-08-18. Principle VII amended to 3.0.0; no waiver |
| **OPEN** | **0** | — |
| **Total** | **95** | |

The 95 tasks are the phase's original task list and it has not grown. The 2026-08-18 corrections —
the destination policy, the error registry, and the constitutional amendment — are **maintainer
rulings on an open pull request**, not new tasks, and inventing task numbers for them would make the
denominator move while the work was being reviewed against it.

### What is required before Phase 003 can close — revised 2026-08-18

| # | Requirement | Status |
|---|---|---|
| 1 | The T093a ruling on constitution principle VII | **RULED** — amended to 3.0.0 (§7.7). No waiver; W-007 does not exist |
| 2 | A decision on whether the missed failing-first ordering blocks closure | **RULED** — it does not block closure; T008 and T015–T024 stay permanently MISSED |
| 3 | The two carried advisory findings, A-R6 and A-R9 (and A-R8, carried as partial) | **RULED and FIXED** — see §6.3. Three new registry codes, `schemaVersion` 2, and the removal branch deleted |
| 4 | **A qualified independent human requirements review** | **OPEN.** Advisory reviews do not satisfy it |
| 5 | **A qualified independent human security review**, on `paths.rs` and `place.rs`, against the **final** head | **OPEN.** Advisory reviews do not satisfy it |

**No Phase 003 phase-level waiver exists or has been drafted, and this document does not assume one
is available** (FR-046). Items 4 and 5 are the whole of what remains.

**None of the four is engineering work the author can complete.** All four are decisions.
