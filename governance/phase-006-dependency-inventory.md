# Phase 006 — Dependency Inventory

**Date**: 2026-08-24
**Phase**: 006 — Persistence (SQLx, PostgreSQL and MySQL), and the container scope addition
**Authoritative for**: constitution principle III (package-first boundaries), principle VIII
(feature isolation), principle XI (supply-chain integrity)
**Licence policy**: [`deny.toml`](../deny.toml) is the enforced allow-list. Every selection below is
on it; **no exception is requested**.

> **Every row was established by compiling and resolving the candidate, or by reading the official
> artifact.** Not one is a README claim.

---

## 1. Selected — runtime

| Package | Version | Licence | Why |
|---|---|---|---|
| `sqlx` | 0.9.0 | MIT OR Apache-2.0 | Async PostgreSQL and MySQL, compile-time-checked types without an object mapper. `default-features = false`; see §2 for the two features deliberately **not** taken |

**Net new runtime packages in the container scope addition: zero.**

`Cargo.lock` and every workspace `Cargo.toml` are **byte-identical** across the two commits that
make up the container work. Verified:

```
$ git diff --name-only 1a83149..HEAD | grep -E 'Cargo\.(toml|lock)$'
crates/renvor-cli/tests/fixtures/phase-006-v3-project/Cargo.toml   <- a TEST FIXTURE
```

The one manifest in the diff is fixture content — a captured template-version-3 project with
`publish = false` and an empty `[dependencies]`. It is not a workspace member.

The container profile therefore adds **no Rust dependency at all**. It is generated text: a
`compose.yaml`, a `Dockerfile`, a `.dockerignore`, and an `.env.example`, rendered by the
`minijinja` engine the generator already used.

## 2. Two `sqlx` features rejected, each on a measured policy failure

Recorded because both are the *obvious* choice, and both fail this repository's own gates.

| Feature | Why rejected |
|---|---|
| `tls-rustls` | An alias for `tls-rustls-ring`, itself an alias for `tls-rustls-ring-webpki`, which resolves **`webpki-roots`** — licensed **CDLA-Permissive-2.0**, which is not on `deny.toml`'s allow-list. `deny.toml` sets `all-features = true`, so a feature this crate merely *exposes* is evaluated. **`tls-rustls-ring-native-roots`** is taken instead: same `rustls`, same `ring`, platform certificate store, passes |
| `mysql-rsa` | Resolves **`rsa`**, which carries **RUSTSEC-2023-0071** with `patched = []` — no fixed version exists. The consequence is stated in `connect_mysql`'s documentation rather than discovered: without RSA key exchange, MySQL's `caching_sha2_password` cannot complete a **first** authentication over a plaintext channel. Use TLS, or a user already in the server's cache |
| `macros` | Not a policy failure, a build-integrity one. The checked `query!` macros need either a live `DATABASE_URL` at **compile** time or a committed `.sqlx` cache. The first makes the build depend on a running server — including in the offline generation path Phase 004 guarantees. `derive` alone gives `FromRow`, which is what is used |

Both rejections are **verified in the resolved graph**, not asserted:

```
webpki-roots  in `cargo tree --workspace --all-features`  ->  0
rsa           in `cargo tree --workspace --all-features`  ->  0
```

The only near-miss is `rustls-webpki`, a **different crate** — the X.509 verifier, not the root
store. Positive control: the same walk surfaces `ring`, `rustls`, and `security-framework`, so it
can see what is there.

## 3. Container images — not Rust dependencies, and chosen on the same policy

Generated `compose.yaml` files reference images. These are not in the dependency graph and
`cargo deny` cannot evaluate them, so the same standard is applied by hand.

| Image | Pinned tag | Licence | Why |
|---|---|---|---|
| PostgreSQL | `17.11-trixie`, `18.6-trixie` | PostgreSQL Licence (permissive) | The versions this phase's suites ran green against |
| MySQL | `8.4.11`, `9.7.2` | GPL-2.0 (server, run as a container — not linked) | Same |
| Valkey | `9.1.1-alpine` | **BSD-3-Clause** | See below |

**Valkey rather than Redis, and the reason is the licence.** Redis relicensed in March 2024 to
RSALv2/SSPLv1, neither OSI-approved; Redis 8 added AGPLv3 — OSI-approved but strongly copyleft — as
a third option. Valkey is the Linux Foundation fork of Redis 7.2.4 under BSD-3-Clause, the terms
Redis left behind, and is the default `redis`-compatible package in Debian, Ubuntu, Fedora, and
Arch.

Refusing a **crate** for CDLA-Permissive-2.0 while shipping an image under SSPL would be two
standards. See [ADR-0019](../decisions/0019-generated-container-profile-and-the-cache-engine.md).

**Tags, not digests.** A digest is architecture-specific in the single-platform form people copy,
unreadable to a human deciding whether an upgrade is due, and goes stale the first time upstream
republishes a security rebuild under the same tag. Carried as L-10 rather than claimed as an
immutability the generated project has no mechanism to maintain.

## 4. Feature isolation, measured with positive controls

A count of zero proves nothing without proof the walk can see what is present.

```
db-postgres only:  sqlx-mysql    0     sqlx-postgres  1  (control)
db-mysql    only:  sqlx-postgres 0     sqlx-mysql     1  (control)
neither:           either driver 0
renvor-cli:        sqlx          0     renvor-database 1 (control)
renvor-cli:        HTTP clients  0     clap/minijinja/cap-std present (control)
```

The last row is load-bearing for FR-063: a generator whose executable cannot resolve an HTTP client
has no ordinary way to pull an image, whatever its code says.
