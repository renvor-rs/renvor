//! Providers: registration, dependency declaration, and single-pass resolution.
//!
//! A provider is a unit of application capability that may depend on other capabilities. The
//! kernel's job here is narrow and entirely deterministic: given a set of registered providers and
//! their declared dependencies, decide an initialisation order, or refuse with a diagnostic that
//! names every provider involved in the refusal.
//!
//! The resolution machinery lives in [`graph`]. It is separated from registration because the two
//! answer different questions — registration decides what is *declared*, resolution decides what
//! that declaration *means* — and because the resolver carries a counted work budget that must be
//! observable on its own.
//!
//! # This module is the Register phase
//!
//! [`ProviderRegistry::resolve`] is the wiring between the two: it turns declared capabilities
//! into graph indices, runs the instrumented traversal, and converts the resolver's own error
//! shapes into the kernel taxonomy. Everything it can refuse, it refuses **here** — before any
//! provider is booted, which is what SC-005's "0 cases reach Boot" actually means.
//!
//! The order it checks in is load-bearing, not incidental:
//!
//! 1. **Declared ceilings** — on counts alone, before a graph exists (C-G2).
//! 2. **Ambiguous capabilities** — before dependencies are looked up, because a lookup against an
//!    ambiguous map would silently pick a winner.
//! 3. **Missing dependencies** — naming both endpoints (C-G11).
//! 4. **The traversal**, then its **work budget**, then **cycles**.
//!
//! Budget is checked before cycles deliberately. A cycle inside the ceilings must be reported *as
//! a cycle* (FR-039c), so if the budget were somehow exhausted on a cyclic graph the kernel must
//! say the kernel is broken rather than blame the author's graph — and it can only say that if it
//! looks at the budget first.

pub mod graph;
pub mod registry;

pub use registry::{
    CapabilityId, DeclaredSize, InitContext, InitialisationOrder, Provider, ProviderFuture,
    ProviderId, ProviderRegistry, ResolutionReport,
};

use std::collections::HashMap;

use crate::error::KernelError;
use crate::provider::graph::{GraphSizeError, ProviderIx, ResolverGraphBuilder};

impl ProviderRegistry {
    /// Resolves the registered providers into an initialisation order.
    ///
    /// Returns the order **and** the measured resolution report, because SC-021 requires graph
    /// size and traversal work to be assertable separately and a caller that only gets the order
    /// cannot assert either.
    ///
    /// # Errors
    ///
    /// Returns, in the order the module documentation lists:
    ///
    /// - [`KernelError::LimitExceeded`] — a declared ceiling was breached (FR-039a).
    /// - [`KernelError::CapabilityDuplicate`] — two providers offer the same capability.
    /// - [`KernelError::DependencyMissing`] — a declared dependency is unprovided (FR-014).
    /// - [`KernelError::Internal`] — the work budget was exhausted. **A defect in Renvor**, never
    ///   a statement about the author's graph (FR-039c).
    /// - [`KernelError::DependencyCycle`] — providers form a cycle, naming **every** provider in
    ///   it (FR-013).
    pub fn resolve(&self) -> Result<(InitialisationOrder, ResolutionReport), KernelError> {
        let size = self.declared_size()?;

        // Step 2: the capability map. Built before any lookup, because a map that silently kept
        // one of two providers for a capability would answer every later lookup confidently and
        // wrongly — the silent fallback FR-022 prohibits.
        let mut offered: HashMap<&CapabilityId, u32> = HashMap::new();
        for (position, provider) in self.providers().iter().enumerate() {
            let position = u32::try_from(position).unwrap_or(u32::MAX);
            for capability in provider.provides() {
                if let Some(first) = offered.insert(capability, position) {
                    return Err(KernelError::CapabilityDuplicate {
                        capability: capability.to_string(),
                        first: self.name_at(first),
                        second: provider.id().to_string(),
                    });
                }
            }
        }

        // Step 3: edges, directed dependent -> dependency (C-G5). That direction is what makes the
        // traversal's reverse-topological output the initialisation order with no second pass.
        let mut builder =
            ResolverGraphBuilder::with_capacity(size.providers as usize, size.edges as usize);
        for provider in self.providers() {
            let mut targets = Vec::with_capacity(provider.dependencies().len());
            for capability in provider.dependencies() {
                let Some(&target) = offered.get(capability) else {
                    return Err(KernelError::DependencyMissing {
                        dependent: provider.id().to_string(),
                        capability: capability.to_string(),
                    });
                };
                targets.push(ProviderIx::new(target));
            }
            builder.push_provider(targets);
        }

        let graph = builder.build().map_err(|error| self.size_error(error))?;

        // Step 4: one traversal, then its budget, then cycles.
        let resolution = graph.resolve();
        resolution.check_budget(size.providers, size.edges)?;

        if let Some(cycle) = resolution.cycles().next() {
            return Err(KernelError::DependencyCycle {
                providers: cycle
                    .members()
                    .iter()
                    .map(|member| self.name_at(member.get()))
                    .collect(),
            });
        }

        let counters = resolution.counters();
        let order = InitialisationOrder::new(
            resolution
                .initialisation_order()
                .into_iter()
                .map(|index| (index.get(), self.id_at(index.get())))
                .collect(),
        );

        Ok((
            order,
            ResolutionReport {
                provider_count: size.providers,
                edge_count: size.edges,
                providers_examined: counters.provider_examinations,
                edges_examined: counters.edge_examinations,
                work_units: counters.total_work_units(),
            },
        ))
    }

    /// The provider name at a registration position.
    ///
    /// Falls back to the position itself rather than panicking. Every caller here passes an index
    /// the resolver returned, so the fallback is unreachable — but SC-004 requires **0** panics in
    /// ordinary use, and an index-out-of-range panic in a diagnostic path would turn a helpful
    /// error into a crash at exactly the moment the author needed the error.
    fn id_at(&self, position: u32) -> ProviderId {
        self.get(position).map_or_else(
            || ProviderId::new(format!("<unregistered position {position}>")),
            |provider| provider.id().clone(),
        )
    }

    /// The provider name at a registration position, as a `String` for error payloads.
    fn name_at(&self, position: u32) -> String {
        self.id_at(position).as_str().to_owned()
    }

    /// Converts a graph-construction refusal into the kernel taxonomy.
    ///
    /// The two ceiling variants are already caught by [`Self::declared_size`], so reaching them
    /// here would mean the two checks disagree; they are converted faithfully rather than
    /// swallowed, so a disagreement surfaces as the same diagnostic instead of vanishing.
    ///
    /// `UnknownDependency` is structurally unreachable from this caller: every index handed to the
    /// builder came from the capability map, which only ever holds registration positions that
    /// exist. It is converted rather than asserted for the SC-004 reason above — a future refactor
    /// that broke the invariant should degrade to a diagnostic, not to a panic.
    fn size_error(&self, error: GraphSizeError) -> KernelError {
        match error {
            GraphSizeError::TooManyProviders { declared, ceiling } => KernelError::LimitExceeded {
                limit: "provider",
                ceiling,
                observed: declared,
            },
            GraphSizeError::TooManyEdges { declared, ceiling } => KernelError::LimitExceeded {
                limit: "dependency edge",
                ceiling,
                observed: declared,
            },
            GraphSizeError::UnknownDependency {
                dependent,
                named,
                provider_count,
            } => KernelError::DependencyMissing {
                dependent: self.name_at(dependent.get()),
                capability: format!(
                    "<registration position {named}, which is outside the {provider_count} registered providers>"
                ),
            },
        }
    }
}
