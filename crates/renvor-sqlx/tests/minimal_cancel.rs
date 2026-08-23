//! The smallest decisive question: after ONE cancelled in-flight query, can the pool still serve?
mod support;

#[cfg(feature = "db-mysql")]
#[tokio::test]
async fn mysql_one_cancellation() {
    run(support::MYSQL_URL, renvor_database::DatabaseKind::MySql).await;
}

#[cfg(feature = "db-postgres")]
#[tokio::test]
async fn postgres_one_cancellation() {
    run(support::POSTGRES_URL, renvor_database::DatabaseKind::Postgres).await;
}

#[cfg(any(feature = "db-postgres", feature = "db-mysql"))]
async fn run(variable: &str, kind: renvor_database::DatabaseKind) {
    use std::time::{Duration, Instant};

    use renvor_database::{Database, PoolSettings, UnitOfWork};
    use sqlx::AssertSqlSafe;

    let Some(dsn) = support::url(variable) else {
        return;
    };
    let settings = PoolSettings::default()
        .with_max_connections(2)
        .expect("bounded")
        .with_acquire_timeout(Duration::from_secs(3))
        .expect("bounded");

    macro_rules! body {
        ($connect:path) => {{
            let database = $connect(&dsn, &settings).await.expect("connects");
            for statement in [
                "DROP TABLE IF EXISTS rv_min",
                "CREATE TABLE rv_min (id BIGINT PRIMARY KEY)",
            ] {
                sqlx::query(AssertSqlSafe(statement.to_owned()))
                    .execute(database.pool())
                    .await
                    .expect("prepares");
            }
            let insert = format!("INSERT INTO rv_min (id) VALUES ({})", kind.placeholder(1));

            // ONE cancellation, aimed at the in-flight window.
            let _ = tokio::time::timeout(Duration::from_micros(200), async {
                let mut uow = database.begin().await.expect("begins");
                sqlx::query(AssertSqlSafe(insert.clone()))
                    .bind(1_i64)
                    .execute(&mut **uow.inner())
                    .await
                    .expect("inserts");
                tokio::time::sleep(Duration::from_secs(30)).await;
                uow.commit().await.expect("commits");
            })
            .await;

            // Can the pool serve its full capacity? One attempt, generously bounded, no loop.
            let started = Instant::now();
            let first = database.begin().await;
            let second = database.begin().await;
            let elapsed = started.elapsed();
            println!(
                "{kind:?}: first={:?} second={:?} elapsed={elapsed:?} status={:?}",
                first.as_ref().map(|_| "ok").map_err(|e| e.kind()),
                second.as_ref().map(|_| "ok").map_err(|e| e.kind()),
                database.connections()
            );
            if let Ok(uow) = first {
                let _ = uow.rollback().await;
            }
            if let Ok(uow) = second {
                let _ = uow.rollback().await;
            }
        }};
    }

    match kind {
        #[cfg(feature = "db-mysql")]
        renvor_database::DatabaseKind::MySql => body!(renvor_sqlx::connect_mysql),
        #[cfg(feature = "db-postgres")]
        renvor_database::DatabaseKind::Postgres => body!(renvor_sqlx::connect_postgres),
        _ => {}
    }
}
