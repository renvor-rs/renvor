//! Override and wrapper **presence** in Cargo's configuration — parsed, never evaluated.
//!
//! The record carries `rustc_override` and `wrapper` as booleans (§5.2, SR-012-3): whether the
//! operator's configuration names a `build.rustc`, a `build.rustc-wrapper`, or a
//! `build.rustc-workspace-wrapper`. Cargo reads those keys from `$CARGO_HOME/config.toml` (or
//! `config`) and from `.cargo/config.toml` (or `config`) in the working directory and every
//! ancestor. This module reads the same files and reports **presence only**: no value, no path,
//! no interpretation of relative paths or of Cargo's merge order.
//!
//! A file Cargo itself cannot parse is treated as declaring nothing: the check that runs Cargo
//! will fail by name on that file, which is a better report than a guess made here.

use std::path::Path;

use crate::generate::verify::Sealed;
use crate::toolchain::locate;

/// Which of the three keys the configuration names, anywhere Cargo would read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfigPresence {
    /// `[build] rustc = "…"`.
    pub rustc: bool,
    /// `[build] rustc-wrapper = "…"`.
    pub rustc_wrapper: bool,
    /// `[build] rustc-workspace-wrapper = "…"`.
    pub rustc_workspace_wrapper: bool,
}

impl ConfigPresence {
    /// The union of two files' presence.
    #[must_use]
    pub const fn or(self, other: Self) -> Self {
        Self {
            rustc: self.rustc || other.rustc,
            rustc_wrapper: self.rustc_wrapper || other.rustc_wrapper,
            rustc_workspace_wrapper: self.rustc_workspace_wrapper || other.rustc_workspace_wrapper,
        }
    }
}

/// The presence declared by one configuration file; nothing for an absent or unparseable one.
#[must_use]
pub fn in_file(file: &Path) -> ConfigPresence {
    let Ok(text) = std::fs::read_to_string(file) else {
        return ConfigPresence::default();
    };
    let Ok(document) = toml::from_str::<toml::Value>(&text) else {
        return ConfigPresence::default();
    };
    let Some(build) = document.get("build").and_then(toml::Value::as_table) else {
        return ConfigPresence::default();
    };
    // A key is present when it is a string; Cargo refuses any other type for these three and
    // the check would then fail by name.
    let named = |key: &str| build.get(key).is_some_and(toml::Value::is_str);
    ConfigPresence {
        rustc: named("rustc"),
        rustc_wrapper: named("rustc-wrapper"),
        rustc_workspace_wrapper: named("rustc-workspace-wrapper"),
    }
}

/// The two names Cargo accepts for a configuration file, in its own precedence order.
const FILE_NAMES: [&str; 2] = ["config.toml", "config"];

/// The presence in a `.cargo` directory at `base` — `base/config.toml`, then `base/config`.
fn in_cargo_directory(base: &Path) -> ConfigPresence {
    FILE_NAMES
        .iter()
        .map(|name| in_file(&base.join(name)))
        .fold(ConfigPresence::default(), ConfigPresence::or)
}

/// The presence Cargo would see from `dir`: `.cargo/config[.toml]` in `dir` and every ancestor,
/// and `$CARGO_HOME/config[.toml]` — `CARGO_HOME` from the sealed variables, else
/// `<HOME>/.cargo` (`USERPROFILE` on Windows). Reads only; evaluates nothing.
#[must_use]
pub fn presence(dir: &Path, sealed: &Sealed) -> ConfigPresence {
    let from_ancestors = dir
        .ancestors()
        .map(|ancestor| in_cargo_directory(&ancestor.join(".cargo")))
        .fold(ConfigPresence::default(), ConfigPresence::or);
    let home_variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let cargo_home = locate::variable(sealed, "CARGO_HOME")
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            locate::variable(sealed, home_variable)
                .filter(|value| !value.is_empty())
                .map(|home| Path::new(home).join(".cargo"))
        });
    let from_cargo_home = cargo_home
        .map(|base| in_cargo_directory(&base))
        .unwrap_or_default();
    from_ancestors.or(from_cargo_home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn sealed(variables: &[(&str, &Path)]) -> Sealed {
        Sealed {
            variables: variables
                .iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value.as_os_str())))
                .collect(),
            credentials: Vec::new(),
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }

    #[test]
    fn nothing_configured_is_nothing_present() {
        let root = tempfile::tempdir().expect("tempdir");
        let project = root.path().join("project");
        std::fs::create_dir_all(&project).expect("mkdir");
        let seal = sealed(&[("CARGO_HOME", &root.path().join("cargo-home"))]);
        assert_eq!(presence(&project, &seal), ConfigPresence::default());
    }

    #[test]
    fn the_three_keys_are_read_from_the_directory_its_ancestors_and_cargo_home() {
        let root = tempfile::tempdir().expect("tempdir");
        let project = root.path().join("work").join("project");
        std::fs::create_dir_all(&project).expect("mkdir");
        let cargo_home = root.path().join("cargo-home");
        let seal = sealed(&[("CARGO_HOME", &cargo_home)]);
        // The project's own `.cargo/config.toml` names `build.rustc`.
        write(
            &project.join(".cargo").join("config.toml"),
            "[build]\nrustc = \"/opt/other/rustc\"\n",
        );
        assert_eq!(
            presence(&project, &seal),
            ConfigPresence {
                rustc: true,
                ..ConfigPresence::default()
            }
        );
        // An ancestor's `.cargo/config` (the extension-less name) names the wrapper.
        write(
            &root.path().join("work").join(".cargo").join("config"),
            "[build]\nrustc-wrapper = \"sccache\"\n",
        );
        // `$CARGO_HOME/config.toml` names the workspace wrapper.
        write(
            &cargo_home.join("config.toml"),
            "[build]\nrustc-workspace-wrapper = \"/opt/wrap\"\n",
        );
        assert_eq!(
            presence(&project, &seal),
            ConfigPresence {
                rustc: true,
                rustc_wrapper: true,
                rustc_workspace_wrapper: true,
            }
        );
        // Without CARGO_HOME the home directory's `.cargo` is read instead.
        let home = root.path().join("home");
        write(
            &home.join(".cargo").join("config.toml"),
            "[build]\nrustc-workspace-wrapper = \"/opt/wrap\"\n",
        );
        let home_variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let seal_home = sealed(&[(home_variable, &home)]);
        assert!(presence(&project, &seal_home).rustc_workspace_wrapper);
    }

    #[test]
    fn an_unparseable_or_wrongly_typed_file_declares_nothing() {
        // Cargo fails by name on a file it cannot parse; a guess here would be a second, worse
        // report. And a key of the wrong type is not a declared value.
        let root = tempfile::tempdir().expect("tempdir");
        let project = root.path().join("project");
        write(
            &project.join(".cargo").join("config.toml"),
            "[build\nrustc = \"x\"\n",
        );
        let seal = sealed(&[("CARGO_HOME", &root.path().join("cargo-home"))]);
        assert_eq!(presence(&project, &seal), ConfigPresence::default());
        write(
            &project.join(".cargo").join("config.toml"),
            "[build]\nrustc = 1\n",
        );
        assert_eq!(presence(&project, &seal), ConfigPresence::default());
        // Values are never kept: the presence type has no room for one.
        assert_eq!(std::mem::size_of::<ConfigPresence>(), 3);
    }
}
