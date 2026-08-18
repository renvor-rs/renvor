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

## Implementation status, 2026-08-18 (revision 2)

**This section replaces a version that reported "57 of 95 tasks complete" while the checkboxes below
showed 33 ticked and 62 unticked. Neither number was reconciled against the code. Both are replaced
by the table below, which was produced by checking each task against its own acceptance wording.**

### Counts

| Status | Count | Meaning |
|---|---|---|
| **COMPLETED** | **79** | The task's full acceptance wording is met, and something fails if it stops being met. |
| **WITHDRAWN** | **4** | T009–T012. The requirement was removed, not waived — see D6 revision 2. |
| **MISSED** | **11** | T008, T015–T024. The behaviour is built and tested; the **failing-first ordering** the task asked for did not happen and cannot be created retrospectively. |
| **HUMAN-GATED** | **1** | T093a. The constitution principle VII ruling is the maintainer's to make. |
| **OPEN** | **0** | — |
| **Total** | **95** | |

The 4 WITHDRAWN tasks are ticked because the requirement no longer exists. The 11 MISSED and the 1
HUMAN-GATED are **not** ticked, because a task whose stated requirement was not met is not complete
however much of its substance was built.

### Ordering requirements that were missed, and whether they block closure

T008 and T015–T024 are **failing-first** tasks. Each says, in effect, *write this harness against
code that does not exist, watch it fail, then make it pass* — and T008 adds *"each containing one
deliberately failing assertion, so an empty test file cannot be mistaken for a passing suite"*.
Phase 2's own preamble says why: *"a transactional guarantee tested afterwards is tested by somebody
who already believes it holds."*

**That ordering did not happen.** The implementation came first and the harnesses came after. No
rerun changes that, and nothing in this document should be read as claiming otherwise. What *was*
done, on 2026-08-18:

- **All nine test files T008 names now exist** — `transaction.rs`, `hostile.rs`, `parity.rs`,
  `cli.rs`, `redaction.rs`, `bounds.rs`, `offline.rs`, `generated.rs`, `tls_consent.rs`.
- **Every behavioural requirement in T015–T024 is now implemented and passing**, including the two
  that were previously called unreachable: cancelling at **each** prompt (T015) and the
  scripted-terminal parity comparison (T024). Both were unblocked by adopting `portable-pty`
  (research D15) instead of the Unix-only `rexpect` the earlier note had settled on.
- **Every positive claim carries a negative control**, which is the compensating discipline for the
  ordering that was lost. A failing-first harness proves it can fail by having failed; these prove it
  by other means — the failure injector demonstrably makes each step fail, the un-injected control
  proves the harness is not refusing everything, the prompt census fails if a prompt is added, and
  `place`'s FR-012 promise was **demonstrated failing on purpose** by moving the injection point past
  the removal, then restored.

**Do these missed requirements block phase closure?** They are a **process** failure, not a coverage
gap, and this document does not decide the question. What it can say precisely:

- No functional requirement or success criterion is unmet *because* of the ordering.
- The compensating controls are weaker evidence than failing-first would have been: a control proves
  the harness *can* fail today, where failing-first proved it *did* fail against absent code.
- The decision is the maintainer's, alongside T093a. It is listed in the human checkpoint rather
  than resolved here.

### Deviations from the task text, stated rather than absorbed

1. **T023 is discharged in `tests/capabilities.rs`, not `tests/hostile.rs`.** The no-archive
   assertion shares a lockfile-closure walk and a negative control with the FR-043 no-network-client
   assertion. Moving one would put a single claim in two files or duplicate the walk.
2. **T080's four template bounds are unit-tested, not integration-tested.** Templates are embedded
   (contract C-4), so no external input can reach the renderer — which is the same property FR-040
   relies on. `tests/bounds.rs` covers the one bound an operator *can* aim at (`manifest_bytes`, via
   a directory named on the command line) and its header tabulates where the other four live, each
   with an over-bound **and** a boundary case.
3. **Data-model §5 rule 8 has no `details.rule`.** "Canonical destination is inside the canonical
   parent" is discharged **structurally** by the `cap_std::fs::Dir` handle, which refuses the escape
   with no rule of ours involved. That is what adopting the capability bought; there is no named rule
   because there is no check. The other seven rules each name themselves.
4. **T089 says "all 69 items". The three checklists contain 85.** The task text is stale; all 85
   were worked through and are recorded in `governance/phase-003-evidence.md`.

### Register: found and fixed on 2026-08-18 (this session)

