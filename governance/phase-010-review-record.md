# Phase 010 — Review Record

**Companion to**: [`phase-010-evidence.md`](phase-010-evidence.md)
**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities

**No independent human review of this phase has occurred, and none is claimed.** Closure rests on
the **proposed** Phase 010 phase-closure waiver (`phase-010-proposed-waivers.md`), which would be
the **tenth consecutive** phase-level waiver of the same rule for the same reason if granted. Everything below is maintainer-commissioned and **advisory**, not independent.

## 1. What was commissioned, and what came back

| Agent | Purpose | Disposition |
|---|---|---|
| `research-cache-jobs` | RESP clients, job crates, retry crates: versions, licences, MSRV, advisories, graphs | **DELIVERED** on the first commission (one pass, 30 KB) |
| `research-mail-storage` | SMTP crates and every S3-compatible candidate against the same gates | **DELIVERED** (23 KB) |
| `research-observability` | `tracing-subscriber` redaction, metrics ports, OTLP transports, semantic conventions | **DELIVERED** (45 KB) |
| Codex review (`/codex:review`) of the whole branch diff | the closing review before the pull request | **see §4** |

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

## 3. Codex review

Recorded in §4 of `phase-010-evidence.md` after the pull request is open, with the exact head it
reviewed, its findings, and the disposition of each. Until then it is **NOT PERFORMED**, and an
absent review is never inferred clean.

## 4. What this record does not claim

- That any reviewer other than the maintainer read the code.
- That the research reports were verified beyond the claims this record lists as re-measured.
- That a survivor in the mutation ledger is "fine" without the stated reason (two survived; both
  are recorded with why and with what keeps them from becoming a defect).
