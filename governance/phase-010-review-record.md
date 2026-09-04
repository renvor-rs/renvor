# Phase 010 — Review Record

**Companion to**: [`phase-010-evidence.md`](phase-010-evidence.md)
**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities

**No independent human review of this phase has occurred, and none is claimed.** Closure rests on
[W-022](waivers.md), granted 2026-09-04, the **tenth consecutive** phase-level waiver of the same
rule for the same reason; ADR-0031 … ADR-0037 are `accepted` under [W-021](waivers.md), a separate
exception on a separate axis; constitution principle VII's generator obligation (L-14) is deferred
under [W-023](waivers.md) and [W-024](waivers.md), not closed. Everything below is
maintainer-commissioned and **advisory**, not independent. *(Until the grant this paragraph read
"rests on the **proposed** … waiver … if granted"; the draft is kept, marked consumed.)*

## 1. What was commissioned, and what came back

| Agent | Purpose | Disposition |
|---|---|---|
| `research-cache-jobs` | RESP clients, job crates, retry crates: versions, licences, MSRV, advisories, graphs | **DELIVERED** on the first commission (one pass, 30 KB) |
| `research-mail-storage` | SMTP crates and every S3-compatible candidate against the same gates | **DELIVERED** (23 KB) |
| `research-observability` | `tracing-subscriber` redaction, metrics ports, OTLP transports, semantic conventions | **DELIVERED** (45 KB) |
| Codex review (`/codex:review`) of the whole branch diff | the closing review before the pull request | **PERFORMED by the maintainer on `1328dd3`; sixteen findings; see §3** |

**Every research agent delivered on the first commission**, so the one-follow-up rule was not
exercised. **Every decisive claim was re-measured before it was relied on**: the RESP client's
root-store feature logic and the `webpki-roots` edge in the rejected client (source and `cargo
tree` probes); `apalis-sql`'s +42 packages with `rsa` and a duplicate `sqlx 0.8` stack (probe);
`lettre`'s advisory history and feature map (crates.io and source); the `0BSD` licence under
`builder` (deny probe); the platform-verifier CDLA failure that rejects `mail-send`,
`object_store`, and `opendal` on the all-target licence gate (deny probe); `aws-sdk-s3`'s
1.94.1 MSRV; and the `tracing-subscriber` JSON formatter bypass, which the observability report
could not verify and which was verified here against `fmt/format/json.rs` 0.3.23.

**Where a report and the measurement disagreed, the measurement won**: the reports counted
packages from a fresh scratch project, this record counts additions to the real 290-package
baseline, and the baseline numbers are the ones in the inventory.

## 2. What the repository's own gates found after the batches were green

The phase's most useful findings came from the gates, not from a review, and each was fixed at
the root and pinned:

