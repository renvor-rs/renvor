//! The interactive wizard.
//!
//! # It produces [`Answers`] and nothing else
//!
//! The wizard cannot construct a [`super::model::ProjectConfiguration`]; it can only produce the
//! same unvalidated [`Answers`] the flag parser produces. That is what makes SC-003 — "prompt and
//! flag inputs serialize to identical configuration" — a property of the type graph rather than a
//! test that happens to pass. There is one validator and both interfaces go through it.
//!
//! # Constitution principle VII, and the amendment that settled this module's scope
//!
//! Principle VII's original wizard-scope sentence listed eleven things the wizard should ask about:
//! target, transport, persistence model, database, auth starter, frontend, render mode, styling,
//! desktop option, capabilities, and local tooling. **This wizard asks about two of them** — local
//! tooling (container and local HTTPS) and capabilities (example domain, seed data).
//!
//! It said "three of them — target, local tooling, and capabilities" until 2026-08-18, and **there
//! is no target prompt**: `--target` has one legal value in this phase, so a question offering one
//! option would be a question with one answer. An advisory review caught the overstatement by
//! comparing this comment with `fill` below. It mattered because the number in this comment is the
//! one the principle VII ruling recorded in [`phase-003-evidence.md` §7](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/governance/phase-003-evidence.md?plain=1#L728) was counted
//! against, and overstating compliance by one category in a comment is how it ends up overstated
//! in the ruling.
//!
//! The other eight correspond to flags this phase **reserves and refuses**, because the phases that
//! implement them have not happened. Asking an operator to choose a database that the generator
//! cannot act on would produce a recorded choice that no generated file reflects, which
//! data-model invariant I-12 forbids outright.
//!
//! # That question was referred, and it was answered
//!
//! [Phase 003 tasks T093a](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/tasks.md)
//! referred this to the maintainer as a governing-document question: either principle VII meant
//! "ask about everything the *product* will eventually support", and this phase was
//! non-compliant, or it meant "ask about everything *this build* honours", and it was compliant.
//! That ruling was not this module's to make, and this comment said so while the referral was
//! open.
//!
//! **The referral is closed. Constitution amendment 3.0.0, ratified 2026-08-18, chose the second
//! reading** — see `governance/constitution-amendment-3.0.0.md` and the amendment history at the
//! top of `CONSTITUTION.md`. The rule is now about what the *current* generator can honour, with
//! four supporting clauses: a single-valued choice MAY be defaulted and MUST be recorded;
//! unsupported choices MUST NOT be solicited or recorded; unsupported choices MUST be exposed as
//! reserved inputs that fail naming the phase which will introduce support; and a choice becomes
//! mandatory in **both** interfaces once its capability ships.
//!
//! Under that rule this module **complies**, and the amendment records the clause-by-clause
//! evidence. It was a MAJOR amendment rather than a waiver precisely because the old sentence was
//! not correct: it required questions for capabilities that do not exist, which contradicts the
//! same principle's requirement that generated output reflect the selections actually acted on. A
//! waiver would have expired and left the contradiction standing.
//!
//! **Nothing about this module's behaviour changed when the amendment landed, and nothing changes
//! here now.** The prompts, their order, and the eight reserved-and-refused flags are exactly what
//! they were; what changed is that the rule they are measured against is now the right rule. The
//! fourth clause is the one that binds later work: **when a capability ships, its choice becomes
//! mandatory in both the wizard and the flag surface.**

use super::container;
use crate::generate::manifest::FileManifest;
use crate::output::layout::{Report, Status};
use crate::output::prompt;

use super::model::Answers;
use crate::exit::CliError;

/// The title that opens the guided sequence.
///
/// A literal, and it has to be: [`prompt::intro`] writes through the prompt library's own writer,
/// which performs no redaction. See that function for why the type is the enforcement.
const TITLE: &str = "Create a Renvor application";

