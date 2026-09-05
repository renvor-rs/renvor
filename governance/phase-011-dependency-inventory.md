# Phase 011 — Dependency Inventory

**Date**: 2026-09-05
**Phase**: 011 — Generators, the auth starter, and the testing kit
**Authoritative for**: constitution principle III (package-first boundaries), principle VIII
(feature isolation), principle XI (supply-chain integrity)
**Licence policy**: [`deny.toml`](../deny.toml) is the enforced allow-list and **is unchanged by
this phase** — no licence added, `exceptions = []`, still.
**Lockfile**: 528 → **529 packages**. The one addition is `minreq 3.0.0` (ISC, **zero
dependencies**), an *optional* dependency of `renvor-testkit` behind its `client` feature; nothing
in the workspace enables the feature, so it reaches a build only through a generated starter's
`[dev-dependencies]`. The `renvor` executable's closure is unchanged
(`crates/renvor-cli/tests/capabilities.rs::the_executable_reaches_no_http_client_crate` still
green).

Every candidate was measured against the real workspace lockfile — additions to the 528-package
baseline, licences under `cargo deny` across every target `deny.toml` evaluates, RustSec with a
positive control, MSRV against the 1.94.0 floor, and the workspace's own bans (`unsafe` in
workspace code; no `reqwest`, `hyper`, `ureq`, `curl`, `isahc`, `attohttpc`, `surf`,
`http-client`, or `native-tls` anywhere in the CLI's closure, dev-dependencies included). The
working record is `specs/…/research/packages.md` (gitignored); this file is the clone-visible
mirror of its decisions.

## 1. Selected

| Crate | Version | Licence | Where | Why this one |
|---|---|---|---|---|
| `minreq` | 3.0.0 | ISC | `renvor-testkit`, optional, feature `client`; a generated starter's dev-dependency (`features = ["client"]`) | one crate, no dependencies; response framing by `Content-Length` and by `Transfer-Encoding: chunked` and per-request timeouts already implemented and tested upstream — the part a hand-written loopback client gets wrong (the phase's own first draft, since deleted, carried ~90 lines of exactly that). Released 2026-06-15, MSRV 1.63, five open issues. **Default features only, always**: `https-rustls` drags `aws-lc-rs` into a workspace that pins the single `ring` provider (the redis adapter panics with two), and `https-native-tls`/`https-openssl` reach `native-tls` and the CDLA-licensed platform verifier. The lock is proven to omit every optional dependency. |

## 2. Edges added without a package

