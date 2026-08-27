//! Nothing the driver said reaches telemetry — asserted by entering a subscriber and reading back.
//!
//! # Why this file exists
//!
//! `classify_db_error` and `classify_error` used to emit `driver_error = %error` at `debug`, and
//! the module's own prose defended it: *"the original text ... reaches operators rather than
//! callers"*.
//! `CONSTITUTION.md` principle VI grants no such right. It forbids secrets in *"repositories,
//! generated manifests, logs, telemetry, URLs, browser bundles, desktop resources, examples,
//! fixtures, or snapshots"* — telemetry named explicitly, with no operator carve-out and no
//! severity threshold. `debug` is a level, not an exemption.
//!
//! A [`sea_orm::DbErr`]'s `Display` is SeaORM's own string, and SeaORM's is often the driver's
//! wrapped. It carries the offending value, the table and column, the GENERATED SQL — which this
//! adapter produces rather than the caller — and, for a connection failure, the host and the user.
//! Formatting one into a field is not "recording context"; it is copying an unbounded third-party
//! string into a sink this crate does not control the retention, routing, or redaction of.
//!
//! # Why a hand-written subscriber
//!
//! `tracing-subscriber` would do this in a few lines and is deliberately **not** added for it.
//! `crates/renvor-http/tests/telemetry.rs` made the same call for the same reason: the `Subscriber`
//! trait lives in `tracing`, which this crate already depends on, so the capture costs nothing
//! beyond this file. Reading fields back as name/value pairs is also stricter than searching
//! formatted output — an assertion on `adapter` matches a field, not a substring that happens to
//! appear in a message.

use std::sync::{Arc, Mutex, PoisonError};

use renvor_database::{DatabaseError, DatabaseErrorKind};
use renvor_seaorm::error::{classify_connect_error, classify_db_error, classify_error};
use sea_orm::{DbErr, RuntimeErr};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Metadata, Subscriber};

// ---------------------------------------------------------------------------------------------
// The planted secrets
// ---------------------------------------------------------------------------------------------

/// A password. Written whole on purpose: `crates/renvor-core/tests/diagnostics.rs` selects
/// credential-handling files by searching for this literal, and a file that plants a password must
/// be inside that gate rather than outside it.
const CREDENTIAL_CANARY: &str = "hunter2CanaryDoNotLeak";

/// A host. A connection failure is the one case where the driver's text names the server.
const HOST_CANARY: &str = "db.internal.example";

/// A statement, with a value inside it. Both halves are secrets of different kinds: the SQL is
/// schema, the literal is user data.
const SQL_CANARY: &str = "INSERT INTO rv_widget (name) VALUES ('LEAKED-TAIL')";

/// Every planted string, as one list, so a new case cannot forget one.
fn canaries() -> [&'static str; 5] {
    [
        CREDENTIAL_CANARY,
        HOST_CANARY,
        SQL_CANARY,
        "LEAKED-TAIL",
        // The driver's own wording, not a planted value. A message reproduced verbatim minus the
        // secret is still the driver's text, and still unbounded.
        "password",
    ]
}

// ---------------------------------------------------------------------------------------------
// The capture
// ---------------------------------------------------------------------------------------------

/// Collects field name/value pairs, whichever way `tracing` chooses to hand them over.
struct Fields(Vec<(String, String)>);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name().to_owned(), value.to_owned()));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.push((field.name().to_owned(), value.to_string()));
    }
}

/// One captured event, as the field name/value pairs it carried — the message included, which
/// `tracing` records under the field name `message`.
type CapturedEvent = Vec<(String, String)>;

#[derive(Clone, Default)]
struct Recorder {
    seen: Arc<Mutex<Vec<CapturedEvent>>>,
}

impl Subscriber for Recorder {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &Attributes<'_>) -> Id {
        // `Id` may not be zero. Nothing here opens a span; this exists to satisfy the trait.
        Id::from_u64(1)
    }

    fn record(&self, _: &Id, _: &Record<'_>) {}

    fn record_follows_from(&self, _: &Id, _: &Id) {}

    fn event(&self, event: &Event<'_>) {
        let mut fields = Fields(Vec::new());
        event.record(&mut fields);
        // The message is a field named `message`, so this captures it too.
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(fields.0);
    }

    fn enter(&self, _: &Id) {}

    fn exit(&self, _: &Id) {}
}

