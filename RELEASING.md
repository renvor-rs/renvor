# Releasing Renvor

**Audience**: the release approver. **Satisfies**: FR-032 – FR-034, FR-038 – FR-041, FR-045, FR-046.

> ## Phase 001 publishes nothing
>
> **No Renvor crate has ever been published. No tag exists. No release exists.** Phase 001
> rehearses this procedure and publishes none of it. Every command below that would reach
> the registry is either not run at all in this phase, or is run with `--dry-run`.
>
> The rehearsal is real evidence — it packages, verifies, and inspects the artifact — but
> it deliberately stops one step short of the registry. Treat any claim that a Renvor
> version is available as false until this document's release ledger says otherwise.

This document is the procedure, not the policy. Where a number or a rule belongs to a
policy, this document **references** the governing document instead of restating it. A
second copy of a duration or a licence rule is a second thing to forget when the first
changes, and reviewers then trust the stale one.

| Governing document | Owns |
|---|---|
| [`governance/evidence-retention-policy.md`](governance/evidence-retention-policy.md) | How long every class of release evidence is kept |
| [`governance/dependency-advisory-policy.md`](governance/dependency-advisory-policy.md) | Advisory triage and remediation deadlines against dependencies |
| [`SECURITY.md`](SECURITY.md) | Inbound private vulnerability reports about Renvor itself |
| [`deny.toml`](deny.toml) | The enforced licence and dependency allow-list |
| [`contracts/support-policy.md`](contracts/support-policy.md) | **Normative** — MSRV, supported platforms, and the support window. [`SUPPORT.md`](SUPPORT.md) summarises it |
| [`contracts/package-metadata.md`](contracts/package-metadata.md) | Required manifest fields and the rehearsal contract |

---

## 1. Gates — every one must pass

A release proceeds only when **all** of the following hold. There are no conditional
gates: a check that cannot run is a failure, never a skip.

| # | Gate | How it is proven |
|---|---|---|
| 1 | `cargo xtask verify` exits **0** on the declared MSRV | CI job `verify (1.94.0)` |
| 2 | `cargo xtask verify` exits **0** on current stable | CI job `verify (stable)` |
| 3 | Secret scanning finds nothing, history and working tree | CI job `security` |
| 4 | Documentation builds and every link resolves | CI job `docs` |
| 5 | `cargo deny check` passes all four sections | Inside gate 1, verification step 6 |
| 6 | No open **Critical** or **High** dependency advisory | [`governance/dependency-advisory-policy.md`](governance/dependency-advisory-policy.md) §7 — these **cannot be waived** |
| 7 | The release commit is **signed**, and the release **tag is signed** | §6 |
| 8 | The release runs from the **protected release environment** | §7 |
| 9 | The independent evidence archive exists and its restore test has passed | §11 — **fails closed today** |
| 10 | No open blocker in `governance/phase-001-evidence.md` | Manual review by the release approver |

**Exit code 2 from `cargo xtask verify` is not a pass.** It means required tooling was
missing and *no steps ran at all*.

### Clean checkout is mandatory

The release is built from a **freshly created checkout of the exact tagged commit**, in a
directory that has never been used for development.

```sh
tmp="$(mktemp -d)"
git clone --no-local --depth 1 --branch "v<VERSION>" \
    https://github.com/renvor-rs/renvor.git "$tmp/renvor"
cd "$tmp/renvor"
```

This is not ceremony. `cargo package` honours the `include` list, but a development
directory accumulates untracked files, stale build output, editor state, and local
configuration; building there means the thing you inspected is not the thing you shipped.
`--no-local` forces a real object transfer rather than a hardlink farm, so the clone is a
genuine independent copy.

**Never pass `--allow-dirty`.** It exists to package uncommitted work, which is the exact
opposite of a reproducible release.

---

## 2. Version and changelog rules

- **Semantic versioning**, no exceptions.
- **A published version is immutable.** crates.io will never let a version be overwritten
  and the code cannot be deleted. The only remedy for a defective release is **yank and
  replace with a new version** (§9). Plan accordingly: there is no undo.
- Pre-1.0, the public API may change in a minor release; that is what pre-1.0 means, and
  [`SUPPORT.md`](SUPPORT.md) states it as a promise rather than leaving it implied.
