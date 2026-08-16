//! Hierarchical cancellation scopes.
//!
//! # What this adds over the underlying token
//!
//! `tokio_util::sync::CancellationToken` already provides propagation and child tokens. This
//! module adds the two things FR-023 and FR-024 actually need on top of that, and deliberately
//! nothing else:
//!
//! 1. **A named scope per provider**, so a cancellation diagnostic can say *which* provider was
//!    interrupted rather than reporting that "something" was cancelled.
//! 2. **A drop guard that cannot be forgotten.** FR-024 requires that cancellation arriving in any
//!    phase leaves **no provider half-initialised**. A provider scope that is dropped without
//!    being explicitly completed cancels itself, so an early return — including one caused by the
//!    `?` operator on an unrelated error — cannot leave a scope live and its work orphaned.
//!
//! The second is the load-bearing one. "Remember to cancel on every error path" is a rule; a drop
//! guard is a mechanism, and only the mechanism survives a future edit that adds a new early
//! return.
//!
//! # Why not re-export the token directly
//!
//! Because a bare token has no name, and FR-024's guarantee is about *providers*, not about
//! tokens. Handing authors a raw `CancellationToken` would make the naming optional, and anything
//! optional is absent in the diagnostic you actually need at 3am.

use core::fmt;
use std::sync::Arc;

use tokio_util::sync::CancellationToken;

/// The root cancellation scope for one application run.
///
/// Cancelling this cancels every scope derived from it, at any depth.
#[derive(Clone, Debug)]
pub struct CancelScope {
    token: CancellationToken,
    /// `Arc<str>` rather than `&'static str` because provider names are runtime values — an
    /// author's provider is named at registration, not at compile time — and rather than `String`
    /// because a scope is cloned once per child and per task that watches it.
    name: Arc<str>,
}

impl CancelScope {
    /// Creates the root scope.
    #[must_use]
    pub fn root() -> Self {
        Self {
            token: CancellationToken::new(),
            name: Arc::from("root"),
        }
    }

    /// Creates a named child scope.
    ///
    /// Cancelling the parent cancels this child; cancelling this child does **not** cancel the
    /// parent. That asymmetry is the whole point of a hierarchy — one provider failing to stop
    /// must not tear down its siblings' scopes.
    #[must_use]
    pub fn child(&self, name: impl Into<Arc<str>>) -> Self {
        Self {
            token: self.token.child_token(),
            name: name.into(),
        }
    }

    /// Creates a **guarded** scope for one provider's initialisation.
    ///
    /// The returned [`ProviderScope`] cancels itself on drop unless [`ProviderScope::complete`]
    /// is called first. See the module documentation for why this is a guard rather than a rule.
    #[must_use]
    pub fn provider(&self, provider: impl Into<Arc<str>>) -> ProviderScope {
        ProviderScope {
            scope: self.child(provider),
            completed: false,
        }
    }

    /// This scope's name, for diagnostics.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether cancellation has been signalled for this scope.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Signals cancellation for this scope and every scope derived from it.
    pub fn cancel(&self) {
        self.token.cancel();
    }

    /// Resolves when this scope is cancelled.
    ///
    /// Returns immediately if it is already cancelled, so a task that starts after cancellation
    /// does not wait forever for an event that has already happened.
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    /// The underlying token, for interoperating with libraries that accept one.
    ///
    /// Provided because refusing to expose it would push authors into keeping a parallel token of
    /// their own, which is worse than exposing this one.
    #[must_use]
    pub const fn token(&self) -> &CancellationToken {
        &self.token
    }
}

/// A provider's initialisation scope, which cancels itself if dropped without completing.
///
/// This is the mechanism behind FR-024. A provider that returns early — for any reason, including
/// a `?` on an error that has nothing to do with cancellation — drops this guard, and the drop
/// cancels the scope. Work started under it observes cancellation instead of continuing against a
/// provider that no longer exists.
#[derive(Debug)]
pub struct ProviderScope {
    scope: CancelScope,
    completed: bool,
}

