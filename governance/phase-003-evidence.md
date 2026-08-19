# Phase 003 — interactive CLI: evidence summary

**Status**: closed · **Merged**: 2026-08-19 · **Independently reviewed**: **no**

This is the **public summary**. The full evidence ledger and the review pack are internal working
records and are **retained in the project's private records rather than in this repository**. Git
history is unchanged: earlier commits and pull requests still contain them.

## What the phase delivered

The `renvor` command surface — `new`, `check`, `dev`, `docker`, `tls`, `doctor` — with
capability-based path containment, a transactional generator, a published exit-code taxonomy, and a
machine-readable JSON output contract. The contracts are public in
[`contracts/`](../contracts/).

## Integration

Pull request **#28** was squash-merged into `main` on **2026-08-19T08:47:14Z** as
**`01327b1ee61b73ebbd4f9198c04d651b38367ba8`**, from the reviewed head `68295ef6`. The merged tree
is byte-identical to the reviewed tree.

## Review status — stated plainly

| Item | State |
|---|---|
| Maintainer acceptance | **COMPLETE** — Ahmed Anbar, project author and maintainer |
| Automated / advisory review | **COMPLETE, and NON-INDEPENDENT** |
| Independent human requirements review | **NOT PERFORMED** |
| Independent human security review | **NOT PERFORMED** |
| Windows CI | **COMPLETE — automated platform evidence, not human Windows review** |
| **W-008** | **ACTIVE**, narrowly scoped, expiring **2027-02-11** |

> Ahmed Anbar, as project maintainer, reviewed the consolidated technical findings and accepts
> Phase 003 closure under the explicitly scoped waiver. The independent-review requirement was
> **not satisfied**. Automated and maintainer reviews are supporting evidence, **not** independent
> review.

An automated reviewer is not a person and cannot be independent under any reading of the criteria
in [`phase-003-independent-review-packet.md`](phase-003-independent-review-packet.md) §0. That
packet's §-1.3 sign-off log is **empty**, and filling it in is the only thing that closes W-008.

## Defect disposition

A five-area adversarial audit ran on 2026-08-19, with every reported finding independently re-run
by a reviewer instructed to **refute** it.

**22 claims examined — 14 confirmed and fixed, 8 refuted with recorded grounds.**

| Severity | Count | State |
|---|---|---|
| Critical | 0 | — |
| **High** | **3** | all FIXED |
| Medium | 6 | all FIXED |
| Low | 5 | all FIXED |

The three HIGH findings: `--output json dev` and `--output json docker` emitted unparseable stdout
on **every** run, and a quoted credential was printed alongside its own `[redacted]` marker — worse
than no redaction, because a log scrubber reads the marker as proof the line was clean.

**No finding is unresolved and none was waived.** Every fix carries a regression test observed to
fail before it and pass after, and every guard was mutation-tested.

Two of the phase's own new tests initially passed **for the wrong reason** and were caught by that
mutation testing — one because a reserved name was being refused by a different rule than the one
under test, hiding a real defect in which `renvor new demo --path COM¹` succeeded and created a
directory Windows cannot open.

## Carried forward

- **No independent human review of Phase 003 has occurred** (W-008). Windows is the widest gap, and
  it is **wider than "unreviewed by a person"**. An earlier draft of this summary said the five
  Windows behaviours in the packet's §-1.2 "have been exercised by CI"; that was **false**, and the
  packet it summarises says so directly. What is true:

  | Windows behaviour | Actually established by |
  |---|---|
  | The test suite passes on `windows-latest`, both toolchains | **Measured.** CI runs the tests the author wrote |
  | `MoveFileEx` rename semantics being sufficient for C-5 | **Not established.** Reasoned from documentation |
  | Reserved names, drive-relative paths, trailing dot/space stripping | **Reasoned from Windows behaviour that was never observed on Windows** — the packet's own words |
  | Junction / reparse-point classification by `describe` | **Reasoned, not measured** — the function's own doc comment says so |
  | Control-character escaping against a Windows console host | **Open question.** Not measured |

  CI cannot notice a test nobody wrote. Green Windows CI establishes that the written tests pass —
  it does not establish that these five behaviours are correct.
- **The waiver trend guard is tripped.** W-003, W-005 and W-008 waive the same rule for the same
  reason in three consecutive phases, which is a **release blocker** — on publishing, tagging, and
  deploying, not on merging — absent dated recruitment progress. **RO-001** is that obligation, with
  a first review date of **2026-11-19**, recorded as *not yet progress*.
- **W-008 confers nothing on Phase 004.**
