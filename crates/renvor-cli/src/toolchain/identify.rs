//! FR-012-7a steps (2)–(4) and FR-012-7c: **identify before invoking**.
//!
//! Nothing runs a rustup proxy in a pinned directory, or with an absent toolchain named, until
//! the rustup that owns the proxy is known to honour `RUSTUP_AUTO_INSTALL=0` — which is rustup
//! 1.28.1 and later ([`RUSTUP_FLOOR`]). Before that release a proxy asked for an absent toolchain
//! **installed** it, from inside generation, and hiding `rustup` from `PATH` changed nothing:
//! `rustc` and `cargo` on `PATH` may still be the proxies.
//!
//! | Step | Function | Guarantee |
//! |---|---|---|
//! | (2) the floor | [`floor`] | `rustup --version` runs only under [`Isolation`]; unparseable or below 1.28.1 is refused, and no proxy has run |
//! | (3) classify | [`is_proxy_of`], [`classify`] | a binary that is the same file as a located rustup is an *identified* proxy; nothing runs |
//! | (4) the probe | [`isolated_probe`] | a binary that is not: `-vV` once, under [`Isolation`]; an identity → bare; rustup's words → refused by name; else unreadable |
//! | FR-012-7c | [`confirm_no_install`] | on an identified proxy only: asked for a name that cannot be installed, it must answer `is not installed` |
//!
//! # Tests are stubs, and never download
//!
//! The behaviour of old rustup releases is not measured against old rustup releases — none is
//! installed for the purpose (U-10). Shell-script stubs stand in: a stub that "installs" (writes
//! a marker and connects to the dist address) whenever it runs where a toolchain is named proves
//! the generator never runs it there. The stubs are POSIX shell scripts and the tests that use
//! them are `#[cfg(unix)]`; on Windows they do not compile in, and the module's same-file rule
//! for Windows (identical bytes) is covered by the pure classification test alone. That gap is
//! stated here rather than hidden.

use std::path::{Path, PathBuf};

use crate::exit::{CliError, Code};
use crate::generate::verify::{Sealed, sealed_command};
use crate::toolchain::isolate::{self, Isolation, RunError};
use crate::toolchain::{Classification, Identity, RUSTUP_FLOOR, grammar, unreadable};

/// The toolchain name the FR-012-7c confirmation asks for: a custom name is never downloadable
/// (rustup can only *link* one), so a rustup that honours the guarantee answers
/// `is not installed` and nothing else (measured on rustup 1.29.0, 2026-09-07).
pub const UNINSTALLABLE_NAME: &str = "renvor-uninstallable-toolchain-name";

/// The `details.tool` of every refusal that is about rustup's version or identity.
pub const RUSTUP_TOOL: &str = "rustup >= 1.28.1";

/// [`RUSTUP_FLOOR`] as a version.
#[must_use]
pub fn floor_version() -> semver::Version {
    semver::Version::parse(RUSTUP_FLOOR).unwrap_or_else(|_| semver::Version::new(1, 28, 1))
}

/// Which tool an isolated probe is asking, which decides the first word of a valid answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// `rustc -vV`.
    Rustc,
    /// `cargo -vV`.
    Cargo,
}

impl Tool {
    /// The executable's name, and the first word of its `-vV` answer.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Rustc => "rustc",
            Self::Cargo => "cargo",
        }
    }
}

/// The floor refusal (FR-012-7a step (2)): exit 5, `tool = "rustup >= 1.28.1"`.
fn refuse_floor(found_version: &str, ran: bool) -> CliError {
    CliError::new(
        Code::ToolMissing,
        format!(
            "the rustup that owns the toolchain proxies on PATH reports {found_version}, and the \
             generator requires 1.28.1 or later: that release's `RUSTUP_AUTO_INSTALL=0` is what \
             makes a pinned-but-absent toolchain a refusal rather than a download from inside \
             generation. No proxy was run and nothing was written to the destination. Update rustup \
             with `rustup self update`"
        ),
    )
    .with("tool", RUSTUP_TOOL)
    .with("required", "true")
    .with("found", if ran { "true" } else { "false" })
    .with("foundVersion", found_version)
    .with("requiredVersion", RUSTUP_FLOOR)
    .with("remedy", "update rustup to 1.28.1 or later: rustup self update")
}

