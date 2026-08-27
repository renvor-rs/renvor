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
//! must be identical either way. Granting consent and still observing zero modifications is
//! the interesting half — it distinguishes "the gate held" from "there is nothing behind the gate",
//! and only the second is what FR-036 promises.
//!
//! # This test reads the real trust store and never writes to it
//!
//! It observes whatever trust-store artifacts this platform actually has, before and after. It has
//! no code path that modifies one. `the_snapshot_can_detect_a_change` is the control that keeps the
//! comparison from being vacuous: it proves the mechanism notices a difference, against a fixture
//! directory rather than against anything real.
//!
//! # What is observed on macOS, and why it is not the keychain file (finding F-3)
//!
//! **This suite used to fingerprint `~/Library/Keychains/login.keychain-db` as bytes, and that was
//! a defect.** The assertion was right and the observation window was wrong.
//!
//! `login.keychain-db` is not a trust store. It is a **credential database** whose certificates are
//! one tenant among many — generic passwords, internet passwords, keys and certificates share the
//! same file. Anything on the machine that touches the login keychain rewrites it: a credential
//! helper, Docker Desktop, a browser, a background macOS service. When one of those wrote inside
//! the before/after window, the comparison changed and the test attributed a stranger's write to
//! `renvor new`:
//!
//! ```text
//! test no_command_in_this_phase_modifies_the_trust_store ... FAILED
//! panicked at crates/renvor-cli/tests/tls_consent.rs:154:5
//! ```
//!
//! The correction is to observe **certificate and trust state** rather than the bytes of the
//! container that happens to hold it. Measured on macOS 26.3 (build 25D125), against a throwaway
//! keychain so that nothing real was touched:
//!
//! | Observation | Under three unrelated `add-generic-password` writes |
//! |---|---|
//! | the file's bytes | **changes** — `20460` bytes to `24232` |
//! | `security find-certificate -a -p` | **identical** |
//! | `security add-certificates` | **noticed**, so the new observation is not merely quiet |
//!
//! `macos_observation_boundary` holds all three of those as tests rather than as a claim in a
//! comment.
//!
//! ## Every macOS query here is read-only, and covers exactly what the command says it would touch
//!
//! `commands::tls::trust_store_description` promises *"the login keychain
//! (`~/Library/Keychains/login.keychain-db`), and the System keychain if a system-wide certificate
//! were requested"*. Both are observed, as certificates. Possession is not trust, so the two
//! writable trust-settings domains are observed as well — a certificate sitting in a keychain is
//! inert until a trust setting says otherwise, and "install a certificate authority" means both.
//! `/etc/ssl/cert.pem` continues to be observed as a file, because it is one.
//!
//! ## The failure mode this observation is most likely to have
//!
//! **`security find-certificate` does not fail on a path that is not a keychain.** Measured: a
//! nonexistent path, a directory, and a plain text file all exit `0` and print nothing. So a
//! mistyped path would look exactly like a keychain holding no certificates, and every comparison
//! in this file would silently compare nothing to nothing while reporting a pass. Two guards stand
//! against that, and both are load-bearing: [`keychain_certificates`] refuses to accept a path that
//! is not a file, and `the_snapshot_sees_something_real` requires the System keychain to yield an
//! actual certificate.

mod harness;

use std::path::{Path, PathBuf};
use std::process::Command;

use harness::{Terminal, renvor};

/// One named, comparable observation of certificate or trust state.
///
/// A pair rather than a bare string so that a failing comparison names **which** store moved,
/// rather than printing two large blobs and leaving the reader to diff them.
type Observation = (String, String);

/// The macOS System keychain, named by `trust_store_description()` as a store this command would
/// touch if the operation existed.
const MACOS_SYSTEM_KEYCHAIN: &str = "/Library/Keychains/System.keychain";

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

