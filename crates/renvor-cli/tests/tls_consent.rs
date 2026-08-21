//! SC-010: **0** trust-store modifications, across every command in the phase.
//!
//! # The assertion is absolute, and that is deliberate
//!
//! SC-010 does not say "no modification without consent". It says zero, *"verified by comparing the
//! trust store before and after every command in the phase, with consent both given and withheld"*,
//! and the spec explains why: a phase that ships no certificate issuance should be able to make the
//! absolute claim, and an absolute claim is the one a reader can check. "None without consent"
//! requires trusting that the consent gate is correct; "none at all" does not.
//!
//! So every command below is run **with consent granted** as well as withheld, and the trust store
//! must be byte-identical either way. Granting consent and still observing zero modifications is
//! the interesting half — it distinguishes "the gate held" from "there is nothing behind the gate",
//! and only the second is what FR-036 promises.
//!
//! # This test reads the real trust store and never writes to it
//!
//! It hashes whatever trust-store artifacts this platform actually has, before and after. It has no
//! code path that modifies one. `the_snapshot_can_detect_a_change` is the control that keeps the
//! comparison from being vacuous: it proves the mechanism notices a difference, against a fixture
//! directory rather than against anything real.

mod harness;

use std::path::{Path, PathBuf};

use harness::{Terminal, renvor};

/// A content fingerprint of a set of files, for before/after comparison.
///
/// Records `(path, length, digest)` per file. The digest is FNV-1a rather than SHA-256: this
/// compares a snapshot against another taken seconds later on the same machine, so collision
/// resistance is not the property needed.
fn fingerprint(paths: &[PathBuf]) -> Vec<(String, u64, u64)> {
    let mut entries = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut children: Vec<PathBuf> = std::fs::read_dir(path)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect();
            children.sort();
            entries.extend(fingerprint(&children));
        } else if let Ok(bytes) = std::fs::read(path) {
            let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
            for byte in &bytes {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            entries.push((path.display().to_string(), bytes.len() as u64, hash));
        }
    }
    entries.sort();
    entries
}

/// The trust-store artifacts this platform has, as file paths.
fn trust_store_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(&home).join("Library/Keychains/login.keychain-db"));
        }
        paths.push(PathBuf::from("/etc/ssl/cert.pem"));
    } else if !cfg!(target_os = "windows") {
        paths.push(PathBuf::from("/etc/ssl/certs/ca-certificates.crt"));
        paths.push(PathBuf::from("/usr/local/share/ca-certificates"));
        paths.push(PathBuf::from("/etc/pki/ca-trust/source/anchors"));
    }
    paths.into_iter().filter(|path| path.exists()).collect()
}

