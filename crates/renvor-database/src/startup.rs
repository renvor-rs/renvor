//! What an operator is told when a database fails to start.
//!
//! # The problem this solves
//!
//! A provider that fails at boot used to return its [`DatabaseError`](crate::DatabaseError)
//! unchanged. That error is safe — it carries a kind and nothing else — but it is not a
//! *diagnostic*: it does not say **which** database failed, at **which** point, or what the
//! operator should do next. On a machine running one PostgreSQL and one MySQL, "the connection
//! attempt failed" identifies neither.
//!
//! `PLAN.md` §Phase 008 accepts the phase only when *"startup diagnostics identify the selected
//! provider and safe corrective action"*. This type is both halves.
//!
//! # Why it cannot leak a secret
//!
//! **Structurally, not by filtering.** The struct has four fields and none of them can hold caller
//! text:
//!
//! | Field | Type | Where the value comes from |
//! |---|---|---|
//! | `adapter` | `&'static str` | the adapter crate's own name, a literal in this repository |
//! | `database` | [`DatabaseKind`] | a two-variant enum |
//! | `phase` | [`StartupPhase`] | a three-variant enum |
//! | `kind` | [`DatabaseErrorKind`] | a fieldless enum |
//!
//! There is no `String`, no `source`, and no constructor that accepts one. A DSN, a password, a
//! token, a SQL statement, and the name of a credential-bearing environment variable are all
//! *unrepresentable* here — not scrubbed on the way out, which is a filter someone can forget to
//! apply to a new field.
//!
//! The set of renderable diagnostics is therefore **finite**, and
//! `no_diagnostic_can_render_a_secret` enumerates every one of them rather than sampling.
//!
//! # Why the adapter is named by its crate rather than by an ORM name
//!
//! `renvor-sqlx` and `renvor-seaorm` are facts about what is running. The vocabulary a project
//! manifest uses for the same choice is settled elsewhere and is not repeated here, because a
//! second spelling of the same concept is how two authorities start disagreeing.

use core::fmt;

use crate::{DatabaseErrorKind, DatabaseKind};

/// How far startup had got when it failed.
///
/// Three points, because they call for three different responses: an unreachable server, a server
/// that answered but is not usable, and a schema change that did not apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum StartupPhase {
    /// Opening the pool.
    Connect,
    /// The readiness check, after the pool opened.
    Readiness,
    /// Applying migrations on boot.
    Migration,
}

impl StartupPhase {
    /// Every phase, for exhaustive tests.
    pub const ALL: [Self; 3] = [Self::Connect, Self::Readiness, Self::Migration];

    /// What was being attempted, as a clause.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Connect => "opening the connection pool",
            Self::Readiness => "checking that the database was ready",
            Self::Migration => "applying migrations on boot",
        }
    }
}

/// A startup failure, named and actionable, carrying nothing that could be a secret.
///
/// Returned by both adapters' providers. See the module documentation for why it cannot leak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupDiagnostic {
    adapter: &'static str,
    database: DatabaseKind,
    phase: StartupPhase,
    kind: DatabaseErrorKind,
}

impl StartupDiagnostic {
    /// Records a startup failure.
    ///
    /// `adapter` is the adapter crate's own name and must be a literal. It is `&'static str` so
    /// that a caller cannot pass a formatted string built from configuration.
    #[must_use]
    pub const fn new(
        adapter: &'static str,
        database: DatabaseKind,
        phase: StartupPhase,
        kind: DatabaseErrorKind,
    ) -> Self {
        Self {
            adapter,
            database,
            phase,
            kind,
        }
    }

