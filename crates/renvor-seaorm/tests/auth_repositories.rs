//! The SeaORM authentication repositories, against real servers on both engines.
//!
//! # These are the SAME assertions the direct-SQLx suite makes
//!
//! Every property below is asserted through `renvor_auth::repository`'s traits, not through either
//! adapter's own types. That is what turns *"an application swapping `renvor-sqlx` for
//! `renvor-seaorm` does not rewrite its auth code"* from a claim into a measurement — and with
//! these two rows the four-row matrix is complete for the auth persistence layer.
//!
//! # The race is a rendezvous, never a sleep
//!
//! `renvor_testkit::concurrency` states the rule: *"A race arranged with `sleep` is not a race."*
//! Both races below wait on a `tokio::sync::Barrier`, released when the last caller arrives, and
//! the expiry test **moves the clock** rather than waiting for it.

mod support;

macro_rules! seaorm_auth_suite {
    ($module:ident, $feature:literal, $driver:ty, $connect:path, $run:ident, $url:expr, $engine:literal) => {
        #[cfg(feature = $feature)]
        mod $module {
            use std::path::{Path, PathBuf};
            use std::sync::Arc;

            use chrono::{TimeZone as _, Utc};
            use renvor_auth::opaque::{Opaque, OpaqueKind, SecretDigest};
            use renvor_auth::password::PasswordService;
            use renvor_auth::repository::{
                CredentialRepository as _, Registration, SingleUseTokenRepository as _,
                UserRepository as _,
            };
            use renvor_auth::session::SessionRepository as _;
            use renvor_core::observe::entropy::{FixedEntropy, OsEntropy};
            use renvor_database::{Database as _, MigrationSettings};
            use renvor_seaorm::auth::TokenTable;
            use renvor_seaorm::auth::$module::{
                SeaOrmCredentialRepository, SeaOrmSessionRepository,
                SeaOrmSingleUseTokenRepository, SeaOrmUserRepository,
            };
            use renvor_seaorm::migrate::Migrations;
            use sea_orm::ConnectionTrait as _;

            use crate::support;

            const AUTH_TABLES: [&str; 8] = [
                "rv_auth_attempt",
                "rv_auth_refresh",
                "rv_auth_refresh_family",
                "rv_auth_password_reset",
                "rv_auth_verification",
                "rv_auth_session",
                "rv_auth_credential",
                "rv_auth_user",
            ];

            /// The migration set for this engine — the SAME directory the SQLx rows use.
            ///
            /// ADR-0022 has both adapters migrating on SQLx's engine, so one pair of directories
            /// serves all four rows. A second copy here would be a second thing to drift.
            fn auth_set() -> PathBuf {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("..")
                    .join("renvor-auth")
                    .join("migrations")
                    .join($engine)
            }

            async fn migrated() -> Option<(
                Arc<renvor_seaorm::SeaOrmDatabase<$driver>>,
                tokio::sync::MutexGuard<'static, ()>,
            )> {
                let guard = support::SHARED_FIXTURE.lock().await;
                let dsn = support::url($url)?;
                let database = $connect(&dsn, &support::settings())
                    .await
                    .expect("connects");
                {
                    let connection = database.acquire().await.expect("acquires");
                    for table in AUTH_TABLES {
                        connection
                            .execute_unprepared(&format!("DROP TABLE IF EXISTS {table}"))
                            .await
                            .expect("cleans");
                    }
                    connection
                        .execute_unprepared("DROP TABLE IF EXISTS _sqlx_migrations")
                        .await
                        .expect("cleans");
                }
                Migrations::load(&auth_set(), MigrationSettings::default())
                    .await
                    .expect("loads")
                    .$run(&database)
                    .await
                    .expect("migrates");
                Some((Arc::new(database), guard))
            }

            #[tokio::test]
            async fn a_duplicate_registration_resolves_to_exactly_one_account() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();

                let first = users
                    .register("ada@example.test", now)
                    .await
                    .expect("registers");
                let Registration::Created(id) = first else {
                    panic!("the first registration must create: {first:?}")
                };

                let second = users
                    .register("ada@example.test", now)
                    .await
                    .expect("a duplicate is an OUTCOME, not an error");
                assert_eq!(second, Registration::AlreadyRegistered);

                // POSITIVE CONTROL: a different address still creates, so the refusal is about the
                // duplicate rather than about a repository that refuses everything.
                let other = users
                    .register("grace@example.test", now)
                    .await
                    .expect("registers");
                assert!(matches!(other, Registration::Created(_)));
                assert_ne!(
                    users
                        .find_by_email("grace@example.test")
                        .await
                        .expect("finds")
                        .expect("present")
                        .id,
                    id
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn concurrent_registrations_of_one_address_admit_exactly_one() {
                // FR-080, through the SeaORM adapter. The unique constraint decides it, which is
                // what makes the answer identical to the direct-SQLx rows.
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let barrier = Arc::new(tokio::sync::Barrier::new(4));

                let mut handles = Vec::new();
                for _ in 0..4 {
                    let users = SeaOrmUserRepository::new(Arc::clone(&database));
                    let barrier = Arc::clone(&barrier);
                    handles.push(tokio::spawn(async move {
                        barrier.wait().await;
                        users.register("race@example.test", now).await
                    }));
                }

                let mut created = 0_usize;
                let mut already = 0_usize;
                for handle in handles {
                    match handle.await.expect("joins").expect("no database fault") {
                        Registration::Created(_) => created += 1,
                        Registration::AlreadyRegistered => already += 1,
                    }
                }
                assert_eq!(created, 1, "exactly one racer may create the account");
                assert_eq!(already, 3, "every loser must be told the address is taken");

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn a_single_use_token_is_consumed_exactly_once_under_concurrency() {
                // FR-050 and SC-007. One conditional UPDATE, so the outcome does not depend on the
                // isolation level — which C-16 §3 forbids relying on and which the engines default
                // differently.
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let Registration::Created(user) = users
                    .register("reset@example.test", now)
                    .await
                    .expect("registers")
                else {
                    panic!("created")
                };

                let secret = Opaque::generate(OpaqueKind::PasswordReset, &OsEntropy::new())
                    .expect("entropy");
                let digest = SecretDigest::of(&secret);
                SeaOrmSingleUseTokenRepository::new(
                    Arc::clone(&database),
                    TokenTable::PasswordReset,
                )
                .issue(user, &digest, now + chrono::Duration::hours(1))
                .await
                .expect("issues");

                let barrier = Arc::new(tokio::sync::Barrier::new(4));
                let mut handles = Vec::new();
                for _ in 0..4 {
                    let tokens = SeaOrmSingleUseTokenRepository::new(
                        Arc::clone(&database),
                        TokenTable::PasswordReset,
                    );
                    let barrier = Arc::clone(&barrier);
                    handles.push(tokio::spawn(async move {
                        barrier.wait().await;
                        tokens.consume(&digest, now).await
                    }));
                }

                let mut consumed = 0_usize;
                for handle in handles {
                    if handle.await.expect("joins").expect("no fault").is_some() {
                        consumed += 1;
                    }
                }
                assert_eq!(
                    consumed, 1,
                    "exactly one caller may consume a single-use token"
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn a_resend_invalidates_every_outstanding_token_for_that_user() {
                // The persistence half of "at most one live token per purpose per account".
                // Asserted on a real server because it is a property of a conditional UPDATE's
                // reach, not of the code that calls it — and the two engines could differ.
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let Registration::Created(ada) = users
                    .register("resend@example.test", now)
                    .await
                    .expect("registers")
                else {
                    panic!("created")
                };
                let Registration::Created(bob) = users
                    .register("other@example.test", now)
                    .await
                    .expect("registers")
                else {
                    panic!("created")
                };

                let tokens = SeaOrmSingleUseTokenRepository::new(
                    Arc::clone(&database),
                    TokenTable::Verification,
                );

                // Three live tokens for Ada, one for Bob.
                let mut ada_secrets = Vec::new();
                for _ in 0..3 {
                    let secret = Opaque::generate(OpaqueKind::Verification, &OsEntropy::new())
                        .expect("entropy");
                    tokens
                        .issue(
                            ada,
                            &SecretDigest::of(&secret),
                            now + chrono::Duration::hours(1),
                        )
                        .await
                        .expect("issues");
                    ada_secrets.push(secret);
                }
                let bob_secret =
                    Opaque::generate(OpaqueKind::Verification, &OsEntropy::new()).expect("entropy");
                tokens
                    .issue(
                        bob,
                        &SecretDigest::of(&bob_secret),
                        now + chrono::Duration::hours(1),
                    )
                    .await
                    .expect("issues");

                let swept = tokens
                    .invalidate_all_for(ada, now)
                    .await
                    .expect("invalidates");
                assert_eq!(swept, 3, "every one of Ada's live tokens must be swept");

                for secret in &ada_secrets {
                    assert!(
                        tokens
                            .consume(&SecretDigest::of(secret), now)
                            .await
                            .expect("no fault")
                            .is_none(),
                        "an invalidated token must not be consumable"
                    );
                }
                // POSITIVE CONTROL: BOB's token is untouched. Without this, an implementation that
                // swept the whole table would pass every assertion above.
                assert_eq!(
                    tokens
                        .consume(&SecretDigest::of(&bob_secret), now)
                        .await
                        .expect("no fault"),
                    Some(bob),
                    "another user's token must survive"
                );

                // A second sweep finds nothing left to do.
                assert_eq!(
                    tokens
                        .invalidate_all_for(ada, now)
                        .await
                        .expect("invalidates"),
                    0,
                    "already-invalidated rows must not be swept twice"
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn an_expired_token_is_refused_against_the_injected_clock() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let issued = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let Registration::Created(user) = users
                    .register("expiry@example.test", issued)
                    .await
                    .expect("registers")
                else {
                    panic!("created")
                };

                let secret =
                    Opaque::generate(OpaqueKind::Verification, &OsEntropy::new()).expect("entropy");
                let digest = SecretDigest::of(&secret);
                let tokens = SeaOrmSingleUseTokenRepository::new(
                    Arc::clone(&database),
                    TokenTable::Verification,
                );
                tokens
                    .issue(user, &digest, issued + chrono::Duration::hours(1))
                    .await
                    .expect("issues");

                // The clock MOVES; the test does not wait.
                let after = issued + chrono::Duration::hours(1) + chrono::Duration::seconds(1);
                assert!(
                    tokens
                        .consume(&digest, after)
                        .await
                        .expect("no fault")
                        .is_none(),
                    "an expired token must not be consumable"
                );
                // POSITIVE CONTROL: before expiry the same token IS consumable.
                assert_eq!(
                    tokens.consume(&digest, issued).await.expect("no fault"),
                    Some(user)
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn the_table_holds_a_digest_and_never_the_secret() {
                // FR-048. A stolen backup must yield digests, and a digest cannot be presented.
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let Registration::Created(user) = users
                    .register("digest@example.test", now)
                    .await
                    .expect("registers")
                else {
                    panic!("created")
                };

                let secret =
                    Opaque::generate(OpaqueKind::Verification, &FixedEntropy::new(vec![7]))
                        .expect("entropy");
                SeaOrmSingleUseTokenRepository::new(
                    Arc::clone(&database),
                    TokenTable::Verification,
                )
                .issue(
                    user,
                    &SecretDigest::of(&secret),
                    now + chrono::Duration::hours(1),
                )
                .await
                .expect("issues");

                let connection = database.acquire().await.expect("acquires");
                let row = connection
                    .query_one_raw(sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        "SELECT token_hash FROM rv_auth_verification",
                    ))
                    .await
                    .expect("queries")
                    .expect("one row");
                let stored: Vec<u8> = row.try_get("", "token_hash").expect("reads");

                assert_eq!(stored.len(), 32, "the stored value is a digest");
                assert_eq!(
                    stored,
                    SecretDigest::of(&secret).as_bytes().to_vec(),
                    "the stored value must be the digest of the secret"
                );
                drop(connection);

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn a_credential_round_trips_and_replaces() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let Registration::Created(user) = users
                    .register("cred@example.test", now)
                    .await
                    .expect("registers")
                else {
                    panic!("created")
                };

                let service = PasswordService::default();
                let credentials = SeaOrmCredentialRepository::new(Arc::clone(&database));
                assert!(
                    credentials.find(user).await.expect("no fault").is_none(),
                    "a new account has no credential yet"
                );

                let hash = service
                    .hash("correct horse battery staple", &OsEntropy::new())
                    .expect("hashes");
                credentials
                    .upsert(user, &hash, false, now)
                    .await
                    .expect("stores");

                let loaded = credentials
                    .find(user)
                    .await
                    .expect("no fault")
                    .expect("present");
                assert_eq!(loaded.password_hash, hash);
                assert!(!loaded.must_change);
                assert!(service.verify("correct horse battery staple", &loaded.password_hash));

                let replacement = service
                    .hash("a different correct horse", &OsEntropy::new())
                    .expect("hashes");
                credentials
                    .upsert(user, &replacement, true, now)
                    .await
                    .expect("replaces");
                let reloaded = credentials
                    .find(user)
                    .await
                    .expect("no fault")
                    .expect("present");
                assert_eq!(reloaded.password_hash, replacement);
                assert!(reloaded.must_change, "the compromise flag must persist");

                let connection = database.acquire().await.expect("acquires");
                let row = connection
                    .query_one_raw(sea_orm::Statement::from_string(
                        connection.get_database_backend(),
                        "SELECT COUNT(*) AS n FROM rv_auth_credential",
                    ))
                    .await
                    .expect("queries")
                    .expect("one row");
                let count: i64 = row.try_get("", "n").expect("reads");
                assert_eq!(count, 1, "an upsert must not accumulate rows");
                drop(connection);

                database.close().await.expect("closes");
            }

            /// Phase 011. Found by driving the generated authentication starter against a real
            /// mail sink: the confirmation consumed its token and the column stayed NULL, because
            /// no repository method wrote it. The first instant wins on a second confirmation.
            #[tokio::test]
            async fn marking_an_address_verified_records_the_first_instant_only() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let Registration::Created(user) = users
                    .register("verify@example.test", now)
                    .await
                    .expect("registers")
                else {
                    panic!("created")
                };
                assert_eq!(
                    users
                        .find_by_id(user)
                        .await
                        .expect("no fault")
                        .expect("present")
                        .email_verified_at,
                    None,
                    "a new account is unverified"
                );

                let first = now + chrono::Duration::minutes(5);
                users.mark_email_verified(user, first).await.expect("marks");
                let record = users
                    .find_by_id(user)
                    .await
                    .expect("no fault")
                    .expect("present");
                assert_eq!(record.email_verified_at, Some(first));

                let later = first + chrono::Duration::hours(1);
                users
                    .mark_email_verified(user, later)
                    .await
                    .expect("marks again");
                let record = users
                    .find_by_id(user)
                    .await
                    .expect("no fault")
                    .expect("present");
                assert_eq!(
                    record.email_verified_at,
                    Some(first),
                    "a second confirmation must keep the first instant"
                );

                // An identity nobody registered is not an error to the caller.
                users
                    .mark_email_verified(renvor_auth::subject::UserId::from_bytes([7; 16]), later)
                    .await
                    .expect("an unknown identity is not a fault");

                database.close().await.expect("closes");
            }

            // ---- sessions (batch F) --------------------------------------------------------

            /// Registers one subject and returns its identity.
            async fn a_subject(
                users: &SeaOrmUserRepository,
                email: &str,
                now: chrono::DateTime<Utc>,
            ) -> renvor_auth::subject::UserId {
                match users.register(email, now).await.expect("registers") {
                    Registration::Created(id) => id,
                    other => panic!("expected a fresh account, got {other:?}"),
                }
            }

            fn a_session_secret() -> Opaque {
                Opaque::generate(OpaqueKind::Session, &OsEntropy::new()).expect("entropy")
            }

            #[tokio::test]
            async fn a_session_row_holds_a_digest_and_never_the_identifier() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let sessions = SeaOrmSessionRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let user = a_subject(&users, "digest@example.test", now).await;

                let secret = a_session_secret();
                sessions
                    .create(user, &SecretDigest::of(&secret), now)
                    .await
                    .expect("creates");

                // SCOPED: the acquired connection must return to the pool before `close()`, or
                // the close waits for a checkout that is still held and reports DeadlineExceeded.
                let stored: Vec<u8> = {
                    let connection = database.acquire().await.expect("acquires");
                    let row = connection
                        .query_one_raw(sea_orm::Statement::from_string(
                            connection.get_database_backend(),
                            "SELECT token_hash FROM rv_auth_session".to_owned(),
                        ))
                        .await
                        .expect("reads")
                        .expect("one row");
                    row.try_get("", "token_hash").expect("column")
                };

                // Positive control: it IS the digest.
                assert_eq!(
                    stored.as_slice(),
                    SecretDigest::of(&secret).as_bytes().as_slice(),
                    "the row does not hold the digest"
                );
                // And it is NOT the identifier. Compared BYTES to BYTES: comparing 32 stored bytes
                // against a 64-character hex string would pass against any implementation at all.
                let raw: Vec<u8> = (0..32)
                    .map(|i| {
                        u8::from_str_radix(&secret.expose()[i * 2..i * 2 + 2], 16).expect("hex")
                    })
                    .collect();
                assert_ne!(
                    stored, raw,
                    "the raw session identifier was written to the database"
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn two_concurrent_logouts_revoke_exactly_once() {
                // A rendezvous, not a sleep. `revoke` is ONE conditional UPDATE whose SET always
                // alters a value, so `rows_affected` is unambiguous on both engines.
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let user = a_subject(&users, "race-logout@example.test", now).await;
                let secret = a_session_secret();
                SeaOrmSessionRepository::new(Arc::clone(&database))
                    .create(user, &SecretDigest::of(&secret), now)
                    .await
                    .expect("creates");

                let barrier = Arc::new(tokio::sync::Barrier::new(4));
                let mut handles = Vec::new();
                for _ in 0..4 {
                    let sessions = SeaOrmSessionRepository::new(Arc::clone(&database));
                    let barrier = Arc::clone(&barrier);
                    let digest = SecretDigest::of(&secret);
                    handles.push(tokio::spawn(async move {
                        barrier.wait().await;
                        sessions.revoke(&digest, now).await
                    }));
                }
                let mut winners = 0_usize;
                for handle in handles {
                    if handle.await.expect("joins").expect("no database fault") {
                        winners += 1;
                    }
                }
                assert_eq!(winners, 1, "exactly one logout may revoke a live session");

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn touching_twice_at_one_instant_keeps_the_session_live() {
                // THE MySQL TRAP. `touch` decides liveness from `rows_affected`, and MySQL reports
                // rows *changed* unless the client negotiated `CLIENT_FOUND_ROWS`. A second touch
                // at the SAME instant writes `last_seen_at` the value it already holds — zero rows
                // changed, one row matched. Without the flag this test signs the user out.
                //
                // `sqlx-mysql` does set it, and grepping its source is not proof; this is.
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let sessions = SeaOrmSessionRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap();
                let user = a_subject(&users, "same-instant@example.test", now).await;
                let secret = a_session_secret();
                let digest = SecretDigest::of(&secret);
                sessions.create(user, &digest, now).await.expect("creates");

                let idle = now - chrono::Duration::minutes(30);
                let absolute = now - chrono::Duration::hours(12);
                for attempt in 1..=3 {
                    let live = sessions
                        .touch(&digest, now, idle, absolute)
                        .await
                        .expect("no database fault");
                    assert!(
                        live.is_some(),
                        "touch #{attempt} at the same instant reported a dead session"
                    );
                }

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn each_expiry_window_refuses_a_session_on_its_own() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let sessions = SeaOrmSessionRepository::new(Arc::clone(&database));
                let start = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let user = a_subject(&users, "windows@example.test", start).await;
                let secret = a_session_secret();
                let digest = SecretDigest::of(&secret);
                sessions
                    .create(user, &digest, start)
                    .await
                    .expect("creates");

                // Positive control first: inside both windows it is live.
                let inside = start + chrono::Duration::minutes(10);
                assert!(
                    sessions
                        .touch(
                            &digest,
                            inside,
                            inside - chrono::Duration::minutes(30),
                            inside - chrono::Duration::hours(12)
                        )
                        .await
                        .expect("no fault")
                        .is_some(),
                    "the control must be live"
                );

                // Idle alone: the absolute window is wide, the idle one has lapsed.
                let idled = inside + chrono::Duration::hours(1);
                assert!(
                    sessions
                        .touch(
                            &digest,
                            idled,
                            idled - chrono::Duration::minutes(30),
                            idled - chrono::Duration::days(30)
                        )
                        .await
                        .expect("no fault")
                        .is_none(),
                    "the inactivity window did not end the session"
                );

                // Absolute alone: activity is recent, the overall window has lapsed.
                let aged = inside + chrono::Duration::hours(20);
                assert!(
                    sessions
                        .touch(
                            &digest,
                            aged,
                            aged - chrono::Duration::days(30),
                            aged - chrono::Duration::hours(12)
                        )
                        .await
                        .expect("no fault")
                        .is_none(),
                    "the overall window did not end the session"
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn a_revoked_session_can_never_be_touched_again() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let sessions = SeaOrmSessionRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let user = a_subject(&users, "revoked@example.test", now).await;
                let secret = a_session_secret();
                let digest = SecretDigest::of(&secret);
                sessions.create(user, &digest, now).await.expect("creates");

                assert!(sessions.revoke(&digest, now).await.expect("revokes"));
                let later = now + chrono::Duration::minutes(1);
                assert!(
                    sessions
                        .touch(
                            &digest,
                            later,
                            later - chrono::Duration::minutes(30),
                            later - chrono::Duration::hours(12)
                        )
                        .await
                        .expect("no fault")
                        .is_none(),
                    "a revoked session was replayed"
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn live_sessions_come_back_least_recently_seen_first() {
                // The ORDER BY is the eviction order and therefore part of the port's contract:
                // newest-first would evict the session the subject is using right now.
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let sessions = SeaOrmSessionRepository::new(Arc::clone(&database));
                let start = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let user = a_subject(&users, "order@example.test", start).await;

                let mut digests = Vec::new();
                for minute in 0..3 {
                    let secret = a_session_secret();
                    let digest = SecretDigest::of(&secret);
                    sessions
                        .create(user, &digest, start + chrono::Duration::minutes(minute))
                        .await
                        .expect("creates");
                    digests.push(digest);
                }

                // Refresh the OLDEST so it becomes the newest by activity.
                let refreshed = start + chrono::Duration::minutes(10);
                sessions
                    .touch(
                        &digests[0],
                        refreshed,
                        refreshed - chrono::Duration::minutes(30),
                        refreshed - chrono::Duration::hours(12),
                    )
                    .await
                    .expect("no fault")
                    .expect("still live");

                let now = start + chrono::Duration::minutes(11);
                let live = sessions
                    .live_for(
                        user,
                        now - chrono::Duration::minutes(30),
                        now - chrono::Duration::hours(12),
                    )
                    .await
                    .expect("lists");
                assert_eq!(live.len(), 3);
                let order: Vec<[u8; 32]> = live.iter().map(|h| *h.digest.as_bytes()).collect();
                assert_eq!(
                    order,
                    vec![
                        *digests[1].as_bytes(),
                        *digests[2].as_bytes(),
                        *digests[0].as_bytes()
                    ],
                    "live_for must order by activity, not by creation"
                );

                database.close().await.expect("closes");
            }

            #[tokio::test]
            async fn revoking_every_session_leaves_another_subject_untouched() {
                let Some((database, _fixture)) = migrated().await else {
                    return;
                };
                let users = SeaOrmUserRepository::new(Arc::clone(&database));
                let sessions = SeaOrmSessionRepository::new(Arc::clone(&database));
                let now = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
                let mine = a_subject(&users, "mine@example.test", now).await;
                let theirs = a_subject(&users, "theirs@example.test", now).await;

                for _ in 0..3 {
                    sessions
                        .create(mine, &SecretDigest::of(&a_session_secret()), now)
                        .await
                        .expect("creates");
                }
                let survivor = a_session_secret();
                let survivor_digest = SecretDigest::of(&survivor);
                sessions
                    .create(theirs, &survivor_digest, now)
                    .await
                    .expect("creates");

                let revoked = sessions.revoke_all_for(mine, now).await.expect("revokes");
                assert_eq!(revoked, 3);

                let later = now + chrono::Duration::minutes(1);
                assert!(
                    sessions
                        .touch(
                            &survivor_digest,
                            later,
                            later - chrono::Duration::minutes(30),
                            later - chrono::Duration::hours(12)
                        )
                        .await
                        .expect("no fault")
                        .is_some(),
                    "another subject's session was revoked"
                );
                // And revoking again finds nothing left to do.
                assert_eq!(
                    sessions.revoke_all_for(mine, later).await.expect("again"),
                    0
                );

                database.close().await.expect("closes");
            }
        }
    };
}

seaorm_auth_suite!(
    postgres,
    "db-postgres",
    sqlx::Postgres,
    renvor_seaorm::connect_postgres,
    run_postgres,
    support::POSTGRES_URL,
    "postgres"
);
seaorm_auth_suite!(
    mysql,
    "db-mysql",
    sqlx::MySql,
    renvor_seaorm::connect_mysql,
    run_mysql,
    support::MYSQL_URL,
    "mysql"
);
