---
description: "Contract C-5 — the generation transaction and its destination-safety guarantees"
version: "1.2.0"
status: "normative — the safety core of the generator. 1.2.0 (2026-09-07, Phase 012, L-2): the sealed environment forces `RUSTUP_AUTO_INSTALL=0`, no longer passes `RUSTUP_DIST_SERVER` or `RUSTUP_UPDATE_ROOT`, refuses rustup below 1.28.1, an unidentifiable proxy, and a pinned-but-absent toolchain by name before any check runs, records, per check, the outcome, the launch observation, and the queried identities (release, commit, host; override and wrapper presence only) — or that cached artifacts were reused with no launch observed — and applies the same seal to `rustfmt` at generation and to `doctor`'s probes; `generate auth` stages its scratch copy beside the project so the project's toolchain selection is preserved. The protocol, atomicity, and residue rules are unchanged; the seal is not a sandbox for trusted wrappers or build scripts. The protocol block is CORRECTED to the order the implementation has followed since C-4 1.2.0 — VERIFY, then RECORD (the provenance record is written after verification and before the manifest), then MANIFEST, then the review, then PLACE — and renumbered: VERIFY is step 4 (step 5 in 1.1.0's numbering, which the Phase 012 brief cites); its verb is unchanged and its reading is stated in the body (A-8, 2026-09-07). 1.1.0 (2026-09-05, Phase 011 correction round): the sealed environment strips proxy credentials and a check's output is redacted before it is reported. first explicit version assigned to this contract text on 2026-08-19; earlier revisions are in public Git history. This version identifies the contract text, not a stability promise"
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

POSIX `rename(2)` **silently replaces an empty destination directory**. Steps 1 and 7 both check for
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
4. VERIFY        the generated project formats, compiles, tests, and starts
5. RECORD        write .renvor/generated.toml — after verification resolved the lockfile,
                 from what step 4 observed and queried, before the manifest so it is listed
6. MANIFEST      walk the staging tree, produce the sorted manifest
   (REVIEW)      show the manifest and ask for consent — waived by --yes, absent on a dry run;
                 declining is `cancelled`, exit 4, and drops the staging tree
