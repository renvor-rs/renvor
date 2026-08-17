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

## Phase 1: Setup

- [ ] T001 Create `crates/renvor-cli/Cargo.toml` declaring package `renvor-cli` with `[[bin]] name = "renvor"`, inheriting `edition`, `rust-version`, `license`, `repository`, `homepage`, and `authors` from `[workspace.package]` and **restating none of them** (ADR-0002)
- [ ] T002 Add `crates/renvor-cli` to `members` in the workspace `Cargo.toml`, keeping the list alphabetically ordered
- [ ] T003 Create `crates/renvor-cli/src/main.rs` with a binary that parses no arguments yet and exits `0`, so the workspace builds from the first commit
- [ ] T004 Assert in `xtask/src/main.rs` that the built executable is named exactly `renvor` and **not** `renvor-cli`, with a control proving the check fails if the `[[bin]]` name is changed — FR-001 is a compatibility promise and a test that cannot fail is not a test
- [ ] T005 [P] Create `crates/renvor-cli/README.md` stating what the crate is, that its executable is `renvor`, and that the crate is pre-release and unstable
- [ ] T006 [P] Add the declared dependencies from [`research.md`](research.md) D1–D14 to `crates/renvor-cli/Cargo.toml` with **narrow feature sets**, each with a comment naming the decision that selected it
- [ ] T007 Run `cargo deny check` and record that every new dependency resolves to a licence on the `deny.toml` allow-list, with **0** exceptions added
- [ ] T008 Create the test module skeletons `crates/renvor-cli/tests/{transaction,hostile,parity,cli,redaction,bounds,offline,generated,tls_consent}.rs`, each containing one deliberately failing assertion, so an empty test file cannot be mistaken for a passing suite

---

## Phase 2: Blocking gates and failing-first harnesses

**Nothing in Phase 3 onward may begin until T009–T024 are complete.** Each is a gate, not a warm-up.

### The decision-record gate

- [ ] T009 Write `decisions/0011-path-containment-without-capability-handles.md` recording the D6 decision: **hand-composed path containment selected over `cap-std` 4.0.2**. It MUST name the package evaluated, its concrete shortcomings for this use (whole-generator adoption required, transitive tree to inventory), the ownership cost of the alternative, the **explicit statement that a checked boundary is weaker than a capability boundary**, and an exit strategy
- [ ] T010 Record in `decisions/0011-…md` the alternatives **rejected with reasons**: `cap-std` (rejected on adoption scope, not on quality), `normpath` + `dunce` (retained as supporting, not sufficient alone), and `path-clean` (**rejected: purely lexical, cannot detect a symlink escape; last released 2023-02-24**)
- [ ] T011 Obtain and record, in `decisions/0011-path-containment-without-capability-handles.md`, two clean-context advisory reviews of ADR-0011 — one architecture, one security — each labelled **NON-INDEPENDENT and ADVISORY**, each producing either enumerated findings or an explicit written "no findings" statement naming what was checked. **A review that returns nothing is recorded as NOT PERFORMED, never as passed**
- [ ] T012 Disposition every ADR-0011 finding individually (fixed, or refused with a stated reason) in `decisions/0011-path-containment-without-capability-handles.md`
- [ ] T013 Record the waiver position for ADR-0011 in `governance/waivers.md`: **neither W-004 nor W-005 authorises accepting it** — W-004 covers ADR-0007 alone and W-005 is phase-level and authorises accepting no decision record. State plainly whether a new waiver is required and, if so, that it is the **fourth** explicit exception and exceeds the ledger's own expected maximum
- [ ] T014 **GATE**: confirm ADR-0011 is `accepted` with T011–T013 on the record. **`crates/renvor-cli/src/paths.rs` MUST NOT merge before this task is complete**

### Failing-first transactional harness

- [ ] T015 Write `crates/renvor-cli/tests/transaction.rs` cancellation coverage: drive the wizard to **each** prompt in turn and cancel there, asserting exit `4` and a destination that does not exist. Parameterise over prompts so adding a prompt without covering it fails the suite
- [ ] T016 Write injected-failure coverage in `crates/renvor-cli/tests/transaction.rs`: fail at **each** protocol step (stage, render, manifest, verify, place) against **both** an absent destination and a pre-existing empty one, byte-comparing the pre-existing case before and after
- [ ] T017 Add the **positive control** to `crates/renvor-cli/tests/transaction.rs`: an un-injected run into the same fixtures succeeds and produces a project. Without it, a harness that refuses everything satisfies T015 and T016
- [ ] T018 Write concurrency coverage in `crates/renvor-cli/tests/transaction.rs`: two runs targeting one destination, asserting **at most one succeeds** and the other reports `destination_not_empty` — never a corrupt tree
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

