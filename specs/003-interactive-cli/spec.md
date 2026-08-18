# Feature Specification: Interactive CLI, templates, and local runtime

**Feature Branch**: `feat/phase-003-interactive-cli`

**Created**: 2026-08-17

**Status**: Draft

**Input**: Phase 003 of `PLAN.md` §20, elaborated from §9 (Interactive CLI contract), §7.3 (planned
workspace), and §7.4 (feature isolation), under the Renvor Constitution v2.0.0 — principally VII
(deterministic and safe generation), IV (deterministic lifecycle and failure semantics), III
(package-first boundaries), V (contract-first compatibility), and VI (security, privacy, and
fail-closed defaults).

> **What this phase is.** The first thing a person ever runs from Renvor. Phase 002 produced a
> kernel that a Rust programmer can call; Phase 003 produces an executable that a newcomer can
> type. Everything here is a **public contract** the moment it ships: the command names, the flag
> names, the exit codes, the JSON shape, and what appears on `stdout` versus `stderr`.
>
> **What this phase is not.** It is not a web framework. Phase 004 is the first real transport, so
> nothing generated here serves HTTP. A generated project is a compiling, testing, startable
> skeleton — not an application.

## Clarifications

### Session 2026-08-17

- Q: Which of `PLAN.md` §9.1's fifteen wizard prompts does this phase actually ask? → A: **Only the
  prompts this phase can honour.** The remaining flags are **reserved**: accepted by the grammar and
  rejected at validation with a message naming the phase that will support them. Rejected
  alternatives: asking all fifteen and recording the answers (breaks FR-031, because a manifest
  would describe a project that was not generated), and asking all fifteen to reject fourteen
  combinations (a hostile first run).
- Q: What does "local HTTPS" mean in a phase with no transport? → A: **The consent boundary and the
  configuration surface only. This phase issues no certificate and performs no trust-store
  modification of any kind.** A certificate generated now protects nothing, since Phase 004 is the
  first transport, and would likely expire before it had a consumer. **This narrows `PLAN.md`'s
  "clean local HTTPS" deliverable and is recorded as a narrowing, not delivered as one.**
- Q: Are templates ever read from an archive in this phase? → A: **No. Every template is embedded
  in the executable.** There is no archive path, local or remote. **Archive hardening is therefore
  removed from scope**, because a hardening test against a code path that does not exist proves
  nothing; what replaces it is a structural assertion that no archive-reading capability is present.
- Q: Where is generation staged, given that falling back to a non-atomic copy is forbidden? → A: **A
  sibling directory inside the destination's parent**, so the final rename is same-filesystem *by
  construction* rather than by luck. Rejected: staging in the system temporary directory, which is a
  different filesystem on most Linux containers and would make the forbidden fallback the common
  case.
- Q: What are the exit codes and the machine-readable output contract? → A: **A fixed, small,
  documented taxonomy** — `0` success, `1` **unclassified or internal**, `2` usage, `3` validation,
  `4` cancelled, `5` environment — and **one** JSON document on `stdout` carrying an integer
  `schemaVersion`, a `status`, and a stable `error.code` drawn from a documented registry. `1` is
  reserved so that an unclassified failure is distinguishable from a classified one, because an
  unclassified failure is a defect rather than an outcome.

## Known non-compliance with Constitution principle VII

**This is stated as non-compliance, not as a narrowing.** The clarification session called the
shorter wizard a "narrowing", and that framing was too soft: constitution v2.0.0 principle VII says

> *"The wizard **MUST** ask for target, transport, persistence model, database, auth starter,
> frontend, compatible render mode, styling profile where applicable, desktop option, capabilities,
> and local tooling."*

FR-005a asks for **none of the first nine**, because no subsystem behind them exists — Phase 004 is
the first transport, and persistence, auth, and frontends come later still. Asking would produce a
manifest recording choices the generator did not honour, which breaks FR-031 and is its own
violation.

So the position is:

| | |
|---|---|
| **What is true** | Phase 003 ships a `renvor new` that does **not** satisfy principle VII's wizard clause |
| **Why** | The subsystems the clause names do not exist yet. Compliance is not deferrable by effort; it is blocked by sequence |
| **What is NOT claimed** | That principle VII is satisfied, that the gap is minor, or that "narrowing" makes it compliant |
| **What satisfies it** | The phase that completes the wizard once the subsystems exist — `PLAN.md` §20 Phase 025, the unified full-stack generator |
| **Who decides the waiver question** | **The maintainer.** The constitution permits exceptions only through a time-bounded written waiver naming the violated rule. Whether one is required here — or whether a partially implemented command is simply not yet subject to the clause — is a governance ruling, and this specification does not make it |

**Principle VII's other clauses ARE satisfied**: both interfaces resolve to one validated
configuration; generation validates before writing, stages in an owned directory, verifies, and
commits atomically; cancellation and failure leave the destination unchanged; existing files are
never overwritten; and `--dry-run` produces an accurate manifest without writes. The gap is the
wizard's question set, and only that.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Create a project by answering questions (Priority: P1)

Someone who has just installed Renvor types `renvor new`. They are asked a short series of
questions, shown a review screen listing exactly what will be created and the equivalent
non-interactive command, and asked to confirm. On confirmation a project directory appears,
complete and working. If they press Ctrl-C at any question, nothing has been created.

**Why this priority**: This is the feature. Every other story is a variation on it or a service to
it. If only this ships, Renvor is usable.

**Independent Test**: Run `renvor new` against a scripted terminal, answer every prompt, confirm,
and assert the destination contains a project that builds. Separately, abort at each prompt in turn
and assert the destination does not exist.

**Acceptance Scenarios**:

1. **Given** an empty parent directory, **When** the wizard is completed and confirmed, **Then** a
   project directory exists at the chosen destination containing a valid `renvor.toml`, and it
   formats, compiles, tests, and starts.
2. **Given** the wizard is at any prompt, **When** the person cancels, **Then** the process exits
   non-zero with a message stating that nothing was created, **and** the destination path does not
   exist.
3. **Given** the wizard has reached the review screen, **When** the person declines confirmation,
   **Then** nothing is written and the exact equivalent non-interactive command is still printed,
   so the answers are not lost.
4. **Given** a destination that already exists and is not empty, **When** generation is attempted,
   **Then** it is refused **before any write**, naming the destination and stating that merging
   into an existing project is not supported.

---

### User Story 2 - Create the same project without a terminal (Priority: P1)

An automation author, or the same person a second time, runs the equivalent command with flags and
`--yes`. No question is asked. The result is the same project.

**Why this priority**: Equal to US1 rather than below it. Constitution VII requires *both*
interfaces to resolve to the same validated configuration, so a wizard without its flag equivalent
is an incomplete feature, not a smaller one.

**Independent Test**: Run the wizard with a scripted terminal and the flag form with the same
answers, into two destinations, and compare the two `renvor.toml` files and the two file manifests
byte for byte.

**Acceptance Scenarios**:

1. **Given** equivalent answers, **When** one project is produced by prompts and another by flags,
   **Then** both `renvor.toml` files are byte-identical and both file manifests list the same
   paths with the same content hashes.
2. **Given** a flag with an unsupported value, **When** the command runs, **Then** it exits
   non-zero **before any write**, and the message lists the supported values for that flag.
3. **Given** a combination of individually valid flags that is not a supported combination,
   **When** the command runs, **Then** it exits non-zero **before any write**, naming both flags
   and why they conflict.
4. **Given** `stdin` is not a terminal and required answers were not supplied by flags, **When**
   the command runs, **Then** it exits non-zero with a message naming the missing flags. It MUST
   NOT hang waiting for input, and it MUST NOT silently substitute defaults.

---

### User Story 3 - See what would happen, and get a machine-readable answer (Priority: P2)