/// The trust-store artifacts this platform holds **as files**.
///
/// On macOS this is deliberately only `/etc/ssl/cert.pem`. The keychains are observed as
/// certificates instead — see the module header, finding F-3. Linux and Windows are unchanged by
/// that correction: their artifacts are certificate bundles and anchor directories, which do not
/// carry unrelated tenants the way a keychain does, and no measurement showed them moving.
fn trust_store_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if cfg!(target_os = "macos") {
        paths.push(PathBuf::from("/etc/ssl/cert.pem"));
    } else if !cfg!(target_os = "windows") {
        paths.push(PathBuf::from("/etc/ssl/certs/ca-certificates.crt"));
        paths.push(PathBuf::from("/usr/local/share/ca-certificates"));
        paths.push(PathBuf::from("/etc/pki/ca-trust/source/anchors"));
    }
    paths.into_iter().filter(|path| path.exists()).collect()
}

/// Renders a file fingerprint as named observations.
fn file_observations(paths: &[PathBuf]) -> Vec<Observation> {
    fingerprint(paths)
        .into_iter()
        .map(|(path, length, digest)| (format!("file:{path}"), format!("{length}:{digest:016x}")))
        .collect()
}

/// Runs a **read-only** `security` query and returns its standard output.
///
/// Every caller passes a query subcommand. Nothing in this function's callers writes, and the one
/// module that does write (`macos_observation_boundary`) has its own runner and only ever aims it
/// at a keychain it created inside a temporary directory.
///
/// # Errors
///
/// Returns the failure text when `security` could not be executed, or exited non-zero for any
/// reason other than the one measured non-zero exit that is a real observation.
fn security_query(arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("/usr/bin/security")
        .args(arguments)
        .output()
        .map_err(|error| format!("`/usr/bin/security` could not be executed: {error}"))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    // THE ONE NON-ZERO EXIT THAT IS AN OBSERVATION RATHER THAN A FAILURE.
    //
    // MEASURED on macOS 26.3 (build 25D125): a trust-settings domain that holds nothing exits `1`
    // and says so on stderr. That is an observation of an EMPTY domain, not a failure to observe
    // one. Treating it as a failure would make this suite fail loudly on every machine that has
    // never had a user trust setting, which is most of them.
    //
    // The match is on the message rather than on the exit code, deliberately: exit `1` is also
    // what an unknown subcommand returns, and those two must not be confused.
    if stderr.contains("No Trust Settings were found") {
        return Ok(String::new());
    }

    Err(format!(
        "`security {}` exited with {} and said: {}",
        arguments.join(" "),
        output.status,
        stderr.trim()
    ))
}

/// The certificates in one keychain, as PEM — the certificate state, not the database bytes.
///
/// # Panics
///
/// When the query could not be run at all. That is deliberate and is the point of finding F-3's
/// correction: an observation that quietly degrades to "nothing" turns every comparison in this
/// file into a comparison of two empty strings, and the suite would report a pass having watched
/// nothing happen.
fn keychain_certificates(name: &str, keychain: &Path) -> Observation {
    // LOAD-BEARING. `security find-certificate` exits 0 and prints nothing for a path that is not
    // a keychain — measured against a nonexistent path, a directory, and a text file — so the exit
    // code cannot tell a mistyped path from an empty keychain. This check is the half of that
    // guard that lives here; `the_snapshot_sees_something_real` is the other half.
    //
    // A keychain that is absent is RECORDED rather than skipped, because a keychain appearing
    // during a command is itself a change and a skipped probe would not notice it.
    if !keychain.is_file() {
        return (name.to_owned(), "<no such keychain>".to_owned());
    }

    match security_query(&[
        "find-certificate",
        "-a",
        "-p",
        &keychain.display().to_string(),
    ]) {
        Ok(pem) => (name.to_owned(), pem),
        Err(why) => panic!(
            "the macOS certificate observation could not run, so this suite would be comparing \
             nothing to nothing: {why}"
        ),
    }
}

