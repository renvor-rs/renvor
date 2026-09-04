//! Container development controls: the choices, their grammar, and their bounds.
//!
//! # What this generates, and what it does not claim
//!
//! A **local development** Compose profile. Not a deployment, not a production topology, and not a
//! statement that the services it names are wired into application code. The database service is
//! Phase 006 functionality — a generated project with persistence has migrations, a repository, and
//! now something to run them against. The optional cache service is **local infrastructure only**:
//! Renvor's cache capability exists (`renvor-cache`, behind the facade's `capability-cache`
//! feature), the generated project does not wire it, and nothing here pretends otherwise.
//!
//! # No password can reach any of this
//!
//! Data-model invariant I-3 says a configuration value cannot hold a secret. Every type below obeys
//! it, which is what makes the guarantee structural rather than a filter somebody has to remember
//! to apply: a database *user* is a name, a port is a number, an image is a pinned tag. The
//! password lives in `.env`, which generation writes an **example** of and never fills in.
//!
//! Writing a working `.env` would be worse than asking the operator to. Generation would have to
//! invent a credential, persist it to disk, and then either print it — putting it in scrollback and
//! CI logs — or leave it silently on the filesystem where nobody knows it exists.

use serde::Serialize;

use renvor_database::DatabaseKind;

use crate::exit::{CliError, Code};

/// The longest a database name may be.
///
/// PostgreSQL truncates identifiers at `NAMEDATALEN - 1`, which is 63 in every default build. MySQL
/// allows 64. The smaller of the two is the portable answer, and a name is **refused** rather than
/// truncated: a silently shortened database name produces a project whose manifest and whose
/// running container disagree about which database it is.
const MAX_DATABASE_NAME: usize = 63;

/// The longest a database user may be.
///
/// MySQL's `user` column is 32 characters and has been since 5.7; PostgreSQL allows 63. Same rule,
/// same reason — the portable bound, enforced rather than trimmed.
const MAX_DATABASE_USER: usize = 32;

/// The documented default database user.
///
/// Not `root`, and not `postgres`. A generated project that connects as the server's superuser
/// teaches the habit, and the habit is the problem: the credential a developer copies into their
/// first deployment is the one their generator handed them.
pub const DEFAULT_DATABASE_USER: &str = "renvor";

/// A database or user name that both engines accept unquoted.
///
/// # Why a newtype rather than a validated `String`
///
/// Because the value is interpolated into a generated `compose.yaml`, and a type that can only be
/// built by [`Identifier::parse`] cannot carry something that needs escaping there. The grammar is
/// deliberately narrower than either engine's: leading letter or underscore, then ASCII letters,
/// digits, and underscores. No quoting rule, no case-folding surprise, no character whose meaning
/// depends on which engine reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Identifier(String);

impl Identifier {
    /// The value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parses an identifier, naming the rule that refused it.
    ///
    /// `flag` and `limit` are passed in so one grammar serves both the database name and the user,
    /// which have different length bounds and different flags but the same character rules.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] with `details.rule` naming which rule fired.
    pub fn parse(value: &str, flag: &'static str, limit: usize) -> Result<Self, CliError> {
        let refuse = |rule: &'static str, why: String| {
            CliError::new(Code::UnsupportedValue, why)
                .with("flag", flag)
                .with("value", value.to_owned())
                .with("rule", rule)
        };

