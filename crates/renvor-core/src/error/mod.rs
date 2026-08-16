//! The kernel error taxonomy: fourteen categories, each matchable without reading message text.
//!
//! # Why a category is a separate type
//!
//! Contract C-E1 requires a category to be inspectable **programmatically**. Matching on a message
//! string is not an API; it is a defect waiting for a rewording. [`ErrorCategory`] is therefore a
//! small `Copy` enum that callers match on, and [`KernelError::category`] is total — every error
//! has exactly one category, decided at construction and never inferred from text.
//!
//! # Redaction is enforced by construction, not by discipline
//!
//! Contract C-E3 allows **0** error output form to emit a secret-marked configuration value or the
//! contents of registered typed state. The usual way to satisfy that is a rule ("do not put values
//! in errors") plus a test that looks for leaks. This module does something stronger: **no variant
//! has a field that can hold a configuration value or a state value.**
//!
//! [`KernelError::Configuration`] takes the key, the violated constraint, the source layer, and
//! the expected type. Every one of those is safe by construction — they describe the *shape* of
//! the problem, never the data. A caller who wants to put the offending value in an error cannot,
//! because there is no field for it. That is the difference between a rule and a type.
//!
//! State variants carry `&'static str` type names only, which is exactly what FR-037(b) permits
//! and no more. The kernel never assumes opaque state is safe to print merely because it was not
//! marked secret.
//!
//! # `Internal` is deliberately unreachable in ordinary use
//!
//! FR-039(c) forbids reporting a dependency cycle by exhausting the resolution work budget, so
//! budget exhaustion must not be representable as any author-facing category. It gets its own
//! variant, and the doc comment says plainly what seeing it means: the kernel is wrong, not the
//! author's graph.

pub mod context;

use crate::lifecycle::LifecyclePhase;
use crate::provider::graph::BudgetAxis;

/// A programmatically matchable classification for every [`KernelError`].
///
/// One variant per row of contract C-E1. Kept separate from the error itself so a caller can
/// match on *what kind of thing went wrong* without destructuring the payload, and so a category
/// can be recorded as a tracing field or a metric label without formatting an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ErrorCategory {
    /// A configuration value is missing, ill-typed, out of range, or undecodable.
    Configuration,
    /// Incompatible structural shapes for the same key across layers.
    ConfigurationConflict,
    /// A second value of the same type was registered.
    StateDuplicate,
    /// An unregistered type was retrieved.
    StateMissing,
    /// Providers form a dependency cycle.
    DependencyCycle,
    /// A declared dependency is unprovided.
    DependencyMissing,
    /// Two providers offer the same capability, so a dependency on it has no single answer.
    CapabilityDuplicate,
    /// A provider or edge ceiling was breached at Register.
    LimitExceeded,
    /// A provider failed during Boot.
    ProviderInit,
    /// A provider failed during Stop or rollback.
    ProviderStop,
    /// A cancellation signal ended the operation.
    Cancelled,
    /// A bounded wait elapsed.
    DeadlineExceeded,
    /// Work was submitted after shutdown began.
    ShuttingDown,
    /// The host refused a resource the kernel asked for.
    ResourceUnavailable,
    /// A defect in the kernel itself.
    Internal,
}

impl ErrorCategory {
    /// Every category, for exhaustiveness tests.
    ///
    /// C-E1 lists fifteen rows as of taxonomy 1.2.0. A test asserts this array's length against
    /// that number, so adding a category without updating the contract fails loudly.
    pub const ALL: [Self; 15] = [
        Self::Configuration,
        Self::ConfigurationConflict,
        Self::StateDuplicate,
        Self::StateMissing,
        Self::DependencyCycle,
        Self::DependencyMissing,
        Self::CapabilityDuplicate,
        Self::LimitExceeded,
        Self::ProviderInit,
        Self::ProviderStop,
        Self::Cancelled,
        Self::DeadlineExceeded,
        Self::ShuttingDown,
        Self::ResourceUnavailable,
        Self::Internal,
    ];