- [ ] T025 [US1] Define `ProjectConfiguration` in `crates/renvor-cli/src/config/model.rs` with the fields and constraints of [`data-model.md`](data-model.md) §1, and **no field capable of holding a secret** (invariant I-3)
- [ ] T026 [US1] Make validation the **only** constructor in `crates/renvor-cli/src/config/model.rs`, so an unvalidated configuration cannot exist (invariant I-1) — this is what makes FR-007 structural rather than a matter of call ordering
- [ ] T027 [US1] Implement canonical serialization in `crates/renvor-cli/src/config/model.rs` such that equal configurations serialize byte-identically regardless of origin (invariant I-2)
- [ ] T028 [US1] Implement the wizard in `crates/renvor-cli/src/config/prompts.rs`, asking **only** the prompts this phase honours (FR-005a): name and destination, local domain, container, local HTTPS, seed data, example domain
- [ ] T029 [US1] Gate wizard entry on `std::io::IsTerminal` in `crates/renvor-cli/src/config/prompts.rs`, and map `InquireError::NotTTY` as an independent backstop (research D2, D12) — two mechanisms, because FR-010 forbids two distinct failure modes
- [ ] T030 [US1] Map `InquireError::OperationCanceled` and `OperationInterrupted` to exit `4` in `crates/renvor-cli/src/config/prompts.rs`, distinct from every failure code
- [ ] T031 [US1] Implement the review screen in `crates/renvor-cli/src/config/prompts.rs` listing resolved selections, paths to be created, warnings, and **the exact equivalent non-interactive command** (FR-009)
- [ ] T032 [US1] Ensure declining confirmation still prints the equivalent command in `crates/renvor-cli/src/config/prompts.rs`, so the answers are not lost (US1 acceptance scenario 3)
- [ ] T033 [US1] Implement staging in `crates/renvor-cli/src/generate/staging.rs`: a uniquely named directory **inside the destination's parent**, never the system temporary directory (contract C-5)
- [ ] T034 [US1] Implement bounded rendering in `crates/renvor-cli/src/generate/render.rs` with strict-undefined behaviour and an **allow-listed** filter and function set (FR-026, FR-027, FR-028)
- [ ] T035 [US1] Implement `FileManifest` in `crates/renvor-cli/src/generate/manifest.rs`: sorted by path, SHA-256 per file, symbolic links not followed (invariants I-9, I-11)
- [ ] T036 [US1] Implement pre-move verification in `crates/renvor-cli/src/generate/place.rs`: run the generated project's own fmt, clippy, build, test, and start **while it is still in staging** (FR-030)
- [ ] T037 [US1] Implement placement in `crates/renvor-cli/src/generate/place.rs` as a **single rename**, failing with `placement_failed` rather than falling back to a copy (FR-016)
- [ ] T038 [US1] Implement `ProjectManifest` writing in `crates/renvor-cli/src/generate/manifest.rs`, recording **only honoured choices** plus generator and template versions (invariant I-12)
- [ ] T039 [US1] Create the embedded API-only template set under `crates/renvor-cli/templates/` with a version constant, producing a project that formats, compiles, tests, and starts
- [ ] T040 [US1] Wire `renvor new` in `crates/renvor-cli/src/commands/new.rs` to the protocol order of contract C-5
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

**BLOCKED ON T014.** `paths.rs` must not merge before ADR-0011 is accepted.

**Goal**: every hostile destination and template is refused before any write.

**Independent test**: T020–T023 pass, including the positive control.

- [ ] T049 [US5] Implement `DestinationPath` in `crates/renvor-cli/src/paths.rs` with the eight validation rules of [`data-model.md`](data-model.md) §5, each returning a `details.rule` naming which rule rejected
- [ ] T050 [US5] Implement canonicalisation-based containment in `crates/renvor-cli/src/paths.rs`: the canonical destination must be inside the canonical parent (symlink-escape rejection)
- [ ] T051 [US5] Implement platform-reserved-name rejection in `crates/renvor-cli/src/paths.rs`, enumerating the names rather than describing the class
- [ ] T052 [US5] Ensure rejection precedes **any** creation, including the staging directory, in `crates/renvor-cli/src/commands/new.rs` (CHK020)
- [ ] T053 [US5] Document invariant I-17 in `crates/renvor-cli/src/paths.rs`: the time-of-check-to-time-of-use race is **converted into a clean failure, not eliminated**, and eliminating it needs the capability handles ADR-0011 rejected
- [ ] T054 [US5] Make T020–T023 pass — `crates/renvor-cli/tests/hostile.rs` green, including the T021 positive control

