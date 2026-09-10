//! FR-043 and SC-011: every local flow completes with networking unavailable.
//!
//! # What "unavailable" means here, stated precisely rather than implied
//!
//! Three independent measures are applied to every run below, and each blocks a different route:
//!
//! 1. **Every proxy variable** cargo, curl, and the common Rust HTTP clients honour is pointed at
//!    `http://127.0.0.1:1` — a port nothing listens on — so a proxied request fails at once instead
//!    of succeeding quietly from a warm cache.
//! 2. **`CARGO_NET_OFFLINE=true`**, so the `cargo build` and `cargo test` that pre-placement
//!    verification runs will *refuse* to touch the network rather than merely failing to reach it.
//!    This is the measure that turns "no network was reached" into "no network was permitted".
//! 3. **`RUSTUP_TOOLCHAIN` is left alone but `CARGO_NET_RETRY=0`**, so a would-be fetch fails on
//!    the first attempt rather than retrying into the test's own timeout and looking like a hang.
//!
//! # What this does NOT demonstrate, said plainly
//!
//! A direct connection that ignores proxy variables would not be blocked by any of the above.
//! Demonstrating *that* requires a network namespace (`unshare -rn`), which exists on Linux, needs
//! privileges, and has no equivalent on the macOS and Windows legs this suite also runs on. Rather
//! than run a weaker test on two platforms and a stronger one on a third — and then describe the
//! whole thing by its strongest leg — the limit is stated here.
//!
//! What closes the gap is **structural** and lives in `tests/capabilities.rs`: the executable's
//! resolved dependency closure contains no HTTP client at all, with a negative control proving the
//! walk can see crates that are present. A binary that cannot reach `reqwest`, `hyper`, `ureq`,
//! `curl`, `isahc`, `attohttpc`, `surf`, `http-client`, or `native-tls` has no ordinary way to open
//! a connection, proxied or not. The two together are the evidence; neither alone is.
//!
//! And the generated project **declares no dependencies**, so there is no registry for cargo to
//! resolve against even if it were allowed to try — asserted below rather than assumed.

mod harness;

use harness::renvor;

/// The environment every test in this file runs under.
fn offline() -> Vec<(&'static str, &'static str)> {
    let blackhole = "http://127.0.0.1:1";
    vec![
        ("http_proxy", blackhole),
        ("https_proxy", blackhole),
        ("HTTP_PROXY", blackhole),
        ("HTTPS_PROXY", blackhole),
        ("all_proxy", blackhole),
        ("ALL_PROXY", blackhole),
        ("ftp_proxy", blackhole),
        // Emptied, not unset: a populated `no_proxy` would exempt the very hosts under test.
        ("no_proxy", ""),
        ("NO_PROXY", ""),
        // The measure that makes this a refusal rather than a failed attempt.
        ("CARGO_NET_OFFLINE", "true"),
        ("CARGO_NET_RETRY", "0"),
    ]
}

#[test]
fn every_generated_variant_is_produced_with_networking_unavailable() {
    // The heaviest local flow, and the only one that runs subprocesses: `renvor new` shells out to
    // `cargo fmt`, `cargo build`, and `cargo test` for pre-placement verification. If anything in
    // this phase needed the network, this is where it would show.
    let base = tempfile::tempdir().expect("a temporary directory");
    for variant in [
        vec!["new", "plain", "--yes"],
        vec!["new", "domain", "--yes", "--example-domain"],
        vec!["new", "seeded", "--yes", "--example-domain", "--seed-data"],
        vec!["new", "boxed", "--yes", "--container"],
        vec!["new", "secured", "--yes", "--local-https"],
    ] {
        let name = variant[1];
        let (exit, _, stderr) = renvor(&variant, base.path(), &offline());
        assert_eq!(exit, 0, "{variant:?} needed the network:\n{stderr}");
        assert!(
            base.path().join(name).join("renvor.toml").is_file(),
            "{variant:?}"
        );
    }
}

