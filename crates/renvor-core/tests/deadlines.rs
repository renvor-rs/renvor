//! T064 — SC-015: **0** unbounded waits exist in kernel-owned paths (FR-025, C-L7).
//!
//! # What "0 unbounded waits" can and cannot be proven by
//!
//! A test cannot enumerate every future the kernel might ever await. What it *can* do is close the
//! set — and **an earlier version of this file closed the wrong set.**
//!
//! It has now closed the wrong set **twice**, each time because the *shape of the search decided
//! the shape of the answer*:
//!
//! 1. It enumerated **three** waits, searching for `.await`. A **synchronous** call that never
//!    returns is not an await, and `Load` and `Validate` call author code from a non-async
//!    `build`. The set was five.
//! 2. It then enumerated **five**, searching three named files. Author code is not reached only
//!    from three files: `EntropySource::fill`, `ReadinessContributor::readiness`, and the
//!    `Provider` declaration accessors are called from elsewhere, and all three were unbounded.
//!    The set is **eight**.
//!
//! | Kernel-owned wait | Kind | Bounded by |
//! |---|---|---|
//! | The entropy source filling the run identifier | **synchronous** | the build deadline |
//! | A configuration source reporting its name | **synchronous** | the build deadline |
//! | A configuration source loading | **synchronous** | the build deadline |
//! | A configuration source validating | **synchronous** | the build deadline |
//! | A provider declaring its capabilities at `Register` | **synchronous** | the build deadline |
//! | A provider initialising | async | the provider deadline |
//! | A provider stopping | async | the provider deadline |
//! | A readiness contributor answering | **synchronous** | the readiness deadline |
//!
//! (In-flight work draining is bounded by the drain budget, but nothing author-supplied is *called*
//! there — the kernel waits on its own counter — so it is a bounded wait rather than a callback.)
//!
//! # The gate no longer trusts a list of files
//!
//! Twice burned. [`every_file_that_can_reach_author_code_is_accounted_for`] **discovers** every
//! source file under `src/` at test time and finds the ones holding a `dyn` handle to an
//! author-implemented trait — which is the only way the kernel can reach author code at all.
//! Every such file must either bound its own calls or be named in the inventory as bounded by an
//! ancestor, with that ancestor checked. A new file that takes a `dyn Provider` and awaits it
//! fails here, without anybody remembering to add it to a list.
//!
//! Behaviour tests prove the bounds work **today**; the source checks are what notice when a
//! future edit removes one, because the behaviour test for a removed bound does not fail — it
//! hangs, and a hung test looks like a slow CI machine.
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

/// Signals that a file can reach author-implemented code.
///
/// Two kinds, because either alone misses files. A `dyn` handle finds the files that *hold*
/// author code; a method call finds the files that *invoke* it having obtained it from somewhere
/// else — `lifecycle/application.rs` boots providers it gets from the registry and never names
/// `dyn Provider` once, so a handle-only scan skipped the single most important file in the crate.
///
/// `.name()` and `.id()` are deliberately **absent**: Renvor has its own accessors with those
/// names, so including them would flag half the crate and train a reader to add exemptions. They
/// are covered by the exact call-site checks in [`no_call_into_author_code_is_left_bare`] instead.
const AUTHOR_TRAITS: &[&str] = &[
    // Handles.
    "dyn ConfigSource",
    "dyn Provider",
    "dyn ReadinessContributor",
    "dyn EntropySource",
    // Invocations.
    ".load()",
    ".validate()",
    ".initialise(",
    ".stop()",
    ".provides()",
    ".dependencies()",
    ".readiness()",
    ".fill(",
];

/// Constructs that bound a wait. A file that reaches author code must contain one, or be listed
/// in [`BOUNDED_BY_ANCESTOR`] naming the file that does.
const BOUNDING_CONSTRUCTS: &[&str] = &["tokio::time::timeout(", "bounded_call(", "recv_timeout("];

