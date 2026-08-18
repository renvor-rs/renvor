//! Bounded template expansion.
//!
//! Governs FR-024 to FR-028 and FR-040, and implements contract C-4.
//!
//! # The one design decision worth reading before the code
//!
//! MiniJinja offers two constructors, and the difference between them is the difference between a
//! deny-list and a deny-by-default. [`minijinja::Environment::new`] installs every built-in filter,
//! test, and global, and the application is then expected to *remove* the ones it does not want —
//! which means every MiniJinja release that adds a builtin silently widens this program's template
//! surface. [`minijinja::Environment::empty`] installs **none**, and the allow-list below is
//! additive.
//!
//! Constitution principle VI asks for deny-by-default. Here that is one function name, so this
//! module uses `empty()` and every capability a template has is named in [`Renderer::new`].
//!
//! # What is absent rather than disabled
//!
//! MiniJinja has no filesystem, process, or network capability of its own; a template can only
//! reach what the embedding application hands it. Nothing below hands it any of those, and no
//! loader is installed, so `{% include %}` and `{% extends %}` can only name a template that was
//! explicitly registered. FR-027 and FR-043 are satisfied by absence, not by a switch that a future
//! edit could flip.
//!
//! # Auto-escaping is off, deliberately
//!
//! `Environment::empty()` installs no auto-escape callback. That is correct here and would be wrong
//! elsewhere: this renders Rust, TOML, and Markdown, and HTML-escaping a Rust generic bound would
//! corrupt it. **If a template ever renders HTML, this decision has to be revisited**, which is why
//! it is stated here rather than left as a default nobody chose.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use cap_std::fs::Dir;
use minijinja::{Environment, UndefinedBehavior};

use crate::exit::{CliError, Code};

/// Every documented bound, in one place, with the reason for each value.
///
/// FR-026 requires bounded expansion and SC-013 requires a test that demonstrates each bound holds.
/// Constants live here rather than inline so that the test module can assert against the same
/// number the renderer uses — a test carrying its own copy of a limit proves nothing about the
/// limit the program enforces.
pub mod bounds {
    /// MiniJinja instructions per render.
    ///
    /// This is the bound that stops a runaway loop, and it only exists because the `fuel` feature
    /// is enabled explicitly in `Cargo.toml` — it is **not** in MiniJinja's default feature set, so
    /// a project that adopts the crate the obvious way has no expansion bound at all.
    ///
    /// One million is roughly three orders of magnitude above the largest template in the shipped
    /// set, which leaves room for a legitimately large project without leaving room for an
    /// unbounded loop.
    pub const FUEL: u64 = 1_000_000;

    /// Template recursion depth. MiniJinja's own default is 500; a project skeleton needs single
    /// digits, and the gap between "enough" and "the library's ceiling" is where a stack blowout
    /// lives.
    pub const RECURSION_DEPTH: usize = 16;

    /// Files one render may produce. A skeleton is tens of files.
    pub const MAX_FILES: usize = 512;

    /// Bytes any single rendered file may reach.
    pub const MAX_FILE_BYTES: usize = 1_048_576;

    /// Bytes the whole render may reach. Checked incrementally, so a render that would exceed it
    /// stops at the file that crosses the line rather than after writing everything.
    pub const MAX_TOTAL_BYTES: usize = 8_388_608;
}

/// One file a template set produces.
///
/// # Output paths are literal, and that is the security property
///
/// `path` is **not** itself a template. Contract C-4 requires that an entry whose output path would
/// escape the staging root be a *load-time* error, "so such an entry cannot exist in a shipped
/// binary" — and that is only achievable if the path does not depend on runtime input. A templated
/// path could be made to escape by a context value, which would move the check to render time and
/// make the contract's own claim false.
///
/// The cost is that a file whose name varies with configuration needs an entry per variant. That is
/// a real cost, and it is the one being paid deliberately.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateEntry {
    /// Relative output path, forward-slashed, literal.
    pub path: &'static str,
    /// The template body.
    pub body: &'static str,
}

