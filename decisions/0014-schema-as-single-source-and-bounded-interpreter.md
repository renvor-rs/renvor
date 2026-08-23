# ADR-0014: Make the schema the single source, and interpret a bounded subset of it at runtime

| Field | Value |
|---|---|
| **ID** | 0014 |
| **State** | `accepted` |
| **Reviewer** | Ahmed Anbar — self-review under W-011 |
| **Review date** | 2026-08-23 |
| **Superseded by** | *(not superseded)* |

> **This record is `accepted` under W-011. The review behind it was NOT independent.**
>
> Constitution §Development and Phase Workflow #4 and spec FR-013 require a recorded **independent**
> review before acceptance. **No independent human review of this record has occurred, and none is
> claimed.** Acceptance rests on **[W-011](../governance/waivers.md)**, a time-bounded written
> waiver granted on 2026-08-23, owned by Ahmed Anbar, expiring **2027-02-11** or **immediately**
> when a qualified independent human reviewer becomes available — whichever is first.
>
> **Automated review is advisory and does not satisfy the requirement.** W-011 covers this record,
> ADR-0014 and ADR-0015 as one coupled Phase 005 decision; it authorises **nothing else** — not
> phase closure, which is W-012, and not any publication, tag, release or deployment. When its
> removal plan is executed, this record is re-reviewed **in full** and W-011 closes.

## Context

PLAN.md §Phase 005's acceptance criterion is:

> *"runtime validation and published schemas agree"*

There are two ways to satisfy a sentence like that. One is to declare the rules twice and test that
the copies match. The other is to make them **the same value**, so agreement is not a property that
can fail.

Phase 004 already chose the second shape for routes.
[`contracts/http-routing.md`](../contracts/http-routing.md) states why:

> *"Both functions take `&RouteRegistry`. Neither has access to any other source, so a route cannot
> reach dispatch without reaching inspection… Making them agree is not a discipline anyone has to
> keep — there is only one value."*

## The package research

| Candidate | Version | Validates? | Emits JSON Schema? |
|---|---|---|---|
| `validator` | 0.21.0 (2026-07-27, MIT) | yes | **no** |
| `garde` | 0.23.0 (2026-05-23, MIT OR Apache-2.0) | yes | **no** |
| `schemars` | 1.2.2 (2026-07-27, MIT) | **no** | yes — draft 2020-12 |
| `jsonschema` | 0.50.1 (2026-08-22, MIT) | yes, against a schema | n/a |

**No maintained crate does both.** Pairing a validator with `schemars` means declaring every
constraint **twice** — once for the validator, once for the schema generator — which is the
two-copies design the acceptance criterion exists to rule out.

The obvious unification is to validate **against the schema itself** with `jsonschema`. That was
measured rather than assumed:

```
renvor-http's runtime dependency graph .................  65 packages
jsonschema 0.50.1, default-features = false ............ 103 packages
```

Adding it would **more than double** the transport's dependency surface — pulling the entire ICU
stack (`icu_normalizer`, `icu_properties`, `icu_collections`, `icu_provider`), `fancy-regex`,
`num-bigint`, `uuid-simd`, `email_address`, and `wasm-bindgen` — to check a request body.

Constitution principle XII requires *"the smallest design satisfying the contract"*, and PLAN.md §22
names *"optional features increase compile/dependency cost"* as a tracked program risk.

## Decision

**One `Declaration` holds one schema value. `validate()` interprets it; `schema()` publishes it.**

`schemars` produces the schema from a Rust type, including the constraints declared with its own
derive attributes. Renvor interprets a **bounded, declared subset** of that schema at runtime.

Net new runtime packages, measured against the existing lockfile: **seven** — `base64`,
`dyn-clone`, `ref-cast`, `ref-cast-impl`, `schemars`, `schemars_derive`, `serde_derive_internals`.
Against 103. Six of the seven are `schemars`' own subtree; `base64` is a separate selection, for
cursor encoding.