/// Files that reach author code but are bounded by a **caller**, and which caller.
///
/// Each entry is a claim, and the claim is checked: the named ancestor must exist and must itself
/// contain a bounding construct. An entry cannot be used to wave a file through.
const BOUNDED_BY_ANCESTOR: &[(&str, &str, &str)] = &[
    (
        "renvor-core/provider/mod.rs",
        "renvor-core/lifecycle/builder.rs",
        "`resolve_tracking` reads provider declarations; `build` calls it inside `bounded_call`",
    ),
    (
        "renvor-core/provider/registry.rs",
        "renvor-core/lifecycle/builder.rs",
        "`declared_size` reads `dependencies()`; reached only from `resolve_tracking`, which \
         `build` bounds. The `Debug` impl also reads declarations, and formatting a provider is \
         not a lifecycle wait",
    ),
    (
        "renvor-core/observe/run_id.rs",
        "renvor-core/lifecycle/builder.rs",
        "`generate` calls `EntropySource::fill`; `build` calls it inside `bounded_call`",
    ),
    (
        "renvor-core/health/mod.rs",
        "renvor-core/health/contributor.rs",
        "`readiness` delegates every contributor call to `contributor::ask`, which bounds it",
    ),
];

/// Every bounding call site into author code, by file, and how many there are.
///
/// Counted per file and per construct rather than as one crate-wide total. The crate-wide form
/// was wrong the first time it was written: `bounded_call`'s own implementation contains a
/// `recv_timeout`, so the mechanism counted itself as a ninth call site.
const EXPECTED_BOUNDS: &[(&str, &str, usize)] = &[
    // entropy, source name, load, validate, Register declarations.
    (
        "renvor-core/lifecycle/builder.rs",
        "bounded_call(deadline,",
        5,
    ),
    (
        "renvor-core/lifecycle/application.rs",
        "tokio::time::timeout(",
        1,
    ),
    (
        "renvor-core/lifecycle/rollback.rs",
        "tokio::time::timeout(",
        1,
    ),
    (
        "renvor-core/health/contributor.rs",
        "recv_timeout(deadline)",
        1,
    ),
];

/// The drain's bound, excluded from [`EXPECTED_BOUNDS`] and checked separately.
///
/// It is a bounded wait but **not** a callback: the kernel waits on its own permit counter, and
/// no author method is called. Folding it into the callback total would make the number mean two
/// things at once.
const DRAIN_BOUND: (&str, &str) = ("renvor-core/lifecycle/drain.rs", "tokio::time::timeout(");

/// Reads every `.rs` file under `src/`, returning `(relative path, production source)`.
///
/// Discovery rather than a list. The two times this file closed the wrong set, it was because
/// something decided in advance where to look.
fn kernel_sources() -> Vec<(String, String)> {
    // BOTH kernel crates. Walking only `renvor-core` evidenced SC-015 for one of the two crates
    // that can reach author code, and said "every file under src/" while meaning one crate's.
    let core = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let config = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate directory has a parent")
        .join("renvor-config/src");

    let mut found = Vec::new();
    let mut pending = vec![core.clone(), config.clone()];

    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(&directory).expect("a kernel crate's src/ is readable");
        for entry in entries {
            let path = entry.expect("a readable directory entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = std::fs::read_to_string(&path).expect("a readable source file");
                // Test modules may call author code freely: a test is not a kernel-owned path.
                let production = text
                    .split("#[cfg(test)]")
                    .next()
                    .expect("split always yields one part")
                    .to_owned();
                let relative = path
                    .strip_prefix(&core)
                    .map(|rest| format!("renvor-core/{}", rest.to_string_lossy()))
                    .or_else(|_| {
                        path.strip_prefix(&config)
                            .map(|rest| format!("renvor-config/{}", rest.to_string_lossy()))
                    })
                    .expect("every path came from one of the two roots")
                    .replace('\\', "/");
                found.push((relative, production));
            }
        }
    }
    found.sort();
    found
}