/// A versioned, embedded, inert set of templates.
///
/// # Why `entries` is a `Vec` of `Copy` entries rather than a `&'static` slice
///
/// The **catalogue** is static; the **selection** is not. A configuration without container
/// controls must not render a `Dockerfile`, so the set handed to a [`Renderer`] is chosen at
/// runtime from the catalogue. Each entry is still two `&'static str`s, so the `Vec` allocates
/// pointers and copies nothing.
///
/// The load-time guarantee is preserved by [`crate::templates`]'s own test, which validates the
/// **whole catalogue** rather than whichever subset a run selected — so a malformed entry fails the
/// build, not a user's generation.
#[derive(Debug, Clone)]
pub struct TemplateSet {
    /// Recorded in every generated `renvor.toml` (FR-024).
    pub version: &'static str,
    /// The entries, in declaration order.
    pub entries: Vec<TemplateEntry>,
}

impl TemplateSet {
    /// Validates every entry path before anything is rendered.
    ///
    /// This is invariant I-5. It runs once, over data that is compiled into the binary, so a set
    /// that fails here cannot be used at all — and the test module runs it over the shipped set, so
    /// a bad entry fails the build rather than a user's generation.
    ///
    /// # Errors
    ///
    /// [`Code::Internal`], because a malformed embedded template set is a **defect in this program**
    /// rather than anything the operator did. Exit `1` is the correct signal: it means "report this
    /// as a bug", which is exactly right.
    pub fn validate(&self) -> Result<(), CliError> {
        let defect = |path: &str, why: &str| {
            CliError::new(
                Code::Internal,
                format!(
                    "embedded template entry `{path}` {why}; this is a defect in renvor itself"
                ),
            )
            .with("entry", path.to_owned())
        };

        let mut seen = BTreeSet::new();
        for entry in &self.entries {
            if entry.path.is_empty() {
                return Err(defect(entry.path, "has an empty output path"));
            }
            if entry.path.contains('\\') {
                return Err(defect(
                    entry.path,
                    "uses a backslash; entry paths are forward-slashed",
                ));
            }
            let candidate = Path::new(entry.path);
            if candidate.is_absolute() {
                return Err(defect(entry.path, "is absolute"));
            }
            for component in candidate.components() {
                match component {
                    Component::Normal(_) => {}
                    Component::CurDir => {
                        return Err(defect(entry.path, "contains a `.` component"));
                    }
                    Component::ParentDir => {
                        return Err(defect(entry.path, "contains a `..` component"));
                    }
                    Component::RootDir | Component::Prefix(_) => {
                        return Err(defect(entry.path, "is rooted"));
                    }
                }
            }
            if !seen.insert(entry.path) {
                return Err(defect(
                    entry.path,
                    "appears twice; the later one would overwrite the earlier",
                ));
            }
        }
        Ok(())
    }
}

/// The rendering environment, built once with every capability named.
pub struct Renderer<'a> {
    environment: Environment<'a>,
    set: TemplateSet,
}

impl std::fmt::Debug for Renderer<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `Environment` is not `Debug`, and dumping it would print template bodies into any log
        // that formats a `Renderer`. Naming the version is the useful half anyway.
        formatter
            .debug_struct("Renderer")
            .field("template_version", &self.set.version)
            .field("entries", &self.set.entries.len())
            .finish()
    }
}