| Found by | Defect | Disposition |
|---|---|---|
| the MySQL rows of the shared job contract | the bounded reclaim `UPDATE` inside the claim transaction deadlocked three of four concurrent claimers on InnoDB (gap locks on an empty index range) — PostgreSQL never showed it | reclaim moved to its own autocommit statement on both adapters; all four rows re-run on the final code |
| designing the reclaim statement | an expired lease at the last attempt returned to `ready` in the SQL **and** the memory substitute — a permanently hanging handler would be retried without bound | dead-letters at the last attempt on all five stores; a sixteenth contract assertion with a one-attempt-short control |
| the kernel's diagnostics gate, first run this phase | 14 assertion messages in canary-holding files interpolated renderings (6 from batch B, committed gate-red) | rewritten to fixed messages or a case index; both kernel gates joined every batch's pre-commit sequence |
| the new step 7 capability row | `renvor-cache --features valkey` built alone compiled `rustls` with **no crypto provider**; the workspace's other crates had been supplying `ring`, and a consumer would have hit `ClientConfig::builder()`'s panic at boot | `rustls` named with `ring` on the crate's own `valkey` feature; the manifest comment that said "no crypto provider is added" was the assumption the row disproved |
| the local OTLP receiver | the `service.name` resource never reached the wire: the SDK hands the resource to processors, and Renvor's owns its exporter inside a task | set on the exporter before the drain starts |
| the Mailpit suite | a pooled `lettre` transport built outside a runtime aborts the whole test binary in a destructor; two tests sending through one sink saw each other's message | tests on a runtime; a process-wide guard |
| the final gate, leg A step 4, on the head that closed batch M | the L-11 event test missed its event a second time. `tracing-core` caches callsite interest process-wide and, while at most one dispatcher is registered, computes it against the *registering* thread's dispatcher (0.1.36, `DefaultCallsite::register`, `Dispatchers::rebuilder`); the neighbouring store-failure test, which has no subscriber, could cache the `warn!` callsite as `never` while the test's thread-local recorder was live. Reproduced: 5 of 40 paired runs in fresh processes missed before the fix, 0 after | one global recorder per test binary, each test selecting its events by a correlation identifier no other test uses; the `L-13` row that had recorded the first miss as unexplained is withdrawn; mutation J-M3 re-run on the corrected test and killed |
| the final gate's second run, leg A step 7 (the per-driver compile), on `3bfb552` | both four-row job-store suites (`renvor-sqlx/tests/jobs.rs`, `renvor-seaorm/tests/jobs.rs`) gated their engine modules on the database feature but not on `jobs`, so `cargo check --locked --no-default-features --features db-postgres --all-targets` reached the adapter and the crates only `jobs` enables and did not build. Batch L's step-7 evidence came from `cargo test -p xtask`, which resolves graphs; the per-driver compile runs only inside `cargo xtask verify`, and no full run on this branch had reached step 7 before this one | crate-level `#![cfg(feature = "jobs")]` on both files, the convention every other adapter suite already followed; all four persistence rows and both `jobs` rows compile with `--all-targets`; steps 7–9 were then probed by hand before the next full run (the four rows, the three facade compiles, `gitleaks` in both modes, tree cleanliness) |
| step 8 (`gitleaks`), probed by hand because no full run on this branch had reached it | five `generic-api-key` findings in `renvor-observability`: the redaction canary `hunter2CanaryDoNotLeak` assigned to `password`, `token` and `secret` fields in the very tests that assert it is redacted (commits `ee7720e`, `d66b413`) | recorded as FP-004 in `.gitleaks.toml` under the file's own policy — a content regex, no `paths` entry, the history reason FP-003 states — and verified the policy's way: both scans clean with the entry active, an injected `api_key` canary in the same file still reported by `gitleaks dir .`, and the `xtask` test that refuses path allowlists on shipped source passing |
| pull request #61, the `security` workflow's dependency review, on `c5bf188` | the action's own licence allow-list rejected `quoted_printable` (0BSD) and `xxhash-rust` (BSL-1.0): `deny.toml` had gained both in this phase with the crate named, and the workflow line that claims to mirror it had not — the stale-copy trap this repository records for every duplicated figure | the two identifiers added to the workflow's list with the crates named, next to the comment that says divergence is a defect |
| pull request #61, the release dry run (`cargo package --workspace`), on `c5bf188` | `renvor-testkit` was packaged before `renvor-jobs` and could not resolve it. cargo 1.94's packaging graph (`ops/cargo_package/mod.rs`, `local_deps`) includes optional path dependencies **and** dev-dependencies that carry a version, and ignores versionless ones because publishing strips them; the new `renvor-jobs → renvor-testkit` dev-dependency carried `version = "0.0.0"` while the testkit depends on `renvor-jobs` under `jobs`, which closed a cycle the topological sort resolved the wrong way. Reproduced locally in 29 s. Removing the version first was wrong twice over — it left the order unchanged until the cycle itself was gone, and a versionless path dependency is a wildcard that `deny.toml` bans, which the next local gate run caught at step 6 | the cycle removed at its source: `renvor-jobs` no longer dev-depends on the testkit, and the shared contract runs against `MemoryJobStore` from the testkit's own test directory (`tests/jobs_memory.rs`, gated on `jobs`); `cargo deny` clean, every crate packages locally in order with `renvor-jobs` ahead of the testkit |
| pull request #61, CodeQL, on `c5bf188` | two critical `rust/hard-coded-cryptographic-value` alerts: the refused-credential tests of the Valkey and SMTP suites passed the literal canary as the wrong password | the wrong credential is now built at run time from the clock and asserted absent from every rendering by value, so the tests still prove a refused credential fails closed and is never rendered, and no literal in either file is a password |
| pull request #61, both Windows platform legs, on `c5bf188` | `an_unreachable_server_fails_boot_within_the_connect_timeout` reported `Unanswered` where Linux and macOS report `Unreachable`: the driver reports its connect bound and its response bound as the same timeout error, and Windows retries a SYN to a closed loopback port for longer than the bound, so a slow refusal was classified as a readiness failure — a platform-dependent category, the wrong category the probe exists to prevent | `ValkeyCache::connect` now performs a bounded plain TCP connect to the driver's address first (refusal, no route, or the bound elapsing are `Unreachable` everywhere); the accept-then-silent case still reaches the driver handshake and stays `Unanswered`; the three category tests and the live suite pass locally, and the Windows legs are the proof for the platform that showed it |

