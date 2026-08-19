# ADR-0007: Build three Phase 002 kernel primitives in-house, and no others

| Field | Value |
|---|---|
| **ID** | 0007 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-004 |
| **Review date** | 2026-08-16 |
| **Superseded by** | — |

> **Scope was fixed by measurement, then corrected by review.** This record covers **three**
> primitives, and it is worth being precise about how each arrived, because the two mechanisms are
> different.
>
> **Two were fixed by measurement**, before anything was drafted: the configuration gate **failed**
> (4 of 8), so its fallback is in scope; the resolver **algorithm** gate **passed** (8 of 8), so
> `tarjan_scc` is adopted and is not in scope. That is W-004 compensating control 1 working —
> scope set by execution rather than by the author's prediction.
>
> **The third was added by advisory review.** The first draft excluded the entire provider-graph
> adapter on an unsourced line count that was wrong by a factor of two, and never mentioned that
> `petgraph::csr::Csr` — an adoptable compressed-adjacency type implementing all three required
> traits — exists. The storage underneath the adapter was a genuine build-versus-adopt choice, so
> it belongs here. That is W-004 compensating control 3 working: a finding that changed the
> record rather than being absorbed into it.
>
> A gate can only measure what it was pointed at. Neither gate was pointed at the storage layer,
> which is exactly why the review requirement is not redundant with them.

## Context

Constitution principle III and spec FR-035 make custom infrastructure the exception: a maintained
package is preferred, and choosing to build instead requires a decision record naming the packages
evaluated, their concrete shortcomings, the ownership cost, the testing burden, the exit triggers,
and a replacement strategy. This record is that justification for Phase 002.

Phase 002 builds a transport-independent kernel. Most of it is package adoption — `tokio`,
`tokio-util`, `tracing`, `thiserror`, `getrandom`, `petgraph`, `serde`, `toml`, `secrecy`. Three
places resisted, for different reasons, and only one of them was known in advance.

**The known one** is the typed state map. FR-011 requires duplicate registration to error *naming
the type*, and FR-037(b) permits the kernel to emit a state value's *type name only*. Both need the
name stored beside the value, and an any-map has nowhere to put it.

**The discovered one** is configuration layering. Research D6 predicted that `confique` would fail
two of eight obligations. It failed **four**, and one of them — source attribution — turned out to
be unrecoverable rather than merely awkward. That verdict is what put a configuration adapter in
this record's scope.

**The third was found by being challenged.** Research revision 1 rejected `petgraph` and specified
a custom resolver; revision 2 showed revision 1 had evaluated the wrong function, and the resolver
gate confirmed `petgraph::algo::tarjan_scc` meets every requirement with measured counters. The
**algorithm** is therefore adopted. But the **storage** the algorithm traverses was written by
hand, and `petgraph::csr::Csr` was available — a fact the first draft of this record neither
mentioned nor evidently knew. The advisory review caught it. Primitive 3 is the result.

## Decision

**Three primitives are built in-house. Everything else in Phase 002 adopts a package** — including
the resolution *algorithm*, which is `petgraph::algo::tarjan_scc` unmodified.

### Primitive 1 — the typed state map

`HashMap<TypeId, StateEntry>`, where `StateEntry` carries the boxed value **and** its
`&'static str` type name.

| FR-035 requirement | Answer |
|---|---|
| **Packages evaluated** | `anymap3` 1.1.0; `http::Extensions`; `state`; `std::any` used directly (the choice made) |
| **Concrete shortcoming — `anymap3`** | An any-map is keyed by type and returns the value. It has **nowhere to store the type name**, so satisfying FR-011 and FR-037(b) means maintaining a **second parallel map** of names in lockstep with the first — two structures that can disagree, wrapping something that is otherwise a `std` `HashMap`. The custom version is *smaller* than the adoption |
| **Concrete shortcoming — `http::Extensions`** | Drags an HTTP crate into the transport-independent core. Contradicts principle II outright, and FR-033 specifically |
| **Concrete shortcoming — `state`** | Global / lazy-static oriented, colliding with FR-032's prohibition on hidden global mutable state |
| **Ownership cost** | Low and bounded. One `HashMap`, one struct, one insert path, one lookup path. No algorithm, no unsafe, no lifetime gymnastics. `TypeId` and `type_name` are `std` and stable |
| **Testing burden** | Duplicate registration, missing state, type-name reporting, and the 0-panics-in-ordinary-use assertion (SC-004) — T041, T054 |
| **Exit trigger** | An any-map that stores per-entry metadata alongside the value, with the same or better ergonomics. That single capability is the entire reason this is custom |
| **Replacement strategy** | The map is private behind the state API. Swapping the backing store changes no public signature. The exit is a body change |

