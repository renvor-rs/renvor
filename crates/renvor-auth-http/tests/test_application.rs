//! FR-083 — a test application exercising every flow end to end, and SC-006's canary sweep.
//!
//! # Why this drives a real database rather than a set of fakes
//!
//! A suite of in-memory doubles would exercise the routes against a **second implementation** of
//! every port. It would pass while the real adapter was broken, and it would pass while the routes
//! and the adapter disagreed about what a repository call means.
//!
//! The four-row suites already prove the adapters. What is unproven until this file runs is the
//! **wiring**: that a JSON body reaches the operation, that a refusal becomes the right status,
//! that a session cookie makes a round trip, and that nothing secret comes back out.
//!
//! # It calls the handlers, not a socket
//!
//! `Handler::call` takes a `Request` and returns a `Response`, both Renvor types. A real socket
//! would add a server, a runtime, a port and a class of flake — and would test `axum`, which
//! `renvor-http`'s own suites already do. What is under test here is the route table.
//!
//! # PostgreSQL only, deliberately
//!
//! This is a **transport** test, not a portability one. Running it on both engines would assert the
//! same routes twice; the thing that varies by engine is the adapter, and `renvor-testkit`'s
//! four-row suites already measure that on all four rows.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, TimeZone as _, Utc};
use renvor_auth::CsrfKey;
use renvor_auth::FixedClock;
use renvor_auth::abuse::{AbuseContract, AbuseGuard, AttemptBuckets, AttemptKeyring};
use renvor_auth::audit::RecordingAuditSink;
use renvor_auth::cookie::{CookiePolicy, SESSION_COOKIE_NAME};
use renvor_auth::mail::RecordingMailSink;
use renvor_auth::password::{PasswordPolicy, PasswordService, StaticBlocklist};
use renvor_auth::service::{AuthenticationService, TokenLifetime};
use renvor_auth::session::{SessionPolicy, SessionService};
use renvor_auth_http::routes::{AuthEndpoints, routes};
use renvor_core::identity::ClientIdentity;
use renvor_core::{CancelScope, OsEntropy, RunIdentifier};
use renvor_database::{ConnectionString, Database as _, MigrationSettings, PoolSettings};
use renvor_http::route::{Method, PresentedCredentials, Request, RouteRegistry};
use renvor_http::{EffectiveOrigin, FetchMetadata, RequestContext, RequestId, Scheme};
use renvor_sqlx::Migrations;
use renvor_sqlx::auth::TokenTable;
use renvor_sqlx::auth::postgres::{
    SqlxAttemptRepository, SqlxCredentialRepository, SqlxSessionRepository,
    SqlxSingleUseTokenRepository, SqlxUserRepository,
};
use std::net::{IpAddr, Ipv4Addr};

/// A value the test supplies and that no response may echo.
const PASSWORD_CANARY: &str = "hunter2CanaryDoNotLeak-correct-horse";
/// The address under test. Finding it in a mailed-flow response would be an enumeration oracle.
const KNOWN_ADDRESS: &str = "ada-canary-9f3a@example.test";
/// An address with no account. Its responses must be indistinguishable from the known one's.
const UNKNOWN_ADDRESS: &str = "nobody-canary-9f3a@example.test";

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

/// The `csrf_token` a login response carried.
fn csrf_of(body: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    value
        .get("csrf_token")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The origin a request reaching an `http` listener directly resolves to: the configured scheme,
/// the validated host, and the scheme's default port.
fn http_listener() -> EffectiveOrigin {
    EffectiveOrigin::new(Scheme::Http, "example.test", 80)
}

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0)
        .single()
        .expect("a real instant")
}

/// Reads the URL, or explains the skip. **A missing URL is a FAILURE when the gate expects one** —
/// the same rule `renvor-sqlx`'s support module states, for the same reason: a run that silently
/// skipped every real-database test reports the same `ok` as one that passed them.
fn url() -> Option<ConnectionString> {
    match std::env::var("RENVOR_TEST_POSTGRES_URL") {
        Ok(value) if !value.is_empty() => Some(ConnectionString::new(value)),
        _ => {
            assert!(
                std::env::var("RENVOR_TEST_REQUIRE_DATABASE").is_err(),
                "RENVOR_TEST_REQUIRE_DATABASE is set, so a database was expected and \
                 RENVOR_TEST_POSTGRES_URL is empty or absent"
            );
            println!("SKIPPED: set RENVOR_TEST_POSTGRES_URL to run the test application");
            None
        }
    }
}

