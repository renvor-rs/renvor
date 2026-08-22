//! The flag surface (contract C-1).
//!
//! # The reserved flags are the interesting part
//!
//! `--transport`, `--orm`, `--database`, `--auth`, `--frontend`, `--styling`, `--render-mode`, and
//! `--desktop` are **declared here and rejected in validation**, with exit `3` and a message naming
//! the phase that will support them.
//!
//! Two alternatives were available and both are worse. Omitting them makes clap report *"unexpected
//! argument"*, which tells an operator their command is wrong without telling them it will be right
//! later. Accepting and ignoring them means a Phase 003 command line silently changes meaning when
//! a later phase implements the flag — the same script, the same output, a different project.
//!
//! # Why the derive API
//!
//! The flag surface **is** the contract, and one declaration keeps `--help`, validation, and future
//! completions from drifting apart. Research D1.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use super::model::Answers;
use crate::exit::{CliError, Code};
use crate::output::style::Role;

/// How `--help` is coloured.
///
/// # The renderer stays clap's; only the palette is ours
///
/// Contract C-8 requires the *content* of `--help` to be clap's own generated usage, options, and
/// descriptions. Replacing the renderer with handwritten strings is the tempting shortcut and it is
/// how a documented surface drifts from the parsed one: the flag surface **is** the contract, and
/// one declaration is what keeps help, validation, and future completions agreeing.
///
/// So this styles what clap draws and changes nothing about what clap says.
///
/// The colours come from [`Role`] rather than from `anstyle` literals, so `--help` shares one
/// palette with every other line the program prints — a heading here is the same heading as a
/// heading in `renvor doctor`.
///
/// # Colour here is decoration and clap already guarantees that
///
/// clap's own help remains fully structured without it: the section headings are still on their own
/// lines and the flags still start their own. Nothing in `--help` is distinguished by colour alone.
///
/// # This is applied unconditionally, and clap resolves it
///
/// `Command::styles` describes the palette; clap's `color` feature decides whether to emit it,
/// suppressing it when the stream is not a terminal or `NO_COLOR` is set. The two remaining
/// policies — `--no-color` and `TERM=dumb` — are enforced where `--help` is actually written, in
/// `main`, which strips through the same `AutoStream` boundary every other line goes through.
const HELP_STYLES: clap::builder::Styles = clap::builder::Styles::styled()
    .header(Role::Heading.style())
    .usage(Role::Heading.style())
    .literal(Role::Accent.style())
    .placeholder(Role::Muted.style())
    .valid(Role::Success.style())
    .invalid(Role::Error.style())
    .error(Role::Error.style());

/// The `renvor` command line.
#[derive(Debug, Parser)]
#[command(
    name = "renvor",
    about = "Create and run Renvor projects.",
    long_about = "Create and run Renvor projects.\n\nExit codes: 0 success, 1 internal defect, \
                  2 usage, 3 validation, 4 cancelled, 5 environment.",
    version,
    disable_help_subcommand = true,
    styles = HELP_STYLES
)]
pub struct Cli {
    /// Result format for stdout.
    #[arg(
        long,
        global = true,
        default_value = "human",
        value_name = "human|json"
    )]
    pub output: String,

    /// Accept confirmations. Never waives validation.
    #[arg(long, global = true)]
    pub yes: bool,

    /// Compute and report without writing anything.
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Disable styling. Styling is also off automatically when the stream is not a terminal.
    #[arg(long, global = true)]
    pub no_color: bool,

    /// The command to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The commands this phase implements. Nothing is stubbed.