## 3. Codex review — performed by the maintainer on `1328dd3`, sixteen findings, one bounded correction round (2026-09-04)

The maintainer ran `/codex:review` on the whole branch diff at the pull request head `1328dd3`
(base `c57b4fb`) and returned sixteen findings. Every one was **verified against the tree before
anything was changed** (the reproducer or code path is in the table), and every confirmed one was
corrected at the root with a RED→GREEN test and a controlled mutation, in one round, with nothing
weakened. The working record — dispositions, RED and GREEN outputs, every mutation, every
assertion changed — is `specs/010-…/evidence/correction-round.md` and `round-{A,D1,E,F,G}.md`
(gitignored, per the phase's convention); this table is the mirror.

| # | Finding (Codex) | Verified as | Correction | Discriminating test(s) | Mutations |
|---|---|---|---|---|---|
| 1 | CSRF `Origin` defence compares the host only | **confirmed**: `cross_site_refused` compared `origin_host()` with the validated host, both port-stripped and scheme-blind; the CORS carve-out `is_same_origin` the same by design | `renvor_http::EffectiveOrigin` (RFC 6454 scheme/host/effective port) parsed by the host normaliser; the request's origin resolved once (configured `public_scheme`, or a trusted proxy's single `proto`; the `Host` port or the scheme's default; a garbage port now refuses the request); both the gate and the carve-out compare the triple | `origin::tests` (10), `identity::tests` (+2), `host::tests`, `renvor-auth-http/src/routes.rs` unit tests (6), `renvor-http/tests/effective_origin.rs` (11), the PostgreSQL-backed flow test extended to nine `Origin` shapes | R-G-M1, M2, M3, M6, M7, M8, M9 — all killed |
| 2a | Reset/verification secret in a query string | **confirmed**: `auth.rs` rendered `{base}{path}?token={t}`; spec FR-051 itself said "only in the body" | the link is the bare page; the token is a code in the text and HTML bodies; the auth routes already read it from the POST body | `the_link_is_built_from_configuration_and_the_token_is_only_in_the_body` (every `https://` word in either body must equal the bare link), the reset test, the live Mailpit bridge test | R-C-M2 killed |
| 2b | SMTP credential inside an SMTP URL | **confirmed**: `SmtpSettings::new(Secret<url>, …)` with `user:password@` | `SmtpEndpoint` + `Option<SmtpCredentials>`; the URL parser deleted; the test sink's credential in `RENVOR_TEST_SMTP_USERNAME`/`_PASSWORD` and the suite refuses a URL with `@` | the settings unit tests; live 6/6 against Mailpit | R-C-M1, R-C-M3 killed |
| 2c | Valkey credential inside a Redis URL | **confirmed**: `ValkeySettings::new(Secret<url>, …)` | `ValkeyEndpoint` + `Option<ValkeyCredentials>`, the driver's `ConnectionInfo` built from parts; `RENVOR_TEST_VALKEY_PASSWORD`; `xtask` step 1, CI, and CONTRIBUTING carry the split | the settings unit tests; live 10/10 against Valkey 9.1.1 | R-B-M2, R-B-M3 killed |
| 3 | Plaintext Valkey to any host | **confirmed**: `redis://` to a non-loopback host connected in plaintext with no check | `ValkeyEndpoint::plaintext` accepted only when loopback **and** `with_allow_insecure_loopback(true)`, refused before any socket as `CacheBootError::PlaintextRefused`; the same double opt-in enforced in the SMTP adapter, which previously *upgraded* a non-loopback plaintext request silently | `a_plaintext_endpoint_to_a_non_loopback_host_is_refused_even_with_the_opt_in`, `…needs_the_opt_in`, and the live `…fails_boot_before_any_socket` (TEST-NET-1 refused in < 100 ms where a connect would take 500 ms); C-C7 rewritten | R-B-M2 killed |
| 4 | W3C Trace Context Level 1 | **confirmed ×5**: repeated `traceparent` taken first; only the first `tracestate` field read; digit-first simple keys accepted (the test pinned `9rojo=1` valid); duplicate keys accepted; unknown flag bits re-rendered | parser: the §3.3.1.3.1 ABNF, one entry per key (§3.3.1.4), `TraceFlags::supported()` and a masked `render_traceparent` (§3.2.2.5.2, §4.3); transport: a repeated `traceparent` is invalid and counted, every `tracestate` field combined in arrival order (§3.3.1.1), the validated context handed to the handler | `renvor-core` unit and property tests (the round-trip property now against the outbound form, with a positive control), `renvor-http/tests/trace_context.rs` (+3) | R-A-M2, M2b, M3, M4; R-G-M4, M5 — all killed |
| 5 | Constitution VII generator obligation | **confirmed as a gap; not correctable in this round** — see `phase-010-limitations.md` L-14 and ADR-0031's qualified consequence | none invented; ADR-0031's compliance claim withdrawn in place; the routes (Phase 011 scope, a ruling on "ships", a waiver) are the maintainer's | — | — |
| 6 | SECURITY.md / CONTRIBUTING.md untruthful | **confirmed**: both described a Phase 002 kernel with "no transport, no listener"; SECURITY.md's scope named two crates | SECURITY.md lists every network surface (inbound HTTP, authentication, the three outbound clients, jobs, storage) and every workspace crate; CONTRIBUTING.md's intro and environment table rewritten; README gains the Phase 010 sentence | — (documents) | — |
| 7 | Retry deadline does not bound a running attempt | **confirmed**: `timeout(attempt_timeout, …)` with the deadline checked only between attempts | each attempt runs under `min(attempt_timeout, remaining)`; a deadline-governed cut returns `DeadlineExceeded` with that attempt counted and one event; the tie goes to the deadline | three new tests (a 10 s attempt under a 1 s deadline ends at exactly 1 s; a 2.5 s deadline cuts the third 1 s attempt at 2.5 s; the tie) with an in-binary event recorder, 80 stability runs | R-A-M1, M1b, M1c killed |
| 8 | Worker Boot never touches the store | **confirmed**: `initialise` spawned the loop and set Ready; a store with no schema booted Ready and warned for ever | one bounded `depth` probe (`STORE_PROBE_TIMEOUT` 10 s) before anything is spawned or registered; `WorkerBootError::StoreNotAnswering(category)` | `a_store_that_does_not_answer_fails_boot_and_never_starts_the_loop` (claim count 0), `a_hanging_store_fails_boot_within_the_probe_bound` | R-D1-M1 killed |
| 9 | Stop releases unbounded, outcomes discarded | **confirmed**: two `let _ = release(…)` sites with no bound; `released` incremented regardless; provider Stop always `Ok` | releases concurrent under `RELEASE_TIMEOUT` (2 s, `MAX_STOP_GRACE + RELEASE_TIMEOUT ≤ 30 s` pinned at compile time); `WorkerReport::{released, release_failed, release_timed_out}`; `released` counts confirmed releases only; the provider's Stop returns `LeasesNotReleased { failed, timed_out }` | four new tests (a hanging release bounded; a refused release counted and never marked released; the provider's unclean stop through `ApplicationBuilder`; the clean-stop positive control) | R-D1-M2, M3, M4 killed |
| 10 | Depth bound not enforced under concurrency | **confirmed** on both engines: eight racers held 4 against a bound of 3 in the first round | per-queue lock row `rv_job_queue` (migration 5), upserted by an autocommit statement, taken `FOR UPDATE` as the first statement of the enqueue transaction on both adapters; `depth ≤ bound` | `concurrent_enqueues_never_exceed_the_depth_bound` in the shared contract (8 racers × 3 rounds, half keyed), on the memory store and all four rows; contract 17 assertions | R-D2-M1, M2, M3 killed — M3 (lock after the key read) fails MySQL alone 3/3 while PostgreSQL passes: the REPEATABLE READ snapshot trap, measured |
| 11 | FR-011 typed configuration sections | **confirmed**: no capability implemented `ConfigSchema`; nothing ran in Validate | `renvor_config::SchemaSource::with_validator`, `layer_of`, `SectionKeys`; `CacheSection`, `JobsSection`, `MailSection`, `StorageSection`, `OtlpSection` with defaults, caps, `settings_from`/`settings_at`; `from_config` constructors on the cache, mail, and storage providers; C-C11 | per crate: a bound over its cap, a missing key, a malformed credential (present-and-empty included), a refused plaintext endpoint, each naming key, constraint, and layer with 0 providers initialised; live boots from a validated section against Valkey, Mailpit, and a real root; the nested-prefix test | R-H-M0…M3 killed |
| 12, 16 | Cache TTL: contract 24 h vs code and spec 7 days | **confirmed**: one wrong copy, in `capabilities-contract.md` C-C2 | corrected in the contract (1.1.0) with the transcription noted; no dated record rewritten | `bounds_have_defaults_and_hard_caps` already pinned 7 days | — |
| 13 | Filesystem bytes and content type not one atomic unit | **confirmed**: 565 of 931 reads paired one writer's bytes with the other's content type in the first RED run | one file per object (`RVO1` + length-prefixed content type + bytes), one rename; `head` reads only the header; corrupt files closed as `Unavailable`; `meta/` gone | the barrier race (0 inconsistent in 10 consecutive runs), `head_reads_only_the_header`, eight corrupt shapes, the read bound against the body | R-E-M1, M2, M3 killed |
| 14 | OTLP shutdown leaves a detached drain | **confirmed**: on timeout only a warning; the task ran on; the timeout was swallowed | on timeout the drain is aborted **and joined**; unexported spans counted (`renvor_otel_spans_unexported_total`) and returned as `OtelShutdownError::FlushTimedOut { unexported }`; the queue closed before the final sweep; a dropped handle aborts the drain | five unit tests (a pending exporter under a 200 ms bound returns within 500 ms with all 8 spans counted and the task finished; the clean-flush control; a panicking drain; drop; a span ending after the sweep) | R-F-M1, M2, M3, M4 killed |
| 15 | `CacheKey` accepts Unicode whitespace | **confirmed**: `a\u{a0}b` accepted | `char::is_control` and `char::is_whitespace` per character | `unicode_whitespace_is_refused_and_unicode_letters_are_not` | R-B-M1 killed |

