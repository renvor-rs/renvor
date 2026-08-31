# Phase 009 — Evidence

**Phase**: 009 — Authentication, sessions, tokens, and policies
**Closed**: 2026-08-31
**Reviewed head**: `0090c6784acdbfac863fc966e449245201a2b1fd`  
**Reviewed tree**: `dd1b27e32f79a41efaaaa6abc2e4d477262f326d`
**Companions**: [`phase-009-limitations.md`](phase-009-limitations.md) ·
[`phase-009-mutation-ledger.md`](phase-009-mutation-ledger.md) ·
[`phase-009-review-record.md`](phase-009-review-record.md) ·
[`phase-009-dependency-inventory.md`](phase-009-dependency-inventory.md)

> **This phase is `complete` under [W-020](waivers.md). No independent human review occurred.**
> W-020 is the **ninth consecutive** phase-level waiver of the same rule for the same reason.
> The seven decision records ADR-0024 … ADR-0030 are `accepted` under [W-019](waivers.md), a
> separate exception on a separate axis. Automated and maintainer reviews are **advisory**.

---

## 1. What this phase added

A transport-independent authentication domain (`renvor-auth`), its HTTP adapter
(`renvor-auth-http`), and the repository implementations for all four persistence rows.

- **Passwords** — Argon2id at RFC 9106 parameters, NFC normalisation before hashing, code-point
  length counting per NIST SP 800-63B-4 §3.1.1.2, and a blocklist with a stated boundary (ADR-0024).
- **Sessions** — opaque cookie sessions with the `__Host-` prefix rules stated as **two** rules
  rather than one (ADR-0026), a lifecycle with rotation on privilege change, and CSRF binding
  (ADR-0027).
- **Tokens** — optional signed JWT access tokens with **one algorithm per key**, and opaque refresh
  tokens whose rotation revokes the whole family on replay (ADR-0028).
- **Abuse control** — a counter space with a **provable finite row bound** (ADR-0029).
- **Audit** — a closed event vocabulary with no field that can carry arbitrary text.
- **Policies** — authorization enforced at real application call sites, not decoratively.
- **Migrations** — one auth migration set **per engine**, not one portable set (ADR-0025).

## 2. Acceptance criteria, from PLAN §20 Phase 009

| Criterion | Disposition |
|---|---|
| auth integration suite passes for all four persistence rows | **MET.** The census requires **63** suites — 12 tests on each direct-SQLx row, 11 on each SeaORM row, the refresh-rotation and abuse-control contracts on every row, and the end-to-end test application — and all 63 reported in on both toolchains |
| account enumeration and credential leakage tests pass | **MET.** Known and unknown accounts are asserted indistinguishable across the mailed flows; credential canaries sweep `Display`, `Debug`, errors, logs, panics and audit captures |
| revoked/expired credentials fail closed | **MET.** Storage errors refuse; refresh replay revokes the family |
| policy checks live in application operations | **MET.** T038 is closed through a real application-operation call site, not a unit test |
| password behavior matches the standards register | **MET, after a correction.** See §5 |

## 3. Verification — commands, platforms, results

Both gates ran on the exact closing head `0090c678`, sequentially, never concurrently, against
live PostgreSQL and MySQL, with stdin `/dev/null` to match CI. The runner refuses to start unless
`git rev-parse HEAD` equals that commit and the worktree is clean, so this table cannot be filled
from a different head. An earlier two-leg run was green at `ee461e0`; **those numbers are
deliberately not carried forward here**, because `ee461e0` is not the closing head.

| | leg A | leg B |
|---|---|---|
| Command | `cargo +1.94.0 xtask verify` | `cargo +stable xtask verify` |
| Steps | **11/11** (every sub-step ok: relay, census, both secret scans, both docs steps) | **11/11** (every sub-step ok: relay, census, both secret scans, both docs steps) |
| Exit | **0** | **0** |
| Tests | 117 binaries, **1804 passed, 0 failed**, 5 ignored | 117 binaries, **1804 passed, 0 failed**, 5 ignored |
| Census | **63/63** | **63/63** |
| Elapsed | 2h 52m 22s (06:47:24Z → 09:39:46Z) | 3h 54m 31s (09:39:46Z → 13:34:17Z) |
| HEAD before/after | `0090c678` → `0090c678` | `0090c678` → `0090c678` |
| Tree before/after | clean → clean | clean → clean |

