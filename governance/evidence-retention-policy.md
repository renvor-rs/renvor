# Evidence Retention Policy

**Status**: Adopted 2026-08-12 by maintainer decision (task **T103**)
**Authoritative for**: spec FR-046, FR-045; `contracts/package-metadata.md`; `RELEASING.md` (written at T070)
**Owner**: Ahmed Anbar
**Resolves**: governance checklist **CHK048** — FR-046 required evidence to be retained "for a stated period" and no duration existed anywhere.

> **The numeric periods below are Renvor policy decisions.** They are not durations
> mandated by GitHub, by NIST, or by any external authority. The references at the end
> informed the decision; none of them prescribes seven years, three years, or ninety days.
> Anyone citing this policy should cite it as a project choice, not as compliance.

## 1. Ordinary CI logs and temporary workflow artifacts — 90 days

Retained for **90 days**, the maximum retention GitHub Actions supports for public
repositories. No project setting extends this, so 90 days is both the policy and the
platform ceiling.

## 2. Actions artifacts are transport, not archive

**GitHub Actions artifacts are temporary evidence transport, never the durable release
archive.** Anything that must outlive a build is copied into a retained location before its
Actions retention expires. Treating an Actions artifact as the archive of record is a
defect: the artifact disappears on schedule whether or not anyone noticed.

## 3. Governance records — lifetime of the project

Phase-completion and release-rehearsal evidence stored as **tracked governance records**
lives in the repository for the **lifetime of the project**. Git history is the retention
mechanism; no separate copy is required.

## 4. What every published release retains

For **every published prerelease or stable release**, all of the following are retained:

- source archive;
- packaged artifacts;
- SHA-256 checksums;
- release manifest;
- CycloneDX software bill of materials;
- provenance and attestation bundles;
- signing and verification metadata;
- toolchain version, platform, operator, and build date.

## 5. Binary release evidence — the later of two dates

Retained until **whichever is later**:

- **seven years** after publication; or
- **three years** after the supported lifetime of that release ends.

The "later of" construction matters: a long-supported release must not lose its evidence
seven years after publication merely because it was supported for a long time.

## 6. Compact integrity and provenance records — lifetime of the project

The release manifest, checksums, SBOM, attestation and provenance bundle, and signing
metadata are retained for the **lifetime of the project**, independently of clause 5.

These are small. Keeping them permanently costs almost nothing and preserves the ability to
answer "what exactly was shipped, and does this artifact match it?" long after the binaries
themselves have been aged out.

## 7. Canonical copies and the independent archive

The **canonical public copy** of release evidence is the corresponding **immutable GitHub
Release**, where release immutability is available.

A **second, independently controlled archive** is required before the first real crates.io
release, with all of:

- independent control — not the same account or platform as the canonical copy;
- encryption at rest;
- versioning;
- access logging;
- an **annual restore test**.

**No such archive exists today.** This document does not claim one, and no other document
may imply one.

## 8. Provider undecided; the Phase 013 gate fails closed

The storage provider for the independent archive may be chosen later.

**The Phase 013 release gate MUST fail closed if the independent archive and its restore
test are not ready.** "We will set it up after the release" is not an accepted outcome — the
archive exists to protect the release that has already shipped.

## 9. Phase 001 today

Phase 001 publishes **no crate and no release**. Its durable evidence location is the
tracked record `governance/phase-001-evidence.md`, retained under clause 3 for the lifetime
of the project. Temporary CI artifacts from Phase 001 workflow runs follow the 90-day rule
in clause 1.

Clauses 4 through 8 have **no Phase 001 obligations to discharge**, because nothing has been
published. They bind from the first published prerelease onward.

## 10. Incorporation into `RELEASING.md`

`RELEASING.md` is written at **T070** and MUST incorporate this policy exactly — by
reproducing these periods or by referencing this document as authoritative. A divergent
restatement is a defect. **T070 is not complete and must not be marked complete on the
strength of this policy existing.**

## References

Primary sources consulted while making these decisions. **None mandates the numeric periods
above**; they inform what is possible and what is good practice.

- GitHub — configuring the retention period for Actions artifacts and logs:
  <https://docs.github.com/en/organizations/managing-organization-settings/configuring-the-retention-period-for-github-actions-artifacts-and-logs-in-your-organization>
- GitHub — about releases:
  <https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases>
- GitHub — artifact attestations:
  <https://docs.github.com/en/actions/concepts/security/artifact-attestations>
- NIST SP 800-218, Secure Software Development Framework:
  <https://doi.org/10.6028/NIST.SP.800-218>
