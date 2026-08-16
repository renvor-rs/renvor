//! T098 — proving `build()` installed **nothing** (FR-029, contract C-O7).
//!
//! # How you prove an absence
//!
//! You cannot ask `tracing` whether a global subscriber is installed. So the proof is indirect and
//! stronger than a query would be: **install one afterwards, and succeed**. The global slot can be
//! claimed once per process, so a successful claim after a full application lifecycle is proof
//! that the lifecycle did not claim it.
//!
//! # One test, on purpose
//!
//! The global subscriber is process-wide, so two tests racing for the single slot would make the
//! loser report a failure that is really a scheduling accident. Everything that needs the slot
//! happens in one test, in order.

use renvor_core::observe::{AlreadyInstalled, try_init_global};
use renvor_core::{ApplicationBuilder, LifecyclePhase};

#[tokio::test(start_paused = true)]
async fn a_full_lifecycle_leaves_the_global_subscriber_unclaimed() {
    // A complete run: build, boot, and shut down. If any of it installed a subscriber, the claim
    // below would fail.
    let mut application = ApplicationBuilder::new()
        .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
            17;
            32
        ])))
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    assert_eq!(application.phase(), LifecyclePhase::Ready);
    application.shutdown().await;
    assert_eq!(application.phase(), LifecyclePhase::Stop);

    // The proof. A whole application ran and the slot is still free.
    try_init_global(tracing::subscriber::NoSubscriber::default())
        .expect("the kernel installed nothing, so this claim must succeed");

    // POSITIVE CONTROL: the slot really is single-claim, so the success above means "was free"
    // rather than "this call always succeeds". Without this, a `try_init_global` that ignored its
    // argument and returned `Ok` would pass the assertion above.
    assert_eq!(
        try_init_global(tracing::subscriber::NoSubscriber::default()),
        Err(AlreadyInstalled),
        "the second claim must be refused, or the first proved nothing"
    );

    // And a further application still runs with a subscriber someone else installed — the kernel
    // neither needs the slot nor minds who has it.
    let second = ApplicationBuilder::new()
        .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
            19;
            32
        ])))
        .build()
        .expect("assembles")
        .boot()
        .await;
    assert!(
        second.is_ok(),
        "the kernel runs under someone else's subscriber"
    );
}
