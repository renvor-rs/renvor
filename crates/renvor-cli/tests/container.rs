//! Container development controls: the generated profile, its bounds, and its refusals.
//!
//! # What is asserted here and what is asserted against real Docker
//!
//! Everything in this file is **platform-independent**: it generates trees and reads them. No test
//! here starts a container, pulls an image, or needs a daemon, so the whole file runs identically
//! on the Linux, macOS, and Windows legs.
//!
//! Starting the services, waiting for health, and connecting through the generated configuration
//! are asserted separately against a real daemon, and that evidence names the Docker version it ran
//! under. Splitting them is deliberate: a suite that skipped half its cases on two platforms would
//! report the same "ok" as one that ran them.
//!
//! # The secret assertions are absence assertions
//!
//! Several tests below assert that a credential is **not** present rather than that a redaction
//! marker is. A marker can be printed beside the secret it was meant to replace; an absent string
//! cannot be there at all.

mod harness;

use harness::renvor;

/// A generated project, or the failure that stopped it being one.
struct Generated {
    code: i32,
    stdout: String,
    stderr: String,
    root: std::path::PathBuf,
    _directory: tempfile::TempDir,
}

impl Generated {
    fn read(&self, name: &str) -> String {
        // The io error is DELIBERATELY not interpolated. `renvor-core`'s diagnostics gate forbids
        // a credential-handling file from printing a rendering into a panic, because the run where
        // that matters is the run where redaction just broke. The file name is enough to find it.
        std::fs::read_to_string(self.root.join(name))
            .unwrap_or_else(|_| panic!("`{name}` is unreadable"))
    }

    fn has(&self, relative: &str) -> bool {
        self.root.join(relative).exists()
    }

