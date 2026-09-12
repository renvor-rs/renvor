# Phase 012 — Limitations

**Companion to**: [`phase-012-evidence.md`](phase-012-evidence.md) · [`phase-012-specification-and-decision-brief.md`](phase-012-specification-and-decision-brief.md) · [`phase-012-task-plan.md`](phase-012-task-plan.md)
**Phase**: 012 — REST documentation, production examples, and the L-1/L-2 carry-over
**State**: **OPEN — the phase is in progress and this ledger is incomplete.** Batches B0 and B1
are merged; B1f onward are not. Nothing here is published, tagged, released, or deployed, no
waiver was created or granted, and **no limitation of any phase is closed by this file**. L-1 and
L-2 stay open exactly as `phase-011-limitations.md` states them until their §6 closure conditions
are measured and the maintainer marks them closed (authorization A-3).

Every row states **what**, **why it was not closed**, and **who it belongs to**.

## Security-relevant

| Row | What | Why it was not closed | Owner / target / consequence |
|---|---|---|---|
| **L-1** | **A trusted compiler wrapper runs inside the "sealed" step with the operator's full rights, and the record cannot contradict it.** `PASSED_THROUGH` keeps `RUSTC`, `RUSTC_WRAPPER`, `RUSTFLAGS`, and `RUSTDOCFLAGS` (D-L2-3, approved with corrections 2026-09-07). A wrapper named by `RUSTC_WRAPPER` — `sccache` is the intended use — is launched by Cargo inside the step the contract calls sealed, and so is every build script of every dependency. What FR-012-7d records is a **launch observation plus a queried identity**: renvor saw a particular executable launched, and asked that executable who it was. It is not proof that the compilation was performed by what answered. A wrapper is free to execute something else, and no field in `[verified_with]` claims otherwise. | **Refusing wrappers is worse.** A refusal would break every machine using a build cache, which is the intended and overwhelmingly common use, to defend against an attacker who already controls the operator's environment variables — and who therefore already runs code as the operator by a dozen easier routes. The seal's guarantee is deliberately narrower and stated exactly: no secret of the operator's shell reaches the child (C-5), no toolchain is provisioned (SR-012-1), and what was launched and what was queried is recorded, with a cached run recorded as cached rather than inferred. Narrowing this further needs a threat model that says what the seal is defending against; none has been written. | **Owner**: a future phase that states a sandbox threat model, if one is ever wanted. **Target**: none set — this is a recorded residual, not scheduled work. **Consequence**: an operator whose environment is already compromised gets a provenance record that names the compiler the wrapper claimed to be. The record's own wording (§5.2, FR-012-7d) is what keeps this honest: it says *observed* and *queried*, never *verified*. `renvor doctor` reports whether the resolved compiler is a proxy (FR-012-7c), so the condition is visible rather than silent. |

## Correctness and operations

| Row | What | Why it was not closed | Owner / target / consequence |
|---|---|---|---|
| *(none yet)* | Rows are added as each batch merges. | — | — |

## Findings carried openly, tracked outside this ledger

| Finding | Where | State |
|---|---|---|
| **Finding 1** — the relay's unbounded join | [`phase-012-finding-1-relay-join-followup.md`](phase-012-finding-1-relay-join-followup.md) | **open and untouched.** B1 added an `Invocation::command()` seam for FR-012-6 but did not change the detached-reader behaviour the finding is about. |
| **Finding 4** — rustdoc doctest observation | [`phase-012-finding-4-rustdoc-observation-proposal.md`](phase-012-finding-4-rustdoc-observation-proposal.md) | **fixed in B1** (`record_version` 3). Its untested boundaries are recorded in that document, not here. |
