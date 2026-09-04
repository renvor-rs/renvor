//! The cache capability: a narrow port, a deterministic substitute, and a Valkey adapter.
//!
//! # What this crate is
//!
//! [`Cache`] is four operations over bounded inputs. [`MemoryCache`] implements it in memory with
//! a real capacity and a pausable clock, for tests and for applications that want a per-process
//! cache with the same contract. [`valkey::ValkeyCache`] implements it over RESP against a Valkey
//! or Redis-compatible server, behind the off-by-default `valkey` feature, with rustls and the
//! platform trust store.
//!
//! # What it is not
//!
//! It is not a data-structure server, it never falls back from the server to memory, and a
//! failure is never reported as a miss. The reasons are in [`port`].
//!
//! # Startup
//!
//! Both implementations are kernel providers offering the capability
//! [`provider::CACHE_CAPABILITY`]. A service that depends on it and finds no provider fails at
//! Register; a Valkey provider whose server does not answer an authenticated `PING` fails at Boot
//! with a diagnostic that names the phase and the category and never the address or credential.
//!
//! # Configuration
//!
//! With `valkey`, [`config::CacheSection`] is the typed `[cache]` section: decoded by
//! `renvor-config`, defaulted, capped, and refused at **Validate** — naming the key, the
//! constraint, and the layer — before any provider boots (FR-011).
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** See the `renvor` facade documentation.

pub mod memory;
pub mod port;
pub mod provider;

/// The Valkey adapter. Behind the `valkey` feature: a build without it resolves no RESP client
/// and no TLS crate through this package, which `xtask` proves from the dependency graph.
#[cfg(feature = "valkey")]
pub mod valkey;

/// The typed `[cache]` configuration section (FR-011): defaults, hard caps, and a Validate-phase
/// refusal naming key, constraint, and layer. Behind `valkey` because the settings it produces
/// are the adapter's.
#[cfg(feature = "valkey")]
pub mod config;

pub use memory::{DEFAULT_CAPACITY, MemoryCache};
pub use port::{
    Cache, CacheBounds, CacheError, CacheKey, CacheMetrics, CacheValue, DEFAULT_MAX_TTL,
    DEFAULT_MAX_VALUE_BYTES, DEFAULT_OPERATION_TIMEOUT, Deleted, MAX_KEY_BYTES,
    MAX_NAMESPACE_BYTES, MAX_TTL_CAP, MAX_VALUE_BYTES_CAP, MIN_TTL, Namespace,
    OPERATION_TIMEOUT_CAP, Refusal, Stored, Ttl,
};
pub use provider::{
    BootPhase, CACHE_CAPABILITY, CacheBootError, MemoryCacheProvider, cache_capability,
};