impl ProviderScope {
    /// Marks initialisation as successfully finished, disarming the drop guard.
    ///
    /// Consumes `self` and returns the underlying scope, which remains live for the provider's
    /// running work. Forgetting to call this is safe by design: the scope cancels instead.
    #[must_use = "the returned scope is the provider's live cancellation scope; dropping it \
                  immediately would cancel the work that was just successfully started"]
    pub fn complete(mut self) -> CancelScope {
        self.completed = true;
        self.scope.clone()
    }

    /// The provider name this scope guards.
    #[must_use]
    pub fn provider(&self) -> &str {
        self.scope.name()
    }

    /// Whether this scope has been cancelled.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.scope.is_cancelled()
    }

    /// Borrows the scope for passing to work started during initialisation.
    #[must_use]
    pub const fn scope(&self) -> &CancelScope {
        &self.scope
    }
}

impl Drop for ProviderScope {
    fn drop(&mut self) {
        if !self.completed {
            // The provider never reached a successful initialisation. Anything it started is now
            // orphaned, so cancel rather than leaving it running against a half-built provider.
            self.scope.cancel();
        }
    }
}

impl fmt::Display for CancelScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::CancelScope;

    #[test]
    fn cancelling_a_parent_cancels_its_children() {
        let root = CancelScope::root();
        let child = root.child("http");
        let grandchild = child.child("listener");

        assert!(!grandchild.is_cancelled());
        root.cancel();
        assert!(child.is_cancelled(), "cancellation must propagate down");
        assert!(grandchild.is_cancelled(), "to any depth");
    }

    #[test]
    fn cancelling_a_child_does_not_cancel_its_parent_or_siblings() {
        // The asymmetry is the point: one provider failing must not tear down its siblings.
        let root = CancelScope::root();
        let first = root.child("db");
        let second = root.child("cache");

        first.cancel();
        assert!(first.is_cancelled());
        assert!(!root.is_cancelled(), "a child must not cancel its parent");
        assert!(!second.is_cancelled(), "nor its siblings");
    }

    #[test]
    fn a_provider_scope_dropped_without_completing_cancels_itself() {
        // FR-024. This is the case a "remember to cancel" rule loses: the early return here is
        // not about cancellation at all, and no cleanup code was written for it.
        let root = CancelScope::root();
        let observed = {
            let guard = root.provider("db");
            let watcher = guard.scope().clone();
            assert!(!watcher.is_cancelled(), "live during initialisation");
            // guard drops here without `complete()` — as it would on any early return
            watcher
        };
        assert!(
            observed.is_cancelled(),
            "an abandoned provider scope must cancel, or its work outlives the provider"
        );
    }

    #[test]
    fn a_completed_provider_scope_stays_live() {
        // POSITIVE CONTROL for the drop guard: if the guard cancelled unconditionally, the test
        // above would pass and this one would fail. Both together prove it discriminates.
        let root = CancelScope::root();
        let live = {
            let guard = root.provider("db");
            guard.complete()
        };
        assert!(
            !live.is_cancelled(),
            "a successfully initialised provider's scope must survive"
        );

        // And it is still attached to the hierarchy, not detached by completing.
        root.cancel();
        assert!(live.is_cancelled(), "completing must not orphan the scope");
    }

    #[test]
    fn a_scope_carries_its_name_for_diagnostics() {
        let root = CancelScope::root();
        assert_eq!(root.name(), "root");
        assert_eq!(root.child("db").name(), "db");
        assert_eq!(root.provider("cache").provider(), "cache");
        assert_eq!(root.child("db").to_string(), "db");
    }

    #[tokio::test]
    async fn awaiting_an_already_cancelled_scope_returns_immediately() {
        // A task that starts after cancellation must not wait for an event that already happened.
        let root = CancelScope::root();
        root.cancel();
        root.cancelled().await;
    }

    #[tokio::test]
    async fn awaiting_resolves_when_cancellation_arrives_later() {
        // POSITIVE CONTROL for the test above: proves `cancelled()` actually waits rather than
        // always returning immediately.
        let root = CancelScope::root();
        let child = root.child("worker");
        let waiter = tokio::spawn(async move { child.cancelled().await });

        assert!(!waiter.is_finished(), "must still be waiting");
        root.cancel();
        waiter.await.expect("the waiter resolves once cancelled");
    }
}