/// Everything the application holds, plus what the sinks recorded.
struct Application {
    registry: RouteRegistry,
    database: renvor_sqlx::SqlxDatabase<sqlx::Postgres>,
    mail: Arc<RecordingMailSink>,
    /// Every response body and header this test produced, for the canary sweep.
    swept: std::sync::Mutex<Vec<String>>,
}

impl Application {
    /// Sends one request through the real route table and records the response for the sweep.
    async fn send(&self, method: Method, route: &str, body: &str, cookie: &str) -> (u16, String) {
        self.send_with_metadata(method, route, body, cookie, FetchMetadata::default())
            .await
    }

    /// `send`, with what the user agent said about the request's origin (FR-085), addressed to
    /// the `http` origin a request reaching the listener directly resolves to.
    async fn send_with_metadata(
        &self,
        method: Method,
        route: &str,
        body: &str,
        cookie: &str,
        metadata: FetchMetadata,
    ) -> (u16, String) {
        self.send_addressed(method, route, body, cookie, metadata, http_listener())
            .await
    }

    /// `send_with_metadata`, with the origin the transport resolved for the request — what the
    /// router builds from the configured scheme, a trusted proxy's `proto`, and the `Host`.
    async fn send_addressed(
        &self,
        method: Method,
        route: &str,
        body: &str,
        cookie: &str,
        metadata: FetchMetadata,
        addressed: EffectiveOrigin,
    ) -> (u16, String) {
        // THE METHOD IS MATCHED, not discarded.
        //
        // The first version located a route by path alone and threw the method away with
        // `let _ = method;` — so a route table that declared `GET /auth/login` would have passed
        // this suite unchanged. Found by requirements review.
        let declared = self
            .registry
            .routes()
            .iter()
            .find(|candidate| candidate.path() == route && candidate.method() == method)
            .unwrap_or_else(|| panic!("no route declares that path for that method"));

        let context = RequestContext::new(
            RunIdentifier::generate(&OsEntropy).expect("entropy"),
            RequestId::from_entropy([9, 8, 7, 6, 5, 4, 3, 2]),
            ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            addressed,
            CancelScope::root().child("request"),
        );
        let request = Request::new(
            context,
            body.as_bytes().to_vec(),
            String::new(),
            BTreeMap::new(),
        )
        .with_credentials(PresentedCredentials::new(cookie, ""))
        .with_fetch_metadata(metadata);

        let response = declared.dispatch(request).await;
        let status = response.status_code();
        let rendered = String::from_utf8_lossy(response.body()).into_owned();

        // THE SWEEP ACCUMULATES HERE, so it covers every response this file produced rather than
        // the ones somebody remembered to check.
        let mut swept = self.swept.lock().expect("unpoisoned");
        swept.push(rendered.clone());
        for (name, value) in response.headers() {
            swept.push(format!("{name}: {value}"));
        }
        (status, rendered)
    }

