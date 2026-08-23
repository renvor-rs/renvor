# Phase 005 — Evidence

**Phase**: 005 — Validation, Problem Details, and OpenAPI
**Date**: 2026-08-23
**Branch**: `feat/phase-005-validation-problem-openapi`
**Base**: `731d28dcd1b3796f799293996801e109e7681e9c` (live `main`)
**Status**: **open** — see §8. This phase has **no** independent human review and **no** waiver.

---

## 1. What this phase delivers

PLAN.md §Phase 005's acceptance criteria, and where each is met:

| Criterion | Where |
|---|---|
| *runtime validation and published schemas agree* | §3 — an **identity**, not a test result |
| *production errors never expose sensitive internals* | §4 |
| *every public route appears in OpenAPI* | §5 |
| *breaking API changes fail the compatibility gate unless explicitly versioned* | §6 |

Three crates were added: `renvor-error`, `renvor-validation`, `renvor-openapi`.

---

## 2. The OpenAPI 3.2.0 gate — the fail-closed proof

Constitution principle V forbids claiming a version the tooling does not implement, and PLAN.md
§11.1 says the phase *"remains blocked until selected tooling can emit and validate the promised
version correctly."*

**No maintained Rust crate emits a document with genuine 3.2 semantics.** `utoipa` 5.5.0 emits
`"3.1.0"` — established by compiling a `#[derive(OpenApi)]` type and printing the field.

**One published crate does emit the version string**: `salvo-oapi` 0.95.2, opt-in — and it
implements `$self` and nothing else from the 3.2 object model, is hard-bound to Salvo, and is
Apache-2.0 only. It is the clearest illustration of why the gate below has a **negative half**: it
produces a document that is schema-valid against OAS 3.2 while carrying no 3.2 semantics, which is
precisely the relabelling proof 3 detects.

> **Corrected 2026-08-23.** This paragraph previously read *"No maintained Rust crate emits
> 3.2.0"*, flatly. That was **false**. Found by package research after the claim was written; the
> correction narrows the claim and strengthens the case for the gate.

The full matrix is in [`phase-005-dependency-inventory.md`](phase-005-dependency-inventory.md) §3.

Renvor therefore serialises the document ([ADR-0013](../decisions/0013-openapi-3-2-document-serialiser.md)),
and the claim is proven rather than asserted:

| # | Proof | Result |
|---|---|---|
| 1 | The document declares `3.2.0`, and the version is a **constant** with no setter | **pass** |
| 2 | It validates against the **official** OAS 3.2 schema, via `jsonschema` 0.50.1 — independent of the generator | **pass** |
| 3 | The **same** document is rejected by the **official** 3.1 schema **with 3.1's version pattern neutralised** — so the rejection is structural, not a version-string mismatch | **pass** |
| 4 | **Control** — a genuinely relabelled 3.1 document *passes* that neutralised check, proving the discriminator discriminates rather than rejecting everything | **pass** |
| 5 | **Controls** — four malformed documents are rejected by the 3.2 schema, proving the validator is not vacuous | **pass** |

Proof 3 is the one that matters. OpenAPI 3.2 is largely backwards compatible, so a relabelled 3.1
document validates against the 3.2 schema perfectly well — a gate checking only proofs 1 and 2 would
pass for exactly the relabelling principle V forbids.

It works because **both** official schemas use `unevaluatedProperties: false`, so a document
carrying a 3.2-only member is structurally invalid against 3.1. Generated documents therefore always
carry 3.2-only constructs: `$self`, Response `summary`, Tag `summary`/`kind`, and Response objects
with **no** `description` — which 3.1 requires and 3.2 does not.

**Schemas are vendored, unmodified**, so the gate runs offline. Sources and dates in the dependency
inventory §9.

**Suite**: `crates/renvor-openapi/tests/openapi_3_2_gate.rs` — 12 tests.

### Proven end to end, not only in unit tests

