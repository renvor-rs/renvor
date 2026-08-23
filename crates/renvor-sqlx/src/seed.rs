//! Running declared seeds against a real database.
//!
//! # Production is not a scope, so "do not seed production" is not a rule anyone can forget
//!
//! FR-033. [`SeedScope`] has two variants, `Development` and `Test`, and no third. There is no
//! flag to set wrongly and no environment string to mistype: a production seed is **unrepresentable**
//! rather than refused. That is the same construction `DatabaseError` uses to guarantee it carries
//! no credential — make the bad state impossible to build, rather than checking for it.
//!
//! # Idempotence is declared, never inferred
//!
//! FR-035. A seed that may run twice and a seed that must run once are different things, and
//! nothing here guesses which it has. [`renvor_database::Idempotence::RunOnce`] seeds are recorded in a ledger table
//! and skipped on a later run; [`renvor_database::Idempotence::Idempotent`] seeds always run, because the author
//! said they are safe to.

use renvor_database::{
    Database, DatabaseError, DatabaseErrorKind, SeedDeclaration, SeedScope, UnitOfWork,
};

use crate::SqlxDatabase;

/// The ledger of seeds that have already run.
///
/// `VARCHAR(191)` rather than `VARCHAR(255)`: MySQL's older `utf8mb4` index limit is 767 bytes, and
/// 191 four-byte characters is the largest key that fits everywhere without a per-engine branch.
const CREATE_LEDGER: &str =
    "CREATE TABLE IF NOT EXISTS _renvor_seeds (name VARCHAR(191) NOT NULL PRIMARY KEY)";

/// Every seed already applied, in a deterministic order.
const APPLIED: &str = "SELECT name FROM _renvor_seeds ORDER BY name";

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

    /// Seeds skipped because they declare [`renvor_database::Idempotence::RunOnce`] and had already run.
    #[must_use]
    pub fn skipped_already_applied(&self) -> &[String] {
        &self.skipped_already_applied
    }

    /// Whether anything at all was applied.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.applied.is_empty()
    }
}

/// Applies every seed the scope permits.
///
/// # Determinism
///
/// FR-034. Seeds run in the order given, each inside its own transaction, and the report lists
/// exactly what happened. Nothing is concurrent and nothing depends on filesystem order, so the
/// same input produces the same report every time.
///
/// # Each seed is its own transaction
///
/// So that a failing seed leaves the ones before it applied and the ones after it untouched, rather
/// than rolling back work that succeeded. The ledger row is written in the **same** transaction as
/// the seed's statements, so a seed can never be recorded as applied without having been.
///
/// # Errors
///
/// [`DatabaseErrorKind::StatementRejected`] if a seed's SQL fails, after which no later seed runs.
pub async fn run<DB>(
    database: &SqlxDatabase<DB>,
    scope: SeedScope,
    seeds: &[SqlSeed],
) -> Result<SeedReport, DatabaseError>
where
    DB: sqlx::Database,
    for<'p> &'p sqlx::Pool<DB>: sqlx::Executor<'p, Database = DB>,
    for<'e> &'e mut DB::Connection: sqlx::Executor<'e, Database = DB>,
    <DB as sqlx::Database>::Arguments: sqlx::IntoArguments<DB>,
    String: for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    for<'r> &'r str: sqlx::Encode<'r, DB> + sqlx::Type<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
{
    sqlx::query(CREATE_LEDGER)
        .execute(database.pool())
        .await
        .map_err(|error| crate::error::classify_error(&error))?;

    let already: Vec<String> = sqlx::query_scalar(APPLIED)
        .fetch_all(database.pool())
        .await
        .map_err(|error| crate::error::classify_error(&error))?;

    let record = format!(
        "INSERT INTO _renvor_seeds (name) VALUES ({})",
        database.kind().placeholder(1)
    );

    let mut report = SeedReport::default();
    for seed in seeds {
        let name = seed.declaration.name().to_owned();

        if !seed.declaration.permits(scope) {
            report.skipped_out_of_scope.push(name);
            continue;
        }
        let ran_before = already.iter().any(|applied| applied == &name);
        if ran_before && !seed.declaration.idempotence().permits_repeat() {
            report.skipped_already_applied.push(name);
            continue;
        }

        let mut uow = database.begin().await?;
        for statement in &seed.statements {
            sqlx::query(sqlx::AssertSqlSafe(statement.clone()))
                .execute(&mut **uow.inner())
                .await
                .map_err(|error| {
                    let _ = crate::error::classify_error(&error);
                    DatabaseError::new(DatabaseErrorKind::StatementRejected)
                })?;
        }
        // Recorded in the SAME transaction, and only when it was not recorded before. An
        // idempotent seed that has run already keeps its single ledger row rather than colliding
        // with it.
        if !ran_before {
            sqlx::query(sqlx::AssertSqlSafe(record.clone()))
                .bind(name.as_str())
                .execute(&mut **uow.inner())
                .await
                .map_err(|error| {
                    let _ = crate::error::classify_error(&error);
                    DatabaseError::new(DatabaseErrorKind::StatementRejected)
                })?;
        }
        uow.commit().await?;
        report.applied.push(name);
    }

    Ok(report)
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
mod tests {
    use super::*;
    use renvor_database::Idempotence;

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
