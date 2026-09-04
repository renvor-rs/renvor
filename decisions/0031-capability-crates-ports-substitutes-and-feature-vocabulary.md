# ADR-0031: Five capability crates with narrow ports, deterministic substitutes, and off-by-default adapters

| Field | Value |
|---|---|
| **ID** | 0031 |
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
> Acceptance accepts the decisions this record makes and **not the reading it withdrew**: its claim
> of compliance with constitution principle VII is withdrawn below, and that obligation is
> **deferred** under [W-024](../governance/waivers.md) (and, for Phase 009's auth starter, W-023),
> not met.
>
> Accepted **2026-09-04** against head `5f26334b394f20ae86b3037ccb77a23705c40ed9`,
> tree `47aeb8d8fda9e07bd5a4520406cef4eada44273c`. W-021 expires **2027-02-11**, or
> immediately when a qualified independent human reviewer becomes available — whichever
> is first.

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
  `--auth`. **Contested (2026-09-04, the Phase 010 correction round).** The maintainer's review
  rejected this reading: principle VII says each governed choice "becomes mandatory in both
  interfaces on the day its capability ships", and the five capabilities shipped in this phase.
  The narrowest literal implementation — a mandatory `--capabilities` choice recorded in
  `renvor.toml` — would solicit and record a choice the generator cannot honour (a generated
  project declares no Renvor dependency until one is publishable), which the same principle
  forbids; honouring it needs the Phase 011 generator scope. The obligation is therefore
  **unmet and unresolved by this record**: it is `governance/phase-010-limitations.md` L-14, and
  its resolution (Phase 011 scope, a ruling on what "ships" means for library-only phases, or a
  waiver) is the maintainer's. This paragraph does not claim compliance with principle VII.
  **Deferred 2026-09-04 under W-023 (Phase 009's `--auth`) and W-024 (this phase's
  capabilities)** (`governance/waivers.md`): the maintainer ruled that "ships" is not
  reinterpreted; the obligation stays unmet and visible as L-14, with an absolute expiry of
  **2026-10-04** or earlier when Phase 011 implements and proves the generator support.
- **What would reverse this**: a decision to consolidate the crates would be a public-API change
  under C-S1 while the window is open, and would need a superseding record.

## Compliance

- **Constitution III** — package-first, no silent replacement: every adapter is a maintained
  crate; every substitute is explicit.
- **Constitution IV, VI** — fail-closed startup and bounded work: decisions 2, 3, 5.
- **Constitution VIII** — feature isolation: decision 6, asserted with controls.
- **Constitution VII** — **not claimed**; see the contested consequence above and L-14, deferred
  under W-024 (and W-023 for Phase 009's auth starter), unmet until Phase 011 proves the support.
- **PLAN §7.3, §7.4, §20 Phase 010** — the crate set and the feature vocabulary as planned.
- **PLAN §21 items 13 and 14** — item 13 is ADR-0032; item 14 (documentation platform) is unchanged
  by this phase.
