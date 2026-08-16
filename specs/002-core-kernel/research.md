---
description: "Phase 002 research — package-first evaluation, dependency records, and resolved design decisions for the transport-independent core kernel"
---

# Phase 0 Research: Transport-Independent Core Kernel

**Feature**: [spec.md](./spec.md) | **Plan**: [plan.md](./plan.md)
**Date**: 2026-08-16 *(revision 2 — corrections applied, see §0)*
**Branch**: `feat/phase-002-core-kernel`

> **This phase adds the first external dependencies Renvor has ever had.** The workspace
> lockfile currently resolves **2 packages** — `renvor` and `xtask` — and neither has a
> dependency. Every finding below therefore establishes a precedent, not just a choice.

> **ADR-0003 limitation R-7 becomes live here.** MSRV 1.94.0 has never been validated against
> real dependencies. Section 3 is the first evidence that it can be.

## 0. Corrections applied in revision 2

Revision 1 of this document contained **four wrong or overstated claims**. They are listed here
rather than quietly overwritten, because a research record that edits its own errors out of
existence teaches nothing about how much to trust the rest of it.

| # | Revision-1 claim | Status | Corrected in |
|---|---|---|---|
| 1 | "petgraph cannot satisfy FR-013 — `Cycle` names one node" | **WRONG.** True of `toposort`, but `tarjan_scc` returns **every** node of each component. Revision 1 evaluated the wrong function and rejected the package on it | **D8 — reversed; petgraph is now adopted** |
| 2 | "Kahn's algorithm + Tarjan for cycles fits the budget" | **WRONG.** Kahn alone consumes 2 node + 2 edge examinations per item — the *entire* allowance. A second Tarjan pass exceeds it | **D8 — single-pass design** |
| 3 | "`secrecy`'s `Debug` and `Display` implementations print a placeholder" | **WRONG.** `SecretBox` implements `Debug`; it implements **no `Display` at all** | **D2 — Renvor owns the output contract** |
| 4 | "`confique` … matches the mandated order" (stated without its limits) | **INCOMPLETE.** True of layer composition, but confique supplies **no source attribution**, treats an unparseable **empty** env value as **unset**, reads env only for annotated fields, and does not document array replacement | **D6 — four limits recorded, proof gate expanded to 8 obligations** |

Two further corrections apply to framing rather than to a decision: §3 was mislabelled as a
dependency *inventory* when it is a **direct-dependency candidate evaluation** (§3 note), and the
observability decision did not state who owns the global subscriber (**D4**).

## How this research was verified

Every version, licence, MSRV, and advisory figure below was read from a **primary source**, and
each query was paired with a control proving it could return a non-trivial answer. Where revision
1 relied on rendered documentation and got an API wrong, revision 2 reads the **published crate
source** — the `.crate` tarball from `static.crates.io`, which is the exact artifact Cargo would
resolve.

| Source | Used for | Control |
|---|---|---|
| `index.crates.io` (sparse index) | latest non-yanked version, declared `rust_version`, yank counts | `serde` returned **316** version records; a fabricated crate name returned **404** |
| `crates.io/api/v1/crates/{name}` | licence, last-update date, download totals | `serde` returned `MIT OR Apache-2.0` and a non-zero download count |
| `api.osv.dev/v1/query` | advisories **and their affected version ranges** | `time` returned **4** advisories; a fabricated crate name returned **0** |
| **published `.crate` source** | `tarjan_scc` semantics; confique's env and fallback behaviour | both tarballs downloaded with `http=200` and extracted before reading |

**Finding 0 — the crates.io API 403 recorded in Phase 001 was a User-Agent problem, not a bot
gate.** Phase 001 rewrote its registry gates after `crates.io/api/v1` returned 403 and
`jq '.versions | length'` printed `0` for `serde` — a check that failed **open**. A descriptive
`User-Agent` naming the project and a contact address returns **200**. Recorded because it
explains a Phase 001 limitation; it does not reopen Phase 001.

## 1. Decisions

### D1 — Error taxonomy: `thiserror`

- **Decision**: derive kernel error types with `thiserror` 2.0; do **not** take `anyhow`.
- **Rationale**: FR-019 requires a programmatically inspectable category and FR-020 a preserved
  causal chain. `thiserror` generates `std::error::Error` implementations with `#[from]`/`source()`
  chaining while leaving the enum — and therefore the match-on-category surface — entirely ours.
- **Alternatives considered**: `anyhow` — type-erased, right for applications, wrong for a library
  whose errors must be matched on. `snafu` — comparable capability, larger surface, lower
  adoption. Hand-written `impl Error` — mechanical boilerplate per variant; errors get skipped
  when the boilerplate is expensive.

### D2 — Secret redaction: `secrecy` for *access control*, Renvor for the *output contract*

**Revision 1 claimed `SecretBox` redacts both `Debug` and `Display`. That is wrong.** Read from
the published API surface, `SecretBox<S>` implements:

