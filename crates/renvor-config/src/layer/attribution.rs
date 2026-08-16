//! Reporting which layer supplied each resolved key.
//!
//! # This module is thin on purpose
//!
//! Attribution is **produced by the merge**, not computed afterwards — see [`crate::layer::merge`].
//! What is left here is presentation: turning the map into something an operator can read, and
//! answering "which keys did this layer win?" without the caller writing the filter themselves.
//!
//! A module that recomputed provenance from the merged tree would be a second source of truth for
//! the same fact, and the two would eventually disagree. The rejected candidates force exactly
//! that, which is why obligation 4 was the one that failed.
//!
//! # Attribution is a disclosure surface
//!
//! A report naming every key and its layer tells a reader which file exists, which variables are
//! set, and how the deployment is structured. It names **keys and layers only, never values** —
//! and that is a property of what [`renvor_core::config_port::Attribution`] can hold, not a rule
//! this module follows.

use renvor_core::config_port::{Attribution, SourceLayer};

/// Renders the attribution map as one line per key, in key order.
///
/// Keys and layers only. There is no value to omit, because the input carries none.
#[must_use]
pub fn render(attribution: &[(String, Attribution)]) -> String {
    let mut lines: Vec<String> = attribution
        .iter()
        .map(|(key, at)| format!("{key} <- {} ({:?})", at.layer.label(), at.presence))
        .collect();
    lines.sort();
    lines.join("\n")
}

/// The keys a given layer won.
#[must_use]
pub fn keys_won_by<'a>(
    attribution: &'a [(String, Attribution)],
    layer: &SourceLayer,
) -> Vec<&'a str> {
    let mut keys: Vec<&str> = attribution
        .iter()
        .filter(|(_, at)| &at.layer == layer)
        .map(|(key, _)| key.as_str())
        .collect();
    keys.sort_unstable();
    keys
}

#[cfg(test)]
mod tests {
    use super::{keys_won_by, render};
    use renvor_core::config_port::{Attribution, Presence, SourceLayer};

    fn sample() -> Vec<(String, Attribution)> {
        vec![
            (
                "server.port".to_owned(),
                Attribution {
                    layer: SourceLayer::Environment,
                    presence: Presence::Present,
                },
            ),
            (
                "server.host".to_owned(),
                Attribution {
                    layer: SourceLayer::File("base.toml".into()),
                    presence: Presence::Present,
                },
            ),
        ]
    }

    #[test]
    fn a_report_names_keys_and_layers_and_carries_no_value() {
        let rendered = render(&sample());
        assert!(
            rendered.contains("server.port <- environment"),
            "{rendered}"
        );
        assert!(rendered.contains("server.host <- base.toml"), "{rendered}");

        // The input has nowhere to put a value, so this is a property of the type rather than of
        // the formatting. Asserted anyway: it is the claim a reader most wants checked.
        assert!(!rendered.contains("8080"));
    }

    #[test]
    fn keys_are_reported_per_layer() {
        assert_eq!(
            keys_won_by(&sample(), &SourceLayer::Environment),
            vec!["server.port"]
        );
        // POSITIVE CONTROL: a layer that won nothing reports nothing, so the filter discriminates.
        assert!(keys_won_by(&sample(), &SourceLayer::Defaults).is_empty());
    }
}
