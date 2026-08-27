//! What an operator is told when a database fails to start.
//!
//! # The problem this solves
//!
//! A provider that fails at boot used to return its [`DatabaseError`]
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
//! **Structurally, not by filtering.** Every field is a fieldless enum, and none of them can hold
//! caller text:
//!
//! | Field | Type | Where the value comes from |
//! |---|---|---|
//! | `adapter` | [`DatabaseAdapter`] | a two-variant enum |
//! | `database` | [`DatabaseKind`] | a two-variant enum |
//! | `phase` | [`StartupPhase`] | a three-variant enum |
//! | `cause` | [`DatabaseError`] | one fieldless-enum field, and nothing else |
//!
//! There is no `String` and no constructor that accepts one. A DSN, a password, a token, a SQL
//! statement, and the name of a credential-bearing environment variable are all *unrepresentable*
//! here — not scrubbed on the way out, which is a filter someone can forget to apply to a new
//! field.
//!
//! The set of renderable diagnostics is therefore **finite**, and
//! `no_diagnostic_can_render_a_secret` enumerates every one of them rather than sampling.
//!
//! ## `&'static str` was not enough, and this is the escape it allowed
//!
//! `adapter` was a `&'static str` until Phase 008's correction cycle, with a comment saying it
//! *"must be a literal"*. `'static` is a lifetime, not a provenance: `Box::leak` promotes any
//! runtime `String` — a formatted DSN included — to `&'static str`, and the constructor accepted
//! it. A test that built exactly that leak rendered `hunter2` out of a type documented as unable
//! to carry one.
//!
//! [`DatabaseAdapter`] closes it by construction rather than by asking callers not to. The
//! `compile_fail` control on [`StartupDiagnostic::new`] is what keeps the closure honest.
//!
//! # Why the adapter is named by its crate rather than by an ORM name
//!
//! `renvor-sqlx` and `renvor-seaorm` are facts about what is running. The vocabulary a project
//! manifest uses for the same choice is settled elsewhere and is not repeated here, because a
//! second spelling of the same concept is how two authorities start disagreeing.

use core::fmt;

use crate::{DatabaseError, DatabaseErrorKind, DatabaseKind};

crate::closed_named_enum! {
    /// Which Renvor persistence adapter was starting.
    ///
    /// # A closed set, because the field it fills is rendered
    ///
    /// The two variants are the two adapter crates this workspace ships. A caller cannot introduce
    /// a third, and — the point of the type — cannot introduce a *string*. See the module
    /// documentation for the `Box::leak` escape the previous `&'static str` field allowed.
    ///
    /// `as_str` returns the adapter's **crate name**.
    ///
    /// # Declared rather than written out
    ///
    /// The enum, [`ALL`](DatabaseAdapter::ALL) and [`as_str`](DatabaseAdapter::as_str) come from
    /// one list, through [`closed_named_enum`](crate::closed_named_enum). They were three separate
    /// authorities until a review found that a variant added to the enum and **omitted from
    /// `ALL`** passed every test in this file — because every test reads `ALL`. That macro's
    /// documentation carries the mutation and both of its controls.
    pub enum DatabaseAdapter {
        /// `renvor-sqlx` — the direct-SQLx adapter.
        Sqlx => "renvor-sqlx",
        /// `renvor-seaorm` — the SeaORM adapter.
        SeaOrm => "renvor-seaorm",
    }
}

crate::closed_named_enum! {
    /// How far startup had got when it failed.
    ///
    /// Three points, because they call for three different responses: an unreachable server, a
    /// server that answered but is not usable, and a schema change that did not apply.
    ///
    /// `as_str` returns what was being attempted, as a clause. Declared through
    /// [`closed_named_enum`](crate::closed_named_enum) for the same reason as
    /// [`DatabaseAdapter`]: this type is rendered into a diagnostic, so its variant list and its
    /// rendered names must be one authority rather than two.
    pub enum StartupPhase {
        /// Opening the pool.
        Connect => "opening the connection pool",
        /// The readiness check, after the pool opened.
        Readiness => "checking that the database was ready",
        /// Applying migrations on boot.
        Migration => "applying migrations on boot",
    }
}

/// A startup failure, named and actionable, carrying nothing that could be a secret.
///
/// Returned by both adapters' providers. See the module documentation for why it cannot leak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupDiagnostic {
    adapter: DatabaseAdapter,
    database: DatabaseKind,
    phase: StartupPhase,
    /// The normalised failure this diagnostic wraps, kept whole rather than reduced to its kind.
    ///
    /// This is the link [`core::error::Error::source`] hands back, and it is where the chain
    /// deliberately **ends**: [`DatabaseError`] implements no `source` of its own, so the driver's
    /// error and its uncontrolled text are not reachable from here.
    cause: DatabaseError,
}

