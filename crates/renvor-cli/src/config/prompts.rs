//! The interactive wizard.
//!
//! # It produces [`Answers`] and nothing else
//!
//! The wizard cannot construct a [`super::model::ProjectConfiguration`]; it can only produce the
//! same unvalidated [`Answers`] the flag parser produces. That is what makes SC-003 — "prompt and
//! flag inputs serialize to identical configuration" — a property of the type graph rather than a
//! test that happens to pass. There is one validator and both interfaces go through it.
//!
//! # Known non-compliance with constitution principle VII, stated here as well as in the spec
//!
//! Principle VII lists eleven things the wizard should ask about: target, transport, persistence
//! model, database, auth starter, frontend, render mode, styling, desktop option, capabilities, and
//! local tooling. **This wizard asks about three of them** — target, local tooling (container and
//! local HTTPS), and capabilities (example domain, seed data).
//!
//! The other eight correspond to flags this phase **reserves and refuses**, because the phases that
//! implement them have not happened. Asking an operator to choose a database that the generator
//! cannot act on would produce a recorded choice that no generated file reflects, which
//! data-model invariant I-12 forbids outright.
//!
//! **This is a real deviation and it is not resolved here.** `tasks.md` T093a refers it to the
//! maintainer as a governing-document question: either principle VII means "ask about everything
//! the *product* will eventually support" and this phase is non-compliant, or it means "ask about
//! everything *this build* honours" and it is compliant. That ruling is not this module's to make.

use inquire::{Confirm, Text, error::InquireError};

use crate::generate::manifest::FileManifest;

use super::model::Answers;
use crate::exit::{CliError, Code};

/// Maps an `inquire` failure onto the error taxonomy.
///
/// # Why this is worth its own function
///
/// `InquireError` distinguishes `OperationCanceled` (ESC), `OperationInterrupted` (Ctrl-C), and
/// `NotTTY` as **separate typed variants** — which is exactly why research D2 chose this crate over
/// `dialoguer`. Cancellation gets exit `4` because the operator said no, and a missing terminal gets
/// a usage error because the invocation was wrong; inferring the difference from an I/O error kind
/// would get one of them wrong.
fn from_inquire(error: InquireError) -> CliError {
    match error {
        InquireError::OperationCanceled | InquireError::OperationInterrupted => CliError::new(
            Code::Cancelled,
            "cancelled; nothing was written and the destination is unchanged",
        ),
        InquireError::NotTTY => CliError::new(
            Code::Usage,
            "there is no terminal to prompt on; supply the answers as flags instead",
        ),
        other => CliError::new(
            Code::Internal,
            format!("the prompt failed unexpectedly: {other}"),
        ),
    }
}

/// Runs the wizard, filling only what the flags did not supply.
///
/// Every flag already given is **not** asked about again. A wizard that re-asks what the operator
/// already typed teaches them to stop using flags.
///
/// # Errors
///
/// [`Code::Cancelled`] (exit `4`) if the operator cancels at any prompt, or [`Code::Usage`] if
/// there is no terminal.
pub fn fill(mut answers: Answers) -> Result<Answers, CliError> {
    if answers.name.is_none() {
        let derived = answers
            .destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("app")
            .to_owned();
        let name = Text::new("Project name")
            .with_default(&derived)
            .with_help_message("ASCII letters, digits, `-`, and `_`; it becomes a package name")
            .prompt()
            .map_err(from_inquire)?;
        answers.name = Some(name);
    }

    if answers.local_domain.is_none() {
        let derived = format!("{}.test", answers.name.as_deref().unwrap_or("app"));
        let domain = Text::new("Local development domain")
            .with_default(&derived)
            .prompt()
            .map_err(from_inquire)?;
        answers.local_domain = Some(domain);
    }

    if !answers.example_domain {
        answers.example_domain = Confirm::new("Generate the example domain module?")
            .with_default(true)
            .prompt()
            .map_err(from_inquire)?;
    }

    if answers.example_domain && !answers.seed_data {
        // Only offered when it can be honoured. Offering seed data without the example domain
        // would present a choice whose only outcome is a validation failure.
        answers.seed_data = Confirm::new("Generate seed data for it?")
            .with_default(false)
            .prompt()
            .map_err(from_inquire)?;
    }

    if !answers.container {
        answers.container = Confirm::new("Generate container development controls?")
            .with_default(false)
            .prompt()
            .map_err(from_inquire)?;
    }

    if !answers.local_https {
        answers.local_https = Confirm::new(
            "Record that local HTTPS is wanted? (nothing is issued and no trust store is touched)",
        )
        .with_default(false)
        .prompt()
        .map_err(from_inquire)?;
    }

    Ok(answers)
}