```text
Clone, Debug, Default, Deserialize<'de>, Drop, ExposeSecret<S>,
ExposeSecretMut<S>, From<Box<S>>, Serialize, Zeroize, ZeroizeOnDrop
```

**There is no `Display` implementation at all.** So `secrecy` does not "redact `Display`" — it
makes `Display` *unavailable*, which is a different property with a different failure mode: any
Renvor code path that needs to render a configuration value in a human-readable message cannot
use the secret type directly and must be given a redacted rendering by Renvor.

- **What `secrecy` actually supplies**: restricted access through `ExposeSecret` (making every
  read explicit and greppable), zeroization on drop via `zeroize`, a **redacted `Debug`**, and a
  refusal to serialise unless the author opts in with the `SerializableSecret` marker.
- **What Renvor's boundary type must own** — because FR-018 requires redaction in **every** output
  form the kernel can produce, and `secrecy` covers exactly one of them:

  | Output path | Owner | Required behaviour |
  |---|---|---|
  | `Debug` | `secrecy` | redacted placeholder |
  | **`Display`** | **Renvor** | renders `[REDACTED]`; **`secrecy` provides nothing here** |
  | **error messages / error context** | **Renvor** | value never enters the message or the context map |
  | **structured tracing / log fields** | **Renvor** | the field records a placeholder, not the value |
  | **any serialization path** | **Renvor** | serialization is **refused**; the `SerializableSecret` marker is deliberately **not** implemented |

- **Testing obligation**: the redaction suite must exercise **every** path above, and must include
  a **positive-control leaking wrapper** — a type that deliberately does *not* redact — proving
  the assertions can actually detect a leak. A redaction suite that only ever sees redacting types
  cannot distinguish "nothing leaked" from "the check never fired".
- **Caveat recorded**: `secrecy` was last published **2024-10-09 (675 days ago)**. Small, stable,
  145 M downloads, **0** advisories — staleness here reads as "finished", but that is a judgement
  a reviewer may challenge. Mitigation: the boundary type is Renvor's, so replacing `secrecy`
  later is an internal change.
- **Alternatives considered**: a fully hand-rolled `Secret<T>` — rejected; it would also have to
  reimplement zeroization, which is exactly the "custom cryptographic mechanism" principle III
  forbids. Only the *output contract* is Renvor's, not the memory handling.

### D3 — Cancellation: `tokio-util::sync::CancellationToken`

- **Decision**: use `CancellationToken` for FR-023/FR-024.
- **Rationale**: hierarchical child tokens map onto per-provider scopes, and it is the primitive
  the rest of the planned stack (Tokio, Axum, Tower — named in the constitution) already speaks.
- **Alternatives considered**: broadcast-channel shutdown — no hierarchy, no `cancelled()` future.
  `futures::AbortHandle` — aborts a task rather than signalling cooperative shutdown, the wrong
  semantics for a bounded drain.

### D4 — Observability: `tracing`, and **the library never installs a global subscriber**

- **Decision**: emit one span per lifecycle phase with `tracing`. **Building an `Application`
  installs nothing.** The bootstrap API **returns** a subscriber/layer for the author to install;
  a global-install helper exists only as an explicitly-named, explicitly-documented convenience.
- **Rationale**: a process has exactly one global subscriber, and installing it is a
  **process-wide, once-only, irreversible** decision. A library that takes it during `build()`
  has made a choice that belongs to the binary — and silently overrides or silently loses to
  whatever the author configured. Constitution principle I prohibits opaque runtime behaviour and
  FR-029 requires initialisation to be explicit and never a side effect of building an
  application.
- **Shape**:

  | API | Behaviour |
  |---|---|
  | `Application::build()` | installs **nothing**; emits through the `tracing` facade, which is a no-op when no subscriber is installed |
  | `renvor::observe::layer()` / `subscriber()` | **preferred** — returns a composable value; the author installs it |
  | `renvor::observe::try_init_global()` | optional helper; documented as **process-wide**; returns `Err(AlreadyInstalled)` on a second attempt — **never panics, never silently succeeds, never silently replaces** |

  The deterministic already-installed outcome is the point: "safe to attempt more than once"
  (FR-029) must mean a *specified* result, not an unspecified one that happens not to crash.
- **Advisory note**: `tracing-subscriber` **RUSTSEC-2025-0055** (ANSI escape injection through log
  fields) affects `< 0.3.20`. The selected **0.3.23** is outside that range — verified against the
  advisory's own affected-range data.
- **Alternatives considered**: `log` — no structured fields, which FR-043 requires. Installing a
  global subscriber in `build()` — rejected above. Requiring the author to write subscriber setup
  from scratch — rejected as needless friction; returning a value is the middle path.

### D5 — Run-identifier entropy: `getrandom` behind a Renvor `EntropySource`

- **Decision**: 16 bytes from `getrandom` (the OS CSPRNG) at **exactly one generation site**,
  reached through an `EntropySource` trait with `OsEntropy` (production) and a fixed-byte source
  (tests).