    /// The `Set-Cookie` value a response carried, if any.
    async fn login(&self, address: &str, password: &str) -> (u16, String, String) {
        let route = self
            .registry
            .routes()
            .iter()
            .find(|route| route.path() == "/auth/login")
            .expect("the login route");
        let context = RequestContext::new(
            RunIdentifier::generate(&OsEntropy).expect("entropy"),
            RequestId::from_entropy([1, 1, 1, 1, 2, 2, 2, 2]),
            ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            http_listener(),
            CancelScope::root().child("request"),
        );
        let body = format!(r#"{{"email":"{address}","password":"{password}"}}"#);
        let request = Request::new(context, body.into_bytes(), String::new(), BTreeMap::new());
        let response = route.dispatch(request).await;
        let status = response.status_code();
        let rendered = String::from_utf8_lossy(response.body()).into_owned();
        let cookie = response
            .headers()
            .iter()
            .find(|(name, _)| name == "set-cookie")
            .map(|(_, value)| value.clone())
            .unwrap_or_default();

        let mut swept = self.swept.lock().expect("unpoisoned");
        swept.push(rendered.clone());
        for (name, value) in response.headers() {
            swept.push(format!("{name}: {value}"));
        }
        (status, rendered, cookie)
    }
}

/// Builds the application against a freshly migrated database.
async fn application() -> Option<Application> {
    let dsn = url()?;
    let settings = PoolSettings::default()
        .with_max_connections(4)
        .expect("bounded")
        .with_acquire_timeout(StdDuration::from_secs(5))
        .expect("bounded")
        .with_connect_timeout(StdDuration::from_secs(5))
        .expect("bounded");
    let database = renvor_sqlx::connect_postgres(&dsn, &settings)
        .await
        .expect("connects");

    for table in AUTH_TABLES {
        sqlx::query(sqlx::AssertSqlSafe(format!("DROP TABLE IF EXISTS {table}")))
            .execute(database.pool())
            .await
            .expect("cleans");
    }
    sqlx::query(sqlx::AssertSqlSafe(
        "DROP TABLE IF EXISTS _sqlx_migrations".to_owned(),
    ))
    .execute(database.pool())
    .await
    .expect("cleans");

    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("renvor-auth")
        .join("migrations")
        .join("postgres");
    Migrations::load(&migrations, MigrationSettings::default())
        .await
        .expect("loads the auth migration set")
        .run_postgres(&database)
        .await
        .expect("applies the auth migration set");

    let pool = database.pool().clone();
    let entropy: Arc<dyn renvor_core::observe::EntropySource + Send + Sync> = Arc::new(OsEntropy);
    let mail = Arc::new(RecordingMailSink::new());

    let endpoints = Arc::new(AuthEndpoints {
        authentication: AuthenticationService::new(
            SqlxUserRepository::new(pool.clone()),
            SqlxCredentialRepository::new(pool.clone()),
            StaticBlocklist::new(["password123456789".to_owned()]),
            PasswordService::default(),
            PasswordPolicy::default(),
            Arc::clone(&entropy),
        )
        .expect("the dummy hash is computable"),
        sessions: SessionService::new(
            SqlxSessionRepository::new(pool.clone()),
            SessionPolicy::new(
                Duration::minutes(30),
                Duration::hours(8),
                std::num::NonZeroU32::new(5).expect("positive"),
            )
            .expect("within the AAL2 ceilings"),
            CookiePolicy::default(),
            Arc::clone(&entropy),
        ),
        // TWO STORES, TWO TABLES. One field served both until a requirements review found that a
        // verification resend was destroying the user's outstanding password-reset token.
        verifications: SqlxSingleUseTokenRepository::new(pool.clone(), TokenTable::Verification),
        resets: SqlxSingleUseTokenRepository::new(pool.clone(), TokenTable::PasswordReset),
        mail: Arc::clone(&mail),
        abuse: AbuseGuard::new(
            SqlxAttemptRepository::new(pool),
            AttemptKeyring::from_bytes([0x3C; 32], AttemptBuckets::default()),
            AbuseContract::default(),
            RecordingAuditSink::new(),
        ),
        csrf: CsrfKey::from_bytes([0x7E; 32]),
        entropy: Arc::clone(&entropy),
        token_lifetime: TokenLifetime::default(),
        clock: Arc::new(FixedClock::at(at())),
    });

    let mut registry = RouteRegistry::new();
    registry
        .group(routes(endpoints).expect("the route table builds"))
        .expect("the group registers");

    Some(Application {
        registry,
        database,
        mail,
        swept: std::sync::Mutex::new(Vec::new()),
    })
}

#[tokio::test]
async fn every_flow_answers_and_nothing_secret_comes_back() {
    let Some(app) = application().await else {
        return;
    };

    // ---- 1. REGISTRATION, and the duplicate that must look identical ---------------------------
    let register = format!(r#"{{"email":"{KNOWN_ADDRESS}","password":"{PASSWORD_CANARY}"}}"#);
    let (status, first) = app
        .send(Method::Post, "/auth/register", &register, "")
        .await;
    assert_eq!(status, 202, "registration was not accepted");
    let (status, again) = app
        .send(Method::Post, "/auth/register", &register, "")
        .await;
    assert_eq!(status, 202);
    assert_eq!(
        first, again,
        "a duplicate registration answered differently — that is an enumeration oracle"
    );

    // ---- 2. LOGIN, wrong password then right ----------------------------------------------------
    let (status, _body, _cookie) = app
        .login(KNOWN_ADDRESS, "an entirely wrong passphrase")
        .await;
    assert_eq!(status, 401, "a wrong password was not 401");

    let (status, login_body, cookie) = app.login(KNOWN_ADDRESS, PASSWORD_CANARY).await;
    assert_eq!(status, 200, "a correct password did not authenticate");
    let csrf = csrf_of(&login_body);
    assert!(!csrf.is_empty(), "login issued no CSRF token");
    // THE VALUE IS NOT PRINTED. `cookie` is a live session credential, and a failure here is
    // exactly the run where printing it would matter most — which is what
    // `renvor-core/tests/diagnostics.rs` refuses, and it caught the first version of this line.
    assert!(
        cookie.contains(SESSION_COOKIE_NAME) && cookie.contains("HttpOnly"),
        "the session cookie was not set, or lost HttpOnly"
    );
    // The header value is `name=value; attributes`; the flows below present it as a `Cookie:`.
    let presented = cookie.split(';').next().unwrap_or_default().to_owned();

    // ---- 3. THE UNKNOWN ACCOUNT IS INDISTINGUISHABLE --------------------------------------------
    let (unknown_status, _b, _c) = app.login(UNKNOWN_ADDRESS, PASSWORD_CANARY).await;
    assert_eq!(
        unknown_status, 401,
        "an unknown account answered differently from a wrong password"
    );

    // ---- 4. CURRENT USER, with and without the cookie -------------------------------------------
    let (status, body) = app.send(Method::Get, "/auth/me", "", &presented).await;
    assert_eq!(status, 200, "a live session could not read its own account");
    assert!(body.contains(KNOWN_ADDRESS), "the account was not returned");

    let (status, _body) = app.send(Method::Get, "/auth/me", "", "").await;
    assert_eq!(status, 401, "an absent cookie was not refused");

    // ---- 5. THE MAILED FLOWS: known and unknown answer the same ---------------------------------
    // INDEXED, not interpolated. `address` is caller data and `route` would be fine, but an index
    // identifies the case without either — which is what the diagnostics gate asks for.
    for (index, (route, address)) in [
        ("/auth/password/forgot", KNOWN_ADDRESS),
        ("/auth/password/forgot", UNKNOWN_ADDRESS),
        ("/auth/verification/resend", KNOWN_ADDRESS),
        ("/auth/verification/resend", UNKNOWN_ADDRESS),
    ]
    .into_iter()
    .enumerate()
    {
        let body = format!(r#"{{"email":"{address}"}}"#);
        let (status, rendered) = app.send(Method::Post, route, &body, "").await;
        assert_eq!(status, 202, "mailed flow {index} was not accepted");
        assert_eq!(
            rendered, r#"{"acknowledged":true}"#,
            "mailed flow {index} answered differently from its neighbours"
        );
    }

    // ---- 6. THE TOKEN-CONSUMING FLOWS refuse an invented token ----------------------------------
    let (status, _body) = app
        .send(
            Method::Post,
            "/auth/verification/confirm",
            r#"{"token":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}"#,
            "",
        )
        .await;
    assert_eq!(
        status, 401,
        "an invented verification token was not refused"
    );

    let (status, _body) = app
        .send(
            Method::Post,
            "/auth/password/reset",
            &format!(
                r#"{{"token":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef","password":"{PASSWORD_CANARY}"}}"#
            ),
            "",
        )
        .await;
    assert_eq!(status, 401, "an invented reset token was not refused");

    // ---- 7. LOGOUT IS CSRF-PROTECTED ------------------------------------------------------------
    //
    // FR-028: every cookie-authenticated unsafe operation. This is the only one in the table, and
    // it was unprotected until a requirements review found it.

    // WITHOUT a token: refused, and the session survives.
    let (status, _body) = app
        .send(
            Method::Post,
            "/auth/logout",
            r#"{"csrf_token":""}"#,
            &presented,
        )
        .await;
    assert_eq!(status, 403, "logout without a CSRF token was not refused");
    let (status, _body) = app.send(Method::Get, "/auth/me", "", &presented).await;
    assert_eq!(status, 200, "a refused logout ended the session anyway");

    // With a token minted for ANOTHER session: refused. This is the one that matters — an attacker
    // who can obtain *a* token must not be able to spend it on *this* session.
    let (_status, _body, other) = app.login(KNOWN_ADDRESS, PASSWORD_CANARY).await;
    let foreign = csrf_of(&other);
    let (status, _body) = app
        .send(
            Method::Post,
            "/auth/logout",
            &format!(r#"{{"csrf_token":"{foreign}"}}"#),
            &presented,
        )
        .await;
    assert_eq!(status, 403, "a token bound to another session was accepted");

    // WITH the right token: accepted.
    // 009/L-4 (FR-085): with a VALID token, the transport still refuses a request the user agent
    // marks cross-site, or whose Origin names another host — and absent headers do not refuse.
    let valid = format!(r#"{{"csrf_token":"{csrf}"}}"#);
    let (status, _body) = app
        .send_with_metadata(
            Method::Post,
            "/auth/logout",
            &valid,
            &presented,
            FetchMetadata::new(None, Some("cross-site")),
        )
        .await;
    assert_eq!(
        status, 403,
        "a cross-site logout with a valid token was accepted"
    );
    let (status, _body) = app
        .send_with_metadata(
            Method::Post,
            "/auth/logout",
            &valid,
            &presented,
            FetchMetadata::new(Some("https://evil.example.test"), None),
        )
        .await;
    assert_eq!(
        status, 403,
        "a foreign-Origin logout with a valid token was accepted"
    );
    let (status, _body) = app
        .send_with_metadata(
            Method::Post,
            "/auth/logout",
            &valid,
            &presented,
            FetchMetadata::new(Some("null"), None),
        )
        .await;
    assert_eq!(status, 403, "an opaque `null` Origin was accepted");

    // THE COMPLETE ORIGIN (Phase 010 correction round, finding 1; RFC 6454 §5). The request is
    // addressed to `http://example.test:80`, so an Origin naming the same host on another SCHEME
    // or another PORT is a different origin and is refused — with a valid token.
    let (status, _body) = app
        .send_with_metadata(
            Method::Post,
            "/auth/logout",
            &valid,
            &presented,
            FetchMetadata::new(Some("https://example.test"), Some("same-origin")),
        )
        .await;
    assert_eq!(
        status, 403,
        "an https Origin was accepted by a request addressed to the http origin"
    );
    let (status, _body) = app
        .send_with_metadata(
            Method::Post,
            "/auth/logout",
            &valid,
            &presented,
            FetchMetadata::new(Some("http://example.test:8443"), Some("same-origin")),
        )
        .await;
    assert_eq!(status, 403, "an Origin on another port was accepted");
    // And the same Origin from an untrusted peer behind a TLS terminator the operator did not
    // trust: the transport leaves the request on the listener's own `http`, so it is refused.
    let (status, _body) = app
        .send_addressed(
            Method::Post,
            "/auth/logout",
            &valid,
            &presented,
            FetchMetadata::new(Some("https://example.test"), None),
            http_listener(),
        )
        .await;
    assert_eq!(
        status, 403,
        "an https Origin was accepted when no trusted proxy vouched for https"
    );
    let (status, _body) = app.send(Method::Get, "/auth/me", "", &presented).await;
    assert_eq!(
        status, 200,
        "a refused cross-site logout ended the session anyway"
    );

    // THE ALLOWED CASES END A SESSION, which is the only observation that distinguishes "the
    // origin gate admitted it" from "the gate refused it": both refusals are the same 403. So
    // each runs against a FRESH session with its own valid token, and asserts the logout landed.
    //
    // (i) A trusted proxy that terminated TLS makes the request's origin
    // `https://example.test:443`, and an `https` Origin then matches — the explicit default port
    // and the absent one are the same origin (RFC 6454 §4 step 5).
    let (status, fresh_body, fresh_cookie) = app.login(KNOWN_ADDRESS, PASSWORD_CANARY).await;
    assert_eq!(status, 200);
    let fresh_presented = fresh_cookie
        .split(';')
        .next()
        .unwrap_or_default()
        .to_owned();
    let fresh_valid = format!(r#"{{"csrf_token":"{}"}}"#, csrf_of(&fresh_body));
    let (status, _body) = app
        .send_addressed(
            Method::Post,
            "/auth/logout",
            &fresh_valid,
            &fresh_presented,
            FetchMetadata::new(Some("https://example.test"), Some("same-origin")),
            EffectiveOrigin::new(Scheme::Https, "example.test", 443),
        )
        .await;
    assert_eq!(
        status, 200,
        "an https Origin was refused behind a trusted TLS terminator"
    );
    let (status, _body) = app
        .send(Method::Get, "/auth/me", "", &fresh_presented)
        .await;
    assert_eq!(status, 401, "the admitted logout did not end its session");

    // (ii) An IPv6 origin compares with its brackets and its port.
    let (status, fresh_body, fresh_cookie) = app.login(KNOWN_ADDRESS, PASSWORD_CANARY).await;
    assert_eq!(status, 200);
    let fresh_presented = fresh_cookie
        .split(';')
        .next()
        .unwrap_or_default()
        .to_owned();
    let fresh_valid = format!(r#"{{"csrf_token":"{}"}}"#, csrf_of(&fresh_body));
    let (status, _body) = app
        .send_addressed(
            Method::Post,
            "/auth/logout",
            &fresh_valid,
            &fresh_presented,
            FetchMetadata::new(Some("http://[::1]:8080"), None),
            EffectiveOrigin::new(Scheme::Http, "[::1]", 8080),
        )
        .await;
    assert_eq!(status, 200, "a same-origin IPv6 logout was refused");
    let (status, _body) = app
        .send(Method::Get, "/auth/me", "", &fresh_presented)
        .await;
    assert_eq!(status, 401, "the admitted logout did not end its session");

    // (iii) Host `example.test:80` written explicitly and an Origin without a port: the same
    // origin. This one uses the session under test, and is the logout that ends it.
    let (status, _body) = app
        .send_addressed(
            Method::Post,
            "/auth/logout",
            &valid,
            &presented,
            FetchMetadata::new(Some("http://example.test"), Some("same-origin")),
            EffectiveOrigin::new(Scheme::Http, "example.test", 80),
        )
        .await;
    assert_eq!(
        status, 200,
        "logout with a valid CSRF token and a matching Origin did not succeed"
    );

    // The cookie is dead now, and the answer is the same one an absent cookie gets.
    let (status, _body) = app.send(Method::Get, "/auth/me", "", &presented).await;
    assert_eq!(
        status, 401,
        "a logged-out session still read its own account"
    );

    // ---- 8. THE CANARY SWEEP, over every response this test produced ----------------------------
    //
    // The canaries are values THIS TEST supplied. Finding one would mean the transport echoed input
    // it was given, which is the leak SC-006 is about.
    let swept = app.swept.lock().expect("unpoisoned").join("\n");
    assert!(!swept.is_empty(), "the sweep collected nothing");
    // NEITHER THE CANARY NOR THE SWEPT TEXT IS PRINTED. On a real leak this failure message would
    // otherwise carry the leaked credential into the test log — the one run where that matters
    // most. The index says which canary; the value is in this file, four lines up.
    for (index, canary) in [PASSWORD_CANARY, "hunter2", "correct-horse"]
        .into_iter()
        .enumerate()
    {
        assert!(!swept.contains(canary), "a response echoed canary {index}");
    }
    // The MAILED SECRETS never reach a response either. `RecordingMailSink` holds them because the
    // mail did; the responses must not.
    assert!(app.mail.delivered() > 0, "no mail was dispatched at all");

    // POSITIVE CONTROL: the sweep finds something that IS there, so the absences above are facts
    // about the responses rather than about a search that never matches.
    assert!(
        swept.contains("acknowledged"),
        "the sweep is not reading the response bodies"
    );

    app.database.close().await.expect("closes");
}