A throwaway project was built that **genuinely depends on the framework** by path, declares two
operations with real constraints, and answers the metadata protocol. The **real `renvor openapi`
binary** was then run against it:

```
$ renvor openapi --output json
envelope status  : success
envelope command : openapi
schemaVersion    : 2
openapi          : 3.2.0
info.title       : Orders 2.1.0
paths            : ['/orders']
operationIds     : ['listOrders', 'createOrder']
error codes      : 12
```

That relayed document — the one a real operator would receive, having travelled through the CLI,
`cargo run`, the protocol envelope, and back — was validated against the vendored official schemas:

```
vs official 3.2: VALID
vs official 3.1: REJECTED  (version pattern neutralised)
    first error: "description" is a required property @ /paths/~1orders/get/responses/200
```

and carries four distinct 3.2-only constructs: `response.summary`, `tag.kind`, `tag.summary`, and a
`200` response with **no** `description` — required in 3.1, optional in 3.2.

This is the whole chain end to end: **a real project, the real command, the real protocol, judged by
the official schema.** The tests in §2 assert the same properties in isolation; this shows they hold
when everything is connected.

---

## 3. Runtime validation and the published schema are one value

Not two documents that agree. **The same `serde_json::Value`**, reached from both directions:

```
                    RouteRegistry  (one value)
           ╱               │              ╲
   build::router    inspect::render   describe::document
     dispatch         route table       OpenAPI 3.2.0
           │                                  │
           └──────── the SAME schema ─────────┘
```

`describe::document` takes `&RouteRegistry`, exactly as the other two do — there is no other source
it could read. Phase 005 added a **third consumer** without adding a second collection.

**Asserted** in `crates/renvor-http/tests/describe.rs`: the published schema for a parameter is
compared by equality against the registry's own copy, and then the same declaration is exercised at
runtime to prove the equality is between two *used* values rather than two unused ones.

### The interpreter agrees with the standard

Renvor interprets a bounded JSON Schema subset rather than resolving a validator into the transport
— measured at **103 packages against 65**
([ADR-0014](../decisions/0014-schema-as-single-source-and-bounded-interpreter.md)).

`crates/renvor-validation/tests/differential.rs` asserts that Renvor's verdict **equals**
`jsonschema` 0.50.1's over a corpus covering **every** enforced keyword, including the cases a naive
implementation gets wrong: code-point string lengths, `1.0` as an integer, `0.3` as a multiple of
`0.1` despite binary floating point, and structural `uniqueItems`. A further test fails if a keyword
is added to the enforced set without a differential case.

**A declaration using an unsupported keyword is refused at declaration time**, so an unenforceable
constraint never ships.

---

## 4. Production errors expose nothing

Three guarantees hold **by construction**, not by review:

| Guarantee | Mechanism |
|---|---|
| `detail` cannot carry runtime data | it is `&'static str` |
| An invalid parameter cannot carry the rejected value | the type has **no field** one could occupy |
| A reason cannot be a validator's message | it is `&'static str` |

The third exists because a real validator produced `"not an object" is not of type "object"` during
this phase's tooling work — the rejected value is inside the message text.

**Suite**: `crates/renvor-http/tests/problem_details.rs` — 13 tests through a **real router**, every
negative search paired with a positive control proving the probe discriminates. Canaries are sent as
values, as attacker-chosen member names, and inside panic payloads. A separate test sweeps every
failure path for `src/`, `.rs:`, `panic`, `unwrap`, `backtrace`, `thread '`, SQL keywords, and
filesystem prefixes.

**Correlation**: the document's `correlationId`, the `x-request-id` header, and the telemetry field
are rendered from **one** `RequestId`. Asserted by equality across all three.

**A real gap was found by these tests and fixed**: the router's 404 fallback still answered in plain
text. It now emits a Problem Details document carrying a correlation identifier, as every other
refusal does.

---

