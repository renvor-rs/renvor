# Phase 009 — Mutation Ledger

**Companion to**: [`phase-009-evidence.md`](phase-009-evidence.md)
**Phase**: 009 — Authentication, sessions, tokens, and policies
**Total**: **114 controlled mutations**, **113 killed**, **1 survivor investigated to a conclusion**.

A mutation is applied to the implementation, the test that should notice is named **in advance**, and
the suite is run. A mutation nothing catches is a test that measures nothing, and is recorded as such
rather than quietly re-run until it passes.

## Totals by batch

| Batch | Scope | Applied | Killed | Survivors |
|---|---|---|---|---|
| F | password hashing, sessions, cookies | 38 | 38 | 0 |
| G | access tokens, refresh rotation | 25 | 24 | **1 — resolved, see below** |
| G2 | refresh-family revocation | 14 | 14 | 0 |
| H | policies | 5 | 5 | 0 |
| CodeQL correction | cleartext-logging remediation | 4 | 4 | 0 |
| I / J | abuse control, audit, transport | 25 | 25 | 0 |
| **L (T056)** | **census extension** | **3** | **3** | **0** |
| | | **114** | **113** | **1** |

## The one survivor, and why it is the most useful entry here

`G-M3` removed the **backend layer's** issuer check and survived. The first harness recorded that as
*"the test is decorative"* — **and that was wrong**. An explicit issuer check twenty lines below still
refused the token, so the suite was correct and the diagnosis was not.

A second experiment resolved it properly, predicting each outcome before running it:

| Mutation | Predicted | Observed |
|---|---|---|
| `G-M14` issuer, backend layer only | survive | survive |
| `G-M15` issuer, explicit layer only | survive | survive |
| `G-M16` issuer, **both** layers | kill | **kill** |
| `G-M17` audience, explicit layer only | survive | survive |
| `G-M18` audience, **both** layers | kill | **kill** |

5/5 as predicted. The two layers are **individually redundant and jointly load-bearing** — which is
what "belt and braces" is supposed to mean, and is now measured rather than asserted. The false
kill-claim was corrected in place rather than deleted, because a ledger that silently acquires the
right answer is not a ledger.

## T056 — the census extension (SC-013)

SC-013 requires the census to fail when a required auth row is *removed, renamed, or feature-gated*.
All three were executed, each in two halves, because only the second half is about the census:

| ID | Mutation | `cargo test` | The real census | Verdict |
|---|---|---|---|---|
| M1 | rename the test | `ok. 2 passed` | FAILED, named the row | KILLED |
| M2 | `#[cfg(feature = "t056-absent")]` | `ok. 0 passed` | FAILED, named the row | KILLED |
| M3 | delete the function outright | `ok. 0 passed` | FAILED, named the row | KILLED |

M2 and M3 are the sharper results: the binary runs **nothing** and `cargo test` still exits `0`.

```
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

A suite that executes nothing is indistinguishable, at the exit code, from a suite that passes. That
is the hole the census fills, and the reason `--workspace` alone is not coverage.

## Restoration discipline

Every mutated file was restored with `git checkout` and verified by **git blob hash against `HEAD`**,
never by trusting a backup copy. One restore during T056 produced a **one-byte difference** — a
truncation index that had already consumed the file's trailing newline — and the hash caught it. A
restore that is assumed rather than verified is not a restore.
