//! FR-032 and C-L7 — shutdown is bounded, end to end, through `Server::serve`.
//!
//! # Why these drive a real socket
//!
//! The drain computation already had unit tests, and they passed while `Server::serve` could not
//! return at all: the tests exercised `drain_when_closed` in isolation, and the defect was in how
//! `serve` combined it with the serving future. A test that cannot observe that combination cannot
//! observe the defect.
//!
//! So every test here binds a real listener, connects a real client, and measures the wall-clock
//! time `serve` takes to return.
//!
//! # Every assertion has an upper bound
//!
//! An unbounded wait fails these as a **timeout with a message**, never as a hung test run. A test
//! that hangs reports nothing; a test that fails at a deadline reports exactly which bound was
//! exceeded.

use core::time::Duration;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use renvor_core::{CancelScope, DrainOutcome, OsEntropy, RunIdentifier, TypedStateMap, WorkGate};
use renvor_http::Scheme;
use renvor_http::route::build::{RouterConfig, router};
use renvor_http::{
    CorsPolicy, HostPolicy, Limits, Request, Response, RouteRegistry, Server, TrustedProxies,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const HOST: &str = "example.test";

/// The budget every test runs under. Short, so a bounded shutdown is visibly different from an
/// unbounded one without making the suite slow.
const BUDGET: Duration = Duration::from_millis(300);

/// The deadline a *failing* test reports at. Far above `BUDGET`, so exceeding it means the bound
/// was not applied at all rather than that the machine was briefly busy.
const FAILURE_DEADLINE: Duration = Duration::from_secs(5);

fn config(gate: WorkGate, cancel: CancelScope) -> RouterConfig {
    RouterConfig {
        hosts: HostPolicy::deny_all().allow(HOST).expect("a valid host"),
        trusted_proxies: TrustedProxies::none(),
        cors: CorsPolicy::deny_all(),
        public_scheme: Scheme::Http,
        limits: Limits::new(),
        run_id: RunIdentifier::generate(&OsEntropy).expect("entropy"),
        cancel,
        gate,
        state: Arc::new(TypedStateMap::new()),
    }
}

/// Never returns. Stands for a handler waiting on something that will not arrive.
async fn never_finishes(_: Request) -> Response {
    std::future::pending::<()>().await;
    Response::text("unreachable")
}

async fn quick(_: Request) -> Response {
    Response::text("ok")
}

fn any_port() -> SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
}

/// Opens a connection and sends a complete request, without reading the response.
///
/// Returns the stream so the caller can hold the connection open — dropping it would close the
/// socket and let the server finish, which is the opposite of what these tests need to observe.
async fn send_request(address: SocketAddr, path: &str) -> TcpStream {
    let mut stream = TcpStream::connect(address).await.expect("connects");
    let request = format!("GET {path} HTTP/1.1\r\nHost: {HOST}\r\nConnection: keep-alive\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("the request is written");
    stream.flush().await.expect("flushes");
    stream
}

#[tokio::test]
async fn a_handler_that_never_finishes_does_not_extend_shutdown_beyond_the_budget() {
    // THE DEFECT THIS FILE EXISTS FOR. `serve` awaited both the bounded drain and the serving
    // future, and the serving future's own wait has no timeout — so in exactly the case the bound
    // exists for, `serve` never returned.
    let gate = WorkGate::new();
    let cancel = CancelScope::root();

    let mut registry = RouteRegistry::new();
    registry.get("/slow", never_finishes).expect("route");
    let app = router(&registry, config(gate.clone(), cancel.clone())).expect("valid");

    let server = Server::bind(any_port(), gate.clone(), BUDGET, cancel)
        .await
        .expect("binds");
    let address = server.local_addr().expect("address");

    let (trigger, wait) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        server
            .serve(app, async {
                wait.await.ok();
            })
            .await
    });

    // Hold the connection open with a request that will never complete.
    let _held = send_request(address, "/slow").await;

    // Wait until the request has actually been admitted, so the drain has something to report.
    let admitted = tokio::time::timeout(Duration::from_secs(2), async {
        while gate.outstanding() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(admitted.is_ok(), "the request was never admitted");

    let started = std::time::Instant::now();
    trigger.send(()).expect("the shutdown signal is received");

    let returned = tokio::time::timeout(FAILURE_DEADLINE, serving).await;
    let elapsed = started.elapsed();

    let outcome = returned
        .expect("`serve` did not return within the failure deadline: the shutdown is UNBOUNDED")
        .expect("the serving task did not panic")
        .expect("serving did not fail");

    assert_eq!(
        outcome,
        DrainOutcome::Incomplete { outstanding: 1 },
        "the outstanding request was not reported"
    );
    // Asserted against a multiple of the BUDGET, not against FAILURE_DEADLINE. `elapsed` is timed
    // from the shutdown trigger, so the contract predicts roughly one budget — steps 3 and 5 share
    // it. Comparing against the 5-second failure deadline would let a shutdown over-run by three
    // seconds and still report that the bound held, which is the assertion agreeing with its own
    // failure message rather than with the contract. The headroom is for loaded CI runners.
    assert!(
        elapsed < BUDGET * 5,
        "shutdown took {elapsed:?}, which is not bounded by {BUDGET:?}"
    );
}

#[tokio::test]
async fn work_that_finishes_before_the_budget_shuts_down_cleanly() {
    // POSITIVE CONTROL for the test above. Without it, a `serve` that always returned
    // `Incomplete` immediately would pass, and the bound would be unproven.
    let gate = WorkGate::new();
    let cancel = CancelScope::root();

    let mut registry = RouteRegistry::new();
    registry.get("/quick", quick).expect("route");
    let app = router(&registry, config(gate.clone(), cancel.clone())).expect("valid");

    let server = Server::bind(any_port(), gate.clone(), BUDGET, cancel)
        .await
        .expect("binds");
    let address = server.local_addr().expect("address");

    let (trigger, wait) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        server
            .serve(app, async {
                wait.await.ok();
            })
            .await
    });

    // A complete request/response exchange, so nothing is in flight when shutdown begins.
    let mut stream = send_request(address, "/quick").await;
    let mut buffer = [0_u8; 64];
    let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buffer))
        .await
        .expect("the response arrives")
        .expect("the read succeeds");
    assert!(read > 0, "no response was read");
    drop(stream);

    trigger.send(()).expect("signal");
    let outcome = tokio::time::timeout(FAILURE_DEADLINE, serving)
        .await
        .expect("`serve` returned within the deadline")
        .expect("no panic")
        .expect("serving did not fail");

    assert_eq!(
        outcome,
        DrainOutcome::Clean,
        "work that finished in time was reported as outstanding"
    );
}

