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

/// Forbids provisioning in a child whose environment is otherwise **inherited**, not sealed
/// (FR-012-6's table).
///
/// # Why these two commands are the exception
///
/// The seal exists for the children that stand behind a claim: the five verification checks,
/// `rustfmt`, `doctor`'s probes, and the identification and resolution probes all report
/// something renvor then writes down, so what they ran with has to be known. `renvor dev` and
/// `renvor routes` report nothing of the sort — they run the operator's own project in the
/// operator's own shell, and clearing that shell would break a project that legitimately needs a
/// variable from it (a database URL, a registry token, a `RUSTFLAGS` the operator set on purpose).
///
/// What still holds, unconditionally, is SR-012-1: **nothing renvor spawns provisions a
/// toolchain.** So the inherited environment loses the two install-server variables and gains a
/// forced `RUSTUP_AUTO_INSTALL=0`, and nothing else about it changes.
pub(crate) fn no_provisioning(command: &mut std::process::Command) -> &mut std::process::Command {
    command
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env_remove("RUSTUP_DIST_SERVER")
        .env_remove("RUSTUP_UPDATE_ROOT")
}

pub mod check;
pub mod dev;
pub mod docker;
pub mod doctor;
pub mod generate;
pub mod new;
pub mod openapi;
pub mod relay;
pub mod routes;
pub mod tls;