#[test]
fn the_generated_project_declares_no_dependencies_so_there_is_nothing_to_resolve() {
    // The structural reason the test above can pass at all, asserted rather than assumed. If a
    // template ever gains a dependency, `CARGO_NET_OFFLINE=true` makes that a loud failure — but
    // only if the manifest is what we think it is, and only this checks that.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, _, stderr) = renvor(
        &["new", "demo", "--yes", "--example-domain"],
        base.path(),
        &offline(),
    );
    assert_eq!(exit, 0, "{stderr}");

    let manifest = std::fs::read_to_string(base.path().join("demo/Cargo.toml")).expect("readable");
    let after_dependencies = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("the generated manifest has a [dependencies] section");
    let declared: Vec<&str> = after_dependencies
        .lines()
        .map(str::trim)
        .take_while(|line| !line.starts_with('['))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    assert!(
        declared.is_empty(),
        "the generated project declares dependencies, so FR-043's offline guarantee now rests on a \
         registry cache rather than on there being nothing to fetch: {declared:?}"
    );

    // And the lockfile pre-placement verification produced agrees.
    let lock = std::fs::read_to_string(base.path().join("demo/Cargo.lock")).expect("readable");
    assert_eq!(
        lock.matches("[[package]]").count(),
        1,
        "the resolved graph is more than the project itself:\n{lock}"
    );
}

#[test]
fn every_other_command_completes_with_networking_unavailable() {
    // The remaining local flows. `dev` and `docker` run under `--dry-run` because their non-dry
    // forms start a build loop and a container runtime respectively — neither of which is a
    // *local flow this phase claims to complete*, and pretending otherwise would make this test
    // depend on Docker being installed on every matrix leg.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, _, stderr) = renvor(&["new", "demo", "--yes"], base.path(), &offline());
    assert_eq!(exit, 0, "{stderr}");
    let project = base.path().join("demo");

    // The `docker` commands validate that the project actually has container controls before
    // anything else, so they need a project generated with `--container`. That refusal is correct
    // behaviour — it names the field and the constraint — and it is not what this test is about.
    let (exit, _, stderr) = renvor(
        &["new", "boxed", "--yes", "--container"],
        base.path(),
        &offline(),
    );
    assert_eq!(exit, 0, "{stderr}");
    let containerised = base.path().join("boxed");

    for (arguments, working_directory) in [
        (vec!["doctor"], base.path().to_path_buf()),
        (
            vec!["doctor", "--output", "json"],
            base.path().to_path_buf(),
        ),
        (vec!["check"], project.clone()),
        (vec!["check", "--output", "json"], project.clone()),
        (vec!["dev", "--dry-run"], project.clone()),
        (vec!["docker", "status", "--dry-run"], containerised.clone()),
        (vec!["docker", "up", "--dry-run"], containerised.clone()),
        (vec!["docker", "down", "--dry-run"], containerised.clone()),
        (vec!["docker", "logs", "--dry-run"], containerised.clone()),
        (vec!["--help"], base.path().to_path_buf()),
        (vec!["--version"], base.path().to_path_buf()),
    ] {
        let (exit, _, stderr) = renvor(&arguments, &working_directory, &offline());
        assert_eq!(
            exit, 0,
            "{arguments:?} did not complete offline (exit {exit}):\n{stderr}"
        );
    }
}

#[test]
fn a_dry_run_generation_also_completes_with_networking_unavailable() {
    // SC-006 makes the dry run produce the same manifest as a real run, which means it renders and
    // verifies too — so it has the same network exposure and needs the same proof.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, stdout, stderr) = renvor(
        &["new", "demo", "--yes", "--dry-run", "--output", "json"],
        base.path(),
        &offline(),
    );
    assert_eq!(exit, 0, "{stderr}");
    let document: serde_json::Value = serde_json::from_str(&stdout).expect("one JSON document");
    assert_eq!(document["status"], "success");
    assert!(
        !base.path().join("demo").exists(),
        "a dry run wrote to the destination"
    );
}