**Platforms.** Local is macOS/aarch64. Ubuntu, macOS and **Windows** on both toolchains are
exercised by CI on the pull request; see §8.

### 3a. The gate must be run with stdin detached

`renvor-cli`'s transaction suite spawns concurrent `renvor new` children. Given a **real terminal on
stdin**, those children race for raw mode and one loses with `NotConnected` → `Code::Usage` → exit 2,
failing two tests that have nothing to do with this phase.

Proven by a controlled A/B in one tmux session, same head, toolchain, features and thread count,
differing only in stdin:

| stdin | result |
|---|---|
| tmux pty | `FAILED. 11 passed; 2 failed` |
| `/dev/null` | `ok. 13 passed; 0 failed` |

CI has no terminal, so CI never saw it. **Nothing in the repository said so**, and a maintainer
running the gate from a terminal will hit it. Recorded here and carried as a limitation.

## 4. Testing discipline

- **114 controlled mutations, 113 killed.** The single survivor was investigated to a conclusion
  rather than explained away — see the mutation ledger.
- **T056** proved the census extension detects a row that is renamed, feature-gated, or deleted, in
  two halves each: `cargo test` stays green, and the census fails naming the row.
- Every new behavioural requirement was taken **RED before GREEN**.
- Capability types (`Authorized<R>`, `Admitted`) are pinned by `compile_fail` doctests **with
  compiling controls**, because an error-code pin alone is inert on stable.

## 5. Defects found, and by what

Recorded by discovery mechanism, because the mechanism is the interesting part.

**By the security review** — `Admitted` derived `Copy`, making one counted attempt admit unlimited
calls; and the network axis keying on a full IPv6 address, which a routine `/64` walks through.

**By the requirements review** — FR-028's CSRF token unwired; one token repository serving two
tables; a test application that discarded the HTTP method.

**By a research agent** — `spec.md`, `research.md`, ADR-0024 and `password.rs` all claimed
NIST SP 800-63B-4 *"does not define 'character'"*. The standard contains a `SHALL` that does. The
**implementation was already correct**; only the justification was wrong — the more dangerous shape,
because nothing fails and the record quietly claims discretion where a mandate exists. The primary
source was re-fetched and confirmed before any file changed.

**By the repository's own gates, after both reviews reported** — an error code with no declared HTTP
status; a stale committed OpenAPI snapshot; ten credential-handling diagnostics that printed what
they asserted about; a publishable-package count that disagreed with its own table; and two
`gitleaks` matches on a synthetic token fixture.

### 5a. Why the gitleaks findings were invisible for a day

`cargo xtask verify` returns at the **first** failing step. CI had been failing at **step 4** on the
stale snapshot, so steps 5–11 never ran. Fixing step 4 is what revealed step 8. One red CI job and
four unrun checks look identical from outside.

## 6. Generated-project smoke tests

PLAN §6.2 requires these. **Phase 009 ships libraries and does not generate projects** — `renvor new
--auth` is reserved until Phase 011 and this phase deliberately did not activate it. The closest
executed evidence is the end-to-end test application in `renvor-auth-http`, which is censused as a
required row, plus the cheat-sheet execution recorded separately.

Stated as a **gap against §6.2**, not ticked off.

## 7. Documentation and migration notes

- `contracts/problem-details.md` moved to registry version 1.2.0, adding three codes; the table
  classifies **Add** as non-breaking and the committed OpenAPI snapshot was refreshed to match.
- Auth migrations are **per engine** (ADR-0025); `contracts/database-portability.md` §7's
  one-statement-per-file rule holds for all of them.
- `RELEASING.md` records `renvor-auth-http` at position 5 and the publishable count is now
  **thirteen**.

## 8. CI on the pull request

