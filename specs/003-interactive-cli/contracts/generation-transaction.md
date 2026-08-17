# Contract C-5 — The generation transaction

**Status**: defined before implementation. **This is the safety core of Phase 003.**

Everything else in this phase is a convenience. This is the part that must not be wrong, because it
is the part that touches a directory somebody cares about.

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

## The race this does not eliminate

Between the boundary check and the rename, an attacker with write access to the destination's parent
can change what the destination names. The rename refuses an existing destination, which converts
that race into a **clean failure rather than an overwrite**.

**It is not eliminated, and this contract does not claim it is.** Eliminating it needs directory
handles held across the whole operation — which is `cap-std`, which is the D6 decision record, which
is why that record blocks the path-containment component from merging.
