//! One declaration for a closed enum, its exhaustive list, and its rendered names.
//!
//! See [`closed_named_enum`](crate::closed_named_enum) for what it generates and why.

/// Declares a fieldless enum together with `ALL` and `as_str`, from a single list.
///
/// # The mutation this exists to make unrepresentable
///
/// [`DatabaseAdapter`](crate::DatabaseAdapter) was hand-written as three separate authorities: the
/// `enum`, a `const ALL: [Self; 2]`, and an `as_str` match. Phase 008 ran a mutation — **M-24** —
/// that added `Custom(&'static str)` **and listed it in `ALL`**, and the guard test caught it. The
/// ledger recorded the mutation as killed.
///
/// That conclusion was too broad, and a review found the gap. Adding `Custom(&'static str)` while
/// **omitting it from `ALL`** left every test green:
///
/// - `as_str`'s catch-all-free match forces the author to *handle* the variant, not to handle it
///   safely. Returning the carried string satisfies the compiler.
/// - `#[non_exhaustive]` does not apply within the declaring crate, so nothing else objects.
/// - Every guard test iterates `ALL`, so a variant absent from `ALL` is a variant no test can
///   reach. The exhaustive redaction enumeration never constructs it; the reviewed-names check
///   compares `ALL.len()` against its own list and finds two against two.
///
/// The defect was not a missing assertion. It was that `ALL` was a **hand-maintained restatement**
/// of the variant list, so the two could disagree — and a test that reads the restatement cannot
/// notice that it disagrees.
///
/// # What this changes
///
/// The list is written once. `ALL` is generated from it, so a variant that exists but is absent
/// from `ALL` is not something an author can express. A **unit** variant is added in one reviewed
/// line and appears everywhere automatically. A **data-bearing** variant does not match
/// `$variant:ident` and is a macro error before it is anything else.
///
/// Note the ordering: this closes the hole at the *declaration*, where the earlier fix closed it at
/// the *constructor*. Closing the constructor stopped a caller passing text; only this stops a
/// maintainer re-opening it.
///
/// # Syntax
///
/// ```
/// renvor_database::closed_named_enum! {
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
/// **Control 1 — a unit variant is added in one line and reaches `ALL` on its own.** This is the
/// property the hand-written form did not have.
///
/// ```
/// renvor_database::closed_named_enum! {
///     /// Two reviewed adapters, and a third.
///     pub enum Adapter {
///         /// The direct-SQLx adapter.
///         Sqlx => "renvor-sqlx",
///         /// The SeaORM adapter.
///         SeaOrm => "renvor-seaorm",
///         /// A third, added the way a genuine new adapter arrives.
///         Turso => "renvor-turso",
///     }
/// }
/// assert_eq!(Adapter::ALL.len(), 3);
/// assert_eq!(Adapter::ALL[2].as_str(), "renvor-turso");
/// ```
///
/// **Control 2 — M-24b itself. The same declaration, differing in exactly one token sequence:
/// `Turso` becomes `Custom(&'static str)`. It does not compile.**
///
/// ```compile_fail
/// renvor_database::closed_named_enum! {
///     /// Two reviewed adapters, and a third.
///     pub enum Adapter {
///         /// The direct-SQLx adapter.
///         Sqlx => "renvor-sqlx",
///         /// The SeaORM adapter.
///         SeaOrm => "renvor-seaorm",
///         /// A third, carrying caller text.
///         Custom(&'static str) => "renvor-turso",
///     }
/// }
/// assert_eq!(Adapter::ALL.len(), 3);
/// assert_eq!(Adapter::ALL[2].as_str(), "renvor-turso");
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
