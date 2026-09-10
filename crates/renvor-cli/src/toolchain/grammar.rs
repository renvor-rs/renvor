//! The parsers of FR-012-7e: nothing a tool printed reaches a record, a JSON document, or an
//! operator stream until it has matched one of these grammars.
//!
//! Every function here is pure, takes the text a child wrote, and returns either a value made of
//! validated pieces or an error that names **which line** was wrong and never quotes it. That is
//! the whole discipline: a `-vV` answer is untrusted text — a wrapper, a shim, or a distribution
//! patch may have written anything — so the only strings that leave this module are the ones
//! that matched a character-by-character rule.
//!
//! # The grammars (FR-012-7e)
//!
//! | Piece | Rule |
//! |---|---|
//! | `release` | `^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$` |
//! | `commit-hash` | `^[0-9a-f]{7,40}$`, or the word `unknown` (a distribution compiler) |
//! | `host` | `^[A-Za-z0-9_.-]{1,64}$` |
//!
//! Implemented with plain character checks rather than a regular-expression crate: three
//! character classes and a length do not justify a matcher, and a rule written out as code is
//! one a reader can check against the table above without learning a second syntax.
//!
//! # The attribution table (FR-012-7b)
//!
//! `rustup show active-toolchain` explains its choice in a parenthesised suffix. The strings are
//! pinned by a table test against what rustup 1.29.0 printed on 2026-09-07 and against rustup's
//! own `ActiveReason` rendering for the one form that cannot be measured without editing the
//! operator's rustup settings. Text the table does not know is [`SelectedBy::Unknown`] — never
//! a guess.

use super::{CargoIdentity, DriverIdentity, Identity, SelectedBy};

/// Why a child's answer did not match its grammar. Carries the **name** of the offending piece,
/// never its text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrammarError {
    /// The answer does not begin the way the tool's own answer begins (`rustc `, `cargo `,
    /// `clippy `, `rustup `), or is empty.
    Shape(&'static str),
    /// The named line (`release:`, `commit-hash:`, `host:`) is absent.
    Missing(&'static str),
    /// The named piece is present and outside its grammar.
    Malformed(&'static str),
}

impl std::fmt::Display for GrammarError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape(what) => write!(formatter, "the answer is not a `{what}` answer"),
            Self::Missing(line) => write!(formatter, "the `{line}` line is missing"),
            Self::Malformed(piece) => {
                write!(formatter, "the `{piece}` value is outside the grammar")
            }
        }
    }
}

impl std::error::Error for GrammarError {}

/// `^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$`.
#[must_use]
pub fn is_release(text: &str) -> bool {
    let (numbers, suffix) = match text.split_once('-') {
        Some((numbers, suffix)) => (numbers, Some(suffix)),
        None => (text, None),
    };
    let mut parts = numbers.split('.');
    let three = (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    });
    if !three || parts.next().is_some() {
        return false;
    }
    match suffix {
        None => true,
        Some(suffix) => {
            !suffix.is_empty()
                && suffix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.')
        }
    }
}

/// `^[0-9a-f]{7,40}$`, or `unknown`.
#[must_use]
pub fn is_commit(text: &str) -> bool {
    text == "unknown"
        || ((7..=40).contains(&text.len())
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

/// `^[A-Za-z0-9_.-]{1,64}$`.
#[must_use]
pub fn is_host(text: &str) -> bool {
    (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// A toolchain name as it may appear in a refusal or a remedy: `^[A-Za-z0-9._-]{1,64}$` — the
/// same class as a host triple, which is what a rustup toolchain name is made of.
#[must_use]
pub fn is_channel_name(text: &str) -> bool {
    is_host(text)
}

/// The first line that is not blank, with trailing whitespace removed.
fn first_line(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim_end)
        .find(|line| !line.is_empty())
}

/// The value of the first line beginning `<key>: `, trimmed.
fn labelled<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let line = line.trim_end();
        line.strip_prefix(key)
            .and_then(|rest| rest.strip_prefix(':'))
            .map(str::trim)
    })
}

