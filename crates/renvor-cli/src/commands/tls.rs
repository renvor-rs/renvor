//! `renvor tls trust` — the consent boundary for a trust-store modification that does not exist yet.
//!
//! # Why a command exists for an operation that is unavailable
//!
//! FR-037, in full:
//!
//! > *"Any future modification of the operating system trust store MUST be preceded by a description
//! > of exactly what will change and MUST require explicit consent; a non-interactive run MUST
//! > require an explicit flag whose name states its effect. **The boundary MUST be built now so that
//! > it is never built under pressure later.**"*
//!
//! The last sentence is the requirement. A consent gate designed in the same week as the feature it
//! guards is designed by somebody who wants the feature to work, and it ends up with a
//! `--force` that skips it. Built now, against an operation that cannot succeed, it has nothing to
//! be relaxed for.
//!
//! # What this command does, in order
//!
//! 1. **Describes exactly what would change** — which store, on this platform, and what would go
//!    into it. Not "modifies the trust store".
//! 2. **Requires explicit consent**: a confirmation on a terminal, or [`CONSENT_FLAG`] without one.
//!    A non-interactive run with no flag is refused and told the flag's name.
//! 3. **Refuses to do it anyway**, because Phase 003 ships no transport and a certificate issued now
//!    would protect nothing (FR-036).
//!
//! Step 3 after step 2 looks redundant and is not: it is what makes steps 1 and 2 **executable
//! code with tests** rather than a design note that a later phase has to build from scratch. The
//! consent path and the refusal path are both exercised by `tests/tls_consent.rs`.
//!
//! # This command modifies nothing, on every path
//!
//! SC-010 requires **0** trust-store modifications across every command in the phase, consent given
//! **and** withheld — deliberately stronger than "none without consent", because a phase that
//! issues no certificate should be able to make the absolute claim. `tests/tls_consent.rs`
//! snapshots the real trust store around every command and asserts it is byte-identical.

use crate::exit::{CliError, Code, Exit};
use crate::output::Reporter;

/// The non-interactive consent flag.
///
/// **The name states the effect**, which FR-037 requires and which is the whole point: someone
/// pasting this into CI has to type what it does. `--yes` deliberately does **not** grant this
/// consent — contract C-1 scopes `--yes` to the review screen, and a general-purpose "assume yes"
/// that also installs a certificate authority is exactly the accident this boundary exists to
/// prevent. `tests/tls_consent.rs` asserts that `--yes` alone is refused.
pub const CONSENT_FLAG: &str = "--i-understand-this-modifies-my-system-trust-store";

/// The trust store this platform would modify, named concretely.
///
/// A description that says "the system trust store" tells an operator nothing they can go and
/// inspect. These are the locations a future implementation would have to touch.
#[must_use]
pub fn trust_store_description() -> &'static str {
    if cfg!(target_os = "macos") {
        "the login keychain (`~/Library/Keychains/login.keychain-db`), and the System keychain if a \
         system-wide certificate were requested"
    } else if cfg!(target_os = "windows") {
        "the current user's `Root` certificate store (`Cert:\\CurrentUser\\Root`)"
    } else {
        "the system anchor directory (`/usr/local/share/ca-certificates` on Debian-derived systems, \
         `/etc/pki/ca-trust/source/anchors` on Fedora-derived ones), followed by the vendor's \
         trust-store update command"
    }
}

/// Prints exactly what would change. **Called before consent is sought, on every path.**
fn describe(reporter: &Reporter) {
    reporter.note("`renvor tls trust` would, if it were available:");
    reporter.note(&format!(
        "  1. generate a local certificate authority and store its PRIVATE KEY under renvor's \
         configuration directory"
    ));
    reporter.note(&format!("  2. install that authority's certificate into {}", trust_store_description()));
    reporter.note(
        "  3. issue a leaf certificate for the project's local development domain, signed by it",
    );
    reporter.note("");
    reporter.note(
        "  Consequence: any certificate that authority signs would be trusted by every program on \
         this machine that uses the system trust store, until it is removed by hand.",
    );
    reporter.note("");
}

/// Runs `renvor tls trust`.
///
/// # Errors
///
/// [`Code::Usage`] when a non-interactive run omits [`CONSENT_FLAG`], [`Code::Cancelled`] (exit
/// `4`) when consent is withheld, and [`Code::ReservedForLaterPhase`] (exit `3`) when consent is
/// given — because the operation is not available in this phase.
pub fn trust(
    reporter: &Reporter,
    consent_flag_given: bool,
    terminal: bool,
    dry_run: bool,
) -> Result<Exit, CliError> {
    // The description comes first on EVERY path, including the dry run and including the paths that
    // end in a refusal. An operator who is about to be asked a question has to be able to read what
    // they are being asked about; printing it only on the path that proceeds would show it to
    // exactly the people who already said yes.
    describe(reporter);

    if dry_run {
        return Ok(reporter.finish(
            "tls",
            "dry run: nothing was asked and nothing would be changed",
            serde_json::json!({
                "dryRun": true,
                "trustStoreModifications": 0,
                "wouldModify": trust_store_description(),
            }),
        ));
    }

    // ── CONSENT ─────────────────────────────────────────────────────────────────────────
    let consented = if consent_flag_given {
        true
    } else if terminal {
        inquire::Confirm::new("Install a new certificate authority into your system trust store?")
            .with_default(false) // NEVER default to yes. See the module header.
            .prompt()
            .map_err(|error| match error {
                inquire::error::InquireError::OperationCanceled
                | inquire::error::InquireError::OperationInterrupted => CliError::new(
                    Code::Cancelled,
                    "consent withheld; the trust store was not modified",
                ),
                other => CliError::new(Code::Internal, format!("the prompt failed: {other}")),
            })?
    } else {
        // US6 acceptance scenario 3. Naming the flag is the point: a refusal that says "consent
        // required" and stops leaves an automation author guessing.
        return Err(CliError::new(
            Code::Usage,
            format!(
                "this would modify your system trust store and there is no terminal to ask on. \
                 Pass `{CONSENT_FLAG}` to consent explicitly. `--yes` does not grant this consent \
                 and is not intended to. Nothing was changed"
            ),
        )
        .with("required", CONSENT_FLAG)
        .with("trustStoreModifications", "0"));
    };

    if !consented {
        // US6 acceptance scenario 2: report what was skipped and how to do it later.
        return Err(CliError::new(
            Code::Cancelled,
            format!(
                "consent withheld; the trust store was not modified and nothing else was affected. \
                 To do this later, run `renvor tls trust` again, or pass `{CONSENT_FLAG}` in a \
                 non-interactive run"
            ),
        )
        .with("trustStoreModifications", "0"));
    }

    // ── AND THEN REFUSE ─────────────────────────────────────────────────────────────────
    //
    // FR-036: this phase performs no trust-store modification and issues no certificate. Exiting 0
    // here would be the failure mode the requirement names — "silently succeeding" — because a
    // caller would record that the certificate was installed.
    Err(CliError::new(
        Code::ReservedForLaterPhase,
        "consent recorded, and the operation is NOT AVAILABLE in this phase: renvor ships no \
         transport yet, so a certificate issued now would protect nothing. Nothing was changed. \
         This becomes available in Phase 004 (the first real transport)",
    )
    .with("flag", "tls trust")
    .with("phase", "Phase 004 (the first real transport)")
    .with("trustStoreModifications", "0"))
}
