# Support Policy

This is a public promise.

> **This page is the human-facing summary.** The **sole normative authority** is the versioned
> contract [`contracts/support-policy.md`](contracts/support-policy.md). Where this page and
> that contract disagree, **the contract wins** — and the disagreement is a defect in this page,
> to be fixed here rather than argued from.
>
> The three-platform claim below is carried by
> [`ADR-0011`](decisions/0011-support-linux-macos-and-windows.md).

It changes only through the process described at the end of this document.

## Current status

**Renvor is pre-release and unpublished.** **Nothing is published** — neither `renvor` nor
`renvor-cli` exists on crates.io, and there is no way to install Renvor.

The repository contains a working **transport-independent kernel** as of Phase 002:
application lifecycle, provider resolution, layered configuration, health, and a failure
injection harness. It has **no transport** — no HTTP, no database, no CLI — so it can start
and stop an application but cannot yet serve anything.

**Every API is explicitly unstable (FR-036)** and carries no compatibility promise before
`0.1.0`. Everything below is the *support contract* that will govern the framework as it is
built — established before the code, deliberately, so the promise is not retrofitted around
whatever happened to be convenient.

## Supported Rust versions

| Field | Value |
|---|---|
| **Minimum supported Rust version (MSRV)** | **1.94.0** (released 2026-03-05) |
| Kind | **Fixed floor** — not N-3, N-4, or any offset from current stable |
| Also tested | **The current stable channel** — resolved and recorded by CI at run time, not pinned here |
| Edition | 2024 |
| Cargo resolver | 3, declared explicitly in the virtual workspace |

### What "fixed floor" means

**A new Rust stable release does not invalidate the MSRV.** It does not shorten it, and it
does not trigger a review. The minimum-version CI job stays pinned at 1.94.0; only the
stable job moves.

**No stable version number is stated on this page.** A number written into a document does not
float: it is correct on the day it is typed and silently false afterwards, with no way for a
reader to tell which day they are on. The stable channel is whatever CI resolves it to on the
run that produced the evidence, and the run records it.

This was chosen deliberately over a release-count window. Rust 1.98.0 promoted to stable
on 2026-08-20, which would have pushed 1.94.0 to four releases behind and violated an
"N-3" policy **through the calendar alone** — no code change, no dependency change, no
decision by anyone. A fixed floor removes release count from the policy entirely, and
means you can read **the floor** here rather than compute it from a release schedule.

### Where it is declared

`rust-version` is declared **once**, at `[workspace.package]` in the root manifest.
Every member inherits it with `rust-version.workspace = true`. No second independent
declaration exists, and that is asserted mechanically rather than trusted.

## Supported panic strategy

| Field | Value |
|---|---|
| **Supported** | **`unwind`** — the Rust default |
| **Unsupported** | **`panic = "abort"`** — refused at compile time |

Renvor sets no `panic` key in any profile, so the default applies unless a consumer
overrides it.

### Why abort is unsupported rather than merely discouraged

Contract C-L9 and success criterion SC-009 require a panicking provider or readiness
contributor to be **contained** and reported as a failure. Both containments are built on
`std::panic::catch_unwind`, which catches panics that *unwind*. Under `panic = "abort"` a
panic calls the abort handler and the process ends with no unwinding for a landing pad to
intercept — so there is nothing to catch, by Renvor or by anyone.

That made a build profile capable of silently removing the kernel's central guarantee. A
consumer who never read this document would get an application that ended on a
misbehaving readiness check with no indication that containment had ever been promised.

`renvor-core` therefore **refuses to compile** under `panic = "abort"`:

```text
error: renvor-core does not support `panic = "abort"`.
```

A consumer who needs `panic = "abort"` needs a kernel that does not promise panic
containment. That is a different product, not a configuration of this one.

## Supported platforms

**Normative statement**: [`contracts/support-policy.md` §Platform support](contracts/support-policy.md).
This section summarises it.