---

## Phase 6: User Story 3 — Dry run and machine-readable output (P2)

**Goal**: see what would happen; get a parseable answer.

**Independent test**: gate 6 and gate 7 of [`quickstart.md`](quickstart.md).

- [ ] T055 [US3] Implement `--dry-run` in `crates/renvor-cli/src/commands/new.rs` producing the manifest with **zero** writes at the destination (FR-020)
- [ ] T056 [US3] Produce the dry-run and real manifests from **one** code path in `crates/renvor-cli/src/generate/manifest.rs` (invariant I-10), so SC-006's exact match is structural
- [ ] T057 [US3] Implement the JSON envelope in `crates/renvor-cli/src/output/json.rs` per contract C-2: integer `schemaVersion`, `status`, `command`, and `result` xor `error`
- [ ] T058 [US3] Implement the error-code registry in `crates/renvor-cli/src/output/codes.rs` with every code of contract C-2 mapped to exactly one exit code
- [ ] T059 [US3] Ensure failure **also** emits one valid JSON document in `crates/renvor-cli/src/main.rs` — the case that matters most, and the one most often missed
- [ ] T060 [US3] Enforce stream discipline in `crates/renvor-cli/src/output/mod.rs`: results to `stdout`, everything human to `stderr` (FR-004)
- [ ] T061 [US3] Handle a prematurely closed `stdout` in `crates/renvor-cli/src/output/mod.rs` without panicking
- [ ] T062 [US3] Write `crates/renvor-cli/tests/cli/` trycmd contract files covering `--help` structure, every exit code, and the `stdout`/`stderr` split
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
- [ ] T082 [P] Write `crates/renvor-cli/tests/generated.rs` asserting the skeleton formats, compiles, tests, and starts (SC-005) and that two generations from identical inputs produce identical manifests (SC-016)
- [ ] T083 Produce the complete resolved dependency inventory in `governance/phase-003-dependency-inventory.md` from the **actual `Cargo.lock`**, not from [`research.md`](research.md), cross-checked with `cargo tree` (FR-044, SC-015)
- [ ] T084 Record advisories, licences, and MSRV for every resolved transitive dependency in `governance/phase-003-dependency-inventory.md`
- [ ] T085 [P] Write rustdoc for every public item in `crates/renvor-cli/src/`, and run `cargo doc` with warnings denied
- [ ] T086 [P] Document the command surface, exit codes, and JSON contract in `docs/docs/` so the public contract is published, not only specified
- [ ] T087 Record the **three scope narrowings** in `governance/phase-003-evidence.md` — no certificate issuance, no archive support, a wizard shorter than `PLAN.md` §9.1's fifteen prompts — so `PLAN.md` §20 is not later read as fully delivered (CHK058–CHK063)
- [ ] T088 Record the complete FR-001…FR-048 and SC-001…SC-016 evidence mapping in `governance/phase-003-evidence.md`, so a gap appears as an empty cell rather than as an absence nobody looked for
- [ ] T089 Work through all 69 items of `checklists/{requirements,generation-safety,contracts}.md` and record each verdict
- [ ] T090 Run `cargo xtask verify` on **both** 1.94.0 and current stable and record both results in `governance/phase-003-evidence.md` (SC-014)
- [ ] T091 Record in `governance/phase-003-evidence.md` which platforms `.github/workflows/ci.yml` actually exercised, and **claim no platform CI did not run** (SC-014)
- [ ] T092 Obtain, and record in `governance/phase-003-evidence.md`, two clean-context advisory reviews of the phase — one requirements, one security — each labelled **NON-INDEPENDENT and ADVISORY**, each producing enumerated findings or an explicit "no findings" statement naming what was checked
- [ ] T093 Disposition every review finding individually in `governance/phase-003-evidence.md`
- [ ] T094 Record in `governance/phase-003-evidence.md` that the **independent human requirements and security review remains open**, that advisory reviews are not independent, and that this phase does **not** assume a waiver is available (FR-046)

---

## Dependencies

```text
Phase 1 (T001-T008)
   └─▶ Phase 2 GATES (T009-T024)
          ├─ T009-T014  ADR-0011 ──────────────▶ BLOCKS Phase 5 (paths.rs)
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
