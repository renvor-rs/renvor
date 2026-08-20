//! Observability primitives: the entropy port and the per-run identifier.
//!
//! This module owns the parts of observability that must exist before any span is emitted. It
//! installs **no** global subscriber — that decision belongs to the application author, not to a
//! library (contract C-O7,
//! [Phase 002 research §D4](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/002-core-kernel/research.md)).
//! A library that grabs the process-global subscriber takes
//! a decision away from every consumer that depends on it, and takes it silently.

pub mod bootstrap;
pub mod entropy;
pub mod run_id;
pub mod spans;

pub use bootstrap::{AlreadyInstalled, try_init_global};
pub use entropy::{EntropySource, EntropyUnavailable, FixedEntropy, OsEntropy};
pub use run_id::{RUN_IDENTIFIER_BYTES, RUN_IDENTIFIER_LEN, RunIdentifier};
pub use spans::{PHASE_SPAN_NAME, phase_span};
