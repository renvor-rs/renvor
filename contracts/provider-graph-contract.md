---
description: "Phase 002 contract — provider registration, single-pass dependency resolution, graph ceilings, and the deterministic work budget"
version: "1.2.0"
status: "unstable — the surface it describes is explicitly unstable under FR-036; this version identifies the contract text, not a stability promise"
---

# Contract: Provider Graph

**Feature**: the phase specification *(internal record)* | **Satisfies**: FR-012…FR-014, FR-039; SC-005, SC-021
**Status**: contract for an **explicitly unstable** surface (FR-036). *Revision 2 — resolver redesigned.*

> **Revision 2 corrects a design that could not meet its own budget.** Revision 1 specified Kahn's
> algorithm for ordering plus Tarjan for cycle reporting. Kahn alone examines each node twice and
> each edge twice — **the entire allowance** — so any second traversal broke the budget. It also
> rejected `petgraph` on a property of the wrong function. **The budget is unchanged; the design
> is.** See the phase research record *(internal record)* D8.

## C-G1 — Two independent families of bound

A single undifferentiated "resource limit" cannot say whether a rejection means *the author's
graph is too big* or *resolution misbehaved*. This contract keeps them apart.

| Family | Checked | Bound |
|---|---|---|
| **Declared graph size** | at Register, **before traversal** | ≤ **1024** providers, ≤ **8192** declared edges |
| **Resolution work** | during traversal | ≤ **2** provider examinations per accepted provider, ≤ **2** edge examinations per accepted edge |

At the maximum accepted graph size the **allowances** are **2048** provider examinations, **16384**
edge examinations, and **18432** total work units.

### What one **examination** is

A budget stated in units nobody defined is not a budget. These are the definitions, and they are
normative — two independent implementations counting by them **MUST** arrive at the same numbers.

| Unit | Incremented **exactly once** per | Not counted |
|---|---|---|
| **provider examination** | each yield of a node identifier from the adapter's node-identifier iterator, **and** each **call to** the adapter's neighbour function | reading a provider's own fields; formatting it into a diagnostic; yielding a node **from** a neighbour iterator |
| **edge examination** | each **advance** of a neighbour iterator that yields an edge's target | constructing the adjacency list before traversal; iterating edges to build a cycle diagnostic **after** the verdict |
| **total work unit** | the arithmetic sum of the two counters above | — nothing separate is counted here |

> **Corrected 2026-08-16 (T021–T025), and the correction is the point of writing the units down.**
> The provider-examination row previously read *"each yield of a node identifier … **and** each
> yield of a node from a neighbour iterator"*. Counting it that way gives `providers + edges`
> provider examinations — **9216** at the ceilings — against an allowance of **2048**. The design
> would have failed its own budget by a factor of four and a half, and no implementation could
> have satisfied both this row and the **2048** recorded three sections below in C-G4.
>
> It was also **unobservable**. C-G6 requires every counted event to pass through Renvor code, and
> the second provider source in C-G4 is petgraph's internal `visit()`, which Renvor cannot see.
> What Renvor *can* see is the `neighbors(v)` **call** `visit()` makes — and reading petgraph
> 0.8.3's source shows it makes exactly one such call per provider, because `visit` is guarded so
> each node is visited at most once and its neighbour loop never exits early. So the observable
> `1 × providers` **is** the unobservable one, counted at the boundary Renvor owns.
>
> The correction is to the **definition**, not to the budget: the allowances (2048 / 16384 /
> 18432) and the observed values (2048 / 8192 / 10240) are unchanged, and the implementation now
> reproduces them exactly. Evidence: `crates/renvor-core/tests/resolver_proof.rs`;
> `specs/002-core-kernel/research.md` §D8.

Two consequences follow, and both are intended:

- Counting happens where the traversal **consumes** — at iterator yield, and at the request for a
  neighbour list — not at algorithm entry. The counters are therefore a property of what the
  traversal actually did rather than of what it was asked to consider. Iterator **exhaustion** is
  not a consumption: the final `next` that returns nothing is not counted, or every traversal would
  carry one phantom unit per iterator.