        if value.is_empty() {
            return Err(refuse("not_empty", format!("`{flag}` cannot be empty")));
        }
        if value.len() > limit {
            return Err(refuse(
                "length",
                format!(
                    "`{value}` is {} characters; `{flag}` is limited to {limit}, which is the \
                     smaller of PostgreSQL's and MySQL's limits. It is refused rather than \
                     shortened, because a truncated name would leave the manifest and the running \
                     container disagreeing about which database this is",
                    value.len()
                ),
            ));
        }
        if !value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(refuse(
                "character_set",
                format!(
                    "`{value}` may contain only ASCII letters, digits, and `_`. A `-` is legal in \
                     a project name but not in an unquoted identifier on either engine, so it is \
                     replaced with `_` when a default is derived — and refused when you type one"
                ),
            ));
        }
        if !value
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            return Err(refuse(
                "starts_with_letter_or_underscore",
                format!("`{value}` must start with an ASCII letter or `_`"),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    /// Derives a default database name from a project name.
    ///
    /// The one documented transformation: `-` becomes `_`, because a project name may contain a
    /// hyphen and an unquoted identifier may not. Nothing else changes, and nothing is shortened —
    /// a project name too long to be a database name is **refused**, with `--database-name` named
    /// as the way to say what you meant.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] when the derived name breaks the grammar.
    pub fn derive_database_name(project: &str) -> Result<Self, CliError> {
        Self::parse(
            &project.replace('-', "_"),
            "--database-name",
            MAX_DATABASE_NAME,
        )
    }

    /// Parses a supplied database name.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`].
    pub fn database_name(value: &str) -> Result<Self, CliError> {
        Self::parse(value, "--database-name", MAX_DATABASE_NAME)
    }

    /// Parses a supplied database user.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`].
    pub fn database_user(value: &str) -> Result<Self, CliError> {
        Self::parse(value, "--database-user", MAX_DATABASE_USER)
    }
}

/// A database image version, drawn from the versions Phase 006 actually tested.
///
/// # The choice set is the support matrix, not the registry
///
/// Constitution principle VII: a governed choice set offers what is supported, and refuses the rest
/// **by name**. Every variant here is a version the cancellation suite, the migration suite, and
/// the repository suite ran green against on a real server. Offering `postgres:16` because the
/// registry has it would be offering a version nothing verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseVersion {
    /// PostgreSQL 17, tested at 17.11.
    Postgres17,
    /// PostgreSQL 18, tested at 18.6.
    Postgres18,
    /// MySQL 8.4 LTS, tested at 8.4.11.
    MySql84,
    /// MySQL 9.7, tested at 9.7.2.
    MySql97,
}

impl DatabaseVersion {
    /// Every tested version.
    pub const ALL: [Self; 4] = [
        Self::Postgres17,
        Self::Postgres18,
        Self::MySql84,
        Self::MySql97,
    ];

