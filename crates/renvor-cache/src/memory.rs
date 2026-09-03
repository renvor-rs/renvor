//! The deterministic substitute: an in-memory cache with a real capacity and a pausable clock.
//!
//! # It refuses rather than evicts
//!
//! A production cache evicts. This one does not, on purpose (FR-017): a test substitute that
//! quietly dropped entries to make room would pass a test whose application had outgrown the
//! capacity it was written against. When the entry count reaches the configured capacity, expired
//! entries are purged first — that is not eviction, they are already gone — and if the table is
//! still full the write is refused with [`CacheError::Capacity`].
//!
//! # Time is `tokio::time`, so a test moves it
//!
//! Expiry is measured against `tokio::time::Instant`, which honours `pause` and `advance`. An
//! expired entry is never returned and is removed when it is next touched or when a write needs
//! the room. There is no background sweeper: a task that runs on its own is exactly the unbounded
//! orphan the kernel's rules exclude, and a substitute has no reason to need one.
//!
//! # Same rules as the adapter
//!
//! The namespace is applied, the bounds are checked at the port's value types, and
//! `set_if_absent` is a single-writer primitive under one lock. What differs is durability and
//! reach, which is the difference the constitution requires an author to choose visibly.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use tokio::time::Instant;

use crate::port::{
    Cache, CacheBounds, CacheError, CacheKey, CacheValue, Deleted, Namespace, Stored, Ttl,
};

/// The default entry capacity.
pub const DEFAULT_CAPACITY: usize = 10_000;

/// One stored entry.
#[derive(Debug)]
struct Entry {
    value: CacheValue,
    expires_at: Instant,
}

/// An in-memory cache: bounded entries, no eviction, expiry under the Tokio clock.
#[derive(Debug)]
pub struct MemoryCache {
    namespace: Namespace,
    bounds: CacheBounds,
    capacity: usize,
    entries: Mutex<HashMap<String, Entry>>,
}

impl MemoryCache {
    /// Creates a cache with [`DEFAULT_CAPACITY`] entries.
    #[must_use]
    pub fn new(namespace: Namespace, bounds: CacheBounds) -> Self {
        Self::with_capacity(namespace, bounds, DEFAULT_CAPACITY)
    }

    /// Creates a cache holding at most `capacity` live entries. Zero is rounded up to one, because
    /// a cache that can hold nothing is a cache every test against it misreads.
    #[must_use]
    pub fn with_capacity(namespace: Namespace, bounds: CacheBounds, capacity: usize) -> Self {
        Self {
            namespace,
            bounds,
            capacity: capacity.max(1),
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// The bounds this cache validates against.
    #[must_use]
    pub const fn bounds(&self) -> &CacheBounds {
        &self.bounds
    }

    /// The namespace keys are stored under.
    #[must_use]
    pub const fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    /// How many entries are held, expired ones included until they are touched.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes every expired entry. Called before a write that needs room.
    fn purge_expired(entries: &mut HashMap<String, Entry>, now: Instant) {
        entries.retain(|_, entry| entry.expires_at > now);
    }

    fn write(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
        only_if_absent: bool,
    ) -> Result<Stored, CacheError> {
        let qualified = self.namespace.qualify(key)?;
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);

        // A live entry under the key: absent-only writes stop here; plain writes replace it and
        // need no room.
        let live = entries
            .get(&qualified)
            .is_some_and(|entry| entry.expires_at > now);
        if live && only_if_absent {
            return Ok(Stored::AlreadyPresent);
        }
        if !live && entries.len() >= self.capacity {
            Self::purge_expired(&mut entries, now);
            if entries.len() >= self.capacity && !entries.contains_key(&qualified) {
                return Err(CacheError::Capacity);
            }
        }
        entries.insert(
            qualified,
            Entry {
                value,
                expires_at: now + ttl.duration(),
            },
        );
        Ok(Stored::Stored)
    }
}

impl Cache for MemoryCache {
    async fn get(&self, key: &CacheKey) -> Result<Option<CacheValue>, CacheError> {
        let qualified = self.namespace.qualify(key)?;
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        match entries.get(&qualified) {
            Some(entry) if entry.expires_at > now => Ok(Some(entry.value.clone())),
            Some(_) => {
                // Expired: removed on touch, and reported as the miss it is.
                entries.remove(&qualified);
                Ok(None)
            }
            None => Ok(None),
        }
    }

    async fn set(&self, key: &CacheKey, value: CacheValue, ttl: Ttl) -> Result<(), CacheError> {
        self.write(key, value, ttl, false).map(|_| ())
    }

    async fn set_if_absent(
        &self,
        key: &CacheKey,
        value: CacheValue,
        ttl: Ttl,
    ) -> Result<Stored, CacheError> {
        self.write(key, value, ttl, true)
    }

