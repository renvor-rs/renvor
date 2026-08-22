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
//! So this module uses that helper for the half it does well — **stop accepting** — and bounds
//! every wait itself.
//!
//! # The bound has to be applied, not merely computed
//!
//! An earlier revision computed the right outcome and then never returned it:
//!
//! ```text
//! let (outcome, served) = tokio::join!(gate.drain_when_closed(budget), serving);
//! ```
//!
//! `join!` waits for **both**. `serving` is the future whose wait is unbounded, so in exactly the
//! case the bound exists for — work that outlives the budget — `serve` did not return. The drain
//! outcome was correct and unreachable. Its unit tests passed throughout, because they exercised
//! the drain in isolation and the defect was in the combination.
//!
//! Shutdown is now a sequence in which **every step has an upper bound**:
//!
//! ```text
//! shutdown requested
//!   │
//!   ├─ 1. close the work gate ......... admission refuses immediately (503)
//!   ├─ 2. signal the serve helper ..... the listener stops accepting
//!   ├─ 3. drain(budget) ............... BOUNDED wait for in-flight work
//!   ├─ 4. cancel, if work remains ..... abandoned work is CANCELLED, never left running
//!   └─ 5. wait out the REMAINDER ...... connection tasks get what is left of the budget
//!                                       and not one millisecond more
//! ```
//!
//! Step 5 is bounded by `budget - elapsed`, so steps 3 and 5 together never exceed the budget. If
//! it elapses, the serving task is **aborted** and that is reported through the outcome rather
//! than waited out.
//!
//! # An idle connection is not outstanding work
//!
//! A `Clean` outcome with the serving task still running is a normal, correct result: a keep-alive
//! socket with no request on it is a socket, not work. [`renvor_core::DrainOutcome`] reports
//! **work**, which is what an operator needs to know before stopping a process, and bounding on
//! the work gate rather than on the connection count is what keeps that report honest.
//!
//! # Why the outcome can be `Incomplete` and that is the point
//!
//! FR-007 prohibits reporting an incomplete drain as clean. A drain that could only ever return
//! "finished" would satisfy the prohibition by making it unobservable, which is not the same thing.

use core::time::Duration;
use std::io;
use std::net::SocketAddr;

use axum::Router;
use renvor_core::{CancelScope, DrainOutcome, WorkGate};
use tokio::net::TcpListener;