## 5. Route completeness, both directions

`crates/renvor-http/tests/describe.rs` compares the registry's `(method, path)` set against the
description's, by equality. Both directions in one assertion: no route missing, no operation that no
route declares.

A route registered **without** declarations still appears, described as an operation with no
declared inputs — a missing route is the failure the rule exists to prevent, and "the author
declared no inputs" is not a reason to describe a different API from the one being served.

**Determinism**: two generations produce byte-identical output; open-ended maps are emitted sorted.

**No side effects**: generation runs with **no async runtime at all**, and a positive control proves
the absence of a runtime is detectable in that test. Binding a listener, opening a connection, or
starting a provider all require a reactor.

---

## 6. The compatibility gate

`crates/renvor-openapi/tests/compatibility.rs` — 27 tests.

| | Count | Result |
|---|---|---|
| Breaking mutations that **must** fail the gate | 16 | all detected, each with its specific classification |
| Harmless mutations that **must not** fail it | 7 | **zero** false positives |
| Gate-integrity tests | 4 | pass |

Both directions are required. A gate that failed on every change would be an obstacle, and an
obstacle gets routed around.

**FR-048** — that a regenerated snapshot cannot approve its own breaking diff — is asserted by
**attempting the bypass**: a self-comparison is shown to be silent, then the same broken document is
compared against the committed baseline and the break is caught.

---

## 7. Verification

| Gate | Result |
|---|---|
| Workspace tests, `--all-features` | **1038 passed, 0 failed, 1 ignored** |
| `cargo fmt --all --check` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `renvor-error` | 21 passed |
| `renvor-validation` | 47 passed |
| `renvor-openapi` | 39 passed |
| `renvor-http` | 213 passed |
| `renvor-cli` | 344 passed |
| `renvor-core` | 208 passed |
| `renvor-config` | 108 passed |
| `renvor-testkit` | 13 passed |
| `renvor` | 12 passed |
| `xtask` | 24 passed |

The full `cargo xtask verify` sequence on both toolchains is recorded in the pull request.

### The Phase 004 gap this phase closed

`renvor routes` used a blocking `Command::output()`, and its own source recorded the consequence:
*"a project binary that does not recognise the dump flag, ignores it, boots normally, and streams
logs forever."* FR-053 made the fix a requirement.

`crates/renvor-cli/src/commands/relay.rs` spawns with a piped stdout, drains it on a **concurrent
thread** (a parent that waited for exit before reading would deadlock against the very oversized
answer the bound refuses), bounds the wait with a deadline, and **kills the child** when it elapses.

Written **once**, so both `renvor routes` and `renvor openapi` use it — two implementations of "run
the project binary safely" would be two things that can drift.

Asserted by `a_binary_that_never_answers_is_stopped_at_the_deadline` and
`a_binary_that_streams_forever_is_stopped_rather_than_filling_memory`.

---

## 7a. Defects found in this phase's own work, and fixed

Recorded rather than quietly folded into the feature commits. Each was found by adversarial
self-review **after** the feature suites were green, which is the point worth stating: 1024 passing
tests did not find any of them.

### D-1 — a crafted query string panicked the request path

`crates/renvor-http/src/route/spec.rs`. `percent_decode` read its two hex digits as
`&text[index + 1..index + 3]`. Slicing a `&str` by byte index **panics** when either end is not a
character boundary:

```
byte index 3 is not a char boundary; it is inside 'é' (bytes 2..4) of `%aé`
```

A query string is attacker-controlled, so `?a=%aé` was a caller-triggerable abort on **every**
operation declaring a query parameter. Phase 004's panic boundary contained it as a `500` rather
than taking the process down — which is exactly the containment that makes this class easy to miss.

**Fixed** by decoding over bytes and never slicing the `&str`. Regression tests cover the failing
input directly, plus every byte reachable in the escape branch.

