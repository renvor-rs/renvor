//! One declaration for a closed enum and its rendered names.
//!
//! See [`closed_named_enum`] for what it generates and why.
//!
//! # Why this is a second copy, and why it may not be shared
//!
//! `renvor-database` carries the same macro. That is duplication, and it is **forced by the crate
//! DAG rather than chosen**: `renvor-database` declares no dependency on this kernel, deliberately,
//! and `xtask` step 7 asserts that absence against the resolved graph **with a positive control**.
//! Importing this macro there would re-open a dependency the architecture invariant exists to keep
//! shut; importing the database copy from `renvor-http` would breach transport/persistence
//! isolation, which the same step asserts.
//!
//! The two copies generate independent types, so they cannot disagree about a *value* — only about
//! which features the macro offers. Stated here rather than left for a reader to discover, because
//! duplication that is invisible is duplication that drifts.

/// Declares a fieldless enum together with `as_str`, from a single list.
///
/// # The mutation this exists to make unrepresentable
///
/// A closed enum written by hand is several authorities that can disagree: the `enum` itself, any
/// exhaustive list beside it, and the `as_str` match. `renvor-database` learned this the expensive
/// way — its `DatabaseAdapter` survived a mutation (**M-24b**) that added `Custom(&'static str)`
/// and left every test green, because `as_str`'s catch-all-free match forces an author to *handle*
/// a new variant, not to handle it *safely*: returning the carried string satisfies the compiler.
///
/// The list is written once here. A **unit** variant is added in one reviewed line. A
/// **data-bearing** variant does not match `$variant:ident` and is a macro error before it is
/// anything else — so a caller cannot pass runtime text, and a maintainer cannot re-open the
/// channel that would let them.
///
/// # Why `renvor-http` needs it
///
/// `renvor_http::HttpErrorDetail` is not declared here — it belongs to the transport — but
/// the property it needs is exactly this one. `HttpError::new` used to take
/// `detail: impl Into<String>`, so an application author could write
/// `HttpError::new(kind, format!("could not reach {dsn}"))` and put a DSN into telemetry by design.
/// Closing the constructor stops the caller. Only closing the **declaration** stops the next
/// maintainer from adding a variant that carries the text again.
///
/// # Syntax
///
/// ```
/// renvor_core::closed_named_enum! {
///     /// Doc comments, attributes and visibility pass through.
///     pub enum Flavour {
///         /// Each variant carries its own documentation.
///         Sweet => "sweet",
///         /// And its reviewed rendering, as a literal.
///         Salty => "salty",
///     }
/// }
///
/// assert_eq!(Flavour::ALL.len(), 2);
/// assert_eq!(Flavour::Salty.as_str(), "salty");
/// ```
///
/// `Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash` and
/// `#[non_exhaustive]` are applied by the macro rather than by the caller, so an invocation cannot
/// drop one.
///
/// # The two controls
///
/// **Control 1 — a unit variant is added in one line and reaches `ALL` on its own.**
///
/// ```
/// renvor_core::closed_named_enum! {
///     /// Two reviewed reasons, and a third.
///     pub enum Reason {
///         /// The first.
///         HostRejected => "host_rejected",
///         /// The second.
///         BodyTooLarge => "body_too_large",
///         /// A third, added the way a genuine new reason arrives.
///         OriginRejected => "origin_rejected",
///     }
/// }
/// assert_eq!(Reason::ALL.len(), 3);
/// assert_eq!(Reason::ALL[2].as_str(), "origin_rejected");
/// ```
///
/// **Control 2 — the same declaration, differing in exactly one token sequence:
/// `OriginRejected` becomes `Custom(&'static str)`. It does not compile.**
///
/// ```compile_fail
/// renvor_core::closed_named_enum! {
///     /// Two reviewed reasons, and a third.
///     pub enum Reason {
///         /// The first.
///         HostRejected => "host_rejected",
///         /// The second.
///         BodyTooLarge => "body_too_large",
///         /// A third, carrying caller text.
///         Custom(&'static str) => "origin_rejected",
///     }
/// }
/// assert_eq!(Reason::ALL.len(), 3);
/// assert_eq!(Reason::ALL[2].as_str(), "origin_rejected");
/// ```
///
/// The pair is the point. A `compile_fail` block passes when compilation fails for **any** reason,
/// including a typo, so on its own it proves nothing. Control 1 is the same source with the one
/// token sequence changed back: it compiles and its assertions hold, so the only thing control 2
/// can be failing on is the variant shape.
#[macro_export]
macro_rules! closed_named_enum {
    (
        $(#[$enum_meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_meta:meta])*
                $variant:ident => $rendered:literal
            ),+ $(,)?
        }
    ) => {
        $(#[$enum_meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        $vis enum $name {
            $(
                $(#[$variant_meta])*
                $variant,
            )+
        }

        impl $name {
            /// Every variant, in declaration order.
            ///
            /// Generated from the same list as the enum itself, so it cannot fall behind it.
            pub const ALL: [Self; <[()]>::len(&[$($crate::closed_named_enum!(@unit $variant)),+])] =
                [$(Self::$variant),+];

            /// The variant's reviewed name.
            ///
            /// Every arm returns a literal written at the declaration. No value derived from
            /// configuration, a caller, or a server can reach the returned string.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $rendered,)+
                }
            }
        }
    };

    // Internal: one `()` per variant, so `ALL`'s length is counted rather than restated.
    (@unit $variant:ident) => {
        ()
    };
}
