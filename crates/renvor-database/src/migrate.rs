//! The migration contract: ordering, checksums, bounds, reversibility, and policy.
//!
//! # This contract describes a wrapper, not an engine
//!
//! Constitution principle III forbids building custom infrastructure *"merely to own the
//! implementation"*. SQLx already provides deterministic ordering, per-migration checksums,
//! fail-closed mismatch detection, and a **database-level** lock. Renvor does not reimplement any
//! of that.
//!
//! What it adds is what the driver does not provide and the contracts require:
//!
//! | Requirement | Source |
//! |---|---|
//! | a **bounded** lock wait | `sqlx-mysql` issues `GET_LOCK(?, -1)` — an infinite wait |
//! | reversibility declared, and refused before data changes | `PLAN.md` §12 |
//! | observability per migration | `PLAN.md` §12 |
//! | automatic migration is never the production default | `PLAN.md` §12 |
//! | redacted failures | constitution principle VI |

use core::time::Duration;

/// Whether a migration can be undone.
///
/// # Declared, never inferred
///
/// The presence of a `down` script is evidence that somebody *wrote* one, not that running it is
/// safe. A migration that dropped a column has a `down` that recreates the column and cannot
/// recreate the data. `PLAN.md` §12 requires rollback support to be *declared per migration*, and
/// this type is that declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reversibility {
    /// A rollback exists and is safe to run.
    Reversible,
    /// No rollback. A request to undo this migration is refused **before** the lock is taken.
    Irreversible,
}

impl Reversibility {
    /// Whether a rollback may be attempted.
    #[must_use]
    pub const fn is_reversible(self) -> bool {
        matches!(self, Self::Reversible)
    }
}

/// When migrations run.
///
/// # The default is `Never`, and that is a safety decision
///
/// `PLAN.md` §12: *"Production does not automatically run irreversible migrations without an
/// explicit deployment policy."* A framework whose default is "migrate on boot" makes every
/// deployment a schema change, including the rollback deployment that was supposed to undo one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MigrationPolicy {
    /// Migrations never run automatically. They run when an operator asks.
    #[default]
    Never,
    /// Migrations run during boot, before readiness is reported.
    ///
    /// Appropriate for development and for deployments whose policy says so. Choosing it is an
    /// explicit, recorded act — it is never reached by leaving a field unset.
    OnBoot,
}

impl MigrationPolicy {
    /// Whether boot should apply pending migrations.
    #[must_use]
    pub const fn runs_on_boot(self) -> bool {
        matches!(self, Self::OnBoot)
    }
}

/// The longest Renvor waits for the migration lock.
///
/// Renvor imposes this because the driver does not. See the module documentation.
pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(60);

/// The ceiling on any migration bound.
pub const MAX_MIGRATION_TIMEOUT: Duration = Duration::from_secs(3600);

/// Bounded migration settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationSettings {
    policy: MigrationPolicy,
    lock_timeout: Duration,
    run_timeout: Duration,
}

impl Default for MigrationSettings {
    fn default() -> Self {
        Self {
            policy: MigrationPolicy::Never,
            lock_timeout: DEFAULT_LOCK_TIMEOUT,
            run_timeout: Duration::from_secs(300),
        }
    }
}

impl MigrationSettings {
    /// The policy.
    #[must_use]
    pub const fn policy(&self) -> MigrationPolicy {
        self.policy
    }

    /// How long to wait for the migration lock before giving up.
    #[must_use]
    pub const fn lock_timeout(&self) -> Duration {
        self.lock_timeout
    }

    /// How long the whole migration run may take.
    #[must_use]
    pub const fn run_timeout(&self) -> Duration {
        self.run_timeout
    }

    /// Sets the policy.
    #[must_use]
    pub const fn with_policy(mut self, policy: MigrationPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// Sets the lock deadline.
    ///
    /// # Errors
    ///
    /// Refuses zero and anything above [`MAX_MIGRATION_TIMEOUT`]. An unbounded lock wait is
    /// exactly what this setting exists to prevent, so it cannot be expressed.
    pub fn with_lock_timeout(mut self, value: Duration) -> Result<Self, crate::DatabaseError> {
        self.lock_timeout = bounded(value)?;
        Ok(self)
    }

    /// Sets the whole-run deadline.
    ///
    /// # Errors
    ///
    /// Refuses zero and anything above [`MAX_MIGRATION_TIMEOUT`].
    pub fn with_run_timeout(mut self, value: Duration) -> Result<Self, crate::DatabaseError> {
        self.run_timeout = bounded(value)?;
        Ok(self)
    }
}

/// Refuses a zero or over-long duration.
fn bounded(value: Duration) -> Result<Duration, crate::DatabaseError> {
    if value.is_zero() || value > MAX_MIGRATION_TIMEOUT {
        return Err(crate::DatabaseError::new(
            crate::DatabaseErrorKind::Unclassified,
        ));
    }
    Ok(value)
}

/// What happened to one migration.
///
/// # These describe an observation, not an attribution
///
/// The distinction matters under concurrent startup and is stated rather than left to be
/// discovered. A runner reads the applied set before it starts and after it finishes; the lock is
/// taken and released **inside** that window, so which process performed a given apply is not
/// recoverable through the driver's public API.
///
/// Consequently, when several processes start at once, **more than one may report the same
/// migration as [`MigrationOutcome::Applied`]** — each is truthfully saying "this was not recorded
/// as applied when I began". The database still applied it exactly once, which is the property that
/// matters and the one the contract suite asserts against the bookkeeping table rather than against
/// the sum of the reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// It was **not recorded as applied when this run began**.
    ///
    /// For a single starter this means "this run applied it". See the type documentation for why
    /// that reading is not safe when several processes start together.
    Applied,
    /// It was already recorded as applied when this run began, and its checksum matched.
    AlreadyApplied,
}

