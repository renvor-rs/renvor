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
