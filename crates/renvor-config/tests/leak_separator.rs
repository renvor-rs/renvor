//! W-005 security finding 2.1, end to end rather than at the unit boundary.
//!
//! `Constraint::from_decoder` strips the offending value out of a decoder's message. It used
//! `split_once`, which splits at the **first** `", expected "` — so a value containing that
//! literal put the first occurrence inside itself and its tail was copied into the error.
//!
//! The unit tests in `renvor-core` cover the stripping function. This covers the **path an
//! attacker actually controls**: a real environment variable, through the real resolver, to the
//! rendered error a log would capture.

use renvor_config::{ConfigSchema, LayeredResolverBuilder};
use renvor_core::config_port::ConfigResolver as _;
use serde::Deserialize;
use std::collections::BTreeMap;
use toml::Table;

/// Nothing reads `port` back; the schema exists so that decoding it can fail.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct Settings {
    port: u16,
}
#[allow(dead_code)]
#[derive(Debug, Default, Deserialize)]
struct PartialSettings {
    port: Option<u16>,
}
impl ConfigSchema for Settings {
    type Partial = PartialSettings;
}

#[test]
fn a_separator_bearing_value_does_not_leak_through_the_real_stack() {
    // W-005 security finding 2.1, end to end rather than at the unit boundary.
    // Payloads are identified by index rather than by content. Naming the offending string in
    // the diagnostic would mean the one run that proves a leak exists is the run that prints
    // the leaked value into the test log — the same defect this test is here to catch.
    for (payload_index, payload) in [
        "s3cr3t-token-abc123\", expected u16",
        "hunter2, expected LEAKED-TAIL",
    ]
    .into_iter()
    .enumerate()
    {
        let env: BTreeMap<String, String> = [("RENVOR_PORT".to_owned(), payload.to_owned())]
            .into_iter()
            .collect();
        let error = LayeredResolverBuilder::new()
            .with_defaults("port = 80".parse::<Table>().expect("valid"))
            .with_environment_map("RENVOR_", env)
            .build::<Settings>()
            .resolve()
            .expect_err("a string is not a u16");
        let rendered = error.to_string();
        for (needle_index, secret) in ["s3cr3t-token-abc123", "LEAKED-TAIL", "hunter2"]
            .into_iter()
            .enumerate()
        {
            assert!(
                !rendered.contains(secret),
                "payload {payload_index} leaked needle {needle_index} \
                 through the environment layer"
            );
        }
        println!("env  ok: payload {payload_index}");
    }
}
