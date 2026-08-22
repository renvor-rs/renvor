# Phase 004 dependency inventory

**Satisfies**: Phase 001 FR-040 · **Date**: 2026-08-22 · **Toolchain**: Rust 1.94.0 (MSRV floor)

Every row below was produced by running the command shown, on the branch head, and pasting the
result. Nothing here is recalled or estimated.

## How to reproduce this file

```
cargo tree -p renvor --features transport-rest --edges normal --prefix none | sed 's/ (\*)//' | sort -u > with
cargo tree -p renvor                          --edges normal --prefix none | sed 's/ (\*)//' | sort -u > without
comm -23 with without
```

The delta is what enabling `transport-rest` costs. Presenting it as a delta rather than as a flat
list is deliberate: the number that matters to a consumer is what the **feature** adds, not how
large the whole graph is.

## Direct dependencies of `renvor-http`

| Crate | Version | Licence | Declared MSRV | Why it is here |
|---|---|---|---|---|
| `renvor-core` | 0.0.0 | MIT OR Apache-2.0 | 1.94.0 | the kernel this adapter depends **inward** on |
| `axum` | 0.8.9 | MIT | 1.80 | router and server |
| `tower` | 0.5.3 | MIT | 1.64.0 | service composition, concurrency limit, `oneshot` for real-router tests |
| `tower-http` | 0.7.0 | MIT | 1.65 | CORS layer, body limit, timeout, trace |
| `tokio` | 1.53.1 | MIT | 1.71 | runtime; `net` is enabled **here** and nowhere in the kernel |
| `tracing` | 0.1.44 | MIT | 1.65.0 | instrumentation only; installs no global subscriber (C-O7) |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | 1.56 | route-inspection payload only |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | 1.71 | route-inspection payload only |

**Dev-only**: `http-body-util` 0.1.5 (MIT, 1.61) for collecting a real response body in tests, and
`tokio` with `test-util` for deterministic time. Neither reaches a consumer's production graph.

### Features taken, and features deliberately not taken

`axum` is taken with `default-features = false` and an explicit list — `http1`, `json`, `query`,
`tokio`, `original-uri`. **`multipart`, `ws`, `form`, and `macros` are excluded**, so their code and
their transitive dependencies are absent rather than merely unused.

**`tower-http`'s `request-id` feature is deliberately NOT enabled.** Its `SetRequestId` adopts a
caller-supplied header as the request identity and offers no overwrite; see
[ADR-0012](../decisions/0012-phase-004-custom-http-primitives.md) Finding 1.

## Full transitive delta introduced by `transport-rest`

**35 packages**, including `renvor-http` itself.

