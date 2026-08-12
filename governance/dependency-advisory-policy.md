# Dependency Advisory Response Policy

**Status**: Adopted 2026-08-12 by maintainer decision (task **T104**)
**Authoritative for**: spec FR-010; `contracts/support-policy.md`; `CONTRIBUTING.md`; ADR-0003; `deny.toml`
**Owner**: Ahmed Anbar
**Resolves**: governance checklist **CHK050** — FR-010 required advisory handling to be stated but set no response or triage window, so an advisory could remain unactioned indefinitely without violating any written rule.

> **This is not `SECURITY.md`.** That document governs **inbound private vulnerability
> reports about Renvor itself**. This document governs **advisories against dependencies**
> that Renvor consumes. The two have different sources, different owners of the underlying
> defect, and deliberately different clocks. Neither supersedes the other.

> **The numeric deadlines below are Renvor policy decisions.** CVSS, FIRST, RustSec, and
> NIST informed them; none prescribes 7, 14, 30, or 90 days. Cite them as a project choice.

## 1. Scope

Advisories from **RustSec**, **`cargo-deny`**, **Dependabot**, the **GitHub Advisory
Database**, and equivalent verified sources, affecting any dependency in any ecosystem the
project uses — `cargo`, `github-actions`, and `npm`.

## 2. Triage windows — maximum time to first assessment

Measured from **confirmed detection**. Triage means: severity assessed, affected versions
determined, and a **named owner** assigned.

| Advisory condition | Triage within |
|---|---|
| **Known active exploitation, or Critical** | **24 hours** |
| **High** | **48 hours** |
| **Medium** | **5 calendar days** |
| **Low** | **10 calendar days** |

## 3. Remediation targets — maximum time to resolution

Measured from **confirmed detection**, not from triage.

| Advisory condition | Required target |
|---|---|
| **Known active exploitation** | Begin **immediately**; mitigation and the public/private advisory decision within **24 hours** |
| **Critical** | Fix, remove, disable, or isolate within **7 calendar days** |
| **High** | Fix, remove, disable, or isolate within **14 calendar days** |
| **Medium** | Remediate within **30 calendar days** |
| **Low** | Remediate within **90 calendar days**, or the next scheduled prerelease, **whichever occurs first** |

## 4. Severity is not CVSS alone

A CVSS base score measures **severity, not risk**. Assessment MUST weigh all of:

- the CVSS score;
- **reachability** — whether Renvor's code paths actually reach the vulnerable code;
- which **released versions** are affected;
- **exploit maturity** — proof-of-concept, weaponised, or theoretical;
- **known exploitation** in the wild;
- **deployment exposure** — whether the dependency runs in a build tool, a library, or a network-facing surface;
- **Renvor-specific impact**.

A high base score with no reachable path may be assessed lower; a moderate score that is
reachable and actively exploited may be assessed higher. **The assessment and its reasoning
are recorded** — an unrecorded downgrade is indistinguishable from ignoring the advisory.

## 5. Every advisory gets a dated record

Each advisory receives a record containing **all** of:

| Field | Content |
|---|---|
| Source and identifier | e.g. `RUSTSEC-YYYY-NNNN`, `GHSA-xxxx-xxxx-xxxx` |
| Affected dependency and versions | Name and the exact affected range |
| Detection time | When the project confirmed detection — starts both clocks |
| Severity and contextual risk | The clause 4 assessment, with reasoning |
| Reachability | Whether Renvor reaches the vulnerable code, and how that was determined |
| Named owner | An individual, never a role or team |
| Chosen action | Fix, remove, disable, replace, isolate, or time-bounded exception |
| Deadline | The clause 3 date, stated absolutely |
| Mitigation | What protects the project until resolution |
| Resolution and verification evidence | What was done, and how it was confirmed |

## 6. No upstream fix does not extend the deadline

**If no fixed upstream version exists, the same deadline still applies.** The project must
**remove, disable, replace, or isolate** the dependency, or **block the affected release**.

Waiting for an upstream maintainer is not a remediation, and "no patch available" is not an
extension. The deadline is a commitment about Renvor's exposure, not about upstream's
schedule.

## 7. Critical and High cannot be waived for a public release

Known **Critical** or **High** vulnerabilities are **public-release blockers** and **cannot
be waived**. This is consistent with the waiver ledger's standing rule that security release
blockers are outside what any waiver may cover.

## 8. Medium and Low acceptance requires a written exception

Accepting **Medium** or **Low** risk requires a written, **time-bounded** exception stating
mitigation, owner, **expiry**, **reassessment date**, and removal plan — recorded in
`governance/waivers.md` under the standing waiver rules, including the mandatory absolute
expiry date.

## 9. Silent ignoring is forbidden

**An ignored advisory without a narrowly scoped, dated record is forbidden.** In practice
this means `deny.toml`'s `advisories.ignore` list stays empty unless each entry points at a
dated record satisfying clauses 5 and 8. An advisory identifier added to a configuration
file with no record is a policy violation, not a configuration choice.

## 10. Progress updates on open Critical and High records

Open **Critical** and **High** advisory records receive a **progress update at least every
five calendar days** until resolved. A record that goes quiet is a record nobody is working.

## 11. Already-released versions

If a released version is affected:

- **publish an advisory** and remediation guidance;
- **yank or supersede** an affected prerelease where appropriate;
- **never silently miss the deadline.**

A missed deadline is itself recorded, with the reason, rather than being allowed to pass
unremarked.

## 12. Relationship to `SECURITY.md`

| | This policy | `SECURITY.md` |
|---|---|---|
| Governs | Advisories against **dependencies** | **Inbound private reports** about Renvor |
| Source | RustSec, cargo-deny, Dependabot, GHSA | A reporter emailing the security contact |
| Clock starts | Confirmed detection | Report received |
| Owner of the defect | Upstream | Renvor |

`SECURITY.md` response commitments are unchanged by this policy.

## References

Primary sources consulted. **None prescribes the numeric deadlines above.**

- Rust security policy: <https://rust-lang.org/policies/security/>
- RustSec Advisory Database: <https://rustsec.org/>
- FIRST — CVSS measures severity, not risk:
  <https://www.first.org/cvss/user-guide.html#CVSS-Base-Score-CVSS-B-Measures-Severity-not-Risk>
- FIRST — qualitative severity rating scale:
  <https://www.first.org/cvss/specification-document#Qualitative-Severity-Rating-Scale>
- NIST SP 800-218: <https://doi.org/10.6028/NIST.SP.800-218>
