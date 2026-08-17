# Phase 001 Evidence Pack

> **Record-count note added 2026-08-17.** Statements below that there are **six** Phase 001
> decision records, or that they are **ADR-0001 through ADR-0006**, describe the set as it
> stood on their own dates. There are now **seven**: **ADR-0010** was accepted under W-002 on
> 2026-08-17, and **ADR-0001 is now `superseded`** by it. Both facts are recorded at
> [§ADR-0010](#adr-0010--the-executable-name-unified-with-the-product-2026-08-17) at the foot
> of this file. The SC-009 row and the §3ay result are **true as of their dates** and are not
> rewritten; they are extended there.
>
> **Naming note added 2026-08-17.** Occurrences of **`renover`** below are **historical**.
> ADR-0010 superseded ADR-0001 on 2026-08-17 and renamed the installed executable to
> **`renvor`**; the primary command is `renvor new` and the package command is `renvor add`.
> Nothing below has been edited to agree with that change — this is dated evidence, and
> rewriting it would make the record disagree with what was actually verified and approved on
> the dates shown.
>
> **Deployment note added 2026-08-17.** Every statement below that **no Renvor site is deployed**,
> that **no image is published**, or that **`renvor.dev` serves no Renvor content** was true on its
> own date and is **false from 2026-08-17**. The landing site is deployed and serving over a valid
> Let's Encrypt certificate, from a published, digest-pinned, anonymously pullable image.
> **`docs.renvor.dev` is unchanged and still not deployed**, so statements naming that hostname
> alone remain true. Nothing below has been rewritten; individual rows most likely to mislead carry
> their own inline note. Current state:
> [`deployment-evidence.md`](deployment-evidence.md).


**Status**: **Phase 001 closure candidate** — **108 completed, 1 waived, 1 cancelled, 4 transferred (114 total)**. **Counted by task ID and explicit status marker, not by counting checkboxes** — see "How to count the tasks in this file" in `specs/001-governance-foundation/tasks.md`. **Waived**: **T088** (`WAIVED / NOT MET` under **W-003** — no independent human review of Phase 001 has occurred). **Cancelled**: **T114** (GitLab cutover abandoned; its recovery requirements were never met). **Transferred and still non-completed**: **T102**, **T108**, **T109**, **T111** — the four deployment gates, each with a named destination. **All six decision records accepted** (ADR-0001…ADR-0006), each reviewed as a **non-independent** self-review under W-002. Governance checklist 79/79. Verification passes on both toolchains with exit 0. The release procedure is documented and rehearsed without publishing (§3z); **no crate, package, container image, release, or tag has been published, and no site has been deployed** — *(verified read-only 2026-08-15: `crates.io` sparse index returns HTTP 404 for `renvor`, `renvor-cli`, and `renover` with `serde` returning 200 as a control; all four repositories hold zero releases and zero tags; no repository contains an image-publishing workflow; and `renvor.dev`, `docs.renvor.dev`, and `www.renvor.dev` each resolve to a shared origin returning HTTP 404 with no Renvor content. `renvor-rs/renvor-infra` **source** was published as a public repository on 2026-08-15, which is not an artifact publication. **GHCR was not independently enumerated** — the available token lacks `read:packages` and anonymous GHCR returns HTTP 403 without distinguishing absent from private — so the no-image statement rests on the absence of any publishing workflow or run, not on a registry listing.)* **Zero tasks remain open.** **Phase 001 is not, and must not be described as, independently reviewed**, and **no Renvor 1.0 claim is made or implied**.
**Satisfies**: spec FR-042, FR-043; PLAN.md §6.2
**Schema**: `specs/001-governance-foundation/data-model.md`
**Current topology (2026-08-15, ADR-0006 D13)**: all four repositories — `renvor-rs/renvor`, `renvor-rs/renvor-site`, `renvor-rs/renvor-docs`, `renvor-rs/renvor-infra` — are **public on GitHub and canonical there**. **No Renvor repository is private**, and no Renvor process depends on GitLab. **This ledger is dated and append-only**: sections before §3av describe the state on their own dates, including sections written while repositories were private or planned as private. Read any earlier section as evidence of its date, and **§3av as current state**. ADR-0006 is still `proposed` pending T106.
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
| Path | the maintainer's local backup directory, file `renvor-git-backup-2026-08-11.tar.gz` (outside the work tree) *(absolute path withheld 2026-08-15)* |
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

Byte-identical copies placed at `crates/renvor/` so the crate carries both when it is published.
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
| Path | the maintainer's local backup directory, file `renvor-pre-rewrite-backup-2026-08-11.tar.gz` *(absolute path withheld 2026-08-15)* |
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
| DNS (external check) | *(as observed 2026-08-11 — **superseded**, see §3af)* `renvor.dev` and `ahmedanbar.dev` both on Cloudflare nameservers (`coco`/`earl.ns.cloudflare.com`). `renvor.dev` had **no A record** at that time. `ahmedanbar.dev` resolves **directly to the origin IP**, i.e. DNS-only, not proxied |

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

## 3v. T054 — first public push (2026-08-11T23:29Z)

Gate 1 approved by the maintainer, scoped to the public framework repository only.

| Check | Result |
|---|---|
| Worktree / index before push | clean; 0 staged paths |
| Commit identities (all 12) | `Ahmed Anbar <admin@ahmedanbar.dev>` author **and** committer; **0 deviations** |
| Publish set | 70 files; all 13 exclusion classes absent; 0 credential-shaped paths |
| Excluded paths in any reachable commit | **0** |
| Secret scan, history | 1.37 MB, **0 findings** |
| Secret scan, working tree | **0 findings** |
| T054 prerequisites | T022 names confirmed; T044 both licences; T045 `SECURITY.md` with T052-attested contact; T053 zero-finding re-scan |
| Remote before push | **HTTP 409 "Git Repository is empty"**, `git ls-remote` returned 0 refs |

**Remote configured**: `git@github.com:renvor-rs/renvor.git` — the only remote.

**Push command** — one exact refspec, no `--mirror`, `--all`, `--tags`, `--force`, `--force-with-lease`, or wildcard:

```
git push origin refs/heads/main:refs/heads/main
```

### Post-push verification

| Check | Result |
|---|---|
| Local `main` = remote `main` | `d077844d64cdd7854331afe31ff4c47961af579f` — **identical** |
| Root tree | `457b09aaeb0088067c3edc5467222cb7a947644e` — **identical** |
| Remote blobs | **70**, listing not truncated |
| Excluded paths published | **0** |
| Tags published | **0** |
| `refs/codex/*` on remote | **0** |

### The session ref was inventoried, not deleted

A `refs/codex/*` ref had regenerated (`…/1786490358873/…`, tree `457b09aa`). Inventoried
before the push: **70 entries, 0 excluded paths, 0 blobs unique to it of 66** — a snapshot
of the same tree as `main`. It was **not** deleted, to avoid the delete/regenerate loop the
maintainer warned against. The exact refspec structurally cannot carry it, and the remote
was confirmed to hold zero `refs/codex/*` afterwards.

## 3w. T055–T063 — repository protection and automation (2026-08-11)

### Observed protection baseline — read back from the API, not asserted

| Field | Observed |
|---|---|
| Pull request required | **true** |
| Required approving reviews | **0** — under waiver **W-001** only |
| Dismiss stale reviews | true |
| **Enforce for administrators** | **true — no account can bypass, including admins** |
| Push restrictions | none — protection applies to everyone equally |
| Required status checks | **`verify (1.94.0)`, `verify (stable)`, `security`, `docs`** |
| Strict (branch must be current) | true |
| Linear history required | true |
| Force pushes | **false** |
| Deletions | **false** |
| Conversation resolution required | true |

**Protection was proven, not assumed.** A direct push of the bootstrap branch to `main` was
attempted and **rejected**: `remote: - Changes must be made through a pull request.`
`! [remote rejected] bootstrap-ci -> main (protected branch hook declined)` — while
authenticated as an organization administrator.

### Security controls

| Control | State |
|---|---|
| Secret scanning | **enabled** |
| Secret scanning push protection | **enabled** |
| Dependabot security updates | **enabled** |
| Dependency graph / vulnerability alerts | **enabled** (HTTP 204) |
| Code scanning (CodeQL default setup) | **configured**, languages `["rust"]`, suite `default` |
| Dependency review | **enabled** via workflow on pull requests |
| Repository | `public`, default branch `main` |

**Two optional settings could not be enabled** — reported rather than omitted:
`secret_scanning_non_provider_patterns` and `secret_scanning_validity_checks`. The API
accepts the PATCH and returns HTTP 200, but both remain `disabled`; they are Secret
Protection features not available on this repository's tier. **Neither is in the T056
required set**, so no control required by the specification is missing, and **no
control-unavailability waiver is created**.

### Workflows and permissions

| Workflow | Top-level permissions | Job elevation | Produces |
|---|---|---|---|
| `ci.yml` | `contents: read` | none | `verify (1.94.0)`, `verify (stable)` |
| `security.yml` | `contents: read` | `pull-requests: read` on the dependency-review job only | `security` |
| `docs.yml` | `contents: read` | none | `docs` |

`docs.yml` grants **no** `pages: write` or `id-token: write`. The production documentation
site is a separate repository behind a separate gate, so no deployment permission exists to
be abused.

**CodeQL is not an advanced workflow.** GitHub rejects a repository configured with both
default setup and an advanced CodeQL workflow. Default setup is enabled, so `security.yml`
covers cargo-deny, clippy, and dependency review instead. This is a deliberate choice, not
an omission.

### Third-party action pinning (T062)

All **6** distinct third-party actions are pinned to full 40-character commit SHAs;
**0 unpinned**. gitleaks is installed from a pinned release with its archive SHA-256
verified before extraction — an unverified download inside the job that performs secret
scanning would undermine the check it exists to run.

### ⚠️ A control was preserved rather than weakened

The first bootstrap run **failed `dependency-review`**: the build-cache action
`Swatinem/rust-cache` is **LGPL-3.0**, which is not on the `deny.toml` allow-list, and
dependency review evaluates GitHub Actions as dependencies.

**The allow-list was not widened.** The caching action was removed instead. A build cache
is a convenience; the licence policy is a control, and adding LGPL-3.0 to the allow-list to
make a bootstrap job green would have inverted that. Continuous integration runs roughly a
minute slower and `deny.toml` and dependency review now agree.

### Required checks — first observed results

| Check | Result | Duration |
|---|---|---|
| `verify (1.94.0)` | **pass** | 59s |
| `verify (stable)` | **pass** | 53s |
| `security` | **pass** | 43s |
| `docs` | **pass** | 40s |
| `dependency-review` | pass | 11s |
| `Analyze (rust)` (CodeQL) | pass | 1m55s |
| `CodeQL` | pass | 1s |

All well inside the 10-minute performance target in the verification contract.

### Waiver count (SC-008)

| Category | Expected | Actual |
|---|---|---|
| Repository **approval** waivers | exactly 1 | **1** — W-001 |
| **Control-unavailability** waivers | 0 | **0** — every required control was available on the free public tier |
| Explicit reviewed exceptions | outside the count | 1 — W-002 |

## 3x. T086 governance review and decision-record acceptance (2026-08-12)

### T086 — all 79 checklist items reviewed

Reviewed by **Ahmed Anbar — self-review under W-002**. **Not independent**, and not
described as such anywhere.

| Outcome | Count |
|---|---|
| Passed with recorded basis | **77** |
| **Failed — genuine specification gap** | **2** (CHK048, CHK050) |
| Requirements weakened to obtain a pass | **0** |
| Items with a defensible recorded outcome | **79 / 79** |

**CHK048** — FR-046 requires evidence retention "for a stated period" and no duration
exists anywhere. **Task T103.**
**CHK050** — FR-010 requires advisory handling to be stated but defines no response or
triage window, so an advisory could sit unactioned indefinitely without breaking any rule.
**Task T104.**

Both live tensions flagged at checklist creation were resolved by recorded ruling rather
than rewording: **CHK074** by W-002 (structured self-review, reviewer string fixed,
prohibition on calling it independent) and **CHK075** by the three-category waiver ledger
(approval = exactly 1, control-unavailability = 0 observed, reviewed exceptions outside
both counts).

### W-002 control evidence, applied per record

| Control | Evidence |
|---|---|
| 1 — written alternatives-and-consequences review | Each record carries 5–7 rejected alternatives with reasons and a stated cost section |
| 2 — verification against `checklists/governance.md` | T086 complete 2026-08-12, 77/79 |
| 3 — all required CI and security checks passing | 2026-08-11 on `renvor-rs/renvor`: `verify (1.94.0)` 59s, `verify (stable)` 53s, `security` 43s, `docs` 40s, plus dependency review and CodeQL |
| 4 — dated review record stored with the ADR | Acceptance-gate section in each record, dated 2026-08-12 |

### Verdicts

| ADR | Controls met | State | Reason |
|---|---|---|---|
| **0001** public naming | 1 ✅ 2 ✅ 3 ✅ 4 ✅ | **accepted** | CHK011–CHK019 all passed; no unresolved requirement affects it |
| **0002** workspace boundaries | 1 ✅ 2 ✅ 3 ✅ 4 ✅ | **accepted** | CHK028–CHK036 all passed |
| **0003** MSRV and dependency policy | 1 ✅ **2 ❌** 3 ✅ 4 ✅ | **proposed** | **CHK050 falls inside the dependency-policy scope this record decides.** Accepting it would put an accepted record's name behind a policy with an undefined advisory window. Blocked on **T104** |
| **0004** documentation platform | 1 ✅ 2 ✅ 3 ✅ 4 ✅ | **accepted** | CHK053–CHK058 all passed; this record supplies the CHK055 cadence |
| **0005** web-property topology | 1 ✅ 2 ✅ 3 ✅ 4 ✅ | **accepted** | CHK023 passed — brand-asset licensing is explicitly outside the code grant and tracked as T098, which blocks repository creation rather than this decision |
| **0006** hosting and edge | 1 ✅ 2 ✅ 3 ✅ 4 ✅ | **proposed** | **All four W-002 controls met, and still not accepted.** Four material architecture questions inside its own scope remain open (T099 registry, T105 `www` redirect, T106 backup ruling, T101 CSP). A record must not be accepted while its own text says its decisions have not been made |

**Accepted: 4. Remaining proposed: 2.** Every accepted record carries reviewer
`Ahmed Anbar — self-review under W-002`, review date 2026-08-12, and an explicit statement
that the review is not independent.

**T102 remains deliberately open** — the shared-server audit must be re-verified
immediately before any deployment and must not be marked complete in advance.

## 3y. T103 and T104 — policy corrections closing the T086 findings (2026-08-12)

The T086 review found two genuine specification gaps. Both are now closed by writing the
missing policy, not by softening the requirement. **The original finding of two failures
stands recorded**; the checklist preserves both dated failure notes with resolution notes
appended beneath them.

### T103 — evidence retention

**Authoritative**: `governance/evidence-retention-policy.md`.
**Also stated in**: spec FR-046 · `contracts/package-metadata.md` §Release evidence ·
data-model §Evidence Retention Schedule.

| Class | Retention |
|---|---|
| Ordinary CI logs and temporary workflow artifacts | **90 days** — the platform maximum for public repositories |
| Tracked governance evidence records | **Lifetime of the project** |
| Binary release evidence | **The later of** 7 years after publication **or** 3 years after that release's supported lifetime ends |
| Manifest, checksums, SBOM, attestation bundle, signing metadata | **Lifetime of the project** |

Workflow artifacts are evidence **transport**, never the durable archive. The canonical
public copy is the corresponding immutable release. A **second, independently controlled,
encrypted, versioned archive with access logging and an annual restore test** is required
before the first real registry publication — **it does not exist today, and no document
claims it does.** The Phase 013 release gate **fails closed** without it. Provider undecided.

**Phase 001 has no clause 4–8 obligation to discharge**, because it publishes no crate and
no release. Its durable evidence is this tracked record, retained for the lifetime of the
project; its CI artifacts follow the 90-day rule.

**The numeric periods are Renvor policy decisions**, not durations mandated by GitHub or
NIST. `RELEASING.md` must incorporate the policy exactly at **T070, which remains open**.

### T104 — dependency-advisory response

**Authoritative**: `governance/dependency-advisory-policy.md`.
**Also stated in**: spec FR-010 · `contracts/support-policy.md` · `CONTRIBUTING.md` ·
ADR-0003 · data-model §Advisory Record · a `deny.toml` comment explaining why a duration
cannot live in that file.

Measured from confirmed detection:

| Condition | Triage | Remediate |
|---|---|---|
| Known active exploitation | 24 hours | Begin immediately; decision within 24 hours |
| Critical | 24 hours | 7 calendar days |
| High | 48 hours | 14 calendar days |
| Medium | 5 calendar days | 30 calendar days |
| Low | 10 calendar days | 90 days or the next prerelease, whichever is first |

Severity is not the CVSS base score alone. **Absence of an upstream fix does not extend a
deadline.** Critical and High are public-release blockers that **cannot be waived**. Medium
and Low acceptance requires a time-bounded written exception. An ignored advisory without a
dated record is prohibited. Open Critical and High records get progress updates at least
every 5 calendar days.

**The numeric deadlines are Renvor policy decisions**, informed by CVSS, FIRST, RustSec, and
NIST SP 800-218 but mandated by none of them.

**`SECURITY.md` is unchanged.** It governs inbound private reports about Renvor; this policy
governs advisories against dependencies. The two clocks are deliberately different, and each
document now says so.

### T086 final result

| Stage | Result |
|---|---|
| Initial review, 2026-08-12 | **77 passed, 2 failed** (CHK048, CHK050), 0 weakened |
| Corrective work | **T103** and **T104** |
| Final re-review, 2026-08-12 | **79 of 79 passed**, 0 failed, 0 weakened |

### ADR-0003 — accepted on re-review

| # | W-002 control | Status |
|---|---|---|
| 1 | Written alternatives-and-consequences review | ✅ five alternatives, costs stated |
| 2 | Governance checklist | ✅ **79/79** — CHK050, which blocked this record, resolved by T104 |
| 3 | Required CI and security checks | ✅ all four passing |
| 4 | Dated review record with the ADR | ✅ 2026-08-12 |

**State `accepted`.** Reviewer `Ahmed Anbar — self-review under W-002`, review date
2026-08-12, explicitly **not independent**. The record keeps its review history showing the
first review did not accept it.

The advisory policy is incorporated **by authoritative reference**, not copied, so the
numbers exist in one file and every other mention resolves in its favour.

**Decision records: 5 accepted, 1 proposed.** ADR-0006 remains `proposed` — T099, T101,
T102, T105, and T106 all remain open and were not touched in this pass. *(Dated statement,
true when written. **ADR-0006 was accepted 2026-08-15** after T106 closed; T099, T101, and
T105 closed earlier. **T102 remains non-completed and transferred.**)*

## 3z. T070–T081 — release documentation and non-publishing rehearsal (2026-08-12)

**Nothing was published, tagged, released, or uploaded.** Every registry-reaching command
was run with `--dry-run`, and the one live registry query was a read.

### 3z.1 T070 — `RELEASING.md`

Written at repository root. Covers the ten release gates, the mandatory clean checkout,
topological publication order, package inspection, version and changelog rules, signing
expectations, the manual first-publication bootstrap with immediate token revocation,
trusted publishing thereafter, the evidence set, and yank-and-replace.

Policy numbers are **referenced, not copied**. `governance/evidence-retention-policy.md`
is named authoritative for retention and
`governance/dependency-advisory-policy.md` for advisory deadlines, discharging the T103
obligation that `RELEASING.md` incorporate the retention policy exactly rather than
restating it divergently.

`RELEASING.md` §11 states that the required independent encrypted, versioned archive
**does not exist**, and that the release gate therefore **fails closed**. §12 lists the
three decisions blocking a first release. The document opens by stating that Phase 001
publishes nothing.

### 3z.2 T073 / T081 — `.github/workflows/release-dry-run.yml`

| Property | Value |
|---|---|
| Triggers | `workflow_dispatch`, `pull_request` (packaging paths only) |
| `pull_request_target` | **absent** |
| Top-level permissions | `contents: read` |
| Job `rehearse` | inherits `contents: read`, no elevation |
| Job `attest` | `contents: read`, `id-token: write`, `attestations: write` — the documented `actions/attest-*` minimum |
| `contents: write` | granted **nowhere** |
| `packages: write` | granted **nowhere** |
| Registry credential | none referenced; no `secrets.` expression appears at all |
| `cargo publish` invocations | 1, carrying `--dry-run` |
| Tag / release / image creation | none |
| Third-party actions | 7, each pinned to a full 40-character commit SHA with a version comment |

Actions pinned: `actions/checkout` v7.0.1, `dtolnay/rust-toolchain` v1,
`taiki-e/install-action` v2.85.11, `actions/upload-artifact` v7.0.1,
`actions/download-artifact` v8.0.1, `actions/attest-build-provenance` v4.2.2,
`actions/attest-sbom` v4.1.0. All are MIT-licensed and therefore raise no
`deny.toml` conflict; no licence policy was widened to accommodate any of them.

Artifact retention is set to **90 days**, matching
`governance/evidence-retention-policy.md` clause 1 and the platform maximum for public
repositories.

**Contract amendment.** `contracts/package-metadata.md` previously required both artifact
attestation (FR-045) and a release workflow elevating **nothing**. Those cannot both hold.
The contract row was corrected to the documented minimum, with the reason recorded in
place. The invariant that mattered — the workflow cannot publish — is unchanged.

### 3z.3 Two defects the rehearsal found in its own workflow

Recorded because the first attempt did not succeed, and the record says so.

| # | Defect | Correction |
|---|---|---|
| 1 | `cargo cyclonedx` writes a BOM adjacent to **every** manifest in the workspace, not only the one named by `--manifest-path`. It also emitted one beside `xtask`, which the original step left behind in the checkout | Strays are deleted; only the publishable crate's SBOM is kept |
| 2 | The artifact directory sat **inside** the checkout, so the cleanliness assertion could never pass — it tripped over the untracked directory the previous step had just created | Artifacts moved outside the checkout; the checkout is asserted unmodified |
| 3 | The fix for defect 2 used `${{ runner.temp }}` in a **workflow-level `env:` block**, where the `runner` context does not exist. GitHub rejected the file at validation time and produced **two failed runs** before any job started | Path set in a step from `$RUNNER_TEMP`, which the runner always exports |

Defects 1 and 2 were fixed in commit `ca9ce23`, kept **separate** from the original so the
sequence remains legible. Defect 2 is the reason defect 1 survived the first write: an
assertion that cannot pass teaches its reader to ignore it.

**Defect 3 reached the remote and failed there**, which is recorded rather than hidden. Its
signature is worth knowing: an invalid workflow produces a run named after the **file path**
instead of the workflow's `name:`, with **zero jobs**, attributed to the `push` event even
when the file declares no `push` trigger — because the failure happens during workflow
validation, before triggers are evaluated. Local YAML parsing passed, since the file was
valid YAML and invalid *Actions schema*; the two are different checks.

`actionlint` 1.7.12 reproduces it exactly:

```text
release-dry-run.yml:52:22: context "runner" is not allowed here.
available contexts are "github", "inputs", "secrets", "vars".
```

`actionlint` now reports **no findings across all four workflow files**. It is not yet part
of `cargo xtask verify`; adding it is a candidate improvement, not a claim made here.

### 3z.4 T074–T079 — clean-checkout rehearsal

Run 2026-08-12, `darwin/aarch64`, operator Ahmed Anbar, toolchain
`rustc 1.94.0 (4a4ef493e 2026-03-02)`.

Executed from a **freshly created clone** (`git clone --no-local`, no hardlinks) checked
out at the exact candidate commit **`ca9ce236e93407402dc4ec9897393ac733b225de`**, verified
clean with zero untracked files before any command ran.

| Task | Command | Result |
|---|---|---|
| T074 | `cargo package -p renvor --list` | **8 files** (below) |
| T075 | Exclusion inspection, 9 categories + `gitleaks dir` | **all CLEAN**, 0 leaks in ~19.83 KB |
| T076 | `cargo package -p renvor` | Packaged 8 files, 19.4 KiB (7.2 KiB compressed) |
| T076 | `cargo publish -p renvor --dry-run` | `warning: aborting upload due to dry run` — **no upload** |
| T077 | Metadata validation | **12 of 12** required fields present |
| T078 | One live registry query | **zero versions** for every intended name |
| T079 | Credential inspection | **no long-lived registry credential** |
| — | CycloneDX SBOM | CycloneDX **1.5** JSON, subject `renvor 0.0.0`, 0 components |
| — | Checkout unmodified after all steps | ✅ clean |

**Shipped file set (T074)** — exactly what `include` states, plus the three files Cargo
adds itself:

```text
.cargo_vcs_info.json   Cargo.lock   Cargo.toml   Cargo.toml.orig
LICENSE-APACHE         LICENSE-MIT  README.md    src/lib.rs
```

**T075 inspection outcome** — every category returned zero hits: editor/agent/local
configuration; build output and caches; env, credential, and key files; internal
specifications and planning documents; branding assets; absolute-path or home-directory
leakage; hostname leakage; foreign email addresses; tool-attribution wording. An
independent `gitleaks dir` pass over the extracted archive reported **no leaks found**.

`.cargo_vcs_info.json` contains only `{"git":{"sha1":"ca9ce23…"},"path_in_vcs":"crates/renvor"}` —
a commit reference and a repository-relative path, no build-machine detail.

**Artifact (T076)**

| Property | Value |
|---|---|
| Path as produced | `target/package/renvor-0.0.0.crate` |
| Size | **7,418 bytes** (registry limit 10,485,760) |
| SHA-256 | `0993c5e1082f7bac925df0b71c7027d6434b14da0f884f8b433c69dbdfce6f63` |

**Checksums are commit-bound, and that is correct.** An earlier rehearsal at commit
`93b4f3f` produced SHA-256 `fa6836ab86bebc20cba64b213c32c5eedf42ea1b60d0a4d6cb18e094242b94b7`.
The archive embeds the commit hash in `.cargo_vcs_info.json`, so a different commit
necessarily yields a different archive. Both values are recorded rather than the second
quietly replacing the first.

**The SBOM is not byte-reproducible.** `cargo-cyclonedx` emits a fresh random
`serialNumber` UUID per run (`942a0e89-…` and `6147b919-…` across two runs of identical
input), so its checksum differs run to run even at the same commit. Recorded as a known
property of the tool; a release must checksum the SBOM it actually publishes rather than
expecting a stable value.

**T077 metadata** — `name`, `version`, `description`, `license`, `repository`, `homepage`,
`documentation`, `readme`, `keywords`, `categories`, `rust-version`, `edition` all present.
`license` is exactly `MIT OR Apache-2.0`; `rust-version` is exactly `1.94.0`, matching
`SUPPORT.md`; `edition` is `2024`.

**Publishable set and dependency shape** — `renvor` is publishable with **0 dependencies,
0 path dependencies, 0 git dependencies**. `xtask` is `publish = false` and is therefore
not packaged: `cargo package --workspace` *would* have included it, which is why the
rehearsal uses the per-package form.

**T078 — one authoritative registry query**, `GET https://crates.io/api/v1/crates?q=renvor&per_page=100`
at **2026-08-12T06:35:01Z**. Total matches: **0**. Neither `renvor` nor `renvor-cli` exists
on crates.io; both have **zero published versions**. A single namespace query was used
rather than repeated per-name polling.

**T079 — credential inspection.**

| Location | Result |
|---|---|
| Repository Actions secrets | **0** |
| Dependabot secrets | **0** |
| Actions variables | **0** |
| Environment secrets | **0** — no environment exists |
| `CARGO_REGISTRY_TOKEN` / `cargo login` / `credentials.toml` in tracked files | **none** outside specification prose |
| `.cargo/config.toml` | contains only the `xtask` alias |
| `~/.cargo/credentials.toml` on the operator machine | **absent** |

**Scope limit, stated rather than glossed:** organization-level secrets could **not** be
enumerated — the API returned HTTP 403 for want of the `admin:org` scope. This evidence
therefore establishes that no repository-level registry credential exists; it does **not**
establish the same for organization-level secrets. Closing that gap needs a separate
authenticated check by the organization owner.

### 3z.5 Release artifact set a real release would retain

| Artifact | Rehearsal value |
|---|---|
| `renvor-0.0.0.crate` | 7,418 bytes |
| `renvor-package-list.txt` | 107 bytes |
| `renvor-sbom.json` | 2,140 bytes, CycloneDX 1.5 |
| `SHA256SUMS` | 264 bytes, covering all three |
| Build provenance attestation | produced by the merged workflow, `workflow_dispatch` only |
| SBOM attestation | produced by the merged workflow, `workflow_dispatch` only |
| Toolchain, platform, operator, date | this record |

Retention of each class is governed by `governance/evidence-retention-policy.md`.

### 3z.6 T071 — signing discovery (read-only, **nothing changed**)

No private key was read, printed, copied, created, or modified.

| Setting | Local | Global |
|---|---|---|
| `commit.gpgsign` | unset | unset |
| `tag.gpgsign` | unset | unset |
| `gpg.format` | unset | unset |
| `user.signingkey` | unset | unset |
| `gpg.ssh.allowedsignersfile` | unset | unset |

- **Signing is not configured anywhere.** Every commit authored locally in this repository
  is unsigned; GitHub reports `verified: false, reason: "unsigned"` for all of them.
- The four merge commits **are** `verified: true`, signed by GitHub's web-flow key as
  server-side merge metadata. That is the platform's signature, **not** the maintainer's.
- `gpg` and `ssh-keygen` are both available. **Zero GPG secret keys** exist on the machine.
- Six SSH key pairs exist, all for authentication to other hosts, none dedicated to
  signing; no key comment mentions signing and no `~/.ssh/allowed_signers` file exists.
- **No dedicated signing identity exists.**
- **Vigilant mode state is unknown.** GitHub exposes no API for it. It is not inferable
  from commit verification status, and no claim is made either way. The account owner must
  confirm it in account settings.
- Account signing-key metadata could not be listed: `admin:ssh_signing_key` and
  `admin:gpg_key` scopes are absent from the current token. Recorded as a scope limit.

**T071 remains open.** The recommendation is in §3z.8; the choice is the maintainer's.

### 3z.7 T072 — release environment discovery (read-only, **nothing changed**)

`GET /repos/renvor-rs/renvor/environments` → `total_count: 0`. **No environment exists.**

Available protection controls: required reviewers (up to 6 people or teams, **one**
approval sufficient), wait timer, deployment branch **and tag** policies, prevent
self-review, disable administrator bypass, and custom rules via GitHub Apps. Environment
protection is available on **public** repositories on the free plan, which this repository
is.

**The deadlock is real and must be decided, not assumed.** With one maintainer, enabling
*prevent self-review* while naming that maintainer as the only reviewer makes every release
unapprovable: the person who triggers the run is the only person who could approve it. The
proposed configuration in §3z.8 resolves this deliberately rather than by discovering it
during a release.

**T072 remains open** pending the maintainer's decision and explicit approval to mutate
GitHub state.

### 3z.8 Recommendations requiring a maintainer decision

**Signing — recommended: a dedicated SSH signing key.**

| | Dedicated SSH key | Dedicated GPG key |
|---|---|---|
| Setup | `ssh-keygen -t ed25519`, set `gpg.format ssh`, upload as a **signing** key | Generate keypair, configure agent and pinentry, export and upload |
| Existing material | 6 SSH keys already present and working | **0 GPG secret keys** — starts from nothing |
| Expiry / revocation | No native expiry; rotation is manual — remove the old key, add the new | Native expiry and revocation certificates |
| Verification of history | Needs an `allowed_signers` file for local `git verify-tag`; GitHub verifies server-side | Verifies from the keyring |
| Loss of key | Rotate and re-upload; past signatures stop verifying unless the old public key is retained | Revocation certificate, if one was generated **in advance** |
| Failure mode | Few moving parts | Agent, pinentry, and expiry each fail independently, usually at release time |

SSH is recommended because the operational surface is smaller and this project has one
maintainer: GPG's advantages — native expiry and revocation — are governance features that
matter most for multi-party keyrings, while its costs land on every single signing
operation. GPG's genuine advantage is that a revocation certificate lets a lost key be
positively invalidated; SSH has no equivalent, so **key loss is the risk that must be
accepted**, mitigated by keeping the old public key registered so historical signatures
continue to verify.

**Exact user action required** (not performed — T071 is open):

1. Generate a key used **only** for signing, not reused for authentication.
2. Upload it to GitHub as a **Signing key** (a separate key type from an authentication key).
3. Set `gpg.format ssh`, `user.signingkey`, `commit.gpgsign true`, `tag.gpgsign true`.
4. Create `~/.ssh/allowed_signers` so `git verify-tag` works locally.
5. Enable **vigilant mode** in account settings.
6. Back up the private key offline; retain the old public key on rotation.

**Proposed protected release environment** (not created — T072 is open):

| Setting | Proposed value | Reason |
|---|---|---|
| Name | `release` | Referenced by name from the future publish workflow |
| Required reviewers | **`AhmedAnbar`**, named individually | A role or team alias is not a named approver |
| Prevent self-review | **Disabled**, with the reason recorded | Enabling it with a single maintainer deadlocks every release. Recorded as a known limitation to reverse when a second maintainer exists — **not** as a control that was quietly skipped |
| Deployment **tag** rule | `v*` | Publication runs only from a release tag |
| Deployment branch rule | none | Releases come from tags, not branches |
| Administrator bypass | **Disabled** | A bypass available to the only approver is not a gate |
| Wait timer | 0 | A timer is not a control; the reviewer gate is |
| Environment secrets | **none** | Trusted publishing leaves nothing to store |

The approval gate remains meaningful with self-review disabled: it forces a **deliberate,
logged, out-of-band confirmation** between tagging and publishing, and it restricts
publication to `v*` tags. It does not provide four-eyes review, and this record does not
claim that it does. That gap is the same one W-001 already records for pull requests.

### 3z.9 Validation for this pass (2026-08-12)

| Check | Command | Result |
|---|---|---|
| Full sequence, MSRV | `rustup run 1.94.0 cargo xtask verify` | **exit 0** — all 10 steps ran and passed |
| Full sequence, stable | `rustup run stable cargo xtask verify` | **exit 0** — all 10 steps ran and passed |
| Toolchains genuinely differ | `rustc --version` per channel | 1.94.0 vs **1.97.1** — two different compilers, not one twice |
| Secret scan, working tree | `gitleaks dir .` | 0 findings over 9.49 MB |
| Secret scan, history | `gitleaks git .` | 0 findings, 25 commits |
| Canary detection | injected credential, FP-001 file | **detected** — see below |
| Release workflow syntax | YAML parse of all four workflows | all parse; permission matrix as designed |
| Package contents, metadata, dry run, SBOM, checksums | §3z.4 | all pass |
| No runtime capability added | `git diff --stat main...HEAD` | **0 Rust source files touched**; public API still exactly 3 constants |

Steps 1–9 cover formatting, clippy, tests, API documentation, dependency and licence
policy, secret scanning, documentation build, and link checking; step 10 is working-tree
cleanliness.

**Canary verification — and a mis-designed first attempt, recorded rather than discarded.**

The first attempt injected an AWS *documentation example* key into `RELEASING.md` and was
not detected. That was a **defective test, not a detection failure**: the value is a
publicly known example that gitleaks deliberately does not flag, and `RELEASING.md` is not
the file the FP-001 allowlist covers, so the test exercised nothing the canary exists to
exercise. Reporting it as a regression would have been wrong; so would deleting it.

Re-run correctly against the file FP-001 actually scopes:

| Stage | Observed |
|---|---|
| Baseline | 0 findings — the allowlisted prose at line 679 stays suppressed |
| Canary injected at line 682 of the **same file** | **1 finding**, rule `gitlab-pat`, exit 1 |
| File restored | SHA-256 `19883f55…4711` before and after — **byte-identical** |
| Working tree | clean |

This is the property that matters: the allowlist suppresses **one prose match by content**
and does not exclude the file, because a secret added to that same file three lines later
is still caught. A `paths`-scoped allowlist would have failed this test, which is exactly
how the original defective form was found.

### 3z.10 Dependency advisories raised 2026-08-11 — dated triage record

Surfaced by GitHub during the branch push of 2026-08-12. Recorded here because
`governance/dependency-advisory-policy.md` §9 forbids an ignored advisory without a dated
record. **Not remediated in this pass** — remediation was outside its authorised scope.

| GHSA | Severity | Package | Ecosystem | Manifest | Upstream fix |
|---|---|---|---|---|---|
| `GHSA-5c6j-r48x-rmvq` | **High** | `serialize-javascript` ≤ 7.0.2 | npm | `docs/package-lock.json` | **7.0.3** |
| `GHSA-w3rx-r6r6-pgpr` | **High** | `image-size` ≤ 2.0.2 | npm | `docs/package-lock.json` | **none** |
| `GHSA-5p2g-fcmc-qvqq` | **High** | `image-size` ≤ 2.0.2 | npm | `docs/package-lock.json` | **none** |
| `GHSA-qj8w-gfj5-8c6v` | Medium | `serialize-javascript` < 7.0.5 | npm | `docs/package-lock.json` | 7.0.5 |
| `GHSA-w5hq-g745-h8pq` | Medium | `uuid` < 11.1.1 | npm | `docs/package-lock.json` | 11.1.1 |

- **Detected** 2026-08-11T23:39:22Z. As of 2026-08-12T06:49Z, **7.2 hours** have elapsed:
  within the 48-hour High triage window and the 5-day Medium window. **No deadline has
  been breached.** Remediation deadlines are 14 days (High) and 30 days (Medium) from
  detection.
- **Reachability**: all five are transitive npm dependencies of the **documentation site**.
  The crate that would be published has **zero dependencies** of any kind, and the packaged archive
  contains 8 files, none from `docs/` (§3z.4). **None of these advisories reaches the
  crate that would be published.** `cargo deny check` passes because it governs the Cargo
  graph, which is genuinely clean — not because it was configured to overlook these.
- **Two High advisories have no upstream fix.** Policy §6 is explicit that this does not
  extend the deadline: `image-size` must be updated, removed, replaced, or isolated, or
  the affected release blocked. It is a Docusaurus transitive dependency, so "isolate" in
  practice means the documentation build, not the crate.
- **Owner**: Ahmed Anbar. **Named owner assignment and the full clause-5 advisory records
  are still outstanding** — this entry starts the trail, it does not complete it.

**No release is blocked today**, because **no release artifact** is published and no release
is in progress. *(Scope clarified 2026-08-15: "published" here means a crate, package, image,
or release — repository **source** was later published for `renvor-infra`, §3av, which is not
a release and blocks nothing.)* The High advisories become release blockers under policy §7 the moment one is.

## 3aa. T107–T109 — documentation dependency advisory triage (2026-08-12)

Five advisories raised 2026-08-11T23:39:22Z against `docs/package-lock.json`. **Two closed,
three open with a gate.** All five are **transitive**; none is a declared dependency.

### 3aa.1 Triage table

| GHSA | CVE | Package | Path | D/T | Sev | CVSS | Installed | First patched | Reachable | PR-input exposure | Upstream | Action | Triage due | Remediate due | Disposition |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `GHSA-5c6j-r48x-rmvq` | — | `serialize-javascript` | core → bundler → copy-webpack-plugin@11 / css-minimizer-webpack-plugin@5 | T | **High** | 8.1 | 6.0.2 | **7.0.3** | **Yes** — runs in every production build | None | Fixed upstream | Override → `^7.1.0` | 2026-08-13 | 2026-08-25 | **RESOLVED** |
| `GHSA-qj8w-gfj5-8c6v` | CVE-2026-34043 | `serialize-javascript` | same | T | Medium | 5.9 | 6.0.2 | **7.0.5** | **Yes** | None | Fixed upstream | Override → `^7.1.0` | 2026-08-16 | 2026-09-10 | **RESOLVED** |
| `GHSA-w3rx-r6r6-pgpr` | CVE-2025-71330 | `image-size` | core → @docusaurus/mdx-loader → image-size | T | **High** | 7.5 | 2.0.2 | **none** | **No** — 0 MDX image embeds | None | **No fix exists** | Gate + mitigate (T108) | 2026-08-13 | 2026-08-25 | **OPEN — deployment gate** |
| `GHSA-5p2g-fcmc-qvqq` | CVE-2025-71329 | `image-size` | same | T | **High** | 7.5 | 2.0.2 | **none** | **No** | None | **No fix exists** | Gate + mitigate (T108) | 2026-08-13 | 2026-08-25 | **OPEN — deployment gate** |
| `GHSA-w5hq-g745-h8pq` | CVE-2026-41907 | `uuid` | core → webpack-dev-server → sockjs → uuid | T | Medium | 7.5 | 8.3.2 | 11.1.1 | **No** — v4 only, no `buf` | None | `sockjs` 0.3.24 is latest, pins `^8.3.2` | Record, reassess (T109) | 2026-08-16 | 2026-09-10 | **OPEN — not reachable** |

Owner for all five: **Ahmed Anbar**. Detected 2026-08-11T23:39:22Z; triage completed
2026-08-12, **within** the 48-hour High and 5-day Medium windows.

### 3aa.2 Two counting traps in npm's output, both rejected

**npm reported 25 "vulnerabilities" (19 high) before the fix and 21 after.** Those are
*packages along dependency paths*, not advisories. They trace to **3 vulnerable packages**
and **5 distinct advisories**. The honest post-fix figure is **3 advisories remaining**, not
21. Aggregated path counts were not used anywhere in this record.

**npm offered `@easyops-cn/docusaurus-search-local@0.29.0` as a fix.** The project runs
`^0.52.1`, so that is a **downgrade across 23 minor versions** — the historical downgrade
the triage rules forbid without compatibility proof. **Rejected.** `npm audit fix --force`
was never run, Docusaurus was neither downgraded nor replaced, and no scanning, workflow
permission, or licence control was weakened.

### 3aa.3 T107 — the one compatible fix, and why an override was the only route

Docusaurus 3.10.2's bundler pins `copy-webpack-plugin ^11.0.0` and
`css-minimizer-webpack-plugin ^5.0.1`; both majors require `serialize-javascript ^6`. Only
`copy-webpack-plugin@14.0.0` and `css-minimizer-webpack-plugin@8.0.0` moved to `^7.0.3`,
and both sit outside the ranges Docusaurus declares. **No ordinary compatible update
exists**, which is what makes a reviewed override the correct instrument rather than a
shortcut.

The override is safe on evidence, not assertion:

| Version | `type` | `main` | `exports` map | `engines.node` |
|---|---|---|---|---|
| 6.0.2 (was) | commonjs | `index.js` | none | — |
| 7.1.0 (now) | commonjs | `index.js` | none | `>=20.0.0` |

Identical CommonJS shape; the only change is an engines floor that `.nvmrc` already
satisfies at Node 22.

**Proof performed** — a CommonJS load exercising the exact RegExp and Date paths the RCE
advisory concerned:

```text
require('serialize-javascript') OK
{"a":new RegExp("ab+c", "gi"),"d":new Date("2026-01-01T00:00:00.000Z"),"n":[1,2,3]}
```

Dependency path after the override, both consumers resolved:

```text
copy-webpack-plugin@11.0.0        -> serialize-javascript@7.1.0 overridden
css-minimizer-webpack-plugin@5.0.1 -> serialize-javascript@7.1.0 deduped
vulnerable 6.x entries remaining  : NONE
```

Frozen install (`npm ci`, exit 0), production build, and link check all passed;
`cargo xtask verify` steps 1–9 green with the override in place.

### 3aa.4 T108 — `image-size`, no fix at any version

**2.0.2 is simultaneously the affected version and the latest published version.** No
override, pin, or update can resolve this, because no fixed release exists to point at.

Reachability: `@docusaurus/mdx-loader` invokes `image-size` to measure images referenced
from MDX. The documentation source contains **zero MDX image embeds** and exactly **one**
image file, `static/img/favicon.ico`, which is copied verbatim and never parsed. All
content is project-authored; there is **no untrusted image input path**, and the site takes
no user uploads.

Temporary mitigations, recorded as mitigations and **not** as remediation:

- no image is embedded in MDX, so the vulnerable parsers are not invoked during the build;
- the ICNS, JXL, and HEIF formats the advisories name appear nowhere in the source tree;
- the build runs in an ephemeral CI container with no network-reachable service;
- the impact is denial of service against a build, not the published site.

Per dependency advisory policy §6 the absence of an upstream fix **does not extend the
deadline**. **The public documentation deployment gate stays closed** while these remain
open. **Not remediated**, and this record does not claim otherwise. Owner Ahmed Anbar,
reassessment **2026-08-26**, tracked at **T108**.

### 3aa.5 T109 — `uuid`, present but not reachable

`sockjs/lib/transport.js` line 9 binds `uuidv4 = require('uuid').v4` and line 37 calls
`uuidv4()` — **v4 only, with no `buf` argument**. The advisory affects **v3, v5, and v6
when `buf` is provided**. The vulnerable code path is therefore never entered.

`sockjs` also arrives through `webpack-dev-server`, which serves `docusaurus start` and is
**not** part of `docusaurus build` or the deployed output.

`sockjs` 0.3.24 is the latest release and pins `uuid ^8.3.2`, so no compatible update
exists. Forcing `uuid` across three majors would pin a transitive dependency inside a code
path CI never executes — an override that cannot be exercised is untested by definition,
and would trade real breakage risk for no risk reduction. **Recorded, not forced.**
Reassessment **2026-09-11**, tracked at **T109**.

## 3ad. T071 — commit and tag signing (2026-08-12)

### 3ad.1 The signing key

| Field | Value |
|---|---|
| Algorithm | Ed25519 (`ssh-ed25519`) |
| Public fingerprint | `SHA256:Y77mGrK4VudFhkJt+EKyCysSqH6nsp6N4GP0kIPKVTM` |
| Comment | `Ahmed Anbar <admin@ahmedanbar.dev> Renvor signing` |
| Private-key path | *(withheld — see the redaction note below)* |
| Public-key path | *(withheld — see the redaction note below)* |
| Private permissions | `-rw-------` (0600) |
| Public permissions | `-rw-r--r--` (0644) |
| Encryption | `aes256-ctr`, `bcrypt` KDF, **100 rounds** |
| Created | 2026-08-12T11:32:20Z |
| Purpose | **Signing only** — not registered as an authentication, deploy, or login key |

**No private-key contents and no passphrase appear in this record, in the repository, or in
any output.**

> **Path redaction, 2026-08-15 (maintainer decision).** The two rows above previously gave the
> **absolute filesystem paths** of the signing key pair. This repository is public, so those
> rows published the exact on-disk location of the release signing private key to anyone. A
> path is not a key and discloses no key material, but it converts any read access into
> precise targeting, and it proves nothing that the rows retained above do not already prove.
>
> **What is retained, because it is what actually establishes signing identity and hygiene:**
> the algorithm, the **public fingerprint** — which is the verifiable identity, and which is
> already published in `governance/allowed-signers` — the key comment, both permission modes,
> the passphrase encryption and KDF rounds, the creation timestamp, and the signing-only
> purpose. Nothing verifiable was lost.
>
> **Scope of this redaction:** it changes the published document. **Git history is not
> rewritten**, so the earlier revisions of this file still contain the paths and remain
> reachable; this is a deliberate trade — rewriting a public repository's history is more
> disruptive than the disclosure it would remove, and would break every signature and SHA
> already recorded in this ledger. **No key was read, opened, moved, copied, rotated, or
> deleted** by this change, and none is proposed here.
>
> **A separate, unaddressed matter is recorded as a future security action**: the key pair is
> stored in a **directory synchronised to a third-party file-sync service**, which widens its
> blast radius beyond this machine to that account and every device syncing it. **That is a
> storage-hardening question, not a documentation question**, it is out of scope for this
> phase, and it is tracked in §7 (recurring obligations) rather than silently closed here.

### 3ad.2 A generation defect, caught and corrected

**The key was first generated without a passphrase.** The `-a 100` argument was present but
had no effect: bcrypt KDF rounds only apply when there is a passphrase to derive from, so a
command that reads as hardened produced a plaintext private key.

Detected two ways before anything depended on it:

```text
OpenSSH key header decode:  ciphername = none   kdfname = none   kdfoptions = 0 bytes
empty-passphrase probe:     accepted -> key was unencrypted
```

This mattered specifically because of clause 3ad.5: an unencrypted private key in a
Dropbox-synchronised directory is the exact scenario the residual-risk mitigations exist to
cover.

Corrected with `ssh-keygen -p -a 100`, which **re-wraps the existing private key under a new
KDF rather than generating a new one**. Nothing was overwritten, deleted, or regenerated,
and no second key was created. Confirmed afterwards:

```text
ciphername = aes256-ctr   kdfname = bcrypt   rounds = 100
empty passphrase rejected
fingerprint unchanged: SHA256:Y77mGrK4VudFhkJt+EKyCysSqH6nsp6N4GP0kIPKVTM
```

The unchanged fingerprint is the proof that the key material survived: a fingerprint is a
hash of the public key, so a regenerated key would have produced a different one.

### 3ad.3 Agent, registration, and configuration

**Private/public correspondence proven without exposing private material.** The macOS SSH
agent loaded the key from the **decrypted private key** and reported
`SHA256:Y77mGrK4VudFhkJt+EKyCysSqH6nsp6N4GP0kIPKVTM` — identical to the fingerprint of the
`.pub` file. Deriving the fingerprint from the `.pub` alone would have been circular; this
is not.

| GitHub registration | Value |
|---|---|
| Account | `AhmedAnbar` (id 4220036) |
| Key id | **1108446** |
| Title | `Renvor release signing — Ahmed Anbar` |
| Type | **signing** (`/user/ssh_signing_keys`), not an authentication key |
| Key type | `ssh-ed25519` |
| Registered | 2026-08-12T11:45:52+03:00 |
| Fingerprint match, GitHub vs local | **YES** |

Only the public key was uploaded.

**Scope limit, stated rather than glossed:** the account's **authentication** keys could not
be enumerated — that needs `admin:public_key`, which was deliberately **not** requested,
because §6 authorises signing-key registration only. This record therefore establishes that
the key was *registered* as signing-only; it does not independently prove the absence of a
same-key authentication registration made previously.

Repository-local Git configuration (global identity untouched — it remains
`Ahmed anbar <begnulinux@gmail.com>`):

```text
user.name  = Ahmed Anbar          gpg.format      = ssh
user.email = admin@ahmedanbar.dev commit.gpgsign  = true
user.signingkey = <path withheld — public key file, see §3ad.1>
tag.gpgsign = true
gpg.ssh.allowedSignersFile = governance/allowed-signers
```

`governance/allowed-signers` is tracked and contains **public key data only**, with
`namespaces="git"` so a signature produced for another purpose cannot be replayed as a Git
signature.

### 3ad.4 Verification results

**Temporary tag** — deliberately named so it cannot be mistaken for a release and cannot
match the `v*` deployment policy.

| Field | Value |
|---|---|
| Tag name | `signing-selftest-do-not-release-20260812` |
| Matches `v*` | **No** — by design |
| Object type | `tag` (annotated) |
| Target commit | `ea51766e397620e7fab194201e203875a24104f6` |
| Signing fingerprint | `SHA256:Y77mGrK4VudFhkJt+EKyCysSqH6nsp6N4GP0kIPKVTM` |
| `git verify-tag` | `Good "git" signature for admin@ahmedanbar.dev` — **exit 0** |
| Verified | 2026-08-12T08:46:36Z |
| Deleted | Yes, immediately after verification |
| Ever pushed | **No** — 0 remote tags, 0 GitHub Releases, 0 matches on the remote |

**Gate logic tested in both directions.** A gate proven only on its happy path is a gate
nobody has tested.

| Check | Positive | Negative |
|---|---|---|
| Tag-name format | `v1.0.0`, `v0.1.0-rc.1`, `v10.20.30` accepted | `v1.0`, `version1.0.0`, `v1.0.0-`, and the selftest name rejected |
| Annotated-tag requirement | annotated tag → `type=tag`, accepted | genuine lightweight tag → `type=commit`; `verify-tag` fails with *cannot verify a non-tag object of type commit* |
| Signature, principal, fingerprint | all three matched | an unknown fingerprint did **not** match |
| Allowed-signers dependence | verified with the tracked file | **failed** against an empty allowed-signers file |

The last row is the one that matters most: it proves verification depends on *who* signed,
not merely on a signature existing.

**A test that was initially invalid, recorded rather than quietly fixed.** The first attempt
to create a lightweight tag failed, because `tag.gpgsign = true` makes plain `git tag`
refuse rather than produce an unannotated tag. The check therefore ran against a
*non-existent* tag and "passed" for the wrong reason. Re-run with
`git -c tag.gpgsign=false`, a real lightweight tag was created and genuinely rejected.

### 3ad.5 Vigilant mode — maintainer attestation

**Maintainer attestation, 2026-08-12**: Ahmed Anbar confirms that vigilant mode is enabled
on the `AhmedAnbar` GitHub account, flagging unsigned commits as unverified.

Recorded as an **attestation, not a measurement**. GitHub exposes no API for this setting,
and it is **not** inferable from commit verification status: the REST API reports
`verified: false, reason: "unsigned"` for an unsigned commit whether or not vigilant mode is
on. The maintainer's explicit confirmation is therefore the only evidence that exists, and
this record does not dress it up as anything stronger.

Why it matters: without vigilant mode, an unsigned commit and a signed one are visually
similar to a casual reader, which removes most of the value of signing at all.

### 3ad.6 Dropbox storage — explicit residual risk

**Storing the encrypted private signing key in a Dropbox-synchronised directory is an
explicit maintainer decision**, recorded here as accepted risk rather than presented as
best practice.

| Residual risk | Detail |
|---|---|
| Account or device compromise | Compromise of the Dropbox account, or of any synchronised device, exposes the encrypted private-key **file** |
| Passphrase dependence | Security then rests almost entirely on the strength and secrecy of the passphrase. The 3ad.2 defect is exactly why this was verified rather than assumed |
| Not a revocation mechanism | Dropbox versioning and sync are **not** a substitute for revocation or recovery planning. Sync propagates a compromise as readily as a backup |
| Team growth | A hardware-backed or local-only key should be reconsidered as the maintainer team grows |

Mitigations in force:

- strong, unique passphrase, entered directly by the maintainer, never printed, logged,
  committed, or stored beside the key;
- private key `0600`, public key `0644`, containing directory `drwx------`;
- key held in the macOS SSH agent and Keychain, so the passphrase is not re-entered per use;
- public fingerprint recorded here and in `governance/allowed-signers`;
- **no additional copies created** — exactly one private-key file exists;
- Dropbox account should carry strong multifactor authentication (**maintainer
  responsibility; not verified by this record**).

**Rotation and compromise response**

1. Generate a replacement key at a new path; **never** reuse the compromised path.
2. Register the new public key on GitHub as a **signing** key.
3. Add the new principal to `governance/allowed-signers` **by pull request**, keeping the
   old key entry so historical signatures continue to verify.
4. Update `user.signingkey` and the pinned fingerprint in
   `.github/workflows/release-tag-verify.yml`.
5. **Delete the compromised signing key from GitHub**, which stops it verifying new objects.
6. Remove the old private key from Dropbox and every synchronised device, and purge
   Dropbox version history for that path.
7. Record the incident, the compromise window, and every release tag signed inside it.
8. **Re-verify every release tag signed in that window.** SSH signing has no revocation
   certificate — this manual step is the price of the SSH choice over GPG, and it was
   accepted knowingly.

## 3ab. T072 — protected release environment (created 2026-08-12)

Created via API after resolving the reviewer identity first: `AhmedAnbar`, numeric id
**4220036**, repository permission **admin**.

Complete API read-back, every authorized property confirmed:

| Property | Required | Read back | |
|---|---|---|---|
| Environment name | `release` | `release` | PASS |
| Required reviewer | `AhmedAnbar` | `AhmedAnbar` | PASS |
| Reviewer numeric id | 4220036 | 4220036 | PASS |
| Prevent self-review | **disabled** | `false` | PASS |
| Wait timer | 0 | rule absent | PASS |
| Administrator bypass | **disabled** | `can_admins_bypass: false` | PASS |
| Custom ref policies | enabled | `true` | PASS |
| Protected-branches policy | not used | `false` | PASS |
| Deployment refs | **tags `v*` only** | 1 policy, `name: v*`, `type: tag` | PASS |
| Environment secrets | none | **0** | PASS |
| Environment variables | none | **0** | PASS |

Created 2026-08-12T08:39:42Z. No branch policy, no pull-request ref, and no unrestricted
ref is permitted: the single policy is of type `tag`.

**Why prevent-self-review is disabled, recorded rather than glossed.** The project has one
maintainer. Enabling it would make every release impossible, because the person who
triggers a release run would be the only person able to approve it. The environment still
delivers a deliberate, logged approval checkpoint and restricts publication to `v*` tags.
**It does not provide independent four-eyes review, and nothing here claims it does.** This
must be revisited when a second qualified maintainer joins — the same gap W-001 already
records for pull requests.

## 3ac. T081 — final attestation ledger entries (merged run)

Recorded from the successful merged run. **The rehearsal was not re-run**, and no
replacement attestation was created to simplify documentation.

| Field | Value |
|---|---|
| Workflow | `release-dry-run` |
| Run ID | **31572250881** |
| Run URL | <https://github.com/renvor-rs/renvor/actions/runs/31572250881> |
| Event | `workflow_dispatch` |
| Commit SHA | `0375016cfa7577f4d1c5d2da7d7fa8c138a4f440` |
| Conclusion | success |
| Package archive | `renvor-0.0.0.crate` |
| Archive SHA-256 | `ee64b04dcc9c18bb971b91f5384cdb2686c326f886e53979158949ed36dd8677` |
| `SHA256SUMS` SHA-256 | `9b8ebcb22010680643adf1c30d3278aeae269174f7fbdd9b1524f80665d69c42` |
| `renvor-package-list.txt` SHA-256 | `f4e61d2d1ec91ed5a2ce80c30dd8877c114ff6efc199cd0c4a95970abbef8992` |
| Artifact name | `release-rehearsal` |
| Artifact size | **8,903 bytes** |
| Artifact digest | `sha256:ebb113d5095c8affccbc5596cc4e547774618a4607743b4b9853a8a524086894` |
| Uploaded | 2026-08-12T07:01:28Z |
| Retention expires | **2026-11-10T07:01:13Z** (exactly **90 days**) |
| Operator | Ahmed Anbar |
| Date recorded | 2026-08-12 |

**Attestations — two, both verified by API read-back:**

| Predicate type | Bundle media type | Subjects |
|---|---|---|
| `https://slsa.dev/provenance/v1` | `application/vnd.dev.sigstore.bundle.v0.3+json` | `renvor-0.0.0.crate`, `SHA256SUMS`, `renvor-package-list.txt` |
| `https://cyclonedx.org/bom` | `application/vnd.dev.sigstore.bundle.v0.3+json` | `renvor-0.0.0.crate` |

SBOM format: **CycloneDX 1.5 JSON**, subject `renvor 0.0.0`, 0 components.

Verification command and result:

```text
GET /repos/renvor-rs/renvor/attestations/sha256:ee64b04d...
  -> 2 attestations; provenance buildDefinition names
     .github/workflows/release-dry-run.yml
```

Result: **PASS.** Both attestations resolve against the archive digest and the provenance
names the workflow that produced it.

## 3ae. T080 — the complete release-identity control set (2026-08-12)

Every control is marked **configured**, **covered by a dated waiver**, or **open**. Nothing
is marked configured on the strength of a plan.

| # | Control | State | Evidence |
|---|---|---|---|
| 1 | Dedicated signing key, signing-only | **Configured** | Ed25519 `SHA256:Y77mGrK4VudFhkJt+EKyCysSqH6nsp6N4GP0kIPKVTM`, §3ad.1 |
| 2 | Private key encrypted at rest | **Configured** | `aes256-ctr` / `bcrypt` 100 rounds, §3ad.2 |
| 3 | Commit signing | **Configured** | `commit.gpgsign=true`; two commits `verified=true, reason=valid` on GitHub, §3ad.4 |
| 4 | Tag signing | **Configured** | `tag.gpgsign=true`; annotated tag verified, §3ad.4 |
| 5 | Approved-signer register, tracked and public | **Configured** | `governance/allowed-signers`, `namespaces="git"` |
| 6 | Signing key registered with the platform | **Configured** | GitHub signing key id **1108446**, §3ad.3 |
| 7 | Vigilant mode | **Configured — maintainer attestation** | §3ad.5. No API exists; attested, not measured |
| 8 | Fail-closed signed-tag release gate | **Configured** | `.github/workflows/release-tag-verify.yml`, tested in both directions, §3ad.4 |
| 9 | Protected release environment | **Configured** | `release`, 11 properties read back, §3ab |
| 10 | Named release approver | **Configured** | `AhmedAnbar` (id 4220036), §3ab |
| 11 | Deployment restricted to release tags | **Configured** | one policy, `v*`, `type: tag`, §3ab |
| 12 | Administrator bypass disabled | **Configured** | `can_admins_bypass: false`, §3ab |
| 13 | No environment secrets or variables | **Configured** | 0 and 0, §3ab |
| 14 | Build provenance attestation | **Configured** | SLSA provenance v1 over 3 subjects, run 31572250881, §3ac |
| 15 | Software bill of materials | **Configured** | CycloneDX 1.5, attested, §3ac |
| 16 | Artifact checksums | **Configured** | `SHA256SUMS`, attested, §3ac |
| 17 | Evidence retention | **Configured** | 90-day artifact expiry verified; `governance/evidence-retention-policy.md` |
| 18 | No long-lived registry credential | **Configured** | 0 repository secrets, 0 environment secrets, §3z.4 |
| 19 | **Independent four-eyes release approval** | **Waived — W-001** | Single maintainer. Self-review permitted; expiry 2027-02-11 |
| 20 | **Independent review of decision records** | **Waived — W-002** | Single maintainer; expiry 2027-02-11 |
| 21 | Independent encrypted evidence archive | **OPEN — gate fails closed** | Does not exist. Blocks the first crates.io publication, `RELEASING.md` §11 |
| 22 | Authentication-key separation, independently verified | **OPEN — scope limit** | Registered signing-only; `admin:public_key` deliberately not requested, §3ad.3 |

**Nothing in rows 1–18 is claimed on the basis of intent.** Rows 19 and 20 are covered by
dated waivers with absolute expiry dates. Rows 21 and 22 are open and are **not** waived —
row 21 in particular keeps the first-publication gate closed.

**The set is complete for Phase 001, which publishes nothing.** It is *not* sufficient for a
first release: row 21 must be closed first.

## 3af. Workspace reorganisation, repository reconciliation, DNS and TLS (2026-08-12)

> **Absolute local paths retained here, deliberately** *(noted 2026-08-15)*. The signing-key
> and backup paths elsewhere in this ledger were withheld on the same date, because a path
> proved nothing those records did not already prove. **This section is the exception**:
> §3af.3a diagnoses a stale compiled-in `CARGO_MANIFEST_DIR`, and the finding **is** the
> before-and-after comparison of two absolute paths. Redacting them would delete the evidence
> rather than protect anything. The paths here disclose a working-directory layout and a
> macOS account name — **no key, credential, or backup location** — and the account name is
> already public in this project's commit metadata. **Whether to generalise them anyway is
> recorded as an open question for the maintainer, not silently decided here.**

**Nothing was deployed, published, released, or provisioned.** No GitHub repository was
created, renamed, or made public. No Cloudflare record was created or changed. No
certificate was issued. No Kubernetes object was touched.

### 3af.1 The three private repositories already exist

**Created manually by Ahmed Anbar, outside any implementation run. This session did not
create them, and does not claim to have.**

| Repository | Visibility | Size | Branches | Commits |
|---|---|---|---|---|
| `renvor-rs/renvor` | **public** | 769 KB | 2 | 1+ |
| `renvor-rs/renvor-site` | **private** | 0 KB | 0 | **0 — empty** |
| `renvor-rs/renvor-docs` | **private** | 0 KB | 0 | **0 — empty** |
| `renvor-rs/renvor-infra` | **private** | 0 KB | 0 | **0 — empty** |

`renvor-site` and `renvor-infra` **supersede the earlier proposed names** `renvor-landing`
and `renvor-deploy`. Forward-looking references were updated; dated historical observations
were preserved and labelled rather than rewritten.

Local checkouts were created by cloning. **GitHub state was not modified**: no push, no
commit, no branch, no README, no licence, no workflow, no settings change. All three
remained empty after the clones, verified by API.

### 3af.2 Local workspace reorganisation

`/Users/ahmedanbar/Documents/renvor` is **no longer a Git repository**; it is a plain
container directory. Same-filesystem atomic renames were used throughout — no large tree was
copied and deleted.

| Path | Repository | Remote |
|---|---|---|
| `framework/` | yes | `git@github.com:renvor-rs/renvor.git` |
| `site/` | yes | `git@github.com:renvor-rs/renvor-site.git` |
| `branding/` | **no — deliberately not a repository, and contains none** | — |
| `docs/` | yes, **empty** | `git@github.com:renvor-rs/renvor-docs.git` |
| `infra/` | yes, **empty** | `git@github.com:renvor-rs/renvor-infra.git` |

> **Layout corrected 2026-08-12 (later the same day).** The V7 landing checkout was
> initially placed at `branding/landing-v7`; it was subsequently moved to top-level `site/`
> by atomic rename, so that every repository is a direct child of the workspace root and
> `branding/` contains no repository at all. The remote, the absence of commits, and the
> empty remote were all re-verified after the move, and `pnpm` typecheck and production
> build both pass from the new path. The pre-migration backup deliberately stayed at
> `branding/.migration-backup/landing-v7-original`, because it archives what V7 looked like
> before migration rather than forming part of the live site.

**Framework integrity across the move**, verified against a pre-migration manifest:

| Check | Result |
|---|---|
| `git rev-parse --show-toplevel` | `/Users/ahmedanbar/Documents/renvor/framework` |
| HEAD before and after | `2183b246ef3bbfca69b8ec92e2683a8125a5b7d2` — **unchanged** |
| Branch | `main` |
| Origin URL | preserved |
| Tracked files | **85 of 85 byte-identical** (index blob hashes and modes compared) |
| Worktree status | clean, 0 entries |

**Branding preserved exactly**: **223,389 files, 2,150,116,984 bytes** before and after,
across all 14 versions. `Branding` → `branding` used a unique intermediate name, because the
filesystem is **case-insensitive** — confirmed by `Branding` and `branding` resolving to the
same inode — so a direct case-only rename was avoided.

**Recovery artefacts** in the maintainer's local backup directory, under `renvor-migration-backup-20260812T095519Z/` *(absolute path withheld 2026-08-15)*:

| Artefact | SHA-256 |
|---|---|
| `framework-git.tar.gz` | `293e8988de4fd993274e3a92fa465cd2a9d0ce850c344c1b488e302163fdafca` |
| `framework-tracked-objects.txt` | `4d32d96c7a98f469973cf88a205b8e1fb6ecd83bc407d950329da541fe7ca992` |
| `landing-v7-source.sha256` | `d5cadc57b63b0f954dcb3eedcb428bdd1fd1799f6a7a85dd94659263502631b9` |
| `branding-full-manifest.txt` | `27b6a2f4ec4e70d804cc1c184e2bd121d6fa0a2d01a98c432b4b245d1352f44d` |

The backup was **restore-tested before the first move**: extracted to a fresh directory,
`git fsck --full --strict` clean, HEAD, `main`, `origin/main`, and the origin URL all
verified. No prune, garbage collection, reflog expiry, or history rewrite was performed.

### 3af.3 Landing V7

The complete original is preserved untouched at
`branding/.migration-backup/landing-v7-original` — **33,362 files**. **It must not be
deleted** until the site migration is independently validated and its deletion explicitly
approved.

**Exactly 14 production-source files** were copied into the checkout, each verified
**byte-identical** against the pre-migration SHA-256 manifest. Deliberately excluded:
`node_modules/` (33,307 files), `build/` (20), `.docusaurus/` (15), and `inspection/`
(6 screenshots). No credentials, environment files, editor state, local tooling state, PDFs,
or brand collateral entered the checkout.

**Validation on its own Node 24 toolchain** (`node v24.19.0`, `pnpm 11.21.0`; the framework
pins Node 22 — the two must not be conflated):

| Check | Result |
|---|---|
| Frozen install (`pnpm install --frozen-lockfile`) | **pass**, 5.4 s |
| Strict typecheck (`tsc --noEmit`) | **pass**, exit 0 |
| Production build | **pass**, 20 files generated |
| Desktop 1440×900 horizontal overflow | **0 px** |
| Mobile 390×844 horizontal overflow | **0 px** |
| Light / dark themes | both honoured — `rgb(246,248,252)` / `rgb(12,25,48)` |
| Images with `alt` | 3 of 3 |
| Buttons without accessible name | 0 |
| `lang` attribute | `en` |

**Reduced motion, measured rather than assumed:**

| `prefers-reduced-motion` | media query matches | ScrollTrigger pin-spacers | elements with inline animation |
|---|---|---|---|
| `no-preference` | false | 1 | 43 |
| **`reduce`** | **true** | **0** | **0** |

GSAP's `matchMedia` gating and its cleanup both work: under reduced motion no animation is
applied and no ScrollTrigger is created.

**Landing V7 remains uncommitted**: 0 local commits, 0 staged files, remote 0 refs / 0 KB.
It stays that way until T095–T098 pass.

**Two files do not exist and were not invented**: `.nvmrc` (already tracked by **T100**) and
`.gitignore`. `package.json` declares `engines.node >= 24.0` and
`packageManager: pnpm@11.21.0`. A `.gitignore` must be written before the first commit.

### 3af.3a A relocation hazard the move exposed

**`cargo xtask verify` failed at step 2 immediately after the move**, reporting
`could not find Cargo.toml in /Users/ahmedanbar/Documents/renvor` — the new *container*
directory, not the framework root.

Cause: `xtask` derives the workspace root from `env!("CARGO_MANIFEST_DIR")`, which is baked
in **at compile time**. The cached binary in `target/` still carried
`/Users/ahmedanbar/Documents/renvor/xtask` from before the move, whose parent is now the
container directory. Cargo did not consider the binary stale, because no source file had
changed.

```text
strings target/debug/xtask | grep /Users/...
  before rebuild: /Users/ahmedanbar/Documents/renvor/xtask
  after  rebuild: /Users/ahmedanbar/Documents/renvor/framework/xtask
```

Resolved with `cargo clean -p xtask` followed by a rebuild. **The source is not defective**
— `env!("CARGO_MANIFEST_DIR")` is the documented way to do this and is correct whenever the
binary matches the tree it was built in. CI is unaffected, because every run builds from a
fresh checkout.

Recorded because the failure mode is misleading rather than obvious: it names a path the
operator never chose, and it survives a `cargo build` that reports success. **Anyone
relocating this repository must rebuild `xtask` before trusting a verification result.**

### 3af.4 DNS — verified against both authoritative nameservers

**Records created manually by Ahmed Anbar. This session did not create or modify them.**

Verified **2026-08-12T10:04:01Z** directly against `coco.ns.cloudflare.com` and
`earl.ns.cloudflare.com`, not via a recursive resolver:

| Record | Value | coco | earl |
|---|---|---|---|
| `renvor.dev` A | `153.92.208.119` | ✅ | ✅ |
| `docs.renvor.dev` A | `153.92.208.119` | ✅ | ✅ |
| `www.renvor.dev` A | `153.92.208.119` | ✅ | ✅ |
| AAAA / CNAME on all three | none | ✅ | ✅ |
| `*.renvor.dev` A / AAAA / CNAME | **absent** | ✅ | ✅ |

**Wildcard absence proven by random-hostname probe**, not by absence of a record lookup:
`zz-39ee9a547b17.renvor.dev` and `qq-98484e48d3ee.renvor.dev` — two freshly generated names,
never previously queried — returned **no answer on both authoritative nameservers**.

> **An earlier check on 2026-08-12T09:21:57Z found a wildcard `*.renvor.dev A 153.92.208.119`
> present**, and that run stopped without editing any file. The maintainer subsequently
> removed it. The earlier finding is recorded rather than discarded, because it is the
> reason the precondition existed.

**Proxy state: DNS-only (grey cloud)**, confirmed structurally — the authoritative answer is
the origin IP itself, not a Cloudflare edge address.

**This state is temporary.** It exists so cert-manager can complete HTTP-01 validation and
issue publicly trusted Let's Encrypt origin certificates. A Cloudflare Origin CA certificate
is **not** usable while records are DNS-only, because browsers reach the origin directly and
would receive a certificate no public root trusts.

### 3af.5 TLS — origin is not ready

Inspected read-only via SNI against `153.92.208.119:443`. **Identical certificate for all
three hostnames:**

| Field | Value |
|---|---|
| Subject | `CN=TRAEFIK DEFAULT CERT` |
| Issuer | `CN=TRAEFIK DEFAULT CERT` (**self-signed**) |
| Valid from | 2026-08-10 05:09:18 GMT |
| Valid to | 2027-08-10 05:09:18 GMT |
| SAN | `DNS:d1207dec8680e832079d94399df82634.db6371138054b2788704127dcfbf3720.traefik.default` |
| Verify | **18 (self-signed certificate)** |

**No hostname certificate exists.** This is truthful evidence that origin TLS is not ready,
and **no deployment, certificate, Cloudflare proxy, Full (strict), or origin-authentication
gate is marked complete.**

The final intended architecture is unchanged:

```text
Visitor → Cloudflare proxied edge → Full (strict) → Authenticated Origin Pulls → Traefik → Renvor service
```

A Let's Encrypt certificate on the VPS does **not** replace Cloudflare proxying; it supplies
the valid origin certificate that Full (strict) requires.

### 3af.6 T105 — the `www` redirect decision

**Decision: Cloudflare serves the permanent `www.renvor.dev` → `https://renvor.dev`
redirect, HTTP 301, preserving path and query string.**

Rationale: the redirect must work for visitors who never reach the origin, and it should not
consume an origin certificate, an Ingress, or a Traefik router to answer a request whose only
purpose is to be redirected. Cloudflare answers it at the edge before the origin is involved.

Consequences:

- `www.renvor.dev` needs a **proxied** record for the rule to apply. It is DNS-only today, so
  **the redirect cannot function yet** and the rule is deliberately **not created**.
- Until then `www.renvor.dev` resolves to the origin and receives the Traefik default
  certificate — a browser will show a certificate warning. Expected in the temporary state.
- Traefik needs **no** `www` router, and no certificate is issued for `www` at the origin.
- If Cloudflare proxying is ever disabled, **the redirect stops working**; a Traefik fallback
  would then be required, and that is a deliberate trade recorded here rather than discovered
  during an outage.

**The rule was not created in this pass.**

## 3ag. Brand mark published, and the T098 gate deliberately overridden (2026-08-12)

**This records a maintainer decision to act ahead of an open gate. It is an override, not a
gate closure, and T098 remains open.**

### 3ag.1 What was done

`assets/renvor-mark-v7.svg` was added to the **public** framework repository and is
displayed at the top of `README.md`. The same mark now appears in the READMEs of
`renvor-site`, `renvor-docs`, and `renvor-infra`.

### 3ag.2 What it conflicts with

| Authority | Statement |
|---|---|
| **ADR-0005** (accepted) rejected alternative | Placing brand assets in the public framework repository *"puts brand assets under a repository that declares `MIT OR Apache-2.0`, **making an unintended licensing claim**"* |
| **ADR-0005** consequences | *"Brand assets in `Branding/brand-v7` are **not** covered by the framework's `MIT OR Apache-2.0` grant"* |
| **T098** | Blocks *"any public use of the site or its brand assets"* |

Before this change the framework repository shipped **no** brand mark — only
`docs/static/img/favicon.ico`. This is therefore new public exposure of an asset whose
licence terms are undecided.

### 3ag.3 The decision, and the compensating control

**Maintainer decision, Ahmed Anbar, 2026-08-12: proceed, with an explicit exclusion notice.**
The conflict was raised in full before the change was made, with the alternative of deciding
T098 first offered and declined in favour of proceeding.

The compensating control is a **brand-mark notice in `README.md`** stating that the mark is
not covered by `MIT OR Apache-2.0`, that no trademark or brand licence is implied, that
usage terms are undecided and tracked at T098, and that the mark should be treated as all
rights reserved meanwhile.

**This addresses ADR-0005's stated reasoning without closing its conclusion.** The risk that
record names is an *unintended* licensing claim; an explicit exclusion makes the claim
intended and bounded. It does not decide the usage terms, and it does not satisfy T098.

### 3ag.4 What is still open

- **T098 remains open.** The website-code licence and the brand-asset usage terms are still
  undecided. This entry does not close it and must not be cited as closing it.
- **ADR-0005 is not amended.** Its rejected-alternative analysis stands; this is a scoped
  exception to it, recorded here rather than by quietly editing the record.
- The correct resolution remains a decision record fixing the licence and usage terms, after
  which this override should be replaced by an ordinary, licensed use of the mark.

## 3ah. `renvor-site` first content commit (2026-08-12)

**The first content commit and push to `renvor-site` occurred with T095, T096, T097, and
T098 all open.** Authorised explicitly by the maintainer after those four gates were
identified and quoted.

| | |
|---|---|
| Repository | `renvor-rs/renvor-site` (**private**) |
| Contents | 14 production-source files plus `.gitignore` |
| Excluded | `node_modules/`, `build/`, `.docusaurus/`, inspection screenshots, env files, credentials, editor and tool state |
| Deployment | **none** — nothing is built, served, or publicly reachable from this push |

**What this does not mean.** The site is **not** deployed and **not** public. The
release-honesty defects T095–T097 identified are **still present in the source**: the
development-status notice is absent, the `renover new` / `renover add` commands still
reference crates nobody can install, and three CTA destinations still do not resolve. Those
gates now block **deployment** rather than the first commit, and they remain open.

The repository README states the four open gates and that the licence is undecided, so a
reader encountering the source is not misled about its status.

## 4. Acceptance criteria coverage

**Populated 2026-08-15 (T082).** One row per PLAN.md Phase 001 acceptance criterion and per
**SC-001 through SC-016**. **Every row carries an evidence link, a command or action, a
platform, an operator, a date, and a result.** Rows whose result is anything other than a clean
pass say so in the result cell rather than in a footnote.

**Operator `AA` = Ahmed Anbar. Platform `mac` = macOS 26.3 (Darwin 25.3.0, arm64);
`gha` = GitHub Actions `ubuntu-latest`.**

> **These results are self-recorded and reviewed under W-002 and W-003 as a NON-INDEPENDENT
> self-review.** No independent human has verified this table. That is a recorded exception,
> not a claim of independence.

### 4.1 PLAN.md Phase 001 acceptance criteria

| Criterion | Evidence link | Command or action | Platform | Operator | Date | Result |
|---|---|---|---|---|---|---|
| Clean checkout passes formatting, lint, test, and doc placeholders | §3q, §3r | `cargo xtask verify` steps 1–5 | mac + gha | AA | 2026-08-15 | **Pass** — exit 0 on both toolchains |
| Secrets and build output are ignored | §3a, §3c | `.gitignore` review; `git status --porcelain` empty after a full verify | mac | AA | 2026-08-15 | **Pass** — step 10 reports 0 untracked, 0 modified |
| Workflow permissions are minimal | §3v, §3aw.4 | every workflow declares top-level `permissions`; elevation only on the job needing it | gha | AA | 2026-08-15 | **Pass** — framework workflows and `renvor-site`'s `landing-ci` all at `contents: read` with no job-level widening |
| All public names are confirmed | §3g, `governance/name-availability.md` | registry, DNS, and executable-name checks | mac | AA | 2026-08-12 | **Pass**, with limitation **R-1** — names are verified but **unreserved**, and **R-3** — clearance is bounded, not exhaustive |
| No ADR is falsely marked accepted | §3ay, ADR-0006 Acceptance gate | all six ADR headers read; W-002 controls checked per record | mac | AA | 2026-08-15 | **Pass** — 6 of 6 accepted, each with reviewer `Ahmed Anbar — self-review under W-002` and a review date. **Explicitly not independent** |
| Release dry-run workflow packages a placeholder internal crate without publishing | §3z, §3z.4 | `cargo package`, `cargo publish --dry-run` | mac + gha | AA | 2026-08-12 | **Pass** — 1 artifact, **0 publish operations**; registry still 404 for all three names on 2026-08-15 |
| **§26**: Phase 001 records topology, ownership, security boundaries, and the deployment decision process **only** — it provisions nothing and deploys nothing | §3av, §3ay, §3aw | live read-only inspection of GitHub, the shared host, and three hostnames | mac | AA | 2026-08-15 | **Pass** — **no Renvor namespace, workload, PVC, or ingress exists**; no DNS record was created or changed; no site is deployed |

### 4.2 Success criteria SC-001 – SC-016

| Criterion | Evidence link | Command or action | Platform | Operator | Date | Result |
|---|---|---|---|---|---|---|
| **SC-001** — public names dated, definite, under project control; 0 unconfirmed names in frozen references; 0 registry names claimed by publication | §3g, §3ai.4, §3ax.2 | `governance/name-availability.md`; crates.io sparse index with a `serde` control | mac | AA | 2026-08-15 | **Pass** — `renvor`, `renvor-cli`, `renover` all **HTTP 404**; 0 claimed by publication. Residual risk **R-1** |
| **SC-002** — a contributor reaches a passing verification run from a fresh clone with no undocumented steps | §3ba (T085) | quickstart Gates 0–8 from a clean isolated checkout | mac | AA | 2026-08-15 | **Pass** — see §3ba for the gate-by-gate record |
| **SC-003** — verification completes with 0 failing and 0 **silently skipped** checks on both toolchains | §3q, §3r, §3ba | `cargo xtask verify` under 1.94.0 and current stable | mac + gha | AA | 2026-08-15 | **Pass** — 10/10 both toolchains; every step prints an explicit result, so a skip cannot pass silently |
| **SC-004** — after a full verification run on a clean checkout, 0 untracked and 0 modified files | §3r, §3ba | `cargo xtask verify` step 10 | mac + gha | AA | 2026-08-15 | **Pass** |
| **SC-005** — **both** required secret scans report 0 findings, each with its own tool version, date, and scope | §5, §3ba | `gitleaks git` (history) and `gitleaks dir` (tree incl. untracked), 8.30.1 | mac | AA | 2026-08-15 | **Pass** — both **exit 0**, 0 findings, recorded separately. A single clean scan would **not** satisfy this |
| **SC-006** — six governance documents within 1 link of the root; security path findable in under 2 minutes | §3i | link inventory from the rendered README; `SECURITY.md` path test | mac | AA | 2026-08-12 | **Pass** |
| **SC-007** — 100% of workflows declare read-only default permissions; 100% of third-party steps use immutable references; 0 unwaived exceptions | §3v, §3aw.2 | permissions grep; 40-character SHA check on every `uses:` | mac + gha | AA | 2026-08-15 | **Pass** — framework workflows clean; `renvor-site` **9 of 9** actions pinned with version comments |
| **SC-008** — 100% of public-tier security controls enabled and verified before the first content push; control-unavailability waivers **0**; approval waivers exactly **1** | §3l, §3av | protection and security settings read back from GitHub | mac | AA | 2026-08-15 | **Pass** — control-unavailability waivers **0**; approval waivers **1** (W-001). W-002 and W-003 are explicit reviewed exceptions and are **counted separately**, per the waiver ledger |
| **SC-009** — 0 decision records accepted without a recorded reviewer and review date; 100% of required decisions accepted before the phase closes | §3ay, ADR headers | all six ADR header tables read | mac | AA | 2026-08-15 | **Pass** — 6 of 6 accepted with reviewer and date; **0** accepted without both |
| **SC-010** — the rehearsal produces 1 artifact and performs 0 publish operations; the registry shows 0 new versions | §3z.4, §3ax.2 | `cargo package`; registry re-check | mac | AA | 2026-08-15 | **Pass** — 1 artifact, 0 publishes, registry **404** |
| **SC-011** — 100% of PLAN Phase 001 acceptance criteria map to dated evidence; 0 unevidenced | §4.1 above | this table | mac | AA | 2026-08-15 | **Pass** — 7 of 7 criteria evidenced and dated |
| **SC-012** — the documentation set builds and link checking reports 0 broken links | §3ba, verify step 8–9 | `docusaurus build`; `lychee` | mac + gha | AA | 2026-08-15 | **Pass** — build succeeds; **225 OK, 0 errors** over 257 links |
| **SC-013** — 0 runtime framework capabilities implemented, confirmed against the exclusion list | §3ax.1 | rustdoc item enumeration + `pub` grep, two independent methods | mac | AA | 2026-08-15 | **Pass** — 3 public items, all `pub const &str`; 3 lines of library code; 0 of every excluded category |
| **SC-014** — 0 long-lived registry or publishing credentials anywhere; 100% of release-identity controls configured or covered by a dated waiver | §3ad, §3ae | `gh secret list`; workflow grep; signing configuration; `release` environment | mac + gha | AA | 2026-08-15 | **Pass** — no `CARGO_REGISTRY_TOKEN` stored; signing key configured, **signing-only**, passphrase-encrypted; protected `release` environment with required reviewers and **0 deployments**. Key **storage** hardening is an open obligation (§7.4, **R-13**), not a credential leak |
| **SC-015** — 100% of publishable packages declare `MIT OR Apache-2.0`; both licence texts present; 0 unlicensed or divergent | §3h, §3ba | `Cargo.toml` assertion; licence file presence | mac | AA | 2026-08-15 | **Pass** — `renvor` declares `MIT OR Apache-2.0`; both texts present at root and in the crate |
| **SC-016** — MSRV reads exactly `1.94.0` everywhere it is stated, 0 mismatches; resolver 3 declared explicitly; minimum-version resolution demonstrated | §3s, §3ba | single-source MSRV check; `resolver = "3"` in the root manifest | mac + gha | AA | 2026-08-15 | **Pass** — 0 mismatches; resolver 3 explicit. Limitation **R-7**: the floor is proven against a **zero-dependency** crate and must be revalidated before Phase 006 (FR-061) |

**Coverage: 6 of 6 PLAN.md `Acceptance:` criteria, plus 1 additional Phase 001 scope
constraint, and 16 of 16 success criteria — 23 rows, all evidenced and dated. 0 unevidenced
rows.**

> **Why 6 + 1 and not 7.** *(Clarified 2026-08-15.)* `PLAN.md` §20's Phase 001 **`Acceptance:`**
> line contains exactly **six** semicolon-separated criteria, and §4.1's first six rows map to
> them one-to-one and in order. The seventh row covers the **`Web properties (Section 26):`**
> constraint, which sits in the same Phase 001 block under a **different label** and is a scope
> constraint rather than an acceptance criterion. It is covered because it is a real Phase 001
> gate, but counting it as a seventh *acceptance criterion* overstated the source. **This is
> over-coverage, not a gap** — no criterion under the `Acceptance:` label is missing.

## 5. Secret scans

Two scans are required and both must report zero findings (SC-005). A single clean scan
does not satisfy the criterion — the earlier scan predates the content the later one
authorises.

| Scan | Purpose | Tool | Version | Date | Scope | Findings |
|---|---|---|---|---|---|---|
| pre-creation (a) | Gates organization and repository creation | gitleaks `git` | 8.30.1 | 2026-08-11 19:11 | Full history — 2 commits (`1b182d0`..`bfb6925`), 311.63 KB | **0** |
| pre-creation (b) | Gates organization and repository creation | gitleaks `dir` | 8.30.1 | 2026-08-11 19:11 | Working tree — 4.80 MB of text across 122,660 files | **0** |
| pre-push (a) | Gates the first content push | gitleaks `git` | 8.30.1 | 2026-08-11 17:28 | Full history — **all 8 proposed commits** at `ddfc39d`, 1.29 MB | **0** |
| pre-push (b) | Gates the first content push | gitleaks `dir` | 8.30.1 | 2026-08-11 17:28 | Working tree — 6.57 MB of text | **0** |
| convergence (a) | Phase 001 closure re-scan | gitleaks `git` | 8.30.1 | 2026-08-15 | Full history at the convergence head | **0**, exit 0 |
| convergence (b) | Phase 001 closure re-scan | gitleaks `dir` | 8.30.1 | 2026-08-15 | Working tree including untracked files | **0**, exit 0 |

> **The two `pre-push` rows were blank until 2026-08-15, reading "*(not run — T053)*".** That
> was stale, not accurate: **T053 ran on 2026-08-11 and its results were recorded in §3n and
> §3s** — the summary table here was simply never filled in from them. **SC-005 depends on this
> table**, so a blank row here made a satisfied criterion look unevidenced. Populated from §3s,
> the re-scan taken against the exact proposed commits, which is the run that actually gated
> the first content push. *(Found 2026-08-15 by advisory review, which located the blank cells
> by running a detector proven to fire on real blanks.)*
>
> **The canary test accompanies both pre-push rows**: the FP-001 allowlist suppressed exactly
> one prose match and **detected** an injected canary credential, with the test file restored
> byte-identically. An allowlist that had silently widened would have failed this.

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

## 3ai. Edge-architecture correction and landing-page release honesty (2026-08-12)

**Nothing was committed, pushed, deployed, published, or changed on any external service in
this pass.** DNS, the VPS, Kubernetes, certificates, registries, and repository settings were
read only.

### 3ai.1 Workspace topology — verified, matches record

| Check | Result |
|---|---|
| Workspace root is **not** a Git repository | ✅ no `.git`; `git rev-parse` reports "not a git repository" |
| Four independent repositories | ✅ `framework/`, `site/`, `docs/`, `infra/` each hold a **directory** `.git`, not a gitdir file |
| Submodules | ✅ none — no `.gitmodules` anywhere in the workspace |
| Symbolic links | ✅ none found to depth 4, excluding `node_modules` and `.git` |
| `branding/` outside every repository | ✅ 2.6 GB local archive, no `.git` |
| Migration backup intact | ✅ `branding/.migration-backup/landing-v7-original/` — 399 MB, `src/`, `static/`, `PRODUCT.md`, `docusaurus.config.ts`, lockfile all present |
| `branding/landing-v7/` absent | ✅ correctly absent — moved to `site/`, not moved again |
| Generated dependencies or tool state tracked | ✅ **zero** matches for `node_modules`, `target/`, `.docusaurus`, `build/`, `.venv`, `.claude/`, `.specify/`, `.remember/`, `.idea/`, `.DS_Store` in `git ls-files` for `framework` or `site` |

| Repository | Remote | Branch | HEAD | Commits | Status |
|---|---|---|---|---|---|
| `framework/` | `renvor-rs/renvor` (public) | `main` | `ffb322f` | 21 | in sync with `origin/main` (0 ahead, 0 behind) |
| `site/` | `renvor-rs/renvor-site` (private) | `main` | `8b68e32` | 1 | in sync, 15 tracked files |
| `docs/` | `renvor-rs/renvor-docs` (private) | `main` | *(no commits)* | 0 | untracked `README.md`, `assets/renvor-mark-v7.svg` |
| `infra/` | `renvor-rs/renvor-infra` (private) | `main` | *(no commits)* | 0 | untracked `README.md`, `assets/renvor-mark-v7.svg` |

`docs/` and `infra/` are therefore accurately described as **initialised repositories with
local, uncommitted starter files** — not as empty directories and not as pushed repositories.

### 3ai.2 Authoritative DNS — re-verified read-only

Queried directly against the authoritative nameservers. **Screenshots were not used.**

| Item | Result |
|---|---|
| Authoritative NS | `coco.ns.cloudflare.com`, `earl.ns.cloudflare.com` |
| `renvor.dev` A | `153.92.208.119` |
| `docs.renvor.dev` A | `153.92.208.119` |
| `www.renvor.dev` A | `153.92.208.119` |
| `AAAA` on all three | none |
| Proxy state | **DNS-only** — the authoritative answer is the origin IP, not a Cloudflare anycast address |
| Wildcard `*.renvor.dev` | **absent** — proven by two freshly generated random hostnames (`zz9x4k7q2m`, `zz3v8n1p6t`) returning no answer, not by a missing lookup |
| `CAA` | **absent** — nothing currently constrains which CA may issue for this domain |

### 3ai.3 Origin TLS — unchanged, still not ready

Inspected read-only by SNI against `153.92.208.119:443`. **Identical self-signed certificate
for all three hostnames**: subject and issuer `CN=TRAEFIK DEFAULT CERT`, valid 2026-08-10 to
2027-08-10. No hostname certificate exists.

### 3ai.4 Publication status — the framework README was wrong

Checked against the crates.io **registry index**, the path cargo itself reads. The web API
returned HTTP 403 to a scripted request under the crates.io data-access policy, so it is not
evidence either way; the index is.

| Crate | Index response | Meaning |
|---|---|---|
| `renvor` | **HTTP 404** | not published |
| `renvor-cli` | **HTTP 404** | not published |
| `serde` *(control)* | HTTP 200 | the index and the method work |

**`framework/README.md` claimed "The published `renvor` crate exposes three constants".** That
statement was false — nothing is published. It is corrected in this pass. This was found while
auditing the site and is outside the literal scope of the landing-page audit; it is recorded
here rather than left standing, because a verified-false claim in a public README is the same
defect class the release-honesty gate exists to catch.

### 3ai.5 CTA destinations — measured

| Destination | Result | Action |
|---|---|---|
| `https://github.com/renvor-rs/renvor` | **HTTP 200** | kept, and now the only external target |
| `https://docs.renvor.dev/getting-started` | **no response** | removed |
| `https://docs.renvor.dev` | **no response** | removed |
| `https://crates.io/crates/renvor` | 404 by index; web returns 403 to scripts | removed |

### 3ai.6 T110 — the corrective hosting decision

Recorded in full at **T110** and rewritten into ADR-0006 D3, D4, D5, D10, and a new **D11**.
Cloudflare is authoritative DNS only; the proxy stays off; TLS is issued at the origin by the
existing cert-manager; the `www` redirect moves from a Cloudflare rule to Traefik.

**A numbering defect was corrected at the same time.** ADR-0006 contained **two sections both
numbered `D6`** — the delivery decision, and the T105 `www`-redirect decision appended after
the Consequences section. The second is renumbered **D11** and moved into the decision
sequence. D1–D11 now each appear exactly once.

**T105 is preserved, not rewritten.** It recorded a correct decision under the architecture
then in force, and its Cloudflare-versus-Traefik comparison is carried into D11 so the cost of
the reversal is visible: the redirect now needs its own certificate and cannot answer while
the origin is down — the two reasons Cloudflare was originally chosen.

**ADR-0006 remains `proposed`.** T099, T101, and T106 are unresolved.

### 3ai.7 Landing-page release honesty — what was false, and what it now says

| Claim as published | Verified state | Correction |
|---|---|---|
| "Renvor **4.0 production release**" (hero) | no release exists | "In development — nothing released" |
| "Renvor **4.0 stable**" (closing section) | no release exists | "In development — no release" |
| `renover new` / `renover new commerce` | CLI unbuilt, unpublished | struck through, labelled not installable; the hero mock-up carries an explicit "Design mock-up" caption |
| `renover add renvor-rbac` (×2) | no package exists | struck through, labelled not installable |
| 8 package names each tagged **`crates.io`** | none published | tag changed to **`not published`**, with a caveat naming `renvor` and `renvor-cli` as 404 |
| "Generate Next.js, Yew, Dioxus, or Leptos clients… Tauri" | no generator | "The design: …" + **Planned for Renvor 3.0** |
| "Generate secure backend auth flows and matching frontend routes" | none | **Backend planned for 1.0 · screens for 3.0** |
| "Use SQLx or SeaORM… Generated adapters include migrations" | none | "The design: …" + **Planned for Renvor 1.0** |
| "Optional RBAC package" | none | "RBAC package planned for 4.0" |
| "Authentication **is generated** across the stack" | none | "is **designed to** reach across the stack" |
| "Production behavior **is** part of the application shape" | specification only | "is designed in" + "The specification exists; the runtime does not" |
| Meta description asserting present capability | — | states development status first |
| Navbar item **"Docs"** → a section saying docs are undeployed | — | relabelled **"Status"** |

Release labels are **sourced from `PLAN.md`, not invented**: §11.1 REST 1.0, §11.2 GraphQL
2.0, §13.1 backend auth 1.0, §13.2 frontend auth 3.0, §10.2 frontend matrix 3.0, Phase 030
package ecosystem 4.0.

**The development-status notice is deliberately outside every GSAP timeline.** A disclosure
that fades in is absent for the first frames, absent under reduced motion, and absent if
scripting fails. It renders above the hero at full opacity in all four theme/motion
combinations.

### 3ai.8 Site validation — commands and results

Node 24.19.0, pnpm 11.21.0 (both matching `engines.node` ≥ 24 and `packageManager`).

| Command | Result |
|---|---|
| `pnpm install --frozen-lockfile` | **exit 0** — "Already up to date" |
| `pnpm run typecheck` (`tsc --noEmit`) | **exit 0** |
| `pnpm run build` | **exit 0** — `onBrokenLinks: 'throw'` and `onBrokenAnchors: 'throw'` are in force, so every internal anchor resolved |

Behavioural checks against the production build, served on a **verified-free** port 4891:

| Check | Result |
|---|---|
| Desktop 1440×900 horizontal overflow | **0 px** |
| Mobile 390×844 horizontal overflow | **0 px** |
| Status notice within first viewport, both widths | ✅ (358 px wide inside a 390 px viewport) |
| Images with `alt` | 3 of 3 |
| Keyboard tab stops | **21**, **0** without an accessible name, **0** without a visible focus ring |
| Tablist arrow keys | `ArrowRight` → Data (selected), `End` → Delivery, `Home` → Backend |
| Evaluation-lens button via `Enter` | content changed ✅ |
| External CTAs | 7, **all** to `github.com/renvor-rs/renvor` (HTTP 200) |
| Surviving references to `docs.renvor.dev`, `crates.io/crates`, "4.0 production release", "4.0 stable" | **0 each**, in rendered HTML |

**Reduced motion**, measured rather than asserted:

| | `no-preference` | `reduce` |
|---|---|---|
| Pin spacers | 1 | **0** |
| Elements with inline `transform` | 17 | **0** |
| Elements with inline `opacity` | 36 | **0** |
| Marquee `transform` | animated | **`none`** |
| Status-notice opacity | 1 | **1** |

**Contrast**, measured from **rendered pixels** rather than computed styles — the element's ink
was hidden, the backdrop screenshotted, and the 5th/95th percentile luminance used so
gradients and decorative bars are covered and antialiasing outliers are not:

| Theme | Elements checked | Failures below 4.5:1 | Worst |
|---|---|---|---|
| Light | 31 | **0** | 4.56:1 |
| Dark | 31 | **0** | 4.56:1 |

Four contrast defects were **introduced and then fixed inside this pass**, all from the same
mistake — using the theme token `var(--muted)` on surfaces that keep a fixed appearance in
both themes:

| Surface | Defect | Fix |
|---|---|---|
| `.solutionSideA` (blue→violet gradient, white text, fixed) | badge and note at **1.27:1** in light | literal `#fff`; measured 4.60:1 at the `#3267ff` stop, 5.55:1 at `#7048e8` |
| `.authStarter` (dark card in both themes) | badge at **2.54:1** in light | literal `#fff` |
| `.docsSection` (dark, with bright decorative bars behind the text) | caveat at **3.35:1** against the brightest sampled bar | `#e8eefa` → 4.88:1 |
| `.unpublishedTag` at `opacity: .85`, 9.9 px | **4.04:1** | opacity removed → 5.57:1 |

The `unavailableCommand` style also carried `opacity: .62`, measuring **2.70:1** on the
gradient card. It was removed; the line-through carries the meaning without it. **A disclosure
that is hard to read is not a disclosure.**

### 3ai.9 Framework validation — both toolchains

| Toolchain | Steps 1–9 | Step 10 | Exit |
|---|---|---|---|
| **1.94.0** (declared MSRV) | all **ok** | FAILED — working tree not clean | **3** |
| **stable 1.97.1** | all **ok** | FAILED — working tree not clean | **3** |

Exit 3 is the documented code for "every check passed but the tree is dirty". The four files
it names are exactly this pass's uncommitted governance edits — `PLAN.md`, `README.md`,
`decisions/0006-…md`, `specs/001-governance-foundation/tasks.md`. **This is the correct
outcome**: committing was not authorised, so a clean tree was not achievable, and the gate was
not weakened to produce a green result.

Both runs include **secret scan (history)** and **secret scan (working tree)** — both passed
on both toolchains.

### 3ai.10 Secret scans — all four repositories

`gitleaks` 8.30.1.

| Repository | Scope | Result |
|---|---|---|
| `framework/` | history + working tree, via `cargo xtask verify` step 7 | **no leaks** (both toolchains) |
| `site/` | history (1 commit, 519 KB) | **no leaks**, exit 0 |
| `site/` | working tree (888 KB) | **no leaks**, exit 0 |
| `docs/` | working tree (no commits exist) | **no leaks**, exit 0 |
| `infra/` | working tree (no commits exist) | **no leaks**, exit 0 |

**Prohibited-attribution scan** over all twelve changed files: three files matched a broad
pattern, and all three are false positives of exactly the kind the constitution names —
`codexhub` is a real Kubernetes namespace and live site on the shared host, and `.claude/` in
`site/.gitignore` is a rule that *prevents* local agent state being committed. **No authorship
attribution was found or added.**

## 3aj. Open decisions — research for the maintainer, deliberately not decided (2026-08-12)

**None of the decisions below is made here.** Each is presented with the evidence needed to
rule on it, and each remains open.

### 3aj.1 T098 — website-code licence and brand-asset terms

Two distinct things need separate answers, and conflating them is the trap: **the website
source code** and **the brand assets** (`renvor-mark-v7.svg`, the favicon, the dark variant).

| Model | Website code | Brand assets | Consequence |
|---|---|---|---|
| **A — Match the framework** | `MIT OR Apache-2.0` | same | Simplest and consistent. **Grants everyone the right to use the Renvor mark**, including to brand a fork or a competing product. ADR-0005 identified exactly this as the risk to avoid |
| **B — Split (recommended)** | `MIT OR Apache-2.0` | **all rights reserved**, with a short written trademark-style usage policy | Code stays reusable; the mark stays controlled. This is what the framework README already asserts as an interim state, so choosing it ratifies current practice rather than changing it. Costs one short policy document |
| **C — Source-available site, reserved marks** | proprietary / no licence | all rights reserved | Maximum control. The repository is private, so this changes little today, and it forecloses reusing site components in the docs site later without a second decision |

**Recommendation: B.** It is the only option that separates the two questions, and the
framework README is already written as though B were chosen — leaving it undecided is what
creates the inconsistency, not choosing it.

**Precedent worth checking before ruling**: Rust itself, and most Rust-ecosystem projects,
dual-licence code while reserving the name and logo under a separate trademark policy. B is
the ecosystem-conventional answer, not a novel one.

### 3aj.2 T099 — container registry

| | **GHCR** (`ghcr.io`) | **GitLab registry already on the VPS** |
|---|---|---|
| Private source, public image | Supported — package visibility is independent of repository visibility | Supported |
| Kubernetes pull credential | `imagePullSecret` from a token; GHCR supports repository-scoped tokens | `imagePullSecret` from a deploy token scoped to one project |
| **Token lifetime** | **Classic PATs can be long-lived — the risk.** GitHub Actions can instead use a per-run `GITHUB_TOKEN` for *publishing*, so no long-lived push credential need exist | Deploy tokens support an expiry date, but **pull still needs a stored credential** |
| Least-privilege publishing | **Strongest**: `packages: write` on the job only, OIDC-based, no stored push secret | Requires a stored GitLab credential in GitHub Actions — a cross-system long-lived secret |
| Digest pinning | Native | Native |
| **Failure coupling** | External to the VPS; a VPS outage does not prevent publishing | **Couples Renvor to another project's service on the same host.** A GitLab outage or migration breaks Renvor deploys, and GitLab is a Docker workload outside k3s |
| Availability during a cluster rebuild | Independent | **Registry lives on the machine being rebuilt** — a chicken-and-egg problem during recovery |
| Cost / quota | Free for public images; private images count against storage quota | Local disk (328 GB free) |

**The decisive asymmetry is the publishing credential.** GHCR with GitHub Actions OIDC needs
**no stored push secret at all**; the GitLab registry requires a long-lived cross-system
credential held in GitHub. That is the same class of risk the release process already went to
some trouble to avoid for crates.io.

**The recovery argument is the second one**: pulling images from a registry that lives on the
host you are trying to restore is a dependency loop. ADR-0006 D9 rests on "recovery is
redeploying a known digest from a private registry" — which only holds if the registry is not
on the failed machine.

**Nothing was created.** No package, no token, no credential.

### 3aj.3 T106 — backup design

> **The read-only server inspection for this pass could not be completed, and the design below
> therefore rests on the recorded 2026-08-11 audit rather than fresh observation.**
>
> Port 2022 on the origin is **reachable** — the failure was **authentication**, not
> connectivity. The configured SSH profile targets user `deploy`, while `~/.ssh/config` maps
> the host to user `root` with a different identity file. Resolving that means changing
> credential configuration for a live production host shared with five unrelated namespaces,
> which is outside a read-only pass and was not authorised. **No workaround was attempted.**
>
> **T102 already requires the server facts to be re-verified immediately before any
> deployment.** This design must be re-checked at that point regardless.

What needs protecting, and why each is different:

| Asset | Where it lives | If lost |
|---|---|---|
| **k3s datastore** | `/var/lib/rancher/k3s/server/db/state.db` — **SQLite, not etcd** | All cluster state for **five production namespaces**. This is the single highest-value file on the host |
| **Kubernetes manifests** | `renvor-infra` (for Renvor) | Recoverable from Git — for Renvor. **Neighbouring workloads have no such repository**, so their manifests exist only inside `state.db` |
| **Secrets** | Cluster secrets inside `state.db` | Not in Git by policy. Losing them means re-issuing every credential on the host |
| **Certificates** | cert-manager `Secret` objects inside `state.db` | Recoverable by re-issuing via ACME, **provided** DNS and rate limits allow. Let's Encrypt duplicate-certificate limits make a mass re-issue slow |

**The design, stated as a proposal only — nothing was installed or configured:**

1. **`state.db` is the whole job.** Because k3s here uses SQLite, `k3s etcd-snapshot` does not
   apply. The correct primitive is SQLite's **online backup** (`sqlite3 .backup` or
   `VACUUM INTO`), which is consistent under concurrent writes. **A plain `cp` of a live
   SQLite file can capture a torn write and restore into a corrupt database** — this is the
   most likely way a naive backup silently fails.