- **An MSRV raise is a minor or major release only**, never a patch, and requires an
  accepted decision record naming the concrete requirement that forces it. The rules live
  in [`contracts/support-policy.md`](contracts/support-policy.md); this document does not
  restate them.
- **Every release updates the changelog before the tag is created**, not after. A
  changelog written after the fact describes what someone remembers shipping.
- The changelog records, per version: added, changed, deprecated, removed, fixed, and
  **security**. A security entry cross-references the advisory identifier.

---

## 3. Package inspection — before anything is published

Run from the clean checkout of §1.

```sh
cargo package -p <crate> --list          # 1. exactly what would ship
cargo package -p <crate>                 # 2. build the .crate archive
cargo publish -p <crate> --dry-run       # 3. full publish verification, no upload
shasum -a 256 target/package/<crate>-<version>.crate   # 4. checksum
```

**Step 1 is reviewed by a human, line by line.** The file list is short by design — the
manifest states an explicit `include` list rather than inferring one — so reading it is
cheap and there is no excuse for skipping it.

Reject the release if the list contains any of:

- credentials, tokens, keys, or `.env` files of any kind;
- local or editor configuration;
- build output, generated files, or caches;
- internal specifications, planning documents, or governance working files;
- absolute paths, home-directory names, or anything else that leaks the build machine;
- assets whose licence differs from the crate's (brand assets in particular are **not**
  covered by `MIT OR Apache-2.0`).

Then confirm, against
[`contracts/package-metadata.md`](contracts/package-metadata.md):

- every required manifest field is present and resolving;
- `rust-version` matches [`contracts/support-policy.md`](contracts/support-policy.md) exactly;
- the licence is `MIT OR Apache-2.0` and nothing else;
- **no publishable package carries a git dependency, or a path-only dependency.** An
  intra-workspace dependency **must** carry both keys — `{ path = "../x", version = "0.0.0" }` —
  because cargo rewrites it to the version requirement at publish time and drops the path. A
  `path` with no `version` tells the registry nothing about what to resolve, which is the
  *path-only* case FR-040 prohibits. `xtask` is exempt only because it declares
  `publish = false`; if that line is ever removed, this rule binds it.

  Checked mechanically by `cargo xtask verify` step 7, not only by reading this list.

The `.crate` archive has a hard **10 MB** registry limit. A release approaching it is a
signal that something is being shipped that should not be.

---

## 4. Topological publication order

Packages publish **dependencies first**, waiting for the registry index to serve each one
before the next begins. A dependent published before its dependency is visible fails to
verify, and the failure looks like a transient network error rather than an ordering
mistake.

Current order:

| Position | Package | Depends on | Notes |
|---|---|---|---|
| 1 | `renvor-core` | *(nothing in the workspace)* | The kernel. Nothing else can publish before it |
| 1 | `renvor-error` | *(nothing in the workspace)* | The public API error registry and RFC 9457 documents. Depends on **no** Renvor crate — it names no transport and no kernel type — so it shares position 1 with the kernel and the two may publish concurrently |
| 2 | `renvor-config` | `renvor-core` | The configuration adapter |
| 2 | `renvor-validation` | `renvor-error` | The validation boundary. Independent of the kernel |
| 3 | `renvor-database` | `renvor-core`, `renvor-validation` | The persistence **ports**. Names no driver, so it can be depended on by an application that has not chosen one |
| 3 | `renvor-openapi` | `renvor-validation`, `renvor-error` | Description generation. Waits for the validation boundary, whose schema values it embeds |
| 4 | `renvor-testkit` | `renvor-core`, `renvor-database` | The test harness. **Moved from position 2 in Phase 007**: it now hosts the shared persistence contract both adapters are measured against, so it publishes after the ports |
| 4 | `renvor-sqlx` | `renvor-core`, `renvor-database`, `renvor-validation` | The direct-SQLx adapter. Publishes after the ports it implements |
| 4 | `renvor-seaorm` | `renvor-core`, `renvor-database`, `renvor-validation` | The SeaORM adapter. **A sibling of `renvor-sqlx`, not a dependant** — neither names the other, which is what keeps a SeaORM application's graph free of a direct-SQLx crate. Same position, and the two may publish concurrently |
| 4 | `renvor-http` | `renvor-core`, `renvor-error`, `renvor-validation`, `renvor-openapi` | The REST transport. It **adapts** all three Phase 005 contracts to HTTP, so it publishes after every one of them |
| 5 | `renvor` | `renvor-core`, `renvor-config`, `renvor-http`, `renvor-error`, `renvor-validation`, `renvor-openapi` | Facade. `renvor-config` is optional-but-default-on; the other four are optional-and-default-**off**, and `transport-rest` enables `renvor-http`, `renvor-error`, `renvor-validation` and `renvor-openapi` together. **All six** must exist first |
| — | `xtask` | *(nothing)* | **Never published** — `publish = false` |

