//! Step 2 of C-C2: merge the decoded layers by precedence, attributing every key as it goes.
//!
//! # Merging trees rather than typed partials, and why that is still C-C2
//!
//! C-C2 objects to *merge-first-decode-last* for one reason, stated in the contract: it "resolves
//! a shape conflict by picking a winner **silently**". The order is normative because of what the
//! wrong order hides, not for its own sake.
//!
//! Here, **every source has already been decoded against the schema** by
//! [`crate::layer::decode`] before this function is called, so no type error can be discovered
//! during the merge — and a shape conflict is not resolved during it either: it **fails**, naming
//! both layers. What remains after the merge is assembling the result, which cannot silently
//! resolve anything because there is nothing left to resolve.
//!
//! Merging decoded trees rather than typed partials is what makes this generic. A typed merge
//! needs one line of code per field, which is why every crate that does it ships a derive macro.
//! Renvor ships neither, and pays for that in [`crate::schema`] instead.
//!
//! # Attribution is produced by the merge, not reconstructed after it
//!
//! Every time a leaf is written, the layer that wrote it is recorded. Reconstructing provenance
//! afterwards means keeping a second structure in step with the first — the failure mode ADR-0007
//! rejects — and it is exactly what the rejected candidates force. Here the map and the value are
//! written in the same statement.

use std::collections::BTreeMap;

use core::fmt;

use renvor_core::KernelError;
use renvor_core::config_port::{Attribution, Presence, SourceLayer};
use renvor_core::error::context::conflict;
use toml::{Table, Value};

/// One source, decoded and ready to merge.
#[derive(Clone)]
pub struct DecodedLayer {
    /// Which layer this is, for attribution and diagnostics.
    pub layer: SourceLayer,
    /// The source's contents.
    pub table: Table,
}

impl DecodedLayer {
    /// Pairs a decoded source with the layer it came from.
    #[must_use]
    pub const fn new(layer: SourceLayer, table: Table) -> Self {
        Self { layer, table }
    }
}

/// Emits the layer and the **number** of keys, never the keys' values.
///
/// Hand-written for the reason C-E3 gives: a decoded source is a configuration value, and **0**
/// output forms may emit one. This derived `Debug` until the W-005 verification re-review (finding
/// 1.1) printed it and got `table: {"password": String("hunter2-do-not-print"), ...}`.
///
/// The lesson is the shape of the first fix, not the miss itself. `ResolvedConfig` was hand-written
/// because a review named *that type*; the four siblings holding the same data were never looked
/// for. A redaction guarantee is a property of a **set** of types, and fixing the one that was
/// pointed at is not the same as establishing it.
impl fmt::Debug for DecodedLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecodedLayer")
            .field("layer", &self.layer)
            .field("keys", &self.table.len())
            .finish_non_exhaustive()
    }
}

/// The merged tree plus the layer that won each resolved key.
#[derive(Clone, Default)]
pub struct Merged {
    /// The merged configuration tree.
    pub table: Table,
    /// Which layer supplied each resolved key, keyed by dotted path.
    ///
    /// A `BTreeMap` so the reported order is the key order rather than a hash order that changes
    /// between runs — FR-016 asks for a report, and a report that reshuffles itself is harder to
    /// diff than one that does not.
    pub attribution: BTreeMap<String, Attribution>,
}

/// Emits key **counts** and the attribution map, never a value. Same reasoning as [`DecodedLayer`].
///
/// Attribution is safe and is the useful half: which layer won for which key is exactly what a
/// reader debugging configuration wants, and it never needed the value to learn it.
impl fmt::Debug for Merged {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Merged")
            .field("keys", &self.table.len())
            .field("attribution", &self.attribution)
            .finish_non_exhaustive()
    }
}