/// The Windows `CurrentUser\Root` store, as `certutil`'s own rendering of it.
fn windows_root_store() -> Option<String> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let output = std::process::Command::new("certutil")
        .args(["-user", "-store", "Root"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Everything this platform can observe about its trust store, in one comparable value.
fn snapshot() -> (Vec<(String, u64, u64)>, Option<String>) {
    (fingerprint(&trust_store_paths()), windows_root_store())
}

#[test]
fn the_snapshot_sees_something_real() {
    // THE LOAD-BEARING CONTROL. Every other test here compares a snapshot to a snapshot; if
    // `snapshot()` returned nothing — a wrong path, an unreadable file, `certutil` missing — all of
    // them would pass while observing nothing at all. That is the failure mode a trust-store test
    // is most likely to have and least likely to notice.
    let (files, windows) = snapshot();
    if cfg!(target_os = "windows") {
        let store = windows.expect("`certutil -user -store Root` must be runnable on Windows");
        assert!(
            !store.trim().is_empty(),
            "certutil produced nothing that looks like a certificate store:\n{store}"
        );
    } else {
        assert!(
            !files.is_empty(),
            "no trust-store artifact was found, so every comparison in this file would be \
             comparing nothing to nothing. Paths tried: {:?}",
            trust_store_paths()
        );
    }
}

#[test]
fn the_snapshot_can_detect_a_change() {
    // The second control: the mechanism must be able to notice a difference. Demonstrated against a
    // fixture, because demonstrating it against the real trust store would require modifying the
    // real trust store — the one thing this suite exists to prove never happens.
    let base = tempfile::tempdir().expect("a temporary directory");
    let anchors = base.path().join("anchors");
    std::fs::create_dir(&anchors).expect("created");
    std::fs::write(anchors.join("existing.crt"), b"original").expect("written");

    let before = fingerprint(std::slice::from_ref(&anchors));
    assert!(!before.is_empty(), "the fixture snapshot is empty");

    std::fs::write(anchors.join("added.crt"), b"a new authority").expect("written");
    assert_ne!(
        before,
        fingerprint(std::slice::from_ref(&anchors)),
        "an added file was not noticed"
    );

    std::fs::remove_file(anchors.join("added.crt")).expect("removed");
    assert_eq!(
        before,
        fingerprint(std::slice::from_ref(&anchors)),
        "removal did not restore the snapshot"
    );

    std::fs::write(anchors.join("existing.crt"), b"tampered").expect("written");
    assert_ne!(
        before,
        fingerprint(&[anchors]),
        "a modified file was not noticed"
    );
}

/// Runs `arguments`, and returns the exit code, having asserted the trust store did not move.
fn unchanged_by(arguments: &[&str], working_directory: &Path) -> i32 {
    let before = snapshot();
    let (exit, _, _) = renvor(arguments, working_directory, &[]);
    assert_eq!(before, snapshot(), "{arguments:?} modified the trust store");
    exit
}

#[test]
fn no_command_in_this_phase_modifies_the_trust_store() {
    // SC-010's breadth requirement: EVERY command, not only the TLS one. A `renvor new` that
    // quietly installed something on the `--local-https` path would satisfy a test that only
    // watched `renvor tls`.
    let base = tempfile::tempdir().expect("a temporary directory");

    assert_eq!(unchanged_by(&["new", "plain", "--yes"], base.path()), 0);
    assert_eq!(
        unchanged_by(&["new", "secured", "--yes", "--local-https"], base.path()),
        0
    );
    assert_eq!(
        unchanged_by(&["new", "boxed", "--yes", "--container"], base.path()),
        0
    );

    let project = base.path().join("secured");
    let containerised = base.path().join("boxed");
    assert_eq!(unchanged_by(&["check"], &project), 0);
    assert_eq!(unchanged_by(&["doctor"], base.path()), 0);
    assert_eq!(unchanged_by(&["dev", "--dry-run"], &project), 0);
    for action in ["up", "down", "status", "logs"] {
        assert_eq!(
            unchanged_by(&["docker", action, "--dry-run"], &containerised),
            0
        );
    }

    // And the manifest records the intent, so this is not passing because the selection was lost.
    let manifest = std::fs::read_to_string(project.join("renvor.toml")).expect("readable");
    assert!(
        manifest.contains("local_https = \"requested\""),
        "the selection must be RECORDED — otherwise nothing was gated and nothing was proved:\n{manifest}"
    );
}

#[test]
fn consent_withheld_without_a_terminal_is_refused_and_changes_nothing() {
    // US6 acceptance scenario 3, and FR-037's "a non-interactive run MUST require an explicit flag
    // whose name states its effect".
    let base = tempfile::tempdir().expect("a temporary directory");
    let before = snapshot();
    let (exit, stdout, _) = renvor(&["tls", "trust", "--output", "json"], base.path(), &[]);
    assert_eq!(before, snapshot(), "a refused run modified the trust store");
    assert_eq!(exit, 2, "a missing consent flag is a usage error");

    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["error"]["code"], "usage");
    assert_eq!(
        document["error"]["details"]["required"],
        "--i-understand-this-modifies-my-system-trust-store",
        "the refusal must name the flag, or an automation author is left guessing: {document}"
    );
    assert_eq!(document["error"]["details"]["trustStoreModifications"], "0");
}

#[test]
fn yes_does_not_grant_trust_store_consent() {
    // Contract C-1 scopes `--yes` to the review screen. A general-purpose "assume yes" that also
    // installs a certificate authority is exactly the accident this boundary exists to prevent.
    let base = tempfile::tempdir().expect("a temporary directory");
    let before = snapshot();
    let (exit, stdout, _) = renvor(
        &["tls", "trust", "--yes", "--output", "json"],
        base.path(),
        &[],
    );
    assert_eq!(before, snapshot());
    assert_eq!(exit, 2, "`--yes` must not satisfy the consent gate");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["error"]["code"], "usage", "{document}");
}