/// FR-012-7a step (2): `rustup --version` of the located `rustup`, under [`Isolation`], parsed
/// from stdout line 1 only.
///
/// # Errors
///
/// [`Code::ToolMissing`] (`tool = "rustup >= 1.28.1"`) when the answer is unparseable, the
/// program cannot be run, or the version is below the floor; [`Code::ProjectVerificationFailed`]
/// with `reason = compiler_identity_unreadable` when it does not answer within the deadline, or
/// `probe_isolation_unavailable` when no isolation can be created.
pub fn floor(rustup: &Path, sealed: &Sealed) -> Result<semver::Version, CliError> {
    let isolation = Isolation::create(sealed)?;
    let answer = match isolation.run(rustup.as_os_str(), &["--version"], sealed) {
        Ok(answer) => answer,
        Err(RunError::TimedOut(_)) => return Err(unreadable("rustup --version")),
        Err(RunError::Spawn(_) | RunError::Wait(_)) => {
            return Err(refuse_floor("a rustup that could not be run", false));
        }
    };
    let parsed = answer
        .status
        .success()
        .then(|| grammar::parse_rustup_version(&answer.stdout).ok())
        .flatten();
    match parsed {
        Some(version) if version >= floor_version() => Ok(version),
        Some(version) => Err(refuse_floor(&version.to_string(), true)),
        None => Err(refuse_floor("an unparseable version", true)),
    }
}

/// Whether `a` and `b` are the same file after symlinks are followed: same device and inode on
/// Unix; identical bytes on Windows, where rustup's proxies are copies of `rustup.exe`.
#[must_use]
pub fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (std::fs::metadata(a), std::fs::metadata(b)) {
            (Ok(ma), Ok(mb)) if ma.is_file() && mb.is_file() && ma.len() == mb.len() => {
                matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
            }
            _ => false,
        }
    }
}

/// FR-012-7a step (3) for one binary: it is a proxy of `rustup` when it is the same file.
#[must_use]
pub fn is_proxy_of(binary: &Path, rustup: &Path) -> bool {
    same_file(binary, rustup)
}

/// FR-012-7a step (3) for the resolved `rustc`: [`Classification::Proxy`] when a rustup was
/// located, passed the floor, and `rustc` is the same file as it; otherwise
/// [`Classification::Bare`] — which is not yet a verdict: step (4) must then run on every binary
/// that is not an identified proxy. Runs nothing.
#[must_use]
pub fn classify(rustc: &Path, located: Option<(PathBuf, semver::Version)>) -> Classification {
    match located {
        Some((rustup, version)) if is_proxy_of(rustc, &rustup) => {
            Classification::Proxy { rustup, version }
        }
        _ => Classification::Bare,
    }
}

/// The refusal of a proxy whose rustup could not be located (FR-012-7a step (4)).
fn refuse_unidentified(tool: Tool) -> CliError {
    CliError::new(
        Code::ToolMissing,
        format!(
            "`{}` on PATH answers in rustup's words, so it is a rustup proxy — but no `rustup` \
             was located on PATH, beside it, under CARGO_HOME, or under the home directory, so \
             the rustup that owns it could not be version-checked. A proxy of an unknown rustup \
             is never run where a toolchain is named. Nothing was written to the destination",
            tool.name()
        ),
    )
    .with("tool", RUSTUP_TOOL)
    .with("required", "true")
    .with("found", "false")
    .with("reason", "proxy_unidentified")
    .with(
        "remedy",
        "put the rustup that owns these proxies first on PATH",
    )
}

