# Phase 010 — Mutation Ledger

**Companion to**: [`phase-010-evidence.md`](phase-010-evidence.md)
**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities
**Total**: **136 controlled mutations** — **88 in the phase** (85 killed by a named test, 1
killed by the harness wall clock — a hang, recorded as such — 2 survived as predicted and
investigated to a conclusion), **40 in the 2026-09-04 correction round, every one killed by a
named test**, and **8 in the same day's L-16 correction, every one killed by a named test** (the
two tables at the end).

A mutation is applied to the implementation, the test that should notice is named **in advance**,
the suite is run, and the pristine file is restored and re-run green. A mutation nothing catches is
recorded with why, never re-run until it passes. The harness (`mutate.py`, scratch-only) runs each
`cargo test` in its own process group under a 600 s limit and kills the group on expiry, after the
C-M15 incident below left a spinning test binary alive.

## Totals by batch

| Batch | Scope | Applied | Killed | Survivors |
|---|---|---|---|---|
| A | clock, retry, metrics, trace context (kernel) | 10 | 10 | 0 — one **prediction** wrong, see below |
| B | cache port, memory, Valkey | 9 | 9 | 0 |
| C | job port, memory store, worker | 15 | 14 (+1 by hang) | 0 |
| D | job store on the four rows | 9 | 8 | **1 — predicted equivalent** |
| E | mail port, recording, SMTP, auth bridge | 11 | 11 | 0 |
| F | storage port, memory, filesystem | 9 | 8 | **1 — equivalent** |
| G | redaction, JSON subscriber, Prometheus, health, cache metrics | 10 | 10 | 0 |
| H | OTLP exporter | 7 | 7 | 0 |
| I / J | trace context and fetch metadata in `renvor-http`; L-4 and L-11 dispositions | 7 | 7 | 0 |
| L | census extension (a misspelled job row) | 1 | 1 | 0 |
| | | **88** | **86** | **2** |

J-M3 (the store-failure event is not emitted) was **re-run on the corrected L-11 test** after the
final gate's diagnosis (`phase-010-review-record.md` §2, last row): KILLED by the same named test,
restore verified green. The totals above are unchanged.

## The entries worth reading

**A-M4b — a wrong prediction.** Removing only the base delay cap in `RetryPolicy::delay` was
predicted to *survive* ("the final `min` is a second layer"). It was **killed**: `powi` overflows
to `+inf` for an absurd attempt number and `Duration::from_secs_f64(+inf)` panics; the base cap is
what keeps the float finite before the conversion. The two caps are load-bearing for different
reasons, and the comment beside them now says so.

**C-M15 — a hang, not a kill.** Removing `break 'outer` in the worker's closed-gate branch turns
the run loop into a claim → refused permit → release → claim spin over the memory store with no
yielding `.await`, so the test's own `tokio::time::timeout(5 s, …)` can never fire — a timeout is a
future that needs the executor to be polled. The first harness run had no per-run limit and was
killed at ten minutes with the mutant still on disk; the pristine copy was restored from the
harness backup and verified by `diff` (one line differed: the mutant). Recorded as killed by the
wall clock, honestly; a test that kills it cleanly would need the spinning task on its own thread
and a runtime that does not wait for it at teardown.

**D-M6 — predicted survivor.** Dropping the `AND state = 0` re-check from the lease `UPDATE`
survived the four-racer claim race on both engines, as predicted: `FOR UPDATE SKIP LOCKED` already
prevents the double claim. The re-check is defence in depth against a surprising lock semantics
and is kept, with the survival recorded rather than a test invented to make it die.

**F-M1 — equivalent mutant.** Removing the explicit `..` segment check survived: the "no segment
ending in a dot" rule (a Windows rule) refuses `.` and `..` on its own. Kept, with the note that
relaxing the trailing-dot rule must never reopen traversal; the explicit check is what guarantees
that.

