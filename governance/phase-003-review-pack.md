# Phase 003 — Human review pack: `paths.rs` and `place.rs`

**For**: the maintainer, and any independent reviewer.
**Scope**: exactly two files — the destination boundary and the placement transaction.
**Why these two**: every other file in the crate can be wrong and produce a bad project. These two
can be wrong and **write outside the directory the operator named**, or **destroy something that was
already there**. They are 1470 lines together — over half of that comments and tests — and they are the ones worth a human's time.

**Read this in one sitting.** It is written to be sufficient on its own; you should not need to
reconstruct the design from the code.

> **REVISION 2 — 2026-08-18.** Revision 1 was submitted for approval and **approval was DENIED**,
> pending the maintainer's rulings of the same day. What changed in these two files as a result:
>
> | Was | Is |
> |---|---|
> | An existing **empty** destination was a legal target; `place` deleted it and let the rename create a fresh one | **Every** existing destination is refused — empty directory, non-empty directory, file, symlink, dangling symlink |
> | A failed rename **restored** the deleted directory, ignoring its own error | Nothing is restored, because nothing is removed |
> | An unreadable destination could fall through to "absent" and generation proceeded | Any error that is not an authoritative `NotFound` **fails closed**, carrying the original OS error |
> | Refused with `destination_not_empty` | Refused with `destination_exists`, `details.found` naming what was there. `destination_not_empty` is **retired**; `schemaVersion` is now `2` |
> | Staging that could not be **created** reported `placement_failed` | Reports `staging_failed` |
>
> §5, §6, §8, §9, and §11 are rewritten. Sections marked *(unchanged)* were not affected.

---

## 0. The exact head this pack describes

