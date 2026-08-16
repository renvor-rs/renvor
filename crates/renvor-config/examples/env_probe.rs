//! Behavioural probe for configuration proof-gate obligations 5 and 6.
//!
//! Runs in a CHILD PROCESS on purpose. Both obligations are about how the environment is read,
//! and mutating the environment in-process is unsound under a multi-threaded test harness —
//! `std::env::set_var` is `unsafe` in edition 2024, and this workspace declares
//! `unsafe_code = "forbid"`. Inheriting a deliberately-set environment from the parent is the
//! only sound way to observe the behaviour.
//!
//! Driven by `RENVOR_PROOF_MODE`:
//!
//! * `expect-error` — exits 0 if loading FAILED (obligation 5: invalid non-empty value).
//! * `expect-fallthrough` — exits 0 if loading SUCCEEDED with the default (obligation 6: an
//!   invalid EMPTY value is silently treated as unset).

use confique::Config;

#[derive(Debug, Config)]
struct EnvProbe {
    #[config(env = "RENVOR_PROOF_THREADS", default = 7)]
    threads: u16,
}

fn main() -> std::process::ExitCode {
    let mode = std::env::var("RENVOR_PROOF_MODE").unwrap_or_default();
    let loaded = EnvProbe::builder().env().load();

    match (mode.as_str(), &loaded) {
        ("expect-error", Err(e)) => {
            println!("obligation 5 OBSERVED: decode failed as required: {e}");
            std::process::ExitCode::SUCCESS
        }
        ("expect-error", Ok(v)) => {
            eprintln!("obligation 5 VIOLATED: expected an error, got {v:?}");
            std::process::ExitCode::FAILURE
        }
        ("expect-fallthrough", Ok(v)) if v.threads == 7 => {
            println!(
                "obligation 6 OBSERVED: an empty, undecodable value silently became the \
                 default ({}), rather than failing",
                v.threads
            );
            std::process::ExitCode::SUCCESS
        }
        ("expect-fallthrough", Ok(v)) => {
            eprintln!("obligation 6: unexpected value {v:?}");
            std::process::ExitCode::FAILURE
        }
        ("expect-fallthrough", Err(e)) => {
            eprintln!("obligation 6 would be MET (the empty value failed): {e}");
            std::process::ExitCode::FAILURE
        }
        _ => {
            eprintln!("set RENVOR_PROOF_MODE to expect-error or expect-fallthrough");
            std::process::ExitCode::FAILURE
        }
    }
}
