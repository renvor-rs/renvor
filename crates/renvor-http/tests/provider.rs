//! FR-033 and contract C-10's opening clause — the HTTP server participates in the kernel lifecycle.
//!
//! # These drive a real `Application`, not a description of one
//!
//! Every test builds a `renvor_core::Application` through its own builder, boots it, and shuts it
//! down. Nothing here asserts against the lifecycle contract's text; the contract's claims are
//! observed as behaviour, which is the only form of evidence FR-028 accepts for ordering.

use core::time::Duration;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdListener};
use std::sync::{Arc, Mutex};

use renvor_core::{
    ApplicationBuilder, CapabilityId, InitContext, Provider, ProviderId, Readiness,
};
use renvor_http::route::{Request, Response, RouteRegistry};
use renvor_http::{HostPolicy, HttpServerConfig, HttpServerProvider};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const HOST: &str = "example.test";

/// What happened, in the order it happened. The only way to assert an ordering claim.
type Journal = Arc<Mutex<Vec<&'static str>>>;

fn journal() -> Journal {
    Arc::new(Mutex::new(Vec::new()))
}

fn record(journal: &Journal, event: &'static str) {
    journal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(event);
}

fn entries(journal: &Journal) -> Vec<&'static str> {
    journal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

/// Stands for a resource provider the transport depends on — a connection pool, say.
///
/// Records when it stops, which is what makes "the server drained before this stopped" assertable.
struct Dependency {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    journal: Journal,
}

impl Provider for Dependency {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn initialise<'a>(
        &'a self,
        _: &'a mut InitContext<'_>,
    ) -> renvor_core::provider::ProviderFuture<'a> {
        Box::pin(async move {
            record(&self.journal, "dependency-started");
            Ok(())
        })
    }

    fn stop(&self) -> renvor_core::provider::ProviderFuture<'_> {
        Box::pin(async move {
            record(&self.journal, "dependency-stopped");
            Ok(())
        })
    }
}

fn any_port() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

fn config(address: SocketAddr) -> HttpServerConfig {
    let mut config = HttpServerConfig::new(address);
    config.hosts = HostPolicy::deny_all().allow(HOST).expect("a valid host");
    // Short, so an over-budget case is visible without making the suite slow.
    config.limits.drain_budget = Duration::from_secs(2);
    config
}

async fn quick(_: Request) -> Response {
    Response::text("ok")
}

/// Sends a complete request and reads the whole response.
async fn round_trip(address: SocketAddr, path: &str) -> String {
    let mut stream = TcpStream::connect(address).await.expect("connects");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {HOST}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.expect("writes");
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), stream.read_to_end(&mut response))
        .await
        .expect("the response arrives")
        .expect("the read succeeds");
    String::from_utf8_lossy(&response).into_owned()
}

#[tokio::test]
async fn the_server_boots_inside_a_real_application_and_then_serves() {
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("route");

    let provider = Arc::new(HttpServerProvider::new(
        "http",
        config(any_port()),
        registry,
    ));

    // Registered as a trait object; the `Arc` is kept so the test can read the bound address.
    let application = ApplicationBuilder::new()
        .with_provider(Box::new(ArcProvider(Arc::clone(&provider))))
        .build()
        .expect("the application builds");

    // Before Boot: nothing is bound, and readiness says so.
    assert_eq!(provider.bound_address(), None, "a socket existed before Boot");
    assert!(!provider.is_serving());

    let mut application = application.boot().await.expect("boot succeeds");

    let address = provider
        .bound_address()
        .expect("Boot bound the listener");
    assert_ne!(address.port(), 0, "the assigned port was not recorded");
    assert!(provider.is_serving());

    assert_eq!(
        application.health().readiness().readiness,
        Readiness::Ready,
        "the application was not ready with the server bound and serving"
    );

    let response = round_trip(address, "/health").await;
    assert!(response.contains("200"), "{response}");
    assert!(response.contains("ok"), "{response}");

    application.shutdown().await;
    assert!(!provider.is_serving(), "the server still reports serving");
}

#[tokio::test]
async fn a_bind_failure_aborts_boot_and_rolls_the_other_providers_back() {
    // FR-033 and C-L3. A provider that cannot take its port has not started, and the kernel must
    // treat that exactly as it treats any other provider failure.
    let occupied = StdListener::bind("127.0.0.1:0").expect("takes a port");
    let taken: SocketAddr = occupied.local_addr().expect("address");

    let log = journal();
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("route");

    let application = ApplicationBuilder::new()
        .with_provider(Box::new(Dependency {
            id: ProviderId::new("dependency"),
            provides: vec![CapabilityId::new("database")],
            journal: Arc::clone(&log),
        }))
        .with_provider(Box::new(
            HttpServerProvider::new("http", config(taken), registry).requires("database"),
        ))
        .build()
        .expect("builds");

    let failure = application
        .boot()
        .await
        .expect_err("binding an occupied port must fail Boot");

    // The dependency started, then was rolled back. Both halves matter: a rollback that never ran
    // and a dependency that never started are indistinguishable from the failure alone.
    assert_eq!(
        entries(&log),
        vec!["dependency-started", "dependency-stopped"],
        "the already-initialised provider was not rolled back"
    );

    let rendered = format!("{failure:?}");
    assert!(
        rendered.contains("http"),
        "the failure did not name the provider that failed: {rendered}"
    );

    drop(occupied);
}

