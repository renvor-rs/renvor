---
description: "Phase 002 contract — configuration source decoding, precedence, merging, source attribution, and secret redaction"
version: "1.1.0"
status: "unstable — the surface it describes is explicitly unstable under FR-036; this version identifies the contract text, not a stability promise"
---

# Contract: Configuration

**Feature**: [../spec.md](../spec.md) | **Satisfies**: FR-015…FR-018, FR-038, FR-044; SC-003, SC-007, SC-020
**Status**: contract for an **explicitly unstable** surface (FR-036). *Revision 2 — proof gate expanded to 8 obligations.*

## C-C1 — Exactly three source kinds

**Built-in defaults**, **TOML files**, **environment variables**. **0** JSON and **0** YAML
sources — enforced **structurally**, by not enabling those features of the underlying crate,
rather than by policy. A prohibition that cannot be violated by writing code is stronger than one
that can.

## C-C2 — Two steps, in this order

```text
Step 1: decode each source against the declared schema   (per-source, independent)
Step 2: merge the decoded layers by precedence           (cross-layer, attributed)
```

**The order is normative.** Merging first and decoding last — the model used by the mainstream
layered-configuration crates — resolves a shape conflict by picking a winner **silently**, which
constitution principle IV prohibits.

## C-C3 — Step 1: per-source decoding

- Every source **MUST** be decoded against the declared typed schema **before any merging occurs**.
- Decoding a **textual environment value into its declared type** — `8080` into an integer, `true`
  into a boolean — is **permitted** and **MUST NOT** be treated as cross-layer coercion. The target
  type comes from the author's declaration, so nothing is being reconciled between layers.
- An environment value that **cannot** decode **MUST** fail at **Validate**, naming **3 of 3**: the
  **key**, the **source layer**, and the **expected type**.
- **There is no empty-string exemption.** An environment variable set to `""` for a field that
  cannot decode `""` **MUST fail**. It **MUST NOT** be reinterpreted as unset and **MUST NOT** fall
  through to a lower layer.

> **This clause exists because the candidate crate does the opposite.** Its documentation states:
> *"If the env var is set to an empty string and if the field fails to parse/deserialize/validate,
> it is treated as unset."* That is a **silent fallback** — `PORT=""` would quietly become the
> default instead of an error. Whatever implementation is selected must not inherit it.

## C-C4 — Step 2: precedence

Lowest to highest:

```text
built-in defaults  <  TOML file 1  <  TOML file 2  < … <  environment variables
```

A later file overrides an earlier one. Environment always wins. Environment is a **layer with a
precedence position**, not a per-field opt-in annotation.

## C-C5 — Merge rules

| Rule | Behaviour | Why |
|---|---|---|
| **(a) Tables** | merge **per key**; a later layer overrides only the keys it sets | a small override file need not restate a whole section |
| **(b) Arrays** | **replace wholesale**; never concatenated or merged element-wise | element-wise merging makes it impossible to *remove* an entry |
| **(c) Shape conflict** | **fail at Validate**, naming the key and **both** layers | not coerced into a common shape, not resolved by last-wins |

## C-C6 — Source attribution is mandatory and is Renvor's to own

The resolved source layer for **every** key **MUST** remain reportable, so FR-016's "which layer
did this come from" is answerable at runtime, not only at review time.

> **The candidate crate cannot contribute this.** Its layer combinator is documented as *"basically
> like `Option::or`"* — field-by-field selection that **discards which layer won**. Attribution
> must therefore come from Renvor's own merge step, and any implementation that cannot produce it
> fails the proof gate below.

## C-C7 — The proof gate

The **first configuration task MUST demonstrate all eight** obligations. This gate decides whether
the candidate crate is adopted or the recorded fallback triggers.

| # | Obligation |
|---|---|
| 1 | precedence holds: `defaults < earlier TOML < later TOML < environment` |
| 2 | per-key **nested-table** merge — sibling keys from lower layers survive |
| 3 | **wholesale array replacement** — **0** concatenations |
| 4 | **source attribution for every resolved key** |
| 5 | invalid **non-empty** environment decoding **fails**, naming key, layer, expected type |
| 6 | invalid **empty** environment decoding **also fails** rather than falling back |
| 7 | structural conflicts **fail naming both layers** |
| 8 | **0** JSON/YAML features present in the **resolved** dependency graph |

**If any one cannot be demonstrated**, the fallback triggers: a **Renvor partial-layer adapter over
`serde` + `toml`** owning decode-per-source, ordered layering, attribution, and structural-conflict
detection. That adapter is **custom infrastructure under FR-035** and **MUST NOT merge** before
**ADR-0007** is reviewed and accepted through the governance gate — which neither W-002 nor W-003
supplies. See [../research.md](../research.md) D6 and D11.

