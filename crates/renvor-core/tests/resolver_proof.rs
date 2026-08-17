//! The provider-resolver feasibility and counter proof (tasks T022–T024).
//!
//! # What this file is, and what it is not
//!
//! Revision 1 of the provider-graph design specified Kahn's algorithm for ordering plus a second
//! Tarjan pass for cycle reporting. Kahn alone examines each node twice and each edge twice —
//! **the entire allowance** — so the second pass broke the budget. The design was infeasible
//! against its own numbers. Revision 2 replaced it with a single `tarjan_scc` pass, and **this
//! file is the evidence that revision 2's numbers hold**, not a restatement of the claim.
//!
//! Every assertion here is a *counted observation*. None depends on elapsed wall-clock time
//! (SC-021, contract C-G3) — a timing bound would make the budget a property of the host, flaky
//! under CI load and silently passing on fast hardware. That property is itself asserted
//! mechanically, at the bottom of this file, rather than merely promised here.
//!
//! # Positive controls
//!
//! An assertion that cannot fail proves nothing. Several tests here would pass vacuously against
//! a broken implementation — an ordering predicate that always returns `true`, a budget check that
//! never fires, a cycle detector that finds cycles everywhere. Each of those carries a paired
//! control that drives the same code path to the opposite verdict. Where a control exists it is
//! named in the test.

use renvor_core::provider::graph::{
    Allowances, BudgetAxis, GraphSizeError, MAX_EDGES, MAX_PROVIDERS, ProviderIx, ResolverGraph,
    ResolverGraphBuilder,
};

// --- Fixtures -------------------------------------------------------------------------------

/// Builds a graph from per-provider dependency lists, in registration order.
///
/// `dependencies[i]` is provider `i`'s declared dependency list, in declaration order. Edges run
/// **dependent → dependency** (contract C-G5).
fn graph_from(dependencies: &[Vec<u32>]) -> ResolverGraph {
    let edge_count = dependencies.iter().map(Vec::len).sum();
    let mut builder = ResolverGraphBuilder::with_capacity(dependencies.len(), edge_count);
    for provider in dependencies {
        builder.push_provider(provider.iter().copied().map(ProviderIx::new));
    }
    builder
        .build()
        .expect("fixture graph is within the ceilings")
}

/// Flattens per-provider dependency lists into `(dependent, dependency)` pairs.
fn edge_list(dependencies: &[Vec<u32>]) -> Vec<(u32, u32)> {
    dependencies
        .iter()
        .enumerate()
        .flat_map(|(dependent, targets)| {
            let dependent = u32::try_from(dependent).expect("fixture is small");
            targets.iter().map(move |&target| (dependent, target))
        })
        .collect()
}

/// A diamond: `app` depends on `http` and `db`, both of which depend on `config`.
///
/// The shape matters — it has a provider reachable by two distinct paths, so an ordering that
/// merely happened to be sorted by index would not be distinguishable from a correct one.
fn diamond() -> Vec<Vec<u32>> {
    vec![
        vec![1, 2], // 0 app     -> http, db
        vec![3],    // 1 http    -> config
        vec![3],    // 2 db      -> config
        vec![],     // 3 config
    ]
}

/// A three-provider cycle, plus a dependent outside the cycle and an unrelated provider.
///
/// The two extra providers are the point: they prove the cycle report names the cycle's members
/// and *only* the cycle's members.
fn cycle_with_bystanders() -> Vec<Vec<u32>> {
    vec![
        vec![1], // 0 -> 1  \
        vec![2], // 1 -> 2   } cycle {0, 1, 2}
        vec![0], // 2 -> 0  /
        vec![0], // 3 -> 0   depends into the cycle, but is not in it
        vec![],  // 4        unrelated
    ]
}

/// A graph at **both** ceilings simultaneously: 1024 providers and exactly 8192 edges, acyclic.
///
/// Contract C-G2 requires this to succeed. A ceiling that cannot actually be reached is a lower
/// ceiling wearing a misleading number.
///
/// Every edge runs from a lower index to a higher one, which makes acyclicity structural rather
/// than something the fixture has to be trusted about.
fn graph_at_both_ceilings() -> Vec<Vec<u32>> {
    let n = MAX_PROVIDERS;
    let mut dependencies: Vec<Vec<u32>> = (0..n)
        .map(|i| (1..=8).map(|d| i + d).filter(|&t| t < n).collect())
        .collect();

    // The dense band above yields 8156 edges. Top up to exactly MAX_EDGES with a ninth dependency
    // on the first 36 providers, keeping every edge low-index -> high-index.
    let deficit = MAX_EDGES - 8156;
    for (i, provider) in dependencies.iter_mut().enumerate().take(deficit as usize) {
        provider.push(u32::try_from(i).expect("index is small") + 9);
    }

    dependencies
}

