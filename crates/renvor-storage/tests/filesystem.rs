//! The filesystem adapter against a real temporary directory: both traversal layers measured,
//! atomic writes that carry the bytes and the content type as one unit (C-C5), symbolic-link
//! refusal, the read-side bound measured against the body rather than the file, corrupt object
//! files reported closed, and the Boot probe.

#![cfg(feature = "filesystem")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use renvor_core::provider::ProviderId;
use renvor_core::{ApplicationBuilder, Readiness};
use renvor_storage::filesystem::{FilesystemSettings, FilesystemStore};
use renvor_storage::port::STORAGE_EVENT_TARGET;
use renvor_storage::provider::StorageProvider;
use renvor_storage::{
    ContentType, Deleted, ObjectKey, ObjectStore as _, StorageBounds, StorageError, StorageRefusal,
};
use tokio::sync::Barrier;

fn key(text: &str) -> ObjectKey {
    ObjectKey::new(text).unwrap()
}

fn store(root: &std::path::Path, bounds: StorageBounds) -> FilesystemStore {
    FilesystemStore::open(&FilesystemSettings::new(root, bounds)).unwrap()
}

/// One object file as the adapter lays it out: `RVO1`, the big-endian `u16` content-type length
/// (0 when there is none), the content type, then the bytes. Built here independently of the
/// adapter so the tests pin the format the module documentation states, not whatever the adapter
/// happens to write.
fn framed(content_type: Option<&str>, body: &[u8]) -> Vec<u8> {
    let content_type = content_type.unwrap_or("").as_bytes();
    let mut file = b"RVO1".to_vec();
    file.extend_from_slice(&u16::try_from(content_type.len()).unwrap().to_be_bytes());
    file.extend_from_slice(content_type);
    file.extend_from_slice(body);
    file
}

/// The fields of one recorded event, rendered.
type Record = Vec<(String, String)>;

/// What a racing reader saw in one `get`: the first byte, whether every byte matched it, and the
/// content type.
type Observation = (u8, bool, Option<ContentType>);

/// Records every `ERROR` event on the storage target and nothing else.
#[derive(Clone, Default)]
struct Recorder {
    errors: Arc<Mutex<Vec<Record>>>,
}

#[derive(Default)]
struct Collector(Record);

impl tracing::field::Visit for Collector {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn core::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
}

impl tracing::Subscriber for Recorder {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        *metadata.level() == tracing::Level::ERROR && metadata.target() == STORAGE_EVENT_TARGET
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        let mut collector = Collector::default();
        event.record(&mut collector);
        self.errors
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(collector.0);
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

/// The one global recorder this binary installs. Global rather than thread-local because
/// `tracing` caches a callsite's interest against the dispatcher of whichever thread registers
/// it first: a thread-local subscriber in one test silently loses events to a neighbouring test
/// that reached the callsite earlier. The library still installs nothing (C-O7).
fn recorder() -> &'static Recorder {
    static RECORDER: OnceLock<Recorder> = OnceLock::new();
    RECORDER.get_or_init(|| {
        let recorder = Recorder::default();
        tracing::subscriber::set_global_default(recorder.clone())
            .expect("another global subscriber is already installed in this test binary");
        recorder
    })
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

    // Last writer wins; the content type travels with the bytes in the one object file, so
    // dropping it leaves nothing beside the object and no sidecar tree anywhere.
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
    assert!(!root.path().join("meta").exists(), "a sidecar tree exists");

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
    // Something else grows the body past the ceiling (in a well-formed file; a malformed one is
    // `a_corrupt_object_is_reported_closed`): the read refuses, the head still answers.
    std::fs::write(root.path().join("objects/ok"), framed(None, &[0; 9])).unwrap();
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
    assert!(root.path().join("objects").is_dir());
    assert!(
        !root.path().join("meta").exists(),
        "the probe created a sidecar tree"
    );
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

/// C-C5: `put` is last-writer-wins, **whole, never interleaved** — the bytes and the content
/// type together. Two writers race on one key while two readers watch. A reader must never pair
/// one writer's bytes with the other writer's content type, nor see a torn body; the former
/// sidecar layout allowed the first because the sidecar and the object were two renames.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_writers_never_interleave_bytes_and_content_type() {
    const ROUNDS: usize = 400;
    const PAYLOAD: usize = 64 * 1024;
    let root = tempfile::tempdir().unwrap();
    let store = Arc::new(store(root.path(), StorageBounds::new()));
    let raced = key("raced/object");
    let a_side = ContentType::new("text/a-side").unwrap();
    let b_side = ContentType::new("text/b-side").unwrap();
    store
        .put(&raced, vec![b'A'; PAYLOAD], Some(a_side.clone()))
        .await
        .unwrap();

