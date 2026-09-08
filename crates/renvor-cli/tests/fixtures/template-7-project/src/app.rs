//! The application's providers, and the kernel that runs them.
//!
//! # `Services` is what every handler reaches for
//!
//! Handlers receive a `Request` and read the application's state from it. That state is one
//! [`Services`] value: the providers, each behind an `Arc`, each answering `None` before Boot and
//! `Some` after it. A handler that runs is a handler the HTTP provider started, and the HTTP
//! provider boots last — so by the time a request arrives every `Option` below is `Some`, and the
//! `InternalError` branches exist for the compiler, not for a path a request can take.
//!
//! # `Shared<P>` — one provider, two owners
//!
//! The kernel owns each provider (it boots and stops it); handlers hold an `Arc` to the same
//! provider and read what it published. `Shared` is the delegating wrapper that lets one value be
//! both: it implements `Provider` by forwarding to the `Arc`. Twelve lines, and every one of them
//! is here rather than in a macro.

use std::sync::Arc;
use std::sync::OnceLock;

use renvor::{ApplicationBuilder, RouteRegistry};
use renvor::{CapabilityId, InitContext, Provider, ProviderFuture, ProviderId};
use renvor::{HostPolicy, HttpServerConfig, HttpServerProvider, TypedStateMap};
use renvor_config::ConfigHandle;

use crate::config::{self, HttpSection};
use renvor_database::{
    ConnectionString, DatabaseKind, MigrationPolicy, MigrationSettings, PoolSettings,
};
use renvor_sqlx::Migrations;
use renvor_sqlx::provider::SqlxProvider;

/// The capability name the database provider offers, and its dependants require.
pub const DATABASE: &str = "database";
/// The HTTP provider's id.
pub const HTTP: &str = "http";

/// The framework's persistence provider for the selected row.
type FrameworkDatabase = SqlxProvider<sqlx::Postgres>;
/// The database handle the provider publishes after Boot.
pub type Database = renvor_sqlx::PostgresDatabase;

/// The database provider, constructed at Boot from `RENVOR_DATABASE_URL`.
///
/// The connection string carries its credential (framework limitation 010/L-15, retained in
/// Phase 011). It is read by the provider that needs it, at the moment it needs it — not by
/// [`Services::new`] — so the inspection requests `renvor routes` and `renvor openapi` send are
/// answered without it, and so the value lives in exactly one place: never in a log, a manifest,
/// or a rendered error.
pub struct DatabaseProvider {
    id: ProviderId,
    provides: [CapabilityId; 1],
    migrations: MigrationSettings,
    inner: OnceLock<FrameworkDatabase>,
}

impl DatabaseProvider {
    fn new() -> Self {
        Self {
            id: ProviderId::new(DATABASE),
            provides: [CapabilityId::new(DATABASE)],
            // Migrations run on Boot: both halves declared, as the framework requires.
            migrations: MigrationSettings::default().with_policy(MigrationPolicy::OnBoot),
            inner: OnceLock::new(),
        }
    }

    /// The booted database, or `None` before Boot has reached this provider.
    pub fn database(&self) -> Option<&Database> {
        self.inner.get()?.database()
    }

    /// The provider deadline Boot needs: long enough for the migration lock wait, the run, and
    /// the session close, answered from the settings because the provider does not exist yet.
    pub fn required_boot_deadline(&self) -> std::time::Duration {
        FrameworkDatabase::boot_deadline_for(&self.migrations)
    }
}

impl std::fmt::Debug for DatabaseProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseProvider")
            .field("id", &self.id)
            .field("booted", &self.database().is_some())
            .finish()
    }
}

impl Provider for DatabaseProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            let dsn = std::env::var("RENVOR_DATABASE_URL")
                .map_err(|_| "RENVOR_DATABASE_URL is not set; the database provider needs it")?;
            let migrations =
                Migrations::load(std::path::Path::new("migrations"), self.migrations.clone())
                    .await?;
            let provider = FrameworkDatabase::new(
                self.id.clone(),
                self.provides[0].clone(),
                ConnectionString::new(dsn),
                PoolSettings::default(),
                DatabaseKind::Postgres,
            )
            .with_migrations(migrations);
            self.inner
                .get_or_init(|| provider)
                .initialise(context)
                .await
        })
    }

    fn stop(&self) -> ProviderFuture<'_> {
        match self.inner.get() {
            Some(inner) => inner.stop(),
            None => Box::pin(async { Ok(()) }),
        }
    }
}

