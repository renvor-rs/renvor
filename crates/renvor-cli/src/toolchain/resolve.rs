//! FR-012-7b: what resolves in the measured directory, under the seal — after FR-012-7a has
//! identified the tools, never before.
//!
//! The resolution probe is `rustc -vV` and `cargo -vV` **inside** the directory whose selection
//! is being measured (§5.4: the staging directory for `renvor new`, the project directory and
//! then the scratch copy for `generate auth`), then the two component queries, then — for an
//! identified proxy — `rustup show active-toolchain` for the attribution. Every child runs
//! through [`sealed_command`] with a bounded wait; every answer is parsed under
//! [`super::grammar`] before any part of it is kept.
//!
//! # What the refusals say, and what they do not
//!
//! - rustup's `toolchain '<channel>' is not installed` is `tool_missing`, exit 5, naming the
//!   channel and the `rustup toolchain install` remedy — before any check runs and before
//!   anything is placed. Nothing resolves to "whatever the default is" (SR-012-2).
//! - a resolved compiler below the manifest's `rust-version` is the same refusal naming
//!   `rustc >= <msrv>`: **the generator's prerequisite check of the compiler the preflight
//!   resolution names** — not a statement about Cargo's effective compiler under `RUSTC`,
//!   `build.rustc`, or a wrapper, and not a prediction of what Cargo would do later (A-8).
//! - a toolchain without `rustfmt` or `clippy` is `tool_missing` naming the component and its
//!   `rustup component add` remedy. `cargo clippy --version` is parsed for **presence only**; its
//!   answer never becomes a driver identity (that is FR-012-7d's separate query of the observed
//!   `clippy-driver`), and no [`super::DriverIdentity`] leaves this module.
//! - `selected_by` is rustup's own attribution, from the pinned table; unknown text is
//!   `unknown`; a bare compiler is `no_rustup`. The path rustup prints is never stored.

use std::ffi::OsStr;
use std::path::Path;
use std::time::Duration;

use crate::exit::{CliError, Code};
use crate::generate::verify::{Sealed, sealed_command};
use crate::toolchain::isolate::{self, Answer, RunError};
use crate::toolchain::{
    Classification, Expectations, Resolution, SelectedBy, config, grammar, locate, tool_absent,
    unreadable,
};

/// How long a resolution query may take: the identity queries of FR-012-7b/7d answer at once;
/// a minute is a wedged proxy, not a slow one.
pub const RESOLUTION_TIMEOUT: Duration = Duration::from_secs(60);

/// The refusal of a pinned-but-absent toolchain (FR-012-7b): exit 5, the channel, the remedy.
fn toolchain_absent(channel: &str) -> CliError {
    CliError::new(
        Code::ToolMissing,
        format!(
            "the toolchain this directory selects, {channel}, is not installed; rustup answered \
             so by name and downloaded nothing (RUSTUP_AUTO_INSTALL=0). No check was run and \
             nothing was written to the destination. Install it with `rustup toolchain install \
             {channel} --component rustfmt --component clippy --profile minimal`"
        ),
    )
    .with("tool", format!("rustup toolchain {channel}"))
    .with("required", "true")
    .with("found", "false")
    .with(
        "remedy",
        format!(
            "rustup toolchain install {channel} --component rustfmt --component clippy --profile minimal"
        ),
    )
}

/// The refusal of a missing component (FR-012-7b).
fn component_absent(component: &str, pinned: Option<&str>) -> CliError {
    let remedy = match pinned {
        Some(channel) => format!("rustup component add {component} --toolchain {channel}"),
        None => format!("rustup component add {component}"),
    };
    CliError::new(
        Code::ToolMissing,
        format!(
            "the toolchain this directory selects has no `{component}`, which verification \
             requires; no check was run and nothing was written to the destination. Add it \
             with `{remedy}`"
        ),
    )
    .with("tool", component)
    .with("required", "true")
    .with("found", "false")
    .with("remedy", remedy)
}

