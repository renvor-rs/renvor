# Phase 003 — Independent review packet

**You are being asked to perform the independent review that Phase 003 cannot close without.**

This packet is self-contained. You should not need to be briefed by anyone on the project, and you
should not accept a briefing from anyone on the project about whether the code is correct — that is
the thing you are here to decide.

> **REVISION 2 — 2026-08-18.** Revision 1 pointed at head `08d3f89`. That head is **superseded**:
> the maintainer denied approval of the `paths.rs`/`place.rs` review pack and directed a change to
> the destination policy, the error registry, and constitution principle VII. **Review the head named
> in §-1, not `08d3f89`.**

---

## -1. The exact head to review

| | |
|---|---|
| **Branch** | `feat/phase-003-interactive-cli` |
| **Content head** | `15330732eba0c6583c73e80bf80dc04c77021680` |
| **Branch tip** | one commit later — the commit that wrote this table, touching only this file and `phase-003-review-pack.md` |
| **Pull request** | [#28](https://github.com/renvor-rs/renvor/pull/28), open and unmerged. Its description names the exact tip SHA |
| **Superseded head** | `08d3f8997ed6c85ab544bc93dff3c8eb07a00a2e` — do **not** review this one |

```sh
git fetch origin feat/phase-003-interactive-cli
git checkout feat/phase-003-interactive-cli
git diff 15330732eba0c6583c73e80bf80dc04c77021680..HEAD --stat    # must list exactly two governance files, and nothing else
```

A stamp cannot contain its own commit's SHA, which is why this names the content head and bounds
what came after it. Everything in this packet, and every line reference in the review pack,
describes that head.



---

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
cargo test --workspace      # 218 tests in renvor-cli, 563 across the workspace
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
| 3c | **The "renvor never deletes the destination" claim is guarded by a source-text scan.** It would not catch a removal expressed through an alias or another crate. | review pack §5.3 |
| 3d | **B-R3 is the one advisory finding still only partially fixed.** The redaction corpus is narrow: two values through one injection point, plus one successful-run case. No explicit dry-run case. | evidence §6.3 |
| 3e | **Human output escapes control characters; `\n` and `\t` are exempt.** So a newline injected through a **non-final** `--path` component still reaches the terminal. Deliberate — cargo's multi-line stderr must stay readable — and a stated residual. | evidence §6.0.1; review pack S14 |
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

## 8. Explicitly out of scope

- Phases 004 and later. Reserved flags refuse with `details.phase`; that is this phase's contract.
- Publication, deployment, DNS, VPS, Kubernetes. None occurs in this phase.
- The other three repositories (`site`, `site-2`, `infra`). Untouched by Phase 003.
- Whether the *product* is a good idea.

---

**Branch**: `feat/phase-003-interactive-cli` · **PR**: #28 (open, unmerged) ·
**Packet revision 2**: 2026-08-18 · **Head to review**: see §-1
