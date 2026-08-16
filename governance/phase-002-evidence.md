# Phase 002 — Evidence Ledger

**Feature**: [`specs/002-core-kernel`](../specs/002-core-kernel/spec.md) | **Status**: in progress
**Toolchain**: 1.94.0 | **Source of truth**: the live workspace and the tracked `Cargo.lock`

This ledger records what was **executed and observed**, not what was intended. Where something was
not done, it says so; where something failed, it says that too. A ledger that only records
successes is a marketing document.

## Gate outcomes

| Gate | Tasks | Outcome | Evidence |
|---|---|---|---|
| Configuration compatibility proof (8 obligations) | T014–T020 | **FAILED — 4 of 8 met.** Obligations 1, 4, 6, 7 unmet; obligation 4 unrecoverable. `serde` + `toml` fallback **triggered** | `crates/renvor-config/tests/proof_gate.rs` (10 tests), `examples/env_probe.rs`, research §D6 |
| Provider-resolver feasibility and counters | T021–T025 | **PASSED — 8 of 8 met.** Counters exactly 2048 / 8192 / 10240 against allowances 2048 / 16384 / 18432. Iterative-SCC fallback **not** triggered | `crates/renvor-core/tests/resolver_proof.rs` (13 tests), research §D8 |
| ADR-0007 governance gate | T026–T029 | **PASSED — accepted under W-004** after 8 advisory findings, all dispositioned. Two changed the record | `decisions/0007-phase-002-custom-kernel-primitives.md` |
| Complete resolved transitive dependency inventory | T030–T034 | **PASSED.** 55 external packages; 0 without a licence, 0 over MSRV, `cargo deny` clean on all four checks | `governance/phase-002-dependency-inventory.md` |

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

| Gate | Result |
|---|---|
| 0–5 (workspace, format, lint, test, docs, deny) | covered by `xtask verify` steps 1–6, all passed |
| 13 (crate DAG and facade isolation) | **now automated** as verify step 7, with controls |
| 14 (publication status) | passed — see T110 below |
| 15 (working-tree cleanliness) | verify step 11, passes once work is committed |

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

## Named open items

| # | Item | Why it is open | Blocking? |
|---|---|---|---|
| 1 | ~~**T065** — re-point the eight-obligation gate at the Renvor adapter~~ | **CLOSED 2026-08-16.** The gate runs against the adapter and passes; obligations 4, 6, and 7 are demonstrated | — |
| 2 | **ADR-0008** remains `proposed` | W-004 covers ADR-0007 alone and confers no authority here. FR-035 does not require acceptance for a packaging decision | No |
| 3 | Independent re-review of ADR-0007 when W-004 closes | No qualified independent reviewer is available (research §D11 criteria 1, 2, 4) | Blocks W-004 closure |
| 4 | **W-005** — Phase 002 independent requirements-and-security review | Same staffing gap, phase level | Blocks public release |
| 5 | `ConfigSource::load` and `validate` return `Result<(), KernelError>` and carry **no value type** | US1 needs the *phase behaviour on failure*; typed decoding is `ConfigResolver`, implemented at T071. A placeholder value type now would be a shape nobody measured | Closes with US3 |
| 6 | The facade's `config` re-export is currently **vacuous** — `renvor-config` exports no items yet | The gate is structurally correct and will carry real items from T071; today it gates an empty module | No |
| 7 | `DEFAULT_PROVIDER_DEADLINE` is **30 s by Renvor's choice, not by specification** | FR-025 and C-L7 require the bound; no artifact names a value. Chosen to match the drain default as a symmetry, not from measurement | No — but a phase that measures real provider start-up times should revisit it |
| 8 | The TOML boundary's generated-input testing is a **hand-written deterministic generator**, not a coverage-guided fuzzer | `cargo-fuzz` needs nightly against a fixed 1.94.0 floor; `proptest`/`arbitrary` are new packages in a recorded-gate inventory. Explores far less of the space | No — but a phase with a nightly CI lane should add real fuzzing |
| 9 | An author writes a **second, all-optional struct** per schema | Decode-per-source needs an all-optional decode target and Renvor has no derive macro. A proc-macro of its own is custom infrastructure under FR-035 needing its own accepted record | No |
| 10 | `expected_type` is reported **inside the constraint text**, not as its own field, for file and environment layers | `KernelError::Configuration::expected_type` is `&'static str` so it cannot carry a value (C-E3); the adapter has no schema description to read a per-key type from. All three facts C-C3 requires are in the message | No |
| 11 | `MAX_FILE_BYTES` is **1 MiB by Renvor's choice** | C-C10 requires the bound; no artifact names a value. Overridable per file | No |
| ~~12~~ | ~~SC-015 does not hold~~ | **CLOSED**: both bounded by a worker thread and `recv_timeout`; the enumeration is corrected from three waits to five, with a counting test so a sixth cannot escape it | — |
| 13 | **C-L9's `Panic` behaviour is not injectable for _providers_** at `Boot` or `Stop` | Containing a panic across an `await` needs a `'static` future (ruled out by `InitContext` borrowing state) or a new dependency. **Narrowed 2026-08-16**: configuration sources and readiness contributors *are* contained, both being reached synchronously | **Yes — SC-009 is met for phases, and for all three behaviours everywhere except provider initialise and stop** |
| 15 | A configuration source that never returns **leaks its worker thread** | No Rust API can interrupt a blocked thread. The kernel's *wait* is bounded, which is what FR-025 requires; the thread is not, and cannot be | No — but an application that hits it repeatedly will accumulate threads |
| ~~14~~ | ~~SC-013 partially met~~ | **CLOSED**: the full 11-step sequence ran on both 1.94.0 and 1.97.1, 0 skipped, with toolchain propagation verified by probe | — |

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