    /// The spelling an operator types and the manifest records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Postgres17 => "17",
            Self::Postgres18 => "18",
            Self::MySql84 => "8.4",
            Self::MySql97 => "9.7",
        }
    }

    /// Which engine this version belongs to.
    #[must_use]
    pub const fn kind(self) -> DatabaseKind {
        match self {
            Self::Postgres17 | Self::Postgres18 => DatabaseKind::Postgres,
            Self::MySql84 | Self::MySql97 => DatabaseKind::MySql,
        }
    }

    /// The **pinned** image reference.
    ///
    /// # Pinned to a patch, and why not to a digest
    ///
    /// A floating `postgres:17` would silently become a different server between two `renvor
    /// docker up` runs, which is exactly the class of difference that makes a local reproduction
    /// stop reproducing. These pin the patch versions this phase's suites ran against.
    ///
    /// A **digest** would pin harder still, and this deliberately does not use one. A digest is
    /// architecture-specific in the single-platform form most people copy, it cannot be read by a
    /// human deciding whether an upgrade is due, and it goes stale the first time upstream
    /// republishes a security rebuild under the same tag — leaving a generated project pinned to a
    /// known-vulnerable image with no signal that it is. The honest position is a pinned tag plus
    /// this paragraph, not a digest plus a claim of immutability that the generated project has no
    /// mechanism to maintain.
    #[must_use]
    pub const fn image(self) -> &'static str {
        match self {
            Self::Postgres17 => "docker.io/library/postgres:17.11-trixie",
            Self::Postgres18 => "docker.io/library/postgres:18.6-trixie",
            Self::MySql84 => "docker.io/library/mysql:8.4.11",
            Self::MySql97 => "docker.io/library/mysql:9.7.2",
        }
    }

    /// The port the server listens on **inside** the container.
    ///
    /// Distinct from the host port an operator chooses: containers on the project network reach
    /// each other on this one, and changing the published port changes neither.
    #[must_use]
    pub const fn container_port(self) -> u16 {
        match self.kind() {
            DatabaseKind::Postgres => 5432,
            _ => 3306,
        }
    }

    /// The default **host** port.
    #[must_use]
    pub const fn default_host_port(self) -> u16 {
        self.container_port()
    }

    /// Where the server keeps its data **inside** the container.
    ///
    /// # PostgreSQL 17 and 18 differ, and getting it wrong loses the data
    ///
    /// The 17 image keeps `PGDATA` at `/var/lib/postgresql/data` and declares that as its volume.
    /// The 18 image moved to a versioned layout — `PGDATA=/var/lib/postgresql/18/docker`, with the
    /// volume one level up at `/var/lib/postgresql`. Mounting 17's path on an 18 server puts the
    /// named volume somewhere the server does not write, so every `renvor docker down` would
    /// silently discard the database.
    ///
    /// Read off the running images rather than remembered:
    ///
    /// ```text
    /// rv-pg17  volume /var/lib/postgresql/data   PGDATA=/var/lib/postgresql/data
    /// rv-pg18  volume /var/lib/postgresql        PGDATA=/var/lib/postgresql/18/docker
    /// ```
    #[must_use]
    pub const fn data_dir(self) -> &'static str {
        match self {
            Self::Postgres17 => "/var/lib/postgresql/data",
            Self::Postgres18 => "/var/lib/postgresql",
            Self::MySql84 | Self::MySql97 => "/var/lib/mysql",
        }
    }

    /// The health check, as the Compose `test` list.
    ///
    /// # Every one of these was checked in BOTH directions
    ///
    /// A health check that cannot fail is worse than none: it reports healthy for a dead server and
    /// `depends_on: service_healthy` then starts the application against nothing. Both commands
    /// were run against a live server and against a dead endpoint before being written here.
    ///
    /// ```text
    /// pg_isready -h 127.0.0.1 -U … -d …   live: 0   dead port: 2
    /// mysqladmin -h 127.0.0.1 ping        live: 0   dead port: 1   unreachable host: 1
    /// ```
    ///
    /// # `-h 127.0.0.1` is the whole point, and it was missing
    ///
    /// Without a host, both tools answer over the **unix socket**. Both official entrypoints run
    /// their initialisation server with networking disabled — `postgres` with
    /// `-c listen_addresses=''`, `mysql` with `--skip-networking` — so a socket probe reports
    /// healthy while the port is closed:
    ///
    /// ```text
    /// pg_isready -U renvor -d demo                 -> accepting connections   exit 0
    /// pg_isready -h 127.0.0.1 -U renvor -d demo    -> no response             exit 2
    /// mysqladmin ping --silent                     -> exit 0
    /// mysqladmin -h 127.0.0.1 ping --silent        -> exit 1
    /// ```
    ///
    /// The application reaches the database over **TCP** on the project network, and
    /// `depends_on: service_healthy` is what holds it back. A socket-only probe releases it into
    /// the initialisation window, where `SqlxDatabase::connect` opens the pool during Boot by
    /// design — so it is a hard boot failure, not a retry. The two directions originally checked
    /// were *live* and *dead port*; this third state is the one that actually occurs.
    ///
    /// # Neither carries a credential
    ///
    /// `pg_isready` needs no password — it asks whether the server is accepting connections, not
    /// whether it will authenticate you. `mysqladmin ping` answers from the server's response, so it
    /// is given no credentials at all. Both are visible in `docker inspect` and in the container's
    /// process list, which is exactly why neither may contain one.
    #[must_use]
    pub fn healthcheck(self, user: &str, database: &str) -> Vec<String> {
        match self.kind() {
            DatabaseKind::Postgres => vec![
                "CMD".to_owned(),
                "pg_isready".to_owned(),
                "-h".to_owned(),
                "127.0.0.1".to_owned(),
                "-U".to_owned(),
                user.to_owned(),
                "-d".to_owned(),
                database.to_owned(),
            ],
            _ => vec![
                "CMD".to_owned(),
                "mysqladmin".to_owned(),
                "-h".to_owned(),
                "127.0.0.1".to_owned(),
                "ping".to_owned(),
                "--silent".to_owned(),
            ],
        }
    }

    /// The newest tested version for an engine, used as the prompt's default.
    #[must_use]
    pub const fn newest_for(kind: DatabaseKind) -> Self {
        match kind {
            DatabaseKind::Postgres => Self::Postgres18,
            _ => Self::MySql97,
        }
    }

    /// The `--help`-style hint naming this engine's tested versions.
    ///
    /// # A literal, because the prompt library will not take anything else
    ///
    /// `prompt::text` takes `Option<&'static str>` on purpose: a hint that could carry operator
    /// input is a hint that could carry a secret onto the screen and into a terminal recording. So
    /// the list cannot be `format!`ed from [`DatabaseVersion::supported_for`] at the call site.
    ///
    /// It lives here rather than in the wizard so that the literal sits beside the variants it
    /// describes, and `the_version_hint_names_exactly_the_supported_versions` fails if a variant is
    /// added without updating it.
    #[must_use]
    pub const fn version_hint(kind: DatabaseKind) -> &'static str {
        match kind {
            DatabaseKind::Postgres => {
                "`17` or `18`; these are the versions this release was tested against"
            }
            _ => "`8.4` or `9.7`; these are the versions this release was tested against",
        }
    }

    /// Every tested version for one engine, oldest first.
    #[must_use]
    pub fn supported_for(kind: DatabaseKind) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|version| version.kind() == kind)
            .collect()
    }

    /// Parses a `--database-version` value against the selected engine.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] naming the versions this engine actually supports. A version that
    /// belongs to the *other* engine is refused the same way rather than quietly accepted, because
    /// `--database postgres --database-version 8.4` is a mistake worth reporting.
    pub fn parse(kind: DatabaseKind, value: &str) -> Result<Self, CliError> {
        let supported = Self::supported_for(kind);
        supported
            .iter()
            .copied()
            .find(|version| version.as_str() == value)
            .ok_or_else(|| {
                let names = supported
                    .iter()
                    .map(|version| version.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                CliError::new(
                    Code::UnsupportedValue,
                    format!(
                        "`{value}` is not a tested {} version; the supported values are {names}. \
                         The choice set is this phase's support matrix rather than the registry's \
                         tag list, so a version that nothing verified is not offered",
                        kind.as_str()
                    ),
                )
                .with("flag", "--database-version")
                .with("value", value.to_owned())
                .with("supported", names)
            })
    }
}