> Obligations **4 and 6 carry known negative primary-source evidence** for the candidate crate.
> Obligation 6 is recoverable if Renvor reads and decodes the environment itself; obligation 4 is
> not recoverable through the crate at all. The gate still runs — this note exists so its result is
> not presented later as a surprise.

**What counts as evidence, and where it lives.** An obligation is met when, and only when, a
**named executing test** in `crates/renvor-config/tests/proof_gate.rs` asserts it and passes. A
prose claim, a reading of the crate's documentation, or an argument that it "should work" is
**not** evidence for this gate — the whole point of the gate is that two of these obligations look
satisfiable on paper and are not.

| Question | Answer |
|---|---|
| Who decides an obligation is met? | the test outcome, not a reviewer's judgement |
| What is recorded? | the obligation number, the test name, pass or fail, and — on failure — the observed behaviour |
| Where is it recorded? | [../research.md](../research.md) §D6, per obligation |
| Who records it? | the task that runs the gate (T020) |
| What does a partial result mean? | **failure** — the gate is all-eight-or-fallback, with no partial adoption |

**Between failure and adoption of the fallback the configuration surface is unimplemented, not
half-implemented.** No task may begin building on the candidate crate while the gate's result is
outstanding; the branch is selected once, at T020, and every downstream task reads that decision
rather than re-deriving it.

## C-C8 — Errors

A configuration error **MUST** identify **3 of 3**: the key, the violated constraint, and the
source layer. Invalid configuration **MUST** prevent `Boot`; **0** listeners, tasks, or providers
may start.

- **Acceptance**: SC-003 — **0** failing configurations result in a started listener, task, or provider.

## C-C9 — Secret fields: Renvor owns the output contract

Fields declared secret **MUST** be redacted in **every** output form the kernel can produce. The
underlying secret crate covers **one** of them, so the rest are Renvor's boundary type to
implement:

| Output path | Provided by | Required behaviour |
|---|---|---|
| `Debug` | the secret crate | redacted placeholder |
| **`Display`** | **Renvor** | renders a placeholder — **the crate implements no `Display` at all** |
| **error message and error context** | **Renvor** | the value never enters either |
| **structured tracing / log fields** | **Renvor** | the field records a placeholder, not the value |
| **any serialization path** | **Renvor** | serialization is **refused**; the crate's opt-in serialisation marker is deliberately **not** implemented |
| memory zeroization on drop | the secret crate | value wiped |

- The **field name remains visible**; the value **MUST NOT**.
- Redaction is a **property of the type**, not a convention applied per call site — a convention
  regresses the moment a new output path is added.
- **Nesting is not an exemption.** A secret **MUST** stay redacted at **any depth** — inside a
  `Vec`, a map value, a nested configuration table, a tuple, or an enum variant — and in **every**
  row of the table above. Because redaction is a property of the type, this follows by
  construction: the derived output of a container calls its element's own `Debug`/`Display`, which
  is the redacting one. It is stated normatively anyway, and **tested at depth ≥ 2**, because
  "the top-level field was redacted" is the failure mode this clause exists to exclude.
- **Testing obligation**: the suite **MUST** exercise **every** path in the table **and** include a
  **positive-control leaking wrapper** — a type that deliberately does not redact — proving the
  assertions can detect a leak. A redaction suite that only ever sees redacting types cannot
  distinguish "nothing leaked" from "the check never fired".
- **Acceptance**: SC-007 — **0** occurrences of a secret-marked value in any output form.

## C-C10 — Hostile input fails closed

A malformed, truncated, or unexpectedly large TOML file **MUST** produce a **bounded, actionable**
error. It **MUST NOT** start the application, panic the process, or consume unbounded memory or
time while parsing (FR-038).

Per principle IX this parser boundary receives **property or fuzz testing**, not only
example-based tests.

## C-C11 — A source that is present but contributes nothing

A configuration source can exist and supply **0** values: an empty TOML file, a file containing
only comments, a table with no keys, or an environment with none of the declared variables set.
This is **not** an error and **not** a missing source.

| Situation | Required behaviour |
|---|---|
| declared source present, **0** keys | resolution **succeeds**; the layer contributes nothing and wins **0** keys |
| declared source **absent** where the source kind permits absence (e.g. an optional file) | resolution **succeeds**; recorded as *absent*, distinct from *present and empty* |
| declared source **absent** where it was required | resolution **fails**, naming the source — **never** silently treated as empty (FR-022) |
| **every** layer contributes nothing for a key that has a default | the **default** wins and attribution reports the defaults layer |
| **every** layer contributes nothing for a key with **no** default | resolution **fails** at Validate, naming the key — **0** silent substitutions of a zero value, an empty string, or `None` |

- *Present and empty* and *absent* **MUST** be distinguishable in the attribution report. Collapsing
  them loses the answer to "did my file get read at all?", which is the first question an author
  asks when a value does not take effect.
- The distinction is the same one obligation 6 draws for an environment variable set to the empty
  string versus unset (C-C3), applied one level up at the source rather than the key.