/// Merges layers in precedence order, lowest first.
///
/// # Errors
///
/// Returns [`KernelError::ConfigurationConflict`] naming the key and **both** layers when two
/// layers give the same key incompatible structural shapes (C-C5(c)). It is not coerced into a
/// common shape and not resolved by last-wins.
pub fn merge_layers(layers: &[DecodedLayer]) -> Result<Merged, KernelError> {
    let mut merged = Merged::default();
    let mut shapes: BTreeMap<String, (SourceLayer, &'static str)> = BTreeMap::new();

    for layer in layers {
        overlay(
            &mut merged.table,
            &layer.table,
            &layer.layer,
            "",
            &mut merged.attribution,
            &mut shapes,
        )?;
    }

    Ok(merged)
}

/// Writes one layer over the accumulator, recording attribution for every leaf it wins.
fn overlay(
    into: &mut Table,
    from: &Table,
    layer: &SourceLayer,
    prefix: &str,
    attribution: &mut BTreeMap<String, Attribution>,
    shapes: &mut BTreeMap<String, (SourceLayer, &'static str)>,
) -> Result<(), KernelError> {
    for (key, value) in from {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };

        let shape = shape_of(value);
        if let Some((first_layer, first_shape)) = shapes.get(&path) {
            if *first_shape != shape {
                return Err(conflict(
                    path,
                    first_layer.label(),
                    first_shape,
                    layer.label(),
                    shape,
                ));
            }
        } else {
            shapes.insert(path.clone(), (layer.clone(), shape));
        }

        match value {
            // (a) Tables merge per key, so a later layer overrides only the keys it sets and a
            // small override file need not restate a whole section.
            Value::Table(inner) => {
                let slot = into
                    .entry(key.clone())
                    .or_insert_with(|| Value::Table(Table::new()));
                let Some(existing) = slot.as_table_mut() else {
                    // Unreachable: the shape check above already refused a table-over-non-table.
                    // Reported rather than asserted (SC-004 permits 0 panics).
                    return Err(conflict(
                        path,
                        "an earlier layer",
                        shape_of(slot),
                        layer.label(),
                        "table",
                    ));
                };
                overlay(existing, inner, layer, &path, attribution, shapes)?;
            }
            // (b) Arrays replace wholesale. Element-wise merging would make it impossible to
            // *remove* an entry a lower layer set, so an override could only ever add.
            // Everything else is a scalar and simply replaces.
            other => {
                into.insert(key.clone(), other.clone());
                attribution.insert(
                    path,
                    Attribution {
                        layer: layer.clone(),
                        presence: presence_of(other),
                    },
                );
            }
        }
    }

    Ok(())
}

/// The structural shape of a value, for conflict reporting.
///
/// Integers and floats are both `number` on purpose: a layer supplying `1` where another supplied
/// `1.5` is not a *structural* conflict, and per-source decoding has already proved both fit the
/// declared type. Structure means table versus array versus scalar.
const fn shape_of(value: &Value) -> &'static str {
    match value {
        Value::Table(_) => "table",
        Value::Array(_) => "array",
        Value::String(_) => "string",
        Value::Integer(_) | Value::Float(_) => "number",
        Value::Boolean(_) => "boolean",
        Value::Datetime(_) => "datetime",
    }
}

/// Whether a supplied value has content.
///
/// C-C11 requires *present and empty* to stay distinguishable from *absent*. Collapsing them is
/// how `THREADS=""` silently became a default in the candidate crate.
fn presence_of(value: &Value) -> Presence {
    let empty = match value {
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Table(table) => table.is_empty(),
        _ => false,
    };
    if empty {
        Presence::PresentButEmpty
    } else {
        Presence::Present
    }
}

#[cfg(test)]
mod tests {
    use super::{DecodedLayer, merge_layers};
    use renvor_core::ErrorCategory;
    use renvor_core::config_port::{Presence, SourceLayer};
    use toml::Table;

    fn layer(name: &str, text: &str) -> DecodedLayer {
        let source = match name {
            "defaults" => SourceLayer::Defaults,
            "environment" => SourceLayer::Environment,
            other => SourceLayer::file(other),
        };
        DecodedLayer::new(source, text.parse::<Table>().expect("valid TOML fixture"))
    }

    #[test]
    fn a_later_layer_overrides_an_earlier_one() {
        // C-C4: defaults < earlier file < later file < environment.
        let merged = merge_layers(&[
            layer("defaults", "port = 1"),
            layer("base.toml", "port = 2"),
            layer("override.toml", "port = 3"),
            layer("environment", "port = 4"),
        ])
        .expect("no conflicts");

        assert_eq!(merged.table["port"].as_integer(), Some(4));
        assert_eq!(
            merged.attribution["port"].layer,
            SourceLayer::Environment,
            "the winning layer is the one attribution reports"
        );
    }

