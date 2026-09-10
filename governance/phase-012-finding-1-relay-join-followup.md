# Finding 1 — the unbounded reader join in `commands/relay.rs`

**STATUS: DEFERRED, SCOPED, NOT FIXED. `crates/renvor-cli/src/commands/relay.rs` is UNCHANGED by
the correction that carries this file.**

Separating this defect from the findings-2-and-3 correction is a sequencing decision by the
maintainer (2026-09-09). **It is not acceptance of the defect and not closure of it.**

## The defect

`Invocation::run`, `crates/renvor-cli/src/commands/relay.rs:231`; the join is line **307**:

```rust
let collected = reader.join().map_err(|_| RelayFailure::ReaderFailed)?;
```

`self.timeout` bounds `child.wait_timeout` and nothing else. The **timeout** path (lines 271-299)
deliberately DETACHES the reader with `drop(reader)`, and carries a long comment explaining the
grandchild-holds-the-pipe hazard. The **success** path joins it with no bound at all.

So when the direct child exits zero while a descendant still holds the inherited stdout write end,
`read_to_end` stays blocked and `run` waits for the orphan — the exact hang the module header says
it prevents ("The deadline bounds this process's wait") and that `DEFAULT_TIMEOUT`'s own
documentation calls "a hang an operator has to notice and interrupt".

## Provenance and scope

- **Inherited, genuinely.** `relay.rs` exists at PR #72's base `6c0d780`, and the join dates to
  Phase 005 (`git log -L 300,310:crates/renvor-cli/src/commands/relay.rs` → `d3ddfbc`).
  This is the ONLY one of the four findings that is not introduced by PR #72.
- **`5de7590`, this branch's own commit, does not touch it** — its `relay.rs` diff is the
  `command()` extraction and `no_provisioning` only.
- **Declared out of B1's scope.** `specs/spec.md:310` lists `commands/relay.rs` as
  "not toolchain invocations | unchanged". Changing it needs authority to deviate from that.

## Reachability — two shipped commands, not tests

- `renvor routes` — `crates/renvor-cli/src/commands/routes.rs:124`
- `renvor openapi` — `crates/renvor-cli/src/commands/openapi.rs:94`, inside `pub fn run`, wired at
  `main.rs:483`. The file's top-level `use super::relay::{Invocation, RelayFailure}` is at line 53
  and its `#[cfg(test)]` block does not begin until line 280.

`dev.rs:126` genuinely is test-only (`#[cfg(test)]` at line 112). An earlier draft of the
assessment called `openapi.rs` test-only; that was wrong, was found by the independent automated
validation, and it understated the blast radius.

**Trigger**: the generated project's binary spawns a process it does not wait for, then exits.

## Reproduction (retained, externally bounded)

A probe in a scratch copy of the tree, never on the branch:

```rust
// child exits 0 immediately; a grandchild holds the inherited stdout write end for 6s
Invocation::new(shell("sleep 6 & exit 0")).with_timeout(Duration::from_millis(300))
```

Result — the call returns `Ok`, having waited far past its deadline:

| platform | compiler | measured wait vs a 300 ms deadline |
|---|---|---|
| macOS aarch64 | 1.94.0 | `6.010368833s`, `6.0112155s` |
| Linux aarch64 | 1.94.0 | `6.004421503s` |
| Linux aarch64 | **1.98.1** (CI's stable by release and commit) | `6.005016753s` |

The control — a child with no grandchild — returns at once on all three, so the probe measures the
orphaned pipe and not the child's own exit. Every run was bounded by an external timeout; none was
allowed to hang the investigation.

## Proposed correction

The pattern is already in this tree, added by this same branch: `toolchain/isolate.rs:277`
(`remaining(deadline)`) and the two-phase bounded shape at `:322-327`. `isolate::run_bounded` and
`evidence::answer` were corrected to it during this round. Apply it here: one deadline covering the
wait AND the collection, via an `mpsc` channel and `recv_timeout`, so a reader that cannot be
joined in time is detached exactly as the timeout path already detaches it.

## Acceptance criteria

1. **The reproduction above fails before the change and passes after it**, with the same external
   bound, on the same three platform/compiler combinations.
2. **THE COMPLETE SLOW-RESPONSE CONTROL.** A child that writes a **large** answer **slowly** but
   finishes **inside** the deadline must still return that answer **complete and byte-identical**.
   This is the control that makes the fix meaningful rather than merely fast: a bound that
   truncates a legitimate slow answer would pass criterion 1 and silently corrupt every large
   route dump. It must assert the full payload, not its length and not a prefix, and it must use a
   payload larger than the pipe buffer so the child genuinely blocks on write.
3. A child that exits non-zero with output already buffered still surfaces that output.
4. The existing timeout-path tests are unchanged and still pass — the detach behaviour on timeout
   is not what is being corrected.
5. `renvor routes` and `renvor openapi` are exercised end to end against a project binary that
   spawns an orphan, and both terminate within the deadline.
6. No change to the relay's public surface, its exit codes, or its diagnostics.

## What this record does not claim

It does not claim a severity beyond "high impact, low likelihood": the hang needs a forking project
binary. It does not claim the defect is unreachable in practice, and it does not claim the
separation from findings 2 and 3 reduces it. It records that the maintainer sequenced it
separately and did not accept it.
