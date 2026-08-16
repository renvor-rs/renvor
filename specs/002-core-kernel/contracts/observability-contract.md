---
description: "Phase 002 contract — lifecycle spans, structured fields, run identifier, health and readiness"
version: "1.0.0"
status: "unstable — the surface it describes is explicitly unstable under FR-036; this version identifies the contract text, not a stability promise"
---

# Contract: Observability, Health, and Readiness

**Feature**: [../spec.md](../spec.md) | **Satisfies**: FR-026…FR-029, FR-043; SC-008, SC-018, SC-019
**Status**: contract for an **explicitly unstable** surface (FR-036).

## C-O1 — One span per lifecycle phase

The kernel emits **one span per lifecycle phase**, so the observed phase sequence is assertable
**from the telemetry itself** rather than only through a test-only channel.

- **Acceptance**: SC-018 — **7 of 7** lifecycle phases emit a span.

## C-O2 — Structured fields only

All emitted data **MUST** use structured fields. Interpolated message strings are prohibited —
a field that has been formatted into a sentence cannot be filtered, indexed, or redacted.

- **Acceptance**: SC-018 — **0** emitted records use an interpolated message string in place of
  structured fields.

## C-O3 — Run identifier on every record

Every emitted record carries a kernel-generated **run identifier**, identical for the lifetime of
one application run.

- **Acceptance**: SC-018 — **100%** of emitted records carry it.
- Later phases **MAY** nest request-level identifiers beneath it. This phase **MUST NOT** define
  them.

## C-O4 — The run identifier is opaque by construction

| Property | Requirement |
|---|---|
| Generation sites | **exactly 1** |
| Input | cryptographically secure random bytes **only** |
| Forbidden inputs | hostname, timestamp, process identifier, counter, **any** configuration value |
| Production entropy | the **operating-system CSPRNG** |

An identifier that encodes facts becomes an unreviewed disclosure channel — which is why the
constraint is on the **inputs**, not on the output's appearance.

## C-O5 — Opacity is verified deterministically

| Check | Gating? | What it establishes |
|---|---|---|
| Review of the single generation site | **gating** | **0** inputs other than the entropy source |
| Fixed-entropy purity test | **gating** | the identifier is a **pure function of the supplied bytes**: **0** of its bytes change when hostname, clock, process id, counter, and the entire configuration vary while entropy is held fixed |
| Production wiring test | **gating** | **1 of 1** production entropy sources is the OS CSPRNG |
| Random-sample collision / ordering check | **NON-GATING** | a smoke signal only; **0** release gates depend on it |

> **Why the sample check is not a gate.** A collision or monotonicity assertion over a random
> sample is a **probabilistic** statement about a random source, so it can fail on a correct
> implementation. A gate that fails by chance teaches a team to re-run gates, which costs more
> than the check is worth.
>
> **What none of this proves.** These checks establish that nothing but entropy is *encoded*.
> They cannot prove non-recoverability, and **no black-box test can**.

## C-O6 — Redaction applies to every emitted field

Redaction under FR-018 and FR-037 applies to **every** emitted field **without exception** —
including span fields, which are the path most easily forgotten because they are not "logs".

## C-O7 — The library never owns the process-global subscriber

A process has **exactly one** global tracing subscriber, and installing it is a **process-wide,
once-only, effectively irreversible** decision. It belongs to the binary, not to a library the
binary happens to use.

| API | Required behaviour |
|---|---|
| `Application::build()` | installs **nothing**. It **MUST NOT** register, replace, or initialise a global subscriber, dispatcher, or default. The kernel emits through the tracing facade, which is a **no-op** when nothing is installed — so a kernel that emits and an application that never configured tracing both behave correctly |
| **preferred** bootstrap | **returns** a subscriber, layer, or dispatch **value** for the author to install. Renvor supplies the configuration; the author performs the installation |
| optional global helper | **MAY** exist, but **MUST** be explicitly named and explicitly called, **MUST** document its **process-wide** consequence in its own API documentation, and **MUST** have a **specified deterministic already-installed outcome**: it returns an `AlreadyInstalled` error. It **MUST NOT** panic, **MUST NOT** silently succeed, and **MUST NOT** silently replace an existing subscriber |

**"Safe to attempt more than once" (FR-029) means a *specified* result, not an unspecified one
that happens not to crash.** A helper that quietly returns `Ok(())` on the second call is
indistinguishable, to the caller, from one that installed something — and that ambiguity is the
defect.

> Constitution principle I prohibits opaque runtime behaviour and implicit initialisation, and
> FR-029 requires initialisation never to be a side effect of building an application. Installing a
> subscriber during `build()` would violate both, and would silently override — or silently lose
> to — whatever the author configured.

## C-O8 — Health and readiness are independent

Two questions, two answers:

| Question | Answer |
|---|---|
| "Is this process alive?" | **liveness** |
| "Should it receive work?" | **readiness** |

- They **MUST** be independently queryable and **MUST** be able to disagree. Deriving one from the
  other makes SC-008 unsatisfiable by construction.
- Entering `Drain` **MUST** make readiness report not-ready **while liveness continues to report
  alive**.
- A failing readiness contributor **MUST** be individually identifiable.
- A contributor that **panics** is caught, treated as not-ready, and identified.

- **Acceptance**: SC-008 — health and readiness disagree in at least **1** asserted state.

> Conflating the two causes an unready-but-alive process to be killed, or a draining process to
> keep receiving work. Both are outages caused by the primitive rather than by the application.
