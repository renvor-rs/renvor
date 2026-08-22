---
description: "Contract C-10 — HTTP runtime lifecycle, admission, bounded drain, limits, and timeouts"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-10 — HTTP runtime, admission, and drain

**Status**: defined before implementation, per constitution principle V.
**Applies to**: `renvor-http`, and the `renvor` facade under the `transport-rest` feature.

## The server is a provider

The HTTP server participates in the kernel lifecycle defined by
[`lifecycle-contract.md`](lifecycle-contract.md). It introduces no phase and reorders none.

| Phase | What the server does |
|---|---|
| `Load` · `Validate` · `Register` | configuration is read and validated; **nothing is bound**. An invalid configuration fails here, before a socket exists |
| `Boot` | the listener is bound. A bind failure rolls back exactly as any provider failure does |
| `Ready` | requests are served. Readiness reports ready |
| `Drain` | new work refused; in-flight work bounded; outcome reported |
| `Stop` | providers stop in **reverse actual initialisation order** (C-L3) |

Because C-L1 fixes `Drain` before `Stop`, **the server drains before providers stop.** That ordering
is inherited, not redefined here.

## Admission

Every request takes a **work permit** from the kernel's work gate before reaching a handler, and
holds it for the request's lifetime.

- A permit releases **on drop** — including on an early return, a rejection, a timeout, or a panic.
  There is one release path, so there is no path that forgets.
- Once the gate is closed, a request is refused with **503** and **is not routed to a handler**.
- Refusal is an **error**, never a silent drop and never a silent acceptance (FR-006, C-L6).

## Drain is bounded, and the bound is the point

```
shutdown requested
   │
   ├─ work gate closes ............ new requests refused (503)
   ├─ listener stops accepting .... no new connections
   └─ drain(budget) ............... BOUNDED wait
         ├─ finished in budget ......... Clean
         └─ budget elapsed ............. Incomplete { outstanding: N }
```

| Property | Value |
|---|---|
| Default budget | **30 seconds** — the kernel's `DEFAULT_DRAIN_BUDGET`, **reused, not restated** |
| Overridable | yes |
| Zero | a valid budget meaning *stop immediately*; **not** invalid, **not** "wait forever" |
| Zero with work in flight | reports that work as outstanding, on the **same code path** a timed-out drain uses |

**An incomplete drain is never reported as clean** (FR-007). There is no third outcome for "finished
but unsure", because an ambiguous answer given nowhere to live cannot be given.

> **Why Renvor bounds this itself.** The underlying server library's graceful-shutdown helper stops
> accepting and then waits for connection tasks **with no timeout at all**. Contract C-L7 states
> that *"an unbounded wait in a kernel-owned path is a defect, not a configuration choice."* Renvor
> therefore uses that helper for the stop-accepting half only, and owns the bound. See
> [`ADR-0012`](../decisions/0012-phase-004-custom-http-primitives.md).

## Readiness and liveness disagree during drain

Entering `Drain` makes readiness report **not ready** while liveness continues to report **alive**
(C-O8). Conflating them kills a draining process or keeps sending it work; both are outages caused
by the primitive rather than by the application.

## Cancellation reaches the application

A per-request cancellation scope is a **child** of the application scope.

- Application shutdown cancels every in-flight request.
- One request's cancellation cancels no other.
- **Client disconnect and request timeout both cancel that scope**, so an application service sees
  one mechanism rather than two.
- The value an application service receives carries **no HTTP type**. Cancellation crosses the
  boundary as the kernel's own scope.

## Limits and timeouts

Every value here is public, documented, and asserted **at its exact boundary and at boundary + 1**.

| Bound | Value | At the boundary | Beyond it |
|---|---|---|---|
| Request body | **2 MiB** (2 097 152 bytes) | exactly 2 MiB is **accepted** | **413** |
| Concurrent requests | **1024** | the 1024th is admitted | the 1025th is refused |
| Request timeout | **30 seconds** | — | **408** |
| Route ceiling | **4096** | the 4096th registers | registration error |
| Drain budget | **30 seconds** | — | `Incomplete { outstanding }` |

The body boundary is **inclusive**: a body of exactly the limit passes. This is stated because the
opposite is the more common assumption, and a contract that leaves it to assumption will be
implemented both ways.

### What Renvor does NOT bound — named, not implied

**Renvor sets no header-count or header-size limit.** The underlying HTTP implementation applies its
own; Renvor exposes no control over them through the server entry point it uses, and therefore makes
**no promise** about their values.

This row exists because omitting it would let a reader infer that every bound on this page is
Renvor's. Constitution principle XII requires the limitation to be visible, and a bound that is not
controlled must not be claimed.

## Errors and telemetry

- A rejection carries a **status** and a **stable code**. It carries **no** internal filesystem path,
  **no** source-error chain, **no** configuration value, and **no** panic payload (FR-041).
- Emitted records use **structured fields only** (C-O2), carry the **run identifier** (C-O3) with the
  request identifier nested beneath it, and are subject to redaction **including span fields**
  (C-O6).
- A handler panic is **caught, contained, and reported as a failure**. It is never a hang, and its
  payload never reaches a response.

## Feature isolation

This runtime exists **only** under the `transport-rest` feature, which is **off by default**. The
kernel crate resolves no HTTP server, routing, or middleware dependency under **any** feature
combination. Both directions are asserted, the second as a positive control.
