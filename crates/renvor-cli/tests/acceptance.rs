//! Acceptance criteria that are properties of the whole program rather than of one module.
//!
//! Each test below names the criterion it discharges. Where a criterion is only **partially**
//! discharged, the test says so in its own comment rather than in a document somebody has to find.

use std::path::Path;
use std::process::Command;

/// Runs `renvor` and returns (exit code, stdout, stderr).
fn renvor(args: &[&str], directory: &Path, env: &[(&str, &str)]) -> (i32, String, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_renvor"));
    command.args(args).current_dir(directory);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("renvor runs");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

// ── Fail closed on hostile destinations ─────────────────────────────────────────────────────

#[test]
fn every_hostile_destination_is_refused_and_writes_nothing() {
    // "malicious paths, symlinks, and overwrite attempts fail closed."
    //
    // Asserted from OUTSIDE the process, so it covers the wiring as well as the validator. A unit
    // test on `Destination::open` proves the rule exists; this proves the rule is reached.
    let base = tempfile::tempdir().expect("tempdir");
    let outside = base.path().join("outside");
    std::fs::create_dir_all(&outside).expect("mkdir");
    std::fs::write(outside.join("precious"), b"do not touch").expect("write");
    let work = base.path().join("work");
    std::fs::create_dir_all(&work).expect("mkdir");

    // An existing, non-empty destination.
    std::fs::create_dir_all(work.join("occupied")).expect("mkdir");
    std::fs::write(work.join("occupied/theirs"), b"theirs").expect("write");

    // A symlink pointing out of the tree.
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, work.join("linked")).expect("symlink");

    let hostile: Vec<&str> = {
        let mut cases = vec!["../escape", "occupied", "..", "."];
        if cfg!(unix) {
            cases.push("linked");
        }
        cases
    };

    for case in hostile {
        let (code, stdout, _) = renvor(
            &["new", "demo", "--path", case, "--yes", "--output", "json"],
            &work,
            &[],
        );
        assert_eq!(
            code, 3,
            "`{case}` must be refused with a validation exit, got {code}"
        );
        let document: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("one JSON document even on failure");
        assert_eq!(document["status"], "failure", "{case}");
        assert!(
            document["error"]["code"].is_string(),
            "{case} produced no stable error code"
        );
    }

    // NOTHING was written anywhere by any of them.
    assert_eq!(
        std::fs::read_to_string(outside.join("precious")).expect("read"),
        "do not touch",
        "a refused destination still reached outside the tree"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("occupied/theirs")).expect("read"),
        "theirs",
        "an occupied destination was overwritten"
    );
    let residue: Vec<String> = std::fs::read_dir(&work)
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".renvor-staging-"))
        .collect();
    assert!(
        residue.is_empty(),
        "refused runs left staging behind: {residue:?}"
    );
}

#[test]
fn an_ordinary_destination_is_still_accepted() {
    // POSITIVE CONTROL for the test above. A generator that refused every path would satisfy every
    // assertion there and be useless.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, _, stderr) = renvor(&["new", "fine", "--yes"], base.path(), &[]);
    assert_eq!(code, 0, "an ordinary destination was refused:\n{stderr}");
    assert!(base.path().join("fine/Cargo.toml").is_file());
}

// ── Local TLS trust ─────────────────────────────────────────────────────────────────────────

