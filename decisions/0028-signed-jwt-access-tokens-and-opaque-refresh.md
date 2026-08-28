# ADR-0028: Signed JWT access tokens, opaque refresh tokens, and one algorithm per key

| Field | Value |
|---|---|
| **ID** | 0028 |
| **State** | `proposed` |
| **Reviewer** | *(none — not reviewed)* |
| **Review date** | *(not reviewed)* |
| **Superseded by** | *(not superseded)* |

> **`proposed`, not `accepted`.** No independent review, and no authority has been given to accept.

## Context

Phase 009 batch G implements FR-035 … FR-043: an **optional**, dependency-isolated API-token mode
with short-lived access credentials, exact issuer and audience validation, an algorithm chosen by
the verifier rather than by the token, bounded skew, rotated and hashed-at-rest refresh tokens,
family revocation on replay, and scope enforced at the operation.

`package-decisions.md` deferred the wire format to this batch and named `jsonwebtoken 11.0.0` the
leading candidate *if JWT is chosen*. Both halves of that condition are now settled, and they were
settled by different kinds of argument.

**The format was settled by the maintainer, on interoperability.** A dependency-cost comparison
favours PASETO, and that comparison is preserved below because it is true. It was overruled on a
ground the dependency graph cannot express: Renvor is a **general-purpose framework**, and the
public wire format its users emit is consumed by resource servers, API gateways, and SDKs that
already speak JWT's standardized `iss`/`aud`/`scope`/`exp` vocabulary and its key-publication
conventions. Dependency cost is an implementation problem to be solved; a less interoperable public
format is a permanent tax on every downstream consumer. **The dependency graph is ours to fix. The
wire format is theirs to live with.**

**The backend was settled by measurement**, recorded below, because "solve the dependency cost as an
implementation problem" is only a decision once the implementation is shown to exist.

### What the measurement found

Every candidate was resolved **against this workspace's real graph** — added to `renvor-auth`,
resolved with `cargo tree`, and checked with `cargo deny`, whose `deny.toml` sets
`all-features = true`. Advisory lookups carry the `openssl`/`time` positive control (both **200**)
so a **404** means "no advisory filed", not "wrong URL".

| Candidate | New packages | New duplicate crate names | Unpatched advisory | Native build | Verdict |
|---|---|---|---|---|---|
| `jsonwebtoken 11.0.0` + `aws_lc_rs` | **+19** | **+1** (`base64`, not a crypto crate) | **none** | **yes** — `cmake` + a C compiler | **SELECTED** |
| `jsonwebtoken 11.0.0` + `rust_crypto` | +45 | +4, incl. **`hmac`** and **`sha2`** | **`rsa 0.9.10` → RUSTSEC-2023-0071, `patched = []`** | no | rejected |
| `jwt-simple 0.13.1` (`pure-rust`) | +75 | +10 | **`rsa 0.9.10`** via `superboring` | no | rejected |
| `josekit 0.10.3` | +23 | +1 | *(openssl advisories not reached)* | **yes** — `openssl-sys`, system or vendored | rejected |

**The backend was not, in the end, a judgement call.** `xtask`'s step 7 has banned `rsa`
**workspace-wide under `--all-features`** since Phase 007, with the reason written into the source:
*"`rsa` carries RUSTSEC-2023-0071 with no fixed version."* Two of the four candidates resolve `rsa`,
so both would have failed an existing gate written two phases before this decision — before
`cargo deny` was even consulted. The measurement below explains *why* the remaining candidate is
the right one; the gate had already removed the other two.

Three findings did the work:

1. **`rust_crypto` cannot be limited to the algorithm we use.** Its feature list is
   `['dep:ed25519-dalek', 'dep:hmac', 'dep:p256', 'dep:p384', 'dep:rand', 'dep:rsa', 'dep:sha2']`.
   Cargo features are additive at the crate level, so selecting EdDSA still resolves `rsa`, and
   `rsa 0.9.10` carries **RUSTSEC-2023-0071** (Marvin Attack) with `patched = []` — cargo-deny
   reports *"No safe upgrade is available!"*. Silencing it would require a waiver. Waivers are not
   available in this phase, and a framework should not ship one for a key-recovery side channel it
   does not need.
2. **`jwt-simple`'s pure-Rust mode is not rsa-free.** `superboring` is a **non-optional**
   dependency, and it depends on `rsa 0.9.10`. The same unpatched advisory, plus ten duplicate
   crate names including `sha2`, `der`, `pkcs8`, `spki`, `signature`, and `rand`. The name and the
   `pure-rust` feature both suggest otherwise; the resolved graph is what was believed.
3. **`josekit` requires OpenSSL**, whose only escape is `vendored`. Its manifest declares no
   `rust-version`, so 1.94.0 compatibility is unverified. Strictly worse than `aws_lc_rs` on native
   build, MSRV, and advisory surface at once.

