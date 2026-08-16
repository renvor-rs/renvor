//! Observability primitives: the entropy port and the per-run identifier.
//!
//! This module owns the parts of observability that must exist before any span is emitted. It
//! installs **no** global subscriber — that decision belongs to the application author, not to a
//! library (contract C-O7, research D4). A library that grabs the process-global subscriber takes
//! a decision away from every consumer that depends on it, and takes it silently.

pub mod entropy;
pub mod run_id;

pub use entropy::{EntropySource, EntropyUnavailable, FixedEntropy, OsEntropy};
pub use run_id::{RUN_IDENTIFIER_BYTES, RUN_IDENTIFIER_LEN, RunIdentifier};
