//! The job-store migration set, embedded (Phase 011).
//!
//! Same argument as `renvor_auth::migrations`: the generator copies this set into a generated
//! project and, by contract C-4, embeds rather than reads. Five pairs per engine (contract C-J10);
//! `tests/embedded_migrations.rs` proves the constants equal the files and `tests/migration_set.rs`
//! that the README says five.
/// The engine a set targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// PostgreSQL — `migrations/postgres`.
    Postgres,
    /// MySQL — `migrations/mysql`.
    MySql,
}

impl Engine {
    /// The name `renvor_database::DatabaseKind::as_str` uses for the same engine.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::MySql => "mysql",
        }
    }
}

/// One migration file: its name, as it must be written into a project's `migrations/`
/// directory, and its contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationFile {
    name: &'static str,
    contents: &'static str,
}

impl MigrationFile {
    /// The file name, `<version>_<description>.<up|down>.sql`.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// The SQL, byte for byte the file in this crate's `migrations/` directory.
    #[must_use]
    pub const fn contents(&self) -> &'static str {
        self.contents
    }

    /// The version prefix, which is what orders the set.
    #[must_use]
    pub fn version(&self) -> &'static str {
        self.name.split('_').next().unwrap_or(self.name)
    }
}

/// One engine's complete set, in name order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineSet {
    engine: Engine,
    files: &'static [MigrationFile],
}

impl EngineSet {
    /// The engine.
    #[must_use]
    pub const fn engine(&self) -> Engine {
        self.engine
    }

    /// Every file, sorted by name, ups and downs interleaved as the directory holds them.
    #[must_use]
    pub const fn files(&self) -> &'static [MigrationFile] {
        self.files
    }
}

/// The set for an engine named as `renvor_database::DatabaseKind` names it, or `None`.
#[must_use]
pub fn for_engine(name: &str) -> Option<&'static EngineSet> {
    match name {
        "postgres" => Some(postgres()),
        "mysql" => Some(mysql()),
        _ => None,
    }
}

const POSTGRES: &[MigrationFile] = &[
    MigrationFile {
        name: "20260904000001_create_job.down.sql",
        contents: include_str!("../migrations/postgres/20260904000001_create_job.down.sql"),
    },
    MigrationFile {
        name: "20260904000001_create_job.up.sql",
        contents: include_str!("../migrations/postgres/20260904000001_create_job.up.sql"),
    },
    MigrationFile {
        name: "20260904000002_unique_job_idempotency.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260904000002_unique_job_idempotency.down.sql"
        ),
    },
    MigrationFile {
        name: "20260904000002_unique_job_idempotency.up.sql",
        contents: include_str!(
            "../migrations/postgres/20260904000002_unique_job_idempotency.up.sql"
        ),
    },
    MigrationFile {
        name: "20260904000003_index_job_claim.down.sql",
        contents: include_str!("../migrations/postgres/20260904000003_index_job_claim.down.sql"),
    },
    MigrationFile {
        name: "20260904000003_index_job_claim.up.sql",
        contents: include_str!("../migrations/postgres/20260904000003_index_job_claim.up.sql"),
    },
    MigrationFile {
        name: "20260904000004_index_job_lease.down.sql",
        contents: include_str!("../migrations/postgres/20260904000004_index_job_lease.down.sql"),
    },
    MigrationFile {
        name: "20260904000004_index_job_lease.up.sql",
        contents: include_str!("../migrations/postgres/20260904000004_index_job_lease.up.sql"),
    },
    MigrationFile {
        name: "20260904000005_create_job_queue.down.sql",
        contents: include_str!("../migrations/postgres/20260904000005_create_job_queue.down.sql"),
    },
    MigrationFile {
        name: "20260904000005_create_job_queue.up.sql",
        contents: include_str!("../migrations/postgres/20260904000005_create_job_queue.up.sql"),
    },
];

static POSTGRES_SET: EngineSet = EngineSet {
    engine: Engine::Postgres,
    files: POSTGRES,
};

/// The postgres set.
#[must_use]
pub const fn postgres() -> &'static EngineSet {
    &POSTGRES_SET
}

const MYSQL: &[MigrationFile] = &[
    MigrationFile {
        name: "20260904000001_create_job.down.sql",
        contents: include_str!("../migrations/mysql/20260904000001_create_job.down.sql"),
    },
    MigrationFile {
        name: "20260904000001_create_job.up.sql",
        contents: include_str!("../migrations/mysql/20260904000001_create_job.up.sql"),
    },
    MigrationFile {
        name: "20260904000002_unique_job_idempotency.down.sql",
        contents: include_str!(
            "../migrations/mysql/20260904000002_unique_job_idempotency.down.sql"
        ),
    },
    MigrationFile {
        name: "20260904000002_unique_job_idempotency.up.sql",
        contents: include_str!("../migrations/mysql/20260904000002_unique_job_idempotency.up.sql"),
    },
    MigrationFile {
        name: "20260904000003_index_job_claim.down.sql",
        contents: include_str!("../migrations/mysql/20260904000003_index_job_claim.down.sql"),
    },
    MigrationFile {
        name: "20260904000003_index_job_claim.up.sql",
        contents: include_str!("../migrations/mysql/20260904000003_index_job_claim.up.sql"),
    },
    MigrationFile {
        name: "20260904000004_index_job_lease.down.sql",
        contents: include_str!("../migrations/mysql/20260904000004_index_job_lease.down.sql"),
    },
    MigrationFile {
        name: "20260904000004_index_job_lease.up.sql",
        contents: include_str!("../migrations/mysql/20260904000004_index_job_lease.up.sql"),
    },
    MigrationFile {
        name: "20260904000005_create_job_queue.down.sql",
        contents: include_str!("../migrations/mysql/20260904000005_create_job_queue.down.sql"),
    },
    MigrationFile {
        name: "20260904000005_create_job_queue.up.sql",
        contents: include_str!("../migrations/mysql/20260904000005_create_job_queue.up.sql"),
    },
];

static MYSQL_SET: EngineSet = EngineSet {
    engine: Engine::MySql,
    files: MYSQL,
};

/// The mysql set.
#[must_use]
pub const fn mysql() -> &'static EngineSet {
    &MYSQL_SET
}