/// The shared shape of `rustc -vV` and `cargo -vV`: line 1 `<tool> <release> (<short> <date>)`,
/// then labelled lines of which `release:`, `commit-hash:`, and `host:` are read. `tool` is the
/// first word a valid answer begins with (`rustc` or `cargo`).
///
/// # Errors
///
/// [`GrammarError`] naming the piece; the text is never quoted.
pub fn parse_vv(text: &str, tool: &'static str) -> Result<Identity, GrammarError> {
    let first = first_line(text).ok_or(GrammarError::Shape(tool))?;
    if !first.starts_with(tool) || !first[tool.len()..].starts_with(' ') {
        return Err(GrammarError::Shape(tool));
    }
    let release = labelled(text, "release").ok_or(GrammarError::Missing("release"))?;
    if !is_release(release) {
        return Err(GrammarError::Malformed("release"));
    }
    let commit = labelled(text, "commit-hash").ok_or(GrammarError::Missing("commit-hash"))?;
    if !is_commit(commit) {
        return Err(GrammarError::Malformed("commit-hash"));
    }
    let host = labelled(text, "host").ok_or(GrammarError::Missing("host"))?;
    if !is_host(host) {
        return Err(GrammarError::Malformed("host"));
    }
    Ok(Identity {
        release: release.to_owned(),
        commit: commit.to_owned(),
        host: host.to_owned(),
    })
}

/// Parses `rustc -vV`. A `commit-hash: unknown` is accepted: a distribution compiler prints it.
///
/// # Errors
///
/// [`GrammarError`] naming the piece; the text is never quoted.
pub fn parse_rustc_vv(text: &str) -> Result<Identity, GrammarError> {
    parse_vv(text, "rustc")
}

/// Parses `cargo -vV`: the same lines, the host read for the grammar's sake and then dropped —
/// the record keeps cargo's release and commit only.
///
/// # Errors
///
/// [`GrammarError`] naming the piece.
pub fn parse_cargo_vv(text: &str) -> Result<CargoIdentity, GrammarError> {
    let identity = parse_vv(text, "cargo")?;
    Ok(CargoIdentity {
        release: identity.release,
        commit: identity.commit,
    })
}

/// Parses `clippy <release> (<commit> <date>)` — the line `clippy-driver --version` prints, and
/// the line `cargo clippy --version` prints (measured 2026-09-07 on 1.90.0, 1.94.0, 1.95.0,
/// and 1.97.1, e.g. `clippy 0.1.94 (4a4ef493e3 2026-03-02)`).
///
/// # Errors
///
/// [`GrammarError`] naming the piece.
pub fn parse_clippy_version(text: &str) -> Result<DriverIdentity, GrammarError> {
    let first = first_line(text).ok_or(GrammarError::Shape("clippy"))?;
    let rest = first
        .strip_prefix("clippy ")
        .ok_or(GrammarError::Shape("clippy"))?;
    let (release, parenthesised) = rest
        .split_once(' ')
        .ok_or(GrammarError::Missing("commit"))?;
    if !is_release(release) {
        return Err(GrammarError::Malformed("release"));
    }
    let inner = parenthesised
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .ok_or(GrammarError::Missing("commit"))?;
    let commit = inner
        .split_whitespace()
        .next()
        .ok_or(GrammarError::Missing("commit"))?;
    if commit == "unknown" || !is_commit(commit) {
        return Err(GrammarError::Malformed("commit"));
    }
    Ok(DriverIdentity {
        release: release.to_owned(),
        commit: commit.to_owned(),
    })
}

/// Whether `text` is `rustfmt`'s own version answer: line 1 `rustfmt <version> (<commit> <date>)`
/// (measured 2026-09-08: `rustfmt 1.8.0-stable (4a4ef493e3 2026-03-02)`, printed identically for
/// `--version`, `-V`, and `-vV`).
///
/// # Why this is a predicate and not a parser
///
/// The identification probe of FR-012-7a step (4) asks one question — *did this binary answer as
/// itself, or in rustup's words?* — and `rustfmt`'s answer enters no record: the toolchain
/// identities the record carries come from `rustc` and from `clippy-driver`. A parser here would
/// return a value with nowhere to go, and the `-stable` suffix its release carries is not
/// [`is_release`]'s shape, so it would need a second grammar to hold nothing.
#[must_use]
pub fn is_rustfmt_version(text: &str) -> bool {
    first_line(text).is_some_and(|first| {
        first
            .strip_prefix("rustfmt ")
            .is_some_and(|rest| !rest.trim().is_empty())
    })
}

