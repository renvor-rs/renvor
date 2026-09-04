//! The cache port with its deterministic substitute.
//!
//! ```sh
//! cargo run -p renvor --example cache --features capability-cache
//! ```
//!
//! The Valkey adapter is `renvor::cache::valkey` behind `renvor-cache/valkey`; it implements the
//! same `Cache` trait, so the code below changes only where the cache is constructed.

use std::time::Duration;

use renvor::cache::{CacheBounds, Namespace, Stored};
use renvor::{Cache as _, CacheKey, CacheValue, MemoryCache, Ttl};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bounds = CacheBounds::new();
    let cache = MemoryCache::new(Namespace::new("example")?, bounds);
    let key = CacheKey::new("greeting")?;
    let ttl = Ttl::within(Duration::from_secs(60), &bounds)?;

    cache
        .set(&key, CacheValue::within(b"hello".to_vec(), &bounds)?, ttl)
        .await?;
    let value = cache.get(&key).await?.expect("just set");
    println!("get: {} bytes", value.as_bytes().len());

    // `set_if_absent` is the single-writer primitive: exactly one caller stores.
    let outcome = cache
        .set_if_absent(&key, CacheValue::within(b"second".to_vec(), &bounds)?, ttl)
        .await?;
    println!("set_if_absent while present: {outcome:?}");
    assert_eq!(outcome, Stored::AlreadyPresent);

    // A key or value over a bound is refused before any backend is touched.
    let too_long = "k".repeat(1024);
    println!(
        "over-bound key refused: {}",
        CacheKey::new(&too_long).is_err()
    );
    Ok(())
}
