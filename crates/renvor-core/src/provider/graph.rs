//! The instrumented provider dependency graph and its single-pass resolver.
//!
//! # What this module is for
//!
//! Resolution must answer two questions from **one** traversal (contract C-G4): what order do
//! providers initialise in, and — if they cannot — which providers form a cycle. A design that
//! orders in one pass and detects cycles in another cannot fit the work budget, because the first
//! pass alone is permitted to consume it.
//!
//! [`petgraph::algo::tarjan_scc`] answers both from a single `O(|V| + |E|)` pass: its returned
//! components carry **every** member of each cycle, and the components come back in postorder,
//! which is reverse topological order.
//!
//! # Why Renvor owns the adapter
//!
//! `tarjan_scc` is generic over three traits. Renvor implements them on [`ResolverGraph`], its
//! own compact adjacency list, so **every** node identifier and **every** neighbour the algorithm
//! consumes passes through Renvor code. The counters in [`WorkCounters`] are therefore exact
//! observations at a boundary Renvor controls — not estimates, and not a reading of a
//! dependency's internals (contract C-G6).
//!
//! A change in petgraph that altered its traversal pattern would change these counters and the
//! SC-021 assertions would fail loudly. That is the intended behaviour, not a fragility.
//!
//! # Edge direction is load-bearing
//!
//! Edges are directed **dependent → dependency** (contract C-G5). Reverse topological order of a
//! graph so directed lists dependencies before dependents, so the flattened component order **is**
//! the initialisation order — no reversal pass, no second traversal.
//!
//! # What one examination is
//!
//! The units are normative, because a budget stated in units nobody defined is not a budget. Both
//! are counted where the algorithm actually consumes them, which is also the only place Renvor can
//! observe:
//!
//! | Unit | Incremented exactly once per |
//! |---|---|
//! | provider examination | each yield of [`NodeIdentifiers`], **and** each call to [`IntoNeighbors::neighbors`] |
//! | edge examination | each yield of [`Neighbors`] |
//!
//! `neighbors` is called exactly once per provider, because `TarjanScc::visit` is guarded so each
//! node is visited at most once and its neighbour loop never exits early. So the two provider
//! sources are `1 × providers` each, giving **2 × providers**, and the single edge source gives
//! **1 × edges**. Verified against petgraph 0.8.3's published source, not assumed.
//!
//! A neighbour yield is an **edge** examination and is deliberately **not** also counted as a
//! provider examination. Counting it as both would double-count the same act of traversal and
//! would put provider examinations at `providers + edges`, which exceeds the `2 × providers`
//! allowance at any interesting graph — a budget the design could never meet. See
//! the Phase 002 research record §D8, retained in public Git history rather than the current
//! tree: <https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md>
//!
//! Building the adjacency list and rendering a cycle diagnostic are outside the budget. The budget
//! measures *resolution*; folding construction into it would make the number depend on how the
//! list was assembled rather than on how the graph was traversed.

use core::cell::Cell;

use petgraph::algo::tarjan_scc;
use petgraph::visit::{GraphBase, IntoNeighbors, IntoNodeIdentifiers, NodeIndexable};

/// The maximum number of providers a single graph may declare (contract C-G2).
///
/// This bound is also the bound on recursion depth: `tarjan_scc` is recursive, and the deepest
/// possible recursion is the longest dependency chain, which cannot exceed the provider count.
/// The 1024-node chain test in `tests/resolver_proof.rs` exercises exactly that depth.
pub const MAX_PROVIDERS: u32 = 1024;

/// The maximum number of declared dependency edges a single graph may carry (contract C-G2).
pub const MAX_EDGES: u32 = 8192;

/// A provider's position in registration order.
///
/// Registration order is the whole basis of determinism here (contract C-G7): the adapter yields
/// identifiers and neighbours in this order, so the same providers registered in the same order
/// produce the same initialisation order on every run and every machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderIx(u32);

