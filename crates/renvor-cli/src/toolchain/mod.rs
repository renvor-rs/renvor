//! The generated project's toolchain: the pin, the record's evidence, and the sealed resolution.
//!
//! Phase 012, L-2 (`governance/phase-012-specification-and-decision-brief.md` §5, as amended in
//! its §4.3.1 and §4.3.2). Everything a verification learns about the compiler is one of two
//! things, and the record keeps them apart:
//!
//! - a **queried identity** — what a tool answered to its own supported version query
//!   (`rustc -vV`, `cargo -vV`, `clippy-driver --version`), parsed under the strict grammar of
//!   FR-012-7e before it enters anything; and
//! - a **launch observation** — what Cargo's own `-vv` output says it launched for the project's
//!   units, or that it launched nothing because the units were positively `Fresh`.
//!
//! Together they are *launch observation plus queried identity* — never proof of fresh compiler
//! execution through a wrapper (SR-012-3). Nothing here installs, lists, updates, or sets a
//! rustup default (SR-012-4); every child runs with `RUSTUP_AUTO_INSTALL=0` and without the two
//! install-server variables (FR-012-6).
//!
//! # Module map
//!
//! | Module | Requirement |
//! |---|---|
//! | [`grammar`] | FR-012-7e — the parsers; nothing unparsed reaches a record or a stream |
//! | [`pin`] | FR-012-1/2 — the framework checkout's channel and MSRV, each read on its own |
//! | [`locate`] | FR-012-7a step (1) — locate `rustup` without executing a proxy |
//! | [`isolate`] | FR-012-7a steps (2) and (4) — exclusively created empty directories |
//! | [`mod@identify`] | FR-012-7a steps (2)–(4), FR-012-7c — the floor, the classification, the probe |
//! | [`mod@resolve`] | FR-012-7b — what resolves in the measured directory, under the seal |
//! | [`config`] | override and wrapper **presence** in Cargo's configuration, parsed never evaluated |
//! | [`evidence`] | FR-012-7d — the `-vv` streams, the chains, the counts, the driver query |
//! | [`notice`] | FR-012-8 — the two stderr notices and the cached-artifacts line |

pub mod config;
pub mod evidence;
pub mod grammar;
pub mod identify;
pub mod isolate;
pub mod locate;
pub mod notice;
pub mod pin;
pub mod resolve;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::exit::{CliError, Code};
use crate::generate::verify::Sealed;

/// The rustup release below which the generator refuses to run a proxy (FR-012-7a step (2)):
/// 1.28.1 introduced `RUSTUP_AUTO_INSTALL`, the variable the no-provisioning guarantee rests on.
pub const RUSTUP_FLOOR: &str = "1.28.1";

/// The scope rule `tree_digest` is computed under (FR-012-5d). A reader that meets a number it
/// does not know refuses the record by name.
pub const TREE_SCOPE: u32 = 1;

/// The record format this generator writes and the newest it reads (FR-012-5b).
pub const RECORD_VERSION: u32 = 2;

/// A compiler identity parsed under FR-012-7e: `release`, `commit`, and `host`, never one string
/// and never raw child output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// `1.94.0`, or `1.99.0-nightly` — the `release:` line.
    pub release: String,
    /// The `commit-hash:` line — seven to forty hexadecimal digits, or `unknown` (a distribution
    /// compiler may print that).
    pub commit: String,
    /// The `host:` triple.
    pub host: String,
}

/// `cargo -vV`, parsed the same way; the host is not recorded for cargo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CargoIdentity {
    /// The `release:` line.
    pub release: String,
    /// The `commit-hash:` line, or `unknown`.
    pub commit: String,
}

/// The observed `clippy-driver` executable's answer to `clippy-driver --version`
/// (`clippy 0.1.94 (4a4ef493e3 2026-03-02)`): release `0.1.94`, commit `4a4ef493e3`.
///
/// Measured on 2026-09-07 on 1.90.0, 1.94.0, 1.95.0, and 1.97.1 in an isolated pin-less
/// directory: exit 0, that one line on stdout. Taken from the executable Cargo's `Running` line
/// names — never from `cargo clippy --version`, which is the component query of FR-012-7b.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriverIdentity {
    /// `0.1.94`.
    pub release: String,
    /// The hexadecimal commit in the parentheses, a prefix of the compiler's `commit-hash`.
    pub commit: String,
}