/// One trust-settings domain, as `security`'s own rendering of it.
///
/// # Panics
///
/// When the query could not be run — for the same reason as [`keychain_certificates`].
fn trust_settings(name: &str, domain: &[&str]) -> Observation {
    let mut query = vec!["dump-trust-settings"];
    query.extend_from_slice(domain);

    match security_query(&query) {
        Ok(dump) => (name.to_owned(), dump),
        Err(why) => panic!(
            "the macOS trust-settings observation could not run, so this suite would be comparing \
             nothing to nothing: {why}"
        ),
    }
}

/// Everything macOS can be asked about its certificate and trust state, read-only.
///
/// Empty on every other platform, so that this compiles and is inert off macOS rather than being
/// conditionally absent.
fn macos_observations() -> Vec<Observation> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }

    let mut observations = Vec::new();

    // The two keychains the command's own description names as the ones it would touch.
    if let Some(home) = std::env::var_os("HOME") {
        let login = PathBuf::from(&home).join("Library/Keychains/login.keychain-db");
        observations.push(keychain_certificates("login-keychain-certificates", &login));
    }
    observations.push(keychain_certificates(
        "system-keychain-certificates",
        Path::new(MACOS_SYSTEM_KEYCHAIN),
    ));

    // POSSESSION IS NOT TRUST. A certificate sitting in a keychain is inert until a trust setting
    // says otherwise, so "install a certificate authority into the trust store" means both halves,
    // and an observation of only the certificates would miss a change that granted trust to one
    // already present. The user and admin domains are the two that `security add-trusted-cert` can
    // write; the system domain is Apple's own and is not a store this command claims it could
    // modify, so it is left out rather than watched for no reason.
    observations.push(trust_settings("user-trust-settings", &[]));
    observations.push(trust_settings("admin-trust-settings", &["-d"]));

    observations
}