    let barrier = Arc::new(Barrier::new(4));
    let done = Arc::new(AtomicBool::new(false));
    let gets: Arc<Mutex<Vec<Observation>>> = Arc::default();
    let heads: Arc<Mutex<Vec<Option<ContentType>>>> = Arc::default();

    let writer = |byte: u8, content_type: ContentType| {
        let store = Arc::clone(&store);
        let raced = raced.clone();
        let barrier = Arc::clone(&barrier);
        tokio::spawn(async move {
            barrier.wait().await;
            for _ in 0..ROUNDS {
                store
                    .put(&raced, vec![byte; PAYLOAD], Some(content_type.clone()))
                    .await
                    .expect("a racing put failed");
            }
        })
    };
    let reader = || {
        let store = Arc::clone(&store);
        let raced = raced.clone();
        let barrier = Arc::clone(&barrier);
        let done = Arc::clone(&done);
        let gets = Arc::clone(&gets);
        let heads = Arc::clone(&heads);
        tokio::spawn(async move {
            barrier.wait().await;
            while !done.load(Ordering::Acquire) {
                let object = store
                    .get(&raced)
                    .await
                    .expect("a racing get failed")
                    .expect("the object vanished during the race");
                let first = object.bytes[0];
                let uniform = object.bytes.iter().all(|byte| *byte == first);
                gets.lock().unwrap_or_else(PoisonError::into_inner).push((
                    first,
                    uniform,
                    object.content_type,
                ));
                let meta = store
                    .head(&raced)
                    .await
                    .expect("a racing head failed")
                    .expect("the object vanished during the race");
                heads
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(meta.content_type);
            }
        })
    };

    let (writer_a, writer_b) = (writer(b'A', a_side.clone()), writer(b'B', b_side.clone()));
    let (reader_one, reader_two) = (reader(), reader());
    writer_a.await.unwrap();
    writer_b.await.unwrap();
    done.store(true, Ordering::Release);
    reader_one.await.unwrap();
    reader_two.await.unwrap();

    let gets = gets.lock().unwrap_or_else(PoisonError::into_inner);
    let heads = heads.lock().unwrap_or_else(PoisonError::into_inner);
    let mixed = gets
        .iter()
        .filter(|(first, uniform, content_type)| {
            let a = *first == b'A' && content_type.as_ref() == Some(&a_side);
            let b = *first == b'B' && content_type.as_ref() == Some(&b_side);
            !(*uniform && (a || b))
        })
        .count();
    let foreign = heads
        .iter()
        .filter(|content_type| {
            content_type.as_ref() != Some(&a_side) && content_type.as_ref() != Some(&b_side)
        })
        .count();
    eprintln!(
        "race: {} get observations ({mixed} inconsistent), {} head observations ({foreign} \
         foreign)",
        gets.len(),
        heads.len()
    );
    // POSITIVE CONTROL: the readers observed the race rather than finishing before it started.
    let count = gets.len();
    assert!(
        count >= 2,
        "the readers made too few observations to prove anything: {count}"
    );
    let count = mixed;
    assert_eq!(
        count, 0,
        "a get paired one writer's bytes with the other writer's content type, or saw a torn \
         body, in {count} observations"
    );
    let count = foreign;
    assert_eq!(
        count, 0,
        "a head reported a content type neither writer stored, in {count} observations"
    );
}

/// `head` answers from the header alone: the size is the body's length, not the file's, and the
/// content type is the one in the header, present or absent. The on-disk shape is pinned here
/// too — one file per object, header then body, no sidecar tree — because the format is what
/// makes one rename carry both.
#[tokio::test]
async fn head_reads_only_the_header() {
    let root = tempfile::tempdir().unwrap();
    let store = store(root.path(), StorageBounds::new());
    let body = vec![0x5a; 1024 * 1024];
    let plain = ContentType::new("text/plain; charset=utf-8").unwrap();
    store
        .put(&key("big"), body.clone(), Some(plain.clone()))
        .await
        .unwrap();
    let meta = store.head(&key("big")).await.unwrap().unwrap();
    assert_eq!(
        meta.size,
        body.len() as u64,
        "head counted the header in the size"
    );
    assert_eq!(meta.content_type, Some(plain.clone()));
    let on_disk = std::fs::read(root.path().join("objects/big")).unwrap();
    assert!(
        on_disk == framed(Some(plain.as_str()), &body),
        "the object file is not header-then-body"
    );
    assert!(!root.path().join("meta").exists(), "a sidecar tree exists");

    store.put(&key("bare"), body.clone(), None).await.unwrap();
    let meta = store.head(&key("bare")).await.unwrap().unwrap();
    assert_eq!(meta.size, body.len() as u64);
    assert_eq!(meta.content_type, None);
    let on_disk = std::fs::read(root.path().join("objects/bare")).unwrap();
    assert!(
        on_disk == framed(None, &body),
        "an object without a content type is not header-then-body"
    );
}