**A correction carried in deliberately.** Research revision 1 cited `anymap3`'s licence expression
`BlueOak-1.0.0 OR MIT OR Apache-2.0` as a reason to reject it. That was **wrong** — the `OR` means
`cargo deny` resolves to an allowed term and the policy passes. The licence is a footnote. The
reason is metadata colocation, above, and the overstated reason is recorded rather than quietly
dropped, because a future reader re-evaluating `anymap3` needs to know which objection was real.

### Primitive 2 — the `serde` + `toml` partial-layer configuration adapter

Renvor reads each configuration source into a partial representation, merges the partials in an
explicit precedence order while recording which layer supplied each key, then decodes the merged
result once via `serde`. `toml` parses; `serde` decodes; the **layering and attribution** are
Renvor's.

| FR-035 requirement | Answer |
|---|---|
| **Packages evaluated** | `confique` 0.4.0 (the candidate, gate-tested); `figment`; `config`; `toml` + `serde` with Renvor layering (the choice made) |
| **Concrete shortcoming — `confique`, decisive** | **Obligation 4, source attribution: unrecoverable.** Its only combining primitive is `Layer::with_fallback`, documented as *"basically like `Option::or`"*. `Option::or` returns a **value** and retains nothing about which side supplied it. There is no hook, no richer return type, no opt-in. Provenance is destroyed **at the point of merge**, so FR-016 cannot be satisfied by any amount of wrapping |
| **Concrete shortcoming — `confique`, structural** | The environment is a **per-field `#[config(env = "…")]` annotation**, not an orderable layer with a precedence position. Contract C-C4 requires it to be a layer. This was not one of the eight obligations; it was found while running them, and it rules the crate out independently |
| **Concrete shortcoming — `confique`, recoverable but real** | Obligation 1 — precedence is **inverted** (earlier source wins); recoverable by reversing argument order, and a test records that it is. Obligation 6 — `THREADS=""` silently became the default `7`, because an empty value that fails to deserialize is documented as *"treated as not set"*. That is a silent fallback, which FR-022 forbids. Obligation 7 — a shape conflict fails, but names only the file being parsed, not both layers |
| **Why the whole crate falls to one obligation** | Contract C-C7 is **all-eight-or-fallback**. Obligations 1, 6, and 7 would each have been survivable alone. **4 is not**, and it alone decides the gate |
| **Concrete shortcoming — `figment` 0.10.19, `config` 0.15.25** | Both **merge an untyped value tree first and decode last**, the inverse of contract C-C2's normative order, and neither documents a shape-conflict error — `figment`'s own documentation reads *"join and adjoin use the existing value; merge and admerge use the incoming value"*, i.e. **last-wins, silently**. C-C2 exists precisely because merge-then-decode *"resolves a shape conflict by picking a winner silently"*, which principle IV prohibits. Evidence: research §3, rows for both crates. **Corrected after advisory review** — the first draft called these two "unexamined", which understated the record: they were evaluated against primary-source documentation and both fail on structural ordering, before merge semantics are even reached |
| **Ownership cost** | **The highest cost in this record.** Merge semantics are subtle: per-key nested-table merge with surviving siblings, wholesale array replacement, shape-conflict detection across layers, and per-key provenance. This is the part most likely to grow |
| **Testing burden** | **Not yet paid, and the first draft said otherwise.** It claimed the eight-obligation gate "becomes the adapter's own acceptance suite" in the present tense. It does not yet: `crates/renvor-config/src/lib.rs` is a documentation-only stub, and every `obligation_*` test in `crates/renvor-config/tests/proof_gate.rs` exercises **`confique`**, the rejected candidate — not a Renvor adapter. The harness and fixtures exist and are reusable, which is the part that is real; **T065 is the task that re-points them at the replacement, and it is open.** Corrected after advisory review, and carried below as a named open item |
| **Exit trigger** | Any maintained layering crate that (a) reports the winning layer per resolved key, (b) treats the environment as an orderable layer, and (c) fails rather than falls through on an empty invalid value. Re-run the eight obligations against it; 8 of 8 means adopt |
| **Replacement strategy** | The adapter sits behind the configuration API, so `toml` and `serde` are retained either way and a replacement swaps only the layering step. **But signature compatibility is not sufficient**, and saying "a body change" alone would understate it: because provenance is destroyed *inside* a merge that does not track it — the same structural fact that sank `confique` — any replacement must be **natively provenance-tracking**. That constrains a replacement to a family of algorithms, not merely to a matching function signature |