Before committing, a person runs the same command with `--dry-run`. They get the complete list of
files that would be created. A tool runs it with `--output json` and gets a result it can parse.

**Why this priority**: It is what makes the destructive operation reviewable, and it is the
contract other tooling will depend on. Valuable, but the feature works without it.

**Independent Test**: Run `--dry-run` into a destination, assert the destination is unchanged and
the manifest is complete; then run for real and assert the manifest predicted the outcome exactly.

**Acceptance Scenarios**:

1. **Given** `--dry-run`, **When** the command completes, **Then** zero filesystem entries are
   created, modified, or removed anywhere outside the process's own temporary space, **and** the
   printed manifest lists every path the real run would create.
2. **Given** a `--dry-run` manifest and a subsequent real run with identical inputs, **When** both
   complete, **Then** the set of created paths equals the manifest exactly — no extra file, no
   missing file.
3. **Given** `--output json`, **When** the command succeeds or fails, **Then** `stdout` contains
   one valid JSON document and nothing else, and all human-readable progress went to `stderr`.
4. **Given** `--output json`, **When** the command fails, **Then** the JSON document reports the
   failure in the same documented shape as success, rather than the process printing an unstructured
   error and exiting.

---

### User Story 4 - Find out why the environment is wrong (Priority: P2)

A person's project will not build, or their tooling is missing. They run `renvor doctor` and are
told what is wrong and what to do about it. `renvor check` validates the project without building
it. `renvor dev` runs the local development loop.

**Why this priority**: These are what turn a generated skeleton into something maintainable. They
are not needed to create the first project.

**Independent Test**: Run `renvor doctor` in an environment with a deliberately missing prerequisite
and assert it identifies that prerequisite by name and exits non-zero.

**Acceptance Scenarios**:

1. **Given** a missing required tool, **When** `renvor doctor` runs, **Then** it names the tool, its
   required version, what was found instead, and a corrective action, and exits non-zero.
2. **Given** a healthy environment, **When** `renvor doctor` runs, **Then** it exits zero and reports
   what it checked — a check that reports nothing verified is not a pass.
3. **Given** a project whose `renvor.toml` is invalid, **When** `renvor check` runs, **Then** it
   names the field and the constraint violated, and exits non-zero without modifying the project.
4. **Given** any of these commands, **When** run with `--output json`, **Then** the result is
   machine-readable in the same documented shape.

---

### User Story 5 - Refuse to be tricked into writing somewhere else (Priority: P1)

A hostile or careless input — a destination containing `..`, an absolute path where a relative one
was expected, a symlink pointing outside the intended tree, a Windows reserved device name, a
template that tries to escape its output directory — is refused rather than followed.

**Why this priority**: P1 with US1, not below it. A generator that can be persuaded to write outside
its destination is a file-overwrite primitive, and this is the phase where a person first runs
Renvor against a directory they care about.

**Independent Test**: A table of hostile inputs, each asserted to be refused before any write, with
a positive control proving the same code path accepts a legitimate destination.

**Acceptance Scenarios**:

1. **Given** a destination containing a path-traversal component, **When** generation is attempted,
   **Then** it is refused before any write, and no path outside the intended destination is
   created, modified, or removed.
2. **Given** a destination that is a symlink to another directory, **When** generation is attempted,
   **Then** it is refused before any write rather than writing through the link.
3. **Given** a template whose output path escapes the destination, **When** rendering is attempted,
   **Then** it is refused and the destination is left unchanged.
4. **Given** a reserved device name as a project name on a platform that has them, **When**
   generation is attempted, **Then** it is refused with a message naming the restriction.
5. **POSITIVE CONTROL** — **Given** an ordinary legitimate destination, **When** generation runs,
   **Then** it succeeds. A refusal path that refuses everything satisfies scenarios 1–4 and is
   worthless.

---

### User Story 6 - Never have TLS trust changed behind your back (Priority: P2)

A person enables local HTTPS. Anything that would modify the operating system's trust store is
described first and requires explicit consent. Declining leaves the trust store untouched and the
rest of the command still works.

