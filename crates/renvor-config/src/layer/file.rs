//! The TOML file layer.
//!
//! # Required and optional files are different things
//!
//! C-C11 draws a line the convenient implementation blurs. A declared file that is **absent where
//! absence is permitted** contributes nothing and resolution succeeds. A declared file that is
//! **absent where it was required** fails, naming the source — *never* silently treated as empty.
//!
//! Those are two different callers' intentions, so they are two different constructors
//! ([`FileLayer::required`] and [`FileLayer::optional`]) rather than one function with a boolean
//! nobody reads at the call site.
//!
//! # Hostile input fails closed
//!
//! C-C10 requires a malformed, truncated, or unexpectedly large file to produce a bounded,
//! actionable error — never a panic and never unbounded memory. Two things provide that: a
//! declared **byte ceiling** checked before the parser sees anything, and a parse error that is
//! converted into a diagnostic rather than propagated as a panic. Reading first and checking after
//! would make the ceiling decorative, so the file is measured before it is read.

use std::path::{Path, PathBuf};

use renvor_core::KernelError;
use renvor_core::config_port::SourceLayer;
use renvor_core::error::context::{Constraint, configuration};
use toml::Table;

/// The largest configuration file the loader will read, in bytes.
///
/// **Renvor's number, not the specification's.** C-C10 requires the bound; no artifact names a
/// value. One mebibyte is far above any hand-written configuration and far below anything that
/// threatens a process, and it is overridable.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// A TOML file to load as one layer.
#[derive(Clone, Debug)]
pub struct FileLayer {
    path: PathBuf,
    required: bool,
    max_bytes: u64,
}

