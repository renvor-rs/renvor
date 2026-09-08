//! The proof that this project generates, formats, compiles, migrates, starts,
//! answers, and shuts down cleanly — against the real services,
//! the way the framework proves itself.
//!
//! # Skips honestly, fails loudly
//!
//! Without `RENVOR_DATABASE_URL` this test prints `SKIPPED` and passes, so `cargo test` works on
//! a machine with no services. With `RENVOR_TEST_REQUIRE_DATABASE` set, a missing service is a
//! **failure**: that is the rule the framework's own suites apply, and it is what keeps a green
//! run from meaning "nothing ran".
//!
//! # It drives the real binary over a real socket
//!
//! The application is spawned as a child process with the same environment a deployment would
//! set, waited on through `/readyz`, driven over loopback HTTP with the standard library alone,
//! and then sent the interrupt a terminal would send. The exit status is the last assertion.

mod support;

use support::*;

#[tokio::test]
async fn the_starter_starts_answers_and_stops_cleanly() {
    let Ok(url) = std::env::var("RENVOR_DATABASE_URL") else {
        assert!(
            std::env::var("RENVOR_TEST_REQUIRE_DATABASE").is_err(),
            "RENVOR_TEST_REQUIRE_DATABASE is set and RENVOR_DATABASE_URL is not"
        );
        println!("SKIPPED: set RENVOR_DATABASE_URL to run the starter test");
        return;
    };
    reset(&url).await;

    let mut app = start();
    let address = app.address.clone();

    // ── 0. the ledger holds one row per shipped migration ───────────────────────────
    let applied = applied_migrations(&url).await;
    assert_eq!(
        applied.len(),
        shipped_migrations(),
        "the ledger must hold one row per shipped migration: {applied:?}"
    );

    // ── 1. it names itself ──────────────────────────────────────────────────────────
    let reply = http(&address, "GET", "/", &[], "");
    assert_eq!(reply.status, 200);
    assert!(reply.body.contains(NAME), "{}", reply.body);
    assert!(
        reply.header("content-type").is_some(),
        "the answer declares its type"
    );

    // ── 2. the example domain ────────────────────────────────────
    let reply = http(&address, "GET", "/items", &[], "");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let created = http(&address, "POST", "/items", &[], r#"{"name":"third"}"#);
    assert_eq!(created.status, 201, "{}", created.body);

    // ── finally: the interrupt a terminal sends, and a clean exit ─────────────────
    let stopped = app.stop();
    if let Some(status) = stopped {
        assert!(
            status.success(),
            "the application did not exit cleanly: {status}"
        );
    } else {
        // Windows has no SIGINT to send a child from a test: the process was ended, and the
        // clean-shutdown path is proven on the platforms that can send one.
        println!("SKIPPED: no interrupt can be sent to a child on this platform");
    }
}
