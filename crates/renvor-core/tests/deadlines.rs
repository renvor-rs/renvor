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
//! **The set is eight *lifecycle* callbacks, and that qualifier is load-bearing.** Two `Debug`
//! implementations — for `dyn Provider` and `dyn ReadinessContributor` — also call author code,
//! with **no** deadline. They are excluded deliberately, enumerated by
//! [`formatting_reaches_author_code_unbounded_and_that_is_named_rather_than_hidden`], and no
//! lifecycle phase formats either type. Stating "eight" without that qualifier is what the W-005
//! verification re-review (N8) objected to: the exclusion was named in the test and not in the
//! claim, and a reader meets the claim first.
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
                // COMMENTS ARE NOT CODE, and this gate matches text.
                //
                // Found by this gate firing on itself, in the useful direction: T139 added a
                // comment to `renvor-config/src/layer/file.rs` explaining that a caller runs
                // `source.load()` inside `bounded_call`, and the gate read that prose as a call
                // into author code and demanded the file bound something. A file that merely
                // *describes* a callback is not a file that *makes* one.
                //
                // Stripping makes the gate stricter, not laxer, in both directions: a bounding
                // construct that appears only in a comment no longer satisfies the check either.
                //
                // TWO LIMITS, stated rather than implied (W-005 delta D7-5). This is a `split_once`
                // on `//`, not a comment parser. It truncates a line at any `//`, including one
                // inside a string literal — `let base = "https://x"; provider.stop().await;` loses
                // its call. One production line is truncated today, `observe/spans.rs:163`, with no
                // live effect. And `/* … */` is not stripped at all, so the false positive this
                // fixed reproduces verbatim in block-comment form. Both fail in the STRICTER
                // direction for bounding constructs and the LAXER direction for author calls, so
                // neither is safe to forget; a real parser is the fix if either ever bites.
                let production = production
                    .lines()
                    .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
                    .collect::<Vec<_>>()
                    .join("\n");
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

/// Every `impl fmt::Debug` on an author-implemented trait object in the kernel.
///
/// Found by the W-005 requirements review (Q4-1, Q4-2, Q4-3), which is worth recording: the
/// discovery gate above scans for *handles and invocations* and both of these are invocations, so
/// it saw them — and then the `BOUNDED_BY_ANCESTOR` entry for each file waved them through on the
/// strength of a **lifecycle** ancestor that has nothing to do with formatting. The exemption was
/// real; the reason attached to it was about a different call.
///
/// # This list used to record a defect. It now records a closure.
///
/// Until T145 this constant was named `FORMATTING_CALLS_AUTHOR_CODE` and its second column named
/// the author methods each impl invoked — `name()` for the contributor, `id()`/`provides()`/
/// `dependencies()` for the provider. The test below asserted that the set had not *grown*, and
/// documented the unbounded call as a permanent limitation.
///
/// That was the wrong gate. A limitation that contradicts FR-025 is not something to enumerate
/// accurately; it is something to remove. Both impls now render from **static** text and call no
/// author method at all, so the list below is a list of impls to keep honest rather than a list of
/// known holes.
const DEBUG_IMPLS_ON_AUTHOR_TRAITS: &[(&str, &str)] = &[
    (
        "renvor-core/health/contributor.rs",
        "impl fmt::Debug for dyn ReadinessContributor",
    ),
    (
        "renvor-core/provider/registry.rs",
        "impl fmt::Debug for dyn Provider",
    ),
];

/// The text of every `impl fmt::Debug for dyn ...` block in `text`, brace-matched.
///
/// A pure function taking the source, so the controls below can feed it synthetic input and prove
/// the detector detects. A gate that only ever runs against a passing tree cannot distinguish
/// "nothing is wrong" from "nothing is being checked" — this file has been caught by that twice.
fn debug_impl_bodies(text: &str) -> Vec<String> {
    const MARKER: &str = "impl fmt::Debug for dyn ";

    let mut bodies = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find(MARKER) {
        let after = &rest[start..];
        let Some(open) = after.find('{') else { break };

        let mut depth = 0usize;
        let mut end = None;
        for (offset, character) in after[open..].char_indices() {
            match character {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(open + offset + 1);
                        break;
                    }
                }
                _ => {}
            }
        }

        match end {
            Some(end) => {
                bodies.push(after[..end].to_owned());
                rest = &after[end..];
            }
            // An unbalanced block cannot be judged, so it is reported rather than skipped.
            None => {
                bodies.push(after.to_owned());
                break;
            }
        }
    }

    bodies
}