**It also uncovered a second, quieter bug**: the bound was `index + 2 < len`, off by one, so a
trailing `%41` decoded to the literal text `%41` instead of `A`. Silent, and wrong.

### D-2 — `uniqueItems` was an algorithmic-complexity denial of service

`crates/renvor-validation/src/schema.rs`. The check was an O(n²) pairwise scan, with a comment
claiming `maxItems` bounded it. **It did not.** `uniqueItems` is reachable without `maxItems` —
`schemars` emits exactly that for a set-typed field — leaving only the 2 MiB body limit. An array of
~500 000 small integers fits inside it, and 500 000² is 2.5 × 10¹¹ comparisons of a caller's
choosing.

**Fixed** to O(n log n) via a set keyed on each item's rendering, which is exact rather than
approximate because `serde_json::Map` is ordered. A timing regression test fails on the quadratic
shape, with a positive control proving repeats are still detected.

### D-3 — declaration-time schema walking was unbounded

`crates/renvor-validation/src/schema.rs`. `check_subset` recursed with no depth bound. The runtime
walker was already bounded; this one was not.

**Fixed** with the same bound. Writing the test also established a fact worth recording:
`serde_json::Value`'s `Drop` is itself recursive, so a 2000-deep value overflows the stack while
being **freed**, before any Renvor code sees it — and `serde_json::from_str` enforces its own
recursion limit, so a *parsed* schema never reaches that depth. The guard is for a schema built
programmatically, where nothing else would stop it.

### D-4 — CI's licence gate caught what the local one could not see

Not a defect in Phase 005's code, but a defect this phase's dependencies **exposed**, and it belongs
here because it changes what "step 6 passed" is worth.

`borrow-or-share` 0.2.4 entered the lockfile transitively:

```
jsonschema (dev) -> referencing -> fluent-uri -> borrow-or-share   [MIT-0]
```

`MIT-0` was not on the allow-list. **`cargo deny check` passed. GitHub's dependency-review action
failed the pull request.**

The two gates inspect different graphs. Measured: `cargo deny list` reports `schemars` (runtime) and
reports **neither** `jsonschema`, `proptest`, `fluent-uri`, `referencing`, nor `borrow-or-share` —
all dev-only. Setting `exclude-dev = false` changed nothing.

**Resolved**: `MIT-0` is now allowed in **both** gates. It is MIT *without* the attribution
requirement — strictly more permissive than the already-allowed `MIT`, OSI-approved — and it reaches
no published crate, existing only in the build environment. The reasoning is recorded in
`deny.toml` beside the allowance rather than left to be re-litigated.

> **This is a licence-policy addition.** `deny.toml` states that adding to the allow-list *"is a
> policy change and goes through pull request review"*, which is where it now is. It is flagged
> explicitly for the maintainer rather than folded in as a routine fix.

**Not resolved**: the gap itself. The next dev-only licence outside the list will be invisible
locally in exactly the same way. Recorded in
[`deferred-verification-work.md`](deferred-verification-work.md), and
[`contracts/verification-sequence.md`](../contracts/verification-sequence.md) 1.1.2 now states step
6's actual scope rather than leaving the wider reading standing.

**The cause is not established, and is not guessed at.** The observation is confirmed; why
cargo-deny's graph omits that subgraph is not. A fix built on a guessed cause stops working when the
guess turns out wrong.

### D-5 — a bounded request bought an unbounded response *(security review, HIGH)*

`crates/renvor-validation/src/schema.rs`. Every bound in this phase was real except the one on the
**number of issues**. `validate` produced one issue per violating element, uncapped.

Measured by the reviewer against the **real** 2 MiB body limit, schema
`{"type":"array","items":{"type":"string"}}`, body `[1,1,1,…]`:

```
request 2,097,001 B  ->  1,048,500 invalidParams  ->  response 68,090,221 B   (32x)
parse 11 ms · validate 62 ms · serialise 342 ms  =  416 ms CPU · peak RSS 210 MB
```

