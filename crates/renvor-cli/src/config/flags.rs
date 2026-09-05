//! The flag surface (contract C-1).
//!
//! # The reserved flags are the interesting part
//!
//! `--frontend`, `--styling`, `--render-mode`, and `--desktop` are **declared here and rejected in
//! validation**, with exit `3` and a message naming the phase that will support them.
//! `--transport`, `--orm`, `--database`, and — since Phase 011 — `--auth` were reserved once and
//! are honoured now; `--capabilities` and `--framework-path` arrived honoured.
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
    /// Show the OpenAPI description this project would publish.
    ///
    /// Asks the application binary for its own declarations — the same values that validate
    /// requests at runtime — rather than parsing source or reading a second manifest. A project
    /// that declares no Renvor dependency has nothing to describe, and that is a failure with a
    /// name rather than an empty description.
    Openapi {
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
    /// Add to an existing project, rerun-safe: a file you changed is never overwritten
    Generate {
        #[command(subcommand)]
        action: GenerateAction,
    },
}

/// `renvor tls <action>`.
///
/// One action, deliberately. A `tls status` or `tls untrust` would have to report on, or undo,
/// something this phase never creates.
/// What `renvor generate` can add.
#[derive(Debug, Subcommand)]
pub enum GenerateAction {
    /// Write a reversible migration pair, or import the framework's `auth` or `jobs` set
    Migration {
        /// The migration's name: lowercase letters, digits, and `_`, starting with a letter
        #[arg(required_unless_present = "import", conflicts_with = "import")]
        name: Option<String>,
        /// Copy the framework's migration set for this project's engine: `auth` or `jobs`
        #[arg(long, value_name = "auth|jobs")]
        import: Option<String>,
        /// The project directory
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Add a resource to a starter: a module, a migration pair, five routes, and a test
    Resource {
        /// The resource's type name, in PascalCase (`Post`)
        name: String,
        /// Its columns, as `name:type` with type one of string, text, integer, boolean, float
        #[arg(value_name = "FIELD:TYPE")]
        fields: Vec<String>,
        /// The project directory
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Add the session authentication starter to a starter that has a database and `mail`
    Auth {
        /// The project directory
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
}

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

    // VISIBLE since Phase 006. Persistence has shipped, so these are real choices rather than
    // placeholders that will be refused. Selecting a database is what makes the generated project
    // carry persistence sources and migrations.
    /// Persistence layer: `sqlx` or `seaorm`. Omitting it with `--database` keeps `sqlx`
    #[arg(long)]
    pub orm: Option<String>,
    /// Database to generate for. `postgres` or `mysql`
    #[arg(long)]
    pub database: Option<String>,

    // ── CONTAINER DEVELOPMENT CONTROLS. Only meaningful with `--container`. ─────────────
    //
    // Every one is refused as an UNSUPPORTED COMBINATION when `--container` is absent, rather than
    // ignored. A flag that parses and does nothing is the worst of the three options: the operator
    // believes they configured something, and the generated tree does not reflect it.
    //
    // NONE OF THESE CAN CARRY A PASSWORD, and no flag that could will be added. A credential on a
    // command line lands in shell history, in `ps` output, and in the CI log of whatever ran it.
    /// Database image version: `17` or `18` for postgres, `8.4` or `9.7` for mysql
    #[arg(long, value_name = "VERSION")]
    pub database_version: Option<String>,
    /// Database name inside the container profile. Defaults to the project name with `-` as `_`.
    #[arg(long)]
    pub database_name: Option<String>,
    /// Database user inside the container profile. **Never a password.**
    #[arg(long)]
    pub database_user: Option<String>,
    // A STRING, NOT A `u16`, ON PURPOSE. clap would reject `70000` with its own message before any
    // renvor code ran, so the interactive and non-interactive paths would refuse the same value
    // with two different diagnoses — and only one of them would carry `details.flag`.
    /// Published host port for the container database. Bound to 127.0.0.1.
    #[arg(long, value_name = "1-65535")]
    pub database_port: Option<String>,
    /// Local cache container: `none` or `valkey`. Development infrastructure only.
    #[arg(long, value_name = "none|valkey")]
    pub container_cache: Option<String>,
    /// Published host port for the container cache. Bound to 127.0.0.1.
    #[arg(long, value_name = "1-65535")]
    pub cache_port: Option<String>,

    // ── PHASE 011: the auth starter and the capabilities (W-023, W-024), and the framework
    // source they need. VISIBLE, honoured, and validated by the one configuration model. `--auth`
    // was a reserved flag from Phase 003 to Phase 010; it left the reserved table the day the
    // generator could honour it, which is the rule constitution VII's fourth clause states.
    /// Authentication starter: `none` or `session`. `session` needs `--database` and the `mail` capability
    #[arg(long, value_name = "none|session")]
    pub auth: Option<String>,
    /// Capabilities to wire, comma-separated: `cache`, `jobs`, `mail`, `storage`, `observability`; or `none`
    #[arg(long, value_name = "LIST")]
    pub capabilities: Option<String>,
    /// Path to a Renvor framework checkout. Needed by `--auth session` and by any capability until a crate is published
    #[arg(long, value_name = "DIR")]
    pub framework_path: Option<PathBuf>,

    // ── RESERVED. Parsed, then refused with exit 3. See the module header. ──────────────
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
const RESERVED: [(&str, &str); 4] = [
    // `--orm` and `--database` left this table in Phase 006, which is when persistence shipped.
    //
    // `--auth` left it in Phase 011, which is when the generator could honour it. It had named
    // Phase 009 until Phase 009 corrected it and Phase 013 before Phase 006 checked it against the
    // roadmap; the drift test below now pins the OTHER direction — that it is honoured everywhere
    // it used to be reserved — so the flag cannot quietly become reserved again.
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
        let supplied: [(&str, bool); 4] = [
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
            orm: self.orm,
            database: self.database,
            database_version: self.database_version,
            database_name: self.database_name,
            database_user: self.database_user,
            database_port: self.database_port,
            container_cache: self.container_cache,
            cache_port: self.cache_port,
            auth: self.auth,
            capabilities: self.capabilities,
            framework_path: self.framework_path,
        })
    }
}

#[cfg(test)]
mod tests {
    /// Phase 011 — `--auth` is HONOURED in every place that used to state its reservation.
    ///
    /// # The same drift test, pointed the other way
    ///
    /// Through Phases 006–010 this test pinned that the reserved `--auth` named Phase 011 in three
    /// places: the table here, a JSON fixture, and the published contract. Phase 011 is the phase
    /// that honours the flag, so the three statements changed together — and this now pins that
    /// none of them can drift BACK. A reserved-flag paragraph that still listed `--auth` would tell
    /// an operator a working flag is unsupported.
    ///
    /// The JSON fixture that pinned the reserved shape now does so with `--frontend`, which is
    /// still reserved (Phase 019), so the `reserved_for_later_phase` document shape stays covered.
    #[test]
    fn the_auth_starter_is_honoured_everywhere_it_used_to_be_reserved() {
        assert!(
            !RESERVED.iter().any(|(flag, _)| *flag == "--auth"),
            "`--auth` is back in the reserved table; Phase 011 honours it"
        );

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");

        // SITE 2 — the JSON fixture for the reserved shape now uses a flag that IS reserved.
        assert!(
            !root
                .join("crates/renvor-cli/tests/json/reserved-auth.json")
                .exists(),
            "the reserved-auth fixture still exists; it would pin a refusal the CLI no longer makes"
        );
        let fixture = std::fs::read_to_string(
            root.join("crates/renvor-cli/tests/json/reserved-frontend.json"),
        )
        .expect("the reserved-frontend fixture is readable");
        assert!(
            fixture.contains("Phase 019 (full-stack architecture)"),
            "the reserved-frontend fixture does not name Phase 019"
        );
        assert!(
            !fixture.contains("--auth"),
            "the reserved fixture still names `--auth`"
        );

        // SITE 3 — the published contract's reserved-flag paragraph.
        let contract = std::fs::read_to_string(root.join("contracts/command-surface.md"))
            .expect("the command-surface contract is readable");
        let reserved_section = contract
            .split("## Reserved flags")
            .nth(1)
            .expect("the contract has a reserved-flags section")
            .split("\n## ")
            .next()
            .expect("the section has a body");
        assert!(
            !reserved_section.contains("`--auth`"),
            "the contract's reserved-flag section still lists `--auth`"
        );
        assert!(
            contract.contains("## `--auth`"),
            "the contract does not document `--auth` as an honoured flag"
        );

        // POSITIVE CONTROLS: both files were genuinely read and are the ones intended.
        assert!(
            fixture.contains("reserved_for_later_phase"),
            "the fixture read is not a reserved-flag fixture"
        );
        assert!(
            reserved_section.contains("`--frontend`"),
            "the reserved section does not list `--frontend`, so the section split found the \
             wrong text"
        );
    }

