//! The snapshot stability policy (Phase 011, FR-043; `template-contract.md` §"Snapshot stability
//! policy"): a generated tree's manifest — its sorted paths and digests — is pinned per template
//! version, and changes only together with a `templates::VERSION` bump.
//!
//! # What a failure here means
//!
//! The snapshot is named by the version. A template body edited without the version bumped
//! renders different bytes under the same version, so the same-named snapshot fails, and the
//! message below names the constant to bump. A version bump names a new snapshot, which insta
//! records on the first run with `INSTA_UPDATE` permitting it; CI runs with `INSTA_UPDATE=no` and
//! `INSTA_FORCE_PASS` unset, so a drift on a runner fails rather than rewrites, and
//! `cargo insta review` is the one path that accepts a change.
//!
//! # Skeletons only, and why
//!
//! The dependency-free skeleton generates in seconds on any machine, so every variant is pinned
//! here on every run. A starter's manifest depends on the framework checkout it is pointed at
//! (`Cargo.lock` is resolved, not rendered, and the contract excludes it) and costs a build; its
//! byte-identity is proven by `starter_matrix.rs` instead.

use std::path::Path;
use std::process::Command;

/// Every skeleton variant, by the flags that change what is rendered.
const VARIANTS: [(&str, &[&str]); 6] = [
    ("bare", &[]),
    ("domain", &["--example-domain"]),
    ("seeded", &["--example-domain", "--seed-data"]),
    (
        "container",
        &["--example-domain", "--seed-data", "--container"],
    ),
    ("postgres", &["--database", "postgres", "--example-domain"]),
    (
        "mysql-seaorm",
        &["--database", "mysql", "--orm", "seaorm", "--example-domain"],
    ),
];

fn dry_run(base: &Path, flags: &[&str]) -> serde_json::Value {
    let mut args = vec!["new", "demo", "--dry-run", "--output", "json", "--yes"];
    args.extend_from_slice(flags);
    let output = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(&args)
        .current_dir(base)
        .env("CARGO_TARGET_DIR", base.join(".target"))
        .output()
        .expect("renvor runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let document: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("not JSON:\n{stdout}"));
    assert_eq!(document["status"], "success", "{document}");
    document
}

#[test]
fn every_skeleton_manifest_is_pinned_per_template_version() {
    let base = tempfile::tempdir().expect("tempdir");
    for (variant, flags) in VARIANTS {
        let document = dry_run(base.path(), flags);
        let version = document["result"]["templateVersion"]
            .as_str()
            .expect("the template version");
        let manifest: Vec<serde_json::Value> = document["result"]["manifest"]
            .as_array()
            .expect("a manifest")
            .iter()
            .filter(|entry| entry["kind"] == "file")
            .map(|entry| {
                // `template-contract.md` §"Snapshot stability policy": `Cargo.lock` is resolved
                // rather than rendered and is excluded from the pinned digests, and the
                // provenance record lists the lockfile's digest, so it is excluded with it.
                // Both PATHS are pinned — a variant that stopped producing either would fail —
                // and a template drift still fails through the digest of the file that drifted.
                let pinned =
                    entry["path"] != "Cargo.lock" && entry["path"] != ".renvor/generated.toml";
                serde_json::json!({
                    "path": entry["path"],
                    "digest": if pinned { entry["digest"].clone() } else { serde_json::Value::Null },
                })
            })
            .collect();
        let name = format!("manifest-v{version}-{variant}");
        // Named so that a body edit under the SAME version meets the same snapshot and fails,
        // and so the failure says what to do about it.
        insta::with_settings!({
            description => format!(
                "Template version {version}, variant `{variant}`. A change here is a change to a \
                 generated tree: bump `templates::VERSION` in crates/renvor-cli/src/templates.rs \
                 and record the new snapshot with `cargo insta review`; do not overwrite this one."
            ),
            omit_expression => true,
        }, {
            insta::assert_json_snapshot!(name, manifest);
        });
    }
}
