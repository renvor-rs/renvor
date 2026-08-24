# Phase 007 — Dependency Inventory

**Date**: 2026-08-24
**Phase**: 007 — SeaORM parity
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
| `sea-orm` | 2.0.2 | MIT OR Apache-2.0 | The ORM. `default-features = false`; see §3 for the feature deliberately **not** taken |
| `sea-query-sqlx` | 0.9.1 | MIT OR Apache-2.0 | Supplies `SqlxValues`, the `sqlx::Arguments` implementation that binds a `sea_query::Values`. **The same crate at the same version SeaORM itself binds through** — a dependency rather than a reimplementation, under principle III |
| `async-trait` | 0.1.92 | MIT OR Apache-2.0 | Already how SeaORM declares `ConnectionTrait`; Renvor's impl must use the same attribute |

`sqlx` stays at **0.9.0**, the version Phase 006 selected. `sea-orm` 2.0.2 requires `sqlx = "0.9.0"`
exactly, so **one** `sqlx` resolves across the workspace — verified, not assumed:

```
$ grep -c '^name = "sqlx"' Cargo.lock
1
$ grep -A1 '^name = "sqlx"' Cargo.lock | grep version
version = "0.9.0"
```

## 2. MSRV: an exact match, with zero headroom

`sea-orm` 2.0.2 declares `rust-version = "1.94.0"`. Renvor's MSRV is **1.94.0**.

That is a match, not a margin. Any `sea-orm` patch release that raises its MSRV becomes
un-adoptable without a Renvor MSRV bump, which is a MAJOR decision under ADR-0003. Recorded here
rather than discovered at the next `cargo update`. `sea-query-sqlx` 0.9.1 declares the same.

## 3. One `sea-orm` feature rejected, on a measured policy failure

| Feature | Why rejected |
|---|---|
| `runtime-tokio-rustls` | It resolves SQLx's `tls-rustls` alias chain, which ends at **`webpki-roots`** — licensed **CDLA-Permissive-2.0**, which is not on `deny.toml`'s allow-list. The same failure Phase 006 recorded for `sqlx`'s own `tls-rustls`. `runtime-tokio` selects the runtime and **nothing** about TLS, so TLS comes from this crate's own aligned `sqlx` dependency using `tls-rustls-ring-native-roots` — already approved. Cargo unifies the two into one `sqlx` |

`macros` **is** taken here, and is refused in `renvor-sqlx`. That is not an inconsistency: SQLx's
`macros` enables the checked `query!` macros, which need a live `DATABASE_URL` at **compile** time
or a committed cache. SeaORM's `macros` is the `DeriveEntityModel` derive set — purely derive-based,
no database, no cache — so the offline generation path Phase 004 guarantees is unaffected.

`sea-orm-cli` is **absent**, deliberately: its defaults enable unwanted drivers and TLS choices, and
nothing shells out to it.

`sea-orm-migration` is **absent**, on a capability gap rather than a policy one — see
[ADR-0022](../decisions/0022-one-migration-history-on-sqlx-engine.md).

## 4. What actually enters the graph

The lockfile gained **78** entries. Most are `sea-orm`'s *optional* dependencies — `arrow*`,
`pgvector`, `rkyv`, `borsh`, `bigdecimal`, `rust_decimal`, `sea-schema` and so on — which Cargo
records in `Cargo.lock` whether or not a feature enables them. A lockfile entry is not a dependency.

Measured on the **resolved** graph, `renvor-seaorm` with both drivers adds **27** packages over
`renvor-sqlx` with both drivers:

```
aho-corasick  async-stream  async-stream-impl  async-trait  darling  darling_core
darling_macro  derive-where  derive_more  derive_more-impl  fnv  ident_case  itertools
ordered-float  pluralizer  regex  regex-automata  regex-syntax  renvor-seaorm  sea-bae
sea-orm  sea-orm-macros  sea-query  sea-query-derive  sea-query-sqlx  strum  unicode-xid
```

Licences of the load-bearing additions:

| Package | Version | Licence |
|---|---|---|
| `sea-orm` | 2.0.2 | MIT OR Apache-2.0 |
| `sea-query` | 1.0.2 | MIT OR Apache-2.0 |
| `sea-query-sqlx` | 0.9.1 | MIT OR Apache-2.0 |
| `sea-orm-macros` | 2.0.2 | MIT OR Apache-2.0 |
| `sea-bae` | 0.2.2 | MIT |
| `derive-where` | 1.6.1 | MIT OR Apache-2.0 |
| `derive_more` | 2.1.1 | MIT |
| `ordered-float` | 4.6.0 | MIT |
| `pluralizer` | 0.5.0 | MIT/Apache-2.0 |
| `strum` | 0.28.0 | MIT |
| `itertools` | 0.14.0 | MIT OR Apache-2.0 |

**The whole policy passes, with `all-features = true`:**

```
$ cargo deny check
advisories ok, bans ok, licenses ok, sources ok
```

That command evaluates features a crate merely *exposes*, which is why the `runtime-tokio-rustls`
rejection above is enforced rather than merely intended.

## 5. Feature isolation, measured with positive controls

A count of zero proves nothing without proof the walk can see what is present. Every line below has
both directions, and the run failed once already — on a control, because the harness itself was
building its pattern wrongly.

```
renvor-seaorm + db-postgres:  sqlx-mysql 0, sqlx-sqlite 0, webpki-roots 0, rsa 0,
                              sea-schema 0, sea-orm-cli 0, sea-orm-migration 0
                              CONTROLS: sqlx-postgres 1, sea-orm 1, sea-query-sqlx 2
renvor-seaorm + db-mysql:     sqlx-postgres 0, sqlx-sqlite 0, webpki-roots 0, rsa 0
                              CONTROLS: sqlx-mysql 1, sea-orm 1, sea-query-sqlx 2
renvor-seaorm, no driver:     sqlx-postgres 0, sqlx-mysql 0, sqlx-sqlite 0
                              CONTROLS: sea-orm 1, sqlx 1
renvor-database --all:        sea-orm 0, sqlx 0, renvor-seaorm 0, renvor-sqlx 0
                              CONTROL: renvor-database 1
renvor-core --all:            sea-orm 0, sqlx 0, renvor-seaorm 0, renvor-sqlx 0
                              CONTROL: renvor-core 1
```

**And the two adapters are siblings**, which is what keeps a SeaORM application's graph free of a
direct-SQLx crate:

```
renvor-seaorm:  renvor-sqlx 0     CONTROL: renvor-database 1
renvor-sqlx:    sea-orm 0, renvor-seaorm 0     CONTROL: sqlx 1
```

## 6. Generated projects add nothing

A project generated with `--orm seaorm` declares **no dependencies at all** — the same as
`--orm sqlx`, and for a stronger reason. `sea-orm` is published, so it *could* be declared; it is
not, because generation runs the staged project's own `cargo fmt`, `clippy`, `build`, `test` and
`run` before placing it, and a real dependency would make `renvor new` resolve and compile SeaORM and
SQLx from the registry. Renvor guarantees offline generation, and one ORM choice is not a reason to
withdraw it. `seaorm_generation_succeeds_offline` pins that with `CARGO_NET_OFFLINE=true`.