> **Extended 2026-08-24 (Phase 006).** `renvor-database` and `renvor-sqlx` join the table, and they
> are publishable for a **different reason** from every crate above them. Those are *forced*: the
> facade reaches them, and ADR-0008 records by experiment that a publishable package cannot depend
> on an unpublishable one. The facade does **not** depend on either of these — persistence is
> deliberately outside it, so a project wanting no database resolves no driver. They are publishable
> because an **application** depends on them directly: `renvor-database` declares the ports a
> repository is written against, and `renvor-sqlx` is the adapter an application names in its own
> manifest with exactly one driver feature.
>
> The release-dry-run guard caught the omission on the first push of that branch, which is the third
> time it has done so and the reason the list is pinned rather than derived.

> **Extended 2026-08-24 (Phase 007).** `renvor-seaorm` joins at position 4, beside `renvor-sqlx`
> rather than after it. The two adapters implement the same ports against different programming
> models and neither depends on the other; `xtask` step 7 asserts both directions with a control.
>
> `renvor-testkit` **moved from position 2 to position 4**, because it gained a dependency on
> `renvor-database`. It hosts `renvor_testkit::persistence`, the contract functions both adapters'
> suites call, which is what makes *"both SeaORM rows pass the same application contracts as direct
> SQLx"* a fact about the build rather than two suites that agree by inspection. A crate cannot
> publish before something it depends on, so the move is forced rather than cosmetic.
>
> This time the publishable-count assertion in `xtask` reported the new crate **before** any list
> was edited, which is the outcome that assertion was pinned for.

> **Corrected 2026-08-23.** The facade row previously listed three dependencies and said "all
> three". `renvor` declares **six** workspace dependencies: the `transport-rest` feature enables
> `renvor-error`, `renvor-validation` and `renvor-openapi` alongside `renvor-http`. **The ordering
> was and remains valid** — all three omitted packages sit at positions 1–3, ahead of position 5 —
> so nothing about publication order changes. What was wrong was the description of it, and a
> release table that under-describes its own constraints is the same defect class as one that gets
> the order wrong. Found by maintainer self-review during the Phase 005 closing audit.

Packages sharing a position have no dependency on each other and may publish in any
order, or concurrently. Each later position waits for **every** package at every
earlier one — an optional dependency still has to be resolvable at publish time.

**Eight publishable packages.** `xtask` step 7 asserts that count against the actual manifests, so
a package added without appearing in this table fails verification rather than being discovered at
publication time.

> **Amended 2026-08-23 (Phase 005).** Three packages joined, taking the publishable set from five
> to eight, and `renvor-http` moved from position 2 to position 4 because it now depends on all
> three of them.
>
> `renvor-error` sits at **position 1 beside the kernel**, which is the non-obvious part and is a
> property worth stating: a public API error code is a compatibility promise, so it depends on no
> transport — and it turns out it needs no kernel either. Nothing in the registry or the RFC 9457
> document model names a Renvor type outside its own crate. `tests/boundary.rs` in `renvor-http`
> asserts that direction for all four crates, and this ordering is the release-time consequence.
>
> `renvor-validation` and `renvor-openapi` are `publish = true` for the reason ADR-0008 records by
> experiment: a publishable package cannot depend on an unpublishable one, and `renvor-http` — which
> is itself forced to be publishable by the facade — depends on both.