#[test]
fn every_file_that_can_reach_author_code_is_accounted_for() {
    // T115. The gate that does not trust a list of filenames — see the module documentation for
    // the two times trusting one produced a confidently wrong answer.
    let sources = kernel_sources();

    // POSITIVE CONTROL 1: discovery actually found the crate. A walk that silently returned
    // nothing would make every assertion below vacuously true.
    assert!(
        sources.len() > 10,
        "the source walk found only {} files; it is not reading the crate",
        sources.len()
    );

    let bounded_elsewhere: std::collections::HashMap<&str, &str> = BOUNDED_BY_ANCESTOR
        .iter()
        .map(|(file, ancestor, _)| (*file, *ancestor))
        .collect();

    let mut reaching = Vec::new();
    for (path, text) in &sources {
        if !AUTHOR_TRAITS.iter().any(|handle| text.contains(handle)) {
            continue;
        }
        reaching.push(path.clone());

        if BOUNDING_CONSTRUCTS
            .iter()
            .any(|construct| text.contains(construct))
        {
            continue;
        }

        let ancestor = bounded_elsewhere.get(path.as_str()).unwrap_or_else(|| {
            panic!(
                "`src/{path}` holds a handle to author-implemented code, bounds nothing itself, \
                 and is not listed in BOUNDED_BY_ANCESTOR. Either bound its calls or record which \
                 caller does (FR-025, SC-015)."
            )
        });

        // The claim is checked, not taken. An ancestor that stopped bounding anything must not
        // keep vouching for its descendants.
        let (_, ancestor_text) = sources
            .iter()
            .find(|(candidate, _)| candidate == ancestor)
            .unwrap_or_else(|| panic!("`src/{path}` names `src/{ancestor}`, which does not exist"));
        assert!(
            BOUNDING_CONSTRUCTS
                .iter()
                .any(|construct| ancestor_text.contains(construct)),
            "`src/{path}` claims to be bounded by `src/{ancestor}`, but that file contains no \
             bounding construct — the claim is stale"
        );
    }

    // POSITIVE CONTROL 2: files really were found to reach author code. If the trait names were
    // ever renamed, `reaching` would be empty and the loop above would check nothing at all.
    assert!(
        reaching.len() >= 6,
        "only {} file(s) were found to reach author code: {reaching:?}. The AUTHOR_TRAITS tokens \
         have probably drifted from the real trait names",
        reaching.len()
    );

    // POSITIVE CONTROL 3: every recorded ancestor claim corresponds to a file that really does
    // reach author code, so the list cannot rot into a set of exemptions for files that moved on.
    for (file, _, _) in BOUNDED_BY_ANCESTOR {
        assert!(
            reaching.iter().any(|found| found == file),
            "BOUNDED_BY_ANCESTOR lists `src/{file}`, which no longer reaches author code — remove \
             the entry rather than leaving a standing exemption"
        );
    }
}

#[test]
fn no_call_into_author_code_is_left_bare() {
    // The complement of the test above: that one asks *which files* can wait, this one asks
    // whether the specific known call shapes are still wrapped. Both are needed — a file can
    // contain a bounding construct and still add a second, bare call beside it.
    let sources = kernel_sources();
    let read = |name: &str| -> String {
        sources
            .iter()
            .find(|(path, _)| path == name)
            .map(|(_, text)| text.clone())
            .unwrap_or_else(|| panic!("src/{name} not found; the call site moved"))
    };

    let boot = read("renvor-core/lifecycle/application.rs");
    let stop = read("renvor-core/lifecycle/rollback.rs");
    let builder = read("renvor-core/lifecycle/builder.rs");
    let contributor = read("renvor-core/health/contributor.rs");

    // Each row: the file, the bare shape that must NOT appear, and the wrapper that must.
    let checks: &[(&str, &str, &[&str])] = &[
        (
            "renvor-core/lifecycle/application.rs",
            "provider.initialise(&mut context).await",
            &["tokio::time::timeout(", "provider.initialise("],
        ),
        (
            "renvor-core/lifecycle/rollback.rs",
            "provider.stop().await",
            &["tokio::time::timeout(", "provider.stop()"],
        ),
        (
            "renvor-core/lifecycle/builder.rs",
            "handle.load()?",
            &["bounded_call(deadline, move || handle.load())"],
        ),
        (
            "renvor-core/lifecycle/builder.rs",
            "handle.validate()?",
            &["bounded_call(deadline, move || handle.validate())"],
        ),
        (
            "renvor-core/lifecycle/builder.rs",
            "RunIdentifier::generate(self.entropy",
            &["bounded_call(deadline, move || RunIdentifier::generate("],
        ),
        (
            "renvor-core/lifecycle/builder.rs",
            "self.registry.resolve()",
            &["registry.resolve_tracking(&counter)"],
        ),
        (
            "renvor-core/health/contributor.rs",
            "contributor.readiness()",
            &["recv_timeout(deadline)", "handle.readiness()"],
        ),
    ];

    for (file, bare, wrappers) in checks {
        let text = match *file {
            "renvor-core/lifecycle/application.rs" => &boot,
            "renvor-core/lifecycle/rollback.rs" => &stop,
            "renvor-core/lifecycle/builder.rs" => &builder,
            _ => &contributor,
        };
        assert!(
            !text.contains(bare),
            "`src/{file}` calls author code without a deadline: found `{bare}` (FR-025, C-L7)"
        );
        // POSITIVE CONTROL: the wrapped form is present, so the absence above means "bounded"
        // rather than "the call was deleted and the scan found nothing".
        for wrapper in *wrappers {
            assert!(
                text.contains(wrapper),
                "`src/{file}` no longer contains `{wrapper}`; this check is guarding a call site \
                 that has moved"
            );
        }
    }
}

