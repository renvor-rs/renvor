//! The two stderr notices of FR-012-8 (as corrected under A-8) and the cached-artifacts line of
//! FR-012-7d (d), as pure functions.
//!
//! Each returns the exact line or `None`; the command that runs a verification prints it through
//! the reporter's diagnostic stream — stderr, never stdout — so the text lives in one place and
//! a table test pins it. Nothing here decides whether to refuse: a difference is **said**, never
//! refused (SR-012-2), because the operator or the CI chose it.
//!
//! # The three lines
//!
//! - **Resolution** (FR-012-8 (1)): what the preflight resolution of FR-012-7b found, against
//!   the project's pin. Printed when the resolved release is not the pinned channel, when the
//!   compiler is bare (`no_rustup`: the pin cannot be selected there), when rustup's attribution
//!   is one the table does not know (`unknown`: a clean machine's selection is the file, and
//!   whatever this was, it was not that), and for a legacy tree that pins nothing. It describes
//!   the resolution and nothing else — never Cargo's effective compiler.
//! - **Observation** (FR-012-8 (2)): the compiler a launch was observed for, when its queried
//!   identity differs from the resolution in release or commit — printed **whether or not the
//!   resolution equals the pin**, so a `PATH` probe equal to the pin never conceals an observed
//!   override. Stated as launch observation plus queried identity, not as execution proof.
//! - **Cached** (FR-012-7d (d)): the checks whose own units were all `Fresh`. A cached run
//!   prints this and no observation notice, because no observation exists.
//!
//! Nothing is printed when the pin resolved, a launch was observed, and the two agree.

use super::{Identity, SelectedBy};

/// The resolution notice (FR-012-8 (1)), or `None` when the resolution is the pin.
///
/// `pinned` is the channel the project's `rust-toolchain.toml` names; `None` for a legacy tree.
#[must_use]
pub fn resolution(
    resolved: &Identity,
    selected_by: SelectedBy,
    pinned: Option<&str>,
) -> Option<String> {
    let release = &resolved.release;
    let attribution = selected_by.as_str();
    match (pinned, selected_by) {
        (None, _) => Some(format!(
            "toolchain resolved before verification: rustc {release} ({attribution}); the \
             project pins nothing"
        )),
        (Some(channel), SelectedBy::NoRustup) => Some(format!(
            "toolchain resolved before verification: rustc {release} ({attribution}); the \
             project pins {channel}, which cannot be selected here"
        )),
        (Some(channel), SelectedBy::Unknown) => Some(format!(
            "toolchain resolved before verification: rustc {release} ({attribution}); the \
             project pins {channel}"
        )),
        (Some(channel), _) if channel == release => None,
        (Some(channel), _) => Some(format!(
            "toolchain resolved before verification: rustc {release} ({attribution}); the \
             project pins {channel}"
        )),
    }
}

/// The observation notice (FR-012-8 (2)): `Some` when a launch was observed and its queried
/// identity differs from the resolution in release or commit; `None` when nothing was observed
/// (a cached run) or the two agree.
#[must_use]
pub fn observation(observed: Option<&Identity>, resolved: &Identity) -> Option<String> {
    let observed = observed?;
    if observed.release == resolved.release && observed.commit == resolved.commit {
        return None;
    }
    Some(format!(
        "verification launched rustc {} ({}), not the resolved rustc {} ({}): launch \
         observation plus queried identity",
        observed.release, observed.commit, resolved.release, resolved.commit
    ))
}

