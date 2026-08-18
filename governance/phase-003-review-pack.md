# Phase 003 — Human review pack: `paths.rs` and `place.rs`

**For**: the maintainer, and any independent reviewer.
**Scope**: exactly two files — the destination boundary and the placement transaction.
**Why these two**: every other file in the crate can be wrong and produce a bad project. These two
can be wrong and **write outside the directory the operator named**, or **destroy something that was
already there**. They are 1,144 lines together and they are the ones worth a human's time.

**Read this in one sitting.** It is written to be sufficient on its own; you should not need to
reconstruct the design from the code.

---

## 1. What the two files are for

| File | Responsibility | Lines |
|---|---|---|
| [`crates/renvor-cli/src/paths.rs`](../crates/renvor-cli/src/paths.rs) | Decide whether a requested destination may be written to **at all**, and produce the capability that authorises it | 530 |
| [`crates/renvor-cli/src/generate/place.rs`](../crates/renvor-cli/src/generate/place.rs) | Own a staging directory, and move it onto the destination **atomically or not at all** | 614 |

## 2. The flow, end to end

```text
  renvor new <name> --path <destination>
        │
        ▼
  ┌───────────────────────── paths.rs ──────────────────────────┐
  │ validate_project_name(name)          paths.rs:307           │
  │   (only when the operator SUPPLIED a name — before any I/O) │
  │     name_not_empty · name_length · single_path_component    │
  │     reserved_device_name · name_character_set               │
  │     name_starts_with_letter                                 │
  │        │ any failure → exit 3, details.rule names which     │
  │        ▼                                                    │
  │ Destination::open(requested)          paths.rs:137          │
  │   RULE 0  no `..` component ANYWHERE          paths.rs:144  │
  │   RULE 1  final component is one ordinary name paths.rs:171 │
  │   RULE 2  not a reserved device name           paths.rs:205 │
  │   RULE 3  parent opens  ◄── THE ONE AMBIENT CALL paths.rs:224
  │   RULE 4  destination is not a symlink         paths.rs:245 │
  │   RULE 5  destination absent, or present and EMPTY paths.rs:260
  │        │                                                    │
  │        ▼  returns Destination { parent: Dir, name: String } │
  └────────────────── the capability, not a path ───────────────┘
        │
        ▼
  ┌───────────────────────── place.rs ──────────────────────────┐
  │ Staging::create(&destination)          place.rs:61          │
  │   creates `.renvor-staging-{pid}-{nanos}-{seq}`             │
  │   INSIDE the destination's parent, never the system tmpdir  │
  │        │                                                    │
  │        ▼   render → verify → manifest  (all through the Dir)│
  │                                                             │
  │ Staging::place(destination)            place.rs:144         │
  │   STEP 1  make room, or refuse         place.rs:160         │
  │            remove_dir(target) — kernel refuses if non-empty │
  │   STEP 2  DROP OUR HANDLE              place.rs:201         │
  │   STEP 3  rename, classify from ErrorKind place.rs:208      │
  │            on failure: restore a removed empty destination  │
  └─────────────────────────────────────────────────────────────┘
        │
        ▼  Drop for Staging  place.rs:270 — removes the tree on EVERY
           other path, including a panic
```

## 3. Security invariants — the things that must be true