/// The cache engine a generated project can run locally.
///
/// # One variant, chosen on evidence rather than on habit
///
/// Redis was the reflexive answer and is the wrong one. Redis Ltd. relicensed in March 2024 to
/// RSALv2/SSPLv1, neither OSI-approved; Redis 8 added AGPLv3, an OSI-approved but strongly
/// copyleft third option. Valkey is the Linux Foundation fork of Redis 7.2.4 under
/// **BSD-3-Clause** — the permissive terms Redis left behind — maintained by a foundation rather
/// than a single vendor, and now the default `redis`-compatible package in Debian, Ubuntu, Fedora,
/// and Arch.
///
/// This repository already refuses a dependency for its licence: `sqlx`'s obvious TLS feature was
/// rejected because `webpki-roots` is CDLA-Permissive-2.0 and not on `deny.toml`'s allow-list.
/// Handing a generated project an image under SSPL while refusing a crate under CDLA would be two
/// standards.
///
/// # There is no multi-choice prompt, and that is deliberate
///
/// A prompt offering one real option and one worse option is a prompt pretending to be a decision.
/// The wizard asks *whether* a cache container is wanted; which engine is recorded, not asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheEngine {
    /// Valkey — BSD-3-Clause, Linux Foundation.
    Valkey,
}

impl CacheEngine {
    /// Every supported engine.
    ///
    /// One today. It is an array rather than a bare constant so that the licence and pinning
    /// assertions below iterate over whatever the set becomes, instead of over the one name
    /// somebody remembered to write in the test.
    pub const ALL: [Self; 1] = [Self::Valkey];

