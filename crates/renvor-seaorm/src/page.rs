//! Binding a C-15 cursor to storage, for the SeaORM row.
//!
//! # There is one renderer, not two
//!
//! FR-034 requires SeaORM to produce **identical** ordering and cursors to the direct-SQLx row.
//! The obvious way to satisfy that sentence is to write a second renderer that agrees with the
//! first — and two renderers agree until one is edited, after which the guarantee is a claim about
//! a diff nobody read.
//!
//! So there is no second renderer. `seek_predicate`, `order_clause` and `limit_clause` live in
//! `renvor-database` — they import no driver type and never did — and both adapters re-export the
//! same functions. "Identical" is then a property of the build rather than of a review.
//!
//! Phase 007 moved them there. A review found FR-034 evidenced by the argument *"shared types; no
//! adapter-specific behaviour exists"*, which was false while `renvor-sqlx/src/page.rs` held the
//! renderers and this crate had no counterpart at all.

pub use renvor_database::page::{limit_clause, order_clause, seek_predicate};
