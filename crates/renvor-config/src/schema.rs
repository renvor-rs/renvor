//! The schema an author declares, and the partial form one source decodes into.
//!
//! # Why an author writes two types
//!
//! Contract C-C3 requires **every source to be decoded against the declared typed schema before
//! any merging occurs**. A single source rarely sets every field, so decoding one source into the
//! full schema would fail with "missing field" on a file that was never meant to be complete.
//!
//! The decode target therefore has to be an **all-optional** form of the schema. `confique` and
//! `schematic` both generate that type with a derive macro; Renvor has no derive macro, so the
//! author writes it. That is a real ergonomic cost and it is stated here rather than discovered:
//!
//! ```
//! use serde::Deserialize;
//! use renvor_config::ConfigSchema;
//!
//! #[derive(Debug, Deserialize)]
//! struct Settings {
//!     host: String,
//!     port: u16,
//! }
//!
//! #[derive(Debug, Default, Deserialize)]
//! struct PartialSettings {
//!     host: Option<String>,
//!     port: Option<u16>,
//! }
//!
//! impl ConfigSchema for Settings {
//!     type Partial = PartialSettings;
//! }
//! ```
//!
//! The alternative was a proc-macro of Renvor's own, which is custom infrastructure under FR-035
//! and would need its own accepted decision record. Writing one to save an author a struct is not
//! a trade this phase is willing to make on its own authority — see the open items in
//! `governance/phase-002-evidence.md`.
//!
//! # What the partial is and is not used for
//!
//! It is used to **type-check one source in isolation**. It is **not** used to merge — merging
//! happens on decoded source trees, so it needs no per-field code and no macro. See
//! [`crate::layer::merge`] for why that is faithful to C-C2 rather than a shortcut around it.

use serde::de::DeserializeOwned;

/// A configuration type that can be resolved from layered sources.
///
/// Implemented by the author for the type they want back. Both this type and its [`Self::Partial`]
/// need `Deserialize`; nothing else is required, and in particular nothing here mentions TOML.
pub trait ConfigSchema: DeserializeOwned {
    /// The all-optional form of this schema, into which a **single** source is decoded.
    ///
    /// Every field must be optional. A field that is not optional would make a source that does
    /// not set it fail validation, which is the opposite of what per-source decoding is for.
    ///
    /// Adding `#[serde(deny_unknown_fields)]` is recommended and deliberately not forced: without
    /// it, a key no field claims is ignored rather than reported. Renvor cannot enforce it for the
    /// author because it has no description of the schema's keys to check against — see the open
    /// items in the evidence ledger.
    type Partial: DeserializeOwned + Default;
}
