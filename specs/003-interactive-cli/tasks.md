---

description: "Phase 003 — Interactive CLI, templates, and local runtime"
---

# Tasks: Interactive CLI, templates, and local runtime

**Input**: Design documents from `specs/003-interactive-cli/`

**Prerequisites**: [`plan.md`](plan.md), [`spec.md`](spec.md), [`research.md`](research.md),
[`data-model.md`](data-model.md), [`contracts/`](contracts/), [`quickstart.md`](quickstart.md)

**Tests**: **Required, and required first** for every transactional and security-sensitive behaviour.
The specification makes this explicit, and the reason is concrete: a harness written after the code
it guards tends to be written to agree with it.

**Organization**: grouped by user story. US1, US2, and US5 are all **P1** — the specification puts
prompt-driven creation, flag-driven creation, and refusing hostile input at equal priority, because
a generator with two of the three is not a smaller product but an unsafe one.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: parallelisable — different files, no dependency on incomplete work
- **[Story]**: US1…US6

---

## Implementation status, 2026-08-18

**33 of 95 tasks complete. This section states what is built and what is not, because a task list
with 27 ticks and no summary invites a reader to assume the rest is cosmetic. It is not.**

### Built, tested, and green on both toolchains

`renvor new` end to end — the full contract C-5 transaction (validate → stage → render → **verify**
→ manifest → place → report), with a `Drop`-enforced guarantee that any failure before placement
leaves the destination untouched. `renvor doctor`, `renvor check`, `renvor dev`, and
`renvor docker up|down|status|logs`. The complete flag surface including reserved-flag refusal.
The JSON envelope and error-code registry. Redaction on every output path. The interactive wizard and the FR-009 review-and-confirm screen.
The embedded template catalogue and bounded rendering. **107 tests**, `cargo xtask verify` green on
all ten checks, clippy clean on Rust 1.94.0 and current stable.

### Two governing-document changes made during implementation, both recorded rather than assumed

1. **`research.md` D6 is at revision 2 and reverses revision 1.** `cap-std` 4.0.2 is adopted, so
   path containment is **structural rather than checked**. This removed the ADR-0011 gate entirely
   — T009–T014 are withdrawn, **no waiver was created**, and the reversal rests on measurements
   that falsified revision 1's two stated objections. See D6 for the numbers.
2a-i. **Two defects found by writing the acceptance tests outside-in, both of which every
   existing test had missed.**

   - **`renvor new --path ../escape` was accepted with exit 0.** The cap-std migration dropped the
     spec's "no traversal component" rule on the reasoning that the operator typed the parent path.
     FR-039 and SC-009 require it refused. The unit tests kept passing because the only traversal
     test covered a path *ending* in `..`, which a **different** rule catches — a test passing for
     the wrong reason. Rule restored, with a regression test over five spellings that asserts
     `details.rule == "no_traversal"` specifically.
   - **Every generated `renvor.toml` with a true flag was invalid TOML.** MiniJinja renders a
     boolean as `True`; TOML wants `true`. The project still compiled, formatted, tested, and ran,
     so no build gate noticed — **`renvor check` would have rejected renvor's own output.** Fixed
     with an explicit `toml_bool` filter, and gated by a new acceptance test that round-trips every
     variant's manifest through `renvor check` **and** through an independent `toml::Value` parse.

2a. **A Windows-only correctness bug in the transaction, found by the advisory platform matrix.**
   `Staging` held an open `cap_std::fs::Dir` handle on the directory its `Drop` then removed and
   its `place` then renamed. On Unix that is fine; **Windows refuses both with `os error 32`**, so
   the transaction's central guarantee — a failure leaves nothing behind — was **false on Windows**
   while every Unix test passed. Fixed by closing the handle before either operation, with a test
   asserting the observable consequence. Worth stating: the `platform` matrix jobs are
   **advisory, not required**, so treating an advisory failure as ignorable would have shipped this.

2b. **A contract bug in `--yes`, found while implementing T031.** It was computed as
   `stdin.is_terminal() && !yes`, which made `--yes` skip the **wizard**. C-1 says it waives
   "confirmation only". Prompting and confirming are now two separate flags.