**`confique` stays a `[dev-dependency]`** solely so the gate remains reproducible. It never enters
the production graph — confirmed against the real lockfile in
`governance/phase-002-dependency-inventory.md`. Deleting it would delete the evidence.

### Primitive 3 — the compact adjacency storage behind the resolver adapter

> **Added after advisory review, and the record was wrong before it.** The first draft excluded the
> whole provider-graph adapter from FR-035, describing it as *"roughly 300 lines implementing three
> `petgraph` traits."* The architecture reviewer checked the file. It is **648 lines**, of which
> the petgraph trait conformance is **~101** (from line 547); the preceding ~450 are a
> hand-written storage type, a builder, a counter subsystem, and result types. The 300 figure was
> **unsourced and wrong**, and it sat at exactly the point the draft had labelled "the most
> contestable line in the record."
>
> Worse for the original argument: **`petgraph::csr::Csr` exists**, is not feature-gated, and
> already implements all three required traits. The draft never mentioned it. So the storage was a
> genuine build-versus-adopt choice, which is precisely what FR-035 governs — and the honest
> remedy is the one the draft itself pre-committed to: add it here rather than argue the
> definition.

**What is in scope is the storage, not the trait impls.** They are separated because only one of
them was a choice:

| Component | Lines (approx.) | Was there a package alternative? |
|---|---|---|
| `GraphBase` / `NodeIndexable` / `IntoNodeIdentifiers` / `IntoNeighbors` impls | ~101 | **No.** C-G6 requires the counters observed at a Renvor-owned boundary; a trait impl is the only place that boundary exists. Required whether the storage is Renvor's or petgraph's |
| `WorkCounters`, `Allowances`, `BudgetExhausted`, `BudgetAxis` | ~90 | **No.** The work budget is a Renvor contract (C-G1, C-G3). No package offers it |
| `Resolution`, `Component` | ~80 | **No.** Classifying an SCC as a cycle per C-G8 — including the single-node self-edge case `tarjan_scc` cannot distinguish — is Renvor's rule |
| **`ResolverGraph` + `ResolverGraphBuilder` (compressed adjacency storage)** | **~180** | **Yes — `petgraph::csr::Csr`.** This row is the one FR-035 governs |

| FR-035 requirement | Answer |
|---|---|
| **Packages evaluated** | `petgraph::csr::Csr`; `petgraph::Graph`; `petgraph::StableGraph`; hand-written compressed adjacency (the choice made) |
| **Concrete shortcoming — `Csr`, ordering** | `Csr` keeps each node's neighbour list **sorted by target index**: `find_edge_pos` inserts in sorted position, and `from_sorted_edges` documents that edges *"**must** be sorted and unique."* Contract **C-G7** requires neighbours to be yielded in **declaration order**. Sorted order is deterministic but it is not the order the author wrote, so `Csr` fails C-G7 as written |
| **Concrete shortcoming — `Csr`, silent dedup** | `add_edge_` returns `Ok(false)` when the edge already exists and **drops it silently**. Renvor's work-budget denominator is the **declared** edge count, so a silently dropped duplicate would move the denominator with nothing reporting it. A silent drop is exactly what FR-022 and principle IV forbid, and it would corrupt the one number SC-021 asserts |
| **Concrete shortcoming — `Graph` / `StableGraph`** | Both carry per-node and per-edge weight storage Renvor has no use for, and `StableGraph` additionally maintains a free list for removals that never happen here. Neither is wrong; both are simply larger than the problem |
| **Ownership cost** | Low. Two `Vec<u32>`, an offsets array, and a builder that appends. No algorithm — the algorithm is petgraph's, and that has not changed |
| **Testing burden** | Already paid: the 13 tests in `crates/renvor-core/tests/resolver_proof.rs`, including exact-equality counter assertions at both ceilings and the 1024-node recursion proof |
| **Exit trigger** | A petgraph storage type that (a) preserves declaration order for neighbours and (b) reports rather than silently drops a duplicate edge. Either capability landing in `Csr` upstream makes this row reversible |
| **Replacement strategy** | The storage is private behind `ResolverGraph`. The trait impls, the counters, and `resolve()` would be retained and re-pointed at the inner type; the counter assertions in the proof suite are the acceptance test for the swap |

