//! A redacting JSON subscriber, the health documents, and Prometheus text.
//!
//! ```sh
//! cargo run -p renvor --example observability --features observability
//! ```
//!
//! The subscriber is a value: this example installs it for the process through the kernel's one
//! install path. OTLP export is `renvor::observability::otel` behind `observability-otel`.

use std::sync::Arc;

use renvor::kernel::observe::metrics::Registry;
use renvor::kernel::observe::try_init_global;
use renvor::observability::health::{liveness_document, readiness_document};
use renvor::observability::prometheus;
use renvor::{HealthState, LogSettings, Readiness, ReadinessContributor};

#[derive(Debug)]
struct Database;

impl ReadinessContributor for Database {
    fn name(&self) -> &str {
        "database"
    }
    fn readiness(&self) -> Readiness {
        Readiness::Ready
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = LogSettings::new().with_filter("info", "observability.log.filter");
    let subscriber = renvor::observability::build(&settings, std::io::stdout)?;
    try_init_global(subscriber)?;

    // The `password` field is redacted on the way out; the record is one JSON object.
    let span = tracing::info_span!("example", run_id = "run-0123456789abcdef");
    let _entered = span.enter();
    tracing::info!(user = "ada", password = "not printed", "signed in");

    let health = HealthState::new();
    health.register(Arc::new(Database));
    println!("{}", liveness_document(&health));
    println!("{}", readiness_document(&health));

    let registry = Registry::new();
    let requests = registry.counter("example_requests_total", "Requests seen.", &["route"])?;
    requests.increment(&[("route", "/")], 1);
    print!("{}", prometheus::render(&registry.snapshot()));
    Ok(())
}