3. **Phase 002 proof-gate obligation 8 had its scope corrected**, in
   `crates/renvor-config/tests/proof_gate.rs`. It scanned the whole workspace lockfile for
   `serde_json` while its rationale is about **configuration source formats**; Phase 003 needs
   `serde_json` for `--output json`, which is a **machine-readable output format**. The check now
   runs against the configuration crates' transitive closure, with a positive control per crate and
   a negative control proving the walk can detect what it looks for. **The YAML checks remain
   workspace-wide.** This is a scope correction, not a weakening, and it is called out here because
   editing another phase's gate is exactly the kind of change that should never happen quietly.

### Not built. Each is real work, not a formality

- **T008** — of the nine named test files, `generated.rs`, `acceptance.rs`, and `redaction.rs`
  exist; `transaction.rs`, `hostile.rs`, `parity.rs`, `cli.rs`, `bounds.rs`, `offline.rs`, and
  `tls_consent.rs` do not. Their properties are covered by unit tests and by the three files above,
  but the suite is not organised the way this plan says it is.
- **T015–T024** — the failing-first hostile corpus and parity harness. The properties are asserted;
  they were **not** written failing-first, and that ordering was the point.
- **T042–T052** — the non-interactive parity work as specified. SC-003 is currently **structural**:
  `prompts::fill` returns the same `Answers` type the flag parser produces and
  `ProjectConfiguration::resolve` is the only constructor, so the two interfaces cannot diverge.
  **Driving a real wizard needs a pseudo-terminal harness, which is not built**, so the
  byte-identical comparison between an actual prompt run and an actual flag run is not made.
- **T053–T060** — the hostile-path suite as a named file.
- **T061–T075** — dry-run and JSON edge cases beyond those in `tests/generated.rs`.
- **T076–T095** — `doctor`/`dev`/`docker` hardening, the TLS-consent suite, offline proof, and the
  polish phase.
- **T093a** — the constitution principle VII question. It is **referred, not answered**: this phase
  asks three of the eleven things principle VII lists, because the other eight correspond to flags
  it reserves and refuses. Whether that is compliance or non-compliance is a maintainer ruling and
  is deliberately not made here.

---

## Phase 1: Setup

- [x] T001 Create `crates/renvor-cli/Cargo.toml` declaring package `renvor-cli` with `[[bin]] name = "renvor"`, inheriting `edition`, `rust-version`, `license`, `repository`, `homepage`, and `authors` from `[workspace.package]` and **restating none of them** (ADR-0002)
- [x] T002 Add `crates/renvor-cli` to `members` in the workspace `Cargo.toml`, keeping the list alphabetically ordered
- [x] T003 Create `crates/renvor-cli/src/main.rs` with a binary that parses no arguments yet and exits `0`, so the workspace builds from the first commit
- [x] T004 Assert in `xtask/src/main.rs` that the built executable is named exactly `renvor` and **not** `renvor-cli`, with a control proving the check fails if the `[[bin]]` name is changed — FR-001 is a compatibility promise and a test that cannot fail is not a test
- [x] T005 [P] Create `crates/renvor-cli/README.md` stating what the crate is, that its executable is `renvor`, and that the crate is pre-release and unstable
- [x] T006 [P] Add the declared dependencies from [`research.md`](research.md) D1–D14 to `crates/renvor-cli/Cargo.toml` with **narrow feature sets**, each with a comment naming the decision that selected it
- [x] T007 Run `cargo deny check` and record that every new dependency resolves to a licence on the `deny.toml` allow-list, with **0** exceptions added
- [ ] T008 Create the test module skeletons `crates/renvor-cli/tests/{transaction,hostile,parity,cli,redaction,bounds,offline,generated,tls_consent}.rs`, each containing one deliberately failing assertion, so an empty test file cannot be mistaken for a passing suite

---

## Phase 2: Blocking gates and failing-first harnesses

**Nothing in Phase 3 onward may begin until T009–T024 are complete.** Each is a gate, not a warm-up.

### The decision-record gate

