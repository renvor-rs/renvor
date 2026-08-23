//! Keyset pagination against real databases, and the same boundaries from both.
//!
//! FR-037 and FR-041. The cursor is C-15's [`renvor_database::Keyset`] — there is no second,
//! storage-specific format — and the two engines must agree on where each page ends. Agreement is
//! asserted by paging both and comparing, not by reading two query builders and concluding they
//! look similar.

mod support;

#[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
use renvor_database::{Direction, Keyset, SortAllowlist};

/// Deliberately duplicated names, so the tiebreaker decides real boundaries.
///
/// With distinct names the ordering would be total on `name` alone and the tiebreaker would never
/// be exercised — the test would pass while proving nothing about the property it exists for.
#[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
const ROWS: [(&str, i64); 9] = [
    ("alpha", 3),
    ("alpha", 1),
    ("alpha", 2),
    ("beta", 5),
    ("beta", 4),
    ("delta", 9),
    ("delta", 7),
    ("delta", 8),
    ("gamma", 6),
];

#[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
const PAGE: usize = 2;

#[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
fn allowlist() -> SortAllowlist {
    SortAllowlist::new().allow("name", "name").tiebreaker("id")
}

/// The fingerprint the cursors below are issued under.
///
/// A constant here because the filter set never changes in this test. In an application it is
/// derived from the filters, which is what makes a cursor presented against different filters
/// refusable rather than silently meaningless.
#[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
const FINGERPRINT: u64 = 0x5150_4147_4531_0001;

macro_rules! pager {
    ($name:ident, $alias:ty, $connect:path, $url:expr, $kind:expr) => {
        /// Pages the whole table, returning every page in order.
        #[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
        async fn $name() -> Option<Vec<Vec<(String, i64)>>> {
            use renvor_sqlx::page::{limit_clause, order_clause, seek_predicate};
            use sqlx::AssertSqlSafe;

            let dsn = support::url($url)?;
            let database: $alias = $connect(&dsn, &support::settings()).await.expect("connects");

            sqlx::query(AssertSqlSafe(
                "CREATE TABLE IF NOT EXISTS rv_page (id BIGINT PRIMARY KEY, name VARCHAR(50) NOT NULL)"
                    .to_owned(),
            ))
            .execute(database.pool())
            .await
            .expect("creates");
            sqlx::query(AssertSqlSafe("DELETE FROM rv_page".to_owned()))
                .execute(database.pool())
                .await
                .expect("clears");
            for (name, id) in ROWS {
                sqlx::query(AssertSqlSafe(format!(
                    "INSERT INTO rv_page (id, name) VALUES ({}, {})",
                    $kind.placeholder(1),
                    $kind.placeholder(2)
                )))
                .bind(id)
                .bind(name)
                .execute(database.pool())
                .await
                .expect("inserts");
            }

            let order = allowlist()
                .order_by(&[("name".to_owned(), Direction::Ascending)])
                .expect("allowed");
            let clause = order_clause(&order, $kind);
            let seek = seek_predicate(&order, $kind, 1).expect("renders");

            let mut cursor: Option<Keyset> = None;
            let mut pages = Vec::new();
            loop {
                // The statement is assembled from Renvor-owned constants only: the columns come
                // from the allowlist, the placeholders from `DatabaseKind`, and the limit from a
                // `usize`. Both cursor values are BOUND.
                let sql = match cursor {
                    None => format!(
                        "SELECT name, id FROM rv_page ORDER BY {clause} {}",
                        limit_clause(PAGE)
                    ),
                    Some(_) => format!(
                        "SELECT name, id FROM rv_page WHERE {seek} ORDER BY {clause} {}",
                        limit_clause(PAGE)
                    ),
                };
                let mut query = sqlx::query_as::<_, (String, i64)>(AssertSqlSafe(sql));
                if let Some(position) = &cursor {
                    let name = String::from_utf8(position.values()[0].clone())
                        .expect("the cursor carried valid UTF-8");
                    let id = i64::from_be_bytes(
                        position.values()[1]
                            .as_slice()
                            .try_into()
                            .expect("the cursor carried eight bytes"),
                    );
                    query = query.bind(name).bind(id);
                }
                let rows = query
                    .fetch_all(database.pool())
                    .await
                    .expect("reads a page");
                if rows.is_empty() {
                    break;
                }

                // The cursor is REBUILT FROM THE LAST ROW and round-tripped through its opaque
                // text form, so the encoding is exercised on every page rather than only once.
                let (name, id) = rows.last().expect("non-empty").clone();
                let issued = Keyset::new(
                    FINGERPRINT,
                    vec![name.into_bytes(), id.to_be_bytes().to_vec()],
                );
                let text = issued.to_cursor();
                cursor = Some(
                    Keyset::from_cursor(text.encode().as_str(), FINGERPRINT).expect("its own cursor decodes"),
                );

                pages.push(rows);
            }
            Some(pages)
        }
    };
}

pager!(
    postgres_pages,
    renvor_sqlx::PostgresDatabase,
    renvor_sqlx::connect_postgres,
    support::POSTGRES_URL,
    renvor_database::DatabaseKind::Postgres
);
pager!(
    mysql_pages,
    renvor_sqlx::MySqlDatabase,
    renvor_sqlx::connect_mysql,
    support::MYSQL_URL,
    renvor_database::DatabaseKind::MySql
);

/// FR-041: identical data, identical page boundaries, on both engines.
#[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
#[tokio::test]
async fn both_databases_return_the_same_page_boundaries() {
    let (Some(postgres), Some(mysql)) = (postgres_pages().await, mysql_pages().await) else {
        // Both databases are needed to compare them, so one missing means this proves nothing.
        // Saying so is the point: PLAN.md §17 forbids reporting a skipped test as a gate.
        println!(
            "SKIPPED: set both {} and {} to compare page boundaries across engines",
            support::POSTGRES_URL,
            support::MYSQL_URL
        );
        return;
    };

    assert_eq!(
        postgres, mysql,
        "the two databases disagree about where pages end"
    );

    // And the paging actually covered the table, so an empty-equals-empty pass is impossible.
    let seen: Vec<(String, i64)> = postgres.iter().flatten().cloned().collect();
    assert_eq!(seen.len(), ROWS.len(), "paging did not cover every row");
    assert_eq!(
        postgres.len(),
        ROWS.len().div_ceil(PAGE),
        "unexpected number of pages: {postgres:?}"
    );

    // The tiebreaker did real work: the duplicated names are returned in `id` order.
    let alphas: Vec<i64> = seen
        .iter()
        .filter(|(name, _)| name == "alpha")
        .map(|(_, id)| *id)
        .collect();
    assert_eq!(alphas, vec![1, 2, 3], "the tiebreaker did not order the ties");
}

/// FR-037: a cursor issued under one query is refused by another.
#[cfg(all(feature = "db-postgres", feature = "db-mysql"))]
#[test]
fn a_cursor_from_a_different_query_is_refused() {
    let issued = Keyset::new(FINGERPRINT, vec![b"alpha".to_vec(), 1_i64.to_be_bytes().to_vec()]);
    let text = issued.to_cursor();
    assert!(Keyset::from_cursor(text.encode().as_str(), FINGERPRINT).is_ok());
    // The same bytes, presented against a different filter set.
    assert!(
        Keyset::from_cursor(text.encode().as_str(), FINGERPRINT ^ 1).is_err(),
        "a foreign cursor was accepted"
    );
}