#[test]
fn requesting_local_https_issues_nothing_and_touches_no_trust_store() {
    // FR-036: "local TLS trust is explicit and never installed silently."
    //
    // The strongest available assertion for "never installed silently" is that the generated tree
    // contains no key, no certificate, and no invocation of a trust-store tool — checked over the
    // WHOLE tree rather than over the files we expected to write.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, _, stderr) = renvor(
        &["new", "secure", "--yes", "--local-https"],
        base.path(),
        &[],
    );
    assert_eq!(code, 0, "{stderr}");

    let project = base.path().join("secure");
    let mut offenders = Vec::new();
    let mut stack = vec![project.clone()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("read_dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if name.ends_with(".pem") || name.ends_with(".key") || name.ends_with(".crt") {
                offenders.push(format!("{name} (a key or certificate)"));
            }
            let contents = std::fs::read_to_string(&path).unwrap_or_default();
            for tool in [
                "mkcert",
                "security add-trusted-cert",
                "certutil",
                "update-ca-certificates",
            ] {
                if contents.contains(tool) {
                    offenders.push(format!("{name} invokes `{tool}`"));
                }
            }
            if contents.contains("BEGIN CERTIFICATE") || contents.contains("BEGIN PRIVATE KEY") {
                offenders.push(format!("{name} embeds PEM material"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`--local-https` produced trust material: {offenders:?}"
    );

    // And the intent IS recorded, so a later phase can act on it without asking again.
    let manifest = std::fs::read_to_string(project.join("renvor.toml")).expect("read");
    assert!(
        manifest.contains("local_https = \"requested\""),
        "the request was not recorded:\n{manifest}"
    );
}

// ── Offline ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn generation_completes_with_every_proxy_pointed_at_a_closed_port() {
    // FR-043: "no command in this phase requires network access to complete its local flows, and
    // this MUST be demonstrated with networking unavailable rather than asserted."
    //
    // ── WHAT THIS DOES AND DOES NOT DEMONSTRATE ────────────────────────────────────────────
    //
    // Every proxy variable cargo and curl honour is pointed at a closed local port, so any attempt
    // to reach a registry or a host fails immediately rather than silently succeeding from a warm
    // cache. Generation — including the `cargo build` and `cargo test` that pre-placement
    // verification runs — must still complete.
    //
    // This is NOT full network isolation. A direct connection that ignores proxy variables would
    // not be blocked by it, and demonstrating that needs a network namespace, which is not
    // portable across the three platforms this suite runs on. **The gap is stated here rather than
    // left for a reader to assume it was covered.** What makes the result meaningful anyway is the
    // structural fact asserted below: the generated project declares no dependencies at all, so
    // there is no registry for cargo to reach.
    let base = tempfile::tempdir().expect("tempdir");
    let blackhole = "http://127.0.0.1:1";
    let env: Vec<(&str, &str)> = vec![
        ("http_proxy", blackhole),
        ("https_proxy", blackhole),
        ("HTTP_PROXY", blackhole),
        ("HTTPS_PROXY", blackhole),
        ("ALL_PROXY", blackhole),
        ("all_proxy", blackhole),
        ("CARGO_NET_OFFLINE", "true"),
        ("no_proxy", ""),
        ("NO_PROXY", ""),
    ];

    let (code, _, stderr) = renvor(
        &["new", "offline", "--yes", "--example-domain", "--seed-data"],
        base.path(),
        &env,
    );
    assert_eq!(code, 0, "generation needed the network:\n{stderr}");

    // THE STRUCTURAL HALF, which is what makes the above more than a cache hit.
    let manifest = std::fs::read_to_string(base.path().join("offline/Cargo.toml")).expect("read");
    let dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("a dependencies section")
        .trim();
    assert!(
        dependencies.is_empty(),
        "the generated project declares dependencies, so `cargo build` needs a registry and the \
         offline claim rests on a warm cache rather than on there being nothing to fetch:\n\
         {dependencies}"
    );
}

#[test]
fn doctor_completes_with_every_proxy_pointed_at_a_closed_port() {
    // Same caveat as above. `doctor` runs local executables with `--version`; nothing resolves a
    // name.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, stdout, stderr) = renvor(
        &["doctor", "--output", "json"],
        base.path(),
        &[
            ("http_proxy", "http://127.0.0.1:1"),
            ("https_proxy", "http://127.0.0.1:1"),
        ],
    );
    assert_eq!(code, 0, "doctor needed the network:\n{stderr}");
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one document");
    assert!(document["result"]["probes"].is_array());
}

// ── The wizard and the flags produce one configuration ──────────────────────────────────────

