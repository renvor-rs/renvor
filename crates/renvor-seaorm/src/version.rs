//! Compile guards against the upstream shapes this crate depends on.
//!
//! # Why guards rather than trusting the call sites
//!
//! Every function here is **never called and always type-checked**. A removal or a rename would
//! already be a compile error at the call site; what a guard adds is protection against a future
//! that quietly stopped being `Send`, and against a signature that changed shape.
//!
//! # What these guards do NOT cover, stated because the doc used to over-claim
//!
//! This module said the guards catch "a change that still *accepts* the same call while meaning
//! something different". A type check cannot do that, and a security review named the concrete
//! case: `run_direct`'s third parameter is `skip`, and upstream documents skipping as *"not
//! executing the SQL of the migrations, but marking them as applied"*. An upstream release that
//! inverted or repurposed that bool would type-check straight through
//! `compile_guard_postgres` — not a doc link, because the guards are compiled only when a driver
//! feature is on and the link would not resolve in a default build — and the end state would be an
//! empty schema whose
//! `_sqlx_migrations` table claims it is fully migrated, which no later run repairs.
//!
//! **What catches that is the behavioural suite, not this file.** Flipping the literal to `true`
//! at the one call site was mutation-tested: it is killed by six tests across both engines, the
//! clearest being `on_boot_applies_migrations_before_readiness_is_reported`, which fails with
//! *"readiness was reported against an un-migrated schema"* — because it counts rows the migration
//! was supposed to insert rather than trusting that the migration reported success.
//!
//! The guards are kept for the properties they do pin. The claim is narrowed to match.
//!
//! Phase 006 learned this the expensive way: the property the whole migration design rests on is
//! not "the function exists", it is "the future it returns can be boxed into a `Send` provider
//! future". Only the second is worth guarding, and only an explicit assertion states it.

/// Asserts a future is `Send` without running it.
#[cfg_attr(
    not(any(feature = "db-postgres", feature = "db-mysql")),
    allow(dead_code, reason = "every guard that calls it is feature-gated")
)]
fn assert_send<T: Send>(_: T) {}

/// Fails the build if `Migrator::run_direct` stops being the thing [`crate::migrate`] depends on.
///
/// `run_direct` is `#[doc(hidden)]`, so SQLx may change it in a **patch** release without breaking
/// its own semver promise. ADR-0018 records why the dependency is taken and what the fallback is;
/// this is the mechanism that makes a breaking change loud. `renvor-seaorm` needs its own copy
/// because it calls `run_direct` itself rather than reusing `renvor-sqlx`'s runner.
#[cfg(feature = "db-postgres")]
#[expect(dead_code, reason = "a compile guard is type-checked, never executed")]
fn compile_guard_postgres() {
    fn guard(migrator: &sqlx::migrate::Migrator, connection: &mut sqlx::PgConnection) {
        assert_send(migrator.run_direct(None, connection, false));
    }
    let _ = guard;
}

/// The MySQL half of [`compile_guard_postgres`].
#[cfg(feature = "db-mysql")]
#[expect(dead_code, reason = "a compile guard is type-checked, never executed")]
fn compile_guard_mysql() {
    fn guard(migrator: &sqlx::migrate::Migrator, connection: &mut sqlx::MySqlConnection) {
        assert_send(migrator.run_direct(None, connection, false));
    }
    let _ = guard;
}

/// Fails the build if this crate's `sea-query-sqlx` stops being the one SeaORM binds through.
///
/// # What would otherwise go wrong, silently
///
/// `sea_orm::Statement::values` is a `sea_query::Values`, and `SqlxValues` is the
/// `sqlx::Arguments` implementation that binds it. If Cargo ever resolved **two** `sea-query`
/// versions — this crate's `sea-query-sqlx` against one, SeaORM against another — the two
/// `Values` types would be distinct types with the same name, and the error message at the bind
/// site would be the notoriously unhelpful *"expected `Values`, found `Values`"*.
///
/// This guard states the requirement in one place, so the failure names the coupling rather than
/// a line of query-building code.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
#[expect(dead_code, reason = "a compile guard is type-checked, never executed")]
fn compile_guard_value_binding() {
    fn guard(values: sea_orm::sea_query::Values) -> sea_query_sqlx::SqlxValues {
        // The identity that must hold: SeaORM's `Values` IS `sea-query-sqlx`'s input.
        sea_query_sqlx::SqlxValues(values)
    }
    let _ = guard;
}

/// Fails the build if SeaORM's `ConnectionTrait` stops producing `Send` futures.
///
/// # Why this specific property
///
/// `SeaOrmProvider` boxes its work into `renvor_core`'s `ProviderFuture`, which is
/// `Pin<Box<dyn Future + Send>>`. `ConnectionTrait` is declared with `#[async_trait::async_trait]`
/// today, which boxes with a `Send` bound — so the property holds by construction. If SeaORM
/// migrated the trait to native `async fn` in trait, it would **stop** holding, and the failure
/// would appear as an unrelated lifetime error deep in the provider. Phase 006's L-6 was exactly
/// that class of error, misdiagnosed for a whole phase.
/// # Both, because a one-driver build must be guarded too
///
/// This guard was written once, under `#[cfg(feature = "db-postgres")]` — which is the **exact**
/// mistake `renvor-sqlx`'s migration guard already records having made and fixed: *"in a
/// `db-mysql`-only build the guard was not compiled at all, so the promise that it 'fails the build
/// the moment its shape changes' held for exactly one of the two configurations this crate
/// supports."* Reproducing a documented past defect is worse than making a new one, so both halves
/// exist here and the pair is what a review caught.
///
/// Nothing else in this crate forces the proof for MySQL: `SeaOrmProvider::initialise` reaches the
/// database through `sqlx::Executor` directly, never through `ConnectionTrait`.
#[cfg(feature = "db-postgres")]
#[expect(dead_code, reason = "a compile guard is type-checked, never executed")]
fn compile_guard_connection_is_send_postgres() {
    fn guard(connection: &crate::SeaOrmConnection<sqlx::Postgres>) {
        use sea_orm::ConnectionTrait as _;
        assert_send(connection.execute_unprepared("SELECT 1"));
        assert_send(connection.query_all_raw(sea_orm::Statement::from_string(
            sea_orm::DbBackend::Postgres,
            "SELECT 1",
        )));
    }
    let _ = guard;
}

/// The MySQL half of [`compile_guard_connection_is_send_postgres`].
#[cfg(feature = "db-mysql")]
#[expect(dead_code, reason = "a compile guard is type-checked, never executed")]
fn compile_guard_connection_is_send_mysql() {
    fn guard(connection: &crate::SeaOrmConnection<sqlx::MySql>) {
        use sea_orm::ConnectionTrait as _;
        assert_send(connection.execute_unprepared("SELECT 1"));
        assert_send(connection.query_all_raw(sea_orm::Statement::from_string(
            sea_orm::DbBackend::MySql,
            "SELECT 1",
        )));
    }
    let _ = guard;
}
