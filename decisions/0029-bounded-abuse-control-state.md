# ADR-0029: Bound abuse-control state by construction, not by retention

| Field | Value |
|---|---|
| **ID** | 0029 |
| **State** | `proposed` |
| **Reviewer** | *(none — not reviewed)* |
| **Review date** | *(not reviewed)* |
| **Superseded by** | *(not superseded)* |

> **`proposed`, not `accepted`.** No independent review, and no authority has been given to accept.

`plan.md` risk **R-2** requires this decision to exist *before* the abuse-control table ships. It did
not, and the table shipped in batch C carrying the defect — which `data-model.md` recorded at the
time. This record is written after the fact and says so.

## Context

Phase 009 FR-063 bounds six authentication flows. FR-066 requires the counter storage to be
**bounded and expiring**; FR-067 requires that *"an attacker MUST NOT be able to grow the key space
without bound"*. Security checklist question **SQ-4** asks the sharp version: is the key space
provably bounded when the attacker chooses the account identifier in a forgot-password request?

Batch C shipped this:

```sql
PRIMARY KEY (dimension, key_hash, window_start)
```

`key_hash` is a digest of caller-supplied text, and `window_start` is in the key. **A digest is a
total function on an infinite domain.** `n` distinct addresses produce `n` rows; hashing bounded the
key's *width* and left its *cardinality* untouched. Putting the window in the key multiplies that
again, once per window, forever. Neither is fixed by a longer hash, a keyed hash, or an index.

This is not confined to the account axis. **IPv6 alone defeats the naive design**: an attacker
holding a routine `/64` has 2^64 source addresses, so "one row per address" is unbounded before an
email is ever submitted.

Package research (`specs/009-.../package-decisions.md`) found no crate that solves it. `governor` —
the ecosystem's only serious rate-limiter, 73.4M downloads — has an unbounded keyed store whose only
cleanup is a manual `retain_recent()`, and its persistence hook is **synchronous**, so a SQL-backed
store does not compile. `pingora-limits` is the one candidate that bounds storage, and it does so by
being a Count-Min Sketch: **approximate, and over-counting on collision**.

## Decision

**One row per `(dimension, bucket)`, and nothing else in the key.**

Every identifier is mapped, **before any storage call**, into a fixed space:

```
bucket = HMAC-SHA256(server_secret, dimension_tag ‖ 0x1F ‖ key_bytes) & (buckets - 1)
```

- `dimension` is the discriminant of a **closed, fieldless** Rust enum with **ten** variants — one
  per `(flow, axis)` pair actually counted.
- `buckets` is a configured **power of two** in `[256, 1_048_576]`, default `65_536`. A power of two
  makes the reduction a mask, so there is no modulo bias toward low indices.
- The window is **content of the row**, not part of its key. Rolling a window is an `UPDATE`.

```
max_rows = |AttemptDimension| × buckets = 10 × 65_536 = 655_360
```

**and that bound holds whether or not any sweep ever runs.** Both factors are bounded twice — in
Rust where the value is computed, and by two schema `CHECK` constraints.

The count is computed in **Rust**, not SQL: `saturating_add` with a ceiling that fits `BIGINT`, so
there is no wrap and no engine overflow error inside a rate-limit check on an unauthenticated
endpoint.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **Keep the digest key, add a retention sweep** (the shape the commissioned package research recommended) | A retention sweep is a **race** against an attacker who chooses the insert rate. A bound a `DELETE` has to keep winning is not a bound. |
| **Key the account axis on a resolved `UserId`, and do not count unknown accounts** | Bounds the key space perfectly — `rv_auth_user`'s primary key is already finite — and builds a **complete enumeration oracle**. Send five forgot-password requests; if the sixth is refused, the account exists. *Being rate-limited* is itself the signal, however generic the response body. |
| **A Count-Min Sketch** (`pingora-limits`) | Fixed memory regardless of key count, and **over-counts on collision with unkeyed hashes** — so an attacker can compute a victim's counters and push them over a threshold. Probabilistic counting is not an acceptable substrate for a control that refuses service. |
| **An in-process limiter** (`governor`, `ratelimit`, `leaky-bucket`) | FR-069 requires the state to be persisted and proven on all four rows. All of these are in-memory, and a fleet of processes would each enforce its own limit. |
| **Redis** (`brakes`, `barnacle-rs`, `limitador`) | The four-row requirement is PostgreSQL **and** MySQL. Adding a second datastore to satisfy a rate limit is a large operational dependency for a framework to impose. |
| **Unkeyed hashing into buckets** | An attacker could compute which bucket a victim occupies and fill it directly. HMAC under a server secret makes the assignment unguessable from outside, which is what turns a targeted lockout into filling all `buckets` at `limit` each. |

## Consequences

**Bucketing is lossy, and distinct accounts share counters.** This is the same objection that
disqualified the Count-Min Sketch, and it applies here. It is accepted on three grounds, and none of
them eliminates it:

1. The mapping is **HMAC-keyed**, so collisions are accidents rather than targets.
2. The refusal is a **windowed limit that expires**, not a lockout. Nothing is disabled.
3. The residual — untargeted degradation for a fraction of users, at a cost the network axis is
   charging the whole time — is priced and observable rather than removed.

**Rotating the server secret re-randomises every bucket assignment**, discarding accumulated counts
and failing **open** for one window. That is the same cost as restarting with an empty table, and it
is the operator's decision.

**Adding a seventh flow is now expensive.** `|AttemptDimension|` is a factor in the bound, a `CHECK`
constraint, a four-row assertion, and four mutations. Registration is deliberately *not* bounded for
exactly this reason, and that gap is recorded rather than closed quietly.

**The batch C migration was corrected in place rather than patched forward.** Four reasons, in
`evidence/sq-4-bounded-abuse-state.md` §7 — the decisive one being that a forward migration would
leave the unsound table in the recorded history, to be created and dropped on every fresh deployment.

## Compliance

- **Constitution principle V** (decisions defined against measurements): the row bound is a formula
  checked by a test that submits 100,000 distinct identifiers and 400 distinct IPv6 addresses, and by
  a four-row assertion that runs with `prune` never called.
- **PLAN.md** risk R-2: resolved, late, and the lateness is recorded.
- **`contracts/database-portability.md` §3**: the read-modify-write locks the row it read
  (`SELECT ... FOR UPDATE`), so it depends on no isolation level.
- **`contracts/database-portability.md` §7**: one schema statement per migration file.