/// Runs `body` with a capturing subscriber installed for THIS THREAD only.
///
/// Thread-local rather than global on purpose: `cargo test` runs these in parallel, and a global
/// default would let one test observe another's events.
fn captured<T>(body: impl FnOnce() -> T) -> (T, Vec<CapturedEvent>) {
    let recorder = Recorder::default();
    let seen = Arc::clone(&recorder.seen);
    let guard = tracing::subscriber::set_default(recorder);
    let value = body();
    drop(guard);
    let events = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
    (value, events)
}

// ---------------------------------------------------------------------------------------------
// The errors under test
// ---------------------------------------------------------------------------------------------

fn protocol_error() -> sqlx::Error {
    sqlx::Error::Protocol(format!(
        "{SQL_CANARY} failed against {HOST_CANARY} with password {CREDENTIAL_CANARY}"
    ))
}

fn io_error() -> sqlx::Error {
    sqlx::Error::Io(std::io::Error::other(format!(
        "connection refused by {HOST_CANARY} for user renvor password {CREDENTIAL_CANARY}"
    )))
}

fn configuration_error() -> sqlx::Error {
    sqlx::Error::Configuration(Box::new(std::io::Error::other(format!(
        "postgres://renvor:{CREDENTIAL_CANARY}@{HOST_CANARY}:5432/ledger is not a valid url"
    ))))
}

/// One named invocation of a public classifier, with its secret-bearing input already bound.
type Case = (&'static str, Box<dyn Fn() -> DatabaseError>);

/// Every public classifier this crate exposes, each exercised with a secret-bearing input.
///
/// Named cases rather than one blob: a classifier added later that is not listed here shows up as
/// a count mismatch below rather than as silence.
fn db_error(inner: &str) -> DbErr {
    DbErr::Query(RuntimeErr::Internal(inner.to_owned()))
}

fn connection_db_error(inner: &str) -> DbErr {
    DbErr::Conn(RuntimeErr::Internal(inner.to_owned()))
}

fn cases() -> Vec<Case> {
    let planted =
        format!("{SQL_CANARY} failed against {HOST_CANARY} with password {CREDENTIAL_CANARY}");
    let for_query = planted.clone();
    let for_conn = planted.clone();
    let for_custom = planted.clone();
    let for_migration = planted.clone();
    let for_not_found = planted;
    vec![
        (
            "classify_db_error/query",
            Box::new(move || classify_db_error(&db_error(&for_query))),
        ),
        (
            "classify_db_error/conn",
            Box::new(move || classify_db_error(&connection_db_error(&for_conn))),
        ),
        (
            "classify_db_error/custom",
            Box::new(move || classify_db_error(&DbErr::Custom(for_custom.clone()))),
        ),
        (
            "classify_db_error/migration",
            Box::new(move || classify_db_error(&DbErr::Migration(for_migration.clone()))),
        ),
        (
            "classify_db_error/record_not_found",
            Box::new(move || classify_db_error(&DbErr::RecordNotFound(for_not_found.clone()))),
        ),
        (
            "classify_error/protocol",
            Box::new(|| classify_error(&protocol_error())),
        ),
        (
            "classify_error/io",
            Box::new(|| classify_error(&io_error())),
        ),
        (
            "classify_error/configuration",
            Box::new(|| classify_error(&configuration_error())),
        ),
        (
            "classify_connect_error/protocol",
            Box::new(|| classify_connect_error(&protocol_error())),
        ),
        (
            "classify_connect_error/io",
            Box::new(|| classify_connect_error(&io_error())),
        ),
        (
            "classify_connect_error/configuration",
            Box::new(|| classify_connect_error(&configuration_error())),
        ),
    ]
}

// ---------------------------------------------------------------------------------------------
// The property
// ---------------------------------------------------------------------------------------------

/// No public classifier copies driver text into a `tracing` field.
#[test]
fn no_public_classifier_puts_driver_text_into_telemetry() {
    let cases = cases();

    // POSITIVE CONTROL. An empty case list would make every assertion below vacuously true.
    let count = cases.len();
    assert!(
        count >= 11,
        "the case list covers only {count} classifier invocations; it is not exercising this crate"
    );

    for (index, (label, run)) in cases.iter().enumerate() {
        let (classified, events) = captured(run);

        // POSITIVE CONTROL, per case. A classifier that emitted NOTHING would satisfy every
        // absence assertion below without proving anything about redaction. The safe record is
        // required, so silence is a failure here rather than a pass.
        assert!(
            !events.is_empty(),
            "case {index} ({label}) emitted no telemetry at all, so the redaction assertions \
             below would hold vacuously"
        );

        for (position, fields) in events.iter().enumerate() {
            for (name, value) in fields {
                for (needle_index, needle) in canaries().into_iter().enumerate() {
                    // Neither the field value nor the needle is interpolated. If this fails, the
                    // value IS the leaked secret, and printing it would put the credential into
                    // the log of the failing run — the exact defect this file guards against.
                    assert!(
                        !value.contains(needle),
                        "case {index} ({label}) event {position} field {needle_index} carried \
                         a planted secret into telemetry"
                    );
                }
                // A field NAME that promises driver text is a defect even if the value is
                // currently safe, because the name is the contract the next author reads.
                assert!(
                    !name.contains("driver_error") && !name.contains("migrate_error"),
                    "case {index} ({label}) event {position} still declares a raw-error field"
                );
            }
        }

        // NOT VACUOUS. The safe fields are present, and they say what the returned value says.
        let kind = classified.kind();
        let adapter: Vec<&str> = field_values(&events, "adapter");
        let reported: Vec<&str> = field_values(&events, "database_error_kind");
        let transient: Vec<&str> = field_values(&events, "transient");

        assert!(
            adapter.iter().all(|value| *value == "renvor-seaorm"),
            "case {index} ({label}) did not name this adapter in every record"
        );
        assert!(
            !adapter.is_empty(),
            "case {index} ({label}) emitted no `adapter` field"
        );
        assert!(
            reported.iter().all(|value| *value == kind.as_str()),
            "case {index} ({label}) reported a kind in telemetry that differs from the one it \
             returned"
        );
        assert!(
            !reported.is_empty(),
            "case {index} ({label}) emitted no `database_error_kind` field"
        );
        assert!(
            transient
                .iter()
                .all(|value| *value == kind.is_transient().to_string()),
            "case {index} ({label}) reported a retryability in telemetry that differs from the \
             kind it returned"
        );
        assert!(
            !transient.is_empty(),
            "case {index} ({label}) emitted no `transient` field"
        );
    }
}

/// Every value recorded under `name`, across every captured event.
fn field_values<'a>(events: &'a [CapturedEvent], name: &str) -> Vec<&'a str> {
    events
        .iter()
        .flatten()
        .filter(|(field, _)| field == name)
        .map(|(_, value)| value.as_str())
        .collect()
}

