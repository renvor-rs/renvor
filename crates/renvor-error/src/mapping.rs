//! The explicit relationship between the **public API** error registry and the **command-line**
//! error registry.
//!
//! # Why this module exists rather than an assumption
//!
//! Both registries are closed sets of `lower_snake_case` names. That resemblance is the hazard:
//! a reader who meets `internal` in one and `internal_error` in the other will reasonably assume
//! they are the same vocabulary spelled two ways, and they are not.
//!
//! FR-011 requires the relationship to be **stated**. This module states it, and asserts it — the
//! mirror below is checked against `renvor-cli`'s own registry by a guard test in that crate, for
//! the same reason and by the same mechanism its route-dump flag constant is guarded.
//!
//! # The relationship, stated plainly
//!
//! **The two vocabularies are disjoint. No name appears in both.**
//!
//! They describe different surfaces with different consumers, different lifetimes, and different
//! versioning:
//!
//! | | Public API registry | Command-line registry |
//! |---|---|---|
//! | Consumer | an HTTP client, over the network | a shell, a script, a CI job |
//! | Carried by | RFC 9457 `code` extension | the C-2 envelope's `error.code` |
//! | Accompanied by | an HTTP status | a process exit code |
//! | Versioned by | [`crate::code::API_ERROR_REGISTRY_VERSION`] | the envelope's `schemaVersion` |
//! | Changes when | a public route's failure vocabulary changes | a command's failure vocabulary changes |
//!
//! Sharing one version integer would mean a code added for `renvor doctor` bumped a version REST
//! consumers pin. Sharing one *vocabulary* would be worse: it would tie an HTTP response shape to a
//! shell exit convention, so neither could change without the other.
//!
//! # The near-misses, named
//!
//! Two pairs read as if they were the same thing. Neither is:
//!
//! | Command line | Public API | Why they are different |
//! |---|---|---|
//! | `internal` (exit 1) | `internal_error` (500) | The first says **the `renvor` tool** has a defect. The second says **the application being served** has one. An operator debugging the first reads Renvor's issue tracker; an operator debugging the second reads their own logs. |
//! | `bound_exceeded` (exit 3) | `payload_too_large` (413) | The first is a bound **the CLI** applied to something it read. The second is a bound **the server** applied to a request it received. |
//!
//! # There is no conversion function, deliberately
//!
//! A `From` implementation between the two would be an invitation to use it, and every use would
//! be a place where a server-side failure vocabulary leaked into a CLI exit status or vice versa.
//! The relationship is documentation and an assertion of disjointness — not a bridge.

use crate::code::ApiErrorCode;

/// The command-line registry's code names, mirrored here for the disjointness assertion.
///
/// **Stated as literals on purpose.** `renvor-error` is a transport-independent library and must
/// not depend on `renvor-cli` — which is `publish = false` and could not be depended on by a
/// publishable crate in any case. The guard against drift is a test in `renvor-cli` that compares
/// its own registry against this array and fails if they differ, which is the same mechanism
/// `renvor-cli`'s `DUMP_FLAG` constant already uses to stay in step with `renvor-http`'s.
pub const CLI_ERROR_CODES: [&str; 20] = [
    "usage",
    "unsupported_value",
    "unsupported_combination",
    "reserved_for_later_phase",
    "invalid_project_name",
    "destination_exists",
    "destination_rejected",
    "destination_parent_missing",
    "manifest_invalid",
    "project_verification_failed",
    "cancelled",
    "tool_missing",
    "container_runtime_unavailable",
    "transport_not_wired",
    "container_controls_missing",
    "render_failed",
    "bound_exceeded",
    "staging_failed",
    "placement_failed",
    "internal",
];

/// Whether `name` belongs to the command-line registry.
#[must_use]
pub fn is_cli_error_code(name: &str) -> bool {
    CLI_ERROR_CODES.contains(&name)
}

/// Whether `name` belongs to the public API registry.
#[must_use]
pub fn is_api_error_code(name: &str) -> bool {
    ApiErrorCode::ALL.iter().any(|code| code.as_str() == name)
}

#[cfg(test)]
mod tests {
    use super::{CLI_ERROR_CODES, is_api_error_code, is_cli_error_code};
    use crate::code::ApiErrorCode;
    use std::collections::BTreeSet;

    #[test]
    fn the_two_registries_share_no_name() {
        let cli: BTreeSet<&str> = CLI_ERROR_CODES.iter().copied().collect();
        let api: BTreeSet<&str> = ApiErrorCode::ALL.iter().map(|c| c.as_str()).collect();

        let shared: Vec<&&str> = cli.intersection(&api).collect();
        assert!(
            shared.is_empty(),
            "the registries share {shared:?}; FR-011 requires them to be distinct, and a shared \
             name would make the two surfaces' versioning inseparable"
        );
    }

    #[test]
    fn the_near_misses_are_genuinely_different_names() {
        // If these ever became equal, the disjointness test above would catch it — but it would
        // report a set intersection rather than the specific confusion this documents.
        assert_ne!(ApiErrorCode::InternalError.as_str(), "internal");
        assert_eq!(ApiErrorCode::InternalError.as_str(), "internal_error");
        assert!(is_cli_error_code("internal"));
        assert!(!is_api_error_code("internal"));
        assert!(is_api_error_code("internal_error"));
        assert!(!is_cli_error_code("internal_error"));
    }

    #[test]
    fn the_mirrored_cli_registry_has_no_duplicates() {
        let unique: BTreeSet<&str> = CLI_ERROR_CODES.iter().copied().collect();
        assert_eq!(
            unique.len(),
            CLI_ERROR_CODES.len(),
            "the mirrored registry lists a name twice"
        );
    }

    #[test]
    fn membership_queries_discriminate() {
        // POSITIVE CONTROLS on both sides, so neither predicate can pass by always answering the
        // same thing.
        assert!(is_cli_error_code("destination_exists"));
        assert!(!is_cli_error_code("validation_failed"));
        assert!(is_api_error_code("validation_failed"));
        assert!(!is_api_error_code("destination_exists"));
        assert!(!is_cli_error_code("a_name_in_neither_registry"));
        assert!(!is_api_error_code("a_name_in_neither_registry"));
    }
}
