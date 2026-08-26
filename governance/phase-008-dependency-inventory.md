# Phase 008 — Dependency Inventory

**Date**: 2026-08-26
**Phase**: 008 — Four-row database hardening
**Authoritative for**: constitution principle III (package-first boundaries), principle VIII
(feature isolation), principle XI (supply-chain integrity)
**Licence policy**: [`deny.toml`](../deny.toml) is the enforced allow-list.

---

## 1. Selected — runtime

**None.** Phase 008 added no runtime dependency to any crate.

## 2. Selected — development

**None.** Phase 008 added no development dependency to any crate.

## 3. Where one was considered and not taken

| Candidate | For | Why not |
|---|---|---|
| `futures-util` (or `futures`) | driving a variable number of concurrent, borrowing futures in the concurrency suite | `tokio::join!` already does it. `renvor-testkit` depends on `tokio` for time control, and `sync` (barriers) and `macros` (`join!`) were already in its feature set. The fixed arity is checked against `CONCURRENT_WRITERS` by a `const` assertion, and the number is pinned to the pool capacity for an independent reason, so the flexibility a combinator would buy has no use here |

The crate is already in the workspace graph — `renvor-http` depends on it directly — so this was not
a question of introducing a new package to the lockfile. It was still declined: a new **direct
edge** from `renvor-testkit` is a new thing to keep correct, and no capability was missing.

## 4. What this phase added instead

Every Phase 008 deliverable is built from what was already there:

| Deliverable | Built from |
|---|---|
| concurrency and idempotency suite | `tokio::sync::Barrier`, `tokio::join!` |
| portability contract | `sqlx` and `sea_orm`, already selected |
| upgrade fixtures | the adapters' own migration runners |
| startup diagnostics | `core::fmt`, `core::error::Error` |
| backup/restore guidance | the tools inside the four pinned images — **no host-side installation** |
| four-row census | `std::process::Command`, in a deliberately dependency-free `xtask` |

`xtask` remains dependency-free. Giving the checker dependencies of its own would put its supply
chain outside the check it performs.

## 5. Verification

`cargo deny check` is step 6 of the verification sequence and passed on the Phase 008 head. No
exception is requested, and none was needed: the dependency graph is unchanged from Phase 007
except for the workspace's own path crates.