/// FR-006, measured the way it is promised: `CARGO_NET_OFFLINE=true` generation of a starter
/// succeeds when the framework has been built on the machine. The precondition is realised in an
/// **empty** `CARGO_HOME` — never this machine's warm cache — by fetching the framework's own
/// lockfile closure into it, which is what any build of the framework leaves in the registry
/// cache for the crates it built (a fetch covers every feature at once). Everything the starter
/// then resolves must already be there: the framework's `Cargo.lock` seeds resolution, and a
/// package outside that lock is exactly the failure this test exists to catch (the seeded lock was
/// one package short — `signal-hook-registry` — until the correction round of 2026-09-05).
#[test]
fn a_starter_is_generated_with_networking_unavailable_from_the_cache_a_framework_build_leaves() {
    let framework = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root exists");
    let cargo_home = tempfile::tempdir().expect("an empty CARGO_HOME");
    let fetched = std::process::Command::new("cargo")
        .args(["fetch", "--locked"])
        .current_dir(&framework)
        .env("CARGO_HOME", cargo_home.path())
        .output()
        .expect("cargo runs");
    assert!(
        fetched.status.success(),
        "the precondition could not be established (the framework's lock closure fetched into an \
         empty cache):\n{}",
        String::from_utf8_lossy(&fetched.stderr)
    );

    let base = tempfile::tempdir().expect("a temporary directory");
    let target = tempfile::tempdir().expect("a build directory");
    let cargo_home = cargo_home.path().to_str().expect("utf-8").to_owned();
    let target = target.path().to_str().expect("utf-8").to_owned();
    let framework = framework.to_str().expect("utf-8").to_owned();
    let mut env = offline();
    env.push(("CARGO_HOME", cargo_home.as_str()));
    env.push(("CARGO_TARGET_DIR", target.as_str()));
    let (exit, stdout, stderr) = renvor(
        &[
            "new",
            "offline-starter",
            "--capabilities",
            "storage",
            "--framework-path",
            &framework,
            "--output",
            "json",
            "--yes",
        ],
        base.path(),
        &env,
    );
    assert_eq!(
        exit, 0,
        "FR-006: a starter must generate offline from the cache the framework's build left:\n{stderr}\n{stdout}"
    );
    assert!(
        base.path()
            .join("offline-starter")
            .join("Cargo.lock")
            .is_file(),
        "the starter was not placed"
    );
}

// ───────────────────────────── Phase 012: no provisioning is not no network (SR-012-1, SR-012-5)

/// The directory rustup keeps its toolchains in, read the way an operator would read it.
///
/// **Never `rustup toolchain list`.** Asking rustup what is installed runs the proxy, and on a
/// pre-1.28 rustup — or in a pinned directory — the question can itself install what it is asked
/// about (measured 2026-09-07). A control for "nothing was installed" that might install
/// something is not a control. This reads the directory.
fn toolchains_directory() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("RUSTUP_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| std::path::PathBuf::from(home).join(".rustup"))
        })?;
    let toolchains = home.join("toolchains");
    toolchains.is_dir().then_some(toolchains)
}