    /// The adapter that was running.
    #[must_use]
    pub const fn adapter(&self) -> &'static str {
        self.adapter
    }

    /// Which database it was talking to.
    #[must_use]
    pub const fn database(&self) -> DatabaseKind {
        self.database
    }

    /// How far it got.
    #[must_use]
    pub const fn phase(&self) -> StartupPhase {
        self.phase
    }

    /// What went wrong.
    #[must_use]
    pub const fn kind(&self) -> DatabaseErrorKind {
        self.kind
    }

    /// What the operator should do next.
    ///
    /// # Every kind gets one, and none of them says "check the logs"
    ///
    /// An action that sends the reader somewhere else is not an action. Each string below names a
    /// **specific next step**.
    ///
    /// The match has **no catch-all arm**, and it compiles: `#[non_exhaustive]` does not apply
    /// within the crate that declares the enum, so this file sees every variant. Adding a kind to
    /// `error.rs` therefore breaks *this* build until somebody decides what to advise — which is
    /// the point. A `_` arm here would have silently handed every future kind the same shrug.
    ///
    /// None of them quotes configuration. "Check the configured user" is safe; printing the user
    /// is a step towards printing the password beside it.
    #[must_use]
    pub const fn corrective_action(&self) -> &'static str {
        match self.kind {
            DatabaseErrorKind::ConnectFailed => {
                "Confirm the server is running and reachable from this host, and that the \
                 configured address and port are the ones it listens on. Nothing about the \
                 connection string is printed here on purpose."
            }
            DatabaseErrorKind::AcquireTimeout => {
                "The pool opened but had no free connection within its acquire timeout. Raise \
                 `max_connections`, shorten the transactions holding them, or raise the timeout \
                 — in that order of preference."
            }
            DatabaseErrorKind::NotReady => {
                "The server accepted the connection but reported it is not ready. If it was just \
                 started, it is still initialising; wait and retry rather than reconfiguring."
            }
            DatabaseErrorKind::PoolClosed => {
                "The pool was already closed when startup used it, which means shutdown ran \
                 first. Check for a cancelled boot rather than a database problem."
            }
            DatabaseErrorKind::MigrationChecksumMismatch => {
                "An already-applied migration file no longer matches what was applied. Restore \
                 the original file — never edit an applied migration — and express the change as \
                 a new one."
            }
            DatabaseErrorKind::MigrationDirty => {
                "A previous migration failed partway and the ledger is marked dirty. On MySQL its \
                 earlier statements are already committed, because DDL commits implicitly, so \
                 inspect the schema and repair forward rather than assuming a rollback happened."
            }
            DatabaseErrorKind::MigrationLockTimeout => {
                "Another process holds the migration lock. Wait for it, or find the stuck run — \
                 do not remove the lock while a migration may still be applying."
            }
            DatabaseErrorKind::MigrationIrreversible => {
                "A migration has no reverse and the configured policy requires one. Supply the \
                 down migration, or change the policy deliberately."
            }
            DatabaseErrorKind::MigrationFailed => {
                "A migration statement was refused by the server. Run it by hand against a copy \
                 to see the server's own message; it is not repeated here because it would carry \
                 the statement text."
            }
            DatabaseErrorKind::DeadlineExceeded => {
                "Startup ran out of time. If migrations run on boot, the provider deadline must \
                 exceed the migration lock and run bounds combined — see `required_boot_deadline`."
            }
            DatabaseErrorKind::Cancelled => {
                "Startup was cancelled, usually by shutdown arriving first. Not a database fault."
            }
            DatabaseErrorKind::TypeMismatch => {
                "The schema does not match what the code expects. Confirm the migrations that ran \
                 are the ones this build was written against."
            }
            // The remaining kinds describe statement-level outcomes. Reaching one during startup
            // means a migration or the readiness check produced it, and the advice is the same:
            // the schema is not what this build expects.
            DatabaseErrorKind::StatementRejected
            | DatabaseErrorKind::UniqueViolation
            | DatabaseErrorKind::ForeignKeyViolation
            | DatabaseErrorKind::NotNullViolation
            | DatabaseErrorKind::CheckViolation
            | DatabaseErrorKind::TransactionConflict
            | DatabaseErrorKind::NotFound
            | DatabaseErrorKind::CommitFailed
            | DatabaseErrorKind::RollbackFailed => {
                "A statement run during startup was refused. Confirm the database holds the schema \
                 this build's migrations produce, and that nothing else is writing to it."
            }
            DatabaseErrorKind::Unclassified => {
                "The driver reported a failure this build does not recognise. Treat it as \
                 unresolved: do not retry blindly, and capture the server's own log for the \
                 moment of the failure."
            }
        }
    }
}

