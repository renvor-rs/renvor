# Phase 010 — Evidence

**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities
**State**: **implemented on `feat/phase-010-operational-capabilities`, reviewed, corrected in one
bounded round and then in one maintainer-directed L-16 correction (both 2026-09-04); NOT
closed.** Closure is the maintainer's decision at the merge-authority checkpoint.
**Base**: `c57b4fb131b1c254dd89ce21fd78aae2ac2f0b37` (origin/main)
**Reviewed head**: `1328dd3` (the head Codex reviewed); **correction-round source head**: `8099f017`, gate results in §3a; **L-16 correction source head**: `8b275803`, gate results in §3a′
**Companions**: [`phase-010-limitations.md`](phase-010-limitations.md) ·
[`phase-010-mutation-ledger.md`](phase-010-mutation-ledger.md) ·
[`phase-010-review-record.md`](phase-010-review-record.md) ·
[`phase-010-dependency-inventory.md`](phase-010-dependency-inventory.md) ·
[`phase-010-proposed-waivers.md`](phase-010-proposed-waivers.md)

> **No independent human review of this phase has occurred, and none is claimed.** The seven
> decision records ADR-0031 … ADR-0037 are **`proposed`** and carry no authority. Two waivers are
> drafted and **not granted**. Automated and maintainer reviews are **advisory**.

---

## 1. What this phase added

Five capability crates, each a narrow port with a deterministic substitute and an adapter behind
an off-by-default feature, plus the operational adapters Phase 009 declined to ship:

- **Kernel primitives** (`renvor-core`): an injectable `Clock`, a pure bounded `RetryPolicy`
  and `retry` helper (ADR-0037), a bounded-cardinality metrics port, a total fail-closed W3C
  trace-context parser with property-based fuzzing, and the semantic-convention names.
- **Cache** (`renvor-cache`, ADR-0033): `Cache` with bounded keys, values and TTLs; `MemoryCache`;
  a Valkey adapter over `redis` with rustls native roots and **one** crypto provider.
- **Jobs** (`renvor-jobs`, ADR-0032): `JobStore` with entropy identifiers, leases, bounded
  reclaim, idempotency keys and a depth bound; `MemoryJobStore`; a worker on kernel work permits
  with panic containment, per-job timeouts, retry scheduling, and structured events; the store
  on **all four persistence rows** (`renvor-sqlx`, `renvor-seaorm` × PostgreSQL, MySQL) behind
  `jobs`, with one migration set per engine.
- **Mail** (`renvor-mail`, ADR-0034): `Mailer` with injection-free bounded messages;
  `RecordingMailbox`; an SMTP adapter over `lettre` (TLS by default, plaintext only loopback +
  flag); a bridge to `renvor_auth::MailPort` rendering from configured base URL and sender.
- **Storage** (`renvor-storage`, ADR-0035): `ObjectStore` with traversal-proof keys and bounds;
  `MemoryStore`; a filesystem adapter rooted in a `cap-std` capability with atomic writes. **No
  S3 adapter** — every candidate failed a gate; the routes are recorded.
- **Observability** (`renvor-observability`, ADR-0036): a JSON subscriber with central
  redaction of event **and** span fields (Renvor's formatter, because the crate's bypasses a
  field formatter — measured); a Prometheus renderer cross-checked against a reference encoder;
  health documents and `/healthz`, `/readyz`; a bounded OTLP/HTTP exporter behind `otel` with
  counted drops and a redacting exporter.
- **Transport**: `renvor-http` parses inbound `traceparent`/`tracestate` as untrusted bounded
  input and attaches bounded fetch metadata to every request; **009/L-4** is closed at the
  cookie-authenticated unsafe gate; **009/L-11** is closed by a recorded infrastructure event.
- **Facade**: five features, module and narrow root re-exports, one example per capability,
  and a port-boundary scanner; the tree's "arrives in Phase 010" promises corrected.

## 2. Acceptance criteria

