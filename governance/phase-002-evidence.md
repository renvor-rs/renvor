# Phase 002 — Evidence Ledger

**Feature**: [`specs/002-core-kernel`](../specs/002-core-kernel/spec.md)
**Toolchain**: 1.94.0 | **Source of truth**: the live workspace and the tracked `Cargo.lock`

## Status

**Implementation is complete, conditional on integration into `main`.** The branch
`feat/phase-002-core-kernel` is **pushed and open as pull request #19**.
`specs/002-core-kernel/tasks.md` is authoritative for which tasks are checked, and this ledger must
never claim more than it does.

**This paragraph deliberately no longer names the open set.** It named it three times and was wrong
twice — the third occurrence was found by the W-005 requirements delta review (D6-1), which
observed this sentence naming *T140 and T141* while `tasks.md`, declared authoritative in the same
sentence, marked *T128, T132, T141*. A summary of a moving boundary drifts every time the boundary
moves, and the fix that keeps failing is remembering to update it. Read `tasks.md`.

> **Corrected a third time, 2026-08-16 (T139).** Reviewing the open pull request found a second
> layer of defects, recorded in [Post-review corrections](#post-review-corrections-t133t142). Six
> tasks that had been checked were **not** complete on their own terms — most sharply T128, whose
> W-005 reviews genuinely ran but whose findings were not all individually dispositioned. The
> boundary in this paragraph has now moved three times and been wrong at two of them, which is
> itself the argument for the rule that `tasks.md` is authoritative and this sentence is not.

> **Corrected twice, in opposite directions, and the second correction is the more interesting
> one.** The W-005 requirements review (Q6-1) caught this paragraph claiming T001–T132 complete
> while T127–T132 were not done. It was rewritten to claim T001–T126 with the rest "in progress" —
> accurate when written, and then **two later commits moved reality past it without revisiting
> it** (`2cd0530` checked T131, `bf70553` checked T132). The verification re-review (N1) caught the
> stale version, now *under*-claiming.
>
> The same drift mechanism produced both. A status line that names a moving boundary has to be
> re-read every time the boundary moves, and twice it was not.

> **Corrected 2026-08-16, on finding Q6-1 of the W-005 requirements review.** This paragraph
> previously read *"Every task T001–T132 has been executed and verified."* That was false when
> written: T132 is the push and pull request, which had not happened and provably could not have —
> the branch had no upstream and no remote ref existed — and T128 is the very review that caught
> it, which was being written at the time. The claim was **prospective**, stated as though it were
> observed, and it was the first and most load-bearing sentence in the document. A ledger whose
> opening completion claim runs ahead of its own evidence is the exact failure this ledger's first
> paragraph says it exists to avoid.

Three things this ledger is **not** claiming, stated here so no reader has to infer them:

1. **The branch is not integrated.** It is unmerged. Nothing in this document should be read as a
   statement about `main`.
2. **No independent review has occurred.** Phase 002 closes under **W-005**, which waives *who
   reviews* and waives nothing about *what must be true*. Every review recorded here is
   **NON-INDEPENDENT and ADVISORY**, and saying otherwise anywhere would be false.
3. **Nothing is published.** 0 crates, 0 tags, 0 releases.

A pre-shipping audit on **2026-08-16** found defects that the 11-step verification sequence does
not cover. They are recorded in [Pre-shipping corrections](#pre-shipping-corrections-t111t132)
below, and T001–T110 must be read as *complete-as-specified*, not as *audited-and-shipped*.

A second audit the same day, this time of the **open pull request**, found a further layer:
[Post-review corrections](#post-review-corrections-t133t142). Six of Phase 10's own corrections
were incomplete, and two MAJOR security findings on public API were still live behind a grouped
disposition. T111–T132 must be read the same way T001–T110 is.

This ledger records what was **executed and observed**, not what was intended. Where something was
not done, it says so; where something failed, it says that too. A ledger that only records
successes is a marketing document.

## Gate outcomes

| Gate | Tasks | Outcome | Evidence |
|---|---|---|---|
| Configuration compatibility proof (8 obligations) | T014–T020 | **FAILED — 4 of 8 met.** Obligations 1, 4, 6, 7 unmet; obligation 4 unrecoverable. `serde` + `toml` fallback **triggered** | `crates/renvor-config/tests/proof_gate.rs` (10 tests), `examples/env_probe.rs`, research §D6 |
| Provider-resolver feasibility and counters | T021–T025 | **PASSED — 8 of 8 met.** Counters exactly 2048 / 8192 / 10240 against allowances 2048 / 16384 / 18432. Iterative-SCC fallback **not** triggered | `crates/renvor-core/tests/resolver_proof.rs` (13 tests), research §D8 |
| ADR-0007 governance gate | T026–T029 | **PASSED — accepted under W-004** after 8 advisory findings, all dispositioned. Two changed the record | `decisions/0007-phase-002-custom-kernel-primitives.md` |
| Complete resolved transitive dependency inventory | T030–T034, T122 | **PASSED.** **48** external packages (45 production, 3 dev-only); 0 without a licence, 0 over MSRV, `cargo deny` clean on all four checks. *55 was the pre-`confique` figure and is corrected here* | `governance/phase-002-dependency-inventory.md` |

**The two proof gates disagreed, and that is them working.** One failed on evidence the research had
predicted; the other passed and reversed a prior decision to build. Only the failing one
contributed custom infrastructure.

## W-004 — compensating controls for ADR-0007

W-004 waives the **independent-review requirement** for ADR-0007 and for nothing else. It waives
nothing about what must be true.

### The four counted controls

| # | Control | Status | Evidence |
|---|---|---|---|
| 1 | Configuration proof gate **and** resolver counter proof completed and recorded **before** ADR acceptance | **PASSED** | Both recorded in research §D6/§D8 before the record was drafted. The record's scope was set by their outcomes, not by prediction |
| 2 | Two clean-context advisory reviews — one architecture, one security — each producing a recorded result | **PASSED on attempt 2**; attempt 1 recorded as **NOT PERFORMED** | See "Advisory review attempts" below |
| 3 | Every finding individually dispositioned | **PASSED** | 8 findings, 8 dispositions, in ADR-0007's review record. 2 were MAJOR and both changed the record materially |
| 4 | No custom infrastructure merges until controls 1–3 are recorded | **HOLDS** | T041 unstarted and blocked; the configuration adapter unimplemented; the resolver storage's FR-035 justification now on the record |

### The three restated preconditions

Restated for completeness and **deliberately not counted** — a control another rule already mandates
unconditionally compensates for nothing.

| Precondition | Already required by | Status |
|---|---|---|
| Alternatives-and-consequences analysis | FR-035, principle III | **HOLDS** — 7 rejected alternatives with reasons; costs stated, including three disclosure surfaces the record initially missed |
| Package-first evaluation of every custom primitive | FR-035, principle III | **HOLDS** — 8 packages across 3 primitives: 2 gate-executed, 6 by documentation and source |
| CI, dependency, licence, advisory, secret-scanning, code-quality gates running unconditionally on every PR | Phase 001 | **HOLDS** — `ci.yml` (gitleaks 8.30.1, `cargo xtask verify`), `security.yml` (cargo-deny, clippy `-D warnings`, dependency-review), both on `pull_request` |

### Advisory review attempts

**Attempt 1 — NOT PERFORMED.** Two agents were dispatched with distinct written checklists. **Both
went idle without returning findings and without returning a "no findings" statement.** W-004
control 2 requires an empty result to be recorded as not performed, never as passed, and it is so
recorded here.

This is the **second** occurrence of this failure mode in the project. The clause exists because it
happened once before, and it has now caught it a second time — which is the clause doing its job
rather than a reason to weaken it.

**Diagnosis**: long prompts spanning many files, with delivery depending on the agent's final
message surviving to the caller.

**Remedy**: the retry narrowed each checklist to seven numbered questions and made the deliverable a
**file written to disk** rather than a returned message, removing message delivery from the
critical path. Both retried reviews delivered on the first attempt after that change.

**Attempt 2 — both performed.** Architecture: 1 MAJOR, 3 MINOR. Security: 1 MAJOR, 3 MINOR. All
eight dispositioned in ADR-0007.

**Neither review is independent.** Both are agent reviews operating under a recorded exception, and
must never be described as independent in any document.

### What the reviews actually caught

Recorded because a review process that never changes anything is theatre, and the counter-evidence
is worth keeping:

- **The FR-035 scope was wrong.** The record excluded the provider-graph adapter on an unsourced
  "~300 lines" figure. The file is **648 lines**, of which ~101 are trait conformance — and
  `petgraph::csr::Csr` exists and implements all three required traits, which the draft never
  mentioned. A third primitive was added as a result.
- **A claim was written in the wrong tense.** The record said the eight-obligation gate "becomes the
  adapter's own acceptance suite". `renvor-config` is a stub and every `obligation_*` test exercises
  the **rejected** candidate. Corrected, and carried as open item 1 below.
- **Three disclosure surfaces had never been considered** anywhere in the phase: attribution
  reporting as a channel, type names as metadata (including const generics rendered verbatim), and
  `confique-macro` as build-time code execution rather than build time.

## User Story 1 — what implementing it changed

**Status**: T042–T056 complete. Workspace tests **72 → 122**, all passing. `cargo xtask verify`
**9 of 10** before commit; step 10 is the working-tree cleanliness check, which passes once the
work is committed.

### Three defects found by writing the code, not by reading it

| # | Defect | How it surfaced | Fix |
|---|---|---|---|
| 1 | `EntropySource` is a **public, fallible trait that nobody outside its module could implement** — `EntropyUnavailable` had a private field and no constructor | Writing a test double that reports entropy failure. `FixedEntropy` cycles its bytes and always succeeds, so it cannot play that part | `EntropyUnavailable::new` made public. The fallible half of the trait — the half C-E4 is about — is now exercisable |
| 2 | `CancelScope` required `&'static str` names | Provider names are registration-time runtime values, so no provider could have a named scope | Names are `Arc<str>`; cheap to clone, and a scope is cloned once per child |
| 3 | The facade's own "no implementation" scan was substring-based and **missed indented declarations** | Its **positive control failed** — the scan could not find the declarations in its own test module | Rewritten line-oriented. The weaker version would have passed while checking nothing nested |

Defect 3 is the one worth keeping: a check written to prove an absence was itself unable to detect
a presence, and only the control said so.

### Two decisions taken on measured evidence

**`ErrorCategory::CapabilityDuplicate` — the taxonomy grew from 13 to 14 rows.** Two providers
offering the same `CapabilityId` is reachable, and no earlier artifact covered it. Picking a winner
is a silent fallback (FR-022), panicking breaks SC-004, `DependencyMissing` would print a false
statement, and `Internal` would blame Renvor for an author's mistake. Recorded as
`contracts/error-taxonomy.md` **revision 1.1.0** with the rejected alternatives.

**`BuildError` has two variants rather than widening `Internal`.** An operating system refusing to
supply random bytes is an environment failure, not a Renvor defect. Reporting it as `Internal`
would have been the smaller diff and would have told authors their framework was broken when their
sandbox was. `BuildError::category()` returns `None` for that variant on purpose, and a test
asserts a kernel failure *does* return one so the `None` discriminates.

### Measured, not asserted

A 3-provider / 3-edge graph consumes **exactly** 6 provider examinations, 3 edge examinations, and
9 work units — 2 per provider and 1 per edge, the same constants the maximum-size proof gate
measured at 2048 / 8192 / 10240. The integration test asserts **equalities, not bounds**: a
loosened `<=` would keep passing if the traversal started doing twice the work.

## User Story 2 — the requirement that could not be met by writing a test

**Status**: T057–T064 complete. Workspace tests **122 → 148**, all passing.

**T064 asks for an assertion that was false when it was written.** SC-015 requires **0** unbounded
waits in kernel-owned paths, and the kernel had **two**: a provider that never returned from
`initialise` hung `Boot` for ever, and one that never returned from `stop` hung shutdown. Neither
was reachable through any existing test, because every test provider answered.

Cancellation does not close this. A [`CancelScope`] asks a provider to stop; honouring it is the
provider's choice, and C-L9's `Hang` behaviour exists precisely to model a provider that does not.
So `Boot` and rollback now bound each provider call with `tokio::time::timeout`, and a breach is
reported as `DeadlineExceeded` rather than as a provider failure — **not answering and refusing are
different faults, and they lead to different investigations**.

### The bound's value is Renvor's choice, and is recorded as such

FR-042 fixes **30 seconds** for the *drain* budget. It says nothing about per-provider waits. FR-025
and C-L7 require the bound to exist but name no number, so one had to be chosen:
`DEFAULT_PROVIDER_DEADLINE` matches the drain default as a deliberate symmetry, **not** as a
measured figure, and is author-overridable. The constant's own documentation says so, rather than
letting a reader assume the specification supplied it. Carried as open item 7.

### What the deadline tests prove, and what they cannot

A test cannot enumerate every future the kernel might await. It can close the set. The kernel awaits
foreign code in exactly **three** places — provider initialise, provider stop, and the drain — and
`tests/deadlines.rs` bounds each with a behaviour test **and** reads the kernel's own source to fail
if either provider call loses its `timeout` wrapper.

That source check earns its place because of an asymmetry: **a behaviour test for a removed bound
does not fail — it hangs**, and a hung test on CI reads as a slow machine rather than as a defect.

### Zero-budget drain has no fast path

FR-042 requires a zero budget with work in flight to report that work as outstanding *exactly as a
timed-out drain would*. `WorkGate::drain` therefore has **no** `if budget.is_zero()` branch — zero
flows through the same `timeout` as the 30-second default. The one early return is "there is nothing
to wait for", which is true at every budget. A dedicated zero branch would have been the obvious
implementation and one edit away from returning `Clean` for the exact case the requirement exists to
prevent.

Both directions are asserted: zero **with** work reports `Incomplete`, zero **without** work reports
`Clean`. The second is the control — without it, an implementation that always reported `Incomplete`
for a zero budget would pass.

### FR-009 is satisfied more strongly than it asks

FR-009 requires shutdown before `Ready` to roll back whatever was initialised. In this design an
application that is *initialised but not `Ready`* **cannot be observed at all**: `Application::boot`
consumes `self`, so a failed or interrupted boot returns no application. The interrupted case rolls
back inside `boot` and is proven in `tests/lifecycle_edges.rs`; the never-booted case shuts down with
0 providers stopped and a phase record that does **not** claim it passed through `Boot` and `Ready`.

## User Story 3 — the adapter, and the gate that now points at it

**Status**: T065–T074 complete. Workspace tests **148 → 207**, all passing.

### Open item 1 is closed

The eight-obligation gate no longer exercises `confique`, the crate the project had already
rejected. `crates/renvor-config/tests/proof_gate.rs` runs the same obligations, against the same
T015 fixtures, against **the Renvor adapter** — and passes. Obligations 4, 6, and 7 are now
*demonstrated* rather than *designed for*.

`confique` and its child-process probe were **deleted**, as its manifest comment pre-committed:
*"if the gate fails, it is deleted rather than demoted."* Seven dev-only packages left the resolved
graph with it, including two duplicate major versions of `toml` and `winnow`. The inventory is
revised to **48** external packages.

### The pre-committed re-examination was performed

Research §D6's fallback clause named `schematic` 0.19.7 as *"the first candidate to re-examine if
the fallback triggers, **before writing the adapter**"*. It was re-examined by reading the crate's
source, and the result **corrects part of the earlier record**:

- `schematic`'s precedence order is **right** — defaults, then merged layers, then environment —
  where `confique`'s is inverted. The impression that every candidate got ordering wrong was wrong.
- Its `ConfigLoadResult` exposes per-layer partials **with their sources**, a seam attribution
  could be rebuilt from. `confique` has no such seam.

It is still rejected, on a clause neither earlier evaluation had tested it against: **C-C4 requires
the environment to be a layer with a precedence position, not a per-field opt-in annotation**, and
`schematic` reads environment values only for settings marked `#[setting(env = "…")]`. A field
nobody remembered to annotate is silently unreachable from the environment. Full record in
research §D6.

### What running the gate against the adapter revealed

**Obligation 7 could not pass as written, and the reason is a feature.** The `shape_conflict.toml`
fixture makes `server` a table in one file and a scalar in another. Against the adapter that never
reaches the merge: decode-per-source rejects the scalar in step 1, because it does not fit the
declared type. The obligation asks for a diagnostic naming **both** layers; what arrives names the
key, **the one layer at fault**, and the expected type.

That is stricter, not weaker — the author is told which file is wrong rather than that two files
differ. The obligation is therefore proven along **both** reachable paths:

| Path | When it fires | What it names |
|---|---|---|
| **7a** — per-source decode | the key is in the declared schema | key, the offending layer, expected type |
| **7b** — merge conflict | the schema does not constrain the key | key and **both** layers, with both shapes |

7b is what "naming both layers" actually applies to, and it is where the merge-level check earns
its place. This split was found by running the gate, not by reading the contract.

### Three more things the implementation surfaced

| # | Finding | Resolution |
|---|---|---|
| 1 | `ConfigSource::layer()` assumed every `Load` participant maps to **one** layer. The first real implementation spans defaults, two files, and the environment | The method is now `name()`. A method that would have had to lie is worse than one that returns less |
| 2 | Deriving `Debug` on `SchemaSource` would have **printed the resolved configuration** — precisely what holds credentials. The compiler asked for `T: Debug` | Hand-written `Debug` emitting the name and a resolved/not flag. The right answer was not to add the bound but to stop printing the value, asserted with a positive control |
| 3 | `serde` gives no key path when deserializing a `toml::Table`; the usual fix is another dependency | The whole-source decode is tried first, and **only on failure** does a bisection narrow to the smallest failing sub-tree, producing a dotted path. Cost is paid once, on a path already returning an error |

### Property testing without a new dependency

`cargo-fuzz` needs nightly, which the fixed 1.94.0 floor rules out, and `proptest`/`arbitrary`
would each be a new package in a recorded-gate inventory. The generator in
`crates/renvor-config/tests/hostile.rs` is `SplitMix64` written out — eleven lines, **no**
dependency, fixed seed, no clock — so a failing case is reproducible by index on any machine.

**The trade is real and recorded**: this explores far less of the input space than a
coverage-guided fuzzer would. It is entered as open item 8 rather than presented as equivalent.

The generator carries its own positive control: it must produce **some** documents the stack
accepts and **not all** of them, or one of the two paths it claims to exercise is untested.

## User Story 4 — a real leak, found in code written the same day

**Status**: T075–T083 complete. Workspace tests **207 → 228**, all passing.

### The defect

`serde` **quotes the offending value in its error message**. Measured, not assumed:

```text
deserializing  port = "hunter2-do-not-print"  into a u16 reports:
    invalid type: string "hunter2-do-not-print", expected u16
```

The configuration adapter written earlier this phase forwarded that message straight into
`KernelError::Configuration`'s `constraint` field — which meant **a secret configuration value
reached every output form of the error**. C-E3 allows **0**.

The error type had been designed so that *no field can hold a configuration value*, and that was
recorded as redaction "enforced by construction". **It was not enough, and the earlier claim was
too strong**: a field cannot hold a value, but a *message* can, and the adapter supplied the
message. The gap existed for the length of three commits.

### How it is closed — two mechanisms, because either alone is a rule

| # | Mechanism | Why the other is insufficient alone |
|---|---|---|
| 1 | `Constraint` **cannot carry a value**: every variant holds shapes, bounds, or a `&'static str`. The single `String`-carrying variant is reachable only through `Constraint::from_decoder`, which strips the value | Without mechanism 2, an adapter could keep formatting its own string and the type would have no say |
| 2 | `KernelError::Configuration` is **`#[non_exhaustive]`**, so no crate outside `renvor-core` can build it with a struct literal | Without mechanism 1, the mandatory constructor would still accept an arbitrary `String` |

`from_decoder` **fails closed**: a message matching neither the key-only shapes nor
`", expected …"` is **discarded entirely** and replaced with the structural shapes. A decoder
message nobody anticipated loses information rather than leaking.

The fix touched **seven** construction sites across `renvor-config`, and the compiler found every
one of them — which is the difference between mechanism 2 and a code-review note.

### The control that would have caught it, and now does

`crates/renvor-config/tests/redaction.rs` runs a redacting type and a **deliberately leaking**
one through the identical harness. The leaking control must leak through **every** form, or the
harness is not exercising them all. Two further tests come at it from the other end: one asserts a
decoder message quoting the credential produces a clean error, and one drives the **whole adapter**
with a secret-shaped environment value and asserts nothing leaks.

### `tracing::Value` is sealed, and the honest answer is not an impl

C-C9 assigns the structured-field path to Renvor, and the obvious move is
`impl tracing::Value for Secret<T>`. **That trait is sealed** — Renvor cannot implement it.

What secures the path instead is that **every route into a tracing field bottoms out in `Display`,
`Debug`, or a primitive** (`%value`, `?value`, `field::display`, `field::debug`), and all three of
this type's routes render the placeholder. There is no fourth route, so there is nothing left for
an impl to protect. `Secret::redacted_field` exists for explicitness at the call site, not as a
substitute for an impl that cannot exist.

### Two smaller things

- **A self-reading test found its own needle.** The serialization test searched its own source for
  `impl SerializableSecret` and matched the assertion's own text. The needles are now assembled
  from fragments so the literal never appears in the file. Every other source-scanning test in this
  phase scans only the pre-`#[cfg(test)]` half and was never exposed to this.
- **`Constraint::TooLarge` carries bytes, not characters.** Reusing `TooLong { maximum }` for a
  file-size ceiling would have reported a byte count in a message that says "characters" — a number
  that does not mean what it says.

## User Story 5 — health and readiness, proven by disagreement

**Status**: T084–T088 complete. Workspace tests **228 → 240**, all passing.

FR-026 is easy to satisfy on paper — expose two methods, derive one from the other, and never write
a test that could tell. So independence is asserted by **driving the two answers apart in both
directions**:

| Case | Liveness | Readiness | Why it matters |
|---|---|---|---|
| Draining (FR-027) | **Alive** | **NotReady** | A drain reported as not-alive invites a restart in the middle of the shutdown it is completing |
| Liveness set to `Dead` | **Dead** | **Ready** | Operationally nonsensical, and **reachable** — which is the proof that nothing in the readiness path reads liveness |

The second row is the one that catches a derived implementation: deriving readiness from liveness
would still permit the first row.

**A panicking contributor is caught, named, and does not take the process down.** Both obvious
implementations fail FR-028: letting the panic propagate makes the health check the outage, and
catching it to report the whole set as not-ready tells an operator nothing about *which* of twelve
checks broke. `ContributorVerdict` carries `panicked` as a **separate field** rather than a special
`Readiness` variant, because "this check is broken" and "this check says no" call for different
responses and folding them would make a defect indistinguishable from a working negative answer.

`catch_unwind` requires no `unsafe`, which mattered: this workspace declares
`unsafe_code = "forbid"`, so an approach needing it would not have been available.

## User Story 6 — the harness, and two gaps it found in the kernel

**Status**: T089–T095 complete. Workspace tests **240 → 250**, all passing, plus three runnable
examples.

SC-009 asks for failure injectable at **7 of 7** phases. That holds, and the coverage test takes
its phase list from `LifecyclePhase::ALL` so an eighth phase fails the file rather than quietly
going uncovered. Each phase's *characteristic* failure is asserted, because coverage without
outcomes would pass on a harness that injected nothing.

Building it surfaced **two gaps in the kernel**, and neither is worked around:

### Gap 1 — `Load` and `Validate` are unbounded kernel-owned paths

Both call author-supplied code **synchronously**, and `ApplicationBuilder::build` is not `async`,
so there is no deadline around either. A configuration source reading a hung network mount blocks
the process indefinitely.

**This corrects a claim made earlier this phase.** `tests/deadlines.rs` enumerates "three
kernel-owned waits" and bounds each; it missed these two because it searched for `.await`, and a
synchronous call that never returns is not an await. That test is correct about what it checked and
**incomplete about the set it claimed to close**. SC-015's "0 unbounded waits" therefore does **not**
hold today. Open item 12.

### Gap 2 — a panicking provider is not contained

C-L9 requires `Panic` injectable at every phase. The kernel does not contain a panicking provider:
catching a panic across an `await` needs either a `'static` future — ruled out because
`InitContext` borrows the state map — or `futures::FutureExt::catch_unwind`, a new dependency in a
phase whose inventory is a recorded gate. Readiness contributors **are** contained, because they
are synchronous and `catch_unwind` suffices there.

`Harness::run` **refuses** a `Panic` point at `Boot` or `Stop` with a diagnostic naming the gap and
pointing at this file. A test asserts that refusal, so **closing the gap fails the test** and forces
whoever closes it to update the claim. Open item 13.

### A third thing, smaller

`tokio::time::pause` panics **both ways** — outside a `current_thread` runtime, and when the clock
is *already* paused, with the message `time is already frozen`. Since `start_paused = true` is the
idiom used throughout this workspace, a clock helper that called `pause` itself panicked in the
common case. Measured, not assumed: the first version of `TestClock` did exactly that and its own
tests failed. `TestClock::new` now attaches to an already-paused clock and `TestClock::pausing`
covers the other case.

## Phase 9 — polish, and the verification sequence itself

**Status**: T096–T105 and T107–T110 complete; **T106 partially** (see below). Workspace tests
**250 → 262**.

### Observability: the kernel installs nothing, proven by installing something afterwards

You cannot ask `tracing` whether a global subscriber is installed, so the proof is indirect and
stronger than a query: `tests/observe_bootstrap.rs` runs a **complete lifecycle** — build, boot,
shutdown — and then **successfully claims the global slot**. The slot is single-claim per process,
so a successful claim afterwards proves the lifecycle never took it (C-O7, FR-029).

Its positive control immediately claims again and asserts `AlreadyInstalled`. Without that, a
`try_init_global` that ignored its argument and returned `Ok` would satisfy the first assertion.

`try_init_global` returns an error on a second call rather than panicking, silently succeeding, or
silently replacing. FR-029 permits **0** of those three, and each fails the same way — the caller
believes something about their subscriber that is not true.

### Two more self-reading tests caught themselves

- The span module's "0 interpolated strings" scan matched **its own prose**, which said *there is
  no `format!` in this module*. Comment lines are now stripped before scanning. That is the second
  time this phase has walked into the trap; the earlier instance was in the secret module.
- The span field test asserted against `Span::metadata()`, which returns `None` when **no
  subscriber is listening** — so it was inspecting a disabled span and would have passed for the
  wrong reason. Replaced with a hand-written recording `Subscriber` (~40 lines, no
  `tracing-subscriber` in `renvor-core`) that asserts what a subscriber **actually receives**.

### The run identifier is generated first, and that changed the build order

FR-043 requires **every** emitted record to carry the run identifier, and the first phase span is
emitted by `PhaseCursor::start`. Generating the identifier after `Load`, as the first version did,
would have left the records a **startup failure** produces unattributed — which are the ones anybody
actually reads. It is now the first thing `build` does.

### The verification sequence grew from 10 steps to 11

New **step 7, architecture invariants**, carrying T101, T102, and T104. Each check has a positive
control, because each asserts an absence:

| Check | Claim | Control |
|---|---|---|
| Crate DAG (T101) | `renvor-core` resolves **0** of `serde`, `toml`, `secrecy`; nothing depends on `renvor-testkit` | the same query finds `petgraph` and `tokio`, and the facade **does** resolve `renvor-core` |
| Facade isolation (T102) | `renvor --no-default-features` resolves **0** configuration crates | the same query **with** default features **does** resolve all three — measured: `secrecy 0.10.3`, `serde 1.0.229`, `toml 1.1.4` |
| SC-022 wording (T104) | the closure sentence is byte-identical in **3** locations; **0** phase numbers in FR-036's normative clause | the extracted clause is asserted to contain both conditions, so "no phase number" cannot mean "nothing was scanned" |

T104's check nearly asserted the wrong sentence. The byte-identical text is *"the first real
transport adapter has exercised the surface and its feedback has been applied"*, confirmed present
**3** times. FR-036 does mention Phase 004 — deliberately, in a parenthetical marked *"Roadmap
rationale, not part of the condition"* — so the phase-number check extracts the conditions
themselves rather than scanning the whole requirement, which would have failed on text that is
explicitly not part of the condition.

### T106 — the complete sequence on **both** toolchains

| Toolchain | What ran | Result | Skipped |
|---|---|---|---|
| **1.94.0** (pinned) | full `cargo xtask verify` | **11 of 11 passed** | **0** |
| **1.97.1** (current stable) | full `cargo xtask verify` | **11 of 11 passed** | **0** |

SC-013 asks for **0** failing and **0** *silently* skipped checks. Both runs completed every step;
nothing was skipped, so nothing needed naming.

**The propagation was verified rather than assumed.** `rust-toolchain.toml` pins 1.94.0, and
`xtask` spawns `cargo` as a child process — so a stable run could have silently fallen back to the
pin and produced a result indistinguishable from the pinned one. Probed directly:

```text
$ rustup run stable bash -c 'echo $RUSTUP_TOOLCHAIN; cargo --version; rustc --version'
stable-aarch64-apple-darwin
cargo 1.97.1 (c980f4866 2026-06-30)
rustc 1.97.1 (8bab26f4f 2026-07-14)
```

`RUSTUP_TOOLCHAIN` is set in the environment and takes precedence over `rust-toolchain.toml`, and
it is inherited by children. The stable run was genuinely stable. Without this probe the second row
of that table would have been an assumption wearing a result's clothes.

**Three toolchain versions ahead of the floor and clippy is still clean** — no new lint in 1.95,
1.96, or 1.97 fires on this code.

### T107 — quickstart gates

**Superseded by T125.** The table that stood here was wrong in three ways, and each was in the
direction that made coverage look better than it was:

1. It **collapsed gates 0–5** into "covered by `xtask verify` steps 1–6". The verification sequence
   and the quickstart gates are different checks with different assertions — Gate 3 runs the
   eight-obligation configuration proof and Gate 5 runs the 1024-node chain, neither of which any
   `xtask` step performs.
2. It **omitted gates 6 through 12 entirely** — drain, health, failure injection, run-identifier
   opacity, tracing ownership, hostile input, and examples. Seven gates, not recorded at all.
3. It **labelled Gate 15 "working-tree cleanliness"**, which is verification *step* 11. Gate 15 is
   the resolved-dependency inventory. The row therefore recorded a pass for a check that was never
   run, under the name of a different one.

Each of the sixteen gates has now been run individually. See
[T125 — every quickstart gate, run individually](#t125--every-quickstart-gate-run-individually).

### T108 — scope and authorization

- **FR-041**: this phase implements **no authentication, no authorization, and no identity**.
  Authorization impact is therefore **none**. Confirmed by review: `renvor-core` has no user, role,
  permission, token, or session type, and no code path consults one.
- **FR-033 / SC-010**: **0** runtime capabilities outside declared scope. What was checked, by name:
  no HTTP, no listener, no socket, no database, no CLI. `renvor-core`'s `tokio` features are
  `rt`, `time`, `sync`, `macros` — `net`, `fs`, `process`, `signal`, and `full` are all excluded, so
  the kernel **cannot** acquire a transport by accident. `renvor-config` reads the filesystem for
  TOML files and the environment, both of which FR-015 declares.

### T109 — Phase 001 deployment gates, still open and untouched

| Gate | Status |
|---|---|
| **`001-T102`** | non-completed, untouched by Phase 002 |
| **`001-T108`** | non-completed, untouched by Phase 002 |
| **`001-T109`** | non-completed, untouched by Phase 002 |
| **`001-T111`** | non-completed, untouched by Phase 002 |

The `001-` prefix is mandatory: Phase 002 has its own T105, and an unprefixed "T105" in this record
would read as this phase's rustdoc task.

### T110 — FR-034, and a defect in the check itself

**0 crates published, 0 tags, 0 releases.**

| Query | Result |
|---|---|
| `gh api repos/renvor-rs/renvor/tags` | **0** |
| `gh api repos/renvor-rs/renvor/releases` | **0** |
| `crates.io/api/v1/crates/{renvor, renvor-core, renvor-config, renvor-testkit}` | **404** — all four |

**The first run of this check reported HTTP 403 for all four crates**, which T110's own rule
("any status other than 200 or 404 is a failure") correctly classifies as a failure. The cause was
**a missing `User-Agent` header**, which crates.io requires — not publication status. Re-run with
one, all four return 404.

Worth recording because the check behaved exactly as designed: it refused to interpret an
unexpected status as a pass. A check that treated "not 200" as "not published" would have reported
success for the wrong reason.

## Open item 12 closed — `Load` and `Validate` are now bounded

**Status**: SC-015 holds. Workspace tests **262 → 267**.

### The fix is a thread, not an async signature

The obvious fix was to make `ApplicationBuilder::build` async and wrap each source call in
`tokio::time::timeout`. **That would not have worked.** A blocking call inside an async block never
yields, so the timer never gets to fire — bounding a *synchronous* call needs a second thread
whatever else changes, and making `build` async would have churned every call site **and still**
needed `spawn_blocking` underneath.

So each source call runs on its own thread and the kernel waits with
`std::sync::mpsc::Receiver::recv_timeout`. The signature is unchanged; nine call sites moved from
`Box<dyn ConfigSource>` to `Arc<dyn ConfigSource>`, because a worker thread needs an owned handle.

### The cost, stated rather than discovered

**A thread that never returns is leaked.** No Rust API can interrupt a blocked thread, so the
process keeps one stuck thread per hung source. That is strictly better than the alternative — the
*whole application* stuck — and it is worse than nothing, so it is recorded here rather than
omitted. Open item 15.

### An unplanned benefit: panic containment on these two phases

A source that **panics** kills only its own thread, dropping the channel sender. The wait ends with
`RecvTimeoutError::Disconnected` rather than an unwind through `build`, so a panicking
configuration source is a **failure** instead of a process abort.

That is C-L9's `Panic` behaviour, obtained as a property of the mechanism rather than as a second
feature — and it narrows open item 13, which now covers **providers only**. Configuration sources
and readiness contributors are both contained, because both are reached synchronously.

### The correction to my own earlier claim

`tests/deadlines.rs` enumerated **three** kernel-owned waits and called the set complete. It was
searching for `.await`, and **a synchronous call that never returns is not an await**. The set is
five:

| Kernel-owned wait | Kind | Bounded by |
|---|---|---|
| A configuration source loading | **synchronous** | the source deadline |
| A configuration source validating | **synchronous** | the source deadline |
| A provider initialising | async | the provider deadline |
| A provider stopping | async | the provider deadline |
| In-flight work draining | async | the drain budget |

**The shape of the search decided the shape of the answer.** A scan for `.await` can only ever find
awaits; reporting its result as "every kernel-owned wait" was a claim the scan could not support.
The file now checks the synchronous call sites too, and a new test **counts the bounding call
sites** so a sixth wait added later fails there rather than silently escaping the enumeration —
which is exactly how the set came to be wrong the first time.

## Open item 13 closed — provider panics are contained

**Status**: SC-009 is met in full — 7 of 7 phases **and** all three C-L9 behaviours. Workspace
tests **267 → 278**. **0 packages added.**

### The package-first comparison, and why the answer needed no package

| Approach | New packages | `unsafe` | Verdict |
|---|---|---|---|
| `futures::FutureExt::catch_unwind` | **`futures-util` and its tree** — confirmed absent from the lockfile, so a genuine addition | no | works, but pays a dependency for ~30 lines |
| `tokio::spawn` + `JoinError::is_panic` | none | no | **does not work** — needs a `'static` future, and `InitContext` borrows the state map. Forcing it would put the state map behind a `Mutex` and stop `InitContext::state` returning a borrow: **changing the provider API to buy a panic guard** |
| **Poll the boxed future inside `catch_unwind`** | **none** | **none** | chosen |

The third works because of a decision made much earlier for an unrelated reason. Wrapping an
arbitrary future normally needs to project `Pin<&mut Self>` to `Pin<&mut F>`, which requires
`unsafe` or a projection macro — and this workspace declares `unsafe_code = "forbid"`.

**`ProviderFuture` is already `Pin<Box<dyn Future>>`**, because the trait had to be dyn-compatible.
`Pin<Box<T>>` is `Unpin`, so a struct holding one is `Unpin`, so `Pin::get_mut` is safe and free.
The boxing that was the cost of avoiding `async-trait` is what makes containment free here.

### What was proven, and what was proven *not* to change

`catch_unwind` around a call site cannot catch a panic that happens on a **later poll**, so every
test panics **after an await** — the case the naive approach misses.

| Claim | Test |
|---|---|
| Panic at `Boot` fails the boot, naming the provider | `lifecycle_edges::a_panicking_provider_fails_the_boot_without_ending_the_process` |
| Panic at `Stop` does not strand the providers behind it (C-L4) | `lifecycle_edges::a_provider_that_panics_while_stopping_does_not_strand_the_rest` |
| **Attribution unweakened** — still `ProviderInit`/`ProviderStop`, and a caller can downcast the cause to `Panicked` to tell a panic from a returned error | both of the above |
| **Rollback unweakened** — what started is still stopped, in reverse actual order | both of the above |
| **Deadlines unweakened** — a hang is still a deadline, **not** reported as a panic | `lifecycle_edges::containing_a_panic_does_not_weaken_the_deadline` |
| All three behaviours injectable at `Boot` and `Stop` | `injection::all_three_behaviours_are_injectable_at_boot_and_stop` |

`contain` sits **inside** `timeout`, not outside. Reversed, a panicking provider would look like a
slow one, and the two failure modes call for different investigations.

### The gap tests did their job

`injection.rs` asserted the gap so that closing it would **fail the suite** and force the claim to
be updated. It did exactly that, and this section is the update. The alternative — omitting the
gap — would have left SC-009 quietly overstated.

### Dependency gates re-run

No direct dependency was added, and the gates were re-run rather than assumed:

| Gate | Result |
|---|---|
| `Cargo.lock` | **byte-identical** — 0 packages added, 0 removed; still **53** entries (48 external + 5 workspace) |
| `cargo deny check licenses advisories bans sources` | **all four ok** |
| MSRV 1.94.0 | the pinned build compiles and tests clean |
| Feature isolation | verify step 7, both directions, with controls |

## Named open items

| # | Item | Why it is open | Blocking? |
|---|---|---|---|
| 1 | ~~**T065** — re-point the eight-obligation gate at the Renvor adapter~~ | **CLOSED 2026-08-16.** The gate runs against the adapter and passes; obligations 4, 6, and 7 are demonstrated | — |
| 2 | **ADR-0008** remains `proposed` | W-004 covers ADR-0007 alone and confers no authority here. FR-035 does not require acceptance for a packaging decision | No |
| 3 | Independent re-review of ADR-0007 when W-004 closes | No qualified independent reviewer is available (research §D11 criteria 1, 2, 4) | Blocks W-004 closure |
| 4 | **W-005** — Phase 002 independent requirements-and-security review | Same staffing gap, phase level | Blocks public release |
| ~~5~~ | ~~`ConfigSource::load` and `validate` carry no value type~~ | **CLOSED 2026-08-16 (T126).** It was written to close with US3, and US3 shipped: typed decoding lives in `ConfigResolver`, and `SchemaSource` bridges it to the lifecycle by resolving during `load` and holding the value for a `ConfigHandle`. The port carries no value type *by design*, not pending a decision — the kernel never sees the author's schema, which is what keeps `renvor-core` free of a parser | — |
| ~~6~~ | ~~The facade's `config` re-export is **vacuous**~~ | **CLOSED 2026-08-16 (T126).** It was written when `renvor-config` was a documentation-only stub. The crate now exports `ConfigSchema`, `LayeredResolver`, `LayeredResolverBuilder`, `FileLayer`, `MAX_FILE_BYTES`, `SchemaSource`, `ConfigHandle`, `Secret`, `REDACTED`, `NESTING_SEPARATOR`, `DecodedLayer`, and `Merged`. The feature gate is load-bearing: `--no-default-features` resolves no parser, derive macro, or secret crate, asserted in both directions by `xtask` step 7 | — |
| 7 | `DEFAULT_PROVIDER_DEADLINE` is **30 s by Renvor's choice, not by specification** | FR-025 and C-L7 require the bound; no artifact names a value. Chosen to match the drain default as a symmetry, not from measurement | No — but a phase that measures real provider start-up times should revisit it |
| 8 | The TOML boundary's generated-input testing is a **hand-written deterministic generator**, not a coverage-guided fuzzer | `cargo-fuzz` needs nightly against a fixed 1.94.0 floor; `proptest`/`arbitrary` are new packages in a recorded-gate inventory. Explores far less of the space | No — but a phase with a nightly CI lane should add real fuzzing |
| 9 | An author writes a **second, all-optional struct** per schema | Decode-per-source needs an all-optional decode target and Renvor has no derive macro. A proc-macro of its own is custom infrastructure under FR-035 needing its own accepted record | No |
| 10 | `expected_type` is reported **inside the constraint text**, not as its own field, for file and environment layers | `KernelError::Configuration::expected_type` is `&'static str` so it cannot carry a value (C-E3); the adapter has no schema description to read a per-key type from. All three facts C-C3 requires are in the message | No |
| 11 | `MAX_FILE_BYTES` is **1 MiB by Renvor's choice** | C-C10 requires the bound; no artifact names a value. Overridable per file | No |
| ~~12~~ | ~~SC-015 does not hold~~ | **CLOSED**: both bounded by a worker thread and `recv_timeout`. The enumeration went from three to five here, and **from five to eight at T116** when `Register`, `Ready`, and entropy were found unbounded too — the counting test is what forced the second correction | — |
| ~~13~~ | ~~`Panic` not injectable for providers~~ | **CLOSED**: contained by polling the already-boxed provider future inside `catch_unwind` — 0 new packages, 0 `unsafe`. SC-009 met in full | — |
| 15 | A configuration source that never returns **leaks its worker thread** | No Rust API can interrupt a blocked thread. The kernel's *wait* is bounded, which is what FR-025 requires; the thread is not, and cannot be | No — but an application that hits it repeatedly will accumulate threads |
| ~~14~~ | ~~SC-013 partially met~~ | **CLOSED**: the full 11-step sequence ran on both 1.94.0 and 1.97.1, 0 skipped, with toolchain propagation verified by probe | — |
| 16 | `catch_unwind` contains only **unwinding** panics; under `panic = "abort"` there is **no** provider or contributor panic containment | A property of the panic strategy, not of Renvor. Nothing in the language can catch an abort. Renvor sets no `panic` profile, so the default `unwind` applies unless a consumer changes it. Also outside reach: a double panic, `process::abort`, and a stack overflow | No — but a consumer who sets `panic = "abort"` loses C-L9's `Panic` guarantee and must be told so |
| 17 | `DEFAULT_READINESS_DEADLINE` is **5 s by Renvor's choice, not by specification** | Same shape as items 7 and 11. FR-025 requires the bound; no artifact names a value. Chosen to sit under a typical container probe interval, so a hung contributor reports as hung rather than as a probe that never answered | No — a phase that measures real probe intervals should revisit it |
| 18 | A readiness probe now starts **one thread per contributor per call** | Bounding `ReadinessContributor::readiness` needs a thread, for the same reason `Load` does: a blocking call inside an async block never yields. An application probed once a second with twelve contributors creates twelve threads a second, all short-lived. Not measured under load | No — but a phase that adds a real probe endpoint should measure it |
| 20 | `Drain × Panic` and `Drain × Fail` reach the kernel **identically** | Renvor has no join handle for an author's task, so the drain cannot distinguish a task that panicked from one that returned. The pair proves the work permit is released **by unwinding**, which is a `Drop` property, not a kernel branch | No — but C-L9's 21 combinations are 21 *injections*, not 21 distinct kernel paths, and that distinction is now stated rather than implied |
| 23 | `MAX_OUTSTANDING_PROBES` has **no setter**, so an application whose contributors have all hung reports `NotAsked` until the process restarts | The application is genuinely broken at that point — 64 readiness workers have never returned — and reporting not-ready is correct. But an operator cannot raise or reset the budget | No |
| ~~22~~ | ~~**9 open CodeQL `rust/cleartext-logging` alerts** on PR #19, *"all nine false positives"*~~ | **CLOSED 2026-08-16 (T133, T136, T141). GitHub reports 0 open alerts on PR #19.** The original entry was wrong: only **three** were false positives. **#4–#9 were real defects and are fixed in source** — GitHub reports all six `fixed`. The three sanitiser demonstrations are **dismissed individually** as `false positive`, each with its own stated reason. They were #1, #2, #3 and are #1, **#10**, **#11**: changing the file re-fingerprinted #2 and #3, which GitHub closed as `fixed` and reissued at the same lines with byte-identical source | — |
| 21 | `MAX_KEY_DEPTH` (32), `MAX_OUTSTANDING_PROBES` (64), and `DEFAULT_READINESS_DEADLINE` (5 s) are **Renvor's numbers** | Same shape as items 7, 11, and 17. Each bound is required — by FR-025, or by the measured stack-overflow and thread-leak findings — and no artifact names a value | No — a phase with production measurements should revisit all six together |
| 19 | A hung readiness contributor **leaks its thread**, exactly as item 15 describes for configuration sources | Identical cause, identical impossibility: no Rust API can interrupt a blocked thread. The *wait* is bounded, and since the W-005 security review (finding 5.1) the *number of leaked threads* is bounded too, at `MAX_OUTSTANDING_PROBES` — but each leaked thread is permanent | No — the leak is capped rather than removed |
| 24 | The FIFO refusal is **check-then-open** and therefore racy | SV-N2's fix rejects a non-regular file on a `stat`, which does not open the path — so a FIFO present at check time never reaches the blocking `open`. An attacker who can **replace** the path between the `stat` and the `open` can still present one. Closing that needs `O_NONBLOCK` at open time, which means a direct `libc` dependency and a new row in the FR-040 inventory | No — under `ApplicationBuilder::build` even the residual is contained: `source.load()` runs inside `bounded_call` and is reported as a timeout. A caller using `FileLayer::read()` directly, on a path an attacker can write, is exposed |
| 25 | A configuration error message is **unbounded in length** | Round-1 finding 2.2 and SV-N3. A key is not a secret, so its content is safe to name — but a 1 MiB TOML key or a ~128 KiB environment variable name becomes an error string of that size in every log that catches the failure. SV-N1's own fix adds a call site with this property, since an over-deep key is long by construction | No — but a phase that adds structured logging should cap the rendered key |
| 26 | "0 examples require a transport" rests on a **denylist**, not a proof | RV-N4. Gate 13a checks that no transport, database, or CLI package name appears in the resolved graph. That is what is available without a capability model; it cannot prove a future package is not a transport | No |
| 27 | Gate 15's **15d has no positive control** | RV-N10. It is fail-closed through `pipefail` on its own pipeline, but nothing plants a row with an empty licence cell and requires the awk to catch it — the same shape as Q7-5, introduced by Q7-7's fix. T138 added three controls to 15f and none to 15d | No — but the file's own rule is that every zero-asserting check carries a control, and this one does not |
| 28 | `MAX_OUTSTANDING_PROBES` is **not re-exported** from `health`'s root | SV-N4. Callers reach through `health::contributor::`, and the crate's own integration test does. The matching defect in `renvor-config` was fixed at T139 — `MAX_KEY_DEPTH` now sits at the crate root — so this is the remaining half of the same inconsistency | No |
| 29 | Gate 12's repository-state check **cannot see an ignored path** | D5-1. `git status --porcelain` consults `.gitignore` — the same blind spot T120 removed from `release-dry-run.yml`, where the fix was a `find` comparison. Both of Gate 12's own probe files are git-visible, so the detector sees what it is aimed at | No — but the claim it supports is "no tracked or untracked-but-visible file changed", not "byte-identical" |
| 30 | The wait-inventory gate's comment strip is **not a comment parser** | D7-5. `split_once("//")` truncates at a `//` inside a string literal — one production line is truncated today, `observe/spans.rs:163`, with no live effect — and `/* … */` is not stripped at all, so the original false positive reproduces in block-comment form | No — the errors run in opposite directions (stricter for bounding constructs, laxer for author calls) and both are named in the code |
| 31 | A **regular file is now required** for a configuration source | S3-2. `/dev/null` as a deliberately-empty source, `/dev/stdin` when stdin is a pipe, and process substitution are all refused. The alternative — permitting character devices — readmits `/dev/zero`, which is the memory exhaustion finding 4.1 was raised for | No — but an operator who passed `/dev/stdin` before now gets a refusal, and the release note should say so |

## Pre-shipping corrections (T111–T132)

A **read-only pre-shipping audit** on 2026-08-16, run against the finished tree at `ef76f4e`, found
defects that T001–T110 and the 11-step verification sequence had not caught. Every one is recorded
here with what it was, why the existing checks missed it, and what now catches it.

**The pattern across all of them is the same**: a check that looked right and never executed the
thing it claimed to check. That is the class of defect worth naming, because each individual
instance reads as an oversight and the class reads as a method problem.

### A — runtime and compile correctness

| # | Defect | Why nothing caught it | Now caught by |
|---|---|---|---|
| T111 | `cargo check -p renvor --no-default-features --all-targets` **failed to compile**: the `configuration` example uses `renvor::config` with no `required-features` declaration | Step 7 asked `cargo tree` whether the lean graph resolved the config crates. **Resolving a graph is not compiling against it.** The tree query was green throughout | `lean_facade_compiles` in step 7, with a control that must **fail**: the example must not build without the feature, or the gate guards nothing |
| T112 | `std::env::vars()` **panics** on the first non-Unicode entry — including one set by an unrelated program. Measured: exit 101 | Every environment test supplied a `BTreeMap` through `with_environment_map`, because `set_var` is `unsafe` and the workspace forbids it. **Nothing ever read the real environment** | `read_process_environment` over `vars_os`, and `tests/environment_bytes.rs`, which re-executes the test binary with real non-Unicode variables attached |
| T113 | `std::thread::spawn` **panics** when the OS refuses a thread — in a module that deliberately leaks one thread per hung source | Thread exhaustion is a *consequence of this module's own design*, and nothing tested it because nothing could: making the OS refuse needs `libc` and an `unsafe` `setrlimit` | `Builder::spawn`, `KernelError::ResourceUnavailable`, and a spawner **seam** so the refusal path is reachable from a test |
| T115 | The wait-inventory gate read **three hardcoded files** and searched for `.await` | It had already closed the wrong set once, for the same reason, and the fix kept the shape that caused it. **The shape of the search decided the shape of the answer**, twice | A gate that **discovers** every file under `src/` and flags any reaching author code without a bound. Mutation-tested: a planted unbounded file fails it |
| T116 | **10 of C-L9's 21 combinations did not execute.** `Load`, `Validate`, `Register`, `Ready`, and `Drain` each accepted a `Behaviour` and ignored it | `every_combination()` returned 21 points and the tests asserted each *fired*. A phase that fails identically for all three behaviours fires every time | All 21 execute and are asserted for the outcome that behaviour produces at that phase. Writing them **found two more unbounded callbacks** |
| T116 | `Provider::dependencies`, `ReadinessContributor::readiness`, and `EntropySource::fill` were **unbounded** | They are accessors. "Looks like a field read" is not a bound, and the inventory had never been asked to include them | All three bounded. The kernel-owned callback inventory moves from **five to eight** |
| T117 | C-L9's `Panic` guarantee was stated unconditionally | `catch_unwind` was described by what it does, not by what it cannot do | Documented in `provider/contain.rs`, `health/contributor.rs`, and open item 16 |

**One diagnostic regression was introduced and corrected during T116.** Moving
`ReadinessContributor::name` inside the panic guard meant a contributor that panicked in
`readiness` — the likely case — lost its name to a registration position. The two calls are now
guarded **separately**, so only a contributor whose *name* panics degrades to a position.

### B — release and package contracts

| # | Defect | Why nothing caught it | Now caught by |
|---|---|---|---|
| T118 | `package-metadata.md` and `RELEASING.md` prohibited **any** path dependency. FR-040 prohibits a **path-only** one. Read literally, they made this workspace unpublishable by rule while it was publishable in fact | Three documents stated the rule and **nothing executed it** | `publishable_dependencies_are_resolvable` in step 7, plus seven unit tests covering the permitted form, both prohibited forms, the unreadable sub-table shape, the `publish = false` exemption, and a scan that reads nothing |
| T119 | `RELEASING.md` listed `renvor` alone at position 1, "declares no dependencies at all" | True in Phase 001. Phase 002 gave the facade dependencies and added three publishable crates; the table was never revisited | Corrected to `renvor-core` → {`renvor-config`, `renvor-testkit`} → `renvor`, with the stale claim quoted and dated |
| T120 | `release-dry-run.yml` claimed every artifact lands outside the checkout. `cargo package` writes to `target/`, **inside** it. The cleanliness assertion used `git status`, which consults `.gitignore` and is **structurally incapable** of seeing the files this job produces | The assertion always passed, and always would have | `CARGO_TARGET_DIR` outside the checkout; a `find`-based file-listing diff that ignores `.gitignore` entirely; and a **positive control that plants a file in `target/` and requires the detector to see it** |
| T121 | `README.md`, `SUPPORT.md`, `SECURITY.md`, `CONTRIBUTING.md`, both crate READMEs, the facade's `description`, and four documentation-site pages all still said Renvor "ships no runtime capability" and exposes "three constants" | Nothing checks prose against the code | Rewritten to describe the kernel honestly **and** to state the limit that matters: there is no transport, so nothing can be served |

### C — verification and evidence integrity

| # | Defect | Why nothing caught it | Now caught by |
|---|---|---|---|
| T122 | The inventory's resolved-set table still carried the deleted `confique` tree; the direct/transitive split read 11/37 against a live 10/38; and the MSRV note named three dev-only packages when two remain and **both are production** | The summary counts were corrected when `confique` was deleted. The 54-row table beneath them was not | The table regenerated from `cargo metadata --locked`; **Gate 15 now compares the document against the live graph in both directions**, with a control that plants a package the graph does not contain |
| T123 | Gate 12 globbed `examples/*.rs` from the repository root and ran **zero** examples | **The T123 explanation was itself wrong, and T138 corrected it.** It said the directory did not exist and that `for f in <no matches>` runs the body zero times and exits 0. `examples/` *does* exist — tracked, holding `.gitkeep` and a README — with no `.rs` file in it; and an unmatched glob is not a no-op. bash leaves it literal, so the body ran **once** with `f=examples/*.rs` and `cargo run --example '*'` exited non-zero (script status **101**); zsh rejects the unmatched glob before the loop (status **1**). The gate **failed loudly in both shells**. The defect was that a pass was recorded for it regardless | Discovery under `crates/renvor/examples`, a minimum count, and a control that plants a failing example and requires the gate to reject it. T138 adds `while IFS= read -r` over a file (the old `for name in $EXAMPLES` word-split in bash but not in zsh, where it passed all three names as one argument), a before/after `git status --porcelain` comparison with its own planted-leftover control, and execution under **both** shells |
| T124 | Gate 14d described the ADR-0007 authority as *"a separately proposed and separately approved waiver"* — future tense, written before W-004 existed — and matched status with `^status: accepted`, which the decision-record template **has never produced** | The gate had not been run against the record since the record was accepted | Rewritten for the merged W-004: ledger presence, `active` status, the exact reviewer string, all four counted controls, a recorded advisory result, and the honest denial of independence |
| T125 | Gate 13f counted the facade's **own unit tests** as implementation items and reported 5 | `^\s*(pub )?(fn\|impl\|struct\|enum)` matches an indented `fn` inside `#[cfg(test)] mod tests` | The test module excluded, with **two** controls: the facade must re-export something, and the pattern must still match the test functions when run against the whole file |

**Gates 13, 14, and 15 would each have failed if run on 2026-08-16 before these corrections.** The
T107 record of "all gates pass" reflected gates that were run in an earlier form, or in the case of
Gate 12 a gate that passed by executing nothing. The full sixteen-gate result is recorded below.

### T125 / T140 — every quickstart gate, run individually

Run one at a time on 2026-08-16 against the final tree, each in its own shell process with the
Setup preamble prepended exactly as a reader would paste it. **No gate is collapsed into another,
and none is omitted.** Individual processes are the point: a concatenated script lets one gate's
`set -e` abort the rest and reports a partial run as a clean one.

> **Re-run and re-recorded at T140** (RV-N19: the previous table predated the gate rewrites it was
> cited as evidence for). The re-run added the **tests executed** column, and that column is what
> found C3 above — four of these gates were passing on **zero** tests, which no exit status could
> have shown.

| Gate | Criterion | Result | Tests run | Note |
|---|---|---|---|---|
| 0 | Workspace builds, format, lint | **PASS** | — | |
| 1 | Dependency policy (SC-012, SC-017) | **PASS** | — | includes the empty allow-list control |
| 2 | Lifecycle order and rollback (SC-001, SC-002) | **PASS** | **47** | was 31; `tests/lifecycle.rs` and `lifecycle_edges.rs` were being skipped |
| 3 | Configuration proof gate, 8 obligations (SC-020) | **PASS** | **19** | **was 0** — the filter matched nothing |
| 4 | Secrets and opaque state (SC-007, SC-016) | **PASS** | **18** | **was 0** |
| 5 | Provider graph ceilings and work budget (SC-005, SC-021) | **PASS** | **7** | **was 0**; includes the 1024-node chain |
| 6 | Drain, including the zero budget (SC-006) | **PASS** | **20** | was 9 |
| 7 | Health and readiness disagree (SC-008) | **PASS** | **15** | was 6 |
| 8 | Failure injection at every phase (SC-009) | **PASS** | **13** | was 2 — all **21** combinations are in `tests/injection.rs`, which the gate never ran |
| 9 | Run identifier opacity (SC-019) | **PASS** | **12** | was 6 |
| 10 | Tracing ownership (FR-029) | **PASS** | **3** | was 2; the install-after-build proof itself was skipped |
| 11 | Hostile configuration input (FR-038) | **PASS** | **8** | **was 0**; hand-written generator, see open item 8 |
| 12 | Examples and documentation (SC-013, SC-014) | **PASS** | — | 3 examples run individually, **4 controls fire**, repository state byte-identical before and after. Run under **bash 3.2 and zsh** |
| 13 | Scope discipline and crate DAG (SC-010) | **PASS** | — | **would have FAILED before T125** — 13f counted test functions |
| 14 | Publication and governance (SC-011, FR-034, ADR-0007) | **PASS** | — | **14d would have FAILED before T124** — status pattern never matched |
| 15 | Resolved-dependency inventory (FR-040) | **PASS** | — | **compared nothing before T122**, and read no prose before T134 — now 48 = 48 both directions, with the narrative checked and three planted controls |

**16 of 16 pass. 0 skipped. 0 inconclusive. 162 tests executed across gates 2–11**, against **56**
before — the sum of the column above — four of which contributed nothing. Repository entries before and after the full run were
identical and no probe file survived. Gate 14b's GitHub-releases half was verified with an
authenticated `gh`, not recorded as an unverified gap.

## T127 — final repository-local `speckit-analyze`

Run on **2026-08-16** against the finished artifacts and the implementation, after the T111–T126
corrections. Cross-artifact consistency was checked mechanically where a claim is a number or a
fixed string, and by reading where it is not.

### Findings

| ID | Category | Severity | Location | Finding | Disposition |
|---|---|---|---|---|---|
| A1 | Inconsistency | **MEDIUM** | `tasks.md` §Four gates | Recorded **55** external packages and "43 of 55 arrived transitively" | **RESOLVED.** Corrected to 48 and 38 of 48, with the superseded figure named and dated |
| A2 | Inconsistency | **MEDIUM** | `error/mod.rs` module doc | Said "fourteen categories" after `ResourceUnavailable` made fifteen | **RESOLVED.** Corrected. The `ALL` length assertion had already failed and been updated; the prose had not |
| A3 | Inconsistency | **MEDIUM** | `xtask/src/main.rs` step 7 | Two comments said "three claims" / "the three share a shape" after the step grew to **five** sub-checks | **RESOLVED.** Both corrected |
| A4 | Inconsistency | **MEDIUM** | `phase-002-evidence.md` open item 12 | Said the wait enumeration "is corrected from three waits to five" — true when written, superseded at T116 by eight | **RESOLVED.** Now records both corrections and which check forced the second |
| A5 | Staleness | **LOW** | `decisions/0007-*.md` dispositions S-4 and the consequences paragraph | Cite "all 55 external packages" | **DISPOSITIONED — left as written**, with a dated note added. A decision record is an account of what was decided on what evidence at a date; rewriting its figures would destroy exactly the property that makes it evidence. The note points at the live inventory |
| A6 | Ambiguity | **LOW** | `spec.md` SC-009 | "**7 of 7** phases injectable, **100%** covered by a test" does not state on its face that C-L9's three behaviours multiply it to 21 | **DISPOSITIONED — no change to the specification.** The text is not wrong: C-L9 supplies the behaviours and SC-009 supplies the coverage requirement, and *100% of those injections* is what makes 21 the correct reading. The ambiguity was in the **implementation**, which honoured 11 of 21, and that is fixed at T116. Amending settled specification text to describe the fix would be rewriting the requirement to match the code |

### Coverage

| Check | Result |
|---|---|
| FR-001…FR-044 each mapped to implementation and tests | **44 of 44**, 0 empty cells |
| SC-001…SC-022 each mapped to evidence | **22 of 22**, 0 empty cells |
| Tasks with no mapped requirement | **0** |
| Requirements with no task | **0** |
| Resolver work-budget figures agreeing across artifacts | **yes** — `2048 / 16384 / 18432` (allowances) and `2048 / 8192 / 10240` (observed) are the only two triples that appear, and never mixed |
| Configuration proof gate and its fallback represented consistently | **yes** — failed 4 of 8 in every artifact, fallback triggered in every artifact, `renvor-config` in ADR-0007 scope in every artifact |
| ADR-0007 sequenced as a blocking human gate that neither W-002 nor W-003 authorises | **yes**, and W-004 is now named as the authority that does |
| Constitution conflicts | **0** |

**0 CRITICAL. 0 HIGH. 4 MEDIUM, all resolved. 2 LOW, both dispositioned explicitly.**

## T128 — W-005 advisory reviews

Both run on **2026-08-16** in **clean contexts**, against explicit written checklists of seven
numbered questions each, with the deliverable written to a file rather than returned as a message.

**Both are NON-INDEPENDENT and ADVISORY.** They are agent reviews performed under a recorded
exception. Neither is an independent human review, no independent human review of Phase 002 has
occurred, and describing either as independent anywhere would be false.

### The delivery remedy worked, and was needed again

W-004's attempt 1 recorded both reviews as **NOT PERFORMED** because the agents went idle without
returning anything. The remedy — narrow checklists and a **file on disk** rather than a returned
message — was adopted then. It earned itself here: the requirements reviewer **went idle again**,
and its 841-line review was on disk and complete. Had the deliverable still depended on message
delivery, this would have been the third occurrence of the same failure.

### Results

| Review | Result | Findings, counted from the enumerated rows |
|---|---|---|
| Requirements | **PERFORMED**, all seven questions answered | **1 CRITICAL, 7 MAJOR, 18 MINOR = 26** |
| Security | **PERFORMED**, all seven questions answered | **0 CRITICAL, 5 MAJOR, 7 MINOR = 12** |

Neither returned silence. Question 3 of the security checklist returned an explicit **NO FINDINGS**
naming what was checked, which is a recorded result and not an absence.

> **Reconciled 2026-08-16 (T139), and two counts changed.** This section previously recorded
> *"1 CRITICAL, 7 MAJOR, 17 MINOR"* for the requirements review, which is the total the review
> deliverable states on its own last line. Its own **enumerated summary table has 26 rows**, of
> which 18 are MINOR. The reviewer miscounted by one in its closing line, and this ledger copied
> the closing line rather than counting the table. The figures above are now derived by counting
> the rows, per the rule that a total must be mechanically derivable from the findings it totals.
>
> **Every finding is now listed individually below, with its own ID, severity, and disposition.**
> The previous form grouped 19 of the 24 MINOR findings into four thematic rows and omitted the
> other 5 entirely — requirements **Q4-2, Q4-3, Q4-4, Q6-3, Q7-6** appeared only inside a MAJOR
> table or not at all, and security **2.2** appeared nowhere. A grouped disposition is a legitimate
> way to explain a shared reason; it is not a legitimate substitute for a missing row, and here it
> had become one.
>
> **The primary deliverables are the source.** All four review files were recovered and re-read
> before this reconciliation, so nothing below is reconstructed from memory or inferred from the
> previous summary. The IDs are the reviewers' own.
>
> **The re-review IDs collided and are now namespaced.** Both re-reviews numbered their new
> findings `N1, N2, …` independently, so "N1" named two unrelated defects. They are **RV-N1…RV-N19**
> (requirements) and **SV-N1…SV-N4** (security) from here on. The previous text hid the collision
> by referring to "the security re-review's remaining four" without listing them — which is how two
> MAJOR security findings came to have no individual disposition at all.

### Round 1, requirements — all 26 findings

Severities are the reviewer's. Locations are in the deliverable and are not repeated here.

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| Q1-1 | MINOR | FR-041 cites a prose section of this document; SC-019 cites an implementation property. Neither names a test or gate | **ACCEPTED.** Citation format, not missing evidence: both cited artifacts exist and the reviewer checked them |
| Q1-2 | MINOR | Five evidence rows cite unit tests by module nickname rather than by path | **ACCEPTED.** The tests exist — 32 attributes across four modules, counted by the reviewer. A reader following any of them arrives at real evidence |
| Q2-1 | **MAJOR** | FR-032 / SC-014's "no hidden global mutable state" was cited to Gate 12, which contained no such check | **FIXED.** Gate 12 greps the examples for `static mut`, `lazy_static!`, `once_cell`, `OnceLock`, `thread_local!` and the atomic/lock forms, with a control that plants a global and requires the pattern to fire |
| Q2-2 | MINOR | FR-022 cites `tests/no_silent_fallback.rs` for the environment two-candidate decode; that file has no environment coverage. The real coverage is in uncited `renvor-config` tests | **ACCEPTED.** The behaviour is covered; the citation points at the wrong file |
| Q3-1 | **MAJOR** | `Drain × Panic` and `Drain × Fail` execute identical kernel code — the harness catches the panic before `shutdown()` — so "all 21 are attributed" overstated the pair | **ACCEPTED AND CORRECTED, not fixed.** Renvor holds no join handle for an author's task, so there is no kernel-side observation to make. The pair proves the permit is released by unwinding, a `Drop` property nothing tested. Stated as a limit in the harness docs, in the test, and as **open item 20** |
| Q3-2 | MINOR | Six combinations (Fail/Panic × Load/Validate/Register) are asserted identically as `BuildFailed(_)` though the kernel distinguishes them | **ACCEPTED.** The assertions are true and under-specific. Tightening them at the end of a phase risks encoding today's behaviour as a requirement |
| Q3-3 | MINOR | `Register × Fail` yields `DependencyCycle` (self-provided capability), not the documented `DependencyMissing` | **ACCEPTED**, with Q3-2 |
| Q3-4 | MINOR | The file's stated positive control does inject and never calls `shutdown()`; its name claims both | **ACCEPTED.** The control is real; its name over-describes it |
| Q4-1 | **MAJOR** | `impl Debug for dyn ReadinessContributor` calls the author's `name()` unbounded, invisible to all three `deadlines.rs` checks | **FIXED as a named exclusion.** Bounding a `Debug` impl would be worse than the defect — it has no way to report a deadline except by writing into the output it is producing. A test enumerates every `impl fmt::Debug for dyn` and fails if one appears or moves unrecorded. No lifecycle phase formats an author's provider or contributor |
| Q4-2 | MINOR | `impl Debug for dyn Provider` calls three author methods unbounded, exempted by an unenforced prose claim | **FIXED with Q4-1**, by the same enumeration test. *Previously filed in the MAJOR table; it is MINOR* |
| Q4-3 | MINOR | The derived `Debug` on `ApplicationBuilder` invokes the author's `ConfigSource: fmt::Debug` impl unbounded, and `Debug` is not a tracked shape | **FIXED with Q4-1.** RV-N7 later found this disposition still overstated; see there. *Previously filed in the MAJOR table; it is MINOR* |
| Q4-4 | MINOR | The discovery gate walked only `renvor-core/src`; `crates/renvor-config/src` was never scanned | **FIXED.** Both crates are walked. The answer is unchanged — 11 `renvor-config` files reach no author trait, because the crate *implements* `ConfigSource` rather than calling it — but it is now measured rather than unasked. *Previously filed in the MAJOR table; it is MINOR* |
| Q5-1 | **MAJOR** | `tasks.md` marked T127 and T129 `[ ]` while this ledger presented both as complete; SC-013 cited a T131 record that did not exist | **FIXED.** The task list is authoritative and the ledger says so |
| Q6-1 | **CRITICAL** | The opening sentence claimed *"Every task T001–T132 has been executed and verified"*. T132 is the push and pull request, which had not happened and provably could not have — no upstream, no remote ref | **FIXED.** The claim was prospective, stated as observed, and it was the first sentence in the document. RV-N1 later found the correction had gone stale in the opposite direction |
| Q6-2 | MINOR | The open-items table is misordered (item 15 precedes struck item 14) and SC-013's "T131 record" citation resolves to nothing | **FIXED for the citation** — SC-013 now cites the T131 section that exists. **ACCEPTED for the ordering**: the numbering is chronological and renumbering would break every reference into it |
| Q6-3 | MINOR | "16 of 16 pass" rests on an authenticated-`gh` claim whose exit status cannot be distinguished from the unverified branch | **ACCEPTED**, and RV-N10 raised the same shape against 15d. Recorded rather than restated as proof. *This finding had no row at all in the previous version* |
| Q7-1 | **MAJOR** | Gate 12's Pass paragraph claims "0 examples require a transport, a port, or a database" and is labelled SC-014; the script checked exit status only | **FIXED.** The global-state half is now executed (Q2-1); the transport half is attributed to Gate 13a, which is what evidences it. RV-N4 notes what 13a can and cannot prove |
| Q7-2 | MINOR | `grep -c` exits 1 on zero under `pipefail`, aborting before CONTROL 1's diagnostic can print | **FIXED at T138.** The count is now `$(grep -c . "$EXAMPLE_LIST" \|\| true)`, so CONTROL 1 reaches its own message |
| Q7-3 | MINOR | 13b's alternation lists two packages no longer in the lockfile and omits four that are; the control does not test the spellings | **ACCEPTED.** Fail-closed today; recorded as known imprecision |
| Q7-4 | MINOR | 13f's regex cannot match `impl<T>`, `pub(crate) fn`, `async fn`, `type`, `trait`, `const`, `static`, or `macro_rules!` | **ACCEPTED.** Currently exact for the forms present; recorded |
| Q7-5 | **MAJOR** | 14c is a zero-asserting check with **no control**, and `grep -r` on a missing directory exits 2, which its `\|\|` branch read as "found nothing" | **FIXED.** The directory's existence is asserted, and a control plants a `docker/build-push-action` step and requires the pattern to match |
| Q7-6 | MINOR | 14b's "NOT VERIFIED" branch exits 0, so the recorded gap never reaches the gate's verdict | **ACCEPTED.** The gap is printed and recorded; making an unverifiable network condition fail the gate would make the gate depend on network reachability. *This finding had no row at all in the previous version* |
| Q7-7 | **MAJOR** | Gate 15's Pass paragraph claimed licence/MSRV/origin verification and duplicate recording; the parser captured name and version only, `--duplicates` went to `\|\| true`, and `features.txt` was deleted unread | **FIXED.** A per-column check requires every documented row to carry a non-empty licence, MSRV, origin, and reach; duplicates and feature output are printed. RV-N9 notes the feature half prints a count, not the content |
| Q7-8 | MINOR | Gate 15's CONTROL 2 cannot fail given the gate reached it; it tests `diff`, not the extraction pipeline | **ACCEPTED.** It is weak rather than wrong; T138's 15f adds three controls that do exercise their pipeline |
| Q7-9 | MINOR | Python codepoint ordering versus locale-dependent `sort -u`, with no `LC_ALL` in Setup | **ACCEPTED.** Fail-closed; it would misattribute a locale failure to inventory drift |
| Q7-10 | MINOR | The Summary-of-gates table under-reports controls for Gate 12 and mislabels Gate 15 as non-zero-asserting | **FIXED at T138**, together with RV-N11, which found the same table still stale after the first fix |

### Round 1, security — all 12 findings

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| 1.1 | **MAJOR** | `ResolvedConfig<T>` derived `Debug`, so every raw configuration value had a printing route | **FIXED**, then found incomplete by SV round 2 — four sibling types held the same data and kept deriving it. All five are hand-written and a set-wide test now covers the property |
| 1.2 | MINOR | The zeroization guarantee is narrower than the C-C9 table implies | **ACCEPTED.** Recorded as a documented limit rather than restated as a stronger guarantee |
| 2.1 | **MAJOR** | `Constraint::from_decoder` used `split_once`, so a value containing `", expected "` had its tail copied into the error; also enables log-line injection | **FIXED.** `rsplit_once` — the decoder appends its separator last — plus an `is_type_description` whitelist. Three tests including the reviewer's exact payloads, and an end-to-end check through `renvor-config`. Verified CLOSED in round 2 against 18 payloads × 2 layers and 15 decoder shapes |
| 2.2 | MINOR | The key-only path forwards an unbounded decoder message verbatim | **ACCEPTED**, and re-raised as SV-N3, which measured the volume: a 1 MiB key becomes a 1 MiB error string. Content is permitted (keys are not secrets); volume is not bounded. **Open item 25.** *This finding had no row at all in the previous version* |
| 3.1 | **MAJOR** | A single environment variable name with ~3,000 separators — 9 KB, far under the ~128 KiB an OS permits — overflowed the stack and **aborted**. Not catchable: an overflow is not a panic | **FIXED.** `MAX_KEY_DEPTH = 32`, checked before anything is nested. The asymmetry the reviewer identified is the sharpest part: the file layer was protected by `toml_parser`'s own recursion limit, not by Renvor. Verified CLOSED in round 2 against 9 attacks; SV-N1 then found the same overflow on the *public* path |
| 3.2 | MINOR | Silent, deterministic within-layer key collision (`APP_password` beats `APP_PASSWORD`) | **ACCEPTED.** Deterministic and documented; changing it would change resolution semantics at the end of a phase |
| 4.1 | **MAJOR** | The byte ceiling checked `metadata.len()` then called `read_to_string`. A FIFO or `/proc` entry reports zero and yields indefinitely | **FIXED.** `Read::take` bounds the read by construction. Verified CLOSED in round 2 **for the byte ceiling only** — SV-N2 then found the *time* half still unbounded |
| 4.2 | MINOR | The depth bound is real, unowned, and misattributed by its own test comment | **ACCEPTED**, and re-raised as RV-N16 |
| 5.1 | **MAJOR** | A hung readiness contributor leaked **one thread per probe, without limit** | **FIXED**, then **REGRESSED** by that fix and fixed again — see the regression section below. The leak is not removed; nothing in Rust interrupts a blocked thread. It is **bounded**, which is the difference between a limitation and a denial of service |
| 6.1 | MINOR | The panic-containment limitation list omits destructor panics, panic hooks, OOM, and provider-spawned threads | **ACCEPTED**, folded into **open item 16** |
| 7.1 | MINOR | The build-time code-execution disclosure names the one proc macro that left and none of the four that stayed | **ACCEPTED**, recorded against the inventory |
| 7.2 | MINOR | The inventory contradicts itself on the package count: T033 asserted the FR-040 gate over 55 packages while the graph is 48 | **FIXED at T138**, and only there. This was previously dispositioned as *"the same superseded 55 already corrected at T122"* — which was **wrong**: T122 corrected the summary row and left the prose at 55 in three places. Gate 15's new 15f check now reads the prose and fails on a stale total |

**No finding at HIGH or above was refused in round 1.** The single CRITICAL and all twelve MAJOR
findings are either fixed with a regression test or accepted with the reason stated and an open
item raised.

## T128 (continued) — W-005 verification re-review

The first pair of reviews found defects; those defects were then **fixed**; and the final head
therefore contained code neither reviewer had seen. Reporting fixes as verified when the
verification predates the fix is the same prospective claim Q6-1 was raised for, so both reviews
were re-run against `bf70553`, scoped to **attacking the fixes** rather than repeating themselves.

Both **NON-INDEPENDENT and ADVISORY**, both delivered to disk, both returned enumerated findings.

| Re-review | Original findings | New findings, counted from the enumerated rows |
|---|---|---|
| Requirements | **6 CLOSED**, 0 not closed, 0 regressed | **19** — RV-N1…RV-N19 |
| Security | **3 CLOSED**, **1 NOT CLOSED**, **1 REGRESSED** (4.1's CLOSED is qualified: byte ceiling only) | **4** — SV-N1…SV-N4 |

> **Reconciled 2026-08-16 (T139), and both rows changed.** The requirements row previously read
> "2 CRITICAL, 3 MAJOR, 6 MINOR", which totals **11** against a re-review whose own last line says
> **19 NEW FINDINGS** — RV-N5, RV-N6, RV-N7, RV-N9, RV-N10, RV-N11 and the three reproduced MAJORs
> RV-N12, RV-N13, RV-N14 had no row. The security row recorded "3 CLOSED, 1 REGRESSED, 1 closed on
> the byte ceiling only" and **dropped the NOT CLOSED verdict entirely**: finding 1.1 was returned
> NOT CLOSED because four sibling types still printed the credential. Both are corrected against
> the deliverables.

**This is the pass that earned its cost.** It found a regression I introduced while fixing a
finding, and a fix that closed the type it was pointed at while four siblings kept the defect.

### The regression: a fix that traded one denial of service for another

Security 5.1's fix bounded the readiness thread leak with a **process-global** `static`. Both
reviewers found what that cost, independently, and both **reproduced** it:

| Defect | Evidence |
|---|---|
| One application's hung contributor permanently refuses **every other application's** probes in the same process | A separate `HealthState` with a healthy contributor returned `NotAsked` for ever |
| It broke the **shipped test suite** under `--test-threads=1` | libtest runs alphabetically; the ceiling test saturated the counter and a later test received the refusal fallback instead of its contributor's name |
| The ceiling was not atomic — `load()` then `fetch_add` with an allocation between | Reproduced at 4096-way concurrency: 64, then **65**, then **67** workers entered. The documented "hard ceiling" was `64 + concurrent callers` |
| The shipped regression test probed **serially**, asserting a property the implementation did not hold, without exercising the case that broke it | — |

**Fixed by moving the counter into the `HealthState`** — so the budget is per application, and the
cross-application coupling cannot exist — **and by claiming slots with `fetch_update`**, so the
documented bound is the real one. Two new tests: one proving two applications are isolated, one
driving 256 concurrent probes and asserting the ceiling holds, with a control requiring the ceiling
to actually be approached.

`--test-threads=1` now passes. It is the standard invocation for chasing a flake, and it was
reproducibly broken.

### The sibling defect: fixing the type that was pointed at

Security 1.1 named `ResolvedConfig<T>` for deriving `Debug`. It was hand-written. **Four other
public types held the same data and kept deriving it**, each reproduced printing the credential:

| Type | What printed |
|---|---|
| `DecodedLayer` | `table: {"password": String("hunter2-do-not-print"), …}` |
| `Merged` | the whole merged tree |
| `LayeredResolverBuilder` | defaults **and** `env_override` — a map of environment **values** |
| `LayeredResolver` | the same, and **this is the value `SchemaSource` wraps** |

The last row is the one worth sitting with. `SchemaSource` hand-writes `Debug` specifically so a
resolved configuration cannot print, and it holds a `LayeredResolver` any caller could print. The
guarded door had an open window; the first fix closed a *different* window and said so.

All four are now hand-written. The new test enumerates **every public type that can hold a
configuration value** and asserts the property across the set, because C-E3 is a claim about a set
and fixing the member somebody named does not establish it.

### Round 2, requirements — all 19 new findings

The reviewer labelled RV-N12…RV-N19 with severities and left RV-N1…RV-N11 unlabelled. Where the
column below says *(assigned here)*, the severity is **this ledger's judgement, not the
reviewer's**, and is marked so rather than presented as the reviewer's own.

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| RV-N1 | **CRITICAL** *(assigned here)* | The Status paragraph, rewritten to fix Q6-1, went stale in the **opposite** direction — `4d8711c` fixed it, then `2cd0530` checked T131 and `bf70553` checked T132 without revisiting it, so it under-claimed | **FIXED**, with both corrections recorded. A status line naming a moving boundary must be re-read every time the boundary moves; twice it was not |
| RV-N2 | **CRITICAL** *(assigned here)* | The T132 section recorded head `2cd0530` and a diff total uncaveated, while T131's section carried exactly that caveat | **FIXED.** The section states which head it observed and that conclusions were re-confirmed later |
| RV-N3 | **MAJOR** *(assigned here)* | The new global-state pattern missed `static X: AtomicUsize` and `static X: Mutex<…>` — the idiomatic forms since `Mutex::new` became `const` — while its Pass paragraph claimed the broader property. **The same commit had added a `static _: AtomicUsize` to the kernel** | **FIXED.** The alternation covers them and the control plants an `AtomicUsize`. The kernel static it would have caught is gone, removed by the RV-N13 fix |
| RV-N4 | MINOR *(assigned here)* | "0 examples require a transport" is attributed to Gate 13a, which is a **denylist** of ten package names rather than a proof | **ACCEPTED.** A denylist is what is available without a capability model. Recorded rather than restated as proof. **Open item 26** |
| RV-N5 | MINOR *(assigned here)* | The `impl Debug for dyn` enumeration is **file-granular, not impl-granular**: the loop pushes each path once, so a second such impl in an already-listed file is invisible | **ACCEPTED.** Two impls in one file would evade the count while remaining in a recorded file. Recorded as a known limit of the enumeration. *This finding had no row in the previous version* |
| RV-N6 | MINOR *(assigned here)* | The match is an exact literal: `impl std::fmt::Debug for dyn Foo` and `impl Debug for dyn Foo` both evade it | **ACCEPTED.** The workspace writes one form and `rustfmt` keeps it; recorded rather than made into a parser. *No row in the previous version* |
| RV-N7 | MINOR *(assigned here)* | The recorded method descriptions are never asserted against anything and can rot; and Q4-3's disposition overstates what the test covers, since a **derived** `Debug` is not an `impl … for dyn` at all | **ACCEPTED, and Q4-3's disposition corrected above** to say the test covers the `dyn` impls and not the derive. *No row in the previous version* |
| RV-N8 | MINOR *(assigned here)* | "The set is eight" appears in three places; only the test carried the `Debug` exclusion | **FIXED.** The module documentation and the FR-025 evidence row say **eight lifecycle callbacks**, with the exclusion named |
| RV-N9 | MINOR *(assigned here)* | Gate 15's enabled-feature clause prints only the line **count**; `features.txt` is still deleted unread. The fix records the output's size instead of its content | **ACCEPTED.** Smaller than Q7-7 but the same shape. The file is written and counted; printing 277 lines of feature graph into every gate run buys less than it costs. Recorded. *No row in the previous version* |
| RV-N10 | MINOR *(assigned here)* | 15d is a **new zero-asserting check with no positive control** — the same shape as Q7-5, introduced by Q7-7's fix | **ACCEPTED.** 15d is fail-closed through `pipefail` on its own pipeline, but nothing plants an incomplete row. T138's 15f adds three planted controls to Gate 15; 15d itself still has none. **Open item 27.** *No row in the previous version* |
| RV-N11 | MINOR *(assigned here)* | The Summary-of-gates table was not updated for the controls the Q7-5/Q7-7 fixes added | **FIXED at T138**, together with Q7-10. *No row in the previous version* |
| RV-N12 | **MAJOR** | The readiness-ceiling fix makes the **shipped test suite order-dependent** and breaks `--test-threads=1`. Reproduced | **FIXED** — see the regression section above. `--test-threads=1` now passes and is run as part of every verification |
| RV-N13 | **MAJOR** | The ceiling is **process-global**, not per-application, and this is not disclosed: one application's hung contributor permanently refuses every other application's probes in the same process. Reproduced | **FIXED.** The counter moved into `HealthState`, so the budget is per application and the coupling cannot exist |
| RV-N14 | **MAJOR** | The ceiling is **not atomically enforced** — `load()` then `fetch_add` with an allocation between — while being documented as a hard bound. Reproduced at 4096-way concurrency: 64, then 65, then 67 workers entered | **FIXED.** Slots are claimed with `fetch_update`, so the documented bound is the real one. A 256-way concurrency test asserts it, with a control requiring the ceiling to actually be approached |
| RV-N15 | MINOR | The FIFO regression test **silently skips** where `mkfifo` is absent, against the project's own fail-closed rule | **FIXED at T138.** `mkfifo(1)` is POSIX and required on every platform the `cfg(unix)` block compiles for, so its absence now fails the test as a broken environment |
| RV-N16 | MINOR | The depth ceiling is hard-coded in its own error message, and the test asserts the literal | **ACCEPTED.** Recorded with the phase's other chosen bounds; the literal and the constant would need a format-time interpolation the message text does not currently take |
| RV-N17 | MINOR | The end-to-end leak test covers one of the two layers the finding was measured on | **ACCEPTED.** The unit tests cover the stripping function on both; the end-to-end path is exercised on the environment layer, which is the attacker-controlled one |
| RV-N18 | MINOR | A comment in `hostile.rs` still states the superseded rationale | **FIXED at T138** — that comment block was rewritten wholesale when the FIFO test grew to cover all three variants |
| RV-N19 | MINOR | T125's gate table predates the gate rewrites it is cited as evidence for | **FIXED at T140.** T125's table is re-run and re-recorded against the final gate scripts |

### Round 2, security — all 4 new findings

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| SV-N1 | **MAJOR** | The depth guard is on the caller, not on the recursion. `MAX_KEY_DEPTH` was enforced in `read_environment`; the recursion it protects is reached by the **public** `decode_single`, which a 3,000-segment key drove to `fatal runtime error: stack overflow`, exit 134 | **FIXED at T139**, not accepted. Reproduced first from **outside** the crate against the published API, then closed by checking the depth at the public boundary. The reviewer suggested putting the ceiling in `nest`; `nest` is an iterative `pop` loop, and the depth is consumed by `try_into`'s descent and by the nested table's recursive `Drop`, so bounding the constructor would have protected nothing. A regression test uses the measured 3,000 verbatim, with a positive control at the ceiling. `MAX_KEY_DEPTH` is now re-exported from the crate root, because a limit a caller can hit and cannot name is one they discover by crashing |
| SV-N2 | **MAJOR** | The file read is bounded in bytes and **unbounded in time**. Two variants reproduce and were still blocked at 35 s: a FIFO with **no writer** blocks in `File::open` before any ceiling is reached, and a **slow writer** holding the descriptor open leaves `read_to_end` waiting for an EOF that never comes. FR-025 prohibits an unbounded wait in a kernel-owned path | **FIXED at T139**, not accepted, **with a named residual**. A path that is not a regular file is now refused on the `metadata` already read — a `stat`, which does not open the path, so the refusal happens *before* the blocking open. The FIFO test now covers all three variants (no writer, slow writer, flooding writer) on a worker thread under a 10-second timeout, so a hang is a failure rather than a test that never returns, plus a regular-file positive control. **RESIDUAL: this is check-then-open.** An attacker who can replace the path between the `stat` and the `open` can still present a FIFO and block. Closing that needs `O_NONBLOCK` at open time, hence a direct `libc` dependency and a new FR-040 inventory row — a scope change this phase does not take. **Open item 24** |
| SV-N3 | MINOR | The key-only path forwards an unbounded message: a 1 MiB TOML key or a ~128 KiB environment variable name becomes an error string of that size in every log that catches the failure. Content is permitted (keys are not secrets); volume is not | **ACCEPTED. Open item 25.** The same shape as round-1 finding 2.2. Note that SV-N1's fix **adds a call site with this property** — its diagnostic names the offending key, and an over-deep key is long by construction |
| SV-N4 | MINOR | `MAX_OUTSTANDING_PROBES` is not re-exported from `health`'s root, so callers reach through `health::contributor::`; the crate's own integration test does exactly that | **ACCEPTED** for `renvor-core`. The equivalent defect in `renvor-config` was **FIXED at T139** as part of SV-N1: `MAX_KEY_DEPTH` now sits at the crate root beside `MAX_FILE_BYTES` and `NESTING_SEPARATOR`. **Open item 28** |

**Both MAJOR security findings from round 2 are now fixed rather than accepted.** They previously
had no individual disposition at all: they were inside a row reading *"N15–N19, and the security
re-review's remaining four … ACCEPTED. None is a false pass."* That row was wrong twice over — it
named the wrong review's IDs, and both SV-N1 and SV-N2 **were** live defects on public API. The
sentence "No finding at HIGH or above was refused, in either round" was therefore true only because
the two findings that would have contradicted it had been absorbed into a group.

**No finding at HIGH or above is refused, in either round, as of T139** — and this is now a claim
about 61 individually enumerated findings (26 + 12 + 19 + 4) rather than about 37 claimed ones.

## Complete requirement evidence map (T129)

**Exactly FR-001…FR-044 and SC-001…SC-022.** Every requirement in the specification has a row.
A row with no concrete artifact would be visibly unmet; there are none, and the point of writing
all 66 out is that an absence would show as an empty cell rather than as a requirement nobody
looked for.

Two rows carry a qualifier rather than a plain **MET**, and both are stated in the row itself
rather than in a footnote.

### Functional requirements

| # | Requirement | Implementation | Tests and evidence | Status |
|---|---|---|---|---|
| **FR-001** | Seven phases, order enforced | `lifecycle/phase.rs` `LifecyclePhase::ALL`; `PhaseCursor::advance` has no target argument, so a backwards transition is **unrepresentable** | `tests/lifecycle.rs`, `tests/lifecycle_edges.rs` | **MET** |
| **FR-002** | Phase sequence inspectable without instrumenting internals | `ApplicationBuilder::phase_log` → `PhaseLog::entries`, taken **before** `build` | `tests/lifecycle.rs`; every `HarnessRun.phases` | **MET** |
| **FR-003** | Required configuration and dependencies validated before Ready | `Load`/`Validate`/`Register` all run inside `build`, which returns before `boot` exists | `tests/lifecycle.rs`, `renvor-config/tests/layering.rs` | **MET** |
| **FR-004** | Boot failure rolls back in exact reverse **actual** initialisation order | `lifecycle/rollback.rs`; `InitialisedProvider` records actual order, not declared | `tests/lifecycle.rs` order-divergence control | **MET** |
| **FR-005** | A rollback failure does not abort the remaining rollback | `roll_back` collects and never returns early (C-L4) | `tests/lifecycle_edges.rs`, `tests/deadlines.rs` | **MET** |
| **FR-006** | Shutdown rejects new work, drains within a bound, stops in reverse | `lifecycle/drain.rs` `WorkGate`; `Application::shutdown` | `tests/drain.rs` | **MET** |
| **FR-007** | An incomplete drain is reported as incomplete with the outstanding count | `DrainOutcome::Incomplete { outstanding }` — no third "probably clean" variant exists | `tests/drain.rs` | **MET** |
| **FR-008** | Repeated shutdown is safe and never stops a provider twice | `WorkGate::close` reports whether *it* closed the gate, inside one `send_if_modified` | `tests/drain.rs`, `tests/lifecycle_edges.rs` | **MET** |
| **FR-009** | Shutdown before Ready still rolls back in reverse order | Shared `roll_back` path; no separate pre-Ready branch | `tests/lifecycle_edges.rs` | **MET** |
| **FR-010** | Typed application state retrievable by type | `state/mod.rs` `TypedStateMap` | `state` unit tests | **MET** |
| **FR-011** | Duplicate state registration errors, naming the type | `KernelError::StateDuplicate { type_name }` | `state` unit tests | **MET** |
| **FR-012** | Providers declare dependencies; initialised in dependency order | `provider/mod.rs` `resolve_tracking`; edges directed dependent → dependency | `tests/provider_graph.rs` | **MET** |
| **FR-013** | Cycles detected before Boot, naming **every** provider | `tarjan_scc`; `KernelError::DependencyCycle { providers }` | `tests/provider_graph.rs` | **MET** |
| **FR-014** | Missing dependency detected before Boot, naming both endpoints | `KernelError::DependencyMissing { dependent, capability }` | `tests/provider_graph.rs` | **MET** |
| **FR-015** | Typed, layered configuration over exactly three source kinds | `renvor-config` `layer/{env,file}.rs`, `resolver.rs` | `renvor-config/tests/layering.rs` | **MET** |
| **FR-016** | Errors identify key, constraint, and source layer | `error/context.rs` `configuration()` | `renvor-config/tests/layering.rs`, `error` unit tests | **MET** |
| **FR-017** | Invalid configuration prevents Boot; nothing starts | Structural: `build` returns before `boot` exists | `tests/lifecycle.rs` | **MET** |
| **FR-018** | Secret fields redacted in **every** kernel output form | `secret/mod.rs` `Secret<T>`, `REDACTED`; no `Deref`, no `Into` | `renvor-config/tests/redaction.rs`, `tests/redaction.rs` | **MET** |
| **FR-019** | Errors expose an inspectable category | `ErrorCategory`, 15 variants, total `category()` match | `error` unit tests | **MET** |
| **FR-020** | Causal chain preserved | `#[source] BoxedCause` on `ProviderInit`/`ProviderStop` | `tests/lifecycle_edges.rs` downcast to `Panicked` | **MET** |
| **FR-021** | No error path emits a secret | `Constraint` cannot hold a value; `Configuration` is `#[non_exhaustive]` so no outside crate can bypass it | `error/context.rs` tests, `tests/redaction.rs` | **MET** |
| **FR-022** | No silent fallbacks | Every refusal returns an error; the env layer's two-candidate decode offers the **same value** to the **same type** | `tests/no_silent_fallback.rs` | **MET** |
| **FR-023** | Cancellation propagates to running work | `cancel/mod.rs` `CancelScope`, `ProviderScope` | `cancel` unit tests | **MET** |
| **FR-024** | Cancellation leaves no provider half-initialised | Rollback runs on every Boot failure path, cancellation included | `tests/lifecycle_edges.rs` | **MET** |
| **FR-025** | Deadlines explicit and bounded; **0** unbounded kernel-owned waits | **Eight** bounded *lifecycle* callbacks — entropy, source name, load, validate, Register declarations, provider init, provider stop, readiness. Two `Debug` impls call author code unbounded and are a **named exclusion**, not a gap | `tests/deadlines.rs`, including the discovery gate (T115) | **MET** |
| **FR-026** | Health and readiness independently queryable, able to disagree | `health/mod.rs`; readiness reads no liveness value | `tests/health.rs`, both directions | **MET** |
| **FR-027** | Drain makes readiness not-ready while liveness stays alive | `HealthState::begin_draining` touches only the drain flag | `tests/health.rs` | **MET** |
| **FR-028** | A failing readiness contributor is individually identifiable | `ContributorVerdict { name, readiness, fault }`; `ContributorFault` distinguishes panicked from timed out from not-asked | `tests/health.rs`, `renvor-testkit/tests/injection.rs` | **MET** |
| **FR-029** | Tracing init explicit, repeat-safe, installs nothing implicitly | `observe/bootstrap.rs` `try_init_global` → `AlreadyInstalled` | `tests/observe_bootstrap.rs`, install-after-build control | **MET** |
| **FR-030** | Failure injection at each named phase | `renvor-testkit` `Harness`, `FailureInjectionPoint` | `renvor-testkit/tests/injection.rs` | **MET** |
| **FR-031** | Deadline and drain behaviour without real elapsed time | `TestClock`; `#[tokio::test(start_paused = true)]` | `tests/deadlines.rs` — two one-hour deadlines in < 1 s | **MET** |
| **FR-032** | Examples compile, run, no hidden global mutable state | `crates/renvor/examples/{minimal,providers,configuration}.rs` | quickstart Gate 12, with a control | **MET** |
| **FR-033** | No HTTP, GraphQL, persistence, auth, CLI, generation, or frontend | `tokio` features are `rt`, `time`, `sync`, `macros` only | `xtask` step 7 `crate_dag_holds`; quickstart Gate 13a | **MET** |
| **FR-034** | No crate, package, image, release, or tag published | 0 published; `publish = true` states a crate *may* be, not that it was | quickstart Gate 14a–14c; sparse-index 404 × 4 with a 200 control | **MET** |
| **FR-035** | Custom infrastructure justified by an accepted ADR | ADR-0007, accepted under W-004 | quickstart Gate 14d; `decisions/0007-*.md` | **MET — under waiver** |
| **FR-036** | Public surface declared explicitly unstable | SC-022 sentence, byte-identical in three normative locations | `xtask` step 7 `instability_wording_agrees` | **MET** |
| **FR-037** | Two classes of sensitive data, both non-emitting | `Secret<T>` (a); `TypedStateMap` emits type names only (b) | `tests/redaction.rs` | **MET** |
| **FR-038** | Hostile configuration fails closed | `MAX_FILE_BYTES`; `locate_failure` bisection; non-Unicode environment refusal | `renvor-config/tests/hostile.rs`, `tests/environment_bytes.rs` | **MET** |
| **FR-039** | Concrete numeric ceilings | 1024 providers, 8192 edges, 2048/16384/18432 work units | `tests/provider_graph.rs`, `resolver_proof.rs` | **MET** |
| **FR-040** | Every external dependency recorded with full evidence | `governance/phase-002-dependency-inventory.md`, 48 packages | quickstart Gate 15 — document compared to live graph, both directions | **MET** |
| **FR-041** | Authorization impact is none | No user, role, permission, token, or session type exists | T108 record above | **MET** |
| **FR-042** | Drain budget author-overridable, documented default 30 s | `DEFAULT_DRAIN_BUDGET`; `with_drain_budget` | `tests/drain.rs` | **MET** |
| **FR-043** | One span per lifecycle phase, carrying the run identifier | `observe/spans.rs` `phase_span(phase, run_id)` | `tests/observe_bootstrap.rs`, recording subscriber | **MET** |
| **FR-044** | Configuration resolves as two distinct steps | `decode_source` per layer, then `merge_layers` | `renvor-config/tests/proof_gate.rs` obligations 7a/7b | **MET** |

### Success criteria

| # | Criterion | Evidence | Status |
|---|---|---|---|
| **SC-001** | Lifecycle order asserted by a test | `tests/lifecycle.rs` | **MET** |
| **SC-002** | Reverse shutdown order at position *n* of *k* | `tests/lifecycle.rs`, with an order-divergence control | **MET** |
| **SC-003** | **0** failing configurations start anything | `tests/lifecycle.rs`; structural — `build` precedes `boot` | **MET** |
| **SC-004** | Distinct named errors; **0** panics in ordinary use | `state` tests; `provider/mod.rs` `id_at` degrades rather than panicking | **MET** |
| **SC-005** | Cycles and missing dependencies caught before Boot; 0 reach Boot | `tests/provider_graph.rs` | **MET** |
| **SC-006** | Over-budget drain incomplete in 100% of runs, 0 reported clean | `tests/drain.rs`; zero budget has **no** fast path | **MET** |
| **SC-007** | **0** secret-marked values in any output form | `tests/redaction.rs`, `renvor-config/tests/redaction.rs` | **MET** |
| **SC-008** | Health and readiness disagree in ≥ 1 asserted state | `tests/health.rs` — asserted in **both** directions | **MET** |
| **SC-009** | 7 of 7 phases injectable, 100% covered | `renvor-testkit/tests/injection.rs` — all **21** combinations execute and are attributed | **MET** |
| **SC-010** | **0** capabilities outside declared scope | `xtask` step 7; quickstart Gate 13, 5 sub-checks each with a control | **MET** |
| **SC-011** | **0** crates, images, releases, or tags published | quickstart Gate 14; non-200/404 treated as FAIL | **MET** |
| **SC-012** | 100% of selected packages carry a recorded evaluation | `research.md` §3; `governance/phase-002-dependency-inventory.md` | **MET** |
| **SC-013** | Full sequence, 0 failing and 0 silently skipped, on both toolchains | T106 and T131 records; 11/11 on 1.94.0 and on stable | **MET** |
| **SC-014** | Every example compiles, runs, no global mutable state | quickstart Gate 12, with a failing-example control | **MET** |
| **SC-015** | **0** unbounded waits in kernel-owned paths | `tests/deadlines.rs`; discovery gate mutation-tested (T115) | **MET** |
| **SC-016** | **0** registered-state contents in any output | `tests/redaction.rs` — an unmarked credential-bearing value | **MET** |
| **SC-017** | 100% of dependencies carry version, licence, MSRV, advisory status | `governance/phase-002-dependency-inventory.md`; Gate 15 comparison | **MET** |
| **SC-018** | 7 of 7 phases emit a span; 100% carry the run identifier | `tests/observe_bootstrap.rs`, `tests/run_id.rs` | **MET** |
| **SC-019** | Opacity by construction and a deterministic accessor | `OsEntropy` has **no fields** and `new()` takes **0** inputs | **MET** |
| **SC-020** | Decoding and precedence asserted separately and end to end | `renvor-config/tests/proof_gate.rs` (8 obligations), `layering.rs` | **MET** |
| **SC-021** | Every graph bound asserted numerically, size and work separately | `ResolutionReport`; `tests/resolver_proof.rs` — 2048 / 8192 / 10240 | **MET** |
| **SC-022** | Closure stated as event plus accepted superseding record | `xtask` step 7 — 3 byte-identical copies, 0 phase numbers in the clause | **MET** |

**Counts**: 44 functional requirements, 44 rows. 22 success criteria, 22 rows. **0 empty cells.**

**FR-035 is MET under a waiver, not unconditionally.** ADR-0007 is accepted; the independent review
its acceptance would otherwise require has **not** occurred, and W-004 is the recorded authority for
that gap. A reader who needs FR-035 satisfied without a waiver should treat it as open.

**SC-009's `Panic` behaviour is bounded by the panic strategy.** It holds wherever unwinding is the
strategy, which is the default and is what CI runs. Under `panic = "abort"` it does not hold and
cannot — see open item 16.

## T131 (re-run) — verification after the W-005 re-review fixes

Run against commit **`2c6e7dc`**, the head that closes the verification re-review's findings.
Working tree empty before and after; `HEAD` identical before and after.

| Check | Toolchain | Result |
|---|---|---|
| `cargo xtask verify` | 1.94.0 (pinned) | **11 of 11**, exit 0 |
| `cargo xtask verify` | 1.97.1 (current stable) | **11 of 11**, exit 0 |
| Workspace tests, **`--test-threads=1`** | 1.94.0 | 27 targets, 0 failing |
| `HEAD` / tree, before and after | — | `2c6e7dc` / empty, both times |

**`--test-threads=1` is recorded deliberately.** The readiness-ceiling regression was invisible to
every parallel invocation and reproducible only single-threaded, and neither CI nor `cargo xtask
verify` pins the thread count. A suite that passes only under parallelism passes on scheduling
luck; this run is what says it does not.

### Pull request #19, final check results at `2c6e7dc`

| Check | Required? | Result |
|---|---|---|
| `verify (1.94.0)`, `verify (stable)`, `security`, `docs` | **yes** | **all pass** |
| `dependency-review`, `package and verify without publishing`, `Analyze (actions)`, `Analyze (rust)` | no | pass |
| `attest rehearsal artifacts` | no | skipping — `workflow_dispatch` only |
| `CodeQL` | no | **fail — 9 alerts, unchanged** |

The CodeQL count held at exactly **9**, in the same two files, across three heads. The redaction
work in `2c6e7dc` neither added nor closed any: it touched different files. See open item 22.

## T131 — verification on the clean tree, before the re-review fixes

Run on **2026-08-16** against commit **`4d8711c`**, with the working tree empty before and after —
both recorded in the run's own log, because a verification run that edits the tree it is verifying
is void, and this has happened once in this phase.

| Check | Toolchain | Result |
|---|---|---|
| `cargo xtask verify` | 1.94.0 (pinned) | **11 of 11**, exit 0 |
| `cargo xtask verify` | 1.97.1 (current stable) | **11 of 11**, exit 0 |
| `cargo check --locked -p renvor --no-default-features --all-targets` | 1.94.0 | exit 0 |
| `cargo check --locked -p renvor --no-default-features --all-targets` | 1.97.1 | exit 0 |
| `cargo deny check licenses advisories bans sources` | — | **advisories ok, bans ok, licenses ok, sources ok** |
| Quickstart gates 0–15, individually | 1.94.0 | **16 of 16** — see T125 |
| Workspace tests | both | 27 targets, 0 failing |
| `HEAD` before and after | — | `4d8711c` both times |
| Working tree before and after | — | empty both times |

**0 failing. 0 silently skipped.** Every step's result is printed by the runner and counted; a step
that could not run is a failure by construction (FR-023), not a skip.

### What this run does and does not cover

It covers the **code and gates as of `4d8711c`**. The commit that records this table is
documentation-only and changes no code, no manifest, and no gate — but it is, strictly, not the
commit that was verified locally. That gap is closed by **CI on the pull request**, which runs the
same `cargo xtask verify` entry point against the actual final head on Ubuntu, the claimed platform.

Stating it this way rather than claiming the final commit was locally verified: a ledger that
records a run against a commit that did not exist when the run happened is the same class of
prospective claim the W-005 review caught at Q6-1.

### Scope and publication, re-verified at the same commit

| Statement | How it was checked |
|---|---|
| **0** crates published | Sparse index, 4 of 4 phase crates return 404, with a control returning 200 for `serde` |
| **0** tags | `git tag --list` empty; `gh api repos/renvor-rs/renvor/tags` returns 0 |
| **0** releases | `gh release list` empty, with an authenticated `gh` |
| **Phase 003 not begun** | `specs/` contains `001-governance-foundation` and `002-core-kernel` only; 0 branches naming 003 |
| Commit signatures | Every commit authored in this branch verifies `G`. The one `E` is GitHub's own web-flow merge commit for PR #18, whose key is not in the local keyring |

## T132 — pull request #19, and the one check that failed

Opened **2026-08-16** into `main` at `19605e9`, non-draft, mergeable, 112 changed files.

> **This section records observations taken at head `2cd0530`.** The head has since advanced —
> `bf70553` recorded these results, and a further commit closes the verification re-review's
> findings. The check *conclusions* below were re-confirmed on `bf70553` and are unchanged; the
> diff totals move with each commit and are therefore not restated here.
>
> Caveated because the W-005 verification re-review (N2) is right that the T131 section carries
> exactly this caveat and this one did not, so it read as observed-at-final-head when it was
> observed one commit earlier. Same class as Q6-1, smaller claim.

### Check results

| Check | Required? | Result |
|---|---|---|
| `verify (1.94.0)` | **yes** | pass |
| `verify (stable)` | **yes** | pass |
| `security` | **yes** | pass |
| `docs` | **yes** | pass |
| `dependency-review` | no | pass |
| `package and verify without publishing` (release-dry-run) | no | pass |
| `Analyze (actions)` | no | pass |
| `Analyze (rust)` | no | pass |
| `attest rehearsal artifacts` | no | **skipping** — `workflow_dispatch` only, by design |
| `CodeQL` | no | **FAIL — 9 high alerts** |

**All four required checks pass.**

### The release-dry-run rewrite is verified in CI, not only locally

T120's new control printed `detector fires on an ignored generated file that git cannot see`: the
planted file in `target/` was caught by the `find`-based diff **and** confirmed invisible to
`git status`. That is the defect T120 fixed, demonstrated on the platform that matters.
`CARGO_TARGET_DIR` resolved outside the checkout, five crates packaged, and
`aborting upload due to dry run` appeared **four** times — four crates staged, **0 published**.

### CodeQL: 9 high `rust/cleartext-logging` alerts — *as observed at `2cd0530`*

> **AMENDED 2026-08-16 (T136). The analysis below was recorded at this head and was wrong on two
> points of fact.** It is kept because it is what was observed and concluded at the time; the
> corrections follow it, and the resolution is recorded in the final section of this document.
> Nothing here describes the current state.

| Location | What CodeQL saw |
|---|---|
| `crates/renvor/examples/configuration.rs:99–102` (4) | a value named `password` flowing into `println!` |
| `crates/renvor-config/src/secret/mod.rs:155–157, 179, 183` (5) | a secret flowing into `format!`, inside `#[cfg(test)]` |

**Measured, not argued.** Running the flagged example produced:

```text
  Display  : [redacted]
  Debug    : Secret { key: "password", value: "[redacted]" }
  in a msg : the password is [redacted]
  expose() : 7 characters
```

The credential appeared **0** times in the example's output.

**CodeQL is right about the dataflow and wrong about the consequence** — for three of the nine.
A value derived from a field named `password` does reach `println!`; it passes through
`Secret<T>`'s redacting `Display` on the way, and CodeQL does not model that a `Display` impl can
sanitise.

### The two errors in the paragraph above, corrected at T136

**Error 1 — "all nine are false positives" was wrong for six of them.**

| Alert | Line | Verdict at T136 |
|---|---|---|
| #1 | `configuration.rs:99` | **False positive.** `Display` renders `[redacted]` |
| #2 | `configuration.rs:100` | **False positive.** `Debug` renders the key and `[redacted]` |
| #3 | `configuration.rs:101` | **False positive.** Embedded formatting uses the same `Display` |
| #4 | `configuration.rs:102` | **REAL, in the sense that matters.** It printed `password.expose().len()`. A length is not the value, but it *is* a fact about the credential, and a static analyser reading the file cannot tell a length from the value. **Fixed in source**: the line prints a fixed message and nothing derived from the secret |
| #5–#7 | `secret/mod.rs:155–157` | **REAL as diagnostics.** Each was `assert!(…, "{rendered}")`. On a pass they print nothing; on a *redaction regression* the panic message is the leaked string. The one run that proves a leak exists would have been the run that published it. **Fixed in source**: fixed diagnostics naming which check failed |
| #8 | `secret/mod.rs:179` | **REAL as a diagnostic.** `"a field route leaked the value: {rendered}"` — same shape. **Fixed in source**: `"field route {n} leaked the value"`, identifying the route by index |
| #9 | `secret/mod.rs:183` | **REAL as a diagnostic.** `assert!(rendered.contains(REDACTED), "{rendered}")`. **Fixed in source**: a fixed message |

Calling all nine false positives was the comfortable reading. Six were genuine defects in test and
example *diagnostics* — not leaks on the passing path, but leaks on the failing one, which is worse
because that is the path a person reads.

**Error 2 — the positive control is at line 188, not line 183.** The paragraph above described
line 183 as "the positive control that formats the raw test constant deliberately". Line 183 was
`assert!(rendered.contains(REDACTED), "{rendered}")`. The actual raw-credential control is
**line 188**, `assert!(format!("{CREDENTIAL:?}").contains(CREDENTIAL))` — and CodeQL never flagged
it, because it has no format argument for the taint tracker to follow. The line that was defended
as a deliberate control was in fact one of the six defects, and the line that really is a
deliberate control was never in the alert list at all.

**A corollary worth stating: the nine were a floor, not a ceiling.** `assert_eq!` generates its
panic message from both operands, so `assert_eq!(secret().to_string(), REDACTED)` leaks on
regression exactly as `"{rendered}"` does — and CodeQL flagged none of those, because it models
explicit format arguments and not macro-generated ones. Three such assertions were found and fixed
by reading, not by the scanner.

### "No fix is available" was also wrong

The previous version of this section headed a table *"Why this is reported rather than resolved:
no fix is available inside this workflow's authorization."* Six of the nine had a fix available in
source the whole time, and it was a small one. The table's four rows were answers to the question
*how do we make the alerts go away*; the right question was *is each alert pointing at something*.

What survives of that table is narrower and still true: for #1–#3 there is no source fix that does
not destroy the evidence.

| Option for #1–#3 | Why not |
|---|---|
| Restructure the example so no raw `String` exists | Would need `Deserialize` for `Secret<T>`, which the crate does **not** implement — deliberately, since `Serialize` is forbidden by C-C9. New public API belongs in its own decision |
| Delete or weaken the demonstrations | The example is the evidence for **FR-018, FR-021, SC-007, and SC-016**. Removing it to satisfy a static analyser trades real evidence for a green square |
| Add a CodeQL config file excluding examples | CodeQL runs here via **default setup**, which honours no in-repository config |

So #1–#3 are dismissed as false positives, individually and with a stated reason, and #4–#9 are
fixed in source.

**CodeQL is treated as a W-001 cleanliness gate.** It is not one of the four required status
contexts — `verify (1.94.0)`, `verify (stable)`, `security`, `docs` are — and it is tempting to
treat a non-required red check as cosmetic. W-001 requires the checks to be *clean*, not merely
*passing where required*, and six of these nine were pointing at real defects, which is the
argument against treating any of them as cosmetic. The alerts are worked to zero.

## Post-review corrections (T133–T142)

A second audit on **2026-08-16**, this time of the **open pull request** at `a7643c5`, found a
layer of defects that Phase 10's corrections had introduced or left standing. Six previously
checked tasks were reopened.

**Phase 10's pattern was a check that never executed what it claimed. Phase 11's is a record that
summarised away the thing it was recording.** Four instances, and they are the same mistake:

| Where | The summary | What it replaced |
|---|---|---|
| W-005 MINOR dispositions | four thematic group rows | 24 findings, of which **5 had no row at all** |
| W-005 re-review | "the security re-review's remaining four … ACCEPTED" | **two MAJOR findings still live on public API** |
| Requirements review total | "1 CRITICAL, 7 MAJOR, 17 MINOR", copied from the reviewer's closing line | its own table, which has **18** MINOR rows |
| CodeQL | "all nine are false positives" | three false positives and **six real diagnostic defects** |

None of these is a lie about an outcome. Each is a summary that was easier to write than the thing
it stood for, and in every case the harder version was available — the review deliverables were on
disk the whole time, the reviewer's own table was one scroll below its own total, and the six
CodeQL alerts each pointed at a line that would print a credential on a test failure.

### A — the CodeQL sinks, and what the mutation test showed (T133)

Alerts #4–#9 were fixed by removing secret-derived output. The proof is a **mutation test**, not a
reading: `Display` was broken to render the value, then `Debug` was, and the suite was run each
time.

| Mutation | Tests that failed | Occurrences of the credential in the output |
|---|---|---|
| `Debug` renders the value | `debug_names_the_key_and_redacts_the_value`, `every_route_…` | **0** |
| `Display` renders the value | `display_renders_the_placeholder`, `every_route_…`, `serialization_is_refused_…` | **0** |

Failures are reported as *which check failed* — `"Debug omitted the placeholder"`, `"field route 1
leaked the value"` — never as *what leaked*.

**Three further leaks were found by reading that the scanner did not flag**, all `assert_eq!`:
`assert_eq!(secret().to_string(), REDACTED)`, `assert_eq!(secret().expose(), CREDENTIAL)`, and
`assert_eq!(reported, 0)`'s siblings. `assert_eq!` builds its panic message from both operands, so
the obvious way to write a redaction test is itself the leak. CodeQL models explicit format
arguments and not macro-generated ones, which is why the nine alerts were a **floor** on this
defect class rather than a ceiling.

The example now prints `expose() : access is explicit; the value is not printed` and calls
`expose()` nowhere. A length is not the value, but it is a fact about it, and a static analyser
reading the file cannot tell one from the other.

### B — two MAJOR security findings that a group row had absorbed (T139)

Both were reproduced before being fixed, and one was reproduced from **outside the crate**, against
the published API, in a scratch package taking `renvor-config` as a path dependency:

| ID | Reproduced | Fix | Residual |
|---|---|---|---|
| SV-N1 | `decode_single` with a 3,000-segment key: `fatal runtime error: stack overflow`, exit 134 | depth checked at the public boundary; `MAX_KEY_DEPTH` re-exported from the crate root | none |
| SV-N2 | a FIFO with **no writer** blocks in `File::open`; a **slow writer** blocks in `read_to_end`. Both still blocked at 35 s | a non-regular path is refused on the `metadata` already read — a `stat` never opens the path, so the refusal precedes the blocking open | **open item 24**: check-then-open is racy |

The reviewer proposed putting SV-N1's ceiling in `nest`. That would have protected nothing: `nest`
is a `pop` loop and is iterative. The depth is consumed by `try_into`'s recursive descent and again
by the nested table's recursive `Drop`, so the guard belongs at the entry point, not at the
constructor. Recorded because the difference is easy to miss and the wrong fix would have passed
review.

The FIFO test now covers **all three** variants on a worker thread under a 10-second timeout — a
hang is not an assertion failure, it is a test that never returns, so the bound has to be in the
harness — with a regular-file positive control proving the refusal discriminates. It also no longer
skips silently when `mkfifo` is absent (RV-N15): `mkfifo(1)` is POSIX, so its absence is a broken
environment and is reported as one.

### C — Gate 12 was bash-only, and the history of why was wrong (T138)

The gate discovered its examples into a scalar and iterated with `for name in $EXAMPLES`. Measured:

| Shell | Iterations | Consequence |
|---|---|---|
| bash 3.2 | 3, one per example | works |
| zsh 5.9 | **1**, with all three names glued into one string | `cargo run --example` fails on the argument |

The comment two lines above it explained that `mapfile` was avoided *for portability*. The
replacement was bash-only in a different way, in a repository whose default shell is zsh.

**And the recorded history of the original defect was wrong in the flattering direction.** It said
the glob matched a directory that does not exist and that the loop ran zero times and passed:

| Claim | Measured |
|---|---|
| "a directory that does not exist" | `examples/` exists, is tracked, holds `.gitkeep` and a README. It holds no `.rs` file |
| "the loop body never executed once" | bash leaves an unmatched glob **literal**; the body ran once with `f=examples/*.rs` |
| "the gate reported a pass having run nothing" | `cargo run --example '*'` exited non-zero; the script died with **101** in bash and **1** in zsh |

The gate **failed loudly, in both shells**. Locating the fault in the shell was more comfortable
than locating it where it was: a pass had been recorded for a script that could not run to
completion.

### C2 — the wait-inventory gate fired on a comment, and that was a defect in the gate (T139)

SV-N2's fix added a comment to `renvor-config/src/layer/file.rs` explaining that
`ApplicationBuilder::build` runs `source.load()` inside `bounded_call`. `deadlines.rs` matches
**text**, so it read that prose as a call into author code and demanded the file bound something.

The tempting fix is to reword the comment. That is hiding from the check. The gate now strips
line comments before matching, and the correction was mutation-tested **in both directions**:

| Planted in `renvor-config/src/gate_probe.rs` | Gate |
|---|---|
| `pub fn probe(source: &dyn ConfigSource) { let _ = source.load(); }` — a real call | **FIRES**: "holds a handle to author-implemented code, bounds nothing itself" |
| `// A caller runs source.load() inside bounded_call;` — the same token, in a comment | **passes** |

The strip makes the gate **stricter**, not laxer, in both directions: a bounding construct that
appears only in a comment no longer satisfies the check either. A real call survives, because a
real call is not preceded on its line by `//`. Both probes were removed and the tree verified
clean afterwards.

### C3 — ten gates were selecting tests with a filter that could not match them (T140)

Found by re-running all sixteen gates individually and **counting what each one executed**, rather
than reading their exit statuses. Four gates ran **zero tests** and reported a pass.

Every affected gate selected tests with a `<file>::` filter. That is a **module path**, and an
integration test in `tests/<file>.rs` compiles to its **own binary** whose test names carry no
module prefix. `libtest` exits 0 when a filter matches nothing, so a gate that selected no test at
all was byte-for-byte indistinguishable from one where everything passed.

| Gate | Criterion | Filter | Ran before | Runs now | What was being skipped |
|---|---|---|---|---|---|
| 2 | SC-001, SC-002 | `lifecycle::` | 31 | **47** | `tests/lifecycle.rs`, `tests/lifecycle_edges.rs` — the rollback-order assertions the gate is named for |
| 3 | SC-020 | `layering::` | **0** | **19** | the entire 8-obligation proof gate |
| 4 | SC-007, SC-016 | `redaction::` | **0** | **18** | every redaction test in both crates |
| 5 | SC-005, SC-021 | `provider::graph::` | **0** | **7** | the ceiling and work-budget counters |
| 6 | SC-006 | `drain::` | 9 | **20** | `tests/drain.rs`, including the zero-budget case |
| 7 | SC-008 | `health::` | 6 | **15** | `tests/health.rs`, the disagreement proof |
| 8 | **SC-009** | `injection::` | 2 | **13** | **`tests/injection.rs` — all 21 phase-and-behaviour combinations** |
| 9 | SC-019 | `observe::run_id::` | 6 | **12** | `tests/run_id.rs` |
| 10 | FR-029 | `observe::bootstrap::` | 2 | **3** | `tests/observe_bootstrap.rs`, the install-after-build proof itself |
| 11 | FR-038 | `hostile::` | **0** | **8** | every hostile-input test |

**Gate 8 is the one that matters most.** SC-009 is the 21-combination requirement, its assertions
are in `crates/renvor-testkit/tests/injection.rs`, and the gate cited as its evidence ran four
harness unit tests instead. The tests themselves were correct and were passing in
`cargo test --workspace` the whole time — what was false was the *gate's* claim to have run them.

The fix is not just corrected selectors. A `run_tests_expecting <minimum>` helper in the shared
Setup preamble now counts what libtest reports and fails below the minimum, so a selector that
stops matching is a failure rather than a silent pass. Mutation-tested:

| Form | On a filter matching nothing |
|---|---|
| `cargo test -p renvor-config this_matches_nothing::` | `test result: ok`, **exit 0** |
| `run_tests_expecting 8 -p renvor-config this_matches_nothing::` | `tests executed: 0 (minimum 8)`, **exit 1** |

Gates 2–11 now execute **162** tests between them, against **56** before — of which four gates
contributed nothing at all.

> **The first version of this sentence said 62, and the delta review (D7-1) caught it.** The
> per-gate figures in the table above are correct and sum to 56 (`31+0+0+0+9+6+2+6+2+0`); 62 was
> written from memory rather than from the column beside it. A total that does not derive from its
> own table is the exact defect this section exists to record, recurring inside the record of it.

### D — the dependency inventory disagreed with its own table (T134)

The table was correct and live-verified. The **prose** around it said 55 external packages in three
places after the figure became 48, and one summary row labelled the 38 transitive packages
"evaluated by nobody" — a different set, differing by `zeroize`, which research §3 evaluated as
`secrecy`'s dependency. 37 and 38 are both right and measure different things; the document used
one label for both.

Gate 15's new **15f** derives every figure from `cargo metadata` and from research §3's own
candidate table, then requires the document to state it. A missing row fails as loudly as a wrong
one. Three planted controls — a stale prose total, a wrong summary row, a deleted row — are each
required to be caught.

### C4 — the W-005 delta reviews, round 3 (T139)

Two more clean-context reviews, **NON-INDEPENDENT and ADVISORY**, run against `976648e` and scoped
to the two commits this phase had just produced. Both delivered to disk, both returned enumerated
findings.

| Delta review | Findings |
|---|---|
| Requirements | **0 CRITICAL, 3 MAJOR, 8 MINOR** — D0-1 … D7-6 |
| Security | **0 CRITICAL, 4 MAJOR, 6 MINOR** — S1-1 … S6-2 |

**This round paid for itself twice over, and both times on the same mistake.**

**S1-1 — I fixed the files I was pointed at, again.** The round-2 re-review had already caught this
exact shape: security finding 1.1 named `ResolvedConfig<T>`, four sibling types kept the defect.
T133 then removed secret-derived diagnostics from the three files the CodeQL alerts pointed at —
and left **24 sites of the identical defect in five other files**. The reviewer proved it by
execution: breaking `Secret`'s `Display` printed ``the credential reached the `Display` path:
"hunter2-do-not-print"`` from `renvor-config/tests/redaction.rs`, and reverting `from_decoder`
printed it from `renvor-core/src/error/context.rs`. In the same run, `leak_separator.rs` — one of
the three files T133 *did* fix — also failed, and printed nothing. The technique worked; it had
simply not been finished.

**The response is a gate, not a longer list.** `crates/renvor-core/tests/diagnostics.rs` discovers
every file in the workspace that handles a credential needle and fails if any assertion diagnostic
interpolates anything but an allowlisted label or index. An allowlist, not a denylist: the failure
mode is *a binding nobody anticipated*, and only an allowlist fails closed on one. It carries three
controls, including a negative one, because a gate that fires on the fix is a gate that gets
deleted.

Re-running the reviewer's own mutation afterwards:

| | Before T139 | After |
|---|---|---|
| Failing tests under the Display/Debug/`from_decoder` mutation | 12 | 12 |
| Occurrences of `hunter2` in the failure output | **1** | **0** |
| Occurrences of `s3cr3t-token`, `LEAKED-TAIL`, `do-not-print` | — | **0** |

**S2-1 and S2-2 — the depth guard bounded the key and not the value.** SV-N1's fix counted
`key.split('.')`. The recursion runs over the key **and** the value, so a shallow key with a
1,575-deep value still aborted at exit 134, and `decode_source` — which is what
`LayeredResolver::resolve()` calls — had no depth check of any kind. The ledger had recorded
SV-N1's residual as *"none"*.

`MAX_VALUE_DEPTH = 128` now guards both entry points, measured iteratively so the measurement
cannot overflow on the input it exists to refuse. 128 sits between two measured numbers: the TOML
parser refuses its own nesting at **81**, so nothing readable from a file or an environment
variable is newly refused, and the descent overflows at about **1,575**. A caller-constructed
`toml::Table` skips the parser, which is exactly why the gap mattered.

**The three MAJOR requirements findings were all claims running ahead of reality**, and one of them
is pointed: D7-1 found *"162 tests, against 62 before"* where the table beside it sums to **56**. A
total not derivable from its own table — inside the section recording that defect. D6-1 found the
Status paragraph naming a different open set from `tasks.md`, for the third time; that paragraph no
longer names one. D7-2 found open item 22 struck through as CLOSED, asserting the CodeQL alerts
"are dismissed", while **0** were dismissed and the check was still red.

### Round 3, requirements delta — all 11 findings

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| D0-1 | MINOR | The repository was **modified during** this read-only review: an uncommitted section appeared in the ledger at 21:34. The reviewer re-anchored its documentary findings to the committed blob and said so | **ACCEPTED, and my fault.** Editing the subject of a running review invalidates exactly the property that makes it evidence. The reviewer's handling was correct; the practice is recorded so a future round holds the tree still |
| D3-1 | MINOR | SV-N1's residual was recorded as **"none"**, but the guard counts key segments and the value is uninspected | **FIXED**, with S2-1 — and the residual row corrected. Not attacker-reachable through a file or the environment, so the *security* claim stood; "residual: none" did not |
| D5-1 | MINOR | Gate 12's 12e proves "unchanged" with `git status --porcelain`, which consults `.gitignore` — the blind spot T120 removed from `release-dry-run.yml`. The planted control uses a git-visible path, so the blind region is never exercised | **ACCEPTED, and the claim narrowed.** The gate now states what it establishes: no tracked or untracked-but-visible file changed, which is weaker than "byte-identical". **Open item 29** |
| D6-1 | **MAJOR** | The Status paragraph named the open set as T140/T141 while `tasks.md`, declared authoritative in the same sentence, marked T128/T132/T141. Third occurrence of the RV-N1 drift | **FIXED by deletion.** The paragraph no longer names an open set at all. It named one three times and was wrong twice; the remedy that kept failing was remembering to update it |
| D6-2 | MINOR | `tasks.md` marks T131/T140 `[x]` covering a full matrix the ledger records only at older commits | **FIXED at T140**, whose matrix is recorded at the final head |
| D7-1 | **MAJOR** | *"162 tests … against **62** before"*, stated twice, where the table beside it sums to **56**. Re-measured from `a7643c5`: 56 | **FIXED.** Both occurrences say 56, with the arithmetic shown. The defect class this section documents, recurring inside the documentation of it |
| D7-2 | **MAJOR** | Open item 22 struck through as CLOSED, asserting *"#1–#3 are dismissed"*, while **0** were dismissed and CodeQL was still failing | **FIXED.** The item is open, describes the live state, and will not be struck until GitHub reports 0 open alerts |
| D7-3 | MINOR | T141 says *"dismiss only #1, #2, #3"*; #2 and #3 are now `fixed` and the live open alerts are **#1, #10, #11** | **FIXED.** T141 names the three by content and by their live numbers, with the renumbering explained |
| D7-4 | MINOR | Gate 12's Pass paragraph claims *"each of the five proven by a control"*; there are four controls, and two other lines in the same file say four | **FIXED.** Four are controlled; the fifth is the gate's main assertion, and the paragraph says so |
| D7-5 | MINOR | The comment-strip is `split_once("//")`, not a comment parser: it truncates a line at a `//` inside a string literal, and does not strip `/* … */` at all. One production line is truncated today | **ACCEPTED, with both limits named in the code.** They fail in the stricter direction for bounding constructs and the laxer direction for author calls, so neither is safe to forget. **Open item 30** |
| D7-6 | MINOR | The Summary-of-gates table still showed gates 6 and 9 as zero-asserting with control **"—"**, against the file's own rule | **FIXED.** Gates 6, 7, 8, and 9 name their controls |

### Round 3, security delta — all 10 findings

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| S1-1 | **MAJOR** | The secret-diagnostic removal reached 3 files and left **24 sites of the identical class in 5 others**. Proven live by mutation | **FIXED**, and made a property: `tests/diagnostics.rs` discovers every credential-handling file and fails on any interpolating diagnostic. Re-running the reviewer's mutation now yields **0** credential occurrences |
| S1-2 | MINOR | A bare `assert!` renders its own expression source, which contained the credential literal — so the example still printed `hunter2` on a regression, in a file the same diff had edited | **FIXED.** The assertion carries a message, and its needle is assembled from halves so the expression no longer contains it either |
| S2-1 | **MAJOR** | The depth guard counts `key` segments; the recursion is over key **and** value. `decode_single("a", <1575-deep value>)` still aborted, exit 134 | **FIXED.** `MAX_VALUE_DEPTH`, measured iteratively, checked before the clone |
| S2-2 | **MAJOR** | `decode_source` had **no depth guard at all**, and it is what the shipped resolver calls: abort at depth 1,575 through public API | **FIXED**, same guard, at every entry point. Reachability caveat confirmed: the TOML parser caps at 81, so no file or environment variable reached it — a caller-built `Table` did |
| S2-3 | MINOR | Three effective ceilings are in force — 32 (env key), ~81 (file, via the parser only), unbounded (caller-supplied) | **CLOSED by the S2-2 fix** for the third; the first two remain and are now both Renvor's, named. **Open item 21** covers the numbers |
| S3-1 | **MAJOR** | The check-then-open TOCTOU is trivially winnable: **101 attempts, 1,517 ms**. `ApplicationBuilder::build` containment verified and **holds** | **ACCEPTED as already-declared open item 24**, now with the reviewer's measurement in it rather than a qualitative "racy". Closing it needs `O_NONBLOCK`, hence `libc`, hence an FR-040 inventory row — a scope change this phase does not take |
| S3-2 | MINOR | Legitimate cases refused: `/dev/null` as an empty source, `/dev/stdin` when stdin is a pipe, process substitution. And the message blames indefinite blocking, untrue of `/dev/null` | **Message FIXED** — it states the rule and why the class is refused, rather than a cause that does not apply to every member. **The refusal itself is kept**, because allowing character devices readmits `/dev/zero`. **Open item 31** |
| S4-1 | MINOR | The constant and the two `"32 levels"` messages were independent: setting `MAX_KEY_DEPTH = 8` left both saying 32, and the new test still passed because it asserted the literal | **FIXED.** A `Constraint::TooDeep { maximum_depth }` variant carries the number, so the message *is* the constant, and both tests read the constant instead of a literal |
| S6-1 | MINOR | The depth refusal reproduces the key verbatim: a 9,999,999-byte key becomes a 10,000,269-byte error | **ACCEPTED. Open item 25**, now carrying the measurement |
| S6-2 | MINOR | `read_process_environment` names the lossy form of a non-Unicode variable name, expanding each invalid byte to U+FFFD: a 200,007-byte name produced a **600,221-byte** error — an exact **3.00×** amplification | **ACCEPTED. Open item 25.** Pre-existing and not in the delta, recorded because the amplification factor was not previously known |

**No finding at HIGH or above is refused, across all three rounds.** That is now a claim about
**82** individually enumerated findings (26 + 12 + 19 + 4 + 11 + 10).

### D2 — final cross-artifact analysis (T140)

Re-run against the final artifacts. Every check is **mechanical** — extracted and compared by
script, not read — because the previous round's defects were all cases of a document agreeing with
itself in prose while disagreeing with a machine-readable fact.

| Check | Result |
|---|---|
| `spec.md` identifiers | FR-001…FR-044 (**44**), SC-001…SC-022 (**22**) |
| Evidence-map rows | **44** FR, **22** SC, **0** duplicates, **0** missing, **0** extra, **0** empty cells |
| Requirements with no mention in `tasks.md` | **0** |
| Task ledger | **141** rows, T001–T141, **0** gaps, **0** duplicate IDs, 138 checked + 3 open = 141 |
| Quickstart gate headings vs the Summary-of-gates table | **16** and **16**, identical sets |
| Numeric bounds (1024, 8192, 2048, 16384, 18432) across spec / plan / research / data-model | present in all four, no contradiction |
| Unresolved placeholders (`TODO`, `TKTK`, `???`, `FIXME`) | **0** |
| Terminology: "21 combinations" | consistent, and the `Drain` limit is stated wherever the claim appears |

**0 CRITICAL, 0 HIGH, 0 MEDIUM.** The findings this round came from *executing* the artifacts —
counting the tests each gate ran, running Gate 12 in a second shell, reproducing two security
findings from outside the crate — not from cross-reading them. That is the lesson worth carrying:
the cross-artifact analysis was clean at `a7643c5` too, while four gates were running zero tests.

### E — the workflow-quality defects (T135, T137)

`actionlint -no-color` exited 1 on SC2086: `printf '%s\n' $CRATES` relied on word splitting.
**Quoting it would have been the wrong fix** — the whole list would print as one line and the
sorted comparison below would go on "passing" while comparing something else. `read -ra` splits
deliberately. A positive control re-introduces the unquoted form and confirms actionlint still
detects it.

The release workflow's MSRV comment claimed the toolchain was "taken from rust-toolchain.toml
rather than restated here" while passing `toolchain: "1.94.0"` — the reverse of the mechanism. It
now says what is true, including that **nothing verifies** the three declaration sites agree.

### F — the CodeQL disposition, and what GitHub reports (T141)

Performed at head `68dcc32`, against the live API rather than against a plan.

**Precondition, checked before anything was dismissed.** Alerts **#4–#9** are `fixed` — GitHub's own
state, not an inference from the diff. Nothing was dismissed until that was true, because dismissing
is how a real defect gets closed by mistake.

| Alert | Line | Verdict | Disposition |
|---|---|---|---|
| **#1** | `configuration.rs:99` — `Display` | False positive | **dismissed**, `false positive`: *"Custom sanitizer not modeled by CodeQL: `Secret<T>::Display` always emits `[redacted]`…"* |
| **#10** | `configuration.rs:100` — `Debug` | False positive | **dismissed**, `false positive`: *"…`Secret<T>::Debug` emits only the key and `[redacted]`…"* |
| **#11** | `configuration.rs:101` — embedded `Display` | False positive | **dismissed**, `false positive`: *"…embedded formatting uses `Secret<T>::Display`…"* |
| **#2–#9** | — | #2 and #3 re-fingerprinted; #4–#9 real | **fixed in source**, or closed by GitHub as `fixed` |

**The numbers moved under the work, and that is worth recording rather than smoothing over.** The
three false positives were #1, #2, #3 when the plan for this task was written. Changing
`configuration.rs` caused CodeQL to re-fingerprint the file: it closed **#2** and **#3** as `fixed`
and opened **#10** and **#11** in their place. Lines 99, 100, and 101 are byte-identical to what
they were; the rule, path, line, message, and severity all match. #10 and #11 are #2 and #3 under
new numbers.

Each was therefore matched **by content** before being dismissed — same rule, same file, same line,
same rendering, verified by running the example — rather than by trusting a number the platform had
reassigned. Dismissing #10 and #11 on the strength of the numbers alone would have been the same
mistake as calling all nine false positives.

**Re-verified at dismissal time, by execution:**

```text
  Display  : [redacted]
  Debug    : Secret { key: "password", value: "[redacted]" }
  in a msg : the password is [redacted]
```

0 occurrences of the credential in the example's output.

**Live state after the dispositions**: `gh api …/code-scanning/alerts?ref=refs/pull/19/head` reports
**0 open**, 8 `fixed`, 3 `dismissed`. **11 review threads, 0 unresolved** — GitHub resolved the
github-advanced-security threads itself as their alerts closed.

**CodeQL was treated as a W-001 cleanliness gate**, though it is not one of the four required status
contexts. The temptation was to call a non-required red check cosmetic; six of the nine were
pointing at real leak-on-failure diagnostics, which is the argument against ever doing that.

## Merge-blocking corrections (T143–T158)

Phase 11 left PR #19 with ten green checks, **0** open CodeQL alerts, and **0** unresolved
conversations. It was still not mergeable, and reviewing the *closure records* rather than the code
is what showed why.

**This is the third distinct failure pattern in this branch, and it is the least comfortable.**

| Round | Pattern |
|---|---|
| Phase 10 | checks that **never executed** what they claimed |
| Phase 11 | records that **summarised away** the thing they were recording |
| Phase 12 | **accurate records of things that should not have been accepted** |

Every item below was already written down, correctly, in the PR body's named-limitations list or in
the Dependabot alert set. Nothing was hidden and nothing was wrong. Four of them contradicted a
MUST anyway. **Writing a defect down does not discharge it** — a limitation is a thing a reader
must accept, and a reader cannot accept "a public API can be blocked for ever by a substituted
FIFO" on a kernel whose contract is bounded calls.

### A — the check-then-open race was a defect, not a residual (T143)

Named limitation 5 stated it exactly: *"The FIFO refusal is check-then-open, and the race is
winnable — measured at 101 attempts, 1,517 ms."* It then argued the scope cost — a direct `libc`
dependency and an FR-040 row — was more than the phase should take.

That trade was wrong in both directions.

**The cost was near zero.** `libc` was already in the resolved graph, arriving as
`renvor-core → getrandom 0.4.3 → libc 0.2.189`. Promoting it to a direct dependency of
`renvor-config` adds **no package at all**: the lockfile held **48** external packages before the
change and **48** after, and the entire `Cargo.lock` diff is one line adding `libc` to
`renvor-config`'s dependency list.

**The exposure was on public API.** `ApplicationBuilder::build` contains the residual through
`bounded_call`, and that containment was verified — but `FileLayer::read()` is public, documented,
and callable directly, and nothing wraps a direct caller.

#### The fix is structural rather than a narrower window

| Before | After |
|---|---|
| `std::fs::metadata(&self.path)` — resolves the **pathname** | `file.metadata()` — `fstat` on the **open descriptor** |
| `std::fs::File::open(&self.path)` in `read_bounded` — resolves the pathname **again** | the descriptor is **moved in**; nothing is reopened |
| two lookups, with a winnable gap between them | **one** lookup; there is no second one to race |

The open uses `O_NONBLOCK` so that opening a FIFO cannot block *before* the type is known — the
ordering problem that made the old check-then-open shape look necessary. Only the integer constant
comes from `libc`; the open itself is `std::fs::OpenOptions` with
`OpenOptionsExt::custom_flags`, so **no `unsafe` block and no FFI call appears in Renvor's
source**, and `unsafe_code = "forbid"` is not relaxed.

#### The race test, and the mutation that proves it is a test

`replacing_the_path_between_check_and_open_cannot_block` points a symlink alternately at a regular
file and a FIFO, re-pointing it with `rename` so the path is never absent, while a worker calls
`FileLayer::read()` 400 times.

| Code under test | Result |
|---|---|
| T143's fix | **passes** — 400/400 attempts return |
| check-then-open restored by mutation | **fails** — `read()` did not return within **30 s** |

It also carries a positive control requiring the run to have observed **both** outcomes — some
attempts parsing the regular file, some refusing the FIFO. Without it a run in which the swapper
never won would pass having reproduced nothing, which is precisely how the original defect
survived a green suite.

The three pre-existing FIFO variants — no writer, slow writer, flooding writer — still pass, as do
the ordinary and oversized regular-file cases.

### B — `libc` under FR-040 (T144)

Recorded in `governance/phase-002-dependency-inventory.md` under **T143** with version, licence
(`MIT OR Apache-2.0`, both branches on the allow list), MSRV (**1.65**, under the 1.94.0 floor),
maintenance (`rust-lang/libc`), advisories (none), and feature cost (`default-features = false`).
The alternatives are recorded with their measured cost: `rustix` would have added **three**
packages — `bitflags`, `errno`, `linux-raw-sys` — for a constant `libc` already exposes.

Two summary rows moved: *directly chosen* **10 → 11**, *arrived transitively* **38 → 37**. Both
were found by **Gate 15f**, which derives them from `cargo metadata` rather than reading them, and
both were corrected until the gate passed. `cargo deny check licenses advisories bans sources`:
**all four ok**.

### C — formatting called author code, and the gate recorded it as permanent (T145)

Named limitation 4 stated it: *"`impl Debug for dyn Provider` and `dyn ReadinessContributor` call
author methods with no deadline."* `deadlines.rs` went further and **asserted the defect's
continued existence** — a constant named `FORMATTING_CALLS_AUTHOR_CODE` listing which author
methods each impl invoked, with a test that failed if the list ever changed.

That is a gate pointed backwards. FR-025 and C-L7 require **0** unbounded calls into author code;
the correct response to finding two was to remove them, not to enumerate them accurately.

**Neither impl now calls any author method.** `&self` is all `fmt` receives and every fact about a
trait object is behind a trait method, so both render static text — `Provider { .. }` and
`ReadinessContributor { .. }`. Identity was not lost, only relocated: it comes from
`ResolutionReport`, `InitialisationOrder`, and `ReadinessReport`, which hold names Renvor captured
itself inside already-bounded calls.

Bounding the call instead was considered and rejected: a `Debug` impl spawning a thread per field
would make every formatted provider a scheduling event, and `fmt::Debug` has no channel for a
deadline failure except the output it is producing — so a log line would silently become a timeout
report.

The gate is now `DEBUG_IMPLS_ON_AUTHOR_TRAITS` and refuses any `impl fmt::Debug for dyn ...` whose
body contains `self.` — an allowlist-free rule, since **every** dereference of `self` in that
position is author code. Four runtime tests back it: author methods that **panic** and author
methods that **never return**, for both traits, plus a control proving the blocking fixtures
really do block.

| Mutation | Static gate | Panic test | Blocking test |
|---|---|---|---|
| pre-T145 `Debug for dyn Provider` restored | **fails**, naming the file | **fails** | **fails at 10 s** |

The other 11 tests in the suite stayed green under that mutation, so the three failures are
targeted rather than a blanket break.

### D — diagnostics were unbounded in length (T146)

Named limitation 6 carried the measurements: a **9,999,999**-byte key produced a
**10,000,269**-byte error, and a non-Unicode variable name amplified **3.00×** through its lossy
rendering. Both numbers are attacker-chosen — a configuration file and an environment variable are
the two inputs an operator most often lets someone else supply.

`MAX_IDENTIFIER_BYTES = 128` and `bounded_identifier` now bound every identifier that reaches a
diagnostic. The original byte length is reported alongside the truncation, because truncating
silently would make two different 10 MB keys produce byte-identical errors.

**Applied at the chokepoints, not at the call sites.** `KernelError::Configuration` was already
`#[non_exhaustive]`, so `configuration()` was the only route in and bounding it covered every
adapter. `ConfigurationConflict` was **not** — `renvor-config` built it with a struct literal,
carrying three attacker-influenced identifiers with nothing in a position to bound them. It is now
`#[non_exhaustive]` with a `conflict()` constructor, so the two variants describing the same
subject finally have the same construction rule.

`SourceLayer` gained a bounding `file()` constructor and **hand-written `Display` and `Debug`**.
The derived `Debug` was the more important of the two: an attribution report is a struct of
`SourceLayer`s and is far more likely to be logged with `{:?}` than formatted field by field, so
bounding only `Display` would have covered the rarer path.

The environment layer takes a **window** off the raw bytes *before* the lossy conversion, so the
3× expansion applies to at most a window rather than to a 10 MB name. The window is at least one
byte longer than the prefix, which keeps `starts_with` deciding on exactly the bytes it always did
— asserted in both directions, including a prefix longer than the ceiling.

| Proof | Result |
|---|---|
| 128 KB key vs 1.28 MB key, rendered | sizes differ by **≤ 4 bytes** |
| 100 KB vs 1 MB key, through the real file → merge → error stack | differ by **≤ 8 bytes** |
| 1 MB astral-plane key (4-byte characters) | bounded; byte length reported |
| 500 KB environment variable name, through the real stack | bounded |
| 200 KB file path (source attribution) | bounded |
| 3 MB unrepresentable name, worst case (every byte invalid) | bounded; control confirms the fixture really amplifies 3× |
| **ordinary keys** — `port`, `server.tls.certificate_chain_path` | **unchanged, no truncation notice** |

That last row is the one that matters in practice: a bound that rewrote ordinary keys would satisfy
every size assertion above and make every real diagnostic worse.

#### The exact scope of the bound, stated rather than implied

The bound covers **configuration identifiers** — values that arrive from a TOML file, an
environment variable, or a file path, and are therefore chosen by whoever supplies the
configuration. Every output path such an identifier can reach was enumerated rather than sampled:

| Path | Covered by |
|---|---|
| TOML keys | `configuration()` / `conflict()`, the only constructors for the two variants that carry them |
| Environment variable names | the same, plus the pre-conversion window in `lossy_identifier_window` |
| Source attribution | `SourceLayer::file`, and `bounded_label` inside both rendering impls |
| Error `Display` | the constructors — the stored `String` is already bounded |
| `SourceLayer` `Display` and `Debug` | hand-written, both bounding |
| **Tracing** | **nothing to bound.** `phase_span` records exactly two fields: `phase`, a `&'static str`, and `run_id`, a fixed-length identifier. **No configuration key, layer, or path is ever recorded as a tracing field**, so there is no unbounded tracing path — verified by reading `observe/spans.rs`, not assumed |

`KernelError` has **nine** variants carrying a `String`. The two bounded here are the two that
carry configuration-derived values. The other seven — `DependencyMissing`, `CapabilityDuplicate`,
`ProviderInit`, `ProviderStop`, `DeadlineExceeded`, `ShuttingDown`, `ResourceUnavailable` — carry
provider identifiers and operation labels written by the application author in code, not supplied
through a configuration source. They are **not** bounded, and that is a scope statement rather than
an oversight: an author who constructs a `ProviderId` from configuration input at run time would
sit outside this bound. Named here so it is a known edge rather than a discovered one.

Truncation lands on a character boundary — `&raw[..n]` panics when `n` splits a character, and
SC-004 allows **0** panics on hostile input. Exercised across every offset in the boundary region.

### E — the panic-strategy ruling (T147)

Named limitation 1 said `panic = "abort"` removes provider and readiness panic containment. True,
and unactionable as written: a consumer who never read the PR body would get a kernel whose central
guarantee was silently absent.

**The maintainer ruling is that `panic = "abort"` is unsupported.** `cfg(panic = ...)` has been
stable since Rust 1.60 — well under the 1.94.0 floor — so the contradiction is refused rather than
described:

```text
$ RUSTFLAGS="-C panic=abort" cargo check -p renvor-core
error: renvor-core does not support `panic = "abort"`.

       C-L9 and SC-009 require a panicking provider or readiness contributor to be contained
       and reported as a failure. Both containments use `std::panic::catch_unwind`, which
       catches only panics that UNWIND -- under `panic = "abort"` there is nothing to catch,
       so the kernel would silently lose its central guarantee.
       ...
  --> crates/renvor-core/src/lib.rs:39:1
error: could not compile `renvor-core` (lib) due to 1 previous error
$ echo $?
101
```

The control matters as much as the guard: the same command **without** `RUSTFLAGS` compiles
cleanly, so the refusal is about the panic strategy and not about a crate that stopped building.

`contain.rs`, `contributor.rs`, `SECURITY.md`, and `SUPPORT.md` state the ruling. The cases
unwinding still cannot reach — a double panic, `process::abort`, a stack overflow — are **not**
waived by it and remain stated, because no panic strategy makes them catchable.

**One correction inside this item.** The first draft of the guard's test asserted
`cfg!(panic = "unwind")` at run time. Clippy rejected it as an assertion on a constant, and was
right for the interesting reason: the crate-root guard makes that condition structurally true, so a
build under abort never produces a test binary and no assertion inside one can observe the abort
case. It was a tautology dressed as evidence. The honest statement — that **this test running at
all is the evidence** — is now a comment, and the test asserts only what it can: that the guard is
still present in the source with the right `cfg` and the right macro.

### F — Gate 12 cleared its own cleanup trap (T148)

The gate whose stated purpose includes leaving the repository as it found it installed its trap
around the **first** of three probes and then ran `trap - EXIT` while two more were still to be
created:

```bash
trap 'rm -f "$CONTROL"' EXIT     # covers gate12-control.rs
...
rm -f "$CONTROL"; trap - EXIT    # and now NOTHING is covered
...                              # globals-probe.txt created here, trapless
...                              # gate12-leftover-probe.txt created here, trapless
```

An interrupt or an early `set -e` exit after that point left a real file in the checkout, and 12e —
the comparison that would have noticed — never ran, because the abort happened first.

One trap is now installed before the first probe exists, names every checkout probe, and is never
cleared. **12f** proves it fires after a deliberate failure and after a **SIGINT**, each in a
genuine child `sh` so `$$` is that child's own pid. A fourth control runs the identical fragment
*untrapped* and requires the file to survive — otherwise the first two would pass against a shell
that deletes nothing.

Verified in **bash 3.2** and **zsh 5.9**, both with-trap and without-trap directions.

### G — the issue template still said there was no runtime (T149)

`.github/ISSUE_TEMPLATE/bug_report.yml` told every reporter that Renvor "ships no runtime
capability yet, so most reports at this stage concern documentation, governance, or the
verification sequence" — five weeks after the kernel landed. Corrected to describe what Phase 002
actually ships, including that it has no transport, and that API instability is expected rather
than a bug.

### H — Gate 15d finally has a control (T152)

Named limitation 10: *"Gate 15's 15d has no positive control, though every other zero-asserting
check does."* Recorded accurately in two consecutive review rounds and left open in both.

15d asserts that every inventory row carries a non-empty licence, MSRV, origin, and reach. It was
written inline against one fixed path, which is *why* it had no control — a check that cannot be
pointed at a tampered copy cannot be shown to fail. Extracted into `check_inventory_rows`, it now
has one for **each** of its two branches: an **empty** licence cell and one reading **`none`**.

Gate 15 now carries **7** controls and passes in both shells.

### I — platform verification, and a claim that had gone stale (T150)

`SUPPORT.md` listed macOS and Windows as "not yet claimed", giving the reason **"No
platform-sensitive code exists to verify"**. That stopped being true when the configuration layer
landed and nobody revisited it. The kernel resolves filesystem paths, refuses non-regular files by
type from an open descriptor, opens files with a platform-specific flag, and reads `OsString`
environment names that are arbitrary bytes on unix and WTF-8 on Windows — the parts most likely to
differ, verified on exactly one platform.

A `platform` job now runs the workspace suite serially and the no-default-features check on macOS
and Windows, on both toolchains.

**It is a separate job from `verify`, and that is load-bearing.** `verify`'s matrix produces the
required status contexts `verify (1.94.0)` and `verify (stable)`. Adding an `os` dimension to it
would have renamed them to `verify (ubuntu-latest, 1.94.0)`, and branch protection matches contexts
**by name** — so the rule would have silently started requiring checks that no longer existed. The
diff to `ci.yml` is purely additive and `verify`'s matrix block is byte-identical.

| Check | Result |
|---|---|
| `platform (macos-latest, 1.94.0)` | pass |
| `platform (macos-latest, stable)` | pass |
| `platform (windows-latest, 1.94.0)` | pass |
| `platform (windows-latest, stable)` | pass |
| `verify (1.94.0)`, `verify (stable)` | pass — names unchanged |

Unix-only behaviour that cannot exist on Windows — the FIFO refusal, the non-Unicode
environment-name path — is `#[cfg(unix)]`-gated and is therefore verified on Linux and macOS only.
That is a property of the platform, not a gap in the matrix.

### J — the advisories were closed by removal, because no fix exists (T151)

PLAN.md §17.3 forbids accepting a phase with an open Critical or High finding, and the previous
revision carried **two HIGH** advisories as a named limitation on the grounds that they were npm
transitives of the documentation site, absent from the Rust graph, and pre-existing. All three
statements were true. None of them is a waiver, and §17.3 offers none.

| Alert | Severity | Package | Affected | First patched |
|---|---|---|---|---|
| #5 `GHSA-w3rx-r6r6-pgpr` | **high** | `image-size` | `<= 2.0.2` | **none** |
| #4 `GHSA-5p2g-fcmc-qvqq` | **high** | `image-size` | `<= 2.0.2` | **none** |
| #2 `GHSA-w5hq-g745-h8pq` | medium | `uuid` | `< 11.1.1` | 11.1.1 |

`image-size` 2.0.2 is the **latest published version**, so every released version is inside the
affected range and there is nothing to upgrade to. `@docusaurus/mdx-loader@3.10.2` requires it, and
3.10.2 is the latest Docusaurus, so there is no upgrade path from that direction either. Upgrading
was not an option that existed.

**Closed by removal.** An npm `overrides` entry redirects `image-size` to
`docs/vendor/image-size-disabled`, a local no-op that **throws** rather than returning fabricated
dimensions. The vulnerable parsers are never installed: `docs/package-lock.json` now resolves
`node_modules/image-size` to a link, and no published `image-size` tarball appears anywhere in it.

The stub is safe here for a reason that predates it: the site already enforced **no image input at
all** through `docs/scripts/check-image-inputs.mjs`, a build-time guard added with the original
T108 exception. The vulnerable code path was already provably unreachable; T151 removed the package
as well. The guard is retained and reworded — it now protects the *consequence* of the removal (an
image added to this site would render without dimensions) rather than a security premise.

`uuid` raised to **11.1.1** via an override. Its consumer is `sockjs`, which does
`require('uuid').v4`; uuid 11.1.1 ships a CommonJS entry through its `exports` map's `require`
condition, verified by executing `require('uuid').v4()` against the installed package.

| Verification | Result |
|---|---|
| `npm ci` (frozen install from the committed lockfile) | 1320 packages, exit 0 |
| `npm run build` (production) | `[SUCCESS] Generated static files`, exit 0 |
| `[T108]` image-input guard | ok — no MDX image embeds, no raster assets, no image imports |
| `npm audit` | **0 vulnerabilities** |
| vulnerable `image-size` / `uuid` entries in the lockfile | **none** |

**A scoping fact that must not be glossed.** Dependabot computes alerts from the **default
branch**. The fix is on `feat/phase-002-core-kernel`, so alerts #2, #4, and #5 remain `open`
against `main` until this pull request merges — they are not dismissed, not waived, and not
suppressed. The branch is measurably clean; the repository-level alert set closes on merge and not
before.

### K — validation on the clean committed tree (T156)

Every figure below was produced from the committed tree at the head this pull request now carries.

| Check | Result |
|---|---|
| `cargo fmt --all` + `git diff --check` | clean |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0** |
| `cargo test --workspace --all-features -- --test-threads=1` | 28 targets, **333 tests, 0 failing** |
| `cargo xtask verify` @ **1.94.0** | **11/11** |
| `cargo xtask verify` @ **stable 1.97.1** | **11/11** |
| `cargo check --locked -p renvor --no-default-features --all-targets` @ 1.94.0 | pass |
| `cargo check --locked -p renvor --no-default-features --all-targets` @ stable | pass |
| `cargo deny check licenses advisories bans sources` | 4/4 ok |
| `actionlint -no-color` | **0** |
| Quickstart gates **0–15, individually**, each in its own shell | **16/16**, **168** tests executed across gates 2–11 |
| Gate 12 under **bash 3.2** | PASS, 7 controls |
| Gate 12 under **zsh 5.9** | PASS, 7 controls |
| FR-001…FR-044 present in the requirement map | **44/44**, no gaps |
| SC-001…SC-022 present in the requirement map | **22/22**, no gaps |
| Task IDs T001…T158 | contiguous, unique, no duplicates |

`cargo xtask verify` covers formatting, clippy under `-D warnings`, the workspace tests, rustdoc
with warnings denied, `cargo deny`, secret scanning over both the working tree and the full commit
history, the documentation site's frozen install and production build, the link check, and
working-tree cleanliness.

#### One intermittent test failure, recorded rather than smoothed over

The **first** `cargo xtask verify` run on 1.94.0 failed at step 4 with
`a_hanging_provider_is_bounded_at_boot_and_at_stop` in `crates/renvor-testkit/tests/injection.rs`
asserting `stop.fired`. It has not reproduced since.

| Attempt to reproduce | Result |
|---|---|
| the same test, isolated and serial | 1/1 pass |
| the same suite, default parallelism | 5/5 pass |
| `cargo test --workspace --all-features` (the exact step-4 command) | 3/3 pass |
| the suite, 12 runs under deliberate CPU saturation (28 spinners on 14 cores) | 0/12 failed |
| the suite, 60 runs at **this head** under load | 0/60 failed |
| the suite, 60 runs at the **base commit** `2555c01` under identical load | 0/60 failed |
| `cargo xtask verify` @ 1.94.0, re-run | 11/11 |
| `cargo xtask verify` @ stable, first run | 11/11 |
| CI `verify (1.94.0)` and `verify (stable)` on this head | pass |

**132 subsequent runs at this head and 60 at the base produced 0 failures**, so there is no
evidence the intermittency is new, and the corrective batch touches no part of the lifecycle stop
path. `InjectingProvider::stop` calls `mark()` before its first `await`, so `fired` can only be
false if the returned future was dropped before its first poll — a cancellation-ordering race under
`tokio`'s paused clock, in the harness rather than in the kernel.

The constitution states that flaky tests are defects. This one is **recorded as an open item with
an owner** rather than claimed to be fixed: it was observed once, it has not been root-caused to a
line, and asserting a fix that has not been demonstrated is the exact habit these four rounds
exist to correct. It is **not** presented as resolved.

## Publication status

**0 crates published, 0 tags, 0 releases** (FR-034). `publish = true` is a manifest attribute
stating a crate *may* be published; nothing has been.

Verified: `gh api repos/renvor-rs/renvor/tags` → `0`; `.../releases` → `0`. The workspace release
rehearsal runs `cargo publish --dry-run --workspace`, which stages all four crates and aborts every
upload — observed output `warning: aborting upload due to dry run`, four times.

## Reproducing the gate evidence

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check licenses advisories bans sources
cargo publish --dry-run --workspace          # stages 4 crates, publishes 0
cargo xtask verify                           # the full CI gate, locally
```