impl StartupDiagnostic {
    /// Records a startup failure.
    ///
    /// # Caller text cannot reach the adapter field, and this proves it
    ///
    /// `adapter` is a [`DatabaseAdapter`], so the only values that exist are the ones this crate
    /// declares. The negative control is a compile failure:
    ///
    /// ```compile_fail
    /// use renvor_database::{
    ///     DatabaseError, DatabaseErrorKind, DatabaseKind, StartupDiagnostic, StartupPhase,
    /// };
    ///
    /// // `Box::leak` is the exact escape the previous `&'static str` field allowed: it promotes
    /// // runtime text — here a DSN carrying a password — to `&'static str`.
    /// let leaked: &'static str =
    ///     Box::leak(format!("postgres://renvor:{}@db.internal/app", "hunter2").into_boxed_str());
    ///
    /// let _ = StartupDiagnostic::new(
    ///     leaked,
    ///     DatabaseKind::Postgres,
    ///     StartupPhase::Connect,
    ///     DatabaseError::new(DatabaseErrorKind::ConnectFailed),
    /// );
    /// ```
    ///
    /// A `compile_fail` block passes when compilation fails **for any reason**, so on its own it
    /// would also pass if the snippet were merely misspelled. This one compiles and runs, and
    /// differs only in the argument under test:
    ///
    /// ```
    /// use renvor_database::{
    ///     DatabaseAdapter, DatabaseError, DatabaseErrorKind, DatabaseKind, StartupDiagnostic,
    ///     StartupPhase,
    /// };
    ///
    /// let leaked: &'static str =
    ///     Box::leak(format!("postgres://renvor:{}@db.internal/app", "hunter2").into_boxed_str());
    /// assert!(leaked.contains("hunter2"));
    ///
    /// let diagnostic = StartupDiagnostic::new(
    ///     DatabaseAdapter::Sqlx,
    ///     DatabaseKind::Postgres,
    ///     StartupPhase::Connect,
    ///     DatabaseError::new(DatabaseErrorKind::ConnectFailed),
    /// );
    /// assert!(!diagnostic.to_string().contains("hunter2"));
    /// ```
    #[must_use]
    pub const fn new(
        adapter: DatabaseAdapter,
        database: DatabaseKind,
        phase: StartupPhase,
        cause: DatabaseError,
    ) -> Self {
        Self {
            adapter,
            database,
            phase,
            cause,
        }
    }

    /// The adapter that was running.
    #[must_use]
    pub const fn adapter(&self) -> DatabaseAdapter {
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
        self.cause.kind()
    }

    /// The safe, normalised error this diagnostic was built from.
    ///
    /// # Why the chain is kept here and cut one link later
    ///
    /// C-E2 requires a preserved causal chain, and the previous version of this type flattened it:
    /// it kept the *kind* and discarded the [`DatabaseError`], so `source` answered `None` and the
    /// diagnostic was the whole story.
    ///
    /// What C-E2 asks to be preserved is a **Renvor** cause. The link *below* this one — the
    /// driver's own error, carrying its own text — is terminated deliberately, and by construction
    /// rather than by policy: [`DatabaseError`] holds one fieldless-enum field and implements no
    /// `source`, so there is nothing further to reach. Preserving a safe framework cause and
    /// refusing an unsafe third-party one are different acts, and this type performs both.
    #[must_use]
    pub const fn cause(&self) -> DatabaseError {
        self.cause
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
        match self.cause.kind() {
            DatabaseErrorKind::ConnectFailed => {
                // This kind covers FIVE distinct causes — an unreachable server, an unknown
                // database, a rejected user, a rejected password, and a server refusing because
                // it is at its connection limit. `classify_connect_error` folds a server-side
                // handshake refusal in here deliberately, so advice naming only reachability
                // sends four readers out of five to the wrong place.
                //
                // Each item names WHAT TO CHECK, never its value. "The configured user" is safe;
                // printing the user is a step towards printing the password beside it.
                "The connection was not established. Check, in this order: that the server is \
                 running and reachable from this host on the configured address and port; that \
                 the configured database name exists on it; that the configured user exists and \
                 its credentials are the ones the server accepts; and that the server is not \
                 already at its connection limit, which it refuses in the same way. None of \
                 those values is printed here on purpose."
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
            self.adapter.as_str(),
            self.database.as_str(),
            self.phase.as_str(),
            self.cause.kind().as_str(),
            self.corrective_action()
        )
    }
}

impl core::error::Error for StartupDiagnostic {
    /// The safe normalised cause. See [`StartupDiagnostic::cause`] for why the chain stops there.
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every renderable diagnostic, which is a finite set.
    ///
    /// The adapter dimension is [`DatabaseAdapter::ALL`] rather than a list of names repeated
    /// here. A second list would be a second authority, and the one that got a new adapter first
    /// would be the one the other stopped agreeing with.
    fn every_diagnostic() -> Vec<StartupDiagnostic> {
        let mut all = Vec::new();
        for adapter in DatabaseAdapter::ALL {
            for database in DatabaseKind::ALL {
                for phase in StartupPhase::ALL {
                    for kind in DatabaseErrorKind::ALL {
                        all.push(StartupDiagnostic::new(
                            adapter,
                            database,
                            phase,
                            DatabaseError::new(kind),
                        ));
                    }
                }
            }
        }
        all
    }

