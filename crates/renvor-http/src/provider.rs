//! The HTTP server as a **kernel provider** — contract C-10's opening clause.
//!
//! # What this closes
//!
//! Before this module, `renvor-http` could bind and serve, and did so entirely outside the kernel
//! lifecycle. Nothing placed the bind in `Boot`, nothing made a bind failure roll back, nothing
//! contributed to readiness, and nothing put the drain in a documented position relative to
//! provider shutdown. C-10 opens by saying *"the HTTP server participates in the kernel lifecycle"*
//! — it did not.
//!
//! # The ordering is inherited, not redefined
//!
//! ```text
//!  Load · Validate · Register   configuration only; NOTHING is bound
//!  Boot                         the listener is bound. A bind failure rolls back like any other
//!  Ready                        requests are served; readiness reports ready
//!  Drain                        the kernel closes the gate and drains within its budget
//!  Stop                         providers stop in reverse actual initialisation order (C-L3)
//! ```
//!
//! C-L1 already fixes `Drain` before `Stop`, so **the server drains before providers stop** without
//! this module arranging anything. That matters: an application whose database provider stopped
//! first would have in-flight requests failing against a closed pool, and the ordering that
//! prevents it is the kernel's, not the transport's.
//!
//! **The gate is the kernel's own.** [`renvor_core::lifecycle::WorkGate`] is taken from the
//! initialisation context, so the work the kernel drains and the work this server admits are the
//! same units of work. A transport with its own private gate would drain something the kernel could
//! not see, and the kernel would report clean while requests were still running.
//!
//! # A named limitation: which state map handlers can read
//!
//! Handlers read the [`renvor_core::TypedStateMap`] this provider was **given**, not the map the
//! kernel populates through `InitContext::register_state`. The kernel exposes that map only as a
//! borrow whose lifetime ends with initialisation, so it cannot be shared into a router that
//! outlives it.
//!
//! An application that wants a value visible to both therefore registers it in the shared map it
//! hands to [`HttpServerConfig::state`]. This is stated rather than smoothed over, per constitution
//! principle XII: the alternative was to quietly serve an empty map and let an author discover it
//! at runtime.

use core::time::Duration;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use renvor_core::{
    CapabilityId, DrainOutcome, InitContext, Provider, ProviderId, Readiness, ReadinessContributor,
    TypedStateMap,
};

use crate::cors::CorsPolicy;
use crate::host::HostPolicy;
use crate::identity::TrustedProxies;
use crate::limits::Limits;
use crate::route::RouteRegistry;
use crate::route::build::{RouterConfig, router};
use crate::server::Server;

/// Everything the server needs that is not supplied by the kernel at initialisation.
///
/// The run identifier, the cancellation scope, and the work gate are deliberately **absent**: they
/// come from [`InitContext`], which is the only source that guarantees they are the application's
/// own rather than a second set this provider invented.
pub struct HttpServerConfig {
    /// The address to bind. Port `0` is valid and yields an assigned port.
    pub address: SocketAddr,
    /// The hosts this application answers for. Denies all by default.
    pub hosts: HostPolicy,
    /// The peers whose forwarding headers are honoured. Empty by default.
    pub trusted_proxies: TrustedProxies,
    /// Which origins may read responses. Denies all by default.
    pub cors: CorsPolicy,
    /// The documented resource bounds, including the drain budget.
    pub limits: Limits,
    /// The state handlers read. See this module's *named limitation*.
    pub state: Arc<TypedStateMap>,
}

impl HttpServerConfig {
    /// A configuration for `address` with every documented default, denying everything not named.
    #[must_use]
    pub fn new(address: SocketAddr) -> Self {
        Self {
            address,
            hosts: HostPolicy::deny_all(),
            trusted_proxies: TrustedProxies::none(),
            cors: CorsPolicy::deny_all(),
            limits: Limits::new(),
            state: Arc::new(TypedStateMap::new()),
        }
    }
}

/// What a started server left behind for `stop` to shut down.
struct Running {
    shutdown: tokio::sync::oneshot::Sender<()>,
    task: tokio::task::JoinHandle<io::Result<DrainOutcome>>,
}

/// Whether the server is bound and serving.
///
/// Its own type rather than a closure, because [`ReadinessContributor`] is asked from outside the
/// application — a readiness probe — and a contributor that could block would block the probe.
/// Reading one atomic cannot.
#[derive(Debug)]
struct ServerReadiness {
    name: String,
    serving: Arc<AtomicBool>,
}

