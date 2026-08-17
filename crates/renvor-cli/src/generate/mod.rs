//! The generation transaction.
//!
//! The protocol is contract C-5:
//!
//! ```text
//! 1. VALIDATE   every choice and the destination boundary — nothing has touched the filesystem
//! 2. STAGE      a directory the process owns, INSIDE the destination's parent
//! 3. RENDER     bounded template expansion into staging
//! 4. MANIFEST   walk the staged tree, sorted
//! 5. VERIFY     the generated project's own checks, still in staging
//! 6. PLACE      one rename
//! 7. REPORT
//! ```
//!
//! Failure anywhere from 1 to 5 removes the staging directory and leaves the destination exactly
//! as it was. That is enforced by [`place::Staging`]'s `Drop`, so it holds on paths nobody wrote
//! a cleanup for — including a panic.

pub mod manifest;
pub mod place;
pub mod render;