    /// The stable, lowercase name used as a tracing field and metric label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Configuration => "configuration",
            Self::ConfigurationConflict => "configuration_conflict",
            Self::StateDuplicate => "state_duplicate",
            Self::StateMissing => "state_missing",
            Self::DependencyCycle => "dependency_cycle",
            Self::DependencyMissing => "dependency_missing",
            Self::CapabilityDuplicate => "capability_duplicate",
            Self::LimitExceeded => "limit_exceeded",
            Self::ProviderInit => "provider_init",
            Self::ProviderStop => "provider_stop",
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::ShuttingDown => "shutting_down",
            Self::ResourceUnavailable => "resource_unavailable",
            Self::Internal => "internal",
        }
    }

    /// Whether this category indicates a defect in the kernel rather than a problem the author
    /// can act on.
    ///
    /// Exactly one category is a kernel defect. Exposing the question as a method keeps callers
    /// from re-deriving the answer with a match they will forget to update.
    #[must_use]
    pub const fn is_kernel_defect(self) -> bool {
        matches!(self, Self::Internal)
    }
}

impl core::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The boxed cause carried by variants that wrap an author-supplied failure.
///
/// `Send + Sync` because a provider failure crosses task boundaries, and `'static` because it
/// outlives the phase that produced it.
pub type BoxedCause = Box<dyn std::error::Error + Send + Sync + 'static>;

