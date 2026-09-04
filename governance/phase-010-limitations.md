# Phase 010 — Limitations

**Companion to**: [`phase-010-evidence.md`](phase-010-evidence.md)
**Phase**: 010 — Cache, jobs, mail, storage, and observability capabilities
**Count**: **14 retained limitations**; **2 Phase 009 limitations closed** (L-4, L-11) with
measurement rather than prose.

Every row states **what**, **why it was not closed**, and **who it belongs to**. The phase's
working record — spec, clarifications, research, package decisions, plan, data model, threat
model, checklists, tasks, and one evidence file per batch — lives under `specs/`, which is
**gitignored**; `git ls-files specs` returning **0** is a required closing state, and this file
is the mirror.

---

## Security-relevant

| # | Limitation | Why not closed | Owner / target |
|---|---|---|---|
| **L-1** | **No real TLS handshake is exercised** against Valkey, an SMTP relay, or an OTLP collector. The adapters configure rustls with native roots and the `ring` provider (source-verified, and the single-provider state is asserted by step 7), and the plaintext-loopback paths run against real servers. | No trusted certificate authority exists on this machine or a CI runner. | Maintainer; a CI leg with a self-signed CA in Phase 011 |
| **L-2** | **No S3-compatible object-storage adapter ships.** Every candidate failed a gate: `object_store` and `opendal` reach `webpki-root-certs` (CDLA) on wasm32 through `rustls-platform-verifier` and `deny.toml` evaluates every target; `object_store` also carries an unmaintained `humantime`; `rust-s3` carries `webpki-roots`, MPL-2.0 and two `quick-xml` advisories; `aws-sdk-s3` needs 1.94.1. | FR-062 made the adapter conditional on passing the gates unchanged. Three routes are recorded in ADR-0035 (restrict `deny.toml` targets by maintainer decision; `object_store`'s custom-connector route; a Renvor S3 client over `hyper-rustls` under a custom-infrastructure record). None is taken unilaterally. | Maintainer decision; Phase 011 |
| **L-3** | **The transport-level cross-site guard is a function, not a middleware.** `cross_site_refused` is called at the one cookie-authenticated unsafe gate (`POST /auth/logout`); a future route of that shape must call it. | Chosen so the CSRF gate and the transport gate are read together; a middleware would apply silently and could be omitted just as silently. | `renvor-auth-http`; revisit when a second such route exists |
| **L-4** | **No browser is driven.** Fetch-metadata values are constructed as a user agent would send them. | Out of scope for a framework gate. | — |
| **L-5** | **Intermediate symbolic links inside a storage root** (a linked directory, not a linked object) are resolved by the `cap-std` capability within the sandbox and not refused. A link **at** an object path is refused. | The capability cannot escape the root either way; refusing intermediate links would require walking every segment with `symlink_metadata`. Stated in the module documentation. | `renvor-storage`; Phase 011 if a threat model needs it |

## Correctness and operations

| # | Limitation | Why not closed | Owner / target |
|---|---|---|---|
| **L-6** | **Two worker processes on one queue are not exercised.** The claim race is four tasks in one process against one server; the store boundary is the same, but two OS processes are not. | A local suite cannot honestly arrange it; the CI matrix can. | Phase 011 CI |
| **L-7** | **The job migration set is copied, not composed.** An application with both `renvor-auth` and `renvor-jobs` migrations copies both into its one directory; a second `Migrations::load` at another directory is refused by SQLx's ledger check. | A composing API is a public API of two adapters and is not in the spec. Documented in the crate README. | Phase 011 |
| **L-8** | **`TimedOut` on a real filesystem stall is not driven.** The bound's floor and a fast operation inside it are asserted; the provider's boot bound covers a stalled probe. | No slow filesystem is arranged. | `renvor-storage` |
| **L-9** | **The OTLP `force_flush` from the SDK is a documented no-op.** The bounded flush is the handle's async `shutdown`. | The SDK's `SpanProcessor::force_flush` is synchronous and cannot wait on a Tokio task without blocking. | `renvor-observability`; stated in the contract |
| **L-10** | **The messaging semantic-convention names are pinned by literal, not asserted against the conventions crate.** `messaging.system`, `messaging.destination.name`, `messaging.operation.type` are `semconv_experimental` in `opentelemetry-semantic-conventions` 0.32.1. | An experimental feature is not a convention; the HTTP, URL and database names are asserted against the crate. | Re-check when the crate stabilises them |
| **L-11** | **The inbound-invalid trace-context counter needs a `Registry` published in the application's state.** Without one the invalid context is still ignored and reported as an event, but not counted. | Chosen over a new field on the server configuration, which would have been a public-API change. | `renvor-http`; documented in the contract |
| **L-12** | **The C-M15 mutant (a non-yielding worker loop) is killed by the harness wall clock, not by a test assertion.** | A timeout is a future that needs polling; a non-yielding mutant starves it. Recorded honestly in the ledger. | `renvor-jobs` tests |
| **L-13** | **The L-11 event test missed once on its first run** and passed on every run since (the whole abuse module, the full auth suite, and both final gates). | Not reproduced; recorded rather than explained away. | Watch in Phase 011 |
| **L-14** | **This phase would close under a waiver.** No independent human review occurred; the proposed phase-closure waiver (`phase-010-proposed-waivers.md`) would be the tenth consecutive phase-level waiver of the same rule, and is not granted by this session. The three research agents delivered; the Codex review's disposition is in the evidence. | A staffing fact, not a process defect; the obligation to recruit a reviewer stands unchanged. | Maintainer |

## Closed from Phase 009, with the measurement

| Phase 009 row | Closed by | Proof |
|---|---|---|
| **009/L-4** — no `Origin` check anywhere | `renvor-auth-http` refuses a cookie-authenticated unsafe request the user agent marks `cross-site`, or whose `Origin` names another host than the validated one (or is opaque); absent headers do not refuse | the PostgreSQL-backed flow test sends the logout with a **valid** CSRF token four ways (cross-site 403, foreign origin 403, `null` 403, matching origin 200) and proves the session survives the refusals; two mutations killed by that test |
| **009/L-11** — a storage failure leaves no audit record | `AbuseGuard::admit` emits one structured event on `renvor.auth` with the correlation identifier, the flow, and the closed `DatabaseErrorKind` name before returning `ServiceError::Storage`; the audit vocabulary is unchanged | a recording-subscriber test asserts the event's fields and that no account address is in it; the mutation removing the event is killed by it |
