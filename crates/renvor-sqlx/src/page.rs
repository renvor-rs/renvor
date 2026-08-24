//! Binding a C-15 cursor to storage.
//!
//! # The renderers moved to `renvor-database` in Phase 007
//!
//! They imported no driver type, so keeping them here meant the SeaORM adapter could not reach
//! them — and FR-034 requires the two rows to produce *identical* ordering and cursors. Copying
//! would have satisfied the words; two renderers agree until one is edited. They are re-exported
//! here so every existing path keeps working, and so a reader who looks for them where they used
//! to be finds them.

pub use renvor_database::page::{limit_clause, order_clause, seek_predicate};
