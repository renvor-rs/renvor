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

use renvor_database::{Database, DatabaseError, DatabaseErrorKind, SeedScope, UnitOfWork};

use crate::SqlxDatabase;

/// The ledger of seeds that have already run.
///
/// `VARCHAR(191)` rather than `VARCHAR(255)`: MySQL's older `utf8mb4` index limit is 767 bytes, and
/// 191 four-byte characters is the largest key that fits everywhere without a per-engine branch.
/// The seed set, its report, the ledger statements, and the dry description.
///
/// These moved to `renvor-database` in Phase 007 so the SeaORM adapter could reach them — see that
/// module. Re-exported here so every existing path keeps working.
pub use renvor_database::seed::{APPLIED, CREATE_LEDGER, SeedReport, SqlSeed, describe};

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
        let name = seed.declaration().name().to_owned();

        if !seed.declaration().permits(scope) {
            report.record_skipped_out_of_scope(name);
            continue;
        }
        let ran_before = already.iter().any(|applied| applied == &name);
        if ran_before && !seed.declaration().idempotence().permits_repeat() {
            report.record_skipped_already_applied(name);
            continue;
        }

        let mut uow = database.begin().await?;
        for statement in seed.statements() {
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
        report.record_applied(name);
    }

    Ok(report)
}