- **Rationale**: FR-043 and SC-019 require opacity **by construction** plus a **deterministic**
  acceptance test. The trait exists solely to make SC-019(b) possible: with fixed entropy the
  identifier becomes a pure function of the supplied bytes, testable with no probabilistic
  assertion.
- **Alternatives considered**: `uuid` v4 — larger dependency for the same 128 bits, and its
  version/variant bits make "pure function of the supplied bytes" awkward to state honestly.
  `rand` — a distribution/PRNG framework where only OS bytes are needed. Reading `/dev/urandom` —
  platform-specific and the custom-primitive plumbing principle III forbids.

### D6 — Configuration: `confique` **on probation**, with an eight-obligation proof gate

FR-044 mandates **decode each source, then merge the decoded layers**, and mandates that an
incompatible structural shape across layers **fails**. The two mainstream crates do the opposite:

| Package | Order | Shape conflict (table vs scalar) | Verdict |
|---|---|---|---|
| `figment` 0.10.19 | merges an **untyped `Value` tree**; `extract()` deserialises at the end | documented: *"join and adjoin use the existing value; merge and admerge use the incoming value"* — **last-wins, no error** | violates FR-044 order and (c) |
| `config` 0.15.25 | merges an untyped tree; `try_deserialize()` at the end | no documented shape-conflict error; later source wins | violates FR-044 order and (c) |
| `confique` 0.4.0 | deserialises **each source into a per-source partial `Layer`**, then composes with `with_fallback` | a value that cannot decode into the declared type fails **in that layer** | matches the mandated **order** |

Under merge-then-decode, a key that is a table in a file and a scalar in the environment is
resolved by picking one, **silently** — the silent fallback constitution principle IV prohibits.

**Four limits of `confique`, read from its published source and documentation.** Revision 1 stated
none of them:

1. **`with_fallback` is `Option::or`, field by field, and discards which layer won.** Source:
   *"Combines two layers. `self` has a higher priority; missing values in `self` are filled with
   values in `fallback`, if they exist. The semantics of this method is basically like in
   `Option::or`."* There is **no source attribution** anywhere in the trait. FR-016 and FR-044
   require the resolved source layer for **every** key to remain reportable, so **Renvor must own
   attribution outright** — and `with_fallback` cannot contribute to it.
2. **An unparseable *empty* environment value is treated as unset.** Documented: *"If the env var
   is set to an empty string and if the field fails to parse/deserialize/validate, it is treated
   as unset."* The source implements this as `Err(_) if is_empty => Ok(None)` in **three** places
   in `internal.rs`. **This is a silent fallback**: `PORT=""` for an integer field silently drops
   through to the next layer instead of failing. FR-044 requires an undecodable environment value
   to fail at Validate naming key, layer, and expected type — **with no empty-string exemption**.
3. **Environment variables exist only for explicitly annotated fields** (`#[config(env = "KEY")]`);
   source: *"fields annotated with `#[config(env = "...")]`"*. Env is therefore a **per-field
   opt-in**, not an orderable layer with a precedence position, which is what FR-044 defines.
4. **Array replacement is an inference, not a documented guarantee.** It follows from `Option<Vec<T>>`
   layer fields composed with `Option::or`, but confique documents no array behaviour. FR-044(b)
   is load-bearing, and an inference is not evidence.

- **Decision**: `confique` is **provisionally selected on probation** for per-source layer
  decoding and the partial-layer derive, behind a Renvor configuration boundary that owns
  everything above. It is **not** confirmed until the proof gate passes.

- **The proof gate — the first configuration task MUST demonstrate all eight**:

  | # | Obligation | Prior evidence |
  |---|---|---|
  | 1 | precedence `defaults < earlier TOML < later TOML < environment` | needs Renvor ordering; confique composes but does not order files |
  | 2 | per-key nested-table merge | supported via `#[config(nested)]` |
  | 3 | wholesale array replacement | **inference only** (limit 4) |
  | 4 | **source attribution for every resolved key** | **known negative** — `with_fallback` discards it (limit 1) |
  | 5 | invalid **non-empty** environment decoding fails | supported |
  | 6 | invalid **empty** environment decoding **also** fails rather than falling back | **known negative** — documented as "treated as unset" (limit 2) |
  | 7 | structural conflicts fail naming **both** layers | needs Renvor detection |
  | 8 | **0** JSON/YAML features in the resolved dependency graph | achievable — features are opt-in |

- **Expected outcome, stated in advance rather than discovered later**: obligations **4 and 6 have
  known negative primary-source evidence**. Obligation 6 is recoverable — if Renvor's adapter
  reads and decodes the environment itself, confique's env path is never invoked and its
  empty-string exemption never applies. Obligation 4 is **not** recoverable through confique:
  attribution must come from Renvor's own merge, and once Renvor performs the merge,
  `with_fallback` contributes nothing. **The realistic outcome is that the gate fails on
  obligation 4 and the fallback triggers.** That judgement does not pre-empt the gate — the task
  runs and the maintainer decides — but it is recorded so the result is not presented later as a
  surprise.

