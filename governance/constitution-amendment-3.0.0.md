# Constitution amendment 2.0.0 → 3.0.0 — Principle VII and staged delivery

| Field | Value |
|---|---|
| **From** | 2.0.0 (2026-08-17) |
| **To** | **3.0.0** (2026-08-18) |
| **Class** | **MAJOR** — redefines a governing principle |
| **Authority** | Maintainer ruling of 2026-08-18, item 3 |
| **Principle touched** | **VII. Deterministic and Safe Generation**, first paragraph, third sentence |
| **Waiver** | **None.** No waiver was created, requested, or relied on. W-007 does not exist |
| **Canonical text** | [`../CONSTITUTION.md`](../CONSTITUTION.md) |

This document is the written proposal, impact analysis, and migration plan the constitution's
Governance section requires. The amendment history entry in `CONSTITUTION.md` is the summary; this
is the record behind it.

---

## 1. Written proposal

### 1.1 The conflict

Principle VII contained two requirements that could not both be satisfied by a project delivered in
phases.

**The first**, in its opening paragraph:

> *"The wizard MUST ask for target, transport, persistence model, database, auth starter, frontend,
> compatible render mode, styling profile where applicable, desktop option, capabilities, and local
> tooling."*

**The second**, in its third paragraph and reinforced throughout the constitution:

> *"Generation MUST validate the entire selection before writing … Both interfaces MUST resolve to
> the same validated configuration and project manifest."*

Phase 003 ships one target (`api`) and no transport, no persistence, no authentication, no
frontend, and no desktop. Asking an operator to choose a database in that program produces an answer
that **no generated file reflects**. Recording it in `renvor.toml` would make the manifest describe a
project that was never generated. Not recording it would make the wizard ask a question with no
consequence — which is worse than not asking, because the operator reasonably believes they have
chosen something.

Phase 003's `renvor.toml` header states the resolution the implementation had already reached:

```toml
# A choice appears here only if a generated file reflects it. A manifest recording an unhonoured
# choice would describe a project that was never generated.
```

So the phase was in violation of a MUST, and the violation was **structural rather than accidental**:
no amount of implementation work in Phase 003 could satisfy the old sentence without breaking the
other one.

### 1.2 What was considered and rejected

| Option | Why not |
|---|---|
| **Keep Phase 003 open until compliant** | Compliance requires shipping Phases 004, 009, 013, 019, and 024. "Open until then" is not a phase gate; it is a statement that Phase 003 cannot close, which makes the phase structure itself unusable |
| **A time-bounded waiver naming the violated clause** | A waiver is an exception to a rule that stays correct. This rule was **not correct** — it mandated questions for capabilities that do not exist. The waiver would have expired with the conflict intact, and the same decision would have been needed again with less context. The maintainer ruling explicitly forbade creating W-007 |
| **Delete the eleven-item list** | This is the weakening the ruling forbade. The list is the guarantee that a shipped capability gets a wizard question; deleting it would let a future phase ship a database and never ask about it |
| **Amend the sentence to bind on shipped capabilities (chosen)** | Preserves every obligation, moves the moment each one binds to the moment it can be honoured, and forbids the two failure modes the old rule was protecting against — asking nothing, and asking about nothing |

### 1.3 The amended text

The sentence

> The wizard MUST ask for target, transport, persistence model, database, auth starter, frontend,
> compatible render mode, styling profile where applicable, desktop option, capabilities, and local
> tooling.

is replaced by

> The wizard MUST ask for every meaningful choice the current generator can honour. A choice with
> only one supported value MAY be defaulted without prompting and MUST be recorded. The wizard MUST
> NOT solicit or record unsupported choices. Unsupported choices MUST be exposed as reserved inputs
> that fail explicitly with the phase that will introduce support. Once a capability ships, its
> choice becomes mandatory in both the wizard and non-interactive interface. The governed choice set
> is target, transport, persistence model, database, auth starter, frontend, compatible render mode,
> styling profile where applicable, desktop option, capabilities, and local tooling; each becomes
> mandatory in both interfaces on the day its capability ships, and none of them may be dropped from
> this set by an implementation that has not shipped it.

**Every other sentence of Principle VII is unchanged**, verbatim. Nothing about validation before
writing, owned staging, verification, atomic commit, unchanged destination on failure, non-overwrite
of existing files, accurate `--dry-run`, the first-party styling profiles, generated-code quality, or
the package lifecycle was touched.

### 1.4 Why MAJOR

The Governance section defines the classes:

