# Phase 002 — Evidence Ledger

**Feature**: [`specs/002-core-kernel`](../specs/002-core-kernel/spec.md)
**Toolchain**: 1.94.0 | **Source of truth**: the live workspace and the tracked `Cargo.lock`

## Status

**Implementation is complete, conditional on integration into `main`.** Every task T001–T132 has
been executed and verified on the branch `feat/phase-002-core-kernel`.

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
| ~~12~~ | ~~SC-015 does not hold~~ | **CLOSED**: both bounded by a worker thread and `recv_timeout`; the enumeration is corrected from three waits to five, with a counting test so a sixth cannot escape it | — |
| ~~13~~ | ~~`Panic` not injectable for providers~~ | **CLOSED**: contained by polling the already-boxed provider future inside `catch_unwind` — 0 new packages, 0 `unsafe`. SC-009 met in full | — |
| 15 | A configuration source that never returns **leaks its worker thread** | No Rust API can interrupt a blocked thread. The kernel's *wait* is bounded, which is what FR-025 requires; the thread is not, and cannot be | No — but an application that hits it repeatedly will accumulate threads |
| ~~14~~ | ~~SC-013 partially met~~ | **CLOSED**: the full 11-step sequence ran on both 1.94.0 and 1.97.1, 0 skipped, with toolchain propagation verified by probe | — |
| 16 | `catch_unwind` contains only **unwinding** panics; under `panic = "abort"` there is **no** provider or contributor panic containment | A property of the panic strategy, not of Renvor. Nothing in the language can catch an abort. Renvor sets no `panic` profile, so the default `unwind` applies unless a consumer changes it. Also outside reach: a double panic, `process::abort`, and a stack overflow | No — but a consumer who sets `panic = "abort"` loses C-L9's `Panic` guarantee and must be told so |
| 17 | `DEFAULT_READINESS_DEADLINE` is **5 s by Renvor's choice, not by specification** | Same shape as items 7 and 11. FR-025 requires the bound; no artifact names a value. Chosen to sit under a typical container probe interval, so a hung contributor reports as hung rather than as a probe that never answered | No — a phase that measures real probe intervals should revisit it |
| 18 | A readiness probe now starts **one thread per contributor per call** | Bounding `ReadinessContributor::readiness` needs a thread, for the same reason `Load` does: a blocking call inside an async block never yields. An application probed once a second with twelve contributors creates twelve threads a second, all short-lived. Not measured under load | No — but a phase that adds a real probe endpoint should measure it |
| 19 | A hung readiness contributor **leaks its thread**, exactly as item 15 describes for configuration sources | Identical cause, identical impossibility: no Rust API can interrupt a blocked thread. The *wait* is bounded; the thread is not | No — but a permanently-hung contributor accumulates one thread per probe |

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
| T123 | Gate 12 globbed `examples/*.rs` from the repository root — **a directory that does not exist** — and ran zero examples | `for f in <no matches>` executes the body zero times and exits 0. The gate printed `GATE 12 PASS` having run nothing | Discovery under `crates/renvor/examples`, a minimum count, and a control that plants a failing example and requires the gate to reject it |
| T124 | Gate 14d described the ADR-0007 authority as *"a separately proposed and separately approved waiver"* — future tense, written before W-004 existed — and matched status with `^status: accepted`, which the decision-record template **has never produced** | The gate had not been run against the record since the record was accepted | Rewritten for the merged W-004: ledger presence, `active` status, the exact reviewer string, all four counted controls, a recorded advisory result, and the honest denial of independence |
| T125 | Gate 13f counted the facade's **own unit tests** as implementation items and reported 5 | `^\s*(pub )?(fn\|impl\|struct\|enum)` matches an indented `fn` inside `#[cfg(test)] mod tests` | The test module excluded, with **two** controls: the facade must re-export something, and the pattern must still match the test functions when run against the whole file |

**Gates 13, 14, and 15 would each have failed if run on 2026-08-16 before these corrections.** The
T107 record of "all gates pass" reflected gates that were run in an earlier form, or in the case of
Gate 12 a gate that passed by executing nothing. The full sixteen-gate result is recorded below.

### T125 — every quickstart gate, run individually

Run one at a time on 2026-08-16 against the corrected tree. **No gate is collapsed into another,
and none is omitted.**

| Gate | Criterion | Result | Note |
|---|---|---|---|
| 0 | Workspace builds, format, lint | **PASS** | |
| 1 | Dependency policy (SC-012, SC-017) | **PASS** | includes the empty allow-list control |
| 2 | Lifecycle order and rollback (SC-001, SC-002) | **PASS** | |
| 3 | Configuration proof gate, 8 obligations (SC-020) | **PASS** | runs against the Renvor adapter, not the rejected candidate |
| 4 | Secrets and opaque state (SC-007, SC-016) | **PASS** | |
| 5 | Provider graph ceilings and work budget (SC-005, SC-021) | **PASS** | includes the 1024-node chain |
| 6 | Drain, including the zero budget (SC-006) | **PASS** | |
| 7 | Health and readiness disagree (SC-008) | **PASS** | |
| 8 | Failure injection at every phase (SC-009) | **PASS** | now all **21** combinations |
| 9 | Run identifier opacity (SC-019) | **PASS** | |
| 10 | Tracing ownership (FR-029) | **PASS** | |
| 11 | Hostile configuration input (FR-038) | **PASS** | hand-written generator; see open item 8 |
| 12 | Examples and documentation (SC-013, SC-014) | **PASS** | **would have passed vacuously before T123** — 3 examples discovered and run, control fires |
| 13 | Scope discipline and crate DAG (SC-010) | **PASS** | **would have FAILED before T125** — 13f counted test functions |
| 14 | Publication and governance (SC-011, FR-034, ADR-0007) | **PASS** | **14d would have FAILED before T124** — status pattern never matched |
| 15 | Resolved-dependency inventory (FR-040) | **PASS** | **compared nothing before T122** — now 48 = 48, both directions |

**16 of 16 pass. 0 skipped. 0 inconclusive.** Gate 14b's GitHub-releases half was verified with an
authenticated `gh`, not recorded as an unverified gap.

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
| **FR-025** | Deadlines explicit and bounded; **0** unbounded kernel-owned waits | **Eight** bounded callbacks — entropy, source name, load, validate, Register declarations, provider init, provider stop, readiness | `tests/deadlines.rs`, including the discovery gate (T115) | **MET** |
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