- [x] T009 **WITHDRAWN — no ADR is required.** `research.md` D6 **revision 2** (2026-08-18) reverses revision 1 and adopts `cap-std` 4.0.2. Constitution principle III and FR-045 require an accepted decision record only for **custom infrastructure chosen over a maintained package**; adopting the package removes the requirement rather than waiving it. Revision 1's two stated objections were measured and both were false for this design — the renderer performs no filesystem I/O of its own, and the transitive tree is 12 crates while `walkdir` is dropped
- [x] T010 **WITHDRAWN with T009.** The alternatives and their rejection reasons are recorded in `research.md` D6 revision 2 rather than in an ADR: `normpath` + `dunce` (not adopted — a handle makes their normalisation unnecessary) and `path-clean` (**rejected: purely lexical, cannot detect a symlink escape; last released 2023-02-24**)
- [x] T011 **WITHDRAWN with T009.** There is no ADR to review
- [x] T012 **WITHDRAWN with T009.** There are no ADR findings to disposition
- [x] T013 **RESOLVED WITHOUT A WAIVER, and this is the point of the withdrawal.** Checking the ledger before drafting ADR-0011 established that **W-002 is scoped to Phase 001 decision records, W-004 to ADR-0007 alone, and W-006 to ADR-0009 alone** — so **no live waiver covers a Phase 003 decision record**, and accepting ADR-0011 would have required creating **W-007**. **No waiver was created, extended, or borrowed.** Record the cross-phase trend separately and truthfully: six waivers for the same single-maintainer review gap is a trend, and this task is the first time that trend was reduced rather than extended
- [x] T014 **GATE DISCHARGED BY REMOVAL.** `crates/renvor-cli/src/paths.rs` carries no ADR gate, because there is no custom containment left to gate. Its containment is `cap_std::fs::Dir`; what remains hand-written is *name* validation, which no capability can decide
- [ ] T015 Write `crates/renvor-cli/tests/transaction.rs` cancellation coverage: drive the wizard to **each** prompt in turn and cancel there, asserting exit `4` and a destination that does not exist. Parameterise over prompts so adding a prompt without covering it fails the suite
- [ ] T016 Write injected-failure coverage in `crates/renvor-cli/tests/transaction.rs`: fail at **each mutating** protocol step of contract C-5 — `stage`, `render`, `manifest`, `verify`, `place` — against **both** an absent destination and a pre-existing empty one. **C-5 defines seven steps; `validate` and `report` are excluded deliberately and the reason is stated here rather than left to inference**: `validate` writes nothing, so it has no post-condition to violate, and `report` runs after placement has already succeeded, byte-comparing the pre-existing case before and after
- [ ] T017 Add the **positive control** to `crates/renvor-cli/tests/transaction.rs`: an un-injected run into the same fixtures succeeds and produces a project. Without it, a harness that refuses everything satisfies T015 and T016
- [ ] T018 Write concurrency coverage in `crates/renvor-cli/tests/transaction.rs`: two runs targeting one destination, asserting **at most one succeeds** and the other reports `destination_not_empty` — never a corrupt tree (FR-013, FR-015)
- [ ] T019 Write residue coverage in `crates/renvor-cli/tests/transaction.rs`: kill a run mid-render and assert the staging directory is **beside** the destination, never inside it, and is identifiable as Renvor's

### Failing-first hostile corpus