- **The wizard's "exact equivalent non-interactive command" was not a command (FR-009).** It printed
  `renvor new <destination> --name <name> …`, and `--name` **is not a flag this program has** —
  `NewArgs` takes the name positionally and the destination as `--path`. Pasting the wizard's own
  output produced clap's *"unexpected argument '--name'"*. It survived because **every existing test
  asserted the string contained things and nothing ran it**; the unit test guarding it was even
  called `…_round_trips_to_the_same_configuration` while checking four substrings, and that name is a
  large part of why nobody looked. Fixed; the test now **executes the printed command** and compares
  the resulting tree byte for byte, and the unit test is renamed to what it actually checks.
- **`validate_project_name` emitted no `details.rule` for any of its five refusals**, so five
  unrelated problems were indistinguishable to a consumer, and **data-model §5 rule 2 —
  "no absolute path where relative is expected" — had no implementation of its own**. `renvor new
  /etc/x` *was* refused, which is why nothing caught it, but by the character-set rule, as though `/`
  were unusual punctuation. Each refusal now names its rule, and `single_path_component` is the
  named implementation of rule 2. The hostile test that "covered" this was asserting only that
  *something* failed — the same passing-for-the-wrong-reason shape as the `..` defect before it.
- **Which rule fired depended on whether the destination's parent happened to exist.** `renvor new
  /tmp/x` reported a name problem and `renvor new /absolute/path` reported a missing parent — the
  same operator mistake explained two ways, because `/tmp` exists and `/absolute` does not. A
  **supplied** name is now validated before the filesystem is consulted, which touches nothing and
  makes the diagnosis deterministic. A *derived* name still waits, because it cannot exist until the
  destination is resolved.
- **`renvor doctor` had no version check at all (T065).** `Probe::required` is a boolean meaning
  "this command needs it", and it had been standing in for a version constraint that did not exist —
  so a `cargo` older than the project's MSRV was reported as `ok`, and the operator met the problem
  as a generated project failing its own verification. Now compares against `CARGO_PKG_RUST_VERSION`
  (read from the manifest, not restated) with `semver`, and reports required version, found version,
  and remedy for anything missing **or incompatible**.
- **`docker info` could hang forever (T071).** A stale socket or a still-starting daemon answers
  neither yes nor no, and `Command::output()` waits for it indefinitely. Probes are now bounded at 20
  seconds with the child **killed and reaped** on expiry, and the third state gets its own
  `details.reason` (`not_responding`) rather than being folded into "not running". The bound is a
  bound, not a retry.
- **The JSON contract had two snake_case keys among camelCase ones.** `foundVersion` and
  `requiredVersion` shipped as `found_version`/`required_version` into the same document as
  `orphanedStaging`. Caught on the **first run** of the new `insta` shape snapshot (T063), which is
  the thing that snapshot exists to catch.
- **Invariant I-17 was documented nowhere (T053).** The TOCTOU discussion existed in `place.rs`
  prose, but the invariant was never named in either file the task requires. Now stated in both,
  including what would be needed to close the window (`renameat2(RENAME_NOREPLACE)`, Linux-only) and
  why the tests assert the *consequence* rather than an impossibility that does not hold.
- **Three template bounds were enforced and never exercised.** `output_file_count`,
  `total_output_bytes`, and the boundary cases for the other two had no tests — the constants were
  declared, checked in code, and never reached by anything. `RECURSION_DEPTH` turns out to have **no
  reachable trigger at all**: `{% include %}` is not a statement this build's MiniJinja understands,
  because `multi_template` and `macros` are off, so an entry using it is refused when the catalogue
  **loads**. That is a stronger guarantee than a depth counter, and it is now a test that fails if
  either feature is ever enabled.

### A second process failure of mine, recorded because the history is misleading without it

Commit `2448e47` was pushed **with a failing test in it**. The verification chain that was supposed
to catch it was joined with `;` rather than `&&`, so the test command's failure was discarded by the
next command in the sequence and the chain reported success. The exit status that was read belonged
to the last command in the pipeline, not to the test run.

Fixed in `9134a41`. Every verification chain now uses `&&`, and a reported exit status is read from
the command it belongs to rather than from whatever ran last. This is recorded for the same reason
the no-op edit below is: **the commit history looks clean and was not**, and a reader who trusts it
would conclude the branch was green at a commit where it was not.

### Register: what was found and fixed before 2026-08-18

**A typo in `renvor.toml` is a diagnosis rather than a silently ignored setting (T068).** serde
ignores unknown keys by default, so `local_domian` was accepted and the operator was left wondering
why their setting did nothing — a failure with no failure. `deny_unknown_fields` on every table.
**The forward-compatibility cost is real and accepted deliberately**, recorded beside the attribute.
It also makes the manifest struct **exhaustive**: adding a key to the template without adding it
here makes `renvor check` reject renvor's own output — which is exactly what happened, and what the
round-trip gate caught **within the hour**.

**`renvor doctor` finds orphaned staging and refuses to delete it (T066).** Residue was
identifiable; nothing helped anyone find it. Reported, never removed — renvor cannot tell an
abandoned directory from one belonging to a `renvor new` running in another terminal right now, so
the remedy is printed with the process id visible. Controlled by a test proving an ordinary
directory is **not** reported as an orphan.

**FR-012 is checked against the case that only just became reachable.** A failure before placement
must leave a **pre-existing empty** destination exactly as it was — still present, still empty — and
leave no staging behind. A version that removed the destination eagerly at the start of `place`
would pass every other test in that file.

**The residue promise is tested, not just documented (T019).** A run is killed with no chance to
run a destructor; the staging directory survives, is **beside** the destination rather than inside
it, **names the process** that left it, and **no project exists**. Twenty consecutive runs. Until
this test, that promise rested entirely on a comment.

**FR-015 is verified by racing, not by reading.** Six concurrent `renvor new` runs at one
destination produce exactly one success, five clean `destination_not_empty` failures each carrying a
parseable JSON document, a project that passes `renvor check`, and **no staging residue**. Repeated
five times before being committed.

`renvor new` end to end — the full contract C-5 transaction (validate → stage → render → **verify**
→ manifest → place → report), with a `Drop`-enforced guarantee that any failure before placement
leaves the destination untouched. `renvor doctor`, `renvor check`, `renvor dev`, and
`renvor docker up|down|status|logs`. The complete flag surface including reserved-flag refusal.
The JSON envelope and error-code registry. Redaction on every output path. The interactive wizard and the FR-009 review-and-confirm screen.
The embedded template catalogue and bounded rendering. **107 tests**, `cargo xtask verify` green on
all ten checks, clippy clean on Rust 1.94.0 and current stable.

### Two governing-document changes made during implementation, both recorded rather than assumed
- **`research.md` D6 is at revision 2 and reverses revision 1.** `cap-std` 4.0.2 is adopted, so
   path containment is **structural rather than checked**. This removed the ADR-0011 gate entirely
   — T009–T014 are withdrawn, **no waiver was created**, and the reversal rests on measurements
   that falsified revision 1's two stated objections. See D6 for the numbers.
- **Two components disagreed about what a valid destination is.** FR-013 refuses a
   destination that "exists and is **not empty**", so an existing *empty* one must work.
   `Destination::open` accepted it and `Staging::place` refused it — so `renvor new` into an empty
   directory validated, rendered, ran the **full pre-placement verification**, and only then failed,
   with a message that was actively false: *"appeared while the project was being generated"*, when
   it had been there all along. Fixed by removing the empty destination immediately before the
   rename. **`remove_dir` is what makes that safe**: the kernel refuses to remove a non-empty
   directory, so the emptiness check and the removal are one atomic operation rather than a check
   followed by a hopeful delete.
- **A false uniqueness claim in a comment, proved false by a sixteen-thread race on
   macOS CI.** `Staging::create` named its directory `.renvor-staging-{pid}-{nanos}` and the comment
   beside it read *"a monotonic-ish discriminator so two runs in one process never collide"*. Two
   threads can read the same nanosecond, so the claim was untrue — the race failed to create a
   staging directory whose name was already taken. **Not reachable through `renvor new`**, which is
   single-threaded; it was nonetheless a false statement about a uniqueness property, and the fix
   makes the statement true (a process-wide atomic counter) rather than deleting the claim.
   Verified deterministically — 256 staging directories in one process, all distinct — rather than
   by racing, plus 40 consecutive race runs.
- **A process failure of mine, recorded because the commit history is misleading
   without it.** Commit `1417ed6` is titled *"…and classify a lost race correctly"* and
   **contains no classification code**. Five successive string-replacement edits were made to
   `Staging::place`; two of them silently matched nothing (a no-op replace is not an error), and the
   result was committed without re-reading the function. Both the classification **and** the
   Windows handle-drop had vanished from it.
   Consequences that were reported as fact and were not: the classification was **not** fixed in
   that commit, and macOS CI passing on it was **not** evidence the fix worked — the race simply did
   not trigger. The Windows failures were the honest signal and were misattributed.
   `place` has since been **rewritten as one deliberate piece** rather than patched further, and
   verified by running the thread-race test **25 times against a single build: 25 passed, where the
   pre-rewrite code failed 4 of 6**. Every edit now asserts that its pattern matched.
- **A lost race was misclassified, found by the concurrency test on CI after five clean
   local runs.** Between the pre-rename check and the rename, another run can place its project.
   A loser then reported `placement_failed` — which says the move *mechanism* broke and sends an
   operator to debug their filesystem — instead of `destination_not_empty`, which says another run
   got there first. `place` now classifies from **the kernel's own `ErrorKind`**
   (`DirectoryNotEmpty` / `AlreadyExists`) rather than by re-stating the destination — a re-stat is
   itself racy, and that is exactly how `placement_failed` kept leaking through. It also **restores
   a removed empty destination** when the rename fails, closing an FR-012 violation that the
   empty-destination fix had itself introduced. **The local
   machine never hit the window and both macOS and Windows CI did**, which is the argument for
   keeping the platform matrix and for not trimming a slow concurrency test.
- **An unbounded input on a path the operator names.** FR-042 requires *every* input to be
   bounded with the bound documented; `renvor check` read `renvor.toml` with a plain
   `read_to_string`. Since `check` takes a **directory from the command line**, that is an
   out-of-memory anyone can trigger by pointing it at a large file. Now bounded at 64 KiB — three
   orders of magnitude above a real manifest — and the size is checked **twice**, because
   `metadata` reports the size at one instant and a file can grow before the read completes. The
   `take(limit + 1)` makes the read itself bounded rather than trusting the stat.
- **clap's own error path violated C-2, which names this failure mode explicitly.** clap
   prints prose and exits **before** any renvor code runs, so
   `renvor new demo --nonsense --output json` wrote **zero** JSON documents — while C-2 says: *"A
   command that fails by printing an unstructured error and exiting has broken this contract,
   because the consumer that asked for JSON receives something it cannot parse precisely when it
   most needs to know what went wrong."* Fixed with `try_parse` plus a narrow `argv` scan for
   `--output`, which is needed because the format has to be known **while the command line is still
   unparseable** — a chicken-and-egg problem no parser can solve for us. Two controls guard the fix:
   `--help` and `--version` arrive through the same `Err` path and must stay exit-0 successes, and
   the human path must keep clap's caret diagnostics rather than being degraded to buy the JSON one.
- **Rust's own defaults violated both output contracts on the panic path.** C-1 reserves
   exit `1` for "unclassified or internal failure — a panic", **so that anything exiting `1` is a
   bug report** — but Rust exits **101**, measured rather than assumed with a bare
   `fn main() { panic!() }`. And C-2 requires exactly one JSON document "for success and for failure
   alike. **Not zero on failure**" — a panic wrote nothing to `stdout`, so a JSON consumer got
   nothing to parse at the moment it most needed an answer. A panic hook now emits the `internal`
   envelope and exits `1`. The panic **text** is deliberately kept out of the structured envelope
   and sent to `stderr`, because it can carry arbitrary values from wherever the panic happened and
   redaction is a filter rather than a guarantee. The test trigger exists only under
   `debug_assertions`, so it cannot exist in a release binary.
- **A safety defect in the global-flag wiring.** `--dry-run` is declared global in contract
   C-1, but `main` passed it only to `new` — so **`renvor docker up --dry-run` started containers**
   and **`renvor dev --dry-run` ran the build**. A global flag that silently does nothing on the
   commands that can change the world is worse than no flag, because a user reasonably relies on it.
   Both now report what they *would* run and do nothing, and `docker` reports it **before** probing
   the runtime, so a dry run does not require a working container runtime to answer. The regression
   test is outside-in, because the defect was in the wiring rather than in either command — a unit
   test on the command could not have found it.
- **An SC-003 defect in code written this session, found by asking whether the criterion was
   actually at risk rather than assuming the type system covered it.** The rule for deriving a
   default project name and local domain existed in **two independent copies** — one in
   `ProjectConfiguration::resolve`, one in `prompts::fill` as the prompt default — and they were
   **not equivalent**: the wizard read `file_name()` off the raw requested path with a hard-coded
   `"app"` fallback while `resolve` used the *validated* destination's name. For ordinary paths they
   agreed; for a trailing separator or a `.` component they need not have. **That is precisely how
   SC-003 fails: nothing declares the interfaces different, they drift.** Each default now has one
   owner (`derive_project_name`, `derive_local_domain`), and an outside-in test compares a defaulted
   run against an explicitly-answered one byte for byte, with a control proving a different answer
   really does produce a different project.

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
- **A Windows-only correctness bug in the transaction, found by the advisory platform matrix.**
   `Staging` held an open `cap_std::fs::Dir` handle on the directory its `Drop` then removed and
   its `place` then renamed. On Unix that is fine; **Windows refuses both with `os error 32`**, so
   the transaction's central guarantee — a failure leaves nothing behind — was **false on Windows**
   while every Unix test passed. Fixed by closing the handle before either operation, with a test
   asserting the observable consequence. Worth stating: the `platform` matrix jobs are
   **advisory, not required**, so treating an advisory failure as ignorable would have shipped this.

2b. **A contract bug in `--yes`, found while implementing T031.** It was computed as
   `stdin.is_terminal() && !yes`, which made `--yes` skip the **wizard**. C-1 says it waives
   "confirmation only". Prompting and confirming are now two separate flags.
- **Phase 002 proof-gate obligation 8 had its scope corrected**, in
   `crates/renvor-config/tests/proof_gate.rs`. It scanned the whole workspace lockfile for
   `serde_json` while its rationale is about **configuration source formats**; Phase 003 needs
   `serde_json` for `--output json`, which is a **machine-readable output format**. The check now
   runs against the configuration crates' transitive closure, with a positive control per crate and
   a negative control proving the walk can detect what it looks for. **The YAML checks remain
   workspace-wide.** This is a scope correction, not a weakening, and it is called out here because
   editing another phase's gate is exactly the kind of change that should never happen quietly.

---

## Phase 1: Setup

- [x] T001 Create `crates/renvor-cli/Cargo.toml` declaring package `renvor-cli` with `[[bin]] name = "renvor"`, inheriting `edition`, `rust-version`, `license`, `repository`, `homepage`, and `authors` from `[workspace.package]` and **restating none of them** (ADR-0002)
- [x] T002 Add `crates/renvor-cli` to `members` in the workspace `Cargo.toml`, keeping the list alphabetically ordered
- [x] T003 Create `crates/renvor-cli/src/main.rs` with a binary that parses no arguments yet and exits `0`, so the workspace builds from the first commit
- [x] T004 Assert in `xtask/src/main.rs` that the built executable is named exactly `renvor` and **not** `renvor-cli`, with a control proving the check fails if the `[[bin]]` name is changed — FR-001 is a compatibility promise and a test that cannot fail is not a test
- [x] T005 [P] Create `crates/renvor-cli/README.md` stating what the crate is, that its executable is `renvor`, and that the crate is pre-release and unstable
- [x] T006 [P] Add the declared dependencies from [`research.md`](research.md) D1–D14 to `crates/renvor-cli/Cargo.toml` with **narrow feature sets**, each with a comment naming the decision that selected it
- [x] T007 Run `cargo deny check` and record that every new dependency resolves to a licence on the `deny.toml` allow-list, with **0** exceptions added
- [ ] T008 Create the test module skeletons `crates/renvor-cli/tests/{transaction,hostile,parity,cli,redaction,bounds,offline,generated,tls_consent}.rs`, each containing one deliberately failing assertion, so an empty test file cannot be mistaken for a passing suite **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**

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
- [ ] T015 Write `crates/renvor-cli/tests/transaction.rs` cancellation coverage: drive the wizard to **each** prompt in turn and cancel there, asserting exit `4` and a destination that does not exist. Parameterise over prompts so adding a prompt without covering it fails the suite **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**
- [ ] T016 Write injected-failure coverage in `crates/renvor-cli/tests/transaction.rs`: fail at **each mutating** protocol step of contract C-5 — `stage`, `render`, `manifest`, `verify`, `place` — against **both** an absent destination and a pre-existing empty one. **C-5 defines seven steps; `validate` and `report` are excluded deliberately and the reason is stated here rather than left to inference**: `validate` writes nothing, so it has no post-condition to violate, and `report` runs after placement has already succeeded, byte-comparing the pre-existing case before and after **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**
- [ ] T017 Add the **positive control** to `crates/renvor-cli/tests/transaction.rs`: an un-injected run into the same fixtures succeeds and produces a project. Without it, a harness that refuses everything satisfies T015 and T016 **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**
- [ ] T018 Write concurrency coverage in `crates/renvor-cli/tests/transaction.rs`: two runs targeting one destination, asserting **at most one succeeds** and the other reports `destination_not_empty` — never a corrupt tree (FR-013, FR-015) **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**
- [ ] T019 Write residue coverage in `crates/renvor-cli/tests/transaction.rs`: kill a run mid-render and assert the staging directory is **beside** the destination, never inside it, and is identifiable as Renvor's **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**

### Failing-first hostile corpus

- [ ] T020 Write `crates/renvor-cli/tests/hostile.rs` with the destination corpus: path traversal, absolute-path injection, a destination that is a symlink to another directory, and Windows reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`, `LPT1`–`LPT9`), each asserted refused **before any write** **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**
- [ ] T021 Add the **positive control** to `crates/renvor-cli/tests/hostile.rs`: an ordinary legitimate destination still generates successfully (SC-009 explicitly requires this) **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**
- [ ] T022 Add template-escape coverage to `crates/renvor-cli/tests/hostile.rs`: a template entry whose output path escapes the staging root is rejected at **load** time, so it cannot exist in a shipped binary **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**
- [ ] T023 Add a structural assertion to `crates/renvor-cli/tests/hostile.rs` that the built executable carries **no archive-extraction capability** (FR-040), with a control proving the assertion fails if such a dependency is introduced **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**