impl<'a> Renderer<'a> {
    /// Builds the environment and registers every template.
    ///
    /// # Errors
    ///
    /// [`Code::Internal`] if the set fails [`TemplateSet::validate`] or a body fails to compile —
    /// both are defects in this program, not operator errors.
    pub fn new(set: TemplateSet) -> Result<Self, CliError> {
        set.validate()?;

        let mut environment = Environment::empty();

        // FR-028. `Strict` fails on printing, iterating, attribute access, and truthiness of an
        // undefined value. `SemiStrict` would allow `{% if missing %}` to quietly take the false
        // branch, which is exactly the silent-empty-rendering failure FR-028 exists to prevent.
        environment.set_undefined_behavior(UndefinedBehavior::Strict);

        // FR-026, the expansion bound. Requires the `fuel` feature, which is why `Cargo.toml`
        // enables it explicitly.
        environment.set_fuel(Some(bounds::FUEL));
        environment.set_recursion_limit(bounds::RECURSION_DEPTH);

        // KEEP THE FILE'S FINAL NEWLINE.
        //
        // MiniJinja defaults this to `false`, which strips the trailing newline from every rendered
        // template. For a web page that is invisible; for a source file it means the generated
        // project fails `cargo fmt --check` on its first run — which is exactly the "generated
        // skeleton formats, compiles, tests, and starts" acceptance criterion.
        //
        // Found by running that criterion, not by reading the documentation.
        environment.set_keep_trailing_newline(true);

        // NO `set_debug` CALL, AND THAT IS THE POINT.
        //
        // MiniJinja's debug mode attaches the template source and the surrounding variables to
        // every error. Those variables are the project configuration, and FR-018 requires that no
        // secret reach any output mode.
        //
        // `set_debug` does not exist here: `debug` is a **Cargo feature**, and `Cargo.toml`
        // declares `default-features = false` with `builtins`, `fuel`, and `serde` only. So the
        // machinery is not compiled into the binary rather than switched off inside it — absent
        // rather than disabled, which is the same distinction this module's header draws about
        // filesystem and network access. A later edit cannot flip a switch that is not there; it
        // would have to change a dependency declaration, which is visible in review.

        // THE COMPLETE ALLOW-LIST. `Environment::empty()` installed nothing; everything a template
        // can call is on the next few lines.
        //
        // Each is pure, total, and operates only on values already in the context. None can reach
        // the filesystem, spawn a process, or open a socket, because no such function is here to
        // be reached.
        environment.add_filter("upper", |value: String| value.to_uppercase());
        environment.add_filter("lower", |value: String| value.to_lowercase());
        environment.add_filter("replace", |value: String, from: String, to: String| {
            value.replace(&from, &to)
        });
        // `crate_name` is the one project-specific transform: Cargo package names allow `-`, Rust
        // identifiers do not, and a generated `use my-app::…` would not compile.
        environment.add_filter("crate_name", |value: String| value.replace('-', "_"));
        // `toml_bool` EXISTS BECAUSE OF A REAL DEFECT, not for symmetry.
        //
        // MiniJinja renders a boolean as `True` / `False` — Python's spelling. TOML's booleans are
        // lowercase, so `{{ container }}` in a `.toml` template produced `container = True`, which
        // is **invalid TOML**: every generated project that set any flag had a manifest that would
        // not parse, and `renvor check` would have rejected renvor's own output.
        //
        // Found by an outside-in acceptance test, not by reading the rendered file — which is the
        // argument for having the test.
        environment.add_filter(
            "toml_bool",
            |value: bool| if value { "true" } else { "false" },
        );

        for entry in &set.entries {
            environment
                .add_template(entry.path, entry.body)
                .map_err(|error| {
                    CliError::new(
                        Code::Internal,
                        format!(
                            "embedded template `{}` does not compile: {error}; this is a defect in \
                         renvor itself",
                            entry.path
                        ),
                    )
                    .with("entry", entry.path.to_owned())
                })?;
        }

        Ok(Self { environment, set })
    }

