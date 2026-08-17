//! Provider identity, declared capabilities, and the registration set.
//!
//! # Dependencies are declared, never inferred
//!
//! FR-012 requires providers to *declare* what they depend on. Nothing here inspects a provider's
//! behaviour to guess an ordering: a provider names the capabilities it [`Provider::provides`] and
//! the capabilities it [`Provider::dependencies`], and the resolver works from those two lists
//! alone. Inference would make the initialisation order depend on what a provider happened to
//! touch during a particular run, which is the opposite of the determinism C-G7 requires.
//!
//! # Why the async methods return a boxed future by hand
//!
//! [`Provider`] must be usable as `dyn Provider` — the registry holds a heterogeneous set — and a
//! trait with `async fn` is not dyn-compatible. The ecosystem answer is the `async-trait` macro,
//! which expands to exactly the `Pin<Box<dyn Future + Send>>` written out in [`ProviderFuture`].
//!
//! Renvor writes it out instead of taking the dependency. This is not a rejection of the crate:
//! it is the observation that the whole of its value here is the sugar, that `core::future` and
//! `core::pin` already provide the shape, and that a kernel which justifies each dependency
//! individually cannot justify a proc-macro whose output it can spell in one type alias. No
//! custom infrastructure is involved, so FR-035 does not apply — this is std, not a substitute
//! for a package.
//!
//! # Ceilings are checked on declared counts, before any traversal
//!
//! FR-039(a) and contract C-G2 require an oversized graph to be rejected at Register on what was
//! *declared*, not discovered by running out of traversal budget. [`ProviderRegistry::declared_size`]
//! is that check, and it runs before a graph is built.

use core::fmt;
use core::future::Future;
use core::pin::Pin;

use crate::cancel::CancelScope;
use crate::error::{BoxedCause, KernelError};
use crate::health::{HealthState, ReadinessContributor};
use crate::lifecycle::drain::{WorkGate, WorkPermit};
use crate::provider::graph::{MAX_EDGES, MAX_PROVIDERS};
use crate::state::TypedStateMap;

/// A provider's stable, human-readable name.
///
/// Appears **verbatim** in diagnostics, which is the entire reason it is a distinct type rather
/// than a bare `String`: a value that reaches an author's terminal deserves a name in the code
/// that says so.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProviderId(String);

impl ProviderId {
    /// Names a provider.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ProviderId {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for ProviderId {
    fn from(name: String) -> Self {
        Self(name)
    }
}

/// A capability one provider offers and others may depend on.
///
/// Deliberately **not** the same type as [`ProviderId`]. A dependency names a capability, not a
/// provider, so that the provider satisfying it can be swapped without editing every dependent.
/// Making them one type would let the two be confused at every call site, and the compiler would
/// not care.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Names a capability.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The name as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CapabilityId {
    fn from(name: &str) -> Self {
        Self::new(name)
    }
}

impl From<String> for CapabilityId {
    fn from(name: String) -> Self {
        Self(name)
    }
}

/// The future a provider returns from [`Provider::initialise`] and [`Provider::stop`].
///
/// See the module documentation for why this is written out rather than produced by a macro.
pub type ProviderFuture<'a> = Pin<Box<dyn Future<Output = Result<(), BoxedCause>> + Send + 'a>>;

/// What a provider receives while it initialises.
///
/// Carries the two things a provider legitimately needs from the kernel during Boot — somewhere to
/// register state, and a cancellation scope — and nothing else. It does **not** hand over the
/// application, the registry, or the phase, because a provider that can reach those can reorder
/// its own initialisation and defeat the guarantee the resolver just established.
#[derive(Debug)]
pub struct InitContext<'a> {
    provider: &'a ProviderId,
    state: &'a mut TypedStateMap,
    cancel: &'a CancelScope,
    work: &'a WorkGate,
    health: &'a HealthState,
}

