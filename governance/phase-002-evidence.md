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

## Named open items

| # | Item | Why it is open | Blocking? |
|---|---|---|---|
| 1 | **T065** — re-point the eight-obligation gate at the Renvor adapter | The adapter is unimplemented. Its compliance with obligations 4, 6, and 7 is **designed for, not demonstrated** | Blocks the Phase 5 checkpoint, not ADR-0007's acceptance |
| 2 | **ADR-0008** remains `proposed` | W-004 covers ADR-0007 alone and confers no authority here. FR-035 does not require acceptance for a packaging decision | No |
| 3 | Independent re-review of ADR-0007 when W-004 closes | No qualified independent reviewer is available (research §D11 criteria 1, 2, 4) | Blocks W-004 closure |
| 4 | **W-005** — Phase 002 independent requirements-and-security review | Same staffing gap, phase level | Blocks public release |
| 5 | `ConfigSource::load` and `validate` return `Result<(), KernelError>` and carry **no value type** | US1 needs the *phase behaviour on failure*; typed decoding is `ConfigResolver`, implemented at T071. A placeholder value type now would be a shape nobody measured | Closes with US3 |
| 6 | The facade's `config` re-export is currently **vacuous** — `renvor-config` exports no items yet | The gate is structurally correct and will carry real items from T071; today it gates an empty module | No |

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