/// Parses the first stdout line of `rustup --version`: `rustup X.Y.Z (<commit> <date>)`
/// (rustup 1.29.0 on 2026-09-07: `rustup 1.29.0 (28d1352db 2026-03-05)`). Only that line is
/// read; rustup's `info:` lines go to stderr and are not consulted.
///
/// # Errors
///
/// [`GrammarError`] when the line is not rustup's or the version is not a semantic version.
pub fn parse_rustup_version(text: &str) -> Result<semver::Version, GrammarError> {
    let first = first_line(text).ok_or(GrammarError::Shape("rustup"))?;
    let rest = first
        .strip_prefix("rustup ")
        .ok_or(GrammarError::Shape("rustup"))?;
    let token = rest.split_whitespace().next().unwrap_or("");
    semver::Version::parse(token).map_err(|_| GrammarError::Malformed("version"))
}

/// The attribution table of FR-012-7b, applied to `rustup show active-toolchain`'s first line.
///
/// | Text | Result |
/// |---|---|
/// | `overridden by environment variable RUSTUP_TOOLCHAIN` | [`SelectedBy::Environment`] |
/// | `directory override for '` | [`SelectedBy::DirectoryOverride`] |
/// | `overridden by '` | [`SelectedBy::ToolchainFile`] |
/// | `(default)` | [`SelectedBy::Default`] |
/// | anything else | [`SelectedBy::Unknown`] |
///
/// The environment form is tested first and the default form last, so a path that happens to
/// contain `(default)` inside a `rust-toolchain.toml` attribution is still the file's. The path
/// rustup prints is read for the match and then discarded: nothing here keeps it.
#[must_use]
pub fn parse_active_toolchain_attribution(text: &str) -> SelectedBy {
    let Some(line) = first_line(text) else {
        return SelectedBy::Unknown;
    };
    if line.contains("overridden by environment variable RUSTUP_TOOLCHAIN") {
        SelectedBy::Environment
    } else if line.contains("directory override for '") {
        SelectedBy::DirectoryOverride
    } else if line.contains("overridden by '") {
        SelectedBy::ToolchainFile
    } else if line.contains("(default)") {
        SelectedBy::Default
    } else {
        SelectedBy::Unknown
    }
}

/// The channel rustup named in `toolchain '<name>' is not installed`, when the name is a
/// well-formed toolchain name (`^[A-Za-z0-9._-]{1,64}$`); `None` for any other text, including a
/// sentence whose name carries a quote, a space, or a control character — which is then reported
/// without a channel rather than with a doctored one.
#[must_use]
pub fn parse_not_installed_channel(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let after = line
            .find("toolchain '")
            .map(|at| &line[at + "toolchain '".len()..])?;
        let (name, rest) = after.split_once('\'')?;
        (rest.starts_with(" is not installed") && is_channel_name(name)).then(|| name.to_owned())
    })
}

