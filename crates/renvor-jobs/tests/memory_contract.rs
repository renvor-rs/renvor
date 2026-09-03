//! The shared job-store contract, run against the memory substitute (FR-040).
//!
//! The same functions the four persistence rows call, so the substitute cannot promise anything
//! the rows do not, or fail to promise something they do.

use std::sync::Arc;

use renvor_core::observe::OsEntropy;
use renvor_jobs::{JobBounds, MemoryJobStore};
use renvor_testkit::jobs::{JobsFixture, the_shared_jobs_contract_holds};

struct MemoryFixture {
    store: Arc<MemoryJobStore>,
}

impl JobsFixture for MemoryFixture {
    type Store = MemoryJobStore;

    fn store(&self) -> Arc<Self::Store> {
        Arc::clone(&self.store)
    }

    fn bounds(&self) -> JobBounds {
        *self.store.bounds()
    }

    async fn reset(&self) {
        self.store.clear();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_shared_contract_holds_on_the_memory_substitute() {
    let bounds = JobBounds::new().with_max_queue_depth(3).unwrap();
    let fixture = MemoryFixture {
        store: Arc::new(MemoryJobStore::new(bounds, Arc::new(OsEntropy::new()))),
    };
    the_shared_jobs_contract_holds(&fixture).await;
}