/// A linear chain of `n` providers: provider `i` depends on provider `i + 1`.
///
/// This is the deepest graph a given provider count admits, and therefore the deepest recursion
/// `tarjan_scc` can be driven into.
fn linear_chain(n: u32) -> Vec<Vec<u32>> {
    (0..n)
        .map(|i| if i + 1 < n { vec![i + 1] } else { Vec::new() })
        .collect()
}

/// Whether every dependency appears before every provider that depends on it.
///
/// Returns `false` on the first violation. This predicate is exercised in **both** directions by
/// `the_ordering_predicate_can_reject_a_wrong_order`, so a `true` result from it is evidence
/// rather than a tautology.
fn dependencies_precede_dependents(order: &[ProviderIx], edges: &[(u32, u32)]) -> bool {
    let mut position = vec![usize::MAX; order.len()];
    for (slot, provider) in order.iter().enumerate() {
        position[provider.get() as usize] = slot;
    }
    edges.iter().all(|&(dependent, dependency)| {
        position[dependency as usize] < position[dependent as usize]
    })
}

// --- T022: one pass yields both the order and complete cycle membership ----------------------

#[test]
fn a_single_pass_orders_dependencies_before_dependents() {
    let dependencies = diamond();
    let graph = graph_from(&dependencies);
    let edges = edge_list(&dependencies);

    let resolution = graph.resolve();

    assert!(
        !resolution.has_cycle(),
        "the diamond is acyclic; reporting a cycle would mean the detector fires on shared \
         dependencies, which is the most likely way for it to be wrong"
    );

    let order = resolution.initialisation_order();
    assert_eq!(order.len(), dependencies.len(), "every provider is ordered");
    assert!(
        dependencies_precede_dependents(&order, &edges),
        "initialisation order {order:?} places a dependent before its dependency"
    );

    // `config` is reachable from `app` by two distinct paths, so it must be first regardless of
    // which path the traversal took. This is the assertion a merely index-sorted order fails.
    assert_eq!(
        order[0],
        ProviderIx::new(3),
        "the shared dependency initialises first"
    );
    assert_eq!(
        order[3],
        ProviderIx::new(0),
        "the provider nothing depends on initialises last"
    );
}

#[test]
fn the_ordering_predicate_can_reject_a_wrong_order() {
    // POSITIVE CONTROL for `a_single_pass_orders_dependencies_before_dependents`. Without this,
    // a predicate that returned `true` unconditionally would pass that test.
    let dependencies = diamond();
    let edges = edge_list(&dependencies);
    let graph = graph_from(&dependencies);

    let correct = graph.resolve().initialisation_order();
    assert!(dependencies_precede_dependents(&correct, &edges));

    let mut reversed = correct.clone();
    reversed.reverse();
    assert!(
        !dependencies_precede_dependents(&reversed, &edges),
        "the predicate accepted a reversed order, so it cannot distinguish right from wrong"
    );
}

#[test]
fn a_cycle_component_names_every_member_and_no_bystander() {
    let dependencies = cycle_with_bystanders();
    let graph = graph_from(&dependencies);

    let resolution = graph.resolve();

    let cycles: Vec<_> = resolution.cycles().collect();
    assert_eq!(cycles.len(), 1, "exactly one cycle exists in this fixture");

    let mut members: Vec<u32> = cycles[0].members().iter().map(|p| p.get()).collect();
    members.sort_unstable();

    // The whole reason `tarjan_scc` was chosen over `toposort` is that this is the COMPLETE member
    // list. `toposort`'s cycle type names one node, which leaves the author bisecting.
    assert_eq!(
        members,
        vec![0, 1, 2],
        "the cycle report must name all three members"
    );
    assert!(
        !members.contains(&3),
        "provider 3 depends INTO the cycle but is not part of it; including it would blame an \
         innocent provider"
    );
    assert!(!members.contains(&4), "provider 4 is unrelated");
}