    /// Every adapter renders one of a closed, stated set of crate names.
    ///
    /// # Why a second list here is deliberate, and not the duplicate authority it looks like
    ///
    /// [`every_diagnostic`] draws its adapters from [`DatabaseAdapter::ALL`] precisely so it does
    /// not restate them. This test restates them on purpose, because it is asserting a different
    /// thing: not *which* adapters exist, but that **whatever exists renders a reviewed literal**.
    ///
    /// Mutation **M-24** is why it is here. Closing the constructor to an enum stops a *caller*
    /// passing text; it does not stop a *maintainer* re-opening the hole by adding
    /// `Custom(&'static str)`. That mutation was run, and every test in this file passed —
    /// `as_str`'s catch-all-free match forced the author to handle the variant, and returning the
    /// string satisfied it. The enumeration then covered the new variant and found nothing,
    /// because the literal chosen was benign.
    ///
    /// # The claim that used to follow this was false, and a review caught it
    ///
    /// It said *"a variant that carries caller text cannot be made to pass at all"*. It could.
    /// M-24 added `Custom(&'static str)` **to `ALL`**, which is what this test caught. The variant
    /// of the mutation that **omits it from `ALL`** — **M-24b** — passed all fifty-nine tests in
    /// this crate, because `ALL` was a hand-maintained restatement of the variant list and every
    /// test here reads `ALL`. A variant absent from `ALL` was a variant no assertion could reach.
    ///
    /// The correction is in the declaration, not in this test:
    /// [`closed_named_enum`] generates the enum, `ALL` and `as_str` from
    /// one list, so "present in the enum but absent from `ALL`" is no longer expressible and a
    /// data-bearing variant is a macro error. Re-running M-24b against the declared form now fails
    /// with *"no rules expected `(`"* before any test runs.
    ///
    /// This test still earns its place, and its job is now the one it can actually do: a genuine
    /// third adapter — a **unit** variant, which the declaration does accept — reaches `ALL`
    /// automatically and fails here until somebody reviews the name it renders. That is **M-24c**,
    /// and it is killed by the assertions below.
    #[test]
    fn no_adapter_can_render_anything_but_its_own_crate_name() {
        const REVIEWED_NAMES: [&str; 2] = ["renvor-sqlx", "renvor-seaorm"];

        assert_eq!(
            DatabaseAdapter::ALL.len(),
            REVIEWED_NAMES.len(),
            "an adapter was added without a decision about what it is allowed to render"
        );
        for (index, adapter) in DatabaseAdapter::ALL.into_iter().enumerate() {
            assert!(
                REVIEWED_NAMES.contains(&adapter.as_str()),
                "the adapter at index {index} renders a name nobody reviewed"
            );
        }
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
        assert_eq!(
            all.len(),
            DatabaseAdapter::ALL.len()
                * DatabaseKind::ALL.len()
                * StartupPhase::ALL.len()
                * DatabaseErrorKind::ALL.len(),
            "the enumeration did not build the whole renderable set"
        );

        for diagnostic in all {
            // Display, Debug, AND the whole source chain. Correction B gave this type a `source`,
            // and a redaction proof that stopped at the top link would have stopped proving the
            // thing it is named for the moment the chain grew.
            let mut rendered = format!("{diagnostic} {diagnostic:?}");
            let mut link: Option<&(dyn core::error::Error + 'static)> =
                core::error::Error::source(&diagnostic);
            let mut depth = 0usize;
            while let Some(error) = link {
                rendered.push(' ');
                rendered.push_str(&format!("{error} {error:?}"));
                link = error.source();
                depth += 1;
                assert!(depth < 8, "the causal chain does not terminate");
            }
            // CONTROL: the chain is one link long, and it is the safe normalised error. A `source`
            // that had quietly started returning `None` would make the traversal above vacuous.
            assert_eq!(
                depth, 1,
                "the diagnostic no longer preserves exactly one safe cause"
            );

            for (index, leak) in FORBIDDEN.into_iter().enumerate() {
                // The index, never the value, and never the rendering. This file plants a
                // password in a documentation example, so it is a credential-handling file and
                // `renvor-core`'s `diagnostics` suite forbids a message that would print what it
                // is asserting about — on a redaction regression that message is the one place
                // the leak would surface.
                assert!(
                    !rendered.contains(leak),
                    "a startup diagnostic rendered forbidden substring {index}"
                );
            }
        }
    }

    /// RED (Correction B). C-E2 requires a preserved causal chain. The diagnostic keeps only the
    /// kind, so the `DatabaseError` the provider actually failed with is discarded and
    /// `Error::source` answers `None` — a flattened chain, which is what C-E2 forbids.
    #[test]
    fn the_safe_cause_survives_as_a_source() {
        let diagnostic = StartupDiagnostic::new(
            DatabaseAdapter::Sqlx,
            DatabaseKind::Postgres,
            StartupPhase::Connect,
            DatabaseError::new(DatabaseErrorKind::ConnectFailed),
        );
        let source = core::error::Error::source(&diagnostic)
            .expect("a startup diagnostic must preserve the safe error it was built from");
        let cause = source
            .downcast_ref::<crate::DatabaseError>()
            .expect("the immediate cause must be the normalised DatabaseError");
        assert_eq!(cause.kind(), DatabaseErrorKind::ConnectFailed);
    }

    /// Every kind has advice, and the advice is a real instruction.
    #[test]
    fn every_kind_carries_a_corrective_action() {
        for (index, kind) in DatabaseErrorKind::ALL.into_iter().enumerate() {
            let diagnostic = StartupDiagnostic::new(
                DatabaseAdapter::Sqlx,
                DatabaseKind::Postgres,
                StartupPhase::Connect,
                DatabaseError::new(kind),
            );
            let action = diagnostic.corrective_action();
            assert!(
                action.len() > 40,
                "the kind at index {index} has no real corrective action"
            );
            assert!(
                !action.to_ascii_lowercase().contains("check the logs"),
                "the kind at index {index} defers to the logs instead of naming a next step"
            );
        }
    }

    /// RED (Correction C). `ConnectFailed` is returned for an unreachable server, a rejected
    /// user, a rejected password, an unknown database, and a server that refused because it is at
    /// its connection limit. The advice names only the first, so four of the five causes send the
    /// reader looking in the wrong place.
    #[test]
    fn connect_failure_advice_covers_every_cause_it_can_have() {
        let action = StartupDiagnostic::new(
            DatabaseAdapter::Sqlx,
            DatabaseKind::Postgres,
            StartupPhase::Connect,
            DatabaseError::new(DatabaseErrorKind::ConnectFailed),
        )
        .corrective_action();
        let lower = action.to_ascii_lowercase();
        for (index, topic) in [
            "reachable",
            "database name",
            "user",
            "credential",
            "connection limit",
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                lower.contains(topic),
                "the connect-failure advice never mentions the cause at index {index}"
            );
        }
    }