**The MySQL deadlock (batch D)** is not in the table. The reclaim `UPDATE` inside the claim
transaction deadlocked three of four racers on MySQL alone; it was a real failure before green,
diagnosed to InnoDB gap locks on an empty range and fixed by moving the reclaim to its own
autocommit statement on both adapters. Its detection is the MySQL contract run.

**The renvor-cache provider (batch L)** likewise: a new step 7 row found `renvor-cache --features
valkey` built alone had `rustls` with **no crypto provider**; fixed by naming `ring` on the crate's
own feature. Its detection is the gate.

## Per-batch tables

The per-mutation tables — mutation, prediction, result, killing test — are in the phase's private
record (`specs/010-…/evidence/batch-*.md`, gitignored) and in the session transcript; the totals
above are the mirror. Every "killed by" names the test; every real-server kill says so
(Valkey: B-M6, B-M7, B-M8; PostgreSQL/MySQL: D-M1…D-M9, J-M1, J-M2; Mailpit: E-M6, E-M7; the local
OTLP receiver: H-M7).

## Correction round (2026-09-04) — 40 mutations, 40 killed

Every mutation below was applied by hand or by a scratch driver, the test named in advance, the
pristine file restored (byte-checked) and re-run green. Ids are per slice; the RED-phase runs that
reintroduced a defect verbatim are listed where they served as the mutation.