- **Building** the graph and **rendering** a diagnostic are outside the budget. The budget measures
  *resolution*, and folding construction into it would make the number depend on how the adjacency
  list was assembled rather than on how the graph was traversed.

## C-G2 — Size ceilings

- A graph exceeding **either** ceiling **MUST** fail at **Register**, naming the ceiling and the
  observed count.
- It **MUST** be rejected on the **declared counts alone** — never discovered by running out of
  traversal budget.
- A valid acyclic graph **at both ceilings simultaneously (1024 providers, 8192 edges) MUST
  succeed.** A ceiling that cannot be reached is a lower ceiling with a misleading number.

## C-G3 — Work budget is counted, never timed

Enforcement **MUST** count examinations. A wall-clock deadline is prohibited here: it would make
the bound a property of the **host** rather than of the **graph** — flaky under CI load, silently
passing on fast hardware.

- **Acceptance**: SC-021 — **0** of these assertions depend on elapsed wall-clock time.

## C-G4 — Resolution is a **single pass**

Ordering and cycle detection **MUST** come from **one** traversal. A design that orders in one
pass and then detects cycles in another cannot fit the budget, because the first pass alone is
permitted to consume it.

The resolver is **one `petgraph::algo::tarjan_scc` call** over a Renvor-owned adapter. From
petgraph 0.8.3's published documentation:

> *"Return a vector where each element is a strongly connected component (scc). The order of node
> ids within each scc is arbitrary, but the order of the sccs is their postorder (reverse
> topological sort)."* — Time complexity **O(|V| + |E|)**.

One call therefore yields **both** required results: complete cycle membership and a topological
ordering.

**Observed cost, counted from the algorithm's source rather than assumed:**

| Operation | Where Renvor observes it | Counts as | Count |
|---|---|---|---|
| outer scan over node identifiers | a yield of the node-identifier iterator | provider examination | **1** per provider |
| `visit()`, guarded so each node is visited at most once | the `neighbors(v)` **call** `visit()` makes — exactly one per visited node, and every node is visited | provider examination | **1** per provider |
| neighbour iteration — the **only** neighbours call in the implementation | a yield of the neighbour iterator | edge examination | **1** per edge |

The middle row's second column is what makes this table implementable. `visit()` is petgraph's,
not Renvor's, and cannot be counted directly without reading a dependency's internals — which
C-G6 forbids. Its `neighbors(v)` call is a one-for-one proxy that crosses the Renvor boundary, and
the equivalence is a property of the published source, not an approximation.

**Totals: 2 provider examinations per provider, 1 edge examination per edge.** At the ceilings the
observed counters are **2048** provider examinations, **8192** edge examinations, and **10240**
total work units, against allowances of **2048**, **16384**, and **18432** — inside budget on all
three axes, with the edge axis at **half** its allowance.

**These are now measured, not projected.**
`crates/renvor-core/tests/resolver_proof.rs` builds a graph at both ceilings simultaneously and
asserts each of the three figures exactly, and asserts the same `2 × providers` / `1 × edges`
relationship at four graph sizes spanning 4 to 1024 providers.

## C-G5 — Edge direction is normative

Edges **MUST** be directed **dependent → dependency**.

Reverse topological order of a graph so directed lists **dependencies before dependents**, so the
flattened `tarjan_scc` output **is** the initialisation order directly — no reversal pass, no
second traversal. Reversing the edge direction would require an extra pass and reintroduce the
budget problem revision 1 had.

## C-G6 — Counters are observed at a Renvor-owned boundary

The resolver's counters **MUST NOT** be estimates and **MUST NOT** depend on reading a
dependency's internals.

`tarjan_scc` is generic over `IntoNodeIdentifiers + IntoNeighbors + NodeIndexable`. Renvor
implements those three traits on **its own** adapter type over a compact adjacency list, holding
the counters. Every node identifier and every neighbour the algorithm consumes passes through
Renvor code, so `ResolutionReport` records **exact observations**.

A change in the dependency that altered its traversal pattern would change the counters, and the
SC-021 assertions would **fail loudly** — which is the intended behaviour, not a fragility.

## C-G7 — Determinism