2. **Stop-free, but verify.** Every snapshot should be integrity-checked
   (`PRAGMA integrity_check`) *at creation*, and the result recorded. A backup that has never
   been read is a hypothesis.
3. **Off-host, encrypted, versioned.** The backup must not live on the disk it protects.
   Encrypt before it leaves the host, because it contains every cluster secret.
4. **Retention with at least one monthly.** Corruption and accidental deletion are often
   discovered late; a 7-day window is not enough for a shared host.
5. **A declarative export alongside the binary one.** A periodic
   `kubectl get -o yaml --all-namespaces` export, **with secret *values* excluded**, is
   human-readable, diffable, and lets a single object be restored without a full datastore
   rollback. It is a complement to the datastore backup, never a replacement.
6. **The restore test is the deliverable, not the backup.** Restore into a **throwaway
   environment**, never the production host: stand up a scratch k3s, restore the snapshot,
   confirm the API server starts and expected objects exist. **Record the date of the last
   successful restore.** An untested backup is not a backup, and the shared-host context makes
   an in-place restore test unacceptable.

**Two scope facts the maintainer should weigh, unchanged from ADR-0006 D9**: the Renvor
properties are **stateless**, so this gap does not block Renvor specifically; and the gap is
**pre-existing and affects unrelated production workloads**, so it is outside Renvor's remit
to fix unilaterally. The ruling being requested is whether it blocks Renvor deployment.