- **Fallback, already recorded and now expanded**: on failure of **any** obligation, adopt a
  **Renvor partial-layer adapter over `serde` + `toml`**, owning decode-per-source, ordered
  layering, attribution, and structural-conflict detection. That is custom infrastructure under
  FR-035 and **requires ADR coverage — reviewed and accepted per D11 — before it merges**.
  `conflate` and `partially` are held in reserve as merge helpers beneath it.

- **Alternatives considered**: `schematic` 0.19.7 — layered partials with explicit field-level
  merge strategies, the closest competitor; rejected as a full config framework with codegen and
  schema export for a phase whose scope is one typed config. It is the **first candidate to
  re-examine** if the fallback triggers, before writing the adapter.

### D7 — TOML parsing and typed decoding: `toml` + `serde`

- **Decision**: `toml` for parsing, `serde` for the decode step; no second format.
- **Rationale**: FR-015 permits exactly TOML; `toml` is the reference implementation, actively
  maintained (18 days). FR-038 requires malformed input to produce a bounded, actionable error —
  `toml` returns **spanned** errors, which is what makes FR-016's "name the key" achievable.
- **Alternatives considered**: `basic-toml` — smaller, drops spans, directly weakens FR-016.
  `toml_edit` alone — format-preserving editing is capability this phase does not need.

### D8 — Provider dependency resolution: **`petgraph::algo::tarjan_scc`, single pass, instrumented**

**Revision 1 got this wrong twice, and both errors pointed the same way — toward unnecessary
custom code.**

**Error 1 — the wrong petgraph function was evaluated.** Revision 1 rejected petgraph because
`toposort` returns `Cycle<NodeId>`, whose only method is *"Return a node id that participates in
the cycle"*. That is true of `toposort` and **irrelevant**, because `tarjan_scc` exists. From the
published source of petgraph 0.8.3:

```rust
pub fn tarjan_scc<G>(g: G) -> Vec<Vec<G::NodeId>>
where
    G: IntoNodeIdentifiers + IntoNeighbors + NodeIndexable,
```

> *"Return a vector where each element is a strongly connected component (scc). The order of node
> ids within each scc is arbitrary, but the order of the sccs is their postorder (reverse
> topological sort)."*
> *"Time complexity: **O(|V| + |E|)**. Auxiliary space: O(|V|)."*

Each component contains **every** node of that component — satisfying FR-013 — and the components
come back in **reverse topological order**, satisfying FR-012 from the same call.

**Error 2 — Kahn + Tarjan cannot fit the budget.** Kahn's algorithm examines each node twice
(enqueue, dequeue) and each edge twice (in-degree build, relaxation). That is **the entire
allowance** of 2 per provider and 2 per edge. Any subsequent cycle-reporting traversal — Tarjan or
otherwise — pushes the totals to 3N and 3E and **breaks the approved budget**. Revision 1's design
was infeasible against its own numbers. The budget is **not** being raised; the design is.

- **Decision**: **one** `tarjan_scc` pass over an **instrumented, deterministic graph adapter**
  owned by Renvor.

- **Why it fits the budget** — counted from the source, not assumed:

  | Operation | Where | Count |
  |---|---|---|
  | outer scan `for n in g.node_identifiers()` | `run()` | **1 provider examination per provider** |
  | `self.visit(n, …)`, guarded by `rootindex.is_none()` | `run()`/`visit()` | **1 more per provider** — each node is visited at most once |
  | `for w in g.neighbors(v)` — the **only** `neighbors` call in the file | `visit()` | **1 edge examination per edge** |

  Totals: **2 provider examinations per provider**, **1 edge examination per edge**. At the
  ceilings the observed counters are **2048** provider examinations, **8192** edge examinations,
  and **10240** total work units, against allowances of **2048**, **16384**, and **18432** —
  **inside the budget on all three axes, with the edge axis at half its allowance**.

- **How the counters are obtained**: `tarjan_scc` is generic over three traits. Renvor implements
  `IntoNodeIdentifiers`, `IntoNeighbors`, and `NodeIndexable` on its **own** adapter type holding
  `Cell<u32>` counters. Every node identifier yielded and every neighbour yielded passes through
  Renvor code, so the counters are **exact observations at a boundary Renvor owns** — not
  estimates and not petgraph internals.
- **Determinism**: the adapter yields node identifiers and neighbours in **registration order**
  from a compact `Vec` adjacency list, so the traversal — and therefore the resulting order — is
  fully determined by the provider set.
- **Edge direction is load-bearing**: edges are directed **dependent → dependency**. Reverse
  topological order of that graph lists dependencies before dependents, so `tarjan_scc`'s output
  order, flattened, **is the initialisation order** — with no reversal pass and no second
  traversal.
