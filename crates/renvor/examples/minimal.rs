//! The smallest working Renvor application.
//!
//! Run it with:
//!
//! ```text
//! cargo run --example minimal
//! ```
//!
//! # What SC-014 asks this example to demonstrate
//!
//! **0 global mutable state, 0 transports, 0 ports, 0 databases.** Everything the application owns
//! is reachable from the `Application` value, and when that value is dropped there is nothing left
//! behind — no `static`, no `lazy_static`, no process-global registry.
//!
//! That is why the kernel takes an entropy source and a phase log rather than reading a global
//! one, and why nothing here calls an `init()` that would have to be called exactly once.

use renvor::{ApplicationBuilder, LifecyclePhase};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let builder = ApplicationBuilder::new();

    // Taken before the run starts, so it can be read even if the run never produces an
    // application. This is the FR-002 inspection point, and it is ordinary public API.
    let phases = builder.phase_log();

    let mut application = builder.build()?.boot().await?;

    println!("run identifier : {}", application.run_id());
    println!("phase          : {}", application.phase());
    println!("phases entered : {:?}", phases.entries());
    println!("liveness       : {}", application.health().liveness());
    println!(
        "readiness      : {}",
        application.health().readiness().readiness
    );

    assert_eq!(application.phase(), LifecyclePhase::Ready);

    // Shutdown drains in-flight work under a budget, then stops providers in reverse order.
    let report = application.shutdown().await;
    println!("drain          : {}", report.drain());
    println!(
        "stopped        : {} provider(s)",
        report.stop().stopped().len()
    );

    Ok(())
}
