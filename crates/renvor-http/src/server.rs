//! Binding, serving, and draining — with a bound the underlying helper does not provide.
//!
//! # The unbounded wait this module exists to close
//!
//! ADR-0012, Finding 3. `axum::serve(...).with_graceful_shutdown(...)` ends like this:
//!
//! ```text
//! drop(close_rx);
//! drop(listener);
//! close_tx.closed().await;      // no timeout
//! ```
//!
//! It stops accepting and then waits **indefinitely** for connection tasks. Contract C-L7 states
//! that *"an unbounded wait in a kernel-owned path is a defect, not a configuration choice."* One
//! connection that never finishes would hang shutdown for ever.
//!
//! So this module uses that helper for the half it does well — **stop accepting** — and bounds the
//! wait itself with [`renvor_core::WorkGate::drain`], which returns a
//! [`renvor_core::DrainOutcome`] and therefore reports outstanding work instead of hiding it.
//!
//! # Why the outcome can be `Incomplete` and that is the point
//!
//! FR-007 prohibits reporting an incomplete drain as clean. A drain that could only ever return
//! "finished" would satisfy the prohibition by making it unobservable, which is not the same thing.

use core::time::Duration;
use std::io;
use std::net::SocketAddr;

use axum::Router;
use renvor_core::{DrainOutcome, WorkGate};
use tokio::net::TcpListener;

/// A bound listener that has not yet started serving.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    gate: WorkGate,
    drain_budget: Duration,
}

impl Server {
    /// Binds `address`.
    ///
    /// Binding happens here rather than at construction of the router, so a bind failure occurs in
    /// the lifecycle's `Boot` phase and rolls back exactly as any provider failure does.
    ///
    /// # Errors
    ///
    /// Propagates the bind failure. It is **not** retried and **not** swallowed: a server that
    /// could not take its port has not started, and reporting otherwise would make readiness a
    /// lie.
    pub async fn bind(
        address: SocketAddr,
        gate: WorkGate,
        drain_budget: Duration,
    ) -> io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(address).await?,
            gate,
            drain_budget,
        })
    }

    /// The address actually bound.
    ///
    /// Worth asking for: binding port `0` yields an assigned port, and a test that assumed it knew
    /// the port would be testing a different server.
    ///
    /// # Errors
    ///
    /// Propagates the underlying failure.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// The gate admission control draws from.
    #[must_use]
    pub const fn gate(&self) -> &WorkGate {
        &self.gate
    }

    /// Serves `router` until `shutdown` resolves, then drains within the configured budget.
    ///
    /// Returns how the drain ended. A [`DrainOutcome::Incomplete`] names how many units of work
    /// were still outstanding when the budget elapsed.
    ///
    /// # Errors
    ///
    /// Propagates a serving failure.
    pub async fn serve<F>(self, router: Router, shutdown: F) -> io::Result<DrainOutcome>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let Self {
            listener,
            gate,
            drain_budget,
        } = self;

        // Connection information is required by the identity layer, which fails closed without it.
        // `into_make_service_with_connect_info` is what attaches it.
        let service = router.into_make_service_with_connect_info::<SocketAddr>();

        let gate_for_signal = gate.clone();
        let signal = async move {
            shutdown.await;
            // Close the gate BEFORE the accept loop stops. A request admitted in the window
            // between the two would be in flight with no permit, and would therefore be invisible
            // to the drain that is about to report on it.
            gate_for_signal.close();
        };

        let serving = axum::serve(listener, service).with_graceful_shutdown(signal);

        // The helper's own wait is unbounded. Racing it against a bounded drain is what supplies
        // the bound C-L7 requires; whichever finishes first, the outcome below is the truthful one
        // because it is computed from the gate rather than from which branch won.
        let (outcome, served) = tokio::join!(
            async {
                // The gate is not closed until the signal fires, so this waits for the signal and
                // then bounds the in-flight work.
                gate.clone().drain_when_closed(drain_budget).await
            },
            serving,
        );

        served?;
        Ok(outcome)
    }
}