Two facts compounded it. The concurrency ceiling is **1024**. And the 30-second request timeout
**could not cancel it** — validation and serialisation are synchronous with no `.await`, so there
is no yield point and the work blocks a worker thread outright.

**Fixed** by capping in the **walker**, not at render: capping at render would still allocate a
million pointers first. A truncated list ends with `too_many_issues`, so a caller can tell "these
are all your mistakes" from "these are the first hundred". Regression test asserts the cap, the
short-circuit, the truncation marker, and — as a positive control — that an ordinary three-error
body is still reported in full.

### D-6 — the relay's timeout path hung forever against the shape `cargo run` has *(security review, HIGH)*

`crates/renvor-cli/src/commands/relay.rs`. This is the one that matters most, because **I had just
"fixed" it and made it worse.**

D-4's fix documented that killing `cargo` orphans the grandchild. What that documentation missed is
that the orphan holds an **inherited copy of the stdout write end** — so the pipe never closes, the
reader never returns, and `reader.join()` on the timeout path blocks **for as long as the orphan
runs**. I documented the leak and kept the hang, then wrote that the deadline "reliably bounds this
process's wait". It did not.

The reviewer proved it by replicating `Invocation::run` verbatim under a watchdog:

```
sh -c 'yes renvor'          sh EXECs -> ONE process    -> Err(Timeout) after 640 ms
sh -c 'yes renvor & wait'   sh FORKS -> grandchild     -> still blocked after 20 s
```

**Every timeout test in the suite used the first shape.** `sh -c 'sleep 60'` and
`sh -c 'yes renvor'` both cause `sh` to exec, collapsing to a single process — so the tests passed
without ever reaching the hazard. Both `renvor openapi` and `renvor routes` use `cargo run`, which
forks. The C-L7 "no unbounded wait" property was not met by either.

**Fixed** by detaching the reader instead of joining it. Confirmed independently before fixing: a
`sh -c '… & wait'` child leaves a grandchild that survives `kill -9` of its parent.
`a_forking_child_does_not_hold_the_relay` uses the forking shape the suite structurally could not
reach.

### D-7 — five fail-open paths, each contradicting a claim the code makes *(security review, MEDIUM)*

| # | Fail-open | Fixed |
|---|---|---|
| M-1 | A malformed keyword **value** was silently skipped while still being published. `{"maxLength":"5"}`, `{"required":"name"}`, `{"type":42}`, `{"uniqueItems":"true"}` — nine probes, all accepted and all unenforced. The direct counterexample to the module's own claim that "an unenforceable constraint never ships", which held for keyword *names* and failed for every *value* | ✅ every keyword's value is type-checked at declaration time |
| M-2 | A `$ref` resolving to a **non-schema** accepted everything beneath it. An *unresolvable* ref already failed closed; one resolving to a string failed **open** | ✅ refused at declaration time, and the runtime branch now fails closed too |
| M-3 | An **unknown query parameter was ignored**. `?filtr[status]=admin` returned the *unscoped* collection to a caller who believed it had scoped the request — while that module's own header names "an IGNORED PARAMETER" as one of the silent choices principle IV prohibits | ✅ refused, without echoing the caller's key |
| M-4 | `additionalProperties: false` with **no** `properties` — which permits no members at all — accepted everything. Deny-by-default, inverted | ✅ an absent `properties` is the empty declared set |
| M-5 | Integer bounds above 2^53 were bypassable through `f64`. A genuine **parser differential**: validation saw the rounded value while the handler re-parses the original text | ✅ compared as integers when both sides are exact |

### D-8 — a silent correction inside the identity claim *(security review, MEDIUM)*

`describe.rs` published `required: true` for a path parameter an author declared optional, while
`OperationSpec::validate` kept the author's `false` and skipped the presence check. The description
promised a check the runtime never ran — **inside the one place this phase claims description and
enforcement are an identity.**

