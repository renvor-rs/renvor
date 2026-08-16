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