    /// The spelling `--container-cache` accepts and the manifest records.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Valkey => "valkey",
        }
    }

    /// The pinned image. See [`DatabaseVersion::image`] for why a tag and not a digest.
    #[must_use]
    pub const fn image(self) -> &'static str {
        match self {
            Self::Valkey => "docker.io/valkey/valkey:9.1.1-alpine",
        }
    }

    /// The version recorded in the manifest.
    #[must_use]
    pub const fn version(self) -> &'static str {
        match self {
            Self::Valkey => "9.1.1",
        }
    }

    /// The licence, recorded so a reader does not have to go and look.
    #[must_use]
    pub const fn licence(self) -> &'static str {
        match self {
            Self::Valkey => "BSD-3-Clause",
        }
    }

    /// The port inside the container.
    #[must_use]
    pub const fn container_port(self) -> u16 {
        match self {
            Self::Valkey => 6379,
        }
    }

    /// The default host port.
    #[must_use]
    pub const fn default_host_port(self) -> u16 {
        self.container_port()
    }

    /// Where the cache keeps its data inside the container.
    #[must_use]
    pub const fn data_dir(self) -> &'static str {
        match self {
            Self::Valkey => "/data",
        }
    }

    /// The health check, as the Compose `test` list.
    ///
    /// # `CMD-SHELL` and a `grep`, because the obvious version cannot fail
    ///
    /// `valkey-cli ping` exits **0 whether or not the server answered**. Measured against a
    /// password-protected server:
    ///
    /// ```text
    /// valkey-cli ping                    no auth: 0   wrong password: 0   healthy: 0
    /// valkey-cli ping | grep -q PONG     no auth: 1   wrong password: 1   healthy: 0
    /// ```
    ///
    /// The first is the form most examples use and it is a health check that can only ever report
    /// healthy. The `grep` makes the exit status depend on the server's actual reply.
    ///
    /// The password is not in **this** command: `valkey-cli` reads `REDISCLI_AUTH` from the
    /// environment.
    ///
    /// # What that does and does not buy, stated honestly
    ///
    /// It keeps the credential out of the *health check*. It does **not** keep it out of
    /// `docker inspect`, because the cache service's own `command:` passes `--requirepass` as an
    /// argv element and `REDISCLI_AUTH` sits in `environment:`. An earlier version of this comment
    /// claimed the credential "stays out of the command text that `docker inspect` and the process
    /// list expose", which was true of the health check and false of the two lines above it.
    ///
    /// Measured:
    ///
    /// ```text
    /// docker inspect …-cache-1 --format '{{json .Config.Cmd}}'
    ///   ["valkey-server","--requirepass","<the password>"]
    /// docker inspect …-cache-1 --format '{{json .Config.Env}}'
    ///   ["REDISCLI_AUTH=<the password>", …]
    /// ```
    ///
    /// So: a developer who pastes `docker inspect` or `docker compose config` output into a public
    /// issue discloses their **local development** cache password. That is the actual exposure —
    /// bounded by the service being loopback-only and by nothing in the project reading it — and it
    /// is recorded rather than described away. Removing it needs a Compose `secrets:` mount and a
    /// config file, which is a change to the generated profile's shape rather than a comment.
    #[must_use]
    pub fn healthcheck(self) -> Vec<String> {
        match self {
            Self::Valkey => vec![
                "CMD-SHELL".to_owned(),
                "valkey-cli ping | grep -q PONG".to_owned(),
            ],
        }
    }
}

