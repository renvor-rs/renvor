//! The transport-independent validation boundary for the Renvor framework.
//!
//! # One declaration, two consumers
//!
//! A constraint is declared **once**, as a schema. [`Declaration::validate`] interprets that
//! schema at runtime; [`Declaration::schema`] hands the **same value** to the OpenAPI serialiser.
//! "Runtime validation and published schemas agree" is therefore an **identity**, not a property
//! that tests have to keep chasing.
//!
//! This is the structure `contracts/http-routing.md` already established for routes, applied to
//! input rules:
//!
//! ```text
//!                     Declaration
//!                    (one schema value)
//!                   ╱                  ╲
//!            validate()              schema()
//!         runtime enforcement    published description
//! ```
//!
//! # This crate names no transport
//!
//! Nothing here imports `axum`, `tower`, `http`, or a server. A validation rule that named a
//! transport type would be unusable under any other, and constitution principle II forbids it.
//! Mapping an [`Issue`] to a status code is `renvor-http`'s job.
//!
//! # The enforced subset is bounded, and a declaration outside it is refused
//!
//! [`ENFORCED_KEYWORDS`] lists what Renvor checks. A schema using anything else is refused by
//! [`Declaration::new`] **at declaration time**, naming the keyword.
//!
//! That refusal is what separates a bounded subset from a partial implementation: a partial
//! implementation ignores what it does not understand, publishes the constraint anyway, and
//! enforces nothing — so the description becomes false exactly where an author was relying on it.
//!
//! # Example
//!
//! ```
//! use renvor_error::Location;
//! use renvor_validation::Declaration;
//! use serde_json::json;
//!
//! #[derive(serde::Serialize, schemars::JsonSchema)]
//! struct CreateItem {
//!     #[schemars(length(min = 1, max = 64))]
//!     name: String,
//!     #[schemars(range(min = 1, max = 1000))]
//!     quantity: u32,
//! }
//!
//! let declaration = Declaration::of::<CreateItem>()?;
//!
//! // The SAME value the OpenAPI document embeds.
//! assert!(declaration.schema().get("properties").is_some());
//!
//! let issues = declaration.validate(Location::Body, &json!({"name": "", "quantity": 5}));
//! assert_eq!(issues.len(), 1);
//! assert_eq!(issues[0].reason.as_str(), "too_short");
//! assert_eq!(issues[0].pointer.as_str(), "/name");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! # Stability
//!
//! This surface is **explicitly unstable**. See
//! [`contracts/api-stability.md`](https://github.com/renvor-rs/renvor/blob/main/contracts/api-stability.md).

pub mod collection;
pub mod cursor;
pub mod reason;
pub mod schema;

pub use collection::{
    CollectionContract, CollectionQuery, Direction, FilterOperator, FilterTerm, PageBounds,
    SortPlan, SortTerm,
};
pub use cursor::{CURSOR_VERSION, Cursor, CursorError, MAX_CURSOR_BYTES};
pub use reason::Reason;
pub use schema::{ANNOTATION_KEYWORDS, Declaration, DeclarationError, ENFORCED_KEYWORDS, Issue};

/// Renders issues as the wire form a Problem Details document carries.
///
/// A free function rather than a `From` implementation: the conversion is one-way and lossy —
/// [`Reason`] becomes a `&'static str` — and an implicit conversion would invite the reverse,
/// which cannot be written correctly.
#[must_use]
pub fn to_invalid_params(issues: Vec<Issue>) -> Vec<renvor_error::InvalidParam> {
    issues.into_iter().map(Issue::into_invalid_param).collect()
}