### Failing-first parity harness

- [ ] T024 Write `crates/renvor-cli/tests/parity.rs` asserting that a scripted-terminal run and a flag run with equivalent answers produce **byte-identical** `renvor.toml` and identical file manifests. **Written before either interface exists**, so FR-006's single-model requirement is enforced by test rather than by intention **[MISSED — the failing-first ordering; the behaviour it asks for is now complete. See "Ordering requirements that were missed".]**

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
- [x] T041 [US1] Make T015–T019 pass — `crates/renvor-cli/tests/transaction.rs` green, including the T017 positive control
---

## Phase 4: User Story 2 — Create the same project without a terminal (P1)

**Goal**: flags produce the identical project, and unsupported input is refused before any write.

**Independent test**: T024's parity suite passes.

- [x] T042 [US2] Define the flag surface in `crates/renvor-cli/src/config/flags.rs` with one flag per honoured prompt (FR-005)
- [x] T043 [US2] Add the **reserved** later-phase flags to `crates/renvor-cli/src/config/flags.rs` — `--transport`, `--orm`, `--database`, `--auth`, `--frontend`, `--styling`, `--render-mode`, `--desktop` — which **parse successfully** and then fail validation (FR-005b)
- [x] T044 [US2] Implement reserved-flag rejection in `crates/renvor-cli/src/config/model.rs` returning `reserved_for_later_phase` with `details.phase`, exit `3` — never "unknown flag", never silently ignored
- [x] T045 [US2] Implement cross-choice validation in `crates/renvor-cli/src/config/model.rs`, naming both conflicting choices and why (FR-008)
- [x] T046 [US2] Ensure `--yes` waives **confirmation only** and never validation, in `crates/renvor-cli/src/commands/new.rs`
- [x] T047 [US2] Implement the non-terminal path in `crates/renvor-cli/src/config/flags.rs`: exit non-zero naming the missing flags, without blocking and without defaulting (FR-010)
- [x] T048 [US2] Make T024 pass — `crates/renvor-cli/tests/parity.rs` green: one configuration model, two interfaces
---