#[derive(Debug, Subcommand)]
pub enum Command {
    // `NewArgs` is boxed because it is 272 bytes and every other variant is a handful; an unboxed
    // variant would make every `Command` value pay for the largest one, including `Doctor`, which
    // carries nothing.
    //
    // THIS IS A `//` COMMENT ON PURPOSE. It was a `///` doc comment, and clap publishes a
    // subcommand's doc comment as its `--help` description — so `renvor new --help` opened with a
    // paragraph about Rust enum memory layout, and `tests/cmd/help-new.trycmd` had frozen that as
    // the public contract. FR-002 makes the help text a public contract; an internal note is not a
    // description of what the command does.
    /// Create a project, from prompts or from flags.
    ///
    /// With a terminal, `renvor new` asks only the questions this phase can honour and shows a
    /// review screen before writing anything. Without one, it takes the same answers as flags.
    /// Generation is transactional: a failure before placement leaves the destination untouched.
    New(Box<NewArgs>),
    /// Report environment readiness.
    Doctor,
    /// Validate a project without building it.
    Check {
        /// The project directory. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show the routes this project would serve.
    ///
    /// Asks the application binary for its own route registry — the same value that builds the
    /// router — rather than parsing source or reading a second manifest. A project that declares
    /// no Renvor dependency has no registry to report, and that is a failure with a name rather
    /// than an empty table.
    Routes {
        /// The project directory. Defaults to the current directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Run the local development loop.
    Dev,
    /// Container development controls.
    Docker {
        /// The action to perform.
        #[command(subcommand)]
        action: DockerAction,
    },
    /// Local TLS trust. Issues no certificate and modifies no trust store in this phase.
    Tls {
        /// The action to perform.
        #[command(subcommand)]
        action: TlsAction,
    },
}

/// `renvor tls <action>`.
///
/// One action, deliberately. A `tls status` or `tls untrust` would have to report on, or undo,
/// something this phase never creates.
#[derive(Debug, Subcommand)]
pub enum TlsAction {
    /// Install a local certificate authority into the system trust store.
    ///
    /// Describes exactly what would change, requires explicit consent, and then declines: the
    /// operation is unavailable until a transport exists (FR-036, FR-037).
    Trust {
        /// Consent to modifying this machine's trust store, for a run with no terminal.
        ///
        /// **`--yes` does not grant this and is not intended to.**
        #[arg(long = "i-understand-this-modifies-my-system-trust-store")]
        consent: bool,
    },
}

/// `renvor docker <action>`.
#[derive(Debug, Subcommand)]
pub enum DockerAction {
    /// Start the development containers.
    Up,
    /// Stop them.
    Down,
    /// Report their state.
    Status,
    /// Show their logs.
    Logs,
}

/// `renvor new`.
#[derive(Debug, Args)]
pub struct NewArgs {
    /// Project name. Defaults to the destination's final component.
    pub name: Option<String>,

    /// Where to create the project. Defaults to `./<name>`.
    #[arg(long)]
    pub path: Option<PathBuf>,

    /// What to generate. `api` is the only supported value in this phase.
    #[arg(long, default_value = "api")]
    pub target: String,

    /// Local development domain. Defaults to `<name>.test`.
    #[arg(long)]
    pub local_domain: Option<String>,

    /// Generate container development controls.
    #[arg(long)]
    pub container: bool,

    /// Record that local HTTPS is wanted. **Issues nothing and modifies no trust store.**
    #[arg(long)]
    pub local_https: bool,

    /// Generate the example domain module.
    #[arg(long)]
    pub example_domain: bool,

    /// Generate seed data. Requires `--example-domain`.
    #[arg(long)]
    pub seed_data: bool,

    // VISIBLE, unlike the reserved flags below. The transport capability has shipped, so this is
    // a real choice an operator may state rather than a placeholder that will be refused. The
    // rationale lives here rather than in the doc comment, because a doc comment becomes `--help`
    // output and `--help` is not the place for a phase history.
    /// Delivery transport. `rest` is the only supported value
    #[arg(long)]
    pub transport: Option<String>,

    // ── RESERVED. Parsed, then refused with exit 3. See the module header. ──────────────
    /// Reserved for a later phase.
    #[arg(long, hide = true)]
    pub orm: Option<String>,
    /// Reserved for a later phase.
    #[arg(long, hide = true)]
    pub database: Option<String>,
    /// Reserved for a later phase.
    #[arg(long, hide = true)]
    pub auth: Option<String>,
    /// Reserved for a later phase.
    #[arg(long, hide = true)]
    pub frontend: Option<String>,
    /// Reserved for a later phase.
    #[arg(long, hide = true)]
    pub styling: Option<String>,
    /// Reserved for a later phase.
    #[arg(long, hide = true)]
    pub render_mode: Option<String>,
    /// Reserved for a later phase.
    #[arg(long, hide = true)]
    pub desktop: bool,
}

/// Every reserved flag, with the phase that will support it.
///
/// A table rather than a chain of `if let`s, so that adding a flag to [`NewArgs`] and forgetting to
/// reject it is visible as a missing row rather than invisible as a missing branch. The test below
/// asserts the table covers the struct.
const RESERVED: [(&str, &str); 7] = [
    ("--orm", "Phase 009 (persistence)"),
    ("--database", "Phase 009 (persistence)"),
    ("--auth", "Phase 013 (authentication)"),
    ("--frontend", "Phase 019 (full-stack architecture)"),
    ("--styling", "Phase 019 (full-stack architecture)"),
    ("--render-mode", "Phase 019 (full-stack architecture)"),
    ("--desktop", "Phase 024 (desktop)"),
];

impl NewArgs {
    /// Converts parsed flags into the one unvalidated shape both interfaces produce.
    ///
    /// **Rejects reserved flags first**, before anything else is examined, so an operator who
    /// passed one gets told about it rather than being told about a different problem they would
    /// hit afterwards anyway.
    ///
    /// # Errors
    ///
    /// [`Code::ReservedForLaterPhase`] with `details.flag` and `details.phase`, or
    /// [`Code::Usage`] when neither a name nor a path was supplied.
    pub fn into_answers(self) -> Result<Answers, CliError> {
        let supplied: [(&str, bool); 7] = [
            ("--orm", self.orm.is_some()),
            ("--database", self.database.is_some()),
            ("--auth", self.auth.is_some()),
            ("--frontend", self.frontend.is_some()),
            ("--styling", self.styling.is_some()),
            ("--render-mode", self.render_mode.is_some()),
            ("--desktop", self.desktop),
        ];
        for (flag, present) in supplied {
            if present {
                let phase = RESERVED
                    .iter()
                    .find(|(name, _)| *name == flag)
                    .map(|(_, phase)| *phase)
                    .unwrap_or("a later phase");
                return Err(CliError::new(
                    Code::ReservedForLaterPhase,
                    format!(
                        "`{flag}` is reserved for {phase}. It parses today so that this message \
                         can name the phase rather than reporting an unknown flag, and it is \
                         refused today so that the same command line cannot quietly mean something \
                         different later"
                    ),
                )
                .with("flag", flag)
                .with("phase", phase));
            }
        }

        let destination = match (self.path, self.name.as_deref()) {
            (Some(path), _) => path,
            (None, Some(name)) => PathBuf::from(name),
            (None, None) => {
                return Err(CliError::new(
                    Code::Usage,
                    "`renvor new` needs either a NAME or `--path`; with neither there is nowhere \
                     to create the project",
                )
                .with("missing", "NAME or --path"));
            }
        };

        Ok(Answers {
            name: self.name,
            destination,
            local_domain: self.local_domain,
            target: self.target,
            transport: self.transport,
            container: self.container,
            local_https: self.local_https,
            seed_data: self.seed_data,
            example_domain: self.example_domain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("parses")
    }

    #[test]
    fn the_declared_command_surface_is_valid() {
        // clap's own consistency check. Catches a duplicate long flag or a malformed default at
        // test time rather than at first run.
        Cli::command().debug_assert();
    }

    #[test]
    fn a_reserved_flag_parses_and_then_fails_validation() {
        // The whole point of C-1's reserved-flag rule: NOT an unknown-argument usage error.
        for (flag, value) in [
            // `--transport` is NO LONGER HERE. Phase 004 ships the transport capability, so
            // reporting "reserved for Phase 004" from inside Phase 004 would be a false statement.
            // Its replacement behaviour is asserted in the two tests below.
            ("--orm", Some("diesel")),
            ("--database", Some("postgres")),
            ("--auth", Some("session")),
            ("--frontend", Some("react")),
            ("--styling", Some("tailwind")),
            ("--render-mode", Some("ssr")),
            ("--desktop", None),
        ] {
            let mut argv = vec!["renvor", "new", "demo", flag];
            if let Some(value) = value {
                argv.push(value);
            }
            let cli = Cli::try_parse_from(&argv)
                .unwrap_or_else(|error| panic!("{flag} must PARSE, not error: {error}"));
            let Command::New(args) = cli.command else {
                panic!("expected new")
            };
            let error = args.into_answers().unwrap_err();
            assert_eq!(error.code, Code::ReservedForLaterPhase, "{flag}");
            assert!(error.details.iter().any(|(k, v)| k == "flag" && v == flag));
            assert!(
                error.details.iter().any(|(k, _)| k == "phase"),
                "{flag} must name the phase that will support it"
            );
        }
    }

    #[test]
    fn the_supported_transport_is_accepted_rather_than_reserved() {
        // Phase 004 ships it, so `--transport rest` is a real choice an operator may state.
        let cli =
            Cli::try_parse_from(["renvor", "new", "demo", "--transport", "rest"]).expect("parses");
        let Command::New(args) = cli.command else {
            panic!("expected new")
        };
        let answers = args
            .into_answers()
            .expect("`--transport rest` must not be refused");
        assert_eq!(answers.transport.as_deref(), Some("rest"));
    }

    #[test]
    fn an_unsupported_transport_is_an_unsupported_value_not_a_reservation() {
        // The distinction matters: `reserved_for_later_phase` promises support arrives later.
        // For a value no phase will support, that promise would be false.
        let cli =
            Cli::try_parse_from(["renvor", "new", "demo", "--transport", "grpc"]).expect("parses");
        let Command::New(args) = cli.command else {
            panic!("expected new")
        };
        // The flag parser lets it through; the configuration model refuses it, which is where the
        // supported-value set lives.
        let answers = args.into_answers().expect("the flag is not reserved");
        let error =
            crate::config::model::Transport::parse(answers.transport.as_deref().expect("supplied"))
                .expect_err("`grpc` must be refused");

        assert_eq!(error.code, Code::UnsupportedValue);
        assert_ne!(error.code, Code::ReservedForLaterPhase);
        assert!(
            error
                .details
                .iter()
                .any(|(k, v)| k == "supported" && v == "rest"),
            "the refusal must name the supported value"
        );
    }

    #[test]
    fn every_governed_choice_of_principle_seven_is_classified() {
        // ── CONSTITUTION v3.0.0, PRINCIPLE VII, AMENDED 2026-08-18 ─────────────────────
        //
        // > "The governed choice set is target, transport, persistence model, database, auth
        // > starter, frontend, compatible render mode, styling profile where applicable, desktop
        // > option, capabilities, and local tooling; each becomes mandatory in both interfaces on
        // > the day its capability ships, and none of them may be dropped from this set by an
        // > implementation that has not shipped it."
        //
        // The amendment's danger is precisely that last clause: "ask for what you can honour" is
        // trivially satisfiable by honouring nothing, and an implementation that quietly stopped
        // reserving `--database` would satisfy every other test in this file. So each of the
        // eleven is listed here and must be in exactly one of three states:
        //
        //   RESERVED  — declared, parsed, and refused with the phase that will support it
        //   HONOURED  — a real flag the generator acts on, which the wizard therefore asks about
        //   DEFAULTED — a single supported value, defaulted without prompting and RECORDED
        //
        // A choice in none of them has been dropped, which the constitution forbids.
        enum How {
            Reserved(&'static str),
            Honoured(&'static [&'static str]),
            Defaulted(&'static str),
        }
        use How::{Defaulted, Honoured, Reserved};

        let governed: [(&str, How); 11] = [
            ("target", Defaulted("--target")),
            // PHASE 004 MOVED THIS ROW.
            //
            // The transport capability has shipped, so `transport` is no longer a governed choice
            // this phase does not ship. It has exactly ONE supported value, which clause 2 of the
            // amended principle VII permits to be defaulted without prompting provided it is
            // RECORDED — and it is, in `renvor.toml`.
            //
            // `target` has held this classification since Phase 003 for the identical reason, and
            // amendment 3.0.0 §4 records that as COMPLYING rather than as an exception.
            ("transport", Defaulted("--transport")),
            ("persistence model", Reserved("--orm")),
            ("database", Reserved("--database")),
            ("auth starter", Reserved("--auth")),
            ("frontend", Reserved("--frontend")),
            ("compatible render mode", Reserved("--render-mode")),
            ("styling profile where applicable", Reserved("--styling")),
            ("desktop option", Reserved("--desktop")),
            (
                "capabilities",
                Honoured(&["--example-domain", "--seed-data", "--container"]),
            ),
            (
                "local tooling",
                Honoured(&["--local-domain", "--local-https"]),
            ),
        ];

        for (choice, how) in governed {
            match how {
                Reserved(flag) => {
                    assert!(
                        RESERVED.iter().any(|(reserved, _)| *reserved == flag),
                        "`{choice}` is a governed choice this phase does not ship, so `{flag}` \
                         must be a reserved input — dropping it from the reserved table drops the \
                         choice from the governed set, which the constitution forbids"
                    );
                    let (_, phase) = RESERVED
                        .iter()
                        .find(|(reserved, _)| *reserved == flag)
                        .expect("checked above");
                    assert!(
                        !phase.is_empty(),
                        "`{flag}` must fail explicitly WITH the phase that will introduce support"
                    );
                }
                Honoured(flags) => {
                    for flag in flags {
                        assert!(
                            !RESERVED.iter().any(|(reserved, _)| reserved == flag),
                            "`{flag}` cannot be both honoured and reserved"
                        );
                        let mut argv = vec!["renvor", "new", "demo", flag];
                        if *flag == "--local-domain" {
                            argv.push("demo.test");
                        }
                        let cli = Cli::try_parse_from(&argv)
                            .unwrap_or_else(|error| panic!("`{flag}` must parse: {error}"));
                        let Command::New(args) = cli.command else {
                            panic!("expected new")
                        };
                        // `--seed-data` alone is a documented cross-choice conflict, so the only
                        // thing asserted here is that the flag is NOT refused as belonging to a
                        // later phase. A honoured choice that answered `reserved_for_later_phase`
                        // would be a choice the wizard asks about and the generator will not act
                        // on, which is the exact failure the amendment forbids.
                        if let Err(error) = args.into_answers() {
                            assert_ne!(
                                error.code,
                                Code::ReservedForLaterPhase,
                                "`{flag}` is asked about but refused as a later phase's"
                            );
                        }
                    }
                }
                Defaulted(flag) => {
                    assert!(
                        !RESERVED.iter().any(|(reserved, _)| *reserved == flag),
                        "`{flag}` is defaulted, not reserved"
                    );
                    // Defaulted without prompting: parsing with no `--target` yields the value.
                    let cli = Cli::try_parse_from(["renvor", "new", "demo"]).expect("parses");
                    let Command::New(args) = cli.command else {
                        panic!("expected new")
                    };
                    assert_eq!(
                        args.target, "api",
                        "`{choice}` must be defaulted without prompting"
                    );
                    // And RECORDED. `renvor.toml` carries `target = "api"`; the manifest text is
                    // asserted by `commands::new::tests` and the round trip by
                    // `config::model::tests`. What matters here is that the single supported value
                    // is the one that gets defaulted — a default outside the supported set would
                    // record a choice the generator cannot honour.
                    assert!(
                        crate::config::model::Target::parse("api").is_ok(),
                        "the defaulted value must be a supported one"
                    );
                    assert!(
                        crate::config::model::Target::parse("nope").is_err(),
                        "if every value parsed, `{choice}` would not be single-valued and could \
                         not be defaulted without prompting"
                    );
                }
            }
        }
    }

    #[test]
    fn every_reserved_flag_in_the_table_is_declared_on_the_struct() {
        // Guards the pairing in both directions. A flag on the struct with no table row would fall
        // through to "a later phase" with no phase named; a row with no flag is dead text.
        for (flag, _) in RESERVED {
            let mut argv = vec!["renvor", "new", "demo", flag];
            if flag != "--desktop" {
                argv.push("x");
            }
            assert!(
                Cli::try_parse_from(&argv).is_ok(),
                "{flag} is in the reserved table but is not declared on NewArgs"
            );
        }
    }

    #[test]
    fn an_unknown_flag_is_still_a_usage_error() {
        // POSITIVE CONTROL for the reserved-flag behaviour. If everything parsed, the test above
        // would prove nothing.
        assert!(Cli::try_parse_from(["renvor", "new", "demo", "--nonsense"]).is_err());
    }

    #[test]
    fn a_name_alone_is_enough_and_becomes_the_destination() {
        let cli = parse(&["renvor", "new", "commerce"]);
        let Command::New(args) = cli.command else {
            panic!("expected new")
        };
        let answers = args.into_answers().expect("converts");
        assert_eq!(answers.destination, PathBuf::from("commerce"));
        assert_eq!(answers.name.as_deref(), Some("commerce"));
    }

    #[test]
    fn path_overrides_the_name_as_the_destination() {
        let cli = parse(&["renvor", "new", "commerce", "--path", "/tmp/elsewhere"]);
        let Command::New(args) = cli.command else {
            panic!("expected new")
        };
        let answers = args.into_answers().expect("converts");
        assert_eq!(answers.destination, PathBuf::from("/tmp/elsewhere"));
        assert_eq!(
            answers.name.as_deref(),
            Some("commerce"),
            "an explicit name must survive an explicit path"
        );
    }

    #[test]
    fn neither_a_name_nor_a_path_is_a_usage_error_not_a_default() {
        // FR-010. Substituting a default here would create a project somewhere the operator did
        // not name, which is the failure mode the requirement exists for.
        let cli = parse(&["renvor", "new"]);
        let Command::New(args) = cli.command else {
            panic!("expected new")
        };
        let error = args.into_answers().unwrap_err();
        assert_eq!(error.code, Code::Usage);
    }

    #[test]
    fn the_global_flags_are_global() {
        // `--output` after the subcommand must work; a contract that only accepts it before is a
        // contract every user gets wrong once.
        let cli = parse(&["renvor", "new", "demo", "--output", "json", "--dry-run"]);
        assert_eq!(cli.output, "json");
        assert!(cli.dry_run);
    }

    #[test]
    fn the_target_defaults_to_api() {
        let cli = parse(&["renvor", "new", "demo"]);
        let Command::New(args) = cli.command else {
            panic!("expected new")
        };
        assert_eq!(args.target, "api");
    }
}