/// An object file that does not decode — bad magic, a truncated header, a content-type length
/// past the 255-byte bound (C-C2), a content type that fails the grammar, or a length past the
/// end of the file — is `Unavailable`, closed, with one `ERROR` event carrying a closed reason
/// and never the key or the path (FR-063). Never a panic, never the garbage.
#[tokio::test]
async fn a_corrupt_object_is_reported_closed() {
    let recorder = recorder();
    let root = tempfile::tempdir().unwrap();
    let store = store(root.path(), StorageBounds::new());
    let canary = key("hunter2CanaryDoNotLeak/object");
    let directory = root.path().join("objects/hunter2CanaryDoNotLeak");
    std::fs::create_dir_all(&directory).unwrap();
    let too_long = format!("text/{}", "p".repeat(300));
    let cases: [Vec<u8>; 8] = [
        b"not an object file at all".to_vec(),
        // A bad magic whose length field happens to be valid: only the magic check refuses it.
        b"XXXX\x00\x00bytes".to_vec(),
        b"RVO2\x00\x00bytes".to_vec(),
        b"RVO1\x00".to_vec(),
        Vec::new(),
        framed(Some(&too_long), b"body"),
        framed(Some("no-slash"), b"body"),
        {
            let mut short = b"RVO1\x00\x10".to_vec();
            short.extend_from_slice(b"text/");
            short
        },
    ];

    let before = recorder
        .errors
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len();
    for (index, garbage) in cases.iter().enumerate() {
        std::fs::write(directory.join("object"), garbage).unwrap();
        assert_eq!(
            store.get(&canary).await,
            Err(StorageError::Unavailable),
            "get of corrupt case {index} was not reported as unavailable"
        );
        assert_eq!(
            store.head(&canary).await,
            Err(StorageError::Unavailable),
            "head of corrupt case {index} was not reported as unavailable"
        );
    }

    // Copied out so no guard is held across the `delete` below.
    let recent: Vec<Record> = recorder
        .errors
        .lock()
        .unwrap_or_else(PoisonError::into_inner)[before..]
        .to_vec();
    let count = recent.len();
    let expected = cases.len() * 2;
    assert_eq!(
        count, expected,
        "expected {expected} error events, one per corrupt read; got {count}"
    );
    let temporary_name = root
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    for (index, fields) in recent.iter().enumerate() {
        let rendered = fields
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !rendered.contains("hunter2"),
            "corrupt event {index} carried the key"
        );
        assert!(
            !rendered.contains(&temporary_name),
            "corrupt event {index} carried the root path"
        );
        assert!(
            fields
                .iter()
                .any(|(name, value)| name == "reason" && !value.is_empty()),
            "corrupt event {index} carries no closed reason"
        );
    }

    // Whatever the file holds, `delete` removes it: corruption is not a trap.
    assert_eq!(store.delete(&canary).await.unwrap(), Deleted::Deleted);
    assert!(!directory.join("object").exists());
}

/// The bound on read is a bound on the OBJECT: the header is the adapter's, not the caller's, so
/// a body exactly at the ceiling reads back with its content type, and a body grown past the
/// ceiling by something else is still refused while `head` reports the body's size.
#[tokio::test]
async fn the_read_bound_is_checked_against_the_body_not_the_file() {
    let root = tempfile::tempdir().unwrap();
    let store = store(
        root.path(),
        StorageBounds::new().with_max_object_bytes(8).unwrap(),
    );
    let octets = ContentType::new("application/octet-stream").unwrap();
    store
        .put(&key("edge"), vec![1; 8], Some(octets.clone()))
        .await
        .unwrap();
    let on_disk = std::fs::metadata(root.path().join("objects/edge"))
        .unwrap()
        .len();
    assert!(
        on_disk > 8,
        "the file is no larger than the body: the header is missing"
    );
    let object = store.get(&key("edge")).await.unwrap().unwrap();
    assert_eq!(object.bytes, vec![1; 8]);
    assert_eq!(object.content_type, Some(octets.clone()));
    assert_eq!(store.head(&key("edge")).await.unwrap().unwrap().size, 8);

    std::fs::write(
        root.path().join("objects/edge"),
        framed(Some(octets.as_str()), &[1; 9]),
    )
    .unwrap();
    assert_eq!(
        store.get(&key("edge")).await,
        Err(StorageError::Refused(StorageRefusal::ObjectTooLarge))
    );
    let meta = store.head(&key("edge")).await.unwrap().unwrap();
    assert_eq!(meta.size, 9);
    assert_eq!(meta.content_type, Some(octets));
}
