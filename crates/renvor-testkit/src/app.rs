//! The socket-free test application (Phase 011, FR-050).
//!
//! `renvor-auth-http/tests/test_application.rs` hand-wrote this shape: a route registry, a way to
//! build a request with a loopback identity and dispatch it to the declared route without a
//! socket, and a sweep of every response so a test can assert that no secret came back. This is
//! that shape, once, for every test in every crate — with the optional kernel beside it: a
//! [`TestApplication`] can boot an [`ApplicationBuilder`](renvor_core::ApplicationBuilder) with
//! the caller's providers, so a test whose routes read providers gets them booted, and it shuts
//! the application down with the report the kernel produced.
//!
//! # What "socket-free" buys
//!
//! The route's handler runs exactly as it would behind the server — the same `Request`, the same
//! `Response` — with no port, no client, and no timing that depends on the machine. The transport
//! (host policy, CORS, limits, identity derivation) is **not** in the loop: that is what
//! `renvor-http`'s own server tests cover. A test that needs the transport starts the binary.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex, PoisonError};

use renvor_core::identity::ClientIdentity;
use renvor_core::lifecycle::application::Application;
use renvor_core::lifecycle::rollback::BootFailure;
use renvor_core::observe::entropy::OsEntropy;
use renvor_core::{ApplicationBuilder, CancelScope, RunIdentifier, TypedStateMap};
use renvor_http::route::{Method, PresentedCredentials, Request, Response, RouteRegistry};
use renvor_http::{EffectiveOrigin, FetchMetadata, RequestContext, RequestId, Scheme};

/// One dispatched request's answer: the status, the body as text, and the headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispatched {
    /// The status code.
    pub status: u16,
    /// The body, lossily as UTF-8.
    pub body: String,
    /// Every header, in order.
    pub headers: Vec<(String, String)>,
}

impl Dispatched {
    /// The first header named `name`, case-insensitively.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// What the kernel reported when the booted application stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownOutcome {
    /// Every provider stopped within its bound.
    pub clean: bool,
    /// The providers that did not, rendered.
    pub failures: Vec<String>,
}

/// A registry to dispatch into, an optional booted kernel beside it, and every answer swept.
pub struct TestApplication {
    registry: RouteRegistry,
    application: Option<Application>,
    listener: EffectiveOrigin,
    swept: Mutex<Vec<String>>,
    request_ids: Mutex<u64>,
    run_id: RunIdentifier,
    state: Arc<TypedStateMap>,
}

impl std::fmt::Debug for TestApplication {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestApplication")
            .field("routes", &self.registry.routes().len())
            .field("booted", &self.application.is_some())
            .finish()
    }
}

impl TestApplication {
    /// A registry alone: handlers that need no booted provider, or providers the test booted
    /// itself.
    #[must_use]
    pub fn new(registry: RouteRegistry) -> Self {
        Self {
            registry,
            application: None,
            listener: EffectiveOrigin::new(Scheme::Http, "example.test", 80),
            swept: Mutex::new(Vec::new()),
            request_ids: Mutex::new(0),
            run_id: RunIdentifier::generate(&OsEntropy).expect("the operating system has entropy"),
            state: Arc::new(TypedStateMap::new()),
        }
    }

    /// The typed state every dispatched request carries — for handlers that read what the test
    /// built rather than what a booted provider registered. A booted application's own map is
    /// attached by [`Self::boot`]; this replaces it.
    #[must_use]
    pub fn with_state(mut self, state: Arc<TypedStateMap>) -> Self {
        self.state = state;
        self
    }

    /// The one run identifier every request from this application carries: the booted
    /// application's, or one generated when the registry alone was wrapped.
    #[must_use]
    pub const fn run_id(&self) -> RunIdentifier {
        self.run_id
    }

