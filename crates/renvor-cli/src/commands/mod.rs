//! The commands this phase implements.
//!
//! **Nothing here is a stub.** `PLAN.md` §9.3 lists `generate`, `migrate`, `seed`, `routes`,
//! `openapi`, and the package-ecosystem surface; none of them appears in this module or in the
//! flag surface, because a command that exits zero without doing the work reports success for
//! something that did not happen.

pub mod check;
pub mod dev;
pub mod docker;
pub mod doctor;
pub mod new;