The adapter **MUST** yield node identifiers and neighbours in **registration order**. Resolution
output is then fully determined by the provider set: the same providers in the same order produce
the same initialisation order on every run and every machine.

## C-G8 — Cycles are reported as cycles

- A returned component with **more than one node**, or a single node carrying a **self-edge**, is a
  cycle. The component **is** the complete member list, so the diagnostic names **every** provider
  in the cycle.
- A cycle **MUST NOT** be reported by exhausting the work budget.
- **Acceptance**: SC-021 — **0** such runs report budget exhaustion in place of the cycle diagnostic.

## C-G9 — Budget exhaustion is an internal error

Because the budget is a constant multiple of the accepted graph size, **every** graph within the
ceilings — cyclic or acyclic — reaches its verdict inside the budget. Exhausting it indicates a
**defect in resolution itself** and is reported as `Internal`, distinct from every author-facing
diagnostic. If an author ever sees it, the kernel is wrong, not their graph.

## C-G10 — Recursion depth is bounded by the provider ceiling

The selected implementation is **recursive** — its own documentation states *"This implementation
is recursive and does one pass over the nodes"*, and its visit function calls itself. Recursion
depth equals the longest dependency chain, which the **1024** provider ceiling bounds.

**This is a required test, not an assumption**: a **1024-node linear chain** — the maximum
achievable depth — **MUST** resolve without exhausting the stack, and **MUST** be exercised on a
Tokio worker thread, whose default stack is smaller than the main thread's.

The test's conditions are stated numerically, because "it did not crash on my machine" is not a
bound — a stack test that does not pin its stack size measures the host, not the code:

| Condition | Required value |
|---|---|
| chain length | **1024** nodes, each depending on the next — the deepest graph the ceiling admits |
| execution context | a **Tokio worker thread**, not the main thread and not a `block_on` on the main thread |
| worker stack size | pinned **explicitly** via the runtime builder's `thread_stack_size`, **never** inherited |
| pinned value | **2 MiB** (`2 * 1024 * 1024`) — Tokio's *current* documented default, which its own documentation states is **"subject to change in the future"**. That sentence is the reason to pin it: an inherited default that grows would silently relax this test, and one that shrinks would fail it for a reason unrelated to Renvor |
| pass condition | resolution **completes** and returns the correct order; **0** stack overflows |
| what a failure means | the recursive implementation is **rejected**, and the iterative fallback below is triggered — the ceiling is **not** lowered to make the test pass |

The last row matters most. Lowering the provider ceiling until the recursion fits would convert a
proven limit into an unexplained one, and the ceiling is a published number an author designs
against.

> If that test fails, the fallback is a custom **iterative** single-pass SCC resolver, which is
> then custom infrastructure under FR-035 and requires **ADR-0007** coverage and the D11 governance
> gate before it merges. The trigger is written here in advance.

**Outcome, 2026-08-16 (T024): the test passes and the fallback is NOT triggered.** The recursive
implementation resolves the 1024-node chain on the pinned 2 MiB worker stack, and the measured
margin is recorded rather than left as "it did not crash":

| Build profile | Smallest worker stack that resolves the chain | Largest that fails | Headroom at 2 MiB |
|---|---|---|---|
| debug — how CI runs `cargo test` | **512 KiB** | 448 KiB | **≈ 4×** |
| release | **96 KiB** | 64 KiB | **≈ 21×** |

Measured by `crates/renvor-core/examples/stack_depth_probe.rs`, which varies the **stack** and
never the graph — a chain deeper than the ceiling is not a graph Renvor accepts, so the ceiling
stays enforced while the margin is measured. A stack overflow aborts the process and cannot be
caught, so each size is one process invocation.

The worst case is the one CI exercises, and it has roughly four times the stack it needs. No
custom infrastructure is required for the resolver, so **nothing from this section enters ADR-0007's
scope**.

## C-G11 — Missing dependencies

A provider depending on a capability nobody registered **MUST** fail at Register, naming **both**
the dependent and the missing capability. Naming only one leaves the author bisecting.

- **Acceptance**: SC-005 — cycles and missing dependencies are each detected **before any provider
  is booted**; **0** cases reach Boot.
