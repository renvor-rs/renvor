//! Shared harness for the real-database contract suite.
//!
//! # A skipped test is never reported as a gate
//!
//! These tests need a real PostgreSQL and a real MySQL. When the environment does not provide one
//! they **skip with a printed message** naming the variable that would enable them. `PLAN.md` §17
//! forbids describing an ignored test as a gate, so the phase evidence names the command that ran
//! them rather than the command that could have.

#![allow(dead_code)]

use std::time::Duration;

use renvor_database::{ConnectionString, PoolSettings};

/// The variable naming a PostgreSQL instance.
pub const POSTGRES_URL: &str = "RENVOR_TEST_POSTGRES_URL";
/// The variable naming a MySQL instance.
pub const MYSQL_URL: &str = "RENVOR_TEST_MYSQL_URL";

/// The variable that turns a skip into a failure.
///
/// See [`url`]. Set it wherever a database is supposed to be present.
pub const REQUIRE_DATABASE: &str = "RENVOR_TEST_REQUIRE_DATABASE";

/// Reads a database URL, or explains why the test is being skipped.
///
/// Returns `None` after printing, so the caller returns rather than failing.
///
/// # A skipped run must not be able to look like a passing one
///
/// This printed a line and returned `None`, and that was the whole mechanism. It is not enough,
/// and the gap was real rather than theoretical: **no automated gate set either URL**, `libtest`
/// swallows `println!` for a passing test unless `--nocapture` is given, and nothing in this
/// repository passes it. So every real-database test — the entire migration-on-boot suite
/// included — reported `ok` in CI having connected to nothing, and a revert of the code they guard
/// would have gone green.
///
/// `xtask` already carries a paragraph about this exact failure mode being found in the relay
/// test. The remedy there was to make the gate run the thing; the remedy here is to make a
/// **missing** database a failure wherever one is supposed to exist.
///
/// So: when [`REQUIRE_DATABASE`] is set, a missing URL **panics**. CI sets it. A contributor
/// without a local server does not, and still gets the skip. The difference between "there is no
/// database here" and "there was supposed to be a database here" is now expressed rather than
/// assumed.
pub fn url(name: &str) -> Option<ConnectionString> {
    match std::env::var(name) {
        Ok(value) if !value.is_empty() => Some(ConnectionString::new(value)),
        _ => {
            assert!(
                std::env::var(REQUIRE_DATABASE).is_err(),
                "{REQUIRE_DATABASE} is set, so a database was expected and `{name}` is empty or \
                 absent. This is a FAILURE rather than a skip on purpose: a run that silently \
                 skipped every real-database test would report the same `ok` as one that passed \
                 them"
            );
            println!("SKIPPED: set {name} to run this test against a real database");
            None
        }
    }
}

/// Small, fast, bounded settings for tests.
///
/// The pool is deliberately tiny so that exhaustion is reachable in a test rather than theoretical.
pub fn settings() -> PoolSettings {
    PoolSettings::default()
        .with_max_connections(4)
        .expect("bounded")
        .with_acquire_timeout(Duration::from_secs(5))
        .expect("bounded")
        .with_connect_timeout(Duration::from_secs(5))
        .expect("bounded")
}

/// A recognisable value that must never appear in output.
///
/// The same construction Phase 005 used for its redaction proofs: assert the **absence** of a
/// string chosen to be unmistakable, rather than the presence of a redaction marker.
pub const CREDENTIAL_CANARY: &str = "hunter2CanaryDoNotLeak";