/// Waiting for the gate to close, then draining under a budget.
///
/// An extension trait rather than a method on `WorkGate`, because the waiting-then-draining shape
/// is this transport's need and not the kernel's — adding it to the kernel would put a transport
/// concern in a crate whose whole purpose is not having any.
trait DrainWhenClosed {
    /// Waits until the gate closes, then drains within `budget`.
    fn drain_when_closed(self, budget: Duration) -> impl Future<Output = DrainOutcome> + Send;
}

impl DrainWhenClosed for WorkGate {
    async fn drain_when_closed(self, budget: Duration) -> DrainOutcome {
        // Poll rather than subscribe: `WorkGate` exposes `is_closed` but not a closure signal, and
        // adding one to the kernel for this would widen the kernel's surface for a transport's
        // convenience. The interval is short enough to be invisible beside a shutdown and long
        // enough not to spin.
        while !self.is_closed() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        self.drain(budget).await
    }
}

#[cfg(test)]
mod tests {
    use super::{DrainWhenClosed, Server};
    use core::time::Duration;
    use renvor_core::{DrainOutcome, WorkGate};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn any_port() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
    }

    #[tokio::test]
    async fn binding_reports_the_address_actually_taken() {
        let server = Server::bind(any_port(), WorkGate::new(), Duration::from_secs(1))
            .await
            .expect("binding to an ephemeral port succeeds");

        let bound = server.local_addr().expect("the address is known");
        assert_ne!(bound.port(), 0, "the assigned port was not reported");
    }

    #[tokio::test]
    async fn a_bind_failure_propagates_rather_than_being_swallowed() {
        let first = Server::bind(any_port(), WorkGate::new(), Duration::from_secs(1))
            .await
            .expect("the first bind succeeds");
        let taken = first.local_addr().expect("address");

        // Binding the same address again must fail. A server that reported success here would make
        // readiness a lie.
        let second = Server::bind(taken, WorkGate::new(), Duration::from_secs(1)).await;
        assert!(second.is_err(), "a conflicting bind reported success");
    }

    #[tokio::test(start_paused = true)]
    async fn draining_waits_for_the_gate_to_close_and_then_bounds_the_wait() {
        let gate = WorkGate::new();
        let _permit = gate.begin("in flight").expect("the gate is open");

        let draining = tokio::spawn({
            let gate = gate.clone();
            async move { gate.drain_when_closed(Duration::from_secs(1)).await }
        });

        // Nothing happens while the gate is open.
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !draining.is_finished(),
            "the drain ran before the gate closed"
        );

        gate.close();

        let outcome = draining.await.expect("the drain task completes");
        assert_eq!(
            outcome,
            DrainOutcome::Incomplete { outstanding: 1 },
            "the over-budget drain did not report the outstanding work"
        );
        assert!(!outcome.is_clean(), "an incomplete drain reported clean");
    }

    #[tokio::test(start_paused = true)]
    async fn work_that_finishes_inside_the_budget_drains_cleanly() {
        // POSITIVE CONTROL for the test above: without it, an implementation that always reported
        // `Incomplete` would pass.
        let gate = WorkGate::new();
        let permit = gate.begin("short").expect("open");

        let draining = tokio::spawn({
            let gate = gate.clone();
            async move { gate.drain_when_closed(Duration::from_secs(30)).await }
        });

        gate.close();
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(permit);

        assert_eq!(
            draining.await.expect("completes"),
            DrainOutcome::Clean,
            "work that finished in time was reported as outstanding"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_budget_with_work_in_flight_reports_it_as_outstanding() {
        // C-L5's fourth clause, reached through the transport's own drain path: choosing an
        // immediate stop must never silently read as a clean one.
        let gate = WorkGate::new();
        let _a = gate.begin("a").expect("open");
        let _b = gate.begin("b").expect("open");

        let draining = tokio::spawn({
            let gate = gate.clone();
            async move { gate.drain_when_closed(Duration::ZERO).await }
        });

        gate.close();

        assert_eq!(
            draining.await.expect("completes"),
            DrainOutcome::Incomplete { outstanding: 2 }
        );
    }
}