/// How rustup attributes the active toolchain in a directory (FR-012-7b), from
/// `rustup show active-toolchain`'s own words; `unknown` for text the table does not pin, never a
/// guess; `no_rustup` only when the resolved compiler is bare (FR-012-7c).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectedBy {
    /// `overridden by environment variable RUSTUP_TOOLCHAIN`.
    Environment,
    /// `directory override for '<path>'`.
    DirectoryOverride,
    /// `overridden by '<path>/rust-toolchain.toml'`.
    ToolchainFile,
    /// `(default)`.
    Default,
    /// The resolved `rustc` is not a rustup proxy: the pin file is inert here.
    NoRustup,
    /// Text the pinned table does not know.
    Unknown,
}

impl SelectedBy {
    /// The wire name, as it appears in the record and the JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::DirectoryOverride => "directory_override",
            Self::ToolchainFile => "toolchain_file",
            Self::Default => "default",
            Self::NoRustup => "no_rustup",
            Self::Unknown => "unknown",
        }
    }
}

/// Whether the build and test units of the project's own package were launched, reused from a
/// cache, or both (FR-012-7d (d) and (f)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Observation {
    /// Every own unit was launched: a compiler was observed and queried.
    Launched,
    /// Every own unit was positively `Fresh`: no launch observed; the observed identity is
    /// unavailable, and nothing fills it in.
    Cached,
    /// Some launched, some `Fresh`: the identity belongs to the launched units only.
    Mixed,
}

impl Observation {
    /// The wire name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Launched => "launched",
            Self::Cached => "cached",
            Self::Mixed => "mixed",
        }
    }
}

/// What resolved in the measured directory (FR-012-7b): a resolution, labelled as such — the
/// source of the FR-012-8 resolution notice and of the record's `resolved_rustc_*`, and never
/// Cargo's effective compiler under `RUSTC`, `build.rustc`, or a wrapper (A-8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// `rustc -vV` in the directory, under the seal.
    pub rustc: Identity,
    /// `cargo -vV` in the directory, under the seal.
    pub cargo: CargoIdentity,
    /// The located rustup's version, or `None` when no rustup was located (a bare toolchain).
    pub rustup: Option<semver::Version>,
    /// The resolved `rustc` is a proxy of the located rustup (FR-012-7c).
    pub proxy: bool,
    /// rustup's attribution of the selection.
    pub selected_by: SelectedBy,
    /// `RUSTC` in the sealed environment, or Cargo's `build.rustc` — presence only.
    pub rustc_override: bool,
    /// `RUSTC_WRAPPER`/`RUSTC_WORKSPACE_WRAPPER` in the sealed environment, or Cargo's
    /// `build.rustc-wrapper`/`build.rustc-workspace-wrapper` — presence only.
    pub wrapper: bool,
    /// `RUSTFLAGS` present.
    pub rustflags: bool,
    /// `RUSTDOCFLAGS` present.
    pub rustdocflags: bool,
}

/// What a project expects of the compiler that verifies it: the channel it pins (or nothing, for
/// a legacy tree) and the MSRV its manifest declares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expectations {
    /// The channel rendered into `rust-toolchain.toml`; `None` for a legacy tree (FR-012-10a).
    pub pinned: Option<String>,
    /// The `rust-version` the manifest declares; `None` for a legacy tree.
    pub rust_version: Option<semver::Version>,
}

/// What FR-012-7a step (3) made of the resolved `rustc`. An unidentified proxy is a refusal,
/// not a variant: it never reaches the resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    /// The same file as a located rustup that passed the floor.
    Proxy {
        /// The rustup it is a proxy of.
        rustup: std::path::PathBuf,
        /// That rustup's version, at or above [`RUSTUP_FLOOR`].
        version: semver::Version,
    },
    /// Not a proxy of a located rustup; the isolated probe answered a compiler identity.
    Bare,
}

