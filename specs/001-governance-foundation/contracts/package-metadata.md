# Contract: Package Metadata and Release Rehearsal

**Feature**: `specs/001-governance-foundation` | **Satisfies**: FR-032 – FR-034, FR-038 – FR-041, FR-045, FR-046

## Required metadata

Every package intended for publication declares all of the following. A missing field fails metadata validation (FR-040).

| Field | Rule |
|---|---|
| `name` | Matches the [public identity contract](./public-identity.md) exactly |
| `version` | Semantic versioning |
| `description` | One sentence, no marketing claims, no capability not yet shipped (FR-044) |
| `license` | `MIT OR Apache-2.0` — no other value permitted (FR-009) |
| `repository`, `homepage`, `documentation`, `readme` | Present and resolving |
| `keywords`, `categories` | Present and accurate |
| `rust-version` | Matches the [support policy](./support-policy.md) MSRV exactly |
| `include` or `exclude` | Explicit — the shipped file set is stated, never inferred |

**Prohibited**: a **git** dependency, or a **path-only** dependency, in a publishable package (FR-040). `xtask` is exempt because it declares `publish = false`.

> **Corrected 2026-08-16 (T118).** This line previously read "any git or path dependency", which is
> stricter than FR-040 and stricter than reality. FR-040's words are *path-**only*** dependency, and
> the difference is the whole mechanism by which a multi-crate workspace publishes at all:
>
> | Form | Publishable? | Why |
> |---|---|---|
> | `{ git = "…" }` | **No** | crates.io rejects it; nothing pins what was built |
> | `{ path = "../x" }` | **No** | Nothing tells the registry which version to resolve — this is the *path-only* case FR-040 names |
> | `{ path = "../x", version = "0.0.0" }` | **Yes** | cargo rewrites it to the version requirement at publish time and **drops the path** |
>
> The facade has carried `{ path, version }` since Phase 002 and was compliant throughout; the
> contract text was not. Read literally, the old wording made the workspace unpublishable by rule
> while it was publishable in fact.

## Rehearsal procedure

Run from a clean checkout. Performs **zero** publish operations.

| Step | Command | Proves |
|---|---|---|
| 1 | `cargo package -p <crate> --list`, per publishable crate | The exact file set that would ship |
| 2 | Inspect each list | No secret, local configuration, build output, or unintended asset (FR-039) |
| 3 | `cargo package --workspace` | The artifacts build |
| 4 | `cargo publish --dry-run --workspace` | Full publish verification without network write |
| 5 | Compute `sha256` of each artifact | Checksums recorded in evidence |
| 6 | Query the live registry | **Zero** versions exist — absence of publication proven, not assumed (SC-010) |

> **Amended 2026-08-16 (Phase 002).** Steps 1, 3, and 4 read `-p renvor` until Phase 002. The
> publishable set grew from **1 crate to 4** — `renvor-core`, `renvor-config`, `renvor-testkit`,
> and the facade `renvor` — and the single-crate form then stopped working *at all*, rather than
> merely covering less:
>
> ```text
> $ cargo publish -p renvor --dry-run
> error: failed to prepare local package for uploading
> Caused by:
>   no matching package named `renvor-config` found
>   location searched: crates.io index
> ```
>
> The facade now depends on crates that are not on the registry, and `--dry-run` resolves
> dependencies from the registry. **Marking those crates publishable does not fix it** — the
> failure is about registry *presence*, not about the `publish` flag. `cargo publish --dry-run
> --workspace` does work, because cargo stages the workspace members into a temporary registry and
> verifies the whole chain; it requires every member of the chain to be publishable, which is why
> the three new crates are `publish = true`.
>
> This is a consequence of ADR-0002's own provision that *"later phases add implementation crates
> behind it"*, so it is an amendment to the procedure rather than a change of decision. `xtask`
> remains `publish = false` and is excluded automatically — asserted, not assumed, by a positive
> control in `release-dry-run.yml`. Evidence: `specs/002-core-kernel/research.md` §D13
> (four-case experiment); proposed **ADR-0008**.