- [ ] T020 Write `crates/renvor-cli/tests/hostile.rs` with the destination corpus: path traversal, absolute-path injection, a destination that is a symlink to another directory, and Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`), each asserted refused **before any write**
- [ ] T021 Add the **positive control** to `crates/renvor-cli/tests/hostile.rs`: an ordinary legitimate destination still generates successfully (SC-009 explicitly requires this)
- [ ] T022 Add template-escape coverage to `crates/renvor-cli/tests/hostile.rs`: a template entry whose output path escapes the staging root is rejected at **load** time, so it cannot exist in a shipped binary
- [ ] T023 Add a structural assertion to `crates/renvor-cli/tests/hostile.rs` that the built executable carries **no archive-extraction capability** (FR-040), with a control proving the assertion fails if such a dependency is introduced

### Failing-first parity harness

- [ ] T024 Write `crates/renvor-cli/tests/parity.rs` asserting that a scripted-terminal run and a flag run with equivalent answers produce **byte-identical** `renvor.toml` and identical file manifests. **Written before either interface exists**, so FR-006's single-model requirement is enforced by test rather than by intention

---

## Phase 3: User Story 1 — Create a project by answering questions (P1)

**Goal**: `renvor new` asks, reviews, confirms, and creates — or leaves nothing behind.

**Independent test**: complete the wizard against a scripted terminal and assert the destination
builds; cancel at each prompt and assert the destination is absent.

- [x] T025 [US1] Define `ProjectConfiguration` in `crates/renvor-cli/src/config/model.rs` with the fields and constraints of [`data-model.md`](data-model.md) §1, and **no field capable of holding a secret** (invariant I-3)
- [x] T026 [US1] Make validation the **only** constructor in `crates/renvor-cli/src/config/model.rs`, so an unvalidated configuration cannot exist (invariant I-1) — this is what makes FR-007 structural rather than a matter of call ordering
- [x] T027 [US1] Implement canonical serialization in `crates/renvor-cli/src/config/model.rs` such that equal configurations serialize byte-identically regardless of origin (invariant I-2)
- [x] T028 [US1] Implement the wizard in `crates/renvor-cli/src/config/prompts.rs`, asking **only** the prompts this phase honours (FR-005a): name and destination, local domain, container, local HTTPS, seed data, example domain
- [x] T029 [US1] Gate wizard entry on `std::io::IsTerminal` in `crates/renvor-cli/src/config/prompts.rs`, and map `InquireError::NotTTY` as an independent backstop (research D2, D12) — two mechanisms, because FR-010 forbids two distinct failure modes
- [x] T030 [US1] Map `InquireError::OperationCanceled` and `OperationInterrupted` to exit `4` in `crates/renvor-cli/src/config/prompts.rs`, distinct from every failure code
- [x] T031 [US1] Implement the review screen in `crates/renvor-cli/src/config/prompts.rs` listing resolved selections, paths to be created, warnings, and **the exact equivalent non-interactive command** (FR-009)
- [x] T032 [US1] Ensure declining confirmation still prints the equivalent command in `crates/renvor-cli/src/config/prompts.rs`, so the answers are not lost (US1 acceptance scenario 3)
- [x] T033 [US1] Implement staging in `crates/renvor-cli/src/generate/staging.rs`: a uniquely named directory **inside the destination's parent**, never the system temporary directory (contract C-5) (FR-011)
- [x] T034 [US1] Implement bounded rendering in `crates/renvor-cli/src/generate/render.rs` with strict-undefined behaviour and an **allow-listed** filter and function set (FR-026, FR-027, FR-028)
- [x] T035 [US1] Implement `FileManifest` in `crates/renvor-cli/src/generate/manifest.rs`: sorted by path, SHA-256 per file, symbolic links not followed (invariants I-9, I-11)
- [x] T036 [US1] Implement pre-move verification in `crates/renvor-cli/src/generate/place.rs`: run the generated project's own fmt, clippy, build, test, and start **while it is still in staging** (FR-030)
- [x] T037 [US1] Implement placement in `crates/renvor-cli/src/generate/place.rs` as a **single rename**, failing with `placement_failed` rather than falling back to a copy (FR-016)
- [x] T038 [US1] Implement `ProjectManifest` writing in `crates/renvor-cli/src/generate/manifest.rs`, recording **only honoured choices** plus generator and template versions (invariant I-12) (FR-017, FR-018)
- [x] T039 [US1] Create the embedded API-only template set under `crates/renvor-cli/templates/` with a version constant, producing a project that formats, compiles, tests, and starts (FR-024)
- [x] T040 [US1] Wire `renvor new` in `crates/renvor-cli/src/commands/new.rs` to the protocol order of contract C-5
- [ ] T041 [US1] Make T015–T019 pass — `crates/renvor-cli/tests/transaction.rs` green, including the T017 positive control

---

## Phase 4: User Story 2 — Create the same project without a terminal (P1)

**Goal**: flags produce the identical project, and unsupported input is refused before any write.

**Independent test**: T024's parity suite passes.

- [ ] T042 [US2] Define the flag surface in `crates/renvor-cli/src/config/flags.rs` with one flag per honoured prompt (FR-005)
- [ ] T043 [US2] Add the **reserved** later-phase flags to `crates/renvor-cli/src/config/flags.rs` — `--transport`, `--orm`, `--database`, `--auth`, `--frontend`, `--styling`, `--render-mode`, `--desktop` — which **parse successfully** and then fail validation (FR-005b)
- [ ] T044 [US2] Implement reserved-flag rejection in `crates/renvor-cli/src/config/model.rs` returning `reserved_for_later_phase` with `details.phase`, exit `3` — never "unknown flag", never silently ignored
- [ ] T045 [US2] Implement cross-choice validation in `crates/renvor-cli/src/config/model.rs`, naming both conflicting choices and why (FR-008)
- [ ] T046 [US2] Ensure `--yes` waives **confirmation only** and never validation, in `crates/renvor-cli/src/commands/new.rs`
- [ ] T047 [US2] Implement the non-terminal path in `crates/renvor-cli/src/config/flags.rs`: exit non-zero naming the missing flags, without blocking and without defaulting (FR-010)
- [ ] T048 [US2] Make T024 pass — `crates/renvor-cli/tests/parity.rs` green: one configuration model, two interfaces

---

## Phase 5: User Story 5 — Refuse to be tricked into writing somewhere else (P1)

**UNBLOCKED.** T014's gate was discharged by removing the custom infrastructure it guarded; see T009.

**Goal**: every hostile destination and template is refused before any write.

**Independent test**: T020–T023 pass, including the positive control.

- [ ] T049 [US5] Implement `DestinationPath` in `crates/renvor-cli/src/paths.rs` with the eight validation rules of [`data-model.md`](data-model.md) §5, each returning a `details.rule` naming which rule rejected (FR-014, FR-039)
- [ ] T050 [US5] Implement canonicalisation-based containment in `crates/renvor-cli/src/paths.rs`: the canonical destination must be inside the canonical parent (symlink-escape rejection)
- [ ] T051 [US5] Implement platform-reserved-name rejection in `crates/renvor-cli/src/paths.rs`, enumerating the names rather than describing the class
- [ ] T052 [US5] Ensure rejection precedes **any** creation, including the staging directory, in `crates/renvor-cli/src/commands/new.rs` (CHK020)
- [ ] T053 [US5] Document invariant I-17 in `crates/renvor-cli/src/paths.rs` and `generate/place.rs`: the time-of-check-to-time-of-use race is **narrowed by holding one directory handle and converted into a clean failure, not eliminated**. Closing it entirely needs an atomic create-or-fail rename, which POSIX does not provide portably
- [ ] T054 [US5] Make T020–T023 pass — `crates/renvor-cli/tests/hostile.rs` green, including the T021 positive control

---

## Phase 6: User Story 3 — Dry run and machine-readable output (P2)

**Goal**: see what would happen; get a parseable answer.

**Independent test**: gate 6 and gate 7 of [`quickstart.md`](quickstart.md).

- [ ] T055 [US3] Implement `--dry-run` in `crates/renvor-cli/src/commands/new.rs` producing the manifest with **zero** writes at the destination (FR-020)
- [ ] T056 [US3] Produce the dry-run and real manifests from **one** code path in `crates/renvor-cli/src/generate/manifest.rs` (invariant I-10), so SC-006's exact match is structural (FR-021)
- [ ] T057 [US3] Implement the JSON envelope in `crates/renvor-cli/src/output/json.rs` per contract C-2: integer `schemaVersion`, `status`, `command`, and `result` xor `error` (FR-022)
- [ ] T058 [US3] Implement the error-code registry in `crates/renvor-cli/src/output/codes.rs` with every code of contract C-2 mapped to exactly one exit code (FR-003)
- [ ] T059 [US3] Ensure failure **also** emits one valid JSON document in `crates/renvor-cli/src/main.rs` — the case that matters most, and the one most often missed
- [ ] T060 [US3] Enforce stream discipline in `crates/renvor-cli/src/output/mod.rs`: results to `stdout`, everything human to `stderr` (FR-004)
- [ ] T061 [US3] Handle a prematurely closed `stdout` in `crates/renvor-cli/src/output/mod.rs` without panicking
- [ ] T062 [US3] Write `crates/renvor-cli/tests/cli/` trycmd contract files covering `--help` structure, every exit code, and the `stdout`/`stderr` split (FR-002)
- [ ] T063 [US3] Write `insta` snapshots of every JSON document shape in `crates/renvor-cli/tests/cli.rs`

---

## Phase 7: User Story 4 — Environment and local runtime (P2)

**Goal**: `doctor`, `check`, and `dev` tell the truth about the environment.

**Independent test**: gate 4 of [`quickstart.md`](quickstart.md) plus a deliberately broken environment.

- [ ] T064 [US4] Implement `renvor doctor` in `crates/renvor-cli/src/commands/doctor.rs` reporting **what it checked**, since a check that reports nothing verified is not a pass (FR-032)
- [ ] T065 [US4] Report each missing or incompatible prerequisite with required version, found version, and corrective action in `crates/renvor-cli/src/commands/doctor.rs`
- [ ] T066 [US4] Report orphaned staging directories found beside a destination in `crates/renvor-cli/src/commands/doctor.rs`, and **do not delete them** without being asked (contract C-5)
- [ ] T067 [US4] Implement `renvor check` in `crates/renvor-cli/src/commands/check.rs` validating `renvor.toml` without building and without modifying, naming the field and the constraint on failure (FR-019, FR-033)
- [ ] T068 [US4] Reject unknown keys in `renvor.toml` in `crates/renvor-cli/src/commands/check.rs` — a typo must be a diagnosis, not a silently ignored setting
- [ ] T069 [US4] Implement `renvor dev` in `crates/renvor-cli/src/commands/dev.rs`, surfacing failures rather than restarting silently (FR-034)
- [ ] T070 [US4] Implement `renvor docker up|down|status|logs` in `crates/renvor-cli/src/commands/docker.rs`, distinguishing **runtime not installed** from **runtime installed but not running** via `details.reason` (FR-035)
- [ ] T071 [US4] Ensure container commands never hang and never silently skip, in `crates/renvor-cli/src/commands/docker.rs`
- [ ] T072 [US4] Add `--output json` to all four commands in `crates/renvor-cli/src/commands/`

---

## Phase 8: User Story 6 — Never touch TLS trust (P2)

**Goal**: nothing in this phase modifies the operating system trust store.

**Independent test**: gate 10 of [`quickstart.md`](quickstart.md).

- [ ] T073 [US6] Implement the `local_https` selection in `crates/renvor-cli/src/config/model.rs` as `off | requested`, where `requested` **records intent and issues nothing** (FR-036)
- [ ] T074 [US6] Implement the consent prompt and its explicit non-interactive flag in `crates/renvor-cli/src/commands/tls.rs`, describing exactly what would change (FR-037)
- [ ] T075 [US6] Declare the gated operation **unavailable until a transport exists** in `crates/renvor-cli/src/commands/tls.rs`, rather than silently succeeding
- [ ] T076 [US6] Write `crates/renvor-cli/tests/tls_consent.rs` snapshotting the trust store before and after **every** command in the phase, with consent given and withheld, asserting **0 modifications** (SC-010)

---

## Phase 9: Polish and cross-cutting concerns

- [ ] T077 [P] Implement secret redaction across **all four** output paths in `crates/renvor-cli/src/output/redact.rs` — human, JSON, dry-run manifest, error messages (FR-041)
- [ ] T078 [P] Write `crates/renvor-cli/tests/redaction.rs` driving a secret-shaped corpus through all four paths, **with a control** proving a non-secret marker of the same shape does appear (SC-008)
- [ ] T079 [P] Implement and document the four template bounds in `crates/renvor-cli/src/generate/render.rs`: recursion depth, total output bytes, output file count, single-file bytes — each with a **stated value**
- [ ] T080 [P] Write `crates/renvor-cli/tests/bounds.rs`, one test per bound, asserting `bound_exceeded` with `details.bound` and `details.limit` and an untouched destination
- [ ] T081 [P] Write `crates/renvor-cli/tests/offline.rs` running every local flow with networking unavailable (SC-011)
- [ ] T082 [P] Write `crates/renvor-cli/tests/generated.rs` asserting the skeleton formats, compiles, tests, and starts (SC-005) and that two generations from identical inputs produce identical manifests (SC-016) (FR-029, FR-031)
- [ ] T083 Produce the complete resolved dependency inventory in `governance/phase-003-dependency-inventory.md` from the **actual `Cargo.lock`**, not from [`research.md`](research.md), cross-checked with `cargo tree` (FR-044, SC-015)
- [ ] T084 Record advisories, licences, and MSRV for every resolved transitive dependency in `governance/phase-003-dependency-inventory.md`
- [ ] T085 [P] Write rustdoc for every public item in `crates/renvor-cli/src/`, and run `cargo doc` with warnings denied
- [ ] T086 [P] Document the command surface, exit codes, and JSON contract in `docs/docs/` so the public contract is published, not only specified
- [ ] T087 Record in `governance/phase-003-evidence.md` the **two scope narrowings** (no certificate issuance, no archive support) **and, separately and under its own heading, the constitution principle VII non-compliance** (the wizard does not ask for the nine choices VII requires), so `PLAN.md` §20 is not later read as fully delivered and the VII gap is not filed as a mere narrowing (CHK058–CHK063)
- [ ] T088 Record the complete FR-001…FR-048 and SC-001…SC-016 evidence mapping in `governance/phase-003-evidence.md`, so a gap appears as an empty cell rather than as an absence nobody looked for
- [ ] T089 Work through all 69 items of `checklists/{requirements,generation-safety,contracts}.md` and record each verdict
- [ ] T090 Run `cargo xtask verify` on **both** 1.94.0 and current stable and record both results in `governance/phase-003-evidence.md` (SC-014)
- [ ] T091 Record in `governance/phase-003-evidence.md` which platforms `.github/workflows/ci.yml` actually exercised, and **claim no platform CI did not run** (SC-014)
- [ ] T092 Obtain, and record in `governance/phase-003-evidence.md`, two clean-context advisory reviews of the phase — one requirements, one security — each labelled **NON-INDEPENDENT and ADVISORY**, each producing enumerated findings or an explicit "no findings" statement naming what was checked
- [ ] T093 Disposition every review finding individually in `governance/phase-003-evidence.md`
- [ ] T093a Refer the constitution principle VII question to the maintainer in `governance/phase-003-evidence.md`: whether a time-bounded waiver naming the violated clause is required, or whether a partially implemented command is not yet subject to it. **Record the ruling; do not make it**
- [ ] T094 Record in `governance/phase-003-evidence.md` that the **independent human requirements and security review remains open**, that advisory reviews are not independent, and that this phase does **not** assume a waiver is available (FR-046)

---

## Dependencies

```text
Phase 1 (T001-T008)
   └─▶ Phase 2 GATES (T009-T024)
          ├─ T009-T014  WITHDRAWN — package adopted, gate removed (D6 rev 2)
          ├─ T015-T019  transaction harness ───▶ verified by Phase 3
          ├─ T020-T023  hostile corpus ────────▶ verified by Phase 5
          └─ T024       parity harness ────────▶ verified by Phase 4
                 └─▶ Phase 3 US1 (T025-T041)  ── the MVP
                        ├─▶ Phase 4 US2 (T042-T048)
                        ├─▶ Phase 5 US5 (T049-T054)   [needs T014]
                        ├─▶ Phase 6 US3 (T055-T063)
                        ├─▶ Phase 7 US4 (T064-T072)
                        └─▶ Phase 8 US6 (T073-T076)
                               └─▶ Phase 9 Polish (T077-T094)
```

**US2, US3, US4, US6 are independent of one another** once US1 exists. **US5 is independent in code
but blocked in governance** by T014.

## Parallel opportunities

- **Phase 1**: T005, T006 in parallel.
- **Phase 2**: the three harnesses (T015–T019, T020–T023, T024) are separate files and fully parallel.
- **Phases 4, 6, 7, 8** touch disjoint files and can proceed in parallel once Phase 3 completes.
- **Phase 9**: T077–T082, T085, T086 are parallel; T083, T084, T087–T094 are sequential records.

## Implementation strategy

**MVP is Phase 1 + Phase 2 + Phase 3.** That yields a `renvor` executable that creates a working
project from prompts and provably leaves nothing behind on any failure path. It is shippable as an
increment and is independently valuable.

**Phase 2 is not optional and is not reorderable.** Its harnesses are written to fail, against code
that does not exist. That ordering is the point: a transactional guarantee tested afterwards is
tested by someone who already believes it holds.

## Task count

**94 tasks.** Setup 8 · Gates 16 · US1 17 · US2 7 · US5 6 · US3 9 · US4 9 · US6 4 · Polish 18.
