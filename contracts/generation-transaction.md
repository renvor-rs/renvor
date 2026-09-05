---
description: "Contract C-5 — the generation transaction and its destination-safety guarantees"
version: "1.1.0"
status: "normative — the safety core of the generator. 1.1.0 (2026-09-05, Phase 011 correction round): the sealed environment strips proxy credentials and a check's output is redacted before it is reported. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
---

# Contract C-5 — The generation transaction

**Status**: defined before implementation. **This is the safety core of Phase 003.**

Everything else in this phase is a convenience. This is the part that must not be wrong, because it
is the part that touches a directory somebody cares about.

## The destination MUST NOT EXIST. Nothing here deletes

*Revised 2026-08-18 by maintainer ruling. This section previously described the opposite behaviour;
what it described was real, and is what the ruling removed.*

FR-013 refuses **every** existing destination, before anything is staged:

| What is there | Answer |
|---|---|
| nothing | the only case that proceeds |
| an empty directory | `destination_exists`, `details.found = "directory"` |
| a non-empty directory | `destination_exists`, `details.found = "directory"` |
| a regular file | `destination_exists`, `details.found = "file"` |
| a symbolic link, including a dangling one | `destination_exists`, `details.found = "symlink"` |
| anything whose state cannot be established | `destination_rejected`, `details.rule = "destination_unverifiable"`, carrying the original OS error |

`details.rule` is `destination_absent` for every row but the last: the rule that was violated is
that the destination must be absent.

**No production path in this transaction removes the destination.** The previous version deleted an
existing *empty* destination with `remove_dir` and let the rename create a fresh one — so the
operator's directory came back with a different inode and this process's mode and ownership, a
`0700` directory returning as `0755` (finding A-R8) — and restored it, ignoring its own error, if
the rename then failed (finding A-R9). Both halves are gone. `crates/renvor-cli/src/generate/place.rs`
carries a test, `no_production_path_removes_the_destination`, that reads the module's own source and
fails if any removal names anything but this process's own staging directory.

### What "fail closed" means here

An error from inspecting the destination is only treated as absence when it is an authoritative
`NotFound`. Any other error — a permission denial, an I/O error, an unreadable parent — refuses.
The previous code asked a second question after the first failed and, when that also failed, **fell
through to success**, so a destination whose state could not be read at all was treated as absent
and generation proceeded.

### The one residual, stated rather than designed around

POSIX `rename(2)` **silently replaces an empty destination directory**. Steps 1 and 6 both check for
absence, but another process can create an empty directory in the window between the last check and
the rename, and that directory is then replaced. Closing this needs an atomic
create-directory-or-fail rename, which no portable API provides: `renameat2(RENAME_NOREPLACE)` is
Linux-only, and the portable-looking substitute — create the destination first, then rename onto it
— fails on Windows, where `MoveFileEx` refuses a rename onto an existing directory. See invariant
I-17.

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

**The checks run in a sealed environment** (Phase 011). The staged project's `cargo` sees what
the toolchain needs — `PATH`, `HOME`, `CARGO_HOME`, `RUSTUP_*`, `RUSTFLAGS`, proxy and certificate
variables — and nothing else the operator's shell carries: no `RENVOR_*`, no credential. A
`RENVOR_DATABASE_URL` in the shell must not let generation reach a database, and a gate's
`RENVOR_TEST_REQUIRE_DATABASE=1` must not turn a staged project's skip into a failure. The build
directory is `CARGO_TARGET_DIR` when set and absolute, else a temporary directory.

**"No credential" includes the proxy variables** (1.1.0, 2026-09-05, the Standards review of
Phase 011). `CARGO_HTTP_PROXY`, `HTTP_PROXY`, `HTTPS_PROXY`, `http_proxy`, and `https_proxy` pass
through with any `user:password@` **removed** — the scheme, host, port, and path survive, so a
proxy that needs no credential still routes; a value that is not text is dropped. Verification
needs no registry update (the framework's lockfile seeds resolution), so an authenticated proxy is
not something the checks have to be able to use, and a credential the seal let through would
reach every build script and dependency the staged project compiles.

**A child's output is reported redacted, never raw.** When a check fails, the message carries
the tool's stdout and stderr after every URL credential is replaced, every credential the seal
removed is replaced, and every control character is escaped — a build script cannot put a
credential or a terminal sequence into the operator's error.

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
finds the destination occupied and reports `destination_exists`.

## The race this narrows and does not eliminate

*(Revised 2026-08-18 with [Phase 003 research §D6](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/research.md) revision 2.)*

`cap-std` is adopted, so the parent directory is opened **once** and every subsequent operation —
the staging create, every render write, the existence re-check, and the rename — goes through that
one handle. Nothing is re-resolved from a string, so an attacker cannot change **what the parent
means** part-way through: the handle refers to the directory that was opened, not to whatever the
path spells now.

**What remains.** A process with write access to that same directory can still create the
destination *name* inside it between the re-check and the rename. The rename targets a path that
must not exist, so the outcome is a **clean failure rather than an overwrite** — and the staged tree
is removed by `Drop`, so the destination is untouched either way. The **one** exception is an empty
directory created in that window, which POSIX `rename(2)` replaces silently; see *The one residual*
above.

**This contract does not claim the race is closed.** Closing it needs an atomic
create-or-fail rename primitive, which POSIX `renameat` does not provide and `renameat2` provides
only on Linux. A cross-platform generator cannot rely on it, so the residual window is specified
here rather than papered over.
