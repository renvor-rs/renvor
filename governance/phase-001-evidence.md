# Phase 001 Evidence Pack

**Status**: Open — implementation in progress, **65 of 102 tasks complete** (14 web-property tasks added 2026-08-11 by `PLAN.md` §26). Verification passes on both toolchains with exit 0. Stopped before T054 (first push) pending maintainer approval.
**Satisfies**: spec FR-042, FR-043; PLAN.md §6.2
**Schema**: `specs/001-governance-foundation/data-model.md`
**Gates**: this record gates entry to Phase 002. It is complete only when every acceptance criterion below carries dated evidence and `open_blockers` is empty.

---

## 1. Tooling versions (T004)

Recorded 2026-08-11 on `darwin/aarch64`.

| Tool | Required by | Version | Status |
|---|---|---|---|
| rustup | T005 | 1.29.0 (28d1352db 2026-03-05) | ✅ present |
| cargo-deny | T035 dependency and licence policy | 0.19.6 | ✅ present |
| gitleaks | T013, T053 secret scans | 8.30.1 | ✅ present |
| lychee | T068 link checking | 0.24.2 | ✅ installed 2026-08-11 via `cargo install --locked lychee` |
| node | T064 documentation site | v22.12.0 | ✅ present |
| npm | T064 documentation site | 10.9.0 | ✅ present |

All five tools required by `contracts/verification-sequence.md` are present. `lychee` was
the only one missing and was built from source with `--locked` (1m 27s, release profile).

## 2. Rust toolchains (T005)

| Toolchain | Required by | rustc | rustfmt | clippy | Notes |
|---|---|---|---|---|---|
| `1.94.0` (pinned) | MSRV verification job | 1.94.0 (4a4ef493e 2026-03-02) | ✅ | 0.1.94 | Installed 2026-08-11; components added |
| `1.97.1` (pinned) | stable verification job | 1.97.1 | ✅ | 0.1.97 | Pre-existing; **components were missing and were added 2026-08-11** |
| `1.95.0`, `1.90.0` | — | — | — | — | Pre-existing on this machine, unused by this phase |

**Finding — both toolchains lacked the components the verification sequence needs.**
The pinned 1.94.0 installs with `--profile minimal` (no rustfmt, no clippy), and the
pre-existing 1.97.1 was also missing rustfmt. Verification steps 2 and 3 (`cargo fmt
--check`, `cargo clippy`) would have failed on both. `rustup component add rustfmt clippy`
was run for each. This is why `rust-toolchain.toml` (T028) must declare its components
explicitly rather than assuming a default profile.

**Finding — the local `stable` channel is stale.** `rustup show` reports `stable` as the
active default, but `rustc --version` under it returns **1.94.0**, while the current
stable release is **1.97.1** (released 2026-07-16, verified against primary sources in
research.md). The `stable` channel on this machine has not been updated since roughly
March 2026.

Consequence: a verification run invoking `stable` here would silently test 1.94.0 twice —
the MSRV job and the "stable" job would exercise the same compiler, and the stable job
would prove nothing. CI is unaffected (runners resolve `stable` fresh), but local runs are.

**Not remediated.** `rustup update stable` changes the machine's global default toolchain
outside this repository, which exceeds the authorised scope of T005. Recorded here for
the maintainer to action.

## 3. Repository state audit (baseline, pre-cleanup)

Observed 2026-08-11, before any cleanup task ran. See research.md Finding 11.

| Observation | Value |
|---|---|
| Commits | 2 (`1b182d0` Initialization, `bfb6925` docs: establish project governance) |
| Files in `HEAD` | 30 |
| Staged but never committed | 4 × `.idea/*`, plus `.gitignore` |
| Loose objects | 313, 65.81 MiB |
| Unreachable objects | 125 blobs, 24 commits, 73 trees (largest blob 7.05 MB) |
| Default branch | `master` (program convention is `main`) |
| Remotes configured | none |

Unreachable objects are **not** transmitted by `git push`, so this is local weight rather
than public exposure. Remediation is scheduled at T010–T014 and is **not yet authorised**.

## 3a. Publish set (T009, maintainer decision 2026-08-11)

Every ambiguous item carries an explicit include-or-exclude decision. Silence is not a
decision.

| Item | Decision | Reason |
|---|---|---|
| `PLAN.md` | **Include** | The program execution authority; nothing sensitive |
| `specs/` | **Include** | The governance audit trail; FR-042 evidence must be reviewable |
| `CONSTITUTION.md` | **Include** | Authoritative public copy created 2026-08-11 so FR-012 is satisfied without publishing `.specify/` |
| `RENVOR_FRAMEWORK_DEVELOPMENT_PLAN.md` | **Exclude** | Superseded by PLAN.md |
| `RENVOR_MASTER_IMPLEMENTATION_PLAN.md` | **Exclude** | Superseded by PLAN.md |
| `Branding/` | **Exclude** | Separately managed brand package with separate asset licensing; not covered by the `MIT OR Apache-2.0` grant. 1.4 GB on disk |
| `.claude/` | **Exclude** | Local development-tool configuration; must not appear in the public framework repository |
| `.claude-flow/` | **Exclude** | Same |
| `.specify/` | **Exclude** | Local integration-specific tooling metadata. Its one publicly required artifact — the ratified constitution — is now published as `CONSTITUTION.md` |

### Constitution relocation

`.specify/memory/constitution.md` was copied to `/CONSTITUTION.md` on 2026-08-11 and the
Spec Kit "Sync Impact Report" header — itself integration-specific tooling metadata — was
stripped, leaving a neutral public document. Content is otherwise byte-identical:
**Version 1.0.0, Ratified 2026-08-11**.

The local Spec Kit files remain on disk and functional; they are ignored rather than
deleted. `T050` now links `CONSTITUTION.md` as the discoverable copy.

## 3b. ⛔ Pre-push blocker — unpublished history contains excluded material

**Raised 2026-08-11. Blocks T054 (first content push). Requires separate explicit maintainer approval.**

`.gitignore` prevents *future* commits of the excluded paths. It does **not** remove what
is already committed. Both existing commits predate the T009 decision:

| Excluded path | Present in committed history? |
|---|---|
| `.claude/skills/**` (10 files) | **Yes — in `HEAD`** |
| `.specify/**` (20 files, incl. scripts, templates, manifests, constitution) | **Yes — in `HEAD`** |

Pushing as-is would publish every excluded item, permanently, to a public repository —
defeating the T009 decision entirely.

**Requirements before this blocker can be cleared** (none performed, none authorised):