| | |
|---|---|
| **Branch** | `feat/phase-003-interactive-cli` |
| **Head SHA** | `PENDING_FINAL_HEAD` |
| **Pull request** | [#28](https://github.com/renvor/renvor/pull/28), open and unmerged |
| **Revision** | 2, 2026-08-18 |
| **Supersedes** | Revision 1, at head `08d3f8997ed6c85ab544bc93dff3c8eb07a00a2e` — **approval DENIED** |

**Verify before reviewing**, so that what you read and what you approve are the same tree:

```bash
git rev-parse HEAD            # must equal the head SHA above
git status --porcelain        # must be empty
```

If they do not match, this pack describes a different tree than the one in front of you and the
approval statement in §11 must not be signed. **Every `file.rs:NNN` reference below was
verified against this head by script**, and line numbers drift on any edit, so a mismatch here
makes them unreliable too.

---

## 1. What the two files are for

| File | Responsibility | Lines |
|---|---|---|
| [`crates/renvor-cli/src/paths.rs`](../crates/renvor-cli/src/paths.rs) | Decide whether a requested destination may be written to **at all**, and produce the capability that authorises it | 760 |
| [`crates/renvor-cli/src/generate/place.rs`](../crates/renvor-cli/src/generate/place.rs) | Own a staging directory, and move it onto the destination **atomically or not at all** | 710 |

## 2. The flow, end to end

```text
  renvor new <name> --path <destination>
        │
        ▼
  ┌───────────────────────── paths.rs ──────────────────────────┐
  │ validate_project_name(name)                    paths.rs:423 │
  │   (only when the operator SUPPLIED a name — before any I/O) │
  │     name_not_empty · name_length · single_path_component    │
  │     reserved_device_name · name_character_set               │
  │     name_starts_with_letter                                 │
  │        │ any failure → exit 3, details.rule names which     │
  │        ▼                                                    │
  │ Destination::open(requested)                   paths.rs:148 │
  │   RULE 0  no `..` component ANYWHERE           paths.rs:155 │
  │   RULE 1  final component is one ordinary name paths.rs:182 │
  │   RULE 1b no control chars, no trailing `.`/` `paths.rs:216 │
  │   RULE 2  not a reserved device name           paths.rs:264 │
  │   RULE 3  parent opens ◄─ THE ONE AMBIENT CALL paths.rs:283 │
  │   RULE 4  DESTINATION MUST BE ABSENT           paths.rs:304 │
  │             symlink_metadata(name) — one call, follows       │
  │             nothing, sees everything:                        │
  │               NotFound          ─► the ONLY arm that proceeds│
  │               Ok(_)             ─► destination_exists        │
  │                                    details.found = directory │
  │                                    | file | symlink | other  │
  │               any other error   ─► destination_rejected      │
  │                                    rule=destination_unverifiable
  │                                    FAIL CLOSED, error kept   │
  │        │                                                    │
  │        ▼  returns Destination { parent: Dir, name: String } │
  └────────────────── the capability, not a path ───────────────┘
        │
        ▼
  ┌───────────────────────── place.rs ──────────────────────────┐
  │ Staging::create(&destination)                  place.rs:66  │
  │   creates `.renvor-staging-{pid}-{nanos}-{seq}`             │
  │   INSIDE the destination's parent, never the system tmpdir  │
  │   failure ─► staging_failed  (nothing staged, nothing left) │
  │        │                                                    │
  │        ▼   render → verify → manifest  (all through the Dir)│
  │                                                             │
  │ Staging::place(destination)                    place.rs:158 │
  │   STEP 1  REFUSE an existing destination       place.rs:174 │
  │             symlink_metadata(target), same three arms as    │
  │             RULE 4. ** NOTHING IS REMOVED HERE. **           │
  │   STEP 2  DROP OUR HANDLE                      place.rs:227 │
  │   STEP 3  rename, classify from ErrorKind      place.rs:234 │
  │             on failure: nothing to restore — see STEP 1     │
  └─────────────────────────────────────────────────────────────┘
        │
        ▼  Drop for Staging  place.rs:305 — removes THE STAGING TREE
           on every other path, including a panic. It is the only
           removal in the module.
```

## 3. Security invariants — the things that must be true

| # | Invariant | Enforced by | How |
|---|---|---|---|
| **S1** | No filesystem operation can reach outside the destination's parent | `cap_std::fs::Dir` | **Structural.** After `Destination::open`, there is no ambient path API in scope. A code path that *forgets* to validate still cannot escape. |
| **S2** | Exactly one ambient call exists in the whole program | `paths.rs:283` | `Dir::open_ambient_dir` on the parent the operator typed. Everything downstream is `Dir`-relative. |
| **S3** | No `..` component is accepted anywhere in the requested path | `paths.rs:155` | Lexical, **in addition to** S1. Policy, not containment — see §4. |
| **S4** | A project name is one path component | `paths.rs:423` | `/`, `\`, and `:` refused as `single_path_component`. |
| **S5** | Reserved device names are refused **on every platform** | `paths.rs:48`, `:264`, `:423` | 22 names, matched on the **stem**, case-insensitively. A project made on Linux is opened on Windows. |
| **S5b** | The directory name renvor **creates** carries no control character and no trailing dot or space | `paths.rs:216` (RULE 1b) | Added 2026-08-18 after an advisory security review created a directory with an embedded newline via `--path`. Deliberately **narrower** than the package-name rule: `my.project` and `my project` still work, because rejecting those would break ordinary use to prevent nothing. |
| **S6** | Validation completes before **any** filesystem write | `model.rs::resolve` | `Destination::open` reads and opens; it creates nothing. |
| **S7** | A failure before placement leaves the destination exactly as it was | `Drop` at `place.rs:305` | Runs on every path including panic. |
| **S8** | Placement is one rename, never a copy | `place.rs:234` | Both sides are the same `Dir`, so it cannot cross a filesystem. |
| **S9** | Losing a race is a clean failure, never an overwrite | `place.rs:258` | Classified from the **kernel's own `ErrorKind`**. |
| **S10** | Staging is never the system temporary directory | `place.rs:66` | It sits beside the destination so the final step can be a rename. |
| **S11** | **The destination must not exist, in any form** | `paths.rs:304` (RULE 4) and `place.rs:174` (STEP 1) | Added 2026-08-18. Empty directory, non-empty directory, regular file, symbolic link, dangling symbolic link — all refused with `destination_exists`. `symlink_metadata` and **not** `metadata`, so a link is seen rather than followed and a dangling link does not read as absence. |
| **S12** | **An unverifiable destination fails closed** | `paths.rs:304`, `place.rs:174` | Added 2026-08-18. Only an authoritative `NotFound` proceeds. The previous code asked a second question after the first failed and, when that failed too, fell through to success. The original OS error is carried into the message and `details.error`, not discarded. |
| **S13** | **No production path removes the destination** | `place.rs` — asserted by `no_production_path_removes_the_destination` | Added 2026-08-18. The test reads this module's own source and requires that the single removal in it names `&self.name`, this process's own staging directory. A blunt instrument, used knowingly: the code it replaced was written by someone who also believed it was safe. |

## 4. Traversal and symlinks — read this section carefully

**Two mechanisms, deliberately overlapping.**

**The capability (S1) is what makes escape impossible.** `cap-std` refuses traversal,
absolute paths, and symlinks that leave the tree — *inside the handle*, as a property of the
library rather than of a check anyone remembered to write.

**RULE 0 (`paths.rs:155`) is policy on top of that.** It refuses `..` **in the path the operator
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

- **By name**, RULE 4 (`paths.rs:304`): the destination itself must not exist, and a symbolic link
  is one of the five things that counts as existing. A symlink to a directory is a legitimate thing
  to point at and an illegitimate thing to write a project through. Since 2026-08-18 this is no
  longer a symlink-specific rule — it is the general absence rule, with `details.found = "symlink"`
  saying which case fired. **A dangling symlink is caught too**, which is why the check is
  `symlink_metadata` and not `metadata`: `metadata` follows the link and answers `NotFound` for a
  dangling one, so the destination would have read as absent while the name was already taken.
- **By capability**: a symlink *inside* the tree that points out of it is refused by the handle, and
  **no rule of ours checks for it**. `the_handle_refuses_an_escape_that_no_rule_in_this_module_
  checks_for` (`paths.rs:728`) is the test that proves the capability is doing work the hand-written
  rules do not.

**Data-model §5 rule 8** — "canonical destination is inside the canonical parent" — therefore has
**no `details.rule`**, because there is no check to name. That is what adopting the capability
bought. Reviewers should satisfy themselves this is a trade they accept.

## 5. The empty-destination case — three defects, then the policy itself

**This is the section that changed most in revision 2, and it is the reason approval was denied.**

### 5.1 What FR-013 used to say, and the three defects it drove

It refused a destination that "exists and is **not empty**", so an existing **empty** one was a
**legal target**. That single word drove three defects in sequence:

1. **`open` accepted it and `place` refused it.** `renvor new` into an empty directory validated,
   rendered, ran the **full pre-placement verification** (a `cargo build` and `cargo test`), and only
   then failed — with a message claiming the directory had *"appeared while the project was being
   generated"*, when it had been there all along.
2. **The fix introduced an FR-012 violation.** `place` removed the empty destination immediately
   before the rename; if the rename then failed, the operator's directory was **gone**. A restore
   branch was added.
3. **The restore had to not fire when the race was lost**, or it would recreate a directory another
   process had just legitimately filled. Hence `lost_the_race`.

Each fix was correct about the defect in front of it. Advisory review then found two more:

- **A-R8** — the replacement gives back a **different directory**: new inode, this process's
  umask-derived mode, this process's ownership. `mkdir -m 0700 out` came back as `0755`. Documented
  nowhere until it was measured.
- **A-R9** — the restore branch was **reachable from no test**, and its one recovery action was
  `let _ = self.parent.create_dir(target)`, whose error was discarded.

### 5.2 The ruling: the policy was the defect

The maintainer's ruling of 2026-08-18 did not add a sixth fix to the chain. It removed the thing
they were all fixing:

> *"Change generation policy to require the destination to be ABSENT. Refuse every existing
> destination before any write … Do not delete, rename, chmod, replace, or restore any path the
> operator already owns."*

So:

| Then | Now |
|---|---|
| An empty directory was accepted and **deleted** | Refused. `destination_exists`, `details.found = "directory"` |
| A failed rename **restored** what had been deleted | Nothing is restored, because nothing is removed |
| The restore branch was untested (A-R9) | The branch **does not exist** |
| Mode and inode were silently replaced (A-R8) | Nothing is replaced |
| `remove_dir(target)` in `place` | **Gone.** `no_production_path_removes_the_destination` fails if it returns |

`remove_dir` was genuinely what made the old step safe — the kernel refuses to remove a non-empty
directory, so the emptiness check and the removal were **one atomic operation** rather than a check
followed by a hopeful delete. That reasoning was sound and is preserved in the record. It was
answering the wrong question.

### 5.3 What a reviewer should check here

The claim is *"renvor never deletes a path it did not create"*. Three things support it, and the
third is the one worth attacking:

1. `paths.rs` RULE 4 refuses before anything is staged.
2. `place.rs` STEP 1 refuses again, at the last moment before the rename.
3. `no_production_path_removes_the_destination` reads the module source and requires the single
   removal in it to name `&self.name`. **This is a text scan, and text scans are weak.** It will not
   catch a removal expressed through an alias, a helper, or a different crate.

## 6. TOCTOU — invariant I-17, stated as a limitation

Documented at [`paths.rs:53`](../crates/renvor-cli/src/paths.rs) and [`place.rs:234`](../crates/renvor-cli/src/generate/place.rs).

**The window is narrowed, not closed, and the code does not claim otherwise.**

Between `Destination::open` returning a handle and `place` performing the rename, another process
with write access can create the destination. What holds:

- **One resolution, not two.** Every check and the rename go through the same open handle. A design
  that re-`stat`s a path and then renames *by path* resolves the name twice, and an attacker who wins
  between those two resolutions gets a different directory.
- **Losing is clean.** The rename refuses an existing destination, and `place` checks for absence
  again immediately before it.

### 6.1 The one thing that can still be replaced, stated precisely

**POSIX `rename(2)` silently replaces an existing EMPTY destination directory.** So if another
process creates an empty directory at the destination in the window between STEP 1's check and STEP
3's rename, that directory is replaced rather than preserved.

This is a property of the system call, not a decision this program makes, and after the 2026-08-18
ruling it is the **entire** residual: renvor no longer deletes or replaces a destination deliberately
anywhere.

**Why it is not closed:**

| Approach | Why not |
|---|---|
| `renameat2(RENAME_NOREPLACE)` | **Linux-only.** Not available on macOS or Windows |
| `create_dir(target)` first — atomic create-or-fail — then rename onto our own directory | Closes it on Unix and **breaks Windows**, where `MoveFileEx` refuses a rename onto an existing directory. A fix that makes the command fail on a supported platform is not a fix |
| Re-check immediately before the rename | Already done, at STEP 1. It narrows the window; it cannot close it, because there is always an instruction between the check and the call |

The tests assert the **consequence** — concurrent runs produce exactly one project and clean
failures — rather than an impossibility that does not hold.

**This is the residual risk a reviewer is being asked to accept**, and it is smaller than the one
revision 1 asked for: that version deleted an empty destination *on purpose*.

## 7. Windows versus Unix — three real differences

| Behaviour | Unix | Windows | Handled at |
|---|---|---|---|
| Renaming or deleting a directory with an **open handle** | fine | **refused, `os error 32`** | `place.rs:40`, `:227` — the handle is `Option<Dir>` and is dropped before both |
| Reserved device names (`CON`, `NUL`, `COM1`…) | ordinary names | resolve to **devices** | refused on **both**, `paths.rs:48` |
| A trailing `.` or space in a directory name | kept verbatim | **silently stripped** | refused on **both**, `paths.rs:216` |
| `rename` onto an existing **empty** directory | **replaces it silently** | refused (`AlreadyExists`) | the residual in §6.1. The Unix behaviour is the dangerous one |
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
                                     report `destination_exists`
                                     (another run got there first)

     any other kind ──► lost_the_race = false
                             │
                             ▼
                 report `placement_failed` (the move mechanism broke)

  IN NEITHER BRANCH IS ANYTHING RESTORED, because nothing was removed.
  The recovery arm that used to sit here — `let _ = create_dir(target)`,
  reachable from no test and discarding its own error — is deleted with
  the removal it compensated for. (Findings A-R8 and A-R9.)
```

**Why classify from `ErrorKind` rather than by re-checking the destination**: a re-`stat` is *itself*
racy, and that is exactly how `placement_failed` kept leaking through. A loser reporting
`placement_failed` sends an operator to debug their filesystem when the truth is "somebody else won".
This half is **unchanged** by the 2026-08-18 ruling and remains correct.

**One more failure path, and it fails closed.** If STEP 1's `symlink_metadata` returns an error that
is not `NotFound`, `place` reports `placement_failed` rather than proceeding. An unknown filesystem
state is not absence — the same rule as `paths.rs` RULE 4, applied at the last moment.

## 9. Tests, and what they actually establish

| Test | File | Establishes |
|---|---|---|
| `every_traversal_spelling_is_refused_by_the_traversal_rule` | `tests/hostile.rs` | 5 spellings, each asserting `rule == "no_traversal"` — not merely that it failed |
| `an_absolute_path_in_the_name_position_is_refused` | `tests/hostile.rs` | 4 cases incl. Windows drive-relative `C:name`, asserting `single_path_component` |
| `every_reserved_device_name_is_refused_on_every_platform` | `tests/hostile.rs` | **66 cases** — 22 names × {upper, lower, with extension} |
| `a_destination_that_is_a_symlink_to_another_directory_is_refused` | `tests/hostile.rs` | Escape a lexical check cannot see; witness file outside proves nothing was written |
| `every_project_name_refusal_names_a_distinct_rule` | `tests/hostile.rs` | 5 refusals, 5 distinct rules, no collapsing |
| `an_ordinary_legitimate_destination_still_generates` | `tests/hostile.rs` | **Positive control.** Without it a generator that refuses everything passes the file |
| `the_directory_name_taken_from_a_path_is_checked_too` | `tests/hostile.rs` | 5 cases: embedded newline, tab, trailing `. `, `.`, ` ` — each asserting its own rule |
| `an_ordinary_punctuated_directory_name_is_still_accepted` | `tests/hostile.rs` | **Control for the above**: `my.project`, `my project`, `weird!@#name`, `v1.2.3` all still generate |
| `no_shipped_template_can_write_outside_the_destination` | `tests/hostile.rs` | Diffs the destination's **parent** before and after. **Rewritten 2026-08-18** — the first version compared the destination against itself and could not fail |
| `the_handle_refuses_an_escape_that_no_rule_in_this_module_checks_for` | `paths.rs` | The capability catches what the rules do not |
| `a_failure_at_any_mutating_step_leaves_an_absent_destination_absent` | `tests/transaction.rs` | 5 injected C-5 failures |
| `a_pre_existing_empty_destination_is_refused_before_any_step_can_be_injected` | `tests/transaction.rs` | **New, revision 2.** For every injection point: exit 3, `destination_exists`, `details.injected` **absent** (proving the refusal precedes the injected step), inode/mode/uid/gid unchanged, no staging created. Its predecessor asserted the opposite outcome for the same fixture, so this fails against revision 1 |
| `an_uninjected_run_into_the_same_fixtures_succeeds` | `tests/transaction.rs` | **Positive control** for both of the above |
| `concurrent_runs_at_one_destination_produce_one_project_and_no_corruption` | `tests/transaction.rs` | 3 real processes: 1 wins, 2 lose with `destination_exists`, no residue |
| `every_kind_of_existing_destination_is_refused` | `paths.rs` | **New, revision 2.** 5 cases — empty directory, non-empty directory, regular file, symlink, **dangling** symlink — each asserting `destination_exists`, `rule == "destination_absent"`, and the right `details.found`. Fails against revision 1 on its first case |
| `a_destination_whose_state_cannot_be_established_fails_closed` | `paths.rs` | **New, revision 2.** A real `0600` parent, so lookups inside return `PermissionDenied` and never `NotFound`. Probes first and skips if the denial did not take effect (root, or a filesystem that ignores the bits) rather than passing vacuously |
| `an_empty_destination_that_appears_mid_run_is_refused_and_not_replaced` | `place.rs` | **New, revision 2.** Compares inode, mode, uid, gid before and after. Revision 1 **succeeded** in this scenario and replaced the directory |
| `a_file_that_appears_at_the_destination_mid_run_is_refused_and_left_alone` | `place.rs` | **New, revision 2.** The file's contents survive byte-identical |
| `no_production_path_removes_the_destination` | `place.rs` | **New, revision 2.** Reads the module's own source; the single removal must name `&self.name`. Carries its own positive control — a scan matching nothing would pass vacuously |
| `racing_placements_…` | `place.rs` | 16 threads directly on `place` — the test that actually reproduces the race |
| `a_killed_run_leaves_identifiable_residue_beside_the_destination_and_no_project` | `tests/transaction.rs` | `SIGKILL`; residue is beside, named, and no project exists |
| `staging_names_are_unique_within_one_process` | `place.rs` | 256 names, all distinct |

**Result at the head of this branch**: all pass — **212 tests** in `renvor-cli`, **557** across the
workspace — on `ubuntu-latest`, `macos-latest`, and `windows-latest`, on both `1.94.0` and `stable`.

## 10. What a reviewer should try to break

Suggested, in descending order of value:

1. **Find a path that reaches outside the parent.** The claim is that this is structurally
   impossible after `Destination::open`. Attack the window *before* it, and attack RULE 3's ambient
   call.
2. **Find any way to make renvor delete, rename, `chmod`, or replace a path it did not create.**
   This is the central claim of revision 2, and §5.3 names the weakest support for it: a text scan
   of one module. A removal reached through an alias, a helper function, or another crate would pass
   that scan.
3. **Find a destination state that reads as absent.** The absence check is one `symlink_metadata`
   call with three arms. A device node, a FIFO, a mount point, a case-insensitive-filesystem
   collision, or a Windows junction that answers something unexpected would be a finding.
4. **Find a `details.rule` that fires for the wrong reason.** This has happened twice.
5. **Find a Windows behaviour the tests do not exercise.** Two defects were Windows-only, and
   `a_destination_whose_state_cannot_be_established_fails_closed` is `#[cfg(unix)]` — the fail-closed
   rule has **no Windows-specific test**.
6. **Challenge the I-17 trade-off** (§6.1). It is a stated residual risk, not a solved problem, and
   the empty-directory replacement it describes is a real data-loss window on Unix.
7. **Challenge the `schemaVersion` decision.** Retiring `destination_not_empty` and adding four codes
   was ruled a breaking change and bumped `1 → 2`. If you think adding codes alone would not have
   required it, the reasoning is in `contracts/json-output.md` *Schema history*.

---

## 11. Approval statement

**Use this only after reading §§2–10 and the two files themselves.** It is deliberately narrow: it
approves two files, not the phase.

> I have read `crates/renvor-cli/src/paths.rs` and `crates/renvor-cli/src/generate/place.rs` in
> full, together with §§2–10 of **revision 2** of this pack.
>
> I understand and accept:
> - that path containment is **structural** (`cap_std::fs::Dir`) rather than checked, and that
>   data-model §5 rule 8 therefore has no named rule;
> - that `renvor new` **refuses every existing destination** — empty directory, non-empty directory,
>   regular file, symbolic link, dangling symbolic link — and refuses one whose state cannot be
>   established, and that it therefore **never deletes, renames, `chmod`s, replaces, or restores a
>   path the operator already has**;
> - that the time-of-check-to-time-of-use window described in invariant I-17 is **narrowed and not
>   closed**, and specifically that **POSIX `rename(2)` will silently replace an empty directory
>   another process creates in the window between the last check and the rename**, which is not
>   portably closable today;
> - that the strongest guard against a future removal returning to `place.rs` is a **source-text
>   scan**, which would not catch a removal expressed indirectly.
>
> I approve these two files for merge as part of Phase 003.
>
> Name: ______________________  Date: ____________
>
> Commit reviewed: `__________________________________________`  ← must match the head named in §0

**This approval does not close Phase 003.** It covers two files. See
[`phase-003-evidence.md`](phase-003-evidence.md) §8 for what else is outstanding — as of revision 2
that is a qualified independent human requirements review and security review, and nothing else.