**Why this priority**: Below project creation because it is optional, but it is the single most
dangerous thing this phase can touch. Installing a certificate authority into a user's trust store
without consent is a security incident, not a convenience.

**Independent Test**: Run every command with local HTTPS selected and consent withheld, and assert
by inspection that the trust store is unchanged.

**Acceptance Scenarios**:

1. **Given** local HTTPS is selected, **When** an operation would modify the OS trust store,
   **Then** the person is told exactly what would change and asked to consent, and no modification
   occurs before consent is given.
2. **Given** consent is declined, **When** the command completes, **Then** the trust store is
   unchanged and the command reports what was skipped and how to do it later.
3. **Given** a non-interactive run, **When** a trust-store modification would be required, **Then**
   it does **not** happen implicitly — it requires an explicit flag whose name states what it does.
4. **Given** any command in this phase that was not asked to change trust, **When** it runs, **Then**
   the trust store is unchanged.

---

### Edge Cases

- **Destination exists and is empty** — distinct from "exists and is non-empty". Must be decided
  explicitly rather than falling into the non-empty refusal by accident.
- **Destination's parent does not exist** — refuse with a clear message, or create it, but not
  silently either way.
- **The process is killed** (not cancelled cleanly) part-way through rendering — a temporary
  directory may be left behind, and that residue must be identifiable as Renvor's and must not be
  inside the destination.
- **The temporary staging directory and the destination are on different filesystems**, so an
  atomic rename is impossible.
- **Disk fills** during rendering.
- **The destination becomes non-empty between validation and the final rename** — a
  time-of-check-to-time-of-use race with a real attacker.
- **A template expands to an enormous output** — bounded, not merely fast.
- **`stdout` is a pipe that closes early**, e.g. `renvor new --output json | head -1`.
- **A terminal that reports zero width**, or no terminal at all, or one that does not support
  colour.
- **The container runtime is installed but not running**, as distinct from not installed.
- **Two `renvor new` runs target the same destination concurrently.**

## Requirements *(mandatory)*

### Functional Requirements

**The executable and its surface**

- **FR-001**: The project MUST install an executable named exactly `renvor`. The package name and
  the executable name are separate facts and both are normative.
- **FR-002**: Command names, flag names, exit codes, `stdout` versus `stderr` behaviour, `--help`
  text structure, JSON output shape, and cancellation semantics MUST be treated as public contracts
  and MUST be documented before implementation is considered complete.
- **FR-003**: Every command MUST use this exit-code taxonomy, and it MUST be documented as a public
  contract: **0** success, **1** unclassified or internal failure, **2** usage error, **3**
  validation failure, **4** cancelled by the operator, **5** environment failure. **1 is reserved**
  so that an unclassified failure is distinguishable from a classified one — an unclassified failure
  is a defect, and a taxonomy that absorbs it into a general error code hides that.
- **FR-004**: Human-readable progress and diagnostics MUST go to `stderr`. `stdout` MUST carry only
  the command's result.

**One configuration, two interfaces**

- **FR-005**: Every wizard question MUST have an equivalent non-interactive flag.
- **FR-005a**: The wizard MUST ask **only** the questions this phase can honour. A question whose
  answer the generator cannot act on MUST NOT be asked.
- **FR-005b**: Flags for choices belonging to later phases — transport, persistence model, database,
  auth starter, frontend, styling, render mode, desktop shell — MUST be **reserved**: accepted by the
  command grammar and rejected at validation with a message naming the choice and the phase that
  will support it. They MUST NOT be silently ignored, MUST NOT be reported as unknown flags, and
  MUST NOT be recorded in the generated manifest.
- **FR-006**: Both interfaces MUST produce a single validated configuration value. The
  specification MUST NOT be satisfiable by two independent code paths that happen to agree.
- **FR-007**: Validation MUST cover every individual choice and every cross-choice constraint, and
  MUST complete **before** any filesystem write.