    use super::*;
    use clap::CommandFactory;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("parses")
    }

    /// NO FLAG ANYWHERE IN THE SURFACE MAY CARRY A CREDENTIAL.
    ///
    /// # This was a comment, and a comment is not a gate
    ///
    /// The module above says "no flag that could will be added". An audit tested that claim by
    /// adding one. A **visible** `--database-password` was caught — but only by the byte-exact
    /// `--help` snapshot, which fails as "the surface changed" and prints its own regeneration
    /// command, so the reflex fix is to accept the new snapshot. A flag added with
    /// `#[arg(long, hide = true)]` — the pattern five reserved flags in this file already use —
    /// passed **262 tests with zero failures**.
    ///
    /// So this walks the whole parsed surface rather than any one command's help text. `hide` does
    /// not remove an argument from the command, only from the rendering, which is exactly why
    /// reflecting over `clap::Command` catches what a snapshot cannot.
    ///
    /// # Why a name test rather than a type test
    ///
    /// There is no type that means "not a credential". What there is, is a naming convention every
    /// credential-bearing flag in every CLI shares. Refusing the vocabulary is cruder than refusing
    /// the capability and it is what can actually be enforced here.
    ///
    /// A credential on a command line lands in shell history, in `ps` output, and in the CI log of
    /// whatever ran it. That is why the rule exists; this is why it holds.
    ///
    /// # What this cannot see, said so nobody reads it as more than it is
    ///
    /// It catches **naming, not behaviour**. An `--env-file` or `--database-init-file` pointing at
    /// a file that contains a credential passes cleanly, and so would a positional argument if
    /// `new` ever grew one. The property actually defended is *no credential enters through argv*,
    /// and a name matcher is a proxy for it — a good proxy, because a credential flag that hides
    /// its purpose in its name is a deliberate act rather than an oversight.
    ///
    /// It also says nothing about what reaches **output**. That is `tests/container.rs`'s
    /// substring check over stdout and the JSON document, which guards the label rather than the
    /// value. The two backstop each other: a value cannot leak from argv if argv cannot carry one,
    /// and a new key name is caught on the way out.
    #[test]
    fn no_flag_in_the_whole_surface_can_carry_a_credential() {
        /// Substrings a credential-bearing flag would almost certainly contain.
        ///
        /// `pass` and `pw` are the abbreviations, and they are here because the longer words
        /// missed them: `--db-pass` and `--db-pw` matched none of the rest. Checked against the
        /// current surface — 27 long flags — and neither substring collides with any of them. If
        /// one ever does, the resulting failure is the right place to have that conversation
        /// rather than a reason to soften the list now.
        const FORBIDDEN: [&str; 9] = [
            "password",
            "passwd",
            "pass",
            "pw",
            "secret",
            "credential",
            "token",
            "apikey",
            "api-key",
        ];

        fn walk(command: &clap::Command, path: &str, seen: &mut usize) {
            for argument in command.get_arguments() {
                *seen += 1;
                // `get_long` and the aliases, because an alias is as usable as a name.
                let names: Vec<String> = argument
                    .get_long()
                    .into_iter()
                    .chain(argument.get_all_aliases().unwrap_or_default())
                    .map(|name| name.to_ascii_lowercase().replace('_', "-"))
                    .collect();
                for name in names {
                    for forbidden in FORBIDDEN {
                        assert!(
                            !name.contains(forbidden),
                            "`{path} --{name}` looks like it carries a credential. A password on a \
                             command line reaches shell history, `ps`, and CI logs. Secrets belong \
                             in the environment or a file the tool never writes"
                        );
                    }
                }
            }
            for sub in command.get_subcommands() {
                let child = format!("{path} {}", sub.get_name());
                walk(sub, &child, seen);
            }
        }

        let command = Cli::command();
        let mut seen = 0;
        walk(&command, "renvor", &mut seen);

        // POSITIVE CONTROL. A walk that visited nothing would satisfy every assertion above, which
        // is the failure mode this whole test exists to prevent in someone else's code.
        assert!(
            seen > 20,
            "the surface walk visited only {seen} argument(s); it is not reading the command tree"
        );
        // And a control on the CHECK itself, not just the walk: the matcher must recognise a name
        // that should be refused.
        assert!(
            FORBIDDEN
                .iter()
                .any(|forbidden| "database-password".contains(forbidden)),
            "the forbidden list no longer matches an obviously credential-bearing name"
        );
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
            // `--orm` and `--database` are NO LONGER HERE. Phase 006 ships persistence, so
            // reporting "reserved for a later phase" from inside Phase 006 would be a false
            // statement of the same kind `--transport` stopped making in Phase 004. Their
            // replacement behaviour — an unsupported VALUE refused with the supported ones named,
            // and `--orm` without `--database` refused as a combination — is asserted below.
            // `--auth` is NO LONGER HERE. Phase 011 ships the auth starter, so reporting
            // "reserved for Phase 011" from inside Phase 011 would be the false statement
            // `--transport` and `--database` stopped making in their phases. Its honoured
            // behaviour is asserted in `config::model`'s tests and by `every_governed_choice…`.
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
            // PHASE 006 MOVED BOTH OF THESE ROWS.
            //
            // `persistence model` has exactly ONE supported value (`sqlx`), which clause 2 permits
            // to be defaulted without prompting provided it is RECORDED — and it is, in
            // `renvor.toml`, whenever a database was chosen.
            ("persistence model", Defaulted("--orm")),
            // `database` has TWO supported values, so clause 2 does not apply: it is a real flag
            // the generator acts on, and the wizard therefore asks about it (FR-046).
            ("database", Honoured(&["--database"])),
            // PHASE 011 MOVED THIS ROW (W-023). Two supported values, so it is asked, not
            // defaulted; honoured by the generated starter's dependencies, migrations, routes,
            // and wiring; recorded in `renvor.toml`.
            ("auth starter", Honoured(&["--auth"])),
            ("frontend", Reserved("--frontend")),
            ("compatible render mode", Reserved("--render-mode")),
            ("styling profile where applicable", Reserved("--styling")),
            ("desktop option", Reserved("--desktop")),
            // PHASE 011 (W-024): the five Phase 010 capabilities are a real, asked-about choice
            // beside the three Phase 003 conveniences this row already carried.
            (
                "capabilities",
                Honoured(&[
                    "--capabilities",
                    "--example-domain",
                    "--seed-data",
                    "--container",
                ]),
            ),
            // The framework path is where the framework IS, not what the project does: local
            // tooling, asked only when a selection needs it.
            (
                "local tooling",
                Honoured(&["--local-domain", "--local-https", "--framework-path"]),
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
                        if *flag == "--database" {
                            argv.push("postgres");
                        }
                        if *flag == "--auth" || *flag == "--capabilities" {
                            argv.push("none");
                        }
                        if *flag == "--framework-path" {
                            argv.push("framework");
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

                    // Defaulted without prompting: parsing with no flag at all still yields a
                    // usable value.
                    let cli = Cli::try_parse_from(["renvor", "new", "demo"]).expect("parses");
                    let Command::New(args) = cli.command else {
                        panic!("expected new")
                    };

                    // CORRECTED 2026-08-23 (requirements review R-7). This arm previously asserted
                    // `args.target == "api"` and `Target::parse` for EVERY defaulted row while its
                    // failure messages interpolated `{choice}` — so for the `transport` row it
                    // proved only that `--transport` is absent from the reserved table, and read as
                    // though it had proved far more. Same failure class as ledger L-11: a name
                    // promising what the body never observes. Each row now asserts its own type.
                    match choice {
                        "target" => {
                            assert_eq!(
                                args.target, "api",
                                "`{choice}` must be defaulted without prompting"
                            );
                            assert!(
                                crate::config::model::Target::parse("api").is_ok(),
                                "the defaulted value must be a supported one"
                            );
                            assert!(
                                crate::config::model::Target::parse("nope").is_err(),
                                "if every value parsed, `{choice}` would not be single-valued and \
                                 could not be defaulted without prompting"
                            );
                        }
                        "transport" => {
                            assert!(
                                args.transport.is_none(),
                                "`{choice}` must not be required on the command line"
                            );
                            assert!(
                                crate::config::model::Transport::parse("rest").is_ok(),
                                "the defaulted value must be a supported one"
                            );
                            assert!(
                                crate::config::model::Transport::parse("nope").is_err(),
                                "if every value parsed, `{choice}` would not be single-valued and \
                                 could not be defaulted without prompting"
                            );
                        }
                        "persistence model" => {
                            assert!(
                                args.orm.is_none(),
                                "`{choice}` must not be required on the command line"
                            );
                            assert!(
                                crate::config::model::Orm::parse("sqlx").is_ok(),
                                "the defaulted value must be a supported one"
                            );
                            assert!(
                                crate::config::model::Orm::parse("nope").is_err(),
                                "if every value parsed, `{choice}` would not be single-valued and \
                                 could not be defaulted without prompting"
                            );
                        }
                        other => panic!(
                            "`{other}` is classified Defaulted but this arm asserts nothing about \
                             it; add its case rather than letting it pass unobserved"
                        ),
                    }
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