/// FR-012-7a step (4): `binary -vV` once, under [`Isolation`] — nothing is pinned or selected
/// there, so a proxy requests no toolchain and has nothing to install.
///
/// # Errors
///
/// An identity → `Ok` (the binary is *bare*, or a proxy able to answer without a selection).
/// rustup's words → [`Code::ToolMissing`] with `reason = proxy_unidentified`. Anything else, or
/// the deadline → [`Code::ProjectVerificationFailed`] with `reason = compiler_identity_unreadable`.
pub fn isolated_probe(binary: &Path, tool: Tool, sealed: &Sealed) -> Result<Identity, CliError> {
    let label = format!("{} -vV", tool.name());
    let isolation = Isolation::create(sealed)?;
    let answer = match isolation.run(binary.as_os_str(), &["-vV"], sealed) {
        Ok(answer) => answer,
        Err(RunError::TimedOut(_)) => return Err(unreadable(&label)),
        Err(RunError::Spawn(_) | RunError::Wait(_)) => {
            return Err(crate::toolchain::tool_absent(tool.name()));
        }
    };
    if answer.status.success()
        && let Ok(identity) = grammar::parse_vv(&answer.stdout, tool.name())
    {
        return Ok(identity);
    }
    if grammar::looks_like_rustup_words(&answer.stderr)
        || grammar::looks_like_rustup_words(&answer.stdout)
    {
        return Err(refuse_unidentified(tool));
    }
    Err(unreadable(&label))
}

