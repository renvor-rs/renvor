# Implementation Plan: Interactive CLI, templates, and local runtime

**Branch**: `feat/phase-003-interactive-cli` · **Date**: 2026-08-17 · **Spec**: [`spec.md`](spec.md)

**Input**: Feature specification with 48 functional requirements, 16 success criteria, and 5
recorded clarifications, all resolved.

## Summary

Add one workspace crate, `crates/renvor-cli`, producing an executable named **`renvor`**. Its
principal command builds a new project **transactionally**: every choice is validated before any
byte is written, rendering happens in a staging directory inside the destination's parent, the
result is verified, and only then is it moved into place by a single same-filesystem rename. Nothing
partial is ever visible at the destination, on any path — cancellation, validation failure, render
failure, or crash.

Around that core sit the command surface (`new`, `doctor`, `check`, `dev`, `docker`), the dual
prompt/flag interface that resolves to **one** validated configuration value, a versioned embedded
template set, a file manifest that serves dry-run, pre-move verification, and reproducibility alike,
and a machine-readable output contract.

**Three deliverables from `PLAN.md` §20 are deliberately narrower here than the plan's wording, each
settled in the clarification session and each recorded so the phase is not later read as having
delivered them**: no certificate is issued and no trust store is touched; no archive is read; and the
wizard asks fewer than `PLAN.md` §9.1's fifteen prompts.

## Technical Context

| | |
|---|---|
| **Language** | Rust, 2024 edition, resolver 3 |
| **Toolchains gated** | MSRV **1.94.0** and current stable — both, on every supported platform |
| **Safety** | `unsafe_code = "forbid"` workspace-wide; no relaxation in this phase |
| **New crate** | `crates/renvor-cli` (package `renvor-cli`, binary **`renvor`**) |
| **Depends on** | `renvor-core`, `renvor-config` from Phase 002 |
| **Primary dependencies** | clap 4.6.6 · inquire 0.9.4 · indicatif 0.18.6 · anstream 1.0.0 · MiniJinja 2.24.0 · tempfile 3.27.0 · walkdir 2.5.0 · serde_json 1.0.151 · sha2 0.11.0 — each evaluated in [`research.md`](research.md) |
| **Test tooling** | trycmd 1.2.1 · snapbox 1.2.2 · insta 1.48.0 |
| **Storage** | The filesystem, and nothing else. No database, no network, no daemon |
| **Target platforms** | Linux, macOS, Windows — **claimed only where CI exercises them** |
| **Performance goal** | None stated as a target. Bounds, not speeds, are the requirement (FR-042) |

### Resolved by clarification, not left open

| Decision | Effect on this plan |
|---|---|
| Staging inside the destination's **parent** | Removes the cross-filesystem case entirely instead of handling it. FR-016's forbidden copy-fallback becomes unreachable rather than merely prohibited |
| Exit codes `0`/`1`/`2`/`3`/`4`/`5`, `1` reserved | An unclassified failure is visible as a defect rather than absorbed into a general error |
| One JSON document, integer `schemaVersion` | Consumers detect shape change without parsing the payload |
| Templates embedded; **no archive** | Removes zip-slip and decompression-amplification surfaces rather than guarding them |
| Wizard asks only what the phase honours; later-phase flags **reserved** | A command line written today keeps its meaning in Phase 006 instead of becoming an unknown-flag error |
| Consent boundary only for local HTTPS | The irreversible capability is not built before it has a consumer |

## Constitution Check

Evaluated before design, and re-evaluated after it. Both passes recorded.

| Principle | Gate | Status |
|---|---|---|
| **I — Cohesive, explicit Rust** | No hidden global state; no implicit network work; `unsafe` absent | **Pass.** The CLI performs no network I/O at all (FR-043) and holds no global mutable state |
| **II — Transport-independent core** | No transport types in the application core | **Pass by construction.** No transport exists yet |
| **III — Package-first boundaries** | Maintained packages evaluated before custom infrastructure; custom choices need an accepted ADR | **Pass with one BLOCKING GATE.** [`research.md`](research.md) D6 selects hand-composed path containment over `cap-std` and **requires an accepted decision record before that component merges** |
| **IV — Deterministic lifecycle and failure** | No silent fallback; failures stop the operation with actionable diagnostics; retries bounded | **Pass.** FR-016 forbids the copy fallback; exit code `1` keeps unclassified failures visible; D2 selects a prompt library on the strength of its *typed* cancellation rather than an inferred one |
| **V — Contract-first compatibility** | CLI commands define flags, exit codes, stdout/stderr, JSON, cancellation | **Pass.** FR-002 makes them contracts and [`contracts/`](contracts/) defines them before implementation |
| **VI — Security and fail-closed defaults** | Input validated; bounded work; secrets never in output; security failures fail closed | **Pass with the D6 gate.** FR-039/FR-041/FR-042 carry it; the path boundary is checked rather than structural, which is the weakening D6 records |
| **VII — Deterministic and safe generation** | Wizard and flags resolve to the same validated configuration; no partial destination; deterministic output | **Pass, with a stated narrowing.** VII names prompts this phase does not ask, because the subsystems behind them do not exist. FR-005a/FR-005b make that explicit rather than silent |
| **IX — Testing** | Property or fuzz coverage at hostile boundaries | **Pass.** The path-safety corpus and template-bound tests are that coverage, each with a positive control |
| **XII — Honest limitations** | Limits stated, not glossed | **Pass.** Three narrowings, one blocking ADR, and the Windows rename limit are all in the record |