### 3aj.4 T108 — `image-size`, researched against primary sources

**The situation changed materially, and in a way that closes one of T108's three exit paths
permanently.**

| Fact | Source | Value |
|---|---|---|
| `GHSA-w3rx-r6r6-pgpr` — ICNS parser infinite loop | GitHub Advisory API | high, CVSS 7.5, affected `<= 2.0.2`, **first patched version: NONE** |
| `GHSA-5p2g-fcmc-qvqq` — JXL and HEIF parser infinite loops | GitHub Advisory API | high, CVSS 7.5, affected `<= 2.0.2`, **first patched version: NONE** |
| Advisories published / last updated | GitHub Advisory API | 2026-06-10 / **2026-08-07** |
| `image-size` latest published version | npm registry | **2.0.2**, published **2025-04-02** — no release in ~16 months |
| **`image-size` upstream repository** | GitHub API | **`archived: true`** |
| Last code commit | GitHub API | 2025-04-02 (the 2.0.2 release itself) |
| Installed path | `npm ls` | `@docusaurus/core@3.10.2 → @docusaurus/mdx-loader@3.10.2 → image-size@2.0.2` |
| `@docusaurus/core` latest | npm registry | **3.10.2** — the version already installed. **No newer release exists** |

**The upstream repository is archived, by the maintainer's own stated decision.** The README
notice reads: *"Archiving this repo, because I don't want to deal with the same LLM generated
'security advisory' about an infinite loop over and over again."* They add that the project
may be revived **on Codeberg**, and that *"This repo on github will not be updated."*

