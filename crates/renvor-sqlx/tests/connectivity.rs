//! Does a connection actually open, and does readiness actually round-trip?
//!
//! # Why MySQL gets its own reasoning here
//!
//! This crate deliberately does not enable `sqlx/mysql-rsa`, because that feature resolves the
//! `rsa` crate and RUSTSEC-2023-0071 has no patch. Without RSA key exchange, MySQL's
//! `caching_sha2_password` cannot complete a **first** authentication over a plaintext channel.
//!
//! Whether that blocks a normal local development connection is a question about real servers, and
//! this test is the answer rather than an assumption.

mod support;

use renvor_database::{Database, DatabaseKind};

#[cfg(feature = "db-postgres")]
#[tokio::test]
async fn postgres_connects_and_reports_ready() {
    let Some(dsn) = support::url(support::POSTGRES_URL) else {
        return;
    };
    let database = renvor_sqlx::connect_postgres(&dsn, &support::settings())
        .await
        .expect("connects");
    assert_eq!(database.kind(), DatabaseKind::Postgres);
    database.check().await.expect("ready");
    database.close().await.expect("closes");
}

#[cfg(feature = "db-mysql")]
#[tokio::test]
async fn mysql_connects_without_the_rsa_feature() {
    let Some(dsn) = support::url(support::MYSQL_URL) else {
        return;
    };
    let database = renvor_sqlx::connect_mysql(&dsn, &support::settings())
        .await
        .expect("connects without sqlx/mysql-rsa");
    assert_eq!(database.kind(), DatabaseKind::MySql);
    database.check().await.expect("ready");
    database.close().await.expect("closes");
}

#[cfg(feature = "db-postgres")]
#[tokio::test]
async fn a_wrong_credential_fails_without_echoing_it() {
    let Some(dsn) = support::url(support::POSTGRES_URL) else {
        return;
    };
    // Replace the password with a canary, keeping the rest of the URL intact.
    let raw = dsn.expose();
    let Some((scheme, rest)) = raw.split_once("://") else {
        return;
    };
    let Some((_, host)) = rest.split_once('@') else {
        return;
    };
    let poisoned = renvor_database::ConnectionString::new(format!(
        "{scheme}://postgres:{}@{host}",
        support::CREDENTIAL_CANARY
    ));

    let error = renvor_sqlx::connect_postgres(&poisoned, &support::settings())
        .await
        .expect_err("a wrong password must not connect");

    let rendered = format!("{error} {error:?}");
    assert!(
        !rendered.contains(support::CREDENTIAL_CANARY),
        "the credential appeared in the error: {rendered}"
    );
    assert!(
        !rendered.contains("postgres://"),
        "the connection string appeared in the error: {rendered}"
    );
}