impl<'a> InitContext<'a> {
    /// Creates a context for one provider's initialisation.
    #[must_use]
    pub fn new(
        provider: &'a ProviderId,
        state: &'a mut TypedStateMap,
        cancel: &'a CancelScope,
        work: &'a WorkGate,
        health: &'a HealthState,
    ) -> Self {
        Self {
            provider,
            state,
            cancel,
            work,
            health,
        }
    }

    /// The provider this context belongs to.
    #[must_use]
    pub const fn provider(&self) -> &ProviderId {
        self.provider
    }

    /// The cancellation scope for this provider's work.
    ///
    /// Already a child of the application's root scope, so work started under it stops when the
    /// application stops without the provider arranging anything.
    #[must_use]
    pub const fn cancel(&self) -> &CancelScope {
        self.cancel
    }

    /// Admits one unit of in-flight work, which the drain will wait for.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::ShuttingDown`] if shutdown has already begun (FR-006).
    pub fn begin_work(&self, operation: impl Into<String>) -> Result<WorkPermit, KernelError> {
        self.work.begin(operation)
    }

    /// The application's work gate, for a provider that keeps working after initialisation.
    #[must_use]
    pub const fn work(&self) -> &WorkGate {
        self.work
    }

    /// Registers this provider's opinion about whether the application should receive work.
    ///
    /// A provider that owns a connection pool is the thing that knows whether it is usable, so
    /// this is where readiness contributions belong rather than in a central registry somebody has
    /// to keep in step with the provider set (FR-028).
    pub fn register_readiness(&self, contributor: std::sync::Arc<dyn ReadinessContributor>) {
        self.health.register(contributor);
    }

    /// The application's health state.
    #[must_use]
    pub const fn health(&self) -> &HealthState {
        self.health
    }

    /// Registers a value in the application's typed state.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::StateDuplicate`] if a value of this type is already registered
    /// (FR-011).
    pub fn register_state<T: core::any::Any + Send + Sync>(
        &mut self,
        value: T,
    ) -> Result<(), KernelError> {
        self.state.insert(value)
    }

    /// Retrieves a value another provider registered earlier.
    ///
    /// Reaching a dependency's state is only sound because the resolver initialises dependencies
    /// first — this method is where that ordering guarantee is actually spent.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::StateMissing`] if nothing of this type is registered (FR-010).
    pub fn state<T: core::any::Any + Send + Sync>(&self) -> Result<&T, KernelError> {
        self.state.get::<T>()
    }
}

/// A unit of application capability with declared dependencies.
///
/// `Send + Sync` because a provider outlives the phase that built it and is reachable from tasks
/// on any runtime thread.
pub trait Provider: Send + Sync {
    /// This provider's stable name, as it will appear in diagnostics.
    fn id(&self) -> &ProviderId;

    /// The capabilities this provider offers.
    ///
    /// Empty by default: a leaf provider that nothing depends on is a normal thing to write, and
    /// making it say so explicitly would be ceremony.
    fn provides(&self) -> &[CapabilityId] {
        &[]
    }

    /// The capabilities this provider requires, in declaration order.
    ///
    /// Empty by default, for the same reason as [`Self::provides`].
    fn dependencies(&self) -> &[CapabilityId] {
        &[]
    }

    /// Brings this provider up.
    ///
    /// Called during Boot, after every capability in [`Self::dependencies`] has been initialised.
    ///
    /// # Errors
    ///
    /// Any failure aborts Boot. The kernel wraps it in [`KernelError::ProviderInit`], preserving
    /// this error as the cause rather than flattening it into a message (C-E2), and rolls back
    /// every provider already initialised.
    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a>;

    /// Takes this provider down.
    ///
    /// Called during Stop and during rollback, in reverse **actual initialisation** order. The
    /// default does nothing, because a provider that holds no resource has nothing to release and
    /// should not be made to say so.
    ///
    /// # Errors
    ///
    /// A failure here does **not** abort the remaining shutdown (C-L4, FR-005). Every failure is
    /// reported; none masks another.
    fn stop(&self) -> ProviderFuture<'_> {
        Box::pin(async { Ok(()) })
    }
}