Two things follow, and they point in opposite directions:

- **"Wait for an upstream fix" is no longer a strategy.** It is not pending; it is closed.
- **The maintainer disputes the advisories' validity**, characterising them as LLM-generated.
  That is not proof either way, but it is a primary-source signal that the severity may be
  contested rather than settled, and it is recorded here rather than filtered out.

**Docusaurus is actively working the problem, but has not shipped it:**

| | |
|---|---|
| Issue [#12231](https://github.com/facebook/docusaurus/issues/12231) "Replace unmaintained dependency image-size with active CVEs (CVSS 7.5)" | **open**, `status: needs triage`, updated **2026-08-12** |
| PR [#12235](https://github.com/facebook/docusaurus/pull/12235) — replaces `image-size` with `image-dimensions` | **open, DRAFT, unmerged**, labelled **`pr: breaking change`**, **no milestone** |
| PR [#12234](https://github.com/facebook/docusaurus/pull/12234) — earlier attempt | closed |

**That it is labelled a breaking change with no milestone is the important part**: it points
at a Docusaurus **4.0**, not a 3.x patch. Planning around "the next patch release" would be
planning around something that is not scheduled.

**Reachability in this project, re-verified:** the documentation source contains **zero image
embeds** — no `![…](…)` and no `<img>` across all six `.mdx` files and the README. `static/`
holds only `.nojekyll` and `favicon.ico`, which are copied verbatim and never parsed by
`mdx-loader`. The vulnerable parsers are **not reached**, and the site takes no untrusted
image input.

The safe choices, with consequences:

| Option | Consequence |
|---|---|
| **1. Hold, keep the deployment gate closed** *(status quo)* | Honest and safe. But the gate is now indefinite, because the fix depends on an unscheduled Docusaurus major |
| **2. Hold, but re-scope the gate to what is actually at risk** | The advisory is DoS-only, build-time-only, and unreachable with zero image embeds. A gate could be **"no MDX image embeds may be added while this is open"** — mechanically checkable in CI — which lets documentation deploy while keeping the risk genuinely at zero. **This is the strongest option and needs a maintainer ruling, not a unilateral change** |
| **3. Override to `image-dimensions` ahead of upstream** | Follows Docusaurus's own chosen direction, but PR #12235 is a *draft breaking change*; overriding a transitive dependency across an API boundary that upstream is still designing is how a build breaks subtly |
| **4. Wait for Codeberg revival** | Speculative. No timeline, no published package |

**Not done, per instruction and policy**: Docusaurus was not downgraded, `npm audit fix
--force` was not run, the advisory was not suppressed, no claim was made that a fix exists,
and the documentation site was **not deployed**.

### 3aj.5 T109 — `uuid`

Unchanged, and re-stated so it is not silently carried: `GHSA-w5hq-g745-h8pq` (moderate,
`uuid` < 11.1.1) remains **not reachable** — `sockjs` calls `uuid.v4()` with no `buf`
argument, while the advisory affects v3/v5/v6 **with** `buf`; and `sockjs` arrives via
`webpack-dev-server`, which runs only for `docusaurus start` and never in a production build.
`sockjs` 0.3.24 remains the latest release and still pins `uuid ^8.3.2`, so no compatible
update exists.

**No three-major override was forced into a path CI never exercises.** Reassessment remains
due **2026-09-11**, or immediately if `sockjs` ships a fix or the dev server enters a deployed
path.

## 3ak. T098 — website-code licence and brand-asset terms, DECIDED (2026-08-12)

**Maintainer decision: option B — split the two questions**, because they are two questions.

| | Decision |
|---|---|
| Website source code | **`MIT OR Apache-2.0`**, at the recipient's option — the same terms as the framework |
| Renvor names, logos, marks, illustrations, brand assets | **All rights reserved**, under a written brand-usage policy |

**The code licences grant no trademark or brand-identity rights, and no document may imply
they do.** Apache-2.0 withholds trademark rights explicitly in its section 6; MIT is silent on
trademarks rather than granting them. This is the conventional open-source separation — Rust,
Python, and Linux all draw the same line — not a restriction invented here.

**Permitted without asking**: truthful nominative reference ("built with Renvor", "compatible
with Renvor"), links to the official project, the unmodified mark used as a link back,
screenshots, tutorials, reviews and comparisons **including unfavourable ones**, academic and
journalistic use, and community discussion. The policy states plainly that criticism needs no
permission and will not be refused — a brand policy usable to suppress a bad review would be a
bad policy.

**Prior permission required**: naming a fork or derivative "Renvor" or something confusingly
similar, confusingly similar logos or wordmarks, endorsement/partnership/certification claims,
merchandise, modifying the marks, domain and app-store names implying official status, and
company or product names incorporating Renvor.

**Published file set — validated before this task was marked complete:**

| File | Check |
|---|---|
| `site/LICENSE-MIT` | present, **SHA-256 byte-identical** to `framework/LICENSE-MIT` (`13f14c1f…`) |
| `site/LICENSE-APACHE` | present, **SHA-256 byte-identical** to `framework/LICENSE-APACHE` (`cfc7749b…`) |
| `site/BRAND-POLICY.md` | present, v1.0 dated 2026-08-12 |
| `site/README.md` | licence section rewritten from "Undecided — see T098" |
| `.gitignore` exclusion | **none of the four is ignored** — `git check-ignore` clean on all |
| Internal link targets | `LICENSE-MIT`, `LICENSE-APACHE`, `BRAND-POLICY.md`, `static/img/renvor-mark-v7.svg` — **all resolve** |
| Brand assets named in the policy | `renvor-mark-v7.svg`, `renvor-mark-v7-dark.svg`, `renvor-favicon-v7.svg` — **all present** |

References updated consistently in `framework/README.md` (the interim "terms have not been
decided yet" notice replaced with the decided terms), `docs/README.md`, and `infra/README.md`.

> **`docs/` and `infra/` still have no code licence of their own.** T098 decided the *website
> code* and the *brand assets*. Those two repositories' own terms are a separate, undecided
> question, and their READMEs now say exactly that rather than pointing at a closed task.

**The archived historical branding directories were not modified and were not licensed.** They
remain in the local archive outside every repository, unpublished, and the brand policy states
explicitly that it grants no rights to them and makes no claim about their status.

## 3al. T099 — container registry, DECIDED (2026-08-12)

**Maintainer decision: GitHub Container Registry (`ghcr.io`).**

**Publishing credential — none stored:**

- GitHub Actions publishes with the **short-lived `GITHUB_TOKEN`** minted for the workflow run.
- Least privilege: **`contents: read` and `packages: write`, on the image-publishing job
  only** — set per-job so no other job inherits package write access.
- **No PAT, deploy token, repository secret, or long-lived registry credential was created**,
  and none is required.

> **This is not OIDC, and the earlier rationale saying so was wrong.** `GITHUB_TOKEN` is an
> installation token that Actions injects into the run and revokes when it ends. OIDC is a
> distinct mechanism where a workflow exchanges a signed identity token with an external
> provider for temporary credentials — that is how crates.io trusted publishing works, and it
> is **not** how GHCR is authenticated here. Both avoid a stored secret, which is why they are
> easy to conflate; the distinction matters the moment someone tries to configure a trust
> relationship GHCR neither needs nor offers. **The correction is recorded rather than quietly
> substituted.**

**Pull credential — none, by design:** the deployment image is **publicly pullable**, so the
k3s host needs **no `imagePullSecret`** and stores no registry credential at all. GHCR package
visibility is independent of repository visibility, so the sources stay private. The image
carries only the built static site, already served publicly — publishing it discloses nothing
a visitor could not already see.

> **Correction appended 2026-08-15 — two clauses above are wrong, and one was wrong when
> written.** The dated-evidence framing at the head of this ledger is not enough here, because
> one of these was never true, not merely overtaken.
>
> - "*so the sources stay private*" — **stale.** True on 2026-08-12; **false since ADR-0006
>   D13.** No Renvor repository is private. The load-bearing fact is the *independence* of
>   package visibility from repository visibility, which is unaffected.
> - "*already served publicly … nothing a visitor could not already see*" — **this was false
>   on 2026-08-12 and is false now.** **No Renvor site has ever been deployed.** Measured
>   2026-08-15, `renvor.dev`, `docs.renvor.dev`, and `www.renvor.dev` each resolve to the
>   shared origin and return **HTTP 404**, with HTTPS failing validation against a public
>   trust store because Traefik serves its default self-signed certificate. **Something
>   answers; no Renvor content is served.** The argument holds as a property of the design
>   once a site exists; it was never an observation.
>
> **The original wording above is left unedited** — this section is dated evidence, and the
> error is part of what the record has to show. The same two clauses were corrected in place
> in `PLAN.md` §26.4 and ADR-0006 D7, which are current-state documents rather than evidence.

**The trade, stated so it is not inherited by accident**: image contents and pull counts
become public, and the image cannot serve as a private distribution channel. Acceptable for a
static site whose content is already public; **not** acceptable for an image carrying
configuration, credentials, or unreleased material. `PLAN.md` §26.4 now says so explicitly, so
the next image does not silently inherit "public" as a default.

**Addressing**: immutable digest (`@sha256:…`) only; a tag may accompany a digest for
readability, but the digest is what deploys.

**Why the on-host GitLab registry was rejected**, recorded in ADR-0006's Alternatives table:
publishing to it from GitHub Actions needs a **long-lived cross-system credential** — the
exact artefact class the release process already worked to eliminate; and a registry that
lives on the origin is **unavailable in precisely the recovery scenario** ADR-0006 D9 depends
on it for.

**Nothing was configured.** No package, workflow, image, credential, or infrastructure change.
**Only the registry decision is complete; deployment remains blocked.**

## 3am. T108 — `image-size` time-bounded exception; task REMAINS OPEN (2026-08-12)

**The advisories are not suppressed, not dismissed, and not described as fixed or zero-risk.**

| | |
|---|---|
| `GHSA-w3rx-r6r6-pgpr` | high, **CVSS 7.5**, ICNS parser infinite loop, affected `<= 2.0.2`, **first patched version: NONE** |
| `GHSA-5p2g-fcmc-qvqq` | high, **CVSS 7.5**, JXL and HEIF parser infinite loops, affected `<= 2.0.2`, **first patched version: NONE** |
| Upstream repository | **ARCHIVED.** Last code commit 2025-04-02. Maintainer's notice: *"Archiving this repo, because I don't want to deal with the same LLM generated 'security advisory' about an infinite loop over and over again"*, with possible revival on Codeberg and *"This repo on github will not be updated."* |
| `@docusaurus/core` latest | **3.10.2** — the version installed. No newer release exists |
| Docusaurus replacement | PR #12235 (`image-dimensions`) — **open, DRAFT, breaking change, no milestone**; issue #12231 open, `needs triage` |

**No upstream fix is coming from `image-size`.** That is not a pending wait; it is closed.

### Reachability, proven rather than asserted

| Fact | Evidence |
|---|---|
| Reached only as a **build-time transitive** dependency | `@docusaurus/core@3.10.2 → @docusaurus/mdx-loader@3.10.2 → image-size@2.0.2` |
| **No MDX or local image embeds** in documentation source | 0 matches for `![…](…)`, `<img>`, or image `require`/`import` across all six `.mdx` files |
| **No untrusted image input** | The site accepts no uploads and takes no user-supplied images |
| **Absent from production static output** | `build/` = 36 files, 988 KB. **0** files containing the string `image-size`; **0** containing ICNS/JXL/HEIF parser identifiers; **0** raster assets emitted; **0** `node_modules`; **0** requires of `image-size`. Only `favicon.ico`, served as bytes and never parsed |

### Fail-closed control, implemented and tested in both directions

`docs/scripts/check-image-inputs.mjs`, wired as **`prebuild` and `prestart`** so the build
cannot run unless it passes. It exits non-zero on its own internal errors rather than
skipping — a check that cannot run is a failure, matching the verification contract.

| Test | Result |
|---|---|
| Clean tree | **exit 0** — "no MDX image embeds, no raster assets, no image imports" |
| Markdown embed `![a diagram](./diagram.png)` | **REFUSED**, named `docs/intro.mdx:51` |
| `<img src="/img/x.png">` | **REFUSED**, named the element and src |
| Raster dropped into `static/img/` | **REFUSED**, named the file and extension |
| **`npm run build` with an embed present** | **exit 1**, `BUILD REFUSED` printed, **0** "Generated static files" — the build genuinely did not produce output |
| Test file restored afterwards | **byte-identical**, SHA-256 compared |

### Why T108 is NOT complete

Two required compensating controls **cannot be objectively verified, because their subjects do
not exist**:

| Unmet control | Why |
|---|---|
| *"absent from the production runtime container"* | **0 container definitions exist** in the repository — no `Dockerfile`, no `Containerfile`. There is no container to inspect |
| *"absent from the runtime SBOM"* | **0 SBOM artifacts exist** — none is persisted in the tree, and no deployment workflow produces one |

Both must be verified **when the deployment image and its SBOM are first produced**, and T108
cannot close before then. Marking it complete now would record a verification that never
happened.

### Exception terms

**Dated 2026-08-12. Reassess 2026-09-11**, or earlier if Docusaurus ships a maintained
replacement or fixed release. **The exception expires immediately if** documentation begins
accepting image input, the reachability proof fails, or the dependency enters a runtime
artifact.

**Not done**: Docusaurus was not downgraded; `npm audit fix --force` was not run; the
advisories were not suppressed or deleted; the draft breaking-change PR was not adopted; and
the documentation site was **not deployed**.

## 3an. T111 — CAA policy prepared, no DNS change made (2026-08-12)

A read-only check on 2026-08-12 found **no `CAA` record on `renvor.dev`**, so nothing
currently constrains which CA may issue for the domain. ADR-0006 D5 requires one.

**Policy**: Let's Encrypt only. **No wildcard certificate is planned**, and no wildcard DNS
record exists.

**Exact proposed records** — `flags` `0` (non-critical) throughout, so an unrecognised property
tag cannot block issuance outright:

```dns
renvor.dev.  CAA  0 issue     "letsencrypt.org"
renvor.dev.  CAA  0 issuewild ";"
renvor.dev.  CAA  0 iodef     "mailto:admin@ahmedanbar.dev"
```

| Record | Effect |
|---|---|
| `issue "letsencrypt.org"` | Only Let's Encrypt may issue **single-name** certificates. Every other CA is refused |
| `issuewild ";"` | **No CA may issue a wildcard.** `";"` is the explicit deny value. **This record is not redundant**: an `issue` record alone would still permit the named CA to issue wildcards, so omitting it would leave wildcard issuance open to Let's Encrypt |
| `iodef "mailto:…"` | Requests that a CA report policy violations to the security contact. Advisory — CAs are not obliged to send it |

**Inheritance**: `issue` and `issuewild` set at the apex are inherited by every subdomain that
does not publish its own `CAA`, so this one set covers `docs.renvor.dev` and `www.renvor.dev`
without separate records.

**Sequencing warning**: cert-manager's HTTP-01 flow must be working **before** these records
are added. A mistyped CA domain does not fail loudly — it silently refuses all issuance, and
the symptom appears as an unexplained ACME failure.

**Verification after creation**: a `CAA` lookup against **both** authoritative nameservers,
followed by a test issuance. **T111 stays open until the maintainer separately authorises the
DNS change and verifies it. No Cloudflare record was created or modified in this pass.**

## 3ao. FR-032 and macOS — checked, no correction needed in this repository (2026-08-12)

Phase 001 is **Linux-only**, and this was verified rather than assumed.

| Check | Result |
|---|---|
| Renvor **FR-032** text | *"Release tags MUST be signed, and releases MUST run from a protected environment with named approvers."* (`spec.md:214`) — **release process, not platforms** |
| Lines mentioning **both** FR-032 and macOS/Darwin | **0**, across every `.md`, `.yml`, `.yaml`, `.toml`, and `.rs` in the repository |
| FR-032 references | `RELEASING.md`, `contracts/package-metadata.md`, `checklists/governance.md` CHK066, tasks **T071** (signing) and **T072** (protected release environment) — every one about signing and release protection |
| CI runners | **all 7 jobs `ubuntu-latest`** across `ci.yml`, `docs.yml`, `security.yml`, `release-dry-run.yml`, `release-tag-verify.yml`. **No macOS runner exists** |
| macOS references that do exist | `plan.md:27`, `spec.md:290`, `contracts/support-policy.md:27,30` — all state Linux is the only required platform for this phase and macOS enters when platform-sensitive behaviour appears (PLAN.md §17.2). **Already correct** |
| Clarifications sessions | one session, 2026-08-11, six entries — **none mentions macOS** |

**No artifact in this repository incorrectly connects FR-032 to macOS, so no correction was
made and none is recorded as made.** No macOS CI runner, local macOS evidence requirement, or
macOS merge gate was added.

> **The FR-032 ↔ macOS linkage exists in a different project.** `attaa-next`'s Phase 001
> specification defines its own **FR-032** as *"Every requirement above MUST hold on both macOS
> and Linux"*, and carries a clarification entry explicitly tying macOS evidence to FR-032. The
> two projects number their requirements independently. **That artifact is outside this
> workspace and was not modified.**

## 3ap. T106 and T102 — server reinspection still incomplete (2026-08-12)

**Unchanged from §3aj.3, and restated because it gates ADR-0006.** The read-only server
reinspection **failed at authentication**. Port 2022 is reachable, so this is not a
connectivity problem: the configured SSH profile targets user `deploy`, while the host mapping
uses a different user and identity file. **No SSH configuration, credential, user, key, alias,
or port was changed, and no private-key content was read.**

**The 2026-08-11 audit is retained as historical evidence, not as current proof.** Every server
fact in ADR-0006 rests on it and carries that status.

**T106 and T102 both remain open** until a separately authorised read-only inspection
succeeds. **ADR-0006 is not accepted** while T101 or T106 is unresolved.

## 3aq. T095–T097 — maintainer visual approval of the V7 landing page (2026-08-12)

**Review date**: 2026-08-12
**Reviewer**: Ahmed Anbar, maintainer — **self-review under W-002**, not an independent review
**Method**: personal inspection of the **rendered production build** served locally at
`http://localhost:4891/`, on desktop and mobile, in both light and dark themes

### Exact state reviewed

| | |
|---|---|
| Repository | `renvor-rs/renvor-site` (private) |
| HEAD at review | **`8b68e326867e3c296b2117fd13ca855b478cd036`** (`8b68e32`, "Add the V7 landing source", committed 2026-08-12T14:47:12+03:00) |
| **What was actually reviewed** | **HEAD *plus* the uncommitted working-tree corrections** — the reviewed content is **not yet in any commit** |
| Source-set SHA-256 | `b4c0856f0f03c85870d421557d19b09330ef4d69d9022b982aa397746c3bc59b` |
| Build-output SHA-256 | `ae95f1e5c6e9c935a7693d4fbedbbd3260abe7fa676dc9fd8dfe9c1fb815698f` (21 files, 908 KB) |

> **The distinction is deliberate and load-bearing.** Recording only `8b68e32` would attach the
> approval to a commit whose content is the *uncorrected* page. The two content hashes anchor
> what was seen, so the attestation can be checked against the commit that eventually carries
> it rather than assumed to match.

### What was approved

Landing-page presentation; the truthful development-status disclosure; the planned-version
labels; the non-installable CLI demonstrations; the GitHub-only links; the responsive layout;
the accessibility behaviour; the reduced-motion behaviour; and the animations.

### What this approval is NOT

**This is a visual and product approval of the page's presentation and truthfulness. It is
not, and must not later be read as:**

- evidence that the Renvor framework has been released — **it has not**;
- evidence that any crate is published — `renvor` and `renvor-cli` both return **HTTP 404**
  from the crates.io registry index;
- authorisation to deploy the site — **deployment remains blocked** by T101 (CSP), T102
  (server re-verification), T106 (backup ruling), T108 (`image-size`), and T111 (CAA);
- acceptance of ADR-0006, which **remains `proposed`**.

**T095, T096, and T097 are complete.** They closed on maintainer review of the rendered page,
not on the automated checks — which were reported alongside as input to that review and were
explicitly not treated as approval.

## 3ar. T112 — link-check transport made deterministic (2026-08-12)

Verification step 9 failed twice on pull request #11 without any link being broken. Both
failures are preserved below rather than rewritten, because the second one disproved the
diagnosis of the first and that is the useful part of the record.

### Failure 1 — HTTP 503 (run 31639969949, attempt 1)

Both `verify (1.94.0)` and `verify (stable)` failed at `[9/10] link check`, `lychee` exit 2:
`🔍 257 Total 🔗 62 Unique ✅ 204 OK 🚫 21 Errors`. All 21 errors were
**HTTP 503 Service Unavailable** from `github.com`; **0 were 404**. Every affected URL
returned HTTP 200 when checked directly afterwards, and the same check passed locally on
both toolchains. Read at the time as transient throttling.

### Failure 2 — HTTP/2 protocol error (run 31639969949, attempt 2)

Only the two failed jobs were re-run, with **no change to the commit**. The result split:

| Job | Link check | Totals |
|---|---|---|
| `verify (stable)` — job 94271497549 | **ok — passed** | `✅ 225 OK 🚫 0 Errors 🔀 5 Redirects` |
| `verify (1.94.0)` — job 94271497334 | **FAILED — lychee exit 2** | `✅ 187 OK 🚫 38 Errors` |

The 38 errors were **14 real failures plus 24 cached repetitions** of those same 14, across
**9 unique URLs**, all on `github.com`. Every one carried the message
`HTTP/2 protocol error. Server may not support HTTP/2 properly` — a transport fault with
**no HTTP status at all**. There were **0 HTTP 503** and **0 HTTP 404** responses; the string
`503` appears 7 times in that log and every occurrence is a timestamp fragment or a Cargo
line number, not a status. All 9 URLs resolved when checked directly: 4 returned HTTP 200 and
5 returned HTTP 301 to their canonical `blob/` form, final HTTP 200.

Two jobs, same commit, same configuration, same 62 URLs, running concurrently, produced
0 errors and 38 errors. That isolates the cause to the transport rather than to the links,
and it also rules out the throttling diagnosis: throttling returns a status, this returned
none. None of the affected links originate in the pull request — they come from
`docs/docusaurus.config.js` (`editUrl`, navbar and footer targets) and `docs/docs/governance.mdx`,
neither of which is among the 7 changed paths.

### Diagnosis

Every external link in the built site points at one host. `lychee` 0.24.2 defaults to
**10 concurrent requests per host at 50 ms intervals**, and `max_concurrency` was 8, so the
checker opened many simultaneous HTTP/2 streams on a single connection to `github.com`.
GitHub reset those streams under that load, which `reqwest` surfaces as a protocol error
rather than a response. Both matrix jobs ran in parallel from one runner egress address,
doubling the pressure on the same host from the same source.

### Fix

Three changes, all using options verified against the installed `lychee` 0.24.2 rather than
assumed. Both `Config` and `HostConfig` carry `#[serde(deny_unknown_fields)]`, so an
unsupported key fails the config parse loudly; this was confirmed by a negative control that
appended a fabricated `http_version` key and was rejected with
`unknown field 'http_version', expected one of 'concurrency', 'request_interval', 'headers'`.

| Change | Where | Effect |
|---|---|---|
| `[hosts."github.com"] concurrency = 1` | `lychee.toml` | one in-flight request to `github.com` at a time, removing the stream contention |
| `[hosts."github.com"] request_interval = "250ms"` | `lychee.toml` | minimum spacing between consecutive requests to that host |
| `accept = ["200..=299"]` | `lychee.toml` | **429 removed** — a rate-limited response no longer counts as a working link |
| `strategy.max-parallel: 1` | `.github/workflows/ci.yml` | the two matrix jobs no longer compete for the same host from one egress address |
| `GITHUB_TOKEN: ${{ github.token }}` | `.github/workflows/ci.yml` | `lychee`'s documented env interface for `--github-token` |

The per-host table is placed last in `lychee.toml` because a TOML table claims every key that
follows it; a top-level setting added below it would silently become a per-host setting.

**What the token does and does not do.** `handle_github` runs **only after a normal request
has already failed**, and `check_github` returns success solely for bare `owner/repo` URLs —
a URL naming a path inside a repository returns `InvalidGithubUrl`, so the original failure
stands. The token therefore rescues repository URLs and avoids API rate limiting, but the
per-host limits carry the actual determinism. One caveat is recorded rather than discovered
later: for a **private** repository the API fallback returns OK for any URL beneath it without
checking the path. Every `github.com` link here targets `renvor-rs/renvor`, which is public,
so the caveat does not apply today; it would if a link ever pointed into a private repository.

The token is the workflow run's built-in installation token under the existing top-level
`contents: read`. It is not a stored secret, no personal access token was created, and
`lychee` declares it with `hide_env_values = true` and holds it as a `SecretString`.

### What was deliberately not done

`github.com` was not excluded. No link was removed or rewritten. HTTP/2, TLS, timeout, 429
and 5xx failures are **not** accepted — `--accept-timeouts` was not set. No
`continue-on-error`, no failure downgraded to a warning. Retries stay **bounded** at
`max_retries = 2`. Both toolchains and all ten verification steps are unchanged. No
`http_version` or transport-forcing option was invented; `lychee` 0.24.2 exposes none.

### Local validation

| Check | Result |
|---|---|
| Config parses | exit 0 |
| Fabricated key rejected | config error, as above |
| Link check, 5 consecutive runs | **exit 0 every time**, `✅ 225 OK 🚫 0 Errors 🔀 5 Redirects` identical across all five |
| Per-host statistics | `github.com │ 38 reqs │ 100.0% success │ 891ms median` |
| Valid GitHub URLs (repo and file) | **exit 0**, 2 OK |
| Fabricated GitHub URLs (missing file, missing repo) | **exit 2**, 2 errors, both `Rejected status code: 404 Not Found` |

The last row is the one that matters: the fix removes a false failure without removing the
ability to detect a real one.

## 3as. T100 — Node 24 pinned in the landing repository (2026-08-13)

| Field | Value |
|---|---|
| **Date** | 2026-08-13 |
| **Task** | **T100** — pin Node 24 in the landing repository, distinct from the framework's Node 22 |
| **Repository** | `renvor-rs/renvor-site` |
| **Pull request** | **`renvor-rs/renvor-site#2`**, merged with a merge commit. Deliberately not hyperlinked: the site repository is **private**, so its pull-request URL returns HTTP 404 to readers without repository access, including unauthenticated readers |
| **Merged commit** | `fe0e468e8ed6b54d211423b056e0d44a0669b66c` |
| **Reviewed source commit** | `78b2e0fa48212c3a8c5e7eba782e2648df7adf93` |
| **Changed path** | `.nvmrc` |
| **Exact content** | `24\n` — three bytes, hex `32 34 0a` |
| **Diff scope** | one file, one insertion, zero deletions; the file is newly added |

### Validation

Performed locally at the reviewed source commit, using the Node version the new file itself
declares rather than the shell default:

| Check | Result |
|---|---|
| Node selected from `.nvmrc` | `v24.19.0` |
| Package manager | pnpm `11.21.0`, matching the declared `packageManager: pnpm@11.21.0` |
| `pnpm install --frozen-lockfile` | passed |
| `pnpm run typecheck` | passed |
| Production build | passed |
| `pnpm-lock.yaml` | **byte-for-byte unchanged** (SHA-256 compared before and after) |

### Verification basis

The merge was verified through **live GitHub data**, and the signed source commit was
**inspected locally**. The integration commit has exactly two parents —
`4a6937aa0ea767606e558c9b5768763f5ab2d580` (the base) and the reviewed source commit — and
its tree equals the reviewed source tree exactly, so integration introduced no content of its
own. The merge commit carries GitHub's platform signature (`verified: true`); the source
commit beneath it carries the maintainer's own SSH signature and survives in `main`'s
ancestry because a merge commit was used rather than a squash.

The framework's `.nvmrc` was not touched and still reads `22`. The two policies are recorded
separately, which is what this task existed to guarantee.

### Completion boundary

**T100 proves only the repository-local Node 24 pin.** It does not prove CI, branch
protection, deployment readiness, or deployment. A version pin binds only what reads it, and
at the time of writing **nothing in the site repository reads it automatically**.

### Remaining pre-deployment requirement

`renvor-rs/renvor-site` has **no repository-owned CI**: pull request #2 merged with no checks
reported, and the repository contains no workflow of its own.

**GitHub reports `main` as unprotected**: REST reports `protected: false`, GraphQL reports
zero branch-protection rules and no rule attached to `main`, and a direct push to `main`
previously succeeded. The rulesets REST endpoint is unavailable for this private repository
on the current plan, so repository rulesets were not inspected; the conclusion rests on the
three signals above rather than on an exhaustive enumeration of every protection mechanism.

The migration plan requires the landing repository's CI to consume its own `.nvmrc`; that
half is unbuilt and is tracked as **T113**, which also covers the required checks, protected
`main` with no administrator bypass, and SHA-pinned third-party actions.

### Result

`T100 complete; T113 open. No CI, branch-protection, deployment, server, DNS, registry, environment, credential, or private-key change was made.`

## 3at. T101 — V7 landing CSP compatibility verified; no production header, live-server access, or deployment action (2026-08-14)

| Field | Value |
|---|---|
| **Date** | 2026-08-14 |
| **Operator** | Ahmed Anbar |
| **Task** | **T101** — verify CSP compatibility with the V7 landing implementation (GSAP, self-hosted variable fonts) |
| **Repository** | `renvor-rs/renvor-site` |
| **Pull request** | **`renvor-rs/renvor-site#3`**, merged with a merge commit. Deliberately not hyperlinked: the site repository is **private**, so its pull-request URL returns HTTP 404 to readers without repository access |
| **Merge commit** | `206cefdff74399d96f723a75d961fb8d700e0fd5` |
| **Parent 1 — base** | `fe0e468e8ed6b54d211423b056e0d44a0669b66c` |
| **Parent 2 — audited signed source** | `f8f1786a02c2d921859068fbd487b5d5e57a764c` |
| **Source tree = merge tree** | `e7fbc9d1438eaf58dee2c7d634dac4003b8664ec` |
| **Integration verification** | Live GitHub read-only verification on 2026-08-14 reports merge commit `206cefdff74399d96f723a75d961fb8d700e0fd5` as `verified=true`, `reason=valid`; source commit `f8f1786a02c2d921859068fbd487b5d5e57a764c` is the audited signed maintainer commit |
| **Clean site build** | Node.js `v24.19.0`, pnpm `11.21.0` |
| **Enforcement harness** | `@playwright/test` 1.62.1 under Node.js `v22.12.0`; `serve` 14.2.6 |

**The verified state is immutable and exactly identified.** The source tree and the merge tree
are the same object, `e7fbc9d1438eaf58dee2c7d634dac4003b8664ec`, so **integration added no
content beyond the audited source tree** — what was tested is byte-identical to what was
merged. Every result below is bound to that tree and to no other.

### Scope of the change under test

The Docusaurus configuration was changed by adding exactly one line:

```
baseUrlIssueBanner: false,
```

That is the whole production-source change — one file, one insertion, zero deletions.
Disabling Docusaurus's base-URL issue banner removed its generated diagnostic content from
the tested output and reduced the CSP hash and allowance surface. The r4 evidence validates
the resulting tree; it does not establish that the prior banner could be admitted only with
`unsafe-inline`. The resulting build still intentionally authorises one inline script by
hash and one exact style attribute through `unsafe-hashes`.

**Build provenance, which is separate from harness provenance.** The tested artifact came
from a **clean production build from clean generated directories**, run on Node.js
`v24.19.0` with pnpm `11.21.0`. Those versions describe the build only. The Playwright
enforcement harness ran separately, under Node.js `v22.12.0`, and used no pnpm — see the
`Enforcement harness` row above.

### The exact candidate policy — 434 bytes

```
default-src 'self'; script-src 'self' 'sha256-0xt7rjlfRsoD7aukiSWSNvMBWU159Lh5cl3WNaV1g+w='; style-src 'self'; style-src-attr 'unsafe-hashes' 'sha256-biLFinpqYMtWHmXfkA1BPeCY0/fNt46SAZ+BBk5YUog='; img-src 'self' data:; font-src 'self' data:; connect-src 'self'; child-src 'none'; frame-src 'none'; media-src 'none'; frame-ancestors 'none'; form-action 'none'; base-uri 'none'; object-src 'none'; worker-src 'none'; manifest-src 'none'
```

SHA-256 `ab861669ba0b7b7760df0913319e9d261649787f1f599c0bca7f199874d91bed`, **exactly 434
bytes, no trailing newline**.

**This policy is not free of inline allowances, and must not be described as such.** It
carries **one hashed style-attribute allowance using `unsafe-hashes`**
(`style-src-attr 'unsafe-hashes' 'sha256-biLFinpqYMtWHmXfkA1BPeCY0/fNt46SAZ+BBk5YUog='`) and
**`data:` allowances for both images and fonts**. What it does not carry is `unsafe-inline`,
`unsafe-eval`, or any third-party origin.

### Results

| Phase | Result |
|---|---|
| Negative-control Enforcement preflight | **3/3 passed** — one per Chromium, Firefox, and WebKit |
| Final exact-head r4 Enforcement matrix | **48/48 passed** |

The **negative control is what makes the matrix meaningful**. It served the same enforced
policy against a deliberately non-compliant `data:` sentinel script and required the browser
to block it on all three engines, with `disposition: enforce`. Without it, a 48/48 pass would
be equally consistent with a policy that was never actually applied.

### Matrix dimensions

Every combination of:

| Dimension | Values |
|---|---|
| Engine | Chromium, Firefox, WebKit |
| Route | landing, genuine HTTP 404 |
| Viewport | desktop, mobile |
| Theme | light, dark |
| Motion | normal, reduced |

3 × 2 × 2 × 2 × 2 = **48 cases, all passed**.

**GSAP and the fonts were exercised substantively, not merely loaded.** Animation behaviour
ran under the enforced policy in both the normal and reduced-motion paths.

GSAP ran without `unsafe-inline` or `unsafe-eval`. `Outfit Variable` and
`Geist Mono Variable` were fetched from same-origin `/assets/fonts/...` resources under
`font-src 'self'`. The candidate policy also allowed `data:` fonts, but r4 did not establish
that allowance as necessary.

Across all 48 cases the run recorded **zero application CSP events or refusals, zero page
errors, zero failed requests, zero third-party requests, zero duplicate events, and zero
collector transport errors**.

### Artifact hashes

| Artifact | SHA-256 |
|---|---|
| Policy | `ab861669ba0b7b7760df0913319e9d261649787f1f599c0bca7f199874d91bed` |
| Final stage-c specification | `e8825eb629a9e4b7139c7cd60c3b3884af43540c2d0bb8e21d8a436a4f8643aa` |
| Playwright configuration | `cdd3041669c17023447d120c849ddbee3a4ae750177866b44f22001a63045663` |
| Collector | `bed6644cfd21b2e7514ff7e3e1d5d0200f7562001c8f4a546bc7ece97c89311b` |
| Classifier | `f41079dfaf2cd906bf179512c16585d2a9ba292ed880a2210cd5b62ceb47ebe5` |
| Server | `9a100766034ab2d8464f029f9d307754df74173c296ff5b014a42b3b6beaaff7` |
| Harness package | `1bbf70eb816f4456ad7ad2efaeea55c377ed461fbd93f1e7f7306c33bca0951f` |
| JSON reporter | `5d905ede431e0c3cea7b29a6d2a6687eaae0c49c9de40b4a90b95cee0797cd20` |
| Request ledger | `75053214beed6617e0c561425f876a6616428ce7f1e989abac738c67e233a6dd` |

**Repository state was unchanged by the run.** The site state files captured **before and
after** are byte-identical at
`80ebdf604ae662fc976a74c8e8ba27649b27b60d99d01e09f807a3194579a81b`, and the framework state
files before and after are byte-identical at
`202c371722a99754260dab7de989ab3fcfa62a9663455a23cfb63e8982f844c7`. Testing mutated neither
repository.

### Normative versus diagnostic

The request ledger contains **946 records**. **That aggregate is diagnostic inventory only.**
It is **not 946 tests, not 946 assertions, and not a pass threshold**, and no conclusion in
this section rests on the number matching any particular value. Aggregate counts vary
legitimately with sub-resource cache revalidation between runs.

**What is normative** are the semantic, per-representation, enforced-policy, and per-case
assertions. Every full `200` or `404` document response carried the byte-exact enforced
policy, and `Content-Security-Policy-Report-Only` was absent. Chromium and Firefox also
exercised conditional server `304` responses carrying the matching ETag and no CSP header;
Chromium exposed its reload to Playwright as `200`, while Firefox exposed `304`. WebKit
performed an unconditional second `200` and received the byte-exact policy again. Continued
enforcement across conditional revalidation demonstrated inheritance from the cached
representation. The negative-control events reported `disposition: enforce`, and every one
of the 48 application cases independently produced zero application violations.

### Boundary — what this does not establish

**Testing used a local enforcement harness serving the built site over loopback. It was not
production, and it was not Traefik.** A policy proven in that harness is evidence of page
compatibility, not evidence of a deployed control.

**A header was configured and served — locally.** The disposable local server set
`Content-Security-Policy` to the 434-byte candidate policy on every full `200` and `404`
document response; that is precisely what made the run an Enforcement test rather than a
report-only observation, and enforcement persisted across the conditional `304` responses
described above, which carried no CSP header of their own. The
statements below deny **production** header configuration and **live-server** access, not the
existence of that local header.

This section closes **T101 only**. Specifically:

- **It does not prove production deployment readiness.**
- **No production response header was configured or enabled** — no production CSP and no
  production response-header policy exists anywhere.
- **No Traefik middleware was written, configured, or enabled.**
- **No live-server access or production-infrastructure action occurred** — no deployment,
  image publication, DNS, server, Kubernetes, registry, environment, credential, Cloudflare,
  or other infrastructure mutation.
- **T102, T106, T108, T111, and T113 remain deployment gates** and are unaffected by this
  closure.
- **ADR-0006 remains `proposed`**, solely because its one remaining internal unresolved
  question — **T106**, the maintainer ruling on the shared server's absent backups — is still
  open. T101 no longer blocks it.

The candidate policy was validated against the production build generated from tree
`e7fbc9d1438eaf58dee2c7d634dac4003b8664ec`. Any new production build must be revalidated.
Recompute a particular hash only when the corresponding inline script or style-attribute
bytes change; unrelated output changes do not alter that digest.

### Result

`T101 complete. A local harness served the enforcement header; no production response header was configured or enabled, and no Traefik middleware was written, configured, or enabled. No live-server access or production-infrastructure action occurred — no deployment, image publication, DNS, server, Kubernetes, registry, environment, credential, Cloudflare, or private-key change was made. ADR-0006 remains proposed pending T106.`

## 3au. ADR-0006 D12 — hybrid source-control topology decided; GitHub public for applications, private self-hosted GitLab for infrastructure (2026-08-14) — **SUPERSEDED 2026-08-15, see §3av**

> **Dated evidence, not current state.** This section records what was decided and verified on
> **2026-08-14**, when the hybrid topology was accepted into `main`. It was superseded on
> **2026-08-15** by ADR-0006 **D13** (all-public GitHub), recorded in **§3av**. **Everything
> below is retained byte-for-byte as recorded on 2026-08-14** — only this heading and this
> banner were added, and no cell, row, or sentence inside the section was edited or annotated.
> Read it as describing 2026-08-14. **The GitLab cutover it anticipated never happened.**
>
> **Statements below that are false today are corrected here rather than inside the preserved
> evidence. The list is illustrative, not exhaustive** — read every statement in this section
> as describing 2026-08-14:
>
> - "**Infrastructure** | `renvor-infra` targets the private self-hosted GitLab instance at
>   `gitlab.ahmedanbar.dev`. **Destination only — not canonical until T114**" — true on
>   2026-08-14. **Superseded 2026-08-15**: `renvor-rs/renvor-infra` is public on GitHub and
>   canonical there. Per the T114 cancellation record the cutover was abandoned before it ran;
>   *the GitLab instance was deliberately not inspected, so no claim is made about its
>   contents.*
> - "**New gate** | **T114**…" and "*T113 and T114 are open*" — **T114 was cancelled on
>   2026-08-15, not passed** (§3av), and **T113 was closed on 2026-08-15** on live
>   re-verification (§3aw). Neither is open.
> - the closing **Boundary** paragraph — "*T113 and T114 are open. ADR-0006 remains proposed
>   pending T106. Phase 001 is not complete…*" — was true on 2026-08-14. **Superseded
>   2026-08-15**: T114 cancelled, T113 closed, **ADR-0006 accepted**, and Phase 001 is a
>   closure candidate with 0 open tasks. **The clause "no Renvor 1.0 claim is made" still
>   stands.** *(An earlier attempt annotated that paragraph in place, which would have broken
>   the byte-for-byte claim this banner makes. It was reverted and the correction moved here.)*

**This section records a decision and the preflight that authorised it. At the time of this
commit the transition had not been executed**: `renvor-rs/renvor-site` and
`renvor-rs/renvor-docs` were still private, no GitLab group or project existed, and no
infrastructure content had been pushed anywhere.

| Item | Result |
|---|---|
| **Decision** | ADR-0006 **D12**, dated 2026-08-14. Supersedes the all-GitHub, all-private repository model in ADR-0005 and `PLAN.md` §26.1 without rewriting either |
| **Application properties** | `renvor-rs/renvor`, `renvor-rs/renvor-site`, `renvor-rs/renvor-docs` — GitHub, public, and GitHub remains the source, review, and CI surface for all three |
| **Infrastructure** | `renvor-infra` targets the private self-hosted GitLab instance at `gitlab.ahmedanbar.dev`. **Destination only — not canonical until T114** |
| **Registry** | **Unchanged.** T099 and D7 stand: public application images remain planned for GHCR. **The GitLab Registry is not used** and is disabled on the GitLab project |
| **`renvor-docs`** | Public and deliberately **commit-empty** until its licence is decided and **T108 permits migration**. `framework/docs` stays authoritative. **T108 is not altered** |
| **New gate** | **T114** — encrypted off-VPS backup, exact-version isolated restore proof, matching repository refs and hashes, retention/RPO/RTO, and separate human approval |

### Preflight verified before any write (2026-08-14)

| Check | Observed |
|---|---|
| Framework live `main` | `a942a689effb565adc3fbf3adc7a9fcd174d9cca`; PR #14 `merged=true` with that exact merge SHA |
| Site live `main` | `206cefdff74399d96f723a75d961fb8d700e0fd5` |
| Visibility at preflight | framework public; site, docs, and the GitHub infra placeholder all private |
| Docs and infra repositories | **0 commits each**; untracked `README.md` plus `assets/renvor-mark-v7.svg` (543 bytes, `237479dc…`, byte-identical in both) and nothing else |
| GitLab instance | **CE 19.0.1**, revision `35d349e97ce`, at `gitlab.ahmedanbar.dev` |
| GitLab identity | `ahmedanbar`, id 35, `state=active`, `is_admin=true`, `can_create_group=true`, `can_create_project=true` |
| GitLab Renvor objects | **None** — no `renvor-rs` group and no Renvor project. Two unrelated groups and nine unrelated projects exist, all private |
| Secret scanning | `gitleaks` 8.30.1, redacted, full history **and** current files across all four repositories — 59 commits scanned, **no leaks found**, every scan exit 0 |
| Site GitHub metadata | 0 Actions secrets, 0 Dependabot secrets, 0 variables, 0 environments, 0 deploy keys, 0 webhooks, 0 tags, 0 releases; wiki and Pages disabled; not a fork, not archived |

### A tooling correction worth recording

`glab`'s configured default host is **`gitlab.com`**. The first version probe therefore
answered for GitLab SaaS — `19.3.0-pre`, `enterprise: true`, `kas.gitlab.com` — not for the
self-hosted instance. Re-querying with `--hostname gitlab.ahmedanbar.dev` returned the real
figures above. **Every GitLab call in this workflow must name the host explicitly**; without
it, a group would have been created on public SaaS instead of the private instance.

### Two preflight conditions did not hold, and were resolved by rebaselining rather than by force

The authorising workflow expected the site branch `docs/update-deployment-gates` to be local
only, at `3e72a4685e63635a209ecfe5b6f1a1003adb427d`, with no pull request. **Both were false**:
the branch was already pushed at `b6ed04d219c27a4f69526751c17a9db5aba2c575`, seven commits
ahead of `main`, with pull request **#4** open and all five checks green. Reaching the
expected state would have required a force push, a branch deletion, or closing a green pull
request. **The maintainer rebaselined onto the live state instead.** No history was rewritten,
no ref was force-updated, and no branch was deleted.

### Boundary

`T113 and T114 are open. ADR-0006 remains proposed pending T106. Phase 001 is not complete and no Renvor 1.0 claim is made. This section records a decision and a preflight only — at commit time no repository visibility had changed, no GitLab group or project existed, no infrastructure content had been pushed, and no deployment, image publication, registry, DNS, server, Kubernetes, environment, credential, Cloudflare, or private-key action had occurred.`

## 3av. ADR-0006 D13 — public GitHub adopted for all four repositories; `renvor-infra` published and protected; T114 cancelled (2026-08-15)

**D13 supersedes D12.** All four Renvor repositories are public on GitHub and canonical there.
Private self-hosted GitLab is no longer part of the Renvor source-control topology. **D12 is
preserved in ADR-0006 and in §3au as dated history; it was a real decision, accepted into
`main` on 2026-08-14, and is not rewritten.**

| Item | Result |
|---|---|
| **Decision** | ADR-0006 **D13**, dated 2026-08-15. Supersedes **D12** |
| **Topology** | `renvor-rs/renvor`, `renvor-rs/renvor-site`, `renvor-rs/renvor-docs`, `renvor-rs/renvor-infra` — all GitHub, all public, all canonical |
| **Surface** | GitHub is the source, review, and future CI surface for all four. **No Renvor process depends on GitLab** for source control, CI, registry, deployment, or disaster recovery |
| **Registry** | **Unchanged.** T099 and D7 stand: public application images remain planned for GHCR. The GitLab Registry is not used |
| **`renvor-docs`** | **Unchanged** — still public and deliberately commit-empty, still gated on its licence decision and **T108** |
| **Deployment** | **None.** No server, DNS, Cloudflare, Kubernetes, GHCR, or production change. T102, T106, T108, T111, T113 all remain open |

### `renvor-infra` publication (2026-08-15)

| Check | Observed |
|---|---|
| Visibility | **PRIVATE → PUBLIC**, `renvor-infra` only. The other three repositories were not modified |
| Initial commit | `aa52237f4af421e089c31cfe306faa5db7c25e08` — signed, GitHub `verified: true` (`reason: valid`), **zero parents**, no body, **zero trailers** |
| Signing key | ED25519 `SHA256:Y77mGrK4VudFhkJt+EKyCysSqH6nsp6N4GP0kIPKVTM`, matching `governance/allowed-signers` and the GitHub-registered signing key |
| Identity | author and committer both `Ahmed Anbar <admin@ahmedanbar.dev>` |
| Committed tree | `7aaf7705946b0a91b7571167adf4aef1c4ba89f4`, identical local and remote |
| Committed paths | exactly three — `.gitignore`, `README.md`, `assets/renvor-mark-v7.svg` |
| Push | explicit non-force refspec `aa52237f4af421e089c31cfe306faa5db7c25e08:refs/heads/main`; no `--force`, `--mirror`, `--all`, `--tags`, wildcard, or `-u` |
| Anonymous read | unauthenticated HTTPS `git ls-remote` with credential helpers disabled returned the exact SHA; raw `README.md` fetch returned HTTP 200 |
| Write surface | 1 collaborator (the maintainer); 0 teams, 0 deploy keys, 0 webhooks |
| Still empty of | 0 Actions runs, 0 releases, 0 environments, 0 variables, 0 Actions secrets, 0 tags, 0 forks; Pages absent |

### Content minimisation before publication

The `README.md` was rewritten for public release. **Removed**: the origin IPv4 address,
component patch versions, authoritative nameserver names, the unrelated-namespace inventory,
dated server-audit evidence, and the detailed description of absent edge protections.
**Retained**: purpose, high-level architecture, the DNS-only decision, the
additive-and-reversible principle, the workload security baseline, the no-plaintext-secret
rule, links to the three sibling repositories, and licensing. `assets/renvor-mark-v7.svg` was
preserved **byte-for-byte** (`237479dcac0732a3d2e7e072976a9a0acaa7e6134730e5db040d55fc5319ef5f`).

**This is minimisation for a newly public repository. It is not a claim that previously
published framework history became secret.**

| File | SHA-256 | Bytes |
|---|---|---|
| `.gitignore` | `52be9559f966ea6c9fb183a6899bf4e4805e4680fc16f821e34f15faf90873f9` | 1751 |
| `README.md` | `175f94ea78d5c40a935c56fee17baa5b8b76e29144dd9bf4f76727342f07fffc` | 4700 |
| `assets/renvor-mark-v7.svg` | `237479dcac0732a3d2e7e072976a9a0acaa7e6134730e5db040d55fc5319ef5f` | 543 |

**No Kubernetes manifest, deployment workflow, GitHub Actions workflow, credential, licence
file, CODEOWNERS, or dependency file was added.**

### Security audit before publication

`gitleaks` 8.30.1, redacted mode, across **all four repositories** — complete Git history
(framework 26 commits, site 14 commits) **and** filesystem including untracked files, because
`renvor-docs` and `renvor-infra` are commit-empty and history scanning alone would have been
vacuous for them. **Every scan exit 0, zero findings.** A filename-only scan for
private-key-like files found **0** across all four repositories, so no such file needed to be
opened and none was. The SVG was inspected structurally: no script, no event handler, no
`javascript:` URL, no external resource, no `<image>`/`<use>`/`<foreignObject>`, no DOCTYPE or
entity, no metadata, no base64 data URI — 1 `svg`, 1 `defs`, 1 `linearGradient`, 3 `stop`,
3 `path`.

### Protection, verified by read-back from GitHub

| Setting | Value |
|---|---|
| Ruleset | id **`20889836`**, name **`main protection`**, target branch (default), enforcement **`active`** |
| Bypass actors | **0** |
| Rules active | `pull_request`, `required_signatures`, `required_linear_history`, `non_fast_forward`, `deletion` |
| Pull-request parameters | 0 approvals required (sole maintainer), conversation resolution **required**, no CODEOWNERS review, no last-push approval, merge methods squash + rebase |
| Required status checks | **none** — the repository has no CI yet *(observed 2026-08-15. **Changed 2026-08-17T20:42:25Z**: one required check, `validate`, strict. This row is left as observed on its own date; see [`deployment-evidence.md` §5](deployment-evidence.md))* |
| Merge methods | squash ✅, rebase ✅, merge commit ❌, automatic branch deletion ❌ |
| Secret scanning | **enabled** |
| Push protection | **enabled** |
| Vulnerability alerts | **enabled** |
| Dependency graph | active |
| Paid features | **none enabled, no trial started** |

Verification was read-only. **No test force push or destructive test was performed.** Rules
were confirmed to apply to `refs/heads/main` via the repository rules endpoint.

### T114 cancelled — not passed

**T114 is closed as CANCELLED / NOT APPLICABLE.** The GitLab canonical cutover it gated was
abandoned. Recorded precisely:

- **(a)** an encrypted off-VPS backup **was created** on 2026-08-14;
- **(b)** the exact-version isolated restore proof **never completed**, and **no restore
  result was accepted**;
- **(c)** matching restored repository refs and hashes were **never proven**;
- **(d)** **no RPO or RTO figure was measured**; no GitLab RPO or RTO guarantee is claimed;
- **(e)** the separate human cutover approval was **never granted**, because the cutover was
  cancelled.

On **2026-08-15 Ahmed Anbar intentionally deleted** the local Phase 3 and Phase 4 GitLab
backup and evidence directory — **the maintainer's local backup directory** *(absolute path withheld 2026-08-15)*. **None of those local backup
artifacts is preserved.** This statement is scoped to that directory alone and makes **no
claim about any unrelated backup held elsewhere**.

**Public GitHub now provides failure-domain separation for Git repository content**, which is
what T114 existed to protect. **GitHub does not preserve GitLab-specific issues, variables,
users, logs, packages, registry content, or other GitLab metadata**, and no claim is made that
it does.

**Self-hosted GitLab was not deleted, decommissioned, or modified.** No GitLab deletion or
decommissioning is authorised by this record.

**This is the cancellation of an obsolete conditional gate. It is not successful completion of
its recovery requirements, and the checked box in `tasks.md` must never be read as one.**

### Task counts, recalculated mechanically

Counted from `specs/001-governance-foundation/tasks.md` by matching `^- \[[ xX]\] `:

| Measure | Value |
|---|---|
*(Counts as they stood on 2026-08-15 when this section was written. **They moved later the same day** — the current figures are in the Status line at the head of this ledger and in §4. Retained because §3av records a dated state.)*

| Measure | As at §3av | Current |
|---|---|---|
| Total tasks | **114** | **114** |
| Completed | **101** | **108** |
| Waived / not met | 0 | **1** — T088, under **W-003** |
| Cancelled / not applicable | **1** — T114 | **1** — T114 |
| Transferred, still non-completed | 0 | **4** — T102, T108, T109, T111 |
| Open | **12** | **0** |

**Read the current state as `108 completed, 1 waived, 1 cancelled, 4 transferred (114 total)`.**
**Counting is now by task ID and explicit status marker, not by counting checkboxes** — T114
was moved out of checkbox grammar on 2026-08-15 precisely because a checkbox count reported a
cancelled disaster-recovery gate as completed. No replacement task was added; there is no T115.

### Boundary

`ADR-0006 remains proposed pending T106. T113 remains open. Phase 001 is not complete and no Renvor 1.0 claim is made.` *(Dated 2026-08-15, true when written. ADR-0006 was accepted and T113 closed later the same day. **No Renvor 1.0 claim is made — that part still stands.**)* `This section records a repository publication, its security audit, and its branch protection only — no deployment, image publication, registry, DNS, server, Kubernetes, environment, credential, Cloudflare, GHCR, or private-key action occurred, and no self-hosted GitLab instance was accessed, modified, deleted, or decommissioned.`

*(T113 closed 2026-08-15 — §3aw. The boundary paragraph above is dated 2026-08-15 and stated
what was true when §3av was written; T113 was still open at that moment.)*

## 3aw. T113 — landing repository CI and branch protection, verified from the live repository (2026-08-15)

**T113 is closed on a read-only re-verification of `renvor-rs/renvor-site` performed
2026-08-15, not on the pull request alone.** The work landed on 2026-08-14; this section
records that it is still in force, because a gate that was configured and then silently
weakened is not a gate.

**Nothing in this section changed the site repository.** Every observation below is a read.
No push, no setting change, no workflow run triggered, no deployment.

### 3aw.1 What landed

| Item | Value |
|---|---|
| Pull request | `renvor-rs/renvor-site` **#4**, state **MERGED** |
| Merged at | **2026-08-14T18:28:38Z** |
| Reviewed head | `b6ed04d219c27a4f69526751c17a9db5aba2c575` |
| Merge commit | `d3575e5e8b5b8c16f21c6dde1578d8e9993422c4` |
| Current `main` | **`d3575e5e8b5b8c16f21c6dde1578d8e9993422c4`** — the merge commit is still the branch head, so nothing has landed on top of the reviewed state |
| Checks on the merge commit | `build`, `accessibility`, `links`, `dependencies`, `container` — all **success** |

### 3aw.2 The workflow enforces what T113 required

Read from `origin/main:.github/workflows/landing-ci.yml`.

| T113 requirement | How `landing-ci` satisfies it |
|---|---|
| CI consumes the repository-local `.nvmrc` | `actions/setup-node` with `node-version-file: ${{ env.NODE_VERSION_FILE }}` in **every** Node job — the version lives in one place and CI reads it |
| Frozen install | `pnpm install --frozen-lockfile` |
| Typecheck | `pnpm run typecheck` |
| Production build | `rm -rf build .docusaurus` then `pnpm run build`, asserting `build/index.html` and `build/404.html` exist |
| Accessibility | Playwright suite run against the **artifact downloaded from the `build` job**, not a fresh local build — accessibility is asserted about the same bytes that were typechecked and built |
| Link check | `lycheeverse/lychee-action` over `build/**/*.html` with an absolute `--root-dir` |
| Dependency scan | `trivy` filesystem scan, `scanners: vuln`, `severity: HIGH,CRITICAL`, **`exit-code: 1`** — findings fail the build |
| Container scan | `trivy` image scan of `renvor-site:ci`, `scanners: vuln,secret`, `severity: HIGH,CRITICAL`, **`exit-code: 1`**, `ignore-unfixed: false` |

**Beyond the requirement**, and recorded because it is real work: the container job smoke-tests
the image under production constraints — `--read-only`, `--cap-drop ALL`,
`--security-opt no-new-privileges`, `--user 65532:65532` — asserting `/health` 200, `/` 200,
and a missing path **404**; and the pipeline emits **two SBOMs**, a dependency SBOM and a
**runtime SBOM generated from the image** with `syft` pinned to `v1.51.0`, both SPDX-JSON,
uploaded with explicit retention and `if-no-files-found: error` so a missing artifact fails
rather than passing quietly.

**Third-party action pinning — 9 of 9.** Every `uses:` resolves to a full 40-character commit
SHA with a trailing version comment: `actions/cache`, `actions/checkout`,
`actions/download-artifact`, `actions/setup-node`, `actions/upload-artifact`,
`anchore/sbom-action`, `aquasecurity/trivy-action`, `lycheeverse/lychee-action`,
`pnpm/action-setup`. Verified mechanically by extracting every `uses:` value and testing the
post-`@` component for length 40.

### 3aw.3 Protection on `main`, read back from GitHub 2026-08-15

| Control | Observed |
|---|---|
| Pull request required | **yes** (0 approvals — the W-001 single-maintainer gap, unchanged) |
| Required status checks | **5** — `build`, `accessibility`, `links`, `dependencies`, `container` |
| Strict (branch must be up to date) | **true** |
| Administrator bypass | **none** — `enforce_admins: true` |
| Conversation resolution | **required** |
| Force pushes | **blocked** |
| Deletions | **blocked** |

**This is the exact gap T113 was opened to close.** On 2026-08-13 the same endpoints reported
`protected: false`, zero branch-protection rules, and a direct push to `main` that had
succeeded.

### 3aw.4 The task's exclusions held

T113 excluded image publication, environments, credentials, deployment, server access, and DNS
changes. Verified 2026-08-15:

- `landing-ci.yml` declares **one** top-level `permissions: contents: read` and **no job-level
  override anywhere in the file**; there is no `docker login`, no `docker/login-action`, no
  `ghcr.io` reference, no `push: true`, and no `packages: write`. The `container` job builds
  `renvor-site:ci` **locally** and never authenticates to a registry.
- `renvor-rs/renvor-site` has **0 GitHub environments**, **0 releases**, **0 tags**.
- **No site is deployed.** Measured the same day, `renvor.dev`, `docs.renvor.dev`, and
  `www.renvor.dev` each resolve to the shared origin and return **HTTP 404**, with HTTPS
  failing validation against a public trust store because Traefik serves its default
  self-signed certificate.

### 3aw.5 Observed gaps, recorded rather than glossed

**Neither was required by T113**, and neither blocks its closure — both are recorded because
the divergence between the four repositories should be a decision rather than an accident:

| Control | `renvor-rs/renvor-site` | `renvor-rs/renvor` (framework) | `renvor-rs/renvor-infra` |
|---|---|---|---|
| `required_signatures` | **false** | **false** | **enforced** (ruleset `20889836`) |
| `required_linear_history` | **false** | **enforced** | **enforced** |

> **Corrected 2026-08-15.** This table previously read "Gap on `renvor-site` / Framework
> repository: enforced, enforced", asserting that the framework repository enforced **both**.
> **It does not.** Read back from GitHub: `renvor-rs/renvor` has `required_signatures: false`
> on both the protection object and the dedicated endpoint, and holds **0 rulesets**. The
> claim was half false, and it erred in the direction that made the framework repository look
> stronger — the failure mode this ledger is written to prevent.
>
> **The sharper finding:** `renvor-infra`, the least critical of the four, is the **only**
> repository that enforces signed commits, because it was protected by a **ruleset** while the
> other two use classic branch protection configured without that control. **Commits on
> `renvor-rs/renvor` are signed in practice** — the five most recent on `main` all return
> `verified: true, reason: valid` — but that is maintainer configuration, **not server-side
> enforcement**, and an unsigned commit would not be rejected. Carried as limitation **R-16**.

Tracked in §6 (known limitations) with an owner and a target phase.

### Boundary

`T113 is complete. It proves the landing repository's own CI and branch protection, and nothing else. It does NOT prove deployment readiness and it deploys nothing — T102, T106, T108, and T111 remain open, and no Renvor site is deployed. This section records read-only observations of GitHub and of three public hostnames; no repository, workflow, protection setting, server, DNS record, registry, environment, or credential was created, modified, or deleted.`

## 3ax. T087 — runtime-capability and availability-claim audit (2026-08-15)

T087 has two halves, and **they did not both pass on first inspection.** SC-013 — no runtime
capability implemented — passed cleanly. FR-044 — no unshipped capability described as
available — **failed in four places**, and the corrections are recorded below rather than
folded in silently.

### 3ax.1 SC-013 — no runtime framework capability is implemented. **Pass.**

| Measure | Value |
|---|---|
| Rust source files, excluding `target/` and `node_modules/` | **2** — `crates/renvor/src/lib.rs`, `xtask/src/main.rs` |
| Library code in `crates/renvor/src/lib.rs`, non-comment, non-blank, excluding `#[cfg(test)]` | **3 lines** |
| Public items exported by `renvor` | **3**, all `pub const … : &str` — `VERSION`, `MSRV`, `EXECUTABLE` |
| `pub fn`, `pub struct`, `pub trait`, `pub mod`, `macro_rules!`, `extern` | **0 of each** |
| Dependencies of `renvor` | **0** |
| `unsafe` | forbidden at the workspace level (`unsafe_code = "forbid"`) |

**Method, stated because a single method can fail silently.** Two independent enumerations
were run and agreed. `cargo public-api` is **not installed**, so it was not used and is not
claimed. (1) **rustdoc** — `cargo doc -p renvor --no-deps --offline` produced exactly three
item pages: `constant.VERSION.html`, `constant.MSRV.html`, `constant.EXECUTABLE.html`.
(2) **grep** — `grep -rnE '^\s*pub(\s|\()' crates/renvor/src/` returned the same three lines,
with a control pattern proven to match first.

**Against the FR-047 exclusion list — zero of each**: application kernel, lifecycle,
configuration, error taxonomy, dependency injection, command-line behaviour, project
generation, HTTP, GraphQL, persistence, authentication, frontend or desktop output,
installable-package machinery. `xtask` declares **`publish = false`** and is a fixed
ten-step process runner — verification tooling, not framework capability — consistent with
ADR-0002's thin-by-contract rule.

### 3ax.2 FR-044 — four false availability claims found and corrected

**All four asserted that the `renvor` crate is published. It is not.** Verified read-only on
2026-08-15 against the crates.io **sparse index**, with a control proving the method:

| Name | Result |
|---|---|
| `renvor` | **HTTP 404** |
| `renvor-cli` | **HTTP 404** |
| `renover` | **HTTP 404** |
| `serde` *(control)* | **HTTP 200** |

> **A bare `curl` to the crates.io API returns 403 from a bot gate, not 404.** Read without a
> user agent, that 403 could be mistaken for "absent". Both the API and the sparse index were
> queried with an explicit user agent, and the `serde` control proves a present crate returns
> 200 through the same path.

| # | Location | Was | Now |
|---|---|---|---|
| 1 | `SUPPORT.md` | "The **published** `renvor` crate exposes version constants only" | "**Nothing is published** — neither `renvor` nor `renvor-cli` exists on crates.io" |
| 2 | `SECURITY.md` | "the **published** crate exposes version constants only" | "…and **nothing has been published to any registry**" |
| 3 | `docs/docs/intro.mdx` | "The **published** `renvor` crate exposes three constants" | "…and it is **not published**, so there is no way to install it" |
| 4 | `docs/src/pages/index.js` | "The **published** `renvor` crate exposes three constants" | same correction, on the docs-site home page |

**Root cause, recorded because it is the reusable lesson.** This exact wording was caught once
before: §3ai.4 records `framework/README.md` claiming "The published `renvor` crate exposes
three constants" and corrects it. **The fix was applied to `README.md` alone and never
generalised**, leaving four copies of the same sentence live — including the two
highest-traffic reader surfaces. **Link checking cannot catch this**: a false factual claim is
not a broken link. The defect class is *"a correction applied at the site where it was noticed
rather than across every instance of the claim"*, and the guard against it is a repository-wide
re-sweep after every factual correction, with a control proving the sweep can match.

**Generalised in this pass.** A repository-wide sweep for any assertion that a crate *is*
published found and corrected three further instances that the reader-facing sweep did not
cover: `governance/phase-001-evidence.md` ×2 and `decisions/0002` (an accepted-costs bullet now
reading "once it is published — **nothing is published today**"). One survivor is deliberate:
`PLAN.md` §26.7 "a signed release tag or the published crate" names an artifact *class* in a
build rule and asserts nothing about today.

### 3ax.3 Two further corrections made in the same pass

| Location | Defect | Correction |
|---|---|---|
| `docs/docs/governance.mdx` | Stated that **every** Phase 001 decision record "currently remains `proposed`… because one of W-002's compensating controls — all required CI checks passing — is not yet met". **Both halves stale**: five of six are `accepted`, and control 3 has been met since 2026-08-11 | Now states five accepted, ADR-0006 alone `proposed`, blocked on **T106** |
| `crates/renvor/README.md` | Linked the project home to `https://renvor.dev`, which **serves no Renvor content** *(true on 2026-08-15; **as of 2026-08-17 it serves the landing site** — see [`deployment-evidence.md`](deployment-evidence.md). The change made here was still correct: a crate README should point at the repository until the site is deployed and stable)*. This file ships inside the published crate, so it would have been the crate page's only home link — and `lychee.toml` EX-001 excludes that host, so link checking could never flag it | Points at the repository, with a note that it moves to the site once deployed |

### 3ax.4 Categories swept and found clean

Stated explicitly rather than omitted: **no** failing install instruction anywhere
(`cargo add renvor`, `cargo install renvor-cli`, `renover new` appear nowhere as usable
commands); **no** present-tense framework feature claim; **no** claim that any site is
deployed or reachable; **no** version number implying a release — everything is `0.0.0`; **no**
broken relative link in `README.md`. Each sweep was run with a control pattern proven to match
before the result was trusted.

**`PLAN.md` and `CONSTITUTION.md` CLI text is not a finding.** Describing a plan is not
claiming a capability: `PLAN.md` is headed "Program execution authority" and states Renvor
"**will be**"; the constitution uses MUST as a requirement on a framework not yet built. T087
asks for that distinction to be preserved, and it is.

### Boundary

`SC-013 is met: zero runtime framework capabilities are implemented. FR-044 required four corrections in reader-facing documents plus three generalised elsewhere, all recorded above. This audit is read-only with respect to external systems — no crate, package, image, release, or tag was published, and no repository setting, server, DNS record, or credential was created, modified, or deleted. Nothing here makes any capability available; it removes claims that something already was.`

## 3ay. T106 — maintainer ruling on the shared server's absent backups (2026-08-15)

**T106 blocked acceptance of ADR-0006.** It is resolved by a maintainer ruling taken on live,
**read-only** evidence rather than on the 2026-08-11 audit, which ADR-0006 had already demoted
to historical.

### 3ay.1 The inspection

**Strictly read-only, over SSH, on the live shared production host.** No file, service,
workload, namespace, container, Kubernetes object, DNS record, or configuration was created,
modified, or deleted. **No secret value, credential content, or private key was read**, and the
self-hosted GitLab instance was **not** accessed. An earlier attempt on 2026-08-12 failed at
authentication; the profile now in use resolves correctly, and the failure is recorded in §3ap
rather than overwritten.

### 3ay.2 Backup and snapshot capability — **none, of any kind**

| Mechanism | Observed |
|---|---|
| `restic`, `borg`, `borgmatic`, `duplicity`, `duplicati`, `rsnapshot`, `rclone`, `bacula`, `amanda`, `velero`, `kopia` | **all absent** |
| Backup cron job or systemd timer | **none** — cron holds only prune and distro-maintenance jobs; timers are distro maintenance only. `dpkg-db-backup` backs up the **package database**, not data |
| Volume-snapshot CRDs | **none** |
| LVM (`lvs`, `pvs`) | **empty** — no volume groups; root is a plain `ext4` partition |
| Filesystem snapshots | **unavailable** — `ext4`, not ZFS or btrfs |
| k3s cluster-state snapshots | **inapplicable** — the datastore is **embedded SQLite** (`state.db`), so the etcd-snapshot feature does not apply and no `etcdsnapshotfile` exists |
| `/var/backups` | Debian **alternatives database only** |

**This is not "backups are weak". There is no backup of any kind, for anything.**

> **What this section deliberately does not publish.** The ruling needs the *absence* of a
> backup mechanism, because that is the fact it turns on. It does not need per-workload
> volume names and sizes, or precise free capacity, so those are recorded in the inspection
> and withheld here. **Minimisation was applied to `renvor-infra` before publication on the
> same principle** (§3av); it is applied here for the same reason. **Several identifiers for
> this host — its address, firewall state, open ports, and component versions — are already
> published elsewhere in this repository and predate this section.** Bringing those to the
> same standard is a separate, larger task and is recorded as limitation **R-17**, not
> silently treated as handled by this note.

### 3ay.3 What is unprotected, and what is Renvor's

| | |
|---|---|
| Unrelated stateful data | **5 PersistentVolumeClaims totalling roughly 57 GiB across two unrelated namespaces. 0 StatefulSets.** *(Per-workload names and volume sizes were recorded during the inspection and are deliberately **not** published here — they identify third parties' data without adding anything the ruling needs.)* |
| Storage class | `local-path` — node-local, **no replication, no snapshot support** |
| **Renvor's footprint** | **zero** — no `renvor*` namespace, 0 PVCs, 0 workloads, 0 ingresses. Renvor is genuinely stateless here because it is *absent* here |
| Unrelated namespaces | `attaa`, `codexhub`, `portfolio`, `gitlab` (+ `cert-manager`, `default`, `kube-node-lease`, `kube-public`, `kube-system`) |
| `gitlab` namespace | **two ClusterIP service shims only** — GitLab runs entirely **outside** Kubernetes |
| Cluster-wide policy objects | **None exist for Renvor to inherit.** A Renvor deployment must therefore **create its own** `ResourceQuota`, `LimitRange`, and `NetworkPolicy` rather than relying on cluster defaults, and **NetworkPolicy enforcement must be verified on the CNI in use** before it is relied upon |
| Headroom | **Ample for two static-site namespaces** — CPU, memory, disk, and pod-slot utilisation were all measured well below capacity at the time of the ruling. *(Precise free-capacity figures are deliberately **not** published. On a host documented as having no `LimitRange` and no `ResourceQuota`, exact headroom tells an attacker how much to consume and tells this ruling nothing it does not already say.)* |

### 3ay.4 The ruling

> The absence of shared-cluster backups **does not block** future deployment of Renvor's
> **stateless** landing and documentation properties. **This does not remediate, accept
> ownership of, or make guarantees for the unrelated stateful namespaces.** Any future
> **stateful** Renvor workload remains **blocked** until separately reviewed backup and restore
> controls exist. A Renvor deployment must remain **additive, isolated, resource-bounded,
> digest-addressed, and reversible without modifying unrelated workloads**.
>
> **Addition 1 — resource-bounding and isolation must be created, not inherited.** **No
> cluster-wide `ResourceQuota`, `LimitRange`, or `NetworkPolicy` exists for Renvor to inherit,
> and no neighbouring namespace models them.** Renvor must supply its own, and **NetworkPolicy
> enforcement must be verified on the CNI in use before it is relied upon** — a NetworkPolicy
> that no controller enforces is decoration, and would make the isolation claim false while
> looking satisfied.
>
> **Addition 2 — the absence is total, which is why the ruling is narrow.** There is no backup
> of any kind, not merely no *cluster* backups. **Node loss is total loss** for all 57 GiB of
> unrelated stateful data. Renvor is exempt **only** because it is stateless and reconstructible
> from public GitHub plus a registry. **That exemption ends the moment any Renvor workload holds
> state** — a database, a cache with durable meaning, an upload volume, or a queue.

### 3ay.5 What this ruling does and does not do

| | |
|---|---|
| **Does** | Unblock **acceptance of ADR-0006**, and nothing else |
| **Does not** | Authorise any deployment. **T102, T108, and T111 remain non-completed and transferred**, and T102 must still run immediately before any deployment |
| **Does not** | Remediate the neighbours' exposure, or create any obligation of Renvor's toward it. The gap is **recorded, not owned** |
| **Does not** | Make any RPO, RTO, or recovery guarantee for anything on this host |

### Boundary

`T106 is complete. The inspection was strictly read-only: no file, service, workload, namespace, container, Kubernetes object, DNS record, or configuration was created, modified, or deleted; no secret value, credential, or private key was read; the self-hosted GitLab instance was not accessed. Nothing was deployed. This ruling unblocks ADR-0006 acceptance only — every deployment gate that was open before it remains open.`

## 3az. T088 — phase-level review, WAIVED under W-003 and NOT MET (2026-08-15)

**No independent human requirements and security review of Phase 001 has occurred.** This
section records that plainly, because the alternative — describing what did happen as though it
satisfied the requirement — is the exact failure the constitution's review rule exists to
prevent.

### 3az.1 Why a second waiver was needed

`PLAN.md` §6.1 step 10 requires "an independent requirements and security review"; constitution
§Development and Phase Workflow #7 requires that review to compare implementation evidence
against the specification, constitution, compatibility matrix, and security checklist.
`GOVERNANCE.md` defines an independent reviewer as a **person** who (1) did not author the
record, (2) did not author the change it justifies, and (3) is not directed by the author in a
way that would make declining to approve professionally costly.

**W-002 does not cover this.** W-002 waives FR-013 — *decision-record* review. T088 is a
*phase-level* gate under a different rule. Treating W-002 as covering it would have been a
technicality of exactly the kind `GOVERNANCE.md` says the project does not use.

**W-003 was granted by Ahmed Anbar on 2026-08-15**, with all seven mandatory fields, and is
recorded in `governance/waivers.md` together with the scope limits that were part of the grant.

### 3az.2 The limits of the grant, restated

- **W-003 waives only the independent-human-review requirement for Phase 001.**
- **It does not waive any finding, failed check, missing evidence, acceptance criterion, or
  security blocker.** A waiver of *who reviews* is not a waiver of *what must be true*.
- **Security release blockers are never waived.**
- **Phase 001 must receive genuine independent re-review before any public release.**
- Expires **2027-02-11**, or **immediately** when a qualified independent reviewer becomes
  available — whichever occurs first.

### 3az.3 What review actually happened — advisory, NON-INDEPENDENT

**These reviews are not independent and are never to be described as such.** They were run in
clean contexts, against explicit written requirement lists, by AI agents directed by the same
party that authored the work. That is a compensating control, not a substitute.

**They found real defects, which is the honest argument for keeping them and the reason they
must not be oversold:**

| Review | Representative findings |
|---|---|
| Requirements (PR #16) | **Two blocking**: a surviving claim that application source "stays private" when no repository is private, and a claim the landing site is "already served publicly" when no site is deployed. Both sat within a few lines of an identical sentence that *had* been corrected |
| Security (PR #16) | Cancelled disaster-recovery gate counted as completed by any checkbox parser; publication of the release signing key's filesystem path |
| Delta verification | A **blocking** error of the author's own: "returns no HTTP response", inferred from `curl` printing `000`, which is curl's own failure code and not an absence of response |
| T087 audit | **Four blocking** false statements that the `renvor` crate is published, in four separate files, from a correction applied once and never generalised |

**Every finding was individually dispositioned** — fixed, or refused with a stated reason —
rather than accepted in bulk. Where a fix was refused, the reason is recorded: generalising a
hostname inside preserved-verbatim text was declined because it would have falsified a
byte-for-byte preservation claim made in the same commit.

### 3az.4 The other half of T088 — zero open blockers

T088 also requires confirming zero open blockers. **Satisfied by categorisation, not by
rewording** — and the categorisation has **three** kinds, not two:

- **Transferred** — T102, T108, T109, T111, each with an owner and a named destination, each
  still counted as non-completed.
- **Cancelled** — T114, whose subject ceased to exist and whose requirements were never met.
- **Carried** — the shared server's absent backups. **This one is neither transferred nor
  cancelled**, and saying so matters: it is a real, unremediated exposure belonging to
  **unrelated third-party workloads**, which **T106 explicitly declined to own or fix**. It
  stays in §8, un-struck, as a standing record. *(An earlier wording of this paragraph said
  every remaining item was transferred or cancelled. That was false — it silently absorbed a
  carried exposure into two categories that both imply someone has taken responsibility.)*

**No blocker was closed by being restated.**

### Boundary

`T088 is WAIVED / NOT MET under W-003. It is not complete and must never be counted as a completed task. No independent human review of Phase 001 has occurred. The reviews performed are advisory and non-independent. Security release blockers remain unwaivable, and Phase 001 requires genuine independent re-review before any public release.`

## 3ba. T085 — full quickstart gate sequence from a clean isolated checkout (2026-08-15)

**Run from a fresh clone, not from the working tree.** The point of T085 is to prove a
contributor starting from nothing reaches a passing run, so a tree that has been built in for
days cannot be the subject. The clone was made with `--no-local`, so objects were transferred
rather than hardlinked, and its HEAD was asserted equal to the branch HEAD before any gate ran.

| | |
|---|---|
| Isolated checkout | fresh `git clone --no-local` into a scratch directory outside the work tree |
| HEAD asserted | `ab75494639960adc063ccfb97150cbe0f53b316d` — **matched** the branch HEAD |
| Platform | macOS 26.3 (Darwin 25.3.0, arm64) |
| Operator | Ahmed Anbar |

### 3ba.1 Gate results

| Gate | Validates | Result |
|---|---|---|
| **0** — repository clean before a remote exists | SC-004, SC-005 | **Pass.** `gitleaks git` **exit 0**, `gitleaks dir` **exit 0**, both "no leaks found", version **8.30.1**, each recorded separately. Branch `docs/complete-phase-001`; `git status --porcelain` **0 lines** |
| **1** — names verified before anything is claimed | SC-001 | **Pass.** 14 rows in `governance/name-availability.md` matched `available`/`owned-by-project`; **non-zero asserted**, so the check cannot pass vacuously |
| **2** — clean checkout verifies itself | SC-002, SC-003, SC-004, SC-016 | **Pass on both toolchains.** `rustc 1.94.0` → **10/10**; `rustc 1.97.1` (current stable) → **10/10**. Step 10 reports 0 untracked and 0 modified on each |
| **3** — governance discoverable, licence declared | SC-006, SC-015 | **Pass after correcting the gate itself** — see §3ba.2. All 7 governance documents present; workspace declares `MIT OR Apache-2.0`; `crates/renvor` inherits it; `resolver = "3"` explicit |
| **4** — push authorised by a scan of what is actually shipping | SC-005 | **Pass.** Both scans re-run in the isolated checkout, both **exit 0**, with a date distinct from Gate 0's run |
| **5** — protections are real, not configured-and-bypassable | SC-007, SC-008 | **Pass, read-only.** `main` requires pull requests and 4 checks, `strict: true`, `enforce_admins: true`, force pushes and deletions blocked; every `uses:` pinned to a 40-character SHA; every workflow declares `permissions`. **No push was attempted** — see §3ba.3 |
| **6** — documentation builds and links resolve | SC-012 | **Pass.** Docusaurus build succeeds; `lychee` reports **225 OK, 0 errors** across 257 links |
| **7** — release rehearsal publishes nothing | SC-010, SC-014 | **Pass.** 1 artifact, **0 publish operations**; registry re-checked 2026-08-15 → **HTTP 404** for `renvor`, `renvor-cli`, `renover`, with `serde` **200** as a control; no stored registry credential |
| **8** — evidence complete | SC-009, SC-011, SC-013 | **Pass.** §4 carries 7 of 7 PLAN criteria and 16 of 16 success criteria, all dated; all six ADRs accepted with reviewer and date; §8 holds no uncategorised blocker |

### 3ba.2 The run found a defect in the gate, not in the repository

**Gate 3 failed on first execution, and the gate was wrong.** Its licence assertion required a
literal `license = "MIT OR Apache-2.0"` in `crates/renvor/Cargo.toml`. **ADR-0002 makes
`[workspace.package]` the single authoritative declaration site and forbids members from
restating it**, so the member correctly reads `license.workspace = true` — and a crate that
*passed* the assertion would have been **violating** the ADR it was meant to protect.

Corrected to assert both halves — the workspace declares the terms, the member inherits them —
plus an explicit `resolver = "3"` check. **A gate that fails on a correct repository is as much
a defect as one that passes on a broken one**, and this one would have rewarded the exact
violation ADR-0002 exists to prevent. It was caught only because the gate was actually run from
a clean checkout rather than assumed to still hold.

### 3ba.3 Gate 5 is now read-only, and that is a change from the original sequence

The original Gate 5 ended with `git push origin main   # expected: rejected`. **That is a write
attempt against a protected production branch whose safety depends on the very configuration
under test.** It was replaced with assertions that read the protection settings. **No push was
attempted during this run**, and none is required: if a read shows protection absent, that is
the finding.

### 3ba.4 SC-016 verified across every location that states the MSRV

| Location | Value |
|---|---|
| `Cargo.toml` `[workspace.package].rust-version` | `1.94.0` |
| `rust-toolchain.toml` `channel` | `1.94.0` |
| `crates/renvor/src/lib.rs` `MSRV` | `1.94.0` |
| `SUPPORT.md` MSRV statement | `1.94.0` |

**0 mismatches.** `crates/renvor/Cargo.toml` restates `rust-version` **0 times**, as ADR-0002
requires. `resolver = "3"` is declared explicitly at the workspace root. References to `1.97.1`
and `1.98.0` elsewhere describe *current* and *next* stable in policy text and are **not** MSRV
declarations.

### Boundary

`T085 is complete. Every gate was run from a fresh isolated clone on 2026-08-15; no historical result was reused. Gate 5 is read-only and no push was attempted. Nothing was published, deployed, or configured — the only external calls were read-only GitHub queries, a crates.io registry read, and an HTTP read of three hostnames.`

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
| ~~R-6~~ | ~~Local `stable` toolchain stale at 1.94.0 (see §8)~~ | Ahmed Anbar | **Resolved 2026-08-11** — §3p; `rustc +stable` is 1.97.1 |

**Finalised 2026-08-15 (T083).** The rows below were added on that date. Each carries a named
owner and a target phase, and **none is closed by being written down**.

| ID | Limitation | Owner | Target phase |
|---|---|---|---|
| R-7 | **MSRV 1.94.0 has never been validated against real persistence dependencies.** FR-061 requires revalidation before Phase 006, because Phase 001's only crate has **zero dependencies**, so the floor is currently proven against nothing that could raise it | Ahmed Anbar | **Before Phase 006** — registered in §7 |
| R-8 | **`renvor-rs/renvor-site` does not require signed commits or linear history** (`required_signatures: false`, `required_linear_history: false`, observed 2026-08-15). T113 did not require either, so this is a divergence rather than a failure — but it is a divergence in the repository that will serve the public landing page | Ahmed Anbar | Phase 012, with the landing deployment; earlier if a second contributor joins |
| R-17 | **The framework repository publishes operational detail about a live shared production host that its own minimisation standard says to remove.** §3av records that the `renvor-infra` README was minimised before publication by removing **the origin IPv4 address, component patch versions, authoritative nameserver names, and the unrelated-namespace inventory**. This repository publishes all four, in `decisions/0006` and in dated `phase-001-evidence` sections, alongside the host's firewall state and open ports. **Every one of those predates 2026-08-15 and none was introduced by the Phase 001 convergence change** — which is why it is recorded rather than fixed here: correcting it means editing accepted decision records and dated evidence across the repository, and that is its own reviewed change. **The affected stateful workloads belong to third parties**, so the maintainer should also consider whether their owners warrant notice. *(Raised 2026-08-15 by advisory security review.)* | Ahmed Anbar | **Before any deployment or further publication** — treat as blocking for both |
| R-16 | **`renvor-rs/renvor` does not enforce signed commits.** `required_signatures` is **false** on the framework repository — the most security-critical of the four — while `renvor-infra`, the least critical, **does** enforce it via ruleset `20889836`. The cause is mechanism, not intent: the framework uses **classic branch protection** configured without that control, and holds **0 rulesets**. **Commits are signed in practice** (the five most recent on `main` all verify), so this is an **enforcement** gap, not a provenance gap — an unsigned commit would not be rejected. *(Found 2026-08-15 by advisory review, correcting an earlier claim in this ledger that the framework "enforces both".)* | Ahmed Anbar | Before the first signed release — registered in §7.4 |
| R-9 | **`renvor-rs/renvor-infra` has no CI and therefore no required status checks.** Its ruleset requires pull requests, signed commits, and linear history with zero bypass actors, but **nothing is verified** on the way in. A manifest could be merged unreviewed by machine — *__CLOSED LATE 2026-08-17, and the lateness is the record__: CI arrived in the **same commit** as the first manifest, so no manifest was ever unexamined, but the `validate` check was not **required** until 2026-08-17T20:42:25Z. **Pull requests #1–#7 all merged with it advisory.** The deadline in the Status column was therefore **missed on its required-check half**. See [`deployment-evidence.md` §5](deployment-evidence.md)* | Ahmed Anbar | The phase that adds the first manifest — protection without verification must not outlive the repository's emptiness |
| R-10 | **`renvor-rs/renvor-docs` is commit-empty, unprotected, and has secret scanning and push protection disabled.** There is no `main` branch to protect. The controls must exist **before** its first commit, not after | Ahmed Anbar | Phase 012, before the documentation migration permitted by T108 |
| R-11 | **The T095–T097 landing-approval anchors cannot be recomputed.** §3aq records a source-set SHA-256 and a build-output SHA-256 but **not the method that produced them** — no file set, no ordering, no statement of whether filenames are included. The approval is therefore not mechanically re-attachable to any later commit, and the site has since advanced to `d3575e5e…`. A hash that cannot be reproduced verifies nothing | Ahmed Anbar | Phase 012 — re-approve against a reproducible anchor before the landing deploys, and record the recipe with it |
| R-12 | **GHCR cannot be independently enumerated with the credentials in use.** The available token lacks `read:packages`, and anonymous GHCR returns HTTP 403 for absent and private packages alike. "No image has been published" therefore rests on the absence of any publishing workflow or run, **not** on a registry listing — *__SUPERSEDED 2026-08-17__: an image **has** now been published and is **anonymously pullable**, so the enumeration limitation no longer decides the question for this package. The deployed digest is `sha256:56446da7c16e155396114e185206837710eee1587d3b58ef8e5ecca96ddb84af`, verified by an anonymous pull with a clean Docker configuration carrying no credential. The limitation still holds for any package that is **not** public* | Ahmed Anbar | The phase performing the first image publication |
| R-13 | **The release signing key pair is stored in a directory synchronised to a third-party file-sync service.** Its blast radius extends beyond this machine to that account and every device syncing it. **No key was read, moved, rotated, copied, or deleted** in recording this | Ahmed Anbar | Registered in §7 as a security action; before the first signed release |
| R-14 | **Absolute local filesystem paths remain published in §3af**, disclosing a working-directory layout and a macOS account name. They are retained because §3af.3a's finding **is** the before-and-after comparison of two absolute paths; redacting them would delete the evidence. Signing-key and backup paths elsewhere were withheld on 2026-08-15 | Ahmed Anbar | Open question for the maintainer — no phase assigned |
| R-15 | **Phase 001's evidence rests on a single-maintainer, non-independent review.** W-002 covers decision records; the phase-level requirements and security review required by `PLAN.md` §6.1 step 10 has no independent human reviewer. Advisory AI review is **not** a substitute and is never described as one | Ahmed Anbar | When a qualified independent reviewer becomes available — see T088 and the waiver ledger |

## 7. Recurring obligations

**Populated 2026-08-15 (T084).** Every row carries an owner, a first due date, what triggers
it early, and the condition under which it is removed. **An obligation with no removal
condition is a permanent obligation and says so.** Dates are absolute; a condition may
accompany a date, and the obligation fires at whichever arrives first.

**Nothing in this register is discharged by appearing in it.**

### 7.1 Waiver expiries

| Obligation | Owner | First due | Early trigger | Removal condition |
|---|---|---|---|---|
| **W-001 expires** — the single-maintainer approval gap (spec FR-027; a pull request must carry an approving review from someone other than its author) | Ahmed Anbar | **2027-02-11** | **Immediately** when a second maintainer with merge rights joins | Add the second maintainer, enable the required-approving-review setting, **re-review every change merged under the waiver**, close W-001 |
| **W-002 expires** — the decision-record independent-review gap (spec FR-013) | Ahmed Anbar | **2027-02-11** | **Immediately** when a qualified independent reviewer becomes available | Raise the review requirement to a genuinely independent reviewer, **re-review every ADR accepted under the waiver**, close W-002 |
| **W-003 expires** — the Phase 001 **independent requirements and security review** gap (`PLAN.md` §6.1 step 10; constitution §7; T088), granted 2026-08-15 | Ahmed Anbar | **2027-02-11** | **Immediately** when a qualified independent human reviewer becomes available | The first qualified reviewer **re-reviews Phase 001 in full** against the specification, constitution, compatibility matrix, and security checklist; T088 is then satisfied rather than waived; W-003 closes. **Phase 001 must receive that re-review before any public release** |
| **Re-review Phase 001 before any public release** — a standing precondition created by W-003, listed separately because it fires on a *release*, not on a date | Ahmed Anbar | **Before the first public release**, whenever that occurs | Any release preparation beginning while W-003 is active | Discharged only by a genuine independent human review. **Not dischargeable by agent review, self-review, or automated checks** |

> **An expired-but-open waiver is a release blocker.** A waiver reaching its date without its
> condition being met is **not** automatically renewed; it must be re-justified and re-dated,
> or the underlying rule complied with.

### 7.2 Toolchain and dependency obligations

| Obligation | Owner | First due | Early trigger | Removal condition |
|---|---|---|---|---|
| **Quarterly MSRV policy review** (FR-060) — confirm 1.94.0 is still the correct floor and that it reads identically in every location that states it | Ahmed Anbar | **2026-11-11** *(first quarter after the 2026-08-11 policy)* | A dependency raising its own MSRV above the floor | **Permanent** — the policy is a standing commitment, not a task |
| **Revalidate MSRV 1.94.0 against real persistence dependencies before Phase 006** (FR-061; analyze finding G6; limitation R-7) | Ahmed Anbar | **Before Phase 006 begins** | Adding any dependency with a declared `rust-version` | Discharged once revalidation is recorded with the dependency set it was run against |
| **Reassess `image-size` — `GHSA-w3rx-r6r6-pgpr`, `GHSA-5p2g-fcmc-qvqq`** (both High, CVSS 7.5; no fixed version exists and upstream is archived). **T108** | Ahmed Anbar | **2026-09-11** | Docusaurus shipping a maintained replacement or a fixed release | Removed when a fixed or replaced dependency lands, **or** when the documentation toolchain no longer reaches the package |
| **Reassess `GHSA-w5hq-g745-h8pq`** (moderate, `uuid` < 11.1.1, reached only through `sockjs` ← `webpack-dev-server`). **T109** | Ahmed Anbar | **2026-09-11** | `sockjs` shipping a fix, **or** the dev server entering a deployed path | Removed when `sockjs` updates its `uuid` constraint, or when the dev-server path is eliminated. **The 2026-09-11 reassessment has NOT occurred and must not be recorded as done** |
| **Advisory triage windows** (`governance/dependency-advisory-policy.md`) — Critical or known-exploited **24 h**, High **48 h**, Medium **5 days**, Low **10 days**, from confirmed detection | Ahmed Anbar | **On every detection** | — | **Permanent** |

### 7.3 Evidence and release obligations

| Obligation | Owner | First due | Early trigger | Removal condition |
|---|---|---|---|---|
| **Evidence retention** (`governance/evidence-retention-policy.md`) — CI artifacts **90 days**; per-release evidence until the later of **seven years** after publication or **three years** after that release's supported lifetime ends; identity and provenance metadata for the **lifetime of the project** | Ahmed Anbar | **At first publication** | — | **Permanent** |
| **`renvor.dev` domain renewal** — registered in the Add Grace Period, expires **2027-08-11** (limitation R-5) | Ahmed Anbar | **2027-08-11**, acted on well before | Registrar notice | **Permanent** while the project uses the domain |
| **Package-name watch** — `renvor` and `renvor-cli` are verified but **unreserved**; a third party may claim either before first publication (limitation R-1, FR-049 forbids placeholder crates) | Ahmed Anbar | **Continuous until first publication** | Any observed registration of either name | Discharged at first crates.io publication |

### 7.4 Security actions

| Obligation | Owner | First due | Early trigger | Removal condition |
|---|---|---|---|---|
| **Harden signing-key storage** — the release signing key pair is stored in a directory synchronised to a third-party file-sync service, widening its blast radius to that account and every device syncing it (limitation R-13). **Recorded 2026-08-15 as a future security action.** No key was read, moved, rotated, copied, or deleted in recording it, and none is proposed here | Ahmed Anbar | **Before the first signed release** | Any suspected compromise of the sync account or a syncing device | Discharged when the key is held in storage that is not synchronised to a third-party service, **or** when the maintainer records a dated decision accepting the risk with its reasoning |
| **Controls before `renvor-docs` receives its first commit** — branch protection, required checks, secret scanning, and push protection must exist **before** content, not after (limitation R-10) | Ahmed Anbar | **Before the first commit to `renvor-rs/renvor-docs`** | Any attempt to migrate documentation | Discharged when the controls are configured and read back |
| **Enforce signed commits on `renvor-rs/renvor`** — the framework repository has `required_signatures: false` while `renvor-infra` enforces it (limitation **R-16**). Commits are signed in practice; enforcement is absent | Ahmed Anbar | **Before the first signed release** | Any second contributor gaining merge rights | Discharged when the framework repository rejects an unsigned commit server-side — by enabling the classic-protection setting or adopting a ruleset — verified by read-back |
| **Required checks for `renvor-rs/renvor-infra`** — its ruleset requires pull requests and signed commits but verifies nothing, because it has no CI (limitation R-9). **DISCHARGED LATE 2026-08-17T20:42:25Z, deadline MISSED.** The `validate` check (GitHub Actions app 15368, strict) is now required. It was not required when the first manifest merged at 2026-08-17T16:31:44Z, nor for the six pull requests after it | Ahmed Anbar | **Before the first manifest is merged** — *not met* | Any pull request adding a manifest | Discharged when CI exists and its checks are required — **met 2026-08-17, four hours and eleven minutes after the deadline** |

## 8. Open blockers

**The phase remains open while any row here is *uncategorised*.** *(Narrowed 2026-08-15. The rubric previously read "while any row here is present", which no closure could ever satisfy — struck-through resolved rows are retained deliberately as history, so the table is never empty.)* A row may remain here at closure **only** if it is explicitly **resolved**, **transferred** to a named destination, **cancelled**, or **carried** as an unremediated exposure with a stated owner. **A row with no category is an open blocker and blocks the phase.**

| Blocker | Blocks | Owner | Raised | State |
|---|---|---|---|---|
| ~~Excluded material in committed `HEAD`~~ | ~~T054~~ | Maintainer | 2026-08-11 | **resolved** — history rewritten, §3o. One residual `refs/codex/*` tree ref proposed for deletion |
| ~~T041 cannot reach exit 0~~ | ~~Phase closure~~ | Maintainer | 2026-08-11 | **resolved** — task order corrected, both toolchains exit 0, §3q/§3r |
| ~~T052 delivery test not confirmed~~ | ~~T052~~ | Maintainer | 2026-08-11 | **resolved** — maintainer attestation of arrival, §3t. Note: only delivery was tested; SPF/DKIM/DMARC and external-sender deliverability are **not** claimed |
| ~~T043 literal form unmet~~ | ~~T043~~ | Maintainer | 2026-08-11 | **resolved** — step 10 reports a clean tree, §3r |
| ~~**All Phase 001 decision records remain `proposed`**: W-002 compensating control 3 ("all required CI and security checks passing") cannot be met until T057–T059 create the workflows and they run. ADR-0001, ADR-0002, ADR-0003 are written and reviewed but not accepted~~ | ~~T026, T039, T040 acceptance~~ | Maintainer | 2026-08-11 | **resolved 2026-08-15** — the workflows exist and run, so W-002 control 3 is met: `main` requires `verify (1.94.0)`, `verify (stable)`, `security`, and `docs`, strict, with `enforce_admins: true`. **all six records are `accepted`** — ADR-0001 through ADR-0006, each with reviewer `Ahmed Anbar — self-review under W-002` and a review date. ADR-0006 was the last, accepted once **T106** closed the same day. *(This cell first read "5 of 6 … ADR-0006 alone remains `proposed`", which was true for part of 2026-08-15 and stale by the end of it. **§8 is a live table, so a dated-but-stale current claim here is a defect**, not history.)* |
| ~~Organization **admin role for `AhmedAnbar` on `renvor-rs` is attested, not verified** — not publicly readable unauthenticated~~ | ~~Release-control assurance~~ | Maintainer | 2026-08-11 | **resolved 2026-08-15** — verified read-only against the API: `orgs/renvor-rs/memberships/AhmedAnbar` returns `role: admin`, `state: active`, and `orgs/renvor-rs/members?role=admin` returns exactly `["AhmedAnbar"]`, so the account is an org owner and the **sole** one. **Method limit, stated:** this is GitHub's authoritative record read as that account; it is not a third-party attestation, and the single-owner fact is itself a concentration risk rather than a control |
| ~~T012 prune not authorised~~ | ~~T013, T014, T015~~ | Maintainer | 2026-08-11 | resolved — §3e |
| ~~`refs/codex/*` ref exposing excluded paths~~ | ~~Mirror-push safety~~ | Maintainer | 2026-08-11 | **resolved** — the exposing ref is gone; a later benign ref (0 excluded paths, 0 unique blobs) was deleted by exact name 2026-08-12. **Recurring**: session tooling recreates these refs, so re-check before any push |
| ~~**V7 landing page fails the release-honesty gate**~~ — present-tense claims for unbuilt capabilities, `renover` commands for an unpublished crate, zero development-status disclosure, and three dead CTA targets | ~~T095–T097~~; **any public landing deployment** | Maintainer | 2026-08-11 | **resolved as to the review itself, 2026-08-12** — the maintainer inspected the **rendered production build** and approved it; T095–T097 are complete (§3aq). **Residual, carried to limitation R-11 rather than closed silently:** §3aq anchored that approval to a source-set SHA-256 and a build-output SHA-256 but **never recorded how they were computed**, so the approval cannot be mechanically re-attached to the site's current `main` (`d3575e5e…`), which has advanced since. **The drift check could not be performed**, and no claim is made that it was. Re-approval against a reproducible anchor is required before the landing deploys |
| ~~**`framework/README.md` claimed the `renvor` crate was published** — the registry index returns HTTP 404 for `renvor` and `renvor-cli`~~ | ~~Public accuracy of the framework README~~ | Maintainer | 2026-08-12 | **resolved — corrected 2026-08-12 (§3ai.4) and committed.** Verified 2026-08-15: the correction is on `main`, the working tree is clean for that file, and `README.md` now opens with "nothing is published. Neither `renvor` nor `renvor-cli` exists on crates.io". Re-checked the registry the same day: **HTTP 404** for `renvor`, `renvor-cli`, and `renover` |
| **No `CAA` record on `renvor.dev`** — nothing constrains which CA may issue for the domain, and ADR-0006 D5 requires one | **T111**; certificate-issuance control | Maintainer | 2026-08-12 | **OPEN — TRANSFERRED 2026-08-15 to the future deployment workflow.** Policy decided and exact records drafted (§3an); **the DNS change is not authorised and was not made**, and no DNS record was created, modified, or deleted. Blocked until cert-manager HTTP-01 issuance is proven **and** Ahmed Anbar separately authorises the exact writes. **Transferring it does not close it** |
| ~~Step 9 link check fails on transport faults, not broken links~~ — HTTP 503 on one attempt, `HTTP/2 protocol error` on the next, while a concurrent job on the same commit reported `0 Errors` | ~~**T112**; pull request #11 required checks~~ | Maintainer | 2026-08-12 | **resolved 2026-08-12** — per-host concurrency 1 and a 250 ms interval for `github.com`, serialised matrix, run-scoped `GITHUB_TOKEN`, and **429 removed from `accept`**. Not weakened: no host excluded, no link rewritten, retries still bounded, fabricated URLs still fail with exit 2. §3ar |
| ~~Website-code licence and brand-asset usage terms undecided~~ | ~~T098~~ | Maintainer | 2026-08-11 | **resolved 2026-08-12** — option B: code `MIT OR Apache-2.0`, brand assets all rights reserved under `BRAND-POLICY.md`. File set validated, §3ak |
| ~~Container registry undecided~~ | ~~T099~~ | Maintainer | 2026-08-11 | **resolved 2026-08-12** — GHCR; `GITHUB_TOKEN` publishing, public image, no pull secret, digest-pinned. §3al. **Deployment still blocked** |
| **`image-size` exception has two unverifiable controls** — absence from the production runtime container and from the runtime SBOM cannot be checked because **neither artifact exists** for the documentation site | **T108**; public documentation deployment | Maintainer | 2026-08-12 | **OPEN — TRANSFERRED 2026-08-15 to Phase 012 (documentation deployment).** Verify when that image and SBOM are first produced, §3am. The Phase 001 fail-closed image-input guard stays in force. **The two High advisories remain unfixed, unsuppressed, and not waived.** *(Note: `renvor-site` now produces both a container and two SBOMs, but that is the **landing** pipeline; T108 concerns the **documentation** runtime, which still has neither.)* **Transferring it does not close it** |
| **Open npm advisories in the documentation dependency tree** — originally 5 (3 High, 2 Medium) detected 2026-08-11T23:39:22Z; **2 closed, 3 remain** per §3aa. Documentation site only; the crate that would be published has **zero dependencies** and is unaffected | Documentation site; any future release while open | Maintainer | 2026-08-12 | **OPEN — TRANSFERRED 2026-08-15.** The two High `image-size` advisories ride with **T108** to Phase 012; the moderate `uuid` advisory rides with **T109** into the recurring-obligations register (§7.2), reassessment **2026-09-11, not yet performed**. Full clause-5 advisory records are still outstanding. See §3z.10 and §3aa. **Transferring them does not close them** |
| **No backup tooling or cluster snapshots on the production VPS** — all state for the unrelated production namespaces sits in one unreplicated datastore. Pre-existing, affects unrelated workloads, outside Renvor's remit | Server reliability; **formerly ADR-0006 acceptance via T106** | Maintainer | 2026-08-11 | **CARRIED — recorded, not owned, and explicitly not remediated.** **T106 ruled on it 2026-08-15** (§3ay): the gap does **not** block deployment of Renvor's **stateless** properties, and the ruling **remediates nothing** for the unrelated namespaces. **The exposure is unchanged and is not Renvor's to close.** Any **stateful** Renvor workload stays blocked until separately reviewed backup and restore controls exist |
| ~~Local `stable` toolchain stale at 1.94.0~~ | ~~two-toolchain verification~~ | Maintainer | 2026-08-11 | **resolved** — diagnosed and repaired, §3p. `rustc +stable` is now 1.97.1 |
| ~~T006 independent-reviewer ruling~~ | ~~T026, T039, T040, T066~~ | Maintainer | 2026-08-11 | resolved — W-002, `governance/waivers.md` |
| ~~T008 candidate names~~ | ~~T019, T020, T021~~ | Maintainer | 2026-08-11 | resolved — `governance/name-availability.md` |
| ~~T009 publish-set decisions~~ | ~~T010 and the cleanup chain~~ | Maintainer | 2026-08-11 | resolved — §3a |

---

## ADR-0010 — the executable name unified with the product (2026-08-17)

**Appended, not merged into the sections above.** Those sections are dated records of what was
true when they were written, and the correct way to record a later change is to add to the
ledger rather than to edit history into agreement with the present.

### What changed

**ADR-0010 supersedes ADR-0001.** The installed executable is **`renvor`**, matching the
product and the facade crate; the primary command is `renvor new` and the package command is
`renvor add`. `renvor::EXECUTABLE` changed from `"renover"` to `"renvor"`.

The argument is ADR-0001's own, applied consistently. ADR-0001 rejected renaming the *product*
to `Renover` because *"`Renover`/`renovate` is a closer pair than `Renvor`/`renovate`"* — then
left the *executable*, the string users type, at the closer spelling. Measured 2026-08-17:
Levenshtein `renover`↔`renovate` = **3**, `renvor`↔`renovate` = **4**.

### Decision-record set — corrected

| Record | State on 2026-08-17 | Reviewer |
|---|---|---|
| ADR-0001 | **`superseded`** by ADR-0010 | `Ahmed Anbar — self-review under W-002` |
| ADR-0002 … ADR-0006 | `accepted` | `Ahmed Anbar — self-review under W-002` |
| **ADR-0010** | **`accepted` 2026-08-17** | `Ahmed Anbar — self-review under W-002` |

**Seven Phase 001 records: six `accepted`, one `superseded`.** Statements elsewhere in this
file that there are six, or that the set is ADR-0001…ADR-0006, are true of their dates.

**SC-009 still holds** — 0 records accepted without a recorded reviewer and review date. A
`superseded` record is not an unaccepted one: ADR-0001 was accepted on 2026-08-12 with both
fields recorded, and superseding it did not remove them.

### W-002 controls, run before acceptance

| # | Control | Result |
|---|---|---|
| 1 | Written alternatives-and-consequences review before acceptance | ✅ Six alternatives with rejection reasons; four accepted costs |
| 2 | Verification against `checklists/governance.md` | ✅ **79/79**, 0 unchecked. **Found a real defect** — see below |
| 3 | All required CI and security checks passing | ✅ On head `dcdf59b1e9a918ceab718ced164aa621ab91b4d5`: **13 passed, 1 skipped** (`attest rehearsal artifacts`, `push`-gated by design); **0** unresolved conversations; **0** open CodeQL alerts |
| 4 | A dated review record stored with the ADR | ✅ ADR-0010 §Acceptance gate, dated 2026-08-17 |

**Sequence, stated because it is the control that matters most:** ADR-0010 was pushed
`proposed` in **PR #21**, merged as `f9ec01e1ee75cd943f6e6d463d1691cd7a2570c5`; the controls
were then run against that merged state; acceptance followed in a separate change. Phase 002
made the opposite mistake once — acceptance text written before the reviews returned — and it
is not repeated here.

### What control 2 found, and what was done about it

The checklist verification surfaced a defect the author's own impact analysis had missed:
**spec FR-005 mandates the very distinction ADR-0010 removes.**

> *"FR-005: An accepted decision record MUST explain the intentional distinction between the
> product name and the installed executable name…"*

ADR-0010's impact analysis enumerated the constitution, `PLAN.md`, the public-identity
contract, and the documentation — and did not reach FR-005. It was found by running a control
that consults a different artifact than the author was reading, which is the entire reason to
have one.

**Dispositioned:** FR-005 carries a dated amendment note recording that its first clause is
satisfied by ADR-0010 (the requirement is that the naming decision be justified in a record,
and it is) and that its **second clause is unchanged and still binding** — documentation,
tests, and examples must use the executable name consistently. The accepted text of a closed
phase is annotated, not rewritten.

**A second defect of the same shape was fixed alongside it.** Gate 8 in
`specs/001-governance-foundation/quickstart.md` checked *"all six decision records — ADR-0001
through ADR-0006"* — a hard-coded range that silently stopped covering the set the moment
ADR-0010 was added, and whose omission would have been invisible. It now enumerates the
directory and prints each record's state, so the gate cannot fall behind the set again.

### Risk movement

| Risk | State |
|---|---|
| **R-2** — confusability with `renovate` | **Open, reduced** — the typed string moves from distance 3 to 4 |
| **R-3** — bounded clearance of `renover` | **Retired** — the name is no longer used |
| **R-3a** — bounded clearance of `renvor` | **Open** — same bound, now on the name in use. Eight probes on 2026-08-17, each with a positive control; probe 7 additionally bounded because GitHub code search does not index this repository |
| **R-4** — no trademark search for `Renvor` | **Open, unchanged** |
| **R-5** — `renvor.dev` renewal | **Open, unchanged** |

### Still owed

**W-002 does not close.** ADR-0010 joins the set of records a qualified independent reviewer
must re-review in full when one becomes available. **No independent human review of ADR-0010
has occurred**, and the review recorded above must not be described as independent. Six waivers
remain active; the underlying problem is unchanged.
