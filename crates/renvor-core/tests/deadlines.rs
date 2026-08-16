//! T064 — SC-015: **0** unbounded waits exist in kernel-owned paths (FR-025, C-L7).
//!
//! # What "0 unbounded waits" can and cannot be proven by
//!
//! A test cannot enumerate every future the kernel might ever await. What it *can* do is close the
//! set — and **an earlier version of this file closed the wrong set.**
//!
//! It enumerated *three* waits and called the set complete. It was searching for `.await`, and a
//! **synchronous** call that never returns is not an await: `Load` and `Validate` call
//! author-supplied code from a non-async `build`, and both were unbounded. The gap was found by
//! building the failure-injection harness, not by re-reading this file. The set is **five**:
//!
//! | Kernel-owned wait | Kind | Bounded by | Test |
//! |---|---|---|---|
//! | A configuration source loading | **synchronous** | the source deadline | `builder::a_hanging_source_is_bounded_rather_than_blocking_the_process` |
//! | A configuration source validating | **synchronous** | the source deadline | same mechanism, same call site |
//! | A provider initialising | async | the provider deadline | `a_hanging_provider_does_not_hang_boot` |
//! | A provider stopping | async | the provider deadline | `a_hanging_stop_does_not_hang_shutdown` |
//! | In-flight work draining | async | the drain budget | `a_drain_never_waits_past_its_budget` |
//!
//! The lesson is recorded rather than quietly fixed: **the shape of the search decided the shape
//! of the answer**. A scan for `.await` can only ever find awaits, and reporting its result as
//! "every kernel-owned wait" was a claim the scan could not support.
//!
//! Then `the_kernel_never_waits_on_foreign_code_without_a_deadline` reads the kernel's own source
//! and fails if any of those calls loses its wrapper. Behaviour tests prove the bounds work
//! **today**; the source check is what notices when a future edit removes one, because the
//! behaviour test for a removed bound does not fail — it hangs, and a hung test looks like a slow
//! CI machine.
//!
//! Every test runs under `start_paused`, so a thirty-second deadline costs **0** real seconds
//! (FR-031). A suite that took thirty seconds to prove a thirty-second deadline would be disabled
//! within a week.

mod support;

use std::time::Duration;

use renvor_core::{DrainOutcome, ErrorCategory};
use support::{Behaviour, Journal, Scripted, builder};

#[tokio::test(start_paused = true)]
async fn a_hanging_provider_does_not_hang_boot() {
    // C-L9's `Hang`. This provider ignores cancellation entirely, which is the point: a
    // cancellation scope is not a deadline, because honouring it is the provider's choice.
    let journal = Journal::new();
    let failure = builder()
        .with_provider_deadline(Duration::from_secs(2))
        .with_provider(
            Scripted::new(&journal, "first")
                .provides(&["first"])
                .boxed(),
        )
        .with_provider(
            Scripted::new(&journal, "hangs")
                .needs(&["first"])
                .behaving(Behaviour::Hang)
                .boxed(),
        )
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect_err("a hanging provider must not hang the boot");

    assert_eq!(failure.origin().category(), ErrorCategory::DeadlineExceeded);
    let rendered = failure.origin().to_string();
    assert!(rendered.contains("hangs"), "names the provider: {rendered}");
    assert!(rendered.contains("2000"), "names the deadline: {rendered}");

    // And it is still a Boot failure in every other respect: what started is rolled back.
    assert_eq!(journal.inits(), vec!["first"]);
    assert_eq!(journal.stops(), vec!["first"]);
}

#[tokio::test(start_paused = true)]
async fn a_provider_that_answers_inside_the_deadline_is_untouched() {
    // POSITIVE CONTROL: the deadline discriminates rather than failing every boot. Without this, a
    // deadline of zero would satisfy the test above.
    let journal = Journal::new();
    let application = builder()
        .with_provider_deadline(Duration::from_secs(2))
        .with_provider(Scripted::new(&journal, "prompt").provides(&["p"]).boxed())
        .build()
        .expect("assembles")
        .boot()
        .await;

    assert!(application.is_ok(), "a prompt provider must simply work");
    assert_eq!(journal.inits(), vec!["prompt"]);
}

#[tokio::test(start_paused = true)]
async fn a_hanging_stop_does_not_hang_shutdown() {
    // The wait that is easiest to leave unbounded, because it happens on the way out when nobody
    // is watching. A provider that never returns from `stop` would hang shutdown for ever.
    let journal = Journal::new();
    let mut application = builder()
        .with_provider_deadline(Duration::from_secs(3))
        .with_provider(
            Scripted::new(&journal, "sticky")
                .provides(&["sticky"])
                .behaving(Behaviour::HangOnStop)
                .boxed(),
        )
        .with_provider(Scripted::new(&journal, "after").needs(&["sticky"]).boxed())
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    let report = application.shutdown().await;

    let failures = report.stop().failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].category(), ErrorCategory::DeadlineExceeded);
    assert!(
        failures[0].to_string().contains("sticky"),
        "{}",
        failures[0]
    );

    // The provider behind the hanging one was still stopped: a deadline is not an abort.
    assert_eq!(journal.stops(), vec!["after", "sticky"]);
}

