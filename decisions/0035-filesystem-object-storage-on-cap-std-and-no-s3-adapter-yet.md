# ADR-0035: Ship object storage on a `cap-std` filesystem root, and do not ship an S3 adapter until one passes the gates

| Field | Value |
|---|---|
| **ID** | 0035 |
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

PLAN §7.3 plans `renvor-storage` — *"object/file storage port and adapters"* — and §20 Phase 010
requires *"selected maintained adapters"*. The workspace already carries `cap-std` 4.0.3
(capability-based filesystem access, floored by `deny.toml` after GHSA-hp8f-xmx4-4qrg) for the
CLI's transactional generation.

Every S3-compatible client was measured against this workspace's graph with the real `deny.toml`
(`package-decisions.md` §E):

| Candidate | Result |
|---|---|
| `object_store` 0.14.1 | `aws` → `reqwest 0.13` → `rustls-platform-verifier` → `webpki-root-certs` (CDLA-Permissive-2.0) on wasm32 → **licenses FAILED**; also mandatory `humantime 2.4.0`, **unmaintained** (RUSTSEC-2025-0014) |
| `opendal` 0.58.2 | +37 packages; the same platform-verifier licence failure; duplicate `base64`, `hashbrown`, `getrandom`, `digest` |
| `aws-sdk-s3` 1.144.0 | declares **MSRV 1.94.1**, above the 1.94.0 floor; an older pin would freeze a 170-package stack |
| `rust-s3` 0.37.2 | **`webpki-roots`** (banned), **MPL-2.0** (`attohttpc`), CDLA, and `quick-xml 0.38` carrying **RUSTSEC-2026-0194/0195** (patched ≥ 0.41) → advisories **and** licenses FAILED |

`cargo deny` evaluates every target because `deny.toml` declares no `[graph] targets`, and the
file's own comment says why: a dependency licence-incompatible on one platform is still
incompatible. The wasm32-only CDLA crate is therefore counted.

## Decision

1. **The port** (`ObjectStore`: put, get, head, delete) with a key validator that refuses empty
   segments, `.` and `..`, a leading `/`, backslashes, control characters, and Windows reserved
   device names; sizes bounded (default 64 MiB, cap 1 GiB); content types bounded to the RFC 9110
   grammar.
2. **The filesystem adapter**, behind `filesystem`, roots every operation in a
   `cap_std::fs::Dir` and writes through `cap_tempfile::TempFile` then `replace`, so a reader
   never sees a partial object and a traversal is refused by the capability even if the validator
   were wrong. Boot proves the root is writable with a probe file. `cap-tempfile` 4.0.3 is the
   only new package.
3. **The memory substitute**, with a byte capacity.
4. **No S3 adapter ships in this phase.** FR-062 made the adapter conditional on a candidate
   passing the gates; none did. The measurements are recorded here and in the phase limitations
   with **three routes** for a later phase, none taken unilaterally:
   - a maintainer policy decision to restrict `deny.toml`'s `targets` to the three supported
     platforms (ADR-0011), which removes the wasm-only CDLA crate from evaluation — after which
     `object_store`'s unmaintained `humantime` remains to be dispositioned;
   - `object_store` through its custom-connector route, avoiding `reqwest` and the platform
     verifier, if that route proves to exist without the verifier;
   - a Renvor S3 client over `hyper` + `hyper-rustls(native roots)` under a custom-infrastructure
     record with an exit strategy.
5. **`put` is last-writer-wins**, stated; there is no conditional put.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **`object_store` with `aws`** | fails the licence gate as configured; carries an unmaintained crate; adopting it means changing a policy this record has no authority to change |
| **`opendal`** | the same licence failure, 37 packages, an MSRV that moved twice in two months |
| **`aws-sdk-s3` pinned at 1.137.0** | freezing a 170-package stack at a version chosen for its MSRV rather than its fitness, with `default-features = false` required to avoid a second `hyper 0.14`/`rustls 0.21` line carrying open advisories |
| **`rust-s3`** | fails on a banned crate, a copyleft licence, and unpatched advisories |
| **A hand-written SigV4 client now** | custom infrastructure for a solved problem, with an S3 API surface that is not narrow; principle III forbids building it *"merely to own the implementation"* |
| **Widening `deny.toml` in this phase to admit `object_store`** | a licence-policy change made to obtain a dependency — the shape of decision the licence gate exists to slow down; it belongs to the maintainer as a separate, named decision |
| **`std::fs` with the validator alone** | one layer of defence where two are cheap; the capability is already in the graph |

## Consequences

- **The storage capability is complete for local and single-host deployments and absent for
  object stores** — stated in the crate README, the contract, and the limitations with an owner.
- **One package** (`cap-tempfile`) enters only under `filesystem`.
- **Windows behaviour** of `replace` over an existing object is proven by the platform legs, not
  locally.
- **What would reverse decision 4**: any of the three routes, each a superseding or additional
  record.

## Compliance

- **Constitution III** — no custom S3 client; the filesystem adapter reuses a maintained
  capability crate already audited by this repository.
- **Constitution VI** — path traversal prevented at two layers; bounded sizes; atomic writes.
- **Constitution XI, XII** — licence and advisory gates unchanged; an unshipped capability is named
  as unshipped rather than implied.
- **FR-062** — the conditional resolved by measurement.