## Phase 5: User Story 5 — Refuse to be tricked into writing somewhere else (P1)

**UNBLOCKED.** T014's gate was discharged by removing the custom infrastructure it guarded; see T009.

**Goal**: every hostile destination and template is refused before any write.

**Independent test**: T020–T023 pass, including the positive control.

- [x] T049 [US5] Implement `DestinationPath` in `crates/renvor-cli/src/paths.rs` with the eight validation rules of [`data-model.md`](data-model.md) §5, each returning a `details.rule` naming which rule rejected (FR-014, FR-039)
- [x] T050 [US5] Implement canonicalisation-based containment in `crates/renvor-cli/src/paths.rs`: the canonical destination must be inside the canonical parent (symlink-escape rejection)
- [x] T051 [US5] Implement platform-reserved-name rejection in `crates/renvor-cli/src/paths.rs`, enumerating the names rather than describing the class
- [x] T052 [US5] Ensure rejection precedes **any** creation, including the staging directory, in `crates/renvor-cli/src/commands/new.rs` (CHK020)
- [x] T053 [US5] Document invariant I-17 in `crates/renvor-cli/src/paths.rs` and `generate/place.rs`: the time-of-check-to-time-of-use race is **narrowed by holding one directory handle and converted into a clean failure, not eliminated**. Closing it entirely needs an atomic create-or-fail rename, which POSIX does not provide portably
- [x] T054 [US5] Make T020–T023 pass — `crates/renvor-cli/tests/hostile.rs` green, including the T021 positive control
---

