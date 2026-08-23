//! Exit codes and the stable error-code registry.
//!
//! Both are **public contracts** (`contracts/command-surface.md` and
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
    /// The destination already exists, in any form. See [`Code::DestinationExists`].
    ///
    /// # This replaced `destination_not_empty` in schemaVersion 2
    ///
    /// FR-013 used to refuse a destination that existed and was **not empty**, so an existing
    /// *empty* directory was accepted — and placement then deleted and recreated it, which changed
    /// its inode, its mode, and its ownership. `destination_not_empty` was the right name for the
    /// old rule and would be a **false statement** under the new one: refusing an empty directory
    /// with a code that says "not empty" tells a consumer something untrue.
    ///
    /// So the code was replaced rather than redefined, and `schemaVersion` went to `2` because
    /// removing an entry from a closed registry is a breaking change. `details.found` names what
    /// was there.
    DestinationExists,
    /// The destination failed a path-boundary rule.
    DestinationRejected,
    /// The destination's parent does not exist or does not resolve.
    DestinationParentMissing,
    /// `renvor.toml` failed validation.
    ManifestInvalid,
    /// A project's own checks failed, or could not be run.
    ///
    /// Distinct from [`Code::ManifestInvalid`], which is about `renvor.toml`, and from
    /// [`Code::RenderFailed`], which is about template rendering. Added in schemaVersion 2; see the
    /// note on [`Code::ContainerControlsMissing`].
    ProjectVerificationFailed,
    /// The operator cancelled.
    Cancelled,
    /// A required tool is absent or incompatible.
    ToolMissing,
    /// The container runtime is not installed, or is installed and not running.
    ContainerRuntimeUnavailable,
    /// The project does not declare Renvor transport wiring, so its routes cannot be obtained.
    ///
    /// # Why this is a distinct code rather than a silent empty list
    ///
    /// Route inspection asks the **application binary** for its own route registry — that is the
    /// only source that cannot drift from the router. A project that does not depend on the
    /// framework has no such registry to report.
    ///
    /// Printing an empty route table and exiting `0` would be indistinguishable, to a consumer,
    /// from an application that genuinely declares no routes, and the two mean entirely different
    /// things. So this is a failure with a name.
    ///
    /// **Added in Phase 004.** Adding a code to the closed registry is **not** a `schemaVersion`
    /// bump: C-2 defines breaking as renaming, reusing, or **removing** a code. The `1 → 2` bump
    /// was caused by a removal, not by the four additions that accompanied it.
    TransportNotWired,
    /// The project has no container controls to drive.
    ///
    /// # Why three codes were added in schemaVersion 2 rather than reusing existing ones
    ///
    /// An advisory review (A-R6) found three failures reported under codes whose published meaning
    /// did not cover them: a failing `cargo test` and a missing `compose.yaml` both emitted
    /// `manifest_invalid` — *"`renvor.toml` failed validation"* — and a staging directory that
    /// could not be **created** emitted `placement_failed` — *"the final move could not be
    /// performed"*. A consumer matching the published meanings drew the wrong conclusion in all
    /// three cases.
    ///
    /// The fix could have been to widen those three registry entries. It was not, because a
    /// registry entry that covers whatever the code happens to emit is not a contract. Three
    /// accurate codes were added instead.
    ContainerControlsMissing,
    /// Template rendering failed. The destination is untouched.
    RenderFailed,
    /// A documented bound was exceeded.
    BoundExceeded,
    /// The staging directory could not be created.
    ///
    /// Separate from [`Code::PlacementFailed`]: nothing has been staged yet, so nothing can have
    /// been left behind, and the remedy is different — this almost always means the destination's
    /// parent is not writable.
    StagingFailed,
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
            Self::DestinationExists => "destination_exists",
            Self::DestinationRejected => "destination_rejected",
            Self::DestinationParentMissing => "destination_parent_missing",
            Self::ManifestInvalid => "manifest_invalid",
            Self::ProjectVerificationFailed => "project_verification_failed",
            Self::Cancelled => "cancelled",
            Self::ToolMissing => "tool_missing",
            Self::ContainerRuntimeUnavailable => "container_runtime_unavailable",
            Self::TransportNotWired => "transport_not_wired",
            Self::ContainerControlsMissing => "container_controls_missing",
            Self::RenderFailed => "render_failed",
            Self::BoundExceeded => "bound_exceeded",
            Self::StagingFailed => "staging_failed",
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
            | Self::DestinationExists
            | Self::DestinationRejected
            | Self::DestinationParentMissing
            | Self::ManifestInvalid
            | Self::ProjectVerificationFailed
            | Self::ContainerControlsMissing
            | Self::TransportNotWired
            | Self::RenderFailed
            | Self::BoundExceeded
            | Self::StagingFailed
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
    pub const ALL: [Self; 20] = [
        Self::Usage,
        Self::UnsupportedValue,
        Self::UnsupportedCombination,
        Self::ReservedForLaterPhase,
        Self::InvalidProjectName,
        Self::DestinationExists,
        Self::DestinationRejected,
        Self::DestinationParentMissing,
        Self::ManifestInvalid,
        Self::ProjectVerificationFailed,
        Self::Cancelled,
        Self::ToolMissing,
        Self::ContainerRuntimeUnavailable,
        Self::ContainerControlsMissing,
        Self::TransportNotWired,
        Self::RenderFailed,
        Self::BoundExceeded,
        Self::StagingFailed,
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
        //
        // 19 -> 20 in Phase 004: `transport_not_wired`. The literal is updated deliberately rather
        // than derived, because deriving it from `ALL` would make this assertion vacuous — it
        // exists precisely so that growing the registry is a decision somebody records.
        assert_eq!(Code::ALL.len(), 20);
    }

    /// The published registry, parsed out of the contract document itself.
    ///
    /// # Why this reads the document rather than restating it
    ///
    /// A hand-copied list in this file would be a **second** statement of the registry, and two
    /// statements drift. This one has no independent content: it is the contract's own table, so
    /// there is nothing for it to disagree with except reality.
    ///
    /// `include_str!` resolves at compile time, so a moved or deleted contract is a build failure
    /// rather than a skipped test.
    fn published_registry() -> Vec<(String, u8)> {
        const CONTRACT: &str = include_str!("../../../contracts/json-output.md");

        // The registry is the table whose header is `| Code | Exit | Meaning |`. Bounded to that
        // one table so the schema-history tables below it, which have different columns, cannot be
        // mistaken for registry rows.
        let table = CONTRACT
            .split_once("| Code | Exit | Meaning |")
            .expect("the contract still contains the registry table")
            .1;

        table
            .lines()
            .map(str::trim)
            // Past the remainder of the header line, then rows until the table ends.
            .skip_while(|line| !line.starts_with('|'))
            .take_while(|line| line.starts_with('|'))
            // Drop the `|---|---|---|` separator, recognised by having no content of its own
            // rather than by position — a row is a row wherever it sits.
            .filter(|line| !line.chars().all(|c| "|-: ".contains(c)))
            .map(|line| {
                let mut cells = line.split('|').skip(1);
                let code = cells
                    .next()
                    .expect("a code cell")
                    .trim()
                    .trim_matches('`')
                    .to_owned();
                let exit = cells
                    .next()
                    .expect("an exit cell")
                    .trim()
                    .parse::<u8>()
                    .expect("the exit column is a number");
                (code, exit)
            })
            .collect()
    }

    #[test]
    fn the_registry_matches_the_published_contract_exactly() {
        // THE REGISTRY IS A PUBLISHED CLOSED SET, so "complete" above is not enough: a variant
        // could be added to both the enum and `ALL` and never reach `contracts/json-output.md`,
        // and a consumer reading the contract would then meet a code the contract does not list.
        let published = published_registry();

        // POSITIVE CONTROL FIRST. A parser that returned an empty vector would make every
        // assertion below vacuous, and a vacuous gate is the exact defect an advisory review found
        // elsewhere in this phase.
        assert_eq!(
            published.len(),
            Code::ALL.len(),
            "parsed {} rows from the contract's registry table for {} codes",
            published.len(),
            Code::ALL.len()
        );

        let mut emitted: Vec<&str> = Code::ALL.iter().map(|code| code.as_str()).collect();
        let mut expected: Vec<&str> = published.iter().map(|(code, _)| code.as_str()).collect();
        emitted.sort_unstable();
        expected.sort_unstable();
        assert_eq!(
            emitted, expected,
            "the emitted registry and the published registry have diverged; a consumer matching \
             the contract would meet a code it does not list, or would wait for one that never \
             arrives"
        );
        assert!(
            !emitted.contains(&"destination_not_empty"),
            "`destination_not_empty` was RETIRED in schemaVersion 2: it would be a false statement \
             about an empty directory, which the new FR-013 also refuses"
        );
    }

    #[test]
    fn every_code_exits_with_exactly_the_number_the_contract_publishes() {
        // The registry's second column is as much a contract as its first: a script branches on
        // the exit code, and a code that quietly moved from 3 to 5 changes what that script does
        // without changing anything it can see.
        let published = published_registry();
        assert!(!published.is_empty(), "the registry table parsed as empty");

        for (name, exit) in &published {
            let code = Code::ALL
                .iter()
                .find(|candidate| candidate.as_str() == name)
                .unwrap_or_else(|| panic!("`{name}` is published but is not emitted"));
            assert_eq!(
                i32::from(*exit),
                code.exit().code(),
                "`{name}` is published as exit {exit} and exits {}",
                code.exit().code()
            );
        }
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