#[tokio::test]
async fn boot_succeeds_on_a_free_port_which_is_why_the_failure_above_is_about_the_port() {
    // POSITIVE CONTROL for the test above. Without it, a provider that failed to boot for ANY
    // reason would pass, and "the bind failed" would be unproven.
    let log = journal();
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("route");

    let application = ApplicationBuilder::new()
        .with_provider(Box::new(Dependency {
            id: ProviderId::new("dependency"),
            provides: vec![CapabilityId::new("database")],
            journal: Arc::clone(&log),
        }))
        .with_provider(Box::new(
            HttpServerProvider::new("http", config(any_port()), registry).requires("database"),
        ))
        .build()
        .expect("builds");

    let mut application = application
        .boot()
        .await
        .expect("the same configuration on a FREE port boots");

    assert_eq!(entries(&log), vec!["dependency-started"]);
    application.shutdown().await;
}

#[tokio::test]
async fn the_server_drains_before_the_provider_it_depends_on_stops() {
    // C-L1 fixes `Drain` before `Stop`, and C-L3 fixes reverse-initialisation stop order. Together
    // they mean an in-flight request finishes before the resource it is using is torn down.
    //
    // Asserted by ORDER OF OBSERVED EVENTS. The contract's text is not evidence for its own claim.
    let log = journal();

    let handler_log = Arc::clone(&log);
    let slow = move |_: Request| {
        let handler_log = Arc::clone(&handler_log);
        async move {
            // Long enough that a shutdown which did not wait would stop the dependency first.
            tokio::time::sleep(Duration::from_millis(250)).await;
            record(&handler_log, "request-finished");
            Response::text("ok")
        }
    };

    let mut registry = RouteRegistry::new();
    registry.get("/slow", slow).expect("route");

    let provider = Arc::new(HttpServerProvider::new(
        "http",
        config(any_port()),
        registry,
    ));

    let application = ApplicationBuilder::new()
        .with_provider(Box::new(Dependency {
            id: ProviderId::new("dependency"),
            provides: vec![CapabilityId::new("database")],
            journal: Arc::clone(&log),
        }))
        .with_provider(Box::new(ArcProvider(Arc::clone(&provider))))
        .build()
        .expect("builds");

    let mut application = application.boot().await.expect("boots");
    let address = provider.bound_address().expect("bound");

    // Put a request in flight, then shut down while it is still running.
    let inflight = tokio::spawn(async move { round_trip(address, "/slow").await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    application.shutdown().await;
    let response = inflight.await.expect("the request task completes");

    let observed = entries(&log);
    let finished = observed
        .iter()
        .position(|event| *event == "request-finished")
        .expect("the in-flight request never completed");
    let stopped = observed
        .iter()
        .position(|event| *event == "dependency-stopped")
        .expect("the dependency never stopped");

    assert!(
        finished < stopped,
        "the dependency was stopped before the in-flight request drained: {observed:?}"
    );
    assert!(
        response.contains("200"),
        "the drained request did not complete successfully: {response}"
    );
}

#[tokio::test]
async fn readiness_reports_not_ready_once_shutdown_begins() {
    // C-O8 / C-10: entering Drain makes readiness negative while liveness stays positive.
    let mut registry = RouteRegistry::new();
    registry.get("/health", quick).expect("route");

    let provider = Arc::new(HttpServerProvider::new(
        "http",
        config(any_port()),
        registry,
    ));

    let application = ApplicationBuilder::new()
        .with_provider(Box::new(ArcProvider(Arc::clone(&provider))))
        .build()
        .expect("builds");

    let mut application = application.boot().await.expect("boots");

    // POSITIVE CONTROL: ready before shutdown, so the negative below is caused by the shutdown.
    assert_eq!(application.health().readiness().readiness, Readiness::Ready);

    application.shutdown().await;

    let report = application.health().readiness();
    assert_eq!(report.readiness, Readiness::NotReady);
    assert!(report.draining, "the report did not say it was draining");
    assert!(
        !provider.is_serving(),
        "the server contributed ready while stopped"
    );
}

/// Lets a test keep a handle on a provider the application owns.
///
/// The builder takes `Box<dyn Provider>` by value, so without this a test could not read the bound
/// address the provider recorded — and the assigned port is exactly what a port-`0` bind makes
/// unknowable in advance.
struct ArcProvider(Arc<HttpServerProvider>);

impl Provider for ArcProvider {
    fn id(&self) -> &ProviderId {
        self.0.id()
    }

    fn provides(&self) -> &[CapabilityId] {
        Provider::provides(&*self.0)
    }

    fn dependencies(&self) -> &[CapabilityId] {
        self.0.dependencies()
    }

    fn initialise<'a>(
        &'a self,
        context: &'a mut InitContext<'_>,
    ) -> renvor_core::provider::ProviderFuture<'a> {
        self.0.initialise(context)
    }

    fn stop(&self) -> renvor_core::provider::ProviderFuture<'_> {
        self.0.stop()
    }
}
