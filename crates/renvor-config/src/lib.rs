//! Typed, layered configuration and secret redaction for Renvor.
//!
//! Configuration arrives from exactly three source kinds — built-in defaults, TOML files, and
//! environment variables — and every source is decoded against the declared schema *before* any
//! merging occurs. Merging first and decoding last resolves a shape conflict by silently picking
//! a winner, which this framework does not do.
//!
//! # Stability
//!
//! **This surface is explicitly unstable.** See the `renvor` facade documentation.