- **Cycle reporting**: any returned component with more than one node — or a single node carrying
  a self-edge — is a cycle, and the component **is** the complete member list FR-013 demands.
- **Known risk, recorded rather than discovered**: the implementation is **recursive** —
  *"This implementation is recursive and does one pass over the nodes"*, and `visit()` calls
  itself. Recursion depth equals the longest dependency chain, bounded by the **1024** provider
  ceiling. This is a concrete argument *for* that ceiling. A test constructing a **1024-node
  linear chain** — the maximum achievable depth — is **required**, so the stack claim is evidence
  rather than assumption.
- **ADR status**: **none required.** This is adoption of a maintained package plus an adapter that
  exists to *use* it. FR-035 governs custom infrastructure chosen **over** a package; here the
  package is chosen.
- **Alternatives considered**:
  - **Custom single-pass iterative Tarjan** — feasible and avoids a dependency, and its counters
    would be contract-defined rather than observed at a trait boundary. Rejected under principle
    III: it reimplements a maintained, documented, `O(V+E)` algorithm to own it, and it would
    require an ADR, a governance gate, and permanent maintenance. Its one genuine advantage —
    an explicit stack instead of recursion — does not bind below 1024 nodes.
  - **`petgraph::algo::toposort`** — one-node cycle reporting; would need a second pass, which is
    the budget error above.
  - **`pathfinding`'s strongly-connected-components** — comparable capability; not evaluated
    further once petgraph, already the ecosystem default and already licence- and MSRV-clean, was
    shown to satisfy every requirement. Recorded as unexamined rather than as rejected.
  - **`topological-sort`** — no cycle-member reporting, no counters.

### D9 — Typed state map: `std::any::TypeId` in-house, **ADR required**

- **Decision**: `HashMap<TypeId, StateEntry>` where `StateEntry` carries the boxed value **and**
  its `&'static str` type name. Recorded in **ADR-0007** per FR-035.
- **Rationale**: FR-011 requires duplicate registration to error **naming the type**, and FR-037(b)
  permits the kernel to emit a state value's **type name only**. Both require the name to be stored
  **beside** the value. An any-map stores values keyed by type and returns them; it has nowhere to
  put the metadata, so adopting one means maintaining a **second parallel map** of names in
  lockstep with the first — two structures that can disagree, in service of code that is otherwise
  a `std` `HashMap`.
- **Correction to revision 1**: revision 1 gave `anymap3`'s licence expression
  `BlueOak-1.0.0 OR MIT OR Apache-2.0` as a reason to reject it. That was **overstated** — the
  `OR` means `cargo-deny` resolves to an allowed term and the policy passes. The licence is a
  footnote, not a reason. The reason is metadata colocation, above.
- **Alternatives considered**: `anymap3` 1.1.0 — maintained successor to the unmaintained `anymap`,
  a legitimate option, rejected on metadata colocation. `http::Extensions` — drags an HTTP crate
  into the transport-independent core, contradicting principle II outright. `state` —
  global/lazy-static oriented, colliding with FR-032's prohibition on hidden global mutable state.

### D10 — Test time control: Tokio's `test-util`

- **Decision**: satisfy FR-031 with `tokio::time::pause()`/`advance()` under `test-util`; no custom
  clock in the kernel's public surface.
- **Rationale**: kernel deadlines are Tokio timers, and pausing Tokio's clock exercises the real
  timer path rather than a parallel fake — what principle IX asks for. A `Clock` trait threaded
  through every deadline would add public surface to a facade FR-036 requires to be narrow.
- **Alternatives considered**: a Renvor `Clock` trait — rejected above. `tokio-test` — useful for
  task-level assertions; a possible dev-dependency, not needed for FR-031.

### D11 — ADR-0007 governance: **acceptance is a human gate, not an automatic one**

- **Decision**: ADR-0007 (typed-state map; and the configuration adapter **if** D6's fallback
  triggers) **MUST NOT** be marked accepted as a by-product of this phase's work. It requires a
  **qualified independent review**, or a **separately proposed, separately approved, policy-
  compliant waiver** carrying all seven mandatory fields.
- **Resolved 2026-08-16: the second route was taken.** **W-004** was proposed, reviewed, and
  merged to `main` in its own pull request (`19605e9`) *before* any Phase 002 implementation
  began. It waives the independent-review requirement for **ADR-0007 and nothing else**, under
  **4 counted compensating controls** — reduced from 7 during review, because three of the drafted
  controls restated FR-035, principle III, and the unconditional CI gates, which the ledger bars
  from being cited. The reviewer field of ADR-0007 therefore reads exactly
  **`Ahmed Anbar — self-review under W-004`**, and **no independent review has occurred**.