#[test]
fn every_generated_manifest_round_trips_through_renvor_check() {
    // THE GATE THAT WOULD HAVE CAUGHT THE `True` DEFECT.
    //
    // MiniJinja renders a boolean as `True`, so `renvor.toml` carried `example_domain = True` —
    // invalid TOML. Every other test passed: the project compiled, formatted, tested, and ran,
    // because nothing read the manifest back. `renvor check` is the tool's own validator, and a
    // generator whose output its own validator rejects is broken in a way no build gate notices.
    //
    // Run over every variant, because the defect only appeared where a flag was true.
    for flags in [
        &[][..],
        &["--example-domain"],
        &["--example-domain", "--seed-data"],
        &["--example-domain", "--seed-data", "--container"],
        &["--container"],
        &["--local-https"],
    ] {
        let base = tempfile::tempdir().expect("tempdir");
        let mut args = vec!["new", "rt", "--yes"];
        args.extend_from_slice(flags);
        let (code, _, stderr) = renvor(&args, base.path(), &[]);
        assert_eq!(code, 0, "generation failed for {flags:?}:\n{stderr}");

        let project = base.path().join("rt");
        let (code, stdout, stderr) = renvor(
            &[
                "check",
                project.to_str().expect("utf-8"),
                "--output",
                "json",
            ],
            base.path(),
            &[],
        );
        assert_eq!(
            code, 0,
            "renvor check rejected renvor's own output for {flags:?}:\n{stderr}"
        );
        let document: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("one JSON document");
        assert_eq!(document["status"], "success", "{flags:?}");

        // And it is valid TOML by an independent parser, not only by our own reader.
        let text = std::fs::read_to_string(project.join("renvor.toml")).expect("read");
        toml::from_str::<toml::Value>(&text)
            .unwrap_or_else(|error| panic!("{flags:?} produced invalid TOML: {error}\n{text}"));
    }
}

#[test]
fn the_non_interactive_path_is_reachable_without_a_terminal_and_never_blocks() {
    // FR-010. `cargo test` gives us no terminal, which is exactly the condition under test: the
    // command must complete rather than wait for input that can never arrive.
    //
    // The stronger half of SC-003 — that a wizard run and a flag run serialise byte-identically —
    // is structural rather than tested here: `prompts::fill` returns `Answers`, the same type the
    // flag parser produces, and `ProjectConfiguration::resolve` is the only constructor. Driving a
    // real wizard needs a pseudo-terminal harness, which is task T042 and is NOT done. This test
    // does not claim to discharge it.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, _, stderr) = renvor(
        &[
            "new",
            "flags",
            "--yes",
            "--local-domain",
            "flags.test",
            "--example-domain",
        ],
        base.path(),
        &[],
    );
    assert_eq!(code, 0, "{stderr}");
    let manifest = std::fs::read_to_string(base.path().join("flags/renvor.toml")).expect("read");
    assert!(
        manifest.contains("local_domain = \"flags.test\""),
        "{manifest}"
    );
    assert!(manifest.contains("example_domain = true"), "{manifest}");
    // Lowercase, deliberately asserted: `True` is what MiniJinja produces by default and is
    // invalid TOML.
    assert!(
        !manifest.contains("True"),
        "the manifest is not valid TOML:\n{manifest}"
    );
}

