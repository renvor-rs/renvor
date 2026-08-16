# Support Policy

This is a public promise. It changes only through the process described at the end of this
document.

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
| Also tested | **Current stable** (1.97.1 at time of writing, released 2026-07-16) |
| Edition | 2024 |
| Cargo resolver | 3, declared explicitly in the virtual workspace |

### What "fixed floor" means

**A new Rust stable release does not invalidate the MSRV.** It does not shorten it, and it
does not trigger a review. The minimum-version CI job stays pinned at 1.94.0; only the
stable job moves.

This was chosen deliberately over a release-count window. Rust 1.98.0 promotes to stable
on 2026-08-20, which would have pushed 1.94.0 to four releases behind and violated an
"N-3" policy **through the calendar alone** — no code change, no dependency change, no
decision by anyone. A fixed floor removes release count from the policy entirely, and
means you can read the number here rather than compute it from a release schedule.

### Where it is declared

`rust-version` is declared **once**, at `[workspace.package]` in the root manifest.
Every member inherits it with `rust-version.workspace = true`. No second independent
declaration exists, and that is asserted mechanically rather than trusted.

## Supported platforms

Only platforms with passing evidence are listed as supported. Claiming a platform without
a verification run behind it would be a claim exceeding measurement.

| Platform | Status | Why |
|---|---|---|
| Linux (`ubuntu-latest`) | **Supported** | Primary verification platform |
| macOS | **Not yet claimed** | No platform-sensitive code exists to verify |
| Windows | **Not yet claimed** | Same |

macOS and Windows enter the matrix in the phase that introduces platform-sensitive
behaviour. "Not yet claimed" means exactly that — it is not a statement that Renvor fails
on those platforms, only that nothing has been verified, so nothing is promised.

## Rules for raising the MSRV

A raise may occur **only**:

1. in a **planned minor or major release** — never a patch;
2. after an **accepted decision record** naming the concrete dependency, language, or
   security requirement that forces it. *"Newer is better" is not a justification;*
3. documented in **three** places: this document, the changelog, and the release notes;
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

This contract changes only through a **superseding decision record** with an impact
analysis covering published packages, documentation, the compatibility matrix, and any
downstream consumer relying on the current promise.