/// The same server, reached with a password it will refuse.
///
/// # Why a server-side refusal is a different test from an unreachable port
///
/// Port 1 fails in the kernel: no database ever sees the attempt. That proves the diagnostic is
/// produced when nothing answers — it proves nothing about the path taken when a server *does*
/// answer and says no, which is the path [`renvor_sqlx::error::classify_connect_error`] exists for
/// and the one an operator with a wrong credential actually hits.
///
/// The refused password is [`CREDENTIAL_CANARY`], so the value the server rejected is the same
/// value the diagnostic must not repeat.
///
/// The DSN is split rather than parsed with a URL crate on purpose: this is a test helper in a
/// dependency-sensitive workspace, and the shape it accepts is the shape CI writes.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
pub fn with_rejected_password(dsn: &ConnectionString) -> ConnectionString {
    let raw = dsn.expose();
    let (scheme, rest) = raw.split_once("://").expect("a test DSN names a scheme");
    let (userinfo, host) = rest
        .split_once('@')
        .expect("a test DSN carries credentials before the host");
    let user = userinfo.split_once(':').map_or(userinfo, |(user, _)| user);
    ConnectionString::new(format!("{scheme}://{user}:{CREDENTIAL_CANARY}@{host}"))
}

/// Drives one provider's `initialise` with a throwaway kernel context.
///
/// A helper rather than six copies: building an `InitContext` needs five kernel values that have
/// nothing to do with what these tests assert, and repeating that in every test would bury the one
/// line that matters.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
pub async fn initialise<P: renvor_core::provider::registry::Provider>(
    provider: &P,
) -> Result<(), renvor_core::error::BoxedCause> {
    use renvor_core::provider::registry::{InitContext, ProviderId};

    let id = ProviderId::new("test");
    let mut state = renvor_core::state::TypedStateMap::new();
    let cancel = renvor_core::cancel::CancelScope::root();
    let work = renvor_core::lifecycle::WorkGate::new();
    let health = renvor_core::HealthState::new();
    let run_id =
        renvor_core::observe::RunIdentifier::generate(&renvor_core::observe::OsEntropy::new())
            .expect("the OS CSPRNG is available");
    let mut context = InitContext::new(&id, &mut state, &cancel, &work, &health, run_id);
    provider.initialise(&mut context).await
}

/// How many sessions the server currently holds against this database.
///
/// # Why the server is asked rather than the pool
///
/// A pool that believes it is closed has only set a flag. Asking the **server** how many sessions
/// it is still holding proves the sockets are gone, which is the property a leaked migration
/// connection would violate. The count includes this observer's own connection, so callers compare
/// against a baseline rather than against zero.
///
/// # It FAILS rather than returning zero, and that is a correction
///
/// This ended in `.unwrap_or(0)`, justified as avoiding a false leak report inside a polling loop.
/// The cost it did not mention is the one that matters: callers take a `baseline` with this same
/// helper and then loop until `now <= baseline`. If the view became unreadable — a restricted
/// `pg_stat_activity`, a missing MySQL `PROCESS` privilege, a driver change — **both** values are
/// zero, `0 <= 0` breaks on the first iteration, and the leak assertion never runs. The test
/// reports `ok` having checked nothing, which is exactly the failure it exists to catch.
///
/// A test that cannot read the session view should say so loudly.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
pub async fn sessions<DB>(pool: &sqlx::Pool<DB>) -> i64
where
    DB: sqlx::Database,
    for<'a> &'a sqlx::Pool<DB>: sqlx::Executor<'a, Database = DB>,
    i64: sqlx::Type<DB> + for<'a> sqlx::Decode<'a, DB>,
    <DB as sqlx::Database>::Arguments: sqlx::IntoArguments<DB>,
    usize: sqlx::ColumnIndex<<DB as sqlx::Database>::Row>,
{
    // The two engines expose the same fact under different names. Selected on the driver's own
    // constant rather than on a feature flag, because both features can be on at once.
    let sql = if DB::NAME.to_ascii_lowercase().contains("mysql") {
        "SELECT count(*) FROM information_schema.processlist WHERE db = database()"
    } else {
        "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database()"
    };
    sqlx::query_scalar(sqlx::AssertSqlSafe(sql.to_owned()))
        .fetch_one(pool)
        .await
        .expect("the server's session view must be readable for a leak assertion to mean anything")
}