- **What W-004 does not buy, recorded here because it is easy to misread.** Adversarial review of
  the waiver concluded that **no mechanism can block a bad ADR-0007**. One control — the executable
  proof gate — constrains the record's *scope* by measurement rather than prediction. The others
  build an audit trail for a reviewer who does not yet exist. A refused finding of severity HIGH or
  above must therefore be carried forward as a **named open item** to the first qualified
  independent reviewer.
- **Why this needs saying**: two waivers already exist and **neither reaches this decision**.

  | Waiver | Covers | Does **not** cover |
  |---|---|---|
  | **W-002** | the independent-review requirement for **Phase 001 decision records** (spec FR-013) | a **Phase 002** ADR |
  | **W-003** | **Phase 001's phase-level** independent requirements-and-security review (`PLAN.md` §6.1 step 10 / Phase 001 `001-T088`) | any decision record, in any phase |

  W-003's own scope block states it "waives only the independent-human-review requirement for
  Phase 001" and that it "does not waive any finding, failed check, missing evidence, acceptance
  criterion, or security blocker". Treating either waiver as blanket authority to self-accept a new
  ADR would be **exactly** the scope creep both records were written to prevent.
- **What "qualified independent review" means**, since five artifacts required one without ever
  saying what it is. A reviewer satisfies it when **all four** hold:

  | # | Criterion | Why it is on the list |
  |---|---|---|
  | 1 | **A person**, accountable for the review under their own name | an agent cannot hold accountability, so it cannot supply independence regardless of quality |
  | 2 | **Did not author** the artifact under review, nor direct its authoring | self-review is the failure mode the requirement exists to prevent |
  | 3 | **Competent in the subject** — here, Rust API design and the specific ecosystem alternatives ADR-0007 rejects | a review that cannot evaluate the rejected alternatives cannot evaluate the decision |
  | 4 | **Able to reject** without needing the author's consent | a reviewer who cannot say no is a signatory, not a reviewer |

  Renvor currently has **one** maintainer, who authored these artifacts. Criteria 1, 2, and 4
  therefore cannot be satisfied by anyone available today. That is a **staffing fact**, not a
  process defect, and it is precisely the situation a waiver exists to handle — which is why the
  route below is a waiver rather than a reinterpretation of "independent".
- **Consequence for sequencing**: the governance gate is a **blocking predecessor** of any task
  that merges the custom typed-state map or the custom configuration adapter. It is a human
  decision and cannot be discharged by an agent review, which remains **advisory and
  non-independent** under W-003 — and remains advisory and non-independent under any waiver,
  because a waiver removes the *requirement*, never the *fact*.
- **Alternatives considered**: extending W-002 to cover Phase 002 — rejected; a waiver is
  amended by re-justification and re-dating, not by reinterpretation. Deferring the ADR until
  after implementation — rejected; FR-035 requires an **accepted** ADR to justify custom
  infrastructure, and an ADR written to ratify code already merged is a record of what happened,
  not a decision.

### D12 — Lockfile policy: ADR-0003 and FR-040 govern **different objects**, and both hold

- **The apparent conflict.** ADR-0003's dependency table records that **reusable library crates**
  use compatible version requirements and a lockfile that is **"Not committed"**, while
  applications, release tooling, and automation commit theirs. FR-040 requires every dependency's
  version to be resolvable **from a committed lockfile**. This workspace tracks `Cargo.lock`. Read
  carelessly, one of those three statements has to be wrong. Readiness item **CHK044** raised it.
- **Decision**: both statements stand, because the table's two columns have **different owners**.
  - The *version-requirement* column is a property of **each crate's manifest**. Every crate this
    phase adds is a reusable library crate, so each declares compatible requirements (`1.2`) and
    **0** exact pins — that half of the row is honoured literally.
  - The *lockfile* column is a property of the **workspace**, not of a crate, because Cargo
    maintains **exactly one** `Cargo.lock` per workspace and offers no per-member option. This
    workspace contains `xtask` — release tooling and automation — which ADR-0003's **second** row
    requires to commit its lockfile. One lockfile, one governing row, and it is the automation row.
- **Why it is not a contradiction**: ADR-0003's first row describes the common case of a *standalone*
  published library crate, where the workspace and the crate are the same thing. Renvor is a
  mixed workspace, so the two rows apply to different objects within it rather than to the same
  object twice. The committed lockfile constrains **resolution**, not the published requirement
  ranges a downstream consumer sees — a consumer of a published Renvor crate still resolves against
  its own lockfile and is unaffected by this one.
- **Consequence**: FR-040 is satisfiable as written, and T034 records this reconciliation in the
  phase evidence. If ADR-0003 is ever revised, the row that governs here is the automation row.
- **Alternatives considered**: untracking `Cargo.lock` to match the library row literally — rejected,
  it would break the automation row, make `cargo deny` non-reproducible, and leave FR-040 with
  nothing to resolve against. Amending ADR-0003 — rejected for this phase; nothing in it is wrong,
  the table is simply read as if "lockfile" were a per-crate property. A clarifying note there is a
  reasonable future change, not a Phase 002 blocker.