/// Presents the review screen and asks for confirmation (FR-009).
///
/// # Why this runs after rendering rather than before it
///
/// FR-009 requires the screen to list **the paths that will be created**. The only way to know
/// those is to render — a list predicted from the template catalogue would drift the first time a
/// template gained a conditional, and would then be a screen that confidently describes a project
/// other than the one about to appear.
///
/// So the transaction renders and verifies into staging first, shows this, and only then performs
/// the rename. Declining costs a few seconds of work and leaves the destination untouched, which
/// is the right trade against showing an operator a list that might be wrong.
///
/// # Declining is not a failure of the answers
///
/// The equivalent command is printed **before** the question, so it is on screen whether the
/// operator says yes or no. US1 acceptance scenario 3 requires that declining not lose the
/// answers, and printing it only on the success path would lose exactly the case the requirement
/// is about.
///
/// # Errors
///
/// [`Code::Cancelled`] (exit `4`) if the operator declines or interrupts.
pub fn review(
    reporter: &crate::output::Reporter,
    configuration: &super::model::ProjectConfiguration,
    manifest: &FileManifest,
    warnings: &[String],
) -> Result<(), CliError> {
    reporter.note("");
    reporter.note(&format!("  project      {}", configuration.name()));
    reporter.note(&format!(
        "  destination  {}",
        configuration.destination_display()
    ));
    reporter.note(&format!("  local domain {}", configuration.local_domain()));
    reporter.note(&format!(
        "  files        {} ({} bytes)",
        manifest.file_count(),
        manifest.total_bytes()
    ));
    for path in manifest.paths() {
        reporter.note(&format!("    {path}"));
    }
    for warning in warnings {
        reporter.note(&format!("  warning: {warning}"));
    }
    reporter.note("");
    reporter.note(&format!(
        "  equivalent command: {}",
        configuration.equivalent_command()
    ));
    reporter.note("");

    let confirmed = Confirm::new("Create this project?")
        .with_default(true)
        .prompt()
        .map_err(from_inquire)?;

    if !confirmed {
        return Err(CliError::new(
            Code::Cancelled,
            "declined; nothing was written and the destination is unchanged. The equivalent \
             command above reproduces these answers without the prompts",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_and_interruption_both_exit_four() {
        // FR-011 and C-1. Ctrl-C and ESC are different events and the same outcome: the operator
        // said no. Distinguishing them in the exit code would make scripts handle two cases for one
        // meaning.
        for error in [
            InquireError::OperationCanceled,
            InquireError::OperationInterrupted,
        ] {
            let mapped = from_inquire(error);
            assert_eq!(mapped.code, Code::Cancelled);
            assert_eq!(mapped.exit(), crate::exit::Exit::Cancelled);
        }
    }

    #[test]
    fn a_missing_terminal_is_a_usage_error_and_not_a_cancellation() {
        // The distinction research D2 selected this crate for. `NotTTY` means the invocation was
        // wrong, not that anybody declined — and the message must say what to do instead.
        let mapped = from_inquire(InquireError::NotTTY);
        assert_eq!(mapped.code, Code::Usage);
        assert_eq!(mapped.exit(), crate::exit::Exit::Usage);
        assert!(mapped.message.contains("flags"), "{}", mapped.message);
    }

    #[test]
    fn an_unexpected_prompt_failure_is_a_defect_not_a_cancellation() {
        // Exit 1 is reserved for defects. Folding an unknown prompt failure into `cancelled` would
        // hide a bug behind an outcome that looks deliberate.
        let mapped = from_inquire(InquireError::Custom("boom".into()));
        assert_eq!(mapped.code, Code::Internal);
        assert_eq!(mapped.exit(), crate::exit::Exit::Unclassified);
    }
}
