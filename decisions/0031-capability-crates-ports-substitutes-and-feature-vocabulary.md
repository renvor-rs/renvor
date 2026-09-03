# ADR-0031: Five capability crates with narrow ports, deterministic substitutes, and off-by-default adapters

| Field | Value |
|---|---|
| **ID** | 0031 |
| **State** | `proposed` |
| **Reviewer** | *(required to enter `accepted`)* |
| **Review date** | *(required to enter `accepted`)* |
| **Superseded by** | *(not superseded)* |

> **A record MUST NOT be marked `accepted` without a recorded independent review** (spec FR-013).
> Who qualifies as an independent reviewer is established in `GOVERNANCE.md`. Where no independent
> reviewer exists, acceptance requires a waiver recorded in `governance/waivers.md` with an absolute
> expiry date — the gap is never left unrecorded. **This record carries no authority while
> `proposed`.**

## Context

PLAN.md §20 Phase 010 requires *"narrow capability ports; selected maintained adapters; explicit
durable-job storage selection; retry/idempotency/backoff policies; structured logs, metrics, traces,
health, and redaction; local test substitutes"*, and accepts the phase only when *"missing required
capability fails startup; MySQL applications never acquire PostgreSQL implicitly; retries are bounded
and observable; trace context propagates safely; capability-disabled builds exclude their
dependencies."*

§7.3 plans five crates — `renvor-cache`, `renvor-jobs`, `renvor-mail`, `renvor-storage`,
`renvor-observability` — and permits consolidation only when *"a boundary adds no independent
contract"*. §7.4 fixes the feature vocabulary: `capability-cache`, `capability-jobs`,
`capability-mail`, `capability-storage`, `observability-otel`.

Phase 009 already shipped two of the ports' predecessors as ports-with-test-sinks
(`renvor_auth::MailPort`, `renvor_auth::AuditSink`) and recorded in `mail.rs` that *"Phase 009
ships a port; Phase 010 ships the adapter"*. Twelve strings in the tree promise a capability
"arrives in Phase 010".

Constitution principle III: *"A required package or external capability MUST NOT be silently
replaced by an in-memory, insecure, or differently durable implementation."* Principle IV: *"Required
configuration and dependencies MUST be validated before readiness."* Principle VIII: feature
isolation *"checked through minimal, individual, representative combination, and
all-supported-feature builds."*

## Decision

1. **Five crates, as planned.** Each holds one port (a trait plus its value types), a closed error
   enum, and a deterministic substitute available outside `cfg(test)`; each production adapter is
   behind an **off-by-default** feature of its own crate (`valkey`, `smtp`, `auth`, `filesystem`,
   `http`, `otel`). No public signature of any port names a third-party type; the facade's
   `tests/facade_boundary.rs` asserts it for every re-export.
2. **Every adapter and every substitute is a kernel `Provider`** offering a named `CapabilityId`
   (`cache`, `jobs`, `mail`, `storage`). An application that depends on a capability and registers
   no provider for it fails at **Register** with `DependencyMissing`; an adapter whose backend does
   not answer fails at **Boot** with a redacted diagnostic naming provider, category, and action, and
   the kernel rolls back. This is the existing kernel rule (C-G11, C-L2) applied to capabilities;
   nothing new is invented for it.
3. **No silent fallback, structurally.** A substitute is a value the author constructs and
   registers; no adapter constructs one, no configuration key selects one, and no failure path
   substitutes one. A cache miss is `Ok(None)`; a cache *failure* is `Err`.
4. **The facade** gains the five §7.4 features, each off by default, re-exporting each port
   narrowly. The lean facade (`--no-default-features`) resolves no capability crate.
5. **Bounds are typed and capped.** Every key, value, payload, subject, body, TTL, timeout, pool,
   queue depth, concurrency, and lease has a default and a hard cap stated in
   `contracts/capabilities-contract.md`; a configuration above its cap fails Validate naming the
   key, constraint, and layer.
6. **Isolation is proven from the graph.** `xtask` step 7 gains one row per adapter feature in
   each direction (off → forbidden crates absent; on → present), a row per capability crate under
   `--all-features` asserting no database driver, rows asserting `renvor-sqlx`/`renvor-seaorm` with
   `db-mysql` plus every capability feature resolve no `sqlx-postgres`, and a row asserting the
   workspace's `rustls` feature set contains exactly one crypto provider (see ADR-0033).
7. **The twelve promises are made true**: every in-tree "arrives in Phase 010" string is corrected
   to state that the library shipped and that generated projects gain the wiring in Phase 011,
   which owns generators; `cache_wired_into_application = false` stays true and asserted.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **One `renvor-capabilities` crate with four features** | Puts every adapter's optional dependency behind one feature table, so a consumer wanting a cache resolves the job worker's runtime and the step 7 rows become one crate's Cartesian product. §7.3 permits consolidation only when a boundary adds no contract; each of these adds its own bounds, errors, and adapter graph |
| **Substitutes only under `cfg(test)`** | An author testing their own service needs the recording mailbox and the memory job store, exactly as `RecordingMailSink` and `FixedEntropy` are public today. Hiding them forces authors to write fakes that drift from the contract |
| **A configuration switch (`backend = "memory"`) selecting the substitute** | This is the silent fallback principle III forbids, one config edit away from production. The substitute is constructed in code, visibly |
| **Adapters as separate crates (`renvor-cache-valkey`, …)** | Nine publishable crates instead of five, each a bookkeeping line in three lists, for a boundary a feature already draws and step 7 already proves. Splitting *"for naming alone is forbidden"* (§7.3) |
| **Default-on adapter features** | Phase 004 chose off-by-default for the transport because *"enabling a transport is a decision"*; an SMTP client, a RESP client, and an OTLP stack are each a larger decision than that |

## Consequences

- **Publishable count 13 → 18**, in `RELEASING.md`, `release-dry-run.yml`, and the `xtask`
  assertion that has caught every previous omission.
- **Six new step 7 row groups and four new census rows** — the gate grows with the claims.
- **Two adapters ship on maintained crates that need one permissive licence each added to
  `deny.toml`** (ADR-0033, ADR-0034); one adapter does not ship at all because no candidate passed
  the gates (ADR-0035). The vocabulary is complete; the adapter set is what the gates admitted.
- **A `renvor-cli` change** limited to string corrections and their pinning tests; no new flag and
  no new wizard question, because constitution principle VII binds a choice *"once its capability
  ships"* and the capability that ships here is a library — the same reading Phase 009 made for
  `--auth`.
- **What would reverse this**: a decision to consolidate the crates would be a public-API change
  under C-S1 while the window is open, and would need a superseding record.

## Compliance

- **Constitution III** — package-first, no silent replacement: every adapter is a maintained
  crate; every substitute is explicit.
- **Constitution IV, VI** — fail-closed startup and bounded work: decisions 2, 3, 5.
- **Constitution VIII** — feature isolation: decision 6, asserted with controls.
- **PLAN §7.3, §7.4, §20 Phase 010** — the crate set and the feature vocabulary as planned.
- **PLAN §21 items 13 and 14** — item 13 is ADR-0032; item 14 (documentation platform) is unchanged
  by this phase.