| Id | Where | Edit | Killed by |
|---|---|---|---|
| R-A-M1 / M1b / M1c | `renvor-core` retry | the `min` with the remaining budget removed / a deadline cut read as an attempt timeout / the tie given to the attempt timeout | the three new deadline tests (10 s vs 1 s; `AttemptTimedOut` vs `DeadlineExceeded`; the tie's event label) |
| R-A-M2 / M2b | trace-context key grammar | a digit-first simple key / system-id accepted | `tracestate_grammar_is_enforced_member_by_member` |
| R-A-M3 | trace-context | the duplicate-key check deleted | `a_tracestate_key_appears_at_most_once` |
| R-A-M4 | `render_traceparent` | raw flags rendered | the unit flags test and the outbound-form property (`-02` vs `-00`) |
| R-B-M1 | `CacheKey::new` | byte-wise ASCII whitespace again | `unicode_whitespace_is_refused_and_unicode_letters_are_not` |
| R-B-M2 | `ValkeySettings::validate` | accepts everything | the two plaintext refusal tests |
| R-B-M3 | `ValkeySettings::connection_info` | the password never handed to the driver | the live Valkey boot test (`CredentialRefused`) |
| R-C-M1 | `SmtpSettings::validate` | accepts everything | the plaintext refusal test and the built-mailer test |
| R-C-M2 | `AuthMailSettings::link` | the token back in the query string | both bridge link tests |
| R-C-M3 | `SmtpMailer::connect` | credentials never handed to the transport | the live Mailpit boot test |
| R-D1-M1 | jobs provider | the Boot probe deleted | both Boot-probe tests (booted Ready over a refusing and a hanging store) |
| R-D1-M2 | worker `release_lease` | the release timeout removed | `a_release_that_hangs_at_stop_is_bounded_and_reported` (the test's own bound fired) |
| R-D1-M3 | worker `release_lease` | `released` incremented on a refused release | `a_release_that_fails_at_stop_is_counted_and_never_marked_released` (1.0 vs 0.0) |
| R-D1-M4 | jobs provider `stop` | `Ok` whatever the counts | `the_provider_reports_leases_it_could_not_release` |
| R-D2-M1 | `renvor-sqlx` enqueue | the `FOR UPDATE` lock removed | the depth race on both engines (4 against 3) |
| R-D2-M2 | `renvor-seaorm` enqueue | the lock removed | the depth race (PostgreSQL 4, MySQL 5) |
| R-D2-M3 | `renvor-sqlx` enqueue | the lock moved after the key read | the depth race on **MySQL alone**, 3 of 3 runs (5, 6, 6 against 3); PostgreSQL passed — the REPEATABLE READ snapshot trap the module docs describe |
| R-E-M1 | filesystem `write_atomically` | header and body by two renames | the two-writer barrier race (413 inconsistent of 1520) |
| R-E-M2 | filesystem `head` | size from the whole file | `head_reads_only_the_header` and three others |
| R-E-M3 | filesystem `read_header` | the magic check removed | `a_corrupt_object_is_reported_closed` (two bad-magic shapes with a valid length field were added *before* the run so the mutation could not slip past the length check) |
| R-F-M1 | OTLP `shutdown` | the abort removed | the timed-out flush test (the join hung; the test's bound stopped it) |
| R-F-M2 | OTLP `shutdown` | the count zeroed | same test (all eight spans expected) |
| R-F-M3 | OTLP `shutdown` | `Ok` after the bound | same test |
| R-F-M4 | OTLP drain | the queue not closed before the sweep | `a_span_ending_after_the_stop_sweep_is_refused_and_counted_rather_than_lost` |
| R-G-M1 | `cross_site_refused` | host-only comparison again | four gate unit tests |
| R-G-M2 / M3 | `EffectiveOrigin::parse` | the explicit port ignored / the scheme ignored | the origin unit tests, the router tests, the gate tests |
| R-G-M4 / M5 | `parse_inbound_trace` | the first `traceparent` taken / only the first `tracestate` field | `a_repeated_traceparent_is_invalid_and_counted_once` / the two combining tests |
| R-G-M6 | `host::normalise` | a garbage port stripped again | the host, origin, and router port tests |
| R-G-M7 | `identity::resolve_scheme` | a trusted proxy's `proto` ignored | the identity and router proxy tests |
| R-G-M8 / M9 | router carve-out | host-only comparison / the `Host` port ignored | `tests/effective_origin.rs` (5 and 6 failures) |
| R-H-M0 | `SchemaSource::validate` | the validator stored but never run | `a_validator_runs_in_validate_not_load_and_names_key_constraint_and_layer` |
| R-H-M1 / M2 / M3 | the cache / mail / storage sections | the validator not attached to the source | the sections' Validate-refusal tests (the build succeeded and 0 providers refused) |

Superseding note: the phase's batch I/J row (7 mutations on trace context and fetch metadata)
stands as a dated record; R-A-M2…M4 and R-G-M1…M9 are the mutations that pin the corrected
behaviour.

## L-16 correction (2026-09-04, after the round) — 8 mutations, 8 killed

Applied by a scratch driver (`l16/mutate.py`) to `crates/renvor-jobs/src/{worker,provider}.rs`
at the GREEN state that became `8b27580`, the whole `renvor-jobs` unit suite run against
each, the pristine files restored and byte-compared after every one. The tests named are the ones
that failed; every mutant compiled.

| # | Mutation | Killed by |
|---|---|---|
| R-L16-M1 | the stop sweep cancels the scope but never aborts the handler task (cooperative cancellation alone) | the three L-16 tests, the blocked-handler test, and the four older stop tests (every lease withheld instead of released) — 9 failures |
| R-L16-M2 | the join step removed: the wrappers aborted right after the sweep | 8 failures: the L-16 tests and the older stop tests (withheld, not released) |
| R-L16-M3 | every lease released, whether or not its handler terminated | `a_handler_holding_its_thread_keeps_its_lease_rather_than_being_released_under_it`, the provider's withheld test (the row was `Ready` under a live handler) |
| R-L16-M4 | the handler marked terminated at registration, before any join | 6 failures: the L-16 tests, the blocked-handler tests, and the hanging-release test |
| R-L16-M5 | the timeout path aborts without joining | `a_timed_out_handler_is_joined_before_its_attempt_is_recorded` alone |
| R-L16-M6 | the `stopping` mark not read at registration | `a_handler_spawned_after_the_stop_sweep_is_aborted_by_its_own_wrapper` alone |
| R-L16-M7 | the provider's stop ignores withheld leases | `the_provider_reports_a_lease_kept_under_a_handler_that_did_not_terminate` alone |
| R-L16-M8 | the join moved after the releases | 8 failures, as M2 |
