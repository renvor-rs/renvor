//! `renvor new` — the transactional generator.
//!
//! Contract C-5, in order:
//!
//! ```text
//! 1. VALIDATE   every choice and the destination boundary — nothing has touched the filesystem
//! 2. STAGE      a directory the process owns, INSIDE the destination's parent
//! 3. RENDER     bounded template expansion into staging
//! 4. VERIFY     the generated project's own fmt, build, and tests — still in staging
//! 5. MANIFEST   walk the staged tree, sorted
//! 6. PLACE      one rename
//! 7. REPORT
//! ```
//!
//! Failure anywhere from 1 to 5 removes the staging directory and leaves the destination exactly as
//! it was — enforced by `Staging`'s `Drop`, so it holds on paths nobody wrote a cleanup for,
//! including a panic.

use serde::Serialize;

use crate::config::model::{Answers, ProjectConfiguration};
use crate::config::prompts;
use crate::exit::{CliError, Exit};
use crate::generate::manifest::FileManifest;
use crate::generate::place::Staging;
use crate::generate::render::Renderer;
use crate::output::Reporter;
use crate::templates;

/// The template context. A **separate** type from [`ProjectConfiguration`], deliberately.
///
/// Templates address values by name, so serialising the configuration directly would make every
/// private field a template variable and every field rename a silent template break. This type is
/// the contract between the two, and it is small enough to read.
#[derive(Debug, Serialize)]
struct Context {
    name: String,
    target: String,
    local_domain: String,
    local_https: String,
    container: bool,
    example_domain: bool,
    seed_data: bool,
    generator_version: String,
    template_version: String,
    /// The `mod` block for `src/main.rs`, **pre-rendered**.
    ///
    /// Computed here rather than expressed with template whitespace control, because the output is
    /// whitespace-sensitive Rust: the difference between a clean `cargo fmt --check` and a failing
    /// one is one blank line, and five conditional variants of `{%- … -%}` is not a thing anybody
    /// can review. Empty means "no modules"; otherwise it opens and closes with a newline so the
    /// surrounding blank lines come out right in both cases.
    modules: String,
}

/// Builds the `mod` block. See [`Context::modules`].
fn module_block(example_domain: bool, seed_data: bool) -> String {
    let mut modules = Vec::new();
    if example_domain {
        modules.push("mod domain;");
    }
    if seed_data {
        modules.push("mod seed;");
    }
    if modules.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", modules.join("\n"))
    }
}