impl ReadinessContributor for ServerReadiness {
    fn name(&self) -> &str {
        &self.name
    }

    fn readiness(&self) -> Readiness {
        // `Acquire` pairs with the `Release` store in `initialise`, so a probe that observes
        // `true` also observes the bind that preceded it. `Relaxed` would permit reporting ready
        // before the listener existed.
        if self.serving.load(Ordering::Acquire) {
            Readiness::Ready
        } else {
            Readiness::NotReady
        }
    }
}

/// The HTTP server, as a provider the kernel boots, drains, and stops.
///
/// # Why the interior is behind mutexes
///
/// [`Provider::initialise`] and [`Provider::stop`] both take `&self` — a provider is `Send + Sync`
/// and outlives the phase that built it. The registry is *consumed* at initialisation and the
/// running task is *produced* there, so both have to move through shared references.
///
/// The alternative, requiring `&mut self`, would mean the kernel owned each provider exclusively
/// and could not hand out the `&dyn Provider` its registry is built from.
pub struct HttpServerProvider {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    dependencies: Vec<CapabilityId>,
    config: Mutex<Option<HttpServerConfig>>,
    registry: Mutex<Option<RouteRegistry>>,
    running: Mutex<Option<Running>>,
    bound: Mutex<Option<SocketAddr>>,
    serving: Arc<AtomicBool>,
}

