//! The authentication migration set, embedded (Phase 011).
//!
//! # Why the SQL is in the binary as well as on disk
//!
//! Phase 011's generator copies this set into a generated project. Contract C-4 makes every
//! template a part of the executable, so the generator cannot read SQL from a checkout at
//! generation time — it depends on this crate and takes the set from here. Each constant is an
//! `include_str!` of the file beside it, so editing a migration edits the constant, and
//! `tests/embedded_migrations.rs` proves the two never differ and the set is complete.
//!
//! # The two engines do not ship the same number of files, and that is deliberate
//!
//! PostgreSQL carries nine pairs and MySQL eight: the index on `rv_auth_refresh (family_id)` is
//! its own migration on PostgreSQL (`…0009_index_auth_refresh_family`), while MySQL's `…0007`
//! declares it inline as the foreign key's index (`ix_auth_refresh_family`). The schemas agree;
//! the file counts do not, and a test asserting equal counts would be asserting a coincidence.
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
        name: "20260901000001_create_auth_user.down.sql",
        contents: include_str!("../migrations/postgres/20260901000001_create_auth_user.down.sql"),
    },
    MigrationFile {
        name: "20260901000001_create_auth_user.up.sql",
        contents: include_str!("../migrations/postgres/20260901000001_create_auth_user.up.sql"),
    },
    MigrationFile {
        name: "20260901000002_create_auth_credential.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000002_create_auth_credential.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000002_create_auth_credential.up.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000002_create_auth_credential.up.sql"
        ),
    },
    MigrationFile {
        name: "20260901000003_create_auth_session.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000003_create_auth_session.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000003_create_auth_session.up.sql",
        contents: include_str!("../migrations/postgres/20260901000003_create_auth_session.up.sql"),
    },
    MigrationFile {
        name: "20260901000004_create_auth_verification.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000004_create_auth_verification.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000004_create_auth_verification.up.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000004_create_auth_verification.up.sql"
        ),
    },
    MigrationFile {
        name: "20260901000005_create_auth_password_reset.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000005_create_auth_password_reset.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000005_create_auth_password_reset.up.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000005_create_auth_password_reset.up.sql"
        ),
    },
    MigrationFile {
        name: "20260901000006_create_auth_refresh_family.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000006_create_auth_refresh_family.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000006_create_auth_refresh_family.up.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000006_create_auth_refresh_family.up.sql"
        ),
    },
    MigrationFile {
        name: "20260901000007_create_auth_refresh.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000007_create_auth_refresh.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000007_create_auth_refresh.up.sql",
        contents: include_str!("../migrations/postgres/20260901000007_create_auth_refresh.up.sql"),
    },
    MigrationFile {
        name: "20260901000008_create_auth_attempt.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000008_create_auth_attempt.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000008_create_auth_attempt.up.sql",
        contents: include_str!("../migrations/postgres/20260901000008_create_auth_attempt.up.sql"),
    },
    MigrationFile {
        name: "20260901000009_index_auth_refresh_family.down.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000009_index_auth_refresh_family.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000009_index_auth_refresh_family.up.sql",
        contents: include_str!(
            "../migrations/postgres/20260901000009_index_auth_refresh_family.up.sql"
        ),
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
        name: "20260901000001_create_auth_user.down.sql",
        contents: include_str!("../migrations/mysql/20260901000001_create_auth_user.down.sql"),
    },
    MigrationFile {
        name: "20260901000001_create_auth_user.up.sql",
        contents: include_str!("../migrations/mysql/20260901000001_create_auth_user.up.sql"),
    },
    MigrationFile {
        name: "20260901000002_create_auth_credential.down.sql",
        contents: include_str!(
            "../migrations/mysql/20260901000002_create_auth_credential.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000002_create_auth_credential.up.sql",
        contents: include_str!("../migrations/mysql/20260901000002_create_auth_credential.up.sql"),
    },
    MigrationFile {
        name: "20260901000003_create_auth_session.down.sql",
        contents: include_str!("../migrations/mysql/20260901000003_create_auth_session.down.sql"),
    },
    MigrationFile {
        name: "20260901000003_create_auth_session.up.sql",
        contents: include_str!("../migrations/mysql/20260901000003_create_auth_session.up.sql"),
    },
    MigrationFile {
        name: "20260901000004_create_auth_verification.down.sql",
        contents: include_str!(
            "../migrations/mysql/20260901000004_create_auth_verification.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000004_create_auth_verification.up.sql",
        contents: include_str!(
            "../migrations/mysql/20260901000004_create_auth_verification.up.sql"
        ),
    },
    MigrationFile {
        name: "20260901000005_create_auth_password_reset.down.sql",
        contents: include_str!(
            "../migrations/mysql/20260901000005_create_auth_password_reset.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000005_create_auth_password_reset.up.sql",
        contents: include_str!(
            "../migrations/mysql/20260901000005_create_auth_password_reset.up.sql"
        ),
    },
    MigrationFile {
        name: "20260901000006_create_auth_refresh_family.down.sql",
        contents: include_str!(
            "../migrations/mysql/20260901000006_create_auth_refresh_family.down.sql"
        ),
    },
    MigrationFile {
        name: "20260901000006_create_auth_refresh_family.up.sql",
        contents: include_str!(
            "../migrations/mysql/20260901000006_create_auth_refresh_family.up.sql"
        ),
    },
    MigrationFile {
        name: "20260901000007_create_auth_refresh.down.sql",
        contents: include_str!("../migrations/mysql/20260901000007_create_auth_refresh.down.sql"),
    },
    MigrationFile {
        name: "20260901000007_create_auth_refresh.up.sql",
        contents: include_str!("../migrations/mysql/20260901000007_create_auth_refresh.up.sql"),
    },
    MigrationFile {
        name: "20260901000008_create_auth_attempt.down.sql",
        contents: include_str!("../migrations/mysql/20260901000008_create_auth_attempt.down.sql"),
    },
    MigrationFile {
        name: "20260901000008_create_auth_attempt.up.sql",
        contents: include_str!("../migrations/mysql/20260901000008_create_auth_attempt.up.sql"),
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