#[tokio::test]
async fn no_request_is_admitted_once_shutdown_has_begun() {
    // FR-030 / SC-010, observed through a real socket rather than through the admission unit.
    let gate = WorkGate::new();
    let cancel = CancelScope::root();

    let mut registry = RouteRegistry::new();
    registry.get("/quick", quick).expect("route");
    let app = router(&registry, config(gate.clone(), cancel.clone())).expect("valid");

    let server = Server::bind(any_port(), gate.clone(), BUDGET, cancel)
        .await
        .expect("binds");
    let address = server.local_addr().expect("address");

    let (trigger, wait) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        server
            .serve(app, async {
                wait.await.ok();
            })
            .await
    });

    // POSITIVE CONTROL: before shutdown, this address serves.
    let mut before = send_request(address, "/quick").await;
    let mut buffer = [0_u8; 64];
    let read = tokio::time::timeout(Duration::from_secs(2), before.read(&mut buffer))
        .await
        .expect("responds")
        .expect("reads");
    assert!(
        String::from_utf8_lossy(&buffer[..read]).contains("200"),
        "the control request did not succeed"
    );
    drop(before);

    trigger.send(()).expect("signal");
    tokio::time::timeout(FAILURE_DEADLINE, serving)
        .await
        .expect("`serve` returned")
        .expect("no panic")
        .expect("serving did not fail");

    // The gate is closed, so admission refuses regardless of whether a socket still exists.
    assert!(gate.is_closed(), "the gate was not closed by shutdown");
    assert!(
        gate.begin("a request arriving after shutdown").is_err(),
        "a request was admitted after shutdown began"
    );
}

#[tokio::test]
async fn abandoning_in_flight_work_cancels_it_rather_than_leaving_it_running() {
    // C-10: application shutdown cancels every in-flight request. A bounded shutdown that
    // abandoned its work WITHOUT cancelling would leave handlers running against an application
    // that has stopped, which is the failure the bound was supposed to prevent.
    let gate = WorkGate::new();
    let cancel = CancelScope::root();

    let mut registry = RouteRegistry::new();
    registry.get("/slow", never_finishes).expect("route");
    let app = router(&registry, config(gate.clone(), cancel.clone())).expect("valid");

    let server = Server::bind(any_port(), gate.clone(), BUDGET, cancel.clone())
        .await
        .expect("binds");
    let address = server.local_addr().expect("address");

    let (trigger, wait) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        server
            .serve(app, async {
                wait.await.ok();
            })
            .await
    });

    let _held = send_request(address, "/slow").await;
    let admitted = tokio::time::timeout(Duration::from_secs(2), async {
        while gate.outstanding() == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await;
    assert!(admitted.is_ok(), "the request was never admitted");

    assert!(
        !cancel.is_cancelled(),
        "the scope was cancelled before shutdown was even requested"
    );

    trigger.send(()).expect("signal");
    tokio::time::timeout(FAILURE_DEADLINE, serving)
        .await
        .expect("`serve` returned")
        .expect("no panic")
        .expect("serving did not fail");

    assert!(
        cancel.is_cancelled(),
        "work was abandoned at the budget without being cancelled"
    );
}