    /// Renders every entry into `root`, enforcing every bound.
    ///
    /// `root` is the **staging** directory handle, never the destination and never a path. A bound
    /// violation or a render failure therefore leaves the destination untouched, because the
    /// destination has not been created yet — and no output path can reach it, because a `Dir`
    /// handle has no way to name anything outside itself.
    ///
    /// # Errors
    ///
    /// [`Code::BoundExceeded`] with `details.bound` and `details.limit` when a documented limit is
    /// reached, or [`Code::RenderFailed`] for a template or filesystem failure.
    pub fn render_into<S: serde::Serialize>(
        &self,
        root: &Dir,
        context: &S,
    ) -> Result<(), CliError> {
        let exceeded = |bound: &str, limit: usize, detail: String| {
            CliError::new(Code::BoundExceeded, detail)
                .with("bound", bound)
                .with("limit", limit.to_string())
        };

        if self.set.entries.len() > bounds::MAX_FILES {
            return Err(exceeded(
                "output_file_count",
                bounds::MAX_FILES,
                format!(
                    "the template set declares {} files, above the limit of {}",
                    self.set.entries.len(),
                    bounds::MAX_FILES
                ),
            ));
        }

        let mut total = 0usize;

        for entry in &self.set.entries {
            let template = self.environment.get_template(entry.path).map_err(|error| {
                CliError::new(
                    Code::Internal,
                    format!(
                        "template `{}` was registered and is now missing: {error}",
                        entry.path
                    ),
                )
            })?;

            let rendered = template
                .render(context)
                .map_err(|error| classify(entry.path, &error))?;

            if rendered.len() > bounds::MAX_FILE_BYTES {
                return Err(exceeded(
                    "single_file_output_bytes",
                    bounds::MAX_FILE_BYTES,
                    format!(
                        "`{}` rendered to {} bytes, above the per-file limit of {}",
                        entry.path,
                        rendered.len(),
                        bounds::MAX_FILE_BYTES
                    ),
                ));
            }

            total = total.saturating_add(rendered.len());
            if total > bounds::MAX_TOTAL_BYTES {
                return Err(exceeded(
                    "total_output_bytes",
                    bounds::MAX_TOTAL_BYTES,
                    format!(
                        "the render reached {total} bytes at `{}`, above the total limit of {}",
                        entry.path,
                        bounds::MAX_TOTAL_BYTES
                    ),
                ));
            }

            if let Some(parent) = Path::new(entry.path).parent()
                && !parent.as_os_str().is_empty()
            {
                root.create_dir_all(parent).map_err(|error| {
                    CliError::new(
                        Code::RenderFailed,
                        format!(
                            "could not create `{}` in staging: {error}",
                            parent.display()
                        ),
                    )
                    .with("entry", entry.path.to_owned())
                })?;
            }
            root.write(entry.path, rendered.as_bytes())
                .map_err(|error| {
                    CliError::new(
                        Code::RenderFailed,
                        format!("could not write `{}` in staging: {error}", entry.path),
                    )
                    .with("entry", entry.path.to_owned())
                })?;
        }

        Ok(())
    }
}