**Found while correcting, recorded, not folded in**: the histogram registered as
`renvor_job_duration_seconds` where the contract and the family say `renvor_jobs_duration_seconds`
(the code now matches the contract); ADR-0036's description of an `HttpClient` bridge the shipped
design does not have (corrected in place, dated); the handler task detached at the stop grace
(recorded as L-16 and then, on the maintainer's ruling that it is a correctness blocker,
corrected the same day — §3b); `from_forwarded`'s trailing-parameter leniency (L-17); the database connection strings
still carrying their credentials (L-15); the kernel's diagnostics gate, which rejected twenty-nine
interpolated assertion messages the round's own tests had introduced — each rewritten to a fixed
message or a case index before any gate run was cited.

### 3b. The L-16 correction (2026-09-04, after the round) — one finding, one narrowly scoped correction

The maintainer read L-16 — the handler task detached at the stop grace — as a Phase 010
correctness blocker against FR-032, FR-033, bounded shutdown, and lease safety, not as a
retainable limitation, and directed one narrowly scoped correction. Source head `8b27580`;
gate results in `phase-010-evidence.md` §3a′. The working record is
`specs/010-…/evidence/correction-round.md` (its last section); this is the mirror.

| Finding | Verified as | Correction | Discriminating tests | Mutations |
|---|---|---|---|---|
| A handler task aborted at the stop grace is detached, not aborted; its lease is released while it may still run, concurrently with the next claimant | **confirmed** against `538c423`: `run_one` awaited the handler's `JoinHandle` inside the wrapper task; `run` aborted the wrappers with `abort_all()` and released the leases; dropping a `JoinHandle` detaches the task. Five tests written first failed on the defect's own assertions (RED in 0.49 s) | ownership of the handler task threaded through the in-flight state (`HandlerTask::{NotStarted, Running(AbortHandle), Terminated}`, registered under the lock the stop sweep takes, with a `stopping` mark read at registration); at the grace each job's scope is cancelled, its handler task aborted, the wrappers joined under a new `ABORT_JOIN_TIMEOUT` (2 s; `MAX_STOP_GRACE + ABORT_JOIN_TIMEOUT + RELEASE_TIMEOUT ≤ 30 s` pinned at compile time), and a lease released only for a handler marked terminated after its `JoinHandle` resolved; a handler holding its thread inside a poll keeps its lease (`WorkerReport::release_withheld`, one `warn`, the provider's `LeasesNotReleased { failed, timed_out, withheld }`); the handler timeout path joins the same way before the attempt is recorded. No timeout increased; cooperative cancellation not relied on alone | `a_handler_that_ignores_its_scope_cannot_run_after_the_worker_has_stopped` (the future dropped before `run` returns; not polled during 100 yields after), `the_lease_is_not_released_until_the_handler_task_has_terminated` (the store records the drop count at `release`), `a_new_claimant_cannot_overlap_with_the_old_handler_after_shutdown` (peak live executions 1 across two workers), `a_cooperative_handler_still_stops_cleanly_within_the_grace` (control: nothing aborted, released, or withheld), `a_handler_holding_its_thread_keeps_its_lease_rather_than_being_released_under_it` (a poll blocked at a barrier: withheld 1, `release` never called, row still leased; freed on the way out), `a_timed_out_handler_is_joined_before_its_attempt_is_recorded` (single-threaded, paused: `fail` sees the drop), `a_handler_spawned_after_the_stop_sweep_is_aborted_by_its_own_wrapper` (zero grace, paused: the sweep runs before registration; only the `stopping` mark aborts it), and the provider's `the_provider_reports_a_lease_kept_under_a_handler_that_did_not_terminate` | R-L16-M1…M8, all killed (the ledger's L-16 table) |