7. PLACE         one rename: staging directory ──▶ destination
8. REPORT        result to stdout, progress already on stderr
```

*(1.2.0 corrects the order and the numbering. 1.1.0 listed MANIFEST before VERIFY and had no
RECORD line; the implementation has verified first since Phase 003 — the manifest must describe
the tree that is placed, `Cargo.lock` included — and has written the record between the two since
C-4 1.2.0. The review screen is shown in parentheses because it is conditional: it needs a
terminal, `--yes` waives it, and a dry run has nothing to consent to. Where an older document
says "C-5 step 5" it means VERIFY.)*

**Failure at any step from 1 to 6 removes the staging directory and leaves the destination exactly
as it was**, and so does a declined review. Failure at 7 leaves the destination as it was and
reports `placement_failed`.

**What step 4's — VERIFY's — "compiles" means** (step 5 in 1.1.0's numbering, which the Phase 012
brief and ADR-0038 cite; 1.2.0; the maintainer's reading, A-8, 2026-09-07, recorded in
[ADR-0038](../decisions/0038-generated-toolchain-declaration-verified-with-and-the-rustup-floor.md),
which is `proposed`): *the required `cargo build` check succeeds, valid cache reuse included — no
fresh compiler execution is asserted; format, lint, test, and start stay mandatory.* A check whose
relevant units Cargo positively reports `Fresh` passes with its cached status stated explicitly and
its observed compiler identity unavailable (see *What "verify before placing" means* below); a
failure to capture the evidence stays a failure and is never read as caching. The verb in the
protocol block is deliberately unchanged: a project that does not build is still a generation
failure, and `CONSTITUTION.md`'s "Generated projects must format, **compile**, migrate, start, and
execute representative operations" is read the same way, without editing the constitution.

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

Step 4, VERIFY, runs the generated project's own checks **while it is still in staging**. A project that does
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

**The seal provisions nothing** (1.2.0, Phase 012, FR-012-6). The sealed environment sets
`RUSTUP_AUTO_INSTALL=0` **unconditionally** — whether or not the caller set it — and omits
`RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT`, the two variables that configure where rustup
installs from, for **every** child it spawns: the five checks, the floor check and the
identification probe below, the resolution probe, the identity queries, the component query,
`rustfmt` at `renvor generate resource`, and `renvor doctor`'s probes. `RUSTC_WORKSPACE_WRAPPER`
joins the pass-through beside `RUSTC_WRAPPER` (the record's `wrapper` boolean covers both). No
toolchain is installed, listed, updated, or set as a default by anything this contract runs
(SR-012-1, SR-012-4). The seal's other rules — the allow-list, the credential strip, the redaction
— are unchanged. **No provisioning is not no network**: `RUSTUP_AUTO_INSTALL=0` stops rustup from
installing a toolchain; it does not stop Cargo from fetching crates, which stays governed by the
seeded lockfile and `CARGO_NET_OFFLINE` exactly as before (SR-012-5).

**Identify before invoking** (1.2.0, FR-012-7a/7b). A rustup proxy run in a pinned directory, or
with an absent toolchain named, could — before rustup 1.28.0 — install that toolchain from a
listing as innocent as `rustup --version`, which also resolves the active toolchain to report its
`rustc`. So **nothing runs a proxy in a pinned directory, or with an absent toolchain, until that
proxy's rustup is known to honour the no-install guarantee**, and this order is followed before
any check runs — the identification (1–4) depends on no directory and precedes every invocation in
the tree being verified; the resolution probe (5) runs in that tree:

1. **Locate `rustup` without executing a proxy** — on `PATH`; else beside the `rustc` that `PATH`
   resolves; else `$CARGO_HOME/bin/rustup`; else `~/.cargo/bin/rustup`.
2. **The floor.** A located `rustup` answers `rustup --version` **under isolation** — an
   exclusively created empty working directory with no toolchain file above it, `RUSTUP_HOME` and
   `CARGO_HOME` pointed at exclusively created empty directories, `RUSTUP_TOOLCHAIN` unset,
   `RUSTUP_AUTO_INSTALL=0`, `RUSTUP_DIST_SERVER` and `RUSTUP_UPDATE_ROOT` set to an unroutable
   loopback address in that child only, a bounded timeout — so it is never run where a toolchain
   is named. Only the first stdout line is parsed. Unparseable, or below **1.28.1** (the release
   that introduced `RUSTUP_AUTO_INSTALL`), is `tool_missing`, exit `5`,
   `details.tool = "rustup >= 1.28.1"`, and no proxy is invoked.
3. **Classify `rustc` and `cargo` without running them in the pinned directory** — each is a proxy
   of the located rustup when it is the same file after symlinks are followed (same device and
   inode on Unix; identical bytes on Windows, where the proxies are copies). A proxy of a rustup at
   or above the floor is *identified*.
4. **Otherwise, the isolated identification probe** — the binary is run once with `-vV` under the
   same isolation as the floor check. A bare compiler ignores every `RUSTUP_*` variable and prints its identity:
   *bare*, `proxy = false`. An answer in rustup's own words (no default toolchain, nothing
   installed, `rustup` named) is *a proxy whose rustup could not be located* and is **refused
   explicitly** — `tool_missing`, exit `5`, `details.tool = "rustup >= 1.28.1"`,
   `details.reason = proxy_unidentified`, the remedy being to put the `rustup` that owns these
   proxies first on `PATH`. Anything else, or the timeout, is `project_verification_failed`,
   `details.reason = compiler_identity_unreadable`.
5. **The resolution probe runs only after the tools are identified**: `rustc -vV` and `cargo -vV`
   in the directory whose selection is being measured (the staging directory for `renvor new`;
   the project directory and then the scratch copy for `generate auth`), under the seal, followed
   by `rustup show active-toolchain` when rustup is located, and by `rustfmt --version` and
   `cargo clippy --version` for the components. rustup's "is not installed" is `tool_missing`,
   `details.tool = "rustup toolchain <channel>"`, with the exact `rustup toolchain install …`
   remedy — **before step 4 VERIFY runs any check**: for `renvor new` the probe runs in the
   staged tree, after step 3 RENDER has written the pin it measures, and a refusal removes the
   staging directory like any other failure, so **nothing is placed**; a compiler below the
   project's `rust-version` is `tool_missing`, `details.tool = "rustc >= <msrv>"` (the generator's
   prerequisite, applied to the compiler the preflight resolution names — not a claim about
   Cargo's effective compiler under `RUSTC`, `build.rustc`, or a wrapper); a missing component is
   `tool_missing` naming it. On an identified proxy at or above the floor, one further in-run
   witness — the resolved `rustc` run once with `RUSTUP_TOOLCHAIN` set to a name that cannot be
   installed — confirms that this rustup answers "is not installed" and downloads nothing; any
   other answer is `tool_missing`, `details.reason = no_install_guarantee_unconfirmed`. It is never
   run on an unidentified binary.

**The evidence — launch observation plus queried identity** (1.2.0, FR-012-7d as amended by the
maintainer on 2026-09-07). The five checks run unchanged in what they compile, run, and require;
the three that can launch a compiler take the output flag `-vv` (`cargo clippy --all-targets -vv
-- -D warnings`, `cargo build -vv`, `cargo test -vv`; `fmt` launches none; `cargo run --quiet`
launches none after `build`). For each check the record carries the **outcome**; the **observed
launch chains** of the project's own units, parsed from Cargo's `Running` lines (every
`NAME=value` token stripped; wrapper presence as a boolean; paths never); and, separately, the
**queried identity** of the launched compiler — the last binary of the build/test chain, run once
with `-vV` under the same seal. For clippy the launch chain is `[clippy-driver, rustc]` and the
trailing `rustc` argument is **not** clippy's executing compiler, so the observed `clippy-driver`
**executable** — the one the `Running` line names — is queried **itself**, with
`clippy-driver --version`, under the seal and after the identification safeguards above; `cargo
clippy --version` stays the component query of the preflight and **never** fills the driver's
identity, whether or not its answer would match. **Cached checks are represented explicitly**: a
successful check whose relevant units Cargo positively reports `Fresh` may pass without a compiler
launch, and the record then says `observation = "cached"` (or `"mixed"`, with both counts), leaves
the observed identity **absent** — observed compiler identity unavailable, never filled from the
pin, `PATH`, an old record, or `.rustc_info.json` — and one stderr line names the checks whose
units were all `Fresh`: `verification reused cached artifacts for <checks>: no compiler launch
observed`. A cached clippy check records no driver identity. **Capture failure is not caching**:
every unit of the project's own package(s) must be accounted for by a `Running` line or a positive
`Fresh` report; a missing line, a truncated or malformed stream, a parse failure, or a `-vV`
answer outside the identity grammar is `project_verification_failed`,
`details.reason = evidence_capture_failed` (an identity outside the grammar keeps
`compiler_identity_unreadable`), redacted as above, nothing placed — and **never** recorded as
cached. The generator forces no incremental compilation, adds no target, changes no emit flag,
touches no timestamp, and clears no cache to manufacture an observation.

**The seal is not a sandbox.** A wrapper, a `RUSTC` override, `RUSTFLAGS`, and every build script
of every dependency still run with the operator's rights inside the "sealed" step. What the seal
guarantees is narrower and stated exactly — no secret of the operator's shell reaches the child
(1.1.0), no toolchain is provisioned (1.2.0), and what was launched and what was queried is
recorded, a cached run recorded as such. A trusted wrapper may execute something else, and no
field of the record claims otherwise (SR-012-3).

## Residue

A process killed between steps 2 and 7 leaves a staging directory behind. That is unavoidable
without a supervising process, and it is specified rather than ignored:

- The staging directory name is **identifiable as Renvor's** and carries the process identity.
- It is **beside** the destination, never inside it, so residue never becomes part of a project.
- `renvor doctor` reports orphaned staging directories it finds beside a destination, and does not
  delete them without being asked. **Deleting a directory that merely looks like residue is exactly
  the class of action this whole contract exists to prevent.**
- `renvor generate auth` verifies the merged tree in a **scratch copy staged beside the project**
  — `<parent>/.renvor-staging-<pid>-…`, the same naming — never under the system temporary
  directory (1.2.0, Phase 012, FR-012-13), so the copy shares the project's ancestors and
  therefore the directory overrides and ancestor toolchain files that select its compiler; the
  resolution probe runs first in the project directory and then in the copy, and the two must
  agree or the run is `project_verification_failed`,
  `details.reason = toolchain_resolution_diverged`, with nothing written. The copy is removed on
  completion and on failure; killed mid-run, it is residue of the same kind, beside the project
  and reported by `doctor` the same way.

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