- **FR-008**: An unsupported value or an unsupported combination MUST fail with a message that
  names the supported values or the conflicting choices. It MUST NOT be silently corrected.
- **FR-009**: The wizard MUST present a review screen listing the resolved selections, the paths
  that will be created, any warnings, and the exact equivalent non-interactive command, and MUST
  require confirmation unless confirmation was explicitly waived.
- **FR-010**: When `stdin` is not a terminal, the wizard MUST NOT prompt, MUST NOT block, and MUST
  NOT substitute defaults for answers that were not supplied.

**Transactional generation**

- **FR-011**: Rendering MUST occur in a uniquely named directory the process owns, created **inside
  the destination's parent directory** so that the final move is a same-filesystem rename by
  construction. The result MUST be moved into place only after the whole render has been verified.
  Staging in the system temporary directory is **forbidden**: on most Linux containers it is a
  different filesystem, which would make the non-atomic fallback that FR-016 prohibits the ordinary
  case rather than the exceptional one.
- **FR-012**: On cancellation or any failure, the destination MUST be left exactly as it was, and
  only the process's own temporary location MUST be removed.
- **FR-013**: A destination that **already exists MUST be refused**, in every form: an empty
  directory, a non-empty directory, a regular file, a symbolic link, and an entry whose state cannot
  be established. The refusal MUST happen before anything is written or staged, MUST carry a stable
  error code, and MUST name the rule in `details.rule`. Generation MUST NOT delete, rename, change
  the permissions of, replace, or restore any path the operator already has. Merging into an
  existing project is out of scope for this phase and MUST NOT be attempted.

  *Revised 2026-08-18 by maintainer ruling.* The previous wording refused only a destination that
  "exists and is not empty", which made an existing **empty** directory a legal target — and
  placement then deleted and recreated it, so the operator got a different inode with this process's
  mode and ownership (finding A-R8). A generator that can delete a directory is a different program
  from one that cannot, and this one cannot.
- **FR-014**: Generation MUST NOT overwrite, truncate, or delete any file it did not create.
- **FR-015**: Concurrent runs targeting the same destination MUST NOT interleave into a corrupt
  result; at most one MUST succeed and the other MUST fail cleanly.
- **FR-016**: If the destination cannot be produced atomically, the operation MUST fail with a
  message saying so rather than falling back to a non-atomic copy. **The limit MUST be stated rather
  than assumed away**: FR-011's staging makes the rename same-filesystem, and FR-013 requires the
  destination to be absent, but the atomicity of renaming a directory onto a
  non-existent path is a platform property and MUST be documented per platform rather than claimed
  uniformly.

**The project manifest**

- **FR-017**: Generation MUST write a `renvor.toml` recording the resolved non-secret selections,
  the generator version, and the template version.
- **FR-018**: `renvor.toml` MUST NOT contain any password, token, private key, or database
  credential.
- **FR-019**: `renvor.toml` MUST be re-readable and validatable by `renvor check`, and an invalid
  manifest MUST produce a message naming the field and the constraint.

**Dry run and machine-readable output**

- **FR-020**: `--dry-run` MUST produce a complete manifest of the paths that would be created and
  MUST perform zero writes outside the process's own temporary space.
- **FR-021**: The `--dry-run` manifest MUST match the real run's created path set exactly for
  identical inputs.
- **FR-022**: `--output json` MUST emit exactly one JSON document on `stdout` for both success and
  failure, in a documented, versioned shape.
- **FR-023**: The JSON document MUST carry an integer `schemaVersion`, a `status`, and — on failure —
  an `error.code` drawn from a **documented registry of stable codes**. The shape and the registry
  are compatibility contracts: a code MUST NOT be reused for a different meaning, and a consumer
  MUST be able to detect a shape change from `schemaVersion` alone.

**Templates**

- **FR-024**: Templates MUST be versioned, and the version MUST be recorded in the generated
  manifest.