## Phase 6: User Story 3 — Dry run and machine-readable output (P2)

**Goal**: see what would happen; get a parseable answer.

**Independent test**: gate 6 and gate 7 of [`quickstart.md`](quickstart.md).

- [x] T055 [US3] Implement `--dry-run` in `crates/renvor-cli/src/commands/new.rs` producing the manifest with **zero** writes at the destination (FR-020)
- [x] T056 [US3] Produce the dry-run and real manifests from **one** code path in `crates/renvor-cli/src/generate/manifest.rs` (invariant I-10), so SC-006's exact match is structural (FR-021)
- [x] T057 [US3] Implement the JSON envelope in `crates/renvor-cli/src/output/json.rs` per contract C-2: integer `schemaVersion`, `status`, `command`, and `result` xor `error` (FR-022)
- [x] T058 [US3] Implement the error-code registry in `crates/renvor-cli/src/output/codes.rs` with every code of contract C-2 mapped to exactly one exit code (FR-003)
- [x] T059 [US3] Ensure failure **also** emits one valid JSON document in `crates/renvor-cli/src/main.rs` — the case that matters most, and the one most often missed
- [x] T060 [US3] Enforce stream discipline in `crates/renvor-cli/src/output/mod.rs`: results to `stdout`, everything human to `stderr` (FR-004)
- [x] T061 [US3] Handle a prematurely closed `stdout` in `crates/renvor-cli/src/output/mod.rs` without panicking
- [x] T062 [US3] Write `crates/renvor-cli/tests/cli/` trycmd contract files covering `--help` structure, every exit code, and the `stdout`/`stderr` split (FR-002)
- [x] T063 [US3] Write `insta` snapshots of every JSON document shape in `crates/renvor-cli/tests/cli.rs`
---