#[test]
fn defaulted_and_explicitly_supplied_answers_produce_byte_identical_projects() {
    // SC-003, measured from OUTSIDE the process — as close to "the wizard and the flags agree" as
    // is reachable without a pseudo-terminal.
    //
    // The left-hand run supplies nothing and lets `resolve` derive the name and the local domain.
    // The right-hand run supplies **exactly what the wizard would offer as its defaults**. If the
    // two derivations ever diverge — which they did until 2026-08-18, when the rule lived in two
    // non-equivalent copies — these trees stop matching.
    //
    // What this does NOT cover: that a real wizard run produces those answers. That needs a PTY
    // harness (T042) and is not built.
    let base = tempfile::tempdir().expect("tempdir");
    let left = base.path().join("left");
    let right = base.path().join("right");
    std::fs::create_dir_all(&left).expect("mkdir");
    std::fs::create_dir_all(&right).expect("mkdir");

    let (code, _, stderr) = renvor(&["new", "demo", "--yes", "--example-domain"], &left, &[]);
    assert_eq!(code, 0, "the defaulted run failed:\n{stderr}");

    let (code, _, stderr) = renvor(
        // `demo.test` is what `derive_local_domain("demo")` returns and what the wizard offers.
        &[
            "new",
            "demo",
            "--yes",
            "--example-domain",
            "--local-domain",
            "demo.test",
        ],
        &right,
        &[],
    );
    assert_eq!(code, 0, "the explicit run failed:\n{stderr}");

    let read = |root: &Path| -> Vec<(String, Vec<u8>)> {
        let mut files = Vec::new();
        let mut stack = vec![root.join("demo")];
        while let Some(directory) = stack.pop() {
            for entry in std::fs::read_dir(&directory).expect("read_dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let relative = path
                        .strip_prefix(root.join("demo"))
                        .expect("relative")
                        .to_string_lossy()
                        .replace('\\', "/");
                    files.push((relative, std::fs::read(&path).expect("read")));
                }
            }
        }
        files.sort();
        files
    };

    let left_files = read(&left);
    let right_files = read(&right);
    assert!(!left_files.is_empty(), "nothing was generated");
    assert_eq!(
        left_files.len(),
        right_files.len(),
        "the two runs produced different file counts"
    );
    for ((left_path, left_bytes), (right_path, right_bytes)) in
        left_files.iter().zip(right_files.iter())
    {
        assert_eq!(
            left_path, right_path,
            "the two runs produced different paths"
        );
        assert_eq!(
            left_bytes, right_bytes,
            "`{left_path}` differs between a defaulted run and an explicitly-answered one, so the \
             two interfaces do not resolve to the same configuration"
        );
    }
}

#[test]
fn a_different_answer_really_does_produce_a_different_project() {
    // POSITIVE CONTROL for the parity test above. If generation ignored `--local-domain`, the
    // comparison would pass while proving nothing at all.
    let base = tempfile::tempdir().expect("tempdir");
    let left = base.path().join("left");
    let right = base.path().join("right");
    std::fs::create_dir_all(&left).expect("mkdir");
    std::fs::create_dir_all(&right).expect("mkdir");

    renvor(&["new", "demo", "--yes"], &left, &[]);
    renvor(
        &["new", "demo", "--yes", "--local-domain", "elsewhere.test"],
        &right,
        &[],
    );

    let left_manifest = std::fs::read_to_string(left.join("demo/renvor.toml")).expect("read");
    let right_manifest = std::fs::read_to_string(right.join("demo/renvor.toml")).expect("read");
    assert_ne!(
        left_manifest, right_manifest,
        "a different answer produced an identical project, so the parity test above compares \
         nothing"
    );
    assert!(
        right_manifest.contains("elsewhere.test"),
        "{right_manifest}"
    );
}

#[test]
fn the_global_dry_run_flag_reaches_every_command_that_can_change_anything() {
    // THE REGRESSION TEST. `--dry-run` is declared global, but only `new` received it, so
    // `renvor docker up --dry-run` started containers and `renvor dev --dry-run` ran the build.
    //
    // Asserted from outside, because the defect was in the **wiring** — every command's own logic
    // was fine, and `main` simply never passed the flag along. A unit test on the command could
    // not have caught it.
    let base = tempfile::tempdir().expect("tempdir");
    std::fs::write(base.path().join("compose.yaml"), b"services: {}\n").expect("write");
    std::fs::write(base.path().join("renvor.toml"), b"[renvor]\n").expect("write");

    for command in [&["docker", "up"][..], &["dev"][..]] {
        let mut args = command.to_vec();
        args.extend_from_slice(&["--dry-run", "--output", "json"]);
        let (code, stdout, stderr) = renvor(&args, base.path(), &[]);
        assert_eq!(code, 0, "{command:?} --dry-run failed:\n{stderr}");
        let document: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("one JSON document");
        assert_eq!(
            document["result"]["dryRun"], true,
            "{command:?} did not report itself as a dry run, so the flag did not reach it"
        );
        assert!(
            document["result"]["wouldRun"].is_array(),
            "{command:?} did not say what it WOULD have run, which is the whole point"
        );
    }

    // And nothing happened.
    assert!(
        !base.path().join("target").exists(),
        "a dry run built something"
    );
}

