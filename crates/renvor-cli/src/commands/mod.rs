//! The commands this phase implements.
//!
//! **Nothing here is a stub.** `PLAN.md` §9.3 lists `generate`, `migrate`, `seed`, `routes`,
//! `openapi`, and the package-ecosystem surface. `routes` **ships in Phase 004**, with the
//! transport it inspects; the rest still do not appear in this module or in the flag surface,
//! because a command that exits zero without doing the work reports success for something that did
//! not happen.
//!
//! `routes` is held to the same rule: it **fails** when it cannot obtain a route registry, rather
//! than printing an empty table and exiting zero.

pub mod check;
pub mod dev;
pub mod docker;
pub mod doctor;
pub mod new;
pub mod openapi;
pub mod relay;
pub mod routes;
pub mod tls;
