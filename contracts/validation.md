---
description: "Contract C-12 — the validation boundary, its declared subset, and what an issue may say"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-12 — Validation

**Status**: defined before implementation, per constitution principle V.
**Applies to**: `renvor-validation`, and `renvor-http` where it adapts the boundary to HTTP.

## One declaration, and there is nowhere for a second to live

A constraint is declared **once**, as a schema. Exactly two operations consume it:

```
                        Declaration
                      (one schema value)
                     ╱                  ╲
              validate()              schema()
        runtime enforcement     published description
```

**A second validation-rule registry is prohibited.** This is structural rather than advisory: both
operations read the same field of the same value, so there is no other source either could consult.
A constraint cannot be enforced without being published, and cannot be published without being
enforced.

This is the argument [`http-routing.md`](http-routing.md) already makes for routes, applied to input
rules. It is what makes *"runtime validation and published schemas agree"* an **identity** rather
than a property tests have to keep chasing.

## Where input is validated

| Location | Validated | Note |
|---|---|---|
| Path | **yes** | |
| Query | **yes** | |
| Header | **yes** | Matched case-insensitively, as HTTP requires |
| Body | **yes** | `application/json` only |
| Cookie | **no** | A cookie is an authentication and session carrier, and **Phase 009** owns authentication. Declaring cookie validation in a phase with no opinion about a cookie's *meaning* would invite an author to validate a session cookie's shape without validating its authority |

## The enforced subset is bounded, and a declaration outside it is REFUSED

Renvor enforces these keywords, and no others:

```
type          required      properties     additionalProperties
items         minItems      maxItems       uniqueItems
minLength     maxLength     minimum        maximum
exclusiveMinimum            exclusiveMaximum
multipleOf    enum          const          $ref (local, #/$defs/… only)
```

These are carried into the description **without** being enforced, because they are annotations
rather than assertions: `$schema`, `$id`, `$comment`, `$defs`, `title`, `description`, `examples`,
`example`, `default`, `deprecated`, `readOnly`, `format`.

**`format` is an annotation, and that is the standard's answer rather than a shortcut.** JSON Schema
2020-12 places `format` in the format-**annotation** vocabulary by default. Renvor publishes it and
does not enforce it. Stated here because the opposite is the more common assumption.

**A declaration using any other keyword is refused at declaration time**, naming the keyword.

> That refusal is the whole difference between a bounded subset and a partial implementation. A
> partial implementation ignores what it does not understand, publishes the constraint anyway, and
> enforces nothing — so the description becomes false at exactly the point an author was relying on
> it. Refusing at declaration time means an unenforceable constraint never ships.

### Excluded, with a target phase

| Excluded | Reason | Target |
|---|---|---|
| `pattern` | Requires a regex engine on untrusted input. `regex` is **not** in the current runtime graph, so it is a new dependency for one keyword | Phase 012 |
| `allOf` / `anyOf` / `oneOf` / `not` / `if` | The composition keywords. `schemars` emits them for data-carrying enums; those declarations are refused rather than mis-validated | Phase 012 |

## What an issue may say

An issue carries **exactly three things**, and there is no field a value could occupy:

| Field | Rule |
|---|---|
| **location** | one of `path`, `query`, `header`, `body` |
| **pointer** | RFC 6901 JSON Pointer for a body; the declared parameter name otherwise |
| **reason** | a name from a **closed** vocabulary — never a message |

**A rejected value MUST NOT appear in an issue, in any field, in any encoding.**

**A reason is never a library's message.** Measured during this phase's tooling work, a real JSON
Schema implementation produced `"not an object" is not of type "object"` — the rejected value is
*inside the message text*. Renvor maps a validator's structured kind to its own reason enum and
renders nobody's `Display`, including its own.

**An undeclared member's name is not echoed.** For a member the schema declares, the name is the
schema's word and naming it is safe. For an **un**declared member the name is attacker-chosen, so
the issue points at the containing object and says `unknown_member`.

## Every violation, not the first

Validation reports **every** violation it can determine, so a caller can correct an input in one
round trip. Ordering is deterministic: a depth-first walk visiting object members in **sorted** key
order — not the order the request happened to send them in, which is caller-controlled.

## Structural unreadability is a separate outcome

A body that is not readable as the declared media type produces a **distinct** outcome, not a list
of constraint violations. A document that never parsed has no fields to point at, and telling a
caller to correct a field in it would be nonsense.

## Agreement with the standard is asserted, not assumed

Renvor interprets the subset itself rather than resolving a JSON Schema validator into the
transport's dependency graph. Measured on 2026-08-23:

```
renvor-http's runtime graph ....................  65 packages
jsonschema 0.50.1, default-features = false .... 103 packages
```

The reference implementation is a **dev-dependency**, and `tests/differential.rs` asserts that
Renvor's verdict equals it for every case in a corpus covering every enforced keyword. A bounded
subset is only honest while it agrees with the standard it publishes; that test is what makes
"bounded" a boundary rather than an excuse.

## Feature isolation

The validation boundary exists **only** under the `transport-rest` feature at the facade, and
`renvor-validation` resolves **no** HTTP server, router, or middleware crate under any feature
combination. Both directions are asserted, the second as a positive control.