| # | Invariant | Enforced by | How |
|---|---|---|---|
| **S1** | No filesystem operation can reach outside the destination's parent | `cap_std::fs::Dir` | **Structural.** After `Destination::open`, there is no ambient path API in scope. A code path that *forgets* to validate still cannot escape. |
| **S2** | Exactly one ambient call exists in the whole program | `paths.rs:224` | `Dir::open_ambient_dir` on the parent the operator typed. Everything downstream is `Dir`-relative. |
| **S3** | No `..` component is accepted anywhere in the requested path | `paths.rs:144` | Lexical, **in addition to** S1. Policy, not containment — see §4. |
| **S4** | A project name is one path component | `paths.rs:307` | `/`, `\`, and `:` refused as `single_path_component`. |
| **S5** | Reserved device names are refused **on every platform** | `paths.rs:48`, `:205`, `:307` | 22 names, matched on the **stem**, case-insensitively. A project made on Linux is opened on Windows. |
| **S6** | Validation completes before **any** filesystem write | `model.rs::resolve` | `Destination::open` reads and opens; it creates nothing. |
| **S7** | A failure before placement leaves the destination exactly as it was | `Drop` at `place.rs:270` | Runs on every path including panic. |
| **S8** | Placement is one rename, never a copy | `place.rs:208` | Both sides are the same `Dir`, so it cannot cross a filesystem. |
| **S9** | Losing a race is a clean failure, never an overwrite | `place.rs:232` | Classified from the **kernel's own `ErrorKind`**. |
| **S10** | Staging is never the system temporary directory | `place.rs:61` | It sits beside the destination so the final step can be a rename. |

## 4. Traversal and symlinks — read this section carefully

**Two mechanisms, deliberately overlapping.**

**The capability (S1) is what makes escape impossible.** `cap-std` refuses traversal,
absolute paths, and symlinks that leave the tree — *inside the handle*, as a property of the
library rather than of a check anyone remembered to write.

**RULE 0 (`paths.rs:144`) is policy on top of that.** It refuses `..` **in the path the operator
typed**, which the capability would happily honour because the operator names the parent
themselves. FR-039 and SC-009 require the refusal.

> **This rule was dropped and restored, and that is the single most important thing in this pack.**
> The cap-std migration removed it, reasoning that the operator typed the parent path. For a shell
> that reasoning is right; here it is wrong. **`renvor new --path ../escape` was accepted with
> exit 0**, and every unit test still passed — because the only traversal test covered a path
> *ending* in `..`, which RULE 1 catches for an unrelated reason. **A test passing for the wrong
> reason.** The regression test now covers five spellings and asserts `details.rule ==
> "no_traversal"` specifically.

**Symlinks** are refused twice over:

- **By name**, RULE 4 (`paths.rs:245`): the destination itself must not be a symlink. A symlink to a
  directory is a legitimate thing to point at and an illegitimate thing to write a project through.
- **By capability**: a symlink *inside* the tree that points out of it is refused by the handle, and
  **no rule of ours checks for it**. `the_handle_refuses_an_escape_that_no_rule_in_this_module_
  checks_for` (`paths.rs:389+`) is the test that proves the capability is doing work the hand-written
  rules do not.

**Data-model §5 rule 8** — "canonical destination is inside the canonical parent" — therefore has
**no `details.rule`**, because there is no check to name. That is what adopting the capability
bought. Reviewers should satisfy themselves this is a trade they accept.

## 5. The empty-destination case — where a real bug lived

FR-013 refuses a destination that "exists and is **not empty**". So an existing **empty** one is a
**legal target**, and that single word drove three separate defects:

1. **`open` accepted it and `place` refused it.** `renvor new` into an empty directory validated,
   rendered, ran the **full pre-placement verification** (a `cargo build` and `cargo test`), and only
   then failed — with a message claiming the directory had *"appeared while the project was being
   generated"*, when it had been there all along.
2. **The fix introduced an FR-012 violation.** `place` removes the empty destination immediately
   before the rename; if the rename then failed, the operator's directory was **gone**.
   `place.rs:240` restores it.
3. **The restore must not fire when the race was lost**, or it would recreate a directory another
   process just legitimately filled. Hence `lost_the_race` at `place.rs:232`.

**`remove_dir` is what makes step 1 safe**: the kernel refuses to remove a non-empty directory, so
the emptiness check and the removal are **one atomic operation** rather than a check followed by a
hopeful delete.

## 6. TOCTOU — invariant I-17, stated as a limitation

Documented at [`paths.rs:53`](../crates/renvor-cli/src/paths.rs) and [`place.rs:208`](../crates/renvor-cli/src/generate/place.rs).

**The window is narrowed, not closed, and the code does not claim otherwise.**

Between `Destination::open` returning a handle and `place` performing the rename, another process
with write access can create the destination. What holds:

- **One resolution, not two.** Every check and the rename go through the same open handle. A design
  that re-`stat`s a path and then renames *by path* resolves the name twice, and an attacker who wins
  between those two resolutions gets a different directory.
- **Losing is clean.** The rename refuses an existing destination.

**Closing it entirely requires an atomic create-or-fail rename, which POSIX does not provide
portably**: `rename(2)` silently replaces an empty destination directory, and
`renameat2(RENAME_NOREPLACE)` is Linux-only. The tests assert the **consequence** — concurrent runs
produce exactly one project and clean failures — rather than an impossibility that does not hold.

**This is the residual risk a reviewer is being asked to accept.**

## 7. Windows versus Unix — three real differences

| Behaviour | Unix | Windows | Handled at |
|---|---|---|---|
| Renaming or deleting a directory with an **open handle** | fine | **refused, `os error 32`** | `place.rs:40`, `:201` — the handle is `Option<Dir>` and is dropped before both |
| Reserved device names (`CON`, `NUL`, `COM1`…) | ordinary names | resolve to **devices** | refused on **both**, `paths.rs:48` |
| Rename atomicity | POSIX guarantees | weaker | documented rather than claimed equal |

> **The `os error 32` defect made the transaction's central guarantee false on Windows while every
> Unix test passed.** It was caught only by the `platform` matrix, which is **advisory, not
> required**. Treating an advisory failure as ignorable would have shipped it.

## 8. Race classification and recovery

```text
rename fails
     │
     ├── ErrorKind::DirectoryNotEmpty ─┐
     ├── ErrorKind::AlreadyExists ─────┼──► lost_the_race = true
     └── symlink_metadata(target).is_ok() ┘        │
                                                   ▼
                                    report `destination_not_empty`
                                    (another run got there first)
                                    do NOT restore — it is not ours

     any other kind ──► lost_the_race = false
                             │
                             ▼
                 restore the removed empty destination
                 report `placement_failed` (the move mechanism broke)