/// Maps a MiniJinja failure onto the error taxonomy.
///
/// Fuel exhaustion is a **bound**, not a failure: the operator asked for something larger than the
/// documented limit, which is `bound_exceeded` and exit `3`. Everything else is `render_failed`.
fn classify(entry: &str, error: &minijinja::Error) -> CliError {
    if error.kind() == minijinja::ErrorKind::OutOfFuel {
        return CliError::new(
            Code::BoundExceeded,
            format!(
                "rendering `{entry}` exhausted the expansion budget of {} instructions",
                bounds::FUEL
            ),
        )
        .with("bound", "template_fuel")
        .with("limit", bounds::FUEL.to_string())
        .with("entry", entry.to_owned());
    }
    CliError::new(
        Code::RenderFailed,
        format!("`{entry}` could not be rendered: {error}"),
    )
    .with("entry", entry.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cap_std::ambient_authority;
    use serde::Serialize;

    /// A staging directory and a handle on it, kept together so the temporary directory outlives
    /// the handle.
    struct Staged {
        _base: tempfile::TempDir,
        dir: Dir,
    }

    fn staged() -> Staged {
        let base = tempfile::tempdir().expect("tempdir");
        let dir = Dir::open_ambient_dir(base.path(), ambient_authority()).expect("opens");
        Staged { _base: base, dir }
    }

    #[derive(Serialize)]
    struct Ctx {
        name: &'static str,
    }

    fn ctx() -> Ctx {
        Ctx { name: "my-app" }
    }

    fn set(entries: &[TemplateEntry]) -> TemplateSet {
        TemplateSet {
            version: "test",
            entries: entries.to_vec(),
        }
    }

    #[test]
    fn an_undefined_variable_is_an_error_not_an_empty_rendering() {
        // FR-028. The failure mode this prevents is a generated file that is silently missing a
        // value — which compiles, ships, and is discovered by a user.
        let templates = set(&[TemplateEntry {
            path: "a.txt",
            body: "{{ absent }}",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        let error = renderer.render_into(&root.dir, &ctx()).unwrap_err();
        assert_eq!(error.code, Code::RenderFailed);
        assert!(
            root.dir.symlink_metadata("a.txt").is_err(),
            "a partial file was written"
        );
    }

    #[test]
    fn a_templates_final_newline_survives_rendering() {
        // MiniJinja strips it by default. A source file without a trailing newline fails
        // `cargo fmt --check`, so this is the difference between a skeleton that passes its own
        // gates on the first run and one that does not.
        let templates = set(&[TemplateEntry {
            path: "a.rs",
            body: "fn main() {}\n",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        renderer.render_into(&root.dir, &ctx()).expect("renders");
        assert_eq!(
            root.dir.read_to_string("a.rs").expect("read"),
            "fn main() {}\n",
            "the trailing newline was stripped"
        );
    }

    #[test]
    fn a_defined_variable_renders() {
        // POSITIVE CONTROL for the test above: an environment that failed every render would
        // satisfy it and be useless.
        let templates = set(&[TemplateEntry {
            path: "a.txt",
            body: "{{ name }}",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        renderer.render_into(&root.dir, &ctx()).expect("renders");
        assert_eq!(root.dir.read_to_string("a.txt").expect("read"), "my-app");
    }

    #[test]
    fn no_builtin_filter_is_reachable_unless_it_is_on_the_allow_list() {
        // `Environment::new()` would make every one of these work. This test is what makes the
        // choice of `empty()` visible, and it will fail loudly if somebody "fixes" the constructor.
        for body in [
            "{{ name|trim }}",
            "{{ name|title }}",
            "{{ name|length }}",
            "{{ range(3) }}",
        ] {
            let renderer = Renderer::new(set(&[TemplateEntry {
                path: "a.txt",
                body,
            }]))
            .expect("builds");
            let root = staged();
            let error = renderer.render_into(&root.dir, &ctx()).unwrap_err();
            assert_eq!(error.code, Code::RenderFailed, "{body} was reachable");
        }
    }

    #[test]
    fn a_boolean_renders_as_toml_rather_than_as_python() {
        // THE REGRESSION TEST. Bare `{{ flag }}` yields `True`, which is invalid TOML.
        #[derive(Serialize)]
        struct Flags {
            yes: bool,
            no: bool,
        }
        let templates = set(&[TemplateEntry {
            path: "a.toml",
            body: "a = {{ yes | toml_bool }}\nb = {{ no | toml_bool }}\nbare = {{ yes }}\n",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        renderer
            .render_into(
                &root.dir,
                &Flags {
                    yes: true,
                    no: false,
                },
            )
            .expect("renders");
        let rendered = root.dir.read_to_string("a.toml").expect("read");
        assert!(rendered.contains("a = true"), "{rendered}");
        assert!(rendered.contains("b = false"), "{rendered}");
        // NEGATIVE CONTROL, and the reason the filter is needed at all: the bare form is wrong.
        assert!(
            rendered.contains("bare = True"),
            "MiniJinja no longer renders a bare bool as `True`; if that changed upstream, the \
             `toml_bool` filter may be removable — but verify before removing it: {rendered}"
        );
    }

    #[test]
    fn the_allow_listed_filters_do_work() {
        // POSITIVE CONTROL for the deny-by-default test above.
        let templates = set(&[TemplateEntry {
            path: "a.txt",
            body: "{{ name|upper }} {{ name|crate_name }} {{ name|replace('-','+') }}",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        renderer.render_into(&root.dir, &ctx()).expect("renders");
        assert_eq!(
            root.dir.read_to_string("a.txt").expect("read"),
            "MY-APP my_app my+app"
        );
    }

    #[test]
    fn work_beyond_the_expansion_budget_is_stopped_by_fuel_and_reported_as_a_bound() {
        // SC-013. The loop body emits **nothing**, so the only thing that can stop this is the
        // instruction budget — not the byte limits, not the file limit. Without the `fuel` feature
        // this test does not fail, it hangs, which is precisely why the feature is declared
        // explicitly in `Cargo.toml`.
        #[derive(Serialize)]
        struct Wide {
            items: Vec<u32>,
        }
        let templates = set(&[TemplateEntry {
            path: "a.txt",
            body: "{% for a in items %}{% for b in items %}{% endfor %}{% endfor %}",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        // 2_000 × 2_000 = 4,000,000 iterations against a budget of 1,000,000 instructions.
        let context = Wide {
            items: (0..2_000).collect(),
        };
        let error = renderer.render_into(&root.dir, &context).unwrap_err();
        assert_eq!(error.code, Code::BoundExceeded);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "bound" && v == "template_fuel")
        );
        assert!(error.details.iter().any(|(k, _)| k == "limit"));
        assert!(
            root.dir.symlink_metadata("a.txt").is_err(),
            "a partial file survived the bound"
        );
    }

    #[test]
    fn work_inside_the_expansion_budget_completes() {
        // POSITIVE CONTROL for the bound. A renderer that refused every loop would satisfy the test
        // above while being unable to generate anything, and the difference between "bounded" and
        // "broken" is exactly this assertion.
        #[derive(Serialize)]
        struct Wide {
            items: Vec<u32>,
        }
        let templates = set(&[TemplateEntry {
            path: "a.txt",
            body: "{% for a in items %}.{% endfor %}",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        let context = Wide {
            items: (0..1_000).collect(),
        };
        renderer
            .render_into(&root.dir, &context)
            .expect("a 1,000-iteration loop must fit");
        assert_eq!(root.dir.read_to_string("a.txt").expect("read").len(), 1_000);
    }

    #[test]
    fn an_entry_path_that_escapes_is_refused_at_load_time() {
        // I-5. Each of these must fail before any render is attempted.
        for path in ["../escape", "/etc/passwd", "a/../../b", "./a", "a\\b", ""] {
            let leaked: &'static str = Box::leak(path.to_string().into_boxed_str());
            let error = Renderer::new(set(&[TemplateEntry {
                path: leaked,
                body: "x",
            }]))
            .unwrap_err();
            assert_eq!(error.code, Code::Internal, "{path:?} was accepted");
        }
    }

    #[test]
    fn a_duplicate_entry_path_is_refused() {
        let templates = set(&[
            TemplateEntry {
                path: "a.txt",
                body: "one",
            },
            TemplateEntry {
                path: "a.txt",
                body: "two",
            },
        ]);
        assert_eq!(Renderer::new(templates).unwrap_err().code, Code::Internal);
    }

    #[test]
    fn a_nested_entry_path_is_accepted_and_its_directories_are_created() {
        // POSITIVE CONTROL for the load-time refusals: a validator that refused every path would
        // satisfy them all.
        let templates = set(&[TemplateEntry {
            path: "src/bin/main.rs",
            body: "{{ name }}",
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        renderer.render_into(&root.dir, &ctx()).expect("renders");
        assert!(
            root.dir
                .symlink_metadata("src/bin/main.rs")
                .expect("stat")
                .is_file()
        );
    }

    #[test]
    fn a_file_above_the_per_file_limit_is_a_bound_not_a_write() {
        let body: &'static str = Box::leak(
            format!(
                "{{% for i in range_stub %}}{}{{% endfor %}}",
                "x".repeat(4096)
            )
            .into_boxed_str(),
        );
        // `range_stub` is supplied by the context, so this stays inside the allow-list.
        #[derive(Serialize)]
        struct Big {
            range_stub: Vec<u32>,
            name: &'static str,
        }
        let templates = set(&[TemplateEntry {
            path: "big.txt",
            body,
        }]);
        let renderer = Renderer::new(templates).expect("builds");
        let root = staged();
        let context = Big {
            range_stub: (0..400).collect(),
            name: "my-app",
        };
        let error = renderer.render_into(&root.dir, &context).unwrap_err();
        assert_eq!(error.code, Code::BoundExceeded);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "bound" && v == "single_file_output_bytes")
        );
        assert!(
            root.dir.symlink_metadata("big.txt").is_err(),
            "the oversized file was written"
        );
    }

    // Relationships between the bounds, checked at COMPILE time rather than in a test.
    //
    // A test would prove the same thing later and only if it were run; a `const` assertion makes an
    /// Builds `count` distinct entries, each with `body`, leaking the paths so they are `'static`.
    ///
    /// Leaking is acceptable here and nowhere else: these run once, in a test process that exits.
    fn many(count: usize, body: &'static str) -> Vec<TemplateEntry> {
        (0..count)
            .map(|index| TemplateEntry {
                path: Box::leak(format!("file{index}.txt").into_boxed_str()),
                body,
            })
            .collect()
    }

    #[test]
    fn more_output_files_than_the_limit_is_a_bound_not_a_write() {
        // FR-026's file-count bound, which had no test at all — the constant was declared,
        // enforced, and never exercised, which is the same state `fuel` would have been in had
        // nobody noticed it is not a default feature.
        let entries = many(bounds::MAX_FILES + 1, "x");
        let renderer = Renderer::new(set(&entries)).expect("builds");
        let root = staged();
        let error = renderer.render_into(&root.dir, &ctx()).unwrap_err();

        assert_eq!(error.code, Code::BoundExceeded);
        assert!(
            error.details.iter().any(|(key, value)| key == "bound" && value == "output_file_count"),
            "{:?}",
            error.details
        );
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "limit" && value == &bounds::MAX_FILES.to_string()),
            "the limit must be reported so an operator can see what it was: {:?}",
            error.details
        );
        // NOTHING was written. The count is checked before any file is produced.
        assert_eq!(root.dir.entries().expect("readable").count(), 0);
    }

    #[test]
    fn exactly_the_file_limit_is_still_rendered() {
        // The boundary control. Without it, an off-by-one that refused at `MAX_FILES` rather than
        // above it would satisfy the test above and quietly halve the usable budget.
        let entries = many(bounds::MAX_FILES, "x");
        let renderer = Renderer::new(set(&entries)).expect("builds");
        let root = staged();
        renderer.render_into(&root.dir, &ctx()).expect("exactly the limit is allowed");
        assert_eq!(root.dir.entries().expect("readable").count(), bounds::MAX_FILES);
    }

    #[test]
    fn more_total_output_than_the_limit_is_a_bound_not_a_write() {
        // The total-bytes bound, distinct from the per-file one: every file here is exactly at the
        // per-file limit and therefore individually legal, and together they are not. A design
        // with only a per-file bound passes every per-file test and still lets a template set
        // write 512 MiB.
        let body: &'static str = Box::leak("x".repeat(bounds::MAX_FILE_BYTES).into_boxed_str());
        let count = bounds::MAX_TOTAL_BYTES / bounds::MAX_FILE_BYTES + 1;
        let renderer = Renderer::new(set(&many(count, body))).expect("builds");
        let root = staged();
        let error = renderer.render_into(&root.dir, &ctx()).unwrap_err();

        assert_eq!(error.code, Code::BoundExceeded);
        assert!(
            error.details.iter().any(|(key, value)| key == "bound" && value == "total_output_bytes"),
            "{:?}",
            error.details
        );
        assert!(
            error
                .details
                .iter()
                .any(|(key, value)| key == "limit" && value == &bounds::MAX_TOTAL_BYTES.to_string()),
            "{:?}",
            error.details
        );
    }

    #[test]
    fn exactly_the_total_output_limit_is_still_rendered() {
        // The boundary control for the bound above.
        let body: &'static str = Box::leak("x".repeat(bounds::MAX_FILE_BYTES).into_boxed_str());
        let count = bounds::MAX_TOTAL_BYTES / bounds::MAX_FILE_BYTES;
        let renderer = Renderer::new(set(&many(count, body))).expect("builds");
        let root = staged();
        renderer.render_into(&root.dir, &ctx()).expect("exactly the limit is allowed");
    }

    #[test]
    fn a_file_exactly_at_the_per_file_limit_is_still_written() {
        // The boundary control for `a_file_above_the_per_file_limit_is_a_bound_not_a_write`, which
        // existed without one.
        let body: &'static str = Box::leak("x".repeat(bounds::MAX_FILE_BYTES).into_boxed_str());
        let renderer =
            Renderer::new(set(&[TemplateEntry { path: "a.txt", body }])).expect("builds");
        let root = staged();
        renderer.render_into(&root.dir, &ctx()).expect("exactly the limit is allowed");
    }

    #[test]
    fn the_recursion_bound_has_no_reachable_trigger_and_that_is_the_point() {
        // RECURSION_DEPTH is declared, set on the environment, and — with this feature set —
        // **cannot be reached**. Every construct that could recurse is compiled out: MiniJinja's
        // `multi_template` feature (which provides `{% include %}` and `{% extends %}`) and its
        // `macros` feature are both off in `Cargo.toml`.
        //
        // Measured rather than reasoned: `include` is not merely disallowed at render time, it is
        // **not a statement the compiled grammar knows**, so an entry using it is refused when the
        // catalogue loads — which is to say such a template could not exist in a shipped binary at
        // all. That is a stronger guarantee than a depth counter and it is why the counter has no
        // over-bound test: there is no over-bound to reach.
        //
        // This is a test rather than a comment because the structural claim can rot. Enabling
        // either feature to solve some future problem makes recursion reachable, and RECURSION_DEPTH
        // then needs a real over-bound test. This fails at that moment, which is when somebody
        // should be thinking about it.
        let error = Renderer::new(set(&[TemplateEntry {
            path: "a.txt",
            body: "{% include \"a.txt\" %}",
        }]))
        .expect_err("`include` must not be a statement this build understands");

        assert!(
            error.message.contains("unknown statement include"),
            "`include` resolved, which means a template feature was enabled and RECURSION_DEPTH \
             now needs a real over-bound test: {error:?}"
        );
    }

    // inconsistent set of bounds a build failure. `RECURSION_DEPTH` must stay under MiniJinja's own
    // ceiling of 500, or `set_recursion_limit` silently clamps and the declared bound is fiction.
    const _: () = assert!(bounds::FUEL > 0);
    const _: () = assert!(bounds::RECURSION_DEPTH < 500);
    const _: () = assert!(bounds::MAX_FILE_BYTES < bounds::MAX_TOTAL_BYTES);
    const _: () = assert!(bounds::MAX_FILES > 0);
}
