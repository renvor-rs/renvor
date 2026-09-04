# ADR-0033: Speak RESP to Valkey through `redis`, with native roots, one crypto provider, and no fallback

| Field | Value |
|---|---|
| **ID** | 0033 |
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

ADR-0019 chose **Valkey** as the generated local cache engine on its licence (BSD-3-Clause) and
recorded that *"the cache claims nothing … Renvor's runtime cache capability and its adapter arrive
in Phase 010."* Valkey speaks RESP, the Redis protocol, so the client question is which RESP client.

Constraints: rustls with **native roots** (`webpki-roots` is banned on licence), MSRV 1.94.0, no
OpenSSL, a credential that never appears in argv or a log (Phase 006 L-11 is exactly that exposure
in the generated container profile), and constitution IV's rule that a failed dependency stops the
operation rather than degrading it.

Measured against this workspace's resolved graph (`package-decisions.md` §A):

| Candidate | Result |
|---|---|
| **`redis` 1.6.0** (BSD-3-Clause, MSRV 1.88, no advisory) | **+10 packages** with `tokio-rustls-comp, connection-manager`; `tls-rustls` loads the native store through `rustls-native-certs` (`connection.rs`); `rustls` declared `default-features = false`; one licence to allow: `xxhash-rust` is **BSL-1.0** |
| `rustis` 0.25.0 | its only rustls feature hard-depends on **`webpki-roots`** (banned) |
| `fred` 10.1.0 | no commit in 18 months; forces `rt-multi-thread` |
| `deadpool-redis`, `bb8-redis` | pools over `redis`; a multiplexed connection needs none |

Measured on the real workspace under `--all-features`: the `rustls` crate carries exactly the
features `ring` and `std`. `redis` builds its client with `rustls::ClientConfig::builder()`, which
rustls resolves from the process-level provider — installing one automatically when exactly one
provider feature is enabled and **panicking when two are** unless the binary installed one first.

## Decision

1. **`redis` 1.6.0**, `default-features = false, features = ["tokio-rustls-comp",
   "connection-manager"]`. Not `acl`, `cluster`, `sentinel`, `entra-id`, `json`, `script`; never
   `tls-rustls-webpki-roots` or `tls-rustls-insecure`. A manifest test asserts the feature list.
2. **`BSL-1.0` is added to `deny.toml`'s allow list**, for `xxhash-rust`, a mandatory dependency.
   The Boost Software License 1.0 is OSI-approved and permissive, with no binary-attribution clause;
   allowing it widens nothing about copyleft or data-licence exposure. The entry names the crate,
   so removing the crate is the cue to remove the entry — the same shape as the `MIT-0` entry.
3. **The credential is a `Secret`** inside the connection settings; the URL Renvor hands `redis`
   is built from parts at connect time and never rendered by `Debug`, `Display`, an error, or an
   event. The Boot check is an authenticated `PING`.
4. **Reconnection is bounded and configured**: `ConnectionManagerConfig`'s retries, minimum and
   maximum delay, exponent base, connection timeout, and response timeout are all set from Renvor
   configuration with caps. An operation during a reconnect fails loudly; nothing queues.
5. **No fallback and no retry** at the port: a failed operation is `Err`; a miss is `Ok(None)`; a
   cache retry is a latency amplifier under a failing backend (ADR-0037).
6. **One crypto provider, asserted.** Every Renvor connector that can name a provider names
   `ring` (ADR-0034, ADR-0036); `xtask` step 7 asserts the workspace's `rustls` feature set never
   gains `aws-lc-rs`; the capabilities contract records that the process-level provider is the
   binary's decision — the same rule C-O7 applies to the tracing subscriber — and names
   `CryptoProvider::install_default` as the author's tool if their own dependencies add a second
   provider.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **`rustis`** | banned crate hard-wired into its only rustls path; no native-roots option |
| **`fred`** | stale; forces the multi-threaded runtime on every consumer, which the kernel's `current_thread` tests cannot use |
| **A pool crate over `redis`** | `MultiplexedConnection` is cloneable and multiplexes one socket; a pool of multiplexed connections is a second bound for no requirement |
| **Memcached** | ADR-0019 already settled the engine; a second protocol is a second adapter for no user |
| **Refuse the BSL-1.0 crate and ship the port without an adapter** | the cache deliverable would be a port with a memory substitute — honest, but it leaves §7.4's `capability-cache` describing nothing production-ready when a permissive-licence addition resolves it. Recorded as the fallback if the maintainer refuses the allow-list change |
| **Install `ring` as the process default from the cache provider** | takes a process-global decision inside a library — the act C-O7 exists to forbid. Asserting the single-provider state and documenting the author's responsibility keeps the decision in `main` |

## Consequences

- **One licence added to the allow list**, named in the pull request and here.
- **Ten packages** enter the graph only under `valkey`; the isolation rows prove the feature off
  removes all ten.
- **A real TLS handshake is not exercised in CI** (no trusted CA on a runner); the TLS configuration
  path is unit-tested and the loopback plaintext path runs against a real Valkey. Recorded as a
  limitation.
- **A consumer who enables `rustls/aws-lc-rs` elsewhere** must install a provider in `main` before
  Boot or the cache TLS path panics inside `redis` — stated in the contract, not discovered.
- **What would reverse this**: a superseding record selecting another client, which the narrow port
  makes an adapter change rather than an application change.

## Compliance

- **Constitution III** — a maintained package behind a narrow port; failure stays visible.
- **Constitution IV** — bounded reconnection, no silent fallback, Boot fails on an unanswered ping.
- **Constitution VI** — secrets out of every output form; TLS to external dependencies; bounded work.
- **Constitution XI** — licence reviewed and recorded; the addition is a reviewed policy change.
- **ADR-0019** — the engine decision is honoured, not reopened.