### D13 — The publishable set grows from 1 crate to 4, and the release rehearsal must follow

- **How this was found.** Not by reading. `crates/renvor/Cargo.toml` carries the comment *"No
  dependencies. A publishable package may not carry a git or path dependency (FR-040); declaring
  none is the strongest form of compliance."* Phase 002 gives the facade dependencies, so that
  comment stops being true — which prompted running the command instead of reasoning about it.
- **What the experiment showed.** A four-case sandbox, run on cargo 1.94.0:

  | Case | Command | Dependency `publish` | Result |
  |---|---|---|---|
  | 1 | `cargo publish -p facade --dry-run` | `false` | **fails** — *no matching package found, location searched: crates.io index* |
  | 2 | `cargo publish -p facade --dry-run` | `true` | **fails identically** — publishability does not help; the dependency must actually *exist on the registry* |
  | 3 | `cargo publish --dry-run --workspace` | `true` | **succeeds** — cargo stages a temporary registry, unpacks the member, and verifies the chain |
  | 4 | `cargo publish --dry-run --workspace` | `false` | **fails** — an unpublishable member breaks the chain for every publishable crate above it |

  Case 2 is the one worth keeping: the obvious fix — "mark the new crates publishable" — does
  **not** rescue the single-crate rehearsal. Only the workspace form works, and only with the
  whole chain publishable.
- **Decision**: `renvor-core`, `renvor-config`, and `renvor-testkit` are **`publish = true`** with
  the complete metadata set from the Phase 001 package-metadata contract; the facade depends on
  them with **`{ path, version }`**, never path-only; and the release rehearsal moves from
  `-p renvor` to `--workspace`.
- **Why `publish = true` does not conflict with FR-034.** FR-034 forbids **publishing**. `publish
  = true` is a manifest attribute stating a crate *may* be published; it publishes nothing. The
  phase still ends with 0 crates, 0 tags, and 0 releases, asserted at T110.
- **Why this is not an ADR-0002 supersession.** ADR-0002 anticipated it in its own text — *"Later
  phases add implementation crates behind it"* — and Phase 001 FR-040 forbids a **path-*only***
  dependency, not a `path` + `version` one, which is the form cargo requires and the form used
  here. What does become stale is ADR-0002's traceability line *"the facade declares zero git or
  path dependencies"*, which was a Phase-001-scoped consequence rather than a standing rule; it is
  recorded as superseded-in-fact by this phase rather than silently left to contradict the code.