/// Every error the kernel produces.
///
/// Constructed only through its variants, each of which names exactly what contract C-E1 requires
/// it to name — and, by omission, nothing it must not. See the module documentation on why the
/// absent fields are the load-bearing part.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum KernelError {
    /// A configuration value is missing, ill-typed, out of range, or undecodable.
    ///
    /// Carries the key, the violated constraint, the source layer, and the expected type. It
    /// deliberately carries **no field for the offending value** (C-E3).
    ///
    /// `#[non_exhaustive]` so no crate outside `renvor-core` can build it with a struct literal:
    /// adapters must come through [`context::configuration`], which takes a
    /// [`context::Constraint`] that cannot hold a value. Without that, an adapter could forward a
    /// decoder message quoting the offending value straight into `constraint` — which is exactly
    /// what measurement found the first adapter doing.
    #[non_exhaustive]
    #[error(
        "configuration key `{key}` from layer `{layer}` is invalid: expected {expected_type}, {constraint}"
    )]
    Configuration {
        /// The dotted configuration key.
        key: String,
        /// The constraint that was violated, phrased to complete the message above.
        constraint: String,
        /// The layer that supplied the offending value.
        layer: String,
        /// The type the kernel expected to decode.
        expected_type: &'static str,
    },

    /// The same key has incompatible structural shapes in two layers.
    ///
    /// Names **both** layers. Naming one leaves the author bisecting.
    #[error(
        "configuration key `{key}` has conflicting shapes: `{first_layer}` supplies {first_shape}, `{second_layer}` supplies {second_shape}"
    )]
    ConfigurationConflict {
        /// The dotted configuration key.
        key: String,
        /// The earlier layer in precedence order.
        first_layer: String,
        /// The shape that layer supplied, e.g. `table`.
        first_shape: &'static str,
        /// The later layer in precedence order.
        second_layer: String,
        /// The shape that layer supplied, e.g. `string`.
        second_shape: &'static str,
    },

    /// A second value of the same type was registered.
    #[error("state of type `{type_name}` is already registered")]
    StateDuplicate {
        /// The type name, which FR-037(b) permits the kernel to emit.
        type_name: &'static str,
    },

    /// An unregistered type was retrieved.
    #[error("no state of type `{type_name}` is registered")]
    StateMissing {
        /// The type name, which FR-037(b) permits the kernel to emit.
        type_name: &'static str,
    },

    /// Providers form a dependency cycle.
    ///
    /// Carries **every** provider in the cycle. This is why the resolver uses `tarjan_scc`: a
    /// single-node cycle report would leave the author bisecting a graph they already cannot boot.
    #[error("provider dependency cycle: {}", .providers.join(" -> "))]
    DependencyCycle {
        /// Every provider in the cycle, in the order the resolver reported them.
        providers: Vec<String>,
    },

    /// A provider declared a dependency nobody registered.
    ///
    /// Names **both** the dependent and the missing capability (C-G11).
    #[error(
        "provider `{dependent}` depends on capability `{capability}`, which no provider offers"
    )]
    DependencyMissing {
        /// The provider that declared the dependency.
        dependent: String,
        /// The capability it named.
        capability: String,
    },

    /// Two providers offer the same capability.
    ///
    /// Names the capability **and both** providers, because the author's next question is
    /// "which two?" and a diagnostic naming one of them answers half of it.
    ///
    /// This row exists because implementing the registry found a case no earlier artifact
    /// covered. A dependency on an ambiguously-provided capability has no single answer, and the
    /// three ways of not saying so are each already prohibited: picking a winner is a silent
    /// fallback (FR-022), panicking breaks SC-004's zero-panic assertion, and reporting it as
    /// [`Self::DependencyMissing`] would print a false statement — the capability *is* provided,
    /// twice. See `governance/phase-002-evidence.md`.
    #[error(
        "capability `{capability}` is offered by two providers: `{first}` and `{second}` — a dependency on it has no single answer"
    )]
    CapabilityDuplicate {
        /// The capability offered twice.
        capability: String,
        /// The provider that offered it first, in registration order.
        first: String,
        /// The provider that offered it again.
        second: String,
    },

    /// A declared graph exceeded a ceiling at Register.
    ///
    /// Names the ceiling **and** the observed count. This is raised on the **declared** counts,
    /// before traversal — never by running out of work budget (C-G2).
    #[error("{limit} ceiling exceeded at register: declared {observed}, limit {ceiling}")]
    LimitExceeded {
        /// Which ceiling, e.g. `provider` or `dependency edge`.
        limit: &'static str,
        /// The ceiling that was exceeded.
        ceiling: u32,
        /// The declared count.
        observed: u32,
    },

    /// A provider failed during Boot.
    #[error("provider `{provider}` failed to initialise")]
    ProviderInit {
        /// The failing provider.
        provider: String,
        /// The originating failure, preserved rather than flattened (C-E2).
        #[source]
        source: BoxedCause,
    },

    /// A provider failed during Stop or rollback.
    ///
    /// One of these is produced per failing provider. A single failure never masks its siblings
    /// (C-L2): Stop reports **every** failure, not just the first.
    #[error("provider `{provider}` failed to stop")]
    ProviderStop {
        /// The failing provider.
        provider: String,
        /// The originating failure, preserved rather than flattened (C-E2).
        #[source]
        source: BoxedCause,
    },

    /// A cancellation signal ended the operation.
    #[error("cancelled during the `{phase}` phase")]
    Cancelled {
        /// The phase the lifecycle had reached.
        phase: LifecyclePhase,
    },

    /// A bounded wait elapsed.
    #[error("`{operation}` exceeded its {deadline_ms}ms deadline")]
    DeadlineExceeded {
        /// The operation that timed out.
        operation: String,
        /// The deadline it exceeded, in milliseconds.
        deadline_ms: u64,
    },

    /// Work was submitted after shutdown began.
    #[error("`{operation}` was rejected because shutdown has begun")]
    ShuttingDown {
        /// The rejected operation.
        operation: String,
    },

    /// The host refused a resource the kernel asked for, and no author input caused it.
    ///
    /// **This is not [`Self::Internal`], and the difference is the whole reason it exists.** An
    /// exhausted thread table or a process running against `RLIMIT_NPROC` is an environment
    /// failure; reporting it as a kernel defect would tell an author their framework is broken
    /// when their host is. That is the identical argument the builder module already makes for
    /// entropy, applied to the second resource the kernel asks the operating system for.
    ///
    /// It is not [`Self::LimitExceeded`] either: that names a ceiling **Renvor declared**, and
    /// there is no Renvor ceiling on operating-system threads.
    #[error("the host could not supply {resource} needed to {operation}: {cause}")]
    ResourceUnavailable {
        /// What the host refused, e.g. `an operating-system thread`.
        resource: &'static str,
        /// What it was needed for, naming the source and the call so the failure is attributable.
        operation: String,
        /// The operating system's own reason, forwarded rather than paraphrased.
        cause: String,
    },

    /// The resolution work budget was exhausted — a defect in the kernel.
    ///
    /// **If an author sees this, the kernel is wrong, not their graph.** The budget is a constant
    /// multiple of the accepted graph size, so every graph inside the ceilings reaches its verdict
    /// inside the budget (C-G9). It is a distinct category precisely so that a cycle can never be
    /// reported as budget exhaustion, nor budget exhaustion as a cycle.
    #[error(
        "internal: resolution work budget exhausted on the {axis:?} axis — observed {observed}, allowed {allowed}. This is a defect in Renvor, not in your provider graph"
    )]
    Internal {
        /// Which budget axis was exceeded.
        axis: BudgetAxis,
        /// What the traversal consumed.
        observed: u32,
        /// What it was allowed to consume.
        allowed: u32,
    },
}