impl fmt::Debug for dyn Provider {
    /// Renders **without calling a single author method**.
    ///
    /// # Why this prints nothing about the provider
    ///
    /// This impl used to call `id()`, `provides()`, and `dependencies()`. Every one of those is
    /// author code, and `fmt::Debug` is the one place in the kernel that cannot bound it: it has
    /// no deadline, no `catch_unwind` that could report a fault, and no way to signal a failure
    /// except by writing into the output it is producing. A provider whose `id()` blocked would
    /// hang whatever formatted it — including a log line on a shutdown path, which is precisely
    /// when an operator most needs output.
    ///
    /// Bounding it was considered and rejected. A `Debug` impl that spawned a thread per field
    /// would make every formatted provider a scheduling event, and the deadline it enforced could
    /// only ever be reported *inside* the debug text, so a log line would silently become a
    /// timeout report.
    ///
    /// The remaining option is the one taken here: **do not call author code at all.** `&self` is
    /// the only thing this method receives, and every fact about it is behind a trait method, so
    /// there is nothing safe left to print. `finish_non_exhaustive` renders `Provider { .. }`,
    /// which is Rust's conventional "there is more here that is not shown".
    ///
    /// Identity is not lost, only relocated. Renvor already holds every provider's name in records
    /// it built itself — [`ResolutionReport`], [`InitialisationOrder`], and the `ProviderId` values
    /// inside them — and those print the name from Renvor's own memory rather than by asking the
    /// author for it again. A caller who wants to see which provider this is should format the
    /// registry's report, not the trait object.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Provider").finish_non_exhaustive()
    }
}

/// The declared size of a provider graph, in the two units the ceilings are stated in.
///
/// Returned by [`ProviderRegistry::declared_size`] so a caller can assert against graph size and
/// traversal work **separately**, which is what SC-021 requires — conflating them is how a large
/// graph and a misbehaving traversal become indistinguishable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeclaredSize {
    /// How many providers are registered.
    pub providers: u32,
    /// How many dependency edges they declare in total.
    pub edges: u32,
}

/// The resolved initialisation sequence: dependencies before dependents.
///
/// Each entry pairs a provider's **registration position** with its name, in one vector rather
/// than two parallel ones. Two vectors can disagree; one cannot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InitialisationOrder {
    order: Vec<(u32, ProviderId)>,
}

impl InitialisationOrder {
    /// Builds an order from resolved positions and names.
    #[must_use]
    pub fn new(order: Vec<(u32, ProviderId)>) -> Self {
        Self { order }
    }

    /// Every entry, as `(registration position, name)`, in initialisation order.
    #[must_use]
    pub fn entries(&self) -> &[(u32, ProviderId)] {
        &self.order
    }

    /// The registration positions, in initialisation order.
    ///
    /// This is what the boot loop indexes with; the positions are *not* the order, which is the
    /// distinction C-L3 exists to protect.
    pub fn positions(&self) -> impl Iterator<Item = u32> + '_ {
        self.order.iter().map(|(position, _)| *position)
    }

    /// The provider names, in initialisation order.
    pub fn ids(&self) -> impl Iterator<Item = &ProviderId> + '_ {
        self.order.iter().map(|(_, id)| id)
    }

    /// How many providers are in the order.
    #[must_use]
    pub fn len(&self) -> usize {
        self.order.len()
    }

    /// Whether the order is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
}

/// What resolution actually cost, measured rather than estimated.
///
/// These fields exist **because SC-021 requires them to be assertable**, not as incidental
/// telemetry. Graph size and traversal work are separate fields for the same reason they are
/// separate ceilings: a number that mixes them cannot answer *which* of the two went wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolutionReport {
    /// Providers in the resolved graph.
    pub provider_count: u32,
    /// Dependency edges in the resolved graph.
    pub edge_count: u32,
    /// Provider examinations the traversal consumed.
    pub providers_examined: u32,
    /// Edge examinations the traversal consumed.
    pub edges_examined: u32,
    /// Their sum.
    pub work_units: u32,
}