`aws-lc-sys 0.44.0` has five advisories — RUSTSEC-2026-0044 … -0048 — **all patched at `>= 0.38.0`
or `>= 0.39.0`**, so none applies at 0.44.0. `untrusted 0.7.1` carries RUSTSEC-2018-0001, patched
`>= 0.6.2`, likewise not applicable. `cargo deny check advisories licenses bans` reports
**`advisories ok, bans ok, licenses ok`** with the selected configuration under `all-features = true`.

### One measurement changed the design

With the backend reachable from a `renvor-auth` feature, `cargo deny` — which resolves
`all-features = true` — enabled `rust_crypto`, pulled `rsa`, and **failed**. The backend is
therefore pinned **at the dependency edge** and is deliberately not reachable from any Renvor
feature:

```toml
jsonwebtoken = { version = "11.0.0", default-features = false,
                 features = ["aws_lc_rs", "use_pem"], optional = true }
```

This is not tidiness. A consumer who runs `cargo deny` or `cargo audit` over their own tree with
`--all-features` would otherwise inherit an unpatched key-recovery advisory **from a backend Renvor
never uses**. The pin is what makes "solve the dependency cost as an implementation problem" true
for our users and not only for us.

## Decision

**Access tokens are signed JWTs. Refresh tokens are opaque.**

### 1. Two different credentials, for two different reasons

| | Access token | Refresh token |
|---|---|---|
| Format | **signed JWT**, verified without a database round trip | **opaque** `Opaque` + `SecretDigest` |
| Why | resource servers, gateways, and SDKs already validate this | nothing outside Renvor ever parses it |
| At rest | not stored | **only the digest** (FR-041) |
| Lifetime | short (FR-036) | longer, single-use, rotated (FR-040) |
| Revocation | expiry, plus the per-user not-before ASVS V7.4.1 requires | family-wide on replay (FR-042, **ASVS V10.4.5**) |

The refresh half **adds no dependency**: `Opaque` and `SecretDigest` are built, and already give
hashing at rest and a digest-keyed store.

### 2. The verifier shape

```
configured issuer
  └─ bounded key ring, entirely local
      └─ each key permanently bound to exactly ONE asymmetric algorithm
          └─ exact typ / iss / aud / exp / iat / jti / sub / scope validation
```

- **One algorithm per key, in the type.** RFC 8725 §3.1: *"each key MUST be used with exactly one
  algorithm, and this MUST be checked when the cryptographic operation is performed."* The key type
  carries its algorithm; there is no signature anywhere that takes a key and an algorithm as two
  arguments.
- **The token's `alg` is only ever compared.** It selects nothing and can broaden nothing. A
  mismatch is a rejection.
- **`kid` selects only from the local ring.** Unknown `kid` is a rejection.
- **No remote key material, ever.** `jku`, `x5u`, and an embedded `jwk` are rejected outright, not
  ignored — ASVS **V9.1.3 (L1)** covers all three, where RFC 8725 §3.10 omits `jwk`.
- **`jsonwebtoken::Validation` is not exposed**, and neither is any caller-supplied algorithm
  collection. `Validation.algorithms` is a `Vec<Algorithm>`; ours is constructed internally with
  exactly one element, from the key. `jsonwebtoken` is an implementation detail of this crate and
  appears in no public signature.
- **`typ` and `crit` are ours to enforce.** `jsonwebtoken` parses both into `Header` and validates
  neither. The header is inspected before verification: a wrong `typ` and any `crit` at all are
  rejections.
- **Skew is bounded and configurable** (FR-039) and time comes from the injected `Clock`.
- **Scope is re-checked inside the operation** (FR-043), not only at the edge.

### 3. Token type: not RFC 9068

The `typ` is a **collision-resistant Renvor type**, and RFC 7519, RFC 8725, and RFC 9068 are cited
as **design guidance, not conformance**. RFC 9068 profiles an OAuth 2.0 authorization server: it
requires `client_id`, mandates RS256 support, and fixes claim semantics that assume an OAuth
deployment Renvor does not impose on its users. Claiming `typ = at+jwt` without implementing the
whole profile would advertise a conformance that does not exist, and **inventing a `client_id` to
reach it would be fabricating a claim to satisfy a label**.

### 4. Isolation