Step 6 is the one people skip. Proving a negative requires looking; "we didn't run publish" is an assertion, while "the registry reports no versions" is evidence.

## Publication rules (documented now, executed later)

1. **Topological order.** Dependency packages publish before dependents, waiting for registry index availability between each. The order is recorded in `RELEASING.md`.
2. **First release is manual.** Trusted publishing cannot be linked to a package that does not yet exist on the registry, so a new package's first release requires a manually-created token (research Finding 2). That token must be least-scope, single-package, created immediately before use, never committed and never stored as a repository secret, revoked immediately after the release verifies, with the revocation timestamped in the evidence ledger.
3. **All subsequent releases use trusted publishing.** OIDC exchange via `rust-lang/crates-io-auth-action`, bound to the approved repository, protected environment, and exact workflow. The job declares `id-token: write`; no other job does. No long-lived registry credential exists anywhere (FR-033).
4. **Published versions are immutable.** A defective release is **yanked and replaced** with a new version, never overwritten (FR-041).
5. **Release tags are signed**, and releases run from a protected environment with named approvers (FR-032).

**Phase 001 executes none of steps 1–5 against the live registry.** The workflow is written and dry-run-verified; the credentials are never created. Because no package is published in this phase (Q1 clarification), trusted publishing cannot yet be configured at all — expected, and recorded as such.

## Release evidence

Every release, and the Phase 001 rehearsal, produces (FR-045, FR-046):

- the artifact and its `sha256` checksum;
- a CycloneDX software bill of materials;
- build provenance attestation via `actions/attest` — free on public repositories, which is one of the concrete benefits of the Q4 decision;
- the resolved dependency set (the committed lockfile);
- the toolchain version, platform, operator, and date.

Retained per `governance/evidence-retention-policy.md`, which states concrete periods rather than "long enough":

| Class | Retention |
|---|---|
| Ordinary CI logs and temporary workflow artifacts | **90 days** (platform maximum for public repositories) |
| Tracked governance evidence records | **Lifetime of the project** |
| Binary release evidence | **The later of** 7 years after publication **or** 3 years after that release's supported lifetime ends |
| Manifest, checksums, SBOM, attestation bundle, signing metadata | **Lifetime of the project** |

Workflow artifacts are evidence **transport**, not the durable archive. The canonical public
copy is the corresponding immutable release; a second independently controlled, encrypted,
versioned archive with access logging and an annual restore test is required **before the
first real registry publication**, and **does not exist yet**. The Phase 013 release gate
fails closed without it. These periods are Renvor policy decisions, not externally mandated
durations.

## Workflow permission contract

| Workflow | Top-level | Job elevation |
|---|---|---|
| `ci.yml` | `contents: read` | none |
| `security.yml` | `contents: read` | `security-events: write` on the SARIF upload job only |
| `docs.yml` | `contents: read` | `pages: write` + `id-token: write` on the deploy job only |
| `release-dry-run.yml` | `contents: read` | `id-token: write` + `attestations: write` on the attestation job only |

**Amended 2026-08-12 (T081).** This row previously read "none". Artifact attestation cannot be produced without `id-token: write` and `attestations: write` — the minimum documented by `actions/attest-*` — so requiring both attestation (FR-045) and a zero-elevation workflow was an internal contradiction. The elevation is granted at the attestation job and nowhere else, and the packaging job keeps `contents: read`.

**The invariant that mattered is unchanged**: the workflow still cannot publish. `contents: write` and `packages: write` are granted nowhere, no registry credential is referenced, no tag or release is created, and the sole `cargo publish` invocation carries `--dry-run`. The elevation buys the ability to sign a statement about an artifact, not the ability to ship one.

Every third-party action is pinned to a full 40-character commit SHA with a trailing `# vX.Y.Z` comment, maintained by Dependabot (research Finding 7).