**Fixed**: it is now `DescribeError::OptionalPathParameter`. An author can fix a declaration; a
consumer cannot fix a false description.

Two smaller corrections landed with it: the Problem Details status is now converted **before** the
document is built from it (they could otherwise disagree for an out-of-range value), and the
`uniqueItems` rendering-equality assumption is now asserted by a test rather than trusted to
manifests — with `preserve_order` enabled anywhere in the graph it would silently stop detecting
duplicates.

### What this says about the test suite

**All eight passed every existing test.** D-1 to D-3 were found by reading the code adversarially;
D-5 to D-8 by a security review that measured rather than argued.

The sharpest lesson is D-6. It was found **because a previous fix was wrong**: D-4 correctly
identified that the grandchild survives, documented it, and then asserted a bound that the very same
fact destroys. A fix that half-understands a problem can leave the system worse than before, and
its accompanying documentation then makes the remaining half harder to see.

The second lesson is about the tests. D-6 was unreachable by the suite **by construction** —
`sh -c '…'` execs, and the hazard needs a fork. A test that cannot reach a hazard reports a pass it
has not earned, and no amount of running it more often helps.

---

## 8. Limitations — with an owner and a target phase

| # | Limitation | Owner | Target |
|---|---|---|---|
| L-1 | **No independent human review has occurred.** Constitution §Development and Phase Workflow #7 requires one. ADRs 0013–0015 are `proposed` and **no waiver exists** | Ahmed | before merge |
| L-2 | `pattern` is not enforced. It needs a regex engine on untrusted input, and `regex` is not in the runtime graph. A declaration using it is **refused**, not silently unenforced | framework | 012 |
| L-3 | `allOf`/`anyOf`/`oneOf`/`not` are not enforced. `schemars` emits them for data-carrying enums; those declarations are refused | framework | 012 |
| L-4 | Cookie parameters are not validated. A cookie is an authentication carrier and Phase 009 owns authentication | framework | 009 |
| L-5 | Security schemes are absent from the description, for the same reason | framework | 009 |
| L-6 | `format` is published and **not enforced**. This is JSON Schema 2020-12's own default — the format-annotation vocabulary — and is stated rather than left to inference | — | standard behaviour |
| L-7 | Pagination and filtering define **validated public contracts and ports only**. Nothing queries a database; FR-042 forbids it and it is asserted | framework | 006 |
| L-8 | Idempotency keys, conditional requests, and ETags are named in PLAN.md §11.1 for REST 1.0 and require storage | framework | 006+ |
| L-9 | **`renvor openapi` succeeds against no *generated* project**, because no Renvor crate is published and no generated project depends on the framework. The command itself works: §2 records it run end to end against a real project that depends on the framework by path, returning a document the official 3.2 schema accepts. What is zero is its reach across projects the **generator** produces, and it is zero because nothing is published for them to depend on. Carried forward from Phase 004, because it is the same limitation | framework | first publication |
| L-10 | The API snapshot mechanism is implemented and no snapshot is committed, because the framework declares no public API of its own — the gate compares an **application's** description | framework | 012 |
| L-11 | **The relay's deadline bounds this process's wait, not the project's binary.** The direct child is `cargo`, and `cargo run` spawns the binary as a grandchild; killing `cargo` orphans it. Fixing it means a process group, which `std` exposes only through `unsafe` `pre_exec` — and the workspace declares `unsafe_code = "forbid"`. A process-group crate is a dependency decision, not a late edit | framework | 012 |
| L-12 | **A `$ref`-typed parameter is never coerced.** `coerce` reads only the top-level `type`, so a path or query parameter declared `{"$ref": "#/$defs/Quantity"}` stays text and always fails the type check. It **fails closed** — a legitimate request is refused, never wrongly accepted — but it refuses requests it should serve. Security review, LOW | framework | 012 |
| L-13 | **`uniqueItems` holds one rendering per item.** Now O(n log n) rather than O(n²), but it stores each item's rendering in a set: roughly 15–30× the array's bytes in transient memory. Hashing instead of storing would keep the complexity and drop the memory. Bounded by the 2 MiB body limit. Security review, LOW | framework | 012 |
| L-14 | **The security review did not examine everything.** Out of scope for it: `renvor-openapi`'s serialiser and compatibility layer beyond the surface `describe.rs` consumes, the vendored schema artifacts, `route/registry.rs` and the Phase-004 middleware layers, `routes.rs` beyond its relay call sites, `xtask`, and the example. No fuzzing beyond the existing proptest suites, and no concurrent load testing of the D-5 path against a live server — its numbers are single-request measurements extrapolated by the concurrency ceiling | Ahmed | before merge |
| L-15 | **`AGENTS.md` does not exist** in the working tree or anywhere in Git history, though the phase brief named it as required reading. Equivalent authority was taken from `CONSTITUTION.md`, `GOVERNANCE.md`, `PLAN.md`, and `contracts/`. Recorded rather than silently skipped | Ahmed | — |