> **Amended 2026-08-22 (Phase 004).** `renvor-http` was added at position 2 and the facade's
> dependency list grew to three. **A default-OFF optional dependency still blocks the facade's
> publication**, which is the non-obvious half: `cargo publish --dry-run` resolves every optional
> dependency regardless of whether its feature is enabled, so `transport-rest` being off by default
> buys a consumer a smaller graph and buys the release nothing.
>
> `renvor-http` is `publish = true` because ADR-0008 records, by experiment, that a publishable
> package cannot depend on an unpublishable one. It is forced rather than chosen. `renvor-cli`
> stays `publish = false` and is unaffected — nothing publishable depends on it.

> **Corrected 2026-08-16 (T119).** This table previously listed `renvor` alone, at position 1,
> "declares no dependencies at all", with a note that the workspace "contains exactly one
> publishable package with zero dependencies, so the order is trivial today". That was true in
> Phase 001 and became false in Phase 002, which gave the facade its first dependencies and added
> three publishable crates. Following the stale table would have published the facade first, and
> it would have failed against the registry with a message about a missing `renvor-core` that
> reads like a network problem rather than an ordering mistake — the exact failure the paragraph
> above warns about.

> **Run the rehearsal on a clean `target/package`.** Recorded 2026-08-22, after it cost an hour.
>
> A prior **single-crate** `cargo package -p <crate> --list` leaves a partial staging area behind,
> and a later `cargo publish --dry-run --workspace` can then fail while verifying the facade with:
>
> ```text
> error: failed to verify package tarball
> Caused by: no hash listed for renvor-config v0.0.0
> note: this is an unexpected cargo internal error
> ```
>
> The message names a crate that has nothing to do with the cause, and `cargo` itself labels it an
> internal error — so it reads like a defect in whichever crate was added most recently. It is not.
> `rm -rf target/package` and re-run.
>
> **CI is unaffected**, because it always starts from a clean checkout. This is recorded anyway:
> the rehearsal is run locally before a release, and a maintainer who hit this without knowing
> would reasonably conclude the new crate had broken publication.

**Between each package**: wait for the index, then verify.

```sh
cargo info <crate>@<version>     # confirm the index serves the new version
```

Do not proceed on a timer. Proceed on evidence.

---

## 5. First publication — the bootstrap

**crates.io Trusted Publishing cannot be configured for a crate that does not yet exist.**
The trusted-publisher record attaches to a package, and there is no package until
something publishes one. The first release of each new crate name is therefore
**manual**, with a temporary token. This is a documented constraint of the registry, not a
shortcut.

The procedure, in order, with no step omitted:

1. **Complete every gate in §1 first.** The bootstrap does not lower the bar.
2. Create an API token at <https://crates.io/settings/tokens> with the **narrowest scope
   the registry offers**: `publish-new` for a name that does not yet exist, scoped to that
   single crate, and with the **shortest available expiry**.
3. **Create it immediately before use.** A token created in advance is a long-lived
   credential wearing a temporary label.
4. The token is **never** committed, **never** stored as a repository or organization
   secret, **never** placed in a workflow file, and **never** echoed into a log. It exists
   in the operator's shell session and nowhere else.
5. Publish from the clean checkout of §1, one package at a time, in the §4 order.
6. Verify the published artifact: `cargo info`, the rendered docs.rs build, and the
   checksum against the one recorded in §3.
7. **Revoke the token immediately**, at
   <https://crates.io/settings/tokens>, and **record the revocation with a UTC timestamp**
   in `governance/phase-001-evidence.md`. Revocation is a release step, not cleanup — a
   release is not complete while the token still exists.
8. Confirm at the registry that the token no longer appears.

> **No long-lived registry credential may exist anywhere** — not in the repository, not in
> its workflows, not in its secrets, not in an environment (FR-033). The bootstrap token
> is the single exception, it is minutes old when used, and it is dead before the release
> is declared complete.

### Every subsequent release uses Trusted Publishing

Once the crate exists, configure a trusted publisher on the crates.io crate settings page,
bound to:

- the exact repository `renvor-rs/renvor`;
- the exact workflow filename;
- the protected release environment of §7.