### Post-design re-evaluation

Re-run after `data-model.md` and `contracts/` existed. **No new violation, and one thing got
better**: designing the manifest as a single artifact serving dry-run, pre-move verification, and
reproducibility removed a duplicate representation the first sketch had, which principle I would
have flagged as hidden state with two sources of truth.

**The D6 gate did not soften on re-evaluation, and is restated deliberately**: it is the only item
here that can block a merge, and it is the direct analogue of ADR-0007 in Phase 002.

## Project Structure

### Documentation (this feature)

```text
specs/003-interactive-cli/
├── spec.md              48 FRs, 16 SCs, 5 clarifications
├── plan.md              this file
├── research.md          D1-D14 package-first evaluation, one blocking gate
├── data-model.md        the five entities and their invariants
├── contracts/
│   ├── command-surface.md      commands, flags, exit codes, stdout/stderr
│   ├── json-output.md          schemaVersion, status, error-code registry
│   ├── project-manifest.md     renvor.toml schema and what it must never hold
│   ├── template-contract.md    template versioning, bounds, and forbidden capabilities
│   └── generation-transaction.md   the staging/verify/rename protocol and its failure modes
├── quickstart.md        runnable validation of every acceptance criterion
└── checklists/
    └── requirements.md  16/16
```

### Source code

```text
crates/renvor-cli/
├── Cargo.toml           package renvor-cli, [[bin]] name = "renvor"
├── src/
│   ├── main.rs          argument parsing, exit-code mapping, output routing
│   ├── config/          the ONE validated configuration model
│   │   ├── model.rs       resolved, validated selections
│   │   ├── flags.rs       flag surface, including RESERVED later-phase flags
│   │   └── prompts.rs     wizard, entered only when stdin is a terminal
│   ├── generate/        the transaction
│   │   ├── staging.rs     staging directory inside the destination's parent
│   │   ├── manifest.rs    file manifest: dry-run, verification, reproducibility
│   │   ├── render.rs      bounded template expansion, strict undefined
│   │   └── place.rs       verify, then one rename
│   ├── paths.rs         destination validation — GATED ON THE D6 ADR
│   ├── output/          human and JSON, one result from one source
│   └── commands/        new, doctor, check, dev, docker
├── templates/           embedded, versioned; api-only skeleton
└── tests/
    ├── cli/             trycmd contract files
    ├── transaction.rs   cancellation and injected-failure coverage
    ├── hostile.rs       path-safety corpus with positive controls
    └── parity.rs        prompt-vs-flag byte-identity
```

**Structure decision**: one new crate, not several. `PLAN.md` §7.3 lists `renvor-cli` as a single
crate and §7.3 also forbids splitting for naming alone. Nothing here has an independent public
contract that would justify a second crate: the generator, the wizard, and the command surface are
one product boundary, and the template set is data rather than an API.

## Complexity Tracking

| Item | Why it is not simpler | What was rejected |
|---|---|---|
| **Two independent non-TTY checks** (`IsTerminal` before entering the wizard, `NotTTY` from the prompt library) | FR-010 forbids **both** hanging and silently defaulting. One check that regressed would produce exactly one of those, and the two failure modes are indistinguishable from outside | A single check. Rejected because the requirement has two failure modes and one check cannot fail closed on both |
| **A manifest that serves three purposes** | Dry-run, pre-move verification, and reproducibility are the same question asked at three moments. Separate representations would drift, and SC-006 requires dry-run to match reality exactly | Three structures. Rejected on the drift argument, and the post-design constitution pass agreed |
| **Reserved flags that parse and then fail** | FR-005b. An unknown-flag error tells a user their command is wrong; a reserved-flag error tells them *when* it will be right | Omitting the flags. Rejected: it makes a Phase 003 command line silently change meaning in Phase 006 |
| **Hand-composed path containment** | See D6 | `cap-std`. **Not rejected — deferred to an ADR**, because the choice weakens a structural guarantee to a checked one and that is not a decision this plan may take alone |

## Gates carried out of this plan

1. **ADR required for D6** before the path-containment component merges. Blocking.
2. **Dependency inventory from the real `Cargo.lock`**, not from `research.md` (FR-044, SC-015).
3. **Platform claims follow CI**, and the Windows rename limit is documented rather than equated to
   POSIX.
4. **The independent-review gate remains open.** Advisory reviews are not independent, and this phase
   does not assume a waiver is available.