/// Whether a `Debug` impl body reaches through `self` into the object it is formatting.
///
/// `self.` rather than a list of method names, deliberately. A denylist of "the methods we know
/// are author code" fails open the moment a trait gains a method; every field of a trait object is
/// behind a trait method, so **any** dereference of `self` in this position is author code. The
/// signature `fn fmt(&self, ...)` contains `&self,` and not `self.`, so it does not match.
fn reaches_into_the_formatted_object(body: &str) -> bool {
    body.contains("self.")
}

/// Types that hold a **boxed author error** and would reach its `Debug` through a derive.
///
/// # This is the second route, and the first version of this gate did not look at it
///
/// T145 closed `impl fmt::Debug for dyn …`. The round-four security review (S4-2, MAJOR) found the
/// other way author code reaches `fmt::Debug`: `#[derive(Debug)]` on a struct or enum holding
/// `Box<dyn Error + Send + Sync>`. The derive calls the boxed value's `Debug`, which is the
/// author's.
///
/// It was reproduced, not argued: `format!("{error:?}")` on a `KernelError::ProviderInit` whose
/// source had a panicking `Debug` panicked, and one whose source blocked never returned.
///
/// The gate is a *class* check rather than a list of the two types that had the defect, because
/// "fix the files you were pointed at" is the failure mode this branch has now hit four times.
fn derives_debug_over_a_boxed_author_error(text: &str) -> Vec<String> {
    const BOXED: [&str; 2] = ["BoxedCause", "Box<dyn std::error::Error"];

    let mut offenders = Vec::new();
    // A declaration is `#[derive(...Debug...)]` followed by the type it applies to, and then the
    // body. Scanning forward from each derive to the end of its item is enough: a boxed author
    // error mentioned anywhere in that item is a field the derive will format.
    for (index, _) in text.match_indices("#[derive(") {
        let after = &text[index..];
        let Some(close) = after.find(')') else {
            continue;
        };
        if !after[..close].contains("Debug") {
            continue;
        }
        // The item ends at the first line that closes it at column 0.
        let body_end = after.find("\n}").map_or(after.len(), |end| end + 2);
        let item = &after[..body_end];
        if BOXED.iter().any(|needle| item.contains(needle)) {
            let name = item
                .lines()
                .find(|line| line.contains("pub struct") || line.contains("pub enum"))
                .unwrap_or("<unnamed item>")
                .trim()
                .to_owned();
            offenders.push(name);
        }
    }
    offenders
}

