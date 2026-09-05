# Phase 011 — Review Record

**Companion to**: [`phase-011-evidence.md`](phase-011-evidence.md)
**Phase**: 011 — Generators, the auth starter, and the testing kit

**No independent human review of this phase has occurred, and none is claimed.** Everything below
is maintainer-commissioned and **advisory**: a validation agent's review of the W-023/W-024
removal-plan controls (twice — once at an interim head, once at the checkpoint head), the
repository's own gates, and the Codex review the maintainer runs with `/codex:review` at the
checkpoint (§3, written by the maintainer's session, not this one). The phase is **not closed**;
nothing here grants a waiver, accepts a decision record, or authorises a merge.

## 1. The validation agent's first pass — interim head `9dd5bc4` (tree `d1e1350`), 2026-09-05

Commissioned to check every removal-plan control of W-023 and W-024 against an executable proof,
and to name what was prose. It exported the head read-only with `git archive` (the worktree was
mid-batch and briefly did not compile) and ran the database-free suites there; it watched the
first census run finish green (82 rows). **Verdict: 6/10, not closable yet.** Its table found
every control implemented and green, three controls **partial**, and the closure items missing:

| Finding | Disposition |
|---|---|
| No governance evidence names a head and tree; the census binaries were compiled ~80 s before the head was committed | `phase-011-evidence.md` §4 and §10 bind the census, the controls, and the gates to the checkpoint head and tree; every census cited there was run on a clean, committed tree |
| The three FR-032 negative controls (rename, delete, `cfg`-gate a row) had no evidence | run on the checkpoint head, each failing the census by name (`phase-011-evidence.md` §4); the first attempt's rename was a no-op (a `fn` that does not exist — the rows are a macro) and is recorded as such |
| L-14 and the waiver rows untouched | closed only after the second pass (§2), in `waivers.md` and `phase-010-limitations.md`, against the named head |
| **Defect 1** — `renvor.toml`'s comments named `RENVOR_AUTH__CSRF_KEY`, `RENVOR_AUTH__ABUSE_KEY`, `RENVOR_CACHE__PASSWORD` (double underscore) while the starter reads the single-underscore names | fixed in the template; the generated `.env.example` and the starter's own reader already agreed |
| **Defect 2** — the FR-015 sweep checked the bodies of a fixed reply subset for the password and the CSRF token only: no headers, no cookie value, no mailed token, no positive control | the generated test now sweeps every header value and the body of each reply (`leaks`), asserts the session cookie's value never appears in a body, asserts the mailed verification token comes back in no response, and starts with a positive control (a planted header the sweep must see) |
| **Defect 3** — the verification-mail confirmation printed `SKIPPED` and passed without `RENVOR_TEST_SMTP_API_URL`, with no requirement guard | with `RENVOR_TEST_REQUIRE_CAPABILITIES` set (the gate sets it, and the matrix forwards it) a missing sink fails the generated test; the census cannot pass on a skipped mail flow |
| **Defect 4** — the generated mail route fell back to `localhost` as the sender domain and the jobs route to a `default` queue when the section could not be read | both routes answer `503 Unavailable` instead; no invented value |
| **Defect 5** — `contracts/verification-sequence.md` said `version: "2.2.0"` under a 2.3.0 status line | `2.3.0` |
| **Defect 6** — a CI comment cited the old 45-minute bound; the job says 75 | the comment says 75 |
| **Defect 7** — batch A's mutations M-A1 … M-A8 were referenced and recorded nowhere; batch C had no mutation ledger | batch A re-run in full (ten mutations, one survivor fixed); batch C's mutations are the census controls; `phase-011-mutation-ledger.md` |
| **FR-024** — "unselected appears nowhere" was asserted by `Cargo.toml` text and file presence, not by the lock closure the requirement names | `assert_recorded` now walks the placed project's `Cargo.lock` from the manifest's runtime dependencies and asserts each `renvor-<capability>` and `renvor-auth-http` is reachable exactly when selected. The walk's first run found what text search could not: `renvor-auth` reaches every database-backed starter through the persistence adapters, chosen or not — recorded as limitation L-13, asserted as such |
| **AC-002** — the migration ledger was dropped and never read back | the generated test reads the ledger through the application's own driver and asserts one row per shipped `*.up.sql` |
| W-024 (3c) — no CI run existed for the branch | the pull request's checks are recorded in `phase-011-evidence.md` §10 |

The reviewer also confirmed, at that head: no hard-coded secret or credential in templates, fixtures,
or the two generated test files; `random_key()` draws from the standard library's entropy; no inert
choice in any template.

## 2. The validation agent's second pass — the checkpoint head

Commissioned against head `d8e3a445363da965f40470b12100082d02c68254` (tree
`ea7e3a52d0db8198b14508041e1622a327780317`) with the census, the three head-bound controls, and
the mutation logs in hand; forbidden to modify tracked files, to run the gate or the census, or to
compile the starter rows (a control script was running beside it). It ran the database-free suites
in the worktree (`renvor-cli` 288 + 5 + 4 + 1, `renvor-testkit --all-features` 22, `xtask` 36),
re-applied six mutations in a `git archive` export of the head (M-A1, M-B-01, M-D1, M-F2, M-F4,
M-F7 — every one killed by its named test, every file restored byte-identical), and swept every
template, fixture, snapshot, generated test, cited log, and record for credentials (**none**; two
known benign canaries elsewhere in the tree).

**Every removal-plan control of W-023 and W-024: satisfied**, with file:line for each — the
explicit refusals and the governed-choice pins, the pty parity, the `renvor.toml` persistence, the
wiring and the lock-closure walk, the compile-and-start verification, the four full rows with the
ledger read back, the lean rows against their real servers, the census on this head with both
`REQUIRE` flags set, the three negative controls fired on this head, no tag or release. Every
first-pass fix verified in code, not prose.

**Verdict: NOT CLOSABLE at `d8e3a44`** — for one gate regression and for the state of the records:

| Finding | Severity | Disposition |
|---|---|---|
| **D-1** `cargo test -p xtask` fails `publication_order_is_topological`: the testkit's new optional `renvor-http` edge (feature `http`) puts it before `renvor-openapi` in the release dry-run's publication order, so step 4 of `cargo xtask verify` cannot pass on this head | HIGH | `renvor-testkit` moved after `renvor-http` in `release-dry-run.yml` and `RELEASING.md`; the test passes; the closure head is the one carrying this fix, gated there (§evidence §10) |
| **D-2** closure prerequisites unwritten: five `PENDING` fields; a review-record row promising the pull request's checks while no pull request existed | HIGH (governance) | filled at the checkpoint, after the gates and the pull request — this record's §3 and the evidence's §10 |
| **D-3** the mutation ledger said the controls were re-run on the final head before those runs existed | MEDIUM | the three outcomes on `d8e3a44` are quoted; the sentence now names both heads |
| **D-4** two mutations cited without a log (M-A1's re-application, M-F2) | MEDIUM | both re-run to logs (`m-a1-rerun.log`, `m-f2.log`) and independently reproduced by this pass |
| **D-5** "fourteen starter-matrix rows" — `ROW_EVIDENCE` holds eighteen plus parity | LOW | corrected |
| **D-6** "twelve `auth_repositories` rows" — eight | LOW | corrected |
| **D-7** the `renvor.toml` comment's variable names are fixed but nothing pins them | LOW (gap, not a false claim) | a manifest-render test now asserts the single-underscore names and refuses a double underscore (mutation M-F10) |
| **D-8** `cargo doc --all-features` "green by hand" cited no run | LOW | cited, and re-run by this pass |

On 010/L-14 the reviewer's reading: the recorded measurement is sufficient in substance to close
it, and it must be closed against the head that carries D-1's fix with the gate green there, not
against `d8e3a44`. That is what the closure records do (§evidence §10). Task statuses set by the
reviewer: #101, #102, #103 `validated`; #105, #106 `needs_fixes` (D-1 and the records).

## 3. Codex review at the checkpoint

**Not yet performed.** The maintainer runs `/codex:review` on the pull request head at the
checkpoint; the implementing session cannot invoke it and stops to hand over the command. Its
findings, and the one bounded correction round the directive allows, are recorded here by the
maintainer's session when they exist — not before.

## 4. What this record does not claim

- That any review above was independent. The reviewer is an agent commissioned by the same
  session that wrote the code; the maintainer is the sole human.
- That W-023 or W-024 is closed by anything in this file. Closure is a ledger entry in
  `waivers.md`, bound to a head and tree, written only after §2 reports every control satisfied.
- That the phase is complete. The merge-authority checkpoint is the maintainer's decision.
