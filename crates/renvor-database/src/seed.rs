//! Seed scopes and idempotence declarations.
//!
//! # Seeds are not part of boot
//!
//! Running a seed is an explicit call. A framework that seeded during startup would write rows on
//! every deployment, and "the seed is idempotent" is a claim about a script that somebody may edit
//! next week.

/// Where a seed may run.
///
/// # `Production` is deliberately absent
///
/// It is not that production seeding is forbidden in general — an application may write its own
/// data-loading command. It is that **Renvor's** seed mechanism must not offer a value that makes
/// writing rows into production a matter of setting an enum. `PLAN.md` §12 requires opt-in
/// behaviour rather than hidden global behaviour, and an absent variant is the strongest opt-out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SeedScope {
    /// A developer's local database.
    Development,
    /// A test database, typically created and dropped per run.
    Test,
}

impl SeedScope {
    /// Every scope.
    pub const ALL: [Self; 2] = [Self::Development, Self::Test];

    /// The stable name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Test => "test",
        }
    }

    /// Parses a scope name.
    ///
    /// Returns `None` for anything else — including `"production"`, which is refused **by name**
    /// rather than by falling through to a default.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "development" => Some(Self::Development),
            "test" => Some(Self::Test),
            _ => None,
        }
    }
}

/// Whether running a seed twice is defined.
///
/// # Declared, not assumed
///
/// A seed that inserts rows is not idempotent, and running it twice doubles the data. Declaring the
/// property lets a test assert it — `PLAN.md` requires the idempotence policy to be *recorded*,
/// which means a value in the program rather than a sentence in a README.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Idempotence {
    /// Running it again leaves the database in the same state.
    Idempotent,
    /// Running it again is not defined, and the runner refuses a second run.
    RunOnce,
}

impl Idempotence {
    /// Whether a repeat run is permitted.
    #[must_use]
    pub const fn permits_repeat(self) -> bool {
        matches!(self, Self::Idempotent)
    }
}

/// A seed's declaration: what it is called, where it may run, and whether it repeats.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeedDeclaration {
    name: String,
    scope: SeedScope,
    idempotence: Idempotence,
}

impl SeedDeclaration {
    /// Declares a seed.
    #[must_use]
    pub fn new(name: impl Into<String>, scope: SeedScope, idempotence: Idempotence) -> Self {
        Self {
            name: name.into(),
            scope,
            idempotence,
        }
    }

    /// The seed's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Where it may run.
    #[must_use]
    pub const fn scope(&self) -> SeedScope {
        self.scope
    }

    /// Whether it may run twice.
    #[must_use]
    pub const fn idempotence(&self) -> Idempotence {
        self.idempotence
    }

    /// Whether this seed may run in `scope`.
    ///
    /// Deny-by-default: a seed runs only in the scope it declared.
    #[must_use]
    pub const fn permits(&self, scope: SeedScope) -> bool {
        matches!(
            (self.scope, scope),
            (SeedScope::Development, SeedScope::Development) | (SeedScope::Test, SeedScope::Test)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_is_not_a_scope_a_seed_can_declare() {
        assert_eq!(SeedScope::parse("production"), None);
        assert_eq!(SeedScope::parse("prod"), None);
        assert_eq!(SeedScope::ALL.len(), 2);
    }

    #[test]
    fn a_seed_runs_only_in_the_scope_it_declared() {
        let seed = SeedDeclaration::new("posts", SeedScope::Test, Idempotence::Idempotent);
        assert!(seed.permits(SeedScope::Test));
        assert!(!seed.permits(SeedScope::Development));
    }

    #[test]
    fn a_run_once_seed_refuses_a_repeat() {
        assert!(!Idempotence::RunOnce.permits_repeat());
        assert!(Idempotence::Idempotent.permits_repeat());
    }

    #[test]
    fn scope_names_round_trip() {
        for scope in SeedScope::ALL {
            assert_eq!(SeedScope::parse(scope.as_str()), Some(scope));
        }
    }
}

// ── The seed set, its report, and the ledger they share ────────────────────────────────
//
// MOVED HERE FROM `renvor-sqlx` IN PHASE 007. None of it names a driver: a seed is a declaration
// plus SQL text, a report is three lists of names, and the ledger is two constant statements. Only
// the RUNNER is driver-specific, and each adapter keeps its own.
//
// Phase 007's FR-033 requires SeaORM seeding to behave identically to the SQLx row. A review found
// that claim resting on "shared types; no adapter-specific behaviour exists" while `SqlSeed`,
// `SeedReport` and the ledger lived inside the SQLx adapter, where the other one could not reach
// them. Moving them makes the shared half genuinely shared; what remains per-adapter is the loop
// that executes statements, and that is compared by the contract suite rather than by assertion.

/// Creates the seed ledger. Public because each adapter's runner issues it.
pub const CREATE_LEDGER: &str =
    "CREATE TABLE IF NOT EXISTS _renvor_seeds (name VARCHAR(191) NOT NULL PRIMARY KEY)";

/// Every seed already applied, in a deterministic order.
/// Reads the applied seed names. Public for the same reason.
pub const APPLIED: &str = "SELECT name FROM _renvor_seeds ORDER BY name";

/// One seed: what it is, and the statements that apply it.
///
/// The statements are owned `String`s rather than `&'static str` because a seed is normally read
/// from a file at startup. They are executed **verbatim** and are the author's own SQL — this is
/// not a place a request value ever reaches, which is why there is no parameter list.
#[derive(Clone, Debug)]
pub struct SqlSeed {
    declaration: SeedDeclaration,
    statements: Vec<String>,
}

impl SqlSeed {
    /// Declares a seed and the statements that apply it.
    #[must_use]
    pub fn new(declaration: SeedDeclaration, statements: Vec<String>) -> Self {
        Self {
            declaration,
            statements,
        }
    }

    /// What this seed declares about itself.
    #[must_use]
    pub const fn declaration(&self) -> &SeedDeclaration {
        &self.declaration
    }

    /// The statements that apply it, in order.
    ///
    /// Public since Phase 007, because the runner that executes them now lives in a different
    /// crate — one per adapter. The order is the declared one; nothing here reorders.
    #[must_use]
    pub fn statements(&self) -> &[String] {
        &self.statements
    }
}

/// What a seed run did, per seed.
///
/// Both lists are reported. A run that named only what it applied would make "skipped because it
/// had already run" and "skipped because its scope did not permit it" indistinguishable, and those
/// call for different corrections.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeedReport {
    applied: Vec<String>,
    skipped_out_of_scope: Vec<String>,
    skipped_already_applied: Vec<String>,
}

impl SeedReport {
    /// Seeds this run applied, in the order they were applied.
    #[must_use]
    pub fn applied(&self) -> &[String] {
        &self.applied
    }

