# Phase 009 — Dependency Inventory

**Date**: 2026-08-30
**Phase**: 009 — Authentication, sessions, tokens, and policies
**Authoritative for**: constitution principle III (package-first boundaries), principle VIII
(feature isolation), principle XI (supply-chain integrity)
**Licence policy**: [`deny.toml`](../deny.toml) is the enforced allow-list — `MIT`, `MIT-0`,
`Apache-2.0`, `Apache-2.0 WITH LLVM-exception`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`,
`Unicode-3.0`, `Zlib`, `CC0-1.0`. `exceptions = []`, by design.

---

## 1. Selected — runtime, `renvor-auth`

| Crate | Version | For | Why this one |
|---|---|---|---|
| `argon2` | 0.5.3 | password hashing | The RustCrypto implementation of the algorithm RFC 9106 and ASVS V10.4.5 name. Verified directly against crates.io and the RustSec advisory database, with a positive control, **because the package researcher returned nothing** |
| `unicode-normalization` | 0.1.25 | NFC before hashing | NIST SP 800-63B-4 §3.1.1.2 requires code-point counting; normalising first is what makes that count stable. Same direct verification |
| `subtle` | 2.6.1 | constant-time comparison | The ecosystem's constant-time primitive. Avoids hand-rolling the one comparison where an early return is a side channel |
| `sha2` | 0.11.0 | digests | RustCrypto, already in the graph's idiom |
| `hmac` | 0.13.0 | keyed bucketing | The abuse-control mapping must be **keyed**, so collisions are accidents rather than attacker-chosen targets (ADR-0029) |
| `cookie` | 0.18.2 | cookie construction and parsing | Prefix rules and attribute serialisation are fiddly and specified; hand-rolling them is how `__Host-` guarantees get lost (ADR-0026) |
| `jsonwebtoken` | 11.0.0 | signed access tokens | RFC 8725-aware, and it permits pinning **one algorithm per key**, which ADR-0028 requires |
| `aws-lc-rs` | 1.18.0 | the crypto backend | Forced at the dependency edge. The `rsa` ban makes the backend a decision that cannot be left to feature unification |
| `chrono` | 0.4.45 | time | Already the workspace's time type |
| `thiserror` | 2.0.20 | error taxonomy | Already the workspace idiom |
| `serde` / `serde_json` | 1.0.229 / 1.0.151 | token claims | Already in the graph |

## 2. Selected — runtime, `renvor-auth-http`

**No new third-party dependency.** The crate adds `chrono`, `serde` and `serde_json`, all already
in the workspace, plus five internal edges (`renvor-auth`, `renvor-http`, `renvor-error`,
`renvor-openapi`, `renvor-core`).

That the adapter needs both sides and neither side needs it is the whole reason it exists as its
own crate rather than as code inside `renvor-http` — see ADR-0030 and §3 below.

## 3. Where one was considered and not taken

| Candidate | For | Why not |
|---|---|---|
| `governor` | rate limiting / abuse control | Its `StateStore` is **synchronous** and in-memory. The requirement is persisted state proven across four database rows, with a finite row bound. Independently re-derived from the vendored source rather than taken from the report — and the report's own bounding recommendation was **rejected** as an instance of the SQ-4 error: hashing arbitrary input does not bound cardinality |
| `tower-governor` | the same, at the transport | Same store, plus it would put abuse control in the transport, where `renvor-auth` cannot reach it |
| `log` | audit events | `Record` carries `args: fmt::Arguments` — an arbitrary formatted string **by construction**. Strictly weaker than `tracing`, and the audit vocabulary is closed precisely so no free-text field exists |
| a dedicated audit-event crate | audit | None found whose event type forbids free text. An audit type that accepts a `String` is a credential-smuggling channel with a nicer name |

## 4. The research that did not happen

`package-researcher` was commissioned for cookies, CSRF, tokens, blocklist and policy libraries. It
**returned nothing, twice**, and was recorded `NOT PERFORMED` rather than re-rolled.

`argon2` and `unicode-normalization` were therefore verified by the maintainer directly against
crates.io and the RustSec advisory database, with a positive control. **Cookies, CSRF, tokens,
blocklist and policy libraries remain unresearched** — the selections above rest on primary-source
reading of the standards and of the candidates' own source, not on an ecosystem survey.

This is stated as a gap because it is one.

## 5. A gate narrower than the gate it pre-empts

`cargo deny check` and GitHub's dependency-review action **inspect different graphs**: `cargo deny`
did not see `borrow-or-share` (MIT-0, dev-only, transitive) and dependency-review did, failing a
pull request in Phase 005. That gap predates this phase and is recorded in
[`deferred-verification-work.md`](deferred-verification-work.md). It is restated here because a
local gate narrower than its CI counterpart reports a pass it has not earned, and Phase 009 relies
on both.