/// A migration source that cannot be loaded does not put the path into telemetry either.
///
/// The path is the secret here. `Migrations::load` documents that *"the path is **not** carried in
/// the error: a filesystem path is an implementation detail of the deployment"* — and then logged
/// it, through `migrate_error = %inner`, where the io error names the directory it could not read.
#[tokio::test]
async fn a_migration_source_failure_does_not_put_the_path_into_telemetry() {
    use renvor_database::MigrationSettings;

    // Deliberately absent. Nothing is created or removed: `Migrator::new` fails on the lookup, and
    // the error it returns names the path it was given.
    let directory =
        std::path::PathBuf::from(format!("/nonexistent-{CREDENTIAL_CANARY}/migrations"));

    let (result, events) = {
        let recorder = Recorder::default();
        let seen = Arc::clone(&recorder.seen);
        let guard = tracing::subscriber::set_default(recorder);
        let outcome =
            renvor_seaorm::migrate::Migrations::load(&directory, MigrationSettings::default())
                .await;
        drop(guard);
        let events = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
        (outcome, events)
    };

    let error = result.expect_err("a migration directory that does not exist cannot load");
    assert_eq!(error.kind(), DatabaseErrorKind::MigrationFailed);

    assert!(
        !events.is_empty(),
        "the migration failure emitted no telemetry, so the assertion below would hold vacuously"
    );

    for (position, fields) in events.iter().enumerate() {
        for (_, value) in fields {
            assert!(
                !value.contains(CREDENTIAL_CANARY),
                "event {position} carried the migration source path into telemetry"
            );
        }
    }
}
