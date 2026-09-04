# Phase 010 — Mutation Ledger

**Companion to**: [`phase-010-evidence.md`](phase-010-evidence.md)
**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities
**Total**: **88 controlled mutations** — **85 killed by a named test**, **1 killed by the harness
wall clock (a hang, recorded as such)**, **2 survived as predicted and investigated to a
conclusion**.

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