    /// Every generated path, relative and sorted, so a file set can be compared exactly.
    fn files(&self) -> Vec<String> {
        fn walk(base: &std::path::Path, at: &std::path::Path, into: &mut Vec<String>) {
            let Ok(entries) = std::fs::read_dir(at) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(base, &path, into);
                } else if let Ok(relative) = path.strip_prefix(base) {
                    into.push(relative.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.root, &self.root, &mut found);
        found.sort();
        found
    }
}

/// Runs `renvor new` with the given extra flags.
fn generate(extra: &[&str]) -> Generated {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    let mut args = vec![
        "new",
        "demo",
        "--path",
        root.to_str().expect("utf-8"),
        "--yes",
    ];
    args.extend_from_slice(extra);
    let (code, stdout, stderr) = renvor(&args, directory.path(), &[]);
    Generated {
        code,
        stdout,
        stderr,
        root,
        _directory: directory,
    }
}

fn generate_ok(label: &[&str]) -> Generated {
    let generated = generate(label);
    // NEITHER STREAM IS PRINTED. See `read` above: this file handles credentials, and a helper
    // that dumped stdout on failure would dump them too. The flags identify the case; re-running
    // that one command shows the output.
    assert_eq!(
        generated.code, 0,
        "generation failed for these flags: {label:?}"
    );
    generated
}

// ───────────────────────────────────────────────────────────── the generated file set

/// The exact file set, per selection. Not "contains" — **equals**.
///
/// A containment assertion passes when the generator emits extra files nobody asked for, which is
/// how a `.env` holding a manufactured credential would slip in unnoticed.
#[test]
fn each_selection_generates_exactly_its_file_set() {
    let base = [
        ".gitignore",
        "Cargo.lock",
        "Cargo.toml",
        "README.md",
        "renvor.toml",
        "src/main.rs",
    ];
    let persistence = vec![
        "migrations/0001_create_item.down.sql",
        "migrations/0001_create_item.up.sql",
        "src/persistence.rs",
    ];
    let container = vec![
        ".dockerignore",
        ".env.example",
        "Dockerfile",
        "compose.yaml",
    ];

    for (label, args, extra) in [
        ("nothing", vec![], vec![]),
        ("containers only", vec!["--container"], container.clone()),
        (
            "persistence only",
            vec!["--database", "postgres"],
            persistence.clone(),
        ),
        (
            "both",
            vec!["--database", "postgres", "--container"],
            [persistence.clone(), container.clone()].concat(),
        ),
        (
            "both plus a cache",
            vec![
                "--database",
                "mysql",
                "--container",
                "--container-cache",
                "valkey",
            ],
            [persistence, container].concat(),
        ),
    ] {
        let generated = generate_ok(&args);
        let mut expected: Vec<String> = base
            .iter()
            .chain(extra.iter())
            .map(|path| (*path).to_owned())
            .collect();
        expected.sort();
        assert_eq!(
            generated.files(),
            expected,
            "the file set for `{label}` is not what was selected"
        );
    }
}

/// `--container` alone generates a Compose profile with no database and no cache service.
///
/// Supported on purpose: the application image and its private network are useful without either.
#[test]
fn containers_without_persistence_generate_no_database_service() {
    let generated = generate_ok(&["--container"]);
    let compose = generated.read("compose.yaml");
    assert!(compose.contains("app:"), "there is no application service");
    assert!(
        !compose.contains("  database:"),
        "a database service was generated for a project with no persistence"
    );
    assert!(!compose.contains("  cache:"), "an unasked cache service");
    assert!(
        !compose.contains("postgres") && !compose.contains("mysql"),
        "a database image leaked into a project with no persistence"
    );
    // And the manifest agrees, because a manifest that disagreed with the tree would be the
    // failure `renvor check` exists to catch.
    let manifest = generated.read("renvor.toml");
    assert!(manifest.contains("[container]"));
    assert!(!manifest.contains("database_image"));
    assert!(manifest.contains(r#"cache = "none""#));
}

/// Only the selected engine appears. Both directions, because one direction is half a test.
#[test]
fn only_the_selected_database_engine_appears_in_the_profile() {
    let postgres = generate_ok(&["--database", "postgres", "--container"]);
    let compose = postgres.read("compose.yaml");
    assert!(compose.contains("library/postgres:"));
    assert!(
        !compose.contains("library/mysql"),
        "a MySQL service was generated for a PostgreSQL project"
    );

    let mysql = generate_ok(&["--database", "mysql", "--container"]);
    let compose = mysql.read("compose.yaml");
    assert!(compose.contains("library/mysql:"));
    assert!(
        !compose.contains("library/postgres"),
        "a PostgreSQL service was generated for a MySQL project"
    );
    // The environment keys are engine-specific too, and getting them crossed produces a container
    // that starts and then serves the wrong database name.
    assert!(compose.contains("MYSQL_DATABASE") && !compose.contains("POSTGRES_DB"));
}

/// Every tested version generates, and pins the image it names.
#[test]
fn every_offered_version_generates_and_pins_its_image() {
    for (name, variant, expected) in [
        ("postgres", "17", "postgres:17.11"),
        ("postgres", "18", "postgres:18.6"),
        ("mysql", "8.4", "mysql:8.4.11"),
        ("mysql", "9.7", "mysql:9.7.2"),
    ] {
        let generated = generate_ok(&[
            "--database",
            name,
            "--container",
            "--database-version",
            variant,
        ]);
        let compose = generated.read("compose.yaml");
        assert!(
            compose.contains(expected),
            "a tested version did not pin the image `{expected}`"
        );
        assert!(
            !compose.contains(":latest"),
            "a mutable `:latest` tag reached the profile"
        );
        assert!(
            generated
                .read("renvor.toml")
                .contains(&format!(r#"database_version = "{variant}""#)),
            "the manifest does not record the version that was generated"
        );
    }
}

/// PostgreSQL 17 and 18 mount different paths, and mounting the wrong one discards the data.
///
/// The 18 image moved `PGDATA` under a versioned directory and declares its volume one level up.
/// A profile that used 17's path on an 18 server would put the named volume somewhere the server
/// never writes — so `renvor docker down` would silently destroy the database it claims to keep.
#[test]
fn postgresql_17_and_18_mount_their_own_data_directories() {
    let seventeen = generate_ok(&[
        "--database",
        "postgres",
        "--container",
        "--database-version",
        "17",
    ]);
    assert!(
        seventeen
            .read("compose.yaml")
            .contains("database-data:/var/lib/postgresql/data")
    );

    let eighteen = generate_ok(&[
        "--database",
        "postgres",
        "--container",
        "--database-version",
        "18",
    ]);
    let compose = eighteen.read("compose.yaml");
    assert!(compose.contains("database-data:/var/lib/postgresql\n"));
    assert!(
        !compose.contains("/var/lib/postgresql/data"),
        "PostgreSQL 18 was given PostgreSQL 17's data directory, which would discard the volume"
    );
}

/// Every published port binds loopback, on every selection that publishes one.
///
/// # This was claimed and not asserted
///
/// The conformance record said "asserted on the rendered file for all five matrix rows". Those
/// rows came from a shell script that lived in a scratchpad directory — not in the repository, not
/// reproducible by anyone else, and not in CI. One of its five rows read `all 0 published ports
/// bound to 127.0.0.1`: an assertion over an empty set.
///
/// The template hardcodes the `127.0.0.1:` prefix, so no operator input can change it. The
/// regression a test *can* catch is a template edit, and until now nothing caught one.
#[test]
fn every_published_port_binds_loopback_and_none_binds_every_interface() {
    let mut checked = 0;
    for args in [
        vec!["--database", "postgres", "--container"],
        vec!["--database", "mysql", "--container"],
        vec![
            "--database",
            "postgres",
            "--container",
            "--container-cache",
            "valkey",
        ],
    ] {
        let compose = generate_ok(&args).read("compose.yaml");
        let published: Vec<&str> = compose
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("- \"") && line.matches(':').count() >= 2)
            .collect();
        // POSITIVE CONTROL. Without it, a template that published nothing would satisfy the loop
        // below by having nothing to iterate — which is exactly how the shell script's fifth row
        // "passed".
        assert!(
            !published.is_empty(),
            "no published port was found, so the loop below covers nothing"
        );
        for line in &published {
            assert!(
                line.contains("\"127.0.0.1:"),
                "a published port does not bind loopback"
            );
        }
        checked += published.len();
    }
    assert!(checked >= 4, "too few ports examined to be meaningful");
}

/// The security posture of the rendered profile — all of it, not one property.
///
/// # Five of six were asserted nowhere
///
/// The record claimed `no-new-privileges` on every service, a `read_only` application container,
/// and the absence of privileged mode, host networking, a Docker socket mount and a broad host
/// mount. Only the `:latest` check existed in this suite; three of the rest were asserted in no
/// automated test at all. Every one is true of the template today, and nothing would have caught
/// their removal.
#[test]
fn the_rendered_profile_keeps_its_security_posture() {
    for args in [
        vec!["--container"],
        vec!["--database", "postgres", "--container"],
        vec![
            "--database",
            "mysql",
            "--container",
            "--container-cache",
            "valkey",
        ],
    ] {
        let compose = generate_ok(&args).read("compose.yaml");

        // COUNTED, not merely found, so dropping it from one of three services is a failure.
        //
        // Scoped to the `services:` block. A bare two-space-indent count also picks up `internal:`
        // under `networks:` and each named volume, which made this report "1 of 2" for a profile
        // with exactly one service.
        let mut in_services = false;
        let mut services = 0;
        for line in compose.lines() {
            if !line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
                in_services = line.trim_end() == "services:";
                continue;
            }
            if in_services
                && line.starts_with("  ")
                && !line.starts_with("   ")
                && line.trim_end().ends_with(':')
            {
                services += 1;
            }
        }
        let count = compose.matches("no-new-privileges:true").count();
        assert!(
            count > 0 && count == services,
            "no-new-privileges is not on every service (see the `count` and service totals)"
        );
        assert!(
            compose.contains("read_only: true"),
            "the application container is not read-only"
        );

        // COMMENTS STRIPPED FIRST. The template explains what it does NOT do — "No `external:
        // true` and no host networking" — so a naive substring search over the whole file flags
        // the sentence saying the setting is absent. The assertion is about the settings, not the
        // prose about them.
        let settings: String = compose
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "privileged: true",
            "network_mode: host",
            "docker.sock",
            "/var/run/docker",
            "external: true",
            ":latest",
        ] {
            assert!(
                !settings.contains(forbidden),
                "the profile contains a forbidden setting"
            );
        }
        // A broad host mount. A named volume has no leading `/`, `.` or `~`; a bind mount does.
        for line in settings.lines().map(str::trim) {
            if let Some(volume) = line.strip_prefix("- ")
                && !volume.starts_with('"')
                && !volume.starts_with("valkey")
                && !volume.starts_with("--")
                && !volume.starts_with("${")
                && !volume.starts_with("internal")
                && !volume.starts_with(".env")
            {
                assert!(
                    !volume.starts_with('/')
                        && !volume.starts_with('.')
                        && !volume.starts_with('~'),
                    "a host path is bind-mounted into a container"
                );
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────────── the cache

#[test]
fn the_cache_is_absent_unless_it_is_asked_for() {
    let without = generate_ok(&["--database", "postgres", "--container"]);
    assert!(!without.read("compose.yaml").contains("  cache:"));
    assert!(without.read("renvor.toml").contains(r#"cache = "none""#));
    assert!(
        !without
            .read(".env.example")
            .contains("RENVOR_CACHE_PASSWORD"),
        "a placeholder for a service that will not exist"
    );

    let with = generate_ok(&[
        "--database",
        "postgres",
        "--container",
        "--container-cache",
        "valkey",
    ]);
    let compose = with.read("compose.yaml");
    assert!(compose.contains("  cache:"));
    assert!(compose.contains("valkey/valkey:9.1.1"));
    assert!(with.read(".env.example").contains("RENVOR_CACHE_PASSWORD="));
}

/// The cache is stated to be uninvolved, in the two places somebody would look.
///
/// FR: the README and the manifest must both say so. A generated tree that shipped a cache service
/// without saying it is unwired would be read as "this project caches", and the adapter that would
/// make that true does not exist until Phase 010.
#[test]
fn the_cache_says_it_is_not_wired_into_the_application() {
    let generated = generate_ok(&[
        "--database",
        "postgres",
        "--container",
        "--container-cache",
        "valkey",
    ]);
    let readme = generated.read("README.md");
    assert!(
        readme.contains("optional local infrastructure") && readme.contains("Phase 010"),
        "the README does not state the cache limitation"
    );
    assert!(
        generated
            .read("renvor.toml")
            .contains("cache_wired_into_application = false"),
        "the manifest does not record the cache limitation"
    );
    assert!(
        generated.read("compose.yaml").contains("Phase 010"),
        "the profile itself does not state the limitation"
    );
}

// ─────────────────────────────────────────────────────────────────────────── secrets

/// Generation never writes a `.env`, and the example it does write holds nothing usable.
#[test]
fn generation_writes_an_example_and_never_a_credential() {
    let generated = generate_ok(&[
        "--database",
        "postgres",
        "--container",
        "--container-cache",
        "valkey",
    ]);
    assert!(
        !generated.has(".env"),
        "generation wrote a `.env`, which means it invented and persisted a credential"
    );

    let example = generated.read(".env.example");
    for line in example.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (_key, value) = line.split_once('=').expect("a KEY=VALUE line");
        assert!(
            value.is_empty(),
            "a `.env.example` line carries a value, which is a credential in a committed file"
        );
    }
    assert!(example.contains("RENVOR_DATABASE_PASSWORD="));
}

/// `.env` is ignored by git, and `.env.example` is not swept up with it.
#[test]
fn the_env_file_is_ignored_and_its_example_is_not() {
    let generated = generate_ok(&["--database", "postgres", "--container"]);
    let ignore = generated.read(".gitignore");
    assert!(ignore.lines().any(|line| line.trim() == ".env"));
    assert!(
        ignore.lines().any(|line| line.trim() == "!.env.example"),
        "`.env.*` would otherwise sweep up the example that is meant to be committed"
    );
    // The build context is the other route a credential takes into a shared artefact.
    let dockerignore = generated.read(".dockerignore");
    assert!(dockerignore.lines().any(|line| line.trim() == ".env"));
    assert!(
        dockerignore
            .lines()
            .any(|line| line.trim() == "!.env.example")
    );
}

/// Compose must fail closed on a missing secret rather than substituting an empty one.
#[test]
fn a_missing_secret_is_a_refusal_rather_than_a_blank_password() {
    let generated = generate_ok(&[
        "--database",
        "postgres",
        "--container",
        "--container-cache",
        "valkey",
    ]);
    let compose = generated.read("compose.yaml");
    for name in ["RENVOR_DATABASE_PASSWORD", "RENVOR_CACHE_PASSWORD"] {
        assert!(
            compose.contains(&format!("${{{name}:?")),
            "a required secret does not use the `${{VAR:?}}` form, so Compose would substitute an empty string: {name}"
        );
        assert!(
            !compose.contains(&format!("${{{name}:-")),
            "a required secret has a DEFAULT, which is the silent fallback this form exists to prevent: {name}"
        );
    }
}

/// No credential reaches human output, JSON output, or the health-check command text.
#[test]
fn no_secret_reaches_any_output_or_any_command_text() {
    let generated = generate_ok(&[
        "--database",
        "postgres",
        "--container",
        "--container-cache",
        "valkey",
    ]);
    let compose = generated.read("compose.yaml");

    // The health check is visible through `docker inspect` and in the container's process list.
    let health: String = compose
        .lines()
        .filter(|line| line.trim_start().starts_with("test:"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!health.is_empty(), "no health check was generated");
    for forbidden in ["PASSWORD", "password", "-p", "--password"] {
        assert!(
            !health.contains(forbidden),
            "a credential-shaped argument reached a health-check command"
        );
    }

    // AND EVERY OTHER PLACE A SECRET COULD BE WRITTEN DOWN.
    //
    // This examined `test:` lines only, while the rationale it cites — "`docker inspect` and the
    // container's process list expose it" — applies just as much to `command:` and `environment:`.
    // The cache password IS on the server's command line, and this test did not look there.
    //
    // The requirement is not that the word never appears — an env var is *named*
    // `RENVOR_CACHE_PASSWORD`. It is that no credential is ever written as a LITERAL: every
    // occurrence must be a `${...}` reference resolved at run time from `.env`.
    for line in compose.lines() {
        let lowered = line.to_ascii_lowercase();
        if line.trim_start().starts_with('#')
            || (!lowered.contains("password") && !lowered.contains("requirepass"))
        {
            continue;
        }
        assert!(
            line.contains("${") || line.trim_end().ends_with("--requirepass"),
            "a line mentions a credential without a `${{...}}` reference, so it may be a literal"
        );
    }

    for stream in [&generated.stdout, &generated.stderr] {
        assert!(!stream.contains("RENVOR_DATABASE_PASSWORD"));
        assert!(!stream.to_ascii_lowercase().contains("password"));
    }
}

#[test]
fn the_json_output_carries_the_configuration_and_no_secret() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    let (code, stdout, _stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            root.to_str().expect("utf-8"),
            "--yes",
            "--output",
            "json",
            "--database",
            "mysql",
            "--container",
            "--container-cache",
            "valkey",
            "--database-port",
            "13306",
            "--cache-port",
            "16379",
        ],
        directory.path(),
        &[],
    );
    assert_eq!(code, 0, "generation failed");

    let document: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let settings = &document["result"]["configuration"]["container_settings"];
    assert_eq!(settings["database_version"], "9.7");
    assert_eq!(settings["database_port"], 13306);
    assert_eq!(settings["cache"], "valkey");
    assert_eq!(settings["cache_port"], 16379);
    // The spelling must be one the flags accept back — a `"my-sql97"` would be plausible JSON and
    // an unusable value.
    assert!(
        !stdout.contains("my-sql"),
        "a variant name leaked into JSON"
    );
    assert!(!stdout.to_ascii_lowercase().contains("password"));
}

// ───────────────────────────────────────────────────────────── refusals and bounds

/// A container flag without `--container` is refused rather than ignored.
#[test]
fn a_container_flag_without_container_is_an_unsupported_combination() {
    for label in [
        vec!["--database-version", "18"],
        vec!["--database-name", "demo"],
        vec!["--database-user", "renvor"],
        vec!["--database-port", "5432"],
        vec!["--container-cache", "valkey"],
        vec!["--cache-port", "6379"],
    ] {
        let mut args = vec!["--database", "postgres"];
        args.extend_from_slice(&label);
        let generated = generate(&args);
        assert_eq!(
            generated.code, 3,
            "a container flag was accepted without `--container`: {label:?}"
        );
        assert!(
            generated.stderr.contains("--container"),
            "the refusal does not name the flag that would make it coherent"
        );
        assert!(
            !generated.root.exists(),
            "a refused combination still wrote a project"
        );
    }
}

/// A database container flag without a database is refused rather than ignored.
#[test]
fn a_database_container_flag_without_a_database_is_an_unsupported_combination() {
    for label in [
        vec!["--database-version", "18"],
        vec!["--database-name", "demo"],
        vec!["--database-user", "renvor"],
        vec!["--database-port", "5432"],
    ] {
        let mut args = vec!["--container"];
        args.extend_from_slice(&label);
        let generated = generate(&args);
        assert_eq!(
            generated.code, 3,
            "a database container flag was accepted without `--database`: {label:?}"
        );
        assert!(generated.stderr.contains("--database"));
    }
}

/// A cache port with no cache is refused.
#[test]
fn a_cache_port_without_a_cache_is_an_unsupported_combination() {
    let generated = generate(&["--container", "--cache-port", "6379"]);
    assert_eq!(generated.code, 3);
    assert!(generated.stderr.contains("--container-cache"));

    let explicit = generate(&[
        "--container",
        "--container-cache",
        "none",
        "--cache-port",
        "6379",
    ]);
    assert_eq!(
        explicit.code, 3,
        "an explicit `none` with a port is the same incoherent request"
    );
}

/// Two services cannot publish the same host port, and the second is not silently moved.
#[test]
fn a_duplicate_host_port_is_refused_rather_than_reassigned() {
    let generated = generate(&[
        "--database",
        "postgres",
        "--container",
        "--container-cache",
        "valkey",
        "--database-port",
        "15432",
        "--cache-port",
        "15432",
    ]);
    assert_eq!(generated.code, 3);
    assert!(generated.stderr.contains("15432"));
    assert!(
        !generated.root.exists(),
        "a refused combination still wrote a project"
    );
}

/// The port bounds, at both edges.
#[test]
fn port_zero_and_port_65536_are_refused_and_the_edges_are_accepted() {
    // Exit 3 is VALIDATION: the value parsed as an argument and renvor refused it.
    for label in ["0", "65536", "99999", "http", "8080.5", ""] {
        let generated = generate(&[
            "--database",
            "postgres",
            "--container",
            "--database-port",
            label,
        ]);
        assert_eq!(
            generated.code, 3,
            "an out-of-range or non-numeric `--database-port` was accepted: {label}"
        );
        assert!(
            generated.stderr.contains("--database-port"),
            "the refusal does not name the flag"
        );
    }

    // `-1` is refused with exit 2 — USAGE — and that is correct rather than a gap. clap sees a
    // leading `-` and reports an unknown argument before any value reaches renvor, so there is no
    // value to validate. Asserted as its own case rather than folded into the loop above, because
    // a test that accepted either code would also accept `-1` being silently taken as a port.
    let negative = generate(&[
        "--database",
        "postgres",
        "--container",
        "--database-port",
        "-1",
    ]);
    assert_eq!(
        negative.code, 2,
        "`--database-port -1` was not refused as a usage error"
    );
    for label in ["1", "65535"] {
        let generated = generate(&[
            "--database",
            "postgres",
            "--container",
            "--database-port",
            label,
        ]);
        assert_eq!(
            generated.code, 0,
            "a legal edge-of-range `--database-port` was refused: {label}"
        );
    }
}

/// An identifier that would need quoting is refused, and a too-long one is not truncated.
#[test]
fn an_invalid_database_name_or_user_is_refused_by_name() {
    for label in [
        ("--database-name", "my-shop"),
        ("--database-name", "1shop"),
        ("--database-name", "shop;DROP TABLE x"),
        ("--database-name", "shop name"),
        ("--database-name", &"a".repeat(64)),
        ("--database-user", "my-user"),
        ("--database-user", &"u".repeat(33)),
    ] {
        let generated = generate(&["--database", "postgres", "--container", label.0, label.1]);
        assert_eq!(
            generated.code, 3,
            "an invalid identifier was accepted: {label:?}"
        );
        assert!(
            generated.stderr.contains(label.0),
            "the refusal does not name the flag"
        );
    }
}

/// An unknown engine or version is refused, and the refusal names what IS supported.
#[test]
fn an_unknown_engine_or_version_is_refused_with_the_supported_values() {
    let version = generate(&[
        "--database",
        "postgres",
        "--container",
        "--database-version",
        "16",
    ]);
    assert_eq!(version.code, 3);
    assert!(version.stderr.contains("17") && version.stderr.contains("18"));

    // A version belonging to the OTHER engine is a real mistake, not a near-miss to be tolerated.
    let crossed = generate(&[
        "--database",
        "postgres",
        "--container",
        "--database-version",
        "8.4",
    ]);
    assert_eq!(crossed.code, 3);

    let cache = generate(&["--container", "--container-cache", "redis"]);
    assert_eq!(cache.code, 3);
    assert!(cache.stderr.contains("valkey"));
}

// ─────────────────────────────────────────────────────────────────────── defaults

#[test]
fn the_defaults_are_the_documented_ones_and_are_recorded() {
    let generated = generate_ok(&["--database", "postgres", "--container"]);
    let manifest = generated.read("renvor.toml");
    // Newest tested version, the non-superuser default, the engine's own port, and the project
    // name with `-` as `_`. Every one is RECORDED, so an operator reading the manifest does not
    // have to know what the default was on the day it ran.
    assert!(manifest.contains(r#"database_version = "18""#));
    assert!(manifest.contains(r#"database_user = "renvor""#));
    assert!(manifest.contains("database_port = 5432"));
    assert!(manifest.contains(r#"database_name = "demo""#));
    assert!(manifest.contains(r#"cache = "none""#));

    let mysql = generate_ok(&["--database", "mysql", "--container"]);
    assert!(mysql.read("renvor.toml").contains("database_port = 3306"));
}

/// A hyphenated project name derives an underscored database name, and only that.
#[test]
fn a_hyphenated_project_name_derives_a_legal_database_name() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("my-shop-api");
    let (code, _stdout, _stderr) = renvor(
        &[
            "new",
            "my-shop-api",
            "--path",
            root.to_str().expect("utf-8"),
            "--yes",
            "--database",
            "postgres",
            "--container",
        ],
        directory.path(),
        &[],
    );
    assert_eq!(code, 0, "generation failed");
    let manifest = std::fs::read_to_string(root.join("renvor.toml")).expect("manifest");
    assert!(manifest.contains(r#"database_name = "my_shop_api""#));
    assert!(
        !manifest.contains(r#"database_name = "my-shop-api""#),
        "a hyphen reached an unquoted identifier"
    );
}

// ─────────────────────────────────────────────────────────── dry run and offline

/// Dry run lists every container file and writes none of them.
#[test]
fn a_dry_run_lists_every_container_file_and_writes_nothing() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    let (code, stdout, _stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            root.to_str().expect("utf-8"),
            "--yes",
            "--dry-run",
            "--output",
            "json",
            "--database",
            "postgres",
            "--container",
            "--container-cache",
            "valkey",
        ],
        directory.path(),
        &[],
    );
    assert_eq!(code, 0, "generation failed");
    assert!(
        !root.exists(),
        "a dry run created the destination, which is the one thing it must not do"
    );

    let document: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(document["result"]["dryRun"], true);
    let listed: Vec<String> = document["result"]["manifest"]
        .as_array()
        .expect("a manifest array")
        .iter()
        .map(|entry| entry["path"].as_str().expect("a path").to_owned())
        .collect();
    for expected in [
        ".dockerignore",
        ".env.example",
        "Dockerfile",
        "compose.yaml",
    ] {
        assert!(
            listed.iter().any(|path| path == expected),
            "the dry run did not list `{expected}`, so it under-reports what it would create"
        );
    }
    assert!(
        !listed.iter().any(|path| path == ".env"),
        "the dry run claims it would write a `.env`"
    );
}

/// Generation renders files. It does not reach a registry, and there is nothing in it that could.
#[test]
fn generation_pulls_no_image_and_starts_no_container() {
    let blackhole = "http://127.0.0.1:1";
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = directory.path().join("demo");
    let (code, _stdout, _stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            root.to_str().expect("utf-8"),
            "--yes",
            "--database",
            "postgres",
            "--container",
            "--container-cache",
            "valkey",
        ],
        directory.path(),
        &[
            ("http_proxy", blackhole),
            ("https_proxy", blackhole),
            ("HTTP_PROXY", blackhole),
            ("HTTPS_PROXY", blackhole),
            ("ALL_PROXY", blackhole),
            ("no_proxy", ""),
            ("NO_PROXY", ""),
            // A daemon that cannot be reached. If generation tried to talk to Docker at all, this
            // is where it would fail rather than quietly succeed against a running local one.
            ("DOCKER_HOST", "tcp://127.0.0.1:1"),
            ("CARGO_NET_OFFLINE", "true"),
        ],
    );
    assert_eq!(code, 0, "generation needed the network or the daemon");
    assert!(root.join("compose.yaml").is_file());
}

/// `renvor docker up --dry-run` starts nothing.
#[test]
fn docker_up_dry_run_starts_nothing() {
    let generated = generate_ok(&["--database", "postgres", "--container"]);
    let (code, stdout, stderr) = renvor(
        &["docker", "up", "--dry-run"],
        &generated.root,
        // Pointed at a daemon that is not there: if the dry run were to actually invoke Docker,
        // it would fail here instead of reporting what it would have done.
        &[("DOCKER_HOST", "tcp://127.0.0.1:1")],
    );
    assert_eq!(code, 0, "generation failed");
    assert!(
        stdout.contains("Dry run") || stderr.contains("Dry run"),
        "the dry run did not say it was one"
    );
}

// ──────────────────────────────────────────────────── flags and prompts agree

/// The equivalent command reproduces a container project byte for byte.
///
/// FR-009 calls this string the *exact* equivalent command. Until this phase it omitted
/// `--database` entirely, so pasting it produced a project with no persistence — printed but never
/// executed, and therefore a claim rather than a contract. This executes it.
#[test]
fn the_equivalent_command_reproduces_a_container_project() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let first = directory.path().join("a").join("demo");
    std::fs::create_dir_all(first.parent().expect("a parent")).expect("created");

    let (code, stdout, _stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            first.to_str().expect("utf-8"),
            "--yes",
            "--output",
            "json",
            "--database",
            "mysql",
            "--container",
            "--container-cache",
            "valkey",
            "--database-port",
            "13306",
            "--cache-port",
            "16379",
        ],
        directory.path(),
        &[],
    );
    assert_eq!(code, 0, "generation failed");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let settings = &document["result"]["configuration"]["container_settings"];

    // Rebuilt from the RESOLVED configuration rather than from the flags above, so the second run
    // is driven by what the first run decided — including the values it defaulted.
    let second = directory.path().join("b").join("demo");
    std::fs::create_dir_all(second.parent().expect("a parent")).expect("created");
    let port = settings["database_port"].to_string();
    let cache_port = settings["cache_port"].to_string();
    let version = settings["database_version"]
        .as_str()
        .expect("a version")
        .to_owned();
    let name = settings["database_name"]
        .as_str()
        .expect("a name")
        .to_owned();
    let user = settings["database_user"]
        .as_str()
        .expect("a user")
        .to_owned();
    let (code, _stdout, _stderr) = renvor(
        &[
            "new",
            "demo",
            "--path",
            second.to_str().expect("utf-8"),
            "--yes",
            "--database",
            "mysql",
            "--orm",
            "sqlx",
            "--container",
            "--database-version",
            &version,
            "--database-name",
            &name,
            "--database-user",
            &user,
            "--database-port",
            &port,
            "--container-cache",
            "valkey",
            "--cache-port",
            &cache_port,
        ],
        directory.path(),
        &[],
    );
    assert_eq!(code, 0, "generation failed");

    for name in [
        "compose.yaml",
        "renvor.toml",
        ".env.example",
        ".dockerignore",
    ] {
        assert_eq!(
            std::fs::read_to_string(first.join(name)).expect("first"),
            std::fs::read_to_string(second.join(name)).expect("second"),
            "`{name}` differs between the original run and its recorded equivalent"
        );
    }
}

/// `renvor check` accepts the manifest this generator writes.
///
/// A generator whose own output fails its own validator is the defect this catches, and it has
/// happened before: `[persistence]` was refused by `deny_unknown_fields` when it was introduced.
#[test]
fn renvor_check_accepts_a_generated_container_manifest() {
    for label in [
        vec!["--container"],
        vec!["--database", "postgres", "--container"],
        vec![
            "--database",
            "mysql",
            "--container",
            "--container-cache",
            "valkey",
        ],
    ] {
        let generated = generate_ok(&label);
        let (code, _stdout, _stderr) = renvor(&["check", "."], &generated.root, &[]);
        assert_eq!(
            code, 0,
            "`renvor check` rejected its own generator's output for these flags: {label:?}"
        );
    }
}

/// A manifest carrying a credential is REFUSED, which is what makes "no secrets here" enforceable.
#[test]
fn a_manifest_that_grew_a_password_field_is_refused() {
    let generated = generate_ok(&["--database", "postgres", "--container"]);
    let path = generated.root.join("renvor.toml");
    let mut manifest = std::fs::read_to_string(&path).expect("manifest");
    manifest.push_str("\ndatabase_password = \"hunter2\"\n");
    std::fs::write(&path, manifest).expect("written");

    let (code, _stdout, _stderr) = renvor(&["check", "."], &generated.root, &[]);
    assert_ne!(code, 0, "a manifest carrying a password passed validation");
}