impl KernelError {
    /// This error's category.
    ///
    /// Total by construction: the match below has one arm per variant, so adding a variant without
    /// giving it a category is a compile error rather than a runtime surprise.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        match self {
            Self::Configuration { .. } => ErrorCategory::Configuration,
            Self::ConfigurationConflict { .. } => ErrorCategory::ConfigurationConflict,
            Self::StateDuplicate { .. } => ErrorCategory::StateDuplicate,
            Self::StateMissing { .. } => ErrorCategory::StateMissing,
            Self::DependencyCycle { .. } => ErrorCategory::DependencyCycle,
            Self::DependencyMissing { .. } => ErrorCategory::DependencyMissing,
            Self::CapabilityDuplicate { .. } => ErrorCategory::CapabilityDuplicate,
            Self::LimitExceeded { .. } => ErrorCategory::LimitExceeded,
            Self::ProviderInit { .. } => ErrorCategory::ProviderInit,
            Self::ProviderStop { .. } => ErrorCategory::ProviderStop,
            Self::Cancelled { .. } => ErrorCategory::Cancelled,
            Self::DeadlineExceeded { .. } => ErrorCategory::DeadlineExceeded,
            Self::ShuttingDown { .. } => ErrorCategory::ShuttingDown,
            Self::ResourceUnavailable { .. } => ErrorCategory::ResourceUnavailable,
            Self::Internal { .. } => ErrorCategory::Internal,
        }
    }

    /// Whether this error indicates a defect in the kernel rather than a problem the author can
    /// act on.
    #[must_use]
    pub const fn is_kernel_defect(&self) -> bool {
        self.category().is_kernel_defect()
    }
}

