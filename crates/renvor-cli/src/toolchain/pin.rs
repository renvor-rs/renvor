//! The framework checkout's toolchain pin and MSRV, each read on its own (FR-012-1, FR-012-2,
//! D-L2-5).
//!
//! A starter renders `rust-toolchain.toml` from `<framework-path>/rust-toolchain.toml`'s
//! `[toolchain].channel` and `rust-version` from `<framework-path>/Cargo.toml`'s
//! `[workspace.package].rust-version`; the two files are **parsed, never evaluated**, and the two
//! values are compared only after each has been read by itself. A checkout that is inconsistent
//! — a malformed pin, a channel alias, a pin below its own MSRV, an unreadable MSRV — is refused
//! by name before anything is staged: it is the framework's inconsistency, and the message says
//! which file. Nothing here resolves an alias to a version, silently or otherwise.

use std::path::{Path, PathBuf};

use crate::exit::{CliError, Code};

/// The pin a starter renders: an exact release read from the checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    /// `[toolchain].channel`, an exact `X.Y.Z`.
    pub channel: semver::Version,
    /// `[workspace.package].rust-version`, an exact `X.Y.Z`.
    pub msrv: semver::Version,
}

/// Why a checkout's declaration was refused (FR-012-1's named reasons).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinError {
    /// `toolchain_pin_malformed`: the file is unreadable, lacks the key, or the channel is not
    /// `X.Y.Z` and not a recognised alias either.
    Malformed {
        /// The file that was read.
        file: PathBuf,
        /// What was wrong, in words that name no value the file may carry.
        why: &'static str,
    },
    /// `toolchain_pin_unsupported`: a channel alias (`stable`, `beta`, `nightly`, a dated
    /// nightly), a custom toolchain name, or a `path` — none is resolved to a version.
    Unsupported {
        /// The file that was read.
        file: PathBuf,
        /// The shape that was found (`alias`, `dated nightly`, `custom name`, `path`).
        kind: &'static str,
    },
    /// `toolchain_pin_below_msrv`: `channel < msrv`.
    BelowMsrv {
        /// The pin file.
        file: PathBuf,
        /// The channel that was read.
        channel: semver::Version,
        /// The MSRV that was read.
        msrv: semver::Version,
    },
    /// `msrv_unreadable`: the manifest key is absent or not `X.Y.Z`.
    MsrvUnreadable {
        /// The manifest that was read.
        file: PathBuf,
        /// What was wrong.
        why: &'static str,
    },
}

impl PinError {
    /// The wire reason of FR-012-1.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Malformed { .. } => "toolchain_pin_malformed",
            Self::Unsupported { .. } => "toolchain_pin_unsupported",
            Self::BelowMsrv { .. } => "toolchain_pin_below_msrv",
            Self::MsrvUnreadable { .. } => "msrv_unreadable",
        }
    }

    /// The file the refusal names.
    #[must_use]
    pub fn file(&self) -> &Path {
        match self {
            Self::Malformed { file, .. }
            | Self::Unsupported { file, .. }
            | Self::BelowMsrv { file, .. }
            | Self::MsrvUnreadable { file, .. } => file,
        }
    }

    /// The refusal as the operator sees it: exit 3, the `--framework-path` rule family of C-1
    /// (`rule = framework_directory`), the named `reason`, and the `file`.
    #[must_use]
    pub fn into_cli_error(self, shown_framework_path: &str) -> CliError {
        let file = self.file().display().to_string();
        let message = match &self {
            Self::Malformed { why, .. } => format!(
                "the framework checkout's `{file}` does not declare an exact toolchain release: \
                 {why}. This is an inconsistency of the framework checkout, not of your command"
            ),
            Self::Unsupported { kind, .. } => format!(
                "the framework checkout's `{file}` pins a {kind}, which is not an exact release; \
                 a generated project pins exactly what the checkout pins, and an alias is never \
                 resolved to a version"
            ),
            Self::BelowMsrv { channel, msrv, .. } => format!(
                "the framework checkout's `{file}` pins {channel}, below the MSRV {msrv} its own \
                 Cargo.toml declares; the checkout is inconsistent"
            ),
            Self::MsrvUnreadable { why, .. } => format!(
                "the framework checkout's `{file}` does not declare an exact `rust-version` under \
                 `[workspace.package]`: {why}"
            ),
        };
        CliError::new(Code::UnsupportedValue, message)
            .with("flag", "--framework-path")
            .with("value", shown_framework_path.to_owned())
            .with("rule", "framework_directory")
            .with("reason", self.reason())
            .with("file", file)
    }
}

