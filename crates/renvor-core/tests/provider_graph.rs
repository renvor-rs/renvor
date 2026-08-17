//! T044 — cycles and missing dependencies (SC-005, FR-013, FR-014, C-G8, C-G11).
//!
//! Both refusals happen at `Register`, and **0** cases reach `Boot`. That is asserted here on the
//! phase log rather than inferred from "no provider recorded an init", because a provider that
//! silently did nothing would satisfy the weaker check.

mod support;

use renvor_core::{ErrorCategory, LifecyclePhase};
use support::{Journal, Scripted, builder};

#[test]
fn a_cycle_names_every_provider_in_it_and_never_reaches_boot() {
    // FR-013 / C-G8: the diagnostic names the whole strongly connected component, not one
    // representative. `tarjan_scc` was chosen over `toposort` precisely for this.
    let journal = Journal::new();
    let application = builder()
        .with_provider(
            Scripted::new(&journal, "alpha")
                .provides(&["a"])
                .needs(&["c"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "beta")
                .provides(&["b"])
                .needs(&["a"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "gamma")
                .provides(&["c"])
                .needs(&["b"])
                .boxed(),
        );
    let phases = application.phase_log();

    let error = application.build().expect_err("a cycle cannot be resolved");

    assert_eq!(error.category(), Some(ErrorCategory::DependencyCycle));
    let rendered = error.to_string();
    for provider in ["alpha", "beta", "gamma"] {
        assert!(
            rendered.contains(provider),
            "the cycle diagnostic omitted `{provider}`: {rendered}"
        );
    }

    // SC-005: 0 cases reach Boot.
    assert!(!phases.entries().contains(&LifecyclePhase::Boot));
    assert_eq!(
        phases.entries().last(),
        Some(&LifecyclePhase::Register),
        "the run must stop at Register"
    );
    assert!(journal.inits().is_empty(), "0 providers initialised");
}

#[test]
fn a_self_dependency_is_a_cycle() {
    // C-G8's second half: a single node with a self-edge is a cycle, not a trivial component.
    let journal = Journal::new();
    let error = builder()
        .with_provider(
            Scripted::new(&journal, "ouroboros")
                .provides(&["self"])
                .needs(&["self"])
                .boxed(),
        )
        .build()
        .expect_err("a provider that depends on itself cannot be ordered");

    assert_eq!(error.category(), Some(ErrorCategory::DependencyCycle));
    assert!(error.to_string().contains("ouroboros"), "{error}");
}

#[test]
fn an_acyclic_graph_of_the_same_shape_resolves() {
    // POSITIVE CONTROL for both tests above: breaking one edge of the three-provider cycle makes
    // it resolve, so cycle detection discriminates rather than rejecting every connected graph.
    let journal = Journal::new();
    let application = builder()
        .with_provider(Scripted::new(&journal, "alpha").provides(&["a"]).boxed())
        .with_provider(
            Scripted::new(&journal, "beta")
                .provides(&["b"])
                .needs(&["a"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "gamma")
                .provides(&["c"])
                .needs(&["b"])
                .boxed(),
        )
        .build()
        .expect("the acyclic graph resolves");

    let order: Vec<&str> = application
        .initialisation_order()
        .ids()
        .map(renvor_core::ProviderId::as_str)
        .collect();
    assert_eq!(order, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn a_missing_dependency_names_both_endpoints_and_never_reaches_boot() {
    // FR-014 / C-G11: naming only one leaves the author bisecting.
    let journal = Journal::new();
    let application = builder()
        .with_provider(
            Scripted::new(&journal, "http")
                .provides(&["http"])
                .needs(&["database"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "cache")
                .provides(&["cache"])
                .boxed(),
        );
    let phases = application.phase_log();

    let error = application.build().expect_err("nobody provides `database`");

    assert_eq!(error.category(), Some(ErrorCategory::DependencyMissing));
    let rendered = error.to_string();
    assert!(rendered.contains("http"), "dependent missing: {rendered}");
    assert!(
        rendered.contains("database"),
        "capability missing: {rendered}"
    );

    assert!(!phases.entries().contains(&LifecyclePhase::Boot));
    assert!(journal.inits().is_empty(), "0 providers initialised");
}

#[test]
fn two_providers_offering_the_same_capability_is_refused_rather_than_silently_resolved() {
    // Not covered by any earlier artifact — found by implementing the registry. Picking a winner
    // would be a silent fallback (FR-022), so the ambiguity is named instead.
    let journal = Journal::new();
    let error = builder()
        .with_provider(
            Scripted::new(&journal, "postgres")
                .provides(&["database"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "sqlite")
                .provides(&["database"])
                .boxed(),
        )
        .build()
        .expect_err("an ambiguous capability has no single answer");

    assert_eq!(error.category(), Some(ErrorCategory::CapabilityDuplicate));
    let rendered = error.to_string();
    for named in ["database", "postgres", "sqlite"] {
        assert!(rendered.contains(named), "`{named}` missing: {rendered}");
    }
}

#[test]
fn resolution_reports_graph_size_and_traversal_work_separately() {
    // SC-021 / C-G3: work is counted, never timed, and the two families of number are separately
    // assertable. A single conflated figure could not distinguish a large graph from a
    // misbehaving traversal.
    let journal = Journal::new();
    let application = builder()
        .with_provider(
            Scripted::new(&journal, "db")
                .provides(&["database"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "http")
                .provides(&["http"])
                .needs(&["database"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "api")
                .needs(&["http", "database"])
                .boxed(),
        )
        .build()
        .expect("resolves");

    let report = application.resolution_report();
    assert_eq!(report.provider_count, 3);
    assert_eq!(report.edge_count, 3);

    // Exactly 2 examinations per provider and 1 per edge, which is what the measured proof gate
    // recorded at maximum size. These are equalities, not bounds: a loosened `<=` would keep
    // passing if the traversal started doing twice the work.
    assert_eq!(report.providers_examined, 6);
    assert_eq!(report.edges_examined, 3);
    assert_eq!(report.work_units, 9);
    assert_eq!(
        report.work_units,
        report.providers_examined + report.edges_examined
    );
}

#[test]
fn an_application_with_no_providers_resolves_to_an_empty_order() {
    // The degenerate case, asserted rather than assumed: an empty graph is not a failure.
    let application = builder().build().expect("an empty application is valid");
    assert!(application.initialisation_order().is_empty());
    assert_eq!(application.resolution_report().provider_count, 0);
    assert_eq!(application.resolution_report().work_units, 0);
}
