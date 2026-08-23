//! Every documented bound, with an over-bound case and a boundary case.
//!
//! # Only one of the five bounds is reachable from outside the process, and that is by design
//!
//! `renvor` declares five bounds. Four of them — template fuel, output file count, single-file
//! output bytes, total output bytes — guard **template rendering**, and every template is embedded
//! in the executable (contract C-4). There is no flag, file, or environment variable that puts a
//! template in front of the renderer, which is the same property that makes FR-040's
//! no-archive-capability assertion meaningful. So no external input can drive those four past their
//! limits, and an integration test that claimed to would be testing something it had smuggled in.
//!
//! They are covered instead as unit tests in `src/generate/render.rs`, each with an over-bound case
//! **and** a boundary case proving the limit is not off by one:
//!
//! | Bound | Over-bound | Boundary |
//! | --- | --- | --- |
//! | `template_fuel` | `work_beyond_the_expansion_budget_is_stopped_by_fuel_and_reported_as_a_bound` | `work_inside_the_expansion_budget_completes` |
//! | `output_file_count` | `more_output_files_than_the_limit_is_a_bound_not_a_write` | `exactly_the_file_limit_is_still_rendered` |
//! | `single_file_output_bytes` | `a_file_above_the_per_file_limit_is_a_bound_not_a_write` | `a_file_exactly_at_the_per_file_limit_is_still_written` |
//! | `total_output_bytes` | `more_total_output_than_the_limit_is_a_bound_not_a_write` | `exactly_the_total_output_limit_is_still_rendered` |
//!
//! A fifth bound, `RECURSION_DEPTH`, has **no reachable trigger**: `{% include %}` is not a
//! statement this build's MiniJinja understands, because `multi_template` and `macros` are off. It
//! is guarded by `the_recursion_bound_has_no_reachable_trigger_and_that_is_the_point`, which fails
//! if either feature is ever enabled.
//!
//! # `manifest_bytes` is the one an operator can aim at, so it is tested here
//!
//! `renvor check` takes a **directory from the command line** and reads the `renvor.toml` it finds.
//! That is an input the operator names, so its bound is reachable end to end — and until 2026-08-18
//! it did not exist: the manifest was read with a plain `read_to_string`, which is an
//! out-of-memory anybody can trigger by pointing `check` at a large file.

mod harness;

use harness::renvor;

#[test]
fn a_bound_exceeded_failure_carries_its_bound_and_limit_all_the_way_out() {
    // C-2's registry says `bound_exceeded` carries `details.bound` and `details.limit`. Every
    // bound in this program is unit-tested, and until now **none was verified end to end** — so
    // the registry's promise about the wire format rested on reading the code.
    //
    // The manifest-size bound is the one reachable from outside without a pathological template,
    // because `renvor check` reads a file the operator names.
    let base = tempfile::tempdir().expect("tempdir");
    std::fs::write(base.path().join("renvor.toml"), "x".repeat(70 * 1024)).expect("write");

    let (code, stdout, _) = renvor(&["check", ".", "--output", "json"], base.path(), &[]);
    assert_eq!(code, 3, "a bound violation is a validation failure");

    let document: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("one JSON document");
    assert_eq!(document["error"]["code"], "bound_exceeded");
    assert_eq!(
        document["error"]["details"]["bound"], "manifest_bytes",
        "the failure must name WHICH bound was hit"
    );
    assert_eq!(
        document["error"]["details"]["limit"], "65536",
        "the failure must name the ceiling, or the operator cannot act on it"
    );
}
#[test]
fn a_file_just_under_the_bound_is_still_read() {
    // POSITIVE CONTROL. A `check` that rejected every manifest would satisfy the test above and be
    // useless — and an off-by-one in the bound would be invisible without this.
    let base = tempfile::tempdir().expect("tempdir");
    // Valid TOML, comfortably under 64 KiB, padded with a long comment.
    let padding = "#".repeat(60 * 1024);
    let manifest = format!(
        "{padding}\n[renvor]\ngenerator_version = \"0\"\ntemplate_version = \"1\"\n\n[project]\nname = \"padded\"\ntarget = \"api\"\ntransport = \"rest\"\nlocal_domain = \"padded.test\"\ncontainer = false\nlocal_https = \"off\"\nexample_domain = false\nseed_data = false\n"
    );
    assert!(
        manifest.len() < 65_536,
        "the fixture must be under the bound to test it"
    );
    std::fs::write(base.path().join("renvor.toml"), &manifest).expect("write");

    let (code, _, stderr) = renvor(&["check", "."], base.path(), &[]);
    assert_eq!(
        code, 0,
        "a manifest under the bound was rejected:\n{stderr}"
    );
}