impl ProviderIx {
    /// Constructs an index from a registration position.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the registration position this index refers to.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A declared dependency edge's position in declaration order.
///
/// `tarjan_scc` never inspects edge identifiers, but [`GraphBase`] requires the associated type,
/// and giving it a real meaning is cheaper than giving it a placeholder that later has to be
/// explained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EdgeIx(u32);

impl EdgeIx {
    /// Returns the declaration position this index refers to.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Why a graph could not be built.
///
/// Both variants name the ceiling **and** the observed count. Naming only the ceiling leaves the
/// author guessing how far over they are; naming only the count leaves them guessing the limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphSizeError {
    /// More providers were declared than [`MAX_PROVIDERS`] permits.
    TooManyProviders {
        /// The number of providers declared.
        declared: u32,
        /// The ceiling that was exceeded.
        ceiling: u32,
    },
    /// More dependency edges were declared than [`MAX_EDGES`] permits.
    TooManyEdges {
        /// The number of edges declared.
        declared: u32,
        /// The ceiling that was exceeded.
        ceiling: u32,
    },
    /// A declared dependency named a provider that is not in the graph.
    ///
    /// This is the graph-construction half of contract C-G11. It names **both** endpoints,
    /// because naming only one leaves the author bisecting.
    UnknownDependency {
        /// The provider that declared the dependency.
        dependent: ProviderIx,
        /// The registration position it named, which no provider occupies.
        named: u32,
        /// The number of providers that exist.
        provider_count: u32,
    },
}

/// Builds a [`ResolverGraph`] one provider at a time, in registration order.
///
/// Providers must be pushed in the order they were registered, each with its dependency list in
/// declaration order. The builder is the only way to construct a graph, which is what makes the
/// determinism guarantee structural rather than a convention callers have to remember.
#[derive(Clone, Debug, Default)]
pub struct ResolverGraphBuilder {
    /// `offsets[i]..offsets[i + 1]` is provider `i`'s slice of `targets`. Always starts with `0`.
    offsets: Vec<u32>,
    targets: Vec<u32>,
}

impl ResolverGraphBuilder {
    /// Creates an empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            offsets: vec![0],
            targets: Vec::new(),
        }
    }

    /// Creates an empty builder with room for `providers` providers and `edges` edges.
    #[must_use]
    pub fn with_capacity(providers: usize, edges: usize) -> Self {
        let mut offsets = Vec::with_capacity(providers + 1);
        offsets.push(0);
        Self {
            offsets,
            targets: Vec::with_capacity(edges),
        }
    }

    /// Appends the next provider in registration order, with its dependencies in declaration
    /// order.
    ///
    /// Dependencies are **not** validated here — a provider may legitimately name one that is
    /// registered later. Validation happens once, in [`Self::build`], when the full provider set
    /// is known.
    pub fn push_provider<I>(&mut self, dependencies: I) -> ProviderIx
    where
        I: IntoIterator<Item = ProviderIx>,
    {
        let index = ProviderIx::new(
            u32::try_from(self.offsets.len() - 1).expect("provider count exceeds u32"),
        );
        for dependency in dependencies {
            self.targets.push(dependency.get());
        }
        self.offsets
            .push(u32::try_from(self.targets.len()).expect("edge count exceeds u32"));
        index
    }

    /// Validates the declared sizes and freezes the graph.
    ///
    /// Rejection is on the **declared counts alone**, before any traversal (contract C-G2). A
    /// graph that is too large is never discovered by running out of traversal budget — that
    /// would report a kernel defect where the author has an oversized graph.
    ///
    /// # Errors
    ///
    /// Returns [`GraphSizeError`] if either ceiling is exceeded, or if a declared dependency names
    /// a provider that does not exist.
    pub fn build(self) -> Result<ResolverGraph, GraphSizeError> {
        let provider_count =
            u32::try_from(self.offsets.len() - 1).expect("provider count exceeds u32");
        if provider_count > MAX_PROVIDERS {
            return Err(GraphSizeError::TooManyProviders {
                declared: provider_count,
                ceiling: MAX_PROVIDERS,
            });
        }

        let edge_count = u32::try_from(self.targets.len()).expect("edge count exceeds u32");
        if edge_count > MAX_EDGES {
            return Err(GraphSizeError::TooManyEdges {
                declared: edge_count,
                ceiling: MAX_EDGES,
            });
        }

        // Building the adjacency list is outside the work budget, so this scan is uncounted by
        // design — see the module documentation.
        for provider in 0..provider_count {
            let start = self.offsets[provider as usize] as usize;
            let end = self.offsets[provider as usize + 1] as usize;
            for &named in &self.targets[start..end] {
                if named >= provider_count {
                    return Err(GraphSizeError::UnknownDependency {
                        dependent: ProviderIx::new(provider),
                        named,
                        provider_count,
                    });
                }
            }
        }

        Ok(ResolverGraph {
            offsets: self.offsets,
            targets: self.targets,
            provider_examinations: Cell::new(0),
            edge_examinations: Cell::new(0),
        })
    }
}