#[test]
fn the_cycle_detector_reports_nothing_on_an_acyclic_graph() {
    // POSITIVE CONTROL for `a_cycle_component_names_every_member_and_no_bystander`: a detector
    // that reported every component as a cycle would pass that test too.
    let resolution = graph_from(&diamond()).resolve();
    assert_eq!(
        resolution.cycles().count(),
        0,
        "the diamond has no cycle, so a report here means the detector cannot say no"
    );
}

#[test]
fn a_self_dependency_is_reported_as_a_cycle() {
    // A one-member component is the normal case; a one-member component whose provider depends on
    // itself is a cycle. `tarjan_scc` returns both as a single-element component and cannot tell
    // them apart, so this distinction is Renvor's to make (contract C-G8).
    let with_self_edge = graph_from(&[vec![0]]);
    let resolution = with_self_edge.resolve();
    assert_eq!(resolution.cycles().count(), 1);
    assert_eq!(
        resolution
            .cycles()
            .next()
            .expect("just asserted one cycle")
            .members(),
        &[ProviderIx::new(0)]
    );

    // POSITIVE CONTROL: the same one-provider shape without the self edge is not a cycle.
    let without_self_edge = graph_from(&[Vec::new()]);
    assert_eq!(
        without_self_edge.resolve().cycles().count(),
        0,
        "a lone provider with no dependencies was reported as a cycle"
    );
}

#[test]
fn the_order_and_the_cycle_verdict_come_from_the_same_single_pass() {
    // This is the single-pass proof, and the counters are what make it a proof rather than a
    // claim. A design that ordered in one traversal and detected cycles in another would show at
    // least 3x providers or 2x edges here. Exactly 2x providers and 1x edges is only achievable
    // if both results came out of one pass.
    let dependencies = cycle_with_bystanders();
    let graph = graph_from(&dependencies);
    let providers = graph.provider_count();
    let edges = graph.edge_count();

    let resolution = graph.resolve();

    // Both results consumed from the one resolution.
    let _order = resolution.initialisation_order();
    let cycle_count = resolution.cycles().count();
    assert_eq!(cycle_count, 1);

    let counters = resolution.counters();
    assert_eq!(
        counters.provider_examinations,
        2 * providers,
        "a second traversal over providers would push this above 2x"
    );
    assert_eq!(
        counters.edge_examinations, edges,
        "a second traversal over edges would push this above 1x"
    );
}

// --- T023: the counters, at the ceilings and across sizes ------------------------------------

#[test]
fn at_both_ceilings_the_counters_are_2048_8192_10240_within_2048_16384_18432() {
    let dependencies = graph_at_both_ceilings();
    let graph = graph_from(&dependencies);

    assert_eq!(
        graph.provider_count(),
        MAX_PROVIDERS,
        "the fixture must actually sit at the provider ceiling"
    );
    assert_eq!(
        graph.edge_count(),
        MAX_EDGES,
        "the fixture must actually sit at the edge ceiling"
    );

    let resolution = graph.resolve();

    // Contract C-G2: a valid acyclic graph at BOTH ceilings simultaneously must succeed.
    assert!(
        !resolution.has_cycle(),
        "the maximum-size acyclic graph was rejected as cyclic"
    );
    assert!(dependencies_precede_dependents(
        &resolution.initialisation_order(),
        &edge_list(&dependencies)
    ));

    let counters = resolution.counters();
    assert_eq!(counters.provider_examinations, 2048);
    assert_eq!(counters.edge_examinations, 8192);
    assert_eq!(counters.total_work_units(), 10240);

    let allowances = Allowances::for_graph(MAX_PROVIDERS, MAX_EDGES);
    assert_eq!(allowances.provider_examinations, 2048);
    assert_eq!(allowances.edge_examinations, 16384);
    assert_eq!(allowances.total_work_units, 18432);

    assert_eq!(
        resolution.check_budget(MAX_PROVIDERS, MAX_EDGES),
        Ok(()),
        "the worst accepted graph must resolve inside the budget, or the budget is not a budget"
    );

    // The edge axis sits at exactly half its allowance. Recording the headroom keeps a future
    // change that silently doubles edge traversal from passing unnoticed.
    assert_eq!(counters.edge_examinations * 2, allowances.edge_examinations);
}

