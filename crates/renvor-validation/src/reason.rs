//! The closed vocabulary of validation reasons.
//!
//! # Why this is an enum and not a message
//!
//! A validator's own message quotes the offending input. Measured during this phase's tooling
//! work, a real JSON Schema implementation produced:
//!
//! ```text
//! "not an object" is not of type "object"
//! ```
//!
//! The rejected value is *inside the message*. Any design that rendered a validator's `Display`
//! into a response would have leaked it — which is why Renvor maps to this enum and never renders
//! anyone's message, including its own.
//!
//! Each reason converts to a `&'static str`, which is the type
//! [`renvor_error::InvalidParam::reason`] requires. That is not a convenience: a `&'static str`
//! cannot hold a formatted string, so the leak is impossible rather than merely avoided.

use core::fmt;

/// Why an input was refused.
///
/// Closed and `#[non_exhaustive]`: a consumer matches the reasons it handles and treats the rest
/// generically, and Renvor can add one without a breaking change — the same rule the public API
/// error registry follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Reason {
    /// The value was of the wrong JSON type.
    TypeMismatch,
    /// A required member was absent.
    RequiredMissing,
    /// A member was present that the schema does not declare.
    ///
    /// The member's **name is not reported**, because for an undeclared member the name is
    /// attacker-chosen. The pointer names the containing object instead.
    UnknownMember,
    /// A string was shorter than the declared minimum.
    TooShort,
    /// A string was longer than the declared maximum.
    TooLong,
    /// A number was outside the declared range.
    OutOfRange,
    /// A number was not a multiple of the declared step.
    NotMultipleOf,
    /// An array had fewer items than the declared minimum.
    TooFewItems,
    /// An array had more items than the declared maximum.
    TooManyItems,
    /// An array declared unique items and contained a repeat.
    NotUnique,
    /// A value was outside the declared set.
    NotInEnum,
    /// A value was not the single declared constant.
    NotConstant,
    /// A page cursor could not be decoded.
    CursorInvalid,
    /// A page cursor declared an encoding version this build does not understand.
    CursorVersionUnsupported,
    /// A key was supplied more than once where one value is expected.
    DuplicateKey,
    /// A filter or sort key is outside the operation's declared allowlist.
    NotAllowlisted,
    /// A filter operator is outside the declared closed set.
    OperatorNotAllowed,
    /// A list-shaped input exceeded its declared bound.
    TooManyTerms,
}

impl Reason {
    /// The stable wire spelling.
    ///
    /// `&'static str`, which is what makes it impossible to substitute a formatted message here.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TypeMismatch => "type_mismatch",
            Self::RequiredMissing => "required_missing",
            Self::UnknownMember => "unknown_member",
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
            Self::OutOfRange => "out_of_range",
            Self::NotMultipleOf => "not_multiple_of",
            Self::TooFewItems => "too_few_items",
            Self::TooManyItems => "too_many_items",
            Self::NotUnique => "not_unique",
            Self::NotInEnum => "not_in_enum",
            Self::NotConstant => "not_constant",
            Self::CursorInvalid => "cursor_invalid",
            Self::CursorVersionUnsupported => "cursor_version_unsupported",
            Self::DuplicateKey => "duplicate_key",
            Self::NotAllowlisted => "not_allowlisted",
            Self::OperatorNotAllowed => "operator_not_allowed",
            Self::TooManyTerms => "too_many_terms",
        }
    }

    /// Every reason, for registry tests.
    pub const ALL: [Self; 18] = [
        Self::TypeMismatch,
        Self::RequiredMissing,
        Self::UnknownMember,
        Self::TooShort,
        Self::TooLong,
        Self::OutOfRange,
        Self::NotMultipleOf,
        Self::TooFewItems,
        Self::TooManyItems,
        Self::NotUnique,
        Self::NotInEnum,
        Self::NotConstant,
        Self::CursorInvalid,
        Self::CursorVersionUnsupported,
        Self::DuplicateKey,
        Self::NotAllowlisted,
        Self::OperatorNotAllowed,
        Self::TooManyTerms,
    ];
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::Reason;
    use std::collections::BTreeSet;

    #[test]
    fn every_reason_has_a_distinct_lower_snake_case_name() {
        let names: BTreeSet<&str> = Reason::ALL.iter().map(|r| r.as_str()).collect();
        assert_eq!(names.len(), Reason::ALL.len(), "two reasons share a name");
        for reason in Reason::ALL {
            let name = reason.as_str();
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "`{name}` is not lower_snake_case"
            );
        }
    }

    #[test]
    fn no_reason_reads_like_a_message_that_would_quote_a_value() {
        // A reason is a NAME. If one ever contained a space, a quote, or a colon, it would be a
        // message — and a message is where a rejected value hides.
        for reason in Reason::ALL {
            let name = reason.as_str();
            for forbidden in [' ', '"', '\'', ':', '`'] {
                assert!(
                    !name.contains(forbidden),
                    "`{name}` contains `{forbidden}`, which makes it read as a message"
                );
            }
        }
    }
}