/// The database the migration-on-boot suite uses, created if it is not there.
///
/// # Why this suite does not share `renvor_test`
///
/// It drops `_sqlx_migrations` in order to start from nothing, and `tests/migration.rs` uses that
/// same table. Cargo runs test **binaries** in parallel — `--test-threads=1` only serialises within
/// one — so the two would destroy each other's bookkeeping on any ordinary `cargo test`. Giving
/// this suite its own database removes the interference rather than relying on the order two
/// processes happen to interleave in.
///
/// It also gives this suite its own migration lock: both engines derive the advisory-lock key from
/// the database name, so a separate database is a separate lock, and the concurrency test here
/// cannot block or be blocked by an unrelated suite.
///
/// Returns `None` after printing, exactly as [`url`] does, when no server is configured.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
pub async fn isolated_url<DB>(variable: &str, database: &str) -> Option<ConnectionString>
where
    DB: sqlx::Database,
    for<'a> &'a sqlx::Pool<DB>: sqlx::Executor<'a, Database = DB>,
    <DB as sqlx::Database>::Arguments: sqlx::IntoArguments<DB>,
{
    let admin = url(variable)?;

    // Connected through the CONFIGURED database, which is the one the operator proved reachable.
    // `CREATE DATABASE` cannot run inside a transaction on PostgreSQL; `Executor::execute` on a
    // pooled connection is autocommit, so this is the right shape for both engines.
    let pool = sqlx::Pool::<DB>::connect(admin.expose())
        .await
        .expect("the configured database is reachable");

    // "already exists" is the expected outcome on every run after the first, and PostgreSQL has no
    // `CREATE DATABASE IF NOT EXISTS`. The result is discarded rather than matched on a SQLSTATE:
    // if creation genuinely failed, the connect below fails with a message about THIS database
    // rather than one about a statement.
    let statement = format!("CREATE DATABASE {database}");
    let _ = sqlx::Executor::execute(&pool, sqlx::AssertSqlSafe(statement)).await;
    pool.close().await;

    // The DSN's path is the database name. Replacing the final segment is the whole edit, and it
    // is done on the exposed value inside this function so the rewritten string goes straight back
    // into a `ConnectionString` without being logged or returned as a bare `String`.
    let exposed = admin.expose();
    let (prefix, _) = exposed.rsplit_once('/')?;
    Some(ConnectionString::new(format!("{prefix}/{database}")))
}

/// Serialises the tests that share one fixture table.
///
/// # A gate that only passes under a flag is not a gate
///
/// `tests/contract.rs` gives every test the same `rv_post`, dropped and recreated by `fresh()`. Two
/// of them running at once therefore delete each other's rows, and the suite passed only under
/// `--test-threads=1` — with nothing in the code saying so. An ordinary `cargo test` failed with
/// fourteen unrelated-looking assertion errors, which is precisely the shape of failure that gets
/// dismissed as flakiness.
///
/// Holding this across each test makes the requirement structural. It costs the suite its
/// intra-binary parallelism, which it never actually had.
///
/// # It is deliberately NOT a `std::sync::Mutex`
///
/// These tests `.await` while holding the guard. A blocking mutex held across an await point
/// parks a runtime worker, and with a multi-threaded runtime and enough tests that deadlocks. This
/// is a Tokio mutex for the same reason the rest of the crate bounds its waits.
///
/// It is process-wide, so it does nothing about two test **binaries** racing. That is solved
/// separately and differently — see [`isolated_url`], which gives a suite its own database.
#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
pub static SHARED_FIXTURE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Sixteen bytes from the operating system's CSPRNG, for a test-created identity.
///
/// Through `renvor-core`'s entropy port rather than a counter: two rows created in one suite run
/// must differ, and a counter shared across `#[tokio::test]`s that a future change ran in parallel
/// would silently start colliding on the primary key.
pub fn rand16() -> [u8; 16] {
    use renvor_core::observe::entropy::{EntropySource as _, OsEntropy};
    let mut bytes = [0_u8; 16];
    OsEntropy::new().fill(&mut bytes).expect("system entropy");
    bytes
}