Recorded as found, not erased: the blocked-handler test wedged its own binary in the first RED
run (a Tokio runtime cannot shut down while a worker thread is inside a poll) and gained an
unwind-safe guard; the race-window test's first assertion (`dropped == 1`) was wrong for the path
it proves — a task aborted before its first poll never constructs the handler future — and reads
`polls == 0` (deterministic: 20/20 alone, 5/5 under the full parallel suite); the overlap test's
helper first waited on a count the defect itself satisfied early and now waits on the row's
state; the kernel's diagnostics gate rejected one interpolated `panic!` in the bounded-wait helper
before any gate run was cited. One provider text pin changed because the stop message now
carries a third count ("… failed, 0 timed out, and 0 withheld"); it still requires the exact
counts.

## 4. The closure decisions (2026-09-04)

Taken by the maintainer at the merge-authority checkpoint, in writing, and applied by the implementing
session: **L-16** accepted as proven and closed (§3b); principle VII's meaning of "ships" **not**
reinterpreted or weakened; **W-021** and **W-022** granted with every count re-derived from these
records at the grant — 136 mutations (88 + 40 + 8), thirteen §2 rows, sixteen §3 findings, one §3b
correction, 16 retained limitations — and the draft's stale "128 mutations" wording not carried;
**W-023** (Phase 009's auth starter) and **W-024** (this phase's capabilities) created as separate
waivers of principle VII's timing, each the third exception of its phase and recorded as such,
W-023 not retroactive; **L-14** deferred under them, **L-15** and **L-17** left open with their Phase
011 ownership; **ADR-0031 … ADR-0037** marked `accepted` under W-021 with the non-independent
disclosure, bound to head `5f26334b394f20ae86b3037ccb77a23705c40ed9` and tree
`47aeb8d8fda9e07bd5a4520406cef4eada44273c` (`phase-010-evidence.md` §10). The validation pass — the
`validation` agent, read-only, against the phase task's requirements — returned `needs_fixes` on
the mirror before this commit: four stale figures (the evidence's §7 contract versions, one line
behind each bump; the consumed draft's "128 mutations"; a `capabilities-contract.md`
cross-reference to observability-contract 2.0.0; "each with owner and target" where L-4 carries
none), each corrected or marked here, and one cosmetic duplicate `include` glob in
`renvor-jobs/Cargo.toml`, recorded and left (§10). This record was not independently reviewed
either.

## 5. What this record does not claim

- That any reviewer other than the maintainer read the code.
- That the research reports were verified beyond the claims this record lists as re-measured.
- That a survivor in the mutation ledger is "fine" without the stated reason (two survived; both
  are recorded with why and with what keeps them from becoming a defect).