#[test]
fn a_panic_exits_one_and_still_emits_exactly_one_json_document() {
    // TWO CONTRACTS, BOTH VIOLATED BY RUST'S DEFAULTS UNTIL 2026-08-18.
    //
    // C-1 reserves exit `1` for "unclassified or internal failure — a panic". Rust exits **101**,
    // measured with a bare `fn main() { panic!() }`, so the reservation was a fiction.
    //
    // C-2 requires exactly one JSON document "for success and for failure alike. Not zero on
    // failure". A panic wrote **nothing** to stdout, so a JSON consumer got nothing to parse at
    // precisely the moment it most needed an answer.
    //
    // `RENVOR_PANIC_FOR_TESTS` exists only under `debug_assertions`, so this path cannot exist in
    // a release binary.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, stdout, stderr) = renvor(
        &["doctor", "--output", "json"],
        base.path(),
        &[("RENVOR_PANIC_FOR_TESTS", "1")],
    );
    assert_eq!(code, 1, "a panic must exit 1, not Rust's default 101");

    let document: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|error| {
        panic!("a panicking JSON run wrote no parseable document: {error}")
    });
    assert_eq!(document["status"], "failure");
    assert_eq!(document["error"]["code"], "internal");

    // The unstructured detail belongs on stderr, which is not a contract surface. The envelope's
    // message must NOT carry the panic text, which can contain arbitrary values.
    assert!(
        !document["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("deliberate panic"),
        "the panic message reached the structured envelope"
    );
    assert!(
        stderr.contains("deliberate panic"),
        "the detail did not reach stderr"
    );
}

#[test]
fn a_panic_in_human_mode_also_exits_one_and_leaves_stdout_empty() {
    // The other half of stream discipline: in human mode `stdout` carries the result and nothing
    // else, so a failure must leave it empty rather than printing a panic there.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, stdout, _) = renvor(&["doctor"], base.path(), &[("RENVOR_PANIC_FOR_TESTS", "1")]);
    assert_eq!(code, 1);
    assert!(
        stdout.trim().is_empty(),
        "a panic wrote to stdout in human mode: {stdout}"
    );
}

#[test]
fn a_malformed_command_line_still_produces_one_json_document() {
    // C-2 names this exact failure mode:
    //
    //   "A command that fails by printing an unstructured error and exiting has broken this
    //    contract, because the consumer that asked for JSON receives something it cannot parse
    //    precisely when it most needs to know what went wrong."
    //
    // That was renvor's behaviour until 2026-08-18: clap printed prose and exited **before** any
    // of our code ran, so `--output json` produced ZERO documents on a malformed invocation.
    let base = tempfile::tempdir().expect("tempdir");
    for args in [
        &["new", "demo", "--nonsense", "--output", "json"][..],
        &["nosuchcommand", "--output", "json"][..],
        &["new", "--output=json", "--nonsense"][..],
    ] {
        let (code, stdout, _) = renvor(args, base.path(), &[]);
        assert_eq!(code, 2, "{args:?} must be a usage error");
        let document: serde_json::Value = serde_json::from_str(stdout.trim())
            .unwrap_or_else(|error| panic!("{args:?} wrote no parseable document: {error}"));
        assert_eq!(document["status"], "failure", "{args:?}");
        assert_eq!(document["error"]["code"], "usage", "{args:?}");
    }
}

#[test]
fn help_and_version_are_successes_and_still_go_to_stdout() {
    // POSITIVE CONTROL for the change above. clap delivers `--help` and `--version` through the
    // same `Err` path as a malformed command line, so a fix that treated every `Err` as a usage
    // error would turn `renvor --help` into exit 2 — breaking the thing the fix was protecting.
    let base = tempfile::tempdir().expect("tempdir");
    for args in [&["--help"][..], &["--version"][..], &["new", "--help"][..]] {
        let (code, stdout, _) = renvor(args, base.path(), &[]);
        assert_eq!(code, 0, "{args:?} must exit 0");
        assert!(
            !stdout.trim().is_empty(),
            "{args:?} wrote nothing to stdout"
        );
    }
}