impl FileLayer {
    /// A file that **must** exist. Its absence fails resolution, naming the file.
    #[must_use]
    pub fn required(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: true,
            max_bytes: MAX_FILE_BYTES,
        }
    }

    /// A file that **may** be absent. Its absence contributes nothing and is not an error.
    #[must_use]
    pub fn optional(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            required: false,
            max_bytes: MAX_FILE_BYTES,
        }
    }

    /// Overrides the byte ceiling for this file.
    #[must_use]
    pub const fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// The path this layer reads.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The layer identity used in attribution and diagnostics.
    ///
    /// Carries the file's own name, because "the TOML layer" is not an answer when there are two
    /// files and FR-016 has to say which one won.
    #[must_use]
    pub fn source_layer(&self) -> SourceLayer {
        SourceLayer::File(self.path.display().to_string())
    }

    /// Reads and parses the file.
    ///
    /// Returns `None` when an optional file is absent — distinct from `Some(empty table)`, which
    /// means the file exists and set nothing (C-C11).
    ///
    /// # Errors
    ///
    /// - [`KernelError::Configuration`] naming the file when a **required** file is missing, when
    ///   it exceeds its byte ceiling, or when it is not valid TOML.
    pub fn read(&self) -> Result<Option<Table>, KernelError> {
        let metadata = match std::fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if self.required {
                    return Err(self.failure(&Constraint::Rule("the file does not exist")));
                }
                return Ok(None);
            }
            Err(_) => return Err(self.failure(&Constraint::Rule("the file could not be read"))),
        };

        // Checked before reading, so an oversized file is never brought into memory. Checking
        // after would make the ceiling a report rather than a limit (C-C10).
        if metadata.len() > self.max_bytes {
            // Lengths, not contents: the size of an oversized file is a shape fact and safe to
            // report, while a single byte of it would not be.
            return Err(self.failure(&Constraint::TooLarge {
                maximum_bytes: self.max_bytes,
            }));
        }

        let text = std::fs::read_to_string(&self.path)
            .map_err(|_| self.failure(&Constraint::Rule("the file could not be read")))?;

        // A parse error's message describes syntax and position, but a `toml` error also renders
        // the offending line. The message is therefore dropped rather than forwarded — the file
        // is named, and the author can open it.
        text.parse::<Table>()
            .map(Some)
            .map_err(|_| self.failure(&Constraint::Rule("the file is not valid TOML")))
    }

    /// Builds a diagnostic naming this file.
    ///
    /// The **file** is the key here, not a configuration key: C-L2 says a `Load` failure names the
    /// source that could not be read, and at this point no key has been reached.
    fn failure(&self, constraint: &Constraint) -> KernelError {
        configuration(
            self.path.display().to_string(),
            self.source_layer().label(),
            "a readable TOML document",
            constraint,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::FileLayer;
    use renvor_core::ErrorCategory;
    use std::io::Write as _;

    /// Writes a fixture into a uniquely-named directory under the target directory.
    ///
    /// Deliberately not a random name: the caller supplies one, so a failing test names a file
    /// that is still there to look at afterwards.
    fn scratch() -> std::path::PathBuf {
        // `CARGO_TARGET_TMPDIR` is only defined for integration tests, and these are unit tests,
        // so it is read at run time with a fallback rather than baked in at compile time.
        std::env::var("CARGO_TARGET_TMPDIR").map_or_else(
            |_| std::env::temp_dir().join("renvor-config-fixtures"),
            |dir| std::path::PathBuf::from(dir).join("config-fixtures"),
        )
    }

    fn fixture(name: &str, contents: &str) -> std::path::PathBuf {
        let directory = scratch();
        std::fs::create_dir_all(&directory).expect("the fixture directory is writable");
        let path = directory.join(name);
        let mut file = std::fs::File::create(&path).expect("the fixture is writable");
        file.write_all(contents.as_bytes()).expect("written");
        path
    }

    #[test]
    fn a_present_file_is_parsed() {
        let path = fixture("present.toml", "port = 8080\n");
        let table = FileLayer::required(&path)
            .read()
            .expect("the file is valid")
            .expect("the file is present");
        assert_eq!(table["port"].as_integer(), Some(8080));
    }

    #[test]
    fn an_empty_file_is_present_and_contributes_nothing() {
        // C-C11: present with 0 keys is not the same as absent, and is not an error.
        let path = fixture("empty.toml", "# only a comment\n");
        let table = FileLayer::required(&path)
            .read()
            .expect("valid")
            .expect("present");
        assert!(table.is_empty(), "0 keys, but the layer exists");
    }

    #[test]
    fn an_absent_optional_file_contributes_nothing() {
        let path = scratch().join("never-created.toml");
        assert!(
            FileLayer::optional(&path)
                .read()
                .expect("not an error")
                .is_none(),
            "absent is distinct from empty"
        );
    }

    #[test]
    fn an_absent_required_file_fails_naming_the_file() {
        // C-C11's third row: never silently treated as empty.
        let path = scratch().join("never-created.toml");
        let error = FileLayer::required(&path)
            .read()
            .expect_err("a required file that is missing must fail");

        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert!(
            error.to_string().contains("never-created.toml"),
            "the file must be named: {error}"
        );
    }

    #[test]
    fn malformed_toml_fails_with_an_actionable_error() {
        // C-C10: bounded and actionable, never a panic.
        let path = fixture("malformed.toml", "port = = 8080\n");
        let error = FileLayer::required(&path)
            .read()
            .expect_err("not valid TOML");
        assert_eq!(error.category(), ErrorCategory::Configuration);
        assert!(error.to_string().contains("malformed.toml"), "{error}");
    }

    #[test]
    fn a_file_over_the_ceiling_is_refused_before_it_is_read() {
        let path = fixture("large.toml", &"# padding\n".repeat(200));
        let error = FileLayer::required(&path)
            .with_max_bytes(64)
            .read()
            .expect_err("the file is over its ceiling");
        assert!(error.to_string().contains("ceiling"), "{error}");
        assert!(
            error.to_string().contains("64"),
            "the ceiling is named: {error}"
        );

        // POSITIVE CONTROL: the same file loads when the ceiling permits it, so the refusal is
        // about the ceiling rather than about the file.
        FileLayer::required(&path)
            .with_max_bytes(1_000_000)
            .read()
            .expect("the same file is fine under a larger ceiling");
    }

    #[test]
    fn two_files_are_distinguishable_layers() {
        // FR-016: "the TOML layer" is not an answer when there are two files.
        let first = FileLayer::required("base.toml");
        let second = FileLayer::required("override.toml");
        assert_ne!(first.source_layer(), second.source_layer());
        assert_eq!(first.source_layer().label(), "base.toml");
    }
}
