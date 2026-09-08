//! FR-012-7a steps (2) and (4): the isolation an identification probe runs in, and the bounded
//! execution every probe uses.
//!
//! # What the isolation is for
//!
//! A rustup proxy — and rustup itself, whose `--version` also resolves the active toolchain to
//! report its `rustc` (rustup 1.29.0 prints `the currently active rustc version is …`; measured
//! 2026-09-07) — selects a toolchain from the directory it runs in and from `RUSTUP_TOOLCHAIN`.
//! Before 1.28.0 that selection **installed** an absent toolchain. So until a rustup's version
//! is known, nothing may run it, or a proxy of it, anywhere a toolchain is named. The isolation
//! is a place where nothing is named:
//!
//! - the working directory is an exclusively created empty directory with **no
//!   `rust-toolchain.toml` or `rust-toolchain` in any ancestor**;
//! - `RUSTUP_HOME` and `CARGO_HOME` point at exclusively created empty directories, so no
//!   directory override, default, or settings file of the operator's is visible;
//! - `RUSTUP_TOOLCHAIN` is absent from the child's environment whatever the seal carries;
//! - `RUSTUP_AUTO_INSTALL=0`, and `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` point at an
//!   unroutable loopback address **in this child only** — a belt-and-braces control, not a seal
//!   variable;
//! - the wait is bounded (30 s); a child that does not answer is killed and reported as
//!   `compiler_identity_unreadable`.
//!
//! Nothing is pinned or selected there, so the probe requests no toolchain and no rustup
//! release has anything to install. The three directories are removed when the [`Isolation`]
//! is dropped.
//!
//! # Where the directories live
//!
//! Under the system temporary directory, or failing that under the sealed home directory —
//! whichever first has no toolchain file above it. A host where neither qualifies cannot host
//! the probe, and says so: `project_verification_failed` with
//! `details.reason = probe_isolation_unavailable`.

use std::ffi::OsStr;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use wait_timeout::ChildExt as _;

use crate::exit::{CliError, Code};
use crate::generate::verify::{Sealed, sealed_command};
use crate::toolchain::locate;

/// How long an identification probe may take (FR-012-7a): a `--version` or `-vV` answers in
/// milliseconds; thirty seconds is a hung shim, not a slow one.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// The unroutable address the isolated children are told to download from. Port 9 (`discard`)
/// on loopback answers nothing, and the operator's real mirror — if any — is never named.
pub const UNROUTABLE: &str = "http://127.0.0.1:9";

/// The most of a child's stream a probe keeps. An identity answer is a few hundred bytes; a
/// child that writes more is drained and its excess discarded, so a pipe never fills.
pub const MAX_CAPTURED_BYTES: usize = 64 * 1024;

/// The two file names rustup honours as a toolchain file.
const TOOLCHAIN_FILES: [&str; 2] = ["rust-toolchain.toml", "rust-toolchain"];

/// Whether `path` or any ancestor of it carries a toolchain file.
#[must_use]
pub fn has_toolchain_file_above(path: &Path) -> bool {
    path.ancestors().any(|ancestor| {
        TOOLCHAIN_FILES
            .iter()
            .any(|name| ancestor.join(name).exists())
    })
}

/// Three exclusively created, empty directories with no toolchain file above them, removed on
/// drop.
#[derive(Debug)]
pub struct Isolation {
    /// Owns the tree; dropping it removes everything under it. Never read outside the tests —
    /// holding it IS what it is for.
    #[cfg_attr(not(test), allow(dead_code))]
    base: tempfile::TempDir,
    cwd: PathBuf,
    rustup_home: PathBuf,
    cargo_home: PathBuf,
}

