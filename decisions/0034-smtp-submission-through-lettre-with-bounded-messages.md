# ADR-0034: Deliver mail by SMTP submission through `lettre`, with bounded messages and an explicit plaintext rule

| Field | Value |
|---|---|
| **ID** | 0034 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-021. **Not independent** |
| **Review date** | 2026-09-04 |
| **Superseded by** | *(not superseded)* |

> **`accepted` under [W-021](../governance/waivers.md), and the review behind it was NOT
> independent.** No independent human review of this record has occurred, and none is claimed.
> The maintainer authored it and took every measurement it rests on; automated and maintainer
> reviews are **advisory**, never independent.
>
> W-021 covers **ADR-0031 through ADR-0037 as one coupled cluster** — each depends on a boundary
> another draws, so reviewing one alone would review a fragment — and it authorises nothing else.
> It does **not** close Phase 010; [W-022](../governance/waivers.md) is a separate exception on a
> separate axis.
>
> Accepted **2026-09-04** against head `5f26334b394f20ae86b3037ccb77a23705c40ed9`,
> tree `47aeb8d8fda9e07bd5a4520406cef4eada44273c`. W-021 expires **2027-02-11**, or
> immediately when a qualified independent human reviewer becomes available — whichever
> is first.

## Context

Phase 009 shipped `renvor_auth::MailPort` with a recording sink and recorded that *"every candidate
pulls an SMTP client and a TLS stack, and `xtask` step 7 asserts `renvor-auth` resolves no
transport"*, naming `lettre 0.11.23` as *"the leading candidate"* for Phase 010 and asking that the
research be re-verified rather than inherited.

Re-verified on 2026-09-03 and measured against this workspace's graph (`package-decisions.md` §D):

| Candidate | Result |
|---|---|
| **`lettre` 0.11.23** (MIT, MSRV 1.85; RUSTSEC-2020-0069, -2021-0069, -2026-0141 all patched below it; the 2026 advisory is a `boring-tls` hostname-verification bug and `rustls` is unaffected) | **+6 packages** with `tokio1-rustls, ring, rustls-native-certs, smtp-transport, pool, builder`; source-verified: with `rustls-native-certs` on and `rustls-platform-verifier` off it loads the native store; one licence to allow: `quoted_printable` is **0BSD** (via `builder`) |
| `lettre` with `rustls-platform-verifier` | pulls `webpki-root-certs` (CDLA-Permissive-2.0) for wasm32, which `cargo deny` evaluates for every target → **licenses FAILED** |
| `mail-send` 0.6.2 | `rustls-platform-verifier` unconditional → the same failure; no pool; MSRV undeclared |
| `async-smtp` | no TLS integration, no pool, no timeouts; not reached |

`lettre`'s `rustls-tls` feature is `webpki-roots + rustls + ring` and its `dkim` feature pulls
`rsa`; neither may ever be enabled. `lettre` selects its provider as *the installed default, else
ring* when the `ring` feature is on.

## Decision

1. **`lettre` 0.11.23**, `default-features = false, features = ["tokio1-rustls", "ring",
   "rustls-native-certs", "smtp-transport", "pool", "builder"]`. Not `hostname` (the EHLO name
   comes from configuration), not `tracing` (its events could carry server text), never
   `rustls-tls`, `dkim`, `native-tls`, or `boring-tls`. A manifest test asserts the list.
2. **`0BSD` is added to `deny.toml`'s allow list**, for `quoted_printable`, pulled by `builder`.
   Zero-Clause BSD is OSI-approved and public-domain-equivalent. Without `builder` Renvor would
   write RFC 5322 headers, RFC 2047 encoded-words, and quoted-printable bodies itself — custom
   infrastructure for a solved problem. The entry names the crate.
3. **Renvor's `Message` is bounded and injection-free by construction**: an address is at most 254
   octets with exactly one `@` and non-empty halves; no CR, LF, or NUL may appear in any
   header-bound field; at most 32 recipients; subject ≤ 998 bytes; bodies ≤ 1 MiB. `Message` has a
   hand-written `Debug` (counts and lengths), no `Display`, no `Serialize`. `lettre` receives only a
   `Message` that already passed.
4. **TLS is the default and plaintext is a double opt-in**: implicit TLS or STARTTLS over rustls
   with native roots and the `ring` provider; a plaintext connection is accepted only when the host
   is loopback **and** `allow_insecure_loopback = true`. A non-loopback plaintext URL fails Validate.
5. **Bounded**: per-operation timeout (default 30 s, cap 5 min), pool size (default 4, cap 64),
   idle timeout. Boot runs `test_connection()` when `verify_on_boot` is on (the default) and fails
   startup if the server does not answer.
6. **Message identifiers** are 16 entropy bytes over the configured sender domain — no hostname, no
   clock (C-O4's reasoning).
7. **The port makes no retry and states that sending is not idempotent**; at-least-once delivery
   is a durable job with an idempotency key (ADR-0032, ADR-0037).
8. **The `auth` feature bridges `renvor_auth::MailPort`** for any `Mailer`: templates rendered from
   configured base URL and sender — never a request `Host` — with the token only in the body, and
   delivery failure mapped to `MailError::Undeliverable` so Phase 009's enumeration property holds.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **`mail-send`** | the platform verifier is unconditional and fails the all-target licence gate; no pool |
| **`lettre` without `builder` plus `mail-builder`** (the researcher's option) | trades a 0BSD crate for a second message-building crate and a raw-bytes send path with no envelope validation of its own; more surface for the same output |
| **`lettre` with `aws-lc-rs`** (the researcher's suggestion) | would put a second provider feature on `rustls` beside `ring` and turn the cache adapter's `ClientConfig::builder()` into a panic (ADR-0033 decision 6) |
| **An HTTP mail API (a vendor's REST endpoint)** | vendor-specific, needs an HTTP client with the same root-store problem, and PLAN §16 names SMTP submission with TLS as the transport class |
| **Implicit retry inside the adapter** | a retried `DATA` is a duplicate mail; the caller cannot tell a timeout from a delivery, so the adapter must not guess |
| **Accepting CR/LF and letting `lettre` encode them** | `lettre` would encode them harmlessly, but a `Message` that can hold them is a message a future adapter could mis-handle; refusing at construction makes injection unrepresentable rather than filtered |

## Consequences

- **One licence added** to the allow list, named here and in the pull request.
- **Six packages** enter only under `smtp`; `renvor-auth` still resolves no transport.
- **A real TLS handshake is not exercised in CI**; the loopback plaintext path runs against a real
  SMTP sink (Mailpit) whose HTTP API the test reads back to assert exactly one `To` and one
  `Subject` arrived. Recorded as a limitation.
- **Operators of a plaintext relay on a private network** must terminate TLS in front of it or run
  it on loopback; Renvor will not connect otherwise.
- **What would reverse this**: a superseding record; the narrow port makes it an adapter change.

## Compliance

- **Constitution III** — a maintained package behind a narrow port, re-verified rather than
  inherited.
- **Constitution VI** — TLS by default, secrets out of every output form, header injection
  unrepresentable, bounded work.
- **Constitution XI** — licence reviewed; the addition is a reviewed policy change.
- **Phase 009 FR-054/FR-055** — the port's template-data rule and the deterministic sink remain;
  Phase 010 supplies the adapter Phase 009 declined to ship.