> **Corrected 2026-08-23.** This list previously named `zmij` and omitted `base64`. `zmij` is a
> pre-existing transitive of `serde_json` — same version and checksum in the pre-phase lockfile —
> so it is not new. The count of seven is unchanged and the comparison against 103 is unaffected.

### The enforced subset

`type`, `required`, `properties`, `additionalProperties`, `items`, `minItems`, `maxItems`,
`uniqueItems`, `minLength`, `maxLength`, `minimum`, `maximum`, `exclusiveMinimum`,
`exclusiveMaximum`, `multipleOf`, `enum`, `const`, and local `$ref`.

**A declaration using any other keyword is REFUSED at declaration time**, naming the keyword.

> That refusal is the whole difference between a bounded subset and a partial implementation. A
> partial implementation ignores what it does not understand, publishes the constraint anyway, and
> enforces nothing — so the description becomes false at exactly the point an author was relying on
> it. Refusing at declaration time means an unenforceable constraint never ships.

`format` is carried into the description and **not enforced**, because JSON Schema 2020-12 places it
in the format-**annotation** vocabulary by default. That is the standard's answer, not a shortcut,
and it is stated in the contract because the opposite is the more common assumption.

## Consequences

### Agreement with the standard is asserted, not assumed

`jsonschema` is a **dev-dependency**, where its weight is free, and
`crates/renvor-validation/tests/differential.rs` asserts that Renvor's verdict **equals** the
reference implementation's over a corpus covering every enforced keyword — including the cases where
a naive implementation is wrong:

- `minLength`/`maxLength` count **code points**, not bytes;
- `1.0` **is** an integer;
- `0.3` **is** a multiple of `0.1`, despite binary floating point making the quotient
  `2.9999999999999996`;
- `uniqueItems` compares **structurally**, so `[{"a":1},{"a":1}]` is a repeat.

A test also fails if a keyword is added to the enforced set without a differential case, so the
corpus cannot fall behind the claim.

### An issue can never carry the value

The reason vocabulary is a closed enum rendering to `&'static str`, which is what makes it
impossible to substitute a validator's message. Measured during this phase's tooling work, a real
JSON Schema implementation produced `"not an object" is not of type "object"` — the rejected value
is inside the message text. Any design that rendered a validator's `Display` would have leaked it.

### The ownership cost, stated

The interpreter is roughly 400 lines including documentation. It must stay correct against JSON
Schema 2020-12 for the keywords it claims, and the differential corpus is what holds it there.

### The exit strategy

**Deletion trigger.** If a maintained crate appears that both validates **and** emits JSON Schema
from one declaration, or if `jsonschema`'s dependency graph falls to a size comparable with the
six packages `schemars` adds, Renvor's interpreter is replaced and this record is superseded.

The differential test is the acceptance harness for any replacement, so the comparison costs a
dependency swap and a test run.

**Expanding the subset is not a supersession.** `pattern` and the composition keywords are recorded
as Phase 012 work in [`contracts/validation.md`](../contracts/validation.md); adding one extends
this decision rather than replacing it.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| `validator` or `garde` plus `schemars` | Two declarations of one constraint. The acceptance criterion becomes a test rather than an identity, and the two can drift in a way no type system catches |
| `jsonschema` at runtime | **+103 packages**, measured. More than doubles the transport's dependency surface to check a request body |
| A Renvor derive macro emitting both | Needs `renvor-macros`, which nothing else in this phase requires. Principle XII: a new crate must solve a demonstrated requirement, and `schemars_derive` already emits the schema |
| Renvor's own constraint vocabulary, rendered to JSON Schema | Rejects `schemars` and rebuilds structural schema generation from Rust types — far more code than an interpreter, and with no reference implementation to test against |
| Enforce nothing and publish only | The description would promise constraints the runtime does not apply, which is the failure this phase exists to prevent, inverted |