/// The names under [`toolchains_directory`], sorted — the before-and-after of the control below.
fn installed(toolchains: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(toolchains)
        .expect("the toolchain directory is readable")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// A channel that is plausible, exact, and **not installed here**, chosen by reading the
/// directory.
///
/// Plausible matters: a name rustup would reject as malformed would be refused for the wrong
/// reason, and the test would pass without ever reaching the question it asks. A released
/// version that this machine happens not to have is the case an operator meets.
fn an_absent_channel(toolchains: &std::path::Path) -> Option<String> {
    let present = installed(toolchains);
    ["1.72.0", "1.73.0", "1.74.0", "1.75.0", "1.76.0", "1.77.0"]
        .into_iter()
        .find(|candidate| {
            !present
                .iter()
                .any(|name| name == candidate || name.starts_with(&format!("{candidate}-")))
        })
        .map(str::to_owned)
}

/// What one refusal of an absent pin, offline, with every download address unroutable, produced.
struct Refusal {
    exit: i32,
    document: serde_json::Value,
    stderr: String,
    elapsed: std::time::Duration,
    destination_exists: bool,
    before: Vec<String>,
    after: Vec<String>,
}

/// Runs `renvor new` with `RUSTUP_TOOLCHAIN` naming an absent channel and every download address
/// pointed at a port nothing listens on.
///
/// `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` cover rustup's two download roots; the crates.io
/// index covers cargo's. The seal does **not** forward the two rustup variables (D-L2-3), so they
/// are set in the *test's* process environment and the run below also demonstrates that the
/// refusal does not depend on them reaching the child.
fn refuse_an_absent_pin(channel: &str, toolchains: &std::path::Path) -> Refusal {
    let base = tempfile::tempdir().expect("a temporary directory");
    let unroutable = "http://127.0.0.1:9";
    let mut environment = offline();
    environment.push(("RUSTUP_TOOLCHAIN", channel));
    environment.push(("RUSTUP_DIST_SERVER", unroutable));
    environment.push(("RUSTUP_UPDATE_ROOT", unroutable));
    environment.push((
        "CARGO_REGISTRIES_CRATES_IO_INDEX",
        "sparse+http://127.0.0.1:9/index/",
    ));

    let before = installed(toolchains);
    let started = std::time::Instant::now();
    let (exit, stdout, stderr) = renvor(
        &["new", "absent-pin", "--yes", "--output", "json"],
        base.path(),
        &environment,
    );
    let elapsed = started.elapsed();
    let after = installed(toolchains);

    Refusal {
        exit,
        document: serde_json::from_str(&stdout).unwrap_or_else(|_| panic!("not JSON:\n{stdout}")),
        stderr,
        elapsed,
        destination_exists: base.path().join("absent-pin").exists(),
        before,
        after,
    }
}

#[test]
fn an_offline_generation_with_the_pin_installed_passes_and_records_it() {
    // SR-012-5, the first of the two halves: `RUSTUP_AUTO_INSTALL=0` and an unreachable network
    // change nothing about a generation whose pin is already here — which is every generation on
    // a machine that has the toolchain. The pin IS the running toolchain, so "installed" needs no
    // arranging and no second toolchain.
    let base = tempfile::tempdir().expect("a temporary directory");
    let (exit, _, stderr) = renvor(&["new", "pinned", "--yes"], base.path(), &offline());
    assert_eq!(
        exit, 0,
        "an offline generation with the pin present:\n{stderr}"
    );

    let project = base.path().join("pinned");
    let pin = std::fs::read_to_string(project.join("rust-toolchain.toml"))
        .expect("the generated project carries a pin");
    let channel = pin
        .lines()
        .find_map(|line| line.strip_prefix("channel = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("`rust-toolchain.toml` names a channel");

    let record = std::fs::read_to_string(project.join(".renvor").join("generated.toml"))
        .expect("the provenance record is readable");
    assert!(
        record.contains("[toolchain]"),
        "an offline generation recorded no `[toolchain]`:\n{record}"
    );
    assert!(
        record.contains(&format!("pinned = \"{channel}\"")),
        "the record's pin is not the channel the tree declares ({channel}):\n{record}"
    );
    assert!(
        record.contains("[verified_with]"),
        "an offline generation ran its checks and recorded no evidence:\n{record}"
    );
    assert!(
        record.contains("operation = \"new\""),
        "the evidence does not name the operation that wrote it:\n{record}"
    );
}

#[test]
fn an_offline_generation_with_the_pin_absent_is_tool_missing_and_fetches_nothing() {
    // SR-012-5's other half. `RUSTUP_AUTO_INSTALL=0` stops rustup installing a toolchain; it does
    // not stop cargo fetching crates, and the two are separate promises. This one is: an absent
    // pin is refused BEFORE any check, so no crate fetch is ever reached either — the destination
    // is untouched and there is nothing in it to have resolved.
    let Some(toolchains) = toolchains_directory() else {
        println!(
            "SKIPPED an_offline_generation_with_the_pin_absent_is_tool_missing_and_fetches_\
             nothing: no rustup toolchain directory on this machine"
        );
        return;
    };
    let Some(channel) = an_absent_channel(&toolchains) else {
        println!(
            "SKIPPED an_offline_generation_with_the_pin_absent_is_tool_missing_and_fetches_\
             nothing: every candidate channel is installed here"
        );
        return;
    };

    let refusal = refuse_an_absent_pin(&channel, &toolchains);
    assert_eq!(
        refusal.exit, 5,
        "an absent pin is `tool_missing` (exit 5):\n{}\n{}",
        refusal.document, refusal.stderr
    );
    assert_eq!(
        refusal.document["status"], "failure",
        "{}",
        refusal.document
    );
    assert_eq!(
        refusal.document["error"]["code"], "tool_missing",
        "{}",
        refusal.document
    );
    assert!(
        refusal.document["error"]["details"]["tool"]
            .as_str()
            .is_some_and(|tool| tool.contains(&channel)),
        "the refusal does not name the toolchain it wanted: {}",
        refusal.document
    );
    assert!(
        !refusal.destination_exists,
        "a refusal before any check wrote to the destination"
    );
}

#[test]
fn an_absent_pin_with_an_unroutable_dist_server_is_refused_in_bounded_time_and_installs_nothing() {
    // SR-012-1, the no-provisioning control, and the same run as the test above — stated as its
    // own row because it is its own promise. Nothing about generation or verification may
    // provision a toolchain, so with every download address pointed at a closed port the result
    // must be a refusal BY NAME and not a timeout, and the toolchain directory must be exactly
    // what it was.
    //
    // The listing is read from the filesystem, before and after. `rustup toolchain list` is not
    // used, here or anywhere in this file: see `toolchains_directory`.
    let Some(toolchains) = toolchains_directory() else {
        println!(
            "SKIPPED an_absent_pin_with_an_unroutable_dist_server_is_refused_in_bounded_time_and_\
             installs_nothing: no rustup toolchain directory on this machine"
        );
        return;
    };
    let Some(channel) = an_absent_channel(&toolchains) else {
        println!(
            "SKIPPED an_absent_pin_with_an_unroutable_dist_server_is_refused_in_bounded_time_and_\
             installs_nothing: every candidate channel is installed here"
        );
        return;
    };

    let refusal = refuse_an_absent_pin(&channel, &toolchains);
    assert_eq!(
        refusal.exit, 5,
        "the probe did not refuse by name:\n{}\n{}",
        refusal.document, refusal.stderr
    );
    assert_eq!(
        refusal.document["error"]["code"], "tool_missing",
        "{}",
        refusal.document
    );
    // BOUNDED TIME, not "fast": the point is that nothing waited on a connection to a port that
    // is closed. A download attempt against an unroutable address would retry and stall; a
    // refusal by name answers from what rustup already knows.
    assert!(
        refusal.elapsed < std::time::Duration::from_secs(60),
        "the refusal took {:?}, which is a network attempt rather than an answer by name",
        refusal.elapsed
    );
    assert_eq!(
        refusal.before, refusal.after,
        "the toolchain directory changed during a run that must provision nothing"
    );
    assert!(
        !refusal.after.iter().any(|name| name.starts_with(&channel)),
        "the channel the run asked for was installed by the run: {:?}",
        refusal.after
    );
}