/// A compact, deterministic, instrumented provider dependency graph.
///
/// Storage is compressed adjacency: `offsets` holds one entry per provider plus a terminator, and
/// `targets` holds every dependency back to back in declaration order. Two allocations for the
/// whole graph, and provider `i`'s dependencies are the contiguous slice
/// `targets[offsets[i]..offsets[i + 1]]`.
///
/// The counters use [`Cell`] rather than an atomic because resolution is single-threaded and the
/// algorithm holds the graph by shared reference throughout. That also makes `ResolverGraph`
/// deliberately **not** `Sync`: sharing a counting graph across threads would produce counters
/// that describe no particular traversal.
#[derive(Debug)]
pub struct ResolverGraph {
    offsets: Vec<u32>,
    targets: Vec<u32>,
    provider_examinations: Cell<u32>,
    edge_examinations: Cell<u32>,
}

impl ResolverGraph {
    /// Returns the number of providers in the graph.
    #[must_use]
    pub fn provider_count(&self) -> u32 {
        u32::try_from(self.offsets.len() - 1).expect("provider count exceeds u32")
    }

    /// Returns the number of declared dependency edges in the graph.
    #[must_use]
    pub fn edge_count(&self) -> u32 {
        u32::try_from(self.targets.len()).expect("edge count exceeds u32")
    }

    /// Returns the counters observed so far.
    #[must_use]
    pub fn counters(&self) -> WorkCounters {
        WorkCounters {
            provider_examinations: self.provider_examinations.get(),
            edge_examinations: self.edge_examinations.get(),
        }
    }

    /// Resets the counters to zero, so a graph can be resolved more than once and each resolution
    /// reports its own cost rather than a running total.
    pub fn reset_counters(&self) {
        self.provider_examinations.set(0);
        self.edge_examinations.set(0);
    }

    /// Resolves the graph in a single `tarjan_scc` pass.
    ///
    /// Counters are reset first, so the returned [`Resolution`] describes **this** traversal.
    #[must_use]
    pub fn resolve(&self) -> Resolution {
        self.reset_counters();

        // One pass. Components come back in postorder, which is reverse topological order; with
        // edges directed dependent -> dependency, that is already the initialisation order.
        let components = tarjan_scc(self);

        let counters = self.counters();

        // Everything below this line is diagnostic rendering, which the budget deliberately
        // excludes — the counters were snapshotted above, before any of it runs.
        let components = components
            .into_iter()
            .map(|members| {
                let is_cycle = members.len() > 1
                    || members
                        .first()
                        .is_some_and(|&only| self.has_self_edge(only));
                Component { members, is_cycle }
            })
            .collect();

        Resolution {
            components,
            counters,
        }
    }

    /// Whether a provider declares a dependency on itself.
    ///
    /// A single-node component is a cycle only in this case, and `tarjan_scc` cannot distinguish
    /// the two — both come back as a one-element component. This runs **after** the verdict and is
    /// therefore uncounted, per contract C-G1.
    fn has_self_edge(&self, provider: ProviderIx) -> bool {
        self.dependency_slice(provider)
            .iter()
            .any(|&target| target == provider.get())
    }

    fn dependency_slice(&self, provider: ProviderIx) -> &[u32] {
        let start = self.offsets[provider.get() as usize] as usize;
        let end = self.offsets[provider.get() as usize + 1] as usize;
        &self.targets[start..end]
    }

    /// Counts one provider examination, saturating rather than wrapping.
    ///
    /// Saturation is unreachable below the ceilings — the largest possible value is
    /// `2 * MAX_PROVIDERS`. If it were ever reached, saturating at `u32::MAX` fails **closed**:
    /// the value exceeds every allowance, so the budget check reports exhaustion. Wrapping would
    /// fail open, reporting a small number for an enormous traversal.
    fn count_provider_examination(&self) {
        self.provider_examinations
            .set(self.provider_examinations.get().saturating_add(1));
    }