impl Isolation {
    /// Creates the isolation under the first base — the system temporary directory, then the
    /// sealed home — with no toolchain file above it.
    ///
    /// # Errors
    ///
    /// [`Code::ProjectVerificationFailed`] with `reason = probe_isolation_unavailable` when no
    /// such base exists or the directories cannot be created.
    pub fn create(sealed: &Sealed) -> Result<Self, CliError> {
        let home_variable = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
        let candidates = std::iter::once(std::env::temp_dir()).chain(
            locate::variable(sealed, home_variable)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        );
        for base in candidates {
            if !base.is_dir() || has_toolchain_file_above(&base) {
                continue;
            }
            let Ok(tree) = tempfile::Builder::new()
                .prefix("renvor-probe-")
                .tempdir_in(&base)
            else {
                continue;
            };
            let cwd = tree.path().join("cwd");
            let rustup_home = tree.path().join("rustup-home");
            let cargo_home = tree.path().join("cargo-home");
            // `create_dir`, not `create_dir_all`: each must be created here, by this process,
            // now — an existing directory would be somebody else's.
            if [&cwd, &rustup_home, &cargo_home]
                .iter()
                .any(|directory| std::fs::create_dir(directory).is_err())
            {
                continue;
            }
            return Ok(Self {
                base: tree,
                cwd,
                rustup_home,
                cargo_home,
            });
        }
        Err(CliError::new(
            Code::ProjectVerificationFailed,
            "the toolchain identification probe needs an empty directory with no \
             `rust-toolchain.toml` above it, and neither the temporary directory nor the home \
             directory offers one; nothing was run and nothing was written to the destination",
        )
        .with("reason", "probe_isolation_unavailable")
        .with("stage", "pre-placement verification"))
    }

    /// The exclusively created working directory.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// The exclusively created `RUSTUP_HOME`.
    ///
    /// `#[cfg(test)]` because production reads the three directories only through
    /// [`Isolation::command`], which sets them on the child itself; a shipped accessor would be
    /// a way to point something else at them that nothing asked for.
    #[cfg(test)]
    #[must_use]
    pub fn rustup_home(&self) -> &Path {
        &self.rustup_home
    }

    /// The exclusively created `CARGO_HOME`. `#[cfg(test)]`, as [`Isolation::rustup_home`].
    #[cfg(test)]
    #[must_use]
    pub fn cargo_home(&self) -> &Path {
        &self.cargo_home
    }

    /// The root the three directories live under. `#[cfg(test)]`, as
    /// [`Isolation::rustup_home`].
    #[cfg(test)]
    #[must_use]
    pub fn root(&self) -> &Path {
        self.base.path()
    }

    /// A command for `program` in the isolation: the sealed environment minus
    /// `RUSTUP_TOOLCHAIN`, the two homes pointed here, `RUSTUP_AUTO_INSTALL=0`, and both
    /// install-server variables at [`UNROUTABLE`].
    #[must_use]
    pub fn command(&self, program: &OsStr, sealed: &Sealed) -> Command {
        let mut command = sealed_command(program, sealed, &self.cwd);
        command
            .env_remove("RUSTUP_TOOLCHAIN")
            .env("RUSTUP_HOME", &self.rustup_home)
            .env("CARGO_HOME", &self.cargo_home)
            .env("RUSTUP_AUTO_INSTALL", "0")
            .env("RUSTUP_DIST_SERVER", UNROUTABLE)
            .env("RUSTUP_UPDATE_ROOT", UNROUTABLE);
        command
    }

    /// Runs `program` with `arguments` in the isolation, bounded by [`PROBE_TIMEOUT`].
    ///
    /// # Errors
    ///
    /// [`RunError`]: the child could not be started, or did not exit in time and was killed.
    pub fn run(
        &self,
        program: &OsStr,
        arguments: &[&str],
        sealed: &Sealed,
    ) -> Result<Answer, RunError> {
        let mut command = self.command(program, sealed);
        command.args(arguments);
        run_bounded(command, PROBE_TIMEOUT)
    }
}

/// What a bounded child answered: its status and its two streams, each capped at
/// [`MAX_CAPTURED_BYTES`] and decoded lossily. **Raw text**: it is parsed under
/// [`super::grammar`] and never copied into a message or a detail.
#[derive(Debug)]
pub struct Answer {
    /// The exit status.
    pub status: ExitStatus,
    /// Up to [`MAX_CAPTURED_BYTES`] of stdout.
    pub stdout: String,
    /// Up to [`MAX_CAPTURED_BYTES`] of stderr.
    pub stderr: String,
}

/// Why a bounded run produced no answer.
#[derive(Debug)]
pub enum RunError {
    /// The program could not be started.
    Spawn(std::io::Error),
    /// The program did not exit within the deadline; it was killed.
    TimedOut(Duration),
    /// The wait itself failed.
    Wait(std::io::Error),
}