/// FR-012-7c: the in-run witness that an **identified** proxy's rustup downloads nothing.
/// `rustc -vV` with `RUSTUP_TOOLCHAIN` naming [`UNINSTALLABLE_NAME`], under the seal
/// (`RUSTUP_AUTO_INSTALL=0` forced, the install-server variables absent, the operator's own
/// `RUSTUP_HOME` and `CARGO_HOME` — so the witness is about the rustup configuration
/// verification will actually use), in an [`Isolation`]'s exclusively created empty directory
/// — so nothing pinned is involved, and it can run before anything is staged. The only
/// acceptable answer is a failure naming that toolchain as not installed (measured on rustup
/// 1.29.0 from an unpinned empty directory, 2026-09-07: exit 1, `error: toolchain
/// 'renvor-uninstallable-toolchain-name' is not installed`, nothing written there).
///
/// Never called on an unidentified binary — [`fn@super::identify`] calls it only on a
/// [`Classification::Proxy`], after [`floor`] passed.
///
/// # Errors
///
/// [`Code::ToolMissing`] with `reason = no_install_guarantee_unconfirmed` for any other answer,
/// including a timeout; `probe_isolation_unavailable` when no isolation can be created.
pub fn confirm_no_install(rustc: &Path, sealed: &Sealed) -> Result<(), CliError> {
    let isolation = Isolation::create(sealed)?;
    let mut command = sealed_command(rustc.as_os_str(), sealed, isolation.cwd());
    command
        .env("RUSTUP_TOOLCHAIN", UNINSTALLABLE_NAME)
        .arg("-vV");
    let confirmed = match isolate::run_bounded(command, isolate::PROBE_TIMEOUT) {
        Ok(answer) => {
            !answer.status.success()
                && grammar::parse_not_installed_channel(&answer.stderr).as_deref()
                    == Some(UNINSTALLABLE_NAME)
        }
        Err(_) => false,
    };
    if confirmed {
        return Ok(());
    }
    Err(CliError::new(
        Code::ToolMissing,
        "`rustc` on PATH is a rustup proxy, but asked for a toolchain name that cannot be \
         installed it did not answer `is not installed`; the no-provisioning guarantee of rustup \
         1.28.1 could not be confirmed on this machine, so no check was run and nothing was \
         written to the destination",
    )
    .with("tool", RUSTUP_TOOL)
    .with("required", "true")
    .with("found", "true")
    .with("reason", "no_install_guarantee_unconfirmed")
    .with(
        "remedy",
        "update rustup to 1.28.1 or later (rustup self update) and put it first on PATH",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exit::Exit;

    fn detail<'a>(error: &'a CliError, key: &str) -> Option<&'a str> {
        error
            .details
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn the_floor_is_the_rustup_that_introduced_auto_install() {
        assert_eq!(floor_version(), semver::Version::new(1, 28, 1));
        assert_eq!(RUSTUP_FLOOR, "1.28.1");
        assert_eq!(RUSTUP_TOOL, "rustup >= 1.28.1");
    }

    #[test]
    fn classification_is_by_file_identity_and_runs_nothing() {
        let root = tempfile::tempdir().expect("tempdir");
        let rustup = root.path().join("rustup");
        std::fs::write(&rustup, "#!/bin/sh\nexit 0\n").expect("write");
        let other = root.path().join("other");
        std::fs::write(&other, "#!/bin/sh\nexit 1\n").expect("write");
        assert!(same_file(&rustup, &rustup));
        assert!(!same_file(&rustup, &other));
        assert!(!same_file(&rustup, &root.path().join("absent")));
        let version = semver::Version::new(1, 29, 0);
        assert_eq!(
            classify(&other, Some((rustup.clone(), version.clone()))),
            Classification::Bare
        );
        assert_eq!(classify(&rustup, None), Classification::Bare);
        #[cfg(unix)]
        {
            let link = root.path().join("rustc");
            std::os::unix::fs::symlink(&rustup, &link).expect("symlink");
            assert!(is_proxy_of(&link, &rustup), "a symlink is the same file");
            assert_eq!(
                classify(&link, Some((rustup.clone(), version.clone()))),
                Classification::Proxy {
                    rustup: rustup.clone(),
                    version
                }
            );
            // A COPY is not the same file on Unix: rustup's proxies there are links.
            let copy = root.path().join("copy");
            std::fs::copy(&rustup, &copy).expect("copy");
            assert!(!is_proxy_of(&copy, &rustup));
        }
        #[cfg(windows)]
        {
            let copy = root.path().join("rustc.exe");
            std::fs::copy(&rustup, &copy).expect("copy");
            assert!(
                is_proxy_of(&copy, &rustup),
                "identical bytes are the same file on Windows"
            );
        }
    }

    #[cfg(unix)]
    mod stubbed {
        use super::*;
        use crate::toolchain::testing::{PROXY_TOOLCHAIN, Stubs, UNIDENTIFIED_PROXY};
        use crate::toolchain::{Expectations, preflight};

        fn expectations() -> Expectations {
            Expectations {
                pinned: Some("1.93.0".to_owned()),
                rust_version: Some(semver::Version::new(1, 94, 0)),
            }
        }

        #[test]
        fn rustup_below_1_28_1_is_refused_before_any_proxy_runs() {
            // FR-012-7a step (2). A stub `rustup --version` answering 1.27.1 on a PATH with NO
            // cargo: the refusal names the floor, and it comes before the cargo lookup would have
            // failed — so nothing after the floor ran. The stub `rustc` records every invocation
            // and must record none.
            let stubs = Stubs::new();
            stubs.script(
                "rustup",
                "printf 'rustup 1.27.1 (54dd3d00f 2024-04-24)\\n'\nexit 0\n",
            );
            stubs.script("rustc", "printf '%s' \"$RUSTC_VV\"\nexit 0\n");
            let project = stubs.pinned_dir("1.93.0");
            let seal = stubs.sealed(&[]);
            let error = preflight(&project, &seal, &expectations()).expect_err("refused");
            assert_eq!(error.code, Code::ToolMissing);
            assert_eq!(error.exit(), Exit::Environment);
            assert_eq!(detail(&error, "tool"), Some(RUSTUP_TOOL));
            assert_eq!(detail(&error, "foundVersion"), Some("1.27.1"));
            assert_eq!(detail(&error, "requiredVersion"), Some("1.28.1"));
            assert!(detail(&error, "remedy").is_some_and(|r| r.contains("1.28.1")));
            let records = stubs.records();
            assert_eq!(records.len(), 1, "only the floor check ran");
            assert_eq!(records[0].name, "rustup");
            assert_eq!(records[0].args, "--version");
            assert!(
                records.iter().all(|record| record.name != "rustc"),
                "a proxy ran before the floor was known"
            );
        }

        #[test]
        fn an_unparseable_rustup_version_is_refused() {
            let stubs = Stubs::new();
            let rustup = stubs.script("rustup", "printf 'rustup-init 1.29.0\\n'\nexit 0\n");
            let seal = stubs.sealed(&[]);
            let error = floor(&rustup, &seal).expect_err("unparseable is refused");
            assert_eq!(error.code, Code::ToolMissing);
            assert_eq!(detail(&error, "tool"), Some(RUSTUP_TOOL));
            assert_eq!(
                detail(&error, "foundVersion"),
                Some("an unparseable version")
            );
            // A valid line with a failing exit is not an answer either.
            let stubs = Stubs::new();
            let rustup = stubs.script(
                "rustup",
                "printf 'rustup 1.29.0 (28d1352db 2026-03-05)\\n'\nexit 1\n",
            );
            let error = floor(&rustup, &stubs.sealed(&[])).expect_err("a failing exit is refused");
            assert_eq!(detail(&error, "reason"), None);
            assert_eq!(detail(&error, "tool"), Some(RUSTUP_TOOL));
            // POSITIVE CONTROL: the floor itself passes.
            let stubs = Stubs::new();
            let rustup = stubs.script(
                "rustup",
                "printf 'rustup 1.28.1 (0000000 2025-01-01)\\ninfo: ignored\\n'\nexit 0\n",
            );
            assert_eq!(
                floor(&rustup, &stubs.sealed(&[])).expect("at the floor passes"),
                semver::Version::new(1, 28, 1)
            );
        }

        #[test]
        fn the_floor_check_never_runs_rustup_where_a_toolchain_is_named() {
            // FR-012-7a step (2). The stub behaves as rustup did before 1.28.0: `--version` run
            // where a toolchain file names an absent channel, or with RUSTUP_TOOLCHAIN set,
            // "installs" it — writes a marker and connects to the dist address. The caller's
            // directory is pinned to an absent channel AND the seal carries RUSTUP_TOOLCHAIN
            // naming it; the floor check must see neither.
            let stubs = Stubs::new();
            let rustup = stubs.script(
                "rustup",
                "if [ \"$TF\" = yes ] || [ -n \"${RUSTUP_TOOLCHAIN+x}\" ]; then\n  install\n  \
                 printf 'info: installing component\\n' >&2\n  exit 1\nfi\n\
                 printf 'rustup 1.29.0 (28d1352db 2026-03-05)\\n'\nexit 0\n",
            );
            let project = stubs.pinned_dir("1.93.0");
            let seal = stubs.sealed(&[("RUSTUP_TOOLCHAIN", "1.93.0")]);
            let version = floor(&rustup, &seal).expect("the floor check passes in isolation");
            assert_eq!(version, semver::Version::new(1, 29, 0));
            assert!(
                !stubs.installed(),
                "the stub installed: the floor ran where a toolchain was named"
            );
            let records = stubs.records();
            assert_eq!(records.len(), 1);
            let record = &records[0];
            assert_eq!(record.toolchain_file, "no");
            assert_eq!(record.rustup_toolchain, "<unset>");
            assert_eq!(record.dist_server, isolate::UNROUTABLE);
            assert_eq!(record.auto_install, "0");
            let project = std::fs::canonicalize(&project).expect("canonical");
            assert_ne!(Path::new(&record.cwd), project.as_path());
            assert!(
                !Path::new(&record.cwd).starts_with(&project),
                "the floor check ran under the pinned directory"
            );
            assert!(
                !Path::new(&record.cwd).exists(),
                "the isolated directory was removed"
            );
        }

        #[test]
        fn a_rustup_beside_the_proxies_is_located_and_version_checked_first() {
            // FR-012-7a steps (1)–(2), the hidden layout: PATH holds a shim whose `rustc` links
            // into a directory where `rustup` sits, off PATH. Located beside the proxies, it is
            // version-checked BEFORE anything resolves: below the floor, the refusal is the only
            // thing that happened.
            let stubs = Stubs::new();
            let hidden = stubs.root.path().join("cargo-bin");
            std::fs::create_dir_all(&hidden).expect("mkdir");
            let old = PROXY_TOOLCHAIN.replace(
                "1.29.0 (28d1352db 2026-03-05)",
                "1.27.1 (54dd3d00f 2024-04-24)",
            );
            let rustup = stubs.script_in(&hidden, "rustup", &old);
            for tool in ["rustc", "cargo", "rustfmt"] {
                std::os::unix::fs::symlink(&rustup, hidden.join(tool)).expect("symlink");
                std::os::unix::fs::symlink(hidden.join(tool), stubs.bin.join(tool))
                    .expect("symlink");
            }
            let project = stubs.pinned_dir("1.93.0");
            let seal = stubs.sealed(&[]);
            assert!(crate::toolchain::locate::on_path(&seal, "rustup").is_none());
            assert_eq!(
                crate::toolchain::locate::rustup(&seal).and_then(|p| std::fs::canonicalize(p).ok()),
                std::fs::canonicalize(&rustup).ok()
            );
            let error = preflight(&project, &seal, &expectations()).expect_err("below the floor");
            assert_eq!(detail(&error, "tool"), Some(RUSTUP_TOOL));
            assert_eq!(detail(&error, "foundVersion"), Some("1.27.1"));
            let records = stubs.records();
            assert_eq!(records.len(), 1, "only the version check ran");
            assert_eq!(records[0].name, "rustup");
            assert_eq!(records[0].args, "--version");
            assert_eq!(records[0].toolchain_file, "no");
            // POSITIVE CONTROL, same layout, at the floor: the version check is still FIRST,
            // and the proxies are then identified and the resolution runs to completion.
            let stubs = Stubs::new();
            let hidden = stubs.root.path().join("cargo-bin");
            std::fs::create_dir_all(&hidden).expect("mkdir");
            let rustup = stubs.script_in(&hidden, "rustup", PROXY_TOOLCHAIN);
            for tool in ["rustc", "cargo", "rustfmt"] {
                std::os::unix::fs::symlink(&rustup, hidden.join(tool)).expect("symlink");
                std::os::unix::fs::symlink(hidden.join(tool), stubs.bin.join(tool))
                    .expect("symlink");
            }
            let project = stubs.pinned_dir("1.94.0");
            let seal = stubs.sealed(&[]);
            let resolution = preflight(&project, &seal, &expectations()).expect("resolves");
            assert!(resolution.proxy);
            assert_eq!(resolution.rustup, Some(semver::Version::new(1, 29, 0)));
            assert_eq!(
                resolution.selected_by,
                crate::toolchain::SelectedBy::ToolchainFile
            );
            let records = stubs.records();
            assert_eq!(records[0].name, "rustup");
            assert_eq!(records[0].args, "--version");
            assert_eq!(records[0].toolchain_file, "no");
            assert!(records.len() > 1, "the resolution ran after the floor");
        }

        #[test]
        fn an_unidentified_proxy_is_refused_before_any_resolution() {
            // FR-012-7a step (4). No rustup anywhere; `rustc` on PATH is a stub that answers in
            // rustup's words when RUSTUP_HOME is empty and "installs" whenever it runs where a
            // toolchain file names an absent channel. The generator runs it exactly once, in the
            // isolation — never in the pinned directory — and refuses by name.
            let stubs = Stubs::new();
            let script = stubs.script("rustc", UNIDENTIFIED_PROXY);
            std::os::unix::fs::symlink(&script, stubs.bin.join("cargo")).expect("symlink");
            let project = stubs.pinned_dir("1.93.0");
            let seal = stubs.sealed(&[]);
            let error = preflight(&project, &seal, &expectations()).expect_err("refused");
            assert_eq!(error.code, Code::ToolMissing);
            assert_eq!(error.exit(), Exit::Environment);
            assert_eq!(detail(&error, "reason"), Some("proxy_unidentified"));
            assert_eq!(detail(&error, "tool"), Some(RUSTUP_TOOL));
            assert!(detail(&error, "remedy").is_some_and(|r| r.contains("first on PATH")));
            assert!(
                !stubs.installed(),
                "the stub installed: it ran where a toolchain was named"
            );
            let records = stubs.records();
            assert_eq!(records.len(), 1, "only the isolated probe ran");
            assert_eq!(records[0].name, "rustc");
            assert_eq!(records[0].args, "-vV");
            assert_eq!(records[0].toolchain_file, "no");
            assert_eq!(records[0].rustup_toolchain, "<unset>");
            assert_eq!(records[0].rustup_home_entries, "0");
            assert_eq!(records[0].dist_server, isolate::UNROUTABLE);
            assert!(!Path::new(&records[0].cwd).exists());
        }

        #[test]
        fn a_bare_compiler_passes_the_isolated_identification_probe() {
            // A bare compiler ignores every RUSTUP_* variable and prints its identity.
            let stubs = Stubs::new();
            let rustc = stubs.script("rustc", "printf '%s' \"$RUSTC_VV\"\nexit 0\n");
            let seal = stubs.sealed(&[("RUSTUP_TOOLCHAIN", "1.93.0")]);
            let identity = isolated_probe(&rustc, Tool::Rustc, &seal).expect("bare");
            assert_eq!(identity.release, "1.94.0");
            assert_eq!(identity.host, "aarch64-apple-darwin");
            let records = stubs.records();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].rustup_toolchain, "<unset>");
            // And through the preflight: `proxy = false`, `no_rustup`.
            stubs.script(
                "cargo",
                "case \"$1\" in -vV) printf '%s' \"$CARGO_VV\";; clippy) printf '%s\\n' \"$CLIPPY_VERSION\";; *) exit 1;; esac\nexit 0\n",
            );
            stubs.script("rustfmt", "printf '%s\\n' \"$RUSTFMT_VERSION\"\nexit 0\n");
            let project = stubs.pinned_dir("1.93.0");
            let resolution = preflight(&project, &seal, &expectations()).expect("resolves");
            assert!(!resolution.proxy);
            assert_eq!(resolution.rustup, None);
            assert_eq!(
                resolution.selected_by,
                crate::toolchain::SelectedBy::NoRustup
            );
            // Anything that is neither an identity nor rustup's words is unreadable, by name.
            let stubs = Stubs::new();
            let rustc = stubs.script(
                "rustc",
                "printf 'rustc: command not understood\\n' >&2\nexit 2\n",
            );
            let error =
                isolated_probe(&rustc, Tool::Rustc, &stubs.sealed(&[])).expect_err("unreadable");
            assert_eq!(error.code, Code::ProjectVerificationFailed);
            assert_eq!(
                detail(&error, "reason"),
                Some("compiler_identity_unreadable")
            );
        }

        #[test]
        fn the_uninstallable_name_confirmation_runs_only_on_an_identified_proxy() {
            // FR-012-7c. (a) The unidentified stub is never invoked with RUSTUP_TOOLCHAIN set.
            let stubs = Stubs::new();
            let script = stubs.script("rustc", UNIDENTIFIED_PROXY);
            std::os::unix::fs::symlink(&script, stubs.bin.join("cargo")).expect("symlink");
            let project = stubs.pinned_dir("1.93.0");
            let error =
                preflight(&project, &stubs.sealed(&[]), &expectations()).expect_err("refused");
            assert_eq!(detail(&error, "reason"), Some("proxy_unidentified"));
            assert!(
                stubs
                    .records()
                    .iter()
                    .all(|record| record.rustup_toolchain == "<unset>"),
                "an unidentified binary was asked for a toolchain"
            );
            // (b) A bare compiler is never asked either.
            let stubs = Stubs::new();
            stubs.script("rustc", "printf '%s' \"$RUSTC_VV\"\nexit 0\n");
            stubs.script(
                "cargo",
                "case \"$1\" in -vV) printf '%s' \"$CARGO_VV\";; clippy) printf '%s\\n' \"$CLIPPY_VERSION\";; *) exit 1;; esac\nexit 0\n",
            );
            stubs.script("rustfmt", "printf '%s\\n' \"$RUSTFMT_VERSION\"\nexit 0\n");
            let project = stubs.pinned_dir("1.93.0");
            preflight(&project, &stubs.sealed(&[]), &expectations())
                .expect("a bare toolchain resolves");
            assert!(
                stubs
                    .records()
                    .iter()
                    .all(|record| record.rustup_toolchain == "<unset>")
            );
            // (c) An identified proxy at the floor IS asked, once, in the caller's directory,
            // under the seal — and answers `is not installed`.
            let stubs = Stubs::new();
            let rustup = stubs.script("rustup", PROXY_TOOLCHAIN);
            for tool in ["rustc", "cargo", "rustfmt"] {
                std::os::unix::fs::symlink(&rustup, stubs.bin.join(tool)).expect("symlink");
            }
            let project = stubs.pinned_dir("1.94.0");
            let resolution =
                preflight(&project, &stubs.sealed(&[]), &expectations()).expect("resolves");
            assert!(resolution.proxy);
            let asked: Vec<_> = stubs
                .records()
                .into_iter()
                .filter(|record| record.rustup_toolchain == UNINSTALLABLE_NAME)
                .collect();
            assert_eq!(asked.len(), 1, "the confirmation runs exactly once");
            assert_eq!(asked[0].name, "rustc");
            assert_eq!(asked[0].args, "-vV");
            // In an isolation's empty directory — never the pinned one — under the SEAL: the
            // operator's homes, `RUSTUP_AUTO_INSTALL=0` forced, the install servers absent.
            let project = std::fs::canonicalize(&project).expect("canonical");
            assert_ne!(Path::new(&asked[0].cwd), project.as_path());
            assert_eq!(asked[0].toolchain_file, "no");
            assert_eq!(asked[0].cwd_entries, "0");
            assert!(
                !Path::new(&asked[0].cwd).exists(),
                "the isolation was removed"
            );
            assert_eq!(
                asked[0].rustup_home, "<unset>",
                "the seal's homes, not fresh ones"
            );
            assert_eq!(asked[0].auto_install, "0");
            assert_eq!(asked[0].dist_server, "<unset>");
            assert_eq!(asked[0].update_root, "<unset>");
            // (d) Directly: a proxy that answers anything else fails the confirmation by name.
            let stubs = Stubs::new();
            let rustc = stubs.script("rustc", "printf '%s' \"$RUSTC_VV\"\nexit 0\n");
            let error = confirm_no_install(&rustc, &stubs.sealed(&[]))
                .expect_err("an identity is not the answer");
            assert_eq!(error.code, Code::ToolMissing);
            assert_eq!(
                detail(&error, "reason"),
                Some("no_install_guarantee_unconfirmed")
            );
            assert_eq!(detail(&error, "tool"), Some(RUSTUP_TOOL));
        }
    }
}