/// The Windows `CurrentUser\Root` store, as `certutil`'s own rendering of it.
fn windows_root_store() -> Option<String> {
    if !cfg!(target_os = "windows") {
        return None;
    }
    let output = Command::new("certutil")
        .args(["-user", "-store", "Root"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Everything this platform can observe about its trust store, in one comparable value.
fn snapshot() -> Vec<Observation> {
    let mut observations = file_observations(&trust_store_paths());
    observations.extend(macos_observations());
    if let Some(store) = windows_root_store() {
        observations.push(("windows-currentuser-root".to_owned(), store));
    }
    observations.sort();
    observations
}

#[test]
fn the_snapshot_sees_something_real() {
    // THE LOAD-BEARING CONTROL. Every other test here compares a snapshot to a snapshot; if
    // `snapshot()` returned nothing — a wrong path, an unreadable file, `certutil` missing — all of
    // them would pass while observing nothing at all. That is the failure mode a trust-store test
    // is most likely to have and least likely to notice.
    let observations = snapshot();

    if cfg!(target_os = "windows") {
        let (_, store) = observations
            .iter()
            .find(|(name, _)| name == "windows-currentuser-root")
            .expect("`certutil -user -store Root` must be runnable on Windows");
        assert!(
            !store.trim().is_empty(),
            "certutil produced nothing that looks like a certificate store:\n{store}"
        );
    } else if cfg!(target_os = "macos") {
        // macOS needs a STRONGER control than "the set is non-empty", because its queries cannot
        // report a mistyped path: `security find-certificate` exits 0 and prints nothing for a
        // path that is not a keychain at all. So the control asserts a fact that is true of every
        // real macOS installation and false of a broken observation — the System keychain holds
        // Apple's roots, so an empty result there means the observation is broken rather than that
        // the machine has no certificates.
        let (_, system) = observations
            .iter()
            .find(|(name, _)| name == "system-keychain-certificates")
            .expect("the System keychain must be observed on macOS");
        assert!(
            system.contains("BEGIN CERTIFICATE"),
            "the System keychain produced no certificate, so every comparison in this file would \
             be comparing nothing to nothing. Queried: {MACOS_SYSTEM_KEYCHAIN}"
        );
        // ...AND THE SET IS COMPLETE, which is a different requirement from "the set is not
        // empty" and needs its own assertion. Measured: deleting both trust-settings probes left
        // all thirteen tests in this file green, because every other test compares a snapshot to a
        // snapshot and a SMALLER snapshot still matches itself. A probe can therefore be removed
        // without any comparison noticing, so "observes nothing" has to be read as "observes less
        // than it claims to" rather than only as "observes zero".
        for expected in [
            "login-keychain-certificates",
            "system-keychain-certificates",
            "user-trust-settings",
            "admin-trust-settings",
        ] {
            assert!(
                observations.iter().any(|(name, _)| name == expected),
                "the macOS observation no longer includes `{expected}`, so this suite claims a \
                 breadth it does not have: {:?}",
                observations
                    .iter()
                    .map(|(name, _)| name)
                    .collect::<Vec<_>>()
            );
        }
        assert!(
            observations
                .iter()
                .any(|(name, _)| name == "file:/etc/ssl/cert.pem"),
            "`/etc/ssl/cert.pem` was not observed, so the file half of the macOS observation is \
             missing: {:?}",
            observations
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        );
    } else {
        assert!(
            !observations.is_empty(),
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

    // THIS ASSERTION USED TO REQUIRE THE STRING "Phase 004", AND THAT WAS THE DEFECT.
    //
    // The refusal promised the capability would arrive in Phase 004. Phase 004 shipped a
    // transport, and this command still refuses — so the promise was false, and a test asserting
    // the false promise was holding it in place.
    //
    // A `phase` detail is still required by contract C-2 for this code. What it may no longer do
    // is name a phase that has already shipped without delivering it.
    let phase = document["error"]["details"]["phase"]
        .as_str()
        .expect("a phase is named");

    assert!(
        !phase.is_empty(),
        "the refusal must carry a phase: {document}"
    );
    assert!(
        !phase.contains("Phase 004"),
        "the refusal still promises Phase 004, which has shipped without delivering this: {document}"
    );
    assert!(
        !document["error"]["message"]
            .as_str()
            .expect("a message")
            .contains("becomes available in Phase 004"),
        "the message still promises Phase 004: {document}"
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

/// The three proofs that finding F-3's correction is real, plus the guard that keeps a broken
/// query from looking like an empty store.
///
/// # Why `#[cfg]` and not a runtime `if`
///
/// On Linux and Windows these tests **do not exist**, which is honest. A test that returns early
/// on the platform it cannot exercise reports a pass it did not earn, and this file already had one
/// finding about an observation that quietly watched the wrong thing.
///
/// # Nothing here touches a real trust store
///
/// Every write goes to a keychain created inside a `tempfile::tempdir()`, which is never added to
/// the search list and never made the default, so no other program on the machine will consult it.
/// [`isolated_keychain`] refuses outright to hand back a path that is the real login keychain.
#[cfg(target_os = "macos")]
mod macos_observation_boundary {
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::{Arc, Barrier};

    use super::{fingerprint, keychain_certificates, security_query};

    /// A synthetic certificate authority, used only to prove the observation notices a certificate.
    ///
    /// **Certificate only.** The private key was discarded at generation and has never existed in
    /// this repository, so this authority cannot sign anything — adding it to a store would grant
    /// nothing to anybody. It is added only to a throwaway keychain inside a temporary directory.
    const SYNTHETIC_CERTIFICATE: &str = concat!(
        "-----BEGIN CERTIFICATE-----\n",
        "MIIDkzCCAnugAwIBAgIUGS0MsDUq2ZgyM8MiH3ZWp40tLTMwDQYJKoZIhvcNAQEL\n",
        "BQAwWDE4MDYGA1UEAwwvcmVudm9yLXRscy1jb25zZW50LWZpeHR1cmUtbm90LWEt\n",
        "cmVhbC1hdXRob3JpdHkxHDAaBgNVBAoME3JlbnZvciB0ZXN0IGZpeHR1cmUwIBcN\n",
        "MjYwODI3MTI1NTA2WhgPMjEyNjA4MDMxMjU1MDZaMFgxODA2BgNVBAMML3JlbnZv\n",
        "ci10bHMtY29uc2VudC1maXh0dXJlLW5vdC1hLXJlYWwtYXV0aG9yaXR5MRwwGgYD\n",
        "VQQKDBNyZW52b3IgdGVzdCBmaXh0dXJlMIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8A\n",
        "MIIBCgKCAQEAsTIrc9k/vjsPyG6QIe6IcqYVorcYvvM6ErTF35SFTmt3DEXsBsuk\n",
        "JjDUncMabDgzmVkJiik+DhegcIHut3U99oAXpBShmxxd4fnM6iIe1UZ3g1GFrYHx\n",
        "u4WJGKuSf9kP6DlULjXEi4d1Jjj+5fPlcn5EEEFofx8ThLPJWZx9RQlzsJlficlJ\n",
        "R7kGT/DlwKSjQfmYjDKpvRCFf5I5oeoLV+hkrbXMyEVPS2y36uYXGwbABzWMr8m7\n",
        "S3aTNGSCXCzBQOpl2K7TlbOIrGx0QV9rd6qPT8Zhvml8jpCdGedEPSvYQbWnL5H5\n",
        "ECd/c40M7e/DROGy6nN1zO8elCwJy6ekawIDAQABo1MwUTAdBgNVHQ4EFgQUiq48\n",
        "PRwejLjC5dIVxTOnCiVQt7owHwYDVR0jBBgwFoAUiq48PRwejLjC5dIVxTOnCiVQ\n",
        "t7owDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAAW19Bg6O7+3t\n",
        "XBOQsUxbtXQj/nowweIjFXbQnmClp7EHYAEoBtbi2Dm0xMV1jBbtU9dJ3+xAHPT2\n",
        "heFanqKF8ezCCC5IM7K2730LNgJUc6yiEy3czrTQwbt73sizlLSFFQMHtRAr6H+8\n",
        "2N+npQZ1K1Gk+U18crhB1XuSjyX6xGjEwbpvZGJfwk7GGBhVga/bhM55euXmHK/v\n",
        "ugeMKPDeRqvOnuBt36Fslkdrm6Ibl8Mkes4U0JxT3qSlC1vxVoLUi+ves34FeQOK\n",
        "ekWEFC2ISK3qucH6n/V8hQqysxncH2Fzkbaq+hLfN5uWsfFlXpLRkQUQTipF2JJY\n",
        "laNjuIAR3w==\n",
        "-----END CERTIFICATE-----\n",
    );

    /// The password for the fixture keychain. Not a secret: it protects a keychain that exists for
    /// the duration of one test, in a temporary directory, holding invented data.
    const FIXTURE_PASSWORD: &str = "renvor-fixture-keychain";

    /// The fixture keychain's file name, and it is **deliberately not `login.keychain-db`**.
    ///
    /// # This name is load-bearing, and the reason was measured rather than guessed
    ///
    /// `security create-keychain` **adds the new keychain to the user's keychain search list** —
    /// but only when the file is called `login.keychain-db`. macOS special-cases that name and
    /// registers the result as a login keychain. Measured on macOS 26.3 (build 25D125), creating
    /// each of three keychains in a temporary directory:
    ///
    /// | File name | Search-list entries, before → after |
    /// |---|---|
    /// | `probe.keychain-db` | 3 → 3 |
    /// | `login.keychain-db` | 3 → **4** |
    /// | `other.keychain` | 3 → 3 |
    ///
    /// An earlier draft of this module did call the fixture `login.keychain-db`, to mirror the
    /// path the superseded observation derived from `$HOME`. Running the suite a few dozen times
    /// left **twenty-two dead fixture entries** in the developer's search list, each one a path
    /// under `/private/var/folders` that no longer existed. A test that quietly accumulates state
    /// on the machine it runs on is the same class of defect as F-3 itself — an effect outside the
    /// window anybody is watching.
    ///
    /// Renaming the fixture avoids the special case entirely, which is better than cleaning up
    /// after it: there is no window in which the list is wrong, and nothing to get right on the
    /// panicking path. `the_fixture_never_changes_the_keychain_search_list` holds this measurement
    /// as a test, so that renaming the fixture back cannot pass quietly.
    const FIXTURE_KEYCHAIN: &str = "renvor-fixture.keychain-db";

    /// Runs a `security` subcommand that **writes**, and fails loudly if it did not.
    ///
    /// Separate from [`security_query`] on purpose: the read path must never grow a writing
    /// subcommand by accident, so the two have different runners and only this one lives inside
    /// the module that owns a fixture keychain.
    fn write_to_fixture(arguments: &[&str]) {
        let output = Command::new("/usr/bin/security")
            .args(arguments)
            .output()
            .expect("`/usr/bin/security` is runnable");
        assert!(
            output.status.success(),
            "`security {}` failed with {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Creates a keychain inside `directory`, and proves it is not a real one.
    ///
    /// The keychain is **not** added to the search list and **not** made the default, so no other
    /// program on this machine will consult it. Neither `list-keychains -s` nor `default-keychain
    /// -s` is called here or anywhere else in this file — and, less obviously, the fixture is not
    /// named `login.keychain-db`, because `create-keychain` registers *that* name in the search
    /// list all by itself. See [`FIXTURE_KEYCHAIN`].
    fn isolated_keychain(directory: &Path) -> PathBuf {
        // A FAKE `HOME` LAYOUT, not a bare temporary file. The superseded observation derived its
        // path as `$HOME/Library/Keychains/login.keychain-db`, so reproducing what it used to do
        // means reproducing the shape it used to look at — under a `HOME` that is a temporary
        // directory, so the real one is never involved.
        let keychains = directory.join("Library/Keychains");
        std::fs::create_dir_all(&keychains).expect("a fake HOME keychain directory");
        let keychain = keychains.join(FIXTURE_KEYCHAIN);

        // THE SAFETY ASSERTION THIS MODULE CANNOT DO WITHOUT. Everything below writes.
        let real_login = std::env::var_os("HOME")
            .map(|home| PathBuf::from(&home).join("Library/Keychains/login.keychain-db"));
        assert_ne!(
            Some(keychain.clone()),
            real_login,
            "REFUSING TO WRITE TO THE REAL LOGIN KEYCHAIN"
        );

        let path = keychain.display().to_string();
        write_to_fixture(&["create-keychain", "-p", FIXTURE_PASSWORD, &path]);
        write_to_fixture(&["unlock-keychain", "-p", FIXTURE_PASSWORD, &path]);
        assert!(
            keychain.is_file(),
            "the fixture keychain was not created at {path}"
        );
        keychain
    }

    /// **The observation this suite used to make**, reconstructed so that the correction can be
    /// measured against it rather than merely asserted to be better.
    ///
    /// This is what `trust_store_paths()` returned on macOS before finding F-3: the bytes of
    /// `$HOME/Library/Keychains/login.keychain-db`. It is kept, and kept named, because a
    /// correction whose "before" is deleted cannot be checked by a reader.
    fn superseded_file_observation(home: &Path) -> Vec<(String, u64, u64)> {
        fingerprint(&[home.join("Library/Keychains").join(FIXTURE_KEYCHAIN)])
    }

    /// The user's keychain search list, as `security`'s own rendering of it.
    fn keychain_search_list() -> String {
        let output = Command::new("/usr/bin/security")
            .arg("list-keychains")
            .output()
            .expect("`security list-keychains` is runnable");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn the_fixture_never_changes_the_keychain_search_list() {
        // THE REGRESSION GUARD FOR A DEFECT THIS MODULE ALREADY SHIPPED ONCE.
        //
        // `security create-keychain` registers the new keychain in the user's search list when the
        // file is named `login.keychain-db`, and an earlier draft of this module named it exactly
        // that in order to mirror the path the superseded observation read. Running the suite a few
        // dozen times left twenty-two dead entries behind on a developer's machine — every one of
        // them a path under `/private/var/folders` that no longer existed — while `cargo test`
        // reported nothing but passes.
        //
        // The assertion is on the PROPERTY rather than on the file name, so it holds however the
        // fixture comes to be built, and fails if the name is ever changed back.
        let before = keychain_search_list();
        let base = tempfile::tempdir().expect("a temporary directory");
        let keychain = isolated_keychain(base.path());
        let after = keychain_search_list();

        // CANONICALISE BEFORE COMPARING. On macOS `/var` is a symlink to `/private/var`, so the
        // path `tempfile` hands back and the path `security` records are two spellings of one
        // file. The first version of this guard compared them literally, and therefore MISSED the
        // regression it exists to catch: the search list grew from three entries to five while all
        // fourteen tests reported a pass. A guard that cannot fail is the same defect as F-3, one
        // layer further down, so it is written here as a measurement rather than as an intention.
        let recorded = keychain
            .canonicalize()
            .expect("the fixture keychain exists and can be resolved")
            .display()
            .to_string();

        assert!(
            !after.contains(&recorded),
            "CREATING THE FIXTURE KEYCHAIN ADDED IT TO THE USER'S KEYCHAIN SEARCH LIST. A test \
             fixture must not accumulate state on the machine it runs on. See FIXTURE_KEYCHAIN \
             for the measurement. Looking for {recorded} in:\n{after}"
        );
        assert_eq!(
            before, after,
            "the keychain search list moved while a fixture keychain was created"
        );
    }

    #[test]
    fn credential_churn_moves_the_keychain_file_but_not_its_certificates() {
        // FINDING F-3, REPRODUCED AND THEN CLOSED, IN ONE TEST.
        //
        // The first assertion is THE DEFECT: the old observation fingerprinted the keychain as
        // bytes, and an unrelated credential write moves those bytes. On a developer's machine that
        // write arrives from a credential helper, Docker Desktop or a browser, lands inside the
        // before/after window, and gets attributed to whichever `renvor` command was running.
        //
        // The second assertion is THE CORRECTION: the same write leaves the certificate observation
        // identical, because a generic password is not a certificate.
        //
        // Both run against a keychain created in a temporary directory. THE REAL LOGIN KEYCHAIN IS
        // NEVER OPENED FOR WRITING BY THIS TEST.
        let fake_home = tempfile::tempdir().expect("a temporary directory");
        let keychain = isolated_keychain(fake_home.path());

        let file_before = superseded_file_observation(fake_home.path());
        let certificates_before = keychain_certificates("fixture", &keychain);
        assert!(
            !file_before.is_empty(),
            "the superseded observation saw nothing under the fake HOME, so the first assertion \
             below would be comparing nothing to nothing rather than reproducing a defect"
        );

        // The writer runs INSIDE the observation window, which is the shape of the real failure.
        // A barrier rather than a sleep: the window is opened, the writer is released into it, and
        // the window closes only once the writer has finished. Nothing here depends on timing, so
        // nothing here can become the next flake.
        let barrier = Arc::new(Barrier::new(2));
        let writer = {
            let barrier = Arc::clone(&barrier);
            let keychain = keychain.clone();
            std::thread::spawn(move || {
                let path = keychain.display().to_string();
                barrier.wait();
                for index in 0..3 {
                    let account = format!("account{index}");
                    let service = format!("service{index}");
                    write_to_fixture(&[
                        "add-generic-password",
                        "-a",
                        &account,
                        "-s",
                        &service,
                        "-w",
                        "an unrelated secret",
                        "-A",
                        &path,
                    ]);
                }
            })
        };
        barrier.wait();
        writer.join().expect("the unrelated writer finished");

        let file_after = superseded_file_observation(fake_home.path());
        let certificates_after = keychain_certificates("fixture", &keychain);

        assert_ne!(
            file_before, file_after,
            "THE DEFECT DID NOT REPRODUCE. If an unrelated credential write no longer moves the \
             keychain file, then the reason this correction exists needs measuring again rather \
             than assuming — do not delete this test, re-measure it"
        );
        assert_eq!(
            certificates_before, certificates_after,
            "THE CORRECTION IS NOT WORKING: an unrelated credential write changed the certificate \
             observation, which is the false attribution finding F-3 exists to remove"
        );
    }

    #[test]
    fn a_certificate_added_to_the_fixture_is_noticed() {
        // The counterpart control, and the reason the test above is not satisfied by an observation
        // that ignores everything: a snapshot that never changes would pass every comparison in
        // this file while proving nothing at all.
        let base = tempfile::tempdir().expect("a temporary directory");
        let keychain = isolated_keychain(base.path());
        let certificate = base.path().join("synthetic-authority.pem");
        std::fs::write(&certificate, SYNTHETIC_CERTIFICATE).expect("written");

        let before = keychain_certificates("fixture", &keychain);
        assert_eq!(
            before.1, "",
            "a freshly created keychain must hold no certificates, or this test is measuring \
             something it did not put there"
        );

        write_to_fixture(&[
            "add-certificates",
            "-k",
            &keychain.display().to_string(),
            &certificate.display().to_string(),
        ]);

        let after = keychain_certificates("fixture", &keychain);
        assert_ne!(
            before, after,
            "a certificate added to the keychain was NOT noticed, so the observation is quiet \
             rather than correct"
        );
        assert!(
            after.1.contains("BEGIN CERTIFICATE"),
            "the observation should have read back the certificate: {after:?}"
        );
    }

    #[test]
    fn a_query_that_cannot_run_is_an_error_and_not_an_empty_observation() {
        // THE GUARD THAT KEEPS "THE TOOL FAILED" FROM LOOKING LIKE "THE STORE IS EMPTY".
        //
        // Without it, a `security` that stopped working would turn every comparison in this file
        // into a comparison of two empty strings, and the suite would go green having watched
        // nothing happen — the same class of defect as F-3 itself, one layer down.
        let failed = security_query(&["not-a-real-subcommand"]);
        assert!(
            failed.is_err(),
            "a `security` query that failed must not be reported as an empty observation: {failed:?}"
        );

        // ...and the one non-zero exit that IS a real observation still passes through. MEASURED:
        // an empty trust-settings domain exits 1 with "No Trust Settings were found." A machine
        // that HAS user trust settings exits 0. Both are observations, so both must be `Ok`.
        let domain = security_query(&["dump-trust-settings"]);
        assert!(
            domain.is_ok(),
            "an empty trust-settings domain is an observation, not a failure: {domain:?}"
        );
    }

    #[test]
    fn a_path_that_is_not_a_keychain_is_recorded_rather_than_silently_empty() {
        // `security find-certificate` exits 0 and prints NOTHING for a nonexistent path, a
        // directory, and a plain text file — measured on macOS 26.3. So the existence check in
        // `keychain_certificates` is the only thing standing between a mistyped path and a suite
        // that compares nothing to nothing forever.
        let base = tempfile::tempdir().expect("a temporary directory");

        let missing = keychain_certificates("probe", &base.path().join("nothing-here.keychain-db"));
        assert_eq!(
            missing.1, "<no such keychain>",
            "an absent keychain must be RECORDED, so that one appearing is a change"
        );

        let directory = keychain_certificates("probe", base.path());
        assert_eq!(
            directory.1, "<no such keychain>",
            "a directory is not a keychain and must not be observed as an empty one"
        );
    }
}