/// Whether a local cache container is generated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheChoice {
    /// No cache service. The default.
    None,
    /// One cache service, running the named engine.
    Engine(CacheEngine),
}

impl CacheChoice {
    /// The engine, if one was chosen.
    #[must_use]
    pub const fn engine(self) -> Option<CacheEngine> {
        match self {
            Self::None => None,
            Self::Engine(engine) => Some(engine),
        }
    }

    /// The recorded spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Engine(engine) => engine.as_str(),
        }
    }

    /// Parses a `--container-cache` value.
    ///
    /// # Errors
    ///
    /// [`Code::UnsupportedValue`] naming both accepted values.
    pub fn parse(value: &str) -> Result<Self, CliError> {
        if value == "none" {
            return Ok(Self::None);
        }
        // Matched against `CacheEngine::ALL` rather than against a literal, so adding an engine
        // makes it acceptable AND makes the refusal below name it. Two lists that must agree are
        // two lists that eventually do not.
        if let Some(engine) = CacheEngine::ALL
            .into_iter()
            .find(|engine| engine.as_str() == value)
        {
            return Ok(Self::Engine(engine));
        }
        let supported = std::iter::once("none")
            .chain(CacheEngine::ALL.iter().map(|engine| engine.as_str()))
            .collect::<Vec<_>>()
            .join(", ");
        Err(CliError::new(
            Code::UnsupportedValue,
            format!(
                "`{value}` is not a supported cache engine; the supported values are {supported}"
            ),
        )
        .with("flag", "--container-cache")
        .with("value", value.to_owned())
        .with("supported", supported))
    }
}

/// Parses a host port.
///
/// # Why `1` and not `0`
///
/// Port `0` means "let the kernel choose" — which for a *published* port would hand the operator a
/// different port on every start, with nothing recording which one. That is the silent-fallback
/// pattern principle IV prohibits, so it is refused by name rather than accepted and surprising.
///
/// # Errors
///
/// [`Code::UnsupportedValue`] for a non-integer, for `0`, and for anything above 65535.
pub fn parse_port(value: &str, flag: &'static str) -> Result<u16, CliError> {
    let refuse = |why: String| {
        CliError::new(Code::UnsupportedValue, why)
            .with("flag", flag)
            .with("value", value.to_owned())
            .with("supported", "1-65535")
    };
    let number: u32 = value.parse().map_err(|_| {
        refuse(format!(
            "`{value}` is not a whole number; `{flag}` takes 1-65535"
        ))
    })?;
    if number == 0 {
        return Err(refuse(format!(
            "`{flag}` cannot be 0. Port 0 asks the kernel for any free port, which would publish \
             the database on a different port every start with nothing recording which"
        )));
    }
    u16::try_from(number)
        .map_err(|_| refuse(format!("`{value}` is above 65535; `{flag}` takes 1-65535")))
}

/// The container development controls a generated project records.
///
/// # `None` for every database field when persistence was not selected
///
/// Containers without persistence is a supported combination — the application image and its
/// network are useful on their own — so these are options rather than a second struct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContainerSettings {
    /// The database image version, when persistence was selected.
    pub database_version: Option<DatabaseVersion>,
    /// The database name, when persistence was selected.
    pub database_name: Option<Identifier>,
    /// The database user. **Never a password.**
    pub database_user: Option<Identifier>,
    /// The published host port for the database.
    pub database_port: Option<u16>,
    /// Whether a cache container is generated, and which engine.
    pub cache: CacheChoice,
    /// The published host port for the cache, when one was chosen.
    pub cache_port: Option<u16>,
}

