---
description: "Contract C-14 — OpenAPI 3.2.0 generation, determinism, and the compatibility gate"
version: "1.0.0"
status: "normative — public contract from the first release that ships it; nothing has been published yet. This version identifies the contract text, not a stability promise; the surface it describes is explicitly unstable under C-S1"
---

# Contract C-14 — API description and compatibility

**Status**: defined before implementation, per constitution principle V.
**Applies to**: `renvor-openapi`, `renvor-http`'s `route::describe`, and `renvor openapi`.

## The version is exactly 3.2.0, and it is not settable

Constitution principle V:

> *"The initial target is OpenAPI 3.2.0, and emitted documents MUST NOT claim a version that
> selected tooling does not correctly implement."*

`OPENAPI_VERSION` is a **constant**. There is no field, argument, or configuration that sets the
`openapi` member, so relabelling is not something the model can express.

## The proof that it is genuinely 3.2, not relabelled 3.1

Declaring `3.2.0` proves nothing on its own: OpenAPI 3.2 is largely backwards compatible, so a 3.1
document with its version string edited validates against the 3.2 schema perfectly well. **A gate
that checked only the version string would pass for exactly the relabelling principle V forbids.**

Five proofs run against the **vendored official schemas**, offline:

| # | Proof |
|---|---|
| 1 | the document declares `3.2.0` |
| 2 | it validates against the official OpenAPI **3.2** schema, using a validator independent of the generator |
| 3 | it is rejected by the official **3.1** schema **with 3.1's `openapi` version pattern neutralised** — so the rejection is **structural**, not a version-string mismatch |
| 4 | **the control**: a genuinely relabelled 3.1 document *passes* that same neutralised check, proving the discriminator discriminates rather than rejecting everything |
| 5 | **the controls**: four malformed documents are rejected by the 3.2 schema, proving the validator is not vacuous |

Proof 3 works because both official schemas use `unevaluatedProperties: false` throughout, so a
document carrying a 3.2-only member is structurally invalid against 3.1.

**Generated documents therefore always carry 3.2-only constructs**, and this is a requirement rather
than decoration: `$self`, Response `summary`, Tag `summary` and `kind`, and Response objects with no
`description` — which 3.1 **requires** and 3.2 does not.

**The schemas are vendored, unmodified.** A gate that needs the network fails when the network does,
and [`verification-sequence.md`](verification-sequence.md) requires a check that cannot run to be a
failure rather than a skip. They are upstream artifacts; a local edit would make the gate judge
Renvor's opinion of the standard rather than the standard.

## One registry, three consumers

```
                    RouteRegistry  (one value)
           ╱               │              ╲
   build::router    inspect::render   describe::document
     dispatch         route table       OpenAPI 3.2.0
```

`describe::document` takes `&RouteRegistry`, exactly as the other two do. There is no other source
it could read.

| Rule | Behaviour |
|---|---|
| **Completeness** | Every registered route appears **exactly once**, with correct method, path, and operation identifier |
| **No phantoms** | No operation appears that no route declares |
| **Undeclared routes still appear** | A route registered without declarations is described as an operation with no declared inputs. It is **not** omitted — a missing route is the failure this rule exists to prevent, and "the author declared no inputs" is not a reason to describe a different API from the one being served |
| **Agreement** | An input's published schema **is** the value the runtime enforces. Not a copy |
| **No source parsing** | Schemas are never inferred by reading source, and never read from a second manifest |
| **Operation identifiers** | Unique within a document. A collision is a **reported error**, never a silently kept winner — whether the identifiers were supplied or derived |
| **Error responses** | Every public error code an operation can produce appears as a declared response referencing **one shared** Problem Details component. A repeated inline schema is a second copy that can drift |
| **Examples** | Validated against their own schemas at generation time. A contradicting example is a **reported error**, not a warning — examples are copied |

## Determinism

Two generations of the same registry produce **byte-identical** output, in any process, on any
platform.

- Members are emitted in a **fixed declared order**; open-ended maps are emitted **sorted**.
- No value derived from a clock, a random source, a hash seed, an environment variable, a filesystem
  traversal order, or an address is emitted.

`serde_json`'s `preserve_order` is deliberately **not** enabled: it would make output depend on
insertion order, which is the non-determinism this avoids.

## Generation has no side effects

**0** listeners bound, connections opened, migrations run, network calls made, or providers started.
Asserted observably: the generation test runs with **no async runtime at all**, and every one of
those operations requires a reactor.

## The compatibility gate

A committed snapshot is compared against the generated document, **semantically**.

**Breaking — the gate fails:**

1. a route is removed, or its method or path changes;
2. an operation identifier changes;
3. a parameter is removed, or becomes required;
4. a new **required** input appears;
5. a declared type changes;
6. a constraint **narrows**;
7. a declared response status is removed;
8. a public error code is removed;
9. a **guaranteed** response member is removed;
10. a declared content type is removed.

