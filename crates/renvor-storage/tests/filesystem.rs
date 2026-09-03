//! The filesystem adapter against a real temporary directory: both traversal layers measured,
//! atomic writes, symbolic-link refusal, the read-side bound, and the Boot probe.

#![cfg(feature = "filesystem")]

use std::sync::Arc;
use std::time::Duration;

use renvor_core::provider::ProviderId;
use renvor_core::{ApplicationBuilder, Readiness};
use renvor_storage::filesystem::{FilesystemSettings, FilesystemStore};
use renvor_storage::provider::StorageProvider;
use renvor_storage::{
    ContentType, Deleted, ObjectKey, ObjectStore as _, StorageBounds, StorageError, StorageRefusal,
};

fn key(text: &str) -> ObjectKey {
    ObjectKey::new(text).unwrap()
}

fn store(root: &std::path::Path, bounds: StorageBounds) -> FilesystemStore {
    FilesystemStore::open(&FilesystemSettings::new(root, bounds)).unwrap()
}

#[tokio::test]
async fn round_trip_overwrite_nested_keys_and_delete() {
    let root = tempfile::tempdir().unwrap();
    let store = store(root.path(), StorageBounds::new());
    store.probe().await.unwrap();
    assert!(store.get(&key("missing")).await.unwrap().is_none());
    assert!(store.head(&key("missing")).await.unwrap().is_none());
    assert_eq!(
        store.delete(&key("missing")).await.unwrap(),
        Deleted::Absent
    );

    let ct = ContentType::new("image/png").unwrap();
    store
        .put(
            &key("users/42/avatar.png"),
            b"png-bytes".to_vec(),
            Some(ct.clone()),
        )
        .await
        .unwrap();
    let object = store
        .get(&key("users/42/avatar.png"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(object.bytes, b"png-bytes");
    assert_eq!(object.content_type, Some(ct.clone()));
    let meta = store
        .head(&key("users/42/avatar.png"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(meta.size, 9);
    assert_eq!(meta.content_type, Some(ct));
    assert!(root.path().join("objects/users/42/avatar.png").is_file());

    // Last writer wins; dropping the content type removes the sidecar.
    store
        .put(&key("users/42/avatar.png"), b"second".to_vec(), None)
        .await
        .unwrap();
    let object = store
        .get(&key("users/42/avatar.png"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(object.bytes, b"second");
    assert_eq!(object.content_type, None);
    assert!(!root.path().join("meta/users/42/avatar.png").exists());

    assert_eq!(
        store.delete(&key("users/42/avatar.png")).await.unwrap(),
        Deleted::Deleted
    );
    assert!(!root.path().join("objects/users/42/avatar.png").exists());
    assert_eq!(
        store.delete(&key("users/42/avatar.png")).await.unwrap(),
        Deleted::Absent
    );
}

#[tokio::test]
async fn a_write_leaves_no_temporary_file_behind() {
    let root = tempfile::tempdir().unwrap();
    let store = store(root.path(), StorageBounds::new());
    store.put(&key("a/b"), vec![7; 1024], None).await.unwrap();
    store.put(&key("a/c"), vec![8; 1024], None).await.unwrap();
    let names: Vec<String> = std::fs::read_dir(root.path().join("objects/a"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(sorted, ["b", "c"], "a temporary file survived the rename");
}

#[tokio::test]
async fn the_bound_holds_on_write_and_on_read() {
    let root = tempfile::tempdir().unwrap();
    let store = store(
        root.path(),
        StorageBounds::new().with_max_object_bytes(8).unwrap(),
    );
    assert_eq!(
        store.put(&key("big"), vec![0; 9], None).await.unwrap_err(),
        StorageError::Refused(StorageRefusal::ObjectTooLarge)
    );
    assert!(
        !root.path().join("objects/big").exists(),
        "a refused put wrote"
    );
    store.put(&key("ok"), vec![0; 8], None).await.unwrap();
    // Something else grows the file past the ceiling: the read refuses, the head still answers.
    std::fs::write(root.path().join("objects/ok"), vec![0; 9]).unwrap();
    assert_eq!(
        store.get(&key("ok")).await.unwrap_err(),
        StorageError::Refused(StorageRefusal::ObjectTooLarge)
    );
    assert_eq!(store.head(&key("ok")).await.unwrap().unwrap().size, 9);
}

/// T-3, both layers: the validator refuses a traversal key, AND the capability refuses the same
/// path handed to it directly, so a wrong validator would still not reach outside the root.
#[tokio::test]
async fn traversal_is_refused_by_the_validator_and_by_the_capability() {
    let outer = tempfile::tempdir().unwrap();
    std::fs::write(outer.path().join("secret.txt"), b"hunter2CanaryDoNotLeak").unwrap();
    let root = outer.path().join("root");
    std::fs::create_dir(&root).unwrap();

    // Layer 1: the validator.
    for (index, bad) in [
        "../secret.txt",
        "a/../../secret.txt",
        "/secret.txt",
        "..\\secret.txt",
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            ObjectKey::new(bad).is_err(),
            "traversal key case {index} passed the validator"
        );
    }

    // Layer 2: the capability, fed the traversal directly, without the validator.
    let dir = cap_std::fs::Dir::open_ambient_dir(&root, cap_std::ambient_authority()).unwrap();
    assert!(
        dir.read("../secret.txt").is_err(),
        "the capability followed `..` out of the root"
    );
    assert!(
        dir.read(outer.path().join("secret.txt")).is_err(),
        "the capability opened an absolute path"
    );
    // And a link planted inside the root that points outside is refused by the capability too.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(outer.path().join("secret.txt"), root.join("escape")).unwrap();
        assert!(
            dir.read("escape").is_err(),
            "the capability followed a link out of the root"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_symbolic_link_at_an_object_path_is_denied_even_inside_the_root() {
    let root = tempfile::tempdir().unwrap();
    let store = store(root.path(), StorageBounds::new());
    store
        .put(&key("real"), b"bytes".to_vec(), None)
        .await
        .unwrap();
    std::os::unix::fs::symlink("real", root.path().join("objects/alias")).unwrap();
    assert_eq!(
        store.get(&key("alias")).await.unwrap_err(),
        StorageError::Denied
    );
    assert_eq!(
        store.head(&key("alias")).await.unwrap_err(),
        StorageError::Denied
    );
    assert_eq!(
        store.delete(&key("alias")).await.unwrap_err(),
        StorageError::Denied
    );
    assert_eq!(
        store
            .put(&key("alias"), b"x".to_vec(), None)
            .await
            .unwrap_err(),
        StorageError::Denied
    );
    // The link's target was not touched by any of that.
    assert_eq!(
        store.get(&key("real")).await.unwrap().unwrap().bytes,
        b"bytes"
    );
}

#[tokio::test]
async fn a_missing_root_is_unavailable_and_the_provider_boots_a_present_one() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("hunter2CanaryDoNotLeak-missing");
    let error = FilesystemStore::open(&FilesystemSettings::new(&missing, StorageBounds::new()))
        .unwrap_err();
    assert_eq!(error, StorageError::Unavailable);
    assert!(
        !error.to_string().contains("hunter2"),
        "the path was rendered"
    );

    let store = Arc::new(store(root.path(), StorageBounds::new()));
    let provider = StorageProvider::new(ProviderId::new("storage"), store);
    let mut application = ApplicationBuilder::new()
        .with_provider(Box::new(provider))
        .build()
        .expect("registers")
        .boot()
        .await
        .expect("the probe succeeds on a writable root");
    let verdict = application
        .health()
        .readiness()
        .contributors
        .iter()
        .find(|verdict| verdict.name == "storage")
        .map(|verdict| verdict.readiness);
    assert_eq!(verdict, Some(Readiness::Ready));
    assert!(root.path().join("objects").is_dir() && root.path().join("meta").is_dir());
    application.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_read_only_root_fails_boot_as_denied_without_the_path() {
    use std::os::unix::fs::PermissionsExt as _;
    let root = tempfile::tempdir().unwrap();
    let locked = root.path().join("hunter2CanaryDoNotLeak-locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
    // A superuser ignores directory permissions; the property under test does not exist then.
    if std::fs::write(locked.join("writable-anyway"), b"x").is_ok() {
        eprintln!("skipping: this user can write to a read-only directory");
        return;
    }
    let store = Arc::new(store(&locked, StorageBounds::new()));
    let error = store.probe().await.unwrap_err();
    assert_eq!(error, StorageError::Denied);
    let category = renvor_storage::StorageBootError::from(error);
    assert_eq!(category, renvor_storage::StorageBootError::Denied);
    for rendered in [
        error.to_string(),
        category.to_string(),
        format!("{store:?}"),
    ] {
        assert!(!rendered.contains("hunter2"), "the path was rendered");
    }
    let outcome = ApplicationBuilder::new()
        .with_provider(Box::new(StorageProvider::new(
            ProviderId::new("storage"),
            store,
        )))
        .build()
        .expect("registers")
        .boot()
        .await;
    assert!(outcome.is_err(), "boot reached Ready on an unwritable root");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// The bound's floor is enforced at construction and a fast operation completes well inside it.
/// A genuinely slow filesystem is not arranged here; the `TimedOut` arm is exercised by the
/// provider's own boot bound and recorded as such in the evidence.
#[tokio::test]
async fn the_operation_bound_has_a_floor_and_a_fast_operation_completes_inside_it() {
    let root = tempfile::tempdir().unwrap();
    let settings = FilesystemSettings::new(root.path(), StorageBounds::new())
        .with_timeout(Duration::from_secs(1))
        .unwrap();
    let store = FilesystemStore::open(&settings).unwrap();
    let started = std::time::Instant::now();
    store.put(&key("fast"), vec![1; 64], None).await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(
        FilesystemSettings::new(root.path(), StorageBounds::new())
            .with_timeout(Duration::from_millis(999))
            .is_err()
    );
}
