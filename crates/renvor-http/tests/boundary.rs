//! The transport boundary, asserted mechanically.
//!
//! # What this test is for
//!
//! SC-012: **0** transport types appear in application- or domain-facing interfaces. A handler
//! receives `renvor_http::Request` and returns `renvor_http::Response`, and neither may grow an
//! `axum`, `tower`, or `http` type in its signature.
//!
//! # Why it reads the source rather than the type system
//!
//! A trait bound can express "does not name axum" only by enumerating what it *does* allow, which
//! is a list that goes stale. Reading the source catches the case the type system cannot: a
//! `pub` item added to an application-facing module that happens to name a transport type.
//!
//! The facade's own `the_facade_declares_no_implementation_of_its_own` test uses the same
//! technique for the same reason, and this follows its shape deliberately — including its positive
//! control, without which a scan that found nothing would be indistinguishable from a scan that
//! did not work.

/// Modules whose public surface an application author touches.
///
/// `route/build.rs` and `server.rs` are deliberately **absent**: they are where the transport
/// lives, and requiring them to be transport-free would be requiring them not to exist.
const APPLICATION_FACING: [(&str, &str); 4] = [
    ("route/mod.rs", include_str!("../src/route/mod.rs")),
    ("context.rs", include_str!("../src/context.rs")),
    ("error.rs", include_str!("../src/error.rs")),
    (
        "route/registry.rs",
        include_str!("../src/route/registry.rs"),
    ),
];

/// The transport crate names that must not appear.
const TRANSPORT_IDENTIFIERS: [&str; 4] = ["axum", "tower_http", "tower::", "hyper"];

/// Finds transport identifiers in real code, ignoring documentation.
///
/// Documentation is excluded on purpose: these modules **explain** why they do not use the
/// transport types, and a scan that flagged the explanation would force the explanation to be
/// deleted — which is the opposite of what this test is protecting.
fn transport_identifiers_in_code(source: &str) -> Vec<String> {
    source
        .lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("//"))
        .filter(|line| {
            TRANSPORT_IDENTIFIERS
                .iter()
                .any(|identifier| line.contains(identifier))
        })
        .map(str::to_owned)
        .collect()
}

#[test]
fn no_transport_type_appears_in_an_application_facing_module() {
    for (name, source) in APPLICATION_FACING {
        let found = transport_identifiers_in_code(source);
        assert!(
            found.is_empty(),
            "{name} names a transport type in code, which would put it in an \
             application-facing signature: {found:?}"
        );
    }
}

#[test]
fn the_scan_finds_a_transport_type_when_one_is_present() {
    // POSITIVE CONTROL. Without this, a scan with a broken filter would report every module clean
    // and the boundary would be unproven rather than proven.
    //
    // `route/build.rs` is where the transport DOES live, so it is the honest control: real code in
    // this workspace that the scan must flag.
    let build = include_str!("../src/route/build.rs");
    let found = transport_identifiers_in_code(build);
    assert!(
        !found.is_empty(),
        "the scan found no transport identifier in route/build.rs, where the transport lives — \
         so its silence on the other modules means nothing"
    );

    // And a synthetic line, so the control does not depend on `build.rs` keeping its current shape.
    let synthetic = "pub fn handler(request: axum::extract::Request) -> Response { todo!() }";
    assert_eq!(transport_identifiers_in_code(synthetic).len(), 1);
}

#[test]
fn documentation_may_discuss_the_transport_without_failing_the_scan() {
    // The modules above explain why they avoid these types. If the scan flagged prose, the only
    // way to pass would be to delete the explanation.
    let documented = "//! This module never names axum, and here is why.\n/// Not tower::Service.";
    assert!(transport_identifiers_in_code(documented).is_empty());
}