| Criterion (task brief) | Disposition |
|---|---|
| missing required capabilities fail during startup | **MET.** A consumer needing `cache`, `jobs`, `mail`, or `storage` with no provider fails at Register naming both ends (SC-001, tested per crate); an unreachable Valkey, a refused SMTP credential, an unwritable storage root, and a non-answering worker store each fail Boot with a closed category. **The worker clause was false at the reviewed head** (review finding 8: Boot never touched the store) and is true since the correction round, pinned by `a_store_that_does_not_answer_fails_boot_and_never_starts_the_loop` and `a_hanging_store_fails_boot_within_the_probe_bound`. A typed configuration section that breaks a bound, misses a key, or carries a malformed credential fails **Validate** naming key, constraint, and layer with 0 providers initialised (FR-011, C-C11; review finding 11) |
| MySQL applications never acquire PostgreSQL dependencies implicitly | **MET.** `xtask` step 7 rows `renvor-sqlx + db-mysql,jobs` and `renvor-seaorm + db-mysql,jobs` assert `sqlx-postgres` absent with `sqlx-mysql` and `renvor-jobs` as controls |
| retries bounded and observable | **MET.** `RetryPolicy` caps attempts (≤ 100), delays (≤ 1 h) and deadlines; the worker's exact attempt counts are asserted (3 calls for `max_attempts = 3`, then dead-letter) with the events and counters a subscriber and registry actually receive; an expired lease at the last attempt dead-letters on all five stores |
| trace context propagates safely | **MET.** Parser fuzzed (4 property tests × 10 000 cases, no panic; the outbound form zeroes every undefined flag bit and round-trips a defined-bits-only header byte-identically); the router records only validated fields, treats a repeated `traceparent` as invalid and counted, combines `tracestate` fields in arrival order, enforces the Level 1 key grammar and one entry per key, never echoes, never derives `RequestId`; a job carries the validated context into its execution span. Five Level 1 departures at the reviewed head (review finding 4) are corrected |
| capability-disabled builds exclude their dependencies | **MET.** 22 new step 7 rows with controls; the lean facade resolves no capability crate; each `capability-*` feature resolves exactly its crate |

## 3. Verification — commands, platforms, results

### 3a′. The L-16 correction's head, `8b275803` (2026-09-04, after the round)

Both legs ran sequentially on `8b275803` — the one source commit of the L-16 correction, on top of
`538c423` — with `CARGO_INCREMENTAL=0` and stdin detached, against the same live PostgreSQL 17,
MySQL 8.4, Valkey 9.1.1, and Mailpit 1.29.1, one gate process at a time, with the capability
credentials in their own variables. The commit that records this table differs from `8b275803`
only by governance text. Leg B ran twice: the first attempt was killed by the session's tooling
in its test step, ten minutes after the driver started (its partial log is kept beside the
rerun's); the rerun, detached and alone, is the one in the table. Leg A's single run had
completed before the kill.

| | leg A | leg B |
|---|---|---|
| Command | `cargo +1.94.0 xtask verify` | `cargo +stable xtask verify` |
| Toolchain | rustc 1.94.0 (4a4ef493e 2026-03-02) | rustc 1.97.1 (8bab26f4f 2026-07-14) |
| Head | `8b275803` | `8b275803` |
| Steps | 9/9 ok | 9/9 ok |
| Exit | 0 | 0 |
| Tests | 2065 passed, 0 failed, 5 ignored (139 `test result` lines) | 2065 passed, 0 failed, 5 ignored (139 `test result` lines) |
| Census | 67/67 rows reported in | 67/67 rows reported in |
| Elapsed | 9 min 29 s (step 4: 7m 40s) | 8 min 55 s (step 4: 7m 23s) |

The test total rose by the seven L-16 tests and the provider's withheld test; no test was
removed. The eight mutations are in the ledger's L-16 table.

### 3a. The correction round's head, `8099f017` (2026-09-04)

Both legs ran sequentially on `8099f017` — the last source commit of the correction round, thirteen
commits after the checkpoint — with `CARGO_INCREMENTAL=0` and stdin detached, against the same live
PostgreSQL 17, MySQL 8.4, Valkey 9.1.1, and Mailpit 1.29.1, one gate process at a time, with the
capability credentials in their own variables (`verification-sequence.md` 2.2.0). The commit that
records this table differs from `8099f017` only by governance text. The workspace test suite had
also been run in full on the same tree before the commits (2057 passed, 0 failed, 5 ignored).

| | leg A | leg B |
|---|---|---|
| Command | `cargo +1.94.0 xtask verify` | `cargo +stable xtask verify` |
| Toolchain | rustc 1.94.0 (4a4ef493e 2026-03-02) | rustc 1.97.1 (8bab26f4f 2026-07-14) |
| Head | `8099f017` | `8099f017` |
| Steps | 9/9 ok | 9/9 ok |
| Exit | 0 | 0 |
| Tests | 2057 passed, 0 failed, 5 ignored (139 `test result` lines) | 2057 passed, 0 failed, 5 ignored (139 `test result` lines) |
| Census | 67/67 rows reported in | 67/67 rows reported in |
| Elapsed | 12 min 09 s (step 4: 8 min 45 s) | 12 min 45 s (step 4: 9 min 59 s) |

The test total rose from 1966 to 2057: the round's discriminating tests (the sections, the origin
resolution, the depth race in the shared contract, the worker's Boot and Stop, the storage race,
the OTLP shutdown, the retry deadline, the trace-context grammar) and no test was removed.