/// `Debug` implementations the kernel provides that call author code, and why each is unbounded.
///
/// Found by the W-005 requirements review (Q4-1, Q4-2, Q4-3), which is worth recording: the
/// discovery gate above scans for *handles and invocations* and both of these are invocations, so
/// it saw them — and then the `BOUNDED_BY_ANCESTOR` entry for each file waved them through on the
/// strength of a **lifecycle** ancestor that has nothing to do with formatting. The exemption was
/// real; the reason attached to it was about a different call.
const FORMATTING_CALLS_AUTHOR_CODE: &[(&str, &str)] = &[
    (
        "renvor-core/health/contributor.rs",
        "impl fmt::Debug for dyn ReadinessContributor calls `name()`",
    ),
    (
        "renvor-core/provider/registry.rs",
        "impl fmt::Debug for dyn Provider calls `id()`, `provides()`, and `dependencies()`",
    ),
];

#[test]
fn formatting_reaches_author_code_unbounded_and_that_is_named_rather_than_hidden() {
    // T128/Q4-1. **This is a stated limit, not a bound.** Formatting a provider or a contributor
    // calls author code with no deadline, so an author whose `name()` blocks will hang whatever
    // formatted it.
    //
    // It is not bounded, and bounding it would be worse than the disease: a `Debug` impl that
    // spawned a thread per field would make every log line a scheduling event, and `fmt::Debug`
    // has no way to report a deadline failure except by writing one into the output it is
    // producing.
    //
    // What matters is that the claim "the set is eight" is now stated with its exclusion named,
    // rather than being true only because nobody looked at `Debug`. **No lifecycle phase formats
    // an author's provider or contributor** — the kernel's own diagnostics carry names and
    // positions it already holds — so no bounded path is compromised by this.
    let sources = kernel_sources();

    let mut found = Vec::new();
    for (path, text) in &sources {
        if text.contains("impl fmt::Debug for dyn ") {
            found.push(path.clone());
        }
    }
    found.sort();

    let mut expected: Vec<String> = FORMATTING_CALLS_AUTHOR_CODE
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    expected.sort();

    assert_eq!(
        found, expected,
        "a `Debug` impl on an author-implemented trait appeared or moved. Each one calls author \
         code with no deadline; add it to FORMATTING_CALLS_AUTHOR_CODE with the methods it calls, \
         or remove the impl. Do not leave it unrecorded — the eight-callback inventory is only \
         complete if this exclusion is explicit"
    );

    // POSITIVE CONTROL: the search finds something. An empty match would make the equality above
    // hold against an empty expectation and record an exclusion list nobody verified.
    assert!(
        !found.is_empty(),
        "the `impl fmt::Debug for dyn` search matched nothing; it is not reading the crate"
    );
}

#[test]
fn the_enumerated_set_of_kernel_owned_callbacks_is_still_eight() {
    // The set is closed by counting bounding call sites, so a **ninth** added later fails here
    // rather than silently escaping the enumeration in this file's documentation. That is the
    // failure mode this file has already suffered twice.
    let sources = kernel_sources();
    let text_of = |name: &str| -> String {
        sources
            .iter()
            .find(|(path, _)| path == name)
            .map(|(_, text)| text.clone())
            .unwrap_or_else(|| panic!("src/{name} not found; the call site moved"))
    };

    let mut total = 0;
    for (file, construct, expected) in EXPECTED_BOUNDS {
        let found = text_of(file).matches(construct).count();
        assert_eq!(
            found, *expected,
            "`src/{file}` has {found} × `{construct}`, expected {expected}. A wait was added or \
             removed without updating the enumeration in this file's documentation"
        );
        total += found;
    }

    assert_eq!(
        total, 8,
        "expected 8 bounding call sites into author code: entropy, source name, load, validate, \
         and the Register declarations (5 × `bounded_call`); provider initialise and provider \
         stop (2 × `tokio::time::timeout`); and the readiness contributor (1 × `recv_timeout`)"
    );

    // The drain is bounded too, and deliberately outside the total above.
    let (drain_file, drain_construct) = DRAIN_BOUND;
    assert_eq!(
        text_of(drain_file).matches(drain_construct).count(),
        1,
        "`src/{drain_file}` must still bound the drain, even though its wait is on the kernel's \
         own counter rather than on author code"
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
