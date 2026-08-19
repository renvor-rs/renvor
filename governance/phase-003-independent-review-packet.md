# Phase 003 — Independent review packet

**You are being asked to perform the independent review that Phase 003 cannot close without.**

This packet is self-contained. You should not need to be briefed by anyone on the project, and you
should not accept a briefing from anyone on the project about whether the code is correct — that is
the thing you are here to decide.

> **REVISION 3 — 2026-08-19. COMMISSIONED.** This packet is now issued for review rather than
> prepared for it. Two earlier heads are **superseded** and must not be reviewed — `08d3f89` and
> `323bef3`; §-1 names both and says why. **Review only the head §-1 binds.**

---

## -1. The exact head, the one open blocker, and where to sign

### -1.1 The head this review is bound to

| | |
|---|---|
| **Repository** | `renvor-rs/renvor` |
| **Branch** | `feat/phase-003-interactive-cli` |
| **Content head** | `f2aa0d95bfac95585974885d249bad3ab27a321b` |
| **Branch tip** | one commit later — the commit that wrote this table. It touches **only** this file, and nothing else |
| **Pull request** | [#28](https://github.com/renvor-rs/renvor/pull/28) — **0 independent approvals**. Its description names the exact tip SHA |
| **Superseded — do NOT review** | `08d3f8997ed6c85ab544bc93dff3c8eb07a00a2e`, `323bef34c69c75e2989baf926303ec0ff3bc9347`, and `04abb07b41a287c54bc45335f09cdd2ca27d2ba5` |

**Verify before reading anything else.** A review of the wrong commit is worse than no review,
because it produces a sign-off that looks valid:

```sh
git fetch origin feat/phase-003-interactive-cli
git checkout feat/phase-003-interactive-cli
git rev-parse HEAD                                  # the tip
git diff f2aa0d95bfac95585974885d249bad3ab27a321b..HEAD --stat        # MUST list exactly ONE file: this one
git status --porcelain                              # MUST be empty
```

A stamp cannot contain its own commit's SHA, which is why this names the **content** head and bounds
exactly what came after it. Everything in this packet, and every `file.rs:NNN` reference in the
review pack, describes that head.

**Why the two superseded heads are listed by SHA rather than just "older commits".** Both were
circulated as the head to review, and both are wrong for reasons that would not be obvious from
reading them:

- `08d3f89` predates the maintainer's 2026-08-18 rulings. Its destination policy **deletes an
  existing empty directory**, its error registry is `schemaVersion` 1, and constitution principle
  VII is unamended.
- `323bef3` carries a **false doc comment** on `redact.rs`'s `path()` — it claims newline and tab
  are deliberately left unescaped, which is the opposite of what that function does. A security
  reviewer reading it would be reading a description of the *previous* fix.
- `04abb07` predates the 2026-08-19 defect audit and carries **all fourteen** of the defects it
  confirmed — including three HIGH ones: `--output json dev` and `--output json docker` emit
  unparseable stdout on every run, and a quoted credential is printed **alongside** its
  `[redacted]` marker. See evidence §6.0.6. Reviewing it would mean reviewing known-broken code.

### -1.2 The only remaining open blocker — WAIVED FOR THE MERGE, STILL OPEN AS A REQUIREMENT

Phase 003 has **one** unmet requirement. Everything else — the engineering, the testing, the
documentation, the governance, the advisory reviews, and full CI on three platforms and two
toolchains — is complete and recorded.

**Updated 2026-08-19.** This requirement was **not met and was not withdrawn**. Phase 003 merged
under waiver **W-008**, which is time-bounded (expires **2027-02-11** or on the availability of a
qualified reviewer, whichever is first) and scoped to this requirement in this phase alone. The
rows below stay **OPEN**. A waiver is a recorded decision to proceed with a known gap — it is not a
finding that the gap closed, and nothing in this packet should be read as saying the review
happened.

> **BLOCKER — a qualified independent human requirements review and security review, explicitly
> signed off, with Windows coverage.**

| Requirement | State |
|---|---|
| Independent human **requirements** review | **OPEN** |
| Independent human **security** review, concentrating on `crates/renvor-cli/src/paths.rs` and `crates/renvor-cli/src/generate/place.rs` | **OPEN** |
| **Windows coverage**, by a human | **OPEN — and this is the widest gap in the phase** |

**Windows is a hard requirement of this review, not a nice-to-have.** All seven advisory reviews ran
on macOS. CI runs Windows on both toolchains and it is green, but CI runs the tests the *author*
wrote — it cannot notice a test nobody wrote. Windows is where this phase's most expensive defects
have historically been: a rename that fails with `os error 32` while every Unix test passed, and a
trycmd contract regenerated on macOS that silently dropped the `[EXE]` placeholder. The specific
Windows behaviour this phase depends on and **no human has examined**:

1. `MoveFileEx` rename semantics — POSIX-equivalent atomicity is **not** claimed, and the contract
   says so. Is the weaker guarantee actually sufficient for C-5?
2. The open-handle-blocks-delete path (`os error 32`) and the `Option<Dir>` drop ordering that
   works around it.
3. Reserved device names, drive-relative paths (`C:name`), and the silent stripping of a trailing
   dot or space — `paths.rs` refuses these on **every** platform, reasoned from Windows behaviour
   that was never observed on Windows.
4. `destination_exists` classification for a **junction or reparse point**, which has no Unix
   analogue. The `describe` function tests `is_symlink()` first for a reason that is **reasoned, not
   measured** — its own doc comment says so.
5. Whether the control-character escaping in `output/redact.rs` behaves the same against a Windows
   console host.

A review that cannot reach a Windows machine should say so plainly and scope itself; **a review that
is silent about Windows does not discharge this blocker.**

### -1.3 Sign-off log — the review is not complete until this is filled in and committed

**This table is still EMPTY. No independent human review of Phase 003 has occurred.**

**Status changed 2026-08-19.** Phase 003 merged under waiver **W-008**
(`governance/waivers.md`), which waives *this requirement and nothing else* for *this phase and no
other*. The blocker below is therefore **transferred, not discharged**. Filling in this table is
still the only thing that closes it, and the waiver's removal plan says so.

An approval delivered by any other route — a comment, a message, a verbal "looks fine" — does not
count, because the point of logging it here is that the evidence travels with the artefact.

**Do not enter the maintainer in this table.** Ahmed Anbar authored Phase 003 and therefore fails
§0's criteria 1 and 2 by definition. His decision is recorded in §-1.4, which is a *different*
thing and is labelled as one.

| Field | Requirements review | Security review |
|---|---|---|
| Reviewer name | | |
| Affiliation / role | | |
| Not the author, not the maintainer (§0) | ☐ confirmed | ☐ confirmed |
| Head reviewed (must equal §-1.1) | | |
| Date | | |
| Platforms actually exercised | | |
| **Windows examined?** | ☐ yes ☐ no — if no, say what was not covered | ☐ yes ☐ no — if no, say what was not covered |
| Findings: Critical / High / Medium / Low | / / / | / / / |
| Findings document attached at | | |
| Verdict | ☐ approve ☐ approve with conditions ☐ reject | ☐ approve ☐ approve with conditions ☐ reject |
| Signature | | |

**If either verdict is "reject" or "approve with conditions", the phase stays open.** Saying the
work is not ready is the expected outcome of a real review, not a problem to be managed — see §0.

**Nothing in this packet may be edited by the project to make a review easier to pass.** If you find
this document overstates what was done, that is itself a finding, and §7 tells you how to report it.

---

### -1.4 Maintainer acceptance — recorded, and explicitly NOT independent review

This section exists so the decision to close Phase 003 is on the record with its true name. It is
**not** a substitute for §-1.3 and must never be cited as one.

| Field | Value |
|---|---|
| Decision | **Accept Phase 003 closure under waiver W-008** |
| Decided by | **Ahmed Anbar**, project author and maintainer |
| Date | 2026-08-19 |
| Role relative to the work | **Author of every line of Phase 003 and of the evidence describing it** |
| Independent under §0? | **No.** Fails criterion 1 (not a person other than the author) and criterion 2 (not the author). Criterion 4 — able to reject without the author's consent — is meaningless when the two are the same person |
| What was reviewed | The consolidated technical findings: the five-area adversarial defect audit and its 14 confirmed and fixed findings, the CI platform matrix, and the evidence pack |
| Supporting evidence relied on | Automated and maintainer review, both **non-independent** |
| What this decision is | A human maintainer decision to accept a known, recorded risk |
| What this decision is **not** | Independent review; peer review; a second person's approval; a finding that the independent-review requirement was satisfied |

**Recorded in substance, as required:**

> Ahmed Anbar, as project maintainer, reviewed the consolidated technical findings and accepts
> Phase 003 closure under the explicitly scoped waiver. The independent-review requirement was
> **not satisfied**. Automated and maintainer reviews are supporting evidence, **not** independent
> review.

**On the automated reviews.** Phase 003 used automated, agent-assisted review extensively, and that
is recorded rather than hidden. It does not move the needle on independence: §0 requires a
**person**, and an automated reviewer is not one. Its value was finding defects — 14 confirmed,
8 refuted with stated grounds — not conferring approval.

## 0. What counts as independent, and what does not

Phase 003 requires **one qualified independent human requirements review** and **one qualified
independent human security review**. The following do **not** satisfy that, and none of them is
offered as if it did:

| Not independent | Why |
|---|---|
| The advisory AI reviews in [`phase-003-evidence.md`](phase-003-evidence.md) §6 | Spawned by, prompted by, and reporting to the same process that wrote the code. Labelled NON-INDEPENDENT and ADVISORY throughout. |
| The maintainer's own review | Single-maintainer repository. This is the standing condition the six existing waivers all describe. |
| Claude, or any agent | Same as the first row. |
| CI | CI runs the tests the author wrote. It cannot notice a test that was never written. |

**No Phase 003 phase-level waiver exists and none has been drafted.** If you conclude the work is not
ready, saying so is the expected outcome, not a problem to be managed.

## 1. What Phase 003 is

`renvor` is a command-line tool that generates Rust project skeletons. This phase delivers:

- `renvor new` — an interactive wizard **and** an equivalent flag interface, which must produce
  byte-identical projects;
- a **transactional** generator: render into staging, verify, then one atomic rename, with a promise
  that any failure leaves the destination untouched;
- `doctor`, `check`, `dev`, `docker`, and a `tls trust` consent boundary;
- a machine-readable JSON contract and a fixed exit-code taxonomy.

**Nothing is published.** The crate is not on crates.io and `publish = false`. Nothing is deployed.

## 2. Where to start

| Order | Read | Why |
|---|---|---|
| 1 | [`phase-003-review-pack.md`](phase-003-review-pack.md) | The two files that can write outside the operator's directory, in depth. **Highest value per minute.** |
| 2 | [`specs/003-interactive-cli/spec.md`](../specs/003-interactive-cli/spec.md) | 48 functional requirements, 16 success criteria, 6 user stories |
| 3 | [`specs/003-interactive-cli/contracts/`](../specs/003-interactive-cli/contracts/) | C-1 command surface, C-2 JSON, C-4 templates, C-5 the transaction |
| 4 | [`specs/003-interactive-cli/data-model.md`](../specs/003-interactive-cli/data-model.md) | Invariants I-1 … I-17 |
| 5 | [`phase-003-evidence.md`](phase-003-evidence.md) §3 | The requirement-to-evidence map, 64 rows |
| 6 | `crates/renvor-cli/src/` | 22 files, ~5,700 lines |
| 7 | `crates/renvor-cli/tests/` | 9 files + a shared pty harness |

## 3. Reproduce everything

```sh
git checkout feat/phase-003-interactive-cli
cargo xtask verify          # 11 checks: fmt, clippy, tests, rustdoc -D warnings,
                            # cargo-deny, architecture invariants, secret scan,
                            # docs build, link check, working-tree cleanliness
cargo test --workspace      # 223 tests in renvor-cli, 568 across the workspace
```

Nothing requires network access. Nothing requires Docker. The generated projects declare no
dependencies.

## 4. What the project already knows is wrong or unresolved

Stated up front so you spend your time on what nobody has found yet.

| # | Issue | Where |
|---|---|---|
| 1 | **Constitution principle VII said the wizard MUST ask eleven things; it asks three.** **RESOLVED 2026-08-18** — the principle was amended, 2.0.0 → 3.0.0 MAJOR, rather than waived. Whether the amendment is legitimate is a fair thing for you to challenge. | evidence §2, §7.7; `constitution-amendment-3.0.0.md` |
| 2 | **T008 and T015–T024 were specified as failing-first and were not written that way.** The behaviour is complete; the ordering is permanently missed, by ruling, and does not block closure. | `tasks.md` "Ordering requirements that were missed" |
| 3 | **Invariant I-17: the TOCTOU window is narrowed, not closed.** Specifically, POSIX `rename(2)` will silently replace an **empty** directory another process creates between the last check and the rename. Stated residual risk; not portably closable. | review pack §6.1 |
| 3b | **The fail-closed destination check has no Windows-specific test.** `a_destination_whose_state_cannot_be_established_fails_closed` is `#[cfg(unix)]`. | review pack §10 item 5 |
| 3c | **The "renvor never *deliberately* deletes the destination" claim is guarded by a source-text scan.** It would not catch a removal expressed through an alias or another crate. The qualifier matters: POSIX `rename(2)` replaces an empty directory created in the TOCTOU window, which is the system call's behaviour, not renvor's. | review pack §5.3, §6.1 |
| 3d | **B-R3 is the one advisory finding still only partially fixed.** The redaction corpus is narrow: two values through one injection point, plus one successful-run case. No explicit dry-run case. | evidence §6.3 |
| 3e | **Human output escapes path-derived control characters completely**, newline and tab included, at each interpolation site. The `Reporter`-level backstop still exempts `\n`/`\t`, correctly — by then the line is legitimately multi-line. The open question is whether an untrusted value can reach a stream by a route neither escaper covers. | evidence §6.0.1; review pack S14 |
| 3f | **Windows has had no adversarial review at all.** All seven advisory reviews ran on macOS. CI exercises Windows; no reviewer has attacked it. | evidence §6.0.4 |
| 4 | **Offline proof is proxy-based plus a structural no-HTTP-client assertion**, not a network namespace. Limitation stated in the test file's own header. | `tests/offline.rs` |
| 5 | **24 of 64 requirement identifiers are not cited by name at their point of test.** Traceability gap, not a coverage gap. | evidence §3.1 |
| 6 | **Data-model §5 rule 8 has no `details.rule`** — containment is structural. | review pack §4 |
| 7 | **Three template bounds were enforced and untested until 2026-08-18.** Now tested with boundary cases. | `tasks.md` register |

## 5. Eleven defects were found and fixed during this phase. Every one lived in a **seam**

This is offered as a map of where to look, not as reassurance.

| Seam | Defect |
|---|---|
| validator ↔ caller | `--path ../escape` accepted, exit 0 |
| renderer ↔ TOML | booleans rendered `True` — invalid TOML |
| `main` ↔ commands | `--dry-run` started containers |
| taxonomy ↔ runtime | a panic exited 101, not 1 |
| clap ↔ JSON contract | zero JSON documents on malformed input |
| requirement ↔ code | unbounded manifest read |
| `open` ↔ `place` | empty destination accepted, then refused — and the fix for that introduced two more findings (A-R8, A-R9) before the **policy itself** was ruled to be the defect |
| `place` ↔ kernel | a lost race misclassified |
| comment ↔ code | a false uniqueness claim |
| schema ↔ generator | `check` rejected renvor's own output |
| **wizard ↔ flag surface** | the printed "equivalent command" **was not a runnable command** |

**No defect was found inside a single correct component.** Every one was a disagreement between two
components that were each defensible alone. Unit tests found none of them.

## 6. Two process failures by the author, both recorded in the repository

You should know the commit history is misleading without these.

1. **Commit `1417ed6`** is titled *"…and classify a lost race correctly"* and **contains no
   classification code**. Five string-replacement edits were made; two silently matched nothing, and
   the result was committed without re-reading the function.
2. **Commit `2448e47` was pushed with a failing test in it.** The verification chain was joined with
   `;` instead of `&&`, so the test's failure was discarded and the chain reported success.

Both are recorded in `specs/003-interactive-cli/tasks.md`. If you find the record understates what
happened, say so.

## 7. What your review needs to produce

For each finding: **severity** (Critical / High / Medium / Low), **location** (file:line),
**what is wrong**, **how it fails** (concrete inputs → wrong outcome), and **what would fix it**.

A finding of the form *"requirement X is claimed as covered and the test that covers it cannot
fail"* is the **most valuable** thing you can produce here, because that is the failure mode this
phase has repeatedly had and the one its authors are least able to see.

**If you find nothing at a given severity, say so and say what you checked.** A review that lists
only speculative concerns, or that is silent about its own scope, does not discharge the
requirement.

**Then record the verdict in the §-1.3 sign-off log and have it committed.** That is what closes the
blocker; a findings document on its own does not, because the evidence has to travel with the
artefact rather than sitting in somebody's inbox. Attach or link your findings from the
"Findings document attached at" row.

## 8. Explicitly out of scope

- Phases 004 and later. Reserved flags refuse with `details.phase`; that is this phase's contract.
- Publication, deployment, DNS, VPS, Kubernetes. None occurs in this phase.
- The other three repositories (`site`, `site-2`, `infra`). Untouched by Phase 003.
- Whether the *product* is a good idea.
- **Merging.** You approve or you do not; the merge is the maintainer's action afterwards. Nothing
  in this packet asks you to merge, and PR #28 stays open and unmerged until §-1.3 is filled in.

---

**Status**: **COMMISSIONED**, 2026-08-19 · **Blocker**: one, see §-1.2 ·
**PR #28 is NOT merge-ready** until §-1.3 is completed and committed ·
**Branch**: `feat/phase-003-interactive-cli` · **PR**: #28 (open, unmerged, 0 approvals) ·
**Packet revision 2**: 2026-08-18 · **Head to review**: see §-1