/// Budget exhaustion becomes an [`ErrorCategory::Internal`] error and **nothing else**.
///
/// This conversion is the whole of FR-039(c)'s enforcement: it is the only route from the
/// resolver's budget check into the kernel's error type, and it lands on the one category that
/// says plainly the kernel is at fault. There is no path by which an exhausted budget becomes a
/// cycle, a ceiling breach, or any other author-facing diagnostic, because no other conversion
/// exists.
impl From<crate::provider::graph::BudgetExhausted> for KernelError {
    fn from(exhausted: crate::provider::graph::BudgetExhausted) -> Self {
        Self::Internal {
            axis: exhausted.axis,
            observed: exhausted.observed,
            allowed: exhausted.allowed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ErrorCategory, KernelError};
    use crate::lifecycle::LifecyclePhase;
    use crate::provider::graph::BudgetAxis;

    /// One error per category, so the tests below can iterate rather than repeat themselves.
    fn one_of_each() -> Vec<KernelError> {
        vec![
            KernelError::Configuration {
                key: "server.threads".into(),
                constraint: "must be at least 1".into(),
                layer: "environment".into(),
                expected_type: "u16",
            },
            KernelError::ConfigurationConflict {
                key: "server".into(),
                first_layer: "base.toml".into(),
                first_shape: "table",
                second_layer: "environment".into(),
                second_shape: "string",
            },
            KernelError::StateDuplicate {
                type_name: "my_app::Db",
            },
            KernelError::StateMissing {
                type_name: "my_app::Db",
            },
            KernelError::DependencyCycle {
                providers: vec!["a".into(), "b".into(), "c".into()],
            },
            KernelError::DependencyMissing {
                dependent: "http".into(),
                capability: "database".into(),
            },
            KernelError::CapabilityDuplicate {
                capability: "database".into(),
                first: "postgres".into(),
                second: "sqlite".into(),
            },
            KernelError::LimitExceeded {
                limit: "provider",
                ceiling: 1024,
                observed: 1025,
            },
            KernelError::ProviderInit {
                provider: "db".into(),
                source: "connection refused".into(),
            },
            KernelError::ProviderStop {
                provider: "db".into(),
                source: "flush failed".into(),
            },
            KernelError::Cancelled {
                phase: LifecyclePhase::Boot,
            },
            KernelError::DeadlineExceeded {
                operation: "drain".into(),
                deadline_ms: 30_000,
            },
            KernelError::ShuttingDown {
                operation: "enqueue".into(),
            },
            KernelError::ResourceUnavailable {
                resource: "an operating-system thread",
                operation: "load configuration source `application configuration`".to_owned(),
                cause: "cannot allocate memory".to_owned(),
            },
            KernelError::Internal {
                axis: BudgetAxis::ProviderExaminations,
                observed: 9216,
                allowed: 2048,
            },
        ]
    }

    #[test]
    fn the_taxonomy_has_the_fifteen_categories_the_contract_lists() {
        assert_eq!(
            ErrorCategory::ALL.len(),
            15,
            "contract C-E1 revision 1.2.0 lists fifteen categories"
        );
        let mut names: Vec<&str> = ErrorCategory::ALL.iter().map(|c| c.as_str()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "two categories share a name");
    }

    #[test]
    fn every_category_is_produced_by_at_least_one_error() {
        // Without this, a category could exist in the enum with no way to construct an error
        // carrying it — a taxonomy row that documents a state the kernel cannot reach.
        let mut produced: Vec<ErrorCategory> =
            one_of_each().iter().map(KernelError::category).collect();
        produced.sort_unstable();
        produced.dedup();
        assert_eq!(produced.len(), ErrorCategory::ALL.len());
        for category in ErrorCategory::ALL {
            assert!(produced.contains(&category), "{category} is unreachable");
        }
    }

    #[test]
    fn exactly_one_category_is_a_kernel_defect() {
        // C-E9: budget exhaustion must be distinct from every author-facing diagnostic. If a
        // second category ever became a "defect", a caller routing defects to a bug report would
        // start filing the author's mistakes as kernel bugs.
        let defects: Vec<_> = ErrorCategory::ALL
            .into_iter()
            .filter(|c| c.is_kernel_defect())
            .collect();
        assert_eq!(defects, vec![ErrorCategory::Internal]);

        // POSITIVE CONTROL: the predicate can also return false.
        assert!(!ErrorCategory::Configuration.is_kernel_defect());
    }

    #[test]
    fn a_cycle_error_names_every_provider_in_the_cycle() {
        let error = KernelError::DependencyCycle {
            providers: vec!["alpha".into(), "beta".into(), "gamma".into()],
        };
        let rendered = error.to_string();
        for provider in ["alpha", "beta", "gamma"] {
            assert!(
                rendered.contains(provider),
                "cycle diagnostic omitted `{provider}`: {rendered}"
            );
        }
    }

    #[test]
    fn a_conflict_error_names_both_layers() {
        let error = KernelError::ConfigurationConflict {
            key: "server".into(),
            first_layer: "base.toml".into(),
            first_shape: "table",
            second_layer: "environment".into(),
            second_shape: "string",
        };
        let rendered = error.to_string();
        assert!(rendered.contains("base.toml"), "{rendered}");
        assert!(rendered.contains("environment"), "{rendered}");
    }

    #[test]
    fn a_duplicate_capability_error_names_the_capability_and_both_providers() {
        // The author's next question is "which two?". A diagnostic naming one answers half of it.
        let rendered = KernelError::CapabilityDuplicate {
            capability: "database".into(),
            first: "postgres".into(),
            second: "sqlite".into(),
        }
        .to_string();
        for named in ["database", "postgres", "sqlite"] {
            assert!(rendered.contains(named), "`{named}` missing: {rendered}");
        }
    }

    #[test]
    fn budget_exhaustion_converts_only_to_the_internal_category() {
        // FR-039(c). The conversion is the single route from the resolver's budget check into
        // KernelError, so an exhausted budget cannot arrive wearing an author-facing category.
        use crate::provider::graph::BudgetExhausted;

        let error: KernelError = BudgetExhausted {
            axis: BudgetAxis::TotalWorkUnits,
            observed: 20_000,
            allowed: 18_432,
        }
        .into();
        assert_eq!(error.category(), ErrorCategory::Internal);
        assert!(error.is_kernel_defect());

        // POSITIVE CONTROL: an author-facing error is not a kernel defect, so the assertion above
        // discriminates rather than holding for everything.
        assert!(
            !KernelError::DependencyCycle {
                providers: vec!["a".into(), "b".into()],
            }
            .is_kernel_defect()
        );
    }

    #[test]
    fn a_missing_dependency_error_names_both_endpoints() {
        let rendered = KernelError::DependencyMissing {
            dependent: "http".into(),
            capability: "database".into(),
        }
        .to_string();
        assert!(rendered.contains("http"), "{rendered}");
        assert!(rendered.contains("database"), "{rendered}");
    }

    #[test]
    fn causal_chains_are_preserved_rather_than_flattened() {
        // C-E2. Flattening a chain into one message destroys the only information that makes a
        // nested failure diagnosable.
        use std::error::Error as _;

        let error = KernelError::ProviderInit {
            provider: "db".into(),
            source: "connection refused".into(),
        };
        let cause = error.source().expect("the cause must survive construction");
        assert_eq!(cause.to_string(), "connection refused");
        assert!(
            !error.to_string().contains("connection refused"),
            "the top-level message should name the provider; the cause is reached via source()"
        );

        // POSITIVE CONTROL: a variant with no cause reports none, so `source()` is discriminating
        // rather than always-Some.
        assert!(
            KernelError::StateMissing {
                type_name: "my_app::Db"
            }
            .source()
            .is_none()
        );
    }

    #[test]
    fn no_error_variant_can_carry_a_configuration_or_state_value() {
        // C-E3 by construction. This test reads the module's own source and asserts that the
        // Configuration variant's field set is exactly the four safe fields. A future edit adding
        // a `value` field would be caught here even if no leak test happened to exercise it.
        let source = include_str!("mod.rs");
        let start = source
            .find("Configuration {")
            .expect("the Configuration variant exists");
        let body = &source[start..start + 400];

        for safe in ["key:", "constraint:", "layer:", "expected_type:"] {
            assert!(body.contains(safe), "Configuration lost its `{safe}` field");
        }
        for unsafe_field in ["value:", "raw:", "supplied:"] {
            assert!(
                !body.contains(unsafe_field),
                "Configuration gained `{unsafe_field}`, which can carry a secret value (C-E3)"
            );
        }

        // POSITIVE CONTROL: the scan reads real text and can find a field name that IS present.
        assert!(body.contains("key:"));
    }

    #[test]
    fn state_errors_emit_a_type_name_and_nothing_else() {
        // FR-037(b) permits the type name only. The variants have no other field, so this is
        // enforced by the type; the test records the intent and would catch a widened variant.
        let rendered = KernelError::StateDuplicate {
            type_name: "my_app::Db",
        }
        .to_string();
        assert!(rendered.contains("my_app::Db"));
        assert_eq!(
            rendered, "state of type `my_app::Db` is already registered",
            "the message must not grow fields that could carry state contents"
        );
    }

    #[test]
    fn the_internal_error_says_plainly_that_the_kernel_is_at_fault() {
        // If an author ever sees this, the wrong reaction is for them to go looking at their own
        // graph. The message is the only thing standing between them and an hour wasted.
        let rendered = KernelError::Internal {
            axis: BudgetAxis::EdgeExaminations,
            observed: 20_000,
            allowed: 16_384,
        }
        .to_string();
        assert!(rendered.contains("defect in Renvor"), "{rendered}");
        assert!(rendered.contains("20000"), "the counters are named");
        assert!(rendered.contains("16384"), "the allowance is named");
    }
}