#[test]
fn a_malformed_command_line_in_human_mode_keeps_claps_own_diagnostics() {
    // The other control: the JSON path must not have been bought by degrading the human one.
    // clap's caret diagnostics and suggestions are the useful part of its error.
    let base = tempfile::tempdir().expect("tempdir");
    let (code, stdout, stderr) = renvor(&["new", "demo", "--nonsense"], base.path(), &[]);
    assert_eq!(code, 2);
    assert!(
        stdout.trim().is_empty(),
        "human-mode errors must not touch stdout"
    );
    assert!(stderr.contains("unexpected argument"), "{stderr}");
    assert!(
        stderr.contains("tip:"),
        "clap's suggestion was lost:\n{stderr}"
    );
}

#[test]
fn a_closed_stdout_is_not_a_panic() {
    // C-1: "A closed `stdout` (`| head -1`) MUST NOT produce a panic; it exits `0` if the result
    // was already written, and otherwise reports the write failure."
    //
    // Written with `Stdio::piped()` and an immediately-dropped read handle rather than a shell
    // pipeline, because `head` is not reliably present on the Windows runner and this property is
    // not Unix-specific — `println!` panics on a broken pipe everywhere.
    let base = tempfile::tempdir().expect("tempdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_renvor"))
        .args(["doctor", "--output", "json"])
        .current_dir(base.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawns");

    // Close the read end before the child gets a chance to write. Its first write then fails with
    // `BrokenPipe`, which is the case under test.
    drop(child.stdout.take().expect("a piped stdout"));

    let status = child.wait().expect("waits");
    assert!(
        status.success(),
        "a closed stdout produced exit {:?} instead of 0",
        status.code()
    );
    // A Rust panic would be 101 — or 1 with our hook — so `success()` covers both regressions.
    //
    // DEMONSTRATED FAILING on 2026-08-18: removing the `BrokenPipe` arm from
    // `Reporter::write_stdout` makes this exit 1 instead of 0. A gate nobody has watched fail is a
    // gate nobody knows works.
}

#[test]
fn concurrent_runs_at_one_destination_produce_one_project_and_no_corruption() {
    // FR-015: "Concurrent runs targeting the same destination MUST NOT interleave into a corrupt
    // state." Asserted by actually racing them, because the property is about timing and no
    // amount of reading the code establishes it.
    //
    // The design's answer: each run stages under a name carrying its own process id, and
    // placement is a rename onto a path that must not exist. So the losers fail cleanly rather
    // than merging into the winner's tree.
    //
    // ── THIS TEST FOUND A REAL DEFECT THAT LOCAL RUNS COULD NOT ────────────────────────
    //
    // Five clean local runs, then **macOS and Windows CI both failed**: a loser reported
    // `placement_failed` rather than `destination_not_empty`. The cause is the window between the
    // pre-rename check and the rename itself — narrow enough that a fast, idle machine never lands
    // in it, and wide enough that a loaded CI runner does.
    //
    // Losing a race is `destination_not_empty`. `placement_failed` says the move mechanism broke,
    // which sends an operator to debug their filesystem when a second `renvor new` simply beat
    // them to it. `Staging::place` now re-stats after a failed rename to tell those apart.
    //
    // **A local pass does not verify this.** Twelve runs rather than six, to widen the odds, but
    // the platform matrix is what actually exercises it — which is worth remembering before
    // anybody trims this test for being slow.
    let base = tempfile::tempdir().expect("tempdir");
    let destination = base.path().join("contended");

    let mut children = Vec::new();
    for _ in 0..12 {
        children.push(
            Command::new(env!("CARGO_BIN_EXE_renvor"))
                .args([
                    "new",
                    "contended",
                    "--yes",
                    "--example-domain",
                    "--output",
                    "json",
                ])
                .current_dir(base.path())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawns"),
        );
    }

    let mut succeeded = 0;
    let mut failed_cleanly = 0;
    for child in children {
        let output = child.wait_with_output().expect("waits");
        match output.status.code() {
            Some(0) => succeeded += 1,
            Some(3) => {
                // A loser must still honour C-2: exactly one parseable document naming a stable
                // code. "It failed somehow" is not an acceptable answer to a consumer.
                let document: serde_json::Value =
                    serde_json::from_slice(&output.stdout).expect("a loser wrote no document");
                assert_eq!(document["status"], "failure");
                assert_eq!(
                    document["error"]["code"], "destination_not_empty",
                    "a loser failed for an unexpected reason"
                );
                failed_cleanly += 1;
            }
            other => {
                panic!("a concurrent run exited {other:?}, which is neither a win nor a clean loss")
            }
        }
    }

    assert_eq!(succeeded, 1, "exactly one run must win");
    assert_eq!(failed_cleanly, 11, "every other run must lose cleanly");

    // THE PROJECT MUST BE WHOLE, not a merge of six renders. Its own checks are the strongest
    // available statement of that, and they are what a corrupted tree would fail.
    let (code, _, stderr) = renvor(
        &["check", destination.to_str().expect("utf-8")],
        base.path(),
        &[],
    );
    assert_eq!(code, 0, "the surviving project is not valid:\n{stderr}");
    assert!(
        destination.join("src/domain.rs").is_file(),
        "the survivor is incomplete"
    );

    // AND NO RESIDUE. A staging directory left behind by a loser is not corruption, but it is
    // litter in the operator's working directory and the design promises it will not happen.
    let residue: Vec<String> = std::fs::read_dir(base.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".renvor-staging-"))
        .collect();
    assert!(
        residue.is_empty(),
        "concurrent losers left staging behind: {residue:?}"
    );
}