- **FR-025**: Generation MUST NOT download an executable template from an unverified location.
- **FR-026**: Template rendering MUST be bounded — bounded input size, bounded expansion, and a
  bounded number of output files — so that a hostile or mistaken template cannot exhaust memory,
  disk, or time.
- **FR-027**: A template MUST NOT be able to write outside the destination, read arbitrary files,
  or perform network access.
- **FR-028**: An undefined variable in a template MUST be an error, not a silently empty rendering.

**Generated project quality**

- **FR-029**: The generated API-only skeleton MUST pass formatting, linting, building, and testing
  with the project's own toolchain, and MUST start.
- **FR-030**: Generation MUST verify the project before reporting success, so that a project that
  does not build is a generation failure rather than a user's discovery.
- **FR-031**: The generated project MUST be reproducible from the generator version, the template
  version, and the manifest.

**Environment and local runtime**

- **FR-032**: `renvor doctor` MUST report what it checked, name any missing or incompatible
  prerequisite with the required and found versions, and give a corrective action.
- **FR-033**: `renvor check` MUST validate a project without building it and without modifying it.
- **FR-034**: `renvor dev` MUST run the local development loop and MUST surface failures rather
  than restarting silently.
- **FR-035**: Container commands MUST distinguish "runtime not installed" from "runtime installed
  but not running" and MUST fail closed with an actionable message in both cases, never hanging and
  never silently skipping.

**Local HTTPS**

- **FR-036**: **This phase performs no trust-store modification and issues no certificate.** What it
  delivers is the **consent boundary** and the **configuration surface**: the selection is recorded,
  the consent prompt and its non-interactive flag exist, and the operation they gate is declared
  unavailable until the phase that ships a transport. **This is a narrowing of `PLAN.md`'s "clean
  local HTTPS" deliverable and is recorded as one.** A certificate issued now would protect nothing,
  because nothing generated in this phase terminates TLS.
- **FR-037**: Any future modification of the operating system trust store MUST be preceded by a
  description of exactly what will change and MUST require explicit consent; a non-interactive run
  MUST require an explicit flag whose name states its effect. The boundary MUST be built now so that
  it is never built under pressure later.
- **FR-038**: Declining consent MUST leave the trust store unchanged and MUST NOT abort unrelated
  work. Because this phase modifies nothing, the testable form of this requirement is that **every
  command in the phase leaves the trust store byte-identical**, consent given or withheld.

**Security and boundedness**

- **FR-039**: Destination paths MUST be validated against path traversal, absolute-path injection,
  symlink escape, and platform-reserved names, and refused before any write.
- **FR-040**: **No archive is read in this phase.** Every template is embedded in the executable, so
  there is no local or remote archive path. This MUST be asserted structurally — the built executable
  MUST carry no archive-extraction capability — rather than tested as hardening against a code path
  that does not exist. If a later phase introduces an archive, zip-slip and decompression-amplification
  defences become that phase's requirement, and this requirement is its trigger.
- **FR-041**: Secret material MUST be redacted in every output mode — human output, JSON output,
  the dry-run manifest, error messages, and any diagnostic logging.
- **FR-042**: Every input, expansion, retry, and concurrent operation MUST be bounded, and the
  bound MUST be documented.
- **FR-043**: No command in this phase MUST require network access to complete its local flows,
  and this MUST be demonstrated with networking unavailable rather than asserted.

**Governance**

- **FR-044**: Every external dependency added by this phase MUST carry a recorded evaluation of its
  version, licence, maintenance status, MSRV compatibility, known advisories, and feature cost.
- **FR-045**: Any custom infrastructure chosen in preference to a maintained package MUST be
  justified by an accepted decision record naming the packages evaluated and their concrete
  shortcomings.
- **FR-046**: The phase MUST NOT claim an independent review it has not had, and MUST record the
  human gate that remains open rather than treating it as waivable by default.

### Key Entities

- **Project configuration** — the resolved, validated set of answers. Produced identically by the
  wizard and by flags. The single input to generation.
- **Template set** — the versioned collection of files rendered into a project, with the metadata
  needed to record which version produced a given project.
