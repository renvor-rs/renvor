//! The deterministic substitute: objects in a map, under a byte capacity (FR-058).
//!
//! The same bounds as the adapters, the same last-writer-wins `put`, and one difference an author
//! chooses visibly by constructing it: nothing here survives the process.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use crate::port::{
    ContentType, Deleted, Object, ObjectKey, ObjectMeta, ObjectStore, StorageBounds, StorageError,
};

/// The default total capacity: 256 MiB.
pub const DEFAULT_CAPACITY_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Default)]
struct Table {
    objects: HashMap<ObjectKey, (Vec<u8>, Option<ContentType>)>,
    used: u64,
}

/// An in-memory object store with a total byte capacity.
#[derive(Debug)]
pub struct MemoryStore {
    bounds: StorageBounds,
    capacity: u64,
    table: Mutex<Table>,
}

impl MemoryStore {
    /// An empty store with the default capacity.
    #[must_use]
    pub fn new(bounds: StorageBounds) -> Self {
        Self {
            bounds,
            capacity: DEFAULT_CAPACITY_BYTES,
            table: Mutex::new(Table::default()),
        }
    }

    /// Replaces the total capacity. A `put` that would exceed it is refused with
    /// [`StorageError::Capacity`]; nothing is evicted.
    #[must_use]
    pub const fn with_capacity(mut self, bytes: u64) -> Self {
        self.capacity = bytes;
        self
    }

    /// The bounds this store validates against.
    #[must_use]
    pub const fn bounds(&self) -> &StorageBounds {
        &self.bounds
    }

    /// Bytes currently held.
    #[must_use]
    pub fn used(&self) -> u64 {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .used
    }

    /// How many objects are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.table
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .objects
            .len()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Removes every object.
    pub fn clear(&self) {
        *self.table.lock().unwrap_or_else(PoisonError::into_inner) = Table::default();
    }
}

impl ObjectStore for MemoryStore {
    async fn put(
        &self,
        key: &ObjectKey,
        bytes: Vec<u8>,
        content_type: Option<ContentType>,
    ) -> Result<(), StorageError> {
        let incoming = bytes.len() as u64;
        self.bounds.check(incoming)?;
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        let existing = table
            .objects
            .get(key)
            .map_or(0, |(bytes, _)| bytes.len() as u64);
        let after = table.used.saturating_sub(existing).saturating_add(incoming);
        if after > self.capacity {
            return Err(StorageError::Capacity);
        }
        table.objects.insert(key.clone(), (bytes, content_type));
        table.used = after;
        Ok(())
    }

    async fn get(&self, key: &ObjectKey) -> Result<Option<Object>, StorageError> {
        let table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(table.objects.get(key).map(|(bytes, content_type)| Object {
            bytes: bytes.clone(),
            content_type: content_type.clone(),
        }))
    }

    async fn head(&self, key: &ObjectKey) -> Result<Option<ObjectMeta>, StorageError> {
        let table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(table
            .objects
            .get(key)
            .map(|(bytes, content_type)| ObjectMeta {
                size: bytes.len() as u64,
                content_type: content_type.clone(),
            }))
    }

    async fn delete(&self, key: &ObjectKey) -> Result<Deleted, StorageError> {
        let mut table = self.table.lock().unwrap_or_else(PoisonError::into_inner);
        match table.objects.remove(key) {
            Some((bytes, _)) => {
                table.used = table.used.saturating_sub(bytes.len() as u64);
                Ok(Deleted::Deleted)
            }
            None => Ok(Deleted::Absent),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryStore;
    use crate::port::{
        ContentType, Deleted, ObjectKey, ObjectStore as _, StorageBounds, StorageError,
        StorageRefusal,
    };

    fn key(text: &str) -> ObjectKey {
        ObjectKey::new(text).unwrap()
    }

    #[tokio::test]
    async fn round_trip_overwrite_and_delete() {
        let store = MemoryStore::new(StorageBounds::new());
        assert!(store.get(&key("a")).await.unwrap().is_none());
        assert_eq!(store.delete(&key("a")).await.unwrap(), Deleted::Absent);
        let ct = ContentType::new("text/plain").unwrap();
        store
            .put(&key("a"), b"one".to_vec(), Some(ct.clone()))
            .await
            .unwrap();
        let object = store.get(&key("a")).await.unwrap().unwrap();
        assert_eq!(object.bytes, b"one");
        assert_eq!(object.content_type, Some(ct.clone()));
        let meta = store.head(&key("a")).await.unwrap().unwrap();
        assert_eq!(meta.size, 3);
        assert_eq!(meta.content_type, Some(ct));
        // Last writer wins, and the content type is replaced with the bytes.
        store
            .put(&key("a"), b"second".to_vec(), None)
            .await
            .unwrap();
        let object = store.get(&key("a")).await.unwrap().unwrap();
        assert_eq!(object.bytes, b"second");
        assert_eq!(object.content_type, None);
        assert_eq!(store.used(), 6);
        assert_eq!(store.delete(&key("a")).await.unwrap(), Deleted::Deleted);
        assert_eq!(store.delete(&key("a")).await.unwrap(), Deleted::Absent);
        assert_eq!(store.used(), 0);
    }

    #[tokio::test]
    async fn the_object_bound_and_the_capacity_both_refuse() {
        let store = MemoryStore::new(StorageBounds::new().with_max_object_bytes(4).unwrap())
            .with_capacity(10);
        assert_eq!(
            store.put(&key("big"), vec![0; 5], None).await.unwrap_err(),
            StorageError::Refused(StorageRefusal::ObjectTooLarge)
        );
        store.put(&key("a"), vec![0; 4], None).await.unwrap();
        store.put(&key("b"), vec![0; 4], None).await.unwrap();
        // 8 used; 4 more would be 12 > 10.
        assert_eq!(
            store.put(&key("c"), vec![0; 4], None).await.unwrap_err(),
            StorageError::Capacity
        );
        assert_eq!(store.len(), 2, "nothing was evicted");
        // Overwriting counts the replaced bytes as freed.
        store.put(&key("a"), vec![0; 4], None).await.unwrap();
        assert_eq!(store.used(), 8);
        store.put(&key("c"), vec![0; 2], None).await.unwrap();
        assert_eq!(store.used(), 10);
    }
}
