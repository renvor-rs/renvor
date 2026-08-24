//! Applying seed data through the SeaORM adapter.
//!
//! # What is shared and what is not
//!
//! [`renvor_database::SqlSeed`], [`renvor_database::SeedReport`], the ledger statements and
//! [`renvor_database::seed::describe`] live in `renvor-database` and are used by **both** adapters.
//! Only the loop that executes statements is here, because only it names a connection.
//!
//! That split is what makes FR-033's parity claim checkable: the declaration types, the scope and
//! idempotence rules, the ledger schema and the report shape are one implementation, and the two
//! runners are compared by the contract suite rather than by inspection.
//!
//! Phase 007 moved the shared half out of `renvor-sqlx`. A review found FR-033 resting on *"shared
//! types; no adapter-specific behaviour exists"* while those types sat inside the other adapter,
//! where this one could not reach them.

use renvor_database::{
    APPLIED, CREATE_LEDGER, DatabaseError, DatabaseErrorKind, SeedReport, SeedScope, SqlSeed,
};

/// Generates the per-driver seed runner.
///
/// A macro for the reason `migrate.rs` records: the work goes through `Database::begin`, whose
/// implementation is per-driver, and through `ConnectionTrait`, which is implemented per-driver.
macro_rules! runner {
    ($name:ident, $driver:ty, $feature:literal, $insert:literal) => {
        /// Applies every seed the scope permits.
        ///
        /// # Determinism
        ///
        /// Seeds run in the order given, each inside its own transaction, and the report lists
        /// exactly what happened. Nothing is concurrent and nothing depends on filesystem order,
        /// so the same input produces the same report every time.
        ///
        /// # Each seed is its own transaction
        ///
        /// So that a failing seed leaves the ones before it applied and the ones after it
        /// untouched, rather than rolling back work that succeeded. The ledger row is written in
        /// the **same** transaction as the seed's statements, so a seed can never be recorded as
        /// applied without having been.
        ///
        /// # Errors
        ///
        /// [`DatabaseErrorKind::StatementRejected`] if a seed's SQL fails, after which no later
        /// seed runs.
        #[cfg(feature = $feature)]
        pub async fn $name(
            database: &crate::SeaOrmDatabase<$driver>,
            scope: SeedScope,
            seeds: &[SqlSeed],
        ) -> Result<SeedReport, DatabaseError> {
            use renvor_database::{Database as _, UnitOfWork as _};
            use sea_orm::ConnectionTrait as _;

            sqlx::query(sqlx::AssertSqlSafe(CREATE_LEDGER.to_owned()))
                .execute(database.pool())
                .await
                .map_err(|error| crate::error::classify_error(&error))?;

            let already: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(APPLIED.to_owned()))
                .fetch_all(database.pool())
                .await
                .map_err(|error| crate::error::classify_error(&error))?;

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

                let unit = database.begin().await?;
                for statement in seed.statements() {
                    // A seed's statements are operator-authored SQL, exactly as in the direct-SQLx
                    // runner. `execute_unprepared` is the right rung here and nowhere else: there
                    // is no caller-supplied VALUE in a seed, only author-supplied text.
                    unit.execute_unprepared(statement).await.map_err(|error| {
                        let _ = crate::error::classify_db_error(&error);
                        DatabaseError::new(DatabaseErrorKind::StatementRejected)
                    })?;
                }
                // Recorded in the SAME transaction, and only when it was not recorded before. The
                // name is a BOUND value, never interpolated — this is the one place in a seed run
                // where a value crosses into a statement.
                if !ran_before {
                    let record = sea_orm::Statement::from_sql_and_values(
                        unit.get_database_backend(),
                        $insert,
                        [name.as_str().into()],
                    );
                    unit.execute_raw(record).await.map_err(|error| {
                        let _ = crate::error::classify_db_error(&error);
                        DatabaseError::new(DatabaseErrorKind::StatementRejected)
                    })?;
                }
                unit.commit().await?;
                report.record_applied(name);
            }

            Ok(report)
        }
    };
}

// THE PLACEHOLDER IS PER-DRIVER, and it has to be a macro argument rather than a `const`.
//
// It was a single `#[cfg]`-selected constant, and with BOTH features enabled the PostgreSQL arm
// won — so the MySQL runner issued `$1`, which MySQL rejects. `from_sql_and_values` passes SQL
// through to the driver and rewrites nothing.
//
// The four-row matrix caught it, for the SECOND time in this phase: the contract fixture's own
// insert had the identical defect, passing on MySQL and failing on PostgreSQL. Two independent
// occurrences of one mistake is the argument for running every row rather than one of each.
runner!(
    run_postgres,
    sqlx::Postgres,
    "db-postgres",
    "INSERT INTO _renvor_seeds (name) VALUES ($1)"
);
runner!(
    run_mysql,
    sqlx::MySql,
    "db-mysql",
    "INSERT INTO _renvor_seeds (name) VALUES (?)"
);