| Crate | Version | Licence | Declared MSRV |
|---|---|---|---|
| `atomic-waker` | 1.1.2 | Apache-2.0 OR MIT | 1.36 |
| `axum` | 0.8.9 | MIT | 1.80 |
| `axum-core` | 0.5.6 | MIT | 1.78 |
| `bitflags` | 2.13.1 | MIT OR Apache-2.0 | 1.56.0 |
| `form_urlencoded` | 1.2.2 | MIT OR Apache-2.0 | 1.51 |
| `futures-channel` | 0.3.34 | MIT OR Apache-2.0 | 1.71 |
| `futures-task` | 0.3.34 | MIT OR Apache-2.0 | 1.71 |
| `futures-util` | 0.3.34 | MIT OR Apache-2.0 | 1.71 |
| `http` | 1.5.0 | MIT OR Apache-2.0 | 1.57.0 |
| `http-body` | 1.1.0 | MIT | 1.61 |
| `http-body-util` | 0.1.5 | MIT | 1.61 |
| `httparse` | 1.10.1 | MIT OR Apache-2.0 | not declared |
| `httpdate` | 1.0.3 | MIT OR Apache-2.0 | 1.56 |
| `hyper` | 1.11.0 | MIT | 1.63 |
| `hyper-util` | 0.1.20 | MIT | 1.64 |
| `itoa` | 1.0.18 | MIT OR Apache-2.0 | 1.68 |
| `matchit` | 0.8.4 | MIT AND BSD-3-Clause | not declared |
| `memchr` | 2.8.3 | Unlicense OR MIT | 1.61 |
| `mime` | 0.3.17 | MIT OR Apache-2.0 | not declared |
| `mio` | 1.2.2 | MIT | 1.71 |
| `percent-encoding` | 2.3.2 | MIT OR Apache-2.0 | 1.51 |
| `renvor-http` | 0.0.0 | MIT OR Apache-2.0 | 1.94.0 |
| `ryu` | 1.0.23 | Apache-2.0 OR BSL-1.0 | 1.71 |
| `serde_json` | 1.0.151 | MIT OR Apache-2.0 | 1.71 |
| `serde_path_to_error` | 0.1.20 | MIT OR Apache-2.0 | 1.61 |
| `serde_urlencoded` | 0.7.1 | MIT/Apache-2.0 | not declared |
| `slab` | 0.4.12 | MIT | 1.51 |
| `smallvec` | 1.15.2 | MIT OR Apache-2.0 | not declared |
| `socket2` | 0.6.5 | MIT OR Apache-2.0 | 1.70 |
| `sync_wrapper` | 1.0.2 | Apache-2.0 | not declared |
| `tower` | 0.5.3 | MIT | 1.64.0 |
| `tower-http` | 0.7.0 | MIT | 1.65 |
| `tower-layer` | 0.3.3 | MIT | not declared |
| `tower-service` | 0.3.3 | MIT | not declared |
| `zmij` | 1.0.23 | MIT | 1.71 |

## Licence policy

`cargo deny check` was run on the branch head and reported:

```
advisories ok, bans ok, licenses ok, sources ok
```

That is the authoritative result. The notes below explain the four rows a reader would otherwise
have to check by hand:

| Row | Why it passes `deny.toml` |
|---|---|
| `matchit` — `MIT AND BSD-3-Clause` | **Both** halves of the conjunction are on the allow-list. A conjunction requires every licence to be permitted, not merely one |
| `memchr` — `Unlicense OR MIT` | A disjunction needs one permitted licence; `MIT` is on the list. `Unlicense` is not, and does not need to be |
| `ryu` — `Apache-2.0 OR BSL-1.0` | Same shape; `Apache-2.0` is on the list |
| `sync_wrapper` — `Apache-2.0` | A single permitted licence, with no alternative offered |

**No `exceptions` entry was added to `deny.toml`, and no licence was added to `allow`.** The
existing policy already permits every crate in this delta, which is a fact worth recording: a phase
that had to widen the licence policy would be a phase whose dependency choices deserved a second
look.

## Advisories

**Zero** open advisories across the delta, reported by `cargo deny check advisories` as part of the
run above. `deny.toml` sets `ignore = []` and `yanked = "deny"`, so this is a clean result rather
than a filtered one.

## MSRV

Every declared `rust-version` in the delta is **at or below 1.94.0**, the workspace floor. The
highest is `axum` at **1.80**.

Eight rows declare **no** `rust-version` at all — `httparse`, `matchit`, `mime`, `serde_urlencoded`,
`smallvec`, `sync_wrapper`, `tower-layer`, `tower-service`. That is recorded rather than glossed:
an undeclared MSRV is not a promise of compatibility, it is the absence of one, and such a crate can
raise its requirement in a patch release without the resolver noticing. The workspace's
`resolver = "3"` gives MSRV-aware resolution for crates that **do** declare one; for these it gives
nothing, and the protection is the CI job that builds on 1.94.0 rather than the manifest.

## What is NOT in this delta, and why that matters

The kernel `renvor-core` resolves **none** of these crates under **any** feature combination, and
neither does `renvor` with default features. Both directions are asserted in `cargo xtask verify`
step 7, each with a positive control proving the query detects the forbidden condition when it is
present.

The control was exercised by hand on 2026-08-22: injecting `axum` into `renvor-core`'s manifest made
the checker's exact query report `axum v0.8.9` and `tower v0.5.3`. The empty result in the real tree
therefore means **absent**, not **the scan is broken**.