#[test]
fn counters_scale_within_two_times_providers_and_two_times_edges() {
    // Four sizes, spanning three orders of magnitude. One data point cannot distinguish "2 per
    // provider" from "a constant that happens to match at this size".
    for providers in [4_u32, 64, 256, MAX_PROVIDERS] {
        let dependencies: Vec<Vec<u32>> = (0..providers)
            .map(|i| (1..=3).map(|d| i + d).filter(|&t| t < providers).collect())
            .collect();
        let graph = graph_from(&dependencies);
        let edges = graph.edge_count();

        let resolution = graph.resolve();
        let counters = resolution.counters();

        assert_eq!(
            counters.provider_examinations,
            2 * providers,
            "provider examinations must be exactly 2 per provider at size {providers}"
        );
        assert_eq!(
            counters.edge_examinations, edges,
            "edge examinations must be exactly 1 per edge at size {providers}"
        );
        assert!(counters.provider_examinations <= 2 * providers);
        assert!(counters.edge_examinations <= 2 * edges);
        assert_eq!(resolution.check_budget(providers, edges), Ok(()));
    }
}

#[test]
fn the_budget_check_reports_exhaustion_when_the_allowance_is_exceeded() {
    // POSITIVE CONTROL for every `check_budget(..) == Ok(())` above. A check that returned `Ok`
    // unconditionally would satisfy all of them, and the budget would be decorative.
    let graph = graph_from(&diamond());
    let resolution = graph.resolve();

    let observed = resolution.counters();
    assert!(
        observed.provider_examinations > 0,
        "there is work to exceed"
    );

    // Same real resolution, judged against the allowance of an empty graph.
    let verdict = resolution.check_budget(0, 0);
    assert_eq!(
        verdict,
        Err(renvor_core::provider::graph::BudgetExhausted {
            axis: BudgetAxis::ProviderExaminations,
            observed: observed.provider_examinations,
            allowed: 0,
        }),
        "the budget check did not fire on a traversal that plainly exceeded its allowance"
    );
}

#[test]
fn the_ceilings_reject_on_declared_counts_alone() {
    // Contract C-G2: rejection happens before traversal, on the declared counts. Discovering an
    // oversized graph by running out of traversal budget would report a kernel defect (C-G9)
    // where the author has an oversized graph.
    let mut builder = ResolverGraphBuilder::new();
    for _ in 0..=MAX_PROVIDERS {
        builder.push_provider(std::iter::empty());
    }
    assert_eq!(
        builder
            .build()
            .expect_err("a graph one provider over the ceiling must be rejected"),
        GraphSizeError::TooManyProviders {
            declared: MAX_PROVIDERS + 1,
            ceiling: MAX_PROVIDERS,
        }
    );

    // POSITIVE CONTROL: one fewer provider is accepted, so the rejection is the ceiling talking
    // and not a builder that refuses large graphs generally.
    let mut builder = ResolverGraphBuilder::new();
    for _ in 0..MAX_PROVIDERS {
        builder.push_provider(std::iter::empty());
    }
    assert_eq!(
        builder
            .build()
            .expect("a graph exactly at the ceiling is valid")
            .provider_count(),
        MAX_PROVIDERS
    );
}

#[test]
fn a_dependency_on_an_unregistered_provider_names_both_endpoints() {
    // Contract C-G11. Naming only the dependent, or only the missing capability, leaves the
    // author bisecting.
    let mut builder = ResolverGraphBuilder::new();
    builder.push_provider([ProviderIx::new(7)]);
    builder.push_provider(std::iter::empty());

    assert_eq!(
        builder
            .build()
            .expect_err("a dependency on provider 7 in a two-provider graph must be rejected"),
        GraphSizeError::UnknownDependency {
            dependent: ProviderIx::new(0),
            named: 7,
            provider_count: 2,
        }
    );
}

// --- T024: recursion depth on a pinned Tokio worker stack ------------------------------------