    /// The rendering names the adapter, the database, and the phase — the three facts that make
    /// one failure distinguishable from another on a host running both engines.
    #[test]
    fn a_diagnostic_identifies_which_provider_failed() {
        let diagnostic = StartupDiagnostic::new(
            DatabaseAdapter::SeaOrm,
            DatabaseKind::MySql,
            StartupPhase::Migration,
            DatabaseError::new(DatabaseErrorKind::MigrationDirty),
        );
        // Fixed messages, never the rendering: see `no_diagnostic_can_render_a_secret`.
        let rendered = diagnostic.to_string();
        assert!(
            rendered.contains("renvor-seaorm"),
            "the diagnostic did not name the adapter"
        );
        assert!(
            rendered.contains("mysql"),
            "the diagnostic did not name the database"
        );
        assert!(
            rendered.contains("applying migrations on boot"),
            "the diagnostic did not name the phase"
        );
        assert!(
            rendered.contains("What to do:"),
            "the diagnostic offered no corrective action"
        );

        // CONTROL: the other adapter and the other engine are NOT named, so the assertions above
        // are reading this diagnostic rather than a constant that mentions everything.
        assert!(
            !rendered.contains("renvor-sqlx"),
            "the diagnostic named the adapter that did not fail"
        );
        assert!(
            !rendered.contains("postgres"),
            "the diagnostic named the database that did not fail"
        );
    }
}