impl MigrationOutcome {
    /// The stable name, for logs and for the CLI's JSON output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::AlreadyApplied => "already_applied",
        }
    }
}

/// One migration's result.
///
/// # Observability is a deliverable, not a side effect
///
/// `PLAN.md` §12 requires migrations to be *"ordered, checksummed, observable, and safe under
/// concurrent startup"*. SQLx's `Migrator::run` returns `()`. This type is the difference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationStep {
    version: i64,
    description: String,
    reversibility: Reversibility,
    outcome: MigrationOutcome,
    elapsed: Duration,
}

impl MigrationStep {
    /// Records a step.
    #[must_use]
    pub fn new(
        version: i64,
        description: impl Into<String>,
        reversibility: Reversibility,
        outcome: MigrationOutcome,
        elapsed: Duration,
    ) -> Self {
        Self {
            version,
            description: description.into(),
            reversibility,
            outcome,
            elapsed,
        }
    }

    /// The version, which is also the sort key that makes ordering deterministic.
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }

    /// The migration's description.
    ///
    /// # This is developer-authored, not caller-supplied
    ///
    /// A migration filename is written by the application team and committed. It is therefore safe
    /// to report, unlike anything reaching the process from a request.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Whether it declares a rollback.
    #[must_use]
    pub const fn reversibility(&self) -> Reversibility {
        self.reversibility
    }

    /// What happened.
    #[must_use]
    pub const fn outcome(&self) -> MigrationOutcome {
        self.outcome
    }

    /// How long it took.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

/// The result of a whole migration run.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MigrationReport {
    steps: Vec<MigrationStep>,
}

impl MigrationReport {
    /// An empty report.
    #[must_use]
    pub const fn new() -> Self {
        Self { steps: Vec::new() }
    }

    /// Records a step.
    pub fn push(&mut self, step: MigrationStep) {
        self.steps.push(step);
    }

    /// Every step, in the order they were considered.
    #[must_use]
    pub fn steps(&self) -> &[MigrationStep] {
        &self.steps
    }

    /// How many migrations this run actually applied.
    #[must_use]
    pub fn applied(&self) -> usize {
        self.steps
            .iter()
            .filter(|step| step.outcome() == MigrationOutcome::Applied)
            .count()
    }

    /// Whether the versions are strictly increasing.
    ///
    /// # An ordering assertion, not an ordering implementation
    ///
    /// The driver sorts. This checks that what came back **was** sorted, so a future driver change
    /// that broke the guarantee is caught by a test rather than by a production schema.
    #[must_use]
    pub fn is_ordered(&self) -> bool {
        self.steps.windows(2).all(|pair| {
            pair.first().is_some_and(|first| {
                pair.get(1)
                    .is_some_and(|second| first.version() < second.version())
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_migration_is_not_the_default() {
        assert_eq!(MigrationSettings::default().policy(), MigrationPolicy::Never);
        assert!(!MigrationPolicy::default().runs_on_boot());
    }

    #[test]
    fn the_lock_wait_is_bounded_and_an_unbounded_one_cannot_be_expressed() {
        let settings = MigrationSettings::default();
        assert!(!settings.lock_timeout().is_zero());
        assert!(settings.with_lock_timeout(Duration::ZERO).is_err());
        assert!(
            MigrationSettings::default()
                .with_lock_timeout(MAX_MIGRATION_TIMEOUT + Duration::from_secs(1))
                .is_err()
        );
    }

    #[test]
    fn an_irreversible_migration_reports_itself_as_such() {
        assert!(!Reversibility::Irreversible.is_reversible());
        assert!(Reversibility::Reversible.is_reversible());
    }

    #[test]
    fn a_report_counts_only_what_it_applied() {
        let mut report = MigrationReport::new();
        report.push(MigrationStep::new(
            1,
            "create posts",
            Reversibility::Reversible,
            MigrationOutcome::AlreadyApplied,
            Duration::from_millis(1),
        ));
        report.push(MigrationStep::new(
            2,
            "add index",
            Reversibility::Reversible,
            MigrationOutcome::Applied,
            Duration::from_millis(2),
        ));
        assert_eq!(report.applied(), 1);
        assert_eq!(report.steps().len(), 2);
        assert!(report.is_ordered());
    }

    #[test]
    fn an_out_of_order_report_is_detected() {
        let mut report = MigrationReport::new();
        for version in [2_i64, 1] {
            report.push(MigrationStep::new(
                version,
                "x",
                Reversibility::Reversible,
                MigrationOutcome::Applied,
                Duration::ZERO,
            ));
        }
        assert!(!report.is_ordered());
    }
}