| Edge | Why |
|---|---|
| `renvor-cli` → `renvor-auth`, `renvor-jobs` (normal) | `renvor generate migration --import auth\|jobs` copies the engines' **embedded** migration sets (`renvor_auth::migrations`, `renvor_jobs::migrations`, `include_str!`-ed and counted by tests) into a project; the CLI reads them from the crates rather than from a checkout, so a placed project needs no framework path at import time. Neither crate's features are enabled. |
| `renvor-auth` `include = ["migrations/**/*.sql", …]` | the set is `include_str!`-ed, so a package without the files would not build. (`renvor-jobs` listed the pattern twice since Phase 010; listed once now.) |
| `renvor-testkit` → `renvor-http` (optional, feature `http`) | `renvor_testkit::app::TestApplication` dispatches `Request`s through a route registry without a socket. Behind a feature so a driver-free consumer pulls no HTTP stack. |
| `renvor-auth-http` dev → `renvor-testkit` (`http`) | its end-to-end suite is now driven through `TestApplication` instead of a private copy of the same harness. |
| a generated starter: `nix 0.28.0` with `signal`, `cfg(unix)` dev-dependency | the generated test interrupts its own binary with a real `SIGINT` and waits for the clean exit; `nix` is already in the seeded lock at that version (the CLI's `term` feature), `signal = ["process"]` adds no package, and `libc::kill` would need `unsafe`. Windows has no `SIGINT` to send and the generated test says so, the way `Terminal::await_input_readiness` already does. |

## 3. Evaluated and rejected

| Crate | Purpose | Rejected because |
|---|---|---|
| `fake` | factory data | realistic-looking data is not a requirement; +2 packages and a duplicate `either`; value stability across releases not promised |
| `arbitrary` | factory data | fuzz-oriented: structurally valid, semantically meaningless rows; no defaults/overrides model; +2 packages |
| `factori` | FactoryBot-style factories | last commit 2020-08-14, no MSRV — unmaintained by the `[advisories] unmaintained = "workspace"` standard even without an advisory |
| `test-case`, `rstest_reuse` | parameterised cases | `test-case` last released 2023-11; everything it does is `rstest`'s `#[case]`; `rstest_reuse` pulls a duplicate `rand`. `rstest` itself solves fixtures, not factories, and was not needed |
| `proptest` (as a factory) | generators | already an edge for property tests; a fixed-seed `Strategy` is not a factory with named defaults |
| `insta` `glob` / `yaml`, `expect-test`, `similar-asserts` | tree snapshots | `insta 1.48.0` with `json` already declared covers the manifest snapshots; `glob` iterates inputs, not an output tree, and adds `globset`; a second snapshot tool means a second update policy |
| hand-written `TcpStream` client | loopback client | zero packages, but the test then owns chunked-body parsing and timeouts — the ~400 lines `minreq` ships and tests. **This is what the generated support module carried until batch F**; replaced, not kept beside |
| `http` + `httparse` as a client | loopback client | both already edges, but `httparse` parses only the head; body framing stays hand-written |
| `oneio` | loopback client | its `https` feature *is* `reqwest`; without it there is no HTTP client; default features reach `flate2`/`bzip2` and a lock with `suppaftp 7.1.0` (RUSTSEC-2026-0271) |
| `mockito`, `wiremock`, `httptest` | OTLP receiver for a test | each puts `hyper` into the CLI's lock closure and fails `the_executable_reaches_no_http_client_crate` before any merit is weighed |
| `tiny_http` | OTLP receiver | last release 2022-10; +5 packages for one route that needs a `2xx` |
| `libc` (`kill`) | child signals | `libc::kill` is an `unsafe extern fn`; the workspace `unsafe` ban applies, exactly as recorded for termios |
| `signal-child` | child signals | one more package for the `kill(2)` `nix` already wraps; upstream declares maintenance "as-is" |
| `std` `ChildExt::send_signal` | child signals | unstable on 1.94.0 (`unix_send_signal`); `Child::kill()` is `SIGKILL` and proves nothing about clean shutdown |

## 4. Custom code, and the requirement that forces each

| Custom code | Forced by |
|---|---|
| `renvor_testkit::factory` — `Sequence`, `Factory<T>`, `UserFactory`/`UserDraft`, `ItemFactory`/`ItemDraft` | no maintained factory crate exists; the generated `tests/starter.rs` and the resource test need deterministic rows with named defaults and explicit overrides, drawn from the injected entropy and clock (FR-049), and nothing here may depend on a driver |
| the generated support module's OTLP receiver (`httparse` over `std::net::TcpListener`, `cap_observability` rows only) | every mock-server crate reaches `hyper`; the exporter needs one route answering `2xx` and the test needs the recorded bodies |
| `renvor_testkit::client` (over `minreq`) | one signature shared by every generated test — `http(address, method, path, headers, body) -> Reply` — with the JSON content type and the zero-length body rule the starter's write routes expect |
| `renvor_testkit::app::TestApplication` | socket-free dispatch through the registry with the caller's providers (FR-050); the shape `renvor-auth-http`'s suite hand-wrote in Phase 009 |

## 4a. The correction round (2026-09-05): nothing added

The seventeen Native Codex fixes added **no dependency**. The wizard's multi-select is
`cliclack::MultiSelect`, from the prompt library already in the lock; the reserved-word table is a
sorted `&[&str]` in `generate.rs` (a crate carrying the two engines' keyword lists was considered
and rejected as a larger surface than a static table — `sqlparser` would bring a full SQL parser
for one lookup); the merged-tree verification reuses `generate::verify`; the kernel's shared state
is `std::sync::Arc`. `Cargo.lock` is unchanged by the round.

## 5. Advisory queries, with positive controls

- **Query A — local clone of the RustSec advisory database**, HEAD `5a0ebedfe8bdd2e295b171f4162f8c977bcad9a5` (2026-09-02), by `ls crates/<name>/`. Positive controls returned: `chrono` → RUSTSEC-2020-0159; `time` → RUSTSEC-2020-0071, RUSTSEC-2026-0009. Candidates: `minreq`, `httparse`, `fake`, `rstest`, `factori`, `insta`, `expect-test`, `oneio`, `mockito`, `wiremock`, `httptest`, `signal-child`, `cliclack`, `minijinja` — **no directory**. Hits on evaluated names that do not affect the versions in use: `http` (RUSTSEC-2019-0033/-0034, patched ≥ 0.1.20; 1.5.0 unaffected), `tiny_http` (RUSTSEC-2020-0031, patched ≥ 0.8.0), `nix` (RUSTSEC-2021-0119, patched ≥ 0.23.0; 0.28.0 unaffected).
- **Query B — `cargo audit` 0.22.1**, 1239 advisories loaded. Positive control: a scratch package pinning `chrono = "=0.4.19"` reported RUSTSEC-2020-0159 and RUSTSEC-2020-0071 (exit 2). A 32-member candidate workspace at the versions above: **one** vulnerability, RUSTSEC-2026-0271 (`suppaftp 7.1.0`), present only in `oneio`'s lock block — rejected above.
- The gate's own `cargo deny check` (advisories, bans, licences, sources) is run on both toolchains at the checkpoint (`phase-011-evidence.md` §final gates).

## 6. Not verified

- `fake`/`arbitrary` output stability across releases — no document promises it; moot after rejection.
- `cargo-insta`'s `--unreferenced=reject` flag — the binary is not installed; the snapshot policy in `template-contract.md` 1.1.0 does not depend on it (`INSTA_UPDATE=no` in the gate and a `.snap.new` check).