Only platforms with passing evidence are listed as supported. Claiming a platform without
a verification run behind it would be a claim exceeding measurement, and **a claim is carried
by passing CI attached to the exact tree that CI actually tested**.

On a pull request, that tree is **GitHub's synthetic merge commit** — the candidate integration
that GitHub builds from an exact pair, the base commit and the pull-request head commit. That is
what the workflows check out, so that is what the six contexts test, and the evidence belongs to
that candidate integration rather than to the branch head on its own. Move either SHA and it is
a different candidate needing its own passing run; nothing carries forward. On a push to `main`,
CI tests the actual `main` commit — and after a squash merge, that push run rather than the pull
request's is what confirms `main`. The normative form of the rule, including the separation of
acceptance evidence from ongoing evidence, is in
[`contracts/support-policy.md`](contracts/support-policy.md).

| Platform | Status | Evidence |
|---|---|---|
| Linux (`ubuntu-latest`) | **Supported** | `verify (1.94.0)` and `verify (stable)` — the full verification sequence, on every pull request |
| macOS (`macos-latest`) | **Supported** | `platform (macos-latest, 1.94.0)` and `platform (macos-latest, stable)` |
| Windows (`windows-latest`) | **Supported** | `platform (windows-latest, 1.94.0)` and `platform (windows-latest, stable)` |

### The previous entry was wrong, and had been for a while

Until T150 this table listed macOS and Windows as "not yet claimed", giving the reason
**"No platform-sensitive code exists to verify"**. That stopped being true when the
configuration layer landed in Phase 002, and nobody revisited it. The kernel resolves
filesystem paths, refuses non-regular files **by type from an open descriptor**, opens
files with a platform-specific flag (`O_NONBLOCK` on unix), and reads `OsString`
environment names that are arbitrary bytes on unix and WTF-8 on Windows. Those are
precisely the parts most likely to differ between platforms, and they were being verified
on exactly one.

The correction is a verification job, not a rewording: `platform` runs the workspace test
suite serially and the no-default-features check on macOS and Windows, on **both**
toolchains.

`platform` is a **separate job** from `verify` on purpose. `verify`'s matrix produces the
two required status contexts, and adding an `os` dimension to it would have renamed them
to `verify (ubuntu-latest, 1.94.0)` and silently emptied the branch-protection rule, which
matches contexts by name.

### How strongly each platform claim is enforced

| Platform | Runs on every pull request | **Required** by branch protection |
|---|---|---|
| Linux | yes | **yes** — `verify (1.94.0)`, `verify (stable)` |
| macOS | yes | **no** |
| Windows | yes | **no** |

The four `platform (…)` contexts are **not** in `main`'s required-status-check list. They
run on every pull request and their failure is visible, but branch protection would not
block a merge on them alone — so the macOS and Windows claims rest on **review practice,
not on an enforced gate**.

This is stated rather than quietly assumed because the distinction is exactly the kind that
decays: adding a job feels like adding a gate, and it is not one until the protection rule
names it. Making them required is a repository-settings change, which is deliberately
outside the change that added them.

### What "supported" does and does not mean here

It means the tests above pass on that platform for the exact tree CI tested — the candidate
merge tree before a merge, the actual `main` commit after one. It does
**not** mean every platform-specific behaviour has received **independent human review** —
none has, on any platform. W-003, W-005, and W-008 are open for exactly that reason, and a
platform claim does not narrow any of them.

It does **not** mean every platform receives the full verification sequence: `cargo xtask verify`
also runs secret scanning, a link check, and a commit-history scan, which are properties of
the repository rather than of the platform, and running them three times would triple a
link check against github.com to learn nothing.

Two behaviours are `#[cfg(unix)]`-gated and therefore verified on Linux and macOS only:
the FIFO refusal, and the test that drives the non-Unicode environment-name path.

