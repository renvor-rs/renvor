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
