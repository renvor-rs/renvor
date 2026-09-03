//! The object-storage port with its in-memory substitute.
//!
//! ```sh
//! cargo run -p renvor --example storage --features capability-storage
//! ```
//!
//! The filesystem adapter is `renvor::storage::filesystem` behind `renvor-storage/filesystem`;
//! it implements the same `ObjectStore`, rooted in a directory capability.

use renvor::storage::{ContentType, Deleted, StorageBounds};
use renvor::{MemoryStore, ObjectKey, ObjectStore as _};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryStore::new(StorageBounds::new());
    let key = ObjectKey::new("users/42/avatar.png")?;
    store
        .put(
            &key,
            b"png-bytes".to_vec(),
            Some(ContentType::new("image/png")?),
        )
        .await?;
    let meta = store.head(&key).await?.expect("just stored");
    println!("head: {} bytes, {:?}", meta.size, meta.content_type);
    let object = store.get(&key).await?.expect("just stored");
    println!("get: {object:?}");
    println!("delete: {:?}", store.delete(&key).await?);
    assert_eq!(store.delete(&key).await?, Deleted::Absent);

    // A key that could traverse cannot exist.
    for bad in ["../etc/passwd", "a/./b", "a\\b", "CON"] {
        println!("{bad:?} refused: {}", ObjectKey::new(bad).is_err());
    }
    Ok(())
}
