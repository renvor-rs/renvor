//! T084 and T085 — liveness and readiness disagree (SC-008, FR-026, FR-027), and a panicking
//! contributor is caught, identified, and treated as not-ready (FR-028).
//!
//! # The disagreement is the requirement
//!
//! It would be easy to satisfy FR-026 on paper: expose two methods, derive one from the other, and
//! never write a test that could tell. So the tests here **drive the application into a state where
//! the two answers differ** and assert both — which is the only form of the claim that can fail.
//!
//! The realistic case is `Drain`: an application finishing outstanding work is **alive** (do not
//! restart it) and **not ready** (send it nothing new). Getting that wrong in either direction is a
//! production incident, not a style problem — a drain reported as not-alive invites a restart in
//! the middle of the shutdown it is trying to complete.

mod support;

use std::sync::Arc;

use renvor_core::provider::ProviderFuture;
use renvor_core::{
    ApplicationBuilder, CapabilityId, InitContext, Liveness, Provider, ProviderId, Readiness,
    ReadinessContributor,
};

/// A contributor whose answer the test controls.
struct Fixed {
    name: &'static str,
    readiness: Readiness,
}

impl ReadinessContributor for Fixed {
    fn name(&self) -> &str {
        self.name
    }
    fn readiness(&self) -> Readiness {
        self.readiness
    }
}

/// A contributor that panics instead of answering.
struct Panicking;

impl ReadinessContributor for Panicking {
    fn name(&self) -> &str {
        "broken-check"
    }
    fn readiness(&self) -> Readiness {
        panic!("this contributor is deliberately broken");
    }
}

/// A provider that registers a readiness contributor during Boot, as a real one would.
struct Contributing {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    contributor: fn() -> Arc<dyn ReadinessContributor>,
}

impl Provider for Contributing {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }
    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            context.register_readiness((self.contributor)());
            Ok(())
        })
    }
}

fn contributing(
    name: &str,
    contributor: fn() -> Arc<dyn ReadinessContributor>,
) -> Box<dyn Provider> {
    Box::new(Contributing {
        id: ProviderId::new(name),
        provides: vec![CapabilityId::new(name)],
        contributor,
    })
}

fn builder() -> ApplicationBuilder {
    ApplicationBuilder::new().with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(
        vec![13; 32],
    )))
}

#[tokio::test]
async fn a_booted_application_is_alive_and_ready() {
    let application = builder()
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    assert_eq!(application.health().liveness(), Liveness::Alive);
    assert_eq!(application.health().readiness().readiness, Readiness::Ready);
}

#[tokio::test(start_paused = true)]
async fn draining_leaves_the_application_alive_and_makes_it_not_ready() {
    // SC-008 and FR-027, the case the requirement exists for. Both answers are asserted, because
    // asserting only the readiness half would pass on an implementation that reported not-alive.
    let mut application = builder()
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let health = application.health().clone();
    assert_eq!(health.readiness().readiness, Readiness::Ready, "before");

    application.shutdown().await;

    assert_eq!(
        health.readiness().readiness,
        Readiness::NotReady,
        "a draining application must not receive new work"
    );
    assert!(health.readiness().draining, "and says why");
    assert_eq!(
        health.liveness(),
        Liveness::Alive,
        "but must not invite a restart in the middle of its own shutdown"
    );
}

#[tokio::test]
async fn a_provider_registers_its_own_readiness_contribution() {
    // FR-028: the provider that owns a resource is what knows whether it is usable, so readiness
    // is contributed from there rather than from a central list somebody keeps in step by hand.
    let application = builder()
        .with_provider(contributing("db", || {
            Arc::new(Fixed {
                name: "db-pool",
                readiness: Readiness::NotReady,
            })
        }))
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let report = application.health().readiness();
    assert_eq!(report.readiness, Readiness::NotReady);
    assert_eq!(report.contributors.len(), 1);
    assert_eq!(report.blocking()[0].name, "db-pool", "identified by name");

    // And liveness is untouched by an unready dependency: a pool that is warming up is not a
    // reason to restart the process.
    assert_eq!(application.health().liveness(), Liveness::Alive);
}

#[tokio::test]
async fn liveness_and_readiness_disagree_in_the_other_direction_too() {
    // Deriving readiness from liveness would still allow the alive-but-not-ready case above. This
    // is the direction that catches it: dead, and nothing in the readiness path notices.
    let application = builder()
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    application.health().set_liveness(Liveness::Dead);

    assert_eq!(application.health().liveness(), Liveness::Dead);
    assert_eq!(
        application.health().readiness().readiness,
        Readiness::Ready,
        "readiness must not read liveness"
    );
}

