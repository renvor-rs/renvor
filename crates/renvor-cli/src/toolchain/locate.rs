//! FR-012-7a step (1): locate `rustup` **without executing a proxy**, and locate a tool on the
//! sealed `PATH`.
//!
//! # Why this executes nothing
//!
//! `rustc` and `cargo` on `PATH` may be rustup's proxies, and a proxy run in a pinned directory
//! resolves the pin — which, on a rustup older than 1.28.0, installs it. So the question "which
//! rustup owns these proxies?" is answered from the filesystem alone, and the answer is then
//! version-checked under [`super::isolate`] before any proxy runs anywhere (FR-012-7a step (2)).
//!
//! # The four-step rule
//!
//! 1. `rustup` on the sealed `PATH`;
//! 2. else a `rustup` beside the `rustc` that `PATH` resolves — beside the entry itself, and
//!    beside the file it links to (a `/usr/local/bin/rustc -> ~/.cargo/bin/rustc` layout keeps
//!    rustup off `PATH` and next to the proxies' real location);
//! 3. else `$CARGO_HOME/bin/rustup`;
//! 4. else `<HOME>/.cargo/bin/rustup` (`USERPROFILE` on Windows).
//!
//! # Why custom code rather than a `which` crate
//!
//! The lookup reads the **sealed** variables (never `std::env`), honours `PATHEXT` on Windows, and
//! feeds a same-file classification that needs the resolved path, not a boolean. A `which` crate
//! answers a different question from the process environment; the four-step rule with file
//! identity is this module's whole content, and it is small enough to read against the brief.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::generate::verify::Sealed;

/// The value of a sealed variable, if present.
#[must_use]
pub fn variable<'a>(sealed: &'a Sealed, name: &str) -> Option<&'a OsStr> {
    sealed
        .variables
        .iter()
        .find(|(candidate, _)| {
            candidate
                .to_str()
                .is_some_and(|text| crate::generate::verify::same_variable_name(text, name))
        })
        .map(|(_, value)| value.as_os_str())
}

