# Release checklist

Renvor is pre-release. Until the first stable release is published, every public
property must state that the framework is in development (`PLAN.md` Section 26.6).

## Before tagging

- [ ] `cargo xtask verify` passes on `1.94.0` and on current stable
- [ ] `SUPPORT.md`, the changelog, and these notes agree on the MSRV
- [ ] Every decision record affecting this release is `accepted`, not `proposed`
- [ ] No waiver has passed its expiry date
- [ ] Secret scans report zero findings over history and working tree

## Publication

- [ ] Packages published in topological order, waiting for index availability between each
- [ ] First publication of a new crate uses a least-scope manual token, revoked immediately
      afterwards with the revocation timestamped in the evidence ledger
- [ ] Every later release uses trusted publishing; no registry token is stored anywhere
- [ ] The release tag is signed
- [ ] The release ran from the protected environment with a named approver

## Evidence retained

- [ ] Artifact and its `sha256`
- [ ] Software bill of materials
- [ ] Build provenance attestation
- [ ] The resolved dependency set (committed lockfile)
- [ ] Toolchain version, platform, operator, and date
- [ ] The previous known-good version, so rollback needs no investigation

## After publication

- [ ] Installation verified from the public registry
- [ ] Published documentation resolves
- [ ] A defective release is yanked and replaced, never overwritten
