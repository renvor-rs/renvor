# Phase 010 — Evidence

**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities
**State**: **implemented on `feat/phase-010-operational-capabilities`; NOT closed.** Closure is
the maintainer's decision at the merge-authority checkpoint.
**Base**: `c57b4fb131b1c254dd89ce21fd78aae2ac2f0b37` (origin/main)
**Reviewed head**: recorded in §3 with the gate results
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
| missing required capabilities fail during startup | **MET.** A consumer needing `cache`, `jobs`, `mail`, or `storage` with no provider fails at Register naming both ends (SC-001, tested per crate); an unreachable Valkey, a refused SMTP credential, an unwritable storage root, and a non-answering worker store each fail Boot with a closed category |
| MySQL applications never acquire PostgreSQL dependencies implicitly | **MET.** `xtask` step 7 rows `renvor-sqlx + db-mysql,jobs` and `renvor-seaorm + db-mysql,jobs` assert `sqlx-postgres` absent with `sqlx-mysql` and `renvor-jobs` as controls |
| retries bounded and observable | **MET.** `RetryPolicy` caps attempts (≤ 100), delays (≤ 1 h) and deadlines; the worker's exact attempt counts are asserted (3 calls for `max_attempts = 3`, then dead-letter) with the events and counters a subscriber and registry actually receive; an expired lease at the last attempt dead-letters on all five stores |
| trace context propagates safely | **MET.** Parser fuzzed (3 × 10 000 cases, no panic, round-trip); the router records only validated fields, ignores and counts invalid input, never echoes, never derives `RequestId`; a job carries the context into its execution span |
| capability-disabled builds exclude their dependencies | **MET.** 22 new step 7 rows with controls; the lean facade resolves no capability crate; each `capability-*` feature resolves exactly its crate |

## 3. Verification — commands, platforms, results

Both legs ran sequentially on `ff84cd8` with `CARGO_INCREMENTAL=0` and stdin detached, against
live PostgreSQL 17, MySQL 8.4, Valkey 9.1.1, and Mailpit 1.29.1 in local containers, one gate process
at a time, with `cargo clean` between the legs for disk space. The commit that records this table
differs from `ff84cd8` only by governance text. Two earlier runs failed and are not reused: on
`73e4a9a` leg A stopped at step 4 (the L-11 event test); on `3bfb552` leg A passed steps 1–6
and stopped at step 7 (the per-driver compile). Both failures, their diagnoses, and the fixes
are the last rows of `phase-010-review-record.md` §2.

| | leg A | leg B |
|---|---|---|
| Command | `cargo +1.94.0 xtask verify` | `cargo +stable xtask verify` |
| Toolchain | rustc 1.94.0 (4a4ef493e 2026-03-02) | rustc 1.97.1 (8bab26f4f 2026-07-14) |
| Head | `ff84cd8` | `ff84cd8` |
| Steps | 9/9 ok (12 step lines: step 4 reports three, step 8 two) | 9/9 ok (12 step lines) |
| Exit | 0 | 0 |
| Tests | 1966 passed, 0 failed, 5 ignored (the sum of all 138 `test result` lines) | 1966 passed, 0 failed, 5 ignored (138 lines) |
| Census | 67/67 rows reported in | 67/67 rows reported in |
| Elapsed | 10 min 28 s | 10 min 46 s (cold build after `cargo clean`) |

**Platforms.** Local is macOS/aarch64. Ubuntu, macOS and Windows on both toolchains are exercised
by CI on the pull request.

## 4. Reviews

The three commissioned research agents delivered on the first commission and every decisive
claim was re-measured (`phase-010-review-record.md` §1). The Codex review of the whole branch diff
is recorded here after the pull request opens: head reviewed, findings, dispositions.
*(pending)*

## 5. Defects found, and by what

See `phase-010-review-record.md` §2: eight defects found by the repository's own gates and real
servers after batches were green — an InnoDB gap-lock deadlock, an unbounded re-claim, fourteen
credential-file diagnostics, a cache adapter with no crypto provider, a resource missing from
the OTLP wire, a test binary aborted by a destructor, a `tracing-core` callsite-interest race
that dropped a test's recorded event, and two job-store suites compiled into a database-only
build — each fixed at the root and pinned — plus one secret-scanner false positive on the
redaction canary, recorded as FP-004 with the injection proof the scanner's policy demands.
The last three were found by the closing runs themselves: no full run on this branch had
reached steps 7–9 before them.

## 6. Testing discipline

- Red/green per batch with named mutations before each commit; **88 controlled mutations**
  (`phase-010-mutation-ledger.md`).
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

`phase-010-limitations.md`: 13 retained, each with owner and target; 2 Phase 009 rows closed with
measurement.

## 9. What this phase did not do

No S3-compatible adapter (gated out, recorded). No real TLS handshake in CI. No public release,
tag, crate publication, website deployment, repository-setting change, or environment change. No
Phase 011 work started.