#[test]
fn formatting_an_author_trait_object_never_calls_author_code() {
    // T145. **This is now a bound, not a stated limit.** It was a limit until this commit: both
    // impls called author methods with no deadline, so an author whose `name()` blocked hung
    // whatever formatted it — and a readiness probe is externally driven, so "whatever formatted
    // it" was reachable by anyone who could call the probe.
    //
    // Bounding the call was considered and rejected: a `Debug` impl that spawned a thread per
    // field would make every formatted provider a scheduling event, and `fmt::Debug` has no way
    // to report a deadline failure except by writing one into the output it is producing, so a
    // log line would silently become a timeout report.
    //
    // What is done instead is to call nothing. `&self` is all these methods receive and every
    // fact about it is behind a trait method, so both render static text. Identity is unaffected:
    // it comes from `ResolutionReport`, `InitialisationOrder`, and `ReadinessReport`, which hold
    // names Renvor captured itself inside already-bounded calls.
    let sources = kernel_sources();

    let mut found = Vec::new();
    let mut offenders = Vec::new();
    for (path, text) in &sources {
        for body in debug_impl_bodies(text) {
            found.push(path.clone());
            if reaches_into_the_formatted_object(&body) {
                offenders.push(path.clone());
            }
        }
    }
    found.sort();
    offenders.sort();

    assert!(
        offenders.is_empty(),
        "a `Debug` impl on an author-implemented trait reads through `self`, which calls author \
         code with no deadline and no way to report a fault: {offenders:?}. Render static text \
         instead, and take identity from the report Renvor already holds"
    );

    let mut expected: Vec<String> = DEBUG_IMPLS_ON_AUTHOR_TRAITS
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    expected.sort();

    assert_eq!(
        found, expected,
        "a `Debug` impl on an author-implemented trait appeared or moved. Add it to \
         DEBUG_IMPLS_ON_AUTHOR_TRAITS, and make sure it calls no author method"
    );

    // POSITIVE CONTROL 1: the search finds something. An empty match would make the equality above
    // hold against an empty expectation and the emptiness assertion hold vacuously.
    assert!(
        !found.is_empty(),
        "the `impl fmt::Debug for dyn` search matched nothing; it is not reading the crate"
    );

    // POSITIVE CONTROL 2: the detector detects. Feeding it the exact shape this gate exists to
    // catch — the code that was here until T145 — must produce a body and must flag it. Without
    // this, deleting the body of `reaches_into_the_formatted_object` would leave a green gate.
    let offending = "impl fmt::Debug for dyn Provider {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        f.debug_struct(\"Provider\").field(\"id\", &self.id()).finish()\n    }\n}\n";
    let extracted = debug_impl_bodies(offending);
    assert_eq!(extracted.len(), 1, "the extractor missed a whole impl");
    assert!(
        reaches_into_the_formatted_object(&extracted[0]),
        "the detector passed the exact body this gate exists to refuse"
    );

    // POSITIVE CONTROL 3: and it does not flag the clean shape, so control 2 is not passing
    // because the detector says yes to everything.
    let clean = "impl fmt::Debug for dyn Provider {\n    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {\n        f.debug_struct(\"Provider\").finish_non_exhaustive()\n    }\n}\n";
    let extracted = debug_impl_bodies(clean);
    assert_eq!(extracted.len(), 1, "the extractor missed a whole impl");
    assert!(
        !reaches_into_the_formatted_object(&extracted[0]),
        "the detector flags a body that calls nothing, so it flags everything"
    );
}

#[test]
fn no_derived_debug_formats_a_boxed_author_error() {
    // T159 / S4-2, the SECOND route into author code from `fmt::Debug`. The gate above scans
    // `impl fmt::Debug for dyn …`; this one scans derives over a boxed author error, which is how
    // `KernelError::ProviderInit` and `EntropyUnavailable` were still reaching author code after
    // T145 had "removed the unbounded formatting calls".
    let sources = kernel_sources();

    let mut offenders = Vec::new();
    for (path, text) in &sources {
        for item in derives_debug_over_a_boxed_author_error(text) {
            offenders.push(format!("{path}: {item}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "a derived `Debug` formats a boxed author error, which calls author code with no deadline \
         and no way to report a fault: {offenders:?}. Hand-write `Debug` so the cause is omitted; \
         it stays reachable through `Error::source()`"
    );

    // POSITIVE CONTROL: the detector detects. This is the exact shape that was in `error/mod.rs`
    // until T159 — without this, deleting the function body would leave a permanently green gate.
    let offending = "#[derive(Debug, thiserror::Error)]\npub enum KernelError {\n    ProviderInit {\n        provider: String,\n        source: BoxedCause,\n    },\n}\n";
    assert_eq!(
        derives_debug_over_a_boxed_author_error(offending).len(),
        1,
        "the detector passed the exact shape this gate exists to refuse"
    );

    // POSITIVE CONTROL 2: and it does not flag a derive with no boxed author error, nor a boxed
    // author error without a `Debug` derive. Either false positive would make the check useless.
    let clean_derive = "#[derive(Debug)]\npub struct Fine {\n    name: String,\n}\n";
    assert!(
        derives_debug_over_a_boxed_author_error(clean_derive).is_empty(),
        "the detector flags an ordinary derive"
    );
    let hand_written = "#[derive(Clone)]\npub struct Held {\n    source: BoxedCause,\n}\n";
    assert!(
        derives_debug_over_a_boxed_author_error(hand_written).is_empty(),
        "the detector flags a type that holds a cause but does not derive Debug"
    );
}

/// A boxed author error whose `Debug` panics.
struct PanickingCauseDebug;

impl std::fmt::Debug for PanickingCauseDebug {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("formatting a kernel error called the author's Debug impl");
    }
}

impl std::fmt::Display for PanickingCauseDebug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an author error")
    }
}