- **`xtask` is unaffected** — it stays `publish = false` and stays excluded from the rehearsal.
- **Alternatives considered**: keeping the facade dependency-free and having users depend on
  `renvor-core` directly — rejected, ADR-0002 rejects exactly that ("every internal reorganisation
  becomes a breaking change for users"). Marking the facade `publish = false` for this phase —
  rejected, it contradicts ADR-0002's publishable table and hides the problem rather than solving
  it. Adding `--no-verify` to the rehearsal — rejected, it would make the rehearsal pass by
  checking less, which is the failure mode the whole gate exists to prevent.

## 2. Rejected wholesale

| Considered | Why it is not here |
|---|---|
| `anyhow` | type-erased; defeats FR-019's matchable category |
| `uuid` | larger dependency for the same 128 bits; version/variant bits complicate SC-019(b)'s purity claim |
| `figment`, `config` | merge-then-decode with silent last-wins on shape conflict — contradicts FR-044 and principle IV |
| `petgraph::algo::toposort` | one-node cycle reporting; a second pass to fix it breaks the work budget *(the crate itself is **adopted** — see D8)* |
| `anymap3` | no place for the type-name metadata FR-011/FR-037b require; would need a second parallel map |
| `async-trait` | edition 2024 on MSRV 1.94.0 supports async fn in traits natively for this shape; revisit only if dyn-compatibility forces it |
| any HTTP, persistence, or CLI crate | FR-033 excludes them from this phase |

## 3. Direct-dependency candidate evaluation *(FR-040, SC-012, SC-017)*

> **This table is a candidate evaluation of *direct* dependencies. It is NOT the complete
> dependency inventory FR-040 requires.** No manifest and no resolved lockfile exist yet, so the
> **transitive** graph is unknown — and a transitive dependency is exactly as capable of carrying
> an incompatible licence or a live advisory as a direct one. The complete inventory is a
> **required task** (§5, follow-up 5) that runs **after** manifests and `Cargo.lock` exist and
> **before** adoption is confirmed or any custom-infrastructure task merges.

All figures read 2026-08-16. **MSRV is the crate's own declared `rust-version`** from the sparse
index; Renvor's floor is **1.94.0**.

| Crate | Version | Licence | Declared MSRV | ≤ 1.94.0 | Advisories affecting this version | Last release | Downloads |
|---|---|---|---|---|---|---|---|
| `thiserror` | 2.0.20 | MIT OR Apache-2.0 | 1.71 | yes | **0** | 7 d | 1.32 B |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | 1.56 | yes | **0** | 27 d | 1.27 B |
| `toml` | 1.1.4 | MIT OR Apache-2.0 | 1.85 | yes | **0** | 18 d | 820 M |
| `confique` *(probation, D6)* | 0.4.0 | MIT OR Apache-2.0 | 1.68.2 | yes | **0** | 292 d | 408 K |
| **`petgraph`** | **0.8.3** | **MIT OR Apache-2.0** | **1.64** | **yes** | **0** | **319 d** | **467 M** |
| `tokio` | 1.53.1 | MIT | 1.71 | yes | **0** *(10 historical; every range below 1.53.1)* | 26 d | 881 M |
| `tokio-util` | 0.7.19 | MIT | 1.71 | yes | **0** | 25 d | 713 M |
| `tracing` | 0.1.44 | MIT | 1.65.0 | yes | **0** *(RUSTSEC-2023-0078 affects < 0.1.40)* | 240 d | 773 M |
| `tracing-subscriber` | 0.3.23 | MIT | 1.65.0 | yes | **0** *(RUSTSEC-2025-0055 affects < 0.3.20)* | 155 d | 551 M |
| `getrandom` | 0.4.3 | MIT OR Apache-2.0 | 1.85 | yes | **0** | 59 d | 1.79 B |
| `secrecy` | 0.10.3 | Apache-2.0 OR MIT | 1.60 | yes | **0** | **675 d** | 145 M |
| `zeroize` *(via `secrecy`)* | 1.9.0 | Apache-2.0 OR MIT | 1.85 | yes | **0** | 64 d | 621 M |

**Licence policy**: every expression resolves within `deny.toml`'s allow-list. **0** candidates
require an exception — *for the direct set only*; the transitive set is unmeasured.

**Highest MSRV demanded by this set: 1.85** (`toml`, `getrandom`, `zeroize`). Renvor's floor of
**1.94.0** clears it by nine minor versions, so **R-7 is answered for this phase's direct
dependencies** — pending the transitive inventory. R-7 remains live for **Phase 006**.

**Feature cost** — features are enabled explicitly; defaults are disabled where a default pulls
capability this phase does not use:

| Crate | Features enabled | Deliberately excluded |
|---|---|---|
| `tokio` | `rt`, `time`, `sync`, `macros`; `test-util` **dev-only** | `net`, `fs`, `process`, `signal`, `full` |
| `petgraph` | **minimal** — `tarjan_scc` needs only the algo traits | `graphmap`, `stable_graph`, `matrix_graph`, `serde-1`, `rayon` |
| `confique` | `toml` | `yaml`, `json5` — FR-015 prohibits both; excluding the features makes the prohibition **structural** |
| `tracing-subscriber` | `fmt`, `env-filter` | `json`, `ansi` |
| `secrecy` | default (`alloc`, `zeroize`) | `serde` — evaluated against FR-018 before enabling |
| `getrandom` | default | custom backends |

## 4. Design questions resolved without a package

- **Reverse order means reverse *actual initialisation* order.** Resolution may reorder providers
  relative to registration, so the kernel records the realised sequence and replays it backwards.
- **Budget exhaustion is an internal error, not a diagnostic.** FR-039(c) forbids reporting a cycle
  by running out of budget, so the counters terminate with a distinct internal-error variant.
- **A zero drain budget is a valid configuration.** FR-042 makes zero mean "stop now, and still
  report outstanding work", on the same code path a timeout uses.
- **Health and readiness are separate values.** FR-026 requires them to be able to disagree;
  deriving one from the other makes SC-008 unsatisfiable by construction.
- **The framework never owns the process's subscriber** (D4).
- **Attribution is Renvor's, not the config crate's** (D6 limit 1).

## 5. Follow-ups this research creates

1. **ADR-0007 must be drafted, independently reviewed, and accepted before the custom code
   merges** (FR-035 + D11). Scope: the typed-state map (D9), plus the configuration adapter **only
   if** D6's gate fails. Neither W-002 nor W-003 authorises accepting it.
2. **The configuration proof gate (D6) runs first**, with all **eight** obligations. Two have known
   negative evidence; the gate decides, not this document.
3. **The provider-resolver counter proof runs early** — it must demonstrate ≤ 2048 provider and
   ≤ 8192 observed edge examinations at the ceilings, **and** survive a 1024-node linear chain
   without exhausting the stack.
4. **ADR-0002 supersession is not performed here** — Phase 004+ work under FR-036.
5. **A complete resolved-dependency inventory is required** once manifests and `Cargo.lock` exist
   and before adoption is confirmed: every **transitive** package with version, licence, MSRV
   compatibility, and advisory status; `cargo deny check` run against the **actual lockfile**;
   enabled features and duplicate-version findings recorded; **failing** if any dependency lacks
   the evidence FR-040 requires.
6. **`secrecy`'s maintenance status should be re-checked before public release** (675 days).
