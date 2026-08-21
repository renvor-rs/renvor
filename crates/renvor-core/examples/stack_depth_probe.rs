//! Measures how much stack headroom the recursive SCC resolver actually has at the provider
//! ceiling.
//!
//! `tests/resolver_proof.rs` proves the 1024-node chain resolves on the pinned 2 MiB worker stack.
//! That answers "does it fit", not "by how much" — and a test that passes by a hair is one
//! compiler release away from failing, while one that passes by a mile is worth knowing about too.
//!
//! A stack overflow **aborts the process**; it cannot be caught and asserted on. So the search
//! runs one stack size per process invocation, and the caller walks the sizes:
//!
//! ```text
//! for kib in 2048 1024 512 256 128 64 32; do
//!   cargo run -q --example stack_depth_probe -- $kib && echo "$kib KiB ok" || echo "$kib KiB died"
//! done
//! ```
//!
//! Exit code 0 means the chain resolved. Any abort or non-zero exit means that stack was too
//! small. The resulting boundary is recorded in the Phase 002 research record §D8, which is a
//! working artifact retained in public Git history rather than the current tree:
//! <https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md>
//!
//! This deliberately uses only the public API, so the ceiling in
//! [`renvor_core::provider::graph::MAX_PROVIDERS`] still applies: the probe varies the **stack**,
//! never the graph, because a graph deeper than the ceiling is not a graph Renvor accepts.

use renvor_core::provider::graph::{MAX_PROVIDERS, ProviderIx, ResolverGraphBuilder};

fn main() {
    let kib: usize = std::env::args()
        .nth(1)
        .expect("usage: stack_depth_probe <worker-stack-kib>")
        .parse()
        .expect("worker stack size must be an integer number of KiB");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .thread_stack_size(kib * 1024)
        .thread_name("renvor-stack-depth-probe")
        .build()
        .expect("runtime builds");

    let order_len = runtime.block_on(async {
        tokio::spawn(async {
            let mut builder =
                ResolverGraphBuilder::with_capacity(MAX_PROVIDERS as usize, MAX_PROVIDERS as usize);
            for i in 0..MAX_PROVIDERS {
                if i + 1 < MAX_PROVIDERS {
                    builder.push_provider([ProviderIx::new(i + 1)]);
                } else {
                    builder.push_provider(std::iter::empty());
                }
            }
            let graph = builder.build().expect("the chain sits at the ceiling");
            graph.resolve().initialisation_order().len()
        })
        .await
        .expect("the resolution task neither panicked nor was cancelled")
    });

    // Printed, not just returned: a probe that exits 0 without doing the work would look
    // identical to one that succeeded.
    println!("{kib} KiB: resolved {order_len} providers");
    assert_eq!(order_len, MAX_PROVIDERS as usize);
}