1. A verified, restorable backup of the full repository including all refs and reflogs.
2. A written, reviewed cleanup plan for the two unpublished commits.
3. Separate explicit maintainer approval to execute it.
4. Post-cleanup verification that `git log --all --name-only` contains **zero** excluded paths.

**Nothing excluded above may remain in the eventual public Git history.** No history
rewrite, reflog expiry, prune, or garbage collection has been performed.

## 3c. T012 unreachable-object inventory — **superseded, see §3c-rev**

The inventory first gathered on 2026-08-11 at ~18:0x reported 380 loose objects, 140
unreachable blobs, 54 unreachable commits, 91 unreachable trees, and concluded that
"pruning would destroy no content that exists only in the object store."

**Both the counts and the conclusion were wrong**, and are retained here only so the
correction is auditable. The counts were stale within minutes (see §3c-rev "Why the counts
move"), and the conclusion rested on inspecting `PLAN.md` variants and large blobs alone
rather than comparing every unreachable blob against the filesystem.

## 3c-rev. Corrected unreachable-object inventory (2026-08-11 18:50, **no pruning performed**)

Measured immediately before the §3d backup was taken, so these numbers describe exactly
what that backup contains.

| Measure | First inventory | **Measured at backup** |
|---|---|---|
| Loose objects | 380 | **393** |
| Unreachable blobs | 140 | **152** |
| Unreachable commits | 54 | **56** |
| Unreachable trees | 91 | **100** |

### Why the counts move

The unreachable commits are `git stash` snapshots — 28 `WIP on master:` plus 28 `index on
master:`, one pair per snapshot, spanning **13:33:59 to 18:49:02 on 2026-08-11**. The
newest pair is timestamped to the second at which a session prompt was submitted.

No Git hook creates them: `core.hooksPath` is unset, `.git/hooks` holds only samples, and
no file under `.claude/`, `.specify/`, or `.remember/` references `git stash`. The
snapshots originate from session tooling outside the repository. `git stash list` is empty
throughout, so each snapshot is dropped as soon as it is taken.

**Consequence: this count is a moving target.** Any pruning decision must be re-measured
immediately before it executes; a figure recorded even minutes earlier will understate it.

### What would actually be lost by pruning — corrected method

Every one of the 152 unreachable blobs was hashed against **all 122,655 files currently on
disk** (32,101 distinct blob hashes), rather than spot-checking selected paths.

| Class | Count | Assessment |
|---|---|---|
| Byte-identical to a file on disk right now | **126** | Recoverable from the working tree. Pruning loses nothing. |
| **Exist only in the object store** | **26** | **Would be destroyed by pruning.** Itemised below. |

The 26 object-store-only blobs, identified by content inspection:

| Content | Count | Bytes |
|---|---|---|
| Earlier drafts of `specs/001-governance-foundation/` artifacts — `spec.md` ×2, `tasks.md` ×2, `research.md`, `plan.md`, `data-model.md`, `quickstart.md`, `checklists/requirements.md` ×2, `contracts/support-policy.md`, `contracts/public-identity.md` | 13 | 2 KB – 45 KB |
| Earlier drafts of `governance/` records — `phase-001-evidence.md`, `name-availability.md`, `waivers.md` | 3 | 1.8 KB – 5.4 KB |
| `Branding/` assets — 2 PDFs (6.97 MB, 121 KB) and 2 copy-kit text files | 4 | 1.4 KB – 6.97 MB |
| A `.specify` Python script, earlier revision | 1 | 25 KB |
| Superseded `.gitignore` variants | 2 | 100 B, 117 B |
| Ephemeral tooling counters (`trajectoriesRecorded` JSON) | 3 | 117 B each |

**Corrected conclusion.** Pruning **would** destroy 26 blobs held nowhere else on disk.
Every one is an earlier revision of a file whose current version still exists, or an
excluded/ephemeral artifact — so no *distinct* work is lost. That is a maintainer judgment
call, not a mechanical certainty, and it is the reason the §3d backup exists.

**Status**: superseded by §3e — the prune was authorised and executed on 2026-08-11 19:04.

## 3d. Recovery backup (created 2026-08-11 18:50, verified by restore)

Authorised by the maintainer for creation and restore-verification only.

| Field | Value |
|---|---|
| Path | `/Users/ahmedanbar/Documents/renvor-git-backup-2026-08-11.tar.gz` (outside the work tree) |
| Contents | The complete `.git` directory — objects, refs, reflogs, index, config |
| Size | 68,254,289 bytes (65.1 MiB) |
| SHA-256 | `7d98f5c6f40396ac838c8964a90586e14705ed17f5383de3c5be7e0b4494b9bf` |
| Permissions | `-rw-------` (owner read/write only) |
| Checksum file | `renvor-git-backup-2026-08-11.tar.gz.sha256`, same permissions |
| Tracked by Git | **No** — outside the repository, absent from `git status` |

A `git bundle` was rejected as insufficient: it captures only *reachable* objects, which is
precisely the set pruning does not touch. A full `.git` archive is the only form that
preserves the dropped stash snapshots.

### Restore verification

| Check | Result |
|---|---|
| Archive integrity listing (`tar -tzf`, all members read) | 630 entries, no errors |
| SHA-256 re-verified against the file on disk | `OK` |
| Extracted to a fresh `mktemp -d` directory | OK |
| `git fsck --full --strict` on the extracted `.git` | **exit 0**, zero error/missing/broken/corrupt lines |
| Reachable commits recovered | **2** — `bfb6925`, `1b182d0` |
| Unreachable blobs / commits / trees recovered | **152 / 56 / 100** |
| Stash-commit breakdown | **28 `WIP on` + 28 `index on`** = 56, zero non-stash |
| Object-set equivalence, source vs backup | **393 = 393**; zero missing in either direction |
| Source repository after verification | Unchanged — `HEAD bfb6925`, 2 commits, 0 remotes, 0 packs, 393 loose objects, working tree identical |

The restore test used a read-only method: `git hash-object` was invoked without `-w`, so no
verification step wrote an object into either repository.

## 3e. T012 — unreachable-object prune (executed 2026-08-11 19:04, maintainer-authorised)

Authorised after the maintainer confirmed the 27 object-store-only blobs expendable, with
ten mandatory gates. All ten passed; none were waived.

### Gates

| # | Gate | Result |
|---|---|---|
| 1 | Re-verify backup SHA-256 **before** anything | `OK` — matches `7d98f5c6…4b9bf` |
| 2 | Re-measure unreachable objects immediately before pruning | Done 19:04:23 — see table below |
| 3 | Abort on any non-stash unreachable commit, or a new category of object-store-only content | **PASS** — 58/58 commits were dropped stashes (29 `WIP on` + 29 `index on`, 0 other). Orphan blobs 26 → 27; the single addition (`fb18ec8aee11`, 9,914 B) is an earlier draft of this evidence pack — an existing category, not a new one |
| 4 | `.git/index.lock` absent, no Git operation running for this repository | **PASS** — six lock files absent; no `git` process targeting this repo |
| 5 | Preserve backup archive, checksum, and restore-test directory | **PASS** — all three intact before and after |
| 6 | Expire only unreachable reflog entries | `git reflog expire --expire-unreachable=now --all` → exit 0 |
| 7 | Garbage-collect with immediate prune; **`git prune` not run directly** | `git gc --prune=now` → exit 0 |
| 8 | Full strict fsck, both reachable commits, HEAD unchanged, status compared | **PASS** — see below |
| 9 | Re-verify backup checksum **after** pruning | `OK` — unchanged |
| 10 | Record counts and commands here, then mark T012 complete | This section |

### Commands executed, in order

```
git reflog expire --expire-unreachable=now --all
git gc --prune=now
```

No other object-store-mutating command was run. `git prune` was **not** invoked directly.

### Before and after

| Measure | Before (19:04:23) | After |
|---|---|---|
| Loose objects (`count`) | 398 | **0** |
| Objects in pack / packs | 0 / 0 | **85 / 1** |
| Pack size | — | 213 KiB |
| Unreachable blobs | 153 | **0** |
| Unreachable commits | 58 | **0** |
| Unreachable trees | 102 | **0** |
| Reflog entries | 4 (+ unreachable stash entries) | 4 |
| `.git` on disk | **66 MiB** | **332 KiB** |

The count moved between the 18:50 backup (393/152/56/100) and the 19:04 prune
(398/153/58/102) because session tooling kept taking and dropping stash snapshots
throughout — the behaviour documented in §3c-rev. Gate 2 exists precisely for this.

### Post-prune verification

| Check | Result |
|---|---|
| `git fsck --full --strict` | **exit 0** — no errors, no dangling objects |
| Reachable commits | **2** — `bfb6925 docs: establish project governance`, `1b182d0 Initialization` |
| HEAD | `bfb692582dbe61bea1458fe92eee7b9dad12dfd1` — **unchanged** |
| `git status --porcelain` vs pre-prune snapshot | **byte-identical** — working tree untouched |
| Committed history | **not rewritten** — both commit IDs unchanged |
| Backup checksum after prune | `OK` |

**What was destroyed**: 58 dropped stash commits, 102 orphan trees, and 153 unreachable
blobs — of which 126 were byte-identical to files still on disk and 27 existed only in the
object store. All 27 are preserved in the §3d archive.

## 3f. T014 — default branch rename (2026-08-11)

```
git branch -m master main
```

Performed while **zero remotes** are configured, so no remote default-branch update, no
upstream re-pointing, and no open pull requests are affected — the rename is free of the
usual coordination cost. `HEAD` unchanged at `bfb6925`; 2 commits; working tree unaffected;
`master` no longer resolves.

## 3g. T024 — public organization and repository (verified read-only 2026-08-11T16:39Z)

Created manually by the maintainer. Verified here **read-only**; nothing was created,
configured, or initialised by this verification.

| Field | Observed | Source |
|---|---|---|
| Organization | `renvor-rs`, id **315882691**, type **Organization** | `GET /orgs/renvor-rs` → 200 |
| Organization created | **2026-08-11T16:31:16Z** | same |
| Repository | `renvor-rs/renvor`, id **1331135435** | `GET /repos/renvor-rs/renvor` → 200 |
| Repository created | **2026-08-11T16:32:20Z** | same |
| Visibility | **public** (`private: false`) | same |
| Default branch | **`main`** — matches the local branch renamed at T014 | same |
| Fork / archived | `false` / `false` | same |
| Size | **0 KB** | same |
| Branches | **0** | `GET /repos/renvor-rs/renvor/branches` → 200, empty array |
| Commits | **HTTP 409 — "Git Repository is empty."** | `GET /repos/renvor-rs/renvor/commits` |

**Repository confirmed genuinely empty**: size 0, zero branches, and the commits endpoint
returning 409 rather than an empty list. No README, licence, or `.gitignore` was
initialised through GitHub, so the first push will not conflict with a divergent history.

**Not independently verified — attested only.** `GET /orgs/renvor-rs/public_members`
returns an empty array, and organization **admin role is not publicly readable without
authentication**. The account `AhmedAnbar` exists (`GET /users/AhmedAnbar` → 200), but that
it holds owner/admin on `renvor-rs` is a **maintainer attestation**, not a read-only
observation. Confirm via an authenticated check before relying on it for release control.

**No local remote was configured.** `git remote -v` remains empty by instruction; the local
repository is still unlinked from this GitHub repository.

## 3h. T025 — ownership and responsibility register (2026-08-11)

Responsibility assignments. **These name who is accountable; they do not assert that any
external account, credential, or artifact exists.**

| Role | Holder | Scope | Verified? |
|---|---|---|---|
| Maintainer | **Ahmed Anbar** | Decision authority for the project, per `GOVERNANCE.md` (T048) | Attested |
| Security contact | **admin@ahmedanbar.dev** | Monitored address for private vulnerability reports, published in `SECURITY.md` (T045) | Deliverability test pending — **T052** |
| Release approver | **Ahmed Anbar** | Named approver for the protected release environment (T072) | Attested; environment not yet configured |
| Registry bootstrap owner | **Ahmed Anbar** | Accountable for the first manual crates.io publication and for the least-scope token lifecycle in `contracts/package-metadata.md` | Attested |

**Explicit non-claims about the registry.** The registry-bootstrap row assigns
responsibility only. As of 2026-08-11 this project asserts **none** of the following, and
none has been verified:

- that a crates.io account exists or is linked to this project;
- that any crate named `renvor`, `renvor-cli`, or `renover` has been published or reserved
  — verified false at T016/T017, all HTTP 404;
- that any registry API token has been created;
- that trusted publishing is configured — it **cannot** be, because no crate exists yet
  (research Finding 2).

Any later claim to the contrary requires its own dated verification in this pack.

## 3i. T037 — single MSRV declaration site (asserted 2026-08-11)

| Check | Result |
|---|---|
| Literal `rust-version = "…"` declarations across all manifests | **1** — `Cargo.toml:21`, `[workspace.package]` |
| Members inheriting via `rust-version.workspace = true` | 2 — `crates/renvor`, `xtask` |
| Literal `edition = "…"` declarations | **1** — `Cargo.toml:20` |
| `cargo metadata` resolved value, both packages | `rust_version=1.94.0`, `edition=2024` |

**PASS.** No second independent declaration exists, so the two cannot drift.

## 3j. T038 — MSRV-aware resolution is IN EFFECT (asserted 2026-08-11)

Configuration alone proves nothing, so the mechanism was exercised. This workspace has
zero dependencies, which makes in-place demonstration impossible; the test therefore ran
in a throwaway crate outside the repository, using the same cargo binary.

**Direct signal.** Cargo announces the behaviour:

```
Locking 1 package to latest Rust 1.60.0 compatible version
```

**Differential proof.** Identical cargo, identical dependency spec `clap = ">=3"`, only
`rust-version` changed:

| `rust-version` | clap resolved to | Cargo message |
|---|---|---|
| `1.63.0` | **4.0.32** | `Locking 21 packages to latest Rust 1.63.0 compatible versions` |
| `1.94.0` | **4.6.6** | `Locking 15 packages to latest Rust 1.94.0 compatible versions` |

clap 4.6.6 declares `rust-version 1.85`. Under a 1.63 floor the resolver **downgraded**
rather than selecting the newest release. **PASS — resolution is MSRV-aware in practice,
not merely declared.** `cargo 1.94.0 (85eff7c80 2026-01-15)`.

An initial attempt using `bitflags` produced no differential and was discarded as
inconclusive: bitflags 2.13.1 declares `rust-version 1.56.0`, so it was compatible with
both floors and no downgrade was required. Recorded so the negative result is not mistaken
for evidence of absence.

## 3k. T041 / T042 / T043 — verification runs (2026-08-11)

Operator: Ahmed Anbar. Platform: `darwin/aarch64` (macOS). Entry point: `cargo xtask verify`.

### T041 — both toolchains

| Run | Toolchain | Started (UTC) | Steps 1–7 | Step 8 | Exit |
|---|---|---|---|---|---|
| 1 | **1.94.0** (MSRV, `rust-toolchain.toml`) — `rustc 1.94.0 (4a4ef493e 2026-03-02)` | 16:49:25 | **all passed** | failed | **1** |
| 2 | **1.97.1** (current stable) — `rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1` | 16:49:52 | **all passed** | failed | **1** |

Steps that ran and passed on both: toolchain probe, `cargo fmt --all --check`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`cargo test --workspace --all-features` (3 unit + 3 doc tests),
`cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-D warnings`, `cargo deny check`
(`advisories ok, bans ok, licenses ok, sources ok`), `gitleaks git .`, `gitleaks dir .`.

**⚠️ Neither run reached exit 0, and the cause is a task-ordering defect, not a code fault.**
Step 8 builds the documentation site, which requires `docs/package.json`. That file is
created by **T064** — a task numbered *after* the T054 first-push gate. T041 as written
cannot pass until T064 lands. The runner correctly refuses to treat the missing site as a
skip:

> `[8/10] documentation site: FAILED — docs/package.json not found … This is a FAILURE,
> not a skip. The sequence has no conditional steps: a check that cannot run is a failure
> (FR-023). Steps 1-7 above did run and did pass; steps 8-10 did not run.`

**T041 must be re-run after T064** for the phase to close. Recorded as an open blocker.

**⚠️ The stable run used an explicitly pinned 1.97.1, not the `stable` channel**, because
the local `stable` channel is stale at 1.94.0 and `rustup update stable` fails with a
component conflict (os error 66). Using `stable` would have exercised the MSRV compiler
twice and proven nothing. The pinned toolchain is the same version the `stable` channel
resolves to, verified above. The channel problem remains open for separate diagnosis; no
`rustup --force` was used.

### T042 — fail-closed probe

`PATH` restricted to `cargo`, `rustc`, `rustup`, `git`, `/usr/bin`, `/bin`.

```
error: verification cannot run — required tooling is missing
  missing: gitleaks (secret scan, step 7)   / missing: node / missing: npm
no checks were run. verification did not pass.
```

**Exit code 2 — PASS.** The mandated "no checks were run" line is present and no step ran.

**Test limitation, recorded honestly:** `cargo-deny` and `lychee` could **not** be hidden
this way. Both live in `$CARGO_HOME/bin`, which cargo injects into the environment of the
processes it spawns regardless of the caller's `PATH`. The probe was therefore exercised
against 3 of 8 tools. That the fail-closed path works is established; that it works for
those specific two is not, and would need a container or a moved `CARGO_HOME` to prove.

### T043 — working-tree cleanliness

After the verification runs produced **862 files / 24 MB** under `target/`:

| Path | Ignored |
|---|---|
| `target/`, `target/doc`, `target/debug` | yes |
| `Cargo.lock` | **no — tracked deliberately**, per the lockfile rule for workspaces with binaries |

`git status --porcelain` shows **zero build artefacts**. Every entry is an intentional
source or governance file awaiting the initial commit. **PASS — the T010 ignore rules are
correct, not merely present.**

The contract's strict form of step 10 (`git status --porcelain` empty) additionally
requires the initial commit, which is deliberately deferred to the T054 approval. What is
proven here is the substantive control: **a full verification run introduces no untracked
or modified files.**

## 3l. T044 — licence texts

| File | Bytes | Source |
|---|---|---|
| `LICENSE-APACHE` | 11,358 | Fetched from the canonical source `https://www.apache.org/licenses/LICENSE-2.0.txt` (HTTP 200), sha256 `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`, contains `END OF TERMS AND CONDITIONS` |
| `LICENSE-MIT` | 1,096 | Standard MIT text, © 2026 Ahmed Anbar and the Renvor contributors |

Byte-identical copies placed at `crates/renvor/` so the published crate carries both.
Verified identical by sha256.

## 3m. T030 / T044 — shipped file set (`cargo package -p renvor --list`)

```
.cargo_vcs_info.json   Cargo.lock   Cargo.toml   Cargo.toml.orig
LICENSE-APACHE   LICENSE-MIT   README.md   src/lib.rs
```

**Eight files. No secret, no local configuration, no build output, no unintended asset**
(FR-039). The set is stated by the explicit `include` in `crates/renvor/Cargo.toml`, never
inferred. Run with `--allow-dirty`; **zero publish operations were performed.**

## 3n. T053 — pre-push re-scan gate (2026-08-11T16:55:42Z)

Re-run after roughly twenty file-creating tasks, so it describes the state actually
proposed for publication rather than the T013 state.

| Scan | Tool | Scope | Findings |
|---|---|---|---|
| history | gitleaks 8.30.1 `git .` | 2 commits, 311.63 KB | **0** |
| working tree | gitleaks 8.30.1 `dir .` | 5.53 MB of text (up from 4.80 MB at T013) | **0** |

**Canary re-test — has the FP-001 allowlist silently widened?**

| Line | Content | Expected | Observed |
|---|---|---|---|
| 679 | allowlisted prose | suppressed | suppressed ✅ |
| 682 | injected canary credential | detected | **detected** ✅ |

**PASS.** The allowlist still suppresses exactly one prose match and nothing else. Test
file restored byte-identically (hash compared).

## 3o. Unpublished-history remediation (2026-08-11, local only — **nothing pushed**)

Clears the §3b blocker. Authorised as a local-only pass.

### Backup taken first

| Field | Value |
|---|---|
| Path | `/Users/ahmedanbar/Documents/renvor-pre-rewrite-backup-2026-08-11.tar.gz` |
| Size / permissions | 709,415 bytes · `-rw-------` |
| SHA-256 | `b13f370b5b3639b68b625e3036d4b05a7c456fc1ba3b840925361276f6a3067c` |
| Contents | Complete `.git` (index included), all tracked and staged changes, all untracked Phase 001 files, `.claude/` (12 files), `.specify/` (20 files). Excludes `target/` (regenerable) and `Branding/` (1.4 GB, untouched by this pass) |
| Restore test | Extracted to a fresh `mktemp -d`; `git fsck --full --strict` **clean**; 2 commits, 149 objects; staged `.gitignore` blob `6b30b390…` present in the restored index; **all 116 files byte-identical** |

### The rewrite

```
git filter-repo --partial --invert-paths --path .claude --path .specify \
  --name-callback  'return b"Ahmed Anbar"' \
  --email-callback 'return b"admin@ahmedanbar.dev"' \
  --force
```

`--partial` was chosen deliberately: filter-repo otherwise runs
`git reflog expire --expire=now --all` and `git gc --prune=now` automatically, and both
were prohibited for this pass. `--partial` disables both. **No `--force` flag was used on
any rustup or git-gc operation**; the `--force` here is filter-repo's "not a fresh clone"
acknowledgement.

### Commit `1b182d0` was removed entirely, not rewritten

`1b182d0 "Initialization"` contained **29 files, all of them `.claude/` or `.specify/`** —
nothing else. Stripping the excluded paths emptied it completely, so it was pruned rather
than preserved as an empty commit. History went from 2 commits to 1
(`bfb6925` → `1b772d2`, retaining `PLAN.md`).

### Collateral damage from filter-repo's hard reset — detected and repaired

Removing the paths from history caused the post-filter reset to delete them from **disk**,
and to discard uncommitted tracked changes:

| Item | After rewrite | Restored from backup |
|---|---|---|
| `.claude/` | 12 files → **2** | ✅ 12 files |
| `.specify/` | 20 files → **1** | ✅ 20 files |
| `.gitignore` | **deleted** | ✅ restored |
| `PLAN.md` | reverted to the committed version | ✅ worktree version restored |

**Post-restore verification: all 98 project files byte-identical to the pre-rewrite
state.** The only four differing files were live session-tooling scratch
(`.remember/tmp/*`, `.remember/logs/*`, `.claude-flow/neural/stats.json`), which mutate
continuously and are unrelated to the rewrite.

### Verification

| Check | Result |
|---|---|
| `git ls-files .claude .specify` | **empty** ✅ |
| Excluded paths in any reachable commit | **0** ✅ |
| `.claude/` and `.specify/` on disk and ignored | 12 and 20 files, both `ignored=yes` ✅ |
| Every commit author **and** committer | `Ahmed Anbar <admin@ahmedanbar.dev>` — 0 deviations ✅ |
| Remotes / anything pushed | 0 / nothing ✅ |
| Both backups after the rewrite | checksums `OK` ✅ |
| Reflogs expired, gc, prune | **none run** — 23 reflog entries retained ✅ |

Identity normalisation was substantive, not cosmetic: `1b182d0` carried
`Ahmed anbar <begnulinux@gmail.com>` (lower-case surname, personal address).

### ⚠️ Residual — one ref still exposes the excluded content

```
refs/codex/turn-diffs/checkpoints/…/99099561-cbf1-4907-abbd-27ed9c29dcf3
  -> tree b4c29fd5e63c5990459f1bdff1ca741f390ab470  (29 excluded paths)
```

A session-tooling checkpoint ref pointing at a **tree**, not a commit. It therefore does
not violate "no reachable commit contains the excluded paths", and `git push` does not
transmit `refs/codex/*`. **But `git push --mirror` would publish it.** Left in place
pending a maintainer decision; deleting it may break that tool's checkpoint history.

**Proposed remediation** (not executed): `git update-ref -d '<the ref>'`, which is a ref
deletion, not a prune, gc, or reflog expiry.

## 3p. Local Rust `stable` channel — diagnosed and repaired (2026-08-11)

### Diagnosis

The `stable` toolchain directory was **1.3 GB**, against 520 MB for `1.94.0` and 563 MB for
`1.97.1`. Its component ledger had desynchronised from the filesystem:

| `lib/rustlib/components` lists | Manifests and files also present for |
|---|---|
| `rust-src`, `cargo`, `clippy-preview`, `rustc`, `rustfmt-preview` | **`rust-std-aarch64-apple-darwin`**, **`rust-docs-aarch64-apple-darwin`** |

rustup therefore believed `rust-std` and `rust-docs` were absent, tried to install them,
and found their files already on disk — `detected conflict: 'share/doc/rust/html'`.
Separately, stale non-empty directories under `rust-src` blocked rustup's rename-based
file swaps — `Directory not empty (os error 66)`. An interrupted rustup run left it
inconsistent; `rustup update stable` could never converge.

### Repair — scoped to the named stale installation only

```
rustup toolchain uninstall stable
rustup toolchain install stable --component rustfmt --component clippy --component rust-src
```

**No `--force` was used.** Only `~/.rustup/toolchains/stable-aarch64-apple-darwin` was
removed. `1.94.0` and `1.97.1` live in separate directories and were untouched — `1.97.1`'s
directory mtime remains 2026-08-03, predating this session.

### Proof

| Check | Result |
|---|---|
| `rustc +stable --version` | **`rustc 1.97.1 (8bab26f4f 2026-07-14)`** |
| `cargo +stable --version` | `cargo 1.97.1 (c980f4866 2026-06-30)` |
| Differs from the MSRV | **stable 1.97.1 ≠ MSRV 1.94.0** ✅ |
| `rustc +1.94.0 --version` | `rustc 1.94.0 (4a4ef493e 2026-03-02)` — unchanged |
| Upstream agreement | rustup: "latest update on 2026-07-16 for version 1.97.1" |

**The two verification jobs now exercise genuinely different compilers.** Before the
repair, `stable` resolved to 1.94.0 and the "stable" job would have duplicated the MSRV job
while appearing to pass.

## 3q. T064–T069 — documentation package, and the corrected task ordering

### The ordering defect, corrected without weakening the check

Verification step 8 builds `docs/`. As originally numbered, **T041** (run verification) sat
in Phase 4 while **T064** (scaffold the documentation site) sat in Phase 7, behind the
**T054** first-push gate — so T041 could never reach exit 0.

**The fix was to move the work, not to weaken the gate.** The documentation check was not
made optional, not made conditional, and not skipped; `tasks.md` now records that T064,
T065, and T067 precede T041.

### What was built

| Task | Artifact |
|---|---|
| T064 | Docusaurus **3.10.2** in `docs/`, `@easyops-cn/docusaurus-search-local` (local index — `build/search-index.json` ships with the site, no hosted service, FR-054), `docs/package-lock.json` committed (1,320 packages) |
| T065 | `.nvmrc` pinning the Node **22** LTS line |
| T066 | `decisions/0004-documentation-platform-and-versioning.md` — mdBook, MkDocs+Material, Zola, and Algolia DocSearch recorded as rejected with reasons; Node cost and 1,320-package surface disclosed. State `proposed` |
| T067 | Five pages: intro, support-policy, verification, governance, api-reference. Scaffold tutorial content, blog, and unused Docusaurus-branded assets removed |
| T068 | lychee wired into `xtask` step 9 with `lychee.toml` |
| T069 | Shared version-stamp partial `docs/docs/_stamp.mdx` imported by **every** prose page and the API reference — one value, so prose and API cannot drift (FR-056) |

### Link checking required real configuration, twice

A naive `lychee docs/build` reported **142 errors**, none of them broken links:

1. **Root-relative links** (`/docs/intro`) cannot be resolved against the filesystem
   without `--root-dir`. Without it every internal link reads as broken — 142 false
   failures that would train a reader to ignore the step. `--root-dir` is now mandatory in
   step 9.
2. **Three residual errors** were lychee parsing inline `url("data:image/svg+xml;…")`
   values inside Docusaurus's bundled CSS as file paths — a parser false positive on
   generated output containing no authored hyperlinks.

`lychee.toml` records three exclusions, each with an explicit removal condition: EX-001
`renvor.dev` (site not deployed), EX-002 `docs.rs/renvor` (crate not published), EX-003 the
generated CSS bundle. **Final result: 225 OK, 0 errors, 32 excluded.**

## 3r. T041 / T043 — full verification, both toolchains, exit 0

Run against the exact committed state (`ddfc39d`, 8 commits, clean worktree).
Operator: Ahmed Anbar. Platform `darwin/aarch64`.

| Run | Toolchain | Compiler | Started (UTC) | Steps | **Exit** |
|---|---|---|---|---|---|
| 1 | MSRV `1.94.0` | `rustc 1.94.0 (4a4ef493e 2026-03-02)` | 17:27:32 | **all 10 passed** | **0** ✅ |
| 2 | `stable` (repaired) | `rustc 1.97.1 (8bab26f4f 2026-07-14)` | 17:28:06 | **all 10 passed** | **0** ✅ |

```
verification passed: all 10 steps ran and passed.
```

**T043 is satisfied in its literal form**: step 10 reports `no untracked or modified files`.
`git status --porcelain` is empty after a full run, proving the ignore rules are correct
rather than merely present.

## 3s. T053 — final re-scan over the exact proposed commits

`2026-08-11T17:28:52Z`, against `HEAD ddfc39d`, 8 commits, clean worktree.

| Scan | Scope | Findings |
|---|---|---|
| `gitleaks git .` | **all 8 commits**, 1.29 MB | **0** |
| `gitleaks dir .` | working tree, 6.57 MB of text | **0** |

Canary re-test: line 679 prose **suppressed**, line 682 injected credential **detected** —
allowlist still narrow. Test file restored byte-identically; worktree still clean.

## 3t. T052 — security-channel delivery test (2026-08-11)

| Field | Value |
|---|---|
| Channel tested | The private reporting path published in `SECURITY.md` — email to **admin@ahmedanbar.dev** |
| Test message | Subject `[Renvor security channel test] T052`; body stated it was an authorised delivery test containing no vulnerability information or sensitive data |
| Sent from | A separate mailbox controlled by the maintainer |
| Delivery confirmed | **2026-08-11, approximately 20:57 Asia/Riyadh** |
| Evidence type | **Maintainer/operator attestation** that a test message sent through the documented private contact path arrived at the monitored inbox |
| Attested by | Ahmed Anbar |

Neither the message contents nor any screenshot is committed to this repository.

### What this evidence does and does not establish

**Establishes:** a message addressed to the published security contact reaches a mailbox
the maintainer monitors. The reporting path in `SECURITY.md` is not a dead address.

**Does NOT establish** — none of the following was tested, and none is claimed:

- SPF, DKIM, or DMARC alignment for the sending or receiving domain;
- sender-identity verification or anti-spoofing posture;
- inbound spam-filter behaviour for mail from unknown third-party senders;
- deliverability from senders outside the maintainer's own control;
- whether a report from an arbitrary external reporter would avoid a junk folder;
- acknowledgement-time performance against the 72-hour commitment in `SECURITY.md`.

A single self-originated delivery is the weakest useful form of this test: it proves the
address exists and is monitored. Testing whether an *unknown external reporter* reaches the
inbox requires a separate test from an unrelated domain, and is not claimed here.

## 3u. Production server audit — read-only (2026-08-11)

Performed over `ssh hostinger`. **Read-only discovery only.** Nothing was installed,
started, stopped, restarted, configured, uploaded, or deployed. No firewall, DNS, or
Cloudflare change was made. Sensitive values are redacted below; no private key, token,
environment variable, credential file, kubeconfig content, or application secret was
displayed or retrieved.

### Host

| Item | Observed |
|---|---|
| Operating system | **Ubuntu 26.04 LTS** ("Resolute Raccoon"), kernel 7.0.0-22-generic |
| Lifecycle | LTS release; standard support window applies. Not verified against a published EOL date — treat as an open item before production reliance |
| Architecture | x86_64, KVM full virtualisation |
| CPU | **8 vCPU**, AMD EPYC 9354P (1 thread/core) |
| Memory | **31 GiB** total; ~13 GiB used, **~17 GiB available** |
| Swap | **0 B** — no swap configured (`vm.swappiness=60`, immaterial without swap) |
| Disk | 400 GB device; `/` = 387 GB, **328 GB free (16 % used)** |
| Uptime | 64 days |
| Automatic updates | `unattended-upgrades.service` running; no explicit periodic/auto-reboot config found |

### Container and orchestration

| Item | Observed |
|---|---|
| Kubernetes | **k3s v1.35.5+k3s1 already installed and running** (Go 1.25.9), single node, `control-plane` role, `Ready` |
| Datastore | **SQLite** — `/var/lib/rancher/k3s/server/db/state.db`; not etcd |
| Container runtime | containerd v2.2.4 (via k3s), runc 1.3.5, crictl v1.35.0-k3s2 |
| Docker | **Also installed and running** — Docker 29.5.3, separate from k3s |
| Tooling present | `kubectl`, `helm`, `k3s`, `crictl` |
| Not present | kubeadm, kubelet (standalone), k0s, MicroK8s, RKE2, minikube, podman, nerdctl |

### Existing workloads — the server is NOT empty

| Namespace | Workloads |
|---|---|
| `attaa` | api, clamav, minio, postgres, redis, web, worker (7 deployments) |
| `codexhub` | api-gateway, auth-service, billing-service, frontend, mailpit, postgres, redis, dev-tools, task-manager, team-service, user-service (11 deployments) |
| `portfolio` | portfolio (2 replicas) |
| `gitlab` | services bridging to a Docker-hosted GitLab CE |
| `cert-manager` | cert-manager, cainjector, webhook |
| `kube-system` | coredns, local-path-provisioner, metrics-server, **traefik**, svclb DaemonSet |

Outside Kubernetes, Docker runs **GitLab CE** (`gitlab/gitlab-ce:latest`, healthy, 3 weeks)
and a BuildKit builder. A `gitlab-runner.service` is also active.

**28 pods running of 110 capacity. Node utilisation: 285 m CPU (3 %), 13,767 MiB (42 %).**

### Ingress, TLS, and ports

| Item | Observed |
|---|---|
| Ingress controller | **Traefik 3.6.13**, Helm chart `traefik-39.0.701+up39.0.7`, IngressClass `traefik` |
| Port binding | `svclb-traefik` DaemonSet with **hostPorts 80 and 443** (klipper-lb). No userspace listener appears in `ss` because klipper-lb uses iptables DNAT |
| Ports 80/443 | **Occupied** — bound by the svclb DaemonSet and serving existing sites |
| Host listeners | 22 (via docker-proxy), 6443 (k3s API) |
| TLS | **cert-manager v1.20.2** with ClusterIssuer `letsencrypt-prod` = `True`; **6 certificates, all `True`** across attaa, codexhub, gitlab, portfolio |
| Live ingress hosts | `ahmedanbar.dev`, `codexhub.ahmedanbar.dev` |

### Network and firewall

| Item | Observed |
|---|---|
| IPv4 | `153.92.208.x` on `eth0` (public) |
| IPv6 | `2a02:4780:f:88ec::/48` (public, available) |
| Firewall | **`ufw` inactive.** ~520 iptables rules and 7 nft tables present, essentially all Kubernetes/Docker-managed |
| Cloudflare software | **`cloudflared` not installed**; no tunnel or Cloudflare pods in the cluster |
| DNS (external check) | `renvor.dev` and `ahmedanbar.dev` both on Cloudflare nameservers (`coco`/`earl.ns.cloudflare.com`). `renvor.dev` has **no A record**. `ahmedanbar.dev` resolves **directly to the origin IP**, i.e. currently DNS-only, not proxied |

### Backup and recovery

| Item | Observed |
|---|---|
| k3s snapshots | **None** — SQLite backend, so the `etcd-snapshot` mechanism does not apply |
| Backup tooling | **restic, borg, duplicity all absent** |
| Provider snapshots | Not verifiable from inside the VM; must be confirmed in the Hostinger control panel |

**⚠️ Finding:** all cluster state for five production namespaces lives in a single SQLite
file with no observed snapshot schedule and no backup tooling installed. This is a
pre-existing risk affecting workloads unrelated to Renvor. It is recorded, not fixed, and is
outside Renvor's remit — but it should not go unstated.

### Constraints that shaped ADR-0006

1. **Kubernetes is already installed and serving production traffic.** The task is adding
   two namespaces, not installing a distribution.
2. **Ingress and ACME are proven on this exact host** — six valid certificates.
3. **Ports 80/443 are already owned** by the existing ingress path.
4. **Zero swap** — memory limits are mandatory; overcommit produces OOM kills that could
   evict a neighbouring production pod.
5. **`ufw` is inactive** and the origin IP is already public in DNS for another domain, so
   origin-bypass mitigation must work without a host firewall change.
6. **Ample headroom** — 3 % CPU, 42 % memory, 28/110 pods — two static sites are negligible.

## 4. Acceptance criteria coverage

Populated by T082. One row per PLAN.md Phase 001 acceptance criterion and per SC-001
through SC-016.

| Criterion | Evidence link | Command or action | Platform | Operator | Date | Result |
|---|---|---|---|---|---|---|
| *(not yet populated)* | | | | | | |

## 5. Secret scans

Two scans are required and both must report zero findings (SC-005). A single clean scan
does not satisfy the criterion — the earlier scan predates the content the later one
authorises.

| Scan | Purpose | Tool | Version | Date | Scope | Findings |
|---|---|---|---|---|---|---|
| pre-creation (a) | Gates organization and repository creation | gitleaks `git` | 8.30.1 | 2026-08-11 19:11 | Full history — 2 commits (`1b182d0`..`bfb6925`), 311.63 KB | **0** |
| pre-creation (b) | Gates organization and repository creation | gitleaks `dir` | 8.30.1 | 2026-08-11 19:11 | Working tree — 4.80 MB of text across 122,660 files | **0** |
| pre-push | Gates the first content push | | | | | *(not run — T053)* |

### T013 command note — `gitleaks detect` no longer exists

The task originally specified `gitleaks detect`. That command was **removed in Gitleaks
8.x**; version 8.30.1 exposes only `git`, `dir`, and `stdin`. Running the obsolete command
would have failed, or worse, been quietly replaced by a weaker substitute. The task wording
has been corrected. Both scans are required and neither substitutes for the other:

```
gitleaks git . --report-format json --report-path <report> --redact --no-banner
gitleaks dir . --report-format json --report-path <report> --redact --no-banner
```

### T013 scope note — what `dir` actually covers

`gitleaks dir` does **not** honour `.gitignore`; it scanned ignored paths including
`RENVOR_MASTER_IMPLEMENTATION_PLAN.md`. The 4.80 MB figure is *text* volume: gitleaks skips
binary files, so the 1.4 GB of `Branding/` PDFs contributed almost nothing. Do not read
"4.80 MB scanned" as "only tracked files scanned" — the two differ.

### T013 false positive FP-001 — resolved by narrow allowlist

| Field | Value |
|---|---|
| Rule | `generic-api-key` |
| Location | `RENVOR_MASTER_IMPLEMENTATION_PLAN.md` line 679 |
| Fingerprint | `RENVOR_MASTER_IMPLEMENTATION_PLAN.md:generic-api-key:679` |
| Entropy | 3.73 |
| Actual content | English prose listing documentation deliverables: *"… hardening checklist, secrets, CORS/proxies/rate limits and ecommerce …"*. The rule keys off the word "secrets" followed by the next token. No credential present. |
| Exposure | None — the file is `.gitignore`d (line 68), untracked, appears in **0 commits**, and T009 excludes it from the publish set as superseded |
| Resolution | `.gitleaks.toml` allowlist FP-001, scoped by `regexes` + `regexTarget = "match"` on the literal prose |

**A first attempt at this allowlist was defective and was caught by testing.** Scoping it
with `paths` made gitleaks report `scanned ~0 bytes` for that file: a `paths` allowlist
excludes the file *before* scanning, so it is a blanket file exclusion — precisely what
T013 forbids — and an injected canary secret went undetected. The entry was rewritten to
match on content only.

**Canary verification** (re-run required at T053, and after any edit to that file): a
synthetic credential injected three lines below the allowlisted match is still detected,
while the prose match stays suppressed.

| Line | Content | Expected | Observed |
|---|---|---|---|
| 679 | allowlisted prose | suppressed | suppressed ✅ |
| 682 | injected canary credential | **detected** | detected ✅ |

The file was restored byte-identically after the test (hash compared).

## 6. Known limitations

Populated by T083. Each requires a named owner and a target phase.

Seeded 2026-08-11 from the T016–T022 name verification. T083 consolidates and finalises.

| ID | Limitation | Owner | Target phase |
|---|---|---|---|
| R-1 | `renvor` and `renvor-cli` are verified but **unreserved**; a third party may claim either before first publication. Deliberate — FR-049 forbids publishing placeholder crates | Ahmed Anbar | The phase performing the first crates.io publication |
| R-2 | **Confusability with `renovate`** — the crates.io `renovate` crate and the widely used npm `renovate` bot sit 1–2 characters from `renover` | Ahmed Anbar | ADR-0001 (T026) |
| R-3 | `renover` clearance is **bounded, not exhaustive** — no global executable-name registry exists; non-Debian distributions, BSD ports, Windows package managers, and privately distributed binaries were not checked | Ahmed Anbar | Ongoing |
| R-4 | **No trademark or common-law search** was performed for the product name `Renvor`; `contracts/public-identity.md` names one as that row's verification method | Ahmed Anbar | Before first public announcement |
| R-5 | `renvor.dev` is in its registry **Add Grace Period** and expires **2027-08-11**; renewal is an operational obligation | Ahmed Anbar | T084 recurring obligations |
| — | Local `stable` toolchain stale at 1.94.0 (see §8) | Ahmed Anbar | Separate diagnosis |

## 7. Recurring obligations

Populated by T084.

| Obligation | Owner | First due |
|---|---|---|
| *(not yet populated)* | | |

## 8. Open blockers

The phase remains open while any row here is present.

| Blocker | Blocks | Owner | Raised | State |
|---|---|---|---|---|
| ~~Excluded material in committed `HEAD`~~ | ~~T054~~ | Maintainer | 2026-08-11 | **resolved** — history rewritten, §3o. One residual `refs/codex/*` tree ref proposed for deletion |
| ~~T041 cannot reach exit 0~~ | ~~Phase closure~~ | Maintainer | 2026-08-11 | **resolved** — task order corrected, both toolchains exit 0, §3q/§3r |
| ~~T052 delivery test not confirmed~~ | ~~T052~~ | Maintainer | 2026-08-11 | **resolved** — maintainer attestation of arrival, §3t. Note: only delivery was tested; SPF/DKIM/DMARC and external-sender deliverability are **not** claimed |
| ~~T043 literal form unmet~~ | ~~T043~~ | Maintainer | 2026-08-11 | **resolved** — step 10 reports a clean tree, §3r |
| **All Phase 001 decision records remain `proposed`**: W-002 compensating control 3 ("all required CI and security checks passing") cannot be met until T057–T059 create the workflows and they run. ADR-0001, ADR-0002, ADR-0003 are written and reviewed but not accepted | T026, T039, T040 acceptance | Maintainer | 2026-08-11 | **open** |
| Organization **admin role for `AhmedAnbar` on `renvor-rs` is attested, not verified** — not publicly readable unauthenticated | Release-control assurance | Maintainer | 2026-08-11 | open |
| ~~T012 prune not authorised~~ | ~~T013, T014, T015~~ | Maintainer | 2026-08-11 | resolved — §3e |
| ~~`refs/codex/*` ref exposing excluded paths~~ | ~~Mirror-push safety~~ | Maintainer | 2026-08-11 | **resolved** — the exposing ref is gone; a later benign ref (0 excluded paths, 0 unique blobs) was deleted by exact name 2026-08-12. **Recurring**: session tooling recreates these refs, so re-check before any push |
| **V7 landing page fails the release-honesty gate** — present-tense claims for unbuilt capabilities, `renover` commands for an unpublished crate, zero development-status disclosure, and three dead CTA targets | T095–T097; any public landing deployment | Maintainer | 2026-08-11 | **open — blocks deployment** |
| **Website-code licence and brand-asset usage terms undecided** — brand assets are not covered by `MIT OR Apache-2.0` | T098; creation of `renvor-rs/renvor-landing` | Maintainer | 2026-08-11 | **open** |
| **Container registry undecided** — GHCR versus the VPS GitLab registry, including credential model | T099; creation of the private repositories | Maintainer | 2026-08-11 | **open** |
| **No backup tooling or cluster snapshots on the production VPS** — all state for five production namespaces in one SQLite file. Pre-existing, affects unrelated workloads, outside Renvor's remit | Server reliability | Maintainer | 2026-08-11 | **open — recorded, not owned** |
| ~~Local `stable` toolchain stale at 1.94.0~~ | ~~two-toolchain verification~~ | Maintainer | 2026-08-11 | **resolved** — diagnosed and repaired, §3p. `rustc +stable` is now 1.97.1 |
| ~~T006 independent-reviewer ruling~~ | ~~T026, T039, T040, T066~~ | Maintainer | 2026-08-11 | resolved — W-002, `governance/waivers.md` |
| ~~T008 candidate names~~ | ~~T019, T020, T021~~ | Maintainer | 2026-08-11 | resolved — `governance/name-availability.md` |
| ~~T009 publish-set decisions~~ | ~~T010 and the cleanup chain~~ | Maintainer | 2026-08-11 | resolved — §3a |