#[test]
fn a_bound_exceeded_failure_carries_its_bound_and_limit_all_the_way_out() {
    // C-2's registry says `bound_exceeded` carries `details.bound` and `details.limit`. Every
    // bound in this program is unit-tested, and until now **none was verified end to end** — so
    // the registry's promise about the wire format rested on reading the code.
    //
    // The manifest-size bound is the one reachable from outside without a pathological template,
    // because `renvor check` reads a file the operator names.
    let base = tempfile::tempdir().expect("tempdir");
    std::fs::write(base.path().join("renvor.toml"), "x".repeat(70 * 1024)).expect("write");

    let (code, stdout, _) = renvor(&["check", ".", "--output", "json"], base.path(), &[]);
    assert_eq!(code, 3, "a bound violation is a validation failure");

    let document: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("one JSON document");
    assert_eq!(document["error"]["code"], "bound_exceeded");
    assert_eq!(
        document["error"]["details"]["bound"], "manifest_bytes",
        "the failure must name WHICH bound was hit"
    );
    assert_eq!(
        document["error"]["details"]["limit"], "65536",
        "the failure must name the ceiling, or the operator cannot act on it"
    );
}

#[test]
fn a_file_just_under_the_bound_is_still_read() {
    // POSITIVE CONTROL. A `check` that rejected every manifest would satisfy the test above and be
    // useless — and an off-by-one in the bound would be invisible without this.
    let base = tempfile::tempdir().expect("tempdir");
    // Valid TOML, comfortably under 64 KiB, padded with a long comment.
    let padding = "#".repeat(60 * 1024);
    let manifest = format!(
        "{padding}\n[renvor]\ngenerator_version = \"0\"\ntemplate_version = \"1\"\n\n         [project]\nname = \"padded\"\ntarget = \"api\"\nlocal_domain = \"padded.test\"\n"
    );
    assert!(
        manifest.len() < 65_536,
        "the fixture must be under the bound to test it"
    );
    std::fs::write(base.path().join("renvor.toml"), &manifest).expect("write");

    let (code, _, stderr) = renvor(&["check", "."], base.path(), &[]);
    assert_eq!(
        code, 0,
        "a manifest under the bound was rejected:\n{stderr}"
    );
}

#[test]
fn an_unsupported_combination_fails_before_anything_is_created() {
    // "unsupported combinations fail before filesystem writes."
    let base = tempfile::tempdir().expect("tempdir");
    let (code, stdout, _) = renvor(
        &["new", "bad", "--yes", "--seed-data", "--output", "json"],
        base.path(),
        &[],
    );
    assert_eq!(code, 3);
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).expect("one document");
    assert_eq!(document["error"]["code"], "unsupported_combination");
    assert!(
        !base.path().join("bad").exists(),
        "the destination was created anyway"
    );
    let residue: Vec<String> = std::fs::read_dir(base.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(residue.is_empty(), "a refused combination left {residue:?}");
}