    /// Counts one edge examination. Saturates for the same reason as
    /// [`Self::count_provider_examination`].
    fn count_edge_examination(&self) {
        self.edge_examinations
            .set(self.edge_examinations.get().saturating_add(1));
    }
}

/// The work a single resolution actually consumed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkCounters {
    /// Node-identifier yields plus neighbour-list requests. See the module documentation.
    pub provider_examinations: u32,
    /// Neighbour yields.
    pub edge_examinations: u32,
}

impl WorkCounters {
    /// The arithmetic sum of the two counters. Nothing separate is counted here.
    #[must_use]
    pub const fn total_work_units(self) -> u32 {
        self.provider_examinations
            .saturating_add(self.edge_examinations)
    }
}

/// The work a graph of a given declared size is permitted to consume.
///
/// The allowance is a constant multiple of the **accepted** size, which is what makes budget
/// exhaustion an internal error rather than an author-facing one (contract C-G9): every graph
/// inside the ceilings — cyclic or acyclic — reaches its verdict inside the budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Allowances {
    /// At most two provider examinations per accepted provider.
    pub provider_examinations: u32,
    /// At most two edge examinations per accepted edge.
    pub edge_examinations: u32,
    /// The sum of the two above.
    pub total_work_units: u32,
}

impl Allowances {
    /// Derives the allowances for a graph of the given declared size.
    #[must_use]
    pub const fn for_graph(providers: u32, edges: u32) -> Self {
        let provider_examinations = providers.saturating_mul(2);
        let edge_examinations = edges.saturating_mul(2);
        Self {
            provider_examinations,
            edge_examinations,
            total_work_units: provider_examinations.saturating_add(edge_examinations),
        }
    }
}

/// Which axis of the budget was exceeded, and by how much.
///
/// Contract C-G9: this is an **internal** condition, distinct from every author-facing diagnostic.
/// If an author ever sees it, the kernel is wrong, not their graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetExhausted {
    /// The axis that was exceeded.
    pub axis: BudgetAxis,
    /// What the traversal actually consumed on that axis.
    pub observed: u32,
    /// What it was allowed to consume.
    pub allowed: u32,
}

/// The axis of the work budget a [`BudgetExhausted`] refers to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetAxis {
    /// Provider examinations.
    ProviderExaminations,
    /// Edge examinations.
    EdgeExaminations,
    /// The sum of both.
    TotalWorkUnits,
}

/// The outcome of one resolution pass.
#[derive(Clone, Debug)]
pub struct Resolution {
    components: Vec<Component>,
    counters: WorkCounters,
}

/// One strongly connected component of the provider graph.
#[derive(Clone, Debug)]
pub struct Component {
    members: Vec<ProviderIx>,
    is_cycle: bool,
}

impl Component {
    /// Every provider in this component.
    ///
    /// When [`Self::is_cycle`] holds, this is the **complete** member list the diagnostic needs —
    /// not a representative. That completeness is the reason `tarjan_scc` was chosen over
    /// `toposort`, whose cycle type names a single node.
    #[must_use]
    pub fn members(&self) -> &[ProviderIx] {
        &self.members
    }

    /// Whether this component is a dependency cycle: more than one member, or a single member
    /// that depends on itself (contract C-G8).
    #[must_use]
    pub const fn is_cycle(&self) -> bool {
        self.is_cycle
    }
}

impl Resolution {
    /// The components, in postorder — reverse topological order.
    #[must_use]
    pub fn components(&self) -> &[Component] {
        &self.components
    }

    /// The counters observed during this resolution.
    #[must_use]
    pub const fn counters(&self) -> WorkCounters {
        self.counters
    }

    /// Every component that is a dependency cycle.
    pub fn cycles(&self) -> impl Iterator<Item = &Component> {
        self.components.iter().filter(|c| c.is_cycle())
    }

    /// Whether the graph contains any dependency cycle.
    #[must_use]
    pub fn has_cycle(&self) -> bool {
        self.components.iter().any(Component::is_cycle)
    }

    /// The initialisation order: dependencies before dependents.
    ///
    /// This is the flattened component order with **no reversal and no second traversal**, which
    /// only works because edges are directed dependent → dependency (contract C-G5).
    ///
    /// Meaningful only when [`Self::has_cycle`] is `false`. A cyclic graph has no initialisation
    /// order, and callers are expected to check for cycles first rather than consume this.
    #[must_use]
    pub fn initialisation_order(&self) -> Vec<ProviderIx> {
        self.components
            .iter()
            .flat_map(|component| component.members.iter().copied())
            .collect()
    }