The publishing job then exchanges a GitHub OIDC token for a short-lived registry token
via `rust-lang/crates-io-auth-action`, and declares `id-token: write` — **that job and no
other**. No stored registry token is involved at any point.

**Do not skip back to a token** because trusted publishing failed. Fix the binding.

---

## 6. Signing

| Object | Requirement |
|---|---|
| Release commit | **Signed** |
| Release tag | **Signed, annotated** — a lightweight tag carries no signature to verify |
| Verification | The signature verifies against a key published on the maintainer's GitHub account |

Tags are named `v<VERSION>` and are created **only** on a commit that has passed every
gate in §1.

```sh
git tag -s "v<VERSION>" -m "Renvor <VERSION>"
git verify-tag "v<VERSION>"        # verify before pushing, not after
```

`git verify-tag` is part of the procedure. Creating a signature and never checking it
proves only that the signing command exited zero.

**Vigilant mode must be enabled** on the maintainer's GitHub account, so that unsigned
commits attributed to that account are displayed as unverified rather than displayed
neutrally. Without it, an unsigned commit and a signed one look alike to a casual reader,
which removes most of the value of signing at all.

### The signing identity

| Property | Value |
|---|---|
| Algorithm | Ed25519 (`ssh-ed25519`) |
| Fingerprint | `SHA256:Y77mGrK4VudFhkJt+EKyCysSqH6nsp6N4GP0kIPKVTM` |
| Purpose | **Signing only** — never an authentication, deploy, or login key |
| Approved principals | [`governance/allowed-signers`](governance/allowed-signers) |

Verification is bound to that tracked file through the repository-local
`gpg.ssh.allowedSignersFile` setting. This matters: `git verify-tag` without it confirms
only that *a* key signed the object. The allowed-signers file is what makes it confirm
*who*.

### The tag gate is a workflow, not a ruleset

[`.github/workflows/release-tag-verify.yml`](.github/workflows/release-tag-verify.yml)
validates the tag name, requires an **annotated** tag object, runs `git verify-tag` against
the tracked allowed-signers file, and pins both the principal and the fingerprint. A future
publish workflow depends on it through `workflow_call`, so packaging, attestation,
environment approval, publication, and deployment all sit behind it.

> **A GitHub tag ruleset does not do this.** Rulesets can restrict who creates, updates, or
> deletes a tag, and the "require signed commits" rule can target tags — but that rule
> checks **commit** signatures, not the signature on the annotated **tag object**. No
> ruleset rule verifies a tag object's signature. Relying on one would leave the release
> gate open while appearing closed.

---

## 7. Protected release environment

Publication runs from a GitHub **protected environment**, which is what makes "the
approver approved this release" a platform-enforced fact rather than a convention.

| Control | Setting |
|---|---|
| Required reviewers | **Named individuals**, never a role or a team alias |
| Deployment branch and tag policy | **Restricted to release tags** matching `v*` |
| Administrator bypass | **Disabled** |
| Environment secrets | **None** — trusted publishing means there is nothing to store |

The environment is the only place the publish job may run. A publish job that can run
outside it has no approval gate.

> **Status**: the **`release` environment exists** (created 2026-08-12, T072). Read back
> from the API: required reviewer `AhmedAnbar`, one deployment policy of type `tag`
> matching `v*`, administrator bypass **disabled**, wait timer 0, **zero** secrets and
> **zero** variables.
>
> **Prevent self-review is deliberately disabled.** With a single maintainer, enabling it
> would make every release impossible — the person who triggers the run would be the only
> person able to approve it. The gate still forces a deliberate, logged confirmation and
> restricts publication to `v*` tags. **It is not four-eyes review**, and this document does
> not pretend otherwise. Revisit when a second qualified maintainer joins.

---

## 8. Release evidence

Every release emits **all** of the following. A missing item blocks the release.

| Artifact | Produced by |
|---|---|
| The `.crate` archive | `cargo package` |
| SHA-256 checksum of every artifact | `shasum -a 256` |
| CycloneDX software bill of materials | `cargo cyclonedx` |
| Build provenance attestation | `actions/attest-build-provenance` |
| SBOM attestation | `actions/attest-sbom` |
| The resolved dependency set | The committed lockfile |
| Toolchain version, platform, operator, and date | Recorded in the evidence ledger |