### 3b. The checkpoint head, `a0f837b` (2026-09-04, before the review)

Both legs ran sequentially on `a0f837b` with `CARGO_INCREMENTAL=0` and stdin detached, against
live PostgreSQL 17, MySQL 8.4, Valkey 9.1.1, and Mailpit 1.29.1 in local containers, one gate process
at a time, with `cargo clean` between the legs for disk space. The commit that records this table
differs from `a0f837b` only by governance text. Four earlier runs are not reused: on `73e4a9a` leg A stopped at step 4 (the L-11 event test); on `3bfb552` at step 7 (the per-driver compile); `ff84cd8` passed both legs before the pull request's checks found four more defects; on `bc3a166` leg A stopped at step 6 (a versionless dev-dependency is a wildcard `deny.toml` bans). Each failure, its diagnosis, and its fix are rows of `phase-010-review-record.md` §2.

| | leg A | leg B |
|---|---|---|
| Command | `cargo +1.94.0 xtask verify` | `cargo +stable xtask verify` |
| Toolchain | rustc 1.94.0 (4a4ef493e 2026-03-02) | rustc 1.97.1 (8bab26f4f 2026-07-14) |
| Head | `a0f837b` | `a0f837b` |
| Steps | 9/9 ok (12 step lines: step 4 reports three, step 8 two) | 9/9 ok (12 step lines) |
| Exit | 0 | 0 |
| Tests | 1966 passed, 0 failed, 5 ignored (138 `test result` lines) | 1966 passed, 0 failed, 5 ignored (138 `test result` lines) |
| Census | 67/67 rows reported in | 67/67 rows reported in |
| Elapsed | 10 min 38 s | 10 min 49 s (cold build after `cargo clean`) |

**Platforms.** Local is macOS/aarch64. Ubuntu, macOS and Windows on both toolchains are exercised
by CI on the pull request (§4).

## 4. Reviews

The three commissioned research agents delivered on the first commission and every decisive
claim was re-measured (`phase-010-review-record.md` §1).

**Continuous integration on pull request #61.** The first run, on `c5bf188`, passed 8 of 14 checks (both CodeQL analyses, docs, security, both macOS platform legs, and both `verify` legs — the full nine-step sequence on Ubuntu with the Valkey and Mailpit containers) and failed 5: the dependency review's licence list, the release dry run's packaging order, CodeQL on two test literals, and both Windows platform legs on one boot category. Each is a row of `phase-010-review-record.md` §2 with its cause and fix; no check was weakened. The checks on the pull request's final head are the pull request's own record and are quoted in the checkpoint report.

