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

**Prohibited**: any git or path dependency in a publishable package (FR-040). `xtask` is exempt because it declares `publish = false`.

## Rehearsal procedure

Run from a clean checkout. Performs **zero** publish operations.

| Step | Command | Proves |
|---|---|---|
| 1 | `cargo package -p renvor --list` | The exact file set that would ship |
| 2 | Inspect that list | No secret, local configuration, build output, or unintended asset (FR-039) |
| 3 | `cargo package -p renvor` | The artifact builds |
| 4 | `cargo publish --dry-run -p renvor` | Full publish verification without network write |
| 5 | Compute `sha256` of the artifact | Checksum recorded in evidence |
| 6 | Query the live registry | **Zero** versions exist — absence of publication proven, not assumed (SC-010) |

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

Retained long enough for a reviewer to reconstruct what was verified, on which platform, by whom, and when. The retention period is stated in `RELEASING.md`.

## Workflow permission contract

| Workflow | Top-level | Job elevation |
|---|---|---|
| `ci.yml` | `contents: read` | none |
| `security.yml` | `contents: read` | `security-events: write` on the SARIF upload job only |
| `docs.yml` | `contents: read` | `pages: write` + `id-token: write` on the deploy job only |
| `release-dry-run.yml` | `contents: read` | none — it must not be able to publish even if invoked |

Every third-party action is pinned to a full 40-character commit SHA with a trailing `# vX.Y.Z` comment, maintained by Dependabot (research Finding 7).
