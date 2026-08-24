# ADR-0019: Generate a local container profile, and choose Valkey on the licence

| Field | Value |
|---|---|
| **ID** | 0019 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-013 |
| **Review date** | 2026-08-24 |
| **Superseded by** | *(not superseded)* |

> **This record is `accepted` under W-013. The review behind it was NOT independent.**
>
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded **independent**
> review before acceptance. **No independent human review of this record has occurred, and none is
> claimed.** Acceptance rests on **[W-013](../governance/waivers.md)**, expiring **2027-02-11** or
> immediately when a qualified independent human reviewer becomes available — whichever is first.
> W-013 covers this record, ADR-0016, ADR-0017 and ADR-0018 as one coupled Phase 006 decision.

## Context

`--container` generated a `Dockerfile` and a five-line `compose.yaml` with one service. Once
Phase 006 gave generated projects migrations and a repository, that profile named nothing to run
them against — the operator had to write the database service themselves, which is where the
security decisions actually get made.

This scope addition was authorised by the maintainer on 2026-08-24, in writing, as an explicit
extension of Phase 006.

## Decision

`--container` generates a complete **local development** profile: the selected database service,
an optional cache service, `.dockerignore`, `.env.example`, a `.gitignore` that excludes `.env`,
and a `[container]` section in `renvor.toml`. Six flags and six wizard questions cover it, and both
resolve through one function.

Four choices carry the weight.

### 1. The cache engine is Valkey, and the reason is the licence

Redis relicensed in March 2024 to RSALv2/SSPLv1, neither OSI-approved; Redis 8 added AGPLv3 — OSI-
approved but strongly copyleft — as a third option. Valkey is the Linux Foundation fork of Redis
7.2.4 under **BSD-3-Clause**, the permissive terms Redis left behind, and is now the default in
Debian, Ubuntu, Fedora, and Arch.

This repository already refuses a **crate** for its licence: sqlx's obvious TLS feature was
rejected because `webpki-roots` is CDLA-Permissive-2.0 and not on `deny.toml`'s allow-list (see
ADR-0016). Handing a generated project an SSPL image while refusing a CDLA crate would be two
standards. The engine is recorded rather than prompted for — a menu with one real option and one
worse option is a prompt pretending to be a decision.

### 2. Generation never writes a credential

`.env.example` ships with empty placeholders. `.env` is never written, because writing it would
mean **inventing** a password and then either printing it into terminal scrollback and the CI log
of anything that runs `renvor new`, or leaving a working credential silently on disk.

`compose.yaml` uses `${VAR:?message}`, so a missing secret **stops the containers** with a message
naming the variable rather than substituting an empty string — which PostgreSQL would accept as
"trust anybody". No flag accepts a password; `ContainerSettings` has no field that could hold one,
and `renvor check` **refuses** a manifest that grew a `database_password`, so the guarantee is
enforced rather than asserted.

### 3. Health checks were verified to fail, not only to pass

A health check that cannot fail is worse than none: `depends_on: service_healthy` then starts the
application against a dead server. Each was run against a live server **and** a dead endpoint:

```text
pg_isready -U … -d …           live 0   dead port 2
mysqladmin ping --silent       live 0   dead port 1   unreachable host 1
valkey-cli ping                live 0   NO AUTH 0     WRONG PASSWORD 0   <- unusable
valkey-cli ping | grep -q PONG live 0   NO AUTH 1     WRONG PASSWORD 1
```

The plain `valkey-cli ping` is the form most examples use and it can only ever report healthy.
None of the three carries a credential, because `docker inspect` and the container's process list
both expose the command text; `valkey-cli` reads `REDISCLI_AUTH` from the environment instead.

### 4. Pinned tags, deliberately not digests

Every image pins the patch version this phase's suites ran against. A floating `postgres:17` would
resolve to a different server between two `renvor docker up` runs, which is how a local
reproduction stops reproducing.

A digest would pin harder, and is **rejected**: it is architecture-specific in the single-platform
form people copy, unreadable to a human deciding whether an upgrade is due, and goes stale the
first time upstream republishes a security rebuild under the same tag — leaving a generated project
pinned to a known-vulnerable image with no signal that it is. Recorded as L-10 rather than claimed
as immutability the generated project has no mechanism to maintain.

## Consequences

- **PostgreSQL 17 and 18 mount different paths.** The 18 image moved `PGDATA` under a versioned
  directory and declares its volume one level up. Using 17's path on an 18 server would put the
  named volume where the server never writes, so `renvor docker down` would silently destroy the
  database it promises to keep. Read off the running images, not remembered, and asserted.
- **Every published port binds `127.0.0.1`.** Verified functionally as well as textually: the port
  is unreachable on this machine's LAN address while reachable on loopback.
- **`renvor docker down` keeps the data**, and no renvor command removes a volume. A test prevents
  a destructive flag being added without its own confirmation contract, the way `renvor tls trust`
  has one.
- **Generation stays offline.** It renders files and starts nothing, verified with proxies and
  `DOCKER_HOST` pointed at a dead endpoint — and structurally, since `renvor-cli` resolves neither
  `sqlx` nor any HTTP client.
- **Template version 3 → 4**, with a real version-3 project captured by building the v3 generator
  in a detached worktree. It still validates, and is not silently upgraded by `renvor check`.

### The cache claims nothing

The cache container is **local infrastructure only**. No `renvor-cache` crate, no client, no
application API, no middleware. Renvor's runtime cache capability and its adapter arrive in Phase
010. This is stated in the README, in `compose.yaml`, and in `renvor.toml` as
`cache_wired_into_application = false` — in the places a reader looks, not only in this record.
Carried as L-9.