/// Reads `[toolchain].channel` from `rust-toolchain.toml` at `file`.
///
/// # Errors
///
/// [`PinError::Malformed`] or [`PinError::Unsupported`].
pub fn read_channel(file: &Path) -> Result<semver::Version, PinError> {
    let text = std::fs::read_to_string(file).map_err(|_| PinError::Malformed {
        file: file.to_path_buf(),
        why: "the file could not be read",
    })?;
    let document: toml::Value = toml::from_str(&text).map_err(|_| PinError::Malformed {
        file: file.to_path_buf(),
        why: "the file is not valid TOML",
    })?;
    let toolchain = document
        .get("toolchain")
        .and_then(toml::Value::as_table)
        .ok_or(PinError::Malformed {
            file: file.to_path_buf(),
            why: "there is no `[toolchain]` table",
        })?;
    if toolchain.contains_key("path") {
        return Err(PinError::Unsupported {
            file: file.to_path_buf(),
            kind: "path",
        });
    }
    let channel = toolchain
        .get("channel")
        .and_then(toml::Value::as_str)
        .ok_or(PinError::Malformed {
            file: file.to_path_buf(),
            why: "there is no `channel` string under `[toolchain]`",
        })?;
    classify_channel(channel)
        .map_err(|kind| PinError::Unsupported {
            file: file.to_path_buf(),
            kind,
        })?
        .ok_or(PinError::Malformed {
            file: file.to_path_buf(),
            why: "`channel` is not an exact `X.Y.Z` release",
        })
}

/// `Ok(Some(version))` for an exact release, `Ok(None)` for text that is neither a release nor a
/// recognised alias, `Err(kind)` for an alias, a dated nightly, or a custom name — which are
/// refused, not resolved.
fn classify_channel(channel: &str) -> Result<Option<semver::Version>, &'static str> {
    let channel = channel.trim();
    if let Ok(version) = semver::Version::parse(channel)
        && version.pre.is_empty()
        && version.build.is_empty()
    {
        return Ok(Some(version));
    }
    let lower = channel.to_ascii_lowercase();
    let alias = ["stable", "beta", "nightly"];
    if alias.contains(&lower.as_str()) {
        return Err("channel alias");
    }
    if alias.iter().any(|name| {
        lower
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(|rest| !rest.is_empty())
    }) {
        return Err("dated or targeted alias");
    }
    if !channel.is_empty()
        && channel
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
        && !channel.chars().next().is_some_and(|c| c.is_ascii_digit())
    {
        return Err("custom toolchain name");
    }
    Ok(None)
}