**Codex review.** **PERFORMED by the maintainer** with `/codex:review` on the pull request head `1328dd3` (the implementing session could not invoke it and stopped once to hand over the command). **Sixteen findings**, every one verified against the tree before a change and every one confirmed; **fifteen corrected at the root in one bounded round** (2026-09-04) with a RED→GREEN test and a controlled mutation each, and nothing — no test, gate, scanner, contract, or acceptance criterion — weakened; **one (finding 5, constitution VII's generator obligation) confirmed and not correctable** without Phase 011 scope or a maintainer ruling, recorded as L-14 with ADR-0031's compliance claim withdrawn in place. The table of findings, corrections, tests, and mutations is `phase-010-review-record.md` §3.

## 5. Defects found, and by what

**The Codex review** (`phase-010-review-record.md` §3) found sixteen defects and gaps after the
checkpoint — two of them security-relevant at the transport (the host-only origin comparison) and
the credential surfaces (secrets in URLs, plaintext to any Valkey host), one a false clause in
this file's own acceptance table (the worker's Boot), one a contract stating a bound it did not
enforce (`depth ≤ bound + writers − 1`), and one a specification requirement with no
implementation (FR-011). Fifteen are corrected and pinned; one is L-14.

**The maintainer's reading of L-16** (`phase-010-review-record.md` §3b): the correction round had
recorded the handler task detached at the stop grace as a retained limitation; the maintainer
ruled it a correctness blocker against FR-032, FR-033, bounded shutdown, and lease safety, and it
was reproduced by five tests written first and corrected the same day at the root — the handler's
own task aborted and joined before its lease is released, and a lease under a handler that cannot
be dropped withheld and reported rather than released. Eight mutations, eight killed.

See `phase-010-review-record.md` §2: ten defects found by the repository's own gates, real
servers, and the pull request's platform legs after batches were green — an InnoDB gap-lock deadlock, an unbounded re-claim, fourteen
credential-file diagnostics, a cache adapter with no crypto provider, a resource missing from
the OTLP wire, a test binary aborted by a destructor, a `tracing-core` callsite-interest race
that dropped a test's recorded event, two job-store suites compiled into a database-only
build, a platform-dependent boot category in the Valkey adapter (Windows), and a versioned
dev-dependency that cycled the release packaging order — each fixed at the root and pinned —
plus one secret-scanner false positive on the redaction canary (FP-004, with the injection proof
the scanner's policy demands), one licence allow-list found out of step with `deny.toml`, and two
test fixtures that CodeQL read as hard-coded passwords, now built at run time. Five of these were
found by the closing runs and the pull request itself: no full run on this branch had reached
steps 7–9 before them, and no Windows leg had run at all.

## 6. Testing discipline

- Red/green per batch with named mutations before each commit; **88 controlled mutations** in
  the phase, **40 more in the correction round**, and **8 in the L-16 correction**, every one
  killed (`phase-010-mutation-ledger.md`).
- Every adapter against a real server: Valkey 9.1.1, PostgreSQL 17, MySQL 8.4, Mailpit 1.29.1,
  a local OTLP receiver, a real filesystem; each with a redaction canary sweep.
- The four-row census extended to 67 rows and proved to fail on a misspelled row.
- Both kernel cross-crate gates (diagnostics, deadlines) run before every batch commit after
  batch B was found to have been committed gate-red.
- No test, gate, scan, or assertion was weakened to obtain green output; every failure was
  diagnosed before a retry, and every test-side correction is recorded in the batch evidence
  with what changed in the assertion.

## 7. Documentation

Contracts: `capabilities-contract.md` 1.0.0 and `jobs-contract.md` 1.0.0 (new),
`observability-contract.md` 2.0.0, `verification-sequence.md` 2.1.0. Crate READMEs are
pre-release and truthful. No canonical documentation source is recreated in this repository.

## 8. Limitations

`phase-010-limitations.md`: 16 retained, each with owner and target (13 at the checkpoint; L-14
to L-17 added by the correction round, of which L-16 was closed the same day with measurement);
2 Phase 009 rows closed with measurement.

## 9. What this phase did not do

No S3-compatible adapter (gated out, recorded). No real TLS handshake in CI. No public release,
tag, crate publication, website deployment, repository-setting change, or environment change. No
Phase 011 work started.
