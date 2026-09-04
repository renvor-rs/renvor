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

**In scope:** every crate in this workspace — the `renvor` facade, `renvor-cli` and the
`renvor` executable it installs, and the kernel, transport, persistence, authentication, and
capability crates it is built from (`renvor-core`, `renvor-config`, `renvor-error`,
`renvor-validation`, `renvor-openapi`, `renvor-http`, `renvor-database`, `renvor-sqlx`,
`renvor-seaorm`, `renvor-auth`, `renvor-auth-http`, `renvor-cache`, `renvor-jobs`,
`renvor-mail`, `renvor-storage`, `renvor-observability`, `renvor-testkit`) — the build and
release automation in `.github/workflows/`, and this repository's supply chain.

**Out of scope:** vulnerabilities in third-party dependencies (report those upstream,
though we do want to know so we can pin or patch), and issues in project code that Renvor
*generates* for a user — that code is owned by the user, and Renvor makes no security
claim about the application built on top of it.

## Current status — read this before relying on it

**Renvor is pre-release and unpublished.** **Nothing has been published to any registry** —
`renvor` and `renvor-cli` are both absent from crates.io — so no version of Renvor can
currently reach a user's dependency graph.

The repository contains, as of Phase 010, a transport-independent kernel and the surfaces
built on it. **It accepts untrusted input over the network** wherever an application enables
the corresponding feature, and each surface is listed here so nobody relies on an older
statement that it did not:

- **Inbound HTTP** (`renvor-http`, Phase 004 onward): a listener over `axum` that validates the
  `Host` header against a configured set, resolves client identity from trusted proxies only,
  bounds bodies and concurrency, applies a deny-by-default CORS policy, parses inbound W3C
  `traceparent`/`tracestate` and Fetch Metadata (`Origin`, `Sec-Fetch-Site`) as untrusted
  bounded input, and refuses cookie-authenticated unsafe requests whose complete effective
  origin (scheme, host, effective port) differs from the request's own.
- **Authentication** (`renvor-auth`, `renvor-auth-http`, Phase 009): opaque server-side
  sessions in `__Host-` cookies, CSRF double-submit bound to the session, Argon2id passwords,
  single-use expiring verification and reset codes delivered by mail **as codes, never in a
  link**, optional signed JWT access tokens, and bounded abuse controls.
- **Outbound network clients** (Phase 010, each behind an off-by-default feature): a Valkey
  client (`renvor-cache/valkey`), an SMTP submission client (`renvor-mail/smtp`), and an
  OTLP/HTTP exporter (`renvor-observability/otel`). Each uses rustls with the native root store
  and exactly one crypto provider; plaintext is refused unless the peer is loopback **and** the
  configuration opts in; credentials are separate `Secret` settings, never part of a URL, and
  are never rendered by any error, event, or `Debug`.
- **Durable jobs** (`renvor-jobs` on `renvor-sqlx`/`renvor-seaorm`): payloads are bounded on
  write and on read, never logged, and a handler runs in its own task under a timeout with panic
  containment.
- **Object storage** (`renvor-storage/filesystem`): keys cannot traverse (validated, and rooted
  in a `cap-std` directory capability), objects are bounded on write and on read, symbolic
  links are refused.

The kernel's own properties hold underneath all of it: secret redaction across every output
form, bounded deadlines on every call into author code, and containment of a panicking provider
or readiness check — each tested. The Phase 010 evidence, limitations, and review record under
`governance/` say what was measured and what was not.

Panic containment is built on `catch_unwind`, so it holds under the **unwinding** panic strategy
only. Rather than leave that as a caveat a build profile could silently violate, `panic = "abort"`
is **unsupported** and `renvor-core` refuses to compile under it.

This policy was established ahead of the code, so that a reporting path existed from the first
line of functionality rather than being retrofitted after the first incident.

## Supported versions

| Version | Supported |
|---|---|
| `0.0.x` | Pre-release. Fixes land on the latest version only; there is no backporting. |

A published version is never overwritten. A defective release is **yanked and replaced**
with a new version.