/// Whether text is rustup speaking rather than a compiler: `is not installed`, `no default
/// toolchain`, `no override and no default`, or the word `rustup` itself. Used only after an
/// answer has failed the identity grammar — a valid identity is never mistaken for rustup's words.
#[must_use]
pub fn looks_like_rustup_words(text: &str) -> bool {
    [
        "is not installed",
        "no default toolchain",
        "no override and no default",
        "rustup",
    ]
    .iter()
    .any(|words| text.contains(words))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rustc -vV` on the pinned toolchain, measured 2026-09-07.
    const RUSTC_VV: &str = "rustc 1.94.0 (4a4ef493e 2026-03-02)\nbinary: rustc\ncommit-hash: 4a4ef493e3a1488c6e321570238084b38948f6db\ncommit-date: 2026-03-02\nhost: aarch64-apple-darwin\nrelease: 1.94.0\nLLVM version: 21.1.8\n";

    /// `cargo -vV` on the pinned toolchain, measured 2026-09-07.
    const CARGO_VV: &str = "cargo 1.94.0 (85eff7c80 2026-01-15)\nrelease: 1.94.0\ncommit-hash: 85eff7c80277b57f78b11e28d14154ab12fcf643\ncommit-date: 2026-01-15\nhost: aarch64-apple-darwin\nlibgit2: 1.9.2 (sys:0.20.3 vendored)\nlibcurl: 8.7.1\nssl: OpenSSL 3.5.4 30 Sep 2025\nos: Mac OS 26.3.0 [64-bit]\n";

    #[test]
    fn rustc_vv_parses_the_measured_shape() {
        let identity = parse_rustc_vv(RUSTC_VV).expect("the measured answer parses");
        assert_eq!(identity.release, "1.94.0");
        assert_eq!(identity.commit, "4a4ef493e3a1488c6e321570238084b38948f6db");
        assert_eq!(identity.host, "aarch64-apple-darwin");
        // A nightly's release carries a suffix the grammar admits.
        let nightly = RUSTC_VV.replace("release: 1.94.0", "release: 1.99.0-nightly");
        assert_eq!(
            parse_rustc_vv(&nightly).expect("a nightly parses").release,
            "1.99.0-nightly"
        );
    }

    #[test]
    fn a_distribution_compiler_with_an_unknown_commit_is_accepted() {
        let distro = RUSTC_VV.replace(
            "commit-hash: 4a4ef493e3a1488c6e321570238084b38948f6db",
            "commit-hash: unknown",
        );
        let identity = parse_rustc_vv(&distro).expect("a distribution compiler parses");
        assert_eq!(identity.commit, "unknown");
    }

    #[test]
    fn rustc_vv_outside_the_grammar_is_refused_naming_the_piece() {
        let cases: [(String, GrammarError); 10] = [
            (String::new(), GrammarError::Shape("rustc")),
            (
                "error: toolchain '1.93.0' is not installed\n".to_owned(),
                GrammarError::Shape("rustc"),
            ),
            (
                RUSTC_VV.replace("release: 1.94.0\n", ""),
                GrammarError::Missing("release"),
            ),
            (
                RUSTC_VV.replace("release: 1.94.0", "release: 1.94"),
                GrammarError::Malformed("release"),
            ),
            (
                RUSTC_VV.replace("release: 1.94.0", "release: 1.94.0+build"),
                GrammarError::Malformed("release"),
            ),
            (
                RUSTC_VV.replace(
                    "commit-hash: 4a4ef493e3a1488c6e321570238084b38948f6db",
                    "commit-hash: abc",
                ),
                GrammarError::Malformed("commit-hash"),
            ),
            (
                RUSTC_VV.replace(
                    "commit-hash: 4a4ef493e3a1488c6e321570238084b38948f6db\n",
                    "",
                ),
                GrammarError::Missing("commit-hash"),
            ),
            (
                RUSTC_VV.replace("host: aarch64-apple-darwin", "host: aarch64 apple darwin"),
                GrammarError::Malformed("host"),
            ),
            (
                RUSTC_VV.replace(
                    "host: aarch64-apple-darwin",
                    &format!("host: {}", "x".repeat(65)),
                ),
                GrammarError::Malformed("host"),
            ),
            (
                RUSTC_VV.replace("host: aarch64-apple-darwin\n", ""),
                GrammarError::Missing("host"),
            ),
        ];
        for (index, (text, expected)) in cases.iter().enumerate() {
            let error = parse_rustc_vv(text).expect_err("outside the grammar is refused");
            assert_eq!(
                &error, expected,
                "the case at this index names the wrong piece: {index}"
            );
        }
        // The boundaries the table states: 7 and 40 hexadecimal digits are commits, 6 and 41 are
        // not; 64 host characters are a host, 65 are not.
        assert!(is_commit(&"a".repeat(7)) && is_commit(&"0".repeat(40)));
        assert!(!is_commit(&"a".repeat(6)) && !is_commit(&"0".repeat(41)));
        assert!(
            !is_commit("ABCDEF0"),
            "upper-case hexadecimal is not the grammar"
        );
        assert!(is_host(&"h".repeat(64)) && !is_host(&"h".repeat(65)) && !is_host(""));
        assert!(is_release("1.94.0") && is_release("1.99.0-nightly") && is_release("1.0.0-beta.2"));
        assert!(!is_release("1.94") && !is_release("1.94.0-") && !is_release("1.94.0.1"));
        assert!(!is_release("v1.94.0") && !is_release("1.94.0-nightly x"));
    }

    #[test]
    fn cargo_vv_parses_the_measured_shape_and_keeps_no_host() {
        let identity = parse_cargo_vv(CARGO_VV).expect("the measured answer parses");
        assert_eq!(identity.release, "1.94.0");
        assert_eq!(identity.commit, "85eff7c80277b57f78b11e28d14154ab12fcf643");
        // rustc's answer is not cargo's: the first line names the tool.
        assert_eq!(
            parse_cargo_vv(RUSTC_VV).expect_err("a rustc answer is not a cargo answer"),
            GrammarError::Shape("cargo")
        );
    }

    #[test]
    fn clippy_version_parses_the_measured_shapes() {
        // Measured 2026-09-07 on four toolchains (`clippy-driver-version-measurement.txt`).
        for (line, release, commit) in [
            (
                "clippy 0.1.90 (1159e78c47 2025-09-14)",
                "0.1.90",
                "1159e78c47",
            ),
            (
                "clippy 0.1.94 (4a4ef493e3 2026-03-02)",
                "0.1.94",
                "4a4ef493e3",
            ),
            (
                "clippy 0.1.95 (59807616e1 2026-04-14)",
                "0.1.95",
                "59807616e1",
            ),
            (
                "clippy 0.1.97 (8bab26f4f6 2026-07-14)\n",
                "0.1.97",
                "8bab26f4f6",
            ),
        ] {
            let driver = parse_clippy_version(line).expect("a measured line parses");
            assert_eq!(driver.release, release);
            assert_eq!(driver.commit, commit);
        }
        for (index, text) in [
            "",
            "clippy 0.1.94",
            "clippy 0.1.94 4a4ef493e3 2026-03-02",
            "clippy 0.1.94 (zzzz 2026-03-02)",
            "clippy 0.1.94 (unknown 2026-03-02)",
            "clippy 0.1 (4a4ef493e3 2026-03-02)",
            "error: 'cargo-clippy' is not installed for the toolchain '1.94.0'",
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                parse_clippy_version(text).is_err(),
                "the text at this index was accepted: {index}"
            );
        }
    }

    #[test]
    fn rustup_version_is_read_from_stdout_line_1_only() {
        assert_eq!(
            parse_rustup_version("rustup 1.29.0 (28d1352db 2026-03-05)\n").expect("parses"),
            semver::Version::new(1, 29, 0)
        );
        assert_eq!(
            parse_rustup_version("rustup 1.27.1 (54dd3d00f 2024-04-24)").expect("parses"),
            semver::Version::new(1, 27, 1)
        );
        // The `info:` lines rustup 1.29.0 writes are on stderr and never reach this parser; a
        // caller that handed them over would be refused rather than misread.
        for (index, text) in [
            "",
            "rustup",
            "rustup x.y.z (abc 2026)",
            "info: This is the version for the rustup toolchain manager\nrustup 1.29.0 (a 2026)",
            "error: toolchain '1.93.0' is not installed",
        ]
        .into_iter()
        .enumerate()
        {
            assert!(
                parse_rustup_version(text).is_err(),
                "the text at this index was accepted: {index}"
            );
        }
    }

    #[test]
    fn the_attribution_strings_of_rustup_1_28_1_and_1_29_0_are_pinned() {
        // The three forms measured on rustup 1.29.0 (2026-09-07), and the directory-override
        // form from rustup's `ActiveReason::OverrideDB` rendering — not measured, because
        // measuring it edits the operator's rustup settings.
        let table = [
            (
                "1.94.0-aarch64-apple-darwin (overridden by '/home/op/demo/rust-toolchain.toml')\n",
                SelectedBy::ToolchainFile,
            ),
            (
                "1.95.0-aarch64-apple-darwin (overridden by environment variable RUSTUP_TOOLCHAIN)\n",
                SelectedBy::Environment,
            ),
            (
                "stable-aarch64-apple-darwin (default)\n",
                SelectedBy::Default,
            ),
            (
                "1.95.0-aarch64-apple-darwin (directory override for '/home/op/demo')\n",
                SelectedBy::DirectoryOverride,
            ),
            // Unknown text records `unknown`, never a guess.
            (
                "1.94.0-aarch64-apple-darwin (chosen by the operator)\n",
                SelectedBy::Unknown,
            ),
            ("", SelectedBy::Unknown),
            (
                "error: no override and no default toolchain set\n",
                SelectedBy::Unknown,
            ),
            // A path that happens to contain the default marker is still the file's attribution.
            (
                "1.94.0-x (overridden by '/srv/(default)/rust-toolchain.toml')",
                SelectedBy::ToolchainFile,
            ),
        ];
        for (index, (text, expected)) in table.iter().enumerate() {
            assert_eq!(
                parse_active_toolchain_attribution(text),
                *expected,
                "the row at this index is attributed wrongly: {index}"
            );
        }
    }

    #[test]
    fn the_absent_channel_is_taken_from_rustups_sentence_and_sanitized() {
        // Measured 2026-09-07 through the proxy and through `show active-toolchain`.
        assert_eq!(
            parse_not_installed_channel(
                "error: toolchain '1.93.0-aarch64-apple-darwin' is not installed\nhelp: run `rustup toolchain install` to install it\n"
            )
            .as_deref(),
            Some("1.93.0-aarch64-apple-darwin")
        );
        assert_eq!(
            parse_not_installed_channel(
                "error: override toolchain '1.93.0-aarch64-apple-darwin' is not installed: the RUSTUP_TOOLCHAIN environment variable specifies an uninstalled toolchain"
            )
            .as_deref(),
            Some("1.93.0-aarch64-apple-darwin")
        );
        assert_eq!(
            parse_not_installed_channel(
                "error: toolchain 'renvor-uninstallable-toolchain-name' is not installed"
            )
            .as_deref(),
            Some("renvor-uninstallable-toolchain-name")
        );
        for (index, text) in [
            "",
            "error: no default toolchain configured",
            "toolchain '' is not installed",
            "toolchain 'not a name!' is not installed",
            "toolchain 'a\u{1b}[31mb' is not installed",
            "toolchain '1.93.0' is not the one",
            "the toolchain '1.93.0' is not installed",
        ]
        .into_iter()
        .enumerate()
        {
            let accepted = parse_not_installed_channel(text).is_some();
            // The sixth row — a name after the words `the toolchain` — is the sentence rustup
            // prints and is accepted; every other row is refused.
            assert_eq!(
                accepted,
                index == 6,
                "the row at this index is classified wrongly: {index}"
            );
        }
        assert!(
            parse_not_installed_channel(&format!(
                "toolchain '{}' is not installed",
                "n".repeat(65)
            ))
            .is_none(),
            "a name over 64 characters is refused"
        );
    }

    #[test]
    fn rustup_words_are_recognised_and_a_compiler_identity_is_not() {
        for words in [
            "error: toolchain '1.93.0' is not installed",
            "error: no default toolchain configured",
            "error: no override and no default toolchain set",
            "error: rustup could not choose a version of rustc to run",
        ] {
            assert!(
                looks_like_rustup_words(words),
                "rustup's words were not recognised"
            );
        }
        assert!(!looks_like_rustup_words(RUSTC_VV));
        assert!(!looks_like_rustup_words(""));
        assert!(!looks_like_rustup_words("error: unknown flag"));
    }

    #[test]
    fn hostile_input_is_refused_and_never_echoed() {
        // A wrapper or a shim owns every byte of a `-vV` answer. Terminal control bytes and a
        // credential-shaped token planted in the host line are refused by the grammar, and the
        // error carries only the piece's NAME — asserted structurally on the rendering, with
        // fixed messages, because a failure here is the run in which printing would matter.
        let hostile_host = RUSTC_VV.replace(
            "host: aarch64-apple-darwin",
            "host: aarch64\u{1b}[31m-apple-darwin password=pw-9f-fake",
        );
        let error = parse_rustc_vv(&hostile_host).expect_err("a hostile host line is refused");
        assert_eq!(error, GrammarError::Malformed("host"));
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains("pw-9f"), "the error echoed the input");
        assert!(
            !rendered.contains('\u{1b}'),
            "the error echoed a control byte"
        );
        let hostile_release = RUSTC_VV.replace("release: 1.94.0", "release: 1.94.0\u{7}");
        assert_eq!(
            parse_rustc_vv(&hostile_release).expect_err("a control byte is refused"),
            GrammarError::Malformed("release")
        );
        // The channel parser never returns a doctored name either.
        assert!(parse_not_installed_channel("toolchain 'x\u{7}y' is not installed").is_none());
    }
}