/// A bound listener that has not yet started serving.
#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
    gate: WorkGate,
    drain_budget: Duration,
    cancel: CancelScope,
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
        cancel: CancelScope,
    ) -> io::Result<Self> {
        Ok(Self {
            listener: TcpListener::bind(address).await?,
            gate,
            drain_budget,
            cancel,
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

    /// The application scope every request scope descends from.
    ///
    /// Held here because a server that abandons work at the budget must be able to **cancel** what
    /// it abandoned. A bounded shutdown that left handlers running would have replaced an
    /// unbounded wait with an unbounded leak.
    #[must_use]
    pub const fn cancel(&self) -> &CancelScope {
        &self.cancel
    }

    /// Serves `router` until `shutdown` resolves, then shuts down **within the drain budget**.
    ///
    /// Returns how the drain ended. A [`DrainOutcome::Incomplete`] names how many units of work
    /// were still outstanding when the budget elapsed; that work has been **cancelled**, and the
    /// serving task aborted, so the value is a report rather than a promise that something is
    /// still finishing.
    ///
    /// # Errors
    ///
    /// Propagates a serving failure — but only one the serving task actually produced. A task
    /// abandoned at the budget has produced no failure, and inventing one would misreport a
    /// deliberate bound as an error.
    pub async fn serve<F>(self, router: Router, shutdown: F) -> io::Result<DrainOutcome>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let Self {
            listener,
            gate,
            drain_budget,
            cancel,
        } = self;

        // Connection information is required by the identity layer, which fails closed without it.
        // `into_make_service_with_connect_info` is what attaches it.
        let service = router.into_make_service_with_connect_info::<SocketAddr>();

        // The helper's stop-accepting signal, held separately from the caller's `shutdown` future
        // so that the gate can be closed BEFORE accepting stops. A request admitted in the window
        // between the two would be in flight with no permit, and therefore invisible to the drain
        // that is about to report on it.
        let (stop_accepting, accept_signal) = tokio::sync::oneshot::channel::<()>();

        // Spawned rather than awaited inline. This is the whole fix: a future this function
        // `await`s is a future this function's own completion depends on, and the serving future's
        // wait is unbounded. Owning a `JoinHandle` means the wait can be given a deadline and the
        // task abandoned when the deadline passes.
        let mut serving = tokio::spawn(async move {
            axum::serve(listener, service)
                .with_graceful_shutdown(async move {
                    accept_signal.await.ok();
                })
                .await
        });

        shutdown.await;

        // 1. Refuse new work first, so nothing is admitted while the listener is still up.
        gate.close();
        // 2. Then stop accepting.
        let _ = stop_accepting.send(());

        // 3. The bounded wait for work already in flight.
        let started = tokio::time::Instant::now();
        let outcome = gate.drain(drain_budget).await;

        // 4. Anything still outstanding is being abandoned, so cancel it. An application service
        //    observes this through the same `CancelScope` it observes a client disconnect and a
        //    request timeout through — one mechanism, per contract C-10.
        if !outcome.is_clean() {
            cancel.cancel();
        }

        // 5. Whatever is left of the budget, and no more. Connection tasks that are merely idle
        //    finish here; one that will not finish is aborted rather than waited on.
        let remaining = drain_budget.saturating_sub(started.elapsed());
        match tokio::time::timeout(remaining, &mut serving).await {
            Ok(Ok(served)) => served?,
            // The serving task itself panicked. Reported as an error rather than swallowed: a
            // panic in the accept loop is a defect, and a shutdown that hid it would be the last
            // place it could have been seen.
            Ok(Err(panicked)) => {
                return Err(io::Error::other(format!(
                    "the serving task ended abnormally: {panicked}"
                )));
            }
            Err(_) => {
                // The budget is spent. Abandoning the task is the bound being applied, and it is
                // recorded so an operator reading the log knows a connection was cut rather than
                // closed.
                serving.abort();
                tracing::warn!(
                    budget_ms = u64::try_from(drain_budget.as_millis()).unwrap_or(u64::MAX),
                    outstanding = outcome.outstanding(),
                    "the drain budget elapsed; the serving task was abandoned"
                );
            }
        }

        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::Server;
    use core::time::Duration;
    use renvor_core::{CancelScope, DrainOutcome, WorkGate};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn any_port() -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0)
    }

    #[tokio::test]
    async fn binding_reports_the_address_actually_taken() {
        let server = Server::bind(
            any_port(),
            WorkGate::new(),
            Duration::from_secs(1),
            CancelScope::root(),
        )
        .await
        .expect("binding to an ephemeral port succeeds");

        let bound = server.local_addr().expect("the address is known");
        assert_ne!(bound.port(), 0, "the assigned port was not reported");
    }

    #[tokio::test]
    async fn a_bind_failure_propagates_rather_than_being_swallowed() {
        let first = Server::bind(
            any_port(),
            WorkGate::new(),
            Duration::from_secs(1),
            CancelScope::root(),
        )
        .await
        .expect("the first bind succeeds");
        let taken = first.local_addr().expect("address");

        // Binding the same address again must fail. A server that reported success here would make
        // readiness a lie.
        let second = Server::bind(
            taken,
            WorkGate::new(),
            Duration::from_secs(1),
            CancelScope::root(),
        )
        .await;
        assert!(second.is_err(), "a conflicting bind reported success");
    }

    #[tokio::test(start_paused = true)]
    async fn an_over_budget_drain_reports_the_work_it_abandoned() {
        // The drain's own semantics, which `serve` step 3 spends. Asserted here at the unit level
        // and again end-to-end through a real socket in `tests/shutdown.rs`, because passing here
        // and failing there is exactly what happened before: this held while `serve` could not
        // return at all.
        let gate = WorkGate::new();
        let _permit = gate.begin("in flight").expect("the gate is open");

        let outcome = gate.drain(Duration::from_secs(1)).await;

        assert_eq!(
            outcome,
            DrainOutcome::Incomplete { outstanding: 1 },
            "the over-budget drain did not report the outstanding work"
        );
        assert!(!outcome.is_clean(), "an incomplete drain reported clean");
        assert_eq!(outcome.outstanding(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn work_that_finishes_inside_the_budget_drains_cleanly() {
        // POSITIVE CONTROL for the test above: without it, an implementation that always reported
        // `Incomplete` would pass.
        let gate = WorkGate::new();
        let permit = gate.begin("short").expect("open");

        let draining = tokio::spawn({
            let gate = gate.clone();
            async move { gate.drain(Duration::from_secs(30)).await }
        });

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
        // C-L5's fourth clause: choosing an immediate stop must never silently read as a clean one.
        let gate = WorkGate::new();
        let _a = gate.begin("a").expect("open");
        let _b = gate.begin("b").expect("open");

        assert_eq!(
            gate.drain(Duration::ZERO).await,
            DrainOutcome::Incomplete { outstanding: 2 }
        );
    }

    #[tokio::test]
    async fn closing_the_gate_refuses_new_work_before_anything_is_drained() {
        // `serve` step 1. The gate closes BEFORE the listener stops accepting, so there is no
        // window in which a request is admitted without a permit the drain can see.
        let gate = WorkGate::new();
        assert!(gate.begin("before").is_ok());

        gate.close();

        assert!(
            gate.begin("after").is_err(),
            "work was admitted after the gate closed"
        );
    }
}