impl HttpServerProvider {
    /// Builds a provider that will serve `registry` under `config`.
    ///
    /// The registry is taken by value, which is what makes "one authoritative registry" hold
    /// through the lifecycle as well as at construction: there is no second copy to diverge.
    #[must_use]
    pub fn new(id: impl Into<String>, config: HttpServerConfig, registry: RouteRegistry) -> Self {
        Self {
            id: ProviderId::new(id),
            provides: Vec::new(),
            dependencies: Vec::new(),
            config: Mutex::new(Some(config)),
            registry: Mutex::new(Some(registry)),
            running: Mutex::new(None),
            bound: Mutex::new(None),
            serving: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Declares a capability this provider offers.
    #[must_use]
    pub fn provides(mut self, capability: impl Into<String>) -> Self {
        self.provides.push(CapabilityId::new(capability));
        self
    }

    /// Declares a capability this provider requires.
    ///
    /// The kernel initialises dependencies first, which is what lets a handler assume the
    /// resources it reads are already up.
    #[must_use]
    pub fn requires(mut self, capability: impl Into<String>) -> Self {
        self.dependencies.push(CapabilityId::new(capability));
        self
    }

    /// The address actually bound, once `Boot` has completed.
    ///
    /// `None` before the bind. Worth asking for: binding port `0` yields an assigned port, and a
    /// caller that assumed it knew the port would be talking to a different server.
    #[must_use]
    pub fn bound_address(&self) -> Option<SocketAddr> {
        *self.bound.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the server is bound and serving.
    #[must_use]
    pub fn is_serving(&self) -> bool {
        self.serving.load(Ordering::Acquire)
    }
}

impl Provider for HttpServerProvider {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn dependencies(&self) -> &[CapabilityId] {
        &self.dependencies
    }

    fn initialise<'a>(
        &'a self,
        context: &'a mut InitContext<'_>,
    ) -> renvor_core::provider::ProviderFuture<'a> {
        // Everything read from the context is read HERE, before the async block, because the
        // context borrow and the future's lifetime are otherwise entangled.
        let run_id = context.run_id();
        let cancel = context.cancel().clone();
        let gate = context.work().clone();
        let name = self.id.to_string();
        let serving = Arc::clone(&self.serving);

        context.register_readiness(Arc::new(ServerReadiness {
            name: name.clone(),
            serving: Arc::clone(&self.serving),
        }));

        Box::pin(async move {
            let config = self
                .config
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .ok_or_else(|| {
                    // A second Boot of the same provider value. Reported rather than defaulted:
                    // serving an empty configuration would be a silent fallback.
                    boxed("this server provider has already been initialised once")
                })?;
            let registry = self
                .registry
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
                .ok_or_else(|| {
                    boxed("this server provider's route registry was already consumed")
                })?;

            let drain_budget = config.limits.drain_budget;

            let router_config = RouterConfig {
                hosts: config.hosts,
                trusted_proxies: config.trusted_proxies,
                cors: config.cors,
                limits: config.limits,
                run_id,
                cancel: cancel.clone(),
                gate: gate.clone(),
                state: config.state,
            };

            // Built BEFORE the bind. An unsafe CORS configuration must fail without a socket ever
            // existing — that is what FR-022's "at configuration time" means once the server is in
            // a lifecycle.
            let app = router(&registry, router_config).map_err(|error| boxed(error.to_string()))?;

            let server = Server::bind(config.address, gate, drain_budget, cancel)
                .await
                .map_err(|error| {
                    // Propagated, never retried and never swallowed. The kernel wraps this in
                    // `ProviderInit` and rolls back every provider already initialised.
                    boxed(format!("the server could not bind: {error}"))
                })?;

            let bound = server
                .local_addr()
                .map_err(|error| boxed(format!("the bound address could not be read: {error}")))?;
            *self.bound.lock().unwrap_or_else(PoisonError::into_inner) = Some(bound);

            let (shutdown, wait) = tokio::sync::oneshot::channel::<()>();
            let task = tokio::spawn(async move {
                server
                    .serve(app, async move {
                        wait.await.ok();
                    })
                    .await
            });

            *self.running.lock().unwrap_or_else(PoisonError::into_inner) =
                Some(Running { shutdown, task });

            // Last, and with `Release`: readiness must not report ready before the listener exists.
            serving.store(true, Ordering::Release);

            tracing::info!(
                provider = %name,
                address = %bound,
                run_id = %run_id,
                "the http server is bound and serving"
            );
            Ok(())
        })
    }

    fn stop(&self) -> renvor_core::provider::ProviderFuture<'_> {
        Box::pin(async move {
            // Not ready FIRST. An orchestrator polling readiness during shutdown must stop routing
            // here rather than after the socket closes.
            self.serving.store(false, Ordering::Release);

            let Some(Running { shutdown, task }) = self
                .running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take()
            else {
                // Never started, or already stopped. Idempotent by construction — the running
                // handle is taken, so a second call finds nothing rather than consulting a flag.
                return Ok(());
            };

            // The receiver is dropped if the task already ended; that is not a failure.
            let _ = shutdown.send(());

            // Bounded twice over: `Server::serve` bounds itself by the drain budget, and the
            // kernel bounds every `stop` call by its provider deadline. Awaiting here therefore
            // cannot become the unbounded wait C-L7 forbids.
            match task.await {
                Ok(Ok(outcome)) => {
                    if outcome.is_clean() {
                        tracing::info!(provider = %self.id, "the http server drained cleanly");
                        Ok(())
                    } else {
                        // FR-007: an incomplete drain is NEVER reported as clean. Reported as a
                        // provider stop failure, which C-L4 records without aborting the rest of
                        // shutdown.
                        tracing::warn!(
                            provider = %self.id,
                            outstanding = outcome.outstanding(),
                            "the http server drain budget elapsed with work outstanding"
                        );
                        Err(boxed(outcome.to_string()))
                    }
                }
                Ok(Err(error)) => Err(boxed(format!(
                    "the http server failed while serving: {error}"
                ))),
                Err(joined) => Err(boxed(format!(
                    "the http serving task ended abnormally: {joined}"
                ))),
            }
        })
    }
}

impl core::fmt::Debug for HttpServerProvider {
    /// Names the provider and whether it is serving, and **nothing an author supplied**.
    ///
    /// The configuration holds a host allow-list and a CORS policy, which are operator
    /// configuration values — C-11 forbids emitting those. The bound address is Renvor's own
    /// observation and is safe to print.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("HttpServerProvider")
            .field("id", &self.id)
            .field("serving", &self.is_serving())
            .field("bound", &self.bound_address())
            .finish_non_exhaustive()
    }
}

/// Wraps a message as the boxed cause the kernel expects.
fn boxed(message: impl Into<String>) -> renvor_core::error::BoxedCause {
    #[derive(Debug)]
    struct ServerFailure(String);

    impl core::fmt::Display for ServerFailure {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(&self.0)
        }
    }

    impl core::error::Error for ServerFailure {}

    Box::new(ServerFailure(message.into()))
}

/// The drain budget this provider runs with, exposed so a caller can assert against it.
#[must_use]
pub const fn default_drain_budget() -> Duration {
    renvor_core::lifecycle::DEFAULT_DRAIN_BUDGET
}
