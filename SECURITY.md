# Security Policy

## Reporting a vulnerability

**Do not open a public issue for a security problem.** Public issues are visible to
everyone, including anyone who would exploit the report before a fix exists.

Report privately by email to:

> **admin@ahmedanbar.dev**

This address is monitored by **Ahmed Anbar**, the project maintainer.

GitHub's private vulnerability reporting will also be enabled on
[`renvor-rs/renvor`](https://github.com/renvor-rs/renvor/security/advisories/new) as a
second private channel. Until this notice says otherwise, **email is the reliable path**.

### What to include

The more of this you can provide, the faster a fix lands:

- what the issue is, and what an attacker gains;
- the affected version, commit, or crate;
- steps to reproduce, or a proof-of-concept;
- your assessment of severity, if you have one;
- whether the issue is already public anywhere.

Reports in any form are welcome. A partial report is far better than no report.

## What to expect, and when

| Stage | Commitment |
|---|---|
| Acknowledgement of your report | **Within 72 hours** |
| Initial assessment — severity, affected versions, whether it is accepted | **Within 7 days** of acknowledgement |
| Progress update | **At least every 14 days** while the issue is open |
| Fix released, or a dated plan explaining why longer is needed | **Within 90 days** of acknowledgement |

If you have not heard back within 72 hours, assume the message did not arrive and try
again — a silent failure on our side should not cost you your report.

## Disclosure

- We ask that you **hold public disclosure until a fix is released**, or until 90 days
  after acknowledgement, whichever comes first.
- If a vulnerability is already being exploited, tell us and we will move faster; that
  situation overrides the schedule above.
- We will **credit you by name** in the advisory and release notes unless you ask us not
  to. Anonymous reporting is fine.
- Advisories are published as GitHub Security Advisories and referenced in the changelog.
- **A security release blocker cannot be waived.** The project's waiver process
  (`governance/waivers.md`) explicitly excludes security blockers from anything that can
  be waived for a public release.

## Scope

**In scope:** the `renvor` crate, the `renvor-cli` crate and its `renover` executable, the
build and release automation in `.github/workflows/`, and this repository's supply chain.

**Out of scope:** vulnerabilities in third-party dependencies (report those upstream,
though we do want to know so we can pin or patch), and issues in project code that Renvor
*generates* for a user — that code is owned by the user, and Renvor makes no security
claim about the application built on top of it.

## Current status — read this before relying on it

**Renvor is pre-release and ships no runtime capability yet.** As of this writing the
published crate exposes version constants only. There is presently no attack surface in
the library itself. This policy is established now, ahead of any code, so that a reporting
path exists from the first line of functionality rather than being retrofitted after the
first incident.

## Supported versions

| Version | Supported |
|---|---|
| `0.0.x` | Pre-release. Fixes land on the latest version only; there is no backporting. |

A published version is never overwritten. A defective release is **yanked and replaced**
with a new version.
