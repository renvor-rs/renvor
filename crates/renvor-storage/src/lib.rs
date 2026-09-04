//! Object storage for Renvor: a narrow port whose keys cannot traverse and whose objects are
//! bounded, an in-memory substitute with a byte capacity, a filesystem adapter rooted in a
//! `cap-std` directory capability behind `filesystem`, and a provider that probes the backend at
//! Boot.
//!
//! **Pre-release. Nothing here is published and no API is stable.**
//!
//! # Where to start
//!
//! - [`port`] — [`ObjectStore`], [`ObjectKey`], [`ContentType`], the closed [`StorageError`].
//! - [`memory`] — [`MemoryStore`], the deterministic substitute.
//! - [`provider`] — [`StorageProvider`], which probes the backend at Boot.
//! - `filesystem` (feature) — `FilesystemStore` on `cap-std` with atomic writes.
//!
//! # Configuration
//!
//! With `filesystem`, [`config::StorageSection`] is the typed `[storage]` section: decoded by
//! `renvor-config`, defaulted, capped, and refused at **Validate** — naming the key, the
//! constraint, and the layer — before any provider boots (FR-011).
//!
//! # No object-storage service adapter ships in this phase
//!
//! ADR-0035 records the measurements: every S3-compatible candidate failed a licence, advisory,
//! or root-store gate. The port is what makes that a later adapter rather than a later port.

#![forbid(unsafe_code)]

pub mod memory;
pub mod port;
pub mod provider;

#[cfg(feature = "filesystem")]
pub mod filesystem;

/// The typed `[storage]` configuration section (FR-011): defaults, hard caps, and a
/// Validate-phase refusal naming key, constraint, and layer. Behind `filesystem` because the
/// settings it produces are the adapter's.
#[cfg(feature = "filesystem")]
pub mod config;

pub use memory::MemoryStore;
pub use port::{
    ContentType, DEFAULT_MAX_OBJECT_BYTES, Deleted, MAX_KEY_BYTES, MAX_OBJECT_BYTES_CAP, Object,
    ObjectKey, ObjectMeta, ObjectStore, StorageBounds, StorageError, StorageMetrics,
    StorageRefusal,
};
pub use provider::{STORAGE_CAPABILITY, StorageBootError, StorageProvider, storage_capability};