/// The refusal for a tool the sealed `PATH` does not resolve, or that cannot be started.
#[must_use]
pub fn tool_absent(name: &str) -> CliError {
    CliError::new(
        Code::ToolMissing,
        format!(
            "`{name}` is not on PATH in the sealed environment, or could not be run; \
             verification requires it. Nothing was written to the destination"
        ),
    )
    .with("tool", name)
    .with("required", "true")
    .with("found", "false")
    .with(
        "remedy",
        format!(
            "install a Rust toolchain with rustup (https://rustup.rs) and put `{name}` on PATH"
        ),
    )
}

/// The `compiler_identity_unreadable` failure (FR-012-7e): a probe answered outside its
/// grammar, or not at all. `check` is the fixed name of the query; the answer is never quoted.
#[must_use]
pub fn unreadable(check: &str) -> CliError {
    CliError::new(
        Code::ProjectVerificationFailed,
        format!(
            "`{check}` did not answer with an identity this generator can read — the answer was \
             outside the grammar, or did not arrive within the deadline; run it in the directory \
             by hand to see it. Nothing was written to the destination"
        ),
    )
    .with("reason", "compiler_identity_unreadable")
    .with("check", check)
    .with("stage", "pre-placement verification")
}

/// FR-012-7a and FR-012-7c — **identify before invoking** — for the tools the sealed `PATH`
/// resolves. Takes no directory: every step runs nowhere, or in an [`isolate::Isolation`], so
/// it can run **before anything is staged** (FR-012-7a), and nothing pinned is involved. Each
/// step's guarantee:
///
/// 1. [`locate::rustup`] — `rustup` is located from the sealed variables and the filesystem
///    alone; **nothing runs**.
/// 2. [`identify::floor`] — if located, `rustup --version` runs under the isolation (an
///    exclusively created empty directory, fresh homes, no `RUSTUP_TOOLCHAIN`, the install
///    servers unroutable): unparseable or below 1.28.1 is refused, and **no proxy has run**.
///    The `rustc`/`cargo` lookups come after this, so a `PATH` without `cargo` proves the
///    order.
/// 3. [`identify::classify`] — `rustc` (and `cargo`, by the same rule) is an *identified* proxy
///    when it is the same file as the located rustup; **nothing runs**.
/// 4. [`identify::isolated_probe`] — every binary that is not an identified proxy answers `-vV`
///    once under the isolation: its own answer is bare; rustup's words are refused as
///    `proxy_unidentified`; anything else is `compiler_identity_unreadable`.
/// 5. [`identify::confirm_no_install`] — on an identified proxy only, the FR-012-7c witness:
///    asked, in an isolation's empty directory and under the seal, for a toolchain that cannot
///    be installed, it answers `is not installed`.
///
/// # Which tools, and why `rustfmt` is one of them
///
/// Every executable this crate looks up on the sealed `PATH` and then runs **in a directory a
/// toolchain file may govern**: `rustc` and `cargo` for [`fn@resolve`]'s identity queries, and
/// `rustfmt` — which `resolve` runs for the component check, and which `renvor generate resource`
/// runs on the module it renders, both inside the project directory (FR-012-14). A layout where
/// `rustc` and `cargo` are bare but `rustfmt` is a proxy is not exotic: a distribution compiler on
/// `PATH` ahead of a `~/.cargo/bin` that still carries rustup's proxies produces exactly it, and
/// then no `rustup` is located at all — so the floor of step 2 never runs, and step 3 classifies
/// nothing. Step 4 on `rustfmt` is what closes that, and the control is
/// `a_rustfmt_proxy_beside_bare_tools_is_identified_before_it_runs_in_a_pinned_directory`.
///
/// `cargo clippy --version` needs no separate step: it is `cargo`, which step 3 or step 4 has
/// already covered. `clippy-driver` and the trailing compiler of a launch chain (FR-012-7d) are
/// not looked up here and are not `PATH` lookups at all — they are the paths **Cargo's own
/// `Running` line named**, queried after Cargo has already executed them in that same directory,
/// so the toolchain they would resolve is the one that has just built and no absent toolchain is
/// nameable. What runs them is stated in [`evidence`]; that they are within the seal's
/// declared limits rather than outside them is the audit's answer, not an omission.
///
/// The [`Classification`] it returns carries what [`fn@resolve`] needs: the located rustup's path
/// (for `rustup show active-toolchain`) and version, or `Bare`.
///
/// # Errors
///
/// `tool_missing` (exit 5) for the floor, an unidentified proxy, an unconfirmed guarantee, or
/// an absent `rustc`/`cargo`; `project_verification_failed` (exit 3) with
/// `reason = compiler_identity_unreadable` or `probe_isolation_unavailable`.
pub fn identify(sealed: &Sealed) -> Result<Classification, CliError> {
    // 1 and 2: locate, then the floor — before any proxy is looked up, let alone run.
    let located = match locate::rustup(sealed) {
        Some(rustup) => {
            let version = identify::floor(&rustup, sealed)?;
            Some((rustup, version))
        }
        None => None,
    };
    // 3: the proxies, classified by file identity.
    let rustc = locate::on_path(sealed, "rustc").ok_or_else(|| tool_absent("rustc"))?;
    let cargo = locate::on_path(sealed, "cargo").ok_or_else(|| tool_absent("cargo"))?;
    let identified = |binary: &Path| {
        located
            .as_ref()
            .is_some_and(|(rustup, _)| identify::is_proxy_of(binary, rustup))
    };
    // `rustfmt`'s ABSENCE is not this function's refusal: `resolve` reports it as the missing
    // component it is, naming the pinned channel in the `rustup component add` remedy, which is
    // knowledge this step does not have. Its PRESENCE is identified here like any other.
    let rustfmt = locate::on_path(sealed, "rustfmt");
    // 4: the isolated probe for whatever is not identified.
    if !identified(&rustc) {
        identify::isolated_probe(&rustc, identify::Tool::Rustc, sealed)?;
    }
    if !identified(&cargo) {
        identify::isolated_probe(&cargo, identify::Tool::Cargo, sealed)?;
    }
    if let Some(rustfmt) = &rustfmt
        && !identified(rustfmt)
    {
        identify::isolated_probe(rustfmt, identify::Tool::Rustfmt, sealed)?;
    }
    let classification = identify::classify(&rustc, located);
    // 5: the uninstallable-name confirmation, on an identified proxy only.
    if matches!(classification, Classification::Proxy { .. }) {
        identify::confirm_no_install(&rustc, sealed)?;
    }
    Ok(classification)
}