- **MAJOR:** removes or redefines a governing principle or compatibility promise.
- **MINOR:** adds a principle or materially expands mandatory governance.
- **PATCH:** clarifies wording without changing required behavior.

The old sentence stated eleven **unconditional** obligations. The new one states the same eleven
obligations **conditionally**, and adds four clauses governing the conditional state. Required
behaviour therefore changes — a conforming implementation that previously had to ask about databases
now must not — which excludes PATCH. Nothing was added as a new principle; an existing obligation was
redefined, which excludes MINOR. **MAJOR.**

---

## 2. Impact analysis

### 2.1 Public APIs

**No change.** The amendment governs which questions the wizard asks. Phase 003's command surface,
its flags, its exit codes, and its JSON contract are unaffected by it.

The reserved-input requirement — *"Unsupported choices MUST be exposed as reserved inputs that fail
explicitly with the phase that will introduce support"* — was **already implemented and already
tested** before the amendment. `crates/renvor-cli/src/config/flags.rs` carries the table:

| Reserved flag | Phase named in the refusal |
|---|---|
| `--transport` | Phase 004 (the first real transport) |
| `--orm` | Phase 009 (persistence) |
| `--database` | Phase 009 (persistence) |
| `--auth` | Phase 013 (authentication) |
| `--frontend` | Phase 019 (full-stack architecture) |
| `--styling` | Phase 019 (full-stack architecture) |
| `--render-mode` | Phase 019 (full-stack architecture) |
| `--desktop` | Phase 024 (desktop) |

Each exits `3` with `reserved_for_later_phase` and `details.phase`. The amendment makes an existing
behaviour normative rather than requiring a new one.

### 2.2 Generated projects

**No change to any generated file.** `renvor.toml` already records exactly the choices generation
acted on, including the defaulted single-valued `target = "api"`, and records no unsupported choice.
A verbatim manifest from a full Phase 003 run:

```toml
[project]
name = "demo"
target = "api"
local_domain = "demo.test"
container = true
local_https = "requested"
example_domain = true
seed_data = true
```

`target` is the *"choice with only one supported value"* the amended clause permits to be defaulted,
and it is recorded, as the same clause requires.

### 2.3 Security

**No new attack surface, and one hazard removed.** The amendment *forbids* soliciting and recording
unsupported choices. A wizard that collected a database name, a connection string, or an auth
provider that nothing would act on would be writing operator-supplied values — potentially including
credentials — into a manifest for no purpose. Principle VII's own `renvor.toml` secrecy requirement
and FR-018 already prohibit credentials in the manifest; this closes the route by which they would
have arrived.

Nothing in Phase 003's threat model, path boundary, redaction, or trust-store consent gate is touched.

### 2.4 Compatibility

| Surface | Effect |
|---|---|
| Constitution version | `2.0.0` → `3.0.0`; every document citing "v2.0.0 principle VII" is updated in this change |
| CLI JSON contract | Unaffected. The `schemaVersion 1 → 2` bump in this same PR comes from the **destination policy and error registry**, ruling items 4 and 6, and is documented separately in `contracts/json-output.md` |
| Existing generated projects | Unaffected. No manifest field changes meaning |
| Future phases | **Tightened.** A phase that ships a capability without a wizard question now violates a MUST that names it |

### 2.5 Documentation