impl fmt::Display for StartupDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} could not start its {} database while {}: {}. What to do: {}",
            self.adapter,
            self.database.as_str(),
            self.phase.as_str(),
            self.kind.as_str(),
            self.corrective_action()
        )
    }
}

impl core::error::Error for StartupDiagnostic {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adapter names both providers pass. Kept here so the exhaustive test below covers
    /// exactly what ships.
    const ADAPTERS: [&str; 2] = ["renvor-sqlx", "renvor-seaorm"];

    /// Every renderable diagnostic, which is a finite set.
    fn every_diagnostic() -> Vec<StartupDiagnostic> {
        let mut all = Vec::new();
        for adapter in ADAPTERS {
            for database in DatabaseKind::ALL {
                for phase in StartupPhase::ALL {
                    for kind in DatabaseErrorKind::ALL {
                        all.push(StartupDiagnostic::new(adapter, database, phase, kind));
                    }
                }
            }
        }
        all
    }

    /// No diagnostic this crate can construct renders anything that could be a secret.
    ///
    /// # Exhaustive rather than sampled
    ///
    /// Every field is an enum or a `&'static str` chosen here, so the renderable set is finite and
    /// small. This builds all of it. A redaction test that checked one example would pass while a
    /// single unlucky variant leaked.
    #[test]
    fn no_diagnostic_can_render_a_secret() {
        // Shapes rather than values: a DSN contains `://` and usually `@`, a password assignment
        // contains `=`, and a leaked statement contains a SQL keyword.
        const FORBIDDEN: [&str; 12] = [
            "://",
            "@",
            "password",
            "passwd",
            "secret",
            "token",
            "DATABASE_URL",
            "SELECT ",
            "INSERT ",
            "UPDATE ",
            "DELETE ",
            "ALTER ",
        ];

        let all = every_diagnostic();
        assert!(
            all.len() >= 2 * 2 * 3,
            "the enumeration built almost nothing"
        );

        for diagnostic in all {
            let rendered = format!("{diagnostic} {diagnostic:?}");
            for leak in FORBIDDEN {
                assert!(
                    !rendered.contains(leak),
                    "a startup diagnostic rendered `{leak}`: {rendered}"
                );
            }
        }
    }

    /// Every kind has advice, and the advice is a real instruction.
    #[test]
    fn every_kind_carries_a_corrective_action() {
        for kind in DatabaseErrorKind::ALL {
            let diagnostic = StartupDiagnostic::new(
                "renvor-sqlx",
                DatabaseKind::Postgres,
                StartupPhase::Connect,
                kind,
            );
            let action = diagnostic.corrective_action();
            assert!(
                action.len() > 40,
                "{kind:?} has no real corrective action: {action:?}"
            );
            assert!(
                !action.to_ascii_lowercase().contains("check the logs"),
                "{kind:?}'s advice defers to the logs instead of naming a next step"
            );
        }
    }

    /// The rendering names the adapter, the database, and the phase — the three facts that make
    /// one failure distinguishable from another on a host running both engines.
    #[test]
    fn a_diagnostic_identifies_which_provider_failed() {
        let diagnostic = StartupDiagnostic::new(
            "renvor-seaorm",
            DatabaseKind::MySql,
            StartupPhase::Migration,
            DatabaseErrorKind::MigrationDirty,
        );
        let rendered = diagnostic.to_string();
        assert!(rendered.contains("renvor-seaorm"), "{rendered}");
        assert!(rendered.contains("mysql"), "{rendered}");
        assert!(
            rendered.contains("applying migrations on boot"),
            "{rendered}"
        );
        assert!(rendered.contains("What to do:"), "{rendered}");

        // CONTROL: the other adapter and the other engine are NOT named, so the assertions above
        // are reading this diagnostic rather than a constant that mentions everything.
        assert!(!rendered.contains("renvor-sqlx"), "{rendered}");
        assert!(!rendered.contains("postgres"), "{rendered}");
    }
}