/// FR-012-7b — the resolution in `dir`, the directory whose selection is being measured
/// (§5.4), on tools [`fn@identify`] has classified: `rustc -vV`, `cargo -vV`, the components, and
/// rustup's attribution, under the seal. See [`resolve::in_directory`].
///
/// # Errors
///
/// `tool_missing` (exit 5) for an absent toolchain, a compiler below the MSRV, or a missing
/// component; `project_verification_failed` (exit 3) with `reason = compiler_identity_unreadable`.
pub fn resolve(
    dir: &Path,
    sealed: &Sealed,
    classification: &Classification,
    expectations: &Expectations,
) -> Result<Resolution, CliError> {
    resolve::in_directory(dir, sealed, classification, expectations)
}

/// [`fn@identify`] then [`fn@resolve`] in `dir`: the whole preflight of FR-012-7a → 7c → 7b for a
/// caller that has its directory already.
///
/// `#[cfg(test)]`. Neither shipped caller can use it: `renvor new` must identify **before** it
/// stages and resolve **after** it renders, and `renvor generate auth` identifies once and then
/// resolves twice — in the project and in the scratch copy beside it — to compare the two
/// (FR-012-13). The two halves are what production composes; this is the convenience the tests
/// of those halves are written against.
///
/// # Errors
///
/// Those of [`fn@identify`] and [`fn@resolve`].
#[cfg(test)]
pub fn preflight(
    dir: &Path,
    sealed: &Sealed,
    expectations: &Expectations,
) -> Result<Resolution, CliError> {
    let classification = identify(sealed)?;
    resolve(dir, sealed, &classification, expectations)
}