**Not breaking — the gate passes:**

1. a description, summary, or example changes;
2. a new **optional** input appears;
3. a constraint **widens**;
4. a new response status, error code, operation, or response member appears.

**The asymmetry is the whole classification.** Requests are contravariant and responses are
covariant: narrowing rejects input a consumer previously sent successfully, while widening accepts
input it never sent. That is why "an added response member is safe" and "an added required request
member is not" sit on opposite sides of one rule.

**Both directions are required.** A gate that failed on every change would be an obstacle, and an
obstacle gets routed around — so the harmless mutations are as load-bearing as the breaking ones.

### Where the snapshot lives, who owns it, and how it is updated

**File**: `crates/renvor-openapi/tests/snapshots/public-description.json`, read with `include_str!`
so the baseline side resolves at compile time against the committed file.

**Owner**: the maintainer. It is a reviewed artifact — it changes only in a pull request, alongside
the change that moved the description.

**How it is updated**: run

```
cargo test -p renvor-openapi --test compatibility refresh_the_committed_snapshot -- --ignored --nocapture
```

and copy the printed document into the file. The generator is `#[ignore]`d so it never runs in the
gate, and it **only prints** — it cannot write the file it would be approving. That is deliberate:
a refresh step that wrote its own baseline would approve every break it introduced.

Two tests guard it. One compares **semantically** and fails on a breaking difference. The other
compares **byte for byte** and fails on any difference at all, so the committed file cannot go
stale while the semantic gate keeps passing.

> **Added 2026-08-23.** This section did not exist, and FR-043 requires the snapshot's location,
> owner, and update procedure to be stated. Until it was added there was **no committed snapshot at
> all**: the compatibility tests built both sides in memory, while the paragraph below already
> asserted that one side came from committed history. The paragraph was true of the design and
> false of the code. It is now true of both. Found by maintainer self-review during the Phase 005
> closing audit.

### The snapshot cannot approve its own diff

The comparison reads one side from **committed history**. Regenerating both sides would make every
comparison a document against itself, which always passes — including for a change that removes a
route. The bypass is attempted in the test suite and required to fail.

### Declaring an intended break

Introduce the change under a **new path version prefix** while the previous version's operations
remain. The gate then sees additions rather than modifications and passes — because from a
consumer's point of view nothing was taken away. A break **not** accompanied by a retained previous
version fails regardless of intent, so the escape hatch cannot wave one through.

## `renvor openapi`

The same protocol shape as `renvor routes`, with one addition.

| Property | Rule |
|---|---|
| **One source** | The description is rendered from the registry that builds the router |
| **Versioned** | The payload declares `result.protocol`, checked **before** the payload is read. An unrecognised version is refused **by name** |
| **No binary discovery** | The project's own declared default binary, through its build tool. Nothing searches build output |
| **No boot side effects** | The application answers and exits **before** it starts anything |
| **Bounded in size** | An over-sized answer is refused naming the bound |
| **Bounded in TIME** | **New in Phase 005.** The invocation has a deadline, and the child process is **killed** when it elapses |

The time bound closes a gap Phase 004 recorded against itself — `renvor routes` used a blocking
`Command::output()`, and its own source named both the hazard ("a project binary that … boots
normally and streams logs forever") and the fix. It is implemented **once** and both relays use it,
because two implementations of "run the project binary safely" would be two things that can drift.

### Failures are named, and none is an empty success

Unsupported protocol, malformed answer, over-sized answer, timeout, non-zero exit, no framework
dependency — each is a **distinct named failure** with a non-zero exit. An empty description and
"this project cannot be asked" are different facts, and a consumer cannot tell them apart if both
arrive as success.

> **Current, dated limitation — 2026-08-23.** No Renvor crate is published, so **no project the
> current generator produces depends on the framework**, and `renvor openapi` therefore succeeds
> against **none** of them. The relay **is** implemented and **is** asserted end to end against a
> real binary answering through the real library — its reach across *generated* projects is what is
> zero, and it is zero because nothing is published for them to depend on. The same limitation
> `renvor routes` carries, in the same words, because it is the same limitation.

## Renvor owns the serialiser, and that is bounded

On 2026-08-23 every maintained Rust generator emitted an earlier version — `utoipa` 5.5.0 emits
`"3.1.0"`, measured by compiling and running it. See
[`ADR-0013`](../decisions/0013-openapi-3-2-document-serialiser.md) for the full matrix, the
ownership cost, and the deletion trigger.

| | |
|---|---|
| **It does** | emit the document envelope and its operations, deterministically |
| **It does not** | implement JSON Schema — `schemars` does |
| | parse documents written by anyone else |
| | resolve remote references |
| | judge validity — the **official** schema does that |