#[test]
fn a_1024_node_chain_resolves_on_a_tokio_worker_with_a_pinned_2_mib_stack() {
    // `tarjan_scc` is recursive by its own documentation — "This implementation is recursive and
    // does one pass over the nodes" — and `visit` calls itself once per edge followed. Recursion
    // depth therefore equals the longest dependency chain, which the 1024-provider ceiling
    // bounds. This test drives exactly that maximum.
    //
    // The stack size is PINNED rather than inherited. Tokio's documentation states its 2 MiB
    // default is "subject to change in the future": an inherited default that grew would silently
    // relax this test, and one that shrank would fail it for a reason unrelated to Renvor. A stack
    // test that does not pin its stack measures the host, not the code.
    const STACK_BYTES: usize = 2 * 1024 * 1024;
    const WORKER_NAME: &str = "renvor-resolver-proof-worker";

    // The worker name is Renvor's, not Tokio's. An earlier draft asserted on Tokio's own worker
    // name and failed, because 1.53 calls it `tokio-rt-worker` rather than the assumed
    // `tokio-runtime-worker`. Depending on an undocumented internal string would make this test
    // break for a reason that has nothing to do with the resolver — the same failure mode the
    // pinned stack size exists to avoid.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_stack_size(STACK_BYTES)
        .thread_name(WORKER_NAME)
        .build()
        .expect("runtime builds");

    let test_thread = std::thread::current().id();

    let order = runtime.block_on(async move {
        // `spawn` puts the work on a WORKER thread. Running it in `block_on` directly would
        // execute on the main thread, whose stack is larger and is not what the contract requires
        // be exercised.
        tokio::spawn(async move {
            // CONTROL: prove we are where we claim to be. Without this the test could silently
            // degrade to a main-thread run and still pass, measuring nothing. The thread-id
            // comparison is the airtight half — it cannot be satisfied by anything running on the
            // test thread, whatever it is named.
            assert_ne!(
                std::thread::current().id(),
                test_thread,
                "the resolution ran on the test thread, whose stack is not the pinned one"
            );
            let thread = std::thread::current();
            let name = thread.name().unwrap_or_default().to_owned();
            assert_eq!(
                name, WORKER_NAME,
                "expected the runtime worker whose stack was pinned to {STACK_BYTES} bytes"
            );

            let dependencies = linear_chain(MAX_PROVIDERS);
            let graph = graph_from(&dependencies);
            assert_eq!(graph.provider_count(), MAX_PROVIDERS);
            assert_eq!(graph.edge_count(), MAX_PROVIDERS - 1);

            let resolution = graph.resolve();
            assert!(!resolution.has_cycle(), "a chain has no cycle");

            // Every provider is its own component, so the recursion really did unwind 1024 frames
            // rather than collapsing the chain into one component.
            assert_eq!(resolution.components().len(), MAX_PROVIDERS as usize);
            assert!(
                resolution
                    .components()
                    .iter()
                    .all(|c| c.members().len() == 1)
            );

            let counters = resolution.counters();
            assert_eq!(counters.provider_examinations, 2 * MAX_PROVIDERS);
            assert_eq!(counters.edge_examinations, MAX_PROVIDERS - 1);
            assert_eq!(
                resolution.check_budget(MAX_PROVIDERS, MAX_PROVIDERS - 1),
                Ok(())
            );

            resolution.initialisation_order()
        })
        .await
        .expect("the resolution task neither panicked nor was cancelled")
    });

    // The order assertion is what makes the depth claim non-vacuous. A graph that was built wrong
    // — disconnected, or shallower than intended — would resolve without deep recursion and pass
    // a bare "it did not crash" check. Only a genuine 1024-link chain produces exactly this
    // sequence: the deepest dependency first, the outermost dependent last.
    assert_eq!(order.len(), MAX_PROVIDERS as usize);
    for (slot, provider) in order.iter().enumerate() {
        let expected = MAX_PROVIDERS - 1 - u32::try_from(slot).expect("slot is small");
        assert_eq!(
            provider.get(),
            expected,
            "chain order diverges at slot {slot}, so the traversal did not follow the full chain"
        );
    }
}

// --- SC-021: no assertion here depends on wall-clock time ------------------------------------

#[test]
fn no_assertion_in_this_file_depends_on_wall_clock_time() {
    // Contract C-G3 prohibits a timed bound: it would make the budget a property of the HOST
    // rather than of the graph — flaky under CI load, silently passing on fast hardware. SC-021
    // requires zero such assertions, and a promise in a doc comment is not zero.
    //
    // Each needle below must appear exactly ONCE in this file: here, in the needle list itself.
    // That makes the scan self-controlling — a count of 1 proves the scan reads the right file and
    // matches real text, and a count of 2 means a timing API entered the proof.
    let source = include_str!("resolver_proof.rs");
    for needle in ["Instant", "SystemTime", ".elapsed(", "sleep("] {
        let occurrences = source.matches(needle).count();
        assert_eq!(
            occurrences, 1,
            "{needle:?} appears {occurrences} times; expected exactly the one occurrence in this \
             needle list. 0 means the scan is broken and proves nothing; more than 1 means a \
             wall-clock dependency entered the resolver proof"
        );
    }
}
