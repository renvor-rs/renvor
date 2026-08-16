//! T081 — SC-016 and FR-037(b): a credential-bearing value registered **without being marked
//! secret** must not appear in any output.
//!
//! # Why the value here is deliberately not marked
//!
//! FR-037(b) is explicit that the kernel **must not assume opaque state is safe to print merely
//! because it was not marked secret** — an author may register a credential without marking
//! anything, and usually will, because marking it requires knowing it needs marking.
//!
//! So [`UnmarkedCredential`] carries a credential and wears no marker of any kind. If the kernel's
//! guarantee depended on the author remembering, this file would catch it. It does not depend on
//! that: `TypedStateMap` stores values as `Box<dyn Any + Send + Sync>`, a type with **no `Debug`
//! bound**, so the kernel cannot print a stored value even if it wanted to.
//!
//! # A whole application, not a map in isolation
//!
//! The state map's own unit tests already prove its two output forms are clean. This file goes
//! wider: it registers the credential through a real provider, boots a real application, and
//! sweeps **every** output an operator or a test could reach from the outside — because the
//! interesting leak is never the one in the type you were watching.

mod support;

use renvor_core::provider::ProviderFuture;
use renvor_core::{ApplicationBuilder, CapabilityId, InitContext, Provider, ProviderId};

/// The value that must appear in **0** outputs.
const CREDENTIAL: &str = "hunter2-do-not-print";

/// State carrying a credential, registered with **no** secret marker of any kind.
#[derive(Debug)]
struct UnmarkedCredential {
    #[allow(dead_code)]
    token: String,
}

/// A provider that registers the unmarked credential during Boot.
#[derive(Debug)]
struct Registrar {
    id: ProviderId,
    provides: Vec<CapabilityId>,
}

impl Provider for Registrar {
    fn id(&self) -> &ProviderId {
        &self.id
    }
    fn provides(&self) -> &[CapabilityId] {
        &self.provides
    }
    fn initialise<'a>(&'a self, context: &'a mut InitContext<'_>) -> ProviderFuture<'a> {
        Box::pin(async move {
            context.register_state(UnmarkedCredential {
                token: CREDENTIAL.to_owned(),
            })?;
            Ok(())
        })
    }
}

fn registrar() -> Box<dyn Provider> {
    Box::new(Registrar {
        id: ProviderId::new("credential-holder"),
        provides: vec![CapabilityId::new("credentials")],
    })
}

#[tokio::test]
async fn an_unmarked_credential_appears_in_zero_application_outputs() {
    let application = ApplicationBuilder::new()
        .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
            11;
            32
        ])))
        .with_provider(registrar())
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    // Every output form reachable from outside the kernel.
    let outputs: Vec<(&str, String)> = vec![
        ("application Debug", format!("{application:?}")),
        ("application Debug alternate", format!("{application:#?}")),
        ("state Debug", format!("{:?}", application.state())),
        ("run identifier", application.run_id().to_string()),
        ("phase", application.phase().to_string()),
        (
            "resolution report",
            format!("{:?}", application.resolution_report()),
        ),
        ("registry", format!("{:?}", application.registry())),
        (
            "initialisation order",
            format!("{:?}", application.initialisation_order()),
        ),
        (
            "initialised set",
            format!("{:?}", application.initialised()),
        ),
        ("cancel scope", format!("{:?}", application.cancel())),
        ("work gate", format!("{:?}", application.work())),
    ];

    for (name, rendered) in &outputs {
        assert!(
            !rendered.contains(CREDENTIAL),
            "the credential reached `{name}`: {rendered}"
        );
    }

    // POSITIVE CONTROL: the sweep is looking at real text, and the search string is findable when
    // it is present. Without this, a typo in the constant would make every assertion vacuous.
    assert!(format!("{CREDENTIAL:?}").contains(CREDENTIAL));
    assert!(
        outputs.iter().any(|(_, rendered)| !rendered.is_empty()),
        "every output was empty, so nothing was actually inspected"
    );
}

#[tokio::test]
async fn the_permitted_type_name_is_present_while_the_contents_are_not() {
    // FR-037(b) permits the **type name** and nothing else. Asserting only the absence would pass
    // for a `Debug` impl that emitted nothing at all — which would be safe and useless.
    let application = ApplicationBuilder::new()
        .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
            11;
            32
        ])))
        .with_provider(registrar())
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let rendered = format!("{:?}", application.state());
    assert!(
        rendered.contains("UnmarkedCredential"),
        "the permitted type name is missing: {rendered}"
    );
    assert!(!rendered.contains(CREDENTIAL), "{rendered}");
}

#[tokio::test]
async fn an_error_raised_while_the_credential_is_registered_stays_clean() {
    // The path where a leak would actually happen: something goes wrong *after* the credential is
    // in state, and the failure is rendered along with whatever context the kernel has.
    let journal = support::Journal::new();
    let failure = ApplicationBuilder::new()
        .with_entropy(Box::new(renvor_core::observe::FixedEntropy::new(vec![
            11;
            32
        ])))
        .with_provider(registrar())
        .with_provider(
            support::Scripted::new(&journal, "breaks")
                .needs(&["credentials"])
                .behaving(support::Behaviour::FailInit)
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("the second provider fails");

    let mut rendered = format!("{failure}{failure:?}");
    let mut cause: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(&failure);
    while let Some(error) = cause {
        rendered.push_str(&error.to_string());
        cause = error.source();
    }

    assert!(
        !rendered.contains(CREDENTIAL),
        "the credential reached a failure report: {rendered}"
    );
    assert!(
        rendered.contains("breaks"),
        "and the failure still names the provider: {rendered}"
    );
}

/// W-005 security finding 1.1 — the wrapper that was left deriving `Debug`.
///
/// `SchemaSource` and `ConfigHandle` both hand-write `Debug` so a resolved configuration cannot
/// print. `ResolvedConfig` — the value they wrap — derived it, so formatting the thing they were
/// protecting printed every raw value it held.
#[test]
fn a_resolved_configuration_prints_its_keys_and_never_its_values() {
    use renvor_core::config_port::{Attribution, ResolvedConfig, SourceLayer};

    /// Both fields exist to be *printed*, or rather not to be. Nothing reads `host` back, which
    /// is the shape of the test rather than an oversight.
    #[allow(dead_code)]
    #[derive(Debug)]
    struct Settings {
        host: String,
        password: String,
    }

    const CREDENTIAL: &str = "hunter2-do-not-print";

    let resolved = ResolvedConfig::new(
        Settings {
            host: "localhost".to_owned(),
            password: CREDENTIAL.to_owned(),
        },
        vec![(
            "password".to_owned(),
            Attribution {
                layer: SourceLayer::Environment,
                presence: renvor_core::config_port::Presence::Present,
            },
        )],
    );

    let rendered = format!("{resolved:?}");
    assert!(
        !rendered.contains(CREDENTIAL),
        "the configuration value reached Debug output: {rendered}"
    );
    assert!(
        !rendered.contains("localhost"),
        "an unmarked value reached Debug output too: {rendered}"
    );

    // The useful half survives: which key came from which layer.
    assert!(rendered.contains("password"), "{rendered}");
    assert!(rendered.contains("Environment"), "{rendered}");

    // POSITIVE CONTROL: the value really is in there, so the absence above is redaction rather
    // than an empty struct.
    assert_eq!(resolved.value().password, CREDENTIAL);
    assert!(format!("{:?}", resolved.value()).contains(CREDENTIAL));
}