/// Reads `[workspace.package].rust-version` from the workspace manifest at `file`.
///
/// # Errors
///
/// [`PinError::MsrvUnreadable`].
pub fn read_msrv(file: &Path) -> Result<semver::Version, PinError> {
    let unreadable = |why| PinError::MsrvUnreadable {
        file: file.to_path_buf(),
        why,
    };
    let text =
        std::fs::read_to_string(file).map_err(|_| unreadable("the file could not be read"))?;
    let document: toml::Value =
        toml::from_str(&text).map_err(|_| unreadable("the file is not valid TOML"))?;
    let value = document
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("rust-version"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| unreadable("the key is absent"))?;
    let version =
        semver::Version::parse(value.trim()).map_err(|_| unreadable("the value is not `X.Y.Z`"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(unreadable("the value is not an exact `X.Y.Z` release"));
    }
    Ok(version)
}

/// Reads both values from the checkout at `framework`, each on its own, then compares them.
///
/// # Errors
///
/// Any [`PinError`]; the MSRV is read even when the pin is refused, so the first refusal names
/// the pin file and a checkout with both defects is reported for the pin first.
pub fn read(framework: &Path) -> Result<Pin, PinError> {
    let pin_file = framework.join("rust-toolchain.toml");
    let manifest = framework.join("Cargo.toml");
    let channel = read_channel(&pin_file)?;
    let msrv = read_msrv(&manifest)?;
    if channel < msrv {
        return Err(PinError::BelowMsrv {
            file: pin_file,
            channel,
            msrv,
        });
    }
    Ok(Pin { channel, msrv })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(pin: &str, manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("rust-toolchain.toml"), pin).expect("write");
        std::fs::write(dir.path().join("Cargo.toml"), manifest).expect("write");
        dir
    }

    const MANIFEST: &str =
        "[workspace]\nmembers = []\n[workspace.package]\nrust-version = \"1.94.0\"\n";

    #[test]
    fn the_frameworks_own_checkout_reads_as_an_exact_pin_at_or_above_its_msrv() {
        // POSITIVE CONTROL: the repository's own declaration.
        let here = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pin = read(&here).expect("the framework checkout is consistent");
        assert!(pin.channel >= pin.msrv, "the pin is at or above the MSRV");
        assert_eq!(pin.msrv.to_string(), env!("CARGO_PKG_RUST_VERSION"));
    }

    #[test]
    fn a_starter_pin_is_the_checkouts_channel_and_its_msrv_is_the_manifests() {
        let dir = checkout("[toolchain]\nchannel = \"1.95.0\"\n", MANIFEST);
        let pin = read(dir.path()).expect("reads");
        assert_eq!(pin.channel.to_string(), "1.95.0");
        assert_eq!(pin.msrv.to_string(), "1.94.0");
    }

    #[test]
    fn an_alias_channel_is_refused_not_resolved() {
        for (channel, kind) in [
            ("stable", "channel alias"),
            ("beta", "channel alias"),
            ("nightly", "channel alias"),
            ("nightly-2026-01-01", "dated or targeted alias"),
            ("stable-x86_64-unknown-linux-gnu", "dated or targeted alias"),
            ("my-custom-toolchain", "custom toolchain name"),
        ] {
            let dir = checkout(&format!("[toolchain]\nchannel = \"{channel}\"\n"), MANIFEST);
            let error = read(dir.path()).expect_err("an alias is refused");
            assert_eq!(error.reason(), "toolchain_pin_unsupported");
            assert!(matches!(&error, PinError::Unsupported { kind: k, .. } if *k == kind));
        }
        let dir = checkout("[toolchain]\npath = \"/opt/rust\"\n", MANIFEST);
        let error = read(dir.path()).expect_err("a path is refused");
        assert!(matches!(error, PinError::Unsupported { kind: "path", .. }));
    }

    #[test]
    fn a_malformed_channel_is_refused_by_name() {
        for pin in [
            "",
            "not toml [",
            "[toolchain]\n",
            "[toolchain]\nchannel = 1\n",
            "[toolchain]\nchannel = \"1.94\"\n",
            "[toolchain]\nchannel = \"1.94.0-beta.1\"\n",
            "[toolchain]\nchannel = \"1.94.0+build\"\n",
            "[toolchain]\nchannel = \"\"\n",
        ] {
            let dir = checkout(pin, MANIFEST);
            let error = read(dir.path()).expect_err("a malformed pin is refused");
            assert_eq!(error.reason(), "toolchain_pin_malformed");
            assert!(error.file().ends_with("rust-toolchain.toml"));
        }
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("Cargo.toml"), MANIFEST).expect("write");
        let error = read(dir.path()).expect_err("a missing pin file is refused");
        assert_eq!(error.reason(), "toolchain_pin_malformed");
    }

    #[test]
    fn a_pin_below_the_manifests_msrv_is_refused_by_name() {
        let dir = checkout("[toolchain]\nchannel = \"1.93.0\"\n", MANIFEST);
        let error = read(dir.path()).expect_err("below the MSRV is refused");
        assert_eq!(error.reason(), "toolchain_pin_below_msrv");
        assert!(error.file().ends_with("rust-toolchain.toml"));
        // Equal is accepted: the pin may be the MSRV itself.
        let dir = checkout("[toolchain]\nchannel = \"1.94.0\"\n", MANIFEST);
        assert!(read(dir.path()).is_ok());
    }

    #[test]
    fn an_unreadable_msrv_is_refused_by_name() {
        for manifest in [
            "",
            "[workspace]\n",
            "[workspace.package]\n",
            "[workspace.package]\nrust-version = 1\n",
            "[workspace.package]\nrust-version = \"1.94\"\n",
            "[package]\nrust-version = \"1.94.0\"\n",
        ] {
            let dir = checkout("[toolchain]\nchannel = \"1.94.0\"\n", manifest);
            let error = read(dir.path()).expect_err("an unreadable MSRV is refused");
            assert_eq!(error.reason(), "msrv_unreadable");
            assert!(error.file().ends_with("Cargo.toml"));
        }
    }

    #[test]
    fn a_refusal_is_a_framework_path_validation_failure_naming_the_reason_and_the_file() {
        let dir = checkout("[toolchain]\nchannel = \"stable\"\n", MANIFEST);
        let error = read(dir.path())
            .expect_err("refused")
            .into_cli_error("../renvor");
        assert_eq!(error.code, Code::UnsupportedValue);
        assert_eq!(error.exit(), crate::exit::Exit::Validation);
        let detail = |key: &str| {
            error
                .details
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(detail("flag"), Some("--framework-path"));
        assert_eq!(detail("rule"), Some("framework_directory"));
        assert_eq!(detail("reason"), Some("toolchain_pin_unsupported"));
        assert!(detail("file").is_some_and(|f| f.ends_with("rust-toolchain.toml")));
        assert!(
            !error.message.contains("stable"),
            "the message names the shape, never the value the file carried"
        );
    }
}