| Document | Change |
|---|---|
| `CONSTITUTION.md` | Principle VII amended; version, date, and amendment history updated |
| Local specification-tooling working copy | Synchronised and byte-verified against the canonical text |
| [`spec.md`](https://github.com/renvor-rs/renvor/blob/01327b1ee61b73ebbd4f9198c04d651b38367ba8/specs/003-interactive-cli/spec.md) | Two "v2.0.0" citations updated to v3.0.0 and the compliance statement rewritten |
| `governance/phase-003-evidence.md` | §2 rewritten: the referral becomes a recorded ruling with an outcome |
| `docs/docs/governance.mdx` | Constitution version reference updated where present |

### 2.6 Active phases

| Phase | State | Effect |
|---|---|---|
| **001 — governance foundation** | closed | None. Principle VII does not govern it |
| **002 — core kernel** | closed | None. No wizard, no generator |
| **003 — interactive CLI** | open, this PR | **Brought into compliance.** See §4 |
| **004 and later** | not started | Each inherits an obligation that binds when it ships its capability. Phase 004 ships the first transport, so `--transport` stops being a reserved input and becomes a mandatory wizard question in that phase |

---

## 3. Migration plan

Nothing to migrate in the product: no generated file, no manifest field, and no command changes as a
consequence of this amendment. The migration is documentary and forward-looking.

**Immediately, in this PR:**

1. `CONSTITUTION.md` amended, version `3.0.0`, date `2026-08-18`, amendment history entry added.
2. The local specification-tooling working copy synchronised and verified identical from the `# Renvor
   Constitution` heading onward.
3. Every tracked document citing constitution v2.0.0 or quoting the old sentence updated.
4. A test added — `crates/renvor-cli/src/config/flags.rs`,
   `every_governed_choice_of_principle_seven_is_classified` — that fails if a reserved flag stops
   naming its phase or a new reserved flag is added without one. Compliance is checked, not asserted.

   *Corrected 2026-08-18.* This line originally named a test
   `the_amended_principle_seven_is_satisfied_by_this_phase`, **which does not exist**: the citation
   was written before the test was, and the test was then given a different name. An advisory
   requirements review found it by grepping for the name and getting no hits (finding R-1). The
   guarantee described was real all along; the pointer to it was not, and a reader checking this
   claim would have concluded the enforcement was missing.

**At the start of each later phase, as a phase-entry obligation:**

5. When a phase ships a capability from the governed choice set, its flag moves out of the reserved
   table and a wizard question is added in the same change. The reserved-flag test above fails if the
   flag is removed from the table without being handled, which is what makes step 5 enforceable rather
   than remembered.

**Not required:** no re-review of Phases 001 and 002, no regeneration of existing projects, no
`schemaVersion` change attributable to this amendment.

---

## 4. Maintainer ruling, recorded

The maintainer ruled on 2026-08-18, in writing, in the instruction that authorises this change:

> *"Amend Constitution Principle VII to be compatible with staged delivery. The present rule requires
> questions for capabilities that do not exist and conflicts with the requirement not to solicit or
> record choices the generator cannot honour."*

with the governing intent quoted verbatim in §1.3, and:

> *"Treat this as a MAJOR constitutional amendment from 2.0.0 to 3.0.0."*
>
> *"Do not create W-007."*
>
> *"Do not weaken any other Principle VII requirement."*

**Approval status:** approved by the maintainer as the instruction that directed the amendment. This
is the constitution's Governance item 4, "maintainer approval", and it is recorded here rather than
inferred.

**What was NOT authorised, and was not done:** no waiver was created; no other principle was touched;
no sentence of Principle VII other than the one in §1.3 was altered; no phase-level exception was
created for Phase 003.

---

## 5. Compliance of Phase 003 under the amended principle

Stated as a verdict per clause, against the shipped implementation rather than against intent.

| Amended clause | Phase 003 | Evidence |
|---|---|---|
| The wizard MUST ask for every meaningful choice the current generator can honour | **COMPLIES** | Seven prompts — project name, local development domain, example domain module, seed data, container controls, local HTTPS record, confirmation — and the generator honours exactly those. `tests/transaction.rs::the_wizard_asks_exactly_these_prompts` asserts the set behaviourally and fails if a prompt is added, removed, renamed, or reordered |
| A choice with only one supported value MAY be defaulted without prompting and MUST be recorded | **COMPLIES** | `target` supports only `api`. It is defaulted by `#[arg(default_value = "api")]` and recorded as `target = "api"` in `renvor.toml` |
| The wizard MUST NOT solicit or record unsupported choices | **COMPLIES** | No prompt asks about transport, persistence, database, auth, frontend, render mode, styling, or desktop, and no such key appears in `renvor.toml`. `config/model.rs::every_configuration_field_is_inert_and_a_new_one_cannot_be_added_unclassified` fails to compile if a field is added without classification |
| Unsupported choices MUST be exposed as reserved inputs that fail explicitly with the phase that will introduce support | **COMPLIES** | The eight-row table in §2.1, each exiting `3` with `reserved_for_later_phase` and `details.phase`. Asserted by `flags.rs`'s reserved-flag tests and by `tests/cmd/exit-codes.trycmd` as expected output |
| Once a capability ships, its choice becomes mandatory in both the wizard and non-interactive interface | **NOT YET APPLICABLE** | No capability from the governed set ships in Phase 003 beyond `target`, which is single-valued and therefore covered by clause 2. This clause binds Phase 004 onward and is recorded as a phase-entry obligation in §3 step 5 |

**Verdict: Phase 003 complies with Principle VII as amended.** The T093a referral in
`governance/phase-003-evidence.md` §7 is closed by this amendment rather than by a waiver.