---

## 9. What this phase deliberately did not do

- **No persistence of any kind.** No repository port implementation, no SQLx, no SeaORM, no
  migrations, no database-backed pagination.
- **No authentication or authorization.**
- **No publication.** Zero crates published, zero tags, zero releases, zero deployments — verified
  against the live registry, not assumed.
- **No waiver created.** W-011 does not exist and was not created.
- **Phase 006 was not started.**

---

## 9a. The commissioned reviews — corrected

**This section previously said the reviews returned nothing. That was false, and it is corrected
here rather than quietly rewritten.**

Three automated reviews were commissioned: a validation review against all 88 FR/SC items, a
requirements-and-governance review, and a security review. At the time the first version of this
section was written, none had reported and each had been given one recovery request. Recording
"they returned nothing" was accurate **at that moment** and wrong **as a conclusion** — two of them
reported afterwards, and one of them found real defects.

The lesson is recorded rather than smoothed over: **"has not reported yet" is not "will not
report", and writing the second when only the first was true put a false statement in a normative
document.** The correct entry would have been "outstanding at the time of writing".

### What the security review found

Two HIGH findings, both **measured** rather than argued, both now fixed. Details in §7a as D-5 and
D-6.

It also confirmed, by direct attack, the claims this record makes about redaction: it sent canaries
as query keys, as duplicate keys, as `filter[<canary>]`, and as `sort=<canary>`, and found **no
path** by which runtime data reaches a response or a log. It confirmed the redaction tests are not
vacuous, that `MAX_DEPTH` accounting is correct on all three recursion sites, and that nothing
concatenates a filter value anywhere.

Six MEDIUM and four LOW findings followed. Five of the MEDIUMs were **fail-open** paths, each
contradicting a claim the code itself makes. Four are fixed; the remainder are recorded in §8.

### What the package research found

A correction to this record's headline claim, now applied — see §2.

### What did not report

The **validation review** and the **requirements review** produced nothing. That is recorded as a
tooling failure and **not** as a pass. No conclusion is drawn from their silence.

The consequence is uneven coverage, and it is worth naming: the security dimension received a
genuine adversarial review and found real defects; the FR/SC conformance and governance-truthfulness
dimensions received **only self-review**. The first found six defects the test suite missed. There
is no reason to believe the other two dimensions are cleaner — only that nobody looked.

## 10. Automated review is not independent review

Any automated review performed during this phase is **advisory**. Constitution §Development and
Phase Workflow #7 requires an independent review comparing implementation evidence with the
specification, constitution, compatibility matrix, and security checklist.

**That review has not occurred, and this record does not claim it has.** The phase remains **open**
until it does, or until a waiver is recorded with an owner, an expiry, and a removal plan — which is
Ahmed's decision and not this record's.