impl std::error::Error for PanickingCauseDebug {}

/// A boxed author error whose `Debug` never returns.
struct BlockingCauseDebug;

impl std::fmt::Debug for BlockingCauseDebug {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}

impl std::fmt::Display for BlockingCauseDebug {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an author error")
    }
}

impl std::error::Error for BlockingCauseDebug {}

#[test]
fn formatting_a_kernel_error_never_calls_the_causes_debug() {
    // The runtime half of S4-2, on the variant the reviewer reproduced it with. If the derive were
    // back, this panics inside `format!` with the message from the cause.
    let error = renvor_core::KernelError::ProviderInit {
        provider: "billing".to_owned(),
        source: Box::new(PanickingCauseDebug),
    };

    let rendered = format!("{error:?}");
    assert_eq!(
        rendered, "ProviderInit { provider: \"billing\", .. }",
        "the rendering changed; the cause must stay omitted"
    );

    // And the cause is NOT lost — it is reachable where a caller asks for it explicitly.
    assert!(
        std::error::Error::source(&error).is_some(),
        "omitting the cause from Debug must not remove it from the error chain"
    );
}

#[test]
fn formatting_a_kernel_error_whose_cause_blocks_still_returns() {
    let rendered = format_within("kernel error", Duration::from_secs(10), || {
        let error = renvor_core::KernelError::ProviderStop {
            provider: "billing".to_owned(),
            source: Box::new(BlockingCauseDebug),
        };
        format!("{error:?}")
    });

    assert_eq!(rendered, "ProviderStop { provider: \"billing\", .. }");
}

/// A provider whose every declaration accessor is hostile in the loudest possible way.
///
/// Panicking rather than returning a marker value on purpose: a marker could be printed and the
/// test would then have to prove a *string* was absent, which is a weaker claim than proving the
/// call never happened. A panic makes the call itself the failure.
struct PanickingDeclarations;

impl renvor_core::Provider for PanickingDeclarations {
    fn id(&self) -> &renvor_core::ProviderId {
        panic!("formatting called `Provider::id`");
    }

    fn provides(&self) -> &[renvor_core::CapabilityId] {
        panic!("formatting called `Provider::provides`");
    }

    fn dependencies(&self) -> &[renvor_core::CapabilityId] {
        panic!("formatting called `Provider::dependencies`");
    }

    fn initialise<'a>(
        &'a self,
        _: &'a mut renvor_core::InitContext<'_>,
    ) -> renvor_core::provider::ProviderFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

/// A provider whose accessors never return at all.
///
/// The case a panic cannot stand in for: a panicking method still *returns control*, so a test
/// that only used `PanickingDeclarations` would prove formatting does not observe author output
/// while leaving "formatting can hang" untested. These diverge instead.
struct BlockingDeclarations;

impl renvor_core::Provider for BlockingDeclarations {
    fn id(&self) -> &renvor_core::ProviderId {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }

    fn provides(&self) -> &[renvor_core::CapabilityId] {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }

    fn dependencies(&self) -> &[renvor_core::CapabilityId] {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }

    fn initialise<'a>(
        &'a self,
        _: &'a mut renvor_core::InitContext<'_>,
    ) -> renvor_core::provider::ProviderFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

/// A contributor whose `name()` panics.
struct PanickingName;

impl renvor_core::ReadinessContributor for PanickingName {
    fn name(&self) -> &str {
        panic!("formatting called `ReadinessContributor::name`");
    }

    fn readiness(&self) -> renvor_core::Readiness {
        panic!("formatting called `ReadinessContributor::readiness`");
    }
}

/// A contributor whose `name()` never returns.
struct BlockingName;

impl renvor_core::ReadinessContributor for BlockingName {
    fn name(&self) -> &str {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }

    fn readiness(&self) -> renvor_core::Readiness {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}

/// Formats `value` on a worker thread and fails if it has not finished within `limit`.
///
/// The blocking half of this proof cannot be written as a direct call: if formatting *did* reach
/// author code the test would hang rather than fail, and a hanging test is a test that reports
/// nothing. The work is moved to a thread that is allowed to leak — it is parked in a `sleep` and
/// no Rust API interrupts a blocked thread, which is the same permanent-leak limitation the kernel
/// states elsewhere and does not pretend to have solved here.
fn format_within(
    label: &'static str,
    limit: Duration,
    render: impl FnOnce() -> String + Send + 'static,
) -> String {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(render());
    });

    receiver.recv_timeout(limit).unwrap_or_else(|_| {
        panic!(
            "formatting a {label} did not finish within {limit:?}, so it waited on author code \
             (FR-025, C-L7)"
        )
    })
}

#[test]
fn formatting_a_provider_never_calls_its_declaration_accessors() {
    // T145, the runtime half. The static gate above proves the *source* contains no `self.`; this
    // proves the *behaviour*, so a future refactor that reintroduced the call through a helper
    // (and so evaded a text search) still fails.
    //
    // If `Debug` reached `id()`, this panics inside `format!` and the test fails with the message
    // from the accessor — naming which call was made.
    let provider: Box<dyn renvor_core::Provider> = Box::new(PanickingDeclarations);
    let rendered = format!("{provider:?}");

    assert_eq!(
        rendered, "Provider { .. }",
        "the rendering changed; it must stay derived from static text"
    );
}

#[test]
fn formatting_a_provider_whose_accessors_block_still_returns() {
    let rendered = format_within("provider", Duration::from_secs(10), || {
        let provider: Box<dyn renvor_core::Provider> = Box::new(BlockingDeclarations);
        format!("{provider:?}")
    });

    assert_eq!(rendered, "Provider { .. }");
}

#[test]
fn formatting_a_contributor_never_calls_its_name() {
    let contributor: std::sync::Arc<dyn renvor_core::ReadinessContributor> =
        std::sync::Arc::new(PanickingName);
    let rendered = format!("{contributor:?}");

    assert_eq!(
        rendered, "ReadinessContributor { .. }",
        "the rendering changed; it must stay derived from static text"
    );
}

#[test]
fn formatting_a_contributor_whose_name_blocks_still_returns() {
    let rendered = format_within("contributor", Duration::from_secs(10), || {
        let contributor: std::sync::Arc<dyn renvor_core::ReadinessContributor> =
            std::sync::Arc::new(BlockingName);
        format!("{contributor:?}")
    });

    assert_eq!(rendered, "ReadinessContributor { .. }");
}

#[test]
fn the_blocking_fixtures_really_do_block() {
    // POSITIVE CONTROL for the two timeout tests. If `BlockingDeclarations::id` returned promptly
    // — because a future edit replaced the `loop` with a stub — those tests would pass while
    // proving nothing at all. Calling the accessor directly on a worker must NOT finish.
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let provider = BlockingDeclarations;
        let _ = sender.send(renvor_core::Provider::id(&provider).as_str().to_owned());
    });

    assert!(
        receiver.recv_timeout(Duration::from_secs(2)).is_err(),
        "the blocking fixture returned, so the tests above are not proving formatting avoids it"
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