**What this does not change.** The **algorithm** remains adopted: `petgraph::algo::tarjan_scc`,
single pass, unmodified. Research D8's conclusion that the resolver is package adoption stands for
the part D8 was actually about. What was misclassified was the storage underneath it, and only
that row moves into FR-035's scope.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| Adopt `anymap3` and keep a parallel name map | Two structures that can silently disagree, in service of code that is otherwise a `std` `HashMap`. The adoption is **larger** than the thing it replaces |
| Adopt `confique` and accept no source attribution | FR-016 requires it and SC-014 asserts it. This is waiving a requirement to avoid writing code, which is the trade FR-035 exists to prevent |
| Adopt `confique` and add attribution on top | Impossible, not merely costly. `with_fallback` destroys provenance **inside** the merge; there is no seam above it to reconstruct from |
| Gate-test `figment` and `config` before building | **The strongest rejected alternative, and the one this record is weakest against.** It would cost one gate run each against a harness that already exists. Rejected on scope: the gate's purpose was to decide whether the *recorded* candidate met the bar, and D6's fallback was written in advance precisely so a failure would not reopen package selection mid-phase. Recorded as an **accepted cost**, not as a finding against either crate — see Consequences |
| Lower the ceiling / relax an obligation so `confique` passes | Converts a proven limit into an unexplained one. C-C7 is all-eight-or-fallback by design, and the design is what makes the verdict meaningful |
| Build a custom iterative SCC resolver | The revision-1 plan. Ruled out by measurement: the resolver gate passed 8 of 8, with counters at exactly 2048 / 8192 / 10240 and ≈ 4× stack headroom in debug. Building it would reimplement a maintained, documented `O(V+E)` algorithm to own it |
| Defer these primitives to Phase 003 | The kernel's public surface depends on all three. Deferring means shipping a kernel that cannot report a duplicate state type, cannot explain where a configuration value came from, and cannot resolve a provider graph within a counted budget |

## Consequences

**Accepted costs.**

- **Three primitives to maintain in perpetuity**, or until their exit triggers fire. The
  configuration adapter is the expensive one; the state map is close to free.
- **Merge semantics are now Renvor's problem.** Every subtlety that `figment` or `config` would
  have absorbed — nested tables, arrays, shape conflicts across layers, provenance — is code this
  project owns and tests.
- **`figment` and `config` remain unexamined.** This is a real gap, and the honest framing is that
  Phase 002 chose schedule certainty over a more complete package survey. It is recorded as an
  **open item**, not as a closed evaluation, and the exit trigger above is written so that
  examining them later is a normal action rather than a reversal.
- **`confique` stays in the dependency graph as a dev-dependency**, carrying `confique-macro`,
  `heck`, and a second `toml`/`winnow`/`syn` version into the test graph. Confirmed dev-only in the
  inventory; the cost is build time for the test suite.

**What becomes locked in.** All three primitives sit behind public APIs whose *signatures* do not expose
them, so replacement is a body change. What is locked in is the **behaviour**: once configuration
reports a source layer per key, removing that capability is a breaking change for anyone who
diagnosed a problem with it.

**What would have to change to reverse this.** For the state map: an any-map with per-entry
metadata. For the adapter: a layering crate passing all eight obligations. Either would supersede
this record rather than amend it.

**What this record does not buy.** It does not make the three primitives correct. It records that
building them was the justified choice; whether they *work* is decided by the acceptance suites in
Phase 3 and Phase 5, and a passing ADR has never made a failing test pass.

### Disclosure surfaces this record initially failed to consider

All three were raised by the security advisory review, and none had been examined anywhere in the
phase's artifacts. They are recorded as **accepted risks with stated reasoning**, not as findings
argued past.