    #[test]
    fn tables_merge_per_key_so_siblings_survive() {
        // C-C5(a). An override that sets one key must not delete the section's other keys.
        let merged = merge_layers(&[
            layer("base.toml", "[server]\nhost = \"a\"\ntimeout_ms = 100"),
            layer("override.toml", "[server]\nthreads = 8"),
        ])
        .expect("no conflicts");

        let server = merged.table["server"].as_table().expect("a table");
        assert_eq!(server["host"].as_str(), Some("a"), "sibling survived");
        assert_eq!(server["timeout_ms"].as_integer(), Some(100));
        assert_eq!(server["threads"].as_integer(), Some(8));

        assert_eq!(merged.attribution["server.host"].layer.label(), "base.toml");
        assert_eq!(
            merged.attribution["server.threads"].layer.label(),
            "override.toml"
        );
    }

    #[test]
    fn arrays_replace_wholesale_and_are_never_concatenated() {
        // C-C5(b). Element-wise merging would make removing an entry impossible.
        let merged = merge_layers(&[
            layer("base.toml", "features = [\"a\", \"b\", \"c\"]"),
            layer("override.toml", "features = [\"z\"]"),
        ])
        .expect("no conflicts");

        let features = merged.table["features"].as_array().expect("an array");
        assert!(
            features.len() == 1,
            "arrays were concatenated rather than replaced"
        );
        assert_eq!(features[0].as_str(), Some("z"));
    }

    #[test]
    fn a_structural_conflict_fails_naming_both_layers() {
        // C-C5(c). Not coerced, not last-wins.
        let error = merge_layers(&[
            layer("base.toml", "[server]\nhost = \"a\""),
            layer("override.toml", "server = \"just-a-string\""),
        ])
        .expect_err("a table and a string are not reconcilable");

        assert_eq!(error.category(), ErrorCategory::ConfigurationConflict);
        let rendered = error.to_string();
        assert!(rendered.contains("server"), "the key is missing");
        assert!(rendered.contains("base.toml"), "the first layer is missing");
        assert!(
            rendered.contains("override.toml"),
            "the second layer is missing"
        );
        assert!(rendered.contains("table"), "the first shape is missing");
        assert!(rendered.contains("string"), "the second shape is missing");
    }

    #[test]
    fn compatible_shapes_across_layers_do_not_conflict() {
        // POSITIVE CONTROL for the test above: the conflict check discriminates rather than
        // rejecting every key that appears twice.
        let merged = merge_layers(&[
            layer("base.toml", "port = 1"),
            layer("override.toml", "port = 2"),
        ])
        .expect("two integers are the same shape");
        assert_eq!(merged.table["port"].as_integer(), Some(2));
    }

    #[test]
    fn present_but_empty_is_recorded_as_such() {
        // C-C11: distinguishable from absent, so an empty override never reads as "unset".
        let merged = merge_layers(&[layer("base.toml", "name = \"\"\ntags = []\nport = 8080")])
            .expect("no conflicts");

        assert_eq!(
            merged.attribution["name"].presence,
            Presence::PresentButEmpty
        );
        assert_eq!(
            merged.attribution["tags"].presence,
            Presence::PresentButEmpty
        );
        assert_eq!(merged.attribution["port"].presence, Presence::Present);
    }

    #[test]
    fn every_resolved_leaf_is_attributed() {
        // FR-016 says *every* resolved key, so this asserts the count rather than sampling.
        let merged = merge_layers(&[
            layer("defaults", "a = 1\n[nested]\nb = 2\nc = 3"),
            layer("environment", "a = 9"),
        ])
        .expect("no conflicts");

        let mut keys: Vec<&str> = merged.attribution.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["a", "nested.b", "nested.c"]);
        assert_eq!(merged.attribution["a"].layer, SourceLayer::Environment);
        assert_eq!(merged.attribution["nested.b"].layer, SourceLayer::Defaults);
    }

    #[test]
    fn merging_nothing_produces_nothing() {
        let merged = merge_layers(&[]).expect("an empty layer set is valid");
        assert!(merged.table.is_empty());
        assert!(merged.attribution.is_empty());
    }
}