/// The cached-artifacts line (FR-012-7d (d)) for the checks whose own units were all `Fresh`,
/// or `None` when there is none.
#[must_use]
pub fn cached(checks: &[&str]) -> Option<String> {
    if checks.is_empty() {
        return None;
    }
    Some(format!(
        "verification reused cached artifacts for {}: no compiler launch observed",
        checks.join(", ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(release: &str, commit: &str) -> Identity {
        Identity {
            release: release.to_owned(),
            commit: commit.to_owned(),
            host: "aarch64-apple-darwin".to_owned(),
        }
    }

    #[test]
    fn nothing_is_printed_when_resolution_and_observation_both_equal_the_pin() {
        let pin = identity("1.94.0", "4a4ef493e3a1488c6e321570238084b38948f6db");
        assert_eq!(
            resolution(&pin, SelectedBy::ToolchainFile, Some("1.94.0")),
            None,
            "the pin resolved by the file is the clean machine"
        );
        assert_eq!(
            resolution(&pin, SelectedBy::Environment, Some("1.94.0")),
            None,
            "an environment selection of the pinned release is still the pin"
        );
        assert_eq!(
            observation(Some(&pin), &pin),
            None,
            "an observation equal to the resolution says nothing"
        );
        assert_eq!(cached(&[]), None, "no cached check, no line");
    }

    #[test]
    fn the_resolution_notice_is_pinned_for_every_attribution() {
        let other = identity("1.97.1", "8bab26f4f68e0e26f0bb7960be334d5b520ea452");
        let cases: [(SelectedBy, &str); 4] = [
            (
                SelectedBy::Environment,
                "toolchain resolved before verification: rustc 1.97.1 (environment); the \
                 project pins 1.94.0",
            ),
            (
                SelectedBy::DirectoryOverride,
                "toolchain resolved before verification: rustc 1.97.1 (directory_override); the \
                 project pins 1.94.0",
            ),
            (
                SelectedBy::Default,
                "toolchain resolved before verification: rustc 1.97.1 (default); the project \
                 pins 1.94.0",
            ),
            (
                SelectedBy::ToolchainFile,
                "toolchain resolved before verification: rustc 1.97.1 (toolchain_file); the \
                 project pins 1.94.0",
            ),
        ];
        for (index, (selected_by, expected)) in cases.iter().enumerate() {
            let line = resolution(&other, *selected_by, Some("1.94.0"));
            assert_eq!(line.as_deref(), Some(*expected), "case {index}");
        }
    }

    #[test]
    fn a_bare_toolchain_says_the_pin_cannot_be_selected_here() {
        let bare = identity("1.94.0", "unknown");
        assert_eq!(
            resolution(&bare, SelectedBy::NoRustup, Some("1.94.0")).as_deref(),
            Some(
                "toolchain resolved before verification: rustc 1.94.0 (no_rustup); the project \
                 pins 1.94.0, which cannot be selected here"
            ),
            "even a bare compiler at the pinned release is said: the file is inert there"
        );
    }

    #[test]
    fn an_unknown_attribution_is_said_even_at_the_pinned_release() {
        let pin = identity("1.94.0", "4a4ef493e3a1488c6e321570238084b38948f6db");
        assert_eq!(
            resolution(&pin, SelectedBy::Unknown, Some("1.94.0")).as_deref(),
            Some(
                "toolchain resolved before verification: rustc 1.94.0 (unknown); the project \
                 pins 1.94.0"
            ),
            "text the attribution table does not know is not the clean machine's selection"
        );
    }

    #[test]
    fn a_legacy_tree_pins_nothing() {
        let any = identity("1.97.1", "8bab26f4f68e0e26f0bb7960be334d5b520ea452");
        assert_eq!(
            resolution(&any, SelectedBy::Default, None).as_deref(),
            Some(
                "toolchain resolved before verification: rustc 1.97.1 (default); the project \
                 pins nothing"
            ),
            "a legacy tree is always said"
        );
        assert_eq!(
            resolution(&any, SelectedBy::NoRustup, None).as_deref(),
            Some(
                "toolchain resolved before verification: rustc 1.97.1 (no_rustup); the project \
                 pins nothing"
            ),
            "a legacy tree on a bare compiler is said as pinning nothing"
        );
    }

    #[test]
    fn an_observed_identity_that_differs_from_the_resolution_is_printed_even_when_the_resolution_is_the_pin()
     {
        // A-8: a `PATH` probe equal to the pin never conceals an observed override.
        let pin = identity("1.94.0", "4a4ef493e3a1488c6e321570238084b38948f6db");
        let observed = identity("1.95.0", "59807616e1fa2540724bfbac14d7976d7e4a3860");
        assert_eq!(
            resolution(&pin, SelectedBy::ToolchainFile, Some("1.94.0")),
            None,
            "the resolution is the pin"
        );
        assert_eq!(
            observation(Some(&observed), &pin).as_deref(),
            Some(
                "verification launched rustc 1.95.0 (59807616e1fa2540724bfbac14d7976d7e4a3860), \
                 not the resolved rustc 1.94.0 (4a4ef493e3a1488c6e321570238084b38948f6db): launch \
                 observation plus queried identity"
            ),
            "the observation is said on its own"
        );
        // A commit difference alone is a difference: the same release from another build.
        let rebuilt = identity("1.94.0", "0000000deadbeef");
        assert!(
            observation(Some(&rebuilt), &pin).is_some(),
            "a differing commit at the same release is printed"
        );
    }

    #[test]
    fn a_cached_run_prints_the_cached_line_and_no_observation_notice() {
        let pin = identity("1.94.0", "4a4ef493e3a1488c6e321570238084b38948f6db");
        assert_eq!(
            observation(None, &pin),
            None,
            "no observation exists, so no observation notice"
        );
        assert_eq!(
            cached(&["clippy", "build", "test"]).as_deref(),
            Some(
                "verification reused cached artifacts for clippy, build, test: no compiler \
                 launch observed"
            ),
            "the checks are named in order"
        );
        assert_eq!(
            cached(&["build"]).as_deref(),
            Some("verification reused cached artifacts for build: no compiler launch observed"),
            "one check, no comma"
        );
    }
}