Attestations are verified before the release is declared complete:

```sh
gh attestation verify <artifact> --repo renvor-rs/renvor
```

An attestation that is generated and never verified is a file, not a control.

**Retention is governed by
[`governance/evidence-retention-policy.md`](governance/evidence-retention-policy.md),
which is authoritative.** This document does not restate its periods. Two of its rules
change how a release is run, so they are named here as procedure:

- **GitHub Actions artifacts are evidence transport, not the archive.** Anything that must
  outlive the build is copied into a retained location *before* its Actions retention
  expires. Discovering this after expiry is discovering it too late.
- **The canonical public copy is the immutable GitHub Release.**

---

## 9. Rollback, yank, and advisories

**There is no rollback in the sense of undoing a publication.** A published version stays
published forever. Plan for replacement, not reversal.

| Situation | Action |
|---|---|
| Defective release, no security impact | Publish a **fixed higher version**. Yank the defective one so new dependants cannot select it |
| Security defect in a released version | Publish the fix, yank the affected version, and **publish an advisory** |
| Bad release caught before the tag is pushed | Delete the local tag and fix. Nothing is public yet — this is the only free rollback that exists |
| Bad release caught after the tag is pushed but before publication | Publish nothing. Delete the tag, record why, and cut a new version. Do not reuse the version number |

```sh
cargo yank --version <VERSION> <crate>          # prevents new dependants
cargo yank --version <VERSION> <crate> --undo   # reversible if yanked in error
```

**Yanking does not delete anything.** Existing `Cargo.lock` files continue to resolve the
yanked version, and the source remains downloadable. It stops *new* dependants; it does
not protect *existing* ones. Anyone already depending on the yanked version must be
reached through the advisory, not through the yank.

**Advisory handling** — including the deadline that applies once a defect is confirmed —
is governed by
[`governance/dependency-advisory-policy.md`](governance/dependency-advisory-policy.md)
for defects in dependencies, and by [`SECURITY.md`](SECURITY.md) for defects reported in
Renvor itself. The two have different clocks and neither supersedes the other. This
document does not restate either set of durations.

A missed deadline is **recorded**, with the reason, rather than allowed to pass unremarked.

---

## 10. The non-publishing rehearsal

[`.github/workflows/release-dry-run.yml`](.github/workflows/release-dry-run.yml) exercises
this procedure up to, and never including, publication.

**It is structurally incapable of publishing.** Its top-level permission is
`contents: read`; it holds no registry token; it creates no tag; it creates no GitHub
Release; and the only `cargo publish` invocation it contains carries `--dry-run`. The
attestation job elevates to `id-token: write` and `attestations: write` — the documented
minimum for `actions/attest-*` — and to nothing else. It never receives `contents: write`
or `packages: write`.

Its permission boundary is a control, not a convenience. Verify it before changing it.

---

## 11. The archive gate — currently fails closed

[`governance/evidence-retention-policy.md`](governance/evidence-retention-policy.md) §7
requires a **second, independently controlled archive** of release evidence before the
first real registry publication, with **all** of: independent control, encryption at rest,
versioning, access logging, and a **passing annual restore test**.

> ### No such archive exists today.
>
> **The release gate therefore fails closed.** No crates.io publication may occur until
> the archive exists and its restore test has passed.

"We will set it up after the release" is not an accepted outcome. The archive exists to
protect a release that has already shipped; standing it up afterwards protects nothing
that was at risk in the interval. The storage provider is undecided and may be chosen
later — but the gate binds before the first publication, not after.

This is not a Phase 001 obligation to discharge, because Phase 001 publishes nothing. It
binds from the first published prerelease onward.

---

## 12. Open decisions blocking a first release

| # | Decision | Task |
|---|---|---|
| 1 | Signing method — dedicated SSH key or dedicated GPG key | T071 |
| 2 | Protected environment configuration, including reviewer behaviour for a single maintainer | T072 |
| 3 | The independent evidence archive and its provider | Phase 013 |

Until all three are closed, this document describes a procedure that **cannot yet be
executed**, and says so rather than reading as though it were ready.
