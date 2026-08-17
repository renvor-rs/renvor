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
use renvor_core::error::context::bounded_identifier;

/// Renders the attribution map as one line per key, in key order.
///
/// Keys and layers only. There is no value to omit, because the input carries none.
///
/// # Both halves of each line are bounded (T159)
///
/// The **layer** half was bounded by T146, through `SourceLayer::file`. The **key** half was not,
/// and this function is the one whose entire job is producing the report the T146 evidence cited as
/// the motivating case for bounding at all — so the gap sat exactly where the argument pointed.
/// Found by the round-four requirements delta review (R4-3, MAJOR) and measured: a 1,000,000-byte
/// key produced a 1,000,179-byte report, of which the 200 KB file path contributed about 179 bytes
/// because *that* half had been fixed.
///
/// This path needs no `deny_unknown_fields` to reach: a key the schema ignores still merges, and
/// still gets an attribution row. It is reachable in the default configuration.
#[must_use]
pub fn render(attribution: &[(String, Attribution)]) -> String {
    let mut lines: Vec<String> = attribution
        .iter()
        .map(|(key, at)| {
            format!(
                "{} <- {} ({:?})",
                bounded_identifier(key),
                at.layer.label(),
                at.presence
            )
        })
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

    #[test]
    fn a_gigantic_key_produces_a_bounded_report() {
        // R4-3. The layer half of each line was bounded by T146; the key half was not, and this is
        // the function the T146 evidence named as the reason the bound mattered. Measured before
        // the fix: a 1,000,000-byte key produced a 1,000,179-byte report.
        //
        // Two sizes an order of magnitude apart, because the property is "does not grow with the
        // input" rather than "is small".
        let mut lengths = Vec::new();
        for size in [100_000_usize, 1_000_000] {
            let rows = vec![(
                "k".repeat(size),
                Attribution {
                    layer: SourceLayer::file("x".repeat(200_000)),
                    presence: Presence::Present,
                },
            )];
            let rendered = render(&rows);
            assert!(
                rendered.len() < 2_048,
                "an oversized key and layer produced an over-long report"
            );
            lengths.push(rendered.len());
        }

        assert!(
            lengths[1].abs_diff(lengths[0]) <= 8,
            "the report grew when the key grew 10-fold"
        );
    }

    #[test]
    fn an_ordinary_report_is_unchanged() {
        // POSITIVE CONTROL: bounding must not rewrite the reports anyone actually reads. Every key
        // and layer here is short and must appear verbatim.
        let rendered = render(&sample());
        assert!(rendered.contains("server.port"), "{rendered}");
        assert!(
            !rendered.contains("truncated"),
            "an ordinary report was truncated: {rendered}"
        );
    }

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