#[test]
fn consent_granted_still_modifies_nothing_because_the_operation_is_unavailable() {
    // THE INTERESTING HALF. Consent is granted in the strongest form available — the explicit flag
    // — and the trust store is still untouched, because FR-036 makes the gated operation
    // unavailable rather than merely gated. A run that exited 0 here would be the "silently
    // succeeding" failure FR-036 names, and a caller would record a certificate that does not exist.
    let base = tempfile::tempdir().expect("a temporary directory");
    let before = snapshot();
    let (exit, stdout, _) = renvor(
        &[
            "tls",
            "trust",
            "--i-understand-this-modifies-my-system-trust-store",
            "--output",
            "json",
        ],
        base.path(),
        &[],
    );
    assert_eq!(
        before,
        snapshot(),
        "GRANTING CONSENT MODIFIED THE TRUST STORE"
    );
    assert_eq!(
        exit, 3,
        "the operation must be refused as belonging to a later phase, not succeed"
    );

    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(
        document["error"]["code"], "reserved_for_later_phase",
        "{document}"
    );
    assert_eq!(document["error"]["details"]["trustStoreModifications"], "0");
    assert!(
        document["error"]["details"]["phase"]
            .as_str()
            .expect("a phase is named")
            .contains("Phase 004"),
        "the refusal must name the phase that will support it: {document}"
    );
}

#[test]
fn the_description_precedes_the_question_and_names_this_platforms_store() {
    // FR-037's first clause: the modification "MUST be preceded by a description of exactly what
    // will change". Asserted through a terminal, because on the non-interactive path there is no
    // question for it to precede.
    let base = tempfile::tempdir().expect("a temporary directory");
    let before = snapshot();
    let mut terminal = Terminal::spawn(&["tls", "trust"], base.path(), &[]);
    terminal.expect("Install a new certificate authority into your system trust store?");

    let visible = terminal.visible();
    let question_at = visible
        .find("Install a new certificate authority")
        .expect("the question is on screen");
    let description_at = visible
        .find("would, if it were available")
        .expect("the description is on screen");
    assert!(
        description_at < question_at,
        "the description must PRECEDE the question:\n{visible}"
    );

    let expected = if cfg!(target_os = "macos") {
        "login.keychain-db"
    } else if cfg!(target_os = "windows") {
        "CurrentUser"
    } else {
        "ca-certificates"
    };
    assert!(
        visible.contains(expected),
        "the description must name this platform's store concretely, expected {expected:?}:\n{visible}"
    );
    assert!(
        visible.contains("PRIVATE KEY"),
        "the description must say a private key would be created:\n{visible}"
    );

    terminal.key("n");
    let exit = terminal.wait();
    assert_eq!(
        exit,
        4,
        "withholding consent is a cancellation\n{}",
        terminal.visible()
    );
    assert_eq!(
        before,
        snapshot(),
        "withholding consent modified the trust store"
    );
}

#[test]
fn granting_consent_at_the_prompt_also_modifies_nothing() {
    // The interactive counterpart. Both paths into the gate are exercised, because SC-010 says
    // "consent both given and withheld" and a flag is not a prompt.
    let base = tempfile::tempdir().expect("a temporary directory");
    let before = snapshot();
    let mut terminal = Terminal::spawn(&["tls", "trust"], base.path(), &[]);
    terminal.expect("Install a new certificate authority into your system trust store?");
    terminal.key("y");
    let exit = terminal.wait();

    assert_eq!(
        before,
        snapshot(),
        "GRANTING CONSENT AT THE PROMPT MODIFIED THE TRUST STORE"
    );
    assert_eq!(
        exit,
        3,
        "the operation must be refused as belonging to a later phase\n{}",
        terminal.visible()
    );
    assert!(
        terminal.visible().contains("NOT AVAILABLE"),
        "the refusal must be legible on a terminal too:\n{}",
        terminal.visible()
    );
}

#[test]
fn a_dry_run_asks_nothing_and_changes_nothing() {
    let base = tempfile::tempdir().expect("a temporary directory");
    let before = snapshot();
    let (exit, stdout, _) = renvor(
        &["tls", "trust", "--dry-run", "--output", "json"],
        base.path(),
        &[],
    );
    assert_eq!(before, snapshot());
    assert_eq!(
        exit, 0,
        "a dry run that asks nothing and does nothing is a success"
    );
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["result"]["trustStoreModifications"], 0);
    assert_eq!(document["result"]["dryRun"], true);
}