/// Whether `path` names an executable file after symlinks are followed: a regular file that the
/// shell would run (on Unix, one with an execute bit).
#[must_use]
pub fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// The file names a tool called `name` may have in a directory: the bare name, and on Windows
/// the name with each `PATHEXT` extension (`.EXE`, `.CMD`, …) in the sealed variables' order.
fn file_names(sealed: &Sealed, name: &str) -> Vec<String> {
    let mut names = vec![name.to_owned()];
    if cfg!(windows) {
        let extensions = variable(sealed, "PATHEXT")
            .and_then(|value| value.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".to_owned());
        for extension in extensions.split(';').filter(|e| !e.is_empty()) {
            names.push(format!("{name}{extension}"));
        }
    }
    names
}

/// The first executable named `name` in `directory`, by the platform's file-name rules.
fn in_directory(sealed: &Sealed, directory: &Path, name: &str) -> Option<PathBuf> {
    file_names(sealed, name)
        .into_iter()
        .map(|file| directory.join(file))
        .find(|candidate| is_executable_file(candidate))
}

/// The first executable named `name` on the **sealed** `PATH` — the path as the shell would
/// resolve it, symlinks not followed. Executes nothing.
#[must_use]
pub fn on_path(sealed: &Sealed, name: &str) -> Option<PathBuf> {
    let path = variable(sealed, "PATH")?;
    std::env::split_paths(path)
        .filter(|directory| !directory.as_os_str().is_empty())
        .find_map(|directory| in_directory(sealed, &directory, name))
}

/// The home directory the sealed variables name: `HOME`, or `USERPROFILE` on Windows.
fn home(sealed: &Sealed) -> Option<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    variable(sealed, name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// Locates `rustup` by the four-step rule of FR-012-7a step (1). Executes nothing; reads only the
/// sealed variables and the filesystem.
#[must_use]
pub fn rustup(sealed: &Sealed) -> Option<PathBuf> {
    // 1. On PATH.
    if let Some(found) = on_path(sealed, "rustup") {
        return Some(found);
    }
    // 2. Beside the rustc PATH resolves — beside the entry, then beside what it links to.
    if let Some(rustc) = on_path(sealed, "rustc") {
        let beside_entry = rustc.parent().map(Path::to_path_buf);
        let beside_target = std::fs::canonicalize(&rustc)
            .ok()
            .and_then(|real| real.parent().map(Path::to_path_buf));
        for directory in beside_entry.into_iter().chain(beside_target) {
            if let Some(found) = in_directory(sealed, &directory, "rustup") {
                return Some(found);
            }
        }
    }
    // 3. $CARGO_HOME/bin/rustup.
    if let Some(cargo_home) = variable(sealed, "CARGO_HOME").filter(|value| !value.is_empty())
        && let Some(found) = in_directory(sealed, &Path::new(cargo_home).join("bin"), "rustup")
    {
        return Some(found);
    }
    // 4. <HOME>/.cargo/bin/rustup.
    home(sealed).and_then(|home| in_directory(sealed, &home.join(".cargo").join("bin"), "rustup"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn sealed(variables: &[(&str, &OsStr)]) -> Sealed {
        Sealed {
            variables: variables
                .iter()
                .map(|(name, value)| (OsString::from(name), OsString::from(value)))
                .collect(),
            credentials: Vec::new(),
        }
    }

    /// An executable file named `name` in `directory`.
    fn executable(directory: &Path, name: &str) -> PathBuf {
        let path = directory.join(name);
        std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("write");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        path
    }

    fn path_of(directories: &[&Path]) -> OsString {
        std::env::join_paths(directories.iter().map(|d| d.to_path_buf())).expect("joinable")
    }

    #[test]
    fn rustup_on_the_sealed_path_is_found_in_path_order() {
        let root = tempfile::tempdir().expect("tempdir");
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir_all(&first).expect("mkdir");
        std::fs::create_dir_all(&second).expect("mkdir");
        executable(&first, "rustc");
        let wanted = executable(&second, "rustup");
        executable(&second, "rustc");
        let path = path_of(&[&first, &second]);
        let sealed = sealed(&[("PATH", &path)]);
        assert_eq!(rustup(&sealed).as_deref(), Some(wanted.as_path()));
        // `on_path` answers the FIRST entry, as the shell would.
        assert_eq!(
            on_path(&sealed, "rustc").as_deref(),
            Some(first.join("rustc").as_path())
        );
        assert!(on_path(&sealed, "cargo").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_rustup_beside_the_resolved_rustc_is_found_when_none_is_on_path() {
        // The layout that hides rustup: PATH holds a shim directory whose `rustc` links into the
        // proxies' real directory, where `rustup` sits — off PATH, beside the proxies.
        let root = tempfile::tempdir().expect("tempdir");
        let shim = root.path().join("shim");
        let real = root.path().join("cargo-bin");
        std::fs::create_dir_all(&shim).expect("mkdir");
        std::fs::create_dir_all(&real).expect("mkdir");
        let hidden = executable(&real, "rustup");
        std::os::unix::fs::symlink(&hidden, real.join("rustc")).expect("symlink");
        std::os::unix::fs::symlink(real.join("rustc"), shim.join("rustc")).expect("symlink");
        let path = path_of(&[&shim]);
        let sealed = sealed(&[("PATH", &path)]);
        assert!(
            on_path(&sealed, "rustup").is_none(),
            "rustup is not on PATH in this layout"
        );
        // Compared canonically: the located path is the link target's real directory, which on
        // macOS spells the temporary root as `/private/var/…` where the test wrote `/var/…`.
        assert_eq!(
            rustup(&sealed).and_then(|p| std::fs::canonicalize(p).ok()),
            std::fs::canonicalize(&hidden).ok()
        );
        // And beside the PATH entry itself, when the entry is a real file.
        let beside = executable(&shim, "rustup");
        assert_eq!(rustup(&sealed).as_deref(), Some(beside.as_path()));
    }

    #[test]
    fn cargo_home_bin_then_the_home_directory_are_the_last_resorts() {
        let root = tempfile::tempdir().expect("tempdir");
        let empty = root.path().join("empty");
        let cargo_home = root.path().join("cargo-home");
        let home = root.path().join("home");
        std::fs::create_dir_all(&empty).expect("mkdir");
        std::fs::create_dir_all(cargo_home.join("bin")).expect("mkdir");
        std::fs::create_dir_all(home.join(".cargo").join("bin")).expect("mkdir");
        let path = path_of(&[&empty]);
        let home_variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let in_home = executable(&home.join(".cargo").join("bin"), "rustup");
        let sealed_home_only = sealed(&[("PATH", &path), (home_variable, home.as_os_str())]);
        assert_eq!(
            rustup(&sealed_home_only).as_deref(),
            Some(in_home.as_path())
        );
        // CARGO_HOME wins over the home directory.
        let in_cargo_home = executable(&cargo_home.join("bin"), "rustup");
        let sealed_both = sealed(&[
            ("PATH", &path),
            ("CARGO_HOME", cargo_home.as_os_str()),
            (home_variable, home.as_os_str()),
        ]);
        assert_eq!(
            rustup(&sealed_both).as_deref(),
            Some(in_cargo_home.as_path())
        );
    }

    #[test]
    fn nothing_is_located_from_the_process_environment() {
        // The seal is the only source. An empty seal locates nothing, whatever this process's
        // own PATH, CARGO_HOME, or HOME would have found.
        let nothing = sealed(&[]);
        assert!(rustup(&nothing).is_none());
        assert!(on_path(&nothing, "rustc").is_none());
        let empty_path = OsString::from("");
        assert!(rustup(&sealed(&[("PATH", &empty_path)])).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_without_an_execute_bit_or_a_directory_is_not_a_tool() {
        let root = tempfile::tempdir().expect("tempdir");
        let bin = root.path().join("bin");
        std::fs::create_dir_all(bin.join("cargo")).expect("a directory named cargo");
        std::fs::write(bin.join("rustup"), "not executable").expect("write");
        let path = path_of(&[&bin]);
        let sealed = sealed(&[("PATH", &path)]);
        assert!(on_path(&sealed, "rustup").is_none());
        assert!(on_path(&sealed, "cargo").is_none());
        assert!(rustup(&sealed).is_none());
    }
}