**Attribution reporting is itself an output channel.** Primitive 2's headline capability is
reporting *which layer supplied each key*, and nothing in this phase had asked whether that report
can disclose anything. It can, in two narrow ways: the **layer identity** for a key ("this came
from the environment, not from `base.toml`") is a weak signal about deployment topology, and a
**key name** chosen by an author could itself be sensitive. Judged acceptable because keys are
**schema-declared by the application author**, not attacker-supplied; the attribution report is
returned to that same author in-process; and this phase has **no network surface** (FR-033).
Accepted on that basis, and stated so a future phase that exposes attribution over a transport
knows it is changing this premise rather than inheriting it. Contract C-C9's redaction of secret
**values** is unaffected and continues to apply to the value side.

**A type name is metadata, and metadata can talk.** FR-037(b) permits emitting a state value's
type name, and the implementation carries `&'static str` and no value field. The residual risk is
that the *name* is informative: a type called `DbPassword` announces the category of secret in
play, and `std::any::type_name` renders **const-generic parameters verbatim**, so `Rate<500>`
prints `500`. Both are accepted: the type name is the minimum needed for FR-011's "name the type"
diagnostic, and an author who does not want a name emitted controls that name. Recorded because
"type names are safe" was assumed rather than reasoned, and the const-generic case is a real edge
that assumption does not cover.

**`confique-macro` is a proc macro, and the first draft priced it as build time.** Retaining the
rejected candidate as a dev-dependency was justified above on reproducibility grounds, and its
cost was described as slower test builds. That is the wrong risk category. A proc macro **executes
arbitrary code at compile time** on every contributor machine and every CI runner, so a compromised
release of a comparatively low-adoption crate in the dev graph is a build-time code-execution
vector rather than a slower `cargo test`. It is mitigated — `cargo deny check advisories` runs
unconditionally on every pull request and covers dev-only entries, currently reporting 0 open
advisories across all 55 external packages — but the mitigation only counts if the risk it covers
is named, and it was not.

## Authority to accept this record — W-004 and nothing else

This section exists because three waivers touch nearby ground and only one reaches this record.
Getting that wrong would let a Phase 001 exception silently authorise a Phase 002 acceptance.

| Waiver | What it actually covers | Does it authorise accepting ADR-0007? |
|---|---|---|
| **W-002** | The ADR independent-review gap for **Phase 001 decision records only** — ADR-0001 through ADR-0006 | **No.** It does not reach a Phase 002 record. Stated as a limit in the ledger, not inferred here |
| **W-003** | **Phase 001's phase-level** independent requirements-and-security review | **No.** It is a phase-level waiver, and it is the wrong phase besides |
| **W-005** | **Phase 002's phase-level** independent requirements-and-security review | **No.** Right phase, wrong object. W-005 is explicit that it *"does not authorise accepting any decision record"* — the mirror of the limit W-003 places on W-002, written so that "Phase 002" cannot be read as swallowing the records inside it |
| **W-004** | The independent-review requirement for **ADR-0007 and nothing else** | **Yes — and it is the only one.** It does not reach ADR-0008, any future Phase 002 record, or any record in any other phase |

**W-004 waives *who reviews*. It waives nothing about *what must be true*.** Every finding, failed
check, missing evidence item, acceptance criterion, and security blocker stands exactly as it would
without it.

**No independent human review of ADR-0007 has occurred.** The review below is a structured
self-review plus two agent advisory reviews, operating under a recorded exception. It is **not**
independent, and must never be described as independent — here, in the evidence pack, in
`GOVERNANCE.md`, or in any public document. A waiver removes the requirement; it never changes the
fact.

**Acceptance conditions.** This record may not reach `accepted` until **all four counted**
compensating controls are complete, **every restated precondition holds**, and the review record is
dated. The reviewer field then reads exactly `Ahmed Anbar — self-review under W-004`.

**When W-004 closes**, the first qualified independent reviewer re-reviews this record **in full**,
including the alternatives it rejects — with particular attention to the `figment`/`config` gap
recorded above, which is the item most likely to change the outcome.

## Review record

Both reviews below are **NON-INDEPENDENT** and **ADVISORY**. Neither is an independent human
review, and neither may be described as one.

### Attempt 1 — RECORDED AS NOT PERFORMED

Two automated review passes were run on 2026-08-16 with distinct written checklists, one
architecture and one security. **Both went idle without returning findings and without returning a "no findings"
statement.** Under W-004 compensating control 2 that is recorded as **not performed**, never as
passed.

This is the **second** occurrence of this exact failure mode in the project — the clause exists
because it happened once before. Diagnosis: the prompts were long, spanned many files, and made
delivery depend on the agent's final message surviving to the caller. **Remedy applied:** the retry
narrowed each checklist to seven numbered questions and made the deliverable a **file written to
disk** rather than a returned message, removing message delivery from the critical path. Both
retried reviews delivered on the first attempt after that change.

Recorded rather than quietly re-run, because "we tried until it worked" and "it worked" are
different facts, and only one of them is true here.

### Attempt 2 — both reviews performed, 8 findings, all dispositioned

| # | Review | Severity | Finding | Disposition |
|---|---|---|---|---|
| A-1 | Architecture | **MAJOR** | The FR-035 exclusion for the provider-graph adapter rested on "~300 lines… integration surface". The file is **648 lines**, of which only ~101 are petgraph trait conformance; the rest is custom storage, builder, counters, and result types. The figure was unsourced and appears nowhere in research | **FIXED — and it changed the decision.** Verified independently: 648 lines, adapter section begins at line 547. Also confirmed `petgraph::csr::Csr` exists, is not feature-gated, and implements all three traits — which the draft never mentioned. **Primitive 3 added**, scoped to the storage row, with `Csr` evaluated and two concrete shortcomings recorded (sorted-not-declaration order; silent duplicate-edge drop), both verified from petgraph's source |
| A-2 | Architecture | MINOR | "Replacement is a body change" understates that any config replacement must be **natively provenance-tracking**, not merely signature-compatible | **FIXED.** The Primitive 2 replacement-strategy row now states the algorithmic constraint directly instead of leaving it implicit in Consequences |
| A-3 | Architecture | MINOR | `figment` and `config` were never gate-tested despite the harness existing and the cost being stated as "one gate run each" | **FIXED, by correcting the record in the *opposite* direction.** The reviewer's own A5 was right that "unexamined" understated things: research §3 evaluates both against primary-source documentation and both **merge-then-decode**, inverting C-C2's normative order and failing on structural grounds before merge semantics are reached. Gate-testing them would test a design already excluded on a more fundamental axis. The row now cites that evidence instead of claiming ignorance |
| A-4 | Architecture | MINOR | The Compliance table's "package-first… evidenced by execution" did not caveat that only 2 of the listed candidates were gate-executed | **FIXED.** That row now distinguishes gate-executed evidence (2) from documentation-and-source evidence (6), rather than letting one word cover both |
| S-1 | Security | **MAJOR** | The claim that the eight-obligation gate "becomes the adapter's own acceptance suite" is written in the present tense, but `renvor-config` is a documentation-only stub and every `obligation_*` test exercises **`confique`**, the rejected candidate. T065 is open | **FIXED.** The Testing-burden row now states plainly that the burden is **not yet paid**, names the stub, and carries T065 as an open item. The reviewer is right that the prose read more confidently than the evidence supported |
| S-2 | Security | MINOR | Nothing anywhere asks whether **attribution reporting** or a **key name** could itself be a disclosure channel | **FIXED.** Added as a named accepted risk with stated reasoning — schema-declared keys, in-process return, no network surface this phase — and flagged for any future phase that exposes attribution over a transport |
| S-3 | Security | MINOR | Nothing asks whether a **type name** can disclose, including `type_name` rendering const-generic parameters verbatim | **FIXED.** Added as an accepted risk, with the `Rate<500>` edge named explicitly. "Type names are safe" had been assumed, not reasoned |
| S-4 | Security | MINOR | Retaining `confique-macro` is priced as **build time**, but a proc macro is **build-time code execution** on every contributor and CI machine | **FIXED.** Consequences now names the real risk category and the control that covers it — `cargo deny check advisories` runs unconditionally and covers dev-only entries, currently 0 open advisories across all 55 external packages |

> **Dated-figure note, added 2026-08-16 (T127).** Two dispositions above cite *"all 55 external packages"*. That was the resolved count when this record was accepted. Deleting the `confique` tree — a consequence of the configuration gate failing — removed seven packages, and the current figure is **48**. The dispositions are left as written, because a decision record is a dated account of what was decided on what evidence, not a live dashboard. The authoritative current inventory is [`governance/phase-002-dependency-inventory.md`](../governance/phase-002-dependency-inventory.md).

**Verified rather than accepted on assertion:** A-1's line counts, the existence and trait coverage
of `petgraph::csr::Csr`, its sorted-insertion and silent-dedup behaviour, and A-3's research §3
rows were each checked against the files and petgraph's published source before being
dispositioned. A finding that changes a decision record should not be taken on trust either.

**Both reviewers reported items they could not verify** — the architecture reviewer flagged the
waiver ledger and dependency inventory as unread rather than confirmed; the security reviewer noted
it could not confirm the prior attempt's outcome. Both disclosures are recorded here as the correct
behaviour, and the second is answered by the Attempt 1 section above.

## Outcome

**Accepted under W-004**, with the compensating controls evidenced below.

| W-004 counted control | Status |
|---|---|
| 1 — configuration proof gate **and** resolver counter proof completed and recorded **before** acceptance | **PASSED.** 10 and 13 tests, recorded in research §D6 and §D8 before this record was drafted. Scope was set by their outcomes |
| 2 — two clean-context advisory reviews, one architecture and one security, each producing a recorded result | **PASSED on attempt 2.** Attempt 1 recorded as **not performed**, above |
| 3 — every finding individually dispositioned | **PASSED.** 8 findings, 8 dispositions, in the table above. Two changed the record materially |
| 4 — no custom infrastructure merges until controls 1–3 are recorded | **HOLDS.** T041 (typed state map) is unstarted and blocked; the configuration adapter is unimplemented; the resolver storage is committed but its FR-035 justification is now on this record |

| W-004 restated precondition | Status |
|---|---|
| Alternatives-and-consequences analysis (FR-035, principle III) | **HOLDS.** Seven rejected alternatives with reasons; costs stated including the three disclosure surfaces |
| Package-first evaluation of every custom primitive | **HOLDS.** 8 packages evaluated across 3 primitives — 2 gate-executed, 6 by documentation and source |
| CI, dependency, licence, advisory, secret-scanning, and code-quality gates running unconditionally | **HOLDS.** `ci.yml` (gitleaks 8.30.1, `cargo xtask verify`) and `security.yml` (cargo-deny, clippy `-D warnings`, dependency-review) both trigger on every pull request |

**Reviewer**: `Ahmed Anbar — self-review under W-004`. **No independent human review of this record
has occurred**, and this must not be described as independent anywhere.

**Carried forward as named open items** in `governance/phase-002-evidence.md`:

1. **T065** — re-point the eight-obligation gate at the Renvor adapter. Until it runs, the adapter's
   compliance with obligations 4, 6, and 7 is *designed for* and **not demonstrated** (S-1).
2. **ADR-0008** remains `proposed`; W-004 does not reach it.
3. When W-004 closes, the first qualified independent reviewer re-reviews this record **in full** —
   with particular attention to Primitive 3, which exists only because an advisory review
   challenged a classification the author had made twice.

## Compliance

| Rule | How this satisfies it |
|---|---|
| **Spec FR-035** | Custom infrastructure is recorded with packages evaluated, concrete shortcomings, ownership cost, testing burden, exit triggers, and a replacement strategy — for **each** primitive separately |
| **Constitution principle III** | Package-first was applied, and the evidence is of **two kinds, not one**: `confique` and `petgraph` were **gate-executed** (10 and 13 tests), while `figment`, `config`, `anymap3`, `http::Extensions`, `state`, and the three `petgraph` storage types were evaluated against **primary-source documentation and code**. Both are evidence; only the first is execution, and the table says so rather than letting "evidenced by execution" cover all eight. One gate reversed a prior decision to build; one advisory review reversed a prior decision **not** to record |
| **Constitution §Workflow #4** | Captured as a **proposed** record and reviewed before being treated as accepted. The review's non-independence is stated rather than glossed |
| **Constitution principle II / FR-033** | `http::Extensions` was rejected specifically for importing a transport into the transport-independent core |
| **Constitution principle XII** | The limits are stated, not implied: `figment` and `config` are unexamined, the resolver-adapter classification is flagged as contestable, and the overstated `anymap3` licence objection is corrected rather than deleted |
| **FR-011, FR-016, FR-022, FR-037(b)** | Each is named against the specific obligation or shortcoming that drove the decision |
