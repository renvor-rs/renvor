//! T046 / FR-065 — an untrusted peer cannot forge the network identity an abuse control counts.
//!
//! # Why this test needs two crates
//!
//! The claim spans a boundary, so it cannot be made on either side of it.
//!
//! `renvor_http::identity::resolve` decides who is asking. It knows about `Forwarded`,
//! `X-Forwarded-For` and [`TrustedProxies`], and it is thoroughly tested — in `identity/mod.rs`,
//! against every hostile header shape. What those tests cannot show is what happens **next**.
//!
//! `renvor_auth::abuse` counts a network dimension. It knows about buckets and windows, and it is
//! thoroughly tested — in `abuse.rs`, against 100_000 identifiers. What those tests cannot show is
//! where the address came from, because `renvor-auth` has no header parser to fool.
//!
//! FR-065 is that the trusted-proxy handling is **reused rather than reimplemented**, and the way
//! to demonstrate reuse is to run the real resolver and feed its real output to the real counter.
//! That is this file, and it is the whole of it.
//!
//! # What `renvor-auth` does not contain
//!
//! No `Forwarded` parser. No `X-Forwarded-For` parser. No trust list. No constructor that takes a
//! header. The crate names the header nowhere, which is asserted below by scanning its source
//! rather than by anyone remembering.

use renvor_auth::abuse::{AttemptBuckets, AttemptDimension, AttemptKey, AttemptKeyring};
use renvor_core::identity::ClientIdentity;
use renvor_http::TrustedProxies;
use renvor_http::identity::{ForwardingHeaders, resolve};
use std::net::{IpAddr, Ipv4Addr};

fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
}

fn keyring() -> AttemptKeyring {
    AttemptKeyring::from_bytes([0x11; 32], AttemptBuckets::default())
}

/// The bucket an identity would be counted in, through the real mapping.
fn bucket_of(identity: ClientIdentity) -> u32 {
    keyring()
        .bucket(
            AttemptDimension::LogInNetwork,
            AttemptKey::Network(identity),
        )
        .expect("a network key on a network dimension")
        .get()
}

#[test]
fn a_hostile_forwarding_header_cannot_move_an_attacker_into_another_counter() {
    // THE ATTACK. An attacker sends a forwarding header naming a victim's address, hoping either
    // to spend the victim's budget or to escape their own.
    let attacker = v4(203, 0, 113, 9);
    let claimed = v4(198, 51, 100, 7);

    let forwarded = ["for=198.51.100.7;host=evil.example"];
    let xff = ["198.51.100.7"];
    let identity = resolve(
        attacker,
        // The DEFAULT configuration: no proxy is trusted.
        &TrustedProxies::default(),
        ForwardingHeaders {
            forwarded: &forwarded,
            x_forwarded_for: &xff,
        },
    );

    // The resolver attributes the socket, and the counter counts what the resolver said.
    assert_eq!(identity, ClientIdentity::DirectPeer(attacker));
    assert_eq!(
        bucket_of(identity),
        bucket_of(ClientIdentity::DirectPeer(attacker)),
        "the attacker was counted somewhere other than their own address"
    );
    assert_ne!(
        bucket_of(identity),
        bucket_of(ClientIdentity::DirectPeer(claimed)),
        "a header moved an attacker into the victim's counter"
    );
}

#[test]
fn a_trusted_proxy_can_still_attribute_a_client_so_the_refusal_above_is_about_trust() {
    // THE POSITIVE CONTROL, and it is not optional: without it, "the header was ignored" is also
    // what a resolver that ignores every header always does — including a broken one.
    let proxy = v4(10, 0, 0, 1);
    let client = v4(198, 51, 100, 7);

    let forwarded = ["for=198.51.100.7"];
    let identity = resolve(
        proxy,
        &TrustedProxies::none().trust(proxy),
        ForwardingHeaders {
            forwarded: &forwarded,
            x_forwarded_for: &[],
        },
    );

    assert_eq!(
        identity,
        ClientIdentity::ViaTrustedProxy { client, proxy },
        "a trusted proxy's attribution was discarded"
    );
    // And the counter follows the attribution, not the socket — which is what makes a load
    // balancer's own address not become every user's bucket.
    assert_eq!(
        bucket_of(identity),
        bucket_of(ClientIdentity::DirectPeer(client))
    );
    assert_ne!(
        bucket_of(identity),
        bucket_of(ClientIdentity::DirectPeer(proxy))
    );
}