`tokens` is off by default and pulls the JWT stack only when on (FR-035, SC-011), proven by the
dependency graph rather than by the flag's presence.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **PASETO (`pasetors 0.8.0`)** | **Rejected on interoperability, not on merit.** Its advantages are real and are recorded here rather than argued away: a **fixed protocol version instead of a negotiated `alg`**, so **`alg=none` and algorithm confusion are structurally impossible rather than defended against**; a **much smaller dependency graph** (`ct-codecs`, `getrandom`, `subtle` — already a direct dependency — and `zeroize`), with `sha2 ^0.11.0` matching ours exactly; **no native C or assembly build**; and a **stronger construction**, since the version *is* the cipher suite. It was rejected because Renvor is a general-purpose framework: PASETO is not what downstream resource servers, gateways, and SDKs consume, and standards status and ecosystem support outweighed a dependency cost that the `aws_lc_rs` measurement shows can be solved. **The research supporting PASETO is retained, not deleted.** |
| `jsonwebtoken` + `rust_crypto` | Resolves `rsa 0.9.10` unconditionally — **RUSTSEC-2023-0071, `patched = []`** — and adds duplicate `hmac 0.12` and `sha2 0.10` beside this crate's `hmac 0.13` and `sha2 0.11`, the exact duplicate the `cookie` row already refused. |
| `jwt-simple 0.13.1` (`pure-rust`) | The same unpatched `rsa 0.9.10`, reached through the **non-optional** `superboring`, plus **+75 packages** and ten duplicate crate names. |
| `josekit 0.10.3` | Requires OpenSSL — a heavier native dependency than `aws-lc-sys` with a larger advisory history — and declares **no `rust-version`**. |
| Opaque access tokens (no JWT) | Would make every resource-server check a database round trip and give downstream services nothing standard to validate — the interoperability the format decision exists to buy. |
| HMAC (`HS256`) access tokens | A shared secret lets any verifier mint tokens, and RFC 8725 §3.5 forbids deriving such a key from a human-memorizable string — the shape a `JWT_SECRET` environment variable takes in practice. The verifier shape requires an **asymmetric** algorithm. |
| `typ = at+jwt` with RFC 9068 conformance | Not implemented in full, and reaching it would require inventing an OAuth `client_id` Renvor has no basis to assert. |

## Consequences

**Accepted costs, stated plainly:**

- **A C toolchain and CMake are required to build with `tokens` on.** This is a real cost for
  downstream consumers and is not assumed away. It is bounded by three facts: the feature is
  **off by default**, so a consumer who does not use token mode never builds it; `ring 0.17.14` is
  already in this workspace's graph through `rustls`, so a native build is not a new *class* of
  requirement for Renvor as a whole; and `aws-lc-rs` supports the platforms this project already
  gates on. **Windows and Linux remain to be proven in CI** — the local build was macOS/aarch64.
- **The published crate carries an ISC-licensed native library.** `aws-lc-rs` is
  `ISC AND (Apache-2.0 OR ISC)`; both are on `deny.toml`'s allow list, and `cargo deny check
  licenses` passes.
- **`+19` packages and one new duplicate crate name** (`base64`) enter `renvor-auth`'s graph when
  the feature is on, and none when it is off.
- **JWT's failure modes are now ours to defend.** PASETO would have made algorithm confusion
  unrepresentable; here it is prevented by a wrapper and proven by tests, so **the tests are
  load-bearing** and mutation coverage of the algorithm and key binding is not optional.
- **The `rsa` ban is now load-bearing for this decision too.** It was written for the persistence
  adapters; it silently decided the JWT backend as well. Verified rather than assumed: step 7's
  exact query (`cargo tree --edges normal --prefix none --workspace --all-features`) reports `rsa`
  and `webpki-roots` absent, its three controls (`rustls-webpki`, `rustls`, `ring`) present, and
  `jsonwebtoken`, `aws-lc-rs` and `aws-lc-sys` **visible to the same walk** — so the absence is an
  absence and not a blind spot.
- **Claims are public.** A JWT is signed, not encrypted, so no personal data goes in a claim
  unless a documented operation requires it, and clients are documented to treat access tokens as
  **opaque bearer strings**.

**To reverse this** — the format decision, not the backend — the wire format and the verifier both
change; the refresh half, the store, family revocation, and scope enforcement are format-independent
and would survive. **To change only the backend**, the pinned `features` list changes and nothing
public moves, because `jsonwebtoken` appears in no public signature.

## Compliance

- **Package-first** — six candidates resolved against the real graph and checked with the same gate
  CI runs, rather than chosen from reputation. The one custom part, the refresh token, reuses
  `Opaque` and `SecretDigest` and adds nothing.
- **FR-035 / SC-011** — proven by the dependency graph: feature off resolves no `jsonwebtoken` and
  no `aws-lc-*`, and neither adapter inherits it.
- **FR-036 … FR-043** — the verifier shape above maps to each, with the standards cited at the point
  of use: RFC 8725 §3.1 for the key/algorithm binding, ASVS V9.1.3 for `jku`/`x5u`/`jwk`, and
  **ASVS V10.4.5** — not RFC 9700 — for family revocation.
- **No silent fallback** — if a JWT backend had failed every gate, this record would say so and stop.
  It does not fall back to PASETO.
- **ADR-0002** — `edition` and `rust-version` stay inherited; the new dependency states neither.