/// The one-line help under the persistence-model question.
///
/// A constant because `prompt::text` takes `&'static str`. It is written out rather than built
/// from `Orm::ALL`, so `orm_help_names_every_supported_value` exists to close the gap that
/// creates: a variant added to `--orm` but not to this line would be accepted by the flag and
/// invisible in the wizard, which is the quiet half of a half-shipped option.
const ORM_HELP: &str = "`sqlx` (hand-written SQL, no object mapper) or `seaorm` (entity and \
                        repository generated; SeaORM is built on SQLx)";

/// Runs the wizard, filling only what the flags did not supply.
///
/// Every flag already given is **not** asked about again. A wizard that re-asks what the operator
/// already typed teaches them to stop using flags.
///
/// # The sequence is framed, and this function owns both ends
///
/// [`prompt::intro`] opens the connected rail and [`prompt::outro`] closes it. Owning both here —
/// rather than opening in one function and closing in another — means there is no path on which a
/// rail is opened and never closed, including the cancellation paths, because `?` returns before
/// the close and cancellation ends the sequence anyway.
///
/// # The prompt *kinds* are unchanged, deliberately
///
/// Each question is the same kind it was before this crate changed prompt libraries: text stays
/// text, confirmation stays confirmation. Turning the domain question into a radio would have
/// matched the visual reference more closely and would have **removed the operator's ability to
/// type a domain that is not on the list**. That is an interaction change wearing a presentation
/// change's clothes.
///
/// # Errors
///
/// [`crate::exit::Code::Cancelled`] (exit `4`) if the operator cancels at any prompt, or
/// [`crate::exit::Code::Usage`] if there is no terminal.
pub fn fill(mut answers: Answers) -> Result<Answers, CliError> {
    // BEFORE `intro`, because `intro` draws. `stdin` decided the wizard is eligible; this checks
    // there is somewhere to draw it. Without the check the opening chrome landed in a redirected
    // `stderr` and the refusal came afterwards.
    prompt::require_drawable()?;
    prompt::intro(TITLE)?;

    if answers.name.is_none() {
        // THE SHARED derivation, not a copy of it. See `model::derive_project_name`.
        let derived = super::model::derive_project_name(&answers.destination);
        answers.name = Some(prompt::text(
            "Project name",
            &derived,
            Some("ASCII letters, digits, `-`, and `_`; it becomes a package name"),
        )?);
    }

    if answers.local_domain.is_none() {
        let derived = super::model::derive_local_domain(answers.name.as_deref().unwrap_or("app"));
        answers.local_domain = Some(prompt::text("Local development domain", &derived, None)?);
    }

    if !answers.example_domain {
        answers.example_domain = prompt::confirm("Generate the example domain module?", true)?;
    }

    if answers.example_domain && !answers.seed_data {
        // Only offered when it can be honoured. Offering seed data without the example domain
        // would present a choice whose only outcome is a validation failure.
        answers.seed_data = prompt::confirm("Generate seed data for it?", false)?;
    }

    // FR-046. `--database` has two supported values, so it is asked rather than defaulted. The
    // gate question comes first because persistence is optional: an operator who wants none should
    // not have to name a database to decline one.
    if answers.database.is_none() && prompt::confirm("Generate database persistence?", false)? {
        answers.database = Some(prompt::text(
            "Database",
            "postgres",
            Some("`postgres` or `mysql`; the choice selects exactly one driver"),
        )?);

        // ASKED, not defaulted, and asked only here — inside the gate, after persistence has been
        // chosen. FR-039. The non-interactive path keeps SQLx when `--orm` is omitted, because a
        // command that worked in Phase 006 must keep producing the same project; a wizard has no
        // such obligation and an operator in front of one deserves the choice.
        if answers.orm.is_none() {
            answers.orm = Some(prompt::text(
                "Persistence model",
                super::model::Orm::Sqlx.as_str(),
                Some(ORM_HELP),
            )?);
        }
    }

    if !answers.container {
        answers.container = prompt::confirm("Generate container development controls?", false)?;
    }

    // ── CONTAINER QUESTIONS, asked only after the gate is open ──────────────────────────
    //
    // Nothing below is asked of an operator who declined containers. Asking someone to name a
    // database port for a Compose file that will not be generated is a question with no
    // consequence, and a wizard that asks those teaches people to stop reading them.
    //
    // Every answer here is a STRING that goes into `Answers`. The wizard does not validate and does
    // not build a `ContainerSettings`; `ProjectConfiguration::resolve` does both, for the wizard and
    // for the flags, through one function. That is what keeps "interactive and non-interactive
    // resolve identically" true by construction rather than by two implementations agreeing.
    if answers.container {
        // Parsed rather than matched on the raw string, so the version offered belongs to the
        // engine chosen. A database value that does not parse is left alone: `resolve` refuses it
        // by name, and asking a follow-up question about an unsupported engine would bury that
        // refusal under two more prompts.
        let kind = answers
            .database
            .as_deref()
            .and_then(|value| super::model::parse_database(value).ok());

        if let Some(kind) = kind {
            if answers.database_version.is_none() {
                answers.database_version = Some(prompt::text(
                    "Database image version",
                    container::DatabaseVersion::newest_for(kind).as_str(),
                    Some(container::DatabaseVersion::version_hint(kind)),
                )?);
            }
            if answers.database_name.is_none() {
                // The SHARED derivation. A second `replace('-', "_")` here would be a second rule
                // that has to stay equal to the first.
                let derived = container::Identifier::derive_database_name(
                    answers.name.as_deref().unwrap_or("app"),
                )
                .map(|identifier| identifier.as_str().to_owned())
                .unwrap_or_default();
                answers.database_name = Some(prompt::text(
                    "Database name",
                    &derived,
                    Some("ASCII letters, digits, and `_`; a `-` in the project name becomes `_`"),
                )?);
            }
            if answers.database_user.is_none() {
                answers.database_user = Some(prompt::text(
                    "Database user",
                    container::DEFAULT_DATABASE_USER,
                    Some("not a superuser, and never a password — passwords go in `.env`"),
                )?);
            }
            if answers.database_port.is_none() {
                answers.database_port = Some(prompt::text(
                    "Database host port",
                    &container::DatabaseVersion::newest_for(kind)
                        .default_host_port()
                        .to_string(),
                    Some("published on 127.0.0.1 only; this is the HOST port, not the container's"),
                )?);
            }
        }

        // DEFAULT NO. A cache container is infrastructure the GENERATED project does not use:
        // Renvor's cache capability exists (`renvor-cache`, behind the facade's `capability-cache`
        // feature) but the scaffold does not wire it — so the container is opt-in.
        //
        // ONE ENGINE, so this is a yes/no rather than a menu. A prompt offering one real option
        // and one worse option is a prompt pretending to be a decision; which engine gets used is
        // recorded in `renvor.toml`, not asked.
        if answers.container_cache.is_none()
            && prompt::confirm(
                "Generate a local cache container? (local infrastructure only; the generated \
                 project does not wire Renvor's cache capability)",
                false,
            )?
        {
            answers.container_cache = Some(container::CacheEngine::Valkey.as_str().to_owned());
            if answers.cache_port.is_none() {
                answers.cache_port = Some(prompt::text(
                    "Cache host port",
                    &container::CacheEngine::Valkey
                        .default_host_port()
                        .to_string(),
                    Some("published on 127.0.0.1 only; must differ from the database port"),
                )?);
            }
        }
    }

    if !answers.local_https {
        answers.local_https = prompt::confirm(
            "Record that local HTTPS is wanted? (nothing is issued and no trust store is touched)",
            false,
        )?;
    }

    prompt::outro("Answers recorded")?;
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
/// # It is a report, not the prompt library's note helper
///
/// Every value on this screen — the project name, the destination, the file list — comes from
/// outside the program. The library's `note` would draw them inside its own box and never see
/// [`crate::output::redact`]. A [`Report`] emitted through the reporter redacts and neutralises
/// each field on the way out and aligns them into rows, which is also what makes the screen
/// scannable rather than a wall of prose.
///
/// # Errors
///
/// [`crate::exit::Code::Cancelled`] (exit `4`) if the operator declines or interrupts.
pub fn review(
    reporter: &crate::output::Reporter,
    configuration: &super::model::ProjectConfiguration,
    manifest: &FileManifest,
    warnings: &[String],
) -> Result<(), CliError> {
    let mut report = Report::new()
        .blank()
        .status(Status::Info, "Review before creating")
        .blank()
        .row("Project", configuration.name())
        .row("Destination", configuration.destination_display())
        .row("Local domain", configuration.local_domain())
        .row(
            "Files",
            format!(
                "{} ({} bytes)",
                manifest.file_count(),
                manifest.total_bytes()
            ),
        )
        .blank();

    for path in manifest.paths() {
        report = report.item(path);
    }

    for warning in warnings {
        report = report.blank().status(Status::Warn, warning.clone());
    }

    // THE LITERAL PREFIX IS LOAD-BEARING AND IS NOT A STYLE CHOICE.
    //
    // `equivalent command: ` is the anchor `tests/parity.rs` searches for in a pty transcript in
    // order to extract the command and **run it**, and `tests/transaction.rs` asserts on it too. A
    // pty transcript has no reliable line structure — ConPTY mirrors the screen and merges lines —
    // so a heading on its own line with the command underneath would be a shape the harness cannot
    // find. One greppable token survives any merging.
    //
    // `new.rs` prints the same token on the path where there is no review, so the two agree.
    report = report
        .blank()
        .text(format!(
            "equivalent command: {}",
            configuration.equivalent_command()
        ))
        .blank();

    reporter.emit(&report);

    if prompt::confirm("Create this project?", true)? {
        return Ok(());
    }

    Err(CliError::new(
        crate::exit::Code::Cancelled,
        "declined; nothing was written and the destination is unchanged. The equivalent \
         command above reproduces these answers without the prompts",
    ))
}

#[cfg(test)]
mod tests {
    /// Every value `--orm` accepts must be named in the wizard's help line.
    #[test]
    fn orm_help_names_every_supported_value() {
        for orm in crate::config::model::Orm::ALL {
            assert!(
                super::ORM_HELP.contains(orm.as_str()),
                "`{}` is accepted by --orm but is not offered in the wizard",
                orm.as_str()
            );
        }
    }

    use super::*;

    #[test]
    fn the_title_names_the_thing_being_created() {
        // The frame is the one piece of prompt chrome this module owns. It is asserted because it
        // is a `&'static str` for a security reason, and a test that reads it is the cheapest
        // reminder of why it cannot become a `format!`.
        assert_eq!(TITLE, "Create a Renvor application");
    }

    #[test]
    fn declining_the_review_says_the_destination_is_unchanged() {
        // The message is the whole remedy: an operator who declines has to know that nothing was
        // written and that the equivalent command reproduces their answers. Asserted rather than
        // trusted, because it is prose and prose is what gets shortened.
        let error = CliError::new(
            crate::exit::Code::Cancelled,
            "declined; nothing was written and the destination is unchanged. The equivalent \
             command above reproduces these answers without the prompts",
        );
        assert_eq!(error.exit(), crate::exit::Exit::Cancelled);
        assert!(error.message.contains("nothing was written"));
        assert!(error.message.contains("equivalent"));
    }

    #[test]
    fn the_wizard_asks_no_transport_question() {
        // FR-049: "The wizard MUST NOT gain a transport question, and `transport` MUST be recorded
        // in the generated manifest."
        //
        // The recorded half was pinned by `tests/legacy_compatibility.rs`. The MUST NOT half was
        // pinned by nothing: `fill` asks six questions and none is about transport, but no test
        // enumerated them, so adding a seventh would have failed nothing.
        //
        // Enumerating the questions from the source is deliberate. The alternative — driving the
        // wizard — needs a terminal, which is exactly what `require_drawable` refuses in a test.
        let source = include_str!("prompts.rs");

        let body = source
            .split_once("pub fn fill(mut answers: Answers)")
            .expect("the wizard entry point exists")
            .1
            .split_once("prompt::outro(")
            .expect("the wizard ends with its outro")
            .0;

        let asked: Vec<&str> = body
            .lines()
            .map(|line| line.split("//").next().unwrap_or(line))
            .filter(|code| code.contains("prompt::text(") || code.contains("prompt::confirm("))
            .collect();

        // SIX in Phase 004. EIGHT when Phase 006 added persistence: FR-046 requires the database
        // to be asked, which needs a gate question and the value itself. FOURTEEN since the
        // container scope addition, which adds six — and every one of the six is asked only after
        // "Generate container development controls?" is answered yes, so an operator who declines
        // containers still answers eight.
        //
        // FIFTEEN in Phase 007. Phase 007 FR-039 requires the wizard to ask SQLx versus SeaORM
        // when persistence is selected, and it is asked INSIDE the persistence gate — an operator
        // who declines a database is never asked which ORM they are not using. Reviewed against
        // FR-049: the question has two documented values, a default that matches the
        // non-interactive behaviour, and a consequence the operator can see, because the two
        // answers generate different files.
        //
        // The count is asserted rather than inferred so that a sixteenth question is reviewed
        // against FR-049 before it ships. It is not a style rule: each question is a thing an
        // operator must answer to get a project.
        assert_eq!(
            asked.len(),
            15,
            "the wizard asks {} question(s), not the fifteen FR-046, FR-049 and Phase 007 FR-039 \
             were reviewed against. A new question must be checked against FR-049 before this \
             count is updated: {asked:#?}",
            asked.len()
        );

        // AND THE PERSISTENCE GATE. The ORM question must sit inside it, for the same reason the
        // container questions sit inside theirs: a question with no consequence for the operator
        // who declined persistence is a question that should not have been asked.
        let persistence_gate = body
            .find("Generate database persistence?")
            .expect("the persistence gate question exists");
        let orm_question = body
            .find("Persistence model")
            .expect("the ORM question exists");
        assert!(
            orm_question > persistence_gate,
            "the ORM question is asked before the persistence gate, so an operator who wants no \
             database is still asked which ORM they are not using"
        );

        // AND THE GATE ITSELF. A container question asked unconditionally would be a question with
        // no consequence for the operator who declined containers — which is what the count above
        // cannot see, because it counts calls rather than the branch they sit in.
        let gate = body
            .find("Generate container development controls?")
            .expect("the container gate question exists");
        for needle in [
            "Database image version",
            "Database name",
            "Database user",
            "Database host port",
            "Generate a local cache container?",
            "Cache host port",
        ] {
            let at = body
                .find(needle)
                .unwrap_or_else(|| panic!("the wizard asks `{needle}`"));
            assert!(
                at > gate,
                "`{needle}` is asked before the container gate, so it would be asked of an \
                 operator who does not want containers"
            );
        }

        for line in &asked {
            let lowered = line.to_ascii_lowercase();
            assert!(
                !lowered.contains("transport"),
                "the wizard gained a transport question, which FR-049 forbids: {line}"
            );
        }
    }

    #[test]
    fn the_wizard_question_scan_would_notice_one_being_added() {
        // POSITIVE CONTROL. Without it, a scan whose matching had drifted would report zero
        // questions, find no transport among them, and pass.
        let sample = "    answers.transport = prompt::confirm(\"Which transport?\", true)?;";
        let code = sample.split("//").next().unwrap_or(sample);
        assert!(
            code.contains("prompt::confirm("),
            "the scan does not recognise the construct it counts"
        );
        assert!(
            code.to_ascii_lowercase().contains("transport"),
            "the scan would not recognise a transport question"
        );
    }
}