/// Shell-script stand-ins for the toolchain, for the stub-only tests of FR-012-7a/7b/7c.
///
/// Every stub is one POSIX shell script that first **records** its invocation — its name, its
/// arguments, its working directory, the `RUSTUP_*` variables it saw, whether a toolchain file
/// was present, and how many entries its two homes and its directory held — and then behaves
/// as the test's body says. `install` is the behaviour under test: a marker file and a
/// connection attempt to the dist address, which the generator must never provoke.
#[cfg(all(test, unix))]
pub mod testing {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};

    use crate::generate::verify::Sealed;

    /// One recorded invocation of a stub.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct Record {
        /// The stub's file name, as invoked.
        pub name: String,
        /// Its arguments, space-joined.
        pub args: String,
        /// Its physical working directory.
        pub cwd: String,
        /// `RUSTUP_TOOLCHAIN`, or `<unset>`.
        pub rustup_toolchain: String,
        /// `RUSTUP_HOME`, or `<unset>`.
        pub rustup_home: String,
        /// `CARGO_HOME`, or `<unset>`.
        pub cargo_home: String,
        /// `RUSTUP_DIST_SERVER`, or `<unset>`.
        pub dist_server: String,
        /// `RUSTUP_UPDATE_ROOT`, or `<unset>`.
        pub update_root: String,
        /// `RUSTUP_AUTO_INSTALL`, or `<unset>`.
        pub auto_install: String,
        /// `yes` when a toolchain file was in the working directory.
        pub toolchain_file: String,
        /// Entries in `RUSTUP_HOME`, or `absent`.
        pub rustup_home_entries: String,
        /// Entries in `CARGO_HOME`, or `absent`.
        pub cargo_home_entries: String,
        /// Entries in the working directory.
        pub cwd_entries: String,
    }

    /// A stub toolchain that behaves as rustup 1.29.0's proxies do (measured 2026-09-07):
    /// `rustup --version`, `rustup show active-toolchain` with the toolchain-file attribution
    /// (or the environment one when `RUSTUP_TOOLCHAIN` is set), `rustc -vV` answering
    /// `is not installed` for the uninstallable name and an identity otherwise, `cargo -vV`,
    /// `cargo clippy --version`, and `rustfmt --version`. Link `rustc`, `cargo`, and `rustfmt`
    /// to the `rustup` script so the same-file classification identifies them.
    pub const PROXY_TOOLCHAIN: &str = r#"case "$NAME" in
  rustup)
    case "$1" in
      --version) printf 'rustup 1.29.0 (28d1352db 2026-03-05)\n'; printf 'info: no rustc is currently active\n' >&2; exit 0;;
      show)
        if [ -n "${RUSTUP_TOOLCHAIN+x}" ]; then
          printf '%s-aarch64-apple-darwin (overridden by environment variable RUSTUP_TOOLCHAIN)\n' "$RUSTUP_TOOLCHAIN"
        else
          printf "1.94.0-aarch64-apple-darwin (overridden by '%s/rust-toolchain.toml')\n" "$(pwd -P)"
        fi
        exit 0;;
    esac
    exit 1;;
  rustc)
    if [ "${RUSTUP_TOOLCHAIN-}" = renvor-uninstallable-toolchain-name ]; then
      printf "error: toolchain '%s' is not installed\n" "$RUSTUP_TOOLCHAIN" >&2
      exit 1
    fi
    printf '%s' "$RUSTC_VV"; exit 0;;
  cargo)
    case "$1" in
      -vV) printf '%s' "$CARGO_VV"; exit 0;;
      clippy) printf '%s\n' "$CLIPPY_VERSION"; exit 0;;
    esac
    exit 1;;
  rustfmt) printf '%s\n' "$RUSTFMT_VERSION"; exit 0;;
esac
exit 1
"#;

    /// A stub `rustc`/`cargo` that behaves as an OLD proxy whose rustup cannot be located: run
    /// where a toolchain file names an absent channel it "installs" it; run with an empty
    /// `RUSTUP_HOME` it answers in rustup's words; otherwise it prints an identity.
    pub const UNIDENTIFIED_PROXY: &str = r#"if [ "$TF" = yes ]; then
  install
  printf 'info: syncing channel updates for 1.93.0\n' >&2
  exit 1