## Phase 7: User Story 4 — Environment and local runtime (P2)

**Goal**: `doctor`, `check`, and `dev` tell the truth about the environment.

**Independent test**: gate 4 of [`quickstart.md`](quickstart.md) plus a deliberately broken environment.

- [x] T064 [US4] Implement `renvor doctor` in `crates/renvor-cli/src/commands/doctor.rs` reporting **what it checked**, since a check that reports nothing verified is not a pass (FR-032)
- [x] T065 [US4] Report each missing or incompatible prerequisite with required version, found version, and corrective action in `crates/renvor-cli/src/commands/doctor.rs`
- [x] T066 [US4] Report orphaned staging directories found beside a destination in `crates/renvor-cli/src/commands/doctor.rs`, and **do not delete them** without being asked (contract C-5)
- [x] T067 [US4] Implement `renvor check` in `crates/renvor-cli/src/commands/check.rs` validating `renvor.toml` without building and without modifying, naming the field and the constraint on failure (FR-019, FR-033)
- [x] T068 [US4] Reject unknown keys in `renvor.toml` in `crates/renvor-cli/src/commands/check.rs` — a typo must be a diagnosis, not a silently ignored setting
- [x] T069 [US4] Implement `renvor dev` in `crates/renvor-cli/src/commands/dev.rs`, surfacing failures rather than restarting silently (FR-034)
- [x] T070 [US4] Implement `renvor docker up|down|status|logs` in `crates/renvor-cli/src/commands/docker.rs`, distinguishing **runtime not installed** from **runtime installed but not running** via `details.reason` (FR-035)
- [x] T071 [US4] Ensure container commands never hang and never silently skip, in `crates/renvor-cli/src/commands/docker.rs`
- [x] T072 [US4] Add `--output json` to all four commands in `crates/renvor-cli/src/commands/`
---