All checks ran against pull-request head `0090c678` (analysed as `refs/pull/45/head`).

| Check | Result |
|---|---|
| `verify (1.94.0)` | pass — the same single `cargo xtask verify` command, ubuntu |
| `verify (stable)` | pass — the same single command, ubuntu |
| `platform (macos-latest, 1.94.0)` | pass |
| `platform (macos-latest, stable)` | pass |
| `platform (windows-latest, 1.94.0)` | pass |
| `platform (windows-latest, stable)` | pass |
| `docs` | pass |
| `security` | pass |
| `package and verify without publishing` | pass |
| `dependency-review` | pass |
| `Analyze (rust)` / `Analyze (actions)` | pass |
| `attest rehearsal artifacts` | skipping (by design on a pull request) |
| `CodeQL` | pass — **zero open alerts** on `refs/pull/45/head` |

CI runs **one** command, `cargo xtask verify`, and the workflow states outright that it does not
reimplement the individual steps because *"drift is how a skipped check gets reported as a pass."*
The gate is therefore the same on both platforms, not a lighter variant on either.

### Code-scanning disposition at this head

`rust/cleartext-logging` alert **#90** (`renvor-auth/src/abuse.rs`) is **fixed**, closed at
2026-08-31T06:53:00Z by commit `020464a`, which removed the storage-error payload from a failing
test's diagnostic while leaving the assertion predicate byte-identical.

One alert was open at this head: **#127**, `rust/hard-coded-cryptographic-value` at
`renvor-auth/src/service.rs:831`. It is the **same fixture** as the already-dispositioned **#92**:
the line `"a long enough passphrase"` is byte-identical at `ee461e0`, `020464a` and `0090c678`
(sha256 `dd1835da…`) and sits inside `#[cfg(test)]`, which opens at line 556. A single analysis at
2026-08-31T06:53:00Z closed #92 and opened #127 at that same line, because the edit at L834-847
changed CodeQL's surrounding-context fingerprint. **CodeQL dismissals are fingerprint-bound, not
line-bound**, so editing near a dismissed fixture renumbers it. Dispositioned `used in tests` with
that lineage recorded in the dismissal comment.

## 8a. Task ledger at closure, stated as it actually is

The phase-local ledger carries **57 tasks**. Its state at closure:

| Status | Count | Notes |
|---|---|---|
| `coding_done` | 54 | each reconciled under T057 |
| `coding_done, partial` | 1 | T051 |
| `needs_fixes` | 1 | **T012** — FR-011 rehash-on-login has no production caller; carried as **L-2**, not closed |
| `coding_done` (T057) | 1 | this closure task, moved from `todo` when its documentation, evidence and limitations were written |
| `validated` | **0** | |
| `complete` | **0** | |

**No task was moved to `complete`, because none reached `validated`.** The workflow is
`todo → coding_done → validated → complete`, and `validated` is a state a separate validation pass
confers, not one the author may grant himself. Four of seven commissioned agents returned nothing
this phase, so that pass did not happen. Moving 57 tasks to `complete` on the author's own say-so
would be exactly the substitution W-020 exists to make visible rather than to hide.

What was done instead is stated where it can be checked: the reconciliation under T057, the
requirements mapping, the mutation ledger, and the two delivered validator reports. Those are
evidence; they are not the status transition, and this record does not present them as one.

**The phase closes with the ledger in this state deliberately.** A closing record that showed 57
`complete` rows would read better and be false.

## 9. Limitations

**23 retained limitations**, none closed. Every one carries an owner and a target phase in
[`phase-009-limitations.md`](phase-009-limitations.md). The three that would matter most to someone
deploying this:

- **L-1** — a password reset revokes nothing: sessions and refresh families survive it.
- **L-2** — FR-011 is not implemented; `needs_rehash` has no production caller.
- **L-6** — the signing key has no rotation path; the threat model records it as unmitigated.

## 10. What this phase did not do

No production SMTP, cache, jobs, or observability adapter — those are Phase 010. No project
generation for auth — Phase 011. No OpenAPI **document** assembly — the security schemes are
correct and nothing builds a document (L-16).