fi
if [ "$(count "${RUSTUP_HOME-}")" = 0 ] || [ "$(count "${RUSTUP_HOME-}")" = absent ]; then
  printf "error: rustup could not choose a version of rustc to run, because one wasn't specified explicitly, and no default is configured.\n" >&2
  exit 1
fi
printf '%s' "$RUSTC_VV"
exit 0
"#;

    /// The stubs of one test: a `bin` directory on the sealed `PATH`, an empty home, the log
    /// every stub appends to, and the marker `install` writes.
    #[derive(Debug)]
    pub struct Stubs {
        /// Owns everything below.
        pub root: tempfile::TempDir,
        /// The directory the sealed `PATH` names.
        pub bin: PathBuf,
        /// The sealed `HOME`: empty, with no `.cargo/bin`.
        pub home: PathBuf,
        /// The invocation log.
        pub log: PathBuf,
        /// The marker `install` writes.
        pub marker: PathBuf,
    }

    impl Stubs {
        /// Fresh directories.
        pub fn new() -> Self {
            let root = tempfile::tempdir().expect("tempdir");
            let bin = root.path().join("bin");
            let home = root.path().join("home");
            std::fs::create_dir_all(&bin).expect("mkdir");
            std::fs::create_dir_all(&home).expect("mkdir");
            let log = root.path().join("invocations.log");
            let marker = root.path().join("installed.marker");
            Self {
                root,
                bin,
                home,
                log,
                marker,
            }
        }

        /// The recording prelude every stub begins with.
        fn prelude(&self) -> String {
            format!(
                r#"#!/bin/sh
PATH=/usr/bin:/bin
export PATH
NAME=$(basename "$0")
LOG='{log}'
MARKER='{marker}'
TF=no
if [ -f rust-toolchain.toml ] || [ -f rust-toolchain ]; then TF=yes; fi
count() {{ if [ -n "$1" ] && [ -d "$1" ]; then ls -A "$1" | wc -l | tr -d ' '; else printf 'absent'; fi; }}
{{
  printf 'name=%s\n' "$NAME"
  printf 'args=%s\n' "$*"
  printf 'cwd=%s\n' "$(pwd -P)"
  printf 'rustup_toolchain=%s\n' "${{RUSTUP_TOOLCHAIN-<unset>}}"
  printf 'rustup_home=%s\n' "${{RUSTUP_HOME-<unset>}}"
  printf 'cargo_home=%s\n' "${{CARGO_HOME-<unset>}}"
  printf 'dist_server=%s\n' "${{RUSTUP_DIST_SERVER-<unset>}}"
  printf 'update_root=%s\n' "${{RUSTUP_UPDATE_ROOT-<unset>}}"
  printf 'auto_install=%s\n' "${{RUSTUP_AUTO_INSTALL-<unset>}}"
  printf 'toolchain_file=%s\n' "$TF"
  printf 'rustup_home_entries=%s\n' "$(count "${{RUSTUP_HOME-}}")"
  printf 'cargo_home_entries=%s\n' "$(count "${{CARGO_HOME-}}")"
  printf 'cwd_entries=%s\n' "$(count "$(pwd -P)")"
  printf 'end\n'
}} >> "$LOG"
install() {{
  printf 'installed\n' >> "$MARKER"
  curl -s -m 2 "${{RUSTUP_DIST_SERVER:-http://127.0.0.1:9}}/dist/channel-rust-1.93.0.toml" >/dev/null 2>&1 || true
}}
RUSTC_VV='rustc 1.94.0 (4a4ef493e 2026-03-02)
binary: rustc
commit-hash: 4a4ef493e3a1488c6e321570238084b38948f6db
commit-date: 2026-03-02
host: aarch64-apple-darwin
release: 1.94.0
LLVM version: 21.1.8
'
CARGO_VV='cargo 1.94.0 (85eff7c80 2026-01-15)
release: 1.94.0
commit-hash: 85eff7c80277b57f78b11e28d14154ab12fcf643
commit-date: 2026-01-15
host: aarch64-apple-darwin
'
RUSTFMT_VERSION='rustfmt 1.8.0-stable (4a4ef493e3 2026-03-02)'
CLIPPY_VERSION='clippy 0.1.94 (4a4ef493e3 2026-03-02)'
"#,
                log = self.log.display(),
                marker = self.marker.display(),
            )
        }

        /// Writes an executable stub named `name` into `directory`: the prelude, then `body`.
        pub fn script_in(&self, directory: &Path, name: &str, body: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt as _;
            let path = directory.join(name);
            std::fs::write(&path, format!("{}{body}", self.prelude())).expect("write the stub");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make the stub executable");
            path
        }

        /// Writes an executable stub named `name` into `bin`.
        pub fn script(&self, name: &str, body: &str) -> PathBuf {
            self.script_in(&self.bin, name, body)
        }

        /// A directory under the root pinned to `channel`.
        pub fn pinned_dir(&self, channel: &str) -> PathBuf {
            let project = self.root.path().join("project");
            std::fs::create_dir_all(&project).expect("mkdir");
            std::fs::write(
                project.join("rust-toolchain.toml"),
                format!("[toolchain]\nchannel = \"{channel}\"\n"),
            )
            .expect("write the pin");
            project
        }

        /// A seal naming `bin` as the whole `PATH` and the empty home, plus `extra`.
        pub fn sealed(&self, extra: &[(&str, &str)]) -> Sealed {
            let mut variables = vec![
                (OsString::from("PATH"), OsString::from(&self.bin)),
                (OsString::from("HOME"), OsString::from(&self.home)),
            ];
            variables.extend(
                extra
                    .iter()
                    .map(|(name, value)| (OsString::from(name), OsString::from(value))),
            );
            Sealed {
                variables,
                credentials: Vec::new(),
            }
        }

        /// Whether any stub "installed".
        pub fn installed(&self) -> bool {
            self.marker.exists()
        }

        /// Every recorded invocation, in order.
        pub fn records(&self) -> Vec<Record> {
            let text = std::fs::read_to_string(&self.log).unwrap_or_default();
            let mut records = Vec::new();
            let mut current = Record::default();
            for line in text.lines() {
                if line == "end" {
                    records.push(std::mem::take(&mut current));
                    continue;
                }
                let Some((key, value)) = line.split_once('=') else {
                    continue;
                };
                let value = value.to_owned();
                match key {
                    "name" => current.name = value,
                    "args" => current.args = value,
                    "cwd" => current.cwd = value,
                    "rustup_toolchain" => current.rustup_toolchain = value,
                    "rustup_home" => current.rustup_home = value,
                    "cargo_home" => current.cargo_home = value,
                    "dist_server" => current.dist_server = value,
                    "update_root" => current.update_root = value,
                    "auto_install" => current.auto_install = value,
                    "toolchain_file" => current.toolchain_file = value,
                    "rustup_home_entries" => current.rustup_home_entries = value,
                    "cargo_home_entries" => current.cargo_home_entries = value,
                    "cwd_entries" => current.cwd_entries = value,
                    _ => {}
                }
            }
            records
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_constant_parses_as_the_version_the_floor_check_compares_against() {
        assert_eq!(
            semver::Version::parse(RUSTUP_FLOOR).expect("the floor is a version"),
            identify::floor_version()
        );
    }

    #[test]
    fn the_shared_refusals_carry_their_names() {
        let absent = tool_absent("cargo");
        assert_eq!(absent.code, Code::ToolMissing);
        assert!(
            absent
                .details
                .contains(&("tool".to_owned(), "cargo".to_owned()))
        );
        assert!(
            absent
                .details
                .contains(&("found".to_owned(), "false".to_owned()))
        );
        let unread = unreadable("rustc -vV");
        assert_eq!(unread.code, Code::ProjectVerificationFailed);
        assert!(unread.details.contains(&(
            "reason".to_owned(),
            "compiler_identity_unreadable".to_owned()
        )));
        assert!(
            unread
                .details
                .contains(&("check".to_owned(), "rustc -vV".to_owned()))
        );
    }
}