#[tokio::test]
async fn a_panicking_contributor_is_caught_identified_and_does_not_take_the_process_down() {
    // T085. Reaching the assertions at all proves the panic did not escape — an escaping panic
    // would fail this test by aborting it.
    let application = builder()
        .with_provider(contributing("healthy", || {
            Arc::new(Fixed {
                name: "healthy-check",
                readiness: Readiness::Ready,
            })
        }))
        .with_provider(contributing("broken", || Arc::new(Panicking)))
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let report = application.health().readiness();

    assert_eq!(report.readiness, Readiness::NotReady);
    assert_eq!(
        report.contributors.len(),
        2,
        "the healthy one was still asked"
    );

    let panicking = report.panicking();
    assert_eq!(panicking.len(), 1);
    assert_eq!(panicking[0].name, "broken-check");
    assert_eq!(panicking[0].readiness, Readiness::NotReady);

    // A broken check and a working negative answer must look different to whoever reads this.
    let healthy: Vec<&str> = report
        .contributors
        .iter()
        .filter(|verdict| !verdict.panicked())
        .map(|verdict| verdict.name.as_str())
        .collect();
    assert_eq!(healthy, vec!["healthy-check"]);

    // The application is still alive: one broken readiness check is not a reason to restart.
    assert_eq!(application.health().liveness(), Liveness::Alive);
}

#[tokio::test]
async fn a_healthy_application_with_contributors_reports_ready() {
    // POSITIVE CONTROL for every not-ready assertion above: readiness discriminates rather than
    // being negative whenever any contributor exists at all.
    let application = builder()
        .with_provider(contributing("db", || {
            Arc::new(Fixed {
                name: "db-pool",
                readiness: Readiness::Ready,
            })
        }))
        .with_provider(contributing("cache", || {
            Arc::new(Fixed {
                name: "cache-pool",
                readiness: Readiness::Ready,
            })
        }))
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let report = application.health().readiness();
    assert_eq!(report.readiness, Readiness::Ready);
    assert_eq!(report.contributors.len(), 2);
    assert!(report.blocking().is_empty());
    assert!(report.panicking().is_empty());
    assert!(!report.draining);
}

/// W-005 security finding 5.1 — a hung contributor leaked one thread per probe, without limit.
///
/// A readiness probe runs on a timer somebody else controls, so "one leaked thread per probe" is a
/// resource-exhaustion vector that grows for as long as the process is monitored. The leak itself
/// cannot be removed — no Rust API interrupts a blocked thread — so the fix bounds it.
#[tokio::test]
async fn a_permanently_hung_contributor_stops_consuming_threads_at_the_ceiling() {
    use renvor_core::ContributorFault;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SPAWNED: AtomicUsize = AtomicUsize::new(0);

    /// Never returns. Counts how many times it was actually entered.
    struct Hung;

    impl ReadinessContributor for Hung {
        fn name(&self) -> &str {
            "hung-check"
        }
        fn readiness(&self) -> Readiness {
            SPAWNED.fetch_add(1, Ordering::Relaxed);
            loop {
                std::thread::park();
            }
        }
    }

    let application = builder()
        .with_provider(contributing("hung", || Arc::new(Hung)))
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let health = application.health().clone();
    health.set_readiness_deadline(std::time::Duration::from_millis(5));

    // Probe far more times than the ceiling allows.
    let probes = renvor_core::health::contributor::MAX_OUTSTANDING_PROBES * 4;
    let mut refused = 0_usize;
    for _ in 0..probes {
        let report = health.readiness();
        let verdict = &report.contributors[0];
        assert_eq!(
            report.readiness,
            Readiness::NotReady,
            "a hung check is not ready"
        );
        if verdict.fault == ContributorFault::NotAsked {
            refused += 1;
        }
    }

    // The ceiling held: threads stopped being created well before the probe count.
    let entered = SPAWNED.load(Ordering::Relaxed);
    assert!(
        entered <= renvor_core::health::contributor::MAX_OUTSTANDING_PROBES,
        "{entered} workers were entered for {probes} probes; the ceiling did not bound the leak"
    );
    assert!(
        refused > 0,
        "no probe was refused across {probes} attempts, so the ceiling never engaged"
    );

    // POSITIVE CONTROL: refusal is reported as a FAULT, not as a contributor that said no. An
    // operator must be able to tell "we stopped asking" from "the check answered not-ready".
    assert!(
        health.readiness().contributors[0].fault.is_fault(),
        "a refused probe must be reported as a fault"
    );
}