The FIFO case genuinely cannot arise on Windows in this form. The **non-Unicode name**
case can: a Windows environment name is WTF-8 and may contain unpaired surrogates, so
`OsString::into_string` can fail there too. What is unix-gated is the *test*, which
constructs the hostile name from raw bytes, not the code path. The bound itself is
platform-independent, but saying so is a claim about reading rather than about
measurement, and it is recorded as the narrower statement it is.

## Rules for raising the MSRV

A raise may occur **only**:

1. in a **planned minor or major release** — never a patch;
2. after an **accepted decision record** naming the concrete dependency, language, or
   security requirement that forces it. *"Newer is better" is not a justification;*
3. documented in **three** places: [`contracts/support-policy.md`](contracts/support-policy.md), the changelog, and the release notes — and mirrored here;
4. after the outgoing MSRV has been supported for **at least six months**;
5. with a passing verification run at the new exact version — the declared MSRV is never
   published without one.

The policy is **reviewed quarterly**. A review records its outcome and **by itself changes
nothing**; only the process above changes `rust-version`.

### Scheduled revalidation

Rust 1.94.0 is currently justified by an *anticipated* persistence-layer requirement that
does not exist yet. **It must be revalidated against the actual persistence dependencies
before Phase 006 begins.** Owner: Ahmed Anbar. This is recorded rather than glossed,
because a number justified by an expectation is not the same as a number justified by a
measurement.

## Dependencies and lockfiles

| Artifact kind | Version requirements | Lockfile |
|---|---|---|
| Reusable library crates | Compatible requirements (`1.2`), not exact pins | Not committed |
| Applications, generators, release tooling, automation | As resolved | **Committed** |
| Documentation site | As resolved | **Committed** (`package-lock.json`) |

Dependency updates arrive as reviewable pull requests through Dependabot across the
`cargo`, `github-actions`, and `npm` ecosystems. **Unreviewed floating updates are
prohibited.** The authoritative machine-readable licence and dependency policy is
[`deny.toml`](deny.toml).

## Releases

- **Published versions are immutable.** A defective release is **yanked and replaced**
  with a new version, never overwritten.
- Release tags are signed.
- Releases run from a protected environment with named approvers.
- Every release produces the artifact and its checksum, a software bill of materials,
  build provenance attestation, the resolved dependency set, and the toolchain version,
  platform, operator, and date.

## Licence

**`MIT OR Apache-2.0`**, at your option, for Renvor's own source and documentation.
Contributions are accepted under the same dual terms.

**Project code generated for you by Renvor tooling carries no Renvor licensing
obligation** and is owned outright by you. Generated output must not embed a Renvor
licence header implying otherwise.

## Getting help

| You need | Go to |
|---|---|
| To report a security vulnerability | [`SECURITY.md`](SECURITY.md) — **never a public issue** |
| To report a bug or request a feature | [GitHub issues](https://github.com/renvor-rs/renvor/issues) |
| To contribute | [`CONTRIBUTING.md`](CONTRIBUTING.md) |
| To understand who decides what | [`GOVERNANCE.md`](GOVERNANCE.md) |

## Changing this policy

The policy changes only through a **superseding decision record** with an impact analysis
covering published packages, documentation, the compatibility matrix, and any downstream
consumer relying on the current promise.

**The document that changes is [`contracts/support-policy.md`](contracts/support-policy.md)**,
which is versioned and normative. This page is updated to match it; it is not a second contract,
and editing this page does not change the policy.

| Record | What it governs |
|---|---|
| **ADR-0003** | The MSRV floor, the toolchain and dependency policy. **`superseded`** by ADR-0011 on 2026-08-21; its decision body is preserved verbatim, and the MSRV rules it sets are carried forward unchanged |
| **ADR-0011** *(`accepted` 2026-08-21, under W-002 — **not** an independent review)* | Linux, macOS, and Windows as supported platforms, and the required-versus-running distinction |
