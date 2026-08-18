# Contract C-5 — The generation transaction

**Status**: defined before implementation. **This is the safety core of Phase 003.**

Everything else in this phase is a convenience. This is the part that must not be wrong, because it
is the part that touches a directory somebody cares about.

## An existing empty destination is REPLACED, not written into

FR-013 refuses a destination that "exists and is **not** empty", so an existing **empty** one is a
legal target. What placement does with it is `remove_dir` followed by the rename — so the directory
the operator ends up with is a **different** directory: a new inode, with default permissions,
ownership, and extended attributes rather than whatever was set on the original.

Measured, not assumed: a destination created with mode `0700` comes back as `0755`, and the inode
changes.

**This is recorded because it was true and written down nowhere** — not in this contract, not in the
spec, not in the published documentation — until an advisory review measured it on 2026-08-18. It is
a consequence of `remove_dir` being what makes the emptiness check atomic (the kernel refuses to
remove a non-empty directory, so check-and-remove are one operation rather than a check followed by
a hopeful delete), and that trade is worth keeping. Silently discarding an operator's deliberate
`chmod` is not worth keeping quiet about.

## The protocol

```text
1. VALIDATE      every choice, every cross-choice constraint, and the destination boundary
                 ── nothing has touched the filesystem yet ──
2. STAGE         create a uniquely named directory INSIDE the destination's PARENT
3. RENDER        expand templates into the staging directory, under bounds
4. MANIFEST      walk the staging tree, produce the sorted manifest
5. VERIFY        the generated project formats, compiles, tests, and starts
6. PLACE         one rename: staging directory ──▶ destination
7. REPORT        result to stdout, progress already on stderr
```

**Failure at any step from 1 to 5 removes the staging directory and leaves the destination exactly as
it was.** Failure at 6 leaves the destination as it was and reports `placement_failed`.

## Why staging goes in the destination's parent

Not the system temporary directory. The reason is concrete rather than stylistic:

FR-016 forbids falling back to a non-atomic copy when the rename cannot be atomic. On most Linux
containers `/tmp` is a **different filesystem** from the working tree, so staging there would make
the forbidden fallback the ordinary case rather than the exceptional one — and a rule that fires on
every run is a rule that gets deleted.

Staging inside the destination's parent makes the rename **same-filesystem by construction**. The
cross-device case is not handled; it is made unreachable.

## Atomicity, stated per platform rather than claimed uniformly

| Platform | Guarantee |
|---|---|
| POSIX | `rename(2)` onto a non-existent path within one filesystem is atomic |
| Windows | The nearest equivalent onto a non-existent path. **This phase does not claim POSIX-equivalent atomicity on Windows** |

FR-013 guarantees the destination does not already exist, which is what makes the weaker Windows
guarantee sufficient here. **The limit is documented rather than assumed away** (FR-016).

## What "verify before placing" means

Step 5 runs the generated project's own checks **while it is still in staging**. A project that does
not build is therefore a **generation failure**, reported as such, with nothing at the destination —
rather than something the user discovers ten minutes later (FR-030).

This is the step that makes SC-005 an assertion about the generator rather than about the templates.

## Residue

A process killed between steps 2 and 6 leaves a staging directory behind. That is unavoidable
without a supervising process, and it is specified rather than ignored:

- The staging directory name is **identifiable as Renvor's** and carries the process identity.
- It is **beside** the destination, never inside it, so residue never becomes part of a project.
- `renvor doctor` reports orphaned staging directories it finds beside a destination, and does not
  delete them without being asked. **Deleting a directory that merely looks like residue is exactly
  the class of action this whole contract exists to prevent.**

## Concurrency

Two runs targeting one destination: **at most one succeeds**, and the other fails cleanly (FR-015).
Each stages in its own uniquely named directory, so the renders never interleave; the loser's rename
finds the destination occupied and reports `destination_not_empty`.

## The race this narrows and does not eliminate

*(Revised 2026-08-18 with research.md D6 revision 2.)*

`cap-std` is adopted, so the parent directory is opened **once** and every subsequent operation —
the staging create, every render write, the existence re-check, and the rename — goes through that
one handle. Nothing is re-resolved from a string, so an attacker cannot change **what the parent
means** part-way through: the handle refers to the directory that was opened, not to whatever the
path spells now.

**What remains.** A process with write access to that same directory can still create the
destination *name* inside it between the re-check and the rename. The rename targets a path that
must not exist, so the outcome is a **clean failure rather than an overwrite** — and the staged tree
is removed by `Drop`, so the destination is untouched either way.

**This contract does not claim the race is closed.** Closing it needs an atomic
create-or-fail rename primitive, which POSIX `renameat` does not provide and `renameat2` provides
only on Linux. A cross-platform generator cannot rely on it, so the residual window is specified
here rather than papered over.
