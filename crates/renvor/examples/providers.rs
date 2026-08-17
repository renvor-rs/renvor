//! Dependency ordering and rollback, using ordinary language constructs only.
//!
//! ```text
//! cargo run --example providers
//! ```
//!
//! # The point of this example
//!
//! Providers are **registered** in one order and **initialised** in another, because dependencies
//! come first. When one fails, the ones already up are stopped in the reverse of the order they
//! actually started — **not** the reverse of registration. Those two orders differ here on purpose,
//! so the difference is visible in the output rather than only in a contract.
//!
//! ```text
//! registered   : http, cache, db
//! initialised  : db, http, cache      <- dependencies first
//! stopped      : http, db             <- reverse of what actually happened
//! ```
//!
//! FR-032 asks examples to use ordinary language constructs. There is no macro here, no derive, and
//! no attribute: a provider is a trait implementation.

use std::sync::{Arc, Mutex, PoisonError};

use renvor::{ApplicationBuilder, CapabilityId, InitContext, Provider, ProviderFuture, ProviderId};

/// Records what happened, in the order it happened.
#[derive(Clone, Default)]
struct Journal(Arc<Mutex<Vec<String>>>);

impl Journal {
    fn record(&self, entry: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(entry);
    }

    fn entries(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// A provider that announces itself and, optionally, refuses to start.
struct Announcing {
    id: ProviderId,
    provides: Vec<CapabilityId>,
    dependencies: Vec<CapabilityId>,
    fails: bool,
    journal: Journal,
}

impl Announcing {
    /// Returns a boxed provider rather than `Self`, because that is what the registry takes.
    fn declaring(
        journal: &Journal,
        name: &str,
        provides: &[&str],
        dependencies: &[&str],
        fails: bool,
    ) -> Box<dyn Provider> {
        Box::new(Self {
            id: ProviderId::new(name),
            provides: provides.iter().map(|c| CapabilityId::new(*c)).collect(),
            dependencies: dependencies.iter().map(|c| CapabilityId::new(*c)).collect(),
            fails,
            journal: journal.clone(),
        })
    }
}

impl Provider for Announcing {
    fn id(&self) -> &ProviderId {
        &self.id
    }

    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }

    fn dependencies(&self) -> &[CapabilityId] {
        &self.dependencies
    }

    fn initialise<'a>(&'a self, _context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            if self.fails {
                return Err("this provider cannot start".into());
            }
            self.journal.record(format!("init  {}", self.id));
            Ok(())
        })
    }

    fn stop(&self) -> ProviderFuture<'_> {
        Box::pin(async move {
            self.journal.record(format!("stop  {}", self.id));
            Ok(())
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── A run that succeeds, showing the reordering ──────────────────────────────────────────
    let journal = Journal::default();
    let application = ApplicationBuilder::new()
        // Registered first, but depends on `database` — so it cannot initialise first.
        .with_provider(Announcing::declaring(
            &journal,
            "http",
            &["http"],
            &["database"],
            false,
        ))
        .with_provider(Announcing::declaring(
            &journal,
            "cache",
            &["cache"],
            &[],
            false,
        ))
        .with_provider(Announcing::declaring(
            &journal,
            "db",
            &["database"],
            &[],
            false,
        ))
        .build()?
        .boot()
        .await?;

    println!("registered   : http, cache, db");
    print!("initialised  : ");
    println!(
        "{}",
        application
            .initialisation_order()
            .ids()
            .map(ProviderId::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("resolution   : {:?}", application.resolution_report());

    // ── A run that fails partway, showing rollback ────────────────────────────────────────────
    let journal = Journal::default();
    let failure = ApplicationBuilder::new()
        .with_provider(Announcing::declaring(
            &journal,
            "http",
            &["http"],
            &["database"],
            false,
        ))
        .with_provider(Announcing::declaring(
            &journal,
            "cache",
            &["cache"],
            &["http"],
            true,
        ))
        .with_provider(Announcing::declaring(
            &journal,
            "db",
            &["database"],
            &[],
            false,
        ))
        .build()?
        .boot()
        .await
        .expect_err("`cache` is scripted to fail");

    println!();
    println!("failure      : {}", failure.origin());
    for entry in journal.entries() {
        println!("             : {entry}");
    }
    println!(
        "rollback     : {} stopped, {} failure(s) while stopping",
        failure.rollback().stopped().len(),
        failure.rollback().failures().len()
    );

    Ok(())
}
