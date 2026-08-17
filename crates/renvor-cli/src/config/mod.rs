//! The one validated configuration model, and the two interfaces that produce it.
//!
//! Constitution principle VII requires the wizard and the flags to resolve to the **same**
//! validated configuration. That is enforced here structurally: [`model::ProjectConfiguration`]
//! has one constructor, both interfaces call it, and nothing downstream can tell which one ran.

pub mod flags;
pub mod model;
pub mod prompts;