impl std::fmt::Display for RunError {
    /// The kind of failure, never a path or a child's text.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(
                formatter,
                "the program could not be started: {}",
                error.kind()
            ),
            Self::TimedOut(after) => {
                write!(
                    formatter,
                    "the program did not exit within {}s and was stopped",
                    after.as_secs()
                )
            }
            Self::Wait(error) => write!(
                formatter,
                "waiting for the program failed: {}",
                error.kind()
            ),
        }
    }
}

impl std::error::Error for RunError {}

/// Reads at most [`MAX_CAPTURED_BYTES`] from a stream on its own thread, then drains the rest
/// so the child is never blocked on a full pipe.
fn capture(
    stream: Option<impl std::io::Read + Send + 'static>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut collected = Vec::new();
        if let Some(mut stream) = stream {
            let mut limited = (&mut stream).take(MAX_CAPTURED_BYTES as u64);
            let _ = limited.read_to_end(&mut collected);
            let _ = std::io::copy(&mut stream, &mut std::io::sink());
        }
        collected
    })
}

/// Runs `command` with both streams captured and a deadline. On the deadline the direct child
/// is killed and reaped and the readers are detached — joined, they could wait on a grandchild
/// that inherited the pipe (the shape `commands/relay.rs` measured).
///
/// # Errors
///
/// [`RunError`].
pub fn run_bounded(mut command: Command, timeout: Duration) -> Result<Answer, RunError> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RunError::Spawn)?;
    let stdout = capture(child.stdout.take());
    let stderr = capture(child.stderr.take());
    let status = match child.wait_timeout(timeout).map_err(RunError::Wait)? {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            drop(stdout);
            drop(stderr);
            return Err(RunError::TimedOut(timeout));
        }
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok(Answer {
        status,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
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

    #[test]
    fn a_base_under_a_toolchain_file_is_recognised() {
        let root = tempfile::tempdir().expect("tempdir");
        let pinned = root.path().join("pinned");
        let below = pinned.join("a").join("b");
        std::fs::create_dir_all(&below).expect("mkdir");
        std::fs::write(pinned.join("rust-toolchain.toml"), "[toolchain]\n").expect("write");
        assert!(has_toolchain_file_above(&below));
        assert!(has_toolchain_file_above(&pinned));
        let clean = root.path().join("clean");
        std::fs::create_dir_all(&clean).expect("mkdir");
        // The temporary root itself may sit under a pin on a hostile machine; the assertion is
        // relative: the clean sibling is no worse than its parent.
        assert_eq!(
            has_toolchain_file_above(&clean),
            has_toolchain_file_above(root.path())
        );
        // The extension-less legacy name counts too.
        std::fs::write(clean.join("rust-toolchain"), "1.94.0\n").expect("write");
        assert!(has_toolchain_file_above(&clean));
    }

    #[test]
    fn the_probe_environment_selects_no_toolchain_and_points_downloads_at_loopback() {
        // Pure: what the command carries, read back from the command itself.
        let toolchain = OsString::from("1.93.0");
        let path = OsString::from("/usr/bin");
        let seal = sealed(&[("PATH", &path), ("RUSTUP_TOOLCHAIN", &toolchain)]);
        let isolation = Isolation::create(&seal).expect("the isolation is created");
        assert!(isolation.cwd().is_dir() && isolation.rustup_home().is_dir());
        assert!(isolation.cargo_home().is_dir());
        assert!(!has_toolchain_file_above(isolation.cwd()));
        assert_ne!(isolation.cwd(), isolation.rustup_home());
        assert_ne!(isolation.cwd(), isolation.cargo_home());
        assert_ne!(isolation.rustup_home(), isolation.cargo_home());
        let command = isolation.command(OsStr::new("rustup"), &seal);
        let child: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
        let value = |name: &str| {
            child
                .iter()
                .find(|(candidate, _)| {
                    candidate
                        .to_str()
                        .is_some_and(|text| crate::generate::verify::same_variable_name(text, name))
                })
                .map(|(_, value)| *value)
        };
        // After `env_clear`, `env_remove` deletes the entry outright rather than marking it,
        // so the selector is simply not in the child's map; either shape means it is gone.
        assert!(
            !matches!(value("RUSTUP_TOOLCHAIN"), Some(Some(_))),
            "the toolchain selector reaches the isolated child"
        );
        assert_eq!(
            value("RUSTUP_HOME"),
            Some(Some(isolation.rustup_home().as_os_str()))
        );
        assert_eq!(
            value("CARGO_HOME"),
            Some(Some(isolation.cargo_home().as_os_str()))
        );
        assert_eq!(value("RUSTUP_AUTO_INSTALL"), Some(Some(OsStr::new("0"))));
        assert_eq!(
            value("RUSTUP_DIST_SERVER"),
            Some(Some(OsStr::new(UNROUTABLE)))
        );
        assert_eq!(
            value("RUSTUP_UPDATE_ROOT"),
            Some(Some(OsStr::new(UNROUTABLE)))
        );
        assert_eq!(value("PATH"), Some(Some(path.as_os_str())));
        assert_eq!(command.get_current_dir(), Some(isolation.cwd()));
    }

    #[test]
    fn the_directories_are_removed_on_drop() {
        let path = OsString::from("/usr/bin");
        let seal = sealed(&[("PATH", &path)]);
        let isolation = Isolation::create(&seal).expect("created");
        let root = isolation.root().to_path_buf();
        assert!(root.is_dir());
        drop(isolation);
        assert!(!root.exists(), "the isolation left its directories behind");
    }

    #[cfg(unix)]
    #[test]
    fn the_identification_probe_uses_exclusively_created_empty_directories() {
        // FR-012-7a step (4). The stub records where it ran and what its two homes were: three
        // distinct directories, each empty at the moment of the call, none under a toolchain
        // file, none surviving the probe.
        let stubs = crate::toolchain::testing::Stubs::new();
        stubs.script("rustc", "printf '%s' \"$RUSTC_VV\"\nexit 0\n");
        let seal = stubs.sealed(&[("RUSTUP_TOOLCHAIN", "1.93.0")]);
        let isolation = Isolation::create(&seal).expect("created");
        let answer = isolation
            .run(stubs.bin.join("rustc").as_os_str(), &["-vV"], &seal)
            .expect("the stub runs");
        assert!(answer.status.success());
        let root = isolation.root().to_path_buf();
        drop(isolation);
        let records = stubs.records();
        assert_eq!(records.len(), 1, "exactly one invocation");
        let record = &records[0];
        assert_eq!(record.toolchain_file, "no");
        assert_eq!(record.rustup_toolchain, "<unset>");
        assert_eq!(record.auto_install, "0");
        assert_eq!(record.dist_server, UNROUTABLE);
        assert_eq!(record.update_root, UNROUTABLE);
        assert_eq!(record.cwd_entries, "0");
        assert_eq!(record.rustup_home_entries, "0");
        assert_eq!(record.cargo_home_entries, "0");
        let cwd = Path::new(&record.cwd);
        let rustup_home = Path::new(&record.rustup_home);
        let cargo_home = Path::new(&record.cargo_home);
        assert!(cwd != rustup_home && cwd != cargo_home && rustup_home != cargo_home);
        let base = std::fs::canonicalize(&root).ok();
        assert!(base.is_none(), "the isolation must be gone once dropped");
        for surviving in cwd.ancestors().filter(|ancestor| ancestor.exists()) {
            for name in TOOLCHAIN_FILES {
                assert!(
                    !surviving.join(name).exists(),
                    "a toolchain file sits above the probe's working directory"
                );
            }
        }
        assert!(!cwd.exists() && !rustup_home.exists() && !cargo_home.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_child_that_never_exits_is_killed_at_the_deadline() {
        let stubs = crate::toolchain::testing::Stubs::new();
        stubs.script("rustc", "sleep 30\nexit 0\n");
        let seal = stubs.sealed(&[]);
        let isolation = Isolation::create(&seal).expect("created");
        let mut command = isolation.command(stubs.bin.join("rustc").as_os_str(), &seal);
        command.arg("-vV");
        let started = std::time::Instant::now();
        let error = run_bounded(command, Duration::from_millis(300))
            .expect_err("the sleeping stub is killed");
        assert!(matches!(error, RunError::TimedOut(_)));
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the deadline did not bound the wait"
        );
    }
}
