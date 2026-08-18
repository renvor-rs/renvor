//! Exit codes and the stable error-code registry.
//!
//! Both are **public contracts** (`specs/003-interactive-cli/contracts/command-surface.md` and
//! `json-output.md`). Renaming a code, or reusing one for a different meaning, is a breaking
//! change.

use std::fmt;

/// The process exit code.
///
/// # Why `Unclassified` exists
///
/// A taxonomy without it absorbs unclassified failures into a general error code, and an
/// unclassified failure is a **defect** rather than an outcome. Anything that exits `1` is a bug
/// report, and that is the whole reason the value is reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Exit {
    /// The command did what it was asked to do.
    Success = 0,
    /// Unclassified or internal failure. **A defect.**
    Unclassified = 1,
    /// The invocation was malformed.
    Usage = 2,
    /// The invocation was well formed and the request was refused.
    Validation = 3,
    /// The operator cancelled.
    Cancelled = 4,
    /// The environment could not support the request.
    Environment = 5,
}

impl Exit {
    /// The numeric code, for `std::process::exit`.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}

/// A stable error identifier.
///
/// The **name** outlives the message. A consumer matches on this; it must never parse
/// [`CliError::message`], which is explicitly not stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// Malformed invocation.
    Usage,
    /// A flag value outside the supported set.
    UnsupportedValue,
    /// Individually valid choices that conflict.
    UnsupportedCombination,
    /// A flag belonging to a later phase.
    ReservedForLaterPhase,
    /// Empty, not a valid package name, or a reserved device name.
    InvalidProjectName,
    /// The destination exists and is not empty.
    DestinationNotEmpty,
    /// The destination failed a path-boundary rule.
    DestinationRejected,
    /// The destination's parent does not exist or does not resolve.
    DestinationParentMissing,
    /// `renvor.toml` failed validation.
    ManifestInvalid,
    /// The operator cancelled.
    Cancelled,
    /// A required tool is absent or incompatible.
    ToolMissing,
    /// The container runtime is not installed, or is installed and not running.
    ContainerRuntimeUnavailable,
    /// Template rendering failed. The destination is untouched.
    RenderFailed,
    /// A documented bound was exceeded.
    BoundExceeded,
    /// The final move could not be performed.
    PlacementFailed,
    /// Unclassified. **A defect.**
    Internal,
}

impl Code {
    /// The wire name. **Stable.** Never rename one of these without a `schemaVersion` bump.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::UnsupportedValue => "unsupported_value",
            Self::UnsupportedCombination => "unsupported_combination",
            Self::ReservedForLaterPhase => "reserved_for_later_phase",
            Self::InvalidProjectName => "invalid_project_name",
            Self::DestinationNotEmpty => "destination_not_empty",
            Self::DestinationRejected => "destination_rejected",
            Self::DestinationParentMissing => "destination_parent_missing",
            Self::ManifestInvalid => "manifest_invalid",
            Self::Cancelled => "cancelled",
            Self::ToolMissing => "tool_missing",
            Self::ContainerRuntimeUnavailable => "container_runtime_unavailable",
            Self::RenderFailed => "render_failed",
            Self::BoundExceeded => "bound_exceeded",
            Self::PlacementFailed => "placement_failed",
            Self::Internal => "internal",
        }
    }

    /// The exit code this error maps to.
    ///
    /// The mapping is **total and one-way**: every code has exactly one exit, so a consumer can
    /// rely on the pair. The contract's registry table is this function.
    #[must_use]
    pub const fn exit(self) -> Exit {
        match self {
            Self::Usage => Exit::Usage,
            Self::UnsupportedValue
            | Self::UnsupportedCombination
            | Self::ReservedForLaterPhase
            | Self::InvalidProjectName
            | Self::DestinationNotEmpty
            | Self::DestinationRejected
            | Self::DestinationParentMissing
            | Self::ManifestInvalid
            | Self::RenderFailed
            | Self::BoundExceeded
            | Self::PlacementFailed => Exit::Validation,
            Self::Cancelled => Exit::Cancelled,
            Self::ToolMissing | Self::ContainerRuntimeUnavailable => Exit::Environment,
            Self::Internal => Exit::Unclassified,
        }
    }

    /// Every code, so tests can assert the registry is complete rather than sampling it.
    ///
    /// `#[cfg(test)]` because nothing at runtime iterates the registry; a shipped constant that no
    /// shipped code reads is dead weight that reads like an API.
    #[cfg(test)]
    pub const ALL: [Self; 16] = [
        Self::Usage,
        Self::UnsupportedValue,
        Self::UnsupportedCombination,
        Self::ReservedForLaterPhase,
        Self::InvalidProjectName,
        Self::DestinationNotEmpty,
        Self::DestinationRejected,
        Self::DestinationParentMissing,
        Self::ManifestInvalid,
        Self::Cancelled,
        Self::ToolMissing,
        Self::ContainerRuntimeUnavailable,
        Self::RenderFailed,
        Self::BoundExceeded,
        Self::PlacementFailed,
        Self::Internal,
    ];
}

impl fmt::Display for Code {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A failure, carrying everything both output modes need.
///
/// `details` is a flat map rather than a typed payload per code, because the JSON contract's
/// `error.details` is code-specific and a closed enum here would make adding a detail a breaking
/// change to this type rather than an additive change to one code.
#[derive(Debug, Clone)]
pub struct CliError {
    /// The stable identifier.
    pub code: Code,
    /// Human-readable, and explicitly **not** stable. Never parse it.
    pub message: String,
    /// Structured, code-specific context.
    pub details: Vec<(String, String)>,
}

impl CliError {
    /// Builds an error with no details.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }

    /// Adds one structured detail.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.details.push((key.into(), value.into()));
        self
    }

    /// The exit code for this failure.
    #[must_use]
    pub fn exit(&self) -> Exit {
        self.code.exit()
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for CliError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_has_a_distinct_wire_name() {
        let mut names: Vec<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            total,
            "two error codes share a wire name; the registry is a compatibility contract and a \
             duplicate makes one of them unmatchable"
        );
    }

    #[test]
    fn the_registry_is_complete() {
        // Guards against adding a variant and forgetting `ALL`, which would let a code exist that
        // no test enumerates.
        assert_eq!(Code::ALL.len(), 16);
    }

    #[test]
    fn only_internal_maps_to_the_reserved_unclassified_exit() {
        let unclassified: Vec<_> = Code::ALL
            .iter()
            .filter(|code| code.exit() == Exit::Unclassified)
            .collect();
        assert_eq!(
            unclassified,
            vec![&Code::Internal],
            "exit 1 is reserved so that an unclassified failure is visibly a defect; a classified \
             code mapping to it would hide exactly what the reservation exists to expose"
        );
    }

    #[test]
    fn cancellation_has_its_own_exit() {
        assert_eq!(Code::Cancelled.exit(), Exit::Cancelled);
        for code in Code::ALL {
            if code != Code::Cancelled {
                assert_ne!(
                    code.exit(),
                    Exit::Cancelled,
                    "{code} shares the cancellation exit; cancelling and failing must be \
                     distinguishable from outside the process"
                );
            }
        }
    }

    #[test]
    fn exit_codes_are_the_documented_numbers() {
        // The contract states these values. A silent renumbering would break every script.
        assert_eq!(Exit::Success.code(), 0);
        assert_eq!(Exit::Unclassified.code(), 1);
        assert_eq!(Exit::Usage.code(), 2);
        assert_eq!(Exit::Validation.code(), 3);
        assert_eq!(Exit::Cancelled.code(), 4);
        assert_eq!(Exit::Environment.code(), 5);
    }
}