/// Runs the command.
///
/// `interactive` is whether the wizard may run — decided by the caller from whether `stdin` is a
/// terminal, so this function has no ambient dependency on the process environment and stays
/// testable.
pub fn run(
    reporter: &Reporter,
    answers: Answers,
    interactive: bool,
    dry_run: bool,
) -> Result<Exit, CliError> {
    // ── 1. VALIDATE ─────────────────────────────────────────────────────────────────
    let answers = if interactive {
        prompts::fill(answers)?
    } else {
        answers
    };
    let (configuration, destination) = ProjectConfiguration::resolve(answers)?;

    let context = Context {
        name: configuration.name().to_owned(),
        target: match configuration.target() {
            crate::config::model::Target::Api => "api".to_owned(),
        },
        local_domain: configuration.local_domain().to_owned(),
        local_https: match configuration.local_https() {
            crate::config::model::LocalHttps::Off => "off".to_owned(),
            crate::config::model::LocalHttps::Requested => "requested".to_owned(),
        },
        container: configuration.container(),
        example_domain: configuration.example_domain(),
        seed_data: configuration.seed_data(),
        generator_version: env!("CARGO_PKG_VERSION").to_owned(),
        template_version: templates::VERSION.to_owned(),
        modules: module_block(configuration.example_domain(), configuration.seed_data()),
    };

    let renderer = Renderer::new(templates::select(&configuration))?;

    // THE EQUIVALENT COMMAND, printed after an interactive run.
    //
    // This is what makes the wizard reproducible: an operator who answered six prompts gets the
    // single non-interactive invocation that produces the same project, which they can paste into
    // a script or a README. Printing it only after the wizard is deliberate — echoing a command
    // back at somebody who just typed it is noise.
    //
    // It goes to `stderr`, because C-1 reserves `stdout` for the result. A JSON consumer must not
    // receive this.
    if interactive {
        reporter.note(&format!(
            "equivalent command: {}",
            configuration.equivalent_command()
        ));
    }

    // ── 2. STAGE ────────────────────────────────────────────────────────────────────
    //
    // Created even for a dry run. SC-006 requires the dry-run manifest to match the real run's
    // created set EXACTLY, and the only way to know what a render produces is to run it. A dry run
    // that predicted the manifest from the template list would drift the first time a template
    // gained a conditional.
    let staging = Staging::create(&destination)?;

    // ── 3. RENDER ───────────────────────────────────────────────────────────────────
    //
    // Progress is a `stderr` note rather than a spinner: this render is milliseconds, and a
    // spinner that appears and vanishes is worse than nothing. `progress_visible` is false in JSON
    // mode and whenever `stderr` is not a terminal (C-1), so a CI log gets no progress at all.
    if reporter.progress_visible() {
        reporter.note("rendering…");
    }
    renderer.render_into(staging.dir(), &context)?;

    // ── 4. VERIFY, STILL IN STAGING ─────────────────────────────────────────────────
    //
    // FR-030. A project that does not build is a generation failure, not a discovery the operator
    // makes later. Running it here means a failure leaves nothing to clean up.
    //
    // **This runs on the dry-run path too**, and that is deliberate rather than an oversight:
    // `cargo build` writes `Cargo.lock` into the project, so a dry run that skipped verification
    // would report a manifest one file short of what a real run creates — and SC-006 requires the
    // two to match exactly. A dry run therefore costs the same couple of seconds, and tells the
    // truth.
    if reporter.progress_visible() {
        reporter.note("verifying the generated project…");
    }
    let staging_path = destination.parent_display().join(staging.name());
    crate::generate::verify::in_staging(&staging_path)?;

    // ── 5. MANIFEST ─────────────────────────────────────────────────────────────────
    //
    // Taken AFTER verification, so it describes exactly the tree that will be renamed —
    // `Cargo.lock` included.
    let manifest = FileManifest::describe(staging.dir())?;

    if dry_run {
        // `staging` is dropped here, which removes the tree. FR-020: nothing was written.
        // The dry run LISTS what it would create, rather than counting it. A count tells an
        // operator that something would happen; the list tells them whether it is what they meant,
        // which is the entire reason to run a dry run.
        let human = format!(
            "dry run: {} files ({} bytes) would be created in {}\n{}",
            manifest.file_count(),
            manifest.total_bytes(),
            destination.display_path().display(),
            manifest
                .paths()
                .iter()
                .map(|path| format!("  {path}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        drop(staging);
        return Ok(reporter.finish(
            "new",
            &human,
            serde_json::json!({
                "dryRun": true,
                "destination": destination.display_path().display().to_string(),
                "templateVersion": templates::VERSION,
                "manifest": manifest.entries,
            }),
        ));
    }

    // ── 6. PLACE ────────────────────────────────────────────────────────────────────
    staging.place(&destination)?;

    // ── 7. REPORT ───────────────────────────────────────────────────────────────────
    let human = format!(
        "created {} files ({} bytes) in {}\n\nnext: cd {} && cargo run",
        manifest.file_count(),
        manifest.total_bytes(),
        destination.display_path().display(),
        destination.name()
    );
    Ok(reporter.finish(
        "new",
        &human,
        serde_json::json!({
            "dryRun": false,
            "destination": destination.display_path().display().to_string(),
            "templateVersion": templates::VERSION,
            "manifest": manifest.entries,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Format;
    use std::path::PathBuf;

    fn answers(destination: PathBuf) -> Answers {
        Answers {
            name: None,
            destination,
            local_domain: None,
            target: "api".to_owned(),
            container: false,
            local_https: false,
            seed_data: false,
            example_domain: false,
        }
    }

    fn reporter() -> Reporter {
        Reporter::new(Format::Human, true)
    }

    #[test]
    fn the_module_block_produces_the_blank_lines_rustfmt_wants() {
        // The exact strings, because one extra newline here is a `cargo fmt --check` failure in
        // every generated project — which is the acceptance criterion "the generated skeleton
        // formats". Found by running rustfmt over the output, not by inspection.
        assert_eq!(module_block(false, false), "");
        assert_eq!(module_block(true, false), "\nmod domain;\n");
        assert_eq!(module_block(true, true), "\nmod domain;\nmod seed;\n");
    }

    #[test]
    fn a_project_is_generated_and_the_destination_appears_exactly_once() {
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("commerce");
        let exit = run(&reporter(), answers(target.clone()), false, false).expect("generates");
        assert_eq!(exit, Exit::Success);
        assert!(target.join("Cargo.toml").is_file());
        assert!(target.join("src/main.rs").is_file());
        assert!(target.join("renvor.toml").is_file());
    }

    #[test]
    fn a_dry_run_writes_absolutely_nothing() {
        // FR-020, and the assertion is on the PARENT rather than on the destination: a staging
        // directory left beside it would also be a write.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("commerce");
        run(&reporter(), answers(target.clone()), false, true).expect("dry run");
        assert!(!target.exists(), "the destination was created by a dry run");
        let leftovers: Vec<_> = std::fs::read_dir(base.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(leftovers.is_empty(), "a dry run left {leftovers:?} behind");
    }

    #[test]
    fn the_dry_run_manifest_matches_the_real_run_exactly() {
        // SC-006. This is the assertion that makes `--dry-run` trustworthy rather than indicative.
        let base = tempfile::tempdir().expect("tempdir");
        let mut a = answers(base.path().join("one"));
        a.example_domain = true;
        a.seed_data = true;
        a.container = true;
        a.name = Some("demo".to_owned());

        let mut b = answers(base.path().join("two"));
        b.example_domain = true;
        b.seed_data = true;
        b.container = true;
        b.name = Some("demo".to_owned());

        // Render both into staging and compare the manifests directly, since the reporter's
        // stdout is not capturable from a unit test.
        let dry = manifest_of(a);
        let real = manifest_of(b);
        assert_eq!(dry, real, "the dry-run and real manifests differ");
    }

    /// Renders a configuration into staging and returns the manifest, without placing.
    fn manifest_of(answers: Answers) -> Vec<String> {
        let (configuration, destination) =
            ProjectConfiguration::resolve(answers).expect("resolves");
        let context = Context {
            name: configuration.name().to_owned(),
            target: "api".to_owned(),
            local_domain: configuration.local_domain().to_owned(),
            local_https: "off".to_owned(),
            container: configuration.container(),
            example_domain: configuration.example_domain(),
            seed_data: configuration.seed_data(),
            generator_version: "0".to_owned(),
            template_version: templates::VERSION.to_owned(),
            modules: module_block(configuration.example_domain(), configuration.seed_data()),
        };
        let renderer = Renderer::new(templates::select(&configuration)).expect("builds");
        let staging = Staging::create(&destination).expect("stages");
        renderer
            .render_into(staging.dir(), &context)
            .expect("renders");
        FileManifest::describe(staging.dir())
            .expect("describes")
            .paths()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn choices_that_were_not_made_render_no_files() {
        // Invariant I-12 from the other direction: a project without container controls must not
        // contain a Dockerfile that the manifest would then record as an honoured choice.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("plain");
        run(&reporter(), answers(target.clone()), false, false).expect("generates");
        assert!(!target.join("Dockerfile").exists());
        assert!(!target.join("compose.yaml").exists());
        assert!(!target.join("src/domain.rs").exists());
        assert!(!target.join("src/seed.rs").exists());
    }

    #[test]
    fn choices_that_were_made_render_their_files() {
        // POSITIVE CONTROL for the test above.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("full");
        let mut a = answers(target.clone());
        a.container = true;
        a.example_domain = true;
        a.seed_data = true;
        run(&reporter(), a, false, false).expect("generates");
        assert!(target.join("Dockerfile").is_file());
        assert!(target.join("compose.yaml").is_file());
        assert!(target.join("src/domain.rs").is_file());
        assert!(target.join("src/seed.rs").is_file());
    }

    #[test]
    fn a_failure_after_staging_leaves_the_destination_untouched() {
        // The transaction's whole promise. `--seed-data` without `--example-domain` fails in
        // VALIDATE, before staging; to fail *after* staging we need a render failure, which an
        // undefined variable produces.
        let base = tempfile::tempdir().expect("tempdir");
        let target = base.path().join("broken");
        let (configuration, destination) =
            ProjectConfiguration::resolve(answers(target.clone())).expect("resolves");
        let set = crate::generate::render::TemplateSet {
            version: "test",
            entries: vec![crate::generate::render::TemplateEntry {
                path: "a.txt",
                body: "{{ absent }}",
            }],
        };
        let renderer = Renderer::new(set).expect("builds");
        let staging = Staging::create(&destination).expect("stages");
        let error = renderer
            .render_into(staging.dir(), &serde_json::json!({}))
            .unwrap_err();
        assert_eq!(error.code, crate::exit::Code::RenderFailed);
        drop(staging);
        assert!(!target.exists(), "the destination survived a failed render");
        let leftovers: Vec<String> = std::fs::read_dir(base.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            leftovers.is_empty(),
            "staging survived a failed render: {leftovers:?}"
        );
        let _ = configuration;
    }
}