#[test]
fn two_clients_behind_one_trusted_proxy_are_counted_separately() {
    // If the proxy's address were counted, one noisy client behind a load balancer would exhaust
    // the budget for everyone behind it.
    let proxy = v4(10, 0, 0, 1);
    let trusted = TrustedProxies::none().trust(proxy);

    let first = ["for=198.51.100.7"];
    let second = ["for=198.51.100.8"];
    let one = resolve(
        proxy,
        &trusted,
        ForwardingHeaders {
            forwarded: &first,
            x_forwarded_for: &[],
        },
    );
    let other = resolve(
        proxy,
        &trusted,
        ForwardingHeaders {
            forwarded: &second,
            x_forwarded_for: &[],
        },
    );
    assert_ne!(bucket_of(one), bucket_of(other));
}

#[test]
fn a_trusted_proxy_that_sends_nothing_parseable_falls_back_to_a_truthful_counter() {
    // Fail-closed, carried through to the counter: an unparseable claim is attributed to the peer
    // that made it, not discarded into some default bucket.
    let proxy = v4(10, 0, 0, 1);
    let malformed = ["for=\"not an address\""];
    let identity = resolve(
        proxy,
        &TrustedProxies::none().trust(proxy),
        ForwardingHeaders {
            forwarded: &malformed,
            x_forwarded_for: &[],
        },
    );
    assert_eq!(identity, ClientIdentity::DirectPeer(proxy));
    assert_eq!(
        bucket_of(identity),
        bucket_of(ClientIdentity::DirectPeer(proxy))
    );
}

#[test]
fn renvor_auth_contains_no_forwarding_header_parser() {
    // FR-065 says the handling is REUSED rather than reimplemented. The way that stops being true
    // is somebody adding a convenience parser to `renvor-auth` because passing a resolved identity
    // through three layers felt like work. This scan is what notices.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("renvor-auth")
        .join("src");

    let mut scanned = 0_usize;
    let mut stack = vec![root.clone()];
    while let Some(path) = stack.pop() {
        let entries = std::fs::read_dir(&path).expect("the auth source tree is readable");
        for entry in entries {
            let entry = entry.expect("a readable entry");
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("a readable file");
            scanned += 1;
            for forbidden in [
                "X-Forwarded-For",
                "x-forwarded-for",
                "Forwarded:",
                "TrustedProxies",
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} names {forbidden:?} — the trusted-proxy handling is being reimplemented",
                    path.display()
                );
            }
        }
    }

    // POSITIVE CONTROL: the walk actually read the tree. An empty scan would pass every assertion
    // above and prove nothing — which is the failure mode this repository has already been bitten
    // by twice.
    assert!(
        scanned >= 10,
        "the scan read only {scanned} files, so its absences describe an empty walk"
    );
    assert!(
        std::fs::read_to_string(root.join("abuse.rs"))
            .expect("abuse.rs is readable")
            .contains("ClientIdentity"),
        "the scan is not looking at the module that counts a network dimension"
    );

    // The scan deliberately does NOT forbid the string "renvor_http" in `renvor-auth`'s source.
    //
    // The first version did, and it failed — on a doc comment in `audit.rs` explaining which
    // transport type feeds `CorrelationId::parse`. A cross-crate doc link is not a dependency, and
    // forbidding the mention would have pushed the explanation out of the file that needs it.
    //
    // The thing that actually must not happen is `renvor-auth` DEPENDING on the transport, and
    // that is a fact about the manifest rather than about prose. So it is asserted there.
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("renvor-auth")
            .join("Cargo.toml"),
    )
    .expect("the auth manifest is readable");

    // COMMENT LINES ARE STRIPPED, and the reason is that the first two versions of this check
    // failed on prose. `renvor-auth`'s manifest header explains at length why putting
    // authorization in `renvor-http` would make "who may do what" a fact about HTTP — which is the
    // rule being enforced, written down, in the file being scanned.
    //
    // A dependency declaration is never a comment line, so this is exact rather than approximate.
    let declarations: String = manifest
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !declarations.contains("renvor-http"),
        "renvor-auth declares a dependency on the transport"
    );

    // POSITIVE CONTROL: the stripped text still contains the dependencies that ARE declared, so
    // the absence above is not the absence of everything.
    assert!(
        declarations.contains("renvor-core") && declarations.contains("renvor-database"),
        "the manifest scan stripped the dependency table along with the comments"
    );
}