/// The registered provider set, in registration order.
///
/// Registration order is **not** initialisation order — resolution may reorder — and keeping the
/// two ideas in separate types is what stops a test from asserting the wrong one and passing
/// (C-L3).
#[derive(Debug, Default)]
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Provider>>,
}

impl ProviderRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Appends a provider, preserving registration order.
    pub fn register(&mut self, provider: Box<dyn Provider>) {
        self.providers.push(provider);
    }

    /// The registered providers, in registration order.
    #[must_use]
    pub fn providers(&self) -> &[Box<dyn Provider>] {
        &self.providers
    }

    /// How many providers are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// The provider at a registration position, if it exists.
    #[must_use]
    pub fn get(&self, position: u32) -> Option<&dyn Provider> {
        self.providers.get(position as usize).map(AsRef::as_ref)
    }

    /// Checks the **declared** counts against the ceilings, before any traversal.
    ///
    /// Contract C-G2 and FR-039(a): an oversized graph is rejected on what the author declared,
    /// never discovered by exhausting a work budget. Discovering it the other way would report a
    /// kernel defect where the author has a graph that is simply too big.
    ///
    /// # Errors
    ///
    /// Returns [`KernelError::LimitExceeded`] naming the ceiling **and** the observed count.
    /// Providers are checked before edges, so the report names the more fundamental breach when
    /// both hold.
    pub fn declared_size(&self) -> Result<DeclaredSize, KernelError> {
        // Saturating rather than wrapping: a count that cannot fit in a `u32` is unambiguously
        // over a ceiling that fits in one, so saturation preserves the verdict.
        let providers = u32::try_from(self.providers.len()).unwrap_or(u32::MAX);
        if providers > MAX_PROVIDERS {
            return Err(KernelError::LimitExceeded {
                limit: "provider",
                ceiling: MAX_PROVIDERS,
                observed: providers,
            });
        }

        let edges = self.providers.iter().fold(0_u32, |total, provider| {
            let declared = u32::try_from(provider.dependencies().len()).unwrap_or(u32::MAX);
            total.saturating_add(declared)
        });
        if edges > MAX_EDGES {
            return Err(KernelError::LimitExceeded {
                limit: "dependency edge",
                ceiling: MAX_EDGES,
                observed: edges,
            });
        }

        Ok(DeclaredSize { providers, edges })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CapabilityId, InitContext, InitialisationOrder, Provider, ProviderFuture, ProviderId,
        ProviderRegistry,
    };
    use crate::error::ErrorCategory;
    use crate::provider::graph::{MAX_EDGES, MAX_PROVIDERS};

    /// A provider that does nothing but declare, which is all the registry cares about.
    struct Declared {
        id: ProviderId,
        provides: Vec<CapabilityId>,
        dependencies: Vec<CapabilityId>,
    }

    impl Declared {
        fn declaring(id: &str, provides: &[&str], dependencies: &[&str]) -> Box<dyn Provider> {
            Box::new(Self {
                id: ProviderId::new(id),
                provides: provides.iter().map(|c| CapabilityId::new(*c)).collect(),
                dependencies: dependencies.iter().map(|c| CapabilityId::new(*c)).collect(),
            })
        }
    }

    impl Provider for Declared {
        fn id(&self) -> &ProviderId {
            &self.id
        }
        fn provides(&self) -> &[CapabilityId] {
            &self.provides
        }
        fn dependencies(&self) -> &[CapabilityId] {
            &self.dependencies
        }
        fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
            Box::pin(async { Ok(()) })
        }
    }

    #[test]
    fn a_provider_id_and_a_capability_id_are_different_types() {
        // Both wrap a string. If they were one type, a dependency could name a provider instead of
        // a capability and the compiler would accept it. This test records the intent; the
        // enforcement is that the line below would not compile with the types swapped.
        let provider = ProviderId::new("postgres");
        let capability = CapabilityId::new("database");
        assert_eq!(provider.as_str(), "postgres");
        assert_eq!(capability.as_str(), "database");
        assert_eq!(provider.to_string(), "postgres");
    }

    #[test]
    fn declared_size_counts_providers_and_edges_separately() {
        let mut registry = ProviderRegistry::new();
        registry.register(Declared::declaring("a", &["one"], &[]));
        registry.register(Declared::declaring("b", &["two"], &["one"]));
        registry.register(Declared::declaring("c", &[], &["one", "two"]));

        let size = registry.declared_size().expect("well inside both ceilings");
        assert_eq!(size.providers, 3);
        assert_eq!(size.edges, 3, "1 + 2 declared dependencies");
    }

    #[test]
    fn too_many_providers_is_rejected_on_the_declared_count() {
        // FR-039(a) / C-G2: rejected before any traversal, naming ceiling and observed count.
        let mut registry = ProviderRegistry::new();
        for index in 0..=MAX_PROVIDERS {
            registry.register(Declared::declaring(&format!("p{index}"), &[], &[]));
        }

        let error = registry
            .declared_size()
            .expect_err("one over the ceiling must be rejected");
        assert_eq!(error.category(), ErrorCategory::LimitExceeded);
        let rendered = error.to_string();
        assert!(
            rendered.contains("1025"),
            "observed count missing: {rendered}"
        );
        assert!(rendered.contains("1024"), "ceiling missing: {rendered}");
    }

    #[test]
    fn exactly_the_ceiling_is_accepted() {
        // POSITIVE CONTROL for the test above: the check rejects *over* the ceiling, not *at* it.
        // Without this, an off-by-one that rejected the maximum legal graph would pass unnoticed.
        let mut registry = ProviderRegistry::new();
        for index in 0..MAX_PROVIDERS {
            registry.register(Declared::declaring(&format!("p{index}"), &[], &[]));
        }
        let size = registry
            .declared_size()
            .expect("the ceiling itself is legal");
        assert_eq!(size.providers, MAX_PROVIDERS);
    }

    #[test]
    fn too_many_edges_is_rejected_on_the_declared_count() {
        // 9 providers each declaring 1024 dependencies is 9216 edges — over the 8192 ceiling while
        // the provider count stays far under its own, so this isolates the edge axis.
        let capabilities: Vec<String> = (0..1024).map(|index| format!("cap{index}")).collect();
        let borrowed: Vec<&str> = capabilities.iter().map(String::as_str).collect();

        let mut registry = ProviderRegistry::new();
        for index in 0..9 {
            registry.register(Declared::declaring(&format!("p{index}"), &[], &borrowed));
        }

        let error = registry
            .declared_size()
            .expect_err("9216 edges is over the ceiling");
        assert_eq!(error.category(), ErrorCategory::LimitExceeded);
        let rendered = error.to_string();
        assert!(rendered.contains("9216"), "{rendered}");
        assert!(rendered.contains(&MAX_EDGES.to_string()), "{rendered}");
        assert!(
            rendered.contains("dependency edge"),
            "the breached axis must be named: {rendered}"
        );
    }

    #[test]
    fn an_empty_registry_resolves_to_an_empty_order() {
        let order = InitialisationOrder::default();
        assert!(order.is_empty());
        assert_eq!(order.len(), 0);
        assert_eq!(order.ids().count(), 0);
    }

    #[test]
    fn an_order_keeps_positions_and_names_together() {
        // One vector of pairs rather than two parallel vectors: the two cannot fall out of step.
        let order = InitialisationOrder::new(vec![
            (2, ProviderId::new("db")),
            (0, ProviderId::new("http")),
        ]);
        assert_eq!(order.positions().collect::<Vec<_>>(), vec![2, 0]);
        assert_eq!(
            order.ids().map(ProviderId::as_str).collect::<Vec<_>>(),
            vec!["db", "http"]
        );
    }

    #[test]
    fn a_provider_declares_nothing_by_default() {
        struct Bare(ProviderId);
        impl Provider for Bare {
            fn id(&self) -> &ProviderId {
                &self.0
            }
            fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
                Box::pin(async { Ok(()) })
            }
        }

        let bare = Bare(ProviderId::new("leaf"));
        assert!(bare.provides().is_empty());
        assert!(bare.dependencies().is_empty());
    }
}
