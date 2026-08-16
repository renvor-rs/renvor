//! Typed, layered configuration and secret redaction for Renvor.
//!
//! Configuration arrives from exactly three source kinds — built-in defaults, TOML files, and
//! environment variables — and every source is decoded against the declared schema *before* any
//! merging occurs. Merging first and decoding last resolves a shape conflict by silently picking
//! a winner, which this framework does not do.
//!
//! # This is the recorded fallback, and it is custom infrastructure
//!
//! The eight-obligation proof gate ran against `confique` 0.4.0 and it **failed, 4 of 8**. Per
//! contract C-C7 the gate is all-eight-or-fallback, so the recorded `serde` + `toml`
//! partial-layer adapter was triggered. `schematic` 0.19.7 was re-examined first, as research §D6
//! pre-committed, and rejected on its own evidence — see that section.
//!
//! This crate is therefore custom infrastructure under FR-035, covered by
//! `decisions/0007-phase-002-custom-kernel-primitives.md`.
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** See the `renvor` facade documentation.

pub mod layer;
pub mod resolver;
pub mod schema;
pub mod source;

pub use layer::env::NESTING_SEPARATOR;
pub use layer::file::{FileLayer, MAX_FILE_BYTES};
pub use layer::merge::{DecodedLayer, Merged};
pub use resolver::{LayeredResolver, LayeredResolverBuilder};
pub use schema::ConfigSchema;
pub use source::{ConfigHandle, SchemaSource};