    /// Checks the observed counters against the allowances for a graph of the given size.
    ///
    /// # Errors
    ///
    /// Returns [`BudgetExhausted`] naming the first axis exceeded. Axes are checked in the order
    /// provider, edge, total, so the report names the most specific cause rather than the
    /// aggregate that merely reflects it.
    pub fn check_budget(&self, providers: u32, edges: u32) -> Result<(), BudgetExhausted> {
        let allowances = Allowances::for_graph(providers, edges);
        let observed = self.counters;

        if observed.provider_examinations > allowances.provider_examinations {
            return Err(BudgetExhausted {
                axis: BudgetAxis::ProviderExaminations,
                observed: observed.provider_examinations,
                allowed: allowances.provider_examinations,
            });
        }
        if observed.edge_examinations > allowances.edge_examinations {
            return Err(BudgetExhausted {
                axis: BudgetAxis::EdgeExaminations,
                observed: observed.edge_examinations,
                allowed: allowances.edge_examinations,
            });
        }
        if observed.total_work_units() > allowances.total_work_units {
            return Err(BudgetExhausted {
                axis: BudgetAxis::TotalWorkUnits,
                observed: observed.total_work_units(),
                allowed: allowances.total_work_units,
            });
        }
        Ok(())
    }
}

// --- The petgraph adapter -------------------------------------------------------------------
//
// These four impls are the entire surface `tarjan_scc` sees. Every counted event happens here,
// which is what makes the counters observations rather than estimates (contract C-G6).

impl GraphBase for ResolverGraph {
    type EdgeId = EdgeIx;
    type NodeId = ProviderIx;
}

impl NodeIndexable for ResolverGraph {
    fn node_bound(&self) -> usize {
        self.offsets.len() - 1
    }

    fn to_index(&self, a: Self::NodeId) -> usize {
        a.get() as usize
    }

    fn from_index(&self, i: usize) -> Self::NodeId {
        ProviderIx::new(u32::try_from(i).expect("node index exceeds u32"))
    }
}

impl<'g> IntoNodeIdentifiers for &'g ResolverGraph {
    type NodeIdentifiers = NodeIdentifiers<'g>;

    fn node_identifiers(self) -> Self::NodeIdentifiers {
        NodeIdentifiers {
            graph: self,
            next: 0,
            end: self.provider_count(),
        }
    }
}

impl<'g> IntoNeighbors for &'g ResolverGraph {
    type Neighbors = Neighbors<'g>;

    fn neighbors(self, a: Self::NodeId) -> Self::Neighbors {
        // One provider examination per neighbour-list request. `tarjan_scc` requests a provider's
        // neighbours exactly once — `visit` is guarded so each node is visited at most once — so
        // this is `1 x providers`, the second of the two provider-examination sources.
        self.count_provider_examination();

        let start = self.offsets[a.get() as usize];
        let end = self.offsets[a.get() as usize + 1];
        Neighbors {
            graph: self,
            cursor: start,
            end,
        }
    }
}

/// Yields every provider in registration order, counting one provider examination per yield.
#[derive(Debug)]
pub struct NodeIdentifiers<'g> {
    graph: &'g ResolverGraph,
    next: u32,
    end: u32,
}

impl Iterator for NodeIdentifiers<'_> {
    type Item = ProviderIx;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            // Exhaustion is not an examination. The `for` loop that drives this iterator calls
            // `next` one extra time to learn it is done, and counting that would add a phantom
            // unit to every traversal.
            return None;
        }
        let index = self.next;
        self.next += 1;
        self.graph.count_provider_examination();
        Some(ProviderIx::new(index))
    }
}

/// Yields one provider's declared dependencies in declaration order, counting one edge
/// examination per yield.
#[derive(Debug)]
pub struct Neighbors<'g> {
    graph: &'g ResolverGraph,
    cursor: u32,
    end: u32,
}

impl Iterator for Neighbors<'_> {
    type Item = ProviderIx;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.end {
            return None;
        }
        let target = self.graph.targets[self.cursor as usize];
        self.cursor += 1;
        self.graph.count_edge_examination();
        Some(ProviderIx::new(target))
    }
}