```

**Why classify from `ErrorKind` rather than by re-checking the destination**: a re-`stat` is *itself*
racy, and that is exactly how `placement_failed` kept leaking through. A loser reporting
`placement_failed` sends an operator to debug their filesystem when the truth is "somebody else won".

## 9. Tests, and what they actually establish

| Test | File | Establishes |
|---|---|---|
| `every_traversal_spelling_is_refused_by_the_traversal_rule` | `tests/hostile.rs` | 5 spellings, each asserting `rule == "no_traversal"` — not merely that it failed |
| `an_absolute_path_in_the_name_position_is_refused` | `tests/hostile.rs` | 4 cases incl. Windows drive-relative `C:name`, asserting `single_path_component` |
| `every_reserved_device_name_is_refused_on_every_platform` | `tests/hostile.rs` | **66 cases** — 22 names × {upper, lower, with extension} |
| `a_destination_that_is_a_symlink_to_another_directory_is_refused` | `tests/hostile.rs` | Escape a lexical check cannot see; witness file outside proves nothing was written |
| `every_project_name_refusal_names_a_distinct_rule` | `tests/hostile.rs` | 5 refusals, 5 distinct rules, no collapsing |
| `an_ordinary_legitimate_destination_still_generates` | `tests/hostile.rs` | **Positive control.** Without it a generator that refuses everything passes the file |
| `the_handle_refuses_an_escape_that_no_rule_in_this_module_checks_for` | `paths.rs` | The capability catches what the rules do not |
| `a_failure_at_any_mutating_step_leaves_an_absent_destination_absent` | `tests/transaction.rs` | 5 injected C-5 failures |
| `…leaves_a_pre_existing_empty_destination_exactly_as_it_was` | `tests/transaction.rs` | FR-012, incl. the restore path. **Demonstrated failing on purpose** by moving the injection point past the removal |
| `an_uninjected_run_into_the_same_fixtures_succeeds` | `tests/transaction.rs` | **Positive control** for both of the above |
| `concurrent_runs_at_one_destination_produce_one_project_and_no_corruption` | `tests/transaction.rs` | 3 real processes: 1 wins, 2 lose with `destination_not_empty`, no residue |
| `racing_placements_…` | `place.rs` | 16 threads directly on `place` — the test that actually reproduces the race |
| `a_killed_run_leaves_identifiable_residue_beside_the_destination_and_no_project` | `tests/transaction.rs` | `SIGKILL`; residue is beside, named, and no project exists |
| `staging_names_are_unique_within_one_process` | `place.rs` | 256 names, all distinct |

**Result at the head of this branch**: all pass, on `ubuntu-latest`, `macos-latest`, and
`windows-latest`, on both `1.94.0` and `stable`.

## 10. What a reviewer should try to break

Suggested, in descending order of value:

1. **Find a path that reaches outside the parent.** The claim is that this is structurally
   impossible after `Destination::open`. Attack the window *before* it, and attack RULE 3's ambient
   call.
2. **Find a failure path that does not restore a removed empty destination.** §5 defect 2 was
   introduced by the fix for defect 1.
3. **Find a `details.rule` that fires for the wrong reason.** This has happened twice.
4. **Find a Windows behaviour the tests do not exercise.** Two defects were Windows-only.
5. **Challenge the I-17 trade-off** (§6). It is a stated residual risk, not a solved problem.

---

## 11. Approval statement

**Use this only after reading §§2–10 and the two files themselves.** It is deliberately narrow: it
approves two files, not the phase.

> I have read `crates/renvor-cli/src/paths.rs` and `crates/renvor-cli/src/generate/place.rs` in
> full, together with §§2–10 of this pack.
>
> I understand and accept:
> - that path containment is **structural** (`cap_std::fs::Dir`) rather than checked, and that
>   data-model §5 rule 8 therefore has no named rule;
> - that the time-of-check-to-time-of-use window described in invariant I-17 is **narrowed and not
>   closed**, and that closing it is not portably possible today;
> - that `renvor new` will **remove an existing empty destination directory** immediately before the
>   rename, and restores it if the rename fails for any reason other than losing a race.
>
> I approve these two files for merge as part of Phase 003.
>
> Name: ______________________  Date: ____________  Commit reviewed: ____________

**This approval does not close Phase 003.** It covers two files. See
[`phase-003-evidence.md`](phase-003-evidence.md) §7 (principle VII) and §8 (independent review) for
what else is outstanding.
