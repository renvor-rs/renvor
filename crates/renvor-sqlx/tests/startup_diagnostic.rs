//! What an operator is actually handed when this adapter cannot start.
//!
//! # Why this is an integration test rather than a unit test on the type
//!
//! `renvor-database` already proves that **no** `StartupDiagnostic` can render a secret, by
//! enumerating every one it is possible to construct. What that cannot prove is that the provider
//! **returns one** — a boot path that still handed back a bare driver error would satisfy every
//! assertion in that crate while telling an operator nothing.
//!
//! So this drives the real `initialise` against a deliberately unreachable database, with a
//! recognisable credential in the DSN, and reads what comes out.

mod support;

/// One engine's startup failure.
macro_rules! engine {
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $scheme:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use core::time::Duration;

            use renvor_core::provider::registry::{CapabilityId, ProviderId};
            use renvor_database::{
                ConnectionString, DatabaseErrorKind, PoolSettings, StartupDiagnostic, StartupPhase,
            };

            use super::support;

            /// Port 1 is refused immediately on every platform this runs on, so the failure is a
            /// connection refusal rather than a timeout and the test costs no wall-clock.
            fn unreachable_dsn() -> ConnectionString {
                ConnectionString::new(format!(
                    "{}://renvor:{}@127.0.0.1:1/absent",
                    $scheme,
                    support::CREDENTIAL_CANARY
                ))
            }

            #[tokio::test]
            async fn a_failed_start_names_the_provider_and_what_to_do() {
                let provider = renvor_sqlx::provider::SqlxProvider::<$driver>::new(
                    ProviderId::new("database"),
                    CapabilityId::new("database"),
                    unreachable_dsn(),
                    PoolSettings::default()
                        .with_connect_timeout(Duration::from_secs(2))
                        .expect("bounded")
                        .with_acquire_timeout(Duration::from_secs(3))
                        .expect("bounded"),
                    $kind,
                );

                let error = support::initialise(&provider)
                    .await
                    .expect_err("an unreachable database must fail to start");
                let rendered = format!("{error} {error:?}");

                // ---- it is the TYPED diagnostic, not a string that happens to read like one ----
                let diagnostic = error
                    .downcast_ref::<StartupDiagnostic>()
                    .expect("the provider must return a StartupDiagnostic, not a bare error");
                assert_eq!(
                    diagnostic.adapter(),
                    "renvor-sqlx",
                    "the diagnostic names the wrong adapter"
                );
                assert_eq!(diagnostic.database(), $kind);
                assert_eq!(
                    diagnostic.phase(),
                    StartupPhase::Connect,
                    "the failure was a refused connection, so any other phase is a mis-report"
                );
                assert_eq!(diagnostic.kind(), DatabaseErrorKind::ConnectFailed);

                // ---- it identifies the provider and gives an action ----
                //
                // NO MESSAGE INTERPOLATES `rendered`. `renvor-core`'s `diagnostics` suite forbids
                // it in any file that handles a credential, and this file plants one: on a
                // redaction regression the failure message would print the canary into the test
                // log — the single run where that matters most. Each message names its own check
                // instead, which is enough to tell them apart.
                assert!(
                    rendered.contains("renvor-sqlx"),
                    "the diagnostic did not name the adapter"
                );
                assert!(
                    rendered.contains($kind.as_str()),
                    "the diagnostic did not name the database"
                );
                assert!(
                    rendered.contains("What to do:"),
                    "the diagnostic offered no corrective action"
                );
                assert!(
                    !diagnostic.corrective_action().is_empty(),
                    "a diagnostic with no action is half a diagnostic"
                );

                // ---- and it leaks nothing ----
                // `index` rather than the value, and `index` rather than any other name:
                // `renvor-core`'s allowlist of interpolations permitted in a credential-handling
                // file names it, and widening that list to admit a synonym would be widening a
                // safety rule to fit this file. The index identifies which substring was found
                // without printing
                // it — the first entry is the canary itself, so naming the value would defeat the
                // assertion at exactly the moment it fires.
                for (index, leak) in [
                    support::CREDENTIAL_CANARY,
                    "://",
                    "127.0.0.1",
                    "renvor:",
                    "absent",
                ]
                .into_iter()
                .enumerate()
                {
                    assert!(
                        !rendered.contains(leak),
                        "the startup diagnostic leaked forbidden substring {index}"
                    );
                }
            }
        }
    };
}

engine!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_database::DatabaseKind::Postgres,
    "postgres"
);
engine!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_database::DatabaseKind::MySql,
    "mysql"
);