#[tokio::test(start_paused = true)]
async fn a_drain_never_waits_past_its_budget() {
    let mut application = builder()
        .with_drain_budget(Duration::from_secs(4))
        .build()
        .expect("assembles")
        .boot()
        .await
        .expect("boots");

    // A permit nobody will ever release — the drain has to give up on its own.
    let _permit = application.work().begin("never finishes").expect("open");

    assert_eq!(
        application.shutdown().await.drain(),
        DrainOutcome::Incomplete { outstanding: 1 }
    );
}

#[test]
fn the_kernel_never_waits_on_foreign_code_without_a_deadline() {
    // The check that survives a future edit, covering **all five** call sites into author-supplied
    // code — the two synchronous ones included, which the earlier version of this test missed.
    let boot = include_str!("../src/lifecycle/application.rs");
    let stop = include_str!("../src/lifecycle/rollback.rs");
    let builder = include_str!("../src/lifecycle/builder.rs");

    // Async: a bare `.await` on a provider is an unbounded wait.
    for (name, source, bare) in [
        ("boot", boot, "provider.initialise(&mut context).await"),
        ("rollback", stop, "provider.stop().await"),
    ] {
        assert!(
            !source.contains(bare),
            "`{name}` awaits a provider without a deadline: found `{bare}` (FR-025, C-L7)"
        );
    }

    // Synchronous: a direct `source.load()` or `source.validate()` in `build` is the same defect
    // wearing different syntax. Both must go through the bounded call.
    let production = builder
        .split("#[cfg(test)]")
        .next()
        .expect("split always yields one part");
    for bare in ["source.load()?", "source.validate()?"] {
        assert!(
            !production.contains(bare),
            "`build` calls a configuration source without a deadline: found `{bare}`"
        );
    }

    // POSITIVE CONTROL: every call site exists and every one is wrapped, so the absences above
    // mean "bounded" rather than "the call was deleted and the scan found nothing".
    assert!(
        boot.contains("tokio::time::timeout(") && boot.contains("provider.initialise("),
        "the boot call site moved; this test is checking a file that no longer boots providers"
    );
    assert!(
        stop.contains("tokio::time::timeout(") && stop.contains("provider.stop()"),
        "the stop call site moved; this test is checking a file that no longer stops providers"
    );
    assert!(
        production.contains("bounded_call(deadline, source.name(), \"load\"")
            && production.contains("bounded_call(deadline, source.name(), \"validate\""),
        "the configuration call sites moved; this test is checking a file that no longer loads them"
    );
}

#[test]
fn the_enumerated_set_of_kernel_owned_waits_is_still_five() {
    // The set is closed by counting the call sites into author-supplied code, so a **sixth** one
    // added later fails here rather than silently escaping the enumeration above. That is the
    // failure mode this file already suffered once, when the set was three and should have been
    // five.
    let sources = [
        include_str!("../src/lifecycle/application.rs"),
        include_str!("../src/lifecycle/rollback.rs"),
        include_str!("../src/lifecycle/builder.rs"),
    ];

    let bounded: usize = sources
        .iter()
        .map(|source| {
            let production = source
                .split("#[cfg(test)]")
                .next()
                .expect("split always yields one part");
            production.matches("tokio::time::timeout(").count()
                + production.matches("bounded_call(deadline,").count()
        })
        .sum();

    assert_eq!(
        bounded, 4,
        "expected 4 bounding call sites covering 5 waits — provider initialise, provider stop, \
         and one `bounded_call` for each of load and validate; the drain's own bound lives in \
         drain.rs. A different count means a wait was added or removed without updating the \
         enumeration in this file's documentation"
    );
}

#[tokio::test(start_paused = true)]
async fn the_provider_deadline_has_a_documented_default_that_is_overridable() {
    // FR-025 requires the bound to exist. Its *value* is Renvor's choice, not the specification's
    // — recorded as such on the constant and as an open item — so what matters here is that the
    // default is stated in one place and that an author can replace it.
    let default = builder().build().expect("assembles").provider_deadline();
    assert_eq!(default, renvor_core::lifecycle::DEFAULT_PROVIDER_DEADLINE);

    let overridden = builder()
        .with_provider_deadline(Duration::from_millis(250))
        .build()
        .expect("assembles")
        .provider_deadline();
    assert_eq!(overridden, Duration::from_millis(250));
    assert_ne!(overridden, default, "the override actually took effect");
}
