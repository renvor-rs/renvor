//! Containing a provider that panics, so a misbehaving provider is a **failure** and not an abort.
//!
//! # Three approaches, and why this is the one
//!
//! C-L9 requires `Panic` to be an injectable behaviour at every phase, which means the kernel has
//! to survive a provider that unwinds. `std::panic::catch_unwind` alone cannot do it: a provider's
//! future can panic at **any** await point, long after the call that created it returned.
//!
//! | Approach | New packages | `unsafe` | Works |
//! |---|---|---|---|
//! | `futures::FutureExt::catch_unwind` | **`futures-util` and its tree** — none of it in the lockfile today | no | yes |
//! | `tokio::spawn`, reading `JoinError::is_panic` | none | no | **no** — needs a `'static` future, and [`crate::provider::InitContext`] borrows the state map. Forcing it would put the state map behind a `Mutex` and stop `InitContext::state` returning a borrow, changing the provider API to buy a panic guard |
//! | **This module** | **none** | **none** | yes |
//!
//! # Why no `unsafe` is needed, which is the whole trick
//!
//! Wrapping an arbitrary future normally requires projecting `Pin<&mut Self>` down to
//! `Pin<&mut F>`, and that needs either `unsafe` or a projection macro. This workspace declares
//! `unsafe_code = "forbid"`, so neither was available.
//!
//! It is not needed here, because [`crate::provider::ProviderFuture`] is **already**
//! `Pin<Box<dyn Future>>` — and `Pin<Box<T>>` is `Unpin`. A struct holding one is therefore `Unpin`
//! too, so `Pin::get_mut` is safe and free, and the inner future is polled through
//! `Pin::as_mut`. The trait had to be boxed to be dyn-compatible; that decision pays a second time
//! here.
//!
//! # What a contained panic costs
//!
//! A panicked future is **never polled again** — this returns `Poll::Ready` and the future is
//! dropped. That is what makes `AssertUnwindSafe` sound in this position: the future may well be
//! in an inconsistent state, and nothing observes it again.
//!
//! The panic **still prints** through the process's panic hook. That is deliberate: containing a
//! panic should not hide it, and the hook's output carries the backtrace this module cannot.

use core::any::Any;
use core::fmt;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::error::BoxedCause;
use crate::provider::ProviderFuture;

/// A provider panicked instead of returning.
///
/// Carries the panic's message when it had one, because "the provider panicked" without the reason
/// sends an author to a debugger for something the panic already said.
#[derive(Debug)]
pub struct Panicked {
    message: String,
}

impl Panicked {
    /// The panic's message, or a stand-in when the payload was not a string.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Reads the message out of a panic payload.
    ///
    /// `panic!` with a literal produces a `&'static str`; with formatting arguments it produces a
    /// `String`. Anything else — a `panic_any` with a custom type — has no text, and saying so is
    /// better than inventing some.
    fn from_payload(payload: &Box<dyn Any + Send>) -> Self {
        let message = payload.downcast_ref::<&'static str>().map_or_else(
            || {
                payload.downcast_ref::<String>().map_or_else(
                    || "a panic payload that was not a string".to_owned(),
                    Clone::clone,
                )
            },
            |text| (*text).to_owned(),
        );
        Self { message }
    }
}

impl fmt::Display for Panicked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "the provider panicked: {}", self.message)
    }
}

impl std::error::Error for Panicked {}

/// A provider future whose panics are caught rather than allowed to unwind through the kernel.
///
/// `Unpin` without a projection, for the reason in the module documentation.
pub(crate) struct Contained<'a> {
    inner: ProviderFuture<'a>,
}

/// Wraps a provider future so that a panic becomes a value.
pub(crate) const fn contain(inner: ProviderFuture<'_>) -> Contained<'_> {
    Contained { inner }
}

impl Future for Contained<'_> {
    /// The inner future's own result, or the panic that replaced it.
    type Output = Result<Result<(), BoxedCause>, Panicked>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        // Safe because `Self` is `Unpin`: the only field is a `Pin<Box<_>>`, which is `Unpin`.
        let this = self.get_mut();

        // `AssertUnwindSafe` is sound in exactly this position: a panicking future is never polled
        // again, because the arm below returns `Ready` and the caller drops it.
        match catch_unwind(AssertUnwindSafe(|| this.inner.as_mut().poll(context))) {
            Ok(Poll::Ready(outcome)) => Poll::Ready(Ok(outcome)),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(Panicked::from_payload(&payload))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Panicked, contain};
    use crate::error::BoxedCause;
    use crate::provider::ProviderFuture;

    fn immediate_panic() -> ProviderFuture<'static> {
        Box::pin(async { panic!("scripted panic before any await") })
    }

    fn panic_after_await() -> ProviderFuture<'static> {
        Box::pin(async {
            tokio::task::yield_now().await;
            panic!("scripted panic after an await")
        })
    }

    fn formatted_panic() -> ProviderFuture<'static> {
        Box::pin(async {
            let reason = "a formatted reason";
            panic!("scripted panic with {reason}")
        })
    }

    fn succeeds() -> ProviderFuture<'static> {
        Box::pin(async { Ok(()) })
    }

    fn fails() -> ProviderFuture<'static> {
        Box::pin(async { Err::<(), BoxedCause>("an ordinary failure".into()) })
    }

    #[tokio::test]
    async fn a_panic_before_any_await_becomes_a_value() {
        let outcome = contain(immediate_panic()).await;
        let panicked = outcome.expect_err("the panic must be caught");
        assert!(
            panicked.message().contains("before any await"),
            "{panicked}"
        );
    }

    #[tokio::test]
    async fn a_panic_after_an_await_becomes_a_value() {
        // The case `std::panic::catch_unwind` around the call site cannot reach: the panic happens
        // on a later poll, long after the future was created.
        let outcome = contain(panic_after_await()).await;
        let panicked = outcome.expect_err("the panic must be caught");
        assert!(panicked.message().contains("after an await"), "{panicked}");
    }

    #[tokio::test]
    async fn a_formatted_panic_message_survives() {
        // `panic!` with a literal yields `&'static str`; with arguments it yields `String`. Both
        // payload types are read, because losing the message sends an author to a debugger for
        // something the panic already told them.
        let panicked = contain(formatted_panic())
            .await
            .expect_err("the panic must be caught");
        assert!(
            panicked.message().contains("a formatted reason"),
            "{panicked}"
        );
    }

    #[tokio::test]
    async fn a_successful_future_is_untouched() {
        // POSITIVE CONTROL: containment discriminates rather than turning everything into a panic.
        let outcome = contain(succeeds()).await;
        assert!(outcome.expect("no panic").is_ok());
    }

    #[tokio::test]
    async fn an_ordinary_failure_stays_an_ordinary_failure() {
        // The distinction that matters for attribution: a provider that *returns* an error must
        // not be reported as one that panicked.
        let outcome = contain(fails()).await;
        let inner = outcome.expect("no panic");
        let error = inner.expect_err("the provider returned an error");
        assert_eq!(error.to_string(), "an ordinary failure");
    }

    #[test]
    fn a_non_string_payload_is_reported_honestly() {
        let payload: Box<dyn core::any::Any + Send> = Box::new(42_u8);
        let panicked = Panicked::from_payload(&payload);
        assert!(panicked.message().contains("not a string"), "{panicked}");

        // POSITIVE CONTROL: a string payload is read rather than falling into the same arm.
        let text: Box<dyn core::any::Any + Send> = Box::new("a real message");
        assert_eq!(Panicked::from_payload(&text).message(), "a real message");
    }
}
