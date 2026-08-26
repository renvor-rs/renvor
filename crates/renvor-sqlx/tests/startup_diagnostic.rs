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
    ($module:ident, $feature:literal, $driver:ty, $kind:expr, $scheme:literal, $url:expr) => {
        #[cfg(feature = $feature)]
        mod $module {
            use core::time::Duration;

            use renvor_core::provider::registry::{CapabilityId, ProviderId};
            use renvor_database::{
                ConnectionString, DatabaseAdapter, DatabaseError, DatabaseErrorKind, PoolSettings,
                StartupDiagnostic, StartupPhase,
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
                    DatabaseAdapter::Sqlx,
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
                    rendered.contains(DatabaseAdapter::Sqlx.as_str()),
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

            /// A refusal by the SERVER, on the connection this suite's own database answers.
            ///
            /// The test above proves a diagnostic is produced when nothing answers. This one
            /// proves it for the case an operator actually hits — the server answers, completes
            /// the TCP handshake, and refuses the credential — which reaches
            /// `classify_connect_error` rather than the I/O arm, and is a different line of code.
            ///
            /// The refused password is the canary, so the value the server rejected is the value
            /// the diagnostic must not repeat. The assertions traverse `source` as well as
            /// `Display` and `Debug`, because the chain grew a link and a proof that stopped at
            /// the top would have stopped covering it.
            #[tokio::test]
            async fn a_server_side_refusal_names_the_provider_and_what_to_do() {
                let Some(dsn) = support::url($url) else {
                    return;
                };
                // CONTROL: the SAME server accepts the SAME user with the real password. Without
                // this, the assertions below would pass equally well against a DSN pointing at
                // nothing — which is precisely the weakness that makes port 1 insufficient on its
                // own, reproduced one level up.
                let reachable = renvor_sqlx::provider::SqlxProvider::<$driver>::new(
                    ProviderId::new("database"),
                    CapabilityId::new("database"),
                    dsn.clone(),
                    support::settings(),
                    $kind,
                );
                support::initialise(&reachable)
                    .await
                    .expect("the configured database must accept the configured credential");
                renvor_core::provider::registry::Provider::stop(&reachable)
                    .await
                    .expect("the control pool closes within its bound");

                let provider = renvor_sqlx::provider::SqlxProvider::<$driver>::new(
                    ProviderId::new("database"),
                    CapabilityId::new("database"),
                    support::with_rejected_password(&dsn),
                    PoolSettings::default()
                        .with_connect_timeout(Duration::from_secs(5))
                        .expect("bounded")
                        .with_acquire_timeout(Duration::from_secs(5))
                        .expect("bounded"),
                    $kind,
                );

                let error = support::initialise(&provider)
                    .await
                    .expect_err("a refused credential must fail to start");

                let diagnostic = error
                    .downcast_ref::<StartupDiagnostic>()
                    .expect("the provider must return a StartupDiagnostic, not a bare error");
                assert_eq!(
                    diagnostic.adapter(),
                    DatabaseAdapter::Sqlx,
                    "the diagnostic names the wrong adapter"
                );
                assert_eq!(diagnostic.database(), $kind);
                assert_eq!(
                    diagnostic.phase(),
                    StartupPhase::Connect,
                    "the server refused the handshake, so any other phase is a mis-report"
                );
                assert_eq!(
                    diagnostic.kind(),
                    DatabaseErrorKind::ConnectFailed,
                    "a server-side refusal is a failure to connect, not a rejected statement"
                );

                // ---- the safe cause survives, and it is the LAST link ----
                let cause = core::error::Error::source(diagnostic)
                    .expect("the safe cause must be preserved");
                assert!(
                    cause.downcast_ref::<DatabaseError>().is_some(),
                    "the immediate cause must be the normalised DatabaseError"
                );
                assert!(
                    cause.source().is_none(),
                    "the chain must stop before the driver's own error"
                );

                // ---- the advice covers the cause that actually occurred ----
                assert!(
                    diagnostic
                        .corrective_action()
                        .to_ascii_lowercase()
                        .contains("credential"),
                    "a credential refusal was not among the causes the advice names"
                );

                // ---- and nothing leaks, across Display, Debug, and the whole chain ----
                //
                // NO MESSAGE INTERPOLATES A RENDERING, which is also the proof for the fourth
                // surface: a failure here cannot print the canary into the test log, because no
                // assertion message in this file can name it. `renvor-core`'s `diagnostics` suite
                // enforces that for every credential-handling file rather than trusting this note.
                let mut rendered = format!("{error} {error:?}");
                let mut link = core::error::Error::source(diagnostic);
                while let Some(step) = link {
                    rendered.push_str(&format!(" {step} {step:?}"));
                    link = step.source();
                }
                for (index, leak) in [support::CREDENTIAL_CANARY, "://", dsn.expose()]
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
    "postgres",
    support::POSTGRES_URL
);
engine!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_database::DatabaseKind::MySql,
    "mysql",
    support::MYSQL_URL
);