/// The prerequisite refusal (FR-012-7b, A-8): the resolved compiler is below the MSRV.
fn below_msrv(release: &str, msrv: &semver::Version) -> CliError {
    CliError::new(
        Code::ToolMissing,
        format!(
            "the generator's prerequisite check of the resolved compiler: `rustc -vV` in the \
             directory being verified answers release {release}, below the {msrv} the generated \
             project declares as `rust-version`. This is the compiler the preflight resolution \
             names on PATH, checked before any check runs; it is not a statement about Cargo's \
             effective compiler under `RUSTC`, `build.rustc`, or a wrapper. No check was run and \
             nothing was written to the destination"
        ),
    )
    .with("tool", format!("rustc >= {msrv}"))
    .with("required", "true")
    .with("found", "true")
    .with("foundVersion", release)
    .with("requiredVersion", msrv.to_string())
    .with(
        "remedy",
        format!("select a toolchain at or above {msrv} in this directory"),
    )
}

/// One bounded query under the seal in `dir`. A child that cannot be started is the tool's
/// absence; one that does not answer in time is an unreadable identity.
fn query(
    program: &OsStr,
    arguments: &[&str],
    dir: &Path,
    sealed: &Sealed,
    label: &str,
    tool: &str,
) -> Result<Answer, CliError> {
    let mut command = sealed_command(program, sealed, dir);
    command.args(arguments);
    match isolate::run_bounded(command, RESOLUTION_TIMEOUT) {
        Ok(answer) => Ok(answer),
        Err(RunError::TimedOut(_) | RunError::Wait(_)) => Err(unreadable(label)),
        Err(RunError::Spawn(_)) => Err(tool_absent(tool)),
    }
}

/// A failed `-vV`: rustup's refusal by name when it is one, else unreadable.
fn failed_identity(answer: &Answer, label: &str) -> CliError {
    match grammar::parse_not_installed_channel(&answer.stderr) {
        Some(channel) => toolchain_absent(&channel),
        None => unreadable(label),
    }
}

/// Whether the sealed variables carry `name` with a non-empty value — presence, never the value.
fn present(sealed: &Sealed, name: &str) -> bool {
    locate::variable(sealed, name).is_some_and(|value| !value.is_empty())
}

