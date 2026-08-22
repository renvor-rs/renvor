---
description: "Contract C-9 — route registration, dispatch outcomes, and route inspection"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-9 — Routing and route inspection

**Status**: defined before implementation, per constitution principle V.
**Applies to**: `renvor-http`, and the `renvor` facade under the `transport-rest` feature.

## One registry, and there is nowhere for a second to live

Routes are declared in a **single authoritative value**, the route registry. Exactly two operations
consume it:

```
                      RouteRegistry
                     (one value)
                     ╱          ╲
            build a Router      render an inspection
                    │                    │
              real dispatch      human table + JSON
```

**A second route manifest is prohibited.** This is structural rather than advisory: the inspection
function takes a reference to the registry, so there is no other source it could read. A route
cannot reach dispatch without reaching inspection, and cannot reach inspection without reaching
dispatch.

> A route list that can disagree with the router is worse than no route list, because it is trusted.
> That is why this is a contract clause and not an implementation preference.

## Registration

| Rule | Behaviour |
|---|---|
| **Duplicate** | A second registration of the same method **and** path is a **reported error**. It is never a silent overwrite — a silent winner would depend on registration order that nobody wrote down |
| **Ceiling** | **4096** routes. Registration beyond the ceiling is a reported error naming the bound and the limit |
| **Group prefix** | A group contributes a path prefix to every route it contains. Nested groups compose left to right |
| **Group middleware** | A group's middleware applies to every route it contains, and to no route outside it |
| **Path shape** | Every registered path is validated at registration. An invalid pattern is a registration error, not a route that never matches |
| **`OPTIONS`** | **Cannot be registered.** The CORS layer answers every `OPTIONS` request as a preflight, so an application route on `OPTIONS` would never run. Registration is a **reported error** rather than a silent shadow — the same rule as the row above, applied to a method instead of a path |

## Dispatch outcomes

| Situation | Status | Additional |
|---|---|---|
| A declared method and path | the handler's response | — |
| A path that is not declared | **404** | — |
| A declared path, undeclared method | **405** | **`Allow` header naming the declared methods for that path** |

The `Allow` header on `405` is part of this contract. A `405` without it tells a caller its method
was wrong but not which method would be right, which is the one thing the response exists to convey.

## Route inspection

### Output

Two forms, from one registry.

**Human** — a table carrying method, path, and group, in a stable order.

**Structured** — `stdout` carries **exactly one JSON document** in the C-2 envelope
([`json-output.md`](json-output.md)):

```json
{
  "schemaVersion": 2,
  "status": "success",
  "command": "routes",
  "result": {
    "protocol": 1,
    "routes": [
      { "method": "GET", "path": "/api/v1/health", "group": "api-v1" }
    ]
  }
}
```

`result.routes` is sorted by path then method, so two runs against the same registry produce
byte-identical output.

### `result.protocol` — versioned separately, and deliberately

`schemaVersion` describes the **envelope**: the `status`/`command`/`result` shape and the closed
error-code registry. `result.protocol` describes the **payload inside `result`**, which changes for
different reasons — adding a field to a route row is not a change to how failures are reported.

**Current value: `1`.**

A consumer checks `protocol` **before** reading the payload and refuses a version it does not
understand, by name. Parsing an unknown version on a best-effort basis would let a route table
silently lose a column, which is exactly the quietly-wrong output this capability exists not to
produce.

### Where the metadata comes from — stated truthfully

`renvor routes` **cannot dynamically load arbitrary compiled application code**, and this contract
does not pretend otherwise.

**The route-dump protocol.** A Renvor application binary answers a documented invocation by
printing the C-2 envelope for its own registry — the same registry that built its router. The
`renvor routes` command builds and runs the project binary with that invocation and relays the
result.

The authority is therefore **the application's own registry**, reached by asking the application.
There is no static source parsing, no separate manifest file, and no inference.

Four properties are normative:

| Property | Rule |
|---|---|
| **One source** | the payload is rendered from the registry that builds the router |
| **Versioned** | `result.protocol` is checked before the payload is read |
| **No binary discovery** | the project's own declared default binary is run, through its build tool. Nothing searches build output for something executable |
| **No boot side effects** | the application answers and exits **before** it starts anything. A protocol that required booting would make listing routes bind ports and run migrations |

### When the project has no transport wiring

A project that does not depend on the framework cannot answer the dump. The command **fails**, with
the registered error code, exit `3`, and details naming the reason.

**It does not print an empty route table and exit `0`.** An empty success is indistinguishable, to a
consumer, from an application with no routes — and the two mean entirely different things.

> **Current, dated limitation — 2026-08-22.** No Renvor crate is published, so **no project the
> current generator produces depends on the framework**, and `renvor routes` therefore succeeds
> against **none** of them. This is recorded rather than smoothed over: the relay **is**
> implemented and is asserted end to end against a real binary answering through the real library —
> its reach across *generated* projects is what is zero, and it is zero because nothing is
> published for them to depend on. See [`template-contract.md`](template-contract.md) for what the
> generator emits and why.

## Feature isolation

The routing surface exists **only** under the `transport-rest` feature. A build without it resolves
no HTTP server, routing, or middleware crate. Both directions are asserted — the absence, and a
positive control proving the same query finds the crates when the feature **is** enabled.