    /// Seeds skipped because the requested scope does not permit them.
    #[must_use]
    pub fn skipped_out_of_scope(&self) -> &[String] {
        &self.skipped_out_of_scope
    }

    /// Seeds skipped because they declare [`Idempotence::RunOnce`] and had already run.
    #[must_use]
    pub fn skipped_already_applied(&self) -> &[String] {
        &self.skipped_already_applied
    }

    /// Whether anything at all was applied.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.applied.is_empty()
    }

    /// Records a seed as applied.
    ///
    /// # Why the runners record through methods rather than public fields
    ///
    /// The runners live in the adapter crates since Phase 007, so the fields would otherwise have
    /// to be `pub` — and a public field can be set to anything by anyone, including to a state the
    /// three lists disagree about. Three named methods are the whole write surface, and each one
    /// says which outcome it is recording.
    pub fn record_applied(&mut self, name: impl Into<String>) {
        self.applied.push(name.into());
    }

    /// Records a seed skipped because the requested scope does not permit it.
    pub fn record_skipped_out_of_scope(&mut self, name: impl Into<String>) {
        self.skipped_out_of_scope.push(name.into());
    }

    /// Records a seed skipped because it declares `RunOnce` and had already run.
    pub fn record_skipped_already_applied(&mut self, name: impl Into<String>) {
        self.skipped_already_applied.push(name.into());
    }
}

/// The declarations a seed set contains, for a caller that wants to show them before running.
///
/// # Why a dry description exists at all
///
/// So that "what would this do" can be answered without doing it — the same reason `--dry-run`
/// exists in the CLI. A caller that had to run seeds to find out which ones apply has no way to
/// check before touching data.
#[must_use]
pub fn describe(scope: SeedScope, seeds: &[SqlSeed]) -> Vec<(String, bool)> {
    seeds
        .iter()
        .map(|seed| {
            (
                seed.declaration.name().to_owned(),
                seed.declaration.permits(scope),
            )
        })
        .collect()
}

#[cfg(test)]
mod seed_set_tests {
    use super::*;
    use crate::Idempotence;

    fn seed(name: &str, scope: SeedScope, idempotence: Idempotence) -> SqlSeed {
        SqlSeed::new(
            SeedDeclaration::new(name, scope, idempotence),
            vec!["SELECT 1".to_owned()],
        )
    }

    /// FR-033, asserted as a property of the type rather than of a check.
    #[test]
    fn there_is_no_production_scope_to_seed() {
        assert_eq!(SeedScope::ALL.len(), 2);
        assert!(SeedScope::parse("production").is_none());
        assert!(SeedScope::parse("prod").is_none());
    }

    #[test]
    fn describe_answers_without_running_anything() {
        let seeds = [
            seed("dev-only", SeedScope::Development, Idempotence::RunOnce),
            seed("test-only", SeedScope::Test, Idempotence::RunOnce),
        ];
        let described = describe(SeedScope::Test, &seeds);
        assert_eq!(described.len(), 2);
        assert_eq!(described[0], ("dev-only".to_owned(), false));
        assert_eq!(described[1], ("test-only".to_owned(), true));
    }

    #[test]
    fn an_empty_report_says_nothing_was_applied() {
        assert!(SeedReport::default().is_empty());
    }

    /// FR-035: the two idempotence declarations mean different things.
    #[test]
    fn idempotence_is_declared_rather_than_assumed() {
        assert!(Idempotence::Idempotent.permits_repeat());
        assert!(!Idempotence::RunOnce.permits_repeat());
    }
}