- **File manifest** — the ordered list of paths a run would create or did create, with content
  identity, used for `--dry-run`, for verification before the final move, and for reproducibility.
- **Project manifest (`renvor.toml`)** — the record written into the generated project: non-secret
  selections plus generator and template versions.
- **Environment report** — what `renvor doctor` produces: each prerequisite, its requirement, what
  was found, and the corrective action where they differ.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Cancelling at any wizard prompt leaves **0** entries at the destination. Verified at
  every prompt, not a sample.
- **SC-002**: An injected failure at any rendering step leaves **0** modifications to a
  pre-existing destination, byte-compared before and after.
- **SC-003**: A prompt-driven run and a flag-driven run with equivalent answers produce
  **byte-identical** project manifests and **identical** file manifests.
- **SC-004**: **100%** of unsupported values and unsupported combinations are rejected with **0**
  filesystem writes, **and 100% of reserved later-phase flags are rejected with a message naming
  the phase that will support them** — never as an unknown flag and never silently ignored.
- **SC-005**: The generated skeleton passes formatting, linting, build, and test with **0**
  warnings escalated to errors, and starts successfully.
- **SC-006**: `--dry-run` performs **0** writes at the destination, and its manifest matches the
  real run's created path set with **0** differences.
- **SC-007**: `--output json` emits exactly **1** JSON document on `stdout` in **100%** of runs,
  success and failure alike, with **0** human-readable text on `stdout`.
- **SC-008**: **0** secrets appear in any output mode, verified against a corpus of secret-shaped
  inputs across human output, JSON output, dry-run manifests, and error messages.
- **SC-009**: **100%** of hostile destination and template inputs in the security corpus are
  refused before any write, with a positive control proving legitimate inputs still succeed. The
  corpus covers path traversal, absolute-path injection, symlink escape, and platform-reserved
  names. It contains **no archive cases**, because FR-040 removes the archive path entirely; that
  absence is asserted structurally instead.
- **SC-010**: **0** trust-store modifications occur, full stop, verified by comparing the trust
  store before and after **every** command in the phase, with consent both given and withheld. This
  is stronger than "none without consent" and is the correct assertion for a phase that ships no
  certificate issuance.
- **SC-011**: **0** network requests occur during local flows, demonstrated with networking
  unavailable.
- **SC-012**: With `stdin` not a terminal, **0** commands block for input, and **0** substitute a
  default for an unsupplied required answer.
- **SC-013**: Every bound in the phase has a documented value and a test that demonstrates the
  bound holds.
- **SC-014**: Verification passes on **both** the declared MSRV and current stable, on **every**
  platform the phase claims to support — and a platform not exercised in CI is not claimed.
- **SC-015**: Every dependency added by the phase appears in the recorded dependency inventory with
  **0** omissions, cross-checked against the resolved lockfile rather than the manifests.
- **SC-016**: Two generations from the same generator version, template version, and configuration
  produce **identical** file manifests.

## Assumptions

1. **The generated skeleton does not serve HTTP.** Phase 004 is the first transport. "Starts"
   therefore means the process initialises the kernel, reaches ready, and shuts down cleanly on
   request — not that it accepts a request.
2. **Refusing a non-empty destination is the decided policy**, per `PLAN.md` §9.2. A merge mode is
   explicitly future work and is not designed here.
3. **The declared MSRV and current stable both gate**, matching Phase 001 and Phase 002.
4. **Platform claims follow CI.** Phase 002 established that macOS and Windows are exercised; a
   platform-specific behaviour that CI does not run is recorded as unverified rather than claimed.
5. **`--yes` waives confirmation only**, never validation.
6. **Prompts for choices this phase cannot act on are not asked** (FR-005a), and their flags are
   reserved rather than removed (FR-005b), so a command line written today keeps its meaning when a
   later phase implements the choice.
7. **The independent-review gate remains open.** Advisory reviews are not independent, and this
   phase does not assume a waiver is available.