// ── SERIALISATION ───────────────────────────────────────────────────────────────────────
//
// Hand-written, and the derive is deliberately absent. `#[derive(Serialize)]` emits the VARIANT
// name: `DatabaseVersion::MySql97` became `"my-sql97"`, and `CacheChoice::Engine(Valkey)` became
// `{"engine":"valkey"}`. Neither is a value `--database-version` or `--container-cache` accepts, so
// a consumer reading the JSON to script the next run would be handed a string the CLI refuses.
//
// Delegating to `as_str` is what keeps ONE spelling across the flag, the prompt, `renvor.toml`, and
// the JSON. Found by reading the emitted JSON rather than by reading the derive.

impl Serialize for DatabaseVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl Serialize for CacheEngine {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl Serialize for CacheChoice {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tested_version_belongs_to_exactly_one_engine_and_names_a_pinned_image() {
        for version in DatabaseVersion::ALL {
            let image = version.image();
            // A floating tag is the failure this test exists to catch: `postgres:17` would resolve
            // to a different server between two runs of the same generated project.
            assert!(
                !image.ends_with(":latest") && image.contains(':'),
                "{image} must carry a pinned tag"
            );
            let digits = image.rsplit(':').next().expect("a tag");
            assert!(
                digits.matches('.').count() >= 1,
                "{image} pins a major version rather than a patch"
            );
            assert_eq!(
                DatabaseVersion::supported_for(version.kind())
                    .iter()
                    .filter(|other| **other == version)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn a_version_from_the_other_engine_is_refused_by_name() {
        // `--database postgres --database-version 8.4` is a real mistake, and accepting it would
        // generate a Compose file whose database service could not serve the generated migrations.
        let error = DatabaseVersion::parse(DatabaseKind::Postgres, "8.4").expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedValue);
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "supported" && value.contains("17")),
            "the refusal must name the versions that ARE supported"
        );
    }

    #[test]
    fn the_version_hint_names_exactly_the_supported_versions() {
        // The hint has to be a `&'static str` — the prompt library refuses anything else — so it
        // cannot be built from `supported_for`. This is what keeps the literal honest: add a
        // variant without updating the hint and the wizard starts offering a version it does not
        // name, or naming one it does not offer.
        for kind in DatabaseKind::ALL {
            let hint = DatabaseVersion::version_hint(kind);
            let supported = DatabaseVersion::supported_for(kind);
            for version in &supported {
                assert!(
                    hint.contains(version.as_str()),
                    "the {} hint does not name the supported version `{}`",
                    kind.as_str(),
                    version.as_str()
                );
            }
            // And the other direction: a hint naming a version that is not offered is worse than
            // one that is merely incomplete, because the operator types it and is refused.
            for other in DatabaseVersion::ALL {
                if !supported.contains(&other) {
                    assert!(
                        !hint.contains(other.as_str()),
                        "the {} hint names `{}`, which belongs to the other engine",
                        kind.as_str(),
                        other.as_str()
                    );
                }
            }
        }
    }

    #[test]
    fn the_newest_version_of_each_engine_is_a_version_of_that_engine() {
        for kind in DatabaseKind::ALL {
            let newest = DatabaseVersion::newest_for(kind);
            assert_eq!(newest.kind(), kind);
            assert!(DatabaseVersion::supported_for(kind).contains(&newest));
        }
    }

    #[test]
    fn a_hyphenated_project_name_derives_an_underscored_database_name() {
        let derived = Identifier::derive_database_name("my-shop-api").expect("derives");
        assert_eq!(derived.as_str(), "my_shop_api");
    }