/// The resolution in `dir` (FR-012-7b), for tools FR-012-7a has identified.
///
/// # Errors
///
/// [`Code::ToolMissing`] for an absent toolchain, a compiler below the MSRV, or a missing
/// component; [`Code::ProjectVerificationFailed`] with `reason = compiler_identity_unreadable`
/// for an answer outside the grammar or a deadline.
pub fn in_directory(
    dir: &Path,
    sealed: &Sealed,
    classification: &Classification,
    expectations: &Expectations,
) -> Result<Resolution, CliError> {
    let rustc_path = locate::on_path(sealed, "rustc").ok_or_else(|| tool_absent("rustc"))?;
    let cargo_path = locate::on_path(sealed, "cargo").ok_or_else(|| tool_absent("cargo"))?;

    // `rustc -vV`, then `cargo -vV`, in the directory, under the seal.
    let answer = query(
        rustc_path.as_os_str(),
        &["-vV"],
        dir,
        sealed,
        "rustc -vV",
        "rustc",
    )?;
    if !answer.status.success() {
        return Err(failed_identity(&answer, "rustc -vV"));
    }
    let rustc = grammar::parse_rustc_vv(&answer.stdout).map_err(|_| unreadable("rustc -vV"))?;
    let answer = query(
        cargo_path.as_os_str(),
        &["-vV"],
        dir,
        sealed,
        "cargo -vV",
        "cargo",
    )?;
    if !answer.status.success() {
        return Err(failed_identity(&answer, "cargo -vV"));
    }
    let cargo = grammar::parse_cargo_vv(&answer.stdout).map_err(|_| unreadable("cargo -vV"))?;

    // The prerequisite: the resolved release against the manifest's `rust-version`. The
    // pre-release tag is dropped for the comparison so a nightly of the MSRV's own number is
    // not "below" it; the numbers are what `rust-version` states.
    if let Some(msrv) = &expectations.rust_version {
        let resolved =
            semver::Version::parse(&rustc.release).map_err(|_| unreadable("rustc -vV"))?;
        let numbers = semver::Version::new(resolved.major, resolved.minor, resolved.patch);
        if numbers < *msrv {
            return Err(below_msrv(&rustc.release, msrv));
        }
    }

    // The components, presence only.
    let pinned = expectations.pinned.as_deref();
    let rustfmt_path =
        locate::on_path(sealed, "rustfmt").ok_or_else(|| component_absent("rustfmt", pinned))?;
    let answer = query(
        rustfmt_path.as_os_str(),
        &["--version"],
        dir,
        sealed,
        "rustfmt --version",
        "rustfmt",
    )?;
    if !answer.status.success() {
        return Err(component_absent("rustfmt", pinned));
    }
    let answer = query(
        cargo_path.as_os_str(),
        &["clippy", "--version"],
        dir,
        sealed,
        "cargo clippy --version",
        "cargo",
    )?;
    if !answer.status.success() {
        return Err(component_absent("clippy", pinned));
    }
    // Parsed under the grammar for the record's sake — the answer is untrusted text like any
    // other — and then DROPPED: the driver identity is FR-012-7d's, from the observed executable.
    grammar::parse_clippy_version(&answer.stdout)
        .map_err(|_| unreadable("cargo clippy --version"))?;

    // The attribution, for an identified proxy; a bare compiler is `no_rustup` by definition.
    let (rustup, proxy, selected_by) = match classification {
        Classification::Proxy { rustup, version } => {
            let attribution = query(
                rustup.as_os_str(),
                &["show", "active-toolchain"],
                dir,
                sealed,
                "rustup show active-toolchain",
                "rustup",
            )
            .ok()
            .filter(|answer| answer.status.success())
            .map_or(SelectedBy::Unknown, |answer| {
                grammar::parse_active_toolchain_attribution(&answer.stdout)
            });
            (Some(version.clone()), true, attribution)
        }
        Classification::Bare => (None, false, SelectedBy::NoRustup),
    };

    let configured = config::presence(dir, sealed);
    Ok(Resolution {
        rustc,
        cargo,
        rustup,
        proxy,
        selected_by,
        rustc_override: present(sealed, "RUSTC") || configured.rustc,
        wrapper: present(sealed, "RUSTC_WRAPPER")
            || present(sealed, "RUSTC_WORKSPACE_WRAPPER")
            || configured.rustc_wrapper
            || configured.rustc_workspace_wrapper,
        rustflags: present(sealed, "RUSTFLAGS"),
        rustdocflags: present(sealed, "RUSTDOCFLAGS"),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::exit::Exit;
    use crate::generate::verify::seal;
    use crate::toolchain::testing::{PROXY_TOOLCHAIN, Stubs};
    use crate::toolchain::{Identity, preflight};

    fn detail<'a>(error: &'a CliError, key: &str) -> Option<&'a str> {
        error
            .details
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    fn expectations(pinned: Option<&str>) -> Expectations {
        Expectations {
            pinned: pinned.map(str::to_owned),
            rust_version: Some(semver::Version::new(1, 94, 0)),
        }
    }

    /// A bare stub toolchain: `rustc`, `cargo` (with `clippy`), `rustfmt`, answering the
    /// measured identities.
    fn bare_stubs(stubs: &Stubs) {
        stubs.script("rustc", "printf '%s' \"$RUSTC_VV\"\nexit 0\n");
        stubs.script(
            "cargo",
            "case \"$1\" in -vV) printf '%s' \"$CARGO_VV\";; clippy) printf '%s\\n' \"$CLIPPY_VERSION\";; *) exit 1;; esac\nexit 0\n",
        );
        stubs.script("rustfmt", "printf '%s\\n' \"$RUSTFMT_VERSION\"\nexit 0\n");
    }

    #[test]
    fn a_pinned_but_absent_toolchain_is_tool_missing_not_a_compiler_error() {
        let stubs = Stubs::new();
        stubs.script(
            "rustc",
            "printf \"error: toolchain '1.93.0-aarch64-apple-darwin' is not installed\\nhelp: run \\`rustup toolchain install\\` to install it\\n\" >&2\nexit 1\n",
        );
        stubs.script("cargo", "exit 1\n");
        let project = stubs.pinned_dir("1.93.0");
        let error = in_directory(
            &project,
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(Some("1.93.0")),
        )
        .expect_err("refused");
        assert_eq!(error.code, Code::ToolMissing);
        assert_eq!(error.exit(), Exit::Environment);
        assert_eq!(
            detail(&error, "tool"),
            Some("rustup toolchain 1.93.0-aarch64-apple-darwin")
        );
        assert_eq!(
            detail(&error, "remedy"),
            Some(
                "rustup toolchain install 1.93.0-aarch64-apple-darwin --component rustfmt --component clippy --profile minimal"
            )
        );
        assert!(
            !error.message.contains("compile"),
            "this is not a compiler error"
        );
        assert_eq!(
            stubs.records().len(),
            1,
            "cargo was not asked after rustc refused"
        );
        // rustup's other words, or garbage, are unreadable — and never a guessed channel.
        let stubs = Stubs::new();
        stubs.script(
            "rustc",
            "printf 'error: no default toolchain configured\\n' >&2\nexit 1\n",
        );
        stubs.script("cargo", "exit 1\n");
        let error = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(None),
        )
        .expect_err("refused");
        assert_eq!(error.code, Code::ProjectVerificationFailed);
        assert_eq!(
            detail(&error, "reason"),
            Some("compiler_identity_unreadable")
        );
        assert_eq!(detail(&error, "check"), Some("rustc -vV"));
    }

    #[test]
    fn a_compiler_below_the_msrv_is_refused_by_name() {
        let stubs = Stubs::new();
        bare_stubs(&stubs);
        let _ = std::fs::remove_file(stubs.bin.join("rustc"));
        stubs.script(
            "rustc",
            "printf '%s' \"$RUSTC_VV\" | sed 's/release: 1.94.0/release: 1.93.0/'\nexit 0\n",
        );
        let error = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(Some("1.94.0")),
        )
        .expect_err("below the MSRV is refused");
        assert_eq!(error.code, Code::ToolMissing);
        assert_eq!(detail(&error, "tool"), Some("rustc >= 1.94.0"));
        assert_eq!(detail(&error, "foundVersion"), Some("1.93.0"));
        assert_eq!(detail(&error, "requiredVersion"), Some("1.94.0"));
        assert!(
            error
                .message
                .contains("prerequisite check of the resolved compiler")
        );
        assert!(
            !error.message.contains("would refuse"),
            "no prediction about Cargo"
        );
        assert!(
            !error.message.contains("later"),
            "no prediction about Cargo"
        );
        // A nightly of the MSRV's own number is not below it; a legacy tree checks nothing.
        let stubs = Stubs::new();
        bare_stubs(&stubs);
        let _ = std::fs::remove_file(stubs.bin.join("rustc"));
        stubs.script(
            "rustc",
            "printf '%s' \"$RUSTC_VV\" | sed 's/release: 1.94.0/release: 1.94.0-nightly/'\nexit 0\n",
        );
        let resolution = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(None),
        )
        .expect("resolves");
        assert_eq!(resolution.rustc.release, "1.94.0-nightly");
        in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(Some("1.94.0")),
        )
        .expect("a nightly of the MSRV's number passes the prerequisite");
    }

    #[test]
    fn a_pin_without_rustfmt_or_clippy_is_refused_naming_the_component() {
        // No rustfmt on PATH at all.
        let stubs = Stubs::new();
        bare_stubs(&stubs);
        let _ = std::fs::remove_file(stubs.bin.join("rustfmt"));
        let error = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(Some("1.94.0")),
        )
        .expect_err("no rustfmt");
        assert_eq!(error.code, Code::ToolMissing);
        assert_eq!(detail(&error, "tool"), Some("rustfmt"));
        assert_eq!(
            detail(&error, "remedy"),
            Some("rustup component add rustfmt --toolchain 1.94.0")
        );
        // A rustfmt proxy whose component is absent answers as rustup does.
        let stubs = Stubs::new();
        bare_stubs(&stubs);
        let _ = std::fs::remove_file(stubs.bin.join("rustfmt"));
        stubs.script(
            "rustfmt",
            "printf \"error: 'rustfmt' is not installed for the toolchain '1.94.0-aarch64-apple-darwin'\\n\" >&2\nexit 1\n",
        );
        let error = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(None),
        )
        .expect_err("rustfmt absent");
        assert_eq!(detail(&error, "tool"), Some("rustfmt"));
        assert_eq!(
            detail(&error, "remedy"),
            Some("rustup component add rustfmt")
        );
        // `cargo clippy --version` failing names clippy.
        let stubs = Stubs::new();
        bare_stubs(&stubs);
        let _ = std::fs::remove_file(stubs.bin.join("cargo"));
        stubs.script(
            "cargo",
            "case \"$1\" in -vV) printf '%s' \"$CARGO_VV\"; exit 0;; esac\nprintf \"error: 'cargo-clippy' is not installed for the toolchain '1.94.0-aarch64-apple-darwin'\\n\" >&2\nexit 1\n",
        );
        let error = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(Some("1.94.0")),
        )
        .expect_err("clippy absent");
        assert_eq!(detail(&error, "tool"), Some("clippy"));
        assert_eq!(
            detail(&error, "remedy"),
            Some("rustup component add clippy --toolchain 1.94.0")
        );
        // A clippy answer outside the grammar is unreadable, and no driver identity exists to
        // be filled from it: `Resolution` has no such field.
        let stubs = Stubs::new();
        bare_stubs(&stubs);
        let _ = std::fs::remove_file(stubs.bin.join("cargo"));
        stubs.script(
            "cargo",
            "case \"$1\" in -vV) printf '%s' \"$CARGO_VV\";; clippy) printf 'clippy something else\\n';; esac\nexit 0\n",
        );
        let error = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(None),
        )
        .expect_err("unreadable clippy");
        assert_eq!(
            detail(&error, "reason"),
            Some("compiler_identity_unreadable")
        );
        assert_eq!(detail(&error, "check"), Some("cargo clippy --version"));
    }

    #[test]
    fn override_and_wrapper_presence_is_recorded_from_the_seal_and_the_configuration() {
        let stubs = Stubs::new();
        bare_stubs(&stubs);
        let none = in_directory(
            stubs.root.path(),
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(None),
        )
        .expect("resolves");
        assert!(!none.rustc_override && !none.wrapper && !none.rustflags && !none.rustdocflags);
        let seal = stubs.sealed(&[
            ("RUSTC", "/opt/other/rustc"),
            ("RUSTC_WORKSPACE_WRAPPER", "/opt/wrap"),
            ("RUSTFLAGS", "-C target-cpu=native"),
            ("RUSTDOCFLAGS", ""),
        ]);
        let some = in_directory(
            stubs.root.path(),
            &seal,
            &Classification::Bare,
            &expectations(None),
        )
        .expect("resolves");
        assert!(some.rustc_override && some.wrapper && some.rustflags);
        assert!(!some.rustdocflags, "an empty value is not a presence");
        let project = stubs.root.path().join("configured");
        std::fs::create_dir_all(project.join(".cargo")).expect("mkdir");
        std::fs::write(
            project.join(".cargo").join("config.toml"),
            "[build]\nrustc-wrapper = \"sccache\"\n",
        )
        .expect("write");
        let configured = in_directory(
            &project,
            &stubs.sealed(&[]),
            &Classification::Bare,
            &expectations(None),
        )
        .expect("resolves");
        assert!(configured.wrapper && !configured.rustc_override);
    }

    #[test]
    fn a_proxy_is_attributed_by_rustups_own_words_and_unknown_text_is_unknown() {
        let stubs = Stubs::new();
        let rustup = stubs.script("rustup", PROXY_TOOLCHAIN);
        for tool in ["rustc", "cargo", "rustfmt"] {
            std::os::unix::fs::symlink(&rustup, stubs.bin.join(tool)).expect("symlink");
        }
        let proxy = Classification::Proxy {
            rustup: rustup.clone(),
            version: semver::Version::new(1, 29, 0),
        };
        let project = stubs.pinned_dir("1.94.0");
        let resolution = in_directory(
            &project,
            &stubs.sealed(&[]),
            &proxy,
            &expectations(Some("1.94.0")),
        )
        .expect("resolves");
        assert!(resolution.proxy);
        assert_eq!(resolution.rustup, Some(semver::Version::new(1, 29, 0)));
        assert_eq!(resolution.selected_by, SelectedBy::ToolchainFile);
        let seal = stubs.sealed(&[("RUSTUP_TOOLCHAIN", "1.94.0")]);
        let resolution =
            in_directory(&project, &seal, &proxy, &expectations(None)).expect("resolves");
        assert_eq!(resolution.selected_by, SelectedBy::Environment);
        // Text the table does not pin is `unknown`, never a guess; so is a failing query.
        let stubs = Stubs::new();
        let rustup = stubs.script(
            "rustup",
            &PROXY_TOOLCHAIN.replace("overridden by", "chosen by the operator at"),
        );
        for tool in ["rustc", "cargo", "rustfmt"] {
            std::os::unix::fs::symlink(&rustup, stubs.bin.join(tool)).expect("symlink");
        }
        let proxy = Classification::Proxy {
            rustup: rustup.clone(),
            version: semver::Version::new(1, 29, 0),
        };
        let project = stubs.pinned_dir("1.94.0");
        let resolution = in_directory(&project, &stubs.sealed(&[]), &proxy, &expectations(None))
            .expect("resolves");
        assert_eq!(resolution.selected_by, SelectedBy::Unknown);
    }

    /// This process's own `rustc -vV`, as the identity the real-toolchain tests compare to.
    fn own_rustc() -> Identity {
        let output = std::process::Command::new("rustc")
            .arg("-vV")
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()
            .expect("this process's rustc runs");
        grammar::parse_rustc_vv(&String::from_utf8_lossy(&output.stdout))
            .expect("its answer parses")
    }

    #[test]
    fn a_bare_toolchain_is_recorded_as_no_rustup() {
        // FR-012-7c. A directory of symlinks to the real toolchain's own binaries — the sysroot
        // `rustc --print sysroot` names — with rustup off PATH and unreachable: `proxy = false`,
        // `selected_by = no_rustup`, the identity the toolchain itself answers.
        let output = std::process::Command::new("rustc")
            .args(["--print", "sysroot"])
            .env("RUSTUP_AUTO_INSTALL", "0")
            .output()
            .expect("rustc runs");
        assert!(output.status.success(), "the sysroot query failed");
        let sysroot = std::path::PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        let stubs = Stubs::new();
        for tool in [
            "rustc",
            "cargo",
            "rustfmt",
            "cargo-clippy",
            "clippy-driver",
            "cargo-fmt",
        ] {
            let real = sysroot.join("bin").join(tool);
            assert!(
                real.is_file(),
                "the toolchain lacks a binary this test links"
            );
            std::os::unix::fs::symlink(&real, stubs.bin.join(tool)).expect("symlink");
        }
        let project = stubs.pinned_dir("1.94.0");
        let seal = stubs.sealed(&[]);
        assert!(
            locate::rustup(&seal).is_none(),
            "rustup must be unreachable in this layout"
        );
        let resolution = preflight(&project, &seal, &expectations(Some("1.94.0")))
            .expect("a bare toolchain resolves");
        assert!(!resolution.proxy);
        assert_eq!(resolution.rustup, None);
        assert_eq!(resolution.selected_by, SelectedBy::NoRustup);
        let own = own_rustc();
        assert_eq!(resolution.rustc.release, own.release);
        assert_eq!(resolution.rustc.commit, own.commit);
        assert_eq!(resolution.rustc.host, own.host);
    }

    #[test]
    fn a_hidden_rustup_with_proxies_on_path_is_not_no_rustup() {
        // FR-012-7c (D-L2-6). A copy of the real rustup binary in a directory OFF PATH, with
        // `rustc`/`cargo`/`rustfmt` beside it as links to it (the `~/.cargo/bin` layout), and a
        // shim directory ON PATH whose entries link into it: hiding `rustup` from PATH proves
        // nothing — the proxies are located, identified, version-checked, confirmed, and the
        // resolution carries rustup's real attribution, never `no_rustup`.
        let process = seal(std::env::vars_os());
        let real_rustup = locate::rustup(&process).expect("this machine has a rustup to copy");
        let stubs = Stubs::new();
        let hidden = stubs.root.path().join("cargo-bin");
        std::fs::create_dir_all(&hidden).expect("mkdir");
        let copied = hidden.join("rustup");
        std::fs::copy(&real_rustup, &copied).expect("copy the rustup binary");
        for tool in ["rustc", "cargo", "rustfmt"] {
            std::os::unix::fs::symlink(Path::new("./rustup"), hidden.join(tool)).expect("symlink");
            std::os::unix::fs::symlink(hidden.join(tool), stubs.bin.join(tool)).expect("symlink");
        }
        // The real homes, so the copied rustup finds the installed toolchains; the shim as the
        // only PATH entry; the process's own RUSTUP_TOOLCHAIN if it has one (CI selects the leg
        // that way), else the pin.
        let mut variables: Vec<(std::ffi::OsString, std::ffi::OsString)> = process
            .variables
            .iter()
            .filter(|(name, _)| {
                [
                    "HOME",
                    "USERPROFILE",
                    "RUSTUP_HOME",
                    "CARGO_HOME",
                    "RUSTUP_TOOLCHAIN",
                ]
                .iter()
                .any(|keep| name == keep)
            })
            .cloned()
            .collect();
        variables.push(("PATH".into(), stubs.bin.clone().into()));
        let seal = Sealed {
            variables,
            credentials: Vec::new(),
        };
        assert!(
            locate::on_path(&seal, "rustup").is_none(),
            "rustup is off PATH"
        );
        assert_eq!(
            locate::rustup(&seal).and_then(|p| std::fs::canonicalize(p).ok()),
            std::fs::canonicalize(&copied).ok()
        );
        let project = stubs.pinned_dir("1.94.0");
        let resolution =
            preflight(&project, &seal, &expectations(Some("1.94.0"))).expect("the proxies resolve");
        assert!(resolution.proxy, "proxies of a hidden rustup are proxies");
        assert!(
            resolution
                .rustup
                .is_some_and(|version| version >= semver::Version::new(1, 28, 1))
        );
        assert_ne!(resolution.selected_by, SelectedBy::NoRustup);
        assert_ne!(
            resolution.selected_by,
            SelectedBy::Unknown,
            "the real attribution is pinned"
        );
        let own = own_rustc();
        assert_eq!(resolution.rustc.release, own.release);
        assert_eq!(resolution.rustc.commit, own.commit);
    }
}
