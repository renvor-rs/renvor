//! The two steps of contract C-C2, and the sources they operate on.
//!
//! | Module | Step | Contract |
//! |---|---|---|
//! | [`decode`] | decode **one** source against the schema | C-C3 |
//! | [`merge`] | merge decoded sources by precedence | C-C4, C-C5 |
//! | [`attribution`] | report which layer won each key | C-C6, FR-016 |
//! | [`env`] | the environment, as a **layer** | C-C4 |
//! | [`file`] | a TOML file, required or optional | C-C11 |
//!
//! The order is the whole point. Merging first and decoding last — the model used by the
//! mainstream layered-configuration crates — resolves a shape conflict by picking a winner
//! silently, which constitution principle IV prohibits and which the proof gate caught.

pub mod attribution;
pub mod decode;
pub mod env;
pub mod file;
pub mod merge;