    /// Boots `builder` — the caller's providers and configuration sources — and keeps the
    /// application beside the registry, so [`Self::shutdown`] returns the kernel's report.
    ///
    /// # Errors
    ///
    /// The kernel's own [`BootFailure`], with every provider that had booted rolled back.
    #[allow(
        clippy::result_large_err,
        reason = "Preserves the kernel's structured BootFailure in the existing testkit API; the \
                  larger error representation is an accepted startup-path trade-off."
    )]
    pub async fn boot(
        builder: ApplicationBuilder,
        registry: RouteRegistry,
    ) -> Result<Self, BootFailure> {
        let application = builder
            .build()
            .map_err(|error| panic!("the application did not build: {error}"))
            .unwrap_or_else(|never: std::convert::Infallible| match never {});
        let application = application.boot().await?;
        let mut this = Self::new(registry);
        this.run_id = *application.run_id();
        // The map the caller's providers registered into, shared with every request — the shape
        // a served request has, where the transport hands handlers the application's real state.
        if let Some(state) = application.shared_state() {
            this.state = state;
        }
        this.application = Some(application);
        Ok(this)
    }

    /// The origin every request is addressed to; `example.test` over plain HTTP by default.
    #[must_use]
    pub fn with_listener(mut self, listener: EffectiveOrigin) -> Self {
        self.listener = listener;
        self
    }

    /// The registry, for a test that asserts on declarations.
    #[must_use]
    pub fn registry(&self) -> &RouteRegistry {
        &self.registry
    }

    /// A request from the loopback peer, addressed to the listener, with a fresh request id.
    #[must_use]
    pub fn request(&self, body: impl Into<Vec<u8>>) -> Request {
        self.request_to(&self.listener, body)
    }

    /// A request addressed to `listener` rather than the default — for a test that proves what
    /// a request for another origin is answered.
    #[must_use]
    pub fn request_to(&self, listener: &EffectiveOrigin, body: impl Into<Vec<u8>>) -> Request {
        self.request_as(
            ClientIdentity::DirectPeer(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            listener,
            body,
        )
    }

    /// A request from `client` rather than the loopback peer — for a test that proves what a
    /// route promised to loopback answers a caller from elsewhere.
    #[must_use]
    pub fn request_from(&self, client: ClientIdentity, body: impl Into<Vec<u8>>) -> Request {
        self.request_as(client, &self.listener, body)
    }

    /// Every request: this application's one run id, the next request id, the state the booted
    /// providers registered (or the test attached), and a fresh cancellation scope.
    fn request_as(
        &self,
        client: ClientIdentity,
        listener: &EffectiveOrigin,
        body: impl Into<Vec<u8>>,
    ) -> Request {
        let mut counter = self
            .request_ids
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *counter += 1;
        let context = RequestContext::new(
            self.run_id,
            RequestId::from_entropy(counter.to_le_bytes()),
            client,
            listener.clone(),
            CancelScope::root().child("request"),
        );
        Request::new(context, body.into(), String::new(), BTreeMap::new())
            .with_state(Arc::clone(&self.state))
    }

    /// Dispatches `request` to the route declared for `method` and `path`, and sweeps the answer.
    ///
    /// # Panics
    ///
    /// When no route declares that path for that method — a test asking for a route the
    /// application does not have is a test defect, not an answer.
    pub async fn dispatch(&self, method: Method, path: &str, request: Request) -> Dispatched {
        let route = self
            .registry
            .routes()
            .iter()
            .find(|candidate| candidate.path() == path && candidate.method() == method)
            .unwrap_or_else(|| panic!("no route declares `{method:?} {path}`"));
        let response = route.dispatch(request).await;
        self.sweep(response)
    }

    /// Dispatches a JSON body with a cookie, the shape most authenticated flows take.
    pub async fn send(&self, method: Method, path: &str, body: &str, cookie: &str) -> Dispatched {
        let request = self
            .request(body.as_bytes().to_vec())
            .with_credentials(PresentedCredentials::new(cookie, ""))
            .with_fetch_metadata(FetchMetadata::default());
        self.dispatch(method, path, request).await
    }

    fn sweep(&self, response: Response) -> Dispatched {
        let dispatched = Dispatched {
            status: response.status_code(),
            body: String::from_utf8_lossy(response.body()).into_owned(),
            headers: response.headers().to_vec(),
        };
        let mut swept = self.swept.lock().unwrap_or_else(PoisonError::into_inner);
        swept.push(dispatched.body.clone());
        for (name, value) in &dispatched.headers {
            swept.push(format!("{name}: {value}"));
        }
        dispatched
    }

    /// Everything every answer carried — bodies and headers — for a canary sweep.
    #[must_use]
    pub fn swept(&self) -> Vec<String> {
        self.swept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Panics naming the first canary that any answer carried — **by position**, never by
    /// value: the canary is the secret the sweep exists to keep out of output, and the entry that
    /// carried it is the response that leaked it, so a failure that printed either would put the
    /// secret into the test output it was guarding (SR-001; found by the Standards review of
    /// Phase 011). The message names the canary's index among `canaries` and the entry's index
    /// among the swept answers.
    ///
    /// # Panics
    ///
    /// When a swept body or header contains one of `canaries`.
    pub fn assert_nothing_swept_contains(&self, canaries: &[&str]) {
        for (entry, line) in self.swept().iter().enumerate() {
            for (position, canary) in canaries.iter().enumerate() {
                assert!(
                    !line.contains(canary),
                    "a swept response carried canary {position} of {} in entry {entry} ({} bytes); \
                     neither the canary nor the entry is printed, by design",
                    canaries.len(),
                    line.len()
                );
            }
        }
    }

    /// Stops the booted application, if one was booted, and returns what the kernel reported.
    pub async fn shutdown(mut self) -> Option<ShutdownOutcome> {
        let mut application = self.application.take()?;
        let report = application.shutdown().await;
        Some(ShutdownOutcome {
            clean: report.is_clean(),
            failures: report
                .stop()
                .failures()
                .iter()
                .map(ToString::to_string)
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn hello(request: Request) -> Response {
        let body = format!("hello {}", String::from_utf8_lossy(request.body()));
        Response::text(body)
    }

    #[tokio::test]
    async fn a_request_is_dispatched_to_the_declared_route_and_its_answer_is_swept() {
        let mut registry = RouteRegistry::new();
        registry.post("/hello", hello).expect("route");
        let app = TestApplication::new(registry);
        let answer = app.send(Method::Post, "/hello", "canary-8f1c", "").await;
        assert_eq!(answer.status, 200);
        assert_eq!(answer.body, "hello canary-8f1c");
        assert!(answer.header("content-type").is_some());
        assert!(app.swept().iter().any(|line| line.contains("canary-8f1c")));
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            app.assert_nothing_swept_contains(&["canary-8f1c"]);
        }));
        assert!(
            caught.is_err(),
            "the sweep must name a canary that came back"
        );
        app.assert_nothing_swept_contains(&["something-else"]);
        assert!(app.shutdown().await.is_none(), "nothing was booted");
    }

    #[tokio::test]
    async fn an_application_booted_beside_the_registry_shuts_down_with_a_report() {
        let mut registry = RouteRegistry::new();
        registry.get("/", hello).expect("route");
        let app = TestApplication::boot(ApplicationBuilder::new(), registry)
            .await
            .expect("boots with no providers");
        let answer = app.send(Method::Get, "/", "", "").await;
        assert_eq!(answer.status, 200);
        let report = app.shutdown().await.expect("a booted application reports");
        assert!(report.clean, "{:?}", report.failures);
    }

    /// A value a provider publishes at Boot and a route reads.
    struct Published(&'static str);

    struct Publisher {
        id: renvor_core::provider::ProviderId,
    }

    impl renvor_core::provider::Provider for Publisher {
        fn id(&self) -> &renvor_core::provider::ProviderId {
            &self.id
        }

        fn initialise<'a>(
            &'a self,
            context: &'a mut renvor_core::provider::InitContext<'_>,
        ) -> renvor_core::provider::ProviderFuture<'a> {
            let registered = context.register_state(Published("from-boot"));
            Box::pin(async move {
                registered.map_err(|error| Box::new(error) as renvor_core::error::BoxedCause)
            })
        }
    }

    async fn reads_state(request: Request) -> Response {
        match request.state::<Published>() {
            Ok(value) => Response::text(value.0.to_owned()),
            Err(error) => Response::status(500)
                .expect("500 is valid")
                .with_body(error.to_string()),
        }
    }

    #[tokio::test]
    async fn a_request_carries_the_state_the_booted_providers_registered() {
        // FOUND BY THE CODEX REVIEW (P2). `boot` kept the application but built every request
        // with an empty state map, so a route that read what one of the caller's providers had
        // registered got `StateMissing` — the provider-backed harness FR-050 promises was
        // unusable. The booted application's state is shared with every request.
        let mut registry = RouteRegistry::new();
        registry.get("/state", reads_state).expect("route");
        let builder = ApplicationBuilder::new().with_provider(Box::new(Publisher {
            id: renvor_core::provider::ProviderId::new("publisher"),
        }));
        let app = TestApplication::boot(builder, registry)
            .await
            .expect("boots");
        let answer = app.send(Method::Get, "/state", "", "").await;
        assert_eq!(answer.status, 200, "{}", answer.body);
        assert_eq!(answer.body, "from-boot");
        let report = app.shutdown().await.expect("a booted application reports");
        assert!(report.clean, "{:?}", report.failures);
    }

    #[tokio::test]
    async fn every_request_carries_the_applications_one_run_identifier() {
        // FOUND BY THE CODEX REVIEW (P2). Every request generated a fresh `RunIdentifier`, so
        // handler telemetry could not be correlated with the application's lifecycle, unlike a
        // served request, which carries the provider's one run id. One id per test application:
        // the booted application's, or one generated once for a registry alone.
        let mut registry = RouteRegistry::new();
        registry.get("/", hello).expect("route");
        let app = TestApplication::boot(ApplicationBuilder::new(), registry)
            .await
            .expect("boots");
        let first = app.request(Vec::new()).context().run_id();
        let second = app.request(Vec::new()).context().run_id();
        assert_eq!(first, second, "two requests, two run ids");
        assert_eq!(
            first,
            app.run_id(),
            "the requests carry the application's run id"
        );
        let _ = app.shutdown().await;

        let mut registry = RouteRegistry::new();
        registry.get("/", hello).expect("route");
        let bare = TestApplication::new(registry);
        assert_eq!(
            bare.request(Vec::new()).context().run_id(),
            bare.request(Vec::new()).context().run_id()
        );
    }

    use crate::every_rendering_of as every_form_of;

    #[tokio::test]
    async fn a_failed_sweep_names_the_canary_by_index_and_never_by_value() {
        // STANDARDS AXIS (P1). The failure message carried the matched canary and the whole
        // response line — the very secret the sweep exists to keep out of output, printed by the
        // check that found it. A failure now says which canary (by position) and which swept
        // entry (by position), and nothing of either's text.
        let mut registry = RouteRegistry::new();
        registry.post("/hello", hello).expect("route");
        let app = TestApplication::new(registry);
        let secret = "s3cr3t-canary-7c1d";
        let _ = app.send(Method::Post, "/hello", secret, "").await;
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            app.assert_nothing_swept_contains(&["decoy-that-never-appears", secret]);
        }));
        let payload = caught.expect_err("the sweep must fail on a canary that came back");
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .expect("a text panic");
        for form in every_form_of(secret) {
            assert!(
                !message.contains(&form),
                "the failure message carries the canary as {form:?}: {message}"
            );
        }
        assert!(
            message.contains("canary 1") && message.contains("entry"),
            "the message names the canary and the entry by position: {message}"
        );
    }

    #[tokio::test]
    #[should_panic(expected = "no route declares")]
    async fn a_route_the_application_does_not_have_is_a_test_defect() {
        let app = TestApplication::new(RouteRegistry::new());
        let request = app.request(Vec::new());
        let _ = app.dispatch(Method::Get, "/missing", request).await;
    }
}