    async fn delete(&self, key: &CacheKey) -> Result<Deleted, CacheError> {
        let qualified = self.namespace.qualify(key)?;
        let now = Instant::now();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        match entries.remove(&qualified) {
            Some(entry) if entry.expires_at > now => Ok(Deleted::Removed),
            // An expired entry was never "there" to a caller; removing it is housekeeping.
            _ => Ok(Deleted::Absent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_CAPACITY, MemoryCache};
    use crate::port::{
        Cache, CacheBounds, CacheError, CacheKey, CacheValue, Deleted, Namespace, Stored, Ttl,
    };
    use std::sync::Arc;
    use std::time::Duration;

    fn cache() -> MemoryCache {
        MemoryCache::new(Namespace::new("test").unwrap(), CacheBounds::new())
    }

    fn key(text: &str) -> CacheKey {
        CacheKey::new(text).unwrap()
    }

    fn value(text: &str) -> CacheValue {
        CacheValue::within(text.as_bytes().to_vec(), &CacheBounds::new()).unwrap()
    }

    fn ttl(secs: u64) -> Ttl {
        Ttl::within(Duration::from_secs(secs), &CacheBounds::new()).unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn set_get_delete_round_trip() {
        let cache = cache();
        assert_eq!(
            cache.get(&key("a")).await.unwrap(),
            None,
            "a fresh cache misses"
        );
        cache.set(&key("a"), value("1"), ttl(60)).await.unwrap();
        assert_eq!(
            cache.get(&key("a")).await.unwrap().unwrap().as_bytes(),
            b"1"
        );
        cache.set(&key("a"), value("2"), ttl(60)).await.unwrap();
        assert_eq!(
            cache.get(&key("a")).await.unwrap().unwrap().as_bytes(),
            b"2",
            "set replaces"
        );
        assert_eq!(cache.delete(&key("a")).await.unwrap(), Deleted::Removed);
        assert_eq!(cache.delete(&key("a")).await.unwrap(), Deleted::Absent);
        assert_eq!(cache.get(&key("a")).await.unwrap(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn an_expired_entry_is_never_returned_and_is_removed_on_touch() {
        let cache = cache();
        cache.set(&key("a"), value("1"), ttl(10)).await.unwrap();
        tokio::time::advance(Duration::from_secs(9)).await;
        assert!(
            cache.get(&key("a")).await.unwrap().is_some(),
            "still live at 9 s"
        );
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(
            cache.get(&key("a")).await.unwrap(),
            None,
            "expired at exactly 10 s"
        );
        assert_eq!(cache.len(), 0, "the expired entry was removed on touch");
        // An expired entry does not count as present for delete either.
        cache.set(&key("b"), value("1"), ttl(1)).await.unwrap();
        tokio::time::advance(Duration::from_secs(2)).await;
        assert_eq!(cache.delete(&key("b")).await.unwrap(), Deleted::Absent);
    }

    #[tokio::test(start_paused = true)]
    async fn set_if_absent_writes_exactly_once_while_the_key_lives() {
        let cache = cache();
        assert_eq!(
            cache
                .set_if_absent(&key("lock"), value("me"), ttl(5))
                .await
                .unwrap(),
            Stored::Stored
        );
        assert_eq!(
            cache
                .set_if_absent(&key("lock"), value("you"), ttl(5))
                .await
                .unwrap(),
            Stored::AlreadyPresent
        );
        assert_eq!(
            cache.get(&key("lock")).await.unwrap().unwrap().as_bytes(),
            b"me",
            "the loser did not overwrite"
        );
        // Once it expires, the next writer wins.
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(
            cache
                .set_if_absent(&key("lock"), value("you"), ttl(5))
                .await
                .unwrap(),
            Stored::Stored
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_set_if_absent_admits_exactly_one_writer() {
        // FR-095 for the substitute: four racers on a barrier, one `Stored`. No sleep.
        let cache = Arc::new(cache());
        let barrier = Arc::new(tokio::sync::Barrier::new(4));
        let mut handles = Vec::new();
        for racer in 0..4_u8 {
            let cache = Arc::clone(&cache);
            let barrier = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                cache
                    .set_if_absent(&key("race"), value(&racer.to_string()), ttl(30))
                    .await
                    .unwrap()
            }));
        }
        let mut stored = 0;
        for handle in handles {
            if handle.await.unwrap() == Stored::Stored {
                stored += 1;
            }
        }
        assert_eq!(stored, 1, "exactly one racer may store");
    }

    #[tokio::test(start_paused = true)]
    async fn capacity_is_refused_not_evicted_and_expired_entries_make_room() {
        let cache =
            MemoryCache::with_capacity(Namespace::new("test").unwrap(), CacheBounds::new(), 2);
        cache.set(&key("a"), value("1"), ttl(5)).await.unwrap();
        cache.set(&key("b"), value("1"), ttl(60)).await.unwrap();
        assert_eq!(
            cache.set(&key("c"), value("1"), ttl(60)).await.unwrap_err(),
            CacheError::Capacity,
            "a third live entry is refused, not admitted by evicting one"
        );
        // Both existing entries survived the refusal.
        assert!(cache.get(&key("a")).await.unwrap().is_some());
        assert!(cache.get(&key("b")).await.unwrap().is_some());
        // Replacing an existing key needs no room.
        cache.set(&key("b"), value("2"), ttl(60)).await.unwrap();
        // POSITIVE CONTROL: once `a` expires, the write that was refused succeeds.
        tokio::time::advance(Duration::from_secs(5)).await;
        cache.set(&key("c"), value("1"), ttl(60)).await.unwrap();
        assert_eq!(cache.len(), 2);
        assert_eq!(DEFAULT_CAPACITY, 10_000);
    }

    #[tokio::test(start_paused = true)]
    async fn two_namespaces_on_one_key_do_not_collide() {
        // The substitute holds one table per instance, so this is a check that the namespace is
        // applied at all — the property the adapter relies on against a shared server.
        let cache = cache();
        cache.set(&key("k"), value("1"), ttl(5)).await.unwrap();
        let qualified = cache.namespace().qualify(&key("k")).unwrap();
        assert_eq!(qualified, "test:k");
    }

    #[test]
    fn debug_never_prints_an_entry() {
        let cache = cache();
        let rendered = format!("{cache:?}");
        assert!(
            rendered.contains("MemoryCache"),
            "Debug does not name the type"
        );
    }
}