    #[test]
    fn a_too_long_name_is_refused_rather_than_truncated() {
        // THE POINT: 64 `a`s is a legal project name and an illegal PostgreSQL identifier. A
        // generator that trimmed it would produce a manifest naming one database and a container
        // serving another.
        let long = "a".repeat(64);
        let error = Identifier::derive_database_name(&long).expect_err("refused");
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "rule" && value == "length")
        );
        assert!(Identifier::derive_database_name(&"a".repeat(63)).is_ok());
    }

    #[test]
    fn a_user_is_bounded_by_mysqls_limit_rather_than_postgresqls() {
        // 33 characters is a legal PostgreSQL role and an illegal MySQL user. The portable bound is
        // the smaller one, or a generated project works on one engine and not the other.
        assert!(Identifier::database_user(&"u".repeat(32)).is_ok());
        assert!(Identifier::database_user(&"u".repeat(33)).is_err());
    }

    #[test]
    fn an_identifier_cannot_carry_something_that_would_need_quoting() {
        for hostile in [
            "my-app",           // legal project name, illegal unquoted identifier
            "1app",             // leading digit
            "app;DROP TABLE x", // statement separator
            "app`",             // MySQL quoting
            "app\"",            // PostgreSQL quoting
            "app name",         // whitespace
            "app$",             // legal in MySQL, not in PostgreSQL unquoted
            "",                 // empty
        ] {
            assert!(
                Identifier::database_name(hostile).is_err(),
                "`{hostile}` must be refused"
            );
        }
    }

    #[test]
    fn port_zero_and_port_65536_are_both_refused() {
        assert!(parse_port("0", "--database-port").is_err());
        assert!(parse_port("65536", "--database-port").is_err());
        assert!(parse_port("-1", "--database-port").is_err());
        assert!(parse_port("http", "--database-port").is_err());
        assert_eq!(parse_port("1", "--database-port").expect("valid"), 1);
        assert_eq!(
            parse_port("65535", "--database-port").expect("valid"),
            65535
        );
    }

    #[test]
    fn the_cache_engine_is_permissively_licensed() {
        // Recorded as a test rather than a comment, because the reason this engine was chosen over
        // the familiar one IS the licence. A future variant added without checking fails here.
        for engine in CacheEngine::ALL {
            assert_eq!(engine.licence(), "BSD-3-Clause");
            assert!(!engine.image().ends_with(":latest"));
        }
    }

    #[test]
    fn an_unknown_cache_engine_is_refused_naming_the_supported_ones() {
        let error = CacheChoice::parse("memcached").expect_err("refused");
        assert_eq!(error.code, Code::UnsupportedValue);
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "supported" && value.contains("valkey"))
        );
        assert_eq!(
            CacheChoice::parse("none").expect("valid"),
            CacheChoice::None
        );
    }

    #[test]
    fn every_serialised_value_is_one_the_cli_accepts_back() {
        // THE ROUND TRIP. A derive emitted `"my-sql97"` and `{"engine":"valkey"}` — plausible
        // JSON, and neither is a value the flags parse. This asserts the property that matters:
        // whatever comes out can go back in.
        for version in DatabaseVersion::ALL {
            let json = serde_json::to_string(&version).expect("serialises");
            let text = json.trim_matches('"');
            assert_eq!(text, version.as_str());
            assert_eq!(
                DatabaseVersion::parse(version.kind(), text).expect("parses back"),
                version
            );
        }
        for choice in [CacheChoice::None, CacheChoice::Engine(CacheEngine::Valkey)] {
            let json = serde_json::to_string(&choice).expect("serialises");
            let text = json.trim_matches('"');
            assert_eq!(CacheChoice::parse(text).expect("parses back"), choice);
        }
    }

    #[test]
    fn the_default_user_is_not_a_superuser_name() {
        // `root` and `postgres` are the two an operator would reach for, and both teach a habit
        // whose consequence shows up in somebody's first deployment rather than here.
        assert_eq!(DEFAULT_DATABASE_USER, "renvor");
        assert_ne!(DEFAULT_DATABASE_USER, "root");
        assert_ne!(DEFAULT_DATABASE_USER, "postgres");
    }
}