/// A provider the kernel owns and handlers share — every starter has at least the HTTP
/// provider, which `main` asks for the address it bound.
pub struct Shared<P>(pub Arc<P>);

impl<P: Provider> Provider for Shared<P> {
    fn id(&self) -> &ProviderId {
        self.0.id()
    }

    fn provides(&self) -> &[CapabilityId] {
        self.0.provides()
    }

    fn dependencies(&self) -> &[CapabilityId] {
        self.0.dependencies()
    }

    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        self.0.initialise(context)
    }

    fn stop(&self) -> ProviderFuture<'_> {
        self.0.stop()
    }
}

/// Everything a handler can reach, and everything the kernel boots.
#[derive(Clone)]
pub struct Services {
    /// The `[http]` section, resolved at Validate.
    pub http: ConfigHandle<HttpSection>,
    /// The HTTP provider, set by [`build`] so [`Services::bound_address`] can ask it what it
    /// bound: port `0` asks the kernel for a port, and only the provider knows which it got.
    pub http_server: Arc<std::sync::OnceLock<Arc<HttpServerProvider>>>,
    /// The database provider; `database()` answers after Boot.
    pub database: Arc<DatabaseProvider>,
    /// The configuration sources, registered with the kernel in [`build`].
    sources: Arc<Vec<Arc<dyn renvor::ConfigSource>>>,
}

impl Services {
    /// Declares every provider and configuration source. Nothing connects here; Boot does that,
    /// and Boot is also where `RENVOR_DATABASE_URL` is read — see [`DatabaseProvider`].
    pub fn new() -> Self {
        let mut sources: Vec<Arc<dyn renvor::ConfigSource>> = Vec::new();

        let http_source = config::http_source();
        let http = http_source.handle();
        sources.push(Arc::new(http_source));
        let database = Arc::new(DatabaseProvider::new());

        Self {
            http,
            http_server: Arc::new(std::sync::OnceLock::new()),
            database,
            sources: Arc::new(sources),
        }
    }

    /// The address the HTTP provider actually bound, or `None` before Boot.
    ///
    /// Not the configured string: with `RENVOR_HTTP_ADDRESS=127.0.0.1:0` the configured string
    /// says `:0` and the provider bound an assigned port, and a caller told the configured
    /// string would be talking to a different server.
    pub fn bound_address(&self) -> Option<std::net::SocketAddr> {
        self.http_server
            .get()
            .and_then(|provider| provider.bound_address())
    }

    /// The booted database, or `None` before Boot.
    pub fn database(&self) -> Option<&Database> {
        self.database.database()
    }
}

/// Assembles the kernel: sources, providers in dependency order, the HTTP server last.
///
/// # Errors
///
/// An unresolvable dependency (Register), or state that cannot be published — both before
/// anything has connected, listened, or spawned. Configuration is not read here: the kernel
/// resolves every section at Load and checks it at Validate, inside `boot()`, and the HTTP
/// provider builds its server configuration from the resolved `[http]` section at its own Boot.
pub fn build(
    services: &Services,
    registry: RouteRegistry,
) -> Result<renvor::Application, Box<dyn std::error::Error>> {
    let mut state = TypedStateMap::new();
    state.insert(services.clone())?;
    let state = Arc::new(state);

    // Built at Boot, from the section the kernel resolved and validated by then: the bind
    // address, and the host policy that admits loopback and this project's local domain.
    let http_section = services.http.clone();
    let mut http = HttpServerProvider::configured_at_boot(HTTP, registry, move || {
        let (address, local_domain) = http_section.with(|resolved| {
            let section = resolved.value();
            (section.address.clone(), section.local_domain.clone())
        })?;
        let mut server = HttpServerConfig::new(address.parse()?);
        server.hosts = HostPolicy::deny_all()
            .allow("127.0.0.1")?
            .allow("localhost")?
            .allow(&local_domain)?;
        server.state = state;
        Ok(server)
    });
    http = http.requires(DATABASE);

    let mut builder = ApplicationBuilder::new();
    for source in services.sources.iter() {
        builder = builder.with_config_source(Arc::clone(source));
    }
    builder = builder
        .with_provider(Box::new(Shared(Arc::clone(&services.database))))
        .with_provider_deadline(services.database.required_boot_deadline());
    let http = Arc::new(http);
    let _ = services.http_server.set(Arc::clone(&http));
    builder = builder.with_provider(Box::new(Shared(http)));
    let application = builder.build()?;
    Ok(application)
}