## Phase 8: User Story 6 — Never touch TLS trust (P2)

**Goal**: nothing in this phase modifies the operating system trust store.

**Independent test**: gate 10 of [`quickstart.md`](quickstart.md).

- [x] T073 [US6] Implement the `local_https` selection in `crates/renvor-cli/src/config/model.rs` as `off | requested`, where `requested` **records intent and issues nothing** (FR-036)
- [x] T074 [US6] Implement the consent prompt and its explicit non-interactive flag in `crates/renvor-cli/src/commands/tls.rs`, describing exactly what would change (FR-037)
- [x] T075 [US6] Declare the gated operation **unavailable until a transport exists** in `crates/renvor-cli/src/commands/tls.rs`, rather than silently succeeding
- [x] T076 [US6] Write `crates/renvor-cli/tests/tls_consent.rs` snapshotting the trust store before and after **every** command in the phase, with consent given and withheld, asserting **0 modifications** (SC-010)
---

## Phase 9: Polish and cross-cutting concerns

- [x] T077 [P] Implement secret redaction across **all four** output paths in `crates/renvor-cli/src/output/redact.rs` — human, JSON, dry-run manifest, error messages (FR-041)
- [x] T078 [P] Write `crates/renvor-cli/tests/redaction.rs` driving a secret-shaped corpus through all four paths, **with a control** proving a non-secret marker of the same shape does appear (SC-008)
- [x] T079 [P] Implement and document the four template bounds in `crates/renvor-cli/src/generate/render.rs`: recursion depth, total output bytes, output file count, single-file bytes — each with a **stated value**
- [x] T080 [P] Write `crates/renvor-cli/tests/bounds.rs`, one test per bound, asserting `bound_exceeded` with `details.bound` and `details.limit` and an untouched destination
- [x] T081 [P] Write `crates/renvor-cli/tests/offline.rs` running every local flow with networking unavailable (SC-011)
- [x] T082 [P] Write `crates/renvor-cli/tests/generated.rs` asserting the skeleton formats, compiles, tests, and starts (SC-005) and that two generations from identical inputs produce identical manifests (SC-016) (FR-029, FR-031)
- [x] T083 Produce the complete resolved dependency inventory in `governance/phase-003-dependency-inventory.md` from the **actual `Cargo.lock`**, not from [`research.md`](research.md), cross-checked with `cargo tree` (FR-044, SC-015)
- [x] T084 Record advisories, licences, and MSRV for every resolved transitive dependency in `governance/phase-003-dependency-inventory.md`
- [x] T085 [P] Write rustdoc for every public item in `crates/renvor-cli/src/`, and run `cargo doc` with warnings denied
- [x] T086 [P] Document the command surface, exit codes, and JSON contract in `docs/docs/` so the public contract is published, not only specified
- [x] T087 Record in `governance/phase-003-evidence.md` the **two scope narrowings** (no certificate issuance, no archive support) **and, separately and under its own heading, the constitution principle VII non-compliance** (the wizard does not ask for the nine choices VII requires), so `PLAN.md` §20 is not later read as fully delivered and the VII gap is not filed as a mere narrowing (CHK058–CHK063)
- [x] T088 Record the complete FR-001…FR-048 and SC-001…SC-016 evidence mapping in `governance/phase-003-evidence.md`, so a gap appears as an empty cell rather than as an absence nobody looked for
- [x] T089 Work through all 69 items of `checklists/{requirements,generation-safety,contracts}.md` and record each verdict
- [x] T090 Run `cargo xtask verify` on **both** 1.94.0 and current stable and record both results in `governance/phase-003-evidence.md` (SC-014)
- [x] T091 Record in `governance/phase-003-evidence.md` which platforms `.github/workflows/ci.yml` actually exercised, and **claim no platform CI did not run** (SC-014)
- [x] T092 Obtain, and record in `governance/phase-003-evidence.md`, two clean-context advisory reviews of the phase — one requirements, one security — each labelled **NON-INDEPENDENT and ADVISORY**, each producing enumerated findings or an explicit "no findings" statement naming what was checked
- [x] T093 Disposition every review finding individually in `governance/phase-003-evidence.md`
- [ ] T093a Refer the constitution principle VII question to the maintainer in `governance/phase-003-evidence.md`: whether a time-bounded waiver naming the violated clause is required, or whether a partially implemented command is not yet subject to it. **Record the ruling; do not make it** **[HUMAN-GATED — the ruling is the maintainer's.]**
- [x] T094 Record in `governance/phase-003-evidence.md` that the **independent human requirements and security review remains open**, that advisory reviews are not independent, and that this phase does **not** assume a waiver is available (FR-046)
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

**95 tasks.** Setup 8 · Gates 16 · US1 17 · US2 7 · US5 6 · US3 9 · US4 9 · US6 4 · Polish 19.

**It said 94, and the phase breakdown summed to 94, and there have always been 95 checkboxes.** The
missing one is **T093a**, which was inserted into the polish phase after this line was written and
never added to either figure. Corrected here rather than left as a third number for a reader to
reconcile against the other two.

By status — see "Implementation status" above for what each word means:

| COMPLETED | WITHDRAWN | MISSED | HUMAN-GATED | OPEN | Total |
|---|---|---|---|---|---|
| 79 | 4 | 11 | 1 | 0 | **95** |
